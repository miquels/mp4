//
// ISO/IEC 14496-12:2015(E)
// 8.16.3 Segment Index Box
//

use std::io;
use crate::boxes::prelude::*;

def_box! {
    /// 8.16.3 Segment Index Box (ISO/IEC 14496-12:2015(E))
    SegmentIndexBox {
        reference_id:               u32,
        timescale:                  u32,
        earliest_presentation_time: VersionSizedUint,
        first_offset:               VersionSizedUint,
        skip:                       2,
        references:                 [SegmentReference, sized16],
    },
    fourcc => "sidx",
    version => [1, earliest_presentation_time, first_offset],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

/// 8.16.3 Segment Index Box, Segment Reference struct. (ISO/IEC 14496-12:2015(E))
#[derive(Debug)]
pub struct SegmentReference {
    pub reference_type: u8,
    pub referenced_size: u32,
    pub subsegment_duration: u32,
    pub starts_with_sap: bool,
    pub sap_type: u8,
    pub sap_delta_time: u32,
}

impl FromBytes for SegmentReference {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<SegmentReference> {
        let b1 = u32::from_bytes(stream)?;
        let b2 = u32::from_bytes(stream)?;
        let b3 = u32::from_bytes(stream)?;

        Ok(SegmentReference{
            reference_type: ((b1 & 0x80000000) >> 31) as u8,
            referenced_size: b1 &  0x7fffffff,
            subsegment_duration: b2,
            starts_with_sap: (b3 & 0x80000000) > 0,
            sap_type: ((b3 &       0x70000000) >> 28) as u8,
            sap_delta_time: (b3 &  0x0fffffff),
        })
    }

    fn min_size() -> usize { 12 }
}

impl ToBytes for SegmentReference {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let b1 = (((self.reference_type & 0x01) as u32) << 31) | (self.referenced_size & 0x7fffffff);
        let b2 = self.subsegment_duration;
        let b3 = ((self.starts_with_sap as u32) << 31) | (((self.sap_type & 0x7) as u32) << 28) | (self.sap_delta_time & 0x0fffffff);

        b1.to_bytes(stream)?;
        b2.to_bytes(stream)?;
        b3.to_bytes(stream)?;

        Ok(())
    }
}

