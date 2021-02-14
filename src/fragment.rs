use crate::mp4box::{MP4, MP4Box};
use crate::types::*;
use crate::boxes::*;

/// Build a Media Initialization Section for fMP4 segments.
pub fn media_init_section(mp4: &MP4, track_ids: &[u32]) -> MP4 {

    let mut boxes = Vec::new();

    // Start with the FileType box.
    let ftyp = FileTypeBox {
        major_brand:    FourCC::new("isom"),
        minor_version:  1,
        compatible_brands: vec![
            FourCC::new("avc1"),
            FourCC::new("iso6"),
        ],
    };
    boxes.push(MP4Box::FileTypeBox(ftyp));

    // Now the moviebox.
    let movie = mp4.movie();
    let mut movie_boxes = Vec::new();
    let mut mvex_boxes = Vec::new();
    let mut track_id = 0;
    let mut new_track_id = 0;

    for box_ in &movie.boxes {
        match box_ {
            MP4Box::MovieHeaderBox(header) => {
                // initialize MovieExtendsHeaderBox from this box.
                mvex_boxes.push(MP4Box::MovieExtendsHeaderBox(MovieExtendsHeaderBox{
                    fragment_duration: header.duration,
                }));
                // always copy this box.
                let header = header.clone();
                movie_boxes.push(MP4Box::MovieHeaderBox(header.clone()));
            },
            MP4Box::TrackBox(track) => {
                // only the selected tracks.
                track_id += 1;
                if !track_ids.iter().any(|&id| id == track_id) {
                    continue;
                }
                new_track_id += 1;
                let track_box = fmp4_track(track, new_track_id);
                movie_boxes.push(MP4Box::TrackBox(track_box));
                let trex_box = track_extends(track, new_track_id);
                mvex_boxes.push(MP4Box::TrackExtendsBox(trex_box));
            },
            _ => {},
        }
    }

    // add the MovieExtendsBox.
    movie_boxes.push(MP4Box::MovieExtendsBox(MovieExtendsBox{ boxes: mvex_boxes }));

    // finally add the MovieBox to the top level boxes!
    boxes.push(MP4Box::MovieBox(MovieBox{ boxes: movie_boxes }));

    MP4 {
        boxes,
        data_ref: mp4.data_ref.clone(),
        input_file: mp4.input_file.clone(),
    }
}

// Create a TrackExtendsBox from the original trackbox.
fn track_extends(trak: &TrackBox, track_id: u32) -> TrackExtendsBox {
    let mut trex = TrackExtendsBox::default();
    trex.track_id = track_id;

    let mdia = trak.media();
    let minf = mdia.media_info();
    let sample_table = minf.sample_table();

    if first_box!(sample_table, SyncSampleBox).is_some() {
        // has a SyncSampleBox, so most samples are not sync
        trex.default_sample_flags.sample_depends_on = 1;
        trex.default_sample_flags.sample_is_non_sync_sample = true;
    } else {
        // no SyncSampleBox, every sample is sync.
        trex.default_sample_flags.sample_depends_on = 2;
    }

    // For the default duration, simply take the duration of the first sample.
    if let Some(stts) = first_box!(sample_table, TimeToSampleBox) {
        if stts.entries.len() > 0 {
            trex.default_sample_duration = stts.entries[0].delta;
        }
    }

    trex
}

