use std::io;

use crate::boxes::prelude::*;
use crate::boxes::{AacSampleEntry, TrackHeaderBox, MediaBox, EditBox, EditListBox};
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

    /// Check the editlist to see if there's an initial composition time shift (see 8.6.1.3.1).
    ///
    /// If there are multiple edits, this will only return the offset from the
    /// first edit. TODO: maybe we should return an error in that case.
    ///
    /// If there's an initial empty edit, you should probably also take that into
    /// account. This method can't.
    ///
    /// Return value is expressed in movie timescale units.
    pub fn composition_time_shift(&self) -> Option<u32> {
        if let Some(elst) = self.edit_list() {
           for entry in &elst.entries {
                if entry.media_time > 0 {
                    return Some(entry.media_time as u32);
                }
            }
        }
        None
    }

    /// Change editlist entry + version 0 CTTS into no editlist entry +
    /// version 1 CTTS (with begative offsets).
    pub fn convert_to_negative_composition_offset(&mut self) {
        let id = self.track_id();
        let elst = match self.edit_list() {
            Some(elst) => elst,
            None => return,
        };

        // We _could_ probably handle this. Should we bother? Or panic?
        if elst.entries.iter().filter(|e| e.media_time > 0).count() > 1 {
            log::warn!(
                "track #{}: more than one edit list with media_time > 0",
                id,
            );
        }

        // See if there's a edit list for this.
        let mut offset = 0;
        let mut idx = 0;
        while idx < elst.entries.len() {
            if elst.entries[idx].media_time <= 0 {
                idx += 1;
                continue;
            }
            offset = elst.entries[idx].media_time;
            break;
        }
        if offset == 0 {
            return;
        }

        // get a handle to the CTTS table.
        let stbl = self.media_mut().media_info_mut().sample_table_mut();
        let ctts = match stbl.composition_time_to_sample_mut() {
            Some(ctts) => ctts,
            None => return,
        };

        // Adjust the offsets.
        for entry in ctts.entries.iter_mut() {
            entry.offset -= offset as i32;
        }

        // And remove the edit list entry.
        let elst = self.edit_list_mut().unwrap();
        elst.entries.vec.remove(idx);
    }

    /// See if the track has an edit list. If so, check if there is an initial
    /// empty edit. If so, delete that edit, and extend the duration of the
    /// first sample with the length of the empty edit.
    pub fn initial_empty_edit_to_dwell(&mut self) {

        // let handler = self.media().handler();
        // let (is_audio, is_video) = (handler.is_audio(), handler.is_video());
        let media_timescale = self.media().media_header().timescale as u64;
        let movie_timescale = self.movie_timescale;

        // Initial empty edit?
        let elst = match self.edit_list_mut() {
            Some(elst) if elst.entries[0].media_time < 0 => elst,
            _ => return,
        };

/*
        FIXME: initial_empty_edit_to_dwell doesn't work to well on audio tracks.
               This can be fixed by updating `tfdt` instead. For now, just leave
               it like this, because after the initial segment it has the same effect.

        // audio often has fixed-length samples and it would confuse the codec to lengthen
        // the first sample. However, if it a very short empty edit, just get rid of it.
        if is_audio {
            let d = elst.entries[0].segment_duration as f64 / (movie_timescale as f64);
            // less than 10 ms .. we don't care.
            if d < 0.01 {
                elst.entries.vec.remove(0);
            }
            return;
        }

        // the below only works for video.
        if !is_video {
            return;
        }
*/
        let edit = elst.entries.vec.remove(0);
        let segment_duration = edit.segment_duration as u64;
        let offset = (media_timescale * segment_duration / movie_timescale as u64) as u32;

        let stts = self
            .media_mut()
            .media_info_mut()
            .sample_table_mut()
            .time_to_sample_mut();

        let entries = &mut stts.entries;
        if entries.len() == 0 {
            return;
        }

        if entries[0].count > 1 {
            let entry = entries[0].clone();
            entries.vec.insert(0, entry);
            entries[0].count = 1;
            entries[1].count -= 1;
        }

        entries[0].delta += offset;
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

        if let Some(_) = self.edit_list() {
            let tkhd = self.track_header();
            if tkhd.duration.0 == 0 {
                log::error!("TrackBox(id {}): duration 0, but has an edit list!", track_id);
                valid = false;
            }
        }

        valid
    }

    /// If the track has an edit list, see if it is a simple one, so
    /// that it's possible to transmux this track into an fMP4 track.
    ///
    /// We accept:
    /// - zero or one empty edits at the start
    /// - zero or one negative edits at the start
    /// - followed by zero or on edits over the entire track (for CTTS offsets).
    ///
    pub fn validate_simple_editlist(&self) -> bool {
        let track_id = self.track_id();
        let mut valid = true;

        let elst = match self.edit_list() {
            Some(elst) => elst,
            None => return valid,
        };
        let tkhd = self.track_header();

        let mut ctts_shift = false;
        for idx in 0 .. elst.entries.len() {
            let entry = &elst.entries[idx];

            if entry.media_rate != 1 {
                log::error!("TrackBox(id {}): edit list entry #{}: media_rate {}",
                    track_id,
                    idx,
                    entry.media_rate
                );
                valid = false;
                continue;
            }

            if entry.media_time < 0 {
                if idx != 0 {
                    log::error!("TrackBox(id {}): edit list entry #{}: media_time < 0",
                        track_id,
                        idx,
                    );
                    valid = false;
                }
                continue;
            }

            if entry.segment_duration == 0 {
                if idx != 0 {
                    log::error!("TrackBox(id {}): edit list entry #{}: \
                                 segment_duration = 0",
                        track_id,
                        idx)
                    ;
                    valid = false;
                    continue;
                }

                let d = self.duration_to_secs(entry.segment_duration, false);
                if d > 1.0 {
                    log::warn!("TrackBox(id {}): edit list entry #{}: priming: \
                                segment_duration > 1 ({:.3})",
                        idx,
                        track_id,
                        d
                    );
                }
                continue;
            }

            if ctts_shift {
                log::warn!("TrackBox(id {}): edit list entry #{}: \
                            spurious edit at end of track (ignored)",
                    track_id,
                    idx,
                );
                continue;
            }
            ctts_shift = true;

            // Check that this edit convers the entire track.
            let seg_d = self.duration_to_secs(entry.segment_duration, false);
            let track_d = self.duration_to_secs(tkhd.duration.0, false);
            if seg_d / track_d < 0.98 {
                log::error!("TrackBox(id {}): edit list entry #{}: \
                            segment_duration != track duration ({:.0}/{:.0})",
                     track_id,
                    idx,
                    seg_d,
                    track_d,
                );
                valid = false;
                break;
            }

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
