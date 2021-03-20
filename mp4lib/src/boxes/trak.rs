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
                    return Some(&elst);
                }
            }
        }
        None
    }

    /// Check the editlist to see if there's an initial composition time shift (see 8.6.1.3.1).
    ///
    /// Return value is expressed in media timescale units.
    pub fn composition_time_shift(&self) -> Option<u32> {
        if let Some(elst) = self.edit_list() {
            if elst.entries[0].media_time < 0 {
                // This is an empty edit. Assume the easy case, that there is
                // a single empty edit followed by the rest of the track.
                let media_timescale = self.media().media_header().timescale as u64;
                let segment_duration = elst.entries[0].segment_duration as u64;
                let offset = (media_timescale * segment_duration / self.movie_timescale as u64) as u32;
                return Some(offset);
            }
            return Some(elst.entries[0].media_time as u32);
        }
        None
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
