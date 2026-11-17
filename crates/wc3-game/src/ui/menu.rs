//! Single-player menu state machine (mechanic `ui.menu` + `persistence.save`).
//!
//! The flow is explicit and testable: Main → SinglePlayer → Race → Opponent →
//! Map → Difficulty → Loading → InGame, plus the in-game menu (Pause, Options,
//! Save/Load). The save/load slots are wired to the save API when the simulation
//! exposes one; today `Sim` has no save/load, so the placeholder is explicit
//! (`SaveStatus::unavailable`) and never pretends to save.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
}

pub const RACES: [RaceChoice; 5] = [
    RaceChoice::Human,
    RaceChoice::Orc,
    RaceChoice::Undead,
    RaceChoice::NightElf,
    RaceChoice::Random,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
pub const CAMPAIGN_GROUPS: [&str; 6] = [
    "PROLOGUE",
    "HUMAN",
    "UNDEAD",
    "ORC",
    "NIGHT ELF",
    "EXTRA",
];

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
    options_from: MenuPage,
    save_from: MenuPage,
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
            options_from: MenuPage::Main,
            save_from: MenuPage::Paused,
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
        rows.div_ceil(crate::hud::menu::CAMPAIGN_ROWS_PER_PAGE).max(1)
    }

    /// Pages in the custom-game map list.
    pub fn map_pages(&self) -> usize {
        self.maps
            .len()
            .div_ceil(crate::hud::menu::MAP_ROWS_PER_PAGE)
            .max(1)
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

    pub fn apply(&mut self, action: MenuAction) {
        self.message = None;
        match action {
            MenuAction::OpenSinglePlayer => self.page = MenuPage::SinglePlayer,
            MenuAction::SelectRace(race) => {
                self.race = race;
                self.page = MenuPage::OpponentSelect;
            }
            MenuAction::SelectOpponent(opponent) => {
                self.opponent = opponent;
                if self.can_start() {
                    self.page = MenuPage::MapSelect;
                } else {
                    self.message = Some("no shipped maps available".to_string());
                }
            }
            MenuAction::SelectMap(index) => {
                if index < self.maps.len() {
                    self.map_index = index;
                    self.page = MenuPage::Difficulty;
                } else {
                    self.message = Some(format!("map index {index} out of range"));
                }
            }
            MenuAction::SelectDifficulty(difficulty) => self.difficulty = difficulty,
            MenuAction::StartGame => {
                if self.can_start() {
                    self.page = MenuPage::Loading;
                } else {
                    self.message = Some("no map selected".to_string());
                }
            }
            MenuAction::SetRace(race) => {
                self.race = race;
                self.page = MenuPage::SinglePlayer;
            }
            MenuAction::SetOpponent(opponent) => {
                self.opponent = opponent;
                self.page = MenuPage::SinglePlayer;
            }
            MenuAction::SetMap(index) => {
                if index < self.maps.len() {
                    self.map_index = index;
                    self.page = MenuPage::SinglePlayer;
                } else {
                    self.message = Some(format!("map index {index} out of range"));
                }
            }
            MenuAction::SetDifficulty(difficulty) => {
                self.difficulty = difficulty;
                self.page = MenuPage::SinglePlayer;
            }
            MenuAction::Goto(page) => {
                // Pickers never lose the battle that is running behind them.
                if self.page != MenuPage::Loading || page == MenuPage::Main {
                    self.page = page;
                } else {
                    self.message = Some("loading in progress".to_string());
                }
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
                        self.save
                            .reason
                            .as_deref()
                            .unwrap_or("no save API")
                    ));
                }
            }
            MenuAction::LoadSlot(slot) => {
                if self.save.available {
                    self.message = Some(format!("loaded slot {slot}"));
                } else {
                    self.message = Some(format!(
                        "load unavailable: {}",
                        self.save
                            .reason
                            .as_deref()
                            .unwrap_or("no save API")
                    ));
                }
            }
            MenuAction::Back => self.page = self.back_target(),
            MenuAction::QuitToMenu => {
                self.page = MenuPage::Main;
                self.message = None;
            }
        }
    }

    fn back_target(&self) -> MenuPage {
        match self.page {
            MenuPage::Main => MenuPage::Main,
            MenuPage::Campaign => MenuPage::Main,
            MenuPage::CustomGame => MenuPage::Main,
            MenuPage::SinglePlayer => MenuPage::Main,
            MenuPage::RaceSelect => MenuPage::SinglePlayer,
            MenuPage::OpponentSelect => MenuPage::RaceSelect,
            MenuPage::MapSelect => MenuPage::OpponentSelect,
            MenuPage::Difficulty => MenuPage::MapSelect,
            MenuPage::Loading => MenuPage::SinglePlayer,
            MenuPage::Paused => MenuPage::InGame,
            MenuPage::Options => self.options_from,
            MenuPage::SaveLoad => self.save_from,
            MenuPage::InGame => MenuPage::Paused,
        }
    }
}

impl fmt::Display for MenuPage {
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
        assert_eq!(state.map().map(|map| map.name.as_str()), Some("Ночь эльфов 05"));
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
        assert_eq!(state.page, MenuPage::Paused, "options returns to the pause menu");
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
}
