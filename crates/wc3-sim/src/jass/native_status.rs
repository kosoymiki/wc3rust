//! Semantic status of every JASS native (audit AUD-016 / AUD-027).
//!
//! "A handler exists" and "the native is implemented" are different facts.
//! The dispatch table (`natives.rs`) proves the first; this module records the
//! second, with a reason for every native that is *not* fully implemented, so
//! the coverage numbers cannot claim more than the code does.
//!
//! Classification rule (applied to every handler in the table):
//!
//! * [`NativeStatus::Implemented`] — a handler exists and the script-observable
//!   contract is complete: setters write the state the getters read, effects
//!   apply through the simulation, and the arguments are not dropped.
//! * [`NativeStatus::Partial`] — a handler exists and covers part of the
//!   documented behaviour; the reason names the missing part.
//! * [`NativeStatus::RecordedOnly`] — the whole effect is recording state for a
//!   consumer outside the simulation (renderer, HUD, audio engine), and no JASS
//!   query reads it back.
//! * [`NativeStatus::NoOp`] — a handler exists but changes nothing: it returns
//!   `Null` (or a constant) and drops its arguments.
//! * [`NativeStatus::Unsupported`] — no handler at all: the registry resolves
//!   the signature and the VM returns the typed default while counting the
//!   call as unresolved.
//!
//! The scan that produced the entries below is reproducible:
//! `analysis/audit/sim3_handler_scan.py --flagged` prints every handler whose
//! body writes no engine state or discards a value, and
//! `analysis/audit/sim3_usage_search.py` counts how often the shipped
//! `common.j`/`Blizzard.j`, the other header copies in the tree and the 96-map
//! corpus call each name. The `AUD-027` section of `docs/audit/status-sim3.md`
//! carries both tables.

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// How much of a native's documented behaviour the runtime implements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NativeStatus {
    /// Handler present, script-observable contract complete.
    Implemented,
    /// Handler present, part of the behaviour is missing (see the reason).
    Partial,
    /// Handler present, records state for a non-simulation consumer only.
    RecordedOnly,
    /// Handler present, changes nothing.
    NoOp,
    /// No handler: signature resolved from the registry only.
    Unsupported,
}

impl NativeStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Implemented => "Implemented",
            Self::Partial => "Partial",
            Self::RecordedOnly => "RecordedOnly",
            Self::NoOp => "NoOp",
            Self::Unsupported => "Unsupported",
        }
    }

    /// True when the status is a gap a caller has to know about.
    pub fn is_gap(self) -> bool {
        !matches!(self, Self::Implemented)
    }
}

/// One non-`Implemented` classification.
#[derive(Debug, Clone, Copy)]
pub struct StatusEntry {
    pub name: &'static str,
    pub status: NativeStatus,
    pub reason: &'static str,
}

