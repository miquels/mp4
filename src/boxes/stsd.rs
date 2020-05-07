//
// ISO/IEC 14496-12:2015(E)
// 8.5.2 Sample Description Box 
//

use std::io;
use crate::serialize::{FromBytes, ToBytes, ReadBytes, WriteBytes, BoxBytes};
use crate::mp4box::{BoxInfo, FullBox, BoxReader, BoxWriter};
use crate::boxes::MP4Box;
use crate::types::*;

def_box! {
    /// 8.5.2 Sample Description Box (ISO/IEC 14496-12:2015(E))
    SampleDescriptionBox, "stsd",
        entries:    [MP4Box, sized],
}

// version is set to zero unless the box contains an AudioSampleEntryV1, whereupon version must be 1
impl FullBox for SampleDescriptionBox {
}

def_box! {
    /// AVC sample entry.
    AvcSampleEntry, "avc1",
        skip:                   6,
        data_reference_index:   u16,
        _video_encoding_version:    u16,
        _video_encoding_revision:   u16,
        _video_encoding_vendor:     FourCC,
        _video_temporal_quality:    u32,
        _video_spatial_quality:     u32,
        wirdth:                 u16,
        height:                 u16,
        // defaults to 72, 72
        _video_horizontal_dpi:   FixedFloat16_16,
        _video_vertical_dpi:     FixedFloat16_16,
        _video_data_size:       u32,
        // defaults to 1
        _video_frame_count:     u16,
        // Video encoder name is a fixed-size pascal string.
        // _video_encoder_name: PascalString<32>,
        skip:                   32,
        video_pixel_depth:      u16,
        // -1: no table, 0: table follows inline (do not use?), >0: id.
        _video_color_table_id:   u16,
        // avcC and other boxes (pasp?)
        sub_boxes:              [MP4Box, unsized],
}

impl Default for AvcSampleEntry {
    fn default() -> Self {
        AvcSampleEntry {
            data_reference_index:     0,
            _video_encoding_version:  0,
            _video_encoding_revision: 0,
            _video_encoding_vendor:   FourCC::default(),
            _video_temporal_quality:  0,
            _video_spatial_quality:   0,
            wirdth:                   1280,
            height:                   720,
            _video_horizontal_dpi:    FixedFloat16_16::from(72f64),
            _video_vertical_dpi:      FixedFloat16_16::from(72f64),
            _video_data_size:         0,
            _video_frame_count:       1,
            video_pixel_depth:       24,
            _video_color_table_id:    0xffff,
            sub_boxes:                Vec::new(),
        }
    }
}

def_box! {
    /// Box that contains AVC Decoder Configuration Record.
    AvcConfigurationBox, "avcC",
        configuration: AvcDecoderConfigurationRecord,
}

def_struct! {
    /// AVC Decoder Configuration Record.
    AvcDecoderConfigurationRecord,
        configuration_version:  u8,
        profile_idc:            u8,
        constraint_set_flags:    u8,
        level_idc:              u8,
        data:                   Data,
}

impl AvcDecoderConfigurationRecord {
    /// Return human name of codec, like "Baseline" or "High".
    pub fn codec_description(&self) -> Option<&'static str> {
        let v = match self.profile_idc {
            0x2c => "CAVLC 4:4:4",
            0x42 => "Baseline",
            0x4d => "Main",
            0x58 => "Extended",
            0x64 => "High",
            0x6e => "High 10",
            0x7a => "High 4:2:2",
            0xf4 => "High 4:4:4",

            0x53 => "Scalable Baseline",
            0x56 => "Scalable High",

            0x76 => "Multiview High",
            0x80 => "Stereo High",
            0x8a => "Multiview Depth High",
            _ => return None,
        };
        Some(v)
    }

    /// Return codec name as avc1.64001f (High)
    pub fn codec_name(&self) -> String {
        /// FIXME not sure if this is correct, what is the middle value?
        /// Is it `constraint_set_flags`? or something else.
        let mut s = format!("avc1.{:02X}{:02X}{:02X}",
                            self.profile_idc, self.constraint_set_flags, self.level_idc);
        if let Some(p) = self.codec_description() {
            s.push_str(" (");
            s.push_str(p);
            s.push_str(")");
        }
        s
    }
}

/// delegated to AvcDecoderConfigurationRecord::codec_name().
impl std::fmt::Display for AvcDecoderConfigurationRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.codec_name())
    }
}

