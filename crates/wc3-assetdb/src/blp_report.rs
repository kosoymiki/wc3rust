//! BLP validation: header probe plus a decode report with colour/alpha stats.
//!
//! `wc3_formats::blp::decode` returns pixels or an error; this module records
//! *why* a file is good or bad: declared header fields, mip chain consistency,
//! decoded size, palette usage, alpha coverage, colour balance and hashes. It is
//! the data source for the texture/icon/biome catalogs, so every number in the
//! report is computed from the decoded pixels, never assumed.

use std::collections::HashSet;
use std::fmt;

use sha2::{Digest, Sha256};

use wc3_formats::blp::decode as decode_blp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlpHeader {
    pub magic: [u8; 4],
    /// BLP1 content type (0 = JPEG, 1 = paletted) or BLP2 compression.
    pub compression: u32,
    pub alpha_bits: u8,
    pub alpha_type: u8,
    pub width: u32,
    pub height: u32,
    pub mip_count: u8,
    pub offsets: [u32; 16],
    pub sizes: [u32; 16],
    pub jpeg_header_size: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderError {
    TooShort(usize),
    BadMagic([u8; 4]),
    BadDimensions(u32, u32),
    TableTruncated,
}

impl fmt::Display for HeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort(len) => write!(f, "header too short ({len} bytes)"),
            Self::BadMagic(magic) => write!(f, "bad magic {magic:?}"),
            Self::BadDimensions(w, h) => write!(f, "bad dimensions {w}x{h}"),
            Self::TableTruncated => f.write_str("offset/size table truncated"),
        }
    }
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|s| u32::from_le_bytes(s.try_into().expect("4 bytes")))
}

/// Parse the BLP header without decoding pixels.
pub fn probe(bytes: &[u8]) -> Result<BlpHeader, HeaderError> {
    if bytes.len() < 4 {
        return Err(HeaderError::TooShort(bytes.len()));
    }
    let magic: [u8; 4] = bytes[..4].try_into().expect("4 bytes");
    match &magic {
        b"BLP1" => {
            if bytes.len() < 156 {
                return Err(HeaderError::TooShort(bytes.len()));
            }
            let compression = u32_at(bytes, 4).ok_or(HeaderError::TableTruncated)?;
            let alpha_bits = u32_at(bytes, 8).ok_or(HeaderError::TableTruncated)? as u8;
            let width = u32_at(bytes, 12).ok_or(HeaderError::TableTruncated)?;
            let height = u32_at(bytes, 16).ok_or(HeaderError::TableTruncated)?;
            if width == 0 || height == 0 || width > 32768 || height > 32768 {
                return Err(HeaderError::BadDimensions(width, height));
            }
            let mut offsets = [0u32; 16];
            let mut sizes = [0u32; 16];
            for i in 0..16 {
                offsets[i] = u32_at(bytes, 28 + i * 4).ok_or(HeaderError::TableTruncated)?;
                sizes[i] = u32_at(bytes, 92 + i * 4).ok_or(HeaderError::TableTruncated)?;
            }
            let mip_count = offsets.iter().filter(|o| **o != 0).count() as u8;
            let jpeg_header_size = if compression == 0 {
                u32_at(bytes, 156)
            } else {
                None
            };
            Ok(BlpHeader {
                magic,
                compression,
                alpha_bits,
                alpha_type: u8::from(alpha_bits > 0),
                width,
                height,
                mip_count,
                offsets,
                sizes,
                jpeg_header_size,
            })
        }
        b"BLP2" => {
            if bytes.len() < 148 {
                return Err(HeaderError::TooShort(bytes.len()));
            }
            let compression = bytes[8] as u32;
            let alpha_bits = bytes[9];
            let alpha_type = bytes[10];
            let width = u32_at(bytes, 12).ok_or(HeaderError::TableTruncated)?;
            let height = u32_at(bytes, 16).ok_or(HeaderError::TableTruncated)?;
            if width == 0 || height == 0 || width > 8192 || height > 8192 {
                return Err(HeaderError::BadDimensions(width, height));
            }
            let mut offsets = [0u32; 16];
            let mut sizes = [0u32; 16];
            for i in 0..16 {
                offsets[i] = u32_at(bytes, 20 + i * 4).ok_or(HeaderError::TableTruncated)?;
                sizes[i] = u32_at(bytes, 84 + i * 4).ok_or(HeaderError::TableTruncated)?;
            }
            let mip_count = offsets.iter().filter(|o| **o != 0).count() as u8;
            Ok(BlpHeader {
                magic,
                compression,
                alpha_bits,
                alpha_type,
                width,
                height,
                mip_count,
                offsets,
                sizes,
                jpeg_header_size: None,
            })
        }
        other => Err(HeaderError::BadMagic(*other)),
    }
}

