# rust-anything × wc3rust — the parity loop on every layer

Date: 2026-11-21. Repo: `~/src/herofolends-rs` (`~/src/wc3rust` symlink). Parent ledger 592.
Engine: `scripts/port_parity.py` + `scripts/port-parity.sh`. All-layer gate: `scripts/port-parity-all.sh`
(wired into `scripts/test-all.sh`). OpenCode2 plugin: `~/.config/opencode/plugin/rust-anything.js`
(tools `rust_anything_{status,record,diff,gate}` + context guidance). Skill:
`~/.config/opencode/skills/rust-anything/` (upstream SKILL.md + H2 addendum + lock.json).

## What the tool is

`github.com/erkamyaman/rust-anything` (@98adbe2, MIT) is a **behaviour-parity loop for ports**:
`profile → analyze → bridge → record → diff → bench → shadow → gate`. Its value is not code
generation — it is **ground truth**: the reference implementation runs first, its outputs are
recorded as data (`cases.jsonl`), and the port is only accepted when every recorded case replays
N/N and the gate passes. A mismatch is a port bug; editing the recording to hide it is forbidden.

On this device the upstream CLI has no Termux/android-arm64 build, so H2 runs the same contract
through `scripts/port_parity.py` (own engine, same semantics: record/diff/gate/status/retire).
The plugin exposes it as OpenCode2 tools so the rule is enforced in every session, not by memory.

## Why it matters for wc3rust

wc3rust ports Warcraft III behaviour to Rust; the "original" is the retail client + shipped data
+ buildable donor C++. Without a recording, "ported" is an opinion. With a recording it is a
number: `cases=1/1 match, 0 mismatches` + `gate PASS` (recording present, no `unimplemented!`,
cargo check green). It also catches the class of regressions we just hit by hand — a layout or
data change that silently moves output is a red diff on the next run.

What it cannot catch (honest limits, from upstream `how-it-works.md`): behaviour the recording
never exercised — timing, GC effects, float order, regex-engine differences. That is why each
layer is recorded with real shipped inputs, and device-timing rows stay on device evidence.

## Where it applies — layer matrix (all green 2026-11-21)

`PORT_PARITY_ALL_PASS units=11` (`scripts/port-parity-all.sh`, rc=0).

| Layer | Unit | Reference command (recorded) | Cases |
|---|---|---|---|
| game/menu layout | `menu-layout` | `--example menu_layout_report` (5 viewports, incl. 2414x1080) | 1/1 |
| game/campaign | `campaign-load` | `--example campaign_report -- --mission Prologue01 --frames 20` (inline `WC3RUST_DATA`) | 1/1 |
| sim/melee census | `sim-census` | `--example map_census -- analysis/war3_data/Maps/Campaign/Orc01.w3m` | 1/1 |
| sim/abilities | `ability-class` | `--example ability_classification` (763 rows / 428 exec / 335 data-only) | 1/1 |
| anim/geoset | `anim-geoset` | `--example geoset_alpha -- …/Assassin.mdx` | 1/1 |
| audio/events | `audio-events` | `--bin audio_event_table` | 1/1 |
| formats/maps | `formats-census` | `--example map_corpus_audit -- analysis/war3_data/Maps` | 1/1 |
| mpq/codecs | `mpq-codec` | `--example codec_census -- A.mpq B.mpq` | 1/1 |
| assetdb/catalog | `asset-catalog` | `--bin asset_catalog -- --extracted analysis/war3_data` (timing normalised) | 1/1 |
| input/router | `input-touch` | `--example touch-trace -- --check …/box-select.trace` | 1/1 |
| **vs original game** | `original-parity` | `python3 scripts/original_parity.py` | 1/1 |

Each unit is self-contained: paths are in the command, no ambient env required (the one
env-dependent recording was replaced and its old case **retired** with a reason — kept on disk).

### Blocked layers (exact reason + next step, no silent gaps)

