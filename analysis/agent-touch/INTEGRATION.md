# Touch layer — integration notes for other owners

Owner: `agent-touch` (TOUCH/CAMERA/PLATFORM layer). Written 2026-11-12.

This file lists what the touch/immersive layer expects from layers it does not
own. Nothing here was edited outside the owned paths
(`crates/wc3-input/**`, `analysis/agent-touch/**`) plus the two read-only patches
listed below.

## 0. Merge state (single base, 2026-11-12)

`crates/wc3-input` is now the only gesture/camera implementation:
`TouchInput` (routing + gestures), `TouchLayout`/`hit_button` (HUD hit zones),
`Camera` and the minimap mapping pair. The virtual joystick was removed — the
canonical camera is drag/pinch/minimap/edge scroll.

`crates/wc3-game/src/game.rs` is HUD-owned, so the integration is delivered as
`analysis/agent-touch/patches/0001-game-input-delegates.patch`; `camera.rs`
becomes a `pub use wc3_input::Camera` facade in
`analysis/agent-touch/patches/0002-camera-facade.patch`. Full port map:
`analysis/tech-lead/merge-touch.md`. Haptics (§1) are unaffected by the merge.

## 1. Android manifest — VIBRATE permission (owner: app packaging / QA)

Haptics are implemented in `apps/android-native/src/lib.rs` through SDL's JNI
bridge (`SDL_GetAndroidJNIEnv` / `SDL_GetAndroidActivity` → `android.os.Vibrator`).
`Vibrator.vibrate(long)` requires the normal permission below; without it the
call throws `SecurityException`, which the code detects, clears, disables
haptics for the process and logs:

```text
GAME_HAPTICS disabled: vibrate() threw (VIBRATE permission missing?)
```

Add to `apps/android/app/src/main/AndroidManifest.xml`, next to `<uses-sdk>`:

```xml
<uses-permission android:name="android.permission.VIBRATE" />
```

Until then the game still runs: haptics are the only degraded part, and the log
line above is the expected evidence.

## 2. HUD layout — no hud.rs changes were needed

The safe area is applied in `Game::rebuild_layout()`: `HudLayout::new()` is built
for the reduced safe viewport and every rect (`top_bar`, `resource_row`,
`minimap`, `selection`, `command`, `context_row`, `primary[4]`, `chips[4]`) is
translated by `(inset.left, inset.top)`.
`build_hud` keeps drawing the same fields; only the top map-name/FPS text moved
by `safe_insets.top + 22 * scale`.

If `hud.rs` later gains explicit inset fields, keep these invariants:

* `layout.minimap.contains()` must never overlap the system gesture area at the
  bottom/left in landscape.
* `layout.command` is the HUD blocker rectangle: touches inside it but outside a
  button must not reach the world.
* `HudLayout::joystick_center/joystick_radius` are **legacy** after the merge:
  the virtual joystick is removed and no input path reads them. They can be
  deleted from `hud.rs` when the HUD owner next touches the layout.

## 3. Renderer — model yaw offset (owner: world/assets agent, implemented)

`Game::scene()` now builds the unit matrix with
`Quat::from_rotation_y(-unit.facing + self.assets.model(unit.kind).yaw_offset)`.
`ModelAsset.yaw_offset` is `PI` for the Archer and `0` otherwise. Verified by the
unit test `scene_applies_the_model_yaw_offset` (compares the decomposed draw
matrix against the expected quaternion). Device screenshot/video confirmation is
still required from the QA build.

## 4. Touch routing contract (implemented in `wc3-input::TouchInput`)

Order on `pointer_down` (first match wins), identical to the pre-merge game
behavior but owned by the input layer:

1. Minimap — camera jump, drag scrubs the camera, 0.45 s hold issues a move
   order.
2. Command buttons — exact rect, then a `10 px * scale` expanded band with a
   minimum `108 px * scale` target and nearest-centre disambiguation.
3. HUD blockers — `top_bar`, `command`, `selection`.
4. Building placement (world only, no drag).
5. World — tap / pan / drag-box.

No joystick step: the canonical camera is drag/pinch/minimap/edge scroll.
Two pointers mean pinch + two-finger pan (centroid delta); lifting one finger
resumes one-finger panning without a synthetic tap.

## 5. Platform — SDL3 behaviour relied upon

* `SDL_SetWindowFullscreen(window, true)` after `SDL_CreateWindow` →
  `Android_SetWindowFullscreen` → `Android_JNI_SetWindowStyle(true)` →
  `SDLActivity.setWindowStyle(true)` → `COMMAND_CHANGE_WINDOW_STYLE` →
  `WindowInsetsController.hide(systemBars())` +
  `BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE` (API 30+), legacy flags below 30.
* `SDL_GetWindowSafeArea` returns the rectangle fed by
  `SDLSurface.onApplyWindowInsets` (`systemBars | systemGestures |
  mandatorySystemGestures | tappableElement | displayCutout`), so the HUD
  clears both bars and the camera cutout.
* Expected log lines for device QA:
  `GAME_FULLSCREEN entered=true flag=true flags=0x...`,
  `GAME_SAFE_AREA left=0 top=... right=... bottom=...`,
  `GAME_FIRST_FRAME ... fullscreen=true safe=SafeInsets { ... }`,
  `GAME_HAPTICS ready ...` (or the unavailable line from §1).
