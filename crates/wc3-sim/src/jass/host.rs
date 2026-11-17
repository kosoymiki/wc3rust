//! Script-side handle tables and world state that has no simulation
//! counterpart (regions, sounds, camera setups, hashtables, player slots).
//!
//! Unit handles stay bound to real [`crate::Sim`] units through
//! [`crate::SimJass`], so a script-created unit is a simulation unit. Every
//! other handle family lives here as explicit state: a `group` really holds its
//! members, a `location` really holds its coordinates, a `hashtable` really
//! stores values. Handles that the simulation cannot model (an `image`, a
//! `trackable`) still get an id so `GetHandleId`, equality and `null` checks
//! behave, which is what the map scripts actually rely on.

use std::collections::BTreeMap;

use crate::Vec2;
use crate::jass_api::SimJass;

use super::value::{Handle, HandleKind, Value};

/// JASS Bools/Integers/Reals/strings are all stored per handle in a hashtable.
pub type HashtableCells = BTreeMap<(i64, i64), Value>;

/// Trigger registration: what makes a trigger's actions run.
#[derive(Debug, Clone)]
pub enum RegEvent {
    /// `TriggerRegisterTimerEvent` — fires every `period_ticks` ticks.
    TimerEvent {
        period: f32,
        period_ticks: u32,
        periodic: bool,
        elapsed: u32,
    },
    /// `TriggerRegisterTimerExpireEvent`.
    TimerExpire { timer: u32 },
    /// `TriggerRegisterUnitEvent` — `event` is an `EVENT_UNIT_*` constant.
    UnitEvent { unit: u32, event: i64 },
    /// `TriggerRegisterAnyUnitEventBJ` / `TriggerRegisterPlayerUnitEvent`.
    AnyUnitEvent { event: i64, player: Option<u8> },
    /// `TriggerRegisterPlayerEvent` — `event` is an `EVENT_PLAYER_*` constant.
    PlayerEvent { player: u8, event: i64 },
    /// `TriggerRegisterEnterRegion` / `TriggerRegisterLeaveRegion`.
    RegionEnter { region: u32 },
    RegionLeave { region: u32 },
    /// `TriggerRegisterDeathEvent` on a destructable.
    DestructableDeath { destructable: Option<u32> },
    /// `TriggerRegisterPlayerChatEvent`.
    Chat { player: u8, message: String, exact: bool },
    /// `TriggerRegisterDialogEvent` — fired when a button is clicked.
    DialogEvent { dialog: u32 },
    /// `TriggerRegisterUnitInRange` — fired when a unit enters `range` of a unit.
    UnitInRange { unit: u32, range: f32, filter: Option<u32> },
    /// `TriggerRegisterPlayerAllianceChange` — fired when `player`'s alliance
    /// state for `alliance` (an `ALLIANCE_*` constant) changes.
    PlayerAllianceChange { player: u8, alliance: i32 },
    /// `TriggerRegisterPlayerStateEvent` — fired when `player`'s `state`
    /// crosses the `op`/`value` comparison, checked once per tick.
    PlayerStateEvent {
        player: u8,
        state: i32,
        op: i32,
        value: f32,
    },
}

/// A script trigger.
#[derive(Debug, Clone, Default)]
pub struct TriggerState {
    pub enabled: bool,
    pub actions: Vec<u32>,
    /// Condition function indices; all must return true for the actions to run.
    pub conditions: Vec<u32>,
    pub events: Vec<RegEvent>,
    /// Set while the trigger's own actions run (re-entrancy guard, like the
    /// engine's `trigger->running`).
    pub running: bool,
}

impl TriggerState {
    pub fn new() -> Self {
        Self {
            enabled: true,
            ..Self::default()
        }
    }
}

/// A script timer.
#[derive(Debug, Clone, Default)]
pub struct TimerState {
    pub timeout: f32,
    /// Remaining time in ticks; the engine quantizes timers to frames.
    pub ticks_remaining: u32,
    /// One period in ticks (`TimerStart` timeout).
    pub period_ticks: u32,
    pub periodic: bool,
    pub running: bool,
    pub callback: Option<u32>,
    /// Handle of the timer, needed by `GetExpiredTimer` in the callback.
    pub handle: u32,
}

impl TimerState {
    /// Remaining seconds, from the tick countdown.
    pub fn remaining_seconds(&self) -> f32 {
        self.ticks_remaining as f32 / crate::TICKS_PER_SECOND as f32
    }
}

