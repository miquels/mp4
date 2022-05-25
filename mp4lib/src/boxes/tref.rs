use std::io;

use crate::boxes::prelude::*;

def_box! {
    /// 8.3.3 Track Reference Box (ISO/IEC 14496-12:2015(E))
    TrackReferenceBox {
        reference_type: FourCC,
        track_ids: Vec<u32>,
    },
    fourcc => "tref",
    version => [],
    impls => [ basebox, boxinfo, debug ],
}

impl FromBytes for TrackReferenceBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
        let mut reader = BoxReader::new(stream)?;
        let stream = &mut reader;
        let mut size = u32::from_bytes(stream)?;
        let reference_type = FourCC::from_bytes(stream)?;
        let mut track_ids = Vec::new();
        while size >= 12 {
            track_ids.push(u32::from_bytes(stream)?);
            size -= 4;
        }
        Ok(TrackReferenceBox {
            reference_type,
            track_ids,
        })
    }

    fn min_size() -> usize {
        8
    }
}

impl ToBytes for TrackReferenceBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self)?;
        let stream = &mut writer;
        let size: u32 = 8 + 4 * (self.track_ids.len() as u32);
        size.to_bytes(stream)?;
        self.reference_type.to_bytes(stream)?;
        self.track_ids.to_bytes(stream)?;
        Ok(())
    }
}
