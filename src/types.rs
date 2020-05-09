/// Definitions of types used in mp4 boxes.
///
/// This module contains fundamental types used in boxes (such as Time,
/// ZString, IsoLanguageCode, etc).
///
use std::convert::TryInto;
use std::fmt::{Debug, Display, Write};
use std::io;
use std::time::{Duration, SystemTime};

use chrono::{
    self,
    offset::{Local, TimeZone},
};

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
#[derive(Clone)]
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

/// Just some data.
#[derive(Default)]
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
        let data = bytes.read(0)?;
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

#[derive(Clone, Copy)]
pub struct VersionSizedUint(pub u64);
def_from_to_bytes_versioned!(VersionSizedUint);

impl Debug for VersionSizedUint {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

/// Duration_ is a 32/64 bit value where "all ones" means "unknown".
#[derive(Clone, Copy)]
pub struct Duration_(pub u64);
def_from_to_bytes_versioned!(Duration_, 0x7fffffff);

impl Debug for Duration_ {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

/// Time is a 32/64 bit value, measured in seconds since 01-01-1904 00:00:00
#[derive(Clone, Copy)]
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

/// FourCC is the 4-byte name of any atom. Usually this is four bytes
/// of ASCII characters, but it could be anything.
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

/// A 16-bit value containing 3 5-bit values that are interpreted as letters,
/// so that we get a 3-character county code. Such as "eng", "ger", "dut" etc.
#[derive(Clone, Copy)]
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

/// Zero terminated ASCII string.
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
        let data = bytes.read(0)?;
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
        impl Debug for $type {
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

impl_flags!(
    /// Always 0x01.
    VideoMediaHeaderFlags,
    debug
);

impl Default for VideoMediaHeaderFlags {
    fn default() -> Self {
        Self(0x01)
    }
}

impl_flags!(
    /// Track: enabled/in_movie/preview
    TrackFlags
);

impl TrackFlags {
    pub fn get_enabled(&self) -> bool {
        self.get(0)
    }
    pub fn set_enabled(&mut self, on: bool) {
        self.set(0, on)
    }
    pub fn get_in_movie(&self) -> bool {
        self.get(1)
    }
    pub fn set_in_movie(&mut self, on: bool) {
        self.set(1, on)
    }
    pub fn get_in_preview(&self) -> bool {
        self.get(2)
    }
    pub fn set_in_preview(&mut self, on: bool) {
        self.set(2, on)
    }
    pub fn get_in_poster(&self) -> bool {
        self.get(3)
    }
    pub fn set_in_poster(&mut self, on: bool) {
        self.set(3, on)
    }
}

impl Debug for TrackFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut v = vec!["["];
        if self.get_enabled() {
            v.push("enabled");
        }
        if self.get_in_movie() {
            v.push("in_movie");
        }
        if self.get_in_preview() {
            v.push("in_preview");
        }
        if self.get_in_poster() {
            v.push("in_poster");
        }
        v.push("]");
        write!(f, "TrackFlags({})", v.join(" "))
    }
}

impl_flags!(
    /// 0x01 if the data is in the same file (default).
    DataEntryFlags
);

impl DataEntryFlags {
    pub fn get_in_same_file(&self) -> bool {
        self.get(0)
    }
    pub fn set_in_same_file(&mut self, on: bool) {
        self.set(0, on)
    }
}

impl Debug for DataEntryFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut v = vec!["["];
        if self.get_in_same_file() {
            v.push("in_same_file");
        }
        v.push("]");
        write!(f, "DataEntryFlags({})", v.join(" "))
    }
}

impl Default for DataEntryFlags {
    fn default() -> Self {
        Self(0x01)
    }
}

// self.movie.and_then(|m| self.boxes[m as usize].downcast_ref::<Movie>())
pub struct IndexU32(u32);
impl IndexU32 {
    pub fn get(self) -> Option<u32> {
        match self.0 {
            0xffffffff => None,
            some => Some(some),
        }
    }
    pub fn set(&mut self, val: Option<u32>) {
        self.0 = val.unwrap_or(0xffffffff);
    }
}

/// Composition offset entry.
#[derive(Debug)]
pub struct CompositionOffsetEntry {
    count:  u32,
    offset: i32,
}

impl FromBytes for CompositionOffsetEntry {
    // NOTE: This implementation is not _entirely_ correct. If in a
    // version 0 entry the offset >= 2^31 it breaks horribly.
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
        let count = u32::from_bytes(stream)?;
        let offset = if stream.version() == 0 {
            let offset = u32::from_bytes(stream)?;
            std::cmp::min(offset, 0x7fffffff) as i32
        } else {
            i32::from_bytes(stream)?
        };
        Ok(CompositionOffsetEntry { count, offset })
    }

    fn min_size() -> usize {
        8
    }
}

