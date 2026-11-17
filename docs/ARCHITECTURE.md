# ARCHITECTURE — wc3rust

## 1. Layering

```
┌────────────────────────────────────────────────────────────────────┐
│ apps: SDL3 (Android/Windows/Linux) + CLI tools                      │
├────────────────────────────────────────────────────────────────────┤
│ launcher + UI (wgpu UI, native Android settings)                   │
├────────────────────────────────────────────────────────────────────┤
│ input: touch router → gesture recognizer → RTS adapter → commands  │
├────────────────────────────────────────────────────────────────────┤
│ simulation: ECS world, orders, pathfinding, combat, abilities, fog │
│            fixed 20/30 Hz, deterministic, replay log               │
├────────────────────────────────────────────────────────────────────┤
│ scripting: JASS2 VM + trigger engine + map scripts                 │
├────────────────────────────────────────────────────────────────────┤
│ scene: extracted render data (instances, animations, particles)    │
├────────────────────────────────────────────────────────────────────┤
│ renderer: wgpu (Vulkan) pipelines, culling, shadows, terrain, UI   │
├────────────────────────────────────────────────────────────────────┤
│ asset DB: MPQ/CASC/loose → MDX/BLP/SLK → glTF/KTX2 → GPU cache     │
├────────────────────────────────────────────────────────────────────┤
│ platform: Android (JNI/NativeActivity), Windows/Linux (winit)      │
└────────────────────────────────────────────────────────────────────┘
```

**Rule:** simulation never touches GPU/files; renderer never mutates simulation; input emits
commands; asset DB is the only reader of user data.

### 1.1 Platform/library stack (technology baseline)

