use std::io;

use crate::boxes::prelude::*;
use crate::boxes::{TrackHeaderBox, MediaBox};

def_box! {
    /// 8.3.1 Track Box (ISO/IEC 14496-12:2015(E))
    TrackBox, "trak",
        boxes:      [MP4Box],
}

impl TrackBox {

    /// Get a reference to this track's TrackHeaderBox.
    pub fn track_header(&self) -> &TrackHeaderBox {
        first_box!(&self.boxes, TrackHeaderBox).unwrap()
    }

    /// Get a reference to this track's MediaBox.
    pub fn media(&self) -> &MediaBox {
        first_box!(&self.boxes, MediaBox).unwrap()
    }

    /// Get a mutable reference to this track's MediaBox.
    pub fn media_mut(&mut self) -> &mut MediaBox {
        first_box_mut!(&mut self.boxes, MediaBox).unwrap()
    }

    /// Check if this track is valid (has header and media boxes).
    pub fn is_valid(&self) -> bool {
        let mut valid = true;
        let track_id = match first_box!(&self.boxes, TrackHeaderBox) {
            Some(th) => th.track_id,
            None => {
                error!("TrackBox: no TrackHeaderBox present");
                return false;
            },
        };
        match first_box!(&self.boxes, MediaBox) {
            Some(m) => {
                if !m.is_valid() {
                    valid = false;
                }
            },
            None => {
                error!("TrackBox(id {}): no MediaBox present", track_id);
                valid = false;
            },
        }
        valid
    }
}

