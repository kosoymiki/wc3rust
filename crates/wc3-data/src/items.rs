//! Items: the `ItemData.slk` row plus profile text (name, tooltip, icon).

use wc3_formats::gamedata::FourCc;

use crate::error::{DataError, Result};
use crate::table::{Table, TableKind};
use crate::GameData;

/// How an item participates in play, derived from its own row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemUsage {
    /// `usable = 1`: the item has an active use (potion, scroll, wand, …).
    Usable,
    /// Not usable, but it grants abilities (stat/aura items).
    PassiveStat,
    /// No ability and not usable: campaign/quest row with no effect data.
    DataOnly,
}

/// One item's usage classification.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemUsageInfo {
    pub id: String,
    pub usage: ItemUsage,
    pub abilities: Vec<String>,
    /// Present for [`ItemUsage::DataOnly`].
    pub reason: Option<&'static str>,
}

/// An item type.
#[derive(Clone, Copy)]
pub struct ItemView<'a> {
    pub(crate) gd: &'a GameData,
    pub(crate) id: FourCc,
}

impl<'a> ItemView<'a> {
    pub fn id(&self) -> FourCc {
        self.id
    }

    pub fn id_str(&self) -> String {
        self.id.as_str().into_owned()
    }

    fn table(&self) -> Result<&'a Table> {
        self.gd
            .table(TableKind::ItemData)
            .ok_or(DataError::MissingColumn {
                table: "ItemData".to_string(),
                column: "itemID".to_string(),
            })
    }

    fn cell(&self, column: &str) -> Option<&'a str> {
        self.gd
            .table(TableKind::ItemData)?
            .text(self.id.as_str().as_ref(), column)
    }

    fn num(&self, column: &str) -> Result<Option<i32>> {
        self.table()?.i32_overlaid(self.id.as_str().as_ref(), column)
    }

    fn flag(&self, column: &str) -> Result<Option<bool>> {
        self.table()?.bool(self.id.as_str().as_ref(), column)
    }

    // ---- text -----------------------------------------------------------

    pub fn name(&self) -> Option<String> {
        self.gd.localizer().name(self.id.as_str().as_ref())
    }

    pub fn tip(&self) -> Option<String> {
        self.gd.localizer().tip(self.id.as_str().as_ref())
    }

    pub fn ubertip(&self) -> Option<String> {
        self.gd.localizer().ubertip(self.id.as_str().as_ref())
    }

    pub fn description(&self) -> Option<String> {
        self.gd
            .localizer()
            .description(self.id.as_str().as_ref())
    }

    pub fn icon(&self) -> Option<String> {
        self.gd.localizer().icon(self.id.as_str().as_ref())
    }

    pub fn hotkey(&self) -> Option<String> {
        self.gd.localizer().hotkey(self.id.as_str().as_ref())
    }

    pub fn button_pos(&self) -> Option<(u32, u32)> {
        self.gd.localizer().button_pos(self.id.as_str().as_ref())
    }

    // ---- model ----------------------------------------------------------

    /// `file` — the ground model of the item.
    pub fn model_path(&self) -> Option<&'a str> {
        self.cell("file")
    }

    pub fn model_file(&self) -> Option<String> {
        crate::art::normalize_model_path(self.model_path()?)
    }

    pub fn model_exists(&self) -> Option<bool> {
        let catalog = self.gd.catalog()?;
        Some(catalog.resolve_model(&self.model_file()?).is_some())
    }

    // ---- stats ----------------------------------------------------------

    /// Item class under either shipped column name: RoC spells it `itemClass`,
    /// TFT spells it `class`. Both trees expose a class for every item row, so
    /// callers never have to know which install they loaded (`items.schema`).
    pub fn class(&self) -> Option<&'a str> {
        self.class_name()
    }

    /// Item class under either column name: RoC spells it `itemClass`, TFT `class`.
    pub fn class_name(&self) -> Option<&'a str> {
        self.cell("itemClass").or_else(|| self.cell("class"))
    }

    /// `sellable` — whether the item can be sold in a shop (TFT column).
    pub fn sellable(&self) -> Result<Option<bool>> {
        self.flag("sellable")
    }

    /// `powerup` — power-up item flag (TFT column).
    pub fn powerup(&self) -> Result<Option<bool>> {
        self.flag("powerup")
    }

    /// `cooldownID` — item cooldown group (TFT column; `None` on RoC).
    pub fn cooldown_id(&self) -> Option<String> {
        self.cell("cooldownID").map(str::to_string)
    }

    /// `ignoreCD` — the item ignores its cooldown group.
    pub fn ignore_cooldown(&self) -> bool {
        self.flag("ignoreCD").ok().flatten().unwrap_or(false)
    }

    /// `drop` — the item appears in creep drop tables.
    pub fn drop_flag(&self) -> bool {
        self.flag("drop").ok().flatten().unwrap_or(false)
    }

    /// `pickRandom` — a random item from the row's class (TFT column).
    pub fn pick_random(&self) -> bool {
        self.flag("pickRandom").ok().flatten().unwrap_or(false)
    }

    /// `Requires` purchase gating with `DependencyOr` alternatives expanded.
    pub fn requirements(&self) -> Vec<crate::shop::Requirement> {
        self.gd.item_requirements(self.id_str().as_str())
    }

    /// Item abilities (`abilList`) resolved against `AbilityData`.
    pub fn ability_views(&self) -> Vec<crate::abilities::AbilityView<'a>> {
        self.abilities()
            .into_iter()
            .filter_map(|id| self.gd.ability(id))
            .collect()
    }

    /// `Level` — item level (0 for level-less items).
    pub fn level(&self) -> Result<Option<i32>> {
        self.num("Level")
    }

    /// `uses` — number of charges (`0` for unlimited).
    pub fn charges(&self) -> Result<Option<i32>> {
        self.num("uses")
    }

    /// `usable` — whether the item can be used from the inventory.
    pub fn usable(&self) -> Result<bool> {
        Ok(self.flag("usable")?.unwrap_or(false))
    }

    pub fn perishable(&self) -> Result<bool> {
        Ok(self.flag("perishable")?.unwrap_or(false))
    }

    pub fn droppable(&self) -> Result<bool> {
        Ok(self.flag("droppable")?.unwrap_or(false))
    }

    /// `drop` — whether the item drops from creeps.
    pub fn drops_from_creeps(&self) -> Result<bool> {
        Ok(self.flag("drop")?.unwrap_or(false))
    }

    pub fn gold_cost(&self) -> Result<Option<i32>> {
        self.num("goldcost")
    }

    pub fn lumber_cost(&self) -> Result<Option<i32>> {
        self.num("lumbercost")
    }

    /// `HP` — hit points of the item (destructible items).
    pub fn hp(&self) -> Result<Option<i32>> {
        self.num("HP")
    }

    /// `armor` — armour *material* of the item (`Wood`, `Stone`, …).
    ///
    /// The column is a material name, not a number (`observed`: `ratc` carries
    /// `Wood` in `ItemData.slk`), so it is exposed as text.
    pub fn armor_material(&self) -> Option<&'a str> {
        self.cell("armor")
    }

    pub fn stock_max(&self) -> Result<Option<i32>> {
        self.num("stockMax")
    }

    pub fn stock_regen(&self) -> Result<Option<i32>> {
        self.num("stockRegen")
    }

    pub fn stock_start(&self) -> Result<Option<i32>> {
        self.num("stockStart")
    }

    /// `idealPrio` / `prio` — shop placement priorities.
    pub fn ideal_priority(&self) -> Result<Option<i32>> {
        self.num("idealPrio")
    }

    pub fn priority(&self) -> Result<Option<i32>> {
        self.num("prio")
    }

    /// `abilList` — abilities the item grants.
    pub fn abilities(&self) -> Vec<&'a str> {
        self.gd
            .table(TableKind::ItemData)
            .map(|table| table.list(self.id.as_str().as_ref(), "abilList"))
            .unwrap_or_default()
    }

    /// `comment` — editor comment.
    pub fn comments(&self) -> Option<&'a str> {
        self.cell("comment")
    }
}

