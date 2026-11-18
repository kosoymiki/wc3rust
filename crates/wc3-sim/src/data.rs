//! Data-driven unit, weapon, armor and damage tables.
//!
//! Stats come from the Warcraft III data files (`Units/UnitBalance.slk`,
//! `Units/UnitWeapons.slk`, `Units/UnitData.slk`, `Units/MiscGame.txt`) via the
//! generator in `analysis/agent-sim/gen_roster.py`.  The generated roster lives
//! in `roster_gen.rs`; this module owns the shared vocabulary and the combat
//! math so that later WC3 table imports only have to produce `UnitDef` values.
//!
//! The simulations uses *integer permille* multipliers for every combat table
//! lookup: floats would risk cross-platform bit differences between replays.

use glam::Vec2;

use crate::abilities::Spell;

/// Hero primary attribute (`Primary` column of UnitBalance.slk).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HeroAttribute {
    Strength,
    Agility,
    Intelligence,
    None,
}

impl HeroAttribute {
    pub fn name(self) -> &'static str {
        match self {
            Self::Strength => "Strength",
            Self::Agility => "Agility",
            Self::Intelligence => "Intelligence",
            Self::None => "None",
        }
    }
}

/// Player-visible render/logic bucket.  Kept to the four kinds the game crate
/// already renders; the full roster stores its own `kind` mapping and model
/// path, so new races reuse the existing renderer until the data layer lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnitKind {
    Footman,
    Archer,
    Peasant,
    TownHall,
}

/// WC3 armor classes.  Declaration order *is* the column order used by the
/// original `DamageBonus*` tables (SMALL, MEDIUM, LARGE, FORT, NORMAL, HERO,
/// DIVINE, NONE).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArmorType {
    Light,
    Medium,
    Heavy,
    Fortified,
    Unarmored,
    Hero,
    Divine,
    None,
}

impl ArmorType {
    pub const COUNT: usize = 8;

    pub fn index(self) -> usize {
        self as usize
    }

    /// Inverse of [`ArmorType::index`]; used by the `rart` upgrade effect.
    pub fn from_index(index: i32) -> Self {
        match index {
            0 => Self::Light,
            1 => Self::Medium,
            2 => Self::Heavy,
            3 => Self::Fortified,
            4 => Self::Unarmored,
            5 => Self::Hero,
            6 => Self::Divine,
            _ => Self::None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Medium => "Medium",
            Self::Heavy => "Heavy",
            Self::Fortified => "Fortified",
            Self::Unarmored => "Unarmored",
            Self::Hero => "Hero",
            Self::Divine => "Divine",
            Self::None => "None",
        }
    }

    /// Maps the `defType` column of `UnitBalance.slk`.
    pub fn from_slk(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "small" | "light" => Self::Light,
            "medium" => Self::Medium,
            "large" | "heavy" => Self::Heavy,
            "fort" | "fortified" | "structure" => Self::Fortified,
            "normal" | "unarmored" => Self::Unarmored,
            "hero" => Self::Hero,
            "divine" => Self::Divine,
            _ => Self::None,
        }
    }
}

/// WC3 attack classes (rows of the damage table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AttackType {
    Normal,
    Pierce,
    Siege,
    Magic,
    Chaos,
    Spells,
    Hero,
}

impl AttackType {
    pub const COUNT: usize = 7;

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Pierce => "Pierce",
            Self::Siege => "Siege",
            Self::Magic => "Magic",
            Self::Chaos => "Chaos",
            Self::Spells => "Spells",
            Self::Hero => "Hero",
        }
    }

    /// Maps the `atkType1`/`weapTp1` columns of `UnitWeapons.slk`.
    pub fn from_slk(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "pierce" | "missile" => Self::Pierce,
            "siege" | "artillery" => Self::Siege,
            "magic" | "magicattack" => Self::Magic,
            "chaos" => Self::Chaos,
            "spells" => Self::Spells,
            "hero" => Self::Hero,
            _ => Self::Normal,
        }
    }
}

/// Damage multipliers in permille.
///
/// Rows: Normal, Pierce, Siege, Magic, Chaos, Spells, Hero.
/// Columns: Light (SMALL), Medium, Heavy (LARGE), Fortified, Unarmored
/// (NORMAL), Hero, Divine, None.
///
/// Observed verbatim in `analysis/war3x_local/Units/MiscGame.txt:106-112`
/// (`DamageBonusNormal=1.00,1.50,...`), access date 2026-11-12.
pub const DAMAGE_MULT_PERMILLE: [[i32; ArmorType::COUNT]; AttackType::COUNT] = [
    // NORMAL
    [1000, 1500, 1000, 700, 1000, 1000, 50, 1000],
    // PIERCE
    [2000, 750, 1000, 350, 1000, 500, 50, 1500],
    // SIEGE
    [1000, 500, 1000, 1500, 1000, 500, 50, 1500],
    // MAGIC
    [1250, 750, 2000, 350, 1000, 500, 50, 1000],
    // CHAOS
    [1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000],
    // SPELLS
    [1000, 1000, 1000, 1000, 1000, 750, 50, 1000],
    // HERO
    [1000, 1000, 1000, 500, 1000, 1000, 50, 1000],
];

/// Negative-armor damage amplification in permille: `2 - 0.94^armor`.
///
/// Values are the published Liquipedia Armor table (access date 2026-11-12):
/// -1 => 6.00%, -2 => 11.64%, ... -20 => 70.99%.  Stored as integers so the
/// replay hash cannot depend on libm's `powf`.
pub const NEG_ARMOR_MULT_PERMILLE: [i32; 21] = [
    1000, 1060, 1116, 1169, 1219, 1266, 1310, 1352, 1390, 1427, 1461, 1494, 1524, 1553, 1580, 1605,
    1628, 1651, 1672, 1691, 1710,
];

