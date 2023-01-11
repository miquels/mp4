//! Subtitle handling.
//!
use std::borrow::Cow;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::str::FromStr;

use scan_fmt::scan_fmt;

use super::fragment::FragmentSource;
use crate::boxes::sbtl::Tx3GTextSample;
use crate::boxes::*;
use crate::mp4box::MP4;
use crate::serialize::FromBytes;
use crate::track::SampleInfo;

/// Subtitle format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Vtt,
    Srt,
    Tx3g,
}

impl FromStr for Format {
    type Err = io::Error;
    fn from_str(format: &str) -> Result<Self, Self::Err> {
        let mut format = format;
        if let Some(idx) = format.rfind(".") {
            format = &format[idx + 1..];
        }
        match format {
            "vtt" => Ok(Format::Vtt),
            "srt" => Ok(Format::Srt),
            "tx3g" => Ok(Format::Tx3g),
            "3gpp" => Ok(Format::Tx3g),
            _ => Err(ioerr!(InvalidInput, "Could not parse format")),
        }
    }
}

fn ptime(secs: f64, format: Format) -> String {
    let mut tm = (secs * 1000f64) as u64;

    let millis = tm % 1000;
    tm /= 1000;
    let secs = tm % 60;
    tm /= 60;
    let mins = tm % 60;
    tm /= 60;

    let sep = if format == Format::Vtt { "." } else { "," };
    format!("{:02}:{:02}:{:02}{}{:03}", tm, mins, secs, sep, millis)
}

fn cue(
    format: Format,
    timescale: u32,
    seq: Option<u32>,
    sample: SampleInfo,
    subt: Tx3GTextSample,
    tm_off: f64,
) -> String {
    use std::fmt::Write;
    let eol = if format == Format::Vtt { "\n" } else { "\r\n" };

    let starttime = sample.decode_time as f64 / (timescale as f64);
    let mut duration = sample.duration as f64 / (timescale as f64);
    // If two cues are back-to-back, the endtime of the first cue can
    // be the same as the starttime of the second cue. That is valid
    // and the spec says that they do not overlap. However, it's
    // better to be safe than sorry.
    if duration >= 0.02 {
        duration -= 0.01;
    }
    let endtime = starttime + duration;
    let mut cue = String::new();

    if let Some(seq) = seq {
        let _ = write!(cue, "{}{}", seq, eol);
    }

    let _ = write!(
        cue,
        "{} --> {}{}",
        ptime(starttime - tm_off, format),
        ptime(endtime - tm_off, format),
        eol
    );

    for line in subt.text.split('\n') {
        if line == "" {
            continue;
        }
        for c in line.chars() {
            match c {
                '&' => cue.push_str("&amp;"),
                '<' => cue.push_str("&lt;"),
                '>' => cue.push_str("&gt;"),
                c => cue.push(c),
            }
        }
        cue.push_str(eol);
    }
    cue.push_str(eol);

    cue
}

/// Extract a subtitle track into VTT / SRT or 3GPP.
///
/// Note that if the `mp4` file resides on a classical spinning disk,
/// this can be quite slow, since usually the subtitle track is
/// interleaved with the video/audio tracks. Meaning that the disk
/// will probably have to do a seek for each sample.
///
pub fn subtitle_extract(
    mp4: &MP4,
    track: &TrackBox,
    format: Format,
    mut output: impl Write,
) -> io::Result<()> {
    let iter = track.sample_info_iter();
    let timescale = iter.timescale();
    let mut seq = 1;

    if format == Format::Vtt {
        write!(output, "WEBVTT\n")?;
        write!(output, "\n")?;
        if let Some(filename) = mp4.input_file.as_ref() {
            write!(output, "NOTE extracted from {}\n", filename)?;
        } else {
            write!(output, "NOTE extracted from mp4 file\n")?;
        }
        let lang = track.media().media_header().language.to_string();
        if lang != "und" && lang != "unk" {
            write!(output, "NOTE language: {}\n", lang)?;
        }
        write!(output, "\n")?;
    }

    let mut buf = Vec::new();
    buf.resize(256, 0);

    for sample in iter {
        if buf.len() < sample.size as usize {
            buf.resize(sample.size as usize, 0);
        }
        let data = &mut buf[..sample.size as usize];
        if data.len() <= 2 {
            // empty sample. no need to read it from disk.
            data.fill(0);
        } else {
            mp4.data_ref.read_exact_at(data, sample.fpos)?;
        }
        let subt = match Tx3GTextSample::from_bytes(&mut &data[..]) {
            Ok(subt) => subt,
            Err(_) => continue,
        };
        if format == Format::Tx3g {
            output.write(&buf[..sample.size as usize])?;
            continue;
        }
        if subt.text.as_str() == "" {
            continue;
        }
        let cue = cue(format, timescale, Some(seq), sample, subt, 0f64);
        output.write(cue.as_bytes())?;
        seq += 1;
    }

    Ok(())
}

