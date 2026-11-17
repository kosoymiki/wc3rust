//! Hero data: attributes, level, skills, XP curve and revive costs.
//!
//! Everything comes from the shipped tables:
//! - attributes and per-level growth: `UnitBalance.slk` (`STR/INT/AGI`,
//!   `STRplus/INTplus/AGIplus`, `Primary`);
//! - hero level and skills: `UnitBalance.level` and the `heroAbilList` column of
//!   `UnitAbilities.slk`, each skill's level data from `AbilityData.slk`;
//! - XP curve and revive factors: `Units\MiscData.txt` `[Misc]`
//!   (`MaxHeroLevel`, `NeedHeroXP`, `GrantHeroXP`, `HeroExpRange`,
//!   `Revive*Factor`, `HeroMaxRevive*`), including the formula the file itself
//!   documents:
//!   `goldRevivalCost = originalCost * (ReviveBaseFactor + ReviveLevelFactor*(level-1))`
//!   capped by `ReviveMaxFactor`/`HeroMaxReviveCostGold`, and
//!   `revivalTime = originalTime * level * ReviveTimeFactor`.

use crate::error::Result;
use crate::GameDatabase;

/// Attribute block of a hero plus the derived-stat constants read from the
/// same row (`realHP`/`HP`/STR and `realM`/`manaN`/INT) and the gameplay table
/// (`DefenseArmor`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeroAttributes {
    pub strength: i32,
    pub intelligence: i32,
    pub agility: i32,
    pub strength_per_level: f32,
    pub intelligence_per_level: f32,
    pub agility_per_level: f32,
    /// `Primary` — `STR`/`INT`/`AGI`.
    pub primary: Option<PrimaryAttribute>,
    /// Base hit points (`HP`) before the strength contribution.
    pub base_hp: i32,
    /// Hit points per point of strength, derived from the row
    /// (`(realHP - HP) / STR`, 25 for every retail hero).
    pub hp_per_strength: f32,
    /// Base mana (`manaN`) before the intelligence contribution.
    pub base_mana: i32,
    /// Mana per point of intelligence (`(realM - manaN) / INT`, 15 retail).
    pub mana_per_intelligence: f32,
    /// Base armour (`def`) before the agility contribution.
    pub base_armor: i32,
    /// Armour per point of agility (`Units\MiscData.txt DefenseArmor`).
    pub armor_per_agility: f32,
}

/// The hero's main attribute (`UnitBalance.Primary`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimaryAttribute {
    Strength,
    Intelligence,
    Agility,
}

impl PrimaryAttribute {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_uppercase().as_str() {
            "STR" | "STRENGTH" => Some(Self::Strength),
            "INT" | "INTELLIGENCE" => Some(Self::Intelligence),
            "AGI" | "AGILITY" => Some(Self::Agility),
            _ => None,
        }
    }

    /// Attribute value for a hero at `level` (base + growth × (level-1)).
    pub fn value_at(&self, attributes: &HeroAttributes, level: u32) -> f32 {
        let steps = level.saturating_sub(1) as f32;
        match self {
            Self::Strength => attributes.strength as f32 + attributes.strength_per_level * steps,
            Self::Intelligence => {
                attributes.intelligence as f32 + attributes.intelligence_per_level * steps
            }
            Self::Agility => attributes.agility as f32 + attributes.agility_per_level * steps,
        }
    }
}

/// One learnable hero skill (`heroAbilList` + `AbilityData`).
#[derive(Debug, Clone, PartialEq)]
pub struct HeroSkill {
    pub id: String,
    pub name: Option<String>,
    pub icon: Option<String>,
    pub hotkey: Option<String>,
    /// `levels` — number of learnable levels.
    pub levels: u32,
    /// `reqLevel` — hero level required for the first level.
    pub required_level: Option<i32>,
    /// `levelSkip` — hero levels per skill level.
    pub level_skip: Option<i32>,
    /// Level-1 mana/cooldown/area/range as numbers.
    pub mana: Option<i32>,
    pub cooldown: Option<f32>,
    pub area: Option<f32>,
    pub range: Option<f32>,
}

