use std::io;

use crate::boxes::prelude::*;

def_box! {
    EditBox {
        boxes:  Vec<EditListBox>,
    },
    fourcc => "edts",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    EditListBox {
        entries:    ArraySized32<EditListEntry>,
    },
    fourcc => "elst",
    version => [1, entries ],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

/// Entry in an edit list.
#[derive(Clone, Debug)]
pub struct EditListEntry {
    pub segment_duration:   u64,
    pub media_time:     i64,
    pub media_rate: u16,
}

impl FromBytes for EditListEntry {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
        let entry = if stream.version() == 0 {
            EditListEntry {
                segment_duration:   u32::from_bytes(stream)? as u64,
                media_time:         i32::from_bytes(stream)? as i64,
                media_rate:         u16::from_bytes(stream)?,
            }
        } else {
            EditListEntry {
                segment_duration:   u64::from_bytes(stream)?,
                media_time:         i64::from_bytes(stream)?,
                media_rate:         u16::from_bytes(stream)?,
            }
        };
        stream.skip(2)?;
        Ok(entry)
    }

    fn min_size() -> usize {
        12
    }
}

impl ToBytes for EditListEntry {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        if stream.version() == 0 {
            (self.segment_duration as u32).to_bytes(stream)?;
            (self.media_time as i32).to_bytes(stream)?;
        } else {
            self.segment_duration.to_bytes(stream)?;
            self.media_time.to_bytes(stream)?;
        }
        self.media_rate.to_bytes(stream)?;
        0u16.to_bytes(stream)?;
        Ok(())
    }
}

impl FullBox for EditListEntry {
    fn version(&self) -> Option<u8> {
        if self.segment_duration > 0xffffffff ||
           self.media_time < -0x7fffffff ||
           self.media_time > 0x7fffffff {
            Some(1)
        } else {
            Some(0)
        }
    }
}

