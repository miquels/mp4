//! Basic serializer / deserializer.
//!
//! The FromBytes/ToBytes traits and the def_struct! macro are defined here.
//!
//! It also  contains the FromBytes/ToBytes implementations for the
//! primitive types u8/u16/u32/u64/u128.
//!
use std::convert::TryInto;
use std::fs;
use std::io::{self, ErrorKind::UnexpectedEof, Seek, SeekFrom, Write};

use auto_impl::auto_impl;

use crate::io::DataRef;
use crate::types::FourCC;

/// Byte reader in a stream.
#[auto_impl(&mut)]
pub trait ReadBytes: BoxBytes {
    /// Read an exact number of bytes, return a reference to the buffer.
    fn read(&mut self, amount: u64) -> io::Result<&[u8]>;

    /// Read an exact number of bytes, but don't advance position.
    fn peek(&mut self, amount: u64) -> io::Result<&[u8]>;

    /// Skip some bytes in the input.
    fn skip(&mut self, amount: u64) -> io::Result<()>;

    /// How much data is left?
    fn left(&mut self) -> u64;
}

/// Byte writer in a stream.
#[auto_impl(&mut)]
pub trait WriteBytes: BoxBytes {
    /// Write an exact number of bytes.
    fn write(&mut self, data: &[u8]) -> io::Result<()>;
    /// Zero-fill some bytes in the output.
    fn skip(&mut self, amount: u64) -> io::Result<()>;
}

/// A bunch of optional methods for reading/writing boxes rather than
/// simple structs. All the methods have defaults.
#[auto_impl(&mut)]
pub trait BoxBytes {
    /// Get current position in the stream.
    fn pos(&mut self) -> u64 {
        unimplemented!()
    }
    /// Seek to a position in the output stream.
    fn seek(&mut self, _pos: u64) -> io::Result<()> {
        unimplemented!()
    }
    /// Size of the file.
    fn size(&self) -> u64 {
        unimplemented!()
    }
    /// Get version metadata.
    fn version(&self) -> u8 {
        0
    }
    /// Get flags metadata.
    fn flags(&self) -> u32 {
        0
    }
    /// Get last FourCC we read.
    fn fourcc(&self) -> FourCC {
        unimplemented!()
    }
    /// Get a reference to the mdat source data.
    fn data_ref(&self, _size: u64) -> io::Result<DataRef> {
        panic!("data reference unavailable");
    }
    /// Name of the input file.
    fn input_filename(&self) -> Option<&str> {
        None
    }
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

    fn peek(&mut self, amount: u64) -> io::Result<&[u8]> {
        let mut amount = amount as usize;
        if amount > (*self).len() {
            return Ok(&b""[..]);
        }
        if amount == 0 {
            amount = self.len();
        }
        Ok(&self[0..amount])
    }
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        let mut amount = amount;
        if amount > (*self).len() as u64 {
            amount = self.len() as u64;
        }
        (*self) = &self[amount as usize..];
        Ok(())
    }

    #[inline]
    fn left(&mut self) -> u64 {
        (*self).len() as u64
    }
}

impl BoxBytes for &[u8] {
    fn data_ref(&self, _size: u64) -> io::Result<DataRef> {
        panic!("&[u8]: data reference unavailable");
    }
    fn size(&self) -> u64 {
        self.len() as u64
    }
    fn pos(&mut self) -> u64 {
        0
    }
}

impl WriteBytes for fs::File {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.write_all(data)
    }

    fn skip(&mut self, amount: u64) -> io::Result<()> {
        Seek::seek(self, SeekFrom::Current(amount as i64))?;
        Ok(())
    }
}

impl BoxBytes for fs::File {
    fn pos(&mut self) -> u64 {
        Seek::seek(self, SeekFrom::Current(0)).unwrap()
    }

    fn seek(&mut self, pos: u64) -> io::Result<()> {
        Seek::seek(self, SeekFrom::Start(pos))?;
        Ok(())
    }

    fn size(&self) -> u64 {
        self.metadata().unwrap().len()
    }
}

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

impl BoxBytes for &mut [u8] {
    fn data_ref(&self, _size: u64) -> io::Result<DataRef> {
        panic!("&mut [u8]: data reference unavailable");
    }
}

/// Trait to deserialize a type.
pub trait FromBytes {
    fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self>
    where
        Self: Sized;
    fn min_size() -> usize;
}

/// Trait to serialize a type.
pub trait ToBytes {
    fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) -> io::Result<()>;
}

/*
thread_local!(static MP4BOX: RefCell<Vec<Mp4Box>> = RefCell::new(Vec::new());
macro_rules set_vec! {
    (Mp4Box, $field:ident, $value:expr) => {
        MP4BOX.with(|v| {
            *f.borrow_mut() = v;
        });
    },
    ($_type:ty, $field:ident, $value:expr) => {
        let $field = $value;
    }
}
*/

