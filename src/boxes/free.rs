use std::io;
use crate::fromtobytes::{FromBytes, ToBytes, ReadBytes, WriteBytes};

macro_rules! free_box {
    ($name:ident) => {

        #[derive(Debug)]
        pub struct $name(pub u64);

        impl FromBytes for $name {
            fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<$name> {
                let size = stream.left();
                stream.skip(size)?;
                Ok($name(size))
            }

            fn min_size() -> usize { 0 }
        }

        impl ToBytes for $name {
            fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
                stream.skip(self.0)
            }
        }
    };
}

free_box!(Free);
free_box!(Skip);
free_box!(Wide);

