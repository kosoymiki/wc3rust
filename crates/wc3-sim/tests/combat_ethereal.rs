//! Combat gaps `combat.spells-bypass-armor`, `combat.ethereal-damage` and
//! `combat.ethereal-heal`.
//!
//! Engine rule (observed, `analysis/agent-re/sim/ENGINE_MAP.md` §10 and
//! `PROBES.md` §2-3, ledger 404/412):
//!
//! * the armor block at `0x6f19c5f8..0x6f19c628` is gated on the damage
//!   object's physical flag (`*(damage + 0x14) == 0x10`);
//! * the 7x8 attack/armor table (`0x6f19c64a`) and the ethereal row
//!   (`0x6f19c677`) sit outside that gate;
//! * no branch compares the attack-type id (`+0x20`) with 0, and engine id 0 is
//!   `spells` — so a spells hit keeps the table and the ethereal multiplier and
//!   skips armor;
//! * ethereal is a per-target counter (`+0x158 > 0`) that multiplies incoming
//!   damage by `MiscGame.txt:107 EtherealDamageBonus=1.66` and incoming healing
//!   by `MiscGame.txt:108 EtherealHealBonus=1.66`.

use wc3_sim::data::{self, ETHEREAL_DAMAGE_MULT_PERMILLE, ETHEREAL_HEAL_PERMILLE};
use wc3_sim::{Ability, ArmorType, AttackType, Command, Sim, UnitId, Vec2};

/// Medium-armour target (grunt) with a pinned 10 armor rating, and a footman
/// attacker far enough away that no auto-acquisition fires during a tick.
fn staged_fight(seed: u64) -> (Sim, UnitId, UnitId) {
    let mut sim = Sim::new(seed, 16, 16);
    let attacker = sim
        .spawn_def_exact(0, "hfoo", Vec2::new(200.0, 200.0))
        .expect("footman");
    let target = sim
        .spawn_def_exact(1, "ogru", Vec2::new(1400.0, 1400.0))
        .expect("grunt");
    sim.unit_mut(target).unwrap().armor = 10;
    assert_eq!(
        sim.unit(target).unwrap().def().stats.armor_type,
        ArmorType::Medium,
        "the staged target must be Medium armour"
    );
    (sim, attacker, target)
}

// ----------------------------------------------------------------------
// combat.spells-bypass-armor
// ----------------------------------------------------------------------

#[test]
fn combat_spells_bypass_armor() {
    let (mut sim, attacker, target) = staged_fight(0x5E11);

    // Table rows (permille): Normal/Medium 1500, Magic/Medium 750,
    // Spells/Medium 1000.  Armor 10 => x100/160.
    assert_eq!(
        data::DAMAGE_MULT_PERMILLE[AttackType::Normal.index()][ArmorType::Medium.index()],
        1500
    );
    assert_eq!(
        data::DAMAGE_MULT_PERMILLE[AttackType::Spells.index()][ArmorType::Medium.index()],
        1000
    );

    // Pipeline function: table yes, armor no for spells.
    assert_eq!(
        data::typed_damage(AttackType::Normal, ArmorType::Medium, 10, 100),
        93, // 150 * 100 / 160 = 93.75 -> 93
        "a normal hit is reduced by 10 armor"
    );
    assert_eq!(
        data::typed_damage(AttackType::Spells, ArmorType::Medium, 10, 100),
        100,
        "a spells hit keeps the full table value and ignores armor"
    );
    assert_eq!(
        data::typed_damage(AttackType::Magic, ArmorType::Medium, 10, 100),
        46, // 75 * 100 / 160 = 46.875 -> 46
        "magic is a weapon class in WC3 and stays armored"
    );
    assert!(data::skips_armor(AttackType::Spells));
    for attack in [
        AttackType::Normal,
        AttackType::Pierce,
        AttackType::Siege,
        AttackType::Magic,
        AttackType::Chaos,
        AttackType::Hero,
    ] {
        assert!(!data::skips_armor(attack), "{attack:?} must keep armor");
    }

    // Same numbers through the simulation damage path.
    let before = sim.unit(target).unwrap().hp;
    assert_eq!(
        sim.apply_damage(attacker, target, 100, AttackType::Normal),
        93
    );
    assert_eq!(
        sim.apply_damage(attacker, target, 100, AttackType::Spells),
        100
    );
    assert_eq!(
        sim.apply_damage(attacker, target, 100, AttackType::Magic),
        46
    );
    assert_eq!(sim.unit(target).unwrap().hp, before - 93 - 100 - 46);

    // Armor is skipped entirely, so negative armour cannot amplify a spell.
    sim.unit_mut(target).unwrap().armor = -5;
    assert_eq!(
        sim.apply_damage(attacker, target, 100, AttackType::Spells),
        100,
        "negative armor does not touch spells either"
    );
    assert_eq!(
        sim.apply_damage(attacker, target, 100, AttackType::Normal),
        189, // 150 * 1.266 = 189.9 -> 189
        "the same negative armor still amplifies a physical hit"
    );
}

