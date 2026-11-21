# merge-touch — TOUCH/INPUT layer port manifest

Status: **DONE (scope)** — the input layer is merged into the game path and
compiles: `cargo check -p wc3-input` rc 0, `cargo check -p wc3-input -p wc3-game
--lib` rc 0, `cargo check -p herofolends-android` rc 0, `cargo test -p wc3-input`
19/19, `cargo test -p wc3-game --lib game::tests::` 17/17. No device runs.
Details: §9–§10.

Authority: `analysis/tech-lead/BASE.md` v1 (2026-11-12) + this manifest.
Evidence paths are relative to `src/herofolends-rs/`.

## 1. Canonical target per concern

| Concern | Canonical owner (target) | Why | Everyone else |
|---|---|---|---|
| Gesture state machine (tap / double-tap / drag-pan / drag-box / placement / HUD press / minimap) | `wc3-input::TouchInput` (`crates/wc3-input/src/lib.rs:189`) | One recognizer; the copy in `wc3-game` is deleted by patch 0001 | `wc3-game::Game::pointer_*` becomes effect application only |
| Touch routing order (minimap → command card → HUD blockers → placement → world) | `wc3-input::TouchInput::pointer_down` + private `route` (`lib.rs:201`, `:454`) | Routing is input policy; the game supplies data via `TouchLayout` | game builds `TouchLayout` from its HUD rects |
| HUD hit zones (exact rect, finger band, minimum 108 px target, nearest-centre) | `wc3-input::hit_button` (`lib.rs:473`) + `TouchLayout` (`:66`) | Same policy for every host; no duplicated constants | game keeps only `refresh_buttons` + `blocked_rects` glue |
| Camera projection, pan, zoom, screen↔world, height picking | `wc3-input::Camera` (`lib.rs:506`; methods `:520-639`) | Shared by game + renderer + touch; types are identical, not copies | `wc3-game::camera` is a `pub use` facade (patch 0002) |
| Minimap↔world mapping | `wc3-input::{minimap_to_world, world_to_minimap}` (`lib.rs:641`, `:649`) | One inverse pair used by input and HUD drawing | game keeps a one-line `world_to_minimap` facade |
| Pinch zoom + two-finger pan | `TouchInput::pinch_begin/update/end` (`lib.rs:363-383`) | Zoom clamp and centroid pan are camera/input math | game applies `PinchUpdate{distance,pan}` |
| Edge auto-scroll during a box drag | `TouchInput::edge_pan` (`lib.rs:389`) | Band arithmetic belongs to the input layer | game passes the safe rect + scale |
| Virtual joystick | **removed** (no type in `wc3-input`; `game.rs` had already lost the field/gesture before this port) | The canonical RTS camera is drag/pinch/minimap/edge only; no HUD renders a joystick | — |
| Command card / order model (`CommandCard`, `RtsController`, `RtsOrder`) | **removed from `wc3-input`** | Unused second command model; HUD owns command buttons (`hud::build_buttons`, `UiAction`) | HUD/game |
| Haptics cue enum | stays in `wc3-game::game::HapticCue` | Effect-layer concern; the router emits semantic events, the game maps them | platform layer drains |

Proposed **BASE.md v2 row** (input has no row in v1 §2):

> Touch/input, gestures, camera math | `wc3-input` | `TouchInput`, `TouchLayout`,
> `TouchButton`, `touch events`, `hit_button`, `Camera`, `minimap_to_world`,
> `world_to_minimap` | `wc3-game` consumes; `wc3-game::camera` re-exports `Camera`.

## 2. OWNER files changed (this layer)

