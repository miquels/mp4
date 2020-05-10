use std::fmt::Debug;
use std::time::Duration;

use crate::boxes::*;
use crate::mp4box::BoxInfo;
use crate::types::*;

/// General track information.
#[derive(Debug, Default)]
pub struct TrackInfo {
    pub id:             u32,
    pub track_type:     String,
    pub duration:       Duration,
    pub language:       IsoLanguageCode,
    pub specific_info:  SpecificTrackInfo,
}

pub enum SpecificTrackInfo {
    AudioTrackInfo(AudioTrackInfo),
    VideoTrackInfo(VideoTrackInfo),
    SubtitleTrackInfo(SubtitleTrackInfo),
    UnknownTrackInfo(UnknownTrackInfo),
}

impl Default for SpecificTrackInfo {
    fn default() -> SpecificTrackInfo {
        SpecificTrackInfo::UnknownTrackInfo(UnknownTrackInfo {
            codec_id: "und".to_string(),
            codec_name: None,
        })
    }
}

impl Debug for SpecificTrackInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            &SpecificTrackInfo::AudioTrackInfo(ref i) => Debug::fmt(i, f),
            &SpecificTrackInfo::VideoTrackInfo(ref i) => Debug::fmt(i, f),
            &SpecificTrackInfo::SubtitleTrackInfo(ref i) => Debug::fmt(i, f),
            &SpecificTrackInfo::UnknownTrackInfo(ref i) => Debug::fmt(i, f),
        }
    }
}

#[derive(Debug, Default)]
pub struct AudioTrackInfo {
    pub codec_id:   String,
    pub codec_name: Option<String>,
    pub channel_count:   u16,
    pub lfe_channel:    bool,
    pub bit_depth:  Option<u16>,
    pub sample_rate:    Option<u32>,
    pub channel_configuration:  Option<String>,
    pub avg_bitrate:   Option<u32>,
    pub max_bitrate:   Option<u32>,
}

#[derive(Debug, Default)]
pub struct VideoTrackInfo {
    pub codec_id:   String,
    pub codec_name: Option<String>,
}

#[derive(Debug, Default)]
pub struct SubtitleTrackInfo {
    pub codec_id:   String,
    pub codec_name: Option<String>,
}

#[derive(Debug, Default)]
pub struct UnknownTrackInfo {
    pub codec_id:   String,
    pub codec_name: Option<String>,
}

macro_rules! pick_or {
    ($e:expr, $($tt:tt)+) => {
        match $e {
            Some(v) => v,
            None => $($tt)+,
        }
    };
}

/// Extract general track information for all tracks in the movie.
pub fn track_info(base: &[MP4Box]) -> Vec<TrackInfo> {
    let mut v = Vec::new();

    let moov = pick_or!(first_box!(&base, MovieBox), return v);
    let mvhd = pick_or!(first_box!(moov, MovieHeaderBox), return v);

    for track in iter_box!(moov, TrackBox) {
        let mut info = TrackInfo::default();

        let tkhd = pick_or!(first_box!(track, TrackHeaderBox), continue);
        info.id = tkhd.track_id;
        info.duration = Duration::from_millis((1000 * tkhd.duration.0) / (mvhd.timescale as u64));

        let mdia = pick_or!(first_box!(track, MediaBox), continue);

        let mdhd = pick_or!(first_box!(mdia, MediaHeaderBox), continue);
        info.duration = Duration::from_millis((1000 * mdhd.duration.0) / (mdhd.timescale as u64));
        info.language = mdhd.language;

        let hdlr = pick_or!(first_box!(mdia, HandlerBox), continue);
        info.track_type = hdlr.handler_type.to_string();

        let stsd = pick_or!(
            first_box!(mdia, MediaInformationBox / SampleTableBox / SampleDescriptionBox),
            continue
        );

        if let Some(avc1) = first_box!(stsd.entries, AvcSampleEntry) {
            info.specific_info = SpecificTrackInfo::VideoTrackInfo(avc1.track_info());
        } else if let Some(ac3) = first_box!(stsd.entries, Ac3SampleEntry) {
            info.specific_info = SpecificTrackInfo::AudioTrackInfo(ac3.track_info());
        } else if let Some(aac) = first_box!(stsd.entries, AacSampleEntry) {
            info.specific_info = SpecificTrackInfo::AudioTrackInfo(aac.track_info());
        } else {
            let id = stsd.entries.iter().next().map(|e| e.fourcc().to_string()).unwrap_or("unkn".to_string());
            let u = UnknownTrackInfo {
                codec_id: id,
                codec_name: None,
            };
            info.specific_info = SpecificTrackInfo::UnknownTrackInfo(u);
        }

        v.push(info)
    }

    v
}
