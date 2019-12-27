///
/// This module contains macros to define a MP4 Box as a Rust struct.
///
use std::any::Any;
use std::fmt::Debug;
use std::io;
use crate::io::*;
use crate::types::*;

/// Trait to serialize and deserialize a box.
pub trait BoxFromToBytes {
    fn from_bytes<R: BoxReadBytes>(base: Option<Mp4Base>, bytes: &mut R) -> Self;
    fn to_bytes<W: BoxWriteBytes>(&self, bytes: &mut W);
    fn min_size() -> usize;
}

// Trait implemented by every box.
pub trait MP4Box: Debug + Any {
    fn offset(&self) -> u64;
    fn size(&self) -> u64;
    fn fourcc(&self) -> FourCC;
    fn boxes(&self) -> &Vec<Box<dyn MP4Box>>;
}

// Implementation so we can return Box<Movie> as Box<dyn MP4Box>.
impl<B: ?Sized + MP4Box> MP4Box for Box<B> {
    fn offset(&self) -> u64 { B::offset(&*self) }
    fn size(&self) -> u64 { B::size(&*self) }
    fn fourcc(&self) -> FourCC { B::fourcc(&*self) }
    fn boxes(&self) -> &Vec<Box<dyn MP4Box>> { B::boxes(&*self) }
}

/// Basic structure of any box.
pub struct Mp4Base {
    offset:         u64,
    size:           u64,
    orig_size:      u32,
    fourcc:         FourCC,
    boxes:          Vec<Box<dyn MP4Box>>,
}

impl Mp4Base {
    // read all the subboxes that are present in this
    // box at the current offset.
    fn read_boxes<R: BoxReadBytes>(&mut self, bytes: &mut R) {
        let maxpos = self.offset + self.size;
        while bytes.pos() < maxpos - 8 {
            let sub_box = Mp4Base::from_bytes(None, bytes);
            self.boxes.push(sub_box.specialize(bytes));
        }
    }
}

impl Debug for Mp4Base {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Mp4Box")
            //.field("location", &format!("{}:{}", self.offset, self.size))
            .field("fourcc", &self.fourcc)
            .finish()
    }
}

impl BoxFromToBytes for Mp4Base {
    fn from_bytes<R: BoxReadBytes>(_base: Option<Mp4Base>, bytes: &mut R) -> Self {
        let offset = bytes.pos();
        let orig_size = u32::from_bytes(bytes);
        let fourcc = FourCC::from_bytes(bytes);
        let size = match orig_size {
            1 => u64::from_bytes(bytes),
            sz => sz as u64,
        };
        let boxes = Vec::new();
        Mp4Base {
            offset,
            size,
            orig_size,
            fourcc,
            boxes,
        }
    }
    fn to_bytes<W: BoxWriteBytes>(&self, bytes: &mut W) {
        unimplemented!()
    }
    fn min_size() -> usize {
        8
    }
}

impl MP4Box for Mp4Base {
    fn offset(&self) -> u64 { self.offset }
    fn size(&self) -> u64 { self.size }
    fn fourcc(&self) -> FourCC { self.fourcc }
    fn boxes(&self) -> &Vec<Box<dyn MP4Box>> { &self.boxes }
}


#[derive(Debug)]
pub struct MP4 {
    size:   u64,
    boxes:  Vec<Box<dyn MP4Box>>,
}

impl MP4 {
    pub fn read<R: BoxReadBytes>(file: &mut R) -> MP4 {
        let mut boxes = Vec::new();
        while file.left() > 0 {
            println!("XXX 1 {}", file.left());
            let b = Mp4Base::from_bytes(None, file);
            println!("XXX 2 {}", file.left());
            boxes.push(b.specialize(file));
        }
        MP4 {
            size:   file.pos(),
            boxes:  boxes,
        }
    }
}

impl MP4Box for MP4 {
    fn offset(&self) -> u64 { 0 }
    fn size(&self) -> u64 { self.size }
    fn fourcc(&self) -> FourCC { FourCC(0) }
    fn boxes(&self) -> &Vec<Box<dyn MP4Box>> { &self.boxes }
}

// self.movie.and_then(|m| self.boxes[m as usize].downcast_ref::<Movie>())
pub struct IndexU32(u32);
impl IndexU32 {
    pub fn get(self) -> Option<u32> {
        match self.0 {
            0xffffffff => None,
            some => Some(some),
        }
    }
    pub fn set(&mut self, val: Option<u32>) {
        self.0 = val.unwrap_or(0xffffffff);
    }
}