/// Coarse hue bucket, used by the biome catalog to show that a tile decodes to
/// a plausible colour family (grass green, dirt brown, snow white, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HueClass {
    Gray,
    Red,
    Brown,
    Orange,
    Yellow,
    Green,
    Cyan,
    Blue,
    Purple,
    Magenta,
}

impl HueClass {
    pub fn name(self) -> &'static str {
        match self {
            Self::Gray => "gray",
            Self::Red => "red",
            Self::Brown => "brown",
            Self::Orange => "orange",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Cyan => "cyan",
            Self::Blue => "blue",
            Self::Purple => "purple",
            Self::Magenta => "magenta",
        }
    }

    pub fn classify(mean: [u8; 3]) -> Self {
        let [r, g, b] = mean.map(|v| v as f32);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let chroma = max - min;
        if chroma < 18.0 {
            return Self::Gray;
        }
        let hue = if max == r {
            60.0 * (((g - b) / chroma) % 6.0)
        } else if max == g {
            60.0 * ((b - r) / chroma + 2.0)
        } else {
            60.0 * ((r - g) / chroma + 4.0)
        };
        let hue = if hue < 0.0 { hue + 360.0 } else { hue };
        if hue < 20.0 || hue >= 330.0 {
            // Dark red with green/blue below red reads as brown in WC3 terrain.
            if max < 170.0 && g > 30.0 {
                Self::Brown
            } else {
                Self::Red
            }
        } else if hue < 45.0 {
            if max < 150.0 {
                Self::Brown
            } else {
                Self::Orange
            }
        } else if hue < 70.0 {
            Self::Yellow
        } else if hue < 170.0 {
            Self::Green
        } else if hue < 200.0 {
            Self::Cyan
        } else if hue < 260.0 {
            Self::Blue
        } else if hue < 300.0 {
            Self::Purple
        } else {
            Self::Magenta
        }
    }
}

#[derive(Debug, Clone)]
pub struct TextureReport {
    pub name: String,
    pub source_len: usize,
    pub sha256_source: String,
    pub header: Option<BlpHeader>,
    pub header_error: Option<String>,
    pub decoded: bool,
    pub decode_error: Option<String>,
    pub sha256_rgba: Option<String>,
    pub width: u32,
    pub height: u32,
    pub rgba_len: usize,
    pub mip_count: u8,
    /// Mip levels declared by the header (offset table).
    pub declared_mips: u8,
    /// Declared mip dims/sizes are self-consistent (paletted BLP1 stores
    /// exactly w*h indices per level; JPEG levels are variable-size).
    pub declared_mip_layout_ok: bool,
    pub mip_chain_ok: bool,
    pub alpha_min: u8,
    pub alpha_max: u8,
    pub alpha_mean: u8,
    pub transparent_pixels: usize,
    pub translucent_pixels: usize,
    pub opaque_pixels: usize,
    pub unique_colors_sampled: usize,
    pub mean_rgb: [u8; 3],
    pub hue: HueClass,
    pub all_black: bool,
    pub all_transparent: bool,
    pub constant_color: bool,
}

