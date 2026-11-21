//! Gameplay audio events: which retail sound each event maps to.
//!
//! Mapping sources are the shipped data tables only:
//! `UI/SoundInfo/UISounds.slk` (UI, items, quests, `JobDoneSound<Race>`,
//! `ConstructingBuilding*`, `UpgradeComplete*`, `HeroDies*`, `GoldMineCollapse*`,
//! `NoGold/NoLumber/NoFood*`), `AmbienceSounds.slk` (construction loops),
//! `AnimSounds.slk` (`Repair`), `UnitAckSounds.slk` / `UnitCombatSounds.slk`
//! (unit acks/attacks/deaths through `Units/unitUI.slk` sound sets),
//! `AbilitySounds.slk` (ability casts), and `UI/TriggerData.txt` `musictheme`
//! rows (race music, victory/defeat stingers).
//!
//! Two events need a shipped file without a SoundInfo row:
//! `Sound\Interface\ReceiveGold.wav` (its filename states the semantics) is
//! played for the gold deposit; `DepositLumber` has no shipped file at all and
//! stays a documented gap. An event with no shipped sound stays silent and is
//! reported as a data gap; inventing a sound would be worse than the gap.

use glam::Vec3;

use crate::engine::{Category, PlayParams, VoiceId};
use crate::game_audio::{GameAudio, SoundProvider};

/// Playable races (the four per-race sound families).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Race {
    Human,
    Orc,
    NightElf,
    Undead,
}

impl Race {
    pub const ALL: [Race; 4] = [Race::Human, Race::Orc, Race::NightElf, Race::Undead];

    pub fn id(self) -> &'static str {
        match self {
            Race::Human => "human",
            Race::Orc => "orc",
            Race::NightElf => "nightelf",
            Race::Undead => "undead",
        }
    }

    /// `UISounds.slk` row played when this race finishes a building.
    pub fn job_done(self) -> &'static str {
        match self {
            Race::Human => "JobDoneSoundHuman",
            Race::Orc => "JobDoneSoundOrc",
            Race::NightElf => "JobDoneSoundNightElf",
            Race::Undead => "JobDoneSoundUndead",
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Race::Human => "Human",
            Race::Orc => "Orc",
            Race::NightElf => "NightElf",
            Race::Undead => "Undead",
        }
    }

    /// Retail music theme keys for a game state, in preference order.
    pub fn theme_names(self, state: MusicState) -> &'static [&'static str] {
        match (self, state) {
            (Race::Human, MusicState::Playing) => &["Human1", "Human2", "Human3"],
            (Race::Human, MusicState::Victory) => &["VictoryHuman", "HumanVictory"],
            (Race::Human, MusicState::Defeat) => &["DefeatHuman", "HumanDefeat"],
            (Race::Orc, MusicState::Playing) => &["Orc1", "Orc2", "Orc3"],
            (Race::Orc, MusicState::Victory) => &["VictoryOrc", "OrcVictory"],
            (Race::Orc, MusicState::Defeat) => &["DefeatOrc", "OrcDefeat"],
            (Race::NightElf, MusicState::Playing) => &["NightElf1", "NightElf2", "NightElf3"],
            (Race::NightElf, MusicState::Victory) => &["VictoryNightElf", "NightElfVictory"],
            (Race::NightElf, MusicState::Defeat) => &["DefeatNightElf", "NightElfDefeat"],
            (Race::Undead, MusicState::Playing) => &["Undead1", "Undead2", "Undead3"],
            (Race::Undead, MusicState::Victory) => &["VictoryUndead", "UndeadVictory"],
            (Race::Undead, MusicState::Defeat) => &["DefeatUndead", "UndeadDefeat"],
        }
    }
}

/// Music state the campaign/HUD selects a theme for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicState {
    Playing,
    Victory,
    Defeat,
}

/// Target-surface material for a weapon-impact row.
///
/// `UnitCombatSounds.slk` ships one row per `<weapon><material>` pair
/// (`MetalMediumSliceFlesh`, `MetalMediumSliceWood`, ...). The material comes
/// from the target's own `unitUI.slk` class column (`Flesh`/`Wood`/`Metal`/
/// `Stone`/`Ethereal`); the weapon prefix comes from `UnitWeapons.slk`
/// `weapType1`/`weapType2` (`MetalMediumSlice`, `AxeMediumChop`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Material {
    Flesh,
    Wood,
    Stone,
    Metal,
    Ethereal,
}

