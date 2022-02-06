//
// Several helper macros.
//
//

// List of all boxes, used in boxes.rs.
//
// For each box, include its module. Then build an enum with
// a variant for each box.
macro_rules! def_boxes {

    // main entry point.
    ($($name:ident, $fourcc:expr $(=> $mod:tt)? ; )+) => {

        // include modules.
        $(
            $(
                pub(crate) mod $mod;
                pub use self::$mod::*;
            )?
        )+

        // build enum.
        impl_enum!(MP4Box, $($name, $fourcc),*);
    };

}

// Define one box.
//
// def_box! {
//     TypeName => {
//         member: type,
//         member: type,
//     },
//     fourcc => "fourcc",
//     version => [ 1, deps ],
//     impls => [ fullbox, boxinfo ],
//  }
macro_rules! def_box {

    // impls => [ basebox ]
    (@IMPL basebox $name:ident $($_tt:tt)*) => {
        impl_basebox!($name);
    };

    // impls => [ fullbox ]
    (@IMPL fullbox $name:ident, $_fourcc:expr, $version:tt, $_block:tt) => {
        impl_fullbox!($name, $version);
    };

    // impls => [ boxinfo ]
    (@IMPL boxinfo $name:ident, $fourcc:expr, $version:tt, $_block:tt) => {
        impl_boxinfo!($name, $fourcc, $version);
    };

    // impls => [ debug ]
    (@IMPL debug $name:ident, $_fourcc:expr, $_version:tt, $block:tt) => {
        impl_debug!($name, $block);
    };

    // impls => [ fromtobytes ]
    (@IMPL fromtobytes $name:ident, $_fourcc:expr, $_version:tt, $block:tt) => {
        impl_fromtobytes!($name, $block);
    };

    // expand block and call def_struct!
    (@IMPL def_struct $(#[$outer:meta])* $name:ident, { $($block:tt)* }) => {
        def_struct!(@def_struct $(#[$outer])* $name, $($block)*);
    };

    // Main entry point.
    ($(#[$outer:meta])* $name:ident $block:tt, fourcc => $fourcc:expr,
     version => $version:tt, impls => [ $($impl:ident),* ] $(,)?)  => {

        // Define the struct itself.
        def_box!(@IMPL def_struct $(#[$outer])* #[derive(Clone)] $name, $block);

        // And the impl's we want for it.
        $(
            def_box!(@IMPL $impl $name, $fourcc, $version, $block);
        )*
    };

}

// Implement an empty FullBox trait for this struct.
macro_rules! impl_basebox {
    ($name:ident) => {
        // Not a fullbox - default impl.
        impl FullBox for $name {}
    };
}

// Implement the FullBox trait for this struct.
macro_rules! impl_fullbox {
    ($name:ident, [0]) => {
        // Fullbox always version 0.
        impl FullBox for $name {
            fn version(&self) -> Option<u8> { Some(0) }
        }
    };
    ($name:ident, [$maxver:tt $(,$deps:ident)+ ]) => {
        // Check all the dependencies for the minimum ver.
        // TODO what about conflicting versions for deps?
        impl FullBox for $name {
            fn version(&self) -> Option<u8> {
                let mut v = 0;
                $(
                    if let Some(depver) = self.$deps.version() {
                        if depver > v {
                            v = depver;
                        }
                    }
                )+
                Some(v)
            }
            /// XXX FIXME do not use flags as deps. Should be a separate trait?
            fn flags(&self) -> u32 {
                let mut flags = 0;
                $(
                    flags |= self.$deps.flags();
                )+
                flags
            }
        }
    }
}

// Implement the BoxInfo trait for this struct.
macro_rules! impl_boxinfo {
    ($name:ident, $fourcc:expr, [$($maxver:tt)? $(,$deps:ident)*]) => {
        impl BoxInfo for $name {
            const FOURCC: &'static str = $fourcc;
            #[inline]
            fn fourcc(&self) -> FourCC {
                // XXX FIXME make this b"four" instead of "four"
                //FourCC(u32::from_be_bytes(*$fourcc))
                use std::convert::TryInto;
                FourCC(u32::from_be_bytes($fourcc.as_bytes().try_into().unwrap()))
            }
            $(
                #[inline]
                fn max_version() -> Option<u8> {
                    Some($maxver)
                }
            )?
        }
    };
}

// Implement the Debug trait for this struct.
macro_rules! impl_debug {
    ($name:ident, { $( $field:tt: $type:tt $(<$gen:tt>)? ),* $(,)? }) => {
        // Debug implementation that adds fourcc field.
        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                let mut dbg = f.debug_struct(stringify!($name));
                dbg.field("fourcc", &self.fourcc());
                $(
                    if !stringify!($field).starts_with("_") {
                        def_struct!(@filter_skip $field, dbg.field(stringify!($field), &self.$field););
                    }
                )*
                dbg.finish()
            }
        }
    }
}

// Implement the FromBytes and ToBytes traits for this struct.
macro_rules! impl_fromtobytes {
    ($name:ident, { $( $field:tt: $type:tt $(<$gen:tt>)? ),* $(,)? }) => {
        impl FromBytes for $name {
            #[allow(unused_variables)]
            fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<$name> {

                fn alignment(pos: u64) -> u32 {
                    match pos % 8 {
                        0 => 8,
                        2 => 2,
                        4 => 4,
                        6 => 2,
                        _ => 1,
                    }
                }
                log::trace!("{}::from_bytes: min_size {}, position {}, alignment {}",
                       stringify!($name), <$name>::min_size(), stream.pos(),
                       alignment(stream.pos()));

                // Deserialize.
                let mut reader = $crate::mp4box::BoxReader::new(stream)?;
                let reader = &mut reader;

                match (reader.header.version, reader.header.max_version) {
                    (Some(version), Some(max_version)) => {
                        if version > max_version {
                            return Err(ioerr!(InvalidData, "{}: no support for version {}", stringify!($name), version));
                        }
                    },
                    _ => {},
                }

                let r: io::Result<$name> = {
                    def_struct!(@from_bytes $name, [], reader, $(
                        $field: $type $(<$gen>)?,
                    )*)
                };

                //log::trace!("XXX -- done frombyting {}", stringify!($name));

                r
            }

            fn min_size() -> usize {
                $(
                    def_struct!(@min_size $type $(<$gen>)?) +
                )* 0
            }
        }

        impl ToBytes for $name {
            #[allow(unused_variables)]
            fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {

                // Write the header.
                let mut stream = $crate::mp4box::BoxWriter::new(stream, self)?;
                let stream = &mut stream;

                // Serialize.
                def_struct!(@to_bytes self, stream, $(
                    $field: $type $(<$gen>)?,
                )*)
            }
        }
    };
}

// Define the MP4Box enum.
macro_rules! impl_enum {
    ($enum:ident, $($name:ident, $fourcc:expr),*) => {
        //
        // First define the enum.
        //

        /// All the boxes we know.
        #[derive(Clone)]
        pub enum $enum {
            $(
                $name($name),
            )+
            GenericBox(GenericBox),
        }

        impl $enum {
            #[allow(dead_code)]
            pub(crate) fn max_version_from_fourcc(fourcc: FourCC) -> Option<u8> {
                match &fourcc.to_be_bytes() {
                    $(
                        $fourcc => $name::max_version(),
                    )+
                    _ => None,
                }
            }

            /// Number of bytes when serialized.
            pub fn size(&self) -> u64 {
                let mut cb = crate::io::CountBytes::new();
                self.to_bytes(&mut cb).unwrap();
                cb.size()
            }

            /*
            pub fn check() {
                let mut ok = true;
                $(
                    // check if $fourcc == $name::FOURCC
                    if $name::FOURCC.as_bytes() != $fourcc {
                        if !($name::FOURCC == "stco" && $fourcc == b"co64") {
                            println!("mismatch: {:?} {:?}", std::str::from_utf8($fourcc), $name::FOURCC);
                            ok = false;
                        }
                    }
                )+
                if !ok {
                    panic!("MP4Box::check failed");
                }
            }*/


        }

        // Define FromBytes trait for the enum.
        impl FromBytes for $enum {
            fn from_bytes<R: ReadBytes>(mut stream: &mut R) -> io::Result<$enum> {

                // Peek at the header.
                let header = BoxHeader::peek(stream)?;
                log::trace!("MP4Box::from_bytes: header: {:?}", header);

                // If the version is too high, read it as a GenericBox.
                match (header.version, header.max_version) {
                    (Some(version), Some(max_version)) => {
                        if version > max_version {
                            //println!("XXX {:?}", header);
                            return Ok($enum::GenericBox(GenericBox::from_bytes(&mut stream)?));
                        }
                    },
                    _ => {},
                }

                // Read the body.
                let b = header.fourcc.to_be_bytes();
                let e = match &b {
                    $(
                        $fourcc => {
                            $enum::$name($name::from_bytes(stream)?)
                        },
                    )+
                    _ => $enum::GenericBox(GenericBox::from_bytes(stream)?),
                };
                Ok(e)
            }

            fn min_size() -> usize {
                8
            }
        }

        // Define ToBytes trait for the enum.
        impl ToBytes for $enum {
            fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
                match self {
                    $(
                        &$enum::$name(ref b) => b.to_bytes(stream),
                    )+
                    &$enum::GenericBox(ref b) => b.to_bytes(stream),
                }
            }
        }

        // Define BoxInfo trait for the enum.
        impl BoxInfo for $enum {
            #[inline]
            fn fourcc(&self) -> FourCC {
                match self {
                    $(
                        &$enum::$name(ref b) => b.fourcc(),
                    )+
                    &$enum::GenericBox(ref b) => b.fourcc(),
                }
            }
        }

        // Define FullBox trait for the enum.
        impl FullBox for $enum {
            fn version(&self) -> Option<u8> {
                match self {
                    $(
                        &$enum::$name(ref b) => b.version(),
                    )+
                    &$enum::GenericBox(ref b) => b.version(),
                }
            }
            fn flags(&self) -> u32 {
                match self {
                    $(
                        &$enum::$name(ref b) => b.flags(),
                    )+
                    &$enum::GenericBox(ref b) => b.flags(),
                }
            }
        }

        // Debug implementation that delegates to the variant.
        impl Debug for $enum {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                match self {
                    $(
                        &$enum::$name(ref b) => Debug::fmt(b, f),
                    )+
                    &$enum::GenericBox(ref b) => Debug::fmt(b, f),
                }
            }
        }

        $(
            impl_from!($name, $enum);
        )*
        impl_from!(GenericBox, $enum);
    };
}

macro_rules! impl_from {
    (ChunkLargeOffsetBox, $enum:ident) => {
        // skip.
    };
    ($name:ident, $enum:ident) => {
        /*
        impl From<$name> for $enum {
            fn from(value: $name) -> Self {
                $enum::$name(value)
            }
        }
        */
        impl $name {
            pub fn to_mp4box(self) -> $enum {
                $enum::$name(self)
            }
        }
    };
}

/// Find the first box of type $type in $vec.
#[macro_export]
macro_rules! first_box {
    (@FIELD $val:expr, SampleDescriptionBox) => {
        &$val.entries
    };
    (@FIELD $val:expr, $type:ident) => {
        &$val.boxes
    };
    (@MAIN $vec:expr, $type:ident) => {
        {
            let _x: Option<&$type> = $crate::iter_box!($vec, $type).next();
            _x
        }
    };
    (@MAIN $vec:expr, $type:ident $(/$path:ident)+) => {
        first_box!($vec, $type).and_then(|x| {
            let _i = first_box!(@FIELD x, $type);
            first_box!(@MAIN _i, $($path) / *)
        })
    };
    ($vec:ident, $type:ident $($tt:tt)*) => {
        first_box!(@MAIN $vec.boxes, $type $($tt)*)
    };
    ($vec:expr, $type:ident $($tt:tt)*) => {
        first_box!(@MAIN $vec, $type $($tt)*)
    };
}

/// Find the first box of type $type in $vec, mutable.
#[macro_export]
macro_rules! first_box_mut {
    (@FIELD $val:expr, SampleDescriptionBox) => {
        &mut $val.entries
    };
    (@FIELD $val:expr, $type:ident) => {
        &mut $val.boxes
    };
    (@MAIN $vec:expr, $type:ident) => {
        {
            let _x: Option<&mut $type> = $crate::iter_box_mut!($vec, $type).next();
            _x
        }
    };
    (@MAIN $vec:expr, $type:ident $(/$path:ident)+) => {
        first_box_mut!($vec, $type).and_then(|mut x| {
            let &mut _i = first_box_mut!(@FIELD x, $type);
            first_box_mut!(@MAIN _i, $($path) / *)
        })
    };
    ($vec:ident, $type:ident $($tt:tt)*) => {
        first_box_mut!(@MAIN $vec.boxes, $type $($tt)*)
    };
    ($vec:expr, $type:ident $($tt:tt)*) => {
        first_box_mut!(@MAIN $vec, $type $($tt)*)
    };
}

/// Iterate over all boxes of type $type in $vec.
#[macro_export]
macro_rules! iter_box {
    ($vec:ident, $type:ident) => {
        iter_box!($vec.boxes, $type)
    };
    ($vec:expr, $type:ident) => {
        $vec.iter().filter_map(|x| match x {
            &MP4Box::$type(ref b) => Some(b),
            _ => None,
        })
    };
}

/// Iterate over all boxes of type $type in $vec.
#[macro_export]
macro_rules! iter_box_mut {
    ($vec:ident, $type:ident) => {
        iter_box_mut!($vec.boxes, $type)
    };
    ($vec:expr, $type:ident) => {
        $vec.iter_mut().filter_map(|x| match x {
            &mut MP4Box::$type(ref mut b) => Some(b),
            _ => None,
        })
    };
}

/// Helper.
macro_rules! declare_box_methods {
    ($type:ident, $method:ident, $method_mut:ident) => {
        /// Get a reference to the $type.
        pub fn $method(&self) -> &$type {
            first_box!(&self.boxes, $type).unwrap()
        }
        /// Get a mutable reference to the $type.
        pub fn $method_mut(&mut self) -> &mut $type {
            first_box_mut!(&mut self.boxes, $type).unwrap()
        }
    };
}

/// Helper.
macro_rules! declare_box_methods_opt {
    ($type:ident, $method:ident, $method_mut:ident) => {
        /// Get an optional reference to the $type.
        pub fn $method(&self) -> Option<&$type> {
            first_box!(&self.boxes, $type)
        }
        /// Get an optional mutable reference to the $type.
        pub fn $method_mut(&mut self) -> Option<&mut $type> {
            first_box_mut!(&mut self.boxes, $type)
        }
    };
}
