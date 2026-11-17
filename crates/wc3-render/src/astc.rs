//! ASTC container: read/write the raw `.astc` file produced by `astcenc`.
//!
//! The renderer does not embed an ASTC encoder (it is a multi-thousand-line
//! rate-distortion optimiser); the asset pipeline encodes offline with
//! `astcenc`, and the runtime consumes the result through this module:
//!
//! ```text
//! astcenc -c texture.png texture.astc 6x6 -thorough
//!   -> AstcFile::parse(bytes) -> CompressedImage -> TextureStore::upload_compressed
//! ```
//!
//! The container is the 16-byte header `astcenc` writes:
//!
//! ```text
//! 0x00  magic 0x5C 0xA1 0xAB 0x13 (little endian 0x5CA1AB13)
//! 0x04  block_x  u8
//! 0x05  block_y  u8
//! 0x06  block_z  u8
//! 0x07  size_x   u24 little endian
//! 0x0A  size_y   u24
//! 0x0D  size_z   u24
//! 0x10  payload: ceil(x/bx) * ceil(y/by) * 16 bytes per 2D image
//! ```
//!
//! A file has one level only; callers that want a mip chain run the encoder per
//! level and stitch the levels with [`CompressedImage::single_level`].

use crate::texture::{CompressedFormat, CompressedImage, CompressedLevel};

/// Magic written by `astcenc` (`ASTC` little-endian).
pub const ASTC_MAGIC: u32 = 0x5C_A1_AB_13;
/// Header size in bytes.
pub const ASTC_HEADER: usize = 16;

/// Largest texture side accepted, matching `GL_MAX_TEXTURE_SIZE` (16384) on the
/// supported Vulkan/GLES 3 adapters. The container stores u24 dimensions, so a
/// crafted header can name 16M texels per side; the cap keeps the block-grid
/// arithmetic and the GPU upload inside what the device can actually hold.
pub const MAX_TEXTURE_SIDE: u32 = 16_384;
/// Largest texel count in one image (`MAX_TEXTURE_SIDE²`).
pub const MAX_TEXELS: u64 = MAX_TEXTURE_SIDE as u64 * MAX_TEXTURE_SIDE as u64;
/// Largest block grid in one image. ASTC's smallest block is 1x1, so no legal
/// image needs more blocks than it has texels.
pub const MAX_BLOCKS: u64 = MAX_TEXELS;

/// Block grid and payload length for `block` x `size`, every step checked.
///
/// This is the single source of truth for the payload layout: [`AstcFile::parse`]
/// validates the file against it, [`AstcFile::to_image`] re-checks the in-memory
/// image with it, and the tests build fixtures from it, so the three can never
/// disagree about a length.
pub fn payload_len(block: (u32, u32, u32), size: (u32, u32, u32)) -> Result<usize, String> {
    let (bx, by, bz) = block;
    let (w, h, d) = size;
    if bx == 0 || by == 0 || bz == 0 {
        return Err(format!("astc: zero block dimension {block:?}"));
    }
    if w == 0 || h == 0 || d == 0 {
        return Err(format!("astc: zero image dimension {size:?}"));
    }
    if w > MAX_TEXTURE_SIDE || h > MAX_TEXTURE_SIDE {
        return Err(format!(
            "astc: image {w}x{h} exceeds the {MAX_TEXTURE_SIDE}-texel side limit"
        ));
    }
    let texels = (w as u64)
        .checked_mul(h as u64)
        .and_then(|texels| texels.checked_mul(d as u64))
        .ok_or_else(|| format!("astc: {w}x{h}x{d} texel count overflows"))?;
    if texels > MAX_TEXELS {
        return Err(format!(
            "astc: {w}x{h}x{d} exceeds the {MAX_TEXELS}-texel image limit"
        ));
    }
    let blocks_x = (w as u64).div_ceil(bx as u64);
    let blocks_y = (h as u64).div_ceil(by as u64);
    let blocks_z = (d as u64).div_ceil(bz as u64);
    let blocks = blocks_x
        .checked_mul(blocks_y)
        .and_then(|blocks| blocks.checked_mul(blocks_z))
        .ok_or_else(|| format!("astc: block grid overflows for {w}x{h}x{d} with {block:?}"))?;
    if blocks > MAX_BLOCKS {
        return Err(format!(
            "astc: {blocks}-block grid exceeds the {MAX_BLOCKS}-block limit"
        ));
    }
    let bytes = blocks
        .checked_mul(16)
        .ok_or_else(|| format!("astc: payload length overflows for {blocks} blocks"))?;
    usize::try_from(bytes)
        .map_err(|_| format!("astc: {bytes}-byte payload does not fit in this address space"))
}

