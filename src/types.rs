//! Definitions of types used in mp4 boxes.
//!
//! This module contains fundamental types used in boxes (such as Time,
//! ZString, IsoLanguageCode, etc).
//!
use std::convert::TryInto;
use std::fmt::{Debug, Display, Write};
use std::io;
use std::mem;
use std::time::{Duration, SystemTime};

use chrono::{
    self,
    offset::{Local, TimeZone},
};
use serde::Serialize;

use crate::io::DataRef;
use crate::mp4box::FullBox;
use crate::serialize::{FromBytes, ReadBytes, ToBytes, WriteBytes};

// Convenience macro to implement FromBytes/ToBytes for newtypes.
macro_rules! def_from_to_bytes_newtype {
    ($newtype:ident, $type:ty) => {
        impl FromBytes for $newtype {
            fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self> {
                let res = <$type>::from_bytes(bytes)?;
                Ok($newtype(res))
            }
            fn min_size() -> usize {
                <$type>::min_size()
            }
        }
        impl ToBytes for $newtype {
            fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) -> io::Result<()> {
                self.0.to_bytes(bytes)
            }
        }
    };
}

macro_rules! def_from_to_bytes_versioned {
    ($newtype:ident) => {
        def_from_to_bytes_versioned!($newtype, 0xffffffff);
    };
    ($newtype:ident, $max:expr) => {
        impl FromBytes for $newtype {
            fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self> {
                Ok(match bytes.version() {
                    1 => $newtype(u64::from_bytes(bytes)?),
                    _ => $newtype(u32::from_bytes(bytes)? as u64),
                })
            }
            fn min_size() -> usize {
                u32::min_size()
            }
        }
        impl ToBytes for $newtype {
            fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) -> io::Result<()> {
                match bytes.version() {
                    1 => self.0.to_bytes(bytes)?,
                    _ => (std::cmp::min(self.0, $max as u64) as u32).to_bytes(bytes)?,
                }
                Ok(())
            }
        }
        impl FullBox for $newtype {
            fn version(&self) -> Option<u8> {
                if self.0 <= $max {
                    None
                } else {
                    Some(1)
                }
            }
        }
        impl From<$newtype> for u64 {
            fn from(t: $newtype) -> u64 {
                t.0
            }
        }
        impl From<u64> for $newtype {
            fn from(t: u64) -> $newtype {
                $newtype(t)
            }
        }
    };
}

/// The optional "usertype" is a uuid.
#[derive(Clone, Default)]
pub struct Uuid(pub [u8; 16]);

impl FromBytes for Uuid {
    fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self> {
        let data = bytes.read(16)?;
        let mut u = [0u8; 16];
        u.copy_from_slice(data);
        Ok(Uuid(u))
    }

    fn min_size() -> usize {
        16
    }
}

impl ToBytes for Uuid {
    fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) -> io::Result<()> {
        bytes.write(&self.0[..])
    }
}

impl Display for Uuid {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        // 8-4-4-4-12
        let p1 = u32::from_be_bytes((self.0)[0..4].try_into().unwrap());
        let p2 = u16::from_be_bytes((self.0)[4..6].try_into().unwrap());
        let p3 = u16::from_be_bytes((self.0)[6..8].try_into().unwrap());
        let p4 = u16::from_be_bytes((self.0)[8..10].try_into().unwrap());
        let p5 = u16::from_be_bytes((self.0)[10..12].try_into().unwrap());
        let p6 = u32::from_be_bytes((self.0)[12..16].try_into().unwrap());
        write!(
            f,
            "{:08x}-{:04x}-{:04x}-{:04x}-{:04x}{:08x}",
            p1, p2, p3, p4, p5, p6
        )
    }
}

impl Debug for Uuid {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "\"{}\"", self)
    }
}

/// Basically a blob of data.
#[derive(Clone, Default)]
pub struct Data(pub Vec<u8>);

