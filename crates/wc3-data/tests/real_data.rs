//! Tests against the real extracted WC3 data in `analysis/`.
//!
//! Every test parses the shipped SLK/TXT tables (RoC: `analysis/war3_data`,
//! TFT: `analysis/war3x_data`) and asserts counts and invariants that were
//! measured on those exact files. Nothing here uses synthetic fixtures: the
//! numbers are the ones the retail data has.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use wc3_data::source::{DirSource, Listfile};
use wc3_data::table::TableKind;
use wc3_data::units::{Classification, WeaponSlot};
use wc3_data::{GameData, DataError};

fn analysis_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis")
}

fn load(root: &str, listfiles: &[&str]) -> GameData {
    let source = DirSource::new(analysis_dir().join(root));
    let mut data = GameData::load(&source).expect("load game data");
    let mut catalog: Option<Listfile> = None;
    for name in listfiles {
        let listfile = Listfile::from_file(analysis_dir().join(name)).expect("read listfile");
        match catalog.as_mut() {
            Some(existing) => existing.extend(&listfile),
            None => catalog = Some(listfile),
        }
    }
    if let Some(catalog) = catalog {
        data.set_catalog(catalog);
    }
    data
}

const ROC_ROOT: &str = "war3_data";
const TFT_ROOT: &str = "war3x_data";
const ROC_LISTFILE: &str = "list-War3-clean.txt";
const TFT_LISTFILE: &str = "list-Files100_War3x.mpq-clean.txt";

fn roc() -> &'static GameData {
    static ROC: OnceLock<GameData> = OnceLock::new();
    ROC.get_or_init(|| load(ROC_ROOT, &[ROC_LISTFILE]))
}

fn tft() -> &'static GameData {
    static TFT: OnceLock<GameData> = OnceLock::new();
    TFT.get_or_init(|| load(TFT_ROOT, &[TFT_LISTFILE, ROC_LISTFILE]))
}

#[test]
fn roc_table_counts_match_the_extraction() {
    let data = roc();
    let expected: &[(TableKind, usize, usize)] = &[
        // (table, typed rows, positional rows declared by `B;` minus header)
        (TableKind::UnitData, 466, 466),
        (TableKind::UnitBalance, 466, 466),
        (TableKind::UnitWeapons, 316, 316),
        (TableKind::UnitAbilities, 324, 324),
        (TableKind::UnitUi, 466, 466),
        (TableKind::AbilityData, 397, 399),
        (TableKind::ItemData, 132, 132),
        (TableKind::UpgradeData, 58, 58),
        (TableKind::DestructableData, 128, 128),
        (TableKind::Doodads, 278, 278),
    ];
    for (kind, rows, declared) in expected {
        let table = data.table(*kind).unwrap_or_else(|| panic!("{kind:?} missing"));
        assert_eq!(table.len(), *rows, "{} rows", kind.name());
        let declared_rows = table.sheet().declared_rows().expect("bounds") - 1;
        assert_eq!(declared_rows, *declared, "{} declared rows", kind.name());
    }
    // AbilityData carries two spacer rows without an identifier.
    let abilities = data.table(TableKind::AbilityData).expect("abilities");
    assert_eq!(abilities.sheet().skipped_rows(), 2);
    assert_eq!(abilities.len() + abilities.sheet().skipped_rows(), 399);
    // Every other sheet is gap-free, so typed rows equal declared rows.
    assert_eq!(data.table(TableKind::UnitData).unwrap().sheet().skipped_rows(), 0);
}

#[test]
fn tft_table_counts_match_the_extraction() {
    let data = tft();
    let expected: &[(TableKind, usize, usize)] = &[
        (TableKind::UnitData, 812, 812),
        (TableKind::UnitBalance, 812, 812),
        (TableKind::UnitWeapons, 812, 812),
        (TableKind::UnitAbilities, 812, 812),
        (TableKind::UnitUi, 812, 812),
        (TableKind::AbilityData, 763, 767),
        (TableKind::ItemData, 273, 273),
        (TableKind::UpgradeData, 89, 89),
        (TableKind::DestructableData, 246, 246),
        (TableKind::Doodads, 469, 469),
    ];
    for (kind, rows, declared) in expected {
        let table = data.table(*kind).unwrap_or_else(|| panic!("{kind:?} missing"));
        assert_eq!(table.len(), *rows, "{} rows", kind.name());
        assert_eq!(
            table.sheet().declared_rows().expect("bounds") - 1,
            *declared,
            "{} declared rows",
            kind.name()
        );
    }
    let abilities = data.table(TableKind::AbilityData).expect("abilities");
    assert_eq!(abilities.sheet().skipped_rows(), 4);
}

