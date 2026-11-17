//! 8x8 inverse DCT, bit-exact with the integer transform the reference
//! decoder uses for MPEG-4.
//!
//! The float transform this replaced differed from the integer one by one
//! level in ~5% of samples, which is invisible on a single frame but compounds
//! through P-frame prediction (measured: 40 dB on an I-frame degrading to
//! 25 dB thirty frames later). The integer transform below is the classic
//! "simple IDCT" butterfly with 14-bit constants; the arithmetic (unsigned
//! wrapping 32-bit accumulators, `ROW_SHIFT`/`COL_SHIFT` rounding) follows it
//! exactly so decoded pictures match the oracle sample for sample.

/// cos(i*pi/16)*sqrt(2)*(1<<14) + 0.5
const W1: i32 = 22725;
const W2: i32 = 21407;
const W3: i32 = 19266;
const W4: i32 = 16383;
const W5: i32 = 12873;
const W6: i32 = 8867;
const W7: i32 = 4520;

const ROW_SHIFT: u32 = 11;
const COL_SHIFT: u32 = 20;
const DC_SHIFT: u32 = 3;

#[inline]
fn mul(a: i32, b: i32) -> u32 {
    (a as u32).wrapping_mul(b as u32)
}

/// One row pass (`idctRowCondDC`).
#[inline]
fn idct_row(row: &mut [i32; 8]) {
    if row[1..].iter().all(|&v| v == 0) {
        // DC-only rows are filled with the DC value scaled by the fixed-point
        // domain, truncated to 16 bits like the reference's int16 block.
        let v = ((row[0].wrapping_shl(DC_SHIFT)) as i16) as i32;
        row.fill(v);
        return;
    }

    let mut a0 = mul(W4, row[0]).wrapping_add(1 << (ROW_SHIFT - 1));
    let mut a1 = a0;
    let mut a2 = a0;
    let mut a3 = a0;

    a0 = a0.wrapping_add(mul(W2, row[2]));
    a1 = a1.wrapping_add(mul(W6, row[2]));
    a2 = a2.wrapping_sub(mul(W6, row[2]));
    a3 = a3.wrapping_sub(mul(W2, row[2]));

    let mut b0 = mul(W1, row[1]);
    let mut b1 = mul(W3, row[1]);
    let mut b2 = mul(W5, row[1]);
    let mut b3 = mul(W7, row[1]);

    b0 = b0.wrapping_add(mul(W3, row[3]));
    b1 = b1.wrapping_add(mul(-W7, row[3]));
    b2 = b2.wrapping_add(mul(-W1, row[3]));
    b3 = b3.wrapping_add(mul(-W5, row[3]));

    if row[4] != 0 || row[5] != 0 || row[6] != 0 || row[7] != 0 {
        a0 = a0.wrapping_add(mul(W4, row[4])).wrapping_add(mul(W6, row[6]));
        a1 = a1.wrapping_add(mul(-W4, row[4])).wrapping_add(mul(-W2, row[6]));
        a2 = a2.wrapping_add(mul(-W4, row[4])).wrapping_add(mul(W2, row[6]));
        a3 = a3.wrapping_add(mul(W4, row[4])).wrapping_add(mul(-W6, row[6]));

        b0 = b0.wrapping_add(mul(W5, row[5])).wrapping_add(mul(W7, row[7]));
        b1 = b1.wrapping_add(mul(-W1, row[5])).wrapping_add(mul(-W5, row[7]));
        b2 = b2.wrapping_add(mul(W7, row[5])).wrapping_add(mul(W3, row[7]));
        b3 = b3.wrapping_add(mul(W3, row[5])).wrapping_add(mul(-W1, row[7]));
    }

    row[0] = ((a0.wrapping_add(b0)) as i32) >> ROW_SHIFT;
    row[7] = ((a0.wrapping_sub(b0)) as i32) >> ROW_SHIFT;
    row[1] = ((a1.wrapping_add(b1)) as i32) >> ROW_SHIFT;
    row[6] = ((a1.wrapping_sub(b1)) as i32) >> ROW_SHIFT;
    row[2] = ((a2.wrapping_add(b2)) as i32) >> ROW_SHIFT;
    row[5] = ((a2.wrapping_sub(b2)) as i32) >> ROW_SHIFT;
    row[3] = ((a3.wrapping_add(b3)) as i32) >> ROW_SHIFT;
    row[4] = ((a3.wrapping_sub(b3)) as i32) >> ROW_SHIFT;
}

