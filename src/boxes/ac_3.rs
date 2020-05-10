//
// ISO/IEC 14496-12:2015(E)
// 8.5.2 Sample Description Box 
//

use std::io;
use crate::serialize::{FromBytes, ToBytes, ReadBytes, WriteBytes};
use crate::mp4box::{BoxInfo, BoxReader, BoxWriter};
use crate::boxes::MP4Box;
use crate::types::*;

def_box! {
    /// AC-3 sample entry.
    Ac3SampleEntry, "ac-3",
        skip:                   6,
        data_reference_index:   u16,
        skip:                   4,
        // (mono = 1 ; stereo = 2)
        channel_count: u16,
        // audio sample number of bits 8 or 16
        sample_size: u16,
        skip:                   4,
        sample_rate: FixedFloat16_16,
        // sub boxes, probably only dac3.
        boxes: [MP4Box],
}

impl Ac3SampleEntry {
    /// Return human name of codec, like "Baseline" or "High".
    pub fn codec_name(&self) -> &'static str {
        "AC-3 Dolby Digital"
    }

    /// Return codec id as avc1.4D401F
    pub fn codec_id(&self) -> String {
        "ac-3".to_string()
    }
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
    ///
    /// L=Left, R=Right,
    /// S=Surround, SL=Surround Left, SR=Surround Right, BS=Back Surround
    /// C=Center, LC=Left Center, RC=Right Center
    /// LFE=Low Frequency Effects (sub)
    ///
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

