//
// ISO/IEC 14496-12:2015(E)
// 8.5.2 Sample Description Box 
//

use std::io;

use crate::boxes::avcc::AvcConfigurationBox;
use crate::boxes::prelude::*;
use crate::track::VideoTrackInfo;

def_box! {
    /// AVC sample entry (VideoSampleEntry).
    AvcSampleEntry {
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
        // avcC and other boxes (pasp?)
        boxes:              Vec<MP4Box>,
    },
    fourcc => "avc1",
    version => [], 
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

impl Default for AvcSampleEntry {
    fn default() -> Self {
        AvcSampleEntry {
            data_reference_index:     1,
            width:                   1280,
            height:                   720,
            _video_horizontal_dpi:    FixedFloat16_16::from(72f64),
            _video_vertical_dpi:      FixedFloat16_16::from(72f64),
            _video_frame_count:      1,
            video_pixel_depth:       24,
            _pre_defined:            0xffff,
            boxes:                Vec::new(),
        }
    }
}

impl AvcSampleEntry {
    /// Return video specific track info.
    pub fn track_info(&self) -> VideoTrackInfo {
        let config = first_box!(self.boxes, AvcConfigurationBox);
        let codec_id = match config {
            Some(ref c) => c.configuration.codec_id(),
            None => "avc1.unknown".to_string(),
        };
        let codec_name = match config {
            Some(ref c) => c.configuration.codec_name(),
            None => "AVC",
        }.to_string();
        let frame_rate = match config {
            Some(ref c) => c.configuration.frame_rate().unwrap_or(Some(0f64)).unwrap_or(0f64),
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