// Convenience macro to implement FromBytes/ToBytes for u* types.
macro_rules! def_from_to_bytes {
    ($type:ident) => {
        impl FromBytes for $type {
            #[inline]
            fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self> {
                let sz = std::mem::size_of::<$type>();
                let data = bytes.read(sz as u64)?;
                let data = data.try_into().map_err(|_| UnexpectedEof)?;
                Ok($type::from_be_bytes(data))
            }
            #[inline]
            fn min_size() -> usize {
                std::mem::size_of::<$type>()
            }
        }
        impl ToBytes for $type {
            #[inline]
            fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) -> io::Result<()> {
                bytes.write(&self.to_be_bytes()[..])
            }
        }
    };
}

// Define FromBytes/ToBytes for u* types.
def_from_to_bytes!(u8);
def_from_to_bytes!(i16);
def_from_to_bytes!(u16);
def_from_to_bytes!(i32);
def_from_to_bytes!(u32);
def_from_to_bytes!(i64);
def_from_to_bytes!(u64);
def_from_to_bytes!(u128);

/// Generic implementation for Vec<T>
impl<T> FromBytes for Vec<T>
where
    T: FromBytes,
{
    fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
        let mut v = Vec::new();
        let min_size = T::min_size() as u64;
        while stream.left() >= min_size && stream.left() > 0 {
            v.push(T::from_bytes(stream)?);
        }
        Ok(v)
    }
    fn min_size() -> usize {
        0
    }
}

/// Generic implementation for Vec<T>
impl<T> ToBytes for Vec<T>
where
    T: ToBytes,
{
    fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
        for elem in self {
            elem.to_bytes(stream)?;
        }
        Ok(())
    }
}

