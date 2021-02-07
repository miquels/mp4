use std::io;
use crate::boxes::prelude::*;

def_box! {
    #[derive(Default)]
    CompositionOffsetBox {
        entries:        ArraySized32<CompositionOffsetEntry>,
    },
    fourcc => "ctts",
    version => [1, entries],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

impl CompositionOffsetBox {
    /// Returns an iterator that iterates over every sample.
    pub fn iter(&self) -> CompositionOffsetIterator {
        let mut iter = CompositionOffsetIterator {
            entries: &self.entries,
            entry: CompositionOffsetEntry::default(),
            index: 0,
        };
        if iter.entries.len() > 0 {
            iter.entry = iter.entries[0].clone();
        }
        iter
    }

    /// Returns an iterator starting at sample `from`.
    pub fn iter_from(&self, from: u32) -> CompositionOffsetIterator {
        let mut tot = 0;
        let entries = &self.entries;

        // walk over all entries, and find the entry that 'fits' the 'from' sample count.
        for index in 0 .. entries.len() {
            let offset = entries[index].offset;
            let count = entries[index].count;

            // If 'from' fits here, we have a match.
            if from >= tot && from < tot + count {
                // build a 'current entry' with the correct 'count' for this sample offset.
                let entry = CompositionOffsetEntry {
                    count: tot + count - from,
                    offset,
                };
                return CompositionOffsetIterator {
                    entries,
                    entry,
                    index,
                }
            }
            tot += count;
        }
        // No match, so return an iterator that is exhausted.
        CompositionOffsetIterator {
            entries: &self.entries,
            entry: CompositionOffsetEntry::default(),
            index: entries.len(),
        }
    }
}

/// Composition offset entry.
#[derive(Debug, Default, Clone)]
pub struct CompositionOffsetEntry {
    pub count:  u32,
    pub offset: i32,
}

impl FromBytes for CompositionOffsetEntry {
    // NOTE: This implementation is not _entirely_ correct. If in a
    // version 0 entry the offset >= 2^31 it breaks horribly.
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
        let count = u32::from_bytes(stream)?;
        let offset = if stream.version() == 0 {
            let offset = u32::from_bytes(stream)?;
            std::cmp::min(offset, 0x7fffffff) as i32
        } else {
            i32::from_bytes(stream)?
        };
        Ok(CompositionOffsetEntry { count, offset })
    }

    fn min_size() -> usize {
        8
    }
}

impl ToBytes for CompositionOffsetEntry {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        self.count.to_bytes(stream)?;
        self.offset.to_bytes(stream)?;
        Ok(())
    }
}

impl FullBox for CompositionOffsetEntry {
    fn version(&self) -> Option<u8> {
        if self.offset < 0 {
            Some(1)
        } else {
            None
        }
    }
}

pub struct CompositionOffsetIterator<'a> {
    entries:    &'a [CompositionOffsetEntry],
    entry:      CompositionOffsetEntry,
    index:      usize,
}

impl<'a> Iterator for CompositionOffsetIterator<'a> {
    type Item = i32;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.entry.count > 0 {
                self.entry.count -= 1;
                return Some(self.entry.offset);
            }
            self.index += 1;
            if self.index >= self.entries.len() {
                return None;
            }
            self.entry = self.entries[self.index].clone();
        }
    }
}

