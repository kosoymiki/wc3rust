//! Typed definition tables (units, weapons, items, upgrades, upgrade effects,
//! buffs) with one [`GameDatabase::defs`] entry point.
//!
//! Every row of every table is converted; a row that cannot be read is reported
//! in [`Defs::unparsed`] with the family, id and the exact accessor error, so a
//! caller never has to guess why a count is short.
//!
//! Sources: `UnitData`/`UnitBalance`/`UnitWeapons`/`UnitAbilities` (`unitUI` for
//! `dropItems`), `ItemData`, `UpgradeData` + `*UpgradeFunc.txt` `Requires`,
//! `UpgradeEffectMetaData` (effect definitions) and the `*AbilityFunc.txt`
//! buff sections (`Buffart`/`Bufftip`).

use wc3_formats::gamedata::MetaKind;

use crate::error::Result;
use crate::GameDatabase;

/// One row that could not be converted, with the reason.
#[derive(Debug, Clone, PartialEq)]
pub struct UnparsedRow {
    pub family: &'static str,
    pub id: String,
    pub reason: String,
}

/// A unit row (`UnitData` + `UnitBalance` + `unitUI`).
#[derive(Debug, Clone, PartialEq)]
pub struct UnitDef {
    pub id: String,
    pub name: Option<String>,
    pub race: Option<String>,
    /// `UnitData.type` flags (`TownHall,Mechanical`).
    pub class: Vec<String>,
    pub level: Option<i32>,
    pub hp: Option<i32>,
    pub mana: Option<i32>,
    pub armor: Option<i32>,
    pub speed: Option<i32>,
    pub food_used: Option<i32>,
    pub food_provided: Option<i32>,
    pub gold: Option<i32>,
    pub lumber: Option<i32>,
    pub build_time: Option<i32>,
    /// `unitUI.dropItems`.
    pub drop_items: bool,
    /// `UnitAbilities.abilList`.
    pub abilities: Vec<String>,
    /// `UnitAbilities.heroAbilList`.
    pub hero_abilities: Vec<String>,
    /// `UnitBalance.upgrades` — upgrades that apply to this unit.
    pub upgrades: Vec<String>,
}

/// One weapon slot (`UnitWeapons` row, slot 1 or 2).
#[derive(Debug, Clone, PartialEq)]
pub struct WeaponSlotDef {
    pub slot: u8,
    pub weapon_type: Option<String>,
    pub attack_type: Option<String>,
    pub min_damage: Option<i32>,
    pub max_damage: Option<i32>,
    pub attacks_per_second: Option<f32>,
    pub range: Option<i32>,
    pub projectile: Option<String>,
    pub projectile_speed: Option<u32>,
}

/// One `UnitWeapons` row (a unit's weapons, primary and optional secondary).
#[derive(Debug, Clone, PartialEq)]
pub struct WeaponDef {
    pub unit_id: String,
    pub slots: Vec<WeaponSlotDef>,
}

/// One `ItemData` row.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemDef {
    pub id: String,
    pub name: Option<String>,
    pub class: Option<String>,
    /// `Level` — also the item's drop level (`drop` tables are grouped by it).
    pub level: Option<i32>,
    pub gold: Option<i32>,
    pub lumber: Option<i32>,
    /// `uses` — charges (`0` = unlimited).
    pub charges: Option<i32>,
    /// `cooldownID` (TFT column; `None` on RoC).
    pub cooldown_id: Option<String>,
    /// `ignoreCD`.
    pub ignore_cooldown: bool,
    pub droppable: bool,
    /// `drop` — appears in creep drop tables.
    pub drop: bool,
    pub pick_random: bool,
    pub powerup: bool,
    pub sellable: bool,
    pub usable: bool,
    pub perishable: bool,
    pub stock_max: Option<i32>,
    /// `stockRegen` — restock interval in seconds.
    pub stock_regen: Option<i32>,
    /// `stockStart` — initial stock delay in seconds.
    pub stock_start: Option<i32>,
    pub abilities: Vec<String>,
}

