use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::FileExt;

use crate::serialize::{BoxBytes, ReadBytes, WriteBytes};
use crate::types::FourCC;

const MIN_BUFSIZE: usize = 4096;
const SEEK_BUFSIZE: usize = 4096;
const BUFSIZE: usize = 65536;

pub trait ReadAt: Read + FileExt {}
impl<T> ReadAt for T where T: Read + FileExt {}

pub struct Mp4File<F> {
    file:  Box<F>,
    pos:   u64,
    size:  u64,
    buf:   Vec<u8>,
    rdpos: usize,
    wrpos: usize,
}

impl<F> Mp4File<F> {
    pub fn new(file: F) -> Mp4File<F>
    where
        F: Seek,
    {
        let mut file = file;
        let pos = file.seek(SeekFrom::Current(0)).unwrap();
        let size = file.seek(SeekFrom::End(0)).unwrap();

        let mut buf = Vec::new();
        buf.resize(BUFSIZE + MIN_BUFSIZE + SEEK_BUFSIZE, 0);

        file.seek(SeekFrom::Start(pos)).unwrap();

        Mp4File {
            file: Box::new(file),
            pos,
            size,
            buf,
            rdpos: 0,
            wrpos: 0,
        }
    }
}

impl<F> Mp4File<F>
where
    F: Read,
{
    // Make sure at least "amount" is buffered.
    fn fill_buf(&mut self, amount: usize) -> io::Result<()> {
        if self.wrpos - self.rdpos >= amount {
            return Ok(());
        }

        // need to be able to store "amount", if not make space.
        let left = self.buf.len() - self.rdpos;
        debug!("Mp4File::fill_buf: left {}", left);
        if left < std::cmp::min(amount, MIN_BUFSIZE) {
            // copy everything down to the start of the buffer.
            debug!("Mp4File::fill_buf: 1. rdpos {} wrpos {}", self.rdpos, self.wrpos);
            let extra = std::cmp::min(self.rdpos, SEEK_BUFSIZE);
            let len = self.wrpos - self.rdpos;
            let pos = self.rdpos - extra;
            self.buf.copy_within(pos..self.wrpos, 0);
            self.rdpos = extra;
            self.wrpos = extra + len;
            debug!("Mp4File::fill_buf: 2. rdpos {} wrpos {}", self.rdpos, self.wrpos);

            // make _sure_ we have enough free space now.
            assert!(self.buf.len() - self.rdpos >= amount);
        }

        // Now fill the buffer.
        while self.wrpos - self.rdpos < amount {
            let n = self.file.read(&mut self.buf[self.wrpos..])?;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            self.wrpos += n;
        }
        Ok(())
    }
}

impl<F> ReadBytes for Mp4File<F>
where
    F: Read + Seek,
{
    fn read(&mut self, amount: u64) -> io::Result<&[u8]> {
        let amount = if amount == 0 { self.left() } else { amount } as usize;
        //debug!("XXX - read {} at pos {}", amount, self.pos);
        if amount == 0 {
            return Ok(b"");
        }
        if amount > BUFSIZE {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Mp4File::read({}): too large", amount),
            ));
        }
        self.fill_buf(amount as usize)?;
        self.pos += amount as u64;
        let rdpos = self.rdpos;
        self.rdpos += amount;
        Ok(&self.buf[rdpos..rdpos + amount])
    }

    fn skip(&mut self, amount: u64) -> io::Result<()> {
        self.seek(self.pos + amount)
    }

    #[inline]
    fn left(&self) -> u64 {
        if self.pos > self.size {
            0
        } else {
            self.size - self.pos
        }
    }
}

impl<F> WriteBytes for Mp4File<F>
where
    F: Write + Seek,
{
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        assert!(self.wrpos == 0);
        self.file.write_all(data)?;
        self.pos += data.len() as u64;
        Ok(())
    }
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        if amount < 4096 {
            self.pos += amount;
            let buf = [0u8; 4096];
            self.file.write_all(&buf[..amount as usize])
        } else {
            self.seek(self.pos + amount)
        }
    }
}

impl<F> BoxBytes for Mp4File<F>
where
    F: Seek,
{
    fn pos(&self) -> u64 {
        self.pos
    }
    fn seek(&mut self, pos: u64) -> io::Result<()> {
        // see if it fits in the read buffer.
        let bpos = self.pos - (self.rdpos as u64);
        let epos = self.pos + ((self.wrpos - self.rdpos) as u64);
        if pos >= bpos && pos < epos {
            debug!("Mp4File::seek {} in buffer", pos);
            self.rdpos = (pos - bpos) as usize;
            self.pos = pos;
            return Ok(());
        }
        debug!("Mp4File::seek {} NOT in buffer", pos);

        // Nope, seek and invalidate buffer.
        self.file.seek(SeekFrom::Start(pos))?;
        self.pos = pos;
        self.rdpos = 0;
        self.wrpos = 0;
        Ok(())
    }
    fn size(&self) -> u64 {
        self.size
    }
}

pub struct Mp4Data {
    data: Vec<u8>,
    pos:  usize,
}

impl Mp4Data {
    pub fn new() -> Mp4Data {
        Mp4Data {
            data: Vec::new(),
            pos:  0,
        }
    }
}

impl ReadBytes for Mp4Data {
    fn read(&mut self, amount: u64) -> io::Result<&[u8]> {
        let amount = if amount == 0 { self.left() } else { amount };
        if amount == 0 {
            return Ok(b"");
        }
        if amount > self.left() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let pos = self.pos;
        let end = self.pos + amount as usize;
        self.pos += amount as usize;
        Ok(&self.data[pos..end])
    }

    fn skip(&mut self, amount: u64) -> io::Result<()> {
        self.seek(self.pos as u64 + amount)
    }

    #[inline]
    fn left(&self) -> u64 {
        if self.pos > self.data.len() {
            0
        } else {
            (self.data.len() - self.pos) as u64
        }
    }
}

impl WriteBytes for Mp4Data {
    fn write(&mut self, newdata: &[u8]) -> io::Result<()> {
        let pos = self.pos as usize;
        if pos < self.data.len() {
            if pos + newdata.len() > self.data.len() {
                self.data.resize(pos + newdata.len(), 0);
            }
            self.data[pos..pos + newdata.len()].copy_from_slice(newdata);
        } else {
            self.data.extend_from_slice(newdata);
        }
        self.pos += newdata.len();
        Ok(())
    }
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        self.seek(self.pos as u64 + amount)
    }
}

impl BoxBytes for Mp4Data {
    fn pos(&self) -> u64 {
        self.pos as u64
    }
    fn seek(&mut self, pos: u64) -> io::Result<()> {
        let pos = pos as usize;
        if pos > self.data.len() {
            self.data.resize(pos, 0);
        }
        self.pos = pos;
        Ok(())
    }
    fn size(&self) -> u64 {
        self.data.len() as u64
    }
}

impl<'a, B: ?Sized + ReadBytes + 'a> ReadBytes for Box<B> {
    fn read(&mut self, amount: u64) -> io::Result<&[u8]> {
        B::read(&mut *self, amount)
    }
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        B::skip(&mut *self, amount)
    }
    fn left(&self) -> u64 {
        B::left(&*self)
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
    fn pos(&self) -> u64 {
        B::pos(&*self)
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
}
