use std::io;

use crate::boxes::prelude::*;
use crate::boxes::{MediaHeaderBox, HandlerBox, MediaInformationBox, ExtendedLanguageBox};

def_box! {
    /// 8.4.1 Media Box (ISO/IEC 14496-12:2015(E))
    MediaBox {
        boxes:      Vec<MP4Box>,
    },
    fourcc => "mdia",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

impl MediaBox {

    /// Get a reference to the MediaHeaderBox.
    pub fn media_header(&self) -> &MediaHeaderBox {
        first_box!(&self.boxes, MediaHeaderBox).unwrap()
    }

    /// Get a reference to the HandlerBox.
    pub fn handler(&self) -> &HandlerBox {
        first_box!(&self.boxes, HandlerBox).unwrap()
    }

    /// Get a reference to the MediaInformationBox.
    pub fn media_info(&self) -> &MediaInformationBox {
        first_box!(&self.boxes, MediaInformationBox).unwrap()
    }

    /// Get a mutable to the MediaInformationBox.
    pub fn media_info_mut(&mut self) -> &mut MediaInformationBox {
        first_box_mut!(&mut self.boxes, MediaInformationBox).unwrap()
    }

    /// Get an optional reference to the ExtendedLanguageBox.
    pub fn extended_language(&self) -> Option<&ExtendedLanguageBox> {
        first_box!(&self.boxes, ExtendedLanguageBox)
    }

    /// Check if this track is valid (has header, handler, and mediainfo boxes).
    pub fn is_valid(&self) -> bool {
        let mut valid = true;
        if first_box!(&self.boxes, MediaHeaderBox).is_none() {
            log::error!("MediaBox: no MediaHeaderBox present");
            valid = false;
        }
        if first_box!(&self.boxes, HandlerBox).is_none() {
            log::error!("MediaBox: no HandlerBox present");
            valid = false;
        }
        match first_box!(&self.boxes, MediaInformationBox) {
            Some(mi) => {
                if !mi.is_valid() {
                    valid = false;
                }
            },
            None => {
                log::error!("MediaBox: no MediaInformationBox present");
                valid = false;
            }
        }

        valid
    }
}

