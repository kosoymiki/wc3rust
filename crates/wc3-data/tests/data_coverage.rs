//! Content-model acceptance tests over the real shipped data.
//!
//! The invariant this file exists for: **a row the command card can show always
//! resolves a name and an icon**. Everything else here (`cost`, `hotkey`,
//! `tooltip`, requirement display names, `.wts` overrides, base-ability
//! inheritance, buff rows) is measured on the same databases so a resolution
//! regression cannot hide behind a plausible count.
//!
//! Sources:
//! - `War3.mpq` (installed archive, RoC 1.26 data);
//! - `analysis/war3_data` (extracted RoC tree, same tables);
//! - `analysis/war3x_data` (extracted TFT tree — **no** `Units\*Strings.txt`,
//!   those ship in `War3xlocal.mpq`, which is not installed: icons and numbers
//!   are complete, text is not).
//!
//! Every test skips (with a note) when its source is absent.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use wc3_data::coverage::{Field, RowCoverage};
use wc3_data::source::{DirSource, Source};
use wc3_data::{DataError, GameDatabase};
use wc3_formats::gamedata::{ObjectKind, Wts};
use wc3_formats::map::map_mpq_base;
use wc3_mpq::MpqArchive;

const MPQ: &str = "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq";
const DOTF_MAP: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/DotF_v1.4.w3x";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn analysis() -> PathBuf {
    repo_root().join("analysis")
}

/// `Source` over a single MPQ archive (the installed-file path).
struct MpqSource {
    archive: MpqArchive,
}

impl Source for MpqSource {
    fn read(&self, path: &str) -> wc3_data::Result<Vec<u8>> {
        let normalized = path.replace('/', "\\");
        self.archive.read(&normalized).map_err(|source| DataError::Io {
            path: path.to_string(),
            source: std::io::Error::other(source.to_string()),
        })
    }

    fn describe(&self) -> String {
        "mpq".to_string()
    }
}

fn load_mpq() -> Option<GameDatabase> {
    let path = Path::new(MPQ);
    if !path.is_file() {
        eprintln!("skip: {MPQ} not installed");
        return None;
    }
    let archive = MpqArchive::open(path).expect("open War3.mpq");
    Some(GameDatabase::load(&MpqSource { archive }).expect("load from mpq"))
}

fn mpq() -> Option<&'static GameDatabase> {
    static MPQ_DB: OnceLock<Option<GameDatabase>> = OnceLock::new();
    MPQ_DB.get_or_init(load_mpq).as_ref()
}

fn load_dir(root: &str) -> Option<GameDatabase> {
    let path = analysis().join(root);
    if !path.is_dir() {
        eprintln!("skip: {} missing", path.display());
        return None;
    }
    Some(GameDatabase::load(&DirSource::new(path)).expect("load from dir"))
}

fn roc() -> Option<&'static GameDatabase> {
    static ROC: OnceLock<Option<GameDatabase>> = OnceLock::new();
    ROC.get_or_init(|| load_dir("war3_data")).as_ref()
}

fn tft() -> Option<&'static GameDatabase> {
    static TFT: OnceLock<Option<GameDatabase>> = OnceLock::new();
    TFT.get_or_init(|| load_dir("war3x_data")).as_ref()
}

/// Owned RoC database, for tests that merge map object data into it.
fn roc_owned() -> Option<GameDatabase> {
    load_dir("war3_data")
}

/// The two databases whose text layer is complete.
fn text_complete() -> Vec<(&'static str, &'static GameDatabase)> {
    let mut out = Vec::new();
    if let Some(db) = mpq() {
        out.push(("War3.mpq", db));
    }
    if let Some(db) = roc() {
        out.push(("analysis/war3_data", db));
    }
    assert!(
        !out.is_empty(),
        "neither War3.mpq nor analysis/war3_data is available"
    );
    out
}

fn assert_field_complete(label: &str, rows: &RowCoverage, field: Field) {
    let missing = rows.missing(field);
    assert!(
        missing.is_empty(),
        "{label}: {} of {} rows have no {}: {:?}",
        missing.len(),
        rows.rows,
        field.label(),
        missing
    );
}

/// The mission invariant: every id the card can list has a name and an icon.
#[test]
fn card_reachable_rows_resolve_a_name_and_an_icon() {
    for (label, data) in text_complete() {
        let report = data.coverage();
        assert!(
            report.menu.entries > 100,
            "{label}: only {} card entries discovered",
            report.menu.entries
        );
        assert_field_complete(label, &report.menu.reachable, Field::Name);
        assert_field_complete(label, &report.menu.reachable, Field::Icon);
        assert_field_complete(label, &report.menu.reachable, Field::Cost);
        // Every discovered card entry carries name + icon + gold.
        assert_eq!(
            report.menu.complete, report.menu.entries,
            "{label}: {} of {} entries incomplete",
            report.menu.entries - report.menu.complete,
            report.menu.entries
        );
    }
}

