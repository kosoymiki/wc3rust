# P1 — Surface::configure SIGABRT (queue-family) — root fix + device proof

Ledger 352, GPU layer, 2026-11-13. Scope: `crates/wc3-render/**`, `apps/android-native/**`.

## Symptom (observed)

Tombstones `tombstone_17` (19:44:36), `tombstone_18` (20:08:11), `tombstone_19` (20:09:46), all
`org.herofolends.p0`:

```
Abort message: 'wgpu error: Validation Error
Caused by:
  In Surface::configure
    Surface does not support the adapter's queue family'
#14 Surface::configure
#15 main::run+9048
#16 SDL_main
```

Logcat for the same sessions shows the app **did** start: `P0_GPU_ADAPTER … Adreno (TM) 730`,
`GAME_SURFACE 2414x1080 format=Rgba8UnormSrgb present=Fifo`, `GAME_READY`, `GAME_STATE … fps=61`
for ~20–60 s, then `tombstone_19` at process uptime **68 s**. So the crash is not the first
configure.

## Root cause

1. The app fetched `SDL_PROP_WINDOW_ANDROID_WINDOW_POINTER` **once** at startup and never re-read it.
   Android recreates/detaches the `ANativeWindow` on resume, orientation/fullscreen transitions; the
   wgpu `Surface` then still wraps the old handle.
2. `wgpu-hal` Vulkan checks surface support only for **hard-coded queue family 0**
   (`wgpu-hal-30.0.1/src/vulkan/swapchain/native.rs:60-86`, `let queue_family_index = 0; //TODO`).
   Against the stale window `vkGetPhysicalDeviceSurfaceSupportKHR` returns false →
   `Surface::get_capabilities` fails → `UnsupportedQueueFamily`.
3. Both in-loop `surface.configure` calls (resize at old line ~1013, swapchain Lost/Outdated at
   ~1039) were **unguarded**: wgpu's default uncaptured-error handler panics → `SIGABRT`.

## Fix (file:line)

- `crates/wc3-render/src/surface.rs` (new module):
  - `validate_caps` (52), `pick_config` (70), `configure_guarded` (94), `configure_surface` (114),
    `reconfigure` (212). Every configure runs inside `device.push_error_scope(Validation)` and its
    result is returned as `Err(String)`; adapters are filtered by `is_surface_supported` + non-empty
    caps, with fallback to the next surface-compatible adapter. No path can reach the default panic.
- `crates/wc3-render/src/lib.rs:48,66`: module + re-exports.
- `apps/android-native/src/lib.rs`:
  - `current_native_window` (679): re-reads the current `ANativeWindow` pointer (pumps SDL events).
  - `create_android_surface` (701): builds a fresh wgpu surface from the current pointer.
  - `recover_surface` (726): re-read pointer → guarded reconfigure (fast path) → on failure recreate
    the surface from the current window and guarded-configure it.
  - initial setup (842): `configure_surface` + post-configure pointer re-check.
  - resize (1063) and Lost/Outdated (1103) branches now go through `recover_surface`; a failure is
    logged (`P0_SURFACE_FAILED reason=… err=…`) and returned as `Err` — clean exit, no `SIGABRT`.
- `apps/android-native/Cargo.toml:19`: direct `wc3-render` dependency.

## Guard tests (rc)

| command | rc | result |
|---|---|---|
| `cargo check -p wc3-render --all-targets` | 0 | compiles |
| `cargo test -p wc3-render --lib` | 0 | **47 passed** (3 new: format/present/caps guards) |
| `cargo test -p wc3-render --test surface_guard` | 0 | **3 passed** — `validation_error_is_captured_not_fatal` proves a validation error raised in the scope is returned, not aborted; `empty_caps_are_rejected_without_touching_the_device`; `guarded_configure_signature_is_fallible` (compile-time regression guard) |
| `cargo test -p wc3-render --test {fx,parity,cache,astc}` | 0 | 10 / 3 / 3 / 4 passed (no regression) |
| `cargo check -p wc3-game --lib` | 0 | downstream green |
| `cargo build --release -p herofolends-android` | 0 | `target/release/libmain.so` |
| `JAVA_TOOL_OPTIONS=-Xmx768m bash scripts/build-apk.sh` | 0 | APK 5.9 MB, v2+v3 verified, sha256 `e45bb941…` |

## Device verification (observed)

Installed the rebuilt APK (`pm install -r` → `Success`), launched twice, backgrounded/resumed once.

```
P0_SURFACE candidate adapter Adreno (TM) 730
P0_SURFACE configured adapter Adreno (TM) 730 format=Rgba8UnormSrgb present=Fifo
P0_GPU_ADAPTER name="Adreno (TM) 730" backend=Vulkan driver="Qualcomm Technologies Inc. Adreno Vulkan Driver"
GAME_SURFACE 2414x1080 format=Rgba8UnormSrgb present=Fifo
GAME_READY source=user-mpq …
GAME_FIRST_FRAME surface=2414x1080 fullscreen=true … units=16 phase=Playing
P0_SURFACE_RECOVERED reason=lost 2414x1080 ptr=0xb4000072c077bc10      ← startup (old code aborted here)
P0_SURFACE_RECOVERED reason=lost 2414x1080 ptr=0xb400007318726a10      ← after HOME + resume (pointer changed)
GAME_STATE … tick=1734 fps=60
```

- Process survived to **2 m 44 s** uptime (previous crash at 68 s), then was relaunched by the test
  (`am start`, new pid 26809) and rendered `fps=57–61` with focus on the activity.
- Tombstone directory: **32 files before and after**, newest still `tombstone_19` (20:09) — **no new
  tombstone from the fixed build**.
- The recovery path fired twice for the exact failing condition (Lost/stale surface) and the app kept
  rendering; no `P0_SURFACE_SETUP_FAILED` / `P0_SURFACE_FAILED` occurred.

## Result

The app can no longer `SIGABRT` in `Surface::configure`: capability guards run before configure,
every configure is error-scoped and fallible, and a stale `ANativeWindow` is followed by recreating
the wgpu surface from the current pointer. Device launch and background/resume are tombstone-free.

## Risks / Next

- If Android never hands back a usable window, the app now exits with a logged error
  (`P0_SURFACE_FAILED`) instead of aborting — acceptable degradation, no silent crash.
- `wgpu-hal`'s queue-family-0 TODO remains upstream; our guard is the app-side mitigation and should
  be removed only after wgpu-hal enumerates the surface's actual present family.
- Rotation was not forced separately (app advertises landscape); the HOME→resume cycle already
  exercised ANativeWindow recreation with a changed pointer.
