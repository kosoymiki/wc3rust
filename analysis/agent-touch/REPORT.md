# TOUCH / CAMERA / PLATFORM layer — final report

Task ref: `agent-touch`. Owner paths: `crates/wc3-game/src/game.rs` (input/update),
`crates/wc3-game/src/camera.rs`, `crates/wc3-input/src/lib.rs`,
`apps/android-native/src/lib.rs`, `analysis/agent-touch/**`.
Date: 2026-11-12. Device: RMX3709, Android 16 (SDK 36), aarch64, KernelSU root.

## Goal

Turn the RTS slice into a touch-first mobile UX: immersive fullscreen with no
system bars over the HUD, safe-area/cutout insets, a virtual-joystick camera,
deterministic touch routing, pinch/two-finger pan, drag-box selection, large
hit targets, haptic feedback, plus tests and device proof.

## Actions

### `apps/android-native/src/lib.rs` (platform layer)

* After `SDL_CreateWindow`: `SDL_SetWindowFullscreenMode(window, NULL)` +
  `SDL_SetWindowFullscreen(window, true)` + `SDL_SyncWindow` +
  `SDL_DisableScreenSaver`, with a `GAME_FULLSCREEN entered=… flag=… flags=…`
  log. This is the call chain that reaches
  `SDLActivity.setWindowStyle(true)` → `COMMAND_CHANGE_WINDOW_STYLE` →
  `WindowInsetsController.hide(systemBars())` +
  `BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE` (API 30+).
* Bounded fullscreen retry (3 attempts, 900 ms apart) if SDL loses the flag.
* `SDL_GetWindowSafeArea` → `Game::set_safe_insets(left, top, right, bottom)`
  (window→pixel scale corrected), applied at start, on resize, on
  `SDL_EVENT_WINDOW_SAFE_AREA_CHANGED` (0x215) and every 30 frames; logs
  `GAME_SAFE_AREA …` only on change.
* Input: two-finger centroid drives pinch **and** two-finger pan; when one
  finger lifts, panning resumes on the remaining finger (`pointer_resume_pan`)
  instead of going dead; joystick ownership is released by `pinch_begin`.
* Haptics: SDL3 has no phone-vibration API, so `android.os.Vibrator.vibrate(J)V`
  is reached through `SDL_GetAndroidJNIEnv` / `SDL_GetAndroidActivity`
  (one-time resolution, global ref, JNI exception check that disables haptics
  safely and logs why).
* First-frame log now carries `fullscreen=` and `safe=SafeInsets { … }`.

### `crates/wc3-game/src/game.rs`

* `Gesture::Joystick`: pointer-down inside `layout.joystick_center` +
  `joystick_radius` + 12 px grab slop enters it, move feeds a normalized vector,
  `update()` pans the camera continuously at 900 px/s × HUD scale, release
  resets; it never selects or issues orders underneath.
* Routing order: minimap → command buttons (exact rect, then a finger-sized
  band with a minimum 108 px target and nearest-centre disambiguation) →
  joystick → HUD blockers (`top_bar`, `command`, `selection`) → placement →
  world tap/pan/box.
* Two-finger `PinchState` (start distance/zoom + last centroid): ratio zooms
  (700–3600), centroid delta pans like a drag.
* Drag-box selection kept (hold ≥ 0.30 s, slop 16 px) and gained edge
  auto-scroll while the box is held at a safe-area edge.
* Safe-area layout: `rebuild_layout()` builds `HudLayout` for the reduced safe
  viewport and translates every HUD rect + `joystick_center` by the inset
  origin; the 3-D scene stays edge-to-edge. Top map-name/FPS text is offset by
  `safe_insets.top`.
* Unit pick radius enlarged (52/110 px × scale for units/buildings).
* `scene()` applies `-facing + assets.model(kind).yaw_offset` (Archer = π).
* `HapticCue` queue (`Press/Select/Order/Deny/Joystick`) drained by the app.

### `crates/wc3-input/src/lib.rs`

`VirtualJoystick` (expanded hit zone, dead zone, saturation, release) with
unit tests.

### `crates/wc3-game/src/camera.rs`

Tests for pan direction/sub-pixel accumulation, clamping, zoom clamps,
screen↔world round trip and terrain-height picking.

### `analysis/agent-touch/`

`device-smoke.sh` (root smoke: restart, wait first frame, dumpsys focus/insets,
screencap, asserts `statusBars`/`navigationBars visible=false`),
`INTEGRATION.md` (VIBRATE permission request, HUD invariants, routing contract).

## Evidence (observed)

Build:

```text
cargo check -p wc3-game          -> rc=0, 0 errors
cargo check -p herofolends-android -> rc=0, 0 errors
cargo test -p wc3-input          -> 6 passed
cargo test -p wc3-game --lib     -> 52 passed, 1 failed (NOT this layer:
  assets::geoaset_tests::geoaset_skinning_weights_are_normalised, assets.rs:1348,
  owner = world/assets agent), 3 ignored
  all 17 game/camera tests incl. joystick/routing/pinch/box/insets/yaw pass
cargo build -p herofolends-android --release -> rc=0 (libmain.so 7.2 MB)
scripts/build-apk.sh             -> rc=0, APK 5.2 MB, apksigner verify OK
pm install -r                    -> Success
am start -W                      -> Status ok, LaunchState COLD, TotalTime 649 ms
```

