# DONOR INTAKE — group `android/input` (H2 ledger 385)

Date: 2026-11-14 (MSK). Workspace: `~/src/herofolends-rs`.
Scope: `android-activity`, `sdl3-sys-rs`, `SDL`, `parry`, `libadrenotools`,
`libxemit`, `XenosRecomp` → app/input layer, collision geometry, driver/shader
constraints. OWNER: `apps/android-native/**`, `crates/wc3-input/**`,
`analysis/agent-touch/**`. READ-ONLY: `crates/wc3-game/src/game.rs`.

## Provenance (all evidence is local byte-level, no web claims)

| donor | pinned commit | version | license | files | access |
|---|---|---|---|---|---|
| `donors/android-activity` | `b4ddf059b77be12cdb955d394e65a92e7568d936` | 0.6.1 | MIT OR Apache-2.0 | 158 | 2026-11-14, local clone |
| `donors/sdl3-sys-rs` | `a04bef22c5c81cc130bbcc1d019b9e1ed0318bf6` | 0.0.1+sdl3-dev-2023-01-23 | Zlib | 1522 | 2026-11-14, local clone |
| `donors/SDL` | `4ca1b1c2d084b59ca72c87f36a18fc1a3898f424` | 3.5.0 | Zlib | 2415 | 2026-11-14, local clone |
| `donors/parry` | `b3c3584ce9be9bf6edb821fd7548b817087131ff` | 0.31.1 | Apache-2.0 | 530 | 2026-11-14, local clone |
| `donors/libadrenotools` | `8fae8ce254dfc1344527e05301e43f37dea2df80` | — | BSD-2-Clause | 109 | 2026-11-14, local clone |
| `donors/libxemit` | `3660b70443f4f5c94abccd1810d94127d42bbd20` | — | **unknown (no license file)** | 51 | 2026-11-14, local clone |
| `donors/XenosRecomp` | `990d03b28a27b50277ee5d8d942e1c5f873869d1` | — | MIT | 47 | 2026-11-14, local clone |

**Runtime pin (observed):** the shipped `third_party/sdl3-android-build/libSDL3.so`
contains `SDL-3.5.0-4ca1b1c`, i.e. exactly the donor SDL commit above. The
constants in `crates/wc3-input/src/sdl.rs` are therefore pinned against the
runtime that actually ships, not against a header guess. `sdl3-sys-rs` is a
**2023 dev snapshot** (it still spells `SDL_APP_DIDENTERFOREGROUND`,
`SDL_DISPLAYEVENT_*`), so it is reference-only and its numbers are not copied.

Stars: not fetched (offline pass); tier is T1 by file:line, not by metadata.

## ABI layout evidence (the app reads raw `SDL_Event` bytes)

`apps/android-native/src/lib.rs` decodes `SDL_Event` by offset because it links
SDL3 through hand-written FFI. The offsets were re-verified against the shipped
3.5.0 headers:

| field | offset | donor |
|---|---|---|
| `SDL_Event` size / alignment | 128 bytes, `Uint8 padding[128]` | `SDL_events.h:1162` |
| `SDL_TouchFingerEvent.touchID` / `.fingerID` | 16 / 24 | `SDL_events.h:866-867` |
| `.x` / `.y` | 32 / 36 | `SDL_events.h:868-869` |
| `SDL_KeyboardEvent.scancode` | 24 | `SDL_events.h:409` |
| `.mod` | 32 (Uint16) | `SDL_events.h:411` |
| `.repeat` | 37 | `SDL_events.h:414` |
| `SDL_MouseMotionEvent.which` / `.x` / `.y` | 20 / 28 / 32 | `SDL_events.h:446-449` |
| `SDL_MouseButtonEvent.down` / `.x` / `.y` | 25 / 28 / 32 | `SDL_events.h:470-473` |

`SDL_AddEventWatch`, `SDL_RemoveEventWatch`, `SDL_Delay`,
`SDL_GetWindowSafeArea` are exported by the shipped `libSDL3.so`
(`nm -D third_party/sdl3-android-build/libSDL3.so`).

## Matrix — mechanism → our layer → verdict

Tiers: **T1** = file:line read in the donor; ADOPT/ADAPT rows are T1.

