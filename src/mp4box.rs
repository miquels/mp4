//! Traits and `MP4` struct.
use std::fmt::Debug;
use std::io;

use crate::boxes::{MovieBox, FileTypeBox};
use crate::io::DataRef;
use crate::serialize::{BoxBytes, FromBytes, ReadBytes, ToBytes, WriteBytes};
use crate::types::*;

pub use crate::boxes::MP4Box;

/// Gets implemented for every box.
pub trait BoxInfo {
    const FOURCC: &'static str = "xxxx";

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
pub(crate) struct BoxHeader {
    pub(crate) size:        u64,
    pub(crate) fourcc:      FourCC,
    pub(crate) version:     Option<u8>,
    pub(crate) flags:       u32,
    pub(crate) max_version: Option<u8>,
}

impl BoxHeader {
    pub(crate) fn read(stream: &mut impl ReadBytes) -> io::Result<BoxHeader> {
        let size1 = u32::from_bytes(stream)?;
        let fourcc = FourCC::from_bytes(stream)?;
        let mut size = match size1 {
            0 => stream.size() - stream.pos(),
            1 => u64::from_bytes(stream)?.saturating_sub(16),
            x => x.saturating_sub(8) as u64,
        };

        let max_version = MP4Box::max_version_from_fourcc(fourcc.clone());
        let mut version = None;
        let mut flags = 0;
        if max_version.is_some() {
            version = Some(u8::from_bytes(stream)?);
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
        b
    }

    pub(crate) fn peek(stream: &mut impl ReadBytes) -> io::Result<BoxHeader> {
        let size = stream.left();
        let mut data = stream.peek(size)?;
        BoxHeader::read(&mut data)
    }

    pub(crate) fn read_base(stream: &mut impl ReadBytes) -> io::Result<BoxHeader> {
        let size1 = u32::from_bytes(stream)?;
        let fourcc = FourCC::from_bytes(stream)?;
        let size = match size1 {
            0 => stream.size() - stream.pos(),
            1 => u64::from_bytes(stream)?.saturating_sub(16),
            x => x.saturating_sub(8) as u64,
        };

        Ok(BoxHeader {
            size,
            fourcc,
            version: None,
            flags: 0,
            max_version: None,
        })
    }

    pub(crate) fn read_full(mut stream: &mut impl ReadBytes, header: &mut BoxHeader) -> io::Result<()> {
        header.version = Some(u8::from_bytes(&mut stream)?);
        let data = stream.read(3)?;
        let mut buf = [0u8; 4];
        (&mut buf[1..]).copy_from_slice(&data);
        header.flags = u32::from_be_bytes(buf);
        header.size -= 4;
        Ok(())
    }
}

/// Limited reader that reads no further than the box size.
pub(crate) struct BoxReader<'a> {
    pub(crate) header: BoxHeader,
    maxsize:           u64,
    pos:               u64,
    inner:             &'a mut dyn ReadBytes,
}

impl<'a> BoxReader<'a> {
    /// Read the box header, then return a size-limited reader.
    pub fn new(stream: &'a mut impl ReadBytes) -> io::Result<BoxReader<'a>> {
        let header = BoxHeader::read(stream)?;
        let maxsize = std::cmp::min(stream.size(), stream.pos() + header.size);
        log::trace!(
            "XXX header {:?} maxsize {} left {}",
            header,
            maxsize,
            stream.left()
        );
        Ok(BoxReader {
            header,
            maxsize,
            pos: stream.pos(),
            inner: stream,
        })
    }
}

impl Drop for BoxReader<'_> {
    fn drop(&mut self) {
        if self.pos < self.maxsize {
            log::trace!(
                "XXX BoxReader {} drop: skipping {}",
                self.header.fourcc,
                self.maxsize - self.pos
            );
            let _ = self.skip(self.maxsize - self.pos);
        }
    }
}

// Delegate ReadBytes to the inner reader.
impl ReadBytes for BoxReader<'_> {
    #[inline]
    fn read(&mut self, amount: u64) -> io::Result<&[u8]> {
        if self.pos + amount > self.maxsize {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let res = self.inner.read(amount)?;
        self.pos += amount;
        Ok(res)
    }
    #[inline]
    fn peek(&mut self, amount: u64) -> io::Result<&[u8]> {
        if self.pos + amount > self.maxsize {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.inner.peek(amount)
    }
    #[inline]
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        if self.pos + amount > self.maxsize {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.inner.skip(amount)?;
        self.pos += amount;
        Ok(())
    }
    #[inline]
    fn left(&mut self) -> u64 {
        if self.pos > self.maxsize {
            0
        } else {
            self.maxsize - self.pos
        }
    }
}

// Delegate BoxBytes to the inner reader.
impl BoxBytes for BoxReader<'_> {
    #[inline]
    fn pos(&mut self) -> u64 {
        self.pos
    }
    #[inline]
    fn seek(&mut self, pos: u64) -> io::Result<()> {
        if pos > self.maxsize {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.inner.seek(pos)?;
        self.pos = pos;
        Ok(())
    }
    #[inline]
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
    fn data_ref(&self, size: u64) -> io::Result<DataRef> {
        self.inner.data_ref(size)
    }
}

/// Writes the box header.
pub(crate) struct BoxWriter<'a> {
    offset:    u64,
    vflags:    u32,
    finalized: bool,
    inner:     Box<dyn WriteBytes + 'a>,
}

impl<'a> BoxWriter<'a> {
    /// Write a provisional box header, then return a new stream. When
    /// the stream is dropped, the box header is updated.
    pub fn new<B>(stream: &'a mut impl WriteBytes, boxinfo: &B) -> io::Result<BoxWriter<'a>>
    where
        B: BoxInfo + FullBox,
    {
        let offset = stream.pos();
        0u32.to_bytes(stream)?;
        boxinfo.fourcc().to_bytes(stream)?;
        let mut vflags = 0;
        if B::max_version().is_some() {
            let version = boxinfo.version().unwrap_or(0) as u32;
            vflags = version << 24 | boxinfo.flags();
            vflags.to_bytes(stream)?;
        }
        Ok(BoxWriter {
            offset,
            vflags,
            finalized: false,
            inner: Box::new(stream),
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
    fn pos(&mut self) -> u64 {
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
    fn data_ref(&self, size: u64) -> io::Result<DataRef> {
        self.inner.data_ref(size)
    }
}

/// Main entry point for ISOBMFF box structure.
pub struct MP4 {
    /// The boxes at the top level.
    pub boxes:  Vec<MP4Box>,
    pub(crate) data_ref:   DataRef,
    pub(crate) input_file: Option<String>,
}

impl Debug for MP4 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut dbg = f.debug_struct("MP4");
        dbg.field("boxes", &self.boxes);
        dbg.finish()
    }
}

impl MP4 {
    /// Read a ISOBMFF box structure into memory.
    pub fn read<R: ReadBytes>(file: R) -> io::Result<MP4> {
        let data_ref = file.data_ref(file.size())?;
        let input_file = file.input_filename().map(|s| s.to_string());
        let boxes = read_boxes(file)?;
        let mut mp4 = MP4{ boxes, data_ref, input_file };
        mp4.insert_file_type_box();
        Ok(mp4)
    }

    /// Write a ISOBMFF box structure to a file.
    pub fn write<W: WriteBytes>(&self, file: W) -> io::Result<()> {
        write_boxes(file, &self.boxes)
    }

    /// Get a reference to the MovieBox.
    pub fn movie(&self) -> &MovieBox {
        first_box!(&self.boxes, MovieBox).unwrap()
    }

    /// Get a mutable reference to the MovieBox.
    pub fn movie_mut(&mut self) -> &mut MovieBox {
        first_box_mut!(&mut self.boxes, MovieBox).unwrap()
    }

    /// Get a reference to the FileTypeBox.
    pub fn file_type(&self) -> &FileTypeBox {
        first_box!(&self.boxes, FileTypeBox).unwrap()
    }

    /// Get a mutable reference to the FileTypeBox.
    pub fn file_type_mut(&mut self) -> &mut FileTypeBox {
        first_box_mut!(&mut self.boxes, FileTypeBox).unwrap()
    }

    /// Check if the structure of the file is valid and contains all
    /// the primary boxes.
    pub fn is_valid(&self) -> bool {
        let mut valid = true;
        match first_box!(&self.boxes, MovieBox) {
            Some(m) => {
                if !m.is_valid() {
                    valid = false;
                }
            },
            None => {
                log::error!("no MovieBox present");
                valid = false;
            }
        }
        valid
    }

    pub(crate) fn data_ref(&self, offset: u64, len: u64) -> &[u8] {
        &self.data_ref[offset as usize .. (offset + len) as usize]
    }

    pub(crate) fn insert_file_type_box(&mut self) {
        if first_box!(&self.boxes, FileTypeBox).is_some() {
            return;
        }
        let ftype = FileTypeBox {
            major_brand:    FourCC::new("mp41"),
            minor_version:  0,
            compatible_brands: vec![ FourCC::new("mp41") ],
        };
        self.boxes.insert(0, MP4Box::FileTypeBox(ftype));
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

/// Any unknown boxes we encounter are put into a GenericBox.
pub struct GenericBox {
    fourcc: FourCC,
    data:   Option<Vec<u8>>,
    data_ref:   Option<DataRef>,
    size:   u64,
}

impl FromBytes for GenericBox {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<GenericBox> {
        let mut reader = BoxReader::new(stream)?;
        let stream = &mut reader;

        let size = stream.left();
        let mut data = None;
        let mut data_ref = None;
        if size == 0 {
            data = Some(vec![]);
        } else if size < 65536 {
            data = Some(stream.read(size)?.to_vec());
        } else {
            data_ref = Some(DataRef::from_bytes(stream, size)?);
        }
        Ok(GenericBox {
            fourcc: stream.fourcc(),
            data,
            data_ref,
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
        if let Some(ref data) = self.data {
            writer.write(data)?;
        }
        if let Some(ref data_ref) = self.data_ref {
            data_ref.to_bytes(&mut writer)?;
        }
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
        if let Some(ref _data) = self.data {
            let data = format!("[u8; {}]", self.size);
            dbg.field("data", &data);
        }
        if let Some(ref data_ref) = self.data_ref {
            let data = format!("[u8; {}]", data_ref.len());
            dbg.field("data", &data);
        }
        dbg.finish()
    }
}

