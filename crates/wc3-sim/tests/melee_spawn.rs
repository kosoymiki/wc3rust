//! Mechanic tests for the WC3 melee opening.
//!
//! These pin the three properties the operator reported as broken:
//! 1. every unit starts on walkable ground (never in water, never inside a
//!    building model), 2. both sides get five neutral creeps guarding their
//!    gold mine, 3. a trained unit leaves the producer's footprint instead of
//!    appearing in the middle of the building.

use glam::Vec2;
use wc3_sim::{CELL_SIZE, Race, Sim};

fn melee() -> Sim {
    let mut sim = Sim::new(7, 64, 64);
    let setup = sim
        .setup_melee_1v1(
            Race::Orc,
            Race::Human,
            Vec2::new(3_000.0, 3_000.0),
            Vec2::new(6_000.0, 5_000.0),
        )
        .expect("melee opening");
    assert!(setup.player_hall.0 > 0);
    // Worker slots are derived from the hall's footprint; every slot the grid
    // moved is recorded as `SPAWN_ADJUST def=<id> from=<cell> to=<cell>`.
    for line in &sim.spawn_log {
        eprintln!("{line}");
    }
    assert!(
        sim.spawn_log
            .iter()
            .all(|line| line.starts_with("SPAWN_ADJUST def=")),
        "spawn log format: {:?}",
        sim.spawn_log
    );
    sim
}

fn footprint_clearance(sim: &Sim, hall: wc3_sim::UnitId) -> f32 {
    let unit = sim.unit(hall).expect("hall");
    unit.def().footprint.max(1) as f32 * CELL_SIZE * 0.5
}

#[test]
fn melee_opening_units_stand_on_walkable_ground_outside_the_hall() {
    let sim = melee();
    let halls: Vec<_> = sim
        .units
        .iter()
        .filter(|unit| unit.is_building())
        .map(|unit| (unit.owner, unit.pos, footprint_clearance(&sim, unit.id)))
        .collect();
    assert_eq!(halls.len(), 2, "one main hall per side");

    for unit in sim.units.iter().filter(|unit| !unit.is_building()) {
        let cell = sim.grid.world_to_cell(unit.pos);
        assert!(
            sim.grid.walkable(cell.0, cell.1),
            "unit {} spawned on an unwalkable cell at {:?}",
            unit.id.0,
            unit.pos
        );
        // Creeps are neutral; their hall clearance is checked via the camp test.
        let Some((_, hall_pos, clearance)) =
            halls.iter().find(|(owner, _, _)| *owner == unit.owner)
        else {
            continue;
        };
        assert!(
            unit.pos.distance(*hall_pos) > clearance + CELL_SIZE * 0.25,
            "unit {} at {:?} is still inside its hall (clearance {clearance})",
            unit.id.0,
            unit.pos
        );
    }
}

#[test]
fn melee_opening_fields_five_creeps_per_mine_as_one_camp() {
    let sim = melee();
    let creeps: Vec<_> = sim
        .units
        .iter()
        .filter(|unit| unit.owner == wc3_sim::NEUTRAL_OWNER)
        .collect();
    // 5 guards per starting mine (2) + 5 per expansion mine (4).
    assert_eq!(creeps.len(), 30, "five neutral creeps per mine");
    assert_eq!(sim.camps.len(), 6, "one camp per gold mine");
    for camp in &sim.camps {
        assert_eq!(camp.units.len(), 5, "camp owns its five guards");
        assert!(camp.aggro > 0.0 && camp.leash > 0.0);
    }
    for camp in &sim.camps {
        let hall_pos = sim
            .units
            .iter()
            .find(|unit| unit.is_building() && unit.owner <= 1)
            .map(|unit| unit.pos)
            .expect("hall");
        assert!(
            camp.anchor.distance(hall_pos) > 450.0,
            "camp at {:?} is too close to the hall at {hall_pos:?}; workers would be pulled",
            camp.anchor
        );
        assert!(
            camp.aggro <= 400.0 && camp.leash <= 600.0,
            "camp leash is too wide"
        );
    }
    for creep in &creeps {
        let cell = sim.grid.world_to_cell(creep.pos);
        assert!(
            sim.grid.walkable(cell.0, cell.1),
            "creep {} spawned off-ground at {:?}",
            creep.id.0,
            creep.pos
        );
        let near_camp = sim
            .camps
            .iter()
            .any(|camp| creep.pos.distance(camp.anchor) <= 400.0);
        assert!(near_camp, "creep {} is not guarding a camp", creep.id.0);
    }
}

#[test]
fn trained_units_leave_the_producer_footprint() {
    let mut sim = melee();
    let hall = sim
        .units
        .iter()
        .find(|unit| unit.owner == 0 && unit.is_building())
        .expect("player hall")
        .id;
    sim.resources[0] = 5_000;
    sim.lumber[0] = 5_000;
    let built = sim.units.iter().filter(|unit| unit.owner == 0).count();
    let log_before = sim.spawn_log.len();
    sim.train_def(hall, "opeo").expect("queue a worker");

    let mut ticks = 0;
    while sim.units.iter().filter(|unit| unit.owner == 0).count() == built && ticks < 3_000 {
        sim.step(&[]);
        ticks += 1;
    }
    assert!(ticks < 3_000, "worker never finished training");

    let hall_pos = sim.unit(hall).expect("hall").pos;
    let clearance = footprint_clearance(&sim, hall);
    let spawned = sim
        .units
        .iter()
        .filter(|unit| unit.owner == 0 && !unit.is_building())
        .max_by_key(|unit| unit.id.0)
        .expect("trained worker");
    let cell = sim.grid.world_to_cell(spawned.pos);
    assert!(sim.grid.walkable(cell.0, cell.1));
    assert!(
        spawned.pos.distance(hall_pos) > clearance,
        "trained unit {} spawned inside the hall at {:?}",
        spawned.id.0,
        spawned.pos
    );
    // The placement moved the unit out of the producer's footprint, and the
    // move is on the record: `SPAWN_ADJUST def=opeo from=<hall cell> to=<exit>`.
    let adjust = sim
        .spawn_log
        .iter()
        .skip(log_before)
        .find(|line| line.starts_with("SPAWN_ADJUST def=opeo from="))
        .unwrap_or_else(|| {
            panic!(
                "trained unit placement must be recorded: {:?}",
                sim.spawn_log
            )
        });
    eprintln!("{adjust}");
    assert!(
        adjust.contains(" to="),
        "SPAWN_ADJUST names both cells: {adjust}"
    );
}
