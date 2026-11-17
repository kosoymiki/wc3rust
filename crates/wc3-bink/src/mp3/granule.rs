//! Granule post-processing: stereo reconstruction, short-block reordering,
//! alias reduction, IMDCT with overlap-add and frequency inversion.
//!
//! The order follows ISO/IEC 11172-3 §2.4.3.4 (stereo, reorder, alias
//! reduction, IMDCT, frequency inversion).

use crate::mp3::tables;
use crate::mp3::{BandKind, GranuleInfo, Header};

/// Mid/side → left/right for `n` spectral lines.
pub fn midside_stereo(left: &mut [f32], right: &mut [f32], n: usize) {
    for i in 0..n {
        let a = left[i];
        let b = right[i];
        left[i] = a + b;
        right[i] = a - b;
    }
}

fn intensity_band(left: &mut [f32], right: &mut [f32], n: usize, kl: f32, kr: f32) {
    for i in 0..n {
        right[i] = left[i] * kr;
        left[i] = left[i] * kl;
    }
}

fn stereo_top_band(right: &[f32], sfb: &[u8], nbands: usize) -> [i32; 3] {
    let mut max_band = [-1i32; 3];
    let mut off = 0usize;
    for i in 0..nbands {
        let w = usize::from(sfb[i]);
        let mut k = 0usize;
        while k + 1 < w {
            if right[off + k] != 0.0 || right[off + k + 1] != 0.0 {
                max_band[i % 3] = i as i32;
                break;
            }
            k += 2;
        }
        off += w;
    }
    max_band
}

fn stereo_process(
    left: &mut [f32],
    right: &mut [f32],
    ist_pos: &[u8; 39],
    sfb: &[u8],
    hdr: &Header,
    max_band: [i32; 3],
) {
    let pan = &tables::PAN;
    let mut off = 0usize;
    let mut i = 0usize;
    while sfb[i] != 0 {
        let w = usize::from(sfb[i]);
        let ipos = usize::from(ist_pos[i]);
        if i as i32 > max_band[i % 3] && ipos < 7 {
            let s = if hdr.is_ms_stereo() { 1.41421356f32 } else { 1.0 };
            let kl = pan[2 * ipos] * s;
            let kr = pan[2 * ipos + 1] * s;
            intensity_band(&mut left[off..], &mut right[off..], w, kl, kr);
        } else if hdr.is_ms_stereo() {
            midside_stereo(&mut left[off..], &mut right[off..], w);
        }
        off += w;
        i += 1;
    }
}

/// Intensity stereo for one granule pair (MPEG-1 uses 7 pan positions).
pub fn intensity_stereo(
    left: &mut [f32],
    right: &mut [f32],
    ist_pos: &mut [u8; 39],
    gr: &GranuleInfo,
    hdr: &Header,
    sfbtab: &[u8],
) {
    let (n_long, n_short) = match gr.band_kind {
        BandKind::Long => (22usize, 0usize),
        BandKind::Short => (0, 39),
        BandKind::Mixed => (8, 30),
    };
    let n_sfb = n_long + n_short;
    let max_blocks = if n_short > 0 { 3 } else { 1 };
    let mut max_band = stereo_top_band(right, sfbtab, n_sfb);
    if n_long > 0 {
        let m = max_band[0].max(max_band[1]).max(max_band[2]);
        max_band = [m; 3];
    }
    for i in 0..max_blocks {
        let default_pos = 3u8;
        let itop = n_sfb - max_blocks + i;
        let prev = itop as i32 - max_blocks as i32;
        ist_pos[itop] = if max_band[i] >= prev {
            default_pos
        } else {
            ist_pos[prev as usize]
        };
    }
    stereo_process(left, right, ist_pos, sfbtab, hdr, max_band);
}