/// Create a fragment containing the cue(s).
///
/// Outputs raw data. If this is to be sent in a CMAF container, it
/// still needs to be wrapped by a moof + mdat.
pub fn fragment(mp4: &MP4, format: Format, frag: &FragmentSource, tm_off: f64) -> io::Result<Vec<u8>> {
    let mut buffer = Vec::new();

    // shortcut for empty fragments.
    if frag.from_sample == 0 && frag.to_sample == 0 {
        if format == Format::Vtt {
            buffer.extend_from_slice(b"WEBVTT\n\n");
            return Ok(buffer);
        }
        if format == Format::Srt {
            return Ok(buffer);
        }
    }

    let track = mp4
        .movie()
        .track_by_id(frag.src_track_id)
        .ok_or_else(|| ioerr!(NotFound, "track not found"))?;
    let mut iter = track.sample_info_iter();
    let timescale = iter.timescale();

    let mut seq = frag.from_sample;
    iter.seek(frag.from_sample)?;

    if format == Format::Vtt {
        buffer.extend_from_slice(b"WEBVTT\n\n");
    }

    let mut buf = Vec::new();
    buf.resize(256, 0);

    for sample in iter {
        if format == Format::Tx3g || sample.size > 2 {
            if buf.len() < sample.size as usize {
                buf.resize(sample.size as usize, 0);
            }
            let data = &mut buf[..sample.size as usize];
            mp4.data_ref.read_exact_at(data, sample.fpos)?;
            match Tx3GTextSample::from_bytes(&mut &data[..]) {
                Ok(subt) => {
                    if format == Format::Tx3g {
                        buffer.extend_from_slice(&buf[..sample.size as usize]);
                    } else {
                        if subt.text.len() > 0 {
                            let cue = cue(format, timescale, None, sample, subt, tm_off);
                            buffer.extend_from_slice(cue.as_bytes());
                        }
                    }
                },
                Err(_) => {},
            }
        }
        seq += 1;
        if seq > frag.to_sample {
            break;
        }
    }

    Ok(buffer)
}

/// Read an external subtitle file.
///
/// Input and output formats can be webvtt and srt, format will be
/// converted if needed. Output character set is always utf-8.
///
/// Return value is `(mime_type, raw_data)`.
pub fn external(path: &str, to_format: &str) -> io::Result<(&'static str, Vec<u8>)> {

    // see if input and output formats are supported.
    let infmt = Format::from_str(path)?;
    let outfmt = Format::from_str(to_format)?;
    let (eol, mime) = match outfmt {
        Format::Srt => ("\r\n", "text/plain; charset=utf-8"),
        Format::Vtt => ("\n", "text/vtt; charset=utf-8"),
        other => return Err(ioerr!(InvalidData, "unsupported input format {:?}", other)),
    };

    // shortcut for vtt -> vtt.
    if infmt == Format::Vtt && outfmt == Format::Vtt {
        match fs::read_to_string(path) {
            Ok(s) => return Ok((mime, s.into_bytes())),
            Err(e) if e.kind() == io::ErrorKind::InvalidData => {},
            Err(e) => return Err(e),
        }
    }

    // open subtitle file.
    let mut stf = SubtitleFile::open(path, infmt)?;

    let mut buf = String::new();
    if outfmt == Format::Vtt {
        buf.push_str("WEBVTT\n\n");
    }

    let mut seq = 1;
    while let Some(sample) = stf.next() {
        // sequence and timestamps.
        use std::fmt::Write;
        let _ = write!(buf, "{}{}", seq, eol);
        seq += 1;
        format_time(&mut buf, outfmt, sample.start);
        buf.push_str(" --> ");
        format_time(&mut buf, outfmt, sample.start + sample.duration);
        buf.push_str(eol);

        // text.
        if outfmt == Format::Srt {
            for c in sample.text.chars() {
                if c == '\n' {
                    buf.push_str("\r\n");
                } else {
                    buf.push(c);
                }
            }
        } else {
            buf.push_str(&sample.text);
        }
        if !buf.ends_with(eol) {
            buf.push_str(eol);
        }
        buf.push_str(eol);
    }

    Ok((mime, buf.into_bytes()))
}

