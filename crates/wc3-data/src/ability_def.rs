//! Ability numbers straight from `AbilityData.slk`, for simulation/UI code.
//!
//! [`AbilityView`](crate::abilities::AbilityView) exposes the raw columns with
//! their absent/`-` semantics. This module turns one ability row into a
//! *planner-friendly* record: per-level mana, cooldown, area, range, duration
//! and the `Data` slots as numbers, plus the aura summary the buff framework
//! needs.
//!
//! Auras are identified from the data alone (no hardcoded id list):
//! passive cost (`Cost` absent), zero cooldown, a positive radius and a
//! friendly/self target list — Devotion Aura `AHab` and Endurance Aura `AOae`
//! both match, while Blizzard `AHbz` (cost 75, cooldown 6) does not.

use crate::abilities::AbilityView;
use crate::error::Result;
use crate::GameDatabase;

/// One ability level with the numbers a simulator consumes.
#[derive(Debug, Clone, PartialEq)]
pub struct SpellLevel {
    pub level: u32,
    pub mana: Option<i32>,
    /// `Cool` in seconds.
    pub cooldown: Option<f32>,
    /// `Area` in world units.
    pub area: Option<f32>,
    /// `Rng` in world units.
    pub range: Option<f32>,
    /// `Dur` in seconds.
    pub duration: Option<f32>,
    /// `HeroDur` in seconds.
    pub hero_duration: Option<f32>,
    /// `Cast` as stored.
    pub cast: Option<f32>,
    /// `DataA`..`DataE` (TFT) or `Data1`..`Data4` (RoC) as numbers.
    pub data: Vec<Option<f64>>,
    /// `UnitID` — the unit the ability spawns, when used.
    pub unit_id: Option<String>,
}

impl SpellLevel {
    /// `data[index]` with the 1-based slot number the editor shows.
    pub fn slot(&self, slot: u32) -> Option<f64> {
        self.data
            .get(slot.saturating_sub(1) as usize)
            .copied()
            .flatten()
    }
}

/// Aura summary derived from the level numbers.
#[derive(Debug, Clone, PartialEq)]
pub struct AuraNumbers {
    /// `Area1` — aura radius.
    pub radius: f32,
    /// `Data` slots of level 1 (the armour/damage modifier at level 1).
    pub bonuses: Vec<f64>,
    /// `Data` slots per level (index 0 = level 1), so the framework can scale an
    /// aura by the ability level exactly as the sheet does.
    pub bonuses_by_level: Vec<Vec<f64>>,
    /// `targs` — who the aura affects.
    pub targets: Vec<String>,
}

impl AuraNumbers {
    /// Bonus of `slot` (1-based) at `level` (1-based).
    pub fn bonus(&self, level: u32, slot: u32) -> Option<f64> {
        self.bonuses_by_level
            .get(level.saturating_sub(1) as usize)?
            .get(slot.saturating_sub(1) as usize)
            .copied()
    }
}

/// One ability row as numbers (no behaviour).
#[derive(Debug, Clone, PartialEq)]
pub struct AbilityNumbers {
    pub id: String,
    /// `code` — the base ability the row inherits from.
    pub code: String,
    pub name: Option<String>,
    pub icon: Option<String>,
    pub hotkey: Option<String>,
    /// `targs` flags (`air`, `ground`, `friend`, `enemy`, …).
    pub target_flags: Vec<String>,
    pub hero: bool,
    pub item: bool,
    pub use_in_editor: bool,
    pub levels: Vec<SpellLevel>,
    /// Present when the row looks like an aura (see the module docs).
    pub aura: Option<AuraNumbers>,
}

impl AbilityNumbers {
    /// Level-1 numbers, the shape most simulations need.
    pub fn first_level(&self) -> Option<&SpellLevel> {
        self.levels.first()
    }

    /// True when the ability is passive support (aura-like).
    pub fn is_aura(&self) -> bool {
        self.aura.is_some()
    }

