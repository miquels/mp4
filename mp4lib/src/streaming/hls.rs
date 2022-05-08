//! On the fly `HLS` packaging.
//!
//! The functions in this crate can repackage a normal `MP4` file
//! on-the-fly into a `HLS VOD` stream.
//!
//! If you want to use this functionality in your own `HTTP` server, use
//! the functions in the [`http_handler`](crate::streaming::http_handler)
//! module.
//!
//! ## HLS url conventions.
//!
//! A `HLS` stream is a collection of objects served over HTTP. The main
//! entry point for that is the `HLS` master playlist. It contains
//! URLs to per-track playlists, which each contain URLs to per-track
//! media data. Since we generate the playlists, we need to define the
//! mappings from URL to objects:
//!
//! - `master.m3u8`: entry point, the master `HLS` playlist.
//! - `media.<TRACK_ID>.m3u8`: per-track playlist.
//! - `init.<TRACK_ID>.mp4`: `ISOBMFF` initialization segment for the track.
//! - `init.<TRACK_ID>.vtt`: `WEBVTT` initialization segment for the track.
//! - `a/c.<TRACK_ID>.<SEGMENT_ID>.m4a`: MP4 audio segment
//! - `v/c.<TRACK_ID>.<SEGMENT_ID>.mp4`: MP4 video segment
//! - `s/c.<TRACK_ID>.<SEGMENT_ID>.vtt`: webvtt segment
//! - `e/EXTERNALFILE.EXT[:into.vtt]`: external single-segment subtitle track.
//!
//! ## Master playlist.
//!
//! `master.m3u8`.
//!
//! Contains URLs of the per-track playlists.
//!
//! Generated by [`hls_master`](crate::streaming::hls::hls_master).
//!
//! ## Per track playlist.
//!
//! `media.<TRACK_ID>.m3u8`.
//!
//! Contains URLs of the media initialization segment, and all the
//! media data segments of the track.
//!
//! Generated by [`hls_track`](crate::streaming::hls::hls_track).
//!
//! ## ISOBMFF initialization segment.
//!
//! `init.<TRACK_ID>.mp4`.
//!
//! Generated by [`media_init_section`](crate::streaming::fragment::media_init_section).
//!
//! ## WEBVTT initialization segment.
//!
//! `init.<TRACK_ID>.vtt`.
//!
//! Is always just:
//! ```
//! WEBVTT
//!
//! ```
//!
//! ## Video / audio media data segments.
//!
//! `a/c.<TRACK_ID>.<SEGMENT_ID>.m4a`  
//! `v/c.<TRACK_ID>.<SEGMENT_ID>.mp4`
//!
//! `SEGMENT_ID` is in the form `N.X-Y`, which means `samples X to Y of track TRACK_ID`.
//! This is easy to map to part of an MP4 track without holding much state.
//! Can be generated by
//! [`movie_fragment`](crate::streaming::fragment::movie_fragment).
//!
//! ## Subtitle media data segments.
//!
//! `s/c.<TRACK_ID>.<SEGMENT_ID>.vtt`
//!
//! SEGMENT_ID as above. Subtitles tracks in the `mp4` file in `tx3g` format are
//! transmuxed into fragmented `WEBVTT`.
//!
//! ## External subtitle media data segments.
//!
//! `e/<EXTERNALFILE.EXT>[:into.vtt]`.
//!
//! External subtitles can also be included as a media data segment,
//! basically being one big segment.
//!
use std::cmp;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::fmt::Write;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use once_cell::sync::Lazy;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use scan_fmt::scan_fmt;

use crate::io::MemBuffer;
use crate::mp4box::MP4;
use crate::serialize::ToBytes;
use crate::track::SpecificTrackInfo;
use crate::types::FourCC;

use super::fragment::FragmentSource;
use super::http_file::{HttpFile, MemFile, delegate_http_file};
use super::lru_cache::LruCache;
use super::segmenter::Segment;
use super::subtitle::Format;

const SUBTITLE_LANG: [&'static str; 5] = ["en", "nl", "de", "fr", "es"];

const PATH_ESCAPE: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'<')
    .add(b'>')
    .add(b'&')
    .add(b'%')
    .add(b'#')
    .add(b'$')
    .add(b'+')
    .add(b'=')
    .add(b'\\')
    .add(b'"')
    .add(b'\'')
    .add(b'?');

