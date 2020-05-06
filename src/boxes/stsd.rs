//
// ISO/IEC 14496-12:2015(E)
// 8.5.2 Sample Description Box 
//

use std::io;
use crate::serialize::{FromBytes, ToBytes, ReadBytes, WriteBytes};
use crate::mp4box::{BoxInfo, FullBox};
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
        video_color_table_id:   u16,
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
            _video_pixel_depth:       24,
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

