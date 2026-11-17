//! Shops, item purchase gating and item abilities from the install's own data.
//!
//! Sources (all retail, no invented table):
//! - `Units\*UnitFunc.txt` `Sellitems=` — the shop's item list. The field is a
//!   profile field, so map object data (`war3map.w3u`) overlays it through the
//!   same [`Localizer`](crate::localization::Localizer) lookup, and a custom
//!   shop works without a second reader.
//! - `ItemData.slk` — price (`goldcost`/`lumbercost`), `Level`, `itemClass`,
//!   charges (`uses`), stock (`stockMax`/`stockRegen`/`stockStart`) and the
//!   `abilList` item abilities.
//! - `ItemFunc.txt` `Requires=` / `DependencyOr=` — purchase gating. An item
//!   names a requirement token (`phea` → `TWN2`); when the token is a pseudo
//!   requirement its `DependencyOr` lists the concrete alternatives
//!   (`TWN2` → `hkee,ostr,etoa,unp1`).
//!
//! **Recipes are not a retail data structure.** Neither install ships a recipe
//! sheet or metadata, no item class is a recipe, and no item's `Requires` names
//! another item (measured: 18 gated items in TFT, 0 recipe references; see
//! `tests/hero_items.rs::retail_data_has_no_recipe_table`). Combining items in
//! WC3 is map-script logic, so this layer exposes the real shop/charge/ability
//! data and does not fabricate a recipe table.

use crate::localization::Localizer;
use crate::GameDatabase;

/// One item on a shop's list, with the numbers a shop UI needs.
#[derive(Debug, Clone, PartialEq)]
pub struct ShopItem {
    pub item_id: String,
    pub name: Option<String>,
    pub icon: Option<String>,
    pub gold: Option<i32>,
    pub lumber: Option<i32>,
    pub level: Option<i32>,
    /// `itemClass` (RoC) / `class` (TFT) — `Purchasable`, `Charged`, `Permanent`, …
    pub class: Option<String>,
    /// `uses` — charges (`0` = unlimited, `None` = column absent).
    pub charges: Option<i32>,
    pub stock_max: Option<i32>,
    pub stock_regen: Option<i32>,
    pub stock_start: Option<i32>,
    /// `sellable` — TFT column; `None` on installs without it.
    pub sellable: Option<bool>,
    /// `Requires` tokens with their `DependencyOr` alternatives expanded.
    pub requires: Vec<Requirement>,
}

/// One `Requires` token of an item.
#[derive(Debug, Clone, PartialEq)]
pub struct Requirement {
    /// The token as the item names it (`hcas`, `unp2`, `TWN2`, …).
    pub token: String,
    /// Alternatives from the pseudo-requirement section
    /// (`[TWN2] DependencyOr=hkee,ostr,etoa,unp1`); empty for concrete tokens.
    pub alternatives: Vec<String>,
}

impl Requirement {
    /// True when the token is a pseudo requirement with concrete alternatives.
    pub fn is_pseudo(&self) -> bool {
        !self.alternatives.is_empty()
    }
}

/// A shop unit and the items it sells.
#[derive(Debug, Clone, PartialEq)]
pub struct ShopDef {
    pub unit_id: String,
    pub name: Option<String>,
    pub icon: Option<String>,
    pub items: Vec<ShopItem>,
    /// `Sellunits` — units the shop sells (profile field, overlaid by map data).
    pub sells_units: Vec<String>,
}

/// Comma separated profile value, trimmed with WC3's "no object" markers dropped.
fn list_value(localizer: &Localizer, id: &str, key: &str) -> Vec<String> {
    localizer.list(id, key)
}

impl GameDatabase {
    /// `Requires` of one item, with `DependencyOr` alternatives expanded.
    pub fn item_requirements(&self, item_id: &str) -> Vec<Requirement> {
        let localizer = self.localizer();
        list_value(localizer, item_id, "Requires")
            .into_iter()
            .map(|token| Requirement {
                alternatives: list_value(localizer, &token, "DependencyOr"),
                token,
            })
            .collect()
    }

