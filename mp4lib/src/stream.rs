//! On the fly HLS / DASH packaging.
//!
use std::collections::HashMap;
use std::cmp;
use std::fmt::Display;
use std::fmt::Write;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use once_cell::sync::Lazy;
use scan_fmt::scan_fmt;

use crate::fragment::FragmentSource;
use crate::lru_cache::LruCache;
use crate::io::MemBuffer;
use crate::mp4box::MP4;
use crate::segment::Segment;
use crate::serialize::ToBytes;
use crate::types::{FourCC, IsoLanguageCode};
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
        write!(f, r#"FRAME-RATE="{:.03}","#, self.frame_rate)?;
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
            uri: format!("media.{}.m3u8", track.id),
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
            uri: format!("media.{}.m3u8", track.id),
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

fn track_to_segments(mp4: &MP4, track_id: u32, duration: Option<u32>) -> io::Result<Arc<Vec<Segment>>> {
    static SEGMENTS: Lazy<LruCache<(String, u32), Arc<Vec<Segment>>>> = {
        Lazy::new(|| LruCache::new(Duration::new(60, 0)))
    };
    let name = mp4.input_file.as_ref().ok_or_else(|| ioerr!(NotFound, "file not found"))?;
    let key = (name.to_string(), track_id);
    if let Some(segments) = SEGMENTS.get(&key) {
        return Ok(segments);
    }
    let track = mp4.movie().track_by_id(track_id).ok_or_else(|| ioerr!(NotFound, "track not found"))?;
    let segments = crate::segment::track_to_segments(track, duration)?;
    let segments = Arc::new(segments);
    SEGMENTS.put(key, segments.clone());
    Ok(segments)
}

pub fn hls_track(mp4: &MP4, track_id: u32) -> io::Result<String> {

    let movie = mp4.movie();
    let trak = movie.track_by_id(track_id).ok_or_else(|| ioerr!(NotFound, "track not found"))?;

    let video_id = match movie.track_idx_by_handler(FourCC::new("vide")) {
        Some(idx) => movie.tracks()[idx].track_id(),
        None => return Err(ioerr!(NotFound, "mp4 file has no video track")),
    };

    let mut segments = track_to_segments(mp4, video_id, None)?;
    if track_id != video_id {
        let segs: &[Segment] = segments.as_ref();
        segments = Arc::new(crate::segment::track_to_segments_timed(trak, segs)?);
    }

    let handler_type = trak.media().handler().handler_type;
    let (prefix, suffix) = match &handler_type.to_be_bytes()[..] {
        b"vide" => ('v', "mp4"),
        b"soun" => ('a', "m4a"),
        b"sbtl" => ('s', "mp4"),
        b"subt" => ('s', "mp4"),
        _ => return Err(ioerr!(InvalidInput, "unknown handler type {}", handler_type)),
    };

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
    m += &format!("#EXT-X-TARGETDURATION:{}\n", longest);
    m += "#EXT-X-MEDIA-SEQUENCE:0\n";
    m += &format!("#EXT-X-MAP:URI=\"init.{}.mp4\"\n", track_id);

    for (seq, seg) in segments.iter().enumerate() {
        m += &format!("#EXTINF:{},\n{}/c.{}.{}.{}-{}.{}\n", seg.duration, prefix, track_id, seq, seg.start_sample, seg.end_sample, suffix);
    }
    m += "#EXT-X-ENDLIST\n";

    Ok(m)
}

/// Translates the tail of an URL into a fMP4 init section or fragment.
///
/// - init.TRACK_ID.mp4 => initialization segment for track TRACK_ID.
///
/// - a/t.TRACK_ID.SEQUENCE.FROM_SAMPLE.TO_SAMPLE.m4a => audio moof + mdat
/// - v/t.TRACK_ID.SEQUENCE.FROM_SAMPLE.TO_SAMPLE.mp4 => video moof + mdat
///
/// Returns (mime-type, data).
pub fn fragment_from_uri(mp4: &MP4, url_tail: &str) -> io::Result<(&'static str, Vec<u8>)> {

    if let Ok(track_id) = scan_fmt!(url_tail, "init.{}.mp4{e}", u32) {
        let init = crate::fragment::media_init_section(&mp4, &[ track_id ]);
        let mut buffer = MemBuffer::new();
        init.write(&mut buffer)?;
        return Ok(("video/mp4", buffer.into_vec()));
    }

    match scan_fmt!(url_tail, "{[vas]}/c.{}.{}.{}-{}.", char, u32, u32, u32, u32) {
        Ok((tp, track_id, seq_id, start_sample, end_sample)) => {
            let mime = match tp {
                'v' => "video/mp4",
                'a' => "audio/mp4",
                's' => "application/mp4",
                _ => unreachable!(),
            };
            let fs = FragmentSource {
                src_track_id: track_id,
                dst_track_id: 1,
                from_sample:  start_sample,
                to_sample:    end_sample,
            };
            let frag = crate::fragment::movie_fragment(&mp4, seq_id, &[ fs ])?;
            let mut buffer = MemBuffer::new();
            frag.to_bytes(&mut buffer)?;
            Ok((mime, buffer.into_vec()))
        },
        Err(_) => Err(ioerr!(NotFound, "not found")),
    }
}
