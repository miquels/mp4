use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::os::unix::fs::FileExt;

use anyhow::{anyhow, Result};
use clap;
use structopt::StructOpt;

use mp4lib::boxes::*;
use mp4lib::debug;
use mp4lib::first_box;
use mp4lib::fragment::FragmentSource;
use mp4lib::io::Mp4File;
use mp4lib::ioerr;
use mp4lib::iter_box;
use mp4lib::mp4box::{MP4Box, MP4};
use mp4lib::subtitle;

#[derive(StructOpt, Debug)]
#[structopt(setting = clap::AppSettings::VersionlessSubcommands)]
pub struct MainOpts {
    #[structopt(long)]
    /// Log options (like RUSTLOG; trace, debug, info etc)
    pub log: Option<String>,
    #[structopt(subcommand)]
    pub cmd: Command,
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub enum Command {
    #[structopt(display_order = 1)]
    /// Media information.
    Mediainfo(MediainfoOpts),

    #[structopt(display_order = 2)]
    /// Rewrite the mp4 file.
    Rewrite(RewriteOpts),

    #[structopt(display_order = 3)]
    /// extract subtitles.
    Subtitles(SubtitlesOpts),

    #[structopt(display_order = 4)]
    /// fragment an mp4 file.
    Fragment(FragmentOpts),

    #[structopt(display_order = 5)]
    /// interleave and optimize an mp4 file.
    Interleave(InterleaveOpts),

    #[structopt(display_order = 6)]
    /// Show the boxes.
    Boxes(BoxesOpts),

    #[structopt(display_order = 7)]
    /// Dump a track from the mp4 file
    Dump(DumpOpts),

    #[structopt(display_order = 8)]
    /// Debugging.
    Debug(DebugOpts),
}

#[derive(StructOpt, Debug)]
pub struct RewriteOpts {
    #[structopt(short, long)]
    /// Fragment the file.
    pub fragment: bool,
    #[structopt(short, long)]
    /// Select track.
    pub track:    Option<u32>,

    /// Input filename.
    pub input:  String,
    /// Output filename.
    pub output: String,
}

#[derive(StructOpt, Debug)]
pub struct MediainfoOpts {
    #[structopt(short, long)]
    /// Select track.
    pub track: Option<u32>,

    #[structopt(short, long)]
    /// Short output, 1 line per track.
    pub short: bool,

    #[structopt(short, long)]
    /// Output in JSON
    pub json: bool,

    /// Input filename.
    pub input: String,
}

#[derive(StructOpt, Debug)]
pub struct SubtitlesOpts {
    #[structopt(short, long)]
    /// Select track.
    pub track: u32,

    #[structopt(short, long)]
    /// Format (vtt, srt, tx3g)
    pub format: mp4lib::subtitle::Format,

    /// Input filename.
    pub input: String,
}

#[derive(StructOpt, Debug)]
pub struct FragmentOpts {
    #[structopt(long, use_delimiter = true)]
    /// Select primary track.
    pub track: u32,

    #[structopt(long, use_delimiter = true)]
    /// Select secondary track.
    pub track2: Option<u32>,

    #[structopt(short, long)]
    /// Fragments of fixed duration (milliseconds)
    pub duration: Option<u32>,

    /// Input filename.
    pub input: String,

    /// Output filename.
    pub output: String,
}

#[derive(StructOpt, Debug)]
pub struct InterleaveOpts {
    #[structopt(short, long, use_delimiter = true)]
    /// Select tracks.
    pub tracks: Vec<u32>,

    /// Input filename.
    pub input: String,

    /// Output filename.
    pub output: String,
}

#[derive(StructOpt, Debug)]
pub struct BoxesOpts {
    #[structopt(short, long)]
    /// Select a track.
    pub track: Option<u32>,

    /// Input filename.
    pub input: String,
}

#[derive(StructOpt, Debug)]
pub struct DumpOpts {
    #[structopt(short, long)]
    /// Select a track.
    pub track: u32,

    /// Input filename.
    pub input: String,
}

#[derive(StructOpt, Debug)]
pub struct DebugOpts {
    #[structopt(short, long)]
    /// Select a track.
    pub track: Option<u32>,

