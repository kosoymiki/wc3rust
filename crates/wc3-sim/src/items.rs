// One item-action path for pickup, drop, use and purchase.
//
// This file is compiled inside the `data` module (included from
// `items_gen.rs`) so it can read the generated `ITEMS` table and the shop
// catalogue without a second registration hook.  The public entry point is
// `Sim::item_action`; every failure carries an exact, printable reason
// (`ItemActionError::reason`) instead of a generic string.
//
// Mechanics mirrored from WC3:
// * inventory is the six-slot `Unit.items` backpack;
// * passive bonuses are applied on pickup and removed on drop/death;
// * `uses=` charges are consumed per use, and the last charge removes the
//   item; `uses` absent/0 means unlimited (staffs, orbs, crystal ball);
// * an item ability shares the unit's `data_cd` cooldown and is refused while
//   cooling down (the timer is decremented by `advance_timers`);
// * a dead carrier's items are dropped at the corpse, not deleted.

use std::fmt;

// One user/bridge action on a unit's item interface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ItemAction {
    // Ground item -> backpack (six slots, 180-unit pickup range).
    Pickup { item: crate::ItemId },
    // Backpack -> ground at the carrier's feet.
    Drop { item: crate::ItemId },
    // Consume/activate a carried item; `target`/`point` follow the item
    // ability's own target kind (heal potions ignore both).
    Use {
        item: crate::ItemId,
        target: Option<crate::UnitId>,
        point: Option<Vec2>,
    },
    // Buy one item from a shop building into the buyer's backpack.
    Buy {
        shop: crate::UnitId,
        item: &'static str,
    },
}

// Exact reason a `ItemAction` failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemActionError {
    NoUnit,
    UnitDead,
    NoItem,
    NoItemDef { item: &'static str },
    NoShop,
    NotAShop,
    ShopDead,
    BuyerDead,
    AlreadyHeld,
    NotCarried,
    InventoryFull,
    ItemTooFar,
    ShopTooFar,
    NoOwner,
    ItemNotSold { item: &'static str },
    ItemNotStocked { item: &'static str },
    OutOfStock { item: &'static str },
    NotEnoughGold { need: i32, have: i32 },
    MissingRequirement {
        item: &'static str,
        token: &'static str,
        alternatives: &'static [&'static str],
    },
    OnCooldown { item: &'static str, remaining_ticks: u32 },
    Ability { item: &'static str, reason: &'static str },
    NoEffect { item: &'static str },
}

impl ItemActionError {
    // Human- and log-readable exact reason (kept stable for tests/logs).
    pub fn reason(&self) -> String {
        match self {
            Self::NoUnit => "unit does not exist".to_string(),
            Self::UnitDead => "unit is dead".to_string(),
            Self::NoItem => "item does not exist".to_string(),
            Self::NoItemDef { item } => format!("item {item} has no ItemData row"),
            Self::NoShop => "shop does not exist".to_string(),
            Self::NotAShop => "unit is not an item shop".to_string(),
            Self::ShopDead => "shop is dead".to_string(),
            Self::BuyerDead => "buyer is dead".to_string(),
            Self::AlreadyHeld => "item is already held".to_string(),
            Self::NotCarried => "item is not carried".to_string(),
            Self::InventoryFull => "inventory full (6 slots)".to_string(),
            Self::ItemTooFar => "item is too far to pick up".to_string(),
            Self::ShopTooFar => "shop is too far".to_string(),
            Self::NoOwner => "unit has no player owner".to_string(),
            Self::ItemNotSold { item } => format!("{item} is not sold here"),
            Self::ItemNotStocked { item } => format!("{item} has no stock slot in this shop"),
            Self::OutOfStock { item } => format!("{item} is out of stock"),
            Self::NotEnoughGold { need, have } => {
                format!("not enough gold: {need} required, {have} available")
            }
            Self::MissingRequirement {
                item,
                token,
                alternatives,
            } => {
                if alternatives.is_empty() {
                    format!("missing requirement for {item}: {token}")
                } else {
                    format!(
                        "missing requirement for {item}: {token} (need one of {})",
                        alternatives.join(",")
                    )
                }
            }
            Self::OnCooldown {
                item,
                remaining_ticks,
            } => format!("{item} is on cooldown for {remaining_ticks} ticks"),
            Self::Ability { item, reason } => format!("{item} ability failed: {reason}"),
            Self::NoEffect { item } => format!("{item} has no usable effect"),
        }
    }
}

impl fmt::Display for ItemActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason())
    }
}

impl std::error::Error for ItemActionError {}

// Successful item action result.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemOutcome {
    PickedUp { item: crate::ItemId },
    Dropped { item: crate::ItemId },
    Used {
        item: crate::ItemId,
        consumed: bool,
        charges_left: i32,
    },
    Bought {
        item: crate::ItemId,
        shop: crate::UnitId,
        cost: i32,
    },
}

