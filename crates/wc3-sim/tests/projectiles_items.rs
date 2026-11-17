//! ROUND 8 parity gates: projectiles (#12) and item shops (#14).
//!
//! Every assertion runs on the tick clock; nothing sleeps or reads wall time.

mod common;

use common::*;
use wc3_sim::{AttackType, Command, Event, Sim, Vec2, data, def_of};

#[test]
fn qa_combat_projectiles() {
    let mut sim = Sim::new(401, 64, 64);
    sim.passive_income = false;
    let rifle = sim
        .spawn_def(0, "hrif", Vec2::new(1000.0, 1000.0))
        .expect("rifleman");
    let target = sim
        .spawn_def(1, "hfoo", Vec2::new(1300.0, 1000.0))
        .expect("footman");
    sim.step(&[]);
    let hp_before = sim.unit(target).unwrap().hp;
    sim.step(&[Command::Attack {
        units: vec![rifle],
        target,
    }]);
    let mut launched = false;
    for _ in 0..200 {
        sim.step(&[]);
        if !sim.projectiles.is_empty() {
            launched = true;
            break;
        }
    }
    assert!(launched, "ranged attack spawns a missile");
    let projectile = &sim.projectiles[0];
    assert_eq!(projectile.speed, 1900.0, "Missilespeed from HumanUnitFunc");
    assert!(projectile.damage > 0);
    assert!(
        projectile.art.contains("Rifle"),
        "Missileart: {}",
        projectile.art
    );
    assert_eq!(
        sim.unit(target).unwrap().hp,
        hp_before,
        "no damage before impact"
    );
    // Travel time is `total / (Missilespeed / TICKS_PER_SECOND)`; the launch
    // tick already moved the missile once, so every earlier tick must leave
    // the target untouched.
    let total = sim.projectiles[0].total;
    let needed = (total / (1900.0 / 30.0)).ceil() as u32;
    for _ in 1..needed {
        sim.step(&[]);
        if sim.projectiles.is_empty() {
            break;
        }
        assert_eq!(
            sim.unit(target).unwrap().hp,
            hp_before,
            "damage lands on impact, not at launch (total {total}, need {needed} ticks)"
        );
    }
    let mut hit = sim.events.iter().any(|event| {
        matches!(
            event,
            Event::ProjectileHit { from, to, .. } if *from == rifle && *to == target
        )
    });
    for _ in 0..30 {
        if !sim.projectiles.is_empty() {
            sim.step(&[]);
            hit |= sim.events.iter().any(|event| {
                matches!(
                    event,
                    Event::ProjectileHit { from, to, .. } if *from == rifle && *to == target
                )
            });
        } else if hit {
            break;
        } else {
            sim.step(&[]);
            hit = sim.events.iter().any(|event| {
                matches!(
                    event,
                    Event::ProjectileHit { from, to, .. } if *from == rifle && *to == target
                )
            });
        }
    }
    assert!(hit, "impact emits ProjectileHit");
    assert!(
        sim.unit(target).unwrap().hp < hp_before,
        "impact applies damage"
    );
}

#[test]
fn projectiles_arc_and_home_on_a_moving_target() {
    let mut sim = Sim::new(402, 64, 64);
    let rifle = sim
        .spawn_def(0, "hrif", Vec2::new(500.0, 500.0))
        .expect("rifleman");
    let target = sim
        .spawn_def(1, "hfoo", Vec2::new(900.0, 500.0))
        .expect("footman");
    let hp_before = sim.unit(target).unwrap().hp;
    let target_pos = sim.unit(target).unwrap().pos;
    let id = sim
        .launch_projectile(
            rifle,
            Some(target),
            target_pos,
            50,
            AttackType::Pierce,
            900.0,
            0.35,
            true,
            "Test\\Missile.mdx",
        )
        .expect("missile");
    sim.step(&[]);
    assert!(sim.projectiles[0].height() > 0.0, "arc rises in flight");
    assert!(sim.projectiles[0].total > 0.0);
    // Moving the target retargets the homing missile.
    sim.teleport_unit(target, Vec2::new(900.0, 900.0));
    sim.step(&[]);
    let projectile = sim
        .projectiles
        .iter()
        .find(|projectile| projectile.id == id)
        .expect("missile in flight");
    assert_eq!(
        projectile.target_pos,
        sim.unit(target).unwrap().pos,
        "homing missile tracks the moved target"
    );
    let mut impacted = false;
    for _ in 0..240 {
        sim.step(&[]);
        if sim.projectiles.iter().all(|projectile| projectile.id != id) {
            impacted = true;
            break;
        }
    }
    assert!(impacted, "homing missile reaches the moved target");
    assert!(sim.unit(target).unwrap().hp < hp_before);
}

