//
// ISO/IEC 14496-12:2015(E)
// 12.6 Subtitle Media.
//

use std::io;

use crate::boxes::prelude::*;
use crate::track::SubtitleTrackInfo;

def_box! {
    /// 12.6.3.2 XML Subtitle Sample Entry
    XMLSubtitleSampleEntry {
        skip:                   6,
        data_reference_index:   u16,
        namespace:              ZString,
        schema_location:        ZString,
        auxiliary_mime_types:   ZString,
        boxes:                  [MP4Box],
    },
    fourcc => "stpp",
    version => [0], 
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    /// 12.6.3.2 Text Subtitle Sample Entry
    TextSubtitleSampleEntry {
        skip:                   6,
        data_reference_index:   u16,
        content_encoding:       ZString,
        mime_format:            ZString,
        boxes:                  [MP4Box],
    },
    fourcc => "sbtt",
    version => [0], 
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    /// TX3G Subtitle Sample Entry
    Tx3gSubtitleSampleEntry {
        skip:                   6,
        data_reference_index:   u16,
        horizontal_justification: u8,
        vertical_justification:   u8,
        background_color_rgba:  u32,
        default_text_box:       Tx3gBoxRecord,
        default_style:          Tx3gStyleRecord,
        fonts:                  [FontTableBox, unsized],
    },
    fourcc => "tx3g",
    version => [0], 
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_struct! {
    Tx3gBoxRecord,
        top:    u16,
        left:   u16,
        bottom: u16,
        right:  u16,
}

def_struct! {
    Tx3gStyleRecord,
        start_char_offset:  u16,
        end_char_offset:    u16,
        font_id:            u16,
        style_flags:        u8,
        font_size:          u8,
        text_color_rgba:    u32,
}

def_box! {
    FontTableBox {
        fonts:  [Tx3gFontInfo, sized16],
    },
    fourcc => "ftab",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

use super::mp4a::PString;

def_struct! {
    Tx3gFontInfo,
        font_id:    u16,
        font_name:  PString,
}

