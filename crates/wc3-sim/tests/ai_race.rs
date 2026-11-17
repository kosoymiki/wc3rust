//! mechanic `ai.race_builds_and_expands`: per-race AI tech, expansion, army,
//! waves, retreat and worker rebuilds on a fixed seed.

use wc3_sim::{AiConfig, AiMode, Race, SeatController, Sim, Vec2};

fn ai_config(race: Race) -> AiConfig {
    AiConfig {
        workers: 5,
        army_supply: 4,
        first_attack_tick: 600,
        wave_interval: 600,
        retreat_permille: 900,
        composition: wc3_sim::ai_composition_for(race),
        worker: match race {
            Race::Human => "hpea",
            Race::Orc => "opeo",
            Race::Undead => "uaco",
            Race::NightElf => "ewsp",
            Race::Neutral => "hpea",
        },
    }
}

fn worker_id(race: Race) -> &'static str {
    match race {
        Race::Human => "hpea",
        Race::Orc => "opeo",
        Race::Undead => "uaco",
        Race::NightElf => "ewsp",
        Race::Neutral => "hpea",
    }
}

#[test]
fn ai_race_builds_and_expands() {
    for (index, race) in [Race::Human, Race::Orc, Race::Undead, Race::NightElf]
        .into_iter()
        .enumerate()
    {
        let seed = 0xA1374 + index as u64;
        let mut sim = Sim::new(seed, 128, 128);
        sim.passive_income = false;
        sim.resources = [2_000, 2_000];
        let setup = sim
            .setup_melee(
                &[
                    (Race::Human, SeatController::Human),
                    (race, SeatController::Ai(ai_config(race))),
                ],
                seed,
            )
            .expect("melee setup");
        let player_hall_pos = sim.unit(setup.player_hall).expect("player hall").pos;
        // `setup_melee` lays out halls, mines and camps but no forest. A real
        // melee map has wood or the lumber crew cannot exist (and the AI's
        // lumber-cost checks stall its build order). Give both bases a ring of
        // trees so the economy being asserted is the real one.
        for base in [
            player_hall_pos,
            sim.unit(setup.enemy_hall).expect("enemy hall").pos,
        ] {
            for ring in 0..2 {
                let radius = 700.0 + ring as f32 * 320.0;
                let trees = 28 + ring * 12;
                for index in 0..trees {
                    let angle = index as f32 * std::f32::consts::TAU / trees as f32;
                    let _ = sim.spawn_destructable(
                        "LTlt",
                        base + Vec2::new(angle.cos(), angle.sin()) * radius,
                    );
                }
            }
        }
        let mut saw_retreat = false;
        for tick in 0..22_000u32 {
            sim.step(&[]);
            // Deterministic retreat stimulus: the player seat is inert, so an
            // undamaged wave would never drop below `retreat_permille`. Cripple
            // the live army once; the retreat branch must then fire in the real
            // race scenario (it is separately unit-tested in tests/ai.rs).
            if tick == 10_000 {
                for id in sim.army_units(1) {
                    if let Some(unit) = sim.unit_mut(id) {
                        unit.hp = (unit.max_hp / 5).max(1);
                    }
                }
            }
            if sim
                .ai
                .iter()
                .any(|state| state.owner == 1 && state.mode == AiMode::Retreat)
            {
                saw_retreat = true;
            }
        }
        let ai = sim
            .ai
            .iter()
            .find(|state| state.owner == 1)
            .unwrap_or_else(|| panic!("{race:?}: AI state missing"));
        assert!(ai.expanded, "{race:?}: AI did not expand");
        assert!(ai.waves >= 1, "{race:?}: AI sent no wave");
        let ai_buildings = sim
            .units
            .iter()
            .filter(|unit| unit.alive && unit.owner == 1 && unit.is_building())
            .count();
        assert!(
            ai_buildings >= 3,
            "{race:?}: AI built only {ai_buildings} buildings"
        );
        assert!(
            sim.units.iter().any(|unit| {
                unit.alive && unit.owner == 1 && unit.is_building() && unit.def().trains_units()
            }),
            "{race:?}: AI has no unit producer"
        );
        let workers = sim
            .units
            .iter()
            .filter(|unit| unit.alive && unit.owner == 1 && unit.def().id == worker_id(race))
            .count();
        assert!(workers >= 5, "{race:?}: workers rebuilt only {workers}");
        let mut combat_defs: Vec<&str> = sim
            .units
            .iter()
            .filter(|unit| {
                unit.alive && unit.owner == 1 && !unit.is_building() && !unit.def().stats.is_worker
            })
            .map(|unit| unit.def().id)
            .collect();
        combat_defs.sort_unstable();
        combat_defs.dedup();
        assert!(
            combat_defs.len() >= 2,
            "{race:?}: army not mixed ({combat_defs:?})"
        );
        assert!(
            sim.units
                .iter()
                .filter(|unit| unit.alive && unit.owner == 1 && !unit.is_building())
                .count()
                >= 6,
            "{race:?}: army too small"
        );
        assert!(saw_retreat, "{race:?}: AI never retreated");
    }
}
