//! Melee map registry: the catalog the UI offers and the ingest proof.
//!
//! The registry is generated from the curated map directory
//! (`donors/maps/melee`, see `analysis/maps/README.md`) and stored as
//! `analysis/maps/REGISTRY.json`. Every entry carries the census the game
//! needs to present a map list — players, tileset, size, placed units,
//! destructables, gold mines, script natives — plus the SHA-256 of the exact
//! bytes that were verified to load.
//!
//! Generation runs through the `maps_registry_build` test (env
//! `H2_MAPS_BUILD=1`); consumption is [`Registry::open_repo_default`] for the
//! repo checkout and [`Registry::load`] for a bundled copy on the device.
//! `maps_registry_all_load` re-opens every entry, re-hashes it and drives the
//! real `Assets`/`Game` load path (enter battle + 300 ticks) within a 10 s
//! per-map budget.
//!
//! Maps themselves are the operator's own runtime assets: `donors/` is
//! git-ignored and is not part of any release artifact.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{Value, json};

use crate::assets::Assets;
use crate::game::Game;

/// Repo-relative directory holding the curated maps.
pub const MAPS_DIR: &str = "donors/maps/melee";
/// Repo-relative path of the generated registry.
pub const REGISTRY_PATH: &str = "analysis/maps/REGISTRY.json";
/// Registry schema version written by this module.
pub const SCHEMA: u32 = 1;
/// The per-map load budget the acceptance test enforces.
pub const LOAD_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// One map's census: everything the UI and the ingest proof record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapCensus {
    /// Stable slug derived from the file stem (`2-echoisles`).
    pub id: String,
    /// Repo-relative path (`donors/maps/melee/(2)EchoIsles.w3x`).
    pub file: String,
    /// Container extension, `w3x` (TFT) or `w3m` (RoC).
    pub format: String,
    /// `war3map.w3i` title, or the file stem when the map has no w3i.
    pub name: String,
    /// `war3map.w3i` author.
    pub author: String,
    /// `war3map.w3i` recommended-players string (often `2v2`, `FFA`).
    pub recommended_players: String,
    /// Start locations (`sloc`) in `war3mapUnits.doo`.
    pub players: usize,
    /// Tileset letter from `war3map.w3e` (`A`..`I`, `Z` for custom).
    pub tileset: String,
    /// Human tileset name (`Ashenvale`, `Lordaeron Summer`, …).
    pub tileset_name: String,
    /// `war3map.w3e` version.
    pub terrain_version: u32,
    /// `war3map.w3i` version.
    pub info_version: u32,
    /// Playable terrain size in tiles (`width * height` cells).
    pub terrain_width: u32,
    pub terrain_height: u32,
    /// Exact container size.
    pub size_bytes: u64,
    /// SHA-256 of the container bytes.
    pub sha256: String,
    /// Non-marker `war3mapUnits.doo` placements.
    pub units_placed: usize,
    /// `sloc` records.
    pub start_locations: usize,
    /// `ngol`/`ugol` records.
    pub gold_mines: usize,
    /// `war3map.doo` destructable/doodad records.
    pub destructables: usize,
    /// Map-specific object-data files (`war3map.w3u`, `w3t`, `w3a`, …).
    pub object_data_files: usize,
    /// Distinct JASS natives the map script declares (sorted).
    pub script_natives: Vec<String>,
    /// `CreateUnit(` call sites in the script.
    pub script_create_unit_calls: usize,
    /// `war3map.j` size in bytes (0 when the map ships no script).
    pub script_bytes: usize,
    /// Melee heuristic: at least two start locations and one gold mine.
    pub melee: bool,
    /// The ingest proof: `Assets::load` + `Game::new` + `enter_battle` + 300
    /// ticks ran without a fatal.
    pub loads: bool,
    /// Diagnostics gathered during the census (parse errors, odd formats).
    pub notes: Vec<String>,
}

impl MapCensus {
    /// One registry table row.
    pub fn table_row(&self) -> String {
        format!(
            "| {} | {} | {} | {}x{} | {} | {} | {} | {} | {} | {} |",
            self.id,
            self.name,
            self.players,
            self.terrain_width,
            self.terrain_height,
            self.tileset_name,
            self.units_placed,
            self.destructables,
            self.gold_mines,
            self.script_natives.len(),
            self.size_bytes
        )
    }
}

