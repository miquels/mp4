use std::fs::File;
use std::io::{self, BufWriter, Write};

use anyhow::Result;
use clap;
use structopt::StructOpt;

use mp4::io::Mp4File;
use mp4::mp4box::{read_boxes, write_boxes};

#[derive(StructOpt, Debug)]
#[structopt(setting = clap::AppSettings::VersionlessSubcommands)]
pub struct MainOpts {
    #[structopt(short, long)]
    /// Maximum log verbosity: debug (info)
    pub debug:  bool,
    #[structopt(short, long)]
    /// Maximum log verbosity: debug (trace)
    pub trace:  bool,
    #[structopt(subcommand)]
    pub cmd:    Command,
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
    pub track: Option<u32>,

    /// Input filename.
    pub input: String,
    /// Output filename.
    pub output: String,
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
    }
}

fn dump(opts: DumpOpts) -> Result<()> {
    let infh = File::open(&opts.input)?;

    let mut reader = Mp4File::new(infh);
    let boxes = read_boxes(&mut reader)?;

    let stdout = io::stdout();
    let mut handle = BufWriter::with_capacity(128000, stdout.lock());
    let _ = writeln!(handle, "{:#?}", boxes);

    Ok(())
}

fn rewrite(opts: RewriteOpts) -> Result<()> {
    let infh = File::open(&opts.input)?;

    let mut reader = Mp4File::new(infh);
    let boxes = read_boxes(&mut reader)?;

    let outfh = File::create(&opts.output)?;
    let writer = Mp4File::new(outfh);
    write_boxes(writer, &boxes)?;

    Ok(())
}