fn apply_item_stats(unit: &mut crate::Unit, def: &ItemDef) {
    unit.damage += def.bonus_attack;
    unit.armor += def.bonus_armor;
    unit.max_hp += def.bonus_hp;
    unit.hp += def.bonus_hp;
    unit.max_mana += def.bonus_mana;
    unit.mana += def.bonus_mana;
}

fn remove_item_stats(unit: &mut crate::Unit, def: &ItemDef) {
    unit.damage -= def.bonus_attack;
    unit.armor -= def.bonus_armor;
    unit.max_hp -= def.bonus_hp;
    unit.hp = (unit.hp - def.bonus_hp).clamp(0, unit.max_hp.max(0));
    unit.max_mana -= def.bonus_mana;
    unit.mana = (unit.mana - def.bonus_mana).clamp(0, unit.max_mana.max(0));
}

impl crate::Sim {
    // The single item interaction path (pickup / drop / use / buy).
    pub fn item_action(
        &mut self,
        unit: crate::UnitId,
        action: ItemAction,
    ) -> Result<ItemOutcome, ItemActionError> {
        match action {
            ItemAction::Pickup { item } => self.item_pickup(unit, item),
            ItemAction::Drop { item } => {
                let index = self.unit_index(unit).ok_or(ItemActionError::NoUnit)?;
                if !self.units[index].alive {
                    return Err(ItemActionError::UnitDead);
                }
                self.item_drop(unit, item)
            }
            ItemAction::Use {
                item,
                target,
                point,
            } => self.item_use(unit, item, target, point),
            ItemAction::Buy { shop, item } => self.item_buy(unit, shop, item),
        }
    }

    fn item_pickup(
        &mut self,
        unit: crate::UnitId,
        item: crate::ItemId,
    ) -> Result<ItemOutcome, ItemActionError> {
        let unit_index = self.unit_index(unit).ok_or(ItemActionError::NoUnit)?;
        if !self.units[unit_index].alive {
            return Err(ItemActionError::UnitDead);
        }
        let item_index = self
            .items
            .iter()
            .position(|entry| entry.id == item)
            .ok_or(ItemActionError::NoItem)?;
        if self.items[item_index].owner.is_some() {
            return Err(ItemActionError::AlreadyHeld);
        }
        if self.units[unit_index].items.len() >= 6 {
            return Err(ItemActionError::InventoryFull);
        }
        if self.units[unit_index]
            .pos
            .distance(self.items[item_index].pos)
            > 180.0
        {
            return Err(ItemActionError::ItemTooFar);
        }
        let def = self.items[item_index].def();
        apply_item_stats(&mut self.units[unit_index], def);
        self.units[unit_index].items.push(item);
        self.items[item_index].owner = Some(self.units[unit_index].owner);
        self.items[item_index].pos = self.units[unit_index].pos;
        Ok(ItemOutcome::PickedUp { item })
    }

    fn item_drop(
        &mut self,
        unit: crate::UnitId,
        item: crate::ItemId,
    ) -> Result<ItemOutcome, ItemActionError> {
        let unit_index = self.unit_index(unit).ok_or(ItemActionError::NoUnit)?;
        let slot = self.units[unit_index]
            .items
            .iter()
            .position(|id| *id == item)
            .ok_or(ItemActionError::NotCarried)?;
        let item_index = self
            .items
            .iter()
            .position(|entry| entry.id == item)
            .ok_or(ItemActionError::NoItem)?;
        let def = self.items[item_index].def();
        remove_item_stats(&mut self.units[unit_index], def);
        self.units[unit_index].items.remove(slot);
        let pos = self.units[unit_index].pos;
        self.items[item_index].owner = None;
        self.items[item_index].pos = pos + Vec2::new(32.0, 0.0);
        Ok(ItemOutcome::Dropped { item })
    }