/// One upgrade effect slot (`effectN`/`baseN`/`modN`/`codeN`).
#[derive(Debug, Clone, PartialEq)]
pub struct UpgradeEffectValue {
    pub effect: String,
    /// Level-1 value (fractional for armour/speed effects, e.g. `Rhar` 0.2).
    pub base: Option<f32>,
    pub per_level: Option<f32>,
}

/// One `UpgradeData` row plus its profile requirements and owning units.
#[derive(Debug, Clone, PartialEq)]
pub struct UpgradeDef {
    pub id: String,
    pub name: Option<String>,
    pub class: Option<String>,
    pub race: Option<String>,
    pub max_level: Option<i32>,
    pub used: bool,
    pub inherit: bool,
    pub gold_base: Option<i32>,
    pub gold_per_level: Option<i32>,
    pub lumber_base: Option<i32>,
    pub lumber_per_level: Option<i32>,
    pub time_base: Option<i32>,
    pub time_per_level: Option<i32>,
    pub effects: Vec<UpgradeEffectValue>,
    /// `Requires` / `Requirescount` from `*UpgradeFunc.txt`.
    pub requires: Vec<String>,
    pub requires_count: Option<i32>,
    /// Units whose `UnitBalance.upgrades` names this upgrade.
    pub units: Vec<String>,
    /// Abilities whose profile `Requires` names this upgrade (ability-unlock
    /// upgrades: the 37 rows with no stat effect slot).
    pub unlocks: Vec<String>,
}

/// One `UpgradeEffectMetaData` row (the effect id's editor definition).
#[derive(Debug, Clone, PartialEq)]
pub struct UpgradeEffectDef {
    pub id: String,
    pub display_name: Option<String>,
    pub data_type: Option<String>,
    pub effect_type: Option<String>,
    pub min: Option<i32>,
    pub max: Option<i32>,
}

/// One buff section (`*AbilityFunc.txt` with `Buffart`/`Bufftip`).
#[derive(Debug, Clone, PartialEq)]
pub struct BuffDef {
    pub id: String,
    pub art: Option<String>,
    pub tip: Option<String>,
    /// Ability that applies the buff, when the install links it (`B` + ability
    /// tail, or a unique shared icon); `None` for unused/removed abilities.
    pub ability: Option<String>,
    /// How the link was found: `"id"` or `"icon"`.
    pub matched_by: Option<&'static str>,
}

/// One `DestructableData` row (trees, rocks, gates: model + texture + pathing).
#[derive(Debug, Clone, PartialEq)]
pub struct DestructableDef {
    pub id: String,
    pub name: Option<String>,
    pub category: Option<String>,
    pub model: Option<String>,
    pub texture: Option<String>,
    pub path_texture: Option<String>,
    pub hp: Option<i32>,
    pub armor_type: Option<String>,
    pub walkable: bool,
    pub on_water: bool,
}

/// One `Doodads.slk` row (environment prop: model + pathing; the texture is
/// baked into the MDX, so the sheet has no texture column).
#[derive(Debug, Clone, PartialEq)]
pub struct DoodadDef {
    pub id: String,
    pub name: Option<String>,
    pub category: Option<String>,
    pub model: Option<String>,
    pub path_texture: Option<String>,
    pub walkable: bool,
    pub on_water: bool,
    pub animate_in_fog: bool,
}

/// All typed definition tables of one install.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Defs {
    pub units: Vec<UnitDef>,
    pub weapons: Vec<WeaponDef>,
    pub items: Vec<ItemDef>,
    pub upgrades: Vec<UpgradeDef>,
    pub effects: Vec<UpgradeEffectDef>,
    pub buffs: Vec<BuffDef>,
    pub destructables: Vec<DestructableDef>,
    pub doodads: Vec<DoodadDef>,
    pub unparsed: Vec<UnparsedRow>,
}

impl Defs {
    fn unparsed(&mut self, family: &'static str, id: String, reason: String) {
        self.unparsed.push(UnparsedRow {
            family,
            id,
            reason,
        });
    }

    /// Total rows converted across the four balance tables.
    pub fn rows(&self) -> usize {
        self.units.len()
            + self.weapons.len()
            + self.items.len()
            + self.upgrades.len()
            + self.destructables.len()
            + self.doodads.len()
    }
}

