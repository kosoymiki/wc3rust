//! Single-player menu: typed screen model, navigation and the §8 command
//! contract (HUD spec §4 + §8).
//!
//! # One model, one list
//!
//! The screen is derived from state **once**: [`MenuState::model`] returns a
//! [`MenuScreenModel`] whose `items` list is the only description of what the
//! screen offers. Rendering and hit-testing both read that list — the host
//! projects it into the draw groups (`entries`, `tabs`, `rows`, `nav`) in the
//! same order, so an index in the layout is the index in the model. There is no
//! second hard-coded button table and no per-page action arithmetic in the
//! glue layer.
//!
//! Every item carries:
//!
//! * a stable [`ItemId`] (role + position),
//! * its [`MenuItemRole`] (draw group),
//! * [`MenuItemState`] (enabled, selected, disabled reason),
//! * a [`CommandSpec`] — `CommandId`, legacy wire code, target type, enabled
//!   state with an explicit reason, and the command's cooldown,
//! * a declarative [`Transition`] (what the host or the reducer does next).
//!
//! # §4 screens
//!
//! Main, single-player skirmish setup, campaign chapters (group tabs, paged
//! rows, locked/cleared/open state), custom-game map list (paged), settings
//! (audio categories with a draft/applied split and apply/cancel), pause, save
//! and load (slot state, overwrite confirmation, empty/unreadable/unsupported
//! distinctions) and victory/defeat. Every screen has a typed back target
//! ([`BackAction`]); settings opened from the pause menu return to the pause
//! menu, not to main.
//!
//! # §8 command contract
//!
//! [`MenuCommandGate`] is the command-level admission gate. It rejects a
//! mirrored duplicate of the previous physical gesture (same point within
//! 300 ms — even when the screen changed and the same point now names a
//! *different* row) and a re-issued command inside its declared cooldown. The
//! gesture-level rules (press arming, drag-off cancel, one activation per
//! press, priority modal > menu > HUD > world) live in
//! `wc3_input::menu`.
//!
//! # Compatibility
//!
//! [`MenuPage`], [`MenuAction`], [`MenuState`] and the legacy action constants
//! stay exactly as the glue layer and the frozen tests know them; the typed
//! model is additive. The `legacy` code on every command is the same number the
//! device log prints (`GAME_MENU action=<code>`), so a device driver, a log
//! line and a model item all name the same action.

use std::fmt;

use glam::Vec2;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum RaceChoice {
    Human,
    Orc,
    Undead,
    NightElf,
    Random,
}

impl RaceChoice {
    pub fn label(self) -> &'static str {
        match self {
            Self::Human => "HUMAN",
            Self::Orc => "ORC",
            Self::Undead => "UNDEAD",
            Self::NightElf => "NIGHT ELF",
            Self::Random => "RANDOM",
        }
    }

    pub fn index(self) -> usize {
        RACES.iter().position(|race| *race == self).unwrap_or(0)
    }
}

pub const RACES: [RaceChoice; 5] = [
    RaceChoice::Human,
    RaceChoice::Orc,
    RaceChoice::Undead,
    RaceChoice::NightElf,
    RaceChoice::Random,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Difficulty {
    Easy,
    Normal,
    Hard,
}

impl Difficulty {
    pub fn label(self) -> &'static str {
        match self {
            Self::Easy => "EASY",
            Self::Normal => "NORMAL",
            Self::Hard => "HARD",
        }
    }

    pub fn index(self) -> usize {
        DIFFICULTIES
            .iter()
            .position(|level| *level == self)
            .unwrap_or(1)
    }
}

pub const DIFFICULTIES: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapChoice {
    pub name: String,
    pub path: String,
    pub players: u8,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuPage {
    Main,
    /// Single-player campaign: the 44 retail chapters, grouped by campaign.
    Campaign,
    /// Custom game: the melee map registry with race/difficulty pickers.
    CustomGame,
    SinglePlayer,
    RaceSelect,
    OpponentSelect,
    MapSelect,
    Difficulty,
    Loading,
    InGame,
    Paused,
    Options,
    SaveLoad,
}

/// Retail campaign groups of the RoC+TFT chapter list, in menu order.
///
/// `Extra` holds `BonusCredits`, `WarCraftIIICredits` and any chapter whose id
/// does not match a race prefix — those maps are reachable in the same list,
/// never hidden behind a filter the player cannot see.
pub const CAMPAIGN_GROUPS: [&str; 6] = ["PROLOGUE", "HUMAN", "UNDEAD", "ORC", "NIGHT ELF", "EXTRA"];

/// Chapter select rows per page. Five keeps every row at or above the 90 px
/// finger floor next to the campaign tabs and the page nav on a 1080p phone.
pub const CAMPAIGN_ROWS_PER_PAGE: usize = 5;
/// Custom-game rows per page (the 38-map registry is paginated the same way).
pub const MAP_ROWS_PER_PAGE: usize = 5;

/// Chapter state as the campaign screen shows it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CampaignState {
    Cleared,
    Unlocked,
    Locked,
}

impl CampaignState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cleared => "CLEARED",
            Self::Unlocked => "OPEN",
            Self::Locked => "LOCKED",
        }
    }

    fn enabled(self) -> bool {
        self != Self::Locked
    }
}

/// One campaign chapter row, as the menu needs it (no filesystem paths: the
/// registry resolves the map when the row is tapped).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CampaignEntry {
    /// Chapter index inside the campaign registry (`start_campaign_mission`).
    pub index: usize,
    pub id: String,
    pub title: String,
    /// Chapter number inside its campaign (`Human03` -> 3); 0 for interludes.
    pub number: u32,
    /// Index into [`CAMPAIGN_GROUPS`].
    pub group: usize,
    pub state: CampaignState,
    /// Cutscene-only chapter (`Human02Interlude`): drawn with a marker.
    pub interlude: bool,
}

impl MenuPage {
    pub fn title(self) -> &'static str {
        match self {
            Self::Main => "WARCRAFT III",
            Self::Campaign => "CAMPAIGN",
            Self::CustomGame => "CUSTOM GAME",
            Self::SinglePlayer => "SINGLE PLAYER",
            Self::RaceSelect => "SELECT RACE",
            Self::OpponentSelect => "SELECT OPPONENT",
            Self::MapSelect => "SELECT MAP",
            Self::Difficulty => "DIFFICULTY",
            Self::Loading => "LOADING",
            Self::InGame => "GAME",
            Self::Paused => "GAME PAUSED",
            Self::Options => "OPTIONS",
            Self::SaveLoad => "SAVE / LOAD",
        }
    }
}

/// Legacy reducer actions. The glue layer and the frozen tests use this enum;
/// the typed command path ([`MenuState::dispatch`]) maps a [`CommandId`] onto
/// the same transitions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuAction {
    OpenSinglePlayer,
    SelectRace(RaceChoice),
    SelectOpponent(RaceChoice),
    SelectMap(usize),
    SelectDifficulty(Difficulty),
    /// Set a value and stay on the skirmish screen (the retail "cycle" fields).
    SetRace(RaceChoice),
    SetOpponent(RaceChoice),
    SetMap(usize),
    SetDifficulty(Difficulty),
    /// Jump to a page (menu buttons that open a picker).
    Goto(MenuPage),
    StartGame,
    AdvanceLoading,
    Back,
    Pause,
    Resume,
    Restart,
    OpenOptions,
    OpenSaveLoad,
    SaveSlot(u8),
    LoadSlot(u8),
    QuitToMenu,
    // §4.4 settings (draft/applied) and §4.6 save/load screen state.
    SettingsTab(usize),
    SettingsSelect(SettingKey),
    SettingsStep(SettingKey, i8),
    ApplySettings,
    CancelSettings,
    SaveLoadMode(SaveLoadMode),
    SaveLoadSelect(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveStatus {
    pub available: bool,
    pub slots: u8,
    pub reason: Option<String>,
}

impl SaveStatus {
    /// `Sim` exposes no save/load yet; the menu must say so explicitly.
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            slots: 0,
            reason: Some(reason.into()),
        }
    }

    pub fn available(slots: u8) -> Self {
        Self {
            available: true,
            slots,
            reason: None,
        }
    }
}

// ---------------------------------------------------------------------------
// §4.7 navigation model
// ---------------------------------------------------------------------------

/// §4.7 `ScreenId`: the typed identity of a menu screen, including the two
/// end-of-match screens that are host states rather than [`MenuPage`] values.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ScreenId {
    Main,
    Campaign,
    CustomGame,
    SinglePlayer,
    RaceSelect,
    OpponentSelect,
    MapSelect,
    Difficulty,
    Loading,
    InGame,
    Paused,
    Options,
    SaveLoad,
    Victory,
    Defeat,
}

impl ScreenId {
    pub fn page(self) -> Option<MenuPage> {
        Some(match self {
            Self::Main => MenuPage::Main,
            Self::Campaign => MenuPage::Campaign,
            Self::CustomGame => MenuPage::CustomGame,
            Self::SinglePlayer => MenuPage::SinglePlayer,
            Self::RaceSelect => MenuPage::RaceSelect,
            Self::OpponentSelect => MenuPage::OpponentSelect,
            Self::MapSelect => MenuPage::MapSelect,
            Self::Difficulty => MenuPage::Difficulty,
            Self::Loading => MenuPage::Loading,
            Self::InGame => MenuPage::InGame,
            Self::Paused => MenuPage::Paused,
            Self::Options => MenuPage::Options,
            Self::SaveLoad => MenuPage::SaveLoad,
            Self::Victory | Self::Defeat => return None,
        })
    }

    pub fn from_page(page: MenuPage) -> Self {
        match page {
            MenuPage::Main => Self::Main,
            MenuPage::Campaign => Self::Campaign,
            MenuPage::CustomGame => Self::CustomGame,
            MenuPage::SinglePlayer => Self::SinglePlayer,
            MenuPage::RaceSelect => Self::RaceSelect,
            MenuPage::OpponentSelect => Self::OpponentSelect,
            MenuPage::MapSelect => Self::MapSelect,
            MenuPage::Difficulty => Self::Difficulty,
            MenuPage::Loading => Self::Loading,
            MenuPage::InGame => Self::InGame,
            MenuPage::Paused => Self::Paused,
            MenuPage::Options => Self::Options,
            MenuPage::SaveLoad => Self::SaveLoad,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Victory => "VICTORY",
            Self::Defeat => "DEFEAT",
            other => other.page().map(MenuPage::title).unwrap_or("MENU"),
        }
    }
}

/// Which host screen is on top. Mirrors `game::Screen` without importing it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum HostScreen {
    #[default]
    MainMenu,
    Loading,
    Paused,
    Victory,
    Defeat,
}

/// The only navigation verbs. A screen change always goes through
/// [`MenuState::navigate`], so a second tap on the current screen is a no-op
/// instead of a duplicate transition (§4.7).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NavigationAction {
    Goto(ScreenId),
    Back,
    Home,
}

/// The resolved back target of a screen: the "previous logical screen", not a
/// hard-coded main menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BackAction {
    pub from: ScreenId,
    pub to: Option<ScreenId>,
    /// The host owns the back (pause → resume, victory → main, main → quit).
    pub host_owned: bool,
}

/// Outcome of one navigation request.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TransitionResult {
    pub action: NavigationAction,
    pub from: ScreenId,
    pub to: ScreenId,
    pub accepted: bool,
    pub rejected: Option<RejectReason>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RejectReason {
    /// A second tap on the screen that is already active.
    AlreadyOnScreen,
    /// The screen has no model-side parent; the host handles the key.
    HostOwned,
    /// Changing screens is not allowed while loading.
    LoadingInProgress,
}

/// §4.7 screen state: the page plus the two "opened from" back links. A screen
/// that later gains a second parent extends [`MenuState::back_action`] — the
/// only place back targets are defined.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScreenState {
    pub page: MenuPage,
    pub options_from: MenuPage,
    pub save_from: MenuPage,
}

// ---------------------------------------------------------------------------
// §8.1 command contract
// ---------------------------------------------------------------------------

/// What a command acts on (§8.1). The UI never guesses an availability reason
/// from the label; the model publishes the reason with the command.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum TargetType {
    Screen,
    Row,
    Tab,
    Page,
    Slot,
    Setting,
    Confirm,
    Process,
    Field,
}