#[test]
fn unit_sheet_sets_are_consistent() {
    let data = roc();
    let ids = |kind: TableKind| -> Vec<String> {
        data.table(kind).unwrap().ids().to_vec()
    };
    let set = |kind: TableKind| -> std::collections::BTreeSet<String> {
        ids(kind).into_iter().collect()
    };
    let units = set(TableKind::UnitData);
    assert_eq!(units, set(TableKind::UnitBalance));
    assert_eq!(units, set(TableKind::UnitUi));

    let weapons = set(TableKind::UnitWeapons);
    assert_eq!(units.len() - weapons.len(), 150, "units without a weapon row");
    assert!(weapons.is_subset(&units), "every weapon row belongs to a unit");

    let abilities = set(TableKind::UnitAbilities);
    let missing: Vec<&String> = units.difference(&abilities).collect();
    let extra: Vec<&String> = abilities.difference(&units).collect();
    assert_eq!(missing.len(), 145);
    assert_eq!(extra.len(), 3);
    assert_eq!(
        extra.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        vec!["edol", "htws", "ugsp"]
    );
}

#[test]
fn footman_reads_real_values() {
    let data = roc();
    let unit = data.unit("hfoo").expect("hfoo");
    assert_eq!(unit.name().as_deref(), Some("Пехотинец"));
    assert_eq!(unit.race(), Some("human"));
    assert_eq!(unit.hp().unwrap(), Some(420));
    assert_eq!(unit.gold_cost().unwrap(), Some(160));
    assert_eq!(unit.lumber_cost().unwrap(), Some(0));
    assert_eq!(unit.build_time().unwrap(), Some(20));
    assert_eq!(unit.level().unwrap(), Some(2));
    assert_eq!(unit.armor_base().unwrap(), Some(2));
    assert_eq!(unit.armor_per_upgrade().unwrap(), Some(2));
    assert_eq!(unit.armor_with_upgrades(3).unwrap(), Some(8));
    assert_eq!(unit.speed().unwrap(), Some(270));
    assert_eq!(unit.food_used().unwrap(), Some(2));
    assert_eq!(unit.classification().unwrap(), Classification::Unit);
    assert_eq!(unit.model_file().as_deref(), Some("units\\human\\Footman\\Footman.mdx"));
    assert_eq!(unit.icon().as_deref(), Some("ReplaceableTextures\\CommandButtons\\BTNFootman.blp"));
    assert_eq!(unit.hotkey().as_deref(), Some("F"));
    assert_eq!(unit.button_pos(), Some((0, 0)));
    assert_eq!(unit.abilities(), ["Adef"]);
    assert_eq!(unit.upgrades(), ["Rhar", "Rhme", "Rhde"]);

    let weapon = unit.weapon(WeaponSlot::Primary).expect("primary weapon");
    assert_eq!(weapon.weapon_type(), Some("normal"));
    assert_eq!(weapon.attack_type(), Some("normal"));
    assert_eq!(weapon.range().unwrap(), Some(90));
    assert_eq!(weapon.min_damage().unwrap(), Some(12));
    assert_eq!(weapon.max_damage().unwrap(), Some(13));
    assert_eq!(weapon.average_damage().unwrap(), Some(12.5));
    assert_eq!(weapon.cooldown().unwrap(), Some(1.35));
}