| File | Change | Evidence |
|---|---|---|
| `crates/wc3-input/src/lib.rs` | rewritten: single `TouchInput` router, `TouchLayout`/`TouchButton`/`Rect`, `Camera`, minimap mapping, 19 tests. Deleted `GestureRecognizer`, `Gesture`, `TouchPoint`, `VirtualJoystick`, `CommandCard`, `RtsController`, `RtsOrder`. | 951 lines; `grep -rn "wc3_input\|CommandCard\|RtsController\|GestureRecognizer\|VirtualJoystick" crates apps` outside the crate → **no consumers** |
| `analysis/agent-touch/patches/0001-game-input-delegates.patch` | ready patch for `crates/wc3-game/src/game.rs` (HUD owner) | `diff -u` against base sha256 `356ce2b8…` (game.rs, 2026-11-12) |
| `analysis/agent-touch/patches/0002-camera-facade.patch` | ready patch for `crates/wc3-game/src/camera.rs` (read-only) | whole-file replacement: `pub use wc3_input::Camera;` |
| `analysis/tech-lead/merge-touch.md` | this manifest | — |
| `analysis/agent-touch/INTEGRATION.md` | merge note: joystick removed, canonical API pointer | §0 |

`crates/wc3-input/Cargo.toml` is unchanged (`glam`, `wc3-core`); `wc3-game`
already depends on `wc3-input` (`crates/wc3-game/Cargo.toml:22`), so the facade
adds no dependency edge.

## 3. READ-ONLY patches (exact ranges)

### 3.1 `crates/wc3-game/src/game.rs` — patch 0001

Base snapshot (pre-merge): sha256 `22b67934a634f62a6832696d52d9feeace7d1477ecff4e1739ced1cb081f7ff0`
(HUD-owned; it changed repeatedly during the port). The patches are the
base→merged diff and were verified to reproduce the merged tree byte-exactly:
apply both to a fresh copy of this base and `diff -q` against the live files →
identical. Merged files: `game.rs` `c388d245…` (touch merge commit); the current
tree is `0dded63c…` after other-owner edits, with the touch markers verified
intact (11 `wc3_input` references, `touch`/`touch_buttons`, both gesture tests).
`camera.rs` `e166ef3e…`.
Regenerate after any HUD change with
`python3 analysis/agent-touch/tools/make_game_patch.py` (asserts every anchor;
prints the new sha and hunk counts).

| Hunk (`diff -u`) | What it does |
|---|---|
| `@@ -15,26 +15,10 @@` | add `use wc3_input::{TouchButton, TouchEnd, TouchInput, TouchLayout, TouchMove, TouchStart};`; delete the six local gesture constants, the unused `UiState` import and `MIN_SAFE_VIEWPORT` |
| `@@ -61,16 +45,6 @@` | delete `enum Gesture` |
| `@@ -102,21 +76,6 @@` | delete `struct PinchState`, `struct Pointer` |
| `@@ -189,10 +148,9 @@` | `Game` fields: `pointer`/`pinch`/`last_tap` → `touch: TouchInput`, `touch_buttons: Vec<TouchButton>` |
| `@@ -235,10 +193,9 @@` | `Game::new` initialisation |
| `@@ -273,10 +230,8 @@` | `reset_scenario`: `touch.reset()` |
| `@@ -397,7 +352,7 @@` | `reset_scenario` tail: `refresh_buttons()` |
| `@@ -435,8 +390,7 @@`, `@@ -455,8 +409,7 @@` | `resize` / `set_safe_insets`: one `refresh_buttons()` |
| `@@ -504,6 +457,34 @@` | add `refresh_buttons()` (build → reflow → snapshot, fixes the stale mobile rects) and `blocked_rects()` |
| `@@ -685,38 +666,22 @@` | `drive_camera` delegates to `TouchInput::edge_pan` |
| `@@ -887,168 +852,72 @@` | input section part 1: `pointer_down`/`pointer_move` + `button_at` removal, replaced by router calls + effects |
| `@@ -1056,10 +925,14 @@`, `@@ -1067,82 +940,50 @@` | input section part 2: `pointer_up` actions, `pointer_cancel`, `pointer_resume_pan`, `pinch_*` |
| `@@ -1150,13 +991,6 @@` | `tap_action(pos, double)`; delete the internal `last_tap` double-tap block |
| `@@ -1578,11 +1412,7 @@` | pressed state reads `touch.pressed_button()` |
| `@@ -1815,18 +1645,9 @@` | delete `minimap_to_world` and `world_to_minimap` (canonical pair lives in `wc3-input`); add `touch_rect()` |
| `@@ -2009,36 +1830,33 @@` | routing test: internal `pointer` asserts → `touch.*`; console dead space replaces the removed command-cluster dead space |
| `@@ -2081,7 +1899,7 @@`, `@@ -2096,10 +1914,7 @@`, `@@ -2116,10 +1931,7 @@`, `@@ -2142,7 +1954,7 @@`, `@@ -2283,9 +2103,7 @@` | pointer/box/none test asserts → `touch.pressed_button()/is_active()/box_rect()` |
| `@@ -2187,8 +1999,16 @@` | `button_at` test → `wc3_input::hit_button` with a `TouchLayout` |

