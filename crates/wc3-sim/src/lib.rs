//! Deterministic fixed-tick RTS simulation.
//!
//! The simulation is renderer-free and uses only ordered containers / sorted
//! entity vectors so that a seed + command log reproduces bit-identical state.
//! Tick rate is 30 Hz (WC3 melee timing); every subsystem is tick-based, so the
//! same code runs at 60 Hz by halving the tick duration downstream.
//!
//! Pipeline per tick: commands -> timers/production -> orders -> movement ->
//! combat -> economy -> AI -> fog/supply -> victory.
//!
//! Layout:
//! * [`data`]      WC3 stat/armor/weapon tables + generated four-race roster
//! * [`nav`]       bounded A*, flow-field tracing, formations
//! * [`spatial`]   uniform hash for all neighbour queries
//! * [`abilities`] mana/cooldown/targeting spell framework
//! * [`buffs`]     buff/aura/timed-life stat modifiers
//! * [`effects`]   generic ability effect handlers (damage/heal/buff/aura/summon/movement)
//! * [`squad`]     squad-level AI (leader, regroup, attack-or-flee)
//! * [`jass_api`]  JASS natives bound to the simulation
//! * [`ai`]        economy/army/wave opponent

pub mod abilities;
pub mod ai;
pub mod buffs;
pub mod combat;
pub mod combat_gen;
pub mod data;
pub mod effects;
pub mod heroes;
pub mod jass;
pub mod jass_api;
pub mod nav;
pub mod net;
pub mod save;
pub mod spatial;
pub mod squad;

use std::collections::BTreeMap;

pub use glam::Vec2;
use wc3_core::PlayerId;
use wc3_path::Grid;

pub use abilities::{
    GENERIC_ABILITIES, GenericAbility, SPELL_DEFS, Spell, SpellDef, SpellTarget, TargetKind,
    data_ability_ids, data_castable_ids, generic_ability, typed_ability_defs,
    typed_ability_summary,
};
pub use ai::{AiConfig, AiMode, AiState};
pub use buffs::{Buff, BuffApplied, BuffId, BuffKind, StatKey, StatMod};
pub use combat::{damage_range, miss_chance_permille, tg};
pub use combat_gen::{CombatProfile, SplashProfile};
pub use data::{
    ArmorType, AttackType, HeroAttribute, ROSTER, Race, UnitDef, UnitDefId, UnitKind, UnitStats,
    def_at, def_index, def_of, roster_for,
};
pub use effects::{EffectContext, EffectOutcome, EffectSpec, run_effects};
pub use jass_api::SimJass;
pub use squad::{Squad, SquadConfig, SquadKind, SquadManager, SquadState};

pub use wc3_re::gameplay::GameplayConfig;

pub const TICKS_PER_SECOND: u32 = 30;

/// Hero XP needed to advance from level `i+1` to `i+2`
/// (`NeedHeroXP` in `analysis/war3_data/Units/MiscData.txt:108`).
pub const NEED_HERO_XP: [u32; 10] = [200, 500, 900, 1400, 2000, 2700, 3500, 4400, 5400, 5400];
/// XP granted for killing a level `i+1` unit (`GrantHeroXP`, MiscData.txt:109).
pub const GRANT_HERO_XP: [u32; 10] = [80, 100, 120, 140, 160, 200, 240, 280, 320, 400];
/// `MaxHeroLevel` (MiscData.txt:107).
pub const MAX_HERO_LEVEL: u8 = 10;
/// `HeroExpRange` (MiscData.txt:74): heroes in this radius share kill XP.
pub const HERO_EXP_RANGE: f32 = 1200.0;
/// `StrHitPointBonus` (MiscData.txt:113).
pub const STR_HIT_POINT_BONUS: i32 = 25;
/// `IntManaBonus` (MiscData.txt:115).
pub const INT_MANA_BONUS: i32 = 15;
/// `AgiDefenseBonus` (MiscData.txt:117) in thousandths of armor per point.
pub const AGI_DEFENSE_MILLI: i32 = 300;
/// Max hero revive gold (`HeroMaxReviveCostGold` in MiscGame.txt: 700).
pub const HERO_MAX_REVIVE_COST_GOLD: i32 = 700;
/// Max hero revive time (`HeroMaxReviveTime` in MiscGame.txt: 150 s).
pub const HERO_MAX_REVIVE_TIME_SECONDS: f32 = 150.0;
/// Repair cost as a share of the building's build cost (permille).  The
/// extracted MiscGame has no repair row (observed); 50% full-repair cost is the
/// classic WC3 repair economy and is documented as a constant.
pub const REPAIR_COST_PERMILLE: i32 = 500;
/// HP restored per tick by one repairing worker.
pub const REPAIR_HP_PER_TICK: i32 = 8;
/// Gold in a standard mine (`Agld` DataA1 in AbilityData.slk: 12500).
pub const GOLD_MINE_GOLD: i32 = 12_500;

/// XP needed to advance from `level` to `level + 1` (0 at max level).
pub fn hero_xp_to_next(level: u8) -> u32 {
    if level == 0 || level >= MAX_HERO_LEVEL {
        return 0;
    }
    NEED_HERO_XP[(level - 1) as usize]
}

/// Cumulative XP required to reach `level` from level 1.
pub fn hero_xp_for_level(level: u8) -> u32 {
    (1..level.max(1)).map(hero_xp_to_next).sum()
}
pub const CELL_SIZE: f32 = 128.0;
/// Native WPM resolution: one pathing cell is 32 world units (128/4).
pub const PATHING_CELL_SIZE: f32 = 32.0;
/// Hard food ceiling, matching WC3's 100 supply cap.
pub const MAX_SUPPLY: i32 = 100;
/// Eight A* requests per tick; the rest keep moving on their previous path.
pub const PATH_BUDGET: u32 = 8;
/// Maximum queued orders per unit (shift-click cap).
pub const ORDER_QUEUE_MAX: usize = 8;
/// Maximum production queue per building.
pub const PRODUCTION_QUEUE_MAX: usize = 5;
/// Arrival distance for move orders.
const ARRIVE_RADIUS: f32 = 24.0;
/// Gold carried by one worker per trip (`Ahar` Data11).
pub const HARVEST_CAPACITY: i32 = 10;
/// Ticks a worker spends inside the mine (`Ahar` Data12, 2 s at 30 Hz).
pub const HARVEST_GATHER_TICKS: u32 = 60;
/// Cap on recorded `SPAWN_ADJUST` lines; a melee map spawns dozens of units per
/// wave, and the log is a placement record, not a full unit history.
pub const SPAWN_LOG_LIMIT: usize = 512;
/// WC3 lumber carry per trip (`Ahrl` Data11): 10 lumber for a peon/peasant.
pub const HARVEST_LUMBER_CAPACITY: i32 = 10;
/// Radius a worker scans for a replacement tree when its tree is destroyed
/// (WC3 peons keep the lumber loop running across a felled forest).
pub const HARVEST_RETARGET_RADIUS: f32 = 600.0;
/// Slack added to a gather arrival gate beyond the stop radius / blocked
/// footprint, so a worker standing on the nearest walkable cell centre is
/// always accepted as "at the target".
const GATHER_SLACK: f32 = 8.0;
/// Worker stand-off from a building it channels, authored on the same
/// `UnitDef::footprint` scale (32-unit pathing cells) as the footprint itself:
/// a 6-cell town hall is `150 + 6*48 = 438` world units from its centre.  The
/// ring always lies outside the building's blocked cells, which is what lets a
/// builder reach it at all — a construction site blocks its own footprint the
/// moment it is placed.
pub const BUILD_CHANNEL_BASE: f32 = 150.0;
/// Stand-off growth per authored footprint cell (see `BUILD_CHANNEL_BASE`).
pub const BUILD_CHANNEL_PER_CELL: f32 = 48.0;
/// Slack on the construction/repair range check, so a builder that arrived at
/// its stop ring still counts as channeling after separation jitter pushes it
/// a few units out (two movement steps at worker speed).
const BUILD_CHANNEL_SLACK: f32 = 16.0;

// ---- fog of war -----------------------------------------------------------

/// Fog cell size.  Sight radii read from `UnitBalance.slk` (`sight`) are in
/// world units, so the fog grid quantises them to this size.
pub const FOG_CELL_SIZE: f32 = CELL_SIZE;
/// Largest sight radius supported by the precomputed fog disc table (64 cells).
pub const FOG_MAX_SIGHT: f32 = 64.0 * FOG_CELL_SIZE;

// ---- neutral creep camps --------------------------------------------------

/// Two neutral placements closer than this belong to the same camp (greedy
/// anchor clustering on `war3mapUnits.doo`; a WC3 camp body is 2-5 units).
pub const CAMP_CLUSTER_RADIUS: f32 = 600.0;
/// WC3 creep camp aggro radius: a player unit inside this ring around the camp
/// anchor wakes the guards.
pub const CAMP_AGGRO_RADIUS: f32 = 500.0;
/// `MaxGuardDistance` leash: guards chase at most this far from the anchor and
/// then return home.
pub const CAMP_LEASH_RADIUS: f32 = 900.0;
/// A guard inside this radius of the anchor counts as home.
pub const CAMP_HOME_RADIUS: f32 = 80.0;
/// Mobile neutral units with no attack (sheep, dog, crab) flee from player
/// units that come this close.
pub const CRITTER_FLEE_RADIUS: f32 = 300.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ruleset {
    /// Reign of Chaos: 90 food cap; upkeep 0-40 / 41-70 / 71-90.
    Roc,
    /// The Frozen Throne: 100 food cap; upkeep 0-50 / 51-80 / 81-100.
    Tft,
}

/// WC3 upkeep tier.  TFT thresholds are 50/80 food; tax is 0/30/60%.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UpkeepTier {
    None,
    Low,
    High,
}

impl UpkeepTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "NO UPKEEP",
            Self::Low => "LOW UPKEEP",
            Self::High => "HIGH UPKEEP",
        }
    }

    /// Upkeep tax in permille of income (0/300/600).
    pub fn tax_permille(self) -> i32 {
        match self {
            Self::None => 0,
            Self::Low => UPKEEP_LOW_TAX_PERMILLE,
            Self::High => UPKEEP_HIGH_TAX_PERMILLE,
        }
    }

    /// Paid income in permille (1000/700/400).
    pub fn income_permille(self) -> i32 {
        1000 - self.tax_permille()
    }
}

/// TFT upkeep breakpoints (food) and tax rates.  Thresholds/rates are the WC3
/// gameplay constants (50/80, 0.30/0.60); `MiscGame.txt` in this install has
/// revive/XP constants only and no upkeep row (observed).
pub const UPKEEP_LOW_FOOD: i32 = 50;
pub const UPKEEP_HIGH_FOOD: i32 = 80;
pub const UPKEEP_LOW_TAX_PERMILLE: i32 = 300;
pub const UPKEEP_HIGH_TAX_PERMILLE: i32 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HarvestPhase {
    ToMine,
    Gathering,
    ToHall,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Order {
    Idle,
    Move(Vec2),
    Attack(UnitId),
    MoveAttack(Vec2),
    Hold,
    /// Patrol between two points; `forward` selects the next endpoint.
    Patrol {
        a: Vec2,
        b: Vec2,
        forward: bool,
    },
    /// Harvest a gold mine and return the load to the nearest hall.
    Harvest {
        mine: Vec2,
        phase: HarvestPhase,
    },
    /// Chop a destructable tree and return the load to the nearest hall.
    HarvestLumber {
        tree: Vec2,
        phase: HarvestPhase,
    },
    /// Move to and channel a construction site.
    Build(UnitId),
    /// Repair a damaged friendly building.
    Repair(UnitId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Formation {
    Box,
    Line,
    Column,
}

/// Orders accepted by the command queue.  `Command` keeps the original game
/// surface; this enum adds patrol, queueing and explicit work orders without
/// changing the replay encoding of the existing commands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrderSpec {
    Move(Vec2),
    Attack(UnitId),
    AttackMove(Vec2),
    Patrol(Vec2, Vec2),
    Hold,
    Stop,
    Harvest(Vec2),
    HarvestLumber(Vec2),
    Build(UnitId),
    Repair(UnitId),
}

#[derive(Debug, Clone)]
pub struct Production {
    pub kind: UnitKind,
    pub remaining: u32,
    /// Roster index of the unit being trained.
    pub def: u16,
    /// True when completion is waiting for supply headroom.
    pub blocked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Research {
    pub id: &'static str,
    pub remaining: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Construction {
    pub remaining: u32,
    pub total: u32,
}

/// In-flight building upgrade (`[htow] Upgrade=hkee` in `*UnitFunc.txt`).
///
/// WC3 upgrades a building in place: the same unit keeps its position, owner,
/// rally point and production queue, pays the *target* def's gold/lumber and
/// build time, and becomes the new def when the timer runs out with its
/// hit-point *ratio* preserved (a half-damaged Town Hall becomes a half-damaged
/// Keep).  Storing the target as a def index keeps the save deterministic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpgradeJob {
    /// Roster index of the form being built (`data::def_at`).
    pub to_def_index: u16,
    pub remaining: u32,
    pub total: u32,
}

#[derive(Debug, Clone)]
pub struct Unit {
    pub id: UnitId,
    pub owner: u8,
    pub kind: UnitKind,
    /// Roster index (`data::ROSTER`); the game crate keeps rendering `kind`.
    pub def_index: u16,
    pub pos: Vec2,
    pub hp: i32,
    pub max_hp: i32,
    pub mana: i32,
    pub max_mana: i32,
    pub armor: i32,
    pub damage: i32,
    pub range: f32,
    pub speed: f32,
    pub sight: f32,
    pub attack_ticks: u32,
    pub cooldown: u32,
    /// Hero level (1..=MAX_HERO_LEVEL); non-heroes keep the table level.
    pub level: u8,
    /// Cumulative hero XP.
    pub xp: u32,
    /// Unspent hero skill points.
    pub skill_points: u8,
    /// Abilities learned by this hero (`Command::LearnAbility`).
    pub learned: Vec<&'static str>,
    /// Pending altar revive completion tick (0 = not scheduled).
    pub revive_at: u64,
    /// Altar where the hero is reviving.
    pub revive_altar: Option<UnitId>,
    /// Channeling until this tick (Blizzard waves): no auto-attacks.
    pub channel_until: u64,
    pub hero_strength: i32,
    pub hero_agility: i32,
    pub hero_intelligence: i32,
    /// Sub-point agility armor carried between level-ups (milli-armor).
    pub agi_armor_milli: i32,
    /// Cooldowns of data-backed generic abilities (`AbilityData.slk`).
    pub data_cd: Vec<(&'static str, u32)>,
    /// Turn rate from `UnitData.slk` (fraction of a full turn per second).
    pub turn_rate: f32,
    pub order: Order,
    /// Shift-queued orders, executed after the current one completes.
    pub order_queue: Vec<OrderSpec>,
    pub path: Vec<(u32, u32)>,
    pub path_index: usize,
    pub production: Vec<Production>,
    pub facing: f32,
    pub alive: bool,
    /// Gold-mine position this unit is working, kept for game/HUD compatibility.
    pub harvest: Option<Vec2>,
    /// Remaining gather ticks while inside the mine.
    pub harvest_timer: u32,
    /// Gold currently carried back to the hall.
    pub carrying: i32,
    /// Lumber currently carried back to the hall.
    pub carrying_lumber: i32,
    /// Destination kept for MoveAttack/Patrol so the unit resumes after a kill.
    pub goal: Option<Vec2>,
    /// Patrol endpoints remembered while the unit engages a target.
    pub patrol: Option<(Vec2, Vec2)>,
    /// Upgrade research currently running in this building.
    pub research: Vec<Research>,
    /// Items carried by this unit (max 6).
    pub items: Vec<ItemId>,
    /// Neutral camp id for creep units (owner 2).
    pub camp: Option<u32>,
    /// Per-spell cooldowns in ticks.
    pub spell_cd: [u32; abilities::SPELL_COUNT],
    /// Thousandths of a mana point accumulated toward the next point.
    pub mana_milli: i32,
    /// Mana regeneration in thousandths of a point per tick (base + upgrades).
    pub mana_regen_milli: i32,
    /// Thousandths of a hit point accumulated toward the next point.
    pub hp_milli: i32,
    /// Life regeneration in thousandths of a point per tick (upgrade effects).
    pub hp_regen_milli: i32,
    pub rooted_until: u64,
    pub stunned_until: u64,
    pub invulnerable_until: u64,
    /// Ethereal form (`unit + 0x158` in the engine): incoming damage is
    /// multiplied by `EtherealDamageBonus[attack]` and incoming healing by
    /// `EtherealHealBonus` (`1.66` in TFT, `data::ETHEREAL_*_PERMILLE`).
    /// Transient combat state, set by the ethereal-form ability.
    pub ethereal: bool,
    /// Present while a building is under construction.
    pub construct: Option<Construction>,
    /// Present while a building is upgrading into another def (`htow` ->
    /// `hkee`).  The unit keeps its identity, position and queue; only the def
    /// changes when the job completes.
    pub upgrade: Option<UpgradeJob>,
    /// Pathing cells blocked by this building (empty for units).
    pub blocked_cells: Vec<(u32, u32)>,
    /// Spawn/rally point for trained units.
    pub rally: Option<Vec2>,
    // ---- steering scratch (not gameplay-visible) ----
    pub stuck_ticks: u32,
    pub stuck_attempts: u8,
    pub last_pos: Vec2,
}

impl Unit {
    pub fn def_id(&self) -> UnitDefId {
        UnitDefId(self.def_index)
    }

    pub fn def(&self) -> &'static UnitDef {
        def_at(self.def_index).expect("unit def index")
    }

    pub fn is_building(&self) -> bool {
        self.def().is_building()
    }

    pub fn radius(&self) -> f32 {
        self.def().stats.radius
    }

    pub fn is_complete_building(&self) -> bool {
        self.is_building() && self.construct.is_none()
    }

    /// Neutral critter (sheep, dog, crab): a passive animal that never fights.
    /// WC3 marks these with zero attack on a mobile neutral body; the sim
    /// derives the flag from the generated unit table instead of a new column.
    pub fn is_critter(&self) -> bool {
        if self.owner < 2 || self.is_building() {
            return false;
        }
        let stats = &self.def().stats;
        stats.damage <= 0 && stats.damage_dice == 0
    }

    /// A neutral unit that can guard a creep camp: mobile, armed and not a
    /// critter.  Structures (shops, taverns, mercenary camps, fountain) are
    /// excluded — they never aggro and never join a camp.
    pub fn is_camp_guard(&self) -> bool {
        if self.is_building() || self.is_critter() {
            return false;
        }
        let stats = &self.def().stats;
        stats.damage > 0 || stats.damage_dice > 0 || self.damage > 0
    }

    /// Neutral structure that trades with players (item or unit seller).
    /// `ngme`, `ngad`, `nmr8`, `ntav` on Justice: they never aggro.
    pub fn is_trade_building(&self) -> bool {
        self.is_building()
            && (!self.def().sells_items.is_empty() || !data::sold_units(self.def().id).is_empty())
    }
}

/// Per-player fog state of one pathing cell.  Matches WC3's three fog states
/// and the render mask encoding (`wc3_render::fog::FogMask` stores 0/1/2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Visibility {
    /// Never revealed to this player.
    Unexplored = 0,
    /// Revealed at some point; the terrain is remembered, units are not.
    Explored = 1,
    /// Inside a living friendly unit's sight radius right now.
    Visible = 2,
}

impl Visibility {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn from_state(state: u8) -> Self {
        match state {
            0 => Self::Unexplored,
            1 => Self::Explored,
            _ => Self::Visible,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Unexplored => "unexplored",
            Self::Explored => "explored",
            Self::Visible => "visible",
        }
    }
}

/// Last position a player saw an enemy unit (`Sim::last_seen`).  The HUD draws
/// these as last-known blips while the cell is explored but not visible.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LastSeen {
    pub pos: Vec2,
    /// Tick the position was last observed on.
    pub tick: u64,
}

#[derive(Debug, Clone)]
pub struct Camp {
    pub id: u32,
    pub anchor: Vec2,
    pub units: Vec<UnitId>,
    /// Aggro radius from the camp anchor (WC3 creep camp default 500).
    pub aggro: f32,
    /// Leash radius: a creep further than this from the anchor returns home.
    pub leash: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DestructableId(pub u32);

#[derive(Debug, Clone)]
pub struct Destructable {
    pub id: DestructableId,
    pub def_index: u16,
    pub pos: Vec2,
    pub hp: i32,
    pub max_hp: i32,
    pub alive: bool,
    pub blocked_cell: Option<(u32, u32)>,
}

impl Destructable {
    pub fn def(&self) -> &'static data::DestructableDef {
        data::DESTRUCTABLES
            .get(self.def_index as usize)
            .expect("destructable index")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemId(pub u32);

#[derive(Debug, Clone)]
pub struct Item {
    pub id: ItemId,
    pub def_index: u16,
    pub pos: Vec2,
    pub owner: Option<u8>,
    pub charges: i32,
}

impl Item {
    pub fn def(&self) -> &'static data::ItemDef {
        data::ITEMS
            .get(self.def_index as usize)
            .expect("item def index")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectileId(pub u32);

/// A missile in flight.  Ranged attacks spawn one instead of applying damage
/// instantly; damage lands on impact (`Missilespeed` travel time).
#[derive(Debug, Clone)]
pub struct Projectile {
    pub id: ProjectileId,
    pub owner: u8,
    pub source: UnitId,
    /// Homing target; `None` for point missiles.
    pub target: Option<UnitId>,
    pub pos: Vec2,
    pub start: Vec2,
    pub target_pos: Vec2,
    /// World units per second (`Missilespeed`).
    pub speed: f32,
    /// `Missilearc` as a fraction of the launch distance.
    pub arc: f32,
    pub homing: bool,
    pub damage: i32,
    pub attack: AttackType,
    /// Real missile model path (`Missileart`) for the render layer.
    pub art: &'static str,
    pub traveled: f32,
    pub total: f32,
}

impl Projectile {
    /// Parabolic height at the current point, used by renderers: `Missilearc`
    /// is the fraction of the launch distance reached at the top of the arc.
    pub fn height(&self) -> f32 {
        if self.total <= 0.0 {
            return 0.0;
        }
        let progress = (self.traveled / self.total).clamp(0.0, 1.0);
        self.arc * self.total * 4.0 * progress * (1.0 - progress)
    }
}

/// One stock entry of a shop building.
#[derive(Debug, Clone)]
pub struct ShopSlot {
    pub item: &'static str,
    pub stock: i32,
    /// Tick at which one unit restocks; 0 while the slot is already full.
    pub next_restock_tick: u64,
}

/// Runtime shop state built from `Sellitems=` and `ItemData.slk`
/// (`stockMax`/`stockRegen`).
#[derive(Debug, Clone)]
pub struct Shop {
    pub shop: UnitId,
    pub owner: u8,
    pub slots: Vec<ShopSlot>,
}

/// Interaction range for shop purchases (world units).
pub const SHOP_RANGE: f32 = 500.0;

/// Seat controller for a melee seat: local human team or data-driven AI.
#[derive(Debug, Clone, Copy)]
pub enum SeatController {
    Human,
    Ai(AiConfig),
}

/// Basic combat unit per race (the five starting "creeps" per seat).
pub fn basic_unit_for(race: Race) -> &'static str {
    match race {
        Race::Human => "hfoo",
        Race::Orc => "ogru",
        Race::Undead => "ugho",
        Race::NightElf => "earc",
        Race::Neutral => "hfoo",
    }
}

/// AI wave composition per race (data-unit ids).
pub fn ai_composition_for(race: Race) -> &'static [(&'static str, u32)] {
    match race {
        Race::Human => &[("hfoo", 3), ("hrif", 2)],
        Race::Orc => &[("ogru", 3), ("ohun", 2)],
        Race::Undead => &[("ugho", 3), ("ucry", 2)],
        Race::NightElf => &[("earc", 3), ("esen", 2)],
        Race::Neutral => &[("hfoo", 3), ("hrif", 2)],
    }
}

/// Counts produced by the melee opening (1v1..4v4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeleeSetup {
    pub player_hall: UnitId,
    pub enemy_hall: UnitId,
    pub player_mine: Vec2,
    pub enemy_mine: Vec2,
    pub player_workers: usize,
    pub enemy_workers: usize,
    /// Neutral creeps guarding the player's gold mine.
    pub player_creeps: usize,
    /// Neutral creeps guarding the enemy's gold mine.
    pub enemy_creeps: usize,
    pub seats: usize,
    pub team_size: usize,
    pub halls: usize,
    pub start_mines: usize,
    pub expansion_mines: usize,
    pub workers: usize,
    pub creeps: usize,
    pub camps: usize,
}

