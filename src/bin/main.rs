use std::fs::File;

use anyhow::Result;

use mp4::io::Mp4File;
use mp4::mp4box::read_boxes;

fn main() -> Result<()> {
    let file = File::open(std::env::args().skip(1).next().unwrap())?;

    let mut rdr = Mp4File::new(file);
    let base = read_boxes(&mut rdr)?;

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
