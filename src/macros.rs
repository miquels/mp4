
//
//
// Macro that is used to declare _all_ boxes.
//
//

/// Declare a complete list of all boxes.
macro_rules! def_boxes {

    // main entry point.
    ($enum:ident, $($name:ident, $fourcc:expr,  [$($maxver:tt)? $(,$deps:ident)*]  $(=> $block:tt)? ; )+) => {

        def_boxes!(@DEF_MP4BOX $enum, $($name, $fourcc),*);

        $(
            // Call def_box! if needed.
            def_boxes!(@DEF_BOX $name, $fourcc, $($block)*);

            // Implement FullBox automatically if possible.
            def_boxes!(@FULLBOX $name, [$($maxver)? $(,$deps)*]);

            // Implement BoxInfo.
            def_boxes!(@BOXINFO $name, $fourcc, [$($maxver)? $(,$deps)*]);

            // Implement BoxChildren trait for this struct.
            // def_boxes!(@CHILDREN $struct, $children);
        )+
    };

    /*
    // not implemented yet.
    (@CHILDREN $name:ident, -) => {};
    (@CHILDREN $name:ident, $field:ident) => {
        impl BoxChildren for $name {
            fn children(&self) -> Option<&[Box<dyn MP4Box>]> {
                Some(&self.$field[..])
            }
        }
    };
    */

    // Implement the FullBox trait for this struct.
    (@FULLBOX $name:ident, []) => {
        // Not a fullbox - default impl.
        impl FullBox for $name {}
    };
    (@FULLBOX $name:ident, [0]) => {
        // Fullbox always version 0.
        impl FullBox for $name {
            fn version(&self) -> Option<u8> { Some(0) }
        }
    };
    (@FULLBOX $name:ident, [$maxver:tt]) => {
        // Nothing, delegated to boxes/*.rs
    };
    (@FULLBOX $name:ident, [$maxver:tt $(,$deps:ident)+ ]) => {
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
    };

    // Implement the BoxInfo trait for this struct.
    (@BOXINFO $name:ident, $fourcc:expr, [$($maxver:tt)? $(,$deps:ident)*]) => {
        impl BoxInfo for $name {
            #[inline]
            fn fourcc(&self) -> FourCC {
                FourCC(u32::from_be_bytes(*$fourcc))
            }
            $(
                #[inline]
                fn max_version() -> Option<u8> {
                    Some($maxver)
                }
            )?
        }
    };

    // Define the box itself, either through def_box! or by including a module.
    (@DEF_BOX $name:ident, $fourcc:expr, { $($tt:tt)* }) => {
        def_box! {
            $name, $fourcc, $($tt)*
        }
    };
    // def_box that points to module.
    (@DEF_BOX $name:ident, $fourcc:expr, $mod:ident) => {
        mod $mod;
        pub use $mod::*;
    };
    // empty def_box.
    (@DEF_BOX $name:ident, $fourcc:expr,) => {
    };
    // Define the MP4Box enum.
    (@DEF_MP4BOX $enum:ident, $($name:ident, $fourcc:expr),*) => {
        //
        // First define the enum.
        //


        /// All the boxes we know.
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
        }

        // Define FromBytes trait for the enum.
        impl FromBytes for $enum {
            fn from_bytes<R: ReadBytes>(mut stream: &mut R) -> io::Result<$enum> {

                // Peek at the header.
                let saved_pos = stream.pos();
                let header = BoxHeader::read(stream)?;
                stream.seek(saved_pos)?;

                //debug!("XXX got reader for {:?} left {}", reader.header, reader.left());

                // If the version is too high, read it as a GenericBox.
                match (header.version, header.max_version) {
                    (Some(version), Some(max_version)) => {
                        if version > max_version {
                            println!("XXX {:?}", header);
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

        // Define BoxInfo trait for the enum.
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
    };
}

//
//
// Macro that is used to declare one box.
//
//

// Define one box.
macro_rules! def_box {

    ($(#[$outer:meta])* $name:ident, $_fourcc:expr, $( $field:tt: $type:tt $(as $as:tt)? ),* $(,)?) => {
        // Define the struct itself.
        def_struct!(@def_struct $(#[$outer])* $name,
            $(
                $field: $type $(as $as)?,
            )*
        );

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

        impl FromBytes for $name {
            #[allow(unused_variables)]
            fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<$name> {

                debug!("XXX frombyting {} min_size is {} stream.left is {}",
                       stringify!($name), <$name>::min_size(), stream.left());

                // Deserialize.
                let mut reader = $crate::mp4box::BoxReader::new(stream)?;
                let reader = &mut reader;

                match (reader.header.version, reader.header.max_version) {
                    (Some(version), Some(max_version)) => {
                        if version > max_version {
                            return Err(io::Error::new(io::ErrorKind::InvalidData,
                                format!("{}: no suppor for version {}", stringify!($name), version)));
                        }
                    },
                    _ => {},
                }

                let r: io::Result<$name> = {
                    def_struct!(@from_bytes $name, [], reader, $(
                        $field: $type $(as $as)?,
                    )*)
                };

                debug!("XXX -- done frombyting {}", stringify!($name));

                r
            }

            fn min_size() -> usize {
                $(
                    def_struct!(@min_size $type) +
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
                    $field: $type $(as $as)?,
                )*)
            }
        }
    };
}

/// Find the first box of type $type in $vec.
macro_rules! first_box {
    (@FIELD $val:expr, SampleDescriptionBox) => {
        &$val.entries
    };
    (@FIELD $val:expr, $type:ident) => {
        &$val.boxes
    };
    (@MAIN $vec:expr, $type:ident) => {
        {
            let _x: Option<&$type> = iter_box!($vec, $type).next();
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
macro_rules! first_box_mut {
    (@FIELD $val:expr, SampleDescriptionBox) => {
        &mut $val.entries
    };
    (@FIELD $val:expr, $type:ident) => {
        &mut $val.boxes
    };
    (@MAIN $vec:expr, $type:ident) => {
        {
            let _x: Option<&mut $type> = iter_box_mut!($vec, $type).next();
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

/// Find all boxes of type $type in $vec.
macro_rules! iter_box {
    ($vec:ident, $type:ident) => {
        iter_box!($vec.boxes, $type)
    };
    ($vec:expr, $type:ident) => {
        $vec.iter().filter_map(|x| {
            match x {
                &MP4Box::$type(ref b) => Some(b),
                _ => None,
            }
        })
    };
}

macro_rules! iter_box_mut {
    ($vec:ident, $type:ident) => {
        iter_box_mut!($vec.boxes, $type)
    };
    ($vec:expr, $type:ident) => {
        $vec.iter_mut().filter_map(|x| {
            match x {
                &mut MP4Box::$type(ref mut b) => Some(b),
                _ => None,
            }
        })
    };
}

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
    }
}

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
    }
}