Unchanged by the patch: selection/placement semantics (`tap_action` body,
`box_select`, `place_building`, minimap move orders), haptics, HUD rendering,
scenario/AI/economy.

### 3.2 `crates/wc3-game/src/camera.rs` — patch 0002

Base sha256 `fb460ee62503a31abc56f8bf66b2b0f7cb2fec9a9d63119f326a8e19a113bcfe`
(226 lines). Replace the whole file with:

```rust
//! RTS camera facade: the canonical implementation lives in `wc3-input` ...
pub use wc3_input::Camera;
```

Type identity: `wc3_game::camera::Camera` **is** `wc3_input::Camera`; `lib.rs`'s
`pub use camera::Camera` and every `game.rs`/`render.rs` call site keep working.
The 6 camera tests move to `crates/wc3-input/src/lib.rs` (same names).

### 3.3 `crates/wc3-game/src/ui.rs` — no patch required

`ui.rs` owns FDF layout and already exposes touch hit-testing
(`ui.rs:1497 hit_test`, `:1502 hit_test_interactive`, `:1506 hit_test_filtered`,
`:1538 hit_test_all`). It holds no input state and no duplicate gesture code.
The merged HUD can feed `wc3-input::TouchButton` rects from `UiLayout` without a
change here (the touch layer only consumes rects).

## 4. Symbols ported (source → target)

| Symbol | Source (base) | Target |
|---|---|---|
| `Camera` struct + fields | `wc3-game/src/camera.rs:7-18` | `wc3-input/src/lib.rs:506-518` |
| `Camera::new` | `camera.rs:21` | `lib.rs:520` |
| `Camera::eye` | `camera.rs:31` | `lib.rs:530` |
| `Camera::view_proj` | `camera.rs:39` | `lib.rs:538` |
| `Camera::viewport` | `camera.rs:50` | `lib.rs:549` |
| `Camera::world_per_pixel` | `camera.rs:55` | `lib.rs:554` |
| `Camera::pan_pixels` | `camera.rs:61` | `lib.rs:560` |
| `Camera::clamp_target` | `camera.rs:70` | `lib.rs:569` |
| `Camera::zoom_by` | `camera.rs:82` | `lib.rs:575` |
| `Camera::centre_on` | `camera.rs:86` | `lib.rs:579` |
| `Camera::world_to_screen` | `camera.rs:92` | `lib.rs:585` |
| `Camera::screen_to_world` | `camera.rs:109` | `lib.rs:602` |
| gesture state `Pointer`/`Gesture`/`PinchState` | `game.rs:63,103,110` | replaced by `TouchInput` (`lib.rs:189`) + `PointerState` (`:167`)/`PinchState` (`:178`) |
| `Game::pointer_down` routing + button band | `game.rs:855-956` (`button_at` at `:923`) | `TouchInput::pointer_down` (`:201`), `route` (`:454`), `hit_button` (`:473`) |
| `Game::pointer_move` (pan/minimap/button cancel) | `game.rs:958-1000` | `TouchInput::pointer_move` (`:235`) |
| `Game::pointer_up` (button/minimap/place/box/tap) | `game.rs:1002-1056` | `TouchInput::pointer_up` (`:283`) |
| `Game::pointer_cancel` | `game.rs:1058` | `TouchInput::cancel` (`:337`) |
| `Game::pointer_resume_pan` | `game.rs:1065` | `TouchInput::resume_pan` (`:349`) |
| `Game::pinch_begin/update/end` | `game.rs:1079/1090/1103` | `TouchInput::pinch_begin/update/end` (`:363/373/382`) |
| drag-box edge scroll (`drive_camera`) | `game.rs:661-703` | `TouchInput::edge_pan` (`:389`) |
| `minimap_to_world` | `game.rs:1754` | `wc3-input/src/lib.rs:641` |
| `world_to_minimap` | `game.rs:1747` | `wc3-input/src/lib.rs:649` |
| camera source clamps `700..3600` | `game.rs:1086` + `camera.rs:83` | `MIN_DISTANCE`/`MAX_DISTANCE` (`lib.rs:106-107`) used by both |
| double-tap window `0.36 s / 36 px` | `game.rs:1170-1174` | `DOUBLE_TAP_SECONDS`/`DOUBLE_TAP_DISTANCE` (`lib.rs:90-92`) |
| minimap order hold `0.45 s` | `game.rs:1034` | `MINIMAP_ORDER_HOLD_SECONDS` (`lib.rs:100`) |
| camera tests (6) | `camera.rs:145-225` | `lib.rs:869-950` (same assertions) |
| gesture tests (game tests keep calling the public pointer API) | `game.rs:1922-2148` | router tests `lib.rs:682-861` + game integration tests updated by patch 0001 |

