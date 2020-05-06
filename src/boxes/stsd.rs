//
// ISO/IEC 14496-12:2015(E)
// 8.5.2 Sample Description Box 
//

use std::io;
use crate::serialize::{FromBytes, ToBytes, ReadBytes, WriteBytes};
use crate::mp4box::{BoxInfo, FullBox};
use crate::boxes::MP4Box;
use crate::types::*;

def_box! {
    /// 8.5.2 Sample Description Box (ISO/IEC 14496-12:2015(E))
    SampleDescriptionBox, "stsd",
        entries:    [MP4Box, sized],
}

// version is set to zero unless the box contains an AudioSampleEntryV1, whereupon version must be 1
impl FullBox for SampleDescriptionBox {
}

