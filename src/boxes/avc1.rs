//
// ISO/IEC 14496-12:2015(E)
// 8.5.2 Sample Description Box 
//

use std::io;
use crate::serialize::{FromBytes, ToBytes, ReadBytes, WriteBytes};
use crate::mp4box::BoxInfo;
use crate::boxes::MP4Box;
use crate::types::*;
use crate::track::VideoTrackInfo;

def_box! {
    /// AVC sample entry (VideoSampleEntry).
    AvcSampleEntry, "avc1",
        skip:                   6,
        data_reference_index:   u16,
        skip:                   16,
        wirdth:                 u16,
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
        // avcC and other boxes (pasp?)
        sub_boxes:              [MP4Box, unsized],
}

impl Default for AvcSampleEntry {
    fn default() -> Self {
        AvcSampleEntry {
            data_reference_index:     0,
            wirdth:                   1280,
            height:                   720,
            _video_horizontal_dpi:    FixedFloat16_16::from(72f64),
            _video_vertical_dpi:      FixedFloat16_16::from(72f64),
            _video_frame_count:      1,
            video_pixel_depth:       24,
            _pre_defined:            0xffff,
            sub_boxes:                Vec::new(),
        }
    }
}

impl AvcSampleEntry {
    /// Return video specific track info.
    pub fn track_info(&self) -> VideoTrackInfo {
        let config = first_box!(self.sub_boxes, AvcConfigurationBox);
        let codec_id = match config {
            Some(ref a) => a.configuration.codec_id(),
            None => "avc1.unknown".to_string(),
        };
        let codec_name = match config {
            Some(ref a) => a.configuration.codec_name(),
            None => "AVC",
        }.to_string();
        VideoTrackInfo {
            codec_id,
            codec_name: Some(codec_name.to_string()),
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
    pub fn codec_name(&self) -> &'static str {
        match self.profile_idc {
            0x2c => "AVC CAVLC 4:4:4",
            0x42 => "AVC Baseline",
            0x4d => "AVC Main",
            0x58 => "AVC Extended",
            0x64 => "AVC High",
            0x6e => "AVC High 10",
            0x7a => "AVC High 4:2:2",
            0xf4 => "AVC High 4:4:4",

            0x53 => "AVC Scalable Baseline",
            0x56 => "AVC Scalable High",

            0x76 => "AVC Multiview High",
            0x80 => "AVC Stereo High",
            0x8a => "AVC Multiview Depth High",
            _ => "AVC",
        }
    }

    /// Return codec id as avc1.4d401f
    pub fn codec_id(&self) -> String {
        format!("avc1.{:02x}{:02x}{:02x}",
                    self.profile_idc, self.constraint_set_flags, self.level_idc)
    }
}

/// delegated to AvcDecoderConfigurationRecord::codec_id().
impl std::fmt::Display for AvcDecoderConfigurationRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.codec_id())
    }
}

