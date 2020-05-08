use std::io;
use crate::serialize::{FromBytes, ToBytes, ReadBytes, WriteBytes};
use crate::mp4box::BoxInfo;
use crate::types::*;

def_box! {
    /// Apple Item List.
    AppleItemListBox, "ilst",
        items:  [AppleItem],
}

/// Apple item.
pub struct AppleItem {
    pub fourcc: FourCC,
    pub text:   String,
    blob:       Vec<u8>,
}

impl FromBytes for AppleItem {
    fn from_bytes<R: ReadBytes>(bytes: &mut R) -> io::Result<Self> {
        // First read this box (e.g. an "©too").
        let mut size = u32::from_bytes(bytes)?;
        let fourcc = FourCC::from_bytes(bytes)?;
        if size < 8 {
            size = 8;
        }
        debug!("XXX 1 size {} fourcc {}", size, fourcc);
        let mut res = AppleItem {
            fourcc,
            text: String::new(),
            blob: bytes.read((size - 8) as u64)?.to_vec(),
        };
        let mut blob_slice = &res.blob[..];
        let data = &mut blob_slice;

        // Now read the sub-box. Again, length + fourcc.
        let size = u32::from_bytes(data)?;
        let fourcc = FourCC::from_bytes(data)?;

        if fourcc.to_string() == "data" {
            ReadBytes::skip(data, 2)?;
            let flag = u16::from_bytes(data)?;
            if flag == 1 && size >= 16 {
                ReadBytes::skip(data, 4)?;
                let text = data.read((size - 16) as u64)?;
                res.text = String::from_utf8_lossy(text).to_string();
            }
        }
        Ok(res)
    }

    fn min_size() -> usize {
        16
    }
}

impl ToBytes for AppleItem {
    fn to_bytes<W: WriteBytes>(&self, bytes: &mut W) -> io::Result<()> {
        if self.text.len() == 0 {
            // No string data, just write the blob,
            // i.e. write back what we read before.
            if self.blob.len() > 0 {
                let size = (8 + self.blob.len()) as u32;
                size.to_bytes(bytes)?;
                self.fourcc.to_bytes(bytes)?;
                bytes.write(&self.blob[..])?;
            }
            return Ok(());
        }

        // Write the main box (e.g. ©too).
        let mut size = (24 + self.text.len()) as u32;
        size.to_bytes(bytes)?;
        self.fourcc.to_bytes(bytes)?;

        // Now write the data sub-box header (16 bytes)
        size -= 8;
        size.to_bytes(bytes)?;
        bytes.write(b"data")?;
        bytes.skip(2)?;
        1u16.to_bytes(bytes)?;
        bytes.skip(4)?;

        // And finally the data itself.
        bytes.write(self.text.as_bytes())
    }
}

impl std::fmt::Debug for AppleItem {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.text.len() {
            0 => write!(f, "{}: [{} bytes]", self.fourcc, self.blob.len()),
            _ => write!(f, "{}: \"{}\"", self.fourcc, self.text),
        }
    }
}

