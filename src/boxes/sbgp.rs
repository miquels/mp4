use std::io;

use crate::boxes::prelude::*;

def_box! {
    /// 8.9.2 Sample to Group Box (ISO/IEC 14496-12:2015(E))
    SampleToGroupBox {
        grouping_type:  FourCC,
        grouping_type_parameter: Option<u32>,
        entries:        ArraySized32<SampleToGroupEntry>,
    },
    fourcc => "sbgp",
    version => [1],
    impls => [ boxinfo, debug ],
}

fn overlap(a: u32, b:u32, c: u32, d: u32) -> Option<u32> {
    use std::cmp::min;

    if b < c || a > d {
        return None;
    }
    let d = min(b - a, min(b - c, min(d - c, d - a))) + 1;
    Some(d)
}

impl SampleToGroupBox {
    /// Clone a range of the SampleToGroupBox.
    ///
    /// Used for building a TrackFragmentBox.
    pub fn clone_range(&self, from_sample: u32, to_sample: u32) -> SampleToGroupBox {
        let mut sbgp = SampleToGroupBox {
            grouping_type: self.grouping_type.clone(),
            grouping_type_parameter: self.grouping_type_parameter.clone(),
            entries: ArraySized32::<SampleToGroupEntry>::new(),
        };
        let mut begin = 1;
        for entry in self.entries.iter() {
            let end = begin + entry.sample_count - 1;
            if begin > to_sample {
                break;
            }
            if let Some(count) = overlap(from_sample, to_sample, begin, end) {
                sbgp.entries.push(SampleToGroupEntry{
                    sample_count: count,
                    group_description_index: entry.group_description_index,
                });
            }
            begin += entry.sample_count;
        }
        sbgp
    }
}

impl FromBytes for SampleToGroupBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<SampleToGroupBox> {
        let mut reader = BoxReader::new(stream)?;
        let stream = &mut reader;
        let grouping_type = FourCC::from_bytes(stream)?;
        let grouping_type_parameter = if stream.version() == 1 {
            Some(u32::from_bytes(stream)?)
        } else {
            None
        };
        let entries = ArraySized32::<SampleToGroupEntry>::from_bytes(stream)?;
        Ok(SampleToGroupBox {
            grouping_type,
            grouping_type_parameter,
            entries,
        })
    }

    fn min_size() -> usize { 16 }
}

impl ToBytes for SampleToGroupBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self)?;
        let stream = &mut writer;

        self.grouping_type.to_bytes(stream)?;
        if let Some(param) = self.grouping_type_parameter {
            param.to_bytes(stream)?;
        }
        self.entries.to_bytes(stream)?;

        stream.finalize()
    }
}

impl FullBox for SampleToGroupBox {
    fn version(&self) -> Option<u8> {
        if self.grouping_type_parameter.is_some() {
            Some(1)
        } else {
            Some(0)
        }
    }
}

def_struct! {
    /// Entry in SampleToGroupBox.
    SampleToGroupEntry,
        sample_count:               u32,
        group_description_index:    u32,
}

