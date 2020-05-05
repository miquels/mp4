use std::fmt::Debug;
use std::io;

use crate::serialize::{FromBytes, ReadBytes, ToBytes, WriteBytes};
use crate::mp4box::*;
use crate::types::*;

def_boxes! {
    FileTypeBox, "ftyp", [] => {
        major_brand:        FourCC,
        minor_version:      u32,
        compatible_brands:  [FourCC],
    };

    InitialObjectDescriptionBox, "iods", [0] => {
        audio_profile:  u8,
        video_profile:  u8,
    };

    MovieBox, "moov", [] => {
        sub_boxes:      [MP4Box],
    };

    TrackBox, "trak", [] => {
        sub_boxes:      [MP4Box],
    };

    // Don't forget to set volume to default 0x100 when creating this box.
    TrackHeaderBox, "tkhd", [1, cr_time, mod_time, duration] => {
        flags:      TrackFlags,
        cr_time:    Time,
        mod_time:   Time,
        track_id:   u32,
        skip:       4,
        duration:   VersionSizedUint,
        skip:       8,
        layer:      u16,
        alt_group:  u16,
        volume:     FixedFloat8_8,
        skip :      2,
        matrix:     Matrix,
        width:      FixedFloat16_16,
        height:     FixedFloat16_16,
    };

    EditBox, "edts", [] => {
        sub_boxes:  [EditListBox],
    };

    EditListBox, "elst", [1, entries] => {
        entries:    [EditListEntry, sized],
    };

    MediaBox, "mdia", [] => {
        sub_boxes:      [MP4Box, unsized],
    };

    SampleTableBox, "stbl", [] => {
        sub_boxes:      [MP4Box],
    };

    BaseMediaInformationHeaderBox, "gmhd", [] => {
        sub_boxes:      [MP4Box],
    };

    DataInformationBox, "dinf", [] => {
        sub_boxes:      [MP4Box],
    };

    // XXX TODO something with version inheritance.
    DataReferenceBox, "dref", [0] => {
        entries:        [MP4Box, sized],
    };

    DataEntryUrlBox, "url ", [0] => {
        location:       ZString,
    };

    DataEntryUrnBox, "urn ", [0] => {
        name:           ZString,
        location:       ZString,
    };

    MediaInformationBox, "minf", [] => {
        sub_boxes:      [MP4Box],
    };

    VideoMediaInformationBox, "vmhd", [0] => {
        graphics_mode:  u16,
        opcolor:        OpColor,
    };

    SoundMediaHeaderBox, "smhd", [0] => {
        balance:        u16,
        skip:           2,
    };

    NullMediaHeaderBox, "nmhd", [] => {
    };

    UserDataBox, "udta", [] => {
        sub_boxes:      [MP4Box],
    };

    TrackSelectionBox, "tsel", [0] => {
        switch_group:   u32,
        attribute_list: [FourCC],
    };

    // XXX FIXME 8.5.2.3 Semantics
    // version is set to zero unless the box contains an AudioSampleEntryV1, whereupon version must be 1
    SampleDescriptionBox, "stsd", [0] => {
        entries:    u32,
        n1_size:    u32,
        n1_format:  FourCC,
        skip:       6,
        dataref_idx:    u16,
    };

    MediaHeaderBox, "mdhd", [1, cr_time, mod_time, duration] => {
        cr_time:    Time,
        mod_time:   Time,
        time_scale: u32,
        duration:   VersionSizedUint,
        language:   IsoLanguageCode,
        quality:    u16,
    };

    MovieHeaderBox, "mvhd", [1, cr_time, mod_time, duration] => {
        cr_time:    Time,
        mod_time:   Time,
        timescale:  u32,
        duration:   VersionSizedUint,
        pref_rate:  FixedFloat16_16,
        pref_vol:   FixedFloat8_8,
        skip:       10,
        matrix:     Matrix,
        // The next 6 32-bit values are "pre_defined" in ISO/IEC 14496-12:2015,
        // but they appear to be the following:
        preview_time:   u32,
        preview_duration:   u32,
        poster_time:    u32,
        selection_time: u32,
        selection_duration: u32,
        current_time:   u32,
        //
        next_track_id: u32,
    };

    HandlerBox, "hdlr", [0] => {
        skip:       4,
        handler_type:   FourCC,
        skip:       12,
        name:       ZString,
    };

    MetaBox, "meta", [0] => {
        sub_boxes:  [MP4Box],
    };

    NameBox, "name", [] => {
        name:       ZString,
    };

    AppleItemList, "ilst", [] => {
        list:       [AppleItem],
    };

    TimeToSampleBox, "stts", [0] => {
        entries:        [TimeToSampleEntry, sized],
    };

    SyncSampleBox, "stss", [0] => {
        entries:        [u32, sized],
    };

    CompositionOffsetBox, "ctts", [1, entries] => {
        entries:        [CompositionOffsetEntry, sized],
    };

    SampleToChunkBox, "stsc", [0] => {
        entries:        [SampleToChunkEntry, sized],
    };

    ChunkOffsetBox, "stco", [0] => {
        entries:        [u32, sized],
    };

    ChunkLargeOffsetBox, "co64", [0] => {
        entries:        [u64, sized],
    };

    SubtitleMediaHeaderBox, "sthd", [0] => {
    };

    MovieExtendsBox, "mvex", [] => {
        sub_boxes:      [MP4Box],
    };

    TrackExtendsBox, "trex", [0] => {
        track_id:       u32,
        default_sample_description_index:   u32,
        default_sample_duration:    u32,
        default_sample_size:        u32,
        default_sample_flags:       SampleFlags,
    };

    SegmentTypeBox, "styp", [] => {
        major_brand:        FourCC,
        minor_version:      u32,
        compatible_brands:  [FourCC],
    };

    /*
    // 8.16.3 Segment Index Box (ISO/IEC 14496-12:2015(E))
    SegmentIndexBox, "sidx", [1, earliest_presentation_time, first_offset] => {
        reference_id:               u32,
        timescale:                  u32,
        earliest_presentation_time: VersionSizedUint as u64,
        first_offset:               VersionSizedUint as u64,
        skip:                       2,
        references:                 [SegmentReference, sized16],
    };
    */

    MovieFragmentBox, "moof", [] => {
        sub_boxes:      [MP4Box],
    };

    MovieExtendsHeaderBox, "mehd", [0, fragment_duration] => {
        fragment_duration:  VersionSizedUint,
    };

    MovieFragmentHeaderBox, "mfhd", [0] => {
        sequence_number:    u32,
    };

    TrackFragmentBox, "traf", [] => {
        sub_boxes:      [MP4Box],
    };

    TrackFragmentBaseMediaDecodeTimeBox, "tfdt", [1, base_media_decode_time] => {
        base_media_decode_time: VersionSizedUint,
    };

    // Below are boxes that are defined manually in boxes/ *.rs

    Free, "free", [] => free;
    Skip, "skip", [];
    Wide, "wide", [];

    Mdat, "mdat", [] => mdat;

    SampleSizeBox, "stsz", [0] => stsz;
    CompactSampleSizeBox, "stz2", [0] => stz2;
    SampleToGroupBox, "sbgp", [1] => sbgp;

    SegmentIndexBox, "sidx", [1, earliest_presentation_time, first_offset] => sidx;

    TrackFragmentHeaderBox, "tfhd", [1] => tfhd;
    TrackRunBox, "trun", [1] => trun;
}
