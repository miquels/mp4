//
// ISO/IEC 14496-12:2015(E)
// 8.5.2 Sample Description Box 
//

use std::io;

use crate::boxes::prelude::*;
use crate::bitreader::BitReader;
use crate::track::AudioTrackInfo;
use crate::io::CountBytes;

def_box! {
    /// AAC sample entry (AudioSampleEntry).
    AacSampleEntry {
        skip:                   6,
        data_reference_index:   u16,
        skip:                   8,
        // (mono = 1 ; stereo = 2)
        channel_count: u16,
        // audio sample number of bits 8 or 16
        sample_size: u16,
        skip:                   4,
        sample_rate_hi: u16,
        sample_rate_lo: u16,
        // sub boxes, probably only esds.
        boxes: Vec<MP4Box>,
    },
    fourcc => "mp4a",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

impl AacSampleEntry {
    /// Return audio specific track info.
    pub fn track_info(&self) -> AudioTrackInfo {
        let mut ai = AudioTrackInfo {
            codec_id:   "mp4a".to_string(),
            codec_name: Some("MPEG4 Audio".to_string()),
            channel_count:  self.channel_count as u16,
            bit_depth: if self.sample_size > 0 { Some(self.sample_size) } else { None },
            sample_rate: if self.sample_rate_hi > 0 { Some(self.sample_rate_hi as u32) } else { None },
            ..AudioTrackInfo::default()
        };

        if let Some(esds) = first_box!(&self.boxes, ESDescriptorBox) {
            ai.codec_id = esds.codec_id();
            ai.codec_name = Some(esds.codec_name().to_string());
            let config = &esds.es_descriptor.decoder_config;
            if let Some(ref audio) = config.specific_info.audio {
                ai.channel_configuration = match audio.channel_config {
                    1 => Some("1"),
                    2 => Some("L,R"),
                    3 => Some("C,L,R"),
                    4 => Some("C,L,R,S"),
                    5 => Some("C,L,R,LS,RS"),
                    6 => Some("C,L,R,LS,RS"),
                    _ => None,
                }.map(|x| x.to_string());
                let c = match audio.channel_config {
                    1 => 1,
                    2 => 2,
                    3 => 3,
                    4 => 4,
                    5 => 5,
                    6 => 5,
                    7 => 7,
                    11 => 6,
                    12 => 7,
                    13 => 22,
                    14 =>7,
                    _ => 0,
                };
                if c > 0 {
                    ai.channel_count = c;
                }
                ai.lfe_channel = match audio.channel_config {
                    6|7|11|12|13|14 => true,
                    _ => false,
                };
                if audio.sampling_frequency_index < 12 {
                    let sr: u32 = [ 
                        96000, 88200, 64000, 48000, 44100, 32000,
                        24000, 22050, 16000, 12000, 11025, 8000
                    ][audio.sampling_frequency_index as usize];
                    ai.sample_rate = Some(sr);
                } else if audio.sampling_frequency_index == 0xf && audio.sampling_frequency > 0 {
                    ai.sample_rate = Some(audio.sampling_frequency);
                }
            }
        }

        ai
    }
}


def_box! {
    /// MPEG4 ESDescriptor.
    // FIXME? "m4ds" is an alias we currently do not reckognize.
    ESDescriptorBox {
        es_descriptor:   ESDescriptor,
    },
    fourcc => "esds",
    version => [0],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

impl ESDescriptorBox {
    /// Return human name of codec, like "Baseline" or "High".
    pub fn codec_name(&self) -> &'static str {
        let config = &self.es_descriptor.decoder_config;
        if config.stream_type != 5 {
            return "mp4a";
        }
        match config.specific_info.audio {
            Some(ref audio) => match audio.profile {
                2 => "AAC-LC",
                5 => "HE-AAC",
                29 => "HE-AACv2",
                _ => "AAC",
            },
            None => "MPEG-4 Audio",
        }
    }

    /// Return codec id as avc1.4D401F
    pub fn codec_id(&self) -> String {
        let config = &self.es_descriptor.decoder_config;
        if config.stream_type != 5 {
            return "mp4a".to_string();
        }
        match config.specific_info.audio {
            Some(ref audio) => format!("mp4a.{:02x}.{}", config.object_type, audio.profile),
            None => format!("mp4a.{:02x}", config.object_type),
        }
    }
}

//
//
// MPEG4 ESDescriptor for mpeg4 audio:
// - mp4a.40.2
// - mp4a.40.5
// - mp4a.40.29
//
// A lot of thanks to the code-as-documentation in:
// https://github.com/sannies/mp4parser/tree/master/isoparser/src/main/java/org/mp4parser
//

// Every descriptor starts with a length and a tag.
#[derive(Clone, Debug)]
struct BaseDescriptor {
    size: u32,
    tag:    u8,
}

impl FromBytes for BaseDescriptor {
    // Read length and tag.
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<BaseDescriptor> {
        let tag = u8::from_bytes(stream)?;
        let mut size = 0;
        for i in 1..=4 {
            let b = u8::from_bytes(stream)?;
            size = (size << 7) | ((b &0x7f) as u32);
            if b & 0x80 == 0 {
                break;
            }
            if i == 4 {
                log::warn!("ESDescriptorBox: length field > 4 bytes (@{})", stream.pos());
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        Ok(BaseDescriptor{
            size,
            tag,
        })
    }

    fn min_size() -> usize { 0 }
}

impl ToBytes for BaseDescriptor {
    // Write length and tag.
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        self.tag.to_bytes(stream)?;
        let mut size = self.size;

        if size >= 268435456 {
            // 28 bits max.
            log::warn!("ESDescriptorBox: tag {} length field does not find in 28 bits", self.tag);
            return Err(io::ErrorKind::InvalidData.into());
        }

        let mut buf = [0u8; 4];
        for idx in (0..=3).rev() {
            let mut b = (size & 0x7f) as u8;
            size >>= 7;
            if idx != 3 {
                b |= 0x80;
            }
            buf[idx] = b;
        }
        stream.write(&buf[..])?;

        Ok(())
    }
}

// Stream Descriptors. We implement:
const ESDESCRIPTOR_TAG: u8 = 0x03;
const DECODER_CONFIG_DESCRIPTOR_TAG: u8 = 0x04;
const DECODER_SPECIFIC_INFO_TAG: u8 = 0x05;
const SLCONFIG_DESCRIPTOR_TAG: u8 = 0x06;

/// Elementary Stream Descriptor, tag 0x03.
///
/// In a MP4 file, depends_on_es_id, url, and ocr_es_id are always None.
#[derive(Clone, Debug)]
pub struct ESDescriptor {
    // lower 16 bits of Track Id, or 0.
    pub es_id:                      u16,
    pub stream_priority:            u8,
    pub depends_on_es_id:           Option<u16>,
    pub url:                        Option<PString>,
    pub ocr_es_id:                  Option<u16>,
    pub decoder_config:             DecoderConfigDescriptor,
    pub sl_config:                  SLConfigDescriptor,
    pub data:                       Data,
}

impl FromBytes for ESDescriptor {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<ESDescriptor> {
        let base = BaseDescriptor::from_bytes(stream)?;
        assert!(base.tag == ESDESCRIPTOR_TAG);
        let pos = stream.pos();
        let es_id = u16::from_bytes(stream)?;
        let flags = u8::from_bytes(stream)?;
        let stream_priority = flags & 0x1f;
        let depends_on_es_id = if flags & 0x80 > 0 {
            Some(u16::from_bytes(stream)?)
        } else {
            None
        };
        let url = if flags & 0x40 > 0 {
            Some(PString::from_bytes(stream)?)
        } else {
            None
        };
        let ocr_es_id = if flags & 0x20 > 0 {
            Some(u16::from_bytes(stream)?)
        } else {
            None
        };
        let decoder_config = DecoderConfigDescriptor::from_bytes(stream)?;
        let sl_config = SLConfigDescriptor::from_bytes(stream)?;

        let data = trailing_data(stream, pos, base.size)?;

        Ok(ESDescriptor{
            es_id,
            stream_priority,
            depends_on_es_id,
            url,
            ocr_es_id,
            decoder_config,
            sl_config,
            data,
        })
    }
    fn min_size() -> usize { 0 }
}

impl ESDescriptor {
    fn to_bytes_partial<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        self.es_id.to_bytes(stream)?;
        let flags: u8 = self.depends_on_es_id.as_ref().map(|_| 0x80).unwrap_or(0)
            | self.url.as_ref().map(|_| 0x40).unwrap_or(0)
            | self.ocr_es_id.as_ref().map(|_| 0x20).unwrap_or(0)
            | self.stream_priority;
        flags.to_bytes(stream)?;
        if let Some(ref x) = self.depends_on_es_id {
            x.to_bytes(stream)?;
        }
        if let Some(ref x) = self.url {
            x.to_bytes(stream)?;
        }
        if let Some(ref x) = self.url {
            x.to_bytes(stream)?;
        }
        self.decoder_config.to_bytes(stream)?;
        self.sl_config.to_bytes(stream)
    }
}

impl ToBytes for ESDescriptor {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut cnt = CountBytes::new();
        self.to_bytes_partial(&mut cnt)?;

        let base = BaseDescriptor{ tag: ESDESCRIPTOR_TAG, size: cnt.size() as u32 };
        base.to_bytes(stream)?;
        self.to_bytes_partial(stream)
    }
}


