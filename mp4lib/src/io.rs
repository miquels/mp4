//! File read/write.
//!
use std::convert::TryInto;
use std::fs;
use std::io::{self, ErrorKind};
use std::os::unix::fs::FileExt;
use std::sync::Arc;

use memmap::{Mmap, MmapOptions};

use crate::serialize::{BoxBytes, FromBytes, ReadBytes, ToBytes, WriteBytes};
use crate::types::FourCC;

struct FileSegment {
    start: u64,
    len:   u64,
    map:   Mmap,
}

/// Reads a MP4 file.
///
/// Implements `ReadBytes`, so can be passed to `MP4::read`.
pub struct Mp4File {
    file:           Arc<fs::File>,
    pos:            u64,
    size:           u64,
    segments:       Vec<FileSegment>,
    input_filename: Option<String>,
}

impl Mp4File {
    /// Open an mp4 file.
    ///
    /// We use `mmap` to read the contents of the file, except for
    /// any `mdat` boxes. If you are processing a file that has a
    /// loy of `mdat`s interspersed with other boxes - say, a
    /// CMAF file, then set `mmap_all` to `true`. Otherwise, `false`.
    pub fn open(path: impl AsRef<str>, mmap_all: bool) -> io::Result<Mp4File> {
        let path = path.as_ref();
        let file = fs::File::open(path)?;
        let size = file.metadata()?.len();

        let mut segs = Vec::<(u64, u64)>::new();

        if mmap_all {
            // One big segment.
            segs.push((0, size));
        } else {
            // Create a list of segments where we leave out the
            // payload part of MDAT boxes.
            segs.push((0, 0));
            let mut pos = 0;
            while let Some((boxtype, boxpos, boxsize)) = next_box(&file, &mut pos, size)? {
                if &boxtype == b"mdat" {
                    segs.last_mut().unwrap().1 += 16;
                    segs.push((boxpos + boxsize, 0));
                } else {
                    segs.last_mut().unwrap().1 += boxsize;
                }
            }
        }

        // Now mmap those segments.
        let mut segments = Vec::new();
        for seg in &segs {
            if seg.1 == 0 {
                break;
            }
            let map = unsafe { MmapOptions::new().offset(seg.0).len(seg.1 as usize).map(&file)? };
            segments.push(FileSegment {
                start: seg.0,
                len: seg.1,
                map,
            });
        }

        Ok(Mp4File {
            segments,
            file: Arc::new(file),
            pos: 0,
            size,
            input_filename: Some(path.to_string()),
        })
    }

    /// Get a reference to the filehandle.
    pub fn file(self) -> Arc<fs::File> {
        self.file.clone()
    }

    #[inline]
    fn map(&self, amount: u64) -> io::Result<(usize, usize)> {
        //println!("XXX DBG map {}, {}", self.pos, amount);
        for idx in 0..self.segments.len() {
            let seg = &self.segments[idx];
            if self.pos >= seg.start && self.pos < seg.start + seg.len {
                if self.pos + amount > seg.start + seg.len {
                    return Err(io::Error::new(
                        ErrorKind::InvalidInput,
                        "tried to read over mapped segment boundary",
                    ));
                }
                let npos = (self.pos - seg.start) as usize;
                return Ok((idx, npos));
            }
        }
        Err(io::Error::new(
            ErrorKind::InvalidInput,
            "read request outside of any mapped segment",
        ))
    }
}

/// Locate the MOOV box.
fn next_box(file: &fs::File, pos: &mut u64, filesize: u64) -> io::Result<Option<([u8; 4], u64, u64)>> {
    if *pos + 15 >= filesize {
        return Ok(None);
    }
    let mut buf = [0u8; 16];
    file.read_exact_at(&mut buf[..], *pos)?;
    let boxtype = &buf[..4];
    let mut boxsize = u32::from_be_bytes(buf[4..8].try_into().unwrap()) as u64;
    if boxsize == 0 {
        boxsize = filesize - *pos;
    } else if boxsize == 1 {
        boxsize = u64::from_be_bytes(buf[8..16].try_into().unwrap());
    }
    let xpos = *pos;
    *pos += boxsize;
    Ok(Some((boxtype.try_into().unwrap(), xpos, boxsize)))
}

impl ReadBytes for Mp4File {
    #[inline]
    fn read(&mut self, amount: u64) -> io::Result<&[u8]> {
        let (seg, offset) = self.map(amount)?;
        self.pos += amount;
        Ok(&self.segments[seg].map[offset..offset + amount as usize])
    }