/// Hotkeys: every card entry has one except the three Demon Gate summons,
/// which have no `Hotkey` key in the shipped profile text at all.
#[test]
fn card_entry_hotkeys_are_complete_except_the_demon_gate_rows() {
    for (label, data) in text_complete() {
        let report = data.coverage();
        let missing = report.menu.field(Field::Hotkey).expect("hotkey column");
        assert_eq!(
            missing.missing.len(),
            3,
            "{label}: expected the three named Demon Gate rows, got {:?}",
            missing.missing
        );
        for id in &missing.missing {
            assert!(
                ["nfel", "ninf", "nbal"].contains(&id.as_str()),
                "{label}: unexpected hotkey gap {id}"
            );
            let base = data
                .unit(id)
                .unwrap_or_else(|| panic!("{label}: {id} is not a unit row"));
            assert!(
                base.hotkey().is_none(),
                "{label}: {id} ships a Hotkey but the entry lost it"
            );
            assert!(
                !base.name().unwrap_or_default().is_empty(),
                "{label}: {id} must still resolve a name"
            );
        }
    }
}

/// Hero learn-skill buttons are the densest card surface: 4 abilities per hero.
#[test]
fn hero_learn_buttons_resolve_name_icon_hotkey_and_tooltip() {
    for (label, data) in text_complete() {
        let report = data.coverage();
        let buttons = &report.menu.hero_abilities;
        assert!(
            buttons.rows >= 48,
            "{label}: only {} learn buttons",
            buttons.rows
        );
        for field in [Field::Name, Field::Icon, Field::Hotkey, Field::Tooltip] {
            assert_field_complete(label, buttons, field);
        }
    }
}

/// A requirement must name a loaded object row, and (where the text ships) a
/// display name. Unknown ids are a data-model defect, not a text gap.
#[test]
fn requirement_tokens_name_loaded_objects() {
    let mut checked = 0usize;
    for (label, data) in text_complete() {
        let report = data.coverage();
        assert!(
            report.menu.requires > 100,
            "{label}: only {} requirement references",
            report.menu.requires
        );
        assert!(
            report.menu.requires_unknown.is_empty(),
            "{label}: requirements name no loaded row: {:?}",
            report.menu.requires_unknown
        );
        assert!(
            report.menu.requires_unresolved.is_empty(),
            "{label}: requirement display names unresolved: {:?}",
            report.menu.requires_unresolved
        );
        checked += 1;
    }
    assert!(checked > 0);

    // The TFT tree has no text layer, so only the structural half applies.
    if let Some(tft) = tft() {
        let report = tft.coverage();
        assert!(
            report.menu.requires_unknown.is_empty(),
            "tft: requirements name no loaded row: {:?}",
            report.menu.requires_unknown
        );
    }
}

/// `Researches=` drives a third card list; upgrades encode requirements as
/// `Requirescount` + `Requires1..N` with an empty `Requires=`.
#[test]
fn upgrade_research_entries_use_the_numbered_requires_encoding() {
    let Some(data) = roc() else { return };
    assert!(
        data.localizer().name("Rhme").is_some(),
        "Rhme name missing from the profile text"
    );
    let upgrade = data.upgrade("Rhme").expect("Rhme row");
    assert_eq!(
        upgrade.requires(),
        ["hkee", "hcas"],
        "numbered Requires1..N not read"
    );

    let entry = data.research_entry("Rhme").expect("Rhme research entry");
    assert!(entry.name.contains("мечи"), "name: {}", entry.name);
    assert!(entry.icon.is_some(), "research icon");
    assert_eq!(entry.hotkey.as_deref(), Some("S,S,S"));
    assert_eq!(entry.gold, Some(100));
    assert_eq!(entry.lumber, Some(50));
    assert!(entry.build_time.is_some(), "research time");
    assert_eq!(entry.requires, ["hkee", "hcas"]);
    assert_eq!(entry.requires_names.len(), 2);
    assert!(
        entry.requires_names.iter().all(|name| !name.is_empty()),
        "requirement names: {:?}",
        entry.requires_names
    );
    assert_eq!(entry.kind, ObjectKind::Upgrade);
    assert!(entry.is_complete(), "research entry must be card-complete");

    let menu = data.research_menu("hbla");
    assert!(
        menu.iter().any(|entry| entry.id == "Rhme"),
        "hbla research menu: {:?}",
        menu.iter().map(|entry| entry.id.clone()).collect::<Vec<_>>()
    );
    let researchers = data.researchers();
    assert!(
        researchers.iter().any(|(id, _)| id == "hbla"),
        "researchers: {researchers:?}"
    );
}

