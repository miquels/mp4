//
// ISO/IEC 14496-12:2015(E)
// 8.5.2 Sample Description Box 
//

use std::io;
use crate::boxes::prelude::*;

def_box! {
    /// 8.5.2 Sample Description Box (ISO/IEC 14496-12:2015(E))
    SampleDescriptionBox {
        entries:    ArraySized32<MP4Box>,
    },
    fourcc => "stsd",
    // Max version 0, since we do not support AudioSampleEntryV1 right now.
    version => [0],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