    fn item_use(
        &mut self,
        unit: crate::UnitId,
        item: crate::ItemId,
        target: Option<crate::UnitId>,
        point: Option<Vec2>,
    ) -> Result<ItemOutcome, ItemActionError> {
        let unit_index = self.unit_index(unit).ok_or(ItemActionError::NoUnit)?;
        if !self.units[unit_index].alive {
            return Err(ItemActionError::UnitDead);
        }
        if !self.units[unit_index].items.contains(&item) {
            return Err(ItemActionError::NotCarried);
        }
        let item_index = self
            .items
            .iter()
            .position(|entry| entry.id == item)
            .ok_or(ItemActionError::NoItem)?;
        let def = self.items[item_index].def();
        let ability = crate::generic_ability(def.ability_id);
        let cooldown_ticks = ability.map(|ability| ability.cooldown_ticks).unwrap_or(0);
        // `ItemData.slk` groups items by `cooldownID` (healing potion and
        // Essence of Aszune share `AIhe`); `ignoreCD=1` opts out entirely.
        let cooldown_key = item_cooldown(def.id)
            .filter(|(_, ignore)| !*ignore)
            .map(|(id, _)| id)
            .unwrap_or("");
        if cooldown_ticks > 0 && !cooldown_key.is_empty() {
            let ability_id = ability.map(|ability| ability.id);
            if let Some((_, remaining)) = self.units[unit_index]
                .data_cd
                .iter()
                .find(|(id, cooldown)| {
                    (*id == cooldown_key || Some(*id) == ability_id) && *cooldown > 0
                })
            {
                return Err(ItemActionError::OnCooldown {
                    item: def.id,
                    remaining_ticks: *remaining,
                });
            }
        }
        if def.heal > 0 || def.mana_restore > 0 {
            if def.heal > 0 {
                self.heal_unit(unit, def.heal);
            }
            if def.mana_restore > 0 {
                let max_mana = self.units[unit_index].max_mana;
                self.units[unit_index].mana =
                    (self.units[unit_index].mana + def.mana_restore).min(max_mana);
            }
            if cooldown_ticks > 0 && !cooldown_key.is_empty() {
                let unit = &mut self.units[unit_index];
                unit.data_cd.retain(|(id, _)| *id != cooldown_key);
                unit.data_cd.push((cooldown_key, cooldown_ticks));
            }
        } else if ability.map(|ability| ability.castable).unwrap_or(false) {
            self.cast_item_ability(unit, def.ability_id, target, point)
                .map_err(|reason| {
                    if reason == "ability on cooldown" {
                        let remaining = self
                            .units
                            .get(unit_index)
                            .and_then(|unit| {
                                unit.data_cd
                                    .iter()
                                    .find(|(id, cooldown)| {
                                        *id == def.ability_id && *cooldown > 0
                                    })
                                    .map(|(_, cooldown)| *cooldown)
                            })
                            .unwrap_or(0);
                        ItemActionError::OnCooldown {
                            item: def.id,
                            remaining_ticks: remaining,
                        }
                    } else {
                        ItemActionError::Ability {
                            item: def.id,
                            reason,
                        }
                    }
                })?;
            if cooldown_ticks > 0 && !cooldown_key.is_empty() {
                let unit = &mut self.units[unit_index];
                unit.data_cd.retain(|(id, _)| *id != cooldown_key);
                unit.data_cd.push((cooldown_key, cooldown_ticks));
            }
        } else {
            return Err(ItemActionError::NoEffect { item: def.id });
        }
        let charges = self.items[item_index].charges;
        let consumed = charges > 0;
        let mut charges_left = charges;
        if consumed {
            charges_left = charges - 1;
            let item_index = self
                .items
                .iter()
                .position(|entry| entry.id == item)
                .ok_or(ItemActionError::NoItem)?;
            self.items[item_index].charges = charges_left;
            if charges_left <= 0 {
                let unit_index = self.unit_index(unit).ok_or(ItemActionError::NoUnit)?;
                remove_item_stats(&mut self.units[unit_index], def);
                self.units[unit_index].items.retain(|id| *id != item);
                self.items.retain(|entry| entry.id != item);
            }
        }
        Ok(ItemOutcome::Used {
            item,
            consumed,
            charges_left,
        })
    }

