//! Segment a track.
//!
//! Cut a track into segments, either on sync (I-Frame) boundaries,
//! or on fixed intervals.
//!
use crate::boxes::*;
use std::cmp::Ordering;
use std::io;

// For the first 6 seconds.
const MAX_SEGMENT_DURATION_INIT: f64 = 2.01;
// Normal max.
const MAX_SEGMENT_DURATION: f64 = 6.01;
// Max if this is the first fragment of a segment and it's short (<1.2s),
// or if the fragment we're merging into this segment is non-sync,
// to improve the chances of it being merged.
const MAX_SEGMENT_DURATION_LONG: f64 = 10.01;

/// A segment.
///
/// Use the `SampleInfo` iterator to get per-sample information:
/// seek to `start_sample` and then iterate through to end_sample.
///
#[derive(Default, Clone, Debug)]
pub struct Segment {
    pub start_sample: u32,
    pub end_sample: u32,
    pub start_time: f64,
    pub duration: f64,
}

#[derive(Default, Clone, Debug)]
struct Fragment {
    start_sample: u32,
    end_sample: u32,
    start_time: f64,
    duration: f64,
    size: u32,
    is_sync: bool,
}

// Subtitle fragments are not in-sync wrt start/end time with
// the main track. We could have as many fragments as lines
// in the .vtt file. Still, merge fragments that are 'close'.
fn merge_fragments_subtitle(s: Vec<Fragment>) -> Vec<Segment> {
    let mut v = Vec::new();
    let mut idx = 0;

    while idx < s.len() {
        // Create a new Segment struct from the Fragment struct.
        let st = &s[idx];
        let mut sb = Segment {
            start_sample: st.start_sample,
            end_sample: st.end_sample,
            start_time: st.start_time,
            duration: st.duration,
        };
        let mut is_empty = st.size <= 2;
        idx += 1;

        // Merge leading empty segment(s)
        while idx < s.len() && s[idx].size <= 2 {
            sb.end_sample = s[idx].end_sample;
            sb.duration += s[idx].duration;
            idx += 1;
        }

        // Merge segments up to 10 seconds, or 20 seconds if the
        // trailing segments are empty.
        if !is_empty || sb.duration < 5.0 {
            let mut duration = sb.duration;
            while idx < s.len() {
                let sn = &s[idx];
                let d = duration + sn.duration;
                if (sn.size > 2 && d > 10.0) || d > 20.0 {
                    break;
                }
                duration += sn.duration;
                if sn.size > 2 {
                    is_empty = false;
                }
                sb.end_sample = sn.end_sample;
                sb.duration += sn.duration;
                idx += 1;
            }
        }

        // Optimization for when the segment is empty.
        if is_empty {
            sb.start_sample = 0;
            sb.end_sample = 0;
        }

        v.push(sb);
    }
    v
}

// Merge short fragments together, we want to have segments of at least a
// few seconds ideally.
fn merge_fragments(s: Vec<Fragment>, max_segment_size: u32) -> (Vec<Segment>, u64) {
    let mut v = Vec::new();
    let mut max_bw = 0u64;
    let mut idx = 0;
    let mut current_time = 0.0;

    while idx < s.len() {
        // Create a new Segment struct from the Fragment struct.
        let st = &s[idx];
        let mut sb = Segment {
            start_sample: st.start_sample,
            end_sample: st.end_sample,
            start_time: st.start_time,
            duration: st.duration,
        };
        let mut segment_size = st.size;
        idx += 1;

        // Merge short segments.
        //
        // TODO: when creating the fmp4 segment, and the segment exists of
        // multiple fragments, put the fragments in multiple MOOF/MDAT frags
        // and flush after each one. Some players can take advantage of that.
        let start_idx = idx;
        while idx < s.len() {
            let sn = &s[idx];

            let short_initial = idx == start_idx && sb.duration < 1.2;
            let max_duration = if current_time < MAX_SEGMENT_DURATION {
                MAX_SEGMENT_DURATION_INIT
            } else if short_initial || !sn.is_sync {
                MAX_SEGMENT_DURATION_LONG
            } else {
                MAX_SEGMENT_DURATION
            };

            // Stop if the segment would become to long or too big, however
            // make sure that the first segment is always at least two seconds.
            let too_long = sb.duration + sn.duration > max_duration;
            let too_big = max_segment_size > 0 && segment_size + sn.size > max_segment_size;
            if (too_big || too_long) && current_time > 2.0 {
                break;
            }

            sb.end_sample = sn.end_sample;
            sb.duration += sn.duration;
            segment_size += sn.size;
            current_time += sn.duration;
            idx += 1;
        }

        let bw = (segment_size as f64 / sb.duration) as u64;
        if bw > max_bw {
            max_bw = bw;
        }

        v.push(sb);
    }
    (v, max_bw)
}