| # | donor mechanism (T1 anchor) | our layer (anchor) | verdict | what landed |
|---|---|---|---|---|
| 1 | `android-activity` lifecycle event set `MainEvent`: `Start/Resume/SaveState/Pause/Stop/Destroy`, `GainedFocus/LostFocus`, `ConfigChanged/InsetsChanged/LowMemory` (`src/lib.rs:456-543`) | `apps/android-native`: previously polled only quit/resize/safe-area; no app lifecycle, focus, rotation or low-memory handling | **ADAPT** | `wc3-input/src/lifecycle.rs` state machine (`Phase`, `Event`, `Effects`) + SDL event-watch adapter in `apps/android-native/src/lib.rs` |
| 2 | `android-activity` state save/restore: `StateSaver` during `SaveState`, `StateLoader` during `Resume` (`src/lib.rs:522-528`, `native_activity/mod.rs:36-60`) | SDL exposes no app instance-state API; `SDLActivity` keeps the native thread across config changes and **terminates** on recreation by default (`SDL_HINT_ANDROID_ALLOW_RECREATE_ACTIVITY=0`, `SDL_hints.h:80-92`) | **ADAPT** | native equivalent: `Sim::save_to_path` on `WILL_ENTER_BACKGROUND`, restore on launch when the map grid matches, delete on clean quit (`checkpoint_path/save_checkpoint/restore_checkpoint/clear_checkpoint` in the app); opaque Android bundle rejected because SDL exposes no loader |
| 3 | `android-activity` focus/inset/config events (`src/lib.rs:505-543`) | SDL sends `WINDOW_FOCUS_LOST/GAINED` (0x20F/0x20E), `DISPLAY_ORIENTATION` (0x151), `WINDOW_DISPLAY_CHANGED/SCALE` (0x213/0x214), `WINDOW_SAFE_AREA_CHANGED` (0x215) | **ADAPT** | `Lifecycle::event_from_sdl` maps all of them; focus loss resets gestures, rotation rebuilds the surface and reapplies insets |
| 4 | `android-activity` `MotionAction::Cancel` (`src/input.rs:218-243`, `Cancel` at 227) and the input queue (`src/lib.rs:465,1172`) | SDL Android maps `ACTION_CANCEL` → `SDL_EVENT_FINGER_CANCELED` (`SDL_androidtouch.c:102-103`); the app treated it as `FINGER_UP` and could emit a tap/box/order on cancel | **ADOPT (bug fix)** | `TouchRouter::finger_cancel` discards the gesture; trace `multitouch-cancel.trace` pins “no tap on cancel” |
| 5 | SDL application lifecycle delivery: `SDL_SendAppEvent` calls event **watchers only**, never the queue (`src/events/SDL_events.c:2031-2055`) | app's `SDL_PollEvent` loop could never see `WILL/DID_*`, `LOW_MEMORY`, `TERMINATING` | **ADOPT** | `SDL_AddEventWatch(on_sdl_event)` installed before the window; ordered queue drained per frame; removed on exit |
| 6 | SDL finger-ID semantics: Android `pointer_finger_id + 1`, `touch_device_id + 1`, emulator −1/−2 remap (`SDL_androidtouch.c:44-79`) | `Fingers` tracked exactly two ids, ignored a third finger, and mis-promoted after removals | **ADAPT** | `wc3-input/src/session.rs`: `FingerTable` (10 ids, stable, compaction) + `TouchRouter` promotion; third-finger trace |
| 7 | SDL 3.5.0 event enum (`include/SDL3/SDL_events.h:90-231`) | hard-coded numeric constants in the app (correct for the old subset, missing lifecycle/KEY_UP) | **ADOPT** | `wc3-input/src/sdl.rs` single source, host tests pin every value against the donor header |
| 8 | SDL scancode/keycode tables (`SDL_scancode.h:63-105`, `SDL_keycode.h:329-345`) | `scancode_char` + `Hotkey` in `wc3-input` | **ADOPT** | table moved to `sdl::scancode_char`; `sdl::keymod` masks pinned; `scancode_hotkey` added |
| 9 | SDL key repeat: `event.key.repeat` set only when the key was already down (`src/events/SDL_keyboard.c:506-528,620`) | app skipped repeats on key-down but never read key-up, so a released Shift stayed additive | **ADAPT** | `KeyState` handles down/up, swallows repeats, emits `set_additive` on change; `key-repeat.trace` |
| 10 | SDL safe area: `SDL_GetWindowSafeArea` fed from `onApplyWindowInsets` (app code already uses it) | `window_insets()` + `Game::set_safe_insets` | **ADOPT (verified)** | unchanged behaviour; now also re-applied on lifecycle resume/rotation |
| 11 | SDL Android low-latency audio hint (`SDL_HINT_ANDROID_LOW_LATENCY_AUDIO`, default 1, `SDL_hints.h:103-118`) | `wc3-audio` opens the device through `SDL_OpenAudioDeviceStream` (`crates/wc3-audio/src/engine/sdl3.rs`) | **ADOPT** | hint set explicitly before `SDL_Init` and logged (`GAME_LIFECYCLE ... low_latency_audio=1`) |
| 12 | SDL gamepad events (`SDL_events.h:213-215`: 0x650/0x651/0x652) | no gamepad target API: `Game` exposes touch/mouse/keyboard only | **REFERENCE** | values documented in `sdl.rs` (pinned by tests); no fake port without a game-layer action |
| 13 | SDL pinch events 0x710-0x712 (`SDL_events.h:232-236`, Java `ScaleGestureDetector`) | our pinch is synthesized from the two finger stream, device-tested | **REFERENCE** | not adopted: handling both streams would double-apply zoom; needs device verification before replacing |
| 14 | `android-activity` GameActivity JNI backend (`src/game_activity/mod.rs`) | app is SDLActivity (`SDL_main`), SDL owns the Java side | **REFERENCE** | no port: duplicating SDLActivity would add a second ownership model |
| 15 | parry ball/ball distance (`src/query/distance/distance_ball_ball.rs`) | no exact unit-pair query; movement used cell sampling | **ADOPT** | `geometry::distance_ball_ball` |
| 16 | parry ball/ball contact incl. coincident-centre `+X` fallback (`src/query/contact/contact_ball_ball.rs`) | ad-hoc separation math inside `advance_movement` | **ADOPT** | `geometry::contact_ball_ball` (+ unit parity test) |
| 17 | parry point/AABB projection and ball/polyhedron distance (`src/query/point/point_aabb.rs`, `src/query/distance/distance_ball_convex_polyhedron.rs`) | `can_stand` sampled five points at 0.7 r | **ADOPT** | `geometry::distance_point_aabb`, `distance_ball_aabb`, `ball_aabb_intersects`, `ball_standable` (exact cell coverage) |
| 18 | parry ray/AABB slab clip (`src/query/ray/ray_aabb.rs`) | `clear_world_line` sampled every 24 px with five offsets | **ADOPT** | `geometry::ray_aabb_toi`, `segment_aabb_intersects`, `capsule_path_clear` |
| 19 | parry swept ball/ball TOI (`src/query/shape_cast/shape_cast_ball_ball.rs` + `src/query/ray/ray_ball.rs`) | no sweep: fast units could tunnel through a thin gap | **ADOPT** | `geometry::sweep_ball_ball` (quadratic closed form, analytic test) |
| 20 | parry GJK/EPA/SAT manifolds, compound shapes, BVH, heightfield (`src/query/{gjk,epa,sat,contact_manifolds}`, `src/partitioning`) | units are discs, buildings AABBs, grid is the broadphase | **REJECT** | the 5 focused formulas cover the game; pulling GJK/EPA + nalgebra-sized traits would add weight with no query |
| 21 | `libadrenotools` rootless driver replacement/hook (`README.md`, `src/driver.cpp`, `src/hook/*`) | shipping APK uses the system Adreno driver + wgpu/WGSL; optional turnip ICD is a manual device experiment (`configure_turnip`); a Rust port already exists for probes (`crates/wc3-adrenotools`) | **REJECT for shipping** | no custom driver is bundled or loaded by the app; keep the probe crate outside the game link |
| 22 | `libxemit` Xbox 360 shader emitter (`xemit.h`, `xemitops.c`, `xenosc.exe`) | no Xbox 360 shader path; WC3 MDX is rendered by WGSL pipelines | **REJECT** | wrong GPU/ISA generation; Windows `.exe`, Xbox `xetypes.h`, no license file |
| 23 | `XenosRecomp` Xbox 360 SHM3 → HLSL/DXIL/SPIR-V (`README.md`, `XenosRecomp/shader_recompiler.cpp`) | WGSL pipelines; no SHM3 container in WC3 assets | **REJECT** | donor README itself says “do not expect the recompiler to work out of the box” and it targets Unleashed Recompiled |
| 24 | `sdl3-sys-rs` bindings/header snapshot | app FFI declarations | **REFERENCE** | stale 2023 SDL3-dev names; constants taken from `donors/SDL` 3.5.0 instead |

