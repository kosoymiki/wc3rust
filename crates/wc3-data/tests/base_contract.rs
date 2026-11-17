//! Base-contract tests for the port onto `analysis/tech-lead/BASE.md`.
//!
//! These tests pin the interfaces the base consumes:
//! - `GameDatabase` + `Source` naming,
//! - one lookup path (`object(id)`) across all object kinds,
//! - `DatabaseParts`/`from_parts` parity with the byte-loading path (the bridge
//!   `wc3-re::objectdata::ObjectDb::typed()` uses),
//! - the canonical `wc3-formats` kind mapping (`w3b` destructable, `w3d` doodad,
//!   `w3h` buff) that object-data and table routing depend on.
//!
//! They parse the real extracted data, like `tests/real_data.rs`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use wc3_data::source::{DirSource, Source};
use wc3_data::table::TableKind;
use wc3_data::validate::Severity;
use wc3_data::{DatabaseParts, GameDatabase, ObjectInfo};
use wc3_formats::gamedata::{MetaKind, ObjectKind};

fn analysis_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis")
}

fn roc_root() -> PathBuf {
    analysis_dir().join("war3_data")
}

fn roc() -> &'static GameDatabase {
    static ROC: OnceLock<GameDatabase> = OnceLock::new();
    ROC.get_or_init(|| GameDatabase::load(&DirSource::new(roc_root())).expect("load RoC"))
}

fn roc_with_catalog() -> GameDatabase {
    let mut data = GameDatabase::load(&DirSource::new(roc_root())).expect("load RoC");
    let listfile = wc3_data::source::Listfile::from_file(
        analysis_dir().join("list-War3-clean.txt"),
    )
    .expect("RoC listfile");
    data.set_catalog(listfile);
    data
}

/// Rebuild the database from already-parsed layers, exactly as the composed
/// install database does. Uses only public API + the canonical parser.
fn parts_from(root: &Path) -> DatabaseParts {
    let source = DirSource::new(root);
    let mut parts = DatabaseParts {
        root: source.describe(),
        ..DatabaseParts::default()
    };
    for kind in TableKind::ALL {
        if let Ok(bytes) = source.read(kind.path()) {
            let sheet = wc3_formats::gamedata::Sheet::parse(kind.name(), &bytes).expect("sheet");
            parts.tables.push((kind, sheet));
        } else {
            parts.missing.push(kind.path().to_string());
        }
    }
    for kind in [
        MetaKind::Unit,
        MetaKind::Ability,
        MetaKind::Upgrade,
        MetaKind::Destructable,
        MetaKind::Doodad,
        MetaKind::Misc,
        MetaKind::Skin,
        MetaKind::UpgradeEffect,
        MetaKind::Buff,
    ] {
        let path = format!("Units\\{}", kind.sheet_name());
        if let Ok(bytes) = source.read(&path) {
            let table = wc3_formats::gamedata::MetaTable::parse(kind, &bytes).expect("meta");
            parts.meta.push((kind, table));
        }
    }
    for suffix in ["Strings", "Func"] {
        for base in [
            "UnitGlobal",
            "Command",
            "CommonAbility",
            "CampaignUnit",
            "CampaignAbility",
            "CampaignUpgrade",
            "HumanUnit",
            "HumanAbility",
            "HumanUpgrade",
            "OrcUnit",
            "OrcAbility",
            "OrcUpgrade",
            "UndeadUnit",
            "UndeadAbility",
            "UndeadUpgrade",
            "NightElfUnit",
            "NightElfAbility",
            "NightElfUpgrade",
            "NeutralUnit",
            "NeutralAbility",
            "NeutralUpgrade",
            "Item",
            "ItemAbility",
        ] {
            let path = format!("Units\\{base}{suffix}.txt");
            if let Ok(bytes) = source.read(&path) {
                let table =
                    wc3_formats::gamedata::TextTable::parse(path.clone(), &bytes).expect("text");
                parts.profiles.push((path, table.sections().to_vec()));
            }
        }
    }
    if let Ok(bytes) = source.read("UI\\UnitEditorData.txt") {
        let table = wc3_formats::gamedata::TextTable::parse("UnitEditorData", &bytes).expect("txt");
        parts.enums = Some(table.sections().to_vec());
    }
    for name in wc3_data::misc::MiscTables::FILES {
        if let Ok(bytes) = source.read(name) {
            let table = wc3_formats::gamedata::TextTable::parse(name, &bytes).expect("txt");
            parts.misc.push((name.to_string(), table.sections().to_vec()));
        }
    }
    if let Ok(bytes) = source.read("UI\\WorldEditStrings.txt") {
        let table = wc3_formats::gamedata::TextTable::parse("WorldEditStrings", &bytes).expect("txt");
        parts.westrings = Some(table.sections().to_vec());
    }
    parts
}

