use std::fs::File;
use std::io::{self, BufWriter, Write};

use anyhow::Result;
use clap;
use structopt::StructOpt;

use mp4::io::Mp4File;
use mp4::mp4box::MP4;

#[derive(StructOpt, Debug)]
#[structopt(setting = clap::AppSettings::VersionlessSubcommands)]
pub struct MainOpts {
    #[structopt(short, long)]
    /// Maximum log verbosity: debug (info)
    pub debug: bool,
    #[structopt(short, long)]
    /// Maximum log verbosity: debug (trace)
    pub trace: bool,
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
    /// Trac information.
    TrackInfo(TrackInfoOpts),
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
pub struct TrackInfoOpts {
    #[structopt(short, long)]
    /// Select track.
    pub track: Option<u32>,

    /// Input filename.
    pub input: String,
}

fn main() -> Result<()> {
    env_logger::init();

    let opts = MainOpts::from_args();

    if opts.trace {
        log::set_max_level(log::LevelFilter::Trace);
    } else if opts.debug {
        log::set_max_level(log::LevelFilter::Debug);
    } else {
        log::set_max_level(log::LevelFilter::Info);
    }

    match opts.cmd {
        Command::Dump(opts) => return dump(opts),
        Command::Rewrite(opts) => return rewrite(opts),
        Command::TrackInfo(opts) => return trackinfo(opts),
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
    let mp4 = MP4::read(&mut reader)?;

    let outfh = File::create(&opts.output)?;
    let writer = Mp4File::new_with_reader(outfh, reader.into_inner());
    mp4.write(writer)?;

    Ok(())
}

fn trackinfo(opts: TrackInfoOpts) -> Result<()> {
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
