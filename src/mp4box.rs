//!
use std::any::Any;
use std::fmt::Debug;
use std::io;

use crate::fromtobytes::FromToBytes;
use crate::io::*;
use crate::types::*;

/// Common functionality for all boxes.
pub trait MP4Box: Debug + Any + FromToBytes {
    fn fourcc(&self) -> FourCC;
    fn alignment(&self) -> usize;
}

/// Implemented if a box kan have sub-boxes.
pub trait BoxChildren {
    fn children(&self) -> Option<&[Box<dyn MP4Box>]> {
        None
    }
}

/// Reads a box from a stream.
pub fn read_box(stream: &mut dyn BoxReadBytes) -> io::Result<Box<dyn MP4Box>> {
    let size = u32::read_bytes()?.from_be() as u64;
    let fourcc = FourCC::read_bytes()?;
    let limit = match size {
        0 => stream.size() - stream.pos(),
        1 => u64::read_bytes()?.saturating_sub(16),
        x => x.saturating_sub(8),
    };
    let s = stream.limit(size);
    let b = read_box_content(stream)?;
    s.skip(s.left())?;
    Ok(b)
}

// Implementations so we can return Box<Movie> as Box<dyn MP4Box>.
impl<B: ?Sized + BoxChildren> BoxChildren for Box<B> {
    fn children(&self) -> Option<&[Box<dyn MP4Box>]> { B::children(&*self) }
}
impl<B: ?Sized + MP4Box> MP4Box for Box<B> {
    fn fourcc(&self) -> FourCC { B::fourcc(&*self) }
    fn alignment(&self) -> usize { B::alignment(&*self) }
}

/// Main entry point is via this struct.
#[derive(Debug)]
pub struct MP4 {
    size:   u64,
    boxes:  Vec<Box<dyn MP4Box>>,
}

impl MP4 {
    /// Read the structure of an entire MP4 file.
    ///
    /// It reads all the boxes into memory, except for "mdat" for abvious reasons.
    pub fn read<R: BoxReadBytes>(file: &mut R) -> io::Result<MP4> {
        let mut boxes = Vec::new();
        while file.left() >= 8 {
            let b = read_box(file)?;
            boxes.push(b);
        }
        Ok(MP4 {
            size:   file.pos(),
            boxes:  boxes,
        })
    }

    /// Iterate over all boxes in this MP4 container.
    pub fn boxes(&self) -> &[Box<dyn MP4Box>] {
        &self.boxes[..]
    }
}

impl BoxChildren for MP4 {
    fn children(&self) -> Option<&[Box<dyn MP4Box>]> {
        Some(self.boxes())
    }
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

/// Any unknown boxes we encounted are put into a GenericBox.
pub struct GenericBox {
    pub fourcc: FourCC,
    pub data: Vec<u8>,
}

impl GenericBox {
    // overrides the trait method.
    pub fn from_bytes<R: ReadBytes>(stream: &mut R, fourcc: FourCC) -> io::Result<GenericBox> {
        GenericBox {
            fourcc,
            data: stream.read(0)?.to_vec(),
        }
    }
}

impl FromToBytes for GenericBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
        unimplemented!()
    }
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        stream.write(&self.data)
    }
    fn min_size() -> usize {
        0
    }
}

impl MP4Box for GenericBox {
    #[inline]
    fn fourcc(&self) -> FourCC {
        self.fourcc.clone()
    }
    #[inline]
    fn alignment(&self) -> usize {
        0
    }
}

struct U8Array(usize);

impl Debug for U8Array {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "u8[{}]", &self.0)
    }
}

impl Debug for GenericBox {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut dbg = f.debug_struct("Box");
        dbg.field("fourcc", &self.fourcc);
        dbg.field("data", &U8Array(self.data.len()));
        dbg.finish()
    }
}

/// Declare a complete list of all boxes.
macro_rules! def_boxes {
    (@CHILDREN $struct:ty, -) => {};
    (@CHILDREN $struct:ty, $field:ident) => {
        impl BoxChildren for $struct {
            fn children(&self) -> Option<&[Box<dyn MP4Box>]> {
                Some(&self.$field[..])
            }
        }
    };
    ($({ $fourcc:expr, $align:expr, $struct:ty, $children:tt }),+) => {

        // Delegate reading a box to its struct based on the FourCC.
        fn read_box_content(stream: &mut BoxReadBytes, fourcc: FourCC) => io::Result<Box<dyn MP4Box>> {
            let b = match &($fourcc.to_be_bytes())[..] {
                $(
                    $fourcc => {
                        // caller will have limited the size of the stream to
                        // the box length, so after reading the contents,
                        // skip what is left in case there was some unused
                        // space at the end (might be because of alignment).
                        let b = Box::new($struct::from_bytes(stream)?);
                        stream.skip(stream.lef())?;
                        b
                    },
                ),+
                other => Box::new(GenericBox::from_bytes(stream, $fourcc.into())?),
            };
            Ok(b)
        }

        $(
            // Implement MP4Box trait for this struct.
            impl MP4Box for $struct {
                #[inline]
                fn fourcc(&self) -> FourCC {
                    FourCC::new($fourcc)
                }
                #[inline]
                fn alignment(&self) -> usize {
                    $align
                }
            }

            // Implement BoxChildren trait for this struct.
            def_boxes!(@CHILDREN $struct, $children);
        )+
    }
}

