//! Basic serializer / deserializer.
//!
//! The FromToBytes trait and the def_struct! macro are defined here.
//!
//! It also  contains the FromToBytes implementations for the
//! primitive types u8/u16/u32/u64/u128.
//!
use std::convert::TryInto;
use std::io::{self, ErrorKind::UnexpectedEof};
use crate::io::{ReadBytes, WriteBytes};

// Should the ReadBytes and WriteBytes traits also
// be defined here instead of in io.rs?

/// Trait to serialize and deserialize a type.
pub trait FromToBytes {
    fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self> where Self: Sized;
    fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) -> io::Result<()>;
    fn min_size() -> usize;
}

// Convenience macro to implement FromToBytes for u* types.
macro_rules! def_from_to_bytes {
    ($type:ident) => {
        impl FromToBytes for $type {
            fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self> {
                let sz = std::mem::size_of::<$type>();
                let data = bytes.read(sz as u64)?;
                let data = data.try_into().map_err(|_| UnexpectedEof)?;
                $type::from_be_bytes(data)
            }
            fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) -> io::Result<()> {
                bytes.write(&self.to_be_bytes()[..])
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

/// A 3-byte unsigned integer.
///
/// This is a helper for the fake `u24` type, but it can also be used directly.
#[derive(Clone, Copy)]
pub struct U24(pub u32);

impl FromToBytes for U24 {
    fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self> {
        let data = bytes.read(3)?;
        let mut buf = [0u8; 4];
        (&mut buf[1..]).copy_from_slice(&data);
        Ok(U24(u32::from_be_bytes(buf)))
    }
    fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) -> io::Result<()> {
        bytes.write(&self.0.to_be_bytes()[1..])
    }
    fn min_size() -> usize { 3 }
}

/// A macro to define a struct and implement the FromToBytes trait for it.
///
/// Usage:
///
/// ```text
/// def_struct! { Name,
///     field1:     u32,        // primitive type
///     field2:     u24,        // fake primitive type
///     field3:     Time,       // struct that also implements FromToBytes
///     skip:       8,          // skip 8 bytes here while serializing / deserializing.
///     ....
/// }
/// ```
///
/// `u24` is a weird exception and not a real type. In the actual struct it is
/// a `u32`, but it's read and written as 3 bytes.
#[macro_export]
macro_rules! def_struct {
    // minimum size for a certain type. we hard-code u* here.
    (@min_size u8) => { 1 };
    (@min_size u16) => { 2 };
    (@min_size u24) => { 3 };
    (@min_size u32) => { 4 };
    (@min_size u64) => { 8 };
    (@min_size u128) => { 16 };
    (@min_size [ $_type:ty ]) => { 0 };
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
    // Add normal field (u24).
    (@def_struct_ $name:ident, [ $field:ident: u24, $($tt:tt)*] -> [ $($res:tt)* ]) => {
        def_struct!(@def_struct_ $name, [$($tt)*] -> [ $($res)* $field: u32, ]);
    };
    // Add normal field (array).
    (@def_struct_ $name:ident, [ $field:ident: [ $type:ty ], $($tt:tt)*] -> [ $($res:tt)* ]) => {
        //def_struct!(@def_struct_ $name, [$($tt)*] -> [ $($res)* $field: Vec<$type>, ]);
        def_struct!(@def_struct_ $name, [$($tt)*] -> [ $($res)* $field: Vec<u32>, ]);
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
    // Set a field (u24).
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ $field:tt: u24, $($tt:tt)*]
        -> [ $($set:tt)* ] [ $($fields:tt)* ]) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [ let $field = $crate::fromtobytes::U24::from_bytes($stream)?.0; ] ] [ $($fields)* $field ]);
    };
    // Set a field (array).
    (@from_bytes_ $name:ident, $base:tt, $stream:ident, [ $field:tt: [$type:ty], $($tt:tt)*]
        -> [ $($set:tt)* ] [ $($fields:tt)* ]) => {
        def_struct!(@from_bytes_ $name, $base, $stream, [ $($tt)* ] ->
            [ $($set)* [
                let mut $field = Vec::new();
                while $stream.left() >= <$type>::min_size() as u64 {
                    println!("XXX left: {}", $stream.left());
                    let v = <$type>::from_bytes($stream)?;
                    $field.push(v);
                }
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
    // Write a field value (u24).
    (@to_bytes_ $struct:expr, $stream:ident, [ $field:tt: u24, $($tt:tt)*] -> [ $($set:tt)* ]) => {
        def_struct!(@to_bytes_ $struct, $stream, [ $($tt)* ] ->
            [ $($set)* [ $crate::fromtobytes::U24($struct.$field as u32).to_bytes($stream)?; ] ]);
    };
    // Write a field value (array)
    (@to_bytes_ $struct:expr, $stream:ident, [ $field:tt: [$type:tt], $($tt:tt)*] -> [ $($set:tt)* ]) => {
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
            Ok(())
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

        impl FromToBytes for $name {
            fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<Self> {
                def_struct!(@from_bytes $name, [], stream, $(
                    $field: $type $(as $as)?,
                )*)
            }

            fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
                def_struct!(@to_bytes self, stream, $(
                    $field: $type $(as $as)?,
                )*)
            }

            fn min_size() -> usize {
                $( def_struct!(@min_size $type) +)* 0
            }
        }
    }
}

