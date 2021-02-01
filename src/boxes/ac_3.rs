//
// ISO/IEC 14496-12:2015(E)
// 8.5.2 Sample Description Box 
//

use std::io;

use crate::boxes::prelude::*;
use crate::bitreader::BitReader;
use crate::track::AudioTrackInfo;

def_box! {
    /// AC-3 sample entry.
    Ac3SampleEntry {
        skip:                   6,
        data_reference_index:   u16,
        skip:                   8,
        // (mono = 1 ; stereo = 2)
        channel_count: u16,
        // audio sample number of bits 8 or 16
        sample_size: u16,
        skip:                   4,
        sample_rate_hi:         u16,
        sample_rate_lo:         u16,
        // sub boxes, probably only dac3.
        boxes: [MP4Box],
    },
    fourcc => "ac-3",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

impl Default for Ac3SampleEntry {
    fn default() -> Ac3SampleEntry {
        Ac3SampleEntry {
            data_reference_index:   1,
            channel_count:          2,
            sample_size:            16,
            sample_rate_hi:         0,
            sample_rate_lo:         0,
            boxes:                  Vec::new(),
        }
    }
}

impl Ac3SampleEntry {
    /// Return audio specific track info.
    pub fn track_info(&self) -> AudioTrackInfo {
        let mut ai = AudioTrackInfo {
            codec_id:   "ac-3".to_string(),
            codec_name: Some("AC-3 Dolby Digital".to_string()),
            channel_count:  self.channel_count,
            bit_depth: if self.sample_size > 0 { Some(self.sample_size) } else { None },
            sample_rate: if self.sample_rate_hi > 0 { Some(self.sample_rate_hi as u32) } else { None },
            ..AudioTrackInfo::default()
        };

        if let Some(dac3) = first_box!(&self.boxes, AC3SpecificBox) {
            ai.channel_count = dac3.num_channels() as u16;
            ai.lfe_channel = dac3.lfe_channel();
            if dac3.sampling_rate() > 0 {
                ai.sample_rate = Some(dac3.sampling_rate());
            }
            ai.channel_configuration = Some(dac3.channel_configuration().to_string());
            ai.avg_bitrate = if dac3.bitrate() > 0 { Some(dac3.bitrate()) } else { None };
            ai.max_bitrate = if dac3.bitrate() > 0 { Some(dac3.bitrate()) } else { None };
        }

        ai
    }
}

def_box! {
    /// AC-3 specific box.
    AC3SpecificBox {
        fscod: u8,
        // bsid is "version". usually 8.
        bsid: u8,
        bsmod: u8,
        acmod: u8,
        lfeon: bool,
        bitrate_code: u8,
        reserved: u8,
    },
    fourcc => "dac3",
    version => [],
    impls => [ basebox, boxinfo ],
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
    ///
    /// L=Left, R=Right,
    /// S=Surround, SL=Surround Left, SR=Surround Right, BS=Back Surround
    /// C=Center, LC=Left Center, RC=Right Center
    /// LFE=Low Frequency Effects (sub)
    ///
    pub fn channel_configuration(&self) -> &'static str {
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
            // unknown, so it's probably going to be decoded as stereo.
            return 2;
        }
        [2, 1, 2, 3, 3, 4, 4, 5][self.acmod as usize]
    }

    /// LFE (sub) channel?
    pub fn lfe_channel(&self) -> bool {
        self.lfeon
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
        dbg.field("channel_configuration", &self.channel_configuration());
        dbg.field("sub_channel", &self.lfeon);
        dbg.field("num_channels", &self.num_channels());
        dbg.finish()
    }
}

