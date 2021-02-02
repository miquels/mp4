//
// ISO/IEC 14496-12:2015(E)
// 12.6 Subtitle Media.
//

use std::io;

use crate::boxes::prelude::*;

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
    version => [], 
    impls => [ basebox, boxinfo, debug, fromtobytes ],
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
    version => [], 
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    /// 5.16. Text Sample Entry (ETSI TS 126 245 V10.0.0)
    Tx3gTextSampleEntry {
        skip:                   6,
        data_reference_index:   u16,
        display_flags:          u32,
        horizontal_justification: u8,
        vertical_justification:   u8,
        background_color_rgba:  u32,
        default_text_box:       Tx3gBoxRecord,
        default_style:          Tx3gStyleRecord,
        fonts:                  [Tx3gFontTableBox, unsized],
    },
    fourcc => "tx3g",
    version => [], 
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_struct! {
    /// 5.16. Box Record (ETSI TS 126 245 V10.0.0)
    Tx3gBoxRecord,
        top:    u16,
        left:   u16,
        bottom: u16,
        right:  u16,
}

def_struct! {
    /// 5.15. Style Record (ETSI TS 126 245 V10.0.0)
    Tx3gStyleRecord,
        start_char_offset:  u16,
        end_char_offset:    u16,
        font_id:            u16,
        face_style_flags:   u8,
        font_size:          u8,
        text_color_rgba:    u32,
}

def_box! {
    /// 5.16. Font Table Box (ETSI TS 126 245 V10.0.0)
    Tx3gFontTableBox {
        fonts:  [Tx3gFontRecord, sized16],
    },
    fourcc => "ftab",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_struct! {
    /// 5.16. Font Record (ETSI TS 126 245 V10.0.0)
    Tx3gFontRecord,
        font_id:    u16,
        font_name:  PString,
}

def_struct! {
    /// 5.17. TextSample (ETSI TS 126 245 V10.0.0)
    Tx3GTextSample,
        text:   P16String,
        // modifier boxes, the Text*Box boxes below.
        boxes:  [MP4Box],
}

def_box! {
    /// 5.17.1.1 Text Style (ETSI TS 126 245 V10.0.0)
    Tx3gTextStyleBox {
        entries:    [Tx3gStyleRecord, sized16],
    },
    fourcc => "styl",
    version => [],
    impls => [basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    /// 5.17.1.2 Highlight (ETSI TS 126 245 V10.0.0)
    Tx3gTextHighlightBox {
        startchar_offset:   u16,
        endchar_offset:     u16,
    },
    fourcc => "hlit",
    version => [],
    impls => [basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    /// 5.17.1.2 Text Highlight Color (ETSI TS 126 245 V10.0.0)
    Tx3gTextHighlightColorBox {
        skip:   4,
        highlight_color_rgba:   u32,
    },
    fourcc => "hclr",
    version => [],
    impls => [basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    /// 5.17.1.3 Dynamic Highlight (ETSI TS 126 245 V10.0.0)
    Tx3gTextKaraokeBox {
        highlight_start_time:   u32,
        entries:    [Tx3gTextKaraokeEntry, sized16],
    },
    fourcc => "krok",
    version => [],
    impls => [basebox, boxinfo, debug, fromtobytes ],
}

def_struct! {
    Tx3gTextKaraokeEntry,
        highlight_end_time:     u32,
        start_char_offset:      u16,
        end_char_offset:        u16,
}

def_box! {
    /// 5.17.1.4 Scroll Delay (ETSI TS 126 245 V10.0.0)
    Tx3gTextScrollDelayBox {
        scroll_delay:   u32,
    },
    fourcc => "dlay",
    version => [],
    impls => [basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    /// 5.17.1.5 Scroll Delay (ETSI TS 126 245 V10.0.0)
    Tx3gTextHyperTextBox {
        start_char_offset:  u16,
        end_char_offset:    u16,
        url:                PString,
        alt_string:         PString,
    },
    fourcc => "href",
    version => [],
    impls => [basebox, boxinfo, debug, fromtobytes ],
}

