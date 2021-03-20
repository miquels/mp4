//! Segment a track.
//!
//! Cut a track into segments, either on sync (I-Frame) boundaries,
//! or on fixed intervals.
//!
use std::cmp::Ordering;
use std::io;

use crate::boxes::*;

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
    let mut cur_segment = Segment::default();
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
                    segments.push(cur_segment.clone());
                }
                break;
            },
        };

        let do_next_seg = match segment_duration {
            Some(d) => cur_seg_duration >= d,
            None => {
                if let Some(stss_iter) = stss_iter.as_mut() {
                    stss_iter.next().unwrap_or(false)
                } else {
                    // no stss iter? every sample is a sync sample.
                    true
                }
            },
        };
        if do_next_seg || cur_sample == 1 {
            // A new segment starts here.
            if cur_sample > 1 {
                // Finish the previous segment and push it onto the vec.
                cur_segment.end_sample = cur_sample - 1;
                cur_segment.duration = cur_seg_duration as f64 / timescale;
                segments.push(cur_segment.clone());
            }
            cur_segment.start_sample = cur_sample;
            let tm = cur_time as i64 + (delta as i64) - (comp_time_shift as i64);
            let tm = std::cmp::max(0, tm);
            cur_segment.start_time = tm as f64 / timescale;
            cur_seg_duration = 0;
        }
        cur_seg_duration += sample_duration;
        cur_time += sample_duration;
    }

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