// ----------------------------------------------------------------------
// combat.ethereal-damage
// ----------------------------------------------------------------------

#[test]
fn combat_ethereal_damage() {
    assert_eq!(ETHEREAL_DAMAGE_MULT_PERMILLE, [1660; AttackType::COUNT]);
    let (mut sim, attacker, target) = staged_fight(0xE7E1);
    assert!(
        !sim.unit(target).unwrap().ethereal,
        "units spawn non-ethereal"
    );

    // Non-ethereal baselines on the 10-armor target.
    assert_eq!(
        sim.apply_damage(attacker, target, 100, AttackType::Normal),
        93
    );
    assert_eq!(
        sim.apply_damage(attacker, target, 100, AttackType::Spells),
        100
    );

    // Flip the engine `+0x158` counter through the public setter.
    assert!(sim.set_ethereal(target, true));
    assert!(sim.unit(target).unwrap().ethereal);

    // Table -> armor -> ethereal, integer-permille:
    // normal: 150 -> (10 armor) 93 -> *1.66 = 154.38 -> 154
    assert_eq!(
        sim.apply_damage(attacker, target, 100, AttackType::Normal),
        154,
        "an ethereal target takes x1.66 from a normal hit"
    );
    // spells: 100 -> (no armor) 100 -> *1.66 = 166
    assert_eq!(
        sim.apply_damage(attacker, target, 100, AttackType::Spells),
        166,
        "an ethereal target takes x1.66 from a spells hit"
    );
    // chaos: 100 -> (10 armor) 62 -> *1.66 = 102.92 -> 102
    assert_eq!(
        sim.apply_damage(attacker, target, 100, AttackType::Chaos),
        102
    );

    // The pipeline function agrees with the sim path.
    assert_eq!(
        data::typed_damage_vs(AttackType::Normal, ArmorType::Medium, 10, 100, true),
        154
    );
    assert_eq!(
        data::typed_damage_vs(AttackType::Spells, ArmorType::Medium, 10, 100, true),
        166
    );
    assert_eq!(
        data::typed_damage_vs(AttackType::Normal, ArmorType::Medium, 10, 100, false),
        93
    );

    // Turning the counter off restores the plain numbers (no residual state).
    // (The staged grunt has taken 615 damage, so top it up first.)
    sim.unit_mut(target).unwrap().hp = sim.unit(target).unwrap().max_hp;
    assert!(sim.set_ethereal(target, false));
    assert_eq!(
        sim.apply_damage(attacker, target, 100, AttackType::Spells),
        100
    );
    assert_eq!(
        sim.apply_damage(attacker, target, 100, AttackType::Normal),
        93
    );

    // Unknown ids are reported, not silently accepted.
    assert!(!sim.set_ethereal(wc3_sim::UnitId(9999), true));
}

// ----------------------------------------------------------------------
// combat.ethereal-heal
// ----------------------------------------------------------------------

#[test]
fn combat_ethereal_heal() {
    assert_eq!(ETHEREAL_HEAL_PERMILLE, 1660);
    let (mut sim, caster, target) = staged_fight(0x4EA1);

    sim.unit_mut(target).unwrap().hp = 100;
    assert_eq!(
        sim.heal_unit(target, 100),
        100,
        "a non-ethereal target heals the plain amount"
    );
    assert_eq!(sim.unit(target).unwrap().hp, 200);

    assert!(sim.set_ethereal(target, true));
    sim.unit_mut(target).unwrap().hp = 100;
    assert_eq!(
        sim.heal_unit(target, 100),
        166,
        "an ethereal target heals x1.66"
    );
    assert_eq!(sim.unit(target).unwrap().hp, 266);

    // The same rule through the spell command path (`Ability::Heal`).
    // Two identical sims, digest of one tick: the only difference is the
    // ethereal counter, so any per-tick regeneration cancels out.
    let heal_spell = |ethereal: bool| {
        let (mut sim, caster, target) = staged_fight(0x4EA2);
        sim.unit_mut(target).unwrap().hp = 100;
        if ethereal {
            assert!(sim.set_ethereal(target, true));
        }
        sim.step(&[Command::Cast {
            unit: caster,
            ability: Ability::Heal(100),
            target: Some(target),
        }]);
        sim.unit(target).unwrap().hp
    };
    let plain = heal_spell(false);
    let ethereal = heal_spell(true);
    assert_eq!(plain, 200, "non-ethereal heal spell: 100 + 100");
    assert_eq!(ethereal, 266, "ethereal heal spell: 100 + 166");
    assert_eq!(ethereal - plain, 66);

    // A heal that would overflow clamps at `max_hp` and reports the real gain.
    let max_hp = sim.unit(target).unwrap().max_hp;
    sim.unit_mut(target).unwrap().hp = max_hp - 10;
    assert_eq!(
        sim.heal_unit(target, 100),
        10,
        "ethereal overflow clamps at max_hp"
    );
    assert_eq!(sim.unit(target).unwrap().hp, max_hp);
    let _ = caster;
}