#[test]
fn town_hall_tiers_follow_the_building_class_index() {
    let data = roc();
    let cases = [
        ("htow", 1, "HBuilding01", "human"),
        ("hkee", 2, "HBuilding02", "human"),
        ("hcas", 3, "HBuilding03", "human"),
        ("ogre", 1, "OBuilding01", "orc"),
        ("ostr", 2, "OBuilding02", "orc"),
        ("ofrt", 3, "OBuilding03", "orc"),
        ("etol", 1, "EBuilding01", "nightelf"),
        ("etoa", 2, "EBuilding02", "nightelf"),
        ("etoe", 3, "EBuilding03", "nightelf"),
        ("unpl", 1, "UBuilding01", "undead"),
        ("unp1", 2, "UBuilding02", "undead"),
        ("unp2", 3, "UBuilding03", "undead"),
    ];
    for (id, tier, class, race) in cases {
        let unit = data.unit(id).unwrap_or_else(|| panic!("{id} missing"));
        assert!(unit.is_town_hall(), "{id} must be a town hall");
        assert_eq!(unit.hall_tier(), Some(tier), "{id} tier");
        assert_eq!(unit.unit_class(), Some(class), "{id} class");
        assert_eq!(unit.race(), Some(race), "{id} race");
        assert!(unit.is_building().unwrap(), "{id} must be a building");
    }
    // A non-hall building has a class index but no tier.
    let aviary = data.unit("hgra").expect("gryphon aviary");
    assert_eq!(aviary.hall_tier(), None);
    assert_eq!(aviary.building_class_index(), Some(11));
    // A unit has no building class at all.
    assert_eq!(data.unit("hfoo").unwrap().building_class_index(), None);
}

#[test]
fn weapon_damage_formula_matches_shipped_columns() {
    // The sheets carry precomputed mindmg/avgdmg/maxdmg. The dice formula must
    // reproduce them; the single exception is the retail `nbal` row, whose
    // stored `avgdmg2`/`maxdmg2` contradict its own dice.
    for data in [roc(), tft()] {
        let weapons = data.table(TableKind::UnitWeapons).expect("weapons");
        let mut avg_mismatch = Vec::new();
        let mut max_mismatch = Vec::new();
        for id in weapons.ids() {
            let Some(unit) = data.unit(id) else { continue };
            for slot in [WeaponSlot::Primary, WeaponSlot::Secondary] {
                let Some(weapon) = unit.weapon(slot) else { continue };
                let suffix = slot.suffix();
                if let (Ok(Some(computed)), Ok(Some(stored))) = (
                    weapon.average_damage(),
                    weapons.f32(id, &format!("avgdmg{suffix}")),
                ) {
                    if (computed - stored).abs() > 0.01 {
                        avg_mismatch.push(format!("{id}:{suffix} {computed} != {stored}"));
                    }
                }
                if let (Ok(Some(computed)), Ok(Some(stored))) = (
                    weapon.max_damage(),
                    weapons.f32(id, &format!("maxdmg{suffix}")),
                ) {
                    if (computed as f32 - stored).abs() > 0.01 {
                        max_mismatch.push(format!("{id}:{suffix} {computed} != {stored}"));
                    }
                }
            }
        }
        assert!(
            avg_mismatch.len() <= 1,
            "{} average mismatches: {avg_mismatch:?}",
            data.root()
        );
        assert!(
            max_mismatch.len() <= 1,
            "{} maximum mismatches: {max_mismatch:?}",
            data.root()
        );
        for entry in avg_mismatch.iter().chain(max_mismatch.iter()) {
            assert!(entry.starts_with("nbal:"), "unexpected mismatch {entry}");
        }
    }
}