def_box! {
    /// AC-3 sample entry.
    Ac3SampleEntry, "ac-3",
        skip:                   6,
        data_reference_index:   u16,
        // default = 0 ; audio data size before decompression = 1
        _audio_encoding_version: u16,
        // always 0
        _audio_encoding_revision: u16,
        // default 0
        _audio_encoding_vendor: FourCC,
        // (mono = 1 ; stereo = 2)
        channel_count: u16,
        // audio sample number of bits 8 or 16
        sample_size: u16,
        // default = 0
        _audio_compression_id: u16,
        // default = 0
        _audio_packet_size: u16,
        sample_rate: FixedFloat16_16,
        // sub boxes, probably only dac3. FIXME: really only dac3?
        boxes: [MP4Box],
}

pub struct AC3SpecificBox {
    pub fscod: u8,
    // bsid is "version". usually 8.
    pub bsid: u8,
    pub bsmod: u8,
    pub acmod: u8,
    pub lfeon: bool,
    pub bitrate_code: u8,
    pub reserved: u8,
}

impl FromBytes for AC3SpecificBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<AC3SpecificBox> {
        let mut reader = BoxReader::new(stream)?;
        let data = Data::read(&mut reader, 3)?;
        let mut b = BitReader::new(&data.0);
        Ok(AC3SpecificBox {
            fscod:  b.read_bits(2)? as u8,
            bsid:  b.read_bits(5)? as u8,
            bsmod:  b.read_bits(3)? as u8,
            acmod:  b.read_bits(3)? as u8,
            lfeon:  b.read_bits(1)? != 0,
            bitrate_code:  b.read_bits(5)? as u8,
            reserved:  b.read_bits(5)? as u8,
        })
    }
    fn min_size() -> usize { 11 }
}

impl ToBytes for AC3SpecificBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self)?;

        // Don't feel like implementing BitWriter right now ...
        let b1: u8 = (self.fscod << 6) | (self.bsid << 1) | (self.bsmod >> 2);
        let b2: u8 = ((self.bsmod & 0x03) << 6) | (self.acmod << 3) | ((self.lfeon as u8) << 2) | (self.bitrate_code >> 3);
        let b3 = ((self.bitrate_code & 0x7) << 5) | self.reserved;
        b1.to_bytes(&mut writer)?;
        b2.to_bytes(&mut writer)?;
        b3.to_bytes(&mut writer)?;

        writer.finalize()
    }
}

#[derive(Debug)]
#[repr(u8)]
pub enum AudioService {
    CompleteMain,
    MusicAndEffects,
    VisuallyImpaired,
    HearingImpaired,
    Dialog,
    Commentary,
    Emergency,
    VoiceOver,
}

impl AC3SpecificBox {
    /// Sampling rate as coded in `fscod`. `0` means "unknown" (fscod 0b11).
    pub fn sampling_rate(&self) -> u32 {
        if self.fscod > 2 {
            return 0;
        }
        [48000, 44100, 32000][self.fscod as usize]
    }

    /// Audio service from bsmod.
    pub fn audio_service(&self) -> AudioService {
        use AudioService::*;
        match self.bsmod {
            0 => CompleteMain,
            1 => MusicAndEffects,
            2 => VisuallyImpaired,
            3 => HearingImpaired,
            4 => Dialog,
            5 => Commentary,
            6 => Emergency,
            7 => VoiceOver,
            _ => CompleteMain,
        }
    }

    /// Audio channels. "1+2", "L,C,R,SL,SR", etc.
    pub fn audio_channels(&self) -> &'static str {
        match self.acmod {
            0 => "1+2",
            1 => "C",
            2 => "L,R",
            3 => "L,C,R",
            4 => "L,R,S",
            5 => "L,C,R,S",
            6 => "L,R,SL,SR",
            7 => "L,C,R,SL,SR",
            _ => "unknown",
        }
    }

    /// Number of audio channels.
    pub fn num_channels(&self) -> u8 {
        if self.acmod > 7 {
            return 0;
        }
        [2, 1, 2, 3, 3, 4, 4, 5][self.acmod as usize] + self.lfeon as u8
    }

    /// Bitrate from bitrate_code.
    pub fn bitrate(&self) -> u32 {
        if self.bitrate_code > 18 {
            return 0;
        }
        [
            32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192,
            224, 256, 320, 384, 448, 512, 576, 640
        ][self.bitrate_code as usize] * 1000
    }
}