/// Flat WC3 armor coefficient (`DefenseArmor=0.06`, `Units/MiscData.txt:129`).
pub const DEFENSE_ARMOR_PERMILLE: i32 = 60;

/// Applies the WC3 armor formula to already-typed damage.
pub fn armor_reduction(armor: i32, damage: i32) -> i32 {
    if damage <= 0 {
        return 0;
    }
    let reduced = if armor >= 0 {
        // 1 / (1 + 0.06a) == 100 / (100 + 6a)
        damage * 100 / (100 + armor * 6)
    } else {
        let steps = (-armor).min(20) as usize;
        damage * NEG_ARMOR_MULT_PERMILLE[steps] / 1000
    };
    reduced.max(1)
}

/// Ethereal damage multiplier in permille, one row per attack type.
///
/// The engine folds `Ethereal[attackType]` (seven floats at `0x6f7d8c30`) into
/// the multiplier chain whenever the target's ethereal counter (`unit + 0x158`)
/// is positive, *after* the type table and *after* the armor step, and with no
/// dependence on the attack class (`0x6f19c677`).  TFT ships
/// `Units/MiscGame.txt:107 EtherealDamageBonus=1.66` for all seven rows
/// (observed, access date 2026-11-15); RoC-style data ships `1.0`, which is why
/// this is a table and not a single constant.
pub const ETHEREAL_DAMAGE_MULT_PERMILLE: [i32; AttackType::COUNT] = [1660; AttackType::COUNT];

/// Ethereal healing bonus in permille (`Units/MiscGame.txt:108`
/// `EtherealHealBonus=1.66`; heals use their own multiplier).
pub const ETHEREAL_HEAL_PERMILLE: i32 = 1660;

/// `true` when an attack class skips the armor-rating multiplier.
///
/// The engine gates the whole armor block on the damage object's physical flag
/// (`*(damage + 0x14) == 0x10`, block `0x6f19c5f8..0x6f19c628`); the attack-type
/// id is only read afterwards, as the row index of the 7x8 table (`0x6f19c64a`)
/// and of the ethereal row (`0x6f19c677`), and no branch compares it against
/// the spells id.  Attack type 0 in the engine is `spells`, so a spells hit
/// keeps the type table and the ethereal bonus and skips armor
/// (`analysis/agent-re/sim/PROBES.md` §2, `constants.json` `armor_gate`).
pub fn skips_armor(attack: AttackType) -> bool {
    matches!(attack, AttackType::Spells)
}

/// Full WC3 damage pipeline against a non-ethereal target: attack-type vs
/// armor-type multiplier, then the armor rating reduction (skipped for
/// [`AttackType::Spells`]).  Deterministic integer math end to end.
pub fn typed_damage(attack: AttackType, armor_type: ArmorType, armor: i32, base: i32) -> i32 {
    typed_damage_vs(attack, armor_type, armor, base, false)
}

/// Full WC3 damage pipeline including the ethereal bonus.
///
/// Order matches the engine multiplier chain:
/// 1. attack-type vs armor-type table (always, spells included),
/// 2. armor rating reduction (physical attacks only, see [`skips_armor`]),
/// 3. `EtherealDamageBonus[attack]` when the target is ethereal (always,
///    independent of the attack class).
pub fn typed_damage_vs(
    attack: AttackType,
    armor_type: ArmorType,
    armor: i32,
    base: i32,
    ethereal: bool,
) -> i32 {
    if base <= 0 {
        return 0;
    }
    let mult = DAMAGE_MULT_PERMILLE[attack.index()][armor_type.index()];
    let typed = base * mult / 1000;
    let after_armor = if skips_armor(attack) {
        typed
    } else {
        armor_reduction(armor, typed)
    };
    let dealt = if ethereal {
        after_armor * ETHEREAL_DAMAGE_MULT_PERMILLE[attack.index()] / 1000
    } else {
        after_armor
    };
    dealt.max(1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Race {
    Human,
    Orc,
    Undead,
    NightElf,
    Neutral,
}

impl Race {
    pub fn name(self) -> &'static str {
        match self {
            Self::Human => "Human",
            Self::Orc => "Orc",
            Self::Undead => "Undead",
            Self::NightElf => "Night Elf",
            Self::Neutral => "Neutral",
        }
    }

    pub fn from_slk(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "human" => Self::Human,
            "orc" => Self::Orc,
            "undead" => Self::Undead,
            "nightelf" => Self::NightElf,
            _ => Self::Neutral,
        }
    }
}

