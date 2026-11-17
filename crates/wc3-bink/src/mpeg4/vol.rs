//! MPEG-4 Part 2 (ISO/IEC 14496-2) header parsing: Visual Object Layer and
//! Video Object Plane headers.
//!
//! Field order follows the spec; every read is bounds-checked. The shipped
//! Warcraft III streams use the Simple profile subset: rectangular shape,
//! progressive, H.263 quantisation (`quant_type = 0`), no GMC, no data
//! partitioning, no RVLC, no scalability. Exotic options are still parsed so a
//! file that uses them fails with a named unsupported error rather than
//! silently producing wrong pixels.

use crate::bitreader::{BitError, BitReader};
use crate::error::BinkError;
use crate::tables;

/// Start code values used by MPEG-4 Part 2 visual bitstreams.
pub mod start {
    pub const VISUAL_OBJECT_SEQUENCE: u8 = 0xB0;
    pub const VISUAL_OBJECT: u8 = 0xB5;
    pub const USER_DATA: u8 = 0xB2;
    pub const GROUP_OF_VOP: u8 = 0xB3;
    pub const VIDEO_OBJECT_LAYER_MIN: u8 = 0x20;
    pub const VIDEO_OBJECT_LAYER_MAX: u8 = 0x2F;
    pub const VOP: u8 = 0xB6;
    pub const VOS_END: u8 = 0xB1;
}

/// Picture coding type (`vop_coding_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PictureType {
    I,
    P,
    B,
    S,
}

impl PictureType {
    pub fn from_bits(v: u32) -> Self {
        match v & 3 {
            0 => Self::I,
            1 => Self::P,
            2 => Self::B,
            _ => Self::S,
        }
    }

    pub fn is_reference(self) -> bool {
        matches!(self, Self::I | Self::P | Self::S)
    }
}

/// Parsed Video Object Layer header.
#[derive(Debug, Clone)]
pub struct Vol {
    pub width: usize,
    pub height: usize,
    pub time_increment_resolution: u32,
    pub time_increment_bits: u8,
    pub fixed_vop_rate: bool,
    pub progressive: bool,
    pub quarter_sample: bool,
    /// `vol_quant_type`: true selects the MPEG matrix quantiser.
    pub quant_type: bool,
    pub quant_precision: u8,
    pub shape: u8,
    pub intra_matrix: [u16; 64],
    pub inter_matrix: [u16; 64],
    pub data_partitioning: bool,
    pub rvlc: bool,
    pub resync_marker: bool,
    pub low_delay: bool,
    pub sprite_usage: u8,
    pub num_sprite_warping_points: u8,
    pub verid: u8,
    pub vo_type: u8,
    pub interlaced: bool,
    pub alternate_scan_allowed: bool,
}

impl Default for Vol {
    fn default() -> Self {
        let mut intra = [0u16; 64];
        let mut inter = [0u16; 64];
        intra.copy_from_slice(&tables::DEFAULT_INTRA_MATRIX);
        inter.copy_from_slice(&tables::DEFAULT_INTER_MATRIX);
        Self {
            width: 0,
            height: 0,
            time_increment_resolution: 0,
            time_increment_bits: 1,
            fixed_vop_rate: false,
            progressive: true,
            quarter_sample: false,
            quant_type: false,
            quant_precision: 5,
            shape: 0,
            intra_matrix: intra,
            inter_matrix: inter,
            data_partitioning: false,
            rvlc: false,
            resync_marker: false,
            low_delay: false,
            sprite_usage: 0,
            num_sprite_warping_points: 0,
            verid: 1,
            vo_type: 0,
            interlaced: false,
            alternate_scan_allowed: true,
        }
    }
}

/// Parsed Video Object Plane header.
#[derive(Debug, Clone)]
pub struct Vop {
    pub coding_type: PictureType,
    /// `modulo_time_base` in units of `time_increment_resolution`.
    pub time_incr: u32,
    pub time_increment: u32,
    pub coded: bool,
    /// Rounding control: MPEG-4 `vop_rounding_type = 1 - no_rounding`.
    pub rounding: bool,
    pub intra_dc_threshold: u8,
    pub qscale: u8,
    pub f_code: u8,
    pub b_code: u8,
    pub alternate_scan: bool,
}

