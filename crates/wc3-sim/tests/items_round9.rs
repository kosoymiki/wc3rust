//! Item gameplay gates: data-backed item abilities, shop purchase gating,
//! charges and the recipe engine (retail has zero recipe rows).

mod common;

use common::*;
use wc3_sim::{Sim, Vec2, data, generic_ability};

#[test]
fn every_item_ability_is_data_backed() {
    let ability_items: Vec<_> = data::ITEMS
        .iter()
        .filter(|def| !def.ability_id.is_empty())
        .collect();
    assert_eq!(ability_items.len(), 240, "items with abilList");
    let missing: Vec<_> = ability_items
        .iter()
        .filter(|def| generic_ability(def.ability_id).is_none())
        .map(|def| def.ability_id)
        .collect();
    assert!(missing.is_empty(), "missing AbilityData rows: {missing:?}");
    assert!(
        wc3_sim::data_castable_ids().count() >= 108,
        "active castable ability set"
    );
}

#[test]
fn shop_purchase_requires_the_real_town_hall_and_debits_gold() {
    let mut sim = Sim::new(901, 48, 48);
    sim.passive_income = false;
    sim.resources[0] = 5_000;
    let shop = sim
        .spawn_def(2, "ngme", Vec2::new(1000.0, 1000.0))
        .expect("goblin merchant");
    let buyer = sim
        .spawn_def(0, "hfoo", Vec2::new(1100.0, 1000.0))
        .expect("buyer");
    sim.step(&[]);
    // Scroll of Town Portal requires TWN2 = hkee/ostr/etoa/unp1.
    assert_eq!(
        sim.buy_item(buyer, shop, "stwp"),
        Err("missing shop requirement"),
        "no tier-2 hall yet"
    );
    sim.spawn_def(0, "hkee", Vec2::new(800.0, 1000.0))
        .expect("keep");
    sim.step(&[]);
    let item = sim.buy_item(buyer, shop, "stwp").expect("buy stwp");
    assert_eq!(sim.resources[0], 4_650, "350 gold");
    assert_eq!(sim.item(item).unwrap().charges, 1, "uses=1");
    let slot = sim
        .shop(shop)
        .unwrap()
        .slots
        .iter()
        .find(|slot| slot.item == "stwp")
        .expect("stock slot")
        .clone();
    assert_eq!(slot.stock, 1, "stockMax=2 -> one left");
    assert_eq!(sim.buy_item(buyer, shop, "phea"), Err("item not sold here"));
}

#[test]
fn item_ability_use_consumes_a_charge_and_removes_the_item() {
    let mut sim = Sim::new(902, 32, 32);
    sim.passive_income = false;
    let unit = sim
        .spawn_def(0, "hfoo", Vec2::new(600.0, 600.0))
        .expect("unit");
    let item = sim
        .spawn_item("pdiv", Vec2::new(600.0, 600.0), 1)
        .expect("potion of divergence");
    sim.pickup_item(unit, item).expect("pickup");
    // pdiv's abilList is AIdv (self, 1800-tick cooldown, no direct heal).
    sim.use_item(unit, item).expect("data-backed item ability");
    assert!(sim.item(item).is_none(), "last charge consumes the item");
    assert!(!sim.unit(unit).unwrap().items.contains(&item));
}

#[test]
fn item_requirements_and_recipe_engine_are_data_honest() {
    assert_eq!(data::ITEM_REQUIREMENTS.len(), 18, "ItemFunc Requires rows");
    let stwp: Vec<_> = data::item_requirements("stwp").collect();
    assert_eq!(stwp.len(), 1);
    assert_eq!(stwp[0].token, "TWN2");
    assert_eq!(
        stwp[0].alternatives,
        &["hkee", "ostr", "etoa", "unp1"],
        "DependencyOr alternatives"
    );
    // Retail ships no item-to-item Requires row, so the recipe table is empty
    // and the engine refuses instead of inventing combinations.
    assert_eq!(data::RECIPES.len(), 0);
    let mut sim = Sim::new(903, 32, 32);
    let unit = sim
        .spawn_def(0, "hfoo", Vec2::new(400.0, 400.0))
        .expect("unit");
    assert_eq!(
        sim.combine_items(unit, "ratf"),
        Err("no recipe data"),
        "no fabricated recipes"
    );
}