/// Canonical stat block.  Original field names are kept because the game and
/// HUD crates read them directly; everything after `is_building` was appended
/// when the simulation gained the full WC3 data model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitStats {
    pub hp: i32,
    pub armor: i32,
    pub damage: i32,
    pub range: f32,
    pub speed: f32,
    pub attack_ticks: u32,
    /// Backswing portion of the attack animation (`backSw1` seconds * 30).
    pub backswing_ticks: u32,
    pub sight: f32,
    pub cost: i32,
    /// Lumber cost (`lumbercost` column).
    pub lumber_cost: i32,
    pub train_ticks: u32,
    pub is_building: bool,
    /// Maximum mana; 0 for non-casters.
    pub max_mana: i32,
    /// Mana regeneration in thousandths of a point per tick.
    pub mana_regen_milli: i32,
    /// Food/upkeep supply consumed by the unit.
    pub supply: i32,
    /// Food/upkeep supply provided by the unit (buildings).
    pub supply_provided: i32,
    pub armor_type: ArmorType,
    pub attack_type: AttackType,
    /// Collision radius in world units.
    pub radius: f32,
    /// Number of damage dice (`dice1`).
    pub damage_dice: u32,
    /// Sides per die (`sides1`).
    pub damage_sides: u32,
    /// Acquisition range (`acquire`).
    pub acquire_range: f32,
    /// Construction time in ticks for buildings.
    pub build_ticks: u32,
    /// Turn rate (`turnRate` in UnitData.slk), fraction of a full turn per
    /// second; 0 for buildings/instant facing.
    pub turn_rate: f32,
    /// Unit level from `UnitBalance.slk` (`level`); heroes start at 1 and
    /// level through XP, other units keep the table value for kill XP.
    pub level: u8,
    /// Base hero attributes (`STR`/`AGI`/`INT`), zero for non-heroes.
    pub hero_strength: i32,
    pub hero_agility: i32,
    pub hero_intelligence: i32,
    /// Per-level attribute gains (`STRplus`/`AGIplus`/`INTplus`).
    pub hero_strength_gain: f32,
    pub hero_agility_gain: f32,
    pub hero_intelligence_gain: f32,
    /// Primary attribute (`Primary` column).
    pub hero_primary: HeroAttribute,
    /// Worker flag (can harvest and build).
    pub is_worker: bool,
    /// Missile speed in world units per second (`Missilespeed`); 0 = instant
    /// damage (melee) and no missile entity.
    pub missile_speed: f32,
    /// Missile arc as a fraction of travel distance (`Missilearc`).
    pub missile_arc: f32,
    /// Missile homes on a moving target (`MissileHoming`).
    pub missile_homing: bool,
}

impl UnitStats {
    /// Damage roll: `damage + dice * uniform(1..=sides)`.  The caller owns the
    /// RNG so replays stay deterministic.
    pub fn average_damage(&self) -> i32 {
        if self.damage_sides == 0 || self.damage_dice == 0 {
            self.damage
        } else {
            self.damage + (self.damage_dice * (self.damage_sides + 1) / 2) as i32
        }
    }

    pub fn max_damage(&self) -> i32 {
        self.damage + (self.damage_dice * self.damage_sides) as i32
    }
}

/// Stable identity of a roster definition.  Gameplay uses this (backed by the
/// generated SLK roster), not the four render buckets in [`UnitKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitDefId(pub u16);

impl UnitDefId {
    pub fn get(self) -> Option<&'static UnitDef> {
        def_at(self.0)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }

    pub fn from_id(id: &str) -> Option<Self> {
        def_index(id).map(Self)
    }
}

/// One data-defined unit or building.  `id`/`model` follow the WC3 four-letter
/// type id and the real asset path, so the data layer can feed table stats in
/// without touching simulation code.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitDef {
    pub id: &'static str,
    pub name: &'static str,
    pub race: Race,
    pub kind: UnitKind,
    /// Real model path when present in the local MPQ listing, else the
    /// canonical WC3 path with `model_present == false`.
    pub model: &'static str,
    pub model_present: bool,
    pub icon: &'static str,
    pub stats: UnitStats,
    /// Buildings that can train this unit (`trained_at` inverse).
    pub trained_at: &'static [&'static str],
    /// Unit ids this building trains (`Trains=` in `*UnitFunc.txt`); empty for
    /// non-producers.  Used by the data-driven build menu.
    pub trains: &'static [&'static str],
    /// Upgrade ids this building researches (`Researches=`), e.g. `Rhme`.
    pub researches: &'static [&'static str],
    /// Buildings required before the unit can be trained.
    pub requires: &'static [&'static str],
    /// Spells implemented by the simulation (first level of each).
    pub spells: &'static [Spell],
    /// Full WC3 ability id list from `UnitAbilities.slk`, kept for the data
    /// layer.  Simulated spells are the subset in `spells`.
    pub ability_ids: &'static [&'static str],
    /// Square building footprint in pathing cells (1 for units).
    pub footprint: u8,
    /// Directly placeable from the worker build menu.  Upgraded buildings
    /// (`uzg1`) exist as defs but are not worker-buildable in WC3.
    pub buildable: bool,
    /// True for hero definitions (altar-trained, `STR` column present).
    pub hero: bool,
    /// True for the race's main town-hall line (`htow`/`hkee`/`hcas`, …).
    /// Farms, burrows, ziggurats and moon wells provide supply but are not
    /// halls, so victory and "select base" never treat them as one.
    pub main_hall: bool,
    /// Real missile model path (`Missileart`); empty when the unit has none.
    pub missile_art: &'static str,
    /// Item ids this shop sells (`Sellitems=`); empty for non-shops.
    pub sells_items: &'static [&'static str],
}

impl UnitDef {
    pub fn is_building(&self) -> bool {
        self.stats.is_building
    }

    pub fn can_train(&self) -> bool {
        !self.trained_at.is_empty()
    }

    /// True for a producer building (barracks, sanctum, …).
    pub fn trains_units(&self) -> bool {
        self.is_building() && !self.trains.is_empty()
    }

    pub fn is_main_hall(&self) -> bool {
        self.main_hall
    }
}

/// Main hall for a race (town hall line; upgrades are not build-menu entries).
pub fn main_hall_for(race: Race) -> Option<&'static UnitDef> {
    let id = match race {
        Race::Human => "htow",
        Race::Orc => "ogre",
        Race::Undead => "unpl",
        Race::NightElf => "etol",
        Race::Neutral => "htow",
    };
    def_of(id)
}