| layer | choice | почему |
|---|---|---|
| window/input/audio | **SDL3** (`sdl3` crate, zlib) | cross-platform, Android/Windows/Linux, touch/pen/gamepad/haptics/audio, SDL3 GPU API |
| renderer | **wgpu** (Vulkan backend, Apache-2.0/MIT) + **naga** WGSL | Rust-native, WebGPU-style, validation, portable |
| secondary GPU | **SDL3 GPU API** (SPIR-V/DXIL/MSL) behind feature flag | A/B, low-level control, fallback/debug |
| Android | SDL3 Android shim (`SDLActivity`, min API 21 per SDL docs; target 26/29+) | proven Android path; Java shim + JNI |
| GPU driver | system Adreno Vulkan + **Turnip via libadrenotools** (BSD-2) | rootless custom driver, free Mesa builds, fallback |
| ECS | **bevy_ecs** (Apache-2.0/MIT) | mature, deterministic-friendly, no full engine lock-in |
| math | **glam** (MIT/Apache) | SIMD-friendly vectors/matrices |
| collision | **parry** (Apache-2.0) | broadphase/narrowphase, no physics runtime lock-in |
| audio | **kira** (Apache-2.0) + **symphonia** (MPL-2.0, verify) | mixing, streaming, decode |
| assets | **gltf**, **image**, **ktx2**, **basis-universal** | glTF/KTX2/Basis pipeline |
| serialization | **serde** + **postcard**/**rkyv** | cache/save/replay |
| async/net | **tokio** + **quinn** (later) | tools, LAN/QUIC experiments |
| observability | **tracing** + **metrics** | spans/perf |
| quality | `cargo-deny`, `cargo-audit`, `nextest`, `proptest`, `criterion`, `cargo-fuzz`, Miri/sanitizers | supply chain + correctness |

**Turnip is free/open-source:** Mesa core is MIT; Turnip is part of Mesa Freedreno; community
builds (e.g. K11MCH1/AdrenoToolsDrivers) are freely distributed. We ship a **driver manager**
(import/download `.so` → SHA-256 → internal storage → `libadrenotools`), not a hard dependency;
system Vulkan is always the fallback.

## 2. Data flow

```
user data folder
  ├─ MPQ (War3.mpq, War3x.mpq, maps) ─┐
  ├─ CASC (1.30+/Reforged) ───────────┤
  └─ loose files ─────────────────────┘
                ↓
        wc3-formats (readers)
                ↓
        wc3-assetdb (hash, deps, cache)
          ├─ MDX/MDL → glTF 2.0 (meshes, skins, animations, materials)
          ├─ BLP1 → KTX2 (ASTC/BC7/BC1, mips, sRGB)
          ├─ SLK/INI/TXT → typed data tables
          └─ W3M/W3X → map, object data, pathing, triggers
                ↓
        wc3-sim (data) + wc3-render (GPU resources)
```

## 3. Rust workspace (target)

```
wc3rust/
  Cargo.toml (workspace)
  crates/
    wc3-core        ids, math, time, errors, config
    wc3-formats     MPQ, CASC, MDX/MDL, BLP, SLK, INI, W3M/W3X, DDS
    wc3-assetdb     content hashes, dependency graph, cache, async load
    wc3-render      wgpu device/queues/pipelines, culling, shadows, terrain
    wc3-scene       extracted scene graph, instancing, animation state
    wc3-ui          wgpu UI, layout, themes, input focus
    wc3-sim         ECS world, tick, orders, combat, resources, fog
    wc3-path        grid/flow-field/A*, navmesh, collision
    wc3-ai          computer players, build orders, attack waves
    wc3-jass        JASS2 lexer/parser/AST/VM/natives
    wc3-triggers    events, triggers, regions, dialogs, quests
    wc3-input       action/command model, keybinds, mouse
    wc3-touch       router/gesture/RTS adapter, S Pen, haptics
    wc3-audio       WAV/MP3, mixer, 3D sound, music
    wc3-anim        skeletal/keyframe/particles/ribbons
    wc3-android     JNI, NativeActivity/GameActivity, storage, lifecycle
    wc3-desktop     winit window/input/audio
    wc3-launcher    data check, profiles, map/campaign/custom/load
    wc3-replay      command log, hash, verification
    wc3-net         later: lockstep, lobby, checksum
    wc3-testkit     fixtures, golden, fuzz, differential
  tools/
    wc3extract      CLI: list/extract MPQ/CASC, convert MDX/BLP
    bench           device benchmark harness
  apps/
    android         APK (engine only, no assets)
    desktop         Windows/Linux binary
    cli             tools
```

## 4. Renderer (wgpu/Vulkan)

- `wgpu::Instance` with Vulkan backend; optional `libadrenotools` custom driver on Android.
- Window: **SDL3** (`SDL_WINDOW_VULKAN`) with native handle exposed via `SDL_GetWindowProperties`
  and `raw-window-handle`; create `wgpu::Surface` with `SurfaceTargetUnsafe::RawHandle` (Win32
  HWND, X11/Wayland, Android `ANativeWindow`). Verify adapter limits/features at startup.
- Optional `SDL3 GPU` backend (`SDL_CreateGPUDevice` + SPIR-V) for A/B, driver debugging and
  low-level control; wgpu remains the primary path for WGSL/naga/validation.
- Surface: configure sRGB/linear; DPR clamp; recreate on resize/lifecycle.
- Pipelines: terrain, static meshes, skinned meshes, particles/ribbons, water, UI, shadows,
  post-process (FXAA/tonemap); pipeline cache persisted.
- Bind groups: per-frame, per-pass, per-material, per-instance; dynamic offsets for uniforms.
- Culling: frustum + occlusion (optional); instancing for trees/units; LOD.
- Shadows: cascaded shadow maps (2–3 cascades) high profile; blob shadows low.
- Textures: KTX2 ASTC/ETC2 (mobile), BC7/BC1 (desktop), mips, sRGB; streaming.
- Frame graph: acquire → shadow → opaque → alpha → water → UI → post → present.
- Device loss: `device.lost` → rebuild from renderer-agnostic scene snapshot.
- WebGPU? Not required; wgpu native Vulkan; future WebGPU target possible.

## 5. Simulation

- Fixed tick 20/30 Hz; accumulator; input commands timestamped by tick.
- ECS: `bevy_ecs` (or custom archetype) with components: Transform, Unit, Owner, Health, Mana,
  Order, Movement, Attack, Ability, Buff, Production, Vision, Fog.
- Orders: move, attack, attack-move, cast, build, train, research, hold, stop, patrol.
- Pathfinding: grid A* for small, flow field for groups; dynamic blockers; unit collision.
- Combat: attack types/armor/damage tables, cooldown, projectiles, splash, bounce.
- Fog: per-player visibility grid, explored/visible, hidden enemy units.
- Determinism: fixed-point or deterministic float policy; ordered iteration; no HashMap iteration
  in sim; replay hash each tick.
- Save/load: versioned snapshot + RNG state + command log.

## 6. Scripting (JASS2)

- Lexer/parser/AST; VM with typed handles; native registry; events/triggers.
- Map script entry points (`main`, `config`, `InitCustomTriggers`, etc.).
- Sandbox: no filesystem/network from map scripts; memory/time budget; opcode limits.
- Differential tests: run same map in Warsmash/W3AndroidEngine vs Rust; compare triggers/events.
- Fallback: unsupported natives log and no-op? No — must implement or fail loudly with map
  compatibility report; no silent stubs.

## 7. Input / touch

- **SDL3 events**: `SDL_EVENT_FINGER_*`, `SDL_EVENT_PEN_*`, mouse/keyboard/gamepad, haptic,
  audio device, lifecycle. SDL3 is the single platform event source.
- `TouchInputRouter`: SDL3 pointer events → queue → render thread.
- `GestureRecognizer`: tap, long-press, drag, two-finger pan/pinch, two-finger tap; DPI thresholds.
- `RtsTouchAdapter`: gesture → synthetic mouse/keyboard events; command card, selection,
  build/ability targeting, Shift overlay, S Pen, palm filter.
- `wc3-input`: semantic commands (`Select`, `Move`, `Attack`, `Cast`, `Build`, `Train`, `Group`).
- Game logic consumes commands only; replay stores commands, not raw touches.

## 8. Android / SDL3 / Turnip

- **SDL3 Android**: Gradle project + `SDLActivity` Java shim; SDL docs require Android SDK 36+,
  NDK r28c+, min API 21; we target API 26/29+ and arm64-v8a.
- `sdl3` crate with `build-from-source`/`sdl3-sys`; JNI glue; lifecycle via SDL events.
- Storage: user-selected folder via SAF; user data read-only; driver directory in
  `internalDataPath` (never sdcard; libadrenotools requires non-world-writable path).
- Vulkan: system Adreno by default; optional `libadrenotools` custom Turnip driver
  (`adrenotools_open_libvulkan`), `useLegacyPackaging=true`, SHA-256 verification, fallback.
- **Driver manager**: list/import/download Turnip `.so` (Mesa/K11MCH1 builds), verify hash,
  store, select per profile, show driver name/version; rollback to system; no root.
- Lifecycle: pause/resume surface, audio focus, thermal/battery callbacks, haptics.
- APK: engine only; no game assets; split ABIs; strip symbols; baseline profile.

## 9. Threading

- Main/render thread; simulation thread (or same thread at fixed tick for determinism);
- Asset IO thread(s) + decoder pool; audio thread; JASS VM on sim thread.
- No locks in hot path; channels; double-buffered command/event queues.
- GPU upload via staging ring; no per-frame allocations.

## 10. Determinism / replay

- All sim input = `(tick, player, command, args)`; commands sorted by player id.
- RNG: explicit seed + per-system streams; no OS RNG in sim.
- Replay = header (map hash, data hash, engine version) + command log + periodic state hash.
- Cross-platform hash: quantize floats at tick boundary or use fixed-point for sim.
- Desync diagnostics: per-system hashes, first divergent tick, dump.

## 11. Security / anti-cheat

- Single-player: no trust boundary.
- Multiplayer: server/lockstep validates command legality; replay audit; rate limits.
- Map scripts sandboxed; no network/file access; memory/time caps.
- User data read-only; no arbitrary code execution from maps beyond JASS sandbox.

## 12. Observability

- `tracing` spans: load, tick, render, JASS; metrics: FPS, frame time, tick time, memory, VRAM,
  asset cache hits, desyncs.
- Device log export; benchmark JSON; crash handler with backtrace (no PII).