## 5. Duplicates removed and delegation proof

| Duplicate | Removed from | Proof |
|---|---|---|
| Gesture recognizer/state machine | `wc3-game/src/game.rs` (`Gesture`, `Pointer`, `PinchState`, `button_at`, 6 constants) | patch 0001 deletes them; patched file `grep -cE "Gesture\|self\.pointer\|self\.pinch\|last_tap\|button_at"` → **1 hit**, a doc comment ("Gesture classification"); zero code references |
| Camera implementation | `wc3-game/src/camera.rs` | patch 0002 is a `pub use`; same type, so `wc3_game::Camera == wc3_input::Camera` (no copy, no second math) |
| Minimap inverse mapping | `wc3-game/src/game.rs` | `game.rs` keeps one-line `world_to_minimap` delegation; `minimap_to_world` deleted; input calls `wc3_input::minimap_to_world` |
| `wc3-input::VirtualJoystick` | `wc3-input` | deleted; no HUD draws a joystick (`grep -rn "joystick" crates/wc3-game/src` → only `style.rs`/`assets.rs` comments, no input use); `game.rs` had already lost the field before the port |
| `wc3-input::{GestureRecognizer,Gesture,TouchPoint}` | `wc3-input` | deleted; superseded by `TouchInput`; no external consumers (grep) |
| `wc3-input::{CommandCard,RtsController,RtsOrder,CardAction}` | `wc3-input` | deleted; the live command model is `hud::build_buttons` + `game::UiAction`; no consumers (grep) |

Parity of the router against the pre-port game semantics (unchanged effects):

| Input | Old `game.rs` | New `TouchInput` |
|---|---|---|
| tap | `!moved` + `tap_action` | `TouchEnd::Tap{at,double}` |
| double tap | `last_tap` 0.36 s / 36·scale | `tap()` same window, returned as `double` |
| quick drag | slop 16·scale, else Pan | slop 16, `DragMode::Pan` |
| held drag | ≥0.30 s before crossing slop → Box | `DragMode::Box` at 0.30 s |
| button press | exact rect, band 10·scale, min 108·scale, nearest centre | `hit_button` identical |
| button cancel | outside rect + 24·scale | identical |
| minimap | jump on down/move; order after 0.45 s | identical |
| placement | before world, after blockers; no drag | identical |
| edge scroll | only while Box; depth/30·scale, 720·scale px/s | identical |
| pinch | factor `start/now` clamped 0.5..2, distance 700..3600, centroid pan | identical |

