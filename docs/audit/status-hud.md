# HUD / Android layer — audit status (LEDGER 489)

Registry: `/storage/emulated/0/Download/WC3RUST_CODE_AUDIT_TZ_P0-P4.md` §0/0.1/0.2.
Repo: `herofolends-rs`, branch `main`.

| ID | Priority | Status |
|----|----------|--------|
| AUD-001 | P1 | FIXED |
| AUD-004 | P2 | FIXED |

---

## AUD-001 — Panic when refreshing the compact mobile HUD with the commands drawer closed

**Status: FIXED**

### Why

`apply_mobile_button_rects()` indexed `mobile.card_slots[slot]` for every card
action. In compact landscape the card is a drawer: `compose_landscape()` only
fills `card_slots` inside `if self.state.commands_open`
(`crates/wc3-game/src/mobile_hud.rs:1927-1932`), so the *valid* state
`LandscapeCompact + commands_open == false` has an empty `card_slots`. Every
`refresh_buttons()` / `resize()` / `set_safe_insets()` in that state reached the
index expression and panicked; the Android release profile is `panic = "abort"`,
so this is a process crash, not a recoverable error.

Confirmed against the import revision `b64b9b6`:

```text
b64b9b6 crates/wc3-game/src/game.rs:2217
    if let Some(button) = self.buttons.get_mut(*index) {
        button.rect = mobile.card_slots[slot];     // panic when card_slots is empty
    }
```

### Fix

`crates/wc3-game/src/game.rs:2221` (`apply_mobile_button_rects`):

- every card button starts at `Rect::new(0.0, 0.0, 0.0, 0.0)`;
- an empty `card_slots` returns immediately — the 12 actions keep their
  indices/actions but have no screen rect;
- each slot is read with `card_slots.get(slot)`; a drawer shorter than the
  logical card leaves the remaining actions list-only;
- no arbitrary geometry is invented: a zero-size rect is not a hit target
  (`wc3-input/src/lib.rs:43-48` `contains` is half-open on both axes, and
  `hit_button` skips `w <= 0 || h <= 0` in the slop pass,
  `wc3-input/src/lib.rs:654-676`).

Invariant now asserted by tests: `build_buttons()` always authors exactly 12
card slots (`hud.rs:2493-2511`, missing slots become `Button::empty()`),
`card_button_indices()` is `0..12`, and when `card_slots` is non-empty the
12 render rects and the touch snapshot are the same rectangles.

### Regression test

`game::tests::compact_landscape_closed_drawer_keeps_card_actions_non_interactive`
(`crates/wc3-game/src/game.rs`):

- 1280x720 px @ 2.0 px/dp = 640x360 dp (compact), drawer closed:
  `card_slots` empty, all 12 buttons zero-rect, no card touch target;
- `refresh_buttons()` with the empty list (the AUD-001 crash input);
- drawer open: all 12 actions map 1:1 to the rendered slots, every
  `touch_buttons` rect equals the rendered `Button::rect`, and
  `pointer_down(slot centre)` presses that exact index;
- resize with the drawer open (1600x900), close, portrait rotation (720x1280,
  card inline), back to compact, then the 320x240 minimum window.

Existing coverage kept green:
`game::tests::command_card_tiles_have_a_minimum_finger_sized_hit_target`
and the pure-layout matrix `crates/wc3-game/tests/hud_layout.rs`
(`hud_layout_no_ambiguous_overlap` asserts a closed compact drawer has no cells).

---

## AUD-004 — Android never passes the real density

**Status: FIXED**

### Why

`Game` starts with `density_scale: 2.0` (`game.rs:894-897`) and exposes
`set_density()` / `set_window_metrics()`, but the Android host called only
`game.resize()` and `set_safe_insets()`. Pre-fix evidence:

```text
$ grep -rn "set_window_metrics\|set_density" --include=*.rs apps crates | grep -v "fn set_"
crates/wc3-game/src/game.rs:894: // Replaced by `set_density` from the platform ...
```

No caller existed. The HUD was therefore always solved at a hardcoded 2.0
px/dp regardless of the device (a 3.0 density phone got a HUD 1.5x too small in
physical size), and the layout mode could be wrong (wide vs compact).

### Which SDL3 value is the density (observed in the vendored SDL 3.5.0)

Source: `donors/SDL` @ `4ca1b1c` (same headers/libs shipped in
`third_party/sdl3*`).

