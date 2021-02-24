use std::io;

use crate::boxes::prelude::*;
use crate::boxes::{SampleDescriptionBox, SampleToGroupBox, SampleGroupDescriptionBox};
use crate::boxes::{SampleSizeBox, TimeToSampleBox, SampleToChunkBox};
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
    declare_box_methods_opt!(SampleToGroupBox, sample_to_group, sample_to_group_mut);
    declare_box_methods_opt!(SampleGroupDescriptionBox, sample_group_description, sample_group_description_mut);
    declare_box_methods_opt!(CompositionOffsetBox, composition_time_to_sample, composition_time_to_sample_mut);
    declare_box_methods_opt!(SyncSampleBox, sync_samples, sync_samples_mut);

    /// Get a reference to the ChunkOffsetBox or ChunkLargeOffsetBox.
    pub fn chunk_offset_table(&self) -> &ChunkOffsetBox {
        if let Some(stco) = first_box!(&self.boxes, ChunkOffsetBox) {
            return stco;
        }
        first_box!(&self.boxes, ChunkLargeOffsetBox).unwrap()
    }
    /// Get a mutable reference to the ChunkOffsetBox or ChunkLargeOffsetBox.
    pub fn chunk_offset_table_mut(&mut self) -> &mut ChunkOffsetBox {
        for box_ in &mut self.boxes {
            match box_ {
                &mut MP4Box::ChunkOffsetBox(ref mut stco) => return stco,
                &mut MP4Box::ChunkLargeOffsetBox(ref mut co64) => return co64,
                _ => {},
            }
        }
        unreachable!()
    }

    /// Check if this SampleTableBox is valid (has stsd, stts, stsc, stco boxes).
    pub fn is_valid(&self) -> bool {
        let mut valid = true;

        if let Some(box_) = first_box!(&self.boxes, SampleDescriptionBox) {
            if box_.entries.len() == 0 {
                log::error!("SampleTableBox: SampleDescriptionBox: no entries");
                valid = false;
            }
            // FIXME support more than one sample description per track.
            if box_.entries.len() != 1 {
                log::error!("SampleTableBox: SampleDescriptionBox: we only support one entry");
                valid = false;
            }
        } else {
            log::error!("SampleTableBox: no SampleDescriptionBox present");
            valid = false;
        }

        if let Some(box_) = first_box!(&self.boxes, TimeToSampleBox) {
            if box_.entries.len() == 0 {
                log::error!("SampleTableBox: TimeToSampleBox: no entries");
                valid = false;
            }
        } else {
            log::error!("SampleTableBox: no TimeToSampleBox present");
            valid = false;
        }

        if let Some(box_) = first_box!(&self.boxes, SampleToChunkBox) {
            if box_.entries.len() == 0 {
                log::error!("SampleTableBox: SampleToChunkBox: no entries");
                valid = false;
            }
        } else {
            log::error!("SampleTableBox: no SampleDescriptionBox present");
            valid = false;
        }

        if let Some(box_) = first_box!(&self.boxes, ChunkOffsetBox) {
            if box_.entries.len() == 0 {
                log::error!("SampleTableBox: ChunkOffsetBox: no entries");
                valid = false;
            }
        } else {
            log::error!("SampleTableBox: no ChunkOffsetBox present");
            valid = false;
        }

        if let Some(box_) = first_box!(&self.boxes, SampleSizeBox) {
            if box_.entries.len() == 0 {
                log::error!("SampleTableBox: SampleSizeBox: no entries");
                valid = false;
            }
        } else {
            log::error!("SampleTableBox: no SampleSizeBox present");
            valid = false;
        }

        if let Some(box_) = first_box!(&self.boxes, SyncSampleBox) {
            if box_.entries.len() == 0 {
                log::error!("SampleTableBox: SyncSampleBox: no entries");
                valid = false;
            }
        }

        valid
    }
}

