# MODULES — карта модулей wc3rust

Формат: **что → зачем → откуда (донор/спека) → входы/выходы → зависит от → фаза → тесты**.
Границы модулей = границы владения; в H2 multi-agent у каждого `OWNER_PATHS` и evidence gate.

## 1. Сводная карта

| crate | роль | фаза | ключевые доноры/спеки |
|---|---|---|---|
| `wc3-core` | ids, math, time, errors, config | P0 | glam, std |
| `wc3-formats` | MPQ/CASC/MDX/BLP/SLK/W3M/W3X | P1 | StormLib, CascLib, mdx-m3-viewer, war3-model, War3Net |
| `wc3-assetdb` | content hash, deps, async cache | P1 | War3Net, mdx-m3-viewer |
| `wc3-render` | wgpu/Vulkan pipelines | P0–P1 | wgpu, OpenRA separation |
| `wc3-scene` | render scene, instancing, animation state | P1–P2 | mdx-m3-viewer, war3-model |
| `wc3-ui` | wgpu UI/layout/themes | P1–P2 | wc3rust UI flow, W3AndroidEngine launcher |
| `wc3-sim` | ECS world/tick/orders/combat | P2 | Warsmash, OpenRA |
| `wc3-path` | grid/flow-field/A*/collision | P2 | OpenRA, War3Net pathing |
| `wc3-ai` | computer players | P2–P3 | OpenRA, Warsmash |
| `wc3-jass` | JASS2 VM/natives | P3 | Warsmash/W3AndroidEngine/War3Net |
| `wc3-triggers` | events/triggers/regions/dialogs | P3 | Warsmash, War3Net |
| `wc3-input` | semantic commands/keybinds/mouse | P0–P2 | W3AndroidEngine, Generals touch |
| `wc3-touch` | touch translator/gestures/S Pen | P0–P2 | W3AndroidEngine, Generals touch |
| `wc3-audio` | WAV/MP3/mixer/3D | P2–P4 | OpenRA, symphonia/kira |
| `wc3-anim` | skeletal/keyframe/particles/ribbons | P2–P4 | war3-model, mdx-m3-viewer |
| `wc3-platform` | SDL3 window/input/audio/lifecycle | P0 | SDL3, sdl3-rs, sdl3-sys |
| `wc3-android` | SDL3 Android shim/JNI/storage/Turnip | P0 | SDL3 Android, libadrenotools |
| `wc3-gpu` | wgpu adapter/device/surface; optional SDL3 GPU | P0–P1 | wgpu, naga, SDL3 GPU |
| `wc3-drivers` | Turnip driver manager/import/hash/rollback | P0–P1 | libadrenotools, Mesa |
| `wc3-launcher` | data check/profiles/maps/campaign | P1–P4 | W3AndroidEngine, wc3rust |
| `wc3-replay` | command log/hash/verify | P2–P5 | W3AndroidEngine determinism, War3Net.Replay |
| `wc3-net` | lockstep/lobby/checksum | P6 | OpenRA, Gaffer |
| `wc3-testkit` | fixtures/golden/fuzz/differential | P0+ | all donors |

## 2. Module cards

### `wc3-core`
- **Что:** фундаментальные типы: `PlayerId`, `UnitId`, `AbilityId`, `Tick`, `Fixed`, `Vec2/3`,
  `Result`, `Error`, `Config`.
- **Зачем:** единые ids/детерминированная математика/ошибки; никаких `f32` в sim-логике без
  политики.
- **Откуда:** `glam` (math), Rust std; собственные newtypes.
- **Вход/выход:** config/ids → типы для всех крейтов.
- **Зависит:** нет.
- **Фаза:** P0.
- **Тесты:** unit; fixed-point/float policy; serde round-trip.
- **Риск:** преждевременная абстракция; держать минимальным.

### `wc3-formats`
- **Что:** readers/writers: MPQ (v1, listfile, encryption), CASC (BLTE, keys), MDX/MDL (800/1000/1100),
  BLP1, SLK/INI/TXT, W3M/W3X, DDS/TGA, pathing map.