/// Mutable per-unit script state that the simulation does not own.
#[derive(Debug, Clone)]
pub struct UnitMeta {
    pub acquire_range: f32,
    pub hidden: bool,
    pub color: i32,
    pub vertex_color: (i32, i32, i32, i32),
    pub scale: f32,
    pub pathing: bool,
    pub invulnerable: bool,
    pub time_scale: f32,
    pub move_speed: f32,
    pub facing: f32,
    pub facing_timed: Option<(f32, f32)>,
    pub user_data: i32,
    pub custom_value: i32,
    /// Seconds until `UnitApplyTimedLife` removes the unit (engine buff).
    pub timed_life: Option<f32>,
    pub paused: bool,
    pub animation: String,
    /// Gold/lumber left in a resource carrying unit (`SetResourceAmount`).
    pub resource_amount: i32,
    pub max_life_override: Option<i32>,
    pub max_mana_override: Option<i32>,
    pub selected: bool,
    pub name_override: Option<String>,
    /// Units created by `CreateUnit` are marked so `CreateUnit`-only bookkeeping
    /// (e.g. summon expiry) can tell them apart.
    pub summoned: bool,
    pub abilities: Vec<i64>,
    /// `SuspendHeroXP` state.
    pub xp_suspended: bool,
}

impl Default for UnitMeta {
    fn default() -> Self {
        Self {
            acquire_range: 0.0,
            hidden: false,
            color: -1,
            vertex_color: (255, 255, 255, 255),
            scale: 1.0,
            pathing: true,
            invulnerable: false,
            time_scale: 1.0,
            move_speed: 0.0,
            facing: 0.0,
            facing_timed: None,
            user_data: 0,
            custom_value: 0,
            timed_life: None,
            paused: false,
            animation: String::new(),
            resource_amount: 0,
            max_life_override: None,
            max_mana_override: None,
            selected: false,
            name_override: None,
            summoned: false,
            abilities: Vec::new(),
            xp_suspended: false,
        }
    }
}

/// Player slot state (`SetPlayerTeam`, `SetPlayerColor`, tech limits, …).
#[derive(Debug, Clone)]
pub struct PlayerState {
    pub index: u8,
    pub race: i32,
    pub controller: i32,
    pub color: i32,
    pub team: i32,
    pub start_location: i32,
    pub selecting: bool,
    pub active: bool,
    /// `PLAYER_STATE_*` → value (`GetPlayerState`/`SetPlayerState`).
    pub states: BTreeMap<i32, i32>,
    /// `PLAYER_STATE_RESOURCE_GOLD`/`LUMBER` mirror the simulation for the two
    /// simulated players and are kept here for the rest.
    pub tech_max: BTreeMap<(i64, i64), i32>,
    pub tech_researched: BTreeMap<i64, i32>,
    pub ability_available: BTreeMap<i64, bool>,
    pub name: String,
}

impl PlayerState {
    pub fn new(index: u8) -> Self {
        Self {
            index,
            // MAP_CONTROL_USER = 2, RACE_PREF_* default: any race.
            race: 0,
            controller: 2,
            color: index as i32 % 24,
            team: index as i32,
            start_location: index as i32,
            selecting: true,
            active: false,
            states: BTreeMap::new(),
            tech_max: BTreeMap::new(),
            tech_researched: BTreeMap::new(),
            ability_available: BTreeMap::new(),
            name: format!("Player {}", index as i32 + 1),
        }
    }
}

/// Cinematic filter state: every `SetCineFilter*` field, so a later
/// `DisplayCineFilter` can be replayed by a renderer.
#[derive(Debug, Clone, Default)]
pub struct CineFilter {
    pub texture: String,
    pub start_uv: (f32, f32),
    pub end_uv: (f32, f32),
    pub start_color: (i32, i32, i32, i32),
    pub end_color: (i32, i32, i32, i32),
    pub duration: f32,
    pub blend_mode: i32,
    pub tex_map_flags: i32,
    pub displayed: bool,
}

/// A camera setup (`CreateCameraSetup`, `CameraSetupSetField`).
#[derive(Debug, Clone, Default)]
pub struct CameraSetup {
    pub fields: BTreeMap<i64, Value>,
}

/// The script-visible camera state.
///
/// Warcraft III camera natives are not a projection: the script sets a target
/// point, a distance and a set of fields, and the renderer derives the view.
/// The host keeps exactly that state so `GetCameraTargetPositionX/Y/Z`,
/// `GetCameraField` and `CameraSetupGetField` answer from what the script set.
#[derive(Debug, Clone)]
pub struct CameraState {
    /// Current target point (the point the camera looks at).
    pub target: Vec2,
    /// `CAMERA_FIELD_*` values, keyed by the `camerafield` constant.
    pub fields: BTreeMap<i32, f32>,
    /// `SetCameraTargetController` unit, while it is active.
    pub target_controller: Option<u32>,
    /// `CameraSetupApply*` setup that was last applied.
    pub applied_setup: Option<u32>,
}

impl Default for CameraState {
    fn default() -> Self {
        let mut fields = BTreeMap::new();
        // The engine's default gameplay camera: 1650 distance, 70° field of
        // view, 304° rotation, 304° angle of attack (`common.j` defaults are
        // not stated; these are the values the shipped maps read back before
        // they set anything — observed in Human01's `CreateCameras`).
        fields.insert(0, 1650.0);
        fields.insert(2, 304.0);
        fields.insert(3, 70.0);
        fields.insert(5, 90.0);
        Self {
            target: Vec2::ZERO,
            fields,
            target_controller: None,
            applied_setup: None,
        }
    }
}

