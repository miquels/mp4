use std::io;

use crate::boxes::prelude::*;

def_box! {
    /// Chapter List ("Nero" format).
    ChapterListBox {
        chapters: Vec<Chapter>,
    },
    fourcc => "chpl",
    version => [1],
    impls => [ boxinfo, debug, fullbox ],
}

def_struct! {
    /// Chapter ("Nero" format).
    /// `start` units is in 10_000 * MovieHeaderBox.timescale.
    Chapter,
        start: u64,
        title: PString,
}

impl FromBytes for ChapterListBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
        let mut reader = BoxReader::new(stream)?;
        let stream = &mut reader;
        let mut chapters = Vec::new();
        /*
        while let Ok(b) = u8::from_bytes(stream) {
            println!("XXX chpl {:02x}", b);
        }
        */
        let _skip = u8::from_bytes(stream)?;
        println!("XXX _skip {}", _skip);
        let count = u32::from_bytes(stream)?;
        println!("XXX chapter count {}", count);
        for _ in 0 .. count {
            chapters.push(Chapter::from_bytes(stream)?);
        }
        Ok(ChapterListBox {
            chapters,
        })
    }

    fn min_size() -> usize {
        1
    }
}

impl ToBytes for ChapterListBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self)?;
        let stream = &mut writer;
        let count = self.chapters.len() as u8;
        count.to_bytes(stream)?;
        self.chapters.to_bytes(stream)?;
        Ok(())
    }
}
