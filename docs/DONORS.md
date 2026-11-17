# DONORS — byte inventory, matrix, adopt/reject decisions

Дата: 2026-11-12. Метод: shallow clone (`--depth 1`) 21 GitHub-донора + локальные артефакты
Herofolends; файловый/байтовый инвентарь, лицензии, README, механизмы с file:line.
Код доноров **не исполнялся**. Сырые данные: `analysis/donor-inventory.json`,
`docs/DONORS_MATRIX.md` (автогенерация).

## 1. Сводная матрица

| донор | язык | файлы | байты | source | лицензия | commit | роль |
|---|---|---:|---:|---:|---|---|---|
| Herofolends (APK+data) | C++/Urho3D + Java | 5 артефактов | 1 579 379 635 | 0 | Urho3D MIT + StormLib MIT + BLPConverter MIT; ассеты proprietary | — | UI/архитектурный референс; исходников нет |
| WarsmashModEngine | Java/libGDX | 2 177 | 9 098 644 | 2 123 | MIT | `f9e0aeed4b` | доменная модель/JASS/данные |
| W3AndroidEngine | Java/libGDX | 3 133 | 16 145 369 | 3 067 | MIT | `a865a38905` | **главный behavioral donor**: Android/touch/CASC/Reforged |
| WarsmashAndroidPort2022 | Java/libGDX | 1 018 | 4 526 290 | 993 | MIT | `d1788bdca3` | исторический Android-эксперимент |
| War3Net | C#/.NET | 1 881 | 95 055 226 | 1 586 | MIT | `5f6f7bac26` | format/toolkit/test-vector donor |
| mdx-m3-viewer | TypeScript | 555 | 5 187 221 | 461 | MIT | `2ff0bc00c6` | MDX/BLP/MPQ/W3X parser donor |
| war3-model | TypeScript | 84 | 821 411 | 63 | MIT | `52d7f421a5` | MDX/BLP round-trip test donor |
| HiveWE | C++/Qt | 494 | 10 682 820 | 142 | MIT | `cd6d61a8d3` | editor/data-model donor |
| StormLib | C/C++ | 379 | 3 655 073 | 318 | MIT | `116135c27e` | MPQ reference/test oracle |
| CascLib | C/C++ | 125 | 97 054 555 | 90 | MIT | `6b889ec8f4` | CASC reference/test oracle |
| XenosRecomp | C++ | 18 | 113 378 | 11 | MIT | `990d03b28a` | X360 shader→HLSL→SPIR-V track |
| libxemit | C | 22 | 7 533 321 | 10 | unknown | `3660b70443` | Xenos shader research only |
| libadrenotools | C/C++ | 43 | 541 293 | 29 | BSD-2-Clause | `8fae8ce254` | rootless Turnip/custom driver |
| android-activity | Rust | 128 | 2 264 791 | 65 | MIT | `b4ddf059b7` | Rust Android glue (fallback ref) |
| OpenRA | C# | 4 163 | 64 118 593 | 1 772 | MIT | `b4d95a56` | RTS architecture/UX comparison |
| **SDL3** | C | 2 385 | 54 260 557 | 1 668 | **Zlib** | `4ca1b1c2d0` | **platform layer** (window/input/touch/audio/Android) |
| **wgpu** | Rust | 2 710 | 38 273 680 | 1 959 | **Apache-2.0/MIT** | `c92258fcae` | **Vulkan renderer** |
| parry | Rust | 501 | 3 507 237 | 482 | Apache-2.0 | `b3c3584ce9` | collision/geometry |
| kira | Rust | 160 | 4 939 976 | 142 | Apache-2.0 | `b0325713c2` | audio |
| sdl3-sys-rs | Rust | 1 493 | 82 483 828 | 1 122 | unknown (verify) | `a04bef22c5` | SDL3 FFI bindings (research until license clarified) |

> Note: размеры включают тестовые данные/ассеты репозитория (например, CascLib 97 MB,
> War3Net 95 MB); это не размер рантайма. `source` — файлы с расширениями кода/конфигов.

## 2. Donor cards

### D0. Herofolends (local, abandoned port)
- **Что:** APK public alpha 0.0.16, Urho3D 1.x + SDL2 + Lua 5.1/AngelScript; UI: MainMenu,
  SelectRaceScreen, SelectmapScreen, InGameMenu, ChangeResources; `Plane.mdl` placeholder.
