/// Definitions of types used in mp4 boxes.
///
/// This module contains fundamental types used in boxes (such as Time,
/// ZString, IsoLanguageCode, etc).
///
use std::fmt::{Debug, Display};
use std::time::{Duration, SystemTime};

use chrono::{self, offset::{Local, TimeZone}};

use crate::fromtobytes::{FromToBytes, U24};
use crate::io::{ReadBytes, WriteBytes};

// Convenience macro to implement FromToBytes for newtypes.
macro_rules! def_from_to_bytes_newtype {
    ($newtype:ident, $type:ty) => {
        impl FromToBytes for $newtype {
            fn from_bytes<R: ReadBytes>(bytes: &mut R) -> Self {
                let res = <$type>::from_bytes(bytes);
                $newtype(res)
            }
            fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) {
                self.0.to_bytes(bytes);
            }
            fn min_size() -> usize {
                <$type>::min_size()
            }
        }
    }
}

macro_rules! def_from_to_bytes_versioned {
    ($newtype:ident) => {
        impl FromToBytes for $newtype {
            fn from_bytes<R: ReadBytes>(bytes: &mut R) -> Self {
                match bytes.get_version() {
                    1 => $newtype(u64::from_bytes(bytes)),
                    _ => $newtype(u32::from_bytes(bytes) as u64),
                }
            }
            fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) {
                match bytes.get_version() {
                    1 => self.0.to_bytes(bytes),
                    _ => (std::cmp::min(self.0, std::u32::MAX as u64) as u32).to_bytes(bytes),
                }
            }
            fn min_size() -> usize {
                u32::min_size()
            }
        }
    }
}

/// Version is a magic variable. Every time you get or set
/// it, it changes the version on the underlying source
/// so that everything _after_ it is interpreted as that version.
#[derive(Clone, Copy, Debug)]
pub struct Version(u8);

impl FromToBytes for Version {
    fn from_bytes<R: ReadBytes>(bytes: &mut R) -> Self {
        let version = u8::from_bytes(bytes);
        bytes.set_version(version);
        Version(version)
    }
    fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) {
        self.0.to_bytes(bytes);
        bytes.set_version(self.0);
    }
    fn min_size() -> usize {
        u8::min_size()
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
    fn fmt_fourcc(&self, dbg: bool) -> String {
        let c = self.0.to_be_bytes();
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

impl FromToBytes for ZString {
    fn from_bytes<R: ReadBytes>(bytes: &mut R) -> Self {
        let data = bytes.read(0).unwrap();
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
        ZString(s)
    }
    fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) {
        let mut v = Vec::new();
        for c in self.0.chars() {
            if (c as u32) < 256 {
                v.push(c as u8);
            } else {
                v.push(0xff);
            }
        }
        bytes.write(&v).unwrap();
    }
    fn min_size() -> usize { 0 }
}


impl Display for ZString {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Debug for ZString {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Matrix.
pub struct Matrix([[u32; 3]; 3]);

impl FromToBytes for Matrix {
    fn from_bytes<R: ReadBytes>(bytes: &mut R) -> Self {
        let mut m = [[0u32; 3]; 3];
        for x in 0 ..3 {
            for y in 0..3 {
                m[x][y] = u32::from_bytes(bytes);
            }
        }
        Matrix(m)
    }
    fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) {
        for x in 0 ..3 {
            for y in 0..3 {
                let n = (self.0)[x][y].to_be_bytes();
                bytes.write(&n[..]).unwrap();
            }
        }
    }
    fn min_size() -> usize { 36 }
}

impl Debug for Matrix {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Matrix([{:x}][{:x}][{:x}] [{:x}][{:x}][{:x}] [{:x}][{:x}][{:x}])",
               (self.0)[0][0], (self.0)[0][1], (self.0)[0][2],
               (self.0)[1][0], (self.0)[1][1], (self.0)[1][2],
               (self.0)[2][0], (self.0)[2][1], (self.0)[2][2],
        )
    }
}

/// TrackFlags..
#[derive(Clone, Copy)]
pub struct TrackFlags(U24);
def_from_to_bytes_newtype!(TrackFlags, U24);