/// Raw `.astc` payload with its header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AstcFile {
    pub block: (u32, u32, u32),
    pub size: (u32, u32, u32),
    pub data: Vec<u8>,
}

impl AstcFile {
    /// Parse a `.astc` file. Every length is checked; a truncated or padded file
    /// is rejected instead of being uploaded as garbage.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < ASTC_HEADER {
            return Err(format!(
                "astc: {}-byte file is shorter than the {ASTC_HEADER}-byte header",
                bytes.len()
            ));
        }
        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if magic != ASTC_MAGIC {
            return Err(format!("astc: bad magic {magic:#010x}, expected {ASTC_MAGIC:#010x}"));
        }
        let block = (bytes[4] as u32, bytes[5] as u32, bytes[6] as u32);
        let read_u24 = |offset: usize| -> u32 {
            (bytes[offset] as u32) | ((bytes[offset + 1] as u32) << 8) | ((bytes[offset + 2] as u32) << 16)
        };
        let size = (read_u24(7), read_u24(10), read_u24(13));
        if block.0 == 0 || block.1 == 0 || block.2 != 1 {
            return Err(format!(
                "astc: unsupported block {:?} (2D images need block_z == 1)",
                block
            ));
        }
        if size.0 == 0 || size.1 == 0 || size.2 != 1 {
            return Err(format!("astc: unsupported image size {:?}", size));
        }
        let expected = payload_len(block, size)?;
        let data = &bytes[ASTC_HEADER..];
        if data.len() != expected {
            return Err(format!(
                "astc: payload is {} bytes, expected {expected} for {}x{} with {:?} blocks",
                data.len(),
                size.0,
                size.1,
                block
            ));
        }
        Ok(Self {
            block,
            size,
            data: data.to_vec(),
        })
    }

    /// The renderer's compressed format for this block size, if supported.
    pub fn format(&self) -> Option<CompressedFormat> {
        match (self.block.0, self.block.1, self.block.2) {
            (6, 6, 1) => Some(CompressedFormat::Astc6x6),
            (8, 8, 1) => Some(CompressedFormat::Astc8x8),
            _ => None,
        }
    }

    /// Convert to the renderer's image type (single mip level).
    pub fn to_image(&self) -> Result<CompressedImage, String> {
        let format = self.format().ok_or_else(|| {
            format!(
                "astc: block {:?} has no renderer format (supported: 6x6, 8x8); \
                 re-encode with `-c6x6` or `-c8x8`",
                self.block
            )
        })?;
        // Re-check the in-memory image with the same layout arithmetic `parse`
        // used: `AstcFile` fields are public, so a hand-built value must not be
        // able to reach `validate()` with an impossible block grid.
        let expected = payload_len(self.block, self.size)?;
        if self.data.len() != expected {
            return Err(format!(
                "astc: {} payload bytes, expected {expected} for {:?} with {:?} blocks",
                self.data.len(),
                self.size,
                self.block
            ));
        }
        let image = CompressedImage {
            format,
            width: self.size.0,
            height: self.size.1,
            levels: vec![CompressedLevel {
                width: self.size.0,
                height: self.size.1,
                data: self.data.clone(),
            }],
        };
        image.validate()?;
        Ok(image)
    }

    /// Serialise back to `.astc` (used by the asset-pipeline tooling).
    pub fn write(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(ASTC_HEADER + self.data.len());
        out.extend_from_slice(&ASTC_MAGIC.to_le_bytes());
        out.push(self.block.0 as u8);
        out.push(self.block.1 as u8);
        out.push(self.block.2 as u8);
        for value in [self.size.0, self.size.1, self.size.2] {
            out.push((value & 0xFF) as u8);
            out.push(((value >> 8) & 0xFF) as u8);
            out.push(((value >> 16) & 0xFF) as u8);
        }
        out.extend_from_slice(&self.data);
        out
    }
}

