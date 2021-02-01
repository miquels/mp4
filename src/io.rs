use std::fs;
use std::io::{self, ErrorKind};
use std::sync::Arc;

use memmap::{Mmap, MmapOptions};

use crate::serialize::{BoxBytes, ReadBytes, WriteBytes, ToBytes};
use crate::types::FourCC;

pub struct Mp4File {
    mmap:  Arc<Mmap>,
    file:  fs::File,
    pos:   u64,
    size:  u64,
}

/*
pub enum ByteData {
    Mmap {
        Arc<Mmap>),
        offset: u64,
        len:    u64,
    },
    Vec(Arc<Vec<u8>>),
}

pub struct Chunk {
    data:       ByteData,
    is_mdat:    bool,
}

pub struct MdatSource {
    file:   fs::File,
    offset: u64,
    len:    u64,
}
*/

impl Mp4File {
    pub fn open(path: impl AsRef<str>) -> io::Result<Mp4File> {
        let path = path.as_ref();
        let file = fs::File::open(path)?;
        let size = file.metadata()?.len();
        let mmap = unsafe { MmapOptions::new().map(&file)? };
        Ok(Mp4File {
            mmap: Arc::new(mmap),
            file,
            pos: 0,
            size,
        })
    }

    pub fn into_inner(self) -> fs::File {
        self.file
    }
}

impl ReadBytes for Mp4File
{
    #[inline]
    fn read(&mut self, amount: u64) -> io::Result<&[u8]> {
        //println!("XXX DBG read {}", amount);
        if self.pos + amount > self.size {
            return Err(io::Error::new(ErrorKind::UnexpectedEof, "tried to read past eof"));
        }
        let pos = self.pos as usize;
        self.pos += amount;
        Ok(&self.mmap[pos..pos+amount as usize])
    }

    #[inline]
    fn peek(&mut self, amount: u64) -> io::Result<&[u8]> {
        //println!("XXX DBG peek {}", amount);
        if self.pos + amount > self.size {
            return Err(io::Error::new(ErrorKind::UnexpectedEof, "tried to read past eof"));
        }
        let pos = self.pos as usize;
        Ok(&self.mmap[pos..pos+amount as usize])
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

impl BoxBytes for Mp4File
{
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
            mmap: self.mmap.clone(),
            start: self.pos as usize,
            end: (self.pos + size) as usize,
        })
    }
}

/// reference to a chunk of data somewhere in the source file.
pub struct DataRef {
    mmap:  Arc<Mmap>,
    start: usize,
    end: usize,
}

impl DataRef {
    // from_bytes is a bit different.
    pub(crate) fn from_bytes<R: ReadBytes>(stream: &mut R, data_size: u64) -> io::Result<DataRef> {
        let data_ref = stream.data_ref(data_size)?;
        stream.skip(data_size)?;
        Ok(data_ref)
    }

    pub(crate) fn len(&self) -> u64 {
        (self.end - self.start) as u64
    }

    pub(crate) fn is_large(&self) -> bool {
        self.len() > u32::MAX as u64 - 16
    }
}

impl ToBytes for DataRef {
    // writing is simple.
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        stream.write(&self[..])
    }
}

// deref to &[u8]
impl std::ops::Deref for DataRef {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.mmap[self.start..self.end]
    }
}

impl std::fmt::Debug for DataRef {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{{ &file[{}..{}] }}", self.start, self.end)
    }
}

// Count bytes, don't actually write.
#[derive(Debug, Default)]
pub(crate) struct CountBytes {
    pos:    usize,
    max:    usize,
}

impl CountBytes {
    pub fn new() -> CountBytes {
        CountBytes {
            pos: 0,
            max: 0,
        }
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
}