impl Data {
    /// Read an exact number of bytes.
    pub fn read<R: ReadBytes>(stream: &mut R, count: usize) -> io::Result<Self> {
        let mut v = Vec::new();
        if count > 0 {
            let data = stream.read(count as u64)?;
            v.extend_from_slice(data);
        }
        Ok(Data(v))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl FromBytes for Data {
    fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self> {
        let left = bytes.left();
        let data = bytes.read(left)?;
        let mut v = Vec::new();
        v.extend_from_slice(data);
        Ok(Data(v))
    }

    fn min_size() -> usize {
        0
    }
}

impl ToBytes for Data {
    fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) -> io::Result<()> {
        bytes.write(&self.0[..])
    }
}

impl Debug for Data {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.0.len() <= 16 {
            let mut s = String::from("[");
            let mut first = true;
            for d in &self.0 {
                if !first {
                    s.push(' ');
                }
                first = false;
                let _ = write!(s, "{:02x}", d);
            }
            s.push(']');
            write!(f, "{}", s)
        } else {
            write!(f, "[u8; {}]", &self.0.len())
        }
    }
}

/// 32 bits in boxes with version 0, and 64 bits in boxes with version >= 1.
#[derive(Clone, Copy, Default)]
pub struct VersionSizedUint(pub u64);
def_from_to_bytes_versioned!(VersionSizedUint);

impl Debug for VersionSizedUint {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

/// Duration_ is a 32/64 bit value where "all ones" means "unknown".
#[derive(Clone, Copy, Default)]
pub struct Duration_(pub u64);
def_from_to_bytes_versioned!(Duration_, 0x7fffffff);

impl Debug for Duration_ {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

/// Time is a 32/64 bit value, measured in seconds since 01-01-1904 00:00:00
#[derive(Clone, Copy, Default)]
pub struct Time(u64);
def_from_to_bytes_versioned!(Time);

// TZ=UTC date +%s -d "1904-01-01 00:00:00"
const OFFSET_TO_UNIX: u64 = 2082844800;

impl Time {
    #[allow(dead_code)]
    fn to_system_time(&self) -> SystemTime {
        if self.0 >= OFFSET_TO_UNIX {
            SystemTime::UNIX_EPOCH + Duration::new((self.0 - OFFSET_TO_UNIX) as u64, 0)
        } else {
            SystemTime::UNIX_EPOCH - Duration::new((OFFSET_TO_UNIX - self.0) as u64, 0)
        }
    }
    fn to_unixtime(&self) -> i64 {
        (self.0 as i64) - (OFFSET_TO_UNIX as i64)
    }
    fn to_rfc3339(&self) -> String {
        Local.timestamp(self.to_unixtime(), 0).to_rfc3339()
    }
}

impl Debug for Time {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.to_rfc3339())
    }
}

/// FourCC is the 4-byte name of any box.
///
/// Usually this is four bytes of ASCII characters, but it could be anything.
#[derive(Clone, Copy, Default)]
pub struct FourCC(pub u32);
def_from_to_bytes_newtype!(FourCC, u32);

impl FourCC {
    pub fn new(s: &str) -> FourCC {
        s.as_bytes().into()
    }

    fn fmt_fourcc(&self, dbg: bool) -> String {
        let c = self.to_be_bytes();
        for i in 0..4 {
            if (c[i] < 32 || c[i] > 126) && !(i == 0 && c[i] == 0xa9) {
                return format!("0x{:x}", self.0);
            }
        }
        let mut s = String::new();
        if dbg {
            s.push('"');
        }
        for i in 0..4 {
            s.push(c[i] as char);
        }
        if dbg {
            s.push('"');
        }
        s
    }

    #[inline]
    pub fn to_be_bytes(&self) -> [u8; 4] {
        self.0.to_be_bytes()
    }
}

// Let if (fourcc == b"moov") .. work
impl std::cmp::PartialEq<&[u8]> for FourCC {
    fn eq(&self, other: &&[u8]) -> bool {
        &(self.to_be_bytes())[..] == *other
    }
}