/// Column butterflies shared by every output mode.
#[inline]
fn idct_cols(col: &[i32; 8]) -> [i32; 8] {
    let bias = ((1 << (COL_SHIFT - 1)) / W4 as u32) as i32;
    let mut a0 = mul(W4, col[0].wrapping_add(bias));
    let mut a1 = a0;
    let mut a2 = a0;
    let mut a3 = a0;

    a0 = a0.wrapping_add(mul(W2, col[2]));
    a1 = a1.wrapping_add(mul(W6, col[2]));
    a2 = a2.wrapping_sub(mul(W6, col[2]));
    a3 = a3.wrapping_sub(mul(W2, col[2]));

    let mut b0 = mul(W1, col[1]);
    let mut b1 = mul(W3, col[1]);
    let mut b2 = mul(W5, col[1]);
    let mut b3 = mul(W7, col[1]);

    b0 = b0.wrapping_add(mul(W3, col[3]));
    b1 = b1.wrapping_add(mul(-W7, col[3]));
    b2 = b2.wrapping_add(mul(-W1, col[3]));
    b3 = b3.wrapping_add(mul(-W5, col[3]));

    if col[4] != 0 {
        a0 = a0.wrapping_add(mul(W4, col[4]));
        a1 = a1.wrapping_add(mul(-W4, col[4]));
        a2 = a2.wrapping_add(mul(-W4, col[4]));
        a3 = a3.wrapping_add(mul(W4, col[4]));
    }
    if col[5] != 0 {
        b0 = b0.wrapping_add(mul(W5, col[5]));
        b1 = b1.wrapping_add(mul(-W1, col[5]));
        b2 = b2.wrapping_add(mul(W7, col[5]));
        b3 = b3.wrapping_add(mul(W3, col[5]));
    }
    if col[6] != 0 {
        a0 = a0.wrapping_add(mul(W6, col[6]));
        a1 = a1.wrapping_add(mul(-W2, col[6]));
        a2 = a2.wrapping_add(mul(W2, col[6]));
        a3 = a3.wrapping_add(mul(-W6, col[6]));
    }
    if col[7] != 0 {
        b0 = b0.wrapping_add(mul(W7, col[7]));
        b1 = b1.wrapping_add(mul(-W5, col[7]));
        b2 = b2.wrapping_add(mul(W3, col[7]));
        b3 = b3.wrapping_add(mul(-W1, col[7]));
    }

    [
        ((a0.wrapping_add(b0)) as i32) >> COL_SHIFT,
        ((a1.wrapping_add(b1)) as i32) >> COL_SHIFT,
        ((a2.wrapping_add(b2)) as i32) >> COL_SHIFT,
        ((a3.wrapping_add(b3)) as i32) >> COL_SHIFT,
        ((a3.wrapping_sub(b3)) as i32) >> COL_SHIFT,
        ((a2.wrapping_sub(b2)) as i32) >> COL_SHIFT,
        ((a1.wrapping_sub(b1)) as i32) >> COL_SHIFT,
        ((a0.wrapping_sub(b0)) as i32) >> COL_SHIFT,
    ]
}

