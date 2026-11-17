//! Damage/armor tables, acquisition, ranged combat and death handling.

mod common;

use common::*;
use wc3_sim::{ArmorType, AttackType, Sim, Vec2};

#[test]
fn roster_armor_and_attack_types_are_grounded_in_wc3() {
    let mut sim = Sim::new(51, 8, 8);
    let footman = sim.spawn_def(0, "hfoo", Vec2::new(100.0, 100.0)).unwrap();
    let rifleman = sim.spawn_def(0, "hrif", Vec2::new(200.0, 100.0)).unwrap();
    let knight = sim.spawn_def(0, "hkni", Vec2::new(300.0, 100.0)).unwrap();
    let hall = sim.spawn_def(0, "htow", Vec2::new(500.0, 300.0)).unwrap();
    assert_eq!(
        sim.unit(footman).unwrap().def().stats.armor_type,
        ArmorType::Medium
    );
    assert_eq!(
        sim.unit(rifleman).unwrap().def().stats.armor_type,
        ArmorType::Light
    );
    assert_eq!(
        sim.unit(knight).unwrap().def().stats.armor_type,
        ArmorType::Heavy
    );
    assert_eq!(
        sim.unit(hall).unwrap().def().stats.armor_type,
        ArmorType::Fortified
    );
    assert_eq!(
        sim.unit(footman).unwrap().def().stats.attack_type,
        AttackType::Normal
    );
    assert_eq!(
        sim.unit(rifleman).unwrap().def().stats.attack_type,
        AttackType::Pierce
    );
}

#[test]
fn typed_damage_applies_table_then_armor() {
    let mut sim = Sim::new(52, 16, 16);
    let attacker = footman(&mut sim, 0, Vec2::new(300.0, 300.0));
    let target = footman(&mut sim, 1, Vec2::new(340.0, 300.0));
    let before = sim.unit(target).unwrap().hp;
    // Normal vs Medium x1.5 = 150; armor 2 => 150 * 100 / 112 = 133.
    let dealt = sim.apply_damage(attacker, target, 100, AttackType::Normal);
    assert_eq!(dealt, 133);
    assert_eq!(sim.unit(target).unwrap().hp, before - 133);
}

#[test]
fn pierce_versus_light_and_fortified_differ() {
    let mut sim = Sim::new(53, 16, 16);
    let attacker = sim.spawn_def(0, "hrif", Vec2::new(100.0, 100.0)).unwrap();
    let light = sim.spawn_def(1, "hrif", Vec2::new(300.0, 300.0)).unwrap();
    let fort = sim.spawn_def(1, "htow", Vec2::new(600.0, 600.0)).unwrap();
    // Pierce vs Light x2.0, armor 0 => 100.
    assert_eq!(
        sim.apply_damage(attacker, light, 50, AttackType::Pierce),
        100
    );
    // Pierce vs Fortified x0.35, armor 5 => 17 * 100/130 = 13.
    assert_eq!(sim.apply_damage(attacker, fort, 50, AttackType::Pierce), 13);
}

#[test]
fn negative_armor_amplifies_damage() {
    let mut sim = Sim::new(54, 16, 16);
    let attacker = footman(&mut sim, 0, Vec2::new(100.0, 100.0));
    let target = footman(&mut sim, 1, Vec2::new(200.0, 100.0));
    sim.unit_mut(target).unwrap().armor = -5;
    // Normal vs Medium 1.5 = 150; negative armor -5 amplifies x1.266 => 189.
    assert_eq!(
        sim.apply_damage(attacker, target, 100, AttackType::Normal),
        189
    );
}