/// Open a subtitle track file (`vtt` or `srt`) and calculate its duration.
pub fn duration(fspath: &str) -> io::Result<f64> {
    let format = Format::from_str(fspath).map_err(|e| ioerr!(InvalidData, "{}: {}", fspath, e))?;
    let mut stf = SubtitleFile::open(fspath, format)?;
    let mut duration = 0;
    while let Some(sample) = stf.next() {
        duration = std::cmp::max(duration, sample.start + sample.duration);
    }
    Ok((duration as f64 / 1000_f64).ceil())
}

struct SubtitleSample {
    start: u32,
    duration: u32,
    text: String,
}

struct SubtitleFile {
    data: String,
    pos: usize,
}

impl SubtitleFile {
    fn open(path: &str, format: Format) -> io::Result<SubtitleFile> {
        match format {
            Format::Vtt | Format::Srt => {},
            other => return Err(ioerr!(InvalidData, "unsupported input format {:?}", other)),
        }
        let data = read_subtitle(path)?;
        Ok(SubtitleFile{ data, pos: 0 })
    }

    fn read_line(&mut self, line: &mut String) -> io::Result<usize> {
        if self.pos == self.data.len() {
            return Ok(0);
        }
        match self.data[self.pos..].find('\n') {
            Some(off) => {
                let npos = self.pos + off + 1;
                line.push_str(&self.data[self.pos..npos]);
                self.pos = npos;
                if line.ends_with("\r\n") {
                    line.truncate(line.len() - 2);
                    line.push('\n');
                    Ok(off)
                } else {
                    Ok(off + 1)
                }
            }
            None => Ok(0),
        }
    }

    fn next(&mut self) -> Option<SubtitleSample> {
        let mut line = String::new();

        loop {
            // Find the first line with ts --> ts.
            let (start, duration) = loop {
                line.truncate(0);
                let sz = self.read_line(&mut line).ok()?;
                if sz == 0 {
                    return None;
                }
                if let Some(ts) = parse_times(&line) {
                    break ts;
                }
            };
            if duration == 0 {
                continue;
            }

            // Now read the next lines until we see an empty line.
            line.truncate(0);
            loop {
                let start_len = line.len();
                let sz = self.read_line(&mut line).ok()?;
                if sz == 0 {
                    break;
                }
                if &line[start_len..] == "\n" {
                    line.truncate(start_len);
                    break;
                }
            }

            // return cue if it had content, otherwise loop again.
            if line.len() > 0 {
                let text = remove_unsupported_tags(line);
                return Some(SubtitleSample {
                    start,
                    duration,
                    text,
                });
            }
        }
    }
}

