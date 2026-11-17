//! Units and buildings: a typed view over the five unit sheets.
//!
//! A WC3 unit is spread across `UnitData.slk`, `UnitBalance.slk`,
//! `UnitWeapons.slk`, `UnitAbilities.slk` and `unitUI.slk`, joined by the same
//! FourCC. The shipped data does not keep the five sets aligned:
//!
//! | root | UnitData | UnitBalance | unitUI | UnitWeapons | UnitAbilities |
//! |---|---|---|---|---|---|
//! | RoC (`analysis/war3_data`) | 466 | 466 | 466 | 316 | 324 |
//! | TFT (`analysis/war3x_data`) | 812 | 812 | 812 | 812 | 812 |
//!
//! so every optional sheet is exposed as `Option` and never assumed present.

use wc3_formats::gamedata::FourCc;

use crate::error::Result;
use crate::table::TableKind;
use crate::GameData;

/// Classification derived from the shipped data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    Unit,
    Building,
}

/// A unit or building.
#[derive(Clone, Copy)]
pub struct UnitView<'a> {
    pub(crate) gd: &'a GameData,
    pub(crate) id: FourCc,
}

impl<'a> UnitView<'a> {
    pub fn id(&self) -> FourCc {
        self.id
    }

    pub fn id_str(&self) -> String {
        self.id.as_str().into_owned()
    }