/// Decoder config, tag 0x04.
///
/// stream_type:
///   0x05 Audio
///
/// object_type:
/// - often used in MP4.
///   0x40 Audio ISO/IEC 14496-3 g
///
/// - MP3, sometimes used as audio codec in MP4.
///   0x66 Audio ISO/IEC 13818-7 Main Profile
///   0x67 Audio ISO/IEC 13818-7 LowComplexity Profile
///   0x68 Audio ISO/IEC 13818-7 Scaleable Sampling Rate Profile
///   0x69 Audio ISO/IEC 13818-3
///   0x6B Audio ISO/IEC 11172-3
#[derive(Clone, Debug)]
pub struct DecoderConfigDescriptor {
    pub object_type:    u8,
    pub stream_type:    u8,
    pub upstream:       bool,
    pub buffer_size:    u32,
    pub max_bitrate:    u32,
    pub avg_bitrate:    u32,
    pub specific_info:  DecoderSpecificInfo,
}

impl FromBytes for DecoderConfigDescriptor {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<DecoderConfigDescriptor> {
        let base = BaseDescriptor::from_bytes(stream)?;
        assert!(base.tag == DECODER_CONFIG_DESCRIPTOR_TAG);
        let object_type = u8::from_bytes(stream)?;
        let b = u32::from_bytes(stream)?;
        let b1 = ((b & 0xff000000) >> 24) as u8;
        let stream_type = b1 >> 2;
        let upstream = (b1 & 0x02) > 0;
        let buffer_size = b & 0x00ffffff;
        let max_bitrate = u32::from_bytes(stream)?;
        let avg_bitrate = u32::from_bytes(stream)?;
        let specific_info = DecoderSpecificInfo::from_bytes(stream, object_type)?;
        Ok(DecoderConfigDescriptor{
            object_type,
            stream_type,
            upstream,
            buffer_size,
            max_bitrate,
            avg_bitrate,
            specific_info,
        })
    }
    fn min_size() -> usize { 0 }
}

