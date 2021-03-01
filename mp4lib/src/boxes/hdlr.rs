use std::io;

use crate::boxes::prelude::*;

def_box! {
    /// 8.4.3 Handler Reference Box (ISO/IEC 14496-12:2015(E))
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

impl HandlerBox {
    /// Is this a subtitle track.
    pub fn is_subtitle(&self) -> bool {
        self.handler_type == b"subt" || self.handler_type == b"sbtl"
    }

    /// Is this a video track.
    pub fn is_video(&self) -> bool {
        self.handler_type == b"vide"
    }

    /// Is this an audio track.
    pub fn is_audio(&self) -> bool {
        self.handler_type == b"soun"
    }
}
