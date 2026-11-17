//! Data-driven build/train menus.
//!
//! The command card is not authored in code: a producer lists its trainees in
//! the profile text layer (`Trains=` → `utra`), a worker lists its buildings in
//! `Builds=` (`ubui`), and every entry's cost/food/time comes from
//! `UnitBalance.slk`. This module binds those three sources into one
//! [`MenuEntry`] per command-card slot, so the HUD can render exactly what the
//! data says.
//!
//! Availability is *not* decided here: the database has no tech tree state. The
//! requirements are exposed verbatim (`requires`) with their display names so
//! the game/sim can test them.

use wc3_formats::gamedata::ObjectKind;

use crate::GameDatabase;

/// One command-card entry (a trainable unit or a buildable structure).
#[derive(Debug, Clone, PartialEq)]
pub struct MenuEntry {
    /// FourCC of the produced object (`hfoo`, `hhou`).
    pub id: String,
    /// Localised name (`Пехотинец`).
    pub name: String,
    /// Command-card icon (`ReplaceableTextures\\CommandButtons\\*.blp`).
    pub icon: Option<String>,
    /// Hotkey letter as stored (`F`, `B`).
    pub hotkey: Option<String>,
    pub gold: Option<i32>,
    pub lumber: Option<i32>,
    /// Food (`fused`) the produced unit costs.
    pub food: Option<i32>,
    /// Build time in seconds (`bldtm`) for units, `build_ticks` divide by the
    /// sim tick rate for buildings.
    pub build_time: Option<i32>,
    /// `Ubertip`, else `Tip`.
    pub tooltip: Option<String>,
    /// Requirement ids (`htow`, `hkee`, `Rhme`, …) exactly as the profile lists.
    pub requires: Vec<String>,
    /// Display names for [`MenuEntry::requires`], in the same order.
    pub requires_names: Vec<String>,
    /// Object kind of the produced object.
    pub kind: ObjectKind,
}

impl MenuEntry {
    /// True when the entry carries the presentation + cost data a command card
    /// needs (name, icon, gold).
    pub fn is_complete(&self) -> bool {
        !self.name.is_empty() && self.icon.is_some() && self.gold.is_some()
    }

    /// `"FR"`-style hotkey label, or the id when the data has no hotkey.
    pub fn hotkey_label(&self) -> String {
        match &self.hotkey {
            Some(hotkey) if !hotkey.is_empty() => {
                // The profile stores one letter per level (`S,S,S`); the card
                // shows the first one.
                hotkey
                    .split(',')
                    .next()
                    .unwrap_or(hotkey)
                    .trim()
                    .to_uppercase()
            }
            _ => self.id.to_ascii_uppercase(),
        }
    }

    /// Command-card sub-label: `160G 2F` (gold always, food when it costs any).
    pub fn cost_label(&self) -> String {
        let mut out = String::new();
        if let Some(gold) = self.gold {
            out.push_str(&format!("{gold}G"));
        }
        if let Some(lumber) = self.lumber.filter(|lumber| *lumber > 0) {
            out.push_str(&format!(" {lumber}L"));
        }
        if let Some(food) = self.food.filter(|food| *food > 0) {
            out.push_str(&format!(" {food}F"));
        }
        out
    }
}

impl GameDatabase {
    /// Trainees of `producer` according to the profile `Trains=` list.
    pub fn train_menu(&self, producer: &str) -> Vec<MenuEntry> {
        let Some(unit) = self.unit(producer) else {
            return Vec::new();
        };
        let ids = unit.trains();
        self.menu_entries(&ids)
    }

    /// Structures a worker can place according to the profile `Builds=` list.
    pub fn build_menu(&self, worker: &str) -> Vec<MenuEntry> {
        let Some(unit) = self.unit(worker) else {
            return Vec::new();
        };
        let ids = unit.builds();
        self.menu_entries(&ids)
    }

    /// Upgrades a building can research according to the profile `Researches=` list.
    ///
    /// Research entries come from [`GameDatabase::research_entry`], so the
    /// research-tab icon/hotkey (`Researchart`/`Researchhotkey`) is used where
    /// the data defines it and the plain `Art`/`Hotkey` otherwise.
    pub fn research_menu(&self, producer: &str) -> Vec<MenuEntry> {
        let Some(unit) = self.unit(producer) else {
            return Vec::new();
        };
        unit.researches()
            .iter()
            .filter_map(|id| self.research_entry(id))
            .collect()
    }