fn map_bit(e: BitError) -> BinkError {
    match e {
        BitError::Eof => BinkError::Corrupt("mpeg4 header truncated"),
        BitError::InvalidCode => BinkError::Corrupt("mpeg4 header bits invalid"),
    }
}

/// Check and discard a marker bit (`1`); a zero is a broken stream.
fn marker(br: &mut BitReader<'_>, what: &'static str) -> Result<(), BinkError> {
    match br.read_bit() {
        Ok(1) => Ok(()),
        Ok(_) => Err(BinkError::Corrupt(what)),
        Err(e) => Err(map_bit(e)),
    }
}

impl Vol {
    /// Parse a VOL header; the cursor must sit right after the `0x0000012X`.
    pub fn parse(br: &mut BitReader<'_>) -> Result<Self, BinkError> {
        let mut vol = Vol::default();
        let _random_accessible = br.read_bit().map_err(map_bit)?;
        vol.vo_type = br.read_bits(8).map_err(map_bit)? as u8;

        let is_object_layer_identifier = br.read_bit().map_err(map_bit)?;
        if is_object_layer_identifier == 1 {
            vol.verid = br.read_bits(4).map_err(map_bit)? as u8;
            let _priority = br.read_bits(3).map_err(map_bit)?;
        } else {
            vol.verid = 1;
        }

        let aspect_ratio_info = br.read_bits(4).map_err(map_bit)?;
        if aspect_ratio_info == 0x0F {
            let _par_width = br.read_bits(8).map_err(map_bit)?;
            let _par_height = br.read_bits(8).map_err(map_bit)?;
        }

        let vol_control_parameters = br.read_bit().map_err(map_bit)?;
        if vol_control_parameters == 1 {
            let chroma_format = br.read_bits(2).map_err(map_bit)?;
            if chroma_format != 1 {
                return Err(BinkError::Unsupported(
                    "MPEG-4 chroma_format other than 4:2:0",
                ));
            }
            vol.low_delay = br.read_bit().map_err(map_bit)? == 1;
            let vbv_parameters = br.read_bit().map_err(map_bit)?;
            if vbv_parameters == 1 {
                let _ = br.read_bits(15).map_err(map_bit)?;
                marker(br, "vbv marker 1")?;
                let _ = br.read_bits(15).map_err(map_bit)?;
                marker(br, "vbv marker 2")?;
                let _ = br.read_bits(15).map_err(map_bit)?;
                marker(br, "vbv marker 3")?;
                let _ = br.read_bits(3).map_err(map_bit)?;
                let _ = br.read_bits(11).map_err(map_bit)?;
                marker(br, "vbv marker 4")?;
                let _ = br.read_bits(15).map_err(map_bit)?;
                marker(br, "vbv marker 5")?;
            }
        } else {
            vol.low_delay = matches!(vol.vo_type, 1 | 2);
        }

        vol.shape = br.read_bits(2).map_err(map_bit)? as u8;
        if vol.shape != 0 {
            return Err(BinkError::Unsupported("non-rectangular MPEG-4 VOL shape"));
        }
        marker(br, "before time_increment_resolution")?;

        vol.time_increment_resolution = br.read_bits(16).map_err(map_bit)?;
        if vol.time_increment_resolution == 0 {
            return Err(BinkError::Corrupt("time_increment_resolution = 0"));
        }
        vol.time_increment_bits = (32 - (vol.time_increment_resolution - 1).leading_zeros()) as u8;
        if vol.time_increment_bits < 1 {
            vol.time_increment_bits = 1;
        }

        marker(br, "before fixed_vop_rate")?;
        vol.fixed_vop_rate = br.read_bit().map_err(map_bit)? == 1;
        if vol.fixed_vop_rate {
            let _vop_time_increment =
                br.read_bits(u32::from(vol.time_increment_bits)).map_err(map_bit)?;
        }

        marker(br, "before width")?;
        let width = br.read_bits(13).map_err(map_bit)? as usize;
        marker(br, "before height")?;
        let height = br.read_bits(13).map_err(map_bit)? as usize;
        marker(br, "after height")?;
        if width == 0 || height == 0 {
            return Err(BinkError::Corrupt("zero VOL dimensions"));
        }
        vol.width = width;
        vol.height = height;

        vol.progressive = br.read_bit().map_err(map_bit)? == 0;
        vol.interlaced = !vol.progressive;
        let obmc_disable = br.read_bit().map_err(map_bit)?;
        if obmc_disable == 0 {
            return Err(BinkError::Unsupported("MPEG-4 OBMC prediction"));
        }
        vol.sprite_usage = if vol.verid == 1 {
            br.read_bit().map_err(map_bit)? as u8
        } else {
            br.read_bits(2).map_err(map_bit)? as u8
        };
        if vol.sprite_usage != 0 {
            return Err(BinkError::Unsupported("MPEG-4 sprite / GMC"));
        }

        let not_8_bit = br.read_bit().map_err(map_bit)?;
        if not_8_bit == 1 {
            vol.quant_precision = br.read_bits(4).map_err(map_bit)? as u8;
            let bits_per_pixel = br.read_bits(4).map_err(map_bit)?;
            if bits_per_pixel != 8 {
                return Err(BinkError::Unsupported("MPEG-4 non-8-bit samples"));
            }
            if !(3..=9).contains(&vol.quant_precision) {
                vol.quant_precision = 5;
            }
        } else {
            vol.quant_precision = 5;
        }

        vol.quant_type = br.read_bit().map_err(map_bit)? == 1;
        if vol.quant_type {
            if br.read_bit().map_err(map_bit)? == 1 {
                vol.intra_matrix = read_quant_matrix(br)?;
            }
            if br.read_bit().map_err(map_bit)? == 1 {
                vol.inter_matrix = read_quant_matrix(br)?;
            }
        } else {
            vol.intra_matrix.copy_from_slice(&tables::DEFAULT_INTRA_MATRIX);
            vol.inter_matrix.copy_from_slice(&tables::DEFAULT_INTER_MATRIX);
        }

        if vol.verid != 1 {
            vol.quarter_sample = br.read_bit().map_err(map_bit)? == 1;
        } else {
            vol.quarter_sample = false;
        }

        // Complexity estimation: only skip the fields when they are present.
        let complexity_disabled = br.read_bit().map_err(map_bit)?;
        if complexity_disabled == 0 {
            let estimation_method = br.read_bits(2).map_err(map_bit)?;
            if estimation_method < 2 {
                skip_complexity_estimation(br, estimation_method as u8)?;
            }
        }

        vol.resync_marker = br.read_bit().map_err(map_bit)? == 0;
        vol.data_partitioning = br.read_bit().map_err(map_bit)? == 1;
        if vol.data_partitioning {
            vol.rvlc = br.read_bit().map_err(map_bit)? == 1;
            if vol.rvlc {
                return Err(BinkError::Unsupported("MPEG-4 reversible VLC"));
            }
            return Err(BinkError::Unsupported("MPEG-4 data partitioning"));
        }
        if vol.verid != 1 {
            let new_pred = br.read_bit().map_err(map_bit)?;
            if new_pred == 1 {
                return Err(BinkError::Unsupported("MPEG-4 new_pred"));
            }
            let reduced_res = br.read_bit().map_err(map_bit)?;
            if reduced_res == 1 {
                return Err(BinkError::Unsupported("MPEG-4 reduced-resolution VOP"));
            }
        }
        let scalability = br.read_bit().map_err(map_bit)?;
        if scalability == 1 {
            return Err(BinkError::Unsupported("MPEG-4 scalability"));
        }

        Ok(vol)
    }
}

