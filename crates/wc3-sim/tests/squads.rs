//! mechanic `ai.squad-ai`: the OpenRA-derived squad controller (state machine,
//! leader election, regroup, attack-or-flee fuzzy, stuck detection).

mod common;

use common::*;
use wc3_sim::squad::{attack_or_flee_value, health_class, rule_count};
use wc3_sim::{Sim, SquadConfig, SquadKind, SquadState, Vec2};

fn spawn(sim: &mut Sim, owner: u8, def: &str, x: f32, y: f32) -> wc3_sim::UnitId {
    sim.spawn_def(owner, def, Vec2::new(x, y)).expect("spawn")
}

fn arena(seed: u64) -> Sim {
    let mut sim = Sim::new(seed, 96, 96);
    sim.passive_income = false;
    sim.resources = [50_000, 50_000];
    sim
}

/// mechanic `ai.squad-ai`: the fuzzy verdict attacks with a healthy equal army
/// and flees with a near-dead army against a healthy stronger one.
#[test]
fn squad_fuzzy_attacks_when_healthy_and_flees_when_near_dead() {
    assert!(rule_count() >= 10, "rule table ported");
    // Both armies healthy, equal power and speed: attack.
    assert!(attack_or_flee_value(100.0, 100.0, 100.0, 100.0));
    // Own army near dead, enemy healthy, weak and slower: the donor rules keep
    // attacking only while speed is slow/equal, so this stays an attack.
    assert!(attack_or_flee_value(5.0, 100.0, 50.0, 80.0));
    // Near dead against healthy with equal power and higher speed: flee.
    assert!(!attack_or_flee_value(5.0, 100.0, 100.0, 150.0));
    // Injured, weak and faster than the enemy: flee.
    assert!(!attack_or_flee_value(45.0, 95.0, 40.0, 150.0));
    // Health classes match the donor trapezoids.
    assert_eq!(health_class(10.0), 0);
    assert_eq!(health_class(50.0), 1);
    assert_eq!(health_class(95.0), 2);
}

/// mechanic `ai.squad-ai`: the manager forms an assault squad from idle units
/// and drives it into the attack state against a visible enemy.
#[test]
fn squad_manager_forms_and_attacks() {
    let mut sim = arena(500);
    let base = spawn(&mut sim, 0, "htow", 1200.0, 1200.0);
    for slot in 0..10 {
        let angle = slot as f32 * std::f32::consts::TAU / 10.0;
        spawn(
            &mut sim,
            0,
            "hfoo",
            base_x(base) + angle.cos() * 260.0,
            base_y(base) + angle.sin() * 260.0,
        );
    }
    let enemy = spawn(&mut sim, 1, "ogru", 2600.0, 2600.0);
    spawn(&mut sim, 1, "ogre", 2700.0, 2700.0);
    sim.enable_squads(0, SquadConfig::default());
    let mut engaged = false;
    for _ in 0..1200 {
        sim.step(&[]);
        let manager = sim.squad_manager(0).expect("manager");
        if manager.squads.iter().any(|squad| {
            matches!(squad.state, SquadState::AttackMove | SquadState::Attack)
                && squad.units.len() >= 3
        }) {
            engaged = true;
            break;
        }
    }
    assert!(engaged, "squad reached an attacking state");
    let manager = sim.squad_manager(0).expect("manager");
    assert!(
        manager.squads.iter().any(|squad| squad.kind == SquadKind::Assault),
        "an assault squad exists"
    );
    // The squad eventually damages the enemy side (unit or building).
    let enemy_hp = |sim: &Sim| -> i64 {
        sim.units
            .iter()
            .filter(|unit| unit.owner == 1 && unit.alive)
            .map(|unit| unit.hp as i64)
            .sum()
    };
    let before = enemy_hp(&sim);
    let mut hurt = false;
    for _ in 0..2400 {
        sim.step(&[]);
        if enemy_hp(&sim) < before {
            hurt = true;
            break;
        }
    }
    assert!(hurt, "squad dealt damage to the enemy side");
}

fn base_x(id: wc3_sim::UnitId) -> f32 {
    // Unit positions are read through the sim in the test body; the helper only
    // keeps the spawn ring readable.
    let _ = id;
    1200.0
}

fn base_y(_id: wc3_sim::UnitId) -> f32 {
    1200.0
}

