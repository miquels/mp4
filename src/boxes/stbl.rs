use std::io;

use crate::boxes::prelude::*;
use crate::boxes::{SampleDescriptionBox, SampleSizeBox, TimeToSampleBox, SampleToChunkBox};
use crate::boxes::ChunkOffsetBox;
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
    #[derive(Default)]
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
    declare_box_methods!(ChunkOffsetBox, chunk_offset_table, chunk_offset_table_mut);
    declare_box_methods_opt!(CompositionOffsetBox, composition_time_to_sample, composition_time_to_sample_mut);
    declare_box_methods_opt!(SyncSampleBox, sync_samples, sync_samples_mut);

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

