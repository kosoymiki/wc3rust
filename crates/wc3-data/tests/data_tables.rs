//! Typed-table acceptance for the data layer (ledger 375).
//!
//! Every test maps to one `MECHANICS_GAP_MAP.md` row and carries its id in the
//! name (`TEST_POLICY.md`): `items.data`, `upgrades.data`, `items.drops`,
//! `items.shops`. Count assertions live in these tests only; they pin the real
//! TFT extraction (`analysis/war3x_data`).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use wc3_data::drops::drop_level_for_creep_level;
use wc3_data::source::DirSource;
use wc3_data::GameDatabase;

fn analysis_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis")
}

fn tft() -> &'static GameDatabase {
    static TFT: OnceLock<GameDatabase> = OnceLock::new();
    TFT.get_or_init(|| {
        GameDatabase::load(&DirSource::new(analysis_dir().join("war3x_data"))).expect("load TFT")
    })
}

/// `items.data` — the 273 TFT item rows are typed, with zero unparsed rows, and
/// the definition tables carry their full row counts (units 812, weapons 812).
#[test]
fn items_data_rows() {
    let data = tft();
    let defs = data.defs();

    // Four-table row contract (units/weapons/items) — items counted here only.
    assert_eq!(defs.units.len(), 812, "UnitData rows typed");
    assert_eq!(defs.weapons.len(), 812, "UnitWeapons rows typed");
    assert_eq!(defs.items.len(), 273, "ItemData rows typed");

    let unparsed_items: Vec<_> = defs
        .unparsed
        .iter()
        .filter(|row| row.family == "items")
        .collect();
    assert!(
        unparsed_items.is_empty(),
        "unparsed item rows: {unparsed_items:?}"
    );
    assert!(
        defs.unparsed.is_empty(),
        "unparsed rows: {:?}",
        defs.unparsed
    );

    // Every item carries the fields the gap map names: class, cost, charges,
    // ability and drop level.
    let with_class = defs.items.iter().filter(|item| item.class.is_some()).count();
    assert_eq!(with_class, 273, "class exposed");
    let with_level = defs
        .items
        .iter()
        .filter(|item| item.level.is_some())
        .count();
    assert_eq!(with_level, 273, "drop level (`Level`) exposed");
    let with_ability = defs
        .items
        .iter()
        .filter(|item| !item.abilities.is_empty())
        .count();
    assert_eq!(with_ability, 240, "item ability (`abilList`) exposed");
    let droppable = defs.items.iter().filter(|item| item.droppable).count();
    assert_eq!(droppable, 272, "creep-drop pool flag");

    // Spot values from the sheet: Scroll of Town Portal.
    let portal = defs
        .items
        .iter()
        .find(|item| item.id == "stwp")
        .expect("stwp");
    assert_eq!(portal.level, Some(2), "Level");
    assert_eq!(portal.gold, Some(350), "goldcost");
    assert_eq!(portal.charges, Some(1), "uses");
    assert_eq!(portal.abilities, ["AItp"], "abilList");
    assert_eq!(portal.cooldown_id.as_deref(), Some("AItp"), "cooldownID");
}

/// `items.usable` — all 273 items classified: usable / passive-stat /
/// data-only (campaign quest rows; retail carries no recipe/effect table).
#[test]
fn items_usable() {
    let data = tft();
    let usages = data.item_usages();
    assert_eq!(usages.len(), 273, "every item classified");
    let (usable, passive, data_only) = data.item_usage_counts();
    assert_eq!(usable, 133, "usable items (active use flag + ability)");
    assert_eq!(passive, 107, "passive stat/aura items");
    assert_eq!(data_only, 33, "campaign/quest rows without effect data");
    assert_eq!(usable + passive + data_only, 273, "zero unclassified");

    // Every usable item's ability resolves to sheet numbers (generic runner input).
    for info in usages.iter().filter(|info| info.usage == wc3_data::ItemUsage::Usable) {
        assert!(!info.abilities.is_empty(), "{} usable without ability", info.id);
        for ability in &info.abilities {
            let numbers = data
                .ability_numbers(ability)
                .unwrap_or_else(|| panic!("{} ability {ability} numbers", info.id));
            assert!(!numbers.levels.is_empty(), "{} {ability} levels", info.id);
        }
    }
    for info in usages.iter().filter(|info| info.usage == wc3_data::ItemUsage::PassiveStat) {
        assert!(!info.abilities.is_empty(), "{} passive without ability", info.id);
    }
    for info in usages.iter().filter(|info| info.usage == wc3_data::ItemUsage::DataOnly) {
        assert!(info.reason.is_some(), "{} data-only reason", info.id);
    }

    // Sheet samples: potion = usable, claws = passive, quest key = data-only.
    let potion = usages.iter().find(|info| info.id == "phea").expect("phea");
    assert_eq!(potion.usage, wc3_data::ItemUsage::Usable);
    let claws = usages.iter().find(|info| info.id == "ratc").expect("ratc");
    assert_eq!(claws.usage, wc3_data::ItemUsage::PassiveStat);
    let quest = usages.iter().find(|info| info.id == "azhr").expect("azhr");
    assert_eq!(quest.usage, wc3_data::ItemUsage::DataOnly);
}