    /// Target flags that are *unit categories* (not sides).
    pub fn unit_categories(&self) -> Vec<&str> {
        self.target_flags
            .iter()
            .map(String::as_str)
            .filter(|flag| {
                !matches!(
                    *flag,
                    "friend" | "enemy" | "neutral" | "self" | "notself" | "vuln" | "invu"
                )
            })
            .collect()
    }
}

impl<'a> AbilityView<'a> {
    /// Data-derived aura detection (see the module docs).
    pub fn is_aura(&self) -> Result<bool> {
        Ok(self.aura_numbers()?.is_some())
    }

    /// Aura summary for a passive support ability.
    pub fn aura_numbers(&self) -> Result<Option<AuraNumbers>> {
        let Some(level) = self.level(1)? else {
            return Ok(None);
        };
        let allies = self.targets_allies();
        let passive = level.mana_cost.is_none()
            && level.cooldown.unwrap_or(0.0) == 0.0
            && level.duration.unwrap_or(0.0) == 0.0;
        let radius = level.area.unwrap_or(0.0);
        if !(allies && passive && radius > 0.0) {
            return Ok(None);
        }
        let mut bonuses_by_level: Vec<Vec<f64>> = Vec::new();
        for parsed in self.levels()? {
            let level_bonuses: Vec<f64> = parsed
                .data
                .iter()
                .filter_map(|slot| slot.as_deref())
                .filter_map(|text| text.trim().parse::<f64>().ok())
                .collect();
            bonuses_by_level.push(level_bonuses);
        }
        Ok(Some(AuraNumbers {
            radius,
            bonuses: level
                .data
                .iter()
                .filter_map(|slot| slot.as_deref())
                .filter_map(|text| text.trim().parse::<f64>().ok())
                .collect(),
            bonuses_by_level,
            targets: self.target_flags().iter().map(|flag| flag.to_string()).collect(),
        }))
    }

    /// Numeric record for this ability (levels, costs, cooldown, area, range).
    pub fn numbers(&self) -> Result<AbilityNumbers> {
        let mut levels = Vec::new();
        for level in self.levels()? {
            let data = level
                .data
                .iter()
                .map(|slot| slot.as_deref().and_then(|text| text.trim().parse::<f64>().ok()))
                .collect();
            levels.push(SpellLevel {
                level: level.level,
                mana: level.mana_cost,
                cooldown: level.cooldown,
                area: level.area,
                range: level.range,
                duration: level.duration,
                hero_duration: level.hero_duration,
                cast: level.cast,
                data,
                unit_id: level.unit_id,
            });
        }
        let aura = self.aura_numbers()?;
        Ok(AbilityNumbers {
            id: self.id_str(),
            code: self.code().unwrap_or_default().to_string(),
            name: self.name(),
            icon: self.icon(),
            hotkey: self.hotkey(),
            target_flags: self.target_flags().iter().map(|flag| flag.to_string()).collect(),
            hero: self.is_hero_ability()?,
            item: self.is_item_ability()?,
            use_in_editor: self.use_in_editor()?,
            levels,
            aura,
        })
    }
}

impl GameDatabase {
    /// Numeric record for one ability id.
    pub fn ability_numbers(&self, id: &str) -> Option<AbilityNumbers> {
        self.ability(id)?.numbers().ok()
    }

    /// Every ability the data defines with at least one level, sorted by id.
    ///
    /// This is the coverage set for `qa.abilities.coverage`: an ability row is
    /// "covered" when its numbers can be generated from the sheet (no behaviour
    /// assumed).
    pub fn ability_numbers_all(&self) -> Vec<AbilityNumbers> {
        let mut out: Vec<AbilityNumbers> = self
            .abilities()
            .filter_map(|ability| ability.numbers().ok())
            .filter(|numbers| !numbers.levels.is_empty())
            .collect();
        out.sort_by(|left, right| left.id.cmp(&right.id));
        out
    }

    /// Auras in the data (subsystem inventory for the buff framework).
    pub fn auras(&self) -> Vec<AbilityNumbers> {
        let mut out: Vec<AbilityNumbers> = self
            .ability_numbers_all()
            .into_iter()
            .filter(|numbers| numbers.aura.is_some())
            .collect();
        out.sort_by(|left, right| left.id.cmp(&right.id));
        out
    }