/// Why a row is data-only: retail carries no recipe/effect table for it.
pub const ITEM_DATA_ONLY_REASON: &str =
    "no ability and usable=0: campaign/quest row; retail carries no recipe or \
     effect table for this item";

impl GameData {
    /// Classify one item from its `usable` flag and `abilList`.
    pub fn item_usage(&self, id: &str) -> Option<ItemUsageInfo> {
        let item = self.item(id)?;
        let abilities: Vec<String> = item
            .abilities()
            .into_iter()
            .map(|ability| {
                let ability = ability.trim();
                if ability == "_" || ability == "-" {
                    String::new()
                } else {
                    ability.to_string()
                }
            })
            .filter(|ability| !ability.is_empty())
            .collect();
        let usable = item.usable().unwrap_or(false);
        let usage = if usable && !abilities.is_empty() {
            ItemUsage::Usable
        } else if !abilities.is_empty() {
            ItemUsage::PassiveStat
        } else {
            ItemUsage::DataOnly
        };
        Some(ItemUsageInfo {
            id: id.to_string(),
            usage,
            abilities,
            reason: (usage == ItemUsage::DataOnly).then_some(ITEM_DATA_ONLY_REASON),
        })
    }

    /// Every item classified, sorted by id.
    pub fn item_usages(&self) -> Vec<ItemUsageInfo> {
        let mut out: Vec<ItemUsageInfo> = self
            .items()
            .filter_map(|item| self.item_usage(&item.id_str()))
            .collect();
        out.sort_by(|left, right| left.id.cmp(&right.id));
        out
    }

