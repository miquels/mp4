use std::fmt::Debug;
use std::io;

use crate::boxes::prelude::*;
use crate::io::DataRef;
use crate::mp4box::{BoxHeader, GenericBox};

def_box! {
    /// Apple Item List.
    AppleItemListBox {
        items:  Vec<AppleItem>,
    },
    fourcc => "ilst",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

macro_rules! apple_name_item {
    ($box_type:ident, $box_name:ident, $box_fourcc:expr) => {
        /// Apple item $box_name tag.
        #[derive(Clone, Debug)]
        pub struct $box_type {
            pub $box_name:   String,
        }

        impl FromBytes for $box_type {
            fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
                let mut header = BoxHeader::read_base(stream)?;
                BoxHeader::read_full(stream, &mut header)?;

                let size = header.size;

                let rawdata = if size == 0 {
                    Vec::new()
                } else {
                    stream.read(size)?.to_vec()
                };
                let data = match String::from_utf8(rawdata) {
                        Ok(text) => text,
                        Err(_) => "[non-utf8]".to_string(),
                };

                Ok($box_type { $box_name: data })
            }

            fn min_size() -> usize {
                12
            }
        }

        impl ToBytes for $box_type {
            fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
                let mut writer = BoxWriter::new(stream, self)?;
                let stream = &mut writer;

                stream.write(self.$box_name.as_bytes())?;
                stream.finalize()
            }
        }

        impl BoxInfo for $box_type {
            #[inline]
            fn fourcc(&self) -> FourCC {
                FourCC(u32::from_be_bytes(*$box_fourcc))
            }
            fn max_version() -> Option<u8> {
                Some(0)
            }
        }

        impl FullBox for $box_type {
            fn version(&self) -> Option<u8> {
                Some(0)
            }
            fn flags(&self) -> u32 {
                0
            }
        }
    }
}

apple_name_item!(IMeanBox, mean, b"mean");
apple_name_item!(INameBox, name, b"name");

/// Apple item.
#[derive(Clone, Debug)]
pub struct AppleItem {
    tag:    FourCC,
    name:   Option<INameBox>,
    mean:   Option<IMeanBox>,
    data:   Option<IDataBox>,
    boxes:  Vec<GenericBox>,
}

impl FromBytes for AppleItem {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
        let mut reader = BoxReader::new(stream)?;
        let tag = reader.header.fourcc.clone();
        let stream = &mut reader;

        let mut name: Option<INameBox> = None;
        let mut mean: Option<IMeanBox> = None;
        let mut data: Option<IDataBox> = None;
        let mut boxes = Vec::new();

        while stream.left() > 0 {
            let pos = stream.pos();
            let header = BoxHeader::read_base(stream)?;
            stream.seek(pos)?;
            let b = header.fourcc.to_be_bytes();
            match &b {
                b"name" => name = Some(INameBox::from_bytes(stream)?),
                b"mean" =>mean = Some(IMeanBox::from_bytes(stream)?),
                b"data" => data = Some(IDataBox::from_bytes(stream)?),
                _ => boxes.push(GenericBox::from_bytes(stream)?),
            }
        }
        Ok(AppleItem {
            tag,
            name,
            mean,
            data,
            boxes,
        })

    }

    fn min_size() -> usize { 12 }
}

impl ToBytes for AppleItem {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self)?;
        let stream = &mut writer;

        if let Some(ref name) = self.name {
            name.to_bytes(stream)?;
        }
        if let Some(ref mean) = self.mean {
            mean.to_bytes(stream)?;
        }
        if let Some(ref data) = self.data {
            data.to_bytes(stream)?;
        }
        writer.finalize()
    }
}

impl BoxInfo for AppleItem {
    #[inline]
    fn fourcc(&self) -> FourCC {
        self.tag.clone()
    }
}

impl FullBox for AppleItem {}

/// Apple item data.
#[derive(Clone, Debug)]
pub struct IDataBox {
    pub flags:  u32,
    pub data:   AppleData,
}

#[derive(Clone, Debug)]
pub enum AppleData {
    Text(String),
    Binary(Data),
    Extern(DataRef),
}

impl FromBytes for IDataBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
        let mut header = BoxHeader::read_base(stream)?;
        BoxHeader::read_full(stream, &mut header)?;

        stream.skip(4)?;
        let flags = header.flags;
        let size = header.size - 4;

        // If it's too big, don't read it into memory.
        if size > 32768 {
            let data = DataRef::from_bytes_limit(stream, size)?;
            return Ok(IDataBox {
                flags,
                data: AppleData::Extern(data),
            });
        }

        let rawdata = if size == 0 {
            Vec::new()
        } else {
            stream.read(size)?.to_vec()
        };
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
            &AppleData::Text(ref s) => {
                stream.write(s.as_bytes())?;
            },
            &AppleData::Binary(ref b) => {
                b.to_bytes(stream)?;
            },
            &AppleData::Extern(ref e) => {
                e.to_bytes(stream)?;
            },
        }

        stream.finalize()
    }
}

impl BoxInfo for IDataBox {
    #[inline]
    fn fourcc(&self) -> FourCC {
        FourCC(u32::from_be_bytes(*b"data"))
    }
    #[inline]
    fn max_version() -> Option<u8> {
        Some(0)
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
