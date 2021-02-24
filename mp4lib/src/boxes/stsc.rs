use std::io;
use crate::boxes::prelude::*;

def_box! {
    /// 8.7.4 Sample To Chunk Box (ISO/IEC 14496-12:2015(E))
    #[derive(Default)]
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
        SampleToChunkIterator::new(&self.entries[..])
    }
}

def_struct! {
    /// Entry in SampleToChunkBox.
    SampleToChunkEntry,
        first_chunk:                u32,
        samples_per_chunk:          u32,
        sample_description_index:   u32,
}

/// Value returned by SampleToChunkIterator.
///
/// Note that the `chunk` and `sample_description_index` values
/// here are 1-based, as per the ISO/IEC 14496-12 spec.
pub struct SampleToChunkIterEntry {
    pub cur_chunk:  u32,
    pub first_sample: u32,
    pub sample_description_index: u32,
}

/// Iterator over the SampleToChunk table.
#[derive(Clone)]
pub struct SampleToChunkIterator<'a> {
    entries:    &'a [SampleToChunkEntry],
    index:      usize,
    cur_chunk:  u32,
    cur_sdi:    u32,
    count:      u32,
    cur_sample: u32,
    first_sample: u32,
    cur_entry:  Option<&'a SampleToChunkEntry>,
}

impl std::fmt::Debug for SampleToChunkIterator<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut dbg = f.debug_struct("SampleToChunkIterator");
        dbg.field("index", &self.index);
        dbg.field("cur_chunk", &self.cur_chunk);
        dbg.field("cur_sdi", &self.cur_sdi);
        dbg.field("count", &self.count);
        dbg.field("cur_sample", &self.cur_sample);
        dbg.field("first_sample", &self.first_sample);
        dbg.field("cur_entry", &self.cur_entry);
        dbg.finish()
    }
}

impl<'a> SampleToChunkIterator<'a> {
    fn new(entries: &[SampleToChunkEntry]) -> SampleToChunkIterator<'_> {
        if entries.len() == 0 {
            SampleToChunkIterator {
                entries,
                index: 0,
                cur_chunk: 0,
                cur_sdi: 0,
                count: 0,
                cur_sample: 0,
                first_sample: 0,
                cur_entry: None,
            }
        } else {
            let cur_entry = &entries[0];
            SampleToChunkIterator {
                entries,
                index: 0,
                cur_chunk: cur_entry.first_chunk,
                cur_sdi: cur_entry.sample_description_index,
                count: cur_entry.samples_per_chunk,
                cur_sample: 1,
                first_sample: 1,
                cur_entry: None,
            }
        }
    }

    // Get the current SampleToChunkEntry, incrementing 'cur_chunk'.
    // Return the same SampleToChunkEntry until 'cur_chunk' matches
    // the _next_ entry's first_chunk.
    fn next_chunk(&mut self) -> Option<&SampleToChunkEntry> {

        if self.cur_chunk == 0 {
            return None;
        }

        // We might be at the start.
        if self.cur_entry.is_none() {
            self.cur_entry = Some(&self.entries[self.index]);
            return self.cur_entry;
        }

        // Increase chunk number.
        self.cur_chunk += 1;

        // If we're at the end of this run of chunks, take the next.
        if self.index + 1 < self.entries.len() &&
            self.cur_chunk == self.entries[self.index + 1].first_chunk
        {
            self.index += 1;
            self.cur_entry = Some(&self.entries[self.index]);
            self.cur_sdi = self.entries[self.index].sample_description_index;
        }

        self.count = self.entries[self.index].samples_per_chunk;
        self.first_sample = self.cur_sample;

        self.cur_entry
    }

    fn rewind(&mut self) {
        *self = Self::new(&self.entries);
    }

    /// Seek to a sample.
    ///
    /// Sample indices start at `1`.
    pub fn seek(&mut self, to_sample: u32) -> io::Result<()> {
        self.rewind();
        if to_sample <= 1 {
            return Ok(());
        }

        // walk over all entries, and find the entry that 'fits'
        while let Some(chunk) = self.next_chunk() {

            // If 'seek_to' fits here, we have a match.
            let samples_per_chunk = chunk.samples_per_chunk;
            if to_sample >= self.cur_sample && to_sample < self.cur_sample + samples_per_chunk {
                self.count = self.cur_sample + samples_per_chunk - to_sample;
                self.cur_sample = to_sample;
                return Ok(())
            }
            self.cur_sample += samples_per_chunk;
            if self.cur_sample > to_sample {
                break;
            }
        }

        Err(ioerr!(UnexpectedEof))
    }
}

impl<'a> Iterator for SampleToChunkIterator<'a> {
    type Item = SampleToChunkIterEntry;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.count > 0 {
                self.count -= 1;
                self.cur_sample += 1;
                return Some(SampleToChunkIterEntry {
                    cur_chunk: self.cur_chunk,
                    first_sample: self.first_sample,
                    sample_description_index: self.cur_sdi,
                });
            }
            if self.next_chunk().is_none() {
                return None;
            }
        }
    }
}

