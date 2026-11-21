# Agent K — data / gamedata (ledger 607, all-layers push 2026-11-21)

Owner: `crates/wc3-data/**` (only writer). Oracle: the shipped SLK sheets in
`analysis/war3_data` (RoC), `analysis/war3x_data` (TFT) and the full-game
extraction `analysis/war3_full3` (byte-identical copy of the RoC sheets).

## Goal

Prove that the typed game database reads the shipped tables row-for-row and
value-for-value, fix parser/mapping drift, and pin the contract with named tests.

## What was verified

### 1. Row counts — independent parser vs typed database vs sheet bounds

`crates/wc3-data/tools/slk_audit.py` is a second SYLK implementation (positional
`C`/`F` cursor, Y-less row starts, `K` payload to end of line). It also reads each
sheet's own `B;` bounds record — a third, structural witness. Id rows:

| Table | RoC SLK | RoC `counts()` | TFT SLK | TFT `counts()` | `B;` bounds |
|---|---|---|---|---|---|
| UnitData | 466 | 466 | 812 | 812 | 466/812 = exact |
| UnitBalance | 466 | 466 | 812 | 812 | 466/812 = exact |
| UnitWeapons | 316 | 316 | 812 | 812 | 316/812 = exact (TFT buildings carry empty rows, `weapsOn=0`) |
| UnitAbilities | 324 | 324 | 812 | 812 | 324/812 = exact |
| UnitUI | 466 | 466 | 812 | 812 | 466/812 = exact |
| AbilityData | 397 | 397 | 763 | 763 | RoC 399 rows = 397 ids + 2 spacers; TFT 767 = 763 + 4 |
| ItemData | 132 | 132 | 273 | 273 | 132/273 = exact |
| UpgradeData | 58 | 58 | 89 | 89 | 58/89 = exact |
| DestructableData | 128 | 128 | 246 | 246 | 128/246 = exact |
| Doodads | 278 | 278 | 469 | 469 | 278/469 = exact |

Evidence: `evidence-20261121/slk_audit_roc.txt`, `slk_audit_tft.txt`.

Note on the `B;` extents: RoC declares the grid exactly. Two TFT sheets
over-declare unused trailing columns (`UnitData` 31 vs 30 reached, `UnitWeapons`
79 vs 77) and TFT `AbilityData` declares 767 data rows for 763 id rows (4
header-less spacers). The audit therefore requires the parsed grid to fit
*inside* the declared extent and reports the spacer difference; a grid that
exceeds the extent is a failure.

### 2. Source provenance — the sheets are the shipped files

All 10 RoC sheets are **SHA-256 identical** to the copies in the full-game
extraction `analysis/war3_full3` (10 523 files): see
`evidence-20261121/slk_sha256_vs_full3.txt`.

### 3. Spot values — typed accessor vs exact cell (9 tables, both trees)

| Table | Row | Sheet cells | Typed value |
|---|---|---|---|
| UnitData | `hfoo` | `race=human`, `movetp=foot`, `prio=6`, `valid=1` | `UnitDef.race=human`, `food_used=2` |
| UnitBalance | `hfoo` | `HP=420`, `realHP=420`, `level=2`, `goldcost=160` (TFT 135), `lumbercost=0`, `def=2`, `spd=270`, `sight=1400` | hp 420, armor 2, level 2, gold 160/135, lumber 0, speed 270 |
| UnitBalance | `htow` | `type=TownHall,Mechanical`, `unitUI.isbldg=1`, `HP=1500`, `goldcost=450`, `lumbercost=150`, `bldtm=180` | `type_flags=[TownHall,Mechanical]`, `is_town_hall`, `Classification::Building`, hp 1500, gold 450 |
| UnitWeapons | `hfoo` | `dmgplus1=11`, `dice1=1`, `sides1=2`, `rangeN1=90`, `cool1=1.35`, `weapType1=MetalMediumSlice`, `atkType1=normal` | min 12 / max 13, range 90, 1/1.35 aps, types exact |
| UnitAbilities | `hfoo` | `abilList=Adef` (TFT `Adef,Aihn`) | abilities `[Adef]` / `[Adef,Aihn]` |
| AbilityData | `AHhb` | `hero=1`, `levels=3`, `Cost1=75`, `Cool1=5`, `Rng1=800`, `Dur1=0` | hero true, 3 levels, level 1 75 / 5 / 800 / 0 |
| ItemData | `ratc` | RoC `itemClass=ringa`, `Level=7`, `goldcost=500`, `abilList=AItc`, `droppable=1`; TFT `class=Permanent`, `Level=5` | class `ringa`/`Permanent`, level 7/5, gold 500, droppable |
| UpgradeData | `Rhme` | `maxlevel=3`, `goldbase=100`, `goldmod=150` (TFT 75), `lumberbase=50`, `lumbermod=50` (TFT 125), `timebase=60`, `timemod=15` | 100/150, 50/50, 60/15 → levels 100-250-400 g, 50-100-150 l, 60-75-90 s (TFT: 100-175-250 g, 50-175-300 l) |
| DestructableData | `LTlt` | `HP=50`, `armor=Wood`, `targType=tree`, `numVar=10`, `pathTex=PathTextures\4x4Default.tga` | hp 50, armor Wood, model `…LordaeronTree…`, pathTex exact |
| Doodads | `APct` | `category=E`, `file=AshenCatTail`, `defScale=1` | category E, model `…AshenCatTail…` |
| heroes | `Hamg` | `HP=100`, `realHP=450`, `manaN=0`, `realM=285`, `STR=14`, `INT=19`, `AGI=17`, `Primary=INT` | `hp()=100` vs `real_hp()=450`, `mana()=0` vs `real_mana()=285`, hero level-1 pool 450/285, INT 19 |

