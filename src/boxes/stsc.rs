use std::io;
use crate::boxes::prelude::*;

def_box! {
    SampleToChunkBox {
        entries:        ArraySized32<SampleToChunkEntry>,
    },
    fourcc => "stsc",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

impl SampleToChunkBox {
    /// Return an iterator that iterates over every sample.
    pub fn iter(&self) -> SampleToChunkIterator {
        if self.entries.len() == 0 {
            // This should never happen, but code defensively.
            SampleToChunkIterator {
                entries: &self.entries,
                index: 0,
                count: 1,
                chunk: 0,
                sdi: 1,
            }
        } else {
            SampleToChunkIterator {
                entries: &self.entries,
                index: 0,
                count: self.entries[0].samples_per_chunk,
                chunk: self.entries[0].first_chunk.saturating_sub(1),
                sdi: self.entries[0].sample_description_index.saturating_sub(1),
            }
        }
    }
}

def_struct! {
    /// Entry in SampleToChunkBox.
    SampleToChunkEntry,
        first_chunk:                u32,
        samples_per_chunk:          u32,
        sample_description_index:   u32,
}

/// Iterator over the SampleToChunk table.
pub struct SampleToChunkIterator<'a> {
    entries:    &'a [SampleToChunkEntry],
    index:      usize,
    count:      u32,
    chunk:      u32,
    sdi:        u32,
}

/// Value returned by SampleToChunkIterator.
///
/// Note that the `chunk` and `sample_description_index` values
/// here are 0-based, *not* 1-based as in the ISO/IEC 14496-12 spec.
pub struct SampleToChunkIterEntry {
    pub chunk:  u32,
    pub sample_description_index: u32,
}

impl<'a> Iterator for SampleToChunkIterator<'a> {
    type Item = SampleToChunkIterEntry;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.count > 0 {
                self.count -= 1;
                return Some(SampleToChunkIterEntry {
                    chunk: self.chunk,
                    sample_description_index: self.sdi,
                });
            }
            self.chunk += 1;
            self.count = self.entries[self.index].samples_per_chunk;

            let next_index = self.index + 1;
            if next_index >= self.entries.len() {
                // should not happen, but prevent loop.
                if self.count == 0 {
                    self.count = 1;
                }
                continue;
            }

            if self.entries[next_index].first_chunk == self.chunk + 1 {
                self.index += 1;
                self.sdi = self.entries[self.index].sample_description_index.saturating_sub(1);
                self.count = self.entries[self.index].samples_per_chunk;
            }
        }
    }
}

