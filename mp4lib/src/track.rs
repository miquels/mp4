//! Get some general information about the tracks in this movie.
//!
//! See also [`TrackBox`](crate::boxes::TrackBox)
//!
use std::fmt::{self, Debug, Display};
use std::time::Duration;

use serde::{Serialize, Serializer};

use crate::boxes::*;
use crate::mp4box::{BoxInfo, MP4};
use crate::types::*;

pub use crate::sample_info::*;

/// General track information.
#[derive(Debug, Default, Serialize)]
pub struct TrackInfo {
    pub id:            u32,
    pub track_type:    String,
    #[serde(serialize_with = "seconds")]
    pub duration:      Duration,
    pub size:          u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name:          Option<ZString>,
    #[serde(serialize_with = "display")]
    pub language:      IsoLanguageCode,
    pub specific_info: SpecificTrackInfo,
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
            codec_id:   "und".to_string(),
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
    pub codec_id:              String,
    pub codec_name:            Option<String>,
    pub channel_count:         u16,
    pub lfe_channel:           bool,
    pub bit_depth:             Option<u16>,
    pub sample_rate:           Option<u32>,
    pub channel_configuration: Option<String>,
    pub avg_bitrate:           Option<u32>,
    pub max_bitrate:           Option<u32>,
}

impl Display for AudioTrackInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}.{})",
            self.codec_id, self.channel_count, self.lfe_channel as u8
        )
    }
}

/// Video track details.
#[derive(Debug, Default, Serialize)]
pub struct VideoTrackInfo {
    pub codec_id:   String,
    pub codec_name: Option<String>,
    pub width:      u16,
    pub height:     u16,
    pub frame_rate: f64,
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

        info.name = first_box!(track, UserDataBox)
            .and_then(|b| first_box!(b, NameBox))
            .map(|n| n.name.clone());

        let stsd = stbl.sample_description();
        if let Some(avc1) = first_box!(stsd.entries, AvcSampleEntry) {
            let mut avc1_info = avc1.track_info();
            if avc1_info.frame_rate == 0f64 {
                let sample_count = stbl.sample_size().count;
                let timescale = std::cmp::max(1000, mdhd.timescale) as f64;
                let fr = sample_count as f64 / (mdhd.duration.0 as f64 / timescale);
                log::debug!("track::track_info: avcc.framerate == 0, estimate: {}", fr);
                avc1_info.frame_rate = fr;
            }
            avc1_info.frame_rate = (avc1_info.frame_rate * 1000.0).round() / 1000.0;
            info.specific_info = SpecificTrackInfo::VideoTrackInfo(avc1_info);
        } else if let Some(hevc) = first_box!(stsd.entries, HEVCSampleEntry) {
            let mut hevc_info = hevc.track_info();
            if hevc_info.frame_rate == 0f64 {
                let sample_count = stbl.sample_size().count;
                let timescale = std::cmp::max(1000, mdhd.timescale) as f64;
                let fr = sample_count as f64 / (mdhd.duration.0 as f64 / timescale);
                log::debug!("track::track_info: hvcc.framerate == 0, estimate: {}", fr);
                hevc_info.frame_rate = fr;
            }
            hevc_info.frame_rate = (hevc_info.frame_rate * 1000.0).round() / 1000.0;
            info.specific_info = SpecificTrackInfo::VideoTrackInfo(hevc_info);
        } else if let Some(ac3) = first_box!(stsd.entries, Ac3SampleEntry) {
            let mut ac3 = ac3.track_info();
            if ac3.avg_bitrate.is_none() && info.duration.as_secs() > 0 {
                ac3.avg_bitrate = Some((8 * info.size / info.duration.as_secs()) as u32);
            }
            info.specific_info = SpecificTrackInfo::AudioTrackInfo(ac3);
        } else if let Some(aac) = first_box!(stsd.entries, AacSampleEntry) {
            let mut aac = aac.track_info();
            if aac.avg_bitrate.is_none() && info.duration.as_secs() > 0 {
                aac.avg_bitrate = Some((8 * info.size / info.duration.as_secs()) as u32);
            }
            info.specific_info = SpecificTrackInfo::AudioTrackInfo(aac);
        } else {
            let id = stsd
                .entries
                .iter()
                .next()
                .map(|e| e.fourcc().to_string())
                .unwrap_or("unkn".to_string());
            let sp_info = match id.as_str() {
                "tx3g" => {
                    SpecificTrackInfo::SubtitleTrackInfo(SubtitleTrackInfo {
                        codec_id:   id.to_string(),
                        codec_name: Some(String::from("3GPP Timed Text")),
                    })
                },
                "stpp" | "sbtt" => {
                    SpecificTrackInfo::SubtitleTrackInfo(SubtitleTrackInfo {
                        codec_id:   id.to_string(),
                        codec_name: None,
                    })
                },
                _ => {
                    SpecificTrackInfo::UnknownTrackInfo(UnknownTrackInfo {
                        codec_id:   id,
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

// Serialize helper.
fn display<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: Display,
    S: Serializer,
{
    serializer.collect_str(value)
}

// Serialize helper.
fn seconds<S>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_f64(value.as_millis() as f64 / 1000.0)
}
