//! mechanic `jass.sim-bindings`: JASS natives bound to the canonical
//! simulation (W3AndroidEngine / Warsmash native semantics).

mod common;

use common::*;
use wc3_sim::{AttackType, Sim, SimJass, Vec2};

/// mechanic `jass.sim-bindings`: `CreateUnit` spawns a real simulation unit and
/// returns a handle that maps back to it.
#[test]
fn jass_create_unit_binds_a_real_unit() {
    let mut sim = Sim::new(600, 40, 40);
    let mut table = SimJass::new();
    let handle = sim
        .jass_create_unit(&mut table, 0, "hfoo", Vec2::new(700.0, 700.0), 90.0)
        .expect("create");
    let unit = table.unit_of(handle).expect("handle bound");
    assert_eq!(sim.unit(unit).unwrap().def().id, "hfoo");
    assert_eq!(sim.unit(unit).unwrap().pos, Vec2::new(700.0, 700.0));
    assert!(sim.jass_is_unit_alive(&table, handle));
    // Unknown rawcodes are reported, never invented.
    assert_eq!(
        sim.jass_create_unit(&mut table, 0, "zzzz", Vec2::ZERO, 0.0),
        Err("unknown unit type")
    );
}

/// mechanic `jass.sim-bindings`: order natives go through the real order queue.
#[test]
fn jass_orders_reach_the_order_queue() {
    let mut sim = Sim::new(601, 40, 40);
    let mut table = SimJass::new();
    let handle = sim
        .jass_create_unit(&mut table, 0, "hfoo", Vec2::new(600.0, 600.0), 0.0)
        .expect("create");
    let unit = table.unit_of(handle).unwrap();

    sim.jass_issue_point_order(&table, handle, "move", Vec2::new(900.0, 600.0))
        .expect("move order");
    assert!(matches!(sim.unit(unit).unwrap().order, wc3_sim::Order::Move(_)));
    step_n(&mut sim, 20);
    assert!(sim.unit(unit).unwrap().pos.x > 600.0, "unit walked");

    sim.jass_issue_immediate_order(&table, handle, "stop")
        .expect("stop");
    assert!(matches!(sim.unit(unit).unwrap().order, wc3_sim::Order::Idle));
    assert_eq!(
        sim.jass_issue_immediate_order(&table, handle, "dance"),
        Err("unsupported order")
    );

    let enemy = sim
        .jass_create_unit(&mut table, 1, "ogru", Vec2::new(1000.0, 600.0), 0.0)
        .expect("enemy");
    let enemy_unit = table.unit_of(enemy).unwrap();
    sim.jass_issue_target_order(&table, handle, "attack", enemy)
        .expect("attack order");
    assert!(matches!(
        sim.unit(unit).unwrap().order,
        wc3_sim::Order::Attack(target) if target == enemy_unit
    ));
}

/// mechanic `jass.sim-bindings`: `GetUnitState` / `SetUnitState` clamp to the
/// unit's maximum and killing at zero HP runs the death path.
#[test]
fn jass_unit_state_reads_writes_and_clamps() {
    let mut sim = Sim::new(602, 40, 40);
    let mut table = SimJass::new();
    let handle = sim
        .jass_create_unit(&mut table, 0, "hfoo", Vec2::new(600.0, 600.0), 0.0)
        .expect("create");
    let unit = table.unit_of(handle).unwrap();
    let max = sim.unit(unit).unwrap().max_hp;

    let life = sim
        .jass_get_unit_state(&table, handle, wc3_sim::jass_api::UNIT_STATE_LIFE)
        .expect("life");
    assert_eq!(life, max);
    sim.jass_set_unit_state(&table, handle, wc3_sim::jass_api::UNIT_STATE_LIFE, (max - 50) as f32)
        .expect("set life");
    assert_eq!(sim.unit(unit).unwrap().hp, max - 50);
    // Over-heal clamps instead of exceeding the maximum.
    sim.jass_set_unit_state(&table, handle, wc3_sim::jass_api::UNIT_STATE_LIFE, 9_999.0)
        .expect("set life");
    assert_eq!(sim.unit(unit).unwrap().hp, max);
    // Zero HP kills through the real death path.
    sim.jass_set_unit_state(&table, handle, wc3_sim::jass_api::UNIT_STATE_LIFE, 0.0)
        .expect("set life");
    assert!(!sim.unit(unit).unwrap().alive);
    assert!(sim
        .events
        .iter()
        .any(|event| matches!(event, wc3_sim::Event::UnitDied(id) if *id == unit)));
}