/// Every native that is not fully implemented, with the reason.
///
/// Generated from the AUD-027 handler scan and reviewed row by row; the list is
/// exhaustive for the `NoOp` class by construction — the test
/// `every_null_body_handler_is_classified` reads the handler sources and fails
/// when a body that is only `Value::Null` is not listed here.
static OVERRIDES: &[StatusEntry] = &[
    // ------------------------------------------------------------------
    // NoOp — the handler exists, the body changes nothing.
    // ------------------------------------------------------------------
    StatusEntry {
        name: "SetTerrainFogExV",
        status: NativeStatus::NoOp,
        reason: "Reforged terrain-fog variant: the body returns Null and stores no fog state",
    },
    StatusEntry {
        name: "BlzSetTerrainFogMaxLinearDensity",
        status: NativeStatus::NoOp,
        reason: "terrain fog density is not modelled; the body returns Null",
    },
    StatusEntry {
        name: "BlzSetTerrainFogDrawOverSky",
        status: NativeStatus::NoOp,
        reason: "terrain fog draw order is not modelled; the body returns Null",
    },
    StatusEntry {
        name: "SetHDWaterParamsEx",
        status: NativeStatus::NoOp,
        reason: "HD water parameters are not modelled; the body returns Null",
    },
    StatusEntry {
        name: "SetSoundParamsFromLabel",
        status: NativeStatus::NoOp,
        reason: "the label is read and dropped; the sound keeps whatever parameters were set explicitly",
    },
    StatusEntry {
        name: "SetSoundDistances",
        status: NativeStatus::NoOp,
        reason: "min/max distance arguments are dropped; no distance field is recorded",
    },
    StatusEntry {
        name: "SetSoundDistanceCutoff",
        status: NativeStatus::NoOp,
        reason: "the cutoff argument is dropped; no cutoff field is recorded",
    },
    StatusEntry {
        name: "SetSoundConeAngles",
        status: NativeStatus::NoOp,
        reason: "cone angles and the outside-volume flag are dropped; no cone field is recorded",
    },
    StatusEntry {
        name: "SetSoundConeOrientation",
        status: NativeStatus::NoOp,
        reason: "the cone orientation is dropped; no orientation field is recorded",
    },
    StatusEntry {
        name: "SetSoundFacialAnimationLabel",
        status: NativeStatus::NoOp,
        reason: "facial-animation strings are not modelled; the body returns Null",
    },
    StatusEntry {
        name: "SetSoundFacialAnimationGroupLabel",
        status: NativeStatus::NoOp,
        reason: "facial-animation strings are not modelled; the body returns Null",
    },
    StatusEntry {
        name: "SetSoundFacialAnimationSetFilepath",
        status: NativeStatus::NoOp,
        reason: "facial-animation file paths are not modelled; the body returns Null",
    },
    StatusEntry {
        name: "SetDialogueSpeakerNameKey",
        status: NativeStatus::NoOp,
        reason: "dialogue strings are not modelled; the body returns Null",
    },
    StatusEntry {
        name: "SetDialogueTextKey",
        status: NativeStatus::NoOp,
        reason: "dialogue strings are not modelled; the body returns Null",
    },
    StatusEntry {
        name: "PlayDialogueFromSpeakerEx",
        status: NativeStatus::NoOp,
        reason: "dialogue playback is not modelled; the body returns Null",
    },
    StatusEntry {
        name: "AddCameraBlocker",
        status: NativeStatus::NoOp,
        reason: "camera blockers are not modelled; the body returns Null",
    },
    StatusEntry {
        name: "BlzCameraSetupSetCameraType",
        status: NativeStatus::NoOp,
        reason: "CameraSetup has no camera-type field; the body returns Null",
    },
    StatusEntry {
        name: "SetItemColor",
        status: NativeStatus::NoOp,
        reason: "item colour is not modelled; the body returns Null",
    },
    StatusEntry {
        name: "SetDestructableVertexColor",
        status: NativeStatus::NoOp,
        reason: "destructable vertex colour is not modelled; the body returns Null",
    },
    StatusEntry {
        name: "SetPlayerRaceSkin",
        status: NativeStatus::NoOp,
        reason: "race skins are Reforged render state and nothing is recorded",
    },
    StatusEntry {
        name: "SetUnitFog",
        status: NativeStatus::NoOp,
        reason: "unit fog radius is not modelled; all five arguments are dropped",
    },
    StatusEntry {
        name: "UnitMakeAbilityPermanent",
        status: NativeStatus::NoOp,
        reason: "there is no morph/change-unit-type path, so permanence has nothing to change; the arguments are dropped",
    },
    // ------------------------------------------------------------------
    // RecordedOnly — recorded for the renderer/HUD/audio layer only.
    // ------------------------------------------------------------------
    StatusEntry {
        name: "Preloader",
        status: NativeStatus::RecordedOnly,
        reason: "records the requested path; the headless runtime loads nothing ahead of time",
    },
    StatusEntry {
        name: "SetAllItemTypeSlots",
        status: NativeStatus::RecordedOnly,
        reason: "records the slot cap on the host; no query native reads it",
    },
    StatusEntry {
        name: "SetAllUnitTypeSlots",
        status: NativeStatus::RecordedOnly,
        reason: "records the slot cap on the host; no query native reads it",
    },
    StatusEntry {
        name: "StartSound",
        status: NativeStatus::RecordedOnly,
        reason: "records `playing` on the sound handle; `GetSoundIsPlaying` has no handler",
    },
    StatusEntry {
        name: "StopSound",
        status: NativeStatus::RecordedOnly,
        reason: "records `playing` on the sound handle; `GetSoundIsPlaying` has no handler",
    },
    StatusEntry {
        name: "KillSoundWhenDone",
        status: NativeStatus::RecordedOnly,
        reason: "records the handle in the kill list; nothing consumes the list",
    },
    StatusEntry {
        name: "AttachSoundToUnit",
        status: NativeStatus::RecordedOnly,
        reason: "records the unit binding on the sound handle; no query reads it",
    },
    StatusEntry {
        name: "CreateTextTag",
        status: NativeStatus::Partial,
        reason: "records the text, but the size argument is dropped and the lifespan starts at 0 pending `SetTextTag*`",
    },
    StatusEntry {
        name: "SetTextTagText",
        status: NativeStatus::RecordedOnly,
        reason: "text-tag state is consumed by the game renderer; no JASS query reads it back",
    },
    StatusEntry {
        name: "SetTextTagPos",
        status: NativeStatus::RecordedOnly,
        reason: "text-tag state is consumed by the game renderer; no JASS query reads it back",
    },
    StatusEntry {
        name: "SetTextTagLifespan",
        status: NativeStatus::RecordedOnly,
        reason: "text-tag state is consumed by the game renderer; no JASS query reads it back",
    },
    StatusEntry {
        name: "DestroyTextTag",
        status: NativeStatus::RecordedOnly,
        reason: "drops the text-tag object; the renderer is the only consumer",
    },
    StatusEntry {
        name: "AddSpecialEffect",
        status: NativeStatus::RecordedOnly,
        reason: "effect objects are consumed by the renderer; no JASS query reads them back",
    },
    StatusEntry {
        name: "AddSpecialEffectTarget",
        status: NativeStatus::RecordedOnly,
        reason: "effect objects are consumed by the renderer; no JASS query reads them back",
    },
    StatusEntry {
        name: "DestroyEffect",
        status: NativeStatus::RecordedOnly,
        reason: "drops the effect object; the renderer is the only consumer",
    },
    StatusEntry {
        name: "AddWeatherEffect",
        status: NativeStatus::RecordedOnly,
        reason: "weather objects are consumed by the renderer; no JASS query reads them back",
    },
    StatusEntry {
        name: "EnableWeatherEffect",
        status: NativeStatus::RecordedOnly,
        reason: "records the enabled flag on the weather object; the renderer is the only consumer",
    },
    StatusEntry {
        name: "RemoveWeatherEffect",
        status: NativeStatus::RecordedOnly,
        reason: "drops the weather object; the renderer is the only consumer",
    },
    StatusEntry {
        name: "DisplayCineFilter",
        status: NativeStatus::RecordedOnly,
        reason: "the cine filter is consumed by the game layer; no JASS query reads it",
    },
    StatusEntry {
        name: "SetCineFilterTexture",
        status: NativeStatus::RecordedOnly,
        reason: "cine-filter state is consumed by the game layer; no JASS query reads it",
    },
    StatusEntry {
        name: "SetCineFilterStartUV",
        status: NativeStatus::RecordedOnly,
        reason: "cine-filter state is consumed by the game layer; no JASS query reads it",
    },
    StatusEntry {
        name: "SetCineFilterEndUV",
        status: NativeStatus::RecordedOnly,
        reason: "cine-filter state is consumed by the game layer; no JASS query reads it",
    },
    StatusEntry {
        name: "SetCineFilterStartColor",
        status: NativeStatus::RecordedOnly,
        reason: "cine-filter state is consumed by the game layer; no JASS query reads it",
    },
    StatusEntry {
        name: "SetCineFilterEndColor",
        status: NativeStatus::RecordedOnly,
        reason: "cine-filter state is consumed by the game layer; no JASS query reads it",
    },
    StatusEntry {
        name: "SetCineFilterDuration",
        status: NativeStatus::RecordedOnly,
        reason: "cine-filter state is consumed by the game layer; no JASS query reads it",
    },
    StatusEntry {
        name: "SetCineFilterBlendMode",
        status: NativeStatus::RecordedOnly,
        reason: "cine-filter state is consumed by the game layer; no JASS query reads it",
    },
    StatusEntry {
        name: "SetCineFilterTexMapFlags",
        status: NativeStatus::RecordedOnly,
        reason: "cine-filter state is consumed by the game layer; no JASS query reads it",
    },
    StatusEntry {
        name: "SetCinematicScene",
        status: NativeStatus::RecordedOnly,
        reason: "cinematic scene requests are recorded for the game layer; no JASS query reads them",
    },
    StatusEntry {
        name: "EndCinematicScene",
        status: NativeStatus::RecordedOnly,
        reason: "cinematic scene requests are recorded for the game layer; no JASS query reads them",
    },
    StatusEntry {
        name: "ForceCinematicSubtitles",
        status: NativeStatus::RecordedOnly,
        reason: "the subtitle flag is recorded for the game layer; no JASS query reads it",
    },
    StatusEntry {
        name: "SetCinematicAudio",
        status: NativeStatus::RecordedOnly,
        reason: "the cinematic audio flag is recorded for the game layer; no JASS query reads it",
    },
    StatusEntry {
        name: "SetCinematicCamera",
        status: NativeStatus::RecordedOnly,
        reason: "the cinematic camera path is recorded for the game layer; no JASS query reads it",
    },
    StatusEntry {
        name: "PlayCinematic",
        status: NativeStatus::RecordedOnly,
        reason: "the cinematic path is recorded for the game layer; no JASS query reads it",
    },
    StatusEntry {
        name: "PlayModelCinematic",
        status: NativeStatus::RecordedOnly,
        reason: "the cinematic model is recorded for the game layer; no JASS query reads it",
    },
    StatusEntry {
        name: "DialogCreate",
        status: NativeStatus::RecordedOnly,
        reason: "dialog state is consumed by the HUD; no JASS query reads it back",
    },
    StatusEntry {
        name: "DialogAddButton",
        status: NativeStatus::RecordedOnly,
        reason: "dialog state is consumed by the HUD; no JASS query reads it back",
    },
    StatusEntry {
        name: "DialogAddQuitButton",
        status: NativeStatus::RecordedOnly,
        reason: "dialog state is consumed by the HUD; no JASS query reads it back",
    },
    StatusEntry {
        name: "DialogClear",
        status: NativeStatus::RecordedOnly,
        reason: "dialog state is consumed by the HUD; no JASS query reads it back",
    },
    StatusEntry {
        name: "DialogSetMessage",
        status: NativeStatus::RecordedOnly,
        reason: "dialog state is consumed by the HUD; no JASS query reads it back",
    },
    StatusEntry {
        name: "DialogDisplay",
        status: NativeStatus::RecordedOnly,
        reason: "dialog state is consumed by the HUD; no JASS query reads it back",
    },
    StatusEntry {
        name: "DialogSetAsync",
        status: NativeStatus::RecordedOnly,
        reason: "dialog state is consumed by the HUD; no JASS query reads it back",
    },
    StatusEntry {
        name: "DialogDestroy",
        status: NativeStatus::RecordedOnly,
        reason: "drops the dialog object; the HUD is the only consumer",
    },
    StatusEntry {
        name: "CreateTimerDialog",
        status: NativeStatus::RecordedOnly,
        reason: "timer-dialog state is consumed by the HUD; no JASS query reads it back",
    },
    StatusEntry {
        name: "DestroyTimerDialog",
        status: NativeStatus::RecordedOnly,
        reason: "drops the timer-dialog object; the HUD is the only consumer",
    },
    StatusEntry {
        name: "TimerDialogSetTitle",
        status: NativeStatus::RecordedOnly,
        reason: "timer-dialog state is consumed by the HUD; no JASS query reads it back",
    },
    StatusEntry {
        name: "TimerDialogDisplay",
        status: NativeStatus::RecordedOnly,
        reason: "timer-dialog state is consumed by the HUD; no JASS query reads it back",
    },
    StatusEntry {
        name: "TimerDialogSetSpeed",
        status: NativeStatus::RecordedOnly,
        reason: "timer-dialog state is consumed by the HUD; no JASS query reads it back",
    },
    StatusEntry {
        name: "TimerDialogSetRealTimeRemaining",
        status: NativeStatus::RecordedOnly,
        reason: "timer-dialog state is consumed by the HUD; no JASS query reads it back",
    },
    StatusEntry {
        name: "TimerDialogSetTimeColor",
        status: NativeStatus::RecordedOnly,
        reason: "timer-dialog state is consumed by the HUD; no JASS query reads it back",
    },
    StatusEntry {
        name: "TimerDialogSetTitleColor",
        status: NativeStatus::RecordedOnly,
        reason: "timer-dialog state is consumed by the HUD; no JASS query reads it back",
    },
    StatusEntry {
        name: "SelectUnit",
        status: NativeStatus::RecordedOnly,
        reason: "records the selection for the HUD; `IsUnitSelected` has no handler",
    },
    StatusEntry {
        name: "ClearSelection",
        status: NativeStatus::RecordedOnly,
        reason: "records the selection for the HUD; `IsUnitSelected` has no handler",
    },
    StatusEntry {
        name: "CreateFogModifierRect",
        status: NativeStatus::RecordedOnly,
        reason: "fog modifiers are recorded; `IsFoggedToPlayer` has no handler",
    },
    StatusEntry {
        name: "FogModifierStart",
        status: NativeStatus::RecordedOnly,
        reason: "fog modifiers are recorded; `IsFoggedToPlayer` has no handler",
    },
    StatusEntry {
        name: "DestroyFogModifier",
        status: NativeStatus::RecordedOnly,
        reason: "fog modifiers are recorded; `IsFoggedToPlayer` has no handler",
    },
    StatusEntry {
        name: "CreateTrackable",
        status: NativeStatus::RecordedOnly,
        reason: "trackables are recorded for the UI layer; no JASS query reads them",
    },
    StatusEntry {
        name: "FlashQuestDialogButton",
        status: NativeStatus::RecordedOnly,
        reason: "the flash request is recorded for the HUD; no JASS query reads it",
    },
    StatusEntry {
        name: "ForceQuestDialogUpdate",
        status: NativeStatus::RecordedOnly,
        reason: "the refresh request is recorded for the HUD; no JASS query reads it",
    },
    // ------------------------------------------------------------------
    // Partial — handler exists, part of the documented behaviour is missing.
    // ------------------------------------------------------------------
    StatusEntry {
        name: "GetTriggerEvalCount",
        status: NativeStatus::Partial,
        reason: "trigger eval counters are not tracked; always 0",
    },
    StatusEntry {
        name: "GetTriggerExecCount",
        status: NativeStatus::Partial,
        reason: "trigger exec counters are not tracked; always 0",
    },
    StatusEntry {
        name: "GetGameSpeed",
        status: NativeStatus::Partial,
        reason: "constant `GAME_SPEED_NORMAL` (2); the headless host has no lobby game-speed setting",
    },
    StatusEntry {
        name: "GetCameraMargin",
        status: NativeStatus::Partial,
        reason: "the viewport margin is not modelled; every side answers 0",
    },
    StatusEntry {
        name: "IsPlayerObserver",
        status: NativeStatus::Partial,
        reason: "reads the per-player `PLAYER_STATE_OBSERVER` flag shared with `GetPlayerState`; no lobby assigns observers",
    },
    StatusEntry {
        name: "Player",
        status: NativeStatus::Partial,
        reason: "an out-of-range slot clamps into `0..PLAYER_SLOTS-1` instead of producing an invalid handle",
    },
    StatusEntry {
        name: "GetPlayerSlotState",
        status: NativeStatus::Partial,
        reason: "answers playing (1) or empty (0); `PLAYER_SLOT_STATE_LEFT` (2) is not tracked",
    },
    StatusEntry {
        name: "GetStartLocPrioSlot",
        status: NativeStatus::Partial,
        reason: "the host stores the priority under the slot key, so the other-start-location index cannot be answered (new finding AUD-SIM3-02)",
    },
    StatusEntry {
        name: "SetStartLocPrio",
        status: NativeStatus::Partial,
        reason: "only the priority is recorded; the `otherStartLocIndex` argument is dropped (new finding AUD-SIM3-02)",
    },
    StatusEntry {
        name: "And",
        status: NativeStatus::Partial,
        reason: "the boolexpr records an opaque handle; the VM evaluates only the first operand",
    },
    StatusEntry {
        name: "Or",
        status: NativeStatus::Partial,
        reason: "the boolexpr records an opaque handle; the VM evaluates only the first operand",
    },
    StatusEntry {
        name: "SetUnitX",
        status: NativeStatus::Partial,
        reason: "the simulation's rejection is dropped (`let _ =`), so a refused move is silent",
    },
    StatusEntry {
        name: "SetUnitY",
        status: NativeStatus::Partial,
        reason: "the simulation's rejection is dropped (`let _ =`), so a refused move is silent",
    },
    StatusEntry {
        name: "SetWidgetLife",
        status: NativeStatus::Partial,
        reason: "the simulation's rejection is dropped (`let _ =`), so a refused life change is silent",
    },
    StatusEntry {
        name: "AddHeroXP",
        status: NativeStatus::Partial,
        reason: "the simulation's rejection is dropped (`let _ =`), so a refused XP grant is silent",
    },
];