    #[structopt(short, long)]
    /// Show the HLS master playlist.
    pub hls: bool,

    #[structopt(short, long)]
    /// Show the samples for a track.
    pub samples: bool,

    #[structopt(short, long)]
    /// Fragment a track on sync sample boundaries
    pub fragment: bool,

    #[structopt(long, default_value = "1")]
    /// First sample to dump.
    pub from: u32,

    #[structopt(long, default_value = "0")]
    /// Last sample to dump.
    pub to: u32,

    #[structopt(long)]
    /// Dump timestamps of all Track Fragments.
    pub traf: bool,

    /// Input filename.
    pub input: String,
}


fn main() -> Result<()> {
    let opts = MainOpts::from_args();

    let mut builder = env_logger::Builder::new();
    if let Some(ref log_opts) = opts.log {
        builder.parse_filters(log_opts);
    } else if let Ok(ref log_opts) = std::env::var("RUST_LOG") {
        builder.parse_filters(log_opts);
    } else {
        builder.parse_filters("info");
    }
    builder.init();

    match opts.cmd {
        Command::Boxes(opts) => return boxes(opts),
        Command::Debug(opts) => return debug(opts),
        Command::Dump(opts) => return dump(opts),
        Command::Fragment(opts) => return fragment(opts),
        Command::Interleave(opts) => return interleave(opts),
        Command::Mediainfo(opts) => return mediainfo(opts),
        Command::Rewrite(opts) => return rewrite(opts),
        Command::Subtitles(opts) => return subtitles(opts),
    }
}

fn rewrite(opts: RewriteOpts) -> Result<()> {
    let mut reader = Mp4File::open(&opts.input)?;
    let mut mp4 = MP4::read(&mut reader)?;

    mp4lib::rewrite::movie_at_front(&mut mp4);

    let writer = File::create(&opts.output)?;
    mp4.write(writer)?;

    Ok(())
}

fn subtitles(opts: SubtitlesOpts) -> Result<()> {
    let mut reader = Mp4File::open(&opts.input)?;
    let mp4 = MP4::read(&mut reader)?;

    let movie = mp4.movie();
    let tracks = movie.tracks();
    let track = match movie.track_idx_by_id(opts.track) {
        Some(idx) => &tracks[idx],
        None => return Err(anyhow!("subtitles: track id {} not found", opts.track)),
    };

    let stdout = io::stdout();
    let mut handle = stdout.lock();
    subtitle::subtitle_extract(&mp4, track, opts.format, &mut handle)?;

    Ok(())
}

fn fragment(opts: FragmentOpts) -> Result<()> {
    let mut reader = Mp4File::open(&opts.input)?;
    let mp4 = MP4::read(&mut reader)?;
    let mut tracks = Vec::new();

    let track = mp4
        .movie()
        .track_by_id(opts.track)
        .ok_or(anyhow!("track {} not found", opts.track))?;
    let segments = mp4lib::segment::track_to_segments(track, opts.duration)?;
    tracks.push(opts.track);

    // See if we wanted an extra track.
    let mut segments2 = Vec::new();
    let mut track2 = 0;
    if let Some(t2) = opts.track2 {
        let track = mp4
            .movie()
            .track_by_id(t2)
            .ok_or(anyhow!("track {} not found", t2))?;
        segments2 = mp4lib::segment::track_to_segments_timed(track, &segments)?;
        track2 = t2;
        tracks.push(track2);
    }

    // Media init section (empty moov).
    let mut mp4_frag = mp4lib::fragment::media_init_section(&mp4, &tracks);

    let mut segments_iter = segments.iter();
    let mut segments2_iter = segments2.iter();

    // moof + mdats.
    let mut seq = 1;
    let mut prev_seq = 0;
    let mut frag_src = Vec::new();
    while seq > prev_seq {
        frag_src.truncate(0);
        prev_seq = seq;

        // Primary segment.
        if let Some(segment) = segments_iter.next() {
            let fs = FragmentSource {
                src_track_id: opts.track,
                dst_track_id: 1,
                from_sample:  segment.start_sample,
                to_sample:    segment.end_sample,
            };
            frag_src.push(fs);
            //let mut frag = mp4lib::fragment::movie_fragment(&mp4, seq, &[ fs ])?;
            //mp4_frag.boxes.append(&mut frag);
            //seq += 1;
        }

        // Optional extra segment.
        if let Some(segment) = segments2_iter.next() {
            let fs = FragmentSource {
                src_track_id: track2,
                dst_track_id: 2,
                from_sample:  segment.start_sample,
                to_sample:    segment.end_sample,
            };
            frag_src.push(fs);
            //let mut frag = mp4lib::fragment::movie_fragment(&mp4, seq, &[ fs ])?;
            //mp4_frag.boxes.append(&mut frag);
            //seq += 1;
        }
        if frag_src.len() == 0 {
            break;
        }
        let mut frag = mp4lib::fragment::movie_fragment(&mp4, seq, &frag_src)?;
        mp4_frag.boxes.append(&mut frag);
        seq += 1;
    }

    let writer = File::create(&opts.output)?;
    mp4_frag.write(writer)?;

    Ok(())
}

fn interleave(opts: InterleaveOpts) -> Result<()> {
    let mut reader = mp4lib::pseudo::Mp4Stream::open(&opts.input, &opts.tracks[..])
        .map_err(|e| ioerr!(e.kind(), "{}: {}", opts.input, e))?;
    let mut writer = File::create(&opts.output)?;

    let mut buf = Vec::<u8>::new();
    buf.resize(128000, 0);

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| ioerr!(e.kind(), "{}: read: {}", opts.input, e))?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n])?;
    }

    Ok(())
}