/// One cinematic step the script performed.
///
/// These are not decorative counters: a mission's cutscene is exactly the
/// ordered list of camera moves, transmissions and scene text its script
/// issues, so the runtime records them for the HUD/cutscene layer and for
/// headless tests.
#[derive(Debug, Clone, PartialEq)]
pub enum CinematicEvent {
    /// `SetCinematicScene` (a transmission or a scene title card).
    Scene {
        portrait_unit: i32,
        color: i32,
        speaker: String,
        text: String,
        scene_duration: f32,
        voice_duration: f32,
        tick: f64,
    },
    /// `TransmissionFromUnitWithNameBJ` reached `SetCinematicScene` with a
    /// portrait unit: a spoken line attributed to a unit.
    Transmission {
        unit: Option<u32>,
        speaker: String,
        message: String,
        duration: f32,
        tick: f64,
    },
    /// `EndCinematicScene`.
    SceneEnd { tick: f64 },
    /// `CameraSetupApply*` (a scripted camera move).
    CameraApply {
        setup: u32,
        duration: f32,
        tick: f64,
    },
    /// `SetCameraField` / `CameraSetupSetField`.
    CameraField {
        field: i32,
        value: f32,
        duration: f32,
        tick: f64,
    },
    /// `SetCameraPosition` / `PanCameraTo*` / `SetCameraRotateMode`.
    CameraPosition {
        x: f32,
        y: f32,
        duration: f32,
        tick: f64,
    },
    /// `SetCameraTargetController` / `SetCameraOrientController`.
    CameraController {
        unit: Option<u32>,
        x: f32,
        y: f32,
        tick: f64,
    },
    /// `ResetToGameCamera` / `StopCamera`.
    CameraReset { tick: f64 },
    /// `ShowInterface` / `EnableUserControl` / `EnableOcclusion`.
    Interface {
        interface_shown: bool,
        user_control: bool,
        occlusion: bool,
        tick: f64,
    },
    /// `PlayCinematic` / `PlayModelCinematic`.
    Movie { name: String, tick: f64 },
    /// `DisplayCineFilter`.
    Filter {
        displayed: bool,
        texture: String,
        tick: f64,
    },
}

impl CinematicEvent {
    /// A one-line description for logs and test output.
    pub fn describe(&self) -> String {
        match self {
            Self::Scene { speaker, text, tick, .. } => {
                format!("scene[{tick:.2}] {speaker}: {text}")
            }
            Self::Transmission { speaker, message, tick, .. } => {
                format!("transmission[{tick:.2}] {speaker}: {message}")
            }
            Self::SceneEnd { tick } => format!("scene-end[{tick:.2}]"),
            Self::CameraApply { setup, duration, tick } => {
                format!("camera-apply[{tick:.2}] setup={setup} duration={duration:.2}")
            }
            Self::CameraField { field, value, tick, .. } => {
                format!("camera-field[{tick:.2}] field={field} value={value:.2}")
            }
            Self::CameraPosition { x, y, tick, .. } => {
                format!("camera-position[{tick:.2}] {x:.0},{y:.0}")
            }
            Self::CameraController { x, y, tick, .. } => {
                format!("camera-controller[{tick:.2}] {x:.0},{y:.0}")
            }
            Self::CameraReset { tick } => format!("camera-reset[{tick:.2}]"),
            Self::Interface { interface_shown, user_control, tick, .. } => format!(
                "interface[{tick:.2}] shown={interface_shown} user_control={user_control}"
            ),
            Self::Movie { name, tick } => format!("movie[{tick:.2}] {name}"),
            Self::Filter { displayed, texture, tick } => {
                format!("filter[{tick:.2}] displayed={displayed} texture={texture}")
            }
        }
    }

    /// True for events a cutscene viewer must replay (camera or dialogue).
    pub fn is_cutscene(&self) -> bool {
        matches!(
            self,
            Self::Scene { .. }
                | Self::Transmission { .. }
                | Self::CameraApply { .. }
                | Self::CameraPosition { .. }
                | Self::CameraController { .. }
        )
    }
}

/// Quest state the script can toggle but the save format does not carry yet
/// (`QuestSetFailed`, `QuestSetEnabled`). Completed/discovered stay on the
/// quest object itself; these two live here so adding them cannot change the
/// v4 save layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestFlags {
    pub enabled: bool,
    pub failed: bool,
}

impl Default for QuestFlags {
    fn default() -> Self {
        Self {
            enabled: true,
            failed: false,
        }
    }
}

/// Hard cap on the per-run cinematic log. A long campaign session records
/// thousands of camera fields; the log is for playback validation and tests,
/// not an unbounded transcript.
pub const CINEMATIC_LOG_LIMIT: usize = 8192;

/// A sound handle. `StartSound` on a headless simulation cannot emit audio, so
/// the handle records what was asked (path, volume, pitch, position) and the
/// engine-visible queries (`GetSoundDuration` after `SetSoundDuration`) answer
/// from that state.
#[derive(Debug, Clone, Default)]
pub struct SoundState {
    pub path: String,
    pub duration_ms: i32,
    pub volume: i32,
    pub pitch: f32,
    pub position: Option<Vec2>,
    pub unit: Option<u32>,
    pub playing: bool,
    pub channel: i32,
}