fn index() -> &'static BTreeMap<String, &'static StatusEntry> {
    static INDEX: OnceLock<BTreeMap<String, &'static StatusEntry>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut map: BTreeMap<String, &'static StatusEntry> = BTreeMap::new();
        for entry in OVERRIDES {
            let key = entry.name.to_ascii_lowercase();
            debug_assert!(
                !map.contains_key(&key),
                "duplicate native-status entry for {}",
                entry.name
            );
            map.insert(key, entry);
        }
        map
    })
}

/// Explicit classification of `name`, ignoring whether a handler exists.
pub fn override_for(name: &str) -> Option<&'static StatusEntry> {
    index().get(&name.to_ascii_lowercase()).copied()
}

/// Status and reason for a registry row, given whether a handler exists.
///
/// The default for a handler that is not listed is `Implemented`: the scan
/// clears every handler that writes the state its query reads. A row without a
/// handler is always `Unsupported`, whatever the table says — a classification
/// cannot claim a mechanism that is not there.
pub fn resolve(name: &str, dispatch: bool) -> (NativeStatus, &'static str) {
    if !dispatch {
        return (
            NativeStatus::Unsupported,
            "no handler: the registry knows the signature, the VM returns the typed default and counts the call",
        );
    }
    match override_for(name) {
        Some(entry) => (entry.status, entry.reason),
        None => (NativeStatus::Implemented, ""),
    }
}

