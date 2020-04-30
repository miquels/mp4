use std::io;
use crate::fromtobytes::{FromBytes, ToBytes, ReadBytes, WriteBytes};
use crate::types::*;

#[derive(Debug)]
pub struct SampleSizeBox {
    version:        Version,
    flags:          Flags,
    sample_size:    u32,
    sample_count:   u32,
    sample_entries: Vec<u32>,
}

impl FromBytes for SampleSizeBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<SampleSizeBox> {
        let version = Version::from_bytes(stream)?;
        let flags = Flags::from_bytes(stream)?;
        let sample_size = u32::from_bytes(stream)?;
        let sample_count = u32::from_bytes(stream)?;
        let mut sample_entries = Vec::new();
        if sample_size == 0 {
            while sample_entries.len() < sample_count as usize {
                sample_entries.push(u32::from_bytes(stream)?);
            }
        }
        Ok(SampleSizeBox {
            version,
            flags,
            sample_size,
            sample_count,
            sample_entries,
        })
    }

    fn min_size() -> usize { 16 }
}

impl ToBytes for SampleSizeBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        self.version.to_bytes(stream)?;
        self.flags.to_bytes(stream)?;
        self.sample_size.to_bytes(stream)?;
        (self.sample_entries.len() as u32).to_bytes(stream)?;
        for e in &self.sample_entries {
            (*e).to_bytes(stream)?;
        }
        Ok(())
    }
}