| Layer | Status | Reason | Next step |
|---|---|---|---|
| game/HUD layout | blocked | `--example hud_layout` prints `HUD_LAYOUT_GATE FAIL` (pre-existing) | fix the HUD gate, then record `hud-layout` |
| render/terrain | blocked | `terrain_edge` rc=101 (needs GPU adapter) | record from device frame (`GAME_TERRAIN_*`) or headless GPU runner |
| data/coverage | unblocked input, not yet recorded | RoC `War3.mpq` extracted 2026-11-22 at `~/.local/share/wc3-install/War3.mpq` (see below) | record `data_coverage` against the real archive |
| re/manifest | blocked | `wc3re manifest` build+scan too slow for a parity replay, args need tuning | add `--root/--archive` fast path, record with `--frames`-bounded scan |
| guest/PE | resolved 2026-11-21 | operator PE samples exist under `analysis/game_files` / `analysis/tft_files_*` | `original-parity` `guest/pe` is APPLICABLE (15 files, independent PE32 parser) |

## How to run

```sh
# every layer (the gate; also runs inside scripts/test-all.sh unless PORT_PARITY=0)
bash scripts/port-parity-all.sh

# one unit
bash scripts/port-parity.sh record <unit> --cmd "<reference command>"
bash scripts/port-parity.sh diff   <unit> --json
bash scripts/port-parity.sh gate   <unit> --json
bash scripts/port-parity.sh retire <unit> <case-id> --reason "why it no longer replays"
```

OpenCode2 tools (same engine, repository auto-resolved through the `wc3rust` symlink):
`rust_anything_status`, `rust_anything_record`, `rust_anything_diff`, `rust_anything_gate`.
Verified end-to-end: `rc=0 104ms`, 10 units, last_diff/last_gate per unit.

## H2 rules (unchanged, now enforced across layers)

- A port unit is done only when replay is N/N and the gate passes (`gate.port.parity-loop`).
- Never edit `cases.jsonl` to hide a mismatch; intended differences go to `known-diffs.jsonl`
  with a reason (`gate.port.no-fixture-edit`). Retirement is explicit, timestamped and audited.
- A recording is self-contained: data paths live in the command, not in the caller's env.

## Сверка с оригинальной игрой — `original-parity` (2026-11-21)

`python3 scripts/original_parity.py` → `ORIG_PARITY_SUMMARY applicable=19 failures=0` (rc=0).
Оракул — сама игра: StormLib-извлечённое дерево `analysis/war3_full3` (10 523 файла), её
listfile, SLK/MDX/BLP/PE, 44 карты кампании, `Scripts/common.j` и `UI/SoundInfo/*.slk`.
Где возможно, оригинал парсится **независимым** маленьким Python-парсером (SYLK-ячейки,
PE32, MPQ через `mpyq`, SHA-256 извлечённых байтов), а не нашим же Rust.

Инструменты один раз собираются `scripts/h2-cargo build` (по одному вызову на крейт) и
запускаются напрямую: на общем устройстве 15 агентов `cargo run` в каждом чеке упирался в
очередь сборки. Если крейт не собирается, его проверки **FAIL** — устаревший бинарь из
`target/debug` не принимается.