impl TrackFlags {
    fn get(&self, bit: u32) -> bool {
        ((self.0).0 & bit) > 0
    }
    pub fn set(&mut self, bit: u32, on: bool) {
        if on {
            (self.0).0 |= bit;
        } else {
            (self.0).0 &= !bit;
        }
    }
    pub fn get_enabled(&self) -> bool { self.get(0x0001) }
    pub fn set_enabled(&mut self, on: bool) { self.set(0x0001, on) }
    pub fn get_in_movie(&self) -> bool { self.get(0x0002) }
    pub fn set_in_movie(&mut self, on: bool) { self.set(0x0002, on) }
    pub fn get_in_preview(&self) -> bool { self.get(0x0004) }
    pub fn set_in_preview(&mut self, on: bool) { self.set(0x0004, on) }
    pub fn get_in_poster(&self) -> bool { self.get(0x0008) }
    pub fn set_in_poster(&mut self, on: bool) { self.set(0x0008, on) }
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

macro_rules! fixed_float {
    ($name:ident, $type:ty) => {
        #[derive(Clone, Copy)]
        pub struct $name($type);
        def_from_to_bytes_newtype!($name, $type);

        impl $name {
            fn get(&self) -> f64 {
                let max = 1 << (std::mem::size_of::<$type>() * 4);
                (self.0 as f64) / (max as f64)
            }

            #[allow(dead_code)]
            pub fn set(&mut self, value: f64) {
                let max = (1 << (std::mem::size_of::<$type>() * 4)) as f64;
                self.0 = if value <= 0f64 {
                    0 as $type
                } else if value >= (max as f64) {
                    ((max * max) - 1f64) as $type
                } else {
                    (value * max) as $type
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
fixed_float!(FixedFloat32, u32);
fixed_float!(FixedFloat16, u16);

def_struct!{ EditList,
    duration:   u32,
    media_time: u32,
    media_rate: u32,
}

def_struct!{ OpColor,
    red:    u16,
    green:  u16,
    blue:   u16,
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

impl FromToBytes for AppleItem {
    fn from_bytes<R: ReadBytes>(bytes: &mut R) -> Self {
        // First read this box (e.g. an "©too").
        let mut size = u32::from_bytes(bytes);
        let fourcc = FourCC::from_bytes(bytes);
        if size < 8 {
            size = 8;
        }
        println!("XXX 1 size {} fourcc {}", size, fourcc);
        let mut res = AppleItem{
            fourcc,
            data: String::new(),
            blob: bytes.read((size - 8) as u64).unwrap().to_vec(),
        };
        let mut blob_slice = &res.blob[..];
        let data = &mut blob_slice;

        // Now read the sub-box. Again, length + fourcc.
        let size = u32::from_bytes(data);
        let fourcc = FourCC::from_bytes(data);

        if fourcc.to_string() == "data" {
            data.skip(2).unwrap();
            let flag = u16::from_bytes(data);
            if flag == 1 && size >= 16 {
                data.skip(4).unwrap();
                let text = data.read((size - 16)  as u64).unwrap();
                res.data = String::from_utf8_lossy(text).to_string();
            }
        }
        res
    }

    fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) {
        if self.data.len() == 0 {
            // No string data, just write the blob,
            // i.e. write back what we read before.
            if self.blob.len() > 0 {
                let size = (8 + self.blob.len()) as u32;
                size.to_bytes(bytes);
                self.fourcc.to_bytes(bytes);
                bytes.write(&self.blob[..]).unwrap();
            }
            return;
        }

        // Write the main box (e.g. ©too).
        let mut size = (24 + self.data.len()) as u32;
        size.to_bytes(bytes);
        self.fourcc.to_bytes(bytes);

        // Now write the data sub-box header (16 bytes)
        size -= 8;
        size.to_bytes(bytes);
        bytes.write(b"data").unwrap();
        bytes.skip(2).unwrap();
        1u16.to_bytes(bytes);
        bytes.skip(4).unwrap();

        // And finally the data itself.
        bytes.write(self.data.as_bytes()).unwrap();
    }

    fn min_size() -> usize { 16 }
}

impl Debug for AppleItem {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.data.len() {
            0 => write!(f, "{}: [{} bytes]", self.fourcc, self.blob.len()),
            _ => write!(f, "{}: \"{}\"", self.fourcc, self.data),
        }
    }
}

