//! Ability framework: target kinds, mana, cooldowns, waves and buffs.

mod common;

use common::*;
use wc3_sim::{Sim, Spell, SpellTarget, Vec2};

fn caster(sim: &mut Sim, def: &str, pos: Vec2) -> wc3_sim::UnitId {
    sim.spawn_def(0, def, pos).expect("caster def")
}

#[test]
fn holy_light_heals_an_ally_and_spends_mana_with_cooldown() {
    let mut sim = Sim::new(71, 32, 32);
    let paladin = caster(&mut sim, "Hpal", Vec2::new(500.0, 500.0));
    let ally = footman(&mut sim, 0, Vec2::new(700.0, 500.0));
    sim.unit_mut(ally).unwrap().hp = 100;
    let mana_before = sim.unit(paladin).unwrap().mana;
    sim.cast(paladin, Spell::HolyLight, SpellTarget::Unit(ally))
        .expect("holy light");
    assert_eq!(sim.unit(ally).unwrap().hp, 300, "heals 200");
    assert_eq!(sim.unit(paladin).unwrap().mana, mana_before - 75);
    assert_eq!(
        sim.unit(paladin).unwrap().spell_cd[Spell::HolyLight.index()],
        150
    );
    assert_eq!(
        sim.cast(paladin, Spell::HolyLight, SpellTarget::Unit(ally)),
        Err("spell on cooldown")
    );
}

#[test]
fn holy_light_rejects_enemies_and_wrong_targets() {
    let mut sim = Sim::new(72, 32, 32);
    let paladin = caster(&mut sim, "Hpal", Vec2::new(500.0, 500.0));
    let enemy = footman(&mut sim, 1, Vec2::new(700.0, 500.0));
    assert_eq!(
        sim.cast(paladin, Spell::HolyLight, SpellTarget::Unit(enemy)),
        Err("must target an ally")
    );
    assert_eq!(
        sim.cast(
            paladin,
            Spell::HolyLight,
            SpellTarget::Point(Vec2::new(600.0, 500.0))
        ),
        Err("wrong target kind")
    );
    let outsider = sim.spawn_def(0, "hfoo", Vec2::new(1600.0, 500.0)).unwrap();
    assert_eq!(
        sim.cast(paladin, Spell::HolyLight, SpellTarget::Unit(outsider)),
        Err("out of range")
    );
}

#[test]
fn mana_regenerates_over_ticks() {
    let mut sim = Sim::new(73, 32, 32);
    let priest = caster(&mut sim, "hmpr", Vec2::new(500.0, 500.0));
    sim.unit_mut(priest).unwrap().mana = 0;
    step_n(&mut sim, 1500);
    assert!(sim.unit(priest).unwrap().mana > 0, "mana regenerates");
}

#[test]
fn blizzard_damages_every_enemy_in_the_area_over_waves() {
    let mut sim = Sim::new(74, 32, 32);
    let archmage = caster(&mut sim, "Hamg", Vec2::new(500.0, 500.0));
    let a = footman(&mut sim, 1, Vec2::new(900.0, 500.0));
    let b = footman(&mut sim, 1, Vec2::new(980.0, 560.0));
    let outside = footman(&mut sim, 1, Vec2::new(1500.0, 1500.0));
    sim.hold(&[a, b]);
    sim.cast(
        archmage,
        Spell::Blizzard,
        SpellTarget::Point(Vec2::new(940.0, 530.0)),
    )
    .expect("blizzard");
    step_n(&mut sim, 4);
    let early = sim.unit(a).unwrap().hp;
    step_n(&mut sim, 120);
    let a_hp = sim.unit(a).unwrap().hp;
    let b_hp = sim.unit(b).unwrap().hp;
    assert!(a_hp < sim.unit(a).unwrap().max_hp);
    assert!(b_hp < sim.unit(b).unwrap().max_hp);
    assert_eq!(
        sim.unit(outside).unwrap().hp,
        sim.unit(outside).unwrap().max_hp
    );
    assert!(a_hp <= early, "wave damage accumulates");
    // Six waves of typed magic damage against a medium-armour footman.
    let per_wave = wc3_sim::data::typed_damage(
        wc3_sim::AttackType::Magic,
        wc3_sim::ArmorType::Medium,
        2,
        30,
    );
    // Six Blizzard waves must land; the caster may also auto-attack after
    // channeling, so the total is at least the spell damage.
    assert!(
        sim.unit(a).unwrap().max_hp - a_hp >= per_wave * 6,
        "Blizzard waves: {} < {}",
        sim.unit(a).unwrap().max_hp - a_hp,
        per_wave * 6
    );
}