/// Data-driven worker build menu for a race, derived from the generated SLK
/// roster: supply building, static defence, unit producer, main hall, then the
/// remaining buildings ordered by cost.  Costs/names are the real WC3 columns.
pub fn build_menu(race: Race) -> Vec<&'static UnitDef> {
    let mut out: Vec<&'static UnitDef> = Vec::new();
    let push = |def: Option<&'static UnitDef>, out: &mut Vec<&'static UnitDef>| {
        if let Some(def) = def {
            if !out.iter().any(|entry| entry.id == def.id) {
                out.push(def);
            }
        }
    };
    push(supply_building_for(race), &mut out);
    push(
        ROSTER
            .iter()
            .filter(|def| {
                def.race == race
                    && is_worker_buildable(def)
                    && !def.is_main_hall()
                    && (def.stats.damage > 0
                        // The tower line is built as its damage-less base form
                        // and upgraded from there (`[hwtw] Upgrade=hgtw,hctw`),
                        // so a defence slot without a damage row still counts
                        // when its own `Upgrade=` targets shoot.
                        || upgrade_targets_of(def.id)
                            .iter()
                            .any(|target| def_of(target).is_some_and(|form| form.stats.damage > 0)))
            })
            .min_by_key(|def| def.stats.cost),
        &mut out,
    );
    push(
        ROSTER
            .iter()
            .filter(|def| def.race == race && !def.is_main_hall() && def.trains_units())
            .min_by_key(|def| def.stats.cost),
        &mut out,
    );
    push(main_hall_for(race), &mut out);
    let mut rest: Vec<&'static UnitDef> = ROSTER
        .iter()
        .filter(|def| def.race == race && is_worker_buildable(def) && def.stats.cost > 0)
        .collect();
    // Melee item shops (Arcane Vault / Voodoo Lounge / Tomb of Relics /
    // Ancient of Wonders) are worker-buildable like every other building but
    // live in the appended `SHOP_DEFS` table, so the roster scan alone would
    // leave them out of the card.
    rest.extend(
        SHOP_DEFS
            .iter()
            .filter(|def| def.race == race && is_worker_buildable(def) && def.stats.cost > 0),
    );
    rest.sort_by_key(|def| (def.stats.cost, def.id));
    for def in rest {
        push(Some(def), &mut out);
    }
    out
}

/// Every building def of a race, roster and shop table combined.
pub fn building_defs(race: Race) -> Vec<&'static UnitDef> {
    let mut out: Vec<&'static UnitDef> = ROSTER
        .iter()
        .chain(SHOP_DEFS.iter())
        .chain(EXTRA_DEFS.iter())
        .filter(|def| def.race == race && def.is_building())
        .collect();
    out.sort_by_key(|def| def.id);
    out
}

/// Worker-placeable buildings of a race; this is exactly the set the build
/// menu must offer (upgraded forms and spell-placed buildings excluded).
pub fn worker_buildable(race: Race) -> Vec<&'static UnitDef> {
    let mut out: Vec<&'static UnitDef> = building_defs(race)
        .into_iter()
        .filter(|def| is_worker_buildable(def))
        .collect();
    out.sort_by_key(|def| def.id);
    out
}

/// Building that this def upgrades from (`uzg1` -> `uzig`, `hkee` -> `htow`).
pub fn upgrade_path(to_id: &str) -> Option<&'static UnitDef> {
    def_of(to_id)?;
    unit_upgrade_of(to_id).and_then(def_of)
}

/// Upgrade id that grants `ability_id`, if the install data maps one.
pub fn ability_unlock_upgrade(ability_id: &str) -> Option<&'static str> {
    UPGRADES
        .iter()
        .find(|upgrade| {
            upgrade.effects.iter().any(|effect| {
                effect.kind == UpgradeEffectKind::UnlockAbility && effect.ability == ability_id
            })
        })
        .map(|upgrade| upgrade.id)
}

/// Cheapest supply building for a race (farm/burrow/ziggurat/moon well).
/// Race altar (`halt`/`oalt`/`uaod`/`eate`) used for hero revive.
pub fn altar_for(race: Race) -> Option<&'static UnitDef> {
    let id = match race {
        Race::Human => "halt",
        Race::Orc => "oalt",
        Race::Undead => "uaod",
        Race::NightElf => "eate",
        // Neutral (tavern) heroes revive at the Tavern, the building that
        // trains them; `ntav` is the only neutral hero producer.
        Race::Neutral => "ntav",
    };
    def_of(id)
}

/// Neutral Tavern (`ntav`): the building that sells the tavern hero roster.
pub fn tavern() -> Option<&'static UnitDef> {
    def_of("ntav")
}

pub fn supply_building_for(race: Race) -> Option<&'static UnitDef> {    ROSTER
        .iter()
        .filter(|def| {
            def.race == race
                && is_worker_buildable(def)
                && !def.main_hall
                && def.stats.supply_provided > 0
                && def.stats.cost > 0
        })
        .min_by_key(|def| def.stats.cost)
}

/// Building footprint side in sim grid cells (one cell = 32 world units, the
/// resolution the sim grid is built with).
///
/// `ceil(collision / 32)`: a 176-unit Great Hall is 6 cells, a 72-unit tower 3.
/// Clamped to 8 so a malformed collision cannot block a base.
pub fn footprint_for(collision: f32) -> u8 {
    let cells = (collision / crate::PATHING_CELL_SIZE).ceil();
    (cells as u8).clamp(1, 8)
}

// ---------------------------------------------------------------------------
// Canonical four units (the renderer-facing buckets)
// ---------------------------------------------------------------------------