/// A map's `war3map.wts` + object data beat the profile text.
///
/// The fixture is a TFT map (`Ucrl`, its prototype, exists only in the TFT
/// sheet), so it runs against the TFT tree — whose profile text is absent. That
/// makes the assertion stronger: the row has **no** profile name at all, and the
/// name the card shows can only come from the map's own `.wts`.
#[test]
fn wts_and_object_data_override_the_profile_text() {
    let Some(mut data) = load_dir("war3x_data") else { return };
    let map = repo_root().join(DOTF_MAP);
    if !map.is_file() {
        eprintln!("skip: {} missing", map.display());
        return;
    }
    let base = map_mpq_base(&map).expect("map header");
    let archive = MpqArchive::open_at(&map, base).expect("open map");

    let wts_bytes = archive.read("war3map.wts").expect("war3map.wts");
    let wts = Wts::parse(&wts_bytes).expect("parse wts");
    let entries: std::collections::BTreeMap<u32, String> = wts
        .iter()
        .map(|(id, text)| (id, text.to_string()))
        .collect();
    assert!(entries.len() > 100, "wts entries: {}", entries.len());
    data.localizer_mut().set_wts_entries(entries.clone());

    for (file, kind) in [
        ("war3map.w3u", ObjectKind::Unit),
        ("war3map.w3a", ObjectKind::Ability),
        ("war3map.w3t", ObjectKind::Item),
    ] {
        let bytes = archive.read(file).expect("object data");
        data.apply_object_data(file, kind, &bytes)
            .unwrap_or_else(|error| panic!("apply {file}: {error}"));
    }

    // The prototype row exists and has no name of its own (no text layer).
    let prototype = data.object("Ucrl").expect("prototype Ucrl");
    assert!(
        prototype.name.is_none(),
        "TFT tree unexpectedly names Ucrl: {:?}",
        prototype.name
    );

    // `U000` is a custom unit cloned from `Ucrl` with `unam=TRIGSTR_013`.
    let custom = data.object("U000").expect("custom unit U000");
    let expected = entries.get(&13).expect("wts string 13").clone();
    assert_eq!(custom.name.as_deref(), Some(expected.as_str()));
    assert!(
        !expected.starts_with("TRIGSTR_"),
        "wts entry 13 did not resolve: {expected}"
    );

    // Ability names come from the same `.wts` table (`A000 anam=TRIGSTR_023`).
    let ability = data.object("A000").expect("custom ability A000");
    assert_eq!(
        ability.name.as_deref(),
        Some(entries.get(&23).expect("wts string 23").as_str())
    );

    // Item rows resolve their map text and keep the prototype's art.
    let item = data.object("I000").expect("custom item I000");
    assert_eq!(
        item.name.as_deref(),
        Some(entries.get(&55).expect("wts string 55").as_str())
    );
    assert!(item.icon.is_some(), "custom item icon");
}

/// Buffs have no shipped sheet: their card data is the `Bufftip`/`Buffart`
/// profile section, and a `war3map.w3b` entry overrides it.
#[test]
fn buff_rows_resolve_from_the_profile_text_and_w3b_wins() {
    let Some(mut data) = roc_owned() else { return };
    let ids = data.buff_ids();
    assert_eq!(ids.len(), 90, "buffs with a profile section: {ids:?}");
    let buff = data.buff("BHwe").expect("BHwe buff row");
    assert_eq!(buff.name.as_deref(), Some("Дух Воды"));
    assert!(
        buff.icon
            .as_deref()
            .is_some_and(|icon| icon.ends_with("BTNSummonWaterElemental.blp")),
        "buff icon: {:?}",
        buff.icon
    );
    assert_eq!(
        data.object("BHwe").map(|info| info.kind),
        Some(ObjectKind::Buff)
    );
    assert!(!buff.overridden, "retail buff must not be flagged overridden");

    // A map override replaces the name.
    let file = wc3_formats::gamedata::ObjectDataFile {
        kind: ObjectKind::Buff,
        version: 2,
        base: vec![wc3_formats::gamedata::ObjectEntry {
            old_id: wc3_formats::gamedata::FourCc::parse("BHwe").unwrap(),
            new_id: wc3_formats::gamedata::FourCc::new([0; 4]),
            changes: vec![wc3_formats::gamedata::ObjectChange {
                id: "fnam".to_string(),
                value: wc3_formats::gamedata::ChangeValue::String("Changed".to_string()),
                level: 0,
                data_pointer: 0,
            }],
        }],
        custom: vec![],
    };
    data.apply_parsed("war3map.w3b".to_string(), file)
        .expect("apply w3b");
    let overridden = data.buff("BHwe").expect("buff after override");
    assert_eq!(overridden.name.as_deref(), Some("Changed"));
    assert!(overridden.overridden);
}