#[derive(Debug, Clone)]
pub struct GoldMine {
    pub pos: Vec2,
    pub remaining: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ability {
    Heal(i32),
    Fireball(i32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Move {
        units: Vec<UnitId>,
        to: Vec2,
    },
    Attack {
        units: Vec<UnitId>,
        target: UnitId,
    },
    Stop {
        units: Vec<UnitId>,
    },
    /// Move toward `to`, engaging any enemy that comes into sight on the way.
    MoveAttack {
        units: Vec<UnitId>,
        to: Vec2,
    },
    /// Hold position: stop and keep attacking targets in range.
    Hold {
        units: Vec<UnitId>,
    },
    /// Send peasants to a gold mine; they pay out gold while standing on it.
    Harvest {
        units: Vec<UnitId>,
        to: Vec2,
    },
    /// Send peasants to a tree; they chop it and return lumber.
    HarvestLumber {
        units: Vec<UnitId>,
        to: Vec2,
    },
    /// Cancel a queued production item and refund its cost.
    CancelTrain {
        building: UnitId,
        index: usize,
    },
    Train {
        building: UnitId,
        kind: UnitKind,
    },
    Build {
        builder: UnitId,
        kind: UnitKind,
        at: Vec2,
    },
    Cast {
        unit: UnitId,
        ability: Ability,
        target: Option<UnitId>,
    },
    /// Spend one hero skill point on an ability id from the hero's list.
    LearnAbility {
        unit: UnitId,
        ability: &'static str,
    },
    /// Schedule a dead hero's altar revive (700 g, 150 s).
    ReviveHero {
        hero: UnitId,
        altar: UnitId,
    },
    /// Send workers to repair a damaged friendly building.
    Repair {
        units: Vec<UnitId>,
        target: UnitId,
    },
    /// Patrol between two points (the card's Patrol order on the command
    /// stream; `OrderSpec::Patrol` single-unit form).
    Patrol {
        units: Vec<UnitId>,
        a: Vec2,
        b: Vec2,
    },
    /// Start a building upgrade in place (`[htow] Upgrade=hkee`).
    UpgradeBuilding {
        building: UnitId,
        to: &'static str,
    },
    /// Research a `Researches=` upgrade in a building.
    Research {
        building: UnitId,
        upgrade: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    UnitDied(UnitId),
    UnitTrained(UnitId),
    BuildingComplete(UnitId),
    Damage {
        from: UnitId,
        to: UnitId,
        amount: i32,
    },
    AbilityUsed {
        caster: UnitId,
        ability: Ability,
    },
    /// A spell left the caster's hands.
    SpellCast {
        caster: UnitId,
        spell: Spell,
        target: SpellTarget,
    },
    /// A spell dealt damage.
    SpellHit {
        caster: UnitId,
        spell: Spell,
        to: UnitId,
        amount: i32,
    },
    /// A spell healed its target.
    SpellHeal {
        caster: UnitId,
        spell: Spell,
        to: UnitId,
        amount: i32,
    },
    /// An upgrade finished researching.
    UpgradeResearched {
        owner: u8,
        id: &'static str,
    },
    /// A building finished upgrading into another def (`htow` -> `hkee`).
    /// Audio/HUD consume this instead of the generic build-complete sound.
    UpgradeComplete {
        building: UnitId,
        to: &'static str,
    },
    /// A tree/destructable was destroyed.
    DestructableDestroyed {
        id: DestructableId,
    },
    /// A worker delivered lumber.
    LumberDeposited {
        owner: u8,
        amount: i32,
    },
    /// A worker delivered gold.
    GoldDeposited {
        owner: u8,
        amount: i32,
    },
    /// A missile reached its impact point.
    ProjectileHit {
        from: UnitId,
        to: UnitId,
        amount: i32,
    },
    /// A unit bought an item from a shop building.
    ItemBought {
        buyer: UnitId,
        shop: UnitId,
        item: &'static str,
    },
    /// A data-backed generic ability was cast.
    DataAbilityCast {
        caster: UnitId,
        ability: &'static str,
        target: Option<UnitId>,
    },
    /// A generic data ability applied damage or healing.
    DataAbilityHit {
        caster: UnitId,
        ability: &'static str,
        to: UnitId,
        amount: i32,
        heal: bool,
    },
    /// A hero finished altar revival.
    HeroRevived {
        hero: UnitId,
        altar: UnitId,
    },
    /// The built-in AI started a new attack wave.
    AiWave {
        owner: u8,
        wave: u32,
    },
    /// A neutral creep kill paid its WC3 gold bounty to `owner`.
    Bounty {
        owner: u8,
        unit: UnitId,
        gold: i32,
    },
    Victory(u8),
    Error(&'static str),
}

pub struct Sim {
    pub tick: u64,
    pub seed: u64,
    pub units: Vec<Unit>,
    pub next_id: u32,
    pub next_item_id: u32,
    pub next_destructable_id: u32,
    pub next_camp_id: u32,
    pub next_projectile_id: u32,
    pub resources: [i32; 2],
    /// Lumber per player (WC3 second resource).
    pub lumber: [i32; 2],
    pub grid: Grid,
    pub visibility: Vec<u8>,
    pub explored: [Vec<u8>; 2],
    /// Last position each player observed an enemy unit on (`unit id -> pos`).
    /// Updated by `update_fog`; the HUD draws these as last-known blips and
    /// they survive a save/load round trip by being rebuilt from live sight.
    pub last_seen: [BTreeMap<u32, LastSeen>; 2],
    pub events: Vec<Event>,
    /// `SPAWN_ADJUST def=<id> from=<cell> to=<cell>` lines: every spawn whose
    /// requested point was moved so the unit stands outside a footprint or on
    /// walkable ground. Bounded by [`SPAWN_LOG_LIMIT`]; the overflow counter
    /// keeps a truncated log from looking complete.
    pub spawn_log: Vec<String>,
    /// Adjustments dropped after [`SPAWN_LOG_LIMIT`].
    pub spawn_log_dropped: u64,
    pub gold_mines: Vec<GoldMine>,
    pub destructables: Vec<Destructable>,
    pub camps: Vec<Camp>,
    pub projectiles: Vec<Projectile>,
    pub shops: Vec<Shop>,
    pub items: Vec<Item>,
    pub pending_casts: Vec<abilities::PendingCast>,
    pub ai: Vec<AiState>,
    /// Squad controllers (`Sim::enable_squads`); empty means the wave AI owns
    /// army control.
    pub squads: Vec<squad::SquadManager>,
    /// Live buffs, sorted by `(unit, id)` so the order is deterministic.
    pub buffs: Vec<buffs::Buff>,
    /// Next buff id handed out by `Sim::add_buff`.
    pub next_buff_id: u32,
    /// Last order issued to a unit on a given tick (spam guard), keyed by unit.
    pub issue_stamps: BTreeMap<u32, (OrderSpec, u64)>,
    /// Per-player researched upgrade levels (`upgrade id -> level`).
    pub upgrades: [BTreeMap<&'static str, u8>; 2],
    /// Map income stipend (1 gold / half second).  Off by default: WC3 melee
    /// income is produced by workers only, and a drip that runs for both
    /// players hides every harvest bug behind a rising counter.  Scripted demo
    /// tests that assert on the stipend opt in explicitly.
    pub passive_income: bool,
    /// Balance ruleset; the game selects it from the loaded install.
    pub ruleset: Ruleset,
    /// Typed gameplay constants from `wc3_re::gameplay` (MiscGame.txt /
    /// GameplayConstants), consumed by movement, hero leveling and combat.
    pub gameplay: GameplayConfig,
    rng_state: u64,
    id_to_index: Vec<u32>,
    pathfinder: nav::PathFinder,
    path_requests: Vec<(u32, u32, u32)>,
    path_budget: u32,
    flow_cache: BTreeMap<(u32, u32), (Vec<u32>, u64)>,
    spatial: spatial::SpatialHash,
    pub(crate) spatial_buffer: Vec<u32>,
    fog_offsets: Vec<Vec<(i32, i32)>>,
    fog_stamp: Vec<u32>,
    /// Map script machine attached with [`Sim::attach_script`].  Between ticks
    /// the VM keeps its own scratch `Sim`; [`Sim::with_script`] swaps the live
    /// state in for the duration of a script call.  `None` means `Sim::step`
    /// runs exactly the simulation systems with no script work and no branch
    /// cost beyond the `is_none` check.
    script: Option<Box<crate::jass::JassVm>>,
}

/// Clamps a speed to configured limits; a max below the default minimum wins.
fn clamp_speed(value: f32, min: f32, max: f32) -> f32 {
    if min > max {
        value.min(max)
    } else {
        value.clamp(min, max)
    }
}

/// Deterministic FNV-1a hash of a string (buff tags, ability ids).
pub fn fnv_str(value: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in value.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

/// Rotates `current` toward `target` by at most `max_delta` radians,
/// normalising across the -PI..PI wrap.
pub fn rotate_towards(current: f32, target: f32, max_delta: f32) -> f32 {
    if max_delta <= 0.0 {
        return target;
    }
    let mut diff = (target - current) % std::f32::consts::TAU;
    if diff > std::f32::consts::PI {
        diff -= std::f32::consts::TAU;
    } else if diff < -std::f32::consts::PI {
        diff += std::f32::consts::TAU;
    }
    if diff.abs() <= max_delta {
        target
    } else {
        current + diff.signum() * max_delta
    }
}

impl Sim {
    pub fn new(seed: u64, width: u32, height: u32) -> Self {
        let grid = Grid::new(width, height, CELL_SIZE);
        let cells = width as usize * height as usize;
        let mut fog_offsets = Vec::with_capacity(25);
        // Sight radii reach ~1800 world units (56 pathing cells); the old
        // 24-cell cap left a tiny visible bubble and the map read as black.
        for radius in 0..=64i32 {
            let mut offsets = Vec::new();
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx * dx + dy * dy <= radius * radius {
                        offsets.push((dx, dy));
                    }
                }
            }
            fog_offsets.push(offsets);
        }
        Self {
            tick: 0,
            seed,
            units: Vec::new(),
            next_id: 1,
            next_item_id: 1,
            next_destructable_id: 1,
            next_camp_id: 1,
            next_projectile_id: 1,
            resources: [1000, 1000],
            lumber: [1000, 1000],
            visibility: vec![0; cells],
            explored: [vec![0; cells], vec![0; cells]],
            last_seen: [BTreeMap::new(), BTreeMap::new()],
            grid,
            events: Vec::new(),
            spawn_log: Vec::new(),
            spawn_log_dropped: 0,
            gold_mines: Vec::new(),
            destructables: Vec::new(),
            camps: Vec::new(),
            projectiles: Vec::new(),
            shops: Vec::new(),
            items: Vec::new(),
            pending_casts: Vec::new(),
            ai: Vec::new(),
            squads: Vec::new(),
            buffs: Vec::new(),
            next_buff_id: 1,
            issue_stamps: BTreeMap::new(),
            upgrades: [BTreeMap::new(), BTreeMap::new()],
            passive_income: false,
            ruleset: Ruleset::Tft,
            gameplay: GameplayConfig::default(),
            rng_state: seed | 1,
            id_to_index: vec![u32::MAX],
            pathfinder: nav::PathFinder::new(width, height),
            path_requests: Vec::new(),
            path_budget: PATH_BUDGET,
            flow_cache: BTreeMap::new(),
            spatial: spatial::SpatialHash::new(
                256.0,
                width as f32 * CELL_SIZE,
                height as f32 * CELL_SIZE,
            ),
            spatial_buffer: Vec::with_capacity(64),
            fog_offsets,
            fog_stamp: vec![0; cells],
            script: None,
        }
    }

    // ------------------------------------------------------------------
    // Map script attachment
    // ------------------------------------------------------------------

    /// Attaches a compiled map script bundle: every subsequent [`Sim::step`]
    /// runs the simulation systems and then advances the JASS VM by one tick
    /// (30 Hz grid, the VM's 300 000-instruction budget, deterministic).
    ///
    /// The VM already owns a `Sim` field, so the live simulation is swapped
    /// into it only while the script runs (see [`Sim::with_script`]); the tick
    /// hook is a single `is_none` check when no script is attached, which keeps
    /// `Sim`-only callers byte-for-byte unchanged.
    ///
    /// Runs the map entry points (`config` + `main`) with the live state in
    /// place and returns the report, so the caller can verify the script
    /// actually ran instead of assuming it.
    pub fn attach_script(
        &mut self,
        bundle: &crate::jass::ScriptBundle,
        seed: u64,
    ) -> Result<crate::jass::EntryReport, Vec<crate::jass::CompileError>> {
        // The VM needs a `Sim` at construction; it is the placeholder that
        // lives inside the VM between script calls and is replaced by the live
        // state in `with_script`.
        let scratch = Sim::new(seed, self.grid.width, self.grid.height);
        let vm = crate::jass::JassVm::new(&bundle.program, scratch, seed)?;
        self.script = Some(Box::new(vm));
        let report = self
            .with_script(|vm| vm.run_map_entry_points())
            .expect("script was attached immediately above");
        Ok(report)
    }

    /// True when a map script bundle is attached to this simulation.
    pub fn script_attached(&self) -> bool {
        self.script.is_some()
    }

    /// Script clock in seconds, when a script is attached.
    pub fn script_time(&self) -> Option<f64> {
        self.script.as_ref().map(|vm| vm.time)
    }

    /// Read-only view of the attached script machine. The VM's own `sim` field
    /// is the scratch world between ticks; read game state through `Sim`.
    pub fn script(&self) -> Option<&crate::jass::JassVm> {
        self.script.as_deref()
    }

    /// Runs `run` with the attached VM owning the live simulation state.
    ///
    /// The VM's own `Sim` field is swapped for the real one for the duration of
    /// the call, so script natives read and mutate the same units, resources
    /// and events the caller sees.  Returns `None` without a script attached.
    /// `run` must not call [`Sim::step`] (the script tick is already running).
    pub fn with_script<R>(
        &mut self,
        run: impl FnOnce(&mut crate::jass::JassVm) -> R,
    ) -> Option<R> {
        let mut vm = self.script.take()?;
        std::mem::swap(self, &mut vm.sim);
        let result = run(&mut vm);
        std::mem::swap(self, &mut vm.sim);
        self.script = Some(vm);
        Some(result)
    }

    /// One script tick after the simulation systems: the events this step
    /// raised are dispatched, timers/resumed threads run, and region/range
    /// occupancy is recomputed.  Inert when no script is attached.
    fn advance_script(&mut self) {
        if self.script.is_none() {
            return;
        }
        let dt = 1.0 / TICKS_PER_SECOND as f64;
        let _ = self.with_script(|vm| {
            vm.dispatch_pending_events();
            vm.advance(dt);
            vm.track_regions();
            vm.track_unit_ranges();
        });
    }

    // ------------------------------------------------------------------
    // Entity access
    // ------------------------------------------------------------------

    pub fn spawn(&mut self, owner: u8, kind: UnitKind, pos: Vec2) -> UnitId {
        let index = def_index(kind.canonical_id()).expect("canonical def");
        self.spawn_def_index(owner, index, pos, true)
    }

    /// Data-driven spawn: any roster entry, complete and idle.
    pub fn spawn_def_id(&mut self, owner: u8, def_id: UnitDefId, pos: Vec2) -> UnitId {
        self.spawn_def_index(owner, def_id.0, pos, true)
    }

    pub fn spawn_def(&mut self, owner: u8, def_id: &str, pos: Vec2) -> Option<UnitId> {
        let index = def_index(def_id)?;
        Some(self.spawn_def_index(owner, index, pos, true))
    }

    /// Spawns a def at the exact world position without the walkable-cell
    /// snap used for player spawns.  Map placements (`war3mapUnits.doo`) and
    /// script-created units are authoritative about where they stand.
    pub fn spawn_def_exact(&mut self, owner: u8, def_id: &str, pos: Vec2) -> Option<UnitId> {
        let index = def_index(def_id)?;
        Some(self.spawn_def_index_at(owner, index, pos, true, false))
    }

    /// Replaces the walkable grid with a real WPM map at its native cell
    /// size (32 world units), resizing pathfinding and fog buffers.
    pub fn set_pathing(&mut self, width: u32, height: u32, cell_size: f32, walkable: Vec<bool>) {
        self.grid = wc3_path::Grid::from_walkable(width, height, cell_size, walkable);
        self.pathfinder.resize(width, height);
        self.path_requests.clear();
        self.flow_cache.clear();
        self.spatial =
            spatial::SpatialHash::new(256.0, width as f32 * cell_size, height as f32 * cell_size);
        let cells = width as usize * height as usize;
        self.visibility = vec![0; cells];
        self.explored = [vec![0; cells], vec![0; cells]];
        self.last_seen = [BTreeMap::new(), BTreeMap::new()];
        self.fog_stamp = vec![0; cells];
    }

    /// Nearest walkable world position to `pos` (`None` when the grid is
    /// fully blocked nearby).
    pub fn walkable_pos_near(&self, pos: Vec2, max_cells: u32) -> Option<Vec2> {
        let cell = self.grid.world_to_cell(pos);
        let cell = nav::nearest_walkable(&self.grid, cell, max_cells)?;
        Some(self.grid.cell_to_world(cell.0, cell.1))
    }

    fn spawn_def_index(&mut self, owner: u8, def_index: u16, pos: Vec2, complete: bool) -> UnitId {
        self.spawn_def_index_at(owner, def_index, pos, complete, true)
    }

    fn spawn_def_index_at(
        &mut self,
        owner: u8,
        def_index: u16,
        pos: Vec2,
        complete: bool,
        snap_walkable: bool,
    ) -> UnitId {
        let def = def_at(def_index).expect("def index");
        let stats = def.stats;
        let bonus = self.upgrade_bonuses(owner.min(1), def);
        let percent_damage = stats.damage * bonus.damage_permille / 1000;
        let percent_hp = stats.hp * bonus.hp_permille / 1000;
        let id = UnitId(self.next_id);
        self.next_id += 1;
        let index = self.units.len();
        if self.id_to_index.len() <= id.0 as usize {
            self.id_to_index.resize(id.0 as usize + 1, u32::MAX);
        }
        self.id_to_index[id.0 as usize] = index as u32;
        let mut blocked_cells = Vec::new();
        if stats.is_building {
            blocked_cells = self.footprint_cells(pos, self.footprint_side_cells(def.footprint));
            for &(cx, cy) in &blocked_cells {
                self.grid.set_walkable(cx, cy, false);
            }
            self.flow_cache.clear();
        }
        let hp = if complete {
            stats.hp
        } else {
            (stats.hp / 10).max(1)
        };
        let mut pos = pos;
        if !stats.is_building && snap_walkable {
            let cell = self.grid.world_to_cell(pos);
            if !self.grid.walkable(cell.0, cell.1) {
                if let Some(cell) = nav::nearest_walkable(&self.grid, cell, 8) {
                    pos = self.grid.cell_to_world(cell.0, cell.1);
                }
            }
        }
        self.units.push(Unit {
            id,
            owner: owner.min(2),
            kind: def.kind,
            def_index,
            pos,
            hp: hp + bonus.hp + hp * bonus.hp_permille / 1000,
            max_hp: stats.hp + bonus.hp + percent_hp,
            mana: if complete {
                stats.max_mana + bonus.mana
            } else {
                0
            },
            max_mana: stats.max_mana + bonus.mana,
            armor: stats.armor + bonus.armor,
            damage: stats.damage + bonus.damage + percent_damage,
            range: stats.range + bonus.range,
            speed: clamp_speed(
                stats.speed + bonus.speed,
                if stats.is_building {
                    self.gameplay.min_bldg_speed
                } else {
                    self.gameplay.min_unit_speed
                },
                if stats.is_building {
                    self.gameplay.max_bldg_speed
                } else {
                    self.gameplay.max_unit_speed
                },
            ),
            sight: stats.sight + bonus.sight,
            attack_ticks: stats.attack_ticks,
            cooldown: 0,
            level: if def.hero { 1 } else { stats.level.max(1) },
            xp: 0,
            skill_points: if def.hero { 1 } else { 0 },
            learned: Vec::new(),
            revive_at: 0,
            revive_altar: None,
            channel_until: 0,
            hero_strength: stats.hero_strength,
            hero_agility: stats.hero_agility,
            hero_intelligence: stats.hero_intelligence,
            agi_armor_milli: 0,
            data_cd: Vec::new(),
            turn_rate: stats.turn_rate,
            order: Order::Idle,
            order_queue: Vec::new(),
            path: Vec::new(),
            path_index: 0,
            production: Vec::new(),
            research: Vec::new(),
            items: Vec::new(),
            camp: None,
            facing: 0.0,
            alive: true,
            harvest: None,
            harvest_timer: 0,
            carrying: 0,
            carrying_lumber: 0,
            goal: None,
            patrol: None,
            spell_cd: [0; abilities::SPELL_COUNT],
            mana_milli: 0,
            mana_regen_milli: stats.mana_regen_milli + bonus.mana_regen_milli,
            hp_milli: 0,
            hp_regen_milli: bonus.hp_regen_milli,
            rooted_until: 0,
            stunned_until: 0,
            invulnerable_until: 0,
            ethereal: false,
            construct: if stats.is_building && !complete {
                Some(Construction {
                    remaining: stats.build_ticks.max(1),
                    total: stats.build_ticks.max(1),
                })
            } else {
                None
            },
            blocked_cells,
            upgrade: None,
            rally: None,
            stuck_ticks: 0,
            stuck_attempts: 0,
            last_pos: pos,
        });
        if !def.sells_items.is_empty() {
            self.register_shop(id, def.sells_items);
        }
        id
    }

    pub fn unit(&self, id: UnitId) -> Option<&Unit> {
        self.id_to_index
            .get(id.0 as usize)
            .and_then(|&index| {
                if index == u32::MAX {
                    None
                } else {
                    self.units.get(index as usize)
                }
            })
            .filter(|unit| unit.id == id)
    }

    pub fn unit_mut(&mut self, id: UnitId) -> Option<&mut Unit> {
        let index = *self.id_to_index.get(id.0 as usize)?;
        if index == u32::MAX {
            return None;
        }
        self.units.get_mut(index as usize)
    }

    pub(crate) fn unit_index(&self, id: UnitId) -> Option<usize> {
        let index = *self.id_to_index.get(id.0 as usize)?;
        if index == u32::MAX {
            None
        } else {
            Some(index as usize)
        }
    }

    pub fn alive_units(&self) -> impl Iterator<Item = &Unit> {
        self.units.iter().filter(|u| u.alive)
    }

    pub fn units_of(&self, owner: u8) -> impl Iterator<Item = &Unit> {
        self.units
            .iter()
            .filter(move |u| u.alive && u.owner == owner)
    }

    pub fn add_gold_mine(&mut self, pos: Vec2, amount: i32) {
        self.gold_mines.push(GoldMine {
            pos,
            remaining: amount.max(0),
        });
    }

    pub fn nearest_gold_mine(&self, pos: Vec2) -> Option<Vec2> {
        self.gold_mines
            .iter()
            .filter(|mine| mine.remaining > 0)
            .min_by(|a, b| {
                a.pos
                    .distance(pos)
                    .partial_cmp(&b.pos.distance(pos))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|mine| mine.pos)
    }

    // ------------------------------------------------------------------
    // Tick
    // ------------------------------------------------------------------

    pub fn step(&mut self, commands: &[Command]) {
        self.events.clear();
        self.path_budget = PATH_BUDGET;
        for command in commands {
            self.apply(command);
        }
        self.advance_timers();
        self.advance_revival();
        self.advance_orders();
        self.advance_movement();
        self.advance_combat();
        self.advance_projectiles();
        self.advance_upgrades();
        self.advance_production();
        self.advance_research();
        self.advance_construction();
        self.advance_repairs();
        self.advance_economy();
        self.advance_shops();
        // Item maintenance on the real 30 Hz grid: dead carriers drop their
        // backpack (allocation-free cursor walk, see `items.rs`).
        self.items_tick();
        self.advance_ai();
        self.advance_squads();
        self.advance_creeps();
        self.advance_critters();
        self.tick_buffs();
        self.update_fog();
        self.check_victory();
        // Map scripts run on the same 30 Hz grid as the systems above; with no
        // bundle attached this branch does nothing.
        self.advance_script();
        self.tick += 1;
    }

    fn apply(&mut self, command: &Command) {
        match command {
            Command::Move { units, to } => {
                // WC3 smart order: a worker right-clicked onto (or onto the
                // footprint of) a gold mine or a tree gathers it instead of
                // walking.  Non-workers in the same group keep the move.
                let mine_target = self
                    .gold_mines
                    .iter()
                    .map(|mine| mine.pos)
                    .filter(|pos| to.distance(*pos) <= 256.0)
                    .filter(|pos| to.distance(*pos) <= self.gather_gate(*pos, 56.0))
                    .min_by(|a, b| {
                        a.distance(*to)
                            .partial_cmp(&b.distance(*to))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                let tree_target = self
                    .destructables
                    .iter()
                    .filter(|tree| tree.alive)
                    .map(|tree| tree.pos)
                    .filter(|pos| to.distance(*pos) <= 256.0)
                    .filter(|pos| to.distance(*pos) <= self.gather_gate(*pos, 64.0))
                    .min_by(|a, b| {
                        a.distance(*to)
                            .partial_cmp(&b.distance(*to))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                let resource = mine_target
                    .map(|pos| (true, pos))
                    .or_else(|| tree_target.map(|pos| (false, pos)));
                let mut walkers: Vec<UnitId> = Vec::new();
                for &id in units {
                    let Some(index) = self.unit_index(id) else {
                        continue;
                    };
                    let unit = &self.units[index];
                    if let (Some((gold, pos)), true) =
                        (resource, unit.alive && !unit.is_building() && unit.def().stats.is_worker)
                    {
                        self.units[index].order_queue.clear();
                        if gold {
                            self.start_order(index, OrderSpec::Harvest(pos));
                        } else {
                            self.start_order(index, OrderSpec::HarvestLumber(pos));
                        }
                    } else if unit.alive {
                        walkers.push(id);
                    }
                }
                if !walkers.is_empty() {
                    self.issue_group(&walkers, *to, Formation::Box, false);
                }
            }
            Command::Attack { units, target } => {
                for &id in units {
                    if let Some(index) = self.unit_index(id) {
                        if self.units[index].alive && !self.units[index].is_building() {
                            self.units[index].order_queue.clear();
                            self.start_order(index, OrderSpec::Attack(*target));
                        }
                    }
                }
            }
            Command::Stop { units } => {
                for &id in units {
                    if let Some(index) = self.unit_index(id) {
                        self.units[index].order_queue.clear();
                        self.units[index].path.clear();
                        self.units[index].path_index = 0;
                        self.units[index].order = Order::Idle;
                        self.units[index].goal = None;
                        self.units[index].patrol = None;
                        self.units[index].harvest = None;
                    }
                }
            }
            Command::MoveAttack { units, to } => {
                self.issue_group(units, *to, Formation::Box, true);
            }
            Command::Hold { units } => {
                for &id in units {
                    if let Some(index) = self.unit_index(id) {
                        self.units[index].order_queue.clear();
                        self.units[index].path.clear();
                        self.units[index].path_index = 0;
                        self.units[index].order = Order::Hold;
                        self.units[index].goal = None;
                        self.units[index].patrol = None;
                    }
                }
            }
            Command::Harvest { units, to } => {
                for &id in units {
                    if let Some(index) = self.unit_index(id) {
                        let unit = &self.units[index];
                        if unit.alive && !unit.is_building() && unit.def().stats.is_worker {
                            self.units[index].order_queue.clear();
                            self.start_order(index, OrderSpec::Harvest(*to));
                        }
                    }
                }
            }
            Command::HarvestLumber { units, to } => {
                for &id in units {
                    if let Some(index) = self.unit_index(id) {
                        let unit = &self.units[index];
                        if unit.alive && !unit.is_building() && unit.def().stats.is_worker {
                            self.units[index].order_queue.clear();
                            self.start_order(index, OrderSpec::HarvestLumber(*to));
                        }
                    }
                }
            }
            Command::CancelTrain { building, index } => {
                let Some(b_index) = self.unit_index(*building) else {
                    return;
                };
                if *index >= self.units[b_index].production.len() {
                    return;
                }
                let item = self.units[b_index].production.remove(*index);
                let (cost, lumber) = def_at(item.def)
                    .map(|d| (d.stats.cost, d.stats.lumber_cost))
                    .unwrap_or((0, 0));
                let owner = self.units[b_index].owner as usize;
                if owner < self.resources.len() {
                    self.resources[owner] += cost;
                    self.lumber[owner] += lumber;
                }
                self.events.push(Event::Error("production cancelled"));
            }
            Command::Train { building, kind } => {
                let index = match def_index(kind.canonical_id()) {
                    Some(index) => index,
                    None => return,
                };
                let _ = self.enqueue_production(*building, index, true);
            }
            Command::LearnAbility { unit, ability } => {
                let _ = self.learn_ability(*unit, ability);
            }
            Command::ReviveHero { hero, altar } => {
                // Altar revive card: charge 700 g and schedule the 150 s timer.
                let _ = self.revive_hero(*hero, *altar);
            }
            Command::Repair { units, target } => {
                for &id in units {
                    if let Some(index) = self.unit_index(id) {
                        if self.units[index].alive
                            && !self.units[index].is_building()
                            && self.units[index].def().stats.is_worker
                        {
                            self.units[index].order_queue.clear();
                            self.start_order(index, OrderSpec::Repair(*target));
                        }
                    }
                }
            }
            Command::Build { builder, kind, at } => {
                // Legacy instant placement used by the scripted map HUD and its
                // tests; the game bridge uses `build_instant` for an atomic
                // card placement with the explicit DENY log.
                let _ = self.build_instant_impl(*builder, *kind, *at, "not enough gold");
            }
            Command::Patrol { units, a, b } => {
                self.patrol(units, *a, *b);
            }
            Command::UpgradeBuilding { building, to } => {
                let _ = self.upgrade_building(*building, to);
            }
            Command::Research { building, upgrade } => {
                let _ = self.research_upgrade(*building, upgrade);
            }
            Command::Cast {
                unit,
                ability,
                target,
            } => {
                let Some(index) = self.unit_index(*unit) else {
                    return;
                };
                if !self.units[index].alive {
                    return;
                }
                match ability {
                    Ability::Heal(amount) => {
                        if let Some(target_id) = target {
                            self.heal_unit(*target_id, *amount);
                        }
                    }
                    Ability::Fireball(damage) => {
                        if let Some(target_id) = target {
                            let _ =
                                self.apply_damage(*unit, *target_id, *damage, AttackType::Magic);
                        }
                    }
                }
                self.events.push(Event::AbilityUsed {
                    caster: *unit,
                    ability: *ability,
                });
            }
        }
    }

    // ------------------------------------------------------------------
    // Public order API (queueable)
    // ------------------------------------------------------------------

    /// Replaces the unit's current order.  Buildings ignore movement orders.
    ///
    /// A repeated identical click inside one tick is dropped (port of the
    /// "skip spammed smart orders" guard in
    /// `W3AndroidEngine/.../simulation/CUnit.java:2664-2669`), so a UI that
    /// fires several move orders per tick does not restart the path each time.
    pub fn issue(&mut self, units: &[UnitId], spec: OrderSpec) {
        if let OrderSpec::Move(to) | OrderSpec::AttackMove(to) = spec {
            let attack = matches!(spec, OrderSpec::AttackMove(_));
            let stamp = if attack {
                OrderSpec::AttackMove(to)
            } else {
                OrderSpec::Move(to)
            };
            let targets: Vec<UnitId> = units
                .iter()
                .copied()
                .filter(|id| !self.is_order_spam(*id, &stamp))
                .collect();
            self.issue_group(&targets, to, Formation::Box, attack);
            for id in &targets {
                self.stamp_order(*id, stamp);
            }
            return;
        }
        for &id in units {
            if let Some(index) = self.unit_index(id) {
                if !self.units[index].alive || self.units[index].is_building() {
                    continue;
                }
                if self.is_order_spam(id, &spec) {
                    continue;
                }
                self.units[index].order_queue.clear();
                self.start_order(index, spec);
                self.stamp_order(id, spec);
            }
        }
    }

    /// True when `spec` was already issued to this unit on the current tick.
    fn is_order_spam(&self, unit: UnitId, spec: &OrderSpec) -> bool {
        self.issue_stamps
            .get(&unit.0)
            .map(|(last, tick)| *tick == self.tick && last == spec)
            .unwrap_or(false)
    }

    fn stamp_order(&mut self, unit: UnitId, spec: OrderSpec) {
        self.issue_stamps.insert(unit.0, (spec, self.tick));
    }

    /// Appends an order to the shift-queue (max [`ORDER_QUEUE_MAX`]).
    pub fn queue_order(&mut self, units: &[UnitId], spec: OrderSpec) {
        for &id in units {
            if let Some(index) = self.unit_index(id) {
                let unit = &mut self.units[index];
                if !unit.alive || unit.is_building() {
                    continue;
                }
                if unit.order_queue.len() < ORDER_QUEUE_MAX {
                    if matches!(unit.order, Order::Idle | Order::Hold)
                        && unit.order_queue.is_empty()
                    {
                        self.start_order(index, spec);
                    } else {
                        unit.order_queue.push(spec);
                    }
                }
            }
        }
    }

    /// Group move with an explicit formation.
    pub fn move_group(&mut self, units: &[UnitId], to: Vec2, formation: Formation) {
        self.issue_group(units, to, formation, false);
    }

    pub fn attack_move(&mut self, units: &[UnitId], to: Vec2) {
        self.issue_group(units, to, Formation::Box, true);
    }

    pub fn patrol(&mut self, units: &[UnitId], a: Vec2, b: Vec2) {
        for &id in units {
            if let Some(index) = self.unit_index(id) {
                if !self.units[index].alive || self.units[index].is_building() {
                    continue;
                }
                self.units[index].order_queue.clear();
                self.units[index].goal = Some(b);
                self.start_order(index, OrderSpec::Patrol(a, b));
            }
        }
    }

    /// Adds one leg to a patrol route (port of `CUnit.beginOrder` patrol point
    /// accumulation, `W3AndroidEngine/.../simulation/CUnit.java:2679-2687`).
    ///
    /// The simulation models a patrol as two endpoints, so a repeated patrol
    /// click keeps the previous endpoint and moves the far one to `point`,
    /// which is the same "walk the route you clicked" behaviour.
    pub fn extend_patrol(&mut self, units: &[UnitId], point: Vec2) {
        for &id in units {
            let Some(index) = self.unit_index(id) else {
                continue;
            };
            if !self.units[index].alive || self.units[index].is_building() {
                continue;
            }
            let next = match self.units[index].order {
                Order::Patrol { b, .. } => OrderSpec::Patrol(b, point),
                _ => {
                    let here = self.units[index].pos;
                    OrderSpec::Patrol(here, point)
                }
            };
            self.units[index].goal = Some(point);
            self.start_order(index, next);
        }
    }

    /// Drops every queued order on `units` and returns how many were dropped.
    /// The analogue of the donor `onCancelFromQueue` callback: queued work is
    /// released before a replacement order starts.
    pub fn cancel_queued_orders(&mut self, units: &[UnitId]) -> usize {
        let mut cancelled = 0;
        for &id in units {
            if let Some(index) = self.unit_index(id) {
                cancelled += self.units[index].order_queue.len();
                self.units[index].order_queue.clear();
            }
        }
        cancelled
    }

    pub fn hold(&mut self, units: &[UnitId]) {
        for &id in units {
            if let Some(index) = self.unit_index(id) {
                self.units[index].order_queue.clear();
                self.units[index].path.clear();
                self.units[index].order = Order::Hold;
                self.units[index].goal = None;
                self.units[index].harvest = None;
            }
        }
    }

    pub fn stop(&mut self, units: &[UnitId]) {
        for &id in units {
            if let Some(index) = self.unit_index(id) {
                self.units[index].order_queue.clear();
                self.units[index].path.clear();
                self.units[index].order = Order::Idle;
                self.units[index].goal = None;
                self.units[index].harvest = None;
            }
        }
    }

    fn issue_group(&mut self, units: &[UnitId], to: Vec2, formation: Formation, attack_move: bool) {
        // Snap the click itself: a target in water or on a cliff must not
        // invalidate every formation slot.
        let to = self
            .nearest_walkable_cell(self.grid.world_to_cell(to), 64)
            .map(|cell| self.grid.cell_to_world(cell.0, cell.1))
            .unwrap_or(to);
        let mut entries: Vec<(usize, Vec2)> = units
            .iter()
            .filter_map(|&id| {
                let index = self.unit_index(id)?;
                let unit = &self.units[index];
                (unit.alive && !unit.is_building()).then_some((index, unit.pos))
            })
            .collect();
        if entries.is_empty() {
            return;
        }
        // Deterministic slot assignment: entity order, nearest unused slot by
        // squared distance (ties resolved by slot index).
        entries.sort_by_key(|(index, _)| *index);
        let centroid =
            entries.iter().fold(Vec2::ZERO, |acc, (_, pos)| acc + *pos) / entries.len() as f32;
        let slots = match formation {
            Formation::Box => nav::formation_slots(to, to - centroid, entries.len(), 72.0),
            Formation::Line => {
                let dir = (to - centroid).normalize_or_zero();
                let dir = if dir == Vec2::ZERO {
                    Vec2::new(1.0, 0.0)
                } else {
                    dir
                };
                let side = Vec2::new(-dir.y, dir.x);
                (0..entries.len())
                    .map(|i| to + side * ((i as f32) - (entries.len() as f32 - 1.0) * 0.5) * 64.0)
                    .collect()
            }
            Formation::Column => {
                let dir = (to - centroid).normalize_or_zero();
                let dir = if dir == Vec2::ZERO {
                    Vec2::new(1.0, 0.0)
                } else {
                    dir
                };
                (0..entries.len())
                    .map(|i| to - dir * (i as f32) * 64.0)
                    .collect()
            }
        };
        let mut used = vec![false; slots.len()];
        let mut goals: Vec<(usize, Vec2)> = Vec::with_capacity(entries.len());
        for (index, pos) in entries {
            let mut best = 0;
            let mut best_distance = f32::MAX;
            for (slot_index, slot) in slots.iter().enumerate() {
                if used[slot_index] {
                    continue;
                }
                let distance = pos.distance_squared(*slot);
                if distance < best_distance {
                    best_distance = distance;
                    best = slot_index;
                }
            }
            used[best] = true;
            goals.push((index, slots[best]));
        }
        // A flow field from the click point gives the whole group a coherent
        // route; per-unit paths end near their formation slot.
        let goal_cell = self.nearest_walkable_cell(self.grid.world_to_cell(to), 6);
        let field = goal_cell.and_then(|cell| self.flow_field(cell));
        for (index, slot) in goals {
            self.units[index].order_queue.clear();
            let final_cell = self.nearest_walkable_cell(self.grid.world_to_cell(slot), 4);
            let mut path: Vec<(u32, u32)> = Vec::new();
            if let (Some(field), Some(final_cell)) = (field.as_ref(), final_cell) {
                let start = self.grid.world_to_cell(self.units[index].pos);
                let start = self.nearest_walkable_cell(start, 2).unwrap_or(start);
                if let Some(mut traced) = nav::trace_flow(&self.grid, field, start, 512) {
                    if traced.last() != Some(&final_cell) {
                        if let Some(last) = traced.last().copied() {
                            if let Some(mut tail) = self.path_between(last, final_cell) {
                                tail.remove(0);
                                traced.extend(tail);
                            }
                        }
                        // Even without the A* tail the flow prefix is a valid
                        // route: the unit advances and re-requests on arrival.
                        traced.truncate(traced.len().min(4096));
                    }
                    path = traced;
                }
            }
            if path.is_empty() {
                self.request_path(index, final_cell.unwrap_or(self.grid.world_to_cell(slot)));
            } else {
                self.units[index].path = path;
                self.units[index].path_index = 0;
            }
            self.units[index].goal = Some(slot);
            self.units[index].harvest = None;
            self.units[index].order = if attack_move {
                Order::MoveAttack(slot)
            } else {
                Order::Move(slot)
            };
            self.units[index].stuck_ticks = 0;
            self.units[index].stuck_attempts = 0;
        }
    }

    fn start_order(&mut self, index: usize, spec: OrderSpec) {
        if !self.units[index].alive || self.units[index].is_building() {
            return;
        }
        self.units[index].stuck_ticks = 0;
        self.units[index].stuck_attempts = 0;
        match spec {
            OrderSpec::Stop => {
                self.units[index].order = Order::Idle;
                self.units[index].path.clear();
                self.units[index].goal = None;
                self.units[index].patrol = None;
                self.units[index].harvest = None;
            }
            OrderSpec::Hold => {
                self.units[index].order = Order::Hold;
                self.units[index].path.clear();
                self.units[index].goal = None;
                self.units[index].patrol = None;
                self.units[index].harvest = None;
            }
            OrderSpec::Move(to) => {
                let cell = self.nearest_walkable_cell(self.grid.world_to_cell(to), 6);
                self.units[index].order = Order::Move(to);
                self.units[index].goal = Some(to);
                self.units[index].patrol = None;
                self.units[index].harvest = None;
                self.units[index].path.clear();
                self.units[index].path_index = 0;
                if let Some(cell) = cell {
                    self.request_path(index, cell);
                }
            }
            OrderSpec::AttackMove(to) => {
                let cell = self.nearest_walkable_cell(self.grid.world_to_cell(to), 6);
                self.units[index].order = Order::MoveAttack(to);
                self.units[index].goal = Some(to);
                self.units[index].patrol = None;
                self.units[index].harvest = None;
                self.units[index].path.clear();
                self.units[index].path_index = 0;
                if let Some(cell) = cell {
                    self.request_path(index, cell);
                }
            }
            OrderSpec::Patrol(a, b) => {
                self.units[index].order = Order::Patrol {
                    a,
                    b,
                    forward: true,
                };
                self.units[index].goal = Some(b);
                self.units[index].patrol = Some((a, b));
                self.units[index].harvest = None;
                if let Some(cell) = self.nearest_walkable_cell(self.grid.world_to_cell(b), 6) {
                    self.request_path(index, cell);
                }
            }
            OrderSpec::Attack(target) => {
                self.units[index].order = Order::Attack(target);
                self.units[index].patrol = None;
                self.units[index].path.clear();
                self.units[index].harvest = None;
                self.units[index].path_index = 0;
                if let Some(target_unit) = self.unit(target) {
                    let cell =
                        self.nearest_walkable_cell(self.grid.world_to_cell(target_unit.pos), 3);
                    if let Some(cell) = cell {
                        self.request_path(index, cell);
                    }
                }
            }
            OrderSpec::Harvest(mine) => {
                self.units[index].order = Order::Harvest {
                    mine,
                    phase: HarvestPhase::ToMine,
                };
                self.units[index].patrol = None;
                self.units[index].harvest = Some(mine);
                self.units[index].harvest_timer = 0;
                self.units[index].goal = Some(mine);
                self.units[index].path.clear();
                self.units[index].path_index = 0;
                if let Some(cell) = self.nearest_walkable_cell(self.grid.world_to_cell(mine), 6) {
                    self.request_path(index, cell);
                }
            }
            OrderSpec::HarvestLumber(tree) => {
                self.units[index].order = Order::HarvestLumber {
                    tree,
                    phase: HarvestPhase::ToMine,
                };
                self.units[index].harvest = Some(tree);
                self.units[index].harvest_timer = 0;
                self.units[index].goal = Some(tree);
                self.units[index].path.clear();
                self.units[index].path_index = 0;
                if let Some(cell) = self.nearest_walkable_cell(self.grid.world_to_cell(tree), 6) {
                    self.request_path(index, cell);
                }
            }
            OrderSpec::Repair(building) => {
                self.units[index].order = Order::Repair(building);
                self.units[index].patrol = None;
                self.units[index].harvest = None;
                if let Some(target) = self.unit(building) {
                    if let Some(cell) =
                        self.nearest_walkable_cell(self.grid.world_to_cell(target.pos), 3)
                    {
                        self.request_path(index, cell);
                    }
                }
            }
            OrderSpec::Build(site) => {
                self.units[index].order = Order::Build(site);
                self.units[index].patrol = None;
                self.units[index].harvest = None;
                if let Some(building) = self.unit(site) {
                    if let Some(cell) =
                        self.nearest_walkable_cell(self.grid.world_to_cell(building.pos), 3)
                    {
                        self.request_path(index, cell);
                    }
                }
            }
        }
    }

    fn finish_order(&mut self, index: usize) {
        let next = self.units[index].order_queue.first().copied();
        if let Some(spec) = next {
            self.units[index].order_queue.remove(0);
            self.units[index].path.clear();
            self.units[index].path_index = 0;
            self.start_order(index, spec);
        } else {
            self.units[index].order = Order::Idle;
            self.units[index].path.clear();
            self.units[index].path_index = 0;
            self.units[index].goal = None;
            self.units[index].patrol = None;
            self.units[index].harvest = None;
        }
    }

    fn resume_or_finish(&mut self, index: usize) {
        if let Some((a, b)) = self.units[index].patrol {
            let forward = self.units[index].goal != Some(a);
            self.units[index].order = Order::Patrol { a, b, forward };
            let goal = if forward { b } else { a };
            self.units[index].goal = Some(goal);
            if let Some(cell) = self.nearest_walkable_cell(self.grid.world_to_cell(goal), 6) {
                self.request_path(index, cell);
            }
        } else if let Some(goal) = self.units[index].goal {
            let cell = self.nearest_walkable_cell(self.grid.world_to_cell(goal), 6);
            self.units[index].order = Order::MoveAttack(goal);
            if let Some(cell) = cell {
                self.request_path(index, cell);
            }
        } else {
            self.finish_order(index);
        }
    }

    // ------------------------------------------------------------------
    // Pathing
    // ------------------------------------------------------------------

    pub fn invalidate_paths(&mut self) {
        self.flow_cache.clear();
        self.path_requests.clear();
        for unit in &mut self.units {
            unit.path.clear();
            unit.path_index = 0;
        }
    }

    fn nearest_walkable_cell(&self, cell: (u32, u32), max_ring: u32) -> Option<(u32, u32)> {
        nav::nearest_walkable(&self.grid, cell, max_ring)
    }

    fn path_between(&mut self, from: (u32, u32), to: (u32, u32)) -> Option<Vec<(u32, u32)>> {
        // Cross-map clicks need a far larger node budget than the old cap: a
        // 900x500 pathing grid exhausted it and the unit got no path at all.
        // The budget stays bounded (2x `MAX_ASTAR_NODES`) so an unreachable
        // target cannot expand the whole map every tick.
        let nodes = nav::MAX_ASTAR_NODES.saturating_mul(2);
        self.pathfinder.astar(&self.grid, from, to, nodes)
    }

    fn request_path(&mut self, index: usize, goal: (u32, u32)) {
        let id = self.units[index].id.0;
        if let Some(existing) = self
            .path_requests
            .iter_mut()
            .find(|entry| entry.0 == id as u32)
        {
            existing.1 = goal.0;
            existing.2 = goal.1;
        } else if self.path_requests.len() < 512 {
            self.path_requests.push((id as u32, goal.0, goal.1));
        }
    }

    fn flow_field(&mut self, goal: (u32, u32)) -> Option<Vec<u32>> {
        if let Some((field, created)) = self.flow_cache.get(&goal) {
            if self.tick.saturating_sub(*created) < 240 {
                return Some(field.clone());
            }
        }
        let field = self.grid.flow_field(goal);
        if field.iter().all(|&d| d == u32::MAX) {
            return None;
        }
        self.flow_cache.insert(goal, (field.clone(), self.tick));
        if self.flow_cache.len() > 16 {
            if let Some(key) = self.flow_cache.keys().next().copied() {
                self.flow_cache.remove(&key);
            }
        }
        Some(field)
    }

    fn process_path_requests(&mut self) {
        let mut processed = 0;
        while processed < self.path_budget && !self.path_requests.is_empty() {
            let (id, gx, gy) = self.path_requests.remove(0);
            let Some(index) = self.unit_index(UnitId(id)) else {
                processed += 1;
                continue;
            };
            if !self.units[index].alive {
                processed += 1;
                continue;
            }
            let start = self.grid.world_to_cell(self.units[index].pos);
            let start = self.nearest_walkable_cell(start, 4).unwrap_or(start);
            // A click on water, a cliff or a tree is not a destination: WC3
            // walks to the closest reachable ground. Search wide enough to
            // cross a lake (the old 4-cell ring left the goal blocked, so A*
            // failed and the unit walked straight into the obstacle).
            let Some(goal) = self.nearest_walkable_cell((gx, gy), 64) else {
                self.gather_path_failed(index);
                processed += 1;
                continue;
            };
            match self.path_between(start, goal) {
                Some(path) => {
                    self.units[index].path = path;
                    self.units[index].path_index = 0;
                }
                None => {
                    // WC3 never refuses a move: when the exact goal is
                    // unreachable (island, cliff pocket), walk as close as
                    // possible instead of standing still.
                    //
                    // The fallback used to try 12 partial paths, each a full
                    // A* search: on a 512x384 map with 2 000 blocked cells one
                    // unreachable goal cost 13 map-wide searches, which stalled
                    // the tick (observed: 0.13 s/tick on `(2)Justice.w3x`).
                    // One flow field (cached for 240 ticks) plus a single
                    // one-cell A* gives the same "walk as close as possible"
                    // result at bounded cost.
                    let Some(field) = self.flow_field(goal) else {
                        self.gather_path_failed(index);
                        processed += 1;
                        continue;
                    };
                    let width = self.grid.width as usize;
                    let here = start.1 as usize * width + start.0 as usize;
                    let mut best: Option<((u32, u32), u32)> = None;
                    for (dx, dy) in [
                        (-1i32, 0i32),
                        (1, 0),
                        (0, -1),
                        (0, 1),
                        (-1, -1),
                        (-1, 1),
                        (1, -1),
                        (1, 1),
                    ] {
                        let nx = start.0 as i32 + dx;
                        let ny = start.1 as i32 + dy;
                        if nx < 0
                            || ny < 0
                            || nx >= self.grid.width as i32
                            || ny >= self.grid.height as i32
                        {
                            continue;
                        }
                        let cell = (nx as u32, ny as u32);
                        if !self.grid.walkable(cell.0, cell.1) {
                            continue;
                        }
                        let distance = field[ny as usize * width + nx as usize];
                        if distance == u32::MAX || distance >= field[here] {
                            continue;
                        }
                        if best.map(|(_, known)| distance < known).unwrap_or(true) {
                            best = Some((cell, distance));
                        }
                    }
                    let mut walked = false;
                    if let Some((cell, _)) = best
                        && let Some(path) = self.path_between(start, cell)
                    {
                        self.units[index].path = path;
                        self.units[index].path_index = 0;
                        walked = true;
                    }
                    if !walked {
                        self.gather_path_failed(index);
                    }
                }
            }
            processed += 1;
        }
    }

    // ------------------------------------------------------------------
    // Timers, orders, movement
    // ------------------------------------------------------------------

    /// Any unit standing on a blocked cell (bad spawn, building placed on top,
    /// WPM/terrain correction) is moved to the nearest walkable cell. Without
    /// this a single blocked start cell makes every A* fail ("stuck in
    /// textures", "cannot move").
    fn unstick_units(&mut self) {
        for index in 0..self.units.len() {
            if !self.units[index].alive {
                continue;
            }
            // A building's own footprint blocks its centre cell on purpose: it
            // must never be "freed" (the bug teleported every hall one cell on
            // the first tick and left its blocked cells behind as a phantom
            // wall that later wedged a worker).
            if self.units[index].is_building() {
                continue;
            }
            // Cell-only checks miss a unit whose body overlaps a neighbouring
            // blocked cell; `can_stand` is the same test movement uses.
            let pos = self.units[index].pos;
            let radius = self.units[index].radius().max(6.0);
            if self.can_stand(pos, radius) {
                continue;
            }
            if let Some(open) = self.nearest_standable(pos, radius, 8) {
                self.units[index].pos = open;
                self.units[index].path.clear();
                self.units[index].path_index = 0;
            }
        }
    }

    /// A gather order whose path request failed keeps its phase machine: if
    /// the worker already stands inside the interaction gate it starts
    /// gathering; otherwise the order is dropped with an explicit error event
    /// (`gather unreachable`) instead of a silent `Idle`.
    fn gather_path_failed(&mut self, index: usize) {
        let (target, gathering) = match self.units[index].order {
            Order::Harvest { mine, phase } => {
                let gate = self.gather_gate(mine, 56.0);
                (
                    Some(mine),
                    (phase == HarvestPhase::ToMine
                        && self.units[index].pos.distance(mine) <= gate)
                        .then_some(Order::Harvest {
                            mine,
                            phase: HarvestPhase::Gathering,
                        }),
                )
            }
            Order::HarvestLumber { tree, phase } => {
                let gate = self.gather_gate(tree, 64.0);
                (
                    Some(tree),
                    (phase == HarvestPhase::ToMine
                        && self.units[index].pos.distance(tree) <= gate)
                        .then_some(Order::HarvestLumber {
                            tree,
                            phase: HarvestPhase::Gathering,
                        }),
                )
            }
            _ => (None, None),
        };
        self.units[index].path.clear();
        self.units[index].path_index = 0;
        if let Some(order) = gathering {
            self.units[index].order = order;
            self.units[index].harvest_timer = HARVEST_GATHER_TICKS;
            self.units[index].goal = target;
            return;
        }
        self.units[index].order = Order::Idle;
        self.units[index].goal = None;
        self.units[index].harvest = None;
        if target.is_some() {
            self.events.push(Event::Error("gather unreachable"));
        }
    }

    fn advance_timers(&mut self) {
        for unit in &mut self.units {
            if !unit.alive {
                continue;
            }
            if unit.cooldown > 0 {
                unit.cooldown -= 1;
            }
            for cooldown in &mut unit.spell_cd {
                *cooldown = cooldown.saturating_sub(1);
            }
            for (_, cooldown) in &mut unit.data_cd {
                *cooldown = cooldown.saturating_sub(1);
            }
            unit.data_cd.retain(|(_, cooldown)| *cooldown > 0);
            if unit.max_mana > 0 && unit.mana < unit.max_mana {
                let regen = unit.mana_regen_milli;
                if regen > 0 {
                    unit.mana_milli += regen;
                    if unit.mana_milli >= 1000 {
                        unit.mana_milli -= 1000;
                        unit.mana += 1;
                    }
                }
            } else {
                unit.mana_milli = 0;
            }
            if unit.hp < unit.max_hp {
                let regen = unit.hp_regen_milli;
                if regen > 0 {
                    unit.hp_milli += regen;
                    while unit.hp_milli >= 1000 {
                        unit.hp_milli -= 1000;
                        unit.hp += 1;
                        if unit.hp >= unit.max_hp {
                            unit.hp = unit.max_hp;
                            unit.hp_milli = 0;
                            break;
                        }
                    }
                }
            } else {
                unit.hp_milli = 0;
            }
        }
        self.advance_pending_casts();
    }

    fn advance_orders(&mut self) {
        for index in 0..self.units.len() {
            if !self.units[index].alive {
                continue;
            }
            match self.units[index].order {
                Order::Attack(target) => {
                    let owner = self.units[index].owner;
                    let valid = self
                        .unit(target)
                        .map(|unit| unit.alive && unit.owner != owner)
                        .unwrap_or(false);
                    if !valid {
                        self.resume_or_finish(index);
                    }
                }
                Order::Build(site) => {
                    let valid = self
                        .unit(site)
                        .map(|unit| unit.alive && unit.construct.is_some())
                        .unwrap_or(false);
                    if !valid {
                        self.finish_order(index);
                    }
                }
                Order::Harvest { .. } => {}
                _ => {}
            }
        }
    }

    /// Movement intent for one unit: target point, chase flag, stop distance.
    fn movement_target(&self, index: usize) -> Option<(Vec2, bool, f32)> {
        let unit = &self.units[index];
        match unit.order {
            Order::Move(target) => Some((target, false, ARRIVE_RADIUS)),
            Order::MoveAttack(target) => Some((target, false, ARRIVE_RADIUS)),
            Order::Patrol { a, b, forward } => {
                Some((if forward { b } else { a }, false, ARRIVE_RADIUS))
            }
            Order::Attack(target) => {
                let target_unit = self.unit(target)?;
                if !target_unit.alive || target_unit.owner == unit.owner {
                    return None;
                }
                let building_bonus = if target_unit.is_building() {
                    CELL_SIZE * 0.5
                } else {
                    0.0
                };
                let stop = (unit.range + target_unit.radius() + building_bonus - 4.0).max(4.0);
                Some((target_unit.pos, true, stop))
            }
            Order::Build(site) => {
                let building = self.unit(site)?;
                if !building.alive {
                    return None;
                }
                let stop = self.build_channel_range(building.def().footprint);
                Some((building.pos, false, stop))
            }
            Order::Repair(target) => {
                let building = self.unit(target)?;
                if !building.alive {
                    return None;
                }
                let stop = self.build_channel_range(building.def().footprint);
                Some((building.pos, false, stop))
            }
            Order::Harvest { mine, phase } => match phase {
                HarvestPhase::ToMine => Some((mine, false, 56.0)),
                HarvestPhase::ToHall => unit.goal.map(|hall| (hall, false, 160.0)),
                HarvestPhase::Gathering => None,
            },
            Order::HarvestLumber { tree, phase } => match phase {
                HarvestPhase::ToMine => Some((tree, false, 64.0)),
                HarvestPhase::ToHall => unit.goal.map(|hall| (hall, false, 160.0)),
                HarvestPhase::Gathering => None,
            },
            Order::Idle | Order::Hold => None,
        }
    }

    fn nearest_hall(&self, owner: u8, pos: Vec2) -> Option<Vec2> {
        self.units
            .iter()
            .filter(|unit| {
                unit.alive
                    && unit.owner == owner
                    && unit.def().stats.supply_provided > 0
                    && unit.construct.is_none()
            })
            .min_by(|a, b| {
                a.pos
                    .distance(pos)
                    .partial_cmp(&b.pos.distance(pos))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|unit| unit.pos)
    }

    /// World distance from a target's centre to the far edge of the blocked
    /// cells that belong to it (a gold mine's WPM footprint, a tree's cell).
    ///
    /// A worker paths to the nearest walkable cell, which on a blocked
    /// footprint sits `reach` away from the centre — larger than one cell for a
    /// mine.  Every gather arrival gate is derived from this instead of a
    /// constant that a blocked target can never satisfy.
    pub fn blocked_reach(&self, target: Vec2) -> f32 {
        let cell = self.grid.world_to_cell(target);
        let half_diagonal = self.grid.cell_size * std::f32::consts::SQRT_2 * 0.5;
        let mut reach = 0.0f32;
        for dy in -4i32..=4 {
            for dx in -4i32..=4 {
                let x = cell.0 as i32 + dx;
                let y = cell.1 as i32 + dy;
                if x < 0 || y < 0 || x >= self.grid.width as i32 || y >= self.grid.height as i32 {
                    continue;
                }
                if self.grid.walkable(x as u32, y as u32) {
                    continue;
                }
                let centre = self.grid.cell_to_world(x as u32, y as u32);
                reach = reach.max(centre.distance(target) + half_diagonal);
            }
        }
        reach
    }

    /// Arrival gate for a gather interaction: the movement stop radius plus
    /// the target's blocked footprint plus a small slack.
    pub fn gather_gate(&self, target: Vec2, stop: f32) -> f32 {
        (stop + GATHER_SLACK).max(self.blocked_reach(target) + GATHER_SLACK)
    }

    /// Nearest live destructable to `pos` within `radius` (lumber retarget).
    fn nearest_destructable(&self, pos: Vec2, radius: f32) -> Option<Vec2> {
        self.destructables
            .iter()
            .filter(|entry| entry.alive)
            .filter_map(|entry| {
                let distance = entry.pos.distance(pos);
                (distance <= radius).then_some((entry.pos, distance))
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(pos, _)| pos)
    }

    fn next_waypoint(&mut self, index: usize) -> Option<Vec2> {
        if self.units[index].path.is_empty() {
            return None;
        }
        let pos = self.units[index].pos;
        let radius = self.units[index].radius().max(6.0);
        let mut path_index = self.units[index].path_index;
        while path_index < self.units[index].path.len() {
            let cell = self.units[index].path[path_index];
            let waypoint = self.grid.cell_to_world(cell.0, cell.1);
            if pos.distance(waypoint) <= 22.0 {
                path_index += 1;
            } else {
                break;
            }
        }
        let len = self.units[index].path.len();
        if path_index >= len {
            self.units[index].path.clear();
            self.units[index].path_index = 0;
            return None;
        }
        let from = self.grid.world_to_cell(pos);
        let mut best = path_index;
        for ahead in 1..=2 {
            let probe = path_index + ahead;
            if probe >= len {
                break;
            }
            let cell = self.units[index].path[probe];
            let waypoint = self.grid.cell_to_world(cell.0, cell.1);
            if self.grid.line_of_sight(from, cell) && self.clear_world_line(pos, waypoint, radius) {
                best = probe;
            } else {
                break;
            }
        }
        self.units[index].path_index = best;
        let cell = self.units[index].path[best];
        Some(self.grid.cell_to_world(cell.0, cell.1))
    }

    fn advance_movement(&mut self) {
        self.process_path_requests();
        self.unstick_units();
        self.spatial.clear();
        for (index, unit) in self.units.iter().enumerate() {
            if unit.alive {
                self.spatial.insert(index as u32, unit.pos);
            }
        }
        let mut buffer = std::mem::take(&mut self.spatial_buffer);
        let dt = 1.0 / TICKS_PER_SECOND as f32;
        for index in 0..self.units.len() {
            if !self.units[index].alive || self.units[index].is_building() {
                continue;
            }
            if self.units[index].rooted_until > self.tick
                || self.units[index].stunned_until > self.tick
                || self.is_rooted(self.units[index].id)
                || self.is_stunned(self.units[index].id)
            {
                continue;
            }
            let Some((target, chase, stop)) = self.movement_target(index) else {
                self.units[index].stuck_ticks = 0;
                continue;
            };
            let pos = self.units[index].pos;
            let radius = self.units[index].radius().max(6.0);
            let speed = self.effective_speed(self.units[index].id);
            let step = speed * dt;
            let distance = pos.distance(target);
            if !chase && distance <= stop.max(radius * 1.5) {
                self.arrive(index);
                continue;
            }
            if chase && distance <= stop {
                continue;
            }
            // Escape if a building footprint landed on top of the unit.
            let cell = self.grid.world_to_cell(pos);
            if !self.grid.walkable(cell.0, cell.1) {
                if let Some(escape) = self.nearest_walkable_cell(cell, 8) {
                    let toward = self.grid.cell_to_world(escape.0, escape.1);
                    let dir = (toward - pos).normalize_or_zero();
                    self.units[index].pos = pos + dir * step;
                    self.units[index].facing = dir.y.atan2(dir.x);
                }
                continue;
            }
            // Both move and chase orders follow the requested path; a chase
            // without a path falls back to the direct line below.
            let waypoint = self.next_waypoint(index);
            let pure_move = matches!(
                self.units[index].order,
                Order::Move(_) | Order::MoveAttack(_)
            );
            if waypoint.is_none()
                && pure_move
                && !chase
                && self.units[index].path.is_empty()
                && distance > stop.max(radius * 1.5)
                && !self.clear_world_line(pos, target, radius)
            {
                // No reachable path and the straight line is blocked: stop
                // instead of running in place against the terrain.
                self.units[index].stuck_ticks += 1;
                if self.units[index].stuck_ticks >= 30 {
                    self.events.push(Event::Error("path blocked"));
                    self.finish_order(index);
                }
                continue;
            }
            let mut steer_target = waypoint.unwrap_or(target);
            let mut to = steer_target - pos;
            if to.length_squared() < 1.0 {
                steer_target = target;
                to = target - pos;
            }
            let mut dir = to.normalize_or_zero();
            if dir == Vec2::ZERO {
                dir = Vec2::new(1.0, 0.0);
            }
            // Separation from nearby allies and buildings.
            let mut push = Vec2::ZERO;
            self.spatial.query(pos, 96.0, 24, &mut buffer);
            for &raw in &buffer {
                let other_index = raw as usize;
                if other_index == index {
                    continue;
                }
                let Some(other) = self.units.get(other_index) else {
                    continue;
                };
                if !other.alive {
                    continue;
                }
                let other_radius = other.radius().max(6.0);
                let gap = pos.distance(other.pos);
                let minimum = radius + other_radius;
                if gap < minimum && gap > 0.01 {
                    let weight = ((minimum - gap) / minimum).clamp(0.0, 1.0);
                    push += (pos - other.pos) / gap * step * weight * 1.4;
                }
            }
            let mut delta = dir * step + push.clamp_length_max(step * 0.8);
            let mut candidate = pos + delta;
            if !self.can_stand(candidate, radius) {
                let x_only = Vec2::new(delta.x, 0.0);
                let y_only = Vec2::new(0.0, delta.y);
                if x_only.length_squared() > 0.0 && self.can_stand(pos + x_only, radius) {
                    candidate = pos + x_only;
                    delta = x_only;
                } else if y_only.length_squared() > 0.0 && self.can_stand(pos + y_only, radius) {
                    candidate = pos + y_only;
                    delta = y_only;
                } else {
                    candidate = pos;
                    delta = Vec2::ZERO;
                }
            }
            if delta.length_squared() == 0.0 && self.units[index].stuck_ticks >= 8 {
                if let Some(escape) = self.nearest_standable(pos, radius, 12) {
                    candidate = escape;
                    delta = escape - pos;
                } else {
                    let base = self.grid.world_to_cell(pos);
                    'escape: for ring in 1..=12u32 {
                        for dy in -(ring as i32)..=ring as i32 {
                            for dx in -(ring as i32)..=ring as i32 {
                                if dx.abs().max(dy.abs()) != ring as i32 {
                                    continue;
                                }
                                let x = base.0 as i32 + dx;
                                let y = base.1 as i32 + dy;
                                if x < 0
                                    || y < 0
                                    || x >= self.grid.width as i32
                                    || y >= self.grid.height as i32
                                    || !self.grid.walkable(x as u32, y as u32)
                                {
                                    continue;
                                }
                                candidate = self.grid.cell_to_world(x as u32, y as u32);
                                delta = candidate - pos;
                                break 'escape;
                            }
                        }
                    }
                }
            }
            let moved = candidate.distance(pos);
            if moved < step * 0.2 {
                self.units[index].stuck_ticks += 1;
            } else {
                self.units[index].stuck_ticks = 0;
            }
            if self.units[index].stuck_ticks == 24 {
                if let Some(target) = self.movement_target(index) {
                    let goal = self.nearest_walkable_cell(self.grid.world_to_cell(target.0), 6);
                    if let Some(goal) = goal {
                        self.request_path(index, goal);
                    }
                }
            }
            if self.units[index].stuck_ticks >= 30 {
                // Attack and gather orders keep trying: re-path toward their
                // live target instead of dropping the order.  A crowded mine
                // mouth (or a moment against a footprint corner) must not cost
                // a worker for the whole match: on `(2)Justice` one of five
                // workers used to end Idle and never pay again.
                let retry_target = match self.units[index].order {
                    Order::Attack(_) => self.movement_target(index).map(|(target, _, _)| target),
                    Order::Harvest { mine, phase } => Some(match phase {
                        HarvestPhase::ToHall => self.units[index].goal.unwrap_or(mine),
                        _ => mine,
                    }),
                    Order::HarvestLumber { tree, phase } => Some(match phase {
                        HarvestPhase::ToHall => self.units[index].goal.unwrap_or(tree),
                        _ => tree,
                    }),
                    _ => None,
                };
                if let Some(target) = retry_target {
                    let attempts = self.units[index].stuck_attempts;
                    self.units[index].stuck_attempts = attempts.saturating_add(1);
                    self.units[index].stuck_ticks = 0;
                    if attempts >= 3 {
                        self.events.push(Event::Error("path blocked"));
                        self.finish_order(index);
                    } else if let Some(goal) =
                        self.nearest_walkable_cell(self.grid.world_to_cell(target), 8)
                    {
                        self.request_path(index, goal);
                    }
                } else {
                    // Other orders keep the single bounded retry.
                    let attempts = self.units[index].stuck_attempts;
                    self.units[index].stuck_attempts = attempts.saturating_add(1);
                    self.units[index].stuck_ticks = 0;
                    if attempts >= 1 {
                        self.events.push(Event::Error("path blocked"));
                        self.finish_order(index);
                    }
                }
            }
            self.units[index].last_pos = pos;
            self.units[index].pos = candidate;
            if delta.length_squared() > 0.0 {
                let desired = delta.y.atan2(delta.x);
                let max_turn = self.units[index].turn_rate.max(0.0) * std::f32::consts::TAU * dt;
                self.units[index].facing =
                    rotate_towards(self.units[index].facing, desired, max_turn);
            }
        }
        self.spatial_buffer = buffer;
    }

    fn clear_world_line(&self, from: Vec2, to: Vec2, radius: f32) -> bool {
        // Exact segment-vs-inflated-cell clip (parry ray/AABB slab port in
        // `wc3_input::geometry`); 24-pixel steps could miss a thin blocked
        // cell that lies between samples.
        wc3_input::geometry::capsule_path_clear(&self.grid, from, to, radius)
    }

    /// Nearest world point where a unit of `radius` can stand (used to free a
    /// unit that a building footprint boxed in).
    fn nearest_standable(&self, pos: Vec2, radius: f32, max_rings: u32) -> Option<Vec2> {
        let base = self.grid.world_to_cell(pos);
        for ring in 1..=max_rings {
            for dy in -(ring as i32)..=ring as i32 {
                for dx in -(ring as i32)..=ring as i32 {
                    if dx.abs().max(dy.abs()) != ring as i32 {
                        continue;
                    }
                    let x = base.0 as i32 + dx;
                    let y = base.1 as i32 + dy;
                    if x < 0 || y < 0 || x >= self.grid.width as i32 || y >= self.grid.height as i32
                    {
                        continue;
                    }
                    if !self.grid.walkable(x as u32, y as u32) {
                        continue;
                    }
                    let candidate = self.grid.cell_to_world(x as u32, y as u32);
                    if self.can_stand(candidate, radius) {
                        return Some(candidate);
                    }
                }
            }
        }
        None
    }

    fn can_stand(&self, pos: Vec2, radius: f32) -> bool {
        // Exact circle-vs-cell coverage (parry Ball/Aabb port in
        // `wc3_input::geometry`), replacing five-point sampling: a blocked
        // corner between the samples blocked the unit but was not detected.
        wc3_input::geometry::ball_standable(&self.grid, pos, radius)
    }

    fn arrive(&mut self, index: usize) {
        match self.units[index].order {
            Order::Move(_) | Order::MoveAttack(_) => self.finish_order(index),
            Order::Patrol { a, b, forward } => {
                let next_forward = !forward;
                self.units[index].order = Order::Patrol {
                    a,
                    b,
                    forward: next_forward,
                };
                let goal = if next_forward { b } else { a };
                self.units[index].goal = Some(goal);
                self.units[index].patrol = Some((a, b));
                if let Some(cell) = self.nearest_walkable_cell(self.grid.world_to_cell(goal), 6) {
                    self.request_path(index, cell);
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Combat
    // ------------------------------------------------------------------

    fn advance_combat(&mut self) {
        // Acquire targets for idle / attack-move / patrol units.
        let mut buffer = std::mem::take(&mut self.spatial_buffer);
        for index in 0..self.units.len() {
            let unit = &self.units[index];
            if !unit.alive
                || unit.is_building()
                || (unit.damage == 0
                    && unit.def().stats.damage == 0
                    && unit.def().stats.damage_dice == 0)
                || unit.stunned_until > self.tick
                || unit.channel_until > self.tick
                || self.is_stunned(unit.id)
            {
                continue;
            }
            let acquire = unit.def().stats.acquire_range.max(unit.range + 40.0);
            let hold = matches!(unit.order, Order::Hold);
            // Camp guards never acquire on the generic 500-unit sight ring:
            // `advance_creeps` owns their aggro (camp radius) and retaliation
            // (`apply_damage`), so a creep can not be pulled out of its camp.
            if unit.owner >= 2 && unit.camp.is_some() {
                continue;
            }
            let can_acquire = match unit.order {
                Order::Idle | Order::MoveAttack(_) | Order::Patrol { .. } | Order::Hold => true,
                Order::Attack(target) => !self.unit(target).map(|t| t.alive).unwrap_or(false),
                _ => false,
            };
            if !can_acquire {
                continue;
            }
            let range = if hold { unit.range + 8.0 } else { acquire };
            self.spatial.query(unit.pos, range, 32, &mut buffer);
            let owner = unit.owner;
            let pos = unit.pos;
            let mut best: Option<(UnitId, f32)> = None;
            for &raw in &buffer {
                let Some(other) = self.units.get(raw as usize) else {
                    continue;
                };
                if !other.alive || other.owner == owner || raw as usize == index {
                    continue;
                }
                // WC3 targetability: a ground-only weapon (footman, grunt)
                // can not acquire an air unit at all, so it never chases one.
                if !self.can_attack(unit, other) {
                    continue;
                }
                let distance = other.pos.distance(pos);
                if distance > range + other.radius() {
                    continue;
                }
                let candidate = (other.id, distance);
                best = match best {
                    None => Some(candidate),
                    Some((best_id, best_distance)) => {
                        if distance < best_distance
                            || (distance == best_distance && other.id < best_id)
                        {
                            Some(candidate)
                        } else {
                            Some((best_id, best_distance))
                        }
                    }
                };
            }
            if let Some((target, _)) = best {
                if hold {
                    // Hold units keep their order and hit from where they stand.
                    self.attack_if_in_range(index, target);
                } else {
                    self.units[index].order = Order::Attack(target);
                }
            }
        }
        self.spatial_buffer = buffer;

        // Deliver attacks for units and armed buildings.
        for index in 0..self.units.len() {
            let unit = &self.units[index];
            if !unit.alive
                || (unit.damage == 0
                    && unit.def().stats.damage == 0
                    && unit.def().stats.damage_dice == 0)
                || unit.cooldown > 0
                || unit.stunned_until > self.tick
                || unit.channel_until > self.tick
                || self.is_stunned(unit.id)
            {
                continue;
            }
            let target = match unit.order {
                Order::Attack(target) => Some(target),
                Order::Hold => None,
                _ => None,
            };
            if let Some(target) = target {
                self.attack_if_in_range(index, target);
            } else if unit.is_building() {
                if let Some(target) = self.nearest_enemy_in_range(index) {
                    self.attack_if_in_range(index, target);
                }
            }
        }
    }

    fn nearest_enemy_in_range(&self, index: usize) -> Option<UnitId> {
        let unit = &self.units[index];
        let mut best: Option<(UnitId, f32)> = None;
        for other in &self.units {
            if !other.alive || other.owner == unit.owner {
                continue;
            }
            if !self.can_attack(unit, other) {
                continue;
            }
            let distance = other.pos.distance(unit.pos);
            if distance <= unit.range + other.radius() {
                let candidate = (other.id, distance);
                best = match best {
                    None => Some(candidate),
                    Some((best_id, best_distance)) => {
                        if distance < best_distance
                            || (distance == best_distance && other.id < best_id)
                        {
                            Some(candidate)
                        } else {
                            Some((best_id, best_distance))
                        }
                    }
                };
            }
        }
        best.map(|(id, _)| id)
    }

    /// Target classes the unit's weapon may hit (`targs1|targs2`).
    ///
    /// Units missing from the generated profile table fall back to the
    /// weapon's own `[0, u16::MAX]` semantics so an unknown def keeps the
    /// legacy behaviour instead of silently losing its attack.
    fn weapon_targs(&self, unit: &Unit) -> u16 {
        combat_gen::profile(unit.def().id)
            .map(|profile| profile.weapon_targs)
            .unwrap_or(0)
    }

    /// The unit's own target class (`UnitData.slk:targType`).
    fn unit_target_class(&self, unit: &Unit) -> u16 {
        if let Some(profile) = combat_gen::profile(unit.def().id)
            && profile.unit_class != 0
        {
            return profile.unit_class;
        }
        if unit.is_building() {
            tg::STRUCTURE
        } else {
            tg::GROUND
        }
    }

    /// WC3 targetability check between two units.
    fn can_attack(&self, attacker: &Unit, target: &Unit) -> bool {
        let targs = self.weapon_targs(attacker);
        if targs == 0 {
            // No weapon data (unarmed or unknown def): keep the pre-table
            // behaviour so this check never removes an existing attack.
            return true;
        }
        combat::can_target(targs, self.unit_target_class(target))
    }

    fn attack_if_in_range(&mut self, index: usize, target: UnitId) {
        let Some(target_index) = self.unit_index(target) else {
            return;
        };
        if !self.units[target_index].alive {
            return;
        }
        // WC3 targetability: `targs1|targs2` must cover the target's class
        // (`ground`, `air`, `structure`, `ward`).  A ground-only weapon does
        // not damage an air unit even when the order names one.
        if !self.can_attack(&self.units[index], &self.units[target_index]) {
            return;
        }
        let attack_ticks = self.effective_attack_ticks(self.units[index].id);
        let building_bonus = if self.units[target_index].is_building() {
            CELL_SIZE * 0.5
        } else {
            0.0
        };
        let range = self.units[index].range + self.units[target_index].radius() + building_bonus;
        if self.units[index].pos.distance(self.units[target_index].pos) > range {
            return;
        }
        let def = self.units[index].def();
        let dice = def.stats.damage_dice;
        let sides = def.stats.damage_sides;
        let attack_type = def.stats.attack_type;
        let stats = def.stats;
        let attacker = self.units[index].id;
        let target_class = self.unit_target_class(&self.units[target_index]);
        let attacker_class = self.unit_target_class(&self.units[index]);
        let splash = combat_gen::profile(def.id).and_then(|profile| profile.splash);
        // `unit.damage` carries the researched damage upgrades (smith line);
        // the dice are rolled on top exactly as `dmgplus1 + dice1 d sides1`.
        let attack_damage = self.units[index].damage;
        let rolled = combat::roll_weapon_damage(attack_damage, dice, sides, |max| self.random(max));
        let rolled = self.effective_damage(attacker, rolled);
        // WC3 rolls dice first and the cross-class miss second, so the RNG
        // stream for a seed stays reproducible.
        let miss_chance = combat::miss_chance_permille(
            combat::is_air_class(attacker_class),
            combat::is_air_class(target_class),
        );
        let miss = miss_chance > 0 && (self.random(1000) as i32) < miss_chance;
        let base = if miss { 0 } else { rolled };
        self.units[index].cooldown = attack_ticks;
        self.units[index].facing = (self.units[target_index].pos - self.units[index].pos)
            .y
            .atan2((self.units[target_index].pos - self.units[index].pos).x);
        if stats.missile_speed > 0.0 {
            let target_pos = self.units[target_index].pos;
            // A missed missile still flies (it reads as a shot going wide); it
            // carries 0 damage so the impact applies neither hit nor splash.
            self.launch_projectile(
                attacker,
                Some(target),
                target_pos,
                base,
                attack_type,
                stats.missile_speed,
                stats.missile_arc,
                stats.missile_homing,
                def.missile_art,
            );
        } else {
            let _ = self.resolve_weapon_hit(attacker, target, base, attack_type, splash);
        }
    }

    /// Resolves one landed weapon hit: primary damage, then the shipped splash
    /// falloff around the impact point.
    ///
    /// Damage is rolled once at attack time and reused for splash, as in WC3
    /// (`CMissileSplash`): the primary target takes 100%, units inside
    /// `Harea1` take `Hfact1`, units inside `Qarea1` take `Qfact1`.  Splash
    /// respects `splashTargs1` (which may exclude allies via `enemy`), keeps
    /// `targCount1` as a cap, and is applied with the weapon's attack type so
    /// the armour table still decides the final number.
    pub(crate) fn resolve_weapon_hit(
        &mut self,
        from: UnitId,
        to: UnitId,
        damage: i32,
        attack: AttackType,
        splash: Option<SplashProfile>,
    ) -> i32 {
        let dealt = if damage > 0 {
            self.apply_damage(from, to, damage, attack)
        } else {
            0
        };
        let Some(splash) = splash else {
            return dealt;
        };
        if damage <= 0 {
            return dealt;
        }
        let Some(center) = self.unit(to).map(|unit| unit.pos) else {
            return dealt;
        };
        let targs = if splash.targs != 0 {
            splash.targs
        } else {
            self.unit(from)
                .and_then(|unit| combat_gen::profile(unit.def().id))
                .map(|profile| profile.weapon_targs)
                .unwrap_or(0)
        };
        if targs == 0 {
            return dealt;
        }
        let attacker_owner = self.unit(from).map(|unit| unit.owner);
        let mut targets: Vec<(UnitId, f32)> = self
            .units
            .iter()
            .filter(|unit| unit.alive && unit.id != to && unit.id != from)
            .filter(|unit| {
                let class = self.unit_target_class(unit);
                combat::can_target(targs, class)
                    && (targs & tg::ENEMY == 0 || Some(unit.owner) != attacker_owner)
            })
            .map(|unit| (unit.id, unit.pos.distance(center)))
            .filter(|(_, distance)| *distance <= splash.quarter_radius)
            .collect();
        // Deterministic order: distance first, entity id as the tie-break.
        targets.sort_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        let mut hit = 0usize;
        for (unit, distance) in targets {
            if splash.max_targets > 0 && hit >= splash.max_targets as usize {
                break;
            }
            let factor = combat::splash_factor(&splash, distance);
            if factor <= 0 {
                continue;
            }
            let amount = (damage * factor / 1000).max(1);
            let _ = self.apply_damage(from, unit, amount, attack);
            hit += 1;
        }
        dealt
    }

    /// Launches a missile entity.  Homing missiles track a live target; the
    /// point variant flies to `target_pos`.  Damage is applied on impact by
    /// `advance_projectiles`.
    #[allow(clippy::too_many_arguments)]
    pub fn launch_projectile(
        &mut self,
        from: UnitId,
        target: Option<UnitId>,
        target_pos: Vec2,
        damage: i32,
        attack: AttackType,
        speed: f32,
        arc: f32,
        homing: bool,
        art: &'static str,
    ) -> Option<ProjectileId> {
        let (start, owner) = {
            let source = self.unit(from)?;
            (source.pos, source.owner)
        };
        let id = ProjectileId(self.next_projectile_id);
        self.next_projectile_id += 1;
        let total = start.distance(target_pos).max(1.0);
        self.projectiles.push(Projectile {
            id,
            owner,
            source: from,
            target,
            pos: start,
            start,
            target_pos,
            speed: speed.max(1.0),
            arc,
            homing,
            damage,
            attack,
            art,
            traveled: 0.0,
            total,
        });
        Some(id)
    }

    fn advance_projectiles(&mut self) {
        if self.projectiles.is_empty() {
            return;
        }
        let dt = 1.0 / TICKS_PER_SECOND as f32;
        let mut live = Vec::with_capacity(self.projectiles.len());
        let mut impacts: Vec<(UnitId, UnitId, i32, AttackType)> = Vec::new();
        for mut projectile in std::mem::take(&mut self.projectiles) {
            if projectile.homing
                && let Some(target) = projectile.target
                && let Some(unit) = self.unit(target)
                && unit.alive
            {
                projectile.target_pos = unit.pos;
            }
            let delta = projectile.target_pos - projectile.pos;
            let distance = delta.length();
            let step = projectile.speed * dt;
            if distance <= step {
                projectile.pos = projectile.target_pos;
                projectile.traveled = projectile.total;
                let target_alive = projectile
                    .target
                    .and_then(|target| self.unit(target))
                    .map(|unit| unit.alive)
                    .unwrap_or(false);
                if let Some(target) = projectile.target {
                    self.events.push(Event::ProjectileHit {
                        from: projectile.source,
                        to: target,
                        amount: projectile.damage,
                    });
                    if projectile.damage > 0 && target_alive {
                        impacts.push((
                            projectile.source,
                            target,
                            projectile.damage,
                            projectile.attack,
                        ));
                    }
                }
            } else {
                projectile.pos += delta / distance * step;
                projectile.traveled += step;
                live.push(projectile);
            }
        }
        self.projectiles = live;
        for (from, to, damage, attack) in impacts {
            // The splash profile is re-read on impact so a mortar shell that
            // lands after the shooter died still splashes.
            let splash = self
                .unit(from)
                .and_then(|unit| combat_gen::profile(unit.def().id))
                .and_then(|profile| profile.splash);
            let _ = self.resolve_weapon_hit(from, to, damage, attack, splash);
        }
    }

    /// Applies typed damage and resolves death.  Returns the damage dealt.
    pub fn apply_damage(&mut self, from: UnitId, to: UnitId, base: i32, attack: AttackType) -> i32 {
        let Some(target_index) = self.unit_index(to) else {
            return 0;
        };
        if !self.units[target_index].alive {
            return 0;
        }
        if self.units[target_index].invulnerable_until > self.tick {
            return 0;
        }
        if self.is_buff_invulnerable(to) {
            return 0;
        }
        let armor_type = self.effective_armor_type(target_index);
        let armor = self.effective_armor(to);
        let ethereal = self.units[target_index].ethereal;
        let scaled = base * self.damage_taken_permille(to) / 1000;
        // Type table always, armor only for non-spells, ethereal last — the
        // engine's multiplier chain (`0x6f19c5f8` armor gate, `0x6f19c64a`
        // table, `0x6f19c677` ethereal; ledger 404/412).
        let dealt = data::typed_damage_vs(attack, armor_type, armor, scaled, ethereal);
        let target = &mut self.units[target_index];
        target.hp -= dealt;
        let died = target.hp <= 0 && target.alive;
        if died {
            target.alive = false;
            target.path.clear();
            target.path_index = 0;
            target.order = Order::Idle;
            target.order_queue.clear();
            target.production.clear();
            target.construct = None;
        }
        let blocked_cells = if died {
            std::mem::take(&mut self.units[target_index].blocked_cells)
        } else {
            Vec::new()
        };
        for (cx, cy) in blocked_cells {
            let occupied = self
                .units
                .iter()
                .any(|unit| unit.alive && unit.blocked_cells.iter().any(|&c| c == (cx, cy)));
            if !occupied {
                self.grid.set_walkable(cx, cy, true);
            }
        }
        if died {
            self.flow_cache.clear();
            self.events.push(Event::UnitDied(to));
            self.clear_buffs(to);
            self.creep_drop(target_index);
            // Exact-tick backpack drop: the corpse releases its items in the
            // same tick as `Event::UnitDied`, not one tick later in
            // `items_tick`.
            self.drop_items_on_death(to);
        }
        self.events.push(Event::Damage {
            from,
            to,
            amount: dealt,
        });
        // Spiked Barricade (`rspi`): a melee attacker takes flat return damage.
        // The return hit is typed as spells, which skips armor and cannot
        // recurse because spells attackers never trigger spikes.
        if !died && attack != AttackType::Spells && from != to {
            let spikes = self.spike_damage(target_index);
            let melee = self
                .unit(from)
                .map(|unit| unit.def().stats.range <= 128.0)
                .unwrap_or(false);
            if spikes > 0 && melee {
                self.apply_damage(to, from, spikes, AttackType::Spells);
            }
        }
        if died && let Some(killer_owner) = self.unit(from).map(|unit| unit.owner) {
            let grant = self
                .gameplay
                .grant_hero_xp
                .get(self.units[target_index].level.max(1) as usize - 1)
                .copied()
                .unwrap_or_else(|| {
                    GRANT_HERO_XP[(self.units[target_index].level.max(1) as usize - 1).min(9)]
                });
            let dying_pos = self.units[target_index].pos;
            let heroes: Vec<UnitId> = self
                .units
                .iter()
                .filter(|unit| {
                    unit.alive
                        && unit.owner == killer_owner
                        && unit.def().hero
                        && unit.pos.distance(dying_pos) <= self.gameplay.hero_exp_range
                })
                .map(|unit| unit.id)
                .collect();
            for hero in heroes {
                let _ = self.add_hero_xp(hero, grant);
            }
            // WC3 bounty: a kill pays the victim's `UnitBalance.slk` gold
            // bounty to the killing player, once (death is a one-way state).
            // Player-vs-player and player-vs-neutral both pay; own-side kills
            // and neutral kills pay nothing.
            self.grant_kill_bounty(target_index, killer_owner);
        } else if !died
            && self.units[target_index].owner >= 2
            && self.units[target_index].camp.is_some()
        {
            // Camp guards retaliate when a player unit hits them inside the
            // leash; the chase is then owned by `advance_creeps`.
            self.retaliate(target_index, from);
        }
        dealt
    }

    /// A camp guard that is attacked by a player unit fights back, inside its
    /// camp leash.  Neutral-on-neutral damage is ignored (creeps never infight).
    fn retaliate(&mut self, target_index: usize, from: UnitId) {
        let attacker_owner = self.unit(from).map(|unit| unit.owner).unwrap_or(2);
        if attacker_owner >= 2 {
            return;
        }
        let Some(camp_id) = self.units[target_index].camp else {
            return;
        };
        let Some(camp) = self.camps.iter().find(|camp| camp.id == camp_id) else {
            return;
        };
        let anchor = camp.anchor;
        let leash = camp.leash;
        let attacker_pos = match self.unit(from) {
            Some(unit) if unit.alive => unit.pos,
            _ => return,
        };
        if attacker_pos.distance(anchor) > leash {
            return;
        }
        let victim_id = self.units[target_index].id;
        if matches!(self.units[target_index].order, Order::Attack(target) if target == from) {
            return;
        }
        self.start_order(target_index, OrderSpec::Attack(from));
        // The guard is awake: its camp is no longer calm, so it will not heal
        // until the intruder leaves (handled by `advance_creeps`).
        let _ = victim_id;
    }

    /// Pays the WC3 gold bounty of a dying unit to the killing player.
    ///
    /// The table is a byte-faithful extraction of `UnitBalance.slk`
    /// (`bountyplus + bountydice d bountysides`, rolled on the sim RNG).  WC3
    /// pays bounty for both neutral creeps and enemy player units; a unit
    /// killed by its own side, or by a neutral, pays nothing.  The call is
    /// guarded by the death transition in `apply_damage`, so a victim can never
    /// be paid for twice.
    fn grant_kill_bounty(&mut self, victim_index: usize, killer_owner: u8) -> i32 {
        if killer_owner > 1 {
            return 0;
        }
        let victim_owner = self.units[victim_index].owner;
        if victim_owner == killer_owner {
            return 0;
        }
        let def_id = self.units[victim_index].def().id;
        let Some((plus, dice, sides)) = data::unit_bounty(def_id) else {
            return 0;
        };
        let mut gold = plus;
        for _ in 0..dice {
            if sides > 0 {
                gold += self.random(sides) as i32 + 1;
            }
        }
        if gold <= 0 {
            return 0;
        }
        let owner = killer_owner as usize;
        self.resources[owner] += gold;
        self.events.push(Event::Bounty {
            owner: killer_owner,
            unit: self.units[victim_index].id,
            gold,
        });
        gold
    }

    pub fn heal_unit(&mut self, target: UnitId, amount: i32) -> i32 {
        let Some(index) = self.unit_index(target) else {
            return 0;
        };
        let unit = &mut self.units[index];
        if !unit.alive || amount <= 0 {
            return 0;
        }
        // Ethereal targets take `EtherealHealBonus` extra healing
        // (`Units/MiscGame.txt:108`, 1.66 in TFT) — the engine's heal path uses
        // its own multiplier, independent of `EtherealDamageBonus`.
        let amount = if unit.ethereal {
            amount * data::ETHEREAL_HEAL_PERMILLE / 1000
        } else {
            amount
        };
        let before = unit.hp;
        unit.hp = (unit.hp + amount).min(unit.max_hp);
        unit.hp - before
    }

    /// Sets the ethereal-form state (engine `unit + 0x158` counter).
    ///
    /// Returns `false` when the unit id is unknown; a dead unit keeps the flag
    /// until it is cleared, exactly like the engine counter.
    pub fn set_ethereal(&mut self, unit: UnitId, ethereal: bool) -> bool {
        match self.unit_index(unit) {
            Some(index) => {
                self.units[index].ethereal = ethereal;
                true
            }
            None => false,
        }
    }

    /// Area damage used by Thunder Clap, Frost Nova secondary hits and Carrion
    /// Swarm.  Deterministic order: distance, then entity id.
    pub(crate) fn damage_area(
        &mut self,
        caster: UnitId,
        center: Vec2,
        radius: f32,
        power: i32,
        spell: Spell,
    ) {
        let mut targets: Vec<(UnitId, f32)> = self
            .units
            .iter()
            .filter(|unit| {
                unit.alive
                    && unit.pos.distance(center) <= radius + unit.radius()
                    && self
                        .unit(caster)
                        .map(|source| source.owner != unit.owner)
                        .unwrap_or(false)
            })
            .map(|unit| (unit.id, unit.pos.distance(center)))
            .collect();
        targets.sort_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        for (target, _) in targets {
            let dealt = self.apply_damage(caster, target, power, AttackType::Magic);
            if dealt > 0 {
                self.events.push(Event::SpellHit {
                    caster,
                    spell,
                    to: target,
                    amount: dealt,
                });
            }
        }
    }

    fn nearest_enemy_near(
        &self,
        owner: u8,
        pos: Vec2,
        radius: f32,
        exclude: &[UnitId],
    ) -> Option<UnitId> {
        let mut best: Option<(UnitId, f32)> = None;
        for unit in &self.units {
            if !unit.alive || unit.owner == owner || exclude.contains(&unit.id) {
                continue;
            }
            let distance = unit.pos.distance(pos);
            if distance > radius + unit.radius() {
                continue;
            }
            best = match best {
                None => Some((unit.id, distance)),
                Some((best_id, best_distance)) => {
                    if distance < best_distance || (distance == best_distance && unit.id < best_id)
                    {
                        Some((unit.id, distance))
                    } else {
                        Some((best_id, best_distance))
                    }
                }
            };
        }
        best.map(|(id, _)| id)
    }

    fn pos_of(&self, id: UnitId) -> Vec2 {
        self.unit(id).map(|unit| unit.pos).unwrap_or(Vec2::ZERO)
    }

    // ------------------------------------------------------------------
    // Production and construction
    // ------------------------------------------------------------------

    /// Registers already-spawned units as a camp (used by map loading).
    ///
    /// Only camp guards join: living, mobile, armed neutrals.  Structures
    /// (shops, taverns, mercenary camps, fountains) and critters are rejected
    /// so a shop can never be dragged into an aggro ring, and a camp whose
    /// whole body is non-guards is not created at all.
    pub fn register_camp(
        &mut self,
        anchor: Vec2,
        units: &[UnitId],
        aggro: f32,
        leash: f32,
    ) -> Option<u32> {
        let guards: Vec<UnitId> = units
            .iter()
            .copied()
            .filter(|unit| {
                self.unit(*unit)
                    .map(|unit| unit.alive && unit.is_camp_guard())
                    .unwrap_or(false)
            })
            .collect();
        if guards.is_empty() {
            return None;
        }
        let id = self.next_camp_id;
        self.next_camp_id += 1;
        for &unit in &guards {
            if let Some(index) = self.unit_index(unit) {
                self.units[index].camp = Some(id);
            }
        }
        self.camps.push(Camp {
            id,
            anchor,
            units: guards,
            aggro,
            leash,
        });
        Some(id)
    }

    /// Groups units of `owner` that are not in a camp yet into proximity
    /// camps.  Map-placed creep camps are 2-4 units within roughly one camp
    /// radius, so a greedy anchor walk over the stable `UnitId` order
    /// reconstructs them without knowing the camp editor data.  Only camp
    /// guards are clustered (`Unit::is_camp_guard`).
    pub fn cluster_owner_camps(&mut self, owner: u8, radius: f32, aggro: f32, leash: f32) -> usize {
        let mut pending: Vec<(UnitId, Vec2)> = self
            .units
            .iter()
            .filter(|unit| {
                unit.alive && unit.owner == owner && unit.camp.is_none() && unit.is_camp_guard()
            })
            .map(|unit| (unit.id, unit.pos))
            .collect();
        pending.sort_by_key(|(unit, _)| unit.0);
        let mut created = 0;
        while let Some((_, anchor)) = pending.first().copied() {
            let mut grouped = Vec::new();
            let mut rest = Vec::new();
            for (unit, pos) in pending {
                if pos.distance(anchor) <= radius {
                    grouped.push(unit);
                } else {
                    rest.push((unit, pos));
                }
            }
            if self.register_camp(anchor, &grouped, aggro, leash).is_some() {
                created += 1;
            }
            pending = rest;
        }
        created
    }

    /// Neutral (owner 2) variant used for map creep camps: 600-unit proximity,
    /// aggro 500 and leash 900 as in the WC3 creep profile.
    pub fn cluster_neutral_camps(&mut self) -> usize {
        self.cluster_owner_camps(
            2,
            CAMP_CLUSTER_RADIUS,
            CAMP_AGGRO_RADIUS,
            CAMP_LEASH_RADIUS,
        )
    }

    /// Spawns neutral creep units (owner 2) around a camp anchor.  `aggro` and
    /// `leash` default to 500/900 as in a WC3 creep camp.
    pub fn spawn_camp(
        &mut self,
        def_ids: &[&str],
        anchor: Vec2,
        aggro: f32,
        leash: f32,
    ) -> Option<u32> {
        if def_ids.is_empty() {
            return None;
        }
        let id = self.next_camp_id;
        self.next_camp_id += 1;
        let mut units = Vec::new();
        for (index, def_id) in def_ids.iter().enumerate() {
            let angle = index as f32 * std::f32::consts::TAU / def_ids.len() as f32;
            let pos = anchor + Vec2::new(angle.cos(), angle.sin()) * 72.0;
            if let Some(unit) = self.spawn_def(2, def_id, pos) {
                if let Some(unit_index) = self.unit_index(unit) {
                    self.units[unit_index].camp = Some(id);
                }
                units.push(unit);
            }
        }
        if units.is_empty() {
            return None;
        }
        self.camps.push(Camp {
            id,
            anchor,
            units,
            aggro,
            leash,
        });
        Some(id)
    }

    /// Spawns map-placed units (`war3mapUnits.doo`) through the canonical sim.
    ///
    /// Authored positions are authoritative: the map (not our pathing) decides
    /// where a unit stands, so no walkable-cell snap — a snapped hall or creep
    /// silently moves the whole map layout.
    pub fn spawn_map_units(&mut self, entries: &[(u8, &str, Vec2)]) -> Vec<UnitId> {
        entries
            .iter()
            .filter_map(|(owner, def_id, pos)| self.spawn_def_exact(*owner, def_id, *pos))
            .collect()
    }

    /// Sends workers to chop a tree.  The tree position is matched against
    /// spawned destructables within one cell.
    pub fn harvest_lumber(&mut self, units: &[UnitId], tree: Vec2) {
        for &id in units {
            if let Some(index) = self.unit_index(id) {
                if !self.units[index].alive
                    || self.units[index].is_building()
                    || !self.units[index].def().stats.is_worker
                {
                    continue;
                }
                self.units[index].order_queue.clear();
                self.start_order(index, OrderSpec::HarvestLumber(tree));
            }
        }
    }

    /// Spawns a tree/destructable from the generated table.  A tree blocks its
    /// pathing cell until it is destroyed.
    pub fn spawn_destructable(&mut self, def_id: &str, pos: Vec2) -> Option<DestructableId> {
        let def_index_value = data::DESTRUCTABLES
            .iter()
            .position(|def| def.id == def_id)? as u16;
        let def = data::destructable_def(def_id)?;
        // A destructable outside the grid can never be reached: workers would
        // path to it forever and a harvest order would never complete.
        let cell = self.grid.world_to_cell(pos);
        if pos.x < 0.0 || pos.y < 0.0 || cell.0 >= self.grid.width || cell.1 >= self.grid.height {
            return None;
        }
        let id = DestructableId(self.next_destructable_id);
        self.next_destructable_id += 1;
        let walkable = self.grid.walkable(cell.0, cell.1);
        if walkable {
            self.grid.set_walkable(cell.0, cell.1, false);
            self.flow_cache.clear();
        }
        self.destructables.push(Destructable {
            id,
            def_index: def_index_value,
            pos,
            hp: def.hp,
            max_hp: def.hp,
            alive: true,
            blocked_cell: walkable.then_some(cell),
        });
        Some(id)
    }

    /// Attack damage against a tree/destructable.  Returns actual damage.
    pub fn damage_destructable(&mut self, id: DestructableId, amount: i32) -> i32 {
        let Some(index) = self.destructables.iter().position(|entry| entry.id == id) else {
            return 0;
        };
        if !self.destructables[index].alive {
            return 0;
        }
        let dealt = amount.max(1).min(self.destructables[index].hp);
        self.destructables[index].hp -= dealt;
        let destroyed = self.destructables[index].hp <= 0;
        if destroyed {
            self.destructables[index].alive = false;
            if let Some((cx, cy)) = self.destructables[index].blocked_cell.take() {
                self.grid.set_walkable(cx, cy, true);
                self.flow_cache.clear();
            }
            self.events.push(Event::DestructableDestroyed { id });
        }
        dealt
    }

    fn destructable_at(&self, pos: Vec2) -> Option<usize> {
        self.destructables
            .iter()
            .position(|entry| entry.alive && entry.pos.distance(pos) <= 96.0)
    }

    /// Deposits lumber after the upkeep tax (paid amount is returned and
    /// reported in `LumberDeposited`).
    pub fn deposit_lumber(&mut self, owner: u8, amount: i32) -> i32 {
        if amount <= 0 {
            return 0;
        }
        let paid = (amount * self.upkeep(owner).income_permille() / 1000).max(1);
        self.lumber[owner as usize] += paid;
        self.events.push(Event::LumberDeposited {
            owner,
            amount: paid,
        });
        paid
    }

    /// Drops a roster item into the world.  `charges == 0` uses the SLK value.
    /// WC3 creep drop: neutral creeps (owner 2) drop one item from the level
    /// table. Pool and level mapping come from `wc3_data::drops`; retail data
    /// carries no chance table, so the chance is the sim ruleset constant.
    fn creep_drop(&mut self, index: usize) {
        if self.units.get(index).map(|unit| unit.owner) != Some(2) {
            return;
        }
        let creep_level = self.units[index].level.max(1);
        let Some(item_level) = wc3_data::drops::drop_level_for_creep_level(creep_level) else {
            return;
        };
        const CREEP_DROP_CHANCE_PERCENT: u32 = 25;
        if self.random(100) >= CREEP_DROP_CHANCE_PERCENT {
            return;
        }
        let pool: Vec<&'static data::ItemDef> = data::ITEMS
            .iter()
            .filter(|def| def.droppable && def.level == item_level as i32)
            .collect();
        let Some(&def) = pool.get(self.random(pool.len() as u32) as usize) else {
            return;
        };
        let pos = self.units[index].pos;
        self.spawn_item(def.id, pos, 0);
    }

    pub fn spawn_item(&mut self, def_id: &str, pos: Vec2, charges: i32) -> Option<ItemId> {
        let def_index_value = data::ITEMS.iter().position(|item| item.id == def_id)? as u16;
        let def = data::item_def(def_id)?;
        let id = ItemId(self.next_item_id);
        self.next_item_id += 1;
        self.items.push(Item {
            id,
            def_index: def_index_value,
            pos,
            owner: None,
            charges: if charges > 0 { charges } else { def.charges },
        });
        Some(id)
    }

    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.items.iter().find(|item| item.id == id)
    }

    fn apply_item_bonuses(&mut self, unit_index: usize, def: &'static data::ItemDef) {
        self.units[unit_index].damage += def.bonus_attack;
        self.units[unit_index].armor += def.bonus_armor;
        self.units[unit_index].max_hp += def.bonus_hp;
        self.units[unit_index].hp += def.bonus_hp;
        self.units[unit_index].max_mana += def.bonus_mana;
        self.units[unit_index].mana += def.bonus_mana;
    }

    fn remove_item_bonuses(&mut self, unit_index: usize, def: &'static data::ItemDef) {
        self.units[unit_index].damage -= def.bonus_attack;
        self.units[unit_index].armor -= def.bonus_armor;
        self.units[unit_index].max_hp -= def.bonus_hp;
        self.units[unit_index].hp -= def.bonus_hp;
        self.units[unit_index].max_mana -= def.bonus_mana;
        self.units[unit_index].mana -= def.bonus_mana;
    }

    /// Picks up a ground item into a unit's 6-slot inventory and applies its
    /// passive bonuses (attack/armor/HP/mana).
    pub fn pickup_item(&mut self, unit: UnitId, item: ItemId) -> Result<(), &'static str> {
        let Some(unit_index) = self.unit_index(unit) else {
            return Err("no unit");
        };
        let Some(item_index) = self.items.iter().position(|entry| entry.id == item) else {
            return Err("no item");
        };
        if self.items[item_index].owner.is_some() {
            return Err("item already held");
        }
        if self.units[unit_index].items.len() >= 6 {
            return Err("inventory full");
        }
        if self.units[unit_index]
            .pos
            .distance(self.items[item_index].pos)
            > 180.0
        {
            return Err("item too far");
        }
        let def = &data::ITEMS[self.items[item_index].def_index as usize];
        self.apply_item_bonuses(unit_index, def);
        self.units[unit_index].items.push(item);
        self.items[item_index].owner = Some(self.units[unit_index].owner);
        self.items[item_index].pos = self.units[unit_index].pos;
        Ok(())
    }

    /// Drops a carried item back to the ground, removing its passive bonuses.
    pub fn drop_item(&mut self, unit: UnitId, item: ItemId) -> Result<(), &'static str> {
        let Some(unit_index) = self.unit_index(unit) else {
            return Err("no unit");
        };
        let Some(slot) = self.units[unit_index]
            .items
            .iter()
            .position(|id| *id == item)
        else {
            return Err("item not carried");
        };
        let Some(item_index) = self.items.iter().position(|entry| entry.id == item) else {
            return Err("no item");
        };
        let def = &data::ITEMS[self.items[item_index].def_index as usize];
        self.remove_item_bonuses(unit_index, def);
        self.units[unit_index].items.remove(slot);
        let pos = self.units[unit_index].pos;
        self.items[item_index].owner = None;
        self.items[item_index].pos = pos + Vec2::new(32.0, 0.0);
        Ok(())
    }

    /// Uses a consumable item on the carrier (heal/mana) or through its
    /// data-backed ability when no direct effect is parsed.
    pub fn use_item(&mut self, unit: UnitId, item: ItemId) -> Result<(), &'static str> {
        self.use_item_on(unit, item, None, None)
    }

    /// Uses an item with an explicit ability target/point.  A successful use
    /// consumes one charge; the last charge removes the item (and bonuses).
    pub fn use_item_on(
        &mut self,
        unit: UnitId,
        item: ItemId,
        target: Option<UnitId>,
        point: Option<Vec2>,
    ) -> Result<(), &'static str> {
        let Some(unit_index) = self.unit_index(unit) else {
            return Err("no unit");
        };
        if !self.units[unit_index].items.contains(&item) {
            return Err("item not carried");
        }
        let Some(item_index) = self.items.iter().position(|entry| entry.id == item) else {
            return Err("no item");
        };
        let def = &data::ITEMS[self.items[item_index].def_index as usize];
        if def.heal > 0 || def.mana_restore > 0 {
            let healed = self.heal_unit(unit, def.heal);
            let _ = healed;
            if def.mana_restore > 0 {
                let max_mana = self.units[unit_index].max_mana;
                self.units[unit_index].mana =
                    (self.units[unit_index].mana + def.mana_restore).min(max_mana);
            }
        } else if !def.ability_id.is_empty()
            && abilities::generic_ability(def.ability_id)
                .map(|ability| ability.castable)
                .unwrap_or(false)
        {
            self.cast_item_ability(unit, def.ability_id, target, point)
                .map_err(|_| "item ability has no active effect")?;
        } else {
            return Err("item has no implemented effect");
        }
        self.items[item_index].charges -= 1;
        if self.items[item_index].charges <= 0 {
            let _ = self.drop_item(unit, item);
            self.items.retain(|entry| entry.id != item);
        }
        Ok(())
    }

    /// True when the owner has a completed building for every `Requires=`
    /// token of the item (`DependencyOr=` alternatives count as one token).
    fn item_requirements_met(&self, owner: u8, item_id: &str) -> bool {
        data::item_requirements(item_id).all(|requirement| {
            let alternatives = if requirement.alternatives.is_empty() {
                std::slice::from_ref(&requirement.token)
            } else {
                requirement.alternatives
            };
            alternatives.iter().any(|id| {
                self.units.iter().any(|unit| {
                    unit.alive
                        && unit.owner == owner
                        && unit.is_building()
                        && unit.construct.is_none()
                        && unit.def().id == *id
                })
            })
        })
    }

    /// Combines carried items into `result_item` when the retail data defines
    /// a recipe (`RECIPES`; empty on retail installs, where recipes are
    /// map-script logic).  Consumes one of each listed ingredient.
    pub fn combine_items(
        &mut self,
        unit: UnitId,
        result_item: &str,
    ) -> Result<ItemId, &'static str> {
        let index = self.unit_index(unit).ok_or("no unit")?;
        let recipes: Vec<&'static data::RecipeDef> = data::recipes_for(result_item).collect();
        if recipes.is_empty() {
            return Err("no recipe data");
        }
        let def = data::item_def(result_item).ok_or("no item def")?;
        if self.units[index].items.len() >= 6 {
            return Err("inventory full");
        }
        let mut ingredients: Vec<ItemId> = Vec::new();
        for recipe in &recipes {
            let item_index = self.units[index]
                .items
                .iter()
                .copied()
                .find(|item| {
                    self.items
                        .iter()
                        .find(|entry| entry.id == *item)
                        .map(|entry| entry.def().id == recipe.ingredient)
                        .unwrap_or(false)
                })
                .ok_or("missing ingredient")?;
            ingredients.push(item_index);
        }
        for item in &ingredients {
            self.drop_item(unit, *item)
                .map_err(|_| "missing ingredient")?;
            self.items.retain(|entry| entry.id != *item);
        }
        let def_index = data::ITEMS
            .iter()
            .position(|item| item.id == def.id)
            .ok_or("no item def")? as u16;
        let id = ItemId(self.next_item_id);
        self.next_item_id += 1;
        let (pos, owner) = (self.units[index].pos, self.units[index].owner);
        self.items.push(Item {
            id,
            def_index,
            pos,
            owner: Some(owner),
            charges: def.charges,
        });
        self.units[index].items.push(id);
        self.apply_item_bonuses(index, def);
        Ok(id)
    }

    /// Builds shop runtime state for a spawned building.  Sold items are the
    /// real `Sellitems=` list; stock and restock come from `ItemData.slk`.
    fn register_shop(&mut self, shop: UnitId, sells: &'static [&'static str]) {
        let Some(owner) = self.unit(shop).map(|unit| unit.owner) else {
            return;
        };
        let slots: Vec<ShopSlot> = sells
            .iter()
            .filter_map(|id| {
                let def = data::item_def(id)?;
                Some(ShopSlot {
                    item: def.id,
                    stock: def.stock_max.max(1),
                    next_restock_tick: 0,
                })
            })
            .collect();
        self.shops.push(Shop { shop, owner, slots });
    }

    /// Shop stock state for a shop building.
    pub fn shop(&self, shop: UnitId) -> Option<&Shop> {
        self.shops.iter().find(|entry| entry.shop == shop)
    }

    /// Units a shop building sells (`Sellunits=` profile list).
    pub fn shop_units(&self, shop: UnitId) -> &'static [&'static str] {
        self.unit(shop)
            .map(|unit| data::sold_units(unit.def().id))
            .unwrap_or(&[])
    }

    /// Buys a unit from a shop: gold is debited and the unit spawns at the
    /// shop. The sold list is the real `Sellunits=` data (`data::SHOP_UNITS`).
    pub fn buy_unit(&mut self, shop: UnitId, unit_id: &str) -> Result<UnitId, &'static str> {
        let owner = self.unit(shop).map(|unit| unit.owner).ok_or("no shop")?;
        let pos = self.unit(shop).map(|unit| unit.pos).unwrap_or_default();
        let sold = self.shop_units(shop);
        if !sold.iter().any(|candidate| *candidate == unit_id) {
            return Err("unit not sold here");
        }
        let cost = data::def_of(unit_id)
            .map(|def| def.stats.cost)
            .ok_or("no unit def")?;
        if self.resources[owner as usize] < cost {
            return Err("not enough gold");
        }
        self.resources[owner as usize] -= cost;
        self.spawn_def(owner, unit_id, pos).ok_or("spawn failed")
    }

    /// Buys one item from a shop into the buyer's 6-slot inventory.  Gold is
    /// debited, one stock unit consumed and the restock timer armed from the
    /// item's real `stockRegen` value.
    pub fn buy_item(
        &mut self,
        buyer: UnitId,
        shop: UnitId,
        item_id: &str,
    ) -> Result<ItemId, &'static str> {
        let buyer_index = self.unit_index(buyer).ok_or("no unit")?;
        if !self.units[buyer_index].alive {
            return Err("buyer is dead");
        }
        let shop_index = self.unit_index(shop).ok_or("no shop")?;
        if !self.units[shop_index].alive {
            return Err("shop is dead");
        }
        let shop_def = self.units[shop_index].def();
        if shop_def.sells_items.is_empty() {
            return Err("not a shop");
        }
        if !shop_def.sells_items.contains(&item_id) {
            return Err("item not sold here");
        }
        let def = data::item_def(item_id).ok_or("no item def")?;
        if self.units[buyer_index]
            .pos
            .distance(self.units[shop_index].pos)
            > SHOP_RANGE + self.units[shop_index].radius()
        {
            return Err("shop too far");
        }
        let owner = self.units[buyer_index].owner as usize;
        if owner >= self.resources.len() {
            return Err("no owner");
        }
        if self.resources[owner] < def.cost {
            return Err("not enough gold");
        }
        if self.units[buyer_index].items.len() >= 6 {
            return Err("inventory full");
        }
        if !self.item_requirements_met(self.units[buyer_index].owner, def.id) {
            return Err("missing shop requirement");
        }
        let stock_index = self
            .shops
            .iter()
            .position(|entry| entry.shop == shop)
            .ok_or("shop has no stock")?;
        let slot_index = self.shops[stock_index]
            .slots
            .iter()
            .position(|slot| slot.item == def.id)
            .ok_or("item not stocked")?;
        if self.shops[stock_index].slots[slot_index].stock <= 0 {
            return Err("out of stock");
        }
        self.resources[owner] -= def.cost;
        {
            let slot = &mut self.shops[stock_index].slots[slot_index];
            slot.stock -= 1;
            if slot.next_restock_tick == 0 {
                slot.next_restock_tick =
                    self.tick + (def.stock_regen.max(1) as u64) * TICKS_PER_SECOND as u64;
            }
        }
        let def_index = data::ITEMS
            .iter()
            .position(|item| item.id == def.id)
            .ok_or("no item def")? as u16;
        let id = ItemId(self.next_item_id);
        self.next_item_id += 1;
        let (pos, unit_owner) = (self.units[buyer_index].pos, self.units[buyer_index].owner);
        self.items.push(Item {
            id,
            def_index,
            pos,
            owner: Some(unit_owner),
            charges: def.charges,
        });
        self.units[buyer_index].items.push(id);
        self.apply_item_bonuses(buyer_index, def);
        self.events.push(Event::ItemBought {
            buyer,
            shop,
            item: def.id,
        });
        Ok(id)
    }

    fn advance_shops(&mut self) {
        if self.shops.is_empty() {
            return;
        }
        let tick = self.tick;
        for shop in &mut self.shops {
            for slot in &mut shop.slots {
                let Some(def) = data::item_def(slot.item) else {
                    continue;
                };
                let max = def.stock_max.max(1);
                if slot.stock >= max {
                    slot.next_restock_tick = 0;
                    continue;
                }
                let regen = (def.stock_regen.max(1) as u64) * TICKS_PER_SECOND as u64;
                if slot.next_restock_tick == 0 {
                    slot.next_restock_tick = tick + regen;
                }
                while tick >= slot.next_restock_tick && slot.stock < max {
                    slot.stock += 1;
                    slot.next_restock_tick += regen;
                }
                if slot.stock >= max {
                    slot.next_restock_tick = 0;
                }
            }
        }
    }

    /// Applies typed gameplay constants from the operator's install and clamps
    /// every existing unit's speed to the configured limits.
    pub fn apply_gameplay(&mut self, config: GameplayConfig) {
        self.gameplay = config;
        let unit_min = self.gameplay.min_unit_speed;
        let unit_max = self.gameplay.max_unit_speed;
        let bldg_min = self.gameplay.min_bldg_speed;
        let bldg_max = self.gameplay.max_bldg_speed;
        for unit in &mut self.units {
            let (min, max) = if unit.def().stats.is_building {
                (bldg_min, bldg_max)
            } else {
                (unit_min, unit_max)
            };
            unit.speed = clamp_speed(unit.speed, min, max);
        }
    }

    pub fn gameplay_config(&self) -> &GameplayConfig {
        &self.gameplay
    }

    /// XP needed for the next level from the loaded gameplay constants.
    pub fn need_hero_xp(&self, level: u8) -> u32 {
        if level == 0 || level >= self.gameplay.max_hero_level {
            return 0;
        }
        self.gameplay
            .need_hero_xp
            .get((level - 1) as usize)
            .copied()
            .unwrap_or(0)
    }

    /// Cumulative XP to reach `level` from level 1 (gameplay constants).
    pub fn hero_xp_for_level(&self, level: u8) -> u32 {
        (1..level.max(1)).map(|step| self.need_hero_xp(step)).sum()
    }

    /// Current hero level (`None` for a non-hero unit).
    pub fn hero_level(&self, unit: UnitId) -> Option<u8> {
        let unit = self.unit(unit)?;
        unit.def().hero.then_some(unit.level)
    }

    /// Cumulative hero XP (`None` for a non-hero unit).
    pub fn hero_xp(&self, unit: UnitId) -> Option<u32> {
        let unit = self.unit(unit)?;
        unit.def().hero.then_some(unit.xp)
    }

    /// Current strength/agility/intelligence (`None` for a non-hero unit).
    pub fn hero_attributes(&self, unit: UnitId) -> Option<(i32, i32, i32)> {
        let unit = self.unit(unit)?;
        unit.def().hero.then_some((
            unit.hero_strength,
            unit.hero_agility,
            unit.hero_intelligence,
        ))
    }

    /// Adds XP and applies every level-up it earns.  Attribute growth follows
    /// the real constants: +25 HP per Strength, +15 mana per Intelligence,
    /// +0.3 armor per Agility and +1 damage per primary-attribute point.
    pub fn add_hero_xp(&mut self, unit: UnitId, amount: u32) -> Result<u8, &'static str> {
        let index = self.unit_index(unit).ok_or("no unit")?;
        let def = self.units[index].def();
        if !def.hero {
            return Err("not a hero");
        }
        if !self.units[index].alive {
            return Err("hero is dead");
        }
        self.units[index].xp = self.units[index].xp.saturating_add(amount);
        let mut level = self.units[index].level;
        while level < self.gameplay.max_hero_level
            && self.units[index].xp >= self.hero_xp_for_level(level.saturating_add(1))
        {
            level += 1;
            self.apply_hero_level(index, level);
        }
        Ok(self.units[index].level)
    }

    /// Sets a hero's level (level-up only), applying the attribute deltas.
    pub fn set_hero_level(&mut self, unit: UnitId, level: u8) -> Result<(), &'static str> {
        let index = self.unit_index(unit).ok_or("no unit")?;
        if !self.units[index].def().hero {
            return Err("not a hero");
        }
        if !(1..=self.gameplay.max_hero_level).contains(&level) {
            return Err("level out of range");
        }
        if level < self.units[index].level {
            return Err("cannot lower hero level");
        }
        self.units[index].xp = self.units[index].xp.max(self.hero_xp_for_level(level));
        self.apply_hero_level(index, level);
        Ok(())
    }

    fn apply_hero_level(&mut self, index: usize, level: u8) {
        let stats = self.units[index].def().stats;
        if level <= self.units[index].level {
            self.units[index].level = self.units[index].level.max(level);
            return;
        }
        let steps = level as f32 - 1.0;
        let hp_bonus = self.gameplay.str_hit_point_bonus;
        let mana_bonus = self.gameplay.int_mana_bonus;
        let armor_milli = (self.gameplay.agi_defense_bonus * 1000.0).round() as i32;
        let damage_bonus = self.gameplay.str_attack_bonus;
        let attr = |base: i32, gain: f32| base + (gain * steps).round() as i32;
        let strength = attr(stats.hero_strength, stats.hero_strength_gain);
        let agility = attr(stats.hero_agility, stats.hero_agility_gain);
        let intelligence = attr(stats.hero_intelligence, stats.hero_intelligence_gain);
        let d_str = strength - self.units[index].hero_strength;
        let d_agi = agility - self.units[index].hero_agility;
        let d_int = intelligence - self.units[index].hero_intelligence;
        let primary_delta = match stats.hero_primary {
            HeroAttribute::Strength => d_str,
            HeroAttribute::Agility => d_agi,
            HeroAttribute::Intelligence => d_int,
            HeroAttribute::None => 0,
        };
        self.units[index].damage += (primary_delta as f32 * damage_bonus).round() as i32;
        let hp_gain = hp_bonus * d_str;
        self.units[index].max_hp += hp_gain;
        self.units[index].hp += hp_gain;
        let mana_gain = mana_bonus * d_int;
        self.units[index].max_mana += mana_gain;
        self.units[index].mana += mana_gain;
        self.units[index].agi_armor_milli += armor_milli * d_agi;
        let whole = self.units[index].agi_armor_milli / 1000;
        self.units[index].armor += whole;
        self.units[index].agi_armor_milli -= whole * 1000;
        self.units[index].hero_strength = strength;
        self.units[index].hero_agility = agility;
        self.units[index].hero_intelligence = intelligence;
        self.units[index].skill_points = self.units[index]
            .skill_points
            .saturating_add(level.saturating_sub(self.units[index].level));
        self.units[index].level = level;
    }

    /// WC3 melee opening for a 1v1 at explicit base positions.
    pub fn setup_melee_1v1(
        &mut self,
        player_race: Race,
        enemy_race: Race,
        player_base: Vec2,
        enemy_base: Vec2,
    ) -> Result<MeleeSetup, &'static str> {
        let seats = [
            (player_race, SeatController::Human),
            (
                enemy_race,
                SeatController::Ai(AiConfig {
                    workers: 3,
                    army_supply: 6,
                    first_attack_tick: 900,
                    wave_interval: 1200,
                    retreat_permille: 300,
                    composition: ai_composition_for(enemy_race),
                    worker: ai::worker_for_race(enemy_race),
                }),
            ),
        ];
        self.setup_melee_layout(&seats, &[player_base, enemy_base])
    }

    /// General melee setup: even seat count 2..=8 (1v1..4v4), any race per
    /// seat, bases spaced evenly around a fair ring on the current map.
    /// Seats with `Human` join owner 0; seats with `Ai` join owner 1.
    pub fn setup_melee(
        &mut self,
        seats: &[(Race, SeatController)],
        map_seed: u64,
    ) -> Result<MeleeSetup, &'static str> {
        if seats.is_empty() || seats.len() % 2 != 0 || seats.len() > 8 {
            return Err("melee needs an even seat count in 2..=8");
        }
        let world_w = self.grid.width as f32 * self.grid.cell_size;
        let world_h = self.grid.height as f32 * self.grid.cell_size;
        let center = Vec2::new(world_w * 0.5, world_h * 0.5);
        let radius = world_w.min(world_h) * 0.32;
        let offset = (map_seed % 628) as f32 / 100.0;
        let anchors: Vec<Vec2> = (0..seats.len())
            .map(|index| {
                let angle = offset + index as f32 * std::f32::consts::TAU / seats.len() as f32;
                center + Vec2::new(angle.cos() * radius, angle.sin() * radius * 0.9)
            })
            .collect();
        self.setup_melee_layout(seats, &anchors)
    }

