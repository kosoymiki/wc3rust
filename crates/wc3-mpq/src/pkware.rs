//! PKWARE Data Compression Library "explode" (MPQ compression type 0x08).
//!
//! Faithful Rust port of StormLib's `pklib/explode.c` (Ladislav Zezula,
//! 2003-2010). WC3 stores installer payloads and some text tables with this
//! codec; the game cannot be unpacked without it. The algorithm is a
//! header byte pair (mode + dictionary bits), a 16-bit LSB-first bit buffer,
//! a length-code table and a 4-6 bit dictionary distance.

const CMP_BINARY: u8 = 0;
const CMP_ASCII: u8 = 1;

const DIST_BITS: [u8; 64] = [
    2, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
];
const DIST_CODE: [u8; 64] = [
    3, 13, 5, 25, 9, 17, 1, 62, 30, 46, 14, 54, 22, 38, 6, 58, 26, 42, 10, 50, 18, 34, 66, 2, 124, 60, 92, 28, 108, 44, 76, 12, 116, 52, 84, 20, 100, 36, 68, 4, 120, 56, 88, 24, 104, 40, 72, 8, 240, 112, 176, 48, 208, 80, 144, 16, 224, 96, 160, 32, 192, 64, 128, 0,
];
const EX_LEN_BITS: [u8; 16] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8,
];
const LEN_BASE: [u16; 16] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 14, 22, 38, 70, 134, 262,
];
const LEN_BITS: [u8; 16] = [
    3, 2, 3, 3, 4, 4, 4, 5, 5, 5, 5, 6, 6, 6, 7, 7,
];
const LEN_CODE: [u8; 16] = [
    5, 3, 1, 6, 10, 2, 12, 20, 4, 24, 8, 48, 16, 32, 64, 0,
];

const CH_BITS_ASC: [u8; 256] = [
    11, 12, 12, 12, 12, 12, 12, 12, 12, 8, 7, 12, 12, 7, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 13, 12, 12, 12, 12, 12, 4, 10, 8, 12, 10, 12, 10, 8, 7, 7, 8, 9, 7, 6, 7, 8, 7, 6, 7, 7, 7, 7, 8, 7, 7, 8, 8, 12, 11, 7, 9, 11, 12, 6, 7, 6, 6, 5, 7, 8, 8, 6, 11, 9, 6, 7, 6, 6, 7, 11, 6, 6, 6, 7, 9, 8, 9, 9, 11, 8, 11, 9, 12, 8, 12, 5, 6, 6, 6, 5, 6, 6, 6, 5, 11, 7, 5, 6, 5, 5, 6, 10, 5, 5, 5, 5, 8, 7, 8, 8, 10, 11, 11, 12, 12, 12, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 13, 12, 13, 13, 13, 12, 13, 13, 13, 12, 13, 13, 13, 13, 12, 13, 13, 13, 12, 12, 12, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13,
];
const CH_CODE_ASC: [u16; 256] = [
    1168, 4064, 2016, 3040, 992, 3552, 1504, 2528, 480, 184, 98, 3808, 1760, 34, 2784, 736, 3296, 1248, 2272, 224, 3936, 1888, 2912, 864, 3424, 1376, 4672, 2400, 352, 3680, 1632, 2656, 15, 592, 56, 608, 80, 3168, 912, 216, 66, 2, 88, 432, 124, 41, 60, 152, 92, 9, 28, 108, 44, 76, 24, 12, 116, 232, 104, 1120, 144, 52, 176, 1808, 2144, 49, 84, 17, 33, 23, 20, 168, 40, 1, 784, 304, 62, 100, 30, 46, 36, 1296, 14, 54, 22, 68, 48, 200, 464, 208, 272, 72, 1552, 336, 96, 136, 4000, 7, 38, 6, 58, 27, 26, 42, 10, 11, 528, 4, 19, 50, 3, 29, 18, 400, 13, 21, 5, 25, 8, 120, 240, 112, 656, 1040, 16, 1952, 2976, 928, 576, 7232, 3136, 5184, 1088, 6208, 2112, 4160, 64, 8064, 3968, 6016, 1920, 7040, 2944, 4992, 896, 7552, 3456, 5504, 1408, 6528, 2432, 4480, 384, 7808, 3712, 5760, 1664, 6784, 2688, 4736, 640, 7296, 3200, 5248, 1152, 6272, 2176, 4224, 128, 7936, 3840, 5888, 1792, 6912, 2816, 4864, 3488, 1440, 2464, 416, 3744, 1696, 2720, 672, 3232, 1184, 2208, 160, 3872, 1824, 2848, 800, 3360, 1312, 2336, 288, 3616, 1568, 2592, 544, 3104, 1056, 2080, 32, 4032, 1984, 3008, 960, 3520, 1472, 2496, 448, 3776, 1728, 2752, 704, 3264, 1216, 2240, 192, 3904, 1856, 2880, 832, 768, 3392, 7424, 3328, 5376, 1344, 1280, 6400, 2304, 2368, 4352, 256, 7680, 3584, 320, 5632, 1536, 6656, 3648, 1600, 2624, 2560, 4608, 512, 7168, 3072, 5120, 1024, 6144, 2048, 4096, 0,
];

