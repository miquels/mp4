use std::io;
use crate::boxes::prelude::*;

/// 8.7.5 Chunk Offset Box  (ISO/IEC 14496-12:2015(E))
///
/// Implements both "stco" and "co64".
#[derive(Clone, Debug)]
pub struct ChunkOffsetBox {
    pub fourcc:  FourCC,
    pub entries: Entries,
    offset: i64,
    large: bool,
}
pub type ChunkLargeOffsetBox = ChunkOffsetBox;

#[derive(Debug, Clone)]
pub struct Entries(Entries_);

#[derive(Debug, Clone)]
enum Entries_ {
    Normal(ArraySized32<u32>),
    Large(ArraySized32<u64>),
}

impl Entries {
    /// Get the value at index `index`.
    pub fn get(&self, index: usize) -> u64 {
        match &self.0 {
            Entries_::Normal(entries) => entries.get(index) as u64,
            Entries_::Large(entries) => entries.get(index),
        }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> u64 {
        match &self.0 {
            Entries_::Normal(entries) => entries.len() as u64,
            Entries_::Large(entries) => entries.len() as u64,
        }
    }
}

impl FromBytes for ChunkOffsetBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<ChunkOffsetBox> {
        let mut reader = BoxReader::new(stream)?;
        let fourcc = reader.header.fourcc;
        let stream = &mut reader;

        let (entries, large) = if fourcc == b"stco" {
            (Entries_::Normal(ArraySized32::<u32>::from_bytes(stream)?), false)
        } else {
            (Entries_::Large(ArraySized32::<u64>::from_bytes(stream)?), true)
        };

        Ok(ChunkOffsetBox {
            fourcc,
            entries: Entries(entries),
            offset: 0,
            large,
        })
    }

    fn min_size() -> usize { 16 }
}

impl ToBytes for ChunkOffsetBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self)?;
        let stream = &mut writer;
        match &self.entries.0 {
            Entries_::Normal(entries) => {
                (entries.len() as u32).to_bytes(stream)?;
                if self.large {
                    for &entry in entries {
                        let entry = entry as i64 + (self.offset as i64);
                        (entry as u64).to_bytes(stream)?;
                    }
                } else {
                    for &entry in entries {
                        let entry = entry as i64 + (self.offset as i64);
                        (entry as u32).to_bytes(stream)?;
                    }
                }
            },
            Entries_::Large(entries) => {
                (entries.len() as u32).to_bytes(stream)?;
                if self.large {
                    for &entry in entries {
                        let entry = entry as i64 + (self.offset as i64);
                        (entry as u64).to_bytes(stream)?;
                    }
                } else {
                    for &entry in entries {
                        let entry = entry as i64 + (self.offset as i64);
                        (entry as u32).to_bytes(stream)?;
                    }
                }
            },
        }
        Ok(())
    }
}

impl ChunkOffsetBox {
    /// Add a global extra offset to all entries in this box.
    ///
    /// The offset is applied when serializing the box. If after applying
    /// the offset any entry is larger than 4G (2^32 - 1), the box will
    /// be serialized as a ChunkLargeOffsetBox (`co64`).
    pub fn add_offset(&mut self, move_offset: i64) {
        self.offset = move_offset;
        self.check_offsets();
    }

    /// Add an offset to the list.
    ///
    /// If this is a `ChunkOffsetBox` that was created by `from_bytes`, it is
    /// read-only and this method will panic.
    pub fn push(&mut self, offset: u64) {
        if offset as i64 + self.offset > u32::MAX as i64 {
            self.fourcc = FourCC::new("co64");
            self.large = true;
        }

        match &mut self.entries.0 {
            Entries_::Large(e) => e.push(offset),
            Entries_::Normal(_) => unreachable!(),
        }
    }

    /// Returns an iterator over all offsets in the box.
    pub fn iter(&self) -> ChunkOffsetIterator {
        ChunkOffsetIterator::new(self)
    }

    // Check all the offsets in the table and decide whether to write a stco or co64 box.
    fn check_offsets(&mut self) {
        let offset = self.offset;
        let large = match &self.entries.0 {
            Entries_::Normal(entries) => entries.iter_cloned().any(|e| (e as i64 + offset) > u32::MAX as i64),
            Entries_::Large(entries) => entries.iter_cloned().any(|e| (e as i64 + offset) > u32::MAX as i64),
        };
        if large {
            self.fourcc = FourCC::new("co64");
            self.large = true;
        }
    }
}

impl Default for ChunkOffsetBox {
    fn default() -> Self {
        ChunkOffsetBox {
            fourcc:  FourCC::new("stco"),
            entries: Entries(Entries_::Large(ArraySized32::<u64>::default())),
            offset: 0,
            large: false,
        }
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

enum IteratorCloned<'a> {
    Normal(ArrayIteratorCloned<'a, u32>),
    Large(ArrayIteratorCloned<'a, u64>),
}

/// Iterator over the contents of the ChunkOffsetBox.
pub struct ChunkOffsetIterator<'a>(IteratorCloned<'a>);

impl<'a> ChunkOffsetIterator<'a> {
    fn new(stco: &'a ChunkOffsetBox) -> Self {
        let iter = match &stco.entries.0 {
            Entries_::Normal(entries) => IteratorCloned::Normal(entries.iter_cloned()),
            Entries_::Large(entries) => IteratorCloned::Large(entries.iter_cloned()),
        };
        ChunkOffsetIterator(iter)
    }

    /// Check if all items fall in the range.
    pub fn in_range(&self, range: std::ops::Range<u64>) -> bool {
        match &self.0 {
            IteratorCloned::Normal(iter) => {
                if range.start > u32::MAX as u64 || range.end > u32::MAX as u64 {
                    false
                } else {
                    iter.in_range(range.start as u32 .. range.end as u32)
                }
            },
            IteratorCloned::Large(iter) => iter.in_range(range),
        }
    }
}

impl<'a> Iterator for ChunkOffsetIterator<'a> {
    type Item = u64;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.0 {
            IteratorCloned::Normal(iter) => iter.next().map(|o| o as u64),
            IteratorCloned::Large(iter) => iter.next(),
        }
    }
}

