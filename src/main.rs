use std::fs::File;

use anyhow::Result;

#[macro_use]
mod fromtobytes;
mod io;
mod mp4box;
mod types;

use crate::io::Mp4File;
use crate::mp4box::MP4;

fn main() -> Result<()> {
    let file = File::open(std::env::args().skip(1).next().unwrap())?;

    let mut rdr = Mp4File::new(file);
    let base = MP4::read(&mut rdr);

    println!("{:#?}", base);

    Ok(())
}

/*
fn main() -> Result<()> {
    // Spawn thread with explicit stack size
    let child = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(run)
        .unwrap();

    // Wait for thread to join
    child.join().unwrap()
}
*/
