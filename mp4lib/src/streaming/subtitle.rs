//! Subtitle handling.
//!
use std::fs;
use std::io::{self, BufRead, Read, Seek, Write};
use std::mem;
use std::str::FromStr;

use scan_fmt::scan_fmt;

use super::fragment::FragmentSource;
use crate::boxes::sbtl::Tx3GTextSample;
use crate::boxes::*;
use crate::mp4box::BoxInfo;
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

/// Find the first subtitle track with a certain language.
pub fn subtitle_track_bylang<'a>(mp4: &'a MP4, language: &str) -> Option<&'a TrackBox> {
    let movie = mp4.movie();

    for track in &movie.tracks() {
        let mdia = track.media();
        let mdhd = mdia.media_header();
        let hdlr = mdia.handler();

        if hdlr.handler_type != b"sbtl" {
            continue;
        }
        if mdhd.language.to_string() != language {
            continue;
        }

        let stsd = mdia.media_info().sample_table().sample_description();
        match stsd.entries.iter().next() {
            Some(entry) => {
                if entry.fourcc() != b"tx3g" {
                    continue;
                }
            },
            None => continue,
        }

        return Some(*track);
    }
    None
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
    let endtime = starttime + (sample.duration as f64 / (timescale as f64));
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
        //if format == Format::Vtt {
        //    cue.push_str("- ");
        //}
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
    cue
}

/// Extract a subtitle track into VTT / SRT or 3GPP.
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
    let eol = if format == Format::Vtt { "\n" } else { "\r\n" };

    let mut buf = Vec::new();
    buf.resize(256, 0);

    for sample in iter {
        if buf.len() < sample.size as usize {
            buf.resize(sample.size as usize, 0);
        }
        let data = &mut buf[..sample.size as usize];
        mp4.data_ref.read_exact_at(data, sample.fpos)?;
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
        write!(output, "{}{}", cue, eol)?;
        seq += 1;
    }

    Ok(())
}

/// Create a fragment containing the cue(s).
///
/// Outputs raw data. If this is to be sent in a CMAF container, it
/// still needs to be wrapped by a moof + mdat.
pub fn fragment(mp4: &MP4, format: Format, frag: &FragmentSource, tm_off: f64) -> io::Result<Vec<u8>> {
    let track = mp4
        .movie()
        .track_by_id(frag.src_track_id)
        .ok_or_else(|| ioerr!(NotFound, "track not found"))?;
    let mut iter = track.sample_info_iter();
    let timescale = iter.timescale();
    let eol = if format == Format::Vtt {
        &b"\n"[..]
    } else {
        &b"\r\n"[..]
    };

    let mut buffer = Vec::new();
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
                            buffer.extend_from_slice(eol);
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
pub fn external(path: &str, to_format: &str) -> io::Result<(&'static str, Vec<u8>)> {
    // see if input and output formats are supported.
    let infmt = Format::from_str(path)?;
    let outfmt = Format::from_str(to_format)?;
    let (eol, mime) = match outfmt {
        Format::Srt => ("\r\n", "text/plain; charset=utf-8"),
        Format::Vtt => ("\n", "text/vtt; charset=utf-8"),
        other => return Err(ioerr!(InvalidData, "unsupported input format {:?}", other)),
    };

    // open subtitle file.
    let mut stf = SubtitleFile::open(path, infmt)?;

    // shortcut for vtt -> vtt.
    if infmt == Format::Vtt && outfmt == Format::Vtt {
        let mut buf = Vec::new();
        let mut rdr = stf.file.into_inner();
        rdr.read_to_end(&mut buf)?;
        return Ok((mime, buf));
    }

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

pub fn duration(fspath: &str) -> io::Result<f64> {
    let format = Format::from_str(fspath).map_err(|e| ioerr!(InvalidData, "{}: {}", fspath, e))?;
    let mut stf = SubtitleFile::open(fspath, format)?;
    let mut duration = 0;
    while let Some(sample) = stf.next() {
        duration = std::cmp::max(duration, sample.start + sample.duration);
    }
    Ok(duration as f64 / 1000_f64)
}

struct SubtitleSample {
    start:    u32,
    duration: u32,
    text:     String,
}

struct SubtitleFile {
    file:    io::BufReader<fs::File>,
    buf:     Vec<u8>,
    is_utf8: bool,
}

impl SubtitleFile {
    fn open(path: &str, format: Format) -> io::Result<SubtitleFile> {
        let mut file = fs::File::open(path)?;
        match format {
            Format::Vtt | Format::Srt => {},
            other => return Err(ioerr!(InvalidData, "unsupported input format {:?}", other)),
        }

        // skip UTF-8 BOM.
        let mut buf = [0u8; 3];
        let mut is_bom = false;
        if let Ok(_) = file.read_exact(&mut buf) {
            if &buf[..] == &[0xef_u8, 0xbb, 0xbf] {
                is_bom = true;
            }
        }
        if !is_bom {
            file.seek(io::SeekFrom::Start(0))?;
        }

        Ok(SubtitleFile {
            file:    io::BufReader::new(file),
            buf:     Vec::new(),
            is_utf8: true,
        })
    }

    fn read_line(&mut self, line: &mut String) -> io::Result<usize> {
        let start_len = line.len();

        // read next line and change CRLF -> LF.
        // we re-use the read buffer.
        let mut buf = mem::replace(&mut self.buf, Vec::new());
        buf.truncate(0);
        let len = self.file.read_until(b'\n', &mut buf)?;
        if len == 0 {
            return Ok(0);
        }
        if buf.ends_with(b"\r\n") {
            let len = buf.len();
            buf.truncate(len - 1);
            buf[len - 2] = b'\n';
        }
        mem::swap(&mut buf, &mut self.buf);

        // Convert to utf-8.
        if self.is_utf8 {
            match std::str::from_utf8(&self.buf) {
                Ok(l) => line.push_str(l),
                Err(_) => self.is_utf8 = false,
            }
        }
        if !self.is_utf8 {
            // It's not utf-8, so pretend it's latin-1.
            self.buf.iter().for_each(|&b| line.push(b as char));
        }

        Ok(line.len() - start_len)
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
