use std::io;
use crate::serialize::{FromBytes, ToBytes, ReadBytes, WriteBytes};
use crate::types::*;
use crate::mp4box::{BoxReader, BoxWriter};

#[derive(Debug)]
pub struct SampleSizeBox {
    sample_size:    u32,
    sample_count:   u32,
    sample_entries: ArrayUnsized<u32>,
}

impl FromBytes for SampleSizeBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<SampleSizeBox> {
        let mut reader = BoxReader::new(stream)?;
        let stream = &mut reader;

        let sample_size = u32::from_bytes(stream)?;
        let sample_count = u32::from_bytes(stream)?;
        let mut sample_entries = ArrayUnsized::new();

        debug!("SampleSizeBox: sample_size {} sample_count {}", sample_size, sample_count);
        if sample_size == 0 {
            while sample_entries.len() < sample_count as usize  && stream.left() >= 4 {
                sample_entries.push(u32::from_bytes(stream)?);
            }
        }
        Ok(SampleSizeBox {
            sample_size,
            sample_count,
            sample_entries,
        })
    }

    fn min_size() -> usize { 8 }
}

impl ToBytes for SampleSizeBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self)?;
        let stream = &mut writer;

        self.sample_size.to_bytes(stream)?;
        (self.sample_entries.len() as u32).to_bytes(stream)?;
        for e in &self.sample_entries {
            e.to_bytes(stream)?;
        }

        stream.finalize()
    }
}