// Read the text from a .srt or .vtt file into a String.
//
// Srt files are often encoded as ISO-8859-X or windows-12xx instead of UTF-8,
// so the character-set encoding is detected and converted to UTF-8 if needed.
//
// The language is also detected and returned as a iso-639-3 code.
//
fn decode_and_read(name: &str, detect_only: bool) -> io::Result<(String, &'static str, &'static str)> {
    let mut buf = Vec::new();
    let mut detector = chardetng::EncodingDetector::new();
    let mut data = Vec::with_capacity(if detect_only { 256 } else { 65536 });
    let mut sample = Vec::with_capacity(4200);
    let mut state = false;

    // Get the language from the filename, if any.
    let mut words = name.rsplit('.');
    let _ = words.next();
    let tld = words.next().and_then(map_tld);

    // Open file.
    let file = fs::File::open(&name)?;
    let mut reader = BufReader::with_capacity(65536, file);

    // read the file. isolate the text portions and feed that to the decoder.
    loop {
        // line by line.
        if detect_only {
            data.truncate(0);
        }
        let len = data.len();
        if reader.read_until(b'\n', &mut data)? == 0 {
            break;
        }
        let mut line = &data[len..];
        match state {
            false => {
                if line.windows(5).position(|slice| slice == &b" --> "[..]).is_some() {
                    // start of cue.
                    state = true;
                }
            },
            true if line == b"\n" || line == b"\r\n" => {
                // end of cue.
                state = false;
            },
            true => {
                // add the line to the sample we're going to use for language detection.
                if sample.len() < 4096 {
                    sample.extend(line);
                }

                // 0xb6 is ¶, the paragraph sign. often used as musical note.
                // this confuses the detector!
                if line.contains(&0xb6) && std::str::from_utf8(line).is_err() {
                    buf.truncate(0);
                    buf.extend(line);
                    buf.iter_mut().for_each(|b| if *b == 0xb6 { *b = 0x20 });
                    line = &buf[..];
                }
                // and add line to the detector.
                detector.feed(line, false);
            },
        }
    }
    detector.feed(b"", true);

    // find out the encoding.
    let encoding = detector.guess(tld, true);

    // decode sample and data. data first, in case there is a BOM and
    // a different encoding is chosen - then we use that for the sample too.
    let (mut cow, mut encoding, _) = encoding.decode(&data);

    // detect the language, if needed.
    let lang = if tld.is_none() || detect_only {
        let (sample, _, _) = encoding.decode(&sample);
        whatlang::detect_lang(&sample).map(|l| l.code()).unwrap_or("und")
    } else {
        "und"
    };

    if detect_only {
        // no need to double-check the encoding, the language guess doesn't
        // really care if ISO-8859-2 was detected while it was ISO-8859-1.
        return Ok((String::new(), lang, encoding.name()));
    }

    if tld.is_none() {

        // if we didn't have a tld hint, try again now that we know the
        // language. if we have a tld, this is a west-european language.
        let tld = map_tld(lang);
        let enc = encoding.name();
        if tld.is_some() && enc != "UTF-8" && enc != "ISO-8859-1" && enc != "windows-1252" {

            // we would expect UTF-8, ISO-8859-1, or windows1252, but it isn't.
            // detect the charset again, now with tld info. note that we feed
            // the entire srt/vtt data into the detector, not just the cues,
            // but that should not matter for western languages.
            let mut detector = chardetng::EncodingDetector::new();
            detector.feed(&data, true);
            let encoding2 = detector.guess(tld, true);

            if encoding != encoding2 {
                // different encoding! decode again.
                (cow, encoding, _) = encoding2.decode(&data);
            }
        }
    }

    // if the cow is a borrowed string that starts at the same
    // location as data, it was UTF-8 without a BOM, and we can just convert
    // the entire 'data' buffer without allocating a new string.
    let s = match cow {
        Cow::Borrowed(c) if c.as_ptr() == data.as_ptr() => {
            // unsafe would be faster, oh well.
            String::from_utf8(data).unwrap()
        },
        _ => cow.to_string(),
    };
    Ok((s, lang, encoding.name()))
}

// Map a language identifier into a Top Level Domain.
fn map_tld(lang: &str) -> Option<&'static [u8]> {
    match lang {
        "en"|"eng" => Some("uk"),
        "nl"|"nld"|"dut" => Some("nl"),
        "es|esp"|"spa" => Some("es"),
        "de|ger" => Some("de"),
        "fr|fra"|"fre" => Some("fr"),
        _ => None,
    }.map(|t| t.as_bytes())
}

// Read the text from a .srt or .vtt file into a String.
//
// Srt files are often encoded as ISO-8859-X or windows-12xx instead of UTF-8,
// so the character-set encoding is detected and converted to UTF-8 if needed.
//
fn read_subtitle(filename: &str) -> io::Result<String> {
    let (subs, _, _) = decode_and_read(filename, false)?;
    Ok(subs)
}

// See if a language string in the form "nl", "dut", "nl_NL" is a known
// language, then map that to the shortest ISO-639 string (2 or 3 chars).
fn valid_lang(lang: &str) -> Option<&'_ str> {
    use isolang::Language;

    if lang.contains("_") {
        let lang = lang.split('_').next().unwrap();
        if Language::from_639_1(lang).is_some() {
            return Some(lang);
        }
    }

    match lang.len() {
        2 => Language::from_639_1(lang).map(|_| lang),
        3 => {
            let l3 = Language::from_639_3(lang)?;
            l3.to_639_1().or_else(|| Some(l3.to_639_3()))
        },
        _ => None,
    }
}

