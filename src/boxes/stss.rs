use std::io;
use crate::boxes::prelude::*;

def_box! {
    #[derive(Default)]
    SyncSampleBox {
        entries:        ArraySized32<u32>,
    },
    fourcc => "stss",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

impl SyncSampleBox {
    /// Return an iterator that iterates over the sync sample table.
    ///
    /// Even though the sync sample table does not have an entry
    /// for each sample, the iterator iterates over every sample.
    pub fn iter(&self) -> SyncSampleIterator<'_> {
        SyncSampleIterator {
            entries: &self.entries,
            index: 0,
            cur_sample: 0,
        }
    }
}

pub struct SyncSampleIterator<'a> {
    entries:    &'a [u32],
    index:      usize,
    cur_sample: u32,
}

impl<'a> SyncSampleIterator<'a> {
    pub fn seek(&mut self, to_sample: u32) -> io::Result<()> {
        if self.entries.len() == 0 {
            return Ok(())
        }
        if self.index == self.entries.len() {
            self.index = 0;
        }
        let to_sample = std::cmp::max(1, to_sample);
        if self.entries[self.index] < to_sample {
            self.index = 0;
        }
        while self.index < self.entries.len() {
            if self.entries[self.index] >= to_sample {
                break;
            }
            self.index += 1;
        }
        self.cur_sample = to_sample;
        Ok(())
    }
}

impl<'a> Iterator for SyncSampleIterator<'a> {
    type Item = bool;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.entries.len() == 0 {
            return Some(true);
        }
        if self.index == self.entries.len() {
            return Some(false);
        }
        if self.entries[self.index] == self.cur_sample {
            self.cur_sample += 1;
            self.index += 1;
            return Some(true);
        }
        self.cur_sample += 1;
        Some(false)
    }
}
