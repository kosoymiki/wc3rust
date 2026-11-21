//! MPEG-4 Part 2 video decoder (I/P/B-VOPs, 4:2:0, H.263 and matrix
//! quantisation, half-pel and quarter-pel motion compensation).
//!
//! Scope is the subset the Warcraft III cinematics need, implemented in full:
//! I- and P-VOPs with intra DC/AC prediction, 16x16 and 8x8 motion partitions,
//! skip macroblocks, and B-VOPs (direct / forward / backward / interpolated).
//! Features the shipped files never use and that are not implemented return a
//! named [`BinkError::Unsupported`] instead of producing wrong pixels:
//! data partitioning, reversible VLC, sprite/GMC, OBMC, non-rectangular shape,
//! reduced-resolution VOPs and scalable layers.

pub mod coeff;
pub mod frame;
pub mod motion;
pub mod vol;

use crate::bitreader::{BitError, BitReader};
use crate::error::BinkError;
use crate::mpeg4::coeff::{ac_predict, dc_predict, dc_reconstruct, CoeffTables};
use crate::mpeg4::frame::{Picture, Plane};
use crate::mpeg4::motion::{copy_block, decode_motion, mc_plane, motion_compensate, mv_predict};
use crate::mpeg4::vol::{parse_access_unit, PictureType, Vol, Vop};
use crate::tables::{self, ALT_HORIZ, ALT_VERT, ZIGZAG};
use crate::vlc::Vlc;

fn map_bit(e: BitError) -> BinkError {
    match e {
        BitError::Eof => BinkError::Corrupt("mpeg4 bitstream truncated"),
        BitError::InvalidCode => BinkError::Corrupt("mpeg4 invalid VLC code"),
    }
}

/// Per-frame syntax statistics (evidence for the format matrix in
/// `analysis/media/MOVIES.md`, and the fastest way to see which rare paths a
/// stream exercises).
#[derive(Debug, Default, Clone, Copy)]
pub struct DecoderStats {
    pub macroblocks: u32,
    pub p_intra: u32,
    pub four_mv: u32,
    pub dquant: u32,
    pub escapes1: u32,
    pub escapes2: u32,
    pub escapes3: u32,
}

/// Which buffer holds the frame produced by the last [`Mpeg4Decoder::decode`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum Display {
    Last,
    Cur,
    None,
}

/// B macroblock prediction mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BMode {
    Direct0,
    Direct,
    Forward,
    Backward,
    Interpolate,
}

/// Decoder state: reference pictures and per-frame side data.
pub struct Mpeg4Decoder {
    vol: Option<Vol>,
    cur: Picture,
    last: Picture,
    next: Picture,
    has_last: bool,
    has_next: bool,
    display: Display,

    tables: CoeffTables,
    intra_mcbpc: Vlc,
    inter_mcbpc: Vlc,
    cbpy: Vlc,
    mv_vlc: Vlc,
    mb_type_b: Vlc,

    blocks: [[i32; 64]; 6],
    coefs: [[i32; 64]; 6],
    coded: [bool; 6],
    /// DC prediction direction of the block currently being decoded.
    last_dc_dir: u8,
    /// Deferred DC predictor for blocks coded without the intra DC VLC.
    pending_dc: Option<(i32, i32)>,

    qscale: u8,
    vop: Option<Vop>,
    mb_x: usize,
    mb_y: usize,

    // B-frame direct-mode timing and last motion vectors.
    last_mv: [(i32, i32); 2],
    pp_time: i32,
    pb_time: i32,
    debug: bool,
    /// Macroblocks synthesised from the reference after a damaged block.
    concealed_mbs: u64,
    /// `(mb_x, mb_y, reason)` of the last concealed macroblock.
    last_error: Option<(usize, usize, String)>,
    stats: DecoderStats,
}

impl Default for Mpeg4Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Mpeg4Decoder {
    pub fn new() -> Self {
        Self {
            vol: None,
            cur: Picture::new(16, 16),
            last: Picture::new(16, 16),
            next: Picture::new(16, 16),
            has_last: false,
            has_next: false,
            display: Display::None,
            tables: CoeffTables::new(),
            intra_mcbpc: Vlc::new(&tables::INTRA_MCBPC),
            inter_mcbpc: Vlc::new(&tables::INTER_MCBPC),
            cbpy: Vlc::new(&tables::CBPY),
            mv_vlc: Vlc::new(&tables::MV_TAB),
            mb_type_b: Vlc::new(&tables::MB_TYPE_B),
            blocks: [[0; 64]; 6],
            coefs: [[0; 64]; 6],
            coded: [false; 6],
            last_dc_dir: 0,
            pending_dc: None,
            qscale: 1,
            vop: None,
            mb_x: 0,
            mb_y: 0,
            last_mv: [(0, 0); 2],
            pp_time: 1,
            pb_time: 1,
            debug: std::env::var_os("WC3_BINK_DEBUG").is_some(),
            concealed_mbs: 0,
            last_error: None,
            stats: DecoderStats::default(),
        }
    }

    pub fn vol(&self) -> Option<&Vol> {
        self.vol.as_ref()
    }

    /// Macroblocks concealed from the reference after a damaged block.
    pub fn concealed_macroblocks(&self) -> u64 {
        self.concealed_mbs
    }

