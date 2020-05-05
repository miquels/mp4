use std::fs::File;
use std::io::{self, BufWriter, Write};

use anyhow::Result;

use mp4::io::Mp4File;
use mp4::mp4box::{read_boxes, write_boxes};

fn main() -> Result<()> {
    env_logger::init();

    let mut args = std::env::args().skip(1);
    let infile = args.next().unwrap();
    let infh = File::open(&infile)?;

    println!("Reading {}", infile);
    let mut reader = Mp4File::new(infh);
    let boxes = read_boxes(&mut reader)?;

    if let Some(outfile) = args.next() {
        println!("Writing {}", outfile);
        let outfh = File::create(outfile)?;
        let writer = Mp4File::new(outfh);
        write_boxes(writer, &boxes)?;
    } else {
        println!("## Boxes debug output:");
        let stdout = io::stdout();
        let mut handle = BufWriter::with_capacity(128000, stdout.lock());
        let _ = writeln!(handle, "{:#?}", boxes);
    }

    Ok(())
}