#[derive(Debug)]
pub struct ExplodeError;

struct State<'a> {
    data: &'a [u8],
    in_pos: usize,
    bit_buff: u32,
    extra_bits: u32,
    ctype: u8,
    dsize_bits: u32,
    dsize_mask: u32,
    length_codes: [u8; 256],
    dist_pos_codes: [u8; 256],
    ch_bits_asc: [u8; 256],
    offs2c34: [u8; 256],
    offs2d34: [u8; 256],
    offs2e34: [u8; 256],
    offs2eb4: [u8; 256],
    out_buff: [u8; 0x2400],
    output_pos: usize,
    out: Vec<u8>,
}

impl<'a> State<'a> {
    fn new(data: &'a [u8]) -> Result<Self, ExplodeError> {
        if data.len() <= 4 {
            return Err(ExplodeError);
        }
        let ctype = data[0];
        let dsize_bits = data[1] as u32;
        if !(4..=6).contains(&dsize_bits) {
            return Err(ExplodeError);
        }
        if ctype != CMP_BINARY && ctype != CMP_ASCII {
            return Err(ExplodeError);
        }
        Ok(Self {
            data,
            in_pos: 3,
            bit_buff: data[2] as u32,
            extra_bits: 0,
            ctype,
            dsize_bits,
            dsize_mask: 0xFFFF >> (0x10 - dsize_bits),
            length_codes: [0; 256],
            dist_pos_codes: [0; 256],
            ch_bits_asc: CH_BITS_ASC,
            offs2c34: [0; 256],
            offs2d34: [0; 256],
            offs2e34: [0; 256],
            offs2eb4: [0; 256],
            out_buff: [0; 0x2400],
            output_pos: 0x1000,
            out: Vec::new(),
        })
    }

    fn gen_decode_tabs(positions: &mut [u8; 256], start_indexes: &[u8], length_bits: &[u8]) {
        for (i, (&start, &bits)) in start_indexes.iter().zip(length_bits).enumerate() {
            let step = 1usize << bits;
            let mut index = start as usize;
            while index < 0x100 {
                positions[index] = i as u8;
                index += step;
            }
        }
    }

    fn gen_asc_tabs(&mut self) {
        for count in (0..=0xFFusize).rev() {
            let ch_code = CH_CODE_ASC[count];
            let bits_asc = self.ch_bits_asc[count];
            if bits_asc <= 8 {
                let add = 1u32 << bits_asc;
                let mut acc = ch_code as u32;
                while acc < 0x100 {
                    self.offs2c34[acc as usize] = count as u8;
                    acc += add;
                }
            } else if (ch_code & 0xFF) != 0 {
                let acc = (ch_code & 0xFF) as usize;
                self.offs2c34[acc] = 0xFF;
                if (ch_code & 0x3F) != 0 {
                    let bits = bits_asc - 4;
                    self.ch_bits_asc[count] = bits;
                    let add = 1u32 << bits;
                    let mut acc = (ch_code >> 4) as u32;
                    while acc < 0x100 {
                        self.offs2d34[acc as usize] = count as u8;
                        acc += add;
                    }
                } else {
                    let bits = bits_asc - 6;
                    self.ch_bits_asc[count] = bits;
                    let add = 1u32 << bits;
                    let mut acc = (ch_code >> 6) as u32;
                    while acc < 0x80 {
                        self.offs2e34[acc as usize] = count as u8;
                        acc += add;
                    }
                }
            } else {
                let bits = bits_asc - 8;
                self.ch_bits_asc[count] = bits;
                let add = 1u32 << bits;
                let mut acc = (ch_code >> 8) as u32;
                while acc < 0x100 {
                    self.offs2eb4[acc as usize] = count as u8;
                    acc += add;
                }
            }
        }
    }

    fn waste_bits(&mut self, n_bits: u32) -> bool {
        if n_bits <= self.extra_bits {
            self.extra_bits -= n_bits;
            self.bit_buff >>= n_bits;
            return true;
        }
        self.bit_buff >>= self.extra_bits;
        if self.in_pos >= self.data.len() {
            return false;
        }
        self.bit_buff |= (self.data[self.in_pos] as u32) << 8;
        self.in_pos += 1;
        self.bit_buff >>= n_bits - self.extra_bits;
        self.extra_bits = self.extra_bits + 8 - n_bits;
        true
    }

