use std::fmt;
use std::io;

use crate::boxes::prelude::*;
use crate::io::DataRef;

def_box! {
    /// 8.1.1 Media Data Box (ISO/IEC 14496-12:2015(E))
    #[derive(Default)]
    MediaDataBox {
        data:   MediaData,
    },
    fourcc => "mdat",
    version => [],
    impls => [ basebox, boxinfo, debug ],
}

/// Raw media data.
#[derive(Clone)]
pub struct MediaData(MediaData_, u64);

impl FromBytes for MediaDataBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<MediaDataBox> {
        let mut reader = BoxReader::new(stream)?;
        let size = reader.left();
        let offset = reader.pos();
        let data_ref = DataRef::from_bytes_limit(&mut reader, size)?;
        let data = MediaData(MediaData_::DataRef(data_ref), offset);
        Ok(MediaDataBox{ data })
    }
    fn min_size() -> usize { 8 }
}

impl ToBytes for MediaDataBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {

        // First write a header.
        let fourcc = FourCC::new("mdat");
        let mut box_size = self.data.len() + 8;
        let is_large = self.data.is_large();
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

#[derive(Clone)]
enum MediaData_ {
    DataRef(DataRef),
    Data(Vec<u8>),
}

impl MediaData {
    fn is_large(&self) -> bool {
        match &self.0 {
            MediaData_::DataRef(d) => d.is_large(),
            MediaData_::Data(d) => d.len() > (u32::MAX - 20) as usize,
        }
    }

    /// Length in bytes.
    pub fn len(&self) -> u64 {
        match &self.0 {
            MediaData_::DataRef(d) => d.len(),
            MediaData_::Data(d) => d.len() as u64,
        }
    }

    /// Offset of the inner data, relative to the start of the containing file.
    pub fn offset(&self) -> u64 {
        match &self.0 {
            MediaData_::DataRef(_) => self.1,
            MediaData_::Data(_) => {
                if self.is_large() {
                    16
                } else {
                    8
                }
            },
        }
    }

    /// Add data.
    pub fn push(&mut self, data: &[u8]) {
        match &mut self.0 {
            &mut MediaData_::DataRef(_) => panic!("cannot push onto MediaData_::DataRef"),
            &mut MediaData_::Data(ref mut d) => d.extend_from_slice(data),
        }
    }

    /// Resize.
    pub fn resize(&mut self, size: usize) {
        match &mut self.0 {
            &mut MediaData_::DataRef(_) => panic!("cannot push onto MediaData_::DataRef"),
            &mut MediaData_::Data(ref mut d) => d.resize(size, 0),
        }
    }

    /// Reference as bytes.
    pub fn bytes(&self) -> &[u8] {
        match &self.0 {
            MediaData_::DataRef(d) => d.bytes(),
            MediaData_::Data(d) => &d[..],
        }
    }

    /// Mutable reference as bytes.
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        match &mut self.0 {
            &mut MediaData_::DataRef(_) => panic!("cannot write to MediaData_::DataRef"),
            &mut MediaData_::Data(ref mut d) => &mut d[..],
        }
    }
}

impl Default for MediaData {
    fn default() -> MediaData {
        MediaData(MediaData_::Data(Vec::new()), 0)
    }
}

impl fmt::Debug for MediaData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.0 {
            MediaData_::DataRef(d) => d.fmt(f),
            MediaData_::Data(d) => d.fmt(f),
        }
    }
}

impl ToBytes for MediaData {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        match &self.0 {
            MediaData_::DataRef(d) => d.to_bytes(stream),
            MediaData_::Data(d) => stream.write(&d[..]),
        }
    }
}
