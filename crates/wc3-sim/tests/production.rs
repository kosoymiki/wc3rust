//! Production queues, tech requirements, rally points and construction.

mod common;

use common::*;
use wc3_sim::{Order, Sim, Vec2};

#[test]
fn production_queue_is_bounded() {
    let mut sim = sandbox(81);
    let hall_id = sim.units[0].id;
    for _ in 0..wc3_sim::PRODUCTION_QUEUE_MAX {
        sim.step(&[wc3_sim::Command::Train {
            building: hall_id,
            kind: wc3_sim::UnitKind::Peasant,
        }]);
    }
    assert_eq!(sim.unit(hall_id).unwrap().production.len(), 5);
    sim.step(&[wc3_sim::Command::Train {
        building: hall_id,
        kind: wc3_sim::UnitKind::Peasant,
    }]);
    assert_eq!(sim.unit(hall_id).unwrap().production.len(), 5);
    assert!(has_error(&sim.events, "production queue full"));
}

#[test]
fn cancel_refunds_and_advances_the_queue() {
    let mut sim = sandbox(82);
    let hall_id = sim.units[0].id;
    let cost = wc3_sim::def_of("hpea").unwrap().stats.cost;
    sim.step(&[wc3_sim::Command::Train {
        building: hall_id,
        kind: wc3_sim::UnitKind::Peasant,
    }]);
    sim.step(&[wc3_sim::Command::Train {
        building: hall_id,
        kind: wc3_sim::UnitKind::Peasant,
    }]);
    let before = sim.resources[0];
    sim.step(&[wc3_sim::Command::CancelTrain {
        building: hall_id,
        index: 0,
    }]);
    assert_eq!(sim.resources[0], before + cost);
    assert_eq!(sim.unit(hall_id).unwrap().production.len(), 1);
}

#[test]
fn rally_point_sends_new_units_there() {
    let mut sim = sandbox(83);
    let hall_id = sim.units[0].id;
    let rally = Vec2::new(1600.0, 1200.0);
    sim.set_rally(hall_id, Some(rally));
    sim.step(&[wc3_sim::Command::Train {
        building: hall_id,
        kind: wc3_sim::UnitKind::Peasant,
    }]);
    step_n(&mut sim, 520);
    let peasant_id = sim
        .units_of(0)
        .find(|unit| unit.def().id == "hpea" && unit.id != hall_id)
        .map(|unit| unit.id)
        .expect("trained peasant");
    assert!(matches!(
        sim.unit(peasant_id).unwrap().order,
        Order::Move(_)
    ));
    step_n(&mut sim, 600);
    let peasant = sim.unit(peasant_id).unwrap();
    assert!(
        peasant.pos.distance(rally) < 220.0,
        "rally reached: {:?}",
        peasant.pos
    );
}

#[test]
fn tech_requirements_are_enforced_for_data_driven_training() {
    let mut sim = sandbox(84);
    let barracks = sim.spawn_def(0, "hbar", Vec2::new(1400.0, 512.0)).unwrap();
    sim.step(&[]);
    // Rifleman needs a Blacksmith.
    assert_eq!(sim.train_def(barracks, "hrif"), Err("missing requirement"));
    sim.spawn_def(0, "hbla", Vec2::new(1800.0, 512.0));
    sim.step(&[]);
    sim.train_def(barracks, "hrif").expect("rifleman unlocked");
    assert_eq!(
        sim.unit(barracks).unwrap().production[0].def,
        wc3_sim::def_index("hrif").unwrap()
    );
}

#[test]
fn train_def_rejects_the_wrong_producer() {
    let mut sim = sandbox(85);
    let hall_id = sim.units[0].id;
    assert_eq!(sim.train_def(hall_id, "hfoo"), Err("cannot train here"));
    let barracks = sim.spawn_def(0, "hbar", Vec2::new(1400.0, 512.0)).unwrap();
    sim.train_def(barracks, "hfoo")
        .expect("footman from barracks");
}