/// Shared driver: row pass into a local copy, then column pass.
#[inline]
fn transform(block: &[i32; 64]) -> [i32; 64] {
    let mut tmp = *block;
    for i in 0..8 {
        let mut row = [0i32; 8];
        row.copy_from_slice(&tmp[i * 8..i * 8 + 8]);
        idct_row(&mut row);
        tmp[i * 8..i * 8 + 8].copy_from_slice(&row);
    }
    let mut out = [0i32; 64];
    for x in 0..8 {
        let mut col = [0i32; 8];
        for (k, c) in col.iter_mut().enumerate() {
            *c = tmp[k * 8 + x];
        }
        let res = idct_cols(&col);
        for (k, v) in res.iter().enumerate() {
            out[k * 8 + x] = *v;
        }
    }
    out
}

/// IDCT and store clamped samples (MPEG-4 intra: absolute values).
pub fn idct_put(block: &[i32; 64], dst: &mut [u8], stride: usize) {
    let out = transform(block);
    for y in 0..8 {
        for x in 0..8 {
            dst[y * stride + x] = out[y * 8 + x].clamp(0, 255) as u8;
        }
    }
}

/// IDCT and add the residual to the prediction, clamped.
pub fn idct_add(block: &[i32; 64], dst: &mut [u8], stride: usize) {
    let out = transform(block);
    for y in 0..8 {
        for x in 0..8 {
            let v = i32::from(dst[y * stride + x]) + out[y * 8 + x];
            dst[y * stride + x] = v.clamp(0, 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_only_produces_flat_block() {
        // DC 1024 -> mean 128
        let mut block = [0i32; 64];
        block[0] = 1024;
        let mut plane = [0u8; 64];
        idct_put(&block, &mut plane, 8);
        assert!(plane.iter().all(|&p| p == 128), "got {:?}", &plane[..8]);

        // DC 0 -> 0
        let block = [0i32; 64];
        idct_put(&block, &mut plane, 8);
        assert!(plane.iter().all(|&p| p == 0));
    }

    #[test]
    fn matches_float_reference_within_one_level() {
        // Compare against a direct double-precision MPEG-4 IDCT; the integer
        // transform is allowed to differ by one level, never more.
        fn reference(block: &[f64; 64]) -> [f64; 64] {
            let mut out = [0f64; 64];
            for y in 0..8 {
                for x in 0..8 {
                    let mut acc = 0f64;
                    for v in 0..8 {
                        for u in 0..8 {
                            let cu = if u == 0 {
                                std::f64::consts::FRAC_1_SQRT_2
                            } else {
                                1.0
                            };
                            let cv = if v == 0 {
                                std::f64::consts::FRAC_1_SQRT_2
                            } else {
                                1.0
                            };
                            let ax = ((2 * x + 1) as f64) * (u as f64)
                                * std::f64::consts::PI
                                / 16.0;
                            let ay = ((2 * y + 1) as f64) * (v as f64)
                                * std::f64::consts::PI
                                / 16.0;
                            acc += cu * cv * block[v * 8 + u] * ax.cos() * ay.cos();
                        }
                    }
                    out[y * 8 + x] = acc * 0.25;
                }
            }
            out
        }

        let mut state = 0xDEAD_BEEF_1234_5678u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..32 {
            let mut block = [0i32; 64];
            let mut want_in = [0f64; 64];
            for i in 0..64 {
                let scale = if i % 8 == 0 { 400 } else { 40 };
                let v = ((next() % 2001) as i32 - 1000) * scale / 1000;
                block[i] = v;
                want_in[i] = f64::from(v);
            }
            let want = reference(&want_in);
            let got = transform(&block);
            for i in 0..64 {
                let d = (f64::from(got[i]) - want[i]).abs();
                assert!(d <= 1.5, "coef {i}: integer {} vs float {}", got[i], want[i]);
            }
        }
    }

    #[test]
    fn add_is_clamped() {
        let mut block = [0i32; 64];
        block[0] = 1024; // +128
        let mut plane = [200u8; 64];
        idct_add(&block, &mut plane, 8);
        assert!(plane.iter().all(|&p| p == 255));
        let mut block = [0i32; 64];
        block[0] = -2048; // -256
        let mut plane = [10u8; 64];
        idct_add(&block, &mut plane, 8);
        assert!(plane.iter().all(|&p| p == 0));
    }
}