impl Material {
    pub const ALL: [Material; 5] = [
        Material::Flesh,
        Material::Wood,
        Material::Stone,
        Material::Metal,
        Material::Ethereal,
    ];

    pub fn suffix(self) -> &'static str {
        match self {
            Material::Flesh => "Flesh",
            Material::Wood => "Wood",
            Material::Stone => "Stone",
            Material::Metal => "Metal",
            Material::Ethereal => "Ethereal",
        }
    }

    /// Parse a `unitUI.slk` armour/material cell.
    pub fn from_class(value: &str) -> Option<Material> {
        match value.trim().to_ascii_lowercase().as_str() {
            "flesh" => Some(Material::Flesh),
            "wood" => Some(Material::Wood),
            "stone" => Some(Material::Stone),
            "metal" => Some(Material::Metal),
            "ethereal" => Some(Material::Ethereal),
            _ => None,
        }
    }
}

/// Sound group a gameplay event belongs to (drives the mixer's distance rule
/// and the per-group coverage tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioGroup {
    /// Order acknowledgements: move/attack order, worker ack, selection, ready.
    Ack,
    /// Weapon launch acknowledgement and the weapon impact on the target.
    Attack,
    /// Unit / hero death and structure collapse.
    Death,
    /// Construction, upgrade and repair.
    Build,
    /// Interface feedback: click, waypoint, items, quests.
    Ui,
    /// Denial / warning feedback: error, no gold/lumber/food, low resource.
    Error,
}

impl AudioGroup {
    pub const ALL: [AudioGroup; 6] = [
        AudioGroup::Ack,
        AudioGroup::Attack,
        AudioGroup::Death,
        AudioGroup::Build,
        AudioGroup::Ui,
        AudioGroup::Error,
    ];

    pub fn id(self) -> &'static str {
        match self {
            AudioGroup::Ack => "ack",
            AudioGroup::Attack => "attack",
            AudioGroup::Death => "death",
            AudioGroup::Build => "build",
            AudioGroup::Ui => "ui",
            AudioGroup::Error => "error",
        }
    }
}

/// Every gameplay event that has (or should have) a retail sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioEvent {
    // Unit voices (resolved through the unit's own `unitSound` set).
    Attack,
    /// Weapon impact on the target (`UnitCombatSounds` `<weapon><material>`).
    AttackImpact,
    Death,
    AbilityCast,
    WorkerAck,
    MoveOrder,
    AttackOrder,
    TrainComplete,
    /// Selection click: the unit's own acknowledgement (`{set}What`).
    Selected,
    // Construction / economy.
    BuildStart,
    BuildLoop,
    RepairLoop,
    BuildComplete,
    UpgradeComplete,
    DepositGold,
    DepositLumber,
    // UI / deny.
    UiClick,
    UiError,
    /// Rally point / waypoint placement (`RallyPointPlace`).
    Waypoint,
    LevelUp,
    DenyNoGold,
    DenyNoLumber,
    DenyNoFood,
    /// Low-resource warning (`GoldMineLow<Race>` / `NoGoldGeneric`).
    LowGold,
    /// Low-food warning (`NoFood<Race>` / `UpkeepLevel` fallback).
    LowFood,
    // Items / quests.
    ItemPickup,
    ItemReward,
    ItemDrop,
    QuestNew,
    QuestUpdate,
    QuestCompleted,
    QuestFailed,
    // Death / world.
    HeroDeath,
    GoldMineCollapse,
}

impl AudioEvent {
    pub const ALL: [AudioEvent; 34] = [
        AudioEvent::Attack,
        AudioEvent::AttackImpact,
        AudioEvent::Death,
        AudioEvent::AbilityCast,
        AudioEvent::WorkerAck,
        AudioEvent::MoveOrder,
        AudioEvent::AttackOrder,
        AudioEvent::TrainComplete,
        AudioEvent::Selected,
        AudioEvent::BuildStart,
        AudioEvent::BuildLoop,
        AudioEvent::RepairLoop,
        AudioEvent::BuildComplete,
        AudioEvent::UpgradeComplete,
        AudioEvent::DepositGold,
        AudioEvent::DepositLumber,
        AudioEvent::UiClick,
        AudioEvent::UiError,
        AudioEvent::Waypoint,
        AudioEvent::LevelUp,
        AudioEvent::DenyNoGold,
        AudioEvent::DenyNoLumber,
        AudioEvent::DenyNoFood,
        AudioEvent::LowGold,
        AudioEvent::LowFood,
        AudioEvent::ItemPickup,
        AudioEvent::ItemReward,
        AudioEvent::ItemDrop,
        AudioEvent::QuestNew,
        AudioEvent::QuestUpdate,
        AudioEvent::QuestCompleted,
        AudioEvent::QuestFailed,
        AudioEvent::HeroDeath,
        AudioEvent::GoldMineCollapse,
    ];

