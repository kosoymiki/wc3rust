// Shop catalogue runtime: one [`ShopEntry`] per item a shop sells.
//
// This file is compiled inside the `data` module (included from
// `shops_gen.rs`), next to the generated `SHOP_DEFS` table.  The catalogue
// itself is real install data:
//
// * `Makeitems=` (melee: `hvlt`, `ovln`, `utom`, `eden`) and `Sellitems=`
//   (neutral: `ngme`) in `Units/*UnitFunc.txt` — the sold item ids and order;
// * `ItemData.slk` — `goldcost`, `Level`, `class`, `uses`, `stockMax`,
//   `stockRegen` (already materialised in `ItemDef`);
// * `ItemFunc.txt` `Requires=`/`DependencyOr=` — the tech-token prerequisites
//   (`ItemRequirement`).
//
// Stock and restock are the live `Shop` slots when the shop building exists in
// the simulation, and the item's data cap otherwise.

// One purchase line of a shop building.
#[derive(Debug, Clone)]
pub struct ShopEntry {
    // Generated `ItemData.slk` row (name, class, level, charges, ability).
    pub def: &'static ItemDef,
    // `goldcost`: the gold a purchase debits (mirrors `def.cost`, explicit).
    pub cost: i32,
    // Current stock: the live slot, or `stock_max` before the slot exists.
    pub stock: i32,
    // `stockMax` — the cap the slot restocks to.
    pub stock_max: i32,
    // `stockRegen` seconds converted to ticks; 0 when the item never restocks.
    pub restock_ticks: u32,
    // Real `Requires`/`DependencyOr` rows for this item.
    pub requires: &'static [ItemRequirement],
    // True when `owner` has every required building (or has no requirement).
    pub available: bool,
}

// The catalogue of one shop `id` (generated def, sold ids untouched).
pub fn shop_def(id: &str) -> Option<&'static UnitDef> {
    crate::data::SHOP_DEFS.iter().find(|def| def.id == id)
}

// Every generated shop def for a race, sorted by id.
pub fn shops_for_race(race: Race) -> Vec<&'static UnitDef> {
    let mut out: Vec<&'static UnitDef> = crate::data::SHOP_DEFS
        .iter()
        .filter(|def| def.race == race)
        .collect();
    out.sort_by_key(|def| def.id);
    out
}

// True when `owner` owns a completed building for one requirement token.
//
// `alternatives` is the `DependencyOr=` list of a pseudo token (`TWN2` →
// `hkee,ostr,etoa,unp1`); an empty list means the token itself is the
// concrete building id.
fn owner_has_requirement(sim: &crate::Sim, owner: u8, requirement: &ItemRequirement) -> bool {
    let alternatives: &[&'static str] = if requirement.alternatives.is_empty() {
        std::slice::from_ref(&requirement.token)
    } else {
        requirement.alternatives
    };
    alternatives.iter().any(|id| {
        sim.units.iter().any(|unit| {
            unit.alive
                && unit.owner == owner
                && unit.is_building()
                && unit.construct.is_none()
                && unit.def().id == *id
        })
    })
}

impl crate::Sim {
    // Full catalogue of one shop building for a prospective buyer owner.
    //
    // Returns an empty vector when `shop` is not a unit, or is not a building
    // with a real `Makeitems=`/`Sellitems=` list.  Every entry carries the
    // item def, gold cost, live stock, restock interval in ticks and the real
    // tech-token prerequisites, plus `available` computed against `owner`.
    pub fn shop_inventory(&self, shop: crate::UnitId, owner: u8) -> Vec<ShopEntry> {
        let Some(unit) = self.unit(shop) else {
            return Vec::new();
        };
        let def = unit.def();
        if def.sells_items.is_empty() {
            return Vec::new();
        }
        let runtime = self.shops.iter().find(|entry| entry.shop == shop);
        def.sells_items
            .iter()
            .filter_map(|id| {
                let item = item_def(id)?;
                let slot = runtime.and_then(|shop| {
                    shop.slots.iter().find(|slot| slot.item == item.id)
                });
                let requires = item_requirement_set(item.id);
                Some(ShopEntry {
                    def: item,
                    cost: item.cost,
                    stock: slot.map(|slot| slot.stock).unwrap_or(item.stock_max.max(1)),
                    stock_max: item.stock_max.max(1),
                    restock_ticks: if item.stock_regen > 0 {
                        item.stock_regen as u32 * crate::TICKS_PER_SECOND
                    } else {
                        0
                    },
                    requires,
                    available: requires
                        .iter()
                        .all(|requirement| owner_has_requirement(self, owner, requirement)),
                })
            })
            .collect()
    }
}
