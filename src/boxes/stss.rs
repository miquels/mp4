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
    /// It does not iterate over every sample.
    ///
    /// Note that, unlike in the ISO/IRC 14496-12 spec,
    /// the sample index is 0 (zero) based, not 1 based.
    ///
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
        let val = self.entries[self.index].saturating_sub(1);
        self.index += 1;
        Some(val)
    }
}
