use std::io;

use crate::boxes::prelude::*;
use crate::boxes::{AacSampleEntry, SampleTableBox, TrackHeaderBox, MediaBox, EditBox, EditListBox};
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
    /// If there's an initial empty edit, it is ignored. Deal with it another way,
    /// for example with `initial_empty_edit_to_dwell` (for video) or just
    /// ignore it (for audio).
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

        let handler = self.media().handler();
        let (is_audio, is_video) = (handler.is_audio(), handler.is_video());
        let media_timescale = self.media().media_header().timescale as u64;

        // Initial empty edit?
        let elst = match self.edit_list_mut() {
            Some(elst) if elst.entries[0].media_time < 0 => elst,
            _ => return,
        };

        // audio often has fixed-length samples and it would confuse the codec to lengthen
        // the first sample. However, if it a very short empty edit, just get rid of it.
        if is_audio {
            if media_timescale > 0 {
                let d = elst.entries[0].segment_duration as f64 / (media_timescale as f64);
                // less than 10 ms .. we don't care.
                if d < 0.01 {
                    elst.entries.vec.remove(0);
                }
            }
            return;
        }

        // the below only works for video.
        if !is_video {
            return;
        }

        let edit = elst.entries.vec.remove(0);
        let segment_duration = edit.segment_duration as u64;
        let offset = (media_timescale * segment_duration / self.movie_timescale as u64) as u32;

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

        // The must be exactly one MediaBox present.
        match first_box!(&self.boxes, SampleTableBox) {
            Some(m) => {
                if !m.is_valid() {
                    valid = false;
                }
            },
            None => {
                log::error!("TrackBox(id {}): no SampleTableBox present", track_id);
                valid = false;
            },
        }

        // If there is an edit list, the first entry must be valid
        // for the entire track.
        if let Some(elst) = self.edit_list() {
            let tkhd = self.track_header();
            if tkhd.duration.0 != 0 {

                // If the first entry has about the same duration as the track,
                // assume it covers the entire track.
                let entry = &elst.entries[0];
                let x = (entry.segment_duration as f64) / (tkhd.duration.0 as f64);
                if x < 0.95f64 {
                    log::error!("TrackBox(id {}): EditBox: first entry: does not cover entire duration", track_id);
                    valid = false;
                }
                if entry.media_rate != 1 {
                    log::error!("TrackBox(id {}): EditBox: first entry: does not have media_rate 1", track_id);
                    valid = false;
                }
                if entry.media_time > i32::MAX as i64 {
                    log::error!("TrackBox(id {}): EditBox: first entry: media_time too large", track_id);
                    valid = false;
                }
                if entry.media_time < 0 {
                    log::error!("TrackBox(id {}): EditBox: first entry: media_time negative", track_id);
                    valid = false;
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
