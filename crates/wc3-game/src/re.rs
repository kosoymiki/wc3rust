//! Canonical RE / resource-access bridge for the game layer.
//!
//! The playable slice loads MDX/BLP through [`crate::assets`], but the
//! reverse-engineering layer owns the multi-archive Storm view, the composed
//! object database, the TriggerData registry, the map bundle (including
//! `war3map.wts` localization) and the Game.dll boot model. This module makes
//! those capabilities reachable from `wc3-game` through one explicit entry
//! point instead of re-deriving them in game code.
//!
//! It is additive: [`crate::assets`] keeps its existing fast path for the four
//! starter models and the terrain atlas, while `ReLayer` is the authoritative
//! source for object data, icons, map metadata and script execution.

use std::path::{Path, PathBuf};

use wc3_re::engine::{Engine, EngineConfig};
use wc3_re::error::ReError;
use wc3_re::icons::{self, IconAudit};
use wc3_re::mapfiles::MapBundle;
use wc3_re::objectdata::{ObjectDb, ObjectRecord};
use wc3_re::storm::StormFs;
use wc3_re::trigger::TriggerData;
use wc3_data::{GameDatabase, ObjectInfo};

/// One opened install/map view backed by the canonical `wc3-re` layer.
pub struct ReLayer {
    pub install_root: PathBuf,
    pub storm: StormFs,
    pub objects: Option<ObjectDb>,
    /// Typed database over the same parsed layers as `objects` (one lookup path
    /// for the HUD: name/tooltip/icon/model/stats).
    pub typed: Option<GameDatabase>,
    pub triggers: Option<TriggerData>,
    pub map: Option<MapBundle>,
    /// Human-readable load notes; also used by tests/diagnostics.
    pub notes: Vec<String>,
}

impl ReLayer {
    /// Open an extracted install (or archives, in game priority order) and an
    /// optional map. Missing pieces are reported in `notes`, never fatal: the
    /// game still runs with procedural fallbacks.
    pub fn open(install_root: impl AsRef<Path>, map_path: Option<&Path>) -> Self {
        let install_root = install_root.as_ref().to_path_buf();
        let mut notes = Vec::new();
        let mut storm = StormFs::open_wc3_install(&install_root).unwrap_or_else(|error| {
            notes.push(format!("storm_archives_failed={error}"));
            StormFs::new()
        });
        if storm.listfile().is_empty() && install_root.is_dir() {
            match storm.add_loose_dir("install", &install_root) {
                Ok(count) => notes.push(format!("storm_loose_files={count}")),
                Err(error) => notes.push(format!("storm_loose_failed={error}")),
            }
        }
        let mut objects = match ObjectDb::load_storm(&storm) {
            Ok(db) => {
                notes.push(format!(
                    "objects={} sheets={} metadata={}",
                    db.objects.len(),
                    db.sheets.len(),
                    db.metadata.len()
                ));
                Some(db)
            }
            Err(error) => {
                notes.push(format!("objects_failed={error}"));
                None
            }
        };
        let ui_dir = install_root.join("UI");
        let triggers = if ui_dir.is_dir() {
            match TriggerData::load_dir(&ui_dir) {
                Ok(table) => {
                    notes.push(format!(
                        "trigger_actions={} calls={}",
                        table.actions.len(),
                        table.calls.len()
                    ));
                    Some(table)
                }
                Err(error) => {
                    notes.push(format!("trigger_failed={error}"));
                    None
                }
            }
        } else {
            None
        };
        let map = map_path.and_then(|path| match MapBundle::open(path) {
            Ok(bundle) => {
                notes.push(format!(
                    "map={} entries={} wts={}",
                    path.display(),
                    bundle.entries.len(),
                    bundle.wts.as_ref().map_or(0, |wts| wts.entries.len())
                ));
                Some(bundle)
            }
            Err(error) => {
                notes.push(format!("map_failed={error}"));
                None
            }
        });
        if let (Some(db), Some(bundle)) = (objects.as_mut(), map.as_ref()) {
            // Map object overrides are applied over the install database with the
            // canonical merge, so the engine view and the typed view agree.
            for data in bundle.objects.values() {
                db.apply_map_objects(data);
            }
            if let Some(wts) = &bundle.wts {
                db.resolve_texts(wts);
            }
        }
        let typed = objects
            .as_ref()
            .and_then(|db| typed_from_objects(db, &storm, &install_root, map.as_ref()));
        Self {
            install_root,
            storm,
            objects,
            typed,
            triggers,
            map,
            notes,
        }
    }

    /// Object record by four-character id (`hfoo`, `AHhb`, …).
    pub fn object(&self, id: &str) -> Option<&ObjectRecord> {
        self.objects.as_ref()?.record(id)
    }

