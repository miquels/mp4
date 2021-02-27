//! On the fly HLS / DASH packaging.
//!
use std::collections::HashMap;
use std::cmp;
use std::fmt::Display;
use std::fmt::Write;
use std::io;

use crate::mp4box::MP4;
use crate::types::IsoLanguageCode;
use crate::track::SpecificTrackInfo;

struct ExtXMedia {
    type_:   &'static str,
    group_id:   String,
    name:       String,
    language:   Option<&'static str>,
    auto_select: bool,
    default:    bool,
    uri:        String,
}

impl Display for ExtXMedia {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#EXT-X-MEDIA:TYPE={},", self.type_)?;
        write!(f, r#"GROUP-ID="{}","#, self.group_id)?;
        if let Some(ref lang) = self.language {
            write!(f, r#"LANGUAGE="{}","#, lang)?;
        }
        write!(f, r#"NAME="{}","#, self.name)?;
        write!(f, r#"AUTOSELECT="{}","#, if self.auto_select { "YES" } else { "NO" })?;
        write!(f, r#"DEFAULT="{}","#, if self.default { "YES" } else { "NO" })?;
        write!(f, r#"URI="{}","#, self.uri)?;
        write!(f, "\n")
    }
}

#[derive(Default)]
struct ExtXStreamInf {
    audio:  Option<String>,
    avg_bandwidth: Option<u64>,
    bandwidth:  u64,
    codecs: Vec<String>,
    resolution: (u16, u16),
    frame_rate: f64,
    uri: String,
}

impl Display for ExtXStreamInf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#EXT-X-STREAM-INF:")?;
        if let Some(ref audio) = self.audio {
            write!(f, r#"AUDIO="{}","#, audio)?;
        }
        if let Some(ref avg_bw) = self.avg_bandwidth {
            write!(f, r#"AVERAGE-BANDWIDTH={},"#, avg_bw)?;
        }
        write!(f, r#"BANDWIDTH={},"#, self.bandwidth)?;
        let mut codecs = String::new();
        for codec in &self.codecs {
            if codecs.len() > 0 {
                codecs += ",";
            }
            codecs += codec;
        }
        write!(f, r#"CODECS="{}","#, codecs)?;
        write!(f, r#"RESOLUTION={}x{},"#, self.resolution.0, self.resolution.1)?;
        write!(f, r#"FRAME-RATE="{}","#, self.frame_rate)?;
        write!(f, "\n{}\n", self.uri)
    }
}

fn lang(lang: IsoLanguageCode) -> (&'static str, &'static str) {
    match lang.to_string().as_str() {
        "eng" => ("en", "English"),
        "dut" => ("nl", "Nederlands"),
        "fra" => ("fr", "Français"),
        "ger" => ("de", "German"),
        "spa" => ("es", "Español"),
        "afr" => ("za", "Afrikaans"),
        other => {
            if let Some(lang) = isolang::Language::from_639_3(other) {
                if let Some(short) = lang.to_639_1() {
                    (short, lang.to_name())
                } else {
                    ("--", "Und")
                }
            } else {
                ("--", "Und")
            }
        }
    }
}

/// Generate a HLS playlist.
pub fn hls_master(mp4: &MP4) -> String {

    let mut m = String::new();
    m += "#EXTM3U\n";
    m += "# Created by mp4lib.rs\n";
    m += "#\n";
    m += "#EXT-X-VERSION:6\n";
    m += "\n";

    let mut audio_codecs = HashMap::new();

    // Audio tracks.
    for track in crate::track::track_info(mp4).iter() {
        let info = match &track.specific_info {
            SpecificTrackInfo::AudioTrackInfo(info) => info,
            _ => continue,
        };
        if audio_codecs.len() == 0 {
            m += "# AUDIO\n";
        }

        let avg_bw = track.size / cmp::max(1, track.duration.as_secs());
        if let Some(entry) = audio_codecs.get_mut(&info.codec_id) {
            if avg_bw > *entry {
                *entry = avg_bw;
            }
        } else {
            audio_codecs.insert(info.codec_id.clone(), avg_bw);
        }

        let (lang, name) = lang(track.language);
        let mut name = name.to_string();
        if info.channel_count >= 3 {
            name += &format!(" ({}.{})", info.channel_count, info.lfe_channel as u8);
        }

        let audio = ExtXMedia {
            type_: "AUDIO",
            group_id: format!("audio/{}", info.codec_id),
            language: Some(lang),
            name: name,
            auto_select: true,
            default: true,
            uri: format!("audio.{}.m3u8", track.id),
        };

        let _ = write!(m, "{}", audio);
    }
    m += "\n# VIDEO\n";

    // video track.
    for track in crate::track::track_info(mp4).iter() {
        let info = match &track.specific_info {
            SpecificTrackInfo::VideoTrackInfo(info) => info,
            _ => continue,
        };
        let avg_bw = track.size / cmp::max(1, track.duration.as_secs());
        let mut ext = ExtXStreamInf {
            bandwidth: avg_bw,
            codecs: vec![ info.codec_id.clone() ],
            resolution: (info.width, info.height),
            frame_rate: info.frame_rate,
            uri: format!("video.{}.m3u8", track.id),
            .. ExtXStreamInf::default()
        };
        for (audio_codec, audio_bw) in audio_codecs.iter() {
            ext.audio = Some(format!("audio/{}", audio_codec));
            ext.bandwidth = avg_bw + audio_bw;
            ext.codecs = vec![ info.codec_id.clone(), audio_codec.to_string() ];
            let _ = write!(m, "{}", ext);
        }
        if audio_codecs.len() == 0 {
            let _ = write!(m, "{}", ext);
        }
    }

    m
}

pub fn hls_track(mp4: &MP4, track_id: u32) -> io::Result<String> {

    let track = mp4.movie().track_by_id(track_id).ok_or_else(|| ioerr!(NotFound, "track not found"))?;
    let segments = crate::segment::track_to_segments(track, None)?;
    let longest = segments.iter().fold(0u32, |l, s| std::cmp::max((s.duration + 0.5) as u32, l));
    let independent = true;

    let mut m = String::new();
    m += "#EXTM3U\n";
    m += "# Created by mp4lib.rs\n";
    m += "#\n";
    m += "#EXT-X-VERSION:6\n";
    if independent {
        m += "#EXT-X-INDEPENDENT-SEGMENTS\n";
    }
    m += &format!("EXT-X-TARGETDURATION:{}\n", longest);
    m += "#EXT-X-MEDIA-SEQUENCE:0\n";
    m += &format!(r#"#EXT-X-MAP:URI="t.{}.init"\n"#, track_id);

    for seg in &segments {
        m += &format!("#EXTINF:{},\nt.1.{}-{}.mp4\n", seg.duration, seg.start_sample, seg.end_sample);
    }
    m += "#EXT-X-ENDLIST\n";

    Ok(m)
}
