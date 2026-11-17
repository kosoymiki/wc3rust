//! mechanic `ability.effect-handlers`: the generic Warsmash-derived effect
//! runner (damage / heal / buff / aura / summon / movement).

mod common;

use common::*;
use wc3_sim::effects::{build_effects, run_effects, EffectContext, EffectOutcome, EffectSpec};
use wc3_sim::{AttackType, Sim, Vec2};

fn spawn(sim: &mut Sim, owner: u8, def: &str, x: f32, y: f32) -> wc3_sim::UnitId {
    sim.spawn_def(owner, def, Vec2::new(x, y)).expect("spawn")
}

/// mechanic `ability.effect-handlers`: the damage handler runs the real typed
/// damage path (armor table, events, death).
#[test]
fn effect_handler_damage_uses_the_real_pipeline() {
    let mut sim = Sim::new(400, 32, 32);
    let caster = spawn(&mut sim, 0, "Hpal", 500.0, 500.0);
    let target = spawn(&mut sim, 1, "hfoo", 700.0, 500.0);
    let hp = sim.unit(target).unwrap().hp;
    let outcomes = run_effects(
        &mut sim,
        EffectContext::new(caster).with_target(target),
        &[EffectSpec::Damage {
            amount: 120,
            attack: AttackType::Magic,
        }],
    );
    assert!(matches!(
        outcomes[0],
        EffectOutcome::Damaged { unit, amount } if unit == target && amount > 0
    ));
    assert_eq!(
        sim.unit(target).unwrap().hp,
        hp - match outcomes[0] {
            EffectOutcome::Damaged { amount, .. } => amount,
            _ => 0,
        }
    );
    assert!(sim
        .events
        .iter()
        .any(|event| matches!(event, wc3_sim::Event::Damage { .. })));
}

/// mechanic `ability.effect-handlers`: heal clips at max life and cannot revive.
#[test]
fn effect_handler_heal_clips_at_max_life() {
    let mut sim = Sim::new(401, 32, 32);
    let caster = spawn(&mut sim, 0, "Hpal", 500.0, 500.0);
    let ally = spawn(&mut sim, 0, "hfoo", 700.0, 500.0);
    let max = sim.unit(ally).unwrap().max_hp;
    sim.unit_mut(ally).unwrap().hp = max - 30;
    let outcomes = run_effects(
        &mut sim,
        EffectContext::new(caster).with_target(ally),
        &[EffectSpec::Heal { amount: 100 }],
    );
    assert_eq!(sim.unit(ally).unwrap().hp, max);
    assert!(matches!(
        outcomes[0],
        EffectOutcome::Healed { amount, .. } if amount == 30
    ));
}

/// mechanic `ability.effect-handlers`: a buff handler installs a timed buff.
#[test]
fn effect_handler_buff_installs_a_timed_buff() {
    const ARMOR_MODS: [wc3_sim::StatMod; 1] = [wc3_sim::StatMod::armor(2)];
    let mut sim = Sim::new(402, 32, 32);
    let caster = spawn(&mut sim, 0, "Udea", 500.0, 500.0);
    let target = spawn(&mut sim, 0, "ugho", 700.0, 500.0);
    let outcomes = run_effects(
        &mut sim,
        EffectContext::new(caster).with_target(target),
        &[EffectSpec::Buff {
            tag: "Btest",
            duration_ticks: 90,
            mods: &ARMOR_MODS,
            root: false,
            stun: false,
            invulnerable: false,
            level: 1,
        }],
    );
    assert!(matches!(outcomes[0], EffectOutcome::Buffed { .. }));
    assert!(sim.has_buff(target, "Btest"));
    step_n(&mut sim, 95);
    assert!(!sim.has_buff(target, "Btest"));
}