/// Parse a `.astc` byte slice and convert it to a renderer image.
pub fn decode(bytes: &[u8]) -> Result<CompressedImage, String> {
    AstcFile::parse(bytes)?.to_image()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `.astc` byte stream with an explicit payload.
    fn header(block: (u8, u8, u8), size: (u32, u32, u32), payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&ASTC_MAGIC.to_le_bytes());
        bytes.push(block.0);
        bytes.push(block.1);
        bytes.push(block.2);
        for value in [size.0, size.1, size.2] {
            bytes.push((value & 0xFF) as u8);
            bytes.push(((value >> 8) & 0xFF) as u8);
            bytes.push(((value >> 16) & 0xFF) as u8);
        }
        bytes.extend_from_slice(payload);
        bytes
    }

    /// Block grid + payload length through the production layout helper, so the
    /// fixtures and the parser can never disagree.
    fn layout(block: (u8, u8, u8), width: u32, height: u32) -> usize {
        payload_len(
            (block.0 as u32, block.1 as u32, block.2 as u32),
            (width, height, 1),
        )
        .expect("fixture layout")
    }

    fn fixture(block: (u8, u8, u8), width: u32, height: u32) -> Vec<u8> {
        header(
            block,
            (width, height, 1),
            &vec![0xAB; layout(block, width, height)],
        )
    }

    #[test]
    fn parses_a_6x6_file() {
        let bytes = fixture((6, 6, 1), 12, 6);
        let file = AstcFile::parse(&bytes).expect("parse");
        assert_eq!(file.block, (6, 6, 1));
        assert_eq!(file.size, (12, 6, 1));
        assert_eq!(file.data.len(), 2 * 16);
        assert_eq!(file.format(), Some(CompressedFormat::Astc6x6));
        let image = file.to_image().expect("image");
        assert_eq!((image.width, image.height), (12, 6));
        assert_eq!(image.bytes(), 32);
        image.validate().expect("valid");
    }

    #[test]
    fn round_trips_through_write() {
        let bytes = fixture((8, 8, 1), 8, 16);
        let file = AstcFile::parse(&bytes).expect("parse");
        assert_eq!(file.write(), bytes);
    }

    #[test]
    fn rejects_bad_magic_and_truncation() {
        let mut bytes = fixture((6, 6, 1), 6, 6);
        bytes[0] = 0;
        assert!(AstcFile::parse(&bytes).unwrap_err().contains("bad magic"));
        let bytes = fixture((6, 6, 1), 6, 6);
        assert!(AstcFile::parse(&bytes[..20]).unwrap_err().contains("payload"));
        assert!(AstcFile::parse(&bytes[..8]).unwrap_err().contains("shorter"));
    }

    #[test]
    fn rejects_unsupported_block_size() {
        let bytes = fixture((4, 4, 1), 4, 4);
        let file = AstcFile::parse(&bytes).expect("parse");
        assert_eq!(file.format(), None);
        assert!(file.to_image().unwrap_err().contains("no renderer format"));
    }

    /// The block grid and payload length of every supported block size, with
    /// partial edge blocks rounded up.
    #[test]
    fn layout_matches_the_block_grid() {
        assert_eq!(layout((6, 6, 1), 12, 6), 2 * 16);
        assert_eq!(layout((8, 8, 1), 8, 16), 2 * 16);
        // A 7x7 image still needs a full 2x2 block grid.
        assert_eq!(layout((6, 6, 1), 7, 7), 4 * 16);
        // 1x1 blocks are legal in the container (the renderer just has no
        // format for them): one block per texel.
        assert_eq!(layout((1, 1, 1), 4, 3), 12 * 16);
    }

    /// AUD-009: dimensions whose block count overflows `u32` used to wrap to a
    /// small expected length, so a header-only file validated as a huge image.
    /// 98304x98304 with 6x6 blocks is 16384 * 16384 * 16 = 2^32 bytes.
    #[test]
    fn rejects_dimensions_that_overflow_the_u32_payload() {
        let bytes = header((6, 6, 1), (98_304, 98_304, 1), &[]);
        let err = AstcFile::parse(&bytes).unwrap_err();
        assert!(err.contains("side limit"), "{err}");
        assert!(AstcFile::parse(&bytes).is_err());

        // 32768x32768 with 1x1 blocks is 2^34 bytes, which also wrapped to 0.
        let bytes = header((1, 1, 1), (0x8000, 0x8000, 1), &[]);
        assert!(AstcFile::parse(&bytes).unwrap_err().contains("side limit"));

        // The decode path used to panic on the multiply in the block-layout
        // check; it must now be a typed error.
        let bytes = header((6, 6, 1), (98_304, 98_304, 1), &[]);
        assert!(decode(&bytes).is_err());
    }

    /// AUD-009: u24 dimensions near the maximum are rejected by the cap before
    /// any arithmetic can overflow.
    #[test]
    fn rejects_u24_max_dimensions() {
        for side in [0x00FF_FFFFu32, 0x00FF_FFFF - 1, MAX_TEXTURE_SIDE + 1] {
            let bytes = header((6, 6, 1), (side, 6, 1), &[]);
            let err = AstcFile::parse(&bytes).unwrap_err();
            assert!(err.contains("side limit"), "{side}: {err}");
        }
        let bytes = header((6, 6, 1), (6, 0x00FF_FFFF, 1), &[]);
        assert!(AstcFile::parse(&bytes).unwrap_err().contains("side limit"));
    }

    /// AUD-009: zero and unsupported header fields are typed errors.
    #[test]
    fn rejects_zero_and_non_2d_sides() {
        for size in [(0, 6, 1), (6, 0, 1), (6, 6, 0), (6, 6, 2)] {
            let bytes = header((6, 6, 1), size, &[]);
            assert!(
                AstcFile::parse(&bytes)
                    .unwrap_err()
                    .contains("unsupported image size"),
                "{size:?}"
            );
        }
        for block in [(0, 6, 1), (6, 0, 1), (6, 6, 0), (6, 6, 2)] {
            let bytes = header(block, (6, 6, 1), &[]);
            assert!(
                AstcFile::parse(&bytes)
                    .unwrap_err()
                    .contains("unsupported block"),
                "{block:?}"
            );
        }
    }

    /// AUD-009: the payload length is exact — one byte short or long fails.
    #[test]
    fn payload_length_is_exact() {
        let good = fixture((6, 6, 1), 12, 6);
        assert!(AstcFile::parse(&good).is_ok());

        let mut short = good.clone();
        short.pop();
        let err = AstcFile::parse(&short).unwrap_err();
        assert!(err.contains("payload is 31 bytes"), "{err}");

        let mut long = good;
        long.push(0);
        let err = AstcFile::parse(&long).unwrap_err();
        assert!(err.contains("payload is 33 bytes"), "{err}");
    }

    /// AUD-009: the image cap is a hard boundary; at the cap the payload check
    /// still runs, one texel over is refused outright.
    #[test]
    fn enforces_the_image_cap() {
        // Exactly at the side cap: the cap passes, the (absent) payload fails.
        let bytes = header((8, 8, 1), (MAX_TEXTURE_SIDE, 8, 1), &[]);
        assert!(AstcFile::parse(&bytes).unwrap_err().contains("payload is 0 bytes"));
        let bytes = header((8, 8, 1), (MAX_TEXTURE_SIDE, MAX_TEXTURE_SIDE, 1), &[]);
        assert!(AstcFile::parse(&bytes).unwrap_err().contains("payload is 0 bytes"));

        // One texel over the side cap.
        let bytes = header((8, 8, 1), (MAX_TEXTURE_SIDE + 1, 8, 1), &[]);
        assert!(AstcFile::parse(&bytes).unwrap_err().contains("side limit"));
        // A single oversized side hides behind a small other side.
        let bytes = header((8, 8, 1), (8, MAX_TEXTURE_SIDE + 1, 1), &[]);
        assert!(AstcFile::parse(&bytes).unwrap_err().contains("side limit"));

        // The cap holds for the smallest legal block size too.
        assert!(payload_len((1, 1, 1), (MAX_TEXTURE_SIDE, MAX_TEXTURE_SIDE, 1)).is_ok());
        assert!(payload_len((1, 1, 1), (MAX_TEXTURE_SIDE + 1, MAX_TEXTURE_SIDE, 1)).is_err());
    }
}
