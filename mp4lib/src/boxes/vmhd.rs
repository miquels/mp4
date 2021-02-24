use std::io;

use crate::boxes::prelude::*;

def_box! {
    VideoMediaHeaderBox {
        flags:          VideoMediaHeaderFlags,
        graphics_mode:  u16,
        opcolor:        OpColor,
    },
    fourcc => "vmhd",
    version => [0, flags],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

impl_flags!(
    /// Always 0x01.
    VideoMediaHeaderFlags,
    debug
);

impl Default for VideoMediaHeaderFlags {
    fn default() -> Self {
        Self(0x01)
    }
}

def_struct! {
    /// OpColor
    OpColor,
        red:    u16,
        green:  u16,
        blue:   u16,
}

