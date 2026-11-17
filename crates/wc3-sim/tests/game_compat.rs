//! Compile-time compatibility probe for the game/replay/HUD surface.
//!
//! The game crate owns its own rendering work and is edited by another agent;
//! this test pins every simulation field, variant and method those crates read
//! so a sim-side refactor cannot silently break them.  It is deliberately a
//! compile-time assertion plus a tiny behavioural check.

mod common;

use wc3_sim::{Ability, Command, Event, Order, Sim, UnitId, UnitKind, Vec2};

#[test]
fn game_surface_still_compiles_against_the_sim() {
    let mut sim = Sim::new(7, 32, 32);
    // The game assigns a fresh pathing grid after construction.
    sim.grid = wc3_path::Grid::new(32, 32, wc3_sim::CELL_SIZE);
    let hall = sim.spawn(0, UnitKind::TownHall, Vec2::new(300.0, 300.0));
    let peasant = sim.spawn(0, UnitKind::Peasant, Vec2::new(360.0, 300.0));
    let enemy = sim.spawn(1, UnitKind::Footman, Vec2::new(900.0, 300.0));

    let commands = vec![
        Command::Train {
            building: hall,
            kind: UnitKind::Footman,
        },
        Command::Move {
            units: vec![peasant],
            to: Vec2::new(700.0, 700.0),
        },
        Command::Attack {
            units: vec![peasant],
            target: enemy,
        },
        Command::Stop {
            units: vec![peasant],
        },
        Command::MoveAttack {
            units: vec![peasant],
            to: Vec2::new(1200.0, 900.0),
        },
        Command::Hold {
            units: vec![peasant],
        },
        Command::Harvest {
            units: vec![peasant],
            to: Vec2::new(1400.0, 300.0),
        },
        Command::CancelTrain {
            building: hall,
            index: 0,
        },
        Command::Build {
            builder: peasant,
            kind: UnitKind::TownHall,
            at: Vec2::new(900.0, 900.0),
        },
        Command::Cast {
            unit: peasant,
            ability: Ability::Heal(30),
            target: Some(enemy),
        },
    ];
    sim.step(&commands);

    // Field reads performed by game.rs / hud.rs every frame.
    for unit in sim.alive_units() {
        let _: UnitId = unit.id;
        let _: u8 = unit.owner;
        let _: UnitKind = unit.kind;
        let _: Vec2 = unit.pos;
        let _: i32 = unit.hp;
        let _: i32 = unit.max_hp;
        let _: Order = unit.order;
        let _: bool = unit.path.is_empty();
        let _: u32 = unit.cooldown;
        let _: f32 = unit.facing;
        let _: bool = unit.is_building();
        if let Some(production) = unit.production.first() {
            let _: u32 = production.remaining;
            let _: u32 = production.kind.stats().train_ticks;
        }
        let _ = matches!(
            unit.order,
            Order::Move(_) | Order::MoveAttack(_) | Order::Attack(_) | Order::Idle | Order::Hold
        );
    }
    // HUD cost reads.
    let _ = UnitKind::Footman.stats().cost;
    let _ = UnitKind::Archer.stats().cost;
    let _ = UnitKind::Peasant.stats().cost;
    let _ = UnitKind::TownHall.stats().cost;
    // Game AI reads/writes.
    let _ = sim.resources[0];
    let _ = sim.tick;
    let _ = sim.seed;
    let _: u64 = sim.state_hash();
    for unit in sim.units.iter_mut() {
        if unit.owner == 1 {
            unit.alive = false;
        }
    }
    // Event surface with the wildcard arm the game uses.
    let events = sim.events.clone();
    for event in events {
        match event {
            Event::UnitDied(_) => {}
            Event::UnitTrained(_) => {}
            Event::Damage { to: _, .. } => {}
            Event::Victory(_) => {}
            Event::Error(_) => {}
            _ => {}
        }
    }
    // `player` and `unit` lookups used by the HUD.
    let _ = sim.player(0);
    let _ = sim.unit(hall);
}