/// `upgrades.data` — every `UpgradeData` row is typed with levels, costs,
/// effects or ability unlocks, and profile `Requires`; counted (89 TFT).
#[test]
fn upgrades_data_rows() {
    let data = tft();
    let defs = data.defs();

    assert_eq!(defs.upgrades.len(), 89, "UpgradeData rows typed");
    assert!(
        defs.unparsed.is_empty(),
        "unparsed rows: {:?}",
        defs.unparsed
    );

    let with_costs = defs
        .upgrades
        .iter()
        .filter(|upgrade| upgrade.gold_base.is_some() && upgrade.max_level.is_some())
        .count();
    assert_eq!(with_costs, 89, "levels + gold cost");
    let with_effects = defs
        .upgrades
        .iter()
        .filter(|upgrade| !upgrade.effects.is_empty())
        .count();
    assert_eq!(with_effects, 52, "stat effect slots");
    let with_unlocks = defs
        .upgrades
        .iter()
        .filter(|upgrade| !upgrade.unlocks.is_empty())
        .count();
    assert_eq!(with_unlocks, 54, "upgrades referenced by ability Requires");
    let unlock_only = defs
        .upgrades
        .iter()
        .filter(|upgrade| upgrade.effects.is_empty() && !upgrade.unlocks.is_empty())
        .count();
    assert_eq!(unlock_only, 37, "ability-unlock upgrades without a stat slot");
    let fully_effecting = defs
        .upgrades
        .iter()
        .filter(|upgrade| !upgrade.effects.is_empty() || !upgrade.unlocks.is_empty())
        .count();
    assert_eq!(fully_effecting, 89, "89/89 upgrades have an effect");
    let with_requires = defs
        .upgrades
        .iter()
        .filter(|upgrade| !upgrade.requires.is_empty())
        .count();
    assert!(with_requires > 0, "profile Requires exposed");

    // Steel Melee Weapons: 3 levels, 100 + 75/level gold, +1 damage/level.
    let rhme = defs
        .upgrades
        .iter()
        .find(|upgrade| upgrade.id == "Rhme")
        .expect("Rhme");
    assert_eq!(rhme.max_level, Some(3), "maxlevel");
    assert_eq!(rhme.gold_base, Some(100), "goldbase");
    assert_eq!(rhme.gold_per_level, Some(75), "goldmod");
    assert_eq!(rhme.effects[0].effect, "ratd", "effect1");
    assert_eq!(rhme.effects[0].base, Some(1.0), "base1");
    assert_eq!(rhme.effects[0].per_level, Some(1.0), "mod1");
    // Fractional effect values are real (`Rhac` armour: 0.2 per level).
    let rhac = defs
        .upgrades
        .iter()
        .find(|upgrade| upgrade.id == "Rhac")
        .expect("Rhac");
    assert_eq!(rhac.effects[0].effect, "rarm", "effect1");
    assert_eq!(rhac.effects[1].per_level, Some(0.2), "mod2 fractional");
    assert!(
        rhme.units.contains(&"hfoo".to_string()),
        "units carrying Rhme: {:?}",
        rhme.units
    );
}

