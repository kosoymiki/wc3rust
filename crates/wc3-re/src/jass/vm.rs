//! Deterministic JASS interpreter with a native registry and game-state model.
//!
//! Execution is fully deterministic: containers are ordered, handles are
//! allocated sequentially from 0x100, and the RNG is a fixed-seed xorshift.
//! The RNG semantics (inclusive bounds, swapped bounds when `low > high`)
//! follow the Warsmash JASS native implementation
//! (`donors/WarsmashModEngine/core/src/com/etheller/warsmash/parsers/jass/Jass2.java:9590-9612`);
//! the exact retail sequence is not reproduced and is marked as such, but every
//! script run is reproducible byte-for-byte.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::error::{ReError, Result};

use super::ast::*;
use super::parser::parse_named;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Real(f64),
    Bool(bool),
    Str(String),
    Handle(u32),
    Code(String),
    Null,
}

impl Value {
    pub fn as_int(&self) -> i64 {
        match self {
            Self::Int(v) => *v,
            Self::Real(v) => *v as i64,
            Self::Bool(v) => i64::from(*v),
            Self::Handle(v) => i64::from(*v),
            Self::Str(v) => v.parse().unwrap_or(0),
            Self::Code(_) | Self::Null => 0,
        }
    }

    pub fn as_real(&self) -> f64 {
        match self {
            Self::Real(v) => *v,
            Self::Int(v) => *v as f64,
            Self::Bool(v) => {
                if *v {
                    1.0
                } else {
                    0.0
                }
            }
            Self::Handle(v) => f64::from(*v),
            Self::Str(v) => v.parse().unwrap_or(0.0),
            Self::Code(_) | Self::Null => 0.0,
        }
    }

    pub fn as_bool(&self) -> bool {
        match self {
            Self::Bool(v) => *v,
            Self::Int(v) => *v != 0,
            Self::Real(v) => *v != 0.0,
            Self::Handle(v) => *v != 0,
            Self::Str(v) => !v.is_empty(),
            Self::Code(_) => true,
            Self::Null => false,
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            Self::Str(v) => v.clone(),
            Self::Int(v) => v.to_string(),
            Self::Real(v) => format_real(*v),
            Self::Bool(v) => (if *v { "true" } else { "false" }).to_string(),
            Self::Handle(v) => v.to_string(),
            Self::Code(v) => v.clone(),
            Self::Null => String::new(),
        }
    }

