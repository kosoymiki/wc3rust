//! mechanic `buff.timed` / `buff.aura` / `buff.stacking` / `buff.timed-life`:
//! the ported buff system (Warsmash `CBuffTimed`, `CAbilityAuraBase`,
//! `CBuffAuraBase`, `NonStackingStatBuff`).

mod common;

use common::*;
use wc3_sim::buffs::{Buff, BuffApplied, BuffId, BuffKind, StatKey, StatMod};
use wc3_sim::{Sim, Vec2};

fn unit_at(sim: &mut Sim, owner: u8, def: &str, x: f32, y: f32) -> wc3_sim::UnitId {
    sim.spawn_def(owner, def, Vec2::new(x, y)).expect("spawn")
}

/// mechanic `buff.timed`: adds a stat buff, expiry reverts it, base stat is
/// never mutated (the effective accessor carries the change).
#[test]
fn buff_timed_applies_then_expires_without_touching_base_stats() {
    let mut sim = Sim::new(382, 32, 32);
    let knight = unit_at(&mut sim, 0, "hkni", 400.0, 400.0);
    let base_armor = sim.unit(knight).unwrap().armor;
    let mut buff = Buff::timed("BHab", knight, knight, 1, sim.tick, 60).with_armor(5);
    buff.stack_prio = 1;
    assert!(matches!(sim.add_buff(buff), BuffApplied::Added(_)));
    assert_eq!(sim.effective_armor(knight), base_armor + 5);
    assert_eq!(
        sim.unit(knight).unwrap().armor,
        base_armor,
        "buff must not rewrite the base stat"
    );
    step_n(&mut sim, 61);
    assert_eq!(sim.effective_armor(knight), base_armor);
    assert_eq!(sim.buff_count(knight), 0);
}

/// mechanic `buff.stacking`: same `stack_key` keeps only the strongest buff;
/// a stronger incoming buff replaces the weaker one.
#[test]
fn buff_stacking_keeps_the_strongest_same_key_buff() {
    let mut sim = Sim::new(383, 32, 32);
    let foot = unit_at(&mut sim, 0, "hfoo", 400.0, 400.0);
    let base = sim.unit(foot).unwrap().armor;

    let weak = Buff::timed("BHab", foot, foot, 1, sim.tick, 300)
        .with_armor(2)
        .stack_key("devotion")
        .stack_prio(1);
    assert!(matches!(sim.add_buff(weak), BuffApplied::Added(_)));

    // A second level-1 buff of the same key is rejected (not stacked).
    let duplicate = Buff::timed("BHab", foot, foot, 1, sim.tick, 300)
        .with_armor(2)
        .stack_key("devotion")
        .stack_prio(1);
    assert_eq!(sim.add_buff(duplicate), BuffApplied::Rejected);
    assert_eq!(sim.effective_armor(foot), base + 2);

    // Level 2 (stronger) replaces it.
    let strong = Buff::timed("BHab", foot, foot, 2, sim.tick, 300)
        .with_armor(4)
        .stack_key("devotion")
        .stack_prio(2);
    assert!(matches!(sim.add_buff(strong), BuffApplied::Added(_)));
    assert_eq!(sim.buff_count(foot), 1, "old buff removed");
    assert_eq!(sim.effective_armor(foot), base + 4);
}

/// mechanic `buff.aura`: the emitter applies its buff inside the radius, the
/// receiver drops it when the source dies or walks away (2 s re-check).
#[test]
fn buff_aura_applies_in_radius_and_drops_when_source_leaves() {
    let mut sim = Sim::new(384, 48, 48);
    sim.passive_income = false;
    let dk = unit_at(&mut sim, 0, "Udea", 600.0, 600.0);
    let friend = unit_at(&mut sim, 0, "ugho", 900.0, 600.0);
    let far = unit_at(&mut sim, 0, "ugho", 600.0, 2200.0);
    for unit in [dk, friend, far] {
        sim.unit_mut(unit).unwrap().learned.push("AUau");
    }
    assert!(sim.apply_data_aura(dk, "AUau", 1), "aura installed");
    let base_speed = sim.unit(friend).unwrap().speed;

    // First aura pass runs on the next tick.
    step_n(&mut sim, 2);
    assert!(sim.has_buff(friend, "BUau"), "ally inside radius buffed");
    assert!(!sim.has_buff(far, "BUau"), "ally outside radius not buffed");
    assert!(
        sim.effective_speed(friend) > base_speed,
        "unholy aura speeds the ally up"
    );

    // Source dies: the next 2 s re-check removes the buff.
    sim.kill_unit(dk);
    step_n(&mut sim, 70);
    assert!(!sim.has_buff(friend, "BUau"), "buff dropped with the source");
}