/// `items.drops` — the drop tables are exposed from `ItemData.droppable` +
/// `Level`; the chance table is data-only because retail has none.
#[test]
fn items_drops() {
    let data = tft();
    let tables = data.drop_tables();

    assert!(!tables.is_empty(), "drop tables");
    let total: usize = tables.iter().map(|table| table.items.len()).sum();
    assert_eq!(total, 121, "droppable items with a drop level");
    assert_eq!(tables.len(), 8, "drop levels 1..=8");
    assert_eq!(
        data.drop_level_distribution(),
        vec![(1, 23), (2, 25), (3, 18), (4, 20), (5, 14), (6, 14), (7, 4), (8, 3)],
        "drop-level distribution"
    );
    for table in &tables {
        assert!(
            !table.items.is_empty(),
            "level {} table is empty",
            table.item_level
        );
        assert!(
            table.items.iter().all(|item| item.level == table.item_level),
            "level {} items carry their own level",
            table.item_level
        );
    }

    // Engine mapping (documented, not SLK): creep 1-2 -> item 1, 9-10 -> item 5.
    assert_eq!(drop_level_for_creep_level(1), Some(1));
    assert_eq!(drop_level_for_creep_level(2), Some(1));
    assert_eq!(drop_level_for_creep_level(5), Some(3));
    assert_eq!(drop_level_for_creep_level(10), Some(5));
    assert_eq!(drop_level_for_creep_level(11), None);

    let pool = data.drop_pool(5);
    assert!(!pool.is_empty(), "creep level 5 pool");
    assert!(
        data.drop_table(3).is_some_and(|table| !table.items.is_empty()),
        "explicit level-3 table"
    );

    // Honest boundary: no drop-chance table exists in the install.
    assert!(
        data.drops_data_only_reason().is_some(),
        "chance table must be reported as data-only"
    );
}

/// `items.shops` — stock/restock fields from `ItemData` and `Sellunits` from the
/// unit profile are typed and counted.
#[test]
fn items_shops() {
    let data = tft();
    let shop = data.shop("ngme").expect("goblin merchant");

    assert!(shop.items.len() >= 11, "shop lines: {}", shop.items.len());
    let portal = shop
        .items
        .iter()
        .find(|item| item.item_id == "stwp")
        .expect("town portal in TFT stock");
    assert_eq!(portal.stock_max, Some(2), "stockMax");
    assert_eq!(portal.stock_regen, Some(120), "stockRegen (restock interval)");
    assert_eq!(portal.stock_start, Some(440), "stockStart (initial stock)");
    assert_eq!(portal.gold, Some(350), "goldcost");
    assert_eq!(portal.charges, Some(1), "uses");

    assert_eq!(data.sold_units("hshy"), ["hbot"], "Sellunits");
    assert_eq!(
        data.shops_selling_unit("hbot"),
        ["hshy"],
        "reverse Sellunits lookup"
    );
}

/// `combat.buffs` — 148 buff definitions with their applying ability
/// (`B`+tail id rule, then a unique shared icon); the rest are reported
/// data-only because the applying ability is not in this install.
#[test]
fn combat_buffs_data() {
    let data = tft();
    let defs = data.defs();
    assert_eq!(defs.buffs.len(), 148, "buff sections with Buffart");

    let by_id = defs
        .buffs
        .iter()
        .filter(|buff| buff.matched_by == Some("id"))
        .count();
    let by_icon = defs
        .buffs
        .iter()
        .filter(|buff| buff.matched_by == Some("icon"))
        .count();
    let data_only = defs
        .buffs
        .iter()
        .filter(|buff| buff.ability.is_none())
        .count();
    assert_eq!(by_id, 115, "buff id -> ability id tail");
    assert_eq!(by_icon, 10, "unique shared icon");
    assert_eq!(by_id + by_icon + data_only, 148, "zero unclassified buffs");

    let with_art = defs.buffs.iter().filter(|buff| buff.art.is_some()).count();
    assert_eq!(with_art, 147, "buff art (Barm ships an empty Buffart)");
    for buff in &defs.buffs {
        if let Some(ability) = &buff.ability {
            assert!(
                data.ability_numbers(ability).is_some(),
                "{} applied by {ability} (in AbilityData)",
                buff.id
            );
        }
    }
}

