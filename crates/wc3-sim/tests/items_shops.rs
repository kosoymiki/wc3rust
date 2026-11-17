//! Item shop catalogue, requirement gating and backpack mechanics.
//!
//! Mechanic ids covered here:
//! * `items.shop.catalogue-per-race`
//! * `items.shop.neutral-justice`
//! * `items.buy.requirement-denied`
//! * `items.backpack.drop-on-death`
//! * `items.charges.consume`

mod common;

use std::path::Path;

use wc3_re::mapfiles::MapBundle;
use wc3_sim::data::{self, ItemAction, ItemActionError};
use wc3_sim::{Sim, Vec2, def_of};

const MAP_REL: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

/// Retail `Makeitems=` lists, verbatim from the TFT profile files.
const RACE_SHOPS: [(&str, wc3_sim::Race, &[&str]); 4] = [
    (
        "hvlt",
        wc3_sim::Race::Human,
        &["sreg", "mcri", "plcl", "phea", "pman", "stwp", "tsct", "ofir", "ssan"],
    ),
    (
        "ovln",
        wc3_sim::Race::Orc,
        &["shas", "hslv", "dust", "phea", "pman", "stwp", "tgrh", "oli2"],
    ),
    (
        "utom",
        wc3_sim::Race::Undead,
        &["rnec", "plcl", "skul", "phea", "pman", "stwp", "ocor", "shea"],
    ),
    (
        "eden",
        wc3_sim::Race::NightElf,
        &["moon", "plcl", "dust", "phea", "pman", "stwp", "spre", "oven", "pams"],
    ),
];

/// Neutral Goblin Merchant `Sellitems=` (TFT profile).
const NEUTRAL_SHOP: (&str, &[&str]) = (
    "ngme",
    &[
        "stwp", "bspd", "dust", "tret", "prvt", "cnob", "stel", "pnvl", "shea", "spro", "pinv",
    ],
);

fn stock_of(sim: &Sim, shop: wc3_sim::UnitId, item: &str) -> i32 {
    sim.shop(shop)
        .and_then(|shop| shop.slots.iter().find(|slot| slot.item == item))
        .map(|slot| slot.stock)
        .expect("stock slot")
}

/// mechanic `items.shop.catalogue-per-race`: each melee race owns its real item
/// shop def, and the runtime catalogue resolves every `Makeitems=` id with the
/// ItemData price, stock and `ItemFunc` requirements.
#[test]
fn items_shop_catalogue_per_race() {
    for (index, (shop_id, race, expected)) in RACE_SHOPS.iter().enumerate() {
        let def = def_of(shop_id).unwrap_or_else(|| panic!("{shop_id} def"));
        assert!(def.is_building(), "{shop_id} is a building");
        assert!(def.buildable, "{shop_id} is worker-buildable (profile Builds=)");
        assert_eq!(def.race, *race, "{shop_id} race");
        assert_eq!(def.sells_items, *expected, "{shop_id} Makeitems list");
        let menu = data::build_menu(*race);
        assert!(
            menu.iter().any(|entry| entry.id == *shop_id),
            "{shop_id} is in the {race:?} worker build menu: {:?}",
            menu.iter().map(|entry| entry.id).collect::<Vec<_>>()
        );

        let mut sim = Sim::new(447 + index as u64, 64, 64);
        sim.passive_income = false;
        let shop = sim
            .spawn_def(0, shop_id, Vec2::new(600.0, 600.0))
            .unwrap_or_else(|| panic!("spawn {shop_id}"));
        let buyer = sim
            .spawn_def(0, "hfoo", Vec2::new(700.0, 600.0))
            .expect("buyer");
        let _ = buyer;
        let inventory = sim.shop_inventory(shop, 0);
        assert_eq!(
            inventory.len(),
            expected.len(),
            "{shop_id} catalogue length"
        );
        let mut total_stock = 0;
        let mut max_restock = 0;
        for entry in &inventory {
            assert!(
                expected.contains(&entry.def.id),
                "{shop_id} entry {} is not in Makeitems",
                entry.def.id
            );
            assert_eq!(entry.cost, entry.def.cost, "{} goldcost", entry.def.id);
            assert_eq!(entry.stock, entry.def.stock_max.max(1), "{} stockMax", entry.def.id);
            assert!(entry.stock_max >= 1, "{} stock cap", entry.def.id);
            if entry.restock_ticks > 0 {
                assert_eq!(
                    entry.restock_ticks,
                    entry.def.stock_regen as u32 * wc3_sim::TICKS_PER_SECOND,
                    "{} stockRegen",
                    entry.def.id
                );
            }
            assert_eq!(
                entry.requires.len(),
                data::item_requirements(entry.def.id).count(),
                "{} requirement rows",
                entry.def.id
            );
            total_stock += entry.stock;
            max_restock = max_restock.max(entry.restock_ticks);
        }
        eprintln!(
            "SHOP_PROBE shop={shop_id} race={:?} items={} stock={} restock_max_ticks={max_restock}",
            race,
            inventory.len(),
            total_stock
        );
    }
}