    /// `(mb_x, mb_y, reason)` of the last concealed macroblock, if any.
    pub fn last_error(&self) -> Option<(usize, usize, &str)> {
        self.last_error.as_ref().map(|(x, y, r)| (*x, *y, r.as_str()))
    }

    /// Syntax statistics for the frame decoded last.
    pub fn stats(&self) -> DecoderStats {
        self.stats
    }

    /// VOP header of the frame decoded last.
    pub fn last_vop(&self) -> Option<&Vop> {
        self.vop.as_ref()
    }

    /// The frame produced by the last successful [`Self::decode`].
    pub fn picture(&self) -> &Picture {
        match self.display {
            Display::Last => &self.last,
            Display::Cur | Display::None => &self.cur,
        }
    }

    /// Decode one access unit (one AVI video chunk) and return its type.
    pub fn decode(&mut self, payload: &[u8]) -> Result<PictureType, BinkError> {
        let au = parse_access_unit(payload, self.vol.as_ref())?
            .ok_or(BinkError::Corrupt("no VOP in frame payload"))?;
        if let Some(vol) = au.vol {
            self.ensure_size(vol.width, vol.height)?;
            self.vol = Some(vol);
        }
        let vol = self.vol.clone().ok_or(BinkError::Corrupt("no VOL header"))?;
        let vop = au.vop;
        if !vop.coded {
            self.display = Display::None;
            return Ok(vop.coding_type);
        }

        let mut br = BitReader::at(payload, au.mb_bit_offset);
        self.qscale = vop.qscale;
        self.vop = Some(vop.clone());
        self.last_mv = [(0, 0); 2];
        self.display = Display::None;
        self.stats = DecoderStats {
            escapes1: self.tables.escapes1(),
            escapes2: self.tables.escapes2(),
            escapes3: self.tables.escapes3(),
            ..Default::default()
        };

        match vop.coding_type {
            PictureType::I | PictureType::P => {
                self.cur.reset();
                let inter = vop.coding_type != PictureType::I;
                self.decode_macroblock_rows(&mut br, &vol, &vop, inter, false)?;
                if std::env::var_os("WC3_BINK_BITS").is_some() {
                    eprintln!(
                        "bits used={} payload_bits={} mb_offset={}",
                        br.bit_pos(),
                        payload.len() * 8,
                        au.mb_bit_offset
                    );
                }
                if let (Ok(tx), Ok(ty)) = (
                    std::env::var("WC3_BINK_MBX"),
                    std::env::var("WC3_BINK_MBY"),
                ) {
                    if let (Ok(x), Ok(y)) = (tx.parse::<usize>(), ty.parse::<usize>()) {
                        let idx = y * 16 * self.cur.stride_y + x * 16;
                        eprintln!(
                            "END frame px at ({x},{y}) = {:?}",
                            &self.cur.y[idx..idx + 8]
                        );
                    }
                }
                self.cur.pad_edges();
                std::mem::swap(&mut self.cur, &mut self.last);
                self.has_last = true;
                self.has_next = false;
                self.display = Display::Last;
            }
            PictureType::B => {
                if !self.has_next {
                    return Err(BinkError::Unsupported(
                        "B-VOP before its future reference in stream order",
                    ));
                }
                self.cur.reset();
                self.decode_macroblock_rows(&mut br, &vol, &vop, true, true)?;
                self.cur.pad_edges();
                self.display = Display::Cur;
            }
            PictureType::S => return Err(BinkError::Unsupported("S-VOP")),
        }
        Ok(vop.coding_type)
    }

    fn ensure_size(&mut self, width: usize, height: usize) -> Result<(), BinkError> {
        // The VOL is untrusted stream data: bound the picture before it sizes
        // three planes plus the side-data grids (WC3-BINK-001/003).
        if width == 0 || height == 0 {
            return Err(BinkError::Corrupt("MPEG-4 picture has zero dimensions"));
        }
        if width
            .checked_mul(height)
            .is_none_or(|pixels| pixels > crate::MAX_MOVIE_PIXELS)
        {
            return Err(BinkError::Corrupt("MPEG-4 picture dimensions out of range"));
        }
        if self.cur.width == width && self.cur.height == height {
            return Ok(());
        }
        self.cur = Picture::new(width, height);
        self.last = Picture::new(width, height);
        self.next = Picture::new(width, height);
        self.has_last = false;
        self.has_next = false;
        Ok(())
    }