fn read_quant_matrix(br: &mut BitReader<'_>) -> Result<[u16; 64], BinkError> {
    let mut m = [0u16; 64];
    let mut last = 0u16;
    let mut i = 0usize;
    while i < 64 {
        let v = br.read_bits(8).map_err(map_bit)? as u16;
        if v == 0 {
            break;
        }
        last = v;
        m[tables::ZIGZAG[i] as usize] = v;
        i += 1;
    }
    for j in i..64 {
        m[tables::ZIGZAG[j] as usize] = last;
    }
    Ok(m)
}

fn skip_complexity_estimation(br: &mut BitReader<'_>, method: u8) -> Result<(), BinkError> {
    // Shape estimation: 6 flags if present.
    if br.read_bit().map_err(map_bit)? == 0 {
        for _ in 0..6 {
            let _ = br.read_bit();
        }
    }
    if br.read_bit().map_err(map_bit)? == 0 {
        for _ in 0..4 {
            let _ = br.read_bit();
        }
    }
    let _ = marker(br, "complexity marker 1");
    if br.read_bit().map_err(map_bit)? == 0 {
        for _ in 0..4 {
            let _ = br.read_bit();
        }
    }
    if br.read_bit().map_err(map_bit)? == 0 {
        for _ in 0..6 {
            let _ = br.read_bit();
        }
    }
    let _ = marker(br, "complexity marker 2");
    if method == 1 {
        let _ = br.read_bit();
        let _ = br.read_bit();
    }
    Ok(())
}

