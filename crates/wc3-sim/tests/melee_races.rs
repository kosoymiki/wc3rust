//! mechanic `melee.race_*`: every roster unit/building is reachable through
//! the tech tree for each race, workers build structures, repair/supply work.

use wc3_sim::{Command, ROSTER, Race, Sim, UnitId, Vec2, data, def_of};

fn worker_for(race: Race) -> &'static str {
    match race {
        Race::Human => "hpea",
        Race::Orc => "opeo",
        Race::Undead => "uaco",
        Race::NightElf => "ewsp",
        Race::Neutral => "hpea",
    }
}

fn lane(index: usize) -> Vec2 {
    Vec2::new(
        700.0 + (index % 6) as f32 * 520.0,
        2400.0 + (index / 6) as f32 * 520.0,
    )
}

struct RaceResult {
    buildings: usize,
    units: usize,
    ability_only: usize,
    repair_ok: bool,
}

fn ensure_built(sim: &mut Sim, worker: UnitId, def_id: &str, lane_index: &mut usize) -> UnitId {
    if let Some(existing) = sim
        .units_of(0)
        .find(|unit| unit.alive && unit.def().id == def_id)
    {
        return existing.id;
    }
    let def = def_of(def_id).expect("def");
    for required in def.requires {
        ensure_built(sim, worker, required, lane_index);
    }
    let site = lane(*lane_index);
    *lane_index += 1;
    sim.teleport_unit(worker, site + Vec2::new(120.0, 0.0));
    let before_gold = sim.resources[0];
    let before_lumber = sim.lumber[0];
    let id = sim
        .place_building(worker, def_id, site)
        .unwrap_or_else(|error| panic!("{def_id}: place {error}"));
    assert_eq!(
        before_gold - sim.resources[0],
        def.stats.cost,
        "{def_id} gold cost"
    );
    assert_eq!(
        before_lumber - sim.lumber[0],
        def.stats.lumber_cost,
        "{def_id} lumber cost"
    );
    let mut done = false;
    for _ in 0..(def.stats.build_ticks + 900) {
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
    assert!(done, "{def_id}: never completed");
    id
}

fn place_copy(sim: &mut Sim, worker: UnitId, def_id: &str, lane_index: &mut usize) -> UnitId {
    let def = def_of(def_id).expect("def");
    let site = lane(*lane_index);
    *lane_index += 1;
    sim.teleport_unit(worker, site + Vec2::new(120.0, 0.0));
    let id = sim
        .place_building(worker, def_id, site)
        .expect("copy place");
    let mut done = false;
    for _ in 0..(def.stats.build_ticks + 900) {
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
    assert!(done, "{def_id}: copy never completed");
    id
}

fn run_race(race: Race) -> RaceResult {
    let mut sim = Sim::new(0x3740 + race as u64, 96, 96);
    sim.passive_income = false;
    sim.resources[0] = 1_000_000;
    sim.lumber[0] = 1_000_000;
    let hall = data::main_hall_for(race).expect("hall");
    let hall_id = sim
        .spawn_def_exact(0, hall.id, Vec2::new(500.0, 2200.0))
        .expect("hall");
    let worker = sim
        .spawn_def_exact(0, worker_for(race), Vec2::new(700.0, 2200.0))
        .expect("worker");
    let mut lane_index = 0usize;

    // Tier gate: the top hall is denied before the tier-2 hall exists.
    let (tier2, top) = match race {
        Race::Human => ("hkee", "hcas"),
        Race::Orc => ("ostr", "ofrt"),
        Race::Undead => ("unp1", "unp2"),
        Race::NightElf => ("etoa", "etoe"),
        Race::Neutral => ("htow", "hkee"),
    };
    let blocked_site = lane(40);
    sim.teleport_unit(worker, blocked_site + Vec2::new(120.0, 0.0));
    assert_eq!(
        sim.place_building(worker, top, blocked_site),
        Err("missing requirement"),
        "{race:?}: {top} without {tier2}"
    );
    ensure_built(&mut sim, worker, tier2, &mut lane_index);
    let _ = ensure_built(&mut sim, worker, top, &mut lane_index);

    let buildings: Vec<_> = ROSTER
        .iter()
        .filter(|def| def.race == race && def.is_building())
        .collect();
    for def in &buildings {
        ensure_built(&mut sim, worker, def.id, &mut lane_index);
    }

    // Extra supply so every unit can be trained.
    let supply = data::supply_building_for(race).expect("supply building");
    for _ in 0..20 {
        place_copy(&mut sim, worker, supply.id, &mut lane_index);
    }
    assert!(sim.supply_cap(0) > 0, "{race:?}: supply works");
    let _ = sim.upkeep(0);

    let ability_only = ["hmil", "ushd", "ehpr"];
    let units: Vec<_> = ROSTER
        .iter()
        .filter(|def| def.race == race && !def.is_building())
        .collect();
    let mut trained = 0usize;
    let mut skipped = 0usize;
    for def in &units {
        if def.trained_at.is_empty() {
            assert!(
                ability_only.contains(&def.id),
                "{race:?}: {} has no producer and is not ability-produced",
                def.id
            );
            skipped += 1;
            continue;
        }
        let producer = def
            .trained_at
            .iter()
            .find_map(|id| {
                sim.units_of(0)
                    .find(|unit| unit.alive && unit.def().id == *id)
                    .map(|unit| unit.id)
            })
            .unwrap_or_else(|| panic!("{race:?}: no producer for {}", def.id));
        // Real tech tree: `Requires=` buildings must stand before the producer
        // accepts the order (Spell Breaker needs the Arcane Vault, Obsidian
        // Statue the Tomb of Relics, Mountain Giant the Ancient of Wonders).
        for required in def.requires {
            ensure_built(&mut sim, worker, required, &mut lane_index);
        }
        let before_gold = sim.resources[0];
        let before_lumber = sim.lumber[0];
        let before_count = sim
            .units_of(0)
            .filter(|unit| unit.alive && unit.def().id == def.id)
            .count();
        sim.train_def(producer, def.id)
            .unwrap_or_else(|error| panic!("{race:?}: train {}: {error}", def.id));
        assert_eq!(
            before_gold - sim.resources[0],
            def.stats.cost,
            "{} gold",
            def.id
        );
        assert_eq!(
            before_lumber - sim.lumber[0],
            def.stats.lumber_cost,
            "{} lumber",
            def.id
        );
        let mut produced = false;
        for _ in 0..(def.stats.train_ticks + 600) {
            sim.step(&[]);
            if sim
                .units_of(0)
                .filter(|unit| unit.alive && unit.def().id == def.id)
                .count()
                > before_count
            {
                produced = true;
                break;
            }
        }
        assert!(produced, "{race:?}: {} never produced", def.id);
        trained += 1;
    }

    // Repair: damage the main hall and channel a worker.
    let hall_pos = sim.unit(hall_id).expect("hall").pos;
    let enemy = sim
        .spawn_def(1, "hfoo", hall_pos + Vec2::new(400.0, 0.0))
        .expect("enemy");
    sim.apply_damage(enemy, hall_id, 500, wc3_sim::AttackType::Normal);
    let damaged = sim.unit(hall_id).unwrap().hp;
    assert!(damaged < sim.unit(hall_id).unwrap().max_hp);
    sim.teleport_unit(worker, hall_pos + Vec2::new(120.0, 0.0));
    let gold_before = sim.resources[0];
    sim.step(&[Command::Repair {
        units: vec![worker],
        target: hall_id,
    }]);
    let mut repaired = false;
    for _ in 0..400 {
        sim.step(&[]);
        if sim.unit(hall_id).unwrap().hp > damaged {
            repaired = true;
            break;
        }
    }
    assert!(repaired, "{race:?}: repair healed nothing");
    assert!(
        sim.resources[0] <= gold_before,
        "{race:?}: repair must not mint gold"
    );

    RaceResult {
        buildings: buildings.len(),
        units: trained,
        ability_only: skipped,
        repair_ok: repaired,
    }
}

#[test]
fn melee_race_orc() {
    let result = run_race(Race::Orc);
    assert!(result.buildings >= 10 && result.units >= 8);
    assert!(result.ability_only <= 3 && result.repair_ok);
}

#[test]
fn melee_race_human() {
    let result = run_race(Race::Human);
    assert!(result.buildings >= 10 && result.units >= 8);
    assert!(result.ability_only <= 3 && result.repair_ok);
}

#[test]
fn melee_race_undead() {
    let result = run_race(Race::Undead);
    assert!(result.buildings >= 10 && result.units >= 8);
    assert!(result.ability_only <= 3 && result.repair_ok);
}

#[test]
fn melee_race_nightelf() {
    let result = run_race(Race::NightElf);
    assert!(result.buildings >= 10 && result.units >= 8);
    assert!(result.ability_only <= 3 && result.repair_ok);
}
