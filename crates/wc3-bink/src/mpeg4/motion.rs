//! Motion vector decoding, spatial prediction and motion compensation.

use crate::bitreader::{BitError, BitReader};
use crate::error::BinkError;
use crate::mpeg4::frame::{Picture, Plane};
use crate::vlc::Vlc;

fn map_bit(e: BitError) -> BinkError {
    match e {
        BitError::Eof => BinkError::Corrupt("mpeg4 motion vector truncated"),
        BitError::InvalidCode => BinkError::Corrupt("mpeg4 invalid motion VLC"),
    }
}

/// Decode one motion vector component (H.263 / MPEG-4 `motion_code`),
/// returning the predictor-corrected value in half-pel units.
pub fn decode_motion(
    br: &mut BitReader<'_>,
    mvvlc: &Vlc,
    pred: i32,
    f_code: u8,
) -> Result<i32, BinkError> {
    let code = mvvlc.decode(br).map_err(map_bit)?;
    if code == 0 {
        return Ok(pred);
    }
    let sign = br.read_bit().map_err(map_bit)?;
    let shift = u32::from(f_code).saturating_sub(1);
    let mut val = i32::from(code);
    if shift > 0 {
        val = (val - 1) << shift;
        val |= br.read_bits(shift).map_err(map_bit)? as i32;
        val += 1;
    }
    if sign == 1 {
        val = -val;
    }
    val += pred;
    // MPEG-4 uses short vectors: wrap into the 5 + f_code bit range.
    let bits = 5 + u32::from(f_code);
    let range = 1i32 << bits;
    let half = range >> 1;
    if val >= half {
        val -= range;
    } else if val < -half {
        val += range;
    }
    Ok(val)
}

#[inline]
fn mid_pred(a: i32, b: i32, c: i32) -> i32 {
    let mut v = [a, b, c];
    v.sort_unstable();
    v[1]
}

/// Spatial motion-vector predictor for block `block` (0..3) of a macroblock.
///
/// `bx8`/`by8` are the block coordinates inside the frame's 8x8 grid.
pub fn mv_predict(
    pic: &Picture,
    bx8: i32,
    by8: i32,
    block: usize,
    mb_x: usize,
    mb_y: usize,
) -> (i32, i32) {
    let at = |x: i32, y: i32| -> (i32, i32) {
        if x < 0 || y < 0 {
            return (0, 0);
        }
        let mv = pic.mv_at(x as usize, y as usize);
        (i32::from(mv[0]), i32::from(mv[1]))
    };
    // Neighbour offsets per block, matching the H.263/MPEG-4 layout:
    //   block 0: TL, 1: TR, 2: BL, 3: BR
    let off: i32 = match block {
        0 => 2,
        1 => 1,
        2 => 1,
        _ => -1,
    };
    let a = at(bx8 - 1, by8);
    let b = at(bx8, by8 - 1);
    let c = at(bx8 + off, by8 - 1);

    if mb_y == 0 && block < 3 {
        match block {
            0 => {
                if mb_x == 0 {
                    (0, 0)
                } else {
                    a
                }
            }
            1 => a,
            _ => {
                let a = if mb_x == 0 { (0, 0) } else { a };
                (mid_pred(a.0, b.0, c.0), mid_pred(a.1, b.1, c.1))
            }
        }
    } else {
        (mid_pred(a.0, b.0, c.0), mid_pred(a.1, b.1, c.1))
    }
}

/// Sample a plane with edge clamping.
#[inline]
fn sample(plane: &[u8], stride: usize, w: usize, h: usize, x: i32, y: i32) -> i32 {
    let x = x.clamp(0, w as i32 - 1) as usize;
    let y = y.clamp(0, h as i32 - 1) as usize;
    i32::from(plane[y * stride + x])
}

