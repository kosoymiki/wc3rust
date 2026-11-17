//! mechanic `combat.parity`: data-driven damage/armor, cooldown/backswing,
//! projectile speed, acquisition, guard/leash and collision.

use wc3_sim::{ArmorType, AttackType, Command, Event, Sim, Vec2, data, def_of};

#[test]
fn combat_parity_damage_armor_cooldown_projectile_backswing() {
    // Damage/armor table (MiscGame DamageBonus* transposed in data.rs).
    assert_eq!(
        data::typed_damage(AttackType::Pierce, ArmorType::Heavy, 2, 100),
        89
    );
    assert_eq!(
        data::typed_damage(AttackType::Normal, ArmorType::Medium, 2, 100),
        133
    );
    assert_eq!(data::armor_reduction(5, 100), 76);

    let footman = def_of("hfoo").expect("footman").stats;
    assert_eq!(footman.attack_ticks, 40, "cooldown from UnitWeapons");
    assert_eq!(footman.backswing_ticks, 15, "backSw1 * 30 from UnitWeapons");
    assert_eq!(footman.acquire_range, 500.0);
    assert_eq!(footman.radius, 15.5);
    let rifle = def_of("hrif").expect("rifleman").stats;
    assert_eq!(rifle.missile_speed, 1900.0);
    assert!(!rifle.missile_homing);

    // Projectile mechanics: launch, travel, impact damage.
    let mut sim = Sim::new(0xC0B, 64, 64);
    sim.passive_income = false;
    let shooter = sim
        .spawn_def(0, "hrif", Vec2::new(400.0, 400.0))
        .expect("rifleman");
    let target = sim
        .spawn_def(1, "hfoo", Vec2::new(700.0, 400.0))
        .expect("footman");
    sim.step(&[]);
    let before = sim.unit(target).unwrap().hp;
    sim.step(&[Command::Attack {
        units: vec![shooter],
        target,
    }]);
    let mut saw_projectile = false;
    let mut hit = false;
    for _ in 0..120 {
        sim.step(&[]);
        if let Some(projectile) = sim.projectiles.first() {
            saw_projectile = true;
            assert_eq!(projectile.speed, 1900.0);
            assert!(projectile.total > 0.0);
        }
        if sim
            .events
            .iter()
            .any(|event| matches!(event, Event::ProjectileHit { .. }))
        {
            hit = true;
        }
        if hit {
            break;
        }
    }
    assert!(saw_projectile && hit);
    assert!(sim.unit(target).unwrap().hp < before);

    // Guard/leash and collision values come from the data/camp contract.
    let mut camp_sim = Sim::new(0xC0C, 48, 48);
    camp_sim
        .spawn_camp(&["nftr", "nban"], Vec2::new(900.0, 900.0), 400.0, 600.0)
        .expect("camp");
    assert!(camp_sim.camps[0].aggro <= 500.0);
    assert!(camp_sim.camps[0].leash <= 900.0);
    let mut collision = Sim::new(0xC0D, 48, 48);
    let a = collision
        .spawn_def(0, "hfoo", Vec2::new(600.0, 600.0))
        .unwrap();
    let b = collision
        .spawn_def(0, "hfoo", Vec2::new(620.0, 600.0))
        .unwrap();
    collision.step(&[Command::Move {
        units: vec![a, b],
        to: Vec2::new(900.0, 600.0),
    }]);
    for _ in 0..120 {
        collision.step(&[]);
    }
    let pa = collision.unit(a).unwrap().pos;
    let pb = collision.unit(b).unwrap().pos;
    let radii = collision.unit(a).unwrap().radius() + collision.unit(b).unwrap().radius();
    assert!(pa.distance(pb) >= radii - 1.0, "collision separation");
}