macro_rules! def_struct {
    // minimum size for a certain type. we hard-code u* here.
    (@min_size u8) => { 1 };
    (@min_size u16) => { 2 };
    (@min_size u24) => { 3 };
    (@min_size u32) => { 4 };
    (@min_size u64) => { 8 };
    (@min_size u128) => { 16 };
    (@min_size [ $_type:ty ]) => { 0 };
    (@min_size $type:ident) => {
        $type::min_size()
    };
    (@min_size $amount:expr) => { $amount };

    // Define a struct line by line using accumulation and recursion.
    (@def_struct $name:ident, $( $field:tt: $type:tt $(as $as:tt)? ),* $(,)?) => {
        def_struct!(@def_struct_ $name, [ $( $field: $type $(as $as)?, )* ] -> []);
    };
    // During definition of the struct, we skip all the "skip" defitions.
    (@def_struct_ $name:ident, [ skip: $amount:tt, $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $name, [$($tt)*] -> [ $($res)* ]);
    };
    // Add normal field (as).
    (@def_struct_ $name:ident, [ $field:ident: $_type:ident as $type:ident, $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $name, [$($tt)*] -> [ $($res)* $field: $type, ]);
    };
    // Add normal field (u24).
    (@def_struct_ $name:ident, [ $field:ident: u24, $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $name, [$($tt)*] -> [ $($res)* $field: u32, ]);
    };
    // Add normal field (array).
    (@def_struct_ $name:ident, [ $field:ident: [ $type:ident ], $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $name, [$($tt)*] -> [ $($res)* $field: Vec<$type>, ]);
    };
    // Add normal field.
    (@def_struct_ $name:ident, [ $field:ident: $type:ident, $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $name, [$($tt)*] -> [ $($res)* $field: $type, ]);
    };
    // Final.
    (@def_struct_ $name: ident, [] -> [ $($res:tt)* ]) => {
        pub struct $name { $(
            $res
        )* }
    };

    // Generate the from_bytes details for a struct.
    (@from_bytes $name:ident, $base:tt, $stream:tt, $( $field:tt: $type:tt $(as $as:tt)? ),* $(,)?) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $( $field: $type $(as $as)?, )* ] -> [] []);
    };
    // Insert a skip instruction.
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ skip: $amount:tt, $($tt:tt)*]
        -> [ $($set:tt)* ] [ $($fields:tt)* ] ) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [ $stream.skip($amount).unwrap(); ] ] [$($fields)*]);
    };
    // Set a field (as)
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ $field:tt: $in:tt as $out:tt, $($tt:tt)*]
        -> [ $($set:tt)* ] [ $($fields:tt)* ]) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [ let $field = $in::from_bytes($stream) as $out; ] ] [ $($fields)* $field ]);
    };
    // Set a field (u24).
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ $field:tt: u24, $($tt:tt)*]
        -> [ $($set:tt)* ] [ $($fields:tt)* ]) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [ let $field = U24::from_bytes($stream).0; ] ] [ $($fields)* $field ]);
    };
    // Set a field (array).
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ $field:tt: [$type:tt], $($tt:tt)*]
        -> [ $($set:tt)* ] [ $($fields:tt)* ]) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [
                let mut $field = Vec::new();
                while $stream.left() >= $type::min_size() as u64 {
                    println!("XXX left: {}", $stream.left());
                    let v = $type::from_bytes($stream);
                    $field.push(v);
                }
            ] ] [ $($fields)* $field ]);
    };
    // Set a field.
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ $field:tt: $type:tt, $($tt:tt)*]
        -> [ $($set:tt)* ] [ $($fields:tt)* ]) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [ let $field = $type::from_bytes($stream); ] ] [ $($fields)* $field ]);
    };
    // Final.
    (@from_bytes_ $name:ident, [ $($base:tt)* ], $_stream:tt, [] -> [ $([$($set:tt)*])* ] [ $($field:tt)* ]) => {
        {
        $(
            $($set)*
        )*
        $name {
            $($base)*
            $(
                $field,
            )*
        } }
    };

    // Generate the to_bytes details for a struct.
    (@to_bytes $struct:expr, $stream:ident, $( $field:tt: $type:tt $(as $as:tt)? ),* $(,)?) => {
        def_struct!(@to_bytes_ $struct, $stream, [ $( $field: $type $(as $as)?, )* ] -> []);
    };
    // Insert a skip instruction.
    (@to_bytes_ $struct:expr, $stream:ident, [ skip: $amount:tt, $($tt:tt)*] -> [ $($set:tt)* ] ) => {
        def_struct!(@to_bytes_ $struct, $stream, [ $($tt)* ] ->
            [ $($set)* [ $stream.skip($amount).unwrap(); ] ] );
    };
    // Write a field value (as)
    (@to_bytes_ $struct:expr, $stream:ident, [ $field:tt: $type:tt as $_type:tt, $($tt:tt)*] -> [ $($set:tt)* ]) => {
        def_struct!(@to_bytes_ $struct, $stream, [ $($tt)* ] ->
            [ $($set)* [ ($struct.$field as $type).to_bytes($stream); ] ]);
    };
    // Write a field value (u24).
    (@to_bytes_ $struct:expr, $stream:ident, [ $field:tt: u24, $($tt:tt)*] -> [ $($set:tt)* ]) => {
        def_struct!(@to_bytes_ $struct, $stream, [ $($tt)* ] ->
            [ $($set)* [ U24($struct.$field as u32).to_bytes($stream); ] ]);
    };
    // Write a field value (array)
    (@to_bytes_ $struct:expr, $stream:ident, [ $field:tt: [$type:tt], $($tt:tt)*] -> [ $($set:tt)* ]) => {
        def_struct!(@to_bytes_ $struct, $stream, [ $($tt)* ] ->
            [ $($set)* [ for v in &$struct.$field { v.to_bytes($stream); } ] ]);
    };
    // Write a field value.
    (@to_bytes_ $struct:expr, $stream:ident, [ $field:tt: $type:tt, $($tt:tt)*] -> [ $($set:tt)* ]) => {
        def_struct!(@to_bytes_ $struct, $stream, [ $($tt)* ] ->
            [ $($set)* [ $struct.$field.to_bytes($stream); ] ]);
    };
    // Final.
    (@to_bytes_ $_struct:expr, $_stream:tt, [] -> [ $([$($set:tt)*])* ] ) => {
        {$(
            $($set)*
        )*}
    };

    // Helper.
    (@check_skip $this:expr, $dbg:expr, skip) => { };
    (@check_skip $this:expr, $dbg:expr, $field:ident) => { $dbg.field(stringify!($field), &$this.$field); };

    // Main entry point to define just one struct.
    ($name:ident, $($field:tt: $type:tt $(as $as:tt)?),* $(,)?) => {
        def_struct!(@def_struct $name,
            $(
                $field: $type $(as $as)?,
            )*
        );

        // Debug implementation that skips "skip"
        impl Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                let mut dbg = f.debug_struct(stringify!($name));
                $(
                    def_struct!(@check_skip self, dbg,  $field);
                )*
                dbg.finish()
            }
        }

        impl FromToBytes for $name {
            fn from_bytes<R: ReadBytes>(stream: &mut R) -> Self {
                def_struct!(@from_bytes $name, [], stream, $(
                    $field: $type $(as $as)?,
                )*)
            }

            fn to_bytes<W: WriteBytes>(&self, stream: &mut W) {
                def_struct!(@to_bytes self, stream, $(
                    $field: $type $(as $as)?,
                )*);
            }

            fn min_size() -> usize {
                $( def_struct!(@min_size $type) +)* 0
            }
        }
    }
}