## 6. Public API the game/base consumes (exact signatures)

```rust
// crates/wc3-input/src/lib.rs
pub struct Rect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }
impl Rect { pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self;
            pub fn contains(&self, point: Vec2) -> bool; pub fn centre(&self) -> Vec2;
            pub fn right(&self) -> f32; pub fn bottom(&self) -> f32;
            pub fn expanded(&self, dx: f32, dy: f32) -> Self; }

pub struct TouchButton { pub index: usize, pub rect: Rect, pub enabled: bool }
pub struct TouchLayout<'a> { pub minimap: Option<Rect>, pub blocked: &'a [Rect],
                             pub buttons: &'a [TouchButton], pub placing: bool, pub scale: f32 }
pub fn hit_button(point: Vec2, layout: &TouchLayout<'_>) -> Option<usize>;
pub fn minimap_to_world(point: Vec2, rect: Rect, world: Vec2) -> Vec2;
pub fn world_to_minimap(world: Vec2, rect: Rect, size: Vec2) -> Vec2;

pub enum TouchStart { Minimap, Button(usize), Blocked, Place, World }
pub enum TouchMove { Pan(Vec2), Minimap(Vec2) }
pub enum TouchEnd { Tap { at: Vec2, double: bool }, Button(usize),
                    Minimap { at: Vec2, hold: bool }, Box { start: Vec2, end: Vec2 },
                    Place { at: Vec2 } }
pub struct PinchUpdate { pub distance: f32, pub pan: Vec2 }

pub struct TouchInput { /* private */ }
impl TouchInput {
    pub fn new() -> Self;
    pub fn pointer_down(&mut self, point: Vec2, time: f32, layout: &TouchLayout<'_>) -> TouchStart;
    pub fn pointer_move(&mut self, point: Vec2, time: f32, layout: &TouchLayout<'_>) -> Option<TouchMove>;
    pub fn pointer_up(&mut self, point: Vec2, time: f32, layout: &TouchLayout<'_>) -> Option<TouchEnd>;
    pub fn cancel(&mut self);
    pub fn reset(&mut self);
    pub fn resume_pan(&mut self, point: Vec2, time: f32);
    pub fn pinch_begin(&mut self, distance: f32, centre: Vec2, camera_distance: f32);
    pub fn pinch_update(&mut self, distance: f32, centre: Vec2) -> Option<PinchUpdate>;
    pub fn pinch_end(&mut self);
    pub fn edge_pan(&self, dt: f32, bounds: Rect, scale: f32) -> Vec2;
    pub fn box_rect(&self) -> Option<(Vec2, Vec2)>;
    pub fn pressed_button(&self) -> Option<usize>;
    pub fn is_active(&self) -> bool;
    pub fn is_pinching(&self) -> bool;
}

pub struct Camera { pub target: Vec2, pub distance: f32, pub viewport: Vec2,
                    pub world: Vec2, pub pan_residual: Vec2 }
impl Camera {
    pub fn new(world: Vec2) -> Self;
    pub fn eye(&self) -> Vec3;
    pub fn view_proj(&self) -> Mat4;
    pub fn viewport(&mut self, width: f32, height: f32);
    pub fn world_per_pixel(&self) -> f32;
    pub fn pan_pixels(&mut self, delta: Vec2);
    pub fn clamp_target(&mut self);
    pub fn zoom_by(&mut self, factor: f32);
    pub fn centre_on(&mut self, world: Vec2);
    pub fn world_to_screen(&self, point: Vec3) -> Option<Vec2>;
    pub fn screen_to_world(&self, screen: Vec2, height_at: impl Fn(f32, f32) -> f32) -> Option<Vec2>;
}
```

`Game`'s public pointer API is unchanged (`pointer_down/move/up/cancel`,
`pinch_begin/update/end`, `pointer_resume_pan`), so `apps/android-native` needs no
change; its `Fingers` struct is platform finger-id plumbing (distance + centroid)
that only calls `pinch_*` — it contains no gesture semantics.

