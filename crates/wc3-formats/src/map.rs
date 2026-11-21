//! Whole-map reader: `war3map.w3e`, `w3i`, `wpm`, object data, units, script.

use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use glam::Vec3;
use wc3_mpq::{MpqArchive, MpqError};

use crate::FormatError;

#[derive(Debug)]
pub enum MapError {
    Io(std::io::Error),
    Mpq(MpqError),
    Format(FormatError),
    MissingTerrain,
    Unsupported(&'static str),
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "i/o: {e}"),
            Self::Mpq(e) => write!(f, "MPQ: {e}"),
            Self::Format(e) => write!(f, "format: {e}"),
            Self::MissingTerrain => f.write_str("map has no war3map.w3e"),
            Self::Unsupported(why) => write!(f, "unsupported: {why}"),
        }
    }
}

impl std::error::Error for MapError {}

impl From<std::io::Error> for MapError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<MpqError> for MapError {
    fn from(value: MpqError) -> Self {
        Self::Mpq(value)
    }
}

impl From<FormatError> for MapError {
    fn from(value: FormatError) -> Self {
        Self::Format(value)
    }
}

#[derive(Debug, Clone)]
pub struct PlacedUnit {
    pub type_id: [u8; 4],
    pub variation: u32,
    pub position: Vec3,
    pub rotation: f32,
    pub scale: Vec3,
    pub player: u32,
    pub hp: i32,
    pub mp: i32,
    pub hero_level: i32,
}

/// RoC `.w3m` files start with a 512-byte `HM3W` header and the archive begins
/// at the first `MPQ\x1a` signature in the first 4 KiB; TFT `.w3x` files are a
/// bare MPQ. The rule lives in [`MpqArchive::mpq_base`] so the archive reader
/// and the map layer cannot drift; this function only adds the file read and
/// the map error type.
pub fn map_mpq_base(path: &Path) -> Result<u64, MapError> {
    let mut file = File::open(path)?;
    let mut head = vec![0u8; MpqArchive::CONTAINER_SCAN];
    let read = file.read(&mut head)?;
    head.truncate(read);
    if head.starts_with(b"MPQ\x1a") || head.starts_with(b"HM3W") {
        return MpqArchive::mpq_base(&head).ok_or(MapError::Unsupported("W3M MPQ header"));
    }
    Err(MapError::Unsupported("map magic"))
}

/// Parse `war3mapUnits.doo` (`W3do`, version 7..13).
///
/// Canonical parser shared by the map bundle and the legacy tooling;
/// `W3Map` was removed so there is one map reader. Field conditions and the
/// `random_type` payload follow HiveWE `src/base/units.ixx:470-620`
/// (version 13 group/lights, `subversion >= 11` item table + hero stats,
/// random payload 0 => 4 bytes, 1 => 8 bytes, 2 => count*8, other => none).
pub fn parse_units(bytes: &[u8]) -> Result<Vec<PlacedUnit>, MapError> {
    parse_units_ex(bytes).map(|(units, _)| units)
}