fn push_err<T>(defs: &mut Defs, family: &'static str, id: &str, slot: &'static str, value: Result<T>) {
    if let Err(error) = value {
        defs.unparsed(family, id.to_string(), format!("{slot}: {error}"));
    }
}

impl GameDatabase {
    /// Convert every definition row of the install.
    pub fn defs(&self) -> Defs {
        let mut defs = Defs::default();

        for unit in self.units() {
            let id = unit.id_str();
            push_err(&mut defs, "units", &id, "level", unit.level());
            push_err(&mut defs, "units", &id, "hp", unit.hp());
            push_err(&mut defs, "units", &id, "mana", unit.mana());
            push_err(&mut defs, "units", &id, "armor", unit.armor_base());
            push_err(&mut defs, "units", &id, "speed", unit.speed());
            push_err(&mut defs, "units", &id, "food_used", unit.food_used());
            push_err(&mut defs, "units", &id, "food_provided", unit.food_provided());
            push_err(&mut defs, "units", &id, "gold", unit.gold_cost());
            push_err(&mut defs, "units", &id, "lumber", unit.lumber_cost());
            push_err(&mut defs, "units", &id, "build_time", unit.build_time());

            let mut slots = Vec::new();
            for weapon in unit.weapons() {
                let mut slot = WeaponSlotDef {
                    slot: weapon.slot().number(),
                    weapon_type: weapon.weapon_type().map(str::to_string),
                    attack_type: weapon.attack_type().map(str::to_string),
                    min_damage: None,
                    max_damage: None,
                    attacks_per_second: None,
                    range: None,
                    projectile: weapon.projectile(),
                    projectile_speed: weapon.projectile_speed(),
                };
                match weapon.min_damage() {
                    Ok(value) => slot.min_damage = value,
                    Err(error) => {
                        defs.unparsed("weapons", id.clone(), format!("min_damage: {error}"))
                    }
                }
                match weapon.max_damage() {
                    Ok(value) => slot.max_damage = value,
                    Err(error) => {
                        defs.unparsed("weapons", id.clone(), format!("max_damage: {error}"))
                    }
                }
                match weapon.attacks_per_second() {
                    Ok(value) => slot.attacks_per_second = value,
                    Err(error) => defs.unparsed(
                        "weapons",
                        id.clone(),
                        format!("attacks_per_second: {error}"),
                    ),
                }
                match weapon.range() {
                    Ok(value) => slot.range = value,
                    Err(error) => {
                        defs.unparsed("weapons", id.clone(), format!("range: {error}"))
                    }
                }
                slots.push(slot);
            }
            defs.weapons.push(WeaponDef {
                unit_id: id.clone(),
                slots,
            });

            defs.units.push(UnitDef {
                id: id.clone(),
                name: unit.name(),
                race: unit.race().map(str::to_string),
                class: unit.type_flags().iter().map(|flag| flag.to_string()).collect(),
                level: unit.level().ok().flatten(),
                hp: unit.hp().ok().flatten(),
                mana: unit.mana().ok().flatten(),
                armor: unit.armor_base().ok().flatten(),
                speed: unit.speed().ok().flatten(),
                food_used: unit.food_used().ok().flatten(),
                food_provided: unit.food_provided().ok().flatten(),
                gold: unit.gold_cost().ok().flatten(),
                lumber: unit.lumber_cost().ok().flatten(),
                build_time: unit.build_time().ok().flatten(),
                drop_items: unit.drop_items().unwrap_or(false),
                abilities: unit.abilities().iter().map(|s| s.to_string()).collect(),
                hero_abilities: unit.hero_abilities().iter().map(|s| s.to_string()).collect(),
                upgrades: unit.upgrades().iter().map(|s| s.to_string()).collect(),
            });
        }

        for item in self.items() {
            let id = item.id_str();
            push_err(&mut defs, "items", &id, "level", item.level());
            push_err(&mut defs, "items", &id, "gold", item.gold_cost());
            push_err(&mut defs, "items", &id, "lumber", item.lumber_cost());
            push_err(&mut defs, "items", &id, "charges", item.charges());
            push_err(&mut defs, "items", &id, "stock_max", item.stock_max());
            push_err(&mut defs, "items", &id, "stock_regen", item.stock_regen());
            push_err(&mut defs, "items", &id, "stock_start", item.stock_start());
            push_err(&mut defs, "items", &id, "usable", item.usable());
            push_err(&mut defs, "items", &id, "sellable", item.sellable());
            defs.items.push(ItemDef {
                id: id.clone(),
                name: item.name(),
                class: item.class_name().map(str::to_string),
                level: item.level().ok().flatten(),
                gold: item.gold_cost().ok().flatten(),
                lumber: item.lumber_cost().ok().flatten(),
                charges: item.charges().ok().flatten(),
                cooldown_id: item.cooldown_id(),
                ignore_cooldown: item.ignore_cooldown(),
                droppable: item.droppable().unwrap_or(false),
                drop: item.drop_flag(),
                pick_random: item.pick_random(),
                powerup: item.powerup().ok().flatten().unwrap_or(false),
                sellable: item.sellable().ok().flatten().unwrap_or(false),
                usable: item.usable().unwrap_or(false),
                perishable: item.perishable().unwrap_or(false),
                stock_max: item.stock_max().ok().flatten(),
                stock_regen: item.stock_regen().ok().flatten(),
                stock_start: item.stock_start().ok().flatten(),
                abilities: item.abilities().iter().map(|s| s.to_string()).collect(),
            });
        }

        // Reverse index: which units name an upgrade in `UnitBalance.upgrades`.
        let mut upgrade_units: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for unit in self.units() {
            let unit_id = unit.id_str();
            for upgrade in unit.upgrades() {
                let upgrade = upgrade.trim();
                if upgrade.is_empty() || upgrade == "_" || upgrade == "-" {
                    continue;
                }
                upgrade_units
                    .entry(upgrade.to_string())
                    .or_default()
                    .push(unit_id.clone());
            }
        }

        // Abilities name the upgrade that unlocks them in their profile `Requires`.
        let mut unlock_abilities: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for ability in self.abilities() {
            let ability_id = ability.id_str();
            for token in self.localizer().components(&ability_id, "Requires") {
                let token = token.trim().to_string();
                if token.is_empty() || token == "_" || token == "-" {
                    continue;
                }
                unlock_abilities
                    .entry(token)
                    .or_default()
                    .push(ability_id.clone());
            }
        }

        for upgrade in self.upgrades() {
            let id = upgrade.id_str();
            push_err(&mut defs, "upgrades", &id, "max_level", upgrade.max_level());
            push_err(&mut defs, "upgrades", &id, "gold_base", upgrade.gold_base());
            push_err(&mut defs, "upgrades", &id, "gold_per_level", upgrade.gold_per_level());
            push_err(&mut defs, "upgrades", &id, "lumber_base", upgrade.lumber_base());
            push_err(
                &mut defs,
                "upgrades",
                &id,
                "lumber_per_level",
                upgrade.lumber_per_level(),
            );
            push_err(&mut defs, "upgrades", &id, "time_base", upgrade.time_base());
            push_err(
                &mut defs,
                "upgrades",
                &id,
                "time_per_level",
                upgrade.time_per_level(),
            );
            let effects: Vec<UpgradeEffectValue> = upgrade
                .active_effects()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|effect| {
                    let id = effect.code?;
                    Some(UpgradeEffectValue {
                        effect: id,
                        base: effect.base,
                        per_level: effect.delta,
                    })
                })
                .collect();
            let requires: Vec<String> = self.requirements(&id);
            let requires_count = self
                .localizer()
                .raw(&id, "Requirescount")
                .and_then(|value| value.trim().parse::<i32>().ok());
            defs.upgrades.push(UpgradeDef {
                id: id.clone(),
                name: upgrade.name(),
                class: upgrade.class().map(str::to_string),
                race: upgrade.race().map(str::to_string),
                max_level: upgrade.max_level().ok().flatten(),
                used: upgrade.used().unwrap_or(false),
                inherit: upgrade.inherit().unwrap_or(false),
                gold_base: upgrade.gold_base().ok().flatten(),
                gold_per_level: upgrade.gold_per_level().ok().flatten(),
                lumber_base: upgrade.lumber_base().ok().flatten(),
                lumber_per_level: upgrade.lumber_per_level().ok().flatten(),
                time_base: upgrade.time_base().ok().flatten(),
                time_per_level: upgrade.time_per_level().ok().flatten(),
                effects,
                requires,
                requires_count,
                units: upgrade_units.remove(&id).unwrap_or_default(),
                unlocks: unlock_abilities.remove(&id).unwrap_or_default(),
            });
        }

