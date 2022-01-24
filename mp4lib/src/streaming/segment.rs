//! Segment a track.
//!
//! Cut a track into segments, either on sync (I-Frame) boundaries,
//! or on fixed intervals.
//!
use std::cmp::Ordering;
use std::io;

use crate::boxes::*;

const MAX_SEGMENT_SIZE: u32 = 7_200_000;
const MAX_SEGMENT_APPEND: u32 = 800_000;
const MAX_SEGMENT_DURATION_MERGED: f64 = 6.0;

/// A segment.
///
/// Use the `SampleInfo` iterator to get per-sample information:
/// seek to `start_sample` and then iterate through to end_sample.
///
#[derive(Default, Clone, Debug)]
pub struct Segment {
    pub start_sample: u32,
    pub end_sample:   u32,
    pub start_time:   f64,
    pub duration:     f64,
}

#[derive(Default, Clone, Debug)]
struct Segment_ {
    start_sample: u32,
    end_sample:   u32,
    start_time:   f64,
    duration:     f64,
    size:         u32,
    is_sync:      bool,
}

// Subtitle fragments are not in-sync wrt start/end time with
// the main track. We could have as many fragments as lines
// in the .vtt file. Still, merge fragments that are 'close'.
fn squish_subtitle(s: Vec<Segment_>) -> Vec<Segment> {
    let mut v = Vec::new();
    let mut idx = 0;
    let mut delta_t = 0f64;

    while idx < s.len() {
        // Create a new Segment struct from the Segment_ struct.
        let st = &s[idx];
        let mut sb = Segment {
            start_sample: st.start_sample,
            end_sample:   st.end_sample,
            start_time:   st.start_time - delta_t,
            duration:     st.duration + delta_t,
        };
        delta_t = 0.0;

        // Empty segment?
        if s[idx].size <= 2 && idx < s.len() - 1 {
            if st.duration > 10.0 {
                // Long. Give some extra lead-time to the next sample.
                delta_t = 5.0;
                sb.duration -= 5.0;
            } else {
                // Short. Merge with the next sample.
                let sn = &s[idx + 1];
                sb.end_sample = sn.end_sample;
                sb.duration += sn.duration;
                idx += 1;
            }
        }

        // Segment with content?
        if s[idx].size > 2 && idx < s.len() - 1 {
            while idx < s.len() - 1 {
                let sn = &s[idx + 1];

                // always merge contiguous content.
                if sn.size > 2 || sn.duration < 1.0 {
                    sb.end_sample = sn.end_sample;
                    sb.duration += sn.duration;
                    idx += 1;
                    continue;
                }

                // empty.
                if sn.duration > 8.0 {
                    // merge, but give some extra lead-time to the next sample.
                    // shorter durations will be merged in the next iteration.
                    sb.end_sample = sn.end_sample;
                    sb.duration += sn.duration - 5.0;
                    delta_t = 5.0;
                    idx += 1;
                }
                break;
            }
        }

        v.push(sb);
        idx += 1;
    }
    v
}