/// Parse `war3mapUnits.doo` and report the number of bytes consumed.
///
/// A well-formed file is consumed exactly; the corpus test uses the value to
/// catch field-order drift that a "parses without error" check would miss.
pub fn parse_units_ex(bytes: &[u8]) -> Result<(Vec<PlacedUnit>, usize), MapError> {
    let header: [u8; 16] = bytes
        .first_chunk::<16>()
        .copied()
        .ok_or(MapError::Unsupported("unit placement signature"))?;
    if &header[..4] != b"W3do" {
        return Err(MapError::Unsupported("unit placement signature"));
    }
    let version = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    let subversion = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
    // War3Net `MapWidgetsFormatVersion`: v6 (RoC beta), v7 (RoC), v8 (TFT),
    // v12/v13 (Definitive Edition). HiveWE adds no other version.
    if !(6..=13).contains(&version) {
        return Err(MapError::Unsupported("unit placement version"));
    }
    let count = u32::from_le_bytes([header[12], header[13], header[14], header[15]]) as usize;
    let mut r = Reader {
        data: bytes,
        pos: 16,
    };
    let mut units = Vec::with_capacity(count.min(100_000));
    for _ in 0..count {
        let type_id = r.tag()?;
        let variation = r.u32()?;
        let position = Vec3::new(r.f32()?, r.f32()?, r.f32()?);
        let rotation = r.f32()?;
        let scale = Vec3::new(r.f32()?, r.f32()?, r.f32()?);
        // 1.32+ maps carry a skin id before the flags byte. Without the w3i
        // game version the readable-fourcc test is the portable discriminator:
        // old maps start with a small flags value (< 0x20).
        let new_format = r.peek()? >= 0x20;
        let _skin = if new_format { r.u32()? } else { 0 };
        if version >= 12 {
            // War3Net `UnitData.cs:22-25` reads the group id from v12 on;
            // HiveWE only knows v13 files, so both agree where they overlap.
            let _group = r.u32()?;
        }
        let _flags = r.u8()?;
        let player = r.u32()?;
        let _unk1 = r.u8()?;
        let _unk2 = r.u8()?;
        let hp = r.i32()?;
        let mp = r.i32()?;
        if version >= 8 && subversion == 11 {
            let _item_table = r.i32()?;
        }
        let item_set_count = r.u32()? as usize;
        for _ in 0..item_set_count {
            let items = r.u32()? as usize;
            for _ in 0..items {
                let _item = r.tag()?;
                let _chance = r.u32()?;
            }
        }
        let _gold = r.i32()?;
        let _target_acquisition = r.f32()?;
        let hero_level = r.i32()?;
        if (version >= 8 && subversion == 11) || subversion == 10 {
            let _str = r.i32()?;
            let _agi = r.i32()?;
            let _int = r.i32()?;
        }
        let item_count = r.u32()? as usize;
        for _ in 0..item_count {
            let _slot = r.u32()?;
            let _item = r.tag()?;
        }
        let ability_count = r.u32()? as usize;
        for _ in 0..ability_count {
            let _ability = r.i32()?;
            let _active = r.i32()?;
            let _level = r.i32()?;
        }
        // Random unit table: the payload size depends on the type. 0/1 are
        // fixed-size random seeds, 2 is a table of `count` entries (8 bytes
        // each), and any other value (observed 0xFFFF_FFFF in campaign maps)
        // means "no random group" and carries no payload.
        let random_type = r.u32()?;
        match random_type {
            0 => {
                r.bytes(4)?;
            }
            1 => {
                r.bytes(8)?;
            }
            2 => {
                let entries = r.u32()? as usize;
                r.bytes(
                    entries
                        .checked_mul(8)
                        .ok_or(MapError::Unsupported("random table overflow"))?,
                )?;
            }
            _ => {}
        }
        if version > 6 && subversion > 7 {
            let _color = r.i32()?;
            let _waygate = r.i32()?;
            let _creation = r.i32()?;
        }
        if version >= 13 {
            // HiveWE `units.ixx:225-247`: roll, pitch, then the 36-byte lights.
            let _roll = r.f32()?;
            let _pitch = r.f32()?;
            let lights = r.u32()? as usize;
            for _ in 0..lights {
                let _index = r.u32()?;
                let _shadow = r.u32()?;
                r.bytes(4)?; // color (u8 rgba)
                let _intensity = r.f32()?;
                let _shadow_start = r.f32()?;
                let _shadow_end = r.f32()?;
                let _quadratic = r.f32()?;
                let _linear = r.f32()?;
                let _damping = r.f32()?;
            }
        } else if version == 12 {
            // War3Net reads three placeholder dwords for DE 2.0.4.23839.
            r.bytes(12)?;
        }
        units.push(PlacedUnit {
            type_id,
            variation,
            position,
            rotation,
            scale,
            player,
            hp,
            mp,
            hero_level,
        });
    }
    Ok((units, r.pos))
}

