use std::io;

use crate::boxes::prelude::*;

def_box! {
    /// 8.7.3.2 Sample Size Box (ISO/IEC 14496-12:2015(E))
    #[derive(Default)]
    SampleSizeBox {
        size:    u32,
        count:   u32,
        entries: ListSized32<u32>,
    },
    fourcc => "stsz",
    version => [0],
    impls => [ boxinfo, debug, fullbox ],
}

pub type SampleSizeIterator<'a> = ListIteratorCloned<'a, u32>;

impl SampleSizeBox {
    pub fn iter(&self) -> SampleSizeIterator<'_> {
        if self.entries.len() == 0 {
            self.entries.iter_repeat(self.size, self.count as usize)
        } else {
            self.entries.iter_cloned()
        }
    }
}

impl FromBytes for SampleSizeBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<SampleSizeBox> {
        let mut reader = BoxReader::new(stream)?;
        let stream = &mut reader;

        let size = u32::from_bytes(stream)?;

        let entries;
        let count;
        if size == 0 {
            entries = ListSized32::from_bytes(stream)?;
            count = entries.len() as u32;
        } else {
            entries = ListSized32::default();
            count = u32::from_bytes(stream)?;
        }
        log::trace!("SampleSizeBox: size {} count {}", size, count);

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