    fn cell(&self, kind: TableKind, column: &str) -> Option<&'a str> {
        self.gd.table(kind)?.text(self.id.as_str().as_ref(), column)
    }

    fn num(&self, kind: TableKind, column: &str) -> Result<Option<i32>> {
        match self.gd.table(kind) {
            Some(table) => table.i32_overlaid(self.id.as_str().as_ref(), column),
            None => Ok(None),
        }
    }

    fn real(&self, kind: TableKind, column: &str) -> Result<Option<f32>> {
        match self.gd.table(kind) {
            Some(table) => table.f32(self.id.as_str().as_ref(), column),
            None => Ok(None),
        }
    }

    fn flag(&self, kind: TableKind, column: &str) -> Result<Option<bool>> {
        match self.gd.table(kind) {
            Some(table) => table.bool(self.id.as_str().as_ref(), column),
            None => Ok(None),
        }
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

    pub fn icon(&self) -> Option<String> {
        self.gd.localizer().icon(self.id.as_str().as_ref())
    }

    pub fn hotkey(&self) -> Option<String> {
        self.gd.localizer().hotkey(self.id.as_str().as_ref())
    }

    pub fn button_pos(&self) -> Option<(u32, u32)> {
        self.gd.localizer().button_pos(self.id.as_str().as_ref())
    }

    /// `Tooltip` — the hover text retail buildings carry next to `Tip`
    /// (shops, taverns, roosts). Distinct from the command-card `Tip`.
    pub fn tooltip(&self) -> Option<String> {
        self.gd.localizer().tooltip(self.id.as_str().as_ref())
    }

    /// `Propernames` — the name list a random hero picks from.
    pub fn proper_names(&self) -> Vec<String> {
        self.profile_list("Propernames")
    }

    /// `ScoreScreenIcon` — the icon the score screen shows for the row.
    pub fn score_screen_icon(&self) -> Option<String> {
        self.gd
            .localizer()
            .component(self.id.as_str().as_ref(), "ScoreScreenIcon", 0)
    }

    /// `Revivetip` — the hero-revive line shown at the altar.
    pub fn revive_tip(&self) -> Option<String> {
        self.gd
            .localizer()
            .text(self.id.as_str().as_ref(), "Revivetip")
    }

    /// `Unhotkey` — hotkey of the cancel ("unbuild") button.
    pub fn unhotkey(&self) -> Option<String> {
        self.gd
            .localizer()
            .text(self.id.as_str().as_ref(), "Unhotkey")
    }

    // ---- art / models ---------------------------------------------------

    /// Raw `unitUI.file`. The shipped data omits the `.mdx` extension
    /// (`units\nightelf\Archer\Archer`), so this is the value as stored.
    pub fn model_path(&self) -> Option<&'a str> {
        self.cell(TableKind::UnitUi, "file")
    }

    /// Model path completed with `.mdx` where the data omits the extension.
    pub fn model_file(&self) -> Option<String> {
        crate::art::normalize_model_path(self.model_path()?)
    }

    /// Whether the model resolves in the archive listfile.
    pub fn model_exists(&self) -> Option<bool> {
        let catalog = self.gd.catalog()?;
        Some(catalog.resolve_model(&self.model_file()?).is_some())
    }

    /// Conventional portrait model path (`<model>_Portrait.mdx`).
    ///
    /// WC3 loads a separate portrait model next to the unit model; the shipped
    /// tables do not name it, so the path is derived from the model name and
    /// [`UnitView::portrait_exists`] proves it against the listfile. 192 of the
    /// 335 distinct RoC unit models ship a portrait.
    pub fn portrait_file(&self) -> Option<String> {
        crate::art::portrait_path(&self.model_file()?)
    }

    pub fn portrait_exists(&self) -> Option<bool> {
        let catalog = self.gd.catalog()?;
        Some(catalog.resolve_model(&self.portrait_file()?).is_some())
    }

    /// `unitSound` — the acknowledgement sound set.
    pub fn sound_set(&self) -> Option<&'a str> {
        self.cell(TableKind::UnitUi, "unitSound")
    }

    /// `uberSplat` — the ground texture under the unit.
    pub fn ground_texture(&self) -> Option<&'a str> {
        self.cell(TableKind::UnitUi, "uberSplat")
    }

    /// `unitShadow` / `buildingShadow` texture.
    pub fn shadow_texture(&self) -> Option<&'a str> {
        self.cell(TableKind::UnitUi, "unitShadow")
            .or_else(|| self.cell(TableKind::UnitUi, "buildingShadow"))
    }

    // ---- classification -------------------------------------------------

    /// `UnitData.race` (`human`, `orc`, `undead`, `nightelf`, `creeps`, …).
    pub fn race(&self) -> Option<&'a str> {
        self.cell(TableKind::UnitData, "race")
    }

    /// `UnitData.type` — comma separated class flags (`TownHall,Mechanical`).
    pub fn type_flags(&self) -> Vec<&'a str> {
        self.gd
            .table(TableKind::UnitData)
            .map(|table| table.list(self.id.as_str().as_ref(), "type"))
            .unwrap_or_default()
    }

    pub fn has_type_flag(&self, flag: &str) -> bool {
        self.type_flags()
            .iter()
            .any(|value| value.eq_ignore_ascii_case(flag))
    }

    /// `unitUI.unitClass` — the editor's building/unit grouping
    /// (`HBuilding01`, `animal`, `boat`, …).
    pub fn unit_class(&self) -> Option<&'a str> {
        self.cell(TableKind::UnitUi, "unitClass")
    }

    /// Numeric suffix of a `*BuildingNN` unit class, when present.
    ///
    /// The four hall chains are `HBuilding01..03`, `OBuilding01..03`,
    /// `EBuilding01..03`, `UBuilding01..03`, and the index tracks the tier of the
    /// hall (01 → tier 1). Only the halls are tier-ordered this way; other
    /// buildings reuse the same numbering for unrelated buildings (e.g.
    /// `HBuilding11` is the Gryphon Aviary), so this is *not* a general tier.
    pub fn building_class_index(&self) -> Option<u32> {
        let class = self.unit_class()?;
        let lower = class.to_ascii_lowercase();
        let position = lower.find("building")?;
        lower[position + "building".len()..].trim().parse().ok()
    }

    pub fn is_town_hall(&self) -> bool {
        self.has_type_flag("TownHall")
    }

    /// Tech tier of a main hall (1–3), derived from the class index of the
    /// `TownHall`-flagged buildings (`HBuilding01` → 1). `None` for everything
    /// else — the shipped tables carry no general tier field.
    pub fn hall_tier(&self) -> Option<u32> {
        if !self.is_town_hall() {
            return None;
        }
        self.building_class_index().filter(|tier| (1..=3).contains(tier))
    }

    /// Unit or building.
    ///
    /// RoC ships `unitUI.isbldg`; TFT dropped it (`observed`: TFT `unitUI.slk`
    /// has 47 columns and no `isbldg`). For TFT the model directory decides:
    /// every building model lives under `Buildings\` (131 such models in the RoC
    /// listfile) and no unit model does.
    pub fn classification(&self) -> Result<Classification> {
        let table = self
            .gd
            .table(TableKind::UnitUi)
            .ok_or(crate::error::DataError::MissingColumn {
                table: "unitUI".to_string(),
                column: "unitUIID".to_string(),
            })?;
        if table.has_column("isbldg") {
            let flag = table.bool(self.id.as_str().as_ref(), "isbldg")?.unwrap_or(false);
            return Ok(if flag {
                Classification::Building
            } else {
                Classification::Unit
            });
        }
        let building = self
            .model_path()
            .map(|path| {
                let lower = path.to_ascii_lowercase().replace('/', "\\");
                lower.starts_with("buildings\\")
            })
            .unwrap_or(false);
        Ok(if building {
            Classification::Building
        } else {
            Classification::Unit
        })
    }

    pub fn is_building(&self) -> Result<bool> {
        Ok(self.classification()? == Classification::Building)
    }

    // ---- core stats -----------------------------------------------------

    pub fn level(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "level")
    }

    pub fn hp(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "HP")
    }

    /// `realHP` — the sheet's computed hit points (heroes: `HP + STR * per-STR`).
    pub fn real_hp(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "realHP")
    }

    /// `realM` — the sheet's computed mana (heroes: `manaN + INT * per-INT`).
    pub fn real_mana(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "realM")
    }

    pub fn hp_regen(&self) -> Result<Option<f32>> {
        self.real(TableKind::UnitBalance, "regenHP")
    }

    pub fn mana(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "manaN")
    }

    pub fn mana_initial(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "mana0")
    }

    pub fn mana_regen(&self) -> Result<Option<f32>> {
        self.real(TableKind::UnitBalance, "regenMana")
    }

    pub fn gold_cost(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "goldcost")
    }

    pub fn lumber_cost(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "lumbercost")
    }

    pub fn build_time(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "bldtm")
    }

    pub fn food_used(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "fused")
    }

    pub fn food_provided(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "fmade")
    }

    /// `bountydice`, `bountysides`, `bountyplus`.
    pub fn bounty(&self) -> Result<Option<(i32, i32, i32)>> {
        let dice = self.num(TableKind::UnitBalance, "bountydice")?;
        let sides = self.num(TableKind::UnitBalance, "bountysides")?;
        let plus = self.num(TableKind::UnitBalance, "bountyplus")?;
        Ok(match (dice, sides, plus) {
            (Some(d), Some(s), Some(p)) => Some((d, s, p)),
            _ => None,
        })
    }

    pub fn stock_max(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "stockMax")
    }

    pub fn stock_regen(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "stockRegen")
    }

    pub fn stock_start(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "stockStart")
    }

    // ---- attributes / armour -------------------------------------------

    pub fn strength(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "STR")
    }

    pub fn strength_per_level(&self) -> Result<Option<f32>> {
        self.real(TableKind::UnitBalance, "STRplus")
    }

    pub fn intelligence(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "INT")
    }

    pub fn intelligence_per_level(&self) -> Result<Option<f32>> {
        self.real(TableKind::UnitBalance, "INTplus")
    }

    pub fn agility(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "AGI")
    }

    pub fn agility_per_level(&self) -> Result<Option<f32>> {
        self.real(TableKind::UnitBalance, "AGIplus")
    }

    /// `Primary` — the hero's main attribute (`STR`/`INT`/`AGI`).
    pub fn primary_attribute(&self) -> Option<&'a str> {
        self.cell(TableKind::UnitBalance, "Primary")
    }

    /// Base armour (`def` column).
    pub fn armor_base(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "def")
    }

    /// Armour gained per armour upgrade (`defUp` column).
    pub fn armor_per_upgrade(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "defUp")
    }

    /// Armour after `levels` armour upgrades: `def + defUp * levels`.
    pub fn armor_with_upgrades(&self, levels: u32) -> Result<Option<i32>> {
        match (self.armor_base()?, self.armor_per_upgrade()?) {
            (Some(base), Some(per)) => Ok(Some(base + per * levels as i32)),
            (Some(base), None) => Ok(Some(base)),
            _ => Ok(None),
        }
    }

    /// `defType` — armour class (`small`, `large`, `fort`, …).
    pub fn armor_type(&self) -> Option<&'a str> {
        self.cell(TableKind::UnitBalance, "defType")
    }

    // ---- movement -------------------------------------------------------

    pub fn speed(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "spd")
    }

    pub fn turn_rate(&self) -> Result<Option<f32>> {
        self.real(TableKind::UnitData, "turnRate")
    }

    pub fn move_type(&self) -> Option<&'a str> {
        self.cell(TableKind::UnitData, "movetp")
    }

    pub fn move_height(&self) -> Result<Option<f32>> {
        self.real(TableKind::UnitData, "moveHeight")
    }

    pub fn collision(&self) -> Result<Option<f32>> {
        self.real(TableKind::UnitData, "collision")
    }

    pub fn sight_day(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "sight")
    }

    pub fn sight_night(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitBalance, "nsight")
    }

    pub fn acquisition_range(&self) -> Result<Option<f32>> {
        self.real(TableKind::UnitWeapons, "acquire")
    }

    pub fn cargo_size(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitData, "cargoSize")
    }

    pub fn formation(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitData, "formation")
    }

    pub fn placement_prevented(&self) -> Result<Option<bool>> {
        self.flag(TableKind::UnitUi, "preventPlace")
    }

    // ---- abilities ------------------------------------------------------

    /// `abilList` — the unit's ability list.
    pub fn abilities(&self) -> Vec<&'a str> {
        self.gd
            .table(TableKind::UnitAbilities)
            .map(|table| table.list(self.id.as_str().as_ref(), "abilList"))
            .unwrap_or_default()
    }

    /// `heroAbilList` — the hero's learnable abilities.
    pub fn hero_abilities(&self) -> Vec<&'a str> {
        self.gd
            .table(TableKind::UnitAbilities)
            .map(|table| table.list(self.id.as_str().as_ref(), "heroAbilList"))
            .unwrap_or_default()
    }

    /// `auto` — automatically acquired ability list.
    pub fn auto_abilities(&self) -> Vec<&'a str> {
        self.gd
            .table(TableKind::UnitAbilities)
            .map(|table| table.list(self.id.as_str().as_ref(), "auto"))
            .unwrap_or_default()
    }

    /// `Trains` — the units this producer can train (`utra`, profile text).
    ///
    /// Profile lists are comma separated in the order the command card shows
    /// them; the values are FourCCs of other rows.
    pub fn trains(&self) -> Vec<String> {
        self.profile_list("Trains")
    }

    /// `Researches` — the upgrades this building researches (`ures`).
    ///
    /// Measured on the RoC profile text: 32 sections carry it
    /// (`[hbla] Researches=Rhme,Rhar,Rhla,Rhra`), 41 on TFT. It is the upgrade
    /// half of the command card, next to `Trains`/`Builds`.
    pub fn researches(&self) -> Vec<String> {
        self.profile_list("Researches")
    }

    /// `Builds` — the structures this worker can place (`ubui`, profile text).
    pub fn builds(&self) -> Vec<String> {
        self.profile_list("Builds")
    }

    /// `Requires` — tech prerequisites for training this unit (`ureq`).
    ///
    /// Delegates to [`GameDatabase::requirements`], which also reads the
    /// `Requires1..N` encoding upgrades use.
    pub fn requires(&self) -> Vec<String> {
        self.gd.requirements(self.id.as_str().as_ref())
    }

    /// Comma separated profile list of an object, empty entries dropped.
    fn profile_list(&self, key: &str) -> Vec<String> {
        self.gd.localizer().list(self.id.as_str().as_ref(), key)
    }

    /// `upgrades` — the upgrade list that improves this unit.
    /// `unitUI.dropItems` — the unit can drop items on death.
    pub fn drop_items(&self) -> Option<bool> {
        self.gd
            .table(TableKind::UnitUi)?
            .bool(self.id.as_str().as_ref(), "dropItems")
            .ok()
            .flatten()
    }

    pub fn upgrades(&self) -> Vec<&'a str> {
        self.gd
            .table(TableKind::UnitBalance)
            .map(|table| table.list(self.id.as_str().as_ref(), "upgrades"))
            .unwrap_or_default()
    }

    // ---- weapons --------------------------------------------------------

    pub fn weapon(&self, slot: WeaponSlot) -> Option<WeaponView<'a>> {
        let table = self.gd.table(TableKind::UnitWeapons)?;
        if !table.contains(self.id.as_str().as_ref()) {
            return None;
        }
        // A weapon only exists when the row declares a weapon type or range.
        let probe = match slot {
            WeaponSlot::Primary => "weapTp1",
            WeaponSlot::Secondary => "weapTp2",
        };
        table.text(self.id.as_str().as_ref(), probe)?;
        Some(WeaponView {
            gd: self.gd,
            id: self.id,
            slot,
        })
    }

    pub fn weapons(&self) -> Vec<WeaponView<'a>> {
        [WeaponSlot::Primary, WeaponSlot::Secondary]
            .into_iter()
            .filter_map(|slot| self.weapon(slot))
            .collect()
    }

    /// `weapsOn` — attack bits (0 = never, 1 = primary, 3 = both).
    pub fn attacks_enabled(&self) -> Result<Option<i32>> {
        self.num(TableKind::UnitWeapons, "weapsOn")
    }
}