    pub fn id(self) -> &'static str {
        match self {
            AudioEvent::Attack => "attack",
            AudioEvent::AttackImpact => "attack_impact",
            AudioEvent::Death => "death",
            AudioEvent::AbilityCast => "ability_cast",
            AudioEvent::WorkerAck => "worker_ack",
            AudioEvent::MoveOrder => "move_order",
            AudioEvent::AttackOrder => "attack_order",
            AudioEvent::TrainComplete => "train_complete",
            AudioEvent::Selected => "selected",
            AudioEvent::BuildStart => "build_start",
            AudioEvent::BuildLoop => "build_loop",
            AudioEvent::RepairLoop => "repair_loop",
            AudioEvent::BuildComplete => "build_complete",
            AudioEvent::UpgradeComplete => "upgrade_complete",
            AudioEvent::DepositGold => "deposit_gold",
            AudioEvent::DepositLumber => "deposit_lumber",
            AudioEvent::UiClick => "ui_click",
            AudioEvent::UiError => "ui_error",
            AudioEvent::Waypoint => "waypoint",
            AudioEvent::LevelUp => "level_up",
            AudioEvent::DenyNoGold => "deny_no_gold",
            AudioEvent::DenyNoLumber => "deny_no_lumber",
            AudioEvent::DenyNoFood => "deny_no_food",
            AudioEvent::LowGold => "low_gold",
            AudioEvent::LowFood => "low_food",
            AudioEvent::ItemPickup => "item_pickup",
            AudioEvent::ItemReward => "item_reward",
            AudioEvent::ItemDrop => "item_drop",
            AudioEvent::QuestNew => "quest_new",
            AudioEvent::QuestUpdate => "quest_update",
            AudioEvent::QuestCompleted => "quest_completed",
            AudioEvent::QuestFailed => "quest_failed",
            AudioEvent::HeroDeath => "hero_death",
            AudioEvent::GoldMineCollapse => "gold_mine_collapse",
        }
    }

    /// Group the event belongs to (the mixer rule and the coverage matrix).
    pub fn group(self) -> AudioGroup {
        match self {
            AudioEvent::WorkerAck
            | AudioEvent::MoveOrder
            | AudioEvent::AttackOrder
            | AudioEvent::TrainComplete
            | AudioEvent::Selected => AudioGroup::Ack,
            AudioEvent::Attack | AudioEvent::AttackImpact => AudioGroup::Attack,
            AudioEvent::Death | AudioEvent::HeroDeath | AudioEvent::GoldMineCollapse => {
                AudioGroup::Death
            }
            AudioEvent::BuildStart
            | AudioEvent::BuildLoop
            | AudioEvent::RepairLoop
            | AudioEvent::BuildComplete
            | AudioEvent::UpgradeComplete => AudioGroup::Build,
            AudioEvent::UiClick
            | AudioEvent::Waypoint
            | AudioEvent::ItemPickup
            | AudioEvent::ItemReward
            | AudioEvent::ItemDrop
            | AudioEvent::QuestNew
            | AudioEvent::QuestUpdate
            | AudioEvent::QuestCompleted
            | AudioEvent::QuestFailed
            | AudioEvent::AbilityCast
            | AudioEvent::LevelUp => AudioGroup::Ui,
            AudioEvent::UiError
            | AudioEvent::DenyNoGold
            | AudioEvent::DenyNoLumber
            | AudioEvent::DenyNoFood
            | AudioEvent::LowGold
            | AudioEvent::LowFood
            | AudioEvent::DepositGold
            | AudioEvent::DepositLumber => AudioGroup::Error,
        }
    }

    /// Events whose sound row is selected by race (`<Name><Race>`), so playback
    /// without a race must not guess a race.
    pub fn is_race_scoped(self) -> bool {
        matches!(
            self,
            AudioEvent::BuildStart
                | AudioEvent::BuildLoop
                | AudioEvent::BuildComplete
                | AudioEvent::UpgradeComplete
                | AudioEvent::HeroDeath
                | AudioEvent::DenyNoGold
                | AudioEvent::DenyNoLumber
                | AudioEvent::DenyNoFood
                | AudioEvent::LowGold
                | AudioEvent::LowFood
                | AudioEvent::GoldMineCollapse
        )
    }

    /// Unit-voice events resolved through the unit's own `unitSound` set.
    pub fn is_unit_scoped(self) -> bool {
        matches!(
            self,
            AudioEvent::Attack
                | AudioEvent::Death
                | AudioEvent::WorkerAck
                | AudioEvent::Selected
        )
    }

    /// Shipped file the event plays directly, for events with no SoundInfo row.
    ///
    /// `ReceiveGold.wav` is the retail interface cue for gold received; the
    /// filename is the only shipped mapping (no `UISounds.slk` row exists).
    pub fn direct_file(self) -> Option<&'static str> {
        match self {
            AudioEvent::DepositGold => Some("Sound\\Interface\\ReceiveGold.wav"),
            _ => None,
        }
    }

    /// Event with no shipped sound at all: silent by data, not by defect.
    ///
    /// Only `DepositLumber` remains: the archive ships no lumber-receipt cue
    /// and no table row, so the event is reported once as a gap instead of
    /// borrowing the gold cue (which would be the wrong sound).
    pub fn documented_gap(self) -> bool {
        matches!(self, AudioEvent::DepositLumber)
    }
}

