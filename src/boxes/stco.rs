use std::io;
use crate::boxes::prelude::*;

/// 8.7.5 Chunk Offset Box  (ISO/IEC 14496-12:2015(E))
///
/// Implements both "stco" and "co64".
#[derive(Clone, Debug, Default)]
pub struct ChunkOffsetBox {
    fourcc:      FourCC,
    pub count:   u32,
    pub entries: ArrayUnsized<u64>,
}
pub type ChunkLargeOffsetBox = ChunkOffsetBox;

impl FromBytes for ChunkOffsetBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<ChunkOffsetBox> {
        let mut reader = BoxReader::new(stream)?;
        let fourcc = reader.header.fourcc;
        let stream = &mut reader;

        let count = u32::from_bytes(stream)?;
        let mut entries = ArrayUnsized::new();

        while entries.len() < count as usize  && stream.left() >= 4 {
            if fourcc == b"co64" {
                entries.push(u64::from_bytes(stream)?);
            } else {
                entries.push(u32::from_bytes(stream)? as u64);
            }
        }

        Ok(ChunkOffsetBox {
            fourcc,
            count,
            entries,
        })
    }

    fn min_size() -> usize { 16 }
}

impl ChunkOffsetBox {
    /// Check all the offsets in the table and decide whether to write a stco or co64 box.
    pub fn check_sizes(&mut self) {
        let large = self.entries.iter().find_map(|e| {
            if *e > 0xffffffff { Some(true) } else { None }
        }).unwrap_or(false);
        if large {
            self.fourcc = FourCC::new("co64");
        }
    }
}

impl ToBytes for ChunkOffsetBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self)?;
        let stream = &mut writer;

        (self.entries.len() as u32).to_bytes(stream)?;
        for e in &self.entries {
            if self.fourcc == b"co64" {
                e.to_bytes(stream)?;
            } else {
                let b = (*e & 0xffffffff) as u32;
                b.to_bytes(stream)?;
            }
        }

        stream.finalize()
    }
}

impl BoxInfo for ChunkOffsetBox {
    const FOURCC: &'static str = "stco";

    #[inline]
    fn fourcc(&self) -> FourCC {
        self.fourcc
    }
    #[inline]
    fn max_version() -> Option<u8> {
        Some(0)
    }
}

impl FullBox for ChunkOffsetBox {
    fn version(&self) -> Option<u8> {
        Some(0)
    }
}
