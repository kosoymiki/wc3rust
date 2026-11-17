//! ROUND8 parity: hero ability catalog (`PARITY_MATRIX.md` row 11) and item
//! shop/charges/abilities (row 14), read from the real RoC/TFT extractions.
//!
//! Row 11 — every ability a hero's `heroAbilList` references must carry the
//! sheet's own mana/cooldown/range/area for every learnable level, so the sim and
//! the hero UI never fall back to constants.
//!
//! Row 14 — shops come from the units' `Sellitems` profile lists, charges from
//! `ItemData.uses`, item abilities from `ItemData.abilList` resolved against
//! `AbilityData`, and purchase gating from `ItemFunc.txt`
//! `Requires`/`DependencyOr`. Combination recipes are **not** retail data; the
//! last test pins that boundary instead of inventing a table.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use wc3_data::source::{DirSource, Listfile};
use wc3_data::table::TableKind;
use wc3_data::GameDatabase;

fn analysis_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis")
}

fn load(root: &str, listfiles: &[&str]) -> GameDatabase {
    let source = DirSource::new(analysis_dir().join(root));
    let mut data = GameDatabase::load(&source).expect("load game data");
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

fn roc() -> &'static GameDatabase {
    static ROC: OnceLock<GameDatabase> = OnceLock::new();
    ROC.get_or_init(|| load("war3_data", &["list-War3-clean.txt"]))
}

fn tft() -> &'static GameDatabase {
    static TFT: OnceLock<GameDatabase> = OnceLock::new();
    TFT.get_or_init(|| {
        load(
            "war3x_data",
            &[
                "list-Files100_War3x.mpq-clean.txt",
                "list-War3-clean.txt",
            ],
        )
    })
}

/// The twelve ladder heroes of Reign of Chaos (four per race).
const LADDER_HEROES: [&str; 12] = [
    "Hpal", "Hamg", "Hmkg", "Obla", "Ofar", "Othr", "Udea", "Udre", "Ulic", "Ekee", "Emoo", "Edem",
];

/// Row 11: the hero ability catalog is derived from `heroAbilList` +
/// `AbilityData`, with every learnable level present.
#[test]
fn hero_ability_catalog_covers_every_hero_ability() {
    let data = roc();
    let ids = data.hero_ability_ids();
    assert!(ids.len() >= 48, "hero abilities: {}", ids.len());

    let (covered, total) = data.hero_ability_coverage();
    assert_eq!(total, ids.len(), "denominator is the hero ability set");
    assert_eq!(covered, total, "every hero ability has numbers for all levels");

    let catalog = data.hero_abilities();
    assert_eq!(catalog.len(), ids.len(), "one record per id");
    for info in &catalog {
        assert!(info.levels >= 1, "{} levels", info.id);
        assert!(
            info.covers_every_level(),
            "{} levels {}/{}",
            info.id,
            info.numbers.levels.len(),
            info.levels
        );
        assert!(!info.heroes.is_empty(), "{} owner hero", info.id);
        assert!(
            info.level(1).is_some(),
            "{} level 1 from AbilityData",
            info.id
        );
    }

    // Holy Light: the numbers are the sheet's cells, not constants.
    let holy = catalog.iter().find(|info| info.id == "AHhb").expect("AHhb");
    assert_eq!(holy.levels, 3, "levels");
    assert_eq!(holy.required_level, Some(1), "reqLevel");
    let level1 = holy.level(1).expect("AHhb level 1");
    assert_eq!(level1.mana, Some(75), "Cost1");
    assert_eq!(level1.cooldown, Some(5.0), "Cool1");
    assert_eq!(level1.range, Some(800.0), "Rng1");
    assert!(holy.heroes.iter().any(|hero| hero == "Hpal"), "Hpal owns AHhb");
    assert_eq!(holy.level(2).and_then(|level| level.mana), Some(75), "Cost2");
}

/// Row 11: each ladder hero learns exactly four skills and every one of the 48
/// slots resolves to a catalog record with full per-level numbers.
#[test]
fn ladder_heroes_learn_four_sheet_backed_skills() {
    let data = roc();
    let catalog = data.hero_abilities();
    let mut slots = 0;
    for hero_id in LADDER_HEROES {
        let hero = data
            .hero(hero_id)
            .unwrap_or_else(|| panic!("hero {hero_id}"));
        assert_eq!(hero.skills.len(), 4, "{hero_id} skill count");
        for skill in &hero.skills {
            let info = catalog
                .iter()
                .find(|info| info.id == skill.id)
                .unwrap_or_else(|| panic!("{hero_id} skill {} missing from catalog", skill.id));
            assert!(info.covers_every_level(), "{} levels", skill.id);
            assert!(
                info.heroes.iter().any(|owner| owner == hero_id),
                "{} -> {hero_id} ownership",
                skill.id
            );
            slots += 1;
        }
    }
    assert_eq!(slots, 48, "12 ladder heroes x 4 skills");
}

