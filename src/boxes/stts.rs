use std::io;
use crate::boxes::prelude::*;

def_box! {
    TimeToSampleBox {
        entries:        ArraySized32<TimeToSampleEntry>,
    },
    fourcc => "stts",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

impl TimeToSampleBox {
    /// Return an iterator that iterates over every sample.
    pub fn iter(&self) -> TimeToSampleIterator<'_> {
        let mut iter = TimeToSampleIterator {
            entries: &self.entries,
            entry: TimeToSampleEntry::default(),
            index: 0,
        };
        if iter.entries.len() > 0 {
            iter.entry = iter.entries[0].clone();
        }
        iter
    }
}

def_struct! {
    /// Entry in TimeToSampleBox.
    #[derive(Default, Clone)]
    TimeToSampleEntry,
        count:  u32,
        delta:  u32,
}

pub struct TimeToSampleIterator<'a> {
    entries:    &'a [TimeToSampleEntry],
    entry:      TimeToSampleEntry,
    index:      usize,
}

impl<'a> Iterator for TimeToSampleIterator<'a> {
    type Item = u32;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.entry.count > 0 {
                self.entry.count -= 1;
                return Some(self.entry.delta);
            }
            self.index += 1;
            if self.index >= self.entries.len() {
                return None;
            }
            self.entry = self.entries[self.index].clone();
            if self.entry.count == 0 {
                continue;
            }
            self.entry.count -= 1;
            return Some(self.entry.delta);
        }
    }
}