/// Hero rules from `Units\MiscData.txt [Misc]`.
#[derive(Debug, Clone, PartialEq)]
pub struct HeroRules {
    pub max_level: i64,
    /// `NeedHeroXP` — XP needed to reach level 2, 3, …
    pub need_xp: Vec<i64>,
    /// `GrantHeroXP` — XP granted for killing a hero of level 1, 2, …
    pub grant_xp: Vec<i64>,
    pub exp_range: f32,
    pub revive_base_factor: f32,
    pub revive_level_factor: f32,
    pub revive_max_factor: f32,
    pub revive_base_lumber_factor: f32,
    pub revive_lumber_level_factor: f32,
    pub revive_time_factor: f32,
    pub revive_max_time_factor: f32,
    pub max_revive_gold: i64,
    pub max_revive_lumber: i64,
    pub max_revive_time: f32,
    /// `DefenseArmor` — armour gained per point of agility.
    pub armor_per_agility: f32,
}

/// Skill points granted per hero level. WC3's `Game.dll` constant; no SLK/TXT
/// in the retail installs carries it, so it is stated here once, explicitly.
pub const SKILL_POINTS_PER_LEVEL: u32 = 1;

/// The twelve ladder heroes of Reign of Chaos (four per race), by unit id.
///
/// `Otch` is the Tauren Chieftain's ladder id (`Othr` is the campaign row with
/// an INT primary attribute), so the list matches the melee roster the QA
/// `hero.coverage` gate counts.
pub const LADDER_HERO_IDS: [&'static str; 12] = [
    "Hpal", "Hamg", "Hmkg", "Ofar", "Obla", "Otch", "Udea", "Ulic", "Udre", "Ekee", "Emoo", "Edem",
];

/// Revival cost/time of one hero at one level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReviveCost {
    pub gold: i64,
    pub lumber: i64,
    pub seconds: f32,
}

impl HeroRules {
    /// XP required to go from `level` to `level + 1`.
    pub fn xp_to_next(&self, level: u32) -> Option<i64> {
        self.need_xp.get(level.saturating_sub(1) as usize).copied()
    }

    /// XP a hero grants when killed at `level`.
    pub fn kill_xp(&self, level: u32) -> Option<i64> {
        self.grant_xp.get(level.saturating_sub(1) as usize).copied()
    }

    /// Revival cost per the formula documented in `Units\MiscData.txt`.
    pub fn revive_cost(
        &self,
        gold_cost: i64,
        build_time: i64,
        level: u32,
    ) -> ReviveCost {
        let steps = level.saturating_sub(1) as f32;
        let gold_factor = (self.revive_base_factor + self.revive_level_factor * steps)
            .min(self.revive_max_factor);
        let lumber_factor = (self.revive_base_lumber_factor
            + self.revive_lumber_level_factor * steps)
            .min(self.revive_max_factor);
        let time_factor = (level as f32 * self.revive_time_factor).min(self.revive_max_time_factor);
        ReviveCost {
            gold: ((gold_cost as f32 * gold_factor) as i64).min(self.max_revive_gold),
            lumber: ((gold_cost as f32 * lumber_factor) as i64).min(self.max_revive_lumber),
            seconds: (build_time as f32 * time_factor).min(self.max_revive_time),
        }
    }
}

/// A hero definition (unit with `Primary` set and a `heroAbilList`).
#[derive(Debug, Clone, PartialEq)]
pub struct HeroDef {
    pub id: String,
    pub name: Option<String>,
    pub level: i32,
    pub attributes: HeroAttributes,
    pub skills: Vec<HeroSkill>,
}

