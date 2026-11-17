//! Four-race roster: every unit trainable, every building placeable, all data
//! sourced from the generated WC3 tables.

mod common;

use common::*;
use wc3_sim::{ROSTER, Race, Sim, Vec2, data, def_of};

#[test]
fn roster_has_real_wc3_counts() {
    let counts = |race: Race| ROSTER.iter().filter(|def| def.race == race).count();
    // Melee roster + the TFT units each race trains in the extracted tables
    // (human: Spell Breaker, Dragonhawk Rider, upgraded Siege Engine; orc:
    // Troll Berserker, Troll Batrider, Spirit Walker; undead: Obsidian Statue;
    // night elf: Mountain Giant, Faerie Dragon); the neutral group is the
    // Tavern (`ntav`) and its five heroes present in the local TFT extract.
    assert_eq!(counts(Race::Human), 32);
    assert_eq!(counts(Race::Orc), 28);
    assert_eq!(counts(Race::Undead), 28);
    assert_eq!(counts(Race::NightElf), 28);
    assert_eq!(counts(Race::Neutral), 6);
    assert_eq!(ROSTER.len(), 122);
}

#[test]
fn every_unit_def_has_stats_and_a_producer() {
    for def in ROSTER.iter().filter(|def| !def.is_building()) {
        assert!(def.stats.hp > 0, "{} hp", def.id);
        assert!(def.stats.cost >= 0, "{} cost", def.id);
        assert!(!def.model.is_empty(), "{} model", def.id);
        assert!(!def.trained_at.is_empty(), "{} has no producer", def.id);
        for producer in def.trained_at {
            assert!(
                def_of(producer).is_some(),
                "{} producer {}",
                def.id,
                producer
            );
        }
        for required in def.requires {
            assert!(
                def_of(required).is_some(),
                "{} requires {}",
                def.id,
                required
            );
        }
    }
}

#[test]
fn every_building_def_has_a_model_and_footprint() {
    for def in ROSTER.iter().filter(|def| def.is_building()) {
        assert!(def.stats.hp > 0, "{} hp", def.id);
        assert!(def.stats.cost >= 0, "{} cost", def.id);
        assert!(!def.model.is_empty(), "{} model", def.id);
        assert!(
            def.footprint >= 1 && def.footprint <= 8,
            "{} footprint",
            def.id
        );
    }
}

#[test]
fn every_unit_can_be_trained_from_its_real_producer() {
    let mut failures = Vec::new();
    for def in ROSTER.iter().filter(|def| !def.is_building()) {
        let mut sim = Sim::new(100, 48, 48);
        sim.passive_income = false;
        sim.resources = [1_000_000, 1_000_000];
        // Owner 1 is inert so the trained unit is not attacked.
        let _ = sim.spawn_def(1, "htow", Vec2::new(100.0, 100.0));
        let mut lane = 0u32;
        let spawn_building = |sim: &mut Sim, id: &str, lane: &mut u32| -> Option<wc3_sim::UnitId> {
            let x = 600.0 + (*lane % 5) as f32 * 600.0;
            let y = 2600.0 + (*lane / 5) as f32 * 600.0;
            *lane += 1;
            sim.spawn_def(0, id, Vec2::new(x, y))
        };
        // Hall for supply + all producers and prerequisites.
        let hall = sim.spawn_def(0, "htow", Vec2::new(400.0, 400.0));
        assert!(hall.is_some());
        for producer in def.trained_at {
            if def_of(producer).is_some() {
                let _ = spawn_building(&mut sim, producer, &mut lane);
            }
        }
        for required in def.requires {
            if def_of(required).is_some() {
                let _ = spawn_building(&mut sim, required, &mut lane);
            }
        }
        sim.step(&[]);
        let mut producer = None;
        for candidate in def.trained_at {
            if let Some(unit) = sim
                .units_of(0)
                .find(|unit| unit.alive && unit.def().id == *candidate)
            {
                producer = Some(unit.id);
                break;
            }
        }
        let Some(producer) = producer else {
            failures.push(format!("{}: no producer instance", def.id));
            continue;
        };
        if let Err(error) = sim.train_def(producer, def.id) {
            failures.push(format!("{}: train_def {}", def.id, error));
            continue;
        }
        let mut spawned = false;
        for _ in 0..(def.stats.train_ticks + 240) {
            sim.step(&[]);
            if sim
                .units_of(0)
                .any(|unit| unit.alive && unit.def().id == def.id)
            {
                spawned = true;
                break;
            }
        }
        if !spawned {
            failures.push(format!("{}: never spawned", def.id));
        }
    }
    assert!(
        failures.is_empty(),
        "roster training failures: {failures:?}"
    );
}