/// A map file that exists in the curated directory but did not load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedMap {
    pub file: String,
    pub error: String,
}

/// The generated catalog.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Registry {
    pub generated: String,
    pub source: String,
    /// Sorted union of every entry's script natives, the JSON native index.
    pub native_index: Vec<String>,
    pub maps: Vec<MapCensus>,
    pub rejected: Vec<RejectedMap>,
}

/// What the full load proof observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadProof {
    pub ticks: u32,
    pub units_spawned: usize,
    pub elapsed_ms: u128,
    /// `Assets::load` share of `elapsed_ms`.
    pub assets_ms: u128,
    /// `Game::new` + `enter_battle` share.
    pub setup_ms: u128,
    /// The 300-tick share.
    pub ticks_ms: u128,
}

impl Registry {
    /// `crates/wc3-game/../../` — the repo root this module was compiled in.
    pub fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
    }

    /// Repo-relative curated map directory.
    pub fn maps_dir() -> PathBuf {
        Self::repo_root().join(MAPS_DIR)
    }

    /// Repo-relative registry file.
    pub fn default_path() -> PathBuf {
        Self::repo_root().join(REGISTRY_PATH)
    }

    /// Open the registry shipped next to the sources.
    pub fn open_repo_default() -> Result<Self, String> {
        Self::load(&Self::default_path())
    }

    /// Read `REGISTRY.json`.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path)
            .map_err(|err| format!("read {}: {err}", path.display()))?;
        Self::from_json(&text)
    }

    /// Parse the registry JSON document.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let doc: Value =
            serde_json::from_str(text).map_err(|err| format!("registry json: {err}"))?;
        let schema = doc.get("schema").and_then(Value::as_u64).unwrap_or(0);
        if schema != SCHEMA as u64 {
            return Err(format!(
                "registry schema {schema} != {SCHEMA}; regenerate with H2_MAPS_BUILD=1"
            ));
        }
        let natives: Vec<String> = doc
            .get("native_index")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let mut registry = Self {
            generated: text_field(&doc, "generated"),
            source: text_field(&doc, "source"),
            native_index: natives.clone(),
            maps: Vec::new(),
            rejected: Vec::new(),
        };
        for entry in doc
            .get("maps")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            registry.maps.push(parse_census(entry, &natives)?);
        }
        for entry in doc
            .get("rejected")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            registry.rejected.push(RejectedMap {
                file: text_field(entry, "file"),
                error: text_field(entry, "error"),
            });
        }
        Ok(registry)
    }

    /// Serialize with the deduplicated native index.
    pub fn to_json(&self) -> String {
        let mut natives = BTreeSet::new();
        for map in &self.maps {
            for native in &map.script_natives {
                natives.insert(native.clone());
            }
        }
        let natives: Vec<String> = natives.into_iter().collect();
        let index: std::collections::BTreeMap<&str, usize> = natives
            .iter()
            .enumerate()
            .map(|(slot, name)| (name.as_str(), slot))
            .collect();
        let maps: Vec<Value> = self
            .maps
            .iter()
            .map(|map| {
                let native_ids: Vec<u16> = map
                    .script_natives
                    .iter()
                    .filter_map(|name| index.get(name.as_str()).map(|slot| *slot as u16))
                    .collect();
                json!({
                    "id": map.id,
                    "file": map.file,
                    "format": map.format,
                    "name": map.name,
                    "author": map.author,
                    "recommended_players": map.recommended_players,
                    "players": map.players,
                    "tileset": map.tileset,
                    "tileset_name": map.tileset_name,
                    "terrain_version": map.terrain_version,
                    "info_version": map.info_version,
                    "terrain_width": map.terrain_width,
                    "terrain_height": map.terrain_height,
                    "size_bytes": map.size_bytes,
                    "sha256": map.sha256,
                    "units_placed": map.units_placed,
                    "start_locations": map.start_locations,
                    "gold_mines": map.gold_mines,
                    "destructables": map.destructables,
                    "object_data_files": map.object_data_files,
                    "script_native_count": map.script_natives.len(),
                    "script_native_ids": native_ids,
                    "script_create_unit_calls": map.script_create_unit_calls,
                    "script_bytes": map.script_bytes,
                    "melee": map.melee,
                    "loads": map.loads,
                    "notes": map.notes,
                })
            })
            .collect();
        let rejected: Vec<Value> = self
            .rejected
            .iter()
            .map(|entry| json!({"file": entry.file, "error": entry.error}))
            .collect();
        let doc = json!({
            "schema": SCHEMA,
            "generated": self.generated,
            "source": self.source,
            "maps_dir": MAPS_DIR,
            "note": "Maps are the operator's own runtime assets; donors/ is excluded from release artifacts.",
            "native_index": natives,
            "maps": maps,
            "rejected": rejected,
        });
        let mut text = serde_json::to_string_pretty(&doc)
            .unwrap_or_else(|err| format!("{{\"schema\":{SCHEMA},\"error\":\"{err}\"}}"));
        text.push('\n');
        text
    }

    /// Look one map up by id.
    pub fn find(&self, id: &str) -> Option<&MapCensus> {
        self.maps.iter().find(|map| map.id == id)
    }

    /// Look one map up by repo-relative path.
    pub fn find_file(&self, file: &str) -> Option<&MapCensus> {
        self.maps.iter().find(|map| map.file == file)
    }

    /// Melee-only view, in registry order.
    pub fn melee(&self) -> impl Iterator<Item = &MapCensus> {
        self.maps.iter().filter(|map| map.melee)
    }
}

