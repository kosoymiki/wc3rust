# TFT content ingest — the game now runs on the TFT content set

Ledger 408 · 2026-11-15 · repo `~/src/herofolends-rs` · agent-tft layer

## Goal

The operator's install contains the TFT archives, but the app only shipped RoC:
the data layer reported "TFT text absent (war3xlocal.mpq missing)". Verify the
archives, install them into the app install root, make the loader layer them,
and prove that TFT rows resolve through the same reader the game uses — with
RoC-only behaviour preserved when the expansion is absent.

## 1. Archive identity (observed)

| file | size (B) | sha256 | MPQ |
|---|---|---|---|
| `analysis/tft_files_Files100_War3x.mpq` | 300 955 713 | `63bbf2f6d21ecb131cad2d263ef45d8438b2a364edb0190f74358fb72a9e7b97` | v0, sector shift 3, 32 768 hash / 5 634 block entries |
| `analysis/tft_files_Files100_War3xlocal.mpq` | 54 039 425 | `816d682521fd0790524a8094be7132171dc44baafdf54b5882d78ae22a19d5e0` | v0, sector shift 3, 4 096 hash / 1 167 block entries |

Both are `MPQ\x1a` version 0 with a `(listfile)` and `(attributes)` entry
(read with StormLib `tools/storm_probe`, the independent oracle).

Listings: `analysis/agent-tft/war3x_listing.txt` (5 612 entries),
`analysis/agent-tft/war3xlocal_listing.txt` (1 159 entries).

TFT tables inside `War3x.mpq` (`FILE <path> <size>`; sizes vs the RoC
`War3.mpq` listing in `analysis/war3_mpq_listing.txt`):

| table | War3x.mpq (TFT) | War3.mpq (RoC) |
|---|---|---|
| `Units\UnitData.slk` | 298 132 | 197 885 |
| `Units\ItemData.slk` | 120 981 | (extract 42.1 K) |
| `Units\UnitBalance.slk` | 561 950 | — |
| `Units\UnitWeapons.slk` | 789 242 | — |
| `Units\UnitAbilities.slk` | 76 839 | — |
| `Units\unitUI.slk` | 479 966 | 282 528 |
| `Units\AbilityData.slk` | 595 229 | 537 348 |
| `Units\CampaignUnitFunc.txt` | 45 179 | 17 700 (extract) |
| `Units\UnitMetaData.slk` | 61 878 | — |
| `UI\SoundInfo\AbilitySounds.slk` | 15 201 | 13 734 |
| `UI\SoundInfo\AnimLookups.slk` | 31 123 | 19 985 |

Path correction to the mission text: the ability sheet is `Units\AbilityData.slk`
(not `Abilities\AbilityData.slk`) and the sound tables are `UI\SoundInfo\*.slk`
(not `UI\SoundInfo.slk`). Measured, not assumed.

`War3xlocal.mpq` carries the localisation layer: 80 `Units\*Strings.txt`
(`HumanUnitStrings`, `ItemStrings`, `NeutralUnitStrings`, `CampaignUnitStrings`,
…), 38 `UI\FrameDef\*.fdf`, the TFT melee maps (`Melee_V0\*.w3x`) and 904 unit
voice `.wav`. It is the Russian localisation; `[Hblm] Name=Чародей Крови`.

## 2. Install into the app root (observed)

`/data/data/org.herofolends.p0/files/` now holds, next to the existing RoC
`War3.mpq` (438 529 036 B):

| file | size | owner/mode/context | sha256 |
|---|---|---|---|
| `War3x.mpq` | 300 955 713 | `u0_a354:u0_a354 600 u:object_r:app_data_file:s0` | `63bbf2f6…7b97` |
| `War3xlocal.mpq` | 54 039 425 | `u0_a354:u0_a354 600 u:object_r:app_data_file:s0` | `816d6825…d5e0` |

Procedure (`/data/data/com.termux/files/usr/tmp/opencode/h2-tft-install.sh`):
copy under `su` to `.<name>.tmp-<pid>` in the same directory → `sync` →
sha256 of the temp file vs the source → `chown`/`chmod`/`chcon --reference=War3.mpq`
→ `mv -f` (atomic rename) → sha256 of the final path. Both files report
`INSTALLED … sha=<source sha>`, ownership and SELinux context identical to the
RoC archive the game already reads.

Disk: 4 316 580 KiB → 3 969 544 KiB free (Δ 347 036 KiB = 339 MiB, exactly the
two archives). No duplicate copies left behind.

## 3. Loader layering

`StormFs::open_wc3_install` (`crates/wc3-re/src/storm.rs`) already listed
`War3xlocal.mpq` / `War3x.mpq`, but only exact-case names. It now:

- resolves every canonical name case-insensitively (`find_install_archive`) so a
  `war3x.mpq` on a case-sensitive Android data dir is still found;
