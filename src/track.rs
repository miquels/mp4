use std::time::Duration;

use crate::mp4box::BoxInfo;
use crate::boxes::*;
use crate::types::*;

/// General track information.
#[derive (Debug, Default)]
pub struct TrackInfo {
    pub id:             u32,
    pub track_type:     String,
    pub duration:       Duration,
    pub language:       IsoLanguageCode,
    pub codec_id:       String,
    pub codec_name:     String,
    pub audio_channels: u8,
}

macro_rules! pick_or {
    ($e:expr, $($tt:tt)+) => {
        match $e {
            Some(v) => v,
            None => $($tt)+,
        }
    };
}

/// Extract general track information for all tracks in the movie.
pub fn track_info(base: &[MP4Box]) -> Vec<TrackInfo> {
    let mut v = Vec::new();

    let moov = pick_or!(first_box!(&base, MovieBox), return v);
    let mvhd = pick_or!(first_box!(moov, MovieHeaderBox), return v);

    for track in iter_box!(moov, TrackBox) {
        let mut info = TrackInfo::default();

        let tkhd = pick_or!(first_box!(track, TrackHeaderBox), continue);
        info.id = tkhd.track_id;
        info.duration = Duration::from_millis((1000 * tkhd.duration.0) / (mvhd.timescale as u64));

        let mdia = pick_or!(first_box!(track, MediaBox), continue);

        let mdhd = pick_or!(first_box!(mdia, MediaHeaderBox), continue);
        info.duration = Duration::from_millis((1000 * mdhd.duration.0) / (mdhd.timescale as u64));
        info.language = mdhd.language;

        let hdlr = pick_or!(first_box!(mdia, HandlerBox), continue);
        info.track_type = hdlr.handler_type.to_string();

        let stsd = pick_or!(first_box!(mdia, MediaInformationBox / SampleTableBox / SampleDescriptionBox), continue);
        if let Some(avc1) = first_box!(stsd.entries, AvcSampleEntry) {
            info.codec_id = avc1.codec_id();
            info.codec_name = avc1.codec_name().to_string();
        } else if let Some(ac3) = first_box!(stsd.entries, Ac3SampleEntry) {
            info.codec_id = ac3.codec_id();
            info.codec_name = ac3.codec_name().to_string();
        } else if let Some(aac) = first_box!(stsd.entries, AacSampleEntry) {
            info.codec_id = aac.codec_id();
            info.codec_name = aac.codec_name().to_string();
        } else {
            info.codec_id = match stsd.entries.iter().next() {
                Some(b) => b.fourcc().to_string(),
                None => "-".to_string(),
            };
        }

        v.push(info)
    }
    
    v
}