/// Generic non-unit object payload.
#[derive(Debug, Clone)]
pub enum Object {
    Group(Vec<u32>),
    Location(Vec2),
    Rect { min: Vec2, max: Vec2 },
    Region { rects: Vec<u32> },
    Timer(TimerState),
    Trigger(TriggerState),
    Sound(SoundState),
    Force(Vec<u8>),
    /// `Condition()`/`Filter()`: a function index called for a boolean result.
    Boolexpr { func: Option<u32>, negated: bool },
    TriggerCondition { func: Option<u32>, trigger: u32 },
    CameraSetup(CameraSetup),
    FogModifier { rect: u32, enabled: bool },
    Dialog { buttons: Vec<u32> },
    Button { text: String, hotkey: i32 },
    Leaderboard { rows: BTreeMap<i32, (String, i32, String)> },
    Multiboard { rows: BTreeMap<i32, (String, i32)> },
    Quest {
        title: String,
        description: String,
        icon: String,
        required: bool,
        discovered: bool,
        completed: bool,
        items: Vec<u32>,
    },
    Hashtable(HashtableCells),
    ItemPool { items: Vec<i64>, mode: i32 },
    UnitPool { units: Vec<i64>, mode: i32 },
    TextTag { text: String, pos: Vec2, lifespan: f32 },
    Trackable { pos: Vec2 },
    WeatherEffect { rawcode: i64, enabled: bool },
    TerrainDeformation { radius: f32, duration: f32 },
    GameEvent { id: i64 },
    StartLocation(i32),
    TimerDialog { timer: u32, title: String },
    Ability { id: i64, level: i32 },
    Buff { rawcode: i64 },
    Image { pos: Vec2, file: String },
    Ubersplat { pos: Vec2, rawcode: i64 },
    Lightning { code: String },
    /// A handle with identity only.
    Opaque,
}

impl Object {
    pub fn kind(&self) -> HandleKind {
        match self {
            Self::Group(_) => HandleKind::Group,
            Self::Location(_) => HandleKind::Location,
            Self::Rect { .. } => HandleKind::Rect,
            Self::Region { .. } => HandleKind::Region,
            Self::Timer(_) => HandleKind::Timer,
            Self::Trigger(_) => HandleKind::Trigger,
            Self::Sound(_) => HandleKind::Sound,
            Self::Force(_) => HandleKind::Force,
            Self::Boolexpr { .. } => HandleKind::Boolexpr,
            Self::TriggerCondition { .. } => HandleKind::TriggerCondition,
            Self::CameraSetup(_) => HandleKind::CameraSetup,
            Self::FogModifier { .. } => HandleKind::FogModifier,
            Self::Dialog { .. } => HandleKind::Dialog,
            Self::Button { .. } => HandleKind::Button,
            Self::Leaderboard { .. } => HandleKind::Leaderboard,
            Self::Multiboard { .. } => HandleKind::Multiboard,
            Self::Quest { .. } => HandleKind::Quest,
            Self::Hashtable(_) => HandleKind::Hashtable,
            Self::ItemPool { .. } => HandleKind::ItemPool,
            Self::UnitPool { .. } => HandleKind::UnitPool,
            Self::TextTag { .. } => HandleKind::TextTag,
            Self::Trackable { .. } => HandleKind::Trackable,
            Self::WeatherEffect { .. } => HandleKind::WeatherEffect,
            Self::TerrainDeformation { .. } => HandleKind::TerrainDeformation,
            Self::GameEvent { .. } => HandleKind::GameEvent,
            Self::StartLocation(_) => HandleKind::StartLocation,
            Self::TimerDialog { .. } => HandleKind::TimerDialog,
            Self::Ability { .. } => HandleKind::Ability,
            Self::Buff { .. } => HandleKind::Buff,
            Self::Image { .. } => HandleKind::Image,
            Self::Ubersplat { .. } => HandleKind::Ubersplat,
            Self::Lightning { .. } => HandleKind::Lightning,
            Self::Opaque => HandleKind::Misc,
        }
    }
}

/// Player slots in the 1.29+ layout: 24 playable plus four neutral slots.
pub const PLAYER_SLOTS: usize = 28;
/// `PLAYER_NEUTRAL_AGGRESSIVE` — the hostile/creep owner. The 1.24 header puts
/// it at 12 with 12 playable slots; the 24-player layout shifts the four
/// neutral slots to 24..27, which is why `bj_MAX_PLAYER_SLOTS` is 28 (inferred
/// from the 1.24 values plus the 24-slot expansion, not measured).
pub const PLAYER_NEUTRAL_AGGRESSIVE: i32 = 24;
/// `bj_PLAYER_NEUTRAL_VICTIM`.
pub const PLAYER_NEUTRAL_VICTIM: i32 = 25;
/// `bj_PLAYER_NEUTRAL_EXTRA`.
pub const PLAYER_NEUTRAL_EXTRA: i32 = 26;
/// `PLAYER_NEUTRAL_PASSIVE`.
pub const PLAYER_NEUTRAL_PASSIVE: i32 = 27;