#[test]
fn entangling_roots_immobilises_for_a_while() {
    let mut sim = Sim::new(75, 32, 32);
    let keeper = caster(&mut sim, "Ekee", Vec2::new(500.0, 500.0));
    let victim = footman(&mut sim, 1, Vec2::new(900.0, 500.0));
    sim.cast(keeper, Spell::EntanglingRoots, SpellTarget::Unit(victim))
        .expect("roots");
    assert!(sim.unit(victim).unwrap().rooted_until > sim.tick);
    assert!(sim.unit(victim).unwrap().hp < sim.unit(victim).unwrap().max_hp);
    sim.step(&[wc3_sim::Command::Move {
        units: vec![victim],
        to: Vec2::new(1400.0, 500.0),
    }]);
    let start = sim.unit(victim).unwrap().pos;
    step_n(&mut sim, 60);
    assert!(
        sim.unit(victim).unwrap().pos.distance(start) < 4.0,
        "rooted unit cannot move"
    );
    step_n(&mut sim, 180);
    assert!(
        sim.unit(victim).unwrap().rooted_until <= sim.tick,
        "root expired"
    );
    step_n(&mut sim, 200);
    assert!(
        sim.unit(victim).unwrap().pos.distance(start) > 50.0,
        "moves after root expires"
    );
}

#[test]
fn chain_lightning_bounces_with_decay() {
    let mut sim = Sim::new(76, 32, 32);
    let seer = caster(&mut sim, "Ofar", Vec2::new(500.0, 500.0));
    let a = footman(&mut sim, 1, Vec2::new(900.0, 500.0));
    let b = footman(&mut sim, 1, Vec2::new(1100.0, 500.0));
    let c = footman(&mut sim, 1, Vec2::new(1300.0, 500.0));
    let events = {
        sim.cast(seer, Spell::ChainLightning, SpellTarget::Unit(a))
            .expect("chain lightning");
        sim.events.clone()
    };
    let hits: Vec<i32> = events
        .iter()
        .filter_map(|event| match event {
            wc3_sim::Event::SpellHit { amount, .. } => Some(*amount),
            _ => None,
        })
        .collect();
    assert_eq!(hits.len(), 3, "bounced to three targets");
    assert!(
        hits[0] > hits[1] && hits[1] > hits[2],
        "damage decays: {hits:?}"
    );
    let damaged = [a, b, c]
        .iter()
        .filter(|id| sim.unit(**id).map(|u| u.hp < u.max_hp).unwrap_or(false))
        .count();
    assert_eq!(damaged, 3);
}

#[test]
fn divine_shield_blocks_damage_for_its_duration() {
    let mut sim = Sim::new(77, 32, 32);
    let paladin = caster(&mut sim, "Hpal", Vec2::new(500.0, 500.0));
    let enemy = footman(&mut sim, 1, Vec2::new(650.0, 500.0));
    sim.cast(paladin, Spell::DivineShield, SpellTarget::None)
        .expect("shield");
    assert_eq!(
        sim.apply_damage(enemy, paladin, 500, wc3_sim::AttackType::Chaos),
        0
    );
    step_n(&mut sim, 300);
    assert!(
        sim.apply_damage(enemy, paladin, 10, wc3_sim::AttackType::Chaos) > 0,
        "shield expires"
    );
}

#[test]
fn spells_require_the_caster_to_know_them() {
    let mut sim = Sim::new(78, 16, 16);
    let plain = footman(&mut sim, 0, Vec2::new(200.0, 200.0));
    let ally = footman(&mut sim, 0, Vec2::new(300.0, 200.0));
    assert_eq!(
        sim.cast(plain, Spell::HolyLight, SpellTarget::Unit(ally)),
        Err("spell not available")
    );
}

#[test]
fn spell_kills_emit_unit_died() {
    let mut sim = Sim::new(79, 32, 32);
    let archmage = caster(&mut sim, "Hamg", Vec2::new(500.0, 500.0));
    let victim = footman(&mut sim, 1, Vec2::new(900.0, 500.0));
    sim.unit_mut(victim).unwrap().hp = 5;
    sim.cast(
        archmage,
        Spell::Blizzard,
        SpellTarget::Point(Vec2::new(900.0, 500.0)),
    )
    .expect("blizzard");
    let events = step_collect(&mut sim, 120);
    assert!(!sim.unit(victim).unwrap().alive);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, wc3_sim::Event::UnitDied(id) if *id == victim))
    );
}
