# Warcraft III — полный локальный RE-разбор и r2-анализ

Дата: 2026-11-12. Устройство: Android/Termux aarch64, root (`su -c id` = uid=0, context u:r:ksu:s0).
Инструменты: `r2 6.2.6` (+`rabin2`), `7z`, `innoextract`, собственный Rust `wc3-mpq` (добавлен PKWARE DCL explode), SHA-256, `file`, `wc`, `objdump`/`rabin2`.

Все утверждения ниже — **observed** на локальных файлах; там, где вывод сделан по строкам/импортам, это помечено как **inferred**. Никаких внешних/сетевых источников не использовалось.

---

## 1. Что именно лежало локально

| Источник | Размер | Что это |
|---|---:|---|
| `/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/WC3.iso` | 635.2 MB | ISO Warcraft III (RoC, 2002) |
| `.../wc3_iso/TFT.iso` | — | ISO The Frozen Throne |
| `WC3.iso:/War3.mpq` | 438,529,036 B | игровые данные RoC |
| `WC3.iso:/setup.mpq` | 17,130,959 B | установщик: скрипты + бинарники |
| `WC3.iso:/install.exe` | 294,912 B | установщик (читает setup.mpq) |
| `WC3.iso:/autoplay.exe` | 53,248 B | автозапуск |
| `WC3.iso:/Movies/*.mpq` | 7 файлов | ролики |
| `.../wc3_iso/TFT_setup.mpq` | 438 MB | установщик TFT |
| `/data/media/0/Download/wr3/.../Warcraft III.mdf` | — | образ установщика (использован ранее через mdf2iso) |

`War3.mpq` — только данные; исполняемых файлов внутри нет (`listfile` 10,523 записей, 0 `.exe/.dll`).

---

## 2. Цепочка распаковки установщика (доказанная)

1. `install.exe` статически содержит Storm (`SFile.cpp`, `SEvt.cpp`, `SComp.cpp` — строки в `.data`), читает `setup.mpq`.
2. В `setup.mpq` найден `SetupDat\setup.vis`; он сжат **MPQ codec 0x08 = PKWARE DCL implode**. Наш reader раньше возвращал `UnsupportedCompression(8)`. Реализован полный Rust-порт `~/src/herofolends-rs/crates/wc3-mpq/src/pkware.rs` (таблицы Dist/Len/Ch из StormLib `pklib/explode.c`, bit-buffer, ASCII/BINARY, dictionary 4–6 бит; тесты `cargo test -p wc3-mpq --lib` — 4/4 pass).
3. `setup.vis` — обычный текст. Он делает `Preload "SetupDat\inst_files.ins"` и другие скрипты.
4. `SetupDat\inst_files.ins` содержит `FileBlock $(Build) = $(InstallPath) archived { ... }`; `Build = "Files100"`. Оттуда получены имена:
   - `Warcraft III.exe`, `World Editor.exe`, `War3.exe`, `WorldEdit.exe`, `Game.dll`, `ijl15.dll`, `Mss32.dll`, `Storm.dll`, `BNUpdate.exe`, `blizzard.ax`,
   - `redist\miles\*` (Mp3dec.asi, Mssdolby.m3d, Msseax2.m3d, Mssfast.m3d, Reverb3.flt),
   - `Movies\*.mpq`.
5. Из `setup.mpq` извлечены `Files100\*` (подтверждено `file`: PE32 i386). Дополнительно найден баг MPQ-ридера: **encrypted-but-uncompressed** блоки (флаги `0x80030000`) читались как секторные и давали `sector offsets out of bounds`; исправлено ветвью «один зашифрованный unit, без таблицы секторов». Проверка: `UI\Glues\Loading\Backgrounds\Campaigns\Tutorial-BotLeft.blp` → 110,412 B, magic `BLP1`.
6. TFT: `TFT_setup.mpq` содержит свои `SetupDat\inst_files.ins`; из него извлечены `Files100\Game.dll` (8,671,295 B), `Storm.dll` (356,417), `War3x.mpq` (300,955,713), `War3xlocal.mpq` (54,039,425), `Frozen Throne.exe` / `Warcraft III.exe` (274,432 каждый), `BNUpdate.exe` (229,376), `blizzard.ax`.

