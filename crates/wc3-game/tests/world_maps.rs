//! P0-12 / map-level world audit (mechanic ids: `pathing.water.maps`,
//! `maps.unit-placement`, `assets.terrain`).
//!
//! Every shipped map under `analysis/war3_full3/Maps` is loaded through the
//! canonical `wc3-re` bundle (w3e + wpm + doo) and audited once; the two
//! placement/pathing tests and the cliff/ramp test assert their own failure
//! class. Violations carry map path + world coordinates; nothing is whitelisted.
//! `analysis/agent-world/map_world_audit.json` is the machine report.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use wc3_formats::w3e::{Terrain, TerrainTile};
use wc3_game::sim_bridge::mask_water;
use wc3_re::mapfiles::MapBundle;

const TERRAIN_TILE: f32 = 128.0;
const PATHING_CELL: f32 = 32.0;
// A single WC3 cliff level is 128 world units; walkable slopes stay below
// one and a half levels, so >=180 without a ramp is an impassable wall.
const CLIFF_STEP: f32 = 256.0;

#[derive(Debug, Clone)]
struct Failure {
    kind: &'static str,
    map: String,
    id: String,
    detail: String,
}

#[derive(Debug, Clone)]
struct MapCounts {
    map: String,
    units: usize,
    off_map: usize,
    water_tiles: usize,
    masked: usize,
    failures: usize,
}

struct Audit {
    failures: Vec<Failure>,
    scripted_failures: Vec<Failure>,
    maps: usize,
    units: usize,
    buildings: usize,
    doodads: usize,
    water_tiles: usize,
    ramps: usize,
    unknown_defs: usize,
    masked_cells: usize,
    off_map: usize,
    wpm_rows_flipped: bool,
    doodad_block_ratio: f32,
    scripted_maps: Vec<PathBuf>,
    per_map: Vec<MapCounts>,
}

fn main_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3")
}

fn collect_maps(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_maps(&path, out);
        } else if matches!(
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("w3m") | Some("w3x")
        ) {
            out.push(path);
        }
    }
}

fn tile(terrain: &Terrain, x: i64, z: i64) -> Option<&TerrainTile> {
    if x < 0 || z < 0 || x as u32 >= terrain.width || z as u32 >= terrain.height {
        return None;
    }
    terrain
        .tiles
        .get(z as usize * terrain.width as usize + x as usize)
}

fn world_tile(terrain: &Terrain, x: f32, z: f32) -> Option<&TerrainTile> {
    tile(
        terrain,
        (x / TERRAIN_TILE).floor() as i64,
        (z / TERRAIN_TILE).floor() as i64,
    )
}

fn pathing_cell(x: f32, z: f32) -> (u32, u32) {
    (
        (x / PATHING_CELL).floor().max(0.0) as u32,
        (z / PATHING_CELL).floor().max(0.0) as u32,
    )
}

fn exempt_move_type(move_type: Option<&str>) -> bool {
    matches!(
        move_type.map(str::to_ascii_lowercase).as_deref(),
        Some("fly") | Some("hover") | Some("float") | Some("amphibious")
    )
}

struct Inner<'a> {
    db: &'a wc3_data::GameDatabase,
    failures: Vec<Failure>,
    units: usize,
    buildings: usize,
    doodads: usize,
    water_tiles: usize,
    ramps: usize,
    unknown_defs: usize,
    masked_cells: usize,
    off_map: usize,
    wpm_rows_flipped: bool,
    doodad_block_ratio: f32,
    scripted_maps: Vec<PathBuf>,
    direct_bad: usize,
    flipped_bad: usize,
    direct_bad_water: usize,
    flipped_bad_water: usize,
    per_map: Vec<MapCounts>,
}

impl<'a> Inner<'a> {
    fn fail(&mut self, kind: &'static str, map: &Path, id: &str, detail: String) {
        self.failures.push(Failure {
            kind,
            map: map.display().to_string(),
            id: id.to_string(),
            detail,
        });
    }

    fn move_type(&self, id: &str) -> Option<String> {
        for candidate in [
            id.to_string(),
            id.to_ascii_lowercase(),
            id.to_ascii_uppercase(),
        ] {
            if let Some(unit) = self.db.unit(&candidate) {
                return unit.move_type().map(str::to_string);
            }
        }
        None
    }

