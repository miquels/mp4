use std::io;

use crate::boxes::prelude::*;

def_box! {
    /// 8.7.3.2 Sample Size Box (ISO/IEC 14496-12:2015(E))
    #[derive(Default)]
    SampleSizeBox {
        // Default size (if size > 0 && entries.len() == 0)
        size:    u32,
        // Number of samples.
        count:   u32,
        // Size of each sample (if not default).
        entries: ArraySized32<u32>,
    },
    fourcc => "stsz",
    version => [0],
    impls => [ boxinfo, debug, fullbox ],
}

impl SampleSizeBox {
    pub fn iter(&self) -> SampleSizeIterator<'_> {
        SampleSizeIterator::new(&self)
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
            entries = ArraySized32::from_bytes(stream)?;
            count = entries.len() as u32;
        } else {
            entries = ArraySized32::default();
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

/// Iterator over the sizes of the samples.
pub struct SampleSizeIterator<'a> {
    entries:    &'a [u32],
    index:      usize,
    sample_size: u32,
    sample_count: u32,
}

impl<'a> SampleSizeIterator<'a> {
    fn new(sbox: &'a SampleSizeBox) -> SampleSizeIterator {
        SampleSizeIterator {
            entries: &sbox.entries[..],
            index: 0,
            sample_size: sbox.size,
            sample_count: sbox.count,
        }
    }

    /// Seek to a sample.
    ///
    /// Sample indices start at `1`.
    pub fn seek(&mut self, seek_to: u32) -> io::Result<()> {
        if seek_to > self.sample_count {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.index = seek_to.saturating_sub(1) as usize;
        Ok(())
    }

    /// Add up the sizes of all the samples in the range.
    ///
    /// `from_sample` and `to_sample` are one-based, and the range is inclusive.
    pub fn add_sizes(&self, from_sample: u32, to_sample: u32) -> u64 {
        let start = std::cmp::min(self.entries.len(), from_sample.saturating_sub(1) as usize);
        let end = std::cmp::min(self.entries.len(), to_sample.saturating_sub(1) as usize);
        if self.sample_size > 0 {
            return (end - start + 1) as u64 * (self.sample_size as u64);
        }
        let mut totsz = 0;
        for index in start .. end + 1 {
            totsz += self.entries[index] as u64;
        }
        totsz
    }
}

impl<'a> Iterator for SampleSizeIterator<'a> {
    type Item = u32;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.sample_count as usize {
            return None;
        }
        let idx = self.index;
        self.index += 1;
        if self.sample_size > 0 {
            Some(self.sample_size)
        } else {
            Some(self.entries[idx])
        }
    }
}
