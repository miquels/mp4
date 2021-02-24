use std::io;

use crate::boxes::prelude::*;

def_box! {
    /// 8.1.3. Progressive Download Information Box (ISO/IEC 14496-12:2015(E))
    ///
    /// Don't forget to set volume to default 0x100 when creating this box.
    ProgressiveDownloadInfoBox {
        entries:    Vec<ProgressiveDownloadInfoEntry>,
    },
    fourcc => "pdin",
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_struct! {
    ProgressiveDownloadInfoEntry,
        rate:   u32,
        initial_delay:  u32,
}