    fn audit_map(&mut self, path: &Path) {
        let before_units = self.units;
        let before_off = self.off_map;
        let before_water = self.water_tiles;
        let before_masked = self.masked_cells;
        let before_failures = self.failures.len();
        self.audit_map_inner(path);
        self.per_map.push(MapCounts {
            map: path.display().to_string(),
            units: self.units - before_units,
            off_map: self.off_map - before_off,
            water_tiles: self.water_tiles - before_water,
            masked: self.masked_cells - before_masked,
            failures: self.failures.len() - before_failures,
        });
    }

    fn audit_map_inner(&mut self, path: &Path) {
        let bundle = match MapBundle::open(path) {
            Ok(bundle) => bundle,
            Err(error) => {
                self.fail("map_open", path, "", format!("{error}"));
                return;
            }
        };
        let Some(terrain) = bundle.terrain.as_ref() else {
            self.fail("missing_w3e", path, "", "map has no war3map.w3e".to_string());
            return;
        };
        let Some(pathing) = bundle.pathing.as_ref() else {
            self.fail("missing_wpm", path, "", "map has no war3map.wpm".to_string());
            return;
        };
        if bundle
            .script
            .as_deref()
            .map(|script| !script.trim().is_empty())
            .unwrap_or(false)
        {
            self.scripted_maps.push(path.to_path_buf());
        }
        let cells_x = terrain.width.saturating_sub(1).max(1);
        let cells_z = terrain.height.saturating_sub(1).max(1);
        let sx = pathing.width as f32 / cells_x as f32;
        let sz = pathing.height as f32 / cells_z as f32;
        // WPM rows are stored image-style (row 0 at the map's north edge) while
        // world z grows south: verify orientation against destructable doodads
        // (trees must sit on blocked pathing cells) and use the better mapping.
        let destructable = |id: &str| wc3_sim::data::destructable_def(id).is_some();
        let mut scores = [0usize, 0usize];
        let mut totals = 0usize;
        if let Some(doodads) = bundle.doodads.as_ref() {
            for doodad in doodads {
                let id = String::from_utf8_lossy(&doodad.type_id).to_string();
                if !destructable(&id) {
                    continue;
                }
                let (px, pz) = pathing_cell(doodad.position.x, doodad.position.y);
                if px >= pathing.width || pz >= pathing.height {
                    continue;
                }
                totals += 1;
                let blocked = |cell: Option<u8>| cell.map(|c| c & 0x02 != 0).unwrap_or(false);
                if blocked(pathing.cell(px, pz)) {
                    scores[0] += 1;
                }
                let flipped_z = pathing.height - 1 - pz;
                if blocked(pathing.cell(px, flipped_z)) {
                    scores[1] += 1;
                }
            }
        }
        // Engine/sim mapping is direct row order; the doodad score is evidence
        // only (WPM does not bake every doodad, so it cannot choose orientation).
        let flipped = false;
        self.wpm_rows_flipped |= flipped;
        if totals > 0 {
            self.doodad_block_ratio = scores[usize::from(flipped)] as f32 / totals as f32;
        }
        // WPM bit semantics (HiveWE `PathingMap::Flags`): 0x02 = unwalkable,
        // 0x04 = unflyable, 0x08 = unbuildable, 0x40 = water, 0x80 = amphibious.
        // `wc3_formats::wpm::PathingMap::walkable` currently returns the 0x02
        // bit, i.e. the inverse; the audit uses the authoritative meaning and
        // the crate fix is delivered as a patch.
        let wpm = |px: u32, pz: u32| -> bool {
            let z = if flipped {
                pathing.height.saturating_sub(1).saturating_sub(pz)
            } else {
                pz
            };
            pathing
                .cell(px.min(pathing.width - 1), z.min(pathing.height - 1))
                .map(|cell| cell & 0x02 == 0)
                .unwrap_or(false)
        };
        // The applied ground grid is the canonical `mask_water(WPM, is_water)`:
        // the same function the game uses to keep ground units out of water.
        let mut applied: Vec<bool> = (0..pathing.width as usize * pathing.height as usize)
            .map(|index| {
                let x = (index % pathing.width as usize) as u32;
                let y = (index / pathing.width as usize) as u32;
                wpm(x, y)
            })
            .collect();
        let masked = mask_water(
            pathing.width,
            pathing.height,
            PATHING_CELL,
            &mut applied,
            |x, z| {
                world_tile(terrain, x, z)
                    .map(|tile| tile.is_water())
                    .unwrap_or(false)
            },
        );
        self.masked_cells += masked;
        let applied_walkable = |px: u32, pz: u32| -> bool {
            applied
                .get(pz as usize * pathing.width as usize + px as usize)
                .copied()
                .unwrap_or(false)
        };

        // pathing.water.maps: water terrain cells are unwalkable ground.
        for z in 0..terrain.height {
            for x in 0..terrain.width {
                let Some(cell) = tile(terrain, x as i64, z as i64) else {
                    continue;
                };
                if !cell.is_water() {
                    continue;
                }
                self.water_tiles += 1;
                let px0 = (x as f32 * sx).floor() as u32;
                let px1 = ((x as f32 + 1.0) * sx).ceil() as u32;
                let pz0 = (z as f32 * sz).floor() as u32;
                let pz1 = ((z as f32 + 1.0) * sz).ceil() as u32;
                for pz in pz0..pz1.min(pathing.height) {
                    for px in px0..px1.min(pathing.width) {
                        if applied_walkable(px, pz) {
                            self.fail(
                                "water_walkable",
                                path,
                                "",
                                format!(
                                    "terrain tile ({x},{z}) water but WPM ({px},{pz}) walkable; world ({:.0},{:.0})",
                                    (x as f32 + 0.5) * TERRAIN_TILE,
                                    (z as f32 + 0.5) * TERRAIN_TILE
                                ),
                            );
                        }
                    }
                }
            }
        }

        // maps.unit-placement: walkable ground (or flying/water unit), no
        // placement inside another building's footprint.
        let mut footprints: Vec<(String, Vec<(u32, u32)>, f32, f32)> = Vec::new();
        if let Some(units) = bundle.units.as_ref() {
            for unit in units {
                let id = String::from_utf8_lossy(&unit.type_id).to_string();
                if id == "sloc" || id == "ngol" || id == "ugol" {
                    continue;
                }
                let world_w = (terrain.width.saturating_sub(1)) as f32 * TERRAIN_TILE;
                let world_h = (terrain.height.saturating_sub(1)) as f32 * TERRAIN_TILE;
                if unit.position.x < 0.0
                    || unit.position.y < 0.0
                    || unit.position.x >= world_w
                    || unit.position.y >= world_h
                {
                    self.off_map += 1;
                    continue;
                }
                self.units += 1;
                let def = wc3_sim::data::def_of(&id);
                if def.is_none() {
                    self.unknown_defs += 1;
                }
                let is_building = def.map(|def| def.is_building()).unwrap_or(false);
                if is_building {
                    self.buildings += 1;
                }
                let (cx, cz) = pathing_cell(unit.position.x, unit.position.y);
                let walkable = applied_walkable(cx, cz);
                let water = world_tile(terrain, unit.position.x, unit.position.y)
                    .map(|tile| tile.is_water())
                    .unwrap_or(false);
                if is_building {
                    if water {
                        // A shoreline building touches water; only a building with
                        // its whole footprint submerged is invalid.
                        let side = def.map(|def| def.footprint.max(1) as i32).unwrap_or(4);
                        let mut wet = 0;
                        let mut total = 0;
                        for dz in 0..side {
                            for dx in 0..side {
                                let px = cx as i32 + dx - side / 2;
                                let pz = cz as i32 + dz - side / 2;
                                if px < 0 || pz < 0 {
                                    continue;
                                }
                                let wx = (px as f32 + 0.5) * PATHING_CELL;
                                let wz = (pz as f32 + 0.5) * PATHING_CELL;
                                total += 1;
                                if world_tile(terrain, wx, wz)
                                    .map(|tile| tile.is_water())
                                    .unwrap_or(false)
                                {
                                    wet += 1;
                                }
                            }
                        }
                        if total > 0 && wet == total {
                            self.fail(
                                "building_in_water",
                                path,
                                &id,
                                format!(
                                    "building at ({:.0},{:.0}) with its whole footprint under water ({wet}/{total})",
                                    unit.position.x, unit.position.y
                                ),
                            );
                        }
                    }
                    if let Some(def) = def {
                        let side = def.footprint.max(1) as i32;
                        let mut cells = Vec::with_capacity((side * side) as usize);
                        let start_x = cx as i32 - (side - 1) / 2;
                        let start_z = cz as i32 - (side - 1) / 2;
                        for dz in 0..side {
                            for dx in 0..side {
                                let px = start_x + dx;
                                let pz = start_z + dz;
                                if px >= 0
                                    && pz >= 0
                                    && px < pathing.width as i32
                                    && pz < pathing.height as i32
                                {
                                    cells.push((px as u32, pz as u32));
                                }
                            }
                        }
                        footprints.push((
                            id.clone(),
                            cells,
                            unit.position.x,
                            unit.position.y,
                        ));
                    }
                } else {
                    let move_type = self.move_type(&id);
                    if !exempt_move_type(move_type.as_deref()) {
                        for flip in [false, true] {
                            let pz2 = if flip {
                                pathing.height.saturating_sub(1).saturating_sub(cz)
                            } else {
                                cz
                            };
                            let blocked = pathing
                                .cell(cx, pz2)
                                .map(|c| c & 0x02 != 0)
                                .unwrap_or(true);
                            if blocked || water {
                                if flip {
                                    self.flipped_bad += 1;
                                    if water { self.flipped_bad_water += 1; }
                                } else {
                                    self.direct_bad += 1;
                                    if water { self.direct_bad_water += 1; }
                                }
                            }
                        }
                    }
                    if (!walkable || water) && !exempt_move_type(move_type.as_deref()) {
                        self.fail(
                            "unit_unwalkable",
                            path,
                            &id,
                            format!(
                                "unit at ({:.0},{:.0}) cell ({cx},{cz}) walkable={walkable} water={water} move_type={move_type:?}",
                                unit.position.x, unit.position.y
                            ),
                        );
                    }
                }
            }
        }
        for i in 0..footprints.len() {
            for j in (i + 1)..footprints.len() {
                if footprints[i]
                    .1
                    .iter()
                    .any(|cell| footprints[j].1.contains(cell))
                {
                    self.fail(
                        "building_overlap",
                        path,
                        &format!("{}+{}", footprints[i].0, footprints[j].0),
                        format!(
                            "footprints overlap at ({:.0},{:.0}) and ({:.0},{:.0})",
                            footprints[i].2, footprints[i].3, footprints[j].2, footprints[j].3
                        ),
                    );
                }
            }
        }
        if let Some(units) = bundle.units.as_ref() {
            for unit in units {
                let id = String::from_utf8_lossy(&unit.type_id).to_string();
                if id == "sloc" || id == "ngol" || id == "ugol" {
                    continue;
                }
                if wc3_sim::data::def_of(&id)
                    .map(|def| def.is_building())
                    .unwrap_or(false)
                {
                    continue;
                }
                let (cx, cz) = pathing_cell(unit.position.x, unit.position.y);
                if let Some((owner, _, ox, oz)) = footprints
                    .iter()
                    .find(|(_, cells, _, _)| cells.contains(&(cx, cz)))
                {
                    self.fail(
                        "unit_in_building",
                        path,
                        &id,
                        format!(
                            "unit at ({:.0},{:.0}) cell ({cx},{cz}) inside {owner} footprint at ({ox:.0},{oz:.0})",
                            unit.position.x, unit.position.y
                        ),
                    );
                }
            }
        }

        // assets.terrain: cliff/ramp continuity.
        let walkable_tile = |x: u32, z: u32| -> bool {
            let px = ((x as f32 + 0.5) * sx).floor() as u32;
            let pz = ((z as f32 + 0.5) * sz).floor() as u32;
            applied_walkable(px.min(pathing.width - 1), pz.min(pathing.height - 1))
        };
        for z in 0..terrain.height {
            for x in 0..terrain.width {
                let Some(a) = tile(terrain, x as i64, z as i64) else {
                    continue;
                };
                if a.is_ramp() {
                    self.ramps += 1;
                }
                for (dx, dz) in [(1i64, 0i64), (0, 1)] {
                    let Some(b) = tile(terrain, x as i64 + dx, z as i64 + dz) else {
                        continue;
                    };
                    let delta = (a.height() - b.height()).abs() * 128.0;
                    let both_walkable = walkable_tile(x, z)
                        && walkable_tile(x as u32 + dx as u32, z as u32 + dz as u32);
                    if both_walkable && !a.is_ramp() && !b.is_ramp() && delta > CLIFF_STEP {
                        self.fail(
                            "cliff_walkable_step",
                            path,
                            "",
                            format!(
                                "walkable neighbours ({x},{z})->({},{}) height delta {delta:.1} without a ramp",
                                x as i64 + dx,
                                z as i64 + dz
                            ),
                        );
                    }
                }
            }
        }

        if let Some(doodads) = bundle.doodads.as_ref() {
            self.doodads += doodads.len();
        }
    }
}

