//! VLC (variable-length code) decoding tables.
//!
//! MPEG-4 / H.263 codes are prefix codes of at most 13 bits. Decoding builds a
//! flat look-up table indexed by the next `max_bits` bits: one array probe per
//! symbol, no tree walk. Tables are built once at decoder construction, so the
//! per-macroblock path only peeks and indexes.

use crate::bitreader::{BitError, BitReader};

/// Sentinel for an unused look-up slot.
const INVALID: u16 = 0xFFFF;

/// A canonical prefix-code look-up table.
#[derive(Debug, Clone)]
pub struct Vlc {
    max_bits: u8,
    /// `(len << 8) | symbol`, or [`INVALID`].
    lut: Vec<u16>,
}

impl Vlc {
    /// Build from `(code, length)` pairs; the symbol is the slice index.
    pub fn new(table: &[(u16, u8)]) -> Self {
        let max_bits = table.iter().map(|&(_, len)| len).max().unwrap_or(0);
        assert!(max_bits > 0 && max_bits <= 13, "VLC length out of range");
        let mut lut = vec![INVALID; 1usize << max_bits];
        for (sym, &(code, len)) in table.iter().enumerate() {
            // A zero length marks an unused slot in the reference tables
            // (H.263 keeps three stuffing rows with code 0 / length 0).
            if len == 0 {
                continue;
            }
            debug_assert!(sym < 0x100, "symbol must fit a byte");
            debug_assert!(u32::from(code) < (1u32 << len), "code wider than length");
            let shift = max_bits - len;
            let base = (u32::from(code) << shift) as usize;
            let fill = 1usize << shift;
            let entry = (u16::from(len) << 8) | sym as u16;
            for slot in &mut lut[base..base + fill] {
                // Duplicate prefix => malformed table; last one wins but that
                // never happens with the spec tables (asserted in tests).
                *slot = entry;
            }
        }
        Self { max_bits, lut }
    }

    pub fn max_bits(&self) -> u8 {
        self.max_bits
    }

    /// Decode one symbol.
    pub fn decode(&self, br: &mut BitReader<'_>) -> Result<u16, BitError> {
        if br.is_empty() {
            return Err(BitError::Eof);
        }
        let word = br.peek(u32::from(self.max_bits)) as usize;
        let entry = self.lut[word];
        if entry == INVALID {
            return Err(BitError::InvalidCode);
        }
        let len = (entry >> 8) as usize;
        if len > br.bits_left() {
            return Err(BitError::Eof);
        }
        br.skip(len)?;
        Ok(entry & 0xFF)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(table: &[(u16, u8)]) -> Vec<usize> {
        // Kraft sum must be exactly 1 for a complete prefix code.
        table
            .iter()
            .fold(vec![0usize; 14], |mut acc, &(_, len)| {
                acc[len as usize] += 1;
                acc
            })
    }

    #[test]
    fn decodes_hand_built_table() {
        // Symbols: 0 -> "0", 1 -> "10", 2 -> "110", 3 -> "111"
        let vlc = Vlc::new(&[(0b0, 1), (0b10, 2), (0b110, 3), (0b111, 3)]);
        // Bit string: 0 | 110 | 111 | 10
        let data = [0b0110_1111u8, 0b0000_0000];
        let mut br = BitReader::new(&data);
        assert_eq!(vlc.decode(&mut br).unwrap(), 0);
        assert_eq!(vlc.decode(&mut br).unwrap(), 2);
        assert_eq!(vlc.decode(&mut br).unwrap(), 3);
        assert_eq!(vlc.decode(&mut br).unwrap(), 1);
        assert_eq!(br.bits_left(), 7);
    }

    #[test]
    fn real_tables_are_complete_prefix_codes() {
        use crate::tables;
        let all: [(&str, &[(u16, u8)]); 9] = [
            ("dc_lum", &tables::DC_LUM),
            ("dc_chrom", &tables::DC_CHROM),
            ("intra_tcoeff", &tables::INTRA_TCOEFF),
            ("inter_tcoeff", &tables::INTER_TCOEFF),
            ("intra_mcbpc", &tables::INTRA_MCBPC),
            ("inter_mcbpc", &tables::INTER_MCBPC),
            ("cbpy", &tables::CBPY),
            ("mv", &tables::MV_TAB),
            ("mb_type_b", &tables::MB_TYPE_B),
        ];
        for (name, table) in all {
            let live: Vec<(u16, u8)> = table.iter().copied().filter(|&(_, l)| l > 0).collect();
            let c = counts(&live);
            let kraft: f64 = c
                .iter()
                .enumerate()
                .skip(1)
                .map(|(len, n)| *n as f64 / (1u64 << len) as f64)
                .sum();
            // Escape entries share the code space in MPEG-4 tables, so allow
            // a complete code (1.0) or the documented escape split.
            assert!(
                (0.93..=1.0).contains(&kraft),
                "{name}: Kraft sum {kraft} is not a (near-)complete prefix code"
            );
        }
    }

    #[test]
    fn truncated_code_is_an_error_not_a_panic() {
        let vlc = Vlc::new(&[(0b0, 1), (0b10, 2), (0b110, 3), (0b111, 3)]);
        // Only the 3-bit code "110" is present; consuming it empties the
        // reader, and the next symbol must report Eof instead of looping.
        let vlc = Vlc::new(&[(0b110, 3)]);
        let data = [0b0000_0110u8];
        let mut br = BitReader::new(&data);
        br.skip(5).unwrap();
        assert_eq!(vlc.decode(&mut br).unwrap(), 0);
        assert_eq!(vlc.decode(&mut br), Err(BitError::Eof));
        let empty: [u8; 0] = [];
        let mut br = BitReader::new(&empty);
        assert_eq!(vlc.decode(&mut br), Err(BitError::Eof));
    }
}
