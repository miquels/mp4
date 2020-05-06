use std::fmt::Debug;
use std::io;

use crate::boxes::MP4Box;
use crate::io::ReadAt;
use crate::serialize::{BoxBytes, FromBytes, ReadBytes, ToBytes, WriteBytes};
use crate::types::*;

/// Gets implemented for every box.
pub trait BoxInfo {
    /// The "fourcc" name of this box.
    fn fourcc(&self) -> FourCC;
    /// Sub-boxes if this is a container.
    fn boxes(&self) -> Option<&[MP4Box]> {
        None
    }
    /// Highest version that we reckognize.
    /// If it is the default (None) this is a basebox.
    fn max_version() -> Option<u8> {
        None
    }
}

/// Calculated version and flags.
pub trait FullBox {
    /// What version of the containing box does this type require, based on its value
    fn version(&self) -> Option<u8> {
        None
    }
    /// What are the flags for the full box.
    fn flags(&self) -> u32 {
        0
    }
}

//
//
// Helpers to read and write the box header.
//
//

#[derive(Debug, Clone)]
pub struct BoxHeader {
    pub(crate) size:        u64,
    pub(crate) fourcc:      FourCC,
    pub(crate) version:     Option<u8>,
    pub(crate) flags:       u32,
    pub(crate) max_version: Option<u8>,
}

impl BoxHeader {
    pub(crate) fn read(mut stream: &mut impl ReadBytes) -> io::Result<BoxHeader> {
        let size1 = u32::from_bytes(&mut stream)?;
        let fourcc = FourCC::from_bytes(&mut stream)?;
        let mut size = match size1 {
            0 => stream.size() - stream.pos(),
            1 => u64::from_bytes(&mut stream)?.saturating_sub(16),
            x => x.saturating_sub(8) as u64,
        };

        let max_version = MP4Box::max_version_from_fourcc(fourcc.clone());
        let mut version = None;
        let mut flags = 0;
        if max_version.is_some() {
            version = Some(u8::from_bytes(&mut stream)?);
            let data = stream.read(3)?;
            let mut buf = [0u8; 4];
            (&mut buf[1..]).copy_from_slice(&data);
            flags = u32::from_be_bytes(buf);
            size -= 4;
        }

        let b = Ok(BoxHeader {
            size,
            fourcc,
            version,
            flags,
            max_version,
        });
        debug!("BoxHeader::read: {:?}", b);
        b
    }
}

/// Limited reader that reads no further than the box size.
pub struct BoxReader<'a> {
    pub(crate) header: BoxHeader,
    maxsize:           u64,
    // We box it, since a BoxReader might contain a BoxReader.
    inner:             Box<dyn ReadBytes + 'a>,
}