- **Зачем:** весь user-owned контент; без этого нет игры.
- **Откуда:** StormLib (MPQ), CascLib (CASC), mdx-m3-viewer/war3-model (MDX/BLP), War3Net (SLK/maps).
- **Вход:** bytes/reader; **выход:** typed structs + raw buffers.
- **Зависит:** `wc3-core`.
- **Фаза:** P1.
- **Тесты:** golden fixtures из `War3.mpq`, MDX/BLP corpus; round-trip; fuzz; CRC/listfile.
- **Риск:** protected maps/encryption; CASC root handlers; MDX edge chunks.

### `wc3-assetdb`
- **Что:** индексирует источники, считает content hash/зависимости, кэширует decoded ресурсы,
  стримит GPU upload.
- **Зачем:** один вход для sim/render; быстрый cold start; no direct file reads.
- **Откуда:** War3Net Build/MPQ, mdx-m3-viewer parsers; собственная модель.
- **Вход:** data folder + requested path; **выход:** `AssetHandle<T>`.
- **Зависит:** `wc3-formats`, `wc3-core`.
- **Фаза:** P1.
- **Тесты:** cache hit/miss, eviction, dependency invalidation, concurrency.
- **Риск:** memory blowup; budget/LRU/streaming.

### `wc3-render`
- **Что:** wgpu instance/device/surface, pipelines, bind groups, shadow/terrain/water/UI passes,
  culling, pipeline cache, device loss.
- **Зачем:** GPU-рендер без GLES-наследия; Vulkan/Turnip.
- **Откуда:** wgpu docs; OpenRA renderer separation; RTR/GPU Gems lessons.
- **Вход:** `wc3-scene` + camera; **выход:** frame.
- **Зависит:** `wc3-core`, `wc3-scene`, `wc3-assetdb`.
- **Фаза:** P0–P1.
- **Тесты:** golden screenshots, GPU validation, device-loss, frame-time benchmark.
- **Риск:** Android surface/lifecycle, driver bugs; fallback.

### `wc3-scene`
- **Что:** renderer-agnostic scene: instances, meshes, materials, lights, particles, animations;
  extraction from MDX; culling data.
- **Зачем:** разделить sim и GPU; snapshot для device-loss.
- **Откуда:** mdx-m3-viewer/war3-model MDX semantics; RTR.
- **Вход:** asset handles + sim snapshot; **выход:** draw list.
- **Зависит:** `wc3-core`, `wc3-assetdb`.
- **Фаза:** P1–P2.
- **Тесты:** instancing/ordering, animation sampling, LOD.
- **Риск:** overdraw; team color/replaceable IDs.

### `wc3-ui`
- **Что:** wgpu UI: layout, widgets, command card, minimap frame, tooltips, dialogs, themes,
  safe areas, DPI.
- **Зачем:** touch-first HUD, Classic/Reforged skins, accessibility.
- **Откуда:** wc3rust UI XML flow; W3AndroidEngine launcher; FDF concepts.
- **Вход:** UI model + input; **выход:** draw + events.
- **Зависит:** `wc3-render`, `wc3-input`.
- **Фаза:** P1–P2.
- **Тесты:** layout snapshots, hit-testing, scale/safe-area, contrast.
- **Риск:** tiny touch targets; 6"/10" layouts.

### `wc3-sim`
- **Что:** ECS world, fixed tick, players, units, orders, combat, resources, production, fog,
  victory; command application; replay hash.
- **Зачем:** детерминированное ядро игры.
- **Откуда:** Warsmash/W3AndroidEngine domain, OpenRA traits; собственный ECS.
- **Вход:** commands + data tables; **выход:** events/snapshot.
- **Зависит:** `wc3-core`, `wc3-path`, `wc3-jass` (events).
- **Фаза:** P2.
- **Тесты:** golden scenarios, determinism, property (resources/conservation), replay.
- **Риск:** floating-point nondeterminism; ordered iteration.

### `wc3-path`
- **Что:** grid, A*, flow field, dynamic blockers, unit collision, path smoothing.
- **Зачем:** RTS movement/attack-move; performance for groups.
- **Откуда:** OpenRA pathfinding, War3Net pathing map; literature.
- **Вход:** pathing grid + start/goal; **выход:** path/flow.
- **Зависит:** `wc3-core`, `wc3-formats`.
- **Фаза:** P2.
- **Тесты:** shortest path, blocked, dynamic, 1000-unit benchmark.
- **Риск:** pathological maps; cap/fallback.