        if let Some(table) = self.meta(MetaKind::UpgradeEffect) {
            for field in table.fields() {
                let id = field.id().to_string();
                if id.is_empty() {
                    continue;
                }
                defs.effects.push(UpgradeEffectDef {
                    id,
                    display_name: field.raw_column("displayName").map(str::to_string),
                    data_type: field.raw_column("dataType").map(str::to_string),
                    effect_type: field.raw_column("type").map(str::to_string),
                    min: field
                        .raw_column("minVal")
                        .and_then(|value| value.trim().parse().ok()),
                    max: field
                        .raw_column("maxVal")
                        .and_then(|value| value.trim().parse().ok()),
                });
            }
        }

        // Ability ids and their icons, for the buff -> ability link.
        let mut ability_ids: Vec<String> = Vec::new();
        let mut icon_owners: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for ability in self.abilities() {
            let id = ability.id_str();
            if let Some(icon) = self.localizer().raw(&id, "Art") {
                icon_owners.entry(icon).or_default().push(id.clone());
            }
            ability_ids.push(id);
        }
        for destructable in self.destructables() {
            let id = destructable.id_str();
            push_err(&mut defs, "destructables", &id, "hp", destructable.hp());
            push_err(&mut defs, "destructables", &id, "walkable", destructable.walkable().map(Some));
            push_err(&mut defs, "destructables", &id, "on_water", destructable.on_water().map(Some));
            defs.destructables.push(DestructableDef {
                id: id.clone(),
                name: destructable.name().map(str::to_string),
                category: destructable.category().map(str::to_string),
                model: destructable.model_file(),
                texture: destructable.texture().map(str::to_string),
                path_texture: destructable.path_texture().map(str::to_string),
                hp: destructable.hp().ok().flatten(),
                armor_type: destructable.armor_type().map(str::to_string),
                walkable: destructable.walkable().unwrap_or(false),
                on_water: destructable.on_water().unwrap_or(false),
            });
        }