/// Undo the short-block interleaving (three windows per band).
pub fn reorder(grbuf: &mut [f32], scratch: &mut [f32], sfb: &[u8]) {
    let mut src = 0usize;
    let mut dst = 0usize;
    let mut i = 0usize;
    loop {
        let len = usize::from(sfb[i]);
        if len == 0 {
            break;
        }
        for _ in 0..len {
            scratch[dst] = grbuf[src];
            scratch[dst + 1] = grbuf[src + len];
            scratch[dst + 2] = grbuf[src + 2 * len];
            dst += 3;
            src += 1;
        }
        src += 2 * len;
        i += 3;
    }
    grbuf[..dst].copy_from_slice(&scratch[..dst]);
}

/// Alias reduction across `nbands` sub-band boundaries.
pub fn antialias(grbuf: &mut [f32], nbands: usize) {
    let aa = &tables::AA;
    for b in 0..nbands {
        let base = b * 18;
        for i in 0..8 {
            let u = grbuf[base + 18 + i];
            let d = grbuf[base + 17 - i];
            grbuf[base + 18 + i] = u * aa[0][i] - d * aa[1][i];
            grbuf[base + 17 - i] = u * aa[1][i] + d * aa[0][i];
        }
    }
}

fn dct3_9(y: &mut [f32; 9]) {
    let (mut s0, s2, mut s4, s6, s8) = (y[0], y[2], y[4], y[6], y[8]);
    let mut t0 = s0 + s6 * 0.5;
    s0 -= s6;
    let t4 = (s4 + s2) * 0.93969262;
    let t2 = (s8 + s2) * 0.76604444;
    let s6b = (s4 - s8) * 0.17364818;
    s4 += s8 - s2;

    let s2b = s0 - s4 * 0.5;
    y[4] = s4 + s0;
    let s8b = t0 - t2 + s6b;
    let s0b = t0 - t4 + t2;
    let s4b = t0 + t4 - s6b;

    let s1 = y[1];
    let mut s3 = y[3];
    let s5 = y[5];
    let s7 = y[7];

    s3 *= 0.86602540;
    t0 = (s5 + s1) * 0.98480775;
    let t4 = (s5 - s7) * 0.34202014;
    let t2 = (s1 + s7) * 0.64278761;
    let s1b = (s1 - s5 - s7) * 0.86602540;

    let s5b = t0 - s3 - t2;
    let s7b = t4 - s3 - t0;
    let s3b = t4 + s3 - t2;

    y[0] = s4b - s7b;
    y[1] = s2b + s1b;
    y[2] = s0b - s3b;
    y[3] = s8b + s5b;
    y[5] = s8b - s5b;
    y[6] = s0b + s3b;
    y[7] = s2b - s1b;
    y[8] = s4b + s7b;
}

fn imdct36(grbuf: &mut [f32], overlap: &mut [f32], window: &[f32; 18]) {
    let twid = &tables::TWID9;
    let mut co = [0f32; 9];
    let mut si = [0f32; 9];
    co[0] = -grbuf[0];
    si[0] = grbuf[17];
    for i in 0..4 {
        si[8 - 2 * i] = grbuf[4 * i + 1] - grbuf[4 * i + 2];
        co[1 + 2 * i] = grbuf[4 * i + 1] + grbuf[4 * i + 2];
        si[7 - 2 * i] = grbuf[4 * i + 4] - grbuf[4 * i + 3];
        co[2 + 2 * i] = -(grbuf[4 * i + 3] + grbuf[4 * i + 4]);
    }
    dct3_9(&mut co);
    dct3_9(&mut si);
    for k in [1usize, 3, 5, 7] {
        si[k] = -si[k];
    }
    for i in 0..9 {
        let ovl = overlap[i];
        let sum = co[i] * twid[9 + i] + si[i] * twid[i];
        overlap[i] = co[i] * twid[i] - si[i] * twid[9 + i];
        grbuf[i] = ovl * window[i] - sum * window[9 + i];
        grbuf[17 - i] = ovl * window[9 + i] + sum * window[i];
    }
}

