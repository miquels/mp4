use std::io;

use crate::boxes::prelude::*;
use crate::boxes::{SampleDescriptionBox, SampleSizeBox, TimeToSampleBox, SampleToChunkBox};
use crate::boxes::{ChunkOffsetBox, ChunkLargeOffsetBox};
use crate::boxes::{CompositionOffsetBox, SyncSampleBox};

def_box! {
    /// 8.1.1 Sample Table Box (ISO/IEC 14496-12:2015(E))
    ///
    /// It usually contains:
    ///
    /// - TimeToSampleBox, stts
    /// - CompositionOffsetBox. ctts
    /// - SampleDescriptionBox, stsd
    /// - SampleSizeBox, stsz, or CompactSampleSizeBox, stz2
    /// - SampleToChunkBox, stsc
    /// - ChunkOffsetBox, stco, or ChunkLargeOffsetBox, co64
    /// 
    /// Optionally:
    ///
    /// - SyncSampleBox, stss
    /// - SampleToGroupBox, sbgp
    /// - SampleGroupDescriptionBox, sgpd (minimal support)
    ///
    /// We don't implement:
    ///
    /// - CompositionToDecodeBox, cslg
    /// - ShadowSyncBox, stsh
    /// - DegrationPriorityBox, stdp
    /// - SamplePaddingBitsBox, padb
    /// - SampleDependencyTypeBox, sdtp
    /// - SubSampleInformationBox, subs
    /// - SampleAuxiliaryInformationSizesBox, saiz
    /// - SampleAuxiliaryInformationOffsetsBox, saio
    ///
    SampleTableBox {
        boxes:      Vec<MP4Box>,
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
