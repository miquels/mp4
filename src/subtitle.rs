//! Subtitle handling.
//!
use std::time::Duration;

use crate::boxes::*;
use crate::mp4box::MP4;
use crate::serialize::FromBytes;
use crate::mp4box::BoxInfo;

pub fn dump_subtitle(mp4: &MP4, language: &str) {
    let movie = mp4.movie();
    let mvhd = movie.movie_header();

    for track in &movie.tracks() {
        let tkhd = track.track_header();
        let mdia = track.media();
        let mdhd = mdia.media_header();
        let hdlr = mdia.handler();

        if hdlr.handler_type != b"sbtl" {
            continue;
        }
        if mdhd.language.to_string() != language {
            continue;
        }

        let duration = Duration::from_millis((1000 * tkhd.duration.0) / (mvhd.timescale as u64));

        let stsd = mdia.media_info().sample_table().sample_description();
        match stsd.entries.iter().next() {
            Some(entry) => {
                if entry.fourcc() != b"tx3g" {
                    continue;
                }
            },
            None => continue,
        }

        let mut count = 0;
        for sample in track.sample_info_iter() {
            count += 1;
            let mut subt = mp4.data_ref(sample.fpos, sample.size as u64);
            let text = Tx3GTextSamp::from_bytes(&mut subt).unwrap();
            println!("{:02} {:?}", count, text);
        }
        break;
    }
}