// Let if (fourcc == b"moov") .. work
impl std::cmp::PartialEq<&[u8; 4]> for FourCC {
    fn eq(&self, other: &&[u8; 4]) -> bool {
        &self.to_be_bytes() == *other
    }
}

impl std::cmp::PartialEq<FourCC> for FourCC {
    fn eq(&self, other: &FourCC) -> bool {
        self.0 == other.0
    }
}

impl Debug for FourCC {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.fmt_fourcc(true))
    }
}

impl std::fmt::Display for FourCC {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.fmt_fourcc(false))
    }
}

impl From<&[u8]> for FourCC {
    fn from(b: &[u8]) -> FourCC {
        FourCC(u32::from_be_bytes(b.try_into().unwrap()))
    }
}

/// Language code ('eng', 'dut', 'fra', etc).
///
/// A 16-bit value containing 3 5-bit values that are interpreted as letters,
/// so that we get a 3-character county code. Such as "eng", "ger", "dut" etc.
#[derive(Clone, Copy, Serialize)]
pub struct IsoLanguageCode(u16);
def_from_to_bytes_newtype!(IsoLanguageCode, u16);

impl Display for IsoLanguageCode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut s = String::new();
        s.push((((self.0 >> 10) & 0x1f) + 0x60) as u8 as char);
        s.push((((self.0 >> 5) & 0x1f) + 0x60) as u8 as char);
        s.push((((self.0 >> 0) & 0x1f) + 0x60) as u8 as char);
        write!(f, "{}", s)
    }
}

impl Debug for IsoLanguageCode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl Default for IsoLanguageCode {
    fn default() -> IsoLanguageCode {
        // "und"
        IsoLanguageCode(0x55c4)
    }
}

/// Zero terminated ASCII string.
#[derive(Clone, Default)]
pub struct ZString(pub String);

impl ZString {
    fn as_str(&self) -> &str {
        let len = if self.0.ends_with("\0") {
            self.0.len() - 1
        } else {
            self.0.len()
        };
        &(self.0)[..len]
    }
}

impl std::ops::Deref for ZString {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl FromBytes for ZString {
    fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self> {
        let left = bytes.left();
        let data = bytes.read(left)?;
        let mut s = String::new();
        let mut idx = 0;
        let maxlen = data.len();
        while idx < maxlen {
            let b = data[idx];
            s.push(b as char);
            idx += 1;
            if b == 0 {
                break;
            }
        }
        Ok(ZString(s))
    }
    fn min_size() -> usize {
        0
    }
}

impl ToBytes for ZString {
    fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) -> io::Result<()> {
        let mut v = Vec::new();
        for c in self.0.chars() {
            if (c as u32) < 256 {
                v.push(c as u8);
            } else {
                v.push(0xff);
            }
        }
        bytes.write(&v)
    }
}


impl Display for ZString {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Debug for ZString {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "\"{}\"", self.as_str())
    }
}

/// Matrix.
#[derive(Clone, Default)]
pub struct Matrix([(FixedFloat16_16, FixedFloat16_16, FixedFloat2_30); 3]);

impl FromBytes for Matrix {
    fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self> {
        let mut m = [(FixedFloat16_16(0), FixedFloat16_16(0), FixedFloat2_30(0)); 3];
        for x in 0..3 {
            m[x] = (
                FixedFloat16_16::from_bytes(bytes)?,
                FixedFloat16_16::from_bytes(bytes)?,
                FixedFloat2_30::from_bytes(bytes)?,
            );
        }
        Ok(Matrix(m))
    }
    fn min_size() -> usize {
        36
    }
}
impl ToBytes for Matrix {
    fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) -> io::Result<()> {
        for x in 0..3 {
            (self.0)[x].0.to_bytes(bytes)?;
            (self.0)[x].1.to_bytes(bytes)?;
            (self.0)[x].2.to_bytes(bytes)?;
        }
        Ok(())
    }
}

