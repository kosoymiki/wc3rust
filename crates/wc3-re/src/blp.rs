//! BLP1/BLP2 texture access.
//!
//! ## BLP1 (`BLP1`, 156-byte header, all 3,935 local textures are BLP1)
//!
//! ```text
//! 0x00 u32 magic "BLP1"
//! 0x04 u32 content       0 = JPEG, 1 = paletted
//! 0x08 u32 alpha_bits    0/1/4/8
//! 0x0C u32 width
//! 0x10 u32 height
//! 0x14 u32 picture_type  0 = no alpha? (WC3 writes 0/1; kept verbatim)
//! 0x18 u32 picture_subtype
//! 0x1C [u32;16] mip offsets (absolute file offsets)
//! 0x5C [u32;16] mip sizes
//! 0x9C palette[256 x BGRA]  (paletted) OR u32 jpeg_header_size + JPEG header
//! ```
//!
//! Paletted pixels are one byte per pixel at mip 0; the alpha plane follows
//! the pixel plane (`alpha_bits` packed). JPEG content is CMYK with inverted
//! alpha in K (see the `cmyk_pixel` comment in `wc3-formats`).
//!
//! ## BLP2 (`BLP2`, 148-byte header)
//!
//! ```text
//! 0x00 u32 magic "BLP2"
//! 0x04 u32 version
//! 0x08 u8  compression   1 = paletted, 2 = DXT, 3 = BGRA
//! 0x09 u8  alpha_bits
//! 0x0A u8  alpha_type
//! 0x0B u8  has_mips
//! 0x0C u32 width
//! 0x10 u32 height
//! 0x14 [u32;16] mip offsets
//! 0x54 [u32;16] mip sizes
//! 0x94 palette[256 x BGRA] (paletted) + [256 x u8] alpha table
//! ```
//!
//! sRGB note: WC3 treats these as linear RGBA8 for the fixed-function/GL1
//! pipeline; the port uploads the decoded bytes as sRGB textures to match the
//! original framebuffer result.

use wc3_formats::blp::{BlpImage, MipLevel};

use crate::error::{ReError, Result};

pub const BLP1_HEADER_BYTES: usize = 0x9C;
pub const BLP2_HEADER_BYTES: usize = 0x94;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlpEncoding {
    Palette,
    Jpeg,
    Dxt1,
    Dxt3,
    Dxt5,
    Bgra,
}

