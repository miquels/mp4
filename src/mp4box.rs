
use std::fmt::Debug;
use std::io;

use crate::boxes::MP4Box;
use crate::fromtobytes::{FromBytes, ToBytes, ReadBytes, WriteBytes, BoxBytes};
use crate::types::*;

/// Gets implemented for every box.
pub trait BoxInfo {
    /// The "fourcc" name of this box.
    fn fourcc(&self) -> FourCC;
    /// Alignment of this box (as per spec)
    fn alignment(&self) -> usize;
    /// Sub-boxes if this is a container.
    fn boxes(&self) -> Option<&[MP4Box]> {
        None
    }
}

/// Headers + Content = IsoBox.
pub struct IsoBox<C> {
    fourcc: FourCC,
    content: C,
}

impl<C> IsoBox<C> where C: FromBytes + ToBytes + BoxInfo {
    /// Wrap a struct with a box header.
    pub fn wrap(content: C) -> IsoBox<C> {
        IsoBox {
            fourcc: content.fourcc().clone(),
            content,
        }
    }
}

// Define FromBytes trait for IsoBox
impl<C> FromBytes for IsoBox<C> where C: FromBytes + BoxInfo {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<IsoBox<C>> {

        // Read the header.
        let mut reader = BoxReader::new(stream)?;

        // Read the body.
        let content = C::from_bytes(&mut reader)?;

        Ok(IsoBox {
            fourcc: reader.fourcc,
            content
        })
    }

    fn min_size() -> usize {
        8 + C::min_size()
    }
}

// Define ToBytes trait for IsoBox
impl<C> ToBytes for IsoBox<C> where C: ToBytes + BoxInfo {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let mut writer = BoxWriter::new(stream, self.content.fourcc())?;
        writer.set_fourcc(writer.fourcc.clone());
        self.content.to_bytes(&mut writer)?;
        writer.finalize()
    }
}

// Define BoxInfo trait for the enum.
impl<C> BoxInfo for IsoBox<C> where C: BoxInfo {
    #[inline]
    fn fourcc(&self) -> FourCC {
        self.fourcc.clone()
    }
    #[inline]
    fn alignment(&self) -> usize {
        self.content.alignment()
    }
}

impl<C> Debug for IsoBox<C> where C: Debug {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        Debug::fmt(&self.content, f)
    }
}

//
//
// Helpers to read and write the box header.
//
//

/// Reads the box header.
pub struct BoxReader<'a> {
    maxsize: u64,
    prev_version: u8,
    // We box it, since a BoxReader might contain a BoxReader.
    inner: Box<dyn ReadBytes + 'a>,
    pub fourcc: FourCC,
}