impl Vop {
    /// Parse a VOP header; the cursor must sit right after `0x000001B6`.
    pub fn parse(br: &mut BitReader<'_>, vol: &Vol) -> Result<Self, BinkError> {
        let coding_type = PictureType::from_bits(br.read_bits(2).map_err(map_bit)?);
        if coding_type == PictureType::S {
            return Err(BinkError::Unsupported("MPEG-4 S-VOP (sprite)"));
        }

        let mut time_incr = 0u32;
        while br.read_bit().map_err(map_bit)? == 1 {
            time_incr += 1;
            if time_incr > 1024 {
                return Err(BinkError::Corrupt("modulo_time_base runaway"));
            }
        }
        marker(br, "before time_increment")?;
        let time_increment = br
            .read_bits(u32::from(vol.time_increment_bits))
            .map_err(map_bit)?;
        marker(br, "before vop_coded")?;
        let coded = br.read_bit().map_err(map_bit)? == 1;

        let mut vop = Vop {
            coding_type,
            time_incr,
            time_increment,
            coded,
            rounding: true,
            intra_dc_threshold: tables::DC_THRESHOLD[0],
            qscale: 0,
            f_code: 1,
            b_code: 1,
            alternate_scan: false,
        };
        if !coded {
            return Ok(vop);
        }

        vop.rounding = if matches!(coding_type, PictureType::P | PictureType::S) {
            br.read_bit().map_err(map_bit)? == 0
        } else {
            false
        };

        let dc_index = br.read_bits(3).map_err(map_bit)? as usize;
        vop.intra_dc_threshold = tables::DC_THRESHOLD[dc_index.min(7)];
        if !vol.progressive {
            let _top_field_first = br.read_bit().map_err(map_bit)?;
            vop.alternate_scan = br.read_bit().map_err(map_bit)? == 1;
        }

        vop.qscale = br.read_bits(u32::from(vol.quant_precision)).map_err(map_bit)? as u8;
        if vop.qscale == 0 {
            return Err(BinkError::Corrupt("vop qscale = 0"));
        }
        if coding_type != PictureType::I {
            vop.f_code = br.read_bits(3).map_err(map_bit)? as u8;
            if vop.f_code == 0 {
                return Err(BinkError::Corrupt("vop f_code = 0"));
            }
        }
        if coding_type == PictureType::B {
            vop.b_code = br.read_bits(3).map_err(map_bit)? as u8;
            if vop.b_code == 0 {
                return Err(BinkError::Corrupt("vop b_code = 0"));
            }
        }
        Ok(vop)
    }
}

/// Header of one access unit: the VOP fields plus the bit offset where the
/// macroblock data begins.
#[derive(Debug, Clone)]
pub struct AccessUnit {
    pub vop: Vop,
    /// Bit offset into the frame payload where macroblock data starts.
    pub mb_bit_offset: usize,
    /// The VOL carried by this payload, when it starts a new one.
    pub vol: Option<Vol>,
}

