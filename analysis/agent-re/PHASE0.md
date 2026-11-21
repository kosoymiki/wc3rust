# PHASE 0 — FULL WC3 PORT EVIDENCE (ledger 350, RE)

Date: 2026-11-13 · model `opencode-go/deepseek-v4.1-flash#max` · root
`su -c id` → `uid=0(root) context=u:r:ksu:s0`.

Mandate: reach Blizzard fidelity on all layers; no procedural placeholders as
final art; full byte-level extraction/port. This round delivers the Phase-0
evidence base and merges the extractor modules with **pub mod + caller + test**
in the same generation.

## 1. Byte manifest — every MPQ file

API: `crates/wc3-re/src/manifest.rs` (`ByteManifest` line 59,
`build` line 252). Caller: `wc3re manifest` in
`crates/wc3-re/src/bin/wc3re.rs`. Tests:
`manifest::tests::{sha256_matches_known_vector,detects_formats_from_magic_and_extension,json_escapes_control_and_quotes,manifests_a_real_small_archive}`.

Command (rc 0, 2 min 1 s):

```
wc3re manifest --json analysis/agent-re/phase0/manifest.json \
               --md analysis/agent-re/phase0/MANIFEST.md
```

Result (`manifest.json` / `MANIFEST.md`):

- **18 archives**, **7 451 named files**, **543 237 719 B** extracted payload,
  **0 read failures**, `ok=true`.
- Canonical install: `analysis/tft_files_Files100_War3x.mpq` (5 610 named /
  5 634 blocks / 24 unnamed) and `...War3xlocal.mpq` (1 156 / 1 167 / 11).
- 16 split archives under `analysis/war3_full3` (A…Y, `War3_low/med`), all read.
- Formats: 3 056 BLP, 1 457 MDX, 1 284 MP3, 1 079 WAV, 143 TXT, 129 JASS,
  62 PLD, 59 SLK, 38 FDF, 65 images, 57 binaries, 21 MPQ, 1 TOC.
- Every row carries `sha256`, `format`, `codec`, `parser`, `extractor`
  (`wc3-mpq[zlib] + wc3-re::blp` etc.).
- StormLib 9.40 probe (`tools/storm_probe`) listed 5 612 entries in the install
  MPQ: 5 610 payload names + `(attributes)` + `(listfile)` pseudo-files. The
  manifest covers the 5 610 payload names; the 24 unnamed blocks are reported
  per archive instead of being silently dropped.

## 2. Physics/balance constants — consumed at runtime

API: `crates/wc3-re/src/gameplay.rs` (`GameplayConfig` line 68, `build` line
951). Runtime caller: `crates/wc3-game/src/game.rs:274`
(`GameplayConfig::load` → `sim.apply_gameplay`), with the sim consumer at
`crates/wc3-sim/src/lib.rs:2898` and hero XP at `:2960`/`:2997`.
Tests: `gameplay::tests::{config_loads_typed_values_from_the_install,
parses_real_gameplay_tables,weapon_formulas_match_shipped_columns,
json_and_markdown_have_counts_and_schema}` and
`wc3-sim lib.rs:4523 gameplay_config_loads_from_install_and_drives_hero_leveling`.

Command rc 0: `wc3re gameplay` → `analysis/agent-re/phase0/constants.json`,
`CONSTANTS.md`.

Counts: **812 units**, **812 weapons**, **763 abilities**, **273 items**,
**79 MiscGame constants**. Sources: `analysis/war3x_data/Units/{UnitBalance,
UnitWeapons,AbilityData,ItemData}.slk`, `analysis/war3x_local/Units/MiscGame.txt`.

Formula validation against the shipped columns:

- `avg = dmgplus + dice*(sides+1)/2` → **716/717** shipped `avgdmg*` values match
  (tol 0.01); the one mismatch is preserved in the data, not hidden.
- `dps = avg/cool` → **570/570** shipped `DPS` values match (tol 0.05).
- Hero HP estimate `realHP + (level-1)*HP` for hero rows; hero XP/revive/frost
  formulas from MiscGame comments are in the `formulas` array.
- `GameplayConstants.txt` is **absent in this install** (0 entries); all runtime
  constants come from `MiscGame.txt` and are explicitly typed. This is recorded,
  not fabricated.

## 3. Cutscene/activity inventory — 44 maps