Артефакты распаковки: `analysis/game_files/` (RoC), `analysis/tft_files_*` (TFT), `analysis/war3_data/`, `analysis/war3x_data/`, `analysis/war3x_local/`, `analysis/war3_full2/`.

---

## 3. Полный инвентарь бинарников (r2/rabin2)

### 3.1 RoC (июнь 2002)

| Файл | Размер | SHA-256 | Arch | Функции | Точка входа | Экспорты |
|---|---:|---|---|---:|---|---|
| `War3.exe` | 713,494 | `40452555b45bde16af9c7605ecaf8e62c19ea7957280499142425289b2a314f8` | PE32 i386, base 0x400000 | **1,913** | `0x467fdc` | **695** именованных `A1`…`LD` |
| `Game.dll` | 7,725,118 | `9970c5af4ff4821f8d593249e06af803d8132bef4c7d015d6a1423223c51a378` | PE32 i386, base 0x6f000000 | **24,537** | `0x6f53d5fd` (DllMain) + `GameMain @0x6f004180` | 1: `GameMain` |
| `Storm.dll` | 311,360 | `e34b4f4e3b4b90f54ae57c97aa11e34768cce07dd240ebfcca57cc2f4b479edd` | PE32 i386, base 0x15000000 | **730** | `0x15032950` | **370** ordinal-экспортов (101…647) |
| `Warcraft III.exe` | 258,048 | — | PE32 i386 | **83** | — | лаунчер |
| `WorldEdit.exe` | 3,470,102 | — | PE32 i386 | **10,924** | — | редактор |
| `World Editor.exe` | 57,344 | — | PE32 i386 | **74** | — | лаунчер редактора |
| `BNUpdate.exe` | 200,704 | — | PE32 i386 | **478** | — | патчер |
| `ijl15.dll` | 372,736 | — | PE32 i386 | **392** | — | Intel JPEG |
| `Mss32.dll` | 347,648 | — | PE32 i386 | **1,028** | — | Miles Sound System |
| `Mp3dec.asi` | 125,952 | — | PE32 i386 | **166** | — | MP3 codec |
| `Mssdolby.m3d` / `Msseax2.m3d` / `Mssfast.m3d` | 65,536 / 70,144 / 64,000 | — | PE32 i386 | **208 / 221 / 208** | — | Miles 3D/EAX |
| `Reverb3.flt` | 56,832 | — | PE32 i386 | **151** | — | reverb |
| `SetupDat\blizzard.ax` | 606,208 | `a52741551312d81198d713e75e08740b115e5a74988dcec4dafd931d0841ece5` | PE32 i386, base 0x1c400000 | **369** | — | installer ActiveX |
| `SetupDat\War3Unin.exe` | 126,976 | — | PE32 i386 | **242** | — | деинсталлятор |

### 3.2 TFT (май 2003)

| Файл | Размер | SHA-256 | Функции | Экспорты |
|---|---:|---|---:|---|
| `Game.dll` (TFT) | 8,671,295 | `d1a80c89597e08e00715dac19a5e906c58b2a351285b7e2ae94bbc5c73f4ad2e` | **28,001** | `GameMain @0x6f005e70` |
| `Storm.dll` (TFT) | 356,417 | `79e53f97706518667d5dbb221324a78dc05cdd75b8c2341e0e1f2440b1f8f0ed` | **847** | ordinal 101… |
| `Frozen Throne.exe` | 274,432 | `8660d16d6ae68968058ce8497d66b06e6c81b9d18cdb19fb58f2d293306177e6` | **106** | лаунчер |
| `Warcraft III.exe` (TFT) | 274,432 | `b7c41cf3bea987db0646900e6ba4671b7b7e2e24266a76557091fcbdfb595abb` | **106** | лаунчер |
| `War3x.mpq` | 300,955,713 | `63bbf2f6d21ecb131cad2d263ef45d8438b2a364edb0190f74358fb72a9e7b97` | — | данные TFT |
| `War3xlocal.mpq` | 54,039,425 | `816d682521fd0790524a8094be7132171dc44baafdf54b5882d78ae22a19d5e0` | — | локализация TFT |