/// Ability rows store only what differs from their base `code` row; the card
/// must show the inherited label, not an empty string.
#[test]
fn ability_variants_inherit_the_base_row_text() {
    let Some(data) = roc() else { return };
    let variant = data.ability("AIa1").expect("AIa1");
    assert_eq!(variant.code(), Some("AIab"));
    assert!(
        variant.name().is_none(),
        "AIa1 must have no own Name (that is what makes it a variant)"
    );
    let inherited = variant.resolved_name().expect("inherited name");
    assert_eq!(
        data.localizer().name("AIab").as_deref(),
        Some(inherited.as_str())
    );
    assert_eq!(
        data.object("AIa1").and_then(|info| info.name).as_deref(),
        Some(inherited.as_str()),
        "the unified lookup must expose the inherited name"
    );
    // The art stays the variant's own row.
    assert!(
        variant
            .resolved_icon()
            .is_some_and(|icon| icon.ends_with("BTNRingPurple.blp")),
        "variant icon: {:?}",
        variant.resolved_icon()
    );
}

/// `war3map.doo`-placed doodads and `war3mapUnits.doo` ids are map data, but the
/// row counts they resolve against are the shipped sheets: pin them so a table
/// load regression cannot pass silently.
#[test]
fn table_row_counts_are_pinned_for_both_roots() {
    if let Some(data) = roc() {
        let counts: Vec<(&str, usize)> = data
            .counts()
            .iter()
            .map(|(kind, rows)| (kind.name(), *rows))
            .collect();
        assert_eq!(
            counts,
            vec![
                ("UnitData", 466),
                ("UnitBalance", 466),
                ("UnitWeapons", 316),
                ("UnitAbilities", 324),
                ("UnitUI", 466),
                ("AbilityData", 397),
                ("ItemData", 132),
                ("UpgradeData", 58),
                ("DestructableData", 128),
                ("Doodads", 278),
            ],
            "roc table rows"
        );
    }
    if let Some(data) = roc() {
        let report = data.coverage();
        assert_eq!(report.menu.producers, 35, "roc producers");
        assert_eq!(report.menu.builders, 6, "roc builders");
        assert_eq!(report.menu.researchers, 28, "roc researchers");
        assert_eq!(report.menu.entries, 182, "roc card entries");
    }
    if let Some(data) = tft() {
        let report = data.coverage();
        assert_eq!(report.menu.producers, 43, "tft producers");
        assert_eq!(report.menu.builders, 9, "tft builders");
        assert_eq!(report.menu.researchers, 37, "tft researchers");
        assert_eq!(report.menu.entries, 278, "tft card entries");
    }
}

/// The TFT extraction is text-blind by construction: `War3xlocal.mpq` is not
/// installed, so only art/structure can be asserted. This test pins that state
/// instead of letting a "complete" claim cover an absent text layer.
#[test]
fn tft_tree_ships_art_and_numbers_but_no_profile_text() {
    let Some(data) = tft() else { return };
    let missing = data.localizer().missing_files();
    assert!(
        missing
            .iter()
            .any(|path| path.ends_with("OrcUnitStrings.txt")),
        "TFT tree unexpectedly ships the localized strings: {missing:?}"
    );
    assert_eq!(
        data.localizer().loaded_files().len(),
        21,
        "TFT profile layers are the `*Func.txt` art layer only"
    );
    let report = data.coverage();
    // Art resolves everywhere except `orbr`, which the TFT Orc peon lists in
    // `Builds=` but which no TFT sheet in this extraction defines.
    assert_eq!(
        report.menu.reachable.missing(Field::Icon),
        ["orbr"],
        "TFT reachable art"
    );
    assert!(
        report.menu.reachable.missing(Field::Name).len() == report.menu.reachable.rows,
        "TFT names must all be absent without the localized archive"
    );
}
