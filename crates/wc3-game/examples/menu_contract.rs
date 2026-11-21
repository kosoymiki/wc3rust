//! Headless proof of the menu model + §8 command/input contract (ledger 554).
//!
//! Runs the real `wc3_game::hud::menu` model and the real
//! `wc3_input::menu` pointer/gate against scripted gestures. It is a runnable
//! evidence artifact, not a test module: `cargo run -p wc3-game --example
//! menu_contract` prints one `CHECK` line per contract rule and exits non-zero
//! when a rule fails (so CI/device scripts can gate on it).
//!
//! What it proves:
//!
//! * the model is deterministic (same state + facts → same fingerprint);
//! * one physical tap produces exactly one command (mirror duplicate rejected
//!   at the gesture layer *and*, independently, at the command gate — even when
//!   the second copy lands on a different row after a transition);
//! * a drag-off produces zero commands;
//! * a locked/disabled row never activates and reports its reason;
//! * BACK returns to the screen that opened the picker/settings (pause →
//!   settings → pause, campaign → main), not always to main;
//! * save overwrite needs the second tap; settings apply/cancel are explicit.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-game --example menu_contract --offline
//! ```

use glam::Vec2;

use wc3_game::hud::menu::{
    AudioSettings, CampaignEntry, CampaignState, CommandId, CommandVerdict, HostScreen,
    MenuCommandGate, MenuContext, MenuItem, MenuPage, MenuState, RaceChoice, SaveLoadMode,
    SaveSlotInfo, SaveSlotKind, SaveSlotState, SettingKey,
};
use wc3_input::menu::{
    ActivationGate, GateDecision, MENU_MIN_TARGET_DP, MenuPointer, PointerContext, PointerRoute,
    PointerTarget, PressOutcome, ReleaseOutcome, hit_menu, route_pointer,
};
use wc3_input::{Rect, TouchButton};

const SCALE: f32 = 3.0;

struct Checks {
    ok: usize,
    failed: usize,
}

impl Checks {
    fn new() -> Self {
        Self { ok: 0, failed: 0 }
    }

    fn check(&mut self, name: &str, passed: bool, detail: impl std::fmt::Display) {
        if passed {
            self.ok += 1;
            println!("CHECK {name}=ok {detail}");
        } else {
            self.failed += 1;
            println!("CHECK {name}=FAIL {detail}");
        }
    }
}

fn maps() -> Vec<wc3_game::hud::menu::MapChoice> {
    (0..12)
        .map(|index| wc3_game::hud::menu::MapChoice {
            name: format!("Melee Map {index:02}"),
            path: format!("Maps/Melee/map{index:02}.w3m"),
            players: 2 + (index % 3) as u8,
        })
        .collect()
}

fn chapters() -> Vec<CampaignEntry> {
    let mut rows = Vec::new();
    let mut index = 0;
    for group in 0..6 {
        let count = match group {
            0 => 1,
            1 => 7,
            2 => 6,
            3 => 6,
            4 => 8,
            _ => 2,
        };
        for number in 1..=count {
            rows.push(CampaignEntry {
                index,
                id: format!("Chapter{group}{number}"),
                title: format!("CHAPTER {number} OF GROUP {group}"),
                number: number as u32,
                group,
                state: if number <= 2 {
                    CampaignState::Cleared
                } else if number == 3 {
                    CampaignState::Unlocked
                } else {
                    CampaignState::Locked
                },
                interlude: number == count,
            });
            index += 1;
        }
    }
    rows
}

fn facts(host: HostScreen, has_save: bool) -> MenuContext {
    let rows = chapters();
    let mut counts = vec![0usize; 6];
    for row in &rows {
        counts[row.group] += 1;
    }
    let mut state = MenuState::new(maps());
    state.campaign_counts = counts;
    MenuContext {
        host,
        map_name: "Melee Map 00".to_string(),
        chapters: rows,
        chapter_count: state.campaign_counts.iter().sum(),
        has_save,
        save_api: true,
        save_reason: None,
        slots: vec![
            SaveSlotInfo {
                kind: SaveSlotKind::Quick,
                state: SaveSlotState::Ready {
                    map: "Melee Map 00".to_string(),
                    tick: 1234,
                    saved_at: Some("2026-11-18 16:20".to_string()),
                },
            },
            SaveSlotInfo::empty(SaveSlotKind::Autosave),
        ],
        quit_armed: false,
        loading_progress: 0.0,
        save_loading: false,
        end_stats: Some("KILLS 4   TRAINED 12   ARMY 9   TIME 3:12".to_string()),
        audio: Some(AudioSettings::FULL),
    }
}

