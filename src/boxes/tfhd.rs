//
// ISO/IEC 14496-12:2015(E)
// 8.8.7 Track Fragment Header Box
//

use std::io;
use crate::fromtobytes::{FromBytes, ToBytes, ReadBytes, WriteBytes};
use crate::types::*;

//  aligned(8) class TrackFragmentHeaderBox extends FullBox(‘tfhd’, 0, tf_flags){
//      unsigned int(32) track_ID;
//      // all the following are optional fields
//      unsigned int(64) base_data_offset;
//      unsigned int(32) sample_description_index;
//      unsigned int(32) default_sample_duration;
//      unsigned int(32) default_sample_size;
//      unsigned int(32) default_sample_flags
//  } 

#[derive(Debug)]
pub struct TrackFragmentHeaderBox {
    pub version:                    Version,
    pub flags:                      Flags,
    pub track_id:                   u32,
    pub duration_is_empty:          bool,
    pub default_base_is_moof:       bool,
    pub base_data_offset:           Option<u64>,
    pub sample_description_index:   Option<u32>,
    pub default_sample_duration:    Option<u32>,
    pub default_sample_size:        Option<u32>,
    pub default_sample_flags:       Option<SampleFlags>,
}

// as long as we don't have bool.then().
fn b_then<T>(flag: bool, closure: impl FnOnce() -> T) -> Option<T> {
    if flag {
        Some(closure())
    } else {
        None
    }
}

impl FromBytes for TrackFragmentHeaderBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<TrackFragmentHeaderBox> {
        let version = Version::from_bytes(stream)?;
        let flags = Flags::from_bytes(stream)?;

        let track_id = u32::from_bytes(stream)?;

        let duration_is_empty = (flags.0 & 0x010000) > 0;
        let default_base_is_moof = (flags.0 & 0x020000) > 0;

        let base_data_offset = b_then((flags.0 & 0x01) > 0, || u64::from_bytes(stream)).transpose()?;
        let sample_description_index = b_then((flags.0 & 0x02) > 0, || u32::from_bytes(stream)).transpose()?;
        let default_sample_duration = b_then((flags.0 & 0x08) > 0, || u32::from_bytes(stream)).transpose()?;
        let default_sample_size = b_then((flags.0 & 0x10) > 0, || u32::from_bytes(stream)).transpose()?;
        let default_sample_flags = b_then((flags.0 & 0x20) > 0, || SampleFlags::from_bytes(stream)).transpose()?;

        Ok(TrackFragmentHeaderBox {
            version,
            flags,
            track_id,
            duration_is_empty,
            default_base_is_moof,
            base_data_offset,
            sample_description_index,
            default_sample_duration,
            default_sample_size,
            default_sample_flags,
        })
    }

    fn min_size() -> usize { 16 }
}

impl ToBytes for TrackFragmentHeaderBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        self.version.to_bytes(stream)?;
        let flags: u32 =
            self.base_data_offset.is_some() as u32 * 0x01 |
            self.sample_description_index.is_some() as u32 * 0x02 |
            self.default_sample_duration.is_some() as u32 * 0x08 |
            self.default_sample_size.is_some() as u32 * 0x10 |
            self.default_sample_flags.is_some() as u32 * 0x20 |
            self.duration_is_empty as u32 * 0x010000 |
            self.default_base_is_moof as u32 * 0x020000;
        Flags(flags).to_bytes(stream)?;

        self.base_data_offset.as_ref().map_or(Ok(()), |x| x.to_bytes(stream))?;
        self.sample_description_index.as_ref().map_or(Ok(()), |x| x.to_bytes(stream))?;
        self.default_sample_duration.as_ref().map_or(Ok(()), |x| x.to_bytes(stream))?;
        self.default_sample_size.as_ref().map_or(Ok(()), |x| x.to_bytes(stream))?;
        self.default_sample_flags.as_ref().map_or(Ok(()), |x| x.to_bytes(stream))?;

        Ok(())
    }
}

