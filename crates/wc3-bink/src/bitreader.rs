//! MSB-first bit reader for MPEG-4 / H.263 elementary streams.
//!
//! MPEG-4 Part 2 codes are read most-significant-bit first: the first bit of
//! the stream is bit 7 of byte 0. (Contrast with Bink video bundles, which are
//! LSB-first; these WC3 movies carry MPEG-4, so MSB is the correct order here.)
//!
//! Every accessor is bounds-checked and returns [`BitError`] instead of
//! panicking: a truncated or corrupt frame must fail decoding, never take the
//! game down.

/// Failure of a bit-level read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitError {
    /// The stream ended before the requested bits were available.
    Eof,
    /// The bits present do not form a valid code word.
    InvalidCode,
}

/// A cursor over an in-memory bit string, MSB-first.
#[derive(Debug, Clone, Copy)]
pub struct BitReader<'a> {
    data: &'a [u8],
    /// Absolute position in bits.
    pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Start at an explicit bit offset (used after a start-code scan).
    pub fn at(data: &'a [u8], bit: usize) -> Self {
        let pos = bit.min(data.len() * 8);
        Self { data, pos }
    }

    pub fn bit_pos(&self) -> usize {
        self.pos
    }

    pub fn bits_left(&self) -> usize {
        self.data.len() * 8 - self.pos
    }

    pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len() * 8
    }

    /// Underlying bytes (for start-code searches).
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn read_bit(&mut self) -> Result<u32, BitError> {
        if self.pos >= self.data.len() * 8 {
            return Err(BitError::Eof);
        }
        let byte = self.data[self.pos >> 3];
        let bit = (byte >> (7 - (self.pos & 7))) & 1;
        self.pos += 1;
        Ok(u32::from(bit))
    }

    pub fn read_bool(&mut self) -> Result<bool, BitError> {
        Ok(self.read_bit()? != 0)
    }

    /// Read `n` bits (0..=32) as an unsigned value, MSB first.
    pub fn read_bits(&mut self, n: u32) -> Result<u32, BitError> {
        debug_assert!(n <= 32);
        if n == 0 {
            return Ok(0);
        }
        if self.bits_left() < n as usize {
            self.pos = self.data.len() * 8;
            return Err(BitError::Eof);
        }
        let mut v: u32 = 0;
        let mut left = n;
        while left > 0 {
            let byte_idx = self.pos >> 3;
            let bit_off = self.pos & 7;
            let take = left.min((8 - bit_off) as u32);
            let byte = u32::from(self.data[byte_idx]);
            let chunk = (byte >> (8 - bit_off as u32 - take)) & ((1u32 << take) - 1);
            v = (v << take) | chunk;
            self.pos += take as usize;
            left -= take as usize as u32;
        }
        Ok(v)
    }

    /// Read `n` bits as a two's-complement signed value.
    pub fn read_bits_i32(&mut self, n: u32) -> Result<i32, BitError> {
        if n == 0 {
            return Ok(0);
        }
        let v = self.read_bits(n)?;
        if n == 32 {
            return Ok(v as i32);
        }
        let shift = 32 - n;
        Ok(((v << shift) as i32) >> shift)
    }

    /// Look at the next `n` bits without consuming them.
    ///
    /// Bits past the end read as zero, so a VLC look-ahead at the tail of a
    /// frame does not turn a valid short code into an error.
    pub fn peek(&self, n: u32) -> u32 {
        debug_assert!(n <= 32);
        let mut v = 0u32;
        for i in 0..n as usize {
            let p = self.pos + i;
            let bit = if p < self.data.len() * 8 {
                (self.data[p >> 3] >> (7 - (p & 7))) & 1
            } else {
                0
            };
            v = (v << 1) | u32::from(bit);
        }
        v
    }

    pub fn skip(&mut self, n: usize) -> Result<(), BitError> {
        if self.bits_left() < n {
            self.pos = self.data.len() * 8;
            return Err(BitError::Eof);
        }
        self.pos += n;
        Ok(())
    }

    /// Skip to the next byte boundary (MPEG start codes are byte-aligned).
    pub fn align(&mut self) {
        let rem = self.pos & 7;
        if rem != 0 {
            self.pos += 8 - rem;
        }
        self.pos = self.pos.min(self.data.len() * 8);
    }

    /// Distance in bits to the next byte boundary.
    pub fn unaligned_bits(&self) -> usize {
        (8 - (self.pos & 7)) & 7
    }

    /// Read a start code if one is present at the cursor: `0x000001XX`.
    pub fn read_start_code(&mut self) -> Option<u8> {
        if self.bits_left() < 32 {
            return None;
        }
        if self.pos & 7 != 0 {
            return None;
        }
        let b = self.pos >> 3;
        let d = self.data;
        if d[b] == 0 && d[b + 1] == 0 && d[b + 2] == 1 {
            self.pos += 32;
            Some(d[b + 3])
        } else {
            None
        }
    }

    /// Search forward for `0x000001XX`; leaves the cursor after the code byte
    /// and returns it. Returns `None` when no further start code exists.
    pub fn next_start_code(&mut self) -> Option<u8> {
        self.align();
        let mut b = self.pos >> 3;
        let d = self.data;
        while b + 3 < d.len() {
            if d[b] == 0 && d[b + 1] == 0 && d[b + 2] == 1 {
                self.pos = (b + 4) * 8;
                return Some(d[b + 3]);
            }
            b += 1;
        }
        self.pos = d.len() * 8;
        None
    }

    /// Find `0x000001XX` in `data` at or after `from`, returning the byte index
    /// of the code byte (`XX`).
    pub fn find_start_code(data: &[u8], from: usize) -> Option<(usize, u8)> {
        let mut i = from;
        while i + 3 < data.len() {
            if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
                return Some((i + 3, data[i + 3]));
            }
            i += 1;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_msb_first() {
        // 0b1011_0010, 0b0100_1111
        let data = [0b1011_0010u8, 0b0100_1111];
        let mut br = BitReader::new(&data);
        assert_eq!(br.read_bit().unwrap(), 1);
        assert_eq!(br.read_bits(3).unwrap(), 0b011);
        assert_eq!(br.read_bits(5).unwrap(), 0b0_0100);
        assert_eq!(br.read_bits(7).unwrap(), 0b100_1111);
        assert!(br.read_bit().is_err());
    }

    #[test]
    fn matches_reference_bit_string() {
        let data = [0xDE, 0xAD, 0xBE, 0xEF];
        let bits = "11011110101011011011111011101111";
        let mut br = BitReader::new(&data);
        for (i, c) in bits.chars().enumerate() {
            assert_eq!(br.read_bit().unwrap(), c.to_digit(2).unwrap(), "bit {i}");
        }
        assert!(br.is_empty());
    }

    #[test]
    fn signed_reads_sign_extend() {
        let data = [0b1110_0000];
        let mut br = BitReader::new(&data);
        assert_eq!(br.read_bits_i32(4).unwrap(), -2);
        let mut br = BitReader::new(&data);
        assert_eq!(br.read_bits_i32(3).unwrap(), -1);
    }

    #[test]
    fn peek_does_not_consume_and_pads_with_zero() {
        let data = [0xFF];
        let br = BitReader::new(&data);
        assert_eq!(br.peek(4), 0xF);
        assert_eq!(br.peek(12), 0xFF0);
        let mut br = BitReader::new(&data);
        br.skip(6).unwrap();
        assert_eq!(br.peek(4), 0b1100);
        assert_eq!(br.bit_pos(), 6);
    }

    #[test]
    fn truncated_reads_error_not_panic() {
        let data = [0xAB];
        let mut br = BitReader::new(&data);
        assert_eq!(br.read_bits(8).unwrap(), 0xAB);
        assert_eq!(br.read_bits(1), Err(BitError::Eof));
        assert_eq!(br.read_bits(32), Err(BitError::Eof));
        assert_eq!(br.bits_left(), 0);
    }

    #[test]
    fn start_code_scan() {
        let data = [0x00, 0x00, 0x01, 0xB6, 0xFF, 0x00, 0x00, 0x01, 0x20];
        assert_eq!(BitReader::find_start_code(&data, 0), Some((3, 0xB6)));
        assert_eq!(BitReader::find_start_code(&data, 4), Some((8, 0x20)));
        assert_eq!(BitReader::find_start_code(&data, 9), None);
        let mut br = BitReader::new(&data);
        assert_eq!(br.next_start_code(), Some(0xB6));
        assert_eq!(br.bit_pos(), 32);
        assert_eq!(br.next_start_code(), Some(0x20));
    }

    #[test]
    fn fuzz_never_panics() {
        // Deterministic xorshift; every accessor must fail gracefully on
        // arbitrary bytes, including lengths 0..=64.
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for len in 0..=64usize {
            for _ in 0..32 {
                let mut data = vec![0u8; len];
                for b in data.iter_mut() {
                    *b = next() as u8;
                }
                let mut br = BitReader::new(&data);
                for _ in 0..24 {
                    let n = (next() % 33) as u32;
                    match next() % 5 {
                        0 => {
                            let _ = br.read_bit();
                        }
                        1 => {
                            let _ = br.read_bits(n);
                        }
                        2 => {
                            let _ = br.peek(n);
                        }
                        3 => {
                            let _ = br.read_bits_i32(n);
                        }
                        _ => {
                            let _ = br.skip((next() % 40) as usize);
                        }
                    }
                }
                // Start-code scans over the same random bytes must not panic.
                let _ = BitReader::find_start_code(&data, 0);
                let mut br = BitReader::new(&data);
                let _ = br.next_start_code();
            }
        }
    }
}