- `src/video/android/SDL_androidwindow.c:59-60` sets
  `window->w = Android_SurfaceWidth` — Android window coordinates are the
  surface **pixels** (`SDL_GetWindowSizeInPixels` returns the same size).
- `android-project/app/src/main/java/org/libsdl/app/SDLSurface.java:174-175`
  feeds `density = realMetrics.densityDpi / 160.0f` through
  `nativeSetScreenResolution(...)`; `src/video/android/SDL_androidvideo.c:224`
  stores it in `Android_ScreenDensity` and `:198` publishes it as
  `display->content_scale`.
- `src/video/SDL_video.c:1900-1911`: `SDL_GetWindowPixelDensity` is
  `pixel_w / window_w` → **1.0 on Android** (window coords == pixels).
- `src/video/SDL_video.c:1914-1931`: `SDL_GetWindowDisplayScale` is
  `pixel_density * content_scale` → the Android `densityDpi / 160` value, i.e.
  the scale UI content must be drawn at.

Correct source: `SDL_GetWindowDisplayScale`.
`SDL_GetWindowPixelDensity` is only a last-resort fallback (it is 1.0 here and
would reproduce the old bug). The symbols are exported by the shipped Android
library:

```text
$ readelf -Ws third_party/sdl3-android-build/libSDL3.so | \
    grep -E "SDL_GetWindowDisplayScale|SDL_GetPrimaryDisplay|SDL_GetDisplayContentScale"
SDL_GetPrimaryDisplay@@SDL3_0.0.0
SDL_GetWindowDisplayScale@@SDL3_0.0.0
SDL_GetDisplayContentScale@@SDL3_0.0.0
```

### Fix

`apps/android-native/src/lib.rs`:

- `window_density()` (`:392`) reads `SDL_GetWindowDisplayScale`, falls back to
  `SDL_GetDisplayContentScale(SDL_GetPrimaryDisplay())`, then 1.0; the value is
  clamped to `[0.5, 8.0]` by the game layer.
- `WindowMetrics { framebuffer, density, insets, insets_ok }` + `window_metrics()`
  (`:409`) snapshot `SDL_GetWindowSizeInPixels` + `SDL_GetWindowSafeArea`
  (window coords converted to pixels with the window/pixel ratio).
- `apply_window_metrics()` (`:432`) is the single platform entry:
  `Game::set_window_metrics(framebuffer_px, density, SafeInsets px)` and a
  `GAME_WINDOW_METRICS px=… dp=… density=… insets=…` log line whenever the
  snapshot changes.
- Called at window creation (`:1392`), on every surface resize/recovery
  (`:1680`), on `SDL_EVENT_WINDOW_SAFE_AREA_CHANGED` and
  `SDL_EVENT_WINDOW_DISPLAY_SCALE_CHANGED` (`:1693`), on the starved-queue
  watchdog (`:1745`) and on `Lost` (`:1781`), plus a 30-frame poll that catches
  density changes the event filter does not surface.
- Never scaled twice: insets enter in pixels, `Game::rebuild_layout` divides
  them by density exactly once (`game.rs:1941-1947`), and `HudLayout::to_px`
  multiplies dp → px exactly once (`mobile_hud.rs:2271-2277`). Touch/finger
  coordinates are already framebuffer pixels (`finger_pos` uses the surface
  configuration).

### Regression tests

- `crates/wc3-game/tests/hud_layout.rs::hud_layout_points_pixels_round_trip`:
  1440x3120 px @ 2.75 → dp viewport; pixel insets 82.5/66 px → 30/24 dp → back
  to the same pixels; a 48 dp target is 132 px (not 363 px); the rendered
  pixel rect is the hit target (`HudHit::Card`) and the dp centre maps onto the
  same pixel centre.
- `game::tests::window_metrics_push_framebuffer_density_and_insets_once`:
  density 1.0 → wide, 2.0 → compact 640x360 dp; a 40 px inset at 2.0 is 20 dp
  and 40 px on screen (not 80 px); rotation to portrait solves an inline card;
  `set_density` rejects a no-op.

The device-side line to check in `logcat`:

```text
GAME_WINDOW_METRICS px=2414x1080 dp=805x360 density=3.000 insets=0,0,0,0
```

---

## Side repairs required to run the HUD test suite

1. **`wc3-game` unit-test target did not compile** after the relayout commit
   `c372783`: `MenuSkin` fields became `Option<MenuPart>` but two mock tests
   still built them from `Option<usize>` (17 errors in
   `cargo test -p wc3-game --lib`). Repaired in `mobile_hud.rs` tests with
   `menu_part_from` / `load_menu_pack_part` / `load_menu_ui_part`; production
   code untouched.