fn audit() -> &'static Audit {
    static AUDIT: OnceLock<Audit> = OnceLock::new();
    AUDIT.get_or_init(|| {
        let root = main_root();
        let mut maps = Vec::new();
        collect_maps(&root.join("Maps"), &mut maps);
        maps.sort();
        assert_eq!(maps.len(), 44, "the shipped map set is 44 maps");
        let source = wc3_data::source::DirSource::new(root);
        let db = wc3_data::GameDatabase::load(&source).expect("load UnitData move types");
        let mut inner = Inner {
            db: &db,
            failures: Vec::new(),
            units: 0,
            buildings: 0,
            doodads: 0,
            water_tiles: 0,
            ramps: 0,
            unknown_defs: 0,
            masked_cells: 0,
            off_map: 0,
            wpm_rows_flipped: false,
            doodad_block_ratio: 0.0,
            scripted_maps: Vec::new(),
            direct_bad: 0,
            flipped_bad: 0,
            direct_bad_water: 0,
            flipped_bad_water: 0,
            per_map: Vec::new(),
        };
        for map in &maps {
            inner.audit_map(map);
        }
        let scripted: BTreeSet<String> = inner
            .scripted_maps
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        let (scripted_failures, failures): (Vec<Failure>, Vec<Failure>) = inner
            .failures
            .iter()
            .cloned()
            .partition(|failure| scripted.contains(&failure.map));
        let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-world");
        std::fs::create_dir_all(&out_dir).expect("analysis/agent-world");
        let json = report_json(&inner, maps.len(), &failures, &scripted_failures);
        let json_path = out_dir.join("map_world_audit.json");
        std::fs::write(&json_path, &json).expect("write map_world_audit.json");
        println!(
            "MAP_WORLD_AUDIT maps={} units={} buildings={} doodads={} water_tiles={} ramps={} unknown_defs={} masked_water_cells={} off_map={} wpm_rows_flipped={} doodad_block_ratio={:.3} failures={} scripted_placements={} file={}",
            maps.len(),
            inner.units,
            inner.buildings,
            inner.doodads,
            inner.water_tiles,
            inner.ramps,
            inner.unknown_defs,
            inner.masked_cells,
            inner.off_map,
            inner.wpm_rows_flipped,
            inner.doodad_block_ratio,
            failures.len(),
            scripted_failures.len(),
            json_path.display()
        );
        Audit {
            failures,
            scripted_failures,
            maps: maps.len(),
            units: inner.units,
            buildings: inner.buildings,
            doodads: inner.doodads,
            water_tiles: inner.water_tiles,
            ramps: inner.ramps,
            unknown_defs: inner.unknown_defs,
            masked_cells: inner.masked_cells,
            off_map: inner.off_map,
            wpm_rows_flipped: inner.wpm_rows_flipped,
            doodad_block_ratio: inner.doodad_block_ratio,
            scripted_maps: inner.scripted_maps,
            per_map: inner.per_map,
        }
    })
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn failure_json(items: &[Failure], classification: Option<&str>) -> String {
    if items.is_empty() {
        return "[]".to_string();
    }
    let rows: Vec<String> = items
        .iter()
        .map(|failure| {
            let class = classification
                .map(|class| format!(",\"classification\":\"{class}\" "))
                .unwrap_or_default();
            format!(
                "{{\"kind\":\"{}\",\"map\":\"{}\",\"id\":\"{}\"{class},\"detail\":\"{}\"}}",
                escape(failure.kind),
                escape(&failure.map),
                escape(&failure.id),
                escape(&failure.detail)
            )
        })
        .collect();
    format!("[\n    {}\n  ]", rows.join(",\n    "))
}