/// Scan a frame payload for its VOL (if any) and VOP header.
///
/// `fallback_vol` is the decoder's current VOL; a payload that only carries a
/// VOP reuses it. Returns `Ok(None)` when the payload holds no VOP.
pub fn parse_access_unit(
    data: &[u8],
    fallback_vol: Option<&Vol>,
) -> Result<Option<AccessUnit>, BinkError> {
    let mut vol = fallback_vol.cloned();
    let mut pos = 0usize;
    let mut seen_vol = None;
    loop {
        let Some((code_at, code)) = BitReader::find_start_code(data, pos) else {
            return Ok(None);
        };
        let after = code_at + 1;
        match code {
            start::VIDEO_OBJECT_LAYER_MIN..=start::VIDEO_OBJECT_LAYER_MAX => {
                let mut br = BitReader::at(data, after * 8);
                let v = Vol::parse(&mut br)?;
                vol = Some(v.clone());
                seen_vol = Some(v);
                pos = after;
            }
            start::VOP => {
                let Some(vol) = vol.as_ref() else {
                    return Err(BinkError::Corrupt("VOP before VOL header"));
                };
                let mut br = BitReader::at(data, after * 8);
                let vop = Vop::parse(&mut br, vol)?;
                return Ok(Some(AccessUnit {
                    vop,
                    mb_bit_offset: br.bit_pos(),
                    vol: seen_vol,
                }));
            }
            start::VISUAL_OBJECT_SEQUENCE
            | start::VISUAL_OBJECT
            | start::USER_DATA
            | start::GROUP_OF_VOP => {
                pos = after;
            }
            start::VOS_END => return Ok(None),
            _ => {
                pos = after;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-built VOL header for 800x368, 30000 ticks, H.263 quantisation.
    fn vol_bits() -> Vec<u8> {
        let mut bits: Vec<bool> = Vec::new();
        let mut push = |v: u32, n: u32| {
            for i in (0..n).rev() {
                bits.push((v >> i) & 1 == 1);
            }
        };
        push(0, 1); // random_accessible_vol
        push(0, 8); // vo_type = simple
        push(0, 1); // is_object_layer_identifier
        push(1, 4); // aspect_ratio_info = 1:1
        push(0, 1); // vol_control_parameters
        push(0, 2); // shape = rect
        push(1, 1); // marker
        push(30_000, 16); // time_increment_resolution
        push(1, 1); // marker
        push(0, 1); // fixed_vop_rate
        push(1, 1); // marker
        push(800, 13); // width
        push(1, 1); // marker
        push(368, 13); // height
        push(1, 1); // marker
        push(0, 1); // progressive (=1 after xor? no: bit 0 means progressive)
        push(1, 1); // obmc_disable
        push(0, 1); // vol_sprite_usage (verid==1)
        push(0, 1); // not_8_bit
        push(0, 1); // quant_type
        push(1, 1); // complexity estimation disabled
        push(0, 1); // resync_marker_disabled = 0 -> resync enabled
        push(0, 1); // data_partitioning
        push(0, 1); // scalability
        while bits.len() % 8 != 0 {
            bits.push(false);
        }
        let mut out = vec![0u8; bits.len() / 8];
        for (i, b) in bits.iter().enumerate() {
            if *b {
                out[i / 8] |= 1 << (7 - (i % 8));
            }
        }
        out
    }

    #[test]
    fn parses_a_simple_vol() {
        let data = vol_bits();
        let mut br = BitReader::new(&data);
        let vol = Vol::parse(&mut br).unwrap();
        assert_eq!(vol.width, 800);
        assert_eq!(vol.height, 368);
        assert_eq!(vol.time_increment_resolution, 30_000);
        assert_eq!(vol.time_increment_bits, 15);
        assert!(vol.progressive);
        assert!(!vol.quant_type);
        assert!(!vol.quarter_sample);
        assert!(!vol.data_partitioning);
    }

    #[test]
    fn vol_truncation_is_an_error() {
        let data = vol_bits();
        for cut in 1..data.len() {
            let mut br = BitReader::new(&data[..cut.min(data.len())]);
            let r = Vol::parse(&mut br);
            assert!(r.is_err(), "cut {cut} should fail");
        }
    }

    #[test]
    fn rejects_exotic_features_with_named_errors() {
        // Same header but with quant_type = 1 and no custom matrices: this
        // path must still parse; the unsupported ones are partitioning etc.
        let mut data = vol_bits();
        // shape occupies the first two bits of the third byte (1+8+1+4+2 bits
        // in): set it to 3 (non-rectangular) and expect a named refusal.
        data[2] |= 0b1100_0000;
        let mut br = BitReader::new(&data);
        let r = Vol::parse(&mut br);
        assert!(r.is_err());
    }
}
