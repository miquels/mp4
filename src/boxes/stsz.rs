use std::io;
use crate::boxes::prelude::*;

#[derive(Debug, Default)]
pub struct SampleSizeBox {
    pub size:    u32,
    pub count:   u32,
    pub entries: ArrayUnsized<u32>,
}

impl FromBytes for SampleSizeBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<SampleSizeBox> {
        let mut reader = BoxReader::new(stream)?;
        let stream = &mut reader;

        let size = u32::from_bytes(stream)?;
        let count = u32::from_bytes(stream)?;
        let mut entries = ArrayUnsized::new();

        debug!("SampleSizeBox: size {} count {}", size, count);
        if size == 0 {
            while entries.len() < count as usize  && stream.left() >= 4 {
                entries.push(u32::from_bytes(stream)?);
            }
        }
        Ok(SampleSizeBox {
            size,
            count,
            entries,
        })
    }

    fn min_size() -> usize { 8 }
}

impl ToBytes for SampleSizeBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self)?;
        let stream = &mut writer;

        self.size.to_bytes(stream)?;
        if self.size != 0 {
            self.count.to_bytes(stream)?;
        } else {
            (self.entries.len() as u32).to_bytes(stream)?;
            for e in &self.entries {
                e.to_bytes(stream)?;
            }
        }

        stream.finalize()
    }
}