    fn setup_melee_layout(
        &mut self,
        seats: &[(Race, SeatController)],
        anchors: &[Vec2],
    ) -> Result<MeleeSetup, &'static str> {
        if seats.is_empty() || seats.len() % 2 != 0 || seats.len() > 8 {
            return Err("melee needs an even seat count in 2..=8");
        }
        if anchors.len() != seats.len() {
            return Err("anchor count must equal seat count");
        }
        let team_size = seats.len() / 2;
        let world_w = self.grid.width as f32 * self.grid.cell_size;
        let world_h = self.grid.height as f32 * self.grid.cell_size;
        let center = Vec2::new(world_w * 0.5, world_h * 0.5);
        let radius = world_w.min(world_h) * 0.32;
        let mut halls: Vec<UnitId> = Vec::new();
        let mut start_mines = 0usize;
        let mut workers = 0usize;
        let mut creeps = 0usize;
        let mut workers_by_owner = [0usize; 2];
        let mut mines_by_owner = [Vec2::ZERO; 2];
        let mut camps = 0usize;
        let mut ai_configs: [Option<AiConfig>; 2] = [None, None];

        for (index, (race, controller)) in seats.iter().enumerate() {
            let owner = match controller {
                SeatController::Human => 0u8,
                SeatController::Ai(config) => {
                    if ai_configs[1].is_none() {
                        ai_configs[1] = Some(*config);
                    }
                    1u8
                }
            };
            let hall_def = data::main_hall_for(*race).ok_or("race has no main hall")?;
            let hall_pos = self
                .free_site(anchors[index], hall_def.footprint)
                .ok_or("no free hall site")?;
            let hall = self
                .spawn_def_exact(owner, hall_def.id, hall_pos)
                .ok_or("hall spawn failed")?;
            halls.push(hall);
            let outward = (hall_pos - center).normalize_or_zero();
            let mine_pos = self
                .free_site(hall_pos + outward * 360.0, 1)
                .ok_or("no free mine site")?;
            self.add_gold_mine(mine_pos, GOLD_MINE_GOLD);
            start_mines += 1;
            if index == 0 {
                mines_by_owner[0] = mine_pos;
            }
            if index == team_size {
                mines_by_owner[1] = mine_pos;
            }
            let worker_id = ai::worker_for_race(*race);
            for slot in 0..5 {
                // The ring follows the hall's authored footprint instead of a
                // fixed 320: a 6-cell town hall is `hall_clearance = 6*128/2 +
                // 64 = 448` from centre, and a constant put the fifth slot
                // inside the model whenever the footprint grew. The ring also
                // drifts and widens when water/cliffs block a slot, so a base
                // against the map edge still fields all five workers.
                let angle = slot as f32 * std::f32::consts::TAU / 5.0;
                let requested =
                    hall_pos + Vec2::new(angle.cos(), angle.sin()) * self.hall_clearance(hall);
                let Some(pos) = self
                    .melee_worker_spot(hall, hall_pos, slot)
                    .or_else(|| self.free_site(requested, 1))
                else {
                    continue;
                };
                self.log_spawn_adjust(worker_id, requested, pos);
                if self.spawn_def_exact(owner, worker_id, pos).is_some() {
                    workers += 1;
                    workers_by_owner[owner as usize] += 1;
                }
            }
            // Five neutral creeps guard each starting gold mine as one camp.
            // Far enough out that the five guards never aggro the workers at
            // the mine (guard ring 190, mine at 360, camp at 760).
            let camp_anchor = self
                .free_site(mine_pos + outward * 1200.0, 1)
                .unwrap_or(mine_pos);
            if self
                .spawn_camp(
                    &["nftr", "nban", "nftr", "nban", "nftr"],
                    camp_anchor,
                    150.0,
                    400.0,
                )
                .is_some()
            {
                camps += 1;
                creeps += 5;
            }
        }