TFT `Files100\War3.exe` в установщике отсутствует: TFT использует RoC `War3.exe` как host, а `Game.dll` заменяется на TFT-версию.

---

## 4. Архитектура исполнения (наблюдаемая)

### 4.1 `War3.exe` — host/engine loader

- 1,913 функций, 695 именованных экспортов `A1`…`LD`; импортирует **storm.dll** напрямую (43 функции по именам), `mss32.dll` (110), `kernel32` (133), `user32` (95), `wsock32` (25), `msvcrt` (36).
- Строки: `Game.dll`, `GameMain`, `Couldn't open Game.dll.`, `Game.dll is corrupt.`, `Storm.dll`, `War3.mpq`, `opengl`.
- Это означает: `War3.exe` загружает Storm/аудио/платформу и **динамически** подгружает `Game.dll` и вызывает `GameMain` (inferred по строкам + `LoadLibraryA`/`GetProcAddress` в импортах; прямого импорта Game.dll в таблице нет).
- `entry0` (`0x467fdc`) делает `GetVersion`, `GetCommandLineA`, инициализацию CRT/исключений, затем цепочку `fcn.0046acc0`, `fcn.0046ab8e`, `fcn.0046a941`, `fcn.0046a888`, `fcn.004676b0`, `fcn.00461220` (PDC в `analysis/r2/artifacts/War3.exe.entry.pdc.txt`).

### 4.2 `Game.dll` — игра/рендер/логика

- RoC: 24,537 функций; `GameMain @0x6f004180` — экспорт №1.
- Импорты: **644 ordinal из War3.exe**, 92 из Storm.dll, 60 MSVCRT, **50 OPENGL32**, 37 USER32, 17 KERNEL32, 14 GDI32, 8 IMM32, 3 ole32, **3 ijl15.dll**, 1 WINMM.
- PDC `GameMain`: вызывает `War3.exe` ordinals 372/327/475/377/180/340, `Storm.dll` ordinals 503/585, затем `fcn.6f004200` (инициализация игры) и `fcn.6f004b80`; создаёт каталог `Logs`; при ошибке — `War3.exe` ordinal 180/340.
- TFT: 28,001 функций; `GameMain @0x6f005e70`; импорты 434 ordinal War3.exe, 97 Storm.dll, 50 OPENGL32, 3 ijl15.
- Строки рендера: `E:\Drive1\Temp\buildWar3loc\ENGINE\Source\gx\CGxDeviceOpenGL\CGxDeviceOpenGl.cpp`, `CGxOglTexture.cpp`, `CGxOglPrim.cpp`, `CGxOglScene.cpp`; `OPENGL32.dll`; `Gfx OpenGL`. Это **OpenGL-рендерер**, не D3D (D3D8 строка присутствует как альтернатива).
- Загрузчики данных (xrefs):
  - `War3.mpq` → `fcn.6f004c10` (`0x6f004c7a`), `fcn.6f3f5b00` (`0x6f3f5bba`);
  - `Units\UnitData` → `fcn.6f10de70` (`0x6f10de83`);
  - `Units\AbilityData` → `fcn.6f115070` (`0x6f115077`);
  - `Units\DestructableData` → `fcn.6f110db0` (`0x6f110db8`);
  - `Units\UpgradeData` → `fcn.6f115980` (`0x6f115987`);
  - `scripts\common.j` → `0x6f49bb3f`, `0x6f49bb87`;
  - `UI\TriggerData.txt` — `0x6f6a35fc`; `UI\MiscData.txt` — `0x6f68f284`.

