//! Tracks.
//!
use std::fmt::{self, Debug, Display};
use std::time::Duration;

use serde::Serialize;

use crate::boxes::*;
use crate::mp4box::{BoxInfo, MP4};
use crate::types::*;

pub use crate::sample_info::*;

/// General track information.
#[derive(Debug, Default, Serialize)]
pub struct TrackInfo {
    pub id:             u32,
    pub track_type:     String,
    pub duration:       Duration,
    pub size:           u64,
    pub language:       IsoLanguageCode,
    pub specific_info:  SpecificTrackInfo,
}

/// Track-type specific info.
#[derive(Serialize)]
#[serde(untagged)]
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

impl Display for SpecificTrackInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            &SpecificTrackInfo::AudioTrackInfo(ref i) => Display::fmt(i, f),
            &SpecificTrackInfo::VideoTrackInfo(ref i) => Display::fmt(i, f),
            &SpecificTrackInfo::SubtitleTrackInfo(ref i) => Display::fmt(i, f),
            &SpecificTrackInfo::UnknownTrackInfo(ref i) => Display::fmt(i, f),
        }
    }
}

/// Audio track details.
#[derive(Debug, Default, Serialize)]
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

impl Display for AudioTrackInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}.{})", self.codec_id, self.channel_count, self.lfe_channel as u8)
    }
}

/// Video track details.
#[derive(Debug, Default, Serialize)]
pub struct VideoTrackInfo {
    pub codec_id:   String,
    pub codec_name: Option<String>,
}

impl Display for VideoTrackInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.codec_id)?;
        if let Some(name) = self.codec_name.as_ref() {
            write!(f, " ({})", name)?;
        }
        Ok(())
    }
}

/// Subtitle track details.
#[derive(Debug, Default, Serialize)]
pub struct SubtitleTrackInfo {
    pub codec_id:   String,
    pub codec_name: Option<String>,
}

impl Display for SubtitleTrackInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.codec_id)?;
        if let Some(name) = self.codec_name.as_ref() {
            write!(f, " ({})", name)?;
        }
        Ok(())
    }
}

/// Unknown track type.
#[derive(Debug, Default, Serialize)]
pub struct UnknownTrackInfo {
    pub codec_id:   String,
    pub codec_name: Option<String>,
}

impl Display for UnknownTrackInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown")
    }
}

/// Extract general track information for all tracks in the movie.
pub fn track_info(mp4: &MP4) -> Vec<TrackInfo> {
    let mut v = Vec::new();

    let movie = mp4.movie();
    let mvhd = movie.movie_header();

    for track in &movie.tracks() {
        let mut info = TrackInfo::default();

        let tkhd = track.track_header();
        info.id = tkhd.track_id;
        info.duration = Duration::from_millis((1000 * tkhd.duration.0) / (mvhd.timescale as u64));

        let mdia = track.media();

        let mdhd = mdia.media_header();
        info.duration = Duration::from_millis((1000 * mdhd.duration.0) / (mdhd.timescale as u64));
        info.language = mdhd.language;

        let hdlr = mdia.handler();
        info.track_type = hdlr.handler_type.to_string();

        let stbl = mdia.media_info().sample_table();
        info.size = stbl.sample_size().iter().fold(0, |acc: u64, sz| acc + sz as u64);

        let stsd = stbl.sample_description();
        if let Some(avc1) = first_box!(stsd.entries, AvcSampleEntry) {
            info.specific_info = SpecificTrackInfo::VideoTrackInfo(avc1.track_info());
        } else if let Some(ac3) = first_box!(stsd.entries, Ac3SampleEntry) {
            info.specific_info = SpecificTrackInfo::AudioTrackInfo(ac3.track_info());
        } else if let Some(aac) = first_box!(stsd.entries, AacSampleEntry) {
            info.specific_info = SpecificTrackInfo::AudioTrackInfo(aac.track_info());
        } else {
            let id = stsd.entries.iter().next().map(|e| e.fourcc().to_string()).unwrap_or("unkn".to_string());
            let sp_info = match id.as_str() {
                "tx3g" => {
                    SpecificTrackInfo::SubtitleTrackInfo(SubtitleTrackInfo {
                        codec_id: id.to_string(),
                        codec_name: Some(String::from("3GPP Timed Text")),
                    })
                },
                "stpp"|"sbtt" => {
                    SpecificTrackInfo::SubtitleTrackInfo(SubtitleTrackInfo {
                        codec_id: id.to_string(),
                        codec_name: None,
                    })
                },
                _ => {
                    SpecificTrackInfo::UnknownTrackInfo(UnknownTrackInfo {
                        codec_id: id,
                        codec_name: None,
                    })
                },
            };
            info.specific_info = sp_info;
        }

        v.push(info)
    }

    v
}
