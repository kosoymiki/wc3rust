//! MPEG-1 Layer III scalefactors and Huffman coefficient decoding.
//!
//! The codebooks are ISO/IEC 11172-3 Table 3-B.7 (quantised values) and
//! 3-B.8/3-B.9 (count1), stored in [`super::tables`] as the flattened
//! leaf/offset tables of the donor implementation; the walk below is the
//! standard "peek 5 bits, follow negative leaves, flush the leaf length"
//! traversal over those tables.

use crate::error::BinkError;
use crate::mp3::tables;
use crate::mp3::{BandKind, Bits, GranuleInfo, Header};

/// `x^(4/3)` with the fractional interpolation of the requantiser.
pub fn pow43(x: i32) -> f32 {
    if x < 129 {
        return tables::POW43[(16 + x) as usize];
    }
    let mut x = x;
    let mut mult = 256.0f32;
    if x < 1024 {
        mult = 16.0;
        x <<= 3;
    }
    let sign = 2 * x & 64;
    let frac = ((x & 63) - sign) as f32 / ((x & !63) + sign) as f32;
    tables::POW43[16 + ((x + sign) >> 6) as usize]
        * (1.0 + frac * ((4.0 / 3.0) + frac * (2.0 / 9.0)))
        * mult
}

/// `y * 2^(exp_q2 / 4)` in the four-way split of the reference decoder.
fn ldexp_q2(mut y: f32, mut exp_q2: i32) -> f32 {
    if exp_q2 <= 0 {
        return y;
    }
    loop {
        let e = exp_q2.min(120);
        let scale = tables::EXPFRAC[(e & 3) as usize] * ((1u32 << 30) >> (e >> 2)) as f32;
        y *= scale;
        exp_q2 -= e;
        if exp_q2 <= 0 {
            return y;
        }
    }
}

/// Band table row for the sample rate (`sfBandIndex` in the standard).
fn band_row(hdr: &Header) -> usize {
    // MPEG-1 rows are 5..7 (44.1, 48, 32 kHz) in the donor table layout.
    match hdr.sample_rate {
        44100 => 5,
        48000 => 6,
        _ => 7,
    }
}

/// `(long, short)` band counts and the band-width table for one granule.
pub fn bands(hdr: &Header, gr: &GranuleInfo) -> (&'static [u8], usize, usize) {
    let row = band_row(hdr);
    match gr.band_kind {
        BandKind::Long => (&tables::SCF_LONG[row], 22, 0),
        BandKind::Short => (&tables::SCF_SHORT[row], 0, 39),
        BandKind::Mixed => (&tables::SCF_MIXED[row], 8, 30),
    }
}

fn read_scalefactors(
    scf: &mut [u8; 40],
    ist_pos: &mut [u8; 39],
    scf_size: &[u8; 4],
    scf_count: &[u8],
    br: &mut Bits<'_>,
    mut scfsi: u8,
) {
    let mut si = 0usize; // write cursor into scf / ist_pos
    let mut i = 0usize;
    while i < 4 {
        let cnt = scf_count[i] as usize;
        if cnt == 0 {
            break;
        }
        if scfsi & 8 != 0 {
            scf[si..si + cnt].copy_from_slice(&ist_pos[si..si + cnt]);
        } else {
            let bits = scf_size[i];
            if bits == 0 {
                scf[si..si + cnt].fill(0);
                ist_pos[si..si + cnt].fill(0);
            } else {
                for k in 0..cnt {
                    let s = br.get(u32::from(bits)) as u8;
                    ist_pos[si + k] = s;
                    scf[si + k] = s;
                }
            }
        }
        si += cnt;
        i += 1;
        scfsi <<= 1;
    }
    scf[si] = 0;
    scf[si + 1] = 0;
    scf[si + 2] = 0;
}