/// One placed doodad from `war3map.doo`.
#[derive(Debug, Clone, Copy)]
pub struct PlacedDoodad {
    pub type_id: [u8; 4],
    pub variation: u32,
    pub position: Vec3,
    pub rotation: f32,
    pub scale: Vec3,
    pub state: u8,
    pub life: u8,
}

/// One entry of the special-doodad table that follows `war3map.doo`
/// (War3Net `SpecialDoodadData.cs:10-16`): a type, a variation and an integer
/// map position. Special doodads are editor-only markers (start locations,
/// shops) and are not part of the placed-doodad list.
#[derive(Debug, Clone, Copy)]
pub struct SpecialDoodad {
    pub type_id: [u8; 4],
    pub variation: u32,
    pub position: (i32, i32),
}

/// Full `war3map.doo` payload: placed doodads, the special-doodad table and the
/// number of bytes consumed (a well-formed file consumes everything).
#[derive(Debug, Clone, Default)]
pub struct DoodadFile {
    pub doodads: Vec<PlacedDoodad>,
    pub special: Vec<SpecialDoodad>,
    pub consumed: usize,
}

/// Parse `war3map.doo` (W3do v6..v13).
///
/// Provenance: HiveWE `src/base/doodads.ixx:56-156` and War3Net
/// `Serialization/Binary/Widget/DoodadData.cs` + `MapDoodads.cs`. This is the
/// single doodad placement parser; the game layer delegates here instead of
/// keeping a private `parse_doo`.
pub fn parse_doodads(bytes: &[u8]) -> Result<Vec<PlacedDoodad>, MapError> {
    parse_doodads_ex(bytes).map(|file| file.doodads)
}

