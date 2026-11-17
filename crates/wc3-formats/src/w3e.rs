//! Warcraft III terrain (`war3map.w3e`).
//!
//! # Version bounds
//!
//! The classic engine's terrain parser gates the version before reading any
//! field: `if (version < 3) fail` (`0x6f3eb1fe`) and `if (version > 11) fail`
//! (`0x6f3eb207`, `cmp eax, 0xb`), with later branches at 7 (`0x6f3eb42b`) and
//! 9 (`0x6f3eb281`). Reforged (patch 2.0.3+) widened the tile record and moved
//! the format to version 12 — the only other value the map tools know
//! (War3Net `MapEnvironmentFormatVersion.v11 | v12`). Anything below 3 or
//! above 12 is not a terrain file any engine or tool produces.
//!
//! [`Terrain::parse_classic_engine`] applies the measured engine ceiling
//! (3..=11); [`Terrain::parse`] is the bounded multi-era entry point used by
//! corpus tooling that must also read 2.0.3+ maps.

use crate::FormatError;

/// Lowest terrain version the engine accepts (`0x6f3eb1fe`).
pub const VERSION_MIN: u32 = 3;
/// Highest terrain version the classic engine accepts (`0x6f3eb207`).
pub const VERSION_MAX_ENGINE: u32 = 11;
/// Highest terrain version in existence: v12 is the Reforged layout introduced
/// by patch 2.0.3 (War3Net `MapEnvironmentFormatVersion`), read with 8-byte
/// tile records.
pub const VERSION_MAX: u32 = 12;
/// Highest plausible number of ids in the ground/cliff texture lists.
const MAX_TILESET_IDS: usize = 256;

/// v11 tile record size (height, water, texture+flags, variation, cliff).
pub const TILE_SIZE: usize = 7;
/// v12+ tile record size: the texture/flags byte pair became a little-endian
/// `u16` (6-bit texture index, 4 flag bits) while variation/cliff stay bytes.
/// Verified against War3Net `Serialization/Binary/Environment/TerrainTile.cs:15-32`
/// and HiveWE `src/base/terrain.ixx:415-430`.
pub const TILE_SIZE_V12: usize = 8;

#[derive(Debug, Clone, Copy, Default)]
pub struct TerrainTile {
    pub height_data: u16,
    pub water_data: u16,
    pub texture_flags: u8,
    pub variation_data: u8,
    pub cliff_data: u8,
}

impl TerrainTile {
    pub fn height(&self) -> f32 {
        (self.height_data as f32 - 8192.0) / 512.0
    }
    pub fn water_height(&self) -> f32 {
        ((self.water_data & 0x3FFF) as f32 - 8192.0) / 512.0
    }
    pub fn is_edge(&self) -> bool {
        self.water_data & 0x4000 != 0
    }
    /// Ground texture index. For v12+ archives the low nibble is only the
    /// legacy view; [`Terrain::texture_index`] returns the full 6-bit value.
    pub fn texture(&self) -> u8 {
        self.texture_flags & 0x0F
    }
    pub fn flags(&self) -> u8 {
        self.texture_flags >> 4
    }
    /// Ground variation, 5 bits (HiveWE `terrain.ixx:432-433`: `& 0x1F`).
    pub fn variation(&self) -> u8 {
        self.variation_data & 0x1F
    }
    /// Cliff variation, 3 bits (`variation_data >> 5`).
    pub fn cliff_variation(&self) -> u8 {
        self.variation_data >> 5
    }
    pub fn cliff_level(&self) -> u8 {
        self.cliff_data & 0x0F
    }
    pub fn cliff_texture(&self) -> u8 {
        self.cliff_data >> 4
    }
    pub fn is_water(&self) -> bool {
        self.flags() & 0b0100 != 0
    }
    pub fn is_boundary(&self) -> bool {
        self.flags() & 0b1000 != 0
    }
    pub fn is_ramp(&self) -> bool {
        self.flags() & 0b0001 != 0
    }
}

#[derive(Debug, Clone, Default)]
pub struct Terrain {
    pub version: u32,
    pub tileset: u8,
    pub custom_tileset: bool,
    pub ground_tiles: Vec<[u8; 4]>,
    pub cliff_tiles: Vec<[u8; 4]>,
    pub width: u32,
    pub height: u32,
    /// Map origin (`Left`, `Bottom` in War3Net): world offset of tile (0, 0).
    pub center: (f32, f32),
    pub tiles: Vec<TerrainTile>,
    /// Full 6-bit ground-texture index per tile for v12+ archives; empty for
    /// v11 (where the 4-bit index in `TerrainTile::texture_flags` is the whole
    /// value).
    pub texture_indices: Vec<u8>,
    /// Bytes left after the tile array. A well-formed file has zero.
    pub trailing_bytes: usize,
}

