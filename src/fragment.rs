use std::io;

use crate::boxes::*;
use crate::mp4box::{MP4Box, MP4};
use crate::types::*;

/// Build a Media Initialization Section for fMP4 segments.
pub fn media_init_section(mp4: &MP4, track_ids: &[u32]) -> MP4 {
    let mut boxes = Vec::new();

    // Start with the FileType box.
    let ftyp = FileTypeBox {
        major_brand:       FourCC::new("isom"),
        minor_version:     1,
        compatible_brands: vec![FourCC::new("avc1"), FourCC::new("iso6")],
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
                mvex_boxes.push(MP4Box::MovieExtendsHeaderBox(MovieExtendsHeaderBox {
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
    movie_boxes.push(MP4Box::MovieExtendsBox(MovieExtendsBox { boxes: mvex_boxes }));

    // finally add the MovieBox to the top level boxes!
    boxes.push(MP4Box::MovieBox(MovieBox { boxes: movie_boxes }));

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
        b"sbtl" | b"subt" => {
            media_boxes.push(MP4Box::HandlerBox(HandlerBox {
                handler_type: FourCC::new("subt"),
                name:         ZString("SubtitleHandler".into()),
            }));
        },
        _ => {
            media_boxes.push(MP4Box::HandlerBox(hdlr.clone()));
        },
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
    media_info_boxes.push(SampleTableBox { boxes: sample_boxes }.to_mp4box());

    // now add the MediaInformationBox to the MediaBox.
    media_boxes.push(
        MediaInformationBox {
            boxes: media_info_boxes,
        }
        .to_mp4box(),
    );

    // And add media to the track.
    boxes.push(MP4Box::MediaBox(MediaBox { boxes: media_boxes }));

    TrackBox { boxes }
}

// Some values are constant for the entire trackfragment, or even the
// entire track. For now this is pretty coarse and we mostly check
// the whole track for defaults.
struct SampleDefaults {
    sample_duration:                Option<u32>,
    sample_flags:                   Option<SampleFlags>,
    sample_size:                    Option<u32>,
    sample_composition_time_offset: Option<i32>,
}

impl SampleDefaults {
    fn new(track: &TrackBox, from: u32, to: u32) -> SampleDefaults {
        let tables = track.media().sample_table();

        // If the TimeToSample box has just one entry, it covers all
        // samples so that one entry is the default.
        let entries = &tables.time_to_sample().entries;
        let sample_duration = if entries.len() == 1 {
            Some(entries[0].delta)
        } else {
            None
        };

        // We have a SyncSampleBox. Skip the first sample. Then if the rest is all
        // sync or all non-sync, use that as the default.
        let sample_flags;
        if let Some(sync_table) = tables.sync_samples() {
            let mut is_sync = 0_u32;
            for entry in &sync_table.entries {
                if *entry > from && *entry <= to {
                    is_sync += 1;
                }
                if *entry > to {
                    break;
                }
            }
            if is_sync == 0 || is_sync == to - from {
                sample_flags = Some(build_sample_flags(is_sync > 0));
            } else {
                sample_flags = None;
            }
        } else {
            // No SyncSampleBox means all samples are sync.
            sample_flags = Some(build_sample_flags(true));
        }

        // If there is no composition offset box, the default is 0.
        let sample_composition_time_offset = if tables.composition_time_to_sample().is_some() {
            None
        } else {
            Some(0)
        };

        SampleDefaults {
            sample_duration,
            sample_flags,
            sample_size: None,
            sample_composition_time_offset,
        }
    }
}


fn build_sample_flags(is_sync: bool) -> SampleFlags {
    let mut flags = SampleFlags::default();
    if is_sync {
        flags.sample_depends_on = 2;
    } else {
        flags.sample_is_non_sync_sample = true;
    }
    flags
}

fn default_or<A, B>(dfl: &Option<A>, val: B) -> Option<B> {
    if dfl.is_some() {
        None
    } else {
        Some(val)
    }
}

/// Generate a MovieFragmentBox + MediaDataBox for a range of samples.
pub fn movie_fragment(mp4: &MP4, track_id: u32, seq_num: u32, from: u32, to: u32) -> io::Result<Vec<MP4Box>> {
    let movie = mp4.movie();
    let mut mdat = MediaDataBox::default();

    let track = movie
        .track_by_id(track_id)
        .ok_or(ioerr!(NotFound, "{}: no such track", track_id))?;
    let trex =
        movie
            .track_extends_by_id(track_id)
            .ok_or(ioerr!(NotFound, "{}: no TrackExtendsBox", track_id))?;

    // Track fragment.
    let mut traf = TrackFragmentBox::default();

    // Track fragment header.
    let mut tfhd = TrackFragmentHeaderBox::default();
    tfhd.track_id = track.track_id();
    tfhd.default_base_is_moof = true;
    traf.boxes.push(tfhd.to_mp4box());

    // Track run box.
    let sample_count = to - from + 1;
    let mut samples = track.sample_info_iter();
    samples.seek(from)?;
    let first_sample = samples.clone().next().ok_or(ioerr!(UnexpectedEof))?;
    let flags = build_sample_flags(first_sample.is_sync);
    let first_sample_flags = if trex.default_sample_flags != flags {
        Some(flags)
    } else {
        None
    };
    let mut trun = TrackRunBox {
        sample_count,
        data_offset: Some(0),
        first_sample_flags,
        entries: ArrayUnsized::<TrackRunEntry>::new(),
    };

    let dfl = SampleDefaults::new(track, from, to);

    for sample in samples.take(sample_count as usize) {
        let entry = TrackRunEntry {
            sample_duration:                default_or(&dfl.sample_duration, sample.duration),
            sample_flags:                   default_or(&dfl.sample_flags, build_sample_flags(sample.is_sync)),
            sample_size:                    default_or(&dfl.sample_size, sample.size),
            sample_composition_time_offset: default_or(
                &dfl.sample_composition_time_offset,
                sample.composition_delta,
            ),
        };
        trun.entries.push(entry);
    }

    traf.boxes.push(trun.to_mp4box());

    // Now build moof.
    let mut moof = MovieFragmentBox::default();

    // Movie fragment header.
    moof.boxes.push(
        MovieFragmentHeaderBox {
            sequence_number: seq_num,
        }
        .to_mp4box(),
    );

    // Track fragment.
    moof.boxes.push(traf.to_mp4box());

    // moof + mdat.
    let mut boxes = Vec::new();
    boxes.push(moof.to_mp4box());
    boxes.push(mdat.to_mp4box());

    Ok(boxes)
}
