//! Map content extraction: starts, mines, shops, creeps, units, doodads,
//! regions, cameras and sounds for every shipped map.
//!
//! The library melee map `(2)Ponds_v1.4.w3x` is the acceptance fixture:
//! 2 starts, 6 gold mines, 124 units, 5 767 doodads.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use wc3_data::GameDatabase;
use wc3_mpq::MpqArchive;

use crate::creeps::PLAYER_NEUTRAL_AGGRESSIVE;
use crate::error::{ReError, Result};
use crate::mapfiles::{MapBundle, discover_maps};
use crate::manifest::json_str;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StartLocation {
    pub player: u32,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub fixed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MapContent {
    pub path: String,
    pub name: String,
    pub starts: Vec<StartLocation>,
    pub gold_mines: usize,
    pub shops: usize,
    pub creeps: usize,
    pub units: usize,
    pub doodads: usize,
    pub regions: usize,
    pub cameras: usize,
    pub sounds: usize,
    pub upgrades: usize,
}

impl MapContent {
    pub fn to_json(&self) -> String {
        let starts = self
            .starts
            .iter()
            .map(|start| {
                format!(
                    "{{\"player\":{},\"name\":{},\"x\":{},\"y\":{},\"fixed\":{}}}",
                    start.player,
                    json_str(&start.name),
                    start.x,
                    start.y,
                    start.fixed
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"path\":{},\"name\":{},\"starts\":[{}],\"gold_mines\":{},\"shops\":{},\"creeps\":{},\"units\":{},\"doodads\":{},\"regions\":{},\"cameras\":{},\"sounds\":{},\"upgrades\":{}}}",
            json_str(&self.path),
            json_str(&self.name),
            starts,
            self.gold_mines,
            self.shops,
            self.creeps,
            self.units,
            self.doodads,
            self.regions,
            self.cameras,
            self.sounds,
            self.upgrades
        )
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn u32(&mut self) -> Option<u32> {
        let end = self.pos.checked_add(4)?;
        let bytes = self.bytes.get(self.pos..end)?;
        self.pos = end;
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn f32(&mut self) -> Option<f32> {
        self.u32().map(f32::from_bits)
    }

    fn cstr(&mut self) -> Option<String> {
        let rest = self.bytes.get(self.pos..)?;
        let end = rest.iter().position(|byte| *byte == 0)?;
        let text = String::from_utf8_lossy(&rest[..end]).to_string();
        self.pos += end + 1;
        Some(text)
    }

    fn skip(&mut self, count: usize) -> Option<()> {
        self.pos = self.pos.checked_add(count)?;
        (self.pos <= self.bytes.len()).then_some(())
    }
}

/// Parse the W3I player block (`raw_tail` after the four map strings).
///
/// `raw_tail` still contains camera bounds/complements, playable area, flags,
/// loading-screen and environment blocks before the player array; the skip
/// follows War3Net `MapInfo.ReadFrom` for the map's format version.
pub fn parse_start_locations(raw_tail: &[u8], version: u32) -> Vec<StartLocation> {
    let mut reader = Reader::new(raw_tail);
    // camera bounds (8 floats)
    if reader.skip(32).is_none() {
        return Vec::new();
    }
    if version >= 15 && reader.skip(16).is_none() {
        return Vec::new();
    }
    // playable area width/height, flags, tileset
    if reader.skip(8 + 4 + 1).is_none() {
        return Vec::new();
    }
    if version >= 23 {
        // loading screen background + path, text/title/subtitle
        if reader.skip(4).is_none() || reader.cstr().is_none() {
            return Vec::new();
        }
        for _ in 0..3 {
            if reader.cstr().is_none() {
                return Vec::new();
            }
        }
        // game data set + prologue path, text/title/subtitle
        if reader.skip(4).is_none() {
            return Vec::new();
        }
        for _ in 0..4 {
            if reader.cstr().is_none() {
                return Vec::new();
            }
        }
        // fog style + 3 floats + color
        if reader.skip(4 + 12 + 4).is_none() {
            return Vec::new();
        }
        if version >= 25 && reader.skip(4).is_none() {
            return Vec::new();
        }
        // sound environment, light environment, water tint
        if reader.cstr().is_none() || reader.skip(1 + 4).is_none() {
            return Vec::new();
        }
        if version >= 28 && reader.skip(4).is_none() {
            return Vec::new();
        }
        if version >= 31 && reader.skip(8).is_none() {
            return Vec::new();
        }
    } else if version >= 18 {
        // campaign background, loading text/title, loading number, prologue text/title
        if reader.skip(4).is_none() {
            return Vec::new();
        }
        for _ in 0..2 {
            if reader.cstr().is_none() {
                return Vec::new();
            }
        }
        if reader.skip(4).is_none() {
            return Vec::new();
        }
        for _ in 0..3 {
            if reader.cstr().is_none() {
                return Vec::new();
            }
        }
    } else if version >= 15 {
        for _ in 0..7 {
            if reader.cstr().is_none() {
                return Vec::new();
            }
        }
    } else {
        for _ in 0..4 {
            if reader.cstr().is_none() {
                return Vec::new();
            }
        }
    }
    let Some(count) = reader.u32() else {
        return Vec::new();
    };
    if count > 64 {
        return Vec::new();
    }
    let mut starts = Vec::new();
    for _ in 0..count {
        let Some(number) = reader.u32() else { break };
        let Some(_player_type) = reader.u32() else { break };
        let Some(_race) = reader.u32() else { break };
        let Some(fixed) = reader.u32() else { break };
        let Some(name) = reader.cstr() else { break };
        let Some(x) = reader.f32() else { break };
        let Some(y) = reader.f32() else { break };
        // ally low/high, plus enemy low/high from v31
        if reader.u32().is_none() || reader.u32().is_none() {
            break;
        }
        if version >= 31 && (reader.u32().is_none() || reader.u32().is_none()) {
            break;
        }
        starts.push(StartLocation {
            player: number,
            name,
            x,
            y,
            fixed: fixed != 0,
        });
    }
    starts
}

fn unit_id(unit: &wc3_formats::map::PlacedUnit) -> String {
    crate::fourcc_str(&unit.type_id)
}

pub fn inspect(path: &Path, db: Option<&GameDatabase>) -> Result<MapContent> {
    let bundle = MapBundle::open(path)?;
    let shop_ids: BTreeSet<String> = db
        .map(|db| db.shops().into_iter().map(|shop| shop.unit_id).collect())
        .unwrap_or_default();
    let starts = bundle
        .info
        .as_ref()
        .map(|info| parse_start_locations(&info.raw_tail, info.version))
        .unwrap_or_default();
    let units = bundle.units.as_deref().unwrap_or_default();
    let gold_mines = units
        .iter()
        .filter(|unit| unit.type_id == crate::fourcc("ngol"))
        .count();
    let shops = units
        .iter()
        .filter(|unit| shop_ids.contains(&unit_id(unit)))
        .count();
    let creeps = units
        .iter()
        .filter(|unit| unit.player == PLAYER_NEUTRAL_AGGRESSIVE)
        .count();
    let base = wc3_formats::map::map_mpq_base(path)
        .map_err(|error| ReError::Parse(format!("map container: {error}")))?;
    let archive = MpqArchive::open_at(path, base)?;
    let read = |name: &str| archive.read(name).ok();
    let cameras = read("war3map.w3c")
        .and_then(|bytes| crate::cutscenes::parse_cameras(&bytes).ok())
        .map_or(0, |rows| rows.len());
    let regions = read("war3map.w3r")
        .and_then(|bytes| crate::cutscenes::parse_regions(&bytes).ok())
        .map_or(0, |rows| rows.len());
    let sounds = read("war3map.w3s")
        .and_then(|bytes| crate::cutscenes::parse_sound_count(&bytes).ok())
        .unwrap_or(0);
    let name = bundle
        .info
        .as_ref()
        .map(|info| info.name.clone())
        .unwrap_or_default();
    Ok(MapContent {
        path: path.display().to_string(),
        name,
        starts,
        gold_mines,
        shops,
        creeps,
        units: units.len(),
        doodads: bundle.doodads.as_ref().map_or(0, Vec::len),
        regions,
        cameras,
        sounds,
        upgrades: bundle
            .objects
            .get("war3map.w3q")
            .map_or(0, |objects| objects.base_entries.len() + objects.new_entries.len()),
    })
}

#[derive(Debug, Clone, Default)]
pub struct MapContentReport {
    pub maps: Vec<MapContent>,
}

impl MapContentReport {
    pub fn to_json(&self) -> String {
        let maps = self
            .maps
            .iter()
            .map(MapContent::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!("{{\"schema\":\"wc3-map-content-v1\",\"maps\":[{maps}]}}")
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# MAP CONTENT — starts, mines, shops, creeps, objects");
        let _ = writeln!(out, "\n| map | starts | mines | shops | creeps | units | doodads | regions | cameras | sounds |");
        let _ = writeln!(out, "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
        for map in &self.maps {
            let _ = writeln!(
                out,
                "| `{}` | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                map.path,
                map.starts.len(),
                map.gold_mines,
                map.shops,
                map.creeps,
                map.units,
                map.doodads,
                map.regions,
                map.cameras,
                map.sounds
            );
        }
        out
    }
}

pub fn all(root: &Path) -> Result<MapContentReport> {
    let maps_root = root.join("analysis/war3_full3/Maps");
    let maps = discover_maps(&maps_root)?;
    let db = crate::catalog::database(root)?;
    let mut report = MapContentReport::default();
    for map in &maps {
        report.maps.push(inspect(map, Some(&db))?);
    }
    Ok(report)
}

pub fn write_artifacts(report: &MapContentReport, json: &Path, md: &Path) -> Result<()> {
    if let Some(parent) = json.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(json, report.to_json())?;
    std::fs::write(md, report.to_markdown())?;
    Ok(())
}

pub fn ponds_path(root: &Path) -> PathBuf {
    root.join("donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/(2)Ponds_v1.4.w3x")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapcontent_melee_ponds_2_starts_6_mines_124_units_5767_doodads() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let ponds = ponds_path(&root);
        assert!(ponds.is_file(), "Ponds map missing at {}", ponds.display());
        let db = crate::catalog::database(&root).expect("data database");
        let content = inspect(&ponds, Some(&db)).expect("Ponds content");
        assert_eq!(content.starts.len(), 2, "starts: {:?}", content.starts);
        assert_eq!(content.gold_mines, 6, "gold mines");
        assert_eq!(content.units, 124, "units");
        assert_eq!(content.doodads, 5_767, "doodads");
        assert_eq!(content.creeps, 0, "creeps (Ponds has no neutral-aggressive placements)");
        let campaign = root.join("analysis/war3_full3/Maps");
        if campaign.is_dir() {
            let report = all(&root).expect("all maps content");
            assert_eq!(report.maps.len(), 44, "campaign maps");
            assert!(report.maps.iter().all(|map| map.starts.len() <= 24));
        }
    }
}