- **Данные:** `data*.bin` — FreeArc AES-256/CTR, пароль не в APK; `setup.exe` — Inno Setup
  «Warcraft 3», не passworded; ISO-образы `Warcraft III.mdf`/`THEFROZENTHRONE.mdf` содержат
  `War3.mpq`, `setup.mpq`, movies.
- **Сильные стороны:** понятный touch-ориентированный UI-каркас; выбор расы/карты; смена пути
  к ресурсам; StormLib/BLPConverter подтверждают форматный слой.
- **Слабые:** нет исходников движка; APK — лаунчер; данные зашифрованы; Urho3D/GLES и
  Lua/AngelScript — не наш стек; проект заброшен.
- **Берём:** UI-flow (race → map → game), экран ChangeResources, идею «engine without own
  resources», проверку наличия данных на старте.
- **Не берём:** код/бинарники Urho3D, зашифрованные данные, чужие ассеты, Lua/AngelScript.
- **Риск:** IP/ассеты; не распространять; только user-owned data.

### D1. WarsmashModEngine (Java/libGDX)
- **Что:** чистый JC2-движок WC3: `core/src/.../parsers/jass/Jass2.java` ~10 530 строк,
  `datasources/CascDataSource.java`, `MpqDataSource.java`, `GameUI.java`, `fdf/*`, `w3x`-парсеры.
- **Сильные:** наиболее полный JASS2/триггеры, CASC+MPQ, FDF/UI, рендер/симуляция, MIT.
- **Слабые:** JVM/GC, LibGDX GLES, Java-архитектура, производительность на телефоне, нет
  нативного Vulkan/Turnip; FDF/UI завязан на libGDX.
- **Берём:** доменную модель (unit/ability/order/trigger), семантику JASS2, data-source
  abstraction, структуру FDF/UI, тест-кейсы; порт на Rust.
- **Не берём:** Java-код целиком, LibGDX renderer, JVM-рантайм.
- **Лицензия:** MIT — совместима; сохранить copyright.

### D2. W3AndroidEngine (Warsmash Reforged, Android) — **primary donor**
- **Что:** продолжение Warsmash для Windows/Android: Java 17/libGDX, CASC/extracted data,
  JASS2, OpenGL ES shaders, Classic/HD/Reforged graphics sets, campaign/save/load/cinematics,
  determinism/replays, touch + S Pen.
- **Ключевые файлы:** `core/src/com/etheller/warsmash/input/touch/RtsTouchAdapter.java`,
  `TouchInputRouter.java`, `GestureRecognizer.java`, `TouchSettings.java`, `VirtualCursor.java`,
  `PalmFilter.java`, `CascDataSource.java`, `WarsmashGdxMapScreen.java`.
- **Сильные:** лучший открытый Android/touch UX для WC3; CASC 1.31+/Reforged; launcher;
  graphics sets; заявка 30 FPS weak → 120 FPS fast; MIT.
- **Слабые:** JVM/GC, LibGDX GLES, Java heap/loading, нет Vulkan/Turnip, Android UI native views
  смешаны с libGDX.
- **Берём:** **touch translator architecture** (pointer router → gesture recognizer → RTS adapter →
  synthetic mouse events), gesture table, long-press/aim/ghost building, Shift overlay, palm
  filter, DPI thresholds, mode-locking; launcher/data-check flow; CASC/extracted abstraction;
  graphics-set concept; determinism/replay acceptance criteria.
- **Не берём:** Java/LibGDX runtime; копирование кода; GPL? (MIT — можно, но мы делаем clean-room
  Rust-реализацию с собственными тестами).
- **Риск:** поведение touch сложное; нужна device lab и тесты жестов.

### D3. WarsmashAndroidPort2022
- **Что:** исторический Android-порт/пародия; LibGDX/gdx-liftoff; не развивается.
- **Берём:** Gradle/Android packaging, lessons about GLES context, screen lifecycle; не архитектуру.
- **Слабые:** не production; нет Reforged/touch depth.

### D4. War3Net (C#)
- **Что:** полный .NET toolkit: Build, CodeAnalysis.Jass/Decompilers/Transpilers, IO.Mpq/Casc/Slk,
  Modeling, Rendering, Replay, Runtime, Runtime.Api.*.
