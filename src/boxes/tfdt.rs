//
// ISO/IEC 14496-12:2015(E)
// 8.8.12 Track fragment decode time
//

use std::io;
use crate::fromtobytes::{FromBytes, ToBytes, ReadBytes, WriteBytes};
use crate::types::*;

#[derive(Debug)]
pub struct TrackFragmentBaseMediaDecodeTimeBox {
    version:        Version,
    flags:          Flags,
    base_media_decode_time: u64,
}

impl FromBytes for TrackFragmentBaseMediaDecodeTimeBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<TrackFragmentBaseMediaDecodeTimeBox> {
        let version = Version::from_bytes(stream)?;
        let flags = Flags::from_bytes(stream)?;
        let base_media_decode_time = if stream.version() == 0 {
            u32::from_bytes(stream)? as u64
        } else {
            u64::from_bytes(stream)? as u64
        };
        Ok(TrackFragmentBaseMediaDecodeTimeBox {
            version,
            flags,
            base_media_decode_time,
        })
    }

    fn min_size() -> usize { 20 }
}

impl ToBytes for TrackFragmentBaseMediaDecodeTimeBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        self.version.to_bytes(stream)?;
        self.flags.to_bytes(stream)?;
        if self.base_media_decode_time < 0x100000000 {
            stream.set_version(0);
            (self.base_media_decode_time as u32).to_bytes(stream)?;
        } else {
            stream.set_version(1);
            self.base_media_decode_time.to_bytes(stream)?;
        }
        Ok(())
    }
}