/// Retail `SoundInfo` definitions for an event, in preference order.
///
/// Unit-voice events resolve through `unitUI.slk` sound sets instead and return
/// an empty slice. `AbilityCast` is resolved by the caller from
/// `AbilitySounds.slk` (`GameAudio::spell`), and `AttackImpact` from
/// `UnitCombatSounds.slk` (`GameAudio::attack_impact`, weapon × material).
/// `LevelUp` is the shipped `AnimSounds.slk` row; `DepositGold` plays the
/// shipped `Sound\Interface\ReceiveGold.wav` directly
/// ([`AudioEvent::direct_file`]); `DepositLumber` is the one documented gap
/// ([`AudioEvent::documented_gap`]). With `race = None` the race-scoped events
/// list every race variant, which is what the audit matrix needs; playback
/// should pass the race.
pub fn event_defs(event: AudioEvent, race: Option<Race>) -> &'static [&'static str] {
    match (event, race) {
        (AudioEvent::BuildStart, Some(Race::Human)) => &["ConstructingBuildingDefault"],
        (AudioEvent::BuildStart, Some(Race::Orc)) => &["ConstructingBuildingOrc"],
        (AudioEvent::BuildStart, Some(Race::NightElf)) => &["ConstructingBuildingNightElf"],
        (AudioEvent::BuildStart, Some(Race::Undead)) => &["ConstructingBuildingUndead"],
        (AudioEvent::BuildStart, None) => &[
            "ConstructingBuildingDefault",
            "ConstructingBuildingOrc",
            "ConstructingBuildingNightElf",
            "ConstructingBuildingUndead",
        ],
        (AudioEvent::BuildLoop, Some(Race::Human) | Some(Race::NightElf)) => {
            &["BuildingConstructionLoop"]
        }
        (AudioEvent::BuildLoop, Some(Race::Orc)) => &["OrcBuildingConstructionLoop"],
        (AudioEvent::BuildLoop, Some(Race::Undead)) => &["UndeadBuildingConstructionLoop"],
        (AudioEvent::BuildLoop, None) => &[
            "BuildingConstructionLoop",
            "OrcBuildingConstructionLoop",
            "UndeadBuildingConstructionLoop",
        ],
        (AudioEvent::RepairLoop, _) => &["Repair"],
        (AudioEvent::BuildComplete, Some(race)) => race_slice(race.job_done()),
        (AudioEvent::BuildComplete, None) => &[
            "JobDoneSoundHuman",
            "JobDoneSoundOrc",
            "JobDoneSoundNightElf",
            "JobDoneSoundUndead",
        ],
        (AudioEvent::UpgradeComplete, Some(race)) => {
            race_slice_named("UpgradeComplete", race.suffix())
        }
        (AudioEvent::UpgradeComplete, None) => &[
            "UpgradeCompleteHuman",
            "UpgradeCompleteOrc",
            "UpgradeCompleteNightElf",
            "UpgradeCompleteUndead",
        ],
        (AudioEvent::HeroDeath, Some(race)) => race_slice_named("HeroDies", race.suffix()),
        (AudioEvent::HeroDeath, None) => &[
            "HeroDiesHuman",
            "HeroDiesOrc",
            "HeroDiesNightElf",
            "HeroDiesUndead",
        ],
        (AudioEvent::DenyNoGold, Some(race)) => race_slice_named("NoGold", race.suffix()),
        (AudioEvent::DenyNoGold, None) => &[
            "NoGoldHuman",
            "NoGoldOrc",
            "NoGoldNightElf",
            "NoGoldUndead",
        ],
        (AudioEvent::DenyNoLumber, Some(race)) => race_slice_named("NoLumber", race.suffix()),
        (AudioEvent::DenyNoLumber, None) => &[
            "NoLumberHuman",
            "NoLumberOrc",
            "NoLumberNightElf",
            "NoLumberUndead",
        ],
        (AudioEvent::DenyNoFood, Some(race)) => race_slice_named("NoFood", race.suffix()),
        (AudioEvent::DenyNoFood, None) => &[
            "NoFoodHuman",
            "NoFoodOrc",
            "NoFoodNightElf",
            "NoFoodUndead",
        ],
        (AudioEvent::GoldMineCollapse, Some(race)) => {
            race_slice_named("GoldMineCollapse", race.suffix())
        }
        (AudioEvent::GoldMineCollapse, None) => &[
            "GoldMineCollapseHuman",
            "GoldMineCollapseNightElf",
            "GoldMineCollapseOrc",
            "GoldMineCollapseUndead",
        ],
        // Low-resource warnings: the race row first, the generic row second.
        (AudioEvent::LowGold, Some(race)) => race_low_gold(race),
        (AudioEvent::LowGold, None) => &[
            "GoldMineLowHuman",
            "GoldMineLowOrc",
            "GoldMineLowNightElf",
            "GoldMineLowUndead",
            "GoldMineLowGeneric",
            "NoGoldGeneric",
        ],
        (AudioEvent::LowFood, Some(race)) => race_low_food(race),
        (AudioEvent::LowFood, None) => &[
            "NoFoodHuman",
            "NoFoodOrc",
            "NoFoodNightElf",
            "NoFoodUndead",
            "UpkeepLevel",
        ],
        (AudioEvent::Waypoint, _) => &["RallyPointPlace", "MapPing"],
        (AudioEvent::ItemPickup, _) => &["ItemGet", "ItemReward"],
        (AudioEvent::ItemReward, _) => &["ItemReward", "ItemGet"],
        (AudioEvent::ItemDrop, _) => &["ItemDrop"],
        (AudioEvent::QuestNew, _) => &["QuestNew", "QuestLogModified"],
        (AudioEvent::QuestUpdate, _) => &["QuestUpdate", "QuestLogModified", "QuestNew"],
        (AudioEvent::QuestCompleted, _) => &["QuestCompleted"],
        (AudioEvent::QuestFailed, _) => &["QuestFailed"],
        // `AnimSounds.slk` ships the hero level-up cue (`Levelupcaster.wav`);
        // the event was previously reported as a gap.
        (AudioEvent::LevelUp, _) => &["LevelUp"],
        _ => &[],
    }
}

