//! macros to define a MP4 Box as a Rust struct.
//!
use std::any::Any;
use std::fmt::Debug;
use std::io;

use crate::fromtobytes::FromToBytes;
use crate::io::*;
use crate::types::*;

/// Trait to serialize and deserialize a box.
pub trait BoxFromToBytes {
    fn from_bytes<R: BoxReadBytes>(base: Option<Mp4Base>, bytes: &mut R) -> Self;
    fn to_bytes<W: BoxWriteBytes>(&self, bytes: &mut W);
    fn min_size() -> usize;
}

/// Trait implemented by every box.
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

// Basic structure of any box.
#[doc(hidden)]
pub struct Mp4Base {
    offset:         u64,
    size:           u64,
    orig_size:      u32,
    fourcc:         FourCC,
    boxes:          Vec<Box<dyn MP4Box>>,
    blob:           Vec<u8>,
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

    // Read the header.
    fn from_bytes<R: BoxReadBytes>(_base: Option<Mp4Base>, file: &mut R) -> Self {
        let offset = file.pos();
        let orig_size = u32::from_bytes(file);
        let fourcc = FourCC::from_bytes(file);
        let size = match orig_size {
            0 => file.left(),
            1 => u64::from_bytes(file),
            sz => sz as u64,
        };
        Mp4Base {
            offset,
            size,
            orig_size,
            fourcc,
            boxes: Vec::new(),
            blob: Vec::new(),
        }
    }

    // This is a generic, non-specialized box.
    // Just write the data.
    fn to_bytes<W: BoxWriteBytes>(&self, file: &mut W) {
        self.orig_size.to_bytes(file);
        self.fourcc.to_bytes(file);
        if self.orig_size == 1 {
            self.size.to_bytes(file);
        }
        file.write(&self.blob[..]).unwrap();
    }
    fn min_size() -> usize {
        8
    }
}

impl MP4Box for Mp4Base {
    #[inline]
    fn offset(&self) -> u64 { self.offset }
    #[inline]
    fn size(&self) -> u64 { self.size }
    #[inline]
    fn fourcc(&self) -> FourCC { self.fourcc }
    #[inline]
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
    #[inline]
    fn offset(&self) -> u64 { 0 }
    #[inline]
    fn size(&self) -> u64 { self.size }
    #[inline]
    fn fourcc(&self) -> FourCC { FourCC(0) }
    #[inline]
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

macro_rules! def_boxes {

    // Helper.
    (@set_version version, $stream:expr, $version:expr) => {
        $stream.set_version($version);
    };
    (@set_version $($_tt:tt)*) => {};

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
            //
            // The base has already been read, so we pass that in, and read the rest.
            //
            fn from_bytes<R: BoxReadBytes>(base: Option<Mp4Base>, file: &mut R) -> Self {
                println!("XXX {}::from_bytes", stringify!($name));
                let base = base.unwrap();

                // Every member variable.
                let mut res = def_struct!(@from_bytes $name, [ base, ], file, $(
                    $field: $type $(as $as)?,
                )*);
                println!("XXX done!");

                // Reset version.
                file.set_version(0);

                if def_boxes!(@is_container $container) {
                    res.base.read_boxes(file);
                }
                res
            }

            fn to_bytes<W: BoxWriteBytes>(&self, file: &mut W) {
                // First write the base, the the rest.
                self.base.to_bytes(file);

                // Every member variable.
                def_struct!(@to_bytes self, file, $(
                    $field: $type $(as $as)?,
                )*);

                // Reset version.
                file.set_version(0);
            }

            fn min_size() -> usize {
                $(
                    def_struct!(@min_size $type) +
                )* 0
            }
        }
        impl MP4Box for $name {
            #[inline]
            fn offset(&self) -> u64 { self.base.offset }
            #[inline]
            fn size(&self) -> u64 { self.base.size }
            #[inline]
            fn fourcc(&self) -> FourCC { self.base.fourcc }
            #[inline]
            fn boxes(&self) -> &Vec<Box<dyn MP4Box>> { &self.base.boxes }
        }
    };

    // Main entry point of def_boxes.
    ($($name:ident, $fourcc:expr, container: $container:tt, $fields:tt),* $(,)?) => {
        // First define all the structs.
        $(
            def_boxes!(@def_box $name, $container, $fields);
        )*

        impl Mp4Base {
            // This function "specializes" the base box.
            //
            // If the FourCC is reckognized, we map that to a struct, call
            // from_bytes on it, and Box it so that it becomes a Box<dyn MP4Box>.
            //
            // If we do not reckognize the box, we still read the data
            // but just store it as-is so that we can write it out again.
            //
            fn specialize<R: BoxReadBytes>(mut self, file: &mut R) -> Box<dyn MP4Box> {
                let box_end = self.offset + self.size;
                let mut file = file.limit(box_end);
                let b: Box<dyn MP4Box> = match &self.fourcc.0.to_be_bytes()[..] {
                    $(
                        $fourcc => Box::new($name::from_bytes(Some(self), &mut file)),
                    )*
                    other => {
                        if other != b"mdat" {
                            // Not reckognized, just store as a blob.
                            // Unless it is mdat, never store mdat as a blob in memory!
                            let blob = file.read(0).unwrap();
                            self.blob.extend_from_slice(blob);
                        }
                        Box::new(self)
                    },
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
        version:        Version,
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
        version:    Version,
        flags:      TrackFlags,
        cr_time:    Time,
        mod_time:   Time,
        track_id:   u32,
        skip:       4,
        duration:   u32,
        skip:       8,
        layer:      u16,
        alt_group:  u16,
        volume:     FixedFloat16,
        skip :      2,
        matrix:     Matrix,
        width:      FixedFloat32,
        height:     FixedFloat32,
    },
    Edits, b"edts", container: false, {
        version:    Version,
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
        version:    Version,
        flags:      u24,
        entries:    u32,
    },
    MediaInformationBox, b"minf", container: true,  {
    },
    VideoMediaInformation, b"vmhd", container: false, {
        version:        Version,
        flags:          u24,
        graphics_mode:  u16,
        opcolor:        OpColor,
    },
    SoundMediaHeader, b"smhd", container: false, {
        version:        Version,
        flags:          u24,
        balance:        u16,
        skip:           2,
    },
    NullMediaHeader, b"nmhd", container: false, {
    },
    UserDataBox, b"udta", container: true, {
        //udta_list:      [UserData],
    },
    TrackSelection, b"tsel", container: false, {
        version:        Version,
        flags:          u24,
        switch_group:   u32,
        attribute_list: [FourCC],
    },
    SampleDescription, b"stsd", container: false, {
        version:    Version,
        flags:      u24,
        entries:    u32,
        n1_size:    u32,
        n1_format:  FourCC,
        skip:       6,
        dataref_idx:    u16,
    },
    MediaHeader, b"mdhd", container: false, {
        version:    Version,
        flags:      u24,
        cr_time:    Time,
        mod_time:   Time,
        time_scale: u32,
        duration:   u32,
        language:   IsoLanguageCode,
        quality:    u16,
    },
    MovieHeader, b"mvhd", container: false, {
        version:    Version,
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
        version:    Version,
        flags:      u24,
        maintype:   FourCC,
        subtype:    FourCC,
        skip:       12,
        name:       ZString,
    },
    Free, b"free", container: false, {
    },
    Skip, b"skip", container: false, {
    },
    Wide, b"wide", container: false, {
    },
    MetaData, b"meta", container: true, {
        version:    Version,
        flags:      u24,
    },
    AppleItemList, b"ilst", container: false, {
        list:       [AppleItem],
    },
}