// Build a new TrackBox.
fn fmp4_track(trak: &TrackBox, track_id: u32) -> TrackBox {
    let mut boxes = Vec::new();

    // add TrackHeaderBox.
    let mut tkhd = trak.track_header().clone();
    tkhd.track_id = track_id;
    boxes.push(MP4Box::TrackHeaderBox(tkhd));

    // add EditListBox, if present.
    if let Some(edts) = first_box!(&trak.boxes, EditListBox) {
        boxes.push(MP4Box::EditListBox(edts.clone()));
    }

    // media box.
    let mdia = trak.media();
    let mut media_boxes = Vec::new();

    // add media header.
    let mut header = mdia.media_header().clone();
    header.duration = Duration_::default();
    media_boxes.push(MP4Box::MediaHeaderBox(header));

    // add handler. copy, but change "sbtl" => "subt".
    let hdlr = mdia.handler();
    let b = hdlr.handler_type.to_be_bytes();
    match &b[..] {
        b"sbtl"|b"subt" => {
            media_boxes.push(MP4Box::HandlerBox(HandlerBox {
                handler_type:   FourCC::new("subt"),
                name:           ZString("SubtitleHandler".into()),
            }));
        },
        _ => {
            media_boxes.push(MP4Box::HandlerBox(hdlr.clone()));
        }
    }

    // extended language tag, if present.
    if let Some(elng) = first_box!(&mdia.boxes, ExtendedLanguageBox) {
        media_boxes.push(MP4Box::ExtendedLanguageBox(elng.clone()));
    }

    // Media Information Box.
    let minf = mdia.media_info();
    let hdlr = mdia.handler();
    let handler_type = hdlr.handler_type.to_be_bytes();

    // first, add the HeaderBox.
    let mut media_info_boxes = Vec::new();
    if let Some(vmhd) = first_box!(&minf.boxes, VideoMediaHeaderBox) {
        media_info_boxes.push(MP4Box::VideoMediaHeaderBox(vmhd.clone()));
    }
    if let Some(smhd) = first_box!(&minf.boxes, SoundMediaHeaderBox) {
        media_info_boxes.push(MP4Box::SoundMediaHeaderBox(smhd.clone()));
    }
    if let Some(sthd) = first_box!(&minf.boxes, SubtitleMediaHeaderBox) {
        media_info_boxes.push(MP4Box::SubtitleMediaHeaderBox(sthd.clone()));
    }
    if let Some(_) = first_box!(&minf.boxes, NullMediaHeaderBox) {
        if handler_type == *b"subt" || handler_type == *b"sbtl" {
            media_info_boxes.push(MP4Box::SubtitleMediaHeaderBox(SubtitleMediaHeaderBox::default()));
        } else {
            media_info_boxes.push(MP4Box::NullMediaHeaderBox(NullMediaHeaderBox::default()));
        }
    }

    // add DataInformationBox.
    let dinf = minf.data_information().clone();
    media_info_boxes.push(MP4Box::DataInformationBox(dinf));

    // Sample Table box.
    let mut sample_boxes = Vec::new();

    // Boxes that need to be cloned.
    let sample_desc = minf.sample_table().sample_description().clone();
    sample_boxes.push(MP4Box::SampleDescriptionBox(sample_desc));

    // Add empty boxes.
    sample_boxes.push(MP4Box::TimeToSampleBox(TimeToSampleBox::default()));
    sample_boxes.push(MP4Box::SampleToChunkBox(SampleToChunkBox::default()));
    sample_boxes.push(MP4Box::SampleSizeBox(SampleSizeBox::default()));
    sample_boxes.push(MP4Box::ChunkOffsetBox(ChunkOffsetBox::default()));
    sample_boxes.push(MP4Box::CompositionOffsetBox(CompositionOffsetBox::default()));
    sample_boxes.push(MP4Box::SyncSampleBox(SyncSampleBox::default()));

    // Conditionally present empty boxes.
    if minf.sample_table().sync_samples().is_some() {
        sample_boxes.push(SyncSampleBox::default().to_mp4box());
    }

    // add the sample boxes to MediaInfo.
    media_info_boxes.push(SampleTableBox{ boxes: sample_boxes }.to_mp4box());

    // now add the MediaInformationBox to the MediaBox.
    media_boxes.push(MediaInformationBox{ boxes: media_info_boxes }.to_mp4box());

    // And add media to the track.
    boxes.push(MP4Box::MediaBox(MediaBox{ boxes: media_boxes }));

    TrackBox{ boxes }
}

/// Generate a MovieFragmentBox + MediaDataBox for a range of samples.
pub fn movie_fragment(mp4: &MP4, track_id: u32, seq_num: u32, samples: std::ops::Range<u32>) {
    let mut boxes = Vec::new();
    let movie = mp4.movie();
    let mut mdat = Vec::<u8>::new();

    // Header.
    boxes.push(MovieFragmentHeaderBox{ sequence_number: seq_num }.to_mp4box());

    let track = movie.track_by_id(track_id).unwrap();

    let mut traf = TrackFragmentHeaderBox::default();
    traf.track_id = track.track_id();
    traf.base_data_offset = Some(0);


}