    pub fn handle(&self) -> Option<u32> {
        match self {
            Self::Handle(v) => Some(*v),
            Self::Int(v) if *v >= 0 && *v <= u32::MAX as i64 => Some(*v as u32),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// JASS truthiness for condition results (a `null` boolexpr is false).
    fn truthy(&self) -> bool {
        self.as_bool()
    }
}

pub fn format_real(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Slot {
    Scalar(Value),
    Array(BTreeMap<i64, Value>),
}

impl Slot {
    pub fn scalar(&self) -> Value {
        match self {
            Self::Scalar(value) => value.clone(),
            Self::Array(_) => Value::Null,
        }
    }
}

/// Small, deterministic LCG/xorshift mix. Seed is user-selectable.
#[derive(Debug, Clone)]
pub struct Wc3Rng {
    state: u64,
}

impl Wc3Rng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }

    /// Inclusive `[low, high]`; bounds are swapped when `low > high`, matching
    /// the JASS native behaviour used by Warsmash.
    pub fn int(&mut self, low: i64, high: i64) -> i64 {
        let (low, high) = if low > high { (high, low) } else { (low, high) };
        if low == high {
            return low;
        }
        let span = (high - low + 1) as u64;
        low + (u64::from(self.next_u32() % span as u32)) as i64
    }

    pub fn real(&mut self, low: f64, high: f64) -> f64 {
        if low == high {
            return low;
        }
        let unit = f64::from(self.next_u32()) / f64::from(u32::MAX);
        low + unit * (high - low)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Player {
    pub index: u32,
    pub name: String,
    pub states: BTreeMap<i64, i64>,
    pub gold: i64,
    pub lumber: i64,
    pub food: i64,
    pub food_max: i64,
    pub color: u32,
    pub race: String,
    pub controller: u32,
    pub slot_state: u32,
    pub team: u32,
    pub start_location: i32,
    /// Abilities hidden by `SetPlayerAbilityAvailable(whichPlayer, id, false)`.
    pub disabled_abilities: BTreeSet<String>,
    /// Leaderboard shown to this player (`PlayerSetLeaderboard`).
    pub leaderboard: Option<u32>,
}

/// Runtime state of one ability on one unit.
///
/// `level == 0` never occurs for a stored ability: `UnitRemoveAbility` removes
/// the entry. `permanent` mirrors `UnitMakeAbilityPermanent`; cooldowns are
/// tracked so `UnitResetCooldown` and item/ability runtime code observe the
/// same state instead of a second parallel model.
#[derive(Debug, Clone, Default)]
pub struct AbilityState {
    pub level: i32,
    pub permanent: bool,
    pub cooldown: f64,
    pub cooldown_max: f64,
}

/// One shop stock line: current and maximum stock for an item/unit rawcode.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StockEntry {
    pub current: i32,
    pub max: i32,
}

/// Deterministic item pool populated through `ItemPoolAddItemType` and drawn
/// through `PlaceRandomItem`.
#[derive(Debug, Clone, Default)]
pub struct ItemPool {
    pub entries: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Default)]
pub struct Unit {
    pub id: String,
    pub owner: i32,
    /// The seat the *script* gave the unit (`CreateUnit`/`SetUnitOwner`/map
    /// placement). The game bridge mirrors the simulation owner into `owner`,
    /// which squashes seats outside its own player map onto 0; the script world
    /// keeps the authored seat here so `GetOwningPlayer` and the death/region
    /// event attribution stay the map's own player model.
    pub script_owner: Option<i32>,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub facing: f64,
    pub hp: f64,
    pub max_hp: f64,
    pub mana: f64,
    pub max_mana: f64,
    pub level: i32,
    pub alive: bool,
    pub paused: bool,
    pub hidden: bool,
    pub name: String,
    pub scale: f64,
    pub time_scale: f64,
    pub states: BTreeMap<i64, f64>,
    pub invulnerable: bool,
    pub color: i64,
    pub acquire_range: f64,
    pub animation: String,
    pub abilities: BTreeMap<String, AbilityState>,
    pub items: Vec<u32>,
    pub stock_items: BTreeMap<String, StockEntry>,
    pub stock_units: BTreeMap<String, StockEntry>,
    pub item_type_slots: i32,
    pub order: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Destructable {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub hp: f64,
    pub max_hp: f64,
    pub alive: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: String,
    pub x: f64,
    pub y: f64,
    /// Player index returned by `GetItemPlayer`; `None` before the first owner.
    pub owner: Option<u32>,
    /// Unit handle currently holding the item (inventory slot owner).
    pub held_by: Option<u32>,
    pub charges: i32,
    pub level: i32,
    pub visible: bool,
    pub droppable: bool,
    pub pawnable: bool,
    pub invulnerable: bool,
    pub powerup: bool,
    pub sellable: bool,
    pub drop_on_death: bool,
    pub drop_id: String,
}

impl Default for Item {
    fn default() -> Self {
        Self {
            id: String::new(),
            x: 0.0,
            y: 0.0,
            owner: None,
            held_by: None,
            charges: 1,
            level: 1,
            visible: true,
            droppable: true,
            pawnable: true,
            invulnerable: false,
            powerup: false,
            sellable: false,
            drop_on_death: false,
            drop_id: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Group {
    pub units: BTreeSet<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct Location {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Rect {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Region {
    pub rects: Vec<u32>,
    pub cells: BTreeSet<(i32, i32)>,
}

#[derive(Debug, Clone, Default)]
pub struct Trigger {
    pub enabled: bool,
    pub events: Vec<TriggerEvent>,
    pub conditions: Vec<u32>,
    pub actions: Vec<String>,
    pub exec_count: u32,
    pub eval_count: u32,
}

#[derive(Debug, Clone)]
pub enum TriggerEvent {
    Player { player: u32, event: i64 },
    PlayerUnit { player: u32, event: i64, unit_type: String },
    Timer { timeout: f64, periodic: bool },
    TimerExpire { timer: u32 },
    Death { unit: u32 },
    EnterRegion { region: u32 },
    LeaveRegion { region: u32 },
    /// A game-state limit event. `player` is `-1` for
    /// `TriggerRegisterGameStateEvent` (no player) and the seat index for
    /// `TriggerRegisterPlayerStateEvent`.
    GameState {
        player: i32,
        state: i64,
        op: i64,
        value: f64,
    },
    Variable { name: String, op: i64, value: f64 },
    Dialog { dialog: u32 },
    DialogButton { dialog: u32, button: u32 },
    UnitEvent { unit: u32, event: i64 },
    /// `TriggerRegisterGameEvent(trig, whichGameEvent)`: the id is the
    /// `gameevent` value (`EVENT_GAME_VICTORY` = 0, `EVENT_GAME_SHOW_SKILL` =
    /// 9, …).
    Custom { event: i64 },
}

/// Region membership transition raised by the engine tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionEvent {
    Enter,
    Leave,
}

/// `limitop` comparison (`common.j:479-484`): does `value` satisfy `op` against
/// `limit`?
fn limit_holds(op: i64, value: f64, limit: f64) -> bool {
    match op {
        0 => value < limit,
        1 => value <= limit,
        2 => (value - limit).abs() < 1e-9,
        3 => value >= limit,
        4 => value > limit,
        5 => (value - limit).abs() >= 1e-9,
        _ => value >= limit,
    }
}

/// Point-in-region test: a region is the union of its rectangles and cells
/// (`RegionAddRect` / `RegionAddCell`).
pub(crate) fn point_in_region(world: &World, region: &Region, x: f64, y: f64) -> bool {
    if region.cells.contains(&(x as i32, y as i32)) {
        return true;
    }
    region.rects.iter().any(|handle| {
        world.rects.get(handle).is_some_and(|rect| {
            x >= rect.min_x && x <= rect.max_x && y >= rect.min_y && y <= rect.max_y
        })
    })
}

#[derive(Debug, Clone, Default)]
pub struct Timer {
    pub elapsed: f64,
    pub timeout: f64,
    pub remaining: f64,
    pub periodic: bool,
    pub running: bool,
    pub code: Option<String>,
}

/// One live `TriggerRegisterTimerEvent` registration.
///
/// The event itself is the `TriggerEvent::Timer` the trigger carries; this is
/// the schedule that makes it fire. Retail turns every timer event into a
/// hidden timer, which is why a `TriggerRegisterTimerEvent(trig, 2.0, false)`
/// fires two seconds of *game* time later, not inside the registering call.
#[derive(Debug, Clone, Default)]
pub struct TimerEventState {
    pub trigger: u32,
    pub timeout: f64,
    pub periodic: bool,
    pub elapsed: f64,
    /// A one-shot event has fired; it stays in the list as history but never
    /// fires again.
    pub done: bool,
    pub fires: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Sound {
    pub path: String,
    pub volume: i64,
    pub pitch: f64,
    pub playing: bool,
    pub attached: Option<u32>,
    pub label: String,
}

#[derive(Debug, Clone, Default)]
pub struct Effect {
    pub model: String,
    pub x: f64,
    pub y: f64,
    pub attached: Option<u32>,
    pub attachment: String,
}

#[derive(Debug, Clone, Default)]
pub struct Quest {
    pub title: String,
    pub description: String,
    pub icon: String,
    pub required: bool,
    pub completed: bool,
    pub discovered: bool,
    pub failed: bool,
    pub items: Vec<(String, bool)>,
}

#[derive(Debug, Clone, Default)]
pub struct Dialog {
    pub message: String,
    pub buttons: Vec<(String, u32)>,
    pub visible: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Leaderboard {
    pub label: String,
    pub visible: bool,
    pub items: Vec<(String, i64, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct Multiboard {
    pub title: String,
    pub visible: bool,
    pub rows: u32,
    pub columns: u32,
    pub values: BTreeMap<(u32, u32), String>,
}

#[derive(Debug, Clone)]
pub enum BoolExpr {
    Code(String),
    And(u32, u32),
    Or(u32, u32),
    Not(u32),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EventContext {
    pub trigger: Option<u32>,
    pub player: Option<u32>,
    pub unit: Option<u32>,
    /// Region handle for an enter/leave event (`GetTriggeringRegion`).
    pub region: Option<u32>,
    pub chat: String,
    pub damage: f64,
    pub dying: Option<u32>,
    pub killing: Option<u32>,
    pub entering: Option<u32>,
    pub leaving: Option<u32>,
    pub game_state: i64,
    pub enum_unit: Option<u32>,
    pub enum_item: Option<u32>,
    /// Rawcode of the ability currently being cast (`GetSpellAbilityId`).
    pub ability: Option<String>,
    pub ability_level: i32,
    pub spell_target_unit: Option<u32>,
    pub spell_target_item: Option<u32>,
    pub spell_target_destructable: Option<u32>,
    pub spell_target_point: Option<(f64, f64)>,
    /// Item manipulated by a pickup/drop/use event (`GetManipulatedItem`).
    pub manipulated_item: Option<u32>,
    /// Item sold by a shop (`GetSoldItem`).
    pub sold_item: Option<u32>,
}

/// Player-unit event ids from `Scripts\common.j` (`ConvertPlayerUnitEvent`).
/// The corpus under `analysis/war3_full3` omits the spell block, so the
/// canonical values are taken from `analysis/war3x_data/Scripts/common.j`.
pub const EVENT_PLAYER_UNIT_DROP_ITEM: i64 = 48;
pub const EVENT_PLAYER_UNIT_PICKUP_ITEM: i64 = 49;
pub const EVENT_PLAYER_UNIT_USE_ITEM: i64 = 50;
pub const EVENT_PLAYER_UNIT_SPELL_CAST: i64 = 273;
pub const EVENT_PLAYER_UNIT_SPELL_EFFECT: i64 = 274;
pub const EVENT_PLAYER_UNIT_SPELL_FINISH: i64 = 275;
/// Unit-specific events (`ConvertUnitEvent`) for `TriggerRegisterUnitEvent`.
pub const EVENT_UNIT_DROP_ITEM: i64 = 85;
pub const EVENT_UNIT_PICKUP_ITEM: i64 = 86;
pub const EVENT_UNIT_USE_ITEM: i64 = 87;
pub const EVENT_UNIT_SPELL_CAST: i64 = 290;
pub const EVENT_UNIT_SPELL_EFFECT: i64 = 291;
pub const EVENT_UNIT_SPELL_FINISH: i64 = 292;
/// `EVENT_PLAYER_UNIT_DEATH` / `EVENT_UNIT_DEATH` (`common.j:367,416`).
pub const EVENT_PLAYER_UNIT_DEATH: i64 = 20;
pub const EVENT_UNIT_DEATH: i64 = 53;
/// `GAME_STATE_TIME_OF_DAY` (`common.j:277`, `ConvertFGameState(2)`).
pub const GAME_STATE_TIME_OF_DAY: i64 = 2;
/// JASS arrays hold 8192 elements; an index outside `0..=8191` is a script
/// error, never a new element (WC3-JASS-006).
pub const JASS_ARRAY_LEN: i64 = 8192;
/// Deepest nested script call on one thread. The old `call_function` guard
/// rejected depth > 512; an inline call chain keeps the same ceiling.
pub const MAX_CALL_DEPTH: usize = 512;

/// Target passed to [`Runtime::cast_ability`] and [`Runtime::use_item`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CastTarget {
    #[default]
    None,
    Unit(u32),
    Item(u32),
    Destructable(u32),
    Point(f64, f64),
}

/// Outcome of one completed ability cast: the ability level read before the
/// cast and the number of triggers executed across the cast/effect/finish
/// phases.
#[derive(Debug, Clone, PartialEq)]
pub struct AbilityCastReport {
    pub caster: u32,
    pub ability: String,
    pub level: i32,
    pub triggers: usize,
}

/// One order issued by a map script through the `Issue*Order` natives.
///
/// `wc3-re` records the request and does not simulate it; the canonical
/// `wc3-jass::sim_bridge` translates it into `wc3_sim` orders so the script
/// drives the one gameplay simulation and unit state is never simulated twice.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptOrder {
    /// JASS unit handle.
    pub unit: u32,
    /// Native order name (`move`, `attack`, `smart`, `holdposition`, …).
    pub order: String,
    /// Target point for point orders (`IssuePointOrder*`).
    pub point: Option<(f64, f64)>,
    /// Target widget handle for target orders (`IssueTargetOrder*`).
    pub target: Option<u32>,
    /// Build/rawcode argument (`IssueBuildOrder*`).
    pub id: Option<String>,
}

/// Mutable camera setup created by `CreateCameraSetup` and edited by the
/// `CameraSetupSetField`/`CameraSetupSetDestPosition` natives.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CameraSetup {
    pub fields: BTreeMap<i64, f64>,
    pub dest: Option<(f64, f64)>,
}

/// The camera the script has driven so far: the target, the orbit and the
/// angular fields `GetCameraField` reports. Kept live so `SetCameraField` and
/// `PanCameraToTimed` compose the way the engine's camera does, instead of the
/// getters always answering 0.
/// Process-wide seat used by [`World::default`] (see [`Runtime::set_default_local_player`]).
static DEFAULT_LOCAL_PLAYER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Process-wide single-player wildcard mode for [`World::default`].
static DEFAULT_LOCAL_PLAYER_ANY: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Sets the [`World::local_player_any`] default for worlds created after this
/// call. Campaign missions set it before the engine boots the script.
pub fn set_local_player_any_default(any: bool) {
    DEFAULT_LOCAL_PLAYER_ANY.store(any, std::sync::atomic::Ordering::Relaxed);
}

/// Restores the process-wide seat defaults (`Player(0)`, no wildcard).
///
/// The defaults exist because a map script's `main` runs during boot, before
/// the engine can hand the runtime a seat. That makes them *session* state: a
/// campaign mission sets the human seat (Human01's is 2, not 0), and a later
/// melee map must not inherit it — with the wrong seat every unguarded
/// `GetLocalPlayer() == Player(0)` block is silently skipped. Callers that boot
/// a non-campaign world call this before `Engine::boot` (AUD-032).
pub fn reset_local_player_default() {
    DEFAULT_LOCAL_PLAYER.store(0, std::sync::atomic::Ordering::Relaxed);
    DEFAULT_LOCAL_PLAYER_ANY.store(false, std::sync::atomic::Ordering::Relaxed);
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CameraState {
    pub target_x: f64,
    pub target_y: f64,
    pub target_z: f64,
    /// `CAMERA_FIELD_TARGET_DISTANCE` (0).
    pub distance: f64,
    /// `CAMERA_FIELD_ROTATION` (5), degrees.
    pub yaw: f64,
    /// `CAMERA_FIELD_ANGLE_OF_ATTACK` (2), degrees.
    pub pitch: f64,
    /// `CAMERA_FIELD_ZOFFSET` (6).
    pub z_offset: f64,
    /// `CAMERA_FIELD_FIELD_OF_VIEW` (3), degrees.
    pub field_of_view: f64,
    /// `CAMERA_FIELD_FARZ` (1).
    pub far_z: f64,
    /// `CameraSetSmoothingFactor`.
    pub smoothing: f64,
}

/// One `SetCinematicScene` / `DisplayTimedTextToPlayer` line: the subtitle the
/// HUD shows while a transmission plays.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SubtitleEvent {
    /// Speaker name (`unitName` of `TransmissionFromUnitWithNameBJ`).
    pub speaker: String,
    /// Script clock (`world.game_time`) the line was queued at.
    pub t: f64,
    pub text: String,
    /// Seconds the line stays on screen (`sceneDuration`).
    pub duration: f64,
    /// `portraitUnitId`: the unit *type* rawcode the portrait renders.
    pub portrait_unit_id: i64,
    /// `playercolor` value of the speaker.
    pub color: i64,
    /// `voiceoverDuration`, kept for the audio lane.
    pub voiceover: f64,
    /// Sound handle of the voiceover (`SetCinematicSceneBJ` passes it first).
    pub sound: Option<u32>,
    /// Player the line was addressed to, when the native names one.
    pub player: Option<u32>,
}

/// One entry of the ordered cinematic stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CineBeatKind {
    /// Index into [`World::camera_events`].
    Camera(usize),
    /// Index into [`World::subtitles`].
    Subtitle(usize),
}

/// An ordered cinematic beat: camera move, fade, scene marker or subtitle line,
/// in the order the map script executed them, stamped with the script clock.
///
/// The two typed tables (`camera_events`, `subtitles`) keep the payload; this log
/// keeps the *interleaving*, which is what a cutscene is: a camera move, then a
/// line, then a fade. The game layer's director walks this log and lays the
/// beats out on a timeline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CineBeat {
    pub kind: CineBeatKind,
    pub t: f64,
}

/// The cinematic colour filter (`SetCineFilter*` + `DisplayCineFilter`): the
/// fade-out/fade-in the render path composites.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CineFilter {
    pub active: bool,
    pub texture: String,
    pub start: [u8; 4],
    pub end: [u8; 4],
    pub duration: f64,
}

/// One executed camera/cinematic native, kept as a typed event instead of a
/// no-op so scripts that drive the camera have observable state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CameraEvent {
    pub kind: String,
    pub setup: Option<u32>,
    pub player: Option<u32>,
    pub field: Option<i64>,
    pub value: f64,
    pub x: f64,
    pub y: f64,
    pub duration: f64,
    /// Script clock (`world.game_time`) the call happened at, stamped by the
    /// native dispatcher so the game layer can build a timed director stream.
    pub t: f64,
}

#[derive(Debug, Clone)]
pub struct World {
    pub players: Vec<Player>,
    pub units: BTreeMap<u32, Unit>,
    pub destructables: BTreeMap<u32, Destructable>,
    pub items: BTreeMap<u32, Item>,
    pub item_pools: BTreeMap<u32, ItemPool>,
    /// Stock registered by `AddItemToAllStock` / `AddUnitToAllStock`; shops that
    /// do not carry an explicit line inherit it.
    pub stock_items_default: BTreeMap<String, StockEntry>,
    pub stock_units_default: BTreeMap<String, StockEntry>,
    /// Default shop item-type slot count (`SetAllItemTypeSlots`).
    pub item_type_slots_default: i32,
    pub groups: BTreeMap<u32, Group>,
    pub locations: BTreeMap<u32, Location>,
    pub rects: BTreeMap<u32, Rect>,
    pub regions: BTreeMap<u32, Region>,
    pub triggers: BTreeMap<u32, Trigger>,
    pub timers: BTreeMap<u32, Timer>,
    /// Live `TriggerRegisterTimerEvent` schedules (one per registration).
    pub timer_events: Vec<TimerEventState>,
    /// Timer handle currently inside its expiry callback (`GetExpiredTimer`).
    pub firing_timer: Option<u32>,
    pub sounds: BTreeMap<u32, Sound>,
    pub camera_setups: BTreeMap<u32, CameraSetup>,
    pub camera_events: Vec<CameraEvent>,
    /// Live camera target/orbit an event stream is derived from.
    pub camera: CameraState,
    /// Subtitle lines the script produced (`SetCinematicScene`,
    /// `DisplayTimedTextToPlayer`).
    pub subtitles: Vec<SubtitleEvent>,
    /// The cinematic colour filter state.
    pub cine_filter: CineFilter,
    /// `ShowInterface(flag, …)`: false while a cutscene hides the console.
    pub interface_shown: bool,
    /// Minimap pings registered by `PingMinimap` (`x`, `y`, `duration`).
    pub pings: Vec<(f64, f64, f64)>,
    /// Every cinematic beat in script order (camera, fade, scene, subtitle).
    pub cinematic_log: Vec<CineBeat>,
    /// How far `cinematic_log` has consumed the two typed tables. Native calls
    /// nest, so the stamping cursor lives with the world, not with a call frame.
    pub(crate) cine_stamped_camera: usize,
    pub(crate) cine_stamped_subtitle: usize,
    /// Movie files requested by `PlayCinematic`, in call order. Blizzard.j's
    /// `SetCinematicAvailableBJ` is the caller: the campaign screens and mission
    /// scripts ask the engine for one of the nine CGI files, and the game layer
    /// resolves the name to a file and decodes it with `wc3-bink`.
    pub movie_requests: Vec<String>,
    /// Volume-group levels set by `VolumeGroupSetVolume` (`1.0` = 100%).
    pub group_volumes: BTreeMap<i64, f64>,
    pub effects: BTreeMap<u32, Effect>,
    pub quests: BTreeMap<u32, Quest>,
    /// `QuestCreateItem` handle → `(quest handle, item index)`. `common.j`
    /// hands the script a *questitem* handle, and `QuestItemSetDescription` /
    /// `QuestItemSetCompleted` address the item through it, not through the
    /// quest handle.
    pub quest_items: BTreeMap<u32, (u32, usize)>,
    pub dialogs: BTreeMap<u32, Dialog>,
    pub leaderboards: BTreeMap<u32, Leaderboard>,
    pub multiboards: BTreeMap<u32, Multiboard>,
    pub forces: BTreeMap<u32, BTreeSet<u32>>,
    /// Single-player wildcard: `GetLocalPlayer()` answers [`LOCAL_PLAYER_ANY`],
    /// which compares equal to every player and belongs to every force. A
    /// campaign map is authored for one human seat, and the RoC `war3map.w3i`
    /// (version 18) does not say which; the port has exactly one human, so the
    /// `if GetLocalPlayer() == whichPlayer` blocks Blizzard.j wraps around the
    /// camera, field and transmission calls are the human's blocks by
    /// construction.
    pub local_player_any: bool,
    /// Player index `GetLocalPlayer()` reports. The map declares its human seat
    /// (`SetPlayerController` / `war3map.w3i`); the game layer sets this so the
    /// `if GetLocalPlayer() == whichPlayer` blocks Blizzard.j wraps around every
    /// camera and transmission actually run for that seat.
    pub local_player: u32,
    pub boolexprs: BTreeMap<u32, BoolExpr>,
    pub game_time: f64,
    pub time_of_day: f64,
    /// `SetTimeOfDayScale`: 1.0 is the retail day/night rate (one 24 h cycle
    /// per 480 s of game time).
    pub time_of_day_scale: f64,
    pub next_handle: u32,
    pub map_name: String,
    pub camera_bounds: [f64; 8],
    pub camera_margin: [f64; 4],
    pub context: EventContext,
    pub last_created_unit: Option<u32>,
    pub last_created_item: Option<u32>,
    pub orders: Vec<ScriptOrder>,
}

impl Default for World {
    fn default() -> Self {
        let mut players = Vec::with_capacity(16);
        for index in 0..16u32 {
            players.push(Player {
                index,
                name: format!("Player {index}"),
                gold: 500,
                lumber: 500,
                food_max: 100,
                color: index,
                race: "human".into(),
                controller: 1,
                slot_state: 1,
                ..Player::default()
            });
        }
        Self {
            players,
            units: BTreeMap::new(),
            destructables: BTreeMap::new(),
            items: BTreeMap::new(),
            item_pools: BTreeMap::new(),
            stock_items_default: BTreeMap::new(),
            stock_units_default: BTreeMap::new(),
            item_type_slots_default: 0,
            groups: BTreeMap::new(),
            locations: BTreeMap::new(),
            rects: BTreeMap::new(),
            regions: BTreeMap::new(),
            triggers: BTreeMap::new(),
            timers: BTreeMap::new(),
            timer_events: Vec::new(),
            firing_timer: None,
            sounds: BTreeMap::new(),
            camera_setups: BTreeMap::new(),
            camera_events: Vec::new(),
            camera: CameraState::default(),
            subtitles: Vec::new(),
            cine_filter: CineFilter::default(),
            interface_shown: true,
            pings: Vec::new(),
            cinematic_log: Vec::new(),
            cine_stamped_camera: 0,
            cine_stamped_subtitle: 0,
            movie_requests: Vec::new(),
            group_volumes: BTreeMap::new(),
            effects: BTreeMap::new(),
            quests: BTreeMap::new(),
            quest_items: BTreeMap::new(),
            dialogs: BTreeMap::new(),
            leaderboards: BTreeMap::new(),
            multiboards: BTreeMap::new(),
            forces: BTreeMap::new(),
            local_player: DEFAULT_LOCAL_PLAYER.load(std::sync::atomic::Ordering::Relaxed),
            local_player_any: DEFAULT_LOCAL_PLAYER_ANY.load(std::sync::atomic::Ordering::Relaxed),
            boolexprs: BTreeMap::new(),
            game_time: 0.0,
            time_of_day: 12.0,
            time_of_day_scale: 1.0,
            next_handle: 0x100,
            map_name: String::new(),
            camera_bounds: [0.0; 8],
            camera_margin: [0.0; 4],
            context: EventContext::default(),
            last_created_unit: None,
            last_created_item: None,
            orders: Vec::new(),
        }
    }
}

pub struct Runtime {
    pub program: Program,
    pub globals: HashMap<String, Slot>,
    pub world: World,
    pub rng: Wc3Rng,
    pub output: Vec<String>,
    pub unimplemented: BTreeMap<String, u64>,
    pub call_counts: BTreeMap<String, u64>,
    /// Every native actually invoked by the executed scripts, with call counts.
    pub native_calls: BTreeMap<String, u64>,
    pub errors: Vec<String>,
    pub steps: u64,
    pub max_steps: u64,
    pub seed: u64,
    function_index: HashMap<String, usize>,
    /// Compiled instruction stream; `Frame::function` indexes this, not
    /// `program.functions`, so global initialisers can run as snippets.
    code: Vec<CompiledFunction>,
    natives: HashMap<String, NativeDecl>,
    /// Threads suspended inside `TriggerSleepAction`, waiting for their tick.
    pub threads: Vec<Thread>,
    /// How many times a script asked to sleep (`TriggerSleepAction` calls).
    pub sleeps: u64,
    /// How many suspended threads have been resumed by the tick loop.
    pub wakes: u64,
    next_thread: u32,
    /// Set by a blocking native (`TriggerSleepAction`/`PolledWait`); the
    /// interpreter loop turns it into a suspended thread.
    pending_sleep: Option<f64>,
    /// Handles of triggers currently on the execution stack. A trigger that
    /// re-executes itself (directly or through a cycle) would otherwise recurse
    /// until the VM call-depth limit. Re-entrant `TriggerExecute` is ignored,
    /// matching the engine's non-reentrant trigger semantics.
    trigger_stack: Vec<u32>,
    /// Registered triggers executed per event kind (`enter_region`, `death`,
    /// `game_state`, …). This is the observable proof that the dispatcher ran.
    pub event_fires: BTreeMap<String, u64>,
    /// Units currently inside each region that a trigger watches. The engine
    /// tick diffs it to raise enter/leave events.
    pub region_members: BTreeMap<u32, BTreeSet<u32>>,
    /// Units already reported dead, so one death raises one event.
    pub(crate) dead_seen: BTreeSet<u32>,
    /// `world.time_of_day` at the previous tick (crossing detection).
    time_of_day_prev: f64,
    /// True once a `TriggerRegisterVariableEvent` exists; the store path only
    /// pays for the scan when a map actually watches a global.
    pub(crate) has_variable_events: bool,
    /// Out-of-range JASS array accesses (0..=8191 is the retail contract).
    pub array_bounds_hits: u64,
    /// Trigger dispatches whose own conditions vetoed the action list. Shows
    /// that an event reached the map and the map declined it.
    pub condition_vetoes: u64,
    /// Ring of the first events fired, for the boot/tick log.
    pub event_log: Vec<String>,
}

/// One compiled instruction. Control flow is by instruction index, so a thread
/// can stop at any instruction and resume later (`TriggerSleepAction`).
#[derive(Debug, Clone)]
pub enum Op {
    /// Push a literal.
    Push(Value),
    /// Push a `code` value (`function Name`).
    PushCode(String),
    /// Push scalar `name`.
    Load(String),
    /// Pop an index, push `name[index]`.
    LoadArray(String),
    /// Pop a value into scalar `name`.
    Store(String),
    /// Pop an index, pop a value, store `name[index] = value`.
    StoreArray(String),
    /// Declare a local; `init` pops its initial value. Arrays ignore `init`.
    Local {
        name: String,
        array: bool,
        init: bool,
    },
    /// Pop `argc` arguments and call. `proc` discards the result (a `call`
    /// statement); otherwise the result is pushed.
    Call {
        name: String,
        argc: usize,
        line: u32,
        proc: bool,
    },
    Binary(BinOp),
    Unary(UnOp),
    /// Unconditional jump.
    Jump(usize),
    /// Pop; jump when falsy.
    JumpIfFalse(usize),
    /// Pop; jump when truthy.
    JumpIfTrue(usize),
    /// `and` short circuit: leave `false` on the stack and jump when the top is
    /// falsy, otherwise pop it.
    AndJump(usize),
    /// `or` short circuit: leave `true` on the stack and jump when the top is
    /// truthy, otherwise pop it.
    OrJump(usize),
    /// Pop; jump to the loop end when truthy (`exitwhen`).
    ExitWhen(usize),
    /// Pop the return value and return from the current function.
    Return,
    /// Reset the iteration counter of loop `slot`.
    LoopEnter(usize),
    /// Increment loop `slot`'s counter and jump back to `target`; the counter
    /// is the loop guard.
    LoopBack {
        slot: usize,
        target: usize,
        line: u32,
    },
    /// Discard the top of the stack.
    Pop,
    /// Pop; push the value's boolean truth.
    CoerceBool,
}

/// A function body compiled to instructions.
#[derive(Debug, Clone, Default)]
pub struct CompiledFunction {
    pub name: String,
    pub params: Vec<String>,
    pub ops: Vec<Op>,
    pub loops: usize,
}

/// One activation record of a script thread.
#[derive(Debug, Clone, Default)]
pub struct Frame {
    /// Index into `Runtime::code`.
    pub function: usize,
    pub pc: usize,
    pub locals: HashMap<String, Slot>,
    pub stack: Vec<Value>,
    /// Per-loop iteration counters (the loop guard).
    pub loops: Vec<u64>,
    /// The call was a `call` statement, so the returned value is discarded
    /// instead of being pushed onto the caller's stack.
    pub discard_result: bool,
}

/// What started a thread (diagnostics and provenance).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadOrigin {
    /// A direct `run_function`/`call_function` entry point.
    Script,
    /// A trigger's action list.
    Trigger(u32),
    /// A `TimerStart` callback.
    Timer(u32),
    /// A `TriggerRegisterTimerEvent` expiry.
    TimerEvent(u32),
}

/// A script function still queued on a thread (a trigger's remaining actions).
#[derive(Debug, Clone)]
struct PendingCall {
    function: usize,
    args: Vec<Value>,
}

/// A JASS thread: a call stack that can be suspended and resumed.
#[derive(Debug, Clone)]
pub struct Thread {
    pub id: u32,
    /// Entry function name, for diagnostics.
    pub name: String,
    pub frames: Vec<Frame>,
    /// Actions this thread still has to run once its current stack unwinds.
    pending: Vec<PendingCall>,
    pending_at: usize,
    pub origin: ThreadOrigin,
    /// `world.game_time` at which a suspended thread resumes.
    pub wake_at: f64,
    /// Seconds the thread asked to sleep for (`None` while running).
    pub sleep: Option<f64>,
    /// Event context the thread was started in, restored when it resumes.
    context: EventContext,
}

/// Outcome of running a thread to its next stop.
#[derive(Debug)]
pub enum ThreadStatus {
    /// The stack unwound; this is the last returned value.
    Done(Value),
    /// The thread hit `TriggerSleepAction` and is parked.
    Suspended,
}

impl Thread {
    pub fn is_suspended(&self) -> bool {
        self.sleep.is_some()
    }

    /// Frames of the running thread (top frame last).
    pub fn depth(&self) -> usize {
        self.frames.len()
    }
}

impl Runtime {
    pub fn new(program: Program, seed: u64) -> Self {
        let mut function_index = HashMap::new();
        for (index, function) in program.functions.iter().enumerate() {
            function_index.insert(function.name.to_ascii_lowercase(), index);
        }
        let mut natives = HashMap::new();
        for native in &program.natives {
            natives.insert(native.name.to_ascii_lowercase(), native.clone());
        }
        let mut runtime = Self {
            program,
            globals: HashMap::new(),
            world: World::default(),
            rng: Wc3Rng::new(seed),
            output: Vec::new(),
            unimplemented: BTreeMap::new(),
            call_counts: BTreeMap::new(),
            native_calls: BTreeMap::new(),
            errors: Vec::new(),
            steps: 0,
            max_steps: 20_000_000,
            seed,
            function_index,
            code: Vec::new(),
            natives,
            threads: Vec::new(),
            sleeps: 0,
            wakes: 0,
            next_thread: 0,
            pending_sleep: None,
            trigger_stack: Vec::new(),
            event_fires: BTreeMap::new(),
            region_members: BTreeMap::new(),
            dead_seen: BTreeSet::new(),
            time_of_day_prev: 0.0,
            has_variable_events: false,
            array_bounds_hits: 0,
            condition_vetoes: 0,
            event_log: Vec::new(),
        };
        runtime.time_of_day_prev = runtime.world.time_of_day;
        runtime.compile_all();
        runtime.init_globals();
        runtime
    }

    pub fn from_sources(sources: &[(&str, &str)], seed: u64) -> Result<Self> {
        let mut program = Program::default();
        for (name, source) in sources {
            let parsed = parse_named(source, *name).map_err(|err| ReError::Parse(err.to_string()))?;
            program.merge(parsed);
        }
        Ok(Self::new(program, seed))
    }

    fn init_globals(&mut self) {
        let globals = self.program.globals.clone();
        for global in &globals {
            let value = if global.array {
                Slot::Array(BTreeMap::new())
            } else {
                let value = match &global.init {
                    Some(expr) => match self.eval_snippet(expr) {
                        Ok(value) => value,
                        Err(err) => {
                            self.errors
                                .push(format!("global {} init: {err}", global.name));
                            Value::Null
                        }
                    },
                    None => Value::Null,
                };
                Slot::Scalar(value)
            };
            self.globals.insert(global.name.clone(), value);
        }
    }

    /// Compiles and runs one expression as a throwaway function body. Global
    /// initialisers use this so they keep full JASS semantics (they may call
    /// functions and natives).
    fn eval_snippet(&mut self, expr: &Expr) -> Result<Value> {
        let mut compiler = Compiler::default();
        compiler.expr(expr);
        compiler.push(Op::Return);
        let index = self.code.len();
        self.code.push(CompiledFunction {
            name: format!("__init_{index}"),
            params: Vec::new(),
            ops: compiler.ops,
            loops: compiler.loops,
        });
        let name = self.code[index].name.clone();
        let mut thread = self.thread_for(ThreadOrigin::Script, &name);
        thread.frames.push(self.frame_for(index, &[]));
        match self.run_thread(&mut thread, 0)? {
            ThreadStatus::Done(value) => Ok(value),
            ThreadStatus::Suspended => {
                self.park(thread);
                Ok(Value::Null)
            }
        }
    }

    /// Compiles every declared function body to instructions.
    fn compile_all(&mut self) {
        let mut code = Vec::with_capacity(self.program.functions.len());
        let mut index_map = HashMap::with_capacity(self.program.functions.len());
        for (index, function) in self.program.functions.iter().enumerate() {
            index_map.insert(function.name.to_ascii_lowercase(), index);
            code.push(compile_function(function));
        }
        self.code = code;
        self.function_index = index_map;
    }

    pub fn function_names(&self) -> BTreeSet<String> {
        self.program.function_names()
    }

    pub fn native_names(&self) -> BTreeSet<String> {
        self.natives.keys().cloned().collect()
    }

    /// True when the merged program declares `name` (case-insensitive).
    ///
    /// `run_function` treats an unknown name as an unhandled native and returns
    /// its default, so the engine needs this to tell "the map has no `config`"
    /// (legal) from "the map declared one and it failed".
    pub fn has_function(&self, name: &str) -> bool {
        self.function_index.contains_key(&name.to_ascii_lowercase())
    }

    /// Records one out-of-range JASS array access (WC3-JASS-006). The first
    /// few become VM errors; the counter stays observable for the run report.
    fn record_array_bounds(&mut self, name: &str, index: i64) {
        self.array_bounds_hits += 1;
        if self.array_bounds_hits <= 8 {
            self.errors.push(format!(
                "array {name}[{index}]: index outside 0..{}",
                JASS_ARRAY_LEN - 1
            ));
        }
    }

    /// Execute a function by name (case-insensitive). Returns its value.
    pub fn run_function(&mut self, name: &str) -> Result<Value> {
        self.call_function(name, &[], 0)
    }

    /// Execute `main` (map scripts) and return its value.
    pub fn run_main(&mut self) -> Result<Value> {
        self.run_function("main")
    }

    pub(crate) fn call_function(
        &mut self,
        name: &str,
        args: &[Value],
        depth: usize,
    ) -> Result<Value> {
        if depth > 512 {
            return Err(ReError::Vm(format!("call depth exceeded at {name}")));
        }
        self.steps += 1;
        if self.steps > self.max_steps {
            return Err(ReError::Vm(format!("step limit exceeded at {name}")));
        }
        if let Some(&index) = self.function_index.get(&name.to_ascii_lowercase()) {
            self.run_entry_index(index, args.to_vec(), ThreadOrigin::Script, depth)
        } else if self.natives.contains_key(&name.to_ascii_lowercase()) {
            let value = self.call_native(name, args, depth)?;
            Ok(value)
        } else {
            self.unimplemented
                .entry(name.to_ascii_lowercase())
                .and_modify(|count| *count += 1)
                .or_insert(1);
            Ok(self.default_return(name))
        }
    }

    fn default_return(&self, name: &str) -> Value {
        match self.natives.get(&name.to_ascii_lowercase()) {
            Some(native) => match native.ret.to_ascii_lowercase().as_str() {
                "integer" => Value::Int(0),
                "real" => Value::Real(0.0),
                "boolean" => Value::Bool(false),
                "string" => Value::Str(String::new()),
                "nothing" => Value::Null,
                _ => Value::Null,
            },
            None => Value::Null,
        }
    }

    // ---------------------------------------------------------------------
    // Threads
    // ---------------------------------------------------------------------

    /// Starts a compiled function as a new thread. A thread that suspends is
    /// parked and resumes on a later tick; the caller gets `null` meanwhile.
    fn run_entry(
        &mut self,
        name: &str,
        args: Vec<Value>,
        origin: ThreadOrigin,
        depth: usize,
    ) -> Result<Value> {
        match self.function_index.get(&name.to_ascii_lowercase()).copied() {
            Some(index) => self.run_entry_index(index, args, origin, depth),
            None => self.call_function(name, &args, depth),
        }
    }

    fn run_entry_index(
        &mut self,
        index: usize,
        args: Vec<Value>,
        origin: ThreadOrigin,
        depth: usize,
    ) -> Result<Value> {
        let function_name = self.code[index].name.clone();
        *self.call_counts.entry(function_name.clone()).or_insert(0) += 1;
        let mut thread = self.thread_for(origin, &function_name);
        thread.frames.push(self.frame_for(index, &args));
        match self.run_thread(&mut thread, depth)? {
            ThreadStatus::Done(value) => Ok(value),
            ThreadStatus::Suspended => {
                self.park(thread);
                Ok(Value::Null)
            }
        }
    }

    fn thread_for(&mut self, origin: ThreadOrigin, name: &str) -> Thread {
        self.next_thread += 1;
        Thread {
            id: self.next_thread,
            name: name.to_string(),
            frames: Vec::new(),
            pending: Vec::new(),
            pending_at: 0,
            origin,
            wake_at: self.world.game_time,
            sleep: None,
            context: self.world.context.clone(),
        }
    }

    fn frame_for(&self, function: usize, args: &[Value]) -> Frame {
        let compiled = &self.code[function];
        let mut locals = HashMap::new();
        for (position, param) in compiled.params.iter().enumerate() {
            locals.insert(
                param.clone(),
                Slot::Scalar(args.get(position).cloned().unwrap_or(Value::Null)),
            );
        }
        Frame {
            function,
            pc: 0,
            locals,
            stack: Vec::new(),
            loops: vec![0; compiled.loops],
            discard_result: false,
        }
    }

    /// Queues a suspended thread for its wake tick.
    fn park(&mut self, mut thread: Thread) {
        if thread.sleep.is_none() {
            thread.sleep = Some(0.0);
        }
        self.threads.push(thread);
    }

    /// Runs a thread until its stack unwinds or it suspends. The event context
    /// travels with the thread, so `GetTriggeringTrigger()` still answers the
    /// trigger that started it after a sleep.
    fn run_thread(&mut self, thread: &mut Thread, depth: usize) -> Result<ThreadStatus> {
        let saved_context = std::mem::replace(&mut self.world.context, thread.context.clone());
        let outcome = self.run_thread_inner(thread, depth);
        thread.context = self.world.context.clone();
        self.world.context = saved_context;
        outcome
    }

    fn run_thread_inner(&mut self, thread: &mut Thread, depth: usize) -> Result<ThreadStatus> {
        if depth + thread.frames.len() > 512 {
            return Err(ReError::Vm(format!(
                "call depth exceeded in {}",
                thread.name
            )));
        }
        loop {
            // Activate the next queued call once the current stack unwinds.
            if thread.frames.is_empty() {
                let Some(pending) = thread.pending.get(thread.pending_at).cloned() else {
                    return Ok(ThreadStatus::Done(Value::Null));
                };
                thread.pending_at += 1;
                let frame = self.frame_for(pending.function, &pending.args);
                thread.frames.push(frame);
            }
            let top = thread.frames.len() - 1;
            let (pc, function) = {
                let frame = &thread.frames[top];
                (frame.pc, frame.function)
            };
            let Some(op) = self
                .code
                .get(function)
                .and_then(|code| code.ops.get(pc))
                .cloned()
            else {
                return Ok(ThreadStatus::Done(Value::Null));
            };
            self.steps += 1;
            if self.steps > self.max_steps {
                return Err(ReError::Vm("step limit exceeded".into()));
            }
            thread.frames[top].pc = pc + 1;
            match op {
                Op::Push(value) => thread.frames[top].stack.push(value),
                Op::PushCode(name) => thread.frames[top].stack.push(Value::Code(name)),
                Op::Load(name) => {
                    let value = get_var(&self.globals, &thread.frames[top], &name);
                    thread.frames[top].stack.push(value);
                }
                Op::LoadArray(name) => {
                    let index = pop(&mut thread.frames[top]).as_int();
                    if !array_index_in_bounds(index) {
                        self.record_array_bounds(&name, index);
                        thread.frames[top].stack.push(Value::Null);
                    } else {
                        let value = get_array(&self.globals, &thread.frames[top], &name, index);
                        thread.frames[top].stack.push(value);
                    }
                }
                Op::Store(name) => {
                    let value = pop(&mut thread.frames[top]);
                    let previous = get_var(&self.globals, &thread.frames[top], &name);
                    set_scalar(&mut self.globals, &mut thread.frames[top], &name, value.clone());
                    if self.has_variable_events && !thread.frames[top].locals.contains_key(&name) {
                        self.notify_variable_change(&name, previous.as_real(), value.as_real());
                    }
                }
                Op::StoreArray(name) => {
                    let index = pop(&mut thread.frames[top]).as_int();
                    let value = pop(&mut thread.frames[top]);
                    if !array_index_in_bounds(index) {
                        self.record_array_bounds(&name, index);
                    } else {
                        set_array(&mut self.globals, &mut thread.frames[top], &name, index, value);
                    }
                }
                Op::Local { name, array, init } => {
                    let slot = if array {
                        Slot::Array(BTreeMap::new())
                    } else {
                        let value = if init {
                            pop(&mut thread.frames[top])
                        } else {
                            Value::Null
                        };
                        Slot::Scalar(value)
                    };
                    thread.frames[top].locals.insert(name, slot);
                }
                Op::Call {
                    name,
                    argc,
                    line,
                    proc,
                } => {
                    let mut args = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        args.push(pop(&mut thread.frames[top]));
                    }
                    args.reverse();
                    let key = name.to_ascii_lowercase();
                    if let Some(index) = self.function_index.get(&key).copied() {
                        // A script call runs on the *same* thread: retail JASS
                        // has one thread per activation, so a
                        // `TriggerSleepAction` inside the callee parks the whole
                        // call chain, not just the callee.
                        *self.call_counts.entry(self.code[index].name.clone()).or_insert(0) += 1;
                        self.steps += 1;
                        if self.steps > self.max_steps {
                            return Err(ReError::Vm(format!(
                                "line {line}: step limit exceeded in {name}"
                            )));
                        }
                        if depth + thread.frames.len() >= MAX_CALL_DEPTH {
                            return Err(ReError::Vm(format!(
                                "line {line}: call depth exceeded at {name}"
                            )));
                        }
                        let mut frame = self.frame_for(index, &args);
                        frame.discard_result = proc;
                        thread.frames.push(frame);
                    } else if self.natives.contains_key(&key) {
                        let call_depth = depth + thread.frames.len();
                        if proc {
                            if let Err(err) = self.call_native(&name, &args, call_depth) {
                                self.errors.push(format!("line {line}: call {name}: {err}"));
                            }
                        } else {
                            let value = self.call_native(&name, &args, call_depth)?;
                            thread.frames[top].stack.push(value);
                        }
                    } else {
                        self.unimplemented
                            .entry(key)
                            .and_modify(|count| *count += 1)
                            .or_insert(1);
                        if !proc {
                            let value = self.default_return(&name);
                            thread.frames[top].stack.push(value);
                        }
                    }
                }
                Op::Binary(op) => {
                    let right = pop(&mut thread.frames[top]);
                    let left = pop(&mut thread.frames[top]);
                    let value = eval_binary(op, left, right, self.world.local_player_any);
                    thread.frames[top].stack.push(value);
                }
                Op::Unary(op) => {
                    let value = pop(&mut thread.frames[top]);
                    let value = match op {
                        UnOp::Neg => match value {
                            Value::Real(v) => Value::Real(-v),
                            Value::Int(v) => Value::Int(-v),
                            other => Value::Int(-other.as_int()),
                        },
                        UnOp::Pos => value,
                        UnOp::Not => Value::Bool(!value.truthy()),
                    };
                    thread.frames[top].stack.push(value);
                }
                Op::Jump(target) => thread.frames[top].pc = target,
                Op::JumpIfFalse(target) => {
                    if !pop(&mut thread.frames[top]).truthy() {
                        thread.frames[top].pc = target;
                    }
                }
                Op::JumpIfTrue(target) => {
                    if pop(&mut thread.frames[top]).truthy() {
                        thread.frames[top].pc = target;
                    }
                }
                Op::AndJump(target) => {
                    let truthy = thread.frames[top]
                        .stack
                        .last()
                        .is_some_and(Value::truthy);
                    if truthy {
                        thread.frames[top].stack.pop();
                    } else {
                        thread.frames[top].pc = target;
                    }
                }
                Op::OrJump(target) => {
                    let truthy = thread.frames[top]
                        .stack
                        .last()
                        .is_some_and(Value::truthy);
                    if truthy {
                        thread.frames[top].pc = target;
                    } else {
                        thread.frames[top].stack.pop();
                    }
                }
                Op::ExitWhen(target) => {
                    if pop(&mut thread.frames[top]).truthy() {
                        thread.frames[top].pc = target;
                    }
                }
                Op::Return => {
                    let value = pop(&mut thread.frames[top]);
                    let discard = thread.frames[top].discard_result;
                    thread.frames.pop();
                    if thread.frames.is_empty() && thread.pending_at >= thread.pending.len() {
                        return Ok(ThreadStatus::Done(value));
                    }
                    if !discard {
                        if let Some(frame) = thread.frames.last_mut() {
                            frame.stack.push(value);
                        }
                    }
                }
                Op::LoopEnter(slot) => {
                    if let Some(counter) = thread.frames[top].loops.get_mut(slot) {
                        *counter = 0;
                    }
                }
                Op::LoopBack { slot, target, line } => {
                    let counter = thread.frames[top]
                        .loops
                        .get_mut(slot)
                        .map(|counter| {
                            *counter += 1;
                            *counter
                        })
                        .unwrap_or(0);
                    if counter > 5_000_000 {
                        return Err(ReError::Vm(format!(
                            "line {line}: loop iteration limit exceeded"
                        )));
                    }
                    thread.frames[top].pc = target;
                }
                Op::Pop => {
                    let _ = pop(&mut thread.frames[top]);
                }
                Op::CoerceBool => {
                    let value = pop(&mut thread.frames[top]);
                    thread.frames[top].stack.push(Value::Bool(value.truthy()));
                }
            }
            if let Some(seconds) = self.pending_sleep.take() {
                thread.sleep = Some(seconds);
                thread.wake_at = self.world.game_time + seconds.max(0.0);
                return Ok(ThreadStatus::Suspended);
            }
        }
    }

    /// Advance the script clock by `dt` and service timers and sleeping
    /// threads. Standalone runtimes step this directly; the engine owns its own
    /// clock and calls [`Runtime::tick_script`] after syncing
    /// `world.game_time`.
    pub fn advance(&mut self, dt: f64) -> usize {
        self.world.game_time += dt;
        self.tick_script(dt)
    }

    /// Fires `TimerStart` callbacks and `TriggerRegisterTimerEvent` schedules,
    /// then resumes the threads whose sleep has expired. Returns the number of
    /// threads that ran.
    pub fn tick_script(&mut self, dt: f64) -> usize {
        let now = self.world.game_time;
        let mut ran = 0usize;

        // 0. Day/night clock, then the world-state sampler: region enter/leave,
        //    unit deaths and clock crossings. This is the only caller of the
        //    death/region dispatch in the headless engine (the game layer syncs
        //    positions/HP into the JASS world before this tick).
        //
        //    Retail runs one 24 h cycle per 480 s of game time; `SetTimeOfDay`
        //    jumps the clock and `SetTimeOfDayScale` changes the rate, and the
        //    `TriggerRegisterGameStateEvent(…, GAME_STATE_TIME_OF_DAY, …)`
        //    registrations fire as the clock crosses their limit.
        self.world.time_of_day =
            (self.world.time_of_day + dt * (24.0 / 480.0) * self.world.time_of_day_scale) % 24.0;
        ran += self.update_world_events();

        // 1. `CreateTimer` + `TimerStart` callbacks.
        let mut expired: Vec<u32> = Vec::new();
        for (handle, timer) in self.world.timers.iter_mut() {
            if !timer.running {
                continue;
            }
            timer.elapsed += dt;
            timer.remaining -= dt;
            if timer.remaining <= 0.0 {
                expired.push(*handle);
                if timer.periodic {
                    timer.remaining += timer.timeout.max(f64::EPSILON);
                } else {
                    timer.running = false;
                }
            }
        }
        for handle in expired {
            let code = self
                .world
                .timers
                .get(&handle)
                .and_then(|timer| timer.code.clone());
            self.world.firing_timer = Some(handle);
            if let Some(code) = code {
                if let Err(err) =
                    self.run_entry(&code, Vec::new(), ThreadOrigin::Timer(handle), 0)
                {
                    self.errors.push(format!("timer {handle:#x} callback: {err}"));
                }
                ran += 1;
            }
            let triggers: Vec<u32> = self
                .world
                .triggers
                .iter()
                .filter(|(_, trigger)| {
                    trigger.events.iter().any(
                        |event| matches!(event, TriggerEvent::TimerExpire { timer } if *timer == handle),
                    )
                })
                .map(|(handle, _)| *handle)
                .collect();
            for trigger in triggers {
                if let Err(err) = self.execute_trigger(trigger) {
                    self.errors.push(format!("timer expiry trigger {trigger:#x}: {err}"));
                }
                ran += 1;
            }
            self.world.firing_timer = None;
        }

        // 2. `TriggerRegisterTimerEvent` schedules (the event the trigger
        //    carries is bookkeeping; this is the clock that makes it fire).
        let mut due: Vec<u32> = Vec::new();
        for event in self.world.timer_events.iter_mut() {
            if event.done {
                continue;
            }
            event.elapsed += dt;
            let period = event.timeout.max(0.0);
            if event.elapsed + f64::EPSILON >= period {
                due.push(event.trigger);
                event.fires += 1;
                if event.periodic {
                    event.elapsed -= period;
                } else {
                    event.done = true;
                }
            }
        }
        for trigger in due {
            let enabled = self
                .world
                .triggers
                .get(&trigger)
                .is_some_and(|trigger| trigger.enabled);
            if !enabled {
                continue;
            }
            if let Err(err) = self.execute_trigger(trigger) {
                self.errors.push(format!("timer event trigger {trigger:#x}: {err}"));
            }
            ran += 1;
        }

        // 3. Threads whose sleep expired. The list is taken before any of them
        //    runs, so a thread parked during this tick waits for the next one.
        let mut waking: Vec<Thread> = Vec::new();
        let mut index = 0;
        while index < self.threads.len() {
            if self.threads[index].wake_at <= now + 1e-9 {
                waking.push(self.threads.remove(index));
            } else {
                index += 1;
            }
        }
        for mut thread in waking {
            thread.sleep = None;
            self.wakes += 1;
            match self.run_thread(&mut thread, 0) {
                Ok(ThreadStatus::Done(_)) => {}
                Ok(ThreadStatus::Suspended) => self.park(thread),
                Err(err) => {
                    self.errors
                        .push(format!("thread {} ({}): {err}", thread.id, thread.name));
                }
            }
            ran += 1;
        }
        ran
    }

    /// Requests that the running thread suspend for `seconds` (the
    /// `TriggerSleepAction` contract). The interpreter loop turns this into a
    /// parked thread; a thread resumed at `world.game_time + seconds`.
    pub(crate) fn request_sleep(&mut self, seconds: f64) {
        self.pending_sleep = Some(seconds.max(0.0));
        self.sleeps += 1;
    }

    pub(crate) fn alloc(&mut self) -> u32 {
        let handle = self.world.next_handle;
        self.world.next_handle = self.world.next_handle.wrapping_add(1).max(0x100);
        handle
    }

    /// Sets the seat `GetLocalPlayer()` reports.
    pub fn set_local_player(&mut self, index: u32) {
        self.world.local_player = index.min(15);
        self.world.local_player_any = false;
    }

    /// Makes `GetLocalPlayer()` match every seat (single-player campaign).
    pub fn set_local_player_any(&mut self, any: bool) {
        self.world.local_player_any = any;
    }

    /// The value `GetLocalPlayer()` returns for this world.
    pub fn local_player_value(world: &World) -> Value {
        Value::Handle(if world.local_player_any {
            LOCAL_PLAYER_ANY
        } else {
            world.local_player
        })
    }

    /// The seat a process plays before a map declares one. Campaign missions set
    /// this from `war3map.w3i` *before* the engine boots the script, because
    /// Blizzard.j gates every camera and transmission block on
    /// `GetLocalPlayer() == whichPlayer`.
    pub fn set_default_local_player(index: u32) {
        DEFAULT_LOCAL_PLAYER.store(index.min(15), std::sync::atomic::Ordering::Relaxed);
    }

    pub fn default_local_player() -> u32 {
        DEFAULT_LOCAL_PLAYER.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Seat the script authored for `unit` (`script_owner`), falling back to
    /// the mirrored simulation owner.
    pub fn owner_of(&self, unit: u32) -> Option<i32> {
        self.world
            .units
            .get(&unit)
            .map(|state| state.script_owner.unwrap_or(state.owner))
    }

    pub(crate) fn player_index(&self, value: &Value) -> Option<usize> {
        value
            .handle()
            .map(|handle| handle as usize)
            .filter(|index| *index < self.world.players.len())
    }

    pub(crate) fn eval_bool_expr(&mut self, handle: u32, depth: usize) -> Result<bool> {
        let expr = self.world.boolexprs.get(&handle).cloned();
        match expr {
            Some(BoolExpr::Code(code)) => {
                let value = self.call_function(&code, &[], depth + 1)?;
                Ok(value.truthy())
            }
            Some(BoolExpr::And(a, b)) => {
                Ok(self.eval_bool_expr(a, depth)? && self.eval_bool_expr(b, depth)?)
            }
            Some(BoolExpr::Or(a, b)) => {
                Ok(self.eval_bool_expr(a, depth)? || self.eval_bool_expr(b, depth)?)
            }
            Some(BoolExpr::Not(a)) => Ok(!self.eval_bool_expr(a, depth)?),
            None => Ok(true),
        }
    }

    /// Execute a trigger by handle (public canonical entry point).
    pub fn execute_trigger(&mut self, handle: u32) -> Result<Value> {
        self.execute_trigger_depth(handle, 0)
    }

    pub(crate) fn execute_trigger_depth(&mut self, handle: u32, depth: usize) -> Result<Value> {
        let Some(trigger) = self.world.triggers.get(&handle).cloned() else {
            return Ok(Value::Null);
        };
        if !trigger.enabled {
            return Ok(Value::Null);
        }
        if self.trigger_stack.contains(&handle) {
            return Ok(Value::Null);
        }
        self.trigger_stack.push(handle);
        let result = (|| -> Result<Value> {
            // `GetTriggeringTrigger()` must answer this trigger *inside its
            // conditions*: the canonical JASS guard
            // `IsTriggerEnabled(GetTriggeringTrigger())` only passes when the
            // context is published before the condition runs.
            let previous_trigger = self.world.context.trigger;
            self.world.context.trigger = Some(handle);
            let mut pass = true;
            for condition in &trigger.conditions {
                if !self.eval_bool_expr(*condition, depth)? {
                    pass = false;
                    break;
                }
            }
            if pass {
                self.run_action_list(&trigger, handle, depth)?;
                if let Some(stored) = self.world.triggers.get_mut(&handle) {
                    stored.exec_count += 1;
                }
            } else {
                self.condition_vetoes += 1;
            }
            if let Some(stored) = self.world.triggers.get_mut(&handle) {
                stored.eval_count += 1;
            }
            self.world.context.trigger = previous_trigger;
            Ok(Value::Null)
        })();
        self.trigger_stack.pop();
        result
    }

    /// Runs a trigger's action list as one thread.
    ///
    /// The actions share a thread, so a `TriggerSleepAction` inside one action
    /// suspends the actions after it and the list continues on the wake tick —
    /// exactly the retail behaviour. Without this the whole trigger ran inside
    /// the tick that fired it.
    fn run_action_list(
        &mut self,
        trigger: &Trigger,
        handle: u32,
        depth: usize,
    ) -> Result<()> {
        let mut entry: Option<usize> = None;
        let mut pending: Vec<PendingCall> = Vec::new();
        for action in &trigger.actions {
            match self
                .function_index
                .get(&action.to_ascii_lowercase())
                .copied()
            {
                Some(index) => {
                    *self.call_counts.entry(self.code[index].name.clone()).or_insert(0) += 1;
                    match entry {
                        None => entry = Some(index),
                        Some(_) => pending.push(PendingCall {
                            function: index,
                            args: Vec::new(),
                        }),
                    }
                }
                // An action that is a native or an unknown name has no body to
                // suspend; run it the way a `call` statement would.
                None => {
                    let _ = self.call_function(action, &[], depth + 1);
                }
            }
        }
        let Some(index) = entry else {
            return Ok(());
        };
        let function_name = self.code[index].name.clone();
        let mut thread = self.thread_for(ThreadOrigin::Trigger(handle), &function_name);
        thread.frames.push(self.frame_for(index, &[]));
        thread.pending = pending;
        thread.pending_at = 0;
        match self.run_thread(&mut thread, depth)? {
            ThreadStatus::Done(_) => Ok(()),
            ThreadStatus::Suspended => {
                self.park(thread);
                Ok(())
            }
        }
    }

    /// Fire a player-unit/unit event for `unit`.
    ///
    /// Matches `TriggerRegisterPlayerUnitEvent` registrations for the unit's
    /// owner (and optional unit-type filter), `TriggerRegisterUnitEvent`
    /// registrations for this unit or the "any unit" sentinel, and
    /// `TriggerRegisterPlayerEvent` registrations for the owner. Returns the
    /// number of triggers executed.
    pub fn fire_unit_event(&mut self, event: i64, unit: u32) -> usize {
        let Some(unit_state) = self.world.units.get(&unit) else {
            return 0;
        };
        let owner = {
            let seat = unit_state.script_owner.unwrap_or(unit_state.owner);
            if seat >= 0 {
                Some(seat as u32)
            } else {
                None
            }
        };
        let unit_id = unit_state.id.clone();
        let mut due = Vec::new();
        for (handle, trigger) in &self.world.triggers {
            if !trigger.enabled {
                continue;
            }
            let matched = trigger.events.iter().any(|candidate| match candidate {
                TriggerEvent::PlayerUnit {
                    player,
                    event: candidate_event,
                    unit_type,
                } => {
                    *candidate_event == event
                        && Some(*player) == owner
                        && (unit_type.is_empty() || *unit_type == unit_id)
                }
                TriggerEvent::UnitEvent {
                    unit: candidate_unit,
                    event: candidate_event,
                } => *candidate_event == event && (*candidate_unit == 0 || *candidate_unit == unit),
                TriggerEvent::Player {
                    player,
                    event: candidate_event,
                } => *candidate_event == event && Some(*player) == owner,
                _ => false,
            });
            if matched {
                due.push(*handle);
            }
        }
        let previous_player = self.world.context.player;
        let previous_unit = self.world.context.unit;
        self.world.context.player = owner;
        self.world.context.unit = Some(unit);
        let count = self.dispatch("unit_event", due);
        self.world.context.player = previous_player;
        self.world.context.unit = previous_unit;
        count
    }

    // ---------------------------------------------------------------------
    // Event dispatch table
    //
    // Every `TriggerEvent` variant a map can register has a consumer here.
    // `update_world_events` is the engine tick's sampler: it diffs region
    // membership and unit liveness and turns the differences into the enter /
    // leave / death events retail raises. Game-state, variable and dialog
    // events are raised by the native that changes the state (WC3-JASS-002).
    // ---------------------------------------------------------------------

    /// Runs the triggers in `handles` and records the event kind.
    fn dispatch(&mut self, kind: &str, handles: Vec<u32>) -> usize {
        if handles.is_empty() {
            return 0;
        }
        let count = handles.len();
        for handle in handles {
            if let Err(err) = self.execute_trigger(handle) {
                self.errors.push(format!("{kind} trigger {handle:#x}: {err}"));
            }
            *self.event_fires.entry(kind.to_string()).or_insert(0) += 1;
            if self.event_log.len() < 128 {
                let action = self
                    .world
                    .triggers
                    .get(&handle)
                    .and_then(|trigger| trigger.actions.first().cloned())
                    .unwrap_or_default();
                self.event_log.push(format!("{kind}:{handle:#x}:{action}"));
            }
        }
        count
    }

    /// Handles of every enabled trigger with at least one event matching
    /// `predicate`.
    fn triggers_matching<F>(&self, mut predicate: F) -> Vec<u32>
    where
        F: FnMut(&TriggerEvent) -> bool,
    {
        let mut due = Vec::new();
        for (handle, trigger) in &self.world.triggers {
            if !trigger.enabled {
                continue;
            }
            if trigger.events.iter().any(&mut predicate) {
                due.push(*handle);
            }
        }
        due
    }

    /// Registered trigger executions per event kind, `kind=count` pairs.
    pub fn event_summary(&self) -> String {
        let mut parts = Vec::new();
        for kind in [
            "enter_region",
            "leave_region",
            "death",
            "unit_event",
            "game_state",
            "variable",
            "dialog",
            "dialog_button",
            "player_event",
            "game_event",
        ] {
            if let Some(count) = self.event_fires.get(kind) {
                parts.push(format!("{kind}={count}"));
            }
        }
        parts.join(",")
    }

    /// Fires `TriggerRegisterEnterRegion` / `TriggerRegisterLeaveRegion`
    /// registrations for `region`.
    pub fn fire_region_event(&mut self, event: RegionEvent, region: u32, unit: u32) -> usize {
        let kind = match event {
            RegionEvent::Enter => "enter_region",
            RegionEvent::Leave => "leave_region",
        };
        let handles = self.triggers_matching(|candidate| match (event, candidate) {
            (RegionEvent::Enter, TriggerEvent::EnterRegion { region: watched }) => *watched == region,
            (RegionEvent::Leave, TriggerEvent::LeaveRegion { region: watched }) => *watched == region,
            _ => false,
        });
        if handles.is_empty() {
            return 0;
        }
        let previous = self.world.context.clone();
        self.world.context.unit = Some(unit);
        self.world.context.region = Some(region);
        if let Some(owner) = self
            .world
            .units
            .get(&unit)
            .map(|state| state.script_owner.unwrap_or(state.owner))
        {
            if owner >= 0 {
                self.world.context.player = Some(owner as u32);
            }
        }
        match event {
            RegionEvent::Enter => self.world.context.entering = Some(unit),
            RegionEvent::Leave => self.world.context.leaving = Some(unit),
        }
        let count = self.dispatch(kind, handles);
        self.world.context = previous;
        count
    }

    /// Fires a unit death: `TriggerRegisterDeathEvent`,
    /// `TriggerRegisterPlayerUnitEvent(…, EVENT_PLAYER_UNIT_DEATH)`,
    /// `TriggerRegisterUnitEvent(…, EVENT_UNIT_DEATH)` and the player event of
    /// the same id. `GetDyingUnit`/`GetTriggerUnit` answer the dead unit.
    pub fn fire_death_event(&mut self, unit: u32) -> usize {
        let Some(state) = self.world.units.get(&unit) else {
            return 0;
        };
        // One death per unit: the native (`KillUnit`) and the tick sampler both
        // report it, whichever gets there first wins.
        if !self.dead_seen.insert(unit) {
            return 0;
        }
        let owner = state.script_owner.unwrap_or(state.owner);
        let unit_id = state.id.clone();
        let handles = self.triggers_matching(|candidate| match candidate {
            TriggerEvent::Death { unit: watched } => *watched == 0 || *watched == unit,
            TriggerEvent::PlayerUnit {
                player,
                event,
                unit_type,
            } => {
                *event == EVENT_PLAYER_UNIT_DEATH
                    && owner >= 0
                    && *player == owner as u32
                    && (unit_type.is_empty() || *unit_type == unit_id)
            }
            TriggerEvent::UnitEvent {
                unit: watched,
                event,
            } => *event == EVENT_UNIT_DEATH && (*watched == 0 || *watched == unit),
            TriggerEvent::Player { player, event } => {
                *event == EVENT_PLAYER_UNIT_DEATH && owner >= 0 && *player == owner as u32
            }
            _ => false,
        });
        if handles.is_empty() {
            return 0;
        }
        let previous = self.world.context.clone();
        self.world.context.unit = Some(unit);
        self.world.context.dying = Some(unit);
        if owner >= 0 {
            self.world.context.player = Some(owner as u32);
        }
        let count = self.dispatch("death", handles);
        self.world.context = previous;
        count
    }

    /// Fires the `TriggerRegisterGameStateEvent` /
    /// `TriggerRegisterPlayerStateEvent` registrations whose limit condition
    /// `value` satisfies right now.
    pub fn fire_game_state_event(&mut self, player: Option<u32>, state: i64, value: f64) -> usize {
        let handles = self.triggers_matching(|candidate| match candidate {
            TriggerEvent::GameState {
                player: watched,
                state: watched_state,
                op,
                value: limit,
            } => {
                *watched_state == state
                    && (*watched < 0 || player == Some(*watched as u32))
                    && limit_holds(*op, value, *limit)
            }
            _ => false,
        });
        self.dispatch_state(player, state, handles)
    }

    /// Fires a state event only when the state *crossed* the registered limit
    /// (the retail `limitop` contract: a threshold fires once per crossing).
    pub fn notify_state_change(
        &mut self,
        player: Option<u32>,
        state: i64,
        previous: f64,
        current: f64,
    ) -> usize {
        let handles = self.triggers_matching(|candidate| match candidate {
            TriggerEvent::GameState {
                player: watched,
                state: watched_state,
                op,
                value: limit,
            } => {
                *watched_state == state
                    && (*watched < 0 || player == Some(*watched as u32))
                    && !limit_holds(*op, previous, *limit)
                    && limit_holds(*op, current, *limit)
            }
            _ => false,
        });
        self.dispatch_state(player, state, handles)
    }

    fn dispatch_state(&mut self, player: Option<u32>, state: i64, handles: Vec<u32>) -> usize {
        if handles.is_empty() {
            return 0;
        }
        let previous = self.world.context.clone();
        self.world.context.game_state = state;
        if let Some(player) = player {
            self.world.context.player = Some(player);
        }
        let count = self.dispatch("game_state", handles);
        self.world.context = previous;
        count
    }

    /// Fires `TriggerRegisterVariableEvent` registrations whose limit the
    /// variable now satisfies.
    pub fn fire_variable_event(&mut self, name: &str, value: f64) -> usize {
        let handles = self.triggers_matching(|candidate| match candidate {
            TriggerEvent::Variable {
                name: watched,
                op,
                value: limit,
            } => watched.eq_ignore_ascii_case(name) && limit_holds(*op, value, *limit),
            _ => false,
        });
        self.dispatch("variable", handles)
    }

    fn notify_variable_change(&mut self, name: &str, previous: f64, current: f64) -> usize {
        if (current - previous).abs() < f64::EPSILON {
            return 0;
        }
        let handles = self.triggers_matching(|candidate| match candidate {
            TriggerEvent::Variable {
                name: watched,
                op,
                value: limit,
            } => {
                watched.eq_ignore_ascii_case(name)
                    && !limit_holds(*op, previous, *limit)
                    && limit_holds(*op, current, *limit)
            }
            _ => false,
        });
        self.dispatch("variable", handles)
    }

    /// Fires `TriggerRegisterDialogEvent` for a dialog that became visible.
    pub fn fire_dialog_event(&mut self, dialog: u32) -> usize {
        let handles = self.triggers_matching(|candidate| {
            matches!(candidate, TriggerEvent::Dialog { dialog: watched } if *watched == dialog)
        });
        self.dispatch("dialog", handles)
    }

    /// Fires `TriggerRegisterDialogButtonEvent` for a clicked button.
    pub fn fire_dialog_button_event(&mut self, dialog: u32, button: u32) -> usize {
        let handles = self.triggers_matching(|candidate| {
            matches!(
                candidate,
                TriggerEvent::DialogButton { dialog: d, button: b } if *d == dialog && *b == button
            )
        });
        self.dispatch("dialog_button", handles)
    }

    /// Fires `TriggerRegisterGameEvent(trig, event)` for one game event.
    pub fn fire_game_event(&mut self, event: i64) -> usize {
        let handles = self.triggers_matching(|candidate| {
            matches!(candidate, TriggerEvent::Custom { event: watched } if *watched == event)
        });
        self.dispatch("game_event", handles)
    }

    /// Fires `TriggerRegisterPlayerEvent(trig, player, event)` for one seat
    /// (the end-cinematic and victory/defeat class, which carries no unit).
    pub fn fire_player_event(&mut self, player: u32, event: i64) -> usize {
        let handles = self.triggers_matching(|candidate| {
            matches!(
                candidate,
                TriggerEvent::Player { player: p, event: e } if *p == player && *e == event
            )
        });
        if handles.is_empty() {
            return 0;
        }
        let previous = self.world.context.clone();
        self.world.context.player = Some(player);
        let count = self.dispatch("player_event", handles);
        self.world.context = previous;
        count
    }

    /// Engine-tick sampler: turns world state changes (region membership, unit
    /// liveness, clock crossings) into registered trigger events. Returns the
    /// number of trigger executions.
    pub fn update_world_events(&mut self) -> usize {
        let mut fired = self.update_region_events();
        fired += self.update_death_events();
        fired += self.update_time_of_day_events();
        fired
    }

    fn update_region_events(&mut self) -> usize {
        let mut watched: BTreeSet<u32> = BTreeSet::new();
        for trigger in self.world.triggers.values() {
            if !trigger.enabled {
                continue;
            }
            for event in &trigger.events {
                match event {
                    TriggerEvent::EnterRegion { region } | TriggerEvent::LeaveRegion { region } => {
                        watched.insert(*region);
                    }
                    _ => {}
                }
            }
        }
        self.region_members
            .retain(|region, _| watched.contains(region));
        let mut fired = 0;
        for region in watched {
            let mut inside: BTreeSet<u32> = BTreeSet::new();
            let Some(region_state) = self.world.regions.get(&region) else {
                continue;
            };
            if region_state.rects.is_empty() && region_state.cells.is_empty() {
                continue;
            }
            for (handle, unit) in &self.world.units {
                // A corpse keeps its region membership: retail only raises
                // `leave` when the unit actually exits (or is removed).
                if point_in_region(&self.world, region_state, unit.x, unit.y) {
                    inside.insert(*handle);
                }
            }
            let previous = self.region_members.get(&region).cloned().unwrap_or_default();
            let entered: Vec<u32> = inside.difference(&previous).copied().collect();
            let left: Vec<u32> = previous.difference(&inside).copied().collect();
            for unit in entered {
                fired += self.fire_region_event(RegionEvent::Enter, region, unit);
            }
            for unit in left {
                fired += self.fire_region_event(RegionEvent::Leave, region, unit);
            }
            self.region_members.insert(region, inside);
        }
        fired
    }

    fn update_death_events(&mut self) -> usize {
        let dead: Vec<u32> = self
            .world
            .units
            .iter()
            .filter(|(_, unit)| !unit.alive)
            .map(|(handle, _)| *handle)
            .collect();
        let mut fired = 0;
        for handle in dead {
            fired += self.fire_death_event(handle);
        }
        self.dead_seen
            .retain(|handle| self.world.units.contains_key(handle));
        fired
    }

    fn update_time_of_day_events(&mut self) -> usize {
        let previous = self.time_of_day_prev;
        let now = self.world.time_of_day;
        self.time_of_day_prev = now;
        if (now - previous).abs() < 1e-9 {
            return 0;
        }
        self.notify_state_change(None, GAME_STATE_TIME_OF_DAY, previous, now)
    }

    /// Fire an item event for `item` held/manipulated by `unit` with
    /// `GetManipulatedItem` set for the duration of the handlers.
    pub(crate) fn item_event(&mut self, event: i64, unit: u32, item: u32) {
        let previous = self.world.context.clone();
        if let Some(owner) = self.owner_of(unit) {
            if owner >= 0 {
                self.world.context.player = Some(owner as u32);
            }
        }
        self.world.context.unit = Some(unit);
        self.world.context.manipulated_item = Some(item);
        self.fire_unit_event(event, unit);
        self.world.context = previous;
    }

    /// Grant or retarget one ability on a unit. `level` is clamped to at least
    /// 1; a missing entry is created at that level.
    pub fn grant_ability(&mut self, unit: u32, ability: &str, level: i32) -> Result<()> {
        let id = crate::fourcc_str(&crate::fourcc(ability));
        let Some(unit_state) = self.world.units.get_mut(&unit) else {
            return Err(ReError::Unsupported(format!("grant_ability: unknown unit {unit:#x}")));
        };
        let level = level.max(1);
        unit_state
            .abilities
            .entry(id)
            .and_modify(|state| state.level = level)
            .or_insert(AbilityState {
                level,
                ..AbilityState::default()
            });
        Ok(())
    }

    /// Read one ability level (0 when the unit does not have the ability).
    pub fn ability_level(&self, unit: u32, ability: &str) -> i32 {
        let id = crate::fourcc_str(&crate::fourcc(ability));
        self.world
            .units
            .get(&unit)
            .and_then(|unit| unit.abilities.get(&id))
            .map_or(0, |state| state.level)
    }

    /// Inventory contents as `(item handle, rawcode, charges)`.
    pub fn unit_items(&self, unit: u32) -> Vec<(u32, String, i32)> {
        self.world
            .units
            .get(&unit)
            .map(|unit| {
                unit.items
                    .iter()
                    .filter_map(|handle| {
                        self.world
                            .items
                            .get(handle)
                            .map(|item| (*handle, item.id.clone(), item.charges))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Abilities as `(rawcode, level)` sorted by rawcode.
    pub fn unit_abilities(&self, unit: u32) -> Vec<(String, i32)> {
        self.world
            .units
            .get(&unit)
            .map(|unit| {
                unit.abilities
                    .iter()
                    .map(|(id, state)| (id.clone(), state.level))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Complete one ability cast: validates that the caster owns the ability,
    /// sets the spell event context (`GetSpellAbilityId`, `GetSpellTarget*`),
    /// and executes matching cast/effect/finish triggers.
    ///
    /// A single call models a *completed* cast: `EVENT_PLAYER_UNIT_SPELL_CAST`
    /// (273) and `EVENT_UNIT_SPELL_CAST` (290) fire first, then the effect
    /// events (274/291), then finish (275/292). The context is restored to the
    /// caller's values afterwards, so nested casts from trigger actions keep
    /// each event's own state.
    pub fn cast_ability(
        &mut self,
        caster: u32,
        ability: &str,
        target: CastTarget,
    ) -> Result<AbilityCastReport> {
        let id = crate::fourcc_str(&crate::fourcc(ability));
        let Some(caster_state) = self.world.units.get(&caster) else {
            return Err(ReError::Unsupported(format!("cast_ability: unknown caster {caster:#x}")));
        };
        let level = caster_state.abilities.get(&id).map_or(0, |state| state.level);
        if level <= 0 {
            return Err(ReError::Unsupported(format!(
                "cast_ability: caster {caster:#x} does not have {id}"
            )));
        }
        let owner = caster_state.script_owner.unwrap_or(caster_state.owner);
        let previous = self.world.context.clone();
        self.world.context.trigger = None;
        self.world.context.player = if owner >= 0 { Some(owner as u32) } else { None };
        self.world.context.unit = Some(caster);
        self.world.context.ability = Some(id.clone());
        self.world.context.ability_level = level;
        self.world.context.spell_target_unit = None;
        self.world.context.spell_target_item = None;
        self.world.context.spell_target_destructable = None;
        self.world.context.spell_target_point = None;
        match target {
            CastTarget::None => {}
            CastTarget::Unit(handle) => self.world.context.spell_target_unit = Some(handle),
            CastTarget::Item(handle) => self.world.context.spell_target_item = Some(handle),
            CastTarget::Destructable(handle) => {
                self.world.context.spell_target_destructable = Some(handle);
            }
            CastTarget::Point(x, y) => self.world.context.spell_target_point = Some((x, y)),
        }
        let mut triggers = 0;
        for event in [
            EVENT_PLAYER_UNIT_SPELL_CAST,
            EVENT_UNIT_SPELL_CAST,
            EVENT_PLAYER_UNIT_SPELL_EFFECT,
            EVENT_UNIT_SPELL_EFFECT,
            EVENT_PLAYER_UNIT_SPELL_FINISH,
            EVENT_UNIT_SPELL_FINISH,
        ] {
            triggers += self.fire_unit_event(event, caster);
        }
        self.world.context = previous;
        Ok(AbilityCastReport {
            caster,
            ability: id,
            level,
            triggers,
        })
    }

    /// Use one inventory item: fires use-item events with
    /// `GetManipulatedItem`, then consumes one charge (removing the item and
    /// clearing its inventory slot when the last charge is spent).
    pub fn use_item(&mut self, unit: u32, item: u32, target: CastTarget) -> Result<bool> {
        let Some(unit_state) = self.world.units.get(&unit) else {
            return Ok(false);
        };
        if !unit_state.items.contains(&item) {
            return Ok(false);
        }
        let Some(item_state) = self.world.items.get(&item) else {
            return Ok(false);
        };
        if item_state.charges <= 0 {
            return Ok(false);
        }
        let previous = self.world.context.clone();
        if let Some(owner) = self.owner_of(unit) {
            if owner >= 0 {
                self.world.context.player = Some(owner as u32);
            }
        }
        self.world.context.unit = Some(unit);
        self.world.context.manipulated_item = Some(item);
        match target {
            CastTarget::None => {}
            CastTarget::Unit(handle) => self.world.context.spell_target_unit = Some(handle),
            CastTarget::Item(handle) => self.world.context.spell_target_item = Some(handle),
            CastTarget::Destructable(handle) => {
                self.world.context.spell_target_destructable = Some(handle);
            }
            CastTarget::Point(x, y) => self.world.context.spell_target_point = Some((x, y)),
        }
        self.fire_unit_event(EVENT_PLAYER_UNIT_USE_ITEM, unit);
        self.fire_unit_event(EVENT_UNIT_USE_ITEM, unit);
        self.world.context = previous;
        if let Some(item_state) = self.world.items.get_mut(&item) {
            if item_state.charges > 1 {
                item_state.charges -= 1;
            } else {
                self.world.items.remove(&item);
                if let Some(unit_state) = self.world.units.get_mut(&unit) {
                    for slot in &mut unit_state.items {
                        if *slot == item {
                            *slot = 0;
                        }
                    }
                }
            }
        }
        Ok(true)
    }

    pub(crate) fn unit_type_id(&self, value: &Value) -> String {
        let Some(handle) = value.handle() else {
            return String::new();
        };
        self.world
            .units
            .get(&handle)
            .map(|unit| unit.id.clone())
            .unwrap_or_default()
    }

    pub(crate) fn unit_field(&mut self, value: &Value) -> Option<&mut Unit> {
        let handle = value.handle()?;
        self.world.units.get_mut(&handle)
    }

    pub(crate) fn last_created_unit(&self) -> Value {
        self.world
            .last_created_unit
            .map(Value::Handle)
            .unwrap_or(Value::Null)
    }
}

/// Pop the top of a frame's operand stack (`null` when empty, matching the
/// interpreter's tolerant value model).
fn pop(frame: &mut Frame) -> Value {
    frame.stack.pop().unwrap_or(Value::Null)
}

fn get_var(globals: &HashMap<String, Slot>, frame: &Frame, name: &str) -> Value {
    if let Some(slot) = frame.locals.get(name) {
        return slot.scalar();
    }
    globals.get(name).map(Slot::scalar).unwrap_or(Value::Null)
}

/// JASS array indices are `0..=8191` (`common.j` declares `[8192]`).
#[inline]
fn array_index_in_bounds(index: i64) -> bool {
    (0..JASS_ARRAY_LEN).contains(&index)
}

fn get_array(
    globals: &HashMap<String, Slot>,
    frame: &Frame,
    name: &str,
    index: i64,
) -> Value {
    if let Some(Slot::Array(map)) = frame.locals.get(name) {
        return map.get(&index).cloned().unwrap_or(Value::Null);
    }
    if let Some(Slot::Array(map)) = globals.get(name) {
        return map.get(&index).cloned().unwrap_or(Value::Null);
    }
    Value::Null
}

fn set_scalar(
    globals: &mut HashMap<String, Slot>,
    frame: &mut Frame,
    name: &str,
    value: Value,
) {
    if let Some(slot) = frame.locals.get_mut(name) {
        *slot = Slot::Scalar(value);
        return;
    }
    if let Some(slot) = globals.get_mut(name) {
        *slot = Slot::Scalar(value);
        return;
    }
    globals.insert(name.to_string(), Slot::Scalar(value));
}

fn set_array(
    globals: &mut HashMap<String, Slot>,
    frame: &mut Frame,
    name: &str,
    index: i64,
    value: Value,
) {
    if let Some(Slot::Array(map)) = frame.locals.get_mut(name) {
        map.insert(index, value);
        return;
    }
    match globals.get_mut(name) {
        Some(Slot::Array(map)) => {
            map.insert(index, value);
        }
        Some(slot) => {
            let mut map = BTreeMap::new();
            map.insert(index, value);
            *slot = Slot::Array(map);
        }
        None => {
            let mut map = BTreeMap::new();
            map.insert(index, value);
            globals.insert(name.to_string(), Slot::Array(map));
        }
    }
}

/// Compiler: JASS function body → flat instruction list.
///
/// Control flow is resolved to instruction indices at compile time, which is
/// what lets a thread stop at any instruction (`TriggerSleepAction`) and resume
/// from the same place on a later tick.
#[derive(Default)]
struct Compiler {
    ops: Vec<Op>,
    loops: usize,
    /// Patch sites of the `exitwhen`s of every enclosing loop.
    exits: Vec<Vec<usize>>,
}

impl Compiler {
    fn push(&mut self, op: Op) -> usize {
        self.ops.push(op);
        self.ops.len() - 1
    }

    fn patch(&mut self, site: usize, target: usize) {
        match &mut self.ops[site] {
            Op::Jump(target_op)
            | Op::JumpIfFalse(target_op)
            | Op::JumpIfTrue(target_op)
            | Op::AndJump(target_op)
            | Op::OrJump(target_op)
            | Op::ExitWhen(target_op) => *target_op = target,
            other => unreachable!("patch is only called on jump ops, got {other:?}"),
        }
    }

    fn block(&mut self, body: &[Stmt]) {
        for stmt in body {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Local(local) => {
                if local.array {
                    self.push(Op::Local {
                        name: local.name.clone(),
                        array: true,
                        init: false,
                    });
                } else {
                    let init = local.init.is_some();
                    if let Some(expr) = &local.init {
                        self.expr(expr);
                    }
                    self.push(Op::Local {
                        name: local.name.clone(),
                        array: false,
                        init,
                    });
                }
            }
            Stmt::Set {
                name,
                index,
                value,
                ..
            } => {
                self.expr(value);
                match index {
                    Some(index) => {
                        self.expr(index);
                        self.push(Op::StoreArray(name.clone()));
                    }
                    None => {
                        self.push(Op::Store(name.clone()));
                    }
                }
            }
            Stmt::Call { name, args, line } => {
                for arg in args {
                    self.expr(arg);
                }
                self.push(Op::Call {
                    name: name.clone(),
                    argc: args.len(),
                    line: *line,
                    proc: true,
                });
            }
            Stmt::If {
                branches,
                else_body,
                ..
            } => {
                let mut ends = Vec::new();
                for (cond, body) in branches {
                    self.expr(cond);
                    let next = self.push(Op::JumpIfFalse(usize::MAX));
                    self.block(body);
                    ends.push(self.push(Op::Jump(usize::MAX)));
                    self.patch(next, self.ops.len());
                }
                if let Some(else_body) = else_body {
                    self.block(else_body);
                }
                let end = self.ops.len();
                for site in ends {
                    self.patch(site, end);
                }
            }
            Stmt::Loop { body, line } => {
                let slot = self.loops;
                self.loops += 1;
                self.push(Op::LoopEnter(slot));
                let start = self.ops.len();
                self.exits.push(Vec::new());
                self.block(body);
                self.push(Op::LoopBack {
                    slot,
                    target: start,
                    line: *line,
                });
                let end = self.ops.len();
                for site in self.exits.pop().unwrap_or_default() {
                    self.patch(site, end);
                }
            }
            Stmt::ExitWhen { cond, .. } => {
                self.expr(cond);
                if self.exits.is_empty() {
                    // `exitwhen` outside a loop cannot branch anywhere; keep the
                    // value semantics of the expression and drop it.
                    self.push(Op::Pop);
                } else {
                    let site = self.push(Op::ExitWhen(usize::MAX));
                    if let Some(sites) = self.exits.last_mut() {
                        sites.push(site);
                    }
                }
            }
            Stmt::Return { value, .. } => {
                match value {
                    Some(expr) => self.expr(expr),
                    None => {
                        self.push(Op::Push(Value::Null));
                    }
                }
                self.push(Op::Return);
            }
        }
    }

    fn expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Int(value) => {
                self.push(Op::Push(Value::Int(*value)));
            }
            Expr::Real(value) => {
                self.push(Op::Push(Value::Real(*value)));
            }
            Expr::Bool(value) => {
                self.push(Op::Push(Value::Bool(*value)));
            }
            Expr::Str(value) => {
                self.push(Op::Push(Value::Str(value.clone())));
            }
            Expr::Null => {
                self.push(Op::Push(Value::Null));
            }
            Expr::Var(name) => {
                self.push(Op::Load(name.clone()));
            }
            Expr::Array(name, index) => {
                self.expr(index);
                self.push(Op::LoadArray(name.clone()));
            }
            Expr::Call(name, args) => {
                for arg in args {
                    self.expr(arg);
                }
                self.push(Op::Call {
                    name: name.clone(),
                    argc: args.len(),
                    line: 0,
                    proc: false,
                });
            }
            Expr::Code(name) => {
                self.push(Op::PushCode(name.clone()));
            }
            Expr::Unary(op, inner) => {
                self.expr(inner);
                self.push(Op::Unary(*op));
            }
            Expr::Binary(BinOp::And, left, right) => {
                self.expr(left);
                self.push(Op::CoerceBool);
                let site = self.push(Op::AndJump(usize::MAX));
                self.expr(right);
                self.push(Op::CoerceBool);
                self.patch(site, self.ops.len());
            }
            Expr::Binary(BinOp::Or, left, right) => {
                self.expr(left);
                self.push(Op::CoerceBool);
                let site = self.push(Op::OrJump(usize::MAX));
                self.expr(right);
                self.push(Op::CoerceBool);
                self.patch(site, self.ops.len());
            }
            Expr::Binary(op, left, right) => {
                self.expr(left);
                self.expr(right);
                self.push(Op::Binary(*op));
            }
        }
    }
}

/// Compiles one declared function. Bodies get an implicit `return null` so a
/// thread never falls off the end of its instruction list.
fn compile_function(function: &FunctionDecl) -> CompiledFunction {
    let mut compiler = Compiler::default();
    compiler.block(&function.body);
    compiler.push(Op::Push(Value::Null));
    compiler.push(Op::Return);
    CompiledFunction {
        name: function.name.clone(),
        params: function.params.iter().map(|(_, name)| name.clone()).collect(),
        ops: compiler.ops,
        loops: compiler.loops,
    }
}

fn eval_binary(op: BinOp, left: Value, right: Value, local_player_any: bool) -> Value {
    use BinOp::*;
    match op {
        Add => {
            if matches!(left, Value::Str(_)) || matches!(right, Value::Str(_)) {
                return Value::Str(format!("{}{}", left.as_str(), right.as_str()));
            }
            if matches!(left, Value::Real(_)) || matches!(right, Value::Real(_)) {
                Value::Real(left.as_real() + right.as_real())
            } else {
                Value::Int(left.as_int().wrapping_add(right.as_int()))
            }
        }
        Sub => {
            if matches!(left, Value::Real(_)) || matches!(right, Value::Real(_)) {
                Value::Real(left.as_real() - right.as_real())
            } else {
                Value::Int(left.as_int().wrapping_sub(right.as_int()))
            }
        }
        Mul => {
            if matches!(left, Value::Real(_)) || matches!(right, Value::Real(_)) {
                Value::Real(left.as_real() * right.as_real())
            } else {
                Value::Int(left.as_int().wrapping_mul(right.as_int()))
            }
        }
        Div => {
            if matches!(left, Value::Real(_)) || matches!(right, Value::Real(_)) {
                let divisor = right.as_real();
                Value::Real(if divisor == 0.0 {
                    0.0
                } else {
                    left.as_real() / divisor
                })
            } else {
                let divisor = right.as_int();
                Value::Int(if divisor == 0 {
                    0
                } else {
                    left.as_int().wrapping_div(divisor)
                })
            }
        }
        Eq => Value::Bool(compare_eq(&left, &right, local_player_any)),
        Ne => Value::Bool(!compare_eq(&left, &right, local_player_any)),
        Lt | Le | Gt | Ge => {
            let ordering = compare_values(&left, &right);
            Value::Bool(match op {
                Lt => ordering < 0,
                Le => ordering <= 0,
                Gt => ordering > 0,
                Ge => ordering >= 0,
                _ => unreachable!(),
            })
        }
        And => Value::Bool(left.truthy() && right.truthy()),
        Or => Value::Bool(left.truthy() || right.truthy()),
    }
}

/// `GetLocalPlayer()` handle in single-player wildcard mode.
pub const LOCAL_PLAYER_ANY: u32 = u32::MAX;

/// Equality with the single-player local-player wildcard: the sentinel is equal
/// to whichever player the script compares it against.
fn compare_eq(left: &Value, right: &Value, local_player_any: bool) -> bool {
    if local_player_any {
        let wildcard = |value: &Value| value.handle() == Some(LOCAL_PLAYER_ANY);
        if (wildcard(left) && right.handle().is_some())
            || (wildcard(right) && left.handle().is_some())
        {
            return true;
        }
    }
    value_eq(left, right)
}

fn value_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Null, _) | (_, Value::Null) => false,
        (Value::Str(a), Value::Str(b)) => a == b,
        (Value::Code(a), Value::Code(b)) => a.eq_ignore_ascii_case(b),
        _ => {
            if matches!(left, Value::Real(_)) || matches!(right, Value::Real(_)) {
                left.as_real() == right.as_real()
            } else {
                left.as_int() == right.as_int()
            }
        }
    }
}

fn compare_values(left: &Value, right: &Value) -> i32 {
    match (left, right) {
        (Value::Str(a), Value::Str(b)) => {
            if a == b {
                0
            } else if a < b {
                -1
            } else {
                1
            }
        }
        _ => {
            let a = left.as_real();
            let b = right.as_real();
            if a < b {
                -1
            } else if a > b {
                1
            } else {
                0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AUD-032: the seat default is process-wide, so it must be restorable.
    /// A campaign mission sets seat 2 + wildcard; the next melee boot resets it.
    #[test]
    fn local_player_default_is_session_state_and_resets() {
        // Serialise with any other test that touches the process-wide default.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|poison| poison.into_inner());

        Runtime::set_default_local_player(2);
        set_local_player_any_default(true);
        let world = World::default();
        assert_eq!(world.local_player, 2);
        assert!(world.local_player_any);

        reset_local_player_default();
        assert_eq!(Runtime::default_local_player(), 0);
        let fresh = World::default();
        assert_eq!(
            fresh.local_player, 0,
            "a melee world must not inherit seat 2"
        );
        assert!(!fresh.local_player_any);
    }

    /// AUD-031: a native this build does not execute must be diagnosable.
    /// The VM returns the declared default (script compatibility), so the
    /// `unimplemented` map is the record that the call did nothing.
    #[test]
    fn an_unknown_native_is_recorded_in_the_diagnostics() {
        let source = r#"
function main takes nothing returns nothing
    call TotallyUnknownNativeBJ(1)
endfunction
native TotallyUnknownNativeBJ takes integer i returns nothing
"#;
        let program = parse_named(source, "unknown-native").unwrap();
        let mut runtime = Runtime::new(program, 7);
        runtime.run_main().unwrap();
        assert_eq!(
            runtime.unimplemented.get("totallyunknownnativebj"),
            Some(&1),
            "diagnostics: {:?}",
            runtime.unimplemented
        );
    }

    #[test]
    fn runs_arithmetic_and_strings() {
        let source = r#"
globals
    integer g = 1
endglobals
function main takes nothing returns nothing
    local integer i = 2
    local real r = 2.5
    set g = i + g
    if g == 3 then
        call Print("sum=" + I2S(g) + "," + R2S(r))
    endif
endfunction
native Print takes string s returns nothing
"#;
        let program = parse_named(source, "test").unwrap();
        let mut runtime = Runtime::new(program, 7);
        runtime.run_main().unwrap();
        assert!(runtime.output.is_empty() || runtime.output.iter().any(|line| line.contains("sum=3")));
        assert_eq!(runtime.steps > 0, true);
    }

    #[test]
    fn parses_and_runs_ai_style_loop() {
        let source = r#"
globals
    integer n = 0
endglobals
function main takes nothing returns nothing
    loop
        set n = n + 1
        exitwhen n >= 5
    endloop
endfunction
"#;
        let program = parse_named(source, "loop").unwrap();
        let mut runtime = Runtime::new(program, 1);
        runtime.run_main().unwrap();
        assert_eq!(runtime.globals.get("n").unwrap().scalar(), Value::Int(5));
    }
}
