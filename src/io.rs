
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};

/// Byte reader in a stream.
pub trait ReadBytes: CommonBytes {
    /// Read an exact number of bytes, return a reference to the buffer.
    fn read(&mut self, amount: u64) -> io::Result<&[u8]>;
    /// Skip some bytes in the input.
    fn skip(&mut self, amount: u64) -> io::Result<()>;
    /// How much data is left?
    fn left(&self) -> u64;
}

/// Byte writer in a stream.
pub trait WriteBytes: CommonBytes {
    /// Write an exact number of bytes.
    fn write(&mut self, data: &[u8]) -> io::Result<()>;
    /// Zero-fill some bytes in the output.
    fn skip(&mut self, amount: u64) -> io::Result<()>;
}

/// Common methods for both ReadBytes and WriteBytes.
pub trait CommonBytes {
    fn get_version(&self) -> u8 {
        0
    }
    fn set_version(&mut self, _version: u8) {
        unimplemented!()
    }
}

/// Extended ReadBytes for an Mp4Box.
pub trait BoxReadBytes: ReadBytes {
    fn pos(&self) -> u64;
    fn size(&self) -> u64;
    fn limit(&mut self, limit: u64) -> Box<dyn BoxReadBytes + '_>;
}

/// Trait to write an amount of bytes to a source.
/// Extended WriteBytes for an Mp4Box.
pub trait BoxWriteBytes: WriteBytes {
}

/// Implementation of ReadBytes on a byte slice.
impl ReadBytes for &[u8] {
    fn read(&mut self, amount: u64) -> io::Result<&[u8]> {
        let mut amount = amount as usize;
        if amount > (*self).len() {
            return Ok(&b""[..]);
        }
        if amount == 0 {
            amount = self.len();
        }
        let res = &self[0..amount];
        (*self) = &self[amount..];
        Ok(res)
    }

    fn skip(&mut self, amount: u64) -> io::Result<()> {
        let mut amount = amount;
        if amount > (*self).len() as u64 {
            amount = self.len() as u64;
        }
        (*self) = &self[amount as usize..];
        Ok(())
    }

    fn left(&self) -> u64 {
        (*self).len() as u64
    }
}
impl CommonBytes for &[u8] { }

/// Implementation of WriteBytes on a byte slice.
impl WriteBytes for &mut [u8] {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        if (*self).len() < data.len() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let nself = std::mem::replace(self, &mut [0u8; 0]);
        nself.copy_from_slice(data);
        *self = &mut nself[data.len()..];
        Ok(())
    }
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        let mut amount = amount;
        if amount > (*self).len() as u64 {
            amount = self.len() as u64;
        }
        let nself = std::mem::replace(self, &mut [0u8; 0]);
        *self = &mut nself[amount as usize..];
        Ok(())
    }
}
impl CommonBytes for &mut [u8] {}

pub struct Mp4File {
    file: File,
    pos: u64,
    size: u64,
    buf: Vec<u8>,
    version: u8,
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
        }
    }
}

impl ReadBytes for Mp4File {

    fn read(&mut self, amount: u64) -> io::Result<&[u8]> {
        let mut amount = amount as usize;
        if amount == 0 {
            amount = std::cmp::min(1024, self.left()) as usize;
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

impl CommonBytes for Mp4File {
    fn get_version(&self) -> u8 {
        self.version
    }
    fn set_version(&mut self, version: u8) {
        self.version = version;
    }
}

impl BoxReadBytes for Mp4File {
    fn pos(&self) -> u64 {
        self.pos
    }
    fn size(&self) -> u64 {
        self.size
    }
    fn limit(&mut self, limit: u64) -> Box<dyn BoxReadBytes + '_> {
        Mp4FileLimited::new(self, limit)
    }
}

impl WriteBytes for Mp4File {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.file.write_all(data)
    }
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        self.pos += amount;
        self.file.seek(SeekFrom::Start(self.pos))?;
        Ok(())
    }
}

impl BoxWriteBytes for Mp4File {
}

pub struct Mp4FileLimited<'a> {
    inner: &'a mut Mp4File,
    limit: u64,
}

impl<'a> Mp4FileLimited<'a> {
    fn new(file: &mut Mp4File, limit: u64) -> Box<dyn BoxReadBytes + '_> {
        Box::new(Mp4FileLimited { inner: file, limit })
    }
}

impl<'a> ReadBytes for Mp4FileLimited<'a> {

    fn read(&mut self, mut amount: u64) -> io::Result<&[u8]> {
        if amount == 0 {
            amount = std::cmp::min(1024, self.left());
        }
        if self.inner.pos() + amount > self.limit {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.inner.read(amount)
    }

    fn skip(&mut self, amount: u64) -> io::Result<()> {
        if self.inner.pos() + amount > self.limit {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        ReadBytes::skip(self.inner, amount)
    }

    fn left(&self) -> u64 {
        if self.inner.pos() > self.limit {
            0
        } else {
            self.limit - self.inner.pos()
        }
    }
}

impl<'a> BoxReadBytes for Mp4FileLimited<'a> {
    fn pos(&self) -> u64 {
        self.inner.pos()
    }
    fn size(&self) -> u64 {
        self.inner.size()
    }
    fn limit(&mut self, limit: u64) -> Box<dyn BoxReadBytes + '_> {
        Mp4FileLimited::new(self.inner, limit)
    }
}

impl<'a> CommonBytes for Mp4FileLimited<'a> {
    fn get_version(&self) -> u8 {
        self.inner.get_version()
    }
    fn set_version(&mut self, version: u8) {
        self.inner.set_version(version);
    }
}

impl<'a, B: ?Sized + ReadBytes + 'a> ReadBytes for Box<B> {
    fn read(&mut self, amount: u64) -> io::Result<&[u8]> { B::read(&mut *self, amount) }
    fn skip(&mut self, amount: u64) -> io::Result<()> { B::skip(&mut *self, amount) }
    fn left(&self) -> u64 { B::left(&*self) }
}

impl<'a, B: ?Sized + BoxReadBytes + 'a> BoxReadBytes for Box<B> {
    fn pos(&self) -> u64 { B::pos(&*self) }
    fn size(&self) -> u64 { B::size(&*self) }
    fn limit(&mut self, limit: u64) -> Box<dyn BoxReadBytes + '_> { B::limit(&mut *self, limit) }
}

impl<'a, B: ?Sized + CommonBytes + 'a> CommonBytes for Box<B> {
    fn get_version(&self) -> u8 { B::get_version(&*self) }
    fn set_version(&mut self, version: u8) { B::set_version(&mut *self, version) }
}