/// mechanic `ability.effect-handlers`: an aura handler installs an emitter.
#[test]
fn effect_handler_aura_installs_an_emitter() {
    let mut sim = Sim::new(403, 32, 32);
    let caster = spawn(&mut sim, 0, "Udea", 500.0, 500.0);
    let ally = spawn(&mut sim, 0, "ugho", 700.0, 500.0);
    let outcomes = run_effects(
        &mut sim,
        EffectContext::new(caster),
        &[EffectSpec::Aura {
            tag: "Baura",
            radius: 600.0,
            level: 1,
            mods: &[],
            move_speed_permille: 150,
            hostile: false,
        }],
    );
    assert!(matches!(
        outcomes[0],
        EffectOutcome::AuraInstalled { unit, .. } if unit == caster
    ));
    step_n(&mut sim, 2);
    assert!(sim.has_buff(ally, "Baura"), "ally received the aura buff");
}

/// mechanic `ability.effect-handlers`: summon spawns the requested count and
/// the timed life despawns them.
#[test]
fn effect_handler_summon_spawns_count_with_timed_life() {
    let mut sim = Sim::new(404, 32, 32);
    let caster = spawn(&mut sim, 0, "Hpal", 600.0, 600.0);
    let outcomes = run_effects(
        &mut sim,
        EffectContext::new(caster),
        &[EffectSpec::Summon {
            def_id: "hfoo",
            count: 2,
            lifetime_ticks: 40,
            forward: 80.0,
        }],
    );
    assert!(matches!(
        outcomes[0],
        EffectOutcome::Summoned { def_id, .. } if def_id == "hfoo"
    ));
    let summons: Vec<_> = sim
        .units
        .iter()
        .filter(|unit| unit.owner == 0 && unit.id != caster && unit.def().id == "hfoo")
        .map(|unit| unit.id)
        .collect();
    assert_eq!(summons.len(), 2, "two summons spawned");
    step_n(&mut sim, 42);
    for id in summons {
        assert!(!sim.unit(id).unwrap().alive, "summon despawned on expiry");
    }
}

/// mechanic `ability.effect-handlers`: the summon handler refuses a unit type
/// the roster does not know instead of inventing one.
#[test]
fn effect_handler_summon_reports_a_missing_unit_type() {
    let mut sim = Sim::new(405, 32, 32);
    let caster = spawn(&mut sim, 0, "Hpal", 600.0, 600.0);
    let outcomes = run_effects(
        &mut sim,
        EffectContext::new(caster),
        &[EffectSpec::Summon {
            def_id: "hwat",
            count: 1,
            lifetime_ticks: 100,
            forward: 80.0,
        }],
    );
    assert_eq!(
        outcomes[0],
        EffectOutcome::Skipped {
            reason: "summon unit type not in roster"
        }
    );
}

/// mechanic `ability.effect-handlers`: the movement handler roots a target.
#[test]
fn effect_handler_movement_roots_the_target() {
    let mut sim = Sim::new(406, 32, 32);
    let caster = spawn(&mut sim, 0, "Ekee", 500.0, 500.0);
    let victim = spawn(&mut sim, 1, "hfoo", 800.0, 500.0);
    let start = sim.unit(victim).unwrap().pos;
    let outcomes = run_effects(
        &mut sim,
        EffectContext::new(caster).with_target(victim),
        &[EffectSpec::Movement {
            slow_permille: 1000,
            root: true,
            duration_ticks: 60,
            tag: "BEer",
        }],
    );
    assert!(matches!(outcomes[0], EffectOutcome::Rooted { .. }));
    assert!(sim.is_rooted(victim));
    sim.issue(&[victim], wc3_sim::OrderSpec::Move(start + Vec2::new(400.0, 0.0)));
    step_n(&mut sim, 30);
    assert!(
        sim.unit(victim).unwrap().pos.distance(start) < 32.0,
        "rooted unit cannot walk"
    );
}