fn idct3(x0: f32, x1: f32, x2: f32, dst: &mut [f32]) {
    let m1 = x1 * 0.86602540;
    let a1 = x0 - x2 * 0.5;
    dst[1] = x0 + x2;
    dst[0] = a1 + m1;
    dst[2] = a1 - m1;
}

fn imdct12(x: &[f32], dst: &mut [f32], overlap: &mut [f32]) {
    let twid = &tables::TWID3;
    let mut co = [0f32; 3];
    let mut si = [0f32; 3];
    idct3(-x[0], x[6] + x[3], x[12] + x[9], &mut co);
    idct3(x[15], x[12] - x[9], x[6] - x[3], &mut si);
    si[1] = -si[1];
    for i in 0..3 {
        let ovl = overlap[i];
        let sum = co[i] * twid[3 + i] + si[i] * twid[i];
        overlap[i] = co[i] * twid[i] - si[i] * twid[3 + i];
        dst[i] = ovl * twid[2 - i] - sum * twid[5 - i];
        dst[5 - i] = ovl * twid[5 - i] + sum * twid[2 - i];
    }
}

fn imdct_short(grbuf: &mut [f32], overlap: &mut [f32], nbands: usize) {
    for b in 0..nbands {
        let g = b * 18;
        let o = b * 9;
        let mut tmp = [0f32; 18];
        tmp.copy_from_slice(&grbuf[g..g + 18]);
        grbuf[g..g + 6].copy_from_slice(&overlap[o..o + 6]);
        imdct12(&tmp, &mut grbuf[g + 6..g + 12], &mut overlap[o + 6..o + 9]);
        imdct12(&tmp[1..], &mut grbuf[g + 12..g + 18], &mut overlap[o + 6..o + 9]);
        {
            let (lo, hi) = overlap.split_at_mut(o + 6);
            imdct12(&tmp[2..], &mut lo[o..o + 6], &mut hi[..3]);
        }
    }
}

fn change_sign(grbuf: &mut [f32]) {
    let mut b = 0usize;
    let mut base = 18usize;
    while b < 32 {
        for i in (1..18).step_by(2) {
            grbuf[base + i] = -grbuf[base + i];
        }
        b += 2;
        base += 36;
    }
}

/// IMDCT for one channel granule, with overlap-add into `overlap`.
pub fn imdct_gr(grbuf: &mut [f32], overlap: &mut [f32; 288], block_type: u8, n_long_bands: usize) {
    let mut g = 0usize;
    let mut o = 0usize;
    for _ in 0..n_long_bands {
        imdct36(&mut grbuf[g..], &mut overlap[o..], &tables::MDCT_WINDOW[0]);
        g += 18;
        o += 9;
    }
    let rest = 32 - n_long_bands;
    if block_type == 2 {
        imdct_short(&mut grbuf[g..], &mut overlap[o..], rest);
    } else {
        // Only `stop` blocks (3) use the second MDCT window.
        let win = usize::from(block_type == 3);
        for _ in 0..rest {
            imdct36(&mut grbuf[g..], &mut overlap[o..], &tables::MDCT_WINDOW[win]);
            g += 18;
            o += 9;
        }
    }
}

