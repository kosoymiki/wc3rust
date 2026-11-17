# PLAN — Herofolends-RS engineering plan

Дата: 2026-11-12. Статус: Phase 0 (analysis/plan). План живой; каждая фаза заканчивается
работающим вертикальным срезом, тестами и evidence. Никаких заглушек/TODO/placeholder.

## 0. Goal and invariants

**Goal:** Rust-native, Vulkan-first, touch-first WC3-compatible RTS engine for Android/Windows/Linux,
which reads user-owned WC3 data (MPQ/CASC), supports Classic + HD/Reforged graphics sets, JASS2 maps,
deterministic replay, and outperforms existing implementations on measured device benchmarks.

**Invariants:**
1. No Blizzard code/assets in repo; user supplies data.
2. Clean-room code; donor use is behavioral/format reference with license compliance.
3. Server-authoritative? No — single-player deterministic first; multiplayer lockstep later.
4. Fixed-step simulation; rendering decoupled; no simulation state in renderer.
5. Touch input is translated into engine commands; game logic never reads raw touch.
6. One asset database; all formats go through it; no direct file reads from gameplay.
7. Every phase has tests, benchmark, crash/soak, and H2 evidence.
8. No password/DRM cracking; encrypted FreeArc volumes are out of scope without user key.

## 0.1 Technology baseline (must be present)

- **SDL3** (`sdl3`/`sdl3-sys`, zlib): window, input, touch/pen, gamepad, haptics, audio, Android
  `SDLActivity`, lifecycle. SDL3 GPU API is an optional secondary backend.
- **wgpu** (Apache-2.0/MIT) + **naga** for Vulkan/WGSL; SDL3 native window handle → wgpu surface.
- **Turnip** (Mesa Freedreno, MIT) via **libadrenotools** (BSD-2) as optional rootless custom driver;
  system Adreno Vulkan fallback; driver manager with SHA-256 and rollback.
- **bevy_ecs**, **glam**, **parry**, **kira**, **symphonia** (verify MPL-2.0), **gltf**, **image**,
  **ktx2**, **basis-universal**, **serde/rkyv/postcard**, **tracing**, **tokio/quinn** (tools/net).
- Quality: `cargo-deny`, `cargo-audit`, `nextest`, `proptest`, `criterion`, `cargo-fuzz`, Miri/
  sanitizers, reproducible builds, SBOM. No placeholders; no unverified unsafe.

## 1. Workstreams

| stream | owner module(s) | output |
|---|---|---|
| A. Data/format | `wc3-formats`, `wc3-assetdb`, `tools-cli` | MPQ/CASC, MDX/BLP/SLK, glTF/KTX2, test corpus |
| B. Renderer | `wc3-render`, `wc3-scene`, `wc3-ui` | wgpu Vulkan pipelines, terrain/water/shadows/UI |
| C. Simulation | `wc3-sim`, `wc3-path`, `wc3-ai` | ECS world, orders, combat, abilities, fog |
| D. Scripting | `wc3-jass`, `wc3-triggers` | JASS2 VM, natives, triggers, map scripts |
| E. Input/UX | `wc3-input`, `wc3-touch`, `wc3-launcher` | touch translator, gestures, HUD, launcher |
| F. Audio/animation | `wc3-audio`, `wc3-anim` | sound, music, skeletal/particle animation |
| G. Platform | `wc3-platform` (SDL3), `wc3-android`, `wc3-gpu`, `wc3-drivers` | SDL3 window/input/audio, Android/Windows/Linux, Turnip/Adreno |
| H. Quality | `wc3-testkit`, `bench`, `replay` | differential tests, fuzzing, benchmarks, replay |

## 2. Phases