impl Debug for Matrix {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "Matrix([{}][{}][{}] [{}][{}][{}] [{}][{}][{}])",
            (self.0)[0].0,
            (self.0)[0].1,
            (self.0)[0].2,
            (self.0)[1].0,
            (self.0)[1].1,
            (self.0)[1].2,
            (self.0)[2].0,
            (self.0)[2].1,
            (self.0)[2].2,
        )
    }
}

macro_rules! impl_flags {
    ($(#[$outer:meta])* $type:ident $(,$debug:ident)?) => {
        $(#[$outer])*
        #[derive(Clone, Copy)]
        pub struct $type(pub u32);

        impl FromBytes for $type {
            fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self> {
                Ok($type(bytes.flags()))
            }
            fn min_size() -> usize {
                0
            }
        }

        impl ToBytes for $type {
            fn to_bytes<W: WriteBytes>(&self, _bytes: &mut W) -> io::Result<()> {
                Ok(())
            }
        }

        impl FullBox for $type {
            fn flags(&self) -> u32 {
                self.0
            }
        }

        impl_flags_debug!($type, $($debug)?);

        impl $type {
            pub fn get(&self, bit: u32) -> bool {
                let mask = 1 << bit;
                self.0 & mask > 0
            }
            pub fn set(&mut self, bit: u32, on: bool) {
                if on {
                    self.0 |= 1u32 << bit;
                } else {
                    self.0 &= !(1u32 << bit)
                }
            }
        }
    };
}

macro_rules! impl_flags_debug {
    ($type:ty, debug) => {
        impl std::fmt::Debug for $type {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "Flags({:#x})", self.0)
            }
        }
    };
    ($type:ty,) => {};
}

impl_flags!(
    /// Generic 24 bits flags.
    #[derive(Default)]
    Flags,
    debug
);

/// 8.8.3.1 Sample Flags (ISO/IEC 14496-12:2015(E))
///
/// For the first four fields, see 8.6.4.3 (Semantics).
/// The sample_is_non_sync_sample field  provides the same information as the sync sample table [8.6.2].
#[derive(Clone, Debug, Default)]
pub struct SampleFlags {
    pub is_leading:                  u8,
    pub sample_depends_on:           u8,
    pub sample_is_depended_on:       u8,
    pub sample_has_redundancy:       u8,
    pub sample_padding_value:        u8,
    pub sample_is_non_sync_sample:   bool,
    pub sample_degradation_priority: u16,
}

impl FromBytes for SampleFlags {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
        let flags = u16::from_bytes(stream)?;
        let sample_degradation_priority = u16::from_bytes(stream)?;
        Ok(SampleFlags {
            is_leading: ((flags & 0b0000110000000000) >> 10) as u8,
            sample_depends_on: ((flags & 0b0000001100000000) >> 8) as u8,
            sample_is_depended_on: ((flags & 0b0000000011000000) >> 6) as u8,
            sample_has_redundancy: ((flags & 0b0000000000110000) >> 4) as u8,
            sample_padding_value: ((flags & 0b0000000000001110) >> 1) as u8,
            sample_is_non_sync_sample: (flags & 0b0000000000000001) > 0,
            sample_degradation_priority,
        })
    }

    fn min_size() -> usize {
        4
    }
}

impl ToBytes for SampleFlags {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let flags = (((self.is_leading & 0b11) as u16) << 10) |
            (((self.sample_depends_on & 0b11) as u16) << 8) |
            (((self.sample_is_depended_on & 0b11) as u16) << 6) |
            (((self.sample_has_redundancy & 0b11) as u16) << 4) |
            (((self.sample_padding_value & 0b111) as u16) << 1) |
            self.sample_is_non_sync_sample as u16;
        flags.to_bytes(stream)?;
        self.sample_degradation_priority.to_bytes(stream)?;
        Ok(())
    }
}

