use std::io;

use crate::boxes::prelude::*;
use crate::boxes::{SampleDescriptionBox, SampleSizeBox, TimeToSampleBox, SampleToChunkBox};
use crate::boxes::{ChunkOffsetBox, ChunkLargeOffsetBox};
use crate::boxes::{CompositionOffsetBox, SyncSampleBox};

// stsd, stts, stsc, stco, co64
// ctts, cslg, stsz, stz2, stss, stsh, padb, stdp, sbgp, sgpd, subs, saiz, saio

def_box! {
    /// 8.1.1 Sample Table Box (ISO/IEC 14496-12:2015(E))
    SampleTableBox {
        boxes:      [MP4Box],
    },
    fourcc => "stbl",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

impl SampleTableBox {

    declare_box_methods!(SampleDescriptionBox, sample_description, sample_description_mut);
    declare_box_methods!(SampleSizeBox, sample_size, sample_size_mut);
    declare_box_methods!(TimeToSampleBox, time_to_sample, time_to_sample_mut);
    declare_box_methods!(SampleToChunkBox, sample_to_chunk, sample_to_chunk_mut);
    declare_box_methods_opt!(CompositionOffsetBox, composition_time_to_sample, composition_time_to_sample_mut);
    declare_box_methods_opt!(SyncSampleBox, sync_samples, sync_samples_mut);

    /// Get a reference to the ChunkOffsetBox or ChunkLargeOffsetBox
    pub fn chunk_offset(&self) -> &ChunkOffsetBox {
        match first_box!(&self.boxes, ChunkOffsetBox) {
            Some(co) => Some(co),
            None => first_box!(&self.boxes, ChunkLargeOffsetBox),
        }.unwrap()
    }

    /// Get a mutable reference to the ChunkOffsetBox or ChunkLargeOffsetBox
    pub fn chunk_offset_mut(&mut self) -> &mut ChunkOffsetBox {
        if first_box!(&self.boxes, ChunkOffsetBox).is_some() {
            return first_box_mut!(&mut self.boxes, ChunkOffsetBox).unwrap();
        }
        first_box_mut!(&mut self.boxes, ChunkLargeOffsetBox).unwrap()
    }

    /// Move chunk offsets up.
    pub fn move_chunk_offsets_up(&mut self, delta: u64) {
        // Increment in-place.
        self.chunk_offset_mut().entries.iter_mut().for_each(|o| *o += delta);
        self.chunk_offset_mut().check_sizes();
    }

    /// Check if this SampleTableBox is valid (has stsd, stts, stsc, stco boxes).
    pub fn is_valid(&self) -> bool {
        let mut valid = true;
        if first_box!(&self.boxes, SampleDescriptionBox).is_none() {
            log::error!("SampleTableBox: no SampleDescriptionBox present");
            valid = false;
        }
        if first_box!(&self.boxes, TimeToSampleBox).is_none() {
            log::error!("SampleTableBox: no TimeToSampleBox present");
            valid = false;
        }
        if first_box!(&self.boxes, SampleToChunkBox).is_none() {
            log::error!("SampleTableBox: no SampleDescriptionBox present");
            valid = false;
        }
        if first_box!(&self.boxes, ChunkOffsetBox).is_none() {
            log::error!("SampleTableBox: no ChunkOffsetBox present");
            valid = false;
        }
        valid
    }
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

impl SampleTableBox {
    /// Return an iterator that iterates over every sample.
    pub fn time_to_sample_iter(&self) -> TimeToSampleIterator {
        let mut iter = TimeToSampleIterator {
            entries: &self.time_to_sample().entries,
            entry: TimeToSampleEntry::default(),
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

impl SampleTableBox {
    /// Return an iterator that iterates over every sample.
    pub fn composition_time_to_sample_iter(&self) -> CompositionOffsetIterator {
        match self.composition_time_to_sample() {
            Some(e) => {
                let mut iter = CompositionOffsetIterator {
                    entries: &e.entries,
                    entry: CompositionOffsetEntry::default(),
                    index: 0,
                };
                if iter.entries.len() > 0 {
                    iter.entry = iter.entries[0].clone();
                }
                iter
            },
            None => {
                let entry = CompositionOffsetEntry {
                    count:  self.sample_size().entries.len() as u32,
                    offset: 0,
                };
                CompositionOffsetIterator {
                    entries: &[],
                    entry,
                    index: 0,
                }
            },
        }
    }
}

/// Iterator over the SampleToChunk table.
pub struct SampleToChunkIterator<'a> {
    entries:    &'a [SampleToChunkEntry],
    index:      usize,
    count:      u32,
    chunk:      u32,
    sdi:        u32,
    left:       u32,
}

/// Value returned by SampleToChunkIterator.
///
/// Note that the `chunk' and `sample_description_index' values
/// here are 0-based, *not* 1-based as in the spec.
pub struct SampleToChunkIterEntry {
    pub chunk:  u32,
    pub sample_description_index: u32,
}

impl<'a> Iterator for SampleToChunkIterator<'a> {
    type Item = SampleToChunkIterEntry;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.left == 0 {
                return None;
            }
            if self.count > 0 {
                self.count -= 1;
                self.left -= 1;
                return Some(SampleToChunkIterEntry {
                    chunk: self.chunk,
                    sample_description_index: self.sdi,
                });
            }

            self.chunk += 1;
            self.count = self.entries[self.index].samples_per_chunk;

            let next_index = self.index + 1;
            if next_index < self.entries.len() &&
               self.entries[next_index].first_chunk == self.chunk + 1 {
                self.index += 1;
                self.sdi = self.entries[self.index].sample_description_index.saturating_sub(1);
                self.count = self.entries[self.index].samples_per_chunk;
            }
        }
    }
}

impl SampleTableBox {

    /// Return an iterator that iterates over every sample.
    pub fn sample_to_chunk_iter(&self) -> SampleToChunkIterator {
        let num_samples = self.sample_size().entries.len() as u32;
        let entries = &self.sample_to_chunk().entries;
        if entries.len() == 0 {
            // This should never happen, but code defensively.
            SampleToChunkIterator {
                entries,
                index: 0,
                count: 1,
                chunk: 0,
                sdi: 0,
                left: num_samples,
            }
        } else {
            SampleToChunkIterator {
                entries,
                index: 0,
                count: entries[0].samples_per_chunk,
                chunk: entries[0].first_chunk.saturating_sub(1),
                sdi: entries[0].sample_description_index.saturating_sub(1),
                left: num_samples,
            }
        }
    }
}