- **Берём:** форматные схемы, JASS AST/транспиляцию, replay/Map/MPQ API, тест-векторы,
  структуру `War3Net.Build.Core` (MapFactory, UnitObjectData, RandomUnitTable).
- **Не берём:** .NET runtime/рендер; код можно использовать как reference (MIT).
- **Сильные:** широчайшее покрытие форматов и JASS; отличный источник тест-кейсов.

### D5. mdx-m3-viewer (TypeScript)
- **Что:** MDX/M3/W3M/W3X/BLP1/MPQ1/SLK/INI/DDS parsers + JASS2 types + viewer.
- **Берём:** parser semantics, edge cases, тестовые модели, JASS2 type model; MIT.
- **Слабые:** JS/WebGL, не runtime; M3/BLP1 only.

### D6. war3-model (TypeScript)
- **Что:** MDL/MDX parse/generate, BLP1→PNG, DDS, Reforged versions 800/900/1000/1100.
- **Заявка:** 100% classic+Reforged models parse; 99.7% (8753/8773) byte-identical round-trip.
- **Берём:** feature matrix, round-trip test methodology, MDX chunk coverage, team color/
  replaceable IDs/ribbon/particle/PBR details; MIT.
- **Слабые:** BLP1 only; no BLP0/2; no encoder; no Light nodes/priority.

### D7. HiveWE (C++/Qt)
- **Что:** world editor: `src/file_formats/mdx`, `base/map`, `trigger_editor`, `model_editor`,
  `render`, `asset_manager`; Qt/ImGui.
- **Берём:** editor data model, MDX/map/trigger editor concepts, test maps; MIT.
- **Не берём:** Qt/ImGui desktop UI; editor не runtime.

### D8. StormLib (C/C++)
- **Что:** MIT MPQ reference: `SFileOpenArchive`, `SFileReadFile`, listfile/encryption.
- **Берём:** MPQ semantics, test oracle, edge cases (protected maps, listfile, compression).
- **Не берём:** C API как есть; в Rust — собственный крейт/FFI только для тестов.

### D9. CascLib (C/C++)
- **Что:** MIT CASC reader: `CascOpenStorage`, `CascOpenFile`, root handlers (WoW/Diablo/OW/TVFS/Text).
- **Берём:** CASC semantics, BLTE/encoding keys, test oracle; W3 1.30+.
- **Не берём:** весь C++ код; Rust port/FFI test oracle.

### D10. XenosRecomp (C++)
- **Что:** Xbox 360 Xenos shader container → HLSL → DXC → DXIL/SPIR-V; designed for Unleashed
  Recompiled; incomplete features documented (dynamic indexing, integer constants).
- **Берём:** container/instruction documentation, control-flow lowering, constant buffer layout,
  validation strategy; MIT.
- **Не берём:** как runtime для WC3 PC (WC3 не X360); не ждать turnkey.
- **Риск:** incomplete; only one game validated.

### D11. libadrenotools (C/C++, BSD-2-Clause)
- **Что:** rootless custom Adreno driver loading (Turnip), BCn textures, shader dumps, file redirect,
  turbo, user memory mapping; Android 9+ arm64; legacy packaging requirement.
- **Берём:** `adrenotools_open_libvulkan` integration, driver dir in internalDataPath, turbo,
  validation; critical for Turnip.
- **Не берём:** assume Turnip always works; fallback to system Vulkan mandatory.

### D12. android-activity (Rust, MIT)
- **Берём:** NativeActivity/GameActivity glue, lifecycle, JNI; the base for Rust Android app.
- **Не берём:** demo-only patterns; integrate with wgpu surface.

### D13. OpenRA (C#, MIT)
- **Берём:** RTS architecture: World/Actor/Trait, Orders, Network, Replay, Graphics/WorldRenderer,
  Map/FileFormats; comparison for ECS/traits and renderer separation.
- **Не берём:** C# runtime; OpenRA not WC3.

### D14. libxemit (C, license unknown)
- **Что:** Xenos shader emitter, low-level.
- **Use:** research only until license verified; do not link.

### D15. SDL3 (libsdl-org/SDL, Zlib)
- **Что:** cross-platform window/input/touch/pen/gamepad/haptics/audio; SDL3 GPU API; Android
  `SDLActivity` + Gradle (SDK 36+/NDK r28c+/min API 21).