## Ported code (file:line)

| file | content | donor |
|---|---|---|
| `crates/wc3-input/src/sdl.rs` | event/keymod/scancode/hint constants + pinning tests | `donors/SDL/include/SDL3/{SDL_events.h,SDL_keycode.h,SDL_scancode.h,SDL_hints.h}` |
| `crates/wc3-input/src/lifecycle.rs` | `Lifecycle`, `Phase`, `Event`, `Effects`, SDL mapping | `android-activity/src/lib.rs:456-543`; `SDL_events.c:2031-2055` |
| `crates/wc3-input/src/session.rs` | `FingerTable`, `TouchRouter`, `KeyState`, `InputState`, `InputSink`, `RecordingSink` | `SDL_androidtouch.c:44-79,102-103`; `SDL_keyboard.c:506-528`; `android-activity/src/input.rs:218-243` |
| `crates/wc3-input/src/geometry.rs` | ball-ball distance/contact, ball-AABB, ray-AABB, swept ball, exact grid standability/LOS | `donors/parry/src/query/...` (table at the module top) |
| `crates/wc3-input/src/trace.rs` | new commands `touch`, `mouse`, `sdlkey`, `focus`, `rotate` driven through `InputState` | — |
| `apps/android-native/src/lib.rs` | SDL event-watch queue, `InputState` integration, lifecycle effects (pause/resume/rebuild/insets/save), suspended render skip, checkpoint restore/clear | rows 1-11 |
| `analysis/agent-touch/traces/*.trace` | `multitouch-cancel`, `multitouch-third-finger`, `rotation-focus`, `two-finger-tap`, `key-repeat` | — |
| `analysis/agent-touch/patches/0008-sim-exact-collision.patch` | sim `can_stand`/`clear_world_line` delegate to `wc3_input::geometry` (ready for the sim owner; `wc3-sim` is outside this task's OWNER) | rows 17-18 |
| `analysis/agent-touch/patches/0009-game-pointer-cancel-reset.patch` | `Game::pointer_cancel` uses `TouchInput::reset` so pre-pause double-tap history cannot pair with a post-resume tap (ready for the game owner; `game.rs` is READ-ONLY here) | session reset finding |

Key symbols (current line anchors):

- `crates/wc3-input/src/sdl.rs:14` — pinned event/keymod/scancode/hint modules.
- `crates/wc3-input/src/lifecycle.rs:106` — `Lifecycle`; `handle` at 129, `event_from_sdl` at 219.
- `crates/wc3-input/src/session.rs:187` — `TouchRouter`; `finger_cancel` at 286; `KeyState` at 323; `InputState` at 421.
- `crates/wc3-input/src/geometry.rs:140` — `contact_ball_ball`; `ray_aabb_toi` 191; `sweep_ball_ball` 237; `ball_standable` 300; `capsule_path_clear` 330.
- `apps/android-native/src/lib.rs:143` — `on_sdl_event`; `save_checkpoint` 678; `restore_checkpoint` 695; `GameSink` 749; watch install 939; `InputState::apply` call 1240; suspended branch 1276; watch removal 1454.

## Rejections — full reasons

- **libadrenotools (shipping):** the library exists to load a *custom* Vulkan
  driver inside an app by bypassing the Android linker namespace
  (`lib/linkernsbypass/android_linker_ns.cpp`) and interposing
  `android_dlopen_ext`/`fopen`/`gsl_memory_alloc_pure` (`src/hook/hook_impl.cpp`).
  The shipped game is built on the system Adreno driver and wgpu/WGSL, exactly
  the configuration the device evidence was captured on; bundling the hooks
  would change driver ownership and break the verified rendering path
  (`apps/android-native/src/lib.rs` `renderer.set_skin_mode(Cpu)` comment).
  `crates/wc3-adrenotools` already carries the clean-room Rust port for
  device/driver probes, so nothing is lost. A turnip ICD remains an explicit
  manual experiment (`configure_turnip` only activates when the operator pushes
  the ICD into the app's files directory), not a runtime dependency.
- **libxemit:** it is an Xbox 360 shader *emitter/assembler* (`xemit.h` includes
  `<xenos/xe.h>`, `xenos/xe.h` is the Xenon GPU API) with a Windows
  `xenosc.exe` and generated operation table (`xemitops.c`, 348 KB). WC3 assets
  contain MDX models, not Xbox 360 shader containers, and the renderer is WGSL.
  There is no license file in the donor, which independently blocks bundling.
- **XenosRecomp:** converts Xbox 360 SHM3 shader binaries to HLSL and then
  DXIL/SPIR-V via DXC. WC3 MDX has no SHM3 binary; our pipelines are WGSL, and
  the donor README states the recompiler is tuned to Unleashed Recompiled and
  is not expected to work out of the box. Adopting it would add an HLSL/DXC
  toolchain and a second shader language with no input format to feed it.

## Tests and parity evidence

- `cargo test -p wc3-input` — includes `geometry::tests` (ball/ball closed form,
  contact normals/witnesses, sweep TOI, ray/AABB, exact-vs-sampled parity),
  `lifecycle::tests` (ordering/idempotency/save-once), `session::tests`
  (cancel, third finger, key repeat, mouse latch) and the trace fixtures
  (`tests/ui_trace.rs`, 15 fixtures including the 5 new ones).
- **Collision parity test:** `geometry::tests::exact_standability_is_never_looser_than_sampling`
  scans a 12×12 random grid over positions/radii and asserts the exact test
  never accepts a pose the old five-point sample rejects, and that the sampled
  test has corner false negatives; `exact_standability_catches_the_corner_the_sample_misses`
  and `exact_line_of_sight_catches_a_thin_blocked_cell` pin concrete failures of
  the old sampling at exact coordinates.
- **Input traces:** `multitouch-cancel.trace` (cancel is never a tap),
  `multitouch-third-finger.trace` (tracking/promotion), `rotation-focus.trace`
  (reset + Shift release/re-hold), `two-finger-tap.trace` (right-click
  centroid), `key-repeat.trace` (repeat swallowed, Shift up clears).

## Gates (final rc)

1. `cargo test -p wc3-input` — **rc=0**, `89 passed (10 suites, 0.04s)`
   (was 52 before this intake; +37 unit/trace tests). Final-source runs:
   shared target `89 passed`; isolated `CARGO_TARGET_DIR` rerun after the last
   edit `89 passed`, `GATE1_ISOLATED_RC=0`.
2. `cargo check -p herofolends-android` — **rc=0** on the final source in both
   profiles:
   - `cargo check --release -p herofolends-android` → `GATE2R_RC=0`,
     `Finished release profile in 2m 56s`;
   - `CARGO_TARGET_DIR=/data/data/com.termux/files/usr/tmp/h2-385-appcheck CARGO_PROFILE_DEV_DEBUG=0 cargo check -p herofolends-android --offline -j 2`
     → `GATE2_ISOLATED_RC=0`, `Finished dev profile in 3m 45s`.
   The shared `target/` run was repeatedly starved by peer donor-intake builds
   holding `.cargo-lock`, and one queued attempt died on `failed to write
   .../target/debug/.fingerprint/...` when the userdata filesystem hit 100 %;
   the isolated/release runs are the clean evidence.
3. New traces — all five are in `tests/ui_trace.rs` and pass inside gate 1
   (`multitouch-cancel`, `multitouch-third-finger`, `rotation-focus`,
   `two-finger-tap`, `key-repeat`).

## Findings / hand-offs (no silent tails)

1. `crates/wc3-game/src/game.rs:2441-2443` — `Game::pinch_end` discards the
   `Option<Vec2>` two-finger-tap centroid from `TouchInput::pinch_end`, so the
   touch right-click cannot reach the game layer. The router now reports it and
   the app logs `GAME_TWO_FINGER_TAP`; wiring it to an actual right-click needs
   a game-layer action (read-only here). Owner: `wc3-game` / tech-lead.
2. `crates/wc3-game/src/game.rs:2414-2416` — `pointer_cancel` called
   `TouchInput::cancel`, which keeps `last_tap`/`last_button`. Ready patch
   `0009-game-pointer-cancel-reset.patch` switches it to `reset()`; the app
   already calls `pointer_cancel` on focus loss/rotation.
3. `crates/wc3-sim/src/lib.rs:2148-2166,2093-2119` — the five-point/sampled
   collision helpers are replaced by exact parry-derived queries via ready
   patch `0008-sim-exact-collision.patch` (adds `wc3-input` to `wc3-sim` and
   delegates both functions). `wc3-sim` is outside this task's OWNER.
4. Concurrent-workspace defect observed at 12:17 MSK: `cargo check -p
   herofolends-android` failed in `wc3-mpq` (`crate::stream::detect_key_by_content`
   missing, type mismatches at `wc3-mpq` line 313/317) while the formats donor
   agent (ledger 381) was mid-edit. The function now exists
   (`crates/wc3-mpq/src/stream.rs:28`); gate 2 was re-queued behind the shared
   `target/` lock.
5. Environment risk observed during the closing gates: 26 concurrent `cargo`
   processes from peer donor-intake agents share one `target/` (24 GB) and the
   userdata filesystem reported **100 % used, 1.5 GB free**. Gate 2 was queued
   on the same target lock; isolated builds were kept to a 153 MB scratch target.