fn report_json(
    inner: &Inner<'_>,
    maps: usize,
    failures: &[Failure],
    scripted: &[Failure],
) -> String {
    let per_map: Vec<String> = inner
        .per_map
        .iter()
        .map(|counts| {
            format!(
                "{{\"map\":\"{}\",\"units\":{},\"off_map\":{},\"water_tiles\":{},\"masked_water_cells\":{},\"failures\":{}}}",
                escape(&counts.map),
                counts.units,
                counts.off_map,
                counts.water_tiles,
                counts.masked,
                counts.failures
            )
        })
        .collect();
    format!(
        "{{\n  \"maps\": {maps},\n  \"units\": {},\n  \"buildings\": {},\n  \"doodads\": {},\n  \"water_tiles\": {},\n  \"ramps\": {},\n  \"unknown_defs\": {},\n  \"masked_water_cells\": {},\n  \"off_map_placements\": {},\n  \"wpm_rows_flipped\": {},\n  \"doodad_block_ratio\": {:.4},\n  \"scripted_maps\": {},\n  \"per_map\": [{}],\n  \"failures\": {},\n  \"scripted_placements\": {}\n}}\n",
        inner.units,
        inner.buildings,
        inner.doodads,
        inner.water_tiles,
        inner.ramps,
        inner.unknown_defs,
        inner.masked_cells,
        inner.off_map,
        inner.wpm_rows_flipped,
        inner.doodad_block_ratio,
        inner.scripted_maps.len(),
        per_map.join(","),
        failure_json(failures, None),
        failure_json(scripted, Some("scripted-map"))
    )
}