    /// Races whose units form the melee tech trees.
    pub const MELEE_RACES: [&'static str; 4] = ["human", "orc", "undead", "nightelf"];

    /// Ability ids referenced by melee-race units (`abilList` + `heroAbilList`).
    ///
    /// This is the denominator of `qa.abilities.coverage` in
    /// `analysis/tech-lead/PARITY_MATRIX.md` §7 row 10 (matrix baseline: 140
    /// melee ids).
    pub fn melee_ability_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for unit in self.units() {
            let Some(race) = unit.race() else { continue };
            if !Self::MELEE_RACES
                .iter()
                .any(|melee| race.eq_ignore_ascii_case(melee))
            {
                continue;
            }
            for ability in unit.abilities().into_iter().chain(unit.hero_abilities()) {
                let id = ability.trim();
                if id.is_empty() || id == "_" || id == "-" {
                    continue;
                }
                if !ids.iter().any(|known| known == id) {
                    ids.push(id.to_string());
                }
            }
        }
        ids.sort();
        ids
    }

    /// `(generated, total)` over the melee ids that `AbilityData` defines with
    /// at least one level — the `qa.abilities.coverage` numerator/denominator.
    pub fn melee_ability_coverage(&self) -> (usize, usize) {
        let ids = self.melee_ability_ids();
        let total = ids
            .iter()
            .filter(|id| {
                self.ability_numbers(id)
                    .is_some_and(|numbers| !numbers.levels.is_empty())
            })
            .count();
        (total, ids.len())
    }

    /// Ratio of ability rows with generated numbers to all editor abilities.
    pub fn ability_coverage(&self) -> (usize, usize) {
        let total = self
            .abilities()
            .filter(|ability| ability.use_in_editor().unwrap_or(false))
            .count();
        let covered = self
            .ability_numbers_all()
            .iter()
            .filter(|numbers| numbers.use_in_editor)
            .count();
        (covered, total)
    }

    /// Ability ids referenced by any unit (`abilList` + `heroAbilList`) that
    /// `AbilityData` defines with at least one level, sorted.
    ///
    /// This is the data population behind the QA `cast.coverage` gate (matrix
    /// G13): every id here has sheet numbers a simulator can cast from.
    pub fn castable_ability_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for unit in self.units() {
            for ability in unit.abilities().into_iter().chain(unit.hero_abilities()) {
                let id = ability.trim();
                if id.is_empty() || id == "_" || id == "-" {
                    continue;
                }
                if !ids.iter().any(|known| known == id) {
                    ids.push(id.to_string());
                }
            }
        }
        ids.sort();
        ids
    }

    /// `(with sheet numbers, referenced ids)` over [`castable_ability_ids`].
    ///
    /// [`castable_ability_ids`]: GameDatabase::castable_ability_ids
    pub fn castable_ability_coverage(&self) -> (usize, usize) {
        let ids = self.castable_ability_ids();
        let covered = ids
            .iter()
            .filter(|id| {
                self.ability_numbers(id)
                    .is_some_and(|numbers| !numbers.levels.is_empty())
            })
            .count();
        (covered, ids.len())
    }
}

/// One hero ability as hero UI/sim code consumes it: which heroes teach it, the
/// learn requirements and the per-level numbers from `AbilityData.slk`.
///
/// The catalog is data-derived: ids come from the heroes' `heroAbilList`
/// (`UnitAbilities.slk`), `levels`/`reqLevel`/`levelSkip` from the ability row,
/// and every learnable level is the same [`SpellLevel`] the spell generator uses,
/// so mana/cooldown/range/area are the sheet's values, never constants.
#[derive(Debug, Clone, PartialEq)]
pub struct HeroAbilityInfo {
    pub id: String,
    pub name: Option<String>,
    pub icon: Option<String>,
    pub hotkey: Option<String>,
    /// Hero unit ids whose `heroAbilList` references this ability.
    pub heroes: Vec<String>,
    /// `levels` — learnable levels.
    pub levels: u32,
    /// `reqLevel` — hero level required for the first skill level.
    pub required_level: Option<i32>,
    /// `levelSkip` — hero levels per skill level (TFT column).
    pub level_skip: Option<i32>,
    pub numbers: AbilityNumbers,
}

