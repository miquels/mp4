//! Convert a ISOBMFF (MP4) file to fragmented mp4 (fMP4).
//!
//! If all segments start with an independent sample (sync sample), then
//! the fMP4 segments are CMAF compatible.
//!
//! If we split on non-sync samples, the segments are technicaly
//! not valid CMAF (but will probably work with HLS and DASH).
//!
use std::convert::TryInto;
use std::fs;
use std::io;

use crate::boxes::*;
use crate::io::{CountBytes, DataRef};
use crate::mp4box::{MP4Box, MP4};
use crate::serialize::{BoxBytes, ToBytes};
use crate::types::*;

/// Passed to [`media_init_section`] and [`movie_fragment`].
#[derive(Hash, PartialEq, Eq, Clone)]
pub struct FragmentSource {
    pub src_track_id: u32,
    pub dst_track_id: u32,
    pub from_sample: u32,
    pub to_sample: u32,
}

/// Build a Media Initialization Section for fMP4 segments.
pub fn media_init_section(mp4: &MP4, tracks: &[u32]) -> MP4 {
    let mut boxes = Vec::new();

    // Start with the FileType box.
    let ftyp = FileTypeBox {
        major_brand: FourCC::new("iso5"),
        minor_version: 1,
        compatible_brands: vec![FourCC::new("avc1"), FourCC::new("mp41")],
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
                if !tracks.iter().any(|&trk| trk == track_id) {
                    continue;
                }
                new_track_id += 1;
                let track_box = fmp4_track(movie, track, new_track_id);
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

    trex.default_sample_description_index = 1;

    let dfl = SampleDefaults::new(trak, 1, u32::MAX);
    if let Some(flags) = dfl.sample_flags.as_ref() {
        trex.default_sample_flags = flags.clone();
    }
    if let Some(duration) = dfl.sample_duration {
        trex.default_sample_duration = duration;
    }
    if let Some(size) = dfl.sample_size {
        trex.default_sample_size = size;
    }

    trex
}

// Build a new TrackBox.
fn fmp4_track(movie: &MovieBox, trak: &TrackBox, track_id: u32) -> TrackBox {
    let mut boxes = Vec::new();

    // add TrackHeaderBox.
    let mut tkhd = trak.track_header().clone();
    tkhd.track_id = track_id;
    tkhd.flags.set_enabled(true);
    tkhd.flags.set_in_movie(true);
    tkhd.flags.set_in_preview(true);
    boxes.push(MP4Box::TrackHeaderBox(tkhd));

    // add EditListBox, if present.
    if let Some(elst) = trak.edit_list() {
        // We can deal with only two edits:
        // - an initial empty edit
        // - one composition time offset edit over the entire length of the track
        //   (usually used with version 0 ctts).
        //
        // If there are more than two edits, or of another type, then
        // the movie will most likely not play correctly.
        let mut new_elst = EditListBox::default();
        for idx in 0..elst.entries.len() {
            let mut entry = elst.entries[idx].clone();

            if elst.entries.len() == 1 && entry.media_time == 0 {
                // Just a single edit covering the whole track, dismiss.
                break;
            }

            if entry.media_time >= 0 {
                // XXX log warning/error if this is _not_ the last edit.
                // set duration to zero, the init segment has no frames.
                entry.segment_duration = 0;
                new_elst.entries.push(entry);
                break;
            }
            new_elst.entries.push(entry);
        }

        if new_elst.entries.len() > 0 {
            let mut edts = EditBox::default();
            edts.boxes.push(new_elst);
            boxes.push(edts.to_mp4box());
        }
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
                name: ZString::from("SubtitleHandler"),
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

    // Clone the SampleGroupDescriptionBox.
    if let Some(sgpd) = minf.sample_table().sample_group_description() {
        sample_boxes.push(sgpd.clone().to_mp4box());
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

    let movie_timescale = movie.movie_header().timescale;

    TrackBox {
        movie_timescale,
        boxes,
    }
}

// Some values are constant for the entire trackfragment, or even the
// entire track. For now this is pretty coarse and we mostly check
// the whole track for defaults.
struct SampleDefaults {
    sample_duration: Option<u32>,
    sample_flags: Option<SampleFlags>,
    sample_size: Option<u32>,
    sample_composition_time_offset: Option<i32>,
}

impl SampleDefaults {
    //
    // Analyze the tables in the SampleTableBox and see if there are any
    // defaults for the samples in this track.
    //
    fn new(track: &TrackBox, from: u32, to: u32) -> SampleDefaults {
        let tables = track.media().media_info().sample_table();

        // If the TimeToSample box has just one entry, it covers all
        // samples so that one entry is the default.
        let entries = &tables.time_to_sample().entries;
        let sample_duration = if entries.len() == 1 {
            Some(entries[0].delta)
        } else {
            None
        };

        // If 'size' in the SampleSize box > 0, then all entries
        // have the same size, so use that as the default.
        // samples so that one entry is the default.
        let size = tables.sample_size().size;
        let sample_size = if size > 0 { Some(size) } else { None };

        // We have a SyncSampleBox. Skip the first sample. Then if the rest is all
        // sync or all non-sync, use that as the default.
        let sample_flags;
        if let Some(sync_table) = tables.sync_samples() {
            let mut is_sync = 0_u32;
            if to >= 1 && to != u32::MAX {
                for entry in &sync_table.entries {
                    if *entry > from && *entry <= to {
                        is_sync += 1;
                    }
                    if *entry > to {
                        break;
                    }
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
            sample_size,
            sample_composition_time_offset,
        }
    }
}

// SampleFlags has a lot of bits, but really all we know is 'is this a key frame'.
// So ttransform that boolean into a 'SampleFlags'.
fn build_sample_flags(is_sync: bool) -> SampleFlags {
    let mut flags = SampleFlags::default();
    if is_sync {
        flags.sample_depends_on = 2;
    } else {
        flags.sample_is_non_sync_sample = true;
    }
    flags
}

// Helper.
fn default_or<A, B>(dfl: &Option<A>, val: B) -> Option<B> {
    if dfl.is_some() {
        None
    } else {
        Some(val)
    }
}

/// Generate a MovieFragmentBox + MediaDataBox for a range of samples from one or more tracks.
///
/// Note that from_sample and to_sample for different tracks need to have
/// the same composition time.
pub fn movie_fragment(mp4: &MP4, seq_num: u32, source: &[FragmentSource]) -> io::Result<Vec<MP4Box>> {
    let movie = mp4.movie();
    let mut mdat = MediaDataBox::default();

    // Create moof and push movie fragment header.
    let mut moof = MovieFragmentBox::default();
    moof.boxes.push(
        MovieFragmentHeaderBox {
            sequence_number: seq_num,
        }
        .to_mp4box(),
    );

    // Track fragments.
    for src in source {
        let track = movie.track_by_id(src.src_track_id).ok_or(ioerr!(
            NotFound,
            "{}: no such track",
            src.src_track_id
        ))?;
        let traf = track_fragment(
            track,
            src.from_sample,
            src.to_sample,
            src.dst_track_id,
            mp4.data_ref.clone(),
            &mut mdat,
        )?;
        moof.boxes.push(traf.to_mp4box());
    }

    // Now that the moof is done, edit the data_offset field in all the trun boxes.
    let mut cb = CountBytes::new();
    moof.to_bytes(&mut cb).unwrap();
    let moof_sz = cb.size();
    if moof_sz > i32::MAX as u64 {
        return Err(ioerr!(InvalidData, "MovieFragmentBox too large: {}", moof_sz));
    }
    let moof_sz = moof_sz as i32;

    for traf in iter_box_mut!(moof, TrackFragmentBox) {
        for trun in iter_box_mut!(traf, TrackRunBox) {
            trun.data_offset.as_mut().map(|d| *d += moof_sz);
        }
    }

    // moof + mdat.
    let mut boxes = Vec::new();
    boxes.push(moof.to_mp4box());
    boxes.push(mdat.to_mp4box());

    Ok(boxes)
}

#[cfg(not(target_os = "macos"))]
fn readahead(file: &fs::File, offset: u64, len: u64) {
    use std::os::unix::io::AsRawFd;
    unsafe {
        libc::posix_fadvise(
            file.as_raw_fd(),
            offset as libc::off_t,
            len as libc::off_t,
            libc::POSIX_FADV_WILLNEED,
        );
    }
}

#[cfg(target_os = "macos")]
fn readahead(file: &fs::File, offset: u64, len: u64) {
    use std::os::unix::io::AsRawFd;
    if offset < i64::MAX as u64 && len < i32::MAX as u64 {
        let ra = libc::radvisory {
            ra_offset: offset as i64,
            ra_count: len as i32,
        };
        unsafe {
            libc::fcntl(file.as_raw_fd(), libc::F_RDADVISE, &ra);
        }
    }
}

// Build a TrackFragmentBox.
fn track_fragment(
    track: &TrackBox,
    from: u32,
    to: u32,
    new_track_id: u32,
    data_ref: DataRef,
    mdat: &mut MediaDataBox,
) -> io::Result<TrackFragmentBox> {
    // Seek to 'from' and peek at the first sample.
    let mut samples = track.sample_info_iter();
    samples.seek(from)?;
    let sample_count = to - from + 1;
    let samples: Vec<_> = samples.take(sample_count as usize).collect();
    if samples.len() == 0 {
        return Err(ioerr!(UnexpectedEof));
    }
    let first_sample = samples[0].clone();

    // Readahead.
    let end = samples[samples.len() - 1].fpos;
    readahead(data_ref.file.as_ref(), samples[0].fpos, end + 1_000_000);

    // Track fragment.
    let mut traf = TrackFragmentBox::default();

    // Track fragment header.
    let mut tfhd = TrackFragmentHeaderBox::default();
    tfhd.track_id = new_track_id;
    tfhd.default_base_is_moof = true;
    // Set sample defaults.
    let dfl = SampleDefaults::new(track, from, to);
    tfhd.sample_description_index = Some(1);
    tfhd.default_sample_duration = dfl.sample_duration;
    tfhd.default_sample_size = dfl.sample_size;
    tfhd.default_sample_flags = dfl.sample_flags.clone();

    traf.boxes.push(tfhd.to_mp4box());

    // SampleToGroupBox.
    if let Some(track_sbgp) = track.media().media_info().sample_table().sample_to_group() {
        let sbgp = track_sbgp.clone_range(from, to);
        if sbgp.entries.len() > 0 {
            traf.boxes.push(sbgp.to_mp4box());
        }
    }

    // Decode time.
    let tfdt = TrackFragmentBaseMediaDecodeTimeBox {
        base_media_decode_time: VersionSizedUint(first_sample.decode_time),
    };
    traf.boxes.push(tfdt.to_mp4box());

    // Track run box.
    //
    // XXX TODO: if we have multiple sync frames in this range,
    //           split them up over multiple trun boxes.
    //
    let flags = build_sample_flags(first_sample.is_sync);
    let first_sample_flags = if dfl.sample_flags.as_ref() != Some(&flags) {
        Some(flags)
    } else {
        None
    };
    let mut trun = TrackRunBox {
        data_offset: Some((mdat.data.len() as u32 + 8).try_into().unwrap()),
        first_sample_flags,
        entries: ArrayUnsized::<TrackRunEntry>::new(),
    };

    for sample in &samples {
        // Add entry info
        let entry = TrackRunEntry {
            sample_duration: default_or(&dfl.sample_duration, sample.duration),
            sample_flags: default_or(&dfl.sample_flags, build_sample_flags(sample.is_sync)),
            sample_size: default_or(&dfl.sample_size, sample.size),
            sample_composition_time_offset: default_or(
                &dfl.sample_composition_time_offset,
                sample.composition_delta,
            ),
        };
        trun.entries.push(entry);

        // Add entry mediadata.
        let start = sample.fpos as usize;
        let end = start + sample.size as usize;
        let oldlen = mdat.data.len() as usize;
        let newlen = oldlen + (end - start);
        mdat.data.resize(newlen);
        data_ref.read_exact_at(&mut mdat.data.bytes_mut()[oldlen..newlen], sample.fpos)?;
    }

    traf.boxes.push(trun.to_mp4box());

    Ok(traf)
}