    #[inline]
    fn peek(&mut self, amount: u64) -> io::Result<&[u8]> {
        let (seg, offset) = self.map(amount)?;
        Ok(&self.segments[seg].map[offset..offset + amount as usize])
    }

    #[inline]
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        if self.pos + amount > self.size {
            return Err(io::Error::new(ErrorKind::UnexpectedEof, "tried to seek past eof"));
        }
        self.pos += amount;
        Ok(())
    }

    #[inline]
    fn left(&mut self) -> u64 {
        if self.pos > self.size {
            0
        } else {
            self.size - self.pos
        }
    }
}

impl BoxBytes for Mp4File {
    #[inline]
    fn pos(&mut self) -> u64 {
        self.pos
    }

    #[inline]
    fn seek(&mut self, pos: u64) -> io::Result<()> {
        if pos > self.size {
            return Err(io::Error::new(ErrorKind::UnexpectedEof, "tried to seek past eof"));
        }
        self.pos = pos;
        Ok(())
    }

    #[inline]
    fn size(&self) -> u64 {
        self.size
    }

    fn data_ref(&self, size: u64) -> io::Result<DataRef> {
        if self.pos + size > self.size {
            return Err(io::Error::new(ErrorKind::UnexpectedEof, "tried to seek past eof"));
        }
        Ok(DataRef {
            file:  self.file.clone(),
            start: self.pos as usize,
            end:   (self.pos + size) as usize,
        })
    }

    fn input_filename(&self) -> Option<&str> {
        self.input_filename.as_ref().map(|s| s.as_str())
    }
}

/// All boxes that are not `MediaDataBox` or `GenericBox` are `mmap`ed
/// into memory. The contents of `MediaDataBox` and `GenericBox` are
/// not, those are referened by this `DataRef`. Stuff in a `DataRef` uses
/// `read_at` and `write_at` to get at the data, rather than accessing
/// it through `mmap`.
///
/// This is done so that we don't have to `mmap` gigabytes of memory.
pub struct DataRef {
    pub(crate) file: Arc<fs::File>,
    start:           usize,
    end:             usize,
}

impl DataRef {
    // This is not the from_bytes from the FromBytes trait, it is
    // a direct method, because it has an extra data_size argument.
    pub(crate) fn from_bytes_limit<R: ReadBytes>(stream: &mut R, data_size: u64) -> io::Result<DataRef> {
        let data_ref = stream.data_ref(data_size)?;
        stream.skip(data_size)?;
        Ok(data_ref)
    }

    /// Number of items.
    pub fn len(&self) -> u64 {
        (self.end - self.start) as u64
    }

    /// Does it need a large box.
    pub fn is_large(&self) -> bool {
        self.len() > u32::MAX as u64 - 16
    }

    pub fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()> {
        self.file.read_exact_at(buf, offset + self.start as u64)
    }
}

impl FromBytes for DataRef {
    /// from_bytes for DataRef is actually not implemented.
    fn from_bytes<R: ReadBytes>(_stream: &mut R) -> io::Result<Self> {
        panic!("DataRef::from_bytes is not implemented- use DataRef::from_bytes_limit");
    }

    fn min_size() -> usize {
        0
    }
}

impl ToBytes for DataRef {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        if self.start == self.end {
            return Ok(());
        }

        let mut buf = Vec::new();
        buf.resize(std::cmp::min((self.end - self.start) as usize, 128000), 0);

        let mut pos = self.start;
        while pos < self.end {
            let to_read = std::cmp::min(buf.len(), self.end - pos);
            let nread = self.file.read_at(&mut buf[..to_read], pos as u64)?;
            if nread == 0 {
                return Err(io::Error::new(ErrorKind::UnexpectedEof, "Unexpected EOF"));
            }
            stream.write(&buf[..nread])?;
            pos += nread;
        }
        Ok(())
    }
}

impl Default for DataRef {
    fn default() -> Self {
        let devzero = fs::File::open("/dev/zero").unwrap();
        DataRef {
            file:  Arc::new(devzero),
            start: 0,
            end:   0,
        }
    }
}

impl Clone for DataRef {
    fn clone(&self) -> Self {
        DataRef {
            file:  self.file.clone(),
            start: self.start,
            end:   self.end,
        }
    }
}