fn grant_requirements(sim: &mut Sim, def_id: &str, lane: &mut u32) {
    let Some(def) = wc3_sim::def_of(def_id) else {
        return;
    };
    for required in def.requires {
        grant_requirements(sim, required, lane);
    }
    let x = 3300.0 + (*lane % 6) as f32 * 360.0;
    let y = 5000.0 + (*lane / 6) as f32 * 360.0;
    *lane += 1;
    let _ = sim.spawn_def(0, def_id, Vec2::new(x, y));
}

#[test]
fn every_building_can_be_constructed_by_a_worker() {
    let mut failures = Vec::new();
    for def in ROSTER.iter().filter(|def| def.is_building()) {
        let mut sim = Sim::new(101, 48, 48);
        sim.passive_income = false;
        sim.resources = [1_000_000, 1_000_000];
        sim.lumber = [1_000_000, 1_000_000];
        hall(&mut sim, 0, Vec2::new(400.0, 400.0));
        let mut lane = 0;
        for required in def.requires {
            grant_requirements(&mut sim, required, &mut lane);
        }
        let worker_def = wc3_sim::ai::worker_for_race(def.race);
        // The 128-unit grid makes the site footprint 2 cells wide, so the
        // builder stands one cell away (1700 -> 2000) to stay outside it; the
        // placement rule refuses a footprint with a unit standing inside.
        let builder = sim
            .spawn_def(0, worker_def, Vec2::new(1700.0, 2000.0))
            .expect("builder");
        sim.step(&[]);
        let site = Vec2::new(2000.0, 2000.0);
        let id = match sim.place_building(builder, def.id, site) {
            Ok(id) => id,
            Err(error) => {
                failures.push(format!("{}: place {}", def.id, error));
                continue;
            }
        };
        let mut done = false;
        for _ in 0..(def.stats.build_ticks + 600) {
            sim.step(&[]);
            if sim
                .unit(id)
                .map(|unit| unit.construct.is_none())
                .unwrap_or(false)
            {
                done = true;
                break;
            }
        }
        if !done {
            failures.push(format!("{}: never completed", def.id));
            continue;
        }
        let unit = sim.unit(id).unwrap();
        if unit.hp != unit.max_hp {
            failures.push(format!("{}: hp {}/{}", def.id, unit.hp, unit.max_hp));
        }
        if def.stats.supply_provided > 0 {
            let cap = sim.supply_cap(0);
            if cap < def.stats.supply_provided.min(wc3_sim::MAX_SUPPLY) {
                failures.push(format!("{}: supply cap {}", def.id, cap));
            }
        }
        if def.is_building() && unit.blocked_cells.is_empty() {
            failures.push(format!("{}: no footprint", def.id));
        }
    }
    assert!(
        failures.is_empty(),
        "building construction failures: {failures:?}"
    );
}

#[test]
fn all_unit_models_are_exposed_for_the_data_layer() {
    let mut present = 0;
    for def in ROSTER.iter() {
        if def.model_present {
            present += 1;
        }
    }
    assert!(
        present >= 80,
        "at least 80 of 98 models exist in the local RoC archive, got {present}"
    );
    // The data layer can enumerate every selectable model.
    assert!(ROSTER.iter().all(|def| !def.model.is_empty()));
}

const RACES: [Race; 4] = [Race::Human, Race::Orc, Race::Undead, Race::NightElf];

/// The Orc peon build list in `OrcUnitFunc.txt:162` (`Builds=`), one entry per
/// WC3 building the card offers.  Every one must resolve.
const ORC_BUILDS: [&str; 9] = ["ogre", "otrb", "obar", "ofor", "oalt", "obea", "osld", "otto", "owtw"];

