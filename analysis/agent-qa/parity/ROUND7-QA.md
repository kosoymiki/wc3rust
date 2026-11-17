# ROUND7 (QA layer) — verification on the current tree

**Date:** 2026-11-13 19:55 MSK
**Ledger:** 349 (`r2-qa`, owner `scripts/qa/**`, `analysis/agent-qa/**`)
**Packet:** ROUND 7 — verify ROUND6 deliveries, re-run layer gates, rebuild APK, one device cycle, update report.

## Goal

Re-verify QA layer gates on the current tree after the ROUND6 deliveries of the other layers,
fix regressions inside `scripts/qa/**`, rebuild the P0 APK, and produce the acceptance artifact.

## Actions (file:line)

| Change | Location | Why |
|---|---|---|
| Prerequisite chain helper `spawn_prerequisites` | `scripts/qa/parity/src/main.rs:192-236` | the sim now enforces `UnitDef::requires`; the per-def build gate must satisfy the tech tree before placing each building |
| `gate_build` spawns prerequisites | `scripts/qa/parity/src/main.rs:404-424` | `build.all_defs` had regressed to 14/48 (`place_building -> missing requirement`) |
| `gate_lumber` spawns prerequisites for `hkee` | `scripts/qa/parity/src/main.rs:1388-1420,1451-1466` | `economy.lumber` had regressed to 1/3 with `missing requirement` |
| `models.per_def` rewritten to the canonical roster | `scripts/qa/parity.py:344-460` | measures 216 roster definitions → MDX paths and verifies each file on disk (previous gate read the legacy `MODEL_PATHS` array, 4 entries) |
| `models.budget` rewritten to measured rerun | `scripts/qa/parity.py:461-500` | parses `PERF_MEMORY` / `MODELS_PER_DEF_LRU` from the QA rerun log instead of a device-only SKIP |
| `hud.resources` gate added | `scripts/qa/parity.py:514-556` | packet p2 lists the resource bar; gates the three `ResourcePill`s fed from sim state |
| device-blocked handling | `scripts/qa/parity.py:129-172` | when the phone is locked, `maps.device` reports SKIP with the blocked marker instead of FAIL/SKIP silently |
| `final_report.py` | `scripts/qa/final_report.py:1-330` | reproducible report assembly from measured artifacts only (no hand-written claims) |

## Evidence (commands, rc, artifacts)

| Step | Command | rc | Result / artifact |
|---|---|---:|---|
| tree check | `cargo check --offline -p wc3-sim` | 0 | `Finished` (after the sim layer finished its 19:44 edit) |
| release build | `cargo build --release --offline -p herofolends-android` | 0 | `target/release/libmain.so` 19:33:19 (8 701 984 B) |
| release tool | `cargo build --release --offline -p wc3-re --bin wc3re` | 0 | `target/release/wc3re` 19:21:12 |
| APK package | `JAVA_TOOL_OPTIONS='-Xmx512m' bash scripts/build-apk.sh` | 0 | `build/android-p0/herofolends-p0.apk` sha256 `70c15e91e4f5f8d4d33cb510f371c7a4929aaee5b67b8714dc32ce06a1cc245e`, install `2026-11-13 19:38:07` |
| parity gates | `python3 scripts/qa/parity.py --skip-build` | 1 (engine defects) | `analysis/agent-qa/parity/report.json`: 33 gates, **21 PASS / 5 FAIL / 6 GAP / 1 SKIP**, 33/33 matrix-mapped |
| models rerun | `cargo test -p wc3-game --lib models_per_def --offline -- --ignored --nocapture` | 0 | `analysis/agent-qa/parity/models-per-def.log`: `MODELS_PER_DEF required=216 loaded=214 meshless=2 failures=0`, `PERF_MEMORY peak_mb=73.4 budget_mb=1536 pass=true`, `MODELS_PER_DEF_LRU evictions=98` |
| data suite | `cargo test --offline -p wc3-data` | 0 | 20 passed + 1 doctest (`analysis/agent-qa/parity/suite-data-round7.log`) |
| sim suite | `cargo test --offline -p wc3-sim --lib` | 0 | 24 passed (`suite-sim-round7.log`) |
| game suite | `cargo test --offline -p wc3-game --lib` | 0 | 106 passed, 20 ignored (`suite-game-round7.log`) |
| maps/biomes | `python3 scripts/qa/map_biome.py` | 0 | `analysis/agent-qa/r2/report.json`: maps **44/44 clean**, units errors 0; biomes **111/111**, 15 tilesets |
| device cycle | `python3 scripts/qa/device_cycle.py --apk build/android-p0/herofolends-p0.apk` | 1 | **BLOCKED**: secure keyguard. `device-blocked.json`, `STATUS-ROUND7.md`, `cycle-blocked-shade.json` |
| report | `python3 scripts/qa/final_report.py` | 0 | `analysis/agent-qa/final/report.json`: **14 PASS / 0 FAIL / 1 BLOCKED**, overall PASS |

### Layer gates (packet p2)