    /// One research-tab entry for an upgrade id.
    pub fn research_entry(&self, id: &str) -> Option<MenuEntry> {
        let upgrade = self.upgrade(id)?;
        let requires = upgrade.requires();
        let requires_names = requires.iter().map(|req| self.requirement_name(req)).collect();
        Some(MenuEntry {
            id: upgrade.id_str(),
            name: upgrade.name().unwrap_or_else(|| id.to_string()),
            icon: upgrade.research_icon(),
            hotkey: upgrade.research_hotkey(),
            gold: upgrade.gold_cost(1).ok().flatten(),
            lumber: upgrade.lumber_cost(1).ok().flatten(),
            food: None,
            build_time: upgrade.research_time(1).ok().flatten(),
            tooltip: upgrade.research_ubertip().or_else(|| upgrade.research_tip()),
            requires,
            requires_names,
            kind: ObjectKind::Upgrade,
        })
    }

    /// Menu entries for explicit ids, in the given order.
    pub fn menu_entries(&self, ids: &[String]) -> Vec<MenuEntry> {
        ids.iter()
            .filter_map(|id| self.menu_entry(id))
            .collect()
    }

    /// One menu entry for `id`, or `None` when no table defines it.
    pub fn menu_entry(&self, id: &str) -> Option<MenuEntry> {
        let info = self.object(id)?;
        let stats = &info.stats;
        // Requirements live in the profile text, in both retail encodings.
        let requires = self.requirements(id);
        let requires_names = requires
            .iter()
            .map(|req| self.requirement_name(req))
            .collect();
        Some(MenuEntry {
            id: info.id,
            name: info.name.unwrap_or_else(|| id.to_string()),
            icon: info.icon,
            hotkey: info.hotkey,
            gold: stats.gold_cost,
            lumber: stats.lumber_cost,
            food: stats.food_used,
            build_time: stats.build_time,
            tooltip: info.ubertip.or(info.tip),
            requires,
            requires_names,
            kind: info.kind,
        })
    }

    /// Display name of a tech requirement (unit, upgrade or ability id).
    pub fn requirement_name(&self, id: &str) -> String {
        if let Some(object) = self.object(id) {
            if let Some(name) = object.name {
                return name;
            }
        }
        id.to_string()
    }

    /// Every producer in the data, with its trainee count: `(producer, count)`.
    pub fn producers(&self) -> Vec<(String, usize)> {
        let mut out = Vec::new();
        for unit in self.units() {
            let trains = unit.trains();
            if trains.is_empty() {
                continue;
            }
            out.push((unit.id_str(), trains.len()));
        }
        out.sort();
        out
    }

    /// Every worker in the data, with its build-list size: `(worker, count)`.
    pub fn builders(&self) -> Vec<(String, usize)> {
        let mut out = Vec::new();
        for unit in self.units() {
            let builds = unit.builds();
            if builds.is_empty() {
                continue;
            }
            out.push((unit.id_str(), builds.len()));
        }
        out.sort();
        out
    }

    /// Every building with a non-empty `Researches` list: `(building, count)`.
    pub fn researchers(&self) -> Vec<(String, usize)> {
        let mut out = Vec::new();
        for unit in self.units() {
            let researches = unit.researches();
            if researches.is_empty() {
                continue;
            }
            out.push((unit.id_str(), researches.len()));
        }
        out.sort();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_entry_labels_come_from_the_data() {
        let entry = MenuEntry {
            id: "hfoo".to_string(),
            name: "Пехотинец".to_string(),
            icon: Some("BTNFootman.blp".to_string()),
            hotkey: Some("F".to_string()),
            gold: Some(160),
            lumber: Some(0),
            food: Some(2),
            build_time: Some(20),
            tooltip: None,
            requires: vec!["htow".to_string()],
            requires_names: vec!["Ратуша".to_string()],
            kind: ObjectKind::Unit,
        };
        assert_eq!(entry.hotkey_label(), "F");
        assert_eq!(entry.cost_label(), "160G 2F");
        assert!(entry.is_complete());
        assert_eq!(entry.requires_names, ["Ратуша"]);
    }

    #[test]
    fn per_level_hotkey_lists_use_the_first_letter() {
        let entry = MenuEntry {
            id: "Rhme".to_string(),
            name: "Железные мечи".to_string(),
            icon: None,
            hotkey: Some("S,S,S".to_string()),
            gold: Some(100),
            lumber: Some(50),
            food: None,
            build_time: Some(60),
            tooltip: None,
            requires: Vec::new(),
            requires_names: Vec::new(),
            kind: ObjectKind::Upgrade,
        };
        assert_eq!(entry.hotkey_label(), "S");
        assert_eq!(entry.cost_label(), "100G 50L");
    }
}
