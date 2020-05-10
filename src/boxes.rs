use std::fmt::Debug;
use std::io;

use crate::mp4box::*;
use crate::serialize::{FromBytes, ReadBytes, ToBytes, WriteBytes};
use crate::types::*;

def_boxes! {
    MP4Box,

    FileTypeBox, b"ftyp", [] => {
        major_brand:        FourCC,
        minor_version:      u32,
        compatible_brands:  [FourCC],
    };

    InitialObjectDescriptionBox, b"iods", [0] => {
        audio_profile:  u8,
        video_profile:  u8,
    };

    MovieBox, b"moov", [] => {
        sub_boxes:      [MP4Box],
    };

    TrackBox, b"trak", [] => {
        sub_boxes:      [MP4Box],
    };

    // Don't forget to set volume to default 0x100 when creating this box.
    TrackHeaderBox, b"tkhd", [1, flags, cr_time, mod_time, duration] => {
        flags:      TrackFlags,
        cr_time:    Time,
        mod_time:   Time,
        track_id:   u32,
        skip:       4,
        duration:   Duration_,
        skip:       8,
        layer:      u16,
        alt_group:  u16,
        volume:     FixedFloat8_8,
        skip :      2,
        matrix:     Matrix,
        width:      FixedFloat16_16,
        height:     FixedFloat16_16,
    };

    EditBox, b"edts", [] => {
        sub_boxes:  [EditListBox],
    };

    EditListBox, b"elst", [1, entries] => {
        entries:    [EditListEntry, sized],
    };

    MediaBox, b"mdia", [] => {
        sub_boxes:      [MP4Box, unsized],
    };

    SampleTableBox, b"stbl", [] => {
        sub_boxes:      [MP4Box],
    };

    BaseMediaInformationHeaderBox, b"gmhd", [] => {
        sub_boxes:      [MP4Box],
    };

    DataInformationBox, b"dinf", [] => {
        sub_boxes:      [MP4Box],
    };

    // XXX TODO something with version inheritance.
    DataReferenceBox, b"dref", [0, flags] => {
        flags:          DataEntryFlags,
        entries:        [MP4Box, sized],
    };

    DataEntryUrlBox, b"url ", [0, flags] => {
        flags:          DataEntryFlags,
        location:       ZString,
    };

    DataEntryUrnBox, b"urn ", [0, flags] => {
        flags:          DataEntryFlags,
        name:           ZString,
        location:       ZString,
    };

    MediaInformationBox, b"minf", [] => {
        sub_boxes:      [MP4Box],
    };

    VideoMediaInformationBox, b"vmhd", [0, flags] => {
        flags:          VideoMediaHeaderFlags,
        graphics_mode:  u16,
        opcolor:        OpColor,
    };

    SoundMediaHeaderBox, b"smhd", [0] => {
        balance:        u16,
        skip:           2,
    };

    NullMediaHeaderBox, b"nmhd", [0] => {
    };

    UserDataBox, b"udta", [] => {
        sub_boxes:      [MP4Box],
    };

    TrackSelectionBox, b"tsel", [0] => {
        switch_group:   u32,
        attribute_list: [FourCC],
    };

    MediaHeaderBox, b"mdhd", [1, cr_time, mod_time, duration] => {
        cr_time:    Time,
        mod_time:   Time,
        timescale:  u32,
        duration:   Duration_,
        language:   IsoLanguageCode,
        quality:    u16,
    };

    MovieHeaderBox, b"mvhd", [1, cr_time, mod_time, duration] => {
        cr_time:    Time,
        mod_time:   Time,
        timescale:  u32,
        duration:   Duration_,
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

    HandlerBox, b"hdlr", [0] => {
        skip:       4,
        handler_type:   FourCC,
        skip:       12,
        name:       ZString,
    };

    MetaBox, b"meta", [0] => {
        sub_boxes:  [MP4Box],
    };

    NameBox, b"name", [] => {
        name:       ZString,
    };

    TimeToSampleBox, b"stts", [0] => {
        entries:        [TimeToSampleEntry, sized],
    };

    SyncSampleBox, b"stss", [0] => {
        entries:        [u32, sized],
    };

    CompositionOffsetBox, b"ctts", [1, entries] => {
        entries:        [CompositionOffsetEntry, sized],
    };

    SampleToChunkBox, b"stsc", [0] => {
        entries:        [SampleToChunkEntry, sized],
    };

    ChunkOffsetBox, b"stco", [0] => {
        entries:        [u32, sized],
    };

    ChunkLargeOffsetBox, b"co64", [0] => {
        entries:        [u64, sized],
    };

    SubtitleMediaHeaderBox, b"sthd", [0] => {
    };

    MovieExtendsBox, b"mvex", [] => {
        sub_boxes:      [MP4Box],
    };

    TrackExtendsBox, b"trex", [0] => {
        track_id:       u32,
        default_sample_description_index:   u32,
        default_sample_duration:    u32,
        default_sample_size:        u32,
        default_sample_flags:       SampleFlags,
    };

    SegmentTypeBox, b"styp", [] => {
        major_brand:        FourCC,
        minor_version:      u32,
        compatible_brands:  [FourCC],
    };

    MovieFragmentBox, b"moof", [] => {
        sub_boxes:      [MP4Box],
    };

    MovieExtendsHeaderBox, b"mehd", [0, fragment_duration] => {
        fragment_duration:  VersionSizedUint,
    };

    MovieFragmentHeaderBox, b"mfhd", [0] => {
        sequence_number:    u32,
    };

    TrackFragmentBox, b"traf", [] => {
        sub_boxes:      [MP4Box],
    };

    TrackFragmentBaseMediaDecodeTimeBox, b"tfdt", [1, base_media_decode_time] => {
        base_media_decode_time: VersionSizedUint,
    };

    // Below are boxes that are defined manually in boxes/ *.rs

    Free, b"free", [] => free;
    Skip, b"skip", [];
    Wide, b"wide", [];

    MdatBox, b"mdat", [] => mdat;

    // Max version 0, since we do not support AudioSampleEntryV1 right now.
    SampleDescriptionBox, b"stsd", [0] => stsd;

    AvcSampleEntry, b"avc1", [] => avc1;
        AvcConfigurationBox, b"avcC", [];
    AacSampleEntry, b"mp4a", [] => mp4a;
        ESDescriptorBox, b"esds", [0];
    Ac3SampleEntry, b"ac-3", [] => ac_3;
        AC3SpecificBox, b"dac3", [];

    SampleSizeBox, b"stsz", [0] => stsz;
    CompactSampleSizeBox, b"stz2", [0] => stz2;

    SampleToGroupBox, b"sbgp", [1] => sbgp;
    SampleGroupDescriptionBox, b"sgpd", [2] => sgpd;

    SegmentIndexBox, b"sidx", [1, earliest_presentation_time, first_offset] => sidx;

    TrackFragmentHeaderBox, b"tfhd", [1] => tfhd;
    TrackRunBox, b"trun", [1] => trun;

    AppleItemListBox, b"ilst", [] => ilst;
}

def_boxes! {
    AppleItem,

    IAlbumBox, b"\xa9alb", [] => { boxes: [AppleItem], };
    IArtistBox, b"\xa9art", [] => { boxes: [AppleItem], };
    ICommentBox, b"\xa9cmt", [] => { boxes: [AppleItem], };
    IComposerBox, b"\xa9com", [] => { boxes: [AppleItem], };
    IEncoderBox, b"\xa9too", [] => { boxes: [AppleItem], };
    IGenBox, b"\xa9gen", [] => { boxes: [AppleItem], };
    IGroupingBox, b"\xa9grp", [] => { boxes: [AppleItem], };
    ILyricsBox, b"\xa9lyr", [] => { boxes: [AppleItem], };
    IMovementIndexBox, b"\xa9mvi", [] => { boxes: [AppleItem], };
    IMovementNameBox, b"\xa9mvm", [] => { boxes: [AppleItem], };
    IMovementTotalBox, b"\xa9mvc", [] => { boxes: [AppleItem], };
    ITitleBox, b"\xa9nam", [] => { boxes: [AppleItem], };
    ITrackBox, b"\xa9trk", [] => { boxes: [AppleItem], };
    IWrittenByBox, b"\xa9wrt", [] => { boxes: [AppleItem], };
    IYearBox, b"\xa9day", [] => { boxes: [AppleItem], };

    ICompilationBox, b"cpil", [] => { boxes: [AppleItem] ,};
    ICoverArtBox, b"covr", [] => { boxes: [AppleItem], };
    IDescription, b"desc", [] => { boxes: [AppleItem], };
    IGenreBox, b"gnre", [] => { boxes: [AppleItem], };
    IHDVideoFlag, b"hdvd", [] => { boxes: [AppleItem], };
    IICDSetNoBox, b"disk", [] => { boxes: [AppleItem], };
    IItunesBox, b"----", [] => { boxes: [AppleItem], };
    ILongDescription, b"ldes", [] => { boxes: [AppleItem], };
    IMediaType, b"stik", [] => { boxes: [AppleItem], };
    ITempoBox, b"tmpo", [] => { boxes: [AppleItem], };
    ITrackNoBox, b"trkn", [] => { boxes: [AppleItem], };

    IDataBox, b"data", [0u8];
    INameBox, b"name", [0] => { name: ZString, };
    IMeanBox, b"mean", [0] => { mean: ZString, };
}
