# wc3rust

wc3rust — чистая (clean-room) реализация RTS-движка на Rust/Vulkan, которая работает с
собственными данными Warcraft III: Reign of Chaos / The Frozen Throne. Это не форк, не перепаковка
и не обёртка чужого движка: симуляция, рендерер, форматы, HUD и touch-слой написаны здесь на Rust
против легально принадлежащих оператору файлов игры, которые приложение читает из своего приватного
каталога данных во время работы. Основная платформа — Android (Termux, aarch64, SDL3 + wgpu/Vulkan);
host-сборка запускает те же крейты для тестов, инструментов и headless-проверок.

В репозитории нет кода Blizzard, моделей, текстур, карт и звуков. Warcraft — торговая марка Blizzard
Entertainment; проект не аффилирован с Blizzard. Подробно: `docs/LEGAL.md`.

## Требования и инструменты

| Пункт | Значение |
| --- | --- |
| Язык | Rust 1.99.1, edition 2024 |
| Workspace | 30 членов (`crates/`, `apps/`, `tools/`) |
| Платформа | Termux/Android aarch64 (основная), host Linux для тестов/инструментов |
| Графика | SDL3 + wgpu 30 / Vulkan; Turnip через `crates/wc3-adrenotools` |
| Android SDK | `aapt2`, `javac`, `d8`, `zipalign`, `apksigner`, `keytool`, `android.jar` (автопоиск в `$PREFIX/tmp/opencode/android-sdk/android-36/android.jar`) |
| Root | `su` (KernelSU) для установки, миграции данных и logcat |
| SDL3 native | предсобранный `third_party/sdl3-android-build/libSDL3.so` |

`scripts/h2-cargo` — единственная точка входа для cargo на этом устройстве. Он сериализует все
запуски через один глобальный flock-слот, ждёт порог свободной памяти, ставит
`CARGO_BUILD_JOBS=2` и ограничивает тесты через `RUST_TEST_THREADS=2`; лог —
`$PREFIX/tmp/h2-cargo.log`. Два одновременных тяжёлых тестовых бинаря исчерпывают 11 ГБ телефона, и
ядро убивает процесс на середине, поэтому голый `cargo` не используется.

## Сборка, установка, запуск

Host-тесты (два гейта, покрывающих рендерер и terrain-путь):

```sh
scripts/h2-cargo test -p wc3-render --lib --offline
scripts/h2-cargo test -p wc3-game --lib --offline terrain_render::
```

Общий гейт workspace (список членов берётся из `cargo metadata`, задачи выполняются по одной):

```sh
scripts/test-all.sh --check-only     # cargo check --workspace
scripts/test-all.sh                  # check + проверки Android-таргетов + тесты по членам
```

APK для Android — два шага, release-APK попадает в `build/android-p0/release/`:

```sh
scripts/h2-cargo build --release -p wc3rust-android
scripts/build-apk.sh --release
# -> build/android-p0/release/wc3rust-p0.apk
```

Установка и запуск на устройстве:

```sh
su -c 'pm install -r -d build/android-p0/release/wc3rust-p0.apk'
su -c 'am start -n org.wc3rust.p0/org.libsdl.app.SDLActivity'
su -c 'logcat -d -v brief | grep wc3rust'
```

### applicationId и миграция данных

applicationId — **`org.wc3rust.p0`** (`apps/android/app/src/main/AndroidManifest.xml`). Это
единственный пакет, который собирает сборка; все runtime-пути движка указывают на
`/data/data/org.wc3rust.p0/files`.

Установка, сделанная до переименования пакета (`org.herofolends.p0`), мигрируется копированием
каталога данных в новый пакет — без перемещения и без удаления. Старый пакет и его данные остаются
нетронутыми, старый APK продолжает работать:

```sh
su -c 'pm install -r -d build/android-p0/release/wc3rust-p0.apk'
su -c 'mkdir -p /data/data/org.wc3rust.p0/files'
su -c 'cp -a /data/data/org.herofolends.p0/files/. /data/data/org.wc3rust.p0/files/'
su -c 'chown -R u0_a360:u0_a360 /data/data/org.wc3rust.p0/files'   # uid пакета org.wc3rust.p0
su -c 'restorecon -R /data/data/org.wc3rust.p0/files'
```