## 7. Tests (written; run after freeze — not run now)

New `wc3-input` unit tests (fixtures are pure geometry, no assets/GPU):

| Test | Fixture |
|---|---|
| `tap_without_motion_is_a_single_tap` | 640×360 world point, blank layout |
| `second_tap_within_the_window_is_a_double_tap` | two taps 0.2 s apart |
| `quick_drag_pans_and_never_selects` | 40 px drag before 0.30 s |
| `held_drag_becomes_a_box_and_edge_scrolls` | hold 0.4 s then drag to x=1276 in a 1280×720 safe rect |
| `minimap_tap_jumps_and_hold_orders` | minimap rect (20,100,200,140), tap/hold 0.6 s |
| `command_card_press_activates_only_inside_the_target` | 3 buttons 80×80; enabled/disabled/cancel |
| `button_band_claims_a_finger_just_outside_the_rect` | point 4 px above the rect |
| `hud_blockers_swallow_the_touch` | two blocked rects |
| `placement_wins_over_the_world_and_requires_no_drag` | `placing = true` |
| `pinch_zooms_and_two_finger_pan_moves_the_camera` | 200→400 px spread, centroid +60, extreme distances |
| `second_finger_lift_resumes_panning` | `resume_pan` then 60 px drag |
| `reset_forgets_the_double_tap_history` | tap, reset, tap |
| `world_per_pixel_scales_with_zoom_distance` | camera 4096², viewport 1280×720 |
| `pan_moves_target_opposite_the_drag_and_keeps_subpixels` | 1 px × 100 drags |
| `pan_is_clamped_to_the_world_with_a_margin` | 10k drags |
| `screen_centre_round_trips_to_the_camera_target` | flat height |
| `screen_to_world_accounts_for_terrain_height` | height 0 vs 200 |
| `centre_on_clamps_into_the_world_margin` | ±50k target |
| `world_to_minimap_round_trips` | (1024,3072) in 4096² |
| `console_layout_blocks_world_clicks_and_placement` (game integration) | 2414×1080 + insets 48/80/48/0, console dead space: tap/drag/placement must not click through; world tap above builds |

After freeze, run: `cargo test -p wc3-input` (19 tests), `cargo test -p wc3-game
--lib` (the patch keeps the existing 17 game tests, updated to the delegated
internals), then `cargo check -p wc3-game --lib`, then the APK build gate. Device
smoke stays `analysis/agent-touch/device-smoke.sh` (already written; nothing to
change because the platform API is unchanged).

## 8. Conflicts, unknowns, exact next step

1. **`crates/wc3-game/src/game.rs` is HUD-owned.** The input patch was applied
   in the execution window (2026-11-13) per BASE §12; merged sha
   `0dded63c…` (current tree; the touch merge landed as `c388d245…`). If the HUD
   rewrite lands again:
   `python3 analysis/agent-touch/tools/make_game_patch.py` regenerates the patch
   against the new file (it fails loudly on a missing anchor), then re-apply.
   HUD review requested for the non-input lines: `refresh_buttons()` call sites,
   the `pressed` line in `build_hud`, and the dead-code removals.
2. **`crates/wc3-game/src/camera.rs`** is now the 9-line `pub use
   wc3_input::Camera` facade (merged). The canonical implementation and its
   tests live in `wc3-input`.
3. **`WC3RE_MERGE_MATRIX.md` now exists** (45.6 KB, 2026-11-13). It contains no
   input/gesture decision beyond the BASE §2 touch row; the input layer has no
   RE/parser dependency.
4. **BASE.md v1 has no input row.** §1 lists `wc3-input -> wc3-core, glam` and
   `wc3-game -> … wc3-input`; BASE v0/v1 §2 now carries the proposed touch row
   (§1 of this manifest).
5. **No joystick.** The type is deleted from `wc3-input`; `game.rs` and
   `mobile_hud.rs` contain no joystick field/drawing. Do not re-add a second
   camera control.
