use std::io;

use crate::boxes::prelude::*;
use crate::boxes::{MovieHeaderBox, TrackBox, TrackExtendsBox};

def_box! {
    /// 8.2.1 Movie Box (ISO/IEC 14496-12:2015(E))
    #[derive(Default)]
    MovieBox {
        boxes:      Vec<MP4Box>,
    },
    fourcc => "moov",
    version => [],
    impls => [ basebox, boxinfo, debug ],
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

    /// Get the track by id.
    pub fn track_by_id(&self, track_id: u32) -> Option<&TrackBox> {
        self.tracks().iter().find_map(|&t| {
            if t.track_id() == track_id {
                Some(t)
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

    /// Get the Track Extends box for this track.
    pub fn track_extends_by_id(&self, track_id: u32) -> Option<&TrackExtendsBox> {
        self.boxes.iter().find_map(|b| {
            match b {
                MP4Box::TrackExtendsBox(t) => {
                    if t.track_id == track_id {
                        return Some(t);
                    }
                }
                _ => {},
            }
            None
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

impl FromBytes for MovieBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<MovieBox> {
        let mut reader = BoxReader::new(stream)?;
        let mut boxes = Vec::<MP4Box>::from_bytes(&mut reader)?;
        if let Some(movie_header) = first_box!(&boxes, MovieHeaderBox) {
            let timescale = movie_header.timescale;
            for trak in iter_box_mut!(&mut boxes, TrackBox) {
                trak.movie_timescale = timescale;
            }
        }
        Ok(MovieBox {
            boxes,
        })
    }
    fn min_size() -> usize { 8 }
}

impl ToBytes for MovieBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self)?;
        self.boxes.to_bytes(&mut writer)?;
        writer.finalize()
    }
}