impl std::fmt::Debug for DataRef {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "DataRef{{ start: {}, end: {} }}", self.start, self.end)
    }
}

// Count bytes, don't actually write.
#[derive(Debug, Default)]
pub(crate) struct CountBytes {
    pos: usize,
    max: usize,
}

impl CountBytes {
    pub fn new() -> CountBytes {
        CountBytes { pos: 0, max: 0 }
    }
}

impl WriteBytes for CountBytes {
    fn write(&mut self, newdata: &[u8]) -> io::Result<()> {
        self.pos += newdata.len();
        if self.max < self.pos {
            self.max = self.pos;
        }
        Ok(())
    }

    fn skip(&mut self, amount: u64) -> io::Result<()> {
        self.pos += amount as usize;
        Ok(())
    }
}

impl BoxBytes for CountBytes {
    fn pos(&mut self) -> u64 {
        self.pos as u64
    }
    fn seek(&mut self, pos: u64) -> io::Result<()> {
        self.pos = pos as usize;
        Ok(())
    }
    fn size(&self) -> u64 {
        self.max as u64
    }
}

#[cfg(feature = "streaming")]
mod membuffer {
    use super::*;

    /// Memory buffer that implements WriteBytes.
    #[derive(Debug, Default)]
    pub(crate) struct MemBuffer {
        data: Vec<u8>,
        pos:  usize,
    }

    impl MemBuffer {
        pub fn new() -> MemBuffer {
            MemBuffer {
                data: Vec::new(),
                pos:  0,
            }
        }

        pub fn into_vec(self) -> Vec<u8> {
            self.data
        }
    }

    impl WriteBytes for MemBuffer {
        fn write(&mut self, newdata: &[u8]) -> io::Result<()> {
            let mut newdata = newdata;
            if self.pos < self.data.len() {
                let len = std::cmp::min(self.data.len() - self.pos, newdata.len());
                self.data[self.pos..self.pos + len].copy_from_slice(&newdata[..len]);
                newdata = &newdata[len..];
                self.pos += len;
            }
            if newdata.len() > 0 {
                self.data.extend_from_slice(newdata);
                self.pos = self.data.len();
            }
            Ok(())
        }

        fn skip(&mut self, amount: u64) -> io::Result<()> {
            self.pos += amount as usize;
            if self.pos > self.data.len() {
                self.data.resize(self.pos, 0);
            }
            Ok(())
        }
    }

    impl BoxBytes for MemBuffer {
        fn pos(&mut self) -> u64 {
            self.pos as u64
        }
        fn seek(&mut self, pos: u64) -> io::Result<()> {
            self.pos = pos as usize;
            if self.pos > self.data.len() {
                self.data.resize(self.pos, 0);
            }
            Ok(())
        }
        fn size(&self) -> u64 {
            self.data.len() as u64
        }
    }
}
#[cfg(feature = "streaming")]
pub(crate) use membuffer::*;

impl<'a, B: ?Sized + ReadBytes + 'a> ReadBytes for Box<B> {
    fn read(&mut self, amount: u64) -> io::Result<&[u8]> {
        B::read(&mut *self, amount)
    }
    fn peek(&mut self, amount: u64) -> io::Result<&[u8]> {
        B::peek(&mut *self, amount)
    }
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        B::skip(&mut *self, amount)
    }
    fn left(&mut self) -> u64 {
        B::left(&mut *self)
    }
}

impl<'a, B: ?Sized + WriteBytes + 'a> WriteBytes for Box<B> {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        B::write(&mut *self, data)
    }
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        B::skip(&mut *self, amount)
    }
}

impl<'a, B: ?Sized + BoxBytes + 'a> BoxBytes for Box<B> {
    fn pos(&mut self) -> u64 {
        B::pos(&mut *self)
    }
    fn seek(&mut self, pos: u64) -> io::Result<()> {
        B::seek(&mut *self, pos)
    }
    fn size(&self) -> u64 {
        B::size(&*self)
    }
    fn version(&self) -> u8 {
        B::version(&*self)
    }
    fn flags(&self) -> u32 {
        B::flags(&*self)
    }
    fn fourcc(&self) -> FourCC {
        B::fourcc(&*self)
    }
    fn data_ref(&self, size: u64) -> io::Result<DataRef> {
        B::data_ref(&*self, size)
    }
    fn input_filename(&self) -> Option<&str> {
        B::input_filename(&*self)
    }
}