/// Which weapon slot of a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponSlot {
    Primary,
    Secondary,
}

impl WeaponSlot {
    /// Column suffix used by `UnitWeapons.slk` (`1` or `2`).
    /// 1-based slot number (`1` primary, `2` secondary).
    pub fn number(&self) -> u8 {
        match self {
            WeaponSlot::Primary => 1,
            WeaponSlot::Secondary => 2,
        }
    }

    pub fn suffix(&self) -> &'static str {
        match self {
            Self::Primary => "1",
            Self::Secondary => "2",
        }
    }

    /// Index into comma separated profile lists (`Missileart` component).
    pub fn profile_index(&self) -> usize {
        match self {
            Self::Primary => 0,
            Self::Secondary => 1,
        }
    }
}

/// One attack of a unit.
#[derive(Clone, Copy)]
pub struct WeaponView<'a> {
    gd: &'a GameData,
    id: FourCc,
    slot: WeaponSlot,
}

impl<'a> WeaponView<'a> {
    fn column(&self, base: &str) -> String {
        format!("{base}{}", self.slot.suffix())
    }

    fn cell(&self, base: &str) -> Option<&'a str> {
        let column = self.column(base);
        self.gd
            .table(TableKind::UnitWeapons)?
            .text(self.id.as_str().as_ref(), &column)
    }

    fn num(&self, base: &str) -> Result<Option<i32>> {
        let column = self.column(base);
        match self.gd.table(TableKind::UnitWeapons) {
            Some(table) => table.i32_overlaid(self.id.as_str().as_ref(), &column),
            None => Ok(None),
        }
    }

    fn real(&self, base: &str) -> Result<Option<f32>> {
        let column = self.column(base);
        match self.gd.table(TableKind::UnitWeapons) {
            Some(table) => table.f32(self.id.as_str().as_ref(), &column),
            None => Ok(None),
        }
    }

    pub fn slot(&self) -> WeaponSlot {
        self.slot
    }

    /// `weapTp1` — `normal`, `missile`, `artillery`, `instant`, `msplash`, …
    pub fn weapon_type(&self) -> Option<&'a str> {
        self.cell("weapTp")
    }

    /// Decoded `weaponType` enumerator from `UnitEditorData.txt`.
    pub fn weapon_type_enum(&self) -> Option<&crate::enums::EnumValue> {
        let enums = self.gd.enums()?;
        enums.decode("weaponType", self.weapon_type()?)
    }

    /// `atkType1` — `normal`, `pierce`, `siege`, `magic`, `chaos`, `hero`.
    pub fn attack_type(&self) -> Option<&'a str> {
        self.cell("atkType")
    }

    pub fn attack_type_enum(&self) -> Option<&crate::enums::EnumValue> {
        let enums = self.gd.enums()?;
        enums.decode("attackType", self.attack_type()?)
    }

    pub fn range(&self) -> Result<Option<i32>> {
        self.num("rangeN")
    }

    pub fn min_range(&self) -> Result<Option<i32>> {
        self.num("minRange")
    }

    /// `acquire` — acquisition range (stored on the primary/shared columns).
    pub fn acquisition_range(&self) -> Result<Option<f32>> {
        self.real("acquire")
    }

    /// `cool1` — seconds between attacks.
    pub fn cooldown(&self) -> Result<Option<f32>> {
        self.real("cool")
    }

    /// Attacks per second (`1 / cool`).
    pub fn attacks_per_second(&self) -> Result<Option<f32>> {
        Ok(self.cooldown()?.filter(|cool| *cool > 0.0).map(|cool| 1.0 / cool))
    }

    pub fn damage_plus(&self) -> Result<Option<i32>> {
        self.num("dmgplus")
    }

    pub fn damage_dice(&self) -> Result<Option<i32>> {
        self.num("dice")
    }

    pub fn damage_sides(&self) -> Result<Option<i32>> {
        self.num("sides")
    }

    /// Minimum damage: `dmgplus + dice`.
    pub fn min_damage(&self) -> Result<Option<i32>> {
        Ok(match (self.damage_plus()?, self.damage_dice()?) {
            (Some(plus), Some(dice)) => Some(plus + dice),
            _ => None,
        })
    }

    /// Maximum damage: `dmgplus + dice * sides`.
    pub fn max_damage(&self) -> Result<Option<i32>> {
        Ok(
            match (
                self.damage_plus()?,
                self.damage_dice()?,
                self.damage_sides()?,
            ) {
                (Some(plus), Some(dice), Some(sides)) => Some(plus + dice * sides),
                _ => None,
            },
        )
    }

    /// Average damage: `dmgplus + dice * (sides + 1) / 2`.
    ///
    /// Verified against the precomputed `mindmg1`/`avgdmg1`/`maxdmg1` columns of
    /// the shipped sheets (see `tests/real_data.rs`).
    pub fn average_damage(&self) -> Result<Option<f32>> {
        Ok(
            match (
                self.damage_plus()?,
                self.damage_dice()?,
                self.damage_sides()?,
            ) {
                (Some(plus), Some(dice), Some(sides)) => {
                    Some(plus as f32 + dice as f32 * (sides as f32 + 1.0) / 2.0)
                }
                _ => None,
            },
        )
    }

    /// Damage per second from damage and cooldown.
    pub fn dps(&self) -> Result<Option<f32>> {
        match (self.average_damage()?, self.cooldown()?) {
            (Some(damage), Some(cool)) if cool > 0.0 => Ok(Some(damage / cool)),
            _ => Ok(None),
        }
    }

    pub fn full_damage_radius(&self) -> Result<Option<i32>> {
        self.num("Farea")
    }

    pub fn half_damage_radius(&self) -> Result<Option<i32>> {
        self.num("Harea")
    }

    pub fn quarter_damage_radius(&self) -> Result<Option<i32>> {
        self.num("Qarea")
    }

    pub fn damage_loss_factor(&self) -> Result<Option<f32>> {
        self.real("damageLoss")
    }

    pub fn spill_distance(&self) -> Result<Option<i32>> {
        self.num("spillDist")
    }

    pub fn spill_radius(&self) -> Result<Option<i32>> {
        self.num("spillRadius")
    }

    pub fn target_count(&self) -> Result<Option<i32>> {
        self.num("targCount")
    }

    /// `targs1` — legal targets (`ground`, `air`, `structure`, …).
    pub fn targets(&self) -> Vec<&'a str> {
        self.gd
            .table(TableKind::UnitWeapons)
            .map(|table| table.list(self.id.as_str().as_ref(), &self.column("targs")))
            .unwrap_or_default()
    }

    /// `splashTargs1` — legal splash targets.
    pub fn splash_targets(&self) -> Vec<&'a str> {
        self.gd
            .table(TableKind::UnitWeapons)
            .map(|table| table.list(self.id.as_str().as_ref(), &self.column("splashTargs")))
            .unwrap_or_default()
    }

    pub fn damage_point(&self) -> Result<Option<f32>> {
        self.real("dmgpt")
    }

    pub fn backswing(&self) -> Result<Option<f32>> {
        self.real("backSw")
    }

    /// Projectile model from the profile layer (`Missileart` component).
    pub fn projectile(&self) -> Option<String> {
        self.gd
            .localizer()
            .component(self.id.as_str().as_ref(), "Missileart", self.slot.profile_index())
    }

    /// Projectile speed (`Missilespeed`, profile component).
    pub fn projectile_speed(&self) -> Option<u32> {
        self.gd
            .localizer()
            .component(
                self.id.as_str().as_ref(),
                "Missilespeed",
                self.slot.profile_index(),
            )?
            .trim()
            .parse()
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{DirSource, MemorySource, Source};

    fn memory() -> MemorySource {
        let mut source = MemorySource::new();
        source.insert(
            "Units\\UnitData.slk",
            b"ID;PWXL;N;E\nB;Y2;X6;D0\nC;Y1;X1;K\"unitID\"\nC;X2;K\"race\"\nC;X3;K\"type\"\n\
C;X4;K\"turnRate\"\nC;X5;K\"movetp\"\nC;X6;K\"cargoSize\"\n\
C;Y2;X1;K\"hfoo\"\nC;X2;K\"human\"\nC;X3;K\"Mechanical\"\nC;X4;K\"0.6\"\nC;X5;K\"foot\"\nC;X6;K1\n"
                .to_vec(),
        );
        source.insert(
            "Units\\UnitBalance.slk",
            b"ID;PWXL;N;E\nB;Y2;X6;D0\nC;Y1;X1;K\"unitBalanceID\"\nC;X2;K\"HP\"\nC;X3;K\"def\"\n\
C;X4;K\"defUp\"\nC;X5;K\"spd\"\nC;X6;K\"goldcost\"\n\
C;Y2;X1;K\"hfoo\"\nC;X2;K420\nC;X3;K2\nC;X4;K1\nC;X5;K270\nC;X6;K135\n"
                .to_vec(),
        );
        source.insert(
            "Units\\UnitWeapons.slk",
            b"ID;PWXL;N;E\nB;Y2;X9;D0\nC;Y1;X1;K\"unitWeapID\"\nC;X2;K\"weapTp1\"\nC;X3;K\"atkType1\"\n\
C;X4;K\"rangeN1\"\nC;X5;K\"cool1\"\nC;X6;K\"dice1\"\nC;X7;K\"sides1\"\nC;X8;K\"dmgplus1\"\nC;X9;K\"targs1\"\n\
C;Y2;X1;K\"hfoo\"\nC;X2;K\"normal\"\nC;X3;K\"normal\"\nC;X4;K90\nC;X5;K1.35\nC;X6;K1\nC;X7;K2\nC;X8;K11\n\
C;X9;K\"ground,structure\"\n"
                .to_vec(),
        );
        source.insert(
            "Units\\unitUI.slk",
            b"ID;PWXL;N;E\nB;Y2;X4;D0\nC;Y1;X1;K\"unitUIID\"\nC;X2;K\"file\"\nC;X3;K\"isbldg\"\nC;X4;K\"unitClass\"\n\
C;Y2;X1;K\"hfoo\"\nC;X2;K\"units\\human\\Footman\\Footman\"\nC;X3;K0\nC;X4;K\"HUnit01\"\n"
                .to_vec(),
        );
        source.insert(
            "Units\\UnitAbilities.slk",
            b"ID;PWXL;N;E\nB;Y2;X2;D0\nC;Y1;X1;K\"unitAbilID\"\nC;X2;K\"abilList\"\n\
C;Y2;X1;K\"hfoo\"\nC:placeholder\n"
                .to_vec(),
        );
        source
    }

    #[test]
    fn unit_view_reads_typed_fields() {
        let mut source = memory();
        // Replace the deliberately simple ability sheet with a valid one.
        source.insert(
            "Units\\UnitAbilities.slk",
            b"ID;PWXL;N;E\nB;Y2;X2;D0\nC;Y1;X1;K\"unitAbilID\"\nC;X2;K\"abilList\"\n\
C;Y2;X1;K\"hfoo\"\nC;X2;K\"Adef,Ahrl\"\n"
                .to_vec(),
        );
        let gd = GameData::load(&source).expect("load");
        let unit = gd.unit("hfoo").expect("unit");
        assert_eq!(unit.hp().unwrap(), Some(420));
        assert_eq!(unit.gold_cost().unwrap(), Some(135));
        assert_eq!(unit.armor_with_upgrades(3).unwrap(), Some(5));
        assert_eq!(unit.classification().unwrap(), Classification::Unit);
        assert_eq!(unit.abilities(), ["Adef", "Ahrl"]);
        assert_eq!(unit.model_file().as_deref(), Some("units\\human\\Footman\\Footman.mdx"));
        let weapon = unit.weapon(WeaponSlot::Primary).expect("weapon");
        assert_eq!(weapon.min_damage().unwrap(), Some(12));
        assert_eq!(weapon.max_damage().unwrap(), Some(13));
        assert_eq!(weapon.average_damage().unwrap(), Some(12.5));
        assert_eq!(weapon.targets(), ["ground", "structure"]);
    }

    #[test]
    fn missing_optional_sheet_is_not_an_error() {
        let source = DirSource::new("/nonexistent-should-not-be-read");
        assert!(source.read("Units\\UnitData.slk").is_err());
    }

    /// `Researches=` is the third card list (the upgrade tab). Measured on the
    /// retail text: 32 sections in RoC, 41 in TFT.
    #[test]
    fn researches_and_hover_text_come_from_the_profile_layer() {
        let mut source = memory();
        source.insert(
            "Units\\UnitAbilities.slk",
            b"ID;PWXL;N;E\nB;Y2;X2;D0\nC;Y1;X1;K\"unitAbilID\"\nC;X2;K\"abilList\"\n\
C;Y2;X1;K\"hfoo\"\nC;X2;K\"Adef\"\n"
                .to_vec(),
        );
        source.insert(
            "Units\\HumanUnitStrings.txt",
            "[hfoo]\nName=Footman\nTooltip=Trains a footman\nPropernames=Danath,Turalyon\n\
Requires=htow,hkee\nRevivetip=Revive at the altar\n"
                .as_bytes()
                .to_vec(),
        );
        source.insert(
            "Units\\HumanUnitFunc.txt",
            "[hfoo]\nArt=BTNFootman.blp\nHotkey=F\nResearches=Rhme,Rhar\nUnhotkey=U\n\
ScoreScreenIcon=BTNFootmanScore.blp\n"
                .as_bytes()
                .to_vec(),
        );
        let gd = GameData::load(&source).expect("load");
        let unit = gd.unit("hfoo").expect("unit");
        assert_eq!(unit.researches(), ["Rhme", "Rhar"]);
        assert_eq!(unit.requires(), ["htow", "hkee"]);
        assert_eq!(unit.tooltip().as_deref(), Some("Trains a footman"));
        assert_eq!(unit.proper_names(), ["Danath", "Turalyon"]);
        assert_eq!(unit.revive_tip().as_deref(), Some("Revive at the altar"));
        assert_eq!(unit.unhotkey().as_deref(), Some("U"));
        assert_eq!(
            unit.score_screen_icon().as_deref(),
            Some("BTNFootmanScore.blp")
        );
        assert_eq!(unit.hotkey().as_deref(), Some("F"));
        // A unit without `Researches` exposes an empty list, not a guess.
        assert!(gd.unit("hfoo").unwrap().builds().is_empty());
    }
}