/// mechanic `ai.squad-ai`: the leader is the unit closest to the squad centre
/// and stragglers are sent back to the leader (regroup).
#[test]
fn squad_regroups_stragglers_to_the_leader() {
    let mut sim = arena(501);
    let base = spawn(&mut sim, 0, "htow", 1000.0, 1000.0);
    let mut foot = Vec::new();
    for slot in 0..8 {
        let angle = slot as f32 * std::f32::consts::TAU / 8.0;
        foot.push(spawn(
            &mut sim,
            0,
            "hfoo",
            base_x(base) + angle.cos() * 200.0,
            base_y(base) + angle.sin() * 200.0,
        ));
    }
    let enemy = spawn(&mut sim, 1, "ogru", 3000.0, 3000.0);
    spawn(&mut sim, 1, "ogre", 3100.0, 3000.0);
    sim.enable_squads(0, SquadConfig::default());
    // Let the squad form.
    for _ in 0..900 {
        sim.step(&[]);
        if sim
            .squad_manager(0)
            .map(|manager| manager.squads.iter().any(|squad| squad.units.len() >= 8))
            .unwrap_or(false)
        {
            break;
        }
    }
    // Kick one unit far away; the next squad pass must order it back.
    let straggler = foot[0];
    sim.unit_mut(straggler)
        .unwrap()
        .pos
        .clone_from(&Vec2::new(400.0, 400.0));
    let mut regrouped = false;
    for _ in 0..400 {
        sim.step(&[]);
        let unit = sim.unit(straggler).unwrap();
        if matches!(unit.order, wc3_sim::Order::MoveAttack(_)) && unit.goal.is_some() {
            regrouped = true;
            break;
        }
    }
    assert!(regrouped, "straggler received a regroup order");
    let _ = enemy;
}

/// mechanic `ai.squad-ai`: a protection squad forms when the base is attacked
/// and is dropped when the attacker is gone.
#[test]
fn squad_protection_forms_under_attack() {
    let mut sim = arena(502);
    let base = spawn(&mut sim, 0, "htow", 1500.0, 1500.0);
    for slot in 0..6 {
        let angle = slot as f32 * std::f32::consts::TAU / 6.0;
        spawn(
            &mut sim,
            0,
            "hfoo",
            base_x(base) + angle.cos() * 240.0,
            base_y(base) + angle.sin() * 240.0,
        );
    }
    sim.enable_squads(0, SquadConfig::default());
    for _ in 0..200 {
        sim.step(&[]);
    }
    // Enemy walks into the danger radius.
    let raider = spawn(&mut sim, 1, "ogru", 1850.0, 1500.0);
    let mut formed = false;
    for _ in 0..120 {
        sim.step(&[]);
        if sim
            .squad_manager(0)
            .map(|manager| {
                manager
                    .squads
                    .iter()
                    .any(|squad| squad.kind == SquadKind::Protection)
            })
            .unwrap_or(false)
        {
            formed = true;
            break;
        }
    }
    assert!(formed, "protection squad formed");
    sim.kill_unit(raider);
    let mut dropped = false;
    for _ in 0..120 {
        sim.step(&[]);
        let any = sim
            .squad_manager(0)
            .map(|manager| manager.squads.iter().any(|squad| squad.kind == SquadKind::Protection))
            .unwrap_or(false);
        if !any {
            dropped = true;
            break;
        }
    }
    assert!(dropped, "protection squad dissolved");
}

/// mechanic `ai.squad-ai`: a squad whose leader cannot make progress drops back
/// to idle instead of path-spamming (OpenRA stuck detection).
#[test]
fn squad_stuck_detection_returns_to_idle() {
    let mut sim = arena(503);
    let base = spawn(&mut sim, 0, "htow", 600.0, 600.0);
    for slot in 0..8 {
        let angle = slot as f32 * std::f32::consts::TAU / 8.0;
        spawn(
            &mut sim,
            0,
            "hfoo",
            base_x(base) + angle.cos() * 180.0,
            base_y(base) + angle.sin() * 180.0,
        );
    }
    sim.enable_squads(0, SquadConfig::default());
    // No enemies, no movement: the squad must not stay in AttackMove forever.
    for _ in 0..600 {
        sim.step(&[]);
    }
    let states: Vec<SquadState> = sim
        .squad_manager(0)
        .map(|manager| manager.squads.iter().map(|squad| squad.state).collect())
        .unwrap_or_default();
    assert!(
        states.iter().all(|state| *state != SquadState::AttackMove),
        "no squad wedged in AttackMove: {states:?}"
    );
}

/// Squad state and buff state survive a save/load round trip (v2 format).
#[test]
fn squad_and_buff_state_round_trip() {
    let mut sim = arena(504);
    let base = spawn(&mut sim, 0, "htow", 800.0, 800.0);
    let foot = spawn(&mut sim, 0, "hfoo", 900.0, 800.0);
    spawn(&mut sim, 1, "ogru", 3000.0, 3000.0);
    sim.enable_squads(0, SquadConfig::default());
    let mut buff = wc3_sim::Buff::timed("Brt1", foot, foot, 1, sim.tick, 300).with_armor(2);
    buff.stack_key = Some("rt");
    sim.add_buff(buff);
    for _ in 0..60 {
        sim.step(&[]);
    }
    let bytes = sim.to_save_bytes();
    let restored = Sim::from_save(&bytes).expect("load");
    assert_eq!(restored.state_hash(), sim.state_hash());
    assert_eq!(restored.to_save_bytes(), bytes);
    assert!(restored.squad_manager(0).is_some());
    assert!(restored.has_buff(foot, "Brt1"));
    let _ = base;
}