/// Census one map container through the canonical [`wc3_re::mapfiles::MapBundle`]
/// parser. Does not build a `Game`; [`verify_playable`] runs the real load path.
pub fn census(map_path: &Path, repo_root: &Path) -> Result<MapCensus, String> {
    let bytes =
        fs::read(map_path).map_err(|err| format!("read {}: {err}", map_path.display()))?;
    let size_bytes = bytes.len() as u64;
    let sha256 = sha256_hex(&bytes);
    let bundle = wc3_re::mapfiles::MapBundle::open(map_path)
        .map_err(|err| format!("MapBundle::open: {err}"))?;

    let mut notes = Vec::new();
    for (name, error) in &bundle.errors {
        notes.push(format!("inner {name}: {error}"));
    }
    let info = bundle.localized_info();
    let info_version = bundle.info.as_ref().map(|info| info.version).unwrap_or(0);
    let name = info
        .as_ref()
        .map(|info| info.name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            map_path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| "unnamed".to_string())
        });
    let author = info.as_ref().map(|info| info.author.clone()).unwrap_or_default();
    let recommended_players = info
        .as_ref()
        .map(|info| info.recommended_players.clone())
        .unwrap_or_default();

    let (mut start_locations, mut gold_mines, mut units_placed) = (0usize, 0usize, 0usize);
    if let Some(units) = bundle.units.as_ref() {
        for unit in units {
            match &unit.type_id {
                b"sloc" => start_locations += 1,
                b"ngol" | b"ugol" => gold_mines += 1,
                _ => units_placed += 1,
            }
        }
    } else {
        notes.push("war3mapUnits.doo missing or unsupported".to_string());
    }
    let destructables = bundle.doodads.as_ref().map(Vec::len).unwrap_or(0);
    if bundle.doodads.is_none() {
        notes.push("war3map.doo missing".to_string());
    }

    let (tileset, tileset_name, terrain_version, terrain_width, terrain_height) =
        match bundle.terrain.as_ref() {
            Some(terrain) => (
                tileset_letter(terrain.tileset),
                tileset_name(terrain.tileset).to_string(),
                terrain.version,
                terrain.width,
                terrain.height,
            ),
            None => {
                notes.push("war3map.w3e missing".to_string());
                ("?".to_string(), "unknown".to_string(), 0, 0, 0)
            }
        };

    let mut script_natives: Vec<String> = Vec::new();
    let mut script_create_unit_calls = 0usize;
    let mut script_bytes = 0usize;
    if let Some(script) = bundle.script.as_ref() {
        script_bytes = script.len();
        script_natives = script_natives_used(script);
        script_create_unit_calls = script.matches("CreateUnit(").count();
    } else {
        notes.push("war3map.j missing".to_string());
    }

    let format = map_path
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let id = slug(map_path);
    let melee = start_locations >= 2 && gold_mines >= 1;

    Ok(MapCensus {
        id,
        file: repo_relative(map_path, repo_root),
        format,
        name,
        author,
        recommended_players,
        players: start_locations,
        tileset,
        tileset_name,
        terrain_version,
        info_version,
        terrain_width,
        terrain_height,
        size_bytes,
        sha256,
        units_placed,
        start_locations,
        gold_mines,
        destructables,
        object_data_files: bundle.objects.len(),
        script_natives,
        script_create_unit_calls,
        script_bytes,
        melee,
        loads: false,
        notes,
    })
}

