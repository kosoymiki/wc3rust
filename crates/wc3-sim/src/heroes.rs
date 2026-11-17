//! Hero roster menus: altar / tavern training lists, revive candidates and the
//! LEARN submenu rows.
//!
//! The roster itself is generated (`roster_gen.rs`) from the real WC3 tables;
//! this module only turns the generated `trained_at` / `requires` links into
//! the lists a hero card asks for, so the card never hard-codes hero ids.

use crate::abilities::{self, Spell};
use crate::data::{self, Race, UnitDef};
use crate::{Sim, UnitId};

/// Every hero definition in the roster (melee + tavern).
pub fn all_heroes() -> impl Iterator<Item = &'static UnitDef> {
    data::ROSTER.iter().filter(|def| def.hero)
}

/// Heroes a building produces (`Altar of Kings`, `Tavern`, ...).
pub fn heroes_of_building(building: &str) -> Vec<&'static UnitDef> {
    data::ROSTER
        .iter()
        .filter(|def| def.hero && def.trained_at.contains(&building))
        .collect()
}

/// The melee hero line of one race (four entries for Human/Orc/Undead/Night
/// Elf; empty for Neutral).
pub fn race_heroes(race: Race) -> Vec<&'static UnitDef> {
    data::ROSTER
        .iter()
        .filter(|def| def.hero && def.race == race)
        .collect()
}

/// Tavern roster: neutral heroes sold at `ntav`.
pub fn tavern_heroes() -> Vec<&'static UnitDef> {
    heroes_of_building("ntav")
}

/// One LEARN-submenu row: the id, the resolved spell/variant and the real
/// `reqLevel` gate from `AbilityData.slk`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeroAbilityRow {
    pub id: &'static str,
    pub name: &'static str,
    pub spell: Spell,
    pub req_level: u8,
    pub learned: bool,
}

impl Sim {
    /// Heroes this owner can start training from a living, completed altar or
    /// tavern it owns.  Deduplicated and roster-ordered so the card is stable.
    pub fn trainable_heroes(&self, owner: u8) -> Vec<&'static UnitDef> {
        let mut out: Vec<&'static UnitDef> = Vec::new();
        for unit in &self.units {
            if unit.owner != owner || !unit.alive || unit.construct.is_some() {
                continue;
            }
            let building = unit.def();
            if !building.is_building() {
                continue;
            }
            for def in data::ROSTER.iter() {
                if def.hero
                    && def.trained_at.contains(&building.id)
                    && !out.iter().any(|entry| entry.id == def.id)
                {
                    out.push(def);
                }
            }
        }
        out
    }

    /// Card contents for one altar/tavern: the exact hero list that building
    /// trains (four for a race altar, the tavern roster for `ntav`).
    pub fn altar_menu(&self, altar: UnitId) -> Vec<&'static UnitDef> {
        let Some(unit) = self.unit(altar) else {
            return Vec::new();
        };
        if !unit.alive || unit.construct.is_some() {
            return Vec::new();
        }
        heroes_of_building(unit.def().id)
    }

    /// Dead heroes this altar may revive: same owner and a hero whose altar is
    /// this building (tavern heroes revive at the tavern in this data set).
    pub fn revive_candidates(&self, altar: UnitId) -> Vec<UnitId> {
        let Some(altar_unit) = self.unit(altar) else {
            return Vec::new();
        };
        if !altar_unit.alive || altar_unit.construct.is_some() {
            return Vec::new();
        }
        let altar_def = altar_unit.def().id;
        let owner = altar_unit.owner;
        self.units
            .iter()
            .filter(|unit| {
                unit.owner == owner
                    && !unit.alive
                    && unit.def().hero
                    && unit.def().trained_at.contains(&altar_def)
            })
            .map(|unit| unit.id)
            .collect()
    }

    /// LEARN submenu: every ability on the hero's real list with its resolved
    /// spell and the level gate; `learned` marks the ranks already bought.
    pub fn hero_ability_menu(&self, hero: UnitId) -> Vec<HeroAbilityRow> {
        let Some(unit) = self.unit(hero) else {
            return Vec::new();
        };
        if !unit.def().hero {
            return Vec::new();
        }
        let mut rows = Vec::with_capacity(unit.def().ability_ids.len());
        for id in unit.def().ability_ids {
            // Inventory/spellbook helper rows are not learnable hero ranks.
            let Some(def) = abilities::generic_ability(id) else {
                continue;
            };
            if def.req_level == 0 {
                continue;
            }
            let Some(spell) = abilities::spell_for_ability(id) else {
                continue;
            };
            rows.push(HeroAbilityRow {
                id,
                name: def.name,
                spell,
                req_level: def.req_level,
                learned: unit.learned.contains(&id),
            });
        }
        rows
    }
}