macro_rules! def_boxes {

    // Helper.
    (@is_container true) => { true };
    (@is_container false) => { false };
    (@is_container [ $($_tt:tt)* ]) => { true };
    (@is_container) => { false };

    // Define one box.
    {@def_box $name:ident, $container:tt, { $($field:tt: $type:tt $(as $as:tt)?),* $(,)? }} => {
        def_struct!(@def_struct $name,
            base:   Mp4Base,
            $(
                $field: $type $(as $as)?,
            )*
        );

        // Debug implementation that hides the base struct.
        impl Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                let mut dbg = f.debug_struct(stringify!($name));
                //dbg.field("location", &format!("{}:{}", self.base.offset, self.base.size));
                dbg.field("fourcc", &self.base.fourcc);
                $(
                    def_struct!(@check_skip self, dbg,  $field);
                )*
                if def_boxes!(@is_container $container) {
                    dbg.field("boxes", &self.base.boxes);
                }
                dbg.finish()
            }
        }

        impl BoxFromToBytes for $name {
            fn from_bytes<R: BoxReadBytes>(base: Option<Mp4Base>, stream: &mut R) -> Self {
                println!("XXX {}::from_bytes", stringify!($name));
                let base = base.unwrap();
                let mut res = def_struct!(@from_bytes $name, [ base, ], stream, $(
                    $field: $type $(as $as)?,
                )*);
                println!("XXX done!");
                if def_boxes!(@is_container $container) {
                    res.base.read_boxes(stream);
                }
                res
            }

            fn to_bytes<W: BoxWriteBytes>(&self, stream: &mut W) {
                def_struct!(@to_bytes self, stream, $(
                    $field: $type $(as $as)?,
                )*);
            }

            fn min_size() -> usize {
                $(
                    def_struct!(@min_size $type) +
                )* 0
            }
        }
        impl MP4Box for $name {
            fn offset(&self) -> u64 { self.base.offset }
            fn size(&self) -> u64 { self.base.size }
            fn fourcc(&self) -> FourCC { self.base.fourcc }
            fn boxes(&self) -> &Vec<Box<dyn MP4Box>> { &self.base.boxes }
        }
    };

    // Main entry point of def_boxes.
    ($($name:ident, $fourcc:expr, container: $container:tt, $fields:tt),* $(,)?) => {
        // First define all the structs.
        $(
            def_boxes!(@def_box $name, $container, $fields);
        )*

        // Now create the mapping from fourcc to struct.
        impl Mp4Base {
            fn specialize<R: BoxReadBytes>(self, file: &mut R) -> Box<dyn MP4Box> {
                let box_end = self.offset + self.size;
                let mut file = file.limit(box_end);
                let b: Box<dyn MP4Box> = match &self.fourcc.0.to_be_bytes()[..] {
                    $(
                        $fourcc => Box::new($name::from_bytes(Some(self), &mut file)),
                    )*
                    _ => Box::new(self),
                };
                if file.pos() < box_end {
                    println!("XXX skipping to {}", box_end);
                    file.skip(box_end - file.pos()).unwrap();
                }
                b
            }
        }
    };
}