Device (new APK, `analysis/agent-touch/device-verify/`):

```text
I wc3rust: GAME_FULLSCREEN entered=true flag=true flags=0x10002621
I wc3rust: GAME_SURFACE 2414x1080 format=Rgba8UnormSrgb present=Fifo
I wc3rust: GAME_SAFE_AREA left=210 top=120 right=90 bottom=96
I wc3rust: GAME_FIRST_FRAME surface=2414x1080 fullscreen=true
               safe=SafeInsets { left: 210.0, top: 120.0, right: 90.0, bottom: 96.0 } units=11 phase=Playing
I wc3rust: GAME_HAPTICS ready method=Vibrator.vibrate(J)V
I wc3rust: GAME_HAPTICS disabled: vibrate() threw (VIBRATE permission missing?)

dumpsys window:
  mCurrentFocus=Window{65e31b u0 org.herofolends.p0/org.libsdl.app.SDLActivity}
  InsetsSource type=navigationBars frame=[0,1032][2414,1080] visible=false
  InsetsSource type=statusBars     frame=[0,0][2414,120]   visible=false
SMOKE_PASS: fullscreen + hidden system bars + safe-area insets
```

Screenshot proof (`device-verify/screenshot.png`, 2414×1080 landscape):

* top 120 px band: mean RGB (147,161,149), **0.0000 %** bright pixels → no clock,
  battery or status icons (the old build's top band contained them).
* bottom 96 px band: mean (112,127,64), no nav bar.
* minimap region at the inset position (227,198)–(438,352) is 86.4 % dark green,
  i.e. the HUD was translated by the reported safe insets (210,120).
* installed `classes.dex` contains `WindowInsetsController`,
  `setSystemBarsBehavior`, `COMMAND_CHANGE_WINDOW_STYLE`, `setWindowStyle`;
  `libmain.so` references `SDL_SetWindowFullscreen` → the shipped APK contains
  both sides of the immersive path.
* `aapt2 dump permissions` lists **no permissions** → the VIBRATE denial above is
  a packaging gap, not a code error (see INTEGRATION.md §1).

## Result

* Immersive fullscreen: **verified on device** — status and navigation bars are
  `visible=false`, the app is focused, and the screenshot shows no system bars.
* Safe-area insets: **verified on device** — the HUD is laid out inside
  `left=210 top=120 right=90 bottom=96`; the minimap lands exactly at the
  translated position.
* Virtual joystick, routing order, expanded/min-size hit targets, pinch zoom,
  two-finger pan, drag-box + edge scroll, haptic cues and the Archer yaw offset:
  implemented and unit-verified (17 game/camera tests green).
* Haptics: implemented and wired; on this APK the vibrator call is denied because
  the manifest has no `VIBRATE` permission (packaging owner, exact line in
  INTEGRATION.md). The code logs the failure and disables haptics without
  crashing.
* Archer yaw offset reaches the scene matrix (unit test); on-device visual
  confirmation of the running archer is a QA/vision item.
* Not caused by this layer, observed while testing: `wc3-game` visual test fails
  with `Index 13266 extends beyond limit 12762` (renderer/geometry, owner =
  render agent) and the HUD chrome is low-contrast over the current bright
  terrain (HUD/terrain agents).

## Risks / next

* SDL's Android safe area is the union of system bars + gestures + cutout and
  keeps reporting bar thickness while the bars are hidden
  (`getInsets` ignores visibility). The HUD therefore keeps a comfortable margin
  even in immersive mode. This is deliberate: sticky-immersive transient swipes
  must not cover the HUD. It can be tightened only by reading the cutout-only
  insets over JNI (a platform-layer change, not done here).
* `VIBRATE` permission is the one packaging dependency (INTEGRATION.md §1).
* Device verification used the shared build tree at 15:48; other agents were
  editing `mobile_hud.rs`/`terrain_render.rs`/`ui.rs` concurrently, so a later
  integration build must re-run `analysis/agent-touch/device-smoke.sh`.

## H2_PERSISTED

| layer | id |
| --- | --- |
| monitor ledger `agent-touch` | 306 (closed, completed) |
| capture (research: SDL3 fullscreen/safe-area mechanics) | 544 |
| capture (device proof + test run) | 549 |
| experience (SDL3 Android immersive chain + safe area) | 4258 |
| experience (SDL safe area includes hidden bars; verify via dumpsys) | 4259 |
| device evidence dir | `analysis/agent-touch/device-verify/` |
| verified screenshot copy | `/sdcard/Download/wc3rust-immersive-verified.png` |
| smoke script | `analysis/agent-touch/device-smoke.sh` |
