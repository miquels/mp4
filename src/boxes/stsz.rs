use std::convert::TryInto;
use std::io;

use crate::boxes::prelude::*;
use crate::io::DataRef;

def_box! {
    /// 8.7.3.2 Sample Size Box (ISO/IEC 14496-12:2015(E))
    #[derive(Default)]
    SampleSizeBox {
        size:    u32,
        count:   u32,
        entries: DataRef,
    },
    fourcc => "stsz",
    version => [0],
    impls => [ boxinfo, debug, fullbox ],
}

impl SampleSizeBox {
    pub fn iter(&self) -> SampleSizeIterator<'_> {
        SampleSizeIterator {
            size:   self.size,
            count:  self.count,
            entries: &self.entries[..],
            index: 0,
        }
    }
}

pub struct SampleSizeIterator<'a> {
    size:       u32,
    count:      u32,
    entries:    &'a [u8],
    index:      usize,
}

impl<'a> Iterator for SampleSizeIterator<'a> {
    type Item = u32;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.count as usize {
            return None;
        }
        if self.entries.len() == 0 {
            Some(self.size)
        } else {
            let idx = self.index * 4;
            let size = u32::from_be_bytes(self.entries[idx..idx+4].try_into().unwrap());
            self.index += 1;
            Some(size)
        }
    }
}

impl FromBytes for SampleSizeBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<SampleSizeBox> {
        let mut reader = BoxReader::new(stream)?;
        let stream = &mut reader;

        let size = u32::from_bytes(stream)?;
        let count = u32::from_bytes(stream)?;

        log::trace!("SampleSizeBox: size {} count {}", size, count);
        let entries_count = if size == 0 { count * 4 } else { 0 };
        let entries = DataRef::from_bytes(stream, entries_count as u64)?;

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
            ((self.entries.len() / 4) as u32).to_bytes(stream)?;
            self.entries.to_bytes(stream)?;
        }

        stream.finalize()
    }
}

