//! Basic serializer / deserializer.
//!
//! The FromBytes/ToBytes traits and the def_struct! macro are defined here.
//!
//! It also  contains the FromBytes/ToBytes implementations for the
//! primitive types u8/u16/u32/u64/u128.
//!
use std::convert::TryInto;
use std::io::{self, ErrorKind::UnexpectedEof};

use auto_impl::auto_impl;

use crate::io::ReadAt;
use crate::types::FourCC;

/// Byte reader in a stream.
#[auto_impl(&mut)]
pub trait ReadBytes: BoxBytes {
    /// Read an exact number of bytes, return a reference to the buffer.
    fn read(&mut self, amount: u64) -> io::Result<&[u8]>;
    /// Skip some bytes in the input.
    fn skip(&mut self, amount: u64) -> io::Result<()>;
    /// How much data is left?
    fn left(&self) -> u64;
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
    fn pos(&self) -> u64 {
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
    fn mdat_ref(&self) -> Option<&dyn ReadAt> {
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

    fn skip(&mut self, amount: u64) -> io::Result<()> {
        let mut amount = amount;
        if amount > (*self).len() as u64 {
            amount = self.len() as u64;
        }
        (*self) = &self[amount as usize..];
        Ok(())
    }

    #[inline]
    fn left(&self) -> u64 {
        (*self).len() as u64
    }
}

// Uses defaults.
impl BoxBytes for &[u8] {}

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

// Uses defaults.
impl BoxBytes for &mut [u8] {}

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

// Convenience macro to implement FromBytes/ToBytes for u* types.
macro_rules! def_from_to_bytes {
    ($type:ident) => {
        impl FromBytes for $type {
            fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self> {
                let sz = std::mem::size_of::<$type>();
                let data = bytes.read(sz as u64)?;
                let data = data.try_into().map_err(|_| UnexpectedEof)?;
                Ok($type::from_be_bytes(data))
            }
            fn min_size() -> usize {
                std::mem::size_of::<$type>()
            }
        }
        impl ToBytes for $type {
            fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) -> io::Result<()> {
                bytes.write(&self.to_be_bytes()[..])
            }
        }
    };
}

// Define FromBytes/ToBytes for u* types.
def_from_to_bytes!(u8);
def_from_to_bytes!(u16);
def_from_to_bytes!(i32);
def_from_to_bytes!(u32);
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
#[macro_export]
macro_rules! def_struct {
    // minimum size for a certain type. we hard-code u* here.
    (@min_size u8) => { 1 };
    (@min_size u16) => { 2 };
    (@min_size u32) => { 4 };
    (@min_size i32) => { 4 };
    (@min_size u64) => { 8 };
    (@min_size u128) => { 16 };
    (@min_size [ $type:ty, sized ]) => { 4 };
    (@min_size [ $type:ty, sized16 ]) => { 2 };
    (@min_size [ $type:ty, sized32 ]) => { 4 };
    (@min_size [ $type:ty, unsized ]) => { 0 };
    (@min_size [ $_type:ty ]) => { 0 };
    (@min_size $type:ident) => {
        $type::min_size()
    };
    (@min_size $amount:expr) => { $amount };

    // @def_struct: Define a struct line by line using accumulation and recursion.
    (@def_struct $(#[$outer:meta])* $name:ident, $( $field:tt: $type:tt $(as $as:tt)? ),* $(,)?) => {
        def_struct!(@def_struct_ [$(#[$outer])* $name], [ $( $field: $type $(as $as)?, )* ] -> []);
    };
    // During definition of the struct, we skip all the "skip" defitions.
    (@def_struct_ $info:tt, [ skip: $amount:tt, $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $info, [$($tt)*] -> [ $($res)* ]);
    };
    // Add normal field (as).
    (@def_struct_ $info:tt, [ $field:ident: $_type:ident as $type:ident, $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $info, [$($tt)*] -> [ $($res)* pub $field: $type, ]);
    };
    // Add normal field (ArraySized16)
    (@def_struct_ $info:tt, [ $field:ident: [ $type:ty, sized16 ], $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $info, [$($tt)*] -> [ $($res)* pub $field: ArraySized16<$type>, ]);
    };
    // Add normal field (ArraySized32)
    (@def_struct_ $info:tt, [ $field:ident: [ $type:ty, sized ], $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $info, [$($tt)*] -> [ $($res)* pub $field: ArraySized32<$type>, ]);
    };
    // Add normal field (ArrayUnsized)
    (@def_struct_ $info:tt, [ $field:ident: [ $type:ty, unsized ], $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $info, [$($tt)*] -> [ $($res)* pub $field: Vec<$type>, ]);
    };
    // Add normal field (Vec)
    (@def_struct_ $info:tt, [ $field:ident: [ $type:ty ], $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $info, [$($tt)*] -> [ $($res)* pub $field: Vec<$type>, ]);
    };
    // Add normal field.
    (@def_struct_ $info:tt, [ $field:ident: $type:ident, $($tt:tt)*] -> [ $($res:tt)* ]) => {
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
    (@from_bytes $name:ident, $base:tt, $stream:tt, $( $field:tt: $type:tt $(as $as:tt)? ),* $(,)?) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $( $field: $type $(as $as)?, )* ] -> [] []);
    };
    // Insert a skip instruction.
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ skip: $amount:tt, $($tt:tt)*]
        -> [ $($set:tt)* ] [ $($fields:tt)* ] ) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [ $stream.skip($amount).unwrap(); ] ] [$($fields)*]);
    };
    // Set a field (as)
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ $field:tt: $in:tt as $out:tt, $($tt:tt)*]
        -> [ $($set:tt)* ] [ $($fields:tt)* ]) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [ let $field: $out = $in::from_bytes($stream)?.into(); ] ] [ $($fields)* $field ]);
    };
    // Set a field (ArraySized16)
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ $field:tt: [ $type:ty, sized16 ], $($tt:tt)*]
        -> [ $($set:tt)* ] [ $($fields:tt)* ]) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [ let $field = ArraySized16::<$type>::from_bytes($stream)?; ] ] [ $($fields)* $field ]);
    };
    // Set a field (ArraySized32)
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ $field:tt: [ $type:ty, sized ], $($tt:tt)*]
        -> [ $($set:tt)* ] [ $($fields:tt)* ]) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [ let $field = ArraySized32::<$type>::from_bytes($stream)?; ] ] [ $($fields)* $field ]);
    };
    // Set a field (ArrayUnsized)
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ $field:tt: [ $type:ty, unsized ], $($tt:tt)*]
        -> [ $($set:tt)* ] [ $($fields:tt)* ]) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [ let $field = Vec::<$type>::from_bytes($stream)?; ] ] [ $($fields)* $field ]);
    };
    // Set a field (Vec)
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ $field:tt: [ $type:ty ], $($tt:tt)*]
        -> [ $($set:tt)* ] [ $($fields:tt)* ]) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [ let $field = Vec::<$type>::from_bytes($stream)?; ] ] [ $($fields)* $field ]);
    };
    // Set a field.
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ $field:tt: $type:tt, $($tt:tt)*]
        -> [ $($set:tt)* ] [ $($fields:tt)* ]) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [ let $field = $type::from_bytes($stream)?; ] ] [ $($fields)* $field ]);
    };
    // Final.
    (@from_bytes_ $name:ident, [ $($base:tt)* ], $_stream:tt, [] -> [ $([$($set:tt)*])* ] [ $($field:tt)* ]) => {
        Ok({
        $(
            $($set)*
        )*
        $name {
            $($base)*
            $(
                $field,
            )*
        } })
    };

    // @to_bytes: Generate the to_bytes details for a struct.
    (@to_bytes $struct:expr, $stream:ident, $( $field:tt: $type:tt $(as $as:tt)? ),* $(,)?) => {
        {
            $(
                def_struct!(@to_bytes_ $struct, $stream, $field: $type $(as $as)?);
            )*
            Ok(())
        }
    };
    // Insert a skip instruction.
    (@to_bytes_ $struct:expr, $stream:ident, skip: $amount:tt) => {
        $stream.skip($amount)?;
    };
    // Write a field value (as)
    (@to_bytes_ $struct:expr, $stream:ident, $field:tt: $type:tt as $_type:tt) => {
        $type::from($struct.$field).to_bytes($stream)?;
    };
    // Write a field value.
    (@to_bytes_ $struct:expr, $stream:ident, $field:tt: $type:tt) => {
        $struct.$field.to_bytes($stream)?;
    };

    // Helper.
    (@filter_skip skip, $($tt:tt)*) => {};
    (@filter_skip $field:ident, $($tt:tt)*) => { $($tt)* };

    // Main entry point to define just one struct.
    ($(#[$outer:meta])* $name:ident, $($field:tt: $type:tt $(as $as:tt)?),* $(,)?) => {
        def_struct!(@def_struct $(#[$outer])* $name,
            $(
                $field: $type $(as $as)?,
            )*
        );

        // Debug implementation that skips "skip"
        impl Debug for $name {
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
                    $field: $type $(as $as)?,
                )*)
            }

            fn min_size() -> usize {
                $( def_struct!(@min_size $type) +)* 0
            }
        }

        impl ToBytes for $name {
            fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
                def_struct!(@to_bytes self, stream, $(
                    $field: $type $(as $as)?,
                )*)
            }

        }
    }
}