impl GameDatabase {
    /// Hero rules from `Units\MiscData.txt`.
    pub fn hero_rules(&self) -> Option<HeroRules> {
        let file = "Units\\MiscData.txt";
        let read_f32 = |key: &str, default: f32| -> f32 {
            self.misc()
                .get(file, "Misc", key)
                .and_then(|value| value.trim().parse::<f32>().ok())
                .unwrap_or(default)
        };
        let read_i64 = |key: &str, default: i64| -> i64 {
            self.misc()
                .get(file, "Misc", key)
                .and_then(|value| value.trim().parse::<i64>().ok())
                .unwrap_or(default)
        };
        let list_i64 = |key: &str| -> Vec<i64> {
            self.misc()
                .get(file, "Misc", key)
                .map(|value| {
                    value
                        .split(',')
                        .filter_map(|part| part.trim().parse::<i64>().ok())
                        .collect()
                })
                .unwrap_or_default()
        };
        let need_xp = list_i64("NeedHeroXP");
        if need_xp.is_empty() {
            return None;
        }
        Some(HeroRules {
            max_level: read_i64("MaxHeroLevel", 10),
            need_xp,
            grant_xp: list_i64("GrantHeroXP"),
            exp_range: read_f32("HeroExpRange", 1200.0),
            revive_base_factor: read_f32("ReviveBaseFactor", 0.4),
            revive_level_factor: read_f32("ReviveLevelFactor", 0.1),
            revive_max_factor: read_f32("ReviveMaxFactor", 4.0),
            revive_base_lumber_factor: read_f32("ReviveBaseLumberFactor", 0.0),
            revive_lumber_level_factor: read_f32("ReviveLumberLevelFactor", 0.0),
            revive_time_factor: read_f32("ReviveTimeFactor", 0.5),
            revive_max_time_factor: read_f32("ReviveMaxTimeFactor", 2.0),
            max_revive_gold: read_i64("HeroMaxReviveCostGold", 700),
            max_revive_lumber: read_i64("HeroMaxReviveCostLumber", 0),
            max_revive_time: read_f32("HeroMaxReviveTime", 150.0),
            armor_per_agility: read_f32("DefenseArmor", 0.06),
        })
    }

    /// Hero definition for `id`, `None` when the unit is not a hero.
    pub fn hero(&self, id: &str) -> Option<HeroDef> {
        let unit = self.unit(id)?;
        let primary_text = unit.primary_attribute()?;
        let primary = PrimaryAttribute::parse(primary_text)?;
        let strength = unit.strength().ok().flatten().unwrap_or(0);
        let intelligence = unit.intelligence().ok().flatten().unwrap_or(0);
        let agility = unit.agility().ok().flatten().unwrap_or(0);
        let base_hp = unit.hp().ok().flatten().unwrap_or(0);
        let base_mana = unit.mana().ok().flatten().unwrap_or(0);
        let real_hp = unit.real_hp().ok().flatten().unwrap_or(base_hp);
        let real_mana = unit.real_mana().ok().flatten().unwrap_or(base_mana);
        // The per-attribute constants are derived from the row itself, never
        // authored. The sheet's own computed columns state the formula in their
        // Excel remnant: `realHP = 650;ERC[-1]+RC[+15]*25` (HP + STR*25) and
        // `realM = 255;EIF(...)*15` (manaN + INT*15), so both the multiplier and
        // the identity `realHP = HP + STR*k` are data, not a convention.
        let hp_per_strength = if strength > 0 {
            (real_hp - base_hp) as f32 / strength as f32
        } else {
            0.0
        };
        let mana_per_intelligence = if intelligence > 0 {
            (real_mana - base_mana) as f32 / intelligence as f32
        } else {
            0.0
        };
        let armor_per_agility = self
            .hero_rules()
            .map(|rules| rules.armor_per_agility)
            .unwrap_or(0.06);
        let attributes = HeroAttributes {
            strength,
            intelligence,
            agility,
            strength_per_level: unit.strength_per_level().ok().flatten().unwrap_or(0.0),
            intelligence_per_level: unit.intelligence_per_level().ok().flatten().unwrap_or(0.0),
            agility_per_level: unit.agility_per_level().ok().flatten().unwrap_or(0.0),
            primary: Some(primary),
            base_hp,
            hp_per_strength,
            base_mana,
            mana_per_intelligence,
            base_armor: unit.armor_base().ok().flatten().unwrap_or(0),
            armor_per_agility,
        };
        let mut skills = Vec::new();
        for skill_id in unit.hero_abilities() {
            let Some(ability) = self.ability(skill_id) else {
                continue;
            };
            let level = ability.level(1).ok().flatten();
            skills.push(HeroSkill {
                id: ability.id_str(),
                name: ability.name(),
                icon: ability.icon(),
                hotkey: ability.hotkey(),
                levels: ability.max_level(),
                required_level: ability.required_level().ok().flatten(),
                level_skip: ability.level_skip().ok().flatten(),
                mana: level.as_ref().and_then(|level| level.mana_cost),
                cooldown: level.as_ref().and_then(|level| level.cooldown),
                area: level.as_ref().and_then(|level| level.area),
                range: level.as_ref().and_then(|level| level.range),
            });
        }
        Some(HeroDef {
            id: unit.id_str(),
            name: unit.name(),
            level: unit.level().ok().flatten().unwrap_or(1),
            attributes,
            skills,
        })
    }

