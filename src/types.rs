/// Definitions of types used in mp4 boxes.
///
/// This module contains fundamental types used in boxes (such as Time,
/// ZString, IsoLanguageCode, etc).
///
use std::convert::TryInto;
use std::fmt::{Debug, Display};
use std::io;
use std::time::{Duration, SystemTime};

use chrono::{self, offset::{Local, TimeZone}};

use crate::fromtobytes::{FromBytes, ToBytes, ReadBytes, WriteBytes};

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
    }
}

macro_rules! def_from_to_bytes_versioned {
    ($newtype:ident) => {
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
                    _ => (std::cmp::min(self.0, std::u32::MAX as u64) as u32).to_bytes(bytes)?,
                }
                Ok(())
            }
        }
    }
}

/// Version is a magic variable. Every time you get or set
/// it, it changes the version on the underlying source
/// so that everything _after_ it is interpreted as that version.
#[derive(Clone, Copy)]
pub struct Version(u8);

impl FromBytes for Version {
    fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self> {
        let version = u8::from_bytes(bytes)?;
        bytes.set_version(version);
        Ok(Version(version))
    }
    fn min_size() -> usize {
        u8::min_size()
    }
}
impl ToBytes for Version {
    fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) -> io::Result<()> {
        self.0.to_bytes(bytes)?;
        bytes.set_version(self.0);
        Ok(())
    }
}

impl Debug for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
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

    fn min_size() -> usize { 16 }
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
        write!(f, "{:08x}-{:04x}-{:04x}-{:04x}-{:04x}{:08x}", p1, p2, p3, p4, p5, p6)
    }
}

impl Debug for Uuid {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "\"{}\"", self)
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
#[derive(Clone, Copy)]
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
    fn min_size() -> usize { 0 }
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
        for x in 0 ..3 {
            m[x] = (
                FixedFloat16_16::from_bytes(bytes)?,
                FixedFloat16_16::from_bytes(bytes)?,
                FixedFloat2_30::from_bytes(bytes)?
            );
        }
        Ok(Matrix(m))
    }
    fn min_size() -> usize { 36 }
}
impl ToBytes for Matrix {
    fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) -> io::Result<()> {
        for x in 0 ..3 {
            (self.0)[x].0.to_bytes(bytes)?;
            (self.0)[x].1.to_bytes(bytes)?;
            (self.0)[x].2.to_bytes(bytes)?;
        }
        Ok(())
    }
}

impl Debug for Matrix {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Matrix([{}][{}][{}] [{}][{}][{}] [{}][{}][{}])",
               (self.0)[0].0, (self.0)[0].1, (self.0)[0].2,
               (self.0)[1].0, (self.0)[1].1, (self.0)[1].2,
               (self.0)[2].0, (self.0)[2].1, (self.0)[2].2,
        )
    }
}

macro_rules! impl_flags {
    ($type:tt) => {

        /// 24 bits of flags.
        #[derive(Clone, Copy)]
        pub struct $type(pub u32);

        impl FromBytes for $type {
            fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self> {
                let data = bytes.read(3)?;
                let mut buf = [0u8; 4];
                (&mut buf[1..]).copy_from_slice(&data);
                Ok($type(u32::from_be_bytes(buf)))
            }
            fn min_size() -> usize { 3 }
        }

        impl ToBytes for $type {
            fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) -> io::Result<()> {
                bytes.write(&self.0.to_be_bytes()[1..])
            }
        }


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
    }
}

impl_flags!(Flags);
impl Debug for Flags {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Flags(0x{:x})", self.0)
    }
}

impl_flags!(TrackFlags);

impl TrackFlags {
    pub fn get_enabled(&self) -> bool { self.get(0) }
    pub fn set_enabled(&mut self, on: bool) { self.set(0, on) }
    pub fn get_in_movie(&self) -> bool { self.get(1) }
    pub fn set_in_movie(&mut self, on: bool) { self.set(1, on) }
    pub fn get_in_preview(&self) -> bool { self.get(2) }
    pub fn set_in_preview(&mut self, on: bool) { self.set(2, on) }
    pub fn get_in_poster(&self) -> bool { self.get(3) }
    pub fn set_in_poster(&mut self, on: bool) { self.set(3, on) }
}

impl Debug for TrackFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut v = vec![ "[" ];
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

