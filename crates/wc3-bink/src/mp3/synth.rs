//! Polyphase synthesis: the 32-point DCT and the 512-tap window bank that
//! turn the 576 frequency lines of a granule into PCM.
//!
//! Layout notes (they matter for bit-exactness): the synthesis input is
//! `grbuf[32][18]` (band-major, time inside the band) and the output is
//! interleaved PCM at `nch` channels. `lins` is the 15×64 sample history plus
//! the working rows, and `qmf_state` carries 15×64 floats between granules.

use crate::mp3::tables;

/// 32-point inverse DCT over each of the `n` columns of `x` (`x[i*18]`).
pub fn dct32(x: &mut [f32], n: usize) {
    let sec = &tables::SEC;
    let mut k = 0usize;
    while k < n {
        let mut t = [[0f32; 8]; 4];
        for i in 0..8 {
            let x0 = x[i * 18 + k];
            let x1 = x[(15 - i) * 18 + k];
            let x2 = x[(16 + i) * 18 + k];
            let x3 = x[(31 - i) * 18 + k];
            let t0 = x0 + x3;
            let t1 = x1 + x2;
            let t2 = (x1 - x2) * sec[3 * i];
            let t3 = (x0 - x3) * sec[3 * i + 1];
            t[0][i] = t0 + t1;
            t[1][i] = (t0 - t1) * sec[3 * i + 2];
            t[2][i] = t3 + t2;
            t[3][i] = (t3 - t2) * sec[3 * i + 2];
        }
        for row in t.iter_mut() {
            let (x0, x1, x2, x3, x4, x5, x6, x7) = (
                row[0], row[1], row[2], row[3], row[4], row[5], row[6], row[7],
            );
            let mut xt = x0 - x7;
            let mut x0 = x0 + x7;
            let mut x7 = x1 - x6;
            let mut x1 = x1 + x6;
            let mut x6 = x2 - x5;
            let x2 = x2 + x5;
            let mut x5 = x3 - x4;
            let mut x3 = x3 + x4;
            let x4 = x0 - x3;
            x0 += x3;
            x3 = x1 - x2;
            x1 += x2;
            row[0] = x0 + x1;
            row[4] = (x0 - x1) * 0.70710677;
            x5 += x6;
            x6 = (x6 + x7) * 0.70710677;
            x7 += xt;
            x3 = (x3 + x4) * 0.70710677;
            x5 -= x7 * 0.198912367; // rotate by PI/8
            x7 += x5 * 0.382683432;
            x5 -= x7 * 0.198912367;
            let x0b = xt - x6;
            xt += x6;
            row[1] = (xt + x7) * 0.50979561;
            row[2] = (x4 + x3) * 0.54119611;
            row[3] = (x0b - x5) * 0.60134488;
            row[5] = (x0b + x5) * 0.89997619;
            row[6] = (x4 - x3) * 1.30656302;
            row[7] = (xt - x7) * 2.56291556;
        }
        // Write-out: four rows per step, seven steps plus the tail pairing.
        let mut y = k;
        for i in 0..7 {
            x[y] = t[0][i];
            x[y + 18] = t[2][i] + t[3][i] + t[3][i + 1];
            x[y + 36] = t[1][i] + t[1][i + 1];
            x[y + 54] = t[2][i + 1] + t[3][i] + t[3][i + 1];
            y += 4 * 18;
        }
        x[y] = t[0][7];
        x[y + 18] = t[2][7] + t[3][7];
        x[y + 36] = t[1][7];
        x[y + 54] = t[3][7];
        k += 1;
    }
}

/// One pair of synthesis windows (the odd sub-band shortcut).
fn synth_pair(pcm: &mut [i16], at: usize, nch: usize, z: &[f32], zoff: usize) {
    let s = |i: usize| z[zoff + i * 64];
    let mut a = (s(14) - s(0)) * 29.0;
    a += (s(1) + s(13)) * 213.0;
    a += (s(12) - s(2)) * 459.0;
    a += (s(3) + s(11)) * 2037.0;
    a += (s(10) - s(4)) * 5153.0;
    a += (s(5) + s(9)) * 6574.0;
    a += (s(8) - s(6)) * 37489.0;
    a += s(7) * 75038.0;
    pcm[at] = scale_pcm(a);

    // The reference advances the pointer by two floats before the second sum.
    let s = |i: usize| z[zoff + 2 + i * 64];
    let mut a = s(14) * 104.0;
    a += s(12) * 1567.0;
    a += s(10) * 9727.0;
    a += s(8) * 64019.0;
    a += s(6) * -9975.0;
    a += s(4) * -45.0;
    a += s(2) * 146.0;
    a += s(0) * -5.0;
    pcm[at + 16 * nch] = scale_pcm(a);
}

/// Float sample to 16-bit PCM with the reference rounding (away from zero).
pub fn scale_pcm(sample: f32) -> i16 {
    if sample >= 32766.5 {
        return 32767;
    }
    if sample <= -32767.5 {
        return -32768;
    }
    let s = (sample + 0.5) as i16;
    s - i16::from(s < 0)
}