/// A start location defined by the map script (`DefineStartLocation`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StartLocation {
    pub index: i32,
    pub pos: Vec2,
}

/// World-level state owned by the script host.
pub struct Host {
    /// Unit handle table bound to the simulation.
    pub units: SimJass,
    /// Script-only per-unit state, keyed by unit handle id.
    pub unit_meta: BTreeMap<u32, UnitMeta>,
    /// Non-unit handles.
    pub objects: BTreeMap<u32, Object>,
    /// Item handle id → simulation item id.
    pub item_binding: BTreeMap<u32, crate::ItemId>,
    /// Destructable handle id → simulation destructable id.
    pub destructable_binding: BTreeMap<u32, crate::DestructableId>,
    next_object: u32,
    /// Player slots 0..28: 0..23 are playable, 24..27 are the neutral slots
    /// (`PLAYER_NEUTRAL_AGGRESSIVE` 24, `bj_PLAYER_NEUTRAL_VICTIM` 25,
    /// `bj_PLAYER_NEUTRAL_EXTRA` 26, `PLAYER_NEUTRAL_PASSIVE` 27).
    pub players: Vec<PlayerState>,
    /// `DefineStartLocation` list.
    pub start_locations: Vec<StartLocation>,
    /// Map name/description set by the script (`SetMapName`).
    pub map_name: String,
    pub map_description: String,
    /// `SetTeams` / `SetPlayers` / `SetGamePlacement` lobby state.
    pub teams: i32,
    pub player_count: i32,
    pub placement: i32,
    /// `SetGameTypeSupported(gameType, flag)`.
    pub game_types: BTreeMap<i32, bool>,
    /// `SetFloatGameState` values.
    pub float_states: BTreeMap<i32, Value>,
    pub game_difficulty: i32,
    /// `SetPlayerAlliance` overrides, keyed `(from << 8 | to, ALLIANCE_*)`.
    pub alliances: BTreeMap<(u32, i32), u8>,
    /// Orders issued through the script API, in order: `(unit handle, order)`.
    pub order_log: Vec<(u32, String)>,
    /// Text shown by `DisplayTextToPlayer`/`DisplayTimedTextToPlayer`.
    pub display_log: Vec<(Option<u8>, String)>,
    /// `SetStartLocPrio` (start index, slot) → importance.
    pub start_prio: BTreeMap<(i32, i32), i32>,
    /// `SetStartLocPrioCount` per start index.
    pub start_prio_count: BTreeMap<i32, i32>,
    pub sky_model: String,
    pub water_color: (i32, i32, i32, i32),
    /// `SetTerrainFog`/`SetTerrainFogEx` state.
    pub terrain_fog: Option<(String, f32, (i32, i32, i32), f32)>,
    /// Sounds marked `KillSoundWhenDone`.
    pub sound_kill_list: std::collections::BTreeSet<u32>,
    /// `Preloader` paths, recorded in call order.
    pub preloaded: Vec<String>,
    pub all_item_type_slots: i32,
    pub all_unit_type_slots: i32,
    /// `FogEnable` / `FogMaskEnable` state.
    pub fog_enabled: bool,
    pub fog_mask_enabled: bool,
    /// `EnableUserUI` state.
    pub user_ui_enabled: bool,
    /// `SetTimeOfDayScale` value.
    pub time_of_day_scale: f32,
    /// Cinematic filter state (`DisplayCineFilter`, `SetCineFilter*`).
    pub cine_filter: CineFilter,
    /// `GetIntegerGameState` values, keyed by `igamestate` constant.
    pub integer_game_states: BTreeMap<i32, i32>,
    /// Quest item id → (description, completed).
    pub quest_items: BTreeMap<u32, (String, bool)>,
    /// `SetDayNightModels` / `NewSoundEnvironment` / `SetMapMusic`: recorded so
    /// the renderer/audio layer can consume them.
    pub day_night_models: (String, String),
    pub sound_environment: String,
    pub map_music: String,
    /// Last `DisplayTextToPlayer` target for the output log.
    pub last_text_target: Option<u8>,
    /// Camera bounds set by the script.
    pub camera_bounds: Option<(Vec2, Vec2)>,
    /// Selection from `SelectUnit`/`ClearSelection`.
    pub selection: Vec<u32>,
    /// `TriggerEvaluate`-visible event context, propagating nested triggers.
    pub event_stack: Vec<super::vm::EventContext>,
    /// Current `bj_lastCreatedUnit` etc. are ordinary globals; this records the
    /// last unit created for `GetLastCreatedUnit`-style BJ helpers.
    pub last_created_group: Option<u32>,
    /// Units whose creation happened inside the current tick, in order.
    pub created_units: Vec<u32>,
    /// Script-visible camera state (`SetCameraField`, `PanCameraTo`, ...).
    pub camera: CameraState,
    /// Ordered cinematic steps the script performed this run.
    pub cinematic_events: Vec<CinematicEvent>,
    /// Cinematic steps dropped after [`CINEMATIC_LOG_LIMIT`] was reached; kept
    /// so a truncated log never looks like a complete one.
    pub cinematic_events_dropped: u64,
    /// `ShowInterface` state (false during a cutscene).
    pub interface_shown: bool,
    /// `EnableUserControl` state.
    pub user_control_enabled: bool,
    /// `EnableOcclusion` state.
    pub occlusion_enabled: bool,
    /// `ForceCinematicSubtitles` state.
    pub cinematic_subtitles: bool,
    /// `SetCinematicAudio` state.
    pub cinematic_audio: bool,
    /// `QuestSetEnabled`/`QuestSetFailed` state per quest handle.
    pub quest_flags: BTreeMap<u32, QuestFlags>,
    /// Script time of the last `FlashQuestDialogButton`/`ForceQuestDialogUpdate`.
    pub quest_flash: f64,
    /// Destination of the last point order (`GetOrderPointX/Y/Loc`).
    pub last_order_point: Vec2,
    /// Target widget of the last target order (`GetOrderTarget*`).
    pub last_order_target: Option<Handle>,
    /// `SetCameraQuickPosition` marker (shown on the minimap).
    pub quick_position: Vec2,
    /// Slot whose screen the script drives (`GetLocalPlayer`).
    ///
    /// A campaign mission puts the human in the slot the map's `config`
    /// marks `MAP_CONTROL_USER` — Human01 uses Player(1) — so the local slot
    /// cannot be a constant 0 without breaking every `if GetLocalPlayer() ==
    /// player` block in `Blizzard.j` (camera applies, transmissions). The slot
    /// is discovered from `SetPlayerController`; `None` means "not declared,
    /// fall back to slot 0".
    pub local_player: Option<u8>,
    /// `war3map.wts` + `GlobalStrings.fdf` table behind `GetLocalizedString`.
    pub strings: super::locale::LocalizedStrings,
    /// The map's `war3map.w3i` `MapFlags` word, when a header was loaded.
    /// `None` (no header) answers every `IsMapFlagSet` with `false`.
    pub map_flags: Option<u32>,
    /// `mapflag` values `IsMapFlagSet` was asked about that are not engine
    /// bits, recorded so the gap is visible instead of silently `false`.
    pub unknown_map_flags: std::collections::BTreeSet<i32>,
    /// Per-`(player, ALLIANCE_*)` epoch, bumped by `SetPlayerAlliance`; a
    /// `TriggerRegisterPlayerAllianceChange` registration fires when the epoch
    /// it last saw moves.
    pub alliance_epoch: BTreeMap<(u8, i32), u64>,
    next_alliance_epoch: u64,
    /// Epoch already delivered to a registration, keyed
    /// `(trigger, player, alliance)`.
    pub alliance_watch: BTreeMap<(u32, u8, i32), u64>,
    /// Last result of a `TriggerRegisterPlayerStateEvent` comparison, keyed
    /// `(trigger, player, state, op, value bits)`. The event fires on the
    /// false → true crossing, so a registration whose comparison already holds
    /// at registration time does not fire until the state leaves and re-enters.
    pub state_watch: BTreeMap<(u32, u8, i32, i32, u32), bool>,
}