/// Apple item.
///
/// We do our best to decode the text in an item, and that's
/// it. If we fail, just skip it. Probably the best since this
/// is not part of the ISO standard.
pub struct AppleItem {
    pub fourcc: FourCC,
    pub data:   String,
    blob:       Vec<u8>,
}

impl FromBytes for AppleItem {
    fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self> {
        // First read this box (e.g. an "©too").
        let mut size = u32::from_bytes(bytes)?;
        let fourcc = FourCC::from_bytes(bytes)?;
        if size < 8 {
            size = 8;
        }
        debug!("XXX 1 size {} fourcc {}", size, fourcc);
        let mut res = AppleItem{
            fourcc,
            data: String::new(),
            blob: bytes.read((size - 8) as u64)?.to_vec(),
        };
        let mut blob_slice = &res.blob[..];
        let data = &mut blob_slice;

        // Now read the sub-box. Again, length + fourcc.
        let size = u32::from_bytes(data)?;
        let fourcc = FourCC::from_bytes(data)?;

        if fourcc.to_string() == "data" {
            ReadBytes::skip(data, 2)?;
            let flag = u16::from_bytes(data)?;
            if flag == 1 && size >= 16 {
                ReadBytes::skip(data, 4)?;
                let text = data.read((size - 16)  as u64)?;
                res.data = String::from_utf8_lossy(text).to_string();
            }
        }
        Ok(res)
    }

    fn min_size() -> usize { 16 }
}

impl ToBytes for AppleItem {
    fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) -> io::Result<()> {
        if self.data.len() == 0 {
            // No string data, just write the blob,
            // i.e. write back what we read before.
            if self.blob.len() > 0 {
                let size = (8 + self.blob.len()) as u32;
                size.to_bytes(bytes)?;
                self.fourcc.to_bytes(bytes)?;
                bytes.write(&self.blob[..])?;
            }
            return Ok(());
        }

        // Write the main box (e.g. ©too).
        let mut size = (24 + self.data.len()) as u32;
        size.to_bytes(bytes)?;
        self.fourcc.to_bytes(bytes)?;

        // Now write the data sub-box header (16 bytes)
        size -= 8;
        size.to_bytes(bytes)?;
        bytes.write(b"data")?;
        bytes.skip(2)?;
        1u16.to_bytes(bytes)?;
        bytes.skip(4)?;

        // And finally the data itself.
        bytes.write(self.data.as_bytes())
    }

}

impl Debug for AppleItem {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.data.len() {
            0 => write!(f, "{}: [{} bytes]", self.fourcc, self.blob.len()),
            _ => write!(f, "{}: \"{}\"", self.fourcc, self.data),
        }
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
        Ok(CompositionOffsetEntry {
            count,
            offset,
        })
    }

    fn min_size() -> usize { 8 }
}

impl ToBytes for CompositionOffsetEntry {
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        self.count.to_bytes(stream)?;
        self.offset.to_bytes(stream)?;
        if self.offset < 0 {
            stream.set_version(1);
        }
        Ok(())
    }
}

macro_rules! fixed_float {
    ($name:ident, $type:tt, $frac_bits:expr) => {
        #[derive(Clone, Copy)]
        pub struct $name($type);
        def_from_to_bytes_newtype!($name, $type);

        impl $name {
            fn get(&self) -> f64 {
                (self.0 as f64) / ((1 << $frac_bits) as f64)
            }

            #[allow(dead_code)]
            pub fn set(&mut self, value: f64) {
                let v = (value * (( 1 << $frac_bits) as f64)).round();
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
    }
}

// Some fixed float types.
fixed_float!(FixedFloat2_30, u32, 30);
fixed_float!(FixedFloat16_16, u32, 16);
fixed_float!(FixedFloat8_8, u16, 8);

def_struct!{ OpColor,
    red:    u16,
    green:  u16,
    blue:   u16,
}

def_struct!{ EditListEntry,
    duration:   u32,
    media_time: u32,
    media_rate: FixedFloat16_16,
}

def_struct!{ TimeToSampleEntry,
    count:  u32,
    delta:  u32,
}

def_struct!{ SampleToChunkEntry,
    first_chunk:                u32,
    samples_per_chunk:          u32,
    sample_description_index:   u32,
}

def_struct!{ SampleToGroupEntry,
    sample_count:               u32,
    group_description_index:    u32,
}