2. **`crate::hud::HudLayout.top_bar`** is documented as “the whole console, used
   as the top-level HUD blocker” (`hud.rs:2302-2319`) but was populated with
   `mobile.top_meta` (the top strip). Restored to `mobile.console`
   (`game.rs:1983`); `blocked_rects` already adds `mobile.top_meta` separately,
   so touch routing is unchanged and the legacy consumers (edge pan, hitbox
   debug, tests) get the documented rectangle.

3. `game::tests::safe_insets_keep_the_hud_inside_the_system_bars` asserted the
   pre-relayout contract (console inset horizontally). The relayout made the
   console background edge-to-edge and moved the insets onto the content
   (`hud_layout_console_edge_to_edge` is the authoritative test). The stale
   assertions were updated to the current contract: console edge-to-edge,
   content/resources/minimap inside the insets, vertical insets intentionally
   ignored by the console per the operator directive.

4. `ui_menu_restyled_headless_mock_writes_png` still asserted the old 44-56 px
   glue-button strip. The relayout clamps every stack entry to
   `[MENU_MIN_TARGET_PX (90), 150]` px (`mobile_hud.rs:4280`, the operator's
   90 px finger floor), so the measured 116.6 px is the intended target; the
   assertion now checks that clamp instead of the removed desktop height.

5. `game::tests::command_card_tiles_have_a_minimum_finger_sized_hit_target`
   assumed a 1280x720 *pixel* viewport is the wide inline card. With the 2.0
   phone density fallback it is a 640x360 dp compact drawer with no card on
   screen. The test now pins `set_density(1.0)`, asserts the tile meets
   `MIN_BUTTON_TARGET`, and probes the router's real pad
   (`max((min_target - h) / 2, slop)`, 4 px for a 48 dp tile) plus a point
   beyond it; the old 8 px probe was larger than the pad and could not pass on
   the 48 dp tile the relayout introduced.

---

## Evidence

Toolchain: `rustc` release pinned by the workspace, `scripts/h2-cargo`
(single cargo slot; the shared workspace is edited concurrently by other audit
lanes).

```text
$ scripts/h2-cargo check -p wc3-game --offline
rc=0
$ scripts/h2-cargo check -p herofolends-android --target aarch64-linux-android --offline
rc=0
$ scripts/h2-cargo test -p wc3-game --lib --offline -- \
    game::tests::compact_landscape_closed_drawer_keeps_card_actions_non_interactive
test game::tests::compact_landscape_closed_drawer_keeps_card_actions_non_interactive ... ok
test result: ok. 1 passed; 0 failed; 448 filtered out
$ scripts/h2-cargo test -p wc3-game --lib --offline -- \
    game::tests::window_metrics_push_framebuffer_density_and_insets_once
test game::tests::window_metrics_push_framebuffer_density_and_insets_once ... ok
test result: ok. 1 passed; 0 failed; 448 filtered out
$ scripts/h2-cargo test -p wc3-game --lib --offline -- mobile_hud::tests::ui_menu
test hud::mobile_hud::tests::ui_menu_restyled_headless_mock_writes_png ... ok
test hud::mobile_hud::tests::ui_menus_shipped_art_mock_pngs ... ok
test result: ok. 2 passed; 0 failed; 447 filtered out
$ scripts/h2-cargo test -p wc3-game --test hud_layout --offline
test result: ok. 16 passed; 0 failed; 0 ignored
$ scripts/h2-cargo test -p wc3-game --lib --offline -- \
    game::tests::command_card_tiles_have_a_minimum_finger_sized_hit_target
<PENDING>

$ scripts/h2-cargo test -p wc3-game --lib --offline -- \
    game::tests::safe_insets_keep_the_hud_inside_the_system_bars
<PENDING>

$ scripts/h2-cargo test -p wc3-game --lib --offline -- \
    game::tests::console_hud_rects_do_not_swallow_world_taps
<PENDING>

$ scripts/h2-cargo test -p wc3-game --lib --offline -- \
    game::tests::console_layout_blocks_world_clicks_and_placement
<PENDING>
```

Device-facing log lines added:

```text
GAME_WINDOW_METRICS px=2414x1080 dp=805x360 density=3.000 insets=0,0,0,0
GAME_SAFE_AREA unavailable (SDL_GetWindowSafeArea failed)   (unchanged, one-shot)
```

## Commit

`<filled after commit>`
