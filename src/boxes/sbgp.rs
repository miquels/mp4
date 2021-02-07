use std::io;

use crate::boxes::prelude::*;

def_box! {
    SampleToGroupBox {
        grouping_type:  FourCC,
        grouping_type_parameter: Option<u32>,
        entries:        ArraySized32<SampleToGroupEntry>,
    },
    fourcc => "sbgp",
    version => [1],
    impls => [ boxinfo, debug ],
}

impl FromBytes for SampleToGroupBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<SampleToGroupBox> {
        let mut reader = BoxReader::new(stream)?;
        let stream = &mut reader;
        let grouping_type = FourCC::from_bytes(stream)?;
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
        let mut writer = BoxWriter::new(stream, self)?;
        let stream = &mut writer;

        self.grouping_type.to_bytes(stream)?;
        if let Some(param) = self.grouping_type_parameter {
            param.to_bytes(stream)?;
        }
        self.entries.to_bytes(stream)?;

        stream.finalize()
    }
}

impl FullBox for SampleToGroupBox {
    fn version(&self) -> Option<u8> {
        if self.grouping_type_parameter.is_some() {
            Some(1)
        } else {
            Some(0)
        }
    }
}

def_struct! {
    /// Entry in SampleToGroupBox.
    SampleToGroupEntry,
        sample_count:               u32,
        group_description_index:    u32,
}