/// Get the subtitle info from the tags embedded in the filename. If the 
/// filename doesn't include a language tag, the file is opened and
/// we probe for the filename using [whatlang](https://docs.rs/whatlang).
///
/// If the `mp4file` is passed, that filename without `.mp4` is used as
/// a basename. If the `filename` starts with `basename`, only tags
/// _after_ that are considered. As a result, with `video.en.nl.srt`
/// and `video.en.mp4`, `en` is not seen as a tag, only `nl` is.
///
/// Returns a tuple with:
/// - ISO-693 language tag (2 letter code or 3 letter code)
/// - sdh
/// - forced
///
/// See also:
/// - https://jellyfin.org/docs/general/server/media/external-files/ (naming)
/// - https://www.rfc-editor.org/rfc/rfc5646 (language codes).
///
pub fn subtitle_info(filename: &str, mp4file: Option<&str>) -> io::Result<(String, bool, bool)> {
    let mut forced = false;
    let mut sdh = false;
    let mut lang = None;
    let mut used_prefix = false;

    if let Some(prefix) = mp4file.and_then(|f| f.strip_suffix("mp4")) {
        if let Some(suffix) = filename.strip_prefix(prefix) {
            let mut flags = suffix.split('.');
            // First flag is the language.
            lang = flags.next();
            for flag in flags {
                match flag {
                    "sdh"|"cc"|"hi" => sdh = true,
                    "forced"|"foreign" => forced = true,
                    _ => {},
                }
            }
            used_prefix = true;
        }
    }

    if !used_prefix {
        // Start at the last flag and work in reverse.
        // Heuristic: first unknown flag must be the language.
        for flag in filename.rsplit('.').skip(1) {
            match flag {
                "sdh"|"cc"|"hi" => sdh = true,
                "forced"|"foreign" => forced = true,
                _ => {
                    lang = Some(flag);
                    break;
                }
            }
        }
    }

    if let Some(l) = lang {
        lang = match valid_lang(l) {
            Some(l) => Some(l),
            None => {
                let (_, l, _) = decode_and_read(filename, true)?;
                (l != "und").then(|| valid_lang(l)).flatten()
            }
        }
    }

    Ok((lang.unwrap_or("und").to_string(), sdh, forced))
}

// parse 04:02.500 --> 04:05.000
fn parse_times(line: &str) -> Option<(u32, u32)> {
    let mut fields = line.split_whitespace();
    let f1 = fields.next()?;
    let f2 = fields.next()?;
    let f3 = fields.next()?;
    if f2 != "-->" {
        return None;
    }
    let t1 = parse_time(f1)?;
    let t2 = std::cmp::max(parse_time(f3)?, t1);
    Some((t1, t2 - t1))
}

// parse 04:02.500
fn parse_time(time: &str) -> Option<u32> {
    // Split in [hh:]mm:ss and subseconds.
    let mut fields = if time.contains(".") {
        time.split('.')
    } else {
        time.split(',')
    };
    let time = fields.next()?;
    let ms = fields.next().map(|f| f.parse::<u32>().ok()).flatten()?;

    // hh:mm::ss
    if let Ok((h, m, s)) = scan_fmt!(time, "{}:{}:{}{e}", u32, u32, u32) {
        if h > 24 || m > 60 || s > 60 {
            return None;
        } else {
            return Some(1000 * (h * 3600 + m * 60 + s) + ms);
        }
    }

    // mm:ss
    if let Ok((m, s)) = scan_fmt!(time, "{}:{}{e}", u32, u32) {
        if m > 60 || s > 60 {
            return None;
        } else {
            return Some(1000 * (m * 60 + s) + ms);
        }
    }

    None
}

fn format_time(line: &mut String, format: Format, mut time: u32) {
    use std::fmt::Write;
    let ms = time % 1000;
    time /= 1000;
    let s = time % 60;
    time /= 60;
    let m = time % 60;
    time /= 60;
    let h = time;
    let sep = if format == Format::Srt { ',' } else { '.' };
    let _ = write!(line, "{:02}:{:02}:{:02}{}{:03}", h, m, s, sep, ms);
}

// remove all tags except <i>, </i>, <b>, </b>, <u>, </u>.
fn remove_unsupported_tags(line: impl Into<String>) -> String {
    let line = line.into();
    if !line.contains("<") && !line.contains(">") {
        return line;
    }
    let mut r = String::new();
    let mut iter = line.chars();
    while let Some(c) = iter.next() {
        if c == '<' {
            let tag = parse_tag(&mut iter);
            match tag.as_str() {
                "i" | "/i" | "b" | "/b" | "u" | "/u" => {
                    r.push('<');
                    r.push_str(&tag);
                    r.push('>');
                },
                _ => {},
            }
            continue;
        }
        r.push(c);
    }
    r
}

fn parse_tag(iter: &mut std::str::Chars) -> String {
    let mut r = String::new();
    while let Some(c) = iter.next() {
        if c == '>' {
            if let Some(idx) = r.find('.') {
                r.truncate(idx);
            }
            if let Some(idx) = r.find(|c: char| c.is_whitespace()) {
                r.truncate(idx);
            }
            return r;
        }
        if c == '\'' || c == '"' {
            r.push(c);
            while let Some(d) = iter.next() {
                r.push(d);
                if d == c {
                    break;
                }
            }
            continue;
        }
        r.push(c);
    }
    r
}