impl DecoderConfigDescriptor {
    fn to_bytes_partial<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        self.object_type.to_bytes(stream)?;
        let b = (self.stream_type << 2) | ((self.upstream as u8) << 1) | 0x01;
        let c = self.buffer_size | ((b as u32) << 24);
        c.to_bytes(stream)?;
        self.max_bitrate.to_bytes(stream)?;
        self.avg_bitrate.to_bytes(stream)?;
        self.specific_info.to_bytes(stream)
    }
}

impl ToBytes for DecoderConfigDescriptor {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut cnt = CountBytes::new();
        self.to_bytes_partial(&mut cnt)?;

        let base = BaseDescriptor{ tag: DECODER_CONFIG_DESCRIPTOR_TAG, size: cnt.size() as u32 };
        base.to_bytes(stream)?;
        self.to_bytes_partial(stream)
    }
}

#[derive(Clone, Debug, Default)]
pub struct DecoderSpecificInfo {
    pub data:   Data,
    pub audio:  Option<AudioSpecificConfig>,
}

/// For mp4a.40.<profile>.
///
/// Common profiles:
/// 2:  AAC-LC
/// 5:  HE-AAC   (AAC-LC + SBR)
/// 29: HE-AACv2 (AAC-LC + SBR + PS)
#[derive(Clone, Debug, Default)]
pub struct AudioSpecificConfig {
    pub profile:    u8,
    pub sampling_frequency_index:    u8,
    pub sampling_frequency:    u32,
    pub channel_config: u8,
}

