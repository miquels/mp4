use std::io;
use crate::boxes::prelude::*;

def_box! {
    FileTypeBox {
        major_brand:        FourCC,
        minor_version:      u32,
        compatible_brands:  [FourCC],
    },
    fourcc => "ftyp",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    InitialObjectDescriptionBox {
        audio_profile:  u8,
        video_profile:  u8,
    },
    fourcc => "iods",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    /// 8.3.2 Track Header Box
    ///
    /// Don't forget to set volume to default 0x100 when creating this box.
    TrackHeaderBox {
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
    },
    fourcc => "tkhd",
    version => [1, flags, cr_time, mod_time, duration],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    EditBox {
        boxes:  [EditListBox],
    },
    fourcc => "edts",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    EditListBox {
        entries:    [EditListEntry, sized],
    },
    fourcc => "elst",
    version => [1, entries ],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    BaseMediaInformationHeaderBox {
        boxes:      [MP4Box],
    },
    fourcc => "gmhd",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    DataInformationBox {
        boxes:      [MP4Box],
    },
    fourcc => "dinf",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    // XXX TODO something with version inheritance.
    DataReferenceBox {
        flags:          DataEntryFlags,
        entries:        [MP4Box, sized],
    },
    fourcc => "dref",
    version => [0, flags],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    DataEntryUrlBox {
        flags:          DataEntryFlags,
        location:       ZString,
    },
    fourcc => "url ",
    version => [0, flags],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    DataEntryUrnBox {
        flags:          DataEntryFlags,
        name:           ZString,
        location:       ZString,
    },
    fourcc => "urn ",
    version => [0, flags],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    VideoMediaInformationBox {
        flags:          VideoMediaHeaderFlags,
        graphics_mode:  u16,
        opcolor:        OpColor,
    },
    fourcc => "vmhd",
    version => [0, flags],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    SoundMediaHeaderBox {
        balance:        u16,
        skip:           2,
    },
    fourcc => "smhd",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}
def_box! {
    NullMediaHeaderBox {
    },
    fourcc => "nmhd",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    UserDataBox {
        boxes:      [MP4Box],
    },
    fourcc => "udta",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    TrackSelectionBox {
        switch_group:   u32,
        attribute_list: [FourCC],
    },
    fourcc => "tsel",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    MediaHeaderBox {
        cr_time:    Time,
        mod_time:   Time,
        timescale:  u32,
        duration:   Duration_,
        language:   IsoLanguageCode,
        quality:    u16,
    },
    fourcc => "mdhd",
    version => [1, cr_time, mod_time, duration],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    MovieHeaderBox {
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
    },
    fourcc => "mvhd",
    version => [1, cr_time, mod_time, duration],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    HandlerBox {
        skip:       4,
        handler_type:   FourCC,
        skip:       12,
        name:       ZString,
    },
    fourcc => "hdlr",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    ExtendedLanguageBox {
        language:   ZString,
    },
    fourcc => "elng",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    MetaBox {
        boxes:  [MP4Box],
    },
    fourcc => "meta",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    NameBox {
        name:       ZString,
    },
    fourcc => "name",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    PixelAspectRatioBox {
        h_spacing:  u32,
        v_spacing:  u32,
    },
    fourcc => "pasp",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    CleanApertureBox {
        clean_aperture_width_n: u32,
        clean_aperture_width_d: u32,
        clean_aperture_height_n: u32,
        clean_aperture_height_d: u32,
        horiz_off_n: u32,
        horiz_off_d: u32,
        vert_off_n: u32,
        vert_off_d: u32,
    },
    fourcc => "clap",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    TimeToSampleBox {
        entries:        [TimeToSampleEntry, sized],
    },
    fourcc => "stts",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    SyncSampleBox {
        entries:        [u32, sized],
    },
    fourcc => "stss",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    CompositionOffsetBox {
        entries:        [CompositionOffsetEntry, sized],
    },
    fourcc => "ctts",
    version => [1, entries],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    SampleToChunkBox {
        entries:        [SampleToChunkEntry, sized],
    },
    fourcc => "stsc",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    SubtitleMediaHeaderBox {
    },
    fourcc => "sthd",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    MovieExtendsBox {
        boxes:      [MP4Box],
    },
    fourcc => "mvex",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    TrackExtendsBox {
        track_id:       u32,
        default_sample_description_index:   u32,
        default_sample_duration:    u32,
        default_sample_size:        u32,
        default_sample_flags:       SampleFlags,
    },
    fourcc => "trex",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    SegmentTypeBox {
        major_brand:        FourCC,
        minor_version:      u32,
        compatible_brands:  [FourCC],
    },
    fourcc => "styp",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    MovieFragmentBox {
        boxes:      [MP4Box],
    },
    fourcc => "moof",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    MovieExtendsHeaderBox {
        fragment_duration:  VersionSizedUint,
    },
    fourcc => "mehd",
    version => [0, fragment_duration],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    MovieFragmentHeaderBox {
        sequence_number:    u32,
    },
    fourcc => "mfhd",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    TrackFragmentBox {
        boxes:      [MP4Box],
    },
    fourcc => "traf",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    TrackFragmentBaseMediaDecodeTimeBox {
        base_media_decode_time: VersionSizedUint,
    },
    fourcc => "tfdt",
    version => [1, base_media_decode_time],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}