### 4. Full-table differential — every row, every mapped field

`crates/wc3-data/tools/slk_diff.py` compares the `dump_gamedata sqlite` dump
against the independent SLK reader for **all rows and all 30 mapped columns**:

| Table | RoC rows / mismatches | TFT rows / mismatches |
|---|---|---|
| units (race, hp, mana, level, gold, lumber, food×2, speed, armor) | 466 / 0 | 812 / 0 |
| items (class, level, gold, lumber, charges, stock×3, sellable) | 132 / 0 | 273 / 0 |
| upgrades (max_level, gold/lumber/time base+per-level) | 58 / 0 | 89 / 0 |
| abilities (hero, item, useInEditor, levels) | 397 / 357 pre-fix → 0 post-fix | 763 / 749 pre-fix → 0 post-fix |

Pre-fix evidence: `evidence-20261121/slk_diff_roc_prefix.txt`,
`slk_diff_tft_prefix.txt`. Post-fix: `slk_diff_roc_postfix.txt`,
`slk_diff_tft_postfix.txt`.

## Drift found and fixed

### D1 (parser/mapping) — ability level count came from sheet width, not the row

`AbilityView::max_level()` returned the highest level **column set** present in
the sheet (`Cool1..3` RoC, `Cool1..4` TFT), ignoring the row's own `levels` cell.
Effect: every RoC one-level ability reported 3 levels (357 of 397 rows); every
TFT ability reported 4 (749 of 763 rows), including the 18 `levels=0` engine
pseudo-abilities (`Aatk`, `Amov`, …). `AbilityNumbers.levels` therefore carried
fabricated empty levels, and `HeroSkill.levels`/`HeroAbilityInfo.levels`
over-reported learnable levels.

Fix (`src/abilities.rs`): the `levels` cell is authoritative; the column probe
is kept as `column_levels()` and is only the fallback for a row/sheet with no
parseable `levels` cell. `validate.rs` now warns on schema width (`column_levels
== 0`) instead of on legitimate zero-level rows; `balance.rs` checks
`declared > columns` (a real schema contradiction) instead of comparing the
declared count to a count derived from itself.
The TFT `AHbz` row still shows the behaviour concretely: it declares 3 levels
while the level-4 columns are filled (`Area4=200`); the typed record says 3
levels, the raw cell stays readable through the table.

Tests: `units_data_slk_rows_and_values_match`,
`abilities_data_slk_rows_and_values_match` (full-row differential over all 397 +
763 abilities), `heroes_attributes_hero_pool_columns_are_distinct`,
`economy_lumber_slk_rows_and_values_match`, `items_data_slk_rows_and_values_match`,
`upgrades_data_slk_rows_and_values_match`, plus the updated
`real_data::ability_levels_come_from_the_real_columns`.

### D2 (test oracle) — `economy_lumber_doodad_defs` depended on an absent donor

The test required `donors/War3Net/tests/…/(2)Ponds_v1.4.w3x`, which is not part
of this checkout (no `donors/` tree), so the whole `wc3-data` suite was red
before any work in this lane. The test now measures the melee map that ships in
this repository (`analysis/war3_map/Maps/Melee.w3x`, in-map name "Justice" by
Blizzard Entertainment, 1v1, 2 902 placed doodads, 80 distinct ids, no
object-data files): all 80 ids resolve to a shipped
destructable/doodad definition; 5 destructables carry `texFile=_` (texture in the
MDX: `LTba LTcr LTrc YTpb YTpc`); 21 ambient doodads carry `pathTex=none/_` and
are pinned as such instead of being asserted into a fake pathing texture.

### D3 (tooling hygiene) — donor paths in examples / skips

`examples/map_override_probe.rs` and `examples/wts_probe.rs` hardcoded the
absent War3Net map directory. `map_override_probe` now takes the map as an
argument and prints the exact next step (including the `dump_gamedata
--object-data … --wts …` path for extracted maps) instead of panicking;
`wts_probe` defaults to the repository's own map trees.
`tests/data_coverage.rs::wts_and_object_data_override_the_profile_text` keeps its
donor fixture as the preferred oracle but its skip message now names the
repo-shipped equivalent (`war3_map/Human04`, covered by
`base_contract::map_overrides_resolve_through_the_typed_lookup` and
`real_data::object_data_merges_into_the_typed_tables`).
Three pre-existing `wc3-data` warnings (unused `hero.rs` import, needless
`mut` in `coverage.rs`, dead `FamilyAudit::warn`) were removed.

