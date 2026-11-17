//! Game state: scenario, selection, commands, AI, economy, win/lose and the
//! per-frame scene description consumed by the renderer.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use glam::{Mat4, Quat, Vec2, Vec3};
use wc3_anim::Animator;
use wc3_path::Grid;
use wc3_sim::{Command, Event, Sim, Spell, SpellTarget, TargetKind, UnitId, UnitKind};
use wc3_assetdb::audio_provider::VoiceId;

use crate::assets::{Assets, Carry, SequenceKind, TERRAIN_TILE_SIZE};
use crate::camera::Camera;
use crate::hud::mobile_hud::{
    ArmyChip, CommandButton, GroupView, HintView, HudView, MenuButton, MenuScreen, MenuView,
    MinimapDot, MinimapView, PhaseView, PortraitFrame, RedrawSkin, ResourcePill, SafeArea,
    SelectionView, ToastTone, ToastView,
};
use crate::hud::{self, Button, Font, HudLayout, Rect, TextAlign, UiBatch};
use wc3_input::{TouchButton, TouchEnd, TouchInput, TouchLayout, TouchMove, TouchStart};

/// Input-layer hotkey type re-exported for platform hosts (SDL key events).
pub use wc3_input::Hotkey;

pub const TICK_SECONDS: f32 = 1.0 / 30.0;

/// Touch-target index base for the on-screen control-group row (1..9 map to
/// 100..108). Card buttons keep 0..11.
const GROUP_TARGET_BASE: usize = 100;

/// Touch-target index base for the army-panel icons (`selection` grid): index
/// `ARMY_TARGET_BASE + n` selects the n-th selected unit.
const ARMY_TARGET_BASE: usize = 200;
/// Persistent `GROUPS` drawer control (compact landscape / portrait).
const GROUPS_BUTTON_TARGET: usize = 300;
/// Persistent `COMMANDS` drawer control (compact landscape).
const COMMANDS_BUTTON_TARGET: usize = 301;

// Glue-screen button actions (index-coded in `MenuButton.action`).
pub const MENU_ACT_PLAY: u8 = 0;
pub const MENU_ACT_MAPS: u8 = 1;
pub const MENU_ACT_QUIT: u8 = 2;
pub const MENU_ACT_BACK: u8 = 3;
pub const MENU_ACT_RESUME: u8 = 7;
pub const MENU_ACT_MENU: u8 = 8;
pub const MENU_ACT_RESTART: u8 = 9;
pub const MENU_ACT_RACE: u8 = 10;
pub const MENU_ACT_OPPONENT: u8 = 11;
pub const MENU_ACT_MAP: u8 = 12;
pub const MENU_ACT_DIFFICULTY: u8 = 13;
/// Pause menu: write the running match into save slot 1.
pub const MENU_ACT_SAVE: u8 = 14;
/// Pause menu: restore save slot 1 and resume the battle.
pub const MENU_ACT_LOAD: u8 = 15;
/// Main menu: continue the checkpoint save (only with a save on disk).
pub const MENU_ACT_CONTINUE: u8 = 16;
/// Main menu: open the campaign chapter list.
pub const MENU_ACT_CAMPAIGN: u8 = 17;
/// Main menu: open the custom-game map list.
pub const MENU_ACT_CUSTOM: u8 = 18;
/// Main menu: options (audio/video/gameplay).
pub const MENU_ACT_OPTIONS: u8 = 19;
/// Rows 40..=44: start the visible campaign chapter at that slot.
pub const MENU_ACT_CHAPTER: u8 = 40;
/// Campaign group tabs (Prologue, Human, Undead, Orc, Night Elf, Extra).
pub const MENU_ACT_TAB: u8 = 60;
/// Chapter list paging.
pub const MENU_ACT_PAGE_PREV: u8 = 70;
pub const MENU_ACT_PAGE_NEXT: u8 = 71;
/// Rows 80..=84: pick the visible custom map at that slot.
pub const MENU_ACT_MAP_ROW: u8 = 80;
/// Custom-game list paging.
pub const MENU_ACT_MAP_PREV: u8 = 90;
pub const MENU_ACT_MAP_NEXT: u8 = 91;
/// Custom game: start the picked map with the current race/difficulty.
pub const MENU_ACT_START: u8 = 97;
/// Pause-menu save slots (`slot1.sav`); WC3 has ten, the mobile pause menu
/// exposes one quick-save slot and the host keeps its own resume checkpoint.
pub const MENU_SAVE_SLOT: u8 = 1;

/// WC3 melee result, written once when the match is decided.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchOutcome {
    /// Seat that survived (WC3: the last player with a base).
    pub winner: u8,
    /// Seats eliminated, in detection order.
    pub defeated: Vec<u8>,
    /// Simulated battle time in seconds (`sim.tick * TICK_SECONDS`).
    pub elapsed_s: f32,
    /// `no-buildings-or-workers`, `mutual-annihilation` or `scripted-victory`.
    pub reason: &'static str,
}

impl MatchOutcome {
    /// The `GAME_MATCH_END` device-log line.
    pub fn log_line(&self) -> String {
        format!(
            "GAME_MATCH_END winner={} defeated={:?} reason={} elapsed_s={:.1}",
            self.winner, self.defeated, self.reason, self.elapsed_s
        )
    }
}

/// One row of the WC3 post-game score screen, per seat.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScoreRow {
    pub seat: u8,
    pub units_trained: u32,
    pub units_lost: u32,
    pub units_killed: u32,
    pub buildings_built: u32,
    pub buildings_razed: u32,
    pub gold_gathered: u32,
    pub lumber_gathered: u32,
    pub upkeep_paid: u32,
    /// Highest living hero level this seat reached (0 = no hero).
    pub hero_level: u8,
    /// Orders per minute. Only the local seat issues `Command`s through the
    /// game layer; the AI issues its orders inside the sim, which exposes no
    /// per-seat order counter, so its APM reads 0.
    pub apm: u32,
}

/// Raw per-seat counters behind [`ScoreRow`]; accumulated from sim events and
/// saved with the match so a reload keeps the running score.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct SeatScore {
    units_trained: u32,
    units_lost: u32,
    units_killed: u32,
    buildings_built: u32,
    buildings_razed: u32,
    gold_gathered: u32,
    lumber_gathered: u32,
    upkeep_paid: u32,
    commands: u32,
}

/// Directory the pause-menu save slots live in. `HEROFOLENDS_SAVE_DIR`
/// overrides it for tests and portable installs.
fn default_save_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("HEROFOLENDS_SAVE_DIR") {
        return PathBuf::from(dir);
    }
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|home| PathBuf::from(home).join(".local/share/herofolends-rs/saves"))
        .unwrap_or_else(|| std::env::temp_dir().join("herofolends-saves"))
}

/// `<slot>.sav` -> `<slot>.sav.meta` (same sidecar convention as the host
/// resume checkpoint).
fn sidecar_path(save: &Path, suffix: &str) -> PathBuf {
    let mut name = save.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Save files are player data: owner-only where the platform has modes.
fn restrict_save_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// `sloc` start markers of a map in the sim's local coordinates (translated by
/// the `.w3e` origin, exactly like `sim_bridge::spawn_map_scene`).
fn map_start_points(bundle: &wc3_re::mapfiles::MapBundle) -> Vec<Vec2> {
    let origin = bundle
        .terrain
        .as_ref()
        .map(|terrain| Vec2::new(terrain.center.0, terrain.center.1))
        .unwrap_or(Vec2::ZERO);
    bundle
        .units
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .filter(|unit| {
            String::from_utf8_lossy(&unit.type_id).eq_ignore_ascii_case("sloc")
        })
        .map(|unit| Vec2::new(unit.position.x - origin.x, unit.position.y - origin.y))
        .collect()
}

/// True when the loaded map file is a retail campaign mission
/// (`Maps\\Campaign\\...`).
///
/// Campaign maps either run their own script or — when a legacy stem forces
/// them into the melee arena (`MELEE_MAP_STEMS`) — keep the canonical 1v1
/// opening, because their scripted placements are not a fair two-seat match
/// (Human04 places three halls, Orc01's mines are script-created).
fn map_path_is_campaign(assets: &Assets) -> bool {
    assets
        .debug
        .iter()
        .find_map(|line| line.strip_prefix("map="))
        .is_some_and(|path| map_file_is_campaign(Path::new(path)))
}

/// Path form of [`map_path_is_campaign`] (same `Maps\Campaign` rule).
fn map_file_is_campaign(path: &Path) -> bool {
    path.to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase()
        .contains("\\campaign\\")
}

/// Menu race choice → the sim's race enum. `Random` is deterministic per seed
/// (Orc on even seeds, Human on odd ones) so a replay is reproducible.
fn sim_race(choice: crate::hud::menu::RaceChoice, seed: u64) -> wc3_sim::Race {
    use crate::hud::menu::RaceChoice;
    match choice {
        RaceChoice::Human => wc3_sim::Race::Human,
        RaceChoice::Orc => wc3_sim::Race::Orc,
        RaceChoice::Undead => wc3_sim::Race::Undead,
        RaceChoice::NightElf => wc3_sim::Race::NightElf,
        RaceChoice::Random => {
            if seed % 2 == 0 {
                wc3_sim::Race::Orc
            } else {
                wc3_sim::Race::Human
            }
        }
    }
}

/// Map picker contents: every `.w3m`/`.w3x` under the install's `Maps/` tree/// (the loaded map first). A headless demo install lists its built-in scenario.
fn map_choices_from(assets: &Assets) -> Vec<crate::hud::menu::MapChoice> {
    use crate::hud::menu::MapChoice;
    let current = assets
        .debug
        .iter()
        .find_map(|line| line.strip_prefix("map="))
        .map(PathBuf::from);
    let mut maps: Vec<MapChoice> = Vec::new();
    if let Some(current) = current.as_ref() {
        maps.push(MapChoice {
            name: assets.map_name.clone(),
            path: current.to_string_lossy().into_owned(),
            players: assets.start_locations.max(1).min(u8::MAX as usize) as u8,
        });
        if let Some(root) = install_root_for(current) {
            let maps_root = root.join("Maps");
            collect_maps(&maps_root, 0, &mut maps);
        }
    }
    if maps.is_empty() {
        maps.push(MapChoice {
            name: assets.map_name.clone(),
            path: String::new(),
            players: 2,
        });
    }
    maps.sort_by(|a, b| {
        // Keep the running map first, then alphabetical.
        let a_current = a.path.is_empty() || current.as_ref().is_some_and(|c| a.path == c.to_string_lossy());
        let b_current = b.path.is_empty() || current.as_ref().is_some_and(|c| b.path == c.to_string_lossy());
        b_current.cmp(&a_current).then_with(|| a.name.cmp(&b.name))
    });
    maps.dedup_by(|a, b| a.path == b.path);
    maps.truncate(32);
    maps
}

fn collect_maps(dir: &Path, depth: usize, out: &mut Vec<crate::hud::menu::MapChoice>) {
    if depth > 4 || out.len() >= 64 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_maps(&path, depth + 1, out);
            continue;
        }
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if extension != "w3m" && extension != "w3x" {
            continue;
        }
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        out.push(crate::hud::menu::MapChoice {
            name,
            path: path.to_string_lossy().into_owned(),
            players: 2,
        });
        if out.len() >= 64 {
            return;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Playing,
    Won,
    Lost,
}

/// Top-level parity screens. Gameplay remains the default for simulation
/// tests; a real install session boots the retail glue flow (main menu) on its
/// first frame, and every screen is reachable with the back key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    /// Glue screens (main / race / opponent / map / difficulty pickers).
    MainMenu,
    MapSelect,
    Loading,
    /// In battle while the F10 game menu is open (sim is frozen).
    Paused,
    Victory,
    Defeat,
    Playing,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UiAction {
    Stop,
    SelectArmy,
    CentreBase,
    Gather,
    AttackMove,
    /// WC3 Move (M): next ground tap issues a plain move.
    Move,
    /// WC3 Patrol (P): next ground tap patrols between the unit and the tap.
    Patrol,
    Hold,
    Train(UnitKind),
    Build(UnitKind),
    /// Worker card: show the next page of the building list.
    BuildPageNext,
    /// Train an arbitrary roster unit by WC3 type id (JASS/roster path).
    TrainDef(&'static str),
    /// Place an arbitrary roster building by WC3 type id (farm, tower, …).
    BuildDef(&'static str),
    /// Prepare a hero/unit item for use; the slot index is the unit's own
    /// 1..6 inventory position (WC3 uses the same card cells).
    UseItem(u8),
    /// Open/close the quest log dialog (WC3 F9 console button).
    QuestLog,
    /// Cast a roster spell; unit/point spells enter target mode.
    Cast(Spell),
    /// Research one of the selected building's shipped `Researches=` upgrades.
    Research(&'static str),
    /// Cancel the selected construction site and refund it.
    CancelConstruction,
    /// Pick up the nearest ground item within reach of the selected unit.
    PickupNearestItem,
    /// Buy a stocked item from the selected shop (a shop row carries the id).
    BuyItem(&'static str),
    /// Hire a unit from the selected shop/tavern (`Sellunits=` row).
    BuyUnit(&'static str),
    ZoomIn,
    ZoomOut,
    Restart,
}

/// Immersive-mode safe-area insets in pixels, measured from each window edge.
/// The 3-D scene renders under the cutout; the HUD is laid out inside these.
/// Platform safe-area insets. One type across HUD and input; the canonical
/// implementation lives in `hud::mobile_hud::SafeArea`.
pub type SafeInsets = SafeArea;

/// Tactile feedback emitted by touch input. SDL3 has no portable phone-haptics
/// call, so the platform layer maps these cues to the Android vibrator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HapticCue {
    /// HUD button grabbed.
    Press,
    /// A unit or group became selected.
    Select,
    /// A move / attack / build order was issued.
    Order,
    /// The action was refused (no target, cannot build, not enough gold).
    Deny,
}

impl HapticCue {
    pub fn duration_ms(self) -> u64 {
        match self {
            HapticCue::Press => 10,
            HapticCue::Select => 14,
            HapticCue::Order => 20,
            HapticCue::Deny => 32,
        }
    }
}

/// A unit that died a moment ago and is still playing its Death animation.
struct DeathVisual {
    def_id: &'static str,
    /// Owner kept so the corpse keeps its player's team colour while it plays
    /// the Death clip.
    owner: u8,
    pos: Vec2,
    facing: f32,
    animator: Animator,
    elapsed: f32,
}

struct UnitVisual {
    animator: Animator,
    sequence: Option<SequenceKind>,
    played: Option<usize>,
    /// Last simulated position + tick, so "is it actually moving" is measured
    /// per SIM TICK instead of per render frame. The sim advances in fixed
    /// 1/30 ticks through an accumulator, so a per-frame delta is zero on every
    /// frame that did not tick — that flipped Walk<->Idle each frame and
    /// restarted the clip (the "model jitters" symptom).
    last_pos: wc3_sim::Vec2,
    last_tick: u64,
    /// Hysteresis: once walking, stay walking until the unit really stops.
    moving: bool,
    /// Attack cooldown at the previous tick: the sim re-arms it on every swing,
    /// which is the event that must restart the (non-looping) attack clip.
    last_cooldown: u32,
}

pub struct UnitDraw {
    pub model: usize,
    pub world: Mat4,
    pub tint: [f32; 4],
    pub nodes: Vec<Mat4>,
    /// Per-mesh geoset alpha at this frame (`GEOA` animations).
    pub mesh_alpha: Vec<f32>,
    /// Per-mesh, per-layer material alpha at this frame (`KMTA`).
    pub layer_alpha: Vec<Vec<f32>>,
}

pub struct RingDraw {
    pub pos: Vec3,
    pub radius: f32,
    pub color: [f32; 4],
}

/// Live placement ghost: the state the HUD log and the tests read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GhostState {
    pub def_id: &'static str,
    pub at: Vec2,
    pub legal: bool,
}

/// One rising/fading world-anchored label (`+35 G` on a kill, WC3 feedback).
pub struct FloatingLabel {
    pub text: String,
    /// World position the label is anchored to.
    pub pos: Vec2,
    pub elapsed: f32,
    pub life: f32,
    pub color: [f32; 4],
}

impl FloatingLabel {
    pub fn new(text: impl Into<String>, pos: Vec2, color: [f32; 4]) -> Self {
        Self {
            text: text.into(),
            pos,
            elapsed: 0.0,
            life: 1.2,
            color,
        }
    }

    /// 0..1 progress of the rise.
    pub fn progress(&self) -> f32 {
        (self.elapsed / self.life.max(0.01)).clamp(0.0, 1.0)
    }

    /// Alpha over the label's life: full for the first half, then fades out.
    pub fn alpha(&self) -> f32 {
        let progress = self.progress();
        if progress < 0.45 {
            1.0
        } else {
            (1.0 - (progress - 0.45) / 0.55).clamp(0.0, 1.0)
        }
    }

    /// World lift in render units: rises ~90 units over the life.
    pub fn lift(&self) -> f32 {
        90.0 * self.progress()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HealthBar {
    pub screen: Vec2,
    pub ratio: f32,
    pub width: f32,
    /// Unit the bar belongs to (tests/diagnostics; the renderer ignores it).
    pub unit: UnitId,
}

pub struct Scene {
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub viewport: Vec2,
    pub units: Vec<UnitDraw>,
    pub rings: Vec<RingDraw>,
    pub health: Vec<HealthBar>,
    pub ui: UiBatch,
    pub overlay: Option<(String, String)>,
}

pub struct Game {
    pub assets: Assets,
    pub sim: Sim,
    /// Map-driven JASS engine, booted when `Assets` was loaded with a map.
    /// `None` keeps the procedural scenario used by the menu/demo slice.
    pub engine: Option<wc3_re::engine::Engine>,
    /// Canonical bridge between the engine's JASS world and [`Game::sim`].
    /// Between ticks it owns a scratch sim; `update` swaps the live sim in.
    pub bridge: Option<crate::sim_bridge::JassBridge>,
    /// `true` when the loaded map runs as the 1v1 melee arena: terrain, pathing
    /// and trees are real map data, but the campaign script/units are skipped.
    pub melee: bool,
    pub camera: Camera,
    pub layout: HudLayout,
    /// Modern mobile HUD renderer and the last computed hit-test layout.
    pub mobile_hud: crate::hud::mobile_hud::MobileHud,
    pub mobile_layout: Option<crate::hud::mobile_hud::MobileHudLayout>,
    pub font: Font,
    pub selection: Vec<UnitId>,
    /// Gold mine under the last tap (`index` into `sim.gold_mines`). WC3 shows
    /// a mine's info plate when it is clicked; a mine is not a `UnitId`, so the
    /// selection needs its own slot. Invariant: `Some` only while
    /// `selection` is empty — `refresh_buttons` clears it on any unit pick.
    pub selected_mine: Option<usize>,
    pub selected_cells: Vec<Option<UnitId>>,
    pub phase: Phase,
    pub screen: Screen,
    pub loading_time: f32,
    pub placing: Option<&'static str>,
    /// Last pointer position in world space while placing (drives the ghost).
    pub placing_at: Option<Vec2>,
    pub attack_move: bool,
    /// Move mode: the next ground tap issues a plain move.
    pub move_order: bool,
    /// Patrol mode: the next ground tap starts a patrol order.
    pub patrol_order: bool,
    /// Spell waiting for a tap target (unit/point abilities).
    pub targeting: Option<Spell>,
    visuals: BTreeMap<u32, UnitVisual>,
    pub buttons: Vec<Button>,
    /// Command-card entries of the current selection, straight from the typed
    /// database (`Trains=` / `Builds=`), used by `hud::build_buttons`.
    pub menu: Vec<wc3_data::MenuEntry>,
    pub viewport: Vec2,
    pub elapsed: f32,
    pub fps: f32,
    pub frames: u64,
    pub player: u8,
    pub enemy: u8,
    pub base_pos: Vec2,
    pub enemy_base: Vec2,
    pub mines: [Vec2; 2],
    pub total_trained: u32,
    pub total_kills: u32,
    /// WC3 score counters per seat ([`ScoreRow`] is the public view).
    score: [SeatScore; 2],
    /// Seats that had units when the battle started; only a seat that was
    /// actually in the match can be defeated (an empty demo map must not end
    /// on frame one).
    seat_started: [bool; 2],
    /// Seats eliminated so far, in detection order.
    defeated: Vec<u8>,
    /// Match result, written once by [`Game::evaluate_match`].
    outcome: Option<MatchOutcome>,
    /// WC3 single-player pause: sim ticks stop, rendering/input continue.
    pub paused: bool,
    /// Directory the pause-menu save slots live in (`set_save_dir` in tests).
    pub save_dir: PathBuf,
    pub log: Vec<String>,
    /// Safe-area insets from the window manager (immersive mode / cutout).
    pub safe_insets: SafeInsets,
    /// Pixels per logical dp in framebuffer space (`px = dp * density`), from
    /// SDL's window-pixel / window-point ratio. The HUD layout is solved in dp
    /// and converted exactly once with this value (TZ §4.1).
    pub density_scale: f32,
    /// Compact landscape: the GROUPS drawer is open.
    pub groups_open: bool,
    /// Compact landscape: the 4x3 COMMANDS drawer is open.
    pub commands_open: bool,
    /// Developer overlay (FPS + hitboxes); off in release.
    pub dev_overlay: bool,
    accumulator: f32,
    pending: Vec<Command>,
    touch: TouchInput,
    touch_buttons: Vec<TouchButton>,
    /// WC3 control groups 1..9, shared by the keyboard digits and the ARMY-chip
    /// touch gestures.
    pub control_groups: wc3_input::ControlGroups,
    /// Card hotkey table (`ui.hotkeys.remap`): CommandStrings/object-data letters.
    pub hotkeys: wc3_input::remap::Remap,
    /// Active Tab subgroup kind (hero first, then unit types).
    pub subgroup_kind: Option<u32>,
    /// Worker build-menu page (0-based); the card shows three entries per page.
    pub build_page: usize,
    /// Short corpse window: the unit's Death animation after it dies.
    death_visuals: Vec<DeathVisual>,
    /// Device self-test script (`files/autotest.txt`), one command per line.
    autotest: Vec<String>,
    autotest_step: usize,
    autotest_wait: u32,
    /// Last recalled group (slot, time) for the double-press "centre" rule.
    group_press: Option<(u8, f32)>,
    /// Long-press tooltip text for the world unit under the finger.
    tooltip_text: Option<String>,
    long_press_applied: bool,
    /// Leaked once per Game: the HUD menu view requires a `'static` map title.
    menu_map_name: &'static str,
    haptics: Vec<HapticCue>,
    toast: Option<(String, f32)>,
    /// Live toast band: newest first, each with its remaining seconds. The HUD
    /// draws them as separate rows above the console.
    toast_queue: Vec<(String, f32)>,
    /// Selection the command card was built for; `hotkey` rebuilds the card
    /// when the selection changed programmatically (group recall, tests).
    card_selection: Vec<UnitId>,
    /// Last `GAME_CARD` label signature, so the card log is one line per
    /// rebuild instead of one per frame.
    card_log: String,
    /// Selection length the stored `mobile_layout` was solved for. The army
    /// row's slot count depends on it, so a selection change rebuilds the
    /// layout before the touch chips are authored.
    layout_army: usize,
    /// Last ghost cell that produced a `GHOST` log line (dedupes pointer moves).
    ghost_cell: Option<(u32, u32)>,
    /// World-anchored feedback labels (bounty "+35 G"): rise and fade, drawn
    /// through the same fitted-text path as the rest of the HUD.
    floating: Vec<FloatingLabel>,
    move_marker: Option<(Vec2, f32)>,
    /// Audio bookkeeping: last acked leader, listener pose, menu ambience.
    audio_last_selection: Option<UnitId>,
    audio_listener: Option<Vec2>,
    audio_ambience: Option<VoiceId>,
    /// Open in-game dialog (WC3 quest log behind the F9 console button).
    pub dialog: Option<crate::hud::mobile_hud::DialogScreen>,
    /// Retail glue flow: main → race/opponent/map/difficulty → loading → battle,
    /// plus the pause menu behind F10/BACK.
    pub menu_flow: crate::hud::menu::MenuState,
    /// Buttons of the current glue screen (labels are interned, bounded by the
    /// distinct map/stat strings the session ever shows).
    menu_buttons: Vec<crate::hud::mobile_hud::MenuButton<'static>>,
    /// Campaign group tabs of the campaign screen (Prologue…Extra).
    menu_tabs: Vec<crate::hud::mobile_hud::MenuButton<'static>>,
    /// Chapter/map rows of the current page.
    menu_rows: Vec<crate::hud::mobile_hud::MenuButton<'static>>,
    /// Bottom navigation (PREV / NEXT / START / BACK).
    menu_nav: Vec<crate::hud::mobile_hud::MenuButton<'static>>,
    /// Page counter (`HUMAN 1/2`).
    menu_page_label: &'static str,
    /// Dark veil over the live scene behind the glue panel.
    menu_veil: f32,
    menu_title: &'static str,
    menu_subtitle: &'static str,
    menu_labels: Vec<&'static str>,
    /// A real install session boots the main menu unless the host injects input
    /// first (headless probes drive the sim directly).
    boot_menu: bool,
    input_seen: bool,
    /// QUIT is a two-tap confirm: the first tap arms it, the second exits.
    quit_armed: bool,
    /// `war3map.w3i` header of the running map: the quest log's objective text
    /// comes from the map itself, not from a hardcoded scenario string.
    map_info: Option<MapInfoText>,
    seed: u64,
}

/// Objective text of the running map (`war3map.w3i`).
#[derive(Clone, Debug, Default)]
pub struct MapInfoText {
    pub name: String,
    pub author: String,
    pub description: String,
    pub players: String,
}

impl MapInfoText {
    /// Load the header from the map file (cheap: the map container only).
    pub fn load(path: &Path) -> Option<Self> {
        let bundle = wc3_re::mapfiles::MapBundle::open(path).ok()?;
        let info = bundle.info?;
        Some(Self {
            name: info.name,
            author: info.author,
            description: info.description.replace(['\r', '\n'], " "),
            players: info.recommended_players,
        })
    }
}

/// Map files live at `<install>/Maps/<category>/<map>`; walk up until the
/// WC3 data directories identify the install root. Mirrors `Engine`'s boot
/// layout (`Units/`, `Scripts/`, `UI/`).
fn install_root_for(map: &Path) -> Option<PathBuf> {
    for ancestor in map.ancestors().skip(1) {
        if ancestor.join("Units").is_dir() || ancestor.join("Scripts").is_dir() {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

/// Retail `AbilitySounds.slk` entry for an implemented spell, when one exists.
///
/// The table ships `BlizzardWave/BlizzardLoop`, `RainOfFireWave/Loop`, fire and
/// building loops; among the 11 simulated spells only Blizzard has a matching
/// retail sound, so the rest stay silent rather than borrowing a wrong sound.
fn spell_sound(spell: Spell) -> Option<&'static str> {
    match spell {
        Spell::Blizzard => Some("BlizzardWave"),
        _ => None,
    }
}

/// Load the real WC3 UI font (`Fonts\FRIZQT__.TTF`) from the mounted MPQ via
/// the canonical asset cache. Only accepts it when it covers Cyrillic (the
/// Russian Human04 map name and GlobalStrings); otherwise the system font is
/// the fallback. This keeps the HUD on original-game glyphs, not a synthetic
/// ASCII font.
/// Map the simulation race to the audio event race (Neutral has no theme row).
fn audio_race(race: wc3_sim::Race) -> Option<wc3_audio::events::Race> {
    match race {
        wc3_sim::Race::Human => Some(wc3_audio::events::Race::Human),
        wc3_sim::Race::Orc => Some(wc3_audio::events::Race::Orc),
        wc3_sim::Race::NightElf => Some(wc3_audio::events::Race::NightElf),
        wc3_sim::Race::Undead => Some(wc3_audio::events::Race::Undead),
        wc3_sim::Race::Neutral => None,
    }
}

/// Publish the asset pipeline summary once: the device log must be able to
/// prove which install, atlas and object database the frame was built from.
fn log_asset_summary(assets: &Assets) {
    for line in &assets.debug {
        let interesting = line.starts_with("ui_atlas=")
            || line.starts_with("objects")
            || line.starts_with("missing")
            || line.starts_with("mpq=");
        if interesting {
            crate::dev_log!("GAME_ASSETS {line}");
        }
    }
}

fn load_game_font(assets: &Assets) -> Font {
    for line in &assets.debug {
        let Some(path) = line.strip_prefix("mpq=") else {
            continue;
        };
        let Ok(mut db) = wc3_assetdb::AssetDb::open_mpq(path) else {
            continue;
        };
        let Ok(bytes) = db.read("Fonts\\FRIZQT__.TTF") else {
            continue;
        };
        if let Some(font) = Font::load_bytes(&bytes[..])
            && font.coverage().cyrillic_covered >= 60
        {
            return font;
        }
    }
    Font::load()
}

/// The twelve retail player colours (`PLAYER_COLOR_*`), rgb in 0..=1.
/// WC3 tints a unit's team-colour layer (the white/grey mask painted into the
/// model's pauldrons, banners and shields) with the owner's colour; drawing the
/// mask untinted is what made every peon's shoulder pad black-and-white.
pub const PLAYER_COLORS: [[f32; 3]; 12] = [
    [1.0, 0.012, 0.012],   // 0 red    (255, 3, 3)
    [0.0, 0.259, 1.0],     // 1 blue   (0, 66, 255)
    [0.110, 0.902, 0.725], // 2 teal   (28, 230, 185)
    [0.329, 0.0, 0.506],   // 3 purple (84, 0, 129)
    [1.0, 0.988, 0.0],     // 4 yellow (255, 252, 0)
    [0.996, 0.541, 0.055], // 5 orange (254, 138, 14)
    [0.125, 0.753, 0.0],   // 6 green  (32, 192, 0)
    [0.898, 0.357, 0.690], // 7 pink   (229, 91, 176)
    [0.584, 0.588, 0.592], // 8 grey   (149, 150, 151)
    [0.494, 0.749, 0.945], // 9 light blue (126, 191, 241)
    [0.063, 0.384, 0.275], // 10 dark green (16, 98, 70)
    [0.306, 0.165, 0.016], // 11 brown (78, 42, 4)
];

/// Tint for a unit of `owner`. The map's own player colour is not parsed yet, so
/// melee defaults apply: player 0 red, player 1 blue (WC3 `Player(0)`/`Player(1)`).
pub fn player_tint(owner: u8) -> [f32; 4] {
    let colour = PLAYER_COLORS[(owner as usize) % PLAYER_COLORS.len()];
    [colour[0], colour[1], colour[2], 1.0]
}

impl Game {
    pub fn new(mut assets: Assets, seed: u64) -> Self {
        let world = assets.world_size;
        let mut camera = Camera::new(world);
        camera.viewport = Vec2::new(1280.0, 720.0);
        let map_w = assets.map_width_cells.max(24);
        let map_h = assets.map_height_cells.max(24);
        let mut sim = Sim::new(seed, map_w, map_h);
        // Merge the decompiled gameplay constants into the runtime sim (reads
        // MiscGame.txt from the operator's MPQ; defaults if unavailable).
        if let Some(root) = assets
            .debug
            .iter()
            .find_map(|line| line.strip_prefix("mpq="))
            .and_then(|path| std::path::Path::new(path).parent())
        {
            match wc3_re::gameplay::GameplayConfig::load(root) {
                Ok(config) => sim.apply_gameplay(config),
                Err(error) => assets.debug.push(format!("gameplay_config_failed={error}")),
            }
        }
        sim.grid = build_grid(&assets, map_w, map_h);
        // The melee arena runs a real shipped map's terrain/pathing/trees but
        // never its campaign script (Human04's script spawns three halls).
        let melee = assets
            .debug
            .iter()
            .find_map(|line| line.strip_prefix("map="))
            .and_then(|path| Path::new(path).file_stem())
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| {
                Self::MELEE_MAP_STEMS
                    .iter()
                    .any(|candidate| stem.eq_ignore_ascii_case(candidate))
            })
            // A real melee map carries two or more `sloc` start markers, no
            // matter what the file is called. The old name-only test made the
            // same map run as a demo when it had its original filename.
            // Retail layout: campaign maps live under `Maps\\Campaign` and are
            // never melee, even when their `.doo` carries start markers.
            || (assets.start_locations >= 2 && !map_path_is_campaign(&assets));
        // The HUD menu view requires a `'static` map title; leak one copy per
        // game instance (bounded, created once).
        let menu_map_name = Box::leak(assets.map_name.clone().into_boxed_str());
        let mut game = Self {
            layout: HudLayout::new(Vec2::new(1280.0, 720.0)),
            mobile_hud: crate::hud::mobile_hud::MobileHud::new(),
            mobile_layout: None,
            font: {
                log_asset_summary(&assets);
                load_game_font(&assets)
            },
            camera,
            selection: Vec::new(),
            selected_mine: None,
            selected_cells: Vec::new(),
            phase: Phase::Playing,
            screen: Screen::Playing,
            loading_time: 0.0,
            placing: None,
            placing_at: None,
            attack_move: false,
            move_order: false,
            patrol_order: false,
            targeting: None,
            visuals: BTreeMap::new(),
            buttons: Vec::new(),
            menu: Vec::new(),
            viewport: Vec2::new(1280.0, 720.0),
            elapsed: 0.0,
            fps: 0.0,
            frames: 0,
            player: 0,
            enemy: 1,
            base_pos: Vec2::ZERO,
            enemy_base: Vec2::ZERO,
            mines: [Vec2::ZERO; 2],
            total_trained: 0,
            total_kills: 0,
            score: Default::default(),
            seat_started: [false; 2],
            defeated: Vec::new(),
            outcome: None,
            paused: false,
            save_dir: default_save_dir(),
            log: Vec::new(),
            safe_insets: SafeInsets::default(),
            // Replaced by `set_density` from the platform before the first
            // frame; 2.0 is a conservative phone default so a headless host
            // still gets sane 48 dp targets instead of a 1:1 px HUD.
            density_scale: 2.0,
            groups_open: false,
            commands_open: false,
            dev_overlay: false,
            accumulator: 0.0,
            pending: Vec::new(),
            touch: TouchInput::new(),
            touch_buttons: Vec::new(),
            control_groups: wc3_input::ControlGroups::default(),
            hotkeys: crate::ui::hotkeys::game_remap(),
            subgroup_kind: None,
            build_page: 0,
            death_visuals: Vec::new(),
            autotest: Vec::new(),
            autotest_step: 0,
            autotest_wait: 0,
            group_press: None,
            tooltip_text: None,
            long_press_applied: false,
            menu_map_name,
            haptics: Vec::new(),
            toast: None,
            toast_queue: Vec::new(),
            card_selection: Vec::new(),
            card_log: String::new(),
            layout_army: 0,
            ghost_cell: None,
            floating: Vec::new(),
            move_marker: None,
            seed,
            audio_last_selection: None,
            audio_listener: None,
            audio_ambience: None,
            dialog: None,
            menu_flow: crate::hud::menu::MenuState::new(map_choices_from(&assets)),
            menu_buttons: Vec::new(),
            menu_tabs: Vec::new(),
            menu_rows: Vec::new(),
            menu_nav: Vec::new(),
            menu_page_label: "",
            menu_veil: 0.55,
            menu_title: "WARCRAFT III",
            menu_subtitle: "",
            menu_labels: Vec::new(),
            // The melee arena is the interactive session: it opens on the
            // retail glue screens. A campaign map boots straight into its own
            // script (WC3 starts the mission from the campaign screen).
            boot_menu: melee,
            input_seen: false,
            quit_armed: false,
            map_info: None,
            sim,
            engine: None,
            bridge: None,
            melee,
            assets,
        };
        game.rebuild_layout();
        game.sync_menu();
        game.map_info = game
            .scenario_map_path()
            .as_deref()
            .and_then(MapInfoText::load);
        // Shipped FDF: parse the console screen and report coverage/overflow in
        // the runtime log (a real consumer of the parsed UI definitions).
        {
            let screen = crate::fdf_ui::UiScreen::Console;
            let coverage = crate::fdf_ui::coverage(&game.assets.ui, screen);
            let config = crate::ui::LayoutConfig::new(game.viewport);
            let bindings = crate::ui::TextBindings::new();
            let overflow = crate::fdf_ui::render(&game.assets.ui, screen, &config, &bindings, None)
                .map(|layout| crate::fdf_ui::overflow(&layout).len())
                .unwrap_or(0);
            game.log(format!(
                "GAME_UI_FDF screen={} docs={}/{} frames={} controls={}/{} skin={}/{} overflow={}",
                coverage.screen,
                coverage.documents_loaded,
                coverage.documents_expected,
                coverage.frames,
                coverage.required_present,
                coverage.required_present + coverage.required_missing.len(),
                coverage.skin_present,
                coverage.skin_present + coverage.skin_missing.len(),
                overflow
            ));
        }
        if !game.melee {
            game.attach_map_runtime();
        }
        game.reset_scenario();
        game.load_autotest();
        game.start_audio_for_scenario();
        game
    }

    /// Audio start-up for the scenario: retail music theme + 3D listener.
    ///
    /// The theme follows the local player's race (a melee map can seat either
    /// race on the local slot), and the tileset ambient loop starts from the
    /// shipped MIDI+DLS pair. Hosts can still override the music with
    /// [`Game::set_music`].
    fn start_audio_for_scenario(&mut self) {
        let race = self
            .player_audio_race()
            .unwrap_or(wc3_audio::events::Race::Human);
        let tileset = self
            .assets
            .terrain
            .as_ref()
            .map(|terrain| terrain.tileset as char);
        if let Some(audio) = self.assets.audio.as_mut() {
            audio.set_listener(
                Vec3::new(self.camera.target.x, 0.0, self.camera.target.y),
                0.0,
            );
            if audio.music_for_player_race(race).is_none() {
                let _ = audio.music("Human1");
            }
            if let Some(tileset) = tileset
                && let Some(voice) = audio.ambient_for_tileset(tileset, false)
            {
                crate::dev_log!(
                    "AUDIO_AMBIENT tileset={} night=0 voice={} {}",
                    tileset,
                    voice.0,
                    audio.last_event_line().unwrap_or("-")
                );
            }
        }
        self.audio_listener = Some(self.camera.target);
    }

    /// Host override for the gameplay music theme (retail theme name, e.g.
    /// `Human1`, `Orc1`, `DarkAgents`).
    pub fn set_music(&mut self, theme: &str) {
        if let Some(audio) = self.assets.audio.as_mut() {
            let _ = audio.music(theme);
        }
    }

    /// Menu ambience: the retail glue-screen wind loop from `AmbienceSounds.slk`.
    fn enter_menu_audio(&mut self) {
        if let Some(audio) = self.assets.audio.as_mut() {
            if self.audio_ambience.is_none() {
                self.audio_ambience = audio.ambient("GlueScreenWind");
            }
        }
    }

    fn leave_menu_audio(&mut self) {
        if let (Some(audio), Some(voice)) =
            (self.assets.audio.as_mut(), self.audio_ambience.take())
        {
            audio.stop_voice(voice);
        }
    }

    /// Per-frame audio: acknowledge selection changes once, track the listener.
    fn update_audio(&mut self) {
        // The mixer runs on the deterministic sim clock, not wall time: the ack
        // limiter and the music rotation must behave identically in a headless
        // run and on the phone.
        if let Some(audio) = self.assets.audio.as_mut() {
            audio.set_clock_ms(self.sim.tick * 1000 / 30);
            audio.tick_music();
        }
        let leader = self.selection.first().copied();
        if leader != self.audio_last_selection {
            self.audio_last_selection = leader;
            // A different unit starts its own address chain ("What?" again)
            // instead of continuing the previous unit's Pissed streak.
            if let Some(audio) = self.assets.audio.as_mut() {
                audio.selection_reset();
            }
            let unit = leader.and_then(|id| {
                self.sim.unit(id).map(|unit| {
                    (
                        unit.def().id,
                        Vec3::new(unit.pos.x, 0.0, unit.pos.y),
                    )
                })
            });
            if let Some((unit_id, pos)) = unit {
                if let Some(audio) = self.assets.audio.as_mut() {
                    let _ = audio.unit_ack(unit_id, Some(pos));
                }
            }
        }
        let target = self.camera.target;
        let moved = self
            .audio_listener
            .map(|last| last.distance_squared(target) > 0.25)
            .unwrap_or(true);
        if moved {
            self.audio_listener = Some(target);
            if let Some(audio) = self.assets.audio.as_mut() {
                audio.set_listener(Vec3::new(target.x, 0.0, target.y), 0.0);
            }
        }
    }

    fn reset_scenario(&mut self) {
        let world = self.assets.world_size;
        self.sim = Sim::new(
            self.seed,
            self.assets.map_width_cells,
            self.assets.map_height_cells,
        );
        // Native WPM pathing (32-unit cells) instead of the downsampled grid.
        crate::sim_bridge::apply_assets_pathing(&mut self.sim, &self.assets);
        // Typed gameplay constants from the operator's install (wc3_re::gameplay).
        if let Some(root) = crate::sim_bridge::gameplay_root_from_assets(&self.assets) {
            let _ = crate::sim_bridge::load_and_apply_gameplay(&mut self.sim, &root);
        }
        self.selection.clear();
        self.selected_cells.clear();
        self.visuals.clear();
        self.pending.clear();
        self.phase = Phase::Playing;
        self.placing = None;
        self.placing_at = None;
        self.attack_move = false;
        self.move_order = false;
        self.patrol_order = false;
        self.targeting = None;
        self.elapsed = 0.0;
        self.accumulator = 0.0;
        self.total_trained = 0;
        self.total_kills = 0;
        self.score = Default::default();
        self.seat_started = [false; 2];
        self.defeated.clear();
        self.outcome = None;
        self.paused = false;
        self.touch.reset();
        self.control_groups = wc3_input::ControlGroups::default();
        self.group_press = None;
        self.tooltip_text = None;
        self.dialog = None;
        self.haptics.clear();
        self.floating.clear();
        self.move_marker = None;

        self.base_pos = self.find_spot(Vec2::new(world.x * 0.24, world.y * 0.24));
        self.enemy_base = self.find_spot(Vec2::new(world.x * 0.76, world.y * 0.76));
        self.mines = [
            self.find_spot(self.base_pos + Vec2::new(360.0, 40.0)),
            self.find_spot(self.enemy_base - Vec2::new(360.0, 40.0)),
        ];
        self.camera
            .centre_on(self.base_pos + Vec2::new(180.0, 120.0));
        self.camera.distance = 1500.0;
        self.log_ui_surface_gaps();

        // Melee arena: real map terrain/pathing/trees, our own 1v1 spawns.
        if self.melee {
            self.spawn_melee_scenario();
            self.refresh_buttons();
            return;
        }

        // A loaded map owns its units, doodads and script: spawn the
        // `war3mapUnits.doo`/`CreateAllUnits` world instead of the demo base.
        if self.engine.is_some() {
            self.reset_map_runtime();
            self.refresh_buttons();
            return;
        }

        // Player base.
        let hall = self
            .sim
            .spawn(self.player, UnitKind::TownHall, self.base_pos);
        let peasant_a = self.sim.spawn(
            self.player,
            UnitKind::Peasant,
            self.base_pos + Vec2::new(-140.0, 150.0),
        );
        let peasant_b = self.sim.spawn(
            self.player,
            UnitKind::Peasant,
            self.base_pos + Vec2::new(-60.0, 180.0),
        );
        let footman = self.sim.spawn(
            self.player,
            UnitKind::Footman,
            self.base_pos + Vec2::new(150.0, 40.0),
        );
        let archer = self.sim.spawn(
            self.player,
            UnitKind::Archer,
            self.base_pos + Vec2::new(220.0, 120.0),
        );
        // Ability roster: a Paladin (Holy Light / Divine Shield) and an
        // Archmage (Blizzard) for the player, a Far Seer (Chain Lightning) for
        // the AI.  A starting farm keeps the 12-food hall from blocking the
        // opening army while the build menu exposes more farms.
        let paladin = self.sim.spawn_def(
            self.player,
            "Hpal",
            self.base_pos + Vec2::new(-260.0, -120.0),
        );
        let archmage = self.sim.spawn_def(
            self.player,
            "Hamg",
            self.base_pos + Vec2::new(320.0, -120.0),
        );
        let farm = self
            .sim
            .spawn_def(self.player, "hhou", self.base_pos + Vec2::new(520.0, 320.0));
        let farm_b = self.sim.spawn_def(
            self.player,
            "hhou",
            self.base_pos + Vec2::new(-420.0, 320.0),
        );
        let enemy_seer = self.sim.spawn_def(
            self.enemy,
            "Ofar",
            self.enemy_base + Vec2::new(240.0, -120.0),
        );
        let _ = (
            hall, footman, archer, paladin, archmage, farm, farm_b, enemy_seer,
        );

        // Enemy base plus a forward squad so contact happens within seconds.
        let enemy_hall = self
            .sim
            .spawn(self.enemy, UnitKind::TownHall, self.enemy_base);
        self.sim.spawn(
            self.enemy,
            UnitKind::Peasant,
            self.enemy_base + Vec2::new(140.0, 130.0),
        );
        self.sim.spawn(
            self.enemy,
            UnitKind::Footman,
            self.enemy_base + Vec2::new(-120.0, 60.0),
        );
        self.sim.spawn(
            self.enemy,
            UnitKind::Archer,
            self.enemy_base + Vec2::new(-170.0, 140.0),
        );
        let squad_centre = self.base_pos.lerp(self.enemy_base, 0.62);
        let squad_foot = self.sim.spawn(
            self.enemy,
            UnitKind::Footman,
            squad_centre + Vec2::new(-70.0, 30.0),
        );
        let squad_archer = self.sim.spawn(
            self.enemy,
            UnitKind::Archer,
            squad_centre + Vec2::new(60.0, -20.0),
        );
        let _ = enemy_hall;

        // Both sides start harvesting; that makes the economy loop live at once.
        self.pending.push(Command::Harvest {
            units: vec![peasant_a, peasant_b],
            to: self.mines[0],
        });
        let enemy_peasants: Vec<UnitId> = self
            .sim
            .alive_units()
            .filter(|unit| unit.owner == self.enemy && unit.def().stats.is_worker)
            .map(|unit| unit.id)
            .collect();
        if !enemy_peasants.is_empty() {
            self.pending.push(Command::Harvest {
                units: enemy_peasants,
                to: self.mines[1],
            });
        }
        self.pending.push(Command::MoveAttack {
            units: vec![squad_foot, squad_archer],
            to: self.base_pos,
        });

        // The canonical simulation owns economy and opponent AI. The shim
        // registers the scenario mines and enables the data-driven AI without
        // keeping any game-local AI/supply state.
        // The map owns its six gold mines; only the opponent AI is enabled here
        // (setup_scenario would add two synthetic mines on top of them).
        self.sim.enable_ai(
            self.enemy,
            wc3_sim::ai::AiConfig {
                workers: 3,
                army_supply: 6,
                first_attack_tick: 900,
                wave_interval: 1200,
                retreat_permille: 300,
                composition: &[("hfoo", 3), ("hrif", 2)],
                worker: "hpea",
            },
        );
        self.log(format!(
            "GAME_SCENARIO map={} world={:.0}x{:.0} base=({:.0},{:.0}) enemy=({:.0},{:.0}) source={}",
            self.assets.map_name,
            self.assets.world_size.x,
            self.assets.world_size.y,
            self.base_pos.x,
            self.base_pos.y,
            self.enemy_base.x,
            self.enemy_base.y,
            self.assets.source
        ));
        self.refresh_buttons();
    }

    /// The shipped map used for the 1v1 arena (player Orc vs Human).
    ///
    /// `Orc01.w3m` is a real RoC map with WPM pathing and `war3map.doo` trees.
    /// The campaign script is intentionally not executed: it places scripted
    /// units (Human04 spawns three halls), while this scenario owns its spawns.
    const MELEE_MAP_STEMS: &'static [&'static str] = &["Human04", "Orc01", "Melee", "(2)Ponds_v1.4"];

    fn scenario_map_path(&self) -> Option<PathBuf> {
        self.assets
            .debug
            .iter()
            .find_map(|line| line.strip_prefix("map="))
            .map(PathBuf::from)
    }

    /// 1v1 melee setup: one hall + one gold mine + five neutral creeps per side.
    ///
    /// Trees from the map are spawned first so their cells block pathing before
    /// bases are chosen; every base/unit position is snapped to a walkable cell
    /// (halls also require their whole footprint to fit on walkable ground).
    /// Melee scenario on a REAL map from the library.
    ///
    /// A multiplayer melee map is loaded exactly as it ships: the map's own
    /// `war3mapUnits.doo` content (creeps, gold mines, shops, destructables)
    /// and its `sloc` start markers go in through
    /// [`crate::sim_bridge::spawn_map_scene`]; nothing is generated except the
    /// two main halls and their five starting workers, and those take the
    /// `war3map.w3i` race of the seat that owns each start marker. A map with
    /// no usable w3i seat block falls back to the synthetic `setup_melee_1v1`
    /// opening. A legacy campaign stem forced into the arena adopts only the
    /// map's trees and keeps the synthetic opening (its script owns the real
    /// placements).
    fn spawn_melee_scenario(&mut self) {
        let world = self.assets.world_size;
        let map = self.scenario_map_path();
        let campaign_arena = map_path_is_campaign(&self.assets);
        let mut scene: Option<crate::sim_bridge::MapSpawns> = None;
        let mut starts: Vec<Vec2> = Vec::new();
        if let Some(map) = map.as_ref()
            && let Ok(bundle) = wc3_re::mapfiles::MapBundle::open(map)
        {
            if campaign_arena {
                // Legacy campaign arena: the mission's scripted placements are
                // not a fair 1v1 (Human04 places three halls, Orc01 has no
                // mines in its doo). Terrain trees are still the map's.
                starts = map_start_points(&bundle);
                // The arena must run the map's real WPM, like a melee map:
                // `UnitDef::footprint` is authored at the WC3 32-unit pathing
                // resolution (a hall is 6 cells), and the coarse 128-unit
                // terrain grid made `free_site` demand a 7x7-tile (896-unit)
                // clearing, so every campaign start marker was refused with
                // "no free hall site" and the arena came up with zero halls.
                crate::sim_bridge::apply_map_pathing(&mut self.sim, &bundle);
                let trees = crate::sim_bridge::spawn_map_destructables(&mut self.sim, &bundle);
                crate::dev_log!(
                    "GAME_SCENE_MAP source=campaign-arena map={} starts={} trees={} grid={}x{} cell={:.0}",
                    map.display(),
                    starts.len(),
                    trees,
                    self.sim.grid.width,
                    self.sim.grid.height,
                    self.sim.grid.cell_size
                );
                self.log(format!(
                    "GAME_SCENARIO melee map={} source=campaign-arena starts={} trees={}",
                    map.display(),
                    starts.len(),
                    trees
                ));
            } else {
                // The map owns terrain, pathing, its units, mines, trees and
                // the neutrals grouped into creep camps. `MapSpawns::units` is
                // the map's own `war3mapUnits.doo` count, so it stays
                // comparable to the file; the melee opening is reported
                // separately.
                let counts = crate::sim_bridge::spawn_map_scene(&mut self.sim, &bundle);
                crate::dev_log!(
                    "GAME_SCENE_MAP source=spawn_map_scene map={} units={} mines={} destructables={} starts={} camps={} melee_races={:?} skipped={}",
                    map.display(),
                    counts.units,
                    counts.gold_mines,
                    counts.destructables,
                    counts.start_locations,
                    counts.camps,
                    counts.melee.map(|bases| (bases.player_race, bases.enemy_race)),
                    crate::sim_bridge::scene_skips(&counts)
                );
                self.log(format!(
                    "GAME_SCENARIO melee map={} source=map starts={} units={} mines={} trees={} camps={} skipped={}",
                    map.display(),
                    counts.start_locations,
                    counts.units,
                    counts.gold_mines,
                    counts.destructables,
                    counts.camps,
                    crate::sim_bridge::scene_skips(&counts)
                ));
                starts = counts.start_points.clone();
                scene = Some(counts);
            }
        }
        // Per-seat opening: `spawn_map_scene` already placed one hall and five
        // workers per seat from the w3i race of each `sloc` marker's creator.
        if let Some(bases) = scene.as_ref().and_then(|counts| counts.melee) {
            self.base_pos = bases.player_start;
            self.enemy_base = bases.enemy_start;
            // The opponent keeps its canonical, race-driven AI.
            self.sim.enable_ai_for_race(self.enemy, bases.enemy_race);
            crate::dev_log!(
                "GAME_SCENE_SEATS source=w3i player={:?} enemy={:?} player_hall={:?} enemy_hall={:?} player_workers={} enemy_workers={} sim_units={} sim_mines={}",
                bases.player_race,
                bases.enemy_race,
                bases.player_hall.map(|id| id.0),
                bases.enemy_hall.map(|id| id.0),
                bases.player_workers,
                bases.enemy_workers,
                self.sim.alive_units().count(),
                self.sim.gold_mines.len()
            );
            self.log(format!(
                "GAME_RACE player={:?} enemy={:?} player_hall={:?} enemy_hall={:?} workers={} enemy_workers={} mines={} source=w3i",
                bases.player_race,
                bases.enemy_race,
                bases.player_hall,
                bases.enemy_hall,
                bases.player_workers,
                bases.enemy_workers,
                self.sim.gold_mines.len()
            ));
        } else {
            // No usable w3i seats (campaign-shaped doo or an unknown w3i
            // layout): place a fair 1v1 at the markers the map does carry,
            // else at a fair split of the arena, and generate the opening.
            // Skirmish races drive the real hall/worker defs through the sim.
            let player_race = sim_race(self.menu_flow.race, self.seed);
            let enemy_race = sim_race(self.menu_flow.opponent, self.seed ^ 0x9E37);
            let hall_footprint = wc3_sim::data::main_hall_for(player_race)
                .map(|def| def.footprint)
                .unwrap_or(6);
            let (mut player_base, mut enemy_base) = if starts.len() >= 2 {
                let mut best = (starts[0], starts[1]);
                let mut best_distance = 0.0;
                for (i, a) in starts.iter().enumerate() {
                    for b in starts.iter().skip(i + 1) {
                        let distance = a.distance(*b);
                        if distance > best_distance {
                            best_distance = distance;
                            best = (*a, *b);
                        }
                    }
                }
                best
            } else {
                (
                    self.find_spot_footprint(
                        Vec2::new(world.x * 0.22, world.y * 0.5),
                        hall_footprint,
                    ),
                    self.find_spot_footprint(
                        Vec2::new(world.x * 0.78, world.y * 0.5),
                        hall_footprint,
                    ),
                )
            };
            // A `sloc` marker can sit inside the map's own forest/cliff
            // (campaign maps author them for a scripted layout). The melee
            // opening must still bring up both halls, so an unusable anchor
            // slides to the nearest clearing that fits the real footprint —
            // the map's marker is a preference, never a reason to drop a side.
            for (label, anchor) in [("player", &mut player_base), ("enemy", &mut enemy_base)] {
                if self.footprint_fits(*anchor, hall_footprint) {
                    continue;
                }
                let moved = self.find_spot_footprint(*anchor, hall_footprint);
                self.log(format!(
                    "GAME_SCENE_ANCHOR side={label} from=({:.0},{:.0}) to=({:.0},{:.0}) footprint={hall_footprint}",
                    anchor.x, anchor.y, moved.x, moved.y
                ));
                *anchor = moved;
            }
            self.base_pos = player_base;
            self.enemy_base = enemy_base;
            match crate::sim_bridge::setup_melee_1v1(
                &mut self.sim,
                player_race,
                enemy_race,
                player_base,
                enemy_base,
            ) {
                Ok(setup) => {
                    // The map's own mines are authoritative: drop the ones the
                    // synthetic opening appended, so a map scene never runs
                    // with the map's gold plus a generated copy.
                    let map_mines = scene.as_ref().map(|counts| counts.gold_mines).unwrap_or(0);
                    if map_mines > 0 && self.sim.gold_mines.len() > map_mines {
                        self.sim.gold_mines.truncate(map_mines);
                    }
                    crate::dev_log!(
                        "GAME_SCENE_KIT source=setup_melee_1v1 halls={} workers={} enemy_workers={} start_mines={} expansion_mines={} camps={} creeps={} sim_units={} sim_mines={}",
                        setup.halls,
                        setup.player_workers,
                        setup.enemy_workers,
                        setup.start_mines,
                        setup.expansion_mines,
                        setup.camps,
                        setup.creeps,
                        self.sim.alive_units().count(),
                        self.sim.gold_mines.len()
                    );
                    self.log(format!(
                        "GAME_RACE choice={} opponent={} player_hall={:?} enemy_hall={:?} workers={} enemy_workers={} mines={} start_mines={} expansion_mines={} camps={} creeps={} source=melee_kit",
                        self.menu_flow.race.label(),
                        self.menu_flow.opponent.label(),
                        setup.player_hall,
                        setup.enemy_hall,
                        setup.player_workers,
                        setup.enemy_workers,
                        self.sim.gold_mines.len(),
                        setup.start_mines,
                        setup.expansion_mines,
                        setup.camps,
                        setup.creeps,
                    ))
                }
                Err(error) => self.log(format!("GAME_SCENARIO melee failed err={error}")),
            }
        }
        // The opening view is the local player's hall, exactly like the retail
        // game: the map's `sloc` order can seat either race on either side, so
        // the hall of `self.player` is the only authoritative base marker.
        let player_hall = self
            .sim
            .alive_units()
            .find(|unit| unit.owner == self.player && unit.def().main_hall)
            .map(|unit| unit.pos);
        if let Some(hall) = player_hall {
            self.base_pos = hall;
        }
        if let Some(hall) = self
            .sim
            .alive_units()
            .find(|unit| unit.owner == self.enemy && unit.def().main_hall)
            .map(|unit| unit.pos)
        {
            self.enemy_base = hall;
        }
        if !self.sim.gold_mines.is_empty() {
            self.mines = [
                self.sim
                    .nearest_gold_mine(self.base_pos)
                    .unwrap_or(self.base_pos),
                self.sim
                    .nearest_gold_mine(self.enemy_base)
                    .unwrap_or(self.enemy_base),
            ];
        } else {
            self.mines = [self.base_pos, self.enemy_base];
        }
        // The camera model anchors the eye at `distance * 0.92` above the zero
        // plane while the terrain sits at its own W3E height (Justice's bases
        // are ~627u up). Too short a boom puts the base above the top of the
        // screen. Scale the start distance with the ground so the drawn hall,
        // its workers and the tapped trees are inside the viewport.
        let ground = self.assets.height_at(self.base_pos.x, self.base_pos.y);
        self.camera.centre_on(self.base_pos);
        self.camera.distance = (ground * 1.95).max(1500.0).min(2600.0);
        if std::env::var_os("H2_TAP_DEBUG").is_some() {
            let probe = self
                .sim
                .alive_units()
                .find(|unit| unit.owner == self.player && unit.def().stats.is_worker)
                .map(|unit| (unit.pos, self.assets.height_at(unit.pos.x, unit.pos.y)));
            eprintln!(
                "[scene] base=({:.0},{:.0}) enemy=({:.0},{:.0}) camera=({:.0},{:.0}) dist={:.0} player={} melee={} probe={:?}",
                self.base_pos.x,
                self.base_pos.y,
                self.enemy_base.x,
                self.enemy_base.y,
                self.camera.target.x,
                self.camera.target.y,
                self.camera.distance,
                self.player,
                self.melee,
                probe
            );
        }
        self.log(format!(
            "GAME_SCENARIO melee-ready map={} units={} mines={} hall_base={:?} enemy_base={:?} camera={:.0}",
            self.assets.map_name,
            self.sim.alive_units().count(),
            self.sim.gold_mines.len(),
            self.base_pos,
            self.enemy_base,
            self.camera.distance
        ));
    }

    /// Boot the map script engine when the assets were loaded from a map.
    /// The engine is mounted without a map first; `reset_scenario` enters the
    /// map so menu resets replay the script exactly once.
    fn attach_map_runtime(&mut self) {
        let map = self
            .assets
            .debug
            .iter()
            .find_map(|line| line.strip_prefix("map="))
            .map(PathBuf::from);
        let Some(map) = map else {
            return;
        };
        // Session isolation (AUD-032): the JASS seat default is process-wide
        // because the script's `main` runs during boot, and a campaign mission
        // sets it before `Game::new` (Human01's human seat is 2, not 0). A
        // melee map must not inherit that seat: `GetLocalPlayer() == Player(0)`
        // would be false and every unguarded script block would be dropped.
        // Campaign maps keep the defaults the campaign installed.
        if !map_file_is_campaign(&map) {
            wc3_re::jass::vm::reset_local_player_default();
        }
        let root = self
            .assets
            .debug
            .iter()
            .find_map(|line| line.strip_prefix("mpq="))
            .map(PathBuf::from)
            .and_then(|mpq| mpq.parent().map(Path::to_path_buf))
            .or_else(|| install_root_for(&map));
        let Some(root) = root else {
            self.log(format!(
                "GAME_MAP_RUNTIME skipped map={} (install root not found)",
                map.display()
            ));
            return;
        };
        let mut config = wc3_re::engine::EngineConfig::new(root);
        config.seed = self.seed;
        match wc3_re::engine::Engine::boot(config) {
            Ok(mut engine) => {
                // AUD-031: a native this build does not execute returns the
                // declared default so the script keeps running; the diagnostic
                // list is the only observable record that it did nothing.
                if let Some(runtime) = engine.runtime.as_ref()
                    && !runtime.unimplemented.is_empty()
                {
                    let total: u64 = runtime.unimplemented.values().sum();
                    let top = engine
                        .unimplemented_natives()
                        .iter()
                        .take(8)
                        .map(|(name, count)| format!("{name}:{count}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    self.log(format!(
                        "GAME_JASS_UNIMPLEMENTED natives={} calls={} top={}",
                        runtime.unimplemented.len(),
                        total,
                        top
                    ));
                }
                engine.config.map = Some(map);
                self.engine = Some(engine);
            }
            Err(error) => self.log(format!("GAME_MAP_RUNTIME boot_failed={error}")),
        }
    }

    /// Re-enter the loaded map and adopt its JASS world into the canonical
    /// sim. The bridge keeps the handle↔UnitId map across ticks; `Game::sim`
    /// stays the owner of gameplay state between frames.
    fn reset_map_runtime(&mut self) {
        let Some(map) = self.engine.as_ref().and_then(|engine| engine.config.map.clone()) else {
            return;
        };
        // Map scripts recurse through nested blocks/if-chains deep enough to
        // overflow the 2 MB default thread stack (Human01 is the worst map).
        // Run the entry point on a dedicated 32 MB stack; the interpreter then
        // executes the shipped script instead of dying with SIGSEGV.
        let entered: Result<(), String> = {
            let engine = self.engine.as_mut().expect("engine checked above");
            std::thread::scope(|scope| {
                match std::thread::Builder::new()
                    .name("wc3-map-script".into())
                    .stack_size(32 * 1024 * 1024)
                    .spawn_scoped(scope, || engine.enter_map(&map).map_err(|e| e.to_string()))
                {
                    Ok(handle) => handle
                        .join()
                        .unwrap_or_else(|_| Err("map script thread panicked".to_string())),
                    Err(error) => Err(format!("map script thread: {error}")),
                }
            })
        };
        if let Err(error) = entered {
            self.log(format!("GAME_MAP_RUNTIME enter_failed={error}"));
            return;
        }
        let mut bridge = crate::sim_bridge::JassBridge::new(Sim::new(
            self.seed,
            self.assets.map_width_cells,
            self.assets.map_height_cells,
        ));
        if let Some(runtime) = self.engine.as_mut().and_then(|engine| engine.runtime_mut()) {
            bridge.attach(&mut runtime.world);
        }
        self.sim = std::mem::replace(
            &mut bridge.sim,
            Sim::new(
                self.seed,
                self.assets.map_width_cells,
                self.assets.map_height_cells,
            ),
        );
        self.bridge = Some(bridge);
        let world = self.assets.world_size;
        self.base_pos = self
            .sim
            .units_of(self.player)
            .next()
            .map_or_else(|| self.find_spot(Vec2::new(world.x * 0.24, world.y * 0.24)), |unit| unit.pos);
        self.enemy_base = self
            .sim
            .units_of(self.enemy)
            .next()
            .map_or_else(|| self.find_spot(Vec2::new(world.x * 0.76, world.y * 0.76)), |unit| unit.pos);
        self.mines = [self.base_pos, self.enemy_base];
        self.camera.centre_on(self.base_pos);
        self.camera.distance = 1500.0;
        if let Some(engine) = self.engine.as_ref() {
            let spawn = engine.map_spawn();
            self.log(format!(
                "GAME_MAP_RUNTIME map={} units={} doodads={} spawned={}/{} matched={}/{} world_units={} world_destructables={}",
                map.display(),
                spawn.map_units,
                spawn.map_doodads,
                spawn.spawned_units,
                spawn.map_units,
                spawn.matched_units,
                spawn.map_doodads,
                self.sim.units.len(),
                self.sim.destructables.len(),
            ));
        }
    }

    /// Walkable spot whose whole building footprint fits on open ground.
    ///
    /// Halls occupy `footprint × footprint` cells; a single walkable centre is
    /// not enough (the rest of the building would sit inside trees/water).
    /// Footprint side in this grid's cells for an authored WC3 footprint.
    ///
    /// `UnitDef::footprint` is authored at the 32-unit pathing resolution (a
    /// Great Hall is 6). The same rule the sim applies in
    /// `Sim::footprint_side_cells`: scale by the grid's cell size and round up,
    /// so a 128-unit sim grid needs one cell where a 32-unit game grid needs
    /// four.
    fn footprint_side_cells(&self, footprint: u8) -> i32 {
        let world = footprint.max(1) as f32 * wc3_sim::PATHING_CELL_SIZE;
        ((world / self.sim.grid.cell_size.max(1.0)).ceil() as i32).max(1)
    }

    /// Whole-footprint walkability check for an authored footprint.
    fn footprint_fits(&self, pos: Vec2, footprint: u8) -> bool {
        let side = self.footprint_side_cells(footprint);
        let half = side / 2;
        let cell = self.sim.grid.world_to_cell(pos);
        for dy in -half..=half {
            for dx in -half..=half {
                let x = cell.0 as i32 + dx;
                let y = cell.1 as i32 + dy;
                if x < 0
                    || y < 0
                    || x as u32 >= self.sim.grid.width
                    || y as u32 >= self.sim.grid.height
                    || !self.sim.grid.walkable(x as u32, y as u32)
                {
                    return false;
                }
            }
        }
        true
    }

    /// Nearest position to `wanted` whose whole `footprint` (authored in WC3
    /// 32-unit pathing cells) stands on walkable ground.
    fn find_spot_footprint(&self, wanted: Vec2, footprint: u8) -> Vec2 {
        let world = self.assets.world_size;
        let cell = self.sim.grid.world_to_cell(Vec2::new(
            wanted.x.clamp(0.0, world.x),
            wanted.y.clamp(0.0, world.y),
        ));
        let half = (self.footprint_side_cells(footprint) / 2).max(0);
        let fits = |cx: i32, cy: i32| -> bool {
            for dy in -half..=half {
                for dx in -half..=half {
                    let x = cx + dx;
                    let y = cy + dy;
                    if x < 0
                        || y < 0
                        || x as u32 >= self.sim.grid.width
                        || y as u32 >= self.sim.grid.height
                        || !self.sim.grid.walkable(x as u32, y as u32)
                    {
                        return false;
                    }
                }
            }
            true
        };
        if fits(cell.0 as i32, cell.1 as i32) {
            return self.sim.grid.cell_to_world(cell.0, cell.1);
        }
        for radius in 1..80i32 {
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs() != radius && dy.abs() != radius {
                        continue;
                    }
                    let x = cell.0 as i32 + dx;
                    let y = cell.1 as i32 + dy;
                    if fits(x, y) {
                        return self.sim.grid.cell_to_world(x as u32, y as u32);
                    }
                }
            }
        }
        self.find_spot(wanted)
    }

    fn find_spot(&self, wanted: Vec2) -> Vec2 {
        let world = self.assets.world_size;
        let cell = self.sim.grid.world_to_cell(Vec2::new(
            wanted.x.clamp(0.0, world.x),
            wanted.y.clamp(0.0, world.y),
        ));
        if self.sim.grid.walkable(cell.0, cell.1) {
            return self.sim.grid.cell_to_world(cell.0, cell.1);
        }
        for radius in 1..40u32 {
            for dy in -(radius as i32)..=(radius as i32) {
                for dx in -(radius as i32)..=(radius as i32) {
                    if dx.abs() != radius as i32 && dy.abs() != radius as i32 {
                        continue;
                    }
                    let x = cell.0 as i32 + dx;
                    let y = cell.1 as i32 + dy;
                    if x >= 0
                        && y >= 0
                        && (x as u32) < self.sim.grid.width
                        && (y as u32) < self.sim.grid.height
                        && self.sim.grid.walkable(x as u32, y as u32)
                    {
                        return self.sim.grid.cell_to_world(x as u32, y as u32);
                    }
                }
            }
        }
        wanted
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.viewport = Vec2::new(width.max(1.0), height.max(1.0));
        self.camera.viewport(width, height);
        self.rebuild_layout();
        self.refresh_buttons();
    }

    /// Pixels per logical dp reported by the platform (SDL:
    /// `SDL_GetWindowDisplayScale`, the Android `densityDpi / 160` content
    /// scale). Re-solves the HUD in dp; no second scale transform.
    pub fn set_density(&mut self, density_scale: f32) -> bool {
        let next = if density_scale.is_finite() && density_scale > 0.05 {
            density_scale.clamp(0.5, 8.0)
        } else {
            return false;
        };
        if (next - self.density_scale).abs() < 0.001 {
            return false;
        }
        self.density_scale = next;
        self.rebuild_layout();
        self.refresh_buttons();
        true
    }

    /// Framebuffer size, density and safe insets in one call (platform hook).
    pub fn set_window_metrics(
        &mut self,
        framebuffer: Vec2,
        density_scale: f32,
        insets: SafeInsets,
    ) {
        self.viewport = Vec2::new(framebuffer.x.max(1.0), framebuffer.y.max(1.0));
        self.camera.viewport(self.viewport.x, self.viewport.y);
        if density_scale.is_finite() && density_scale > 0.05 {
            self.density_scale = density_scale.clamp(0.5, 8.0);
        }
        self.safe_insets = insets.sanitised(self.viewport);
        self.rebuild_layout();
        self.refresh_buttons();
    }

    /// Developer overlay (FPS + hitbox debug). Off in release.
    pub fn set_dev_overlay(&mut self, on: bool) {
        if self.dev_overlay != on {
            self.dev_overlay = on;
            self.rebuild_layout();
        }
    }

    /// Toggle the compact GROUPS drawer. Returns true when the state changed.
    pub fn toggle_groups_drawer(&mut self) -> bool {
        self.groups_open = !self.groups_open;
        self.commands_open = false;
        self.rebuild_layout();
        self.refresh_buttons();
        true
    }

    /// Toggle the compact 4x3 COMMANDS drawer. Returns true when it changed.
    pub fn toggle_commands_drawer(&mut self) -> bool {
        self.commands_open = !self.commands_open;
        self.groups_open = false;
        self.rebuild_layout();
        self.refresh_buttons();
        true
    }

    pub fn close_hud_drawers(&mut self) -> bool {
        if !self.groups_open && !self.commands_open {
            return false;
        }
        self.groups_open = false;
        self.commands_open = false;
        self.rebuild_layout();
        self.refresh_buttons();
        true
    }

    /// Apply the window-manager safe area (immersive bars, cutout, gesture
    /// insets). The scene keeps rendering edge to edge; only the HUD moves.
    /// Returns true when the layout changed, so the caller can log it once.
    pub fn set_safe_insets(&mut self, left: f32, top: f32, right: f32, bottom: f32) -> bool {
        let next = SafeInsets {
            left,
            top,
            right,
            bottom,
        }
        .sanitised(self.viewport);
        if next == self.safe_insets {
            return false;
        }
        self.safe_insets = next;
        self.rebuild_layout();
        self.refresh_buttons();
        true
    }

    /// Lay the HUD out inside the safe area: build it for the reduced viewport,
    /// then translate every rect by the inset origin. The HUD geometry is
    /// derived from the same layout, so its hit ring matches what is drawn.
    fn rebuild_layout(&mut self) {
        self.safe_insets = self.safe_insets.sanitised(self.viewport);
        // The engine works in dp; the window manager reports px. This is the
        // one conversion of the insets (TZ §4.3), and `HudLayout::to_px` is the
        // one conversion back.
        let d = self.density_scale.max(0.05);
        let safe = SafeArea::new(
            self.safe_insets.left / d,
            self.safe_insets.top / d,
            self.safe_insets.right / d,
            self.safe_insets.bottom / d,
        );
        let dummy_resources = [
            ResourcePill::new("", "", [1.0; 4]),
            ResourcePill::new("", "", [1.0; 4]),
            ResourcePill::new("", "", [1.0; 4]),
        ];
        let dummy_commands: [CommandButton; 12] = std::array::from_fn(|_| CommandButton::empty());
        // The army row is a function of the live selection: the chip count
        // drives the panel's pagination (`army_slot_hits(count)`). Without it
        // the stored layout had zero army slots, so the chips were neither
        // drawn as hit rects nor tappable while the draw path laid them out
        // from the same selection — the hit ring and the art diverged.
        let army = self.army_layout_chips();
        let view = HudView::new(self.viewport)
            .density(d)
            .safe(safe)
            .resources(&dummy_resources)
            .commands(&dummy_commands)
            .redraw(self.hud_redraw_skin())
            .hud_skin(self.assets.hud_skin())
            .army(&army)
            .selection_count(self.selection_count_for_layout())
            .groups_open(self.groups_open)
            .commands_open(self.commands_open)
            .dev_overlay(self.dev_overlay);
        let mobile = self.mobile_hud.layout(&view);
        self.layout_army = army.len();
        let mut layout = HudLayout::new(self.viewport);
        // `HudLayout::scale` is the px-per-dp factor for every legacy consumer
        // (ring radii, edge pan, hitbox debug). The pack art has its own scale
        // inside `MobileHudLayout::scale`.
        layout.scale = self.density_scale;
        // `HudLayout::top_bar` is the console blocker (see its doc): the top
        // strip is published separately through the mobile layout and
        // `blocked_rects` adds it explicitly.
        layout.top_bar = mobile.console;
        layout.resource_row = mobile.resources;
        layout.minimap = mobile.minimap;
        layout.minimap_image = mobile.minimap_image;
        layout.selection = mobile.selection;
        layout.command = mobile.card;
        layout.button = mobile.cell;
        self.layout = layout;
        self.mobile_layout = Some(mobile);
        // Automation/support contract: publish the touch geometry once per
        // layout rebuild so the device evidence harness taps exact slots
        // instead of guessing pixels (`GAME_HUD_RECTS` is consumed by
        // `analysis/tech-lead/device_pass.sh`).
        {
            let m = self.mobile_layout.as_ref().expect("just stored");
            let slots: String = m
                .card_slots
                .iter()
                .enumerate()
                .map(|(index, rect)| {
                    format!("{index}@{:.0},{:.0}", rect.centre().x, rect.centre().y)
                })
                .collect::<Vec<_>>()
                .join(" ");
            crate::dev_log!(
                "GAME_HUD_RECTS scale={:.2} card={:.0},{:.0},{:.0},{:.0} tile={:.1} slots=[{}] \
                 army={:.0},{:.0} minimap={:.0},{:.0},{:.0},{:.0} \
                 console={:.0},{:.0},{:.0},{:.0} strip={:.0},{:.0} \
                 playfield={:.0},{:.0},{:.0},{:.0} army_slots={}",
                m.scale,
                m.card.x,
                m.card.y,
                m.card.w,
                m.card.h,
                m.cell,
                slots,
                m.army.centre().x,
                m.army.centre().y,
                m.minimap.x,
                m.minimap.y,
                m.minimap.w,
                m.minimap.h,
                m.console.x,
                m.console.y,
                m.console.w,
                m.console.h,
                m.top_meta.y,
                m.top_meta.h,
                m.top_meta.x,
                m.top_meta.bottom(),
                (m.top_meta.w).max(0.0),
                (m.console.y - m.top_meta.bottom()).max(0.0),
                m.army_slots.len()
            );
        }
    }

    fn card_button_indices(&self) -> [usize; 12] {
        // build_buttons authors every slot exactly (row-major 4x3): no
        // reordering, otherwise MORE/ARMY/queue slots never become visible.
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
    }

    /// Rebuild the command buttons and their touch snapshot in one place.
    /// `apply_mobile_button_rects` runs before the snapshot, so a hit test can
    /// never use the pre-reflow slots.
    fn refresh_buttons(&mut self) {
        // A gold-mine plate is a resource read-out, not a unit: any unit
        // selection replaces it (WC3 has one info plate).
        if !self.selection.is_empty() {
            self.selected_mine = None;
        }
        // The army row paginates off the selected-unit count, so a selection
        // change must re-solve the layout before the chips become touch
        // targets; the card rebuild below is the single place every selection
        // change already passes through.
        if self.mobile_layout.is_some()
            && self.selection.len().min(8) != self.layout_army
        {
            self.rebuild_layout();
        }
        self.menu = self.compute_menu();
        self.buttons = hud::build_buttons(self);
        // A selected gold mine is a resource, not a command source: WC3 leaves
        // the base/zoom card on screen (no train/build/spell rows).
        if self.selected_mine.is_some() && self.selection.is_empty() {
            self.buttons.retain(|button| {
                matches!(
                    button.action,
                    UiAction::CentreBase | UiAction::ZoomIn | UiAction::ZoomOut
                )
            });
        }
        self.card_selection = self.selection.clone();
        self.apply_mobile_button_rects();
        self.touch_buttons.clear();
        self.touch_buttons
            .extend(
                self.buttons
                    .iter()
                    .enumerate()
                    .map(|(index, button)| TouchButton {
                        index,
                        rect: touch_rect(button.rect),
                        enabled: button.enabled,
                    }),
            );
        let slots = self.group_row_slots();
        self.touch_buttons
            .extend(slots.iter().enumerate().map(|(index, rect)| TouchButton {
                index: GROUP_TARGET_BASE + index,
                rect: touch_rect(*rect),
                enabled: true,
            }));
        // Drawer toggles: persistent 48 dp controls in compact landscape (and
        // the GROUPS control in portrait). They are ordinary buttons with
        // reserved target indices so the router never has to guess.
        if let Some(mobile) = self.mobile_layout.as_ref() {
            for (index, rect) in [
                (GROUPS_BUTTON_TARGET, mobile.groups_button),
                (COMMANDS_BUTTON_TARGET, mobile.commands_button),
            ] {
                if rect.w > 0.0 && rect.h > 0.0 {
                    self.touch_buttons.push(TouchButton {
                        index,
                        rect: touch_rect(rect),
                        enabled: true,
                    });
                }
            }
        }
        // Army panel icons are tappable: a tap singles the unit out, a double
        // tap centres the camera on it (WC3 selection grid behaviour).
        let army: Vec<Rect> = self
            .mobile_layout
            .as_ref()
            .map(|layout| layout.army_slots.clone())
            .unwrap_or_default();
        let selected = self.selection.len();
        self.touch_buttons
            .extend(army.iter().enumerate().map(|(index, rect)| TouchButton {
                index: ARMY_TARGET_BASE + index,
                rect: touch_rect(*rect),
                enabled: index < selected,
            }));
        // Device harness contract: one line per command-card rebuild with the
        // label in every slot, so a tap target is "BUILD"/"TRAIN"/"HARVEST" by
        // name instead of by guessing the slot. `refresh_buttons` runs every
        // frame, so the line is emitted only when the labels change.
        let labels: Vec<String> = self
            .buttons
            .iter()
            .enumerate()
            .map(|(index, button)| format!("{index}:{}", button.label))
            .collect();
        let signature = labels.join(" ");
        if signature != self.card_log {
            self.card_log = signature;
            crate::dev_log!(
                "GAME_CARD slots={} labels=[{}]",
                self.buttons.len(),
                self.card_log
            );
        }
    }

    /// True when `pos` lands on an open drawer panel or one of its toggles.
    fn drawer_contains(&self, pos: Vec2) -> bool {
        let Some(mobile) = self.mobile_layout.as_ref() else {
            return false;
        };
        for rect in [
            mobile.groups_overlay,
            mobile.commands_overlay,
            mobile.groups_button,
            mobile.commands_button,
        ] {
            if rect.w > 0.0 && rect.h > 0.0 && rect.contains(pos) {
                return true;
            }
        }
        for slot in &mobile.group_slots {
            if slot.contains(pos) {
                return true;
            }
        }
        for slot in &mobile.card_slots {
            if slot.contains(pos) {
                return true;
            }
        }
        false
    }

    /// HUD rectangles that swallow a touch instead of passing it to the world:
    /// the whole console band (edge to edge), the top resource strip and any
    /// open drawer. Everything between the strip and the console is the
    /// playfield and must reach the world.
    fn blocked_rects(&self) -> Vec<wc3_input::Rect> {
        let mut rects = vec![
            touch_rect(self.layout.top_bar),
            touch_rect(self.layout.command),
            touch_rect(self.layout.selection),
        ];
        if let Some(mobile) = self.mobile_layout.as_ref() {
            for rect in [
                mobile.console,
                mobile.top_meta,
                mobile.groups_overlay,
                mobile.commands_overlay,
            ] {
                if rect.w > 0.0 && rect.h > 0.0 {
                    rects.push(touch_rect(rect));
                }
            }
        }
        rects
    }

    /// Screen rect the tooltip is anchored to: the pressed card slot, group
    /// chip or army icon, else the selection panel (idle unit tooltip).
    fn tooltip_anchor_rect(&self) -> Option<Rect> {
        let layout = self.mobile_layout.as_ref()?;
        if let Some(index) = self.touch.pressed_button() {
            if index >= ARMY_TARGET_BASE {
                return layout.army_slots.get(index - ARMY_TARGET_BASE).copied();
            }
            if index >= GROUP_TARGET_BASE {
                let slot = index - GROUP_TARGET_BASE;
                return self.group_row_slots().get(slot).copied();
            }
            if let Some(slot) = layout.card_slots.get(index) {
                return Some(*slot);
            }
        }
        Some(layout.selection)
    }

    fn apply_mobile_button_rects(&mut self) {
        let Some(mobile) = self.mobile_layout.clone() else {
            return;
        };
        for button in &mut self.buttons {
            button.rect = Rect::new(0.0, 0.0, 0.0, 0.0);
        }
        // Compact landscape draws the card in a drawer: while it is closed the
        // 12 actions have no screen rect, and tapping the COMMANDS control is
        // the only way to reach them (the buttons keep their indices/actions).
        if mobile.card_slots.is_empty() {
            return;
        }
        for (slot, index) in self.card_button_indices().iter().enumerate() {
            // A drawer may expose fewer cells than the logical card (a very
            // short window); the remaining actions stay list-only.
            let Some(rect) = mobile.card_slots.get(slot).copied() else {
                break;
            };
            if let Some(button) = self.buttons.get_mut(*index) {
                button.rect = rect;
            }
        }
    }


    /// Device self-test: `files/autotest.txt` commands, one per 45 ticks.
    ///
    /// Commands: `select_workers`, `move <x> <y>`, `build <def> <x> <y>`,
    /// `harvest <x> <y>`. Every step logs `AUTOTEST <cmd> ok|err=<reason>` so a
    /// device run can prove the interaction loop without hand taps.
    fn load_autotest(&mut self) {
        let path = std::path::Path::new("/data/data/org.herofolends.p0/files/autotest.txt");
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        self.autotest = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_string)
            .collect();
        self.log(format!("AUTOTEST loaded steps={}", self.autotest.len()));
    }

    fn run_autotest(&mut self) {
        if self.autotest_step >= self.autotest.len() {
            return;
        }
        self.autotest_wait = self.autotest_wait.saturating_add(1);
        if self.autotest_wait < 45 {
            return;
        }
        self.autotest_wait = 0;
        let step = self.autotest[self.autotest_step].clone();
        self.autotest_step += 1;
        let parts: Vec<&str> = step.split_whitespace().collect();
        let result: Result<String, String> = match parts.as_slice() {
            ["select_workers"] => {
                let workers: Vec<UnitId> = self
                    .sim
                    .alive_units()
                    .filter(|unit| unit.owner == self.player && unit.def().stats.is_worker)
                    .map(|unit| unit.id)
                    .collect();
                self.selection = workers.clone();
                Ok(format!("selected={}", workers.len()))
            }
            ["move", x, y] => {
                let to = Vec2::new(x.parse().unwrap_or(0.0), y.parse().unwrap_or(0.0));
                let units = self.selected_army().unwrap_or_default();
                if units.is_empty() {
                    Err("no selection".to_string())
                } else {
                    self.sim.issue(&units, wc3_sim::OrderSpec::Move(to));
                    let paths: usize = units
                        .iter()
                        .filter_map(|id| self.sim.unit(*id))
                        .map(|unit| unit.path.len())
                        .sum();
                    Ok(format!("units={} paths={paths}", units.len()))
                }
            }
            ["harvest", x, y] => {
                let to = Vec2::new(x.parse().unwrap_or(0.0), y.parse().unwrap_or(0.0));
                let units = self.selected_worker().unwrap_or_default();
                if units.is_empty() {
                    Err("no worker".to_string())
                } else {
                    self.sim.issue(&units, wc3_sim::OrderSpec::Harvest(to));
                    Ok(format!("workers={}", units.len()))
                }
            }
            ["build_first", x, y] => {
                let to = Vec2::new(x.parse().unwrap_or(0.0), y.parse().unwrap_or(0.0));
                let def = crate::sim_bridge::build_menu(&self.sim, self.player)
                    .first()
                    .map(|entry| entry.def_id);
                let units = self.selected_worker().unwrap_or_default();
                match (def, units.first().copied()) {
                    (None, _) => Err("empty build menu".to_string()),
                    (Some(_), None) => Err("no worker".to_string()),
                    (Some(def), Some(worker)) => self
                        .sim
                        .place_building_ordered(worker, def, to)
                        .map(|id| format!("{def} site={}", id.0))
                        .map_err(|error| error.to_string()),
                }
            }
            ["build", def, x, y] => {
                let to = Vec2::new(x.parse().unwrap_or(0.0), y.parse().unwrap_or(0.0));
                let units = self.selected_worker().unwrap_or_default();
                match units.first().copied() {
                    None => Err("no worker".to_string()),
                    Some(worker) => self
                        .sim
                        .place_building_ordered(worker, def, to)
                        .map(|id| format!("site={}", id.0))
                        .map_err(|error| error.to_string()),
                }
            }
            _ => Err(format!("unknown step {step}")),
        };
        match result {
            Ok(detail) => self.log(format!("AUTOTEST {step} ok {detail}")),
            Err(error) => self.log(format!("AUTOTEST {step} err={error}")),
        }
    }

    /// Number of worker build pages (three entries per page).
    pub fn build_pages(&self) -> usize {
        let menu = crate::sim_bridge::build_menu(&self.sim, self.player);
        menu.len().div_ceil(3).max(1)
    }

    /// Horizontal distance from the audio listener (the camera target that
    /// `update_audio` feeds the mixer) to a world position, in whole units.
    /// Logged on every voice line so the device earshot gate can prove that the
    /// mixer only plays what the camera can hear.
    pub fn audio_distance(&self, pos: Vec2) -> f32 {
        let listener = self.audio_listener.unwrap_or(self.camera.target);
        (pos.distance(listener)).round()
    }

    /// Per-unit acknowledgement voice (`UnitUI.unitSound` -> `UnitAckSounds`).
    /// Acknowledgement played on selection, from the unit's own
    /// `UnitUI.unitSound` set (`{set}Yes1..3/What1..2`). The resolved
    /// definition is logged as `GAME_AUDIO_ACK` for device QA, so a shared or
    /// wrong set is visible in logcat.
    pub fn play_unit_ack(&mut self, id: UnitId) {
        let Some(unit) = self.sim.unit(id) else {
            return;
        };
        let def_id = unit.def().id;
        let pos = Vec3::new(
            unit.pos.x,
            self.assets.height_at(unit.pos.x, unit.pos.y),
            unit.pos.y,
        );
        if self.assets.unit_sound_set(def_id).is_none() {
            return;
        }
        // Retail selection tap: every fourth consecutive tap on the same unit
        // plays `{set}Pissed`, the others `{set}What`/`{set}Yes`.
        let played = self
            .assets
            .audio
            .as_mut()
            .and_then(|audio| audio.selection_tap(def_id, Some(pos)));
        let last = self
            .assets
            .audio
            .as_ref()
            .and_then(|audio| audio.last_event_line())
            .unwrap_or("-")
            .to_string();
        let dist = self.audio_distance(unit.pos);
        match played {
            Some(voice) => {
                crate::dev_log!(
                    "GAME_AUDIO_ACK unit={def_id} voice={} dist={dist:.0} {last}",
                    voice.0
                )
            }
            None => crate::dev_log!("GAME_AUDIO_ACK unit={def_id} def=none dist={dist:.0}"),
        }
    }

    /// Attack acknowledgement from the unit's own sound set
    /// (`{set}YesAttack1..3`, set `What` fallback).
    fn play_unit_attack(&mut self, id: UnitId) {
        let Some(unit) = self.sim.unit(id) else {
            return;
        };
        let def_id = unit.def().id;
        let pos = Vec3::new(
            unit.pos.x,
            self.assets.height_at(unit.pos.x, unit.pos.y),
            unit.pos.y,
        );
        let Some(set) = self.assets.unit_sound_set(def_id) else {
            return;
        };
        if let Some((definition, voice)) = self
            .assets
            .audio
            .as_mut()
            .and_then(|audio| audio.unit_attack_set(&set, Some(pos)))
        {
            crate::dev_log!(
                "GAME_AUDIO_ATTACK unit={def_id} set={set} def={definition} voice={} dist={:.0}",
                voice.0,
                self.audio_distance(unit.pos)
            );
        }
    }

    /// Death line from the unit's own sound set (`{set}Death`; the set's
    /// `What` lines are the fallback only when no death row exists).
    fn play_unit_death(&mut self, id: UnitId) {
        let Some(unit) = self.sim.unit(id) else {
            return;
        };
        let def_id = unit.def().id;
        let pos = Vec3::new(
            unit.pos.x,
            self.assets.height_at(unit.pos.x, unit.pos.y),
            unit.pos.y,
        );
        let Some(set) = self.assets.unit_sound_set(def_id) else {
            return;
        };
        if let Some((definition, voice)) = self
            .assets
            .audio
            .as_mut()
            .and_then(|audio| audio.unit_death_set(&set, Some(pos)))
        {
            crate::dev_log!(
                "GAME_AUDIO_DEATH unit={def_id} set={set} def={definition} voice={} dist={:.0}",
                voice.0,
                self.audio_distance(unit.pos)
            );
        }
    }

    /// Training-complete line from the unit's own sound set (`{set}Ready`).
    fn play_unit_ready(&mut self, id: UnitId) {
        let Some(unit) = self.sim.unit(id) else {
            return;
        };
        let def_id = unit.def().id;
        let pos = Vec3::new(
            unit.pos.x,
            self.assets.height_at(unit.pos.x, unit.pos.y),
            unit.pos.y,
        );
        let Some(set) = self.assets.unit_sound_set(def_id) else {
            return;
        };
        if let Some((definition, voice)) = self
            .assets
            .audio
            .as_mut()
            .and_then(|audio| audio.unit_ready_set(&set, Some(pos)))
        {
            crate::dev_log!(
                "GAME_AUDIO_READY unit={def_id} set={set} def={definition} voice={} dist={:.0}",
                voice.0,
                self.audio_distance(unit.pos)
            );
        }
    }

    /// Enter build placement with the ghost anchored immediately (WC3 shows
    /// the translucent building under the finger the moment BUILD is pressed).
    pub fn begin_placement(&mut self, def_id: &'static str) {
        self.placing = Some(def_id);
        self.placing_at = self
            .camera
            .screen_to_world(self.viewport * 0.5, |x, z| self.assets.height_at(x, z))
            .or(Some(self.camera.target));
        self.ghost_cell = None;
        self.log_ghost();
        let name = wc3_sim::def_of(def_id)
            .map(|def| def.name)
            .unwrap_or(def_id);
        self.toast(format!("PLACING {name}"), 1.6);
    }

    /// The placement ghost the renderer draws: the real MDX of `def_id` at the
    /// pointer, 50% alpha, green when the footprint is legal and red when not.
    pub fn ghost(&self) -> Option<GhostState> {
        let def_id = self.placing?;
        let at = self.placing_at?;
        Some(GhostState {
            def_id,
            at,
            // One placement predicate for ghost, card and placement: geometry,
            // requirements and cost. `can_place` is geometry only, so a Watch
            // Tower without its War Mill showed a green ghost and then a denied
            // placement.
            legal: self
                .sim
                .place_denial(self.player, def_id, at)
                .is_none(),
        })
    }

    /// Device/trace evidence for the ghost: one line per cell change, plus the
    /// placement attempt. Dedupes pointer moves inside the same grid cell.
    fn log_ghost(&mut self) {
        let Some(ghost) = self.ghost() else {
            self.ghost_cell = None;
            return;
        };
        let cell = self.sim.grid.world_to_cell(ghost.at);
        if self.ghost_cell == Some(cell) {
            return;
        }
        self.ghost_cell = Some(cell);
        self.log(format!(
            "GHOST def={} at=({:.0},{:.0}) legal={}",
            ghost.def_id, ghost.at.x, ghost.at.y, ghost.legal
        ));
    }

    pub fn status_line(&self) -> String {
        // Bounded selection summary for device probes: the def id of each
        // selected unit (max 6) plus lumber, so a harvest order can be proven
        // from the periodic GAME_STATE line alone.
        let sel_defs: Vec<String> = self
            .selection
            .iter()
            .take(6)
            .filter_map(|id| self.sim.unit(*id).map(|unit| unit.def().id.to_string()))
            .collect();
        format!(
            "phase={:?} units={} army={} enemy={} gold={} lumber={} sel_defs=[{}] supply={}/{} selected={} trained={} kills={} tick={} fps={:.0}",
            self.phase,
            self.sim.alive_units().count(),
            self.army_count(),
            self.enemy_count(),
            self.sim.resources[self.player as usize],
            self.sim.lumber[self.player as usize],
            sel_defs.join(","),
            self.supply().0,
            self.supply().1,
            self.selection.len(),
            self.total_trained,
            self.total_kills,
            self.sim.tick,
            self.fps
        )
    }

    /// Queue a simulation command (used by the app and by tests).
    pub fn issue(&mut self, command: Command) {
        // Order feedback through the unit's own retail sound set.
        if let Some(leader) = self.selection.first().copied() {
            match &command {
                Command::Attack { .. } => self.play_unit_attack(leader),
                Command::Move { .. } | Command::MoveAttack { .. } => self.play_unit_ack(leader),
                _ => {}
            }
        }
        // Code-issued orders are input: without this the boot menu gate keeps
        // `update` from ever ticking the sim (measured: 2400 updates, tick=0).
        self.input_seen = true;
        self.pending.push(command);
    }

    /// Run a command-card action from code (the HUD calls the same path).
    pub fn activate_action(&mut self, action: UiAction) {
        self.input_seen = true;
        if let Some(audio) = self.assets.audio.as_mut() {
            let _ = audio.ui_click();
        }
        self.activate(action);
    }

    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }

    fn log(&mut self, line: String) {
        // Gameplay evidence (`GAME_MATCH_END`, `GAME_SCORE`, `GAME_MENU`,
        // `GAME_PAUSE`, …) must reach the platform log/the headless stdout,
        // not only the in-memory ring the HUD console shows.
        if line.starts_with("GAME_") {
            crate::dev_log!("{line}");
        }
        self.log.push(line);
    }

    pub fn army_count(&self) -> usize {
        self.sim
            .alive_units()
            .filter(|unit| unit.owner == self.player && !unit.is_building())
            .count()
    }

    /// Placeholder chips the stored HUD layout is solved for: one per selected
    /// unit, the same 8-slot cap the draw path uses. Icons carry no geometry,
    /// so the layout only needs the count to page the army row.
    fn army_layout_chips(&self) -> Vec<ArmyChip> {
        vec![ArmyChip::default(); self.selection.len().min(8)]
    }

    /// True selection size (the chip row is capped at 8, the strip at 6).
    fn selection_count_for_layout(&self) -> usize {
        if self.selection.is_empty() && self.selected_mine.is_some() {
            1
        } else {
            self.selection.len()
        }
    }

    pub fn enemy_count(&self) -> usize {
        self.sim
            .alive_units()
            .filter(|unit| unit.owner == self.enemy && !unit.is_building())
            .count()
    }

    pub fn selected_army(&self) -> Option<Vec<UnitId>> {
        let ids: Vec<UnitId> = self
            .selection
            .iter()
            .copied()
            .filter(|id| {
                self.sim
                    .unit(*id)
                    .map(|unit| unit.alive && unit.owner == self.player && !unit.is_building())
                    .unwrap_or(false)
            })
            .collect();
        if ids.is_empty() { None } else { Some(ids) }
    }

    pub fn selected_hall(&self) -> Option<UnitId> {
        self.selection.iter().copied().find(|id| {
            self.sim
                .unit(*id)
                .map(|unit| {
                    unit.alive
                        && unit.owner == self.player
                        && unit.is_building()
                        && unit.def().is_main_hall()
                })
                .unwrap_or(false)
        })
    }

    /// The selected building that can produce units: the main hall, and also a
    /// barracks/altar/workshop — any building whose shipped `Trains=` list (or
    /// the roster `trains` list) is non-empty.
    ///
    /// Why: `UiAction::TrainDef` was gated on [`Self::selected_hall`], so a
    /// barracks card rendered its footman row and the tap silently no-oped —
    /// a human could never produce a combat unit.
    pub fn selected_producer(&self) -> Option<UnitId> {
        self.selection.iter().copied().find(|id| {
            self.sim
                .unit(*id)
                .map(|unit| {
                    unit.alive
                        && unit.owner == self.player
                        && unit.is_building()
                        && (!unit.def().trains.is_empty()
                            || !self.assets.train_menu(unit.def().id).is_empty())
                })
                .unwrap_or(false)
        })
    }

    /// The selected shop/tavern/mercenary building: sells items or units.
    pub fn selected_shop(&self) -> Option<UnitId> {
        self.selection.iter().copied().find(|id| {
            self.sim
                .unit(*id)
                .map(|unit| {
                    unit.alive
                        && unit.is_building()
                        && (!unit.def().sells_items.is_empty()
                            || !self.sim.shop_units(*id).is_empty()
                            || self.sim.shop(*id).is_some())
                })
                .unwrap_or(false)
        })
    }

    /// The first selected player-owned building (any kind).
    pub fn selected_building(&self) -> Option<UnitId> {
        self.selection.iter().copied().find(|id| {
            self.sim
                .unit(*id)
                .map(|unit| unit.alive && unit.owner == self.player && unit.is_building())
                .unwrap_or(false)
        })
    }

    /// `GAME_TRAIN <def> hall=<id>` / `GAME_TRAIN <def> barracks=<id>`: the
    /// device harness and the production acceptance test read the producer.
    fn train_log_line(&self, def_id: &str, producer: UnitId) -> String {
        let hall = self
            .sim
            .unit(producer)
            .map(|unit| unit.def().is_main_hall())
            .unwrap_or(false);
        let slot = if hall { "hall" } else { "barracks" };
        format!("GAME_TRAIN {def_id} {slot}={}", producer.0)
    }

    /// Verbs whose game-layer dispatch is live but whose card row still lives in
    /// the HUD authoring file: one `UI_MISSING` line per scenario names the gap
    /// instead of leaving the player with a silent no-op.
    fn log_ui_surface_gaps(&mut self) {
        for line in [
            "UI_MISSING research_upgrade reason=no-card-row",
            "UI_MISSING pickup_item reason=no-ground-item-tap",
            "UI_MISSING buy_item reason=no-shop-card",
            "UI_MISSING buy_unit reason=no-shop-card",
            "UI_MISSING cancel_construction reason=no-cancel-row",
        ] {
            self.log(line.to_string());
        }
    }

    pub fn selected_worker(&self) -> Option<Vec<UnitId>> {
        let ids: Vec<UnitId> = self
            .selection
            .iter()
            .copied()
            .filter(|id| {
                self.sim
                    .unit(*id)
                    .map(|unit| {
                        unit.alive && unit.owner == self.player && unit.def().stats.is_worker
                    })
                    .unwrap_or(false)
            })
            .collect();
        if ids.is_empty() { None } else { Some(ids) }
    }

    pub fn can_afford(&self, kind: UnitKind) -> bool {
        self.sim.resources[self.player as usize] >= kind.stats().cost
    }

    /// Affordability for data-driven roster entries (farm, tower, …).
    pub fn can_afford_def(&self, def_id: &str) -> bool {
        crate::sim_bridge::can_afford_def(&self.sim, self.player, def_id)
    }

    /// Food used / cap straight from the canonical simulation.
    pub fn supply(&self) -> (i32, i32) {
        crate::sim_bridge::supply(&self.sim, self.player)
    }

    pub fn upkeep_label(&self) -> &'static str {
        crate::sim_bridge::upkeep_label(&self.sim, self.player)
    }

    /// The fantasy-atlas HUD skin, when the install ships `ui_atlas.png`.
    /// Every HUD draw site falls back to the retail chrome on `None`.
    pub fn hud_skin(&self) -> Option<&crate::hudskin::Skin> {
        self.assets.hud_skin()
    }

    /// Sub-slice dialog skin (row/button/banner variants) for the quest log.
    pub fn hud_skin_dialog(&self) -> Option<&crate::hudskin::Skin> {
        self.assets.hud_skin_dialog()
    }

    pub fn player_hall(&self) -> Option<UnitId> {
        self.sim.main_hall(self.player)
    }

    pub fn update(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        self.elapsed += dt;
        // Retail boot: a real install session opens on the main menu. A host
        // that injects input before the first frame (probes, sim tests) drives
        // the battle directly and is never interrupted by a glue screen.
        if self.boot_menu {
            self.boot_menu = false;
            if !self.input_seen {
                self.open_main_menu();
            }
        }
        if self.screen != Screen::Playing {
            self.tick_screen(dt);
            return;
        }
        self.frames += 1;
        self.update_audio();
        let instant = if dt > 0.0 { 1.0 / dt } else { 0.0 };
        self.fps = if self.fps == 0.0 {
            instant
        } else {
            self.fps * 0.92 + instant * 0.08
        };
        if let Some((_, remaining)) = &mut self.toast {
            *remaining -= dt;
            if *remaining <= 0.0 {
                self.toast = None;
            }
        }
        for entry in &mut self.toast_queue {
            entry.1 -= dt;
        }
        self.toast_queue.retain(|(_, remaining)| *remaining > 0.0);
        for label in &mut self.floating {
            label.elapsed += dt;
        }
        self.floating.retain(|label| label.elapsed < label.life);
        if let Some((_, remaining)) = &mut self.move_marker {
            *remaining -= dt;
            if *remaining <= 0.0 {
                self.move_marker = None;
            }
        }
        if self.paused {
            // WC3 single-player pause: the sim does not tick, but the frame
            // keeps running (camera, HUD, render). Orders tapped while paused
            // stay queued and execute on resume.
            self.drive_camera(dt);
            self.refresh_buttons();
            return;
        }
        if self.phase != Phase::Playing {
            return;
        }
        self.drive_camera(dt);
        self.update_tooltip();
        self.accumulator += dt;
        let mut steps = 0;
        while self.accumulator >= TICK_SECONDS && steps < 5 {
            self.accumulator -= TICK_SECONDS;
            steps += 1;
            let commands = std::mem::take(&mut self.pending);
            self.score[self.player as usize].commands += commands.len() as u32;
            if let (Some(engine), Some(bridge)) = (self.engine.as_mut(), self.bridge.as_mut()) {
                // Swap the canonical sim into the bridge for the tick, then
                // take it back: the bridge's sim field is a scratch buffer
                // between frames, so no clone or allocation per tick.
                std::mem::swap(&mut self.sim, &mut bridge.sim);
                bridge.step_engine(engine, f64::from(TICK_SECONDS), &commands);
                std::mem::swap(&mut self.sim, &mut bridge.sim);
            } else {
                self.sim.step(&commands);
            }
            self.process_events();
            if self.phase != Phase::Playing {
                break;
            }
        }
        if self.sim.tick % 30 == 0
            && let Some(id) = self.selection.first().copied()
            && let Some(unit) = self.sim.unit(id)
        {
            let line = format!(
                "TRACE unit={} pos=({:.0},{:.0}) path={} idx={} goal={:?} order={:?} cell={:?} walkable={}",
                id.0,
                unit.pos.x,
                unit.pos.y,
                unit.path.len(),
                unit.path_index,
                unit.goal,
                unit.order,
                self.sim.grid.world_to_cell(unit.pos),
                self.sim.grid.walkable(
                    self.sim.grid.world_to_cell(unit.pos).0,
                    self.sim.grid.world_to_cell(unit.pos).1
                )
            );
            if std::env::var_os("H2_TAP_DEBUG").is_some() {
                eprintln!("[trace] {line}");
            }
            self.log(line);
        }
        self.run_autotest();
        self.sync_visuals(dt);
        self.prune_selection();
        self.refresh_buttons();
    }

    /// Continuous camera motion: drag-box edge scroll. The band arithmetic is
    /// owned by `wc3-input`; the camera owns pixels -> world units.
    fn drive_camera(&mut self, dt: f32) {
        let bounds = wc3_input::Rect::new(
            self.safe_insets.left,
            self.safe_insets.top,
            (self.viewport.x - self.safe_insets.left - self.safe_insets.right).max(1.0),
            (self.viewport.y - self.safe_insets.top - self.safe_insets.bottom).max(1.0),
        );
        let delta = self.touch.edge_pan(dt, bounds, self.layout.scale);
        if delta != Vec2::ZERO {
            self.camera.pan_pixels(delta);
        }
    }

    /// Long-press on the world shows the WC3 tooltip of the unit under the
    /// finger; releasing or starting a drag clears it.
    fn update_tooltip(&mut self) {
        if !self.touch.is_active() {
            self.long_press_applied = false;
        }
        let pressed = self.touch.long_press(self.elapsed);
        // ui.selection.add: a long-press on a friendly unit joins the group
        // (touch parity with Shift-click) while the tooltip still shows.
        if let Some(point) = pressed
            && !self.long_press_applied
            && let Some(id) = self.pick_unit(point)
            && let Some(unit) = self.sim.unit(id)
            && unit.owner == self.player
        {
            crate::ui::selection::long_press_add(self, id);
            self.long_press_applied = true;
            self.push_haptic(HapticCue::Select);
        }
        let tooltip = pressed
            .and_then(|point| self.pick_unit(point))
            .and_then(|id| self.sim.unit(id))
            .map(|unit| {
                let name = unit.kind.name().to_ascii_uppercase();
                let base = self
                    .assets
                    .unit_tooltip(unit.kind)
                    .map(|text| game_tooltip_text(&text, 90))
                    .filter(|text| !text.is_empty())
                    .unwrap_or_else(|| name.clone());
                format!("{name}  ·  {base}  ·  {} HP", unit.hp.max(0))
            });
        self.tooltip_text = tooltip;
    }

    // ------------------------------------------------------ control groups

    /// Keyboard parity for the WC3 card and control groups. The platform layer
    /// forwards SDL key events as `Hotkey` values.
    pub fn hotkey(&mut self, key: wc3_input::Hotkey, ctrl: bool) {
        self.input_seen = true;
        // A selection changed by code (group recall, tests, AI) must rebuild
        // the card before a letter resolves against it.
        if self.card_selection != self.selection {
            self.refresh_buttons();
        }
        match key {
            wc3_input::Hotkey::Digit(digit) => {
                if ctrl {
                    self.assign_group(digit);
                } else {
                    let double = self.group_press.is_some_and(|(slot, time)| {
                        slot == digit && self.elapsed - time <= 0.4
                    });
                    self.recall_group(digit, double);
                }
            }
            wc3_input::Hotkey::Letter(letter) => self.activate_hotkey(letter),
            wc3_input::Hotkey::Escape => {
                if self.screen != Screen::Playing {
                    self.menu_back();
                    return;
                }
                if self.dialog.is_some() {
                    self.dialog = None;
                    return;
                }
                if self.placing.is_some()
                    || self.attack_move
                    || self.move_order
                    || self.patrol_order
                    || self.targeting.is_some()
                {
                    // Escape cancels the pending order mode first (WC3 order).
                    self.placing = None;
                    self.placing_at = None;
                    self.ghost_cell = None;
                    self.attack_move = false;
                    self.move_order = false;
                    self.patrol_order = false;
                    self.targeting = None;
                    self.touch.cancel();
                    return;
                }
                // BACK with nothing to cancel opens the game menu (F10 parity).
                self.open_game_menu();
            }
        }
    }

    /// Platform helper: raw character + Ctrl state from SDL. Unknown keys are
    /// ignored; the parser lives in `wc3-input` so the platform never maps keys.
    pub fn key_char(&mut self, key: char, ctrl: bool) {
        self.input_seen = true;
        if let Some(hotkey) = Hotkey::parse(key) {
            self.hotkey(hotkey, ctrl);
        }
    }

    /// Host "add to selection" modifier (Shift). The touch router reports it
    /// with the gesture; taps, boxes and long-press picks merge instead of
    /// replacing the selection (WC3 Shift-click / Shift-box).
    pub fn set_additive(&mut self, additive: bool) {
        self.touch.set_additive(additive);
    }

    /// Card letter for an action, resolved through the live remap table
    /// (`ui.hotkeys.remap`). `hotkey_label` stays the static WC3 default for
    /// drawing; input resolution and `set_hotkey` use this table.
    pub fn hotkey_letter(&self, action: UiAction) -> Option<char> {
        self.hotkeys
            .action_letter(crate::ui::hotkeys::action_id(action))
    }

    /// Rebind one card action at runtime (configurable remap table).
    pub fn set_hotkey(&mut self, action: UiAction, key: char) {
        self.hotkeys
            .set_action(crate::ui::hotkeys::action_id(action), key);
        self.refresh_buttons();
    }

    /// Tab / Shift+Tab: cycle the active subgroup (hero first, then unit types).
    pub fn cycle_subgroups(&mut self, reverse: bool) {
        let units = crate::ui::selection::snapshot(self);
        let selected_ids: Vec<u32> = self.selection.iter().map(|id| id.0).collect();
        let mut subgroups = wc3_input::subgroups::Subgroups::build(&units, &selected_ids);
        if subgroups.order().is_empty() {
            return;
        }
        // Continue from the last active kind while it is still selected.
        if let Some(current) = self.subgroup_kind {
            let mut guard = 0;
            while subgroups.active() != Some(current) && guard <= subgroups.order().len() {
                subgroups.cycle(false);
                guard += 1;
            }
        }
        let kind = subgroups.cycle(reverse);
        self.subgroup_kind = kind;
        let label = kind
            .and_then(|kind| {
                self.selection
                    .iter()
                    .filter_map(|id| self.sim.unit(*id))
                    .find(|unit| unit.kind as u32 == kind)
                    .map(|unit| unit.kind.name())
            })
            .unwrap_or("none");
        self.log(format!("SUBGROUP {label}"));
    }

    pub fn assign_group(&mut self, slot: u8) {
        let units: Vec<u32> = self.selection.iter().map(|id| id.0).collect();
        if units.is_empty() {
            self.toast("SELECT UNITS FIRST", 1.4);
            self.push_haptic(HapticCue::Deny);
            return;
        }
        self.control_groups.assign(slot, &units);
        self.toast(format!("GROUP {} SAVED ({} UNITS)", slot, units.len()), 1.4);
        self.push_haptic(HapticCue::Order);
    }

    pub fn recall_group(&mut self, slot: u8, centre: bool) {
        let Some(units) = self.control_groups.recall(slot) else {
            self.toast(format!("GROUP {} EMPTY", slot), 1.2);
            self.push_haptic(HapticCue::Deny);
            return;
        };
        let ids: Vec<UnitId> = units
            .iter()
            .copied()
            .filter(|id| {
                self.sim
                    .unit(UnitId(*id))
                    .map(|unit| unit.alive)
                    .unwrap_or(false)
            })
            .map(UnitId)
            .collect();
        if ids.is_empty() {
            self.toast(format!("GROUP {} GONE", slot), 1.2);
            self.push_haptic(HapticCue::Deny);
            return;
        }
        self.selection = ids.clone();
        if centre && let Some(first) = ids.first().and_then(|id| self.sim.unit(*id)) {
            self.camera.centre_on(first.pos);
        }
        // The command card belongs to the recalled group from this tap on.
        self.refresh_buttons();
        self.toast(
            format!("GROUP {} ({} UNITS)", slot, ids.len()),
            1.4,
        );
        self.group_press = Some((slot, self.elapsed));
        self.push_haptic(HapticCue::Select);
    }

    /// Touch parity for control groups on the ARMY chip: hold = save the
    /// current selection to the next free group, double tap = cycle groups.
    fn save_group_from_touch(&mut self) {
        let Some(slot) = self.control_groups.next_free() else {
            self.toast("ALL GROUPS FULL", 1.4);
            self.push_haptic(HapticCue::Deny);
            return;
        };
        self.assign_group(slot);
    }

    fn cycle_group_from_touch(&mut self) {
        if self.control_groups.is_empty_all() {
            self.toast("NO CONTROL GROUPS", 1.4);
            self.push_haptic(HapticCue::Deny);
            return;
        }
        let after = self.group_press.map(|(slot, _)| slot).unwrap_or(0);
        if let Some(slot) = self.control_groups.next_non_empty(after) {
            self.recall_group(slot, true);
        }
    }

    // ------------------------------------------- on-screen control groups

    /// Ten WC3-style group buttons (1..9,0) centred in the console dead space,
    /// clear of the minimap/selection/card so they never steal a HUD touch.
    pub fn group_row_slots(&self) -> [Rect; 10] {
        // The layout owns the control-group targets: 3x3 tray for IDs 1..9 plus
        // the tenth slot (ID `0`) — the same ten slots `ControlGroups` proves
        // (`groups: [Vec<u32>; 10]`, `slot_order` 1..9 then 0). When the tray is
        // collapsed into the drawer the slots are zero-sized until it opens;
        // the GROUPS button is then the only group control.
        if let Some(mobile) = self.mobile_layout.as_ref()
            && mobile.group_slots.len() == 10
        {
            return std::array::from_fn(|index| mobile.group_slots[index]);
        }
        [Rect::new(0.0, 0.0, 0.0, 0.0); 10]
    }

    /// Redrawn HUD art when the redraw pack tiles are the loaded ones. The
    /// pack tiles are 256x288; retail `HumanUITile01..04` are 512x512, so the
    /// dimension check keeps the retail layout untouched when the pack is absent.
    fn hud_redraw_skin(&self) -> Option<RedrawSkin> {
        let ui = &self.assets.ui;
        let is_pack = ui
            .texture_info("HumanUITile01")
            .is_some_and(|info| info.width == 256 && info.height == 288);
        if !is_pack {
            return None;
        }
        let skin = RedrawSkin {
            console_left_cap: ui.texture("HumanUITile01"),
            console_left_mid: ui.texture("HumanUITile02"),
            console_right_mid: ui.texture("HumanUITile03"),
            console_right_cap: ui.texture("HumanUITile04"),
            topbar: ui.texture("HudRedrawTopbar"),
            button: [
                ui.texture("HudRedrawButtonNormal").or(ui.texture("CommandButtonBorder")),
                ui.texture("HudRedrawButtonPressed"),
                ui.texture("HudRedrawButtonDisabled"),
                ui.texture("HudRedrawButtonCooldown"),
                ui.texture("HudRedrawButtonAutocast"),
                ui.texture("HudRedrawButtonEmpty"),
            ],
            portrait: [
                ui.texture("HudRedrawPortraitNormal").or(ui.texture("PortraitFrame")),
                ui.texture("HudRedrawPortraitHero"),
                ui.texture("HudRedrawPortraitEnemy"),
            ],
            minimap_frame: ui.texture("HudRedrawMinimapFrame"),
            minimap_frame_alert: ui.texture("HudRedrawMinimapFrameAlert"),
            minimap_header: ui.texture("HudRedrawMinimapHeader"),
            pill: ui.texture("HudRedrawPill"),
            pill_warn: ui.texture("HudRedrawPillWarn"),
            selection_panel: ui.texture("HudRedrawSelectionPanel"),
            group_inactive: ui.texture("HudRedrawGroupInactive"),
            group_active: ui.texture("HudRedrawGroupActive"),
            tooltip_panel: ui.texture("HudRedrawTooltipPanel"),
            progress: ui.texture("HudRedrawProgress"),
        };
        skin.has_console().then_some(skin)
    }

    /// Draw the group row: slot number + live unit count; the last recalled
    /// group is highlighted.
    fn draw_group_row(&self, ui: &mut UiBatch) {
        let s = self.layout.scale;
        let border = [0.55, 0.68, 0.82, 0.30];
        let hot = [0.40, 0.72, 1.0, 0.88];
        let dim = [0.72, 0.79, 0.88, 0.78];
        let active = self.group_press.map(|(slot, _)| slot);
        for (index, rect) in self.group_row_slots().iter().enumerate() {
            let slot = if index < 9 { index as u8 + 1 } else { 0 };
            let label = if slot == 0 { "0" } else { &slot.to_string() };
            let count = self.control_groups.len(slot);
            let fill = if count == 0 {
                [0.045, 0.055, 0.075, 0.55]
            } else if active == Some(slot) {
                [0.20, 0.38, 0.62, 0.92]
            } else {
                [0.07, 0.10, 0.14, 0.82]
            };
            let group_texture = if count > 0 || active == Some(slot) {
                self.assets.ui.texture("HudRedrawGroupActive")
            } else {
                self.assets.ui.texture("HudRedrawGroupInactive")
            };
            if let Some(texture) = group_texture {
                ui.textured_quad(texture, *rect, [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
            } else {
                self.assets.chrome.flat_rect(ui, *rect, fill);
                ui.border(*rect, 1.0 * s, if active == Some(slot) { hot } else { border });
            }
            self.font.text_box(
                ui,
                label,
                *rect,
                0.42 * s,
                if count > 0 { dim } else { [0.45, 0.50, 0.56, 0.60] },
                TextAlign::Centre,
                None,
            );
            if count > 0 {
                self.font.text_box(
                    ui,
                    &count.to_string(),
                    *rect,
                    0.28 * s,
                    hot,
                    TextAlign::Centre,
                    None,
                );
            }
        }
    }

    fn hit_group_slot(&mut self, slot_index: usize, hold: bool, double: bool) {
        let slot = if slot_index < 9 { slot_index as u8 + 1 } else { 0 };
        self.push_haptic(HapticCue::Press);
        if hold {
            self.assign_group(slot);
        } else if double {
            self.recall_group(slot, true);
        } else {
            self.recall_group(slot, false);
        }
    }

    /// Selection marquee in screen space while a box drag is active.
    pub fn box_marquee_rect(&self) -> Option<Rect> {
        let (a, b) = self.touch.box_rect()?;
        let min = a.min(b);
        let max = a.max(b);
        Some(Rect::new(
            min.x,
            min.y,
            (max.x - min.x).max(2.0),
            (max.y - min.y).max(2.0),
        ))
    }

    fn draw_box_marquee(&self, ui: &mut UiBatch) {
        let Some(rect) = self.box_marquee_rect() else {
            return;
        };
        ui.rect(rect, [0.25, 0.65, 1.0, 0.10]);
        ui.border(rect, 1.5 * self.layout.scale, [0.45, 0.85, 1.0, 0.85]);
    }

    fn activate_hotkey(&mut self, letter: char) {
        let found = self.buttons.iter().position(|button| {
            if !button.enabled {
                return false;
            }
            if self.hotkey_letter(button.action) == Some(letter) {
                return true;
            }
            button
                .hotkey
                .chars()
                .next()
                .map(|c| c.to_ascii_lowercase())
                == Some(letter)
        });
        let Some(index) = found else {
            return;
        };
        if let Some(audio) = self.assets.audio.as_mut() {
            let _ = audio.ui_click();
        }
        let label = self.buttons[index].label.clone();
        let action = self.buttons[index].action;
        self.log(format!(
            "HOTKEY {} {}",
            letter.to_ascii_uppercase(),
            label
        ));
        self.push_haptic(HapticCue::Press);
        self.activate(action);
    }

    fn push_haptic(&mut self, cue: HapticCue) {
        if self.haptics.len() < 8 {
            self.haptics.push(cue);
        }
    }

    /// Drain tactile cues for the platform vibrator (called once per frame).
    pub fn take_haptics(&mut self) -> Vec<HapticCue> {
        std::mem::take(&mut self.haptics)
    }

    fn process_events(&mut self) {
        let events = self.sim.events.clone();
        for event in events {
            match event {
                Event::Bounty { owner, unit, gold } => {
                    // WC3 kill feedback: a floating "+N" at the victim, plus a
                    // device-log line the QA gate can parse.
                    // The victim id resolves to its last sim position; when the
                    // sim no longer has it, no label is invented (`pos=none`).
                    let pos = self.sim.unit(unit).map(|victim| victim.pos);
                    if let Some(pos) = pos {
                        self.log(format!(
                            "GAME_BOUNTY amount={gold} pos=({:.0},{:.0}) owner={owner}",
                            pos.x, pos.y
                        ));
                        if owner == self.player {
                            self.floating.push(FloatingLabel::new(
                                format!("+{gold} G"),
                                pos,
                                [0.98, 0.82, 0.30, 1.0],
                            ));
                        }
                    } else {
                        self.log(format!(
                            "GAME_BOUNTY amount={gold} pos=none owner={owner}"
                        ));
                    }
                }
                Event::GoldDeposited { owner, amount } => {
                    // WC3 score: "gold mined" is the gross harvest, so the
                    // upkeep tax the sim withheld is added back for the score
                    // and reported separately as upkeep paid.
                    if let Some(score) = self.score.get_mut(owner as usize) {
                        let tier = self.sim.upkeep(owner);
                        let income = tier.income_permille().max(1);
                        let tax = (amount * tier.tax_permille() / income).max(0);
                        score.gold_gathered += (amount + tax).max(0) as u32;
                        score.upkeep_paid += tax as u32;
                    }
                    if owner == self.player {
                        self.log(format!("GAME_DEPOSIT kind=gold amount={amount}"));
                    }
                }
                Event::LumberDeposited { owner, amount } => {
                    if let Some(score) = self.score.get_mut(owner as usize) {
                        score.lumber_gathered += amount.max(0) as u32;
                    }
                    if owner == self.player {
                        self.log(format!("GAME_DEPOSIT kind=lumber amount={amount}"));
                    }
                }
                Event::UnitDied(id) => {
                    if let Some(unit) = self.sim.unit(id) {
                        let owner = unit.owner;
                        let building = unit.is_building();
                        if owner < 2 {
                            let rival = 1 - owner;
                            if building {
                                // "Buildings razed" belongs to the other seat:
                                // damage attribution is not in the event, and a
                                // two-seat match has exactly one opponent.
                                self.score[rival as usize].buildings_razed += 1;
                            } else {
                                self.score[owner as usize].units_lost += 1;
                                self.score[rival as usize].units_killed += 1;
                            }
                        }
                        if owner == self.enemy {
                            self.total_kills += 1;
                        }
                    }
                    // Retail death line from the fallen unit's own sound set.
                    self.play_unit_death(id);
                    // Corpse window: keep the model playing Death for ~1.4 s.
                    if let Some(unit) = self.sim.unit(id) {
                        let def_id = unit.def().id;
                        let pos = unit.pos;
                        let facing = unit.facing;
                        if let Some(model) = self.assets.model_for_def(def_id).model.as_ref() {
                            let mut animator = Animator::from_model(model);
                            if let Some(sequence) = self
                                .assets
                                .model_for_def(def_id)
                                .sequence(SequenceKind::Death)
                                .and_then(|index| {
                                    self.assets
                                        .model_for_def(def_id)
                                        .sequences
                                        .get(index)
                                        .cloned()
                                })
                            {
                                animator.play(&sequence);
                            }
                            self.death_visuals.push(DeathVisual {
                                def_id,
                                owner: unit.owner,
                                pos,
                                facing,
                                animator,
                                elapsed: 0.0,
                            });
                        }
                    }
                }
                Event::BuildingComplete(id) => {
                    if let Some(unit) = self.sim.unit(id) {
                        if let Some(score) = self.score.get_mut(unit.owner as usize) {
                            score.buildings_built += 1;
                        }
                        let position = Vec3::new(unit.pos.x, 0.0, unit.pos.y);
                        if let Some(audio) = self.assets.audio.as_mut()
                            && let Some(race) = audio_race(unit.def().race)
                            && let Some(voice) = audio.job_done(race, Some(position))
                        {
                            crate::dev_log!(
                                "GAME_AUDIO_BUILD_COMPLETE race={} voice={}",
                                race.id(),
                                voice.0
                            );
                        }
                    }
                }
                Event::UpgradeResearched { owner, id } => {
                    if owner == self.player
                        && let Some(race) = self.player_audio_race()
                    {
                        let played = self.assets.audio.as_mut().and_then(|audio| {
                            audio.play_event(
                                wc3_audio::events::AudioEvent::UpgradeComplete,
                                None,
                                Some(race),
                                None,
                            )
                        });
                        if let Some(voice) = played {
                            crate::dev_log!(
                                "GAME_AUDIO_UPGRADE upgrade={id} race={} voice={}",
                                race.id(),
                                voice.0
                            );
                        }
                    }
                }
                Event::UnitTrained(id) => {
                    if let Some(unit) = self.sim.unit(id) {
                        if let Some(score) = self.score.get_mut(unit.owner as usize) {
                            score.units_trained += 1;
                        }
                        if unit.owner == self.player {
                            self.total_trained += 1;
                            self.toast(format!("TRAINED {}", unit.kind.name()), 1.5);
                        }
                    }
                    // Retail ready line from the trained unit's own set.
                    self.play_unit_ready(id);
                }
                Event::Damage { from, to, .. } => {
                    // Retail impact row: weapon class x material class, from the
                    // shipped UnitWeapons/unitUI tables (lane 470).
                    let attacker = self.sim.unit(from).map(|u| u.def().id);
                    let target = self.sim.unit(to).map(|u| u.def().id);
                    let pos = self
                        .sim
                        .unit(to)
                        .map(|u| Vec3::new(u.pos.x, 0.0, u.pos.y));
                    if let Some(audio) = self.assets.audio.as_mut() {
                        let _ = audio.attack_impact(attacker, target, pos);
                    }
                    if let Some(unit) = self.sim.unit(to)
                        && unit.owner == self.player
                        && unit.hp <= 0
                    {
                        self.log(format!("UNIT_LOST {}", unit.kind.name()));
                    }
                }
                Event::Victory(owner) => {
                    // The sim's built-in rule ends a side when its main hall
                    // falls. In melee that is not enough: a seat with a
                    // barracks and workers can rebuild (WC3 base rule), so the
                    // match-end decision belongs to `evaluate_match`. A
                    // scripted map keeps the sim's verdict as its fail-safe.
                    if self.engine.is_some() {
                        self.finish(owner);
                    }
                }
                Event::AiWave { .. } => {
                    if let Some(text) = crate::sim_bridge::wave_toast(&event, self.enemy) {
                        self.toast(text, 2.4);
                    }
                }
                Event::Error(message) => {
                    if message != "gold +10" && message != "production cancelled" {
                        self.toast(message.to_ascii_uppercase(), 1.6);
                        // Retail deny line for the failing resource.
                        let lower = message.to_ascii_lowercase();
                        let event = if lower.contains("gold") {
                            Some(wc3_audio::events::AudioEvent::DenyNoGold)
                        } else if lower.contains("lumber") {
                            Some(wc3_audio::events::AudioEvent::DenyNoLumber)
                        } else if lower.contains("food") || lower.contains("supply") {
                            Some(wc3_audio::events::AudioEvent::DenyNoFood)
                        } else {
                            Some(wc3_audio::events::AudioEvent::UiError)
                        };
                        if let (Some(event), Some(race)) = (event, self.player_audio_race()) {
                            let played = self
                                .assets
                                .audio
                                .as_mut()
                                .and_then(|audio| audio.play_event(event, None, Some(race), None));
                            if let Some(voice) = played {
                                crate::dev_log!(
                                    "GAME_AUDIO_DENY event={} race={} voice={}",
                                    event.id(),
                                    race.id(),
                                    voice.0
                                );
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        // WC3 melee end rule: a seat is out when it has no building left and no
        // worker to put one down. Decided here, after every tick's events.
        self.evaluate_match();
    }

    /// Run the sim event queue through the game layer (score counters, logs,
    /// audio). `update` calls this after every tick; headless QA drivers that
    /// stage sim events (a scripted `KillUnit`, a direct deposit) call it to
    /// exercise the same counters.
    pub fn drain_sim_events(&mut self) {
        self.process_events();
    }

    /// Whether the seat ever owned a unit. `Sim::units` keeps dead units, so
    /// this is stable even when a seat is wiped before its first evaluation.
    fn seat_has_units(&self, seat: u8) -> bool {
        self.sim.units.iter().any(|unit| unit.owner == seat)
    }

    /// Alive buildings and workers of one seat. A seat with either can still
    /// fight: workers rebuild, buildings keep producing.
    fn seat_strength(&self, seat: u8) -> (u32, u32) {
        let mut buildings = 0;
        let mut workers = 0;
        for unit in self.sim.units_of(seat) {
            if unit.is_building() {
                buildings += 1;
            } else if unit.def().stats.is_worker {
                workers += 1;
            }
        }
        (buildings, workers)
    }

    /// The RNG seed this battle runs on (advanced by [`Game::restart`]).
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Battle time from the sim clock (pauses and menus do not count).
    pub fn match_elapsed_s(&self) -> f32 {
        self.sim.tick as f32 * TICK_SECONDS
    }

    /// Evaluate the WC3 melee end rule and finish the match exactly once.
    ///
    /// A seat is **defeated** when every building it owns is destroyed *and*
    /// it has no living worker left that could rebuild one — the retail
    /// "no base, no way back" rule, not merely "the hall died". The match is
    /// **won** by the last seat that is not defeated. When both bases fall on
    /// the same tick the seat with more living units takes it
    /// (`mutual-annihilation`).
    pub fn evaluate_match(&mut self) -> Option<MatchOutcome> {
        if let Some(outcome) = &self.outcome {
            return Some(outcome.clone());
        }
        // Scripted maps decide their own outcome (campaign victory/defeat
        // triggers). Their seat layout is script-authored — a mission can put
        // the player on a slot with no halls — so the melee "no base, no
        // workers" rule must never end such a map on frame one.
        if self.engine.is_some() {
            return self.match_outcome();
        }
        let seats = [self.player, self.enemy];
        for seat in seats {
            if !self.seat_started[seat as usize] && self.seat_has_units(seat) {
                self.seat_started[seat as usize] = true;
            }
        }
        if self.phase != Phase::Playing {
            return self.match_outcome();
        }
        let mut defeated_now = Vec::new();
        for seat in seats {
            if !self.seat_started[seat as usize] {
                continue;
            }
            let (buildings, workers) = self.seat_strength(seat);
            if buildings == 0 && workers == 0 {
                defeated_now.push(seat);
            }
        }
        if defeated_now.is_empty() {
            return None;
        }
        for seat in defeated_now {
            if !self.defeated.contains(&seat) {
                self.defeated.push(seat);
            }
        }
        let survivor = seats
            .iter()
            .copied()
            .find(|seat| !self.defeated.contains(seat));
        let winner = match survivor {
            Some(seat) => seat,
            None => {
                let player_left = self.sim.units_of(self.player).filter(|u| u.alive).count();
                let enemy_left = self.sim.units_of(self.enemy).filter(|u| u.alive).count();
                if player_left >= enemy_left {
                    self.player
                } else {
                    self.enemy
                }
            }
        };
        let outcome = MatchOutcome {
            winner,
            defeated: self.defeated.clone(),
            elapsed_s: self.match_elapsed_s(),
            reason: if survivor.is_some() {
                "no-buildings-or-workers"
            } else {
                "mutual-annihilation"
            },
        };
        self.outcome = Some(outcome.clone());
        self.finish(winner);
        Some(outcome)
    }

    /// The decided result of this match, `None` while it is still running.
    pub fn match_outcome(&self) -> Option<MatchOutcome> {
        self.outcome.clone().or_else(|| match self.phase {
            Phase::Won => Some(MatchOutcome {
                winner: self.player,
                defeated: vec![self.enemy],
                elapsed_s: self.match_elapsed_s(),
                reason: "scripted-victory",
            }),
            Phase::Lost => Some(MatchOutcome {
                winner: self.enemy,
                defeated: vec![self.player],
                elapsed_s: self.match_elapsed_s(),
                reason: "scripted-victory",
            }),
            Phase::Playing => None,
        })
    }

    /// WC3 post-game score rows, local seat first (`GAME_SCORE` payload).
    pub fn score_rows(&self) -> Vec<ScoreRow> {
        let elapsed = self.match_elapsed_s();
        let minutes = (elapsed / 60.0).max(1.0 / 60.0);
        let mut rows = Vec::with_capacity(2);
        for seat in [self.player, self.enemy] {
            let raw = self.score[seat as usize];
            let hero_level = self
                .sim
                .units_of(seat)
                .filter(|unit| unit.alive && unit.def().hero)
                .map(|unit| unit.level)
                .max()
                .unwrap_or(0);
            rows.push(ScoreRow {
                seat,
                units_trained: raw.units_trained,
                units_lost: raw.units_lost,
                units_killed: raw.units_killed,
                buildings_built: raw.buildings_built,
                buildings_razed: raw.buildings_razed,
                gold_gathered: raw.gold_gathered,
                lumber_gathered: raw.lumber_gathered,
                upkeep_paid: raw.upkeep_paid,
                hero_level,
                apm: (raw.commands as f32 / minutes).round() as u32,
            });
        }
        rows
    }

    /// One-line score dump for the device log / headless QA (`GAME_SCORE …`).
    pub fn score_line(&self) -> String {
        let state = match self.phase {
            Phase::Playing => "playing",
            Phase::Won => "won",
            Phase::Lost => "lost",
        };
        let mut line = format!(
            "GAME_SCORE state={state} elapsed_s={:.1}",
            self.match_elapsed_s()
        );
        for row in self.score_rows() {
            line.push_str(&format!(
                " s{}[trained={} lost={} killed={} built={} razed={} gold={} lumber={} upkeep={} hero={} apm={}]",
                row.seat,
                row.units_trained,
                row.units_lost,
                row.units_killed,
                row.buildings_built,
                row.buildings_razed,
                row.gold_gathered,
                row.lumber_gathered,
                row.upkeep_paid,
                row.hero_level,
                row.apm
            ));
        }
        line
    }

    /// The same score rows in the HUD screen's `ScoreData` shape (label/value),
    /// so the screens lane draws real `Game` numbers without knowing the
    /// counter layout. Columns are `YOU / ENEMY`.
    pub fn score_data(&self) -> crate::hud::ScoreData {
        let rows = self.score_rows();
        let local = &rows[0];
        let rival = &rows[1];
        let column = |left: u32, right: u32| format!("{left} / {right}");
        let pairs: Vec<(&str, String)> = vec![
            ("UNITS TRAINED", column(local.units_trained, rival.units_trained)),
            ("UNITS LOST", column(local.units_lost, rival.units_lost)),
            ("UNITS KILLED", column(local.units_killed, rival.units_killed)),
            (
                "BUILDINGS BUILT",
                column(local.buildings_built, rival.buildings_built),
            ),
            (
                "BUILDINGS RAZED",
                column(local.buildings_razed, rival.buildings_razed),
            ),
            ("GOLD MINED", column(local.gold_gathered, rival.gold_gathered)),
            (
                "LUMBER HARVESTED",
                column(local.lumber_gathered, rival.lumber_gathered),
            ),
            ("UPKEEP PAID", column(local.upkeep_paid, rival.upkeep_paid)),
            (
                "HERO LEVEL",
                format!("{} / {}", local.hero_level, rival.hero_level),
            ),
            ("APM", column(local.apm, rival.apm)),
        ];
        let title = match self.phase {
            Phase::Won => "VICTORY",
            Phase::Lost => "DEFEAT",
            Phase::Playing => "SCORE",
        };
        let elapsed = self.match_elapsed_s();
        let subtitle = format!(
            "{}   TIME {}:{:02}   YOU / ENEMY",
            self.assets.map_name,
            (elapsed as u32) / 60,
            (elapsed as u32) % 60
        );
        crate::hud::ScoreData::from_pairs(title, subtitle, pairs)
    }

    /// Write the score dump into the device log (end screen / QA).
    pub fn log_score(&mut self) {
        let line = self.score_line();
        self.log(line);
    }

    /// Directory + path of a pause-menu save slot.
    pub fn save_slot_path(&self, slot: u8) -> PathBuf {
        self.save_dir.join(format!("slot{slot}.sav"))
    }

    /// Point the pause-menu save slots elsewhere (tests, portable installs).
    pub fn set_save_dir(&mut self, dir: impl Into<PathBuf>) {
        self.save_dir = dir.into();
    }

    /// Map identity of this battle. A demo/scenario battle without a map file
    /// gets a zero stamp (`len=0`, `hash=0`), which still round-trips through
    /// [`crate::sim_bridge::CheckpointMeta`].
    fn save_map_stamp(&self) -> crate::sim_bridge::MapStamp {
        self.scenario_map_path()
            .as_deref()
            .and_then(crate::sim_bridge::MapStamp::of_file)
            .unwrap_or(crate::sim_bridge::MapStamp {
                path: String::new(),
                len: 0,
                hash: 0,
            })
    }

    /// Game-side counters that the sim save does not carry: battle clock and
    /// the score screen rows.
    fn match_state_encode(&self) -> String {
        let mut text = format!("h2match=1\nelapsed={:.3}\n", self.elapsed);
        text.push_str(&format!(
            "seat_started={},{}\n",
            u8::from(self.seat_started[0]),
            u8::from(self.seat_started[1])
        ));
        for seat in 0..2 {
            let raw = self.score[seat];
            text.push_str(&format!(
                "score{seat}={},{},{},{},{},{},{},{},{}\n",
                raw.units_trained,
                raw.units_lost,
                raw.units_killed,
                raw.buildings_built,
                raw.buildings_razed,
                raw.gold_gathered,
                raw.lumber_gathered,
                raw.upkeep_paid,
                raw.commands
            ));
        }
        text
    }

    fn match_state_decode(&mut self, text: &str) -> bool {
        let mut seen = false;
        for line in text.lines() {
            let Some((key, value)) = line.trim().split_once('=') else {
                continue;
            };
            match key {
                "h2match" if value == "1" => seen = true,
                "elapsed" => {
                    if let Ok(elapsed) = value.parse::<f32>() {
                        self.elapsed = elapsed;
                    }
                }
                "seat_started" => {
                    if let Some((a, b)) = value.split_once(',') {
                        self.seat_started = [a == "1", b == "1"];
                    }
                }
                "score0" | "score1" => {
                    let seat = if key == "score0" { 0 } else { 1 };
                    let values: Vec<u32> = value
                        .split(',')
                        .filter_map(|item| item.parse::<u32>().ok())
                        .collect();
                    if values.len() == 9 {
                        self.score[seat] = SeatScore {
                            units_trained: values[0],
                            units_lost: values[1],
                            units_killed: values[2],
                            buildings_built: values[3],
                            buildings_razed: values[4],
                            gold_gathered: values[5],
                            lumber_gathered: values[6],
                            upkeep_paid: values[7],
                            commands: values[8],
                        };
                    }
                }
                _ => {}
            }
        }
        seen
    }

    /// Write a resumable pause-menu save: sim payload + `*.sav.meta` gate +
    /// `*.sav.state` score/clock sidecar. Returns the sim save digest.
    pub fn save_match(&self, path: &Path) -> Result<u64, String> {
        if self.engine.is_some() {
            return Err("campaign-engine-state-unsupported".to_string());
        }
        if self.phase != Phase::Playing {
            return Err("match-finished".to_string());
        }
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir).map_err(|error| format!("mkdir {}: {error}", dir.display()))?;
        }
        self.sim
            .save_to_path(path)
            .map_err(|error| format!("{error}"))?;
        let digest = self.sim.save_digest();
        let meta = crate::sim_bridge::CheckpointMeta::new(
            self.save_map_stamp(),
            (self.sim.grid.width, self.sim.grid.height),
            "playing",
            self.sim.tick,
            digest,
        );
        let meta_path = sidecar_path(path, ".meta");
        std::fs::write(&meta_path, meta.encode())
            .map_err(|error| format!("meta {}: {error}", meta_path.display()))?;
        let state_path = sidecar_path(path, ".state");
        std::fs::write(&state_path, self.match_state_encode())
            .map_err(|error| format!("state {}: {error}", state_path.display()))?;
        restrict_save_permissions(path);
        restrict_save_permissions(&meta_path);
        restrict_save_permissions(&state_path);
        Ok(digest)
    }

    /// Restore a pause-menu save produced by [`Game::save_match`].
    ///
    /// The map/grid gate rejects a save from another map (a resumed board on
    /// the wrong terrain is not a save), the digest gate rejects a corrupt
    /// payload, and the score/clock sidecar restores the running score screen.
    /// A campaign engine's JASS world is not part of the sim payload, so those
    /// maps refuse instead of resuming a desynchronised script.
    pub fn load_match(&mut self, path: &Path) -> Result<u64, String> {
        if self.engine.is_some() {
            return Err("campaign-engine-state-unsupported".to_string());
        }
        let meta_path = sidecar_path(path, ".meta");
        let text = std::fs::read_to_string(&meta_path).map_err(|error| {
            format!("meta {}: {error}", meta_path.display())
        })?;
        let meta = crate::sim_bridge::CheckpointMeta::decode(&text)
            .ok_or_else(|| "meta-unreadable".to_string())?;
        meta.verdict(
            Some(&self.save_map_stamp()),
            (self.sim.grid.width, self.sim.grid.height),
        )
        .map_err(|reject| reject.reason().to_string())?;
        let sim = Sim::from_save_path(path).map_err(|error| format!("{error}"))?;
        let digest = sim.save_digest();
        if !meta.matches(sim.tick, digest) {
            return Err(format!(
                "digest saved={:016x} loaded={:016x}",
                meta.digest, digest
            ));
        }
        let was_paused = self.paused || self.screen == Screen::Paused;
        self.sim = sim;
        self.phase = Phase::Playing;
        self.paused = false;
        self.screen = Screen::Playing;
        self.boot_menu = false;
        self.selection.clear();
        self.selected_cells.clear();
        self.visuals.clear();
        self.death_visuals.clear();
        self.pending.clear();
        self.accumulator = 0.0;
        self.placing = None;
        self.placing_at = None;
        self.targeting = None;
        self.attack_move = false;
        self.move_order = false;
        self.patrol_order = false;
        self.dialog = None;
        self.haptics.clear();
        self.outcome = None;
        self.defeated.clear();
        self.seat_started = [false; 2];
        self.score = Default::default();
        // Old saves without a score sidecar resume with a zero score rather
        // than inheriting the counters of the battle that was open.
        if let Ok(state) = std::fs::read_to_string(sidecar_path(path, ".state")) {
            self.match_state_decode(&state);
        }
        self.menu_flow
            .apply(crate::hud::menu::MenuAction::Resume);
        self.refresh_buttons();
        self.sync_menu();
        if was_paused {
            self.log("GAME_PAUSE on=false".to_string());
        }
        Ok(digest)
    }

    /// Pause-menu SAVE: slot 1 by default, with the `GAME_MENU action=save`
    /// evidence line.
    pub fn save_slot(&mut self, slot: u8) -> Result<u64, String> {
        let path = self.save_slot_path(slot);
        let result = self.save_match(&path);
        match &result {
            Ok(digest) => {
                self.log(format!(
                    "GAME_MENU action=save slot={slot} path={} digest={digest:016x} tick={} units={} elapsed_s={:.1}",
                    path.display(),
                    self.sim.tick,
                    self.sim.units.iter().filter(|unit| unit.alive).count(),
                    self.match_elapsed_s()
                ));
                self.toast("GAME SAVED", 1.4);
            }
            Err(error) => {
                self.log(format!(
                    "GAME_MENU action=save slot={slot} rc=error err={error}"
                ));
                self.toast(format!("SAVE FAILED: {}", error.to_ascii_uppercase()), 2.0);
            }
        }
        self.sync_menu();
        result
    }

    /// Pause-menu LOAD: slot 1 by default; restores the board and resumes.
    pub fn load_slot(&mut self, slot: u8) -> Result<u64, String> {
        let path = self.save_slot_path(slot);
        let result = self.load_match(&path);
        match &result {
            Ok(digest) => {
                self.log(format!(
                    "GAME_MENU action=load slot={slot} path={} digest={digest:016x} tick={} units={}",
                    path.display(),
                    self.sim.tick,
                    self.sim.units.iter().filter(|unit| unit.alive).count()
                ));
                self.toast("GAME LOADED", 1.4);
            }
            Err(error) => {
                self.log(format!(
                    "GAME_MENU action=load slot={slot} rc=error err={error}"
                ));
                self.toast(format!("LOAD FAILED: {}", error.to_ascii_uppercase()), 2.0);
            }
        }
        result
    }

    /// Whether a pause-menu slot exists on disk (drives LOAD enabled state).
    pub fn save_slot_exists(&self, slot: u8) -> bool {
        let path = self.save_slot_path(slot);
        path.is_file() && sidecar_path(&path, ".meta").is_file()
    }

    /// WC3 single-player pause: sim ticks stop, rendering and input keep
    /// running. Returns `true` when the state actually changed.
    pub fn set_paused(&mut self, paused: bool) -> bool {
        if !matches!(self.screen, Screen::Playing | Screen::Paused) {
            return false;
        }
        if self.paused == paused {
            return false;
        }
        self.paused = paused;
        if let Some(audio) = self.assets.audio.as_mut() {
            audio.set_paused(paused);
        }
        self.log(format!("GAME_PAUSE on={paused}"));
        true
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Race of the player's main hall, for victory/defeat stingers.
    fn player_audio_race(&self) -> Option<wc3_audio::events::Race> {
        self.sim
            .alive_units()
            .find(|unit| unit.owner == self.player && unit.def().is_main_hall())
            .and_then(|unit| audio_race(unit.def().race))
    }

    fn finish(&mut self, winner: u8) {
        if self.phase != Phase::Playing {
            return;
        }
        self.phase = if winner == self.player {
            Phase::Won
        } else {
            Phase::Lost
        };
        self.paused = false;
        if self.outcome.is_none() {
            let loser = if winner == self.player {
                self.enemy
            } else {
                self.player
            };
            if !self.defeated.contains(&loser) {
                self.defeated.push(loser);
            }
            self.outcome = Some(MatchOutcome {
                winner,
                defeated: self.defeated.clone(),
                elapsed_s: self.match_elapsed_s(),
                reason: "scripted-victory",
            });
        }
        self.selection.clear();
        self.dialog = None;
        // The battle ends on a real screen with the battle stats, not a toast:
        // Restart and Main Menu are its buttons (back key also returns).
        self.screen = if winner == self.player {
            Screen::Victory
        } else {
            Screen::Defeat
        };
        self.quit_armed = false;
        self.menu_flow
            .apply(crate::hud::menu::MenuAction::Pause);
        self.sync_menu();
        self.log(format!(
            "GAME_{} tick={} kills={} trained={}",
            if winner == self.player {
                "VICTORY"
            } else {
                "DEFEAT"
            },
            self.sim.tick,
            self.total_kills,
            self.total_trained
        ));
        if let Some(outcome) = self.outcome.clone() {
            self.log(outcome.log_line());
        }
        self.log(self.score_line());
        // Retail victory/defeat stinger for the player's race.
        if let Some(race) = self.player_audio_race() {
            let state = if winner == self.player {
                wc3_audio::events::MusicState::Victory
            } else {
                wc3_audio::events::MusicState::Defeat
            };
            if let Some(audio) = self.assets.audio.as_mut() {
                let _ = audio.music_for_race(race, state);
            }
        }
    }

    /// Name of the sequence a unit's animator is currently playing. Used by the
    /// device harness and by the carry-animation test: the sack exists only in
    /// the Gold/Lumber variants, so "which clip" is the observable contract.
    /// Current animation frame time (ms) of a unit's clip.
    ///
    /// Exposed for the fight-animation contract: a swing restarts the attack
    /// clip, so this value must jump backwards at least once while the unit
    /// fights. Without the restart the clip freezes at its last frame and the
    /// unit looks static even though the sim keeps dealing damage.
    pub fn unit_clip_time_ms(&self, id: wc3_sim::UnitId) -> Option<u32> {
        self.visuals.get(&id.0).map(|visual| visual.animator.frame_time())
    }

    pub fn unit_sequence_name(&self, id: wc3_sim::UnitId) -> Option<&str> {
        let visual = self.visuals.get(&id.0)?;
        let kind = visual.sequence?;
        let Some(unit) = self.sim.unit(id) else {
            return None;
        };
        let model = self.assets.model_for_def(unit.def().id);
        let carry = if unit.carrying > 0 {
            Carry::Gold
        } else if unit.carrying_lumber > 0 {
            Carry::Lumber
        } else {
            Carry::None
        };
        let index = visual
            .played
            .or_else(|| model.sequence_with(kind, carry))?;
        model.sequences.get(index).map(|sequence| sequence.name.as_str())
    }

    fn sync_visuals(&mut self, dt: f32) {
        let mut alive = Vec::new();
        for unit in self.sim.alive_units() {
            alive.push((
                unit.id.0,
                unit.owner,
                unit.kind,
                unit.is_building(),
                unit.order,
                unit.path.is_empty(),
                unit.cooldown,
                unit.def().id,
                unit.construct.is_some(),
                unit.carrying > 0,
                unit.carrying_lumber > 0,
                unit.goal.is_some(),
            ));
        }
        for (
            id,
            _owner,
            kind,
            building,
            order,
            path_empty,
            cooldown,
            def_id,
            constructing,
            carrying_gold,
            carrying_lumber,
            has_goal,
        ) in &alive {
            if !self.visuals.contains_key(id) {
                // The animator must come from the SAME MDX the scene renders:
                // a four-kind animator over per-def geometry produced exploded
                // (pink, stretched) triangles.
                let model = self.assets.model_for_def(def_id);
                if let Some(mdx) = &model.model {
                    self.visuals.insert(
                        *id,
                        UnitVisual {
                            last_pos: self
                                .sim
                                .unit(wc3_sim::UnitId(*id))
                                .map(|unit| unit.pos)
                                .unwrap_or_default(),
                            last_tick: self.sim.tick,
                            moving: false,
                            last_cooldown: 0,
                            animator: Animator::from_model(mdx),
                            sequence: None,
                            played: None,
                        },
                    );
                }
            }
            let Some(visual) = self.visuals.get_mut(id) else {
                continue;
            };
            // Harvest orders own the whole cycle: travel, gather, return. Using
            // the generic attack/cooldown branch made a peon chop the air while
            // walking (its spell cooldown is non-zero during the trip).
            let harvest_phase = match order {
                wc3_sim::Order::Harvest { phase, .. }
                | wc3_sim::Order::HarvestLumber { phase, .. } => Some(*phase),
                _ => None,
            };
            // A worker hauls a visible sack: WC3 selects the Gold/Lumber
            // sequence set for that (measured on Peon.mdx: `Walk Gold`,
            // `Walk Lumber`, `Stand Gold`, `Stand Lumber`, `attack gold`,
            // `stand work gold`, `Attack Lumber`, `Stand Work Lumber`). The
            // sack *is* the carry geoset of those variants.
            //
            // During `Gathering` the variant comes from the resource the order
            // names, not from the load in hand: the sim sets `carrying_*` only
            // when the swing finishes (`wc3-sim` `HarvestPhase::Gathering`), so
            // keying the chop clip on the load played plain `Stand Work` and the
            // worker swung at the tree without the lumber variant.
            let gathering = matches!(harvest_phase, Some(wc3_sim::HarvestPhase::Gathering));
            let carry = if *carrying_gold
                || (gathering && matches!(order, wc3_sim::Order::Harvest { .. }))
            {
                Carry::Gold
            } else if *carrying_lumber
                || (gathering && matches!(order, wc3_sim::Order::HarvestLumber { .. }))
            {
                Carry::Lumber
            } else {
                Carry::None
            };
            // Motion is sampled once per SIM TICK (30 Hz), never per render
            // frame: the sim steps through an accumulator, so most frames see
            // no positional change at all. A peon at speed 190 covers ~6.3
            // units/tick; 0.4 units/tick is a walk, <= 0.05 is a stop, and the
            // band in between keeps the previous state (hysteresis).
            let pos = self
                .sim
                .unit(wc3_sim::UnitId(*id))
                .map(|unit| unit.pos)
                .unwrap_or(visual.last_pos);
            if self.sim.tick != visual.last_tick {
                let moved = (pos - visual.last_pos).length();
                visual.moving = if moved >= 0.4 {
                    true
                } else if moved <= 0.05 {
                    false
                } else {
                    visual.moving
                };
                visual.last_pos = pos;
                visual.last_tick = self.sim.tick;
            }
            let moving = visual.moving;
            let wanted = if *building && *constructing {
                SequenceKind::Birth
            } else if *building {
                SequenceKind::Idle
            } else if let Some(phase) = harvest_phase {
                match phase {
                    // Walking to the mine/tree: carry set only on the way back,
                    // so the outward leg is plain Walk.
                    wc3_sim::HarvestPhase::ToMine => {
                        if moving {
                            SequenceKind::Walk
                        } else {
                            SequenceKind::Idle
                        }
                    }
                    // At the resource: the chop/mining loop, with the gold or
                    // lumber work variant.
                    wc3_sim::HarvestPhase::Gathering => SequenceKind::Work,
                    // Hauling the load home: the carry variant shows the sack.
                    wc3_sim::HarvestPhase::ToHall => SequenceKind::Walk,
                }
            } else if matches!(
                order,
                wc3_sim::Order::Build(_) | wc3_sim::Order::Repair(_)
            ) {
                SequenceKind::Build
            } else if moving {
                SequenceKind::Walk
            } else if matches!(order, wc3_sim::Order::Attack(_)) || *cooldown > 0 {
                // Standing and fighting: the swing plays even when the target is
                // in range and no path exists.
                SequenceKind::Attack
            } else {
                SequenceKind::Idle
            };
            let model = self.assets.model_for_def(def_id);
            // A swing re-arms `cooldown`; that event must restart the attack
            // clip, otherwise a non-looping Attack sequence plays once and the
            // unit freezes while the sim keeps dealing damage (operator: «бьют,
            // хп уменьшается, но они не двигаются»).
            let swing = *cooldown > visual.last_cooldown;
            visual.last_cooldown = *cooldown;
            let replay = wanted == SequenceKind::Attack && swing;
            if visual.sequence != Some(wanted) || replay {
                visual.sequence = Some(wanted);
                if let Some(index) = model.sequence_with(wanted, carry)
                    && let Some(sequence) = model.sequences.get(index).cloned()
                {
                    // Device evidence: the state machine must pick the carry
                    // variants (`Walk Gold`, `stand work gold`) for hauling
                    // workers, which is what draws the sack.
                    crate::dev_log!(
                        "GAME_SEQ unit={} def={} seq={} carry={:?} order={:?}",
                        id,
                        def_id,
                        sequence.name,
                        carry,
                        order
                    );
                    visual.animator.play(&sequence);
                    visual.played = Some(index);
                }
            }
            if visual.played.is_some() {
                visual.animator.update(dt);
            }
        }
        for death in &mut self.death_visuals {
            death.animator.update(dt);
            death.elapsed += dt;
        }
        self.death_visuals.retain(|death| death.elapsed < 1.4);
        let live: Vec<u32> = self.sim.alive_units().map(|unit| unit.id.0).collect();
        self.visuals.retain(|id, _| live.contains(id));
    }

    fn prune_selection(&mut self) {
        self.selection
            .retain(|id| self.sim.unit(*id).map(|unit| unit.alive).unwrap_or(false));
        self.selection.dedup();
        keep_selection_cells(&mut self.selected_cells, &self.selection);
    }

    pub fn restart(&mut self) {
        // WC3 restart: a fresh run of the same map. Advance the seed so the
        // new match is a new roll (creep drops, AI waves) while remaining
        // deterministic for a given run.
        self.seed = self
            .seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(0x9E37_79B9_7F4A_7C15);
        self.reset_scenario();
        self.screen = Screen::Playing;
        self.boot_menu = false;
        self.loading_time = 0.0;
        self.toast_queue.clear();
        self.log(format!("GAME_RESTART seed={}", self.seed));
    }

    // ---------------------------------------------------------------- input

    /// One-finger press: route through the HUD zones and let the game apply the
    /// effect. Gesture classification and camera maths live in `wc3-input`.
    // ---------------------------------------------------------- parity screens

    /// Show the main menu (retail glue screen). Hosts may call this at any
    /// time; the back key navigates it page by page.
    /// Enter the already-spawned battle, skipping the retail glue screens.
    ///
    /// The interactive app boots on the main menu (`boot_menu`): its first
    /// frame opens the glue flow unless the host has injected input. Headless
    /// hosts — probes, the sim harness, integration tests — drive the battle
    /// itself and need the live tick loop from the first frame; this is their
    /// explicit entry point. The scenario itself is untouched: `Game::new`
    /// already spawned it.
    pub fn enter_battle(&mut self) {
        self.boot_menu = false;
        self.input_seen = true;
        self.paused = false;
        if self.screen != Screen::Playing {
            self.screen = Screen::Playing;
            self.loading_time = 0.0;
        }
    }

    pub fn open_main_menu(&mut self) {
        self.screen = Screen::MainMenu;
        self.paused = false;
        self.quit_armed = false;
        self.dialog = None;
        self.placing = None;
        self.placing_at = None;
        self.pointer_cancel();
        self.menu_flow
            .apply(crate::hud::menu::MenuAction::QuitToMenu);
        self.sync_menu();
        self.enter_menu_audio();
    }

    /// Intern a menu label once (bounded by the distinct strings a session
    /// shows: map names, stats lines, picker values).
    fn intern(&mut self, text: &str) -> &'static str {
        if let Some(existing) = self.menu_labels.iter().find(|line| **line == text) {
            return existing;
        }
        let leaked: &'static str = Box::leak(text.to_string().into_boxed_str());
        self.menu_labels.push(leaked);
        leaked
    }

    fn menu_button(&mut self, label: &str, sub: &str, action: u8, enabled: bool) -> MenuButton<'static> {
        let label = self.intern(label);
        let sub = self.intern(sub);
        MenuButton {
            label,
            sub,
            action,
            enabled,
        }
    }

    /// Glue page → rendered menu screen.
    fn menu_screen(&self) -> MenuScreen {
        use crate::hud::menu::MenuPage;
        match self.screen {
            Screen::Paused => MenuScreen::PauseOptions,
            Screen::Victory => MenuScreen::Victory,
            Screen::Defeat => MenuScreen::Defeat,
            Screen::Loading | Screen::Playing => MenuScreen::Loading,
            Screen::MapSelect => MenuScreen::MapSelect,
            Screen::MainMenu => match self.menu_flow.page {
                MenuPage::Main => MenuScreen::Main,
                MenuPage::Campaign => MenuScreen::Campaign,
                MenuPage::CustomGame => MenuScreen::CustomGame,
                MenuPage::SinglePlayer => MenuScreen::SinglePlayer,
                MenuPage::RaceSelect | MenuPage::OpponentSelect | MenuPage::Difficulty => {
                    MenuScreen::Skirmish
                }
                MenuPage::MapSelect => MenuScreen::MapSelect,
                MenuPage::Loading => MenuScreen::Loading,
                MenuPage::Paused => MenuScreen::PauseOptions,
                MenuPage::Options | MenuPage::SaveLoad => MenuScreen::PauseOptions,
                MenuPage::InGame => MenuScreen::Main,
            },
        }
    }

    /// Stats block of the victory/defeat screen: the real battle numbers.
    fn end_stats(&self) -> String {
        format!(
            "KILLS {}   TRAINED {}   ARMY {}   TIME {}:{:02}   GOLD {}   LUMBER {}",
            self.total_kills,
            self.total_trained,
            self.army_count(),
            (self.elapsed as u32) / 60,
            (self.elapsed as u32) % 60,
            self.sim.resources[self.player as usize].max(0),
            self.sim.lumber[self.player as usize].max(0)
        )
    }

    /// Rebuild the current glue screen's title, subtitle and buttons. Called
    /// whenever the page, the battle state or the quit arm changes.
    fn sync_menu(&mut self) {
        use crate::hud::menu::{DIFFICULTIES, MenuPage, RACES};
        let page = self.menu_flow.page;
        let race = self.menu_flow.race.label();
        let opponent = self.menu_flow.opponent.label();
        let difficulty = self.menu_flow.difficulty.label();
        let map_name = self
            .menu_flow
            .map()
            .map(|map| map.name.clone())
            .unwrap_or_else(|| self.assets.map_name.clone());
        let stats = self.end_stats();
        let quit_armed = self.quit_armed;
        let mut buttons: Vec<MenuButton<'static>> = Vec::new();
        let mut tabs: Vec<MenuButton<'static>> = Vec::new();
        let mut rows: Vec<MenuButton<'static>> = Vec::new();
        let mut nav: Vec<MenuButton<'static>> = Vec::new();
        let title: &'static str;
        let subtitle: String = match (self.screen, page) {
            (Screen::Paused, _) => {
                title = "GAME PAUSED";
                format!("{map_name}  ·  {race}  ·  {difficulty}")
            }
            (Screen::Victory, _) => {
                title = "VICTORY";
                stats.clone()
            }
            (Screen::Defeat, _) => {
                title = "DEFEAT";
                stats.clone()
            }
            (Screen::Loading, _) | (_, MenuPage::Loading) => {
                title = "LOADING";
                format!("{map_name}  ·  {race} vs {opponent}  ·  {difficulty}")
            }
            (_, MenuPage::Main) => {
                title = "WARCRAFT III";
                "REIGN OF CHAOS · MOBILE".to_string()
            }
            (_, MenuPage::SinglePlayer) => {
                title = "SKIRMISH";
                format!("{race} vs {opponent}  ·  {map_name}  ·  {difficulty}")
            }
            (_, MenuPage::Campaign) => {
                title = "CAMPAIGN";
                format!("CHAPTERS  ·  {difficulty}")
            }
            (_, MenuPage::CustomGame) => {
                title = "CUSTOM GAME";
                format!("{map_name}  ·  {race} vs {opponent}")
            }
            (_, MenuPage::RaceSelect) => {
                title = "SELECT RACE";
                format!("CURRENT {race}")
            }
            (_, MenuPage::OpponentSelect) => {
                title = "SELECT OPPONENT";
                format!("CURRENT {opponent}")
            }
            (_, MenuPage::MapSelect) => {
                title = "SELECT MAP";
                map_name.clone()
            }
            (_, MenuPage::Difficulty) => {
                title = "DIFFICULTY";
                format!("CURRENT {difficulty}")
            }
            (_, MenuPage::Options) => {
                title = "OPTIONS";
                "AUDIO · VIDEO · GAMEPLAY".to_string()
            }
            (_, MenuPage::SaveLoad) => {
                title = "SAVE / LOAD";
                self.menu_flow
                    .save
                    .reason
                    .clone()
                    .unwrap_or_else(|| "SAVE API UNAVAILABLE".to_string())
            }
            (_, MenuPage::Paused) | (_, MenuPage::InGame) => {
                title = "GAME PAUSED";
                format!("{map_name}  ·  {race}  ·  {difficulty}")
            }
        };
        match (self.screen, page) {
            (Screen::Victory, _) | (Screen::Defeat, _) => {
                buttons.push(self.menu_button("RESTART", "ENTER", 9, true));
                buttons.push(self.menu_button("MAIN MENU", "ESC", 8, true));
            }
            (Screen::Paused, _) => {
                buttons.push(self.menu_button("RESUME", "ESC", 7, true));
                let save_ok = self.engine.is_none() && self.phase == Phase::Playing;
                buttons.push(self.menu_button("SAVE GAME", "SLOT 1", 14, save_ok));
                buttons.push(self.menu_button(
                    "LOAD GAME",
                    "SLOT 1",
                    15,
                    self.engine.is_none() && self.save_slot_exists(MENU_SAVE_SLOT),
                ));
                buttons.push(self.menu_button("RESTART", "ENTER", 9, true));
                buttons.push(self.menu_button(
                    if quit_armed { "CONFIRM QUIT" } else { "QUIT TO MENU" },
                    "2x",
                    2,
                    true,
                ));
            }
            (Screen::Loading, _) | (_, MenuPage::Loading) => {}
            (_, MenuPage::Main) => {
                // WC3 main menu: CONTINUE resumes the checkpoint, the three
                // campaign/custom/load entries open real screens, OPTIONS and
                // QUIT close the list. All six are 90 px+ rows.
                if self.menu_flow.has_save {
                    buttons.push(self.menu_button("CONTINUE", "SAVE", MENU_ACT_CONTINUE, true));
                }
                let chapters = self.campaign_chapters().len();
                buttons.push(self.menu_button(
                    "CAMPAIGN",
                    &format!("{chapters} CHAPTERS"),
                    MENU_ACT_CAMPAIGN,
                    chapters > 0,
                ));
                let maps = self.menu_flow.maps.len();
                buttons.push(self.menu_button(
                    "CUSTOM GAME",
                    &format!("{maps} MAPS"),
                    MENU_ACT_CUSTOM,
                    maps > 0,
                ));
                buttons.push(self.menu_button(
                    "LOAD GAME",
                    "SLOT 1",
                    MENU_ACT_LOAD,
                    self.save_slot_exists(MENU_SAVE_SLOT),
                ));
                buttons.push(self.menu_button("OPTIONS", "SETUP", MENU_ACT_OPTIONS, true));
                buttons.push(self.menu_button(
                    if quit_armed { "CONFIRM QUIT" } else { "QUIT" },
                    "2x",
                    MENU_ACT_QUIT,
                    true,
                ));
            }
            (_, MenuPage::Campaign) => {
                let chapters = self.campaign_chapters();
                // Group tabs: the retail campaigns plus EXTRA (credits reels and
                // any chapter without a race prefix) — never hidden.
                for (index, name) in crate::hud::menu::CAMPAIGN_GROUPS.iter().enumerate() {
                    let count = chapters.iter().filter(|row| row.group == index).count();
                    tabs.push(self.menu_button(
                        name,
                        &count.to_string(),
                        MENU_ACT_TAB + index as u8,
                        count > 0,
                    ));
                }
                let tab = self.menu_flow.campaign_tab.min(crate::hud::menu::CAMPAIGN_GROUPS.len() - 1);
                let group: Vec<crate::campaign::ChapterRow> = chapters
                    .iter()
                    .filter(|row| row.group == tab)
                    .cloned()
                    .collect();
                let pages = self.menu_flow.campaign_pages();
                let page = self.menu_flow.campaign_page.min(pages.saturating_sub(1));
                let per_page = crate::hud::menu::CAMPAIGN_ROWS_PER_PAGE;
                for (slot, row) in group
                    .iter()
                    .skip(page * per_page)
                    .take(per_page)
                    .enumerate()
                {
                    let label = if row.interlude {
                        format!("{}  {}", "—", row.title)
                    } else {
                        format!("{:02}  {}", row.number, row.title)
                    };
                    rows.push(self.menu_button(
                        &label,
                        row.state.as_str(),
                        MENU_ACT_CHAPTER + slot as u8,
                        row.state != crate::campaign::ChapterState::Locked,
                    ));
                }
                nav.push(self.menu_button("PREV", "<", MENU_ACT_PAGE_PREV, page > 0));
                nav.push(self.menu_button(
                    "NEXT",
                    ">",
                    MENU_ACT_PAGE_NEXT,
                    page + 1 < pages,
                ));
                nav.push(self.menu_button("BACK", "ESC", MENU_ACT_BACK, true));
                let label = format!(
                    "{} {}/{}",
                    crate::hud::menu::CAMPAIGN_GROUPS[tab],
                    page + 1,
                    pages.max(1)
                );
                self.menu_page_label = self.intern(&label);
            }
            (_, MenuPage::CustomGame) => {
                // Race and difficulty are cycle fields on this screen; the map
                // list is paginated rows with the player count from the census.
                tabs.push(self.menu_button("RACE", race, MENU_ACT_RACE, true));
                tabs.push(self.menu_button("DIFFICULTY", difficulty, MENU_ACT_DIFFICULTY, true));
                let pages = self.menu_flow.map_pages();
                let page = self.menu_flow.map_page.min(pages.saturating_sub(1));
                let per_page = crate::hud::menu::MAP_ROWS_PER_PAGE;
                let map_rows: Vec<(String, usize)> = self
                    .menu_flow
                    .maps
                    .iter()
                    .skip(page * per_page)
                    .take(per_page)
                    .map(|map| (map.name.clone(), map.players.max(1) as usize))
                    .collect();
                for (slot, (name, players)) in map_rows.into_iter().enumerate() {
                    let index = page * per_page + slot;
                    let selected = index == self.menu_flow.map_index;
                    let sub = format!(
                        "{}P · {} PLAYERS",
                        if selected { ">" } else { "" },
                        players
                    );
                    rows.push(self.menu_button(
                        &name,
                        &sub,
                        MENU_ACT_MAP_ROW + slot as u8,
                        true,
                    ));
                }
                nav.push(self.menu_button("PREV", "<", MENU_ACT_MAP_PREV, page > 0));
                nav.push(self.menu_button(
                    "NEXT",
                    ">",
                    MENU_ACT_MAP_NEXT,
                    page + 1 < pages,
                ));
                nav.push(self.menu_button(
                    "START",
                    "PLAY",
                    MENU_ACT_START,
                    self.menu_flow.can_start(),
                ));
                nav.push(self.menu_button("BACK", "ESC", MENU_ACT_BACK, true));
                let label = format!("MAPS {}/{}", page + 1, pages.max(1));
                self.menu_page_label = self.intern(&label);
            }
            (_, MenuPage::SinglePlayer) => {
                buttons.push(self.menu_button("RACE", race, 10, true));
                buttons.push(self.menu_button("OPPONENT", opponent, 11, true));
                buttons.push(self.menu_button("MAP", "SELECT", 12, true));
                buttons.push(self.menu_button("DIFFICULTY", difficulty, 13, true));
                buttons.push(self.menu_button("START GAME", "ENTER", 0, self.menu_flow.can_start()));
                buttons.push(self.menu_button("BACK", "ESC", 3, true));
            }
            (_, MenuPage::RaceSelect) => {
                for (index, choice) in RACES.iter().enumerate() {
                    buttons.push(self.menu_button(choice.label(), if *choice == self.menu_flow.race { "CURRENT" } else { "PICK" }, 20 + index as u8, true));
                }
                buttons.push(self.menu_button("BACK", "ESC", 3, true));
            }
            (_, MenuPage::OpponentSelect) => {
                for (index, choice) in RACES.iter().enumerate() {
                    buttons.push(self.menu_button(choice.label(), if *choice == self.menu_flow.opponent { "CURRENT" } else { "PICK" }, 30 + index as u8, true));
                }
                buttons.push(self.menu_button("BACK", "ESC", 3, true));
            }
            (_, MenuPage::MapSelect) => {
                let maps = self.menu_flow.maps.clone();
                for (index, map) in maps.iter().enumerate() {
                    buttons.push(self.menu_button(
                        &map.name,
                        if index == self.menu_flow.map_index { "CURRENT" } else { "PICK" },
                        40 + index as u8,
                        true,
                    ));
                }
                buttons.push(self.menu_button("BACK", "ESC", 3, true));
            }
            (_, MenuPage::Difficulty) => {
                for (index, level) in DIFFICULTIES.iter().enumerate() {
                    buttons.push(self.menu_button(
                        level.label(),
                        if *level == self.menu_flow.difficulty { "CURRENT" } else { "PICK" },
                        50 + index as u8,
                        true,
                    ));
                }
                buttons.push(self.menu_button("BACK", "ESC", 3, true));
            }
            (_, MenuPage::Options) | (_, MenuPage::SaveLoad) => {
                buttons.push(self.menu_button("BACK", "ESC", 3, true));
            }
            (_, MenuPage::Paused) | (_, MenuPage::InGame) => {}
        }
        self.menu_buttons = buttons;
        self.menu_title = self.intern(title);
        self.menu_subtitle = self.intern(&subtitle);
    }

    /// Labels of the current glue/pause screen buttons, in draw order (HUD
    /// host and headless QA read the same list the renderer draws).
    pub fn menu_button_labels(&self) -> Vec<&'static str> {
        self.menu_buttons.iter().map(|button| button.label).collect()
    }

    fn menu_view(&self) -> MenuView<'_> {
        let screen = self.menu_screen();
        let mut view = MenuView::for_screen(screen, self.viewport, self.menu_map_name);
        view.title = self.menu_title;
        view.subtitle = self.menu_subtitle;
        view.buttons = &self.menu_buttons;
        view.map_name = self.menu_map_name;
        view.status = self.menu_flow.message.as_deref().unwrap_or("");
        view.progress = self.loading_time;
        view.redraw = self.menu_skin();
        view.tabs = &self.menu_tabs;
        view.rows = &self.menu_rows;
        view.nav = &self.menu_nav;
        view.active_tab = self.menu_flow.campaign_tab;
        view.page_label = self.menu_page_label;
        view.veil = self.menu_veil;
        view
    }

    /// Pack menu art for the glue screens: `menu_panel_normal_512x384` as the
    /// frame, `menu_button_*_256x64` for the four button states, the loading
    /// frame and the pack serif face. Every part carries its authored size and
    /// insets, so a 512x384 panel is never sliced with 256x64 maths.
    fn menu_skin(&self) -> Option<crate::hud::mobile_hud::MenuSkin> {
        use crate::hud::mobile_hud::{MenuPart, MenuSkin};
        let pack = self
            .assets
            .hud_skin
            .as_ref()
            .and_then(|skin| skin.pack.as_ref());
        let part = |name: &str| {
            pack.and_then(|pack| pack.asset(name)).map(|asset| {
                MenuPart::new(asset.texture, asset.w as f32, asset.h as f32, asset.insets)
            })
        };
        // Retail `EscMenu*` textures are absent from the data archives; the
        // pack ships them, so the pack wins and the FDF art is the fallback.
        let fdf = |name: &str, size: [f32; 2]| {
            self.assets.ui.texture(name).map(|texture| {
                MenuPart::new(texture, size[0], size[1], [24.0, 24.0, 24.0, 24.0])
            })
        };
        let fonts = pack.and_then(|pack| crate::hud::pack_fonts(&pack.root));
        let skin = MenuSkin {
            panel: part("menu_panel_normal_512x384")
                .or_else(|| fdf("EscMenuBackground", [512.0, 384.0])),
            panel_border: part("score_panel_normal_1024x512")
                .or_else(|| fdf("EscMenuBorder", [1024.0, 512.0])),
            title_band: part("topbar_plate_normal_720x32")
                .or_else(|| fdf("EscMenuTitleBand", [720.0, 32.0])),
            button_normal: part("menu_button_normal_256x64")
                .or_else(|| fdf("EscMenuButton", [256.0, 64.0])),
            button_hover: part("menu_button_hover_256x64")
                .or_else(|| fdf("EscMenuButtonMouseOverHighlight", [256.0, 64.0])),
            button_pressed: part("menu_button_pressed_256x64")
                .or_else(|| fdf("EscMenuButtonPressed", [256.0, 64.0])),
            button_disabled: part("menu_button_disabled_256x64")
                .or_else(|| fdf("EscMenuButtonDisabled", [256.0, 64.0])),
            loading_frame: part("loading_frame_normal_1024x512")
                .or_else(|| fdf("EscMenuLoading", [1024.0, 512.0])),
            banner: part("topbar_plate_normal_720x32"),
            fonts,
        };
        (skin.panel.is_some() || skin.button_normal.is_some()).then_some(skin)
    }

    fn tick_screen(&mut self, dt: f32) {
        if self.screen == Screen::Loading {
            if self.loading_time <= 0.01 {
                // The loading screen owns the map start: the battle is already
                // reset when it begins (see `begin_battle`).
            }
            self.loading_time = (self.loading_time + dt * 0.9).min(1.0);
            if self.loading_time >= 1.0 {
                self.screen = Screen::Playing;
                self.loading_time = 0.0;
                self.menu_flow
                    .apply(crate::hud::menu::MenuAction::AdvanceLoading);
                self.sync_menu();
                self.toast("BATTLE START", 1.2);
                self.leave_menu_audio();
                self.start_audio_for_scenario();
            }
        }
    }

    /// Leave the battle for the pause menu (F10 / BACK).
    pub fn open_game_menu(&mut self) {
        if self.screen != Screen::Playing {
            return;
        }
        self.menu_flow.apply(crate::hud::menu::MenuAction::Pause);
        self.screen = Screen::Paused;
        self.set_paused(true);
        self.quit_armed = false;
        self.placing = None;
        self.placing_at = None;
        self.targeting = None;
        self.pointer_cancel();
        self.sync_menu();
        self.log("GAME_MENU open".to_string());
    }

    fn resume_battle(&mut self) {
        self.menu_flow
            .apply(crate::hud::menu::MenuAction::Resume);
        self.screen = Screen::Playing;
        self.set_paused(false);
        self.quit_armed = false;
        self.sync_menu();
        self.log("GAME_MENU resume".to_string());
    }

    /// Start the selected map: reload the assets when the menu picked a
    /// different map file, otherwise restart the same battle.
    fn begin_battle(&mut self) {
        self.menu_flow
            .apply(crate::hud::menu::MenuAction::StartGame);
        if self.menu_flow.page != crate::hud::menu::MenuPage::Loading {
            // No map selected (empty install): stay on the menu with its message.
            self.sync_menu();
            return;
        }
        let target = self
            .menu_flow
            .map()
            .map(|map| map.path.clone())
            .unwrap_or_default();
        let current = self
            .scenario_map_path()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !target.is_empty() && target != current {
            self.load_map(std::path::Path::new(&target));
            return;
        }
        self.restart();
        self.screen = Screen::Loading;
        self.loading_time = 0.0;
        self.sync_menu();
        self.log(format!(
            "GAME_LOAD map={} race={} opponent={} difficulty={}",
            self.assets.map_name,
            self.menu_flow.race.label(),
            self.menu_flow.opponent.label(),
            self.menu_flow.difficulty.label()
        ));
    }

    /// Load another shipped map from the menu: rebuild the whole game from that
    /// file (assets, terrain, units, engine) under the same seed and races.
    fn load_map(&mut self, path: &Path) {
        let Some(mpq) = self.mpq_source_path() else {
            self.log("GAME_LOAD no-install".to_string());
            self.toast("NO GAME INSTALL", 1.6);
            return;
        };
        let seed = self.seed;
        let race = self.menu_flow.race;
        let opponent = self.menu_flow.opponent;
        let difficulty = self.menu_flow.difficulty;
        let map_index = self.menu_flow.map_index;
        let mut next = Game::new(Assets::load(Some(&mpq), Some(path)), seed);
        next.menu_flow.race = race;
        next.menu_flow.opponent = opponent;
        next.menu_flow.difficulty = difficulty;
        next.menu_flow.map_index = map_index;
        next.boot_menu = false;
        next.screen = Screen::Loading;
        next.loading_time = 0.0;
        next.log(format!(
            "GAME_LOAD map={} path={}",
            next.assets.map_name,
            path.display()
        ));
        *self = next;
        self.sync_menu();
    }

    fn mpq_source_path(&self) -> Option<PathBuf> {
        self.assets
            .debug
            .iter()
            .find_map(|line| line.strip_prefix("mpq="))
            .map(PathBuf::from)
            .filter(|path| path.is_file())
    }

    /// Stable name for a menu action code (`GAME_MENU action=…`).
    fn menu_action_name(action: u8) -> &'static str {
        match action {
            0 => "play",
            1 => "map-select",
            2 => "quit",
            3 => "back",
            7 => "resume",
            8 => "main-menu",
            9 => "restart",
            10 => "race",
            11 => "opponent",
            12 => "map",
            13 => "difficulty",
            14 => "save",
            15 => "load",
            20..=24 => "set-race",
            30..=34 => "set-opponent",
            // Difficulty codes sit inside the map range; they must be matched
            // first or map index 10..12 shadows them.
            50..=52 => "set-difficulty",
            40..=63 => "set-map",
            _ => "unknown",
        }
    }

    /// One glue-screen button. Actions are page-relative indices into the
    /// buttons `sync_menu` produced (`activate_menu` maps them back).
    pub fn activate_menu(&mut self, action: u8) {
        self.log(format!(
            "GAME_MENU action={} id={action}",
            Self::menu_action_name(action)
        ));
        // The pause menu owns its own QUIT: it leaves the battle for the main
        // menu and keeps the process alive. The main menu's QUIT is the exit.
        match self.screen {
            Screen::Victory | Screen::Defeat => {
                match action {
                    MENU_ACT_RESTART => self.begin_battle(),
                    MENU_ACT_MENU => self.open_main_menu(),
                    _ => self.sync_menu(),
                }
                return;
            }
            Screen::Paused => {
                match action {
                    MENU_ACT_RESUME => self.resume_battle(),
                    MENU_ACT_SAVE => {
                        let _ = self.save_slot(MENU_SAVE_SLOT);
                    }
                    MENU_ACT_LOAD => {
                        let _ = self.load_slot(MENU_SAVE_SLOT);
                    }
                    MENU_ACT_RESTART => self.begin_battle(),
                    MENU_ACT_QUIT => {
                        // WC3 pause menu: "Quit Mission" asks once, then leaves
                        // the battle for the main menu; the process stays alive
                        // (the main menu's own QUIT is the two-tap exit).
                        if !self.quit_armed {
                            self.quit_armed = true;
                            self.sync_menu();
                            self.log("GAME_QUIT armed".to_string());
                            self.toast("TAP QUIT AGAIN TO MENU", 2.0);
                            return;
                        }
                        self.quit_armed = false;
                        self.open_main_menu();
                    }
                    _ => self.sync_menu(),
                }
                return;
            }
            _ => {}
        }
        if action == MENU_ACT_QUIT {
            if !self.quit_armed {
                self.quit_armed = true;
                self.sync_menu();
                self.log("GAME_QUIT armed".to_string());
                self.toast("TAP QUIT AGAIN TO EXIT", 2.0);
                return;
            }
            self.log("GAME_QUIT confirmed".to_string());
            // Real quit: the host process ends (WC3 returns to the desktop).
            std::process::exit(0);
        }
        self.quit_armed = false;
        use crate::hud::menu::{DIFFICULTIES, MenuAction, MenuPage, RACES};
        let page = self.menu_flow.page;
        let maps = self.menu_flow.maps.len();
        let applied = if (20..25).contains(&action) {
            RACES
                .get((action - 20) as usize)
                .copied()
                .map(MenuAction::SetRace)
        } else if (30..35).contains(&action) {
            RACES
                .get((action - 30) as usize)
                .copied()
                .map(MenuAction::SetOpponent)
        } else if (50..53).contains(&action) {
            // Difficulty before maps: 50..52 would otherwise read as map
            // indices once the install ships 11+ maps.
            DIFFICULTIES
                .get((action - 50) as usize)
                .copied()
                .map(MenuAction::SetDifficulty)
        } else if (40..64).contains(&action) && ((action - 40) as usize) < maps {
            Some(MenuAction::SetMap((action - 40) as usize))
        } else {
            match action {
                // PLAY opens the skirmish screen; START GAME (same code) starts
                // the battle from it.
                MENU_ACT_PLAY if page == MenuPage::Main => {
                    Some(MenuAction::Goto(MenuPage::SinglePlayer))
                }
                MENU_ACT_PLAY => Some(MenuAction::StartGame),
                MENU_ACT_MAPS => Some(MenuAction::Goto(MenuPage::MapSelect)),
                MENU_ACT_BACK => Some(match page {
                    // Pickers return to the skirmish screen they came from.
                    MenuPage::RaceSelect
                    | MenuPage::OpponentSelect
                    | MenuPage::MapSelect
                    | MenuPage::Difficulty => MenuAction::Goto(MenuPage::SinglePlayer),
                    MenuPage::Main => MenuAction::Goto(MenuPage::Main),
                    _ => MenuAction::Back,
                }),
                MENU_ACT_RACE => Some(MenuAction::Goto(MenuPage::RaceSelect)),
                MENU_ACT_OPPONENT => Some(MenuAction::Goto(MenuPage::OpponentSelect)),
                MENU_ACT_MAP => Some(MenuAction::Goto(MenuPage::MapSelect)),
                MENU_ACT_DIFFICULTY => Some(MenuAction::Goto(MenuPage::Difficulty)),
                _ => None,
            }
        };
        if let Some(applied) = applied {
            self.menu_flow.apply(applied);
        }
        if self.menu_flow.page == MenuPage::Loading && self.screen != Screen::Loading {
            self.begin_battle();
            return;
        }
        self.sync_menu();
    }

    /// Back key / Escape on a glue screen: walk one page up, or arm QUIT on the
    /// main menu (Android back exits the app).
    fn menu_back(&mut self) {
        match self.screen {
            Screen::Paused => self.resume_battle(),
            Screen::Victory | Screen::Defeat => self.open_main_menu(),
            Screen::Loading => {}
            _ => {
                if self.menu_flow.page == crate::hud::menu::MenuPage::Main {
                    // Same two-tap confirm as the QUIT button.
                    self.activate_menu(MENU_ACT_QUIT);
                } else {
                    self.menu_flow.apply(crate::hud::menu::MenuAction::Back);
                    self.sync_menu();
                }
            }
        }
    }

    /// F-keys from the host: F1/F9 open the quest log, F10 opens the game menu.
    /// The Android BACK key arrives as [`Hotkey::Escape`].
    pub fn function_key(&mut self, key: u8) {
        self.input_seen = true;
        match key {
            1 | 9 => {
                if self.screen == Screen::Playing {
                    self.activate(UiAction::QuestLog);
                }
            }
            10 => {
                if self.screen == Screen::Paused {
                    self.resume_battle();
                } else {
                    self.open_game_menu();
                }
            }
            _ => {}
        }
    }

    pub fn pointer_down(&mut self, pos: Vec2) {
        self.input_seen = true;
        if self.screen != Screen::Playing {
            let view = self.menu_view();
            let layout = self.mobile_hud.layout_menu(&view);
            for (index, rect) in layout.buttons.iter().enumerate() {
                if rect.contains(pos)
                    && let Some(button) = view.buttons.get(index)
                {
                    self.activate_menu(button.action);
                    return;
                }
            }
            return;
        }
        if self.phase != Phase::Playing {
            self.restart();
            return;
        }
        // An open dialog (quest log) owns the next tap: it closes, nothing else.
        if self.dialog.is_some() {
            self.dialog = None;
            self.push_haptic(HapticCue::Press);
            return;
        }
        // A drawer owns the gesture it was opened for: a touch outside it
        // closes the drawer instead of reaching the world behind it.
        if (self.groups_open || self.commands_open) && !self.drawer_contains(pos) {
            self.close_hud_drawers();
            self.push_haptic(HapticCue::Press);
            return;
        }
        let blocked = self.blocked_rects();
        let layout = TouchLayout {
            minimap: Some(touch_rect(self.layout.minimap)),
            blocked: blocked.as_slice(),
            buttons: self.touch_buttons.as_slice(),
            placing: self.placing.is_some(),
            // Finger tolerances are dp: multiply by the one density factor.
            scale: self.density_scale,
        };
        match self.touch.pointer_down(pos, self.elapsed, &layout) {
            TouchStart::Minimap => {
                self.camera.centre_on(wc3_input::minimap_to_world(
                    pos,
                    touch_rect(self.layout.minimap_image),
                    self.assets.world_size,
                ));
            }
            TouchStart::Button(_) => self.push_haptic(HapticCue::Press),
            TouchStart::Blocked | TouchStart::Place | TouchStart::World => {}
        }
    }

    pub fn pointer_move(&mut self, pos: Vec2) {
        self.input_seen = true;
        if self.placing.is_some() {
            self.placing_at = self
                .camera
                .screen_to_world(pos, |x, z| self.assets.height_at(x, z));
            self.log_ghost();
        } else {
            self.placing_at = None;
            self.ghost_cell = None;
        }
        let blocked = self.blocked_rects();
        let layout = TouchLayout {
            minimap: Some(touch_rect(self.layout.minimap)),
            blocked: blocked.as_slice(),
            buttons: self.touch_buttons.as_slice(),
            placing: self.placing.is_some(),
            scale: self.layout.scale,
        };
        if let Some(movement) = self.touch.pointer_move(pos, self.elapsed, &layout) {
            match movement {
                TouchMove::Pan(delta) => self.camera.pan_pixels(delta),
                TouchMove::Minimap(point) => {
                    self.camera.centre_on(wc3_input::minimap_to_world(
                        point,
                        touch_rect(self.layout.minimap_image),
                        self.assets.world_size,
                    ));
                }
            }
        }
    }

    pub fn pointer_up(&mut self, pos: Vec2) {
        self.input_seen = true;
        let blocked = self.blocked_rects();
        let layout = TouchLayout {
            minimap: Some(touch_rect(self.layout.minimap)),
            blocked: blocked.as_slice(),
            buttons: self.touch_buttons.as_slice(),
            placing: self.placing.is_some(),
            scale: self.density_scale,
        };
        let Some(end) = self.touch.pointer_up(pos, self.elapsed, &layout) else {
            return;
        };
        match end {
            TouchEnd::Button {
                index,
                double,
                hold,
            } => {
                if index == GROUPS_BUTTON_TARGET {
                    self.toggle_groups_drawer();
                    self.push_haptic(HapticCue::Press);
                    return;
                }
                if index == COMMANDS_BUTTON_TARGET {
                    self.toggle_commands_drawer();
                    self.push_haptic(HapticCue::Press);
                    return;
                }
                if index >= ARMY_TARGET_BASE {
                    let slot = index - ARMY_TARGET_BASE;
                    if let Some(id) = self.selection.get(slot).copied() {
                        self.selection = vec![id];
                        if let Some(unit) = self.sim.unit(id) {
                            if double {
                                self.camera.centre_on(unit.pos);
                            }
                            self.log(format!(
                                "ARMY_PICK slot={slot} unit={} def={}",
                                id.0,
                                unit.def().id
                            ));
                        }
                        self.push_haptic(HapticCue::Select);
                        self.refresh_buttons();
                    }
                    return;
                }
                if index >= GROUP_TARGET_BASE {
                    self.hit_group_slot(index - GROUP_TARGET_BASE, hold, double);
                    return;
                }
                if let Some(button) = self.buttons.get(index).cloned()
                    && button.enabled
                {
                    self.push_haptic(HapticCue::Press);
                    self.log(format!("BUTTON {}", button.label));
                    if button.action == UiAction::SelectArmy {
                        if hold {
                            self.save_group_from_touch();
                        } else if double {
                            self.cycle_group_from_touch();
                        } else {
                            self.activate(button.action);
                        }
                    } else {
                        self.activate(button.action);
                    }
                }
            }
            TouchEnd::Minimap { at, hold } => {
                let world = wc3_input::minimap_to_world(
                    at,
                    touch_rect(self.layout.minimap_image),
                    self.assets.world_size,
                );
                self.camera.centre_on(world);
                if hold && let Some(units) = self.selected_army() {
                    // ui.minimap.orders: long-press on the minimap is an
                    // attack-move for the selection (short tap is the camera).
                    self.pending.push(Command::MoveAttack { units, to: world });
                    self.move_marker = Some((world, 1.2));
                    self.push_haptic(HapticCue::Order);
                }
            }
            TouchEnd::Place { at } => {
                self.log(format!("PLACE_TAP {:.0},{:.0}", at.x, at.y));
                match self
                    .camera
                    .screen_to_world(at, |x, z| self.assets.height_at(x, z))
                {
                    Some(world) => {
                        self.log(format!("PLACE_WORLD {:.0},{:.0}", world.x, world.y));
                        self.place_building(world);
                    }
                    None => self.log("PLACE_RAY_MISS".to_string()),
                }
            }
            TouchEnd::Box { start, end } => self.box_select(start, end),
            TouchEnd::Tap { at, double } => self.tap_action(at, double),
        }
    }

    pub fn pointer_cancel(&mut self) {
        // `cancel` drops the live pointer/pinch but keeps the double-tap and
        // last-button history, so a tap after focus loss/rotation could pair
        // with one from before the transition. `reset` clears both.
        self.touch.reset();
    }

    /// A second finger was lifted: keep panning with the finger that remains.
    pub fn pointer_resume_pan(&mut self, pos: Vec2) {
        if self.phase != Phase::Playing {
            return;
        }
        self.touch.resume_pan(pos, self.elapsed);
    }

    pub fn pinch_begin(&mut self, distance: f32, centre: Vec2) {
        self.input_seen = true;
        self.touch
            .pinch_begin(distance, centre, self.camera.distance);
    }

    /// Pinch zoom and two-finger pan in one gesture.
    pub fn pinch_update(&mut self, distance: f32, centre: Vec2) {
        if let Some(update) = self.touch.pinch_update(distance, centre) {
            self.camera.distance = update.distance;
            if update.pan != Vec2::ZERO {
                self.camera.pan_pixels(update.pan);
            }
        }
    }

    pub fn pinch_end(&mut self) {
        // ui.minimap.orders: a stationary two-finger tap is the touch
        // right-click (move order / attack at that point).
        if let Some(centre) = self.touch.pinch_end() {
            self.right_click(centre);
        }
    }

    /// Touch right-click (two-finger tap): minimap -> move order to that world
    /// point; world -> attack the picked enemy or move the selection.
    fn right_click(&mut self, at: Vec2) {
        let units: Vec<UnitId> = self
            .selection
            .iter()
            .copied()
            .filter(|id| self.sim.unit(*id).is_some_and(|unit| unit.alive))
            .collect();
        if units.is_empty() {
            return;
        }
        let minimap = touch_rect(self.layout.minimap);
        let world = if minimap.contains(at) {
            wc3_input::minimap_to_world(
                at,
                touch_rect(self.layout.minimap_image),
                self.assets.world_size,
            )
        } else if let Some(world) = self
            .camera
            .screen_to_world(at, |x, z| self.assets.height_at(x, z))
        {
            world
        } else {
            return;
        };
        if let Some(target) = self.pick_unit(at)
            && self
                .sim
                .unit(target)
                .is_some_and(|unit| unit.owner != self.player)
        {
            self.pending.push(Command::Attack { units, target });
            self.log(format!("RIGHT_CLICK attack target={}", target.0));
        } else {
            self.pending.push(Command::Move { units, to: world });
            self.log(format!("RIGHT_CLICK move at=({:.0},{:.0})", world.x, world.y));
        }
        self.move_marker = Some((world, 0.9));
        self.push_haptic(HapticCue::Order);
    }

    /// Resource intent with a worker selection: a tap on a gold mine or a
    /// living tree issues the gather order. Checked *before* `pick_unit` so a
    /// neutral creep standing on the mine does not turn the tap into an attack.
    /// Returns true when the tap was consumed.
    fn resource_tap_intent(&mut self, screen: Vec2) -> bool {
        let radius = 72.0 * self.layout.scale;
        let Some(workers) = self.selected_worker() else {
            return false;
        };
        if let Some(mine) = self
            .sim
            .gold_mines
            .iter()
            .map(|mine| mine.pos)
            .find(|mine| self.near_screen_world(screen, *mine, 24.0, radius))
        {
            self.log(format!(
                "INTENT harvest_gold pos=({:.0},{:.0}) units={}",
                mine.x,
                mine.y,
                workers.len()
            ));
            self.pending.push(Command::Harvest { units: workers, to: mine });
            self.move_marker = Some((mine, 1.2));
            self.toast("GATHERING GOLD", 1.2);
            self.push_haptic(HapticCue::Order);
            return true;
        }
        if let Some(tree) = self
            .sim
            .destructables
            .iter()
            .filter(|tree| tree.alive)
            .find(|tree| self.near_screen_world(screen, tree.pos, 24.0, radius))
        {
            let at = tree.pos;
            self.log(format!(
                "INTENT harvest_lumber pos=({:.0},{:.0}) units={}",
                at.x,
                at.y,
                workers.len()
            ));
            self.pending.push(Command::HarvestLumber { units: workers, to: at });
            self.move_marker = Some((at, 1.2));
            self.toast("GATHERING LUMBER", 1.2);
            self.push_haptic(HapticCue::Order);
            return true;
        }
        false
    }

    /// Resource/building tap intents: harvest (mine or tree), repair a damaged
    /// friendly building, return to the hall, rally with a building selected.
    /// Returns true when the tap was consumed by a contextual order.
    fn touch_tap_intent(&mut self, world: Vec2, screen: Vec2) -> bool {
        let radius = 72.0 * self.layout.scale;
        if let Some(workers) = self.selected_worker() {
            let repair_site = self
                .sim
                .alive_units()
                .filter(|unit| {
                    unit.owner == self.player && unit.is_building() && unit.hp < unit.max_hp
                })
                .find(|unit| self.near_screen_world(screen, unit.pos, 40.0, radius))
                .map(|unit| unit.id);
            if let Some(site) = repair_site {
                self.log(format!(
                    "INTENT repair target={} units={}",
                    site.0,
                    workers.len()
                ));
                self.pending.push(Command::Repair { units: workers, target: site });
                self.toast("REPAIRING", 1.2);
                self.push_haptic(HapticCue::Order);
                return true;
            }
            let hall = self
                .player_hall()
                .and_then(|hall| self.sim.unit(hall).map(|unit| unit.pos));
            if let Some(hall_pos) = hall
                && self.near_screen_world(screen, hall_pos, 40.0, radius)
            {
                self.log(format!(
                    "INTENT return_resources pos=({:.0},{:.0}) units={}",
                    hall_pos.x,
                    hall_pos.y,
                    workers.len()
                ));
                self.pending.push(Command::Move { units: workers, to: hall_pos });
                self.toast("RETURNING RESOURCES", 1.2);
                self.push_haptic(HapticCue::Order);
                return true;
            }
            // A worker selection that hit nothing near the finger falls
            // through to the generic move below; log it so a device probe can
            // tell an empty tap from a missed harvest intent.
            self.log(format!(
                "INTENT none world=({:.0},{:.0}) units={}",
                world.x,
                world.y,
                workers.len()
            ));
        }
        // Rally: a selected friendly building + a ground tap.
        if let Some(building) = self.selection.iter().copied().find(|id| {
            self.sim
                .unit(*id)
                .is_some_and(|unit| unit.owner == self.player && unit.is_building())
        }) {
            self.log(format!(
                "INTENT rally pos=({:.0},{:.0}) building={}",
                world.x,
                world.y,
                building.0
            ));
            self.sim.set_rally(building, Some(world));
            self.toast("RALLY POINT SET", 1.2);
            self.push_haptic(HapticCue::Order);
            return true;
        }
        false
    }

    /// Screen proximity for world-tap intents. The projection must use the
    /// terrain height under the object (plus a small lift), exactly like the
    /// model renderer: projecting the ground plane at y=0 on a raised map
    /// (Justice's bases are ~627u up) offsets the hit point by hundreds of
    /// pixels and every tree/mine tap would miss.
    fn near_screen_world(&self, screen: Vec2, point: Vec2, lift: f32, radius: f32) -> bool {
        let ground = self.assets.height_at(point.x, point.y);
        self.camera
            .world_to_screen(Vec3::new(point.x, ground + lift, point.y))
            .is_some_and(|projected| projected.distance(screen) <= radius)
    }

    /// Gold mine under a screen tap, using the same hit box as the worker's
    /// harvest intent (`resource_tap_intent`): 24 px above the ground, 72 dp
    /// radius scaled by the HUD layout. `None` when the finger missed.
    fn mine_at_screen(&self, screen: Vec2) -> Option<usize> {
        let radius = 72.0 * self.layout.scale;
        self.sim
            .gold_mines
            .iter()
            .position(|mine| self.near_screen_world(screen, mine.pos, 24.0, radius))
    }

    /// Select a gold mine (WC3: click a mine -> its info plate). Clears the
    /// unit selection, so the card rebuild below shows the mine's BASE+ZOOM card.
    fn select_mine(&mut self, index: usize) -> bool {
        let Some(mine) = self.sim.gold_mines.get(index) else {
            return false;
        };
        let (pos, remaining) = (mine.pos, mine.remaining);
        self.selected_mine = Some(index);
        self.selection.clear();
        self.selected_cells.clear();
        self.refresh_buttons();
        self.log(format!(
            "SELECT mine={index} remaining={remaining} pos=({:.0},{:.0})",
            pos.x, pos.y
        ));
        self.push_haptic(HapticCue::Select);
        self.toast(format!("GOLD MINE · {remaining}"), 1.4);
        true
    }

    fn tap_action(&mut self, pos: Vec2, double: bool) {
        let Some(world) = self
            .camera
            .screen_to_world(pos, |x, z| self.assets.height_at(x, z))
        else {
            return;
        };
        let picked = self.pick_unit(pos);
        let tap_line = format!(
            "TAP screen=({:.0},{:.0}) world=({:.0},{:.0}) picked={:?} sel={}",
            pos.x,
            pos.y,
            world.x,
            world.y,
            picked.map(|id| id.0),
            self.selection.len()
        );
        if std::env::var_os("H2_TAP_DEBUG").is_some() {
            eprintln!(
                "[tap] {tap_line} camera=({:.0},{:.0}) dist={:.0} viewport={:?} screen={:?}",
                self.camera.target.x,
                self.camera.target.y,
                self.camera.distance,
                self.camera.viewport,
                self.screen
            );
        }
        self.log(tap_line);
        if let Some(spell) = self.targeting {
            let Some(caster) = self.caster_for(spell) else {
                self.targeting = None;
                self.toast("SELECT A CASTER", 1.2);
                self.push_haptic(HapticCue::Deny);
                return;
            };
            let target = match spell.def().target {
                TargetKind::Unit => match picked {
                    Some(id) => SpellTarget::Unit(id),
                    None => {
                        self.toast("TAP A TARGET UNIT", 1.2);
                        self.push_haptic(HapticCue::Deny);
                        return;
                    }
                },
                TargetKind::Point => SpellTarget::Point(world),
                TargetKind::None => SpellTarget::None,
            };
            self.perform_cast(caster, spell, target);
            return;
        }
        // WC3 smart order: with a worker selection a tap on a mine/tree is a
        // gather order even when a unit stands on the resource. Real melee maps
        // guard every mine with a neutral creep, so the picked creep must not
        // steal the tap. A friendly unit under the finger still selects (that is
        // the only way to change the selection on a phone).
        let picked_friendly = picked
            .and_then(|id| self.sim.unit(id))
            .is_some_and(|unit| unit.owner == self.player);
        if !picked_friendly && self.resource_tap_intent(pos) {
            if std::env::var_os("H2_TAP_DEBUG").is_some() {
                eprintln!("[intent] {}", self.log.last().cloned().unwrap_or_default());
            }
            return;
        }
        if let Some(id) = picked {
            let Some(unit) = self.sim.unit(id) else {
                return;
            };
            if unit.owner == self.player {
                if double {
                    let kind = unit.kind;
                    self.select_same_kind(kind);
                } else {
                    // ui.selection.add: Shift/long-press merges, plain tap replaces.
                    crate::ui::selection::tap_select(self, id, self.touch.additive());
                    if let Some(first) = self.selection.last().copied() {
                        self.play_unit_ack(first);
                        // The tap already acknowledged: stop `update_audio`
                        // from repeating the same line on the next frame.
                        self.audio_last_selection = Some(first);
                    }
                }
                // The card must follow the new selection on this same tap:
                // the previous version returned with a stale (empty) card, so
                // tapping a worker showed no build buttons at all.
                self.refresh_buttons();
                self.push_haptic(HapticCue::Select);
            } else if let Some(units) = self.selected_army() {
                self.log(format!(
                    "INTENT attack target={} units={}",
                    id.0,
                    units.len()
                ));
                self.pending.push(Command::Attack { units, target: id });
                self.move_marker = Some((world, 0.8));
                self.push_haptic(HapticCue::Order);
                return;
            } else {
                self.toast("SELECT YOUR UNITS FIRST", 1.2);
                self.push_haptic(HapticCue::Deny);
                return;
            }
        }
        // WC3: with nothing selected, a tap on a gold mine opens the mine's info
        // plate (the worker harvest intent above already consumed its own tap).
        if picked.is_none()
            && self.selection.is_empty()
            && let Some(index) = self.mine_at_screen(pos)
            && self.select_mine(index)
        {
            return;
        }
        if self.touch_tap_intent(world, pos) {
            if std::env::var_os("H2_TAP_DEBUG").is_some() {
                eprintln!(
                    "[intent] {}",
                    self.log.last().cloned().unwrap_or_default()
                );
            }
            return;
        }
        if let Some(units) = self.selected_army() {
            if self.patrol_order {
                // WC3 patrol: walk between the current position and the tap,
                // engaging anything met on either leg.
                self.patrol_order = false;
                self.log(format!(
                    "ORDER PATROL units={} at=({:.0},{:.0})",
                    units.len(),
                    world.x,
                    world.y
                ));
                for id in &units {
                    let from = self.sim.unit(*id).map(|unit| unit.pos);
                    if let Some(from) = from {
                        self.sim.issue(&[*id], wc3_sim::OrderSpec::Patrol(from, world));
                    }
                }
                self.move_marker = Some((world, 1.2));
                self.toast("PATROL", 1.2);
            } else if self.attack_move {
                self.log(format!(
                    "ORDER ATTACK-MOVE units={} at=({:.0},{:.0})",
                    units.len(),
                    world.x,
                    world.y
                ));
                self.pending.push(Command::MoveAttack { units, to: world });
                self.attack_move = false;
                self.move_order = false;
                self.patrol_order = false;
                self.toast("ATTACK-MOVE", 1.2);
            } else {
                self.log(format!(
                    "INTENT move units={} at=({:.0},{:.0})",
                    units.len(),
                    world.x,
                    world.y
                ));
                self.pending.push(Command::Move { units, to: world });
            }
            self.move_order = false;
            self.move_marker = Some((world, 1.2));
            self.push_haptic(HapticCue::Order);
        } else {
            self.selection.clear();
        }
        // Selection changed: rebuild the card and touch snapshot immediately,
        // not on the next simulation tick, so the next tap uses the new slots.
        self.refresh_buttons();
    }

    fn pick_unit(&self, pos: Vec2) -> Option<UnitId> {
        let mut best: Option<(UnitId, f32, bool)> = None;
        for unit in self.sim.alive_units() {
            let y = self.assets.height_at(unit.pos.x, unit.pos.y)
                + self.assets.model_for_def(unit.def().id).height * 0.55;
            let Some(screen) = self
                .camera
                .world_to_screen(Vec3::new(unit.pos.x, y, unit.pos.y))
            else {
                continue;
            };
            let radius = if unit.is_building() { 110.0 } else { 52.0 } * self.layout.scale;
            let distance = screen.distance(pos);
            if distance > radius {
                continue;
            }
            let own = unit.owner == self.player;
            let better = match best {
                None => true,
                Some((_, best_distance, best_own)) => {
                    (own && !best_own) || (own == best_own && distance < best_distance)
                }
            };
            if better {
                best = Some((unit.id, distance, own));
            }
        }
        best.map(|(id, _, _)| id)
    }

    fn select_same_kind(&mut self, kind: UnitKind) {
        // ui.selection.double: same type, on screen only (WC3 double-click).
        crate::ui::selection::type_select_on_screen(self, kind);
        let name = self.assets.unit_name(kind);
        match self.assets.unit_tooltip(kind) {
            Some(tooltip) => {
                let text = game_tooltip_text(&tooltip, 120);
                self.toast(format!("{name} — {text}"), 2.4);
            }
            None => self.toast(format!("SELECTED {}", name.to_uppercase()), 1.2),
        }
    }

    fn box_select(&mut self, a: Vec2, b: Vec2) {
        let (Some(wa), Some(wb)) = (
            self.camera
                .screen_to_world(a, |x, z| self.assets.height_at(x, z)),
            self.camera
                .screen_to_world(b, |x, z| self.assets.height_at(x, z)),
        ) else {
            return;
        };
        let min = wa.min(wb);
        let max = wa.max(wb);
        // ui.selection.add: Shift keeps the current group, plain drag replaces.
        crate::ui::selection::box_select(self, min, max, self.touch.additive());
        if !self.selection.is_empty() {
            self.push_haptic(HapticCue::Select);
        }
        self.refresh_buttons();
    }

    fn place_building(&mut self, world: Vec2) {
        let Some(def_id) = self.placing else {
            self.log("PLACE_NO_MODE".to_string());
            return;
        };
        // The tap is the placement attempt: log the ghost exactly where the
        // building would appear, with the legality the player sees.
        let denial = self.sim.place_denial(self.player, def_id, world);
        self.log(format!(
            "GHOST def={} at=({:.0},{:.0}) legal={} reason={}",
            def_id,
            world.x,
            world.y,
            denial.is_none(),
            denial.unwrap_or("ok")
        ));
        self.ghost_cell = None;
        let Some(workers) = self.selected_worker() else {
            self.placing = None;
            self.placing_at = None;
            return;
        };
        let Some(def) = wc3_sim::def_of(def_id) else {
            self.placing = None;
            self.placing_at = None;
            return;
        };
        let cell = self.sim.grid.world_to_cell(world);
        if !self.sim.grid.walkable(cell.0, cell.1) {
            self.log(format!("PLACE_BLOCKED cell={:?}", cell));
            self.toast("CANNOT BUILD THERE", 1.6);
            self.push_haptic(HapticCue::Deny);
            return;
        }
        // Requirements first: the ghost already shows the reason, the toast must
        // say it too instead of leaving the player with a silent refusal.
        if let Some(reason) = self.sim.place_denial(self.player, def_id, world) {
            if reason == "missing requirement" {
                self.log(format!("DENY BUILD{reason:?} def={def_id}"));
                self.toast("REQUIREMENTS NOT MET", 1.8);
                self.push_haptic(HapticCue::Deny);
                self.placing = None;
                self.placing_at = None;
                return;
            }
        }
        if !self.can_afford_def(def_id) {
            self.log(format!("DENY BUILD gold def={def_id}"));
            self.toast("NOT ENOUGH GOLD", 1.6);
            self.placing = None;
            self.placing_at = None;
            self.push_haptic(HapticCue::Deny);
            return;
        }
        let builder = workers[0];
        match crate::sim_bridge::place_from_card(&mut self.sim, builder, def_id, world) {
            Ok(crate::sim_bridge::Placement::Built(id)) => {
                // Hall line: cost and spawn happen atomically in the sim.
                self.log(format!(
                    "PLACE_BUILT {def_id} cost={} gold={}",
                    def.stats.cost, self.sim.resources[self.player as usize]
                ));
                self.selection = vec![id];
        self.build_page = 0;
                self.toast(format!("BUILDING {}", def.name), 1.4);
            }
            Ok(crate::sim_bridge::Placement::Site(site)) => {
                // Farms/towers use the canonical construction-site lifecycle;
                // the worker walks to the site when it is out of range.
                self.log(format!(
                    "PLACE_SITE {def_id} cost={} gold={}",
                    def.stats.cost, self.sim.resources[self.player as usize]
                ));
                self.selection = vec![site];
                self.toast(format!("BUILDING {}", def.name), 1.4);
            }
            Err(reason) => {
                self.log(format!("DENY BUILD {reason}"));
                self.toast(reason.to_ascii_uppercase(), 1.6);
                self.push_haptic(HapticCue::Deny);
                return;
            }
        }
        self.placing = None;
        self.placing_at = None;
        // The new building becomes the selection: its training card must show
        // immediately, not on the next simulation tick.
        self.refresh_buttons();
        self.push_haptic(HapticCue::Order);
    }

    /// First selected living unit that knows `spell`.
    fn caster_for(&self, spell: Spell) -> Option<UnitId> {
        self.selection.iter().copied().find(|id| {
            self.sim
                .unit(*id)
                .map(|unit| unit.alive && unit.def().spells.contains(&spell))
                .unwrap_or(false)
        })
    }

    /// HUD ability slot for the selected caster (label, mana sub, action,
    /// enabled).  `None` when no selected unit knows a spell in this slot.
    pub fn ability_button(&self, index: usize) -> Option<(String, String, UiAction, bool)> {
        let caster = self.selection.iter().copied().find_map(|id| {
            self.sim
                .unit(id)
                .filter(|unit| unit.alive && !unit.def().spells.is_empty())
        })?;
        let spell = *caster.def().spells.get(index)?;
        let def = spell.def();
        let ready = caster.mana >= def.mana && caster.spell_cd[spell.index()] == 0;
        Some((
            spell.name().to_ascii_uppercase(),
            format!("{}M", def.mana),
            UiAction::Cast(spell),
            ready,
        ))
    }

    fn perform_cast(&mut self, caster: UnitId, spell: Spell, target: SpellTarget) {
        match self.sim.cast(caster, spell, target) {
            Ok(()) => {
                self.targeting = None;
                self.toast(spell.name().to_ascii_uppercase(), 1.2);
                self.push_haptic(HapticCue::Order);
                // Retail ability sound (AbilitySounds.slk); spells without
                // a matching entry stay silent instead of inventing one.
                if let (Some(audio), Some(sound)) =
                    (self.assets.audio.as_mut(), spell_sound(spell))
                {
                    let _ = audio.spell(sound, None);
                }
            }
            Err(reason) => {
                self.toast(reason.to_ascii_uppercase(), 1.4);
                self.push_haptic(HapticCue::Deny);
            }
        }
    }

    /// WC3 extended tooltip for a card slot: the shipped object `Tip`/
    /// `Ubertip` (colour codes flattened by the HUD) plus the cost/hotkey line,
    /// resolved from the profile data of the action's type id.
    pub fn card_tooltip(&self, button: &crate::hud::Button) -> String {
        let def_id = match button.action {
            UiAction::BuildDef(id) | UiAction::TrainDef(id) => Some(id),
            UiAction::Build(kind) | UiAction::Train(kind) => Some(kind.canonical_id()),
            UiAction::UseItem(slot) => self
                .selection
                .iter()
                .filter_map(|id| self.sim.unit(*id))
                .find(|unit| !unit.items.is_empty())
                .and_then(|unit| unit.items.get(usize::from(slot)).copied())
                .and_then(|id| self.sim.item(id))
                .map(|item| item.def().id),
            _ => None,
        };
        let mut lines: Vec<String> = Vec::new();
        if let Some(info) = def_id.and_then(|id| self.assets.info_for_def(id)) {
            if let Some(tip) = info.tip.as_deref() {
                let tip = game_tooltip_text(tip, 160);
                if !tip.is_empty() {
                    lines.push(tip);
                }
            }
            if let Some(ubertip) = info.ubertip.as_deref() {
                let ubertip = game_tooltip_text(ubertip, 260);
                if !ubertip.is_empty() {
                    lines.push(ubertip);
                }
            }
        }
        let head = if button.sub.is_empty() {
            button.label.clone()
        } else {
            format!("{}  ·  {}", button.label, button.sub)
        };
        if lines.is_empty() {
            lines.push(head);
        } else {
            lines.insert(0, head);
        }
        if button.action == UiAction::SelectArmy {
            lines.push("HOLD SAVE GROUP  ·  2x RECALL".to_string());
        }
        lines.join("\n")
    }

    /// Retail tooltip section 3: cost / food / build time / requirements of a
    /// card entry, straight from the typed data (max 8 rows).
    pub fn card_cost_lines(&self, button: &crate::hud::Button) -> Vec<String> {
        let def_id = match button.action {
            UiAction::BuildDef(id) | UiAction::TrainDef(id) => Some(id),
            UiAction::Build(kind) | UiAction::Train(kind) => Some(kind.canonical_id()),
            _ => None,
        };
        let Some(def_id) = def_id else { return Vec::new() };
        let mut lines: Vec<String> = Vec::new();
        if let Some(def) = wc3_sim::data::def_of(def_id) {
            let mut cost: Vec<String> = Vec::new();
            if def.stats.cost > 0 {
                cost.push(format!("{} GOLD", def.stats.cost));
            }
            if def.stats.lumber_cost > 0 {
                cost.push(format!("{} LUMBER", def.stats.lumber_cost));
            }
            if !cost.is_empty() {
                lines.push(format!("COST {}", cost.join(" · ")));
            }
            if def.stats.supply > 0 {
                lines.push(format!("FOOD {}", def.stats.supply));
            }
            if def.stats.train_ticks > 0 {
                lines.push(format!(
                    "BUILD {:.0}S",
                    def.stats.train_ticks as f32 / wc3_sim::TICKS_PER_SECOND as f32
                ));
            }
        }
        if let Some(entry) = self.assets.menu_entry(def_id)
            && !entry.requires_names.is_empty()
        {
            lines.push(format!("REQUIRES {}", entry.requires_names.join(", ")));
        }
        lines.truncate(crate::hud::mobile_hud::TOOLTIP_MAX_COST_LINES);
        lines
    }

    /// Coarse fog grid for the minimap: 0 = never seen, 1 = explored, 2 =
    /// currently visible. Sampled at `width x height` cells over the world.
    pub fn minimap_fog_cells(&self, width: usize, height: usize) -> Vec<u8> {
        let width = width.max(1);
        let height = height.max(1);
        let world = self.assets.world_size.max(Vec2::splat(1.0));
        let mut cells = Vec::with_capacity(width * height);
        for gy in 0..height {
            for gx in 0..width {
                let pos = Vec2::new(
                    (gx as f32 + 0.5) / width as f32 * world.x,
                    (gy as f32 + 0.5) / height as f32 * world.y,
                );
                let value = if self.sim.is_visible(self.player, pos) {
                    2
                } else if self.sim.is_explored(self.player, pos) {
                    1
                } else {
                    0
                };
                cells.push(value);
            }
        }
        cells
    }

    /// Live quest-log rows: the scenario objective plus the numbers WC3 shows
    /// in the quest log (resources, supply, army, idle workers, hero level).
    pub fn quest_log_lines(&self) -> Vec<String> {
        let (used, cap) = self.supply();
        let idle = self.sim.idle_workers(self.player).len();
        // Objective text from the running map's `war3map.w3i` when the install
        // carries it: the quest log names the map, its author and the players.
        let mut lines: Vec<String> = Vec::new();
        if let Some(info) = self.map_info.as_ref() {
            lines.push(format!("MAP {}", info.name.to_uppercase()));
            if !info.author.trim().is_empty() {
                lines.push(format!("BY {}", info.author.to_uppercase()));
            }
            if !info.players.trim().is_empty() {
                lines.push(format!("PLAYERS {}", info.players.to_uppercase()));
            }
            if !info.description.trim().is_empty() {
                let mut description = info.description.clone();
                description.truncate(220);
                lines.push(description.to_uppercase());
            }
        }
        lines.push("MAIN QUEST".to_string());
        lines.push("DESTROY THE ENEMY TOWN HALL.".to_string());
        lines.extend(vec![
            format!(
                "GOLD {}   LUMBER {}",
                self.sim.resources[self.player as usize].max(0),
                self.sim.lumber[self.player as usize].max(0)
            ),
            format!(
                "SUPPLY {used}/{cap}   {}",
                self.upkeep_label().to_uppercase()
            ),
            format!(
                "TRAINED {}   KILLS {}   ARMY {}",
                self.total_trained,
                self.total_kills,
                self.army_count()
            ),
            format!("IDLE WORKERS {idle}"),
        ]);
        if let Some(hero) = self
            .sim
            .alive_units()
            .find(|unit| unit.owner == self.player && unit.def().hero)
        {
            let name = self
                .assets
                .name_for_def(hero.def().id)
                .unwrap_or_else(|| hero.def().name.to_string());
            lines.push(format!("HERO {}  LEVEL {}", name.to_uppercase(), hero.level));
        }
        lines
    }

    fn activate(&mut self, action: UiAction) {
        match action {
            UiAction::Stop => {
                let units: Vec<UnitId> = self
                    .selection
                    .iter()
                    .copied()
                    .filter(|id| self.sim.unit(*id).map(|u| u.alive).unwrap_or(false))
                    .collect();
                if !units.is_empty() {
                    self.pending.push(Command::Stop { units });
                }
            }
            UiAction::SelectArmy => {
                self.selection = self
                    .sim
                    .alive_units()
                    .filter(|unit| unit.owner == self.player && !unit.is_building())
                    .map(|unit| unit.id)
                    .collect();
                if let Some(first) = self.selection.first().and_then(|id| self.sim.unit(*id)) {
                    self.camera.centre_on(first.pos);
                }
                self.push_haptic(HapticCue::Select);
            }
            UiAction::CentreBase => {
                if let Some(hall) = self.player_hall() {
                    if let Some(unit) = self.sim.unit(hall) {
                        self.camera.centre_on(unit.pos);
                    }
                    // Pressing BASE is also the reliable way to select the hall
                    // on a phone, where troops can cover its screen centre.
                    self.selection = vec![hall];
                } else {
                    self.camera.centre_on(self.base_pos);
                }
                self.push_haptic(HapticCue::Select);
            }
            UiAction::Gather => {
                if let Some(workers) = self.selected_worker() {
                    // Nearest mine to the selected worker, not the first of the
                    // two base-side entries: the map has seven.
                    let worker_pos = workers
                        .first()
                        .and_then(|id| self.sim.unit(*id))
                        .map(|unit| unit.pos)
                        .unwrap_or(self.base_pos);
                    let mine = self
                        .sim
                        .nearest_gold_mine(worker_pos)
                        .unwrap_or(self.mines[0]);
                    self.pending.push(Command::Harvest {
                        units: workers,
                        to: mine,
                    });
                    self.move_marker = Some((mine, 1.2));
                    self.toast("GATHERING GOLD", 1.2);
                    self.push_haptic(HapticCue::Order);
                } else {
                    self.push_haptic(HapticCue::Deny);
                }
            }
            UiAction::AttackMove => {
                self.attack_move = !self.attack_move;
                if self.attack_move {
                    self.toast("TAP A DESTINATION TO ATTACK-MOVE", 2.4);
                }
                self.push_haptic(HapticCue::Press);
            }
            UiAction::Move => {
                self.move_order = !self.move_order;
                self.attack_move = false;
                self.patrol_order = false;
                if self.move_order {
                    self.toast("TAP A DESTINATION TO MOVE", 2.4);
                }
                self.push_haptic(HapticCue::Press);
            }
            UiAction::Patrol => {
                self.patrol_order = !self.patrol_order;
                self.attack_move = false;
                self.move_order = false;
                if self.patrol_order {
                    self.toast("TAP A DESTINATION TO PATROL", 2.4);
                }
                self.push_haptic(HapticCue::Press);
            }
            UiAction::Hold => {
                if let Some(units) = self.selected_army() {
                    self.pending.push(Command::Hold { units });
                    self.toast("HOLD POSITION", 1.2);
                    self.push_haptic(HapticCue::Order);
                } else {
                    self.push_haptic(HapticCue::Deny);
                }
            }
            UiAction::Train(kind) => {
                let Some(producer) = self.selected_producer() else {
                    self.toast("SELECT A BARRACKS", 1.4);
                    self.push_haptic(HapticCue::Deny);
                    return;
                };
                let def_id = kind.canonical_id();
                let line = self.train_log_line(def_id, producer);
                self.log(line);
                self.pending.push(Command::Train {
                    building: producer,
                    kind,
                });
            }
            UiAction::BuildPageNext => {
                let pages = self.build_pages();
                if pages > 1 {
                    self.build_page = (self.build_page + 1) % pages;
                    self.refresh_buttons();
                    self.toast(
                        format!("BUILD PAGE {}/{}", self.build_page + 1, pages),
                        1.2,
                    );
                }
                self.push_haptic(HapticCue::Press);
            }
            UiAction::Build(kind) => {
                // ui.placement: BUILD again cancels placement (WC3 toggle).
                if self.placing == Some(kind.canonical_id()) {
                    self.placing = None;
                    self.placing_at = None;
                    self.toast("BUILD CANCELLED", 1.0);
                } else if self.selected_worker().is_some() {
                    self.begin_placement(kind.canonical_id());
                } else {
                    self.toast("SELECT A PEASANT", 1.4);
                }
            }
            UiAction::BuildDef(def_id) => {
                if self.placing == Some(def_id) {
                    self.placing = None;
                    self.placing_at = None;
                    self.toast("BUILD CANCELLED", 1.0);
                } else if self.selected_worker().is_some() {
                    if self.can_afford_def(def_id) {
                        self.begin_placement(def_id);
                    } else {
                        self.toast("NOT ENOUGH GOLD", 1.4);
                        self.push_haptic(HapticCue::Deny);
                    }
                } else {
                    self.toast("SELECT A PEASANT", 1.4);
                }
            }
            UiAction::TrainDef(def_id) => {
                let Some(producer) = self.selected_producer() else {
                    self.toast("SELECT A BARRACKS", 1.4);
                    self.push_haptic(HapticCue::Deny);
                    return;
                };
                match self.sim.train_def(producer, def_id) {
                    Ok(()) => {
                        let line = self.train_log_line(def_id, producer);
                        self.log(line);
                        self.push_haptic(HapticCue::Order);
                    }
                    Err(reason) => {
                        self.toast(reason.to_ascii_uppercase(), 1.6);
                        self.push_haptic(HapticCue::Deny);
                    }
                }
            }
            UiAction::Cast(spell) => {
                let Some(caster) = self.caster_for(spell) else {
                    self.toast("SELECT A CASTER", 1.2);
                    self.push_haptic(HapticCue::Deny);
                    return;
                };
                match spell.def().target {
                    TargetKind::None => self.perform_cast(caster, spell, SpellTarget::None),
                    _ => {
                        self.targeting = Some(spell);
                        self.placing = None;
                        self.placing_at = None;
                        self.attack_move = false;
                        self.move_order = false;
                        self.patrol_order = false;
                        self.toast(format!("TAP TARGET: {}", spell.name()), 2.0);
                        self.push_haptic(HapticCue::Press);
                    }
                }
            }
            UiAction::UseItem(slot) => {
                let unit = self.selection.iter().copied().find(|id| {
                    self.sim
                        .unit(*id)
                        .is_some_and(|unit| unit.alive && usize::from(slot) < unit.items.len())
                });
                match unit {
                    Some(unit) => {
                        let item = self
                            .sim
                            .unit(unit)
                            .and_then(|unit| unit.items.get(usize::from(slot)).copied());
                        if let Some(item) = item {
                            let name = self
                                .sim
                                .item(item)
                                .map(|item| item.def().name)
                                .unwrap_or("ITEM");
                            match self.sim.use_item(unit, item) {
                                Ok(()) => {
                                    self.toast(format!("USED {name}"), 1.2);
                                    self.push_haptic(HapticCue::Order);
                                }
                                Err(reason) => {
                                    self.toast(reason.to_ascii_uppercase(), 1.4);
                                    self.push_haptic(HapticCue::Deny);
                                }
                            }
                        }
                    }
                    None => {
                        self.push_haptic(HapticCue::Deny);
                    }
                }
            }
            UiAction::Research(upgrade_id) => {
                let Some(building) = self.selected_building() else {
                    self.toast("SELECT A BUILDING", 1.4);
                    self.push_haptic(HapticCue::Deny);
                    return;
                };
                match self.sim.research_upgrade(building, upgrade_id) {
                    Ok(()) => {
                        self.log(format!(
                            "GAME_RESEARCH {upgrade_id} building={}",
                            building.0
                        ));
                        self.toast("RESEARCHING", 1.2);
                        self.push_haptic(HapticCue::Order);
                    }
                    Err(reason) => {
                        self.toast(reason.to_ascii_uppercase(), 1.6);
                        self.push_haptic(HapticCue::Deny);
                    }
                }
            }
            UiAction::CancelConstruction => {
                let site = self.selection.iter().copied().find(|id| {
                    self.sim
                        .unit(*id)
                        .map(|unit| {
                            unit.alive && unit.owner == self.player && unit.construct.is_some()
                        })
                        .unwrap_or(false)
                });
                let Some(site) = site else {
                    self.toast("NO SITE SELECTED", 1.4);
                    self.push_haptic(HapticCue::Deny);
                    return;
                };
                let cancelled = self.sim.cancel_construction(site);
                self.log(format!(
                    "GAME_CANCEL_BUILD site={} refunded={cancelled}",
                    site.0
                ));
                if cancelled {
                    self.toast("CONSTRUCTION CANCELLED", 1.4);
                    self.push_haptic(HapticCue::Order);
                } else {
                    self.toast("CANNOT CANCEL", 1.2);
                    self.push_haptic(HapticCue::Deny);
                }
            }
            UiAction::PickupNearestItem => {
                let unit = self.selection.iter().copied().find(|id| {
                    self.sim
                        .unit(*id)
                        .map(|unit| unit.alive && unit.owner == self.player)
                        .unwrap_or(false)
                });
                let Some(unit) = unit else {
                    self.toast("SELECT A HERO", 1.4);
                    self.push_haptic(HapticCue::Deny);
                    return;
                };
                let pos = self.sim.unit(unit).map(|unit| unit.pos).unwrap_or_default();
                // `Sim::pickup_item` reaches 180 world units; offer the
                // nearest ground item inside that reach.
                let nearest = self
                    .sim
                    .items
                    .iter()
                    .filter(|item| item.owner.is_none())
                    .map(|item| (item.id, item.def().name, item.pos.distance(pos)))
                    .filter(|(_, _, distance)| *distance <= 180.0)
                    .min_by(|a, b| a.2.total_cmp(&b.2));
                let Some((item, name, _)) = nearest else {
                    self.toast("NO ITEM IN REACH", 1.4);
                    self.push_haptic(HapticCue::Deny);
                    return;
                };
                match self.sim.pickup_item(unit, item) {
                    Ok(()) => {
                        self.log(format!(
                            "GAME_PICKUP item={} unit={} def={}",
                            item.0, unit.0, name
                        ));
                        self.toast(format!("PICKED UP {}", name.to_uppercase()), 1.4);
                        self.push_haptic(HapticCue::Order);
                    }
                    Err(reason) => {
                        self.toast(reason.to_ascii_uppercase(), 1.6);
                        self.push_haptic(HapticCue::Deny);
                    }
                }
            }
            UiAction::BuyItem(item_id) => {
                let shop = self.selected_shop();
                let shop_pos = shop.and_then(|shop| self.sim.unit(shop)).map(|unit| unit.pos);
                let buyer = self
                    .selected_army()
                    .and_then(|ids| ids.first().copied())
                    .or_else(|| {
                        // No hero selected: the shop serves the nearest living
                        // player unit in range (the sim enforces SHOP_RANGE).
                        let shop_pos = shop_pos?;
                        self.sim
                            .alive_units()
                            .filter(|unit| {
                                unit.owner == self.player
                                    && !unit.is_building()
                                    && unit.pos.distance(shop_pos) <= wc3_sim::SHOP_RANGE
                            })
                            .map(|unit| unit.id)
                            .next()
                    });
                let (Some(shop), Some(buyer)) = (shop, buyer) else {
                    self.toast("SELECT A SHOP AND A HERO", 1.6);
                    self.push_haptic(HapticCue::Deny);
                    return;
                };
                match self.sim.buy_item(buyer, shop, item_id) {
                    Ok(item) => {
                        self.log(format!(
                            "GAME_BUY_ITEM item={item_id} id={} shop={} buyer={}",
                            item.0, shop.0, buyer.0
                        ));
                        self.toast(format!("BOUGHT {}", item_id.to_uppercase()), 1.4);
                        self.push_haptic(HapticCue::Order);
                    }
                    Err(reason) => {
                        self.toast(reason.to_ascii_uppercase(), 1.6);
                        self.push_haptic(HapticCue::Deny);
                    }
                }
            }
            UiAction::BuyUnit(unit_id) => {
                let Some(shop) = self.selected_shop() else {
                    self.toast("SELECT A SHOP", 1.4);
                    self.push_haptic(HapticCue::Deny);
                    return;
                };
                // `Sim::buy_unit` charges the shop's owner and spawns for it, a
                // correct player purchase only for the player's own shop; a
                // neutral tavern needs the buyer argument the sim does not have.
                let shop_owner = self.sim.unit(shop).map(|unit| unit.owner);
                if shop_owner != Some(self.player) {
                    self.log(format!(
                        "UI_MISSING buy_unit shop={} reason=neutral-shop-owner",
                        shop.0
                    ));
                    self.toast("THIS SHOP IS NOT YOURS", 1.6);
                    self.push_haptic(HapticCue::Deny);
                    return;
                }
                match self.sim.buy_unit(shop, unit_id) {
                    Ok(unit) => {
                        self.log(format!(
                            "GAME_BUY_UNIT unit={unit_id} id={} shop={}",
                            unit.0, shop.0
                        ));
                        self.toast(format!("HIRED {}", unit_id.to_uppercase()), 1.4);
                        self.push_haptic(HapticCue::Order);
                    }
                    Err(reason) => {
                        self.toast(reason.to_ascii_uppercase(), 1.6);
                        self.push_haptic(HapticCue::Deny);
                    }
                }
            }
            UiAction::QuestLog => {
                self.dialog = if self.dialog.is_some() {
                    None
                } else {
                    Some(crate::hud::mobile_hud::DialogScreen::QuestLog)
                };
                self.push_haptic(HapticCue::Press);
            }
            UiAction::ZoomIn => self.camera.zoom_by(0.85),
            UiAction::ZoomOut => self.camera.zoom_by(1.18),
            UiAction::Restart => self.restart(),
        }
        // Every card action can change what the card should show (see the
        // selection actions above): rebuild it on the same tap instead of
        // waiting for the next simulation frame.
        self.refresh_buttons();
    }

    /// Menu of the current selection: a producer's trainees or a worker's
    /// structures, filtered to ids the simulation can actually spawn.
    ///
    /// The producer is *any* building with a non-empty `Trains=` list, not just
    /// the main hall: a barracks/altar/workshop card must be data-driven too.
    pub fn compute_menu(&self) -> Vec<wc3_data::MenuEntry> {
        let producer = self
            .selected_producer()
            .and_then(|id| self.sim.unit(id))
            .map(|unit| unit.def().id);
        let worker = self
            .selected_worker()
            .and_then(|ids| ids.first().copied())
            .and_then(|id| self.sim.unit(id))
            .map(|unit| unit.def().id);
        let mut menu = if let Some(def_id) = producer {
            self.assets.train_menu(def_id)
        } else if let Some(def_id) = worker {
            self.assets.build_menu(def_id)
        } else {
            Vec::new()
        };
        menu.retain(|entry| wc3_sim::data::def_of(&entry.id).is_some());
        menu
    }

    /// `(entry, action)` pairs for the card, with the roster's static id so the
    /// action stays `&'static str`.
    pub fn menu_actions(&self) -> Vec<(wc3_data::MenuEntry, UiAction)> {
        self.menu
            .iter()
            .filter_map(|entry| {
                let def = wc3_sim::data::def_of(&entry.id)?;
                let action = if def.is_building() {
                    UiAction::BuildDef(def.id)
                } else {
                    UiAction::TrainDef(def.id)
                };
                Some((entry.clone(), action))
            })
            .collect()
    }

    /// Affordability of a typed menu entry (gold + food headroom for units).
    pub fn can_afford_menu(&self, entry: &wc3_data::MenuEntry) -> bool {
        let gold = entry.gold.unwrap_or(0).max(0) as i32;
        if self.sim.resources[self.player as usize] < gold {
            return false;
        }
        if let Some(food) = entry.food.filter(|food| *food > 0) {
            let (used, cap) = self.supply();
            if used + food > cap {
                return false;
            }
        }
        true
    }

    fn toast(&mut self, text: impl Into<String>, seconds: f32) {
        let text = text.into();
        // The HUD draws a stack of rows: newest first, at most three. The
        // single `toast` field mirrors the newest line for older callers.
        self.toast_queue.retain(|(line, _)| line != &text);
        self.toast_queue.insert(0, (text.clone(), seconds));
        self.toast_queue.truncate(3);
        self.toast = Some((text, seconds));
    }

    // ---------------------------------------------------------------- scene

    pub fn scene(&self) -> Scene {
        let view_proj = self.camera.view_proj();
        let mut units = Vec::new();
        let mut rings = Vec::new();
        let mut health = Vec::new();
        for unit in self.sim.alive_units() {
            // Fog of war: an enemy unit outside the player's sight radius is
            // not drawn at all (WC3 shows nothing there). The sim's visibility
            // grid is authoritative; the minimap keeps the last-known blip.
            if unit.owner != self.player && !self.sim.is_visible(self.player, unit.pos) {
                continue;
            }
            // Per-definition model: a burrow, a barracks and a great hall each
            // render their own MDX/texture, not the shared four-kind fallback.
            let def_id = unit.def().id;
            let model_index = self.assets.model_index_for_def(def_id);
            let ground = self.assets.height_at(unit.pos.x, unit.pos.y);
            let model = self.assets.model_for_def(def_id);
            // Authored base offset: lift models whose visible geometry starts
            // below their origin so the lowest vertex sits on the terrain.
            let base = ground - model.pivot;
            // Uniform scale: scaling only pivot/height lifted the model off
            // its foundation and made buildings look stretched upward.
            let world = Mat4::from_scale_rotation_translation(
                Vec3::splat(model.scale.max(0.01)),
                Quat::from_rotation_y(-unit.facing + model.yaw_offset),
                Vec3::new(unit.pos.x, base, unit.pos.y),
            );
            let tint = if unit.owner == self.player {
                [0.55, 0.72, 1.0, 1.0]
            } else {
                [1.0, 0.48, 0.42, 1.0]
            };
            let visuals = self.visuals.get(&unit.id.0);
            let nodes = visuals
                .map(|visual| visual.animator.world_matrices())
                .unwrap_or_default();
            let time_ms = visuals
                .map(|visual| visual.animator.frame_time())
                .unwrap_or(0);
            let sequence = visuals.and_then(|visual| visual.animator.sequence.clone());
            let mesh_alpha = self
                .assets
                .mesh_alpha(model_index, sequence.as_ref(), time_ms);
            let layer_alpha = self
                .assets
                .mesh_layer_alpha(model_index, sequence.as_ref(), time_ms);
            units.push(UnitDraw {
                model: model_index,
                world,
                tint,
                nodes,
                mesh_alpha,
                layer_alpha,
            });
            if self.selection.contains(&unit.id) {
                rings.push(RingDraw {
                    pos: Vec3::new(unit.pos.x, ground + 3.0, unit.pos.y),
                    radius: if unit.is_building() { 190.0 } else { 62.0 },
                    color: [0.25, 1.0, 0.35, 0.85],
                });
            }
        }
        // Health bars: one clamped builder for the render path and the HUD
        // overlay. A raw projected bar can land over the top telemetry strip
        // when its unit is behind the camera's top edge.
        health.extend(self.health_bars());
        // Build ghost: the real MDX of the selected building at the pointer,
        // half-transparent, tinted green when the footprint is free and red
        // when it is not (WC3 placement preview), plus the footprint ring.
        if let Some(ghost) = self.ghost()
            && let Some(def) = wc3_sim::data::def_of(ghost.def_id)
        {
            let def_id = ghost.def_id;
            let at = ghost.at;
            let model_index = self.assets.model_index_for_def(def_id);
            let model = self.assets.model_for_def(def_id);
            let ground = self.assets.height_at(at.x, at.y);
            let world = Mat4::from_scale_rotation_translation(
                Vec3::splat(model.scale.max(0.01)),
                Quat::from_rotation_y(model.yaw_offset),
                Vec3::new(at.x, ground - model.pivot, at.y),
            );
            let valid = ghost.legal;
            // WC3 ghost look: 50% alpha over the real model, green/red tint.
            let ghost_alpha = 0.5;
            let mesh_alpha = self
                .assets
                .mesh_alpha(model_index, None, 0)
                .into_iter()
                .map(|alpha| alpha * ghost_alpha)
                .collect();
            let layer_alpha = self
                .assets
                .mesh_layer_alpha(model_index, None, 0)
                .into_iter()
                .map(|layers| layers.into_iter().map(|alpha| alpha * ghost_alpha).collect())
                .collect();
            units.push(UnitDraw {
                model: model_index,
                world,
                tint: if valid {
                    [0.45, 1.0, 0.5, 1.0]
                } else {
                    [1.0, 0.35, 0.3, 1.0]
                },
                nodes: Vec::new(),
                mesh_alpha,
                layer_alpha,
            });
            rings.push(RingDraw {
                pos: Vec3::new(at.x, ground + 3.0, at.y),
                radius: def.footprint.max(1) as f32 * 128.0 * 0.5 + 24.0,
                color: if valid {
                    [0.3, 1.0, 0.4, 0.9]
                } else {
                    [1.0, 0.3, 0.25, 0.9]
                },
            });
        }
        // Rally point of a selected producer: the mustered point is a ghost
        // ring on the ground (blue inner ring + white core), exactly the
        // marker WC3 shows while the building is selected.
        for id in &self.selection {
            let Some(unit) = self.sim.unit(*id) else {
                continue;
            };
            let Some(rally) = unit.rally else { continue };
            if unit.owner != self.player || !unit.is_building() {
                continue;
            }
            let ground = self.assets.height_at(rally.x, rally.y);
            rings.push(RingDraw {
                pos: Vec3::new(rally.x, ground + 4.0, rally.y),
                radius: 54.0,
                color: [0.35, 0.75, 1.0, 0.95],
            });
            rings.push(RingDraw {
                pos: Vec3::new(rally.x, ground + 5.0, rally.y),
                radius: 24.0,
                color: [0.92, 0.96, 1.0, 0.85],
            });
        }
        // Corpse window: units that died a moment ago keep rendering their
        // Death sequence before they disappear.
        for death in &self.death_visuals {
            let model_index = self.assets.model_index_for_def(death.def_id);
            let model = self.assets.model_for_def(death.def_id);
            let ground = self.assets.height_at(death.pos.x, death.pos.y);
            let world = Mat4::from_scale_rotation_translation(
                Vec3::splat(model.scale.max(0.01)),
                Quat::from_rotation_y(-death.facing + model.yaw_offset),
                Vec3::new(death.pos.x, ground - model.pivot, death.pos.y),
            );
            let sequence = death.animator.sequence.clone();
            units.push(UnitDraw {
                model: model_index,
                world,
                tint: player_tint(death.owner),
                nodes: death.animator.world_matrices(),
                mesh_alpha: self.assets.mesh_alpha(
                    model_index,
                    sequence.as_ref(),
                    death.animator.frame_time(),
                ),
                layer_alpha: self.assets.mesh_layer_alpha(
                    model_index,
                    sequence.as_ref(),
                    death.animator.frame_time(),
                ),
            });
        }
        // Gold mines are sim entities, not units: draw each one with its own
        // MDX (`ngol`) so the map's mines are visible. They have no per-instance
        // animator, so the skinning palette is the model's bind pose — computed
        // once for the whole draw list, since every mine shares one model. An
        // empty palette would make the GPU skin path treat a skinned model as
        // static implicitly; the bind pose is the same pose, stated explicitly.
        let mine_model = self.assets.model_index_for_def("ngol");
        let mine_nodes: Vec<Mat4> = self
            .assets
            .models
            .get(mine_model)
            .and_then(|model| model.model.as_ref())
            .map(|mdx| wc3_anim::Animator::from_model(mdx).world_matrices())
            .unwrap_or_default();
        for mine in &self.sim.gold_mines {
            let model_index = mine_model;
            if model_index == 0 {
                continue;
            }
            let model = &self.assets.models[model_index];
            let ground = self.assets.height_at(mine.pos.x, mine.pos.y);
            let world = Mat4::from_scale_rotation_translation(
                Vec3::splat(model.scale.max(0.01)),
                Quat::from_rotation_y(model.yaw_offset),
                Vec3::new(mine.pos.x, ground - model.pivot, mine.pos.y),
            );
            units.push(UnitDraw {
                model: model_index,
                world,
                tint: [1.0, 1.0, 1.0, 1.0],
                nodes: mine_nodes.clone(),
                mesh_alpha: self.assets.mesh_alpha(model_index, None, 0),
                layer_alpha: self.assets.mesh_layer_alpha(model_index, None, 0),
            });
        }
        if let Some((marker, _)) = self.move_marker {
            rings.push(RingDraw {
                pos: Vec3::new(
                    marker.x,
                    self.assets.height_at(marker.x, marker.y) + 3.0,
                    marker.y,
                ),
                radius: 52.0,
                color: [1.0, 0.85, 0.25, 0.9],
            });
        }
        for mine in self.mines {
            rings.push(RingDraw {
                pos: Vec3::new(mine.x, self.assets.height_at(mine.x, mine.y) + 4.0, mine.y),
                radius: 46.0,
                color: [1.0, 0.78, 0.2, 0.85],
            });
        }

        let mut ui = UiBatch::default();
        let overlay = self.build_hud(&mut ui);
        Scene {
            view_proj,
            eye: self.camera.eye(),
            viewport: self.viewport,
            units,
            rings,
            health,
            ui,
            overlay,
        }
    }

    fn build_hud(&self, ui: &mut UiBatch) -> Option<(String, String)> {
        if self.screen != Screen::Playing {
            let view = self.menu_view();
            let _ = self.mobile_hud.draw_menu(&view, ui, &self.font, &self.assets.chrome);
            return None;
        }
        let s = self.layout.scale;
        let white = [1.0, 1.0, 1.0, 1.0];
        let gold = [0.95, 0.76, 0.30, 1.0];
        let green = [0.32, 0.86, 0.52, 1.0];
        let blue = [0.34, 0.68, 1.0, 1.0];
        let red = [1.0, 0.40, 0.38, 1.0];

        let gold_text = self.sim.resources[self.player as usize].max(0).to_string();
        let lumber_text = self.sim.lumber[self.player as usize].max(0).to_string();
        let (supply_used, supply_cap) = self.supply();
        let supply_text = format!("{supply_used}/{supply_cap}");
        // Pack resource icons win when the redraw override is present.
        let resource_icon = |redraw: &str, retail: &str| {
            self.assets
                .ui
                .texture(redraw)
                .or_else(|| self.assets.ui.texture(retail))
        };
        let resources = [
            ResourcePill::new(&gold_text, "GOLD", gold)
                .icon(resource_icon("HudRedrawGold", "ResourceGold")),
            ResourcePill::new(&lumber_text, "LUM", green)
                .icon(resource_icon("HudRedrawLumber", "ResourceLumber")),
            ResourcePill::new(&supply_text, "SUP", blue)
                .icon(resource_icon("HudRedrawFood", "ResourceSupply")),
        ];

        let command = |index: usize| -> CommandButton<'_> {
            let Some(button) = self.buttons.get(index) else {
                return CommandButton::empty();
            };
            if button.label.is_empty() {
                return CommandButton::empty();
            }
            let pressed = self.touch.pressed_button() == Some(index);
            let short = if !button.short.is_empty() {
                button.short.as_str()
            } else {
                match button.label.as_str() {
                "ATTACK" | "ATK-MOVE" => "ATK",
                "MOVE" => "MOVE",
                "PATROL" => "PAT",
                "ARMY" => "ARMY",
                "GATHER" => "GATH",
                "FOOTMAN" => "FOOT",
                "ARCHER" => "ARCH",
                "PEASANT" => "PEAS",
                "BUILD" => "BLD",
                "ZOOM+" => "Z+",
                "ZOOM-" => "Z-",
                label => label,
                }
            };
            let hotkey = match button.action {
                // Data-driven slots carry their own hotkey from the profile text.
                UiAction::TrainDef(_) | UiAction::BuildDef(_) if !button.hotkey.is_empty() => {
                    button.hotkey.as_str()
                }
                action if !button.hotkey.is_empty() => button.hotkey.as_str(),
                action => hotkey_label(action),
            };
            let icon = self
                .assets
                .ui
                .command_icon(&button.action)
                .or_else(|| {
                    let def_id = match button.action {
                        UiAction::TrainDef(id) | UiAction::BuildDef(id) => Some(id),
                        _ => None,
                    }?;
                    let name = self.assets.icon_name_for_def(def_id)?;
                    self.assets.ui.texture(&name)
                })
                .or_else(|| {
                    // Item slot: the inventory cell draws the shipped item icon
                    // resolved by the item's own WC3 id.
                    let UiAction::UseItem(slot) = button.action else {
                        return None;
                    };
                    let item_id = self
                        .selection
                        .iter()
                        .filter_map(|id| self.sim.unit(*id))
                        .find(|unit| !unit.items.is_empty())
                        .and_then(|unit| unit.items.get(usize::from(slot)).copied())
                        .and_then(|id| self.sim.item(id))
                        .map(|item| item.def().id)?;
                    let name = self.assets.icon_name_for_def(item_id)?;
                    self.assets.ui.texture(&name)
                })
                .or_else(|| {
                    matches!(
                        button.action,
                        UiAction::BuildDef(_) | UiAction::TrainDef(_) | UiAction::Build(_)
                    )
                    .then(|| self.assets.ui.texture("BTNHumanBuild"))
                    .flatten()
                });
            CommandButton::new(&button.label, icon)
                .short(short)
                .hotkey(hotkey)
                .sub(&button.sub)
                .enabled(button.enabled)
                .pressed(pressed)
        };
        // 4x3 console card: 8 used slots, 4 empty.
        let card_indices = self.card_button_indices();
        let commands: Vec<CommandButton<'_>> = (0..12)
            .map(|slot| {
                card_indices
                    .get(slot)
                    .map(|index| command(*index))
                    .unwrap_or_else(CommandButton::empty)
            })
            .collect();

        let selected_unit = self.selection.first().and_then(|id| self.sim.unit(*id));
        // Typed object data resolved by the unit's own WC3 id: an Orc peon is
        // `opeo` (Раб, orc, worker), never one of the four starter kinds.
        let typed_info = selected_unit.and_then(|unit| self.assets.info_for_def(unit.def().id));
        let selected_name = selected_unit
            .map(|unit| {
                self.assets
                    .name_for_def(unit.def().id)
                    .unwrap_or_else(|| self.assets.unit_name(unit.kind))
                    .to_uppercase()
            })
            .unwrap_or_default();
        let selected_class = selected_unit
            .map(|unit| {
                let role = if unit.is_building() {
                    "BUILDING"
                } else if unit.def().stats.is_worker {
                    "WORKER"
                } else {
                    "WARRIOR"
                };
                match typed_info.as_ref().and_then(|info| info.race.as_deref()) {
                    Some(race) => format!("{} · {}", race.to_uppercase(), role),
                    None => role.to_string(),
                }
            })
            .unwrap_or_else(|| "UNIT".to_string());
        let selected_kind = selected_unit
            .map(|unit| {
                if unit.is_building() {
                    "BUILDING"
                } else {
                    "UNIT"
                }
            })
            .unwrap_or("UNIT");
        let production_name = selected_unit
            .and_then(|unit| unit.production.first())
            .map(|item| {
                wc3_sim::data::def_at(item.def)
                    .and_then(|def| self.assets.name_for_def(def.id))
                    .unwrap_or_else(|| item.kind.name().to_string())
            })
            .unwrap_or_default();
        let production_view = selected_unit
            .and_then(|unit| unit.production.first())
            .map(|item| {
                let progress =
                    1.0 - item.remaining as f32 / item.kind.stats().train_ticks.max(1) as f32;
                crate::hud::mobile_hud::ProductionView {
                    name: &production_name,
                    progress: progress.clamp(0.0, 1.0),
                    remaining: item.remaining,
                }
            });
        let selection = selected_unit.map(|unit| {
            // Command-card icon from the typed data (`Art` field), falling back
            // to the compiled-in FDF icon name when the install has no object DB.
            let portrait = self
                .assets
                .ui
                .type_icon(unit.def().id)
                .or_else(|| {
                    self.assets
                        .icon_name_for_def(unit.def().id)
                        .and_then(|name| self.assets.ui.texture(&name))
                })
                .or_else(|| {
                    self.assets
                        .unit_icon_name(unit.kind)
                        .and_then(|name| self.assets.ui.texture(&name))
                })
                .or_else(|| self.assets.ui.unit_icon(unit.kind));
            // Armour/damage from the shipped tables when present (the sim state
            // already derives from them; this keeps map overrides live).
            let armor = typed_info
                .as_ref()
                .and_then(|info| info.stats.armor)
                .unwrap_or(unit.armor);
            let damage = typed_info
                .as_ref()
                .and_then(|info| info.stats.average_damage)
                .map(|damage| damage.round() as i32)
                .unwrap_or(unit.damage);
            let frame = if unit.owner != self.player {
                PortraitFrame::Enemy
            } else if unit.def().hero {
                PortraitFrame::Hero
            } else {
                PortraitFrame::Normal
            };
            SelectionView::new(&selected_name, unit.hp.max(0), unit.max_hp)
                .class(&selected_class)
                .kind(selected_kind)
                .portrait(portrait)
                .frame(frame)
                .stats(armor, damage)
                .range({
                    let range = typed_info
                        .as_ref()
                        .and_then(|info| info.stats.range)
                        .unwrap_or(unit.range);
                    range
                })
                .production(production_view)
                .mana(
                    unit.mana.max(0),
                    unit.max_mana.max(0),
                )
                .level(unit.level.max(1) as u32)
        }).or_else(|| {
            // A selected gold mine is a resource plate: WC3 shows its name and
            // the remaining gold, with no HP/mana bar (see `SelectionView.bars`).
            let index = self.selected_mine?;
            let mine = self.sim.gold_mines.get(index)?;
            let icon = self
                .assets
                .hud_skin
                .as_ref()
                .and_then(|skin| skin.pack.as_ref())
                .and_then(|pack| pack.texture("icon_gold_64"))
                .or_else(|| self.assets.ui.texture("BTNGoldMine"))
                .or_else(|| self.assets.ui.texture("ResourceGold"));
            Some(
                SelectionView::new("GOLD MINE", mine.remaining.max(0), mine.remaining.max(1))
                    .class("RESOURCE")
                    .kind("GOLD")
                    .portrait(icon)
                    .resource(mine.remaining),
            )
        });

        let world = self.assets.world_size.max(Vec2::splat(1.0));
        // Minimap model from the data-driven UI layer: own units always,
        // others only when the sim says they are visible; neutrals flagged.
        let dots: Vec<MinimapDot> = hud::minimap_blips(self)
            .into_iter()
            .map(|blip| MinimapDot::new(blip.pos.x, blip.pos.y, blip.size, blip.color))
            .collect();
        let aspect = self.viewport.x / self.viewport.y.max(1.0);
        let half_h = self.camera.distance * 22.5f32.to_radians().tan();
        let half_w = half_h * aspect;
        let camera = [
            ((self.camera.target.x - half_w) / world.x).clamp(0.0, 1.0),
            ((self.camera.target.y - half_h) / world.y).clamp(0.0, 1.0),
            ((2.0 * half_w) / world.x).clamp(0.0, 1.0),
            ((2.0 * half_h) / world.y).clamp(0.0, 1.0),
        ];
        let fog_cells = self.minimap_fog_cells(32, 32);
        let minimap = MinimapView::new(&self.assets.map_name).fog(&fog_cells, 32, 32);
        let minimap = MinimapView {
            dots: &dots,
            camera: Some(camera),
            ..minimap
        };

        let (phase, overlay) = if self.phase == Phase::Playing {
            (None, None)
        } else {
            let (title, subtitle) = match self.phase {
                Phase::Won => (
                    "VICTORY",
                    "THE ENEMY TOWN HALL IS DESTROYED. TAP TO PLAY AGAIN.",
                ),
                Phase::Lost => ("DEFEAT", "YOUR TOWN HALL HAS FALLEN. TAP TO PLAY AGAIN."),
                Phase::Playing => ("", ""),
            };
            (
                Some(PhaseView {
                    title,
                    subtitle,
                    won: Some(matches!(self.phase, Phase::Won)),
                }),
                Some((title.to_string(), subtitle.to_string())),
            )
        };
        let toast = self
            .toast
            .as_ref()
            .map(|(text, _)| ToastView::new(text, ToastTone::Info));
        let hint_text = if let Some(def_id) = self.placing {
            let name = wc3_sim::def_of(def_id)
                .map(|def| def.name)
                .unwrap_or(def_id);
            Some(format!("TAP GROUND TO PLACE {name}"))
        } else if self.attack_move {
            Some("TAP A DESTINATION TO ATTACK-MOVE".to_string())
        } else if self.patrol_order {
            Some("TAP A DESTINATION TO PATROL".to_string())
        } else {
            // The top strip always carries state: upkeep tier, supply, both
            // resources and an alert when a friendly unit is under attack.
            let (used, cap) = self.supply();
            Some(format!(
                "{}  ·  SUPPLY {used}/{cap}  ·  {}g {}l",
                self.upkeep_label().to_uppercase(),
                self.sim.resources[self.player as usize].max(0),
                self.sim.lumber[self.player as usize].max(0),
            ))
        };
        let hint = hint_text.as_deref().map(|text| HintView::new(text, true));
        let mode = if self.placing.is_some() {
            "PLACE"
        } else if self.attack_move {
            "ATTACK"
        } else {
            "MOVE"
        };

        let pressed = self.touch.pressed_button();
        let pressed_tooltip = pressed.and_then(|index| {
            if index >= ARMY_TARGET_BASE {
                return None;
            }
            if index >= GROUP_TARGET_BASE {
                let slot = (index - GROUP_TARGET_BASE) as u8 + 1;
                return Some(format!(
                    "GROUP {}  ·  {} UNITS  ·  HOLD SAVE  ·  2x CENTRE",
                    slot,
                    self.control_groups.len(slot)
                ));
            }
            self.buttons.get(index).map(|button| self.card_tooltip(button))
        });
        // Section 3 of the tooltip (cost/requirements) of the hovered command.
        let tooltip_costs = pressed
            .filter(|index| *index < GROUP_TARGET_BASE)
            .and_then(|index| self.buttons.get(index))
            .map(|button| self.card_cost_lines(button))
            .unwrap_or_default();
        // Only a hovered/pressed command gets a floating tooltip. The selected
        // unit's name/stats already live in the console card; rendering them as
        // a floating panel covered half the screen (measured: 473 px wide and
        // the whole band tall on a 2414x1080 device).
        let tooltip = pressed_tooltip.or_else(|| self.tooltip_text.clone());
        // WC3 tooltips float above the button they describe: anchor the box to
        // the pressed/hovered card slot, the group row or the army icon.
        let tooltip_anchor = self.tooltip_anchor_rect();
        let toasts: Vec<ToastView> = self
            .toast_queue
            .iter()
            .map(|(text, _)| ToastView::new(text, ToastTone::Info))
            .collect();
        // Army chips for the atlas-skin console row: resolved icons + live HP,
        // exactly the units the fallback block draws (first is the active
        // subgroup, so it takes the gold `slot_gold` frame).
        let army: Vec<ArmyChip> = self
            .selection
            .iter()
            .take(8)
            .map(|id| {
                let Some(unit) = self.sim.unit(*id) else {
                    return ArmyChip::default();
                };
                let icon = self
                    .assets
                    .ui
                    .type_icon(unit.def().id)
                    .or_else(|| {
                        self.assets
                            .icon_name_for_def(unit.def().id)
                            .and_then(|name| self.assets.ui.texture(&name))
                    })
                    .or_else(|| {
                        self.assets
                            .unit_icon_name(unit.kind)
                            .and_then(|name| self.assets.ui.texture(&name))
                    })
                    .or_else(|| self.assets.ui.unit_icon(unit.kind));
                ArmyChip {
                    icon,
                    hp: (unit.hp as f32 / unit.max_hp.max(1) as f32).clamp(0.0, 1.0),
                    selected: false,
                }
            })
            .collect();
        // The ten control-group plates take the game's own hit rects, so the
        // skin art can never drift from the touch row.
        let group_rects = self.group_row_slots();
        let active_group = self.group_press.map(|(slot, _)| slot);
        let groups: Vec<GroupView> = group_rects
            .iter()
            .enumerate()
            .map(|(index, rect)| {
                let slot = if index < 9 { index as u8 + 1 } else { 0 };
                GroupView {
                    rect: *rect,
                    count: self.control_groups.len(slot) as u32,
                    active: active_group == Some(slot),
                }
            })
            .collect();
        let mut army = army;
        if let Some(first) = army.first_mut() {
            first.selected = true;
        }
        let view = HudView::new(self.viewport)
            .safe(SafeArea::new(
                self.safe_insets.left,
                self.safe_insets.top,
                self.safe_insets.right,
                self.safe_insets.bottom,
            ))
            .resources(&resources)
            .selection(selection)
            .minimap(minimap)
            .commands(&commands)
            .phase(phase)
            .toast(toast)
            .hint(hint)
            .map_name(&self.assets.map_name, self.fps)
            .mode(mode)
            .console_tiles([
                self.assets.ui.texture("HumanUITile01"),
                self.assets.ui.texture("HumanUITile02"),
                self.assets.ui.texture("HumanUITile03"),
                self.assets.ui.texture("HumanUITile04"),
            ])
            .button_border(self.assets.ui.texture("CommandButtonBorder"))
            .portrait_frame(self.assets.ui.texture("PortraitFrame"))
            .redraw(self.hud_redraw_skin())
            .tooltip_art(
                self.assets.ui.texture("TooltipBackground"),
                self.assets.ui.texture("TooltipBorder"),
            )
            .dialog_art(
                self.assets.ui.texture("EscMenuBackground"),
                self.assets.ui.texture("EscMenuBorder"),
                self.assets.ui.texture("EscMenuTitleBand"),
            )
            .tooltip(tooltip)
            .tooltip_costs(tooltip_costs)
            .tooltip_anchor(tooltip_anchor)
            .toasts(toasts)
            .upkeep(self.upkeep_label())
            .army(&army)
            .groups(&groups)
            .hud_skin(self.assets.hud_skin());
        let _ = self
            .mobile_hud
            .draw(&view, ui, &self.font, &self.assets.chrome);
        // Quest log / hero skills / building panel: the same shipped dialog
        // chrome as the menus, drawn above the console and below the input
        // feedback. Content is live scenario data, never a mock string.
        if let Some(screen) = self.dialog {
            let quest = self.quest_log_lines();
            let lines: Vec<&str> = quest.iter().map(String::as_str).collect();
            let view = crate::hud::mobile_hud::DialogView::for_screen(screen, self.viewport)
                .with_content("QUEST LOG", &lines, "TAP · CLOSE")
                .hud_skin(self.assets.hud_skin(), self.assets.hud_skin_dialog());
            let _ = self
                .mobile_hud
                .draw_dialog(&view, ui, &self.font, &self.assets.chrome);
        }
        // Input feedback drawn after the console so it is always visible while
        // the finger is down: the group row and the box marquee. With the atlas
        // skin the ten group plates are region 8 inside `mobile_hud::draw`, so
        // only the fallback chrome is drawn here.
        if self.hud_skin().is_none() {
            self.draw_group_row(ui);
        }
        self.draw_box_marquee(ui);

        // Army panel fallback: one icon per selected unit with its own HP bar,
        // inside the console under the selection text. With the atlas skin the
        // chips are region 8 (`army::draw_chip`) inside `mobile_hud::draw`.
        if self.hud_skin().is_none() {
            let s = self.layout.scale;
            let slots: Vec<Rect> = self
                .mobile_layout
                .as_ref()
                .map(|layout| layout.army_slots.clone())
                .unwrap_or_default();
            for (slot_index, id) in self.selection.iter().take(slots.len()).enumerate() {
                let Some(unit) = self.sim.unit(*id) else {
                    continue;
                };
                let rect = slots[slot_index];
                let icon = self
                    .assets
                    .ui
                    .type_icon(unit.def().id)
                    .or_else(|| {
                        self.assets
                            .icon_name_for_def(unit.def().id)
                            .and_then(|name| self.assets.ui.texture(&name))
                    })
                    .or_else(|| {
                        self.assets
                            .unit_icon_name(unit.kind)
                            .and_then(|name| self.assets.ui.texture(&name))
                    })
                    .or_else(|| self.assets.ui.unit_icon(unit.kind));
                match icon {
                    Some(texture) => {
                        // The icon never leaves its slot: the picture is the
                        // slot minus the 3 px HP strip at the bottom.
                        let picture = Rect::new(rect.x, rect.y, rect.w, (rect.h - 4.0 * s).max(1.0));
                        ui.textured_quad(texture, picture, [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
                    }
                    None => ui.rect(rect, [0.18, 0.22, 0.3, 0.9]),
                }
                ui.border(
                    rect,
                    1.5 * s,
                    if self.selection.contains(id) {
                        [0.35, 1.0, 0.45, 0.9]
                    } else {
                        [0.5, 0.55, 0.6, 0.9]
                    },
                );
                let ratio = (unit.hp as f32 / unit.max_hp.max(1) as f32).clamp(0.0, 1.0);
                let bar = Rect::new(
                    rect.x + 1.0 * s,
                    rect.bottom() - 4.0 * s,
                    (rect.w - 2.0 * s).max(1.0),
                    3.0 * s,
                );
                ui.rect(bar, [0.1, 0.1, 0.1, 0.9]);
                ui.rect(
                    Rect::new(bar.x, bar.y, bar.w * ratio, bar.h),
                    if ratio > 0.5 {
                        [0.2, 0.95, 0.3, 0.95]
                    } else if ratio > 0.25 {
                        [0.95, 0.8, 0.25, 0.95]
                    } else {
                        [0.95, 0.3, 0.25, 0.95]
                    },
                );
            }
        }

        // Bounty / kill feedback: world-anchored "+N G" labels rise and fade.
        // The rect is measured from the fitted text (clip policy), so a label
        // can never bleed past its own box.
        for label in &self.floating {
            let ground = self.assets.height_at(label.pos.x, label.pos.y);
            let world = Vec3::new(label.pos.x, ground + 120.0 + label.lift(), label.pos.y);
            let Some(screen) = self.camera.world_to_screen(world) else {
                continue;
            };
            let scale = 0.95 * s;
            let text_w = self.font.measure(&label.text, scale);
            let chip = Rect::new(
                screen.x - text_w * 0.5 - 4.0 * s,
                screen.y - 14.0 * s,
                text_w + 8.0 * s,
                self.font.glyph_box(scale) + 6.0 * s,
            );
            let alpha = label.alpha();
            if alpha <= 0.02 {
                continue;
            }
            let mut color = label.color;
            color[3] *= alpha;
            let shadow = [0.02, 0.02, 0.02, 0.85 * alpha];
            self.font.draw_fit(
                ui,
                chip,
                &label.text,
                scale,
                crate::hud::MIN_DENSE_PX,
                color,
                TextAlign::Centre,
                Some(shadow),
            );
        }

        // Health bars remain world-space overlays above the models.
        for bar in &self.health_bars() {
            let width = bar.width;
            let height = 7.0 * s;
            let origin = Vec2::new(bar.screen.x - width * 0.5, bar.screen.y);
            ui.rect(
                Rect::new(origin.x, origin.y, width, height),
                [0.02, 0.02, 0.02, 0.85],
            );
            let color = if bar.ratio > 0.55 {
                green
            } else if bar.ratio > 0.25 {
                gold
            } else {
                red
            };
            ui.rect(
                Rect::new(
                    origin.x + 1.0,
                    origin.y + 1.0,
                    (width - 2.0) * bar.ratio,
                    height - 2.0,
                ),
                color,
            );
        }
        let _ = white;
        overlay
    }

    fn health_bars(&self) -> Vec<HealthBar> {
        // The bars are screen-space quads drawn over the world. They belong to
        // the playfield rectangle between the top telemetry strip and the
        // console: a unit behind the camera's top edge used to paint its bar
        // over the 57.6 px strip (device frame 2026-11-16, green bar at y≈8).
        let scale = self.layout.scale;
        let (play_top, play_bottom) = self
            .mobile_layout
            .as_ref()
            .map(|layout| (layout.top_meta.bottom(), layout.console.y))
            .unwrap_or((0.0, self.viewport.y));
        let minimap = self.mobile_layout.as_ref().map(|layout| layout.minimap);
        let bar_height = 7.0 * scale.max(0.1);
        let mut bars = Vec::new();
        for unit in self.sim.alive_units() {
            if unit.owner != self.player && !self.sim.is_visible(self.player, unit.pos) {
                continue;
            }
            let selected = self.selection.contains(&unit.id);
            if !selected && unit.hp >= unit.max_hp {
                continue;
            }
            let ground = self.assets.height_at(unit.pos.x, unit.pos.y);
            let y = ground + self.assets.model_for_def(unit.def().id).height * 0.95;
            if let Some(screen) = self
                .camera
                .world_to_screen(Vec3::new(unit.pos.x, y, unit.pos.y))
            {
                let width = if unit.is_building() { 110.0 } else { 48.0 } * scale;
                let quad = Rect::new(screen.x - width * 0.5, screen.y, width, bar_height);
                // Never paint over the strip/console or the minimap ring; a
                // half-off-screen bar may clip at the viewport edge.
                if quad.y < play_top || quad.bottom() > play_bottom {
                    continue;
                }
                if let Some(minimap) = minimap
                    && quad.intersects(&minimap)
                {
                    continue;
                }
                if quad.right() < 0.0 || quad.x > self.viewport.x {
                    continue;
                }
                bars.push(HealthBar {
                    screen,
                    ratio: (unit.hp as f32 / unit.max_hp.max(1) as f32).clamp(0.0, 1.0),
                    width,
                    unit: unit.id,
                });
            }
        }
        bars
    }
}

fn build_grid(assets: &Assets, width: u32, height: u32) -> Grid {
    let Some(pathing) = &assets.pathing else {
        return Grid::new(width, height, TERRAIN_TILE_SIZE);
    };
    let mut walkable = vec![true; (width * height) as usize];
    let sx = pathing.width as f32 / width as f32;
    let sy = pathing.height as f32 / height as f32;
    for y in 0..height {
        for x in 0..width {
            let px = ((x as f32 + 0.5) * sx) as u32;
            let py = ((y as f32 + 0.5) * sy) as u32;
            walkable[(y * width + x) as usize] =
                pathing.walkable(px.min(pathing.width - 1), py.min(pathing.height - 1));
        }
    }
    // Deep water is not walkable for ground units even when the WPM bit says
    // otherwise (`Assets::is_water` reads the terrain water layer).
    crate::sim_bridge::mask_water(width, height, TERRAIN_TILE_SIZE, &mut walkable, |x, z| {
        assets.is_water(x, z)
    });
    Grid::from_walkable(width, height, TERRAIN_TILE_SIZE, walkable)
}

/// WC3 command-card hotkey labels for the visible card actions.
///
/// One source of truth for both the letter drawn on the button and the letter
/// the keyboard handler matches, so the card and the keys can never drift.
/// Letters follow WC3: Attack A, Move M, Stop S, Hold H, Patrol P, worker
/// Gather G, back-to-base B. `ARMY` (select all) has no WC3 card equivalent
/// and is bound to E. Roster entries (`TrainDef`/`BuildDef`) carry their own
/// letters from the object data and are not listed here.
pub fn hotkey_label(action: UiAction) -> &'static str {
    match action {
        UiAction::AttackMove => "A",
        UiAction::Move => "M",
        UiAction::Stop => "S",
        UiAction::Hold => "H",
        UiAction::Patrol => "P",
        UiAction::Gather => "G",
        UiAction::SelectArmy => "E",
        UiAction::CentreBase => "B",
        UiAction::Train(kind) => match kind {
            UnitKind::Footman => "F",
            UnitKind::Archer => "R",
            UnitKind::Peasant => "P",
            _ => "",
        },
        UiAction::Build(_) => "B",
        UiAction::ZoomIn => "+",
        UiAction::ZoomOut => "-",
        UiAction::Restart => "R",
        UiAction::TrainDef(_) | UiAction::BuildDef(_) | UiAction::Cast(_) | UiAction::UseItem(_) => {
            ""
        }
        UiAction::QuestLog => "",
        UiAction::BuildPageNext => "",
        UiAction::Research(_)
        | UiAction::CancelConstruction
        | UiAction::PickupNearestItem
        | UiAction::BuyItem(_)
        | UiAction::BuyUnit(_) => "",
    }
}

/// Lower-case hotkey letter resolved against the live card.
pub fn hotkey_letter(action: UiAction) -> Option<char> {
    hotkey_label(action)
        .chars()
        .next()
        .map(|letter| letter.to_ascii_lowercase())
}/// HUD rect -> `wc3-input` rect for the touch router.
fn touch_rect(rect: Rect) -> wc3_input::Rect {
    wc3_input::Rect::new(rect.x, rect.y, rect.w, rect.h)
}

/// Presentation-only: keep a unit in its cell while it stays selected, so the
/// panel does not reshuffle on every death (ported from WargusHD's
/// `KeepSelectionCells`).
fn keep_selection_cells(cells: &mut Vec<Option<UnitId>>, selected: &[UnitId]) {
    if cells.is_empty() {
        *cells = selected.iter().copied().map(Some).collect();
        return;
    }
    let mut shared = false;
    for cell in cells.iter_mut() {
        match *cell {
            None => {}
            Some(id) if selected.contains(&id) => shared = true,
            Some(_) => *cell = None,
        }
    }
    if !shared {
        *cells = selected.iter().copied().map(Some).collect();
        return;
    }
    for id in selected {
        if cells.iter().any(|cell| *cell == Some(*id)) {
            continue;
        }
        if let Some(free) = cells.iter_mut().find(|cell| cell.is_none()) {
            *free = Some(*id);
        } else {
            cells.push(Some(*id));
        }
    }
}

/// Flatten a WC3 tooltip for a one-line toast: drop colour codes (`|c…`, `|r`),
/// turn `|n` into spaces and cut on a character boundary.
fn game_tooltip_text(text: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(position) = rest.find('|') {
        out.push_str(&rest[..position]);
        let tail = &rest[position..];
        let skip = if tail.starts_with("|c") {
            tail.char_indices()
                .nth(10)
                .map(|(index, _)| index)
                .unwrap_or(tail.len())
        } else if tail.starts_with("|n") || tail.starts_with("|r") {
            if tail.starts_with("|n") {
                out.push(' ');
            }
            2
        } else {
            1
        };
        rest = &tail[skip..];
    }
    out.push_str(rest);
    let trimmed = out.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut clipped: String = trimmed.chars().take(max_chars).collect();
    clipped.push('…');
    clipped
}

#[cfg(test)]
mod tooltip_tests {
    use super::game_tooltip_text;

    #[test]
    fn strips_wc3_markup_and_clips() {
        let text = "|cffffcc00Пехотинец|r — рядовой|nсолдат армии людей.";
        let flat = game_tooltip_text(text, 120);
        assert!(!flat.contains("|c"), "colour code removed: {flat}");
        assert!(!flat.contains("|r"), "reset removed: {flat}");
        assert!(!flat.contains("|n"), "line break flattened: {flat}");
        assert!(flat.contains("Пехотинец"), "{flat}");
        let clipped = game_tooltip_text("abcdefghij", 4);
        assert_eq!(clipped.chars().count(), 5, "4 chars + ellipsis: {clipped}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Game {
        Game::new(Assets::load(None, None), 0x5eed)
    }

    #[test]
    fn scenario_spawns_bases_army_and_mines() {
        let game = game();
        assert_eq!(game.phase, Phase::Playing);
        assert!(game.player_hall().is_some(), "player town hall");
        assert!(
            game.army_count() >= 3,
            "starting army: {}",
            game.army_count()
        );
        assert!(
            game.enemy_count() >= 2,
            "enemy squad: {}",
            game.enemy_count()
        );
        assert!(game.mines[0].distance(game.base_pos) > 0.0);
    }

    #[test]
    fn training_spends_gold_and_finishes() {
        let mut game = game();
        let hall = game.player_hall().unwrap();
        game.selection = vec![hall];
        for unit in game.sim.units.iter_mut() {
            if unit.owner == game.enemy && !unit.is_building() {
                unit.alive = false;
            }
        }
        game.sim.disable_ai(game.enemy);
        game.activate_action(UiAction::Train(UnitKind::Footman));
        let before = game.army_count();
        for _ in 0..900 {
            game.update(1.0 / 30.0);
        }
        assert!(
            game.army_count() > before,
            "trained unit appears: {before} -> {}",
            game.army_count()
        );
        assert!(game.total_trained >= 1);
    }

    #[test]
    fn destroying_enemy_base_wins() {
        // WC3 base rule: a dead hall is not enough; every building and every
        // worker of the seat has to be gone.
        let mut game = game();
        let doomed: Vec<UnitId> = game
            .sim
            .units
            .iter()
            .filter(|unit| unit.alive && unit.owner == game.enemy)
            .map(|unit| unit.id)
            .collect();
        for id in doomed {
            game.sim.kill_unit(id);
        }
        game.drain_sim_events();
        assert_eq!(game.phase, Phase::Won);
        assert_eq!(
            game.match_outcome().map(|outcome| outcome.winner),
            Some(game.player)
        );
    }

    #[test]
    fn losing_own_base_defeats() {
        let mut game = game();
        let doomed: Vec<UnitId> = game
            .sim
            .units
            .iter()
            .filter(|unit| unit.alive && unit.owner == game.player)
            .map(|unit| unit.id)
            .collect();
        for id in doomed {
            game.sim.kill_unit(id);
        }
        game.drain_sim_events();
        assert_eq!(game.phase, Phase::Lost);
        assert_eq!(
            game.match_outcome().map(|outcome| outcome.winner),
            Some(game.enemy)
        );
    }

    #[test]
    fn selection_cells_keep_positions() {
        let mut cells = vec![Some(UnitId(1)), Some(UnitId(2)), Some(UnitId(3))];
        keep_selection_cells(&mut cells, &[UnitId(1), UnitId(3), UnitId(4)]);
        // The new unit takes the lowest free cell; existing cells never move.
        assert_eq!(
            cells,
            vec![Some(UnitId(1)), Some(UnitId(4)), Some(UnitId(3))]
        );
    }

    #[test]
    fn ai_attacks_after_the_opening_wave() {
        let mut game = game();
        for _ in 0..200 {
            game.update(1.0 / 30.0);
        }
        let engaged = game
            .sim
            .alive_units()
            .filter(|unit| unit.owner == game.enemy)
            .any(|unit| {
                matches!(
                    unit.order,
                    wc3_sim::Order::Move(_)
                        | wc3_sim::Order::MoveAttack(_)
                        | wc3_sim::Order::Attack(_)
                )
            });
        assert!(engaged, "enemy squad should have orders");
    }

    // ------------------------------------------------------------ touch layer

    fn touch_game() -> Game {
        let mut game = game();
        game.resize(1280.0, 720.0);
        game
    }

    #[test]
    fn safe_insets_keep_the_hud_inside_the_system_bars() {
        // Contract (operator directive 2026-11-16): the console is pinned to the
        // physical bottom and the top telemetry strip is flush at y = 0, so the
        // *vertical* insets are intentionally ignored. The *horizontal* insets
        // are kept: the console and its zones never slide under a cutout or a
        // side gesture gutter. This test asserts exactly that split.
        let mut game = touch_game();
        assert!(!game.set_safe_insets(0.0, 0.0, 0.0, 0.0));
        assert!(game.set_safe_insets(0.0, 96.0, 0.0, 48.0));
        let height = game.viewport.y;
        let strip = game
            .mobile_layout
            .as_ref()
            .expect("mobile layout")
            .top_meta;
        assert_eq!(strip.y, 0.0, "top strip is flush at the physical top: {strip:?}");
        assert!(
            strip.bottom() < 96.0,
            "the vertical inset (96) is not applied to the strip: {strip:?}"
        );
        let console = game.layout.top_bar;
        assert!(
            (console.bottom() - height).abs() < 0.5,
            "console is pinned to the physical bottom ({height}), not above the 48 px inset: {console:?}"
        );
        assert!(
            game.layout.resource_row.y < 96.0 + 1e-3,
            "the resource row lives in the strip, above the ignored top inset: {:?}",
            game.layout.resource_row
        );
        assert!(
            !game.set_safe_insets(0.0, 96.0, 0.0, 48.0),
            "unchanged is a no-op"
        );

        // Horizontal insets stay authoritative for the content; the console
        // background itself is edge to edge (TZ §4.3), so the insets land on
        // the content, the resources and the minimap — never on the backdrop.
        assert!(game.set_safe_insets(80.0, 96.0, 80.0, 48.0));
        assert!(game.layout.top_bar.x.abs() < 1e-3, "console background is edge to edge");
        assert!(
            (game.layout.top_bar.right() - 1280.0).abs() < 1e-3,
            "console background is edge to edge: {:?}",
            game.layout.top_bar
        );
        let mobile = game.mobile_layout.as_ref().expect("mobile layout");
        assert!(
            (mobile.dp().safe.left - 40.0).abs() < 1e-3
                && (mobile.dp().safe.right - 40.0).abs() < 1e-3,
            "80 px insets are 40 dp at density 2.0: {:?}",
            mobile.dp().safe
        );
        assert!(
            (mobile.dp().content.x - 48.0).abs() < 1e-3,
            "content starts after the left inset + pad: {:?}",
            mobile.dp().content
        );
        assert!(
            mobile.dp().content.right() <= 640.0 - 40.0 - 8.0 + 1e-3,
            "content ends before the right inset + pad: {:?}",
            mobile.dp().content
        );
        assert!(game.layout.minimap.x >= 96.0 - 1e-3);
        assert!(
            game.layout.resource_row.right() <= 1280.0 - 80.0 + 1e-3,
            "resource row inside the right inset: {:?}",
            game.layout.resource_row
        );

        // Insets survive a resize (rotation / window size change): vertical
        // insets stay ignored, horizontal ones still apply, the stored values
        // are unchanged.
        game.resize(1600.0, 900.0);
        assert_eq!(game.safe_insets.top, 96.0);
        assert_eq!(game.safe_insets.bottom, 48.0);
        let strip = game
            .mobile_layout
            .as_ref()
            .expect("mobile layout")
            .top_meta;
        assert_eq!(strip.y, 0.0, "strip stays flush after resize: {strip:?}");
        assert!(
            (game.layout.top_bar.bottom() - 900.0).abs() < 0.5,
            "console stays pinned to the new physical bottom: {:?}",
            game.layout.top_bar
        );
        assert!(
            game.layout.top_bar.x.abs() < 1e-3
                && (game.layout.top_bar.right() - 1600.0).abs() < 1e-3,
            "console background stays edge to edge: {:?}",
            game.layout.top_bar
        );
        let mobile = game.mobile_layout.as_ref().expect("mobile layout");
        assert!(
            (mobile.dp().safe.left - 40.0).abs() < 1e-3
                && (mobile.dp().safe.right - 40.0).abs() < 1e-3,
            "horizontal insets survive the resize: {:?}",
            mobile.dp().safe
        );
    }

    #[test]
    fn routing_prefers_minimap_then_buttons_then_hud_blockers_then_world() {
        let mut game = touch_game();
        let before = game.camera.target;
        game.pointer_down(game.layout.minimap.centre());
        game.pointer_up(game.layout.minimap.centre());
        assert!(
            game.camera.target.distance(before) > 1.0,
            "minimap tap jumps the camera"
        );

        let army_index = game
            .buttons
            .iter()
            .position(|button| button.action == UiAction::SelectArmy)
            .expect("ARMY chip on the card");
        let army = game.buttons[army_index].clone();
        game.pointer_down(army.rect.centre());
        assert_eq!(game.touch.pressed_button(), Some(army_index));
        game.pointer_up(army.rect.centre());
        assert!(game.log.iter().any(|line| line == "BUTTON ARMY"));
        assert!(!game.selection.is_empty(), "ARMY chip selects the army");

        // The selection panel and the console dead space (between the widgets)
        // swallow the touch instead of passing it to the world.
        game.pointer_down(game.layout.selection.centre());
        assert!(!game.touch.is_active());
        // Console top strip right of the selection panel: inside the console
        // blocker, clear of the minimap, the selection and the group row.
        let dead = Vec2::new(
            game.layout.selection.right() + 8.0 * game.layout.scale,
            game.layout.top_bar.y + 4.0 * game.layout.scale,
        );
        game.pointer_down(dead);
        assert!(!game.touch.is_active());
        game.pointer_down(Vec2::new(
            game.layout.top_bar.x + 4.0,
            game.layout.top_bar.y + 4.0,
        ));
        assert!(!game.touch.is_active());

        // A tap on the world starts a tap gesture, not a HUD action.
        game.pointer_down(Vec2::new(640.0, 360.0));
        assert!(game.touch.is_active());
        assert_eq!(game.touch.pressed_button(), None);
    }

    #[test]
    fn pinch_zooms_and_two_finger_pan_moves_the_camera() {
        let mut game = touch_game();
        let distance = game.camera.distance;
        game.pinch_begin(200.0, Vec2::new(640.0, 360.0));
        game.pinch_update(400.0, Vec2::new(640.0, 360.0));
        assert!(
            game.camera.distance < distance,
            "spreading fingers zooms in: {distance} -> {}",
            game.camera.distance
        );

        let before = game.camera.target;
        game.pinch_update(400.0, Vec2::new(700.0, 360.0));
        assert!(
            game.camera.target.x > before.x,
            "two-finger pan moves the camera: {:?} -> {:?}",
            before,
            game.camera.target
        );

        game.pinch_end();
        let zoomed = game.camera.distance;
        game.update(1.0 / 30.0);
        assert!((game.camera.distance - zoomed).abs() < 1e-6);
    }

    #[test]
    fn quick_drag_pans_the_camera_instead_of_boxing() {
        let mut game = touch_game();
        let before = game.camera.target;
        game.pointer_down(Vec2::new(640.0, 300.0));
        game.pointer_move(Vec2::new(640.0, 220.0));
        assert!(
            game.camera.target.y < before.y,
            "drag up looks north: {:?} -> {:?}",
            before,
            game.camera.target
        );
        game.pointer_up(Vec2::new(640.0, 220.0));
        assert!(!game.touch.is_active());
    }

    #[test]
    fn drag_box_selects_the_starting_army_after_the_hold() {
        let mut game = touch_game();
        game.camera.centre_on(game.base_pos);
        let centre = game.viewport * 0.5;
        let a = centre - Vec2::new(260.0, 160.0);
        let b = centre + Vec2::new(260.0, 160.0);
        game.pointer_down(a);
        for _ in 0..12 {
            game.update(1.0 / 30.0);
        }
        game.pointer_move(b);
        assert!(game.touch.box_rect().is_some());
        game.pointer_up(b);
        assert!(
            game.selection.len() >= 2,
            "box selection picks the base army: {:?}",
            game.selection
        );
    }

    #[test]
    fn box_drag_at_the_edge_auto_scrolls_the_camera() {
        let mut game = touch_game();
        game.pointer_down(Vec2::new(640.0, 360.0));
        for _ in 0..12 {
            game.update(1.0 / 30.0);
        }
        game.pointer_move(Vec2::new(game.viewport.x - 4.0, 360.0));
        assert!(game.touch.box_rect().is_some());
        let before = game.camera.target;
        game.update(0.1);
        assert!(
            game.camera.target.x > before.x,
            "right-edge box drag scrolls east: {:?} -> {:?}",
            before,
            game.camera.target
        );
        game.pointer_up(Vec2::new(game.viewport.x - 4.0, 360.0));
    }

    #[test]
    fn second_finger_lift_resumes_panning_with_the_first() {
        let mut game = touch_game();
        let before = game.camera.target;
        game.pointer_resume_pan(Vec2::new(600.0, 300.0));
        game.pointer_move(Vec2::new(600.0, 240.0));
        assert!(
            game.camera.target.y < before.y,
            "remaining finger keeps panning"
        );
        game.pointer_up(Vec2::new(600.0, 240.0));
        assert!(!game.touch.is_active());
    }

    #[test]
    fn console_hud_rects_do_not_swallow_world_taps() {
        use crate::hud::mobile_hud::{
            CARD_W_DP, CONSOLE_MAX_DP, CONSOLE_MIN_DP, CONSOLE_PAD_DP, TOP_BAR_DP,
        };
        let mut game = game();
        game.resize(2414.0, 1080.0);
        game.set_safe_insets(210.0, 120.0, 90.0, 96.0);
        let mobile = game.mobile_layout.clone().expect("mobile layout");
        let console = mobile.console;
        let strip = mobile.top_meta;
        let card = mobile.card;
        let selection = mobile.selection;
        eprintln!(
            "HUD_RECTS console=({},{},{},{}) strip=({},{},{},{}) playfield=({},{},{},{}) card=({},{},{},{}) selection=({},{},{},{}) minimap=({},{},{},{})",
            console.x,
            console.y,
            console.w,
            console.h,
            strip.x,
            strip.y,
            strip.w,
            strip.h,
            strip.x,
            strip.bottom(),
            strip.w,
            (console.y - strip.bottom()).max(0.0),
            card.x,
            card.y,
            card.w,
            card.h,
            selection.x,
            selection.y,
            selection.w,
            selection.h,
            mobile.minimap.x,
            mobile.minimap.y,
            mobile.minimap.w,
            mobile.minimap.h
        );
        // Current dp engine (relayout): the console background is edge to edge
        // and pinned to the physical bottom; the safe insets move the content,
        // not the backdrop. The telemetry strip is 30 dp.
        assert_eq!(console.x, 0.0, "console backdrop is edge to edge");
        assert!(
            (console.right() - 2414.0).abs() < 0.5,
            "console backdrop is edge to edge: {console:?}"
        );
        assert!(
            (console.bottom() - 1080.0).abs() < 0.5,
            "console is pinned to the physical bottom (vertical insets are ignored by contract): {console:?}"
        );
        let dp = mobile.dp();
        // The band is the 0.34 rule clamped to [176, 220] dp plus the part of
        // the bottom inset that exceeds the console padding.
        let bottom_extra = (dp.safe.bottom - CONSOLE_PAD_DP).max(0.0);
        let console_dp =
            (0.34 * dp.screen.y).clamp(CONSOLE_MIN_DP, CONSOLE_MAX_DP) + bottom_extra;
        assert!(
            (dp.console.h - console_dp).abs() < 0.01,
            "console is the 0.34 band clamped to [{CONSOLE_MIN_DP}, {CONSOLE_MAX_DP}] dp \
             (+{bottom_extra:.1} bottom inset): {:?}",
            dp.console
        );
        assert!(
            (dp.top_bar.h - TOP_BAR_DP).abs() < 0.01,
            "telemetry strip is {TOP_BAR_DP} dp: {:?}",
            dp.top_bar
        );
        assert_eq!(strip.y, 0.0, "top strip is flush at y = 0: {strip:?}");
        assert!(
            (dp.content.x - (105.0 + CONSOLE_PAD_DP)).abs() < 0.01
                && (dp.content.right() - (dp.screen.x - 45.0 - CONSOLE_PAD_DP)).abs() < 0.01,
            "safe insets land on the content (210/90 px at 2 px/dp): {:?}",
            dp.content
        );
        assert_eq!(game.layout.command, card, "card rect is the command rect");
        assert!(
            (card.w - CARD_W_DP * mobile.density).abs() < 0.6,
            "command card is the 4x3 dp plate ({CARD_W_DP} dp): {card:?}"
        );
        assert!(selection.w > 0.0, "selection panel exists: {selection:?}");

        // The tap-through region is exactly the playfield rectangle between the
        // strip and the console; a tap there must start a world gesture.
        let playfield_top = strip.bottom();
        let playfield_bottom = console.y;
        assert!(
            playfield_bottom > playfield_top + 400.0,
            "the playfield band exists: {playfield_top}..{playfield_bottom}"
        );
        let above = Vec2::new(console.x + console.w * 0.5, console.y - 100.0);
        assert!(!console.contains(above));
        let blocked = game.blocked_rects();
        {
            let blocked = game.blocked_rects();
            let layout = TouchLayout {
                minimap: Some(touch_rect(mobile.minimap)),
                blocked: blocked.as_slice(),
                buttons: game.touch_buttons.as_slice(),
                placing: false,
                scale: game.layout.scale,
            };
            for y in [
                playfield_top + 1.0,
                (playfield_top + playfield_bottom) * 0.5,
                playfield_bottom - 1.0,
            ] {
                let point = Vec2::new(game.viewport.x * 0.5, y);
                assert!(
                    !blocked.iter().any(|rect| rect.contains(point)),
                    "playfield point {point:?} is not a HUD blocker"
                );
                assert!(
                    wc3_input::hit_button(point, &layout).is_none(),
                    "playfield point {point:?} is not a HUD button"
                );
            }
            assert!(
                wc3_input::hit_button(above, &layout).is_none(),
                "padding must not steal world taps"
            );
        }
        game.pointer_down(above);
        assert!(game.touch.is_active(), "a playfield tap starts a world gesture");
        game.pointer_up(above);

        // Both HUD bands swallow their own taps: strip (telemetry) and console.
        // The console point is dead space (left of the minimap, below its ring)
        // so no card/group/minimap button answers instead.
        let dead_console = Vec2::new(console.x + 5.0, console.bottom() - 5.0);
        for (name, point) in [
            ("top strip", Vec2::new(strip.centre().x, strip.centre().y)),
            ("console", dead_console),
        ] {
            let blocked = game.blocked_rects();
            assert!(
                blocked.iter().any(|rect| rect.contains(point)),
                "{name} is a HUD blocker at {point:?}"
            );
            {
                let layout = TouchLayout {
                    minimap: Some(touch_rect(mobile.minimap)),
                    blocked: blocked.as_slice(),
                    buttons: game.touch_buttons.as_slice(),
                    placing: false,
                    scale: game.layout.scale,
                };
                assert!(
                    wc3_input::hit_button(point, &layout).is_none(),
                    "{name} point {point:?} is not a HUD button"
                );
            }
            game.pointer_down(point);
            assert!(!game.touch.is_active(), "{name} tap is swallowed at {point:?}");
            game.pointer_up(point);
        }
    }

    /// Ledger 439 / device frame 2026-11-16: a unit behind the camera's top edge
    /// painted its green health bar over the 57.6 px telemetry strip. Bars are
    /// screen quads, so they must be clamped to the playfield rectangle between
    /// the strip and the console (and off the minimap ring).
    #[test]
    fn hud_bars_playfield_clamp() {
        let mut game = game();
        game.resize(2414.0, 1080.0);
        game.set_safe_insets(210.0, 120.0, 90.0, 96.0);
        let mobile = game.mobile_layout.clone().expect("mobile layout");
        let play_top = mobile.top_meta.bottom();
        let play_bottom = mobile.console.y;
        let ids: Vec<UnitId> = game
            .sim
            .alive_units()
            .filter(|unit| unit.owner == game.player)
            .map(|unit| unit.id)
            .take(2)
            .collect();
        assert_eq!(ids.len(), 2, "two player units for the clamp probe");
        let aim = |game: &Game, screen: Vec2| {
            game.camera
                .screen_to_world(screen, |x, z| game.assets.height_at(x, z))
        };
        let near_top_screen = Vec2::new(game.viewport.x * 0.5, play_top + 20.0);
        let centre_screen = Vec2::new(
            game.viewport.x * 0.5,
            (play_top + play_bottom) * 0.5,
        );
        let near_top = aim(&game, near_top_screen).expect("world under the top edge");
        let centre = aim(&game, centre_screen).expect("world at the playfield centre");
        for (id, pos) in [(ids[0], near_top), (ids[1], centre)] {
            if let Some(unit) = game.sim.units.iter_mut().find(|unit| unit.id == id) {
                unit.pos = pos;
                unit.hp = (unit.max_hp / 2).max(1);
            }
        }
        // Prove the near-top unit's *raw* bar would paint over the strip.
        let unit = game.sim.unit(ids[0]).expect("near-top unit");
        let ground = game.assets.height_at(unit.pos.x, unit.pos.y);
        let lift = ground + game.assets.model_for_def(unit.def().id).height * 0.95;
        let raw = game
            .camera
            .world_to_screen(Vec3::new(unit.pos.x, lift, unit.pos.y))
            .expect("near-top bar projects");
        assert!(
            raw.y < play_top,
            "the probe unit's raw bar is inside the strip ({raw:?} < {play_top}); \
             place it closer to the top edge"
        );

        let bars = game.health_bars();
        let bar_height = 7.0 * game.layout.scale.max(0.1);
        for bar in &bars {
            let quad = Rect::new(
                bar.screen.x - bar.width * 0.5,
                bar.screen.y,
                bar.width,
                bar_height,
            );
            assert!(
                quad.y >= play_top && quad.bottom() <= play_bottom,
                "bar quad {quad:?} outside the playfield {play_top}..{play_bottom}"
            );
            assert!(
                !quad.intersects(&mobile.top_meta) && !quad.intersects(&mobile.console),
                "bar quad {quad:?} paints over a HUD band"
            );
            assert!(
                !quad.intersects(&mobile.minimap),
                "bar quad {quad:?} paints over the minimap"
            );
        }
        assert!(
            !bars.iter().any(|bar| bar.unit == ids[0]),
            "a bar under the strip is rejected, not clamped into the world: {bars:?}"
        );
        assert!(
            bars.iter().any(|bar| bar.unit == ids[1]),
            "the playfield-centre bar is kept: {bars:?}"
        );
    }

    #[test]
    fn tap_selects_moves_and_places_a_building() {
        let mut game = game();
        game.resize(2414.0, 1080.0);
        game.set_safe_insets(210.0, 120.0, 90.0, 96.0);
        let footman = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player && !unit.is_building())
            .expect("mobile player unit");
        let footman_id = footman.id;
        let footman_pos = Vec3::new(
            footman.pos.x,
            game.assets.height_at(footman.pos.x, footman.pos.y)
                + game.assets.model(footman.kind).height * 0.55,
            footman.pos.y,
        );
        let screen = game
            .camera
            .world_to_screen(footman_pos)
            .expect("footman on screen");
        assert!(
            !game.layout.top_bar.contains(screen),
            "unit tap is above the console"
        );
        game.pointer_down(screen);
        game.pointer_up(screen);
        assert!(
            game.selection.contains(&footman_id),
            "tap selects the footman"
        );

        // A ground tap that is guaranteed above the console: pick the screen
        // point first, then let the camera resolve the world point under it.
        let target_screen = Vec2::new(game.viewport.x * 0.5, game.layout.top_bar.y - 180.0);
        assert!(
            !game.layout.top_bar.contains(target_screen),
            "ground tap outside the console"
        );
        game.pointer_down(target_screen);
        game.pointer_up(target_screen);
        assert!(
            game.pending
                .iter()
                .any(|command| matches!(command, Command::Move { .. })),
            "tap on ground issues a move"
        );

        let peasant = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player && unit.kind == UnitKind::Peasant)
            .expect("peasant");
        game.selection = vec![peasant.id];
        game.activate_action(UiAction::Build(UnitKind::TownHall));
        assert!(game.placing.is_some(), "build mode entered");
        let spot = game.find_spot(game.base_pos + Vec2::new(420.0, -180.0));
        let spot_screen = game
            .camera
            .world_to_screen(Vec3::new(
                spot.x,
                game.assets.height_at(spot.x, spot.y),
                spot.y,
            ))
            .expect("build spot on screen");
        assert!(
            !game.layout.top_bar.contains(spot_screen),
            "build spot outside the console"
        );
        game.pointer_down(spot_screen);
        game.pointer_up(spot_screen);
        assert!(
            game.log
                .iter()
                .any(|line| line.starts_with("PLACE_BUILT htow")),
            "tap in build mode spawns and charges the building"
        );
        assert!(game.placing.is_none(), "placement mode cleared");
    }

    /// The merged mobile HUD is a bottom console: taps, drags and placement
    /// clicks that land on the console must not reach the world, while the same
    /// gestures above it must keep working.
    #[test]
    fn console_layout_blocks_world_clicks_and_placement() {
        let mut game = game();
        game.resize(2414.0, 1080.0);
        game.set_safe_insets(210.0, 120.0, 90.0, 96.0);
        let console = game.layout.top_bar;
        // Console top strip right of the selection panel: inside the console
        // blocker, clear of the minimap, the selection and the group row.
        let dead = Vec2::new(
            console.right() - 3.0 * game.layout.scale,
            console.y + 4.0 * game.layout.scale,
        );
        assert!(console.contains(dead), "dead space is inside the console");
        assert!(!game.layout.minimap.contains(dead));
        assert!(!game.layout.selection.contains(dead));

        // Select a player unit with a tap above the console.
        let footman = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player)
            .expect("player unit");
        let footman_id = footman.id;
        let footman_screen = game
            .camera
            .world_to_screen(Vec3::new(
                footman.pos.x,
                game.assets.height_at(footman.pos.x, footman.pos.y)
                    + game.assets.model(footman.kind).height * 0.55,
                footman.pos.y,
            ))
            .expect("unit on screen");
        assert!(!console.contains(footman_screen));
        game.pointer_down(footman_screen);
        game.pointer_up(footman_screen);
        assert!(game.selection.contains(&footman_id));
        let selected = game.selection.clone();

        // A console tap must not click through to the world.
        let pending = game.pending.len();
        game.pointer_down(dead);
        game.pointer_up(dead);
        assert_eq!(game.selection, selected, "console tap must not re-select");
        assert_eq!(game.pending.len(), pending, "console tap must not order");
        assert!(!game.touch.is_active());

        // A drag that starts on the console must not become a selection box.
        game.pointer_down(dead);
        for _ in 0..12 {
            game.update(1.0 / 30.0);
        }
        game.pointer_move(Vec2::new(dead.x, console.y - 120.0));
        game.pointer_up(Vec2::new(dead.x, console.y - 120.0));
        assert_eq!(game.selection, selected, "console drag must not box-select");

        // Placement mode: a console tap must not place or clear the mode.
        let peasant = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player && unit.kind == UnitKind::Peasant)
            .expect("peasant");
        game.selection = vec![peasant.id];
        game.activate_action(UiAction::Build(UnitKind::TownHall));
        assert!(game.placing.is_some(), "build mode entered");
        let pending = game.pending.len();
        game.pointer_down(dead);
        game.pointer_up(dead);
        assert!(game.placing.is_some(), "console tap must not place");
        assert_eq!(
            game.pending.len(),
            pending,
            "no build command from the console"
        );

        // The same placement tap above the console still builds.
        let spot = game.find_spot(game.base_pos + Vec2::new(420.0, -180.0));
        let spot_screen = game
            .camera
            .world_to_screen(Vec3::new(
                spot.x,
                game.assets.height_at(spot.x, spot.y),
                spot.y,
            ))
            .expect("build spot on screen");
        assert!(!console.contains(spot_screen));
        game.pointer_down(spot_screen);
        game.pointer_up(spot_screen);
        assert!(
            game.log
                .iter()
                .any(|line| line.starts_with("PLACE_BUILT htow")),
            "world tap in build mode spawns and charges the building"
        );
        assert!(
            game.placing.is_none(),
            "placement mode cleared after a world tap"
        );
    }

    #[test]
    #[ignore = "diagnostic: prints device coordinates; needs the device; run_layer_gates.sh --ignored"]
    fn print_build_device_coords() {
        let mpq =
            std::path::Path::new("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq");
        let map = std::path::Path::new(
            "/data/data/com.termux/files/home/src/herofolends-rs/analysis/war3_data/Maps/Campaign/Human04.w3m",
        );
        if !mpq.is_file() || !map.is_file() {
            eprintln!("SKIP coords: install missing");
            return;
        }
        let assets = Assets::load(Some(mpq), Some(map));
        let mut game = Game::new(assets, 0xC0FFEE);
        game.resize(2414.0, 1080.0);
        game.set_safe_insets(210.0, 120.0, 90.0, 96.0);
        let peasant = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player && unit.kind == UnitKind::Peasant)
            .expect("peasant");
        let p = Vec3::new(
            peasant.pos.x,
            game.assets.height_at(peasant.pos.x, peasant.pos.y)
                + game.assets.model(UnitKind::Peasant).height * 0.55,
            peasant.pos.y,
        );
        let screen = game.camera.world_to_screen(p).expect("peasant screen");
        let slot = game.mobile_layout.as_ref().expect("layout").card_slots[7].centre();
        let spot = game.find_spot(game.base_pos + Vec2::new(420.0, -180.0));
        let ground = game
            .camera
            .world_to_screen(Vec3::new(
                spot.x,
                game.assets.height_at(spot.x, spot.y),
                spot.y,
            ))
            .expect("ground screen");
        let base = game
            .camera
            .world_to_screen(Vec3::new(
                game.base_pos.x,
                game.assets.height_at(game.base_pos.x, game.base_pos.y),
                game.base_pos.y,
            ))
            .expect("base screen");
        eprintln!(
            "BUILD_COORDS peasant=({:.1},{:.1}) slot7=({:.1},{:.1}) ground=({:.1},{:.1}) base=({:.1},{:.1}) viewport={:?}",
            screen.x, screen.y, slot.x, slot.y, ground.x, ground.y, base.x, base.y, game.viewport
        );
        for y in (300..=700).step_by(100) {
            for x in (800..=1800).step_by(100) {
                if let Some(world) = game
                    .camera
                    .screen_to_world(Vec2::new(x as f32, y as f32), |wx, wz| {
                        game.assets.height_at(wx, wz)
                    })
                {
                    let cell = game.sim.grid.world_to_cell(world);
                    if game.sim.grid.walkable(cell.0, cell.1) {
                        eprintln!(
                            "WALK screen=({x},{y}) world=({:.0},{:.0}) cell={cell:?}",
                            world.x, world.y
                        );
                    }
                }
            }
        }
        for unit in game.sim.alive_units().filter(|u| u.owner == game.player) {
            let p = Vec3::new(
                unit.pos.x,
                game.assets.height_at(unit.pos.x, unit.pos.y)
                    + game.assets.model(unit.kind).height * 0.55,
                unit.pos.y,
            );
            if let Some(s) = game.camera.world_to_screen(p) {
                eprintln!(
                    "UNIT kind={:?} id={:?} screen=({:.1},{:.1})",
                    unit.kind, unit.id, s.x, s.y
                );
            }
        }
    }

    #[test]
    fn build_button_slot7_enters_placement() {
        // Worker card (retail information design): the first nine tiles ARE the
        // build list (`opeo` builds nine structures), the generic orders share
        // the last row. Slot 0 is the first build entry; tapping it must enter
        // placement mode for exactly that def.
        let mut game = game();
        game.resize(2414.0, 1080.0);
        let peasant = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player && unit.def().stats.is_worker)
            .expect("peasant");
        game.selection = vec![peasant.id];
        game.refresh_buttons();
        let button = game.buttons.first().cloned().expect("build button");
        let slot = game.mobile_layout.as_ref().expect("layout").card_slots[0];
        let hit = wc3_input::hit_button(
            slot.centre(),
            &wc3_input::TouchLayout {
                minimap: Some(touch_rect(game.layout.minimap)),
                blocked: game.blocked_rects().as_slice(),
                buttons: game.touch_buttons.as_slice(),
                placing: game.placing.is_some(),
                scale: game.layout.scale,
            },
        );
        eprintln!(
            "BUILD_DIAG selection={:?} workers={} slot0=({:?},{:?},enabled={}) tap=({},{},{},{}) hit={:?}",
            game.selection,
            game.selected_worker().map(|w| w.len()).unwrap_or(0),
            button.label,
            button.action,
            button.enabled,
            slot.x,
            slot.y,
            slot.w,
            slot.h,
            hit
        );
        assert!(button.enabled, "the first build entry is affordable/available");
        assert_eq!(hit, Some(0), "slot 0 hits the first build button");
        let expected = match button.action {
            UiAction::BuildDef(id) => Some(id),
            UiAction::Build(kind) => Some(kind.canonical_id()),
            _ => None,
        }
        .expect("a build action");
        game.pointer_down(slot.centre());
        game.pointer_up(slot.centre());
        assert_eq!(game.placing, Some(expected), "BUILD enters placement mode");
    }


    /// Long-press on a world unit opens its WC3 tooltip; release clears it.
    #[test]
    fn long_press_shows_a_unit_tooltip() {
        let mut game = touch_game();
        game.camera.centre_on(game.base_pos);
        let footman = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player)
            .expect("player unit");
        let screen = game
            .camera
            .world_to_screen(Vec3::new(
                footman.pos.x,
                game.assets.height_at(footman.pos.x, footman.pos.y)
                    + game.assets.model(footman.kind).height * 0.55,
                footman.pos.y,
            ))
            .expect("unit on screen");
        game.pointer_down(screen);
        for _ in 0..16 {
            game.update(1.0 / 30.0);
        }
        let tooltip = game.tooltip_text.clone().expect("long-press tooltip");
        assert!(tooltip.contains("HP"), "tooltip text: {tooltip}");
        game.pointer_up(screen);
        game.update(1.0 / 30.0);
        assert!(game.tooltip_text.is_none(), "release clears the tooltip");
    }

    /// ARMY chip: hold saves the selection to a control group, double tap
    /// recalls the group; a single tap keeps selecting the whole army.
    #[test]
    fn army_chip_hold_saves_and_double_tap_recalls_a_group() {
        let mut game = touch_game();
        let footman = game
            .sim
            .alive_units()
            .find(|unit| {
                unit.owner == game.player && !unit.is_building() && !unit.def().stats.is_worker
            })
            .map(|unit| unit.id)
            .expect("player army unit");
        game.selection = vec![footman];
        game.update(1.0 / 30.0);
        let chip_index = game
            .buttons
            .iter()
            .position(|button| button.action == UiAction::SelectArmy)
            .expect("ARMY chip");
        let chip = game.buttons[chip_index].clone();
        game.pointer_down(chip.rect.centre());
        for _ in 0..16 {
            game.update(1.0 / 30.0);
        }
        game.pointer_up(chip.rect.centre());
        assert_eq!(
            game.control_groups.len(1),
            1,
            "hold saves the selection to the first free group"
        );

        // A later single tap selects the army, the following tap recalls group 1.
        game.selection.clear();
        game.update(1.0 / 30.0);
        game.elapsed += 1.0;
        let chip = game
            .buttons
            .iter()
            .find(|button| button.action == UiAction::SelectArmy)
            .cloned()
            .expect("ARMY chip");
        game.pointer_down(chip.rect.centre());
        game.pointer_up(chip.rect.centre());
        assert!(
            game.selection.len() > 1,
            "single ARMY tap still selects the whole army"
        );
        // The chip's double-tap cycles control groups (`cycle_group_from_touch`):
        // drive it directly so the assertion measures the group logic, not the
        // touch double-tap timer.
        game.selection.clear();
        game.cycle_group_from_touch();
        assert_eq!(
            game.selection,
            vec![footman],
            "the ARMY chip cycle recalls the saved group"
        );
        // HOLD on a group row chip saves the current selection to that slot.
        game.selection = vec![footman];
        let row = game.group_row_slots();
        game.pointer_down(row[2].centre());
        for _ in 0..16 {
            game.update(1.0 / 30.0);
        }
        game.pointer_up(row[2].centre());
        assert_eq!(
            game.control_groups.len(3),
            1,
            "hold on group row 3 saves the selection there"
        );
    }

    /// Keyboard parity: digits assign/recall groups, card letters activate the
    /// visible action, Escape cancels placement/targeting.
    #[test]
    fn hotkeys_drive_the_card_and_control_groups() {
        let mut game = touch_game();
        game.update(1.0 / 30.0);
        game.activate_action(UiAction::SelectArmy);
        let army = game.selection.len();
        assert!(army > 1);
        game.hotkey(wc3_input::Hotkey::Digit(4), true);
        assert_eq!(game.control_groups.len(4), army, "ctrl+digit assigns");
        game.selection.clear();
        game.key_char('4', false);
        assert_eq!(game.selection.len(), army, "raw key char recalls the group");

        game.selection.clear();
        game.hotkey(wc3_input::Hotkey::Letter('b'), false);
        assert!(game.selected_hall().is_some(), "B selects the town hall");

        game.placing = Some("htow");
        game.hotkey(wc3_input::Hotkey::Escape, false);
        assert!(game.placing.is_none(), "Escape cancels placement");
    }

    /// The on-screen control-group row is drawn inside the console dead space,
    /// clear of every HUD widget, and its ten slots (1..9,0) save/recall/centre groups.
    #[test]
    fn control_group_row_is_on_screen_and_tappable() {
        let mut game = touch_game();
        game.update(1.0 / 30.0);
        let console = game.layout.top_bar;
        let slots = game.group_row_slots();
        assert_eq!(slots.len(), 10, "WC3 row is 1-0");
        for (index, rect) in slots.iter().enumerate() {
            let slot = if index < 9 { index as u8 + 1 } else { 0 };
            assert!(
                console.contains(rect.centre()),
                "slot {slot} inside the console"
            );
            assert!(
                !game.layout.command.intersects(rect),
                "slot {slot} clear of the card"
            );
            assert!(!game.layout.selection.intersects(rect));
            assert!(!game.layout.minimap.intersects(rect));
        }
        // The row must actually reach the UI batch (drawn after the console).
        let scene = game.scene();
        let first = slots[0];
        assert!(
            scene.ui.white.iter().any(|vertex| {
                (vertex.pos[0] - first.x).abs() < 0.5 && (vertex.pos[1] - first.y).abs() < 0.5
            }),
            "group row fill is drawn at slot 1"
        );

        let footman = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player && !unit.is_building())
            .map(|unit| unit.id)
            .expect("mobile player unit");
        game.selection = vec![footman];
        game.update(1.0 / 30.0);

        // Slot 3 (index 2): hold saves, tap recalls, double-tap centres.
        let slot3 = game.group_row_slots()[2];
        game.pointer_down(slot3.centre());
        assert_eq!(game.touch.pressed_button(), Some(100 + 2));
        for _ in 0..16 {
            game.update(1.0 / 30.0);
        }
        game.pointer_up(slot3.centre());
        assert_eq!(game.control_groups.len(3), 1, "hold saves to slot 3");

        game.selection.clear();
        game.update(1.0 / 30.0);
        game.pointer_down(slot3.centre());
        game.pointer_up(slot3.centre());
        assert_eq!(game.selection, vec![footman], "tap recalls slot 3");

        game.selection.clear();
        game.elapsed += 1.0;
        game.camera.centre_on(game.base_pos);
        let before = game.camera.target;
        game.pointer_down(slot3.centre());
        game.pointer_up(slot3.centre());
        game.pointer_down(slot3.centre());
        game.pointer_up(slot3.centre());
        assert_eq!(game.selection, vec![footman]);
        assert!(
            game.camera.target.distance(before) > 1.0,
            "double tap centres the group"
        );

        // Slot 0 (index 9): hold saves, tap recalls (WC3 1-0 row).
        game.selection = vec![footman];
        game.update(1.0 / 30.0);
        let slot0 = game.group_row_slots()[9];
        game.pointer_down(slot0.centre());
        assert_eq!(game.touch.pressed_button(), Some(100 + 9));
        for _ in 0..16 {
            game.update(1.0 / 30.0);
        }
        game.pointer_up(slot0.centre());
        assert_eq!(game.control_groups.len(0), 1, "hold saves to slot 0");

        game.selection.clear();
        game.update(1.0 / 30.0);
        game.pointer_down(slot0.centre());
        game.pointer_up(slot0.centre());
        assert_eq!(game.selection, vec![footman], "tap recalls slot 0");
    }

    /// The box marquee follows the finger and clears on release.
    #[test]
    fn box_marquee_rect_tracks_the_drag() {
        let mut game = touch_game();
        let a = Vec2::new(500.0, 300.0);
        let b = Vec2::new(760.0, 470.0);
        game.pointer_down(a);
        for _ in 0..12 {
            game.update(1.0 / 30.0);
        }
        game.pointer_move(b);
        let rect = game.box_marquee_rect().expect("marquee while boxing");
        assert!((rect.x - a.x).abs() < 1.0 && (rect.y - a.y).abs() < 1.0);
        assert!((rect.w - 260.0).abs() < 1.0 && (rect.h - 170.0).abs() < 1.0);
        // The marquee must reach the UI batch at the finger rectangle.
        let scene = game.scene();
        assert!(
            scene.ui.white.iter().any(|vertex| {
                (vertex.pos[0] - rect.x).abs() < 0.5 && (vertex.pos[1] - rect.y).abs() < 0.5
            }),
            "marquee fill is drawn at the drag rectangle"
        );
        game.pointer_up(b);
        assert!(game.box_marquee_rect().is_none(), "marquee clears on release");
    }

    #[test]
    fn command_card_tiles_have_a_minimum_finger_sized_hit_target() {
        let mut game = touch_game();
        // Pin 1 px/dp so the 1280x720 viewport is the inline wide card; the
        // 2.0 phone fallback makes it the compact drawer (no tile on screen)
        // and is not what this target-padding test is about.
        game.set_density(1.0);
        // The first card tile sits on the top row, so the padded hit target
        // above it can only be claimed by the button itself.
        let tile_index = game.card_button_indices()[0];
        let tile = game.buttons[tile_index].clone();
        let min_target = wc3_input::MIN_BUTTON_TARGET * game.layout.scale;
        assert!(
            tile.rect.w + 0.01 >= min_target && tile.rect.h + 0.01 >= min_target,
            "card tile is a minimum finger target: {:?}",
            tile.rect
        );
        // The router pads a target by `max((min_target - h) / 2, slop)`;
        // a probe inside that band must press this very tile.
        let inside = Vec2::new(tile.rect.centre().x, tile.rect.y - 2.0 * game.layout.scale);
        assert!(!tile.rect.contains(inside), "outside the visual tile");
        game.pointer_down(inside);
        assert_eq!(
            game.touch.pressed_button(),
            Some(tile_index),
            "the padded card hit target must claim a finger just above the tile"
        );
        game.pointer_cancel();
        // A finger beyond the pad is not hijacked by the tile.
        let tile = game.buttons[tile_index].rect;
        let outside = Vec2::new(tile.centre().x, tile.y - 12.0 * game.layout.scale);
        game.pointer_down(outside);
        assert_ne!(
            game.touch.pressed_button(),
            Some(tile_index),
            "a finger beyond the target band is not the tile"
        );
        game.pointer_cancel();
    }

    #[test]
    fn compact_landscape_closed_drawer_keeps_card_actions_non_interactive() {
        // AUD-001: compact landscape has no `card_slots` while the command
        // drawer is closed. A refresh/resize in that state must not index the
        // empty slot list, and every action stays list-only (zero rect, not a
        // phantom target) until the drawer opens.
        use crate::hud::mobile_hud::HudLayoutMode;

        let mut game = touch_game(); // 1280x720 px @ 2.0 px/dp = 640x360 dp
        assert_eq!(
            game.mobile_layout.as_ref().expect("layout").layout_mode,
            HudLayoutMode::LandscapeCompact
        );
        assert_eq!(
            hud::build_buttons(&game).len(),
            12,
            "build_buttons authors all 12 card slots"
        );
        assert_eq!(game.card_button_indices(), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
        let hidden = |game: &Game| {
            game.card_button_indices().iter().all(|index| {
                let rect = game.buttons[*index].rect;
                rect.w == 0.0 && rect.h == 0.0
            })
        };
        assert!(game.mobile_layout.as_ref().unwrap().card_slots.is_empty());
        assert!(hidden(&game), "closed drawer: card actions have no rect");

        // The crash path from AUD-001: refresh with an empty `card_slots`.
        game.refresh_buttons();
        assert!(hidden(&game));
        assert!(
            game.touch_buttons
                .iter()
                .filter(|button| button.index < 12)
                .all(|button| button.rect.w == 0.0 && button.rect.h == 0.0),
            "hidden card actions must not keep a touch rect"
        );
        let blocked = game.blocked_rects();
        let layout = TouchLayout {
            minimap: Some(touch_rect(game.layout.minimap)),
            blocked: &blocked,
            buttons: &game.touch_buttons,
            placing: false,
            scale: game.density_scale,
        };
        assert_eq!(wc3_input::hit_button(Vec2::ZERO, &layout), None);

        // Open: the 12 actions map one-to-one onto the rendered drawer slots,
        // and the router's hitbox is the rendered rect.
        assert!(game.toggle_commands_drawer());
        let slots = game.mobile_layout.as_ref().unwrap().card_slots.clone();
        assert_eq!(slots.len(), 12, "4x3 drawer");
        for (slot, index) in game.card_button_indices().iter().enumerate() {
            let rect = game.buttons[*index].rect;
            assert_eq!(rect, slots[slot], "action {index} must sit on slot {slot}");
            let target = game
                .touch_buttons
                .iter()
                .find(|button| button.index == *index)
                .expect("card touch target");
            assert_eq!(
                target.rect,
                wc3_input::Rect::new(rect.x, rect.y, rect.w, rect.h),
                "hitbox == rendered rect for action {index}"
            );
            game.pointer_down(rect.centre());
            assert_eq!(game.touch.pressed_button(), Some(*index));
            game.pointer_cancel();
        }

        // Resize with the drawer open re-solves both sides together.
        game.resize(1600.0, 900.0);
        let slots = game.mobile_layout.as_ref().unwrap().card_slots.clone();
        assert_eq!(slots.len(), 12);
        for (slot, index) in game.card_button_indices().iter().enumerate() {
            assert_eq!(game.buttons[*index].rect, slots[slot]);
        }

        // Close again -> list-only, then portrait keeps the card inline, then
        // back to the compact drawer.
        assert!(game.close_hud_drawers());
        assert!(hidden(&game));
        game.resize(720.0, 1280.0);
        assert_eq!(
            game.mobile_layout.as_ref().unwrap().layout_mode,
            HudLayoutMode::Portrait
        );
        assert_eq!(game.mobile_layout.as_ref().unwrap().card_slots.len(), 12);
        assert!(!hidden(&game), "portrait card is inline");
        game.resize(1280.0, 720.0);
        assert!(hidden(&game), "back to compact: list-only again");

        // Smallest supported window: no panic and no phantom targets.
        game.resize(320.0, 240.0);
        assert!(hidden(&game));
    }

    #[test]
    fn window_metrics_push_framebuffer_density_and_insets_once() {
        // AUD-004: the platform hook hands the HUD the real framebuffer, the
        // real density and pixel insets in one snapshot; the layout divides
        // the insets once and multiplies dp rects once.
        use crate::hud::mobile_hud::HudLayoutMode;

        let mut game = game();
        game.set_window_metrics(Vec2::new(1280.0, 720.0), 1.0, SafeInsets::default());
        assert_eq!(game.viewport, Vec2::new(1280.0, 720.0));
        assert_eq!(game.density_scale, 1.0);
        {
            let mobile = game.mobile_layout.as_ref().expect("layout");
            assert_eq!(mobile.layout_mode, HudLayoutMode::LandscapeWide);
            assert_eq!(mobile.dp().screen, Vec2::new(1280.0, 720.0));
            assert_eq!(game.buttons[0].rect, mobile.card_slots[0]);
        }

        // 1280x720 px at 2 px/dp is a 640x360 dp phone: compact, drawer shut.
        game.set_window_metrics(Vec2::new(1280.0, 720.0), 2.0, SafeInsets::default());
        assert_eq!(game.density_scale, 2.0);
        {
            let mobile = game.mobile_layout.as_ref().expect("layout");
            assert_eq!(mobile.dp().screen, Vec2::new(640.0, 360.0));
            assert_eq!(mobile.layout_mode, HudLayoutMode::LandscapeCompact);
            assert!(mobile.card_slots.is_empty());
        }

        // 40 px inset at 2 px/dp is 20 dp -> 40 px on screen, never 80 px.
        game.set_window_metrics(
            Vec2::new(1280.0, 720.0),
            2.0,
            SafeInsets::new(40.0, 0.0, 0.0, 0.0),
        );
        {
            let mobile = game.mobile_layout.as_ref().expect("layout");
            assert!((mobile.dp().safe.left - 20.0).abs() < 1e-3);
            assert!((mobile.safe_frame.x - 40.0).abs() < 0.51);
            assert!(
                mobile.console.x.abs() < 1e-3,
                "console stays edge to edge"
            );
        }

        // Rotation keeps the density; portrait solves an inline card.
        game.set_window_metrics(Vec2::new(720.0, 1280.0), 2.0, SafeInsets::default());
        {
            let mobile = game.mobile_layout.as_ref().expect("layout");
            assert_eq!(mobile.layout_mode, HudLayoutMode::Portrait);
            assert_eq!(mobile.card_slots.len(), 12);
            assert_eq!(game.buttons[0].rect, mobile.card_slots[0]);
        }

        // `set_density` is the single-value path and rejects a no-op.
        assert!(game.set_density(3.0));
        assert_eq!(game.density_scale, 3.0);
        assert!(!game.set_density(3.0));
    }

    #[test]
    fn scene_applies_the_model_yaw_offset() {
        let game = touch_game();
        let archer = game
            .sim
            .alive_units()
            .find(|unit| unit.kind == UnitKind::Archer && unit.owner == game.player)
            .expect("starting archer");
        let position = Vec3::new(
            archer.pos.x,
            game.assets.height_at(archer.pos.x, archer.pos.y),
            archer.pos.y,
        );
        let expected =
            Quat::from_rotation_y(-archer.facing + game.assets.model(UnitKind::Archer).yaw_offset);
        let scene = game.scene();
        let draw = scene
            .units
            .iter()
            .find(|draw| (draw.world.w_axis.truncate() - position).length() < 0.01)
            .expect("archer draw");
        let (_, rotation, _) = draw.world.to_scale_rotation_translation();
        assert!(
            (rotation.dot(expected).abs() - 1.0).abs() < 1e-4,
            "archer yaw offset must reach the matrix: {rotation:?} vs {expected:?}"
        );
    }
    // ------------------------------------------------------- merge: sim/game

    #[test]
    fn scenario_wires_casters_farm_and_sim_ai() {
        let game = game();
        assert!(
            game.sim
                .alive_units()
                .any(|unit| unit.owner == game.player && unit.def().id == "Hpal")
        );
        assert!(
            game.sim
                .alive_units()
                .any(|unit| unit.owner == game.player && unit.def().id == "Hamg")
        );
        assert!(
            game.sim
                .alive_units()
                .any(|unit| unit.owner == game.enemy && unit.def().id == "Ofar")
        );
        assert!(
            game.sim
                .alive_units()
                .any(|unit| unit.owner == game.player && unit.def().id == "hhou")
        );
        assert!(game.sim.ai.iter().any(|state| state.owner == game.enemy));
        assert!(game.supply().1 >= 18, "hall+farm cap: {:?}", game.supply());
    }

    #[test]
    fn build_menu_exposes_farm_and_tower() {
        let mut game = game();
        let worker = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player && unit.def().stats.is_worker)
            .expect("peasant")
            .id;
        game.selection = vec![worker];
        let buttons = hud::build_buttons(&game);
        assert!(
            buttons
                .iter()
                .any(|button| button.action == UiAction::BuildDef("hhou"))
        );
        assert!(
            buttons
                .iter()
                .any(|button| button.action == UiAction::BuildDef("hgtw"))
        );
    }

    #[test]
    fn farm_placement_from_card_raises_supply() {
        let mut game = game();
        // Flush the scenario's opening harvest commands before placing, so the
        // construct order is the last order the worker receives.
        game.update(1.0 / 30.0);
        let worker = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player && unit.def().stats.is_worker)
            .expect("peasant")
            .id;
        game.selection = vec![worker];
        let before = game.supply().1;
        game.activate_action(UiAction::BuildDef("hhou"));
        assert_eq!(game.placing, Some("hhou"));
        let spot = game.find_spot(game.base_pos + Vec2::new(620.0, 520.0));
        for unit in game.sim.units.iter_mut() {
            if unit.owner == game.enemy && !unit.is_building() {
                unit.alive = false;
            }
        }
        game.sim.disable_ai(game.enemy);
        game.place_building(spot);
        for _ in 0..2600 {
            game.update(1.0 / 30.0);
        }
        let farms = game
            .sim
            .alive_units()
            .filter(|unit| unit.owner == game.player && unit.def().id == "hhou")
            .count();
        assert!(farms >= 3, "two starting farms + built farm: {farms}");
        assert!(
            game.supply().1 > before,
            "farm raised cap: {} -> {}",
            before,
            game.supply().1
        );
    }

    #[test]
    fn caster_buttons_and_holy_light_work() {
        let mut game = game();
        let paladin = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player && unit.def().id == "Hpal")
            .expect("paladin")
            .id;
        let ally = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player && unit.def().id == "hfoo")
            .expect("footman")
            .id;
        game.selection = vec![paladin];
        let (label, _, action, enabled) = game.ability_button(0).expect("holy light");
        assert_eq!(label, "HOLY LIGHT");
        assert_eq!(action, UiAction::Cast(Spell::HolyLight));
        assert!(enabled);
        assert!(game.ability_button(2).is_none());
        let buttons = hud::build_buttons(&game);
        assert!(
            buttons
                .iter()
                .any(|button| button.action == UiAction::Cast(Spell::HolyLight))
        );

        let hp = game.sim.unit(ally).unwrap().hp - 200;
        game.sim.set_unit_hp(ally, hp);
        let before_hp = game.sim.unit(ally).unwrap().hp;
        let before_mana = game.sim.unit(paladin).unwrap().mana;
        game.perform_cast(paladin, Spell::HolyLight, SpellTarget::Unit(ally));
        assert!(game.sim.unit(ally).unwrap().hp > before_hp);
        assert!(game.sim.unit(paladin).unwrap().mana < before_mana);
        assert!(game.targeting.is_none());
    }

    #[test]
    fn unit_target_spell_enters_target_mode() {
        let mut game = game();
        let paladin = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player && unit.def().id == "Hpal")
            .expect("paladin")
            .id;
        game.selection = vec![paladin];
        game.activate_action(UiAction::Cast(Spell::HolyLight));
        assert_eq!(game.targeting, Some(Spell::HolyLight));
        game.activate_action(UiAction::Cast(Spell::DivineShield));
        assert!(game.sim.unit(paladin).unwrap().invulnerable_until > game.sim.tick);
    }
    #[test]
    fn hall_placement_from_card_charges_gold_once() {
        let mut game = game();
        game.update(1.0 / 30.0);
        game.sim.resources[game.player as usize] = 1000;
        let worker = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player && unit.def().stats.is_worker)
            .expect("peasant")
            .id;
        game.selection = vec![worker];
        let before_units = game.sim.alive_units().count();
        game.activate_action(UiAction::Build(UnitKind::TownHall));
        let spot = game.find_spot(game.base_pos + Vec2::new(900.0, 700.0));
        game.place_building(spot);
        // Board placement is atomic: no pending command, gold and unit change
        // already happened when the world tap was accepted.
        assert!(!matches!(game.pending.last(), Some(Command::Build { .. })));
        assert_eq!(game.sim.resources[game.player as usize], 550);
        assert_eq!(game.sim.alive_units().count(), before_units + 1);
        assert!(
            game.log
                .iter()
                .any(|line| line.starts_with("PLACE_BUILT htow"))
        );

        // Rejection path: 400 gold cannot pay the 450 hall.
        game.sim.resources[game.player as usize] = 400;
        game.selection = vec![worker];
        game.activate_action(UiAction::Build(UnitKind::TownHall));
        assert!(!game.can_afford_def(UnitKind::TownHall.canonical_id()));
        let spot = game.find_spot(game.base_pos + Vec2::new(1300.0, 900.0));
        game.place_building(spot);
        assert_eq!(game.sim.resources[game.player as usize], 400);
        assert!(
            game.log
                .iter()
                .any(|line| line.starts_with("DENY BUILD gold"))
        );
    }

    /// P0 gate: the melee arena runs a real shipped map, spawns exactly one
    /// hall + one gold mine + five creeps per side, and every spawn sits on a
    /// walkable cell (no units inside textures, no scripted third hall).
    #[test]
    fn map_units_spawn() {
        let map = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/Maps/Campaign/Orc01.w3m");
        if !map.is_file() {
            eprintln!("skip: Orc01.w3m missing");
            return;
        }
        let game = Game::new(crate::assets::Assets::load(None, Some(&map)), 0x5EED);
        assert!(game.melee, "Orc01 runs as the melee scenario");
        assert!(game.engine.is_none(), "campaign script is not booted");
        let halls: Vec<&wc3_sim::Unit> = game
            .sim
            .alive_units()
            .filter(|unit| unit.def().main_hall)
            .collect();
        assert_eq!(halls.len(), 2, "one town hall per side: {halls:?}");
        // One start mine per side plus two expansion mines per side (the
        // canonical melee layout guards every expansion with its own camp).
        assert_eq!(game.sim.gold_mines.len(), 6, "2 start + 4 expansion mines");
        assert_eq!(game.sim.camps.len(), 6, "one camp per mine");
        let creeps = game
            .sim
            .alive_units()
            .filter(|unit| unit.owner == 2 && !unit.is_building())
            .count();
        assert_eq!(creeps, 30, "five creeps per camp: 2 start + 4 expansion mines");
        // Every non-building spawn must be walkable, and buildings must not be
        // walled in by blocked cells (the "hall inside textures" symptom).
        for unit in game.sim.alive_units() {
            let cell = game.sim.grid.world_to_cell(unit.pos);
            if unit.is_building() {
                // The spawn blocked the hall's own footprint; a building is
                // healthy when at least one cell just outside its footprint is
                // open (not embedded in trees/water/textures).
                let footprint = unit.def().footprint.max(1) as i64;
                let reach = footprint / 2 + 1;
                let ring_open = (-1..=1).any(|dy| {
                    (-1..=1).any(|dx| {
                        if dx == 0 && dy == 0 {
                            return false;
                        }
                        let x = cell.0 as i64 + dx * reach;
                        let y = cell.1 as i64 + dy * reach;
                        x >= 0
                            && y >= 0
                            && x < game.sim.grid.width as i64
                            && y < game.sim.grid.height as i64
                            && game.sim.grid.walkable(x as u32, y as u32)
                    })
                });
                assert!(ring_open, "building {} is walled in at {:?}", unit.def().id, cell);
            } else {
                assert!(
                    game.sim.grid.walkable(cell.0, cell.1),
                    "unit {} spawned in a blocked cell {:?}",
                    unit.def().id,
                    cell
                );
            }
            let ground = game.assets.height_at(unit.pos.x, unit.pos.y);
            assert!(ground.is_finite(), "unit {} has no terrain height", unit.def().id);
        }
        let mut game = game;
        game.update(TICK_SECONDS);
        assert!(!game.sim.alive_units().collect::<Vec<_>>().is_empty());
        assert!(
            game.log.iter().any(|line| line.starts_with("GAME_SCENARIO melee")),
            "melee scenario logged: {:?}",
            game.log
        );
    }

    /// 3D product verification for the melee arena: real MPQ models on real
    /// terrain heights with the pivot offset, rendered through the game renderer.
    #[test]
    #[ignore = "heavy: renders the melee scenario with real MPQ models; needs War3.mpq + GPU; run_layer_gates.sh --ignored"]
    fn melee_scene_render() {
        let map = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/Maps/Campaign/Orc01.w3m");
        let mpq = [
            "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
            "/data/data/org.herofolends.p0/files/War3.mpq",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|path| path.is_file());
        let (true, Some(mpq)) = (map.is_file(), mpq) else {
            crate::dev_log!("SKIP melee_scene_render: map/mpq missing");
            return;
        };
        let mut game = Game::new(crate::assets::Assets::load(Some(&mpq), Some(&map)), 0x5EED);
        for unit in game.sim.alive_units().filter(|unit| unit.is_building()) {
            let model = game.assets.model(unit.kind);
            let ground = game.assets.height_at(unit.pos.x, unit.pos.y);
            crate::dev_log!(
                "MELEE_PLACEMENT id={} pos=({:.0},{:.0}) ground={:.1} pivot={:.1} base_y={:.1} height={:.1}",
                unit.def().id,
                unit.pos.x,
                unit.pos.y,
                ground,
                model.pivot,
                ground - model.pivot,
                model.height
            );
        }
        game.camera.viewport = Vec2::new(1280.0, 720.0);
        game.camera.distance = 1700.0;
        let scene = game.scene();
        let font = crate::hud::Font::load();
        let options = crate::render::RenderOptions {
            skip_ui: true,
            ..Default::default()
        };
        let rgba = crate::render::render_scene_to_rgba_with(
            &game.assets,
            &font.atlas,
            None,
            &scene,
            1280,
            720,
            options,
        )
        .expect("melee scene render");
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-world/gltf");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("melee-scene.ppm");
        crate::render::write_ppm(path.to_str().unwrap(), 1280, 720, &rgba);
        let background = [rgba[0], rgba[1], rgba[2]];
        let non_bg = rgba
            .chunks_exact(4)
            .filter(|px| {
                (px[0] as i32 - background[0] as i32).abs()
                    + (px[1] as i32 - background[1] as i32).abs()
                    + (px[2] as i32 - background[2] as i32).abs()
                    > 24
            })
            .count();
        crate::dev_log!(
            "MELEE_SCENE_RENDER halls={} units={} non_bg={} file={}",
            game.sim.alive_units().filter(|unit| unit.is_building()).count(),
            game.sim.alive_units().count(),
            non_bg,
            path.display()
        );
        assert!(non_bg > 50_000, "scene has visible terrain/units: {non_bg}");
    }

    #[test]
    fn map_runtime_campaign_script_boots() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
        let map = root.join("Maps/Campaign/Human01.w3m");
        if !map.is_file() {
            eprintln!("skip: Human01.w3m missing");
            return;
        }
        let mut game = Game::new(Assets::load(None, Some(&map)), 0x5EED);
        assert!(!game.melee, "a campaign map is not the melee arena");
        let engine = game.engine.as_ref().expect("map engine booted from map provenance");
        let spawn = engine.map_spawn();
        assert!(spawn.map_units > 0, "war3mapUnits.doo placements: {spawn:?}");
        assert_eq!(
            spawn.spawned_units + spawn.matched_units,
            spawn.map_units,
            "every placement spawned or matched: {spawn:?}"
        );
        assert!(game.bridge.is_some(), "JassBridge attached to the map runtime");
        let report = engine.script_report();
        assert!(report.main_ok, "war3map.j main ran in-game");
        assert!(report.executed_triggers > 0, "InitCustomTriggers executed");
        let before = engine.runtime().expect("runtime").world.game_time;
        game.update(TICK_SECONDS);
        let after = game
            .engine
            .as_ref()
            .unwrap()
            .runtime()
            .expect("runtime")
            .world
            .game_time;
        assert!(after > before, "Engine ticks inside Game::update: {before} -> {after}");
        assert!(!game.sim.units.is_empty(), "map units are live in the canonical sim");
    }

    /// Human04 is the shipped melee arena: its campaign script (which spawns
    /// three halls) is deliberately not booted and the canonical 1v1 opening
    /// owns every spawn.
    #[test]
    fn map_runtime_human04_is_melee() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
        let map = root.join("Maps/Campaign/Human04.w3m");
        if !map.is_file() {
            eprintln!("skip: Human04.w3m missing");
            return;
        }
        let game = Game::new(Assets::load(None, Some(&map)), 0x5EED);
        assert!(game.melee, "Human04 is the melee arena");
        assert!(game.engine.is_none(), "campaign script is not booted in melee");
        let halls = game
            .sim
            .alive_units()
            .filter(|unit| unit.def().main_hall)
            .count();
        assert_eq!(halls, 2, "one main hall per side: {:?}", game.sim.alive_units().filter(|u| u.def().main_hall).map(|u| (u.owner, u.def().id)).collect::<Vec<_>>());
        assert_eq!(game.sim.gold_mines.len(), 6, "2 start + 4 expansion mines");
        let creeps = game.sim.alive_units().filter(|unit| unit.owner == 2).count();
        assert_eq!(creeps, 30, "five creeps per mine (six camps)");
        for (owner, label) in [(game.player, "player"), (game.enemy, "enemy")] {
            let workers = game
                .sim
                .alive_units()
                .filter(|unit| unit.owner == owner && unit.def().stats.is_worker)
                .count();
            assert_eq!(workers, 5, "five workers for the {label}");
        }
    }

    #[test]
    fn wc3_command_hotkeys_are_attack_move_stop_hold_patrol() {
        use super::*;
        assert_eq!(hotkey_letter(UiAction::AttackMove), Some('a'));
        assert_eq!(hotkey_letter(UiAction::Move), Some('m'));
        assert_eq!(hotkey_letter(UiAction::Stop), Some('s'));
        assert_eq!(hotkey_letter(UiAction::Hold), Some('h'));
        assert_eq!(hotkey_letter(UiAction::Patrol), Some('p'));
        assert_eq!(hotkey_letter(UiAction::Gather), Some('g'));
        let letters: Vec<char> = [
            UiAction::AttackMove,
            UiAction::Move,
            UiAction::Stop,
            UiAction::Hold,
            UiAction::Patrol,
            UiAction::SelectArmy,
        ]
        .into_iter()
        .filter_map(hotkey_letter)
        .collect();
        let unique: std::collections::BTreeSet<char> = letters.iter().copied().collect();
        assert_eq!(unique.len(), 6, "duplicate command hotkey: {letters:?}");
    }

    // ------------------------------------------------------- UI flow / ghost
    // Ledger 402: the in-game UI behaves like WC3 — build ghost, anchored
    // tooltip, own toast band, real menu flow, army panel, rally marker.

    fn ghost_def() -> &'static str {
        // The first buildable structure the sim knows (a real def id, not a
        // test-only name): WATCH TOWER on the shipped roster.
        for candidate in ["hgtw", "otot", "hhou", "ogre"] {
            if let Some(def) = wc3_sim::def_of(candidate)
                && def.stats.is_building
            {
                return def.id;
            }
        }
        "hhou"
    }

    #[test]
    fn ui_ghost_draws_the_model_at_half_alpha_with_legal_tint() {
        let mut game = game();
        game.resize(2414.0, 1080.0);
        let worker = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player && unit.def().stats.is_worker)
            .map(|unit| unit.id)
            .expect("starting worker");
        game.selection = vec![worker];
        let def_id = ghost_def();
        game.begin_placement(def_id);
        let ghost = game.ghost().expect("ghost state");
        assert_eq!(ghost.def_id, def_id);
        assert!(
            game.log
                .iter()
                .any(|line| line.starts_with(&format!("GHOST def={def_id}"))),
            "begin_placement logs the ghost: {:?}",
            game.log
        );
        let scene = game.scene();
        let model = game.assets.model_index_for_def(def_id);
        let at = ghost.at;
        // The ghost is pushed after every live unit: take the last match.
        let draw = scene
            .units
            .iter()
            .rev()
            .find(|unit| {
                unit.model == model
                    && (unit.world.w_axis.x - at.x).abs() < 1.0
                    && (unit.world.w_axis.z - at.y).abs() < 1.0
            })
            .expect("ghost model in the scene at the pointer");
        let alpha = draw
            .mesh_alpha
            .iter()
            .copied()
            .fold(0.0f32, f32::max)
            .max(
                draw.layer_alpha
                    .iter()
                    .flat_map(|layers| layers.iter().copied())
                    .fold(0.0f32, f32::max),
            );
        assert!(
            (alpha - 0.5).abs() < 0.06,
            "ghost must be 50% alpha, measured {alpha}"
        );
        if ghost.legal {
            assert!(
                draw.tint[1] > draw.tint[0] + 0.2,
                "legal ghost is green: {:?}",
                draw.tint
            );
        } else {
            assert!(
                draw.tint[0] > draw.tint[1] + 0.2,
                "blocked ghost is red: {:?}",
                draw.tint
            );
        }
        assert!(
            scene.rings.iter().any(|ring| ring.radius > 40.0),
            "build radius ring is drawn"
        );

        // Moving onto unwalkable ground flips the ghost to red and logs it.
        let mut blocked_at = None;
        'blocked: for y in 0..game.sim.grid.height {
            for x in 0..game.sim.grid.width {
                if !game.sim.grid.walkable(x, y) {
                    blocked_at = Some(game.sim.grid.cell_to_world(x, y));
                    break 'blocked;
                }
            }
        }
        let blocked_at = blocked_at.expect("the map has blocked cells (trees/cliffs)");
        game.placing_at = Some(blocked_at);
        game.ghost_cell = None;
        game.log_ghost();
        let blocked = game.ghost().expect("ghost state");
        assert!(!blocked.legal, "a blocked cell is never legal");
        let scene = game.scene();
        // The ghost is pushed after every live unit: take the last match.
        let draw = scene
            .units
            .iter()
            .rev()
            .find(|unit| {
                unit.model == model
                    && (unit.world.w_axis.x - blocked.at.x).abs() < 1.0
                    && (unit.world.w_axis.z - blocked.at.y).abs() < 1.0
            })
            .expect("blocked ghost model");
        assert!(
            draw.tint[0] > draw.tint[1],
            "blocked ghost is red: tint={:?} at={:?} model={} legal={}",
            draw.tint,
            blocked.at,
            model,
            blocked.legal
        );
        assert!(
            game.log
                .iter()
                .any(|line| line.contains(&format!("GHOST def={def_id}")) && line.contains("legal=false")),
            "blocked ghost is logged: {:?}",
            game.log
        );

        // The placement tap logs the ghost at the tapped point.
        game.placing_at = Some(game.base_pos);
        game.begin_placement(def_id);
        game.log.clear();
        game.place_building(game.base_pos);
        assert!(
            game.log
                .iter()
                .any(|line| line.starts_with("GHOST def=") && line.contains("at=(")),
            "place_building logs the ghost: {:?}",
            game.log
        );
    }

    #[test]
    fn ui_menu_flow_boots_to_main_menu_and_reaches_battle() {
        let mut game = game();
        game.boot_menu = true;
        game.update(1.0 / 30.0);
        assert_eq!(game.screen, Screen::MainMenu, "retail boot opens the menu");
        assert_eq!(game.menu_flow.page, crate::hud::menu::MenuPage::Main);
        assert_eq!(
            game.menu_buttons.first().map(|button| button.label),
            Some("PLAY")
        );
        // PLAY → skirmish screen with the live race/opponent/map/difficulty.
        game.activate_menu(MENU_ACT_PLAY);
        assert_eq!(game.menu_flow.page, crate::hud::menu::MenuPage::SinglePlayer);
        assert!(
            game.menu_buttons
                .iter()
                .any(|button| button.label == "RACE" && button.sub == "HUMAN"),
            "skirmish shows the race value: {:?}",
            game.menu_buttons.iter().map(|b| (b.label, b.sub)).collect::<Vec<_>>()
        );
        // RACE picker → ORC sets the choice and returns to the skirmish page.
        game.activate_menu(MENU_ACT_RACE);
        assert_eq!(game.menu_flow.page, crate::hud::menu::MenuPage::RaceSelect);
        game.activate_menu(21); // race index 1 = ORC
        assert_eq!(game.menu_flow.page, crate::hud::menu::MenuPage::SinglePlayer);
        assert_eq!(game.menu_flow.race, crate::hud::menu::RaceChoice::Orc);
        // BACK walks up to the main menu.
        game.activate_menu(MENU_ACT_BACK);
        assert_eq!(game.menu_flow.page, crate::hud::menu::MenuPage::Main);
        // START GAME runs the loading screen and reaches the battle.
        game.activate_menu(MENU_ACT_PLAY);
        game.activate_menu(MENU_ACT_PLAY);
        assert_eq!(game.screen, Screen::Loading, "loading screen owns the start");
        for _ in 0..90 {
            game.update(1.0 / 30.0);
            if game.screen == Screen::Playing {
                break;
            }
        }
        assert_eq!(game.screen, Screen::Playing, "loading reaches the battle");
        assert_eq!(game.phase, Phase::Playing);
        assert_eq!(game.menu_flow.page, crate::hud::menu::MenuPage::InGame);
    }

    #[test]
    fn ui_game_menu_pauses_resumes_and_confirms_quit() {
        let mut game = game();
        game.resize(2414.0, 1080.0);
        // BACK in the battle with nothing to cancel opens the game menu.
        game.hotkey(Hotkey::Escape, false);
        assert_eq!(game.screen, Screen::Paused);
        assert!(
            game.menu_buttons.iter().any(|button| button.label == "RESUME"),
            "pause menu has RESUME: {:?}",
            game.menu_buttons.iter().map(|b| b.label).collect::<Vec<_>>()
        );
        assert!(game.menu_buttons.iter().any(|button| button.label == "RESTART"));
        // BACK again resumes the battle.
        game.hotkey(Hotkey::Escape, false);
        assert_eq!(game.screen, Screen::Playing);
        // F10 toggles the same menu.
        game.function_key(10);
        assert_eq!(game.screen, Screen::Paused);
        // F9 opens the quest log in battle.
        game.hotkey(Hotkey::Escape, false);
        assert_eq!(game.screen, Screen::Playing);
        game.function_key(9);
        assert_eq!(
            game.dialog,
            Some(crate::hud::mobile_hud::DialogScreen::QuestLog),
            "F9 opens the quest log"
        );
        // Escape closes the dialog first (WC3 order), then reopens the menu.
        game.hotkey(Hotkey::Escape, false);
        assert_eq!(game.dialog, None, "Escape closes the quest log");
        assert_eq!(game.screen, Screen::Playing);
        game.hotkey(Hotkey::Escape, false);
        assert_eq!(game.screen, Screen::Paused);
        // QUIT is a two-tap confirm: the first tap only arms it.
        game.activate_menu(MENU_ACT_QUIT);
        assert!(game.quit_armed);
        assert_eq!(game.screen, Screen::Paused, "one tap never exits");
        assert!(
            game.menu_buttons.iter().any(|button| button.label == "CONFIRM QUIT"),
            "armed quit relabels the button"
        );
        assert!(
            game.log.iter().any(|line| line.contains("GAME_QUIT armed")),
            "armed quit is logged: {:?}",
            game.log
        );
        assert!(
            game.toast_queue
                .iter()
                .any(|(text, _)| text.contains("TAP QUIT AGAIN")),
            "armed quit tells the player what the second tap does: {:?}",
            game.toast_queue
        );
    }

    #[test]
    fn ui_victory_screen_reports_stats_and_restarts() {
        let mut game = game();
        game.resize(2414.0, 1080.0);
        game.total_kills = 7;
        game.total_trained = 4;
        game.finish(game.player);
        assert_eq!(game.phase, Phase::Won);
        assert_eq!(game.screen, Screen::Victory, "victory is a real screen");
        assert_eq!(game.menu_title, "VICTORY");
        assert!(
            game.menu_subtitle.contains("KILLS 7") && game.menu_subtitle.contains("TRAINED 4"),
            "victory stats: {}",
            game.menu_subtitle
        );
        assert!(
            game.menu_buttons.iter().any(|button| button.label == "RESTART")
                && game.menu_buttons.iter().any(|button| button.label == "MAIN MENU"),
            "victory buttons: {:?}",
            game.menu_buttons.iter().map(|b| b.label).collect::<Vec<_>>()
        );
        // RESTART rebuilds the battle from the same map.
        game.activate_menu(MENU_ACT_RESTART);
        assert_eq!(game.phase, Phase::Playing);
        assert_eq!(game.screen, Screen::Loading, "restart runs the loading screen");
        for _ in 0..90 {
            game.update(1.0 / 30.0);
            if game.screen == Screen::Playing {
                break;
            }
        }
        assert_eq!(game.screen, Screen::Playing);
        // Defeat uses the same screen with the loss title.
        game.finish(game.enemy);
        assert_eq!(game.screen, Screen::Defeat);
        assert_eq!(game.menu_title, "DEFEAT");
        game.activate_menu(MENU_ACT_MENU);
        assert_eq!(game.screen, Screen::MainMenu);
    }

    #[test]
    fn ui_army_panel_lists_each_selected_unit_and_its_taps_select_it() {
        let mut game = game();
        game.resize(2414.0, 1080.0);
        let army: Vec<UnitId> = game
            .sim
            .alive_units()
            .filter(|unit| unit.owner == game.player && !unit.is_building())
            .map(|unit| unit.id)
            .take(3)
            .collect();
        assert!(army.len() >= 2, "starting army");
        game.selection = army.clone();
        game.refresh_buttons();
        let layout = game.mobile_layout.clone().expect("mobile layout");
        // The army band is one console tile: 95 px = 36 dp at the device
        // density (2.625), the operator's reflowed geometry.
        assert!(
            (layout.army.h - 95.0).abs() < 0.5,
            "army band is the 36 dp console tile: {:?}",
            layout.army
        );
        assert!(
            !layout.army_slots.is_empty(),
            "a non-empty selection must produce army hit slots: {:?}",
            layout.army_slots
        );
        let chip_side = layout.army_slots[0].w;
        for slot in &layout.army_slots {
            assert!(
                (slot.w - slot.h).abs() < 0.01,
                "each army chip hit rect is square: {slot:?}"
            );
            assert!(
                (slot.w - chip_side).abs() < 0.01,
                "one uniform chip side per page: {slot:?}"
            );
            assert!(
                slot.w >= layout.army.h - 1e-3,
                "a chip is never smaller than the 36 dp band: {slot:?}"
            );
        }
        // Pagination is the army panel's own contract: chips never shrink below
        // the touch floor, the row pages instead. Solve the panel with the
        // observed chip side and assert the HUD published exactly that plan.
        let plan_for = |count: usize| {
            crate::hud::army_panel::layout(
                layout.army,
                count,
                0,
                layout.scale,
                crate::hud::army_panel::TouchMetrics {
                    min_side: chip_side,
                },
            )
        };
        let plan = plan_for(army.len());
        assert_eq!(
            layout.army_slots, plan.hits,
            "the game's army hit row is the panel's paginated plan"
        );
        assert_eq!(
            plan.pages,
            army.len().div_ceil(plan.per_page.max(1)),
            "pages = ceil(count / per_page): {plan:?}"
        );
        assert_eq!(
            plan.per_page,
            3,
            "three 126 px touch chips fit the 423 px band: {plan:?}"
        );
        // Beyond `per_page` the row pages instead of shrinking or dropping.
        while game
            .sim
            .alive_units()
            .filter(|unit| unit.owner == game.player && !unit.is_building())
            .count()
            < 5
        {
            game.sim.spawn_def(
                game.player,
                "hfoo",
                game.base_pos + Vec2::new(240.0, 0.0),
            );
        }
        let wider: Vec<UnitId> = game
            .sim
            .alive_units()
            .filter(|unit| unit.owner == game.player && !unit.is_building())
            .map(|unit| unit.id)
            .take(5)
            .collect();
        assert!(wider.len() > plan.per_page, "need more units than one page");
        game.selection = wider.clone();
        game.refresh_buttons();
        let layout = game.mobile_layout.clone().expect("mobile layout");
        let paged = plan_for(wider.len());
        assert!(
            paged.pages > 1,
            "5 units at per_page={} page: {paged:?}",
            paged.per_page
        );
        assert_eq!(
            layout.army_slots.len(),
            paged.per_page,
            "one page of chips is published when the selection pages"
        );
        assert_eq!(layout.army_slots, paged.hits, "paged hit row is the plan");
        let army_buttons: Vec<_> = game
            .touch_buttons
            .iter()
            .filter(|button| button.index >= ARMY_TARGET_BASE && button.enabled)
            .collect();
        assert_eq!(
            army_buttons.len(),
            layout.army_slots.len(),
            "every visible chip is tappable"
        );
        // A tap on the first chip singles that unit out (page 0 maps slot n to
        // selection[n]).
        let slot = layout.army_slots[0];
        let centre = Vec2::new(slot.centre().x, slot.centre().y);
        game.pointer_down(centre);
        game.pointer_up(centre);
        assert_eq!(game.selection, vec![wider[0]], "army chip selects its unit");
        assert!(
            game.log.iter().any(|line| line.starts_with("ARMY_PICK slot=0")),
            "army pick is logged: {:?}",
            game.log
        );
    }

    #[test]
    fn sim_only_verbs_are_wired_and_named() {
        // Ledger 439 / audit §2.2/2.5/2.9: `research_upgrade`, `pickup_item`,
        // `buy_item`, `buy_unit` and `cancel_construction` had no game-layer
        // caller. The verbs are dispatched from `UiAction` now; the card rows
        // still live in the read-only HUD file, so each scenario names the gap
        // with one `UI_MISSING <verb>` line instead of a silent no-op.
        let mut game = game();
        for verb in [
            "research_upgrade",
            "pickup_item",
            "buy_item",
            "buy_unit",
            "cancel_construction",
        ] {
            let prefix = format!("UI_MISSING {verb}");
            assert!(
                game.log.iter().any(|line| line.starts_with(&prefix)),
                "missing {prefix}: {:?}",
                game.log
            );
        }

        // Research: the hall may research its race list; the handler queues it
        // and logs the transaction.
        let hall = game.player_hall().expect("player hall");
        game.sim.resources[game.player as usize] = 10_000;
        game.sim.lumber[game.player as usize] = 10_000;
        game.selection = vec![hall];
        game.activate_action(UiAction::Research("Rhpm"));
        assert!(
            game.log
                .iter()
                .any(|line| line == &format!("GAME_RESEARCH Rhpm building={}", hall.0)),
            "research reaches the sim: {:?}",
            game.log
        );

        // Cancel construction: a real site refunds its gold and lumber.
        let worker = game
            .sim
            .alive_units()
            .filter(|unit| unit.owner == game.player && unit.def().stats.is_worker)
            .map(|unit| unit.id)
            .next()
            .expect("a living worker");
        let base = game.sim.unit(hall).expect("hall").pos;
        let mut site = None;
        'spot: for ring in 0..24 {
            let radius = 260.0 + ring as f32 * 48.0;
            for step in 0..24 {
                let angle = step as f32 / 24.0 * std::f32::consts::TAU;
                let candidate = base + Vec2::new(angle.cos(), angle.sin()) * radius;
                if game.sim.place_denial(game.player, "hhou", candidate).is_none() {
                    site = Some(candidate);
                    break 'spot;
                }
            }
        }
        let site = site.expect("a legal farm site near the hall");
        let gold_before = game.sim.resources[game.player as usize];
        let lumber_before = game.sim.lumber[game.player as usize];
        let building = game
            .sim
            .place_building_ordered(worker, "hhou", site)
            .expect("farm site placed");
        assert!(game.sim.unit(building).is_some_and(|unit| unit.construct.is_some()));
        game.selection = vec![building];
        game.activate_action(UiAction::CancelConstruction);
        assert!(
            game.log.iter().any(|line| line
                == &format!("GAME_CANCEL_BUILD site={} refunded=true", building.0)),
            "cancel reaches the sim: {:?}",
            game.log
        );
        assert_eq!(
            game.sim.resources[game.player as usize], gold_before,
            "cancel refunds the full gold cost"
        );
        assert_eq!(
            game.sim.lumber[game.player as usize], lumber_before,
            "cancel refunds the full lumber cost"
        );
        assert!(
            game.sim
                .unit(building)
                .map(|unit| !unit.alive)
                .unwrap_or(true),
            "the cancelled site is gone"
        );
    }

    #[test]
    fn ui_rally_marker_is_drawn_for_a_selected_producer() {
        let mut game = game();
        let hall = game.player_hall().expect("player hall");
        game.selection = vec![hall];
        let rally = game.base_pos + Vec2::new(420.0, 180.0);
        game.sim.set_rally(hall, Some(rally));
        let scene = game.scene();
        let marker = scene
            .rings
            .iter()
            .find(|ring| {
                (ring.pos.x - rally.x).abs() < 1.0 && (ring.pos.z - rally.y).abs() < 1.0
            })
            .expect("rally ring at the mustered point");
        assert!(
            marker.color[2] > marker.color[0] && marker.color[2] > marker.color[1],
            "rally marker is the blue ghost ring: {:?}",
            marker.color
        );
        // The next production order musters there (sim-owned rally point).
        let unit = game.sim.unit(hall).expect("hall");
        assert_eq!(unit.rally, Some(rally));
    }


    #[test]
    fn ui_bounty_spawns_a_rising_label_and_logs_amount_and_position() {
        let mut game = game();
        game.resize(2414.0, 1080.0);
        let (victim, victim_pos) = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.enemy)
            .map(|unit| (unit.id, unit.pos))
            .expect("enemy unit");
        game.sim.events.push(Event::Bounty {
            owner: game.player,
            unit: victim,
            gold: 35,
        });
        game.process_events();
        assert!(
            game.log.iter().any(|line| {
                line.starts_with("GAME_BOUNTY amount=35 pos=(") && line.contains("owner=0")
            }),
            "bounty is logged with amount and position: {:?}",
            game.log
        );
        assert_eq!(game.floating.len(), 1, "one floating label");
        let label = &game.floating[0];
        assert_eq!(label.text, "+35 G");
        assert_eq!(label.pos, victim_pos);
        assert!(label.alpha() > 0.9, "label starts opaque");
        let start = label.lift();
        // The scene draws the label through `draw_fit` (debug builds assert the
        // fitted string stays inside the measured chip).
        let _ = game.scene();
        for _ in 0..20 {
            game.update(TICK_SECONDS);
        }
        let raised = game.floating.first().map(FloatingLabel::lift).unwrap_or(0.0);
        assert!(raised > start, "label rises: {start} -> {raised}");
        for _ in 0..30 {
            game.update(TICK_SECONDS);
        }
        assert!(game.floating.is_empty(), "label expires after 1.2 s");
    }

    #[test]
    fn ui_deposits_are_logged_without_an_invented_position() {
        let mut game = game();
        game.sim.events.push(Event::GoldDeposited {
            owner: game.player,
            amount: 10,
        });
        game.sim.events.push(Event::LumberDeposited {
            owner: game.player,
            amount: 20,
        });
        game.sim.events.push(Event::Bounty {
            owner: game.player,
            unit: UnitId(9999),
            gold: 5,
        });
        game.process_events();
        assert!(
            game.log.iter().any(|line| line == "GAME_DEPOSIT kind=gold amount=10"),
            "gold deposit is logged: {:?}",
            game.log
        );
        assert!(
            game.log.iter().any(|line| line == "GAME_DEPOSIT kind=lumber amount=20"),
            "lumber deposit is logged: {:?}",
            game.log
        );
        // A bounded position event cannot be invented: the sim had no unit, so
        // the label is skipped and the log says pos=none.
        assert!(game.floating.is_empty(), "no label without a position");
        assert!(
            game.log.iter().any(|line| line.contains("GAME_BOUNTY amount=5 pos=none")),
            "missing victim position is explicit: {:?}",
            game.log
        );
    }


    // -------------------------------------------------- fog / fog-of-war UI
    #[test]
    fn ui_fog_hides_enemy_models_but_keeps_visible_friendlies() {
        let mut game = game();
        game.resize(2414.0, 1080.0);
        let (enemy_id, enemy_pos) = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.enemy)
            .map(|unit| (unit.id, unit.pos))
            .expect("enemy unit");
        game.update(TICK_SECONDS);
        assert!(
            !game.sim.is_visible(game.player, enemy_pos),
            "the enemy start base is outside the player's sight"
        );
        let hidden_scene = game.scene();
        let hidden = hidden_scene.units.iter().any(|draw| {
            (draw.world.w_axis.x - enemy_pos.x).abs() < 2.0
                && (draw.world.w_axis.z - enemy_pos.y).abs() < 2.0
        });
        assert!(!hidden, "an enemy in fog must not produce a draw entry");
        // A friendly unit in sight still draws.
        let friend = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player)
            .map(|unit| unit.pos)
            .expect("player unit");
        let drawn_friend = hidden_scene.units.iter().any(|draw| {
            (draw.world.w_axis.x - friend.x).abs() < 2.0
                && (draw.world.w_axis.z - friend.y).abs() < 2.0
        });
        assert!(drawn_friend, "friendly units always draw");
        // Bring the enemy into sight: it becomes one more draw entry (the sim
        // may shove it one cell aside, so compare counts, not the exact point).
        let hidden_draws = hidden_scene.units.len();
        let target = game.base_pos + Vec2::new(90.0, 0.0);
        for unit in game.sim.units.iter_mut() {
            if unit.id == enemy_id {
                unit.pos = target;
            }
        }
        game.sim.step(&[]);
        assert!(
            game.sim.is_visible(game.player, target),
            "the enemy is now inside the player's sight"
        );
        let visible_scene = game.scene();
        assert!(
            visible_scene.units.len() > hidden_draws,
            "a visible enemy adds its model to the scene: {} -> {}",
            hidden_draws,
            visible_scene.units.len()
        );
    }

    #[test]
    fn ui_minimap_keeps_a_faded_last_seen_blip_in_explored_fog() {
        let mut game = game();
        game.resize(2414.0, 1080.0);
        // Put an enemy inside sight, tick the fog so it is recorded.
        let enemy = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.enemy)
            .map(|unit| unit.id)
            .expect("enemy unit");
        let wanted = game.base_pos + Vec2::new(80.0, 40.0);
        for unit in game.sim.units.iter_mut() {
            if unit.id == enemy {
                unit.pos = wanted;
            }
        }
        game.sim.step(&[]);
        // The sim may shove a unit one cell aside: the observed position is
        // whatever the unit actually had on the tick the fog was stamped.
        let seen_at = game.sim.unit(enemy).map(|unit| unit.pos).expect("enemy");
        let live = crate::hud::minimap_blips(&game);
        let world = game.assets.world_size;
        let expect = Vec2::new(seen_at.x / world.x, seen_at.y / world.y);
        assert!(
            live.iter().any(|blip| !blip.last_seen
                && blip.color[0] > 0.9
                && (blip.pos - expect).length() < 0.01),
            "a visible enemy is a live red blip at the observed cell"
        );
        // The enemy leaves AND the player's own units leave the area, so the
        // observed cell drops back to explored: the minimap must keep a faded
        // last-known contact there.
        let far = game.enemy_base;
        let home = Vec2::new(game.viewport.x * 0.0, 0.0);
        for unit in game.sim.units.iter_mut() {
            if unit.id == enemy {
                unit.pos = far;
            } else if unit.owner == game.player {
                unit.pos = home;
            }
        }
        game.sim.step(&[]);
        assert!(
            !game.sim.is_visible(game.player, seen_at),
            "the observed cell is no longer visible"
        );
        assert_eq!(
            game.sim.visibility_at(game.player, seen_at),
            wc3_sim::Visibility::Explored,
            "the observed cell is explored"
        );
        let blips = crate::hud::minimap_blips(&game);
        let last_seen = blips
            .iter()
            .find(|blip| blip.last_seen)
            .expect("last-known blip");
        assert!(
            last_seen.color[3] < 0.6,
            "last-known blip is faded: {:?}",
            last_seen.color
        );
        assert!(
            (last_seen.pos - expect).length() < 0.01,
            "last-known blip stays at the observed position: {:?} vs {expect:?}",
            last_seen.pos
        );
        assert!(
            !blips.iter().any(|blip| {
                !blip.last_seen
                    && blip.color[0] > 0.9
                    && blip.color[1] < 0.5
                    && (blip.pos - expect).length() < 0.01
            }),
            "no live enemy blip where the unit no longer is"
        );
    }



    #[test]
    fn ui_boot_menu_never_interrupts_an_input_driven_session() {
        // Retail: a quiet install session opens on the main menu…
        let mut quiet = game();
        quiet.boot_menu = true;
        quiet.update(1.0 / 30.0);
        assert_eq!(quiet.screen, Screen::MainMenu, "retail boot shows the menu");
        // …but a host that injects input before the first frame (headless
        // probes like gather_probe, device self-tests) drives the battle.
        let mut driven = game();
        driven.resize(2414.0, 1080.0);
        driven.boot_menu = true;
        let pos = Vec2::new(driven.viewport.x * 0.5, driven.layout.top_bar.y - 200.0);
        driven.pointer_down(pos);
        driven.pointer_up(pos);
        driven.update(1.0 / 30.0);
        assert_eq!(
            driven.screen,
            Screen::Playing,
            "input before the first frame skips the glue screens"
        );
        assert_eq!(driven.phase, Phase::Playing);
    }

}
