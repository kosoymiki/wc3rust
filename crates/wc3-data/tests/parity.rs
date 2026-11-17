//! PARITY: the typed database must cover every SLK/object-data field the game
//! needs — costs, tech requirements, tooltips, icons, hotkeys, armour and
//! damage — and it must be able to drive the build/train menus itself.
//!
//! All assertions run against the real RoC extraction (`analysis/war3_data`);
//! the expected lists below were read from `Units\HumanUnitFunc.txt` and
//! `UnitBalance.slk` (quoted in the comments), not authored here.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use wc3_data::source::DirSource;
use wc3_data::{GameDatabase, MenuEntry};
use wc3_formats::gamedata::ObjectKind;

fn analysis_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis")
}

fn roc() -> &'static GameDatabase {
    static ROC: OnceLock<GameDatabase> = OnceLock::new();
    ROC.get_or_init(|| {
        GameDatabase::load(&DirSource::new(analysis_dir().join("war3_data"))).expect("load RoC")
    })
}

/// `HumanUnitFunc.txt: [hbar] Trains = hfoo,hrif,hkni`
#[test]
fn barracks_train_menu_comes_from_the_profile_list() {
    let data = roc();
    let menu = data.train_menu("hbar");
    let ids: Vec<&str> = menu.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(ids, ["hfoo", "hrif", "hkni"], "hbar Trains order");

    for entry in &menu {
        assert!(!entry.name.is_empty(), "{} name", entry.id);
        assert!(entry.icon.is_some(), "{} icon", entry.id);
        assert!(entry.gold.is_some(), "{} gold", entry.id);
        assert_eq!(entry.kind, ObjectKind::Unit, "{} kind", entry.id);
        assert!(entry.tooltip.is_some(), "{} tooltip", entry.id);
        assert!(entry.is_complete(), "{} complete", entry.id);
    }
    let footman: &MenuEntry = &menu[0];
    assert_eq!(footman.name, "Пехотинец");
    assert_eq!(footman.gold, Some(160), "UnitBalance goldcost");
    assert_eq!(footman.food, Some(2), "UnitBalance fused");
    assert_eq!(footman.build_time, Some(20), "UnitBalance bldtm");
    assert_eq!(footman.hotkey_label(), "F", "Hotkey=F");
}

/// `[htow] Trains = hpea`, `[hkee] Trains = hpea`, `[hcas] Trains = hpea`
#[test]
fn hall_train_menu_matches_each_hall_upgrade() {
    let data = roc();
    for hall in ["htow", "hkee", "hcas"] {
        let menu = data.train_menu(hall);
        assert_eq!(menu.len(), 1, "{hall} trains exactly the peasant");
        assert_eq!(menu[0].id, "hpea", "{hall} trainee");
        assert_eq!(menu[0].name, "Работник");
        assert!(menu[0].gold.is_some(), "peasant cost");
    }
}

/// `[hpea] Builds = htow,hhou,hbar,hbla,hwtw,halt,harm,hars,hlum,hgra`
#[test]
fn peasant_build_menu_comes_from_the_profile_list() {
    let data = roc();
    let menu = data.build_menu("hpea");
    let ids: Vec<&str> = menu.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "htow", "hhou", "hbar", "hbla", "hwtw", "halt", "harm", "hars", "hlum", "hgra"
        ],
        "hpea Builds order"
    );
    for entry in &menu {
        assert_eq!(entry.kind, ObjectKind::Unit, "{} kind", entry.id);
        assert!(!entry.name.is_empty(), "{} name", entry.id);
        assert!(entry.icon.is_some(), "{} icon", entry.id);
        assert!(entry.gold.is_some(), "{} gold", entry.id);
        // Buildings carry their footprint cost in the same UnitBalance columns.
        assert!(entry.build_time.is_some(), "{} build time", entry.id);
    }
    let farm = menu.iter().find(|entry| entry.id == "hhou").expect("farm");
    assert_eq!(farm.name, "Ферма");
    assert_eq!(farm.gold, Some(80), "Farm gold from UnitBalance");
    assert_eq!(farm.food, None, "buildings provide food, they do not cost it");
}

