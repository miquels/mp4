use std::io;

// Read binary data bit-by-bit.
pub(crate) struct BitReader<'a> {
    pub data:   &'a[u8],
    pub pos:    usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> BitReader<'a> {
        BitReader {
            data,
            pos: 0,
        }
    }

    pub fn read_bit(&self, pos: usize) -> io::Result<bool> {
        let b = pos / 8;
        if b >= self.data.len() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let c = pos - 8 * b;
        let bit = self.data[b] & (128 >> c);
        Ok(bit > 0)
    }

    pub fn read_bits(&mut self, count: u8) -> io::Result<u32> {
        let mut count = count;
        let mut r = 0;

        while count > 0 {
            r = r << 1 | (self.read_bit(self.pos)?) as u32;
            self.pos += 1;
            count -= 1;
        }
        Ok(r)
    }
}

