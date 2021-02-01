use std::io;
use crate::boxes::prelude::*;

def_box! {
    SyncSampleBox {
        entries:        [u32, sized],
    },
    fourcc => "stss",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

impl SyncSampleBox {
    /// Return an iterator that iterates over every sample.
    pub fn iter(&self) -> SyncSampleIterator<'_> {
        SyncSampleIterator {
            entries: &self.entries,
            index: 0,
        }
    }
}

pub struct SyncSampleIterator<'a> {
    entries:    &'a [u32],
    index:      usize,
}

impl<'a> Iterator for SyncSampleIterator<'a> {
    type Item = u32;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.entries.len() {
            return None;
        }
        let val = self.entries[self.index];
        self.index += 1;
        Some(val)
    }
}