#[test]
fn damage_kills_and_clears_production() {
    let mut sim = Sim::new(55, 16, 16);
    let attacker = footman(&mut sim, 0, Vec2::new(300.0, 300.0));
    let hall = sim.spawn_def(1, "htow", Vec2::new(600.0, 600.0)).unwrap();
    sim.step(&[wc3_sim::Command::Train {
        building: hall,
        kind: wc3_sim::UnitKind::Peasant,
    }]);
    assert!(!sim.unit(hall).unwrap().production.is_empty());
    let events = {
        sim.apply_damage(attacker, hall, 10_000, AttackType::Chaos);
        sim.events.clone()
    };
    assert!(!sim.unit(hall).unwrap().alive);
    assert!(sim.unit(hall).unwrap().production.is_empty());
    assert!(
        events
            .iter()
            .any(|event| matches!(event, wc3_sim::Event::UnitDied(id) if *id == hall))
    );
}

#[test]
fn ranged_unit_shoots_without_closing_to_melee() {
    let mut sim = Sim::new(56, 32, 32);
    let rifleman = sim.spawn_def(0, "hrif", Vec2::new(500.0, 500.0)).unwrap();
    let enemy = footman(&mut sim, 1, Vec2::new(900.0, 500.0));
    sim.step(&[wc3_sim::Command::Attack {
        units: vec![rifleman],
        target: enemy,
    }]);
    step_n(&mut sim, 160);
    let shooter = sim.unit(rifleman).unwrap();
    let victim = sim.unit(enemy).unwrap();
    assert!(victim.hp < victim.max_hp, "target takes ranged damage");
    // The melee footman closes in; the rifleman itself stays at its post.
    assert!(
        shooter.pos.distance(Vec2::new(500.0, 500.0)) < 120.0,
        "rifleman does not charge into melee: {:?}",
        shooter.pos
    );
}

#[test]
fn attack_walks_into_melee_range_and_kills() {
    let mut sim = Sim::new(57, 24, 24);
    let attacker = footman(&mut sim, 0, Vec2::new(400.0, 400.0));
    let target = footman(&mut sim, 1, Vec2::new(1200.0, 400.0));
    sim.step(&[wc3_sim::Command::Attack {
        units: vec![attacker],
        target,
    }]);
    step_n(&mut sim, 1600);
    assert!(!sim.unit(target).unwrap().alive);
}

#[test]
fn invulnerable_target_ignores_damage() {
    let mut sim = Sim::new(58, 16, 16);
    let attacker = footman(&mut sim, 0, Vec2::new(100.0, 100.0));
    let target = footman(&mut sim, 1, Vec2::new(200.0, 100.0));
    sim.unit_mut(target).unwrap().invulnerable_until = sim.tick + 100;
    assert_eq!(
        sim.apply_damage(attacker, target, 500, AttackType::Chaos),
        0
    );
    sim.unit_mut(target).unwrap().invulnerable_until = 0;
    assert!(sim.apply_damage(attacker, target, 10, AttackType::Chaos) > 0);
}

#[test]
fn building_tower_attacks_nearby_enemies() {
    let mut sim = Sim::new(59, 24, 24);
    let tower = sim.spawn_def(0, "hgtw", Vec2::new(800.0, 800.0)).unwrap();
    let enemy = footman(&mut sim, 1, Vec2::new(1000.0, 800.0));
    assert!(sim.unit(tower).unwrap().def().stats.damage > 0);
    step_n(&mut sim, 300);
    assert!(sim.unit(enemy).unwrap().hp < sim.unit(enemy).unwrap().max_hp);
}

#[test]
fn dead_units_stop_acting() {
    let mut sim = Sim::new(60, 16, 16);
    let attacker = footman(&mut sim, 0, Vec2::new(200.0, 200.0));
    let victim = footman(&mut sim, 1, Vec2::new(400.0, 200.0));
    sim.apply_damage(attacker, victim, 10_000, AttackType::Chaos);
    let position = sim.unit(victim).unwrap().pos;
    step_n(&mut sim, 120);
    assert_eq!(sim.unit(victim).unwrap().pos, position);
    assert!(!sim.unit(victim).unwrap().alive);
}
