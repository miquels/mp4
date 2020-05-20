use std::fs::File;
use std::io::{self, BufWriter, Write};

#[macro_use]
extern crate log;

use anyhow::Result;
use clap;
use structopt::StructOpt;

use mp4::io::Mp4File;
use mp4::mp4box::MP4;
use mp4::debug;

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
    /// Dump the mp4 file
    Dump(DumpOpts),
    #[structopt(display_order = 2)]
    /// Rewrite the mp4 file.
    Rewrite(RewriteOpts),
    #[structopt(display_order = 3)]
    /// Track information.
    Trackinfo(TrackinfoOpts),
    #[structopt(display_order = 4)]
    /// Debugging.
    Debug(DebugOpts),
}

#[derive(StructOpt, Debug)]
pub struct DumpOpts {
    /// Input filename.
    pub input: String,
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
pub struct TrackinfoOpts {
    #[structopt(short, long)]
    /// Select track.
    pub track: Option<u32>,

    /// Input filename.
    pub input: String,
}

#[derive(StructOpt, Debug)]
pub struct DebugOpts {
    #[structopt(short, long)]
    /// Debug track.
    pub debugtrack: Option<u32>,

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
        Command::Dump(opts) => return dump(opts),
        Command::Rewrite(opts) => return rewrite(opts),
        Command::Trackinfo(opts) => return trackinfo(opts),
        Command::Debug(opts) => return debug(opts),
    }
}

fn dump(opts: DumpOpts) -> Result<()> {
    let infh = File::open(&opts.input)?;

    let mut reader = Mp4File::new(infh);
    let mp4 = MP4::read(&mut reader)?;

    let stdout = io::stdout();
    let mut handle = BufWriter::with_capacity(128000, stdout.lock());
    let _ = writeln!(handle, "{:#?}", mp4);

    Ok(())
}

fn rewrite(opts: RewriteOpts) -> Result<()> {
    let infh = File::open(&opts.input)?;

    let mut reader = Mp4File::new(infh);
    let mut mp4 = MP4::read(&mut reader)?;

    mp4::rewrite::movie_at_front(&mut mp4);

    let outfh = File::create(&opts.output)?;
    let writer = Mp4File::new_with_reader(outfh, reader.into_inner());
    mp4.write(writer)?;

    Ok(())
}

fn trackinfo(opts: TrackinfoOpts) -> Result<()> {
    let infh = File::open(&opts.input)?;

    let mut reader = Mp4File::new(infh);
    let mp4 = MP4::read(&mut reader)?;

    let res = mp4::track::track_info(&mp4);
    if let Some(track) = opts.track {
        for t in &res {
            if t.id == track {
                println!("{:#?}", t);
            }
        }
    } else {
        println!("{:#?}", res);
    }

    Ok(())
}

fn debug(opts: DebugOpts) -> Result<()> {
    let infh = File::open(&opts.input)?;

    let mut reader = Mp4File::new(infh);
    let mp4 = MP4::read(&mut reader)?;

    if let Some(track) = opts.debugtrack {
        debug::dump_track(&mp4, track);
        return Ok(());
    }

    error!("debug: no options");

    Ok(())
}
