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

