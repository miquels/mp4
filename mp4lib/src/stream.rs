//! On the fly HLS / DASH packaging.
//!
use std::cmp;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::fmt::Write;
use std::io;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use once_cell::sync::Lazy;
use scan_fmt::scan_fmt;

use crate::fragment::FragmentSource;
use crate::io::MemBuffer;
use crate::lru_cache::LruCache;
use crate::mp4box::MP4;
use crate::segment::Segment;
use crate::serialize::ToBytes;
use crate::subtitle::Format;
use crate::track::SpecificTrackInfo;
use crate::types::FourCC;

struct ExtXMedia {
    type_:       &'static str,
    group_id:    String,
    name:        String,
    channels:    Option<u16>,
    language:    Option<&'static str>,
    auto_select: bool,
    default:     bool,
    uri:         String,
}

impl Display for ExtXMedia {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#EXT-X-MEDIA:TYPE={},", self.type_)?;
        write!(f, r#"GROUP-ID="{}","#, self.group_id)?;
        if let Some(ref channels) = self.channels {
            write!(f, r#"CHANNELS="{}","#, channels)?;
        }
        if let Some(ref lang) = self.language {
            write!(f, r#"LANGUAGE="{}","#, lang)?;
        }
        write!(f, r#"NAME="{}","#, self.name)?;
        write!(
            f,
            r#"AUTOSELECT={},"#,
            if self.auto_select { "YES" } else { "NO" }
        )?;
        write!(f, r#"DEFAULT={},"#, if self.default { "YES" } else { "NO" })?;
        write!(f, r#"URI="{}""#, self.uri)?;
        write!(f, "\n")
    }
}

#[derive(Default)]
struct ExtXStreamInf {
    audio:         Option<String>,
    avg_bandwidth: Option<u64>,
    bandwidth:     u64,
    codecs:        Vec<String>,
    subtitles:     bool,
    resolution:    (u16, u16),
    frame_rate:    f64,
    uri:           String,
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
        if self.subtitles {
            write!(f, r#"SUBTITLES="subs","#)?;
        }
        write!(f, r#"CODECS="{}","#, codecs)?;
        write!(f, r#"RESOLUTION={}x{},"#, self.resolution.0, self.resolution.1)?;
        write!(f, r#"FRAME-RATE={:.03}"#, self.frame_rate)?;
        write!(f, "\n{}\n", self.uri)
    }
}

fn lang(lang: &str) -> (Option<&'static str>, &'static str) {
    // shortcut for known language tags, with localized name.
    match lang {
        "en" | "eng" => return (Some("en"), "English"),
        "nl" | "dut" | "nld" => return (Some("nl"), "Nederlands"),
        "fr" | "fra" | "fre" => return (Some("fr"), "Français"),
        "de" | "ger" => return (Some("de"), "German"),
        "es" | "spa" => return (Some("es"), "Español"),
        "za" | "afr" | "zaf" => return (Some("za"), "Afrikaans"),
        _ => {},
    }

    use isolang::Language;

    // first look up 2-letter or 3-letter language code.
    let language = match lang.len() {
        2 => Language::from_639_1(lang),
        3 => Language::from_639_3(lang),
        _ => return (None, "Undetermined"),
    };

    // Did we succeed?
    let language = match language {
        Some(l) => l,
        None => return (None, "Undetermined"),
    };

    // use 2-letter code if it exists, otherwise 3-letter code.
    let code = language.to_639_1().unwrap_or(language.to_639_3());

    (Some(code), language.to_name())
}

fn lang_from_path(path: &str) -> &str {
    let fields: Vec<_> = path.split('.').collect();
    if fields.len() < 3 {
        return "und";
    }
    let mut idx = fields.len() - 2;
    if fields[idx] == "forced" || fields[idx] == "sdh" {
        idx -= 1;
    }
    if idx > 0 {
        fields[idx]
    } else {
        "und"
    }
}

/// Generate a HLS playlist.
///
/// You can pass in external subtitles using the `subs` argument.
/// The subtitle path needs to be relative, just a filename, and
/// the file needs to be in the same subdir as the mp4 file.
pub fn hls_master(mp4: &MP4, subs: Option<&Vec<String>>) -> String {
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

        let (lang, name) = lang(&track.language.to_string());
        let mut name = name.to_string();
        if info.channel_count >= 3 {
            name += &format!(" ({}.{})", info.channel_count, info.lfe_channel as u8);
        }

        let audio = ExtXMedia {
            type_:       "AUDIO",
            group_id:    format!("audio/{}", info.codec_id),
            language:    lang,
            channels:    Some(info.channel_count + info.lfe_channel as u16),
            name:        name,
            auto_select: true,
            default:     false,
            uri:         format!("media.{}.m3u8", track.id),
        };

        let _ = write!(m, "{}", audio);
    }

    // Subtitle tracks.
    let mut sublang = HashSet::new();

    if let Some(subs) = subs {
        for sub in subs {
            // look up language and language code.
            let language = lang_from_path(sub);
            let (lang, name) = lang(language);

            // no duplicates.
            if sublang.contains(name) {
                continue;
            }

            if sublang.is_empty() {
                m += "\n# SUBTITLES\n";
            }
            sublang.insert(name.to_string());

            let dotdot = if sub.starts_with("/") { "" } else { "../" };
            let sub = ExtXMedia {
                type_:       "SUBTITLES",
                group_id:    "subs".to_string(),
                language:    lang,
                channels:    None,
                name:        name.to_string(),
                auto_select: true,
                default:     false,
                uri:         format!("{}{}:media.m3u8", dotdot, sub),
            };

            let _ = write!(m, "{}", sub);
        }
    }

    for track in crate::track::track_info(mp4).iter() {
        match &track.specific_info {
            SpecificTrackInfo::SubtitleTrackInfo(_) => {},
            _ => continue,
        }

        let (lang, name) = lang(&track.language.to_string());

        // no duplicates.
        if sublang.contains(name) {
            continue;
        }

        if sublang.is_empty() {
            m += "\n# SUBTITLES\n";
        }
        sublang.insert(name.to_string());

        let sub = ExtXMedia {
            type_:       "SUBTITLES",
            group_id:    "subs".to_string(),
            language:    lang,
            channels:    None,
            name:        name.to_string(),
            auto_select: true,
            default:     false,
            uri:         format!("media.{}.m3u8", track.id),
        };

        let _ = write!(m, "{}", sub);
    }

    // video track.
    m += "\n# VIDEO\n";
    for track in crate::track::track_info(mp4).iter() {
        let info = match &track.specific_info {
            SpecificTrackInfo::VideoTrackInfo(info) => info,
            _ => continue,
        };
        let avg_bw = track.size / cmp::max(1, track.duration.as_secs());
        let mut ext = ExtXStreamInf {
            bandwidth: avg_bw,
            codecs: vec![info.codec_id.clone()],
            resolution: (info.width, info.height),
            frame_rate: info.frame_rate,
            uri: format!("media.{}.m3u8", track.id),
            subtitles: sublang.len() > 0,
            ..ExtXStreamInf::default()
        };
        for (audio_codec, audio_bw) in audio_codecs.iter() {
            ext.audio = Some(format!("audio/{}", audio_codec));
            ext.bandwidth = avg_bw + audio_bw;
            ext.codecs = vec![info.codec_id.clone(), audio_codec.to_string()];
            let _ = write!(m, "{}", ext);
        }
        if audio_codecs.len() == 0 {
            let _ = write!(m, "{}", ext);
        }
    }

    m
}

fn track_to_segments(mp4: &MP4, track_id: u32, duration: Option<u32>) -> io::Result<Arc<Vec<Segment>>> {
    #[rustfmt::skip]
    static SEGMENTS: Lazy<LruCache<(String, u32), Arc<Vec<Segment>>>> = {
        Lazy::new(|| LruCache::new(Duration::new(60, 0)))
    };
    let name = mp4
        .input_file
        .as_ref()
        .ok_or_else(|| ioerr!(NotFound, "file not found"))?;
    let key = (name.to_string(), track_id);
    if let Some(segments) = SEGMENTS.get(&key) {
        return Ok(segments);
    }
    let track = mp4
        .movie()
        .track_by_id(track_id)
        .ok_or_else(|| ioerr!(NotFound, "track not found"))?;
    let segments = crate::segment::track_to_segments(track, duration)?;
    let segments = Arc::new(segments);
    SEGMENTS.put(key, segments.clone());
    Ok(segments)
}

pub fn hls_track(mp4: &MP4, track_id: u32) -> io::Result<String> {
    let movie = mp4.movie();
    let trak = movie
        .track_by_id(track_id)
        .ok_or_else(|| ioerr!(NotFound, "track not found"))?;
    let handler = trak.media().handler();
    let handler_type = handler.handler_type;

    let seg_duration = None; // Some(4000);

    let segments = if !handler.is_subtitle() {
        let video_id = match movie.track_idx_by_handler(FourCC::new("vide")) {
            Some(idx) => movie.tracks()[idx].track_id(),
            None => return Err(ioerr!(NotFound, "mp4 file has no video track")),
        };

        let mut segments = track_to_segments(mp4, video_id, seg_duration)?;
        if track_id != video_id {
            let segs: &[Segment] = segments.as_ref();
            segments = Arc::new(crate::segment::track_to_segments_timed(trak, segs)?);
        }
        segments
    } else {
        // Subtitles do not have the same number of segments and duration.
        // They are a master list, like the video.
        track_to_segments(mp4, track_id, None)?
    };

    let (prefix, suffix) = match &handler_type.to_be_bytes()[..] {
        b"vide" => ('v', "mp4"),
        b"soun" => ('a', "m4a"),
        b"sbtl" => ('s', "vtt"),
        b"subt" => ('s', "vtt"),
        _ => return Err(ioerr!(InvalidInput, "unknown handler type {}", handler_type)),
    };

    let longest = segments
        .iter()
        .fold(0u32, |l, s| std::cmp::max((s.duration + 0.5) as u32, l));
    let independent = seg_duration.is_none();

    let mut m = String::new();
    m += "#EXTM3U\n";
    m += "#EXT-X-VERSION:6\n";
    m += "## Created by mp4lib.rs\n";
    m += "#\n";
    if independent || handler.is_audio() {
        m += "#EXT-X-INDEPENDENT-SEGMENTS\n";
    }
    m += &format!("#EXT-X-TARGETDURATION:{}\n", longest);
    m += "#EXT-X-PLAYLIST-TYPE:VOD\n";
    if !handler.is_subtitle() {
        m += &format!("#EXT-X-MAP:URI=\"init.{}.mp4\"\n", track_id);
    }

    for (seq, seg) in segments.iter().enumerate() {
        // Skip segments that are < 0.1 ms.
        if seg.duration.partial_cmp(&0.0001) == Some(std::cmp::Ordering::Greater) {
            m += &format!(
                "#EXTINF:{},\n{}/c.{}.{}.{}-{}.{}\n",
                seg.duration,
                prefix,
                track_id,
                seq + 1,
                seg.start_sample,
                seg.end_sample,
                suffix
            );
        }
    }
    m += "#EXT-X-ENDLIST\n";

    Ok(m)
}

pub fn hls_subtitle(path: &str, duration: f64) -> String {
    let duration = (duration + 0.5).round();
    let mut m = String::new();
    let dotslash = if path.contains(":") { "./" } else { "" };
    m += "#EXTM3U\n";
    m += "#EXT-X-VERSION:6\n";
    m += "## Created by mp4lib.rs\n";
    m += "#\n";
    m += &format!("#EXT-X-TARGETDURATION:{}\n", duration);
    m += "#EXT-X-PLAYLIST-TYPE:VOD\n";
    m += &format!("#EXTINF:{}\n", duration);
    m += dotslash;
    m += path;
    m += "\n#EXT-X-ENDLIST\n";
    m
}

/// Translates the tail of an URL into a fMP4 init section or fragment.
///
/// - init.TRACK_ID.mp4 => initialization segment for track TRACK_ID.
/// - init.TRACK_ID.vtt => initialization segment for track TRACK_ID.
///
/// - a/c.TRACK_ID.SEQUENCE.FROM_SAMPLE.TO_SAMPLE.m4a => audio moof + mdat
/// - v/c.TRACK_ID.SEQUENCE.FROM_SAMPLE.TO_SAMPLE.mp4 => video moof + mdat
/// - s/c.TRACK_ID.SEQUENCE.FROM_SAMPLE.TO_SAMPLE.vtt => webvtt fragment
/// - e/PATH/TO/EXTERNAL/SUBTITLES/FILE/format.EXT
///
/// Returns (mime-type, data).
pub fn fragment_from_uri(
    mp4: &MP4,
    url_tail: &str,
    range: Option<Range<u64>>,
) -> io::Result<(&'static str, Vec<u8>, u64)> {
    // initialization section.
    if let Ok((track_id, ext)) = scan_fmt!(url_tail, "init.{}.{}{e}", u32, String) {
        match ext.as_str() {
            "mp4" => {
                let init = crate::fragment::media_init_section(&mp4, &[track_id]);
                let mut buffer = MemBuffer::new();
                init.write(&mut buffer)?;
                let data = buffer.into_vec();
                let size = data.len() as u64;
                return Ok(("video/mp4", data, size));
            },
            "vtt" => {
                let buffer = b"WEBVTT\n\n";
                let size = buffer.len() as u64;
                return Ok(("text/vtt", buffer.to_vec(), size));
            },
            _ => return Err(ioerr!(InvalidData, "Bad request")),
        }
    }

    // external file.
    if url_tail.starts_with("e/") {
        // if url_tail ends in /format.VTT|SRT|..>, then strip it off and
        // pass that as the format. Otherwise, pass url_tail as the format.
        // subtitles::external only looks at the extension anyway.
        let mut filename = &url_tail[2..];
        let format = filename
            .rfind("/format.")
            .map(|idx| {
                let fmt = &filename[idx + 1..];
                filename = &filename[..idx];
                fmt
            })
            .unwrap_or(filename);

        // Find the dirname of the mp4 file and add the subtitle filename.
        let path_ref = mp4.input_file.as_ref();
        let mut path = match path_ref.and_then(|f| Path::new(f).parent()) {
            Some(path) => path.to_path_buf(),
            None => return Err(ioerr!(NotFound, "no base directory for {}", filename)),
        };
        path.push(filename);
        let (mime, data) = crate::subtitle::external(path.to_str().unwrap(), format)?;
        let size = data.len() as u64;
        return Ok((mime, data, size));
    }

    match scan_fmt!(url_tail, "{[vas]}/c.{}.{}.{}-{}.", char, u32, u32, u32, u32) {
        Ok((typ, track_id, seq_id, start_sample, end_sample)) => {
            let mime = match typ {
                'v' => "video/mp4",
                'a' => "audio/mp4",
                's' => "text/vtt",
                _ => unreachable!(),
            };
            let fs = FragmentSource {
                src_track_id: track_id,
                dst_track_id: 1,
                from_sample:  start_sample,
                to_sample:    end_sample,
            };
            let (buffer, size) = match typ {
                's' => {
                    let buffer = crate::subtitle::fragment(&mp4, Format::Vtt, &fs)?;
                    let size = buffer.len() as u64;
                    (buffer, size)
                },
                _ => movie_fragment(&mp4, seq_id, fs, range)?,
            };
            Ok((mime, buffer, size))
        },
        Err(_) => Err(ioerr!(InvalidData, "bad request")),
    }
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct FragmentKey {
    file:   String,
    source: FragmentSource,
}

fn movie_fragment(
    mp4: &MP4,
    seq_id: u32,
    fs: FragmentSource,
    range: Option<Range<u64>>,
) -> io::Result<(Vec<u8>, u64)> {
    #[rustfmt::skip]
    static FRAGMENTS: Lazy<LruCache<FragmentKey, Arc<Vec<u8>>>> = {
        Lazy::new(|| LruCache::new(Duration::new(60, 0)))
    };

    // Remap usize range to u64.
    let range = if let Some(range) = range {
        if range.end >= usize::MAX as u64 {
            return Err(ioerr!(InvalidData, "requested fragment too large"));
        }
        Some(Range {
            start: range.start as usize,
            end:   range.end as usize,
        })
    } else {
        None
    };

    // See if we have the data in the cache.
    let file = mp4
        .input_file
        .as_ref()
        .map(|s| s.to_owned())
        .unwrap_or(String::new());
    let key = FragmentKey {
        file,
        source: fs.clone(),
    };
    let mut cached = false;
    let frag = mp4.input_file.as_ref().and_then(|_| FRAGMENTS.get(&key));
    let data = if let Some(frag) = frag {
        cached = true;
        frag
    } else {
        // Not in the cache, so generate it.
        let frag = crate::fragment::movie_fragment(&mp4, seq_id, &[fs])?;
        let mut buffer = MemBuffer::new();
        frag.to_bytes(&mut buffer)?;
        Arc::new(buffer.into_vec())
    };

    // remap no range to full range.
    let mut range = range.unwrap_or(Range {
        start: 0,
        end:   data.len(),
    });

    // check for invalid range.
    if range.start >= range.end || range.start >= data.len() {
        return Err(ioerr!(InvalidData, "416 invalid range"));
    }

    // we might be able to satisfy part of the range.
    if range.end > data.len() {
        range.end = data.len();
    }
    let size = data.len() as u64;

    // reached the end, remove from cache.
    if range.end == data.len() && cached {
        println!("removin from cache");
        FRAGMENTS.remove(&key);
    }
    if range.start != 0 || range.end < data.len() {
        // partial data from a range.
        let partial = data[range].to_owned();
        if !cached && key.file.len() > 0 {
            // cache it for later.
            println!("savin to cache");
            FRAGMENTS.put(key, data);
        }
        Ok((partial, size))
    } else {
        // Try to unwrap the Arc, if noone else is using it we get it without cloning.
        let data = Arc::try_unwrap(data).unwrap_or_else(|data| data.to_vec());
        Ok((data, size))
    }
}