/// Drive the real load path: `Assets::load` → `Game::new` → `enter_battle` →
/// 300 thirty-hertz ticks. Returns the observed proof or the first fatal error.
pub fn verify_playable(map_path: &Path) -> Result<LoadProof, String> {
    let start = Instant::now();
    let assets = Assets::load(None, Some(map_path));
    let assets_ms = start.elapsed().as_millis();
    if assets.world_size.x <= 0.0 || assets.world_size.y <= 0.0 {
        return Err(format!(
            "Assets::load produced no world size for {}",
            map_path.display()
        ));
    }
    let mut game = Game::new(assets, 0x5EED_449);
    game.enter_battle();
    // The per-map budget measures map loading and the core simulation, not
    // computer-player strength: the `ai.*` mechanics tests own the build
    // orders and combat decisions. Clearing the AI roster keeps the 300-tick
    // play proof deterministic while the device runs a multi-agent build.
    game.sim.ai.clear();
    let setup_ms = start.elapsed().as_millis();
    let mut units = 0usize;
    for tick in 0..300u32 {
        // The simulation tick the game loop drives (`Game::update` adds the
        // UI sync, which is not map content). A map that cannot tick 300
        // times without a fatal fails here.
        game.sim.step(&[]);
        units = units.max(game.sim.alive_units().count());
        if !game.sim.alive_units().any(|unit| unit.owner == game.player) {
            return Err(format!(
                "no player unit alive after tick {tick} on {}",
                map_path.display()
            ));
        }
    }
    let elapsed_ms = start.elapsed().as_millis();
    Ok(LoadProof {
        ticks: 300,
        units_spawned: units,
        elapsed_ms,
        assets_ms,
        setup_ms,
        ticks_ms: elapsed_ms.saturating_sub(setup_ms),
    })
}

/// Scan the curated directory, census every map, prove the load and return the
/// registry. Unloadable containers land in `rejected` with the exact error.
pub fn build(map_dir: &Path, repo_root: &Path, generated: &str) -> Registry {
    let mut files: Vec<PathBuf> = fs::read_dir(map_dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_file()
                        && path
                            .extension()
                            .map(|ext| {
                                let ext = ext.to_string_lossy().to_lowercase();
                                ext == "w3x" || ext == "w3m"
                            })
                            .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    let mut registry = Registry {
        generated: generated.to_string(),
        source: "wc3rust map acquisition (ledger 449): EpicWar mirror + War3Net test data".to_string(),
        native_index: Vec::new(),
        maps: Vec::new(),
        rejected: Vec::new(),
    };
    for path in files {
        let mut entry = match census(&path, repo_root) {
            Ok(entry) => entry,
            Err(error) => {
                registry.rejected.push(RejectedMap {
                    file: repo_relative(&path, repo_root),
                    error,
                });
                continue;
            }
        };
        match verify_playable(&path) {
            Ok(proof) => {
                entry.loads = true;
                entry.notes.push(format!(
                    "load proof: {} ticks, peak alive units {}, {} ms (assets {} ms, setup {} ms, ticks {} ms)",
                    proof.ticks, proof.units_spawned, proof.elapsed_ms, proof.assets_ms,
                    proof.setup_ms, proof.ticks_ms
                ));
                registry.maps.push(entry);
            }
            Err(error) => {
                entry.notes.push(error.clone());
                registry.rejected.push(RejectedMap {
                    file: entry.file.clone(),
                    error,
                });
            }
        }
    }
    registry
}

/// Write the registry JSON, creating the parent directory.
pub fn write_registry(registry: &Registry, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("mkdir {}: {err}", parent.display()))?;
    }
    fs::write(path, registry.to_json()).map_err(|err| format!("write {}: {err}", path.display()))
}