| Слой | Проверка | Результат |
|---|---|---|
| mpq/inventory | listfile vs извлечённое дерево | 10 523 = 10 523, missing=0 extra=0 (псевдо-записи `(attributes)`/`(listfile)` исключены) |
| formats/slk | `slk_info` rows vs независимый парсер ячеек | 4 таблицы, mismatches=0 (26/98/13/19) |
| anim/mdx | `geoset_alpha` vs `VRTX`-блоки в `GEOS` | 3 модели, mismatches=0 (Assassin 4, Footman 5, GreatHall 19) |
| render/blp | `blp_info` vs заголовок BLP1 | 5 файлов, w/h и magic совпали |
| campaign/maps | `campaign_report chapters` vs файлы кампании | 44 = 44 |
| jass/natives | `native_status` vs `native … takes` в common.j | оригинал 791, registry 1665, **missing=0**, extra=874 (документированный superset) |
| formats/maps | `map_corpus_audit` vs фактические карты | 44 = 44 |
| jass/signatures | независимый декодер vs `native_sig_dump` | 0 mismatches |
| render/blp | независимый BLP1-декодер vs `blp_hash` SHA-256 | 8/8 идентичны |
| formats/slk | независимые SYLK-ячейки vs `slk_dump` | 137 строк, 0 mismatches |
| **audio/soundinfo** | `UI/SoundInfo/*.slk` (7 таблиц, 2530 def) vs `audio_event_table` | 33 события, 1 документированный gap, 0 mismatches |
| **audio/voices** | `Units/unitUI.slk` + `UnitAckSounds.slk` vs `unit_sound_sets` | 8 юнитов, 0 mismatches, все файлы голосов на месте |
| **sim/units** | `UnitBalance.slk`+`UnitData.slk` vs `dump_gamedata sqlite` | 466 юнитов × 10 полей, 0 mismatches |
| **sim/weapons** | `UnitWeapons.slk` vs `dump_gamedata unit` | 316 строк, сэмпл 30 юнитов (stride 11), 0 mismatches |
| **formats/map-contents** | `mpyq` + SHA-256 vs `map_files` | 44 карты, 620 presence, 548 байт-в-байт |
| **formats/map-bytes** | `war3_data/Maps` vs `war3_full3/Maps` SHA-256 | 44 = 44, 0 mismatches |
| **guest/pe** | независимый PE32-парсер vs `pe_inspect` | 15 PE (RoC+TFT), секции/exports/imports 0 mismatches |
| **mpq/real-archive** | `compare_oracle --mpq <retail> --oracle war3_full3` | 10 523/10 523 matched, 0 mismatched, 0 read_fail, 710 907 954 байт |
| **mpq/codec-census** | `codec_census <retail>` | files=10 523, failures=0, decoded 710 907 954, 7 кодек-комбинаций |

### Граница «нет единого `War3.mpq`» снята — RoC (2026-11-22)

Оператор нашёл установочный образ; retail-архив извлечён и подключён к гейту.

* Образы: `/storage/emulated/0/Download/wr3/Warcraft_install/Warcraft_install/{Warcraft III.mdf,THEFROZENTHRONE.mdf}` (Alcohol MDF).
* Геометрия RoC: stride сектора **2448** байт (2352+96 subchannel), данные с **+16**; `War3.mpq` на LBA **110071**, размер **438 529 036**.
* Архив: `~/.local/share/wc3-install/War3.mpq` (magic `MPQ\x1a`, listfile 10 523, blocks 10 560).
* `compare_oracle --mpq <real> --oracle analysis/war3_full3` → `RESULT files=10523 matched=10523 mismatched=0 read_fail=0 extra_in_archive=0 bytes=710907954` (rc=0): каждый файл, прочитанный нашим MPQ-ридером, байт-в-байт равен независимо извлечённому StormLib-дереву.
* `codec_census <real>` → `files=10523 failures=0 decoded_bytes=710907954`; гистограмма: stored 4122, zlib 3655, смешанные (stored/zlib + adpcm-mono/stereo + huffman) 1316, adpcm-комбинации 25+35+93+2532.
* Проверки встроены в `original_parity` как `mpq/real-archive` и `mpq/codec-census`: в свежем клоне без архива обе честно уходят в **NOT_APPLICABLE** с причиной и путём, куда положить файл (`$WC3_MPQ` или `~/.local/share/wc3-install/War3.mpq`).
* Время: `compare_oracle` ≈ 3 мин 54 с, `codec_census` ≈ 1 мин 35 с на этом устройстве (SHA-256/декод 710 МБ) — учитывается в бюджете гейта.

**Оставшаяся граница (TFT):** `TFT_setup.mpq` извлечён (`~/.local/share/wc3-install/TFT_setup.mpq`, magic ok), но у него **нет listfile** (names=0), поэтому имена файлов внутри неизвестны; извлечение `War3x.mpq` из TFT-образа — отдельная задача со своим owner'ом, гейт на неё не блокируется.

NOT_APPLICABLE (честная граница, не «пропущено»): input/router (платформенный слой, эталона в игре нет),
audio/layered (нужен слоёный War3/War3x install), render/terrain (нужен GPU; device-кадры).
`guest/pe` — APPLICABLE: сэмплы оператора (`analysis/game_files`, `analysis/tft_files_*`) на месте;
без них чек честно уходит в NOT_APPLICABLE.