impl<'a> BoxReader<'a> {
    /// Read the box header, then return a size-limited reader.
    pub fn new(mut stream: &'a mut impl ReadBytes) -> io::Result<BoxReader<'a>> {

        let size1 = u32::from_bytes(&mut stream)?;
        let fourcc = FourCC::from_bytes(&mut stream)?;
        let size = match size1 {
            0 => stream.size() - stream.pos(),
            1 => u64::from_bytes(&mut stream)?.saturating_sub(16),
            x => x.saturating_sub(8) as u64,
        };

        let maxsize = std::cmp::min(stream.size(), stream.pos() + size);
        debug!("XXX here {} size {}, size1 {} maxsize {} left {}", fourcc, size, size1, maxsize, stream.left());
        Ok(BoxReader {
            prev_version: stream.version(),
            maxsize,
            inner: Box::new(stream),
            fourcc,
        })
    }
}

impl <'a> Drop for BoxReader<'a> {
    fn drop(&mut self) {
        if self.pos() < self.maxsize {
            debug!("XXX BoxReader {} drop: skipping {}", self.fourcc, self.maxsize - self.pos());
            let _ = self.skip(self.maxsize - self.pos());
        }
        if self.inner.version() != self.prev_version {
            self.inner.set_version(self.prev_version);
        }
    }
}

// Delegate ReadBytes to the inner reader.
impl<'a> ReadBytes for BoxReader<'a> {
    fn read(&mut self, amount: u64) -> io::Result<&[u8]> {
        if amount == 0 {
            debug!("XXX self reader for {} amount 0 left {}", self.fourcc, self.left());
        }
        let amount = if amount == 0 {
            self.left()
        } else {
            amount
        };
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
        self.inner.version()
    }
    fn set_version(&mut self, version: u8) {
        self.inner.set_version(version)
    }
    fn fourcc(&self) -> FourCC {
        self.fourcc.clone()
    }
    fn set_fourcc(&mut self, fourcc: FourCC) {
        self.fourcc = fourcc;
    }
}

/// Writes the box header.
pub struct BoxWriter<W: WriteBytes> {
    fourcc: FourCC,
    offset: u64,
    inner: W,
    finalized: bool,
}

impl<W> BoxWriter<W> where W: WriteBytes {
    /// Write a provisional box header, then return a new stream. When
    /// the stream is dropped, the box header is updated.
    pub fn new(mut stream: W, fourcc: FourCC) -> io::Result<BoxWriter<W>> {
        let offset = stream.pos();
        0u32.to_bytes(&mut stream)?;
        fourcc.to_bytes(&mut stream)?;
        Ok(BoxWriter{
            fourcc,
            offset,
            inner: stream,
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
        let sz = pos - self.offset;
        sz.to_bytes(&mut self.inner)?;
        Ok(())
    }
}

impl<W> Drop for BoxWriter<W> where W: WriteBytes {
    fn drop(&mut self) {
        self.finalize().unwrap();
    }
}

// Delegate WriteBytes to the inner writer.
impl<W> WriteBytes for BoxWriter<W> where W: WriteBytes {
    fn write(&mut self, data: &[u8]) -> io::Result<()> { self.inner.write(data) }
    fn skip(&mut self, amount: u64) -> io::Result<()> { self.inner.skip(amount) }
}

// Delegate BoxBytes to the inner writer.
impl<W> BoxBytes for BoxWriter<W> where W: WriteBytes {
    fn pos(&self) -> u64 { self.inner.pos() }
    fn seek(&mut self, pos: u64) -> io::Result<()> { self.inner.seek(pos) }
    fn version(&self) -> u8 { self.inner.version() }
    fn set_version(&mut self, version: u8) { self.inner.set_version(version) }
    fn fourcc(&self) -> FourCC { self.inner.fourcc() }
    fn set_fourcc(&mut self, fourcc: FourCC) { self.inner.set_fourcc(fourcc) }
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

//
//
// Helper types.
//
//

/// Any unknown boxes we encounted are put into a GenericBox.
pub struct GenericBox {
    fourcc: FourCC,
    data: Vec<u8>,
    size: u64,
    skip: bool,
}

impl FromBytes for GenericBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<GenericBox> {
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
        stream.write(&self.data)
    }
}

impl BoxInfo for GenericBox {
    #[inline]
    fn fourcc(&self) -> FourCC {
        self.fourcc.clone()
    }
    #[inline]
    fn alignment(&self) -> usize {
        0
    }
}

struct U8Array(u64);

impl Debug for U8Array {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[u8; {}]", &self.0)
    }
}

impl Debug for GenericBox {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut dbg = f.debug_struct("Box");
        dbg.field("fourcc", &self.fourcc);
        dbg.field("data", &U8Array(self.size));
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

    ($($name:ident, $fourcc:expr, $align:expr $(=> $block:tt)? ; )+) => {

        //
        // First define the enum.
        //

        /// All the boxes we know.
        #[derive(Debug)]
        pub enum MP4Box {
            $(
                $name($name),
            )+
            GenericBox(GenericBox),
        }

        // Define FromBytes trait for the enum.
        impl FromBytes for MP4Box {
            fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<MP4Box> {

                // Read the header.
                let mut reader = BoxReader::new(stream)?;
                debug!("XXX got reader for {:?} left {}", reader.fourcc, reader.left());

                // Read the body.
                let b = reader.fourcc.to_be_bytes();
                let e = match std::str::from_utf8(&b[..]).unwrap_or("") {
                    $(
                        $fourcc => MP4Box::$name($name::from_bytes(&mut reader)?),
                    )+
                    _ => MP4Box::GenericBox(GenericBox::from_bytes(&mut reader)?),
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
            #[inline]
            fn alignment(&self) -> usize {
                match self {
                    $(
                        &MP4Box::$name(ref b) => b.alignment(),
                    )+
                    &MP4Box::GenericBox(ref b) => b.alignment(),
                }
            }
        }

        //
        // Now define the struct itself.
        //

        $(
            // Call def_box! if needed.
            def_boxes!(@DEF $name, $fourcc, $($block)*);

            // Implement BoxInfo trait for this struct.
            impl BoxInfo for $name {
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

        impl FromBytes for $name {
            #[allow(unused_variables)]
            fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<$name> {

                debug!("XXX frombyting {}", stringify!($name));

                // Deserialize.
                let r: io::Result<$name> = {
                    def_struct!(@from_bytes $name, [], stream, $(
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
                let mut stream = BoxWriter::new(stream, self.fourcc())?;
                let stream = &mut stream;

                // Serialize.
                def_struct!(@to_bytes self, stream, $(
                    $field: $type $(as $as)?,
                )*)
            }
        }
    };
}