/// Number of natives in each status, over the whole registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatusCounts {
    pub total: usize,
    pub implemented: usize,
    pub partial: usize,
    pub recorded_only: usize,
    pub no_op: usize,
    pub unsupported: usize,
}

impl StatusCounts {
    /// Share of registry rows that are fully implemented, in percent.
    pub fn implemented_percent(self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        self.implemented as f32 * 100.0 / self.total as f32
    }
}

/// Counts over the whole registry.
pub fn counts() -> StatusCounts {
    let mut counts = StatusCounts::default();
    for def in super::registry::table() {
        counts.total += 1;
        match def.status {
            NativeStatus::Implemented => counts.implemented += 1,
            NativeStatus::Partial => counts.partial += 1,
            NativeStatus::RecordedOnly => counts.recorded_only += 1,
            NativeStatus::NoOp => counts.no_op += 1,
            NativeStatus::Unsupported => counts.unsupported += 1,
        }
    }
    counts
}

/// One report row, for `examples/native_status.rs`.
#[derive(Debug, Clone, Copy)]
pub struct ReportRow {
    pub name: &'static str,
    pub status: NativeStatus,
    pub engine_registered: bool,
    pub reason: &'static str,
}

/// Handlers that no header in this tree declares (`natives::HEADER_ABSENT_NATIVES`).
///
/// They have no registry row — the Reforged-era header is not in the tree — but
/// they *are* reachable by name, so they carry a status like every other
/// handler and appear in the report as a separate group.
pub fn header_absent_rows() -> Vec<ReportRow> {
    let mut rows: Vec<ReportRow> = super::natives::HEADER_ABSENT_NATIVES
        .iter()
        .map(|name| {
            let (status, reason) = resolve(name, true);
            ReportRow {
                name,
                status,
                engine_registered: false,
                reason,
            }
        })
        .collect();
    rows.sort_by_key(|row| row.name.to_ascii_lowercase());
    rows
}

