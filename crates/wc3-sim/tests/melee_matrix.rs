//! mechanic `melee.matrix`: 4 races, 1v1..4v4, halls/mines/workers/creeps,
//! expansion mines with guarded camps.

use std::path::Path;

use wc3_sim::{AiConfig, Race, SeatController, Sim};

fn worker_for(race: Race) -> &'static str {
    match race {
        Race::Human => "hpea",
        Race::Orc => "opeo",
        Race::Undead => "uaco",
        Race::NightElf => "ewsp",
        Race::Neutral => "hpea",
    }
}

fn ai_config(race: Race) -> AiConfig {
    AiConfig {
        workers: 3,
        army_supply: 6,
        first_attack_tick: 900,
        wave_interval: 1200,
        retreat_permille: 300,
        composition: wc3_sim::ai_composition_for(race),
        worker: worker_for(race),
    }
}

#[test]
fn melee_matrix_4_races_sizes_1_4() {
    let races = [Race::Orc, Race::Human, Race::Undead, Race::NightElf];
    let mut cases = 0usize;
    let mut seats_total = 0usize;
    let mut halls_total = 0usize;
    let mut start_mines_total = 0usize;
    let mut expansion_total = 0usize;
    let mut workers_total = 0usize;
    let mut creeps_total = 0usize;
    let mut camps_total = 0usize;

    for (player_index, player_race) in races.iter().enumerate() {
        for (ai_index, ai_race) in races.iter().enumerate() {
            for team_size in 1..=4usize {
                cases += 1;
                seats_total += team_size * 2;
                let mut seats: Vec<(Race, SeatController)> = Vec::new();
                for _ in 0..team_size {
                    seats.push((*player_race, SeatController::Human));
                }
                for _ in 0..team_size {
                    seats.push((*ai_race, SeatController::Ai(ai_config(*ai_race))));
                }
                let seed = (player_index * 100 + ai_index * 10 + team_size) as u64;
                let mut sim = Sim::new(seed, 128, 128);
                sim.passive_income = false;
                let setup = sim.setup_melee(&seats, seed).unwrap_or_else(|error| {
                    panic!("{player_race:?} vs {ai_race:?} {team_size}v{team_size}: {error}")
                });

                assert_eq!(setup.seats, team_size * 2);
                assert_eq!(setup.halls, team_size * 2, "halls == seats");
                assert_eq!(setup.start_mines, team_size * 2, "start mines == seats");
                assert_eq!(setup.expansion_mines, 4, "2 expansion mines per side");
                assert_eq!(setup.workers, team_size * 10, "5 workers per seat");
                assert_eq!(setup.creeps, team_size * 10, "5 creeps per seat");
                assert_eq!(setup.camps, team_size * 2 + 4, "start + expansion camps");
                assert_eq!(sim.gold_mines.len(), team_size * 2 + 4);
                assert_eq!(sim.camps.len(), team_size * 2 + 4);
                assert_eq!(
                    sim.units
                        .iter()
                        .filter(|unit| unit.alive && unit.owner == 2)
                        .count(),
                    (team_size * 2 + 4) * 5,
                    "five neutral guards per camp"
                );
                assert!(sim.ai.iter().any(|ai| ai.owner == 1), "AI enabled");

                halls_total += setup.halls;
                start_mines_total += setup.start_mines;
                expansion_total += setup.expansion_mines;
                workers_total += setup.workers;
                creeps_total += setup.creeps;
                camps_total += setup.camps;

                // Halls per owner and no non-building unit inside a footprint.
                let blocked: Vec<(u32, u32)> = sim
                    .units
                    .iter()
                    .filter(|unit| unit.alive && unit.is_building())
                    .flat_map(|unit| unit.blocked_cells.iter().copied())
                    .collect();
                for unit in sim
                    .units
                    .iter()
                    .filter(|unit| unit.alive && !unit.is_building())
                {
                    let cell = sim.grid.world_to_cell(unit.pos);
                    assert!(
                        sim.grid.walkable(cell.0, cell.1),
                        "{} on a blocked cell",
                        unit.def().id
                    );
                    assert!(
                        !blocked.contains(&cell),
                        "{} inside a building footprint at {cell:?}",
                        unit.def().id
                    );
                }
                let halls_for = |owner: u8| {
                    sim.units
                        .iter()
                        .filter(|unit| {
                            unit.alive
                                && unit.owner == owner
                                && unit.is_building()
                                && unit.def().main_hall
                        })
                        .count()
                };
                assert_eq!(halls_for(0), team_size);
                assert_eq!(halls_for(1), team_size);
                let workers_for = |owner: u8, race: Race| {
                    sim.units
                        .iter()
                        .filter(|unit| {
                            unit.alive && unit.owner == owner && unit.def().id == worker_for(race)
                        })
                        .count()
                };
                assert_eq!(workers_for(0, *player_race), team_size * 5);
                assert_eq!(workers_for(1, *ai_race), team_size * 5);
            }
        }
    }

    let mut race_tech = String::new();
    for (index, race) in races.iter().enumerate() {
        let units = wc3_sim::ROSTER
            .iter()
            .filter(|def| def.race == *race && !def.is_building())
            .count();
        let buildings = wc3_sim::ROSTER
            .iter()
            .filter(|def| def.race == *race && def.is_building())
            .count();
        let trainable = wc3_sim::ROSTER
            .iter()
            .filter(|def| {
                def.race == *race && !def.is_building() && !def.trained_at.is_empty()
            })
            .count();
        let upgrades = wc3_sim::data::UPGRADES
            .iter()
            .filter(|upgrade| upgrade.race == *race)
            .count();
        if index > 0 {
            race_tech.push(',');
        }
        race_tech.push_str(&format!(
            "\"{race:?}\": {{\"units\": {units}, \"buildings\": {buildings}, \"trainable\": {trainable}, \"upgrades\": {upgrades}}}"
        ));
    }
    let json = format!(
        "{{\n  \"races\": [\"Orc\", \"Human\", \"Undead\", \"NightElf\"],\n  \"sizes\": [1, 2, 3, 4],\n  \"cases\": {cases},\n  \"seats\": {seats_total},\n  \"halls\": {halls_total},\n  \"start_mines\": {start_mines_total},\n  \"expansion_mines\": {expansion_total},\n  \"workers\": {workers_total},\n  \"creeps\": {creeps_total},\n  \"camps\": {camps_total},\n  \"race_tech\": {{{race_tech}}},\n  \"ok\": true\n}}\n"
    );
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/tech-lead/melee_matrix.json");
    std::fs::write(&path, json).expect("write melee_matrix.json");
}
