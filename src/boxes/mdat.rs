use std::fmt::Debug;
use std::io;

use crate::serialize::{FromBytes, ToBytes, ReadBytes, WriteBytes, BoxBytes};
use crate::mp4box::BoxReader;
use crate::types::*;

macro_rules! def_content_box {
    ($(#[$outer:meta])* $name:ident, $fourcc:expr) => {

        $(#[$outer])*
        #[derive(Default)]
        pub struct $name {
            pub box_pos:  u64,
            pub data_pos: u64,
            pub data_size:  u64,
        }

        impl $name {
            pub fn is_large(&self) -> bool {
                self.data_pos - self.box_pos > 8 || self.data_size >= 0x1_0000_0000 - 8
            }
        }

        impl FromBytes for $name {
            fn from_bytes<R: ReadBytes>(stream: &mut R) -> io::Result<$name> {
                let box_pos = stream.pos();
                let mut reader = BoxReader::new(stream)?;
                let stream = &mut reader;
                let data_pos = stream.pos();
                let size = stream.left();
                stream.skip(size)?;
                Ok($name {
                    box_pos,
                    data_pos,
                    data_size: size,
                })
            }

            fn min_size() -> usize { 8 }
        }

        impl ToBytes for $name {
            fn to_bytes<W: WriteBytes>(&self, stream: &mut W) -> io::Result<()> {

                // First write a header.
                let fourcc = FourCC::new($fourcc);
                let mut box_size = self.data_size + 8;
                if self.is_large() {
                    box_size += 8;
                    1u32.to_bytes(stream)?;
                    fourcc.to_bytes(stream)?;
                    box_size.to_bytes(stream)?;
                } else {
                    (box_size as u32).to_bytes(stream)?;
                    fourcc.to_bytes(stream)?;
                }

                // Do we want to actually write the data?
                if stream.mdat_ref().is_none() {
                    return stream.skip(self.data_size);
                }

                // copy the data.
                let bufsize = std::cmp::min(self.data_size, 128*1024);
                let mut buf = Vec::with_capacity(bufsize as usize);
                buf.resize(bufsize as usize, 0);

                let mut todo = self.data_size;
                let mut pos = self.data_pos;

                while todo > 0 {
                    let sz = std::cmp::min(bufsize, todo) as usize;
                    let n = stream.mdat_ref().unwrap().read_at(&mut buf[..sz], pos)?;
                    if n == 0 {
                        return Err(io::ErrorKind::UnexpectedEof.into());
                    }
                    stream.write(&buf[..n])?;
                    todo -= n as u64;
                    pos += n as u64;
                }
                Ok(())
            }
        }

        impl Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                let mut dbg = f.debug_struct("$name");
                dbg.field("box_pos", &self.box_pos);
                dbg.field("data_pos", &self.data_pos);
                dbg.field("data_size", &self.data_size);
                dbg.finish()
            }
        }
    };
}

def_content_box!{
    /// 8.1.1 Media Data Box (ISO/IEC 14496-12:2015(E))
    Mdat,
    "mdat"
}

def_content_box!{
    /// Data box ('data') contained in Apple item list box ('ilst').
    DataBox,
    "data"
}