fn short(track: &mp4lib::track::TrackInfo) {
    println!(
        "{}. type [{}], length {:?}, lang {}, codec {}",
        track.id, track.track_type, track.duration, track.language, track.specific_info
    );
}

fn mediainfo(opts: MediainfoOpts) -> Result<()> {
    let mut reader = Mp4File::open(&opts.input)?;
    let mp4 = MP4::read(&mut reader)?;
    let mp4 = mp4.clone();

    let res = mp4lib::track::track_info(&mp4);
    if let Some(track) = opts.track {
        for t in &res {
            if t.id == track {
                if opts.short {
                    if opts.json {
                        let json = serde_json::to_string(t)?;
                        println!("{}", json);
                    } else {
                        short(t);
                    }
                } else {
                    if opts.json {
                        let json = serde_json::to_string_pretty(t)?;
                        println!("{}", json);
                    } else {
                        println!("{:#?}", t);
                    }
                }
            }
        }
    } else {
        if opts.short {
            for t in &res {
                if opts.json {
                    let json = serde_json::to_string(t)?;
                    println!("{}", json);
                } else {
                    short(t);
                }
            }
        } else {
            if opts.json {
                let json = serde_json::to_string_pretty(&res)?;
                println!("{}", json);
            } else {
                println!("{:#?}", res);
            }
        }
    }

    Ok(())
}

fn dump(opts: DumpOpts) -> Result<()> {
    let mut reader = Mp4File::open(&opts.input)?;
    let mp4 = MP4::read(&mut reader)?;

    let infh = reader.file();
    let stdout = io::stdout();
    let mut handle = BufWriter::with_capacity(128000, stdout.lock());
    let mut buffer = Vec::new();

    if let Some(movie) = mp4lib::first_box!(mp4, MovieBox) {
        let tracks = movie.tracks();
        let track = match movie.track_idx_by_id(opts.track) {
            Some(idx) => &tracks[idx],
            None => return Err(anyhow!("dump: track id {} not found", opts.track)),
        };

        // tracks.
        for sample_info in track.sample_info_iter() {
            let sz = sample_info.size as usize;
            if buffer.len() < sz {
                buffer.resize(sz, 0);
            }
            infh.read_exact_at(&mut buffer[..sz], sample_info.fpos)?;
            handle.write_all(&buffer[..sz])?;
        }
    }

    let dfl_sample_size = 0;

    for moof in iter_box!(mp4, MovieFragmentBox) {
        for traf in iter_box!(moof, TrackFragmentBox) {
            let tfhd = first_box!(traf, TrackFragmentHeaderBox).unwrap();
            if !tfhd.default_base_is_moof {
                return Err(anyhow!(
                    "dump: track_id {}: default_base_is_moof not set in fragment",
                    opts.track
                ));
            }
            for trun in iter_box!(traf, TrackRunBox) {
                let offset = moof.offset + trun.data_offset.unwrap_or(0) as u64;
                let mut size = 0;
                for entry in &trun.entries {
                    size += entry.sample_size.unwrap_or(dfl_sample_size);
                }
                let sz = size as usize;
                if buffer.len() < sz {
                    buffer.resize(sz, 0);
                }
                infh.read_exact_at(&mut buffer[..sz], offset)?;
                handle.write_all(&buffer[..sz])?;
            }
        }
    }

    Ok(())
}

