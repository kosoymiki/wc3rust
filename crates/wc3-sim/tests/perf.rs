//! Performance gate: 200+ units at a 60 Hz budget on the phone CPU.
//!
//! Run explicitly in release with:
//!   cargo test -p wc3-sim --release --test perf -- --ignored --nocapture

mod common;

use std::time::Instant;

use common::*;
use wc3_sim::{Sim, Vec2};

#[test]
#[ignore = "performance gate: needs a --release build; run_layer_gates.sh --ignored"]
fn two_hundred_units_run_at_sixty_hz_budget() {
    let mut sim = Sim::new(4242, 64, 64);
    for i in 0..200 {
        let pos = Vec2::new(
            300.0 + (i % 20) as f32 * 120.0,
            300.0 + (i / 20) as f32 * 120.0,
        );
        sim.spawn_def(0, if i % 3 == 0 { "hfoo" } else { "hrif" }, pos)
            .unwrap();
        let enemy_pos = Vec2::new(
            5600.0 + (i % 20) as f32 * 60.0,
            5600.0 + (i / 20) as f32 * 60.0,
        );
        sim.spawn_def(1, "hfoo", enemy_pos).unwrap();
    }
    sim.add_gold_mine(Vec2::new(2000.0, 2000.0), 50_000);
    let ids: Vec<_> = sim
        .units_of(0)
        .filter(|unit| !unit.def().stats.is_worker)
        .map(|unit| unit.id)
        .collect();
    sim.attack_move(&ids, Vec2::new(5000.0, 5000.0));

    // Warm up paths and spatial structures.
    step_n(&mut sim, 120);
    let start = Instant::now();
    let ticks = 600u32;
    step_n(&mut sim, ticks);
    let elapsed = start.elapsed();
    let per_tick_ms = elapsed.as_secs_f64() * 1000.0 / ticks as f64;
    println!(
        "PERF units={} ticks={} total_ms={:.2} per_tick_ms={:.3} hash={:016x}",
        sim.units.len(),
        ticks,
        elapsed.as_secs_f64() * 1000.0,
        per_tick_ms,
        sim.state_hash()
    );
    assert!(sim.units.len() >= 400, "benchmark uses 400 units");
    assert!(
        per_tick_ms < 16.67,
        "60 Hz budget (16.67 ms/tick) exceeded: {per_tick_ms:.3} ms"
    );
}

#[test]
#[ignore = "performance gate: needs a --release build; run_layer_gates.sh --ignored"]
fn pathfinding_burst_is_bounded() {
    let mut sim = Sim::new(99, 64, 64);
    for i in 0..200 {
        let pos = Vec2::new(
            200.0 + (i % 20) as f32 * 100.0,
            200.0 + (i / 20) as f32 * 100.0,
        );
        sim.spawn_def(0, "hfoo", pos).unwrap();
    }
    // Wall forces real A* searches.
    for y in 10..54 {
        sim.grid.set_walkable(32, y, false);
    }
    sim.invalidate_paths();
    let ids: Vec<_> = sim.units.iter().map(|unit| unit.id).collect();
    let start = Instant::now();
    sim.move_group(&ids, Vec2::new(7000.0, 3000.0), wc3_sim::Formation::Box);
    step_n(&mut sim, 120);
    let elapsed = start.elapsed();
    println!("PERF burst_path_ms={:.2}", elapsed.as_secs_f64() * 1000.0);
    assert!(elapsed.as_secs_f64() < 2.0, "path burst bounded");
}