#[test]
fn roc_models_and_icons_resolve_against_the_listfile() {
    let data = roc();
    let report = data.validate();
    let path = |class: &str| report.path(class).unwrap_or_else(|| panic!("{class} missing"));

    // 318 non-building units + 148 buildings = all 466 unit rows.
    let units = path("unit-model");
    assert_eq!(units.checked, 318);
    assert_eq!(units.missing, 0);
    let buildings = path("building-model");
    assert_eq!(buildings.checked, 148);
    assert_eq!(buildings.missing, 0);

    // One icon is broken in the shipped data: `Art=Art=Replaceable…` for zshv.
    let icons = path("unit-icon");
    assert_eq!(icons.checked, 465);
    assert_eq!(icons.missing, 1);
    assert_eq!(icons.samples, vec!["zshv: Art=ReplaceableTextures\\CommandButtons\\BTNTemp.blp"]);

    let item_icons = path("item-icon");
    assert_eq!((item_icons.checked, item_icons.missing), (132, 0));
    let item_models = path("item-model");
    assert_eq!((item_models.checked, item_models.missing), (132, 0));

    let destructables = path("destructable-model");
    assert_eq!((destructables.checked, destructables.missing), (128, 0));
    let doodads = path("doodad-model");
    assert_eq!((doodads.checked, doodads.missing), (278, 0));
    // Command icons that name an engine command instead of a file.
    let ability_icons = path("ability-icon");
    assert_eq!(ability_icons.identifiers, 1);

    let catalog = report.catalog.expect("catalog");
    assert_eq!(catalog.entries, 10_523);
    assert_eq!(catalog.paths, 10_523);
    assert_eq!(catalog.unit_models, 411);
    assert_eq!(catalog.building_models, 131);
    assert_eq!(catalog.command_button_icons, 691);
}

#[test]
fn destructables_and_doodads_resolve_model_variations() {
    let data = roc();
    let tree = data.destructable("LTlt").expect("LTlt");
    assert_eq!(tree.target_type(), Some("tree"));
    assert_eq!(tree.armor_type(), Some("Wood"));
    assert_eq!(tree.hp().unwrap(), Some(50));
    assert_eq!(tree.variations().unwrap(), Some(10));
    assert_eq!(
        tree.model_file().as_deref(),
        Some("Doodads\\Terrain\\LordaeronTree\\LordaeronTree.mdx")
    );
    assert_eq!(
        tree.resolved_model().as_deref(),
        Some("doodads/terrain/lordaerontree/lordaerontree0.mdx")
    );
    assert_eq!(tree.model_exists(), Some(true));

    let plant = data.doodad("APct").expect("APct");
    assert_eq!(plant.category(), Some("E"));
    assert_eq!(
        plant.resolved_model().as_deref(),
        Some("doodads/ashenvale/plants/ashencattail/ashencattail.mdx")
    );
    assert_eq!(plant.default_scale().unwrap(), Some(1.0));
}

#[test]
fn roc_validation_is_error_free_and_reports_known_warnings() {
    let data = roc();
    let report = data.validate();
    assert_eq!(report.error_count(), 0, "{:?}", report.errors().collect::<Vec<_>>());
    assert_eq!(report.total_rows(), 466 + 466 + 316 + 324 + 466 + 397 + 132 + 58 + 128 + 278);

    let count = |code: &str| {
        report
            .issues
            .iter()
            .filter(|issue| issue.code == code)
            .count()
    };
    // Real defects in the shipped RoC tables.
    assert_eq!(count("column.artifact"), 97, "Excel/FourCC artifacts in cells");
    assert_eq!(count("column.trailing-garbage"), 0);
    assert_eq!(count("column.bad-value"), 0);
    assert_eq!(count("ref.ability-missing"), 6, "Abilities only TFT defines");
    assert_eq!(count("ref.upgrade-missing"), 2, "Upgrades only TFT defines");
    assert_eq!(count("path.icon-missing"), 1, "zshv Art= typo");
    assert_eq!(count("weapon.avg-mismatch"), 1, "nbal dice");
    assert_eq!(count("weapon.max-mismatch"), 1, "nbal dice");
    assert_eq!(count("table.duplicate-id"), 0);

    // The trailing-garbage cells still read as numbers for gameplay.
    let sorceress = data.unit("hsor").expect("hsor");
    assert_eq!(sorceress.mana_regen().unwrap(), Some(0.5));
}

#[test]
fn tft_validation_is_error_free() {
    let data = tft();
    let report = data.validate();
    assert_eq!(report.error_count(), 0, "{:?}", report.errors().collect::<Vec<_>>());
    assert_eq!(report.total_rows(), 812 * 5 + 763 + 273 + 89 + 246 + 469);

    // TFT ships exactly one deliberate placeholder model.
    let units = report.path("unit-model").expect("unit models");
    assert_eq!(units.checked, 607);
    assert_eq!(units.missing, 1);
    assert!(units.samples[0].contains("InvalidFileOnPurpose.mdx"));
    assert_eq!(report.path("destructable-model").unwrap().missing, 0);
    assert_eq!(report.path("doodad-model").unwrap().missing, 0);
}