/// Decode the scalefactors of one granule/channel into `scf` (linear gains).
pub fn decode_scalefactors(
    hdr: &Header,
    gr: &GranuleInfo,
    br: &mut Bits<'_>,
    ist_pos: &mut [u8; 39],
    scf: &mut [f32; 40],
) -> Result<(), BinkError> {
    let (_, n_long, n_short) = bands(hdr, gr);
    let kind_idx = match gr.band_kind {
        BandKind::Long => 0,
        BandKind::Mixed => 1,
        BandKind::Short => 2,
    };
    let partition = &tables::SCF_PARTITIONS[kind_idx];
    let scf_shift = i32::from(gr.scalefac_scale) + 1;

    let part = tables::SCFC_DECODE[gr.scalefac_compress as usize & 15];
    let scf_size = [part >> 2, part >> 2, part & 3, part & 3];

    let mut iscf = [0u8; 40];
    read_scalefactors(&mut iscf, ist_pos, &scf_size, partition, br, gr.scfsi);

    if n_short > 0 {
        let sh = 3 - scf_shift;
        let mut i = 0usize;
        while i < n_short {
            for j in 0..3 {
                iscf[n_long + i + j] += gr.subblock_gain[j] << sh;
            }
            i += 3;
        }
    } else if gr.preflag {
        for i in 0..10 {
            iscf[11 + i] += tables::PREAMP[i];
        }
    }

    let gain_exp = i32::from(gr.global_gain) - 4 - 210 - if hdr.is_ms_stereo() { 2 } else { 0 };
    let max_scfi = 44; // (MAX_SCF + 3) & ~3 with MAX_SCF = 41
    let gain = ldexp_q2((1u32 << (max_scfi / 4)) as f32, max_scfi - gain_exp);
    for i in 0..(n_long + n_short) {
        scf[i] = ldexp_q2(gain, i32::from(iscf[i]) << scf_shift);
    }
    Ok(())
}

