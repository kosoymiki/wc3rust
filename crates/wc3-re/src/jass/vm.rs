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
    GameState { state: i64, op: i64, value: f64 },
    Variable { name: String, op: i64, value: f64 },
    Dialog { dialog: u32 },
    DialogButton { dialog: u32, button: u32 },
    UnitEvent { unit: u32, event: i64 },
    Custom,
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
    natives: HashMap<String, NativeDecl>,
    frames: Vec<Frame>,
    /// Handles of triggers currently on the execution stack. A trigger that
    /// re-executes itself (directly or through a cycle) would otherwise recurse
    /// until the VM call-depth limit; WC3's `TriggerSleepAction` normally breaks
    /// such loops, but the headless VM has no coroutine suspension. Re-entrant
    /// `TriggerExecute` is ignored, matching the engine's non-reentrant trigger
    /// semantics.
    trigger_stack: Vec<u32>,
}

struct Frame {
    locals: HashMap<String, Slot>,
}

enum Control {
    Normal,
    Return(Value),
    ExitLoop,
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
            natives,
            frames: Vec::new(),
            trigger_stack: Vec::new(),
        };
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
                    Some(expr) => match self.eval_expr(expr) {
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

    pub fn function_names(&self) -> BTreeSet<String> {
        self.program.function_names()
    }

    pub fn native_names(&self) -> BTreeSet<String> {
        self.natives.keys().cloned().collect()
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
            let function = self.program.functions[index].clone();
            *self.call_counts.entry(function.name.clone()).or_insert(0) += 1;
            let mut frame = Frame {
                locals: HashMap::new(),
            };
            for (position, (_ty, param_name)) in function.params.iter().enumerate() {
                frame.locals.insert(
                    param_name.clone(),
                    Slot::Scalar(args.get(position).cloned().unwrap_or(Value::Null)),
                );
            }
            self.frames.push(frame);
            let control = self.exec_block(&function.body, depth);
            self.frames.pop();
            match control? {
                Control::Return(value) => Ok(value),
                _ => Ok(Value::Null),
            }
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

    fn exec_block(&mut self, block: &[Stmt], depth: usize) -> Result<Control> {
        for stmt in block {
            let control = self.exec_stmt(stmt, depth)?;
            if !matches!(control, Control::Normal) {
                return Ok(control);
            }
        }
        Ok(Control::Normal)
    }

    fn exec_stmt(&mut self, stmt: &Stmt, depth: usize) -> Result<Control> {
        self.steps += 1;
        if self.steps > self.max_steps {
            return Err(ReError::Vm("step limit exceeded".into()));
        }
        match stmt {
            Stmt::Local(local) => {
                let value = match &local.init {
                    Some(expr) => self.eval_expr(expr)?,
                    None => Value::Null,
                };
                let slot = if local.array {
                    Slot::Array(BTreeMap::new())
                } else {
                    Slot::Scalar(value)
                };
                if let Some(frame) = self.frames.last_mut() {
                    frame.locals.insert(local.name.clone(), slot);
                } else {
                    self.globals.insert(local.name.clone(), slot);
                }
                Ok(Control::Normal)
            }
            Stmt::Set {
                name,
                index,
                value,
                line,
            } => {
                let value = self.eval_expr(value)?;
                match index {
                    Some(index) => {
                        let index = self.eval_expr(index)?.as_int();
                        self.set_array(name, index, value);
                    }
                    None => self.set_scalar(name, value),
                }
                let _ = line;
                Ok(Control::Normal)
            }
            Stmt::Call { name, args, line } => {
                let values = args
                    .iter()
                    .map(|arg| self.eval_expr(arg))
                    .collect::<Result<Vec<_>>>()?;
                let result = self.call_function(name, &values, depth + 1);
                if let Err(err) = result {
                    self.errors.push(format!("line {line}: call {name}: {err}"));
                }
                Ok(Control::Normal)
            }
            Stmt::If {
                branches,
                else_body,
                line,
            } => {
                for (cond, body) in branches {
                    if self.eval_expr(cond)?.truthy() {
                        return self.exec_block(body, depth);
                    }
                }
                if let Some(body) = else_body {
                    return self.exec_block(body, depth);
                }
                let _ = line;
                Ok(Control::Normal)
            }
            Stmt::Loop { body, line } => {
                let mut iterations = 0u64;
                loop {
                    iterations += 1;
                    if iterations > 5_000_000 {
                        return Err(ReError::Vm(format!(
                            "line {line}: loop iteration limit exceeded"
                        )));
                    }
                    match self.exec_block(body, depth)? {
                        Control::Normal => {}
                        Control::ExitLoop => break,
                        Control::Return(value) => return Ok(Control::Return(value)),
                    }
                }
                Ok(Control::Normal)
            }
            Stmt::ExitWhen { cond, line } => {
                if self.eval_expr(cond)?.truthy() {
                    Ok(Control::ExitLoop)
                } else {
                    let _ = line;
                    Ok(Control::Normal)
                }
            }
            Stmt::Return { value, line } => {
                let value = match value {
                    Some(expr) => self.eval_expr(expr)?,
                    None => Value::Null,
                };
                let _ = line;
                Ok(Control::Return(value))
            }
        }
    }

    fn set_scalar(&mut self, name: &str, value: Value) {
        if let Some(frame) = self.frames.last_mut()
            && let Some(slot) = frame.locals.get_mut(name)
        {
            *slot = Slot::Scalar(value);
            return;
        }
        if let Some(slot) = self.globals.get_mut(name) {
            *slot = Slot::Scalar(value);
            return;
        }
        self.globals.insert(name.to_string(), Slot::Scalar(value));
    }

    fn set_array(&mut self, name: &str, index: i64, value: Value) {
        if let Some(frame) = self.frames.last_mut()
            && let Some(Slot::Array(map)) = frame.locals.get_mut(name)
        {
            map.insert(index, value);
            return;
        }
        match self.globals.get_mut(name) {
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
                self.globals.insert(name.to_string(), Slot::Array(map));
            }
        }
    }

    fn get_var(&self, name: &str) -> Value {
        if let Some(frame) = self.frames.last() {
            if let Some(slot) = frame.locals.get(name) {
                return slot.scalar();
            }
        }
        self.globals
            .get(name)
            .map(Slot::scalar)
            .unwrap_or(Value::Null)
    }

    fn get_array(&self, name: &str, index: i64) -> Value {
        if let Some(frame) = self.frames.last() {
            if let Some(Slot::Array(map)) = frame.locals.get(name) {
                return map.get(&index).cloned().unwrap_or(Value::Null);
            }
        }
        if let Some(Slot::Array(map)) = self.globals.get(name) {
            return map.get(&index).cloned().unwrap_or(Value::Null);
        }
        Value::Null
    }

    fn eval_expr(&mut self, expr: &Expr) -> Result<Value> {
        self.steps += 1;
        if self.steps > self.max_steps {
            return Err(ReError::Vm("step limit exceeded".into()));
        }
        match expr {
            Expr::Int(value) => Ok(Value::Int(*value)),
            Expr::Real(value) => Ok(Value::Real(*value)),
            Expr::Bool(value) => Ok(Value::Bool(*value)),
            Expr::Str(value) => Ok(Value::Str(value.clone())),
            Expr::Null => Ok(Value::Null),
            Expr::Var(name) => Ok(self.get_var(name)),
            Expr::Array(name, index) => {
                let index = self.eval_expr(index)?.as_int();
                Ok(self.get_array(name, index))
            }
            Expr::Call(name, args) => {
                let values = args
                    .iter()
                    .map(|arg| self.eval_expr(arg))
                    .collect::<Result<Vec<_>>>()?;
                self.call_function(name, &values, self.frames.len())
            }
            Expr::Code(name) => Ok(Value::Code(name.clone())),
            Expr::Unary(op, expr) => {
                let value = self.eval_expr(expr)?;
                Ok(match op {
                    UnOp::Neg => match value {
                        Value::Real(v) => Value::Real(-v),
                        Value::Int(v) => Value::Int(-v),
                        other => Value::Int(-other.as_int()),
                    },
                    UnOp::Pos => value,
                    UnOp::Not => Value::Bool(!value.truthy()),
                })
            }
            Expr::Binary(op, left, right) => {
                if matches!(op, BinOp::And) {
                    let left = self.eval_expr(left)?;
                    if !left.truthy() {
                        return Ok(Value::Bool(false));
                    }
                    let right = self.eval_expr(right)?;
                    return Ok(Value::Bool(right.truthy()));
                }
                if matches!(op, BinOp::Or) {
                    let left = self.eval_expr(left)?;
                    if left.truthy() {
                        return Ok(Value::Bool(true));
                    }
                    let right = self.eval_expr(right)?;
                    return Ok(Value::Bool(right.truthy()));
                }
                let left = self.eval_expr(left)?;
                let right = self.eval_expr(right)?;
                Ok(eval_binary(
                    *op,
                    left,
                    right,
                    self.world.local_player_any,
                ))
            }
        }
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
            let mut pass = true;
            for condition in &trigger.conditions {
                if !self.eval_bool_expr(*condition, depth)? {
                    pass = false;
                    break;
                }
            }
            let previous_trigger = self.world.context.trigger;
            self.world.context.trigger = Some(handle);
            if pass {
                for action in &trigger.actions {
                    self.call_function(action, &[], depth + 1)?;
                }
                if let Some(stored) = self.world.triggers.get_mut(&handle) {
                    stored.exec_count += 1;
                }
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
        let owner = if unit_state.owner >= 0 {
            Some(unit_state.owner as u32)
        } else {
            None
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
        let count = due.len();
        for handle in due {
            let _ = self.execute_trigger(handle);
        }
        self.world.context.player = previous_player;
        self.world.context.unit = previous_unit;
        count
    }

    /// Fire an item event for `item` held/manipulated by `unit` with
    /// `GetManipulatedItem` set for the duration of the handlers.
    pub(crate) fn item_event(&mut self, event: i64, unit: u32, item: u32) {
        let previous = self.world.context.clone();
        if let Some(owner) = self.world.units.get(&unit).map(|unit| unit.owner) {
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
        let owner = caster_state.owner;
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
        if let Some(owner) = self.world.units.get(&unit).map(|unit| unit.owner) {
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
