use crate::boxes::prelude::*;
use std::io;

def_box! {
    FileTypeBox {
        major_brand:        FourCC,
        minor_version:      u32,
        compatible_brands:  Vec<FourCC>,
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
     /// Base Media Information Header Atom (Apple/Quicktime)
    BaseMediaInformationHeaderBox {
        boxes:      Vec<MP4Box>,
    },
    fourcc => "gmhd",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    /// Base Media Info Atom (Apple/Quicktime)
    BaseMediaInformationBox {
        graphics_mode:  u16,
        opcolor:        OpColor,
        balance:        u16,
        skip:           2,
    },
    fourcc => "gmin",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

/*
def_box! {
    /// Text Media Info Atom (Apple/Quicktime)
    TextMediaInformationBox {
        matrix:         Matrix,
    },
    fourcc => "text",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}*/

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
    #[derive(Default)]
    NullMediaHeaderBox {
    },
    fourcc => "nmhd",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    UserDataBox {
        boxes:      Vec<MP4Box>,
    },
    fourcc => "udta",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    TrackSelectionBox {
        switch_group:   u32,
        attribute_list: Vec<FourCC>,
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
    ExtendedLanguageBox {
        language:   ZString,
    },
    fourcc => "elng",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    MetaBox {
        boxes:  Vec<MP4Box>,
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
    #[derive(Default)]
    SubtitleMediaHeaderBox {
    },
    fourcc => "sthd",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    MovieExtendsBox {
        boxes:      Vec<MP4Box>,
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

// Default needs to set sample_description_index to 1.
impl Default for TrackExtendsBox {
    fn default() -> TrackExtendsBox {
        TrackExtendsBox {
            track_id: 0,
            default_sample_description_index: 1,
            default_sample_duration: 0,
            default_sample_size: 0,
            default_sample_flags: SampleFlags::default(),
        }
    }
}

def_box! {
    BtrtBox {
        decoding_buffer_size: u32,
        max_bitrate: u32,
        avg_bitrate: u32,
    },
    fourcc => "btrt",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    SegmentTypeBox {
        major_brand:        FourCC,
        minor_version:      u32,
        compatible_brands:  Vec<FourCC>,
    },
    fourcc => "styp",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    MovieExtendsHeaderBox {
        fragment_duration:  Duration_,
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
    TrackFragmentBaseMediaDecodeTimeBox {
        base_media_decode_time: VersionSizedUint,
    },
    fourcc => "tfdt",
    version => [1, base_media_decode_time],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}