mod doc_hidden {
    pub trait FromPrimitive: Sized {
        fn from_usize(n: usize) -> Self;
    }
    impl FromPrimitive for () {
        fn from_usize(_n: usize) -> () { () }
    }
    impl FromPrimitive for u16 {
        fn from_usize(n: usize) -> u16 { n as u16 }
    }
    impl FromPrimitive for u32 {
        fn from_usize(n: usize) -> u32 { n as u32 }
    }

    pub trait ToPrimitive {
        fn to_usize(self) -> usize;
    }
    impl ToPrimitive for ()
    {
        fn to_usize(self) -> usize { unimplemented!() }
    }
    impl ToPrimitive for u16
    {
        fn to_usize(self) -> usize { self as usize }
    }
    impl ToPrimitive for u32
    {
        fn to_usize(self) -> usize { self as usize }
    }
}

#[doc(hidden)]
pub use doc_hidden::*;

/// A mutable list of items.
///
/// When reading from a source file or writing to a destination file,
/// the `N` type indicates whether there is an integer in front of
/// the array's elements stating its size.
///
/// - `()`: no size, elements go on to the end of the box
/// - `u16`: 2 bytes size
/// - `u32`: 4 bytes size.
///
pub struct Array<N, T> {
    vec: Vec<T>,
    num_entries_type: std::marker::PhantomData<N>,
}

impl<N, T> Array<N, T> {
    /// Constructs a new, empty `Array`.
    pub fn new() -> Self {
        Self {
            vec: Vec::<T>::new(),
            num_entries_type: std::marker::PhantomData,
        }
    }

    /// Appends an element to the back.
    pub fn push(&mut self, value: T) {
        self.vec.push(value)
    }

    /// Returns the number of elements in the array.
    pub fn len(&self) -> usize {
        self.vec.len()
    }

    /// Returns an iterator over the elements in this array.
    pub fn iter(&self) -> ArrayIterator<'_, T> {
        ArrayIterator(self.vec.iter())
    }

    /// Returns an iterator that clones.
    pub fn iter_cloned(&self) -> ArrayIteratorCloned<'_, T>
    where
        T: Clone,
    {
        ArrayIteratorCloned::<'_, T> {
            count:      self.len(),
            default:    None,
            entries:    &self.vec[..],
            index:      0,
        }
    }

    /// Returns an iterator that repeats the same item `count` times.
    pub fn iter_repeat(&self, item: T, count: usize) -> ArrayIteratorCloned<'_, T>
    where
        T: Clone,
    {
        ArrayIteratorCloned::<'_, T> {
            count,
            default:    Some(item),
            entries:    &[],
            index:      0,
        }
    }
}

impl<N, T> Array<N, T>
where
    T: Clone,
{
    /// Get a clone of the value at index `index`.
    pub fn get(&self, index: usize) -> T {
        self.vec[index].clone()
    }
}

impl<N, T> Default for Array<N, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<N, T> FromBytes for Array<N, T> where N: FromBytes + ToPrimitive, T: FromBytes {

    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
        let (mut v, count) = if mem::size_of::<N>() == 0 {
            (Vec::new(), std::u32::MAX as usize)
        } else {
            let sz = N::from_bytes(stream)?.to_usize();
            (Vec::with_capacity(sz), sz)
        };
        let min_size = T::min_size() as u64;
        while v.len() < count && stream.left() >= min_size && stream.left() > 0 {
            v.push(T::from_bytes(stream)?);
        }
        Ok(Self {
            vec: v,
            num_entries_type: std::marker::PhantomData,
        })
    }

    fn min_size() -> usize {
        if mem::size_of::<N>() > 0 {
            N::min_size()
        } else {
            0           
        }
    }
}

impl<N, T> ToBytes for Array<N, T> where N: ToBytes + FromPrimitive, T: ToBytes {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        if mem::size_of::<N>() > 0 {
            N::from_usize(self.vec.len()).to_bytes(stream)?;
        }
        for elem in &self.vec {
            elem.to_bytes(stream)?;
        }
        Ok(())
    }
}