/// mechanic `buff.timed-life`: a summon with a timed life despawns on expiry.
#[test]
fn buff_timed_life_despawns_the_summon() {
    let mut sim = Sim::new(385, 32, 32);
    let summoned = unit_at(&mut sim, 1, "ogru", 700.0, 700.0);
    let mut life = Buff::timed("BTLF", summoned, summoned, 1, sim.tick, 30);
    life.kind = BuffKind::TimedLife {
        expire_tick: sim.tick + 30,
    };
    life.stack_key = None;
    assert!(matches!(sim.add_buff(life), BuffApplied::Added(_)));
    step_n(&mut sim, 20);
    assert!(sim.unit(summoned).unwrap().alive, "still alive before expiry");
    step_n(&mut sim, 15);
    assert!(!sim.unit(summoned).unwrap().alive, "despawned on expiry");
}

/// mechanic `buff.attack-speed`: attack interval and damage both resolve
/// through the buff-aware accessors.
#[test]
fn buff_attack_speed_and_damage_scale_the_real_attack() {
    let mut sim = Sim::new(386, 32, 32);
    let grunt = unit_at(&mut sim, 0, "ogru", 400.0, 400.0);
    let victim = unit_at(&mut sim, 1, "hfoo", 470.0, 400.0);
    let base_ticks = sim.unit(grunt).unwrap().attack_ticks;

    let mut haste = Buff::timed("Bhas", grunt, grunt, 1, sim.tick, 300);
    haste.attack_speed_permille = 1000; // +100 % => half the interval
    haste.damage_permille = 500; // +50 % damage
    haste.stack_key = None;
    assert!(matches!(sim.add_buff(haste), BuffApplied::Added(_)));
    assert_eq!(sim.effective_attack_ticks(grunt), (base_ticks / 2).max(1));
    let base_damage = sim.unit(grunt).unwrap().def().stats.damage;
    assert_eq!(
        sim.effective_damage(grunt, base_damage),
        (base_damage * 1500 / 1000).max(1)
    );

    // The buffed attack kills faster than the unbuffed one over the same span.
    let hp = sim.unit(victim).unwrap().hp;
    sim.issue(&[grunt], wc3_sim::OrderSpec::Attack(victim));
    step_n(&mut sim, 30);
    assert!(
        sim.unit(victim).map(|unit| unit.hp).unwrap_or(0) < hp,
        "buffed attacker dealt damage"
    );
}

/// mechanic `buff.damage-taken`: a damage-taken buff changes incoming damage.
#[test]
fn buff_damage_taken_halves_incoming_damage() {
    let mut sim = Sim::new(387, 32, 32);
    let attacker = unit_at(&mut sim, 0, "ogru", 400.0, 400.0);
    let target = unit_at(&mut sim, 1, "hfoo", 900.0, 900.0);
    let control = unit_at(&mut sim, 1, "hfoo", 1000.0, 900.0);

    let mut shell = Buff::timed("Bshl", target, target, 1, sim.tick, 300);
    // Delta relative to 1000 permille: -500 means incoming damage is halved.
    shell.damage_taken_permille = -500;
    shell.stack_key = None;
    assert!(matches!(sim.add_buff(shell), BuffApplied::Added(_)));

    let dealt_buffed = sim.apply_damage(attacker, target, 100, wc3_sim::AttackType::Normal);
    let dealt_plain = sim.apply_damage(attacker, control, 100, wc3_sim::AttackType::Normal);
    assert!(
        dealt_buffed * 2 <= dealt_plain + 1,
        "buffed target took {dealt_buffed}, plain took {dealt_plain}"
    );
}

