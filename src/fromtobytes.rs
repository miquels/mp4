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
    /// Set version metadata.
    fn set_version(&mut self, _version: u8) {
        unimplemented!()
    }
    /// Get last FourCC we read.
    fn fourcc(&self) -> FourCC {
        unimplemented!()
    }
    /// Set last FourCC we read.
    fn set_fourcc(&mut self, _fourcc: FourCC) {
        unimplemented!()
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
    fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self> where Self: Sized;
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
    }
}

// Define FromBytes/ToBytes for u* types.
def_from_to_bytes!(u8);
def_from_to_bytes!(u16);
def_from_to_bytes!(u32);
def_from_to_bytes!(u64);
def_from_to_bytes!(u128);

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
    (@min_size u64) => { 8 };
    (@min_size u128) => { 16 };
    (@min_size [ $_type:ty $(, $cnt:ident)? ]) => { 0 };
    (@min_size $type:ident) => {
        $type::min_size()
    };
    (@min_size $amount:expr) => { $amount };

    // @def_struct: Define a struct line by line using accumulation and recursion.
    (@def_struct $name:ident, $( $field:tt: $type:tt $(as $as:tt)? ),* $(,)?) => {
        def_struct!(@def_struct_ $name, [ $( $field: $type $(as $as)?, )* ] -> []);
    };
    // During definition of the struct, we skip all the "skip" defitions.
    (@def_struct_ $name:ident, [ skip: $amount:tt, $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $name, [$($tt)*] -> [ $($res)* ]);
    };
    // Add normal field (as).
    (@def_struct_ $name:ident, [ $field:ident: $_type:ident as $type:ident, $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $name, [$($tt)*] -> [ $($res)* $field: $type, ]);
    };
    // Add normal field (array).
    (@def_struct_ $name:ident, [ $field:ident: [ $type:ty $(, $cnt:ident)? ], $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $name, [$($tt)*] -> [ $($res)* $field: Vec<$type>, ]);
        //def_struct!(@def_struct_ $name, [$($tt)*] -> [ $($res)* $field: Vec<u32>, ]);
    };
    // Add normal field.
    (@def_struct_ $name:ident, [ $field:ident: $type:ident, $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $name, [$($tt)*] -> [ $($res)* $field: $type, ]);
    };
    // Final.
    (@def_struct_ $name: ident, [] -> [ $($res:tt)* ]) => {
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
            [ $($set)* [ let $field = $in::from_bytes($stream)? as $out; ] ] [ $($fields)* $field ]);
    };
    // Set a field (array => size_field).
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ $field:tt: [$type:ty, $cnt:ident], $($tt:tt)*]
        -> [ $($set:tt)* ] [ $($fields:tt)* ]) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [
                let mut $field = Vec::new();
                let mut count = $cnt;
                // XXX while $stream.left() >= <$type>::min_size() as u64 && count > 0 {
                while $stream.left() > 0 && count > 0 {
                    println!("XXX left: {} count: {} going to read {}", $stream.left(), count, stringify!($type));
                    let v = <$type>::from_bytes($stream)?;
                    $field.push(v);
                    count -= 1;
                }
                println!("XXX left2: {}", $stream.left());
            ] ] [ $($fields)* $field ]);
    };
    // Set a field (array).
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ $field:tt: [$type:ty], $($tt:tt)*]
        -> [ $($set:tt)* ] [ $($fields:tt)* ]) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [
                let mut $field = Vec::new();
                // XXX while $stream.left() >= <$type>::min_size() as u64 {
                while $stream.left() > 0 {
                    println!("XXX left: {} going to read {}", $stream.left(), stringify!($type));
                    let v = <$type>::from_bytes($stream)?;
                    $field.push(v);
                }
                println!("XXX left2: {}", $stream.left());
            ] ] [ $($fields)* $field ]);
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
        def_struct!(@to_bytes_ $struct, $stream, [ $( $field: $type $(as $as)?, )* ] -> []);
    };
    // Insert a skip instruction.
    (@to_bytes_ $struct:expr, $stream:ident, [ skip: $amount:tt, $($tt:tt)*] -> [ $($set:tt)* ] ) => {
        def_struct!(@to_bytes_ $struct, $stream, [ $($tt)* ] ->
            [ $($set)* [ $stream.skip($amount)?; ] ] );
    };
    // Write a field value (as)
    (@to_bytes_ $struct:expr, $stream:ident, [ $field:tt: $type:tt as $_type:tt, $($tt:tt)*] -> [ $($set:tt)* ]) => {
        def_struct!(@to_bytes_ $struct, $stream, [ $($tt)* ] ->
            [ $($set)* [ ($struct.$field as $type).to_bytes($stream)?; ] ]);
    };
    // Write a field value (array)
    //(@to_bytes_ $struct:expr, $stream:ident, [ $field:tt: [$type:ty], $($tt:tt)*] -> [ $($set:tt)* ]) => {
    //    def_struct!(@to_bytes_ $struct, $stream, [ $($tt)* ] ->
    //        [ $($set)* [ for v in &$struct.$field { v.to_bytes($stream)?; } ] ]);
    //};
    // Write a field value (array => size_field)
    (@to_bytes_ $struct:expr, $stream:ident, [ $field:tt: [$type:ty $(, $cnt:ident)? ], $($tt:tt)*] -> [ $($set:tt)* ]) => {
        def_struct!(@to_bytes_ $struct, $stream, [ $($tt)* ] ->
            [ $($set)* [ for v in &$struct.$field { v.to_bytes($stream)?; } ] ]);
    };
    // Write a field value.
    (@to_bytes_ $struct:expr, $stream:ident, [ $field:tt: $type:tt, $($tt:tt)*] -> [ $($set:tt)* ]) => {
        def_struct!(@to_bytes_ $struct, $stream, [ $($tt)* ] ->
            [ $($set)* [ $struct.$field.to_bytes($stream)?; ] ]);
    };
    // Final.
    (@to_bytes_ $_struct:expr, $_stream:tt, [] -> [ $([$($set:tt)*])* ] ) => {
        {
            $(
                $($set)*
            )*
            Ok::<_, io::Error>(())
        }
    };

    // Helper.
    (@check_skip $this:expr, $dbg:expr, skip) => { };
    (@check_skip $this:expr, $dbg:expr, $field:ident) => { $dbg.field(stringify!($field), &$this.$field); };

    // Main entry point to define just one struct.
    ($name:ident, $($field:tt: $type:tt $(as $as:tt)?),* $(,)?) => {
        def_struct!(@def_struct $name,
            $(
                $field: $type $(as $as)?,
            )*
        );

        // Debug implementation that skips "skip"
        impl Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                let mut dbg = f.debug_struct(stringify!($name));
                $(
                    def_struct!(@check_skip self, dbg,  $field);
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