Юнит записан в parity-петлю (`original-parity`), поэтому дрейф относительно оригинала теперь
валит общий гейт: `PORT_PARITY_ALL_PASS units=11`.

## Байтовые циклы (2026-11-21) — разбор → тест → дебаг → фикс

| Цикл | Что сравнивалось | Результат | Действие |
|---|---|---|---|
| 1. JASS-сигнатуры | 791 объявление `native … takes … returns` в `Scripts/common.j` vs `native_sig_dump` (registry) | **2 расхождения**: `GroupAddUnit`, `GroupRemoveUnit` — игра: `returns nothing`, registry (donor 1.32.9): `boolean` | **исправлено**: `shipped_return_override()` в `registry.rs` + регрессионный тест; повторная сверка 0 mismatches |
| 2. MDX-структура | 3 модели: geosets (VRTX в GEOS), sequences (SEQS/132), textures (TEXS/268) vs `geoset_alpha`/`mdx_probe` | 0 расхождений (4/5/19 geosets; 13 seqs; 3/15 textures) | проверка закреплена в `original-parity` |
| 3. BLP-пиксели | 8 палитровых BLP1: sha256(RGBA) нашего декодера vs независимый Python-декодер | **8/8 совпали** | проверка закреплена |
| 4. SLK-значения | 137 строк из Water/CliffTypes/Terrain: id и все непустые значения vs независимый парсер | **0 расхождений** | проверка закреплена |
| 5. Звук (события) | 7 таблиц `UI/SoundInfo` (2530 def) vs `audio_event_table` | все 33 playable-события ссылаются на def и файл из shipped-таблицы; 1 gap `deposit_lumber` (нет cue в игре) | проверка закреплена |
| 6. Звук (голоса) | `unitUI.slk` + `UnitAckSounds.slk` vs `unit_sound_sets` | 8/8 наборов совпали, 19–22 файла на набор присутствуют | проверка закреплена |
| 7. Статы юнитов | `UnitBalance`+`UnitData` (466 строк) vs `dump_gamedata sqlite` | 4660 сравнений, **0 расхождений** (hp/mana/level/gold/lumber/food/speed/armor/race) | проверка закреплена |
| 8. Оружие | `UnitWeapons` (316 строк) vs `dump_gamedata unit`, сэмпл 30 | min/max/range/cool/atkType/weapTp — **0 расхождений** | проверка закреплена |
| 9. Контейнеры карт | `mpyq` (независимый MPQ) vs `map_files`, 44 карты | 620 совпадений присутствия, 548 файлов байт-в-байт (остальные зашифрованы/сжаты — только presence) | проверка закреплена |
| 10. Байты карт | `war3_data/Maps` vs `war3_full3/Maps` SHA-256 | 44/44 идентичны | проверка закреплена |
| 11. Guest PE | независимый PE32-парсер vs `pe_inspect`, 15 файлов | machine/base/size/entry, секции, exports (имя/ординал/RVA), imports (модуль/символ) — **0 расхождений** | проверка закреплена |
| 12. Retail-архив (RoC) | `compare_oracle --mpq War3.mpq --oracle war3_full3` | **10 523/10 523 matched, 0 mismatched, 0 read_fail**, 710 907 954 байт | проверка закреплена (`mpq/real-archive`) |
| 13. Кодеки retail-архива | `codec_census War3.mpq` | files=10 523, **failures=0**, decoded 710 907 954, 7 комбинаций | проверка закреплена (`mpq/codec-census`) |

Инструменты цикла: `scripts/native_sig_compare.py`, `scripts/blp_pixel_compare.py`,
`scripts/slk_value_compare.py`, `scripts/original_parity.py` + примеры `native_sig_dump`,
`blp_hash`, `slk_dump`, `map_files`, `pe_inspect`, `audio_event_table`, `unit_sound_sets`,
`dump_gamedata`, `compare_oracle`, `codec_census`; всё встроено в `scripts/original_parity.py`
(`applicable=19 failures=0`).

Известное pre-existing (не относится к циклам): 2 теста `save::tests::version_5/6…`
падают из-за незакоммиченных правок в `crates/wc3-sim/src/save.rs` (167 строк, чужой WIP);
проверено, что падают и без нашего фикса. Next: владелец save.rs завершает WIP.