impl HeroAbilityInfo {
    /// Numbers of learnable level `level` (1-based).
    pub fn level(&self, level: u32) -> Option<&SpellLevel> {
        self.numbers
            .levels
            .get(level.saturating_sub(1) as usize)
    }

    /// True when the sheet defines numbers for every learnable level.
    pub fn covers_every_level(&self) -> bool {
        self.levels > 0 && self.numbers.levels.len() as u32 == self.levels
    }
}

impl GameDatabase {
    /// Units that are heroes: they carry a primary attribute (`UnitBalance.Primary`).
    ///
    /// This is the same predicate [`GameDatabase::hero`] uses, so the catalog and
    /// the hero record cannot drift apart.
    pub fn hero_unit_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .units()
            .filter(|unit| {
                unit.primary_attribute()
                    .and_then(crate::hero::PrimaryAttribute::parse)
                    .is_some()
            })
            .map(|unit| unit.id_str())
            .collect();
        ids.sort();
        ids
    }

    /// Distinct ability ids referenced by any hero's `heroAbilList`, sorted.
    pub fn hero_ability_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for unit in self.units() {
            if unit.primary_attribute().is_none() {
                continue;
            }
            for ability in unit.hero_abilities() {
                let id = ability.trim();
                if id.is_empty() || id == "_" || id == "-" {
                    continue;
                }
                if !ids.iter().any(|known| known == id) {
                    ids.push(id.to_string());
                }
            }
        }
        ids.sort();
        ids
    }

    /// Every hero ability with its owner heroes, learn gate and per-level numbers.
    pub fn hero_abilities(&self) -> Vec<HeroAbilityInfo> {
        // Owners first: one pass over the units, then one record per ability id.
        let mut owners: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for unit in self.units() {
            if unit.primary_attribute().is_none() {
                continue;
            }
            let hero = unit.id_str();
            for ability in unit.hero_abilities() {
                let id = ability.trim();
                if id.is_empty() || id == "_" || id == "-" {
                    continue;
                }
                owners
                    .entry(id.to_string())
                    .or_default()
                    .push(hero.clone());
            }
        }
        let mut out: Vec<HeroAbilityInfo> = Vec::new();
        for (id, heroes) in owners {
            let Some(ability) = self.ability(&id) else {
                continue;
            };
            let Ok(numbers) = ability.numbers() else {
                continue;
            };
            out.push(HeroAbilityInfo {
                id: id.clone(),
                name: ability.name(),
                icon: ability.icon(),
                hotkey: ability.hotkey(),
                heroes,
                levels: ability.max_level(),
                required_level: ability.required_level().ok().flatten(),
                level_skip: ability.level_skip().ok().flatten(),
                numbers,
            });
        }
        out
    }

    /// `(with numbers for every level, total hero abilities)` — the
    /// `qa.abilities.heroes` gate (matrix row 11).
    pub fn hero_ability_coverage(&self) -> (usize, usize) {
        let catalog = self.hero_abilities();
        let total = catalog.len();
        let covered = catalog
            .iter()
            .filter(|ability| ability.covers_every_level())
            .count();
        (covered, total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spell_level_slot_is_one_based() {
        let level = SpellLevel {
            level: 1,
            mana: Some(75),
            cooldown: Some(6.0),
            area: Some(200.0),
            range: Some(800.0),
            duration: Some(1.0),
            hero_duration: Some(1.0),
            cast: Some(0.0),
            data: vec![Some(6.0), Some(30.0), None],
            unit_id: None,
        };
        assert_eq!(level.slot(1), Some(6.0));
        assert_eq!(level.slot(2), Some(30.0));
        assert_eq!(level.slot(3), None);
        assert_eq!(level.slot(9), None);
    }
}