/// Typed, stable command identity. The same id is used by the model, the
/// reducer, the hit test and the device log line.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum CommandId {
    Continue,
    OpenSinglePlayer,
    OpenCampaign,
    OpenCustomGame,
    LoadGame,
    OpenSettings,
    OpenSaveLoad,
    Quit,
    ExitProcess,
    Resume,
    SaveGame,
    Restart,
    Back,
    StartGame,
    Play,
    OpenRacePicker,
    OpenOpponentPicker,
    OpenMapPicker,
    OpenDifficultyPicker,
    PickRace(RaceChoice),
    PickOpponent(RaceChoice),
    PickDifficulty(Difficulty),
    CampaignTab(u8),
    ChapterRow(u8),
    CampaignPrev,
    CampaignNext,
    MapRow(u8),
    MapPrev,
    MapNext,
    SaveMode,
    LoadMode,
    SaveSlot(u8),
    LoadSlot(u8),
    SettingsTab(u8),
    SettingsSelect(SettingKey),
    SettingStep(SettingKey, i8),
    ApplySettings,
    CancelSettings,
    MainMenu,
}

impl CommandId {
    pub fn label(self) -> &'static str {
        match self {
            Self::Continue => "continue",
            Self::OpenSinglePlayer => "single-player",
            Self::OpenCampaign => "campaign",
            Self::OpenCustomGame => "custom-game",
            Self::LoadGame => "load",
            Self::OpenSettings => "options",
            Self::OpenSaveLoad => "save-load",
            Self::Quit => "quit",
            Self::ExitProcess => "exit",
            Self::Resume => "resume",
            Self::SaveGame => "save",
            Self::Restart => "restart",
            Self::Back => "back",
            Self::StartGame => "start-game",
            Self::Play => "play",
            Self::OpenRacePicker => "race",
            Self::OpenOpponentPicker => "opponent",
            Self::OpenMapPicker => "map",
            Self::OpenDifficultyPicker => "difficulty",
            Self::PickRace(_) => "set-race",
            Self::PickOpponent(_) => "set-opponent",
            Self::PickDifficulty(_) => "set-difficulty",
            Self::CampaignTab(_) => "campaign-tab",
            Self::ChapterRow(_) => "chapter",
            Self::CampaignPrev => "campaign-prev",
            Self::CampaignNext => "campaign-next",
            Self::MapRow(_) => "map-row",
            Self::MapPrev => "map-prev",
            Self::MapNext => "map-next",
            Self::SaveMode => "save-mode",
            Self::LoadMode => "load-mode",
            Self::SaveSlot(_) => "save-slot",
            Self::LoadSlot(_) => "load-slot",
            Self::SettingsTab(_) => "settings-tab",
            Self::SettingsSelect(_) => "settings-select",
            Self::SettingStep(_, _) => "settings-step",
            Self::ApplySettings => "apply-settings",
            Self::CancelSettings => "cancel-settings",
            Self::MainMenu => "main-menu",
        }
    }
}

/// §8.1 one command: what it does, whether it is available, why not, and how
/// long before it can be issued again.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandSpec {
    pub id: CommandId,
    /// Wire code the device driver and `GAME_MENU action=` line use.
    pub legacy: u8,
    pub target: TargetType,
    pub enabled: bool,
    pub disabled_reason: Option<String>,
    /// §8.2: identical command issued again inside this window is dropped.
    pub cooldown_s: f32,
}

impl CommandSpec {
    pub fn new(id: CommandId, legacy: u8, target: TargetType) -> Self {
        Self {
            id,
            legacy,
            target,
            enabled: true,
            disabled_reason: None,
            cooldown_s: NAV_COOLDOWN_S,
        }
    }

    pub fn disabled(mut self, reason: impl Into<String>) -> Self {
        self.enabled = false;
        self.disabled_reason = Some(reason.into());
        self
    }

    pub fn cooldown(mut self, seconds: f32) -> Self {
        self.cooldown_s = seconds.max(0.0);
        self
    }
}

/// Command cooldown for ordinary navigation.
pub const NAV_COOLDOWN_S: f32 = 0.20;
/// Command cooldown for the two-tap confirmations (quit, overwrite). The
/// mirrored duplicate is already rejected by the point-based guard, so this
/// only has to be longer than the mirror window to keep one tap from arming and
/// confirming in a single physical gesture.
pub const CONFIRM_COOLDOWN_S: f32 = 0.35;

/// What the item asks for next. Pure UI transitions are executed by the reducer;
/// host transitions (`StartGame`, `Save`, `Load`, `Quit`, `Resume`, …) are
/// returned to the host, which owns the simulation and the audio engine.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Transition {
    None,
    Goto(MenuPage),
    Back,
    StartGame,
    Continue,
    Save(u8),
    Load(u8),
    Restart,
    Resume,
    QuitToMenu,
    ArmQuit,
    ExitProcess,
    SetRace(RaceChoice),
    SetOpponent(RaceChoice),
    SetDifficulty(Difficulty),
    SelectMap(usize),
    CampaignTab(usize),
    CampaignPage(i8),
    MapPage(i8),
    StartChapter(u8),
    SettingsTab(usize),
    SelectSetting(SettingKey),
    StepSetting(SettingKey, i8),
    ApplySettings,
    CancelSettings,
    SaveSlot(u8),
    LoadSlot(u8),
}

// ---------------------------------------------------------------------------
// §1.2 one typed item list
// ---------------------------------------------------------------------------

/// Draw group of an item, in draw order. The numeric values match the HUD's
/// `MENU_GROUP_*` constants so the host can project without a second enum.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MenuItemRole {
    Entry = 0,
    Tab = 1,
    Row = 2,
    Nav = 3,
}

impl MenuItemRole {
    pub fn group(self) -> u8 {
        self as u8
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Entry => "entry",
            Self::Tab => "tab",
            Self::Row => "row",
            Self::Nav => "nav",
        }
    }
}

/// Stable identity of an item inside its screen: role + position in that role.
/// The semantic identity is [`CommandSpec::id`]; this is the handle the hit test
/// and the pressed-state rendering use.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct ItemId(u16);

impl ItemId {
    pub fn new(role: MenuItemRole, index: usize) -> Self {
        Self(((role.group() as u16) << 12) | (index as u16 & 0x0FFF))
    }

    pub fn role(self) -> MenuItemRole {
        match (self.0 >> 12) as u8 {
            1 => MenuItemRole::Tab,
            2 => MenuItemRole::Row,
            3 => MenuItemRole::Nav,
            _ => MenuItemRole::Entry,
        }
    }

    pub fn index(self) -> usize {
        (self.0 & 0x0FFF) as usize
    }

    pub fn code(self) -> u16 {
        self.0
    }
}

/// §1.2 item state. `enabled` and `selected` are model facts; pressed/hover are
/// host input overlays and stay in the view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItemState {
    pub enabled: bool,
    pub selected: bool,
    pub reason: Option<String>,
}

impl MenuItemState {
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            selected: false,
            reason: None,
        }
    }

    pub fn selected() -> Self {
        Self {
            enabled: true,
            selected: true,
            reason: None,
        }
    }

    pub fn disabled(reason: impl Into<String>) -> Self {
        Self {
            enabled: false,
            selected: false,
            reason: Some(reason.into()),
        }
    }
}

/// One row of the single menu list.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    pub id: ItemId,
    pub role: MenuItemRole,
    pub label: String,
    pub sub: String,
    pub state: MenuItemState,
    pub command: CommandSpec,
    pub transition: Transition,
}

impl MenuItem {
    fn new(
        role: MenuItemRole,
        index: usize,
        label: impl Into<String>,
        sub: impl Into<String>,
        state: MenuItemState,
        command: CommandSpec,
        transition: Transition,
    ) -> Self {
        let command = if state.enabled {
            command
        } else {
            let reason = state
                .reason
                .clone()
                .unwrap_or_else(|| "UNAVAILABLE".to_string());
            command.disabled(reason)
        };
        Self {
            id: ItemId::new(role, index),
            role,
            label: label.into(),
            sub: sub.into(),
            state,
            command,
            transition,
        }
    }

    pub fn enabled(&self) -> bool {
        self.state.enabled && self.command.enabled
    }

    pub fn reason(&self) -> Option<&str> {
        self.command
            .disabled_reason
            .as_deref()
            .or(self.state.reason.as_deref())
    }
}

// ---------------------------------------------------------------------------
// Settings (§4.4)
// ---------------------------------------------------------------------------

/// The audio parameters the engine really exposes (`set_master_gain` /
/// `set_category_gain`). No graphics/control rows are invented: a build that
/// cannot change a value must not draw a control for it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum SettingKey {
    Master,
    Music,
    Effects,
}

pub const SETTING_KEYS: [SettingKey; 3] =
    [SettingKey::Master, SettingKey::Music, SettingKey::Effects];

impl SettingKey {
    pub fn label(self) -> &'static str {
        match self {
            Self::Master => "MASTER",
            Self::Music => "MUSIC",
            Self::Effects => "EFFECTS",
        }
    }

    pub fn index(self) -> usize {
        SETTING_KEYS
            .iter()
            .position(|key| *key == self)
            .unwrap_or(0)
    }
}

/// Audio values in percent (the UI unit). The host maps them to engine gain:
/// `percent / 100`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AudioSettings {
    pub master: u8,
    pub music: u8,
    pub effects: u8,
}

impl AudioSettings {
    pub const FULL: Self = Self {
        master: 100,
        music: 80,
        effects: 100,
    };

    pub fn get(self, key: SettingKey) -> u8 {
        match key {
            SettingKey::Master => self.master,
            SettingKey::Music => self.music,
            SettingKey::Effects => self.effects,
        }
    }

    pub fn set(&mut self, key: SettingKey, value: u8) {
        let value = value.min(100);
        match key {
            SettingKey::Master => self.master = value,
            SettingKey::Music => self.music = value,
            SettingKey::Effects => self.effects = value,
        }
    }

    /// One UI step. Returns `true` when the value changed (0 and 100 clamp).
    pub fn step(&mut self, key: SettingKey, delta: i8) -> bool {
        let before = self.get(key);
        let next = (before as i16 + delta as i16 * 10).clamp(0, 100) as u8;
        self.set(key, next);
        next != before
    }

    pub fn gain(self, key: SettingKey) -> f32 {
        self.get(key) as f32 / 100.0
    }
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self::FULL
    }
}

/// Draft/applied split (§4.4): edits update the draft, APPLY commits, CANCEL and
/// BACK discard. Switching screens never commits implicitly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsState {
    pub category: usize,
    pub selected: SettingKey,
    pub draft: AudioSettings,
    pub applied: AudioSettings,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            category: 0,
            selected: SettingKey::Master,
            draft: AudioSettings::FULL,
            applied: AudioSettings::FULL,
        }
    }
}

impl SettingsState {
    pub fn dirty(&self) -> bool {
        self.draft != self.applied
    }

    pub fn select(&mut self, key: SettingKey) {
        self.selected = key;
    }

    pub fn step(&mut self, key: SettingKey, delta: i8) -> bool {
        self.draft.step(key, delta)
    }

    /// Commit the draft. Returns the new value the host must push to the audio
    /// engine, or `None` when there was nothing to change.
    pub fn apply(&mut self) -> Option<AudioSettings> {
        if !self.dirty() {
            return None;
        }
        self.applied = self.draft;
        Some(self.applied)
    }

    pub fn cancel(&mut self) {
        self.draft = self.applied;
    }

    /// Adopt the engine's current gains as both applied and draft.
    pub fn adopt(&mut self, current: AudioSettings) {
        self.applied = current;
        self.draft = current;
    }
}

// ---------------------------------------------------------------------------
// Save/load (§4.6)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum SaveLoadMode {
    Save,
    Load,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum SaveSlotKind {
    Quick,
    Autosave,
}

impl SaveSlotKind {
    pub fn slot(self) -> u8 {
        match self {
            Self::Quick => 1,
            Self::Autosave => 1,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Quick => "SLOT 1",
            Self::Autosave => "AUTOSAVE",
        }
    }
}