    fn item_buy(
        &mut self,
        buyer: crate::UnitId,
        shop: crate::UnitId,
        item: &'static str,
    ) -> Result<ItemOutcome, ItemActionError> {
        let buyer_index = self.unit_index(buyer).ok_or(ItemActionError::NoUnit)?;
        if !self.units[buyer_index].alive {
            return Err(ItemActionError::BuyerDead);
        }
        let shop_index = self.unit_index(shop).ok_or(ItemActionError::NoShop)?;
        if !self.units[shop_index].alive {
            return Err(ItemActionError::ShopDead);
        }
        let shop_def = self.units[shop_index].def();
        if shop_def.sells_items.is_empty() {
            return Err(ItemActionError::NotAShop);
        }
        if !shop_def.sells_items.contains(&item) {
            return Err(ItemActionError::ItemNotSold { item });
        }
        let def = item_def(item).ok_or(ItemActionError::NoItemDef { item })?;
        if self.units[buyer_index]
            .pos
            .distance(self.units[shop_index].pos)
            > crate::SHOP_RANGE + self.units[shop_index].radius()
        {
            return Err(ItemActionError::ShopTooFar);
        }
        let owner = self.units[buyer_index].owner as usize;
        if owner >= self.resources.len() {
            return Err(ItemActionError::NoOwner);
        }
        if self.resources[owner] < def.cost {
            return Err(ItemActionError::NotEnoughGold {
                need: def.cost,
                have: self.resources[owner],
            });
        }
        if self.units[buyer_index].items.len() >= 6 {
            return Err(ItemActionError::InventoryFull);
        }
        let owner = self.units[buyer_index].owner;
        for requirement in item_requirement_set(def.id) {
            if !owner_has_requirement(self, owner, requirement) {
                return Err(ItemActionError::MissingRequirement {
                    item: def.id,
                    token: requirement.token,
                    alternatives: requirement.alternatives,
                });
            }
        }
        let stock_index = self
            .shops
            .iter()
            .position(|entry| entry.shop == shop)
            .ok_or(ItemActionError::ItemNotStocked { item: def.id })?;
        let slot_index = self.shops[stock_index]
            .slots
            .iter()
            .position(|slot| slot.item == def.id)
            .ok_or(ItemActionError::ItemNotStocked { item: def.id })?;
        if self.shops[stock_index].slots[slot_index].stock <= 0 {
            return Err(ItemActionError::OutOfStock { item: def.id });
        }
        self.resources[owner as usize] -= def.cost;
        {
            let slot = &mut self.shops[stock_index].slots[slot_index];
            slot.stock -= 1;
            if slot.next_restock_tick == 0 {
                slot.next_restock_tick =
                    self.tick + (def.stock_regen.max(1) as u64) * crate::TICKS_PER_SECOND as u64;
            }
        }
        let def_index = crate::data::ITEMS
            .iter()
            .position(|entry| entry.id == def.id)
            .ok_or(ItemActionError::NoItemDef { item: def.id })? as u16;
        let id = crate::ItemId(self.next_item_id);
        self.next_item_id += 1;
        let (pos, unit_owner) = (self.units[buyer_index].pos, self.units[buyer_index].owner);
        self.items.push(crate::Item {
            id,
            def_index,
            pos,
            owner: Some(unit_owner),
            charges: def.charges,
        });
        self.units[buyer_index].items.push(id);
        apply_item_stats(&mut self.units[buyer_index], def);
        self.events.push(crate::Event::ItemBought {
            buyer,
            shop,
            item: def.id,
        });
        Ok(ItemOutcome::Bought {
            item: id,
            shop,
            cost: def.cost,
        })
    }

    // Per-tick item maintenance, called once from `Sim::step`: every dead
    // carrier drops its backpack at the corpse.
    //
    // This is the safety net for death paths that do not drop inline (a unit
    // killed by a future system, a save-game load of a dead carrier) and it
    // also picks up a death whose inline drop is added later.  It must stay
    // allocation-free in the common case: a plain cursor walk over the unit
    // table, no per-tick `Vec`.  `drop_items_on_death` empties the carrier's
    // slot list, so a monotonically increasing cursor visits each unit once.
    pub fn items_tick(&mut self) {
        let mut cursor = 0usize;
        while cursor < self.units.len() {
            let index = cursor;
            cursor += 1;
            if self.units[index].alive || self.units[index].items.is_empty() {
                continue;
            }
            let id = self.units[index].id;
            self.drop_items_on_death(id);
        }
    }

    // Drops every carried item of a dead unit, returning how many dropped.
    //
    // Idempotent: a living unit and a carrier with an empty backpack both
    // return 0, and an item that is already on the ground is not carried, so
    // two death paths in the same tick (inline drop + `items_tick`) cannot
    // duplicate an item or its passive bonus.  Items keep their identity and
    // lose their passive bonuses; positions are spread in a deterministic
    // ring (slot 0 at +X, clockwise) around the corpse.
    pub fn drop_items_on_death(&mut self, unit: crate::UnitId) -> usize {
        let Some(index) = self.unit_index(unit) else {
            return 0;
        };
        if self.units[index].alive || self.units[index].items.is_empty() {
            return 0;
        }
        // Take the backpack first: the loop needs `&mut self` to touch the
        // world item table, and taking the slots is what makes the whole
        // operation idempotent — a second call in the same tick finds an empty
        // list.  No heap clone, no per-death allocation beyond the list the
        // carrier already owned.
        let carried = std::mem::take(&mut self.units[index].items);
        let origin = self.units[index].pos;
        let count = carried.len();
        let mut dropped = 0;
        for (slot_index, item) in carried.into_iter().enumerate() {
            let Some(item_index) = self.items.iter().position(|entry| entry.id == item) else {
                // The def row is gone from the world table (already consumed);
                // nothing to place, and the slot is spent.
                continue;
            };
            let def = self.items[item_index].def();
            if let Some(unit_index) = self.unit_index(unit) {
                remove_item_stats(&mut self.units[unit_index], def);
            }
            let angle = slot_index as f32 * std::f32::consts::TAU / count as f32;
            self.items[item_index].owner = None;
            self.items[item_index].pos = origin + Vec2::new(angle.cos(), angle.sin()) * 32.0;
            dropped += 1;
        }
        dropped
    }
}