    /// One shop entry (`Sellitems` profile list) resolved against `ItemData`.
    pub fn shop(&self, unit_id: &str) -> Option<ShopDef> {
        let unit = self.unit(unit_id)?;
        let ids = list_value(self.localizer(), unit_id, "Sellitems");
        if ids.is_empty() {
            return None;
        }
        let items = ids.iter().filter_map(|id| self.shop_item(id)).collect();
        Some(ShopDef {
            unit_id: unit_id.to_string(),
            name: unit.name(),
            icon: unit.icon(),
            items,
            sells_units: list_value(self.localizer(), unit_id, "Sellunits"),
        })
    }

    /// One shop line: the item's price, stock, charges, class and requirements.
    pub fn shop_item(&self, item_id: &str) -> Option<ShopItem> {
        let item = self.item(item_id)?;
        Some(ShopItem {
            item_id: item_id.to_string(),
            name: item.name(),
            icon: item.icon(),
            gold: item.gold_cost().ok().flatten(),
            lumber: item.lumber_cost().ok().flatten(),
            level: item.level().ok().flatten(),
            class: item.class_name().map(str::to_string),
            charges: item.charges().ok().flatten(),
            stock_max: item.stock_max().ok().flatten(),
            stock_regen: item.stock_regen().ok().flatten(),
            stock_start: item.stock_start().ok().flatten(),
            sellable: item.sellable().ok().flatten(),
            requires: self.item_requirements(item_id),
        })
    }

    /// Every unit that carries a non-empty `Sellitems` list, sorted by id.
    pub fn shops(&self) -> Vec<ShopDef> {
        let mut out: Vec<ShopDef> = self
            .units()
            .filter_map(|unit| self.shop(unit.id_str().as_str()))
            .collect();
        out.sort_by(|left, right| left.unit_id.cmp(&right.unit_id));
        out
    }

    /// `Sellunits` of one shop unit (units it sells, not items).
    pub fn sold_units(&self, unit_id: &str) -> Vec<String> {
        list_value(self.localizer(), unit_id, "Sellunits")
    }

    /// Shop unit ids whose `Sellunits` contains `unit_id`.
    pub fn shops_selling_unit(&self, unit_id: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .units()
            .filter(|unit| {
                self.sold_units(&unit.id_str())
                    .iter()
                    .any(|sold| sold == unit_id)
            })
            .map(|unit| unit.id_str())
            .collect();
        out.sort();
        out
    }

    /// Shop unit ids that sell `item_id`.
    pub fn shops_selling(&self, item_id: &str) -> Vec<String> {
        self.shops()
            .into_iter()
            .filter(|shop| shop.items.iter().any(|item| item.item_id == item_id))
            .map(|shop| shop.unit_id)
            .collect()
    }

    /// Distinct ability ids referenced by any item's `abilList`, sorted.
    pub fn item_ability_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for item in self.items() {
            for ability in item.abilities() {
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

    /// `(generated, total)` over item ability ids with sheet numbers — the
    /// `qa.items.abilities` numerator/denominator.
    pub fn item_ability_coverage(&self) -> (usize, usize) {
        let ids = self.item_ability_ids();
        let covered = ids
            .iter()
            .filter(|id| {
                self.ability_numbers(id)
                    .is_some_and(|numbers| !numbers.levels.is_empty())
            })
            .count();
        (covered, ids.len())
    }

    /// Numbers for every ability an item grants (`abilList` → `AbilityData`).
    pub fn item_ability_numbers(&self, item_id: &str) -> Vec<crate::ability_def::AbilityNumbers> {
        let Some(item) = self.item(item_id) else {
            return Vec::new();
        };
        item.abilities()
            .into_iter()
            .filter_map(|id| self.ability_numbers(id))
            .collect()
    }
}