/// `economy.lumber` — every doodad id placed on the shipped Ponds melee map
/// resolves to a destructable/doodad def (after the map's own object-data
/// overrides are applied) with model, pathing and, for destructables, life;
/// trees that carry a `texFile` also expose the texture (others embed it in the
/// MDX). Map overrides apply to the typed tables with entry counts.
#[test]
fn economy_lumber_doodad_defs() {
    use std::collections::BTreeSet;

    use wc3_formats::map::{map_mpq_base, parse_doodads};
    use wc3_mpq::MpqArchive;

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let map = root.join(
        "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/(2)Ponds_v1.4.w3x",
    );
    assert!(map.is_file(), "Ponds fixture missing: {}", map.display());
    let base = map_mpq_base(&map).expect("mpq base");
    let archive = MpqArchive::open_at(&map, base).expect("open Ponds");
    let doodads = parse_doodads(&archive.read("war3map.doo").expect("war3map.doo"))
        .expect("parse war3map.doo");
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for doodad in &doodads {
        ids.insert(String::from_utf8_lossy(&doodad.type_id).trim_end_matches('\0').to_string());
    }
    assert_eq!(doodads.len(), 5767, "placed doodads");
    assert_eq!(ids.len(), 67, "distinct doodad ids");

    // Apply the map's object data to the typed tables first: custom doodads
    // (`ZPf0`, `LTrt`) exist only in these overrides.
    let mut library = GameDatabase::load(&DirSource::new(analysis_dir().join("war3x_data")))
        .expect("reload TFT");
    let mut applied = Vec::new();
    for name in [
        "war3map.w3u",
        "war3map.w3t",
        "war3map.w3b",
        "war3map.w3d",
        "war3map.w3a",
        "war3map.w3q",
    ] {
        if let Ok(bytes) = archive.read(name) {
            let kind = library
                .apply_object_data_named(name, &bytes)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            applied.push((name, kind, bytes.len()));
        }
    }
    for merge in library.merges() {
        let parsed = merge.base_entries + merge.custom_entries;
        let reported = merge.report.applied + merge.report.shadow + merge.report.profile;
        assert_eq!(
            parsed,
            reported + merge.report.unknown_field.len() + merge.report.missing_sheet.len(),
            "{}: every parsed entry is applied, shadowed or reported",
            merge.source
        );
    }

    let object_files = [
        "war3map.w3u",
        "war3map.w3t",
        "war3map.w3b",
        "war3map.w3d",
        "war3map.w3a",
        "war3map.w3q",
    ];
    let map_files: Vec<&str> = archive
        .listfile()
        .iter()
        .map(String::as_str)
        .filter(|name| object_files.contains(name))
        .collect();
    assert!(map_files.is_empty(), "Ponds carries object data: {map_files:?}");
    assert!(applied.is_empty(), "no override files to apply");
    assert!(library.merges().is_empty(), "no merges on a clean map");
    let defs = library.defs();
    let mut unresolved = Vec::new();
    let mut no_texture = Vec::new();
    for id in &ids {
        if let Some(def) = defs.destructables.iter().find(|def| &def.id == id) {
            assert!(def.model.is_some(), "{id} destructable model");
            assert!(def.path_texture.is_some(), "{id} destructable pathing");
            assert!(def.hp.is_some(), "{id} destructable life");
            if def.texture.is_none() {
                // Texture is baked into the MDX; recorded, not invented.
                no_texture.push(id.clone());
            }
        } else if let Some(def) = defs.doodads.iter().find(|def| &def.id == id) {
            assert!(def.model.is_some(), "{id} doodad model");
            assert!(def.path_texture.is_some(), "{id} doodad pathing");
        } else {
            unresolved.push(id.clone());
        }
    }
    assert_eq!(
        unresolved,
        vec!["LTrt".to_string(), "ZPf0".to_string()],
        "placed ids undefined in every shipped SLK (Ponds has no w3d to define them)"
    );
    assert_eq!(ids.len() - 2, 65, "65 of 67 ids resolve to a def");
    assert_eq!(
        no_texture,
        vec![
            "LOcg", "LTba", "LTbr", "LTbs", "LTbx", "LTcr", "LTex", "LTrc"
        ],
        "destructables whose texture is embedded in the MDX"
    );
    eprintln!(
        "PONDS doodads={} ids={} resolved=65 unresolved={unresolved:?} embedded_texture={no_texture:?}",
        doodads.len(),
        ids.len()
    );
}
