use std::io;

use crate::boxes::prelude::*;
use crate::boxes::{MovieHeaderBox, TrackBox};

def_box! {
    /// 8.2.1 Movie Box (ISO/IEC 14496-12:2015(E))
    MovieBox {
        boxes:      Vec<MP4Box>,
    },
    fourcc => "moov",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

impl MovieBox {

    /// Get a reference to the list of tracks.
    pub fn tracks(&self) -> Vec<&TrackBox> {
        self.boxes.iter().filter_map(|b| {
            match b {
                MP4Box::TrackBox(ref t) => Some(t),
                _ => None,
            }
        }).collect::<Vec<_>>()
    }

    /// Get a mutable reference to the list of tracks.
    pub fn tracks_mut(&mut self) -> Vec<&mut TrackBox> {
        self.boxes.iter_mut().filter_map(|b| {
            match b {
                MP4Box::TrackBox(ref mut t) => Some(t),
                _ => None,
            }
        }).collect::<Vec<_>>()
    }

    /// Get a reference to the MovieHeaderBox.
    pub fn movie_header(&self) -> &MovieHeaderBox {
        first_box!(&self.boxes, MovieHeaderBox).unwrap()
    }

    /// Get the track index by id.
    pub fn track_idx_by_id(&self, track_id: u32) -> Option<usize> {
        self.tracks().iter().enumerate().find_map(|(idx, t)| {
            if t.track_id() == track_id {
                Some(idx)
            } else {
                None
            }
        })
    }

    /// Get the index of the first track with this handler.
    pub fn track_idx_by_handler(&self, handler: FourCC) -> Option<usize> {
        self.tracks().iter().enumerate().find_map(|(idx, t)| {
            if t.media().handler().handler_type == handler {
                Some(idx)
            } else {
                None
            }
        })
    }

    pub fn is_valid(&self) -> bool {
        let mut valid = true;
        if self.tracks().len() == 0 {
            log::error!("MovieBox: no TrackBoxes present");
            valid = false;
        }
        if first_box!(&self.boxes, MovieHeaderBox).is_none() {
            log::error!("MovieBox: no MovieHeaderBox present");
            valid = false;
        }
        for t in &self.tracks() {
            if !t.is_valid() {
                valid = false;
            }
        }
        valid
    }
}