#[test]
fn data_build_train_resolution_every_race() {
    for race in RACES {
        let hall = data::main_hall_for(race).unwrap_or_else(|| panic!("{race:?}: no main hall"));
        let altar = data::altar_for(race).unwrap_or_else(|| panic!("{race:?}: no altar"));
        let supply =
            data::supply_building_for(race).unwrap_or_else(|| panic!("{race:?}: no supply"));
        let menu = data::build_menu(race);
        assert!(menu.len() >= 5, "{race:?}: build menu {menu:?}");
        for def in &menu {
            let resolved = def_of(def.id)
                .unwrap_or_else(|| panic!("{race:?}: build-menu id {} does not resolve", def.id));
            assert_eq!(resolved.race, race, "{} card entry for another race", def.id);
            assert!(
                resolved.is_building() && resolved.buildable,
                "{race:?}: {} is not a worker-buildable building",
                def.id
            );
            assert!(
                resolved.stats.cost > 0,
                "{race:?}: {} has no gold cost",
                def.id
            );
        }
        for def in ROSTER.iter().filter(|def| def.race == race) {
            for id in def.trains {
                let trainee = def_of(id)
                    .unwrap_or_else(|| panic!("{} trains unknown id {id}", def.id));
                assert!(
                    trainee.trained_at.contains(&def.id),
                    "{} trains {id} but {id}.trained_at = {:?}",
                    def.id,
                    trainee.trained_at
                );
            }
            for id in def.trained_at {
                assert!(def_of(id).is_some(), "{} trained_at unknown id {id}", def.id);
            }
            for id in def.requires {
                assert!(def_of(id).is_some(), "{} requires unknown id {id}", def.id);
            }
            for id in def.sells_items {
                assert!(
                    wc3_sim::data::item_def(id).is_some(),
                    "{} sells unknown item {id}",
                    def.id
                );
            }
        }
        let _ = (hall, altar, supply);
    }
}

/// The Orc peon's nine build entries: every one spawns as a real building and
/// accepts construction (WC3 melee parity).
#[test]
fn data_build_train_resolution_orc_peon_all_nine_builds() {
    for id in ORC_BUILDS {
        let def = def_of(id).unwrap_or_else(|| panic!("Orc build {id} does not resolve"));
        assert!(def.is_building(), "{id} is not a building");
        assert!(def.buildable, "{id} is not worker-buildable");
        let mut sim = Sim::new(7, 64, 64);
        sim.passive_income = false;
        sim.resources = [100_000, 100_000];
        sim.lumber = [100_000, 100_000];
        let peon = sim
            .spawn_def(0, "opeo", Vec2::new(300.0, 300.0))
            .expect("peon");
        // Melee prerequisites are real: grant each required building first.
        let mut lane = 0u32;
        for required in def.requires {
            let mut stack = vec![*required];
            while let Some(req) = stack.pop() {
                let Some(req_def) = def_of(req) else { continue };
                for deeper in req_def.requires {
                    stack.push(deeper);
                }
                let x = 4200.0 + (lane % 8) as f32 * 360.0;
                let y = 4200.0 + (lane / 8) as f32 * 360.0;
                lane += 1;
                sim.spawn_def(0, req, Vec2::new(x, y));
            }
        }
        let worker = sim
            .spawn_def(0, "opeo", Vec2::new(600.0, 600.0))
            .expect("builder");
        sim.step(&[]);
        let site = sim.unit(worker).map(|unit| unit.pos + Vec2::new(240.0, 0.0)).unwrap();
        let placed = sim
            .place_building(worker, id, site)
            .unwrap_or_else(|error| panic!("{id}: place_building {error}"));
        for _ in 0..(def.stats.build_ticks + 600) {
            sim.step(&[]);
            if sim
                .unit(placed)
                .map(|unit| unit.construct.is_none())
                .unwrap_or(false)
            {
                break;
            }
        }
        let building = sim.unit(placed).expect("building alive");
        assert!(
            building.construct.is_none(),
            "{id}: never completed construction"
        );
        assert!(building.hp == building.max_hp, "{id}: incomplete hp");
        assert!(!building.blocked_cells.is_empty(), "{id}: no footprint");
        let _ = peon;
    }
}