/// Parse a track into segments, each starting at a sync sample.
///
/// Returns an array with sample start/end, start time, and duration.
///
/// If `segment_duration` is `Some(milliseconds)`, we just create a new segment every
/// `segment_duration` milliseconds.
///
/// You use this to segment the video tracks into segments. The
/// resulting timing data can then be used to segment the audio
/// track(s) into segments with the exact same start_time and duration.
pub fn track_to_segments(
    trak: &TrackBox,
    segment_duration: Option<u32>,
    max_segment_size: Option<u32>,
) -> io::Result<Vec<Segment>> {
    let handler = trak.media().handler();
    let fragments = track_to_fragments(trak, segment_duration, max_segment_size)?;
    let segments = if !handler.is_subtitle() {
        let (segments, _) = merge_fragments(fragments, max_segment_size.unwrap_or(0));
        segments
    } else {
        merge_fragments_subtitle(fragments)
    };
    Ok(segments)
}

pub(crate) fn track_segment_peak_bw(
    trak: &TrackBox,
    segment_duration: Option<u32>,
    max_segment_size: Option<u32>,
) -> io::Result<u64> {
    let fragments = track_to_fragments(trak, segment_duration, max_segment_size)?;
    let (_, bw) = merge_fragments(fragments, max_segment_size.unwrap_or(0));
    Ok(bw)
}

fn track_to_fragments(
    trak: &TrackBox,
    fragment_duration: Option<u32>,
    max_fragment_size: Option<u32>,
) -> io::Result<Vec<Fragment>> {
    let media = trak.media();
    let table = media.media_info().sample_table();
    let handler = media.handler();

    let ts64 = media.media_header().timescale as u64;
    let timescale = ts64 as f64;
    let comp_time_shift = trak.composition_time_shift(true)?;
    let mut fragment_duration = fragment_duration.map(|d| ((d as u64 * ts64) / 1000) as u32);

    let mut stts_iter = table.time_to_sample().iter();
    let mut stsz_iter = table.sample_size().iter();
    let mut stss_iter = match table.sync_samples() {
        Some(stss) => Some(stss.iter()),
        None => {
            // println!("subtitles");
            if !handler.is_subtitle() {
                return Err(ioerr!(InvalidData, "track {}: no SyncSampleBox"));
            }
            fragment_duration = None;
            None
        },
    };
    // println!("stss_iter is {:?}", stss_iter.is_some());

    let mut cur_time = comp_time_shift;
    let mut cur_frag_duration = comp_time_shift;
    let mut cur_frag_size = 0u32;
    let mut fragments = Vec::new();
    let mut cur_fragment = Fragment::default();
    cur_fragment.start_sample = 1;
    cur_fragment.is_sync = true;

    for cur_sample in 1..=u32::MAX {

        let sample_duration = match stts_iter.next() {
            Some((d, _)) => d,
            None => {
                // No more samples, we're done. Finish the last fragment.
                if cur_fragment.start_sample > 0 && cur_frag_duration > 0 {
                    cur_fragment.end_sample = cur_sample - 1;
                    cur_fragment.duration = cur_frag_duration as f64 / timescale;
                    cur_fragment.size = cur_frag_size;
                    fragments.push(cur_fragment.clone());
                }
                break;
            },
        };

        let sample_size = stsz_iter.next().unwrap_or(0);

        let mut is_sync = true;
        let do_next_frag = match fragment_duration {
            Some(d) => cur_frag_duration >= d.into(),
            None => {
                is_sync = if let Some(stss_iter) = stss_iter.as_mut() {
                    stss_iter.next().unwrap_or(false)
                } else {
                    // no stss iter? every sample is a sync sample.
                    true
                };
                if cur_frag_duration <= 0 {
                    false
                } else if is_sync {
                    true
                } else {
                    // Cut the fragment here if it would become too big.
                    // Some players (e.g. my Chromecast) cannot handle
                    // large fragments. I suspect it runs out of memory.
                    max_fragment_size
                        .map(|m| cur_frag_size + sample_size > m)
                        .unwrap_or(false)
                }
            },
        };

        if do_next_frag {
            // Finish the previous fragment and push it onto the vec.
            cur_fragment.end_sample = cur_sample - 1;
            cur_fragment.duration = cur_frag_duration as f64 / timescale;
            cur_fragment.size = cur_frag_size;
            fragments.push(cur_fragment.clone());

            // start a new segment.
            cur_fragment.start_sample = cur_sample;
            cur_fragment.start_time = cur_time as f64 / timescale;
            cur_fragment.is_sync = is_sync;
            cur_frag_duration = 0;
            cur_frag_size = 0;
        }
        cur_frag_duration += sample_duration as i64;
        cur_time += sample_duration as i64;
        cur_frag_size += sample_size;
    }

    Ok(fragments)
}

