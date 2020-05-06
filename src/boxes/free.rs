use std::io;
use crate::serialize::{FromBytes, ToBytes, ReadBytes, WriteBytes};
use crate::mp4box::{BoxReader, BoxWriter};

macro_rules! free_box {
    ($name:ident) => {

        #[derive(Debug)]
        pub struct $name(pub u64);

        impl FromBytes for $name {
            fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<$name> {
                let mut reader = BoxReader::new(stream)?;
                let stream = &mut reader;
                let size = stream.left();
                stream.skip(size)?;
                Ok($name(size))
            }

            fn min_size() -> usize { 0 }
        }

        impl ToBytes for $name {
            fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
                let mut writer = BoxWriter::new(stream, self)?;
                writer.skip(self.0)?;
                writer.finalize()
            }
        }
    };
}

free_box!(Free);
free_box!(Skip);
free_box!(Wide);

