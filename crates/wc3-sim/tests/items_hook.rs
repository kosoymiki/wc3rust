//! Item runtime hooks on the live tick grid: the per-tick maintenance pass and
//! the death drops.  These are the tests that fail when `items_tick` /
//! `drop_items_on_death` are implemented but never called by `Sim::step` or by
//! a death path — the mechanic tests in `items_shops.rs` drive the methods
//! directly and cannot see that.
//!
//! Mechanic ids covered here:
//! * `items.tick.cooldowns`
//! * `items.death.drops-backpack`
//! * `items.death.no-double-drop`

mod common;

use common::*;
use wc3_sim::data::{self, ItemAction, ItemActionError};
use wc3_sim::{AttackType, Event, Sim, Vec2, generic_ability};

/// mechanic `items.tick.cooldowns`: an item cooldown set by `ItemUse` runs on
/// the same 30 Hz grid as everything else — `Sim::step` decrements it once per
/// tick, an item in the shared `AIhe` group is refused while it is live, and it
/// becomes usable again on the tick the counter reaches zero.  The same run
/// proves the per-tick maintenance pass leaves a living carrier's backpack
/// alone.
#[test]
fn items_tick_cooldowns() {
    let mut sim = Sim::new(6001, 64, 64);
    sim.passive_income = false;
    let hero = sim
        .spawn_def(0, "Hpal", Vec2::new(600.0, 600.0))
        .expect("paladin");
    let heal = data::item_def("phea").expect("potion of healing").heal;
    let cooldown_ticks = generic_ability("AIh1")
        .expect("AIh1 healing potion ability row")
        .cooldown_ticks;
    assert!(cooldown_ticks > 0, "AIh1 carries a real cooldown");
    assert_eq!(
        data::item_cooldown("phea"),
        Some(("AIhe", false)),
        "phea shares the retail AIhe cooldown group"
    );

    let first = sim
        .spawn_item("phea", Vec2::new(600.0, 600.0), 0)
        .expect("first potion");
    let second = sim
        .spawn_item("phea", Vec2::new(600.0, 600.0), 0)
        .expect("second potion");
    for item in [first, second] {
        sim.item_action(hero, ItemAction::Pickup { item })
            .expect("pickup");
    }
    assert_eq!(sim.unit(hero).unwrap().items.len(), 2, "two potions carried");

    // First use: heals and starts the AIhe cooldown on this tick.
    let max_hp = sim.unit(hero).unwrap().max_hp;
    sim.set_unit_hp(hero, (max_hp - heal).max(1));
    let used = sim
        .item_action(
            hero,
            ItemAction::Use {
                item: first,
                target: None,
                point: None,
            },
        )
        .expect("first use is free");
    assert!(
        matches!(used, wc3_sim::data::ItemOutcome::Used { consumed: true, .. }),
        "one charge consumed: {used:?}"
    );

    // Second potion shares the group and is refused with the exact remainder.
    let refused = sim
        .item_action(
            hero,
            ItemAction::Use {
                item: second,
                target: None,
                point: None,
            },
        )
        .expect_err("shared AIhe cooldown");
    match refused {
        ItemActionError::OnCooldown {
            item,
            remaining_ticks,
        } => {
            assert_eq!(item, "phea");
            assert_eq!(
                remaining_ticks, cooldown_ticks,
                "cooldown is the AIh1 duration on the tick it was set"
            );
        }
        other => panic!("expected OnCooldown, got {other:?}"),
    }

    // The cooldown is decremented by `Sim::step` exactly once per tick.
    step_n(&mut sim, cooldown_ticks - 1);
    assert_eq!(
        sim.unit(hero).unwrap().items.len(),
        1,
        "items_tick must not drop a living carrier's backpack"
    );
    let withheld = sim
        .item_action(
            hero,
            ItemAction::Use {
                item: second,
                target: None,
                point: None,
            },
        )
        .expect_err("still on cooldown one tick before it ends");
    assert!(
        matches!(
            withheld,
            ItemActionError::OnCooldown {
                remaining_ticks: 1,
                ..
            }
        ),
        "one tick left: {withheld:?}"
    );

    // One more tick retires the entry and the item is usable again.
    step_n(&mut sim, 1);
    let hp_before = (max_hp - heal).max(1);
    sim.set_unit_hp(hero, hp_before);
    let used = sim
        .item_action(
            hero,
            ItemAction::Use {
                item: second,
                target: None,
                point: None,
            },
        )
        .expect("cooldown expired, potion usable again");
    assert!(
        matches!(used, wc3_sim::data::ItemOutcome::Used { consumed: true, .. }),
        "second potion consumed: {used:?}"
    );
    assert_eq!(
        sim.unit(hero).unwrap().hp,
        hp_before + heal,
        "the delayed use heals the full amount"
    );
    assert!(
        sim.unit(hero).unwrap().items.is_empty(),
        "both charges spent"
    );
}

