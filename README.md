# wc3rust — clean-room WC3-compatible RTS engine for Android

Проект: **wc3rust**. Статус: **Phase 0 — P0 core + Rust-порт libadrenotools**:
Rust workspace + **SDL3 3.5.0** + **wgpu 30.0.1/Vulkan** triangle на **Turnip** (Mesa 26.2.6),
StormLib открывает `War3.mpq` (10 525 файлов), извлечены и протестированы MDX/BLP/SLK фикстуры;
8 тестов, clippy/fmt clean. `crates/wc3-adrenotools` — полный Rust-порт
libadrenotools (namespace loader, hooks, KGSL, BCn), 27 тестов, on-device probe
0 failures; Turnip грузится через hook/ICD (Adreno 725). P0 remaining: SDL3
window/surface в Android APK (NDK), baseline W3AndroidEngine, legal sign-off.
Полный план/доки готовы.
Это не форк и не перепаковка чужого кода: это новая Rust/Vulkan-реализация RTS-движка,
совместимого с пользовательскими данными Warcraft III (RoC/TFT), с touch-first управлением
и опциональным рендером через Turnip.

> **Правовая рамка:** в репозитории нет и не будет кода Blizzard, моделей, текстур, карт,
> звуков или иных игровых ассетов. Пользователь подключает свою легальную копию данных.
> Это не аффилированный с Blizzard проект; Warcraft — торговая марка Blizzard Entertainment.
> Подробно: `docs/LEGAL.md`.

## Что это значит для проекта

1. Чужой порт полезен как **архитектурный и UX-референс**, но не как кодовая база:
   Urho3D + Lua/AngelScript + StormLib + BLPConverter + самописный RTS-слой.
2. Наш стек: **Rust + SDL3 + wgpu/Vulkan + Turnip (через libadrenotools)**, touch-first,
   deterministic simulation, clean-room asset pipeline. SDL3 — обязательный platform layer
   (window/input/touch/pen/gamepad/audio/Android); wgpu — основной renderer; SDL3 GPU API —
   optional secondary backend; Turnip — free Mesa driver, user-selectable с fallback.
3. `data*.bin` — **зашифрованные FreeArc** (AES-256/CTR). Пароль/ключ не в APK; installer
   не passworded, но архивный ключ встроен в скрипт/выдаётся `ISDone`. Мы **не ломаем**
   шифрование: нужен пользовательский пароль/запуск оригинального installer на Windows
   или легально извлечённые данные. Это Phase 0 gate, не «потом».

## Технологический baseline

- **SDL3** (zlib) — обязательный platform layer: window/input/touch/pen/gamepad/haptics/audio,
  Android `SDLActivity`; SDL3 GPU API — optional secondary backend.
- **wgpu + naga** (Vulkan/WGSL) — основной renderer; **Turnip** (Mesa MIT) через
  **libadrenotools** (BSD-2) — optional rootless driver с system Vulkan fallback.
- **bevy_ecs**, **glam**, **parry**, **kira**, **symphonia**, **gltf**, **image**, **ktx2**,
  **basis-universal**, **serde/rkyv/postcard**, **tracing**, **tokio/quinn**.
- Quality: `cargo-deny`, `cargo-audit`, `nextest`, `proptest`, `criterion`, `cargo-fuzz`,
  Miri/sanitizers, reproducible builds, SBOM.

## Документы

| файл | зачем |
|---|---|
| `docs/PLAN.md` | полный инженерный план P0–P6, критерии, риски, тесты, H2-гейты |
| `docs/ARCHITECTURE.md` | слои, потоки данных, renderer/sim/audio/input, Android/Turnip, determinism |
| `docs/MODULES.md` | карта модулей: что, зачем, входы/выходы, зависимости, фаза, нарратив |
| `docs/TOUCH.md` | touch-first UX: экраны, жесты, command card, камера, доступность |
| `docs/ASSETS.md` | MPQ/CASC, MDX/BLP/SLK, glTF/KTX2, shader/texture recompilation, Xbox 360 track |
| `docs/GPU_DRIVERS.md` | Turnip/libadrenotools driver manager, fallback, compatibility matrix |
| `docs/DIFFERENTIATION.md` | feature/performance matrix и цель «переплюнуть всех» |
| `docs/LEGAL.md` | clean-room, IP, лицензии, запрет на ассеты Blizzard |
| `diagrams/*.svg` | архитектура, модули, asset pipeline, roadmap |
| `docs/ADRENOTOOLS_RUST_PORT.md` | Rust-порт libadrenotools: матрица, дельты, тесты, probe |
| `docs/ENGINE.md` | карта всех крейтов, покрытие и явные границы |
| `scripts/safe-test.sh` | одномодульный memory-capped тест-раннер (OOM guard) |
| `docs/TESTING.md` | все уровни тестов: unit, asset, gameplay, visual, device |
| `scripts/game-selftest.sh` | on-device tap self-test + скриншоты + Turnip gate |

## Принципы (non-negotiable)

1. **Clean-room:** новый Rust-код; чужие бинарники/ассеты не коммитятся и не распространяются.
2. **User-owned data:** движок читает данные из папки, которую указал пользователь.
3. **Touch-first:** каждое действие доступно пальцем; мышь/клавиатура — enhancement.
4. **Deterministic sim:** фиксированный tick, replay, lockstep-ready.
5. **Vulkan-first:** wgpu (Vulkan backend) + опциональный Turnip через AdrenoTools; fallback
   на системный Adreno Vulkan.
6. **No stubs/placeholders/TODO:** каждая фаза заканчивается работающим срезом и тестами.
7. **Performance budget:** 60 fps на среднем Android, 30 fps low-end; память/термика под контролем.
8. **H2:** corpus gates, capture/experience/monitor, evidence before claims.

## Текущий следующий шаг (Phase 0 gate)

- [ ] Получить легальный доступ к распакованным WC3-данным (пользовательский пароль/Windows-installer
      либо уже извлечённая папка). Без этого нельзя проверить MDX/BLP/MPQ-парсеры на реальных данных.
- [ ] Собрать `cargo` workspace и первый triangle на Android (wgpu + Vulkan + AdrenoTools/Turnip).
- [ ] Подтвердить формат данных: MPQ (RoC/TFT) vs CASC (Reforged) и набор файлов пользователя.
- [ ] Зафиксировать touch-макет для 6" телефона и 10" планшета.