impl<N, T> FullBox for Array<N, T> where T: FullBox {
    fn version(&self) -> Option<u8> {
        // Find the highest version of any entry.
        let mut r = None;
        for e in &self.vec {
            if let Some(ver) = e.version() {
                if let Some(r_ver) = r {
                    if ver > r_ver {
                        r = Some(ver);
                    }
                } else {
                    r = Some(ver);
                }
            }
        }
        r
    }
}

impl<N, T> Clone for Array<N, T> where T: Clone {
    fn clone(&self) -> Self {
        Self {
            vec: self.vec.clone(),
            num_entries_type: std::marker::PhantomData,
        }
    }
}

// Debug implementation that delegates to the inner Vec.
impl<N, T> Debug for Array<N, T> where T: Debug {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if f.alternate() {
            if self.vec.len() > 4 {
                writeln!(f, "\n// Array length: {}", self.vec.len())?;
            }
            if self.vec.len() > 20 {
                writeln!(f, "// (only showing first and last entry)")?;
                let v = vec![&self.vec[0], &self.vec[self.vec.len() - 1]];
                return f.debug_list().entries(v.into_iter()).finish();
            }
        }
        Debug::fmt(&self.vec, f)
    }
}

impl<N, T> std::ops::Deref for Array<N, T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        std::ops::Deref::deref(&self.vec)
    }
}

impl<N, T> std::ops::DerefMut for Array<N, T> {
    fn deref_mut(&mut self) -> &mut[T] {
        std::ops::DerefMut::deref_mut(&mut self.vec)
    }
}

impl<'a, N, T> IntoIterator for &'a Array<N, T> {
    type Item = &'a T;
    type IntoIter = ArrayIterator<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<N, T> std::iter::FromIterator<T> for Array<N, T> {
    fn from_iter<I>(iter: I) -> Self where I: IntoIterator<Item = T> {
        let mut v = Vec::new();
        for i in iter {
            v.push(i);
        }
        Self {
            vec: v,
            num_entries_type: std::marker::PhantomData,
        }
    }
}

pub type ArraySized16<T> = Array<u16, T>;
pub type ArraySized32<T> = Array<u32, T>;
pub type ArrayUnsized<T> = Array<(), T>;

/// Iterator over borrowed elements.
pub struct ArrayIterator<'a, T>(std::slice::Iter<'a, T>);

impl<'a, T> Iterator for ArrayIterator<'a, T>
{
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

pub struct ArrayIteratorCloned<'a, T> {
    count:      usize,
    default:    Option<T>,
    entries:    &'a [T],
    index:      usize,
}

impl<'a, T> ArrayIteratorCloned<'a, T> {
    /// Check if all items fall in the range.
    ///
    /// We assume that the items are ordered, and check only
    /// the first and last item.
    pub fn in_range(&self, range: std::ops::Range<T>) -> bool where T: std::cmp::PartialOrd<T> {
        if self.count == 0 {
            return true;
        }
        if let Some(dfl) = self.default.as_ref() {
            return dfl.ge(&range.start) && dfl.lt(&range.end);
        }
        self.entries[0].ge(&range.start) && self.entries[self.entries.len() - 1].lt(&range.end)
    }
}

/// Iterator over cloned elements.
impl<'a, T> Iterator for ArrayIteratorCloned<'a, T>
where
    T: Clone,
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
        let entry = &self.entries[self.index];
        self.index += 1;
        Some(entry.clone())
    }
}

/// Storage backed by an data reference or an array (vector, really).
///
/// N: If sized, means that the slice begins with a 'count'
///    that indicates the number of elements in the slice.
///
/// T: Type of elements in the slice, used with 'push' and 'iter'.
pub enum List<N=(), T=u8> {
    DataRef(DataRef<N, T>),
    Array(Array<N, T>),
}

pub type ListUnsized<T> = List<(), T>;
pub type ListSized16<T> = List<u16, T>;
pub type ListSized32<T> = List<u32, T>;