/// mechanic `items.tick.corpse-repair`: the per-tick maintenance pass is
/// actually reached from `Sim::step` — a carrier that is already dead with a
/// full backpack when the tick starts (save-game load, a future death path
/// without an inline drop) is emptied by the pass, which only proves out if
/// `step` calls `items_tick`.
#[test]
fn items_tick_corpse_repair() {
    let mut sim = Sim::new(6004, 64, 64);
    sim.passive_income = false;
    let corpse_at = Vec2::new(400.0, 400.0);
    let hero = sim.spawn_def(0, "Hpal", corpse_at).expect("paladin");
    let carried = [
        sim.spawn_item("ratf", corpse_at, 0).expect("claws of attack"),
        sim.spawn_item("phea", corpse_at, 0).expect("potion of healing"),
    ];
    for item in carried {
        sim.item_action(hero, ItemAction::Pickup { item })
            .expect("pickup");
    }
    // Dead carrier, backpack untouched, and no death event: the inline death
    // drops are deliberately not on this path.
    sim.unit_mut(hero).expect("unit").alive = false;
    assert_eq!(
        sim.unit(hero).unwrap().items.len(),
        carried.len(),
        "backpack still full before the tick"
    );

    sim.step(&[]);
    assert!(
        sim.unit(hero).unwrap().items.is_empty(),
        "Sim::step ran items_tick and emptied the stale corpse"
    );
    assert_eq!(sim.items.len(), carried.len(), "both items on the ground");
    for item in carried {
        assert_eq!(sim.item(item).expect("ground item").owner, None);
    }
}

/// mechanic `items.death.drops-backpack`: a hero killed by damage drops its
/// whole three-item backpack in the death tick itself — three ground items,
/// one per carried item, on a deterministic ring around the corpse, with the
/// passive bonuses removed from the corpse.
#[test]
fn items_death_drops_backpack() {
    let mut sim = Sim::new(6002, 64, 64);
    sim.passive_income = false;
    let corpse_at = Vec2::new(700.0, 700.0);
    let hero = sim.spawn_def(0, "Hpal", corpse_at).expect("paladin");
    let base_damage = sim.unit(hero).unwrap().damage;
    let killer = sim.spawn_def(1, "hfoo", Vec2::new(760.0, 700.0)).expect("footman");

    let carried = [
        sim.spawn_item("ratf", corpse_at, 0).expect("claws of attack"),
        sim.spawn_item("phea", corpse_at, 0).expect("potion of healing"),
        sim.spawn_item("pman", corpse_at, 0).expect("potion of mana"),
    ];
    for item in carried {
        sim.item_action(hero, ItemAction::Pickup { item })
            .expect("pickup");
    }
    assert_eq!(sim.unit(hero).unwrap().items.len(), 3, "three items carried");
    assert_eq!(
        sim.unit(hero).unwrap().damage,
        base_damage + 15,
        "claws of attack apply while carried"
    );

    sim.apply_damage(killer, hero, 100_000, AttackType::Normal);
    assert!(!sim.unit(hero).unwrap().alive, "hero died to the hit");
    assert!(
        sim.events.iter().any(|event| *event == Event::UnitDied(hero)),
        "the combat death path emitted UnitDied"
    );
    assert!(
        sim.unit(hero).unwrap().items.is_empty(),
        "backpack emptied in the death tick, not one tick later"
    );
    assert_eq!(
        sim.unit(hero).unwrap().damage, base_damage,
        "passive item bonus removed from the corpse"
    );
    assert_eq!(sim.items.len(), 3, "exactly three ground items, no duplicates");

    // Deterministic ring: slot k sits at +32 units from the corpse at angle
    // k * TAU / 3, so slot 0 is due +X and the order is stable.
    for (slot, item) in carried.iter().enumerate() {
        let ground = sim.item(*item).expect("carried item is now ground");
        assert_eq!(ground.owner, None, "slot {slot} is unowned on the ground");
        let angle = slot as f32 * std::f32::consts::TAU / carried.len() as f32;
        let expected = corpse_at + Vec2::new(angle.cos(), angle.sin()) * 32.0;
        assert!(
            ground.pos.distance(expected) < 0.01,
            "slot {slot} at ring position: {:?} vs {expected:?}",
            ground.pos
        );
    }
    let mut ids: Vec<u32> = sim.items.iter().map(|entry| entry.id.0).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 3, "three distinct item ids, none duplicated");

    // The next tick's maintenance pass sees an empty backpack and adds nothing.
    sim.step(&[]);
    assert_eq!(sim.items.len(), 3, "items_tick does not re-drop the corpse");
}

