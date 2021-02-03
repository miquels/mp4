use std::io;

use crate::boxes::prelude::*;
use crate::boxes::{TrackFragmentHeaderBox, TrackFragmentBaseMediaDecodeTimeBox, TrackRunBox};

def_box! {
    TrackFragmentBox {
        boxes:      Vec<MP4Box>,
    },
    fourcc => "traf",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

impl TrackFragmentBox {
    /// Get a reference to the Track Fragment Header.
    pub fn track_fragment_header(&self) -> Option<&TrackFragmentHeaderBox> {
        first_box!(&self.boxes, TrackFragmentHeaderBox)
    }

    /// Get a reference to the Track Fragment Decode Time.
    pub fn track_fragment_decode_time(&self) -> Option<&TrackFragmentBaseMediaDecodeTimeBox> {
        first_box!(&self.boxes, TrackFragmentBaseMediaDecodeTimeBox)
    }

    /// List of Track Run Boxes.
    pub fn track_run_boxes(&self) -> Vec<&TrackRunBox> {
        self.boxes.iter().filter_map(|b| {
            match b {
                MP4Box::TrackRunBox(ref t) => Some(t),
                _ => None,
            }
        }).collect::<Vec<_>>()
    }

}