/// mechanic `jass.sim-bindings`: `UnitApplyTimedLife` despawns the unit.
#[test]
fn jass_timed_life_despawns() {
    let mut sim = Sim::new(603, 40, 40);
    let mut table = SimJass::new();
    let handle = sim
        .jass_create_unit(&mut table, 0, "hfoo", Vec2::new(600.0, 600.0), 0.0)
        .expect("create");
    let unit = table.unit_of(handle).unwrap();
    sim.jass_apply_timed_life(&table, handle, 1.0)
        .expect("timed life");
    step_n(&mut sim, 20);
    assert!(sim.unit(unit).unwrap().alive);
    step_n(&mut sim, 20);
    assert!(!sim.unit(unit).unwrap().alive);
}

/// mechanic `jass.sim-bindings`: `UnitDamageTarget` applies typed damage.
#[test]
fn jass_damage_target_uses_the_typed_pipeline() {
    let mut sim = Sim::new(604, 40, 40);
    let mut table = SimJass::new();
    let a = sim
        .jass_create_unit(&mut table, 0, "ogru", Vec2::new(600.0, 600.0), 0.0)
        .expect("a");
    let b = sim
        .jass_create_unit(&mut table, 1, "hfoo", Vec2::new(700.0, 600.0), 0.0)
        .expect("b");
    let victim = table.unit_of(b).unwrap();
    let hp = sim.unit(victim).unwrap().hp;
    let dealt = sim
        .jass_unit_damage_target(&table, a, b, 100.0, AttackType::Normal)
        .expect("damage");
    assert!(dealt > 0, "typed damage dealt: {dealt}");
    assert_eq!(sim.unit(victim).unwrap().hp, hp - dealt);
}

/// mechanic `jass.sim-bindings`: ability natives validate against the data
/// table and refuse unknown ids.
#[test]
fn jass_ability_natives_validate_ids() {
    let mut sim = Sim::new(605, 40, 40);
    let mut table = SimJass::new();
    let handle = sim
        .jass_create_unit(&mut table, 0, "Hpal", Vec2::new(600.0, 600.0), 0.0)
        .expect("create");
    let unit = table.unit_of(handle).unwrap();
    sim.jass_unit_add_ability(&table, handle, "AHhb")
        .expect("known ability");
    assert!(sim.unit(unit).unwrap().learned.contains(&"AHhb"));
    assert_eq!(
        sim.jass_unit_add_ability(&table, handle, "Azzz"),
        Err("unknown ability")
    );
    sim.jass_unit_remove_ability(&table, handle, "AHhb")
        .expect("remove");
    assert!(!sim.unit(unit).unwrap().learned.contains(&"AHhb"));
}

/// mechanic `jass.sim-bindings`: `GetUnitTypeId`, `SetUnitOwner` and
/// `RemoveUnit` behave like the donor natives.
#[test]
fn jass_identity_owner_and_removal() {
    let mut sim = Sim::new(606, 40, 40);
    let mut table = SimJass::new();
    let handle = sim
        .jass_create_unit(&mut table, 0, "ogru", Vec2::new(600.0, 600.0), 0.0)
        .expect("create");
    assert_eq!(sim.jass_unit_type_id(&table, handle).unwrap(), "ogru");
    sim.jass_set_unit_owner(&table, handle, 1).expect("owner");
    let unit = table.unit_of(handle).unwrap();
    assert_eq!(sim.unit(unit).unwrap().owner, 1);
    sim.jass_remove_unit(&mut table, handle).expect("remove");
    assert!(table.unit_of(handle).is_none());
    assert!(!sim.unit(unit).unwrap().alive);
    assert_eq!(
        sim.jass_get_unit_pos(&table, handle),
        Err("invalid unit handle")
    );
}

/// Handles are stable and iteration order is deterministic.
#[test]
fn jass_handles_are_stable_and_sorted() {
    let mut sim = Sim::new(607, 40, 40);
    let mut table = SimJass::new();
    let mut handles = Vec::new();
    for slot in 0..5 {
        handles.push(
            sim.jass_create_unit(
                &mut table,
                0,
                "hfoo",
                Vec2::new(600.0 + slot as f32 * 40.0, 600.0),
                0.0,
            )
            .expect("create"),
        );
    }
    // Re-binding an existing unit returns the same handle.
    let unit = table.unit_of(handles[0]).unwrap();
    assert_eq!(table.bind(unit), handles[0]);
    let sorted = table.handles();
    assert!(sorted.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(table.len(), 5);
}
