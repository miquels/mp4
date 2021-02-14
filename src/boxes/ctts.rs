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
            index: 0,
            cur_entry: CompositionOffsetEntry::default(),
        };
        if iter.entries.len() > 0 {
            iter.cur_entry = iter.entries[0].clone();
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
                let cur_entry = CompositionOffsetEntry {
                    count: tot + count - from,
                    offset,
                };
                return CompositionOffsetIterator {
                    entries,
                    index,
                    cur_entry,
                }
            }
            tot += count;
        }
        // No match, so return an iterator that is exhausted.
        CompositionOffsetIterator {
            entries: &self.entries,
            index: entries.len(),
            cur_entry: CompositionOffsetEntry::default(),
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

/// Iterator over the entries of a CompositionOffsetBox.
pub struct CompositionOffsetIterator<'a> {
    entries:    &'a [CompositionOffsetEntry],
    index:      usize,
    cur_entry:  CompositionOffsetEntry,
}

impl <'a> CompositionOffsetIterator<'a> {
    /// Seek to a sample.
    ///
    /// Sample indices start at `1`.
    pub fn seek(&mut self, seek_to: u32) -> io::Result<()> {
        let mut cur_sample = 0;
        let entries = &self.entries;
        let seek_to = seek_to.saturating_sub(1);

        // walk over all entries, and find the entry where to 'seek_to' index fits.
        for index in 0 .. entries.len() {
            let offset = entries[index].offset;
            let count = entries[index].count;

            // If 'seek_to' fits here, we have a match.
            if seek_to >= cur_sample && seek_to < cur_sample + count {
                // build a 'current entry' with the correct 'count' for this sample offset.
                self.cur_entry = CompositionOffsetEntry {
                    count: cur_sample + count - seek_to,
                    offset,
                };
                self.index = index;
                return Ok(());
            }
            cur_sample += count;
        }
        Err(io::ErrorKind::UnexpectedEof.into())
    }
}

impl<'a> Iterator for CompositionOffsetIterator<'a> {
    type Item = i32;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // each entry repeats 'count' times.
            if self.cur_entry.count > 0 {
                self.cur_entry.count -= 1;
                return Some(self.cur_entry.offset);
            }
            if self.index + 1 >= self.entries.len() {
                return None;
            }
            self.index += 1;
            self.cur_entry = self.entries[self.index].clone();
        }
    }
}