        for doodad in self.doodads() {
            let id = doodad.id_str();
            push_err(&mut defs, "doodads", &id, "walkable", doodad.walkable().map(Some));
            push_err(&mut defs, "doodads", &id, "on_water", doodad.on_water().map(Some));
            defs.doodads.push(DoodadDef {
                id: id.clone(),
                name: doodad.name().map(str::to_string),
                category: doodad.category().map(str::to_string),
                model: doodad.model_file(),
                path_texture: doodad.path_texture().map(str::to_string),
                walkable: doodad.walkable().unwrap_or(false),
                on_water: doodad.on_water().unwrap_or(false),
                animate_in_fog: doodad.animate_in_fog().unwrap_or(false),
            });
        }

        for id in self.localizer().sections_with_key("Buffart") {
            let art = self.localizer().raw(&id, "Buffart");
            let tail = &id[id.char_indices().nth(1).map(|(index, _)| index).unwrap_or(id.len())..];
            let mut ability = ability_ids
                .iter()
                .find(|candidate| {
                    candidate.len() == id.len()
                        && candidate.get(1..) == Some(tail)
                })
                .cloned();
            let mut matched_by = Some("id");
            if ability.is_none() {
                ability = art
                    .as_ref()
                    .and_then(|art| icon_owners.get(art))
                    .filter(|owners| owners.len() == 1)
                    .map(|owners| owners[0].clone());
                matched_by = Some("icon");
            }
            if ability.is_none() {
                matched_by = None;
            }
            defs.buffs.push(BuffDef {
                art,
                tip: self.localizer().raw(&id, "Bufftip"),
                ability,
                matched_by,
                id,
            });
        }

        defs
    }
}
