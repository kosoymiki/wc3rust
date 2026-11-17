//! WC3 data formats: MPQ-adjacent binary formats and map data.
//!
//! Implemented from the published format layouts (and validated against real
//! `War3.mpq` fixtures):
//! - `mdx` — classic MDX v800 model (geosets, nodes, tracks, materials, cameras)
//! - `blp` — BLP1/BLP2 textures (paletted, DXT1/3/5, BGRA)
//! - `slk` — SLK spreadsheets (`UnitData.slk`, `AbilityData.slk`, …)
//! - `w3e`/`w3i`/`objects`/`map` — terrain, map info, object data, W3M/W3X
//! - `ini` — key=value config files (`war3mapMisc.txt`, …)

pub mod blp;
pub mod diag;
pub mod fdf;
pub mod gamedata;
pub mod ini;
pub mod map;
pub mod mdx;
pub mod media;
pub mod objects;
pub mod slk;
pub mod w3e;
pub mod w3i;
pub mod wpm;

use std::fmt;

/// First `N` bytes of `bytes` as an array, or `None` when the input is shorter.
///
/// Total by construction: the slice is split with `first_chunk`, so a forged
/// length can never turn into an index panic.
pub(crate) fn head<const N: usize>(bytes: &[u8]) -> Option<[u8; N]> {
    bytes.first_chunk::<N>().copied()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Mdx,
    Blp1,
    Blp2,
    Slk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MdxHeader {
    pub version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlpHeader {
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatError {
    TooShort,
    UnknownMagic([u8; 4]),
    MissingMdxVersionChunk,
    BadMdxVersionChunk,
    Corrupt(&'static str),
    Unsupported(&'static str),
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort => f.write_str("file shorter than header"),
            Self::UnknownMagic(m) => write!(f, "unknown magic {m:?}"),
            Self::MissingMdxVersionChunk => f.write_str("MDX VERS chunk missing"),
            Self::BadMdxVersionChunk => f.write_str("MDX VERS chunk malformed"),
            Self::Corrupt(why) => write!(f, "corrupt data: {why}"),
            Self::Unsupported(why) => write!(f, "unsupported format variant: {why}"),
        }
    }
}

impl std::error::Error for FormatError {}

pub fn detect(bytes: &[u8]) -> Result<Format, FormatError> {
    let magic: [u8; 4] = head(bytes).ok_or(FormatError::TooShort)?;
    match &magic {
        b"MDLX" => Ok(Format::Mdx),
        b"BLP1" => Ok(Format::Blp1),
        b"BLP2" => Ok(Format::Blp2),
        _ if bytes.starts_with(b"ID;PWXL") => Ok(Format::Slk),
        _ => Err(FormatError::UnknownMagic(magic)),
    }
}

pub fn parse_mdx_header(bytes: &[u8]) -> Result<MdxHeader, FormatError> {
    let magic: [u8; 4] = head(bytes).ok_or(FormatError::TooShort)?;
    if &magic != b"MDLX" {
        return Err(FormatError::UnknownMagic(magic));
    }
    let chunk: [u8; 8] = bytes
        .get(4..12)
        .and_then(|slice| slice.first_chunk::<8>().copied())
        .ok_or(FormatError::TooShort)?;
    if &chunk[..4] != b"VERS" {
        return Err(FormatError::MissingMdxVersionChunk);
    }
    let chunk_size = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
    if chunk_size != 4 {
        return Err(FormatError::BadMdxVersionChunk);
    }
    let version_bytes: [u8; 4] = bytes
        .get(12..16)
        .and_then(|slice| slice.first_chunk::<4>().copied())
        .ok_or(FormatError::TooShort)?;
    Ok(MdxHeader {
        version: u32::from_le_bytes(version_bytes),
    })
}

pub fn parse_blp_header(bytes: &[u8]) -> Result<BlpHeader, FormatError> {
    let magic: [u8; 4] = head(bytes).ok_or(FormatError::TooShort)?;
    if &magic != b"BLP1" {
        return Err(FormatError::UnknownMagic(magic));
    }
    let version: [u8; 4] = bytes
        .get(4..8)
        .and_then(|slice| slice.first_chunk::<4>().copied())
        .ok_or(FormatError::TooShort)?;
    Ok(BlpHeader {
        version: u32::from_le_bytes(version),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture(rel: &str) -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_samples")
            .join(rel);
        fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    #[test]
    fn detects_classic_mdx_and_version() {
        let b = fixture("Units/Human/Footman/Footman.mdx");
        assert_eq!(detect(&b).unwrap(), Format::Mdx);
        assert_eq!(parse_mdx_header(&b).unwrap(), MdxHeader { version: 800 });
    }

    #[test]
    fn detects_classic_blp() {
        let b = fixture("Textures/Footman.blp");
        assert_eq!(detect(&b).unwrap(), Format::Blp1);
        assert_eq!(parse_blp_header(&b).unwrap(), BlpHeader { version: 0 });
    }

    #[test]
    fn detects_slk() {
        let b = fixture("Units/UnitData.slk");
        assert_eq!(detect(&b).unwrap(), Format::Slk);
    }

    #[test]
    fn rejects_unknown_and_short() {
        assert_eq!(detect(b"abc"), Err(FormatError::TooShort));
        assert!(matches!(
            detect(b"NOPE1234"),
            Err(FormatError::UnknownMagic(_))
        ));
    }
}