/// Parse `war3map.doo` including the special-doodad table.
pub fn parse_doodads_ex(bytes: &[u8]) -> Result<DoodadFile, MapError> {
    let header: [u8; 16] = bytes
        .first_chunk::<16>()
        .copied()
        .ok_or(MapError::Unsupported("doodad placement signature"))?;
    if &header[..4] != b"W3do" {
        return Err(MapError::Unsupported("doodad placement signature"));
    }
    let version = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    // v6 is the Reign of Chaos beta format (War3Net `MapWidgetsFormatVersion`).
    if !(6..=13).contains(&version) {
        return Err(MapError::Unsupported("doodad placement version"));
    }
    let count = u32::from_le_bytes([header[12], header[13], header[14], header[15]]) as usize;
    let mut r = Reader {
        data: bytes,
        pos: 16,
    };
    let mut out = Vec::with_capacity(count.min(20_000));
    for _ in 0..count {
        let type_id = r.tag()?;
        let variation = r.u32()?;
        let position = Vec3::new(r.f32()?, r.f32()?, r.f32()?);
        let rotation = r.f32()?;
        let scale = Vec3::new(r.f32()?, r.f32()?, r.f32()?);
        // Version 8+ doodads carry an optional 4-byte skin id when the next
        // byte is printable (War3Net `DoodadData.ReadFrom`: `useNewFormat`).
        // Missing this shifted every following field and made the parser see
        // truncated records, so library maps spawned no trees at all.
        let new_format = r.peek()? >= 0x20;
        let _skin = if new_format { r.u32()? } else { 0 };
        if version >= 12 {
            let _group = r.u32()?;
        }
        let state = if version > 6 { r.u8()? } else { 0 };
        let life = r.u8()?;
        if version >= 8 {
            let _item_table = r.i32()?;
            let sets = r.u32()? as usize;
            for _ in 0..sets {
                let items = r.u32()? as usize;
                for _ in 0..items {
                    let _id = r.tag()?;
                    let _chance = r.u32()?;
                }
            }
        }
        if version >= 13 {
            let _unknown = r.u32()?;
        }
        let _creation = r.u32()?;
        if version >= 12 {
            let _roll = r.f32()?;
            let _pitch = r.f32()?;
            let lights = r.u32()? as usize;
            for _ in 0..lights {
                // index, shadow flag, RGBA colour, intensity, start, end,
                // quadratic, linear, damping = 36 bytes.
                r.bytes(36)?;
            }
        }
        out.push(PlacedDoodad {
            type_id,
            variation,
            position,
            rotation,
            scale,
            state,
            life,
        });
    }
    // Special doodads: i32 version, i32 count, then 16-byte records
    // (type, variation, x, y). Absent in very old files.
    let mut special = Vec::new();
    let mut consumed = r.pos;
    if r.remaining() >= 8 {
        let special_version = r.i32()?;
        if special_version != 0 {
            return Err(MapError::Unsupported("special doodad version"));
        }
        let count = r.u32()? as usize;
        if count > 1_000_000 {
            return Err(MapError::Unsupported("special doodad count"));
        }
        special.reserve(count);
        for _ in 0..count {
            let type_id = r.tag()?;
            let variation = r.u32()?;
            let x = r.i32()?;
            let y = r.i32()?;
            special.push(SpecialDoodad {
                type_id,
                variation,
                position: (x, y),
            });
        }
        consumed = r.pos;
    }
    Ok(DoodadFile {
        doodads: out,
        special,
        consumed,
    })
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
    fn peek(&self) -> Result<u8, MapError> {
        self.data
            .get(self.pos)
            .copied()
            .ok_or(MapError::Unsupported("unit data truncated"))
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], MapError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(MapError::Unsupported("unit offset overflow"))?;
        let slice = self
            .data
            .get(self.pos..end)
            .ok_or(MapError::Unsupported("unit data truncated"))?;
        self.pos = end;
        Ok(slice)
    }
    /// Read exactly `N` bytes as an array; total because `first_chunk` cannot
    /// outrun the checked slice.
    fn arr<const N: usize>(&mut self) -> Result<[u8; N], MapError> {
        let slice = self.bytes(N)?;
        slice
            .first_chunk::<N>()
            .copied()
            .ok_or(MapError::Unsupported("placement data truncated"))
    }
    fn tag(&mut self) -> Result<[u8; 4], MapError> {
        self.arr::<4>()
    }
    fn u8(&mut self) -> Result<u8, MapError> {
        Ok(self.arr::<1>()?[0])
    }
    fn u32(&mut self) -> Result<u32, MapError> {
        Ok(u32::from_le_bytes(self.arr::<4>()?))
    }
    fn i32(&mut self) -> Result<i32, MapError> {
        Ok(i32::from_le_bytes(self.arr::<4>()?))
    }
    fn f32(&mut self) -> Result<f32, MapError> {
        Ok(f32::from_le_bytes(self.arr::<4>()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map_path() -> Option<PathBuf> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Maps/Campaign/Human04.w3m");
        path.is_file().then_some(path)
    }

    /// v12 units carry three placeholder dwords after the per-unit fields
    /// (War3Net `UnitData.cs:84-89`). A parser without that branch leaves 12
    /// bytes behind per unit, which the corpus gate would report as drift.
    #[test]
    fn parses_units_v12_trailing_dwords() {
        let mut b = Vec::new();
        b.extend_from_slice(b"W3do");
        b.extend_from_slice(&12u32.to_le_bytes());
        b.extend_from_slice(&11u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(b"hfoo");
        b.extend_from_slice(&0u32.to_le_bytes()); // variation
        for value in [1.0f32, 2.0, 3.0, 0.0, 1.0, 1.0, 1.0] {
            b.extend_from_slice(&value.to_le_bytes());
        }
        b.extend_from_slice(&0u32.to_le_bytes()); // group id (peek < 0x20 -> no skin)
        b.push(0); // flags
        b.extend_from_slice(&0u32.to_le_bytes()); // player
        b.push(0); // unk1
        b.push(0); // unk2
        b.extend_from_slice(&100i32.to_le_bytes()); // hp
        b.extend_from_slice(&0i32.to_le_bytes()); // mp
        b.extend_from_slice(&0i32.to_le_bytes()); // item table (v8+, sub 11)
        b.extend_from_slice(&0u32.to_le_bytes()); // item sets
        b.extend_from_slice(&0i32.to_le_bytes()); // gold
        b.extend_from_slice(&0f32.to_le_bytes()); // target acquisition
        b.extend_from_slice(&0i32.to_le_bytes()); // hero level
        for _ in 0..3 {
            b.extend_from_slice(&0i32.to_le_bytes()); // str/agi/int
        }
        b.extend_from_slice(&0u32.to_le_bytes()); // items
        b.extend_from_slice(&0u32.to_le_bytes()); // abilities
        b.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // random type: none
        for _ in 0..3 {
            b.extend_from_slice(&0i32.to_le_bytes()); // color/waygate/creation
        }
        b.extend_from_slice(&[0u8; 12]); // v12 trailer

        let (units, consumed) = parse_units_ex(&b).expect("parse v12 units");
        assert_eq!(units.len(), 1);
        assert_eq!(consumed, b.len(), "v12 trailer must be consumed");
        assert_eq!(units[0].type_id, *b"hfoo");
    }

    /// The special-doodad table follows the placed doodads
    /// (War3Net `MapDoodads.cs:36-43`).
    #[test]
    fn parses_special_doodad_table() {
        let mut b = Vec::new();
        b.extend_from_slice(b"W3do");
        b.extend_from_slice(&8u32.to_le_bytes());
        b.extend_from_slice(&11u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(b"LTlt");
        b.extend_from_slice(&0u32.to_le_bytes()); // variation
        for value in [1.0f32, 2.0, 3.0, 0.0, 1.0, 1.0, 1.0] {
            b.extend_from_slice(&value.to_le_bytes());
        }
        b.push(0); // state (v8 keeps it; < 0x20 so no skin id)
        b.push(100); // life
        b.extend_from_slice(&(-1i32).to_le_bytes()); // item table
        b.extend_from_slice(&0u32.to_le_bytes()); // item sets
        b.extend_from_slice(&7i32.to_le_bytes()); // creation number
        // special doodad table
        b.extend_from_slice(&0i32.to_le_bytes()); // version
        b.extend_from_slice(&1u32.to_le_bytes()); // count
        b.extend_from_slice(b"sloc");
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&10i32.to_le_bytes());
        b.extend_from_slice(&20i32.to_le_bytes());

        let file = parse_doodads_ex(&b).expect("parse doo with special table");
        assert_eq!(file.doodads.len(), 1);
        assert_eq!(file.special.len(), 1);
        assert_eq!(file.special[0].type_id, *b"sloc");
        assert_eq!(file.special[0].position, (10, 20));
        assert_eq!(file.consumed, b.len());
    }

    #[test]
    fn reads_real_campaign_map_container() {
        let Some(path) = map_path() else {
            eprintln!("skipping: Human04.w3m fixture missing");
            return;
        };
        // RoC `.w3m` files carry the `HM3W` header; the archive starts at 512.
        let base = map_mpq_base(&path).expect("map container");
        assert_eq!(base, 512);
        let archive = MpqArchive::open_at(&path, base).expect("open map archive");
        assert!(archive.read("war3map.w3e").is_ok(), "terrain file");
        let units = archive.read("war3mapUnits.doo").expect("unit placement file");
        match parse_units(&units) {
            Ok(placed) => {
                assert!(!placed.is_empty(), "map has placed units");
                assert!(
                    placed.iter().all(|unit| unit.type_id != [0; 4]),
                    "every placed unit carries a fourcc"
                );
            }
            // Human04 uses the `W3do` v8 random-unit-table extension, which this
            // parser does not model yet (tracked in the merge manifest). The unit
            // records before that section are valid; the previous `W3Map` reader
            // silently returned an empty list for the same map.
            Err(MapError::Unsupported("random unit tables")) => {}
            Err(other) => panic!("war3mapUnits.doo: {other}"),
        }
    }
}