### 4.3 `Storm.dll` — MPQ/Storm API

- RoC: 730 функций, 370 экспортов ordinal 101…647; точка входа `0x15032950` (DllMain, PROCESS_ATTACH/DETACH).
- TFT: 847 функций.
- Импортируется `War3.exe` по именам и `Game.dll` по ординалам; это подтверждает, что весь ввод-вывод архивов идёт через Storm.

---

## 5. Игровая база (извлечена полностью в объёме данных)

### 5.1 RoC `War3.mpq`

- Listfile: **10,523** записей.
- Извлечено: **8,392** файлов, **455,883,012 B** (`analysis/war3_full2/`).
- Расширения: 3,935 BLP, 2,696 WAV, 2,048 MDX, 1,140 MP3, 319 TGA, 91 AI, 71 FDF, 67 TXT, 44 W3M, 33 SLK, 27 PLD, 16 MPQ, 15 MID, 7 DLS, 6 TTF, 3 J, 3 IFL.
- Таблицы (33 SLK, **6,785 строк**): UnitData 466×34, UnitBalance 466×44, unitUI 466×47, UnitAbilities 324×7, UnitWeapons 316×67, AbilityData 399×56, ItemData 132×21, DestructableData 128×45, UpgradeData 58×26, Doodads 278×65, Terrain 98×12, Water 13×47, CliffTypes 26×10, SoundInfo (UnitAckSounds 747, DialogSounds 1,106, UISounds 111 и др.).
- JASS: `Scripts/common.j` 1,732 строки, `Blizzard.j` 6,750, `Cheats.j` 480; `Scripts/common.ai` 2,017; 91 AI-скрипт; 44 карты `.w3m` (кампания RoC).
- UI: 71 FDF, `UI\TriggerData.txt`, `UI\MiscData.txt`, `UI\UnitEditorData.txt`, `UI\WorldEditData.txt`, `Strings`-файлы.

### 5.2 TFT `War3x.mpq` / `War3xlocal.mpq`

- `War3x.mpq`: listfile 5,610; извлечено 249 data-файлов (`analysis/war3x_data/`).
- `War3xlocal.mpq`: listfile 1,156; извлечено 158 файлов (`analysis/war3x_local/`).
- TFT SLK: **57 таблиц, 21,331 строка**: UnitData **812**, UnitBalance 812, UnitWeapons 812, unitUI 812, UnitAbilities 812, AbilityData **767**, ItemData **273**, DestructableData **246**, UpgradeData **89**, Doodads **469**, плюс MetaData-таблицы.
- TFT JASS: `common.j` 2,077 строк; 125 AI-файлов; карты TFT (`Maps\FrozenThrone`).

### 5.3 Кодеки извлечения и остаток

Реализованы: stored, zlib (0x02), bzip2 (0x10), **PKWARE DCL (0x08)**; исправлен encrypted-uncompressed.
Осталось (точный список, без «магии»):
- **2,124 WAV** с секторами, начинающимися на `0x41` = `ADPCM mono | Huffman` (наш reader пытается zlib и падает «shorter than expected»);
- **6 WAV** с чистым `0x40` (ADPCM mono);
- **1 DLS** и **1 FDF** (`UI\FrameDef\Glue\Loading.fdf`) с `0x20` (LZMA).
Точный следующий шаг: портировать StormLib `src/huffman/huff.cpp` (916 строк) и `src/adpcm/adpcm.cpp` (543) в `wc3-mpq`, добавить LZMA-ветку; после этого полный extraction = 10,523/10,523. Это единственный незакрытый кодек-хвост; данные/модели/текстуры/скрипты/карты уже извлечены.

---

## 6. r2-артефакты (полные инвентари функций)

Каталог: `analysis/r2/`