pub const CANONICAL_HUMAN_WORKER: &str = "hpea";
pub const CANONICAL_HUMAN_MELEE: &str = "hfoo";
pub const CANONICAL_HUMAN_RANGED: &str = "hrif";
pub const CANONICAL_HUMAN_HALL: &str = "htow";

impl UnitKind {
    /// Type id of the representative roster entry for this render bucket.
    pub fn canonical_id(self) -> &'static str {
        match self {
            Self::Footman => CANONICAL_HUMAN_MELEE,
            Self::Archer => CANONICAL_HUMAN_RANGED,
            Self::Peasant => CANONICAL_HUMAN_WORKER,
            Self::TownHall => CANONICAL_HUMAN_HALL,
        }
    }

    pub fn canonical_def(self) -> &'static UnitDef {
        def_of(self.canonical_id()).expect("canonical unit definition present")
    }

    /// Stable stat accessor used by the HUD and the game crate.  Values come
    /// from the generated roster, which is generated from real WC3 tables.
    pub fn stats(self) -> UnitStats {
        self.canonical_def().stats
    }

    pub fn name(self) -> &'static str {
        self.canonical_def().name
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "Footman" | "hfoo" => Some(Self::Footman),
            "Archer" | "Rifleman" | "hrif" => Some(Self::Archer),
            "Peasant" | "hpea" => Some(Self::Peasant),
            "TownHall" | "htow" => Some(Self::TownHall),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UpgradeEffectKind {
    AttackDamage,
    /// Attack damage bonus in permille of the unit's base damage.
    AttackDamagePercent,
    Armor,
    HitPoints,
    /// Maximum life bonus in permille of the unit's base life.
    HitPointsPercent,
    MoveSpeed,
    AttackRange,
    Mana,
    ManaRegen,
    /// Hit points regenerated per tick, in thousandths of a point.
    HitPointRegen,
    /// Extra lumber carried per harvesting trip.
    LumberHarvest,
    /// Sight radius bonus in world units.
    Sight,
    /// Attack speed bonus in permille (250 = 25% faster).
    AttackSpeed,
    /// Sets the unit's armor class (`rart`, 3 = Fortified).
    ArmorType,
    /// Spiked barricade: flat damage returned to melee attackers.
    Spikes,
    /// Grants the ability `ability` to affected units when researched.
    UnlockAbility,
    /// Real engine flag with no simulation model yet; `note` carries the
    /// reason and the census reports it separately from working effects.
    DataOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpgradeEffect {
    pub kind: UpgradeEffectKind,
    pub base: i32,
    pub step: i32,
    /// Ability id for [`UpgradeEffectKind::UnlockAbility`].
    pub ability: &'static str,
    /// Human-readable reason for [`UpgradeEffectKind::DataOnly`].
    pub note: &'static str,
}

impl UpgradeEffect {
    pub fn is_data_only(self) -> bool {
        self.kind == UpgradeEffectKind::DataOnly
    }
}

/// Aggregated, level-aware stat bonuses an owner's researched upgrades give to
/// one unit definition.  Computed from `UPGRADES` + `UnitDef::upgrades`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UpgradeBonus {
    pub damage: i32,
    pub damage_permille: i32,
    pub armor: i32,
    pub hp: i32,
    pub hp_permille: i32,
    pub speed: f32,
    pub range: f32,
    pub mana: i32,
    pub mana_regen_milli: i32,
    pub hp_regen_milli: i32,
    pub sight: f32,
    pub lumber: i32,
    pub attack_speed_permille: i32,
    pub armor_type: Option<ArmorType>,
}

#[derive(Debug, Clone, Copy)]
pub struct UpgradeDef {
    pub id: &'static str,
    pub name: &'static str,
    pub race: Race,
    pub class: &'static str,
    pub max_level: u8,
    pub gold_base: i32,
    pub gold_step: i32,
    pub time_base: u32,
    pub time_step: u32,
    pub effects: &'static [UpgradeEffect],
}

impl UpgradeDef {
    pub fn gold_cost(&self, next_level: u8) -> i32 {
        self.gold_base + self.gold_step * next_level.saturating_sub(1) as i32
    }

    pub fn research_ticks(&self, next_level: u8) -> u32 {
        self.time_base + self.time_step * next_level.saturating_sub(1) as u32
    }
}

// The roster is generated: `analysis/agent-sim/gen_roster.py` emits it from
// the real Warcraft III tables.  See `analysis/agent-sim/REPORT.md`.
include!("roster_gen.rs");
include!("creeps_gen.rs");
include!("shops_gen.rs");
include!("extras_gen.rs");
include!("bounty_gen.rs");

/// Upgrade ids that affect a unit (`upgrades` column of `UnitBalance.slk`).
/// WC3 applies a researched upgrade only to units that list it, not to the
/// whole race.
pub fn unit_upgrades(id: &str) -> &'static [&'static str] {
    UNIT_UPGRADES
        .iter()
        .find(|(unit, _)| *unit == id)
        .map(|(_, upgrades)| *upgrades)
        .unwrap_or(&[])
}

/// Armor gained per researched armor upgrade (`defUp` column).
pub fn unit_armor_upgrade(id: &str) -> i32 {
    UNIT_ARMOR_UPGRADE
        .iter()
        .find(|(unit, _)| *unit == id)
        .map(|(_, amount)| *amount)
        .unwrap_or(0)
}

/// Building this def upgrades from (`Upgrade=` in `*UnitFunc.txt`).
pub fn unit_upgrade_of(id: &str) -> Option<&'static str> {
    UNIT_UPGRADE_OF
        .iter()
        .find(|(unit, _)| *unit == id)
        .map(|(_, from)| *from)
}