### `wc3-ai`
- **Что:** computer player: build orders, scouting, attack waves, difficulty, cheating-off.
- **Зачем:** single-player без человека.
- **Откуда:** OpenRA AI, Warsmash; собственные behavior trees/utility.
- **Вход:** world snapshot; **выход:** commands.
- **Зависит:** `wc3-sim`, `wc3-path`.
- **Фаза:** P2–P3.
- **Тесты:** deterministic AI, no illegal commands, difficulty.
- **Риск:** CPU cost; budget per tick.

### `wc3-jass`
- **Что:** JASS2 lexer/parser/AST/compiler/VM/natives; handles; events; string/array; timers.
- **Зачем:** карты/кампании/триггеры; совместимость.
- **Откуда:** Warsmash Jass2 (~10.5k lines), W3AndroidEngine, War3Net AST.
- **Вход:** map script + natives; **выход:** VM state/events.
- **Зависит:** `wc3-core`, `wc3-sim`.
- **Фаза:** P3.
- **Тесты:** parser golden, VM opcode, differential trace, fuzz.
- **Риск:** native surface huge; sandbox limits.

### `wc3-triggers`
- **Что:** trigger events/conditions/actions, regions, dialogs, quests, cinematics, sounds.
- **Зачем:** map logic/campaign.
- **Откуда:** Warsmash/War3Net.
- **Вход:** trigger data + JASS; **выход:** events.
- **Зависит:** `wc3-jass`, `wc3-sim`.
- **Фаза:** P3.
- **Тесты:** trigger fixtures, ordering, save/load.
- **Риск:** event ordering; re-entrancy.

### `wc3-input`
- **Что:** semantic command model, keybinds, mouse, selection, groups, camera; replay commands.
- **Зачем:** единый интерфейс для touch/mouse/keyboard.
- **Откуда:** W3AndroidEngine, OpenRA input.
- **Вход:** raw events; **выход:** `Command`.
- **Зависит:** `wc3-core`.
- **Фаза:** P0–P2.
- **Тесты:** binding, selection, command validation.
- **Риск:** mouse-only assumptions.

### `wc3-touch`
- **Что:** router/gesture recognizer/RTS adapter, S Pen, palm filter, haptics, settings.
- **Зачем:** игра пальцем без мыши; latency/UX.
- **Откуда:** W3AndroidEngine `input/touch/*`, Generals touch doc.
- **Вход:** `PointerEvent`; **выход:** synthetic mouse/commands.
- **Зависит:** `wc3-input`.
- **Фаза:** P0–P2.
- **Тесты:** gesture state machine, DPI thresholds, multi-touch, cancelled touches, device.
- **Риск:** accidental orders; long-press vs drag.

### `wc3-audio`
- **Что:** decode WAV/MP3, mixer, 2D/3D, music, UI, ducking.
- **Зачем:** звук — половина RTS-фидбека.
- **Откуда:** OpenRA sound, symphonia/kira/cpal.
- **Вход:** audio assets/events; **выход:** PCM.
- **Зависит:** `wc3-core`, `wc3-assetdb`.
- **Фаза:** P2–P4.
- **Тесты:** decode, latency, 3D pan, no clicks.
- **Риск:** Android audio focus/latency.

### `wc3-anim`
- **Что:** keyframe/skeletal/morph/particle/ribbon sampling; animation state machine.
- **Зачем:** MDX-анимации, бой, окружение.
- **Откуда:** war3-model/mdx-m3-viewer MDX.
- **Вход:** animation clips + time; **выход:** bone matrices/particles.
- **Зависит:** `wc3-core`, `wc3-assetdb`.
- **Фаза:** P2–P4.
- **Тесты:** sampling, loop/global sequences, team color, particle bounds.
- **Риск:** CPU skinning; GPU skinning.

### `wc3-platform`
- **Что:** SDL3 init/video/events/audio/haptics/gamepad; window; Android SDLActivity glue;
  desktop CLI flags; raw native handle for wgpu.
- **Зачем:** единый современный platform layer на всех ОС; touch/pen/gamepad/audio из коробки.
- **Откуда:** SDL3 (zlib), `sdl3`/`sdl3-sys`; SDL3 Android docs.
- **Вход:** OS events; **выход:** normalized events + surface handle + audio device.
- **Зависит:** `wc3-core`.
- **Фаза:** P0–P1.
- **Тесты:** init/window/event smoke on Android/Windows/Linux; lifecycle; audio device.
- **Риск:** Android Gradle/NDK version; SDL3 API churn.