        // Two expansion mines per side (team), each guarded by a neutral camp.
        let mut expansion_mines = 0usize;
        for team in 0..2 {
            for slot in 0..2 {
                let angle = std::f32::consts::PI * (0.5 + slot as f32)
                    + team as f32 * std::f32::consts::FRAC_PI_2;
                let desired = center + Vec2::new(angle.cos(), angle.sin()) * radius * 0.55;
                let Some(pos) = self.free_site(desired, 1) else {
                    continue;
                };
                if self
                    .gold_mines
                    .iter()
                    .any(|mine| mine.pos.distance(pos) < 300.0)
                {
                    continue;
                }
                self.add_gold_mine(pos, GOLD_MINE_GOLD);
                expansion_mines += 1;
                // Guard camp sits on the map-center side, away from the
                // base/expansion hall sites on the outward side.
                let inward = (center - pos).normalize_or_zero();
                let camp_pos = self.free_site(pos + inward * 700.0, 1).unwrap_or(pos);
                if self
                    .spawn_camp(
                        &["nftr", "nban", "nftr", "nban", "nftr"],
                        camp_pos,
                        250.0,
                        600.0,
                    )
                    .is_some()
                {
                    camps += 1;
                }
            }
        }

        for (owner, config) in ai_configs.iter().enumerate() {
            if let Some(config) = config {
                self.enable_ai(owner as u8, *config);
            }
        }
        let player_hall = *halls.first().ok_or("no player hall")?;
        let enemy_hall = *halls.get(team_size).ok_or("no enemy hall")?;
        Ok(MeleeSetup {
            player_hall,
            enemy_hall,
            player_mine: mines_by_owner[0],
            enemy_mine: mines_by_owner[1],
            player_workers: workers_by_owner[0],
            enemy_workers: workers_by_owner[1],
            seats: seats.len(),
            team_size,
            halls: halls.len(),
            start_mines,
            expansion_mines,
            workers,
            creeps,
            camps,
            player_creeps: 5,
            enemy_creeps: 5,
        })
    }

    fn footprint_free(&self, pos: Vec2, footprint: u8) -> bool {
        let (cx, cy) = self.grid.world_to_cell(pos);
        let half = footprint.max(1) as i32 / 2;
        for dy in -half..=half {
            for dx in -half..=half {
                let x = cx as i32 + dx;
                let y = cy as i32 + dy;
                if x < 0 || y < 0 || x >= self.grid.width as i32 || y >= self.grid.height as i32 {
                    return false;
                }
                if !self.grid.walkable(x as u32, y as u32) {
                    return false;
                }
            }
        }
        true
    }

    fn free_site(&self, desired: Vec2, footprint: u8) -> Option<Vec2> {
        let base = self.walkable_pos_near(desired, 48)?;
        if self.footprint_free(base, footprint) {
            return Some(base);
        }
        let step = self.grid.cell_size;
        for ring in 1..=12 {
            for slot in 0..12 {
                let angle = slot as f32 * std::f32::consts::TAU / 12.0;
                let candidate = base + Vec2::new(angle.cos(), angle.sin()) * step * ring as f32;
                if let Some(pos) = self.walkable_pos_near(candidate, 4)
                    && self.footprint_free(pos, footprint)
                {
                    return Some(pos);
                }
            }
        }
        None
    }

    /// Spends one skill point on an ability from the hero's real ability list.
    /// Ultimate abilities require `reqLevel` 6 (AbilityData.slk).
    pub fn learn_ability(
        &mut self,
        unit: UnitId,
        ability_id: &'static str,
    ) -> Result<(), &'static str> {
        let index = self.unit_index(unit).ok_or("no unit")?;
        if !self.units[index].alive {
            return Err("hero is dead");
        }
        let def = self.units[index].def();
        if !def.hero {
            return Err("not a hero");
        }
        if self.units[index].skill_points == 0 {
            return Err("no skill points");
        }
        if !def.ability_ids.contains(&ability_id) {
            return Err("ability not on hero");
        }
        let ability = abilities::generic_ability(ability_id).ok_or("ability not in data")?;
        if ability.req_level > self.units[index].level {
            return Err("hero level too low");
        }
        if self.units[index].learned.contains(&ability_id) {
            return Err("ability already learned");
        }
        self.units[index].learned.push(ability_id);
        self.units[index].skill_points -= 1;
        Ok(())
    }

    /// Schedules an altar revive: charges `HeroMaxReviveCostGold` (700) and
    /// brings the hero back after `HeroMaxReviveTime` (150 s of ticks).
    pub fn revive_hero(&mut self, hero: UnitId, altar: UnitId) -> Result<i32, &'static str> {
        let index = self.unit_index(hero).ok_or("no hero")?;
        if self.units[index].alive {
            return Err("hero is alive");
        }
        if !self.units[index].def().hero {
            return Err("not a hero");
        }
        if self.units[index].revive_at != 0 {
            return Err("revive already scheduled");
        }
        let altar_index = self.unit_index(altar).ok_or("no altar")?;
        if !self.units[altar_index].alive || self.units[altar_index].construct.is_some() {
            return Err("altar not ready");
        }
        let race = self.units[index].def().race;
        let expected = data::altar_for(race).ok_or("race has no altar")?;
        if self.units[altar_index].def().id != expected.id {
            return Err("wrong altar");
        }
        let owner = self.units[index].owner as usize;
        if owner >= self.resources.len() {
            return Err("no owner");
        }
        if self.resources[owner] < HERO_MAX_REVIVE_COST_GOLD {
            return Err("not enough gold");
        }
        self.resources[owner] -= HERO_MAX_REVIVE_COST_GOLD;
        self.units[index].revive_at =
            self.tick + (HERO_MAX_REVIVE_TIME_SECONDS * TICKS_PER_SECOND as f32).round() as u64;
        self.units[index].revive_altar = Some(altar);
        Ok(HERO_MAX_REVIVE_COST_GOLD)
    }

    fn advance_revival(&mut self) {
        let tick = self.tick;
        let mut revived: Vec<(UnitId, UnitId)> = Vec::new();
        for index in 0..self.units.len() {
            if self.units[index].alive
                || self.units[index].revive_at == 0
                || tick < self.units[index].revive_at
            {
                continue;
            }
            let altar = self.units[index].revive_altar;
            let altar_ok = altar
                .and_then(|id| self.unit(id))
                .map(|unit| unit.alive && unit.construct.is_none())
                .unwrap_or(false);
            self.units[index].revive_at = 0;
            self.units[index].revive_altar = None;
            if !altar_ok {
                continue;
            }
            let altar = altar.unwrap();
            let altar_pos = self
                .unit(altar)
                .map(|unit| unit.pos)
                .unwrap_or(self.units[index].pos);
            let pos = self
                .walkable_pos_near(altar_pos + Vec2::new(0.0, 150.0), 8)
                .unwrap_or(altar_pos);
            let unit = &mut self.units[index];
            unit.alive = true;
            unit.hp = unit.max_hp;
            unit.mana = unit.max_mana;
            unit.pos = pos;
            unit.order = Order::Idle;
            unit.path.clear();
            unit.path_index = 0;
            unit.cooldown = 0;
            unit.data_cd.clear();
            unit.spell_cd = [0; abilities::SPELL_COUNT];
            revived.push((unit.id, altar));
        }
        for (hero, altar) in revived {
            self.events.push(Event::HeroRevived { hero, altar });
        }
    }

    /// Camp AI: guards aggro on approach, chase up to the leash, return home
    /// and heal, and never leave the camp.  WC3 semantics:
    ///
    /// * a living player unit inside `camp.aggro` of the anchor wakes the
    ///   camp (creeps pick the nearest intruder);
    /// * a guard that is further than `camp.leash` from the anchor, or whose
    ///   target is, drops the chase and walks home (`MaxGuardDistance`);
    /// * a calm camp (nobody inside aggro) restores its guards to full HP and
    ///   mana once they are back within `CAMP_HOME_RADIUS` — WC3 camps heal
    ///   their guards on return;
    /// * a guard whose target dies returns home instead of idling in the field.
    fn advance_creeps(&mut self) {
        if self.camps.is_empty() {
            return;
        }
        let mut actions: Vec<(UnitId, OrderSpec)> = Vec::new();
        for camp_index in 0..self.camps.len() {
            let (anchor, aggro, leash) = {
                let camp = &self.camps[camp_index];
                (camp.anchor, camp.aggro, camp.leash)
            };
            let members = self.camps[camp_index].units.clone();
            // Calm camps heal; an intruder inside aggro keeps them in combat.
            let intruder = self.nearest_intruder(anchor, aggro);
            for unit in members {
                let Some(index) = self.unit_index(unit) else {
                    continue;
                };
                if !self.units[index].alive {
                    continue;
                }
                let pos = self.units[index].pos;
                let order = self.units[index].order;
                let home = pos.distance(anchor) <= CAMP_HOME_RADIUS;
                match order {
                    Order::Attack(target) => {
                        let target_ok = self
                            .unit(target)
                            .map(|unit| unit.alive && unit.pos.distance(anchor) <= leash)
                            .unwrap_or(false);
                        if !target_ok || pos.distance(anchor) > leash {
                            actions.push((unit, OrderSpec::Move(anchor)));
                        }
                    }
                    Order::Move(goal) => {
                        if home {
                            actions.push((unit, OrderSpec::Stop));
                        } else if pos.distance(anchor) > leash
                            && goal.distance(anchor) > CAMP_HOME_RADIUS
                        {
                            // Any walk that is not the trip home keeps the guard
                            // inside its leash.
                            actions.push((unit, OrderSpec::Move(anchor)));
                        }
                    }
                    Order::Idle | Order::Hold => {
                        let mut best: Option<(UnitId, f32)> = None;
                        for other in &self.units {
                            if !other.alive || other.owner >= 2 {
                                continue;
                            }
                            let distance = other.pos.distance(anchor);
                            if distance <= aggro {
                                let candidate = (other.id, distance);
                                best = match best {
                                    None => Some(candidate),
                                    Some((best_id, best_distance)) => {
                                        if distance < best_distance
                                            || (distance == best_distance && other.id < best_id)
                                        {
                                            Some(candidate)
                                        } else {
                                            Some((best_id, best_distance))
                                        }
                                    }
                                };
                            }
                        }
                        if let Some((target, _)) = best {
                            actions.push((unit, OrderSpec::Attack(target)));
                        } else if pos.distance(anchor) > leash {
                            actions.push((unit, OrderSpec::Move(anchor)));
                        }
                    }
                    _ => {}
                }
                // Return-home heal: calm camp, guard at home, not fighting.
                if intruder.is_none()
                    && home
                    && !matches!(self.units[index].order, Order::Attack(_))
                    && (self.units[index].hp < self.units[index].max_hp
                        || self.units[index].mana < self.units[index].max_mana)
                {
                    let unit = &mut self.units[index];
                    unit.hp = unit.max_hp;
                    unit.mana = unit.max_mana;
                    unit.mana_milli = 0;
                }
            }
        }
        for (unit, order) in actions {
            self.issue(&[unit], order);
        }
    }

    /// Nearest living unit of a player (owner < 2) inside `radius` of `pos`.
    fn nearest_intruder(&self, pos: Vec2, radius: f32) -> Option<UnitId> {
        let mut best: Option<(UnitId, f32)> = None;
        for unit in &self.units {
            if !unit.alive || unit.owner >= 2 {
                continue;
            }
            let distance = unit.pos.distance(pos);
            if distance > radius {
                continue;
            }
            let replace = match best {
                None => true,
                Some((best_id, best_distance)) => {
                    distance < best_distance || (distance == best_distance && unit.id < best_id)
                }
            };
            if replace {
                best = Some((unit.id, distance));
            }
        }
        best.map(|(id, _)| id)
    }

    /// Critters (mobile neutrals with no attack) run from player units that
    /// come within `CRITTER_FLEE_RADIUS`.  They never fight and never join a
    /// camp, so their only reaction is flight.
    fn advance_critters(&mut self) {
        let mut actions: Vec<(UnitId, OrderSpec)> = Vec::new();
        for index in 0..self.units.len() {
            let unit = &self.units[index];
            if !unit.alive || !unit.is_critter() {
                continue;
            }
            let pos = unit.pos;
            let mut threat: Option<(f32, Vec2)> = None;
            for other in &self.units {
                if !other.alive || other.owner >= 2 || other.is_building() {
                    continue;
                }
                let distance = other.pos.distance(pos);
                if distance > CRITTER_FLEE_RADIUS {
                    continue;
                }
                let replace = match threat {
                    None => true,
                    Some((best_distance, _)) => distance < best_distance,
                };
                if replace {
                    threat = Some((distance, other.pos));
                }
            }
            let Some((_, threat)) = threat else {
                continue;
            };
            // Keep an existing escape that still runs away from the threat.
            if let Order::Move(goal) = unit.order
                && goal.distance(threat) > pos.distance(threat)
            {
                continue;
            }
            let away = pos - threat;
            let dir = if away.length_squared() > 1.0 {
                away.normalize()
            } else {
                Vec2::new(1.0, 0.0)
            };
            let run_to = pos + dir * (CRITTER_FLEE_RADIUS + 120.0);
            if let Some(goal) = self.walkable_pos_near(run_to, 6) {
                actions.push((unit.id, OrderSpec::Move(goal)));
            }
        }
        for (unit, order) in actions {
            self.issue(&[unit], order);
        }
    }

    fn requirements_met(&self, owner: u8, def: &'static UnitDef) -> bool {
        def.requires.iter().all(|required| {
            self.units.iter().any(|unit| {
                unit.alive
                    && unit.owner == owner
                    && unit.is_building()
                    && unit.construct.is_none()
                    && unit.def().id == *required
            })
        })
    }

    /// True when `owner` has a finished building of `def_id`.  Used by the
    /// building-upgrade check, which must treat the building being upgraded as
    /// its own predecessor requirement.
    fn owner_has_building(&self, owner: u8, def_id: &str) -> bool {
        self.units.iter().any(|unit| {
            unit.alive
                && unit.owner == owner
                && unit.is_building()
                && unit.construct.is_none()
                && unit.def().id == def_id
        })
    }

    fn upgrade_bonuses(
        &self,
        owner: u8,
        def: &'static UnitDef,
    ) -> data::UpgradeBonus {
        let mut bonus = data::UpgradeBonus::default();
        for (id, level) in &self.upgrades[owner.min(1) as usize] {
            let Some(upgrade) = data::upgrade_def(id) else {
                continue;
            };
            // WC3 applies an upgrade only to the units whose `upgrades`
            // column lists it (UnitBalance.slk), not to the whole race.
            if !data::affected_by(def, id) {
                continue;
            }
            for effect in upgrade.effects {
                let amount = effect.base + effect.step * level.saturating_sub(1) as i32;
                match effect.kind {
                    data::UpgradeEffectKind::AttackDamage => bonus.damage += amount,
                    data::UpgradeEffectKind::AttackDamagePercent => {
                        bonus.damage_permille += amount
                    }
                    data::UpgradeEffectKind::Armor => {
                        bonus.armor += if effect.base == 0 && effect.step == 0 {
                            data::unit_armor_upgrade(def.id) * *level as i32
                        } else {
                            amount
                        }
                    }
                    data::UpgradeEffectKind::HitPoints => bonus.hp += amount,
                    data::UpgradeEffectKind::HitPointsPercent => bonus.hp_permille += amount,
                    data::UpgradeEffectKind::MoveSpeed => bonus.speed += amount as f32,
                    data::UpgradeEffectKind::AttackRange => bonus.range += amount as f32,
                    data::UpgradeEffectKind::Mana => bonus.mana += amount,
                    data::UpgradeEffectKind::ManaRegen => bonus.mana_regen_milli += amount,
                    data::UpgradeEffectKind::HitPointRegen => bonus.hp_regen_milli += amount,
                    data::UpgradeEffectKind::LumberHarvest => bonus.lumber += amount,
                    data::UpgradeEffectKind::Sight => bonus.sight += amount as f32,
                    data::UpgradeEffectKind::AttackSpeed => {
                        bonus.attack_speed_permille += amount
                    }
                    data::UpgradeEffectKind::ArmorType => {
                        bonus.armor_type = Some(data::ArmorType::from_index(effect.base))
                    }
                    data::UpgradeEffectKind::Spikes
                    | data::UpgradeEffectKind::UnlockAbility
                    | data::UpgradeEffectKind::DataOnly => {}
                }
            }
        }
        bonus
    }

    pub fn upgrade_level(&self, owner: u8, id: &str) -> u8 {
        self.upgrades[owner.min(1) as usize]
            .get(id)
            .copied()
            .unwrap_or(0)
    }

    /// Queues a race-correct upgrade at a building that lists it in its
    /// `Researches=` profile.  Gold is deducted exactly once; completion
    /// applies the stat delta to every existing unit and to future spawns.
    pub fn research_upgrade(
        &mut self,
        building: UnitId,
        upgrade_id: &'static str,
    ) -> Result<(), &'static str> {
        let Some(index) = self.unit_index(building) else {
            return Err("no building");
        };
        if !self.units[index].alive || !self.units[index].is_building() {
            return Err("not a building");
        }
        if self.units[index].construct.is_some() {
            return Err("under construction");
        }
        let owner = self.units[index].owner;
        let Some(upgrade) = data::upgrade_def(upgrade_id) else {
            return Err("unknown upgrade");
        };
        let building_def = self.units[index].def();
        let can_research = building_def.researches.contains(&upgrade_id)
            || (building_def.main_hall
                && (upgrade.race == building_def.race || upgrade.race == Race::Neutral));
        if !can_research {
            return Err("cannot research here");
        }
        let level = self.upgrade_level(owner, upgrade_id);
        if level >= upgrade.max_level {
            return Err("already researched");
        }
        if self.units.iter().any(|unit| {
            unit.alive && unit.owner == owner && unit.research.iter().any(|r| r.id == upgrade_id)
        }) {
            return Err("already researching");
        }
        let cost = upgrade.gold_cost(level + 1);
        if self.resources[owner as usize] < cost {
            self.events.push(Event::Error("not enough gold"));
            return Err("not enough gold");
        }
        self.resources[owner as usize] -= cost;
        self.units[index].research.push(Research {
            id: upgrade_id,
            remaining: upgrade.research_ticks(level + 1).max(1),
        });
        Ok(())
    }

    // ------------------------------------------------------------------
    // Building upgrades (`[htow] Upgrade=hkee`, `[hwtw] Upgrade=hgtw,…`)
    // ------------------------------------------------------------------

    /// Why `building` cannot upgrade into `target` right now, or `None` when it
    /// can.  One predicate for the card affordance, the command stream and the
    /// tests: `upgrade_building` is exactly this check plus the debit.
    pub fn upgrade_denial(&self, building: UnitId, target: &str) -> Option<&'static str> {
        let Some(index) = self.unit_index(building) else {
            return Some("no building");
        };
        let unit = &self.units[index];
        if !unit.alive {
            return Some("building dead");
        }
        if !unit.is_building() {
            return Some("not a building");
        }
        if unit.construct.is_some() {
            return Some("under construction");
        }
        if unit.upgrade.is_some() {
            return Some("already upgrading");
        }
        let Some(target_def) = def_of(target) else {
            return Some("unknown building");
        };
        if !target_def.is_building() {
            return Some("not a building");
        }
        // The chain itself is data: `[htow] Upgrade=hkee` in `*UnitFunc.txt`.
        if !data::can_upgrade_into(unit.def().id, target) {
            return Some("not an upgrade of this building");
        }
        // Every other `Requires=` row of the target still applies, except the
        // predecessor: the building being upgraded *is* that requirement.
        for required in target_def.requires {
            if *required == unit.def().id {
                continue;
            }
            if !self.owner_has_building(unit.owner, required) {
                return Some("missing requirement");
            }
        }
        let owner = unit.owner as usize;
        if self.resources[owner] < target_def.stats.cost {
            return Some("not enough gold");
        }
        if self.lumber[owner] < target_def.stats.lumber_cost {
            return Some("not enough lumber");
        }
        None
    }

    /// Starts the in-place upgrade of `building` into `target`.
    ///
    /// Charge, time and stats come from the *target* unit row (WC3 upgrades pay
    /// the new building's cost and build time); the unit keeps its identity,
    /// position, rally point and production queue, and the hit-point ratio is
    /// preserved on completion.
    pub fn upgrade_building(
        &mut self,
        building: UnitId,
        target: &'static str,
    ) -> Result<(), &'static str> {
        if let Some(reason) = self.upgrade_denial(building, target) {
            if reason == "not enough gold" {
                self.events.push(Event::Error("not enough gold"));
            } else if reason == "not enough lumber" {
                self.events.push(Event::Error("not enough lumber"));
            } else {
                self.events.push(Event::Error(reason));
            }
            return Err(reason);
        }
        let Some(index) = self.unit_index(building) else {
            return Err("no building");
        };
        let Some(target_index) = def_index(target) else {
            return Err("unknown building");
        };
        let target_def = def_at(target_index).ok_or("unknown building")?;
        let owner = self.units[index].owner as usize;
        self.resources[owner] -= target_def.stats.cost;
        self.lumber[owner] -= target_def.stats.lumber_cost;
        let ticks = target_def.stats.build_ticks.max(1);
        self.units[index].upgrade = Some(UpgradeJob {
            to_def_index: target_index,
            remaining: ticks,
            total: ticks,
        });
        // An upgraded building keeps its queue but does not work while the
        // upgrade runs (WC3 greys the production queue out).
        Ok(())
    }

    /// Ticks every in-flight building upgrade; completion swaps the def in
    /// place.  Called from [`Sim::step`] before production so a hall that
    /// finishes this tick can start a tier-2 unit on the same tick.
    fn advance_upgrades(&mut self) {
        let mut completed: Vec<usize> = Vec::new();
        for index in 0..self.units.len() {
            if !self.units[index].alive {
                continue;
            }
            let Some(mut job) = self.units[index].upgrade else {
                continue;
            };
            job.remaining = job.remaining.saturating_sub(1);
            self.units[index].upgrade = Some(job);
            if job.remaining == 0 {
                completed.push(index);
            }
        }
        for index in completed {
            self.finish_upgrade(index);
        }
    }

    /// Swaps a finished upgrade's def in place and refreshes every cached stat
    /// from the new row (the same bonuses `spawn_def_index_at` applies).
    fn finish_upgrade(&mut self, index: usize) {
        let Some(job) = self.units[index].upgrade else {
            return;
        };
        let Some(def) = def_at(job.to_def_index) else {
            self.units[index].upgrade = None;
            return;
        };
        let owner = self.units[index].owner;
        let pos = self.units[index].pos;
        let bonus = self.upgrade_bonuses(owner.min(1), def);
        let stats = def.stats;
        let id = self.units[index].id;
        // Preserve the damaged fraction: a Keep built from a half-dead Town
        // Hall starts at half its own maximum.
        let ratio = if self.units[index].max_hp > 0 {
            self.units[index].hp as f32 / self.units[index].max_hp as f32
        } else {
            1.0
        }
        .clamp(0.0, 1.0);
        let new_max = stats.hp + bonus.hp + stats.hp * bonus.hp_permille / 1000;
        let new_hp = ((new_max as f32 * ratio).round() as i32).clamp(1, new_max);
        // Footprint can change (a Scout Tower and a Guard Tower share 3 cells,
        // but an upgraded hall line does not always): release the old cells and
        // claim the new ones so pathing matches the new def.
        let old_cells = std::mem::take(&mut self.units[index].blocked_cells);
        for &(cx, cy) in &old_cells {
            self.grid.set_walkable(cx, cy, true);
        }
        let mut blocked_cells = Vec::new();
        if stats.is_building {
            blocked_cells = self.footprint_cells(pos, self.footprint_side_cells(def.footprint));
            for &(cx, cy) in &blocked_cells {
                self.grid.set_walkable(cx, cy, false);
            }
            self.flow_cache.clear();
        }
        let unit = &mut self.units[index];
        unit.def_index = job.to_def_index;
        unit.kind = def.kind;
        unit.max_hp = new_max;
        unit.hp = new_hp;
        unit.max_mana = stats.max_mana + bonus.mana;
        unit.mana = unit.mana.min(unit.max_mana);
        unit.armor = stats.armor + bonus.armor;
        unit.damage = stats.damage + bonus.damage + stats.damage * bonus.damage_permille / 1000;
        unit.range = stats.range + bonus.range;
        unit.speed = clamp_speed(
            stats.speed + bonus.speed,
            self.gameplay.min_bldg_speed,
            self.gameplay.max_bldg_speed,
        );
        unit.sight = stats.sight + bonus.sight;
        unit.attack_ticks = stats.attack_ticks;
        unit.level = if def.hero { 1 } else { stats.level.max(1) };
        unit.turn_rate = stats.turn_rate;
        unit.mana_regen_milli = stats.mana_regen_milli + bonus.mana_regen_milli;
        unit.blocked_cells = blocked_cells;
        unit.upgrade = None;
        let sells = def.sells_items;
        if !sells.is_empty() && !self.shops.iter().any(|shop| shop.shop == id) {
            self.register_shop(id, sells);
        }
        self.events.push(Event::UpgradeComplete {
            building: id,
            to: def.id,
        });
        self.events.push(Event::BuildingComplete(id));
    }

    fn advance_research(&mut self) {
        let mut completed: Vec<(u8, &'static str)> = Vec::new();
        for index in 0..self.units.len() {
            if !self.units[index].alive || self.units[index].research.is_empty() {
                continue;
            }
            let owner = self.units[index].owner;
            let finished = if let Some(research) = self.units[index].research.first_mut() {
                research.remaining = research.remaining.saturating_sub(1);
                (research.remaining == 0).then_some(research.id)
            } else {
                None
            };
            if let Some(id) = finished {
                self.units[index].research.remove(0);
                completed.push((owner, id));
            }
        }
        for (owner, id) in completed {
            let level = self.upgrade_level(owner, id) + 1;
            self.upgrades[owner.min(1) as usize].insert(id, level);
            self.apply_upgrade_delta(owner, id, level);
            self.events.push(Event::UpgradeResearched { owner, id });
        }
    }

    fn apply_upgrade_delta(&mut self, owner: u8, id: &'static str, level: u8) {
        let Some(upgrade) = data::upgrade_def(id) else {
            return;
        };
        for index in 0..self.units.len() {
            if !self.units[index].alive || self.units[index].owner != owner {
                continue;
            }
            let def = self.units[index].def();
            if !data::affected_by(def, id) {
                continue;
            }
            let base_damage = def.stats.damage;
            let base_hp = def.stats.hp;
            let unit = &mut self.units[index];
            for effect in upgrade.effects {
                let delta = if level == 1 { effect.base } else { effect.step };
                match effect.kind {
                    data::UpgradeEffectKind::AttackDamage => unit.damage += delta,
                    data::UpgradeEffectKind::AttackDamagePercent => {
                        unit.damage += base_damage * delta / 1000
                    }
                    data::UpgradeEffectKind::Armor => {
                        unit.armor += if effect.base == 0 && effect.step == 0 {
                            data::unit_armor_upgrade(def.id)
                        } else {
                            delta
                        }
                    }
                    data::UpgradeEffectKind::HitPoints => {
                        unit.max_hp += delta;
                        unit.hp += delta;
                    }
                    data::UpgradeEffectKind::HitPointsPercent => {
                        let added = base_hp * delta / 1000;
                        unit.max_hp += added;
                        unit.hp += added;
                    }
                    data::UpgradeEffectKind::MoveSpeed => unit.speed += delta as f32,
                    data::UpgradeEffectKind::AttackRange => unit.range += delta as f32,
                    data::UpgradeEffectKind::Mana => {
                        unit.max_mana += delta;
                        unit.mana += delta;
                    }
                    data::UpgradeEffectKind::ManaRegen => unit.mana_regen_milli += delta,
                    data::UpgradeEffectKind::HitPointRegen => unit.hp_regen_milli += delta,
                    data::UpgradeEffectKind::Sight => unit.sight += delta as f32,
                    // Read through helpers from the upgrade table (lumber trip
                    // capacity, attack interval, armor class) or applied by the
                    // research completion itself (ability grant).
                    data::UpgradeEffectKind::LumberHarvest
                    | data::UpgradeEffectKind::AttackSpeed
                    | data::UpgradeEffectKind::ArmorType
                    | data::UpgradeEffectKind::Spikes
                    | data::UpgradeEffectKind::UnlockAbility
                    | data::UpgradeEffectKind::DataOnly => {}
                }
            }
        }
    }

    /// Extra lumber carried per trip from researched `rlum` upgrades.
    pub fn lumber_bonus_for(&self, unit: UnitId) -> i32 {
        let Some(index) = self.unit_index(unit) else {
            return 0;
        };
        let def = self.units[index].def();
        let owner = self.units[index].owner.min(1) as usize;
        let mut bonus = 0;
        for (id, level) in &self.upgrades[owner] {
            let Some(upgrade) = data::upgrade_def(id) else {
                continue;
            };
            if !data::affected_by(def, id) {
                continue;
            }
            for effect in upgrade.effects {
                if effect.kind == data::UpgradeEffectKind::LumberHarvest {
                    bonus += effect.base + effect.step * level.saturating_sub(1) as i32;
                }
            }
        }
        bonus
    }

    /// Attack speed bonus in permille from researched `rats` upgrades.
    pub fn attack_speed_permille_for(&self, unit: UnitId) -> i32 {
        let Some(index) = self.unit_index(unit) else {
            return 0;
        };
        let def = self.units[index].def();
        let owner = self.units[index].owner.min(1) as usize;
        let mut bonus = 0;
        for (id, level) in &self.upgrades[owner] {
            let Some(upgrade) = data::upgrade_def(id) else {
                continue;
            };
            if !data::affected_by(def, id) {
                continue;
            }
            for effect in upgrade.effects {
                if effect.kind == data::UpgradeEffectKind::AttackSpeed {
                    bonus += effect.base + effect.step * level.saturating_sub(1) as i32;
                }
            }
        }
        bonus
    }

    /// Armor class after researched `rart` upgrades; falls back to the table
    /// class when nothing changed it.
    pub fn effective_armor_type(&self, index: usize) -> data::ArmorType {
        let unit = &self.units[index];
        let def = unit.def();
        for (id, _level) in &self.upgrades[unit.owner.min(1) as usize] {
            let Some(upgrade) = data::upgrade_def(id) else {
                continue;
            };
            if !data::affected_by(def, id) {
                continue;
            }
            for effect in upgrade.effects {
                if effect.kind == data::UpgradeEffectKind::ArmorType {
                    return data::ArmorType::from_index(effect.base);
                }
            }
        }
        def.stats.armor_type
    }

    /// Damage returned to a melee attacker by `rspi` (Spiked Barricade).
    fn spike_damage(&self, index: usize) -> i32 {
        let unit = &self.units[index];
        let def = unit.def();
        let mut damage = 0;
        for (id, level) in &self.upgrades[unit.owner.min(1) as usize] {
            let Some(upgrade) = data::upgrade_def(id) else {
                continue;
            };
            if !data::affected_by(def, id) {
                continue;
            }
            for effect in upgrade.effects {
                if effect.kind == data::UpgradeEffectKind::Spikes {
                    damage += effect.base + effect.step * level.saturating_sub(1) as i32;
                }
            }
        }
        damage
    }

    /// True when the unit owns the ability.  Statically listed abilities are
    /// gated when the install data attaches a research to them; abilities the
    /// unit does not list statically become available once the granting
    /// research (its `UnitBalance.slk` `upgrades` entry) is complete.
    pub fn unit_ability_unlocked(&self, unit: UnitId, ability_id: &str) -> bool {
        let Some(unit_ref) = self.unit(unit) else {
            return false;
        };
        let def = unit_ref.def();
        let owner = unit_ref.owner;
        let granted_by = data::ability_unlock_upgrade(ability_id)
            .filter(|upgrade_id| data::affected_by(def, upgrade_id));
        if def.ability_ids.contains(&ability_id) {
            return match granted_by {
                Some(upgrade_id) => self.upgrade_level(owner, upgrade_id) > 0,
                None => true,
            };
        }
        match granted_by {
            Some(upgrade_id) => self.upgrade_level(owner, upgrade_id) > 0,
            None => false,
        }
    }

    fn enqueue_production(
        &mut self,
        building: UnitId,
        def_index_value: u16,
        permissive: bool,
    ) -> Result<(), &'static str> {
        let Some(index) = self.unit_index(building) else {
            return Err("no building");
        };
        let owner = self.units[index].owner;
        if !self.units[index].alive || !self.units[index].is_building() {
            return Err("not a building");
        }
        if self.units[index].construct.is_some() {
            return Err("under construction");
        }
        if self.units[index].production.len() >= PRODUCTION_QUEUE_MAX {
            self.events.push(Event::Error("production queue full"));
            return Err("production queue full");
        }
        let def = def_at(def_index_value).ok_or("unknown unit")?;
        if !permissive {
            if !def.trained_at.contains(&self.units[index].def().id) {
                return Err("cannot train here");
            }
            for required in def.requires {
                let ok = self.units.iter().any(|unit| {
                    unit.alive
                        && unit.owner == owner
                        && unit.def().id == *required
                        && unit.construct.is_none()
                });
                if !ok {
                    return Err("missing requirement");
                }
            }
        }
        if self.resources[owner as usize] < def.stats.cost {
            self.events.push(Event::Error("not enough gold"));
            return Err("not enough gold");
        }
        if self.lumber[owner as usize] < def.stats.lumber_cost {
            self.events.push(Event::Error("not enough lumber"));
            return Err("not enough lumber");
        }
        self.resources[owner as usize] -= def.stats.cost;
        self.lumber[owner as usize] -= def.stats.lumber_cost;
        self.units[index].production.push(Production {
            kind: def.kind,
            remaining: def.stats.train_ticks.max(1),
            def: def_index_value,
            blocked: false,
        });
        Ok(())
    }

    /// Data-driven training with WC3 tech requirements.
    pub fn train_def(&mut self, building: UnitId, def_id: &str) -> Result<(), &'static str> {
        let index = def_index(def_id).ok_or("unknown unit")?;
        self.enqueue_production(building, index, false)
    }

    fn advance_production(&mut self) {
        let mut completed: Vec<(usize, u16)> = Vec::new();
        for index in 0..self.units.len() {
            if !self.units[index].alive || self.units[index].production.is_empty() {
                continue;
            }
            if self.units[index].construct.is_some() {
                continue;
            }
            if let Some(first) = self.units[index].production.first_mut() {
                if first.blocked {
                    // Retry blocked items: supply may have been freed.
                    completed.push((index, first.def));
                    continue;
                }
                first.remaining = first.remaining.saturating_sub(1);
                if first.remaining == 0 {
                    completed.push((index, first.def));
                }
            }
        }
        for (index, def_index_value) in completed {
            if !self.units[index].alive || self.units[index].production.is_empty() {
                continue;
            }
            let owner = self.units[index].owner;
            let def = match def_at(self.units[index].production[0].def) {
                Some(def) => def,
                None => continue,
            };
            if def_index_value != self.units[index].production[0].def {
                continue;
            }
            if self.supply_used(owner) + def.stats.supply > self.supply_cap(owner) {
                if !self.units[index].production[0].blocked {
                    self.units[index].production[0].blocked = true;
                    if let Some(first) = self.units[index].production.first_mut() {
                        first.remaining = 0;
                    }
                    self.events.push(Event::Error("supply blocked"));
                }
                continue;
            }
            let pos = self.exit_position(index);
            // The unit is placed on the producer's exit ring, never at the
            // building centre; record the move so placement is auditable.
            let building_pos = self.units[index].pos;
            self.log_spawn_adjust(def.id, building_pos, pos);
            let id = self.spawn_def_index(owner, def_index_value, pos, true);
            self.units[index].production.remove(0);
            self.events.push(Event::UnitTrained(id));
            if let Some(rally) = self.units[index].rally {
                if let Some(new_index) = self.unit_index(id) {
                    self.start_order(new_index, OrderSpec::Move(rally));
                }
            }
        }
    }

    /// World position at which a building drops its trained units.
    ///
    /// WC3 buildings place the new unit at the building's edge, oriented to
    /// one of eight exits, never in the middle of the model and never on a
    /// blocked/water cell. The search walks 24 directions at a radius that
    /// clears both the pathing footprint and the model rim, then expands in
    /// half-cell rings so a building hemmed in by trees still finds ground.
    fn exit_position(&self, building: usize) -> Vec2 {
        let unit = &self.units[building];
        let half = unit.def().footprint.max(1) as f32 * CELL_SIZE * 0.5;
        let radius = half + CELL_SIZE * 0.75;
        for ring in 0..12 {
            let ring_radius = radius + ring as f32 * CELL_SIZE * 0.5;
            for step in 0..24 {
                let angle = step as f32 * std::f32::consts::TAU / 24.0;
                let candidate = unit.pos + Vec2::new(angle.cos(), angle.sin()) * ring_radius;
                let cell = self.grid.world_to_cell(candidate);
                if self.grid.walkable(cell.0, cell.1) && self.can_stand(candidate, 12.0) {
                    return candidate;
                }
            }
        }
        // Last resort: snap a point outside the footprint to the nearest
        // walkable cell; the grid is authoritative (it carries WPM + water).
        self.walkable_pos_near(unit.pos + Vec2::new(radius, 0.0), 16)
            .unwrap_or(unit.pos)
    }

    /// Minimum distance from a building centre at which a unit is considered
    /// outside the structure (footprint half-extent plus one pathing cell).
    fn hall_clearance(&self, hall: UnitId) -> f32 {
        self.unit(hall)
            .map(|unit| unit.def().footprint.max(1) as f32 * CELL_SIZE * 0.5 + CELL_SIZE * 0.5)
            .unwrap_or(CELL_SIZE)
    }

    /// Records a spawn that was moved from `requested` to `actual`.
    ///
    /// The line names the unit def and both grid cells, so a placement bug is
    /// visible in the run's evidence instead of being inferred from unit
    /// positions after the fact. Capped by [`SPAWN_LOG_LIMIT`].
    fn log_spawn_adjust(&mut self, def_id: &str, requested: Vec2, actual: Vec2) {
        // The record is per grid cell (the unit of placement), so a sub-cell
        // nudge inside the same cell is not a move and is not logged.
        let from = self.grid.world_to_cell(requested);
        let to = self.grid.world_to_cell(actual);
        if from == to {
            return;
        }
        if self.spawn_log.len() >= SPAWN_LOG_LIMIT {
            self.spawn_log_dropped += 1;
            return;
        }
        self.spawn_log.push(format!(
            "SPAWN_ADJUST def={def_id} from={},{} to={},{}",
            from.0, from.1, to.0, to.1
        ));
    }

    /// Finds a worker start slot on the ring around a hall, widening the ring
    /// and drifting the angle until the spot is walkable and clear of the
    /// structure. Returns `None` only when the hall is fully boxed in.
    fn melee_worker_spot(&self, hall: UnitId, hall_pos: Vec2, slot: usize) -> Option<Vec2> {
        let clearance = self.hall_clearance(hall);
        let base_angle = std::f32::consts::TAU * slot as f32 / 5.0;
        for attempt in 0..32usize {
            let drift = (attempt / 2) as f32 * 0.31;
            let angle = if attempt % 2 == 0 {
                base_angle + drift
            } else {
                base_angle - drift
            };
            let radius = clearance + 40.0 + (attempt / 4) as f32 * 48.0;
            let wanted = hall_pos + Vec2::new(angle.cos(), angle.sin()) * radius;
            if let Some(pos) = self.walkable_pos_near(wanted, 10)
                && pos.distance(hall_pos) > clearance
                && self.can_stand(pos, 12.0)
            {
                return Some(pos);
            }
        }
        // Last resort: nearest walkable ground outside the structure, so a
        // worker is never dropped because the hall sits against water/cliffs.
        self.walkable_pos_near(hall_pos + Vec2::new(clearance + 48.0, 0.0), 24)
            .filter(|pos| pos.distance(hall_pos) > clearance)
    }

    /// Camp anchor for the guards of a gold mine: offset perpendicular to the
    /// hall->mine line so the workers walking mine->hall never enter the aggro
    /// radius (WC3 keeps the starting mine free of guards for this reason).
    fn camp_anchor(mine: Vec2, hall: Vec2) -> Vec2 {
        let away = mine - hall;
        let dir = if away.length_squared() > 1.0 {
            away.normalize()
        } else {
            Vec2::new(0.0, 1.0)
        };
        mine + Vec2::new(-dir.y, dir.x) * 520.0
    }

    /// Five neutral creeps guarding a gold mine, registered as one camp so the
    /// leash/aggro logic in `advance_creeps` owns them.
    fn spawn_melee_camp(&mut self, anchor: Vec2) -> usize {
        const CREEPS: [&str; 5] = ["nftr", "nftb", "nftt", "nkob", "nkog"];
        let mut ids = Vec::new();
        for (slot, id) in CREEPS.iter().enumerate() {
            let base = std::f32::consts::TAU * slot as f32 / CREEPS.len() as f32;
            // Real terrain (trees, water, cliffs) blocks part of the ring, so
            // each guard searches outward in widening rings instead of being
            // dropped — a camp always fields all five creeps.
            let mut placed = None;
            'search: for ring in 0..8u32 {
                let radius = 190.0 + ring as f32 * 56.0;
                for drift in 0..12u32 {
                    let angle = base + drift as f32 * (std::f32::consts::TAU / 48.0);
                    let wanted = anchor + Vec2::new(angle.cos(), angle.sin()) * radius;
                    if let Some(pos) = self.walkable_pos_near(wanted, 12)
                        && self.can_stand(pos, 16.0)
                    {
                        placed = Some(pos);
                        break 'search;
                    }
                }
            }
            // Last resort: the nearest walkable ground to the camp anchor. A
            // guard is never dropped because the ring happened to be water.
            let pos = placed
                .or_else(|| self.walkable_pos_near(anchor, 24))
                .filter(|pos| {
                    let cell = self.grid.world_to_cell(*pos);
                    self.grid.walkable(cell.0, cell.1)
                });
            if let Some(pos) = pos
                && let Some(unit) = self.spawn_def(2, id, pos)
            {
                ids.push(unit);
            }
        }
        // Tight leash: guards defend the camp, they do not chase into a base.
        self.register_camp(anchor, &ids, 350.0, 500.0);
        ids.len()
    }

    /// Instant building placement used by the command card (hall line).
    /// Deducts the exact roster cost once, atomically with the spawn; on
    /// insufficient gold it returns `DENY BUILD gold` and changes nothing.
    pub fn build_instant(
        &mut self,
        builder: UnitId,
        kind: UnitKind,
        at: Vec2,
    ) -> Result<UnitId, &'static str> {
        self.build_instant_impl(builder, kind, at, "DENY BUILD gold")
    }

    fn build_instant_impl(
        &mut self,
        builder: UnitId,
        kind: UnitKind,
        at: Vec2,
        insufficient: &'static str,
    ) -> Result<UnitId, &'static str> {
        let Some(builder_unit) = self.unit(builder) else {
            return Err("no builder");
        };
        if !builder_unit.alive {
            return Err("invalid builder");
        }
        let owner = builder_unit.owner;
        let def_index_value = def_index(kind.canonical_id()).ok_or("unknown building")?;
        let def = def_at(def_index_value).ok_or("unknown building")?;
        if !def.is_building() {
            return Err("not a building");
        }
        if !self.requirements_met(owner, def) {
            return Err("missing requirement");
        }
        if self.resources[owner as usize] < def.stats.cost {
            self.events.push(Event::Error(insufficient));
            return Err(insufficient);
        }
        if self.lumber[owner as usize] < def.stats.lumber_cost {
            self.events.push(Event::Error("not enough lumber"));
            return Err("not enough lumber");
        }
        self.resources[owner as usize] -= def.stats.cost;
        self.lumber[owner as usize] -= def.stats.lumber_cost;
        let id = self.spawn_def_index(owner, def_index_value, at, true);
        self.events.push(Event::BuildingComplete(id));
        Ok(id)
    }

    /// Placement validity for the UI ghost: the whole footprint must be on
    /// walkable ground and clear of living units' occupied cells.
    /// Full placement predicate: geometry, footprint overlap, tech requirements
    /// and cost. Returns the *reason* on refusal so the HUD ghost, the card and
    /// the placement path share one truth.
    ///
    /// Why: the ghost used `can_place` (geometry only), so a Watch Tower without
    /// its War Mill showed green, then the placement answered
    /// `DENY BUILD missing requirement` — the operator's «здания не размещаются».
    pub fn place_denial(&self, owner: u8, def_id: &str, at: Vec2) -> Option<&'static str> {
        self.place_denial_ignoring(owner, def_id, at, None)
    }

    /// [`Sim::place_denial`] with one unit exempted from the "unit inside the
    /// footprint" rule.
    ///
    /// The builder is that unit: `place_building_ordered` hands it an
    /// `Order::Build`, and the worker that walks onto its own site must not
    /// deny it (WC3 places the site and sends the peon in — exactly what the
    /// operator's `place_building_ordered` path does). Every other unit still
    /// refuses the placement, which is what keeps a hall from walling in a
    /// gatherer.
    fn place_denial_ignoring(
        &self,
        owner: u8,
        def_id: &str,
        at: Vec2,
        ignore: Option<UnitId>,
    ) -> Option<&'static str> {
        let Some(def) = data::def_of(def_id) else {
            return Some("unknown building");
        };
        if !def.stats.is_building {
            return Some("not a building");
        }
        let cells = self.footprint_cells(at, self.footprint_side_cells(def.footprint));
        if cells.is_empty() {
            return Some("site outside map");
        }
        if !cells.iter().all(|&(x, y)| self.grid.walkable(x, y)) {
            return Some("site blocked");
        }
        if self.units.iter().any(|unit| {
            Some(unit.id) != ignore
                && unit.alive
                && unit.blocked_cells.iter().any(|cell| cells.contains(cell))
        }) {
            return Some("site occupied");
        }
        // A unit standing inside the footprint would be walled in by its own
        // building (the operator saw `GATHER UNREACHABLE` after a hall landed on
        // a peon). WC3 refuses that placement outright — for every unit except
        // the builder, which is ordered into the site by the ordered variant.
        if self.units.iter().any(|unit| {
            Some(unit.id) != ignore
                && unit.alive
                && cells.contains(&self.grid.world_to_cell(unit.pos))
        }) {
            return Some("site occupied");
        }
        // A gold mine cannot be built over: the AI used to place its expansion
        // hall on the mine, which then blocked the builder's own approach.
        if self
            .gold_mines
            .iter()
            .any(|mine| cells.contains(&self.grid.world_to_cell(mine.pos)))
        {
            return Some("site blocked");
        }
        if !self.requirements_met(owner, def) {
            return Some("missing requirement");
        }
        if self.resources[owner as usize] < def.stats.cost {
            return Some("not enough gold");
        }
        if self.lumber[owner as usize] < def.stats.lumber_cost {
            return Some("not enough lumber");
        }
        None
    }

    /// Card affordance: can this owner start this building at all right now
    /// (requirements + gold + lumber), independent of the ground under the finger.
    pub fn can_build_now(&self, owner: u8, def_id: &str) -> bool {
        let Some(def) = data::def_of(def_id) else {
            return false;
        };
        def.stats.is_building
            && self.requirements_met(owner, def)
            && self.resources[owner as usize] >= def.stats.cost
            && self.lumber[owner as usize] >= def.stats.lumber_cost
    }

    pub fn can_place(&self, def_id: &str, at: Vec2) -> bool {
        let Some(def) = data::def_of(def_id) else {
            return false;
        };
        if !def.stats.is_building {
            return false;
        }
        let cells = self.footprint_cells(at, self.footprint_side_cells(def.footprint));
        if cells.is_empty() {
            return false;
        }
        cells.iter().all(|&(x, y)| self.grid.walkable(x, y))
            && !self.units.iter().any(|unit| {
                unit.alive && unit.blocked_cells.iter().any(|cell| cells.contains(cell))
            })
    }

    /// Places a construction site.  The worker must be nearby; the building
    /// completes when a worker channels it for `build_ticks`.
    pub fn place_building(
        &mut self,
        builder: UnitId,
        def_id: &str,
        at: Vec2,
    ) -> Result<UnitId, &'static str> {
        self.place_building_impl(builder, def_id, at, true)
    }

    /// Player/JASS order variant: the site may be far from the worker.  The
    /// worker walks to it (`Order::Build`) and channels when in range, which is
    /// the WC3 `IssueBuildOrder` behaviour.
    pub fn place_building_ordered(
        &mut self,
        builder: UnitId,
        def_id: &str,
        at: Vec2,
    ) -> Result<UnitId, &'static str> {
        self.place_building_impl(builder, def_id, at, false)
    }

    fn place_building_impl(
        &mut self,
        builder: UnitId,
        def_id: &str,
        at: Vec2,
        require_near: bool,
    ) -> Result<UnitId, &'static str> {
        let builder_index = self.unit_index(builder).ok_or("no builder")?;
        let builder_unit = &self.units[builder_index];
        if !builder_unit.alive || builder_unit.is_building() {
            return Err("invalid builder");
        }
        if require_near && builder_unit.pos.distance(at) > 420.0 {
            return Err("builder too far");
        }
        let owner = builder_unit.owner;
        let def_index_value = def_index(def_id).ok_or("unknown building")?;
        let def = def_at(def_index_value).ok_or("unknown building")?;
        if !def.is_building() {
            return Err("not a building");
        }
        if let Some(reason) = self.place_denial_ignoring(owner, def_id, at, Some(builder)) {
            return Err(reason);
        }
        self.resources[owner as usize] -= def.stats.cost;
        self.lumber[owner as usize] -= def.stats.lumber_cost;
        let id = self.spawn_def_index(owner, def_index_value, at, false);
        if let Some(site_index) = self.unit_index(id) {
            self.start_order(builder_index, OrderSpec::Build(id));
            self.units[site_index].facing = 0.0;
        }
        Ok(id)
    }

    pub fn cancel_construction(&mut self, building: UnitId) -> bool {
        let Some(index) = self.unit_index(building) else {
            return false;
        };
        if self.units[index].construct.is_none() {
            return false;
        }
        let owner = self.units[index].owner;
        let (cost, lumber) = {
            let stats = self.units[index].def().stats;
            (stats.cost, stats.lumber_cost)
        };
        let cells = std::mem::take(&mut self.units[index].blocked_cells);
        for (cx, cy) in cells {
            let occupied = self
                .units
                .iter()
                .any(|unit| unit.alive && unit.blocked_cells.contains(&(cx, cy)));
            if !occupied {
                self.grid.set_walkable(cx, cy, true);
            }
        }
        self.units[index].alive = false;
        self.units[index].order = Order::Idle;
        self.events.push(Event::Error("construction cancelled"));
        self.resources[owner as usize] += cost;
        self.lumber[owner as usize] += lumber;
        true
    }

    /// Repairs channeled by workers: heals `REPAIR_HP_PER_TICK` per worker at
    /// the building's position and charges gold/lumber at 50% of build cost.
    fn advance_repairs(&mut self) {
        let mut finished: Vec<usize> = Vec::new();
        for index in 0..self.units.len() {
            if !self.units[index].alive {
                continue;
            }
            let Order::Repair(target) = self.units[index].order else {
                continue;
            };
            let Some(target_index) = self.unit_index(target) else {
                finished.push(index);
                continue;
            };
            if !self.units[target_index].alive
                || self.units[target_index].construct.is_some()
                || self.units[target_index].owner != self.units[index].owner
            {
                finished.push(index);
                continue;
            }
            if self.units[target_index].hp >= self.units[target_index].max_hp {
                finished.push(index);
                continue;
            }
            if self.units[index].pos.distance(self.units[target_index].pos)
                > self.build_channel_range(self.units[target_index].def().footprint)
                    + BUILD_CHANNEL_SLACK
            {
                continue;
            }
            let max_hp = self.units[target_index].max_hp.max(1);
            let stats = self.units[target_index].def().stats;
            let gain = REPAIR_HP_PER_TICK.min(max_hp - self.units[target_index].hp);
            let gold_cost = (stats.cost as i64 * gain as i64 * REPAIR_COST_PERMILLE as i64
                / (1000 * max_hp as i64)) as i32;
            let lumber_cost = (stats.lumber_cost as i64 * gain as i64 * REPAIR_COST_PERMILLE as i64
                / (1000 * max_hp as i64)) as i32;
            let owner = self.units[index].owner as usize;
            if self.resources[owner] < gold_cost || self.lumber[owner] < lumber_cost {
                finished.push(index);
                self.events
                    .push(Event::Error("not enough resources to repair"));
                continue;
            }
            self.resources[owner] -= gold_cost;
            self.lumber[owner] -= lumber_cost;
            self.units[target_index].hp += gain;
            if self.units[target_index].hp >= self.units[target_index].max_hp {
                finished.push(index);
            }
        }
        for index in finished {
            self.finish_order(index);
        }
    }

    fn advance_construction(&mut self) {
        let mut completed: Vec<usize> = Vec::new();
        for index in 0..self.units.len() {
            if !self.units[index].alive {
                continue;
            }
            let Some(mut construction) = self.units[index].construct else {
                continue;
            };
            let owner = self.units[index].owner;
            let site = self.units[index].id;
            let site_pos = self.units[index].pos;
            // Same ring the builder walks to (`Order::Build` stop distance) —
            // a site that blocked its own footprint used to leave the builder
            // standing 438 units out while this check demanded 320.
            let reach =
                self.build_channel_range(self.units[index].def().footprint) + BUILD_CHANNEL_SLACK;
            let workers = self
                .units
                .iter()
                .filter(|unit| {
                    unit.alive
                        && unit.owner == owner
                        && !unit.is_building()
                        && matches!(unit.order, Order::Build(target) if target == site)
                        && unit.pos.distance(site_pos) <= reach
                })
                .count()
                .min(2) as u32;
            if workers == 0 {
                continue;
            }
            construction.remaining = construction.remaining.saturating_sub(workers);
            let def = self.units[index].def();
            let total = construction.total.max(1);
            let progress = 1.0 - construction.remaining as f32 / total as f32;
            self.units[index].hp = ((def.stats.hp as f32 * (0.1 + 0.9 * progress)).round() as i32)
                .clamp(1, def.stats.hp);
            self.units[index].construct = Some(construction);
            if construction.remaining == 0 {
                completed.push(index);
            }
        }
        for index in completed {
            self.units[index].construct = None;
            self.units[index].hp = self.units[index].max_hp;
            let id = self.units[index].id;
            self.events.push(Event::BuildingComplete(id));
        }
    }

    /// Sets the rally point for a producer.
    pub fn set_rally(&mut self, building: UnitId, rally: Option<Vec2>) {
        if let Some(index) = self.unit_index(building) {
            self.units[index].rally = rally;
        }
    }

    // ------------------------------------------------------------------
    // Economy
    // ------------------------------------------------------------------

    /// Requests a fresh path for a gather phase change (gather -> ToHall,
    /// deposit -> ToMine).  Without it a return trip is steered on the straight
    /// line only: a worker whose line crosses the mine footprint (or a tree
    /// line) stalls until one bounded retry runs out and `finish_order` drops it
    /// to `Idle` — on `(2)Justice` that silently cost one of five workers every
    /// trip while the other four kept paying.
    fn request_gather_path(&mut self, index: usize, target: Vec2) {
        if let Some(cell) = self.nearest_walkable_cell(self.grid.world_to_cell(target), 8) {
            self.request_path(index, cell);
        }
    }

    fn advance_economy(&mut self) {
        if self.passive_income && self.tick % 15 == 0 {
            for player in 0..2 {
                self.resources[player] += 1;
            }
        }
        for index in 0..self.units.len() {
            let unit = &self.units[index];
            if !unit.alive || unit.is_building() {
                continue;
            }
            let Order::Harvest { mine, phase } = unit.order else {
                continue;
            };
            let owner = unit.owner;
            let pos = unit.pos;
            match phase {
                HarvestPhase::ToMine => {
                    if pos.distance(mine) <= self.gather_gate(mine, 56.0) {
                        self.units[index].order = Order::Harvest {
                            mine,
                            phase: HarvestPhase::Gathering,
                        };
                        self.units[index].harvest_timer = HARVEST_GATHER_TICKS;
                        self.units[index].path.clear();
                    }
                }
                HarvestPhase::Gathering => {
                    if self.units[index].harvest_timer > 0 {
                        self.units[index].harvest_timer -= 1;
                        continue;
                    }
                    let mine_index = self
                        .gold_mines
                        .iter()
                        .position(|entry| entry.pos.distance(mine) <= self.gather_gate(mine, 56.0));
                    let available = mine_index
                        .map(|mine_index| self.gold_mines[mine_index].remaining)
                        .unwrap_or(HARVEST_CAPACITY);
                    if available <= 0 {
                        self.units[index].order = Order::Idle;
                        self.units[index].harvest = None;
                        self.events.push(Event::Error("gold mine depleted"));
                        continue;
                    }
                    let load = HARVEST_CAPACITY.min(available);
                    if let Some(mine_index) = mine_index {
                        self.gold_mines[mine_index].remaining -= load;
                    }
                    self.units[index].carrying = load;
                    if let Some(hall) = self.nearest_hall(owner, pos) {
                        self.units[index].goal = Some(hall);
                        self.units[index].order = Order::Harvest {
                            mine,
                            phase: HarvestPhase::ToHall,
                        };
                        self.units[index].path.clear();
                        self.request_gather_path(index, hall);
                    } else {
                        // No hall: legacy behaviour pays out at the mine.
                        let _ = self.deposit_gold(owner, load);
                        self.units[index].carrying = 0;
                        self.units[index].order = Order::Harvest {
                            mine,
                            phase: HarvestPhase::ToMine,
                        };
                        self.units[index].harvest_timer = 0;
                        self.request_gather_path(index, mine);
                    }
                }
                HarvestPhase::ToHall => {
                    let Some(hall_pos) = self.nearest_hall(owner, pos) else {
                        // Hall destroyed mid-trip: bank the load locally.
                        let load = self.units[index].carrying;
                        if load > 0 {
                            self.units[index].carrying = 0;
                            let _ = self.deposit_gold(owner, load);
                        }
                        self.units[index].order = Order::Idle;
                        self.units[index].harvest = None;
                        continue;
                    };
                    if pos.distance(hall_pos) <= self.gather_gate(hall_pos, 160.0).max(210.0) {
                        let load = self.units[index].carrying;
                        self.units[index].carrying = 0;
                        if load > 0 {
                            let _ = self.deposit_gold(owner, load);
                        }
                        self.units[index].goal = Some(mine);
                        self.units[index].order = Order::Harvest {
                            mine,
                            phase: HarvestPhase::ToMine,
                        };
                        self.units[index].harvest_timer = 0;
                        self.units[index].path.clear();
                        self.request_gather_path(index, mine);
                    }
                }
            }
        }

        for index in 0..self.units.len() {
            let unit = &self.units[index];
            if !unit.alive || unit.is_building() {
                continue;
            }
            let Order::HarvestLumber { tree, phase } = unit.order else {
                continue;
            };
            let owner = unit.owner;
            let pos = unit.pos;
            match phase {
                HarvestPhase::ToMine => {
                    let gate = self.gather_gate(tree, 64.0);
                    let near = pos.distance(tree) <= gate + 200.0;
                    let tree_alive = !near || self.destructable_at(tree).is_some();
                    if !tree_alive {
                        // The ordered tree was felled while the worker walked
                        // to it: keep the lumber loop running on a neighbour.
                        if let Some(next) = self.nearest_destructable(pos, HARVEST_RETARGET_RADIUS) {
                            self.units[index].order = Order::HarvestLumber {
                                tree: next,
                                phase: HarvestPhase::ToMine,
                            };
                            self.units[index].harvest = Some(next);
                            self.units[index].goal = Some(next);
                            self.units[index].path.clear();
                            self.units[index].path_index = 0;
                            if let Some(cell) = self
                                .nearest_walkable_cell(self.grid.world_to_cell(next), 6)
                            {
                                self.request_path(index, cell);
                            }
                        } else {
                            self.units[index].order = Order::Idle;
                            self.units[index].harvest = None;
                            self.events.push(Event::Error("no tree"));
                        }
                        continue;
                    }
                    if pos.distance(tree) <= gate {
                        self.units[index].order = Order::HarvestLumber {
                            tree,
                            phase: HarvestPhase::Gathering,
                        };
                        self.units[index].harvest_timer = HARVEST_GATHER_TICKS;
                        self.units[index].path.clear();
                    }
                }
                HarvestPhase::Gathering => {
                    if self.units[index].harvest_timer > 0 {
                        self.units[index].harvest_timer -= 1;
                        continue;
                    }
                    let Some(tree_index) = self.destructable_at(tree) else {
                        // Tree gone while chopping: retarget like WC3 instead
                        // of dropping the worker to Idle mid-forest.
                        if let Some(next) = self.nearest_destructable(pos, HARVEST_RETARGET_RADIUS) {
                            self.units[index].order = Order::HarvestLumber {
                                tree: next,
                                phase: HarvestPhase::ToMine,
                            };
                            self.units[index].harvest = Some(next);
                            self.units[index].goal = Some(next);
                            self.units[index].harvest_timer = 0;
                            self.units[index].path.clear();
                            self.units[index].path_index = 0;
                            if let Some(cell) = self
                                .nearest_walkable_cell(self.grid.world_to_cell(next), 6)
                            {
                                self.request_path(index, cell);
                            }
                        } else {
                            self.units[index].order = Order::Idle;
                            self.units[index].harvest = None;
                            self.events.push(Event::Error("no tree"));
                        }
                        continue;
                    };
                    let tree_id = self.destructables[tree_index].id;
                    let available = self.destructables[tree_index].hp;
                    let capacity = HARVEST_LUMBER_CAPACITY + self.lumber_bonus_for(self.units[index].id);
                    let load = capacity.min(available);
                    self.damage_destructable(tree_id, load);
                    self.units[index].carrying_lumber = load;
                    if let Some(hall) = self.nearest_hall(owner, pos) {
                        self.units[index].goal = Some(hall);
                        self.units[index].order = Order::HarvestLumber {
                            tree,
                            phase: HarvestPhase::ToHall,
                        };
                        self.units[index].path.clear();
                        self.request_gather_path(index, hall);
                    } else {
                        let _ = self.deposit_lumber(owner, load);
                        self.units[index].carrying_lumber = 0;
                        self.units[index].order = Order::HarvestLumber {
                            tree,
                            phase: HarvestPhase::ToMine,
                        };
                        self.units[index].harvest_timer = 0;
                        self.request_gather_path(index, tree);
                    }
                }
                HarvestPhase::ToHall => {
                    let Some(hall_pos) = self.nearest_hall(owner, pos) else {
                        let load = self.units[index].carrying_lumber;
                        if load > 0 {
                            self.units[index].carrying_lumber = 0;
                            let _ = self.deposit_lumber(owner, load);
                        }
                        self.units[index].order = Order::Idle;
                        self.units[index].harvest = None;
                        continue;
                    };
                    if pos.distance(hall_pos) <= self.gather_gate(hall_pos, 160.0).max(210.0) {
                        let load = self.units[index].carrying_lumber;
                        self.units[index].carrying_lumber = 0;
                        if load > 0 {
                            let _ = self.deposit_lumber(owner, load);
                        }
                        self.units[index].goal = Some(tree);
                        self.units[index].order = Order::HarvestLumber {
                            tree,
                            phase: HarvestPhase::ToMine,
                        };
                        self.units[index].harvest_timer = 0;
                        self.units[index].path.clear();
                        self.request_gather_path(index, tree);
                    }
                }
            }
        }
    }

    /// Deposits gold after the upkeep tax (paid amount is returned).
    pub fn deposit_gold(&mut self, owner: u8, amount: i32) -> i32 {
        if amount <= 0 {
            return 0;
        }
        let paid = (amount * self.upkeep(owner).income_permille() / 1000).max(1);
        self.resources[owner as usize] += paid;
        self.events.push(Event::GoldDeposited {
            owner,
            amount: paid,
        });
        paid
    }

    // ------------------------------------------------------------------
    // Supply / upkeep
    // ------------------------------------------------------------------

    pub fn supply_used(&self, owner: u8) -> i32 {
        self.units
            .iter()
            .filter(|unit| unit.alive && unit.owner == owner)
            .map(|unit| unit.def().stats.supply)
            .sum()
    }

    pub fn supply_cap(&self, owner: u8) -> i32 {
        let cap: i32 = self
            .units
            .iter()
            .filter(|unit| {
                unit.alive && unit.owner == owner && unit.is_building() && unit.construct.is_none()
            })
            .map(|unit| unit.def().stats.supply_provided)
            .sum();
        let max = match self.ruleset {
            Ruleset::Roc => 90,
            Ruleset::Tft => MAX_SUPPLY,
        };
        cap.min(max)
    }

    /// Main town hall of `owner`, if any (farms/burrows are not halls).
    pub fn main_hall(&self, owner: u8) -> Option<UnitId> {
        self.units
            .iter()
            .find(|unit| {
                unit.alive
                    && unit.owner == owner
                    && unit.is_building()
                    && unit.def().is_main_hall()
                    && unit.construct.is_none()
            })
            .map(|unit| unit.id)
    }

    /// Race-correct supply building (farm/burrow/ziggurat/moon well) for the
    /// owner's main hall, or `None` when the race has no data.
    pub fn supply_building_for_owner(&self, owner: u8) -> Option<&'static UnitDef> {
        let race = self
            .main_hall(owner)
            .and_then(|id| self.unit(id))
            .map(|unit| unit.def().race)
            .unwrap_or(Race::Human);
        data::supply_building_for(race)
    }

    pub fn set_ruleset(&mut self, ruleset: Ruleset) {
        self.ruleset = ruleset;
    }

    pub fn ruleset(&self) -> Ruleset {
        self.ruleset
    }

    /// Race-correct, SLK-derived worker build menu for `owner`.
    pub fn build_menu_for_owner(&self, owner: u8) -> Vec<&'static UnitDef> {
        let race = self
            .main_hall(owner)
            .and_then(|id| self.unit(id))
            .map(|unit| unit.def().race)
            .unwrap_or(Race::Human);
        data::build_menu(race)
    }

    /// WC3 upkeep tier for the supply currently used: 1000 = no upkeep,
    /// 700 = low (51-80), 400 = high (81+).
    /// Current upkeep tier: TFT 50/80 food, RoC 40/70.
    pub fn upkeep(&self, owner: u8) -> UpkeepTier {
        let supply = self.supply_used(owner);
        let (low, high) = match self.ruleset {
            Ruleset::Tft => (UPKEEP_LOW_FOOD, UPKEEP_HIGH_FOOD),
            Ruleset::Roc => (40, 70),
        };
        if supply > high {
            UpkeepTier::High
        } else if supply > low {
            UpkeepTier::Low
        } else {
            UpkeepTier::None
        }
    }

    /// HUD-facing upkeep label (`NO UPKEEP` / `LOW UPKEEP` / `HIGH UPKEEP`).
    pub fn upkeep_label(&self, owner: u8) -> &'static str {
        self.upkeep(owner).label()
    }

    pub fn upkeep_permille(&self, owner: u8) -> i32 {
        self.upkeep(owner).income_permille()
    }

    pub fn can_afford(&self, owner: u8, def_id: &str) -> bool {
        def_of(def_id)
            .map(|def| self.resources[owner as usize] >= def.stats.cost)
            .unwrap_or(false)
    }

    // ------------------------------------------------------------------
    // Footprints and fog
    // ------------------------------------------------------------------

    /// Footprint side in *this* grid's cells.
    ///
    /// `UnitDef::footprint` is authored at the WC3 32-unit pathing resolution
    /// (a Great Hall is 6). A coarser grid needs proportionally fewer cells: the
    /// sim tests build 128-unit grids, the game builds 32-unit ones, and a single
    /// static number cannot be right for both.
    /// World distance from a building's centre at which a worker channels its
    /// construction or repair.  Single source of truth: the `Order::Build` /
    /// `Order::Repair` movement stop, `advance_construction` and
    /// `advance_repairs` all read it, and it always lands outside the
    /// building's blocked cells.  When the channel check was authored
    /// separately (a flat 320), a builder walked to its 438-unit stop ring for
    /// a town hall, stood still outside the check radius and the site never
    /// gained progress.
    fn build_channel_range(&self, footprint: u8) -> f32 {
        BUILD_CHANNEL_BASE + footprint.max(1) as f32 * BUILD_CHANNEL_PER_CELL
    }

    fn footprint_side_cells(&self, footprint: u8) -> u8 {
        let world = footprint.max(1) as f32 * crate::PATHING_CELL_SIZE;
        ((world / self.grid.cell_size.max(1.0)).ceil() as u8).max(1)
    }

    fn footprint_cells(&self, center: Vec2, side: u8) -> Vec<(u32, u32)> {
        let side = side.max(1) as i32;
        let (cx, cy) = self.grid.world_to_cell(center);
        // Centre the square on the anchor cell. `side / 2` (not `(side-1)/2`)
        // keeps even footprints straddling the centre instead of extending only
        // right/down, which left a half-cell gap a second building could occupy.
        let start_x = cx as i32 - side / 2;
        let start_y = cy as i32 - side / 2;
        let mut cells = Vec::with_capacity((side * side) as usize);
        for y in start_y..start_y + side {
            for x in start_x..start_x + side {
                if x < 0 || y < 0 || x >= self.grid.width as i32 || y >= self.grid.height as i32 {
                    return Vec::new();
                }
                cells.push((x as u32, y as u32));
            }
        }
        cells
    }

    /// Rebuilds the per-player fog for the current tick.
    ///
    /// Visibility is stamped from every living player-owned unit *and
    /// building*: `Unit::sight` comes from `UnitBalance.slk`, so a building
    /// reveals its neighbourhood for as long as it stands and the mask drops
    /// back to explored the tick it dies.  `explored` is monotonic (never
    /// cleared) and `last_seen` records where each player last saw an enemy
    /// unit, which is what the HUD draws as a last-known blip.
    fn update_fog(&mut self) {
        self.visibility.fill(0);
        for unit in self.units.iter() {
            if !unit.alive || unit.owner > 1 {
                continue;
            }
            let marker = (self.tick as u32)
                .wrapping_mul(4)
                .wrapping_add(unit.owner as u32 + 1);
            let radius = ((unit.sight / self.grid.cell_size).ceil() as usize).min(64);
            let (cx, cy) = self.grid.world_to_cell(unit.pos);
            let bit = 1u8 << unit.owner;
            for &(dx, dy) in &self.fog_offsets[radius] {
                let x = cx as i32 + dx;
                let y = cy as i32 + dy;
                if x < 0 || y < 0 || x >= self.grid.width as i32 || y >= self.grid.height as i32 {
                    continue;
                }
                let index = y as usize * self.grid.width as usize + x as usize;
                if self.fog_stamp[index] == marker {
                    continue;
                }
                self.fog_stamp[index] = marker;
                self.visibility[index] |= bit;
                self.explored[unit.owner as usize][index] = 1;
            }
        }
        // Last-known enemy positions: a unit seen this tick overwrites the
        // stored blip; units hidden by fog keep their older entry.
        for index in 0..self.units.len() {
            let unit = &self.units[index];
            if !unit.alive || unit.owner > 2 {
                continue;
            }
            let (id, pos, owner) = (unit.id.0, unit.pos, unit.owner);
            for player in 0..2u8 {
                if owner == player {
                    continue;
                }
                if self.is_visible(player, pos) {
                    self.last_seen[player as usize].insert(
                        id,
                        LastSeen {
                            pos,
                            tick: self.tick,
                        },
                    );
                }
            }
        }
    }

    pub fn is_visible(&self, owner: u8, pos: Vec2) -> bool {
        if owner > 1 {
            return true;
        }
        let (x, y) = self.grid.world_to_cell(pos);
        if x >= self.grid.width || y >= self.grid.height {
            return false;
        }
        self.visibility[y as usize * self.grid.width as usize + x as usize] & (1 << owner) != 0
    }

    pub fn is_explored(&self, owner: u8, pos: Vec2) -> bool {
        if owner > 1 {
            return true;
        }
        let (x, y) = self.grid.world_to_cell(pos);
        if x >= self.grid.width || y >= self.grid.height {
            return false;
        }
        self.explored[owner as usize][y as usize * self.grid.width as usize + x as usize] != 0
    }

    /// Three-state fog at a pathing cell (`Unexplored | Explored | Visible`).
    /// Out-of-grid cells read as `Unexplored` for a player and `Visible` for a
    /// non-player owner (the neutral/omniscient view).
    pub fn visibility(&self, player: u8, cell: (u32, u32)) -> Visibility {
        if player > 1 {
            return Visibility::Visible;
        }
        let (x, y) = cell;
        if x >= self.grid.width || y >= self.grid.height {
            return Visibility::Unexplored;
        }
        let index = y as usize * self.grid.width as usize + x as usize;
        if self.visibility[index] & (1 << player) != 0 {
            Visibility::Visible
        } else if self.explored[player as usize][index] != 0 {
            Visibility::Explored
        } else {
            Visibility::Unexplored
        }
    }

    /// Three-state fog at a world position (the cell that contains it).
    pub fn visibility_at(&self, player: u8, pos: Vec2) -> Visibility {
        self.visibility(player, self.grid.world_to_cell(pos))
    }

    /// Single-point visibility for the observing player (owner 0).
    ///
    /// This is the hook the renderer uses to skip enemy models in fog; the
    /// two-player form above (`is_visible(player, pos)`) stays the canonical
    /// per-player query for the HUD and the minimap.
    pub fn is_visible_at(&self, pos: Vec2) -> bool {
        self.is_visible(0, pos)
    }

    /// Whether `player` hears a sound emitted at `pos` with an audible
    /// `radius`: the source is visible, or a friendly unit is close enough to
    /// hear it (`wc3-game` gates one-shot sound effects through this).
    pub fn is_audible(&self, player: u8, pos: Vec2, radius: f32) -> bool {
        if self.is_visible(player, pos) {
            return true;
        }
        let radius = radius.max(0.0);
        self.units.iter().any(|unit| {
            unit.alive && unit.owner == player && unit.pos.distance(pos) <= radius + unit.radius()
        })
    }

    /// Fraction of the grid the player has explored (0.0..=1.0).
    pub fn explored_fraction(&self, player: u8) -> f32 {
        if player > 1 {
            return 1.0;
        }
        let grid = &self.explored[player as usize];
        if grid.is_empty() {
            return 0.0;
        }
        let seen = grid.iter().filter(|state| **state != 0).count();
        seen as f32 / grid.len() as f32
    }

    /// Monotonic explored grid for `player` (one byte per pathing cell).
    pub fn explored_grid(&self, player: u8) -> &[u8] {
        if player > 1 {
            &[]
        } else {
            &self.explored[player as usize]
        }
    }

    /// Per-player fog mask in the `wc3_render::fog::FogMask` encoding
    /// (0 unexplored, 1 explored, 2 visible).
    pub fn fog_mask(&self, player: u8) -> Vec<u8> {
        let cells = self.grid.width as usize * self.grid.height as usize;
        let mut mask = vec![0u8; cells];
        if player > 1 {
            mask.fill(Visibility::Visible.as_u8());
            return mask;
        }
        for (index, state) in mask.iter_mut().enumerate() {
            *state = if self.visibility[index] & (1 << player) != 0 {
                Visibility::Visible.as_u8()
            } else if self.explored[player as usize][index] != 0 {
                Visibility::Explored.as_u8()
            } else {
                Visibility::Unexplored.as_u8()
            };
        }
        mask
    }

    /// Last position `player` observed enemy unit `unit` at.
    pub fn last_seen(&self, player: u8, unit: UnitId) -> Option<Vec2> {
        self.last_seen_entry(player, unit).map(|seen| seen.pos)
    }

    /// Last-seen position plus the tick it was observed on.
    pub fn last_seen_entry(&self, player: u8, unit: UnitId) -> Option<LastSeen> {
        if player > 1 {
            return None;
        }
        self.last_seen[player as usize].get(&unit.0).copied()
    }

    /// Every last-known enemy blip for `player`, ordered by unit id.
    pub fn last_seen_all(&self, player: u8) -> Vec<(UnitId, LastSeen)> {
        if player > 1 {
            return Vec::new();
        }
        self.last_seen[player as usize]
            .iter()
            .map(|(id, seen)| (UnitId(*id), *seen))
            .collect()
    }

    // ------------------------------------------------------------------
    // Script-facing unit mutations (JASS adapter)
    // ------------------------------------------------------------------

    /// Sets hit points without armour; used by the JASS bridge for
    /// `SetUnitState(UNIT_STATE_LIFE)`.  Zero kills the unit.
    pub fn set_unit_hp(&mut self, id: UnitId, hp: i32) {
        let Some(index) = self.unit_index(id) else {
            return;
        };
        if !self.units[index].alive {
            return;
        }
        let max_hp = self.units[index].max_hp.max(1);
        let hp = hp.clamp(0, max_hp);
        self.units[index].hp = hp;
        if hp == 0 {
            self.kill_unit(id);
        }
    }

    pub fn set_unit_mana(&mut self, id: UnitId, mana: i32) {
        let Some(index) = self.unit_index(id) else {
            return;
        };
        let max_mana = self.units[index].max_mana.max(0);
        self.units[index].mana = mana.clamp(0, max_mana);
    }

    pub fn set_unit_owner(&mut self, id: UnitId, owner: u8) {
        let Some(index) = self.unit_index(id) else {
            return;
        };
        self.units[index].owner = owner.min(1);
    }

    /// Teleports a unit (JASS `SetUnitPosition*`), cancelling its current path.
    pub fn teleport_unit(&mut self, id: UnitId, pos: Vec2) {
        let world_x = self.grid.width as f32 * CELL_SIZE;
        let world_y = self.grid.height as f32 * CELL_SIZE;
        let Some(index) = self.unit_index(id) else {
            return;
        };
        self.units[index].pos = Vec2::new(pos.x.clamp(0.0, world_x), pos.y.clamp(0.0, world_y));
        self.units[index].path.clear();
        self.units[index].path_index = 0;
        self.units[index].stuck_ticks = 0;
    }

    /// Kills a unit immediately, freeing its building footprint and emitting
    /// `Event::UnitDied`.  Used by `KillUnit` and by script state diffs.
    pub fn kill_unit(&mut self, id: UnitId) {
        let Some(index) = self.unit_index(id) else {
            return;
        };
        if !self.units[index].alive {
            return;
        }
        self.units[index].alive = false;
        self.units[index].hp = 0;
        self.units[index].path.clear();
        self.units[index].path_index = 0;
        self.units[index].order = Order::Idle;
        self.units[index].order_queue.clear();
        self.units[index].production.clear();
        self.units[index].construct = None;
        let cells = std::mem::take(&mut self.units[index].blocked_cells);
        for (cx, cy) in cells {
            let occupied = self
                .units
                .iter()
                .any(|unit| unit.alive && unit.blocked_cells.contains(&(cx, cy)));
            if !occupied {
                self.grid.set_walkable(cx, cy, true);
            }
        }
        self.flow_cache.clear();
        self.events.push(Event::UnitDied(id));
        self.clear_buffs(id);
        self.creep_drop(index);
        // Script/JASS kills (`KillUnit`, `SetUnitState(LIFE,0)`, summon expiry)
        // drop the backpack in the same tick.
        self.drop_items_on_death(id);
    }

    // ------------------------------------------------------------------
    // Victory / hash / rng
    // ------------------------------------------------------------------

    fn check_victory(&mut self) {
        // WC3 melee elimination rule: a side is defeated when it has no
        // buildings left — not when its town hall falls. A seat with a
        // barracks, an altar or a farm still rebuilds (this is why the game
        // layer could not use the old hall-only verdict for melee).
        let has_buildings = |units: &[Unit], owner: u8| {
            units
                .iter()
                .any(|unit| unit.alive && unit.owner == owner && unit.def().is_building())
        };
        let side0 = has_buildings(&self.units, 0);
        let side1 = has_buildings(&self.units, 1);
        if !side0 && side1 {
            self.events.push(Event::Victory(1));
        } else if side0 && !side1 {
            self.events.push(Event::Victory(0));
        }
    }

    pub fn random(&mut self, max: u32) -> u32 {
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_state = x;
        if max == 0 { 0 } else { (x % max as u64) as u32 }
    }
    /// Deterministic FNV-1a hash of the simulation state.
    pub fn state_hash(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let mut feed = |value: u64| {
            hash ^= value;
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        };
        feed(self.tick);
        feed(self.resources[0] as u64);
        feed(self.resources[1] as u64);
        feed(self.lumber[0] as u64);
        feed(self.lumber[1] as u64);
        feed(self.rng_state);
        for mine in &self.gold_mines {
            feed(mine.pos.x.to_bits() as u64);
            feed(mine.pos.y.to_bits() as u64);
            feed(mine.remaining as u64);
        }
        for unit in &self.units {
            feed(unit.id.0 as u64);
            feed(unit.owner as u64);
            feed(unit.def_index as u64);
            feed(unit.pos.x.to_bits() as u64);
            feed(unit.pos.y.to_bits() as u64);
            feed(unit.hp as u64);
            feed(unit.mana as u64);
            feed(unit.cooldown as u64);
            feed(unit.alive as u64);
            feed(unit.carrying as u64);
            feed(unit.facing.to_bits() as u64);
            if let Some((a, b)) = unit.patrol {
                feed(a.x.to_bits() as u64);
                feed(a.y.to_bits() as u64);
                feed(b.x.to_bits() as u64);
                feed(b.y.to_bits() as u64);
            }
            match unit.order {
                Order::Idle => feed(0),
                Order::Move(to) => {
                    feed(1);
                    feed(to.x.to_bits() as u64);
                    feed(to.y.to_bits() as u64);
                }
                Order::Attack(target) => {
                    feed(2);
                    feed(target.0 as u64);
                }
                Order::MoveAttack(to) => {
                    feed(3);
                    feed(to.x.to_bits() as u64);
                    feed(to.y.to_bits() as u64);
                }
                Order::Hold => feed(4),
                Order::Patrol { a, b, forward } => {
                    feed(5);
                    feed(a.x.to_bits() as u64);
                    feed(a.y.to_bits() as u64);
                    feed(b.x.to_bits() as u64);
                    feed(b.y.to_bits() as u64);
                    feed(forward as u64);
                }
                Order::Harvest { mine, phase } => {
                    feed(6);
                    feed(mine.x.to_bits() as u64);
                    feed(mine.y.to_bits() as u64);
                    feed(phase as u64);
                }
                Order::HarvestLumber { tree, phase } => {
                    feed(8);
                    feed(tree.x.to_bits() as u64);
                    feed(tree.y.to_bits() as u64);
                    feed(phase as u64);
                }
                Order::Build(site) => {
                    feed(7);
                    feed(site.0 as u64);
                }
                Order::Repair(target) => {
                    feed(10);
                    feed(target.0 as u64);
                }
            }
            // An in-flight building upgrade is live state: a save/load pair that
            // dropped it would produce a different match from the same seed.
            if let Some(job) = unit.upgrade {
                feed(11);
                feed(job.to_def_index as u64);
                feed(job.remaining as u64);
            }
            for spec in &unit.order_queue {
                feed(match spec {
                    OrderSpec::Move(_) => 1,
                    OrderSpec::Attack(_) => 2,
                    OrderSpec::AttackMove(_) => 3,
                    OrderSpec::Patrol(_, _) => 4,
                    OrderSpec::Hold => 5,
                    OrderSpec::Stop => 6,
                    OrderSpec::Harvest(_) => 7,
                    OrderSpec::HarvestLumber(_) => 9,
                    OrderSpec::Build(_) => 8,
                    OrderSpec::Repair(_) => 10,
                });
            }
            for production in &unit.production {
                feed(production.def as u64);
                feed(production.remaining as u64);
            }
            if let Some(construction) = unit.construct {
                feed(construction.remaining as u64);
            }
            for cooldown in unit.spell_cd {
                feed(cooldown as u64);
            }
            feed(unit.rooted_until);
            feed(unit.invulnerable_until);
            feed(unit.ethereal as u64);
        }
        for cast in &self.pending_casts {
            feed(cast.due);
            // Pending waves are typed spells only (Blizzard); its stable
            // SpellDef index keeps the hash independent of enum layout.
            feed(cast.spell.index() as u64);
            feed(cast.at.x.to_bits() as u64);
            feed(cast.at.y.to_bits() as u64);
            feed(cast.waves_left as u64);
        }
        for state in &self.ai {
            feed(state.owner as u64);
            feed(state.waves as u64);
            feed(state.next_wave);
            feed(state.next_train);
            feed(state.next_build);
            feed(match state.mode {
                AiMode::Build => 0,
                AiMode::Attack => 1,
                AiMode::Retreat => 2,
            });
        }
        for buff in &self.buffs {
            feed(buff.id.0 as u64);
            feed(buff.unit.0 as u64);
            feed(buff.source.0 as u64);
            feed(buff.level as u64);
            feed(fnv_str(buff.tag));
            feed(buff.armor as u64);
            feed(buff.damage_permille as u64);
            feed(buff.attack_speed_permille as u64);
            feed(buff.move_speed_permille as u64);
            feed(buff.damage_taken_permille as u64);
            feed(buff.bonus_life as u64);
            feed(buff.root as u64);
            feed(buff.stun as u64);
            feed(buff.invulnerable as u64);
            match buff.kind {
                BuffKind::Timed { expire_tick } => {
                    feed(1);
                    feed(expire_tick);
                }
                BuffKind::TimedLife { expire_tick } => {
                    feed(2);
                    feed(expire_tick);
                }
                BuffKind::AuraSource {
                    radius,
                    hostile,
                    next_refresh,
                } => {
                    feed(3);
                    feed(radius.to_bits() as u64);
                    feed(hostile as u64);
                    feed(next_refresh);
                }
                BuffKind::AuraTarget {
                    source,
                    radius,
                    next_refresh,
                } => {
                    feed(4);
                    feed(source.0 as u64);
                    feed(radius.to_bits() as u64);
                    feed(next_refresh);
                }
            }
        }
        for manager in &self.squads {
            feed(manager.owner as u64);
            for squad in &manager.squads {
                feed(squad.id as u64);
                feed(match squad.kind {
                    SquadKind::Assault => 1,
                    SquadKind::Rush => 2,
                    SquadKind::Protection => 3,
                });
                feed(match squad.state {
                    SquadState::Idle => 1,
                    SquadState::AttackMove => 2,
                    SquadState::Attack => 3,
                    SquadState::Flee => 4,
                });
                feed(squad.target.map(|id| id.0 as u64).unwrap_or(0));
                for unit in &squad.units {
                    feed(unit.0 as u64);
                }
            }
        }
        hash
    }

    pub fn player(&self, raw: u8) -> Option<PlayerId> {
        PlayerId::new(raw).ok()
    }

    // ------------------------------------------------------------------
    // AI entry points
    // ------------------------------------------------------------------

    pub fn enable_ai(&mut self, owner: u8, config: AiConfig) {
        self.ai.retain(|state| state.owner != owner);
        let home = self
            .units_of(owner)
            .find(|unit| unit.is_building())
            .map(|unit| unit.pos)
            .unwrap_or(Vec2::new(512.0, 512.0));
        self.ai.push(AiState::new(owner, home, config));
    }

    pub fn disable_ai(&mut self, owner: u8) {
        self.ai.retain(|state| state.owner != owner);
    }

    /// Idle-worker helper used by the AI and by integration tests.
    pub fn idle_workers(&self, owner: u8) -> Vec<UnitId> {
        self.units
            .iter()
            .filter(|unit| {
                unit.alive
                    && unit.owner == owner
                    && !unit.is_building()
                    && unit.def().stats.is_worker
                    && matches!(unit.order, Order::Idle | Order::Hold)
            })
            .map(|unit| unit.id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn move_sim() -> Sim {
        let mut sim = Sim::new(7, 32, 32);
        sim.spawn(0, UnitKind::Footman, Vec2::new(64.0, 64.0));
        sim
    }

    #[test]
    fn unit_reaches_move_target() {
        let mut sim = move_sim();
        let id = sim.units[0].id;
        let target = Vec2::new(1000.0, 1000.0);
        sim.step(&[Command::Move {
            units: vec![id],
            to: target,
        }]);
        for _ in 0..900 {
            sim.step(&[]);
            if sim
                .unit(id)
                .map(|u| u.order == Order::Idle)
                .unwrap_or(false)
            {
                break;
            }
        }
        let unit = sim.unit(id).expect("unit");
        assert!(unit.pos.distance(target) < 200.0, "pos {:?}", unit.pos);
    }

    #[test]
    fn combat_kills_one_side() {
        let mut sim = Sim::new(1, 16, 16);
        let a = sim.spawn(0, UnitKind::Footman, Vec2::new(300.0, 300.0));
        let b = sim.spawn(1, UnitKind::Footman, Vec2::new(360.0, 300.0));
        for _ in 0..2200 {
            sim.step(&[]);
            if !sim.unit(a).unwrap().alive || !sim.unit(b).unwrap().alive {
                break;
            }
        }
        assert!(!sim.unit(a).unwrap().alive || !sim.unit(b).unwrap().alive);
    }

    #[test]
    fn production_trains_units() {
        let mut sim = Sim::new(2, 16, 16);
        let hall = sim.spawn(0, UnitKind::TownHall, Vec2::new(200.0, 200.0));
        let before = sim.alive_units().count();
        sim.step(&[Command::Train {
            building: hall,
            kind: UnitKind::Peasant,
        }]);
        for _ in 0..700 {
            sim.step(&[]);
        }
        assert!(sim.alive_units().count() > before);
    }

    #[test]
    fn simulation_is_deterministic() {
        let run = || {
            let mut sim = Sim::new(99, 24, 24);
            let a = sim.spawn(0, UnitKind::Footman, Vec2::new(100.0, 100.0));
            let b = sim.spawn(1, UnitKind::Archer, Vec2::new(1000.0, 1000.0));
            let mut hashes = Vec::new();
            sim.step(&[Command::Move {
                units: vec![a],
                to: Vec2::new(800.0, 800.0),
            }]);
            for tick in 0..300 {
                if tick == 150 {
                    sim.step(&[Command::Attack {
                        units: vec![a],
                        target: b,
                    }]);
                } else {
                    sim.step(&[]);
                }
                hashes.push(sim.state_hash());
            }
            hashes
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn ability_heals_target() {
        let mut sim = Sim::new(3, 8, 8);
        let a = sim.spawn(0, UnitKind::Peasant, Vec2::new(100.0, 100.0));
        let b = sim.spawn(0, UnitKind::Footman, Vec2::new(140.0, 100.0));
        sim.unit_mut(b).unwrap().hp -= 100;
        let before = sim.unit(b).unwrap().hp;
        sim.step(&[Command::Cast {
            unit: a,
            ability: Ability::Heal(60),
            target: Some(b),
        }]);
        assert!(sim.unit(b).unwrap().hp > before);
    }
}

#[cfg(test)]
mod gameplay_tests {
    use super::*;

    #[test]
    fn move_attack_engages_enemies_on_the_way() {
        let mut sim = Sim::new(11, 32, 32);
        let a = sim.spawn(0, UnitKind::Footman, Vec2::new(200.0, 200.0));
        let b = sim.spawn(1, UnitKind::Footman, Vec2::new(900.0, 200.0));
        sim.step(&[Command::MoveAttack {
            units: vec![a],
            to: Vec2::new(1800.0, 200.0),
        }]);
        for _ in 0..1800 {
            sim.step(&[]);
        }
        assert!(
            !sim.unit(b).unwrap().alive,
            "move-attack should kill the blocker"
        );
        assert!(sim.unit(a).unwrap().alive);
    }

    #[test]
    fn hold_position_does_not_chase() {
        let mut sim = Sim::new(12, 32, 32);
        let a = sim.spawn(0, UnitKind::Archer, Vec2::new(200.0, 200.0));
        let b = sim.spawn(1, UnitKind::Footman, Vec2::new(1400.0, 200.0));
        sim.step(&[Command::MoveAttack {
            units: vec![b],
            to: Vec2::new(200.0, 200.0),
        }]);
        for _ in 0..60 {
            sim.step(&[Command::Hold { units: vec![a] }]);
        }
        assert_eq!(sim.unit(a).unwrap().order, Order::Hold);
        let start = sim.unit(b).unwrap().pos;
        for _ in 0..60 {
            sim.step(&[]);
        }
        assert!(
            sim.unit(b).unwrap().pos.distance(start) > 100.0,
            "enemy closes in"
        );
        assert_eq!(
            sim.unit(a).unwrap().order,
            Order::Hold,
            "archer stays on hold"
        );
    }

    #[test]
    fn peasants_harvest_gold() {
        let mut sim = Sim::new(13, 32, 32);
        sim.passive_income = false;
        let mine = Vec2::new(500.0, 500.0);
        let p = sim.spawn(0, UnitKind::Peasant, Vec2::new(320.0, 320.0));
        sim.step(&[Command::Harvest {
            units: vec![p],
            to: mine,
        }]);
        let before = sim.resources[0];
        for _ in 0..900 {
            sim.step(&[]);
        }
        assert!(
            sim.resources[0] >= before + 20,
            "harvest paid out: {} -> {}",
            before,
            sim.resources[0]
        );
        assert!(sim.unit(p).unwrap().alive);
    }

    #[test]
    fn gameplay_config_loads_from_install_and_drives_hero_leveling() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Ok(config) = GameplayConfig::load(&root) else {
            eprintln!("skip: MiscGame.txt unavailable");
            return;
        };
        assert_eq!(config.max_hero_level, 10);
        assert_eq!(config.need_hero_xp.first().copied(), Some(200));
        assert_eq!(config.str_hit_point_bonus, 25);
        assert_eq!(config.int_mana_bonus, 15);

        let mut sim = Sim::new(21, 32, 32);
        sim.apply_gameplay(config.clone());
        assert_eq!(sim.need_hero_xp(1), 200);
        let hero = sim
            .spawn_def(0, "Hpal", Vec2::new(100.0, 100.0))
            .expect("Paladin def");
        assert_eq!(sim.add_hero_xp(hero, 200).expect("xp"), 2);

        // The loaded config is the runtime source: a custom table changes the
        // threshold the same `add_hero_xp` path consumes.
        let mut custom = config;
        custom.need_hero_xp = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        let mut sim = Sim::new(22, 32, 32);
        sim.apply_gameplay(custom);
        let hero = sim
            .spawn_def(0, "Hpal", Vec2::new(100.0, 100.0))
            .expect("Paladin def");
        assert_eq!(sim.add_hero_xp(hero, 10).expect("xp"), 2);
    }

    #[test]
    fn cancelling_production_refunds() {
        let mut sim = Sim::new(14, 32, 32);
        let hall = sim.spawn(0, UnitKind::TownHall, Vec2::new(300.0, 300.0));
        sim.step(&[]);
        sim.resources[0] = 1000;
        sim.step(&[Command::Train {
            building: hall,
            kind: UnitKind::Footman,
        }]);
        assert_eq!(sim.resources[0], 1000 - UnitKind::Footman.stats().cost);
        sim.step(&[Command::CancelTrain {
            building: hall,
            index: 0,
        }]);
        assert_eq!(sim.resources[0], 1000, "refund restores the price");
        assert!(sim.unit(hall).unwrap().production.is_empty());
    }
}