#[test]
fn kind_mapping_matches_the_warcraft_file_table() {
    // HiveWE `base/map/map.ixx` load order: w3u units, w3t items, w3a abilities,
    // w3d doodads, w3b destructables, w3h buffs, w3q upgrades.
    assert_eq!(ObjectKind::from_extension("w3u"), Some(ObjectKind::Unit));
    assert_eq!(ObjectKind::from_extension("w3t"), Some(ObjectKind::Item));
    assert_eq!(ObjectKind::from_extension("w3a"), Some(ObjectKind::Ability));
    assert_eq!(
        ObjectKind::from_extension("w3d"),
        Some(ObjectKind::Doodad),
        "w3d is the doodad file"
    );
    assert_eq!(
        ObjectKind::from_extension("w3b"),
        Some(ObjectKind::Destructable),
        "w3b is the destructable file"
    );
    assert_eq!(
        ObjectKind::from_extension("w3h"),
        Some(ObjectKind::Buff),
        "w3h is the buff file"
    );
    assert_eq!(ObjectKind::from_extension("w3q"), Some(ObjectKind::Upgrade));
    assert_eq!(ObjectKind::from_extension("w3x"), None);
    assert_eq!(ObjectKind::Doodad.extension(), "w3d");
    assert_eq!(ObjectKind::Destructable.extension(), "w3b");
    assert_eq!(ObjectKind::Buff.extension(), "w3h");
    // Extended layouts: ability/upgrade/doodad carry level/variation + pointer.
    assert!(ObjectKind::Ability.extended());
    assert!(ObjectKind::Upgrade.extended());
    assert!(ObjectKind::Doodad.extended());
    assert!(!ObjectKind::Unit.extended());
    assert!(!ObjectKind::Buff.extended());
    assert!(!ObjectKind::Destructable.extended());
}

#[test]
fn table_sheet_names_round_trip() {
    for kind in TableKind::ALL {
        assert_eq!(TableKind::from_sheet_name(kind.name()), Some(kind));
        let file = kind.path().rsplit('\\').next().expect("file name");
        assert_eq!(TableKind::from_sheet_name(file), Some(kind));
        let stem = file.rsplit_once('.').map(|(stem, _)| stem).expect("stem");
        assert_eq!(TableKind::from_sheet_name(stem), Some(kind));
    }
    // Doodad metadata names the sheet `DoodadData`, the file is `Doodads.slk`.
    assert_eq!(
        TableKind::from_sheet_name("DoodadData"),
        Some(TableKind::Doodads)
    );
    assert_eq!(TableKind::from_sheet_name("nonsense"), None);
}

#[test]
fn unified_lookup_covers_every_object_kind() {
    let data = roc();
    let cases: [(&str, ObjectKind); 6] = [
        ("hfoo", ObjectKind::Unit),
        ("AHbz", ObjectKind::Ability),
        ("ratc", ObjectKind::Item),
        ("Rhme", ObjectKind::Upgrade),
        ("LTlt", ObjectKind::Destructable),
        ("APct", ObjectKind::Doodad),
    ];
    for (id, kind) in cases {
        let info = data.object(id).unwrap_or_else(|| panic!("{id} missing"));
        assert_eq!(info.kind, kind, "{id} kind");
        assert_eq!(info.id, id);
        assert!(info.name.is_some(), "{id} must have a name");
        assert_eq!(data.object_kind(id), Some(kind));
    }
    assert!(data.object("zzzz").is_none());
}