impl BlpEncoding {
    pub fn label(self) -> &'static str {
        match self {
            Self::Palette => "paletted",
            Self::Jpeg => "jpeg",
            Self::Dxt1 => "dxt1",
            Self::Dxt3 => "dxt3",
            Self::Dxt5 => "dxt5",
            Self::Bgra => "bgra",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BlpProbe {
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub alpha_bits: u32,
    pub encoding: BlpEncoding,
    /// BLP1 content field / BLP2 compression byte, verbatim.
    pub content: u32,
    /// BLP2 alpha encoding; 0 for BLP1.
    pub alpha_type: u32,
    pub mip_offsets: [u32; 16],
    pub mip_sizes: [u32; 16],
    pub mip_count: usize,
    /// BLP1 JPEG header size (offset 0x9C), if JPEG content.
    pub jpeg_header_size: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct DecodedBlp {
    pub probe: BlpProbe,
    pub image: BlpImage,
}

impl DecodedBlp {
    pub fn rgba(&self) -> &[u8] {
        &self.image.rgba
    }

    pub fn mips(&self) -> &[MipLevel] {
        &self.image.mipmaps
    }
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| ReError::Parse(format!("BLP truncated at 0x{offset:x}")))?;
    Ok(u32::from_le_bytes(slice.try_into().expect("4 bytes")))
}

/// Read the header only (no decompression). Used by the install scanner to
/// classify all 3,935 textures without spending 30+ seconds on JPEG decode.
pub fn probe(bytes: &[u8]) -> Result<BlpProbe> {
    if bytes.len() < 4 {
        return Err(ReError::Parse("BLP shorter than magic".into()));
    }
    match &bytes[..4] {
        b"BLP1" => probe_blp1(bytes),
        b"BLP2" => probe_blp2(bytes),
        other => Err(ReError::Parse(format!("not a BLP: {other:?}"))),
    }
}

fn probe_blp1(bytes: &[u8]) -> Result<BlpProbe> {
    if bytes.len() < BLP1_HEADER_BYTES {
        return Err(ReError::Parse("BLP1 header truncated".into()));
    }
    let content = u32_at(bytes, 0x04)?;
    let alpha_bits = u32_at(bytes, 0x08)?;
    let width = u32_at(bytes, 0x0C)?;
    let height = u32_at(bytes, 0x10)?;
    let mut mip_offsets = [0u32; 16];
    let mut mip_sizes = [0u32; 16];
    let mut mip_count = 0;
    for i in 0..16 {
        mip_offsets[i] = u32_at(bytes, 0x1C + i * 4)?;
        mip_sizes[i] = u32_at(bytes, 0x5C + i * 4)?;
        if mip_offsets[i] != 0 && mip_sizes[i] != 0 {
            mip_count += 1;
        }
    }
    let encoding = match content {
        0 => BlpEncoding::Jpeg,
        1 => BlpEncoding::Palette,
        other => {
            return Err(ReError::Unsupported(format!(
                "BLP1 content type {other}"
            )));
        }
    };
    let jpeg_header_size = if encoding == BlpEncoding::Jpeg {
        Some(u32_at(bytes, BLP1_HEADER_BYTES)?)
    } else {
        None
    };
    Ok(BlpProbe {
        version: 1,
        width,
        height,
        alpha_bits,
        encoding,
        content,
        alpha_type: 0,
        mip_offsets,
        mip_sizes,
        mip_count,
        jpeg_header_size,
    })
}

fn probe_blp2(bytes: &[u8]) -> Result<BlpProbe> {
    if bytes.len() < BLP2_HEADER_BYTES {
        return Err(ReError::Parse("BLP2 header truncated".into()));
    }
    let compression = bytes[8] as u32;
    let alpha_bits = bytes[9] as u32;
    let alpha_type = bytes[10] as u32;
    let width = u32_at(bytes, 0x0C)?;
    let height = u32_at(bytes, 0x10)?;
    let mut mip_offsets = [0u32; 16];
    let mut mip_sizes = [0u32; 16];
    let mut mip_count = 0;
    for i in 0..16 {
        mip_offsets[i] = u32_at(bytes, 0x14 + i * 4)?;
        mip_sizes[i] = u32_at(bytes, 0x54 + i * 4)?;
        if mip_offsets[i] != 0 && mip_sizes[i] != 0 {
            mip_count += 1;
        }
    }
    let encoding = match compression {
        1 => BlpEncoding::Palette,
        2 => match (alpha_bits, alpha_type) {
            (0 | 1, 0) => BlpEncoding::Dxt1,
            (1, _) => BlpEncoding::Dxt1,
            (8, _) => BlpEncoding::Dxt3,
            _ => BlpEncoding::Dxt5,
        },
        3 => BlpEncoding::Bgra,
        other => {
            return Err(ReError::Unsupported(format!(
                "BLP2 compression {other}"
            )));
        }
    };
    Ok(BlpProbe {
        version: 2,
        width,
        height,
        alpha_bits,
        encoding,
        content: compression,
        alpha_type,
        mip_offsets,
        mip_sizes,
        mip_count,
        jpeg_header_size: None,
    })
}

/// Full decode to RGBA8 plus every stored mip.
pub fn decode(bytes: &[u8]) -> Result<DecodedBlp> {
    let header = probe(bytes)?;
    let image = wc3_formats::blp::decode(bytes)?;
    Ok(DecodedBlp {
        probe: header,
        image,
    })
}

/// Decode only the first mip (the only level the UI/model layer samples today).
pub fn decode_first_mip(bytes: &[u8]) -> Result<(BlpProbe, Vec<u8>)> {
    let decoded = decode(bytes)?;
    Ok((decoded.probe, decoded.image.rgba))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture() -> Option<Vec<u8>> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/ReplaceableTextures/CommandButtons/BTNFootman.blp");
        std::fs::read(path).ok()
    }

    #[test]
    fn probes_and_decodes_command_button() {
        let Some(bytes) = fixture() else {
            eprintln!("skip: BTNFootman.blp missing");
            return;
        };
        let header = probe(&bytes).expect("probe");
        assert_eq!(header.version, 1);
        assert_eq!((header.width, header.height), (64, 64));
        let decoded = decode(&bytes).expect("decode");
        assert_eq!(decoded.rgba().len(), 64 * 64 * 4);
    }

    #[test]
    fn rejects_non_blp() {
        assert!(probe(b"MDLX\0\0\0\0").is_err());
    }
}
