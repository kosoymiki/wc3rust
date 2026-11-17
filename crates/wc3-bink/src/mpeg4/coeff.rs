//! Block coefficient decoding: DC differential, TCOEFF (run/level) VLC with
//! escapes, inverse quantisation, and the MPEG-4 DC/AC predictors.

use crate::bitreader::{BitError, BitReader};
use crate::error::BinkError;
use crate::mpeg4::frame::{Picture, Plane};
use crate::tables;
use crate::vlc::Vlc;

thread_local! {
    /// Count of blocks that ended past coefficient 63 (recovered, not fatal).
    static OVERRUNS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Blocks whose run overflowed the 8x8 grid and were closed at the last
/// coefficient. A handful across a movie is normal for a stream with a damaged
/// block; a spike means a desync.
pub fn tcoeff_overruns() -> u32 {
    OVERRUNS.with(|c| c.get())
}

pub fn reset_tcoeff_overruns() {
    OVERRUNS.with(|c| c.set(0));
}

fn note_overrun() {
    OVERRUNS.with(|c| c.set(c.get() + 1));
}

fn map_bit(e: BitError) -> BinkError {
    match e {
        BitError::Eof => BinkError::Corrupt("mpeg4 block truncated"),
        BitError::InvalidCode => BinkError::Corrupt("mpeg4 invalid VLC code"),
    }
}

/// Escape symbol index in both TCOEFF tables (the 103rd entry).
const ESCAPE_SYMBOL: u16 = 102;

/// Decoder-wide VLC state, built once.
pub struct CoeffTables {
    esc1: std::cell::Cell<u32>,
    esc2: std::cell::Cell<u32>,
    esc3: std::cell::Cell<u32>,
    pub dc_lum: Vlc,
    pub dc_chrom: Vlc,
    intra: Vlc,
    inter: Vlc,
    /// First symbol index whose code has `last == 1`.
    intra_last: usize,
    inter_last: usize,
    /// Per-table escape helpers (`max_level[last][run]`, `max_run[last][level]`).
    intra_maps: RlMaps,
    inter_maps: RlMaps,
}

/// Derived bounds used by the TCOEFF escape codings (ISO/IEC 14496-2 Table
/// B-16/B-17): the largest level for a run and the largest run for a level.
#[derive(Clone)]
struct RlMaps {
    max_level: [[u8; 64]; 2],
    max_run: [[u8; 64]; 2],
}

impl RlMaps {
    fn build(run: &[i8; 102], level: &[i8; 102], last_from: usize) -> Self {
        let mut maps = Self {
            max_level: [[0; 64]; 2],
            max_run: [[0; 64]; 2],
        };
        for (idx, (&r, &l)) in run.iter().zip(level.iter()).enumerate() {
            let last = usize::from(idx >= last_from);
            let (r, l) = (r as usize, l as u8);
            maps.max_level[last][r] = maps.max_level[last][r].max(l);
            maps.max_run[last][l as usize] = maps.max_run[last][l as usize].max(r as u8);
        }
        maps
    }
}

impl CoeffTables {
    pub fn new() -> Self {
        let intra = Vlc::new(&tables::INTRA_TCOEFF);
        let inter = Vlc::new(&tables::INTER_TCOEFF);
        let intra_last = 67;
        let inter_last = 58;
        let intra_maps = RlMaps::build(&tables::INTRA_RUN, &tables::INTRA_LEVEL, intra_last);
        let inter_maps = RlMaps::build(&tables::INTER_RUN, &tables::INTER_LEVEL, inter_last);
        Self {
            esc1: std::cell::Cell::new(0),
            esc2: std::cell::Cell::new(0),
            esc3: std::cell::Cell::new(0),
            dc_lum: Vlc::new(&tables::DC_LUM),
            dc_chrom: Vlc::new(&tables::DC_CHROM),
            intra,
            inter,
            intra_last,
            inter_last,
            intra_maps,
            inter_maps,
        }
    }

    /// `get_xbits`: MPEG DC differential, sign-magnitude with the inverted
    /// bit pattern: the all-ones code is the largest magnitude.
    fn xbits(br: &mut BitReader<'_>, n: u32) -> Result<i32, BinkError> {
        let s = br.read_bits(n).map_err(map_bit)? as i32;
        let msb = 1i32 << (n - 1);
        Ok(if s & msb != 0 {
            s
        } else {
            s - (1 << n) + 1
        })
    }

    /// Decode a DC differential (size VLC + xbits) for block `n` (0..3 luma,
    /// 4/5 chroma). Returns the differential level.
    pub fn decode_dc_diff(&self, br: &mut BitReader<'_>, n: usize) -> Result<i32, BinkError> {
        let vlc = if n < 4 { &self.dc_lum } else { &self.dc_chrom };
        let before = br.bit_pos();
        let size = vlc.decode(br).map_err(map_bit)?;
        if size == 0 {
            if std::env::var_os("WC3_BINK_DC").is_some() {
                eprintln!("  dc n={n} size=0 bit {before}->{}", br.bit_pos());
            }
            return Ok(0);
        }
        let level = Self::xbits(br, u32::from(size))?;
        if std::env::var_os("WC3_BINK_DC").is_some() {
            eprintln!(
                "  dc n={n} size={size} level={level} bit {before}->{}",
                br.bit_pos()
            );
        }
        if size > 8 {
            // Marker bit after large DC values.
            let _ = br.read_bit().map_err(map_bit)?;
        }
        Ok(level)
    }

    /// Decode AC coefficients of one block into `block` (raster order,
    /// quantised domain). Returns the last written scan index.
    #[allow(clippy::too_many_arguments)]
    pub fn decode_tcoeff(
        &self,
        br: &mut BitReader<'_>,
        intra: bool,
        qscale: u8,
        quant_type: bool,
        scan: &[u8; 64],
        block: &mut [i32; 64],
        // Intra blocks whose DC came from the DC VLC start their AC run at
        // scan index 1; every other block starts at 0 (the `-1` cursor).
        ac_only: bool,
    ) -> Result<i32, BinkError> {
        let (vlc, last_threshold) = if intra {
            (&self.intra, self.intra_last)
        } else {
            (&self.inter, self.inter_last)
        };
        // Intra coefficients are dequantised later by `dequant_intra`, which
        // applies the same qmul/qadd; the escape paths must use the same
        // scaling the encoder applied.
        let qmul: i32 = if intra || quant_type {
            1
        } else {
            i32::from(qscale) << 1
        };
        let qadd: i32 = if intra || quant_type {
            0
        } else {
            (i32::from(qscale) - 1) | 1
        };

        let trace = std::env::var_os("WC3_BINK_TRACE").is_some();
        if trace {
            eprintln!("  >> tcoeff start bit={} intra={intra} q={qscale}", br.bit_pos());
        }
        let mut i: i32 = if ac_only { 0 } else { -1 };
        loop {
            let sym = vlc.decode(br).map_err(map_bit)?;
            if trace {
                eprintln!("     sym={sym} bit={}", br.bit_pos());
            }
            if sym == ESCAPE_SYMBOL {
                let b0 = br.read_bit().map_err(map_bit)?;
                if b0 == 1 {
                    let b1 = br.read_bit().map_err(map_bit)?;
                    if b1 == 1 {
                        self.esc3.set(self.esc3.get() + 1);
                        let _ = &self.esc3;
                        // Third escape: explicit last/run/level.
                        let last = br.read_bit().map_err(map_bit)?;
                        let run = br.read_bits(6).map_err(map_bit)? as i32;
                        let _marker = br.read_bit().map_err(map_bit)?;
                        let mut level = br.read_bits_i32(12).map_err(map_bit)?;
                        let _marker2 = br.read_bit().map_err(map_bit)?;
                        level = if level > 0 {
                            level * qmul + qadd
                        } else {
                            level * qmul - qadd
                        };
                        level = level.clamp(-2048, 2047);
                        if std::env::var_os("WC3_BINK_TRACE2").is_some() {
                            eprintln!("     E3 last={last} run={run} lvl={level} i_before={i}");
                        }
                        i += run + 1;
                        if last == 1 {
                            i += 192;
                        }
                        if i > 62 {
                            i -= 192;
                            if !(0..64).contains(&i) {
                                note_overrun();
                                block[scan[63] as usize] = level;
                                return Ok(63);
                            }
                            block[scan[i as usize] as usize] = level;
                            return Ok(i);
                        }
                        if !(0..64).contains(&i) {
                            note_overrun();
                            block[scan[63] as usize] = level;
                            return Ok(63);
                        }
                        block[scan[i as usize] as usize] = level;
                        continue;
                    }
                    self.esc2.set(self.esc2.get() + 1);
                    // Second escape: VLC (level, run), run extended by max_run.
                    let sym2 = vlc.decode(br).map_err(map_bit)?;
                    if sym2 as usize >= last_threshold && sym2 != ESCAPE_SYMBOL {
                        // fall through with last handling below
                    }
                    let (level2, run2, last2) = self.symbol_parts(sym2, last_threshold, qmul, qadd)?;
                    let level_idx = (level2 / qmul.max(1)).clamp(0, 63) as usize;
                    // The reference stores `last` as +192 inside the run, so a
                    // `last` continuation ends the block: without this the
                    // decoder runs past the block and desynchronises (found on
                    // HumanEd frame 194).
                    let last_bias = if last2 { 192 } else { 0 };
                    let maps = if intra { &self.intra_maps } else { &self.inter_maps };
                    i += run2
                        + last_bias
                        + 1
                        + i32::from(maps.max_run[usize::from(last2)][level_idx])
                        + 1;
                    let sign = -i32::from(br.read_bit().map_err(map_bit)? as i8 as i32);
                    let level = (level2 ^ sign) - sign;
                    if i > 62 {
                        i -= 192;
                        if !(0..64).contains(&i) {
                            note_overrun();
                            block[scan[63] as usize] = level;
                            return Ok(63);
                        }
                        block[scan[i as usize] as usize] = level;
                        return Ok(i);
                    }
                    if !(0..64).contains(&i) {
                        note_overrun();
                        block[scan[63] as usize] = level;
                        return Ok(63);
                    }
                    block[scan[i as usize] as usize] = level;
                    continue;
                }
                self.esc1.set(self.esc1.get() + 1);
                // First escape: VLC (level, run), level extended by max_level.
                let sym2 = vlc.decode(br).map_err(map_bit)?;
                let (level2, run2, last2) = self.symbol_parts(sym2, last_threshold, qmul, qadd)?;
                i += run2 + if last2 { 192 } else { 0 };
                let run_idx = (run2 - 1).clamp(0, 63) as usize;
                let maps = if intra { &self.intra_maps } else { &self.inter_maps };
                let mut level = level2 + i32::from(maps.max_level[usize::from(last2)][run_idx]) * qmul;
                let sign = -i32::from(br.read_bit().map_err(map_bit)? as i8 as i32);
                level = (level ^ sign) - sign;
                if i > 62 {
                    i -= 192;
                    if !(0..64).contains(&i) {
                        note_overrun();
                        block[scan[63] as usize] = level;
                        return Ok(63);
                    }
                    block[scan[i as usize] as usize] = level;
                    return Ok(i);
                }
                if !(0..64).contains(&i) {
                    note_overrun();
                    block[scan[63] as usize] = level;
                    return Ok(63);
                }
                block[scan[i as usize] as usize] = level;
                continue;
            }

            let idx = sym as usize;
            if idx >= tables::INTRA_RUN.len() {
                return Err(BinkError::Corrupt("tcoeff symbol out of range"));
            }
            let (raw_run, raw_level) = if intra {
                (tables::INTRA_RUN[idx], tables::INTRA_LEVEL[idx])
            } else {
                (tables::INTER_RUN[idx], tables::INTER_LEVEL[idx])
            };
            let last = if intra {
                idx >= self.intra_last
            } else {
                idx >= self.inter_last
            };
            let level = i32::from(raw_level) * qmul + qadd;
            let sign_bit = br.read_bit().map_err(map_bit)?;
            let sign = -i32::from(sign_bit as i8 as i32);
            let level = (level ^ sign) - sign;
            i += i32::from(raw_run) + 1;
            if last {
                i += 192;
            }
            if std::env::var_os("WC3_BINK_TRACE2").is_some() {
                eprintln!(
                    "     N sym={sym} run={raw_run} lvl={raw_level} sign={sign_bit} last={last} i={i} bit={}",
                    br.bit_pos()
                );
            }
            if i > 62 {
                i -= 192;
                if !(0..64).contains(&i) {
                    note_overrun();
                    block[scan[63] as usize] = level;
                    return Ok(63);
                }
                block[scan[i as usize] as usize] = level;
                return Ok(i);
            }
            if !(0..64).contains(&i) {
                note_overrun();
                block[scan[63] as usize] = level;
                return Ok(63);
            }
            block[scan[i as usize] as usize] = level;
        }
    }

    pub fn escapes1(&self) -> u32 {
        self.esc1.get()
    }
    pub fn escapes2(&self) -> u32 {
        self.esc2.get()
    }
    pub fn escapes3(&self) -> u32 {
        self.esc3.get()
    }

    /// `(level, run, last)` for a non-escape symbol, with quantiser scaling.
    fn symbol_parts(
        &self,
        sym: u16,
        last_threshold: usize,
        qmul: i32,
        qadd: i32,
    ) -> Result<(i32, i32, bool), BinkError> {
        let idx = sym as usize;
        if idx >= tables::INTRA_RUN.len() {
            return Err(BinkError::Corrupt("escape continuation out of range"));
        }
        let (run, level) = if last_threshold == self.intra_last {
            (tables::INTRA_RUN[idx], tables::INTRA_LEVEL[idx])
        } else {
            (tables::INTER_RUN[idx], tables::INTER_LEVEL[idx])
        };
        Ok((
            i32::from(level) * qmul + qadd,
            i32::from(run) + 1,
            idx >= last_threshold,
        ))
    }
}

impl Default for CoeffTables {
    fn default() -> Self {
        Self::new()
    }
}

/// DC predictor state for one block position.
pub struct DcPredict {
    pub pred: i32,
    /// 0 = predict from the left, 1 = predict from the top.
    pub dir: u8,
}

/// Compute the MPEG-4 DC prediction for a block (`n` 0..3 luma, 4/5 chroma).
pub fn dc_predict(
    pic: &Picture,
    plane: Plane,
    bx8: usize,
    by8: usize,
    mb_x: usize,
    mb_y: usize,
    n: usize,
) -> DcPredict {
    let (mut a, mut b, mut c) = pic.dc_neighbours(plane, bx8, by8);
    // Outside-of-picture predictors are 1024; the guard ring already holds
    // that, but the first coded row needs the spec's explicit values.
    if mb_y == 0 {
        if n != 3 {
            if n != 2 {
                b = 1024;
                c = 1024;
            }
            if n != 1 && mb_x == 0 {
                b = 1024;
                a = 1024;
            }
        }
    }
    // The reference decoder also forces the top-left predictor at the first
    // MB of the second row only (`mb_x == resync_mb_x`); applying it to the
    // whole row corrupts every prediction direction from MB 1 on.
    if mb_y == 1 && mb_x == 0 && matches!(n, 0 | 4 | 5) {
        b = 1024;
    }
    let (a, b, c) = (i32::from(a), i32::from(b), i32::from(c));
    if std::env::var_os("WC3_BINK_DCPRED").is_some() {
        eprintln!("   dcpred n={n} ({mb_x},{mb_y}) a={a} b={b} c={c}");
    }
    if (a - b).abs() < (b - c).abs() {
        DcPredict { pred: c, dir: 1 }
    } else {
        DcPredict { pred: a, dir: 0 }
    }
}

/// Rebuild the quantised DC level from a differential and the predictor, and
/// store the dequantised value for future prediction. Mirrors
/// `mpeg4_get_level_dc`.
pub fn dc_reconstruct(
    pic: &mut Picture,
    plane: Plane,
    bx8: usize,
    by8: usize,
    pred: i32,
    diff: i32,
    scale: i32,
) -> i32 {
    let pred_q = (pred + (scale >> 1)) / scale.max(1);
    let level = diff + pred_q;
    let mut dequant = level * scale;
    if dequant & !2047 != 0 {
        dequant = if dequant < 0 { 0 } else { 2047 };
    }
    *pic.dc_ptr(plane, bx8, by8) = dequant as i16;
    level
}

/// Apply the MPEG-4 AC prediction to a quantised block and update the AC
/// predictor memory. `dir` comes from the DC prediction.
pub fn ac_predict(
    pic: &mut Picture,
    plane: Plane,
    bx8: usize,
    by8: usize,
    mb_x: usize,
    mb_y: usize,
    n: usize,
    cur_q: u8,
    block: &mut [i32; 64],
    dir: u8,
    predicted: bool,
) {
    let nb_left = if bx8 > 0 { Some((bx8 - 1, by8)) } else { None };
    let nb_top = if by8 > 0 { Some((bx8, by8 - 1)) } else { None };

    if predicted && dir == 0 {
        // Predict the left column from the block to the left.
        if let Some((lx, ly)) = nb_left {
            let left = *pic.ac_ref(plane, lx, ly);
            let left_q = pic.qscale[mb_y * pic.mb_w + mb_x.saturating_sub(1)];
            let same = mb_x == 0 || cur_q == left_q || n == 1 || n == 3;
            for i in 1..8 {
                let v = i32::from(left[i]);
                let add = if same {
                    v
                } else {
                    rounded_div(v * i32::from(left_q), i32::from(cur_q))
                };
                block[i * 8] += add;
            }
        }
    } else if predicted {
        let Some((tx, ty)) = nb_top else {
            store_ac(pic, plane, bx8, by8, block);
            return;
        };
        let top = *pic.ac_ref(plane, tx, ty);
        let top_q = if mb_y > 0 {
            pic.qscale[(mb_y - 1) * pic.mb_w + mb_x]
        } else {
            cur_q
        };
        let same = mb_y == 0 || cur_q == top_q || n == 2 || n == 3;
        for i in 1..8 {
            let v = i32::from(top[8 + i]);
            let add = if same {
                v
            } else {
                rounded_div(v * i32::from(top_q), i32::from(cur_q))
            };
            block[i] += add;
        }
    }

    // The reference stores the block's column and row for *every* intra
    // block, predicted or not; skipping the store on non-predicted blocks
    // makes later neighbours predict from zeros (measured: 35.9 dB -> exact).
    store_ac(pic, plane, bx8, by8, block);
}

fn store_ac(
    pic: &mut Picture,
    plane: Plane,
    bx8: usize,
    by8: usize,
    block: &[i32; 64],
) {
    let out = pic.ac_mut(plane, bx8, by8);
    for i in 1..8 {
        out[i] = block[i * 8].clamp(-32768, 32767) as i16;
        out[8 + i] = block[i].clamp(-32768, 32767) as i16;
    }
}

/// Round a ratio the way the reference decoder does (`ROUNDED_DIV`): halves
/// round away from zero, so negative predictions are not biased towards zero.
#[inline]
fn rounded_div(a: i32, b: i32) -> i32 {
    if b == 0 {
        return 0;
    }
    if a >= 0 {
        (a + (b >> 1)) / b
    } else {
        (a - (b >> 1)) / b
    }
}

/// Inverse-quantise an intra block (quantised -> spatial coefficients).
pub fn dequant_intra(
    block: &[i32; 64],
    out: &mut [i32; 64],
    qscale: u8,
    quant_type: bool,
    matrix: &[u16; 64],
    dc_scale: i32,
) {
    // The reference keeps coefficients in an int16 block, so the result is
    // truncated to that range before the IDCT.
    out[0] = (block[0] * dc_scale) as i16 as i32;
    if quant_type {
        let q = i32::from(qscale) << 1;
        for i in 1..64 {
            let level = block[i];
            if level == 0 {
                out[i] = 0;
                continue;
            }
            let v = if level < 0 {
                -(((-level) * q * i32::from(matrix[i])) >> 4)
            } else {
                (level * q * i32::from(matrix[i])) >> 4
            };
            out[i] = v as i16 as i32;
        }
    } else {
        let qmul = i32::from(qscale) << 1;
        let qadd = (i32::from(qscale) - 1) | 1;
        for i in 1..64 {
            let level = block[i];
            if level == 0 {
                out[i] = 0;
                continue;
            }
            let v = if level < 0 {
                level * qmul - qadd
            } else {
                level * qmul + qadd
            };
            out[i] = v as i16 as i32;
        }
    }
}

/// Inverse-quantise an inter block.
///
/// With H.263 quantisation (`quant_type == 0`) the inter levels are already
/// dequantised inside [`CoeffTables::decode_tcoeff`] — the reference decoder
/// installs no unquantiser for that case — so this is a plain conversion.
/// Scaling here as well doubles the residual (measured as a strong vertical
/// gradient on HumanEd frame 193).
pub fn dequant_inter(
    block: &[i32; 64],
    out: &mut [i32; 64],
    qscale: u8,
    quant_type: bool,
    matrix: &[u16; 64],
) {
    if !quant_type {
        for i in 0..64 {
            out[i] = block[i] as i16 as i32;
        }
        return;
    }
    if quant_type {
        let q = i32::from(qscale) << 1;
        for i in 0..64 {
            let level = block[i];
            if level == 0 {
                out[i] = 0;
                continue;
            }
            let v = if level < 0 {
                -(((((-level) << 1) + 1) * q * i32::from(matrix[i])) >> 5)
            } else {
                (((level << 1) + 1) * q * i32::from(matrix[i])) >> 5
            };
            out[i] = v as i16 as i32;
        }
    } else {
        let qmul = i32::from(qscale) << 1;
        let qadd = (i32::from(qscale) - 1) | 1;
        for i in 0..64 {
            let level = block[i];
            out[i] = if level == 0 {
                0
            } else if level < 0 {
                (level * qmul - qadd) as i16 as i32
            } else {
                (level * qmul + qadd) as i16 as i32
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xbits_matches_sign_magnitude_encoding() {
        // n=1: bit 1 -> +1, bit 0 -> -1
        let mut br = BitReader::new(&[0b1000_0000]);
        assert_eq!(CoeffTables::xbits(&mut br, 1).unwrap(), 1);
        let mut br = BitReader::new(&[0b0000_0000]);
        assert_eq!(CoeffTables::xbits(&mut br, 1).unwrap(), -1);
        // n=2: 11 -> +3, 10 -> +2, 01 -> -2, 00 -> -3
        for (bits, want) in [(0b1100_0000u8, 3), (0b1000_0000, 2), (0b0100_0000, -2), (0, -3)] {
            let data = [bits];
            let mut br = BitReader::new(&data);
            assert_eq!(CoeffTables::xbits(&mut br, 2).unwrap(), want, "bits {bits:08b}");
        }
    }

    #[test]
    fn dequant_h263_matches_reference_formula() {
        let mut block = [0i32; 64];
        block[0] = 8;
        block[1] = 2;
        block[5] = -3;
        let mut out = [0i32; 64];
        let matrix = tables::DEFAULT_INTRA_MATRIX;
        dequant_intra(&block, &mut out, 4, false, &matrix, 10);
        // DC: 8 * 10 = 80
        assert_eq!(out[0], 80);
        // AC: qmul = 8, qadd = (4-1)|1 = 3 -> 2*8+3 = 19, -3*8-3 = -27
        assert_eq!(out[1], 19);
        assert_eq!(out[5], -27);
    }

    #[test]
    fn dequant_matrix_path_uses_the_matrix() {
        let mut block = [0i32; 64];
        block[0] = 1;
        block[1] = 4;
        let mut out = [0i32; 64];
        let matrix = tables::DEFAULT_INTRA_MATRIX;
        dequant_intra(&block, &mut out, 2, true, &matrix, 8);
        // q = 4; (4 * 4 * 17) >> 4 = 17
        assert_eq!(out[1], 17);
    }

    #[test]
    fn escaped_run_is_rejected_when_out_of_range() {
        // 13 zero bits then a code that cannot fit: must be an error, not a
        // panic (index safety is the whole point of the range checks).
        let data = [0u8; 8];
        let ctx = CoeffTables::new();
        let mut br = BitReader::new(&data);
        let mut block = [0i32; 64];
        let r = ctx.decode_tcoeff(&mut br, true, 4, false, &tables::ZIGZAG, &mut block, false);
        assert!(r.is_err());
    }
}
