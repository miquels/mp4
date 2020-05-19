use crate::mp4box::MP4;
use crate::boxes::MP4Box;
use crate::io::CountBytes;
use crate::serialize::{BoxBytes, ToBytes};

/// Set the default track.
pub fn set_default_track(mp4: &mut MP4, track_id: u32) {

    let movie = mp4.movie();

    // Find the track by id.
    let track_idx = match movie.track_idx_by_id(track_id) {
        Some(idx) => idx,
        None => {
            debug!("track id {}: no such track", track_id);
            return;
        }
    };
    let t = movie.tracks()[track_idx];

    // Find the first track with the same handler type.
    let first_idx = match movie.track_idx_by_handler(t.media().handler().handler_type) {
        Some(idx) => idx,
        None => return,
    };

    // Already the default track?
    if first_idx == track_idx {
        debug!("set_default_audio_track: already default");
        return;
    }

    let mut tracks = mp4.movie_mut().tracks_mut();
    debug!("set_default_audio_track: setting {} as default", track_id);

    // Swap the enabled flag, but set the first track to enabled always.
    let was_enabled = tracks[track_idx].track_header().flags.get_enabled();
    tracks[first_idx].track_header_mut().flags.set_enabled(was_enabled);
    tracks[track_idx].track_header_mut().flags.set_enabled(true);

    // swap the tracks.
    tracks.swap(first_idx, track_idx);
}

/// Move the "moov" box to the front.
pub fn movie_at_front(mp4: &mut MP4) {

    // Get the index and offset of the moov and mdat boxes.
    let mut mdat_offset = 0u64;
    let mut mdat_size = 0u64;
    let mut moov_offset = 0u64;
    let mut mdat_idx = None;
    let mut moov_idx = None;

    let mut offset = 0u64;
    for (idx, b) in mp4.boxes.iter().enumerate() {
        if let &MP4Box::MovieBox(_) = b {
            moov_idx = Some(idx);
            moov_offset = offset;
        }
        if let &MP4Box::MediaDataBox(ref m) = b {
            mdat_idx = Some(idx);
            mdat_offset = offset;
            mdat_size = m.data.data_size + 12;
        }
        if mdat_idx.is_some() && moov_idx.is_some() {
            break;
        }
        let mut cb = CountBytes::new();
        b.to_bytes(&mut cb).unwrap();
        offset += cb.size();
    }

    // If moov is already before mdat, we're done.
    if moov_offset <= mdat_offset {
        debug!("movie_at_front: MovieBox is already at the start");
        return;
    }

    // Check if all tracks do indeed fall in the first mdat.
    for t in mp4.movie().tracks().iter() {
        let stbl = t.media().media_info().sample_table();
        let co = stbl.chunk_offset();
        let co_len = co.entries.len();
        if co_len > 0 {
            if co.entries[0] < mdat_offset || co.entries[co_len - 1] >= mdat_offset + mdat_size {
                error!("movie_at_front: not all tracks in first MovieDataBox");
                return;
            }
        }
    }

    // now, get the size of the MovieBox.
    let mut cb = CountBytes::new();
    mp4.movie().to_bytes(&mut cb).unwrap();
    let size = cb.size();

    // Then move all the chunk offsets.
    for t in mp4.movie_mut().tracks_mut() {
        let stbl = t.media_mut().media_info_mut().sample_table_mut();
        stbl.move_chunk_offsets_up(size);
    }

    // Then move the MovieBox to the front of the MP4.
    mp4.boxes.swap(mdat_idx.unwrap(), moov_idx.unwrap());
}

