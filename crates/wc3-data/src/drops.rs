//! Creep item drops: the item pool comes from `ItemData` (`droppable` and the
//! item's `Level`), which is the only drop-related data the retail installs
//! carry. (`drop` is a different flag — five level-0 items — and is not the
//! creep pool.)
//!
//! **The chance table is not in the retail data.** Verified on both installs:
//! `ItemData.slk` has no chance column (`droppable` is a boolean), no drop-table file
//! exists in either listfile, `Units\MiscData.txt` carries only `DropItemRange`,
//! and the profile texts have no drop key. WC3 therefore resolves drop *chances*
//! in the engine; this module exposes the data pool and the documented engine
//! row (creep level → item level) and reports
//! [`GameDatabase::drops_data_only_reason`] instead of inventing percentages.
//! The sim supplies its own chance constant in the caller patch.

use crate::GameDatabase;

/// Why the chance table is data-only (retail carries no percentages).
pub const DROP_CHANCE_DATA_ONLY: &str =
    "retail installs carry no drop-chance table: ItemData has only the `droppable` \
     flag and `Level`, no drop-table file exists in either listfile, and MiscData has \
     only DropItemRange; WC3 resolves drop chances in the engine";

/// One item that can appear in a creep drop table.
#[derive(Debug, Clone, PartialEq)]
pub struct DropItem {
    pub id: String,
    pub name: Option<String>,
    pub class: Option<String>,
    /// Item `Level` (also the drop level).
    pub level: u8,
    pub gold: Option<i32>,
    pub charges: Option<i32>,
}

/// All droppable items of one item level (`ItemData.droppable = 1`).
#[derive(Debug, Clone, PartialEq)]
pub struct DropTable {
    pub item_level: u8,
    pub items: Vec<DropItem>,
}

/// One engine rule: creeps of `creep_level_min..=creep_level_max` draw from the
/// `item_level` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropLevelRule {
    pub creep_level_min: u8,
    pub creep_level_max: u8,
    pub item_level: u8,
}

/// Documented WC3 engine mapping (no SLK carries it; stated once, like
/// `SKILL_POINTS_PER_LEVEL`).
pub const DROP_LEVEL_RULES: [DropLevelRule; 5] = [
    DropLevelRule {
        creep_level_min: 1,
        creep_level_max: 2,
        item_level: 1,
    },
    DropLevelRule {
        creep_level_min: 3,
        creep_level_max: 4,
        item_level: 2,
    },
    DropLevelRule {
        creep_level_min: 5,
        creep_level_max: 6,
        item_level: 3,
    },
    DropLevelRule {
        creep_level_min: 7,
        creep_level_max: 8,
        item_level: 4,
    },
    DropLevelRule {
        creep_level_min: 9,
        creep_level_max: 10,
        item_level: 5,
    },
];

/// Item level a creep of `creep_level` draws from.
pub fn drop_level_for_creep_level(creep_level: u8) -> Option<u8> {
    DROP_LEVEL_RULES
        .iter()
        .find(|rule| (rule.creep_level_min..=rule.creep_level_max).contains(&creep_level))
        .map(|rule| rule.item_level)
}

impl GameDatabase {
    /// Droppable items grouped by item level, ascending.
    pub fn drop_tables(&self) -> Vec<DropTable> {
        let mut tables: Vec<DropTable> = Vec::new();
        for item in self.items() {
            if !item.droppable().unwrap_or(false) {
                continue;
            }
            let Some(level) = item.level().ok().flatten() else {
                continue;
            };
            if level < 1 || level > u8::MAX as i32 {
                continue;
            }
            let level = level as u8;
            let entry = DropItem {
                id: item.id_str(),
                name: item.name(),
                class: item.class_name().map(str::to_string),
                level,
                gold: item.gold_cost().ok().flatten(),
                charges: item.charges().ok().flatten(),
            };
            match tables.iter_mut().find(|table| table.item_level == level) {
                Some(table) => table.items.push(entry),
                None => tables.push(DropTable {
                    item_level: level,
                    items: vec![entry],
                }),
            }
        }
        tables.sort_by_key(|table| table.item_level);
        tables
    }

    /// `(item level, item count)` for every drop table, ascending.
    pub fn drop_level_distribution(&self) -> Vec<(u8, usize)> {
        self.drop_tables()
            .into_iter()
            .map(|table| (table.item_level, table.items.len()))
            .collect()
    }

    /// One item-level drop table.
    pub fn drop_table(&self, item_level: u8) -> Option<DropTable> {
        self.drop_tables()
            .into_iter()
            .find(|table| table.item_level == item_level)
    }

    /// Item ids a creep of `creep_level` can drop (engine mapping + data pool).
    pub fn drop_pool(&self, creep_level: u8) -> Vec<String> {
        let Some(level) = drop_level_for_creep_level(creep_level) else {
            return Vec::new();
        };
        self.drop_table(level)
            .map(|table| table.items.into_iter().map(|item| item.id).collect())
            .unwrap_or_default()
    }

    /// The engine mapping table (creep level → item level).
    pub fn drop_level_rules(&self) -> &'static [DropLevelRule] {
        &DROP_LEVEL_RULES
    }

    /// `Some(reason)` while the install carries no drop-chance table.
    pub fn drops_data_only_reason(&self) -> Option<&'static str> {
        if self
            .misc()
            .get("Units\\MiscData.txt", "Misc", "DropChance1")
            .is_some()
        {
            None
        } else {
            Some(DROP_CHANCE_DATA_ONLY)
        }
    }
}
