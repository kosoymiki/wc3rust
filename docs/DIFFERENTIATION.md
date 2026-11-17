# DIFFERENTIATION — как Herofolends-RS должен превзойти существующие реализации

Дата: 2026-11-12. Это **целевая** матрица (target), не измеренный результат. Базовые
измерения доноров — отдельный benchmark-этап; без цифр мы не заявляем победу.

## 1. Конкуренты и их потолки

| реализация | стек | сильная сторона | потолок |
|---|---|---|---|
| **Herofolends** | C++/Urho3D + Lua/AngelScript | WC3-подобный UI, Android alpha | нет исходников, лаунчер, зашифрованные данные, GLES, заброшен |
| **WarsmashModEngine** | Java/libGDX | самый полный JASS2/CASC/MPQ/UI | JVM/GC, GLES, не Android-first, нет Vulkan/Turnip |
| **W3AndroidEngine** | Java/libGDX | Android touch/S Pen, CASC, Reforged, determinism | JVM/GC, GLES, Java heap, нет нативного Vulkan/Turnip |
| **War3Net** | C#/.NET | форматный toolkit/JASS AST/replay | не runtime-движок, нет 3D-рендера |
| **OpenRA** | C# | зрелая RTS-архитектура | не WC3, нет MDX/BLP/JASS2-совместимости |
| **mdx-m3-viewer / war3-model** | TS | парсеры/round-trip | не runtime, WebGL, ограниченный BLP |
| **XenosRecomp** | C++ | X360 shader→SPIR-V | X360-only, incomplete, не WC3 |

## 2. Наш target-профиль

**Herofolends-RS = Rust-native, Vulkan-first, touch-first, deterministic WC3-compatible RTS
для Android/Windows/Linux, читающий user-owned MPQ/CASC, с Classic+HD/Reforged graphics sets,
JASS2-совместимым runtime, modding API и asset recompilation pipeline.**

| слой | наша реализация | почему сильнее |
|---|---|---|
| Язык | Rust (edition 2024), no GC/JVM | предсказуемая latency, память, безопасность, один бинарь |
| Рендер | wgpu (Vulkan backend) + optional Turnip via AdrenoTools; fallback Adreno | нативный Vulkan, control over pipelines, mobile GPU |
| Платформы | Android arm64 (primary), Windows x64, Linux | шире, чем Java/Android-only |
| Данные | MPQ + CASC + loose, user-owned | Classic и Reforged в одном движке |
| Форматы | MDX/MDL, BLP1, SLK/INI/TXT, W3M/W3X, DDS/TGA, MP3/WAV | полный WC3 PC pipeline |
| JASS2 | Rust VM + differential tests vs Warsmash/W3AndroidEngine | совместимость + скорость + safety |
| Touch | Rust `touch` crate: router→gesture→RTS adapter, DPI thresholds, palm filter, S Pen, haptics | native latency, customization, accessibility |
| UI | wgpu UI + native Android views for launcher/settings | 120 Hz, safe areas, adaptive layout |
| Determinism | fixed 20/30 Hz, ordered commands, replay hash | cross-platform replay/lockstep |
| Asset pipeline | MDX→glTF, BLP→KTX2/ASTC/BC, zero-copy streaming | load time/VRAM, mobile compression |
| Modding | data-driven + sandboxed scripts/JASS + map packages | wider than hardcoded engine |
| Shader/texture R&D | MDX/BLP + XenosRecomp track | future native ports, not just WC3 |
| Engineering | H2 gates/evidence, benchmark harness, legal gate | reproducible, auditable |

## 2.1 Технологический стек (must-have)

| слой | выбор | статус |
|---|---|---|
| platform | **SDL3** (zlib), `sdl3`/`sdl3-sys`, SDL3_image/mixer/ttf | обязателен |
| renderer | **wgpu** + **naga** (Vulkan/WGSL) | обязателен |
| secondary GPU | SDL3 GPU API (SPIR-V/DXIL/MSL) | optional/A-B |
| GPU driver | system Adreno Vulkan + Turnip via **libadrenotools** | optional, free |
| ECS/math | bevy_ecs, glam | обязателен |
| collision | parry | обязателен |
| audio | kira + symphonia | обязателен |
| assets | gltf, image, ktx2, basis-universal | обязателен |
| serialization | serde + postcard/rkyv | обязателен |
| quality | cargo-deny/audit, nextest, proptest, criterion, fuzz, Miri | обязателен |

**Turnip:** Mesa core MIT; Turnip — часть Mesa Freedreno; сборки K11MCH1/AdrenoToolsDrivers
свободно распространяются. Мы делаем driver manager (import/SHA-256/internal storage/rollback),
не хардкодим драйвер и всегда имеем system Vulkan fallback.

## 3. Feature matrix vs donors