    fn decode_macroblock_rows(
        &mut self,
        br: &mut BitReader<'_>,
        vol: &Vol,
        vop: &Vop,
        inter_frame: bool,
        is_b: bool,
    ) -> Result<(), BinkError> {
        let mb_w = self.cur.mb_w;
        let mb_h = self.cur.mb_h;
        for mb_y in 0..mb_h {
            for mb_x in 0..mb_w {
                self.mb_x = mb_x;
                self.mb_y = mb_y;
                self.stats.macroblocks += 1;
                let r = if is_b {
                    self.decode_mb_b(br, vol, vop)
                } else if inter_frame {
                    self.decode_mb_p(br, vol, vop)
                } else {
                    self.decode_mb_i(br, vol, vop)
                };
                if let Err(e) = r {
                    if self.debug {
                        eprintln!(
                            "mb({mb_x},{mb_y}) bit={} q={} vop={:?}: {e}",
                            br.bit_pos(),
                            self.qscale,
                            vop.coding_type
                        );
                    }
                    // A damaged macroblock must not kill the whole movie: the
                    // reference decoder conceals the rest of the frame from the
                    // motion-compensated reference and reports it. Do the same
                    // and count the concealed macroblocks so callers can see
                    // exactly how much of a frame was invented.
                    let concealed = (mb_h - mb_y) * mb_w - mb_x;
                    self.concealed_mbs += concealed as u64;
                    self.last_error = Some((mb_x, mb_y, format!("{e}")));
                    for y in mb_y..mb_h {
                        let start_x = if y == mb_y { mb_x } else { 0 };
                        for x in start_x..mb_w {
                            self.mb_x = x;
                            self.mb_y = y;
                            self.set_mb_state(x, y, frame::MB_SKIP | frame::MB_16X16);
                            self.copy_mb_from_last(x, y);
                            self.cur.clean_intra_entries(x, y);
                        }
                    }
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    // ---------------------------------------------------------------- I-VOP

    fn decode_mb_i(
        &mut self,
        br: &mut BitReader<'_>,
        vol: &Vol,
        _vop: &Vop,
    ) -> Result<(), BinkError> {
        let start_bit = br.bit_pos();
        let mut cbpc;
        loop {
            cbpc = self.intra_mcbpc.decode(br).map_err(map_bit)?;
            if cbpc != 8 {
                break;
            }
        }
        let dquant = cbpc & 4 != 0;
        if let (Ok(tx), Ok(ty)) = (
            std::env::var("WC3_BINK_MBX"),
            std::env::var("WC3_BINK_MBY"),
        ) {
            if tx.parse::<usize>().ok() == Some(self.mb_x) && ty.parse::<usize>().ok() == Some(self.mb_y) {
                eprintln!("TRACE I mb({},{}) start={} cbpc={cbpc} dquant={dquant} q={}", self.mb_x, self.mb_y, start_bit, self.qscale);
            }
        }
        if std::env::var_os("WC3_BINK_MB").is_some() && self.mb_y <= 1 {
            eprintln!(
                "I mb({},{}) start={} cbpc={cbpc} dquant={dquant} q={}",
                self.mb_x, self.mb_y, start_bit, self.qscale
            );
        }
        self.decode_intra_mb(br, vol, cbpc, dquant)
    }

    fn decode_intra_mb(
        &mut self,
        br: &mut BitReader<'_>,
        vol: &Vol,
        cbpc: u16,
        dquant: bool,
    ) -> Result<(), BinkError> {
        let (mb_x, mb_y) = (self.mb_x, self.mb_y);
        let ac_pred_flag = br.read_bit().map_err(map_bit)? == 1;
        let cbpy = self.cbpy.decode(br).map_err(map_bit)? as u16;
        let cbp = (cbpc & 3) | (cbpy << 2);
        let threshold = self.vop.as_ref().map_or(99, |v| v.intra_dc_threshold);
        let use_intra_dc_vlc = self.qscale < threshold;
        if dquant {
            let delta = [-1i32, -2, 1, 2][(br.read_bits(2).map_err(map_bit)? & 3) as usize];
            self.qscale = (i32::from(self.qscale) + delta).clamp(1, 31) as u8;
        }
        if !vol.progressive {
            let _interlaced_dct = br.read_bit().map_err(map_bit)?;
        }
        if std::env::var_os("WC3_BINK_MB").is_some()
            && self.mb_y <= 2
            && (14..20).contains(&self.mb_x)
        {
            eprintln!(
                "  intra mb({},{}) acpred={ac_pred_flag} cbpy={cbpy:04b} cbp={cbp:06b} q={} dcvlc={use_intra_dc_vlc} bit={}",
                self.mb_x, self.mb_y, self.qscale, br.bit_pos()
            );
        }
        let mut flags = frame::MB_INTRA;
        if ac_pred_flag {
            flags |= frame::MB_ACPRED;
        }
        self.set_mb_state(mb_x, mb_y, flags);

        let qscale = self.qscale;
        let dc_scale_y = i32::from(tables::Y_DC_SCALE[qscale.min(31) as usize]);
        let dc_scale_c = i32::from(tables::C_DC_SCALE[qscale.min(31) as usize]);
        self.clear_blocks();

        for n in 0..6 {
            let plane = if n < 4 {
                Plane::Y
            } else if n == 4 {
                Plane::U
            } else {
                Plane::V
            };
            let (bx8, by8) = if n < 4 {
                (mb_x * 2 + (n & 1), mb_y * 2 + (n >> 1))
            } else {
                (mb_x * 2, mb_y * 2)
            };
            let coded = (cbp >> (5 - n)) & 1 == 1;
            let dc_scale = if n < 4 { dc_scale_y } else { dc_scale_c };

            let p = dc_predict(&self.cur, plane, bx8, by8, mb_x, mb_y, n);
            self.last_dc_dir = p.dir;
            let scan: &[u8; 64] = if ac_pred_flag {
                let swapped = std::env::var_os("WC3_BINK_SCAN_SWAP").is_some();
                if (p.dir == 0) != swapped {
                    &ALT_VERT
                } else {
                    &ALT_HORIZ
                }
            } else {
                &ZIGZAG
            };

            if use_intra_dc_vlc {
                let diff = self.tables.decode_dc_diff(br, n)?;
                let level = dc_reconstruct(&mut self.cur, plane, bx8, by8, p.pred, diff, dc_scale);
                if self.debug && mb_x < 2 && mb_y == 0 {
                    eprintln!(
                        "mb({mb_x},{mb_y}) blk{n} q={qscale} scale={dc_scale} pred={} dir={} diff={diff} level={level} cbp={cbp:b} coded={coded}",
                        p.pred, p.dir
                    );
                }
                self.blocks[n][0] = level;
                if coded {
                    if std::env::var_os("WC3_BINK_TRACE").is_some() {
                        eprintln!(
                            "-- MB({mb_x},{mb_y}) blk{n} cbp={cbp:06b} q={qscale} acpred={ac_pred_flag} dcvlc={use_intra_dc_vlc} bit={}",
                            br.bit_pos()
                        );
                    }
                    self.tables
                        .decode_tcoeff(br, true, qscale, vol.quant_type, scan, &mut self.blocks[n], true)
                        .map_err(|e| {
                            if self.debug {
                                eprintln!(
                                    "intra mb({mb_x},{mb_y}) n={n} cbp={cbp:06b} q={qscale} acpred={ac_pred_flag} dcvlc={use_intra_dc_vlc} bits={}",
                                    br.bit_pos()
                                );
                            }
                            e
                        })?;
                }
            } else if coded {
                // DC arrives as the first TCOEFF coefficient.
                self.tables
                    .decode_tcoeff(br, true, qscale, vol.quant_type, scan, &mut self.blocks[n], false)
                    .map_err(|e| {
                        if self.debug {
                            eprintln!(
                                "intra(dc-tcoeff) mb({mb_x},{mb_y}) n={n} cbp={cbp:06b} q={qscale} acpred={ac_pred_flag} bits={}",
                                br.bit_pos()
                            );
                        }
                        e
                    })?;
                let diff = self.blocks[n][0];
                let level = dc_reconstruct(&mut self.cur, plane, bx8, by8, p.pred, diff, dc_scale);
                self.blocks[n][0] = level;
            } else {
                let level = dc_reconstruct(&mut self.cur, plane, bx8, by8, p.pred, 0, dc_scale);
                self.blocks[n][0] = level;
            }

            if std::env::var_os("WC3_BINK_MB2").is_some() && mb_x == 16 && mb_y == 1 {
                eprintln!(
                    "  mb(16,1) blk{n} pred={} dir={} level={} scale={dc_scale}",
                    p.pred, p.dir, self.blocks[n][0]
                );
            }
            ac_predict(
                &mut self.cur,
                plane,
                bx8,
                by8,
                mb_x,
                mb_y,
                n,
                qscale,
                &mut self.blocks[n],
                p.dir,
                ac_pred_flag,
            );

            if let (Ok(tx), Ok(ty)) = (
                std::env::var("WC3_BINK_MBX"),
                std::env::var("WC3_BINK_MBY"),
            ) {
                if tx.parse::<usize>().ok() == Some(mb_x) && ty.parse::<usize>().ok() == Some(mb_y) {
                    let nz: Vec<(usize, i32)> = self.blocks[n]
                        .iter()
                        .enumerate()
                        .filter(|(_, v)| **v != 0)
                        .map(|(i, v)| (i, *v))
                        .collect();
                    eprintln!(
                        "  blk{n} coded={coded} acpred={ac_pred_flag} dir={} q={qscale} nz({})={:?}",
                        p.dir,
                        nz.len(),
                        &nz[..nz.len().min(8)]
                    );
                }
            }
            let tmp = self.blocks[n];
            coeff::dequant_intra(
                &tmp,
                &mut self.coefs[n],
                qscale,
                vol.quant_type,
                &vol.intra_matrix,
                dc_scale,
            );
        }

        self.store_intra_mb(mb_x, mb_y);
        if let (Ok(tx), Ok(ty)) = (
            std::env::var("WC3_BINK_MBX"),
            std::env::var("WC3_BINK_MBY"),
        ) {
            if tx.parse::<usize>().ok() == Some(mb_x) && ty.parse::<usize>().ok() == Some(mb_y) {
                let idx = mb_y * 16 * self.cur.stride_y + mb_x * 16;
                eprintln!(
                    "  stored mb({mb_x},{mb_y}) coef0={} scale_note dc={} px={} ",
                    self.coefs[0][0],
                    self.coefs[0][0],
                    self.cur.y[idx]
                );
                eprintln!("  pixels row: {:?}", &self.cur.y[idx..idx + 8]);
            }
        }
        Ok(())
    }

    fn store_intra_mb(&mut self, mb_x: usize, mb_y: usize) {
        if self.debug && mb_x == 0 && mb_y == 0 {
            for n in 0..6 {
                let nz = self.coefs[n].iter().filter(|v| **v != 0).count();
                eprintln!(
                    "MB(0,0) blk{n}: nz={nz} dc={} ac1={} ac2={} ac8={}",
                    self.coefs[n][0], self.coefs[n][1], self.coefs[n][2], self.coefs[n][8]
                );
            }
        }
        let stride = self.cur.stride_y;
        let cstride = self.cur.stride_c;
        for n in 0..6 {
            let f = self.coefs[n];
            if n < 4 {
                let dx = mb_x * 16 + (n & 1) * 8;
                let dy = mb_y * 16 + (n >> 1) * 8;
                let plane = self.cur.y.as_mut_slice();
                crate::idct::idct_put(&f, &mut plane[dy * stride + dx..], stride);
            } else {
                let dx = mb_x * 8;
                let dy = mb_y * 8;
                let plane = if n == 4 {
                    self.cur.u.as_mut_slice()
                } else {
                    self.cur.v.as_mut_slice()
                };
                crate::idct::idct_put(&f, &mut plane[dy * cstride + dx..], cstride);
            }
        }
    }

    // ---------------------------------------------------------------- P-VOP

    fn decode_mb_p(
        &mut self,
        br: &mut BitReader<'_>,
        vol: &Vol,
        vop: &Vop,
    ) -> Result<(), BinkError> {
        let (mb_x, mb_y) = (self.mb_x, self.mb_y);
        if std::env::var_os("WC3_BINK_MBDUMP").is_some() {
            eprintln!("mbdump P ({mb_x},{mb_y}) bit={}", br.bit_pos());
        }
        let skip = br.read_bit().map_err(map_bit)? == 1;
        if skip {
            // A not-coded macroblock carries no vector difference and no
            // residual: the vector is zero and the block is copied from the
            // reference picture (ISO/IEC 14496-2 7.6.3, ffmpeg's `skip mb`).
            self.set_mb_state(mb_x, mb_y, frame::MB_SKIP | frame::MB_16X16 | frame::MB_FORWARD);
            for b in 0..4 {
                let (bx8, by8) = (mb_x * 2 + (b & 1), mb_y * 2 + (b >> 1));
                self.cur.set_mv(bx8, by8, [0, 0]);
            }
            self.copy_mb_from_last(mb_x, mb_y);
            self.cur.clean_intra_entries(mb_x, mb_y);
            return Ok(());
        }

        let mut cbpc;
        loop {
            cbpc = self.inter_mcbpc.decode(br).map_err(map_bit)?;
            if cbpc != 20 {
                break;
            }
        }
        let dquant = cbpc & 8 != 0;
        if cbpc & 4 != 0 {
            self.stats.p_intra += 1;
            if std::env::var_os("WC3_BINK_MBDUMP").is_some() {
                eprintln!("mbdump PI ({mb_x},{mb_y}) bit={}", br.bit_pos());
            }
            if let (Ok(tx), Ok(ty)) = (
                std::env::var("WC3_BINK_MBX"),
                std::env::var("WC3_BINK_MBY"),
            ) {
                if tx.parse::<usize>().ok() == Some(mb_x) && ty.parse::<usize>().ok() == Some(mb_y) {
                    eprintln!(
                        "P-intra mb({mb_x},{mb_y}) cbpc={cbpc} dquant={dquant} q={} bit={}",
                        self.qscale,
                        br.bit_pos()
                    );
                }
            }
            return self.decode_intra_mb(br, vol, cbpc, dquant);
        }
        if dquant {
            self.stats.dquant += 1;
        }

        if let (Ok(tx), Ok(ty)) = (
            std::env::var("WC3_BINK_MBX"),
            std::env::var("WC3_BINK_MBY"),
        ) {
            if tx.parse::<usize>().ok() == Some(mb_x) && ty.parse::<usize>().ok() == Some(mb_y) {
                eprintln!(
                    "P mb({mb_x},{mb_y}) cbpc={cbpc} four_mv={} dquant={dquant} q={} bit={}",
                    cbpc & 16 != 0,
                    self.qscale,
                    br.bit_pos()
                );
            }
        }
        let cbpy = self.cbpy.decode(br).map_err(map_bit)? as u16 ^ 0x0F;
        let cbp = (cbpc & 3) | (cbpy << 2);
        if dquant {
            let delta = [-1i32, -2, 1, 2][(br.read_bits(2).map_err(map_bit)? & 3) as usize];
            self.qscale = (i32::from(self.qscale) + delta).clamp(1, 31) as u8;
        }
        if !vol.progressive && cbp != 0 {
            let _interlaced_dct = br.read_bit().map_err(map_bit)?;
        }

        let four_mv = cbpc & 16 != 0;
        if four_mv {
            self.stats.four_mv += 1;
        }
        let mut mvs = [(0i32, 0i32); 4];
        if four_mv {
            self.set_mb_state(mb_x, mb_y, frame::MB_8X8 | frame::MB_FORWARD);
            for b in 0..4 {
                let (bx8, by8) = (mb_x * 2 + (b & 1), mb_y * 2 + (b >> 1));
                let (px, py) = mv_predict(&self.cur, bx8 as i32, by8 as i32, b, mb_x, mb_y);
                let mx = decode_motion(br, &self.mv_vlc, px, vop.f_code)?;
                let my = decode_motion(br, &self.mv_vlc, py, vop.f_code)?;
                mvs[b] = (mx, my);
                self.cur.set_mv(bx8, by8, [mx as i16, my as i16]);
            }
        } else {
            self.set_mb_state(mb_x, mb_y, frame::MB_16X16 | frame::MB_FORWARD);
            let (px, py) = mv_predict(&self.cur, (mb_x * 2) as i32, (mb_y * 2) as i32, 0, mb_x, mb_y);
            let mx = decode_motion(br, &self.mv_vlc, px, vop.f_code)?;
            let my = decode_motion(br, &self.mv_vlc, py, vop.f_code)?;
            for b in 0..4 {
                mvs[b] = (mx, my);
                let (bx8, by8) = (mb_x * 2 + (b & 1), mb_y * 2 + (b >> 1));
                self.cur.set_mv(bx8, by8, [mx as i16, my as i16]);
            }
        }

        self.cur.clean_intra_entries(mb_x, mb_y);
        self.clear_blocks();
        for n in 0..6 {
            let coded = (cbp >> (5 - n)) & 1 == 1;
            self.coded[n] = coded;
            if !coded {
                continue;
            }
            self.tables
                .decode_tcoeff(br, false, self.qscale, vol.quant_type, &ZIGZAG, &mut self.blocks[n], false)
                .map_err(|e| {
                    if self.debug {
                        eprintln!(
                            "inter mb({mb_x},{mb_y}) n={n} cbp={cbp:06b} q={} bits={}",
                            self.qscale,
                            br.bit_pos()
                        );
                    }
                    e
                })?;
            let tmp = self.blocks[n];
            coeff::dequant_inter(
                &tmp,
                &mut self.coefs[n],
                self.qscale,
                vol.quant_type,
                &vol.inter_matrix,
            );
        }

        // Reconstruct: prediction first, then residual.
        for n in 0..6 {
            let (plane, dx, dy, mv) = if n < 4 {
                (
                    Plane::Y,
                    mb_x * 16 + (n & 1) * 8,
                    mb_y * 16 + (n >> 1) * 8,
                    mvs[n],
                )
            } else {
                (
                    if n == 4 { Plane::U } else { Plane::V },
                    mb_x * 8,
                    mb_y * 8,
                    mvs[0],
                )
            };
            if let (Ok(tx), Ok(ty)) = (
                std::env::var("WC3_BINK_MBX"),
                std::env::var("WC3_BINK_MBY"),
            ) {
                if tx.parse::<usize>().ok() == Some(mb_x) && ty.parse::<usize>().ok() == Some(mb_y) {
                    let stride = self.cur.stride(plane);
                    let before: u32 = (0..8)
                        .flat_map(|y| (0..8).map(move |x| (x, y)))
                        .map(|(x, y)| u32::from(self.cur.plane(plane)[(dy + y) * stride + dx + x]))
                        .sum();
                    let _ = before;
                    eprintln!(
                        "  blk{n} plane={plane:?} coded={} mv={mv:?} dc={} ac={:?}",
                        self.coded[n],
                        self.coefs[n][0],
                        &self.coefs[n][1..5]
                    );
                }
            }
            self.motion_into_cur(plane, true, dx, dy, mv);
            if let (Ok(tx), Ok(ty)) = (
                std::env::var("WC3_BINK_MBX"),
                std::env::var("WC3_BINK_MBY"),
            ) {
                if tx.parse::<usize>().ok() == Some(mb_x) && ty.parse::<usize>().ok() == Some(mb_y) {
                    let stride = self.cur.stride(plane);
                    let s: u32 = (0..8)
                        .flat_map(|y| (0..8).map(move |x| (x, y)))
                        .map(|(x, y)| u32::from(self.cur.plane(plane)[(dy + y) * stride + dx + x]))
                        .sum();
                    eprintln!("    after MC blk{n}: mean={:.1} mv={mv:?}", s as f64 / 64.0);
                }
            }
            if self.coded[n] {
                let f = self.coefs[n];
                match plane {
                    Plane::Y => {
                        let stride = self.cur.stride_y;
                        let p = self.cur.y.as_mut_slice();
                        crate::idct::idct_add(&f, &mut p[dy * stride + dx..], stride);
                    }
                    Plane::U => {
                        let stride = self.cur.stride_c;
                        let p = self.cur.u.as_mut_slice();
                        crate::idct::idct_add(&f, &mut p[dy * stride + dx..], stride);
                    }
                    Plane::V => {
                        let stride = self.cur.stride_c;
                        let p = self.cur.v.as_mut_slice();
                        crate::idct::idct_add(&f, &mut p[dy * stride + dx..], stride);
                    }
                }
            }
        }
        Ok(())
    }

    // ---------------------------------------------------------------- B-VOP

    fn decode_mb_b(
        &mut self,
        br: &mut BitReader<'_>,
        vol: &Vol,
        vop: &Vop,
    ) -> Result<(), BinkError> {
        let (mb_x, mb_y) = (self.mb_x, self.mb_y);
        let xy = mb_y * self.cur.mb_w + mb_x;
        if self.next.skip[xy] {
            self.set_mb_state(mb_x, mb_y, frame::MB_SKIP | frame::MB_16X16 | frame::MB_FORWARD);
            self.copy_mb_from_last(mb_x, mb_y);
            self.cur.clean_intra_entries(mb_x, mb_y);
            return Ok(());
        }

        let modb1 = br.read_bit().map_err(map_bit)? == 1;
        let (mode, cbp) = if modb1 {
            (BMode::Direct0, 0u16)
        } else {
            let modb2 = br.read_bit().map_err(map_bit)?;
            let t = self.mb_type_b.decode(br).map_err(map_bit)?;
            let mode = match t {
                0 => BMode::Direct,
                1 => BMode::Interpolate,
                2 => BMode::Backward,
                _ => BMode::Forward,
            };
            let cbp = if modb2 == 1 {
                0
            } else {
                br.read_bits(6).map_err(map_bit)? as u16
            };
            if !matches!(mode, BMode::Direct | BMode::Direct0) && cbp != 0 {
                if br.read_bit().map_err(map_bit)? == 1 {
                    let delta = if br.read_bit().map_err(map_bit)? == 1 { 2 } else { -2 };
                    self.qscale = (i32::from(self.qscale) + delta).clamp(1, 31) as u8;
                }
            }
            if !vol.progressive && cbp != 0 {
                let _interlaced_dct = br.read_bit().map_err(map_bit)?;
            }
            (mode, cbp)
        };

        let mut mv_f = (0i32, 0i32);
        let mut mv_b = (0i32, 0i32);
        match mode {
            BMode::Forward => {
                mv_f.0 = decode_motion(br, &self.mv_vlc, self.last_mv[0].0, vop.f_code)?;
                mv_f.1 = decode_motion(br, &self.mv_vlc, self.last_mv[0].1, vop.f_code)?;
            }
            BMode::Backward => {
                mv_b.0 = decode_motion(br, &self.mv_vlc, self.last_mv[1].0, vop.b_code)?;
                mv_b.1 = decode_motion(br, &self.mv_vlc, self.last_mv[1].1, vop.b_code)?;
            }
            BMode::Interpolate => {
                mv_f.0 = decode_motion(br, &self.mv_vlc, self.last_mv[0].0, vop.f_code)?;
                mv_f.1 = decode_motion(br, &self.mv_vlc, self.last_mv[0].1, vop.f_code)?;
                mv_b.0 = decode_motion(br, &self.mv_vlc, self.last_mv[1].0, vop.b_code)?;
                mv_b.1 = decode_motion(br, &self.mv_vlc, self.last_mv[1].1, vop.b_code)?;
            }
            BMode::Direct | BMode::Direct0 => {
                let (mx, my) = if matches!(mode, BMode::Direct0) {
                    (0, 0)
                } else {
                    (
                        decode_motion(br, &self.mv_vlc, 0, 1)?,
                        decode_motion(br, &self.mv_vlc, 0, 1)?,
                    )
                };
                let colocated = self.next.mv_at(mb_x * 2, mb_y * 2);
                let pp = self.pp_time.max(1);
                let pb = self.pb_time.max(1);
                mv_f = (
                    i32::from(colocated[0]) * pb / pp + mx,
                    i32::from(colocated[1]) * pb / pp + my,
                );
                mv_b = if mx != 0 || my != 0 {
                    (
                        mv_f.0 - i32::from(colocated[0]),
                        mv_f.1 - i32::from(colocated[1]),
                    )
                } else {
                    (
                        i32::from(colocated[0]) * (pb - pp) / pp,
                        i32::from(colocated[1]) * (pb - pp) / pp,
                    )
                };
            }
        }
        self.last_mv[0] = mv_f;
        self.last_mv[1] = mv_b;

        self.cur.clean_intra_entries(mb_x, mb_y);
        self.clear_blocks();
        for n in 0..6 {
            let coded = (cbp >> (5 - n)) & 1 == 1;
            self.coded[n] = coded;
            if !coded {
                continue;
            }
            self.tables.decode_tcoeff(
                br,
                false,
                self.qscale,
                vol.quant_type,
                &ZIGZAG,
                &mut self.blocks[n],
                false,
            )?;
            let tmp = self.blocks[n];
            coeff::dequant_inter(
                &tmp,
                &mut self.coefs[n],
                self.qscale,
                vol.quant_type,
                &vol.inter_matrix,
            );
        }

        for n in 0..6 {
            let (plane, dx, dy) = if n < 4 {
                (
                    Plane::Y,
                    mb_x * 16 + (n & 1) * 8,
                    mb_y * 16 + (n >> 1) * 8,
                )
            } else {
                (
                    if n == 4 { Plane::U } else { Plane::V },
                    mb_x * 8,
                    mb_y * 8,
                )
            };
            match mode {
                BMode::Forward => self.motion_into_cur(plane, true, dx, dy, mv_f),
                BMode::Backward => self.motion_into_cur(plane, false, dx, dy, mv_b),
                BMode::Interpolate | BMode::Direct | BMode::Direct0 => {
                    self.motion_into_cur(plane, true, dx, dy, mv_f);
                    self.average_next_into_cur(plane, dx, dy, mv_b);
                }
            }
            if self.coded[n] {
                let f = self.coefs[n];
                match plane {
                    Plane::Y => {
                        let stride = self.cur.stride_y;
                        let p = self.cur.y.as_mut_slice();
                        crate::idct::idct_add(&f, &mut p[dy * stride + dx..], stride);
                    }
                    Plane::U => {
                        let stride = self.cur.stride_c;
                        let p = self.cur.u.as_mut_slice();
                        crate::idct::idct_add(&f, &mut p[dy * stride + dx..], stride);
                    }
                    Plane::V => {
                        let stride = self.cur.stride_c;
                        let p = self.cur.v.as_mut_slice();
                        crate::idct::idct_add(&f, &mut p[dy * stride + dx..], stride);
                    }
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------- helpers

    fn clear_blocks(&mut self) {
        self.blocks = [[0i32; 64]; 6];
        self.coefs = [[0i32; 64]; 6];
        self.coded = [false; 6];
    }

    fn set_mb_state(&mut self, mb_x: usize, mb_y: usize, flags: u16) {
        let i = mb_y * self.cur.mb_w + mb_x;
        self.cur.mb_type[i] = flags;
        self.cur.skip[i] = flags & frame::MB_SKIP != 0;
        self.cur.qscale[i] = self.qscale;
    }

    fn copy_mb_from_last(&mut self, mb_x: usize, mb_y: usize) {
        let w = self.cur.width;
        let h = self.cur.height;
        for plane in [Plane::Y, Plane::U, Plane::V] {
            let (bw, bh) = if plane.is_luma() {
                (16usize, 16usize)
            } else {
                (8, 8)
            };
            let (dx, dy) = if plane.is_luma() {
                (mb_x * 16, mb_y * 16)
            } else {
                (mb_x * 8, mb_y * 8)
            };
            let pw = if plane.is_luma() { w } else { (w + 1) / 2 };
            let ph = if plane.is_luma() { h } else { (h + 1) / 2 };
            if dx >= pw || dy >= ph {
                continue;
            }
            let bw = bw.min(pw - dx);
            let bh = bh.min(ph - dy);
            let src_stride = self.last.stride(plane);
            let dst_stride = self.cur.stride(plane);
            let src = self.last.plane(plane);
            let dst = self.cur.plane_mut(plane);
            copy_block(dst, dst_stride, dx, dy, bw, bh, src, src_stride);
        }
    }

    /// Motion-compensate an 8x8 block from `last` (forward) or `next` into the
    /// current picture, using disjoint field borrows.
    fn motion_into_cur(
        &mut self,
        plane: Plane,
        forward: bool,
        dx: usize,
        dy: usize,
        mv: (i32, i32),
    ) {
        let qpel = self.vol.as_ref().is_some_and(|v| v.quarter_sample);
        let rounding = self.vop.as_ref().map_or(true, |v| v.rounding);
        // Chrominance vectors are the luminance vectors halved and rounded to
        // the nearest half chroma sample (ISO/IEC 14496-2 7.6.5): `(mv + 1) >> 1`.
        // Chrominance vectors: integer part `src >> 2` and a single half-pel
        // bit for any non-zero fraction, exactly as the reference decoder
        // derives them (`mpegvideo_motion.c`, FMT_H263 branch):
        //   uvsrc = src >> 1;  uvdxy = (v & 1) | ((v & 2) >> 1)
        let mv = if plane.is_luma() {
            mv
        } else {
            let chroma = |v: i32| ((v >> 2) << 1) | i32::from(v & 3 != 0);
            (chroma(mv.0), chroma(mv.1))
        };
        if forward {
            let src: &Picture = &self.last;
            let dst: &mut Picture = &mut self.cur;
            mc_plane(dst, src, plane, dx, dy, 8, 8, mv, qpel, rounding);
        } else {
            let src: &Picture = &self.next;
            let dst: &mut Picture = &mut self.cur;
            mc_plane(dst, src, plane, dx, dy, 8, 8, mv, qpel, rounding);
        }
    }

    fn average_next_into_cur(&mut self, plane: Plane, dx: usize, dy: usize, mv: (i32, i32)) {
        let qpel = self.vol.as_ref().is_some_and(|v| v.quarter_sample);
        let rounding = self.vop.as_ref().map_or(true, |v| v.rounding);
        // Chrominance vectors are the luminance vectors halved and rounded to
        // the nearest half chroma sample (ISO/IEC 14496-2 7.6.5): `(mv + 1) >> 1`.
        // Chrominance vectors: integer part `src >> 2` and a single half-pel
        // bit for any non-zero fraction, exactly as the reference decoder
        // derives them (`mpegvideo_motion.c`, FMT_H263 branch):
        //   uvsrc = src >> 1;  uvdxy = (v & 1) | ((v & 2) >> 1)
        let mv = if plane.is_luma() {
            mv
        } else {
            let chroma = |v: i32| ((v >> 2) << 1) | i32::from(v & 3 != 0);
            (chroma(mv.0), chroma(mv.1))
        };
        let mut tmp = [0u8; 64];
        {
            let src: &Picture = &self.next;
            let stride = src.stride(plane);
            let (sw, sh) = if plane.is_luma() {
                (src.width, src.height)
            } else {
                ((src.width + 1) / 2, (src.height + 1) / 2)
            };
            motion_compensate(
                &mut tmp,
                8,
                0,
                0,
                8,
                8,
                src.plane(plane),
                stride,
                sw,
                sh,
                mv.0,
                mv.1,
                qpel,
                rounding,
            );
        }
        let dst_stride = self.cur.stride(plane);
        let dst = self.cur.plane_mut(plane);
        let start = dy * dst_stride + dx;
        for y in 0..8 {
            for x in 0..8 {
                let a = i32::from(dst[start + y * dst_stride + x]);
                let b = i32::from(tmp[y * 8 + x]);
                dst[start + y * dst_stride + x] = ((a + b + 1) >> 1) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_starts_empty() {
        let dec = Mpeg4Decoder::new();
        assert!(dec.vol().is_none());
        assert_eq!(dec.picture().width, 16);
    }

    #[test]
    fn garbage_payload_is_an_error_not_a_panic() {
        let mut dec = Mpeg4Decoder::new();
        for payload in [&b""[..], &b"\x00\x00\x01\xb6"[..], &[0xFFu8; 64][..]] {
            assert!(dec.decode(payload).is_err());
        }
    }
}