    /// `(usable, passive_stat, data_only)` counts.
    pub fn item_usage_counts(&self) -> (usize, usize, usize) {
        let mut counts = (0, 0, 0);
        for usage in self.item_usages() {
            match usage.usage {
                ItemUsage::Usable => counts.0 += 1,
                ItemUsage::PassiveStat => counts.1 += 1,
                ItemUsage::DataOnly => counts.2 += 1,
            }
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemorySource;

    fn source() -> MemorySource {
        let mut source = MemorySource::new();
        source.insert(
            "Units\\ItemData.slk",
            b"ID;PWXL;N;E\nB;Y2;X7;D0\n\
C;Y1;X1;K\"itemID\"\nC;X2;K\"comment\"\nC;X3;K\"file\"\nC;X4;K\"itemClass\"\nC;X5;K\"Level\"\n\
C;X6;K\"uses\"\nC;X7;K\"abilList\"\n\
C;Y2;X1;K\"ratc\"\nC;X2;K\"Claws of Attack\"\nC;X3;K\"Objects\\InventoryItems\\ClawsOfAttack\\ClawsOfAttack\"\n\
C;X4;K\"Permanent\"\nC;X5;K3\nC;X6;K0\nC;X7;K\"AIta\"\n"
                .to_vec(),
        );
        source
    }

    #[test]
    fn reads_item_stats() {
        let gd = GameData::load(&source()).expect("load");
        let item = gd.item("ratc").expect("item");
        assert_eq!(item.class(), Some("Permanent"));
        assert_eq!(item.level().unwrap(), Some(3));
        assert_eq!(item.charges().unwrap(), Some(0));
        assert_eq!(item.abilities(), ["AIta"]);
        assert_eq!(
            item.model_file().as_deref(),
            Some("Objects\\InventoryItems\\ClawsOfAttack\\ClawsOfAttack.mdx")
        );
    }
}