/// Buildings `unit_id` can be upgraded into, from `*UnitFunc.txt` `Upgrade=`.
///
/// This is the engine's own upgrade table (`[htow] Upgrade=hkee`,
/// `[hwtw] Upgrade=hgtw,hctw,hatw`, `[uzig] Upgrade=uzg1,uzg2`), stored by the
/// generator as the inverse [`UNIT_UPGRADE_OF`] relation.  The order is the
/// order of that static table, so it is deterministic for a command stream.
pub fn upgrade_targets_of(unit_id: &str) -> Vec<&'static str> {
    UNIT_UPGRADE_OF
        .iter()
        .filter(|(_, from)| *from == unit_id)
        .map(|(target, _)| *target)
        .collect()
}

/// True when `target` is one of the forms the engine upgrades `unit_id` into.
pub fn can_upgrade_into(unit_id: &str, target: &str) -> bool {
    unit_upgrade_of(target) == Some(unit_id)
}

/// Ability that creates this building (`egol` <- Entangle Gold Mine).
pub fn unit_placed_by_ability(id: &str) -> Option<&'static str> {
    UNIT_PLACED_BY_ABILITY
        .iter()
        .find(|(unit, _)| *unit == id)
        .map(|(_, ability)| *ability)
}

/// True when a worker can place this building directly from the build menu:
/// buildable, not an upgraded form, not spell-placed.
pub fn is_worker_buildable(def: &UnitDef) -> bool {
    def.buildable
        && def.is_building()
        && unit_upgrade_of(def.id).is_none()
        && unit_placed_by_ability(def.id).is_none()
}

/// True when the researched `upgrade_id` affects `def`.
pub fn affected_by(def: &UnitDef, upgrade_id: &str) -> bool {
    unit_upgrades(def.id).contains(&upgrade_id)
}

/// WC3 gold bounty of a unit (`bountyplus`, `bountydice`, `bountysides` from
/// `UnitBalance.slk`).  `None` when the unit pays nothing.
pub fn unit_bounty(id: &str) -> Option<(i32, u32, u32)> {
    UNIT_BOUNTY
        .binary_search_by(|(unit, _, _, _)| (*unit).cmp(id))
        .ok()
        .map(|index| {
            let (_, plus, dice, sides) = UNIT_BOUNTY[index];
            (plus, dice, sides)
        })
}

#[derive(Debug, Clone, Copy)]
pub struct DestructableDef {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub hp: i32,
    pub armor: &'static str,
    pub model: &'static str,
    pub texture: &'static str,
    pub radius: f32,
    pub target_type: &'static str,
}

impl DestructableDef {
    pub fn is_tree(&self) -> bool {
        self.target_type.eq_ignore_ascii_case("tree")
    }
}

include!("destructables_gen.rs");

pub fn destructable_def(id: &str) -> Option<&'static DestructableDef> {
    DESTRUCTABLES.iter().find(|def| def.id == id)
}

// The item table is generated by `analysis/agent-sim/gen_items.py`.
#[derive(Debug, Clone, Copy)]
pub struct ItemDef {
    pub id: &'static str,
    pub name: &'static str,
    pub class: &'static str,
    pub level: i32,
    pub cost: i32,
    pub charges: i32,
    /// `stockMax`/`stockRegen` from ItemData.slk (shop restock).
    pub stock_max: i32,
    pub stock_regen: i32,
    pub usable: bool,
    pub sellable: bool,
    pub perishable: bool,
    pub droppable: bool,
    pub ability_id: &'static str,
    pub bonus_attack: i32,
    pub bonus_armor: i32,
    pub bonus_hp: i32,
    pub bonus_mana: i32,
    pub heal: i32,
    pub mana_restore: i32,
}

impl ItemDef {
    pub fn has_effect(&self) -> bool {
        self.bonus_attack != 0
            || self.bonus_armor != 0
            || self.bonus_hp != 0
            || self.bonus_mana != 0
            || self.heal != 0
            || self.mana_restore != 0
    }
}

/// One `Requires=` token of an item with its `DependencyOr` alternatives.
#[derive(Debug, Clone, Copy)]
pub struct ItemRequirement {
    pub item: &'static str,
    pub token: &'static str,
    pub alternatives: &'static [&'static str],
}

/// Recipe row (result item + one required item).  Retail ships zero rows.
#[derive(Debug, Clone, Copy)]
pub struct RecipeDef {
    pub result: &'static str,
    pub ingredient: &'static str,
}

include!("item_reqs_gen.rs");
// The included item/shop runtimes (`items_gen.rs` -> `items.rs`,
// `shops_gen.rs` -> `shops.rs`) share this module's namespace, so the imports
// they need live here once instead of in each included file (two files
// importing the same name into one module is a duplicate-definition error).
use crate::{generic_ability, Item, ItemId, SHOP_RANGE, Sim, TICKS_PER_SECOND, UnitId};

include!("items_gen.rs");

/// Purchase requirements of an item (`Requires=` + `DependencyOr=`).
pub fn item_requirements(item_id: &str) -> impl Iterator<Item = &'static ItemRequirement> {
    ITEM_REQUIREMENTS
        .iter()
        .filter(move |requirement| requirement.item == item_id)
}

/// Recipe rows whose result is `item_id`.
pub fn recipes_for(item_id: &str) -> impl Iterator<Item = &'static RecipeDef> {
    RECIPES
        .iter()
        .filter(move |recipe| recipe.result == item_id)
}

pub fn item_def(id: &str) -> Option<&'static ItemDef> {
    ITEMS.iter().find(|item| item.id == id)
}

