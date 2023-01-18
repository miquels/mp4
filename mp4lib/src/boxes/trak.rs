use std::io;

use crate::boxes::prelude::*;
use crate::boxes::{AacSampleEntry, TrackHeaderBox, MediaBox, EditBox, EditListBox, EditListEntry};
use crate::sample_info::sample_info_iter;

#[doc(inline)]
pub use crate::sample_info::{SampleInfo, SampleInfoIterator};

def_box! {
    /// 8.3.1 Track Box (ISO/IEC 14496-12:2015(E))
    TrackBox {
        movie_timescale:    u32,
        boxes:              Vec<MP4Box>,
    },
    fourcc =>"trak",
    version => [],
    impls => [ basebox, boxinfo, debug ],
}

impl TrackBox {

    /// Get a reference to this track's TrackHeaderBox.
    pub fn track_header(&self) -> &TrackHeaderBox {
        first_box!(&self.boxes, TrackHeaderBox).unwrap()
    }

    /// Get a mutable reference to this track's TrackHeaderBox.
    pub fn track_header_mut(&mut self) -> &mut TrackHeaderBox {
        first_box_mut!(&mut self.boxes, TrackHeaderBox).unwrap()
    }

    /// Get a reference to this track's MediaBox.
    pub fn media(&self) -> &MediaBox {
        first_box!(&self.boxes, MediaBox).unwrap()
    }

    /// Get a mutable reference to this track's MediaBox.
    pub fn media_mut(&mut self) -> &mut MediaBox {
        first_box_mut!(&mut self.boxes, MediaBox).unwrap()
    }

    /// Get the track id.
    pub fn track_id(&self) -> u32 {
        self.track_header().track_id
    }

    /// Set the track id.
    ///
    /// You should call this instead of setting the field directly,
    /// since there might be boxes deeper down that contain
    /// the track id, such as 'mp4a'.
    pub fn set_track_id(&mut self, track_id: u32) {
        self.track_header_mut().track_id = track_id;
        let stsd = self
            .media_mut()
            .media_info_mut()
            .sample_table_mut()
            .sample_description_mut();
        if let Some(mp4a) = first_box_mut!(stsd.entries, AacSampleEntry) {
            mp4a.set_track_id(track_id);
        }
    }

    /// Get the edit list, if it is present and has at least one entry.
    pub fn edit_list(&self) -> Option<&EditListBox> {
        if let Some(edts) = first_box!(&self.boxes, EditBox) {
            if let Some(elst) = edts.boxes.iter().next() {
                if elst.entries.len() > 0 {
                    return Some(elst);
                }
            }
        }
        None
    }

    fn edit_list_mut(&mut self) -> Option<&mut EditListBox> {
        if let Some(edts) = first_box_mut!(&mut self.boxes, EditBox) {
            if let Some(elst) = edts.boxes.iter_mut().next() {
                if elst.entries.len() > 0 {
                    return Some(elst);
                }
            }
        }
        None
    }

    /// Check the editlist to see if there's :
    ///
    /// 1. an initial empty edit (see 8.6.6.1)
    /// 2. an initial composition time shift (see 8.6.1.3.1).
    ///
    /// We can handle "1", "1,2" and "2". If we encounter any other
    /// variations, we give up and return None.
    ///
    /// Note, 1. results in a positive return value, 2. in a negative one.
    ///
    /// Return value is expressed in movie timescale units.
    pub fn composition_time_shift(&self, verbose: bool) -> io::Result<i64> {
        let mut empty = 0u64;
        let mut shift = 0u64;
        let mut valid = true;

        let track_id = self.track_id();
        let elst = match self.edit_list() {
            Some(elst) => elst,
            None => return Ok(0),
        };

        for (idx, entry) in elst.entries.iter().enumerate() {
            if entry.media_rate != 1 {
                if verbose {
                    log::error!("TrackBox(id {}): edit list entry #{}: media_rate {}",
                        track_id,
                        idx,
                        entry.media_rate
                    );
                }
                valid = false;
            }

            if entry.media_time < 0 {
                let media_timescale = self.media().media_header().timescale as u64;
                empty = match entry.segment_duration.checked_mul(media_timescale) {
                    Some(res) => res,
                    None => {
                        if verbose {
                            log::error!("TrackBox(id {}): edit list entry #{}: too big",
                                track_id,
                                idx,
                            );
                        }
                        valid = false;
                        continue;
                    },
                };
                empty /= self.movie_timescale as u64;
                if empty > u64::MAX / 4 {
                    if verbose {
                        log::error!("TrackBox(id {}): edit list entry #{}: \
                                    idiotic segment_duration {}",
                            track_id,
                            idx,
                            entry.segment_duration,
                        );
                    }
                    valid = false;
                }
                continue;
            }

            // Check that this edit convers the entire track.
            let seg_d = self.duration_to_secs(entry.segment_duration, false);
            let track_d = self.duration_to_secs(self.track_header().duration.0, false);
            if seg_d / track_d < 0.98 {
                if verbose {
                    log::error!("TrackBox(id {}): edit list entry #{}: \
                                segment_duration != track duration ({:.0}/{:.0})",
                        track_id,
                        idx,
                        seg_d,
                        track_d,
                    );
                }
                valid = false;
            }
            shift = entry.media_time as u64;

            if verbose {
                // Do a check to see if the segment duration is the same as the
                // offset of the first CTTS entry.
                let stbl = self.media().media_info().sample_table();
                if let Some(ctts) = stbl.composition_time_to_sample() {
                    if ctts.entries.len() > 0 {
                        let offset = ctts.entries[0].offset;
                        if offset as i64 != entry.media_time {
                            log::warn!("TrackBox(id {}): edit list entry #{}: \
                                        ,media_time != ctts[0].offset ({} - {})",
                                track_id,
                                idx,
                                entry.media_time,
                                offset,
                            );
                        }
                    }
                }

                if elst.entries.len() > idx + 1 {
                    log::warn!("TrackBox(id {}): edit list entry #{}: \
                                spurious edit at end of track (ignored)",
                        track_id,
                        idx + 1
                    );
                }
            }

            break;
        }

        if !valid {
            return Err(ioerr!(InvalidData, "track #{}: cannot handle EditList"));
        }

        Ok(empty as i64 - (shift as i64))
    }