/// mechanic `items.death.no-double-drop`: the inline drop in `kill_unit`, the
/// script kill (`SetUnitState(LIFE, 0)` → `kill_unit`), a second damage hit and
/// the per-tick maintenance pass all run in one tick, and the backpack is still
/// dropped exactly once — no duplicated ground items and no double removal of
/// the passive bonuses.
#[test]
fn items_death_no_double_drop() {
    let mut sim = Sim::new(6003, 64, 64);
    sim.passive_income = false;
    let corpse_at = Vec2::new(500.0, 500.0);
    let hero = sim.spawn_def(0, "Hpal", corpse_at).expect("paladin");
    let base_damage = sim.unit(hero).unwrap().damage;
    let attacker = sim.spawn_def(1, "hfoo", Vec2::new(560.0, 500.0)).expect("footman");

    let carried = [
        sim.spawn_item("ratf", corpse_at, 0).expect("claws of attack"),
        sim.spawn_item("phea", corpse_at, 0).expect("potion of healing"),
        sim.spawn_item("pman", corpse_at, 0).expect("potion of mana"),
    ];
    for item in carried {
        sim.item_action(hero, ItemAction::Pickup { item })
            .expect("pickup");
    }

    // Death path 1: direct kill.
    sim.kill_unit(hero);
    assert!(sim.unit(hero).unwrap().items.is_empty(), "dropped by kill_unit");
    assert_eq!(
        sim.events
            .iter()
            .filter(|event| **event == Event::UnitDied(hero))
            .count(),
        1,
        "kill_unit emits exactly one UnitDied"
    );
    // Death path 2: script kill on an already dead unit.
    sim.set_unit_hp(hero, 0);
    // Death path 3: a damage hit on the corpse.
    sim.apply_damage(attacker, hero, 100_000, AttackType::Normal);
    // Death path 4: the per-tick maintenance pass.
    sim.step(&[]);
    sim.step(&[]);

    assert!(!sim.unit(hero).unwrap().alive, "the unit is genuinely dead");
    assert_eq!(
        sim.events
            .iter()
            .filter(|event| **event == Event::UnitDied(hero))
            .count(),
        0,
        "no death path fires a second UnitDied (events are per-tick)"
    );
    assert_eq!(
        sim.items.len(),
        carried.len(),
        "three ground items, not six: {:#?}",
        sim.items.iter().map(|entry| entry.id).collect::<Vec<_>>()
    );
    let mut ids: Vec<u32> = sim.items.iter().map(|entry| entry.id.0).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), carried.len(), "no item id duplicated");
    for (slot, item) in carried.iter().enumerate() {
        let ground = sim.item(*item).expect("carried item is now ground");
        assert_eq!(ground.owner, None, "slot {slot} unowned");
        assert!(
            ground.pos.distance(corpse_at) <= 32.0 + 0.01,
            "slot {slot} stays on the corpse ring: {:?}",
            ground.pos
        );
    }
    assert!(
        sim.unit(hero).unwrap().items.is_empty(),
        "backpack stays empty after every death path"
    );
    assert_eq!(
        sim.unit(hero).unwrap().damage, base_damage,
        "bonus removed once, not once per death path"
    );
}
