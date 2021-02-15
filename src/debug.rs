//! Debug helpers.
//!
use std::io;
use crate::mp4box::{MP4, MP4Box};

/// Dump sample information.
pub fn dump_track_samples(mp4: &MP4, track_id: u32, first_sample: u32, last_sample: u32) -> io::Result<()> {

    let movie = mp4.movie();
    let first_sample = std::cmp::max(1, first_sample);

    // Find the track by id.
    let track_idx = match movie.track_idx_by_id(track_id) {
        Some(idx) => idx,
        None => {
            return Err(io::Error::new(io::ErrorKind::NotFound, format!("track id {}: no such track", track_id)));
        }
    };

    let trak = movie.tracks()[track_idx];
    let mut samples = trak.sample_info_iter();
    samples.seek(first_sample)?;

    let mut idx = first_sample;
    let timescale = samples.timescale();

    let mut next_pos = 1;
    println!("{} {:>8}  {:>10}  {:>6}  {:>10}  {:>6}  {:>5}  {:>7}",
             " ", "#", "filepos", "size", "dtime", "cdelta", "sync", "chunkno");
    for sample in samples {
        let dtime = sample.decode_time as f64 / (timescale as f64);
        let ctime_d = 1000f64 * sample.composition_delta as f64 / (timescale as f64);
        let is_sync = if sample.is_sync { "sync" } else { "" };
        let jump = if next_pos != sample.fpos { "+" } else { " " };
        next_pos = sample.fpos + sample.size as u64;
        println!("{} {:>8}  {:>10}  {:>6}  {:>10.1}  {:>6.0}  {:>5}  {:>7}",
                 jump, idx, sample.fpos, sample.size, dtime, ctime_d, is_sync, sample.chunk);
        idx += 1;
        if last_sample > 0 && idx > last_sample {
            break;
        }
    }

    Ok(())
}

/// Dump timestamps of all the Track Fragments.
pub fn dump_traf_timestamps(mp4: &MP4) {

    let ts: Vec<_> = mp4.movie().tracks().iter().map(|t| t.media().media_header().timescale).collect();
    let mut count = Vec::new();
    count.resize(ts.len(), 0u32);

    let mut time = Vec::new();
    time.resize(ts.len(), 0);

    for box_ in &mp4.boxes {
        let moof = match box_ {
            MP4Box::MovieFragmentBox(moof) => moof,
            _ => continue,
        };
        for traf in moof.track_fragments().iter() {
            let tfhd = match traf.track_fragment_header() {
                Some(tfhd) => tfhd,
                None => continue,
            };
            let dfl_dur = tfhd.default_sample_duration.unwrap_or(0);
            let id = tfhd.track_id as usize;

            let mut is_leading = None;
            let mut delta = 0;
            for trun in traf.track_run_boxes() {
                if delta == 0 {
                    if let Some(isl) = trun.first_sample_flags.as_ref().map(|f| f.is_leading) {
                        is_leading.replace(isl);
                    }
                }
                for entry in &trun.entries {
                    count[id - 1] += 1;
                    delta += entry.sample_duration.unwrap_or(dfl_dur);
                }
            }
            time[id - 1] += delta;

            let start = if let Some(tfdt) = traf.track_fragment_decode_time() {
                tfdt.base_media_decode_time.0 as f64 / (ts[id - 1] as f64)
            } else {
                time[id - 1] as f64 / (ts[id - 1] as f64)
            };
            let end = start + delta as f64 / (ts[id - 1] as f64);

            println!("{}. start: {:.05}, end: {:.05}, leading: {:?}", id, start, end, is_leading);
            if id == ts.len() {
                println!("");
            }
        }
    }
    for (c, idx) in count.iter().enumerate() {
        println!("{}: sample_count: {}", c, idx);
    }
}