    /// Every hero defined by the data, sorted by id.
    pub fn heroes(&self) -> Vec<HeroDef> {
        let mut out: Vec<HeroDef> = self
            .units()
            .filter_map(|unit| self.hero(&unit.id_str()))
            .collect();
        out.sort_by(|left, right| left.id.cmp(&right.id));
        out
    }

    /// The twelve ladder heroes ([`LADDER_HERO_IDS`]) present in this install,
    /// sorted by id.
    pub fn melee_heroes(&self) -> Vec<HeroDef> {
        let mut out: Vec<HeroDef> = LADDER_HERO_IDS
            .iter()
            .filter_map(|id| self.hero(id))
            .collect();
        out.sort_by(|left, right| left.id.cmp(&right.id));
        out
    }

    /// `(present, total)` over the twelve ladder heroes — the data side of the
    /// QA `hero.coverage` gate (matrix C4/G15).
    pub fn melee_hero_coverage(&self) -> (usize, usize) {
        let present = LADDER_HERO_IDS
            .iter()
            .filter(|id| self.hero(id).is_some())
            .count();
        (present, LADDER_HERO_IDS.len())
    }

    /// Revival cost of a hero at `level` (falls back to the sim-free defaults
    /// when the misc table is missing).
    pub fn hero_revive_cost(&self, id: &str, level: u32) -> Option<ReviveCost> {
        let unit = self.unit(id)?;
        let rules = self.hero_rules()?;
        let gold = unit.gold_cost().ok().flatten().unwrap_or(0) as i64;
        let build_time = unit.build_time().ok().flatten().unwrap_or(0) as i64;
        Some(rules.revive_cost(gold, build_time, level))
    }
}

impl HeroDef {
    /// Hit points at `level`: `base_hp + STR(level) * hp_per_strength`.
    pub fn hp_at(&self, level: u32) -> i32 {
        let strength = self
            .attributes
            .value_at(PrimaryAttribute::Strength, level);
        (self.attributes.base_hp as f32 + strength * self.attributes.hp_per_strength).round() as i32
    }

    /// Mana at `level`: `base_mana + INT(level) * mana_per_intelligence`.
    pub fn mana_at(&self, level: u32) -> i32 {
        let intelligence = self
            .attributes
            .value_at(PrimaryAttribute::Intelligence, level);
        (self.attributes.base_mana as f32 + intelligence * self.attributes.mana_per_intelligence)
            .round() as i32
    }

    /// Armour at `level`: `base_armor + AGI(level) * armor_per_agility`.
    pub fn armor_at(&self, level: u32) -> f32 {
        let agility = self.attributes.value_at(PrimaryAttribute::Agility, level);
        self.attributes.base_armor as f32 + agility * self.attributes.armor_per_agility
    }