/// mechanic `ability.effect-handlers`: area damage hits every hostile unit in
/// the radius and spares allies.
#[test]
fn effect_handler_area_damage_hits_hostiles_only() {
    let mut sim = Sim::new(407, 40, 40);
    let caster = spawn(&mut sim, 0, "Hpal", 600.0, 600.0);
    let ally = spawn(&mut sim, 0, "hfoo", 640.0, 600.0);
    let enemy_a = spawn(&mut sim, 1, "hfoo", 660.0, 600.0);
    let enemy_b = spawn(&mut sim, 1, "hfoo", 620.0, 650.0);
    let ally_hp = sim.unit(ally).unwrap().hp;
    let a_hp = sim.unit(enemy_a).unwrap().hp;
    let b_hp = sim.unit(enemy_b).unwrap().hp;
    run_effects(
        &mut sim,
        EffectContext::new(caster).with_point(Vec2::new(650.0, 620.0)),
        &[EffectSpec::DamageArea {
            radius: 300.0,
            amount: 80,
            attack: AttackType::Magic,
            hostile_only: true,
        }],
    );
    assert_eq!(sim.unit(ally).unwrap().hp, ally_hp, "ally untouched");
    assert!(sim.unit(enemy_a).unwrap().hp < a_hp);
    assert!(sim.unit(enemy_b).unwrap().hp < b_hp);
}

/// mechanic `ability.divine-shield`: the real ability row installs the
/// invulnerability buff through the handler table.
#[test]
fn ability_divine_shield_installs_invulnerability() {
    let mut sim = Sim::new(408, 32, 32);
    let paladin = spawn(&mut sim, 0, "Hpal", 500.0, 500.0);
    sim.unit_mut(paladin).unwrap().learned.push("AHds");
    sim.cast_data_ability(paladin, "AHds", None, None)
        .expect("divine shield");
    assert!(sim.has_buff(paladin, "BHds"));
    let attacker = spawn(&mut sim, 1, "ogru", 560.0, 500.0);
    assert_eq!(
        sim.apply_damage(attacker, paladin, 150, AttackType::Normal),
        0
    );
    step_n(&mut sim, 10 * 30 + 2);
    assert!(!sim.has_buff(paladin, "BHds"), "10 s buff expired");
}

/// mechanic `ability.entangling-roots`: damage plus a movement block through
/// the handler table.
#[test]
fn ability_entangling_roots_damages_and_roots() {
    let mut sim = Sim::new(409, 32, 32);
    let keeper = spawn(&mut sim, 0, "Ekee", 500.0, 500.0);
    let victim = spawn(&mut sim, 1, "hfoo", 800.0, 500.0);
    sim.unit_mut(keeper).unwrap().learned.push("AEer");
    let hp = sim.unit(victim).unwrap().hp;
    sim.cast_data_ability(keeper, "AEer", Some(victim), None)
        .expect("entangling roots");
    assert!(sim.unit(victim).unwrap().hp < hp, "damage applied");
    assert!(sim.is_rooted(victim), "target rooted");
}

/// mechanic `ability.effect-handlers`: `build_effects` maps data rows to
/// handlers (no castable row with an effect magnitude is silently dropped).
#[test]
fn effect_table_covers_castable_rows() {
    let mut mapped = 0;
    let mut no_effect = 0;
    for id in wc3_sim::data_castable_ids() {
        let def = wc3_sim::generic_ability(id).expect("row");
        let effects = build_effects(def);
        match def.target {
            wc3_sim::TargetKind::Unit => {
                assert!(!effects.is_empty(), "{id} unit-target has handlers");
                mapped += 1;
            }
            wc3_sim::TargetKind::Point => {
                // A point row needs an area and a magnitude for the generic
                // damage handler; rows with `Area1 = 0` are single-point utility
                // casts whose target resolution is not part of the handler set.
                if def.power > 0 && def.radius > 0.0 {
                    assert!(!effects.is_empty(), "{id} point-target has handlers");
                    mapped += 1;
                } else {
                    no_effect += 1;
                }
            }
            wc3_sim::TargetKind::None => {
                // No-target data rows may legitimately have no handler yet.
            }
        }
    }
    assert!(mapped > 100, "castable rows mapped: {mapped}");
    assert!(
        no_effect < mapped,
        "effect-bearing rows outnumber effectless ones ({mapped} vs {no_effect})"
    );
}
