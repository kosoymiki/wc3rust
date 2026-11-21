# Ledger 554 — menu model + §8 action contract

Task ref `ledger-554-menu-model` · monitor ledger **556** · corpus pack
`ev-db32959c7649b8e2` · date 2026-11-18 · repo `~/src/wc3rust` · base commit
`d7af459` (game.rs mtime 16:54, patch generated against that working tree).

Scope: `crates/wc3-game/src/ui/menu.rs` (model, navigation, reducer) and
`crates/wc3-input/**` (router, priorities, touch). `mobile_hud.rs` (553),
`hudskin/**` (555) and `game.rs` (tech-lead) were **not** edited; the `game.rs`
wiring is delivered as a patch (§7) and type-checked without applying it.

---

## 1. Goal

Replace the three menu sources of truth (`MenuView::for_screen` static tables,
`Game::sync_menu` hand-built lists, `Game::menu_view` overrides) with **one
typed, deterministic screen list** derived from state, and implement HUD §4
screens plus the §8 command/input contract (typed command, disabled reason,
cooldown, target type, no double activation, priority modal > menu > HUD >
world, 48 dp + drag-off touch rules).

## 2. Delivered files

| file | change |
|---|---|
| `crates/wc3-game/src/ui/menu.rs` | rewritten model: +2979/−210 lines (3187 net) |
| `crates/wc3-input/src/menu.rs` | new 466-line §8 input contract module |
| `crates/wc3-input/src/lib.rs` | `pub mod menu;` (2 lines) |
| `crates/wc3-game/examples/menu_contract.rs` | new 561-line headless proof driver |
| `analysis/agent-hud/patches/ledger-554-menu-model-game.patch` | 828-line `game.rs` wiring patch, sha256 prefix `59325d918549cc9d` |

Existing API (`MenuPage`, `MenuAction`, `MenuState`, `RACES`,
`DIFFICULTIES`, `CAMPAIGN_*`, `MAP_ROWS_PER_PAGE`, `SaveStatus`) is preserved;
the frozen tests compile and pass unchanged.

## 3. §4 screens (one list per screen)

`MenuState::model(&MenuContext) -> MenuScreenModel` is pure (no clock, no
filesystem, no RNG). It returns `items: Vec<MenuItem>` — the only description
of the screen. Each item carries `ItemId` (role + in-role index), `MenuItemRole`
(entry/tab/row/nav), `MenuItemState` (enabled, selected, disabled reason),
`CommandSpec` (CommandId, legacy wire code, TargetType, enabled + reason,
cooldown) and `Transition`. Render groups are filters of the same list in the
same order, so layout index == model index.

| § | screen | content |
|---|---|---|
| 4.1 | Main | CONTINUE (only with a resumable save), CAMPAIGN, CUSTOM GAME, LOAD GAME (only with a save), SETTINGS, QUIT (two-tap arm → exit). Multiplayer is not drawn because it is not implemented. |
| 4.2 | Single player / skirmish | RACE, OPPONENT, MAP, DIFFICULTY (open pickers), START GAME (disabled with “NO MAP SELECTED”), BACK. Campaign and Custom Game are the two implemented single-player modes and are one level up on Main (multiplayer absent, so no empty hub screen). |
| 4.3 | Campaign | 6 group tabs with per-group counts (empty group disabled with reason), paged chapter rows (5/page), locked rows disabled with “LOCKED — COMPLETE THE PREVIOUS CHAPTER”, CLEARED/OPEN/LOCKED sub-labels, interlude marker, PREV/NEXT/BACK, page label `HUMAN 1/2`. |
| 4.3 | Custom game / map select | race and difficulty fields, paged map rows (5/page, player count, SELECTED marker), PREV/NEXT/START/BACK, `MAPS 1/3`. |
| 4.4 | Settings | AUDIO tab; MASTER/MUSIC/EFFECTS rows showing the draft value; −10/+10 on the selected row; APPLY (disabled when clean) / CANCEL (disabled when clean) / BACK. Draft/applied split: BACK and CANCEL discard, APPLY commits and returns `HostAction::ApplyAudio`. Only parameters the engine really exposes are drawn. |
| 4.5 | Pause | RESUME, SETTINGS, SAVE GAME (enabled/reason from save API), LOAD GAME (enabled/reason from slot 1), SAVE / LOAD list, RESTART, QUIT TO MENU (two-tap arm). Simulation pause itself remains the host's `set_paused` (already covered by `pause_stops_ticks`). |
| 4.6 | Save / load | SAVE/LOAD mode tabs; SLOT 1 + AUTOSAVE rows with `Empty / Ready{map,tick,saved_at} / Unreadable{reason} / Unsupported{reason}`; overwrite requires the second tap (`confirm_overwrite`); autosave read-only; empty-state and working indicator; no delete control because the host has no delete API. |
| 4.7 | Navigation | `ScreenId`, `ScreenState` (`screen_state()`), `NavigationAction`, `BackAction`, `TransitionResult`, `RejectReason`. BACK is the logical parent: options → the screen that opened them (pause stays pause), pickers → their origin, campaign/custom/single → main. A second request for the current screen is rejected (`AlreadyOnScreen`), not applied twice. |