#[test]
fn unified_lookup_exposes_icons_names_and_stats() {
    let data = roc();

    let footman = data.object("hfoo").expect("hfoo");
    assert_eq!(footman.name.as_deref(), Some("Пехотинец"));
    assert_eq!(
        footman.icon.as_deref(),
        Some("ReplaceableTextures\\CommandButtons\\BTNFootman.blp")
    );
    assert_eq!(footman.hotkey.as_deref(), Some("F"));
    assert_eq!(footman.button_pos, Some((0, 0)));
    assert_eq!(
        footman.model.as_deref(),
        Some("units\\human\\Footman\\Footman.mdx")
    );
    let stats = &footman.stats;
    assert_eq!(stats.hp, Some(420));
    assert_eq!(stats.gold_cost, Some(160));
    assert_eq!(stats.level, Some(2));
    assert_eq!(stats.armor, Some(2));
    assert_eq!(stats.armor_per_upgrade, Some(2));
    assert_eq!(stats.min_damage, Some(12));
    assert_eq!(stats.max_damage, Some(13));
    assert_eq!(stats.average_damage, Some(12.5));
    assert!(stats.targets.iter().any(|target| target == "ground"));

    let blizzard = data.object("AHbz").expect("AHbz");
    assert_eq!(blizzard.name.as_deref(), Some("Буран"));
    assert_eq!(
        blizzard.icon.as_deref(),
        Some("ReplaceableTextures\\CommandButtons\\BTNBlizzard.blp")
    );
    assert_eq!(blizzard.stats.mana_cost, Some(75));
    assert_eq!(blizzard.stats.cooldown, Some(6.0));
    assert_eq!(blizzard.stats.area, Some(200.0));
    assert_eq!(blizzard.stats.range, Some(800.0));
    // Level-1 tooltip with `<AHbz,Data11>` placeholders resolved.
    let ubertip = blizzard.ubertip.expect("tooltip");
    assert!(ubertip.contains("Вызов 6 снежных вихрей"), "{ubertip}");
    assert!(!ubertip.contains('<'), "placeholders must resolve: {ubertip}");

    let claws = data.object("ratc").expect("ratc");
    assert_eq!(claws.name.as_deref(), Some("Боевые когти (+12)"));
    assert_eq!(claws.stats.level, Some(7));
    assert_eq!(claws.stats.gold_cost, Some(500));
    // `uses=-` means unlimited charges, reported as `None`.
    assert_eq!(claws.stats.charges, None);

    let melee = data.object("Rhme").expect("Rhme");
    assert_eq!(melee.stats.max_level, Some(3));
    assert_eq!(melee.stats.gold_cost, Some(100));
    assert_eq!(melee.stats.build_time, Some(60));

    let tree = data.object("LTlt").expect("LTlt");
    assert_eq!(tree.stats.hp, Some(50));
    assert!(tree
        .model
        .as_deref()
        .is_some_and(|model| model.to_ascii_lowercase().ends_with("lordaerontree.mdx")));

    let plant = data.object("APct").expect("APct");
    assert!(plant
        .model
        .as_deref()
        .is_some_and(|model| model.to_ascii_lowercase().ends_with("ashencattail.mdx")));
    assert!(plant.stats.is_empty());
}

#[test]
fn from_parts_reproduces_the_loaded_database() {
    let loaded = roc();
    let parts = parts_from(&roc_root());
    assert!(!parts.tables.is_empty(), "parsed tables");
    assert!(!parts.profiles.is_empty(), "parsed profiles");
    let rebuilt = GameDatabase::from_parts(parts).expect("from_parts");

    // Same tables and rows.
    let loaded_counts: Vec<usize> = loaded.counts().iter().map(|(_, rows)| *rows).collect();
    let rebuilt_counts: Vec<usize> = rebuilt.counts().iter().map(|(_, rows)| *rows).collect();
    assert_eq!(loaded_counts, rebuilt_counts);
    assert_eq!(loaded_counts.len(), 10);

    // Same metadata coverage.
    for kind in [
        MetaKind::Unit,
        MetaKind::Ability,
        MetaKind::Upgrade,
        MetaKind::Destructable,
        MetaKind::Doodad,
        MetaKind::Misc,
        MetaKind::Skin,
        MetaKind::UpgradeEffect,
    ] {
        let loaded_len = loaded.meta_table(kind).map(|table| table.len());
        let rebuilt_len = rebuilt.meta_table(kind).map(|table| table.len());
        assert_eq!(loaded_len, rebuilt_len, "{kind:?} metadata");
    }

    // Same cell values on a spread of tables.
    for (id, table, column) in [
        ("hfoo", TableKind::UnitBalance, "HP"),
        ("hfoo", TableKind::UnitData, "race"),
        ("AHbz", TableKind::AbilityData, "Cost1"),
        ("Rhme", TableKind::UpgradeData, "goldbase"),
        ("ratc", TableKind::ItemData, "Level"),
        ("LTlt", TableKind::DestructableData, "HP"),
        ("APct", TableKind::Doodads, "file"),
    ] {
        let left = loaded.table(table).and_then(|table| table.text(id, column));
        let right = rebuilt.table(table).and_then(|table| table.text(id, column));
        assert_eq!(left, right, "{id}.{column}");
    }

    // Same unified lookup for every kind.
    for id in ["hfoo", "AHbz", "ratc", "Rhme", "LTlt", "APct"] {
        let left: Option<ObjectInfo> = loaded.object(id);
        let right: Option<ObjectInfo> = rebuilt.object(id);
        assert_eq!(left, right, "{id}");
    }

    // Same validation verdict.
    assert_eq!(
        loaded.validate().error_count(),
        rebuilt.validate().error_count()
    );
    assert_eq!(rebuilt.validate().error_count(), 0);
}