impl Default for Host {
    fn default() -> Self {
        Self::new()
    }
}

impl Host {
    pub fn new() -> Self {
        Self {
            units: SimJass::new(),
            unit_meta: BTreeMap::new(),
            objects: BTreeMap::new(),
            item_binding: BTreeMap::new(),
            destructable_binding: BTreeMap::new(),
            next_object: 0x4000_0000,
            players: (0..PLAYER_SLOTS).map(|index| PlayerState::new(index as u8)).collect(),
            start_locations: Vec::new(),
            map_name: String::new(),
            map_description: String::new(),
            teams: 0,
            player_count: 0,
            placement: 0,
            game_types: BTreeMap::new(),
            float_states: BTreeMap::new(),
            game_difficulty: 2,
            alliances: BTreeMap::new(),
            order_log: Vec::new(),
            display_log: Vec::new(),
            start_prio: BTreeMap::new(),
            start_prio_count: BTreeMap::new(),
            sky_model: String::new(),
            water_color: (255, 255, 255, 255),
            terrain_fog: None,
            sound_kill_list: std::collections::BTreeSet::new(),
            preloaded: Vec::new(),
            all_item_type_slots: 0,
            all_unit_type_slots: 0,
            fog_enabled: true,
            fog_mask_enabled: true,
            user_ui_enabled: true,
            time_of_day_scale: 1.0,
            cine_filter: CineFilter::default(),
            quest_items: BTreeMap::new(),
            integer_game_states: BTreeMap::new(),
            day_night_models: (String::new(), String::new()),
            sound_environment: String::new(),
            map_music: String::new(),
            last_text_target: None,
            camera_bounds: None,
            selection: Vec::new(),
            event_stack: Vec::new(),
            last_created_group: None,
            created_units: Vec::new(),
            camera: CameraState::default(),
            cinematic_events: Vec::new(),
            cinematic_events_dropped: 0,
            interface_shown: true,
            user_control_enabled: true,
            occlusion_enabled: true,
            cinematic_subtitles: false,
            cinematic_audio: false,
            quest_flags: BTreeMap::new(),
            quest_flash: 0.0,
            last_order_point: Vec2::ZERO,
            last_order_target: None,
            quick_position: Vec2::ZERO,
            local_player: None,
            strings: super::locale::LocalizedStrings::new(),
            map_flags: None,
            unknown_map_flags: std::collections::BTreeSet::new(),
            alliance_epoch: BTreeMap::new(),
            next_alliance_epoch: 0,
            alliance_watch: BTreeMap::new(),
            state_watch: BTreeMap::new(),
        }
    }