- mounts every remaining `War3*.mpq` after the canonical layers
  (`extra_war3_archives`) — but **only when the root already mounted a canonical
  archive**, and best-effort, because a loose extracted tree carries LOD asset
  packs named `War3_low.mpq` / `War3_med.mpq` (88 MDX entries, 5–6 MB each).
  Mounting one would make the caller skip its loose tree and lose every sheet.
  This was caught by `crates/wc3-re/tests/runtime_wiring.rs`
  (`UnitData through StormFs: NotFound`), fixed, and pinned by
  `storm::tests::loose_tree_asset_packs_are_not_mounted_as_install_archives`.

`crates/wc3-game/src/assets.rs` needed no layering change: both `open_asset_db`
and `open_object_database` already mount `open_wc3_install(root)` where
`root = <install>/War3.mpq.parent()`, so the expansion archives layer
automatically once they exist.

## 4. Defect found and fixed: mixed code page in the TFT localisation

With `War3xlocal.mpq` mounted, `GameDatabase::load` failed:

```
Format { path: "UI\\MiscUI.txt", source: Format(Corrupt("text table is not UTF-8")) }
```

`open_object_database` swallows that into `objects_failed=…` and returns `None`,
so the game would have silently dropped back to compiled-in tables — the whole
point of the install defeated. Root cause (byte-level): the Russian
`UI\MiscUI.txt` in `War3xlocal.mpq` is 158 UTF-8 lines and **one** Windows-1251
line (`QuickSaveFile=Быстрое сохранение`, 6 418 B, first bad byte at 469).
A whole-file decode cannot represent that.

Fix in the install seam (`StormSource::read`, `crates/wc3-game/src/assets.rs`):
`decode_install_text` decodes line by line — UTF-8 where valid, ANSI code page
(CP1251, table in the same file, undefined `0x98` → `U+FFFD`) where not — and
only for text extensions (`txt/slk/fdf/toc/ai/j/ini`), so binary assets are
never touched. Verified by the TFT gate test.

## 5. Measured deltas (same reader, same harness)

`assets.tft-archives-layer` (`crates/wc3-game/src/assets.rs`,
`mod tft_archives`), run as root against the app root because the data dir is
`u0_a354`-only:

```
install_root=/data/data/org.herofolends.p0/files
roc:   layers=["War3.mpq"]                                            units=466 items=132 abilities=397 ability_names=393 ability_icons=322 unit_names=465 sounds=1423 sound_tables=8 missing_tables=0
tft:   layers=["War3xlocal.mpq","War3x.mpq","War3.mpq"]               units=812 items=273 abilities=763 ability_names=742 ability_icons=654 unit_names=812 sounds=2025 sound_tables=8 missing_tables=0
install: layers=["War3xlocal.mpq","War3x.mpq","War3.mpq"]             units=812 items=273 abilities=763 ability_names=742 ability_icons=654 unit_names=812 sounds=2025 sound_tables=8 missing_tables=0
result=tft units 466->812 items 132->273 abilities 397->763 names 393->742 sounds 1423->2025
```

| metric | RoC-only | TFT | delta |
|---|---|---|---|
| unit rows | 466 | 812 | +346 |
| item rows | 132 | 273 | +141 |
| ability rows | 397 | 763 | +366 |
| abilities with a name | 393 / 397 | **742 / 763** | +349 |
| abilities with an icon | 322 | 654 | +332 |
| units with a name | 465 | 812 | +347 |
| sound definitions | 1 423 | 2 025 | +602 |
| sound tables present | 8 | 8 | — |
| missing tables | 0 | 0 | — |

TFT-only ids resolve, with names coming from `War3xlocal.mpq`:

| id | RoC | TFT |
|---|---|---|
| `nwad` (unit) | absent | `Дух-страж` |
| `Hblm` (unit) | absent | `Чародей Крови` |
| `axas` (item) | absent | `Посох Предков` |
| `arsc` (item) | absent | `Свиток Тайного Знания` |

Row counts are distinct parsed ids, one below the sheet's row count
(813/764/274) because the loader drops a duplicate/blank key — the same
−1 relationship holds on RoC (466 vs 467).

Game-layer confirmation (`Assets::load` on the app root, `GAME_AUDIO` wiring
line and model count):

| | RoC-only | TFT |
|---|---|---|
| `GAME_AUDIO … defs/units` | `defs=1423 units=466` | `defs=2025 units=812` |
| `Assets` models | 469 | 814 |
| `objects` | `true` | `true` |

## 6. RoC-only fallback (kept)

- With the archives absent, `StormFs::open_wc3_install` mounts `War3.mpq` only
  (`layers=["War3.mpq"]`) and the harness reproduces the old numbers exactly
  (466/132/397, 393 names, 1 423 sounds). The gate asserts this branch.
- The engine still falls back to the loose extracted tree when no archive is
  mounted (`crates/wc3-re/src/engine.rs`), which the LOD-pack guard now keeps
  intact — `runtime_wiring::host_launch_mounts_storm_and_runs_map_script` passes.
- A RoC run of the game path was re-measured with the TFT archives parked
  (atomic rename, restored): `defs=1423 units=466 models=469 objects=true`.

## 7. Gates run

