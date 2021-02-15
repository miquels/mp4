//! File read/write.
//!
use std::fs;
use std::io::{self, ErrorKind};
use std::mem;
use std::sync::Arc;

use memmap::{Mmap, MmapOptions};

use crate::serialize::{BoxBytes, FromBytes, ReadBytes, ToBytes, WriteBytes};
use crate::types::{FourCC, ToPrimitive};

/// Reads a MP4 file.
///
/// Implements `ReadBytes`, so can be passed to `MP4::read`.
pub struct Mp4File {
    mmap:           Arc<Mmap>,
    file:           fs::File,
    pos:            u64,
    size:           u64,
    input_filename: Option<String>,
}

impl Mp4File {
    /// Open an mp4 file.
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
            input_filename: Some(path.to_string()),
        })
    }

    /// Get the `File` out again.
    pub fn into_inner(self) -> fs::File {
        self.file
    }
}

impl ReadBytes for Mp4File {
    #[inline]
    fn read(&mut self, amount: u64) -> io::Result<&[u8]> {
        //println!("XXX DBG read {}", amount);
        if self.pos + amount > self.size {
            return Err(io::Error::new(ErrorKind::UnexpectedEof, "tried to read past eof"));
        }
        let pos = self.pos as usize;
        self.pos += amount;
        Ok(&self.mmap[pos..pos + amount as usize])
    }

    #[inline]
    fn peek(&mut self, amount: u64) -> io::Result<&[u8]> {
        //println!("XXX DBG peek {}", amount);
        if self.pos + amount > self.size {
            return Err(io::Error::new(ErrorKind::UnexpectedEof, "tried to read past eof"));
        }
        let pos = self.pos as usize;
        Ok(&self.mmap[pos..pos + amount as usize])
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
            mmap:             self.mmap.clone(),
            start:            self.pos as usize,
            end:              (self.pos + size) as usize,
            num_entries_type: std::marker::PhantomData,
            entry_type:       std::marker::PhantomData,
        })
    }

    fn input_filename(&self) -> Option<&str> {
        self.input_filename.as_ref().map(|s| s.as_str())
    }
}

/// Reference to items that are stored in a chunk of data somewhere
/// in the source file. Could be in the `moov` box, or a `moof` box,
/// or an `mdat` box.
///
/// It is a lot like [`Array`](crate::types::Array), except it's
/// read-only.
pub struct DataRef<N = (), T = u8> {
    mmap:             Arc<Mmap>,
    start:            usize,
    end:              usize,
    num_entries_type: std::marker::PhantomData<N>,
    entry_type:       std::marker::PhantomData<T>,
}

pub type DataRefUnsized<T> = DataRef<(), T>;
pub type DataRefSized16<T> = DataRef<u16, T>;
pub type DataRefSized32<T> = DataRef<u32, T>;

impl<N, T> DataRef<N, T> {
    // This is not the from_bytes from the FromBytes trait, it is
    // a direct method, because it has an extra data_size argument.
    pub(crate) fn from_bytes_limit<R: ReadBytes>(
        stream: &mut R,
        data_size: u64,
    ) -> io::Result<DataRef<N, T>> {
        // The stream returns a DataRef<(), u8>. we need to convert it
        // into our type.
        let data_ref = stream.data_ref(data_size)?;
        stream.skip(data_size)?;
        Ok(data_ref.transmute())
    }

    // This is a safe transmute, we only change the PhantomData markers.
    // It changes the input for put() and the output of the iterator.
    pub(crate) fn transmute<N2, T2>(self) -> DataRef<N2, T2> {
        DataRef {
            mmap:             self.mmap,
            start:            self.start,
            end:              self.end,
            num_entries_type: std::marker::PhantomData,
            entry_type:       std::marker::PhantomData,
        }
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.mmap[self.start..self.end]
    }

    /// Number of items.
    pub fn len(&self) -> u64 {
        (self.end - self.start) as u64 / (mem::size_of::<T>() as u64)
    }

    /// Does it need a large box.
    pub fn is_large(&self) -> bool {
        self.len() > u32::MAX as u64 - 16
    }

    /// Return an iterator over all items.
    ///
    /// This panics using `unimplemented()`. It's impossible to return a
    /// reference to some owned data in the iterator itself due to
    /// lifetime issues. Use [`iter_cloned`](Self::iter_cloned).
    pub fn iter(&self) -> DataRefIterator<'_, T>
    where
        T: FromBytes,
    {
        unimplemented!()
    }

    /// return an iterator over all items.
    pub fn iter_cloned(&self) -> DataRefIteratorCloned<'_, T>
    where
        T: FromBytes + Clone,
    {
        DataRefIteratorCloned::<'_, T> {
            count:   self.len() as usize,
            default: None,
            entries: self.bytes(),
            index:   0,
        }
    }

    /// Return an iterator that repeats the same item `count` times.
    pub fn iter_repeat(&self, item: T, count: usize) -> DataRefIteratorCloned<'_, T>
    where
        T: FromBytes + Clone,
    {
        DataRefIteratorCloned::<'_, T> {
            count,
            default: Some(item),
            entries: b"",
            index: 0,
        }
    }
}

