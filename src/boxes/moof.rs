use std::io;

use crate::boxes::prelude::*;
use crate::boxes::TrackFragmentBox;

def_box! {
    /// Movie Fragment Box.
    ///
    /// Contains:
    /// - `1  ` MovieFragmentHeaderBox
    /// - `0-1` MetaBox
    /// - `0+ ` TrackFragmentBox
    ///
    MovieFragmentBox {
        boxes:      Vec<MP4Box>,
    },
    fourcc => "moof",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

impl MovieFragmentBox {
    /// Get a reference to the list of track fragments.
    pub fn track_fragments(&self) -> Vec<&TrackFragmentBox> {
        self.boxes.iter().filter_map(|b| {
            match b {
                MP4Box::TrackFragmentBox(ref t) => Some(t),
                _ => None,
            }
        }).collect::<Vec<_>>()
    }
}

