use std::io;

use crate::boxes::prelude::*;

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

impl_flags!(
    /// Track: enabled/in_movie/preview
    TrackFlags
);

impl TrackFlags {
    pub fn get_enabled(&self) -> bool {
        self.get(0)
    }
    pub fn set_enabled(&mut self, on: bool) {
        self.set(0, on)
    }
    pub fn get_in_movie(&self) -> bool {
        self.get(1)
    }
    pub fn set_in_movie(&mut self, on: bool) {
        self.set(1, on)
    }
    pub fn get_in_preview(&self) -> bool {
        self.get(2)
    }
    pub fn set_in_preview(&mut self, on: bool) {
        self.set(2, on)
    }
    pub fn get_in_poster(&self) -> bool {
        self.get(3)
    }
    pub fn set_in_poster(&mut self, on: bool) {
        self.set(3, on)
    }
}

impl std::fmt::Debug for TrackFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut v = vec!["["];
        if self.get_enabled() {
            v.push("enabled");
        }
        if self.get_in_movie() {
            v.push("in_movie");
        }
        if self.get_in_preview() {
            v.push("in_preview");
        }
        if self.get_in_poster() {
            v.push("in_poster");
        }
        v.push("]");
        write!(f, "TrackFlags({})", v.join(" "))
    }
}