    /// Typed object info: name, tooltip, icon, model and stats for any id.
    pub fn object_info(&self, id: &str) -> Option<ObjectInfo> {
        self.typed.as_ref()?.object(id)
    }

    /// Verify every command-button icon reference against the mounted files.
    pub fn icon_audit(&self) -> Option<IconAudit> {
        Some(icons::audit(self.objects.as_ref()?, &self.storm))
    }

    /// Map name with `TRIGSTR_n` resolved through `war3map.wts`.
    pub fn localized_map_name(&self) -> Option<String> {
        let info = self.map.as_ref()?.localized_info()?;
        (!info.name.is_empty()).then_some(info.name)
    }

    /// Engine configuration mirroring `Game.dll!GameMain`'s boot inputs.
    pub fn engine_config(&self) -> EngineConfig {
        let mut config = EngineConfig::new(&self.install_root);
        if let Some(map) = &self.map {
            config.map = Some(map.path.clone());
        }
        config
    }

    /// Boot the Game.dll-order engine model (mount → object DB → map → JASS).
    pub fn boot_engine(&self) -> Result<Engine, ReError> {
        Engine::boot(self.engine_config())
    }
}

/// Build the typed database from the object database's parsed layers.
///
/// No byte is re-read and no parser runs twice: sheets, text layers and metadata
/// tables come from `ObjectDb`; the UI text tables come from the same Storm view.
fn typed_from_objects(
    db: &ObjectDb,
    storm: &StormFs,
    install_root: &Path,
    map: Option<&MapBundle>,
) -> Option<GameDatabase> {
    let mut parts = wc3_data::DatabaseParts {
        root: format!("re:{}", install_root.display()),
        ..wc3_data::DatabaseParts::default()
    };
    for (name, sheet) in &db.sheets {
        if let Some(kind) = wc3_data::table::TableKind::from_sheet_name(name) {
            parts.tables.push((kind, sheet.clone()));
        }
    }
    for text in db.texts() {
        let sections = text
            .sections
            .iter()
            .map(|section| (section.name.clone(), section.entries.clone()))
            .collect();
        parts.profiles.push((text.path.clone(), sections));
    }
    for (kind, table) in db.metas() {
        parts.meta.push((*kind, table.clone()));
    }
    parts.profile_overlay = db.profile_overlay.clone();
    for name in wc3_data::misc::MiscTables::FILES {
        let Ok(bytes) = storm.read(name) else { continue };
        let Ok(table) = wc3_formats::gamedata::TextTable::parse(name, &bytes) else {
            continue;
        };
        if name.ends_with("UnitEditorData.txt") {
            parts.enums = Some(table.sections().to_vec());
        } else {
            parts.misc.push((name.to_string(), table.sections().to_vec()));
        }
    }
    if let Ok(bytes) = storm.read("UI\\WorldEditStrings.txt") {
        if let Ok(table) = wc3_formats::gamedata::TextTable::parse("WorldEditStrings", &bytes) {
            parts.westrings = Some(table.sections().to_vec());
        }
    }
    if let Some(wts) = map.and_then(|bundle| bundle.wts.as_ref()) {
        parts.wts_entries = Some(wts.entries.clone());
    }
    GameDatabase::from_parts(parts).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install() -> Option<PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
        root.join("Units/UnitData.slk").is_file().then_some(root)
    }

    #[test]
    fn opens_install_objects_and_triggers() {
        let Some(root) = install() else {
            eprintln!("skip: extracted install missing");
            return;
        };
        let layer = ReLayer::open(&root, None);
        let objects = layer.objects.as_ref().expect("object database");
        assert!(objects.objects.len() > 1_000, "objects {}", objects.objects.len());
        assert!(objects.record("hfoo").is_some());
        let triggers = layer.triggers.as_ref().expect("trigger data");
        assert!(triggers.actions.len() > 300, "actions {}", triggers.actions.len());
    }

    #[test]
    fn resolves_map_name_through_wts() {
        let Some(root) = install() else {
            eprintln!("skip: extracted install missing");
            return;
        };
        let map = root.join("Maps/Campaign/Human04.w3m");
        if !map.is_file() {
            eprintln!("skip: Human04.w3m missing");
            return;
        }
        let layer = ReLayer::open(&root, Some(&map));
        let name = layer.localized_map_name().expect("localized map name");
        assert!(!name.starts_with("TRIGSTR_"), "unresolved map name: {name}");
        assert!(layer.map.as_ref().is_some_and(|map| map.script.is_some()));
    }

    #[test]
    fn typed_object_info_resolves_footman() {
        let Some(root) = install() else {
            eprintln!("skip: extracted install missing");
            return;
        };
        let layer = ReLayer::open(&root, None);
        assert!(layer.typed.is_some(), "typed DB: {:?}", layer.notes);
        let typed = layer.typed.as_ref().unwrap();
        let _ = typed.object_kind("hfoo");
        let info = layer.object_info("hfoo").expect("hfoo typed");
        assert_eq!(info.id, "hfoo");
        assert!(info.name.is_some(), "name: {info:?}");
        assert!(info.icon.is_some(), "icon: {info:?}");
    }

    /// Doodad art lookup contract: the map's `war3map.doo` ids must resolve to
    /// `Doodads`/`DestructableData` rows with a non-empty `dir` + `file`.
    /// A zero-model doodad world (2509 parsed, 0 placed) fails here first.
    #[test]
    fn doodad_object_rows_resolve_art() {
        let Some(root) = install() else {
            eprintln!("skip: extracted install missing");
            return;
        };
        let map = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Maps/Campaign/Human04.w3m");
        if !map.is_file() {
            eprintln!("skip: map missing");
            return;
        }
        let layer = ReLayer::open(&root, Some(&map));
        let objects = layer.objects.as_ref().expect("object db");
        let sheet_names: Vec<&String> = objects.sheets.keys().take(12).collect();
        crate::dev_log!(
            "RE_DOODAD objects={} sheets={} sample_sheets={:?}",
            objects.objects.len(),
            objects.sheets.len(),
            sheet_names
        );
        let doodads_sheet = objects
            .sheets
            .keys()
            .find(|name| name.eq_ignore_ascii_case("Doodads") || name.ends_with("\\Doodads"))
            .cloned();
        crate::dev_log!("RE_DOODAD sheet={:?}", doodads_sheet);
        if let Some(sheet) = doodads_sheet {
            let mut resolved = 0usize;
            let mut empty = 0usize;
            let mut sample = String::new();
            for (id, record) in objects.objects.iter().take(4000) {
                let dir = record.slk_field(&sheet, "dir");
                let file = record.slk_field(&sheet, "file");
                if dir.is_some() && file.is_some() {
                    resolved += 1;
                    if sample.len() < 120 {
                        sample.push_str(&format!("{id}:{:?}/{:?} ", dir, file));
                    }
                } else if dir.is_none() || file.is_none() {
                    empty += 1;
                }
            }
            crate::dev_log!(
                "RE_DOODAD resolved={resolved} missing={empty} sample={sample}"
            );
            if let Some(doodads) = layer.map.as_ref().and_then(|map| map.doodads.as_ref()) {
                let ids: Vec<String> = doodads
                    .iter()
                    .take(6)
                    .map(|doodad| {
                        format!(
                            "{:?}/{}",
                            String::from_utf8_lossy(&doodad.type_id),
                            doodad.variation
                        )
                    })
                    .collect();
                let hit = doodads
                    .first()
                    .map(|doodad| {
                        objects
                            .record(&String::from_utf8_lossy(&doodad.type_id))
                            .is_some()
                    })
                    .unwrap_or(false);
                crate::dev_log!(
                    "RE_DOODAD placements={} ids={:?} first_record={hit}",
                    doodads.len(),
                    ids
                );
            }
            for id in ["VTlt", "LSst", "YOtf"] {
                let Some(record) = objects.record(id) else {
                    crate::dev_log!("RE_DOODAD id={id} no_record");
                    continue;
                };
                let dir = record
                    .slk_field("doodads", "dir")
                    .or_else(|| record.slk_field("Doodads", "dir"));
                let file = record
                    .slk_field("doodads", "file")
                    .or_else(|| record.slk_field("Doodads", "file"));
                let num_var = record
                    .slk_field("doodads", "numVar")
                    .or_else(|| record.slk_field("Doodads", "numVar"));
                crate::dev_log!(
                    "RE_DOODAD id={id} dir={:?} file={:?} numVar={:?}",
                    dir,
                    file,
                    num_var
                );
                if let (Some(dir), Some(file)) = (dir, file) {
                    let base = format!("{dir}\\{file}");
                    for candidate in [
                        format!("{base}\\{file}0.mdx"),
                        format!("{base}\\{file}.mdx"),
                        format!("{base}0.mdx"),
                    ] {
                        crate::dev_log!(
                            "RE_DOODAD candidate={candidate} read={:?}",
                            layer.storm.read(&candidate).map(|bytes| bytes.len())
                        );
                    }
                }
            }
            let mpq = PathBuf::from(
                "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
            );
            let assets = crate::assets::Assets::load(Some(&mpq), Some(&map));
            let doodad_root = crate::terrain_render::install_root(&assets);
            crate::dev_log!("RE_DOODAD install_root={doodad_root:?}");
            if let Some(root) = doodad_root {
                let doodad_layer = ReLayer::open(&root, Some(&map));
                crate::dev_log!(
                    "RE_DOODAD root_notes={:?} objects={} read={:?}",
                    doodad_layer.notes.iter().take(3).collect::<Vec<_>>(),
                    doodad_layer.objects.as_ref().map(|db| db.objects.len()).unwrap_or(0),
                    doodad_layer
                        .storm
                        .read("Doodads\\LordaeronSummer\\Structures\\TowerScorched\\TowerScorched.mdx")
                        .map(|bytes| bytes.len())
                );
            }
            let mpq_root = PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso");
            let mpq_layer = ReLayer::open(&mpq_root, Some(&map));
            crate::dev_log!(
                "RE_DOODAD mpq notes={:?} read_tower={:?}",
                mpq_layer.notes.iter().take(4).collect::<Vec<_>>(),
                mpq_layer
                    .storm
                    .read("Doodads\\LordaeronSummer\\Structures\\TowerScorched\\TowerScorched.mdx")
                    .map(|bytes| bytes.len())
            );
            crate::dev_log!(
                "RE_DOODAD storm_files={} read_bird={:?}",
                layer.storm.listfile().len(),
                layer
                    .storm
                    .read("Doodads\\Ashenvale\\Props\\Bird\\Bird.mdx")
                    .map(|bytes| bytes.len())
            );
        }
    }

    /// Doodad loader seam: `parse_doo` ids -> `Doodads`/`DestructableData`
    /// row -> candidate path -> archive read. Prints the first five placements
    /// so a zero-model doodad world is attributable to one step.
    #[test]
    fn doodad_parse_lookup_and_read() {
        let Some(root) = install() else {
            eprintln!("skip: extracted install missing");
            return;
        };
        let map = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Maps/Campaign/Human04.w3m");
        if !map.is_file() {
            eprintln!("skip: map missing");
            return;
        }
        let layer = ReLayer::open(&root, Some(&map));
        let objects = layer.objects.as_ref().expect("object db");
        let archive = match wc3_formats::map::map_mpq_base(&map)
            .map_err(|error| error.to_string())
            .and_then(|base| wc3_re::mpqex::MpqEx::open_at(&map, base).map_err(|error| error.to_string()))
        {
            Ok(archive) => archive,
            Err(error) => {
                crate::dev_log!("RE_DOODAD_LOAD no_archive {error}");
                return;
            }
        };
        let Ok(bytes) = archive.read("war3map.doo") else {
            crate::dev_log!("RE_DOODAD_LOAD no_doo");
            return;
        };
        let Ok(raw) = crate::doodads::parse_doo(&bytes) else {
            crate::dev_log!("RE_DOODAD_LOAD parse_failed");
            return;
        };
        crate::dev_log!("RE_DOODAD_LOAD parsed={}", raw.len());
        for doodad in raw.iter().take(5) {
            let id = String::from_utf8_lossy(&doodad.id).into_owned();
            let record = objects.record(&id);
            let mut line = format!("RE_DOODAD_LOAD id={id:?} rec={}", record.is_some());
            if let Some(record) = record {
                for table in ["doodads", "destructabledata"] {
                    line.push_str(&format!(
                        " {table}:dir={:?},file={:?},numVar={:?}",
                        record.slk_field(table, "dir"),
                        record.slk_field(table, "file"),
                        record.slk_field(table, "numVar")
                    ));
                }
            }
            line.push_str(&format!(" variation={}", doodad.variation));
            crate::dev_log!("{line}");
        }
        // Read every candidate the loader would try for the first placement.
        if let Some(first) = raw.first() {
            let id = String::from_utf8_lossy(&first.id).into_owned();
            if let Some(record) = objects.record(&id) {
                for table in ["doodads", "destructabledata", "Doodads", "DestructableData"] {
                    let Some(dir) = record.slk_field(table, "dir") else { continue };
                    let Some(file) = record.slk_field(table, "file") else { continue };
                    let base = format!("{dir}\\{file}");
                    for candidate in [
                        format!("{base}\\{file}{}.mdx", first.variation),
                        format!("{base}\\{file}.mdx"),
                    ] {
                        crate::dev_log!(
                            "RE_DOODAD_LOAD read {candidate} = {:?}",
                            layer.storm.read(&candidate).map(|bytes| bytes.len())
                        );
                    }
                    break;
                }
            }
        }
    }

    #[test]
    fn icon_audit_resolves_all_references() {
        let Some(root) = install() else {
            eprintln!("skip: extracted install missing");
            return;
        };
        let layer = ReLayer::open(&root, None);
        let audit = layer.icon_audit().expect("icon audit");
        assert!(audit.references > 1_000, "references {}", audit.references);
        assert!(audit.complete(), "missing {:?}", audit.missing.first());
    }
}