    /// If this track has a simple edit list - one that can be parsed
    /// by `composition_time_shift()`, then update the entries in the
    /// CompositionOffsetBox (CTTS) with that offset.
    ///
    /// Now there is the possibility that the CTTS entry of the first
    /// sample is not the same as the composition time shift from the
    /// edit list. In that case, make it so that the first sample has
    /// a CTTS entry with offset 0, and add a single-entry editlist
    /// with the offset.
    ///
    /// This will only succeed if `composition_time_shift()` doesn't error.
    /// If there is an error, the track remains unchanged.
    pub fn simplify_offsets(&mut self) -> io::Result<()> {

        // get delta from the editlist;
        let shift = self.composition_time_shift(true)?;

        // get a handle to the CTTS table.
        let stbl = self.media_mut().media_info_mut().sample_table_mut();
        let ctts = match stbl.composition_time_to_sample_mut() {
            Some(ctts) => ctts,
            None => return Ok(()),
        };

        // get delta from the first sample.
        let ctts0 = ctts.entries[0].offset;

        // no change?
        if shift == 0 && ctts0 == 0 {
            return Ok(());
        }

        // Adjust the offsets.
        if ctts0 != 0 {
            for entry in ctts.entries.iter_mut() {
                entry.offset -= ctts0 as i32;
            }
        }

        let media_timescale = self.media().media_header().timescale as u64;
        let movie_timescale = self.movie_timescale;
        let track_duration = self.track_header().duration.0;

        // There should be just one edit list entry.
        let elst = match self.edit_list_mut() {
            Some(elst) => {
                elst.entries.vec.truncate(1);
                elst
            },
            None => {
                let mut entries = ArraySized32::new();
                entries.push(EditListEntry::default());
                self.boxes.push(MP4Box::EditBox(EditBox {
                    boxes: vec![ EditListBox { entries } ],
                }));
                self.edit_list_mut().unwrap()
            },
        };
        let entry = &mut elst.entries[0];

        // Now update that entry.
        let d = shift + ctts0 as i64;
        if d > 0 {
            // empty edit.
            entry.media_rate = 1;
            entry.media_time = -1;
            entry.segment_duration = d as u64 * movie_timescale as u64 / media_timescale;
        } else {
            // edit to skip some samples at the start.
            entry.media_rate = 1;
            entry.media_time = -d;
            entry.segment_duration = track_duration;
        };

        Ok(())
    }

    fn duration_to_secs(&self, duration: impl Into<u64>, media: bool) -> f64 {
        let duration = duration.into() as f64;
        let timescale = if media {
            self.media().media_header().timescale
        } else {
            self.movie_timescale
        };
        duration / std::cmp::max(1, timescale) as f64
    }

    /// Check if this track is valid (has header and media boxes).
    pub fn is_valid(&self) -> bool {
        let mut valid = true;
        let track_id = match first_box!(&self.boxes, TrackHeaderBox) {
            Some(th) => th.track_id,
            None => {
                log::error!("TrackBox: no TrackHeaderBox present");
                return false;
            },
        };

        // The must be at least one MediaBox present.
        match first_box!(&self.boxes, MediaBox) {
            Some(m) => {
                if !m.is_valid() {
                    valid = false;
                }
            },
            None => {
                log::error!("TrackBox(id {}): no MediaBox present", track_id);
                valid = false;
            },
        }

        valid
    }

    /// Return an iterator over the SampleTableBox of this track.
    ///
    /// It iterates over multiple tables within the SampleTableBox, and
    /// for each sample returns a SampleInfo.
    pub fn sample_info_iter(&self) -> SampleInfoIterator<'_> {
        sample_info_iter(self)
    }
}

impl FromBytes for TrackBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<TrackBox> {
        let mut reader = BoxReader::new(stream)?;
        let boxes = Vec::<MP4Box>::from_bytes(&mut reader)?;
        Ok(TrackBox {
            movie_timescale: 0,
            boxes,
        })
    }
    fn min_size() -> usize { 8 }
}

impl ToBytes for TrackBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self)?;
        self.boxes.to_bytes(&mut writer)?;
        writer.finalize()
    }
}
