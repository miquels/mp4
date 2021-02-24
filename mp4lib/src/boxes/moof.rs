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
    #[derive(Default)]
    MovieFragmentBox {
        offset:     u64,
        boxes:      Vec<MP4Box>,
    },
    fourcc => "moof",
    version => [],
    impls => [ basebox, boxinfo, debug ],
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

impl FromBytes for MovieFragmentBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<MovieFragmentBox> {
        let offset = stream.pos();
        let mut reader = BoxReader::new(stream)?;
        let boxes = Vec::<MP4Box>::from_bytes(&mut reader)?;
        Ok(MovieFragmentBox {
            offset,
            boxes,
        })
    }
    fn min_size() -> usize { 8 }
}

impl ToBytes for MovieFragmentBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self)?;
        self.boxes.to_bytes(&mut writer)?;
        writer.finalize()
    }
}