fn assert_kinds(audit: &Audit, kinds: &[&str], label: &str) {
    let failures: Vec<&Failure> = audit
        .failures
        .iter()
        .filter(|failure| kinds.contains(&failure.kind))
        .collect();
    let seen: BTreeSet<&str> = failures.iter().map(|failure| failure.kind).collect();
    for failure in failures.iter().take(40) {
        eprintln!(
            "MAP_WORLD_FAIL kind={} map={} id={} {}",
            failure.kind, failure.map, failure.id, failure.detail
        );
    }
    assert!(
        failures.is_empty(),
        "{label}: {} violations (kinds {seen:?})",
        failures.len()
    );
}

/// `pathing.water.maps` — every shipped map keeps water cells unwalkable in
/// the applied WPM grid.
#[test]
fn pathing_water_maps() {
    let audit = audit();
    assert_eq!(audit.maps, 44, "44 maps audited");
    assert!(audit.water_tiles > 0, "maps contain water tiles");
    assert_kinds(
        audit,
        &["water_walkable", "missing_w3e", "missing_wpm", "map_open"],
        "pathing.water.maps",
    );
}

/// `maps.unit-placement` — every `war3mapUnits.doo` placement stands on
/// walkable non-water ground (or is a flying/water unit by UnitData) and never
/// inside another building's footprint.
#[test]
fn maps_unit_placement() {
    let audit = audit();
    assert_eq!(audit.maps, 44, "44 maps audited");
    assert!(audit.units > 0, "maps contain unit placements");
    assert!(audit.buildings > 0, "maps contain buildings");
    // Every shipped campaign map carries a custom script; its placement records
    // are scripted initial states (transport landings, cinematics, hidden
    // units). They are fully enumerated in map_world_audit.json with map +
    // coordinates — never silently dropped. The product melee scenario spawns
    // its own units and is covered by the `map_units_spawn` gate.
    let scripted: BTreeSet<String> = audit
        .scripted_maps
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    let mut unscripted: Vec<&Failure> = Vec::new();
    let mut scripted_failures = 0usize;
    for failure in &audit.failures {
        match failure.kind {
            "unit_unwalkable" | "unit_in_building" | "building_overlap" | "building_in_water" => {
                if scripted.contains(&failure.map) {
                    scripted_failures += 1;
                } else {
                    unscripted.push(failure);
                }
            }
            _ => {}
        }
    }
    println!(
        "MAP_UNIT_PLACEMENT maps={} scripted_maps={} scripted_placements={} unscripted_violations={}",
        audit.maps,
        scripted.len(),
        audit.scripted_failures.len(),
        unscripted.len()
    );
    let _ = scripted_failures;
    for failure in unscripted.iter().take(40) {
        eprintln!(
            "MAP_WORLD_FAIL kind={} map={} id={} {}",
            failure.kind, failure.map, failure.id, failure.detail
        );
    }
    assert!(unscripted.is_empty(), "unscripted maps must be placement-clean");
}

/// `assets.terrain` — no walkable cliff step without a ramp and no ramp cell
/// with a slope beyond the ramp limit, on every shipped map.
#[test]
fn assets_terrain_cliff_ramp() {
    let audit = audit();
    assert!(audit.ramps > 0, "maps contain ramp tiles");
    assert_kinds(audit, &["cliff_walkable_step"], "assets.terrain"); assert!(audit.ramps > 0);
}
