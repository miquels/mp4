use std::io;
use crate::fromtobytes::{FromBytes, ToBytes, ReadBytes, WriteBytes};
use crate::types::*;

#[derive(Debug)]
pub struct SampleToGroupBox {
    version:        Version,
    flags:          Flags,
    grouping_type:  u32,
    grouping_type_parameter:    Option<u32>,
    entry_count:    u32,
    entries:        Vec<SampleToGroupEntry>,
}

impl FromBytes for SampleToGroupBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<SampleToGroupBox> {
        let version = Version::from_bytes(stream)?;
        let flags = Flags::from_bytes(stream)?;
        let grouping_type = u32::from_bytes(stream)?;
        let grouping_type_parameter = if stream.version() == 1 {
            Some(u32::from_bytes(stream)?)
        } else {
            None
        };
        let entry_count = u32::from_bytes(stream)?;
        let mut entries = Vec::new();
        while entries.len() < entry_count as usize {
            entries.push(SampleToGroupEntry::from_bytes(stream)?);
        }
        Ok(SampleToGroupBox {
            version,
            flags,
            grouping_type,
            grouping_type_parameter,
            entry_count,
            entries,
        })
    }

    fn min_size() -> usize { 16 }
}

impl ToBytes for SampleToGroupBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        self.version.to_bytes(stream)?;
        self.flags.to_bytes(stream)?;
        self.grouping_type.to_bytes(stream)?;
        if let Some(param) = self.grouping_type_parameter {
            stream.set_version(1);
            param.to_bytes(stream)?;
        }
        (self.entries.len() as u32).to_bytes(stream)?;
        for e in &self.entries {
            e.to_bytes(stream)?;
        }
        Ok(())
    }
}