fn main() {
    let mut checks = Checks::new();

    // ------------------------------------------------------------ determinism
    let state = MenuState::new(maps());
    let main_facts = facts(HostScreen::MainMenu, true);
    let model_a = state.model(&main_facts);
    let model_b = state.model(&main_facts);
    checks.check(
        "model_deterministic",
        model_a == model_b && model_a.fingerprint() == model_b.fingerprint(),
        format!(
            "items={} fingerprint={:016x}",
            model_a.items.len(),
            model_a.fingerprint()
        ),
    );
    println!(
        "MODEL screen={} title={:?} items={} fingerprint={:016x}",
        model_a.screen,
        model_a.title,
        model_a.items.len(),
        model_a.fingerprint()
    );
    for item in &model_a.items {
        println!(
            "ITEM role={} id={} code={} command={} label={:?} sub={:?} enabled={} reason={:?} transition={:?}",
            item.role.name(),
            item.id.code(),
            item.command.legacy,
            item.command.id.label(),
            item.label,
            item.sub,
            item.enabled(),
            item.reason(),
            item.transition
        );
    }
    checks.check(
        "main_continue_only_with_save",
        model_a.by_command(CommandId::Continue).is_some(),
        "save present",
    );
    let no_save = MenuState::new(maps()).model(&facts(HostScreen::MainMenu, false));
    checks.check(
        "main_no_continue_without_save",
        no_save.by_command(CommandId::Continue).is_none()
            && no_save.by_command(CommandId::LoadGame).is_none(),
        format!("items={}", no_save.items.len()),
    );
    checks.check(
        "main_no_multiplayer_button",
        !no_save
            .items
            .iter()
            .any(|item| item.label.contains("MULTI")),
        "not implemented -> not drawn",
    );
    checks.check(
        "touch_floor",
        MENU_MIN_TARGET_DP >= 48.0,
        format!("min={MENU_MIN_TARGET_DP}dp"),
    );

    // ------------------------------------------------- device tap → one action
    // One frame of geometry: every item gets a 64 px row at density 3 (matching
    // the drawn layout scale).
    let row = |index: usize| Rect::new(120.0, 160.0 + index as f32 * 200.0, 1200.0, 170.0);
    let campaign_item = model_a.by_command(CommandId::OpenCampaign).unwrap();
    let campaign_rect = row(0);
    let at = campaign_rect.centre();

    let mut pointer = MenuPointer::new();
    let mut gate = MenuCommandGate::new();
    let mut state = state.clone();
    let mut actions = 0usize;

    let press = pointer.press(
        campaign_item.command.legacy as u32,
        campaign_rect,
        at,
        1.000,
        SCALE,
    );
    let release = pointer.release(at, 1.060);
    let mut verdict = CommandVerdict::Disabled {
        reason: "not admitted".to_string(),
    };
    if let (PressOutcome::Armed { .. }, ReleaseOutcome::Activate { .. }) = (press, release) {
        verdict = gate.admit(&campaign_item.command, at, 1.060, SCALE);
    }
    if verdict == CommandVerdict::Apply {
        let dispatch = state.dispatch(CommandId::OpenCampaign, &main_facts);
        actions += dispatch.applied as usize;
        println!(
            "ACTION tap=campaign page={:?} dispatch={dispatch:?}",
            state.page
        );
    }
    checks.check(
        "tap_activates_once",
        actions == 1 && state.page == MenuPage::Campaign,
        format!("actions={actions} page={:?}", state.page),
    );

    // The SDL mirror: same physical gesture delivered again 80 ms later. The
    // screen has changed, so the same point now names a *different* row — the
    // press layer must refuse to arm it.
    let mirror_press = pointer.press(17, campaign_rect, at, 1.140, SCALE);
    checks.check(
        "mirror_press_suppressed",
        matches!(mirror_press, PressOutcome::MirrorSuppressed { .. }),
        format!("{mirror_press:?}"),
    );

    // Independent command-layer proof: even a second, well-formed gesture at
    // the same point 80 ms later is refused while its command differs (a screen
    // transition between the two copies).
    // The campaign command spec comes from the *before* model; the row spec
    // from the *after* model (the same point now names a different command).
    let campaign_spec = model_a.by_command(CommandId::OpenCampaign).unwrap();
    let chapter_model = state.model(&main_facts);
    let chapter_spec = chapter_model
        .items_for(wc3_game::hud::menu::MenuItemRole::Row)
        .first()
        .map(|item| item.command.clone())
        .unwrap_or_else(|| campaign_spec.command.clone());
    let mut gate2 = MenuCommandGate::new();
    let first = gate2.admit(&campaign_spec.command, at, 10.0, SCALE);
    let second = gate2.admit(&chapter_spec, at, 10.08, SCALE);
    checks.check(
        "mirror_gate_rejects_screen_change",
        first == CommandVerdict::Apply && matches!(second, CommandVerdict::MirrorSuppressed { .. }),
        format!("first={first:?} second={second:?}"),
    );

    // ------------------------------------------------------------- drag-off
    let mut pointer = MenuPointer::new();
    let press = pointer.press(15, row(0), row(0).centre(), 2.000, SCALE);
    let moved = pointer.move_to(row(0).centre() + Vec2::new(0.0, 900.0), SCALE);
    let released = pointer.release(row(0).centre(), 2.200);
    checks.check(
        "drag_off_cancels",
        matches!(press, PressOutcome::Armed { .. })
            && moved.is_some()
            && matches!(released, ReleaseOutcome::Cancel { .. }),
        format!("press={press:?} moved={moved:?} release={released:?}"),
    );

    // ------------------------------------------------ disabled row + reason
    let mut state = MenuState::new(maps());
    state.campaign_counts =
        facts(HostScreen::MainMenu, true)
            .chapters
            .iter()
            .fold(vec![0; 6], |mut counts, row| {
                counts[row.group] += 1;
                counts
            });
    state.page = MenuPage::Campaign;
    state.campaign_tab = 1; // HUMAN: chapters 4..7 are locked
    let campaign_facts = facts(HostScreen::MainMenu, true);
    let campaign_model = state.model(&campaign_facts);
    let locked = campaign_model
        .items_for(wc3_game::hud::menu::MenuItemRole::Row)
        .into_iter()
        .find(|item| !item.enabled());
    if let Some(locked) = locked {
        let mut gate = MenuCommandGate::new();
        let verdict = gate.admit(&locked.command, row(0).centre(), 3.0, SCALE);
        checks.check(
            "locked_row_disabled",
            matches!(verdict, CommandVerdict::Disabled { .. }),
            format!("label={:?} verdict={verdict:?}", locked.label),
        );
    } else {
        checks.check("locked_row_disabled", false, "no locked row in model");
    }

    // ------------------------------------------------------------- back stack
    // Campaign was opened from main: BACK returns to main, not to the wizard.
    let back = state.dispatch(CommandId::Back, &campaign_facts);
    checks.check(
        "back_campaign_to_main",
        back.applied && state.page == MenuPage::Main,
        format!("page={:?} dispatch={back:?}", state.page),
    );

    // Settings opened from the pause menu return to the pause menu (§4.7).
    let mut state = MenuState::new(maps());
    state.apply(wc3_game::hud::menu::MenuAction::OpenSinglePlayer);
    state.apply(wc3_game::hud::menu::MenuAction::SelectRace(
        RaceChoice::Human,
    ));
    state.apply(wc3_game::hud::menu::MenuAction::SelectOpponent(
        RaceChoice::Orc,
    ));
    state.apply(wc3_game::hud::menu::MenuAction::SelectMap(0));
    state.apply(wc3_game::hud::menu::MenuAction::StartGame);
    state.apply(wc3_game::hud::menu::MenuAction::AdvanceLoading);
    state.apply(wc3_game::hud::menu::MenuAction::Pause);
    let paused_facts = facts(HostScreen::Paused, true);
    let open = state.dispatch(CommandId::OpenSettings, &paused_facts);
    let options_page = state.page;
    let back = state.dispatch(CommandId::Back, &paused_facts);
    checks.check(
        "settings_back_to_pause",
        open.applied
            && options_page == MenuPage::Options
            && back.applied
            && state.page == MenuPage::Paused,
        format!("options={options_page:?} back={:?}", state.page),
    );
    // Resume returns to the battle.
    let resume = state.dispatch(CommandId::Resume, &paused_facts);
    checks.check(
        "pause_resume",
        resume.host == Some(wc3_game::hud::menu::HostAction::Resume)
            && state.page == MenuPage::InGame,
        format!("{:?}", resume.host),
    );

    // ------------------------------------------------------------- settings
    let mut state = MenuState::new(maps());
    state.page = MenuPage::Options;
    let main_facts = facts(HostScreen::MainMenu, true);
    let before = state.settings.draft.music;
    let select = state.dispatch(CommandId::SettingsSelect(SettingKey::Music), &main_facts);
    let step = state.dispatch(CommandId::SettingStep(SettingKey::Music, 1), &main_facts);
    let dirty_model = state.model(&main_facts);
    let apply = state.dispatch(CommandId::ApplySettings, &main_facts);
    let after = state.settings.applied.music;
    checks.check(
        "settings_draft_apply",
        select.applied
            && step.applied
            && dirty_model
                .by_command(CommandId::ApplySettings)
                .map(|item| item.enabled())
                .unwrap_or(false)
            && apply.host
                == Some(wc3_game::hud::menu::HostAction::ApplyAudio(
                    state.settings.applied,
                ))
            && after == (before + 10).min(100),
        format!("before={before} after={after} apply={apply:?}"),
    );
    // Cancel reverts the draft.
    state.dispatch(CommandId::SettingStep(SettingKey::Music, -1), &main_facts);
    state.dispatch(CommandId::CancelSettings, &main_facts);
    checks.check(
        "settings_cancel_reverts",
        !state.settings.dirty(),
        format!(
            "draft={:?} applied={:?}",
            state.settings.draft, state.settings.applied
        ),
    );

    // --------------------------------------------------------- save overwrite
    let mut state = MenuState::new(maps());
    state.apply(wc3_game::hud::menu::MenuAction::OpenSinglePlayer);
    state.apply(wc3_game::hud::menu::MenuAction::SelectRace(
        RaceChoice::Human,
    ));
    state.apply(wc3_game::hud::menu::MenuAction::SelectOpponent(
        RaceChoice::Orc,
    ));
    state.apply(wc3_game::hud::menu::MenuAction::SelectMap(0));
    state.apply(wc3_game::hud::menu::MenuAction::StartGame);
    state.apply(wc3_game::hud::menu::MenuAction::AdvanceLoading);
    state.apply(wc3_game::hud::menu::MenuAction::Pause);
    state.apply(wc3_game::hud::menu::MenuAction::OpenSaveLoad);
    let save_facts = facts(HostScreen::Paused, true);
    let first = state.dispatch(CommandId::SaveSlot(1), &save_facts);
    let armed =
        state.saveload.pending_overwrite == Some(SaveSlotKind::Quick) && first.host.is_none();
    let second = state.dispatch(CommandId::SaveSlot(1), &save_facts);
    checks.check(
        "save_overwrite_two_taps",
        armed && second.host == Some(wc3_game::hud::menu::HostAction::Save(1)),
        format!("first={first:?} second={second:?}"),
    );
    // LOAD mode shows the checkpoint and only offers a ready slot.
    let mode = state.dispatch(CommandId::LoadMode, &save_facts);
    let load_model = state.model(&save_facts);
    let load = state.dispatch(CommandId::LoadSlot(1), &save_facts);
    checks.check(
        "load_ready_slot",
        mode.applied
            && state.saveload.mode == SaveLoadMode::Load
            && load.host == Some(wc3_game::hud::menu::HostAction::Load(1))
            && load_model.screen == wc3_game::hud::menu::ScreenId::SaveLoad,
        format!("load={load:?}"),
    );

    // -------------------------------------------------------- input priority
    // Modal > menu > HUD > world. A disabled menu row still swallows the tap.
    let menu_targets = [
        PointerTarget::new(17, Rect::new(100.0, 100.0, 400.0, 160.0), true),
        PointerTarget::new(40, Rect::new(100.0, 300.0, 400.0, 160.0), false),
    ];
    let hud = [TouchButton {
        index: 7,
        rect: Rect::new(900.0, 500.0, 200.0, 200.0),
        enabled: true,
    }];
    let blocked = [Rect::new(600.0, 700.0, 400.0, 300.0)];
    let base = PointerContext {
        modal: None,
        menu: &menu_targets,
        hud: &hud,
        minimap: Some(Rect::new(1600.0, 40.0, 300.0, 300.0)),
        blocked: &blocked,
        placing: false,
        scale: SCALE,
    };
    let menu_hit = route_pointer(Vec2::new(300.0, 180.0), &base);
    let disabled_hit = route_pointer(Vec2::new(300.0, 380.0), &base);
    let hud_hit = route_pointer(Vec2::new(1000.0, 600.0), &base);
    let world_hit = route_pointer(Vec2::new(1400.0, 900.0), &base);
    let modal_hit = route_pointer(
        Vec2::new(1400.0, 900.0),
        &PointerContext {
            modal: Some(PointerTarget::new(
                99,
                Rect::new(0.0, 0.0, 100.0, 100.0),
                true,
            )),
            ..base
        },
    );
    checks.check(
        "priority_modal_menu_hud_world",
        menu_hit == PointerRoute::Menu(17)
            && disabled_hit == PointerRoute::Menu(40)
            && hud_hit == PointerRoute::Hud(7)
            && world_hit == PointerRoute::World
            && modal_hit == PointerRoute::Modal(99),
        format!(
            "menu={menu_hit:?} disabled={disabled_hit:?} hud={hud_hit:?} world={world_hit:?} modal={modal_hit:?}"
        ),
    );
    let nearest = hit_menu(Vec2::new(510.0, 180.0), &menu_targets, SCALE);
    checks.check(
        "menu_hit_slop_nearest",
        nearest == Some(17),
        format!("{nearest:?}"),
    );

    // ------------------------------------------------ command cooldown (§8.1)
    let mut gate = ActivationGate::new();
    let first = gate.admit(17, Vec2::new(1100.0, 1100.0), 20.0, 0.20, SCALE);
    // > 8 dp * 3 = 24 px away, so this is a deliberate second press, not the
    // mirrored copy of the first one at the same point.
    let rapid = gate.admit(17, Vec2::new(1200.0, 1100.0), 20.10, 0.20, SCALE);
    let different = gate.admit(3, Vec2::new(500.0, 500.0), 20.12, 0.20, SCALE);
    checks.check(
        "command_cooldown",
        first == GateDecision::Allow
            && matches!(rapid, GateDecision::Cooldown { .. })
            && different == GateDecision::Allow,
        format!("first={first:?} rapid={rapid:?} different={different:?}"),
    );

    // ------------------------------------------------------------ custom game
    let mut state = MenuState::new(maps());
    state.page = MenuPage::CustomGame;
    let custom = state.model(&main_facts);
    let rows = custom.items_for(wc3_game::hud::menu::MenuItemRole::Row);
    checks.check(
        "custom_game_page_rows",
        rows.len() == wc3_game::hud::menu::MAP_ROWS_PER_PAGE
            && custom.page_label == "MAPS 1/3"
            && custom
                .by_command(CommandId::MapNext)
                .map(|item| item.enabled())
                .unwrap_or(false),
        format!("rows={} label={:?}", rows.len(), custom.page_label),
    );
    // Selecting a row stays on the custom-game screen (the picker's origin).
    let row_spec = rows[1].command.clone();
    let map_index = match rows[1].transition {
        wc3_game::hud::menu::Transition::SelectMap(index) => index,
        other => {
            println!("unexpected transition {other:?}");
            0
        }
    };
    let out = state.dispatch(row_spec.id, &main_facts);
    // The row command is host-resolved (slot -> map index); the model then
    // applies the selection and stays on the custom-game screen.
    state.apply(wc3_game::hud::menu::MenuAction::SetMap(map_index));
    checks.check(
        "custom_map_row_stays",
        out.host == Some(wc3_game::hud::menu::HostAction::MapRow)
            && state.page == MenuPage::CustomGame
            && state.map_index == map_index,
        format!(
            "page={:?} map={} expected={map_index}",
            state.page, state.map_index
        ),
    );

    // ---------------------------------------------------------------- result
    println!(
        "MENU_CONTRACT ok={} failures={} verdict={}",
        checks.ok,
        checks.failed,
        if checks.failed == 0 { "PASS" } else { "FAIL" }
    );
    if checks.failed > 0 {
        std::process::exit(1);
    }
}

// Keep the item type referenced by the table printer above; the example is
// compiled with `deny(warnings)` in CI only for the crate, not for examples.
#[allow(dead_code)]
fn _item_name(item: &MenuItem) -> &str {
    &item.label
}