/// A macro to define a struct and implement the FromBytes/ToBytes traits for it.
///
/// Usage:
///
/// ```text
/// def_struct! { Name,
///     field1:     u32,        // primitive type
///     field3:     Time,       // struct that also implements FromBytes/ToBytes
///     skip:       8,          // skip 8 bytes here while serializing / deserializing.
///     ....
/// }
/// ```
//#[macro_export]
macro_rules! def_struct {
    // minimum size for a certain type. we hard-code u* here.
    (@min_size [ $($tt:tt)* ]) => { $($tt)* };
    (@min_size u8) => { 1 };
    (@min_size u16) => { 2 };
    (@min_size u32) => { 4 };
    (@min_size i32) => { 4 };
    (@min_size u64) => { 8 };
    (@min_size u128) => { 16 };
    (@min_size Vec<$tt:tt>) => { 0 };
    (@min_size ArraySized32<$gen:tt>) => { 4 };
    (@min_size ArraySized16<$gen:tt>) => { 2 };
    (@min_size ArrayUnsized<$gen:tt>) => { 0 };
    (@min_size DataRefSized32<$gen:tt>) => { 4 };
    (@min_size DataRefSized16<$gen:tt>) => { 2 };
    (@min_size DataRefUnsized<$gen:tt>) => { 0 };
    (@min_size ListSized32<$gen:tt>) => { 4 };
    (@min_size ListSized16<$gen:tt>) => { 2 };
    (@min_size ListUnsized<$gen:tt>) => { 0 };
    (@min_size [ $_type:ty ]) => { 0 };
    (@min_size ( $_type:ty )) => { 0 };
    (@min_size { $_type:ty }) => { 0 };
    (@min_size $type:ty) => {
        <$type>::min_size()
    };
    (@min_size $amount:expr) => { $amount };
    (@min_size $($tt:tt)*) => { compile_error!(stringify!($($tt)*)); };

    // Extract a box from the boxes array.
    (@EXTRACT $name:ident, $type:ident) => {
        |boxes: &mut Vec<MP4Box>| {
            use io::ErrorKind::InvalidData;
            let idx = boxes.iter().enumerate().find_map(|(i, b)| {
                match b {
                    &$crate::boxes::MP4Box::$type(..) => Some(i),
                    _ => None,
                }
            }).ok_or_else(|| io::Error::new(InvalidData, format!("{}: missing {}", $name, stringify!($type))))?;
            let b = boxes.remove(idx);
            match b {
                $crate::boxes::MP4Box::$type(b) => Ok(b),
                _ => unreachable!(),
            }
        }
    };

    // @def_struct: Define a struct line by line using accumulation and recursion.
    (@def_struct $(#[$outer:meta])* $name:ident, $( $field:tt: $type:tt $(<$gen:tt>)? ),* $(,)?) => {
        def_struct!(@def_struct_ [$(#[$outer])* $name], [ $( $field: $type $(<$gen>)?, )* ] -> []);
    };
    // During definition of the struct, we skip all the "skip" defitions.
    (@def_struct_ $info:tt, [ skip: $amount:tt, $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $info, [$($tt)*] -> [ $($res)* ]);
    };
    // Add field with type in curlies.
    (@def_struct_ $info:tt, [ $field:ident: { $type:ty }, $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $info, [$($tt)*] -> [ $($res)* pub $field: $type, ]);
    };
    // Add normal field.
    (@def_struct_ $info:tt, [ $field:ident: $type:ty, $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $info, [$($tt)*] -> [ $($res)* pub $field: $type, ]);
    };
    // Final.
    (@def_struct_ [$(#[$outer:meta])* $name:ident], [] -> [ $($res:tt)* ]) => {
        $(#[$outer])*
        pub struct $name { $(
            $res
        )* }
    };

    // @from_bytes: Generate the from_bytes details for a struct.
    (@from_bytes $name:ident, $base:tt, $stream:tt, $( $field:tt: $type:tt $(<$gen:tt>)? ),* $(,)?) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $( $field: $type $(<$gen>)?, )* ] -> [] [] []);
    };
    // Insert a skip instruction.
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ skip: $amount:tt, $($tt:tt)*]
        -> [ $($set:tt)* ] $set2:tt [ $($fields:tt)* ] ) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [ $stream.skip($amount).unwrap(); ] ] $set2 [$($fields)*]);
    };
    // Set a field with type in curlies.
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ $field:tt: { $type:ty } $(<$gen:tt>)?, $($tt:tt)*]
        -> [ $($set:tt)* ] $set2:tt [ $($fields:tt)* ]) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [ let $field = <$type>::from_bytes($stream)?; ] ] $set2 [ $($fields)* $field ]);
    };
    // Set a field.
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ $field:tt: $type:tt $(<$gen:tt>)?, $($tt:tt)*]
        -> [ $($set:tt)* ] $set2:tt [ $($fields:tt)* ]) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [ let $field = <$type $(<$gen>)?>::from_bytes($stream)?; ] ] $set2 [ $($fields)* $field ]);
    };
    // Final.
    (@from_bytes_ $name:ident, [ $($base:tt)* ], $_stream:tt, [] -> [ $([$($set:tt)*])* ] [ $([$($set2:tt)*])* ] [ $($field:tt)* ]) => {
        Ok({
        $(
            $($set)*
        )*
        $(
            $($set2)*(&mut boxes)?;
        )*
        $name {
            $(
                $field,
            )*
        } })
    };

    // @to_bytes: Generate the to_bytes details for a struct.
    (@to_bytes $struct:expr, $stream:ident, $( $field:tt: $type:tt $(<$gen:tt>)? ),* $(,)?) => {
        {
            $(
                def_struct!(@to_bytes_ $struct, $stream, $field: $type $(<$gen>)?);
            )*
            Ok(())
        }
    };
    // Insert a skip instruction.
    (@to_bytes_ $struct:expr, $stream:ident, skip: $amount:tt) => {
        $stream.skip($amount)?;
    };
    // Write a field value.
    (@to_bytes_ $struct:expr, $stream:ident, $field:tt: $type:tt $(<$gen:tt>)?) => {
        $struct.$field.to_bytes($stream)?;
    };

    // Helpers for skip
    (@filter_skip skip, $($tt:tt)*) => {};
    (@filter_skip $field:ident, $($tt:tt)*) => { $($tt)* };

    // Main entry point to define just one struct.
    ($(#[$outer:meta])* $name:ident, $($field:tt: $type:tt $(<$gen:tt>)?),* $(,)?) => {
        def_struct!(@def_struct $(#[$outer])* #[derive(Clone)] $name,
            $(
                $field: $type $(<$gen>)?,
            )*
        );

        // Debug implementation that skips "skip"
        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                let mut dbg = f.debug_struct(stringify!($name));
                $(
                    def_struct!(@filter_skip $field, dbg.field(stringify!($field), &self.$field););
                )*
                dbg.finish()
            }
        }

        impl FromBytes for $name {
            fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
                def_struct!(@from_bytes $name, [], stream, $(
                    $field: $type $(<$gen>)?,
                )*)
            }

            fn min_size() -> usize {
                $( def_struct!(@min_size $type $(<$gen>)?) + )* 0
            }
        }

        impl ToBytes for $name {
            fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
                def_struct!(@to_bytes self, stream, $(
                    $field: $type $(<$gen>)?,
                )*)
            }

        }

    };

    // Alternative entry point.
    ($(#[$outer:meta])* $name:ident { $($tt:tt)* }) => {
        def_struct!($(#[$outer])* $name, $($tt)*);
    }
}