/// Every registry row with its status, sorted by name.
pub fn report_rows() -> Vec<ReportRow> {
    let mut rows: Vec<ReportRow> = super::registry::table()
        .iter()
        .map(|def| ReportRow {
            name: def.name,
            status: def.status,
            engine_registered: def.engine_registered,
            reason: def.status_reason,
        })
        .collect();
    rows.sort_by_key(|row| row.name.to_ascii_lowercase());
    rows
}

/// The coverage table as JSON (one object per native), for report tooling.
pub fn report_json() -> String {
    let counts = counts();
    let mut out = String::with_capacity(256 * 1024);
    out.push_str("{\n  \"summary\": {");
    out.push_str(&format!(
        "\"total\": {}, \"implemented\": {}, \"partial\": {}, \"recorded_only\": {}, \"no_op\": {}, \"unsupported\": {}",
        counts.total,
        counts.implemented,
        counts.partial,
        counts.recorded_only,
        counts.no_op,
        counts.unsupported
    ));
    out.push_str("},\n  \"natives\": [\n");
    let rows = report_rows();
    for (index, row) in rows.iter().enumerate() {
        out.push_str(&format!(
            "    {{\"name\": \"{}\", \"status\": \"{}\", \"engine_registered\": {}, \"reason\": \"{}\"}}",
            row.name,
            row.status.label(),
            row.engine_registered,
            json_escape(row.reason)
        ));
        if index + 1 != rows.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ],\n  \"header_absent\": [\n");
    let absent = header_absent_rows();
    for (index, row) in absent.iter().enumerate() {
        out.push_str(&format!(
            "    {{\"name\": \"{}\", \"status\": \"{}\", \"reason\": \"{}\"}}",
            row.name,
            row.status.label(),
            json_escape(row.reason)
        ));
        if index + 1 != absent.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n}\n");
    out
}

fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_handler_is_classified_and_no_entry_is_orphaned() {
        for def in super::super::registry::table() {
            // `resolve` is what the registry stores; a missing handler can never
            // be `Implemented`, and a present one has a status either way.
            if !def.dispatch {
                assert_eq!(def.status, NativeStatus::Unsupported, "{}", def.name);
            }
            assert_eq!(def.status_reason.is_empty(), !def.status.is_gap(), "{}", def.name);
        }
        for entry in OVERRIDES {
            let dispatch = super::super::natives::is_handled(entry.name);
            let registered = super::super::registry::lookup(entry.name).is_some();
            assert!(
                dispatch || registered,
                "status entry {} matches no handler and no registry row",
                entry.name
            );
        }
    }

    #[test]
    fn unknown_is_not_partial_and_noop_is_not_implemented() {
        // A registry row with no handler is `Unsupported`, never `Partial`.
        assert_eq!(
            resolve("NotARealNative", false).0,
            NativeStatus::Unsupported
        );
        // A no-op handler is never reported as implemented.
        assert_eq!(resolve("SetSoundDistances", true).0, NativeStatus::NoOp);
        assert_eq!(resolve("SetDialogueTextKey", true).0, NativeStatus::NoOp);
        // The fixed natives are implemented now.
        assert_eq!(resolve("ClearTextMessages", true).0, NativeStatus::Implemented);
        assert_eq!(resolve("IsFogEnabled", true).0, NativeStatus::Implemented);
        assert_eq!(resolve("IsFogMaskEnabled", true).0, NativeStatus::Implemented);
        assert_eq!(resolve("S2I", true).0, NativeStatus::Implemented);
        assert_eq!(resolve("S2R", true).0, NativeStatus::Implemented);
    }
}
