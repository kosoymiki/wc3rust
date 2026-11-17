//! Creep/auto-attack diagnosis on a bare arena (no map needed).
//!
//! Operator: «автоатака не работает, мобы бьют по одному разу и стопятся».
//! Three scenarios, each printing HP and the attacker's order over time:
//!   1. explicit Attack order (player unit -> enemy),
//!   2. neutral creep aggro (player unit walks into a camp),
//!   3. idle auto-acquire (two enemies adjacent, no order).
//!
//! ```text
//! scripts/h2-cargo run -p wc3-game --example attack_probe --offline
//! ```

use glam::Vec2;
use wc3_sim::{Command, Order, Sim};

fn hp(sim: &Sim, id: wc3_sim::UnitId) -> i32 {
    sim.unit(id).map(|u| u.hp).unwrap_or(0)
}

fn order_of(sim: &Sim, id: wc3_sim::UnitId) -> String {
    sim.unit(id)
        .map(|u| format!("{:?}", u.order))
        .unwrap_or_else(|| "gone".into())
}

fn run_scenario(name: &str, setup: impl Fn(&mut Sim) -> (wc3_sim::UnitId, wc3_sim::UnitId), order: bool) {
    let mut sim = Sim::new(0xA77AC, 64, 64);
    let (attacker, target) = setup(&mut sim);
    if order {
        sim.step(&[Command::Attack {
            units: vec![attacker],
            target,
        }]);
    }
    let mut last = hp(&sim, target);
    let mut swings = 0u32;
    let mut stopped_at = None;
    for tick in 1..=400 {
        sim.step(&[]);
        let now = hp(&sim, target);
        if now < last {
            swings += 1;
            last = now;
        } else if swings > 0 && stopped_at.is_none() && tick > 40 {
            stopped_at = Some(tick);
        }
        if now <= 0 {
            println!("{name}: target died at tick {tick} swings={swings}");
            return;
        }
    }
    println!(
        "{name}: swings={swings} target_hp={} order={} first_stop~{:?}",
        hp(&sim, target),
        order_of(&sim, attacker),
        stopped_at
    );
}

fn main() {
    // 1. Explicit attack: grunt vs footman, adjacent.
    run_scenario(
        "EXPLICIT",
        |sim| {
            let a = sim.spawn_def_exact(0, "ogru", Vec2::new(1000.0, 1000.0)).expect("grunt");
            let t = sim.spawn_def_exact(1, "hfoo", Vec2::new(1060.0, 1000.0)).expect("footman");
            (a, t)
        },
        true,
    );

    // 2. Creep aggro: a neutral camp guard vs a player unit standing next to it.
    run_scenario(
        "CREEP_AGGRO",
        |sim| {
            let t = sim.spawn_def_exact(0, "hfoo", Vec2::new(1000.0, 1000.0)).expect("footman");
            // Camp guard right next to the player unit; aggro/leash from retail.
            let camp_id = sim.spawn_camp(&["nban"], Vec2::new(1060.0, 1000.0), 500.0, 900.0);
            let a = camp_id
                .and_then(|id| {
                    sim.units
                        .iter()
                        .find(|u| u.alive && u.camp == Some(id))
                        .map(|u| u.id)
                })
                .unwrap_or(t);
            (a, t)
        },
        false,
    );

    // 3. Idle auto-acquire: no order at all.
    run_scenario(
        "AUTO",
        |sim| {
            let a = sim.spawn_def_exact(0, "ogru", Vec2::new(2000.0, 1000.0)).expect("grunt");
            let t = sim.spawn_def_exact(1, "hfoo", Vec2::new(2050.0, 1000.0)).expect("footman");
            (a, t)
        },
        false,
    );
}