// Define one box.
macro_rules! def_box {

    ($name:ident, $_fourcc:expr, $( $field:tt: $type:tt $(as $as:tt)? ),* $(,)?) => {
        // Define the struct itself.
        def_struct!(@def_struct $name,
            $(
                $field: $type $(as $as)?,
            )*
        );

        // Debug implementation that adds fourcc field.
        impl Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                let mut dbg = f.debug_struct(stringify!($name));
                dbg.field("fourcc", &self.fourcc());
                $(
                    def_struct!(@check_skip self, dbg,  $field);
                )*
                dbg.finish()
            }
        }

        impl FromToBytes for $name {
            fn from_bytes<R: BoxReadBytes>(file: &mut R) -> io::Result<Self> {

                // Deserialize.
                let res = def_struct!(@from_bytes $name, [], file, $(
                    $field: $type $(as $as)?,
                )*)?;

                // Reset version.
                file.set_version(0);

                Ok(res)
            }

            fn to_bytes<W: BoxWriteBytes>(&self, file: &mut W) -> io::Result<()> {

                // Serialize.
                def_struct!(@to_bytes self, file, $(
                    $field: $type $(as $as)?,
                )*)?;

                // Reset version.
                file.set_version(0);

                Ok(())
            }

            fn min_size() -> usize {
                $(
                    def_struct!(@min_size $type) +
                )* 0
            }
        }
    };
}

def_box! { FileType, "ftyp",
    major_brand:        FourCC,
    minor_version:      u32,
    compatible_brands:  [FourCC],
}

def_box! { InitialObjectDescription, "iods",
    version:        Version,
    flags:          u24,
    audio_profile:  u8,
    video_profile:  u8,
}

def_box! { MovieBox, "moov",
    sub_boxes:      [Box<dyn MP4Box>],
}

def_box! { MovieFragmentBox, "moof",
    sub_boxes:      [Box<dyn MP4Box>],
}

def_box! { TrackBox, "trak",
    sub_boxes:      [Box<dyn MP4Box>],
}

def_box! { TrackHeader, "tkhd",
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
    volume:     FixedFloat8_8,
    skip :      2,
    matrix:     Matrix,
    width:      FixedFloat16_16,
    height:     FixedFloat16_16,
}

def_box! { Edits, "edts",
    version:    Version,
    flags:      u24,
    entries:    u32,
    table:      [EditList],
}

def_box! { MediaBox, "mdia",
    sub_boxes:      [Box<dyn MP4Box>],
}

def_box! { SampleTableBox, "stbl",
    sub_boxes:      [Box<dyn MP4Box>],
}

def_box! { BaseMediaInformationHeader, "gmhd",
    sub_boxes:      [Box<dyn MP4Box>],
}

def_box! { DataInformationBox, "dinf",
    sub_boxes:      [Box<dyn MP4Box>],
}

def_box! { DataReference, "dref",
    version:    Version,
    flags:      Flags,
    entries:    u32,
}

def_box! { MediaInformationBox, "minf",
    sub_boxes:      [Box<dyn MP4Box>],
}

def_box! { VideoMediaInformation, "vmhd",
    version:        Version,
    flags:          Flags,
    graphics_mode:  u16,
    opcolor:        OpColor,
}

def_box! { SoundMediaHeader, "smhd",
    version:        Version,
    flags:          Flags,
    balance:        u16,
    skip:           2,
}

def_box! { NullMediaHeader, "nmhd",
}

def_box! { UserDataBox, "udta",
    sub_boxes:      [Box<dyn MP4Box>],
}

def_box! { TrackSelection, "tsel",
    version:        Version,
    flags:          Flags,
    switch_group:   u32,
    attribute_list: [FourCC],
}

def_box! { SampleDescription, "stsd",
    version:    Version,
    flags:      Flags,
    entries:    u32,
    n1_size:    u32,
    n1_format:  FourCC,
    skip:       6,
    dataref_idx:    u16,
}

def_box! { MediaHeader, "mdhd",
    version:    Version,
    flags:      Flags,
    cr_time:    Time,
    mod_time:   Time,
    time_scale: u32,
    duration:   u32,
    language:   IsoLanguageCode,
    quality:    u16,
}

def_box! { MovieHeader, "mvhd",
    version:    Version,
    flags:      Flags,
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
}

def_box! { Handler, "hdlr",
    version:    Version,
    flags:      Flags,
    maintype:   FourCC,
    subtype:    FourCC,
    skip:       12,
    name:       ZString,
}

def_box! { Free, "free",
}

def_box! { Skip, "skip",
}

def_box! { Wide, "wide",
}

def_box! { MetaData, "meta",
    version:    Version,
    flags:      Flags,
    sub_boxes:  [Box<dyn MP4Box>],
}

def_box! { Name, "name",
    name:       ZString,
}

def_box! { AppleItemList, "ilst",
    list:       [AppleItem],
}