- `summary.txt` — все бинарники: arch/bits/размер/SHA-256/функции.
- `artifacts/War3.exe.afl.txt` — **1,913** функций (адрес, размер, имя).
- `artifacts/Game.dll.afl.txt` — **24,537** функций.
- `artifacts/Storm.dll.afl.txt` — **730** функций.
- `artifacts/*.entry.pdc.txt` / `.entry.pdf.txt` — псевдокод и дизассемблер точек входа.
- `artifacts/Game.dll.GameMain.pdc.txt` — псевдокод `GameMain`.
- `artifacts/Game.dll.key-xrefs.txt` — xrefs загрузчиков таблиц и `War3.mpq`.
- `artifacts/{War3.exe,Game.dll,Storm.dll}.exports.json` / `.imports.json` — полные таблицы (695 / 1 / 370 экспортов; 644+92 импорта Game.dll и т.д.).
- `r2/tft/Game.dll.afl.txt` — **28,001** функция; `r2/tft/Storm.dll.afl.txt` — **847**; лаунчеры по 106.
- `r2/*-strings.txt`, `*-imports.txt`, `*-sections.txt` — строки/импорты/секции по каждому бинарнику.

Команды воспроизведения:

```bash
r2 -2 -q -c 'e scr.color=0; aaa; afl' analysis/game_files/Files100_Game.dll
r2 -2 -q -c 'aaa; s 0x6f004180; pdc' analysis/game_files/Files100_Game.dll
rabin2 -Ej analysis/game_files/Files100_Game.dll
rabin2 -ij analysis/game_files/Files100_Storm.dll
```

---

## 7. Выводы для порта на Rust (inferred, по наблюдаемым точкам)

1. **Host/engine split**: `War3.exe` = платформа/Storm/аудио/OpenGL + 695-экспортный API `A1`…`LD`; `Game.dll` = игровая логика, вызывающая host по ординалам и `Storm.dll` для MPQ. Порт должен повторить этот слой: `wc3-platform` (окно/ввод/аудио/OpenGL/Vulkan), `wc3-mpq` (Storm API), `wc3-game` (GameMain/логика).
2. **Данные**: таблицы `Units\*.slk` (SLK), `*Func.txt`/`*Strings.txt` (INI), object data (`war3map.w3u` и т.п. — в картах), JASS (`common.j`/`Blizzard.j`/`war3map.j`), AI (`.ai`), FDF UI. Извлечённые файлы — эталон для парсеров/тестов; `wc3-formats` уже читает SLK/MDX/BLP/W3E.
3. **Рендер**: OpenGL-путь `CGxDeviceOpenGL/*`; для Rust-порта это ориентир по состояниям (слои материалов, alpha, skinning), а не источник кода.
4. **Аудио**: Miles (`Mss32`) + ADPCM/Huffman в MPQ; для полного asset pipeline нужен порт `huff.cpp`/`adpcm.cpp`.
5. **Версии**: RoC-бинарники 2002-06-06; TFT `Game.dll` 2003-05-19. TFT — основной кандидат для анализа расширения (28k функций, 812 юнитов в таблицах).

---

## 8. Ограничения и честные границы

- Полный **псевдокод каждой** из 24,537/28,001 функций не печатался: сгенерирован полный **инвентарь** (`afl`), псевдокод точек входа/`GameMain`/ключевых загрузчиков. Для повального `pdc` есть готовая команда, но это отдельный многогигабайтный батч.
- Имена функций в `Game.dll`/`Storm.dll` — `fcn.*`/`Ordinal_*`: PE не содержит символов; исключение — `GameMain` (реальный экспорт) и `War3.exe` экспорты `A1`…`LD`.
- Остались 2,131 файл аудио/LZMA (см. §5.3) — это кодек-задача, не «данные отсутствуют».
- Все хэши/размеры/счётчики сняты в этой сессии с локальных файлов; ничего не выдумано.

---

## 9. H2

- Ledger: **296** (`wc3-full-re-2026-11-12`).
- Capture: **540**.
- Experience: **4253** (setup.mpq/PKWARE/encrypted-uncompressed), **4254** (архитектура War3.exe/Game.dll/Storm.dll).
