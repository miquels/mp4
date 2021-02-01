use std::io;

use crate::boxes::prelude::*;
use crate::io::DataRef;

def_box! {
    /// 8.1.1 Media Data Box (ISO/IEC 14496-12:2015(E))
    MediaDataBox {
        data:   DataRef,
    },
    fourcc => "mdat",
    version => [],
    impls => [ basebox, boxinfo, debug ],
}

impl FromBytes for MediaDataBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<MediaDataBox> {
        let mut reader = BoxReader::new(stream)?;
        let size = reader.left();
        let data = DataRef::from_bytes(&mut reader, size)?;
        Ok(MediaDataBox{ data })
    }
    fn min_size() -> usize { 8 }
}

impl ToBytes for MediaDataBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {

        // First write a header.
        let fourcc = FourCC::new("mdat");
        let data = &self.data;
        let mut box_size = data.len() + 8;
        let is_large = data.is_large();
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