### `wc3-gpu`
- **Что:** wgpu instance/adapter/device/surface; optional SDL3 GPU backend; adapter limits/features;
  device-loss; pipeline cache; GPU info.
- **Зачем:** единый GPU abstraction, Vulkan/Turnip, A/B backends.
- **Откуда:** wgpu/naga; SDL3 GPU docs; libadrenotools.
- **Вход:** window handle/config; **выход:** device/queue/surface.
- **Зависит:** `wc3-platform`.
- **Фаза:** P0–P1.
- **Тесты:** adapter selection, fallback, device loss, backend parity.
- **Риск:** SDL3/wgpu raw-handle integration; driver bugs.

### `wc3-android`
- **Что:** SDL3 Android Gradle project + SDLActivity JNI; SAF storage; lifecycle; Vulkan/Turnip
  loading; permissions; haptics.
- **Зачем:** primary platform.
- **Откуда:** SDL3 Android docs, libadrenotools, android-activity as fallback reference.
- **Вход:** Android events; **выход:** engine lifecycle.
- **Зависит:** `wc3-platform`, `wc3-gpu`.
- **Фаза:** P0.
- **Тесты:** install/launch, rotate/pause, surface loss, Turnip fallback.
- **Риск:** OEM driver/SAF; NDK/SDK mismatch.

### `wc3-drivers`
- **Что:** Turnip driver manager: import/download `.so`, SHA-256, internal storage, select/rollback,
  compatibility list, license notices.
- **Зачем:** rootless custom driver without bundling unknown binaries.
- **Откуда:** libadrenotools API; Mesa/K11MCH1 builds (free).
- **Вход:** user-selected driver file/URL; **выход:** loaded driver handle.
- **Зависит:** `wc3-android`, `wc3-gpu`.
- **Фаза:** P0–P1.
- **Тесты:** hash mismatch, bad ABI, fallback, rollback.
- **Риск:** driver/device compatibility; legal notices.

### `wc3-launcher`
- **Что:** first-run data folder, validate MPQ/CASC, profiles, campaign/map/custom/load/options/help.
- **Зачем:** пользовательский вход; wc3rust/W3AndroidEngine flow.
- **Фаза:** P1–P4.
- **Тесты:** data check, missing files, path changes, profiles.

### `wc3-replay`
- **Что:** header (map/data/engine hash), command log, state hashes, verify/desync diagnostics.
- **Зачем:** determinism, debugging, anti-cheat.
- **Фаза:** P2–P5.
- **Тесты:** bit-exact, corrupted replay, version migration.

### `wc3-net`
- **Что:** lockstep/lobby/checksum/reconnect; LAN/direct only.
- **Зачем:** multiplayer без Battle.net.
- **Фаза:** P6.
- **Тесты:** 2–4 player, latency/loss, desync.

### `wc3-testkit`
- **Что:** fixtures, golden screenshots, MDX/BLP corpus, JASS traces, fuzz, benchmark harness.
- **Зачем:** доказательства, не заявления.
- **Фаза:** P0+.
- **Тесты:** все.

## 3. Dependency graph

```
core
 ├─ formats ── assetdb ── scene ── render ── ui
 ├─ input ── touch
 ├─ sim ── path
 │   ├─ jass ── triggers
 │   └─ ai
 ├─ audio
 ├─ anim
 ├─ replay
 └─ net
platform (SDL3) → gpu → render; android/drivers → gpu; launcher depends on render/input/sim
testkit depends on all
```

## 4. Narrative (почему так)

RTS — это не «рендер + юниты». Это **детерминированная симуляция**, поверх которой
**асинхронный рендер** и **touch-ввод**. Мы разделяем их жёстко: sim не знает про GPU,
render не меняет мир, touch не лезет в правила. Это даёт replay, lockstep, device-loss recovery
и тестируемость. Форматы и asset DB — единственная точка чтения user data, поэтому мы можем
подменить MPQ на CASC или loose files без изменения игры. JASS/триггеры — отдельный sandbox,
чтобы карты не могли ломать движок. UI/launcher — отдельные слои, чтобы touch-first не
превращался в «портированную мышь».