    /// Attack damage bonus at `level` from the primary attribute (+1 per point).
    pub fn damage_bonus_at(&self, level: u32) -> f32 {
        self.attributes.attack_bonus_at(level)
    }

    /// Skill points earned by `level` (one per level-up, WC3's rule).
    pub fn skill_points_at(&self, level: u32) -> u32 {
        level.saturating_sub(1) * SKILL_POINTS_PER_LEVEL
    }

    /// Skills learnable at `level` — `required_level` gate of the first level.
    pub fn skills_at_level(&self, level: u32) -> Vec<&HeroSkill> {
        self.skills
            .iter()
            .filter(|skill| skill.required_level.unwrap_or(1).max(1) as u32 <= level)
            .collect()
    }
}

impl HeroAttributes {
    /// Attribute value at `level` for the selected attribute.
    pub fn value_at(&self, attribute: PrimaryAttribute, level: u32) -> f32 {
        attribute.value_at(self, level)
    }

    /// Attack damage bonus from the primary attribute (WC3: +1 per point).
    pub fn attack_bonus_at(&self, level: u32) -> f32 {
        match self.primary {
            Some(primary) => primary.value_at(self, level),
            None => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> HeroRules {
        HeroRules {
            max_level: 10,
            need_xp: vec![200, 500, 900],
            grant_xp: vec![80, 100, 120],
            exp_range: 1200.0,
            revive_base_factor: 0.4,
            revive_level_factor: 0.1,
            revive_max_factor: 4.0,
            revive_base_lumber_factor: 0.0,
            revive_lumber_level_factor: 0.0,
            revive_time_factor: 0.5,
            revive_max_time_factor: 2.0,
            max_revive_gold: 700,
            max_revive_lumber: 0,
            max_revive_time: 150.0,
            armor_per_agility: 0.06,
        }
    }

    #[test]
    fn xp_curve_and_revive_formula() {
        let rules = rules();
        assert_eq!(rules.xp_to_next(1), Some(200));
        assert_eq!(rules.xp_to_next(3), Some(900));
        assert_eq!(rules.xp_to_next(4), None);
        assert_eq!(rules.kill_xp(2), Some(100));
        // 425G hero at level 1: 40% -> 170, time 60s * 1 * 0.5 = 30 s.
        let cost = rules.revive_cost(425, 60, 1);
        assert_eq!(cost.gold, 170);
        assert_eq!(cost.lumber, 0);
        assert!((cost.seconds - 30.0).abs() < 0.001);
        // Level 4: 40% + 3*10% = 70% -> 297; time 60 * 4 * 0.5 = 120 s.
        let cost = rules.revive_cost(425, 60, 4);
        assert_eq!(cost.gold, 297);
        assert!((cost.seconds - 120.0).abs() < 0.001);
        // Cap: 700 gold.
        let capped = rules.revive_cost(5000, 60, 1);
        assert_eq!(capped.gold, 700);
    }

    #[test]
    fn attribute_growth_is_linear() {
        let attributes = HeroAttributes {
            strength: 22,
            intelligence: 17,
            agility: 13,
            strength_per_level: 2.7,
            intelligence_per_level: 1.8,
            agility_per_level: 1.5,
            primary: Some(PrimaryAttribute::Strength),
            base_hp: 100,
            hp_per_strength: 25.0,
            base_mana: 0,
            mana_per_intelligence: 15.0,
            base_armor: 2,
            armor_per_agility: 0.06,
        };
        assert!((attributes.value_at(PrimaryAttribute::Strength, 1) - 22.0).abs() < 0.001);
        assert!((attributes.value_at(PrimaryAttribute::Strength, 5) - 32.8).abs() < 0.001);
        assert!((attributes.attack_bonus_at(5) - 32.8).abs() < 0.001);
    }
}