    fn decode_lit(&mut self) -> u32 {
        if self.bit_buff & 1 != 0 {
            if !self.waste_bits(1) {
                return 0x306;
            }
            let length_code = self.length_codes[(self.bit_buff & 0xFF) as usize] as u32;
            if !self.waste_bits(LEN_BITS[length_code as usize] as u32) {
                return 0x306;
            }
            let extra_length_bits = EX_LEN_BITS[length_code as usize] as u32;
            let mut length_code = length_code;
            if extra_length_bits != 0 {
                let extra_length = self.bit_buff & ((1 << extra_length_bits) - 1);
                if !self.waste_bits(extra_length_bits) && (length_code + extra_length) != 0x10E {
                    return 0x306;
                }
                length_code = LEN_BASE[length_code as usize] as u32 + extra_length;
            }
            return length_code + 0x100;
        }
        if !self.waste_bits(1) {
            return 0x306;
        }
        if self.ctype == CMP_BINARY {
            let byte = self.bit_buff & 0xFF;
            return if self.waste_bits(8) { byte } else { 0x306 };
        }
        let mut value;
        if self.bit_buff & 0xFF != 0 {
            value = self.offs2c34[(self.bit_buff & 0xFF) as usize] as u32;
            if value == 0xFF {
                if self.bit_buff & 0x3F != 0 {
                    if !self.waste_bits(4) {
                        return 0x306;
                    }
                    value = self.offs2d34[(self.bit_buff & 0xFF) as usize] as u32;
                } else {
                    if !self.waste_bits(6) {
                        return 0x306;
                    }
                    value = self.offs2e34[(self.bit_buff & 0x7F) as usize] as u32;
                }
            }
        } else {
            if !self.waste_bits(8) {
                return 0x306;
            }
            value = self.offs2eb4[(self.bit_buff & 0xFF) as usize] as u32;
        }
        if self.waste_bits(self.ch_bits_asc[value as usize] as u32) {
            value
        } else {
            0x306
        }
    }

    fn decode_dist(&mut self, rep_length: u32) -> u32 {
        let dist_pos_code = self.dist_pos_codes[(self.bit_buff & 0xFF) as usize] as u32;
        let dist_pos_bits = DIST_BITS[dist_pos_code as usize] as u32;
        if !self.waste_bits(dist_pos_bits) {
            return 0;
        }
        let distance = if rep_length == 2 {
            let d = (dist_pos_code << 2) | (self.bit_buff & 0x03);
            if !self.waste_bits(2) {
                return 0;
            }
            d
        } else {
            let d = (dist_pos_code << self.dsize_bits) | (self.bit_buff & self.dsize_mask);
            if !self.waste_bits(self.dsize_bits) {
                return 0;
            }
            d
        };
        distance + 1
    }

    fn expand(&mut self) -> u32 {
        let result;
        loop {
            let next_literal = self.decode_lit();
            if next_literal >= 0x305 {
                result = next_literal;
                break;
            }
            if next_literal >= 0x100 {
                let mut rep_length = next_literal - 0xFE;
                let minus_dist = self.decode_dist(rep_length);
                if minus_dist == 0 {
                    return 0x306;
                }
                let mut target = self.output_pos;
                let minus = minus_dist as usize;
                if minus > target {
                    return 0x306;
                }
                while rep_length > 0 {
                    self.out_buff[target] = self.out_buff[target - minus];
                    target += 1;
                    rep_length -= 1;
                }
                self.output_pos = target;
            } else {
                self.out_buff[self.output_pos] = next_literal as u8;
                self.output_pos += 1;
            }
            if self.output_pos >= 0x2000 {
                self.append_flush();
            }
        }
        let tail_start = 0x1000;
        if self.output_pos > tail_start {
            let tail = self.out_buff[tail_start..self.output_pos].to_vec();
            self.out.extend_from_slice(&tail);
        }
        result
    }

    fn append_flush(&mut self) {
        self.out
            .extend_from_slice(&self.out_buff[0x1000..0x2000]);
        // The C memmove copies output_pos - 0x1000 bytes, including the
        // overflow bytes past 0x2000 that the repeat copy just wrote.
        let len = self.output_pos - 0x1000;
        self.out_buff.copy_within(0x1000..0x1000 + len, 0);
        self.output_pos -= 0x1000;
    }
}

/// Decompress a PKWARE DCL (implode) stream. The MPQ sector byte 0x08 has
/// already been stripped by the caller; `data` starts with the PKWARE mode and
/// dictionary-size bytes.
pub fn explode(data: &[u8]) -> Result<Vec<u8>, ExplodeError> {
    let mut state = State::new(data)?;
    if state.ctype != CMP_BINARY {
        state.gen_asc_tabs();
    }
    State::gen_decode_tabs(&mut state.length_codes, &LEN_CODE, &LEN_BITS);
    State::gen_decode_tabs(&mut state.dist_pos_codes, &DIST_CODE, &DIST_BITS);
    let result = state.expand();
    // StormLib's explode() reports CMP_ABORT when the bit reader reaches the
    // end of the input right after the final symbol (0x306) while still
    // producing the complete output. Certified archives hit this on `PKWARE`
    // sectors of encrypted `.w3m` files (observed: Human04 `war3map.wts`,
    // `crates/wc3-re/src/mpqex.rs` test). The caller always validates the
    // decoded length against the block table, so accepting a non-empty output
    // for 0x306 is safe and keeps the sector readable.
    if result != 0x305 && !(result == 0x306 && !state.out.is_empty()) {
        return Err(ExplodeError);
    }
    Ok(state.out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_truncated_stream() {
        assert!(explode(&[0, 4, 0, 0]).is_err());
    }

}
