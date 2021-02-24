use std::io;
use crate::boxes::prelude::*;

macro_rules! free_box {
    ($name:ident, $fourcc:expr) => {

        def_box! {
            $name {
                size: u64,
            },
            fourcc => $fourcc,
            version => [],
            impls => [ basebox, boxinfo, debug ],
        }

        impl FromBytes for $name {
            fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<$name> {
                let mut reader = BoxReader::new(stream)?;
                let stream = &mut reader;
                let size = stream.left();
                stream.skip(size)?;
                Ok($name { size })
            }

            fn min_size() -> usize { 0 }
        }

        impl ToBytes for $name {
            fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {
                let mut writer = BoxWriter::new(stream, self)?;
                writer.skip(self.size)?;
                writer.finalize()
            }
        }
    };
}

free_box!(Free, "free");
free_box!(Skip, "skip");
free_box!(Wide, "wide");

