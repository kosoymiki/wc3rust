//! Determinism and replay-shape tests: same seed + same command log must
//! produce bit-identical state hashes, including with the AI running.

mod common;

use common::*;
use wc3_sim::{Sim, Vec2};

fn scripted_run(seed: u64) -> Vec<u64> {
    let mut sim = Sim::new(seed, 48, 48);
    sim.passive_income = false;
    hall(&mut sim, 0, Vec2::new(600.0, 600.0));
    hall(&mut sim, 1, Vec2::new(5400.0, 5400.0));
    sim.add_gold_mine(Vec2::new(1200.0, 600.0), 50_000);
    let mut ids = Vec::new();
    for i in 0..24 {
        ids.push(
            sim.spawn_def(
                0,
                if i % 2 == 0 { "hfoo" } else { "hrif" },
                Vec2::new(500.0 + (i % 6) as f32 * 50.0, 900.0 + (i / 6) as f32 * 50.0),
            )
            .unwrap(),
        );
    }
    for i in 0..12 {
        sim.spawn_def(
            1,
            if i % 3 == 0 { "hkni" } else { "hfoo" },
            Vec2::new(
                5000.0 + (i % 4) as f32 * 60.0,
                5000.0 + (i / 4) as f32 * 60.0,
            ),
        )
        .unwrap();
    }
    let peasant = sim.spawn_def(0, "hpea", Vec2::new(700.0, 700.0)).unwrap();
    sim.issue(
        &[peasant],
        wc3_sim::OrderSpec::Harvest(Vec2::new(1200.0, 600.0)),
    );
    sim.attack_move(&ids, Vec2::new(2600.0, 2600.0));
    let mut hashes = Vec::new();
    for tick in 0..1200u32 {
        match tick {
            120 => {
                sim.queue_order(
                    &ids[..6],
                    wc3_sim::OrderSpec::Move(Vec2::new(900.0, 2400.0)),
                );
            }
            300 => {
                let target = sim
                    .units_of(1)
                    .find(|unit| unit.alive)
                    .map(|unit| unit.id)
                    .unwrap();
                sim.issue(&ids[..8], wc3_sim::OrderSpec::Attack(target));
            }
            600 => {
                sim.patrol(
                    &ids[8..12],
                    Vec2::new(700.0, 3000.0),
                    Vec2::new(3400.0, 700.0),
                );
            }
            900 => {
                sim.hold(&ids[12..16]);
            }
            1100 => {
                sim.stop(&ids[16..20]);
            }
            _ => {}
        }
        sim.step(&[]);
        if tick % 50 == 0 {
            hashes.push(sim.state_hash());
        }
    }
    hashes
}

#[test]
fn scripted_command_log_is_bit_identical() {
    assert_eq!(scripted_run(2024), scripted_run(2024));
}

#[test]
fn different_seed_changes_the_damage_rolls() {
    let a = scripted_run(1);
    let b = scripted_run(2);
    assert_ne!(a.last(), b.last(), "RNG seed participates in the hash");
}

#[test]
fn ai_run_is_reproducible() {
    let run = || {
        let mut sim = Sim::new(555, 48, 48);
        sim.passive_income = false;
        sim.resources = [15_000, 15_000];
        hall(&mut sim, 0, Vec2::new(5200.0, 5200.0));
        hall(&mut sim, 1, Vec2::new(700.0, 700.0));
        sim.add_gold_mine(Vec2::new(1200.0, 700.0), 50_000);
        worker(&mut sim, 1, Vec2::new(900.0, 800.0));
        sim.step(&[]);
        sim.enable_ai(
            1,
            wc3_sim::AiConfig {
                workers: 3,
                army_supply: 4,
                first_attack_tick: 600,
                wave_interval: 600,
                retreat_permille: 300,
                composition: &[("hfoo", 1)],
                worker: "hpea",
            },
        );
        step_n(&mut sim, 2400);
        sim.state_hash()
    };
    assert_eq!(run(), run());
}