#[test]
fn qa_items_shop() {
    let mut sim = Sim::new(501, 64, 64);
    sim.passive_income = false;
    sim.resources[0] = 5_000;
    let shop = sim
        .spawn_def(2, "ngme", Vec2::new(1000.0, 1000.0))
        .expect("goblin merchant");
    let buyer = sim
        .spawn_def(0, "hmtm", Vec2::new(1100.0, 1000.0))
        .expect("buyer");
    // Scroll of Town Portal requires TWN2 (a tier-2 hall).
    sim.spawn_def(0, "hkee", Vec2::new(800.0, 1000.0))
        .expect("keep");
    sim.step(&[]);

    // Data: the shop def carries the real Sellitems list.
    let shop_def = def_of("ngme").expect("ngme def");
    assert_eq!(shop_def.sells_items.len(), 11);
    assert!(shop_def.sells_items.contains(&"stwp"));
    let stock = sim.shop(shop).expect("shop stock");
    assert_eq!(stock.slots.len(), 11);
    let scroll = stock.slots.iter().find(|slot| slot.item == "stwp").unwrap();
    assert_eq!(scroll.stock, 2, "stwp stockMax=2");
    assert_eq!(data::item_def("stwp").unwrap().stock_regen, 120);

    // Purchase: gold debited, stock decremented, item lands in the inventory.
    let bought = sim.buy_item(buyer, shop, "stwp").expect("buy stwp");
    assert_eq!(sim.resources[0], 4_650, "350 gold");
    let scroll = sim
        .shop(shop)
        .unwrap()
        .slots
        .iter()
        .find(|slot| slot.item == "stwp")
        .unwrap()
        .clone();
    assert_eq!(scroll.stock, 1);
    assert_eq!(
        scroll.next_restock_tick,
        sim.tick + 120 * 30,
        "stockRegen 120 s"
    );
    let item = sim.item(bought).expect("item");
    assert_eq!(item.charges, 1, "uses=1");
    assert_eq!(sim.unit(buyer).unwrap().items, vec![bought]);
    assert!(
        sim.events
            .iter()
            .any(|event| matches!(event, Event::ItemBought { buyer: b, shop: s, item: "stwp" } if *b == buyer && *s == shop))
    );

    // Sold-out list and distance guards.
    sim.buy_item(buyer, shop, "stwp").expect("second scroll");
    assert_eq!(
        sim.buy_item(buyer, shop, "stwp"),
        Err("out of stock"),
        "stockMax=2"
    );
    assert_eq!(sim.buy_item(buyer, shop, "phea"), Err("item not sold here"));
    sim.teleport_unit(buyer, Vec2::new(3000.0, 3000.0));
    assert_eq!(sim.buy_item(buyer, shop, "dust"), Err("shop too far"));

    // Restock uses the real stockRegen timer (fast-forwarded deterministically).
    let slot_index = sim.shops[0]
        .slots
        .iter()
        .position(|slot| slot.item == "stwp")
        .unwrap();
    sim.shops[0].slots[slot_index].next_restock_tick = sim.tick;
    sim.step(&[]);
    assert_eq!(sim.shops[0].slots[slot_index].stock, 1, "one restocked");

    // Inventory limit.
    let hoarder = sim
        .spawn_def(0, "hfoo", Vec2::new(1100.0, 1100.0))
        .expect("hoarder");
    for _ in 0..6 {
        let item = sim
            .spawn_item("phea", sim.unit(hoarder).unwrap().pos, 1)
            .expect("potion");
        sim.pickup_item(hoarder, item).expect("pickup");
    }
    assert_eq!(sim.unit(hoarder).unwrap().items.len(), 6);
    assert_eq!(sim.buy_item(hoarder, shop, "dust"), Err("inventory full"));
}

#[test]
fn item_charges_decrement_and_consumable_is_used_up() {
    let mut sim = Sim::new(502, 64, 64);
    sim.passive_income = false;
    let hero = sim
        .spawn_def(0, "hfoo", Vec2::new(600.0, 600.0))
        .expect("unit");
    let attacker = sim
        .spawn_def(1, "hfoo", Vec2::new(700.0, 600.0))
        .expect("attacker");
    let potion = sim
        .spawn_item("phea", sim.unit(hero).unwrap().pos, 1)
        .expect("healing potion");
    sim.pickup_item(hero, potion).expect("pickup");
    let _ = sim.apply_damage(attacker, hero, 100, AttackType::Normal);
    let hp_before = sim.unit(hero).unwrap().hp;
    assert_eq!(sim.item(potion).unwrap().charges, 1);
    sim.use_item(hero, potion).expect("drink");
    assert!(sim.unit(hero).unwrap().hp > hp_before, "heal applied");
    assert!(sim.item(potion).is_none(), "last charge consumes the item");
    assert!(!sim.unit(hero).unwrap().items.contains(&potion));
}

#[test]
fn item_table_is_real_and_this_install_has_no_recipes() {
    let scroll = data::item_def("stwp").expect("scroll");
    assert_eq!(scroll.cost, 350);
    assert_eq!(scroll.stock_max, 2);
    assert_eq!(scroll.stock_regen, 120);
    assert!(scroll.usable && scroll.perishable && scroll.sellable);
    // Observed ItemData.slk classes: Artifact, Campaign, Charged,
    // Miscellaneous, Permanent, PowerUp, Purchasable.  There is no Recipe row,
    // so a recipe-combine gate cannot be data-driven from this install and is
    // reported instead of invented.
    assert!(
        data::ITEMS
            .iter()
            .all(|item| !item.class.to_ascii_lowercase().contains("recipe")),
        "no recipe rows in this ItemData.slk"
    );
}