/// A single-name slice for a race-scoped row.
fn race_slice(name: &'static str) -> &'static [&'static str] {
    match name {
        "JobDoneSoundHuman" => &["JobDoneSoundHuman"],
        "JobDoneSoundOrc" => &["JobDoneSoundOrc"],
        "JobDoneSoundNightElf" => &["JobDoneSoundNightElf"],
        "JobDoneSoundUndead" => &["JobDoneSoundUndead"],
        _ => &[],
    }
}

/// `format!` is not const, so the race rows are spelled out.
fn race_slice_named(prefix: &str, suffix: &str) -> &'static [&'static str] {    match (prefix, suffix) {
        ("UpgradeComplete", "Human") => &["UpgradeCompleteHuman"],
        ("UpgradeComplete", "Orc") => &["UpgradeCompleteOrc"],
        ("UpgradeComplete", "NightElf") => &["UpgradeCompleteNightElf"],
        ("UpgradeComplete", "Undead") => &["UpgradeCompleteUndead"],
        ("HeroDies", "Human") => &["HeroDiesHuman"],
        ("HeroDies", "Orc") => &["HeroDiesOrc"],
        ("HeroDies", "NightElf") => &["HeroDiesNightElf"],
        ("HeroDies", "Undead") => &["HeroDiesUndead"],
        ("NoGold", "Human") => &["NoGoldHuman"],
        ("NoGold", "Orc") => &["NoGoldOrc"],
        ("NoGold", "NightElf") => &["NoGoldNightElf"],
        ("NoGold", "Undead") => &["NoGoldUndead"],
        ("NoLumber", "Human") => &["NoLumberHuman"],
        ("NoLumber", "Orc") => &["NoLumberOrc"],
        ("NoLumber", "NightElf") => &["NoLumberNightElf"],
        ("NoLumber", "Undead") => &["NoLumberUndead"],
        ("NoFood", "Human") => &["NoFoodHuman"],
        ("NoFood", "Orc") => &["NoFoodOrc"],
        ("NoFood", "NightElf") => &["NoFoodNightElf"],
        ("NoFood", "Undead") => &["NoFoodUndead"],
        ("GoldMineCollapse", "Human") => &["GoldMineCollapseHuman"],
        ("GoldMineCollapse", "Orc") => &["GoldMineCollapseOrc"],
        ("GoldMineCollapse", "NightElf") => &["GoldMineCollapseNightElf"],
        ("GoldMineCollapse", "Undead") => &["GoldMineCollapseUndead"],
        _ => &[],
    }
}

