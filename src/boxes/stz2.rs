use std::io;
use crate::boxes::prelude::*;

def_box! {
    /// 8.7.3.3 Compact Sample Size Box (ISO/IEC 14496-12:2015(E))
    CompactSampleSizeBox {
        // skip:        3.
        field_size:     u8,
        count:   u32,
        entries: {Vec<u16>},
    },
    fourcc => "stz2",
    version => [0],
    impls => [ boxinfo, debug, fullbox ],
}

impl FromBytes for CompactSampleSizeBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<CompactSampleSizeBox> {
        let mut reader = BoxReader::new(stream)?;
        let stream = &mut reader;

        stream.skip(3)?;
        let field_size = u8::from_bytes(stream)?;
        let count = u32::from_bytes(stream)?;
        let mut entries = Vec::new();
        while entries.len() < count as usize {
            if field_size == 4 {
                let b = u8::from_bytes(stream)?;
                let hi = (b & 0xf0) >> 4;
                let lo = b & 0x0f;
                entries.push(hi as u16);
                if entries.len() < count as usize {
                    entries.push(lo as u16);
                }
            }
            if field_size == 8 {
                entries.push(u8::from_bytes(stream)? as u16);
            }
            if field_size == 16 {
                entries.push(u16::from_bytes(stream)?);
            }
        }
        Ok(CompactSampleSizeBox {
            field_size,
            count,
            entries,
        })
    }

    fn min_size() -> usize { 16 }
}

impl ToBytes for CompactSampleSizeBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self)?;
        let stream = &mut writer;

        (self.field_size as u32).to_bytes(stream)?;
        (self.entries.len() as u32).to_bytes(stream)?;
        let mut i = 0;
        while i < self.entries.len() {
            match self.field_size {
                4 => {
                    let mut b: u8 = ((self.entries[i] & 0xf) as u8) << 4;
                    i += 1;
                    if i < self.entries.len() {
                        b |= (self.entries[i] & 0xf) as u8;
                        i += 1;
                    }
                    b.to_bytes(stream)?;
                },
                8 => {
                    let b: u8 = (self.entries[i] & 0xff) as u8;
                    i += 1;
                    b.to_bytes(stream)?;
                },
                16 => {
                    let b = self.entries[i];
                    i += 1;
                    b.to_bytes(stream)?;
                },
                _ => break,
            }
        }

        stream.finalize()
    }
}

