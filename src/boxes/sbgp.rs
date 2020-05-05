use std::io;
use crate::serialize::{FromBytes, ToBytes, ReadBytes, WriteBytes,BoxBytes};
use crate::types::*;
use crate::mp4box::{BoxReader, FullBox};

#[derive(Debug)]
pub struct SampleToGroupBox {
    grouping_type:  u32,
    grouping_type_parameter:    Option<u32>,
    entries:        ArraySized32<SampleToGroupEntry>,
}

impl FromBytes for SampleToGroupBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<SampleToGroupBox> {
        let mut reader = BoxReader::new(stream)?;
        let stream = &mut reader;
        let grouping_type = u32::from_bytes(stream)?;
        let grouping_type_parameter = if stream.version() == 1 {
            Some(u32::from_bytes(stream)?)
        } else {
            None
        };
        let entries = ArraySized32::<SampleToGroupEntry>::from_bytes(stream)?;
        Ok(SampleToGroupBox {
            grouping_type,
            grouping_type_parameter,
            entries,
        })
    }

    fn min_size() -> usize { 16 }
}

impl ToBytes for SampleToGroupBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        self.grouping_type.to_bytes(stream)?;
        if let Some(param) = self.grouping_type_parameter {
            param.to_bytes(stream)?;
        }
        self.entries.to_bytes(stream)?;
        Ok(())
    }
}

impl FullBox for SampleToGroupBox {
    fn version(&self) -> Option<u8> {
        self.grouping_type_parameter.as_ref().map(|_| 1)
    }
}