6. **HUD hit-rect source.** `game.rs` feeds the router from `HudLayout` rects
   synced from `MobileHudLayout` in `rebuild_layout` + `refresh_buttons`. If the
   HUD moves to `ui.rs` hit-testing, only `blocked_rects()`/`touch_rect()` change;
   the input API stays.
7. **Execution window.** Compile + per-layer tests were run; no APK install,
   screenshot or device automation. The device watchdog SIGTERMs long cargo
   runs, so per-layer tests were executed detached (`setsid nohup`) and completed
   rc 0.

Exact next step (dispatcher): full-suite gate in BASE.md §7 (`scripts/test-all.sh`
+ `cargo check -p wc3-game` + APK build). Known unrelated failures in the full
lib suite: `assets::geoaset_tests::geoaset_skinning_weights_are_normalised`
(world/assets owner) and four `terrain_render::tests::agent_world_*` (world/render
owner).

## 9. State

**DONE (scope).** The input layer is merged into the game path, compiles, and its
per-layer tests pass. Full-suite/device gates remain with the dispatcher.

- `cargo check -p wc3-input` → rc 0
- `cargo check -p wc3-input -p wc3-game --lib` → rc 0
- `cargo check -p herofolends-android` → rc 0
- `cargo test -p wc3-input` → rc 0, **19/19**
- `cargo test -p wc3-game --lib game::tests:: -- --test-threads=1` → rc 0, **17/17**

Console-gesture verification 2026-11-13 03:22 (after the bottom-console HUD
merge): `cargo test -p wc3-game --lib game::tests::` → rc 0, **18/18**, including
`tap_selects_moves_and_places_a_building` (select → move → arm BUILD → place) and
`console_layout_blocks_world_clicks_and_placement` (console tap/drag/placement
clicks are swallowed; the same gestures above the console select and build).

Round 2 (`r2-touch`, ledger 356) 2026-11-13 07:51: `cargo test -p wc3-game --lib
game::tests:: -- --test-threads=1` → rc 0, **25 passed / 1 ignored** (suite grew
from other layers; every touch test green), `cargo test -p wc3-input` → rc 0,
19/19. Physical box-select path + edge pan documented with `file:line` in
`analysis/agent-touch/BOX_SELECT_EDGE_PAN.md` (SDL finger → `Game::pointer_*` →
`TouchInput` route/box/`edge_pan` → `Camera::pan_pixels` → `Game::box_select`;
console interactions and the verification matrix included).

Re-verified 2026-11-13 02:52–02:57 after other-layer churn: `cargo check -p
wc3-input -p wc3-game --lib` rc 0, input 19/19, game 17/17. A transient blocker
appeared while the RE owner was editing `crates/wc3-re/src/objectdata.rs`
(`E0308` ObjectKind mismatch at `objectdata.rs:438/443`); it cleared with the RE
edit and the retry was green. My files did not change during the churn
(`game.rs` now `0dded63c…` after other-owner edits; `camera.rs e166ef3e…`,
`wc3-input ccc89af2…`).

## 11. Parity touch + P0 unblock (2026-11-13, ledger parity-touch 359)

Parity rows (control groups, hotkeys, long-press tooltips, box select, edge pan,
all touch targets) with `file:line` and rc live in
`analysis/agent-touch/PARITY_TOUCH.md`. Summary:

- Control groups 1..9 share one kernel: `wc3-input::ControlGroups` (assign/recall/
  cycle); keyboard Ctrl+digit / digit via `Game::hotkey`/`key_char`; touch via the
  ARMY chip (hold = save to next free group, double = recall/cycle, tap = army).
- Hotkeys: `Hotkey::parse` + `Game::key_char` resolve letters against the visible
  card; Escape cancels placement/targeting.
- Tooltips: `TouchInput::long_press` → `Game::update_tooltip` → HUD tooltip;
  command cells keep their press tooltip and the ARMY cell advertises its gestures.
