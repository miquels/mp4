use std::io;

// Read binary data bit-by-bit.
pub(crate) struct BitReader<'a> {
    pub data: &'a [u8],
    pub pos:  usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> BitReader<'a> {
        BitReader { data, pos: 0 }
    }

    fn read_bit_at(&self, pos: usize) -> io::Result<bool> {
        let b = pos / 8;
        if b >= self.data.len() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let c = pos - 8 * b;
        let bit = self.data[b] & (128 >> c);
        Ok(bit > 0)
    }

    pub fn read_bit(&mut self) -> io::Result<bool> {
        let r = self.read_bit_at(self.pos)?;
        self.pos += 1;
        Ok(r)
    }

    pub fn read_bits(&mut self, count: u8) -> io::Result<u32> {
        let mut count = count;
        let mut r = 0;

        while count > 0 {
            r = r << 1 | (self.read_bit_at(self.pos)?) as u32;
            self.pos += 1;
            count -= 1;
        }
        Ok(r)
    }

    pub fn read_u8(&mut self) -> io::Result<u8> {
        self.read_bits(8).map(|b| b as u8)
    }

    /// Read unsigned exp-golomb code
    pub fn read_ue(&mut self) -> io::Result<u32> {
        let mut cnt = 0u32;
        while self.read_bit()? == false {
            cnt += 1;
        }
        if cnt == 0 {
            return Ok(0);
        }
        if cnt > 8 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let val = self.read_bits(cnt as u8)?;
        let res = (1u32 << cnt) - 1 + val;
        Ok(res)
    }

    pub fn read_ue_max(&mut self, max: u32) -> io::Result<u32> {
        let v = self.read_ue()?;
        if v > max {
            return Err(ioerr!(InvalidData, "value too large"));
        }
        Ok(v)
    }

    /// Read signed exp-golomb code
    pub fn read_se(&mut self) -> io::Result<i32> {
        let val = self.read_ue()?;
        let sign = ((val & 0x1) << 1) as i32 - 1;
        let res = ((val >> 1) + (val & 0x1)) as i32 * sign;
        Ok(res)
    }
}