impl DecoderSpecificInfo {
    fn from_bytes<R: ReadBytes>(stream: &mut R, object_type: u8) -> io::Result<DecoderSpecificInfo> {
        let base = BaseDescriptor::from_bytes(stream)?;
        assert!(base.tag == DECODER_SPECIFIC_INFO_TAG);

        let data = Data::read(stream, base.size as usize)?;

        let audio = if object_type == 0x40 || data.len() >= 2 {

            let mut b = BitReader::new(&data.0);

            let mut profile = b.read_bits(5)? as u8;
            if profile == 31 {
                profile = 32 + b.read_bits(6)? as u8;
            }
            let sampling_frequency_index = b.read_bits(4)? as u8;
            let mut sampling_frequency = 0;
            if sampling_frequency_index == 0xf {
                sampling_frequency = b.read_bits(24)?;
            }
            let channel_config = b.read_bits(4)? as u8;

            Some(AudioSpecificConfig {
                profile,
                sampling_frequency_index,
                sampling_frequency,
                channel_config,
            })
        } else {
            None
        };

        Ok(DecoderSpecificInfo {
            data,
            audio,
        })
    }
}

impl ToBytes for DecoderSpecificInfo {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let base = BaseDescriptor{ tag: DECODER_SPECIFIC_INFO_TAG, size: self.data.len() as u32 };
        base.to_bytes(stream)?;
        self.data.to_bytes(stream)
    }
}

#[derive(Clone, Debug, Default)]
pub struct SLConfigDescriptor {
    pub config_type:    u8,
    pub data:           Data,
}

impl FromBytes for SLConfigDescriptor {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<SLConfigDescriptor> {
        let base = BaseDescriptor::from_bytes(stream)?;
        assert!(base.tag == SLCONFIG_DESCRIPTOR_TAG);
        let pos = stream.pos();

        let config_type = u8::from_bytes(stream)?;
        let data = trailing_data(stream, pos, base.size)?;

        Ok(SLConfigDescriptor {
            config_type,
            data,
        })
    }
    fn min_size() -> usize { 0 }
}

impl SLConfigDescriptor {
    fn to_bytes_partial<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        self.config_type.to_bytes(stream)?;
        self.data.to_bytes(stream)
    }
}

impl ToBytes for SLConfigDescriptor {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut cnt = CountBytes::new();
        self.to_bytes_partial(&mut cnt)?;

        let base = BaseDescriptor{ tag: SLCONFIG_DESCRIPTOR_TAG, size: cnt.size() as u32 };
        base.to_bytes(stream)?;
        self.to_bytes_partial(stream)
    }
}

#[derive(Clone, Debug, Default)]
pub struct UnknownDescriptor {
    pub tag:        u8,
    pub data:       Data,
}

impl FromBytes for UnknownDescriptor {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<UnknownDescriptor> {
        let base = BaseDescriptor::from_bytes(stream)?;
        let pos = stream.pos();

        let tag = u8::from_bytes(stream)?;
        let data = trailing_data(stream, pos, base.size)?;

        Ok(UnknownDescriptor {
            tag,
            data,
        })
    }
    fn min_size() -> usize { 0 }
}

impl ToBytes for UnknownDescriptor {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let base = BaseDescriptor{ tag: self.tag, size: self.data.len() as u32 };
        base.to_bytes(stream)?;
        self.data.to_bytes(stream)
    }
}

// Helper to read any trailing data.
fn trailing_data<R: ReadBytes>(stream: &mut R, start: u64, size: u32) -> io::Result<Data> {
    let done = stream.pos() - start;
    let data = if done < size as u64 {
        let len = size as usize - done as usize;
        Data::read(stream, len)?
    } else {
        Data::default()
    };
    Ok(data)
}