- Box select + edge pan unchanged and re-verified.
- Touch targets: exact rect + `108·scale` minimum band, nearest centre.

P0 compile fix (only real error, HUD menu, not input): `MenuView::for_screen`
requires `&'static str` (`mobile_hud.rs:644`) while `menu_view` passed a borrow
(E0521 `game.rs:1085`). Fixed by leaking the map title once in `Game::new`
(`menu_map_name`, `game.rs:222/177/264/1094`); HUD layout/card code untouched.

rc after the fix: `cargo check -p wc3-input` 0, `cargo check -p wc3-game --lib`
0, `cargo test -p wc3-input` 22/22 0, `cargo test -p wc3-game --lib` 87 passed /
15 ignored 0 (full suite), `game::tests` 28 passed / 1 ignored 0 (10:37, with
`Game::key_char`).

## 11b. Round 4 — on-screen control-group row + box marquee (2026-11-13)

- On-screen control-group row: nine slots in the console dead space
  (`game.rs:1050 group_row_slots`, `:1065 draw_group_row`), touch targets
  100..108 (`game.rs:28`), tap = recall, double = recall + centre, hold = save to
  that slot (`game.rs:1104 hit_group_slot`), per-slot tooltip. This closes the
  PARITY_MATRIX P12 gap "no on-screen group row".
- Box-select physical path: the selection marquee is drawn while dragging
  (`game.rs:1117 box_marquee_rect`, `:1129 draw_box_marquee`), so the real finger
  path is visible before release (QA harness boundary unchanged).
- rc: `cargo check -p wc3-input -p wc3-game --lib` 0; `cargo test -p wc3-input`
  22/22 0; `cargo test -p wc3-game --lib game::tests::` 31 passed / 1 ignored 0.
- Artifact: `analysis/agent-touch/PARITY_TOUCH.md` §1b.

## 10. Execution record (2026-11-13, BASE v0 §12)

Applied in the execution window (write scope: `crates/wc3-input/**`,
`crates/wc3-game/src/camera.rs`, input functions in `game.rs`):

| File | sha256 | Change |
|---|---|---|
| `crates/wc3-input/src/lib.rs` | `ccc89af2…` | single `TouchInput` router + `TouchLayout`/`hit_button` + canonical `Camera` + minimap mapping; 19 tests; joystick/recognizer/command-model duplicates deleted |
| `crates/wc3-input/Cargo.toml` | `deaf8e9e…` | description only |
| `crates/wc3-game/src/camera.rs` | `e166ef3e…` | 9 lines: `pub use wc3_input::Camera` (type identity) |
| `crates/wc3-game/src/game.rs` | `c388d245…` → current `0dded63c…` | input functions delegate to `wc3_input`; `touch`/`touch_buttons` fields; `refresh_buttons`/`blocked_rects`; `pressed` reads `touch.pressed_button()`; dead `UiState` import, `MIN_SAFE_VIEWPORT`, `world_to_minimap` facade removed; routing test uses the merged HUD's console dead space |
| `analysis/tech-lead/merge-touch.md` | this file | manifest |
| `analysis/agent-touch/{INTEGRATION.md,patches/*,tools/*}` | — | notes, applied patches, regenerator |

Coordinate note for HUD (game.rs owner): the patch was applied per the
execution steer. The only non-input lines touched are `refresh_buttons()`
(build + reflow + snapshot, which also fixes the stale mobile card rects), the
one `pressed` line in `build_hud`, and the three dead-code removals. If the HUD
rewrite lands again, rerun `analysis/agent-touch/tools/make_game_patch.py` to
rebase; the tool asserts every anchor.

Remaining, out of this scope (observed, other layers):
`assets::geoaset_tests::geoaset_skinning_weights_are_normalised` and four
`terrain_render::tests::agent_world_*` fail in the full lib suite. The full-suite
run is also cut short by the device's cargo watchdog (SIGTERM during the wc3-re
compile/link), so per-layer tests were executed detached with `setsid nohup`;
they completed rc 0.