impl std::fmt::Debug for AC3SpecificBox {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut dbg = f.debug_struct("AC3SpecificBox");
        dbg.field("bitrate", &self.bitrate());
        dbg.field("audio_service", &self.audio_service());
        dbg.field("audio_channels", &self.audio_channels());
        dbg.field("sub_channel", &self.lfeon);
        dbg.field("num_channels", &self.num_channels());
        dbg.finish()
    }
}

def_box! {
    /// AAC sample entry.
    AacSampleEntry, "mp4a",
        skip:                   6,
        data_reference_index:   u16,
        // default = 0 ; audio data size before decompression = 1
        _audio_encoding_version: u16,
        // always 0
        _audio_encoding_revision: u16,
        // default 0
        _audio_encoding_vendor: FourCC,
        // (mono = 1 ; stereo = 2)
        channel_count: u16,
        // audio sample number of bits 8 or 16
        sample_size: u16,
        // default = 0
        _audio_compression_id: u16,
        // default = 0
        _audio_packet_size: u16,
        sample_rate: FixedFloat16_16,
        // sub boxes, probably only esds. FIXME: really only esds?
        boxes: [MP4Box],
}

def_box! {
    /// MPEG4 ESDescriptor.
    // FIXME? "m4ds" is an alias we currently do not reckognize.
    ESDescriptorBox, "esds",
        es_descriptor:   ESDescriptor,
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
                warn!("ESDescriptorBox: length field > 4 bytes (@{})", stream.pos());
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
        while size > 0 {
            let mut b = (size & 0x7f) as u8;
            size >>= 7;
            if size > 0 {
                b |= 0x80;
            }
            b.to_bytes(stream)?;
        }
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
#[derive(Debug)]
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
#[derive(Debug)]
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

#[derive(Debug, Default)]
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
#[derive(Debug, Default)]
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

#[derive(Debug, Default)]
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

#[derive(Debug, Default)]
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

//
//
// Helpers.
//
//

/// Pascal string. 1 byte of length followed by string itself.
///
/// Note that the length does not include the length byte itself.
#[derive(Debug, Default)]
pub struct PString(String);

impl FromBytes for PString {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<PString> {
        let len = u8::from_bytes(stream)? as u64;
        let data = if len > 0 {
            stream.read(len)?
        } else {
            b""
        };
        if let Ok(s) = std::str::from_utf8(data) {
            return Ok(PString(s.to_string()));
        }
        // If it's not utf-8, mutilate the data.
        let mut s = String::new();
        for d in data {
            s.push(std::cmp::min(*d, 127) as char);
        }
        Ok(PString(s))
    }
    fn min_size() -> usize { 0 }
}

impl ToBytes for PString {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let len = std::cmp::min(self.0.len(), 254);
        (len as u8).to_bytes(stream)?;
        stream.write(self.0[..len].as_bytes())
    }
}

// A writer that doesn't really write, it just counts the bytes
// that it would write if it were a real writer. How much wood
// would a woodchuck etc.
#[derive(Debug, Default)]
struct CountBytes {
    pos:    usize,
    max:    usize,
}

impl CountBytes {
    pub fn new() -> CountBytes {
        CountBytes {
            pos: 0,
            max: 0,
        }
    }
}

impl WriteBytes for CountBytes {
    fn write(&mut self, newdata: &[u8]) -> io::Result<()> {
        self.pos += newdata.len();
        if self.max < self.pos {
            self.max = self.pos;
        }
        Ok(())
    }

    fn skip(&mut self, amount: u64) -> io::Result<()> {
        self.pos += amount as usize;
        Ok(())
    }
}

impl BoxBytes for CountBytes {
    fn pos(&self) -> u64 {
        self.pos as u64
    }
    fn seek(&mut self, pos: u64) -> io::Result<()> {
        self.pos = pos as usize;
        Ok(())
    }
    fn size(&self) -> u64 {
        self.max as u64
    }
}

// Read binary data bit-by-bit.
struct BitReader<'a> {
    data:   &'a[u8],
    pos:    usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> BitReader<'a> {
        BitReader {
            data,
            pos: 0,
        }
    }

    fn read_bit(&self, pos: usize) -> io::Result<bool> {
        let b = pos / 8;
        if b >= self.data.len() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let c = pos - 8 * b;
        let bit = self.data[b] & (128 >> c);
        Ok(bit > 0)
    }

    fn read_bits(&mut self, count: u8) -> io::Result<u32> {
        let mut count = count;
        let mut r = 0;

        while count > 0 {
            r = r << 1 | (self.read_bit(self.pos)?) as u32;
            self.pos += 1;
            count -= 1;
        }
        Ok(r)
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