/// Full post-processing for the channels of one granule.
#[allow(clippy::too_many_arguments)]
pub fn process_granule(
    grbuf: &mut [f32; 1152],
    overlap: &mut [[f32; 288]; 2],
    scratch: &mut [f32; 576],
    gr: &[GranuleInfo; 2],
    nch: usize,
    hdr: &Header,
    sfbtab: &[u8],
) {
    for ch in 0..nch {
        let g = &gr[ch];
        // A mixed block starts with two sub-bands (36 lines) of long windows;
        // 32 kHz only widens the scalefactor bands, not the line count.
        let n_long_subbands =
            (if g.mixed_block_flag { 2 } else { 0 }) << usize::from(hdr.sample_rate == 32000);
        let n_long_sfb = match g.band_kind {
            BandKind::Mixed => 8,
            _ => 0,
        };
        let (lo, hi) = grbuf.split_at_mut(576);
        let buf: &mut [f32] = if ch == 0 { lo } else { hi };
        if g.block_type == 2 {
            reorder(&mut buf[n_long_subbands * 18..], scratch, &sfbtab[n_long_sfb..]);
        }
        let aa_bands = if g.block_type == 2 {
            n_long_subbands.saturating_sub(1)
        } else {
            31
        };
        antialias(buf, aa_bands);
        imdct_gr(buf, &mut overlap[ch], g.block_type, n_long_subbands);
        change_sign(buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn granule(kind: BandKind) -> GranuleInfo {
        GranuleInfo {
            part_23_length: 0,
            big_values: 0,
            global_gain: 0,
            scalefac_compress: 0,
            block_type: if kind == BandKind::Long { 0 } else { 2 },
            mixed_block_flag: kind == BandKind::Mixed,
            table_select: [0; 3],
            region_count: [0; 3],
            subblock_gain: [0; 3],
            preflag: false,
            scalefac_scale: false,
            count1_table: false,
            scfsi: 0,
            band_kind: kind,
        }
    }

    #[test]
    fn reorder_interleaves_three_windows() {
        // One band of 4 lines: [w0 x4][w1 x4][w2 x4] -> [w0 w1 w2] x4
        let mut grbuf = [0f32; 24];
        for i in 0..4 {
            grbuf[i] = 1.0;
            grbuf[4 + i] = 2.0;
            grbuf[8 + i] = 3.0;
        }
        let mut scratch = [0f32; 24];
        // The band table repeats each width once per short window and ends
        // with a zero sentinel; the reorder walks it in steps of three.
        reorder(&mut grbuf[..12], &mut scratch, &[4, 4, 4, 0]);
        for i in 0..4 {
            assert_eq!(&grbuf[i * 3..i * 3 + 3], &[1.0, 2.0, 3.0]);
        }
    }

    #[test]
    fn antialias_is_lossless_on_a_single_band() {
        let mut grbuf = [0.5f32; 64];
        antialias(&mut grbuf, 1);
        // With only one boundary the coefficients still mix two lines, but the
        // energy of the pair is preserved (`cs^2 + ca^2 == 1`).
        let energy: f32 = grbuf.iter().map(|v| v * v).sum();
        assert!((energy - 16.0).abs() < 1e-3, "energy {energy}");
    }

    #[test]
    fn imdct_of_silence_stays_silent() {
        let mut grbuf = [0f32; 576];
        let mut overlap = [0f32; 288];
        imdct_gr(&mut grbuf, &mut overlap, 0, 0);
        assert!(grbuf.iter().all(|v| *v == 0.0));
        assert!(overlap.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn process_granule_handles_long_and_short_blocks() {
        let hdr = Header {
            mpeg1: true,
            layer: 3,
            bitrate_kbps: 192,
            sample_rate: 44100,
            padding: 0,
            mode: 0,
            mode_ext: 0,
            crc: false,
        };
        for kind in [BandKind::Long, BandKind::Short, BandKind::Mixed] {
            let mut grbuf = [0f32; 1152];
            let mut overlap = [[0f32; 288]; 2];
            let mut scratch = [0f32; 576];
            let gr = [granule(kind), granule(kind)];
            let sfbtab: &[u8] = match kind {
                BandKind::Long => &tables::SCF_LONG[5],
                BandKind::Short => &tables::SCF_SHORT[5],
                BandKind::Mixed => &tables::SCF_MIXED[5],
            };
            process_granule(&mut grbuf, &mut overlap, &mut scratch, &gr, 2, &hdr, sfbtab);
            assert!(grbuf.iter().all(|v| v.is_finite()));
        }
    }
}