// The upgrade table is generated by `analysis/agent-sim/gen_upgrades.py`.
include!("upgrades_gen.rs");

pub fn upgrade_def(id: &str) -> Option<&'static UpgradeDef> {
    UPGRADES.iter().find(|upgrade| upgrade.id == id)
}

pub fn upgrades_for(race: Race) -> impl Iterator<Item = &'static UpgradeDef> {
    UPGRADES.iter().filter(move |upgrade| upgrade.race == race)
}

/// Units sold by shop buildings (`Sellunits=` in the unit profile texts),
/// generated from `analysis/war3x_data/Units/*UnitFunc.txt`.
pub static SHOP_UNITS: &[(&str, &[&str])] = &[
    ("ushp", &["ubot", "udes"]),
    ("hshy", &["hbot"]),
    ("ndrk", &["nbdr", "nbdk", "nbwm"]),
    ("ndru", &["nadw", "nadk", "nadr"]),
    ("ndrz", &["nbzw", "nbzk", "nbzd"]),
    ("ndrg", &["ngrw", "ngdk", "ngrd"]),
    ("ndrr", &["nrdk", "nrdr", "nrwm"]),
    ("ngad", &["ngsp", "nzep", "ngir"]),
    ("nshp", &["nbot"]),
    ("nmer", &["nftb", "nfsp", "nogm", "ngrk"]),
    ("nmr2", &["ngnb", "ngnw", "nomg", "ngrk"]),
    ("nmr3", &["ngnv", "nits", "nitt", "ngrk"]),
    ("nmr4", &["ncen", "nhrr", "nrzm", "nhrw"]),
    ("nmr5", &["nsts", "nfrs", "nthl"]),
    ("nmr6", &["nmfs", "nslf", "nstl"]),
    ("nmr7", &["nnwa", "nits", "nrvs", "nnwl"]),
    ("nmr8", &["nrog", "nass", "nkog", "nfsh"]),
    ("nmr9", &["nrog", "nass", "nkog", "nfsh"]),
    ("nmr0", &["nkob", "nmrr", "nass", "nkog"]),
    ("nmra", &["nskf", "nkog", "nowb"]),
    ("nmrb", &["nskf", "nkog", "nowb"]),
    ("nmrc", &["nmsn", "nlsn", "nlds", "ntrt"]),
    ("nmrd", &["nanm", "nbdm", "nmgw", "nfps"]),
    ("nmre", &["npfl", "ndrm", "nvdw", "ndrd"]),
    ("nmrf", &["npfl", "ndrm", "nvdw", "ndrd"]),
    ("ntav", &["Nngs", "Nbrn", "Npbm", "Nplh", "Nbst"]),
    ("ndro", &["nnht", "nndk", "nndr"]),
    ("oshy", &["obot"]),
];

/// `Sellunits` of one shop id (`[]` when it sells no units).
pub fn sold_units(shop_id: &str) -> &'static [&'static str] {
    SHOP_UNITS
        .iter()
        .find(|(id, _)| *id == shop_id)
        .map(|(_, units)| *units)
        .unwrap_or(&[])
}

/// Linear lookup of a roster or camp definition by WC3 type id.
pub fn def_of(id: &str) -> Option<&'static UnitDef> {
    ROSTER
        .iter()
        .chain(CREEP_DEFS.iter())
        .chain(SHOP_DEFS.iter())
        .chain(EXTRA_DEFS.iter())
        .chain(TECH_DEFS.iter())
        .chain(CAMPAIGN_DEFS.iter())
        .find(|def| def.id == id)
}

/// Definition by table index: the melee roster first, then creep, shop, extra
/// and campaign defs.  Campaign rows are last so no shipped index moves.
pub fn def_at(index: u16) -> Option<&'static UnitDef> {
    let index = index as usize;
    if index < ROSTER.len() {
        ROSTER.get(index)
    } else if index < ROSTER.len() + CREEP_DEFS.len() {
        CREEP_DEFS.get(index - ROSTER.len())
    } else if index < ROSTER.len() + CREEP_DEFS.len() + SHOP_DEFS.len() {
        SHOP_DEFS.get(index - ROSTER.len() - CREEP_DEFS.len())
    } else if index < ROSTER.len() + CREEP_DEFS.len() + SHOP_DEFS.len() + EXTRA_DEFS.len() {
        EXTRA_DEFS.get(index - ROSTER.len() - CREEP_DEFS.len() - SHOP_DEFS.len())
    } else if index
        < ROSTER.len() + CREEP_DEFS.len() + SHOP_DEFS.len() + EXTRA_DEFS.len() + TECH_DEFS.len()
    {
        TECH_DEFS.get(index - ROSTER.len() - CREEP_DEFS.len() - SHOP_DEFS.len() - EXTRA_DEFS.len())
    } else {
        CAMPAIGN_DEFS.get(
            index
                - ROSTER.len()
                - CREEP_DEFS.len()
                - SHOP_DEFS.len()
                - EXTRA_DEFS.len()
                - TECH_DEFS.len(),
        )
    }
}

