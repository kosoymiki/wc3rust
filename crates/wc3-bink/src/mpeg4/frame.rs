//! Decoded picture storage, per-macroblock side data and YUV to RGBA output.

/// Plane selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plane {
    Y = 0,
    U = 1,
    V = 2,
}

impl Plane {
    pub fn is_luma(self) -> bool {
        matches!(self, Plane::Y)
    }
}

/// One decoded frame plus the coding side data later frames need.
///
/// Side data is per 8x8 block for luma and per macroblock for chroma, which is
/// the granularity of MPEG-4's DC/AC prediction and motion-vector prediction.
pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub stride_y: usize,
    pub stride_c: usize,
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,

    pub mb_w: usize,
    pub mb_h: usize,

    /// Motion vectors on the 8x8 grid with a one-block guard ring.
    pub mv: Vec<[i16; 2]>,
    pub mv_stride: usize,

    /// Quantiser per macroblock.
    pub qscale: Vec<u8>,
    /// `not_coded` (skipped) flag per macroblock.
    pub skip: Vec<bool>,
    /// Packed macroblock type flags (see `mb_type` constants).
    pub mb_type: Vec<u16>,

    /// DC predictor values: luma on the 8x8 grid, chroma on the MB grid.
    pub dc_y: Vec<i16>,
    pub dc_u: Vec<i16>,
    pub dc_v: Vec<i16>,
    /// AC predictor rows/columns per 8x8 block (`[0..8]` left column,
    /// `[8..16]` top row).
    pub ac_y: Vec<[i16; 16]>,
    pub ac_u: Vec<[i16; 16]>,
    pub ac_v: Vec<[i16; 16]>,

    dc_stride_y: usize,
    dc_stride_c: usize,
}

// Macroblock type flags (subset that the frame-level logic inspects).
pub const MB_INTRA: u16 = 1 << 0;
pub const MB_ACPRED: u16 = 1 << 1;
pub const MB_SKIP: u16 = 1 << 2;
pub const MB_16X16: u16 = 1 << 3;
pub const MB_8X8: u16 = 1 << 4;
pub const MB_INTERLACED: u16 = 1 << 5;
pub const MB_DIRECT: u16 = 1 << 6;
pub const MB_BIDIR: u16 = 1 << 7;
pub const MB_FORWARD: u16 = 1 << 8;
pub const MB_BACKWARD: u16 = 1 << 9;

impl Picture {
    pub fn new(width: usize, height: usize) -> Self {
        let stride_y = (width + 15) & !15;
        let stride_c = ((width + 1) / 2 + 15) & !15;
        let mb_w = (width + 15) / 16;
        let mb_h = (height + 15) / 16;
        let mv_stride = mb_w * 2 + 2;
        let dc_stride_y = mb_w * 2 + 2;
        let dc_stride_c = mb_w + 2;
        Self {
            width,
            height,
            stride_y,
            stride_c,
            y: vec![0u8; stride_y * (height + 1)],
            u: vec![0u8; stride_c * ((height + 1) / 2 + 1)],
            v: vec![0u8; stride_c * ((height + 1) / 2 + 1)],
            mb_w,
            mb_h,
            mv: vec![[0, 0]; mv_stride * (mb_h * 2 + 2)],
            mv_stride,
            qscale: vec![0u8; mb_w * mb_h],
            skip: vec![false; mb_w * mb_h],
            mb_type: vec![0u16; mb_w * mb_h],
            dc_y: vec![1024i16; dc_stride_y * (mb_h * 2 + 2)],
            dc_u: vec![1024i16; dc_stride_c * (mb_h + 2)],
            dc_v: vec![1024i16; dc_stride_c * (mb_h + 2)],
            ac_y: vec![[0i16; 16]; dc_stride_y * (mb_h * 2 + 2)],
            ac_u: vec![[0i16; 16]; dc_stride_c * (mb_h + 2)],
            ac_v: vec![[0i16; 16]; dc_stride_c * (mb_h + 2)],
            dc_stride_y,
            dc_stride_c,
        }
    }