| command | result |
|---|---|
| `cargo test -p wc3-game --lib --offline -- assets` | **10 passed**, 0 failed (incl. `assets::tft_archives::tft_archives_layer`) |
| `cargo test -p wc3-formats --offline` | **71 + 1 + 4 passed**, 0 failed |
| `cargo test -p wc3-re --offline -- storm` | 6 passed (incl. 2 new), 0 failed |
| `cargo test -p wc3-re --offline` (whole crate) | **82 + 5 + 1 + 2 + 3 passed**, 0 failed; doctests 0 passed/0 failed |
| `runtime_wiring::host_launch_mounts_storm_and_runs_map_script` | passed after the LOD-pack guard |
| app-uid read (`su 10354`, the game's own uid) | `head -c 4` on both installed archives → `M P Q \032`, `uid=10354(u0_a354)` |

The first whole-crate run reported `E0463 can't find crate for wc3_data` in the
doctest step; that was a concurrent rebuild of `crates/wc3-mpq/src/lib.rs` by
another agent replacing a dependency rlib mid-run (mtime matched the run).
Re-running the doctests alone: `0 passed, 0 failed, rc=0`.

### Standalone probe (added because the shared tree is under concurrent edits)

`analysis/agent-tft/probe` (`cargo run --offline --release -- [root]`) mounts
`StormFs::open_wc3_install` and reads through `wc3-data`/`wc3-assetdb` only — no
game/sim crates — so the content layer stays verifiable while other layers are
refactored. Run on the live app root (as root):

```
layers=["War3xlocal.mpq","War3x.mpq","War3.mpq"] listfile=16825
rows units=812 items=273 abilities=763 ability_names=742 ability_icons=654 unit_names=812 missing_tables=0
sounds defs=2025 tables=8 layers=["War3xlocal.mpq","War3x.mpq","War3.mpq"]
id nwad name=Some("Дух-страж") cyrillic=true
id Hblm name=Some("Чародей Крови") cyrillic=true
id axas name=Some("Посох Предков") cyrillic=true
id arsc name=Some("Свиток Тайного Знания") cyrillic=true
RESULT tft-content-ok
```

Same probe on a staged RoC-only root (symlink to the ISO `War3.mpq`):

```
layers=["War3.mpq"] listfile=10523
rows units=466 items=132 abilities=397 ability_names=393 ability_icons=322 unit_names=465 missing_tables=0
sounds defs=1423 tables=8 layers=["War3.mpq"]
id nwad name=None ... id arsc name=None
RESULT roc-only-fallback
```

### Concurrent-edit note (not this layer)

While closing, other agents edited `crates/wc3-sim/src/jass/natives.rs`,
`crates/wc3-sim/src/lib.rs`, `crates/wc3-render`, `crates/wc3-game/src/render.rs`
(added `uv_set`/`uvs2`) and additively edited `crates/wc3-game/src/assets.rs` at
05:32 (my `decode_install_text`, `CP1251` table and `mod tft_archives` survived
intact). Re-running the `assets` filter against that moving tree currently stops
in `wc3-sim`, not in this layer; the gate passed twice on consistent trees and
the probe passes on current sources.

### Disk

`target/` had grown to 24 G on a 3.8 G-free partition, so builds began failing
with `No space left on device`. Reclaimed with
`cargo clean -p wc3-game -p wc3-re -p wc3-render` (14 576 files, 9.9 GiB) after
the guard refused a raw `rm -rf`; free space 8 MB → 10 GiB at the time.
Builds since use `CARGO_INCREMENTAL=0` because concurrent cleans removed the
incremental directory mid-build.

## 8. Open item found (pre-existing, not this layer)

`crates/wc3-game/tests/playable_loop.rs::playable_loop_justice` fails at
`1 GATHER gold 1000 -> 1000`. It fails **identically on RoC-only** (archives
parked, `defs=1423 units=466 models=469`), so it is not a TFT regression.
Owner: sim/data layer (393/401/394). Exact repro:
`su -c "TMPDIR=… target/debug/deps/playable_loop-* --nocapture"` on the app
root; next step is the Orc `Harvest` phase machine in
`crates/wc3-sim/src/lib.rs:5326` (`ToMine` → `pos.distance(mine) <=
gather_gate(mine, 56.0)`), independent of the content layer.

## Files

- `crates/wc3-re/src/storm.rs` — `WC3_INSTALL_ORDER`, case-insensitive
  `find_install_archive`, `extra_war3_archives` + archive-install guard, 2 tests.
- `crates/wc3-game/src/assets.rs` — `decode_install_text` + CP1251 table wired
  into both `StormSource::read` impls; `mod tft_archives` gate test.
- `analysis/agent-tft/probe/` — standalone `tft_probe` (prints `RESULT
  tft-content-ok` / `roc-only-fallback`).
- `analysis/agent-tft/war3x_listing.txt`, `war3xlocal_listing.txt`,
  `local/` (extracted strings used for byte-level checks).

## Operator commands

```bash
# content layer, no game crates needed
cd ~/src/herofolends-rs/analysis/agent-tft/probe
cargo run --offline --release --                      # app root, needs root
cargo run --offline --release -- <roc-only-root>      # fallback check

# gate
cd ~/src/herofolends-rs
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test -p wc3-game --lib --offline -- assets
```