/// Index of a type id in the combined roster + creep table.
pub fn def_index(id: &str) -> Option<u16> {
    if let Some(index) = ROSTER.iter().position(|def| def.id == id) {
        return Some(index as u16);
    }
    if let Some(index) = CREEP_DEFS.iter().position(|def| def.id == id) {
        return Some((ROSTER.len() + index) as u16);
    }
    if let Some(index) = SHOP_DEFS.iter().position(|def| def.id == id) {
        return Some((ROSTER.len() + CREEP_DEFS.len() + index) as u16);
    }
    EXTRA_DEFS
        .iter()
        .position(|def| def.id == id)
        .map(|index| (ROSTER.len() + CREEP_DEFS.len() + SHOP_DEFS.len() + index) as u16)
        .or_else(|| {
            TECH_DEFS.iter().position(|def| def.id == id).map(|index| {
                (ROSTER.len() + CREEP_DEFS.len() + SHOP_DEFS.len() + EXTRA_DEFS.len() + index) as u16
            })
        })
        .or_else(|| {
            CAMPAIGN_DEFS.iter().position(|def| def.id == id).map(|index| {
                (ROSTER.len()
                    + CREEP_DEFS.len()
                    + SHOP_DEFS.len()
                    + EXTRA_DEFS.len()
                    + TECH_DEFS.len()
                    + index) as u16
            })
        })
}

/// Every definition for one race.
pub fn roster_for(race: Race) -> impl Iterator<Item = &'static UnitDef> {
    ROSTER.iter().filter(move |def| def.race == race)
}

/// Formula helper used by tests and by the AI to find the nearest drop-off.
pub fn distance_sq(a: Vec2, b: Vec2) -> f32 {
    let d = a - b;
    d.x * d.x + d.y * d.y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_table_matches_wc3_values() {
        // Observed: MiscGame.txt DamageBonusNormal=1.00,1.50,1.00,0.70,1.00,1.00
        assert_eq!(DAMAGE_MULT_PERMILLE[0][ArmorType::Light.index()], 1000);
        assert_eq!(DAMAGE_MULT_PERMILLE[0][ArmorType::Medium.index()], 1500);
        assert_eq!(DAMAGE_MULT_PERMILLE[0][ArmorType::Fortified.index()], 700);
        // DamageBonusPierce=2.00,0.75,1.00,0.35,1.50,0.50
        assert_eq!(DAMAGE_MULT_PERMILLE[1][ArmorType::Light.index()], 2000);
        assert_eq!(DAMAGE_MULT_PERMILLE[1][ArmorType::Fortified.index()], 350);
        // DamageBonusSiege=1.00,0.50,1.00,1.50,1.50,0.50
        assert_eq!(DAMAGE_MULT_PERMILLE[2][ArmorType::Fortified.index()], 1500);
        // DamageBonusMagic=1.25,0.75,2.00,0.35,1.00,0.50
        assert_eq!(DAMAGE_MULT_PERMILLE[3][ArmorType::Heavy.index()], 2000);
        // Chaos is uniform.
        for armor in 0..ArmorType::COUNT {
            assert_eq!(DAMAGE_MULT_PERMILLE[4][armor], 1000);
        }
    }

    #[test]
    fn armor_formula_matches_wc3_table() {
        // armor 5 => 23.08% reduction => 100 * 100 / 130 = 76
        assert_eq!(armor_reduction(5, 100), 76);
        // armor 10 => 37.5% => 62
        assert_eq!(armor_reduction(10, 100), 62);
        assert_eq!(armor_reduction(0, 100), 100);
        // Negative armor amplifies.
        assert_eq!(armor_reduction(-1, 100), 106);
        assert_eq!(armor_reduction(-10, 100), 146);
        // Never zeroes a positive hit.
        assert_eq!(armor_reduction(1000, 1), 1);
    }

    #[test]
    fn typed_damage_combines_table_and_armor() {
        // Pierce vs heavy (x1.00) then armor 2 (89%): 100 -> 100 -> 89
        assert_eq!(
            typed_damage(AttackType::Pierce, ArmorType::Heavy, 2, 100),
            89
        );
        // Normal vs medium x1.5: 100 -> 150, armor 2 -> 133
        assert_eq!(
            typed_damage(AttackType::Normal, ArmorType::Medium, 2, 100),
            133
        );
    }

    #[test]
    fn canonical_kinds_resolve_to_real_roster_entries() {
        for kind in [
            UnitKind::Footman,
            UnitKind::Archer,
            UnitKind::Peasant,
            UnitKind::TownHall,
        ] {
            let def = kind.canonical_def();
            assert_eq!(def.id, kind.canonical_id());
            assert!(def.stats.hp > 0);
        }
        assert!(UnitKind::Footman.stats().cost > 0);
    }

    #[test]
    fn roster_covers_all_four_races_and_buildings() {
        for race in [Race::Human, Race::Orc, Race::Undead, Race::NightElf] {
            let defs: Vec<_> = roster_for(race).collect();
            assert!(
                defs.len() >= 10,
                "{} roster too small: {}",
                race.name(),
                defs.len()
            );
            assert!(
                defs.iter().any(|d| d.is_building()),
                "{} has buildings",
                race.name()
            );
            assert!(
                defs.iter().any(|d| !d.is_building()),
                "{} has units",
                race.name()
            );
            assert!(
                defs.iter().any(|d| d.can_train()),
                "{} has producers",
                race.name()
            );
        }
    }

    #[test]
    fn roster_stats_are_grounded_in_wc3() {
        let footman = def_of("hfoo").unwrap().stats;
        assert_eq!(footman.hp, 420);
        assert_eq!(footman.armor, 2);
        assert_eq!(footman.cost, 160);
        assert_eq!(footman.supply, 2);
        assert_eq!(footman.armor_type, ArmorType::Medium);
        assert_eq!(footman.attack_type, AttackType::Normal);
        assert_eq!(footman.damage, 11);
        let hall = def_of("htow").unwrap().stats;
        assert!(hall.is_building);
        assert_eq!(hall.hp, 1500);
        assert_eq!(hall.supply_provided, 12);
        assert_eq!(hall.armor_type, ArmorType::Fortified);
    }
}