/// `[hgtw] Requires = hlum` — requirements are exposed verbatim plus resolved.
#[test]
fn tech_requirements_are_exposed_and_named() {
    let data = roc();
    let entry = data.menu_entry("hgtw").expect("guard tower");
    assert_eq!(entry.requires, ["hlum"], "Requires = hlum");
    assert_eq!(entry.requires_names.len(), 1);
    assert_ne!(
        entry.requires_names[0], "hlum",
        "requirement name must resolve to the localized name"
    );

    // A requirement can also be an upgrade id (tech tree entries).
    let melee = data.upgrade("Rhme").expect("Rhme");
    assert!(melee.name().is_some());
    assert_eq!(data.requirement_name("Rhme"), melee.name().unwrap());

    // Units without requirements expose an empty list (not a fake gate).
    let footman = data.unit("hfoo").expect("hfoo");
    assert!(footman.requires().is_empty(), "footman has no Requires entry");
}

/// Every producer/worker in the extraction is discoverable from the data.
#[test]
fn producers_and_builders_are_enumerated_from_the_tables() {
    let data = roc();
    let producers = data.producers();
    let builders = data.builders();
    // Measured on `analysis/war3_data/Units/*UnitFunc.txt` (see module docs).
    assert_eq!(producers.len(), 35, "producers with Trains=: {producers:?}");
    assert_eq!(builders.len(), 6, "workers with Builds=: {builders:?}");

    let builder_ids: Vec<&str> = builders.iter().map(|(id, _)| id.as_str()).collect();
    for worker in ["hpea", "opeo", "uaco", "ewsp"] {
        assert!(builder_ids.contains(&worker), "{worker} builds list");
    }
    let hall = producers
        .iter()
        .find(|(id, _)| id == "htow")
        .expect("town hall trains");
    assert_eq!(hall.1, 1, "htow trains one entry");
}

/// Menu entries are renderable without any hardcoded table: `hotkey_label`
/// and `cost_label` are pure data projections.
#[test]
fn menu_labels_are_data_projections() {
    let data = roc();
    let menu = data.train_menu("hbar");
    let knight = menu.iter().find(|entry| entry.id == "hkni").expect("knight");
    assert_eq!(knight.hotkey_label(), "K", "Hotkey=K");
    let label = knight.cost_label();
    assert!(label.contains('G'), "cost label has gold: {label}");
    assert!(label.contains("F"), "knight costs food: {label}");

    // Every trainee in the extraction has a hotkey and an icon path.
    let mut missing_hotkeys = Vec::new();
    let mut missing_icons = Vec::new();
    for (producer, _) in data.producers() {
        for entry in data.train_menu(&producer) {
            if entry.hotkey.is_none() {
                missing_hotkeys.push(format!("{}:{}", producer, entry.id));
            }
            if entry.icon.is_none() {
                missing_icons.push(format!("{}:{}", producer, entry.id));
            }
        }
    }
    // Retail gap, verified in `analysis/war3_data/Units/NeutralUnitFunc.txt`:
    // the three demon-gate trainees (`ndmg` → nfel/ninf/nbal) ship `Art` but no
    // `Hotkey=`, so the card must fall back to the id. Every other trainee has
    // one, and the gap is pinned so a new one cannot appear silently.
    assert_eq!(
        missing_hotkeys,
        ["ndmg:nfel", "ndmg:ninf", "ndmg:nbal"],
        "known retail gap only"
    );
    assert!(
        missing_icons.is_empty(),
        "trainees without an icon: {missing_icons:?}"
    );
}

/// Armour/damage parity: the menu entry and the unit view must agree with the
/// shipped `UnitBalance`/`UnitWeapons` columns for the same object.
#[test]
fn armour_and_damage_match_the_shipped_columns() {
    let data = roc();
    let footman = data.unit("hfoo").expect("hfoo");
    let stats = data.menu_entry("hfoo").expect("footman entry");
    assert_eq!(stats.gold, footman.gold_cost().ok().flatten());
    assert_eq!(stats.food, footman.food_used().ok().flatten());
    let info = data.object("hfoo").expect("footman info");
    assert_eq!(info.stats.armor, footman.armor_base().ok().flatten());
    assert_eq!(
        info.stats.max_damage,
        footman
            .weapon(wc3_data::units::WeaponSlot::Primary)
            .and_then(|weapon| weapon.max_damage().ok().flatten())
    );
    assert_eq!(info.stats.armor, Some(2), "UnitBalance def");
    assert_eq!(info.stats.average_damage, Some(12.5), "dice formula");
}
