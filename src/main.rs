use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::os::unix::fs::FileExt;

use anyhow::{anyhow, Result};
use clap;
use structopt::StructOpt;

use mp4::debug;
use mp4::io::Mp4File;
use mp4::mp4box::{MP4Box, MP4};
use mp4::subtitle;

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
    /// Dump the mp4 file
    Dump(DumpOpts),

    #[structopt(display_order = 6)]
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
    pub format: mp4::subtitle::Format,

    /// Input filename.
    pub input: String,
}

#[derive(StructOpt, Debug)]
pub struct FragmentOpts {
    #[structopt(short, long, default_value = "1", use_delimiter = true)]
    /// Select tracks.
    pub tracks: Vec<u32>,

    #[structopt(long, default_value = "1")]
    /// Start sample.
    pub from: u32,

    #[structopt(long, default_value = "4294967295")]
    /// Last sample.
    pub to: u32,

    /// Input filename.
    pub input: String,

    /// Output filename.
    pub output: String,
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
    /// Show all the boxes.
    pub boxes: bool,

    #[structopt(short, long)]
    /// Show the samples for a track.
    pub samples: bool,

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
    MP4Box::check();

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
        Command::Dump(opts) => return dump(opts),
        Command::Rewrite(opts) => return rewrite(opts),
        Command::Subtitles(opts) => return subtitles(opts),
        Command::Fragment(opts) => return fragment(opts),
        Command::Mediainfo(opts) => return mediainfo(opts),
        Command::Debug(opts) => return debug(opts),
    }
}

fn rewrite(opts: RewriteOpts) -> Result<()> {
    let mut reader = Mp4File::open(&opts.input)?;
    let mut mp4 = MP4::read(&mut reader)?;

    mp4::rewrite::movie_at_front(&mut mp4);

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
        None => return Err(anyhow!("dump: track id {} not found", opts.track)),
    };

    let stdout = io::stdout();
    let mut handle = stdout.lock();
    subtitle::subtitle_extract(&mp4, track, opts.format, &mut handle)?;

    Ok(())
}

fn fragment(opts: FragmentOpts) -> Result<()> {
    let mut reader = Mp4File::open(&opts.input)?;
    let mp4 = MP4::read(&mut reader)?;

    let mut mp4_frag = mp4::fragment::media_init_section(&mp4, &opts.tracks);
    let mut moof = mp4::fragment::movie_fragment(&mp4, opts.tracks[0], 1, opts.from, opts.to)?;
    mp4_frag.boxes.append(&mut moof);

    let writer = File::create(&opts.output)?;
    mp4_frag.write(writer)?;

    Ok(())
}


fn short(track: &mp4::track::TrackInfo) {
    println!(
        "{}. type [{}], length {:?}, lang {}, codec {}",
        track.id, track.track_type, track.duration, track.language, track.specific_info
    );
}

fn mediainfo(opts: MediainfoOpts) -> Result<()> {
    let mut reader = Mp4File::open(&opts.input)?;
    let mp4 = MP4::read(&mut reader)?;
    let mp4 = mp4.clone();

    let res = mp4::track::track_info(&mp4);
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
    let movie = mp4.movie();

    let infh = reader.file();

    let tracks = movie.tracks();
    let track = match movie.track_idx_by_id(opts.track) {
        Some(idx) => &tracks[idx],
        None => return Err(anyhow!("dump: track id {} not found", opts.track)),
    };

    let stdout = io::stdout();
    let mut handle = BufWriter::with_capacity(128000, stdout.lock());

    let mut buffer = Vec::new();
    for sample_info in track.sample_info_iter() {
        let sz = sample_info.size as usize;
        if buffer.len() < sz {
            buffer.resize(sz, 0);
        }
        infh.read_exact_at(&mut buffer[..sz], sample_info.fpos)?;
        handle.write_all(&buffer[..sz])?;
    }

    Ok(())
}

fn debug(opts: DebugOpts) -> Result<()> {
    let mut reader = Mp4File::open(&opts.input)?;
    let mut mp4 = MP4::read(&mut reader)?;

    if opts.samples {
        let track = match opts.track {
            Some(track) => track,
            None => return Err(anyhow!("debug: debugtrack: need --track")),
        };
        debug::dump_track_samples(&mp4, track, opts.from, opts.to)?;
        return Ok(());
    }

    if opts.traf {
        debug::dump_traf_timestamps(&mp4);
        return Ok(());
    }

    if opts.boxes {
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

    Err(anyhow!("debug: no options"))
}