/// mechanic `buff.invulnerable`: Ported Divine Shield buff ignores damage.
#[test]
fn buff_invulnerable_ignores_damage() {
    let mut sim = Sim::new(388, 32, 32);
    let attacker = unit_at(&mut sim, 0, "ogru", 400.0, 400.0);
    let paladin = unit_at(&mut sim, 1, "Hpal", 900.0, 900.0);
    let buff = Buff::timed("BHds", paladin, paladin, 1, sim.tick, 300).invulnerable();
    assert!(matches!(sim.add_buff(buff), BuffApplied::Added(_)));
    let hp = sim.unit(paladin).unwrap().hp;
    let dealt = sim.apply_damage(attacker, paladin, 200, wc3_sim::AttackType::Normal);
    assert_eq!(dealt, 0);
    assert_eq!(sim.unit(paladin).unwrap().hp, hp);
    // `invulnerable_until` is kept in sync for the legacy readers.
    assert!(sim.unit(paladin).unwrap().invulnerable_until > sim.tick);
}

/// Buff ids are handed out deterministically and the buff list stays sorted.
#[test]
fn buff_ids_are_deterministic_and_sorted() {
    let mut sim = Sim::new(389, 32, 32);
    let a = unit_at(&mut sim, 0, "hfoo", 400.0, 400.0);
    let b = unit_at(&mut sim, 0, "hfoo", 500.0, 400.0);
    let first = Buff::timed("Bone", a, a, 1, sim.tick, 100).stacking();
    let second = Buff::timed("Btwo", b, b, 1, sim.tick, 100).stacking();
    let BuffApplied::Added(id_a) = sim.add_buff(first) else {
        panic!("first buff");
    };
    let BuffApplied::Added(id_b) = sim.add_buff(second) else {
        panic!("second buff");
    };
    assert_eq!(id_a, BuffId(1));
    assert_eq!(id_b, BuffId(2));
    let pairs: Vec<(u32, u32)> = sim
        .buffs
        .iter()
        .map(|buff| (buff.unit.0, buff.id.0))
        .collect();
    let mut sorted = pairs.clone();
    sorted.sort();
    assert_eq!(pairs, sorted);
}

/// `StatMod` is the shape an ability table uses; every key must reach a real
/// effective accessor.
#[test]
fn buff_stat_mod_keys_reach_effective_stats() {
    let mut sim = Sim::new(390, 32, 32);
    let foot = unit_at(&mut sim, 0, "hfoo", 400.0, 400.0);
    let mods = [
        StatMod::new(StatKey::Armor, 3),
        StatMod::new(StatKey::AttackDamagePermille, 200),
        StatMod::new(StatKey::AttackSpeedPermille, 300),
        StatMod::new(StatKey::MoveSpeedPermille, 400),
        StatMod::new(StatKey::DamageTakenPermille, -200),
    ];
    let mut buff = Buff::timed("Bmod", foot, foot, 1, sim.tick, 100).stacking();
    for stat in mods {
        match stat.key {
            StatKey::Armor => buff.armor += stat.value,
            StatKey::AttackDamagePermille => buff.damage_permille += stat.value,
            StatKey::AttackSpeedPermille => buff.attack_speed_permille += stat.value,
            StatKey::MoveSpeedPermille => buff.move_speed_permille += stat.value,
            StatKey::DamageTakenPermille => buff.damage_taken_permille += stat.value,
            StatKey::MaxLife => buff.bonus_life += stat.value,
        }
    }
    assert!(matches!(sim.add_buff(buff), BuffApplied::Added(_)));
    let unit = sim.unit(foot).unwrap();
    let base_armor = unit.armor;
    let base_damage = unit.def().stats.damage;
    assert_eq!(sim.effective_armor(foot), base_armor + 3);
    assert_eq!(sim.effective_damage(foot, base_damage), base_damage * 1200 / 1000);
    assert_eq!(sim.damage_taken_permille(foot), 800);
    assert!(sim.effective_speed(foot) > unit.speed);
}
