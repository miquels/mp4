//
// ISO/IEC 14496-12:2015(E)
// 8.9.3 Sample Group Description Box
//

use std::fmt::Debug;
use std::io;
use crate::serialize::{FromBytes, ToBytes, ReadBytes, WriteBytes, BoxBytes};
use crate::types::*;
use crate::mp4box::{BoxReader, BoxWriter, FullBox};

/// 8.9.3 Sample Group Description Box
#[derive(Debug)]
pub struct SampleGroupDescriptionBox {
    grouping_type:              FourCC,
    default_length:             Option<u32>,
    default_sample_description_index: Option<u32>,
    entries:                    ArrayUnsized<SampleGroupDescriptionItem>,
}

impl FromBytes for SampleGroupDescriptionBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<SampleGroupDescriptionBox> {
        let mut reader = BoxReader::new(stream)?;
        let stream = &mut reader;

        let version = stream.version();

        let grouping_type = FourCC::from_bytes(stream)?;
        let default_length = if version == 1 {
            Some(u32::from_bytes(stream)?)
        } else {
            None
        };
        let default_sample_description_index = if version >= 2 {
            Some(u32::from_bytes(stream)?)
        } else {
            None
        };

        let num_entries = u32::from_bytes(stream)? as usize;
        let mut entries = ArrayUnsized::new();
        while entries.len() < num_entries && stream.left() > 0 {
            let entry = SampleGroupDescriptionItem::from_bytes(stream, grouping_type.clone(), default_length)?;
            entries.push(entry);
        }

        Ok(SampleGroupDescriptionBox {
            grouping_type,
            default_length,
            default_sample_description_index,
            entries,
        })
    }

    fn min_size() -> usize { 8 }
}

impl ToBytes for SampleGroupDescriptionBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self)?;
        let stream = &mut writer;

        let version = stream.version();

        self.grouping_type.to_bytes(stream)?;
        if version == 1 {
            self.default_length.unwrap_or(0).to_bytes(stream)?;
        }
        if version >= 2 {
            self.default_sample_description_index.unwrap_or(0).to_bytes(stream)?;
        }

        (self.entries.len() as u32).to_bytes(stream)?;
        for e in &self.entries {
            e.to_bytes(stream, self.default_length.clone())?;
        }

        stream.finalize()
    }
}

impl FullBox for SampleGroupDescriptionBox {
    fn version(&self) -> Option<u8> {
        if self.default_sample_description_index.is_some() {
            return Some(2);
        }
        if self.default_length.is_some() {
            return Some(1);
        }
        Some(0)
    }
}

/// 8.9.3 Sample Group Description Box
#[derive(Debug)]
pub struct SampleGroupDescriptionItem {
    pub description_length: Option<u32>,
    pub entry: SampleGroupDescriptionEntry,
}

impl SampleGroupDescriptionItem {
    fn from_bytes<R: ReadBytes>(stream: &mut R, grouping_type: FourCC, default_length: Option<u32>) -> io::Result<SampleGroupDescriptionItem> {
        let mut description_length = None;
        if stream.version() == 1 && default_length.unwrap_or(0) == 0 {
            description_length = Some(u32::from_bytes(stream)?);
        }
        let entry = SampleGroupDescriptionEntry::from_bytes(stream, grouping_type)?;
        Ok(SampleGroupDescriptionItem {
            description_length,
            entry,
        })
    }
}

impl SampleGroupDescriptionItem {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W, default_length: Option<u32>) -> io::Result<()> {
        let version = stream.version();

        if version == 1 && default_length.unwrap_or(0) == 0 {
            self.description_length.unwrap_or(0).to_bytes(stream)?;
        }
        self.entry.to_bytes(stream)
    }
}

/// Generic (i.e. unreckognized) sample group entry.
#[derive(Debug)]
pub struct GenericSampleGroupEntry {
    pub data: Data,
}

impl GenericSampleGroupEntry {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<GenericSampleGroupEntry> {
        let data = Data::from_bytes(stream)?;
        Ok(GenericSampleGroupEntry{
            data,
        })
    }
}

impl ToBytes for GenericSampleGroupEntry {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        self.data.to_bytes(stream)
    }
}

macro_rules! sample_group_description_entries {
    ($($fourcc:expr => $name:ident,)*) => {

        #[derive(Debug)]
        pub enum SampleGroupDescriptionEntry {
            $(
                $name($name),
            )*
            GenericSampleGroupEntry(GenericSampleGroupEntry),
        }

        impl SampleGroupDescriptionEntry {
            fn from_bytes<R: ReadBytes>(stream: &mut R, grouping_type: FourCC) -> io::Result<SampleGroupDescriptionEntry> {
                let b = grouping_type.to_be_bytes();
                let e = match std::str::from_utf8(&b[..]).unwrap_or("") {
                    $(
                        $fourcc => {
                            SampleGroupDescriptionEntry::$name($name::from_bytes(stream)?)
                        },
                    )*
                    _ => SampleGroupDescriptionEntry::GenericSampleGroupEntry(GenericSampleGroupEntry::from_bytes(stream)?),
                };
                Ok(e)
            }
        }

        impl ToBytes for SampleGroupDescriptionEntry {
            fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
                match self {
                    $(
                        &SampleGroupDescriptionEntry::$name(ref b) => b.to_bytes(stream),
                    )*
                    &SampleGroupDescriptionEntry::GenericSampleGroupEntry(ref b) => b.to_bytes(stream),
                }
            }
        }
    };
}

sample_group_description_entries!{
    "roll" => RollRecoveryEntry,
}

def_struct! {
    /// AudioRollRecoveryEntry or VisualRollRecoveryEntry
    RollRecoveryEntry,
        roll_distance: i16,
}