#[test]
fn ability_levels_come_from_the_real_columns() {
    let roc_data = roc();
    let blizzard = roc_data.ability("AHbz").expect("AHbz");
    assert_eq!(blizzard.name().as_deref(), Some("Буран"));
    assert_eq!(blizzard.max_level(), 3);
    assert_eq!(blizzard.declared_levels().unwrap(), Some(3));
    let level1 = blizzard.level(1).unwrap().expect("level 1");
    assert_eq!(level1.cooldown, Some(6.0));
    assert_eq!(level1.mana_cost, Some(75));
    assert_eq!(level1.area, Some(200.0));
    assert_eq!(level1.range, Some(800.0));
    // RoC stores `Data11..Data14` = level 1, slots 1..4.
    assert_eq!(
        level1.data,
        vec![
            Some("6".to_string()),
            Some("30".to_string()),
            Some("6".to_string()),
            Some("0.5".to_string())
        ]
    );
    assert_eq!(blizzard.level(3).unwrap().unwrap().area, Some(250.0));
    assert_eq!(blizzard.area(2).unwrap(), Some(225.0));
    assert!(!blizzard.targets_air());

    let tft_data = tft();
    let tft_blizzard = tft_data.ability("AHbz").expect("AHbz (TFT)");
    // TFT has four level *columns* but the entry declares three levels.
    assert_eq!(tft_blizzard.max_level(), 4);
    assert_eq!(tft_blizzard.declared_levels().unwrap(), Some(3));
    assert_eq!(tft_blizzard.level(4).unwrap().unwrap().area, Some(200.0));
    // TFT stores `DataA1..DataE4` = slot letter, level.
    assert_eq!(
        tft_blizzard.level(1).unwrap().unwrap().data,
        vec![
            Some("6".to_string()),
            Some("30".to_string()),
            Some("6".to_string()),
            Some("0.5".to_string()),
            Some("0".to_string())
        ]
    );
    // Target flags are real flags from `targs`.
    let holy_light = roc_data.ability("Ahea").expect("Ahea");
    assert!(holy_light.targets_allies());
    assert!(!holy_light.targets_enemies());
}

#[test]
fn upgrades_expose_level_costs_and_effects() {
    let data = roc();
    let melee = data.upgrade("Rhme").expect("Rhme");
    assert_eq!(melee.class(), Some("melee"));
    assert_eq!(melee.race(), Some("human"));
    assert_eq!(melee.max_level().unwrap(), Some(3));
    assert_eq!(
        [
            melee.gold_cost(1).unwrap(),
            melee.gold_cost(2).unwrap(),
            melee.gold_cost(3).unwrap()
        ],
        [Some(100), Some(250), Some(400)]
    );
    assert_eq!(
        [
            melee.lumber_cost(1).unwrap(),
            melee.lumber_cost(2).unwrap(),
            melee.lumber_cost(3).unwrap()
        ],
        [Some(50), Some(100), Some(150)]
    );
    assert_eq!(melee.research_time(3).unwrap(), Some(90));
    let effects = melee.active_effects().unwrap();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].code.as_deref(), Some("ratd"));
    assert_eq!(effects[0].level_value(1), Some(1.0));
    assert_eq!(effects[0].level_value(3), Some(3.0));
    // Per-level name list from the profile text.
    assert_eq!(
        data.localizer().text_level("Rhme", "Name", 2).as_deref(),
        Some("Стальные мечи")
    );
    assert_eq!(
        data.localizer().component("Rhme", "Name", 0).as_deref(),
        Some("Железные мечи")
    );
    // TFT ships the effect dictionary, RoC does not.
    assert!(data.upgrade_effect(Some("ratd")).is_none());
    let tft_effect = tft().upgrade_effect(Some("ratd")).expect("TFT effect");
    assert!(tft_effect.0.is_some(), "comment should exist in TFT");
}