### Phase 0 — RE, legal, skeleton (1–2 weeks)
- [x] Local artifact inventory (APK, setup, data, ISO).
- [x] Donor download + byte inventory + matrix (`DONORS.md`).
- [x] Format/JASS/engine reference map (`RESEARCH.md`).
- [ ] Legal gate `LEGAL.md` signed by operator (docs complete, sign-off pending).
- [x] Data gate: ISO MPQ available and `War3.mpq` opens via StormLib (10 525 entries; samples extracted).
- [x] Cargo workspace + SDL3 3.5.0 init/events + wgpu/Vulkan headless triangle on Termux/Turnip (`p0-smoke`).
- [x] SDL3 window + wgpu surface via SDLActivity/NDK on Android (APK verified on RMX3709: `P0_WINDOW_TRIANGLE_OK surface=2414x1080`, Adreno 730).
- [x] Baseline benchmark harness (`apps/bench`: MPQ/MDX/BLP/map/A*/1000 sim ticks + RSS); W3AndroidEngine side-by-side remains an external install.
- **Exit:** SDL3 init/version, wgpu/Vulkan triangle and Android APK window/surface verified on device; `War3.mpq` opens and real MDX/BLP/SLK/W3E/WPM fixtures pass; legal sign-off and W3AndroidEngine comparison remain external.

### Phase 1 — Data + formats (2–4 weeks)
- [x] MPQ reader (StormLib-compatible core: encrypted tables, sectors, zlib/bzip2/stored; `War3.mpq` tests).
- CASC reader (BLTE/encoding keys; test against user CASC if available).
- [x] MDX v800 parser, BLP1/BLP2 decoder, SLK/INI parsers (glTF exporter, KTX2/ASTC/BC encoders pending).
- [x] W3M/W3X map reader, object data (w3u/w3t/w3a/w3d/w3b/w3h/w3q), W3E terrain, WPM pathing, W3I header.
- [x] Asset DB with cache and MDX texture dependency graph (content hashes/async load pending).
- **Exit:** given `War3.mpq`, extract a map, render one MDX unit with BLP texture on Android at
  60 fps; parsers pass golden/round-trip tests; no direct file I/O outside asset DB.

### Phase 2 — RTS vertical slice (4–8 weeks)
- [x] Deterministic 30 Hz world, command queue, selection model, orders, movement, state hash.
- [x] Terrain/heightmap/pathing, A*, flow field, line-of-sight, fog of war (unit-group formation pending).
- [x] Combat (attack/armor/cooldown/death), resources, production, abilities (projectile visuals pending).
- One classic melee map playable vs passive AI; camera/touch command card.
- **Exit:** 10-minute single-player match on Android at target FPS; deterministic replay from
  seed+commands; memory <1.5 GB; no crash in 30-min soak.

### Phase 3 — JASS2, abilities, campaign (6–10 weeks)
- [x] JASS2 lexer/parser/AST/interpreter, core native registry, handles, triggers/timers; full native surface pending.
- [~] Ability/item data parsing (w3a/w3t/w3q), cooldowns/mana; buffs/autocast pending.
- Campaign flow, save/load, cinematics, quests, dialogs, sounds.
- Differential harness vs Warsmash/W3AndroidEngine on map corpus.
- **Exit:** ≥95% of test maps load and run triggers; campaign chapter 1 completable; replay
  hashes match across repeated runs; no JASS VM crash on fuzz corpus.

### Phase 4 — Touch UX, UI, audio, graphics sets (6–10 weeks)
- Touch translator (router/gesture/RTS adapter), command card, selection, camera, build/ability
  targeting, Shift overlay, S Pen, haptics, accessibility, settings.
- Launcher: data folder, profiles, campaign/map/custom game/load/options/help.
- [~] WAV decode + mixer + 3D attenuation; MDX keyframe sampler + particles; MP3/music/ribbons pending.
- Classic + HD/Reforged graphics sets; PBR/ORM/normal/env; team color.
- **Exit:** phone/tablet playtest (≥5 users) completes a match without mouse/keyboard; touch
  latency p95 <80 ms; UI scales across 6"/10"/fold; no soft-locks.

