use std::io;
use crate::boxes::prelude::*;

def_box! {
    CompositionOffsetBox {
        entries:        [CompositionOffsetEntry, sized],
    },
    fourcc => "ctts",
    version => [1, entries],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

impl CompositionOffsetBox {
    /// Return an iterator that iterates over every sample.
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

