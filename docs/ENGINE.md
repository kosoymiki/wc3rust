# ENGINE — Herofolends-RS architecture and coverage (2026-11-12)

This document is the map of the implemented engine. Every crate below compiles and
has tests; the test runner is `scripts/safe-test.sh` because the shared phone
already hit the kernel OOM killer twice when cargo was run uncapped in the
interactive session (see `analysis/test-*.log`, dmesg evidence in captures 535/536).

## 1. Crates

| crate | responsibility | status |
|---|---|---|
| `wc3-core` | fixed-point Q16.16, ids, ticks | complete, 4 tests |
| `wc3-mpq` | MPQ v0 reader: encrypted tables, sectors, zlib/bzip2/stored, listfile | complete for classic archives; 3 tests on real `War3.mpq` |
| `wc3-formats` | MDX v800, BLP1/BLP2, SLK, W3E, W3I, WPM, object data, INI, whole map | complete for classic data; tests on real fixtures |
| `wc3-assetdb` | provider (MPQ/dir), content cache, MDX texture dependency graph | complete |
| `wc3-path` | 128-unit grid, A* (8-dir, no corner cutting), flow field, LOS | complete, 4 tests |
| `wc3-sim` | deterministic 30 Hz sim: units, orders, movement, combat, economy, production, abilities, fog, state hash | complete, 5 tests |
| `wc3-jass` | JASS2 lexer/parser/AST/interpreter, native registry, triggers/timers | core complete, 4 tests; native set is a documented subset |
| `wc3-replay` | replay container, command codec, checksum verification, lockstep + desync detection | complete, 4 tests |
| `wc3-input` | touch state machine, gestures, camera, command card, RTS adapter | complete, 4 tests |
| `wc3-audio` | PCM WAV decode, mixer, 3D attenuation, WAV writer | complete, 4 tests; MP3 not yet wired |
| `wc3-anim` | MDX node/sequence sampler, world matrices, particle system | complete, 3 tests |
| `wc3-render` | headless wgpu/Vulkan terrain renderer + readback | complete; GPU test runs on the device |
| `wc3-platform` | SDL3 bootstrap (static Android build) | complete |
| `wc3-gpu` | headless wgpu triangle (P0) | complete |
| `wc3-adrenotools` + 3 hook cdylibs | Rust port of libadrenotools | complete, 27 tests + device probe |
| `apps/smoke` | SDL3 + Turnip headless smoke | PASS on device |
| `apps/driver-probe` | namespace/hook/custom-driver/ICD probe | 0 failures on device |
| `apps/android-native` | APK `libmain.so`: SDL3 window + wgpu surface + triangle | PASS on RMX3709, Adreno 730 |
| `apps/headless` | map → sim → replay → JASS vertical runner | complete |
| `apps/bench` | MPQ/MDX/BLP/map/A*/sim timings + RSS | complete |

## 2. Data path

```
War3.mpq --wc3-mpq--> bytes --wc3-formats--> MdxModel / BlpImage / Terrain / MapInfo
                                     \--> wc3-assetdb (cache + dependency graph)
gameplay reads only wc3-assetdb; no direct file I/O from sim/render
```

- MPQ: v0 header, encrypted hash/block tables (hash types 0/1/2/3), sector
  offset table, per-sector codec tag (`0x02` zlib, `0x10` bzip2), stored sectors,
  `(listfile)`, `W3M`/`W3X` base offset. PKWARE/Huffman/ADPCM payloads return
  `MpqError::UnsupportedCompression` with the exact bits; `war3map.wts` in the
  campaign fixture uses PKWARE, so map names show `TRIGSTR_nnn` until that codec
  is ported.
- MDX v800: `VERS/MODL/SEQS/GLBS/TEXS/MTLS/GEOS/BONE/HELP/ATCH/EVTS/PIVT` plus raw
  storage for particle/ribbon/camera/light/collision chunks. Counts are validated
  against remaining bytes before allocation (OOM guard).
- BLP1: paletted and JPEG (via `jpeg-decoder`); BLP2: paletted, DXT1/3/5, BGRA.
- W3E v11: 7-byte tiles (height/water/texture/variation/cliff), 129×129 fixture;
  WPM 512×512 pathing; W3I header strings; object data `.w3u/w3t/w3a/w3d/w3b/w3h/w3q`;
  `war3map.j` script text.

## 3. Simulation model

- 30 Hz fixed step. Command queue applied in a fixed order: commands → movement →
  combat → production → economy → fog → victory.
- Units: Footman/Archer/Peasant/TownHall with HP/armor/damage/range/speed/attack
  ticks/sight/cost/train time. Orders: Idle, Move, Attack, MoveAttack, Hold.
- Movement uses `wc3-path` A* on the map grid, then waypoint steering; combat
  acquires targets in sight, chases, and applies `max(1, damage - armor)`.
- Economy: gold per player, passive income, train/build costs; production queues.
- Fog: per-cell player visibility bitmask from unit sight.
- `state_hash()` is FNV-1a over tick, resources and sorted unit state; two runs with
  the same seed/commands produce identical hashes (tested).
- Abilities: Heal and Fireball with mana/cooldown modelled in the command card.

## 4. JASS2

- Lexer (`//` comments, ints/reals/strings, identifiers, operators), parser
  (`globals`, `function`, `native`, `local`, `set`, `call`, `if/elseif/else`,
  `loop`, `exitwhen`, `return`, arrays, function references), interpreter with
  globals/locals/arrays and a native registry.