/// `GoldMineLow<Race>` with the shipped generic fallbacks behind it.
fn race_low_gold(race: Race) -> &'static [&'static str] {
    match race {
        Race::Human => &["GoldMineLowHuman", "GoldMineLowGeneric", "NoGoldGeneric"],
        Race::Orc => &["GoldMineLowOrc", "GoldMineLowGeneric", "NoGoldGeneric"],
        Race::NightElf => &["GoldMineLowNightElf", "GoldMineLowGeneric", "NoGoldGeneric"],
        Race::Undead => &["GoldMineLowUndead", "GoldMineLowGeneric", "NoGoldGeneric"],
    }
}

/// `NoFood<Race>`; `UpkeepLevel` is the shipped low-supply warning when a
/// localized table has no per-race row.
fn race_low_food(race: Race) -> &'static [&'static str] {
    match race {
        Race::Human => &["NoFoodHuman", "UpkeepLevel"],
        Race::Orc => &["NoFoodOrc", "UpkeepLevel"],
        Race::NightElf => &["NoFoodNightElf", "UpkeepLevel"],
        Race::Undead => &["NoFoodUndead", "UpkeepLevel"],
    }
}

impl<P: SoundProvider> GameAudio<P> {
    /// Play the retail sound for a gameplay event.
    ///
    /// `unit` is the four-letter id for unit-voice events; `race` selects the
    /// per-race rows (build/upgrade/deny/hero death). Ability casts are
    /// resolved by the caller through [`GameAudio::spell`] with the
    /// `AbilitySounds` entry. Returns `None` when the event has no data or the
    /// shipped file is missing from the provider.
    pub fn play_event(
        &mut self,
        event: AudioEvent,
        unit: Option<&str>,
        race: Option<Race>,
        position: Option<Vec3>,
    ) -> Option<VoiceId> {
        self.gated = false;
        self.stats.events_played += 1;
        // Which `SoundInfo` row (or unit sound set) actually answered.
        let mut def_name: Option<String> = None;
        // A limiter drop is not a data gap: the ack path already logged it.
        let limited_before = self.stats.acks_limited;
        let voice = match event {
            AudioEvent::Attack | AudioEvent::AttackOrder => {
                match unit.and_then(|id| self.unit_attack_definition(id)) {
                    Some((_, def)) => {
                        def_name = Some(def.name.clone());
                        unit.and_then(|id| self.unit_attack(id, position))
                    }
                    None => None,
                }
            }
            AudioEvent::Death => match unit.and_then(|id| self.unit_death_definition(id)) {
                Some((_, def)) => {
                    def_name = Some(def.name.clone());
                    unit.and_then(|id| self.unit_death(id, position))
                }
                None => None,
            },
            AudioEvent::WorkerAck | AudioEvent::MoveOrder | AudioEvent::Selected => {
                match unit.and_then(|id| self.unit_ack_definition(id)) {
                    Some((_, def)) => {
                        def_name = Some(def.name.clone());
                        unit.and_then(|id| self.unit_ack(id, position))
                    }
                    None => None,
                }
            }
            AudioEvent::TrainComplete => {
                let resolved = unit.and_then(|id| {
                    let set = self.unit_names().sound_set(id)?;
                    self.sound_table()
                        .unit_ready(set)
                        .map(|def| def.name.clone())
                });
                match resolved {
                    Some(name) => {
                        def_name = Some(name);
                        unit.and_then(|id| self.unit_ready(id, position))
                    }
                    None => None,
                }
            }
            AudioEvent::UiClick => self.ui_click(),
            AudioEvent::UiError => self.ui_error(),
            AudioEvent::AbilityCast => None,
            // The impact row needs the attacker's weapon family and the target's
            // material, so it has its own entry point; without that context the
            // event is reported as missing instead of guessing a row.
            AudioEvent::AttackImpact => None,
            AudioEvent::HeroDeath => {
                let defs = event_defs(event, race);
                match self.play_first_named(defs, position) {
                    Some((def, voice)) => {
                        def_name = Some(def);
                        Some(voice)
                    }
                    None => match unit.and_then(|id| self.unit_death_definition(id)) {
                        Some((_, def)) => {
                            def_name = Some(def.name.clone());
                            unit.and_then(|id| self.unit_death(id, position))
                        }
                        None => None,
                    },
                }
            }
            other => {
                if other.is_race_scoped() && race.is_none() {
                    None
                } else if let Some(file) = other.direct_file() {
                    // Shipped file with no table row (`ReceiveGold.wav`): play
                    // it on the interface bus, non-positional, and prove the
                    // path in the event line.
                    match self.provider().resolve(file) {
                        Some(path) => match self.provider_mut().load_sound(&path) {
                            Some(sound) => {
                                self.last_sound_file = Some(path.clone());
                                def_name = Some(format!("direct:{path}"));
                                Some(self.engine().play(
                                    sound,
                                    PlayParams {
                                        gain: 1.0,
                                        position: None,
                                        category: Category::Ui,
                                        priority: 6,
                                        ..Default::default()
                                    },
                                ))
                            }
                            None => None,
                        },
                        None => None,
                    }
                } else {
                    let defs = event_defs(other, race);
                    match self.play_first_named(defs, position) {
                        Some((def, voice)) => {
                            def_name = Some(def);
                            Some(voice)
                        }
                        None => None,
                    }
                }
            }
        };
        let Some(voice) = voice else {
            if self.stats.acks_limited > limited_before {
                // The per-unit ack limiter already logged `limited=1`.
                return None;
            }
            if self.gated {
                self.stats.events_gated += 1;
                return None;
            }
            self.report_missing_event(
                event,
                &format!(
                    "unit={} race={}",
                    unit.unwrap_or("-"),
                    race.map(Race::id).unwrap_or("-")
                ),
            );
            return None;
        };
        self.emit(
            "EVENT",
            &format!(
                "event={} def={} file={} unit={} race={}",
                event.id(),
                def_name.as_deref().unwrap_or("-"),
                self.last_sound_file.as_deref().unwrap_or("-"),
                unit.unwrap_or("-"),
                race.map(Race::id).unwrap_or("-")
            ),
            Some(voice),
        );
        Some(voice)
    }

