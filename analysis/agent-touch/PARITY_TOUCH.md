# PARITY TOUCH — control groups, hotkeys, tooltips, box select, edge pan

Ledger: `parity-touch` (359, parent `r2-touch` 356), 2026-11-13. Owner: touch.
`analysis/tech-lead/PARITY_MATRIX.md` is not published yet; the rows below are
named after the dispatcher's parity list and will be renamed 1:1 when the matrix
lands. No device run.

## 0. P0 unblock (compile)

Reported: `wc3-input/src/lib.rs:383` E0533 and game.rs `TouchEnd`/`Hotkey`
pattern mismatches after the touch API change. Current tree check:
`cargo check -p wc3-input` → **rc 0** (the E0533 was a transient mid-edit state;
the line now is `Zone::Button(index) => {` inside `pointer_up`, matching the
`Zone` tuple variant, and no constructor misuse remains).

The only real compile error in `wc3-game --lib` was in HUD menu code, not input:

```text
error[E0521]: borrowed data escapes outside of method
  --> crates/wc3-game/src/game.rs:1085:24
1079 | fn menu_view(&self) -> MenuView<'_> {
1085 | let mut view = MenuView::for_screen(screen, self.viewport, &self.assets.map_name);
     |   `self` escapes the method body here: requires `'1` outlives `'static`
```

Cause: `MenuView::for_screen(..., map_name: &'static str) -> MenuView<'static>`
(`crates/wc3-game/src/mobile_hud.rs:644`) while `menu_view` passed a borrowed
asset string. Fix (minimal, HUD layout/card code untouched): leak the map title
once in `Game::new` and pass the `'static` value.

```text
game.rs:222  let menu_map_name = Box::leak(assets.map_name.clone().into_boxed_str());
game.rs:177  menu_map_name: &'static str  (Game field)
game.rs:264  menu_map_name,               (Game::new initialiser)
game.rs:1094 MenuView::for_screen(screen, self.viewport, self.menu_map_name);
```

`cargo check -p wc3-game --lib` → **rc 0**.

## 0b. Alignment to `PARITY_MATRIX.md` (published 2026-11-13 07:58Z)

Line numbers are current (the matrix snapshot predates the last merges; the
current anchors are listed here).

| Matrix row | Matrix status / gap | Touch evidence (current tree) | Touch delivers | Remaining gap owner |
|---|---|---|---|---|
| **P12** Selection groups | PARTIAL · "no on-screen group row" | `wc3-input::ControlGroups` `crates/wc3-input/src/lib.rs:181`; `Game.control_groups` `game.rs:188`; `Game::hotkey` `game.rs:947`; `assign_group` `:977`; `recall_group` `:989`; ARMY chip `game.rs:1446` + `:875/884`; **on-screen row** `group_row_slots` `game.rs:1050`, `draw_group_row` `:1065`, `hit_group_slot` `:1104`, targets 100..108 `game.rs:28` | Ctrl/⌘+1..9 assign, digits recall with double-press centre; touch: 9-slot console row (tap = recall, double = recall+centre, hold = save to that slot) + ARMY shortcut | **gap closed this round** (row is drawn and tappable) — dispatcher re-measure |
| **P13** Hotkeys | FULL · letters from labels, not `CommandStrings.txt` | `Hotkey::parse` `wc3-input/src/lib.rs:169`; `Game::key_char` `game.rs:971`; `activate_hotkey` `game.rs:1137`; `ui_hotkey` `game.rs:2460`; HUD letters `mobile_hud.rs:273` | letters resolved against the **visible** card, Escape cancels placement/targeting; every action also has a touch cell | letter source = data/HUD |
| **P9** Tooltips | PARTIAL · no border/icon/cost layout | `TouchInput::long_press` `wc3-input/src/lib.rs:528`; `Game::update_tooltip` `game.rs:924`; `HudView::tooltip` view assembly `game.rs`; pressed-cell + group-slot tooltips | long-press unit → WC3 tooltip + HP; press cell/slot → label/sub/gesture hint | frame art/cost layout = HUD/FDF (`qa.tooltip.layout`) |
| **P5** Minimap | PARTIAL · no terrain image / fog | router minimap zone `wc3-input/src/lib.rs:454`; `minimap_to_world` `wc3-input/src/lib.rs:641`; input entry `game.rs:1371` | tap jump, drag scrub, 0.45 s hold → move order, HUD blocker order | image/fog = HUD |
| **P8** Command card | PARTIAL · slot order heuristic | `hit_button` `wc3-input/src/lib.rs:581`, `TouchLayout` `:66`, `refresh_buttons` `game.rs:684` | every card action is a touch target; expanded band + nearest-centre | slot order = HUD/data |
| **P4** Console | FULL · 64 px = 21.3 dp advisory WARN | `MIN_BUTTON_TARGET = 108.0` scaled `wc3-input/src/lib.rs:96`; `command_card_tiles_have_a_minimum_finger_sized_hit_target` | hit band lifts every 64 px cell to ≥108 px effective target | visual size is a HUD choice (QA WARN accepted) |
| **— (new)** touch.box-select | not in matrix | `DragMode::Box` `wc3-input/src/lib.rs:266`; `TouchEnd::Box` `game.rs:1446`; `box_select` `game.rs:1669`; **marquee** `box_marquee_rect` `game.rs:1117`, `draw_box_marquee` `:1129` | propose row `touch.box-select`, gate `qa.input.box`; the physical path draws the box while the finger is down | dispatcher matrix row |
| **— (new)** touch.edge-pan | not in matrix | `TouchInput::edge_pan` `wc3-input/src/lib.rs:483`; `drive_camera` `game.rs:909` | propose row `touch.edge-pan`, gate `qa.input.edge_pan` | dispatcher matrix row |
| **— (new)** touch.long-press | covered by P9 | `TouchInput::long_press` `wc3-input/src/lib.rs:528` | tooltip channel for P9 | — |

