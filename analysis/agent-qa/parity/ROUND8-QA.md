# P0 verification (QA layer) — current tree, device NOT_RUN

**Date:** 2026-11-13 21:05 MSK
**Ledger:** 349 (`r2-qa`, owner `scripts/qa/**`, `analysis/agent-qa/**`)
**Directive:** P0 device cycle now → then **STOP DEVICE LAUNCHES** (operator is using the phone).

## Goal

Rebuild the P0 APK from the current tree, run the full QA acceptance suite (parity gates + suites),
produce device evidence, and refresh `analysis/agent-qa/final/report.json` — reporting GAPs as GAPs.

## APK (current tree)

| Item | Value |
|---|---|
| `cargo build --release --offline -p herofolends-android` | rc 0, `target/release/libmain.so` 20:48:41 (8 918 816 B) |
| `JAVA_TOOL_OPTIONS='-Xmx512m -Xms64m' bash scripts/build-apk.sh` | rc 0, `build/android-p0/herofolends-p0.apk` 20:48:49 |
| sha256 | `0f19ebbbf3fdbfdbb94ee7b62981078bcca21b26671423ba9c0007c39b4dfd78` |
| installed | `pm install -r` Success, `lastUpdateTime 2026-11-13 20:49:48` |

`javac` needed `-Xmx512m`; without it the JVM aborts with
`Could not reserve enough space for 2500608KB object heap` (observed earlier, fixed in the invocation).

## Acceptance suite — headless parity (current tree)

Command: `python3 scripts/qa/parity.py --skip-build` (probe `target/debug/h2-parity` 20:56:23, rc 0,
elapsed 2.93 s) → `analysis/agent-qa/parity/report.json`:

**overall PASS — 32 PASS / 0 FAIL / 0 GAP / 1 SKIP**, matrix 33/33 aligned.

| Gate | Status | Measured |
|---|---|---|
| `train.all_defs` | PASS | 53/53 trainable defs |
| `train.menu_linkage` | PASS | 121/121 links resolvable (was 117/119) |
| `build.all_defs` | PASS | 50/50 placed, charged, completed (tech tree satisfied) |
| `tech.requires` | PASS | 2/2 (prerequisite denial + producer gate) |
| `cast.spells` | PASS | 11/11 hand-written spells |
| `cast.coverage` | PASS | 154/154 roster ability ids data-backed; 115 active castable, 245 passive/aura rows; hand-written 11 |
| `hero.stats` | PASS | 12/12 |
| `hero.coverage` | PASS | 12/12 melee heroes present |
| `hero.leveling` | PASS | 12/12 XP→level, attribute growth, `set_hero_level`, non-hero rejection |
| `roster.coverage` | PASS | 4/4 matrix melee ids present |
| `items.data` | PASS | 273/273 TFT rows |
| `items.schema` | PASS | 2/2 trees expose `class` |
| `items.runtime` | PASS | 7/7 |
| `items.gaps` | PASS | 240/240 ability items covered; shop `Requires` gating + stock/charges; 0 retail recipe rows (engine present) |
| `economy.lumber` | PASS | 3/3 |
| `creeps.camps` | PASS | 3/3 |
| `map.units.spawn` | PASS | 2/2 |
| `triggers.jass` | PASS | 45/45 |
| `triggers.return` | PASS | 3/3 (bare `return`) |
| `triggers.elseif` | PASS | 1/1 |
| `audio.game` | PASS | 6/6 |
| `audio.wired` | PASS | game depends on `wc3-audio`, 11 call sites |
| `menu.fdf` | PASS | 15/15 |
| `assets.tree` | PASS | 1/1 |
| `maps.headless` | PASS | 44/44 clean |
| `box.boundary` / `box.getevent` | PASS | harness boundary evidence (last device cycle) |
| `models.per_def` | PASS | 216 definitions → 216 distinct MDX, 214 loaded + 2 meshless, 0 missing |
| `models.budget` | PASS | full-set peak 73.4 MB ≤ 1536 MB; LRU 32 MB, 98 evictions, peak 32.0 MB |
| `jass.engine` / `jass.frame_loop` | PASS | 44/44 map `main()`; `JassBridge` in the frame loop |
| `hud.resources` | PASS | GOLD/LUM/SUP pills from sim state |
| `maps.device` | SKIP | device not run (see below) |

### Note on the packet's GAP list

The dispatch listed `cast.coverage 11/154`, `hero.leveling`, `items.gaps` as remaining gaps. On the
current tree these three gates measure **PASS**, and the report carries the numbers so the claim is
auditable rather than asserted:

- `cast.coverage` counts *roster ability ids with an AbilityData row* (154/154). The hand-written spell
  count is still 11 and is exposed as `metrics.hand_written = 11`; loading a distinct number for
  "spell implemented in code" would be a different metric.
- `hero.leveling` exercises `add_hero_xp` → level 2 with attribute/HP/damage growth, `set_hero_level(6)`
  with XP floor, and rejects XP on a non-hero worker.
- `items.gaps` covers 240/240 ability items plus shop purchase with `Requires` gating and stock/charges;
  `recipe_rows = 0` is the retail truth (the combine engine exists, no shipped recipes) and is recorded
  as such.

If the tech-lead's snapshot predates the sim/data deliveries (sim `lib.rs` 20:41, data/jass 20:12), these
numbers are the newer measurement; nothing was re-labelled to force PASS.

## Suites (current tree)

| Suite | Command | rc | Result |
|---|---|---:|---|
| data | `cargo test --offline -p wc3-data` | 0 | 20 passed + 1 doctest |
| sim | `cargo test --offline -p wc3-sim --lib` | 0 | 24 passed |
| game | `cargo test --offline -p wc3-game --lib` | 0 | 112 passed, 22 ignored |
| maps/biomes | `python3 scripts/qa/map_biome.py` | 0 | maps 44/44 clean (units errors 0); biomes 111/111, 15 tilesets |

Logs: `analysis/agent-qa/parity/suite-{data,sim,game}-round8.log`, `analysis/agent-qa/r2/report.json`.

## Device — NOT_RUN (operator using the phone)

**No launch was performed after the STOP directive.** `org.herofolends.p0` is force-stopped (rc 0);
foreground app is `bin.mt.plus.canary`. Marker: `p0-device/device-not-run.json`, `p0-device/DEVICE-NOT-RUN.md`.

Attempt history (all before the directive, each with evidence):

| # | Time | APK | Result |
|---|---|---|---|
| 1 | 20:41:54 | `e45bb941…` | harness FAIL: fixed-fraction build points all `PLACE_BLOCKED` after the minimap camera jump |
| 2 | 20:49:55 | `0f19ebbb…` | **PARTIAL**: launch, minimap, command card, build, getevent proven; control-group recall, tooltip, train unproven |
| 3 | 20:52:28 | `0f19ebbb…` | harness FAIL: `UnboundLocalError button_logs` (fixed), build blocked |
| 4 | 20:55:22 | `0f19ebbb…` | ABORTED: `screenshot 1080x2414 does not match display 2414x1080` — app not foreground (operator force-stop) |

Proven on device in attempt 2 (APK `0f19ebbb…`):

- launch `GAME_FIRST_FRAME surface=2414x1080 fullscreen=true`, `GAME_READY` (map Альяна4, models=4);
- console/resource-pill crops captured (`01-launch.png`), console rect on screen;
- minimap tap moved the world view (`world_diff` 28.7–51.4, `camera_moved=true`);
- command card `BUTTON BASE` logged; control-group save+recall executed (`selected 6 → 6`);
- build `BUTTON BUILD` → `PLACE_BUILT htow cost=450 gold=624`; new hall selected;
- getevent: injected `input motionevent` reaches `/dev/input/event5` (35–1354 lines per capture);
- cleanup: app stopped, Termux focused (`force_stop_rc=0`, `termux_rc=0`).

**Unproven (stays unproven, not PASS):** control-group recall after a different selection
(BASE→hall, 1→6), tooltip band under press-and-hold, and training from the new hall on this build.
The harness fixes are in `scripts/qa/device_p0_cycle.py` and need one granted cycle.

## Result

- `analysis/agent-qa/final/report.json` (`run_id=p0-verify-headless`): **14 PASS, 0 FAIL, 1 NOT_RUN**,
  overall **PASS**; parity observed **32/0/0/1**; `device.status = NOT_RUN` with the attempt history and
  the exact next step. `packet_acceptance` records that the device set is not claimed.
- `analysis/agent-qa/parity/MATRIX-DELTA.md` refreshed (15 matrix rows updated to measured values).

## Risks / Next

| Item | Owner | Evidence |
|---|---|---|
| Device P0 evidence for `0f19ebbb…` (control groups/tooltip/train + screenshots) | tech-lead device grant → QA | `p0-device/DEVICE-NOT-RUN.md` |
| `cast.coverage` metric meaning (data-backed vs hand-written) | tech-lead decision | `metrics.hand_written=11` in parity report |
| Model budget device PSS (1536 MB) | device grant | headless budget PASS; device PSS unmeasured |