def_boxes! {
    FileType, b"ftyp", container: false, {
        major_brand:        FourCC,
        minor_version:      u32,
        compatible_brands:  [FourCC],
    },
    InitialObjectDescription, b"iods", container: false, {
        version:        u8,
        flags:          u24,
        audio_profile:  u8,
        video_profile:  u8,
    },
    MovieBox, b"moov", container: true, {
    },
    MovieFragmentBox, b"moof", container: true, {
    },
    TrackBox, b"trak", container: true, {
    },
    TrackHeader, b"tkhd", container: false, {
        version:    u8,
        flags:      TrackFlags,
        cr_time:    Time,
        mod_time:   Time,
        track_id:   u32,
        skip:       4,
        duration:   u32,
        skip:       8,
        layer:      u16,
        alt_group:  u16,
        volume:     u16,
        skip :      2,
        matrix:     Matrix,
        width:      u32,
        height:     u32,
    },
    Edits, b"edts", container: false, {
        version:    u8,
        flags:      u24,
        entries:    u32,
        table:      [EditList],
    },
    MediaBox, b"mdia", container: true,  {
    },
    SampleTableBox, b"stbl", container: true,  {
    },
    BaseMediaInformationHeader, b"gmhd", container: true,  {
    },
    DataInformationBox, b"dinf", container: true,  {
    },
    DataReference, b"dref", container: true, {
        version:    u8,
        flags:      u24,
        entries:    u32,
    },
    MediaInformationBox, b"minf", container: true,  {
    },
    VideoMediaInformation, b"vmhd", container: false, {
        version:        u8,
        flags:          u24,
        graphics_mode:  u16,
        opcolor:        OpColor,
    },
    SampleTable, b"stbl", container: true, {
    },
    SampleDescription, b"stsd", container: false, {
        version:    u8,
        flags:      u24,
        entries:    u32,
        n1_size:    u32,
        n1_format:  FourCC,
        skip:       6,
        dataref_idx:    u16,
    },
    MediaHeader, b"mdhd", container: false, {
        version:    u8,
        flags:      u24,
        cr_time:    Time,
        mod_time:   Time,
        time_scale: u32,
        duration:   u32,
        language:   IsoLanguageCode,
        quality:    u16,
    },
    MovieHeader, b"mvhd", container: false, {
        version:    u8,
        flags:      u24,
        cr_time:    Time,
        mod_time:   Time,
        timescale:  u32,
        duration:   u32,
        pref_rate:  u32,
        pref_vol:   u16,
        skip:       10,
        matrix:     Matrix,
        preview_time:   u32,
        preview_duration:   u32,
        poster_time:    u32,
        selection_time: u32,
        selection_duration: u32,
        current_time:   u32,
        next_track_id: u32,
    },
    Handler, b"hdlr", container: false, {
        version:    u8 as u32,
        flags:      u24,
        maintype:   FourCC,
        subtype:    FourCC,
        skip:       12,
        name:       ZString,
    },
}