/// Parse a track into segments, based on a list of segment time/duration.
///
/// Used for audio tracks.
pub fn track_to_segments_timed(trak: &TrackBox, timing_segments: &[Segment]) -> io::Result<Vec<Segment>> {
    let media = trak.media();
    let table = media.media_info().sample_table();

    let timescale = media.media_header().timescale as f64;
    let comp_time_shift = trak.composition_time_shift(true)?;

    let mut stts_iter = table.time_to_sample().iter();
    let mut ctts_iter = table.composition_time_to_sample().map(|ctts| ctts.iter());

    let mut cur_time = comp_time_shift;
    let mut seg_duration = comp_time_shift;
    let mut segments = Vec::new();
    let mut cur_segment = Segment::default();
    cur_segment.start_sample = 1;

    // If our timing_segments track is shorter than this track, keep on
    // going - generate intervals that are similar to what came before.
    let seg_time = timing_segments
        .iter()
        .fold(1.0, |acc, x| if x.duration > acc { x.duration } else { acc });
    let mut timing_segments_iter = timing_segments.iter();
    let mut next_segment_end_time = move |last_segment_end_time| {
        timing_segments_iter
            .next()
            .map(|s| s.start_time + s.duration)
            .unwrap_or_else(|| last_segment_end_time + seg_time)
    };
    let mut segment_end_time = next_segment_end_time(0.0);

    for cur_sample in 1..=u32::MAX {
        let delta = ctts_iter.as_mut().and_then(|iter| iter.next()).unwrap_or(0);

        let sample_duration = match stts_iter.next() {
            Some((d, _)) => d,
            None => {
                // No more samples, we're done. Finish the last segment.
                if cur_segment.start_sample > 0 && seg_duration > 0 {
                    cur_segment.end_sample = cur_sample - 1;
                    cur_segment.duration = seg_duration as f64 / timescale;
                    segments.push(cur_segment.clone());
                }
                break;
            },
        };

        // calculate composition time of this sample.
        let cur_comp_time = (cur_time + (delta as i64)) as f64 / timescale;

        // if composition time >= current segment start + duration,
        // we have to start a new segment.
        if cur_comp_time.partial_cmp(&segment_end_time) != Some(Ordering::Less) {
            // Finish the previous segment and push it onto the vec.
            cur_segment.end_sample = cur_sample - 1;
            cur_segment.duration = seg_duration as f64 / timescale;
            segments.push(cur_segment.clone());

            // start new segment.
            cur_segment.start_sample = cur_sample;
            cur_segment.start_time = cur_comp_time;
            seg_duration = 0;
            segment_end_time = next_segment_end_time(segment_end_time);
        }
        seg_duration += sample_duration as i64;
        cur_time += sample_duration as i64;
    }

    Ok(segments)
}
