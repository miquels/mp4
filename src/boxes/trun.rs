//
// ISO/IEC 14496-12:2015(E)
// 8.8.8 Track Fragment Run Box
//

use std::io;
use crate::boxes::prelude::*;

//  aligned(8) class TrackRunBox
//  extends FullBox(‘trun’, version, tr_flags) {
//      unsigned int(32) sample_count;
//      // the following are optional fields
//      signed int(32) data_offset;
//      unsigned int(32) first_sample_flags;
//      // all fields in the following array are optional
//      {
//          unsigned int(32) sample_duration;
//          unsigned int(32) sample_size;
//          unsigned int(32) sample_flags
//          if (version == 0)
//              { unsigned int(32) sample_composition_time_offset; }
//          else
//              { signed int(32) sample_composition_time_offset; }
//      }[ sample_count ]
//  }

def_box! {
    /// 8.8.8 Track Fragment Run Box (ISO/IEC 14496-12:2015(E))
    #[derive(Default)]
    TrackRunBox {
        data_offset:                Option<i32>,
        first_sample_flags:         Option<SampleFlags>,
        entries:                    ArrayUnsized<TrackRunEntry>,
    },
    fourcc => "trun",
    version => [1],
    impls => [ boxinfo, debug ],
}

// as long as we don't have bool.then().
fn b_then<T>(flag: bool, closure: impl FnOnce() -> T) -> Option<T> {
    if flag {
        Some(closure())
    } else {
        None
    }
}

impl FromBytes for TrackRunBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<TrackRunBox> {
        let mut reader = BoxReader::new(stream)?;
        let stream = &mut reader;

        let flags = stream.flags();

        let sample_count = u32::from_bytes(stream)?;

        let data_offset = b_then((flags & 0x01) > 0, || i32::from_bytes(stream)).transpose()?;
        let first_sample_flags = b_then((flags & 0x04) > 0, || SampleFlags::from_bytes(stream)).transpose()?;

        let do_sample_dur = (flags & 0x0100) > 0;
        let do_sample_size = (flags & 0x0200) > 0;
        let do_sample_flags = (flags & 0x0400) > 0;
        let do_sample_comp = (flags & 0x0800) > 0;

        let mut entries = ArrayUnsized::new();
        while entries.len() < sample_count as usize {
            let sample_duration = b_then(do_sample_dur, || u32::from_bytes(stream)).transpose()?;
            let sample_size = b_then(do_sample_size, || u32::from_bytes(stream)).transpose()?;
            let sample_flags = b_then(do_sample_flags, || SampleFlags::from_bytes(stream)).transpose()?;
            let sample_composition_time_offset = if do_sample_comp {
                if stream.version() == 0 {
                    Some(std::cmp::min(u32::from_bytes(stream)?, 0x7fffffff) as i32)
                } else {
                    Some(i32::from_bytes(stream)?)
                }
            } else {
                None
            };
            entries.push(TrackRunEntry {
                sample_duration,
                sample_size,
                sample_flags,
                sample_composition_time_offset,
            });
        }

        Ok(TrackRunBox {
            data_offset,
            first_sample_flags,
            entries,
        })
    }

    fn min_size() -> usize { 16 }
}

impl ToBytes for TrackRunBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self)?;
        let stream = &mut writer;

        (self.entries.len() as u32).to_bytes(stream)?;

        self.data_offset.as_ref().map_or(Ok(()), |v| v.to_bytes(stream))?;
        self.first_sample_flags.as_ref().map_or(Ok(()), |v| v.to_bytes(stream))?;

        for e in &self.entries {
            e.to_bytes(stream)?;
        }

        stream.finalize()
    }
}

impl FullBox for TrackRunBox {
    fn version(&self) -> Option<u8> {
        for e in &self.entries {
            if let Some(cto) = e.sample_composition_time_offset {
                if cto < 0 {
                    return Some(1);
                }
            }
        }
        Some(0)
    }
    fn flags(&self) -> u32 {
        self.data_offset.is_some() as u32 * 0x01 |
        self.first_sample_flags.is_some() as u32 * 0x04 |
        self.entries.first().map(|e| e.flags()).unwrap_or(0)
    }
}

/// 8.8.8 Track Fragment Run Sample Entry (ISO/IEC 14496-12:2015(E))
#[derive(Clone, Debug)]
pub struct TrackRunEntry {
    pub sample_duration: Option<u32>,
    pub sample_size: Option<u32>,
    pub sample_flags: Option<SampleFlags>,
    pub sample_composition_time_offset: Option<i32>,
}

impl TrackRunEntry {
    fn flags(&self) -> u32 {
        self.sample_duration.is_some() as u32 * 0x0100 |
            self.sample_size.is_some() as u32 * 0x0200 |
            self.sample_flags.is_some() as u32 * 0x0400 |
            self.sample_composition_time_offset.is_some() as u32 * 0x0800
    }
}

impl ToBytes for TrackRunEntry {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let flags = stream.flags();
        if (flags & 0x0100) > 0 {
            self.sample_duration.unwrap_or(0).to_bytes(stream)?;
        }
        if (flags & 0x0200) > 0 {
            self.sample_size.unwrap_or(0).to_bytes(stream)?;
        }
        if (flags & 0x0400) > 0 {
            self.sample_flags.as_ref().unwrap_or(&SampleFlags::default()).to_bytes(stream)?;
        }
        if (flags & 0x0800) > 0 {
            self.sample_composition_time_offset.unwrap_or(0).to_bytes(stream)?;
        }
        Ok(())
    }
}