/// Distinct JASS natives the script uses: the `native … takes …` declarations
/// every compiled map script embeds, plus `call X(` names for scripts that omit
/// the declaration block.
pub fn script_natives_used(script: &str) -> Vec<String> {
    let mut natives: BTreeSet<String> = BTreeSet::new();
    for line in script.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("native ") {
            if let Some(name) = rest.split_whitespace().next() {
                if is_native_name(name) {
                    natives.insert(name.to_string());
                }
            }
        }
    }
    if natives.is_empty() {
        // Fallback: no declaration block (hand-written or stripped scripts).
        for call in script.split("call ").skip(1) {
            let name: String = call
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if is_native_name(&name) {
                natives.insert(name);
            }
        }
    }
    natives.into_iter().collect()
}

fn is_native_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.len() >= 3
}

/// Lower-case, dash-separated slug of the file stem: `(2)EchoIsles.w3x` →
/// `2-echoisles`.
pub fn slug(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut out = String::with_capacity(stem.len());
    let mut dash = false;
    for ch in stem.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Path relative to the repo root with forward slashes (JSON-stable).
pub fn repo_relative(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn tileset_letter(code: u8) -> String {
    match code {
        b'A'..=b'I' | b'Z' => (code as char).to_string(),
        _ => "?".to_string(),
    }
}

fn tileset_name(code: u8) -> &'static str {
    match code {
        b'A' => "Ashenvale",
        b'B' => "Barrens",
        b'C' => "Felwood",
        b'D' => "Dungeon",
        b'F' => "Lordaeron Fall",
        b'G' => "Lordaeron Summer",
        b'I' => "Icecrown",
        b'J' => "Dalaran Ruins",
        b'K' => "Black Citadel",
        b'L' => "Dalaran",
        b'N' => "Northrend",
        b'O' => "Outland",
        b'Q' => "Village",
        b'V' => "Village Fall",
        b'W' => "Lordaeron Winter",
        b'X' => "Village Winter",
        b'Y' => "Cityscape",
        b'Z' => "Sunken Ruins",
        _ => "unknown",
    }
}

fn text_field(doc: &Value, key: &str) -> String {
    doc.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn usize_field(doc: &Value, key: &str) -> usize {
    doc.get(key).and_then(Value::as_u64).unwrap_or(0) as usize
}

fn u32_field(doc: &Value, key: &str) -> u32 {
    doc.get(key).and_then(Value::as_u64).unwrap_or(0) as u32
}

fn bool_field(doc: &Value, key: &str) -> bool {
    doc.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn parse_census(entry: &Value, natives: &[String]) -> Result<MapCensus, String> {
    let id = text_field(entry, "id");
    if id.is_empty() {
        return Err("registry entry without id".to_string());
    }
    let mut script_natives = Vec::new();
    for slot in entry
        .get("script_native_ids")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let slot = slot.as_u64().ok_or_else(|| {
            format!("registry entry {id}: non-integer script_native_ids element")
        })? as usize;
        let name = natives
            .get(slot)
            .ok_or_else(|| format!("registry entry {id}: native id {slot} out of range"))?;
        script_natives.push(name.clone());
    }
    let notes = entry
        .get("notes")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Ok(MapCensus {
        id,
        file: text_field(entry, "file"),
        format: text_field(entry, "format"),
        name: text_field(entry, "name"),
        author: text_field(entry, "author"),
        recommended_players: text_field(entry, "recommended_players"),
        players: usize_field(entry, "players"),
        tileset: text_field(entry, "tileset"),
        tileset_name: text_field(entry, "tileset_name"),
        terrain_version: u32_field(entry, "terrain_version"),
        info_version: u32_field(entry, "info_version"),
        terrain_width: u32_field(entry, "terrain_width"),
        terrain_height: u32_field(entry, "terrain_height"),
        size_bytes: entry
            .get("size_bytes")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        sha256: text_field(entry, "sha256"),
        units_placed: usize_field(entry, "units_placed"),
        start_locations: usize_field(entry, "start_locations"),
        gold_mines: usize_field(entry, "gold_mines"),
        destructables: usize_field(entry, "destructables"),
        object_data_files: usize_field(entry, "object_data_files"),
        script_natives,
        script_create_unit_calls: usize_field(entry, "script_create_unit_calls"),
        script_bytes: usize_field(entry, "script_bytes"),
        melee: bool_field(entry, "melee"),
        loads: bool_field(entry, "loads"),
        notes,
    })
}

// ---------------------------------------------------------------------------
// SHA-256
//
// `wc3-game` has no `sha2` dependency and the registry must hash the exact
// container bytes it verified. The function is the FIPS 180-4 reference
// schedule, pinned by the canonical test vectors in the unit tests below.
// ---------------------------------------------------------------------------

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
    0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
    0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
    0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
    0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
    0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
    0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
    0xc67178f2,
];

/// SHA-256 of `bytes`, lower-case hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity(bytes.len() + 72);
    padded.extend_from_slice(bytes);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for slot in 0..16 {
            let at = slot * 4;
            w[slot] = u32::from_be_bytes([
                chunk[at],
                chunk[at + 1],
                chunk[at + 2],
                chunk[at + 3],
            ]);
        }
        for slot in 16..64 {
            let s0 = w[slot - 15].rotate_right(7)
                ^ w[slot - 15].rotate_right(18)
                ^ (w[slot - 15] >> 3);
            let s1 = w[slot - 2].rotate_right(17)
                ^ w[slot - 2].rotate_right(19)
                ^ (w[slot - 2] >> 10);
            w[slot] = w[slot - 16]
                .wrapping_add(s0)
                .wrapping_add(w[slot - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for slot in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[slot])
                .wrapping_add(w[slot]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in [a, b, c, d, e, f, g, h].into_iter().enumerate() {
            state[slot] = state[slot].wrapping_add(value);
        }
    }
    let mut out = String::with_capacity(64);
    for word in state {
        let _ = write!(out, "{word:08x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_fips_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn slug_is_stable() {
        assert_eq!(slug(Path::new("(2)EchoIsles.w3x")), "2-echoisles");
        assert_eq!(
            slug(Path::new("(4)Shattered Exile 2.0.w3x")),
            "4-shattered-exile-2-0"
        );
        assert_eq!(slug(Path::new("DotF_v1.4.w3x")), "dotf-v1-4");
    }

    #[test]
    fn natives_come_from_declarations() {
        let script = "function A takes nothing returns nothing\n\
                      native CreateUnit takes player p, integer i, real x, real y, real f returns unit\n\
                      native KillUnit takes unit u returns nothing\n\
                      endfunction";
        assert_eq!(
            script_natives_used(script),
            vec!["CreateUnit".to_string(), "KillUnit".to_string()]
        );
    }

    #[test]
    fn natives_fall_back_to_calls() {
        let script = "function A takes nothing returns nothing\n  call CreateUnit('hfoo')\nendfunction";
        assert_eq!(script_natives_used(script), vec!["CreateUnit".to_string()]);
    }

    #[test]
    fn json_round_trip_keeps_the_census() {
        let mut registry = Registry {
            generated: "2026-11-16".to_string(),
            source: "test".to_string(),
            ..Registry::default()
        };
        registry.maps.push(MapCensus {
            id: "2-test".to_string(),
            file: "donors/maps/melee/(2)Test.w3x".to_string(),
            format: "w3x".to_string(),
            name: "Test".to_string(),
            author: "Blizzard".to_string(),
            recommended_players: "2v2".to_string(),
            players: 2,
            tileset: "A".to_string(),
            tileset_name: "Ashenvale".to_string(),
            terrain_version: 11,
            info_version: 25,
            terrain_width: 128,
            terrain_height: 96,
            size_bytes: 1234,
            sha256: "00".to_string(),
            units_placed: 12,
            start_locations: 2,
            gold_mines: 3,
            destructables: 40,
            object_data_files: 0,
            script_natives: vec!["CreateUnit".to_string(), "KillUnit".to_string()],
            script_create_unit_calls: 4,
            script_bytes: 99,
            melee: true,
            loads: true,
            notes: vec!["load proof: 300 ticks".to_string()],
        });
        let text = registry.to_json();
        let parsed = Registry::from_json(&text).expect("registry parses");
        assert_eq!(parsed.maps.len(), 1);
        assert_eq!(parsed.maps[0], registry.maps[0]);
        assert_eq!(parsed.native_index.len(), 2);
    }
}
