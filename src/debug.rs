use crate::mp4box::MP4;
use crate::sample_info::{SampleInfo, sample_info_iter};

/// Dump sample information.
pub fn dump_track(mp4: &MP4, track_id: u32) {

    let movie = mp4.movie();

    // Find the track by id.
    let track_idx = match movie.track_idx_by_id(track_id) {
        Some(idx) => idx,
        None => {
            log::debug!("track id {}: no such track", track_id);
            return;
        }
    };

    let samples = sample_info_iter(movie.tracks()[track_idx]);
    let mut count = 0;
    let timescale = samples.timescale();

    let mut next_pos = 1;
    println!("{} {:>8}  {:>10}  {:>6}  {:>10}  {:>6}  {:>5}  {:>7}",
             " ", "#", "filepos", "size", "dtime", "cdelta", "sync", "chunkno");
    for (idx, sample) in samples.enumerate() {
        let dtime = sample.dtime as f64 / (timescale as f64);
        let ctime_d = 1000f64 * sample.ctime_d as f64 / (timescale as f64);
        let is_sync = if sample.is_sync { "sync" } else { "" };
        let jump = if next_pos != sample.fpos { "+" } else { " " };
        next_pos = sample.fpos + sample.size as u64;
        count += 1;
        println!("{} {:>8}  {:>10}  {:>6}  {:>10.1}  {:>6.0}  {:>5}  {:>7}",
                 jump, idx, sample.fpos, sample.size, dtime, ctime_d, is_sync, sample.chunkno);
    }
    println!("Sample vector size: {} bytes", count * std::mem::size_of::<SampleInfo>());
}

