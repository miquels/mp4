use std::convert::TryInto;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};

use crate::fromtobytes::{BoxBytes, ReadBytes, WriteBytes};
use crate::types::FourCC;

pub struct Mp4File {
    file: File,
    pos: u64,
    size: u64,
    buf: Vec<u8>,
    version: u8,
    fourcc: FourCC,
}

impl Mp4File {
    pub fn new(file: File) -> Mp4File {
        let mut file = file;
        let pos = file.seek(SeekFrom::Current(0)).unwrap();
        let meta = file.metadata().unwrap();
        Mp4File {
            file,
            pos,
            size: meta.len(),
            buf: Vec::new(),
            version: 0,
            fourcc: FourCC(0),
        }
    }
}

impl ReadBytes for Mp4File {

    fn read(&mut self, amount: u64) -> io::Result<&[u8]> {
        println!("XXX - read {} at pos {}", amount, self.pos);
        let amount = amount as usize;
        if amount == 0 {
            return Ok(b"");
        }
        if amount > 4096 {
            return Err(io::Error::new(io::ErrorKind::Other, format!("MP4File::read({}): too large", amount)));
        }
        if self.buf.len() < amount {
            self.buf.resize(amount, 0);
        }
        self.file.read_exact(&mut self.buf[..amount])?;
        self.pos += amount as u64;
        Ok(&self.buf[..amount])
    }

    fn skip(&mut self, amount: u64) -> io::Result<()> {
        self.pos += amount;
        self.file.seek(SeekFrom::Start(self.pos))?;
        Ok(())
    }

    fn left(&self) -> u64 {
        if self.pos > self.size {
            0
        } else {
            self.size - self.pos
        }
    }
}

impl WriteBytes for Mp4File {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.file.write_all(data)?;
        self.pos += data.len() as u64;
        Ok(())
    }
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        self.pos += amount;
        self.file.seek(SeekFrom::Start(self.pos))?;
        Ok(())
    }
}
impl BoxBytes for Mp4File {
    fn pos(&self) -> u64 {
        self.pos
    }
    fn seek(&mut self, pos: u64) -> io::Result<()> {
        let ipos = pos.try_into().unwrap();
        self.file.seek(SeekFrom::Current(ipos))?;
        self.pos = pos;
        Ok(())
    }
    fn size(&self) -> u64 {
        self.size
    }
    fn version(&self) -> u8 {
        self.version
    }
    fn set_version(&mut self, version: u8) {
        self.version = version;
    }
    fn fourcc(&self) -> FourCC {
        self.fourcc.clone()
    }
    fn set_fourcc(&mut self, fourcc: FourCC) {
        self.fourcc = fourcc;
    }
    fn limit(&mut self, limit: u64) -> Box<dyn ReadBytes + '_> {
        Box::new(ReadBytesLimit::new(self, limit))
    }
}

pub struct ReadBytesLimit<R: ReadBytes> {
    inner: R,
    maxsize: u64,
    prev_version:    u8,
}

impl<R> ReadBytesLimit<R> where R: ReadBytes {
    pub fn new(file: R, limit: u64) -> ReadBytesLimit<R> {
        let prev_version = file.version();
        let maxsize = file.pos() + limit;
        ReadBytesLimit { inner: file, maxsize, prev_version }
    }
}

impl<R> Drop for ReadBytesLimit<R> where R: ReadBytes {
    fn drop(&mut self) {
        if self.inner.version() != self.prev_version {
            self.inner.set_version(self.prev_version);
        }
    }
}

impl<R> ReadBytes for ReadBytesLimit<R> where R: ReadBytes {

    fn read(&mut self, amount: u64) -> io::Result<&[u8]> {
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
        if self.inner.pos() > self.maxsize {
            0
        } else {
            self.maxsize - self.inner.pos()
        }
    }
}

impl<R> BoxBytes for ReadBytesLimit<R> where R: ReadBytes {
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
        self.inner.version()
    }
    fn set_version(&mut self, version: u8) {
        self.inner.set_version(version);
    }
    fn fourcc(&self) -> FourCC {
        self.inner.fourcc()
    }
    fn set_fourcc(&mut self, fourcc: FourCC) {
        self.inner.set_fourcc(fourcc);
    }
    fn limit(&mut self, limit: u64) -> Box<dyn ReadBytes + '_> {
        self.inner.limit(limit)
    }
}

impl<'a, B: ?Sized + ReadBytes + 'a> ReadBytes for Box<B> {
    fn read(&mut self, amount: u64) -> io::Result<&[u8]> { B::read(&mut *self, amount) }
    fn skip(&mut self, amount: u64) -> io::Result<()> { B::skip(&mut *self, amount) }
    fn left(&self) -> u64 { B::left(&*self) }
}

impl<'a, B: ?Sized + BoxBytes + 'a> BoxBytes for Box<B> {
    fn pos(&self) -> u64 { B::pos(&*self) }
    fn seek(&mut self, pos: u64) -> io::Result<()> { B::seek(&mut *self, pos) }
    fn size(&self) -> u64 { B::size(&*self) }
    fn version(&self) -> u8 { B::version(&*self) }
    fn set_version(&mut self, version: u8) { B::set_version(&mut *self, version) }
    fn fourcc(&self) -> FourCC { B::fourcc(&*self) }
    fn set_fourcc(&mut self, fourcc: FourCC) { B::set_fourcc(&mut *self, fourcc) }
    fn limit(&mut self, limit: u64) -> Box<dyn ReadBytes + '_> { B::limit(&mut *self, limit) }
}

