//! Shared test helpers.  Every test is tick-based; nothing sleeps or uses
//! wall-clock time for gameplay assertions.

#![allow(dead_code)]

use wc3_sim::Vec2;
use wc3_sim::{Command, Event, Sim, UnitId};

pub fn hall(sim: &mut Sim, owner: u8, pos: Vec2) -> UnitId {
    sim.spawn_def(owner, "htow", pos).expect("town hall")
}

pub fn worker(sim: &mut Sim, owner: u8, pos: Vec2) -> UnitId {
    sim.spawn_def(owner, "hpea", pos).expect("peasant")
}

pub fn footman(sim: &mut Sim, owner: u8, pos: Vec2) -> UnitId {
    sim.spawn_def(owner, "hfoo", pos).expect("footman")
}

pub fn step_n(sim: &mut Sim, ticks: u32) {
    for _ in 0..ticks {
        sim.step(&[]);
    }
}

/// Steps while collecting every event emitted by the simulation.
pub fn step_collect(sim: &mut Sim, ticks: u32) -> Vec<Event> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        sim.step(&[]);
        events.extend(sim.events.iter().cloned());
    }
    events
}

pub fn has_error(events: &[Event], needle: &str) -> bool {
    events
        .iter()
        .any(|event| matches!(event, Event::Error(text) if text.contains(needle)))
}

/// Boots a JASS script against the shipped headers (`common.j` + `Blizzard.j`)
/// and a sandbox `Sim`, the same way the audit tests drive natives. The
/// headers are in the tree, so a missing directory is a test failure, not a
/// skip: a run without them would not exercise the real native surface.
pub fn scripted(script: &str) -> wc3_sim::jass::ScriptedSim {
    use wc3_sim::jass::{ScriptBundle, ScriptedSim};
    let mut bundle = ScriptBundle::new();
    let dir = wc3_sim::jass::find_game_scripts().expect("game headers (common.j/Blizzard.j)");
    bundle.add_game_scripts(&dir).expect("headers");
    bundle.add_source("audit_test.j", script).expect("parse");
    let mut sim = Sim::new(0x1A55, 64, 64);
    sim.passive_income = false;
    sim.resources = [100, 100];
    sim.lumber = [100, 100];
    ScriptedSim::new(sim, &bundle, 0x1A55).expect("compile")
}

pub fn gold_events(events: &[Event]) -> Vec<(u8, i32)> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::GoldDeposited { owner, amount } => Some((*owner, *amount)),
            _ => None,
        })
        .collect()
}

/// Fresh simulation with one complete hall, passive income disabled and a
/// large gold reserve so tests can exercise one system at a time.
pub fn sandbox(seed: u64) -> Sim {
    let mut sim = Sim::new(seed, 64, 64);
    sim.passive_income = false;
    sim.resources = [100_000, 100_000];
    hall(&mut sim, 0, Vec2::new(512.0, 512.0));
    sim.step(&[]);
    sim
}

pub fn queue_move(sim: &mut Sim, units: &[UnitId], to: Vec2) {
    sim.step(&[Command::Move {
        units: units.to_vec(),
        to,
    }]);
}
