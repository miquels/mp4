use std::io;

use crate::boxes::prelude::*;

def_box! {
    DataInformationBox {
        boxes:      Vec<MP4Box>,
    },
    fourcc => "dinf",
    version => [],
    impls => [ basebox, boxinfo, debug, fromtobytes ],
}

def_box! {
    // XXX TODO something with version inheritance.
    DataReferenceBox {
        flags:          DataEntryFlags,
        entries:        ArraySized32<MP4Box>,
    },
    fourcc => "dref",
    version => [0, flags],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    DataEntryUrlBox {
        flags:          DataEntryFlags,
        location:       ZString,
    },
    fourcc => "url ",
    version => [0, flags],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

def_box! {
    DataEntryUrnBox {
        flags:          DataEntryFlags,
        name:           ZString,
        location:       ZString,
    },
    fourcc => "urn ",
    version => [0, flags],
    impls => [ boxinfo, debug, fromtobytes, fullbox ],
}

impl_flags!(
    /// 0x01 if the data is in the same file (default).
    DataEntryFlags
);

impl DataEntryFlags {
    pub fn get_in_same_file(&self) -> bool {
        self.get(0)
    }
    pub fn set_in_same_file(&mut self, on: bool) {
        self.set(0, on)
    }
}

impl std::fmt::Debug for DataEntryFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut v = vec!["["];
        if self.get_in_same_file() {
            v.push("in_same_file");
        }
        v.push("]");
        write!(f, "DataEntryFlags({})", v.join(" "))
    }
}

impl Default for DataEntryFlags {
    fn default() -> Self {
        Self(0x01)
    }
}