#[test]
fn items_expose_stats_charges_and_abilities() {
    let data = roc();
    let claws = data.item("ratc").expect("ratc");
    assert_eq!(claws.name().as_deref(), Some("Боевые когти (+12)"));
    assert_eq!(claws.class(), Some("ringa"));
    assert_eq!(claws.level().unwrap(), Some(7));
    assert_eq!(claws.gold_cost().unwrap(), Some(500));
    assert_eq!(claws.hp().unwrap(), Some(100));
    assert_eq!(claws.armor_material(), Some("Wood"));
    // `uses=-` means unlimited charges.
    assert_eq!(claws.charges().unwrap(), None);
    assert_eq!(claws.abilities(), ["AItc"]);
    assert_eq!(
        claws.icon().as_deref(),
        Some("ReplaceableTextures\\CommandButtons\\BTNClawsOfAttack.blp")
    );

    // potion of healing: one charge, usable
    let potion = data.item("phea").expect("phea");
    assert_eq!(potion.charges().unwrap(), Some(1));
    assert!(potion.usable().unwrap());
    assert!(potion.perishable().unwrap());
}

#[test]
fn object_data_merges_into_the_typed_tables() {
    let mut data = load(ROC_ROOT, &[ROC_LISTFILE]);
    let wts = std::fs::read(analysis_dir().join("war3_map/Human04/war3map.wts")).expect("wts");
    data.set_wts(&wts).expect("parse wts");
    let bytes = std::fs::read(analysis_dir().join("war3_map/Human04/war3map.w3u")).expect("w3u");
    let merge = data
        .apply_object_data("war3map.w3u", wc3_formats::gamedata::ObjectKind::Unit, &bytes)
        .expect("merge")
        .clone();
    assert_eq!(merge.base_entries, 4);
    assert_eq!(merge.custom_entries, 4);
    assert_eq!(merge.report.applied, 4, "isst edits land in ItemData stockStart");
    assert_eq!(merge.report.profile, 4);
    assert_eq!(merge.report.created.len(), 16);
    assert!(merge.report.unknown_field.is_empty());

    // Base edits: item stock start set to 0 for the four map items.
    let item = data.item("phea").expect("phea");
    assert_eq!(item.stock_start().unwrap(), Some(0));

    // Custom units inherit the prototype row.
    let custom = data.unit("uC01").expect("uC01");
    assert_eq!(custom.name().as_deref(), Some("Тимми"), "TRIGSTR_087 resolved via wts");
    assert_eq!(
        custom.model_file().as_deref(),
        Some("units\\undead\\Ghoul\\Ghoul.mdx")
    );
    assert_eq!(custom.hp().unwrap(), Some(330));
    for id in ["nC00", "nC02", "nC03"] {
        assert!(data.unit(id).is_some(), "{id} must exist after the merge");
    }
    // Custom unit without a base weapon sheet can still be queried.
    assert!(data.unit("nC00").unwrap().weapon(WeaponSlot::Primary).is_none());

    // The report records the merge.
    assert_eq!(data.merges().len(), 1);
    assert_eq!(data.object_data().len(), 1);
    let validation = data.validate();
    assert_eq!(validation.error_count(), 0);
}

