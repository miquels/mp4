use std::fmt::Debug;
use std::io;

use crate::fromtobytes::{FromBytes, ToBytes, ReadBytes, WriteBytes};
use crate::types::*;
use crate::mp4box::*;

def_boxes! {
    FileType, "ftyp", 8 => {
        major_brand:        FourCC,
        minor_version:      u32,
        compatible_brands:  [FourCC],
    };

    InitialObjectDescription, "iods", 8 => {
        version:        Version,
        flags:          Flags,
        audio_profile:  u8,
        video_profile:  u8,
    };

    MovieBox, "moov", 8 => {
        sub_boxes:      [MP4Box],
    };

    MovieFragmentBox, "moof", 8 => {
        sub_boxes:      [MP4Box],
    };

    TrackBox, "trak", 8 => {
        sub_boxes:      [MP4Box],
    };

    TrackHeader, "tkhd", 8 => {
        version:    Version,
        flags:      TrackFlags,
        cr_time:    Time,
        mod_time:   Time,
        track_id:   u32,
        skip:       4,
        duration:   u32,
        skip:       8,
        layer:      u16,
        alt_group:  u16,
        volume:     FixedFloat8_8,
        skip :      2,
        matrix:     Matrix,
        width:      FixedFloat16_16,
        height:     FixedFloat16_16,
    };

    Edits, "edts", 8 => {
        sub_boxes:  [IsoBox<EditList>],
    };

    EditList, "elst", 8 => {
        version:                Version,
        flags:                  Flags,
        entry_count:            u32,
        entries:                [EditListEntry, entry_count],
    };

    MediaBox, "mdia", 8 => {
        sub_boxes:      [MP4Box],
    };

    SampleTableBox, "stbl", 8 => {
        sub_boxes:      [MP4Box],
    };

    BaseMediaInformationHeader, "gmhd", 8 => {
        sub_boxes:      [MP4Box],
    };

    DataInformationBox, "dinf", 8 => {
        sub_boxes:      [MP4Box],
    };

    DataReference, "dref", 8 => {
        version:        Version,
        flags:          Flags,
        entry_count:    u32,
        entries:        [MP4Box, entry_count],
    };

    DataEntryUrl, "url ", 8 => {
        version:        Version,
        flags:          Flags,
        location:       ZString,
    };

    DataEntryUrn, "urn ", 8 => {
        version:        Version,
        flags:          Flags,
        name:           ZString,
        location:       ZString,
    };

    MediaInformationBox, "minf", 8 => {
        sub_boxes:      [MP4Box],
    };

    VideoMediaInformation, "vmhd", 8 => {
        version:        Version,
        flags:          Flags,
        graphics_mode:  u16,
        opcolor:        OpColor,
    };

    SoundMediaHeader, "smhd", 8 => {
        version:        Version,
        flags:          Flags,
        balance:        u16,
        skip:           2,
    };

    NullMediaHeader, "nmhd", 8 => {
    };

    UserDataBox, "udta", 8 => {
        sub_boxes:      [MP4Box],
    };

    TrackSelection, "tsel", 8 => {
        version:        Version,
        flags:          Flags,
        switch_group:   u32,
        attribute_list: [FourCC],
    };

    SampleDescription, "stsd", 8 => {
        version:    Version,
        flags:      Flags,
        entries:    u32,
        n1_size:    u32,
        n1_format:  FourCC,
        skip:       6,
        dataref_idx:    u16,
    };

    MediaHeader, "mdhd", 8 => {
        version:    Version,
        flags:      Flags,
        cr_time:    Time,
        mod_time:   Time,
        time_scale: u32,
        duration:   u32,
        language:   IsoLanguageCode,
        quality:    u16,
    };

    MovieHeader, "mvhd", 8 => {
        version:    Version,
        flags:      Flags,
        cr_time:    Time,
        mod_time:   Time,
        timescale:  u32,
        duration:   u32,
        pref_rate:  u32,
        pref_vol:   u16,
        skip:       10,
        matrix:     Matrix,
        preview_time:   u32,
        preview_duration:   u32,
        poster_time:    u32,
        selection_time: u32,
        selection_duration: u32,
        current_time:   u32,
        next_track_id: u32,
    };

    Handler, "hdlr", 8 => {
        version:    Version,
        flags:      Flags,
        skip:       4,
        handler_type:   FourCC,
        skip:       12,
        name:       ZString,
    };

    MetaData, "meta", 8 => {
        version:    Version,
        flags:      Flags,
        sub_boxes:  [MP4Box],
    };

    Name, "name", 8 => {
        name:       ZString,
    };

    AppleItemList, "ilst", 8 => {
        list:       [AppleItem],
    };

    // Below are boxes that are defined manually in boxes/*.rs

    Free, "free", 8 => free;
    Skip, "skip", 8;
    Wide, "wide", 8;
}