impl<N, T> List<N, T> {
    /// Number of elements.
    pub fn len(&self) -> u64 {
        match self {
            List::DataRef(this) => this.len(),
            List::Array(this) => this.len() as u64,
        }
    }

    /// Does it need a large box.
    pub fn is_large(&self) -> bool {
        self.len() > u32::MAX as u64 - 16
    }

    /// return an iterator over all items.
    pub fn iter(&self) -> ListIterator<'_, T>
    where
        T: FromBytes,
    {
        match self {
            List::DataRef(this) => ListIterator::DataRef(this.iter()),
            List::Array(this) => ListIterator::Array(this.iter()),
        }
    }

    /// return an iterator over all items.
    pub fn iter_cloned(&self) -> ListIteratorCloned<'_, T>
    where
        T: FromBytes + Clone,
    {
        match self {
            List::DataRef(this) => ListIteratorCloned::DataRef(this.iter_cloned()),
            List::Array(this) => ListIteratorCloned::Array(this.iter_cloned()),
        }
    }

    /// Return an iterator that repeats the same item `count` times.
    pub fn iter_repeat(&self, item: T, count: usize) -> ListIteratorCloned<'_, T>
    where
        T: FromBytes + Clone,
    {
        match self {
            List::DataRef(this) => ListIteratorCloned::DataRef(this.iter_repeat(item, count)),
            List::Array(this) => ListIteratorCloned::Array(this.iter_repeat(item, count)),
        }
    }
}

impl<N, T> List<N, T>
where
    T: FromBytes + Clone,
{
    /// Get a clone of the value at index `index`.
    pub fn get(&self, index: usize) -> T {
        match self {
            List::DataRef(this) => this.get(index),
            List::Array(this) => this.get(index),
        }
    }
}

impl<N, T> FromBytes for List<N, T>
where
    N: FromBytes + ToPrimitive,
    T: FromBytes,
{
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
        let data_ref = DataRef::<N, T>::from_bytes(stream)?;
        Ok(List::DataRef(data_ref))
    }

    fn min_size() -> usize {
        DataRef::<N, T>::min_size()
    }
}

impl<N, T> ToBytes for List<N, T>
where
    N: ToBytes + FromPrimitive,
    T: ToBytes,
{
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        match self {
            List::DataRef(this) => this.to_bytes(stream),
            List::Array(this) => this.to_bytes(stream),
        }
    }
}

impl<N, T> Default for List<N, T> {
    /// The default is an empty Array.
    fn default() -> Self {
        let array = Array::<N, T>::new();
        List::Array(array)
    }
}

impl<N, T> Clone for List<N, T> where N: Clone, T: Clone {
    fn clone(&self) -> Self {
        match self {
            List::DataRef(this) => List::DataRef(this.clone()),
            List::Array(this) => List::Array(this.clone()),
        }
    }
}

impl<N, T> std::fmt::Debug for List<N, T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            List::DataRef(this) => this.fmt(f),
            List::Array(this) => this.fmt(f),
        }
    }
}

/// Iterator over the elements of the underlying DataRef or Array.
pub enum ListIterator<'a, T> {
    DataRef(crate::io::DataRefIterator<'a, T>),
    Array(ArrayIterator<'a, T>),
}

impl<'a, T> Iterator for ListIterator<'a, T>
where
    T: FromBytes,
{
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            ListIterator::DataRef(this) => this.next(),
            ListIterator::Array(this) => this.next(),
        }
    }
}

/// Iterator over the elements of the underlying DataRef or Array.
pub enum ListIteratorCloned<'a, T> {
    DataRef(crate::io::DataRefIteratorCloned<'a, T>),
    Array(ArrayIteratorCloned<'a, T>),
}

impl<'a, T> ListIteratorCloned<'a, T>
where
    T: FromBytes + Clone,
{
    /// Check if all items fall in the range.
    pub fn in_range(&self, range: std::ops::Range<T>) -> bool where T: std::cmp::PartialOrd<T> {
        match self {
            ListIteratorCloned::DataRef(this) => this.in_range(range),
            ListIteratorCloned::Array(this) => this.in_range(range),
        }
    }
}

impl<'a, T> Iterator for ListIteratorCloned<'a, T>
where
    T: FromBytes + Clone,
{
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            ListIteratorCloned::DataRef(this) => this.next(),
            ListIteratorCloned::Array(this) => this.next(),
        }
    }
}

