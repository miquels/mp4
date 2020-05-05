use std::io;
use crate::serialize::{FromBytes, ToBytes, ReadBytes, WriteBytes};
use crate::mp4box::BoxReader;

/// 8.7.3.3 Compact Sample Size Box (ISO/IEC 14496-12:2015(E))
#[derive(Debug)]
pub struct CompactSampleSizeBox {
    // skip:        3.
    field_size:     u8,
    sample_count:   u32,
    sample_entries: Vec<u16>,
}

impl FromBytes for CompactSampleSizeBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<CompactSampleSizeBox> {
        let mut reader = BoxReader::new(stream)?;
        let stream = &mut reader;

        stream.skip(3)?;
        let field_size = u8::from_bytes(stream)?;
        let sample_count = u32::from_bytes(stream)?;
        let mut sample_entries = Vec::new();
        while sample_entries.len() < sample_count as usize {
            if field_size == 4 {
                let b = u8::from_bytes(stream)?;
                let hi = (b & 0xf0) >> 4;
                let lo = b & 0x0f;
                sample_entries.push(hi as u16);
                if sample_entries.len() < sample_count as usize {
                    sample_entries.push(lo as u16);
                }
            }
            if field_size == 8 {
                sample_entries.push(u8::from_bytes(stream)? as u16);
            }
            if field_size == 16 {
                sample_entries.push(u16::from_bytes(stream)?);
            }
        }
        Ok(CompactSampleSizeBox {
            field_size,
            sample_count,
            sample_entries,
        })
    }

    fn min_size() -> usize { 16 }
}

impl ToBytes for CompactSampleSizeBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        (self.field_size as u32).to_bytes(stream)?;
        (self.sample_entries.len() as u32).to_bytes(stream)?;
        let mut i = 0;
        while i < self.sample_entries.len() {
            match self.field_size {
                4 => {
                    let mut b: u8 = ((self.sample_entries[i] & 0xf) as u8) << 4;
                    i += 1;
                    if i < self.sample_entries.len() {
                        b |= (self.sample_entries[i] & 0xf) as u8;
                        i += 1;
                    }
                    b.to_bytes(stream)?;
                },
                8 => {
                    let b: u8 = (self.sample_entries[i] & 0xff) as u8;
                    i += 1;
                    b.to_bytes(stream)?;
                },
                16 => {
                    let b = self.sample_entries[i];
                    i += 1;
                    b.to_bytes(stream)?;
                },
                _ => break,
            }
        }
        Ok(())
    }
}