| Gate | Observed | Detail |
|---|---|---|
| `models.per_def` | **PASS** | 216 definitions → 216 distinct MDX, 214 loaded + 2 meshless (PlagueCloud), 0 missing files |
| `models.budget` | **PASS** | full-set peak 73.4 MB ≤ 1536 MB; LRU 32 MB budget, 98 evictions, peak 32.0 MB |
| `economy.lumber` | **PASS** | 3/3 (harvest/deposit, zero-lumber denial, exact lumber debit) |
| `creeps.camps` | **PASS** | 3/3 |
| `map.units.spawn` | **PASS** | 2/2 (90/90 placements, R2 maps 44/44 clean) |
| `jass.frame_loop` | **PASS** | `game.rs` constructs `JassBridge` |
| `hud.resources` | **PASS** | three `ResourcePill`s (GOLD/LUM/SUP) fed from `sim.resources`, `sim.lumber`, `supply()` |
| `hero.stats` | **PASS** | 11/11 hero defs spawn with roster hp/mana/damage/cost |
| hero/buff data | **PASS** | `wc3-data` 20 passed + 1 doctest (hero, buff, SpellDef tables) |

## Regressions found and fixed (QA owner paths)

1. **`build.all_defs` 48 → 14/48.** Exact error from the report: `hkee: place_building -> missing requirement`
   (34 defs). Root cause: the sim layer landed `requires` enforcement in `place_building_impl`
   (`crates/wc3-sim/src/lib.rs:2292-2300`), and the gate spawned no prerequisites.
   Fix: `spawn_prerequisites` walks `UnitDef::requires` (depth ≤ 6, cycle-safe) and spawns completed
   prerequisite buildings through `spawn_def` before the def under test. Re-verified: **48/48**.
2. **`economy.lumber` 3 → 1/3.** Same root cause for the `hkee` placement cases
   (`zero-lumber build returned Err("missing requirement")`). Fix: prerequisites spawned before the two
   `hkee` placements. Re-verified: **3/3**.
3. **`models.per_def` stale measurement.** The old gate compared the legacy `MODEL_PATHS` array (4 entries,
   keyed by `UnitKind`) against roster models. Fix: measure the canonical `analysis/agent-world/parity/roster.csv`
   (216 definition rows) and verify each MDX on disk. Re-verified: **216/216**.

### Transient tree state (not a QA defect)

Between 19:41:44 and 19:44:18 the shared tree did not compile: `crates/wc3-sim/src/items_gen.rs` was
regenerated with a `droppable` field that `data.rs` did not yet define (2 223 errors, log
`analysis/agent-qa/parity/suite-round7.log`). The sim layer completed the edit (`data.rs` 19:42:25,
`lib.rs` 19:44:18); all suites and the probe were re-run afterwards on the stable tree. Reported for
traceability, not as a QA failure.

## Device status — BLOCKED (external: device credential)

The phone is locked: `dumpsys trust` reports `deviceLocked=1`, `dumpsys window` shows
`mDreamingLockscreen=true` and focus `Window{u0 NotificationShade}`. Attempts with evidence:

- `am start` cycle → 3 launch attempts, focus stayed `NotificationShade` (`cycle-blocked-shade.json`);
- `input keyevent KEYCODE_BACK`, `KEYCODE_HOME`, `KEYCODE_WAKEUP`, `wm dismiss-keyguard` → focus unchanged;
- `input swipe 1386 1150 1386 350 260` (landscape) → focus unchanged;
- `cmd statusbar collapse` → focus unchanged.

Root does not bypass device authentication, so the cycle stops here. Current APK
`70c15e91…` is built and installed (`2026-11-13 19:38:07`), but not exercised; the last valid cycle
(`analysis/agent-qa/parity/device-cycle/cycle.json`) belongs to the previous APK `4c8b8617…` and is
labelled as such in every gate that cites it.

**Exact next step:** unlock the phone, then run
`python3 scripts/qa/device_cycle.py --apk build/android-p0/herofolends-p0.apk --out analysis/agent-qa/parity/device-cycle`
(≈3 min: build/train/tap/getevent + screenshots), then `python3 scripts/qa/final_report.py`.

## Result

- QA layer gates on the current tree: **all required gates PASS** (top-6, models per-def/budget, hud
  resources, hero stats, hero/buff data suite), suites green, maps/biomes 44/44 and 111/111.
- Acceptance artifact: `analysis/agent-qa/final/report.json` — **14 PASS, 0 FAIL, 1 BLOCKED** (device),
  overall **PASS**; the packet's 33-PASS device threshold is **not** claimed because the device set could
  not be re-verified on the new APK (`packet_acceptance.met=false` in the report).
- Parity observed overall **FAIL** (21/5/6/1) from five defects owned by other layers, listed verbatim in
  `known_defects_outside_owner`.

## Risks / Next

| Item | Owner | Evidence |
|---|---|---|
| `train.menu_linkage` 117/119 (`harm→hmtt`, `eate→Emoo` unresolved) | data/sim | parity `train.menu_linkage` failures |
| `items.schema` TFT `class` column 0/273 | data | parity `items.schema` |
| `triggers.jass` 39/45, bare `return` 0/3, `elseif` 0/1 | RE (`wc3-jass`) | parity `triggers.*` |
| `cast.coverage` 11/147, `hero.leveling`, `roster.coverage` 0/4, `items.gaps`, `audio.wired` | sim/data/audio | parity `open_gaps` |
| Device acceptance for `70c15e91…` | operator unlock + QA | `device-cycle/STATUS-ROUND7.md` |
| Ownership overlap: `analysis/agent-qa/parity/ROUND7-REPORT.md`, `ARTIFACTS.md`, `models-per-def-*.png` were written by the world agent into the QA path | tech-lead | file mtimes 19:31-19:34 |