#[test]
fn construction_site_completes_and_raises_supply() {
    let mut sim = Sim::new(86, 32, 32);
    sim.passive_income = false;
    sim.resources[0] = 10_000;
    hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    let builder = worker(&mut sim, 0, Vec2::new(700.0, 400.0));
    sim.step(&[]);
    let site = sim
        .place_building(builder, "hhou", Vec2::new(1100.0, 400.0))
        .expect("farm site");
    sim.step(&[]);
    let unit = sim.unit(site).unwrap();
    assert!(unit.construct.is_some());
    assert!(
        unit.hp < unit.max_hp / 4,
        "site starts mostly unbuilt: {}",
        unit.hp
    );
    assert_eq!(sim.supply_cap(0), 12, "site provides no supply yet");
    step_n(&mut sim, 1500);
    let unit = sim.unit(site).unwrap();
    assert!(unit.construct.is_none(), "farm completed");
    assert_eq!(unit.hp, unit.max_hp);
    assert_eq!(sim.supply_cap(0), 18);
}

#[test]
fn construction_requires_a_nearby_builder_and_free_ground() {
    let mut sim = Sim::new(87, 32, 32);
    sim.resources[0] = 100_000;
    hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    let far = worker(&mut sim, 0, Vec2::new(3000.0, 3000.0));
    assert_eq!(
        sim.place_building(far, "hhou", Vec2::new(400.0, 1000.0)),
        Err("builder too far")
    );
    let near = worker(&mut sim, 0, Vec2::new(700.0, 400.0));
    // Inside the town hall footprint.
    assert!(
        sim.place_building(near, "hhou", Vec2::new(400.0, 400.0))
            .is_err()
    );
}

#[test]
fn cancel_construction_refunds_and_frees_the_footprint() {
    let mut sim = Sim::new(88, 32, 32);
    sim.passive_income = false;
    sim.resources[0] = 10_000;
    hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    let builder = worker(&mut sim, 0, Vec2::new(700.0, 400.0));
    sim.step(&[]);
    let before = sim.resources[0];
    let site = sim
        .place_building(builder, "hhou", Vec2::new(1000.0, 400.0))
        .unwrap();
    let cost = wc3_sim::def_of("hhou").unwrap().stats.cost;
    assert_eq!(sim.resources[0], before - cost);
    let (cx, cy) = sim.grid.world_to_cell(Vec2::new(1000.0, 400.0));
    assert!(!sim.grid.walkable(cx, cy));
    assert!(sim.cancel_construction(site));
    assert_eq!(sim.resources[0], before);
    assert!(sim.grid.walkable(cx, cy));
    assert!(!sim.unit(site).unwrap().alive);
}

#[test]
fn town_hall_build_keeps_the_legacy_instant_path_for_the_game_hud() {
    let mut sim = Sim::new(89, 32, 32);
    sim.resources[0] = 10_000;
    let hall_id = hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    let builder = worker(&mut sim, 0, Vec2::new(700.0, 400.0));
    sim.step(&[]);
    let before = sim.supply_cap(0);
    sim.step(&[wc3_sim::Command::Build {
        builder,
        kind: wc3_sim::UnitKind::TownHall,
        at: Vec2::new(1500.0, 400.0),
    }]);
    let halls = sim
        .units_of(0)
        .filter(|unit| unit.def().id == "htow")
        .count();
    assert_eq!(halls, 2);
    assert!(sim.supply_cap(0) > before);
    assert!(
        sim.events
            .iter()
            .any(|event| matches!(event, wc3_sim::Event::BuildingComplete(_)))
    );
    let _ = hall_id;
}

#[test]
fn trained_unit_spawns_on_walkable_ground() {
    let mut sim = sandbox(90);
    let hall_id = sim.units[0].id;
    sim.step(&[wc3_sim::Command::Train {
        building: hall_id,
        kind: wc3_sim::UnitKind::Peasant,
    }]);
    step_n(&mut sim, 600);
    let peasant = sim
        .units_of(0)
        .find(|unit| unit.def().id == "hpea")
        .unwrap();
    let cell = sim.grid.world_to_cell(peasant.pos);
    assert!(sim.grid.walkable(cell.0, cell.1));
    assert!(peasant.pos.distance(sim.unit(hall_id).unwrap().pos) > 64.0);
}
