use std::io;

use crate::boxes::prelude::*;
use crate::boxes::{DataInformationBox, SampleTableBox};

def_box! {
    /// 8.4.4 Media Information Box (ISO/IEC 14496-12:2015(E))
    MediaInformationBox {
        boxes:      [MP4Box],
    },
    fourcc => "minf",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

impl MediaInformationBox {

    /// Get a reference to the DataInformationBox.
    pub fn data_information(&self) -> &DataInformationBox {
        first_box!(&self.boxes, DataInformationBox).unwrap()
    }

    /// Get a reference to the SampleTableBox.
    pub fn sample_table(&self) -> &SampleTableBox {
        first_box!(&self.boxes, SampleTableBox).unwrap()
    }

    /// Get a mutable reference to the SampleTableBox.
    pub fn sample_table_mut(&mut self) -> &mut SampleTableBox {
        first_box_mut!(&mut self.boxes, SampleTableBox).unwrap()
    }

    /// Check if this MediaInformationBox is valid (has data_information and sample_table boxes).
    pub fn is_valid(&self) -> bool {
        let mut valid = true;
        if first_box!(&self.boxes, DataInformationBox).is_none() {
            log::error!("MediaInformationBox: no DataInformationBox present");
            valid = false;
        }
        match first_box!(&self.boxes, SampleTableBox) {
            Some(st) => {
                if !st.is_valid() {
                    valid = false;
                }
            },
            None => {
                log::error!("MediaInformationBox: no SampleTableBox present");
                valid = false;
            }
        }
        valid
    }
}