/// §4.6 must distinguish an empty slot, an unreadable one and an unsupported
/// format. The host reads the sidecars; the model only presents the verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveSlotState {
    Empty,
    Ready {
        map: String,
        tick: u32,
        saved_at: Option<String>,
    },
    Unreadable {
        reason: String,
    },
    Unsupported {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveSlotInfo {
    pub kind: SaveSlotKind,
    pub state: SaveSlotState,
}

impl SaveSlotInfo {
    pub fn empty(kind: SaveSlotKind) -> Self {
        Self {
            kind,
            state: SaveSlotState::Empty,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveLoadState {
    pub mode: SaveLoadMode,
    pub selected: usize,
    pub pending_overwrite: Option<SaveSlotKind>,
}

impl Default for SaveLoadState {
    fn default() -> Self {
        Self {
            mode: SaveLoadMode::Save,
            selected: 0,
            pending_overwrite: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime facts (owned by the host, read by the model)
// ---------------------------------------------------------------------------

/// Everything the model needs that does not belong to the menu's own state:
/// campaign registry rows, save sidecar verdicts, the quit arm and the engine's
/// current audio gains. The model stays pure — it never reads a file or a clock.
#[derive(Clone, Debug, Default)]
pub struct MenuContext {
    pub host: HostScreen,
    pub map_name: String,
    pub chapters: Vec<CampaignEntry>,
    /// Number of chapters in the install (the list may be filtered or paged).
    pub chapter_count: usize,
    pub has_save: bool,
    pub save_api: bool,
    pub save_reason: Option<String>,
    /// Quick save (slot 1) first, autosave second, when present.
    pub slots: Vec<SaveSlotInfo>,
    pub quit_armed: bool,
    pub loading_progress: f32,
    pub save_loading: bool,
    pub end_stats: Option<String>,
    /// Engine gains when the host has an audio engine; `None` keeps the model's
    /// own applied values.
    pub audio: Option<AudioSettings>,
}

impl Default for RaceChoice {
    fn default() -> Self {
        RaceChoice::Human
    }
}

impl Default for Difficulty {
    fn default() -> Self {
        Difficulty::Normal
    }
}

impl MenuContext {
    pub fn new(host: HostScreen) -> Self {
        Self {
            host,
            ..Self::default()
        }
    }

    pub fn slot(&self, kind: SaveSlotKind) -> Option<&SaveSlotInfo> {
        self.slots.iter().find(|slot| slot.kind == kind)
    }

    pub fn quick_ready(&self) -> bool {
        matches!(
            self.slot(SaveSlotKind::Quick).map(|slot| &slot.state),
            Some(SaveSlotState::Ready { .. })
        )
    }

    pub fn chapter_count(&self) -> usize {
        self.chapter_count.max(self.chapters.len())
    }

    pub fn group_count(&self, group: usize) -> usize {
        self.chapters
            .iter()
            .filter(|row| row.group == group)
            .count()
    }
}

// ---------------------------------------------------------------------------
// The model
// ---------------------------------------------------------------------------

/// The single deterministic screen description. `items` is ordered in draw
/// order; projecting it by [`MenuItemRole`] preserves that order, so the rect
/// index and the model index are the same.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuScreenModel {
    pub screen: ScreenId,
    pub title: String,
    pub subtitle: String,
    pub page_label: String,
    pub items: Vec<MenuItem>,
    pub message: Option<String>,
    pub progress: f32,
    pub veil: f32,
    pub active_tab: usize,
    pub back: BackAction,
}

impl MenuScreenModel {
    pub fn items_for(&self, role: MenuItemRole) -> Vec<&MenuItem> {
        self.items.iter().filter(|item| item.role == role).collect()
    }

    pub fn by_command(&self, id: CommandId) -> Option<&MenuItem> {
        self.items.iter().find(|item| item.command.id == id)
    }

    /// The item a device tap names. `legacy` is the `GAME_MENU action=` code.
    pub fn by_legacy(&self, legacy: u8) -> Option<&MenuItem> {
        self.items.iter().find(|item| item.command.legacy == legacy)
    }

    pub fn item(&self, id: ItemId) -> Option<&MenuItem> {
        self.items.iter().find(|item| item.id == id)
    }

    /// Deterministic FNV-1a over the structural fields. Two calls with the same
    /// state and facts must produce the same fingerprint (headless proof).
    pub fn fingerprint(&self) -> u64 {
        fn feed(hash: &mut u64, value: u64) {
            *hash ^= value;
            *hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        feed(&mut hash, self.screen as u64);
        for item in &self.items {
            feed(&mut hash, item.id.code() as u64);
            feed(&mut hash, item.command.legacy as u64);
            feed(&mut hash, item.enabled() as u64);
            feed(&mut hash, item.state.selected as u64);
            feed(&mut hash, item.label.len() as u64);
            for byte in item.label.bytes().chain(item.sub.bytes()) {
                feed(&mut hash, byte as u64);
            }
        }
        hash
    }
}

impl MenuState {
    /// Build the one screen list from the current state and the host facts.
    ///
    /// Pure: no clock, no filesystem, no RNG. Calling it twice with the same
    /// arguments returns an equal model (and an equal [`MenuScreenModel::fingerprint`]).
    pub fn model(&self, facts: &MenuContext) -> MenuScreenModel {
        let (screen, title, subtitle, page_label, items, message, progress, veil, _active_tab) =
            match self.effective_screen(facts) {
                ScreenId::Main => self.model_main(facts),
                ScreenId::SinglePlayer => self.model_single_player(facts),
                ScreenId::Campaign => self.model_campaign(facts),
                ScreenId::CustomGame => self.model_custom_game(facts),
                ScreenId::RaceSelect => self.model_race(facts),
                ScreenId::OpponentSelect => self.model_opponent(facts),
                ScreenId::MapSelect => self.model_map_select(facts),
                ScreenId::Difficulty => self.model_difficulty(facts),
                ScreenId::Options => self.model_options(facts),
                ScreenId::SaveLoad => self.model_save_load(facts),
                ScreenId::Paused => self.model_pause(facts),
                ScreenId::Victory => self.model_end(facts, true),
                ScreenId::Defeat => self.model_end(facts, false),
                ScreenId::Loading | ScreenId::InGame => self.model_loading(facts),
            };
        MenuScreenModel {
            screen,
            title,
            subtitle,
            page_label,
            items,
            message,
            progress,
            veil,
            active_tab: self.campaign_tab,
            back: self.back_action(),
        }
    }

    fn effective_screen(&self, facts: &MenuContext) -> ScreenId {
        match facts.host {
            HostScreen::Victory => ScreenId::Victory,
            HostScreen::Defeat => ScreenId::Defeat,
            HostScreen::Loading => ScreenId::Loading,
            HostScreen::Paused => match self.page {
                MenuPage::Options => ScreenId::Options,
                MenuPage::SaveLoad => ScreenId::SaveLoad,
                _ => ScreenId::Paused,
            },
            HostScreen::MainMenu => ScreenId::from_page(self.page),
        }
    }

    fn model_main(
        &self,
        facts: &MenuContext,
    ) -> (
        ScreenId,
        String,
        String,
        String,
        Vec<MenuItem>,
        Option<String>,
        f32,
        f32,
        usize,
    ) {
        let mut items = Vec::new();
        let mut index = 0;
        // §4.1: CONTINUE only with a resumable state; LOAD only when loading
        // works. Multiplayer is absent from this build and is not drawn.
        if facts.has_save {
            items.push(MenuItem::new(
                MenuItemRole::Entry,
                index,
                "CONTINUE",
                "SAVE",
                MenuItemState::enabled(),
                CommandSpec::new(CommandId::Continue, 16, TargetType::Process)
                    .cooldown(CONFIRM_COOLDOWN_S),
                Transition::Continue,
            ));
            index += 1;
        }
        let chapters = facts.chapter_count();
        items.push(MenuItem::new(
            MenuItemRole::Entry,
            index,
            "CAMPAIGN",
            format!("{chapters} CHAPTERS"),
            if chapters > 0 {
                MenuItemState::enabled()
            } else {
                MenuItemState::disabled("NO CAMPAIGN CHAPTERS ON THIS INSTALL")
            },
            CommandSpec::new(CommandId::OpenCampaign, 17, TargetType::Screen),
            Transition::Goto(MenuPage::Campaign),
        ));
        index += 1;
        let maps = self.maps.len();
        items.push(MenuItem::new(
            MenuItemRole::Entry,
            index,
            "CUSTOM GAME",
            format!("{maps} MAPS"),
            if maps > 0 {
                MenuItemState::enabled()
            } else {
                MenuItemState::disabled("NO MAPS ON THIS INSTALL")
            },
            CommandSpec::new(CommandId::OpenCustomGame, 18, TargetType::Screen),
            Transition::Goto(MenuPage::CustomGame),
        ));
        index += 1;
        if facts.has_save {
            items.push(MenuItem::new(
                MenuItemRole::Entry,
                index,
                "LOAD GAME",
                "SLOT 1",
                MenuItemState::enabled(),
                CommandSpec::new(CommandId::LoadGame, 15, TargetType::Process)
                    .cooldown(CONFIRM_COOLDOWN_S),
                Transition::Load(1),
            ));
            index += 1;
        }
        items.push(MenuItem::new(
            MenuItemRole::Entry,
            index,
            "SETTINGS",
            "AUDIO",
            MenuItemState::enabled(),
            CommandSpec::new(CommandId::OpenSettings, 19, TargetType::Screen),
            Transition::Goto(MenuPage::Options),
        ));
        index += 1;
        items.push(MenuItem::new(
            MenuItemRole::Entry,
            index,
            if facts.quit_armed {
                "CONFIRM QUIT"
            } else {
                "QUIT"
            },
            if facts.quit_armed {
                "TAP AGAIN"
            } else {
                "EXIT"
            },
            MenuItemState::enabled(),
            CommandSpec::new(CommandId::Quit, 2, TargetType::Process).cooldown(CONFIRM_COOLDOWN_S),
            if facts.quit_armed {
                Transition::ExitProcess
            } else {
                Transition::QuitToMenu
            },
        ));
        (
            ScreenId::Main,
            "WARCRAFT III".to_string(),
            "REIGN OF CHAOS · MOBILE".to_string(),
            String::new(),
            items,
            None,
            0.0,
            0.55,
            0,
        )
    }

    fn model_single_player(
        &self,
        facts: &MenuContext,
    ) -> (
        ScreenId,
        String,
        String,
        String,
        Vec<MenuItem>,
        Option<String>,
        f32,
        f32,
        usize,
    ) {
        let mut items = Vec::new();
        let index = items.len();
        items.push(self.field(
            MenuItemRole::Entry,
            index,
            "RACE",
            self.race.label(),
            CommandId::OpenRacePicker,
            10,
        ));
        items.push(self.field(
            MenuItemRole::Entry,
            index + 1,
            "OPPONENT",
            self.opponent.label(),
            CommandId::OpenOpponentPicker,
            11,
        ));
        items.push(self.field(
            MenuItemRole::Entry,
            index + 2,
            "MAP",
            "SELECT",
            CommandId::OpenMapPicker,
            12,
        ));
        items.push(self.field(
            MenuItemRole::Entry,
            index + 3,
            "DIFFICULTY",
            self.difficulty.label(),
            CommandId::OpenDifficultyPicker,
            13,
        ));
        items.push(MenuItem::new(
            MenuItemRole::Entry,
            items.len(),
            "START GAME",
            "PLAY",
            if self.can_start() {
                MenuItemState::enabled()
            } else {
                MenuItemState::disabled("NO MAP SELECTED")
            },
            CommandSpec::new(CommandId::StartGame, 0, TargetType::Screen)
                .cooldown(CONFIRM_COOLDOWN_S),
            Transition::StartGame,
        ));
        items.push(self.back_entry(items.len()));
        (
            ScreenId::SinglePlayer,
            "SKIRMISH".to_string(),
            format!(
                "{} vs {}  ·  {}  ·  {}",
                self.race.label(),
                self.opponent.label(),
                facts.map_name,
                self.difficulty.label()
            ),
            String::new(),
            items,
            None,
            0.0,
            0.55,
            0,
        )
    }

    fn model_campaign(
        &self,
        facts: &MenuContext,
    ) -> (
        ScreenId,
        String,
        String,
        String,
        Vec<MenuItem>,
        Option<String>,
        f32,
        f32,
        usize,
    ) {
        let mut items = Vec::new();
        // Group tabs: all six retail groups are visible; a group with no chapter
        // in this install is disabled with an explicit reason.
        for (group, name) in CAMPAIGN_GROUPS.iter().enumerate() {
            let count = facts.group_count(group);
            let mut state = if count > 0 {
                MenuItemState::enabled()
            } else {
                MenuItemState::disabled("NO CHAPTERS IN THIS GROUP")
            };
            state.selected = group == self.campaign_tab;
            items.push(MenuItem::new(
                MenuItemRole::Tab,
                group,
                *name,
                count.to_string(),
                state,
                CommandSpec::new(
                    CommandId::CampaignTab(group as u8),
                    60 + group as u8,
                    TargetType::Tab,
                ),
                Transition::CampaignTab(group),
            ));
        }
        let tab = self.campaign_tab.min(CAMPAIGN_GROUPS.len() - 1);
        let group: Vec<&CampaignEntry> = facts
            .chapters
            .iter()
            .filter(|row| row.group == tab)
            .collect();
        let pages = self.campaign_pages();
        let page = self.campaign_page.min(pages.saturating_sub(1));
        for (slot, row) in group
            .iter()
            .skip(page * CAMPAIGN_ROWS_PER_PAGE)
            .take(CAMPAIGN_ROWS_PER_PAGE)
            .enumerate()
        {
            let label = if row.interlude {
                format!("—  {}", row.title)
            } else {
                format!("{:02}  {}", row.number, row.title)
            };
            items.push(MenuItem::new(
                MenuItemRole::Row,
                slot,
                label,
                row.state.as_str(),
                if row.state.enabled() {
                    MenuItemState::enabled()
                } else {
                    MenuItemState::disabled("LOCKED — COMPLETE THE PREVIOUS CHAPTER")
                },
                CommandSpec::new(
                    CommandId::ChapterRow(slot as u8),
                    40 + slot as u8,
                    TargetType::Row,
                ),
                Transition::StartChapter(slot as u8),
            ));
        }
        items.push(MenuItem::new(
            MenuItemRole::Nav,
            count_role(&items, MenuItemRole::Nav),
            "PREV",
            "<",
            if page > 0 {
                MenuItemState::enabled()
            } else {
                MenuItemState::disabled("FIRST PAGE")
            },
            CommandSpec::new(CommandId::CampaignPrev, 70, TargetType::Page),
            Transition::CampaignPage(-1),
        ));
        items.push(MenuItem::new(
            MenuItemRole::Nav,
            count_role(&items, MenuItemRole::Nav),
            "NEXT",
            ">",
            if page + 1 < pages {
                MenuItemState::enabled()
            } else {
                MenuItemState::disabled("LAST PAGE")
            },
            CommandSpec::new(CommandId::CampaignNext, 71, TargetType::Page),
            Transition::CampaignPage(1),
        ));
        items.push(self.back_nav(count_role(&items, MenuItemRole::Nav)));
        let empty = facts.chapter_count() == 0;
        (
            ScreenId::Campaign,
            "CAMPAIGN".to_string(),
            format!("CHAPTERS  ·  {}", self.difficulty.label()),
            format!("{} {}/{}", CAMPAIGN_GROUPS[tab], page + 1, pages.max(1)),
            items,
            if empty {
                Some("NO CAMPAIGN CHAPTERS ON THIS INSTALL".to_string())
            } else if group.is_empty() {
                Some("NO CHAPTERS IN THIS GROUP".to_string())
            } else {
                None
            },
            0.0,
            0.55,
            tab,
        )
    }

    fn model_custom_game(
        &self,
        facts: &MenuContext,
    ) -> (
        ScreenId,
        String,
        String,
        String,
        Vec<MenuItem>,
        Option<String>,
        f32,
        f32,
        usize,
    ) {
        let mut items = Vec::new();
        items.push(self.field(
            MenuItemRole::Tab,
            0,
            "RACE",
            self.race.label(),
            CommandId::OpenRacePicker,
            10,
        ));
        items.push(self.field(
            MenuItemRole::Tab,
            1,
            "DIFFICULTY",
            self.difficulty.label(),
            CommandId::OpenDifficultyPicker,
            13,
        ));
        let pages = self.map_pages();
        let page = self.map_page.min(pages.saturating_sub(1));
        for (slot, (index, map)) in self
            .maps
            .iter()
            .enumerate()
            .skip(page * MAP_ROWS_PER_PAGE)
            .take(MAP_ROWS_PER_PAGE)
            .enumerate()
        {
            let mut state = MenuItemState::enabled();
            state.selected = index == self.map_index;
            items.push(MenuItem::new(
                MenuItemRole::Row,
                slot,
                map.name.clone(),
                format!(
                    "{} PLAYERS{}",
                    map.players.max(1),
                    if index == self.map_index {
                        "  ·  SELECTED"
                    } else {
                        ""
                    }
                ),
                state,
                CommandSpec::new(
                    CommandId::MapRow(slot as u8),
                    80 + slot as u8,
                    TargetType::Row,
                ),
                Transition::SelectMap(index),
            ));
        }
        items.push(MenuItem::new(
            MenuItemRole::Nav,
            count_role(&items, MenuItemRole::Nav),
            "PREV",
            "<",
            if page > 0 {
                MenuItemState::enabled()
            } else {
                MenuItemState::disabled("FIRST PAGE")
            },
            CommandSpec::new(CommandId::MapPrev, 90, TargetType::Page),
            Transition::MapPage(-1),
        ));
        items.push(MenuItem::new(
            MenuItemRole::Nav,
            count_role(&items, MenuItemRole::Nav),
            "NEXT",
            ">",
            if page + 1 < pages {
                MenuItemState::enabled()
            } else {
                MenuItemState::disabled("LAST PAGE")
            },
            CommandSpec::new(CommandId::MapNext, 91, TargetType::Page),
            Transition::MapPage(1),
        ));
        items.push(MenuItem::new(
            MenuItemRole::Nav,
            count_role(&items, MenuItemRole::Nav),
            "START",
            "PLAY",
            if self.can_start() {
                MenuItemState::enabled()
            } else {
                MenuItemState::disabled("NO MAP SELECTED")
            },
            CommandSpec::new(CommandId::StartGame, 97, TargetType::Screen)
                .cooldown(CONFIRM_COOLDOWN_S),
            Transition::StartGame,
        ));
        items.push(self.back_nav(count_role(&items, MenuItemRole::Nav)));
        let label = format!("MAPS {}/{}", page + 1, pages.max(1));
        (
            ScreenId::CustomGame,
            "CUSTOM GAME".to_string(),
            format!(
                "{} vs {}  ·  {}",
                self.race.label(),
                self.opponent.label(),
                facts.map_name
            ),
            label,
            items,
            if self.maps.is_empty() {
                Some("NO SHIPPED MAPS ON THIS INSTALL".to_string())
            } else {
                None
            },
            0.0,
            0.55,
            0,
        )
    }

    fn model_race(
        &self,
        _facts: &MenuContext,
    ) -> (
        ScreenId,
        String,
        String,
        String,
        Vec<MenuItem>,
        Option<String>,
        f32,
        f32,
        usize,
    ) {
        let mut items = Vec::new();
        for (index, race) in RACES.iter().enumerate() {
            items.push(self.pick_entry(
                items.len(),
                race.label(),
                *race == self.race,
                20 + index as u8,
                CommandId::PickRace(*race),
                Transition::SetRace(*race),
            ));
        }
        items.push(self.back_entry(items.len()));
        (
            ScreenId::RaceSelect,
            "SELECT RACE".to_string(),
            format!("CURRENT {}", self.race.label()),
            String::new(),
            items,
            None,
            0.0,
            0.55,
            0,
        )
    }

    fn model_opponent(
        &self,
        _facts: &MenuContext,
    ) -> (
        ScreenId,
        String,
        String,
        String,
        Vec<MenuItem>,
        Option<String>,
        f32,
        f32,
        usize,
    ) {
        let mut items = Vec::new();
        for (index, race) in RACES.iter().enumerate() {
            items.push(self.pick_entry(
                items.len(),
                race.label(),
                *race == self.opponent,
                30 + index as u8,
                CommandId::PickOpponent(*race),
                Transition::SetOpponent(*race),
            ));
        }
        items.push(self.back_entry(items.len()));
        (
            ScreenId::OpponentSelect,
            "SELECT OPPONENT".to_string(),
            format!("CURRENT {}", self.opponent.label()),
            String::new(),
            items,
            None,
            0.0,
            0.55,
            0,
        )
    }

    fn model_map_select(
        &self,
        _facts: &MenuContext,
    ) -> (
        ScreenId,
        String,
        String,
        String,
        Vec<MenuItem>,
        Option<String>,
        f32,
        f32,
        usize,
    ) {
        let mut items = Vec::new();
        let pages = self.map_pages();
        let page = self.map_page.min(pages.saturating_sub(1));
        for (slot, (index, map)) in self
            .maps
            .iter()
            .enumerate()
            .skip(page * MAP_ROWS_PER_PAGE)
            .take(MAP_ROWS_PER_PAGE)
            .enumerate()
        {
            let mut state = MenuItemState::enabled();
            state.selected = index == self.map_index;
            items.push(MenuItem::new(
                MenuItemRole::Row,
                slot,
                map.name.clone(),
                format!("{}P", map.players.max(1)),
                state,
                CommandSpec::new(
                    CommandId::MapRow(slot as u8),
                    80 + slot as u8,
                    TargetType::Row,
                ),
                Transition::SelectMap(index),
            ));
        }
        items.push(MenuItem::new(
            MenuItemRole::Nav,
            count_role(&items, MenuItemRole::Nav),
            "PREV",
            "<",
            if page > 0 {
                MenuItemState::enabled()
            } else {
                MenuItemState::disabled("FIRST PAGE")
            },
            CommandSpec::new(CommandId::MapPrev, 90, TargetType::Page),
            Transition::MapPage(-1),
        ));
        items.push(MenuItem::new(
            MenuItemRole::Nav,
            count_role(&items, MenuItemRole::Nav),
            "NEXT",
            ">",
            if page + 1 < pages {
                MenuItemState::enabled()
            } else {
                MenuItemState::disabled("LAST PAGE")
            },
            CommandSpec::new(CommandId::MapNext, 91, TargetType::Page),
            Transition::MapPage(1),
        ));
        items.push(MenuItem::new(
            MenuItemRole::Nav,
            count_role(&items, MenuItemRole::Nav),
            "START",
            "PLAY",
            if self.can_start() {
                MenuItemState::enabled()
            } else {
                MenuItemState::disabled("NO MAP SELECTED")
            },
            CommandSpec::new(CommandId::StartGame, 97, TargetType::Screen)
                .cooldown(CONFIRM_COOLDOWN_S),
            Transition::StartGame,
        ));
        items.push(self.back_nav(count_role(&items, MenuItemRole::Nav)));
        (
            ScreenId::MapSelect,
            "SELECT MAP".to_string(),
            self.map()
                .map(|map| map.name.clone())
                .unwrap_or_else(|| "NO MAP".to_string()),
            format!("MAPS {}/{}", page + 1, pages.max(1)),
            items,
            None,
            0.0,
            0.55,
            0,
        )
    }

    fn model_difficulty(
        &self,
        _facts: &MenuContext,
    ) -> (
        ScreenId,
        String,
        String,
        String,
        Vec<MenuItem>,
        Option<String>,
        f32,
        f32,
        usize,
    ) {
        let mut items = Vec::new();
        for (index, level) in DIFFICULTIES.iter().enumerate() {
            items.push(self.pick_entry(
                items.len(),
                level.label(),
                *level == self.difficulty,
                50 + index as u8,
                CommandId::PickDifficulty(*level),
                Transition::SetDifficulty(*level),
            ));
        }
        items.push(self.back_entry(items.len()));
        (
            ScreenId::Difficulty,
            "DIFFICULTY".to_string(),
            format!("CURRENT {}", self.difficulty.label()),
            String::new(),
            items,
            None,
            0.0,
            0.55,
            0,
        )
    }

    fn model_options(
        &self,
        facts: &MenuContext,
    ) -> (
        ScreenId,
        String,
        String,
        String,
        Vec<MenuItem>,
        Option<String>,
        f32,
        f32,
        usize,
    ) {
        let mut items = Vec::new();
        let mut audio_state = MenuItemState::selected();
        audio_state.selected = true;
        items.push(MenuItem::new(
            MenuItemRole::Tab,
            0,
            "AUDIO",
            format!("{}%", self.settings.draft.master),
            audio_state,
            CommandSpec::new(CommandId::SettingsTab(0), 120, TargetType::Tab),
            Transition::SettingsTab(0),
        ));
        // The values shown are the draft: an edit is visible immediately but is
        // not applied until APPLY. When the draft is clean and the host reported
        // the engine gains, the engine is the truth.
        let shown = if self.settings.dirty() {
            self.settings.draft
        } else {
            facts.audio.unwrap_or(self.settings.applied)
        };
        for (index, key) in SETTING_KEYS.iter().enumerate() {
            let mut state = MenuItemState::enabled();
            state.selected = self.settings.selected == *key;
            items.push(MenuItem::new(
                MenuItemRole::Row,
                index,
                key.label(),
                format!("{}%  ·  − / +", shown.get(*key)),
                state,
                CommandSpec::new(
                    CommandId::SettingsSelect(*key),
                    100 + index as u8,
                    TargetType::Setting,
                ),
                Transition::SelectSetting(*key),
            ));
        }
        items.push(MenuItem::new(
            MenuItemRole::Nav,
            count_role(&items, MenuItemRole::Nav),
            "−10",
            "<",
            MenuItemState::enabled(),
            CommandSpec::new(
                CommandId::SettingStep(self.settings.selected, -1),
                110,
                TargetType::Setting,
            ),
            Transition::StepSetting(self.settings.selected, -1),
        ));
        items.push(MenuItem::new(
            MenuItemRole::Nav,
            count_role(&items, MenuItemRole::Nav),
            "+10",
            ">",
            MenuItemState::enabled(),
            CommandSpec::new(
                CommandId::SettingStep(self.settings.selected, 1),
                111,
                TargetType::Setting,
            ),
            Transition::StepSetting(self.settings.selected, 1),
        ));
        items.push(MenuItem::new(
            MenuItemRole::Nav,
            count_role(&items, MenuItemRole::Nav),
            "APPLY",
            "COMMIT",
            if self.settings.dirty() {
                MenuItemState::enabled()
            } else {
                MenuItemState::disabled("NO CHANGES TO APPLY")
            },
            CommandSpec::new(CommandId::ApplySettings, 130, TargetType::Confirm)
                .cooldown(CONFIRM_COOLDOWN_S),
            Transition::ApplySettings,
        ));
        items.push(MenuItem::new(
            MenuItemRole::Nav,
            count_role(&items, MenuItemRole::Nav),
            "CANCEL",
            "REVERT",
            if self.settings.dirty() {
                MenuItemState::enabled()
            } else {
                MenuItemState::disabled("NO CHANGES TO CANCEL")
            },
            CommandSpec::new(CommandId::CancelSettings, 131, TargetType::Confirm),
            Transition::CancelSettings,
        ));
        items.push(self.back_nav(count_role(&items, MenuItemRole::Nav)));
        (
            ScreenId::Options,
            "OPTIONS".to_string(),
            format!(
                "AUDIO  ·  MASTER {}%  ·  MUSIC {}%  ·  EFFECTS {}%",
                shown.master, shown.music, shown.effects
            ),
            String::new(),
            items,
            if self.settings.dirty() {
                Some("UNCOMMITTED CHANGES — APPLY OR CANCEL".to_string())
            } else {
                None
            },
            0.0,
            0.55,
            0,
        )
    }

    fn model_save_load(
        &self,
        facts: &MenuContext,
    ) -> (
        ScreenId,
        String,
        String,
        String,
        Vec<MenuItem>,
        Option<String>,
        f32,
        f32,
        usize,
    ) {
        let mut items = Vec::new();
        for (index, label) in ["SAVE", "LOAD"].iter().enumerate() {
            let mode = if index == 0 {
                SaveLoadMode::Save
            } else {
                SaveLoadMode::Load
            };
            let mut state = MenuItemState::enabled();
            state.selected = self.saveload.mode == mode;
            items.push(MenuItem::new(
                MenuItemRole::Tab,
                index,
                *label,
                if index == 0 { "WRITE" } else { "RESTORE" },
                state,
                CommandSpec::new(
                    if index == 0 {
                        CommandId::SaveMode
                    } else {
                        CommandId::LoadMode
                    },
                    133 + index as u8,
                    TargetType::Tab,
                ),
                Transition::None,
            ));
        }
        let mut slot_index = 0;
        for kind in [SaveSlotKind::Quick, SaveSlotKind::Autosave] {
            let info = facts.slot(kind);
            let state = info.map(|slot| &slot.state);
            let (sub, enabled, reason) = match (self.saveload.mode, state) {
                (_, None) => ("EMPTY".to_string(), false, Some("EMPTY SLOT".to_string())),
                (SaveLoadMode::Save, Some(SaveSlotState::Ready { map, tick, .. })) => (
                    format!("{map}  ·  TICK {tick}"),
                    facts.save_api && kind == SaveSlotKind::Quick,
                    if kind == SaveSlotKind::Autosave {
                        Some("AUTOSAVE IS READ-ONLY".to_string())
                    } else if !facts.save_api {
                        facts.save_reason.clone()
                    } else {
                        None
                    },
                ),
                (SaveLoadMode::Save, Some(SaveSlotState::Empty)) => (
                    "EMPTY".to_string(),
                    facts.save_api && kind == SaveSlotKind::Quick,
                    if kind == SaveSlotKind::Autosave {
                        Some("AUTOSAVE IS READ-ONLY".to_string())
                    } else if !facts.save_api {
                        facts.save_reason.clone()
                    } else {
                        None
                    },
                ),
                (_, Some(SaveSlotState::Ready { map, tick, .. })) => (
                    format!("{map}  ·  TICK {tick}"),
                    facts.save_api,
                    if facts.save_api {
                        None
                    } else {
                        facts.save_reason.clone()
                    },
                ),
                (_, Some(SaveSlotState::Empty)) => {
                    ("EMPTY".to_string(), false, Some("EMPTY SLOT".to_string()))
                }
                (_, Some(SaveSlotState::Unreadable { reason })) => (
                    format!("UNREADABLE — {reason}"),
                    false,
                    Some(reason.clone()),
                ),
                (_, Some(SaveSlotState::Unsupported { reason })) => (
                    format!("UNSUPPORTED FORMAT — {reason}"),
                    false,
                    Some(reason.clone()),
                ),
            };
            let confirm = self.saveload.pending_overwrite == Some(kind)
                && self.saveload.mode == SaveLoadMode::Save;
            let mut item_state = if enabled {
                MenuItemState::enabled()
            } else {
                MenuItemState::disabled(reason.unwrap_or_else(|| "UNAVAILABLE".to_string()))
            };
            item_state.selected = self.saveload.selected == slot_index;
            items.push(MenuItem::new(
                MenuItemRole::Row,
                slot_index,
                if confirm {
                    format!("CONFIRM OVERWRITE — {}", kind.label())
                } else {
                    kind.label().to_string()
                },
                if confirm {
                    "TAP AGAIN".to_string()
                } else {
                    sub
                },
                item_state,
                CommandSpec::new(
                    if self.saveload.mode == SaveLoadMode::Save {
                        CommandId::SaveSlot(kind.slot())
                    } else {
                        CommandId::LoadSlot(kind.slot())
                    },
                    140 + slot_index as u8,
                    TargetType::Slot,
                )
                .cooldown(CONFIRM_COOLDOWN_S),
                if self.saveload.mode == SaveLoadMode::Save {
                    Transition::SaveSlot(kind.slot())
                } else {
                    Transition::LoadSlot(kind.slot())
                },
            ));
            slot_index += 1;
        }
        items.push(self.back_nav(count_role(&items, MenuItemRole::Nav)));
        let reason = facts
            .save_reason
            .clone()
            .unwrap_or_else(|| "NO SAVE API".to_string());
        let message = if facts.save_loading {
            Some("WORKING…".to_string())
        } else if !facts.save_api {
            Some(format!("SAVE API UNAVAILABLE — {reason}"))
        } else if facts.slots.is_empty() {
            Some("NO SAVES ON THIS DEVICE".to_string())
        } else {
            None
        };
        (
            ScreenId::SaveLoad,
            "SAVE / LOAD".to_string(),
            match self.saveload.mode {
                SaveLoadMode::Save => "WRITE THE RUNNING MATCH".to_string(),
                SaveLoadMode::Load => "RESTORE A CHECKPOINT".to_string(),
            },
            String::new(),
            items,
            message,
            0.0,
            0.55,
            0,
        )
    }

    fn model_pause(
        &self,
        facts: &MenuContext,
    ) -> (
        ScreenId,
        String,
        String,
        String,
        Vec<MenuItem>,
        Option<String>,
        f32,
        f32,
        usize,
    ) {
        let mut items = Vec::new();
        items.push(MenuItem::new(
            MenuItemRole::Entry,
            0,
            "RESUME",
            "ESC",
            MenuItemState::enabled(),
            CommandSpec::new(CommandId::Resume, 7, TargetType::Process),
            Transition::Resume,
        ));
        items.push(MenuItem::new(
            MenuItemRole::Entry,
            1,
            "SETTINGS",
            "AUDIO",
            MenuItemState::enabled(),
            CommandSpec::new(CommandId::OpenSettings, 19, TargetType::Screen),
            Transition::Goto(MenuPage::Options),
        ));
        let save_state = if facts.save_api {
            MenuItemState::enabled()
        } else {
            MenuItemState::disabled(
                facts
                    .save_reason
                    .clone()
                    .unwrap_or_else(|| "SAVE UNAVAILABLE".to_string()),
            )
        };
        items.push(MenuItem::new(
            MenuItemRole::Entry,
            2,
            "SAVE GAME",
            "SLOT 1",
            save_state,
            CommandSpec::new(CommandId::SaveGame, 14, TargetType::Process)
                .cooldown(CONFIRM_COOLDOWN_S),
            Transition::Save(1),
        ));
        let load_state = if facts.quick_ready() && facts.save_api {
            MenuItemState::enabled()
        } else {
            MenuItemState::disabled("NO QUICK SAVE IN SLOT 1")
        };
        items.push(MenuItem::new(
            MenuItemRole::Entry,
            3,
            "LOAD GAME",
            "SLOT 1",
            load_state,
            CommandSpec::new(CommandId::LoadGame, 15, TargetType::Process)
                .cooldown(CONFIRM_COOLDOWN_S),
            Transition::Load(1),
        ));
        items.push(MenuItem::new(
            MenuItemRole::Entry,
            4,
            "SAVE / LOAD",
            "SLOTS",
            MenuItemState::enabled(),
            CommandSpec::new(CommandId::OpenSaveLoad, 98, TargetType::Screen),
            Transition::Goto(MenuPage::SaveLoad),
        ));
        items.push(MenuItem::new(
            MenuItemRole::Entry,
            5,
            "RESTART",
            "ENTER",
            MenuItemState::enabled(),
            CommandSpec::new(CommandId::Restart, 9, TargetType::Process)
                .cooldown(CONFIRM_COOLDOWN_S),
            Transition::Restart,
        ));
        items.push(MenuItem::new(
            MenuItemRole::Entry,
            6,
            if facts.quit_armed {
                "CONFIRM QUIT TO MENU"
            } else {
                "QUIT TO MENU"
            },
            if facts.quit_armed { "TAP AGAIN" } else { "2x" },
            MenuItemState::enabled(),
            CommandSpec::new(CommandId::Quit, 2, TargetType::Confirm).cooldown(CONFIRM_COOLDOWN_S),
            if facts.quit_armed {
                Transition::QuitToMenu
            } else {
                Transition::QuitToMenu
            },
        ));
        (
            ScreenId::Paused,
            "GAME PAUSED".to_string(),
            format!(
                "{}  ·  {}  ·  {}",
                facts.map_name,
                self.race.label(),
                self.difficulty.label()
            ),
            String::new(),
            items,
            None,
            0.0,
            0.55,
            0,
        )
    }

    fn model_end(
        &self,
        facts: &MenuContext,
        victory: bool,
    ) -> (
        ScreenId,
        String,
        String,
        String,
        Vec<MenuItem>,
        Option<String>,
        f32,
        f32,
        usize,
    ) {
        let mut items = Vec::new();
        items.push(MenuItem::new(
            MenuItemRole::Entry,
            0,
            "RESTART",
            "ENTER",
            MenuItemState::enabled(),
            CommandSpec::new(CommandId::Restart, 9, TargetType::Process)
                .cooldown(CONFIRM_COOLDOWN_S),
            Transition::Restart,
        ));
        items.push(MenuItem::new(
            MenuItemRole::Entry,
            1,
            "MAIN MENU",
            "ESC",
            MenuItemState::enabled(),
            CommandSpec::new(CommandId::MainMenu, 8, TargetType::Screen),
            Transition::QuitToMenu,
        ));
        (
            if victory {
                ScreenId::Victory
            } else {
                ScreenId::Defeat
            },
            if victory { "VICTORY" } else { "DEFEAT" }.to_string(),
            facts.end_stats.clone().unwrap_or_default(),
            String::new(),
            items,
            None,
            0.0,
            0.55,
            0,
        )
    }

    fn model_loading(
        &self,
        facts: &MenuContext,
    ) -> (
        ScreenId,
        String,
        String,
        String,
        Vec<MenuItem>,
        Option<String>,
        f32,
        f32,
        usize,
    ) {
        let screen = if facts.host == HostScreen::Loading {
            ScreenId::Loading
        } else {
            ScreenId::from_page(self.page)
        };
        (
            screen,
            if facts.host == HostScreen::Loading {
                "LOADING"
            } else {
                ""
            }
            .to_string(),
            format!(
                "{}  ·  {} vs {}  ·  {}",
                facts.map_name,
                self.race.label(),
                self.opponent.label(),
                self.difficulty.label()
            ),
            String::new(),
            Vec::new(),
            None,
            facts.loading_progress,
            0.55,
            0,
        )
    }

    fn field(
        &self,
        role: MenuItemRole,
        index: usize,
        label: &str,
        value: &str,
        command: CommandId,
        legacy: u8,
    ) -> MenuItem {
        MenuItem::new(
            role,
            index,
            label,
            value,
            MenuItemState::enabled(),
            CommandSpec::new(command, legacy, TargetType::Field),
            match command {
                CommandId::OpenRacePicker => Transition::Goto(MenuPage::RaceSelect),
                CommandId::OpenOpponentPicker => Transition::Goto(MenuPage::OpponentSelect),
                CommandId::OpenMapPicker => Transition::Goto(MenuPage::MapSelect),
                CommandId::OpenDifficultyPicker => Transition::Goto(MenuPage::Difficulty),
                _ => Transition::None,
            },
        )
    }

    fn pick_entry(
        &self,
        index: usize,
        label: &str,
        selected: bool,
        legacy: u8,
        command: CommandId,
        transition: Transition,
    ) -> MenuItem {
        if selected {
            MenuItem::new(
                MenuItemRole::Entry,
                index,
                label,
                "CURRENT",
                MenuItemState::selected(),
                CommandSpec::new(command, legacy, TargetType::Row),
                transition,
            )
        } else {
            MenuItem::new(
                MenuItemRole::Entry,
                index,
                label,
                "PICK",
                MenuItemState::enabled(),
                CommandSpec::new(command, legacy, TargetType::Row),
                transition,
            )
        }
    }

    fn back_entry(&self, index: usize) -> MenuItem {
        MenuItem::new(
            MenuItemRole::Entry,
            index,
            "BACK",
            "ESC",
            MenuItemState::enabled(),
            CommandSpec::new(CommandId::Back, 3, TargetType::Screen),
            Transition::Back,
        )
    }

    fn back_nav(&self, index: usize) -> MenuItem {
        MenuItem::new(
            MenuItemRole::Nav,
            index,
            "BACK",
            "ESC",
            MenuItemState::enabled(),
            CommandSpec::new(CommandId::Back, 3, TargetType::Screen),
            Transition::Back,
        )
    }

    /// §4.7 back target: one table, one answer per screen.
    pub fn back_action(&self) -> BackAction {
        let from = ScreenId::from_page(self.page);
        match self.page {
            MenuPage::Main => BackAction {
                from,
                to: None,
                host_owned: true,
            },
            MenuPage::Campaign | MenuPage::CustomGame | MenuPage::SinglePlayer => BackAction {
                from,
                to: Some(ScreenId::Main),
                host_owned: false,
            },
            MenuPage::RaceSelect => BackAction {
                from,
                to: Some(
                    self.picker_return
                        .map(ScreenId::from_page)
                        .unwrap_or(ScreenId::SinglePlayer),
                ),
                host_owned: false,
            },
            MenuPage::OpponentSelect => BackAction {
                from,
                to: Some(
                    self.picker_return
                        .map(ScreenId::from_page)
                        .unwrap_or(ScreenId::RaceSelect),
                ),
                host_owned: false,
            },
            MenuPage::MapSelect => BackAction {
                from,
                to: Some(
                    self.picker_return
                        .map(ScreenId::from_page)
                        .unwrap_or(ScreenId::OpponentSelect),
                ),
                host_owned: false,
            },
            MenuPage::Difficulty => BackAction {
                from,
                to: Some(
                    self.picker_return
                        .map(ScreenId::from_page)
                        .unwrap_or(ScreenId::MapSelect),
                ),
                host_owned: false,
            },
            MenuPage::Loading => BackAction {
                from,
                to: Some(ScreenId::SinglePlayer),
                host_owned: false,
            },
            MenuPage::Paused => BackAction {
                from,
                to: None,
                host_owned: true,
            },
            MenuPage::Options => BackAction {
                from,
                to: Some(ScreenId::from_page(self.options_from)),
                host_owned: false,
            },
            MenuPage::SaveLoad => BackAction {
                from,
                to: Some(ScreenId::from_page(self.save_from)),
                host_owned: false,
            },
            MenuPage::InGame => BackAction {
                from,
                to: Some(ScreenId::Paused),
                host_owned: false,
            },
        }
    }

    /// §4.7 one navigation entry point. A request for the screen that is
    /// already active is rejected, so a doubled tap cannot push a duplicate
    /// transition.
    pub fn navigate(&mut self, action: NavigationAction) -> TransitionResult {
        let from = ScreenId::from_page(self.page);
        match action {
            NavigationAction::Home => {
                let accepted = self.page != MenuPage::Main;
                self.leave_current();
                self.page = MenuPage::Main;
                TransitionResult {
                    action,
                    from,
                    to: ScreenId::Main,
                    accepted,
                    rejected: if accepted {
                        None
                    } else {
                        Some(RejectReason::AlreadyOnScreen)
                    },
                }
            }
            NavigationAction::Goto(target) => {
                if target == from {
                    return TransitionResult {
                        action,
                        from,
                        to: target,
                        accepted: false,
                        rejected: Some(RejectReason::AlreadyOnScreen),
                    };
                }
                if self.page == MenuPage::Loading && target != ScreenId::Main {
                    return TransitionResult {
                        action,
                        from,
                        to: from,
                        accepted: false,
                        rejected: Some(RejectReason::LoadingInProgress),
                    };
                }
                let page = target.page().unwrap_or(MenuPage::Main);
                if page == MenuPage::Options {
                    self.options_from = self.page;
                }
                if page == MenuPage::SaveLoad {
                    self.save_from = self.page;
                }
                self.leave_current();
                if matches!(
                    page,
                    MenuPage::RaceSelect
                        | MenuPage::OpponentSelect
                        | MenuPage::MapSelect
                        | MenuPage::Difficulty
                ) {
                    // A picker opened from the custom-game screen is a leaf of
                    // that screen; the wizard keeps its fixed chain.
                    self.picker_return =
                        (self.page == MenuPage::CustomGame).then_some(MenuPage::CustomGame);
                }
                self.page = page;
                TransitionResult {
                    action,
                    from,
                    to: target,
                    accepted: true,
                    rejected: None,
                }
            }
            NavigationAction::Back => {
                let back = self.back_action();
                let Some(to) = back.to else {
                    return TransitionResult {
                        action,
                        from,
                        to: from,
                        accepted: false,
                        rejected: Some(RejectReason::HostOwned),
                    };
                };
                if to == from {
                    return TransitionResult {
                        action,
                        from,
                        to,
                        accepted: false,
                        rejected: Some(RejectReason::AlreadyOnScreen),
                    };
                }
                self.leave_current();
                self.page = to.page().unwrap_or(MenuPage::Main);
                TransitionResult {
                    action,
                    from,
                    to,
                    accepted: true,
                    rejected: None,
                }
            }
        }
    }

    fn leave_current(&mut self) {
        if self.page == MenuPage::Options {
            self.settings.cancel();
        }
        if self.page == MenuPage::SaveLoad {
            self.saveload.pending_overwrite = None;
        }
    }

    /// §8.1/§4.7 typed reducer. The host calls this with the command the
    /// admission gate allowed and gets either a model-side change or a host
    /// action to execute.
    pub fn dispatch(&mut self, command: CommandId, facts: &MenuContext) -> Dispatch {
        let mut out = Dispatch::default();
        match command {
            CommandId::Continue => out.host = Some(HostAction::Continue),
            CommandId::OpenSinglePlayer => {
                self.open(MenuPage::SinglePlayer, &mut out);
            }
            CommandId::OpenCampaign => self.open(MenuPage::Campaign, &mut out),
            CommandId::OpenCustomGame => self.open(MenuPage::CustomGame, &mut out),
            CommandId::LoadGame => {
                if facts.save_api && facts.has_save {
                    out.host = Some(HostAction::Load(1));
                } else {
                    out.reject = Some(CommandReject::Disabled(
                        facts
                            .save_reason
                            .clone()
                            .unwrap_or_else(|| "NO SAVE ON THIS DEVICE".to_string()),
                    ));
                }
            }
            CommandId::OpenSettings => self.open(MenuPage::Options, &mut out),
            CommandId::OpenSaveLoad => {
                self.saveload.mode = SaveLoadMode::Save;
                self.saveload.pending_overwrite = None;
                self.open(MenuPage::SaveLoad, &mut out);
            }
            CommandId::Quit => {
                out.host = Some(if facts.host == HostScreen::Paused {
                    HostAction::QuitToMenu
                } else {
                    HostAction::Exit
                });
            }
            CommandId::ExitProcess => out.host = Some(HostAction::Exit),
            CommandId::Resume => {
                self.page = MenuPage::InGame;
                out.host = Some(HostAction::Resume);
            }
            CommandId::SaveGame => {
                if facts.save_api {
                    out.host = Some(HostAction::Save(1));
                } else {
                    out.reject = Some(CommandReject::Disabled(
                        facts
                            .save_reason
                            .clone()
                            .unwrap_or_else(|| "SAVE UNAVAILABLE".to_string()),
                    ));
                }
            }
            CommandId::Restart => {
                self.page = MenuPage::Loading;
                out.host = Some(HostAction::Restart);
            }
            CommandId::Back => {
                let result = self.navigate(NavigationAction::Back);
                out.applied = result.accepted;
                if !result.accepted {
                    out.reject = Some(CommandReject::WrongScreen);
                }
            }
            CommandId::StartGame | CommandId::Play => {
                self.apply(MenuAction::StartGame);
                if self.page == MenuPage::Loading {
                    out.host = Some(HostAction::StartBattle);
                } else {
                    out.reject = Some(CommandReject::Disabled(
                        self.message
                            .clone()
                            .unwrap_or_else(|| "NO MAP SELECTED".to_string()),
                    ));
                }
            }
            CommandId::OpenRacePicker => self.open(MenuPage::RaceSelect, &mut out),
            CommandId::OpenOpponentPicker => self.open(MenuPage::OpponentSelect, &mut out),
            CommandId::OpenMapPicker => self.open(MenuPage::MapSelect, &mut out),
            CommandId::OpenDifficultyPicker => self.open(MenuPage::Difficulty, &mut out),
            CommandId::PickRace(race) => {
                self.apply(MenuAction::SetRace(race));
                out.applied = true;
            }
            CommandId::PickOpponent(race) => {
                self.apply(MenuAction::SetOpponent(race));
                out.applied = true;
            }
            CommandId::PickDifficulty(level) => {
                self.apply(MenuAction::SetDifficulty(level));
                out.applied = true;
            }
            CommandId::CampaignTab(tab) => {
                self.campaign_tab = (tab as usize).min(CAMPAIGN_GROUPS.len() - 1);
                self.campaign_page = 0;
                out.applied = true;
            }
            CommandId::ChapterRow(_) => {
                // The host resolves the visible slot against the registry; the
                // model only names the command.
                out.host = Some(HostAction::Chapter);
            }
            CommandId::CampaignPrev | CommandId::CampaignNext => {
                let delta = if command == CommandId::CampaignPrev {
                    -1
                } else {
                    1
                };
                self.campaign_page = step_page(self.campaign_page, delta, self.campaign_pages());
                out.applied = true;
            }
            CommandId::MapRow(_) => out.host = Some(HostAction::MapRow),
            CommandId::MapPrev | CommandId::MapNext => {
                let delta = if command == CommandId::MapPrev { -1 } else { 1 };
                self.map_page = step_page(self.map_page, delta, self.map_pages());
                out.applied = true;
            }
            CommandId::SaveMode => {
                self.saveload.mode = SaveLoadMode::Save;
                self.saveload.pending_overwrite = None;
                out.applied = true;
            }
            CommandId::LoadMode => {
                self.saveload.mode = SaveLoadMode::Load;
                self.saveload.pending_overwrite = None;
                out.applied = true;
            }
            CommandId::SaveSlot(slot) => {
                if !facts.save_api {
                    out.reject = Some(CommandReject::Disabled(
                        facts
                            .save_reason
                            .clone()
                            .unwrap_or_else(|| "SAVE UNAVAILABLE".to_string()),
                    ));
                } else {
                    let kind = if slot == 1 {
                        SaveSlotKind::Quick
                    } else {
                        SaveSlotKind::Autosave
                    };
                    let occupied = matches!(
                        facts.slot(kind).map(|info| &info.state),
                        Some(SaveSlotState::Ready { .. })
                    );
                    if self.confirm_overwrite(kind, occupied) {
                        out.host = Some(HostAction::Save(slot));
                    } else {
                        out.applied = true;
                        self.message = Some(format!("TAP AGAIN TO OVERWRITE {}", kind.label()));
                    }
                }
            }
            CommandId::LoadSlot(slot) => {
                let ready = matches!(
                    facts.slot(SaveSlotKind::Quick).map(|info| &info.state),
                    Some(SaveSlotState::Ready { .. })
                );
                if facts.save_api && ready {
                    out.host = Some(HostAction::Load(slot));
                } else {
                    out.reject = Some(CommandReject::Disabled(
                        "NO QUICK SAVE IN SLOT 1".to_string(),
                    ));
                }
            }
            CommandId::SettingsTab(index) => {
                if index == 0 {
                    self.settings.category = 0;
                    out.applied = true;
                } else {
                    out.reject = Some(CommandReject::Disabled(
                        "CATEGORY NOT EXPOSED BY THIS BUILD".to_string(),
                    ));
                }
            }
            CommandId::SettingsSelect(key) => {
                self.settings.select(key);
                out.applied = true;
            }
            CommandId::SettingStep(key, delta) => {
                if self.settings.step(key, delta) {
                    out.applied = true;
                } else {
                    out.reject = Some(CommandReject::Disabled("LIMIT REACHED".to_string()));
                }
            }
            CommandId::ApplySettings => {
                if let Some(applied) = self.settings.apply() {
                    out.host = Some(HostAction::ApplyAudio(applied));
                    out.applied = true;
                } else {
                    out.reject = Some(CommandReject::Disabled("NO CHANGES TO APPLY".to_string()));
                }
            }
            CommandId::CancelSettings => {
                self.settings.cancel();
                out.applied = true;
            }
            CommandId::MainMenu => {
                self.apply(MenuAction::QuitToMenu);
                out.host = Some(HostAction::QuitToMenu);
                out.applied = true;
            }
        }
        out
    }

    fn open(&mut self, page: MenuPage, out: &mut Dispatch) {
        let target = ScreenId::from_page(page);
        let result = self.navigate(NavigationAction::Goto(target));
        out.applied = result.accepted;
        if !result.accepted {
            out.reject = Some(CommandReject::WrongScreen);
        }
    }

    /// Where a picker returns after a value is picked: the custom-game screen
    /// that opened it as a leaf, otherwise the single-player setup screen (the
    /// wizard chain). Captured before the page is replaced.
    fn picker_origin(&mut self) -> MenuPage {
        self.picker_return
            .take()
            .unwrap_or(if self.page == MenuPage::CustomGame {
                MenuPage::CustomGame
            } else {
                MenuPage::SinglePlayer
            })
    }

    /// §4.6 overwrite gate. Returns `true` when the save may execute: an empty
    /// slot saves immediately, an occupied slot arms and requires the second
    /// tap. The arm is cleared by any other action.
    pub fn confirm_overwrite(&mut self, kind: SaveSlotKind, occupied: bool) -> bool {
        if !occupied {
            self.saveload.pending_overwrite = None;
            return true;
        }
        if self.saveload.pending_overwrite == Some(kind) {
            self.saveload.pending_overwrite = None;
            true
        } else {
            self.saveload.pending_overwrite = Some(kind);
            false
        }
    }
}

fn count_role(items: &[MenuItem], role: MenuItemRole) -> usize {
    items.iter().filter(|item| item.role == role).count()
}

fn step_page(page: usize, delta: i8, pages: usize) -> usize {
    let pages = pages.max(1);
    if delta < 0 {
        page.saturating_sub(1)
    } else {
        (page + 1).min(pages - 1)
    }
}

/// Host work the model cannot do itself (simulation, filesystem, audio).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HostAction {
    StartBattle,
    Continue,
    Save(u8),
    Load(u8),
    Restart,
    QuitToMenu,
    Exit,
    Resume,
    Pause,
    Chapter,
    MapRow,
    ApplyAudio(AudioSettings),
}

/// Why a command was not applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandReject {
    Disabled(String),
    WrongScreen,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dispatch {
    pub applied: bool,
    pub host: Option<HostAction>,
    pub reject: Option<CommandReject>,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuState {
    pub page: MenuPage,
    pub race: RaceChoice,
    pub opponent: RaceChoice,
    pub maps: Vec<MapChoice>,
    pub map_index: usize,
    pub difficulty: Difficulty,
    pub save: SaveStatus,
    pub message: Option<String>,
    /// True when a checkpoint exists: the main menu then offers CONTINUE.
    pub has_save: bool,
    /// Selected campaign group tab (index into `CAMPAIGN_GROUPS`).
    pub campaign_tab: usize,
    /// Page inside the campaign chapter list.
    pub campaign_page: usize,
    /// Page inside the custom-game map list.
    pub map_page: usize,
    /// Chapters per campaign group, filled by `Game::sync_menu` from the
    /// registry so the menu can page without touching the campaign module.
    pub campaign_counts: Vec<usize>,
    pub settings: SettingsState,
    pub saveload: SaveLoadState,
    options_from: MenuPage,
    save_from: MenuPage,
    /// Set when a picker is a leaf of the custom-game screen; `None` keeps the
    /// fixed single-player wizard chain.
    picker_return: Option<MenuPage>,
}

impl MenuState {
    pub fn new(maps: Vec<MapChoice>) -> Self {
        Self {
            page: MenuPage::Main,
            race: RaceChoice::Human,
            opponent: RaceChoice::Orc,
            maps,
            map_index: 0,
            difficulty: Difficulty::Normal,
            save: SaveStatus::unavailable("Sim exposes no save/load API"),
            message: None,
            has_save: false,
            campaign_tab: 0,
            campaign_page: 0,
            map_page: 0,
            campaign_counts: Vec::new(),
            settings: SettingsState::default(),
            saveload: SaveLoadState::default(),
            options_from: MenuPage::Main,
            save_from: MenuPage::Paused,
            picker_return: None,
        }
    }

    /// §4.7 `ScreenState` view of this menu. Kept as a method so the state has
    /// one owner and the back links cannot drift from the page.
    pub fn screen_state(&self) -> ScreenState {
        ScreenState {
            page: self.page,
            options_from: self.options_from,
            save_from: self.save_from,
        }
    }

    /// Pages in the selected campaign tab (1 when the tab is empty, so the
    /// caller can clamp without a divide-by-zero).
    pub fn campaign_pages(&self) -> usize {
        let rows = self
            .campaign_counts
            .get(self.campaign_tab)
            .copied()
            .unwrap_or(0);
        rows.div_ceil(CAMPAIGN_ROWS_PER_PAGE).max(1)
    }

    /// Pages in the custom-game map list.
    pub fn map_pages(&self) -> usize {
        self.maps.len().div_ceil(MAP_ROWS_PER_PAGE).max(1)
    }

    pub fn with_save(mut self, save: SaveStatus) -> Self {
        self.has_save = save.available;
        self.save = save;
        self
    }

    pub fn map(&self) -> Option<&MapChoice> {
        self.maps.get(self.map_index)
    }

    pub fn can_start(&self) -> bool {
        !self.maps.is_empty()
    }

    /// Legacy reducer. Every page change goes through [`Self::navigate`] so a
    /// duplicate request is rejected in one place; the wizard's fixed back
    /// chain stays exactly as the frozen tests expect.
    pub fn apply(&mut self, action: MenuAction) {
        self.message = None;
        match action {
            MenuAction::OpenSinglePlayer => {
                let _ = self.navigate(NavigationAction::Goto(ScreenId::SinglePlayer));
            }
            MenuAction::SelectRace(race) => {
                self.race = race;
                let _ = self.navigate(NavigationAction::Goto(ScreenId::OpponentSelect));
            }
            MenuAction::SelectOpponent(opponent) => {
                self.opponent = opponent;
                if self.can_start() {
                    let _ = self.navigate(NavigationAction::Goto(ScreenId::MapSelect));
                } else {
                    self.message = Some("no shipped maps available".to_string());
                }
            }
            MenuAction::SelectMap(index) => {
                if index < self.maps.len() {
                    self.map_index = index;
                    let _ = self.navigate(NavigationAction::Goto(ScreenId::Difficulty));
                } else {
                    self.message = Some(format!("map index {index} out of range"));
                }
            }
            MenuAction::SelectDifficulty(difficulty) => self.difficulty = difficulty,
            MenuAction::StartGame => {
                if self.can_start() {
                    let _ = self.navigate(NavigationAction::Goto(ScreenId::Loading));
                } else {
                    self.message = Some("no map selected".to_string());
                }
            }
            MenuAction::SetRace(race) => {
                self.race = race;
                let origin = self.picker_origin();
                self.page = origin;
            }
            MenuAction::SetOpponent(opponent) => {
                self.opponent = opponent;
                let origin = self.picker_origin();
                self.page = origin;
            }
            MenuAction::SetMap(index) => {
                if index < self.maps.len() {
                    self.map_index = index;
                    let origin = self.picker_origin();
                    self.page = origin;
                } else {
                    self.message = Some(format!("map index {index} out of range"));
                }
            }
            MenuAction::SetDifficulty(difficulty) => {
                self.difficulty = difficulty;
                let origin = self.picker_origin();
                self.page = origin;
            }
            MenuAction::Goto(page) => {
                let _ = self.navigate(NavigationAction::Goto(ScreenId::from_page(page)));
            }
            MenuAction::Restart => {
                self.page = MenuPage::Loading;
            }
            MenuAction::AdvanceLoading => {
                if self.page == MenuPage::Loading {
                    self.page = MenuPage::InGame;
                }
            }
            MenuAction::Pause => {
                if self.page == MenuPage::InGame {
                    self.page = MenuPage::Paused;
                }
            }
            MenuAction::Resume => {
                if self.page == MenuPage::Paused {
                    self.page = MenuPage::InGame;
                }
            }
            MenuAction::OpenOptions => {
                if matches!(
                    self.page,
                    MenuPage::Main | MenuPage::Paused | MenuPage::InGame
                ) {
                    self.options_from = self.page;
                    self.page = MenuPage::Options;
                }
            }
            MenuAction::OpenSaveLoad => {
                if matches!(self.page, MenuPage::Paused | MenuPage::InGame) {
                    self.save_from = self.page;
                    self.page = MenuPage::SaveLoad;
                }
            }
            MenuAction::SaveSlot(slot) => {
                if self.save.available {
                    self.message = Some(format!("saved slot {slot}"));
                } else {
                    self.message = Some(format!(
                        "save unavailable: {}",
                        self.save.reason.as_deref().unwrap_or("no save API")
                    ));
                }
            }
            MenuAction::LoadSlot(slot) => {
                if self.save.available {
                    self.message = Some(format!("loaded slot {slot}"));
                } else {
                    self.message = Some(format!(
                        "load unavailable: {}",
                        self.save.reason.as_deref().unwrap_or("no save API")
                    ));
                }
            }
            MenuAction::Back => {
                let _ = self.navigate(NavigationAction::Back);
            }
            MenuAction::QuitToMenu => {
                self.settings.cancel();
                self.saveload.pending_overwrite = None;
                self.page = MenuPage::Main;
                self.message = None;
            }
            MenuAction::SettingsTab(index) => {
                if index == 0 {
                    self.settings.category = 0;
                }
            }
            MenuAction::SettingsSelect(key) => self.settings.select(key),
            MenuAction::SettingsStep(key, delta) => {
                self.settings.step(key, delta);
            }
            MenuAction::ApplySettings => {
                self.settings.apply();
            }
            MenuAction::CancelSettings => self.settings.cancel(),
            MenuAction::SaveLoadMode(mode) => {
                self.saveload.mode = mode;
                self.saveload.pending_overwrite = None;
            }
            MenuAction::SaveLoadSelect(index) => self.saveload.selected = index,
        }
    }
}

/// §8 command admission gate. Wraps the input layer's point-based mirror guard
/// with the command's own cooldown. The three rejections are logged by the host
/// as `GAME_MENU rejected=<reason>` — one physical gesture never produces two
/// `GAME_MENU action=` lines.
#[derive(Clone, Debug, Default)]
pub struct MenuCommandGate {
    gate: wc3_input::menu::ActivationGate,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommandVerdict {
    Apply,
    Disabled { reason: String },
    MirrorSuppressed { since_s: f32 },
    Cooldown { remaining_s: f32 },
}

impl MenuCommandGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Admit one activation. `at` is the pointer position of the accepted
    /// press/release pair; `scale` is px per dp.
    pub fn admit(
        &mut self,
        command: &CommandSpec,
        at: Vec2,
        now: f32,
        scale: f32,
    ) -> CommandVerdict {
        if !command.enabled {
            return CommandVerdict::Disabled {
                reason: command
                    .disabled_reason
                    .clone()
                    .unwrap_or_else(|| "UNAVAILABLE".to_string()),
            };
        }
        match self
            .gate
            .admit(command.legacy as u32, at, now, command.cooldown_s, scale)
        {
            wc3_input::menu::GateDecision::Allow => CommandVerdict::Apply,
            wc3_input::menu::GateDecision::MirrorSuppressed { since_s } => {
                CommandVerdict::MirrorSuppressed { since_s }
            }
            wc3_input::menu::GateDecision::Cooldown { remaining_s } => {
                CommandVerdict::Cooldown { remaining_s }
            }
        }
    }

    pub fn reset(&mut self) {
        self.gate.reset();
    }
}

impl fmt::Display for MenuPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.title())
    }
}

impl fmt::Display for ScreenId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.title())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn maps() -> Vec<MapChoice> {
        vec![
            MapChoice {
                name: "Альянс04".to_string(),
                path: "Maps/Campaign/Human04.w3m".to_string(),
                players: 2,
            },
            MapChoice {
                name: "Орда03".to_string(),
                path: "Maps/Campaign/Orc03.w3m".to_string(),
                players: 2,
            },
            MapChoice {
                name: "Ночь эльфов 05".to_string(),
                path: "Maps/Campaign/NightElf05.w3m".to_string(),
                players: 4,
            },
        ]
    }

    #[test]
    fn ui_menu_single_player_flow_transitions() {
        let mut state = MenuState::new(maps());
        assert_eq!(state.page, MenuPage::Main);
        state.apply(MenuAction::OpenSinglePlayer);
        assert_eq!(state.page, MenuPage::SinglePlayer);
        // Race is chosen from the single-player page; it advances to opponent.
        state.apply(MenuAction::SelectRace(RaceChoice::NightElf));
        assert_eq!(state.page, MenuPage::OpponentSelect);
        assert_eq!(state.race, RaceChoice::NightElf);
        state.apply(MenuAction::SelectOpponent(RaceChoice::Undead));
        assert_eq!(state.page, MenuPage::MapSelect);
        assert_eq!(state.opponent, RaceChoice::Undead);
        state.apply(MenuAction::SelectMap(2));
        assert_eq!(state.page, MenuPage::Difficulty);
        assert_eq!(
            state.map().map(|map| map.name.as_str()),
            Some("Ночь эльфов 05")
        );
        state.apply(MenuAction::SelectDifficulty(Difficulty::Hard));
        assert_eq!(state.difficulty, Difficulty::Hard);
        state.apply(MenuAction::StartGame);
        assert_eq!(state.page, MenuPage::Loading);
        state.apply(MenuAction::AdvanceLoading);
        assert_eq!(state.page, MenuPage::InGame);
    }

    #[test]
    fn ui_menu_back_navigation_chain() {
        let mut state = MenuState::new(maps());
        state.apply(MenuAction::OpenSinglePlayer);
        state.apply(MenuAction::SelectRace(RaceChoice::Human));
        state.apply(MenuAction::SelectOpponent(RaceChoice::Orc));
        state.apply(MenuAction::SelectMap(0));
        assert_eq!(state.page, MenuPage::Difficulty);
        state.apply(MenuAction::Back);
        assert_eq!(state.page, MenuPage::MapSelect);
        state.apply(MenuAction::Back);
        assert_eq!(state.page, MenuPage::OpponentSelect);
        state.apply(MenuAction::Back);
        assert_eq!(state.page, MenuPage::RaceSelect);
        state.apply(MenuAction::Back);
        assert_eq!(state.page, MenuPage::SinglePlayer);
        state.apply(MenuAction::Back);
        assert_eq!(state.page, MenuPage::Main);
    }

    #[test]
    fn ui_menu_pause_resume_options_flow() {
        let mut state = MenuState::new(maps());
        state.apply(MenuAction::OpenSinglePlayer);
        state.apply(MenuAction::SelectRace(RaceChoice::Human));
        state.apply(MenuAction::SelectOpponent(RaceChoice::Orc));
        state.apply(MenuAction::SelectMap(0));
        state.apply(MenuAction::StartGame);
        state.apply(MenuAction::AdvanceLoading);
        state.apply(MenuAction::Pause);
        assert_eq!(state.page, MenuPage::Paused);
        state.apply(MenuAction::OpenOptions);
        assert_eq!(state.page, MenuPage::Options);
        state.apply(MenuAction::Back);
        assert_eq!(
            state.page,
            MenuPage::Paused,
            "options returns to the pause menu"
        );
        state.apply(MenuAction::Resume);
        assert_eq!(state.page, MenuPage::InGame);
        state.apply(MenuAction::Pause);
        state.apply(MenuAction::QuitToMenu);
        assert_eq!(state.page, MenuPage::Main);
    }

    #[test]
    fn persistence_save_placeholder_is_explicit_without_sim_api() {
        let mut state = MenuState::new(maps());
        assert!(!state.save.available);
        assert!(
            state.save.reason.as_deref().unwrap_or("").contains("save"),
            "the reason names the missing save API"
        );
        state.apply(MenuAction::OpenSinglePlayer);
        state.apply(MenuAction::SelectRace(RaceChoice::Human));
        state.apply(MenuAction::SelectOpponent(RaceChoice::Orc));
        state.apply(MenuAction::SelectMap(0));
        state.apply(MenuAction::StartGame);
        state.apply(MenuAction::AdvanceLoading);
        state.apply(MenuAction::Pause);
        state.apply(MenuAction::OpenSaveLoad);
        assert_eq!(state.page, MenuPage::SaveLoad);
        let page_before = state.page;
        state.apply(MenuAction::SaveSlot(0));
        assert_eq!(state.page, page_before, "save does not silently succeed");
        assert!(
            state
                .message
                .as_deref()
                .unwrap_or("")
                .contains("save unavailable"),
            "message: {:?}",
            state.message
        );
        state.apply(MenuAction::LoadSlot(0));
        assert!(
            state
                .message
                .as_deref()
                .unwrap_or("")
                .contains("load unavailable")
        );
        // When the sim gains a save API the menu records real slot actions.
        let mut with_api = MenuState::new(maps()).with_save(SaveStatus::available(3));
        with_api.apply(MenuAction::OpenSinglePlayer);
        with_api.apply(MenuAction::SelectRace(RaceChoice::Human));
        with_api.apply(MenuAction::SelectOpponent(RaceChoice::Orc));
        with_api.apply(MenuAction::SelectMap(0));
        with_api.apply(MenuAction::StartGame);
        with_api.apply(MenuAction::AdvanceLoading);
        with_api.apply(MenuAction::Pause);
        with_api.apply(MenuAction::OpenSaveLoad);
        with_api.apply(MenuAction::SaveSlot(2));
        assert_eq!(with_api.message.as_deref(), Some("saved slot 2"));
    }

    #[test]
    fn ui_menu_map_choice_uses_shipped_map_list() {
        let mut state = MenuState::new(maps());
        assert_eq!(state.maps.len(), 3);
        state.apply(MenuAction::OpenSinglePlayer);
        state.apply(MenuAction::SelectRace(RaceChoice::Random));
        state.apply(MenuAction::SelectOpponent(RaceChoice::Random));
        state.apply(MenuAction::SelectMap(1));
        assert_eq!(state.map().map(|map| map.players), Some(2));
        let empty = MenuState::new(Vec::new());
        assert!(!empty.can_start());
        let mut empty = empty;
        empty.apply(MenuAction::StartGame);
        assert_eq!(empty.page, MenuPage::Main);
        assert_eq!(empty.message.as_deref(), Some("no map selected"));
        assert_eq!(RACES.len(), 5);
        assert_eq!(DIFFICULTIES.len(), 3);
    }

    /// The UI layer's own screen plan must hold for every shipped screen, not
    /// just the main menu: each model's entry/nav/row labels are planned at the
    /// five viewport classes and must be fully drawn or honestly ellipsised.
    #[test]
    fn ui_menu_screens_plan_without_clipping_on_every_viewport() {
        use crate::ui::layout::screens::{plan_menu, shipped_windows};
        use crate::ui::layout::text_fit;
        use crate::ui::theme::Theme;

        fn context() -> MenuContext {
            let mut context = MenuContext::new(HostScreen::MainMenu);
            context.chapters = (0..7)
                .map(|index| CampaignEntry {
                    index,
                    id: format!("Human{index:02}"),
                    title: format!("CHAPTER {index}: THE SCATTERING OF THE ALLIANCE"),
                    number: index as u32 + 1,
                    group: index % CAMPAIGN_GROUPS.len(),
                    state: if index == 0 {
                        CampaignState::Unlocked
                    } else {
                        CampaignState::Locked
                    },
                    interlude: false,
                })
                .collect();
            context.chapter_count = 7;
            context
        }

        let theme = Theme::classic_fantasy();
        let mut state = MenuState::new(maps());
        state.campaign_counts = vec![2, 2, 1, 1, 1, 0];
        let windows = shipped_windows();
        let mut planned = 0usize;

        let mut screens: Vec<(MenuPage, HostScreen)> = vec![
            (MenuPage::Main, HostScreen::MainMenu),
            (MenuPage::Campaign, HostScreen::MainMenu),
            (MenuPage::CustomGame, HostScreen::MainMenu),
            (MenuPage::SinglePlayer, HostScreen::MainMenu),
            (MenuPage::RaceSelect, HostScreen::MainMenu),
            (MenuPage::OpponentSelect, HostScreen::MainMenu),
            (MenuPage::MapSelect, HostScreen::MainMenu),
            (MenuPage::Difficulty, HostScreen::MainMenu),
            (MenuPage::Options, HostScreen::MainMenu),
            (MenuPage::SaveLoad, HostScreen::MainMenu),
            (MenuPage::Paused, HostScreen::Paused),
            (MenuPage::Loading, HostScreen::Loading),
            (MenuPage::InGame, HostScreen::MainMenu),
        ];
        screens.push((MenuPage::Main, HostScreen::Victory));
        screens.push((MenuPage::Main, HostScreen::Defeat));

        for (page, host) in screens {
            state.page = page;
            let mut facts = context();
            facts.host = host;
            facts.has_save = page == MenuPage::Main;
            facts.save_api = page == MenuPage::Main;
            let model = state.model(&facts);
            let entries: Vec<&str> = model
                .items_for(MenuItemRole::Entry)
                .into_iter()
                .map(|item| item.label.as_str())
                .collect();
            let rows: Vec<&str> = model
                .items_for(MenuItemRole::Row)
                .into_iter()
                .map(|item| item.label.as_str())
                .collect();
            let nav: Vec<&str> = model
                .items_for(MenuItemRole::Nav)
                .into_iter()
                .map(|item| item.label.as_str())
                .collect();
            let tabs: Vec<&str> = model
                .items_for(MenuItemRole::Tab)
                .into_iter()
                .map(|item| item.label.as_str())
                .collect();

            for (label, window) in &windows {
                // Entries + nav (main menu, pause, victory, loading).
                let plan = plan_menu(
                    window,
                    &theme,
                    &model.title,
                    &model.subtitle,
                    &entries,
                    &nav,
                    0,
                );
                assert!(
                    plan.readable,
                    "{page:?}/{label}: unreadable plan (title {:?}, entries {entries:?})",
                    plan.title
                );
                assert!(
                    plan.title.fits(),
                    "{page:?}/{label}: title {:?} does not fit",
                    plan.title
                );
                for entry in &plan.rows {
                    assert!(
                        entry.fits()
                            || (entry.truncated
                                && text_fit::advance(&entry.text, entry.size_dp)
                                    <= entry.rect.w + 1.0),
                        "{page:?}/{label}: entry label overflows: {entry:?}"
                    );
                }
                // List pages: rows + nav are planned the same way (campaign
                // chapters, custom maps, save slots, settings rows).
                let list = plan_menu(
                    window,
                    &theme,
                    &model.title,
                    &model.subtitle,
                    &rows,
                    &nav,
                    0,
                );
                assert!(
                    list.readable,
                    "{page:?}/{label}: unreadable list plan (rows {rows:?})"
                );
                for row in &list.rows {
                    assert!(
                        row.fits()
                            || (row.truncated
                                && text_fit::advance(&row.text, row.size_dp)
                                    <= row.rect.w + 1.0),
                        "{page:?}/{label}: row label overflows: {row:?}"
                    );
                }
                // Campaign/custom tabs: each tab is a full row too.
                for tab in &tabs {
                    let tab_plan = plan_menu(window, &theme, &model.title, &tab, &[tab], &[], 0);
                    if let Some(only) = tab_plan.rows.first() {
                        assert!(
                            only.fits() || only.truncated,
                            "{page:?}/{label}: tab {tab:?} overflows: {only:?}"
                        );
                    }
                }
                // Every page is reachable, in order, with the same row budget.
                let total = entries.len();
                if total > 0 {
                    let mut seen = 0usize;
                    for page_index in 0..plan.page_count {
                        let next = plan_menu(
                            window,
                            &theme,
                            &model.title,
                            &model.subtitle,
                            &entries,
                            &nav,
                            page_index,
                        );
                        assert_eq!(next.page, page_index, "{page:?}/{label}: page index");
                        assert!(next.readable, "{page:?}/{label}: page {page_index}");
                        seen += next.rows.len();
                    }
                    assert_eq!(seen, total, "{page:?}/{label}: every entry on a page");
                }
                planned += 1;
            }
        }
        assert!(planned >= 13 * 5, "every screen x viewport was planned: {planned}");
    }
}