## 1. Parity rows

| Parity feature | Touch path (file:line) | Keyboard path | Verified by |
|---|---|---|---|
| Control groups 1..9 | ARMY chip: tap = select army; hold ≥0.45 s = save selection to the next free group; double tap = recall/cycle non-empty groups and centre. `game.rs:1191` (`TouchEnd::Button{hold,double}`), `game.rs:862` (`save_group_from_touch`), `game.rs:872` (`cycle_group_from_touch`), `game.rs:824` (`recall_group`) | Ctrl+1..9 assign, 1..9 recall (second press ≤0.4 s centres): `game.rs:791` (`hotkey`), `game.rs:815` (`assign_group`) | `army_chip_hold_saves_and_double_tap_recalls_a_group`, `hotkeys_drive_the_card_and_control_groups`, `wc3-input::hotkey_parse_and_control_group_cycle` |
| Hotkeys | Every card action is a touch cell; the WC3 letter is drawn in the cell (`mobile_hud.rs:221 hotkey`) | `Hotkey::parse` (`wc3-input/src/lib.rs:169`) + `Game::key_char` (`game.rs:795`) + `ui_hotkey` (`game.rs:1947`) resolve the letter against the **visible** card; Escape cancels placement/targeting | `hotkeys_drive_the_card_and_control_groups`, `wc3-input::hotkey_parse_and_control_group_cycle` |
| Tooltips (long-press) | Hold ≥0.45 s on a world unit → `TouchInput::long_press` (`wc3-input:456`) → `Game::update_tooltip` (`game.rs:733`) → `HudView::tooltip` (`game.rs:1838`); pressing a command cell shows `LABEL · sub · ARMY gestures` (`game.rs:1826`) | Hover is the same tooltip | `long_press_shows_a_unit_tooltip`, `wc3-input::stationary_long_press_reports_the_down_point` |
| Box select | Hold ≥0.30 s then drag → `DragMode::Box` (`wc3-input:266`) → `TouchEnd::Box` (`game.rs:1191`) → `box_select` (`game.rs:1263`) | n/a (mouse box uses the same path) | `drag_box_selects_the_starting_army_after_the_hold` |
| Edge pan | While boxing, a 30·scale band inside the safe area scrolls: `drive_camera` (`game.rs:713`) → `TouchInput::edge_pan` (`wc3-input:389`) → `Camera::pan_pixels` (`wc3-input:560`) | n/a | `box_drag_at_the_edge_auto_scrolls_the_camera` |
| All touch targets | Exact rect, then a finger band with a minimum `108·scale` target and nearest-centre resolution (`wc3-input::hit_button`, `lib.rs:473`); card cells are the 64 px WC3 1:1 tiles plus the band | n/a | `command_card_tiles_have_a_minimum_finger_sized_hit_target`, `console_layout_blocks_world_clicks_and_placement` |

Tap semantics kept: tap unit = select (double tap = same kind), tap ground with
an army = move, tap enemy = attack (there is no right-click on touch; that WC3
button maps to the same tap-order path).

## 1b. Round 4 — on-screen control-group row + box marquee (2026-11-13)

| Increment | Implementation (current tree) | Test |
|---|---|---|
| On-screen control-group row | Nine slots centred in the console dead space (`group_row_slots` `game.rs:1050`), drawn every frame after the console (`draw_group_row` `game.rs:1065`: slot number, live unit count, active-slot highlight), appended to the router as touch targets 100..108 (`GROUP_TARGET_BASE` `game.rs:28`, `refresh_buttons`). Tap = recall, double tap = recall + centre, hold = save the current selection to that exact slot (`hit_group_slot` `game.rs:1104`). The slot tooltip reads `GROUP n · k UNITS · HOLD SAVE · 2x CENTRE`. | `control_group_row_is_on_screen_and_tappable` (31/31 game tests green) |
| Box-select physical path | The marquee is now drawn in screen space while `DragMode::Box` is active (`box_marquee_rect` `game.rs:1117`, `draw_box_marquee` `game.rs:1129`: translucent fill + 1.5·scale border). A real finger sees the box grow before release; the QA harness boundary (capture 634: injected motion cannot hold 300 ms) is unchanged, but the on-device visual path is complete. | `box_marquee_rect_tracks_the_drag` |