/// mechanic `items.shop.neutral-justice`: the map's own `war3mapUnits.doo`
/// placements register their Goblin Merchants with the real neutral catalogue.
#[test]
fn items_shop_neutral_justice() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join(MAP_REL);
    if !path.is_file() {
        eprintln!("skip: (2)Justice.w3x fixture missing at {}", path.display());
        return;
    }
    let bundle = MapBundle::open(&path).expect("Justice bundle");
    assert!(
        bundle.objects.is_empty(),
        "melee Justice carries no war3map.w3u object data: {:?}",
        bundle.objects.keys().collect::<Vec<_>>()
    );
    let terrain = bundle.terrain.as_ref().expect("terrain");
    let origin = Vec2::new(terrain.center.0, terrain.center.1);
    let units = bundle.units.as_ref().expect("war3mapUnits.doo");
    let mut sim = Sim::new(0x447, 512, 512);
    sim.passive_income = false;
    let mut shops = Vec::new();
    for unit in units {
        let id = String::from_utf8_lossy(&unit.type_id).to_string();
        if id != NEUTRAL_SHOP.0 {
            continue;
        }
        let pos = Vec2::new(unit.position.x - origin.x, unit.position.y - origin.y);
        let shop = sim
            .spawn_def_exact(2, &id, pos)
            .unwrap_or_else(|| panic!("map shop {id} at {pos:?}"));
        shops.push((shop, pos));
    }
    assert_eq!(shops.len(), 2, "Justice places two Goblin Merchants");
    assert_eq!(sim.shops.len(), 2, "both map shops have runtime stock");
    for (shop, _pos) in &shops {
        let inventory = sim.shop_inventory(*shop, 0);
        assert_eq!(inventory.len(), NEUTRAL_SHOP.1.len(), "neutral catalogue");
        for entry in &inventory {
            assert!(
                NEUTRAL_SHOP.1.contains(&entry.def.id),
                "neutral entry {} not in Sellitems",
                entry.def.id
            );
        }
        let stwp = inventory
            .iter()
            .find(|entry| entry.def.id == "stwp")
            .expect("Scroll of Town Portal");
        assert!(
            !stwp.available,
            "no tier-2 hall -> stwp is shown but not purchasable"
        );
        eprintln!(
            "NEUTRAL_SHOP_PROBE shop={} instance={:?} items={} stwp_stock={} stwp_available={}",
            NEUTRAL_SHOP.0,
            shop,
            inventory.len(),
            stwp.stock,
            stwp.available
        );
    }
}

/// mechanic `items.buy.requirement-denied`: a purchase missing its real
/// `Requires=` token is refused with the exact token/alternatives and changes
/// no gold or stock; after the building exists the same buy succeeds.
#[test]
fn items_buy_requirement_denied() {
    let mut sim = Sim::new(447, 64, 64);
    sim.passive_income = false;
    sim.resources[0] = 5_000;
    let shop = sim
        .spawn_def(2, "ngme", Vec2::new(600.0, 600.0))
        .expect("goblin merchant");
    let buyer = sim
        .spawn_def(0, "hfoo", Vec2::new(700.0, 600.0))
        .expect("buyer");
    let error = sim
        .item_action(
            buyer,
            ItemAction::Buy {
                shop,
                item: "stwp",
            },
        )
        .expect_err("stwp needs TWN2");
    match &error {
        ItemActionError::MissingRequirement {
            item,
            token,
            alternatives,
        } => {
            assert_eq!(*item, "stwp");
            assert_eq!(*token, "TWN2");
            assert_eq!(*alternatives, &["hkee", "ostr", "etoa", "unp1"]);
        }
        other => panic!("expected MissingRequirement, got {other:?}"),
    }
    assert!(error.reason().contains("TWN2"), "{}", error.reason());
    assert_eq!(sim.resources[0], 5_000, "denied buy keeps gold");
    assert_eq!(stock_of(&sim, shop, "stwp"), 2, "denied buy keeps stock");

    sim.spawn_def(0, "hkee", Vec2::new(500.0, 600.0))
        .expect("keep");
    sim.step(&[]);
    let outcome = sim
        .item_action(
            buyer,
            ItemAction::Buy {
                shop,
                item: "stwp",
            },
        )
        .expect("stwp after Keep");
    assert!(matches!(
        outcome,
        wc3_sim::data::ItemOutcome::Bought { cost: 350, .. }
    ));
    assert_eq!(sim.resources[0], 4_650, "350 gold debited");
    assert_eq!(stock_of(&sim, shop, "stwp"), 1, "one stock consumed");

    // A concrete token (`ofir` -> hcas) reports hcas, not the pseudo token.
    let vault = sim
        .spawn_def(0, "hvlt", Vec2::new(1_000.0, 600.0))
        .expect("arcane vault");
    let error = sim
        .item_action(
            buyer,
            ItemAction::Buy {
                shop: vault,
                item: "ofir",
            },
        )
        .expect_err("ofir needs a Castle");
    assert!(matches!(
        error,
        ItemActionError::MissingRequirement { token: "hcas", .. }
    ));
}