- Natives: math/string conversions (`I2R/R2I/I2S/R2S/S2I`, substring, modulo,
  pow/sqrt/sin/cos/atan2), random, unit creation/movement/state/queries,
  `DisplayTextToPlayer`/`BJDebugMsg`, triggers (`CreateTrigger`, `TriggerAddAction`,
  `TriggerRegisterTimerEvent`, `TriggerExecute`) and timers (`tick(dt)`).
- Boundaries: JASS2 `type` declarations and the full ~1000-native surface are not
  implemented; unknown functions/natives return explicit errors.

## 5. Replay / lockstep

- Container `H2RP` v1: seed, map hash, final hash, records `(tick, player, command)`,
  binary command codec, `verify()` re-simulates and compares the final hash.
- `Lockstep`: `delay_ticks`, per-slot command queue, per-tick checksums,
  `desync()` returns the first divergent slot pair.

## 6. Touch / UX

- `GestureRecognizer`: tap/double-tap/long-press/drag/two-finger pan with time and
  distance thresholds. `Camera`: screen↔world mapping, pan, zoom, bounds clamp.
- `CommandCard`: slots, mana/cooldown gating. `RtsController`: selection + gesture →
  Move/Attack/Stop/Train/Build orders.

## 7. Audio / animation / rendering

- WAV PCM decode + mixer with distance attenuation; generated UI sounds via
  `encode_wav_pcm16`. MP3 is a documented gap (symphonia not wired yet).
- `Animator`: sequence playback/looping, node track sampling, hierarchy matrices,
  particle emitter. MDX interpolation supports none/linear/hermite/bezier.
- `TerrainRenderer`: wgpu/Vulkan, terrain mesh from W3E heights, camera uniform,
  offscreen render + readback (test asserts multiple colors and green terrain).

## 8. Android APK

- `apps/android/app`: SDL3 `SDLActivity` Java layer; `apps/android-native` exports
  `SDL_main`, creates an SDL3 Vulkan window, obtains the `ANativeWindow` from
  `SDL_PROP_WINDOW_ANDROID_WINDOW_POINTER`, builds a wgpu surface and presents a
  triangle. `scripts/build-apk.sh` builds a signed APK with aapt2/javac/d8/
  zipalign/apksigner. Verified on RMX3709: `P0_WINDOW_TRIANGLE_OK first_frame
  surface=2414x1080`, Adreno 730, screenshot in `analysis/p0-apk-final.png`.
- SDL3 is built for Android with `third_party/sdl3-android-build`; hook libs are
  Rust cdylibs. Turnip in a native APK needs a bionic Turnip build or a bundled
  Khronos loader + ICD JSON; the current APK uses the vendor driver. See
  `docs/ADRENOTOOLS_RUST_PORT.md` and `docs/WINLATOR_TURNIP_RESEARCH.md`.

## 9. Verification

```bash
# one crate at a time, memory-capped (do NOT run --workspace in-session)
./scripts/safe-test.sh wc3-mpq
./scripts/safe-test.sh wc3-formats
./scripts/safe-test.sh wc3-sim
# ... etc. Each log ends with SAFE_TEST_DONE crate=<name> rc=<0|1>
```

Device evidence:
- `./scripts/p0-smoke.sh` — SDL3 3.5.0 + Turnip triangle.
- `./target/debug/driver-probe` — 0 failures; Turnip ICD loads.
- APK: `analysis/android-p0.log`, `analysis/p0-apk-final.png`.
- Bench: `./target/release/herofolends-bench`.

## 10. Known boundaries (explicit, not stubs)

- MPQ PKWARE/Huffman/ADPCM payloads: typed `UnsupportedCompression`.
- MDX versions other than 800: typed `Unsupported`.
- BLP2 JPEG / unknown compression: typed `Unsupported`.
- W3I only the header strings; random unit tables in `.doo` unsupported.
- JASS native coverage is a core subset; full map scripts may fail parse/run.
- Audio MP3 not wired; WAV complete.


## 11. Playable game layer (`wc3-game`) — 2026-11-12

`crates/wc3-game` turns the engine crates into a playable RTS:

- `assets.rs` loads the player's `War3.mpq` + map: MDX unit meshes (CPU-skinned),
  BLP textures, terrain art via `TerrainArt\Terrain.slk`, WPM pathing, minimap.
  BLP1 JPEG is decoded as CMYK-with-K-alpha (the actual WC3 encoding), MDX is
  converted Z-up→Y-up after skinning, and `GEOA` geoset alpha hides helper
  planes.
- `game.rs` owns the deterministic `Sim`, selection, orders, build placement,
  production, AI (train + timed waves), harvest economy, win/lose, touch/camera
  input, minimap, HUD state and stable selection cells.
- `hud.rs` rasterises the device system font (Roboto) into an atlas, with a
  5×7 fallback; command card, resources, selection panel, minimap, health bars,
  toasts and the victory/defeat overlay.
- `render.rs` draws the real terrain, skinned units, selection rings and HUD
  through wgpu/Vulkan; unit textures get linear-light mip chains.
- `camera.rs` is the RTS camera (45° projection, screen↔terrain picking,
  pinch zoom, sub-pixel pan).

On-device: the APK starts the game loop with SDL3 + wgpu. A root-installed Mesa
Turnip ICD is selected through `SDL_VULKAN_LIBRARY` and the
`crates/wc3-vulkan-loader` shim (AOSP loader + Turnip ICD via the Rust
libadrenotools port). The stock AOSP loader is used when Turnip is absent, so
the app still runs on the vendor Adreno driver.

Tests: `docs/TESTING.md` — unit crates, asset invariants, gameplay wiring,
headless visual regression on the real map, and the on-device tap self-test
(`scripts/game-selftest.sh`), which requires the Turnip adapter and screenshots
every stage.
