//! One lookup path: object id → name / tooltip / icon / model / stats.
//!
//! The game layer talks to the database through [`GameDatabase::object`] so the
//! HUD, shop, tooltip and score-screen code never has to know which of the ten
//! tables an id came from. Each field is filled from the typed view of the
//! owning table; a missing value stays `None` (the shipped data does not define
//! every field for every object).

use wc3_formats::gamedata::ObjectKind;

use crate::GameDatabase;

/// Stats shared by the object kinds, filled from the typed views.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ObjectStats {
    /// Unit/item/ability level column.
    pub level: Option<i32>,
    pub hp: Option<i32>,
    pub mana: Option<i32>,
    pub armor: Option<i32>,
    /// Armour gained per armour upgrade (units only).
    pub armor_per_upgrade: Option<i32>,
    pub gold_cost: Option<i32>,
    pub lumber_cost: Option<i32>,
    pub build_time: Option<i32>,
    pub speed: Option<i32>,
    pub food_used: Option<i32>,
    pub food_provided: Option<i32>,
    /// Item charges (`uses`); `None` means unlimited.
    pub charges: Option<i32>,
    /// Upgrade level count / ability column count.
    pub max_level: Option<i32>,
    /// Ability cost/cooldown/area/range of level 1.
    pub mana_cost: Option<i32>,
    pub cooldown: Option<f32>,
    pub area: Option<f32>,
    pub range: Option<f32>,
    /// Primary weapon damage of a unit.
    pub min_damage: Option<i32>,
    pub max_damage: Option<i32>,
    pub average_damage: Option<f32>,
    pub attacks_per_second: Option<f32>,
    pub targets: Vec<String>,
}

impl ObjectStats {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Everything the UI needs about one object id.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectInfo {
    pub id: String,
    pub kind: ObjectKind,
    pub name: Option<String>,
    pub tip: Option<String>,
    pub ubertip: Option<String>,
    pub description: Option<String>,
    /// Command-card icon (`Art`), first component for race-dependent lists.
    pub icon: Option<String>,
    pub hotkey: Option<String>,
    pub button_pos: Option<(u32, u32)>,
    /// Ground/unit model path (`unitUI.file`, `ItemData.file`, doodad models).
    pub model: Option<String>,
    /// `UnitData.race` / ability race (`human`, `orc`, `nightelf`, `undead`, …).
    pub race: Option<String>,
    /// `unitUI.unitClass` (`HBuilding01`, `animal`, …).
    pub class: Option<String>,
    /// `UnitData.type` class flags (`TownHall`, `Mechanical`, `Ancient`, …).
    pub type_flags: Vec<String>,
    pub stats: ObjectStats,
}

impl ObjectInfo {
    fn new(id: &str, kind: ObjectKind) -> Self {
        Self {
            id: id.to_string(),
            kind,
            name: None,
            tip: None,
            ubertip: None,
            description: None,
            icon: None,
            hotkey: None,
            button_pos: None,
            model: None,
            race: None,
            class: None,
            type_flags: Vec::new(),
            stats: ObjectStats::default(),
        }
    }
}

impl GameDatabase {
    /// Object kind by id, in table priority order.
    pub fn object_kind(&self, id: &str) -> Option<ObjectKind> {
        for kind in crate::table::TableKind::ALL {
            if let Some(table) = self.table(kind) {
                if table.contains(id) {
                    return Some(kind.object_kind());
                }
            }
        }
        self.buff(id).map(|_| ObjectKind::Buff)
    }

    /// Unified lookup used by the game: name, tooltip, icon, model and stats.
    ///
    /// Returns `None` for ids the data does not define. Values the shipped files
    /// leave empty stay `None`; nothing is synthesised.
    pub fn object(&self, id: &str) -> Option<ObjectInfo> {
        let mut info = self.object_direct(id)?;
        self.inherit_prototype_fields(&mut info);
        Some(info)
    }

    /// Fields a custom row does not set are the prototype's (`old_id`) values.
    ///
    /// A map file stores only the changes, so a custom unit's `Art` still comes
    /// from its base row's profile text even when the map wrote no icon field.
    /// Measured on `DotF_v1.4.w3x`: `U000` (base `Ucrl`) and `I000` (base
    /// `ankh`) resolve their name from the map's `.wts` and their icon only
    /// through this link. Inheritance is one level, matching the format's
    /// single `old_id`.
    fn inherit_prototype_fields(&self, info: &mut ObjectInfo) {
        let Some(prototype) = self.object_data().prototype_of(&info.id) else {
            return;
        };
        if prototype.eq_ignore_ascii_case(&info.id) {
            return;
        }
        let Some(base) = self.object_direct(&prototype) else {
            return;
        };
        if info.name.is_none() {
            info.name = base.name;
        }
        if info.tip.is_none() {
            info.tip = base.tip;
        }
        if info.ubertip.is_none() {
            info.ubertip = base.ubertip;
        }
        if info.description.is_none() {
            info.description = base.description;
        }
        if info.icon.is_none() {
            info.icon = base.icon;
        }
        if info.hotkey.is_none() {
            info.hotkey = base.hotkey;
        }
        if info.button_pos.is_none() {
            info.button_pos = base.button_pos;
        }
    }

