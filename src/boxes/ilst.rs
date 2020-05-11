use std::fmt::Debug;
use std::io;

use crate::boxes::prelude::*;
use crate::boxes::{DataRef, AppleItem};

def_box! {
    /// Apple Item List.
    AppleItemListBox, "ilst",
        items:  [AppleItem],
}

/// Apple item.
#[derive(Debug)]
pub struct IDataBox {
    pub flags:  u32,
    pub data:   AppleData,
}

#[derive(Debug)]
pub enum AppleData {
    Text(String),
    Binary(Data),
    Extern(DataRef),
}

impl FromBytes for IDataBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
        let mut reader = BoxReader::new(stream)?;
        let stream = &mut reader;

        stream.skip(4)?;
        let flags = stream.flags();
        let size = stream.left();

        // If it's too big, don't read it into memory.
        if size > 32768 {
            let data = DataRef::from_bytes(stream)?;
            return Ok(IDataBox {
                flags,
                data: AppleData::Extern(data),
            });
        }

        let rawdata = stream.read(size)?.to_vec();
        let data = if flags == 1 {
            match String::from_utf8(rawdata) {
                Ok(text) => AppleData::Text(text),
                Err(e) => AppleData::Binary(Data(e.into_bytes())),
            }
        } else {
            AppleData::Binary(Data(rawdata))
        };

        Ok(IDataBox {
            flags,
            data,
        })
    }

    fn min_size() -> usize {
        16
    }
}

impl ToBytes for IDataBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self)?;
        let stream = &mut writer;

        0u32.to_bytes(stream)?;
        match &self.data {
            &AppleData::Text(ref s) => stream.write(s.as_bytes())?,
            &AppleData::Binary(ref b) => b.to_bytes(stream)?,
            &AppleData::Extern(ref e) => e.to_bytes(stream)?,
        }

        stream.finalize()
    }
}

impl FullBox for IDataBox {
    fn version(&self) -> Option<u8> {
        Some(0)
    }
    fn flags(&self) -> u32 {
        self.flags
    }
}

/*
impl Debug for IDataBox {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.data {
            &AppleData::Text(ref x) => Debug::fmt(x, f),
            &AppleData::Binary(ref x) => Debug::fmt(x, f),
            &AppleData::Extern(ref x) => Debug::fmt(x, f),
        }
    }
}*/
