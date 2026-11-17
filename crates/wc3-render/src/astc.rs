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
        let blocks_x = size.0.div_ceil(block.0);
        let blocks_y = size.1.div_ceil(block.1);
        let expected = (blocks_x * blocks_y * 16) as usize;
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

    fn fixture(block: (u8, u8, u8), width: u32, height: u32) -> Vec<u8> {
        let blocks_x = width.div_ceil(block.0 as u32);
        let blocks_y = height.div_ceil(block.1 as u32);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&ASTC_MAGIC.to_le_bytes());
        bytes.push(block.0);
        bytes.push(block.1);
        bytes.push(block.2);
        for value in [width, height, 1] {
            bytes.push((value & 0xFF) as u8);
            bytes.push(((value >> 8) & 0xFF) as u8);
            bytes.push(((value >> 16) & 0xFF) as u8);
        }
        bytes.extend(std::iter::repeat_n(0xABu8, (blocks_x * blocks_y * 16) as usize));
        bytes
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
}
