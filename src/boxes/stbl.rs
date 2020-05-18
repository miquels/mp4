use std::io;

use crate::boxes::prelude::*;
use crate::boxes::{SampleDescriptionBox, TimeToSampleBox, SampleToChunkBox, ChunkOffsetBox, ChunkLargeOffsetBox};
use crate::boxes::CompositionOffsetBox;

// stsd, stts, stsc, stco, co64
// ctts, cslg, stsz, stz2, stss, stsh, padb, stdp, sbgp, sgpd, subs, saiz, saio

def_box! {
    /// 8.4.4 Media Information Box (ISO/IEC 14496-12:2015(E))
    SampleTableBox, "minf",
        boxes:      [MP4Box],
}

/// Either &ChunkOffsetBox or &ChunkLargeOffsetBox.
pub enum ChunkOffset<'a> {
    CO32(&'a ChunkOffsetBox),
    CO64(&'a ChunkLargeOffsetBox),
}

/// Either &mut ChunkOffsetBox or &mut ChunkLargeOffsetBox.
pub enum ChunkOffsetMut<'a> {
    CO32(&'a mut ChunkOffsetBox),
    CO64(&'a mut ChunkLargeOffsetBox),
}

impl ChunkOffset<'_> {
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            &ChunkOffset::CO32(ref co) => co.entries.len(),
            &ChunkOffset::CO64(ref co) => co.entries.len(),
        }
    }

    /// Get value at an index.
    ///
    /// Unfortunately the Index trait does not work, since it returns a
    /// reference. We'd need to return &(entry as u64), but that results
    /// in "cannot return a reference to a temporary value".
    /// Thus, the discrete function.
    #[inline]
    pub fn index(&self, idx: usize) -> u64 {
        match self {
            &ChunkOffset::CO32(ref co) => co.entries[idx] as u64,
            &ChunkOffset::CO64(ref co) => co.entries[idx],
        }
    }
}

impl SampleTableBox {

    declare_box_methods!(SampleDescriptionBox, sample_description, sample_description_mut);
    declare_box_methods!(TimeToSampleBox, time_to_sample, time_to_sample_mut);
    declare_box_methods!(SampleToChunkBox, sample_to_chunk, sample_to_chunk_mut);
    declare_box_methods_opt!(CompositionOffsetBox, composition_time_to_sample, composition_time_to_sample_mut);

    /// Get a reference to the ChunkOffsetBox, either the 32 or 64 bit version.
    pub fn chunk_offset(&self) -> ChunkOffset {
        if let Some(co) = first_box!(&self.boxes, ChunkOffsetBox) {
            return ChunkOffset::CO32(co)
        }
        if let Some(co64) = first_box!(&self.boxes, ChunkLargeOffsetBox) {
            return ChunkOffset::CO64(co64)
        }
        unreachable!()
    }

    /// Get a mutable reference to the ChunkOffsetBox, either the 32 or 64 bit version.
    pub fn chunk_offset_mut<'a>(&'a mut self) -> ChunkOffsetMut {
        if first_box!(&self.boxes, ChunkOffsetBox).is_some() {
            return ChunkOffsetMut::<'a>::CO32(first_box_mut!(&mut self.boxes, ChunkOffsetBox).unwrap());
        }
        if first_box!(&self.boxes, ChunkLargeOffsetBox).is_some() {
            return ChunkOffsetMut::<'a>::CO64(first_box_mut!(&mut self.boxes, ChunkLargeOffsetBox).unwrap());
        }
        unreachable!()
    }

    /// Rewrite the stco table to a co64 table.
    pub fn move_chunk_offsets_up(&mut self, delta: u64) {
        match self.chunk_offset_mut() {
            ChunkOffsetMut::CO32(ref mut co) => {
                if co.entries.len() == 0 {
                    return;
                }
                if co.entries[0] as u64 + delta > 0xffffffff {
                    // We need to change to a 64 bit offset table.
                    let entries = co.entries.iter().map(|o| *o as u64).collect::<ArraySized32<_>>();
                    let idx = self.boxes.iter().enumerate().find_map(|(idx, b)| {
                        match b {
                            MP4Box::ChunkOffsetBox(_) => Some(idx),
                            _ => None,
                        }
                    }).unwrap();
                    self.boxes[idx] = MP4Box::ChunkLargeOffsetBox(ChunkLargeOffsetBox{ entries });
                    return;
                }
                // Increment in-place.
                let d32 = delta as u32;
                co.entries.iter_mut().for_each(|o| *o += d32);
            },
            ChunkOffsetMut::CO64(ref mut co) => {
                // Increment in-place.
                co.entries.iter_mut().for_each(|o| *o += delta);
            },
        }
    }

    /// Check if this SampleTableBox is valid (has stsd, stts, stsc, stco boxes).
    pub fn is_valid(&self) -> bool {
        let mut valid = true;
        if first_box!(&self.boxes, SampleDescriptionBox).is_none() {
            error!("SampleTableBox: no SampleDescriptionBox present");
            valid = false;
        }
        if first_box!(&self.boxes, TimeToSampleBox).is_none() {
            error!("SampleTableBox: no TimeToSampleBox present");
            valid = false;
        }
        if first_box!(&self.boxes, SampleToChunkBox).is_none() {
            error!("SampleTableBox: no SampleDescriptionBox present");
            valid = false;
        }
        if first_box!(&self.boxes, ChunkOffsetBox).is_none() &&
           first_box!(&self.boxes, ChunkLargeOffsetBox).is_none() {
            error!("SampleTableBox: no ChunkOffsetBox present");
            valid = false;
        }
        valid
    }
}