    /// Play the first definition that exists in the sound table.
    pub fn play_first(&mut self, names: &[&str], position: Option<Vec3>) -> Option<VoiceId> {
        self.play_first_named(names, position).map(|(_, voice)| voice)
    }

    /// Play the first definition that exists, returning the definition name
    /// that started (for device logs and diagnostics).
    pub fn play_first_named(
        &mut self,
        names: &[&str],
        position: Option<Vec3>,
    ) -> Option<(String, VoiceId)> {
        for name in names {
            if self.sound_table().get(name).is_some() {
                if let Some(voice) = self.play_sound(name, position) {
                    return Some((name.to_string(), voice));
                }
                // The row's DistanceCutoff culled this source: do not fall
                // through to a fallback row with a longer range (a death at
                // 8 000 units must stay silent, not become `{set}What` with
                // cutoff 100000).
                if self.gated {
                    return None;
                }
            }
        }
        if let Some(first) = names.first() {
            self.note_unresolved(format!("row:{first}"));
        }
        None
    }

    /// Acknowledgement from a unit's own `UnitUI.unitSound` set
    /// (`{set}What` / `{set}Yes`; the numbered entries are files inside the row).
    pub fn unit_ack_set(
        &mut self,
        set: &str,
        position: Option<Vec3>,
    ) -> Option<(String, VoiceId)> {
        let names = [format!("{set}What"), format!("{set}Yes")];
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let result = self.play_first_named(&refs, position);
        self.emit(
            "ACK_SET",
            &format!("set={set} def={}", result.as_ref().map(|(def, _)| def.as_str()).unwrap_or("none")),
            result.as_ref().map(|(_, voice)| *voice),
        );
        result
    }