- **Берём:** platform layer на всех ОС; SDL3 events как единственный источник ввода; SDL3 GPU
  как optional secondary backend; Android lifecycle.
- **Не берём:** SDL2 legacy; SDL_Renderer как основной 3D renderer.

### D16. wgpu (Apache-2.0/MIT)
- **Что:** Rust WebGPU-style API, Vulkan backend, WGSL/naga, validation, surface from raw handle.
- **Берём:** primary renderer; adapter/device limits, pipeline cache, device loss.
- **Риск:** Android surface/lifecycle integration; version churn.

### D17. parry / kira / sdl3-sys-rs
- **parry:** collision/geometry (Apache-2.0) — narrowphase/broadphase, no full physics lock-in.
- **kira:** audio mixing/streaming (Apache-2.0) — use with symphonia decode.
- **sdl3-sys-rs:** FFI bindings; **license not detected** in the cloned tree → research only,
  do not ship until clarified. Prefer `sdl3` crate/official SDL3 C library with zlib.

## 3. Что мы берём в архитектуру (сводно)

| слой | донор | что именно |
|---|---|---|
| Android touch | W3AndroidEngine | Router → GestureRecognizer → RtsTouchAdapter → synthetic mouse; long-press/aim/ghost; Shift overlay; palm filter; DPI thresholds |
| Data sources | Warsmash/W3AndroidEngine | `DataSource` abstraction: CASC, MPQ, loose files; prefix/aliases |
| JASS2 | Warsmash/W3AndroidEngine/War3Net | lexical/AST/VM semantics; differential tests; native registry |
| Formats | mdx-m3-viewer/war3-model/War3Net | MDX/MDL, BLP1, SLK, MPQ, W3M/W3X, DDS |
| MPQ/CASC | StormLib/CascLib | reference semantics + test oracle |
| Editor/data | HiveWE/War3Net.Build | map/object/trigger data model |
| Renderer | (none directly) | our wgpu/Vulkan; OpenRA separation as architecture reference |
| RTS loop | OpenRA/Warsmash | world/order/trait or ECS, replay, fixed tick |
| Shader/X360 | XenosRecomp/libxemit | separate R&D track |
| Driver | libadrenotools | Turnip loading/fallback |
| Platform | **SDL3** | window/input/touch/pen/gamepad/audio/Android lifecycle |
| Renderer | **wgpu/naga** | Vulkan, WGSL, adapter/pipeline/device loss |
| Android glue | SDL3 Android + libadrenotools | SDLActivity/JNI + Turnip loading |
| Collision/audio | parry/kira | broadphase/narrowphase + mixing |
| UI flow | Herofolends | race → map → game, ChangeResources |

## 4. Лицензии и legal

- MIT: Warsmash, W3AndroidEngine, War3Net, mdx-m3-viewer, war3-model, HiveWE, StormLib, CascLib,
  XenosRecomp, OpenRA, android-activity.
- BSD-2-Clause: libadrenotools.
- libxemit: license unknown — research only, no linking.
- Urho3D MIT, StormLib MIT, BLPConverter MIT in Herofolends; **game assets proprietary**.
- We will not copy GPL/unknown code; preserve MIT/BSD notices; no Blizzard assets/code.

## 5. Donor matrix gaps

- No donor has native Rust/Vulkan/Turnip; that is our differentiation.
- No donor has a complete **Rust** JASS2 + MDX/BLP + MPQ/CASC stack; we must build/port.
- W3AndroidEngine is the closest behavioral reference but Java/GLES; use as oracle, not code.
- Herofolends source absent; only UI/architecture clues.
- XenosRecomp is X360-only; WC3 PC path is MDX/BLP/MPQ/CASC, not Xenos.

## 6. Next actions

1. Read W3AndroidEngine touch classes fully and write Rust `touch` crate tests from its gesture table.
2. Extract `War3.mpq` with StormLib/CascLib CLI/test oracle; validate Rust MPQ parser against file list/CRC.
3. Build MDX/BLP parser against `war3-model`/`mdx-m3-viewer` test fixtures; round-trip tests.
4. Build JASS2 differential harness: same map/triggers in Warsmash vs Rust; compare event traces.
5. Benchmark W3AndroidEngine on device as baseline (cold start, FPS, memory, touch latency).
6. Create `docs/DIFFERENTIATION.md` targets and benchmark protocol.
