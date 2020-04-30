use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};

use anyhow::Result;

use mp4::io::Mp4File;
use mp4::mp4box::read_boxes;

fn main() -> Result<()> {
    let file = File::open(std::env::args().skip(1).next().unwrap())?;
    let file = BufReader::new(file);

    let mut rdr = Mp4File::new(file);
    let base = read_boxes(&mut rdr)?;

    let stdout = io::stdout();
    let mut handle = BufWriter::with_capacity(128000, stdout.lock());
    let _ = writeln!(handle, "{:#?}", base);

    Ok(())
}