### Phase 5 — Determinism, mods, performance (4–8 weeks)
- [x] Replay hash, command log, `H2RP` container verification, lockstep scheduler + desync detection; save/load versioning pending.
- Mod packages (maps + data overrides + scripts), sandbox, compatibility checks.
- Performance: wgpu pipeline cache, GPU/CPU profiling, streaming, LOD, ASTC/BC, thermal.
- **Exit:** replay bit-exact Android/Windows; 60 fps mid/120 high/30 low; cold start <4 s;
  memory <1.5 GB; mod map runs.

**Implementation note (2026-11-12):** P0–P2 core is implemented in Rust (`wc3-mpq`, `wc3-formats`, `wc3-assetdb`, `wc3-path`, `wc3-sim`, `wc3-jass`, `wc3-replay`, `wc3-input`, `wc3-audio`, `wc3-anim`, `wc3-render`, `apps/headless`, `apps/bench`); see `docs/ENGINE.md` for exact coverage and boundaries. Remaining work is listed there per stream (full JASS natives, MP3, HD graphics, network transport, campaign flow, legal sign-off).

### Phase 6 — Multiplayer + hardening (6–10 weeks)
- LAN/direct lockstep (input delay, checksum, desync diagnostics), lobby, reconnect.
- Anti-cheat (command validation, replay audit), crash reporting, telemetry consent.
- Security review, legal review, release packaging, CI, reproducible builds.
- **Exit:** 2–4 player LAN match, no desync over 30 min; release candidate passes gates.

## 3. Acceptance gates (summary)

| gate | criterion |
|---|---|
| G0 legal | no assets/code; user data; licenses recorded |
| G1 data | MPQ `War3.mpq` opens; map extracted; CRC test |
| G2 formats | MDX/BLP/SLK golden tests; 100% test corpus parse |
| G3 render | 60 fps mid, p99 <16.6 ms; context/device loss recovery |
| G4 sim | deterministic replay hash; 1000 units tick <4 ms |
| G5 JASS | ≥95% map corpus; no VM crash on fuzz |
| G6 touch | p95 <80 ms; 5-user playtest; no soft-lock |
| G7 performance | cold start/memory/FPS targets; thermal soak |
| G8 multiplayer | 2–4 player LAN, no desync 30 min |

## 4. Risks and mitigations

| risk | impact | mitigation |
|---|---|---|
| Blizzard IP/AUP | project blocked | clean-room, no assets, legal review, non-commercial |
| FreeArc password | data unavailable | use ISO MPQ; user-assisted extraction; no cracking |
| JASS2 surface | compatibility | differential tests + map corpus; incremental natives |
| Turnip instability | renderer crashes | system Vulkan fallback; driver matrix |
| Scope | years | vertical slices; phase gates; drop non-essential features |
| SDL3 Android build | platform blocker | use official SDL3 Android project; CI build; device smoke |
| Turnip packaging | driver load failure | libadrenotools + internal storage + fallback; driver matrix |
| Touch UX | unplayable | adopt W3AndroidEngine/Gestures; playtests; settings |
| Performance | worse than Java | native Rust/Vulkan; benchmark; profile early |
| Reforged assets | legal/format | user-owned only; HD optional; classic first |

## 5. Team / module ownership

- Data/format: 2 engineers (Rust, binary formats).
- Renderer: 2 (wgpu/Vulkan, shaders, Android GPU).
- Simulation/JASS: 2–3 (RTS, VM, determinism).
- Touch/UI: 1–2 (Android UX, gestures).
- QA/perf: 1 (bench, fuzz, device lab).
For H2 multi-agent: THICK packets, exclusive OWNER_PATHS, evidence gates, monitor close.

## 6. Evidence / H2

Every phase: `harness2_capture`, `harness2_experience`, monitor checkpoint/close; benchmark JSON;
replay corpus; parser golden tests; device logs. No "done" without command output.
