use std::io;

use crate::boxes::prelude::*;
use crate::boxes::{MovieHeaderBox, TrackBox};

def_box! {
    /// 8.2.1 Movie Box (ISO/IEC 14496-12:2015(E))
    MovieBox, "moov",
        boxes:      [MP4Box],
}

impl MovieBox {
    pub fn tracks(&self) -> Vec<&TrackBox> {
        self.boxes.iter().filter_map(|b| {
            match b {
                MP4Box::TrackBox(ref t) => Some(t),
                _ => None,
            }
        }).collect::<Vec<_>>()
    }

    pub fn tracks_mut(&mut self) -> Vec<&mut TrackBox> {
        self.boxes.iter_mut().filter_map(|b| {
            match b {
                MP4Box::TrackBox(ref mut t) => Some(t),
                _ => None,
            }
        }).collect::<Vec<_>>()
    }

    pub fn movie_header(&self) -> &MovieHeaderBox {
        self.movie_header_option().unwrap()
    }

    pub(crate) fn movie_header_option(&self) -> Option<&MovieHeaderBox> {
        self.boxes.iter().find_map(|b| {
            match b {
                MP4Box::MovieHeaderBox(ref t) => Some(t),
                _ => None,
            }
        })
    }

    pub fn is_valid(&self) -> bool {
        let mut valid = true;
        if self.tracks().len() == 0 {
            error!("MovieBox: no TrackBoxes present");
            valid = false;
        }
        if self.movie_header_option().is_none() {
            error!("MovieBox: no MovieHeaderBox present");
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