uid берите из `su -c 'stat -c %u /data/data/org.wc3rust.p0'` — он выделяется при установке. Первый
запуск после этого находит `War3.mpq`, `War3x.mpq`, melee-карту и HUD-пак в новом каталоге.
Чекпойнт, записанный до переименования (`herofolends-resume.sav`), по-прежнему читается, если нового
`wc3rust-resume.sav` нет, а следующее сохранение пишет уже новое имя
(`apps/android-native/src/lib.rs`, `pre_rename_checkpoint`).

### Файлы данных в каталоге приложения

`/data/data/org.wc3rust.p0/files` — корень данных оператора:

```
/data/data/org.wc3rust.p0/files/
  War3.mpq  War3x.mpq  War3xlocal.mpq      собственные архивы игры
  Melee.w3x  Orc01.w3m  Human04.w3m  Maps/ карты
  hud_pack/                               установленный HUD-пак (см. ниже)
  turnip/                                 ICD-дроп Turnip (scripts/setup-turnip.sh)
  ui_atlas.json  ui_atlas.png  ui_atlas_insets.json
  wc3rust-resume.sav  wc3rust-resume.sav.meta
```

ICD Turnip поставляется отключённым (лежит в этом каталоге как `freedreno_icd.disabled`), поэтому
путь по умолчанию — вендорный драйвер Vulkan. Чекпойнт возобновления пишется при уходе приложения
в фон.

## Содержимое репозитория

### Крейты движка

| Крейт | Роль |
| --- | --- |
| `crates/wc3-core` | Детерминированные базовые типы и fixed-point математика |
| `crates/wc3-mpq` | Чтение MPQ-архивов (совместимое с StormLib ядро) |
| `crates/wc3-formats` | MDX, BLP, SLK, W3E/W3I, object data, контейнеры карт |
| `crates/wc3-data` | Типизированная база игровых данных (юниты, способности, предметы, апгрейды) |
| `crates/wc3-path` | Поиск пути по сетке рельефа: A*, flow fields, коллизии |
| `crates/wc3-sim` | Симуляция с фиксированным тиком: ECS, приказы, бой, экономика, способности |
| `crates/wc3-jass` | JASS2: лексер, парсер, интерпретатор, триггеры, реестр нативов |
| `crates/wc3-anim` | Сэмплирование MDX-анимаций, geoset animation, партиклы, ленты |
| `crates/wc3-input` | Touch-роутер, жесты, математика RTS-камеры, маппинг миникарты |
| `crates/wc3-audio` | Декодирование WAV/MP3, пространственный миксер, музыка и 3D-пан |
| `crates/wc3-bink` | Декодер Bink 1 / BinkAudio для кампанийных роликов |
| `crates/wc3-replay` | Формат реплеев, лог команд, контрольные суммы, lockstep-сессия |
| `crates/wc3-assetdb` | Ограниченные кэши, провайдеры ассетов, каталоги, граф зависимостей |
| `crates/wc3-render` | Рендерер wgpu/Vulkan: статическая геометрия, GPU-скиннинг, сортировка по состоянию, UI |
| `crates/wc3-game` | Играбельный слой: приказы, ИИ, HUD, меню, кампания, touch-фронтенд |
| `crates/wc3-adrenotools` | Чистый Rust-порт libadrenotools: загрузка драйверов в namespace |
| `crates/wc3-adrenotools-main-hook` | Интерпозер `dlopen` (libmain_hook.so) |
| `crates/wc3-adrenotools-fopen-hook` | Интерпозер перенаправления `fopen` (libfile_redirect_hook.so) |
| `crates/wc3-adrenotools-gsl-hook` | Интерпозер аллокатора GSL (libgsl_alloc_hook.so) |
| `crates/wc3-vulkan-loader` | Шим загрузчика: AOSP-загрузчик плюс Turnip ICD через драйвер-лоадер |
| `crates/wc3-turnip-vendor` | Шим вендорного драйвера, отдающий полный Vulkan API поверх Turnip ICD |
| `crates/wc3-gpu` | Определение возможностей GPU и выбор адаптера |
| `crates/wc3-platform` | Платформенные сервисы, общие для Android и host |
| `crates/wc3-re` | Слой ресурсов/VM/object data для инструментов реверс-инжиниринга |

`crates/wc3-guest` (модель x86-32 гостя для исследовательского пути статической рекомпиляции) лежит
на диске, но не входит в workspace.

### Приложения и инструменты

