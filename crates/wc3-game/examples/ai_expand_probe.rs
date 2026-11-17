//! Why does the AI not expand? Runs the same melee setup as
//! `tests/ai_race.rs` and prints the AI state, resources and mine distances
//! every 2000 ticks.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-game --example ai_expand_probe --offline
//! ```

use wc3_sim::{AiConfig, AiMode, Race, SeatController, Sim};

fn main() {
    let seed = 0xA1374u64;
    let race = Race::Human;
    let config = AiConfig {
        workers: 5,
        army_supply: 4,
        first_attack_tick: 600,
        wave_interval: 600,
        retreat_permille: 900,
        composition: wc3_sim::ai_composition_for(race),
        worker: "hpea",
    };
    let mut sim = Sim::new(seed, 128, 128);
    sim.passive_income = false;
    sim.resources = [2_000, 2_000];
    let setup = sim
        .setup_melee(
            &[
                (Race::Human, SeatController::Human),
                (race, SeatController::Ai(config)),
            ],
            seed,
        )
        .expect("melee setup");
    let hall = sim.unit(setup.player_hall).unwrap().pos;
    println!("PROBE player_hall=({:.0},{:.0})", hall.x, hall.y);

    for tick in 0..22_000u32 {
        sim.step(&[]);
        if tick % 2000 == 0 {
            let state = sim.ai.iter().find(|s| s.owner == 1);
            let mines: Vec<String> = sim
                .gold_mines
                .iter()
                .map(|m| format!("({:.0},{:.0})r{}", m.pos.x, m.pos.y, m.remaining))
                .collect();
            let buildings = sim
                .units
                .iter()
                .filter(|u| u.alive && u.owner == 1 && u.is_building())
                .count();
            let constructing = sim
                .units
                .iter()
                .filter(|u| u.alive && u.owner == 1 && u.construct.is_some())
                .count();
            println!(
                "t={tick:>5} gold={:>5} expanded={:?} mode={:?} next_expand={:?} mine={:?} buildings={buildings} constructing={constructing} mines={}",
                sim.resources[1],
                state.map(|s| s.expanded),
                state.map(|s| s.mode),
                state.map(|s| s.next_expand),
                state.and_then(|s| s.expand_mine).map(|m| format!("({:.0},{:.0})", m.x, m.y)),
                mines.join(" ")
            );
            if sim.ai.iter().any(|s| s.mode == AiMode::Retreat) {
                println!("t={tick}: retreat observed");
            }
        }
    }
    let state = sim.ai.iter().find(|s| s.owner == 1).unwrap();
    println!("PROBE final expanded={} waves={}", state.expanded, state.waves);
}