/// mechanic `items.backpack.drop-on-death`: a dead carrier's six-slot backpack
/// is dropped at the corpse with passive bonuses removed, exactly once.
#[test]
fn items_backpack_drop_on_death() {
    let mut sim = Sim::new(447, 64, 64);
    sim.passive_income = false;
    let hero = sim
        .spawn_def(0, "Hpal", Vec2::new(600.0, 600.0))
        .expect("paladin");
    let base_damage = sim.unit(hero).unwrap().damage;
    let item = sim
        .spawn_item("ratf", Vec2::new(600.0, 600.0), 0)
        .expect("claws of attack");
    sim.item_action(
        hero,
        ItemAction::Pickup { item },
    )
    .expect("pickup");
    assert_eq!(sim.unit(hero).unwrap().damage, base_damage + 15, "+15 attack");

    sim.kill_unit(hero);
    assert!(!sim.unit(hero).unwrap().alive, "hero died");
    // `kill_unit` drops inline, in the death tick: the backpack is already
    // empty and `items_tick` is only the idempotent safety net.
    assert!(
        sim.unit(hero).unwrap().items.is_empty(),
        "backpack emptied in the kill_unit tick"
    );
    sim.items_tick();

    assert!(
        sim.unit(hero).unwrap().items.is_empty(),
        "backpack emptied on death"
    );
    let ground = sim.item(item).expect("item remains in the world");
    assert_eq!(ground.owner, None, "item is unowned on the ground");
    let corpse = sim.unit(hero).unwrap().pos;
    assert!(
        ground.pos.distance(corpse) <= 64.0,
        "item dropped at the corpse: {:?} vs {corpse:?}",
        ground.pos
    );
    assert_eq!(
        sim.unit(hero).unwrap().damage,
        base_damage,
        "passive bonus removed with the item"
    );
    let items_before = sim.items.len();
    sim.items_tick();
    assert_eq!(sim.items.len(), items_before, "no duplicate drop");
}

/// mechanic `items.charges.consume`: one charge per use, the last charge
/// removes the item, and an unlimited (`uses=` absent) item stays.
#[test]
fn items_charges_consume() {
    let mut sim = Sim::new(447, 64, 64);
    sim.passive_income = false;
    let unit = sim
        .spawn_def(0, "hfoo", Vec2::new(600.0, 600.0))
        .expect("footman");
    let potion = sim
        .spawn_item("phea", Vec2::new(600.0, 600.0), 3)
        .expect("healing potion");
    sim.item_action(unit, ItemAction::Pickup { item: potion })
        .expect("pickup");
    for expected_left in [2, 1, 0] {
        let outcome = sim
            .item_action(
                unit,
                ItemAction::Use {
                    item: potion,
                    target: None,
                    point: None,
                },
            )
            .expect("drink");
        match outcome {
            wc3_sim::data::ItemOutcome::Used {
                consumed,
                charges_left,
                ..
            } => {
                assert!(consumed, "charged potion consumes");
                assert_eq!(charges_left, expected_left);
            }
            other => panic!("expected Used, got {other:?}"),
        }
        if expected_left > 0 {
            assert!(sim.item(potion).is_some(), "item alive at {expected_left}");
            assert!(sim.unit(unit).unwrap().items.contains(&potion));
            // Healing Potion (AIh1) carries a real 600-tick ability cooldown;
            // advance the clock so the next charge is usable.
            common::step_n(&mut sim, 620);
        }
    }
    assert!(sim.item(potion).is_none(), "last charge removes the item");
    assert!(sim.unit(unit).unwrap().items.is_empty());

    // The potion's `cooldownID=AIhe` is shared with Essence of Aszune; wait
    // out the group cooldown before the next item use.
    common::step_n(&mut sim, 620);
    // Unlimited: `esaz` (Essence of Aszune) has no `uses` column -> stays.
    let relic = sim
        .spawn_item("esaz", Vec2::new(600.0, 600.0), 0)
        .expect("essence");
    sim.item_action(unit, ItemAction::Pickup { item: relic })
        .expect("pickup relic");
    let outcome = sim
        .item_action(
            unit,
            ItemAction::Use {
                item: relic,
                target: None,
                point: None,
            },
        )
        .expect("unlimited use");
    match outcome {
        wc3_sim::data::ItemOutcome::Used {
            consumed,
            charges_left,
            ..
        } => {
            assert!(!consumed, "unlimited item is not consumed");
            assert_eq!(charges_left, 0);
        }
        other => panic!("expected Used, got {other:?}"),
    }
    assert!(sim.item(relic).is_some(), "unlimited item stays");
    assert!(sim.unit(unit).unwrap().items.contains(&relic));
}