Victory/defeat screens are modelled too (RESTART, MAIN MENU), and Loading has
no items.

## 4. §8 contract

* `CommandId` — typed identity (navigation, pickers, rows, tabs, slots,
  settings, confirmations); `CommandSpec` — `legacy` wire code (same number the
  device log prints), `TargetType`, `enabled` + `disabled_reason`, `cooldown_s`.
* `MenuCommandGate` (command layer) — rejects a mirrored duplicate **by point**
  (same position within 300 ms) before the command-id check, so a mirror that
  arrives after a screen transition cannot activate a *different* row. Per
  command cooldown: 0.20 s navigation, 0.35 s confirmations.
* `wc3_input::menu` (gesture layer) — `route_pointer` priority modal → menu →
  HUD (minimap → buttons → blocked) → place → world; disabled menu items still
  swallow the tap; `MenuPointer` arms one target per press, cancels on drag-off
  (24 dp slop), activates only on release inside the row, suppresses a mirrored
  press at the same point (300 ms / 8 dp); `ActivationGate` adds point-based
  mirror rejection + cooldown; `hit_menu` uses a 48 dp floor + 4 dp seam with
  nearest-centre resolution; `meets_touch_floor` reports drawn-rect compliance.
* `MenuState::dispatch(CommandId, &MenuContext) -> Dispatch { applied, host,
  reject }` is the typed reducer; `HostAction` carries the work only the host
  can do (start battle, save/load, restart, quit, resume, apply audio).

## 5. Defects found and fixed at model level

1. **BACK lost the pause menu.** `activate_menu` mapped BACK to
   `Goto(Main)` for every page, so settings opened from pause returned to the
   main menu and left the battle. Fixed: `MenuAction::Back` + `back_action()`;
   the patch sends `MENU_ACT_BACK => MenuAction::Back`.
2. **Pickers could not return to custom game.** `SetRace/SetMap/SetDifficulty`
   always landed on `SinglePlayer`, so a map row on the custom-game screen left
   the screen. Fixed with `picker_origin()` (custom-game leaf vs wizard chain).
3. **Options opened from pause had no back link in the typed path.** `Goto`
   did not record `options_from`; fixed inside `navigate`.
4. **Duplicate transition.** `Goto` to the current screen now returns
   `Rejected(AlreadyOnScreen)` instead of re-entering it (§4.7).
5. **Disabled actions guessed.** Activation now reads the model's reason, logs
   `GAME_MENU disabled id=… reason=…` and never navigates.

## 6. Evidence (observed, 2026-11-18)

All cargo through `scripts/h2-cargo`.

