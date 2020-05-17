use std::fmt::Debug;
use std::io;

use crate::boxes::prelude::*;

/// 8.1.1 Media Data Box (ISO/IEC 14496-12:2015(E))
#[derive(Debug, Default)]
pub struct MediaDataBox {
    pub data:   DataRef
}

impl FromBytes for MediaDataBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<MediaDataBox> {
        let pos = stream.pos();
        let mut reader = BoxReader::new(stream)?;
        let is_large = reader.pos() - pos > 8;
        let size = reader.left();
        let mut data = DataRef::from_bytes(&mut reader, size)?;
        if is_large {
            data.is_large = is_large;
        }
        Ok(MediaDataBox{ data })
    }
    fn min_size() -> usize { 8 }
}

impl ToBytes for MediaDataBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {

        // First write a header.
        let fourcc = FourCC::new("mdat");
        let data = &self.data;
        let mut box_size = data.data_size + 8;
        let is_large = data.is_large || data.data_size > std::u32::MAX as u64;
        if is_large {
            box_size += 8;
            1u32.to_bytes(stream)?;
            fourcc.to_bytes(stream)?;
            box_size.to_bytes(stream)?;
        } else {
            (box_size as u32).to_bytes(stream)?;
            fourcc.to_bytes(stream)?;
        }
        self.data.to_bytes(stream)
    }
}


/// A reference to data in a different file.
#[derive(Debug, Default)]
pub struct DataRef {
    pub is_large: bool,
    pub data_pos: u64,
    pub data_size:  u64,
}

impl DataRef {
    pub fn from_bytes<R: ReadBytes>(stream: &mut R, data_size: u64) -> io::Result<DataRef> {
        let data_pos = stream.pos();
        let is_large = data_size + 32 > std::u32::MAX as u64;
        stream.skip(data_size)?;
        Ok(DataRef {
            is_large,
            data_pos,
            data_size,
        })
    }
}

impl ToBytes for DataRef {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {

        // Do we want to actually write the data?
        if stream.mdat_ref().is_none() {
            return stream.skip(self.data_size);
        }

        // copy the data.
        let bufsize = std::cmp::min(self.data_size, 128*1024);
        let mut buf = Vec::with_capacity(bufsize as usize);
        buf.resize(bufsize as usize, 0);

        let mut todo = self.data_size;
        let mut pos = self.data_pos;

        while todo > 0 {
            let sz = std::cmp::min(bufsize, todo) as usize;
            let n = stream.mdat_ref().unwrap().read_at(&mut buf[..sz], pos)?;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            stream.write(&buf[..n])?;
            todo -= n as u64;
            pos += n as u64;
        }
        Ok(())
    }
}

