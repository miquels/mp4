//
// ISO/IEC 14496-12:2015(E)
// 8.5.2 Sample Description Box 
//

use std::io;

use crate::bitreader::BitReader;
use crate::boxes::prelude::*;
use crate::track::VideoTrackInfo;

def_box! {
    /// HEVC sample entry (VideoSampleEntry 'hvc1').
    ///
    /// Contains: 
    ///
    /// - HEVCConfigurationBox (one)
    /// - MPEG4BitRateBox (optional)
    /// - MPEG4ExtensionDescriptorsBox (optional)
    /// - extra boxes.
    HEVCSampleEntry {
        skip:                   6,
        data_reference_index:   u16,
        skip:                   16,
        width:                 u16,
        height:                 u16,
        // defaults to 72, 72
        _video_horizontal_dpi:  FixedFloat16_16,
        _video_vertical_dpi:    FixedFloat16_16,
        skip:                   4,
        // defaults to 1
        _video_frame_count:     u16,
        // Video encoder name is a fixed-size pascal string.
        // _video_encoder_name: PascalString<32>,
        skip:                   32,
        // defaults to 0x0018;
        video_pixel_depth:      u16,
        // always -1
        _pre_defined:           u16,
        // hvcC, etc.
        boxes:              Vec<MP4Box>,
    },
    fourcc => "hvc1",
    version => [], 
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

impl HEVCSampleEntry {
    /// Return video specific track info.
    pub fn track_info(&self) -> VideoTrackInfo {
        let config = first_box!(self.boxes, HEVCConfigurationBox);
        let codec_id = match config {
            Some(ref c) => c.configuration.codec_id(),
            None => "hvc1.unknown".to_string(),
        };
        let codec_name = match config {
            Some(ref c) => c.configuration.codec_name(),
            None => "HEVC",
        }.to_string();
        let frame_rate = match config {
            Some(ref c) => c.configuration.frame_rate(),
            None => 0f64,
        };
        VideoTrackInfo {
            codec_id,
            codec_name: Some(codec_name.to_string()),
            width: self.width,
            height: self.height,
            frame_rate,
        }
    }
}

def_box! {
    /// HEV1 sample entry (VideoSampleEntry 'hev1').
    ///
    /// Contains: 
    ///
    /// - HEVCConfigurationBox (one)
    /// - MPEG4BitRateBox (optional)
    /// - MPEG4ExtensionDescriptorsBox (optional)
    /// - extra boxes.
    HEV1SampleEntry {
        skip:                   6,
        data_reference_index:   u16,
        skip:                   16,
        width:                 u16,
        height:                 u16,
        // defaults to 72, 72
        _video_horizontal_dpi:  FixedFloat16_16,
        _video_vertical_dpi:    FixedFloat16_16,
        skip:                   4,
        // defaults to 1
        _video_frame_count:     u16,
        // Video encoder name is a fixed-size pascal string.
        // _video_encoder_name: PascalString<32>,
        skip:                   32,
        // defaults to 0x0018;
        video_pixel_depth:      u16,
        // always -1
        _pre_defined:           u16,
        // hvcC, etc.
        boxes:              Vec<MP4Box>,
    },
    fourcc => "hev1",
    version => [], 
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

impl HEV1SampleEntry {
    /// Return video specific track info.
    pub fn track_info(&self) -> VideoTrackInfo {
        let config = first_box!(self.boxes, HEVCConfigurationBox);
        let codec_id = match config {
            Some(ref c) => c.configuration.codec_id(),
            None => "hvc1.unknown".to_string(),
        };
        let codec_name = match config {
            Some(ref c) => c.configuration.codec_name(),
            None => "HEVC",
        }.to_string();
        let frame_rate = match config {
            Some(ref c) => c.configuration.frame_rate(),
            None => 0f64,
        };
        VideoTrackInfo {
            codec_id,
            codec_name: Some(codec_name.to_string()),
            width: self.width,
            height: self.height,
            frame_rate,
        }
    }
}

def_box! {
    /// HEVC Configuration box.
    ///
    /// Contains just the HEVCDecoderConfigurationRecord.
    HEVCConfigurationBox {
        configuration:  HEVCDecoderConfigurationRecord,
    },
    fourcc => "hvcC",
    version => [], 
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

// aligned(8) class HEVCDecoderConfigurationRecord
// {
//     unsigned int(8) configurationVersion = 1;
//     unsigned int(2) general_profile_space;
//     unsigned int(1) general_tier_flag;
//     unsigned int(5) general_profile_idc;
//     unsigned int(32) general_profile_compatibility_flags;
//     unsigned int(48) general_constraint_indicator_flags;
//     unsigned int(8) general_level_idc;
//     bit(4) reserved = ‘1111’b;
//     unsigned int(12) min_spatial_segmentation_idc;
//     bit(6) reserved = ‘111111’b;
//     unsigned int(2) parallelismType;
//     bit(6) reserved = ‘111111’b;
//     unsigned int(2) chroma_format_idc;
//     bit(5) reserved = ‘11111’b;
//     unsigned int(3) bit_depth_luma_minus8;
//     bit(5) reserved = ‘11111’b;
//     unsigned int(3) bit_depth_chroma_minus8;
//     bit(16) avgFrameRate;
//     bit(2) constantFrameRate;
//     bit(3) numTemporalLayers;
//     bit(1) temporalIdNested;
//     unsigned int(2) lengthSizeMinusOne;
//     unsigned int(8) numOfArrays;
//     for (j=0; j < numOfArrays; j++)
//     {
//         bit(1) array_completeness;
//         unsigned int(1) reserved = 0;
//         unsigned int(6) NAL_unit_type;
//         unsigned int(16) numNalus;
//         for (i=0; i< numNalus; i++)
//         {
//             unsigned int(16) nalUnitLength;
//             bit(8*nalUnitLength) nalUnit;
//         }
//     }
// }
// 
def_struct! {
    /// HEVC Decoder Configuration Record.
    HEVCDecoderConfigurationRecord,
        configuration_version: u8,
        // 2 bits: profile_space, 1 bit: tier_flags, lower 5 bits: profile_indication
        profile_flags: u8,
        profile_compatibility: u32,
        constraint_indicator_flags_hi: u16,
        constraint_indicator_flags_lo: u32,
        level_indication: u8,
        // top 4 bits reserved: '1111'
        min_spatial_segmentation_idc: u16,
        // top 6 bits: reserved: '111111'
        parallelism_ype: u8,
        // top 6 bits: reserved '111111'
        chroma_format_idc: u8,
        // top 5 bits: '11111', lower 3 bits: bitDepthLumaMinus8
        bit_depth_luma_minus8: u8,
        // op 5 bits: '11111', lower 3 bits: bitDepthLumaMinus8
        bit_depth_chroma_minus8: u8,
        // average frame rate in units of frames/(256 seconds)
        avg_frame_rate: u16,
        // 2 bits: constantFrameRate
        // - 0: unknown
        // - 1: the stream to which this configuration record applies is of constant frame rate
        // - 2: the representation of each temporal layer in the stream is of constant frame rate.
        // 3 bits: numTemporalLayers
        // 1 bit: temporalIdNested
        // 2 bits: lengthSizeMinusOne
        various: u8,
        // SPS, PPS, APS, SEI.
        data: Data,
}

impl HEVCDecoderConfigurationRecord {

    /// Return human name of codec, like "Baseline" or "High".
    pub fn codec_name(&self) -> &'static str {
        match self.profile_flags {
            _ => "HEVC",
        }
    }

    /// Return codec id.
    ///
    /// - 'hev1.' or 'hvc1.' prefix (5 chars)
    /// - profile, e.g. '.A12' (max 4 chars)
    /// - profile_compatibility, dot + 32-bit hex number (max 9 chars)
    /// - tier and level, e.g. '.H120' (max 5 chars)
    /// - up to 6 constraint bytes, bytes are dot-separated and hex-encoded.
    ///
    /// This doesn't appear to be right though.
    pub fn codec_id_as_docced(&self) -> String {
        let profile_space = (self.profile_flags & 0b11100000) >> 5;
        let profile_indication = self.profile_flags & 0b00000111;
        let tier = self.profile_flags & 0x20;
        format!("hcv1.{}{:02x}.{:x}.{}{}",
            (profile_space + 65) as char,
            profile_indication,
            self.profile_compatibility,
            if tier == 0 { 'L' } else { 'H' },
            self.level_indication,
        )
    }

    /// Return codec id.
    ///
    /// - 'hev1.' ' prefix (5 chars) [wrong, should either be hcv1 or hev1!!]
    /// - profile, e.g. '1'
    /// - profile_compatibility, dot + 32-bit hex number (max 9 chars)
    /// - tier and level, e.g. '.H120' (max 5 chars)
    /// - up to 6 constraint bytes, bytes are dot-separated and hex-encoded: 'B0'.
    ///
    /// FIXME: I'm not sure how a codec string for HECV should be constructed.
    /// This seems to work, somewhat, but it creates strings like:
    ///
    /// hev1.1.60000000.L120.B0,ac-3
    ///
    /// While you would expect
    ///
    /// hev1.1.6.L123.B0,ac-3
    ///
    /// If anyone can find the documentation, please contact me. Seems
    /// related to https://www.w3.org/TR/webcodecs-hevc-codec-registration/ .
    ///
    pub fn codec_id(&self) -> String {
        let profile_space = (self.profile_flags & 0b11100000) >> 5;
        let profile_indication = self.profile_flags & 0b00000111;
        let tier = self.profile_flags & 0x20;
        format!("hev1.{:x}.{:x}.{}{}.B0",
            profile_indication,
            self.profile_compatibility,
            if tier == 0 { 'L' } else { 'H' },
            self.level_indication,
        )
    }

    /// Decode the frame rate.
    pub fn frame_rate(&self) -> f64 {
        /*
         debugging to get this complete
        match crate::boxes::avcc::ParameterSet::parse_hevc(&self.data.0) {
            Ok(sets) => {
                println!("{:?}", sets);
                println!("{:?}", sets.sequence_parameters_sets());
            },
            Err(e) => println!("could not decode ParameterSets: {}", e),
        };
        */
        // avg_frame_rate is in frames / (256 secs), so not accurate enough
        // to describe 23.976 etc. Fix that up.
        match self.avg_frame_rate {
            6137|6138 => 23.976,
            7672|7673|7674 => 29.97,
            15343|15344|15345 => 59.94,
            other => (other as f64) / 256.0,
        }
    }
}

// The hvcC 'data' member consists of arrays of ParameterSets.
//
// The timing information we need to get an accurate framerate
// is hidden either in SPS:VUI (Sequence ParameterSet) or in
// VPS (Video Parameter Set).
//
#[derive(Debug, Default)]
pub(crate) struct ParameterSet<'a> {
    vps: Vec<&'a [u8]>,
    sps: Vec<&'a [u8]>,
}

impl<'a> ParameterSet<'a> {
    #[allow(dead_code)]
    pub(crate) fn parse(data: &'a [u8]) -> io::Result<ParameterSet<'a>> {

        if data.len() < 2 {
            return Err(ioerr!(UnexpectedEof, "ParameterSet::parse_hevc: EOF (1)"));
        }

        let mut set = ParameterSet::default();
        let num_arrays = data[0];
        let mut idx: usize = 1;

        for _ in 0 .. num_arrays {
            if idx + 3 >= data.len() {
                return Err(ioerr!(UnexpectedEof, "ParameterSet::parse_hevc: EOF (2)"));
            }
            let nalu_type = data[idx] & 0x3f;
            let num_nalus = u16::from_be_bytes([data[idx + 1], data[idx + 2]]) as usize;
            idx += 3;
            for _ in 0 .. num_nalus {
                if idx + 2 >= data.len() {
                    return Err(ioerr!(UnexpectedEof, "ParameterSet::parse_hevc: EOF (3)"));
                }
                let nalu_len = u16::from_be_bytes([data[idx], data[idx + 1]]) as usize;
                idx += 2;
                if idx + nalu_len > data.len() {
                    return Err(ioerr!(UnexpectedEof, "ParameterSet::parse_hevc: EOF (4)"));
                }
                // FIXME: unescape, so 00 00 03 01 -> 00 00 01
                let nalu = &data[idx .. idx + nalu_len];
                match nalu_type {
                    32 => set.vps.push(nalu),
                    33 => set.sps.push(nalu),
                    _ => {},
                }
                idx += nalu_len;
            }
        }

        Ok(set)
    }

    // Decode the SequenceParametersSets.
    #[allow(dead_code)]
    pub(crate) fn sequence_parameters_sets(&self) -> io::Result<Vec<SeqParameterSet>> {
        let mut v = Vec::new();
        for sps in &self.sps {
            if sps.len() < 4 {
                continue;
            }
            // H.265 NALU header:
            //
            // int(1) forbidden_zero_bits (always 0)
            // int(6) nal_unit_type
            // int(6) nuh_reserved_zero_6bits
            // int(3) nuh_temporal_id_plus1
            //
            // let's just skip it.
            let mut reader = BitReader::new(&sps[2..]);
            let parsed = SeqParameterSet::read(&mut reader)?;
            v.push(parsed);
        }
        Ok(v)
    }
}

/// H.265 SPS.
pub struct SeqParameterSet;

impl SeqParameterSet {
    // TODO: implement h.265 VPS / SPS parser.
    #[allow(dead_code)]
    pub(crate) fn read(_reader: &mut BitReader) -> io::Result<SeqParameterSet> {
        unimplemented!()
    }
}