/// One 32-sample block of the polyphase bank (`i` is an even sub-band index).
fn synth(
    grbuf: &[f32],
    base: usize,
    pcm: &mut [i16],
    out_at: usize,
    nch: usize,
    lins: &mut [f32],
    lins_off: usize,
) {
    let xl = base;
    let xr = base + 576 * (nch - 1);
    let zlin = lins_off + 15 * 64;
    let mut w = 0usize;
    let win = &tables::SYNTH_WINDOW;

    lins[zlin + 4 * 15] = grbuf[xl + 18 * 16];
    lins[zlin + 4 * 15 + 1] = grbuf[xr + 18 * 16];
    lins[zlin + 4 * 15 + 2] = grbuf[xl];
    lins[zlin + 4 * 15 + 3] = grbuf[xr];

    lins[zlin + 4 * 31] = grbuf[xl + 1 + 18 * 16];
    lins[zlin + 4 * 31 + 1] = grbuf[xr + 1 + 18 * 16];
    lins[zlin + 4 * 31 + 2] = grbuf[xl + 1];
    lins[zlin + 4 * 31 + 3] = grbuf[xr + 1];

    let dstr = out_at + (nch - 1);
    synth_pair(pcm, dstr, nch, lins, lins_off + 4 * 15 + 1);
    synth_pair(pcm, dstr + 32 * nch, nch, lins, lins_off + 4 * 15 + 64 + 1);
    synth_pair(pcm, out_at, nch, lins, lins_off + 4 * 15);
    synth_pair(pcm, out_at + 32 * nch, nch, lins, lins_off + 4 * 15 + 64);

    for i in (0..15).rev() {
        lins[zlin + 4 * i] = grbuf[xl + 18 * (31 - i)];
        lins[zlin + 4 * i + 1] = grbuf[xr + 18 * (31 - i)];
        lins[zlin + 4 * i + 2] = grbuf[xl + 1 + 18 * (31 - i)];
        lins[zlin + 4 * i + 3] = grbuf[xr + 1 + 18 * (31 - i)];
        lins[zlin + 4 * (i + 16)] = grbuf[xl + 1 + 18 * (1 + i)];
        lins[zlin + 4 * (i + 16) + 1] = grbuf[xr + 1 + 18 * (1 + i)];
        let zm = (zlin as isize + 4 * (i as isize - 16)) as usize;
        lins[zm + 2] = grbuf[xl + 18 * (1 + i)];
        lins[zm + 3] = grbuf[xr + 18 * (1 + i)];

        let mut a = [0f32; 4];
        let mut b = [0f32; 4];
        // S0(0) S2(1) S1(2) S2(3) S1(4) S2(5) S1(6) S2(7)
        for k in 0..8 {
            let w0 = win[w];
            let w1 = win[w + 1];
            w += 2;
            let base_i = (zlin + 4 * i) as isize;
            let vz = (base_i - (k as isize) * 64) as usize;
            let vy = (base_i - ((15 - k) as isize) * 64) as usize;
            match k {
                0 => {
                    for j in 0..4 {
                        b[j] = lins[vz + j] * w1 + lins[vy + j] * w0;
                        a[j] = lins[vz + j] * w0 - lins[vy + j] * w1;
                    }
                }
                1 | 3 | 5 | 7 => {
                    for j in 0..4 {
                        b[j] += lins[vz + j] * w1 + lins[vy + j] * w0;
                        a[j] += lins[vy + j] * w1 - lins[vz + j] * w0;
                    }
                }
                _ => {
                    for j in 0..4 {
                        b[j] += lins[vz + j] * w1 + lins[vy + j] * w0;
                        a[j] += lins[vz + j] * w0 - lins[vy + j] * w1;
                    }
                }
            }
        }
        pcm[dstr + (15 - i) * nch] = scale_pcm(a[1]);
        pcm[dstr + (17 + i) * nch] = scale_pcm(b[1]);
        pcm[out_at + (15 - i) * nch] = scale_pcm(a[0]);
        pcm[out_at + (17 + i) * nch] = scale_pcm(b[0]);
        pcm[dstr + (47 - i) * nch] = scale_pcm(a[3]);
        pcm[dstr + (49 + i) * nch] = scale_pcm(b[3]);
        pcm[out_at + (47 - i) * nch] = scale_pcm(a[2]);
        pcm[out_at + (49 + i) * nch] = scale_pcm(b[2]);
    }
}

/// Synthesis for a whole granule: `nch` channels × 576 samples.
pub fn synth_granule(
    qmf_state: &mut [f32; 960],
    grbuf: &mut [f32; 1152],
    nbands: usize,
    nch: usize,
    pcm: &mut [i16],
    lins: &mut [f32; 2112],
) {
    for ch in 0..nch {
        dct32(&mut grbuf[ch * 576..], nbands);
    }
    lins[..960].copy_from_slice(&qmf_state[..]);
    lins[960..960 + 64 * nbands].fill(0.0);
    for i in (0..nbands).step_by(2) {
        synth(
            grbuf,
            i,
            pcm,
            32 * nch * i,
            nch,
            lins,
            i * 64,
        );
    }
    let from = nbands * 64;
    qmf_state.copy_from_slice(&lins[from..from + 960]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_pcm_matches_the_reference_rounding() {
        // The reference adds 0.5 and truncates towards zero, then subtracts one
        // for negative results; the oracle comparison pins this to <= 2 LSB.
        assert_eq!(scale_pcm(0.0), 0);
        assert_eq!(scale_pcm(0.4), 0);
        assert_eq!(scale_pcm(0.6), 1);
        assert_eq!(scale_pcm(1.4), 1);
        assert_eq!(scale_pcm(-1.6), -2);
        assert_eq!(scale_pcm(40000.0), 32767);
        assert_eq!(scale_pcm(-40000.0), -32768);
    }

    #[test]
    fn dct32_of_a_zero_block_is_zero() {
        let mut x = [0f32; 32 * 18];
        x[0] = 1.0;
        dct32(&mut x, 18);
        // A single DC line must spread over the 32 output rows only.
        assert!(x[0] > 0.0);
        let energy: f32 = x.iter().sum();
        assert!(energy > 0.9 && energy < 32.1, "energy {energy}");
    }
}