macro_rules! fixed_float {
    ($(#[$outer:meta])* $name:ident, $type:tt, $frac_bits:expr) => {
        #[derive(Clone, Copy, Default)]
        $(#[$outer])*
        pub struct $name($type);
        def_from_to_bytes_newtype!($name, $type);

        impl $name {
            fn get(&self) -> f64 {
                (self.0 as f64) / ((1 << $frac_bits) as f64)
            }

            #[allow(dead_code)]
            pub fn set(&mut self, value: f64) {
                let v = (value * ((1 << $frac_bits) as f64)).round();
                self.0 = if v > (std::$type::MAX as f64) {
                    std::$type::MAX
                } else if v < (std::$type::MIN as f64) {
                    std::$type::MIN
                } else {
                    v as $type
                };
            }
        }

        impl Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", self.get())
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", self.get())
            }
        }

        impl From<f64> for $name {
            fn from(t: f64) -> $name {
                let mut x = $name(0);
                x.set(t);
                x
            }
        }

        impl From<$name> for f64 {
            fn from(t: $name) -> f64 {
                t.get()
            }
        }
    };
}

// Some fixed float types.
fixed_float!(
    /// 32 bits 2.30 fixed float
    FixedFloat2_30,
    u32,
    30
);
fixed_float!(
    /// 32 bits 16.16 fixed float.
    FixedFloat16_16,
    u32,
    16
);

fixed_float!(
    /// 16 bits 8.8 fixed float.
    FixedFloat8_8,
    u16,
    8
);

/// Pascal string. 1 byte of length followed by string itself.
///
/// Note that the length does not include the length byte itself.
#[derive(Clone, Debug, Default)]
pub struct PString(String);

impl PString {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl std::ops::Deref for PString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

impl FromBytes for PString {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<PString> {
        let len = u8::from_bytes(stream)? as u64;
        let data = if len > 0 {
            stream.read(len)?
        } else {
            b""
        };
        if let Ok(s) = std::str::from_utf8(data) {
            return Ok(PString(s.to_string()));
        }
        // If it's not utf-8, mutilate the data.
        let mut s = String::new();
        for d in data {
            s.push(std::cmp::min(*d, 127) as char);
        }
        Ok(PString(s))
    }
    fn min_size() -> usize { 0 }
}

impl ToBytes for PString {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let len = std::cmp::min(self.0.len(), 254);
        (len as u8).to_bytes(stream)?;
        stream.write(self.0[..len].as_bytes())
    }
}

/// Pascal16 string. 2 bytes of length followed by string itself.
///
/// Note that the length does not include the length byte itself.
#[derive(Clone, Debug, Default)]
pub struct P16String(String);

impl P16String {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl std::ops::Deref for P16String {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

impl FromBytes for P16String {
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<P16String> {
        let len = u16::from_bytes(stream)? as u64;
        let data = if len > 0 {
            stream.read(len)?
        } else {
            b""
        };
        if let Ok(s) = std::str::from_utf8(data) {
            return Ok(P16String(s.to_string()));
        }
        // If it's not utf-8, mutilate the data.
        let mut s = String::new();
        for d in data {
            s.push(std::cmp::min(*d, 127) as char);
        }
        Ok(P16String(s))
    }
    fn min_size() -> usize { 0 }
}

impl ToBytes for P16String {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        let len = std::cmp::min(self.0.len(), 254);
        (len as u8).to_bytes(stream)?;
        stream.write(self.0[..len].as_bytes())
    }
}