/// Row 14: the Goblin Merchant's stock is the `Sellitems` profile list, and each
/// line carries `ItemData`'s price/stock/charge columns.
#[test]
fn goblin_merchant_stock_comes_from_sellitems_and_itemdata() {
    let data = roc();
    let shop = data.shop("ngme").expect("goblin merchant");
    assert!(shop.name.is_some(), "shop name resolves through the profile layer");
    let ids: Vec<&str> = shop.items.iter().map(|item| item.item_id.as_str()).collect();
    assert_eq!(
        ids,
        ["phea", "pman", "pinv", "shea", "spro", "wneg", "gemt", "stwp"],
        "Sellitems order"
    );

    let potion = shop
        .items
        .iter()
        .find(|item| item.item_id == "phea")
        .expect("healing potion");
    assert_eq!(potion.gold, Some(150), "goldcost");
    assert_eq!(potion.charges, Some(1), "uses");
    assert_eq!(potion.stock_max, Some(3), "stockMax");
    assert_eq!(potion.stock_regen, Some(120), "stockRegen");
    assert_eq!(potion.stock_start, Some(440), "stockStart");
    assert_eq!(potion.class.as_deref(), Some("potiona"), "itemClass");

    assert_eq!(data.shops_selling("phea"), ["ngme"], "reverse lookup");
    // RoC's `ItemFunc.txt` carries no `Requires` for base items, so the list is
    // ungated on this install; the TFT test covers the gated rows.
    assert!(potion.requires.is_empty(), "RoC base shop ungated");
}

/// Row 14: purchase gating is the sheet's `Requires`, with pseudo tokens expanded
/// through their own `DependencyOr` section (TFT data).
#[test]
fn tft_item_purchase_requirements_expand_dependency_or() {
    let data = tft();
    let shop = data.shop("ngme").expect("goblin merchant");
    let ids: Vec<&str> = shop.items.iter().map(|item| item.item_id.as_str()).collect();
    assert!(ids.contains(&"bspd"), "TFT stock: {ids:?}");

    let potion = shop
        .items
        .iter()
        .find(|item| item.item_id == "stwp")
        .expect("scroll of town portal");
    assert_eq!(potion.requires.len(), 1, "stwp Requires");
    let requirement = &potion.requires[0];
    assert_eq!(requirement.token, "TWN2");
    assert_eq!(
        requirement.alternatives,
        ["hkee", "ostr", "etoa", "unp1"],
        "TWN2 DependencyOr"
    );
    assert!(requirement.is_pseudo());

    // A concrete building requirement has no alternatives of its own.
    let orb = data.item("ofir").expect("orb of fire");
    let requires = orb.requirements();
    assert_eq!(requires.len(), 1);
    assert_eq!(requires[0].token, "hcas");
    assert!(requires[0].alternatives.is_empty());

    // `sellable` is a TFT column: the shop stock marks items sellable.
    assert!(
        shop.items.iter().any(|item| item.sellable == Some(true)),
        "TFT sellable flags"
    );
}

/// Row 14: item abilities are `abilList` ids resolved against `AbilityData`;
/// charges are `uses`.
#[test]
fn item_abilities_and_charges_come_from_the_item_sheet() {
    let data = roc();
    let ids = data.item_ability_ids();
    assert!(ids.len() >= 90, "item ability refs: {}", ids.len());
    let (covered, total) = data.item_ability_coverage();
    assert!(
        covered * 100 >= total * 60,
        "qa.items.abilities: {covered}/{total} < 60%"
    );

    let claws = data.item("ratc").expect("claws of attack");
    assert_eq!(claws.abilities(), ["AItc"], "abilList");
    let numbers = data.item_ability_numbers("ratc");
    assert_eq!(numbers.len(), 1);
    assert_eq!(numbers[0].id, "AItc");
    assert!(!numbers[0].levels.is_empty(), "AItc numbers");
    assert_eq!(claws.ability_views().len(), 1, "typed item ability");
    assert_eq!(claws.ability_views()[0].id_str(), "AItc");

    assert_eq!(data.item("phea").unwrap().charges().unwrap(), Some(1), "uses");
    assert_eq!(data.item("wneg").unwrap().charges().unwrap(), Some(3), "uses");
    // Permanent items write `-`; that is "no charges", surfaced as `None`.
    assert_eq!(data.item("ratc").unwrap().charges().unwrap(), None);
}

/// Row 14 boundary: retail data has no recipe table. Every `Requires` token is a
/// tech token (unit/upgrade/pseudo), never another item, and `ItemData` has no
/// recipe/make column — so combining is map-script logic, not data.
#[test]
fn retail_data_has_no_recipe_table() {
    for data in [roc(), tft()] {
        let table = data.table(TableKind::ItemData).expect("ItemData");
        for column in table.columns() {
            let lower = column.to_lowercase();
            assert!(
                !lower.contains("recipe") && !lower.contains("makeitem"),
                "ItemData recipe column {column}"
            );
        }
        for item in data.items() {
            for requirement in item.requirements() {
                assert!(
                    data.item(&requirement.token).is_none(),
                    "{} requires item {} — that would be recipe data",
                    item.id_str(),
                    requirement.token
                );
            }
            if let Some(class) = item.class_name() {
                assert_ne!(
                    class.to_lowercase(),
                    "recipe",
                    "{} item class is a recipe",
                    item.id_str()
                );
            }
        }
    }
}