    /// Records that `player`'s alliance state for `alliance` changed and
    /// returns the new epoch. The epoch is per seat, because the engine raises
    /// `EVENT_PLAYER_ALLIANCE_CHANGED` for either side of an agreement.
    pub fn bump_alliance_seat(&mut self, player: u8, alliance: i32) -> u64 {
        self.next_alliance_epoch += 1;
        let epoch = self.next_alliance_epoch;
        self.alliance_epoch.insert((player, alliance), epoch);
        epoch
    }

    /// Effective `GetPlayerAlliance(from, to, kind)`: the script's explicit
    /// override when one exists, the melee default (same team → allied)
    /// otherwise. One definition shared by the query native and the change
    /// detection, so a trigger can never fire for a value the script cannot
    /// observe.
    pub fn effective_alliance(&self, from: u8, to: u8, kind: i32) -> bool {
        if let Some(flag) = self
            .alliances
            .get(&((from as u32) << 8 | to as u32, kind))
        {
            return *flag != 0;
        }
        self.players
            .get(from as usize)
            .zip(self.players.get(to as usize))
            .is_some_and(|(a, b)| a.team == b.team)
    }

    /// Value of a `MAP_FLAG_*` bit of the loaded header, or `None` when the
    /// value is not an engine map-flag bit (the caller logs the gap).
    pub fn map_flag(&self, flag: i32) -> Option<bool> {
        if !super::locale::is_known_map_flag(flag) {
            return None;
        }
        Some(self.map_flags.is_some_and(|word| word & flag as u32 != 0))
    }

    /// Records an `IsMapFlagSet` call with a value that is not an engine bit.
    /// One entry per distinct value; the caller keeps answering `false`.
    pub fn note_unknown_map_flag(&mut self, flag: i32) {
        self.unknown_map_flags.insert(flag);
    }

    /// Appends a cinematic step, bounded by [`CINEMATIC_LOG_LIMIT`].
    ///
    /// A cutscene-heavy mission issues tens of thousands of camera-field
    /// calls; the log is a replay/test record, so it stops growing at the cap
    /// and counts the drops instead of silently discarding them.
    pub fn record_cinematic(&mut self, event: CinematicEvent) {
        if self.cinematic_events.len() >= CINEMATIC_LOG_LIMIT {
            self.cinematic_events_dropped += 1;
            return;
        }
        self.cinematic_events.push(event);
    }

    /// Cutscene steps only (camera moves, transmissions, scene text).
    pub fn cutscene_events(&self) -> impl Iterator<Item = &CinematicEvent> {
        self.cinematic_events.iter().filter(|event| event.is_cutscene())
    }

    /// True while the script holds the player in cinematic mode (interface
    /// hidden or user control disabled).
    pub fn cinematic_mode(&self) -> bool {
        !self.interface_shown || !self.user_control_enabled
    }

    fn alloc_id(&mut self) -> u32 {
        let id = self.next_object;
        self.next_object = self.next_object.wrapping_add(1);
        id
    }

    /// Next id the object allocator will hand out. Save/load persists it so a
    /// restored world cannot re-issue a handle that a script still holds.
    pub(crate) fn next_object_id(&self) -> u32 {
        self.next_object
    }

    /// Restores the object allocator after a save/load (`next_object` is the
    /// only private host field; every other one is public state).
    pub(crate) fn set_next_object_id(&mut self, next: u32) {
        self.next_object = next;
    }

    /// Creates a non-unit object and returns its handle.
    pub fn create(&mut self, object: Object) -> Handle {
        let id = self.alloc_id();
        let kind = object.kind();
        self.objects.insert(id, object);
        Handle::new(kind, id)
    }

    /// Creates a handle with an explicit kind. Item/destructable handles map to
    /// simulation ids recorded in `item_binding`/`destructable_binding`, so the
    /// generic object payload stays opaque.
    pub fn create_tagged(&mut self, kind: HandleKind, object: Object) -> Handle {
        let id = self.alloc_id();
        self.objects.insert(id, object);
        Handle::new(kind, id)
    }

    /// Creates an object from an explicit handle id (used when a handle must be
    /// pre-registered, e.g. a player slot).
    pub fn insert_at(&mut self, id: u32, object: Object) -> Handle {
        let kind = object.kind();
        self.objects.insert(id, object);
        Handle::new(kind, id)
    }

