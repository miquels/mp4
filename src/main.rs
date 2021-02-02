use std::fs::File;
use std::io::{self, BufWriter, Read, Seek, Write};

use anyhow::{anyhow, Result};
use clap;
use structopt::StructOpt;

use mp4::io::Mp4File;
use mp4::mp4box::{MP4, MP4Box};
use mp4::debug;
use mp4::subtitle;

#[derive(StructOpt, Debug)]
#[structopt(setting = clap::AppSettings::VersionlessSubcommands)]
pub struct MainOpts {
    #[structopt(long)]
    /// Log options (like RUSTLOG; trace, debug, info etc)
    pub log: Option<String>,
    #[structopt(subcommand)]
    pub cmd:   Command,
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
    /// Dump the mp4 file
    Dump(DumpOpts),

    #[structopt(display_order = 5)]
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
    /// Debug a track.
    pub debugtrack: bool,

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


fn short(track: &mp4::track::TrackInfo) {
    println!("{}. type [{}], length {:?}, lang {}, codec {}",
        track.id, track.track_type, track.duration, track.language, track.specific_info);
}

fn mediainfo(opts: MediainfoOpts) -> Result<()> {
    let mut reader = Mp4File::open(&opts.input)?;
    let mp4 = MP4::read(&mut reader)?;

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

    let mut infh = reader.into_inner();
    infh.seek(io::SeekFrom::Start(0))?;

    let tracks = movie.tracks();
    let track = match movie.track_idx_by_id(opts.track) {
        Some(idx) => &tracks[idx],
        None => return Err(anyhow!("dump: track id {} not found", opts.track)),
    };

    let stdout = io::stdout();
    let mut handle = BufWriter::with_capacity(128000, stdout.lock());

    let stbl = track.media().media_info().sample_table();
    let mut stsc_iter = stbl.sample_to_chunk().iter();
    let chunk_offset = stbl.chunk_offset();

    // Can be empty.
    if stbl.sample_size().entries.len() == 0 {
        return Ok(());

    }
    if chunk_offset.entries.len() == 0 {
        return Err(anyhow!("dump: chunk offset table empty"));
    }

    let mut fpos = 0;
    let mut this_chunk = 0xffffffff;

    for size in &stbl.sample_size().entries {

        if let Some(chunk) = stsc_iter.next() {
            if this_chunk != chunk.chunk {
                this_chunk = chunk.chunk;
                fpos = chunk_offset.entries[this_chunk as usize];
            }
        }

        infh.seek(io::SeekFrom::Start(fpos))?;
        let mut sm = infh.take(*size as u64);
        io::copy(&mut sm, &mut handle)?;
        infh = sm.into_inner();

        fpos += *size as u64;
    }

    Ok(())
}

fn debug(opts: DebugOpts) -> Result<()> {
    let mut reader = Mp4File::open(&opts.input)?;
    let mp4 = MP4::read(&mut reader)?;

    if opts.debugtrack {
        let track = match opts.track {
            Some(track) => track,
            None => return Err(anyhow!("debug: debugtrack: need --track")),
        };
        debug::dump_track(&mp4, track);
        return Ok(());
    }

    if opts.boxes {
        let stdout = io::stdout();
        let mut handle = BufWriter::with_capacity(128000, stdout.lock());
        let _ = writeln!(handle, "{:#?}", mp4);
        return Ok(());
    }

    Err(anyhow!("debug: no options"))
}