impl<'a> BoxReader<'a> {
    /// Read the box header, then return a size-limited reader.
    pub fn new<R: ReadBytes>(mut stream: &'a mut R) -> io::Result<BoxReader<'a>> {
        let header = BoxHeader::read(&mut stream)?;
        let maxsize = std::cmp::min(stream.size(), stream.pos() + header.size);
        debug!(
            "XXX header {:?} maxsize {} left {}",
            header,
            maxsize,
            stream.left()
        );
        Ok(BoxReader {
            header,
            maxsize,
            inner: Box::new(stream),
        })
    }
}

impl<'a> Drop for BoxReader<'a> {
    fn drop(&mut self) {
        if self.pos() < self.maxsize {
            debug!(
                "XXX BoxReader {} drop: skipping {}",
                self.header.fourcc,
                self.maxsize - self.pos()
            );
            let _ = self.skip(self.maxsize - self.pos());
        }
    }
}

// Delegate ReadBytes to the inner reader.
impl<'a> ReadBytes for BoxReader<'a> {
    fn read(&mut self, amount: u64) -> io::Result<&[u8]> {
        if amount == 0 {
            debug!(
                "XXX self reader for {} amount 0 left {}",
                self.header.fourcc,
                self.left()
            );
        }
        let amount = if amount == 0 { self.left() } else { amount };
        if amount == 0 {
            return Ok(b"");
        }
        if self.inner.pos() + amount > self.maxsize {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.inner.read(amount)
    }
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        if self.inner.pos() + amount > self.maxsize {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.inner.skip(amount)
    }
    #[inline]
    fn left(&self) -> u64 {
        let pos = self.inner.pos();
        if pos > self.maxsize {
            0
        } else {
            self.maxsize - pos
        }
    }
}

// Delegate BoxBytes to the inner reader.
impl<'a> BoxBytes for BoxReader<'a> {
    fn pos(&self) -> u64 {
        self.inner.pos()
    }
    fn seek(&mut self, pos: u64) -> io::Result<()> {
        if pos > self.maxsize {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.inner.seek(pos)
    }
    fn size(&self) -> u64 {
        self.maxsize
    }
    fn version(&self) -> u8 {
        self.header.version.unwrap_or(0)
    }
    fn flags(&self) -> u32 {
        self.header.flags
    }
    fn fourcc(&self) -> FourCC {
        self.header.fourcc.clone()
    }
}

/// Writes the box header.
pub struct BoxWriter<'a> {
    offset:    u64,
    vflags:    u32,
    inner:     Box<dyn WriteBytes + 'a>,
    finalized: bool,
}

impl<'a> BoxWriter<'a> {
    /// Write a provisional box header, then return a new stream. When
    /// the stream is dropped, the box header is updated.
    pub fn new<B>(mut stream: impl WriteBytes + 'a, boxinfo: &B) -> io::Result<BoxWriter<'a>>
    where
        B: BoxInfo + FullBox,
    {
        let offset = stream.pos();
        0u32.to_bytes(&mut stream)?;
        boxinfo.fourcc().to_bytes(&mut stream)?;
        let mut vflags = 0;
        if B::max_version().is_some() {
            let version = boxinfo.version().unwrap_or(0) as u32;
            vflags = version << 24 | boxinfo.flags();
            vflags.to_bytes(&mut stream)?;
        }
        Ok(BoxWriter {
            offset,
            vflags,
            inner: Box::new(stream),
            finalized: false,
        })
    }

    /// Finalize the box: seek back to the header and write the size.
    ///
    /// If you don't call this explicitly, it is done automatically when the
    /// BoxWriter is dropped. Any I/O errors will result in panics.
    pub fn finalize(&mut self) -> io::Result<()> {
        self.finalized = true;
        let pos = self.inner.pos();
        self.inner.seek(self.offset)?;
        let sz = (pos - self.offset) as u32;
        sz.to_bytes(&mut self.inner)?;
        self.inner.seek(pos)?;
        Ok(())
    }
}

impl<'a> Drop for BoxWriter<'a> {
    fn drop(&mut self) {
        self.finalize().unwrap();
    }
}

// Delegate WriteBytes to the inner writer.
impl<'a> WriteBytes for BoxWriter<'a> {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.inner.write(data)
    }
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        self.inner.skip(amount)
    }
}

// Delegate BoxBytes to the inner writer.
impl<'a> BoxBytes for BoxWriter<'a> {
    fn pos(&self) -> u64 {
        self.inner.pos()
    }
    fn seek(&mut self, pos: u64) -> io::Result<()> {
        self.inner.seek(pos)
    }
    fn size(&self) -> u64 {
        self.inner.size()
    }
    fn version(&self) -> u8 {
        ((self.vflags & 0xff000000) >> 24) as u8
    }
    fn flags(&self) -> u32 {
        self.vflags & 0x00ffffff
    }
    fn fourcc(&self) -> FourCC {
        self.inner.fourcc()
    }
    fn mdat_ref(&self) -> Option<&dyn ReadAt> {
        self.inner.mdat_ref()
    }
}

/// Read a collection of boxes from a stream.
pub fn read_boxes<R: ReadBytes>(mut file: R) -> io::Result<Vec<MP4Box>> {
    let mut boxes = Vec::new();
    while file.left() >= 8 {
        let b = MP4Box::from_bytes(&mut file)?;
        boxes.push(b);
    }
    Ok(boxes)
}

/// Write a collection of boxes to a stream.
pub fn write_boxes<W: WriteBytes>(mut file: W, boxes: &[MP4Box]) -> io::Result<()> {
    for b in boxes {
        b.to_bytes(&mut file)?;
    }
    Ok(())
}

//
//
// Helper types.
//
//

/// Any unknown boxes we encounted are put into a GenericBox.
pub struct GenericBox {
    fourcc: FourCC,
    data:   Vec<u8>,
    size:   u64,
    skip:   bool,
}

impl FromBytes for GenericBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<GenericBox> {
        let mut reader = BoxReader::new(stream)?;
        let stream = &mut reader;

        let size = stream.left();
        let data;
        let skip;
        if size == 0 {
            skip = false;
            data = vec![];
        } else if size < 65536 {
            if size == 0 || size == 3353696 {
                debug!("GenericBox::from_bytes: size {}", size);
            }
            skip = false;
            data = stream.read(size)?.to_vec();
        } else {
            skip = true;
            stream.skip(size)?;
            data = vec![];
        }
        Ok(GenericBox {
            fourcc: stream.fourcc(),
            data,
            skip,
            size,
        })
    }
    fn min_size() -> usize {
        8
    }
}

impl ToBytes for GenericBox {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self)?;
        writer.write(&self.data)?;
        writer.finalize()
    }
}

impl BoxInfo for GenericBox {
    #[inline]
    fn fourcc(&self) -> FourCC {
        self.fourcc.clone()
    }
}

impl FullBox for GenericBox {}

impl Debug for GenericBox {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut dbg = f.debug_struct("GenericBox");
        dbg.field("fourcc", &self.fourcc);
        let data = format!("[u8; {}]", self.size);
        dbg.field("data", &data);
        if self.skip {
            dbg.field("skip", &true);
        }
        dbg.finish()
    }
}

//
//
// Macro that is used to declare _all_ boxes.
//
//