    /// Zero the picture and reset all predictors for a new frame.
    ///
    /// The reference frame for the first coded frame of a stream is defined to
    /// be black: the retail streams that begin with a P-VOP (UndeadEd, the
    /// trailers) rely on exactly that, and hardware decoders behave the same.
    pub fn reset(&mut self) {
        self.y.fill(0);
        self.u.fill(0);
        self.v.fill(0);
        self.mv.fill([0, 0]);
        self.qscale.fill(0);
        self.skip.fill(false);
        self.mb_type.fill(0);
        self.dc_y.fill(1024);
        self.dc_u.fill(1024);
        self.dc_v.fill(1024);
        self.ac_y.fill([0; 16]);
        self.ac_u.fill([0; 16]);
        self.ac_v.fill([0; 16]);
    }

    /// Macroblock kind map for diagnostics: `I` intra, `S` skipped,
    /// `R` inter with residual, `.` inter without residual.
    pub fn mb_kind_map(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.mb_w * self.mb_h);
        for i in 0..self.mb_w * self.mb_h {
            let flags = self.mb_type[i];
            out.push(if flags & MB_INTRA != 0 {
                b'I'
            } else if flags & MB_SKIP != 0 {
                b'S'
            } else {
                b'R'
            });
        }
        out
    }

    /// Reset the DC/AC predictors of one macroblock to their neutral values.
    ///
    /// Every non-intra macroblock (inter *and* skipped) does this, so a later
    /// intra block that borders it predicts from 1024 instead of from a stale
    /// intra coefficient. Without it, P-frames that mix intra and inter blocks
    /// decode with a badly wrong DC (measured: 38 dB -> exact on HumanEd).
    pub fn clean_intra_entries(&mut self, mb_x: usize, mb_y: usize) {
        for n in 0..6 {
            let (plane, bx8, by8) = if n < 4 {
                (
                    Plane::Y,
                    mb_x * 2 + (n & 1),
                    mb_y * 2 + (n >> 1),
                )
            } else {
                (if n == 4 { Plane::U } else { Plane::V }, mb_x * 2, mb_y * 2)
            };
            *self.dc_ptr(plane, bx8, by8) = 1024;
            *self.ac_mut(plane, bx8, by8) = [0; 16];
        }
    }

    #[inline]
    pub fn plane(&self, p: Plane) -> &[u8] {
        match p {
            Plane::Y => &self.y,
            Plane::U => &self.u,
            Plane::V => &self.v,
        }
    }

    #[inline]
    pub fn plane_mut(&mut self, p: Plane) -> &mut [u8] {
        match p {
            Plane::Y => &mut self.y,
            Plane::U => &mut self.u,
            Plane::V => &mut self.v,
        }
    }

    #[inline]
    pub fn stride(&self, p: Plane) -> usize {
        if p.is_luma() {
            self.stride_y
        } else {
            self.stride_c
        }
    }

    #[inline]
    pub fn mv_at(&self, bx8: usize, by8: usize) -> [i16; 2] {
        self.mv[(by8 + 1) * self.mv_stride + bx8 + 1]
    }

    #[inline]
    pub fn set_mv(&mut self, bx8: usize, by8: usize, mv: [i16; 2]) {
        let s = self.mv_stride;
        self.mv[(by8 + 1) * s + bx8 + 1] = mv;
    }

    #[inline]
    pub fn dc_ptr(&mut self, p: Plane, bx8: usize, by8: usize) -> &mut i16 {
        match p {
            Plane::Y => {
                let s = self.dc_stride_y;
                &mut self.dc_y[(by8 + 1) * s + bx8 + 1]
            }
            Plane::U => {
                let s = self.dc_stride_c;
                let (bx, by) = (bx8 / 2, by8 / 2);
                &mut self.dc_u[(by + 1) * s + bx + 1]
            }
            Plane::V => {
                let s = self.dc_stride_c;
                let (bx, by) = (bx8 / 2, by8 / 2);
                &mut self.dc_v[(by + 1) * s + bx + 1]
            }
        }
    }

    /// Read the DC predictors `(a, b, c)` = (left, top-left, top) around a block.
    pub fn dc_neighbours(&self, p: Plane, bx8: usize, by8: usize) -> (i16, i16, i16) {
        match p {
            Plane::Y => {
                let s = self.dc_stride_y;
                let i = (by8 + 1) * s + bx8 + 1;
                (self.dc_y[i - 1], self.dc_y[i - 1 - s], self.dc_y[i - s])
            }
            Plane::U | Plane::V => {
                let (bx, by) = (bx8 / 2, by8 / 2);
                let s = self.dc_stride_c;
                let i = (by + 1) * s + bx + 1;
                let arr = if p == Plane::U { &self.dc_u } else { &self.dc_v };
                (arr[i - 1], arr[i - 1 - s], arr[i - s])
            }
        }
    }

    #[inline]
    pub fn ac_mut(&mut self, p: Plane, bx8: usize, by8: usize) -> &mut [i16; 16] {
        match p {
            Plane::Y => {
                let s = self.dc_stride_y;
                &mut self.ac_y[(by8 + 1) * s + bx8 + 1]
            }
            Plane::U => {
                let s = self.dc_stride_c;
                let (bx, by) = (bx8 / 2, by8 / 2);
                &mut self.ac_u[(by + 1) * s + bx + 1]
            }
            Plane::V => {
                let s = self.dc_stride_c;
                let (bx, by) = (bx8 / 2, by8 / 2);
                &mut self.ac_v[(by + 1) * s + bx + 1]
            }
        }
    }

    #[inline]
    pub fn ac_ref(&self, p: Plane, bx8: usize, by8: usize) -> &[i16; 16] {
        match p {
            Plane::Y => {
                let s = self.dc_stride_y;
                &self.ac_y[(by8 + 1) * s + bx8 + 1]
            }
            Plane::U => {
                let s = self.dc_stride_c;
                let (bx, by) = (bx8 / 2, by8 / 2);
                &self.ac_u[(by + 1) * s + bx + 1]
            }
            Plane::V => {
                let s = self.dc_stride_c;
                let (bx, by) = (bx8 / 2, by8 / 2);
                &self.ac_v[(by + 1) * s + bx + 1]
            }
        }
    }

    /// Copy edge pixels outward so motion compensation near the border reads
    /// in-frame samples (the "emulated edge" behaviour every decoder uses).
    pub fn pad_edges(&mut self) {
        let w = self.width;
        let h = self.height;
        let sy = self.stride_y;
        for y in 0..h {
            let row = y * sy;
            let last = self.y[row + w - 1];
            for x in w..sy {
                self.y[row + x] = last;
            }
        }
        // One extra replicated row under the picture.
        let src = (h - 1) * sy;
        let dst = h * sy;
        let (head, tail) = self.y.split_at_mut(dst);
        tail[..sy].copy_from_slice(&head[src..src + sy]);

        let cw = (w + 1) / 2;
        let ch = (h + 1) / 2;
        let sc = self.stride_c;
        for plane in [&mut self.u, &mut self.v] {
            for y in 0..ch {
                let row = y * sc;
                let last = plane[row + cw - 1];
                for x in cw..sc {
                    plane[row + x] = last;
                }
            }
            let src = (ch - 1) * sc;
            let dst = ch * sc;
            let (head, tail) = plane.split_at_mut(dst);
            tail[..sc].copy_from_slice(&head[src..src + sc]);
        }
    }

    /// Convert to RGBA8 using BT.601 studio-swing coefficients.
    pub fn to_rgba(&self, out: &mut [u8]) {
        debug_assert_eq!(out.len(), self.width * self.height * 4);
        let t = bt601_table();
        for y in 0..self.height {
            let yrow = y * self.stride_y;
            let crow = (y / 2) * self.stride_c;
            let orow = y * self.width * 4;
            for x in 0..self.width {
                let yy = t.y[i32::from(self.y[yrow + x]) as usize];
                let uu = i32::from(self.u[crow + x / 2]);
                let vv = i32::from(self.v[crow + x / 2]);
                let o = orow + x * 4;
                out[o] = clamp_u8((yy + t.r[vv as usize]) >> 16);
                out[o + 1] = clamp_u8((yy + t.g_cb[uu as usize] + t.g_cr[vv as usize]) >> 16);
                out[o + 2] = clamp_u8((yy + t.b[uu as usize]) >> 16);
                out[o + 3] = 255;
            }
        }
    }
}