fn squish(s: Vec<Segment_>) -> Vec<Segment> {
    let mut v = Vec::new();

    let mut idx = 0;
    while idx < s.len() {
        // Create a new Segment struct from the Segment_ struct.
        let st = &s[idx];
        let mut sb = Segment {
            start_sample: st.start_sample,
            end_sample:   st.end_sample,
            start_time:   st.start_time,
            duration:     st.duration,
        };
        let mut segment_duration = st.duration;
        let mut segment_size = st.size;

        // If the next segment is small, and non-sync, tack it onto this one.
        // I should have documented this better- I'm sure why we do this.
        if idx + 1 < s.len() {
            let sn = &s[idx + 1];
            if !sn.is_sync && sn.size < MAX_SEGMENT_APPEND {
                sb.end_sample = sn.end_sample;
                sb.duration += sn.duration;
                segment_size += sn.size;
                segment_duration += sn.duration;
                idx += 1;
            }
        }

        // Merge short segments. Not at the beginning though, having a
        // few short segments at the start might be better.
        //
        // TODO: when creating the fmp4 segment, put these short segments
        // in multiple MOOF/MDAT frags and flush after each one. Some
        // players can take advantage of that.
        let start_idx = idx;
        while idx > 8 && idx + 1 < s.len() {
            let sn = &s[idx + 1];
            if segment_duration + sn.duration > MAX_SEGMENT_DURATION_MERGED ||
                segment_size + sn.size > MAX_SEGMENT_SIZE
            {
                // Make an exception if the initial segment is really short.
                if !(idx == start_idx && s[idx].duration < 1.2) {
                    break;
                }
            }
            sb.end_sample = sn.end_sample;
            sb.duration += sn.duration;
            segment_size += sn.size;
            segment_duration += sn.duration;
            idx += 1;
        }

        v.push(sb);
        idx += 1;
    }
    v
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
pub fn track_to_segments(trak: &TrackBox, segment_duration: Option<u32>) -> io::Result<Vec<Segment>> {
    let media = trak.media();
    let table = media.media_info().sample_table();
    let handler = media.handler();

    let timescale_ = media.media_header().timescale as f64;
    let timescale = timescale_ as f64;
    let comp_time_shift = trak.composition_time_shift().unwrap_or(0);
    let mut segment_duration = segment_duration.map(|d| ((d as u64 * timescale_ as u64) / 1000) as u32);

    let mut stts_iter = table.time_to_sample().iter();
    let mut ctss_iter = table.composition_time_to_sample().map(|ctts| ctts.iter());
    let mut stsz_iter = table.sample_size().iter();
    let mut stss_iter = match table.sync_samples() {
        Some(stss) => Some(stss.iter()),
        None => {
            println!("subtitles");
            if !handler.is_subtitle() {
                return Err(ioerr!(InvalidData, "track {}: no SyncSampleBox"));
            }
            segment_duration = None;
            None
        },
    };
    println!("stss_iter is {:?}", stss_iter.is_some());

    let mut segments = Vec::new();
    let mut cur_time = 0;
    let mut cur_seg_duration = 0u32;
    let mut cur_seg_size = 0u32;
    let mut cur_segment = Segment_::default();
    cur_segment.start_sample = 1;

    for cur_sample in 1..=u32::MAX {
        // FIXME? with fixed durations, only update delta on sync-frames?
        let delta = ctss_iter.as_mut().and_then(|iter| iter.next()).unwrap_or(0);

        let sample_duration = match stts_iter.next() {
            Some((d, _)) => d,
            None => {
                // No more samples, we're done. Finish the last segment.
                if cur_segment.start_sample > 0 && cur_seg_duration > 0 {
                    cur_segment.end_sample = cur_sample - 1;
                    cur_segment.duration = cur_seg_duration as f64 / timescale;
                    cur_segment.size = cur_seg_size;
                    segments.push(cur_segment.clone());
                }
                break;
            },
        };

        let sample_size = stsz_iter.next().unwrap_or(0);

        let mut is_sync = true;
        let do_next_seg = match segment_duration {
            Some(d) => cur_seg_duration >= d,
            None => {
                is_sync = if let Some(stss_iter) = stss_iter.as_mut() {
                    stss_iter.next().unwrap_or(false)
                } else {
                    // no stss iter? every sample is a sync sample.
                    true
                };
                // Cut the segment here if it would become too big.
                // Some players (e.g. my Chromecast) cannot handle
                // large segments. I suspect it runs out of memory.
                if cur_seg_size + sample_size > MAX_SEGMENT_SIZE {
                    true
                } else {
                    is_sync
                }
            },
        };

        if do_next_seg || cur_sample == 1 {
            // A new segment starts here.
            if cur_sample > 1 {
                // Finish the previous segment and push it onto the vec.
                cur_segment.end_sample = cur_sample - 1;
                cur_segment.duration = cur_seg_duration as f64 / timescale;
                cur_segment.size = cur_seg_size;
                segments.push(cur_segment.clone());
            }
            cur_segment.start_sample = cur_sample;
            let tm = cur_time as i64 + (delta as i64) - (comp_time_shift as i64);
            let tm = std::cmp::max(0, tm);
            cur_segment.start_time = tm as f64 / timescale;
            cur_segment.is_sync = is_sync;
            cur_seg_duration = 0;
            cur_seg_size = 0;
        }
        cur_seg_duration += sample_duration;
        cur_seg_size += sample_size;
        cur_time += sample_duration;
    }

    let segments = if handler.is_subtitle() {
        squish_subtitle(segments)
    } else {
        squish(segments)
    };
    Ok(segments)
}

/// Parse a track into segments, based on a list of segment time/duration.
///
/// Used for audio tracks.
pub fn track_to_segments_timed(trak: &TrackBox, timing_segments: &[Segment]) -> io::Result<Vec<Segment>> {
    let media = trak.media();
    let table = media.media_info().sample_table();

    let timescale = media.media_header().timescale as f64;
    let comp_time_shift = trak.composition_time_shift().unwrap_or(0);

    let mut stts_iter = table.time_to_sample().iter();
    let mut ctss_iter = table.composition_time_to_sample().map(|ctts| ctts.iter());

    let mut cur_time = 0;
    let mut seg_duration = 0;
    let mut segments = Vec::new();
    let mut cur_segment = Segment::default();
    cur_segment.start_sample = 1;

    // If our timing_segments track is shorter than this track, keep on
    // going - generate intervals of 6 seconds.
    let mut timing_segments_iter = timing_segments.iter();
    let mut segment_end_time = 0_f64;
    let mut next_segment_end_time = move || {
        timing_segments_iter
            .next()
            .map(|s| s.start_time + s.duration)
            .unwrap_or_else(|| segment_end_time + 6_f64)
    };
    segment_end_time = next_segment_end_time();

    for cur_sample in 1..=u32::MAX {
        let delta = ctss_iter.as_mut().and_then(|iter| iter.next()).unwrap_or(0);

        let sample_duration = match stts_iter.next() {
            Some((d, _)) => d,
            None => {
                // No more samples, we're done. Finish the last segment.
                if cur_segment.start_sample > 0 {
                    cur_segment.end_sample = cur_sample - 1;
                    cur_segment.duration = seg_duration as f64 / timescale;
                    segments.push(cur_segment.clone());
                }
                break;
            },
        };

        // calculate composition time of this sample.
        let tm = cur_time as i64 + (delta as i64) - (comp_time_shift as i64);
        let tm = std::cmp::max(0, tm);
        let cur_comp_time = tm as f64 / timescale;

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
            segment_end_time = next_segment_end_time();
        }
        seg_duration += sample_duration;
        cur_time += sample_duration;
    }

    Ok(segments)
}
