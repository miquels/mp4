//
// ISO/IEC 14496-12:2015(E)
// 8.16.3 Segment Index Box
//

use std::io;
use crate::fromtobytes::{FromBytes, ToBytes, ReadBytes, WriteBytes};
use crate::types::*;

//  aligned(8) class TrackRunBox
//  extends FullBox(‘trun’, version, tr_flags) {
//      unsigned int(32) sample_count;
//      // the following are optional fields
//      signed int(32) data_offset;
//      unsigned int(32) first_sample_flags;
//      // all fields in the following array are optional
//      {
//          unsigned int(32) sample_duration;
//          unsigned int(32) sample_size;
//          unsigned int(32) sample_flags
//          if (version == 0)
//              { unsigned int(32) sample_composition_time_offset; }
//          else
//              { signed int(32) sample_composition_time_offset; }
//      }[ sample_count ]
//  }

/// 8.16.3 Segment Index Box (ISO/IEC 14496-12:2015(E))
#[derive(Debug)]
pub struct SegmentIndexBox {
    version:                    Version,
    flags:                      Flags,
    reference_id:               u32,
    timescale:                  u32,
    earliest_presentation_time: u64,
    first_offset:               u64,
    references:                 Vec<SegmentReference>,
}

impl FromBytes for SegmentIndexBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<SegmentIndexBox> {
        let version = Version::from_bytes(stream)?;
        let flags = Flags::from_bytes(stream)?;

        let reference_id = u32::from_bytes(stream)?;
        let timescale = u32::from_bytes(stream)?;

        let earliest_presentation_time;
        let first_offset;
        if stream.version() == 0 {
            earliest_presentation_time = u32::from_bytes(stream)? as u64;
            first_offset = u32::from_bytes(stream)? as u64;
        } else {
            earliest_presentation_time = u64::from_bytes(stream)?;
            first_offset = u64::from_bytes(stream)?;
        };
        stream.skip(2)?;

        let reference_count = u16::from_bytes(stream)? as usize;
        let mut references = Vec::new();
        while references.len() < reference_count {
            references.push(SegmentReference::from_bytes(stream)?);
        }

        Ok(SegmentIndexBox {
            version,
            flags,
            reference_id,
            timescale,
            earliest_presentation_time,
            first_offset,
            references,
        })
    }

    fn min_size() -> usize { 16 }
}

impl ToBytes for SegmentIndexBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        self.version.to_bytes(stream)?;
        self.flags.to_bytes(stream)?;

        self.reference_id.to_bytes(stream)?;
        self.timescale.to_bytes(stream)?;

        if self.earliest_presentation_time < 0x100000000 && self.first_offset < 0x100000000 {
            (self.earliest_presentation_time as u32).to_bytes(stream)?;
            (self.first_offset as u32).to_bytes(stream)?;
        } else {
            self.earliest_presentation_time.to_bytes(stream)?;
            self.first_offset.to_bytes(stream)?;
            stream.set_version(1);
        }
        stream.skip(2)?;

        (self.references.len() as u16).to_bytes(stream)?;
        for r in &self.references {
            r.to_bytes(stream)?;
        }

        Ok(())
    }
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