#[inline]
fn clamp_u8(v: i32) -> u8 {
    if v < 0 {
        0
    } else if v > 255 {
        255
    } else {
        v as u8
    }
}

/// Fixed-point BT.601 studio-swing tables (16.16 fixed point).
struct Bt601 {
    /// 1.164 * (Y - 16)
    y: [i32; 256],
    /// 1.596 * (Cr - 128)
    r: [i32; 256],
    /// -0.391 * (Cb - 128)
    g_cb: [i32; 256],
    /// -0.813 * (Cr - 128)
    g_cr: [i32; 256],
    /// 2.017 * (Cb - 128)
    b: [i32; 256],
}

fn bt601_table() -> &'static Bt601 {
    use std::sync::OnceLock;
    static T: OnceLock<Bt601> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = Bt601 {
            y: [0; 256],
            r: [0; 256],
            g_cb: [0; 256],
            g_cr: [0; 256],
            b: [0; 256],
        };
        for i in 0..256 {
            let v = i as f64;
            t.y[i] = (1.164 * (v - 16.0) * 65536.0).round() as i32;
            t.r[i] = (1.596 * (v - 128.0) * 65536.0).round() as i32;
            t.g_cb[i] = (-0.391 * (v - 128.0) * 65536.0).round() as i32;
            t.g_cr[i] = (-0.813 * (v - 128.0) * 65536.0).round() as i32;
            t.b[i] = (2.017 * (v - 128.0) * 65536.0).round() as i32;
        }
        t
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grey_round_trips_to_neutral() {
        let mut p = Picture::new(16, 16);
        p.y.fill(126);
        p.u.fill(128);
        p.v.fill(128);
        let mut rgba = vec![0u8; 16 * 16 * 4];
        p.to_rgba(&mut rgba);
        // 1.164*(126-16) = 128.04 -> 128
        assert_eq!(&rgba[0..4], &[128, 128, 128, 255]);
    }

    #[test]
    fn bt601_reference_triples() {
        // Values cross-checked against the standard equations:
        // R = 1.164(Y-16) + 1.596(V-128)
        // G = 1.164(Y-16) - 0.813(V-128) - 0.391(U-128)
        // B = 1.164(Y-16) + 2.017(U-128)
        let cases = [
            (16u8, 128u8, 128u8, [0u8, 0, 0]),
            (235, 128, 128, [254, 254, 254]),
            (81, 90, 240, [254, 0, 0]),
            (145, 54, 34, [0, 255, 0]),
            (41, 240, 110, [0, 0, 255]),
        ];
        for (y, u, v, want) in cases {
            let mut p = Picture::new(16, 16);
            p.y.fill(y);
            p.u.fill(u);
            p.v.fill(v);
            let mut rgba = vec![0u8; 16 * 16 * 4];
            p.to_rgba(&mut rgba);
            let got = [rgba[0], rgba[1], rgba[2]];
            for i in 0..3 {
                assert!(
                    (i32::from(got[i]) - i32::from(want[i])).abs() <= 2,
                    "y={y} u={u} v={v}: got {got:?} want {want:?}"
                );
            }
        }
    }

    #[test]
    fn dc_neighbours_start_at_1024() {
        let p = Picture::new(32, 32);
        assert_eq!(p.dc_neighbours(Plane::Y, 0, 0), (1024, 1024, 1024));
        let mut p = Picture::new(32, 32);
        *p.dc_ptr(Plane::Y, 0, 0) = 7;
        assert_eq!(p.dc_neighbours(Plane::Y, 1, 0).0, 7);
        assert_eq!(p.dc_neighbours(Plane::Y, 0, 1).2, 7);
    }

    #[test]
    fn edge_padding_replicates_border() {
        let mut p = Picture::new(10, 6);
        for y in 0..6 {
            for x in 0..10 {
                p.y[y * p.stride_y + x] = (x + y) as u8;
            }
        }
        p.pad_edges();
        for x in 10..p.stride_y {
            assert_eq!(p.y[x], 9);
        }
        assert_eq!(p.y[6 * p.stride_y], p.y[5 * p.stride_y]);
    }
}