/// Decode the quantised coefficients of one granule/channel into `dst`.
///
/// `gr_limit` is the bit position where this granule's main data ends; the
/// count1 region stops there, exactly like the reference decoder.
pub fn decode_huffman(
    dst: &mut [f32],
    br: &mut Bits<'_>,
    gr: &GranuleInfo,
    scf: &[f32; 40],
    sfbtab: &[u8],
    gr_limit: usize,
) -> Result<(), BinkError> {
    let mut di = 0usize;
    let mut one = 0f32;
    let mut ireg = 0usize;
    let mut big_val_cnt = gr.big_values as i32;
    let mut sfb_i = 0usize;
    let mut scf_i = 0usize;

    while big_val_cnt > 0 && ireg < 3 {
        let tab_num = gr.table_select[ireg] as usize;
        let mut sfb_cnt = i32::from(gr.region_count[ireg]);
        ireg += 1;
        let base = tables::HUFF_TABINDEX[tab_num] as usize;
        let codebook = &tables::HUFF_TABS[base..];
        let linbits = u32::from(tables::LINBITS[tab_num]);
        loop {
            let np = i32::from(*sfbtab.get(sfb_i).unwrap_or(&0)) / 2;
            sfb_i += 1;
            let pairs = big_val_cnt.min(np);
            one = *scf.get(scf_i).unwrap_or(&0.0);
            scf_i += 1;
            for _ in 0..pairs {
                let mut w = 5u32;
                let mut leaf = i32::from(codebook[br.peek(w) as usize]);
                while leaf < 0 {
                    br.skip(w);
                    w = (leaf & 7) as u32;
                    let idx = br.peek(w) as i32 - (leaf >> 3);
                    leaf = i32::from(codebook[idx as usize]);
                }
                br.skip(((leaf >> 8) & 0xff) as u32);
                let mut l = leaf;
                for _ in 0..2 {
                    let mut lsb = (l & 0x0f) as u32;
                    if lsb == 15 && linbits > 0 {
                        lsb += br.peek(linbits);
                        br.skip(linbits);
                        let neg = br.peek(1) == 1;
                        let v = one * pow43(lsb as i32);
                        dst[di] = if neg { -v } else { v };
                        di += 1;
                        br.skip(1);
                    } else {
                        let neg = br.peek(1) == 1;
                        dst[di] = tables::POW43[16 + lsb as usize - 16 * usize::from(neg)] * one;
                        di += 1;
                        if lsb != 0 {
                            br.skip(1);
                        }
                    }
                    l >>= 4;
                }
            }
            big_val_cnt -= np;
            sfb_cnt -= 1;
            if big_val_cnt <= 0 || sfb_cnt < 0 {
                break;
            }
        }
    }

    // count1: quadruples of ±1 values in the last scalefactor bands.
    let mut np = 1 - big_val_cnt;
    'count1: loop {
        let codebook: &[u8] = if gr.count1_table {
            &tables::HUFF_TAB33
        } else {
            &tables::HUFF_TAB32
        };
        let mut leaf = i32::from(codebook[br.peek(4) as usize]);
        if leaf & 8 == 0 {
            let k = (leaf & 3) as u32;
            let idx = (leaf >> 3) + ((br.peek(4 + k) & ((1 << k) - 1)) as i32);
            leaf = i32::from(codebook[idx as usize]);
        }
        br.skip((leaf & 7) as u32);
        if br.pos > gr_limit || di + 4 > dst.len() {
            break;
        }
        for s in 0..4usize {
            if s == 0 || s == 2 {
                np -= 1;
                if np == 0 {
                    np = i32::from(*sfbtab.get(sfb_i).unwrap_or(&0)) / 2;
                    sfb_i += 1;
                    if np == 0 {
                        break 'count1;
                    }
                    one = *scf.get(scf_i).unwrap_or(&0.0);
                    scf_i += 1;
                }
            }
            if leaf & (128 >> s) != 0 {
                let neg = br.peek(1) == 1;
                dst[di + s] = if neg { -one } else { one };
                br.skip(1);
            }
        }
        di += 4;
    }
    br.pos = gr_limit;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pow43_matches_the_table_and_interpolates() {
        for x in 0..129 {
            assert_eq!(pow43(x), tables::POW43[(16 + x) as usize]);
        }
        // 129^(4/3) = 651.80..; the interpolated branch must be close.
        let v = pow43(129);
        assert!((v - 651.8).abs() < 1.0, "pow43(129) = {v}");
        // 8000^(4/3) = 160000 exactly; the interpolation must land close.
        let hi = pow43(8000);
        assert!((hi - 160_000.0).abs() < 200.0, "pow43(8000) = {hi}");
    }

    #[test]
    fn ldexp_q2_splits_large_exponents() {
        assert!((ldexp_q2(1.0, 0) - 1.0).abs() < 1e-6);
        assert!((ldexp_q2(1.0, 4) - 0.5).abs() < 1e-6);
        // 200/4 = 50 octaves down: 2^-50.
        let small = ldexp_q2(1.0, 200);
        assert!((small - 8.881_784e-16).abs() < 1e-20, "{small}");
        // Every four steps of the exponent is one octave.
        assert!((ldexp_q2(ldexp_q2(1.0, 120), 120) - ldexp_q2(1.0, 240)).abs() < 1e-30);
    }

    #[test]
    fn band_tables_have_the_iso_widths() {
        let hdr = Header {
            mpeg1: true,
            layer: 3,
            bitrate_kbps: 192,
            sample_rate: 44100,
            padding: 0,
            mode: 1,
            mode_ext: 2,
            crc: false,
        };
        let mut gr = GranuleInfo {
            part_23_length: 0,
            big_values: 0,
            global_gain: 0,
            scalefac_compress: 0,
            block_type: 0,
            mixed_block_flag: false,
            table_select: [0; 3],
            region_count: [0; 3],
            subblock_gain: [0; 3],
            preflag: false,
            scalefac_scale: false,
            count1_table: false,
            scfsi: 0,
            band_kind: BandKind::Long,
        };
        let (tab, n_long, n_short) = bands(&hdr, &gr);
        assert_eq!((n_long, n_short), (22, 0));
        let sum: u32 = tab.iter().take_while(|&&v| v != 0).map(|&v| u32::from(v)).sum();
        assert_eq!(sum, 576, "44.1 kHz long bands must cover 576 lines");
        gr.band_kind = BandKind::Short;
        let (tab, n_long, n_short) = bands(&hdr, &gr);
        assert_eq!((n_long, n_short), (0, 39));
        let sum: u32 = tab.iter().take(39).map(|&v| u32::from(v)).sum();
        assert_eq!(sum, 576, "44.1 kHz short bands must cover 576 lines");
    }
}