impl<N, T> DataRef<N, T>
where
    T: FromBytes + Clone,
{
    /// Get a clone of the value at index `index`.
    pub fn get(&self, index: usize) -> T {
        let start = index * mem::size_of::<T>();
        let end = start + mem::size_of::<T>();
        let mut data = &self.bytes()[start..end];
        T::from_bytes(&mut data).unwrap()
    }
}

impl<N, T> FromBytes for DataRef<N, T>
where
    N: FromBytes + ToPrimitive,
    T: FromBytes,
{
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
        let elem_size = mem::size_of::<N>() as u64;
        let (size, skip) = if elem_size == 0 {
            ((stream.left() / elem_size) * elem_size, stream.left())
        } else {
            let sz = (N::from_bytes(stream)?.to_usize() as u64) * elem_size;
            (sz, sz)
        };
        let data_ref = stream.data_ref(size)?;
        stream.skip(skip)?;
        Ok(data_ref.transmute())
    }

    fn min_size() -> usize {
        if mem::size_of::<N>() > 0 {
            N::min_size()
        } else {
            0
        }
    }
}

impl<N, T> ToBytes for DataRef<N, T> {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        stream.write(self.bytes())
    }
}

impl<N, T> Default for DataRef<N, T> {
    fn default() -> Self {
        let devzero = fs::File::open("/dev/zero").unwrap();
        let mmap = unsafe { MmapOptions::new().len(4).map(&devzero).unwrap() };
        DataRef {
            mmap:             Arc::new(mmap),
            start:            0,
            end:              0,
            num_entries_type: std::marker::PhantomData,
            entry_type:       std::marker::PhantomData,
        }
    }
}

impl<N, T> Clone for DataRef<N, T>
where
    N: Clone,
    T: Clone,
{
    fn clone(&self) -> Self {
        DataRef {
            mmap:             self.mmap.clone(),
            start:            self.start,
            end:              self.end,
            num_entries_type: self.num_entries_type,
            entry_type:       self.entry_type,
        }
    }
}

// deref to &[u8]
impl<N, T> std::ops::Deref for DataRef<N, T> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.mmap[self.start..self.end]
    }
}

impl<N, T> std::fmt::Debug for DataRef<N, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{{ &file[{}..{}] }}", self.start, self.end)
    }
}

pub struct DataRefIterator<'a, T> {
    type_: std::marker::PhantomData<&'a T>,
}

impl<'a, T> Iterator for DataRefIterator<'a, T>
where
    T: FromBytes,
{
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // This cannot be implemented. We cannot return a reference to
        // an element if we got it from T::from_bytes, we can't
        // express the lifetime properly.
        unimplemented!()
    }
}


pub struct DataRefIteratorCloned<'a, T> {
    count:   usize,
    default: Option<T>,
    entries: &'a [u8],
    index:   usize,
}

impl<'a, T> DataRefIteratorCloned<'a, T>
where
    T: FromBytes + Clone,
{
    /// Check if all items fall in the range.
    ///
    /// We assume that the items are ordered, and check only
    /// the first and last item.
    pub fn in_range(&self, range: std::ops::Range<T>) -> bool
    where
        T: std::cmp::PartialOrd<T>,
    {
        if self.count == 0 {
            return true;
        }
        if let Some(dfl) = self.default.as_ref() {
            return dfl.ge(&range.start) && dfl.lt(&range.end);
        }
        if let Some((first, last)) = self.first_last() {
            return first >= range.start && last < range.end;
        }
        false
    }

    fn first_last(&self) -> Option<(T, T)> {
        if self.count == 0 {
            return None;
        }
        if let Some(entry) = self.default.as_ref() {
            return Some((entry.clone(), entry.clone()));
        }

        let mut data = &self.entries[0..mem::size_of::<T>()];
        let first = T::from_bytes(&mut data).ok()?;

        let start = (self.count - 1) * mem::size_of::<T>();
        let end = start + mem::size_of::<T>();
        let mut data = &self.entries[start..end];
        let last = T::from_bytes(&mut data).ok()?;
        Some((first, last))
    }
}

impl<'a, T> Iterator for DataRefIteratorCloned<'a, T>
where
    T: FromBytes + Clone,
{
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.count as usize {
            return None;
        }
        if let Some(entry) = self.default.as_ref() {
            self.index += 1;
            return Some(entry.clone());
        }
        let start = self.index * mem::size_of::<T>();
        let end = start + mem::size_of::<T>();
        let mut data = &self.entries[start..end];
        self.index += 1;
        Some(T::from_bytes(&mut data).unwrap())
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