API: `crates/wc3-re/src/cutscenes.rs` (`CutsceneInventory` line 103, `build`
line 585). Caller: `wc3re cutscenes`. Tests:
`cutscenes::tests::{human04_cutscene_files_parse_with_expected_counts,
inventory_covers_all_44_maps,camera_parser_rejects_truncated_payload}`.

Command rc 0: `wc3re cutscenes` → `analysis/agent-re/phase0/cutscenes.json`,
`CUTSCENES.md`.

Totals: **44 maps**, **19 420 script functions**, **4 560 triggers created**,
**2 864 trigger registrations**, **9 981 cinematic calls**, **1 313
transmissions**, **999 cameras**, **4 603 regions**, **1 763 sounds**,
**4 428 WTS strings**, **6 405 script-created units**. Human04 spot check:
13 cameras / 87 regions / 26 sounds / WTG v4 / parse OK.

## 4. Porting matrix

`analysis/agent-re/phase0/PORTING_MATRIX.md`: 26 subsystem rows with
ADOPT/ADAPT/REJECT, current file:line and target file:line in
`wc3-sim`/`wc3-game`. Procedural paths are explicitly **REJECT as final**
(`assets.rs:1612`, `terrain_render.rs:1058`, `mobile_hud.rs:596`); real map
runtime is the target (`game.rs:722`).

## 5. Toolchain audit / installs / 3D pipeline

`analysis/agent-re/phase0/TOOLCHAIN.md` + `analysis/agent-re/toolchain-install.log`.

- Present: assimp 4.5.1, gltf-transform 4.5.1, trimesh 5.2.1, pygltflib 1.16.5,
  three r187, ImageMagick 7.1.2-35, optipng 7.9.1, pngquant 3.0.3, ffmpeg 9.0.2,
  sox 14.8.1, radare2 6.2.6, StormLib 9.40 (+ `tools/storm_probe`).
- Installs this round: `pkg install -y assimp` rc 0, `pip3 install --upgrade
  pygltflib` rc 0, `npm install -g three` rc 0.
- MDX→glTF check: Footman (101 969 B) → **rc 0**, 5 geosets / 505 vertices /
  304 triangles; `trimesh` loads 5 geometries; `assimp info` rc 0;
  `gltf-transform inspect` rc 0 only with `NODE_OPTIONS=--max-old-space-size=2048`
  (default Node heap OOM is recorded as a tooling constraint).

## Merged modules / callers / tests

| Module | pub mod | Caller | Test |
|---|---|---|---|
| `wc3_re::manifest` | `lib.rs` | `wc3re manifest` | 4 unit tests (real A.mpq) |
| `wc3_re::gameplay` | `lib.rs` | `game.rs:274` → `sim.apply_gameplay` | 4 unit + sim test `lib.rs:4523` |
| `wc3_re::cutscenes` | `lib.rs` | `wc3re cutscenes` | 3 unit/integration tests (44 maps) |

## Commands / rc

| Command | rc |
|---|---:|
| `cargo check -p wc3-re --all-targets --offline` | **0** |
| `cargo test -p wc3-re --lib --offline` | **0** (74 passed) |
| `cargo check -p wc3-game --lib --offline` | **0** |
| `cargo test -p wc3-sim --lib ...gameplay_config_loads...` | **0** (1 passed) |
| `wc3re manifest` (full, 18 archives, 7 451 files) | **0** |
| `wc3re gameplay` | **0** |
| `wc3re cutscenes` | **0** |
| `tools/storm_probe analysis/tft_files_Files100_War3x.mpq` | **0** (5 612 listed) |
| `mdx2gltf` Footman → glTF | **0** |
| `gltf-transform inspect` (NODE_OPTIONS 2048) | **0** |

## Honest gaps

- 67 unnamed MPQ blocks across the 18 archives (24 + 11 + 2×16) have no
  listfile name; sizes are exposed by MPQ block metadata only as archive totals.
- `GameplayConstants.txt` is absent from this install; MiscGame supplies 79
  constants, no invented values.
- Campaign maps contain no `war3map.w3q` upgrade overrides (0) and no dialog
  calls (0) in this corpus; reported as zero, not estimated.
- No device/APK visual run: `H2_DEVICE_SLOT=1` was not granted. Offline MDX→glTF
  + trimesh/assimp validation is the visual-check evidence for this round.
- `assimp` CLI reports 4.5.1 while pkg metadata says 6.0.6; recorded as observed.

## H2

- Capture **789**; experience **4453**.
- Ledger **350** closed `completed`, `verification.ok=true`, warnings 0.