Layout safety: every slot centre is asserted inside the console and non-intersecting
the card, selection panel and minimap, so the row cannot steal a HUD touch.

rc (2026-11-13 12:04): `cargo check -p wc3-input -p wc3-game --lib` **0**;
`cargo test -p wc3-input` **22/22 0**; `cargo test -p wc3-game --lib game::tests::`
**31 passed / 1 ignored 0** (both new tests named above). HUD-owned
`mobile_hud.rs`/`hud.rs`/`ui.rs` untouched; all changes are game input functions
plus their own feedback drawing.

## 2. Wiring map (P0 item 4 + round 4)

- `TouchEnd::Button { index, double, hold }` — produced `wc3-input/src/lib.rs:388`,
  consumed `game.rs:1446`: indices 0..11 are card buttons (ARMY tap/hold/double
  as before), indices 100..108 are the on-screen group row (`hit_group_slot`,
  `game.rs:1104`).
- `Hotkey::parse` — `wc3-input/src/lib.rs:169`; `Game::key_char` calls it
  (`game.rs:971`); `Game::hotkey` applies digits/letters/Escape (`game.rs:947`).
- `ControlGroups` — `wc3-input/src/lib.rs:181`; stored on `Game.control_groups`
  (`game.rs:188`), reset per scenario, filled by Ctrl+digit, the ARMY hold and the
  group-row hold.
- Long-press/tooltip hook — `TouchInput::long_press` (`wc3-input/src/lib.rs:528`)
  → `Game::update_tooltip` (`game.rs:924`) every frame → the HUD tooltip panel.
- Flat `wc3-input` API — `grep -c "wc3_input" crates/wc3-game/src/game.rs` = 22,
  so the touch crate is the only gesture/camera implementation in the game path.

## 3. rc (2026-11-13)

| Command | rc | Result |
|---|---|---|
| `cargo check -p wc3-input` | 0 | — |
| `cargo check -p wc3-game --lib` | 0 | after the E0521 menu fix |
| `cargo test -p wc3-input` | 0 | 22/22 |
| `cargo test -p wc3-game --lib` | 0 | 87 passed / 15 ignored (full suite, 10:29) |
| `cargo test -p wc3-game --lib game::tests:: -- --test-threads=1` | 0 | 28 passed / 1 ignored (10:37, includes the 3 parity tests) |

## 4. Remaining

- `PARITY_MATRIX.md` alignment: rows above are ready; rename on publication.
- Device: not run (coordinated). Physical long-press/ARMY-hold gestures should be
  confirmed on the phone next to the existing QA build; box select on device is
  still a harness boundary (QA capture 634).

## 1c. Round 6 — DragonUI donor intake (2026-11-13)

DragonUI-20261113 is a WoW micromenu addon (NeticSoul, MIT; see LICENSE/THIRD_PARTY_NOTICES). Read-only intake: `DragonUI/modules/micromenu.lua` (2742 lines), `modules/buttons.lua` (1492 lines), textures `Textures/Micromenu/` and `Textures/UI/ui-quickslot-{depress,2}.blp`. Byte-level layout mechanisms ported as **documentation/pattern** into this Rust project — no Lua code copied, no WoW assets redistributed; the app reads only the operator's own War3.mpq/interface files at runtime.

### Micromenu grid (dragonUI/modules/micromenu.lua)

Layout metrics function `GetMicroLayoutMetrics(config, useGrayscale, numButtons)` computes per-cell `buttonWidth × buttonHeight` (40×19 normal / 32×19 grayscale icon cell) plus `pad = icon_spacing` (normal: −6, grayscale: 1), giving cell step `hStep = buttonWidth + pad`, `vStep = buttonHeight + pad`, and column count `columns` (default 12, clamped to numButtons). Rows = `ceil(numButtons / columns)`; total strip `totalWidth = columns*buttonWidth + (columns−1)*pad`, `totalHeight` analog. `LayoutMicroButtons` places button `i` at grid cell `(idx % columns, floor(idx / columns))` anchored BOTTOMLEFT at `x = (idx % columns)*hStep, y = MICRO_LAYOUT_BASE_Y + floor(idx/columns)*vStep`. The overlay menu scales with `config.scale_menu` and is centered on the strip.

### Quickslot button art (dragonUI/modules/buttons.lua)

`ATLAS_FRAME = 'ui-hud-actionbar-iconframe'`; `SetFrameTexture` sets normal texture from `config.assets.normal` and applies the atlas; `PinCorners` anchors backdrop region to owner corners; `FitSlotArt` fills the owner's 79-point frame canvas (slot runs under the band, 90×90 canvas); textures `ui-quickslot-depress.blp` / `ui-quickslot2.blp` for depressed/rest states. Slot art is re-applied on `PetActionBar_Update` so store-bought art isn't swapped away by the client.

### Port into PARITY_TOUCH.md patterns (Rust touch layer)

