//! WC3 combat depth rules that sit on top of the damage table in
//! [`crate::data`].
//!
//! The shipped object data carries four combat descriptors per unit and this
//! module turns them into rules:
//!
//! * `UnitData.slk:targType` — the unit's own target class (`ground`, `air`,
//!   `structure`, `ward`).
//! * `UnitWeapons.slk:targs1`/`targs2` — the classes a weapon may hit.
//! * `UnitWeapons.slk:Farea1`/`Harea1`/`Qarea1`/`Hfact1`/`Qfact1` — splash
//!   falloff radii and factors.
//! * `UnitBalance.slk:bountyplus`/`bountydice`/`bountysides` — kill bounty.
//!
//! Everything here is integer/deterministic: the caller supplies the sim's
//! seeded roll function, never wall clock or OS entropy.
//!
//! Provenance: `analysis/war3_full/Units/{UnitData,UnitWeapons,UnitBalance}.slk`
//! (RoC 1.27 extract, access date 2026-11-15) and
//! `analysis/war3x_data/Units/*.slk` for the TFT-only neutral defs.

use crate::combat_gen::SplashProfile;
use crate::data::UnitStats;

/// Target-class bits.
///
/// The shipped vocabulary (`targsN`, `splashTargsN`, `targType`) is
/// `ground,air,structure,ward,debris,item,tree,wall,enemy`.  `enemy` is a
/// relation, not a class: it filters splash to hostile units.
pub mod tg {
    pub const GROUND: u16 = 1 << 0;
    pub const AIR: u16 = 1 << 1;
    pub const STRUCTURE: u16 = 1 << 2;
    pub const WARD: u16 = 1 << 3;
    pub const DEBRIS: u16 = 1 << 4;
    pub const ITEM: u16 = 1 << 5;
    pub const TREE: u16 = 1 << 6;
    pub const WALL: u16 = 1 << 7;
    pub const ENEMY: u16 = 1 << 8;

    /// Maps one `targsN` token.  Unknown tokens map to `0`.
    pub fn from_slk(token: &str) -> u16 {
        match token.trim().to_ascii_lowercase().as_str() {
            "ground" => GROUND,
            "air" => AIR,
            "structure" => STRUCTURE,
            "ward" => WARD,
            "debris" => DEBRIS,
            "item" => ITEM,
            "tree" => TREE,
            "wall" => WALL,
            "enemy" => ENEMY,
            _ => 0,
        }
    }

    /// Maps a comma-separated `targsN` cell (`"ground,structure,air"`).
    pub fn from_slk_list(value: &str) -> u16 {
        value
            .split(',')
            .fold(0, |mask, token| mask | from_slk(token))
    }

    /// Every class bit (the `enemy` relation excluded).
    pub const CLASSES: u16 = GROUND | AIR | STRUCTURE | WARD | DEBRIS | ITEM | TREE | WALL;
}

/// Cross-class miss chances, in permille, applied to auto-attacks.
///
/// Ground weapons firing at air and air weapons firing at ground carry the
/// small miss penalty the mission (ledger 401, 2026-11-15) specifies:
/// 5% ground-to-air, 10% air-to-ground.  Same-class attacks never miss.
///
/// The shipped `Units/MiscGame.txt` carries only `MissDamageReduction=0.5`;
/// these two values are a simulation policy constant, not an extracted table
/// value, and they are isolated here so a future gameplay-constant extract can
/// replace them without touching the damage path.
pub const MISS_CHANCE_GROUND_TO_AIR_PERMILLE: i32 = 50;
pub const MISS_CHANCE_AIR_TO_GROUND_PERMILLE: i32 = 100;

/// Miss chance for one attack pair; `0` means the attack always lands.
pub const fn miss_chance_permille(attacker_is_air: bool, target_is_air: bool) -> i32 {
    match (attacker_is_air, target_is_air) {
        (false, true) => MISS_CHANCE_GROUND_TO_AIR_PERMILLE,
        (true, false) => MISS_CHANCE_AIR_TO_GROUND_PERMILLE,
        _ => 0,
    }
}

/// `true` when `weapon_targs` covers the target's class mask.
///
/// WC3 semantics: the weapon's allowed-target mask must contain the unit's own
/// class bit; the `enemy` relation bit never satisfies a class check.
pub const fn can_target(weapon_targs: u16, target_class: u16) -> bool {
    weapon_targs & target_class & tg::CLASSES != 0
}

/// `true` when the class mask is an air unit (`UnitData.slk:targType=air`).
pub const fn is_air_class(class: u16) -> bool {
    class & tg::AIR != 0
}

/// Rolls one weapon hit: `base + dice` rolls of `1..=sides`.
///
/// WC3 stores `dmgplus1` as the flat part and `dice1`/`sides1` as the roll; the
/// per-hit minimum is `base + dice` and the maximum is `base + dice * sides`.
/// The roll order (dice first, then the caller's miss roll) is fixed so the
/// RNG stream is reproducible per seed.
pub fn roll_weapon_damage(
    base: i32,
    dice: u32,
    sides: u32,
    mut roll: impl FnMut(u32) -> u32,
) -> i32 {
    let mut damage = base;
    if sides > 0 {
        for _ in 0..dice {
            damage += roll(sides) as i32 + 1;
        }
    }
    damage.max(0)
}

/// Minimum and maximum per-hit damage for a stat block (`mindmg1`/`maxdmg1`).
pub fn damage_range(stats: &UnitStats) -> (i32, i32) {
    let dice = stats.damage_dice as i32;
    let sides = stats.damage_sides as i32;
    let min = stats.damage + dice;
    let max = stats.damage + dice * sides.max(1);
    (min.min(max), max)
}

/// WC3 splash factor in permille for a distance from the impact point:
/// 1000 inside the full-damage radius, `half_factor` inside the half radius,
/// `quarter_factor` inside the quarter radius, 0 outside.
pub fn splash_factor(splash: &SplashProfile, distance: f32) -> i32 {
    if distance <= splash.full_radius {
        1000
    } else if distance <= splash.half_radius {
        splash.half_factor
    } else if distance <= splash.quarter_radius {
        splash.quarter_factor
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_mask_parses_the_shipped_vocabulary() {
        assert_eq!(tg::from_slk_list("ground,structure,debris,item,ward"),
                   tg::GROUND | tg::STRUCTURE | tg::DEBRIS | tg::ITEM | tg::WARD);
        assert_eq!(tg::from_slk_list("enemy,ground"), tg::ENEMY | tg::GROUND);
        assert_eq!(tg::from_slk_list("_"), 0);
        assert!(can_target(tg::GROUND | tg::AIR, tg::AIR));
        assert!(!can_target(tg::GROUND, tg::AIR));
        assert!(!can_target(tg::ENEMY, tg::GROUND));
    }

    #[test]
    fn miss_policy_only_touches_cross_class_attacks() {
        assert_eq!(miss_chance_permille(false, false), 0);
        assert_eq!(miss_chance_permille(true, true), 0);
        assert_eq!(miss_chance_permille(false, true), 50);
        assert_eq!(miss_chance_permille(true, false), 100);
    }

    #[test]
    fn damage_roll_hits_both_ends_of_the_range() {
        let low = roll_weapon_damage(11, 1, 2, |_| 0);
        let high = roll_weapon_damage(11, 1, 2, |sides| sides - 1);
        assert_eq!((low, high), (12, 13));
        let stats = crate::def_of("hfoo").expect("footman").stats;
        assert_eq!(damage_range(&stats), (12, 13));
    }

}