    pub fn object(&self, handle: Handle) -> Option<&Object> {
        if handle.kind == HandleKind::Unit {
            return None;
        }
        self.objects.get(&handle.id)
    }

    pub fn object_mut(&mut self, handle: Handle) -> Option<&mut Object> {
        if handle.kind == HandleKind::Unit {
            return None;
        }
        self.objects.get_mut(&handle.id)
    }

    pub fn trigger(&self, handle: Handle) -> Option<&TriggerState> {
        match self.object(handle) {
            Some(Object::Trigger(state)) => Some(state),
            _ => None,
        }
    }

    pub fn trigger_mut(&mut self, handle: Handle) -> Option<&mut TriggerState> {
        match self.object_mut(handle) {
            Some(Object::Trigger(state)) => Some(state),
            _ => None,
        }
    }

    pub fn timer(&self, handle: Handle) -> Option<&TimerState> {
        match self.object(handle) {
            Some(Object::Timer(state)) => Some(state),
            _ => None,
        }
    }

    pub fn timer_mut(&mut self, handle: Handle) -> Option<&mut TimerState> {
        match self.object_mut(handle) {
            Some(Object::Timer(state)) => Some(state),
            _ => None,
        }
    }

    pub fn group(&self, handle: Handle) -> Option<&Vec<u32>> {
        match self.object(handle) {
            Some(Object::Group(members)) => Some(members),
            _ => None,
        }
    }

    pub fn group_mut(&mut self, handle: Handle) -> Option<&mut Vec<u32>> {
        match self.object_mut(handle) {
            Some(Object::Group(members)) => Some(members),
            _ => None,
        }
    }

    pub fn location(&self, handle: Handle) -> Option<Vec2> {
        match self.object(handle) {
            Some(Object::Location(pos)) => Some(*pos),
            _ => None,
        }
    }

    pub fn rect(&self, handle: Handle) -> Option<(Vec2, Vec2)> {
        match self.object(handle) {
            Some(Object::Rect { min, max }) => Some((*min, *max)),
            _ => None,
        }
    }

    pub fn boolexpr(&self, handle: Handle) -> Option<(Option<u32>, bool)> {
        match self.object(handle) {
            Some(Object::Boolexpr { func, negated }) => Some((*func, *negated)),
            _ => None,
        }
    }

    pub fn hashtable(&self, handle: Handle) -> Option<&HashtableCells> {
        match self.object(handle) {
            Some(Object::Hashtable(cells)) => Some(cells),
            _ => None,
        }
    }

    pub fn hashtable_mut(&mut self, handle: Handle) -> Option<&mut HashtableCells> {
        match self.object_mut(handle) {
            Some(Object::Hashtable(cells)) => Some(cells),
            _ => None,
        }
    }

    /// Unit handle → simulation unit id.
    pub fn sim_unit(&self, handle: Handle) -> Option<crate::UnitId> {
        if handle.kind != HandleKind::Unit {
            return None;
        }
        self.units.unit_of(handle.id)
    }

    /// Handle id for a simulation unit, allocating one on first use.
    pub fn unit_handle(&mut self, unit: crate::UnitId) -> Handle {
        let id = self.units.bind(unit);
        Handle::new(HandleKind::Unit, id)
    }

    /// Per-unit script state, created on demand with defaults.
    pub fn unit_meta_mut(&mut self, handle: Handle) -> Option<&mut UnitMeta> {
        if handle.kind != HandleKind::Unit {
            return None;
        }
        if !self.unit_meta.contains_key(&handle.id) {
            self.unit_meta.insert(handle.id, UnitMeta::default());
        }
        self.unit_meta.get_mut(&handle.id)
    }

    pub fn unit_meta(&self, handle: Handle) -> Option<&UnitMeta> {
        self.unit_meta.get(&handle.id)
    }

    /// Player slot for a player handle.
    pub fn player(&self, handle: Handle) -> Option<&PlayerState> {
        if handle.kind != HandleKind::Player {
            return None;
        }
        // Player ids are allocated in the low range: `player #index` has id
        // `index` (the engine's own `Player(i)` returns `i` for 0..23).
        self.players.get(handle.id as usize)
    }

    pub fn player_mut(&mut self, handle: Handle) -> Option<&mut PlayerState> {
        if handle.kind != HandleKind::Player {
            return None;
        }
        self.players.get_mut(handle.id as usize)
    }

    /// `Player(index)` handle. Ids 0..23 collide with the object id space, so
    /// player handles are always produced through this function and the object
    /// table starts at `0x4000_0000`.
    pub fn player_handle(index: i32) -> Handle {
        let clamped = index.clamp(0, PLAYER_SLOTS as i32 - 1) as u32;
        Handle::new(HandleKind::Player, clamped)
    }

    /// Start location by index, as `GetStartLocationLoc` sees it.
    pub fn start_location(&self, index: i32) -> Option<StartLocation> {
        self.start_locations
            .iter()
            .find(|location| location.index == index)
            .copied()
    }

    /// All live unit handles in ascending handle order (deterministic).
    pub fn unit_handles(&self) -> Vec<u32> {
        self.units.handles()
    }
}