struct ExtXMedia {
    type_: &'static str,
    group_id: String,
    name: String,
    channels: Option<u16>,
    language: Option<String>,
    auto_select: bool,
    default: bool,
    forced: bool,
    sdh: bool,
    commentary: bool,
    uri: String,
}

impl Display for ExtXMedia {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#EXT-X-MEDIA:TYPE={},", self.type_)?;
        write!(f, r#"GROUP-ID="{}","#, self.group_id)?;
        if let Some(ref channels) = self.channels {
            write!(f, r#"CHANNELS="{}","#, channels)?;
        }
        write!(f, r#"NAME="{}","#, self.name)?;
        if let Some(ref lang) = self.language {
            write!(f, r#"LANGUAGE="{}","#, lang)?;
        }
        if self.forced {
            write!(f, r#"FORCED=YES,"#)?;
        }
        let mut c = Vec::new();
        if self.sdh {
            c.push("public.accessibility.describes-music-and-sound");
        }
        if self.commentary {
            c.push("public.accessibility.describes-video");
        }
        if c.len() > 0 {
            write!(f, r#"CHARACTERISTICS="{}","#, c.join("?"))?;
        }
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

// If there are entries with the same name, add #1, #2 etc to the name to make them unique.
fn uniqify_audio(media: &mut Vec<ExtXMedia>) {
    let mut hm = HashMap::new();
    let mut idx = 0;
    while idx < media.len() {
        let e = hm.entry(&media[idx].name).or_insert(Vec::new());
        e.push(idx);
        idx += 1;
    }
    let dups: Vec<_> = hm.drain().map(|e| e.1).filter(|v| v.len() > 1).collect();

    for indexes in dups {
        let mut n = 1;
        for idx in indexes {
            media[idx].name += &format!(" #{}", n);
            n += 1;
        }
    }
}

// If there are entries with the same language, pick the one:
// - that doesn't have the forced flag set, or
// - the first we see.
fn uniqify_subtitles(media: &mut Vec<ExtXMedia>, remove_forced: bool) {
    let mut hm = HashMap::new();
    for idx in 0..media.len() {
        let mut key = media[idx].language.as_ref().map(|s| s.as_str()).unwrap_or("").to_string();
        if !remove_forced && media[idx].forced {
            key += ".FORCED";
        }
        if hm.contains_key(&key) {
            continue;
        }
        hm.insert(key, idx);
    }
    let keep: HashSet<_> = hm.into_values().collect();

    let nmedia = media
        .drain(..)
        .enumerate()
        .filter_map(|(n, e)| if keep.contains(&n) { Some(e) } else { None })
        .collect();
    *media = nmedia;
}

#[derive(Default)]
struct ExtXStreamInf {
    audio: Option<String>,
    avg_bandwidth: Option<u64>,
    bandwidth: u64,
    codecs: Vec<String>,
    subtitles: bool,
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
        if self.subtitles {
            write!(f, r#"SUBTITLES="subs","#)?;
        }
        write!(f, r#"CODECS="{}","#, codecs)?;
        write!(f, r#"RESOLUTION={}x{},"#, self.resolution.0, self.resolution.1)?;
        write!(f, r#"FRAME-RATE={:.03}"#, self.frame_rate)?;
        write!(f, "\n{}\n", self.uri)
    }
}

fn lang<'a>(lang: &'a str) -> (Option<&'a str>, &'a str) {
    // shortcut for known language tags, with localized name.
    match lang {
        "en" | "eng" => return (Some("en"), "English"),
        "nl" | "dut" | "nld" => return (Some("nl"), "Nederlands"),
        "fr" | "fra" | "fre" => return (Some("fr"), "Français"),
        "de" | "ger" => return (Some("de"), "German"),
        "es" | "spa" => return (Some("es"), "Español"),
        "za" | "afr" | "zaf" => return (Some("za"), "Afrikaans"),
        "zh" | "chi" | "cn" => return (Some("zh"), "Chinese"),
        "hi" | "hin" => return (Some("hi"), "Hindi"),
        "und" => return (None, "Undetermined"),
        _ => {},
    }

    use isolang::Language;

    // first look up 2-letter or 3-letter language code.
    let language = match lang.len() {
        2 => Language::from_639_1(lang),
        3 => Language::from_639_3(lang),
        _ => return (None, lang),
    };

    // Did we succeed?
    let language = match language {
        Some(l) => l,
        None => return (Some(lang), lang),
    };

    // use 2-letter code if it exists, otherwise 3-letter code.
    let code = language.to_639_1().unwrap_or(language.to_639_3());

    (Some(code), language.to_name())
}

fn want_language(lang: Option<&str>, list: &[&str]) -> bool {
    match lang {
        Some(lang) => list.iter().any(|&e| e == lang),
        None => true,
    }
}

fn subtitle_info_from_name(name: &str) -> (String, bool, bool) {
    let mut forced = false;
    let mut sdh = false;
    let mut lang = "und".to_string();

    let fields: Vec<_> = name.split('.').collect();
    if fields.len() < 3 {
        return (lang, sdh, forced);
    }

    let mut idx = fields.len() - 2;
    while idx > 0 {
        if fields[idx].eq_ignore_ascii_case("forced") {
            forced = true;
            idx -= 1;
            continue;
        }
        if fields[idx].eq_ignore_ascii_case("sdh") {
            sdh = true;
            idx -= 1;
            continue;
        }
        break;
    }

    if idx > 0 {
        lang = fields[idx].to_string();
    }

    (lang, sdh, forced)
}

fn lookup_subtitles(mp4path: Option<&String>) -> Vec<String> {
    let mut subs = Vec::new();
    let mp4path = match mp4path {
        Some(p) => p,
        None => return subs,
    };
    let parent = match Path::new(mp4path).parent() {
        Some(p) => p,
        None => return subs,
    };
    let filename = mp4path.split('/').last().unwrap();
    if !filename.ends_with(".mp4") {
        return subs;
    }
    let prefix = &filename[..filename.len() - 3];

    let _ = (|| {
        for entry in fs::read_dir(parent)? {
            let entry = entry?;
            if let Some(filename) = entry.file_name().to_str() {
                if filename.starts_with(prefix) && (filename.ends_with(".srt") || filename.ends_with(".vtt"))
                {
                    subs.push(filename.to_string());
                }
            }
        }
        Ok::<_, io::Error>(())
    })();
    subs
}

/// Generate a master `HLS` playlist (`m3u8`) from an `MP4` object.
///
/// The extra parameters are:
///
/// - `external_subs`: if `true`, the directory of the original `mp4` file
///   is scanned for `.srt` and `.vtt` files that have the same `"base"`
///   (the part of the filename before the `.mp4` extension) and those are
///   included in the playlist.
///
/// - `filter_subs`: some HLS players only show one subtitle per language, even
///   if multiple are available (say, `main`, `SDH`, `FORCED`, `commentary`.
///   If `filter_subs` is `true`, we try to only include the `main` subtitle
///   for each language in the playlist.
///
///   The generated playlist contains relative URLs, one per track. Each
///   one is in itself another `m3u8` playlist. They come in 2 variants:
///
///   - `media.N.m3u8`: the playlist for track `N` (starting from 1).
///   - `./media.ext:EXTERNALSUB:as.m3u8`: refers to an external
///     subtitle file (`EXTERNALSUB`) which gets translated to
///     `fragmented webvtt`.
///
///   The `http` server that serves these playlists must
///   interpret these URLs and serve the corresponding track playlist.
///
pub fn hls_master(mp4: &MP4, external_subs: bool, filter_subs: bool) -> String {
    let mut m = String::new();
    m += "#EXTM3U\n";
    m += "# Created by mp4lib.rs\n";
    m += "#\n";
    m += "#EXT-X-VERSION:6\n";
    m += "\n";

    let mut audio_codecs = HashMap::new();
    let mut audio_tracks = Vec::new();

    // Audio tracks.
    for track in crate::track::track_info(mp4).iter() {
        let info = match &track.specific_info {
            SpecificTrackInfo::AudioTrackInfo(info) => info,
            _ => continue,
        };
        // Skip empty tracks.
        if track.duration.as_secs() == 0 {
            continue;
        }

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

        let track_lang = track.language.to_string();
        let (lang, name) = lang(&track_lang);
        let mut name = name.to_string();
        if let Some(ref handler_name) = track.name {
            name += &format!(" - {}", handler_name);
        } else if info.channel_count >= 3 {
            name += &format!(" ({}.{})", info.channel_count, info.lfe_channel as u8);
        }
        let commentary = name.contains("Commentary");

        let audio = ExtXMedia {
            type_: "AUDIO",
            group_id: format!("audio/{}", info.codec_id),
            language: lang.map(|s| s.to_string()),
            channels: Some(info.channel_count + info.lfe_channel as u16),
            name,
            auto_select: !commentary,
            default: false,
            forced: false,
            sdh: false,
            commentary,
            uri: format!("media.{}.m3u8", track.id),
        };
        audio_tracks.push(audio);
    }

    uniqify_audio(&mut audio_tracks);
    for audio in &audio_tracks {
        let _ = write!(m, "{}", audio);
    }

    // Subtitle tracks.
    let mut sublang = HashSet::new();
    let mut subtitles = Vec::new();

    // External subtitles.
    if external_subs {
        for sub in lookup_subtitles(mp4.input_file.as_ref()).iter() {
            // look up language and language code.
            let (language, sdh, forced) = subtitle_info_from_name(sub);
            let (lang, name) = lang(&language);

            // no duplicates.
            if sublang.contains(name) {
                continue;
            }
            sublang.insert(name.to_string());

            // FIXME: encode here, or when serializing to m3u8 ?
            let sub = utf8_percent_encode(&sub, PATH_ESCAPE).to_string();

            let subm = ExtXMedia {
                type_: "SUBTITLES",
                group_id: "subs".to_string(),
                language: lang.map(|s| s.to_string()),
                channels: None,
                name: name.to_string(),
                auto_select: true,
                default: false,
                forced,
                sdh,
                commentary: false,
                // note, either keep the "./", or escape the ":".
                uri: format!("./media.ext:{}:as.m3u8", sub),
            };
            subtitles.push(subm);
        }
    }

    // Embedded subtitles.
    for track in crate::track::track_info(mp4).iter() {
        match &track.specific_info {
            SpecificTrackInfo::SubtitleTrackInfo(_) => {},
            _ => continue,
        }
        // Skip empty tracks.
        if track.duration.as_secs() == 0 {
            continue;
        }

        // Track language.
        let track_lang = track.language.to_string();
        let (lang, name) = lang(&track_lang);
        if !want_language(lang, &SUBTITLE_LANG) {
            continue;
        }
        let mut name = name.to_string();

        // Track name. Might be a descriptive name, but can also be one of:
        // - "Forced"
        // - "Hearing Impaired"
        let mut forced = false;
        let mut sdh = false;
        if let Some(ref track_name) = track.name {
            let lname = track_name.to_lowercase();
            if lname.contains("forced") {
                forced = true;
                name = format!("{} (forced)", name);
            }
            if track_name.contains("SDH") || (lname.contains("hearing") && lname.contains("impaired")) {
                sdh = true;
                name = format!("{} (SDH)", name);
            }
            if !forced && !sdh {
                name = track_name.to_string();
            }
        }

        // skip if we already have an external subtitle track file.
        // This is not quite correct, we also should compare forced and sdh.
        if subtitles
            .iter()
            .any(|s| !s.uri.starts_with("media.") && s.language.as_ref().map(|s| s.as_str()) == lang && s.sdh == sdh && s.forced == forced)
        {
            continue;
        }

        let sub = ExtXMedia {
            type_: "SUBTITLES",
            group_id: "subs".to_string(),
            language: lang.map(|s| s.to_string()),
            channels: None,
            name: name.to_string(),
            auto_select: true,
            default: false,
            forced,
            sdh,
            commentary: false,
            uri: format!("media.{}.m3u8", track.id),
        };
        subtitles.push(sub);
    }

    if subtitles.len() > 0 {
        m += "\n# SUBTITLES\n";

        // sort the subtitles so that 'sdh' and 'forced come after none-sdh/forced.
        //
        // this is because if we have multiple subtitles in the same language,
        // and the player only shows one entry per language, we want to have
        // the "normal" subtitles.
        subtitles.sort_by(|a, b| {
            if a.language != b.language {
                return a.language.cmp(&b.language);
            }
            if a.sdh != b.sdh {
                return a.sdh.cmp(&b.sdh);
            }
            a.forced.cmp(&b.forced)
        });

        if filter_subs {
            uniqify_subtitles(&mut subtitles, true);
        }

        for sub in &subtitles {
            let _ = write!(m, "{}", sub);
        }
    }

    // video track.
    m += "\n# VIDEO\n";
    for track in crate::track::track_info(mp4).iter() {
        let info = match &track.specific_info {
            SpecificTrackInfo::VideoTrackInfo(info) => info,
            _ => continue,
        };
        // Skip empty tracks.
        if track.duration.as_secs() == 0 {
            continue;
        }
        let avg_bw = track.size / cmp::max(1, track.duration.as_secs());
        let mut ext = ExtXStreamInf {
            bandwidth: avg_bw,
            codecs: vec![info.codec_id.clone()],
            resolution: (info.width, info.height),
            frame_rate: info.frame_rate,
            uri: format!("media.{}.m3u8", track.id),
            subtitles: subtitles.len() > 0,
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
    let segments = super::segmenter::track_to_segments(track, duration)?;
    let segments = Arc::new(segments);
    SEGMENTS.put(key, segments.clone());
    Ok(segments)
}

/// Generate a `HLS` track playlist.
///
/// This is also an `m3u8` file. It contains a list of media segments.
///
pub fn hls_track(mp4: &MP4, track_id: u32) -> io::Result<String> {
    let movie = mp4.movie();
    let trak = movie
        .track_by_id(track_id)
        .ok_or_else(|| ioerr!(NotFound, "track not found"))?;
    let handler = trak.media().handler();
    let handler_type = handler.handler_type;
    let is_subtitle = handler.is_subtitle();

    let seg_duration = None; // Some(4000);

    let segments = if !is_subtitle {
        let video_id = match movie.track_idx_by_handler(FourCC::new("vide")) {
            Some(idx) => movie.tracks()[idx].track_id(),
            None => return Err(ioerr!(NotFound, "mp4 file has no video track")),
        };

        let mut segments = track_to_segments(mp4, video_id, seg_duration)?;
        if track_id != video_id {
            let segs: &[Segment] = segments.as_ref();
            segments = Arc::new(super::segmenter::track_to_segments_timed(trak, segs)?);
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
    if !is_subtitle {
        m += &format!("#EXT-X-MAP:URI=\"init.{}.mp4\"\n", track_id);
    }

    for (mut seq, seg) in segments.iter().enumerate() {
        if is_subtitle {
            seq = (seg.start_time * 1000.0) as usize;
        } else {
            seq += 1;
        }

        // Skip segments that are < 0.1 ms.
        if seg.duration.partial_cmp(&0.0001) == Some(std::cmp::Ordering::Greater) {
            m += &format!(
                "#EXTINF:{:.5}\n{}/c.{}.{}.{}-{}.{}\n",
                seg.duration, prefix, track_id, seq, seg.start_sample, seg.end_sample, suffix
            );
        }
    }
    m += "#EXT-X-ENDLIST\n";

    Ok(m)
}

fn hls_subtitle(dirname: &str, name: &str) -> io::Result<String> {
    let path = join_path(dirname, name);
    let duration = super::subtitle::duration(&path)?;

    let mut name = name.to_string();
    if !name.ends_with(".vtt") {
        name.push_str(":into.vtt");
    }

    let mut m = String::new();
    m += "#EXTM3U\n";
    m += "#EXT-X-VERSION:6\n";
    m += "## Created by mp4lib.rs\n";
    m += "#\n";
    m += &format!("#EXT-X-TARGETDURATION:{}\n", duration);
    m += "#EXT-X-PLAYLIST-TYPE:VOD\n";
    m += &format!("#EXTINF:{}\n", duration);
    m += "e/";
    m += &utf8_percent_encode(&name, PATH_ESCAPE).to_string();
    m += "\n#EXT-X-ENDLIST\n";
    Ok(m)
}

/// HLS playlist data.
///
/// This struct `impl`s `HttpFile`.
pub struct HlsManifest(pub(crate) MemFile);
delegate_http_file!(HlsManifest);

impl HlsManifest {
    /// Translates the tail of an `Url` into a `HLS` playlist or subtitle file.
    ///
    /// The main idea here is for a http server to serve requests for (example url)
    /// `/media/path/filename.mp4/<playlist.m3u8`. This then serves
    /// a `master` or `track` `HLS` manifest.
    ///
    /// The `url_tail` is the part after `...mp4/` and has this format:
    ///
    /// - `master.m3u8`              => `HLS` master playlist
    /// - `media.<TRACK_ID>.m3u8`     => `HLS` track playlist
    /// - `media.ext:NAME.EXT:as.m3u8` => `HLS` external subtitle file playlist
    ///
    /// The last case looks the most complicated, but is in fact the simplest.
    /// `media.ext` is literal. `NAME.EXT` is the name of the subtitle
    /// file in the same directory as the `mp4` file, either an `srt` or `vtt` file.
    /// `:as.m3u8` means "create a manifest for this file with one entry: this file".
    ///
    /// `master.m3u8` will have entries that refer to the latter two `url`s.
    ///
    /// Each manifest will have entries that refer to per-track media segments.
    ///
    pub fn from_uri(mp4: &MP4, url_tail: &str, filter_subs: bool) -> io::Result<HlsManifest> {
        let data = if url_tail == "main.m3u8" || url_tail == "master.m3u8" {
            // HLS master playlist.
            hls_master(&mp4, true, filter_subs)
        } else if let Ok(track) = scan_fmt!(url_tail, "media.{}.m3u8{e}", u32) {
            // HLS media playlist.
            hls_track(&mp4, track)?
        } else if let Ok((name, _)) = scan_fmt!(url_tail, "media.ext:{}:{}.m3u8{e}", String, String) {
            // external file next to .mp4.
            if name.ends_with(".srt") || name.ends_with(".vtt") {
                // subtitles.
                let dirname = dirname(mp4.input_file.as_ref(), &name)?;
                hls_subtitle(&dirname, &name)?
            } else {
                // unknown external file type.
                return Err(ioerr!(InvalidData, "415 Unsupported Media Type"));
            }
        } else {
            return Err(ioerr!(InvalidData, "415 Unsupported Media Type"));
        };

        let file = &*mp4.data_ref.file;
        let mem_file = MemFile::from_file(data.into_bytes(), "application/vnd.apple.mpegurl; charset=utf-8", file)?;
        Ok(HlsManifest(mem_file))
    }

    /// `HLS` manifest as text.
    pub fn manifest(&self) -> &'_ str {
        std::str::from_utf8(&self.0.content[..]).unwrap_or("")
    }
}

/// A media segment. An `fMP4` fragment, or a `CMAF` segment.
pub struct MediaSegment(pub(crate) MemFile);
delegate_http_file!(MediaSegment);

impl MediaSegment {
    /// Translates the tail of an URL into an MP4 init segment or media segment.
    ///
    /// - `init.TRACK_ID.mp4` => initialization segment for track `TRACK_ID`.
    /// - `init.TRACK_ID.vtt` => initialization segment for track `TRACK_ID`.
    ///
    /// - `a/c.TRACK_ID.SEQUENCE.FROM_SAMPLE.TO_SAMPLE.m4a` => audio moof + mdat
    /// - `v/c.TRACK_ID.SEQUENCE.FROM_SAMPLE.TO_SAMPLE.mp4` => video moof + mdat
    /// - `s/c.TRACK_ID.SEQUENCE.FROM_SAMPLE.TO_SAMPLE.vtt` => webvtt fragment
    /// - `e/EXTERNALFILE.EXT[:into.ext]` => external file next to `.mp4` (`.srt`, `.vtt`)
    ///
    pub fn from_uri(mp4: &MP4, url_tail: &str, range_end: Option<u64>) -> io::Result<MediaSegment> {
        let (mime_type, content) = MediaSegment::from_uri_(mp4, url_tail, range_end)?;
        let file = &*mp4.data_ref.file;
        let mem_file = MemFile::from_file_shared(content, mime_type, file)?;
        Ok(MediaSegment(mem_file))
    }

    fn from_uri_(mp4: &MP4, url_tail: &str, range_end: Option<u64>) -> io::Result<(&'static str, Arc<Vec<u8>>)> {
        // initialization section.
        if let Ok((track_id, ext)) = scan_fmt!(url_tail, "init.{}.{}{e}", u32, String) {
            match ext.as_str() {
                "mp4" => {
                    let init = super::fragment::media_init_section(&mp4, &[track_id]);
                    let mut buffer = MemBuffer::new();
                    init.write(&mut buffer)?;
                    let data = buffer.into_vec();
                    return Ok(("video/mp4", Arc::new(data)));
                },
                "vtt" => {
                    let buffer = b"WEBVTT\n\n".to_vec();
                    return Ok(("text/vtt; charset=utf-8", Arc::new(buffer)));
                },
                _ => return Err(ioerr!(InvalidData, "Bad request")),
            }
        }

        // external file.
        if url_tail.starts_with("e/") && url_tail.ends_with(".vtt") {
            // subtitles.
            let mut iter = url_tail[2..].split(':');
            let name = iter.next().unwrap();
            let format = iter.next().unwrap_or(name);

            let dirname = dirname(mp4.input_file.as_ref(), name)?;
            let path = join_path(dirname, name);

            let (mime, data) = super::subtitle::external(&path, format)?;
            return Ok((mime, Arc::new(data)));
        }

        let tups = match scan_fmt!(url_tail, "{[vas]}/c.{}.{}.{}-{}.", char, u32, u32, u32, u32) {
            Ok(tups) => tups,
            Err(_) => return Err(ioerr!(InvalidData, "bad request")),
        };
        let (typ, track_id, seq_id, start_sample, end_sample) = tups;

        let mime = match typ {
            'v' => "video/mp4",
            'a' => "audio/mp4",
            's' => "text/vtt; charset=utf-8",
            _ => unreachable!(),
        };
        let fs = FragmentSource {
            src_track_id: track_id,
            dst_track_id: 1,
            from_sample: start_sample,
            to_sample: end_sample,
        };
        let content = match typ {
            's' => {
                //let ts = seq_id as f64 / 1000.0;
                Arc::new(super::subtitle::fragment(&mp4, Format::Vtt, &fs, 0.0)?)
            },
            _ => movie_fragment(&mp4, seq_id, fs, range_end)?,
        };

        Ok((mime, content))
    }

    /// `media segment` data as bytes.
    pub fn media_data(&self) -> &'_ [u8] {
        &self.0.content[..]
    }
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct FragmentKey {
    file: String,
    source: FragmentSource,
}

// Caching wrapper around fragment::movie_fragment. Mainly to
// avoid doing work if the client is using range requests.
//
// If the request is for a partial range, we cache the data in
// an LRU cache. If the request has no range, or the range
// extends to te end of the fragment, we remove the fragment
// from the cache (if it was cached).
fn movie_fragment(
    mp4: &MP4,
    seq_id: u32,
    fs: FragmentSource,
    range_end: Option<u64>,
) -> io::Result<Arc<Vec<u8>>> {
    #[rustfmt::skip]
    static FRAGMENTS: Lazy<LruCache<FragmentKey, Arc<Vec<u8>>>> = {
        Lazy::new(|| LruCache::new(Duration::new(60, 0)))
    };

    // No filename means no key. Now this can never happen,
    // but better safe than sorry.
    let file = match mp4.input_file.as_ref() {
        Some(f) => f.to_string(),
        None => {
            let frag = super::fragment::movie_fragment(&mp4, seq_id, &[fs])?;
            let mut buffer = MemBuffer::new();
            frag.to_bytes(&mut buffer)?;
            return Ok(Arc::new(buffer.into_vec()));
        }
    };

    // See if we have the data in the cache.
    let key = FragmentKey {
        file,
        source: fs.clone(),
    };
    let (frag, cached) = match FRAGMENTS.get(&key) {
        Some(frag) => (frag, true),
        None => {
            // Not in the cache, so generate it.
            let frag = super::fragment::movie_fragment(&mp4, seq_id, &[fs])?;
            let mut buffer = MemBuffer::new();
            frag.to_bytes(&mut buffer)?;
            (Arc::new(buffer.into_vec()), false)
        }
    };

    // cache management.
    let partial = range_end.map(|r| r != 0 && r < frag.len() as u64).unwrap_or(false);
    if cached && !partial {
        FRAGMENTS.remove(&key);
    }
    if !cached && partial {
        FRAGMENTS.put(key, frag.clone());
    }
    FRAGMENTS.expire();

    Ok(frag)
}

fn dirname(ref_path: Option<&String>, name: &str) -> io::Result<String> {
    if name.contains(":") || name.contains("/") || name.contains("\\") || name.contains("\0") {
        return Err(ioerr!(InvalidInput, "400 invalid filename {}", name));
    }
    ref_path
        .and_then(|p| Path::new(p).parent())
        .and_then(|p| p.to_str().map(|p| p.to_string()))
        .ok_or_else(|| ioerr!(InvalidInput, "400 no basedir for {}", name))
}

fn join_path(dir: impl Into<String>, name: &str) -> String {
    let mut dir = PathBuf::from(dir.into());
    dir.push(Path::new(name));
    dir.to_str().unwrap().to_string()
}