/// Motion-compensate an `bw` x `bh` block from `src` into `dst`.
///
/// `mv_x`/`mv_y` are in half-pel units (or quarter-pel when `qpel`). `rounding`
/// is the VOP's `rounding_type` (1 = use the +1 rounding of the spec).
#[allow(clippy::too_many_arguments)]
pub fn motion_compensate(
    dst: &mut [u8],
    dst_stride: usize,
    dx: usize,
    dy: usize,
    bw: usize,
    bh: usize,
    src: &[u8],
    src_stride: usize,
    src_w: usize,
    src_h: usize,
    mv_x: i32,
    mv_y: i32,
    qpel: bool,
    rounding: bool,
) {
    let r = i32::from(rounding);
    let (fx, fy, base_x, base_y) = if qpel {
        let fx = mv_x & 3;
        let fy = mv_y & 3;
        (fx, fy, (dx as i32) + (mv_x >> 2), (dy as i32) + (mv_y >> 2))
    } else {
        let fx = mv_x & 1;
        let fy = mv_y & 1;
        (fx, fy, (dx as i32) + (mv_x >> 1), (dy as i32) + (mv_y >> 1))
    };

    for y in 0..bh {
        for x in 0..bw {
            let sx = base_x + x as i32;
            let sy = base_y + y as i32;
            let v = if fx == 0 && fy == 0 {
                if qpel {
                    sample(src, src_stride, src_w, src_h, sx, sy)
                } else {
                    sample(src, src_stride, src_w, src_h, sx, sy)
                }
            } else if qpel {
                // Bilinear quarter-pel interpolation.
                let a = sample(src, src_stride, src_w, src_h, sx, sy);
                let b = sample(src, src_stride, src_w, src_h, sx + 1, sy);
                let c = sample(src, src_stride, src_w, src_h, sx, sy + 1);
                let d = sample(src, src_stride, src_w, src_h, sx + 1, sy + 1);
                let w00 = (4 - fx) * (4 - fy);
                let w10 = fx * (4 - fy);
                let w01 = (4 - fx) * fy;
                let w11 = fx * fy;
                (w00 * a + w10 * b + w01 * c + w11 * d + 8) >> 4
            } else if fy == 0 {
                let a = sample(src, src_stride, src_w, src_h, sx, sy);
                let b = sample(src, src_stride, src_w, src_h, sx + 1, sy);
                (a + b + r) >> 1
            } else if fx == 0 {
                let a = sample(src, src_stride, src_w, src_h, sx, sy);
                let c = sample(src, src_stride, src_w, src_h, sx, sy + 1);
                (a + c + r) >> 1
            } else {
                let a = sample(src, src_stride, src_w, src_h, sx, sy);
                let b = sample(src, src_stride, src_w, src_h, sx + 1, sy);
                let c = sample(src, src_stride, src_w, src_h, sx, sy + 1);
                let d = sample(src, src_stride, src_w, src_h, sx + 1, sy + 1);
                // Four-tap bilinear: the reference rounds with +2 when the VOP
                // sets rounding_type and with +1 otherwise (never truncation).
                let bias = 1 + r;
                (a + b + c + d + bias) >> 2
            };
            dst[(dy + y) * dst_stride + dx + x] = v.clamp(0, 255) as u8;
        }
    }
}

/// Average two predictions (bidirectional B blocks).
pub fn average_block(
    dst: &mut [u8],
    dst_stride: usize,
    other: &[u8],
    other_stride: usize,
    dx: usize,
    dy: usize,
    bw: usize,
    bh: usize,
) {
    for y in 0..bh {
        for x in 0..bw {
            let a = i32::from(dst[(dy + y) * dst_stride + dx + x]);
            let b = i32::from(other[(dy + y) * other_stride + dx + x]);
            dst[(dy + y) * dst_stride + dx + x] = ((a + b + 1) >> 1) as u8;
        }
    }
}

/// Copy a reference plane block unchanged (skip macroblock / zero motion).
///
/// The source is read at the same `(dx, dy)` as the destination: the reference
/// frame is a full picture, not a block buffer.
#[allow(clippy::too_many_arguments)]
pub fn copy_block(
    dst: &mut [u8],
    dst_stride: usize,
    dx: usize,
    dy: usize,
    bw: usize,
    bh: usize,
    src: &[u8],
    src_stride: usize,
) {
    for y in 0..bh {
        let from = (dy + y) * src_stride + dx;
        let to = (dy + y) * dst_stride + dx;
        dst[to..to + bw].copy_from_slice(&src[from..from + bw]);
    }
}