| Цель | Роль |
| --- | --- |
| `apps/android-native` | Точка входа Android (libmain.so): окно SDL3, wgpu-поверхность, чекпойнты |
| `apps/android` | Java-оболочка SDLActivity, манифест, ресурсы и дерево упаковки APK |
| `apps/bench` | Host-бенчмарки `wc3rust-bench` и `wc3rust-gpu-bench` |
| `apps/headless` | `wc3rust-headless` — прогон карты на N тиков без окна |
| `apps/smoke` | `p0-smoke` — smoke-тест загрузки |
| `apps/driver-probe` | Проба драйвера Vulkan/Turnip |
| `tools/mdx-gltf` | `mdx-gltf` — экспорт MDX в glTF 2.0 для визуальной проверки |

### HUD-пак ассетов

HUD управляется манифестом; ни одно смещение в пикселях не подбирается вручную. Раскладка пака:

```
manifest.json          # 92 ассета: w/h/insets/states, атлас, safe zones, design height, шрифты
atlas/  commandcard/  console/  controlgroups/  cursor/  fonts/  icons/  menus/
minimap/  portrait/  resources/  selection/  tooltip/  topbar/  docs/  source/  build/
SHA256SUMS.txt
```

Источник истины — `analysis/hudskin/final/wc3rust-hud-final/manifest.json`; установленная копия —
плоский каталог `hud_pack` в files-каталоге приложения. `crates/wc3-game/src/hudskin/pack.rs`
ищет пак в каталоге приложения, в Download оператора и в дереве analysis и падает громко
(`HUD_PACK missing=…`), а не подставляет арт. Спецификация мобильной раскладки, метрики устройства и
замеры — в `HUD_LAYOUT_QA.md`.

### Аудит, тестирование и доказательства с устройства

- `docs/audit/WC3RUST_CODE_AUDIT.md` — единый реестр аудита: каждый пункт имеет `FIXED`,
  `REPRODUCED_NOT_A_BUG` или `BLOCKED`, регрессионный тест и коммит, закрывший пункт.
- `docs/TESTING.md` — процедура тестирования на устройстве и контракт логов (`GAME_FIRST_FRAME`,
  `GAME_DATA`, `GAME_RESUME`, `GAME_CHECKPOINT`).
- `scripts/qa/device_evidence.sh` и `scripts/qa/device_p0_cycle.py` — установка, запуск,
  скриншот и сбор лога с одним JSON-отчётом на прогон.
- `scripts/game-selftest.sh` — запуск, ожидание первого кадра, сбор logcat в один лог.

### Резервные копии

`~/bin/full-backup.sh` и `~/bin/full-restore.sh` снимают и восстанавливают весь рабочий набор
(репозиторий, `~/.config/opencode`, состояние H2, базы); план и правила хранения — в
`~/BACKUP_PLAN.md`.

### Автоматизация устройства (CN-кит)

Автоматизация DeepTesting/CN живёт вне репозитория: `~/deeptesting_proxy` и лаунчер
`~/bin/deeptest-cn` (`~/bin/deeptest-stop`, `~/bin/deeptest-unblock` для управления).

## Управление на телефоне

Touch-first, мышь не требуется: tap выбирает, drag панорамирует камеру, pinch масштабирует,
two-finger tap — это правый клик (умный приказ), long-press показывает тултипы и подтверждает
разрушительные приказы. Клавиатуру заменяют экранные элементы: консольная полоса с кнопками
меню/простаивающего рабочего/контрольных групп, командная карта 3×3, миникарта (tap — переход, drag —
панорама) и чип армии. Минимальный размер целей — 44–48 dp. Полный контракт жестов и зоны раскладки —
в `docs/TOUCH.md`.

## Известные ограничения и как сообщить о дефекте

- UI-поток Android может аварийно завершиться при уходе активности в фон
  (`FATAL … SIGABRT … hwuiTask0`), пока сносится native-поверхность. Защита поверхности покрыта
  `crates/wc3-render/tests/surface_guard.rs`; фикс acquire-freeze — в следующей сборке устройства,
  см. `HUD_LAYOUT_QA.md`.
- Turnip поставляется отключённым (`freedreno_icd.disabled`): по умолчанию используется
  вендорный драйвер Vulkan. `docs/GPU_DRIVERS.md` описывает включение ICD.
- Ролики кампании и часть карт кампании требуют дополнительных архивов оператора; движок
  пропускает их с строкой в логе, а не выдумывает данные.

Чтобы сообщить о дефекте, приложите: срез logcat (`su -c 'logcat -d -v brief | grep wc3rust'`),
скриншот из `scripts/qa/device_evidence.sh`, вывод `su -c 'pm dump org.wc3rust.p0'` и точный коммит
(`git rev-parse HEAD`). `docs/TESTING.md` перечисляет строки, которые делают отчёт пригодным к работе.