fn boxes(opts: BoxesOpts) -> Result<()> {
    let mut reader = Mp4File::open(&opts.input)?;
    let mut mp4 = MP4::read(&mut reader)?;

    if let Some(opt_track) = opts.track {
        // filter out tracks we don't want.
        let mut boxes = Vec::new();
        let mut movie = mp4.movie_mut();
        let mut trak_id = 0;
        for box_ in movie.boxes.drain(..) {
            match &box_ {
                MP4Box::TrackBox(_) => {
                    trak_id += 1;
                    if opt_track == trak_id {
                        boxes.push(box_);
                    }
                },
                _ => boxes.push(box_),
            }
        }
        movie.boxes = boxes;

        // now do the same at the top level for moof/mdat.
        let mut boxes = Vec::new();
        let mut t_found = false;
        for box_ in mp4.boxes.drain(..) {
            match &box_ {
                MP4Box::MovieFragmentBox(ref moof) => {
                    t_found = moof.track_fragments().iter().any(|frag| {
                        if let Some(hdr) = frag.track_fragment_header() {
                            hdr.track_id == opt_track
                        } else {
                            false
                        }
                    });
                    if t_found {
                        boxes.push(box_);
                    }
                },
                MP4Box::MediaDataBox(_) => {
                    if t_found {
                        boxes.push(box_);
                    }
                },
                _ => boxes.push(box_),
            }
        }
        mp4.boxes = boxes;
    }

    let stdout = io::stdout();
    let mut handle = BufWriter::with_capacity(128000, stdout.lock());
    let _ = writeln!(handle, "{:#?}", mp4);

    return Ok(());
}

fn debug(opts: DebugOpts) -> Result<()> {
    let mut reader = Mp4File::open(&opts.input)?;
    let mp4 = MP4::read(&mut reader)?;

    if opts.hls {
        let m3u = if let Some(track) = opts.track {
            mp4lib::stream::hls_track(&mp4, track)?
        } else {
            mp4lib::stream::hls_master(&mp4)
        };
        print!("{}", m3u);
        return Ok(());
    }

    if opts.samples {
        let track = match opts.track {
            Some(track) => track,
            None => return Err(anyhow!("debug: samples: need --track")),
        };
        debug::dump_track_samples(&mp4, track, opts.from, opts.to)?;
        return Ok(());
    }

    if opts.fragment {
        let track = match opts.track {
            Some(track) => {
                mp4.movie()
                    .track_by_id(track)
                    .ok_or(anyhow!("track {} not found", track))?
            },
            None => return Err(anyhow!("debug: fragment: need --track")),
        };
        let segments = mp4lib::segment::track_to_segments(track, None)?;
        let longest = segments.iter().fold(0_f64, |max, t| {
            if t.duration.partial_cmp(&max) == Some(std::cmp::Ordering::Greater) {
                t.duration
            } else {
                max
            }
        });
        println!("longest sample: {}s", longest);
        println!("{:#?}", segments);
    }

    if opts.traf {
        debug::dump_traf_timestamps(&mp4);
        return Ok(());
    }

    Err(anyhow!("debug: no options"))
}