impl TextureReport {
    /// Structural validity: header parsed, pixels decoded, mip chain
    /// consistent. A fully transparent surface is *not* an error — WC3 ships
    /// blank overlays such as `UI\Widgets\EscMenu\Human\blank-background.blp`
    /// whose palette, indices and alpha plane are all zero on purpose — so it
    /// is reported as a statistic, not as a failure.
    pub fn ok(&self) -> bool {
        self.header.is_some() && self.decoded && self.mip_chain_ok
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Decode a BLP and compute the validation report. One file in, one report out;
/// no decoded pixel buffer is retained, so a full-corpus scan stays flat.
pub fn validate(name: &str, bytes: &[u8]) -> TextureReport {
    let header = probe(bytes);
    let (header, header_error) = match header {
        Ok(header) => (Some(header), None),
        Err(err) => (None, Some(err.to_string())),
    };
    let declared = header
        .as_ref()
        .map(|header| (header.mip_count, declared_mip_layout(header)))
        .unwrap_or((0, false));
    let source_len = bytes.len();
    let sha256_source = sha256_hex(bytes);
    let mut report = TextureReport {
        name: name.to_string(),
        source_len,
        sha256_source,
        header,
        header_error,
        decoded: false,
        decode_error: None,
        sha256_rgba: None,
        width: 0,
        height: 0,
        rgba_len: 0,
        mip_count: 0,
        declared_mips: 0,
        declared_mip_layout_ok: false,
        mip_chain_ok: false,
        alpha_min: 0,
        alpha_max: 0,
        alpha_mean: 0,
        transparent_pixels: 0,
        translucent_pixels: 0,
        opaque_pixels: 0,
        unique_colors_sampled: 0,
        mean_rgb: [0, 0, 0],
        hue: HueClass::Gray,
        all_black: false,
        all_transparent: false,
        constant_color: false,
    };
    let decoded = match decode_blp(bytes) {
        Ok(image) => image,
        Err(err) => {
            report.decode_error = Some(err.to_string());
            return report;
        }
    };
    report.decoded = true;
    report.width = decoded.width;
    report.height = decoded.height;
    report.rgba_len = decoded.rgba.len();
    report.mip_count = decoded.mipmaps.len() as u8;
    report.declared_mips = declared.0;
    report.declared_mip_layout_ok = declared.1;
    report.sha256_rgba = Some(sha256_hex(&decoded.rgba));

    // Mip chain: each level must be the previous halved (min 1) and hold
    // exactly w*h*4 bytes.
    let mut expected_w = decoded.width;
    let mut expected_h = decoded.height;
    let mut chain_ok = !decoded.mipmaps.is_empty();
    for mip in &decoded.mipmaps {
        if mip.width != expected_w || mip.height != expected_h {
            chain_ok = false;
            break;
        }
        if mip.rgba.len() != (mip.width as usize * mip.height as usize * 4) {
            chain_ok = false;
            break;
        }
        expected_w = (expected_w / 2).max(1);
        expected_h = (expected_h / 2).max(1);
    }
    if decoded.mipmaps.len() > 16 {
        chain_ok = false;
    }
    report.mip_chain_ok = chain_ok;

    // Pixel statistics on the first mip only (the surface the game samples).
    let pixels = decoded.rgba.len() / 4;
    if pixels == 0 {
        report.all_transparent = true;
        return report;
    }
    let mut alpha_sum: u64 = 0;
    let mut rgb_sum = [0u64; 3];
    let mut alpha_min = 255u8;
    let mut alpha_max = 0u8;
    let mut transparent = 0usize;
    let mut translucent = 0usize;
    let mut opaque = 0usize;
    let mut unique: HashSet<u32> = HashSet::new();
    let stride = (pixels / 65_536).max(1);
    let mut first: Option<[u8; 4]> = None;
    let mut constant = true;
    let mut all_black = true;
    for (index, px) in decoded.rgba.chunks_exact(4).enumerate() {
        let a = px[3];
        alpha_sum += a as u64;
        alpha_min = alpha_min.min(a);
        alpha_max = alpha_max.max(a);
        match a {
            0 => transparent += 1,
            255 => opaque += 1,
            _ => translucent += 1,
        }
        if index % stride == 0 {
            for c in 0..3 {
                rgb_sum[c] += px[c] as u64;
            }
            let key = u32::from_le_bytes([px[0], px[1], px[2], px[3]]);
            unique.insert(key);
            if let Some(first) = first {
                if first != [px[0], px[1], px[2], px[3]] {
                    constant = false;
                }
            } else {
                first = Some([px[0], px[1], px[2], px[3]]);
            }
            if px[0] > 8 || px[1] > 8 || px[2] > 8 {
                all_black = false;
            }
        }
    }
    let sampled = pixels.div_ceil(stride);
    report.alpha_min = alpha_min;
    report.alpha_max = alpha_max;
    report.alpha_mean = (alpha_sum / pixels as u64) as u8;
    report.transparent_pixels = transparent;
    report.translucent_pixels = translucent;
    report.opaque_pixels = opaque;
    report.unique_colors_sampled = unique.len();
    report.mean_rgb = [
        (rgb_sum[0] / sampled as u64) as u8,
        (rgb_sum[1] / sampled as u64) as u8,
        (rgb_sum[2] / sampled as u64) as u8,
    ];
    report.hue = HueClass::classify(report.mean_rgb);
    report.all_black = all_black;
    report.all_transparent = transparent == pixels;
    report.constant_color = constant;
    report
}

/// Structural check of the declared mip chain: dims halve down to 1x1 and a
/// paletted BLP1 level stores exactly `w*h` indices.
fn declared_mip_layout(header: &BlpHeader) -> bool {
    let paletted = header.magic == *b"BLP1" && header.compression == 1;
    let mut w = header.width as u64;
    let mut h = header.height as u64;
    for level in 0..16 {
        if header.offsets[level] == 0 || header.sizes[level] == 0 {
            return level == header.mip_count as usize;
        }
        if paletted {
            // Paletted BLP1 stores indices followed by the packed alpha plane,
            // so a level is w*h + ceil(w*h*alpha_bits/8) bytes.
            let pixels = w * h;
            let expected = pixels + (pixels * header.alpha_bits as u64).div_ceil(8);
            if header.sizes[level] as u64 != expected {
                return false;
            }
        }
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    true
}

/// Compare a decoded texture against a reference RGBA buffer, and against the
/// same buffer with R/B swapped. Used to prove the channel order is right: a
/// correct decode must be closer to the reference than to its swap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GoldenCompare {
    pub mae_direct: f64,
    pub mae_swapped: f64,
    pub max_error_direct: u8,
}

pub fn compare_golden(decoded: &[u8], reference_rgba: &[u8]) -> Option<GoldenCompare> {
    if decoded.len() != reference_rgba.len() || decoded.is_empty() {
        return None;
    }
    let mut direct: u64 = 0;
    let mut swapped: u64 = 0;
    let mut max_error: u8 = 0;
    for (a, b) in decoded.chunks_exact(4).zip(reference_rgba.chunks_exact(4)) {
        for c in 0..3 {
            let diff = a[c].abs_diff(b[c]);
            direct += diff as u64;
            max_error = max_error.max(diff);
            let swap = a[[1, 0, 2][c]].abs_diff(b[c]);
            swapped += swap as u64;
        }
        let alpha_diff = a[3].abs_diff(b[3]);
        direct += alpha_diff as u64;
        swapped += alpha_diff as u64;
    }
    let count = (decoded.len() / 4) as u64 * 4;
    Some(GoldenCompare {
        mae_direct: direct as f64 / count as f64,
        mae_swapped: swapped as f64 / count as f64,
        max_error_direct: max_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_reads_blp1_header() {
        let mut bytes = vec![0u8; 200];
        bytes[..4].copy_from_slice(b"BLP1");
        bytes[4..8].copy_from_slice(&1u32.to_le_bytes()); // paletted
        bytes[8..12].copy_from_slice(&8u32.to_le_bytes()); // alpha bits
        bytes[12..16].copy_from_slice(&64u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&32u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&156u32.to_le_bytes());
        bytes[92..96].copy_from_slice(&2048u32.to_le_bytes());
        let header = probe(&bytes).expect("header");
        assert_eq!(header.magic, *b"BLP1");
        assert_eq!(header.compression, 1);
        assert_eq!(header.alpha_bits, 8);
        assert_eq!((header.width, header.height), (64, 32));
        assert_eq!(header.mip_count, 1);
        assert_eq!(header.sizes[0], 2048);
    }

    #[test]
    fn probe_rejects_short_and_foreign() {
        assert!(matches!(probe(b"BLP"), Err(HeaderError::TooShort(3))));
        assert_eq!(probe(b"XXXXyyyy"), Err(HeaderError::BadMagic(*b"XXXX")));
    }

    #[test]
    fn classify_separates_terrain_families() {
        assert_eq!(HueClass::classify([80, 90, 70]), HueClass::Green);
        assert_eq!(HueClass::classify([120, 90, 60]), HueClass::Brown);
        assert_eq!(HueClass::classify([200, 200, 205]), HueClass::Gray);
        assert_eq!(HueClass::classify([30, 60, 160]), HueClass::Blue);
    }

    #[test]
    fn golden_compare_prefers_the_correct_channel_order() {
        let reference: Vec<u8> = (0..64u8)
            .flat_map(|i| [i.saturating_mul(3), i.saturating_mul(2), i, 255])
            .collect();
        let same = compare_golden(&reference, &reference).expect("compare");
        assert!(same.mae_direct < 0.001);
        assert!(same.mae_swapped > same.mae_direct);
    }
}
