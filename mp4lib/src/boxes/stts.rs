use std::io;
use crate::boxes::prelude::*;

def_box! {
    #[derive(Default)]
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
            cumulative: 0,
        };
        if iter.entries.len() > 0 {
            iter.entry = iter.entries[0].clone();
        }
        iter
    }
}

def_struct! {
    /// Entry in TimeToSampleBox.
    #[derive(Default)]
    TimeToSampleEntry,
        count:  u32,
        delta:  u32,
}

#[derive(Clone)]
pub struct TimeToSampleIterator<'a> {
    entries:    &'a [TimeToSampleEntry],
    entry:      TimeToSampleEntry,
    index:      usize,
    cumulative: u64,
}

impl TimeToSampleIterator<'_> {
    /// Seek to a sample.
    ///
    /// Sample indices start at `1`.
    pub fn seek(&mut self, seek_to: u32) -> io::Result<()> {
        // FIXME: this is not very efficient. do something smarter.
        let seek_to = std::cmp::max(1, seek_to);
        let mut cur_sample = 1;
        let mut cumulative = 0;
        for (index, entry) in self.entries.iter().enumerate() {
            if seek_to >= cur_sample && seek_to < cur_sample + entry.count {
                self.entry.count = cur_sample + entry.count - seek_to;
                self.cumulative = cumulative + (seek_to - cur_sample) as u64 * (entry.delta as u64);
                self.entry = self.entries[index].clone();
                self.index = index;
                return Ok(());
            }
            cur_sample += entry.count;
            cumulative += entry.count as u64 * (entry.delta as u64);
        }
        Err(ioerr!(UnexpectedEof))
    }
}

impl<'a> Iterator for TimeToSampleIterator<'a> {
    type Item = (u32, u64);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.entry.count > 0 {
                self.entry.count -= 1;
                let cumulative = self.cumulative;
                self.cumulative += self.entry.delta as u64;
                return Some((self.entry.delta, cumulative));
            }
            self.index += 1;
            if self.index >= self.entries.len() {
                return None;
            }
            self.entry = self.entries[self.index].clone();
        }
    }
}