impl Terrain {
    /// Parse any terrain version in existence (3..=12): classic 3..=11 with
    /// 7-byte tiles, Reforged 12 with 8-byte tiles.
    ///
    /// Use this for tooling that inspects archives from every era. The game
    /// runtime, which feeds the classic engine's data set, must use
    /// [`Terrain::parse_classic_engine`].
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        Self::parse_bounded(bytes, VERSION_MAX)
    }

    /// Parse with the classic engine's ceiling: versions 3..=11 only.
    ///
    /// `0x6f3eb207` (`cmp eax, 0xb`) makes the TFT Game.dll reject a v12 file
    /// outright, so a v12 terrain can never be a runtime input of this engine.
    pub fn parse_classic_engine(bytes: &[u8]) -> Result<Self, FormatError> {
        Self::parse_bounded(bytes, VERSION_MAX_ENGINE)
    }

    fn parse_bounded(bytes: &[u8], max_version: u32) -> Result<Self, FormatError> {
        if bytes.len() < 13 || bytes.first_chunk::<4>() != Some(b"W3E!") {
            return Err(FormatError::UnknownMagic(
                bytes.first_chunk::<4>().copied().unwrap_or([0; 4]),
            ));
        }
        let mut r = Reader {
            data: bytes,
            pos: 4,
        };
        let version = r.u32()?;
        if version < VERSION_MIN {
            return Err(FormatError::Unsupported("w3e version below 3"));
        }
        if version > max_version {
            return Err(FormatError::Unsupported(
                if max_version == VERSION_MAX_ENGINE {
                    "w3e version above the classic engine ceiling (11)"
                } else {
                    "w3e version above 12"
                },
            ));
        }
        let tileset = r.u8()?;
        let custom_tileset = r.u32()? != 0;
        // Texture lists are a four-byte id each. The game supports 16 ground
        // and 16 cliff textures; 256 is the "cannot be real" ceiling that stops
        // a forged count from reserving gigabytes before the first read fails.
        let ground_count = r.u32()? as usize;
        if ground_count > MAX_TILESET_IDS {
            return Err(FormatError::Corrupt("w3e ground texture count out of range"));
        }
        let mut ground_tiles = Vec::with_capacity(ground_count);
        for _ in 0..ground_count {
            ground_tiles.push(r.tag()?);
        }
        let cliff_count = r.u32()? as usize;
        if cliff_count > MAX_TILESET_IDS {
            return Err(FormatError::Corrupt("w3e cliff texture count out of range"));
        }
        let mut cliff_tiles = Vec::with_capacity(cliff_count);
        for _ in 0..cliff_count {
            cliff_tiles.push(r.tag()?);
        }
        let width = r.u32()?;
        let height = r.u32()?;
        let center = (r.f32()?, r.f32()?);
        if width == 0 || height == 0 || width > 1024 || height > 1024 {
            return Err(FormatError::Corrupt("invalid terrain dimensions"));
        }
        let count = width as usize * height as usize;
        let remaining = r.remaining();
        // v12 (patch 2.0.3+) widened the per-tile record to 8 bytes. A v11 file
        // always uses 7; the version decides, and the byte count confirms it.
        let tile_size = if version >= 12 {
            TILE_SIZE_V12
        } else {
            TILE_SIZE
        };
        if remaining < count * tile_size {
            return Err(FormatError::Corrupt("terrain tile data truncated"));
        }
        let mut tiles = Vec::with_capacity(count);
        let mut texture_indices = if version >= 12 {
            Vec::with_capacity(count)
        } else {
            Vec::new()
        };
        for _ in 0..count {
            let height_data = r.u16()?;
            let water_data = r.u16()?;
            let (texture, flags) = if version >= 12 {
                // u16 LE: bits 0-5 texture, bits 6-9 flags (ramp, blight,
                // water, boundary).
                let packed = r.u16()?;
                ((packed & 0x3F) as u8, ((packed >> 6) & 0x0F) as u8)
            } else {
                let packed = r.u8()?;
                (packed & 0x0F, packed >> 4)
            };
            let variation_data = r.u8()?;
            let cliff_data = r.u8()?;
            if version >= 12 {
                texture_indices.push(texture);
            }
            tiles.push(TerrainTile {
                height_data,
                water_data,
                // Legacy packed view: low nibble texture, high nibble flags.
                // Texture indices >= 16 are only available through
                // `Terrain::texture_index`.
                texture_flags: (texture & 0x0F) | (flags << 4),
                variation_data,
                cliff_data,
            });
        }
        let trailing_bytes = r.remaining();
        Ok(Self {
            version,
            tileset,
            custom_tileset,
            ground_tiles,
            cliff_tiles,
            width,
            height,
            center,
            tiles,
            texture_indices,
            trailing_bytes,
        })
    }

    pub fn tile(&self, x: u32, y: u32) -> Option<&TerrainTile> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.tiles
            .get(y as usize * self.width as usize + x as usize)
    }

    /// Full ground-texture index: 6 bits on v12+, the legacy 4-bit value on v11.
    pub fn texture_index(&self, x: u32, y: u32) -> Option<u8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let slot = y as usize * self.width as usize + x as usize;
        if self.texture_indices.is_empty() {
            self.tiles.get(slot).map(TerrainTile::texture)
        } else {
            self.texture_indices.get(slot).copied()
        }
    }

    pub fn height_at(&self, x: u32, y: u32) -> f32 {
        self.tile(x, y).map(TerrainTile::height).unwrap_or(0.0)
    }

    pub fn is_walkable(&self, x: u32, y: u32) -> bool {
        self.tile(x, y)
            .map(|t| !t.is_water() && !t.is_boundary())
            .unwrap_or(false)
    }

    pub fn world_size(&self) -> (f32, f32) {
        (
            (self.width.saturating_sub(1)) as f32 * 128.0,
            (self.height.saturating_sub(1)) as f32 * 128.0,
        )
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], FormatError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(FormatError::Corrupt("terrain offset overflow"))?;
        let slice = self
            .data
            .get(self.pos..end)
            .ok_or(FormatError::Corrupt("terrain read past end"))?;
        self.pos = end;
        Ok(slice)
    }
    /// Read exactly `N` bytes as an array (total: `first_chunk` cannot lie).
    fn arr<const N: usize>(&mut self) -> Result<[u8; N], FormatError> {
        let slice = self.bytes(N)?;
        slice
            .first_chunk::<N>()
            .copied()
            .ok_or(FormatError::Corrupt("terrain short read"))
    }
    fn u8(&mut self) -> Result<u8, FormatError> {
        Ok(self.arr::<1>()?[0])
    }
    fn u16(&mut self) -> Result<u16, FormatError> {
        Ok(u16::from_le_bytes(self.arr::<2>()?))
    }
    fn u32(&mut self) -> Result<u32, FormatError> {
        Ok(u32::from_le_bytes(self.arr::<4>()?))
    }
    fn f32(&mut self) -> Result<f32, FormatError> {
        Ok(f32::from_le_bytes(self.arr::<4>()?))
    }
    fn tag(&mut self) -> Result<[u8; 4], FormatError> {
        self.arr::<4>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> Option<Vec<u8>> {
        fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../analysis/war3_map/Human04/war3map.w3e"),
        )
        .ok()
    }

    #[test]
    fn parses_real_campaign_terrain() {
        let Some(bytes) = fixture() else {
            eprintln!("skipping: campaign terrain fixture missing");
            return;
        };
        let terrain = Terrain::parse(&bytes).expect("parse w3e");
        assert_eq!(terrain.version, 11);
        assert_eq!((terrain.width, terrain.height), (129, 129));
        assert_eq!(terrain.tiles.len(), 129 * 129);
        assert_eq!(terrain.tileset, b'V');
        assert!(terrain.tiles.iter().any(|t| t.is_water()));
        assert!(terrain.tiles.iter().any(|t| !t.is_water()));
        assert!(terrain.height_at(0, 0).abs() < 100.0);
    }

    /// v12 (patch 2.0.3+) packs texture+flags into a little-endian u16
    /// (War3Net `TerrainTile.cs:15-22`, HiveWE `terrain.ixx:415-430`):
    /// 6-bit texture, 4 flag bits starting at bit 6.
    #[test]
    fn parses_v12_tiles_with_extended_texture_index() {
        let mut b = Vec::new();
        b.extend_from_slice(b"W3E!");
        b.extend_from_slice(&12u32.to_le_bytes());
        b.push(b'L');
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(b"Ldrt");
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(b"CLdi");
        b.extend_from_slice(&2u32.to_le_bytes()); // tiles per row
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&0f32.to_le_bytes());
        b.extend_from_slice(&0f32.to_le_bytes());
        let tile = |texture: u16, flags: u16, variation: u8, cliff: u8| {
            let mut t = Vec::new();
            t.extend_from_slice(&8192u16.to_le_bytes()); // height 0.0
            t.extend_from_slice(&8192u16.to_le_bytes()); // water 0.0
            t.extend_from_slice(&(texture | (flags << 6)).to_le_bytes());
            t.push(variation);
            t.push(cliff);
            t
        };
        b.extend_from_slice(&tile(20, 0b0101, 0x1F, 0x21)); // texture 20, ramp+water
        b.extend_from_slice(&tile(5, 0, 0x00, 0x00));

        let terrain = Terrain::parse(&b).expect("parse v12 terrain");
        assert_eq!(terrain.version, 12);
        assert_eq!(terrain.tiles.len(), 2);
        assert_eq!(terrain.texture_indices, vec![20, 5]);
        assert_eq!(terrain.texture_index(0, 0), Some(20));
        assert_eq!(terrain.texture_index(1, 0), Some(5));
        // Legacy packed view truncates the index to 4 bits.
        assert_eq!(terrain.tiles[0].texture(), 4);
        assert!(terrain.tiles[0].is_ramp());
        assert!(terrain.tiles[0].is_water());
        assert!(!terrain.tiles[0].is_boundary());
        assert_eq!(terrain.tiles[0].variation(), 0x1F);
        assert_eq!(terrain.tiles[0].cliff_texture(), 2);
        assert_eq!(terrain.tiles[0].cliff_level(), 1);
        assert_eq!(terrain.trailing_bytes, 0);
        assert_eq!(terrain.world_size(), (128.0, 0.0));
    }

    /// Minimal well-formed terrain file for a given version: 1x1 map, one
    /// ground and one cliff texture, one tile of the version's record size.
    fn synthetic(version: u32) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"W3E!");
        b.extend_from_slice(&version.to_le_bytes());
        b.push(b'L');
        b.extend_from_slice(&0u32.to_le_bytes()); // custom tileset
        b.extend_from_slice(&1u32.to_le_bytes()); // ground texture count
        b.extend_from_slice(b"Ldrt");
        b.extend_from_slice(&1u32.to_le_bytes()); // cliff texture count
        b.extend_from_slice(b"CLdi");
        b.extend_from_slice(&1u32.to_le_bytes()); // width
        b.extend_from_slice(&1u32.to_le_bytes()); // height
        b.extend_from_slice(&0f32.to_le_bytes());
        b.extend_from_slice(&0f32.to_le_bytes());
        let tile = if version >= VERSION_MAX { TILE_SIZE_V12 } else { TILE_SIZE };
        b.extend(std::iter::repeat(0u8).take(tile));
        b
    }

    /// Canonical id: `w3e.version-bounds`.
    ///
    /// The classic engine accepts versions 3..=11 and rejects everything else
    /// (`0x6f3eb1fe` version < 3, `0x6f3eb207` `cmp eax, 0xb` version > 11).
    /// Version 12 is the Reforged layout: readable by the multi-era parser for
    /// 2.0.3+ maps, but not an input the classic engine can load.
    #[test]
    fn w3e_version_bounds() {
        for version in VERSION_MIN..=VERSION_MAX_ENGINE {
            let terrain = Terrain::parse_classic_engine(&synthetic(version))
                .unwrap_or_else(|err| panic!("engine must accept v{version}: {err}"));
            assert_eq!(terrain.version, version);
            assert_eq!(terrain.tiles.len(), 1);
        }
        for version in [0u32, 1, 2, 13, 99] {
            let bytes = synthetic(version.max(3));
            let mut spoiled = bytes.clone();
            spoiled[4..8].copy_from_slice(&version.to_le_bytes());
            assert!(
                matches!(
                    Terrain::parse_classic_engine(&spoiled),
                    Err(FormatError::Unsupported(_))
                ),
                "engine profile must reject v{version}"
            );
            assert!(
                matches!(Terrain::parse(&spoiled), Err(FormatError::Unsupported(_))),
                "multi-era parser must reject v{version}"
            );
        }
        // v12 is Reforged-only: the bounded multi-era parser reads it, the
        // engine profile refuses it.
        let reforged = synthetic(VERSION_MAX);
        assert_eq!(Terrain::parse(&reforged).expect("v12 parses").version, 12);
        assert!(matches!(
            Terrain::parse_classic_engine(&reforged),
            Err(FormatError::Unsupported(_))
        ));
    }

    #[test]
    fn rejects_bad_magic_and_size() {
        assert!(Terrain::parse(b"XXXX").is_err());
        let mut bytes = vec![0u8; 20];
        bytes[..4].copy_from_slice(b"W3E!");
        assert!(Terrain::parse(&bytes).is_err());
    }
}