/// Convenience: motion-compensate one plane of `dst` from `src`.
#[allow(clippy::too_many_arguments)]
pub fn mc_plane(
    dst: &mut Picture,
    src: &Picture,
    plane: Plane,
    dx: usize,
    dy: usize,
    bw: usize,
    bh: usize,
    mv: (i32, i32),
    qpel: bool,
    rounding: bool,
) {
    let stride = dst.stride(plane);
    let src_stride = src.stride(plane);
    let (src_w, src_h) = if plane.is_luma() {
        (src.width, src.height)
    } else {
        ((src.width + 1) / 2, (src.height + 1) / 2)
    };
    let dst_plane = dst.plane_mut(plane);
    let src_plane = src.plane(plane);
    motion_compensate(
        dst_plane,
        stride,
        dx,
        dy,
        bw,
        bh,
        src_plane,
        src_stride,
        src_w,
        src_h,
        mv.0,
        mv.1,
        qpel,
        rounding,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables;

    #[test]
    fn motion_zero_copies_pixels() {
        let mut dst = vec![0u8; 16 * 8];
        let mut src = vec![0u8; 16 * 8];
        for i in 0..src.len() {
            src[i] = (i * 7 % 251) as u8;
        }
        motion_compensate(&mut dst, 16, 0, 0, 8, 8, &src, 16, 16, 8, 0, 0, false, true);
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(dst[y * 16 + x], src[y * 16 + x]);
            }
        }
    }

    #[test]
    fn half_pel_interpolates_with_rounding() {
        // 2x1 source: 10, 20 -> half-pel between them = (10+20+1)>>1 = 15
        let src = [10u8, 20u8, 0, 0];
        let mut dst = [0u8; 4];
        motion_compensate(&mut dst, 2, 0, 0, 1, 1, &src, 2, 2, 2, 1, 0, false, true);
        assert_eq!(dst[0], 15);
        let mut dst = [0u8; 4];
        motion_compensate(&mut dst, 2, 0, 0, 1, 1, &src, 2, 2, 2, 1, 0, false, false);
        assert_eq!(dst[0], 15);
    }

    #[test]
    fn half_pel_rounding_flag_changes_result() {
        let src = [10u8, 21u8, 0, 0];
        let mut dst = [0u8; 4];
        motion_compensate(&mut dst, 2, 0, 0, 1, 1, &src, 2, 2, 2, 1, 0, false, true);
        assert_eq!(dst[0], (10 + 21 + 1) >> 1);
        let mut dst = [0u8; 4];
        motion_compensate(&mut dst, 2, 0, 0, 1, 1, &src, 2, 2, 2, 1, 0, false, false);
        assert_eq!(dst[0], (10 + 21) >> 1);
    }

    #[test]
    fn motion_is_clamped_at_frame_edges() {
        let src = [5u8, 6, 7, 8];
        let mut dst = [0u8; 4];
        motion_compensate(&mut dst, 2, 0, 0, 2, 2, &src, 2, 2, 2, -40, -40, false, true);
        assert!(dst.iter().all(|&v| v == 5));
    }

    #[test]
    fn mv_predict_uses_median_of_neighbours() {
        let mut pic = Picture::new(32, 32);
        pic.set_mv(0, 0, [1, 2]);
        pic.set_mv(1, 1, [3, 4]);
        pic.set_mv(2, 1, [5, 6]);
        // Block 3 of MB (0,0) sits at (1,1); A=(0,1), B=(1,0), C=(0,0).
        pic.set_mv(0, 1, [10, 10]);
        pic.set_mv(1, 0, [0, 0]);
        pic.set_mv(0, 0, [1, 2]);
        let (px, py) = mv_predict(&pic, 1, 1, 3, 0, 0);
        assert_eq!((px, py), (1, 2));
    }

    #[test]
    fn decode_motion_zero_code_returns_predictor() {
        let vlc = Vlc::new(&tables::MV_TAB);
        // MV_TAB index 0 is a 1-bit '1' code.
        let br_data = [0b1000_0000u8];
        let mut br = BitReader::new(&br_data);
        let mv = decode_motion(&mut br, &vlc, 7, 1).unwrap();
        assert_eq!(mv, 7);
    }

    #[test]
    fn decode_motion_applies_sign_and_predictor() {
        let vlc = Vlc::new(&tables::MV_TAB);
        // index 1 = code 1 (2 bits '01'), magnitude 1, sign bit 1 -> -1
        let br_data = [0b0110_0000u8];
        let mut br = BitReader::new(&br_data);
        let mv = decode_motion(&mut br, &vlc, 4, 1).unwrap();
        assert_eq!(mv, 3);
    }

    #[test]
    fn fcode_extends_magnitude() {
        let vlc = Vlc::new(&tables::MV_TAB);
        // index 1 (code '01'), sign 0, f_code=2 -> val = ((1-1)<<1 | 0) + 1 = 1
        let br_data = [0b0100_0000u8];
        let mut br = BitReader::new(&br_data);
        let mv = decode_motion(&mut br, &vlc, 0, 2).unwrap();
        assert_eq!(mv, 1);
    }
}
