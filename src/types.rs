/// Common types, traits and macros used in and for Atoms.
///
/// This module contains fundamental types used in Atoms (such as Time,
/// ZString, IsoLanguageCode, etc), and also traits and macros to
/// serialize / deserialize Atoms.
///
use std::any::Any;
use std::convert::TryInto;
use std::fmt::{Debug, Display};
use std::io;
use std::time::{Duration, SystemTime};

use chrono::{self, offset::{Local, TimeZone}};

use crate::io::{ReadBytes, WriteBytes};

/// Trait to serialize and deserialize a type.
pub trait FromToBytes {
    fn from_bytes<R: ReadBytes>(bytes: &mut R) -> Self;
    fn to_bytes<W: WriteBytes>(&self, bytes: &mut W);
    fn min_size() -> usize;
}

// Convenience macro to implement FromToBytes for u* types.
macro_rules! def_from_to_bytes {
    ($type:ident) => {
        impl FromToBytes for $type {
            fn from_bytes<R: ReadBytes>(bytes: &mut R) -> Self {
                let sz = std::mem::size_of::<$type>();
                let data = bytes.read(sz as u64).unwrap();
                $type::from_be_bytes(data.try_into().unwrap())
            }
            fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) {
                bytes.write(&self.to_be_bytes()[..]).unwrap()
            }
            fn min_size() -> usize {
                std::mem::size_of::<$type>()
            }
        }
    }
}

// Define FromToBytes for u* types.
def_from_to_bytes!(u8);
def_from_to_bytes!(u16);
def_from_to_bytes!(u32);
def_from_to_bytes!(u64);
def_from_to_bytes!(u128);

// Convenience macro to implement FromToBytes for newtypes wrapping u* types.
macro_rules! def_from_to_bytes_newtype {
    ($newtype:ident, $type:ident) => {
        impl FromToBytes for $newtype {
            fn from_bytes<R: ReadBytes>(bytes: &mut R) -> Self {
                let res = $type::from_bytes(bytes);
                $newtype(res)
            }
            fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) {
                self.0.to_bytes(bytes);
            }
            fn min_size() -> usize {
                $type::min_size()
            }
        }
    }
}

// U24 Only used internally, in def_struct we use "u24" and it's stored as a u32.
#[derive(Clone, Copy)]
pub(crate) struct U24(pub u32);

impl FromToBytes for U24 {
    fn from_bytes<R: ReadBytes>(bytes: &mut R) -> Self {
        let data = bytes.read(3).unwrap();
        let mut buf = [0u8; 4];
        (&mut buf[1..]).copy_from_slice(&data);
        U24(u32::from_be_bytes(buf))
    }
    fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) {
        bytes.write(&self.0.to_be_bytes()[1..]).unwrap();
    }
    fn min_size() -> usize { 3 }
}

/// Time is a 32 bit value, measured in seconds since 01-01-1904 00:00:00
#[derive(Clone, Copy)]
pub struct Time(pub u32);
def_from_to_bytes_newtype!(Time, u32);

// TZ=UTC date +%s -d "1904-01-01 00:00:00"
const OFFSET_TO_UNIX: u32 = 2082844800;

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

fn fmt_fourcc(fourcc: u32) -> String {
    let c = fourcc.to_be_bytes();
    for i in 0..3 {
        if c[i] < 32 || c[i] > 126 {
            return format!("0x{:x}", fourcc);
        }
    }
    let mut v = vec![ b'\"' ];
    v.extend_from_slice(&c[..]);
    v.push(b'"');
    String::from_utf8(v).unwrap()
}

impl Debug for FourCC {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", fmt_fourcc(self.0))
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