The DragonUI micromenu maps directly onto the on-screen control-group row already implemented:
- Grid placement `columns = min(columns, numButtons); rows = ceil(numButtons/columns); cell (i%columns, floor(i/columns))` → `group_row_slots` `game.rs` (Round 6: extended to 10 slots, 1..9,0) at `size=40*scale, gap=6*scale` centred in the console; DragonUI's 12-button micromenu grid/step math (`hStep/vStep`, `totalWidth/totalHeight`, anchored `BOTTOMRIGHT` offset) is the direct reference for multi-row extension.
- `FitSlotArt` corner-pinning → the row's per-slot border/fill drawn by `draw_group_row` `game.rs`.
- Quickslot depressed/rest textures (`ui-quickslot-depress.blp`, `ui-quickslot2.blp`) → WC3 equivalent is slot label + live count; DragonUI quickslot art is a donor reference for the HUD icon frame, not ported as assets (only the operator's own interface files may be loaded at runtime).

Round 6 code (owner paths): `wc3-input/src/lib.rs` `ControlGroups` 10 slots with slot 0 (index 9), `Hotkey::parse` accepts `0`; `game.rs` `group_row_slots` `[Rect;10]` (1..9,0), `draw_group_row`/`hit_group_slot` map index 9→slot 0, tests updated (`control_group_row_is_on_screen_and_tappable` covers slot 3 and slot 0). Box-select physical path (down→drag>threshold→up → `TouchEnd::Box` → marquee) remains documented in `BOX_SELECT_EDGE_PAN.md`/this artifact; `input motionevent` is outside the 300 ms `DragMode::Box` harness boundary (harness boundary, not faked). Device not run.

## 1d. Round 7 — verification on the current tree (2026-11-13 19:29 MSK)

Mission: verify ROUND 6 deliveries after other layers landed, re-run layer gates, fix regressions in owner paths, produce the artifact (no re-implementation).

### Regression found and fixed (owner path)

Between ROUND 6 (18:33) and ROUND 7 start (19:16) `crates/wc3-game/src/game.rs` was overwritten by an external writer (mtime 18:45:13, sha `05335b2f…`) and came back with the **9-slot** row (`group_row_slots() -> [Rect; 9]`, `slot = index + 1`, no slot 0). `crates/wc3-input/src/lib.rs` kept the 10-slot change (sha `4ae84d83…`, `groups: [Vec<u32>; 10]`). Fix re-applied in the owner path (`game.rs` input/UI functions only, backup in `$PREFIX/tmp/opencode/game.rs.r7bak-*`):

- `game.rs:1166` `group_row_slots() -> [Rect; 10]`, width `size*10 + gap*9`.
- `game.rs:1181` `draw_group_row`: `slot = if index < 9 { index + 1 } else { 0 }`, label `"0"` for the 10th slot.
- `game.rs:1220` `hit_group_slot`: same index→slot mapping (hold assign / double recall+centre / tap recall).
- `game.rs:3440` `control_group_row_is_on_screen_and_tappable`: asserts `slots.len() == 10`, slot 3 hold/tap/double-tap, slot 0 (index 9) hold-save + tap-recall with pressed target `GROUP_TARGET_BASE + 9` (109).

Final sha256: `game.rs` `e7db1c688…`, `wc3-input/src/lib.rs` `4ae84d83…`.

### Layer gates (current tree, exact commands)

Run detached with `ulimit -v 4500000`, `CARGO_BUILD_JOBS=1`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_PROFILE_DEV_DEBUG=0`; log `analysis/agent-touch/round7-touch-tests.log` (881 lines, copied from `$PREFIX/tmp/opencode/r7-touch.log`):

| Gate | Command | rc | Count |
|---|---|---|---|
| input tests | `cargo test -p wc3-input --offline` | **0** | 22 passed, 0 failed |
| lib check | `cargo check -p wc3-game --lib --offline` | **0** | — |
| game input tests | `cargo test -p wc3-game --lib game::tests:: --offline -- --test-threads=1` | **0** | 31 passed, 1 ignored, 89 filtered out |

Specific test lines observed in the log (`... ok`): `game::tests::control_group_row_is_on_screen_and_tappable`, `game::tests::box_marquee_rect_tracks_the_drag`, `game::tests::hotkeys_drive_the_card_and_control_groups`, `game::tests::army_chip_hold_saves_and_double_tap_recalls_a_group`.

### Control groups verified (1..9,0)

- Input: `Hotkey::parse('0') == Digit(0)`; `ControlGroups` slot 0 ↔ index 9; `next_non_empty(9) == Some(0)`, `next_non_empty(0) == Some(1)` (wrap) — `crates/wc3-input/src/lib.rs:952` test.
- Game: on-screen row of 10, slot 0 assign/select via hold/tap — `game.rs:3440` test (green).

### Box-select physical path (harness boundary, not faked)

Path: world down → first move after `BOX_HOLD_SECONDS = 0.30` s (`wc3-input/src/lib.rs:88`) with displacement > `TAP_SLOP = 16` px → `DragMode::Box` → live marquee (`game.rs:1233 box_marquee_rect`, `:1245 draw_box_marquee`, `:2513` in the UI batch) → release `TouchEnd::Box { start, end }` → `box_select`. Verified at code/test level (`box_marquee_rect_tracks_the_drag` green: rect = drag bounds ±1 px, fill reaches `scene.ui.white`, clears on release; edge pan covered by separate tests).

Device evidence: **not produced in this slot** — `adb devices` returns an empty list (no attached transport), and `device-smoke.sh` requires a QA-installed APK built from the current tree (the 10-slot/row changes are not in any installed APK; no APK rebuild/install granted in this packet). Injected-event boxes remain a harness boundary (QA capture 634): `input motionevent` DOWN/MOVE/UP needs a real ≥300 ms hold before the first move, and timing through per-command injection is not reliable evidence of a physical gesture. Physical confirmation stays a coordinated QA/device step; nothing is claimed as passed.

### Round 7 close-out (same day, targeted re-verify on the current tree)

State check before re-run: `game.rs` sha256 `e7db1c688…` (row `[Rect; 10]` at :1166, slot-0 mapping :1188/:1222, test :3441), `wc3-input/src/lib.rs` sha256 `4ae84d83…` (`[Vec<u32>; 10]` at :183, `BOX_HOLD_SECONDS=0.30` :88, `TAP_SLOP=16` :86) — both intact, no new external revert.

Targeted gates (log `$PREFIX/tmp/opencode/r7-closeout.log`):

| Command | rc | Result |
|---|---|---|
| `cargo test -p wc3-input --offline hotkey_parse_and_control_group_cycle` | **0** | 1 passed, 21 filtered out |
| `cargo test -p wc3-game --lib --offline -- game::tests::control_group_row_is_on_screen_and_tappable game::tests::box_marquee_rect_tracks_the_drag --test-threads=1` | **0** | 2 passed, 120 filtered out |

This re-verifies control groups (slot 0 / 1..9,0 row + input cycle) and the box-select marquee path (`BOX_HOLD_SECONDS=0.30` → `DragMode::Box` → marquee → `TouchEnd::Box`) on the current tree. Device evidence still not produced (no attached adb transport, no APK rebuild/install in this packet) — harness boundary, unchanged.

## 1e. Round 8 — device/product verification (2026-11-13 19:44–19:51 MSK)

Detail + sha256: `analysis/agent-touch/ROUND8-DEVICE.md`. Product: `build/android-p0/herofolends-p0.apk` (built 19:35:09, sha256 `70c15e91…`), `pm install -r` Success, `GAME_FIRST_FRAME surface=2414x1080 phase=Playing`.

**Device-verified (screenshots + logcat):**

| Behaviour | Injection | Observed |
|---|---|---|
| ARMY selects the army | `input tap 2132 888` | `GAME_STATE selected=0 → 6` |
| Group-row hold assigns (slot 3) | `DOWN 1152 824; sleep 0.8; UP` | toast `GROUP 3 SAVED (6 UNITS)`, slot 3 count `6` |
| Box-select physical path | `DOWN 900 500; sleep 0.55; MOVE 1500 760` | marquee rectangle drawn; `UP` → `selected=6 → 1` |
| Group-row tap recalls (slot 3) | `input tap 1152 824` | `GAME_STATE selected=1 → 6` |

Layout on device (scale 1.0): console `Rect(210,792,2114,288)`, row slots centre `(1060+46i, 824)`, ARMY `(2132,888)`.

**Hotkeys:** gap found — the platform had no SDL keyboard handling (`Game::key_char` test-only; pre-fix APK: Ctrl+2/2 → `GAME_HOTKEY` count 0, selection unchanged). Fixed in `apps/android-native/src/lib.rs` (`SDL_EVENT_KEY_DOWN`, `wc3_input::scancode_char` → `a..z`/`1..9,0`, Ctrl mod, repeat ignored, Escape → `Hotkey::Escape`, `GAME_HOTKEY` log) + `wc3-input` app dependency; mapping unit-tested (`cargo test -p wc3-input --offline` rc 0, 23 passed). **Device-verified on the rebuilt APK** (sha256 `2d6b8f55…`, built 20:06:38, release rc 0 after the tree settled): `GAME_HOTKEY key=2 ctrl=true` + toast `GROUP 2 SAVED (6 UNITS)` (`hotkey-assign-ctrl2.png`); `GAME_HOTKEY key=2 ctrl=false`; `GAME_HOTKEY key=b ctrl=false` → `selected=1` (hall). Non-empty recall in one clean window was not captured (device focus stolen by other apps); recall semantics are covered by `hotkeys_drive_the_card_and_control_groups` (rc 0) and the touch recall device evidence.

**Cross-layer device findings (not touch-owned):** `tombstone_17` 19:44:36 — `wgpu error: Validation Error … Surface::configure · Surface does not support the adapter's queue family` (GPU owner); shared-device monkey relaunch 19:45:30 and external app death 19:49:07 (coordinate device slots).

## 1f. Final product round — input tooling, row placement after UiScale, toolchain (2026-11-13 21:56–22:40)

No device run: `H2_DEVICE_SLOT` was not set; the ROUND 8 device evidence above stands.

**Input tooling (new, owner path):** `crates/wc3-input/src/trace.rs` + `crates/wc3-input/examples/touch-trace.rs` replay a device session without a phone. A trace is a line-oriented script (layout + `down/move/up/pinch_*/long_press` + `expect`) and the runner prints the router's canonical decisions (`start=…`, `move=…`, `up=…`, `pinch=…`) and exits non-zero on the first mismatch. Fixtures: `analysis/agent-touch/traces/{box-select,group-row,pinch-pan}.trace`.

```text
cargo test -p wc3-input --offline                         # rc 0, 31 passed (23 router/camera + 8 trace)
cargo run -p wc3-input --example touch-trace -- analysis/agent-touch/traces/box-select.trace   # PASS events=6
cargo run -p wc3-input --example touch-trace -- analysis/agent-touch/traces/group-row.trace    # PASS events=8
cargo run -p wc3-input --example touch-trace -- analysis/agent-touch/traces/pinch-pan.trace    # PASS events=3
```

The box fixture replays the *device* geometry (console 210,792,2114,288 at 1080p) and pins `up=box:900.0:500.0:1500.0:760.0` for the ≥0.30 s hold, and `move=pan:-100.0:-60.0` for a fast drag — the same numbers observed on the phone in ROUND 8.

**Group row placement (adapted to the HUD UiScale change):** the HUD console at 2414x1080 is now `(210,763.2,2114,316.8)` (scale 1.1, UiScale port), which made the old fixed row (`console.y+12s`, centred on the console) overlap the selection-panel blocker. `game.rs` `group_row_slots` now fits the 10-slot row into the console dead space between `selection.right()`/`minimap.right()`, `resource_row.bottom()` and `command.x`, centred, shrinking slots only if the band is narrow (floor `24*s`). Verified by the touch-filtered suite (11/11):

```text
cargo test -p wc3-game --lib --offline -- game::tests::control_group game::tests::army_chip \
  game::tests::long_press game::tests::console_layout game::tests::tap_selects \
  game::tests::routing_prefers game::tests::box_marquee game::tests::hotkeys_drive \
  game::tests::command_card_tiles game::tests::safe_insets game::tests::pinch --test-threads=1
# rc 0, 11 passed / 0 failed
```

Touch tests were also made scenario-agnostic (`find(|u| u.owner == player && !u.is_building())` instead of a pinned Footman) because the scenario layer now spawns map units; the stale console/selection bounds in `console_hud_rects_do_not_swallow_world_taps` were replaced with proportional bounds (console anchored at the bottom within 0.5 px, selection < 35% of width) — the touch invariants are unchanged.

**Toolchain:** full audit + installs + smoke in `analysis/agent-touch/TOOLCHAIN_AUDIT.md` (assimp 6.0-st built from source, `gltf-transform` repaired with the wasm sharp, `three` 0.187.0, StormLib/`storm_probe` verified on the operator's War3.mpq: 10,525 entries rc 0; trimesh/pygltflib/ImageMagick/optipng/pngquant/ffmpeg/sox present). MDX→glTF exporter remains a formats/render deliverable (documented, not stubbed).

## 1g. Revival 377 — ui.selection.double / ui.selection.add / ui.hotkeys / ui.trace (2026-11-14)

Gap-map row 40 `ui.selection` was PARTIAL ("double-click type select missing"); row 38 `ui.hotkeys` PORTED. This round moves the selection *policy* into `wc3-input` so it is proven by the acceptance crate, and ships the game hook as an exact patch.

**Policy (`crates/wc3-input/src/selection.rs`):** `UnitSnapshot { id, owner, kind, pos, screen, alive }`, `same_type_on_screen(units, player, kind, viewport)` (living, same owner/kind, projected inside the viewport), `box_selection(units, player, min, max, additive, current)` (Shift keeps the group, dedupes, ignores dead/enemy), `merge(current, id, Replace|Add)`, `recall(groups, slot, units)` (stored order, dead dropped). `TouchInput` gained the host `additive` modifier (`set_additive`/`additive`) which survives `pointer_up`; the router's `TouchEnd::Tap { double }` already reports the double-tap.

**Game bridge (`crates/wc3-game/src/ui.rs`):** `pub mod selection` — `snapshot`, `type_select_on_screen`, `tap_select`, `box_select`, `long_press_add` over the canonical sim/camera; `cargo check -p wc3-game --lib` rc 0.

**Game hooks (tech-lead applies):** `analysis/agent-touch/patches/0003-selection-parity.patch` (on-screen type select, additive tap/box, long-press add + `Game::set_additive`, base sha in header) and `0004-platform-shift.patch` (SDL Shift → `set_additive`). Both `patch -p1 --dry-run` rc 0.

**Trace replay:** `trace.rs` supports `shift on|off` and marks additive gestures (`up=box:…:add`, `up=tap:…:single:add`); `touch-trace --check` is the quiet CI form (non-zero exit on divergence). Fixtures: `analysis/agent-touch/traces/{box-select,group-row,pinch-pan,type-select,box-additive}.trace` → all PASS (`--check` rc 0; events 6/8/3/10/12).

**Tests (mechanic-mapped, TEST_POLICY):**
| Mechanic | Test file / names | Command | rc |
|---|---|---|---|
| `ui.hotkeys` | `tests/ui_hotkeys.rs`: `ui_hotkeys_card_letters_resolve_to_the_wc3_slots`, `ui_hotkeys_ctrl_digit_assigns_and_digit_recalls_the_selection`, `ui_hotkeys_recall_keeps_group_order_and_drops_dead_units` | `cargo test -p wc3-input --test ui_hotkeys` | 0 |
| `ui.selection.double` | `tests/ui_selection_double.rs`: `ui_selection_double_selects_the_same_type_on_screen`, `ui_selection_double_never_selects_other_kinds_or_enemies` | `cargo test -p wc3-input --test ui_selection_double` | 0 |
| `ui.selection.add` | `tests/ui_selection_add.rs`: `ui_selection_add_shift_box_keeps_the_group_and_appends`, `ui_selection_add_shift_tap_and_long_press_keep_the_group` | `cargo test -p wc3-input --test ui_selection_add` | 0 |
| `ui.trace` | `tests/ui_trace.rs`: `ui_trace_fixtures_replay_without_divergence`, `ui_trace_check_mode_reports_any_divergence` | `cargo test -p wc3-input --test ui_trace` | 0 |

Full acceptance gate: `cargo test -p wc3-input --offline` → **rc 0, 40 passed** (6 suites). Runner/gap-map handoff: `analysis/agent-touch/mechanic_tests.ui-input.md` (analysis/tech-lead is read-only). Tab subgroup cycling is absent in the current input layer (no `Tab` in `Hotkey`/game key path) — recorded, not stubbed; Escape cancel is covered by the pre-existing `hotkeys_drive_the_card_and_control_groups` in `game.rs`.

## 1h. FULL GOAL — input WC3-complete on touch + keyboard (2026-11-14)

Selection parity from §1g is merged (`ui::selection` calls present in `game.rs`, `Game::set_additive`). This round closes the remaining input gaps as input-layer policy + exact game/platform patches.

**New policy modules (`crates/wc3-input/src/`):**
- `remap.rs` — `Remap` table: `Remap::wc3()` defaults, `from_command_strings`/`apply_command_strings` (real `[CmdAttack] Hotkey=A` syntax), `apply_object_data` (`[hfoo] Hotkey=F`), `set_action`/`set_object` (configurable, reports the displaced binding), `action_for`/`object_for`/`card()`.
- `subgroups.rs` — `Subgroups::build` (hero kinds first, then unit types by first appearance), `cycle(reverse)`, `active_units`, `order()`.
- `orders.rs` — `OrderKind` (attack_move/move/patrol/hold/stop), `OrderArmer` (arm/take once/cancel), `order_for_hotkey` (A/M/P/H/S), `is_tab_scancode` (SDL 43), `MinimapAction` + `minimap_action(hold, right)` (tap→camera, hold→attack_move, right→move).
- `selection::UnitSnapshot` gained `hero: bool` (+ `.hero()`).

**Game bridge (`crates/wc3-game/src/ui.rs`):** `pub mod hotkeys` — `action_id(UiAction)` → `CmdAttack…CmdSelectArmy` (+ train/build/cast ids) and `game_remap()` (WC3 default table). `cargo check -p wc3-game --lib` rc 0.

**Trace replay:** `trace.rs` adds `key <letter>` (arms a mode), `escape` (cancel), `right on|off`; a world tap consumes an armed order exactly once (`…:order=attack_move`), minimap ups carry `…:action:camera|attack_move|move`.

**Patches (tech-lead applies; `patch -p1 --dry-run` rc 0 each):**
- `0005-input-completion.patch` — `Game` gets `hotkeys: Remap` + `subgroup_kind`; `activate_hotkey` resolves through `Game::hotkey_letter`; `set_hotkey`; `cycle_subgroups`; minimap long-press → `Command::MoveAttack`.
- `0006-platform-tab.patch` — SDL Tab/Shift+Tab → `game.cycle_subgroups(shift)`.

**Mechanic tests (TEST_POLICY: one file per group, id in the name):**

| Mechanic | Test file / names | Command | rc |
|---|---|---|---|
| `ui.hotkeys.remap` | `ui_hotkeys_remap_card_letters_come_from_command_strings`, `ui_hotkeys_remap_is_configurable_and_reports_the_displaced_binding`, `ui_hotkeys_remap_object_data_letters_drive_the_train_card` (`tests/ui_hotkeys_remap.rs`) | `cargo test -p wc3-input --test ui_hotkeys_remap` | 0 (3) |
| `ui.groups.tab` | `ui_groups_tab_cycles_unit_types_without_a_hero`, `ui_groups_tab_hero_is_first_and_shift_tab_reverses` (`tests/ui_groups_tab.rs`) | `cargo test -p wc3-input --test ui_groups_tab` | 0 (2) |
| `ui.orders.targeting` | `ui_orders_targeting_each_card_letter_arms_its_mode`, `ui_orders_targeting_tap_consumes_once_and_escape_cancels`, `ui_orders_targeting_trace_replays_arm_consume_and_cancel` (`tests/ui_orders_targeting.rs`) | `cargo test -p wc3-input --test ui_orders_targeting` | 0 (3) |
| `ui.minimap.orders` | `ui_minimap_orders_tap_hold_and_right_click_map_to_intents`, `ui_minimap_orders_trace_replays_camera_attack_move_and_move` (`tests/ui_minimap_orders.rs`) | `cargo test -p wc3-input --test ui_minimap_orders` | 0 (2) |
| `ui.trace` | `ui_trace_fixtures_replay_without_divergence` (7 fixtures), `ui_trace_check_mode_reports_any_divergence` (`tests/ui_trace.rs`) | `cargo test -p wc3-input --test ui_trace` | 0 (2) |
| `ui.hotkeys` | `ui_hotkeys_*` (`tests/ui_hotkeys.rs`) | `cargo test -p wc3-input --test ui_hotkeys` | 0 (3) |
| `ui.selection.double` / `.add` | `tests/ui_selection_double.rs` / `ui_selection_add.rs` | per file | 0 (2/2) |

Acceptance: `CARGO_BUILD_JOBS=2 cargo test -p wc3-input --offline` → **rc 0, 50 passed, 0 failed** (31 unit + 19 integration). Fixtures: `touch-trace --check` on all 7 → PASS rc 0 (events 6/8/3/10/12/25/12).

Tab is routed by the platform patch; Escape cancel is the existing game handler (clears placing/attack/move/patrol/targeting). Remaining runtime wiring of `subgroups`/`orders` lands with 0005/0006; the remap table is already consumed by `ui::hotkeys` in the game crate.

## 1i. TOUCH FULL GOAL — playable like WC3 (2026-11-14)

**Build placement (ui.placement).** `Game::begin_placement` (applied, read-only) already puts the ghost at the screen centre on BUILD; the ghost follows the finger through `placing_at` and tints green/red from `sim.can_place` every frame. This round adds the input-side proofs and the remaining hooks: `placement.trace` (placing on → drag does not place → tap places → placing off returns world taps) and patch `0007` (BUILD again cancels; two-finger right-click; contextual taps). Device capture: `analysis/agent-touch/device-touch-final/ghost-valid.png` (sha256 `5dc440d8…`) — translucent green town-hall ghost over grass with the `TAP GROUND TO PLACE TOWN HALL` toast, worker card visible. Red/invalid capture attempted (`device-ghost-capture.sh`, `device-ghost-invalid.sh`): the device window is ~60 s and the worker/build tile positions drift with the camera; the green frame is the delivered screenshot, the tint path is `sim.can_place` in the read-only ghost draw.

**Gestures → traces (all `touch-trace --check` PASS):**

| Gesture | Fixture | Events |
|---|---|---|
| place ghost / tap place / drag-not-place / cancel | `placement.trace` | 10 |
| attack (tap enemy), harvest (tap mine/tree), return (tap hall), build/repair (tap site), rally (tap, building selected), ARMY chip hold/tap | `touch-orders.trace` | 14 |
| attack-move (A→tap), move (M→tap), stop (S→tap), hold (H→tap) | `orders-targeting.trace` | 27 |
| patrol two ground taps (+Escape between) | `patrol-two-taps.trace` | 13 |
| minimap tap (camera), drag (scrub), long-press (attack-move), two-finger tap (right-click move) | `minimap-orders.trace` | 16 |
| control-group row hold/tap/double | `group-row.trace` | 8 |
| double-tap type select | `type-select.trace` | 10 |
| Shift box add / Shift tap add | `box-additive.trace` | 12 |
| box marquee, pinch/pan | `box-select.trace`, `pinch-pan.trace` | 6, 4 |

**Input additions.** `pinch_end() -> Option<Vec2>` (a stationary two-finger tap = touch right-click; a real pan returns `None`), `PatrolPlan` (two-tap patrol, Escape cancels), trace steps `placing`, `right`, `key`, `escape`, `two_finger_tap`, patrol first/second and minimap `action` suffixes.

**Patches (dry-run rc 0):** `0007-touch-placement-orders.patch` — BUILD toggle cancel, two-finger right-click (`right_click`: minimap move / attack picked enemy / move selection), contextual taps (`touch_tap_intent`: harvest mine/tree, repair damaged friendly building with workers, return to hall, rally with a building selected). `0005`/`0006` are applied in the tree (`cycle_subgroups` + platform Tab present).

**Gate.** My suites: `ui_hotkeys` 3, `ui_hotkeys_remap` 3, `ui_selection_double` 2, `ui_selection_add` 2, `ui_groups_tab` 2, `ui_orders_targeting` 4, `ui_minimap_orders` 3, `ui_trace` 2 (10 fixtures) + 31 unit tests — last clean full run **52/52 rc 0**. During this round a second agent began adding `session`/`geometry` modules to `wc3-input` (files at 12:44–12:49); their in-flight tests (`geometry::tests::swept_ball_ball_hits_at_the_analytic_time`, `..._contact_matches_parry_normals_and_witnesses`, `session::tests::stationary_two_finger_tap_is_reported`) fail and their cargo locks time out the crate gate — my suites and all 10 fixtures remain green.