| feature | Herofolends | Warsmash | W3AndroidEngine | War3Net | OpenRA | **Herofolends-RS target** |
|---|---|---|---|---|---|---|
| Native Rust | no | no | no | no | no | **yes** |
| Vulkan | no (GLES) | no (GLES) | no (GLES) | no | no (GL) | **yes (wgpu/Turnip optional)** |
| Android-first | partial | no | yes | no | no | **yes** |
| Touch translator | basic | no | strong | no | no | **strong + tests/haptics/accessibility** |
| CASC | StormLib only? | yes | yes | yes | no | **yes** |
| MPQ | StormLib | yes | yes | yes | no | **yes** |
| MDX/MDL | Plane.mdl only | yes | yes | yes | no | **yes + round-trip** |
| BLP | BLPConverter | yes | yes | yes | no | **yes + KTX2/ASTC** |
| JASS2 | Lua/AngelScript | full | full | AST/toolkit | no | **Rust VM + differential** |
| Classic+Reforged | classic only | classic | yes | data | no | **yes** |
| Deterministic replay | no | partial | work | replay | yes | **yes, hash-verified** |
| Modding | scripts missing | yes | yes | maps | yes | **yes + sandbox** |
| 3D renderer | Urho3D | LibGDX | LibGDX | no | 2D/3D | **wgpu PBR/terrain/water** |
| SDL3 platform | no (Urho3D/SDL2) | no (LibGDX) | no (LibGDX) | no | no | **yes, SDL3** |
| Modern Rust libs | no | no | no | no | no | **wgpu/naga/bevy_ecs/parry/kira** |
| Turnip driver manager | no | no | no | no | no | **yes, rootless + fallback** |
| X360 shader track | no | no | no | no | no | **R&D (XenosRecomp)** |
| H2 evidence | no | no | no | no | no | **yes** |

## 4. Что именно «переплюнуть» (измеримые цели)

| метрика | baseline (to measure) | target Herofolends-RS |
|---|---|---|
| Cold start, Adreno 730, classic map | W3AndroidEngine / Warsmash | **< 4 s** (mid), **< 8 s** low-end |
| FPS, 1080p, classic assets | W3AndroidEngine 30–120 target | **60 fps mid**, **120 fps high**, **30 fps low** |
| Frame time p99 | measure | **< 16.6 ms** at 60 fps; no GC pauses |
| RAM, classic map | Java heap baseline | **< 1.5 GB** peak, **< 900 MB** steady |
| APK size (engine only, no assets) | W3AndroidEngine APK | **< 50 MB** arm64 |
| Map load | measure | **< 5 s** classic, **< 10 s** HD |
| Touch→command latency | measure | **< 80 ms** p95 |
| Simulation tick, 1000 units | measure | **< 4 ms** on Adreno 730 CPU |
| Replay hash | W3AndroidEngine determinism work | **bit-exact across Android/Windows** |
| JASS2 compatibility | Warsmash/W3AndroidEngine | **≥ 95% of tested maps**, differential traces |
| MDX/BLP parse | war3-model 100% classic/Reforged claim | **100% of test corpus**, byte-round-trip where expected |
| Crash-free sessions | measure | **> 99.5%** in soak |

Baseline numbers must be measured on the same device/map; no victory claim before that.

## 5. Уникальные преимущества (moat)

1. **Rust + SDL3 + Vulkan/Turnip** — нет JVM/GC, современный platform layer, нативный драйвер,
   контроль памяти/латентности.
2. **Touch-first, not ported** — gesture translator как отдельный тестируемый слой, S Pen,
   haptics, accessibility, настраиваемые пороги; не «мышь, эмулированная тачем».
3. **Deterministic cross-platform** — один и тот же replay/inputs даёт один hash на Android,
   Windows, Linux; основа для lockstep и античита.
4. **Asset recompilation pipeline** — MDX→glTF, BLP→KTX2/ASTC, BLP/MPQ/CASC→runtime cache;
   быстрый старт, меньше VRAM, HD/Reforged.
5. **Classic + Reforged в одном runtime** — переключение graphics sets без второго движка.
6. **Modding + JASS2** — совместимость с картами и триггерами + безопасный sandbox.
7. **Legal clean-room** — user-owned data, no assets in repo, MIT/BSD code; готовность к
   публичному аудиту.
8. **H2 engineering** — gates/evidence/benchmarks/replay corpus; воспроизводимость.
9. **Future-proof R&D** — XenosRecomp/X360 shader-texture track для будущих нативных портов.

## 6. Benchmark protocol (обязателен до заявлений)

- Device: RMX3709 (Adreno 730, Android 16), plus one low-end device.
- Maps: classic 1v1, 4v4, campaign chapter, Reforged HD map.
- Tools: Perfetto/GPU Inspector, `adb shell dumpsys gfxinfo`, custom Rust frame timer,
  `heaptrack`/jemalloc stats, replay hash.
- Runs: cold start ×10, 10-minute soak, thermal throttling, Turnip vs system Vulkan.
- Baselines: W3AndroidEngine APK + Warsmash desktop; same map/data; log versions.
- Acceptance: target table §4; regression gate in CI/nightly.

## 7. Риски, которые могут отменить преимущество

- **Turnip stability** on Adreno 730; system Vulkan fallback must be first-class.
- **JASS2 completeness** — huge surface; differential tests and map corpus required.
- **Scope** — RTS engine is years; phase gates and vertical slices mandatory.
- **Legal** — Blizzard IP/AUP; no assets, no Battle.net, legal review.
- **Performance** — Java/GLES competitors are mature; Rust/Vulkan advantage must be proven.
- **Touch UX** — W3AndroidEngine is strong; we must adopt its best ideas and test on devices.
