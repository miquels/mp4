use crate::mp4box::MP4;

#[derive(Default, Debug)]
pub struct Sample {
    // File position.
    pub fpos:    u64,
    // Size.
    pub size:   u32,
    // Decode time.
    pub dtime:  u64,
    // Composition time delta.
    pub ctime_d:  i32,
    // is it a sync sample
    pub is_sync:    bool,
    // what chunkno is it in.
    pub chunkno:  u32,
}

/// Set the default track.
pub fn dump_track(mp4: &MP4, track_id: u32) {

    let movie = mp4.movie();

    // Find the track by id.
    let track_idx = match movie.track_idx_by_id(track_id) {
        Some(idx) => idx,
        None => {
            debug!("track id {}: no such track", track_id);
            return;
        }
    };
    let trak = movie.tracks()[track_idx];
    let mdhd = trak.media().media_header();
    let stbl = trak.media().media_info().sample_table();
    let media_timescale = mdhd.timescale;

    let shift = trak.composition_time_shift().unwrap_or(0) as i32;

    let mut stts_iter = stbl.time_to_sample_iter();
    let mut ctts_iter = stbl.composition_time_to_sample_iter();
    let mut stsc_iter = stbl.sample_to_chunk_iter();

    let chunk_offset = stbl.chunk_offset();
    let mut fpos = if chunk_offset.entries.len() > 0 {
        chunk_offset.entries[0]
    } else {
        0
    };

    // Now loop over all entries.
    let mut samples = Vec::new();
    let mut dtime = 0;
    let mut last_chunk = 0;
    let is_sync = stbl.sync_samples().is_none();

    for size in &stbl.sample_size().entries {

        let mut sample = Sample{ fpos, size: *size, is_sync, ..Sample::default() };
        fpos += *size as u64;

        if let Some(time) = stts_iter.next() {
            sample.dtime = dtime;
            dtime += time as u64;
        }
        if let Some(delta) = ctts_iter.next() {
            sample.ctime_d = delta - shift;
        }
        if let Some(chunk) = stsc_iter.next() {
            sample.chunkno = chunk.chunk;
            if last_chunk != chunk.chunk {
                last_chunk = chunk.chunk;
                // XXX FIXME check chunk.chunk for index overflow
                fpos = chunk_offset.entries[chunk.chunk as usize];
            }
        }

        samples.push(sample);
    }
    
    if let Some(sync_samples) = stbl.sync_samples() {
        for index in &sync_samples.entries {
            let idx = (*index).saturating_sub(1) as usize;
            if idx < samples.len() {
                samples[idx].is_sync = true;
            }
        }
    }
    println!("{} bytes", samples.len() * std::mem::size_of::<Sample>());

    /*
    let mut next_pos = 1;
    println!("{} {:>8}  {:>10}  {:>6}  {:>10}  {:>6}  {:>5}  {:>7}",
             " ", "#", "filepos", "size", "dtime", "cdelta", "sync", "chunkno");
    for (idx, sample) in samples.iter().enumerate() {
        let dtime = sample.dtime as f64 / (media_timescale as f64);
        let ctime_d = 1000f64 * sample.ctime_d as f64 / (media_timescale as f64);
        let is_sync = if sample.is_sync { "sync" } else { "" };
        let jump = if next_pos != sample.fpos { "+" } else { " " };
        next_pos = sample.fpos + sample.size as u64;
        println!("{} {:>8}  {:>10}  {:>6}  {:>10.1}  {:>6.0}  {:>5}  {:>7}",
                 jump, idx, sample.fpos, sample.size, dtime, ctime_d, is_sync, sample.chunkno);
    }*/
}