/// Declare a complete list of all boxes.
macro_rules! def_boxes {
    /*
    (@CHILDREN $name:ident, -) => {};
    (@CHILDREN $name:ident, $field:ident) => {
        impl BoxChildren for $name {
            fn children(&self) -> Option<&[Box<dyn MP4Box>]> {
                Some(&self.$field[..])
            }
        }
    };
    */

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

    // def_box delegates most of the work to the def_box macro.
    (@DEF $name:ident, $fourcc:expr, { $($tt:tt)* }) => {
        def_box! {
            $name, $fourcc, $($tt)*
        }
    };
    // def_box that points to module.
    (@DEF $name:ident, $fourcc:expr, $mod:ident) => {
        mod $mod;
        pub use $mod::*;
    };
    // empty def_box.
    (@DEF $name:ident, $fourcc:expr,) => {
    };

    ($($name:ident, $fourcc:expr,  [$($maxver:tt)? $(,$deps:ident)*]  $(=> $block:tt)? ; )+) => {

        //
        // First define the enum.
        //

        /// All the boxes we know.
        pub enum MP4Box {
            $(
                $name($name),
            )+
            GenericBox(GenericBox),
        }

        impl MP4Box {
            pub(crate) fn max_version_from_fourcc(fourcc: FourCC) -> Option<u8> {
                let b = fourcc.to_be_bytes();
                match std::str::from_utf8(&b[..]).unwrap_or("") {
                    $(
                        $fourcc => $name::max_version(),
                    )+
                    _ => None,
                }
            }
        }

        // Define FromBytes trait for the enum.
        impl FromBytes for MP4Box {
            fn from_bytes<R: ReadBytes>(mut stream: &mut R) -> io::Result<MP4Box> {

                // Peek at the header.
                let saved_pos = stream.pos();
                let header = BoxHeader::read(stream)?;
                stream.seek(saved_pos)?;

                //debug!("XXX got reader for {:?} left {}", reader.header, reader.left());

                // If the version is too high, read it as a GenericBox.
                match (header.version, header.max_version) {
                    (Some(version), Some(max_version)) => {
                        if version > max_version {
                            return Ok(MP4Box::GenericBox(GenericBox::from_bytes(&mut stream)?));
                        }
                    },
                    _ => {},
                }

                // Read the body.
                let b = header.fourcc.to_be_bytes();
                let e = match std::str::from_utf8(&b[..]).unwrap_or("") {
                    $(
                        $fourcc => {
                            MP4Box::$name($name::from_bytes(stream)?)
                        },
                    )+
                    _ => MP4Box::GenericBox(GenericBox::from_bytes(stream)?),
                };
                Ok(e)
            }

            fn min_size() -> usize {
                8
            }
        }

        // Define ToBytes trait for the enum.
        impl ToBytes for MP4Box {
            fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
                match self {
                    $(
                        &MP4Box::$name(ref b) => b.to_bytes(stream),
                    )+
                    &MP4Box::GenericBox(ref b) => b.to_bytes(stream),
                }
            }
        }

        // Define BoxInfo trait for the enum.
        impl BoxInfo for MP4Box {
            #[inline]
            fn fourcc(&self) -> FourCC {
                match self {
                    $(
                        &MP4Box::$name(ref b) => b.fourcc(),
                    )+
                    &MP4Box::GenericBox(ref b) => b.fourcc(),
                }
            }
        }

        // Define BoxInfo trait for the enum.
        impl FullBox for MP4Box {
            fn version(&self) -> Option<u8> {
                match self {
                    $(
                        &MP4Box::$name(ref b) => b.version(),
                    )+
                    &MP4Box::GenericBox(ref b) => b.version(),
                }
            }
            fn flags(&self) -> u32 {
                match self {
                    $(
                        &MP4Box::$name(ref b) => b.flags(),
                    )+
                    &MP4Box::GenericBox(ref b) => b.flags(),
                }
            }
        }

        // Debug implementation that delegates to the variant.
        impl Debug for MP4Box {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                match self {
                    $(
                        &MP4Box::$name(ref b) => Debug::fmt(b, f),
                    )+
                    &MP4Box::GenericBox(ref b) => Debug::fmt(b, f),
                }
            }
        }

        //
        // Now define the struct itself.
        //

        $(
            // Call def_box! if needed.
            def_boxes!(@DEF $name, $fourcc, $($block)*);

            // Implement FullBox automatically if possible.
            def_boxes!(@FULLBOX $name, [$($maxver)? $(,$deps)*]);

            // Implement BoxInfo trait for this struct.
            impl BoxInfo for $name {
                #[inline]
                fn fourcc(&self) -> FourCC {
                    FourCC::new($fourcc)
                }
                $(
                    #[inline]
                    fn max_version() -> Option<u8> {
                        Some($maxver)
                    }
                )?
            }

            // Implement BoxChildren trait for this struct.
            // def_boxes!(@CHILDREN $struct, $children);

        )+
    }
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
                    def_struct!(@filter_skip $field, dbg.field(stringify!($field), &self.$field););
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