impl ToBytes for CompositionOffsetEntry {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        self.count.to_bytes(stream)?;
        self.offset.to_bytes(stream)?;
        Ok(())
    }
}

impl FullBox for CompositionOffsetEntry {
    fn version(&self) -> Option<u8> {
        if self.offset < 0 {
            Some(1)
        } else {
            None
        }
    }
}

/// 8.8.3.1 Sample Flags (ISO/IEC 14496-12:2015(E))
///
/// For the first four fields, see 8.6.4.3 (Semantics).
/// The sample_is_non_sync_sample field  provides the same information as the sync sample table [8.6.2].
#[derive(Debug, Default)]
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

macro_rules! define_array {
    ($(#[$outer:meta])* $name:ident, $sizetype:ty, $nosize:expr) => {

        $(#[$outer])*
        pub struct $name<T> {
            pub vec: Vec<T>,
            nosize: bool,
        }

        impl<T> $name<T> {
            /// See Vec::new()
            pub fn new() -> $name<T> {
                $name {
                    vec: Vec::<T>::new(),
                    nosize: $nosize,
                }
            }

            /// See Vec::push()
            pub fn push(&mut self, value: T) {
                self.vec.push(value)
            }

            /// See Vec::len()
            pub fn len(&self) -> usize {
                self.vec.len()
            }
        }

        impl<T> FromBytes for $name<T> where T: FromBytes {

            fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
                let count = if $nosize {
                    std::u32::MAX as usize
                } else {
                    <$sizetype>::from_bytes(stream)? as usize
                };
                let mut v = Vec::<T>::new();
                let min_size = T::min_size() as u64;
                while ($nosize || v.len() < count) && stream.left() >= min_size && stream.left() > 0 {
                    v.push(T::from_bytes(stream)?);
                }
                Ok($name {
                    vec: v,
                    nosize: $nosize,
                })
            }
            fn min_size() -> usize {
                if $nosize {
                    T::min_size()
                } else {
                    <$sizetype>::min_size()
                }
            }
        }

        impl<T> ToBytes for $name<T> where T: ToBytes {
            fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
                if !self.nosize {
                    (self.vec.len() as $sizetype).to_bytes(stream)?;
                }
                for elem in &self.vec {
                    elem.to_bytes(stream)?;
                }
                Ok(())
            }
        }

        impl<T> FullBox for $name<T> where T: FullBox {
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

        // Debug implementation that delegates to the inner Vec.
        impl<T> Debug for $name<T> where T: Debug {
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

        impl<T> std::ops::Deref for $name<T> {
            type Target = [T];

            fn deref(&self) -> &[T] {
                std::ops::Deref::deref(&self.vec)
            }
        }

        impl<'a, T> IntoIterator for &'a $name<T> {
            type Item = &'a T;
            type IntoIter = std::slice::Iter<'a, T>;

            fn into_iter(self) -> std::slice::Iter<'a, T> {
                self.iter()
            }
        }
    }
}

define_array!(
    /// Array with 16 bits length-prefix.
    ///
    /// In serialized form the array starts with a 16 bit field that
    /// indicates the number of elements, followed by the elements itself.
    ArraySized16,
    u16,
    false
);

define_array!(
    /// Array with 32 bits length-prefix.
    ///
    /// In serialized form the array starts with a 32 bit field that
    /// indicates the number of elements, followed by the elements itself.
    ArraySized32,
    u32,
    false
);

define_array!(
    /// Array with no length prefix.
    ///
    /// It simply stretches to the end of the containing box.
    ArrayUnsized,
    u32,
    true
);

macro_rules! fixed_float {
    ($(#[$outer:meta])* $name:ident, $type:tt, $frac_bits:expr) => {
        #[derive(Clone, Copy)]
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

def_struct! {
    /// OpColor
    OpColor,
        red:    u16,
        green:  u16,
        blue:   u16,
}

def_struct! { EditListEntry,
    segment_duration:   VersionSizedUint,
    media_time: u32,
    media_rate: FixedFloat16_16,
}

impl FullBox for EditListEntry {
    fn version(&self) -> Option<u8> {
        self.segment_duration.version()
    }
}

def_struct! { TimeToSampleEntry,
    count:  u32,
    delta:  u32,
}

def_struct! { SampleToChunkEntry,
    first_chunk:                u32,
    samples_per_chunk:          u32,
    sample_description_index:   u32,
}

def_struct! { SampleToGroupEntry,
    sample_count:               u32,
    group_description_index:    u32,
}