#[test]
fn shared_slk_parser_accepts_every_sheet() {
    // `Sheet::parse` runs `wc3_formats::slk::SlkTable` first as a well-formedness
    // gate, so this also proves the shared parser accepts all 90 shipped sheets.
    let mut checked = 0;
    for root in [ROC_ROOT, TFT_ROOT] {
        let dir = analysis_dir().join(root);
        let mut stack = vec![dir.clone()];
        while let Some(current) = stack.pop() {
            for entry in std::fs::read_dir(&current).expect("read dir").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|ext| ext.to_str()) != Some("slk") {
                    continue;
                }
                let bytes = std::fs::read(&path).expect("read slk");
                let strict = wc3_formats::gamedata::Sheet::parse(path.display().to_string(), &bytes)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                let (columns, _rows) = wc3_formats::gamedata::Sheet::legacy_view(&bytes)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                assert_eq!(
                    strict.columns(),
                    columns.as_slice(),
                    "{} column headers",
                    path.display()
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 33 + 57);
}

#[test]
fn localization_loads_profiles_and_resolves_text() {
    let data = roc();
    assert_eq!(data.localizer().loaded_files().len(), 39);
    assert_eq!(data.localizer().missing_files().len(), 7);
    assert_eq!(data.localizer().name("hfoo").as_deref(), Some("Пехотинец"));
    // Per-level tooltips are quoted lists; splitting must respect the quotes.
    let components = data.localizer().components("AHbz", "Ubertip");
    assert_eq!(components.len(), 3);
    // The shipped tooltip carries `<AHbz,Data11>` placeholders per level.
    assert!(components[0].contains("<AHbz,Data11>"));
    let blizzard = data.ability("AHbz").expect("AHbz");
    let level1_tooltip = blizzard.resolved_ubertip_level(1).expect("level 1 tooltip");
    assert!(level1_tooltip.contains("Вызов 6 снежных вихрей"), "{level1_tooltip}");
    assert!(level1_tooltip.contains("по 30 ед. урона"), "{level1_tooltip}");
    assert!(!level1_tooltip.contains('<'), "all placeholders resolved");
    assert!(blizzard
        .resolved_ubertip_level(2)
        .expect("level 2")
        .contains("Вызов 8 снежных вихрей"));
    assert_eq!(
        data.localizer().component("AHbz", "Buttonpos", 1).as_deref(),
        Some("2")
    );
    // TFT ships only the functional text files; localised names live in the
    // separate `war3xlocal.mpq` (extracted at `analysis/war3x_local`), which this
    // single-source loader does not layer in.
    assert_eq!(tft().localizer().name("hfoo"), None);
    assert_eq!(tft().localizer().hotkey("hfoo"), None);
}

#[test]
fn misc_tables_and_enums_are_available() {
    let data = roc();
    let stats = data.misc().stats();
    assert_eq!(stats.len(), 5, "{}", data.misc().files().join(", "));
    let unit_editor = stats
        .iter()
        .find(|stat| stat.file.ends_with("UnitEditorData.txt"))
        .expect("UnitEditorData");
    assert_eq!(unit_editor.sections, 12);

    let enums = data.enums().expect("enums");
    assert_eq!(enums.len(), 12);
    assert_eq!(
        enums.display("weaponType", "missile"),
        Some("WESTRING_UE_WEAPONTYPE_MISSILE")
    );
    assert_eq!(
        enums.decode("attackType", "pierce").map(|value| value.data.as_str()),
        Some("pierce")
    );
    // TFT adds the `hero` attack type that RoC does not have.
    assert!(enums.decode("attackType", "hero").is_none());
    assert!(tft().enums().expect("tft enums").decode("attackType", "hero").is_some());
    // MiscData carries the gameplay constants table.
    assert_eq!(
        data.misc().get("UI\\MiscData.txt", "Misc", "GoldTextLifetime"),
        Some("2")
    );
}

#[test]
fn buffs_have_no_shipped_base_table() {
    let data = roc();
    assert!(!data.buffs().is_base_available());
    assert_eq!(data.buffs().overlay_len(), 0);
}

#[test]
fn lookups_are_case_sensitive_on_ids_but_columns_are_not() {
    let data = roc();
    assert!(data.unit("hfoo").is_some());
    assert!(data.unit("HFOO").is_none(), "ids are FourCCs, not case-folded");
    let table = data.table(TableKind::UnitBalance).expect("balance");
    assert_eq!(table.i32("hfoo", "HP").unwrap(), Some(420));
    assert_eq!(table.i32("hfoo", "hp").unwrap(), Some(420));
    assert_eq!(table.text("hfoo", "hp"), Some("420"));
    assert!(matches!(
        table.i32("hfoo", "no-such-column"),
        Ok(None)
    ));
}

#[test]
fn missing_files_are_reported_not_fatal() {
    let source = DirSource::new(analysis_dir().join(ROC_ROOT));
    // A bogus root must fail loudly.
    let missing = GameData::load(&DirSource::new("/nonexistent/wc3"));
    assert!(matches!(missing, Err(DataError::Missing { .. })));
    // The real root records which optional files it does not ship.
    let data = GameData::load(&source).expect("load");
    assert!(data.missing().is_empty(), "RoC ships all ten tables");
}