    /// One row's own values, before prototype inheritance.
    fn object_direct(&self, id: &str) -> Option<ObjectInfo> {
        if let Some(unit) = self.unit(id) {
            let mut info = ObjectInfo::new(&unit.id_str(), ObjectKind::Unit);
            info.name = unit.name();
            info.tip = unit.tip();
            info.ubertip = unit.ubertip();
            info.icon = unit.icon();
            info.hotkey = unit.hotkey();
            info.button_pos = unit.button_pos();
            info.model = unit.model_file();
            info.race = unit.race().map(str::to_string);
            info.class = unit.unit_class().map(str::to_string);
            info.type_flags = unit.type_flags().iter().map(|flag| flag.to_string()).collect();
            let stats = &mut info.stats;
            stats.level = unit.level().ok().flatten();
            stats.hp = unit.hp().ok().flatten();
            stats.mana = unit.mana().ok().flatten();
            stats.armor = unit.armor_base().ok().flatten();
            stats.armor_per_upgrade = unit.armor_per_upgrade().ok().flatten();
            stats.gold_cost = unit.gold_cost().ok().flatten();
            stats.lumber_cost = unit.lumber_cost().ok().flatten();
            stats.build_time = unit.build_time().ok().flatten();
            stats.speed = unit.speed().ok().flatten();
            stats.food_used = unit.food_used().ok().flatten();
            stats.food_provided = unit.food_provided().ok().flatten();
            if let Some(weapon) = unit.weapon(crate::units::WeaponSlot::Primary) {
                stats.min_damage = weapon.min_damage().ok().flatten();
                stats.max_damage = weapon.max_damage().ok().flatten();
                stats.average_damage = weapon.average_damage().ok().flatten();
                stats.attacks_per_second = weapon.attacks_per_second().ok().flatten();
                stats.range = weapon.range().ok().flatten().map(|range| range as f32);
                stats.targets = weapon.targets().iter().map(|flag| flag.to_string()).collect();
            }
            return Some(info);
        }

        if let Some(ability) = self.ability(id) {
            let mut info = ObjectInfo::new(&ability.id_str(), ObjectKind::Ability);
            // Ability text inherits from the base `code` row: retail variants
            // (`AIa1`) carry only `Art`/`EditorSuffix`, the label lives on `AIab`.
            info.name = ability.resolved_name();
            // Ability tooltips are per-level lists: default to level 1.
            info.tip = ability.resolved_tip(1);
            info.ubertip = ability.resolved_tooltip(1);
            info.icon = ability.resolved_icon().or_else(|| ability.research_icon());
            info.hotkey = ability.resolved_hotkey();
            info.button_pos = ability.button_pos();
            info.race = ability.race().map(str::to_string);
            let stats = &mut info.stats;
            stats.level = ability.declared_levels().ok().flatten();
            stats.max_level = ability.declared_levels().ok().flatten();
            stats.mana_cost = ability.mana_cost(1).ok().flatten();
            stats.cooldown = ability.cooldown(1).ok().flatten();
            stats.area = ability.area(1).ok().flatten();
            stats.range = ability.range(1).ok().flatten();
            stats.targets = ability
                .target_flags()
                .iter()
                .map(|flag| flag.to_string())
                .collect();
            return Some(info);
        }

        if let Some(item) = self.item(id) {
            let mut info = ObjectInfo::new(&item.id_str(), ObjectKind::Item);
            info.name = item.name();
            info.tip = item.tip();
            info.ubertip = item.ubertip();
            info.description = item.description();
            info.icon = item.icon();
            info.hotkey = item.hotkey();
            info.button_pos = item.button_pos();
            info.model = item.model_file();
            let stats = &mut info.stats;
            stats.level = item.level().ok().flatten();
            stats.charges = item.charges().ok().flatten();
            stats.hp = item.hp().ok().flatten();
            stats.gold_cost = item.gold_cost().ok().flatten();
            stats.lumber_cost = item.lumber_cost().ok().flatten();
            return Some(info);
        }

        if let Some(upgrade) = self.upgrade(id) {
            let mut info = ObjectInfo::new(&upgrade.id_str(), ObjectKind::Upgrade);
            info.name = upgrade.name();
            info.tip = upgrade.tip();
            info.ubertip = upgrade.ubertip();
            info.description = upgrade.description();
            info.icon = upgrade.icon();
            info.hotkey = upgrade.hotkey();
            info.button_pos = upgrade.button_pos();
            let stats = &mut info.stats;
            stats.max_level = upgrade.max_level().ok().flatten();
            stats.gold_cost = upgrade.gold_cost(1).ok().flatten();
            stats.lumber_cost = upgrade.lumber_cost(1).ok().flatten();
            stats.build_time = upgrade.research_time(1).ok().flatten();
            return Some(info);
        }

        if let Some(object) = self.destructable(id) {
            let mut info = ObjectInfo::new(&object.id_str(), ObjectKind::Destructable);
            // Destructables carry their name in the SLK, not the profile text.
            info.name = object.name().map(str::to_string);
            info.model = object.model_file();
            info.stats.hp = object.hp().ok().flatten();
            return Some(info);
        }

        if let Some(object) = self.doodad(id) {
            let mut info = ObjectInfo::new(&object.id_str(), ObjectKind::Doodad);
            info.name = object.name().map(str::to_string);
            info.model = object.model_file();
            return Some(info);
        }

        // Buffs have no shipped sheet: the profile section carries the name and
        // icon, and a `war3map.w3b` entry overrides any text field.
        if let Some(buff) = self.buff(id) {
            let mut info = ObjectInfo::new(&buff.id, ObjectKind::Buff);
            info.name = buff.name;
            info.tip = buff.tip;
            info.ubertip = buff.ubertip;
            info.icon = buff.icon;
            return Some(info);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemorySource;

    fn database() -> GameDatabase {
        let mut source = MemorySource::new();
        source.insert(
            "Units\\UnitData.slk",
            b"ID;PWXL;N;E\nB;Y2;X4;D0\nC;Y1;X1;K\"unitID\"\nC;X2;K\"race\"\nC;X3;K\"type\"\nC;X4;K\"movetp\"\n\
C;Y2;X1;K\"hfoo\"\nC;X2;K\"human\"\nC;X3;K\"_\"\nC;X4;K\"foot\"\n"
                .to_vec(),
        );
        source.insert(
            "Units\\UnitBalance.slk",
            b"ID;PWXL;N;E\nB;Y2;X4;D0\nC;Y1;X1;K\"unitBalanceID\"\nC;X2;K\"HP\"\nC;X3;K\"goldcost\"\nC;X4;K\"level\"\n\
C;Y2;X1;K\"hfoo\"\nC;X2;K420\nC;X3;K160\nC;X4;K2\n"
                .to_vec(),
        );
        source.insert(
            "Units\\unitUI.slk",
            b"ID;PWXL;N;E\nB;Y2;X2;D0\nC;Y1;X1;K\"unitUIID\"\nC;X2;K\"file\"\n\
C;Y2;X1;K\"hfoo\"\nC;X2;K\"units\\human\\Footman\\Footman\"\n"
                .to_vec(),
        );
        source.insert(
            "Units\\HumanUnitStrings.txt",
            b"[hfoo]\nName=Footman\nTip=Melee\nUbertip=Basic melee unit\n".to_vec(),
        );
        source.insert(
            "Units\\HumanUnitFunc.txt",
            b"[hfoo]\nArt=ReplaceableTextures\\CommandButtons\\BTNFootman.blp\nHotkey=F\nButtonpos=0,0\n"
                .to_vec(),
        );
        GameDatabase::load(&source).expect("load")
    }

    #[test]
    fn unified_lookup_reports_name_icon_and_stats() {
        let data = database();
        let info = data.object("hfoo").expect("hfoo");
        assert_eq!(info.kind, ObjectKind::Unit);
        assert_eq!(info.name.as_deref(), Some("Footman"));
        assert_eq!(
            info.icon.as_deref(),
            Some("ReplaceableTextures\\CommandButtons\\BTNFootman.blp")
        );
        assert_eq!(info.hotkey.as_deref(), Some("F"));
        assert_eq!(info.stats.hp, Some(420));
        assert_eq!(info.stats.gold_cost, Some(160));
        assert_eq!(info.stats.level, Some(2));
        assert_eq!(
            info.model.as_deref(),
            Some("units\\human\\Footman\\Footman.mdx")
        );
        assert_eq!(data.object_kind("hfoo"), Some(ObjectKind::Unit));
        assert!(data.object("nope").is_none());
    }
}