#[test]
fn map_overrides_resolve_through_the_typed_lookup() {
    // The composed database applies map object data before the typed view is
    // built, so a `TRIGSTR_` name must already resolve in `object()`.
    let mut loaded = GameDatabase::load(&DirSource::new(roc_root())).expect("load RoC");
    let w3u = std::fs::read(analysis_dir().join("war3_map/Human04/war3map.w3u")).expect("w3u");
    let wts = std::fs::read(analysis_dir().join("war3_map/Human04/war3map.wts")).expect("wts");
    loaded.set_wts(&wts).expect("wts");
    let kind = loaded
        .apply_object_data_named("war3map.w3u", &w3u)
        .expect("merge");
    assert_eq!(kind, ObjectKind::Unit);

    let info = loaded.object("uC01").expect("custom unit");
    assert_eq!(info.kind, ObjectKind::Unit);
    assert_eq!(info.name.as_deref(), Some("Тимми"));
    assert_eq!(info.stats.hp, Some(330));
    // Base-section edit from the same file: item stock start becomes 0.
    assert_eq!(
        loaded
            .table(TableKind::ItemData)
            .and_then(|table| table.i32("phea", "stockStart").ok().flatten()),
        Some(0)
    );
}

#[test]
fn object_data_kind_is_taken_from_the_extension() {
    let mut loaded = GameDatabase::load(&DirSource::new(roc_root())).expect("load RoC");
    // Minimal version-2 stream: no base entries, no custom entries.
    let mut empty = Vec::new();
    empty.extend_from_slice(&2u32.to_le_bytes());
    empty.extend_from_slice(&0u32.to_le_bytes());
    empty.extend_from_slice(&0u32.to_le_bytes());
    for (file, kind) in [
        ("map/war3map.w3u", ObjectKind::Unit),
        ("map/war3map.w3t", ObjectKind::Item),
        ("map/war3map.w3a", ObjectKind::Ability),
        ("map/war3map.w3q", ObjectKind::Upgrade),
        ("map/war3map.w3b", ObjectKind::Destructable),
        ("map/war3map.w3d", ObjectKind::Doodad),
        ("map/war3map.w3h", ObjectKind::Buff),
    ] {
        assert_eq!(
            loaded.apply_object_data_named(file, &empty).expect(file),
            kind,
            "{file}"
        );
    }
    assert!(matches!(
        loaded.apply_object_data_named("map/war3map.txt", &empty),
        Err(wc3_data::DataError::UnknownObjectKind { .. })
    ));
}

#[test]
fn validation_report_shape_is_stable() {
    let data = roc_with_catalog();
    let report = data.validate();
    assert_eq!(report.error_count(), 0);
    assert!(report.warning_count() > 0, "retail data has known warnings");
    assert!(report
        .warnings()
        .all(|issue| issue.severity == Severity::Warning));
    let table_count = report.tables.len();
    assert_eq!(table_count, 10);
    assert!(report.total_rows() > 2_000);
    // Path classes and catalogue totals (BASE gate 4: 131 building models,
    // 691 command-button icons).
    let classes: Vec<&str> = report.paths.iter().map(|stat| stat.class).collect();
    for expected in [
        "unit-model",
        "building-model",
        "unit-icon",
        "item-icon",
        "destructable-model",
        "doodad-model",
    ] {
        assert!(classes.contains(&expected), "missing path class {expected}");
    }
    let catalog = report.catalog.as_ref().expect("catalogue");
    assert_eq!(catalog.entries, 10_523);
    assert_eq!(catalog.unit_models, 411);
    assert_eq!(catalog.building_models, 131);
    assert_eq!(catalog.command_button_icons, 691);
}