### Deliberate divergences (documented, not defects)
* `UnitDef.hp/mana` expose the raw `HP`/`manaN` cells; for heroes that is the
  level-1 base and the per-level gain (`Hamg: 100 / 0`). The computed pool is
  exposed separately as `real_hp()/real_mana()` (`Hamg: 450 / 285`) and the hero
  record uses those (`hero.hp_at(1)=450`, `mana_at(1)=285`).
* `WeaponSlotDef.weapon_type` is `weapTp1` (normal/missile/instant);
  `UnitWeapons.weapType1` (`MetalMediumSlice`) is the weapon *sound set* and is
  only reachable as a raw cell, not as the typed weapon type.
* `realHP`, `realM` and `realdef` cells carry spreadsheet formula remnants after
  the number (`450;ERC[-1]+IF(...)`); the typed readers use the engine's `atof`
  semantics and the parity test compares the leading number, while the raw text
  stays available.
* TFT `UnitWeapons` has one row per unit (812); buildings carry empty rows with
  `weapsOn=0`. Row-count parity therefore means 812 for every unit sheet.
* TFT `AbilityData` declares 767 rows for 763 ids (4 header-less spacer rows).

## Artifacts

* `crates/wc3-data/tests/data_slk_parity.rs` — 6 named tests (independent parser
  + full-row level differential).
* `crates/wc3-data/tools/slk_audit.py` — independent SYLK reader/auditor
  (row counts, `B;` bounds, cell probes).
* `crates/wc3-data/tools/slk_diff.py` — full-table differential (SQLite dump vs
  independent reader).
* `crates/wc3-data/examples/map_probe.rs` — map probe (name, doodads, object data).
* `analysis/agent-data/evidence-20261121/` — raw evidence files.

## Verification

```
$ scripts/h2-cargo test -p wc3-data --offline --no-fail-fast
EXIT=0   139 passed; 0 failed   (12 test targets; evidence-20261121/data_test_run.txt)
  data_slk_parity: 6/6 passed
  re-run of every pre-existing target: 0 failed (the previously red
  data_tables::economy_lumber_doodad_defs now passes)

$ target/debug/dump_gamedata --root analysis/war3_data --tft analysis/war3x_data counts
rc=0  (evidence-20261121/dump_counts.txt)

$ target/debug/dump_gamedata --root analysis/war3_data validate   -> rc=0
$ target/debug/dump_gamedata --root analysis/war3x_data validate  -> rc=0
(evidence-20261121/validate_roc.txt, validate_tft.txt; warnings only:
 ref.ability-missing 6, ref.upgrade-missing 2/1, weapon.avg/max-mismatch 1/1,
 column.artifact 5 — all pre-existing and reported, not fatal)

$ python3 crates/wc3-data/tools/slk_diff.py --db … <root>
units/items/upgrades/abilities: 0 mismatches on RoC and TFT (post-fix, final binary)
```

Post-fix differential files: `evidence-20261121/slk_diff_roc_postfix.txt`,
`slk_diff_tft_postfix.txt`. Sheet audit: both roots `failures=0`
(`slk_audit_roc.txt`, `slk_audit_tft.txt`).

## Result

Row counts: 10/10 tables exact on both trees, matching the sheets' own bounds.
Spot values: 9 tables + hero pools exact against the cells. Full-table
differential: 0 mismatches on units/items/upgrades before the fix and on all four
tables after it. One real mapping drift (ability levels) found and fixed with a
full-row test; one broken test oracle (absent donor map) repaired against the
repo-shipped map.

## Risks / next

* `h2-cargo` starvation was observed during verification: the normal-priority
  `wc3-data` ticket waited 36+ min while newer priority tickets jumped the FIFO,
  after another lane's `wc3-game ui_screens` test spun for 40 min holding the
  slot (it ended only by its own `timeout 2400`). The verification run was
  relaunched with the documented `H2_CARGO_PRIORITY=1` emergency knob (still one
  cargo at a time; 4.9 GB free) so the lane could close. The queue has no
  anti-starvation aging and no hung-holder timeout — an ops item for the tech
  lead (owner: agent O / infra).
* `max_level()` is consumed by `wc3-sim` data generation and hero UI; the change
  removes phantom levels. Sim-side counts that assumed "4 levels for every TFT
  ability" would now see the declared counts — the data-side contract is correct,
  any consumer expecting the old over-count is a consumer bug (owner: agent F).
* `analysis/war3_full3` and `analysis/war3_data` agree byte-for-byte on all 10
  sheets; a future extraction refresh should keep that identity, otherwise
  `slk_audit.py` will report which table diverged.