| check | rc | evidence |
|---|---|---|
| `check -p wc3-input --offline` | 0 | no warnings in `wc3-input/src/menu.rs` |
| `check -p wc3-game --lib --offline` | 0 | `menu.rs` warning-free (log `menu-check2-554.log`) |
| `test -p wc3-game --lib hud::menu` | 0 | 5 passed, 0 failed (frozen tests unchanged) |
| `test -p wc3-input --lib` | 0 | 68 passed, 0 failed |
| `run -p wc3-game --example menu_contract` | 0 | `MENU_CONTRACT ok=22 failures=0 verdict=PASS` |
| `run -p wc3-game --example menu_layout` | 0 | `GAME_MENU_LAYOUT screen=Main vp=2414x1080 entries=5` + 5 `GAME_MENU_ENTRY` lines |
| `run -p wc3-game --example hud_mock` | 0 | `HUD_MOCK_DONE 5 files` (analysis/agent-hud/553/*.png) |
| `rustfmt --check` (3 changed/new files) | 0 | formatted |
| `git apply --check ledger-554-menu-model-game.patch` | 0 | patch applies to current `game.rs` |
| patched `game.rs` type check | 0 | full crate `rustc --emit=metadata` on a copied crate tree with the patched file: 0 errors (log `game-patched-check2-554.log`) |

`menu_contract` proves, output by output:

* model determinism (`fingerprint=e91701b1248bd870` twice);
* one tap on CAMPAIGN → exactly one command (`actions=1 page=Campaign`);
* the mirrored press is suppressed at the gesture layer
  (`MirrorSuppressed`) and, independently, the command gate rejects the second
  copy at the same point even though the screen changed (`MirrorSuppressed
  since_s=0.0799`);
* drag-off → `Cancel { reason: DragOff }`, zero commands;
* a locked chapter row → `Disabled { reason: "LOCKED — COMPLETE THE PREVIOUS
  CHAPTER" }`;
* BACK: campaign → main; settings → **pause** (options=Options back=Paused);
* pause → resume host action; settings draft +10 → APPLY emits
  `ApplyAudio { music: 90 }`; CANCEL reverts;
* save overwrite: first tap arms, second emits `Save(1)`; LOAD mode emits
  `Load(1)` for the ready slot;
* priority `Menu(17) / Menu(40 disabled) / Hud(7) / World / Modal(99)`;
* 48 dp floor, cooldown rejection at 0.10 s, custom-game paging (`MAPS 1/3`)
  and a map row that stays on the custom-game screen.

## 7. `game.rs` wiring patch (owner applies)

`analysis/agent-hud/patches/ledger-554-menu-model-game.patch` (828 lines,
sha256 prefix `59325d918549cc9d`):

1. fields `menu_model: Option<MenuScreenModel>`, `menu_pointer: MenuPointer`,
   `menu_gate: MenuCommandGate` (+ init);
2. `sync_menu` rebuilt from `menu_flow.model(&menu_context())` and projected
   into the four draw groups (one list → all groups); title/subtitle/page
   label/veil/map name come from the model;
3. `menu_context()` supplies registry chapter rows, save-slot verdicts
   (strict `CheckpointMeta` decode, mtime as date/time), save API reason,
   quit arm, loading progress, end stats and real engine gains;
4. `pointer_down/move/up` go through `MenuPointer` (mirror press suppressed,
   drag-off cancels at move time); `pointer_up` gates the command through
   `MenuCommandGate` and logs `GAME_MENU rejected …` instead of activating;
5. `activate_menu` rejects disabled model items with their reason and routes
   codes ≥ 100 through the typed reducer (`activate_model_command`), including
   `apply_audio` (master/music/effects gains) and the two-tap save;
6. BACK is `MenuAction::Back` (§4.7 fix); new `MENU_ACT_SAVELOAD = 98` entry
   opens the §4.6 list; `menu_action_name` learns the new codes.

Verification performed: `git apply --check` rc=0; the patched file parses
(`rustfmt --edition 2024`, rc=0); and the whole crate type-checks with the
patched file via a copied crate tree + the exact cargo rustc invocation
(`--emit=metadata`, rc=0). It was **not** applied to the live `game.rs`
(owner path).

## 8. Integration notes for the tech-lead / adjacent ledgers

1. **553 layout capacity (blocking for the pause screen after the patch).**
   `menu_layout` currently publishes main rows at `y=526,687,848,1009,1170`
   with height 150 in a 1080-high viewport — the last row is clipped. The pause
   model has 7 entries (RESUME, SETTINGS, SAVE, LOAD, SAVE/LOAD, RESTART, QUIT).
   `layout_menu` must clamp/shrink rows to the safe panel height (553 owner);
   alternatively drop one pause entry. Exact lines:
   `GAME_MENU_ENTRY group=0 index=4 action=2 rect=695,1170,1024,150`.
2. **Save timestamps.** `CheckpointMeta` stores no timestamp; the patch uses the
   save file mtime (`format_unix`) and labels it in the list. If a real
   timestamp is added to the sidecar later, replace that call.
3. **Master gain.** `AudioEngine` has `set_master_gain` but no getter, so the
   menu's applied master is authoritative; music/effects are read from the
   engine buses in `audio_settings()`.
4. **`hud_mock` was transiently broken** by 553's in-flight `Rect::is_empty`
   API change (21 errors in `examples/hud_mock.rs`) at 18:15; the final run at
   18:2x is green (`HUD_MOCK_DONE 5 files`). No action needed unless it
   regresses.

## 9. Acceptance against the task

1. One typed model instead of three sources — done (`MenuScreenModel::items`;
   projections are filters; `fingerprint` determinism proof).
2. §4 screens — all listed screens have a model, transitions and availability
   reasons; settings and save/load have real state machines.
3. §8 contract — `CommandId`, `disabled_reason`, `cooldown_s`, `target_type`,
   duplicate protection (gesture + command layers), priorities, 48 dp and
   drag-off; proven by `menu_contract` output.
4. Run proof — `hud_mock` rc=0, `menu_layout` rc=0, `menu_contract`
   `ok=22 failures=0`; frozen menu tests 5/5.

No device commands were used (`am/input/pm/screencap/logcat` untouched); no
new tests were written.