    /// Attack acknowledgement from the unit's own set
    /// (`{set}YesAttack`, `{set}What` fallback).
    pub fn unit_attack_set(
        &mut self,
        set: &str,
        position: Option<Vec3>,
    ) -> Option<(String, VoiceId)> {
        let names = [format!("{set}YesAttack"), format!("{set}What")];
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let result = self.play_first_named(&refs, position);
        self.emit(
            "ATTACK_SET",
            &format!("set={set} def={}", result.as_ref().map(|(def, _)| def.as_str()).unwrap_or("none")),
            result.as_ref().map(|(_, voice)| *voice),
        );
        result
    }

    /// Death line from the unit's own set (`{set}Death`).
    ///
    /// No `{set}What` fallback: ack rows are audible at 100 000 units and would
    /// turn a silent building death into a map-wide voice line.
    pub fn unit_death_set(
        &mut self,
        set: &str,
        position: Option<Vec3>,
    ) -> Option<(String, VoiceId)> {
        let names = [format!("{set}Death")];
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let result = self.play_first_named(&refs, position);
        self.emit(
            "DEATH_SET",
            &format!("set={set} def={}", result.as_ref().map(|(def, _)| def.as_str()).unwrap_or("none")),
            result.as_ref().map(|(_, voice)| *voice),
        );
        result
    }

    /// Training-complete line from the unit's own set (`{set}Ready`).
    pub fn unit_ready_set(
        &mut self,
        set: &str,
        position: Option<Vec3>,
    ) -> Option<(String, VoiceId)> {
        let names = [format!("{set}Ready"), format!("{set}What")];
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let result = self.play_first_named(&refs, position);
        self.emit(
            "READY_SET",
            &format!("set={set} def={}", result.as_ref().map(|(def, _)| def.as_str()).unwrap_or("none")),
            result.as_ref().map(|(_, voice)| *voice),
        );
        result
    }

    /// Training-complete line for a unit id (own sound set).
    pub fn unit_ready(&mut self, unit_id: &str, position: Option<Vec3>) -> Option<VoiceId> {
        let set = self.unit_names().sound_set(unit_id)?.to_string();
        self.unit_ready_set(&set, position).map(|(_, voice)| voice)
    }

    /// Building-finished cue for one race (`JobDoneSound<Race>`).
    pub fn job_done(&mut self, race: Race, position: Option<Vec3>) -> Option<VoiceId> {
        self.play_event(AudioEvent::BuildComplete, None, Some(race), position)
    }

    /// Race/state music selection (playing loop, victory or defeat stinger).
    ///
    /// An explicit selection ends the match playlist: the victory/defeat stinger
    /// must not be replaced by the next in-game track when the old track length
    /// elapses.
    pub fn music_for_race(&mut self, race: Race, state: MusicState) -> Option<VoiceId> {
        self.clear_playlist();
        for name in race.theme_names(state) {
            if let Some(voice) = self.music(name) {
                return Some(voice);
            }
        }
        self.note_unresolved(format!("music_race:{} {:?}", race.id(), state));
        None
    }

    /// Start the race's in-game music: the hash-ordered playlist (§3), so the
    /// existing game call gets the rotation and the cross-fade without a new
    /// code path. Victory/defeat still go through [`GameAudio::music_for_race`].
    pub fn music_for_player_race(&mut self, race: Race) -> Option<VoiceId> {
        self.start_race_playlist(race)
    }
}
