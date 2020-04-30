use std::io;
use crate::fromtobytes::{FromBytes, ToBytes, ReadBytes, WriteBytes};
use crate::types::*;

/// 8.8.2 Movie Extends Header Box (ISO/IEC 14496-12:2015(E))
#[derive(Debug)]
pub struct MovieExtendsHeaderBox {
    version:            Version,
    flags:              Flags,
    fragment_duration:  u64,
}

impl FromBytes for MovieExtendsHeaderBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<MovieExtendsHeaderBox> {
        let version = Version::from_bytes(stream)?;
        let flags = Flags::from_bytes(stream)?;
        let fragment_duration = if stream.version() == 0 {
            u32::from_bytes(stream)? as u64
        } else {
            u64::from_bytes(stream)?
        };
        Ok(MovieExtendsHeaderBox {
            version,
            flags,
            fragment_duration,
        })
    }

    fn min_size() -> usize { 12 }
}

impl ToBytes for MovieExtendsHeaderBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        self.version.to_bytes(stream)?;
        self.flags.to_bytes(stream)?;
        if self.fragment_duration < 0x100000000 {
            (self.fragment_duration as u32).to_bytes(stream)?;
        } else {
            stream.set_version(1);
            self.fragment_duration.to_bytes(stream)?;
        }
        Ok(())
    }
}

