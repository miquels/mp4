//! Subtitle handling.
//!
use std::io;
use std::io::Write;
use std::str::FromStr;

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

#[doc(hidden)]
pub fn subtitle_dump(mp4: &MP4, track: &TrackBox) {
    let mut count = 0;
    for sample in track.sample_info_iter() {
        count += 1;
        let mut subt = mp4.data_ref(sample.fpos, sample.size as u64);
        let text = Tx3GTextSample::from_bytes(&mut subt).unwrap();
        println!("{:02} {:?}", count, text);
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
    _sample: SampleInfo,
    subt: Tx3GTextSample,
    count: u32,
    start: f64,
    end: f64,
) -> String {
    use std::fmt::Write;
    let eol = if format == Format::Vtt { "\n" } else { "\r\n" };
    let mut cue = String::new();
    let _ = write!(cue, "{}{}", count, eol);
    let _ = write!(cue, "{} --> {}{}", ptime(start, format), ptime(end, format), eol);
    for line in subt.text.split('\n') {
        if format == Format::Vtt {
            cue.push_str("- ");
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
    let mut prev_text = None;
    let mut count = 1;

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

    for sample in iter {
        let mut data = mp4.data_ref(sample.fpos, sample.size as u64);
        let subt = match Tx3GTextSample::from_bytes(&mut data) {
            Ok(subt) => subt,
            Err(_) => continue,
        };
        if format == Format::Tx3g {
            output.write(&data)?;
            continue;
        }
        let endtime = sample.decode_time as f64 / (timescale as f64);
        if let Some((subt, sample)) = prev_text.replace((subt, sample)) {
            if subt.text.as_str() == "" {
                continue;
            }
            let starttime = sample.decode_time as f64 / (timescale as f64);
            let cue = cue(format, sample, subt, count, starttime, endtime);
            write!(output, "{}{}", cue, eol)?;
            count += 1;
        }
    }
    if let Some((subt, sample)) = prev_text.take() {
        if subt.text.as_str() != "" {
            let starttime = sample.decode_time as f64 / (timescale as f64);
            let cue = cue(format, sample, subt, count, starttime, starttime + 5f64);
            writeln!(output, "{}{}", cue, eol)?;
        }
    }
    Ok(())
}
