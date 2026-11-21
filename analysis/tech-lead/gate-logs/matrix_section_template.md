## LAYER GATES — proof per layer (2026-11-15)

Purpose: **no layer may claim done without a command that proves it on the real
map.** This section is the per-layer acceptance ledger; it replaces "the tests
pass" with a named command, a machine-readable result and the open rows.

Run it:

```bash
bash analysis/tech-lead/run_layer_gates.sh                 # all 15 steps, PASS/FAIL table
bash analysis/tech-lead/run_layer_gates.sh game render      # aliases: game=4 tests, render=3 steps
JOBS=2 STEP_TIMEOUT=1800 bash analysis/tech-lead/run_layer_gates.sh
```

Exit codes: `0` every step PASS, `1` at least one FAIL/TIMEOUT/NO-TESTS, `2`
no failures but a SKIP (a real-map gate that did not run the real map, or the
device frame not captured). Artifacts: `analysis/tech-lead/gate-logs/last-summary.txt`
and `last-results.json` (one record per step: kind, status, rc, passed, failed,
ignored, skipped_tests, elapsed).

Rules the script enforces:

* `NO-TESTS` is FAIL — a filter that matches nothing prints `0 passed` and
  exits 0; that trap is what let `playability_real_map.rs` look green.
* A real-map test that prints `skip:` is `SKIP`, never `PASS`.
* Game tests run with `SDL_AUDIODRIVER=dummy`: the SDL device is not part of a
  test, and two concurrent runs (another agent running the same test) block on
  it in `futex` with 0 CPU — observed 2026-11-15 04:10 for 20 minutes. With the
  dummy driver `melee_map_paths` went from 90 s to **10.3 s**; `--test-threads=1`
  is added only when `MemAvailable < 4 GB` (two Games ≈ 2 GB).
* `policy_ignored` fails when an acceptance test hides behind `#[ignore]` or
  when `run_layer_gates.sh` stops running `terrain_ground_detail` with
  `--ignored`.

### Gate table — final run 2026-11-15 (see `gate-logs/run_final_2026-11-15.log`)

| Step | Layer | Criterion (done means) | Proof command | Observed 2026-11-15 | Status |
|---|---|---|---|---|---|
| sim | sim | economy, combat, AI, movement, determinism on the shipped maps | `cargo test -p wc3-sim` | @@STEP:sim@@ | @@STATUS:sim@@ |
| data | data | every typed table parsed and consumed by the sim | `cargo test -p wc3-data` | @@STEP:data@@ | @@STATUS:data@@ |
| formats | formats | 44 shipped maps parse (W3E/WPM/DOO), real doodads/trees | `cargo test -p wc3-formats` | @@STEP:formats@@ | @@STATUS:formats@@ |
| audio | audio | race themes, per-def voice sets, mixer queue policy, audibility radius/cut-off (ledger 400) | `cargo test -p wc3-audio` | @@STEP:audio@@ | @@STATUS:audio@@ |
| audio_earshot | audio-device | device kill log: every `GAME_AUDIO_DEATH` carries `dist=` and is inside the 4000 earshot | `python3 analysis/tech-lead/check_audio_earshot.py` | @@STEP:audio_earshot@@ | @@STATUS:audio_earshot@@ |
| anim | anim | every roster def resolves id → model → sequences | `cargo test -p wc3-anim` | @@STEP:anim@@ | @@STATUS:anim@@ |
| render_ground | render | headless GPU render of the Justice start view; writes `terrain-film-after.ppm` and prints the patch stats | `cargo test -p wc3-game --lib terrain_ground_detail -- --ignored` | @@STEP:render_ground_detail@@ | @@STATUS:render_ground_detail@@ |
| render_sharp | render | terrain sharpness on the rendered frame (device criterion) | `python3 analysis/tech-lead/terrain_sharpness.py` | @@STEP:render_sharpness@@ | @@STATUS:render_sharpness@@ |
| render_sharp_dev | render-device | same measurement on a **device** grass frame | `terrain_sharpness.py --image analysis/agent-qa/terrain/device-grass.png` | @@STEP:render_sharpness_device@@ | @@STATUS:render_sharpness_device@@ |
| hud | hud | card follows selection: 12 slots, labels, hotkeys, touch layout | `cargo test -p wc3-game --lib hud::` | @@STEP:game_hud@@ | @@STATUS:game_hud@@ |
| playable_loop | game | real match: gather → train → build → fight | `cargo test -p wc3-game --test playable_loop` | @@STEP:game_playable_loop@@ | @@STATUS:game_playable_loop@@ |
| touch | game-touch | pointer taps select → build card → tree=lumber, mine=gold | `cargo test -p wc3-game --test touch_playability` | @@STEP:game_touch_playability@@ | @@STATUS:game_touch_playability@@ |
| map_paths | game-map | cross-map move >800 u, ghost anchor, gold+lumber on Justice | `cargo test -p wc3-game --test melee_map_paths` | @@STEP:game_melee_map_paths@@ | @@STATUS:game_melee_map_paths@@ |
| real_map | game-real-map | Orc01 6000 u walk (script-only campaign map, no economy); Justice harvest + building placement | `cargo test -p wc3-game --test playability_real_map` | @@STEP:game_playability_real_map@@ | @@STATUS:game_playability_real_map@@ |
| economy_probe | game-touch-probe | phone pointer path: mine tap = `Harvest` (never `Attack`/`Idle`), tree tap = `HarvestLumber`, income = whole 10-per-trip loads | `bash analysis/tech-lead/run_economy_probe.sh` | @@STEP:economy_probe@@ | @@STATUS:economy_probe@@ |
| policy | policy | every `#[ignore]` has a non-acceptance reason; acceptance-backed ignores are run by this gate | `python3 analysis/tech-lead/audit_ignored_tests.py` | @@STEP:policy_ignored@@ | @@STATUS:policy_ignored@@ |

### Terrain sharpness acceptance (tech-lead thresholds, 2026-11-15)

Criterion for a grass patch on the **device** frame: **edge ≥ 0.12 and channel
std ≥ 18 on at least two channels** (Sobel magnitude > 12.0 over the patch
interior; patch 200×140 at a 2414×1080 frame, anchored 26 % x / 34 % y — the
geometry `analysis/agent-render/film_stats.py` measured with).

Reported by the tech-lead (2026-11-15, provenance: their measurement message):

| Frame | channel std | p5/p50/p95 | Sobel edge |
|---|---|---|---|
| device baseline (2414×1080 grass patch) | (5.5, 12.9, 5.5) — "soap film" | 47/58/66 | 0.051 |
| headless after LOD/specular fix | (26.9, 27.4, 20.2) | 9/48/83 | **0.381** |
| headless before | — | — | 0.098 |

Recomputed in this session with `analysis/agent-render/film_stats.py`
(observed 2026-11-15 01:47 MSK, same metric) — the numbers do **not** match the
quoted ones exactly, so both are recorded rather than pretending one set:

| Frame | channel std (centre patch) | p5/p50/p95 | edge | verdict |
|---|---|---|---|---|
| `terrain-film-before.png` | (14.9, 11.7, 11.2) | 53/63/72 | 0.445 | below the after-thresholds |
| `terrain-film-after.png` | (29.3, 25.4, 24.6) | 32/58/99 | **0.974** | PASS (`film_stats.py` rc=0) |
| `patch-before.png` | (38.5, 25.7, 25.9) | 59/67/150 | 0.381 | matches the quoted *edge* only |
| `patch-after.png` | (58.8, 43.0, 44.1) | 42/71/184 | 0.983 | PASS |

`terrain_ground_detail_justice` itself (2026-11-15 01:58, rc=0, 177 s) prints the
same measurement it wrote to the PPM:
`TERRAIN_GROUND tag=after std=(29.3,25.4,24.6) p5=32 p50=58 p95=99 edge=0.974
blue_excess=-5.26` — the render and the metric agree end-to-end, so the sharpness
PASS is reproducible from the artifact, not inherited from a claim.

**Device frame is not captured yet** (`analysis/agent-qa/terrain/device-grass.png`);
the `render_sharp_dev` step is SKIP until the tech-lead runs the device slot.
A headless PASS is not a device PASS: the operator's complaint was on the phone.

### Device economy touch parity — the actual numbers

`gather_probe` (2026-11-15 01:57, Justice + real MPQ), after the sim/HUD fixes:

```
order 3 ticks after mine tap: Some((Harvest { mine: Vec2(14592.0, 1280.0), phase: ToMine }, ...))
order 3 ticks after tree tap: Some((HarvestLumber { tree: Vec2(15040.0, 1408.0), phase: ToMine }, ...))
gold 1000 -> 1040 (4 trips)      lumber 1000 -> 1080 (8 trips)
PROBE PASS
```

Before the fix the same probe printed `Some(Attack(UnitId(15)))` for the mine
(a creep guarding the mine stole the tap) and `Idle` for the tree, with gold
rising only by the 60-per-30 s passive drip — a working economy system that the
phone could not reach.

`touch_playability` (2026-11-15 01:47, rc=0, 207 s, real Justice + MPQ):

```
PARITY economy.lumber touch_tap: lumber 1000->1010 picked_at=100 deposited_at=152 tick=152 order=HarvestLumber tree=(14976,2048) phase=ToTarget
PARITY economy.gold   touch_tap: gold   1000->1010 picked_at=154 deposited_at=235 tick=235 order=Harvest GOLD mine=(14592,1280) phase=ToTarget
```

The device scenario log (`analysis/agent-qa/operator-bugs/scenarios.json`) has
`"harvest": {"tap": null, "lumber_before": null, "lumber_after": null}` — the
device harvest path was **never exercised** on the 2026-11-14 build, which is
why a unit-test-only scorecard could call the economy green. The next device
cycle must record `tap`, `lumber_before/after` and `gold_before/after`.

### Layer closures folded in (tech-lead updates, 2026-11-15)

**SIM (ledger 393, closed).** Fake income removed — `passive_income: false` at
`crates/wc3-sim/src/lib.rs:845`; gather gates from blocked footprints
(`blocked_reach` / `gather_gate`, `lib.rs:2146/2169`); no silent order drops
(`gather_path_failed`, `lib.rs:1963`); tree retarget within 600 u; WC3 smart
order (Move onto a mine/tree becomes a harvest, `lib.rs:1148`); AI economy
(`ai.rs:262`). Measured: `cargo test -p wc3-sim` **235 passed / 0 failed /
2 ignored rc=0** (the gate measured the same 235 before the ledger closed); the
touch path deposits for real (`touch_playability`: mine +10 at tick 232, tree
+10 at tick ~149) and 5 workers produce +320 gold / +420 lumber per 60 s.

**AUDIO (ledger 400, closed).** `reference_distance` from `MinDistance`
(`soundinfo.rs:79 playback_distances`), hard `DistanceCutoff` cull
(`game_audio.rs:363-385`), zero attenuation past the far end (`engine.rs:952`),
audibility gate `set_audible_radius` / `is_within_earshot` / `gate_voice`
(`game_audio.rs:1003-1039`, default 4000), region-scoped ambient
(`game_audio.rs:906`). Measured: 39 lib + 17 integration + 1 no_alloc rc=0;
12000-unit energy 0.0. Gate: the `audio` step plus `audio_earshot`, which today
**SKIPs** because `device_audio.log` has no `GAME_AUDIO_DEATH` lines and
`game.rs:1440` does not log a distance — the next device log must carry
`dist=<units to camera>` for the check to become a real PASS.

**Campaign maps are not melee maps.** `Orc01.w3m` has `"gold_mines":0`
(`analysis/agent-re/phase0/mapcontent.json`) and starts units on unwalkable
cells (3); its content is script-created. `playability_real_map` therefore uses
it **only** for the 6000 u terrain/path walk and runs the harvest/build checks on
Justice. A campaign map must never be used as a melee economy fixture.

### Device rows behind the P0 verdicts (OPERATOR_BUGS.json v2)

`OPERATOR_BUGS.json` now uses `verified | open | blocked` with an owner and the
exact missing artifact per row: **8 verified, 10 open, 0 blocked**. The
acceptance script gates P0-10/11/12 on these rows, so a device regression fails
the P0 row instead of hiding behind a unit test:

| P0 | Device rows | Open today | Effect |
|---|---|---|---|
| P0-10 UI/input | 5,7,8,9,18 | #7 army icons, #8 hero abilities, #9 labels, #18 truncation | P0-10 FAIL |
| P0-11 audio voices | 4,13 | #4, #13 per-def voice capture | P0-11 FAIL |
| P0-12 render | 10,11,12,14,15,16,17 | #10 ghost invisible, #14 water band | P0-12 FAIL |

Device evidence spans two builds: screens/logcat `d3efe82e…`, audio run
`72627294…`, current APK `d81850bf…` (rebuilt during this session) — recorded in
`device_evidence`; a row that needs both artifacts is verified only after one
build carries both.

### Honesty fixes shipped with this section

1. `playability_real_map.rs` looked for `Maps/Orc01.w3m`; the extracted tree has
   `Maps/Campaign/Orc01.w3m`. All three tests had been returning `skip` with
   rc=0 since they were written. Fixed (both paths probed; a missing map inside
   an existing tree is now a hard failure). Its harvest/build checks moved to
   Justice because **Orc01 genuinely has 0 gold mines**
   (`analysis/agent-re/phase0/mapcontent.json`: `"gold_mines":0`), so the old
   "no gold mine near the base" expectation could never hold there.
2. The building test now searches for a site with `can_place` (the first probe
   landed on an occupied footprint: `Err("site occupied")`); `walkable_pos_near`
   alone never proved a building could be placed.
3. `melee_map_paths` now asserts the worker's spawn cell is walkable (operator
   bug #3) instead of printing it.
4. `wc3-data` doctest failure (`coverage.rs:16`, `Field` not imported) was caught
   by the gate at 01:22 and fixed by the data owner at 01:53 (`--doc` rc=0).
5. A transient red tree during the 01:19 run (`wc3-sim/src/lib.rs` calling a
   not-yet-defined `gather_path_failed`) was reported with the exact compile
   error; the sim owner's fix landed and the following run compiled.
6. `touch_playability` failure messages name the tap log, the issued order, the
   phase, path length, carry, timer and every `Event::Error`; the two failure
   classes observed while building it were (a) camera not centred → off-viewport
   tap, (b) card expected hard-coded Orc builds although the w3i seats the local
   player in either slot. Both assumptions are gone. Note: `Game::pending` is
   private, so the "pending command" is named through the `TAP` log and the
   post-tap `Order`, not by printing the queue.
7. `SDL_AUDIODRIVER=dummy` for game steps: a 20-minute futex hang with 0 CPU was
   another agent's identical test holding the SDL audio device. The gate now
   runs game tests with the dummy driver (10× faster) and serialises only under
   4 GB free RAM.
8. `#[ignore]` audit: 30 ignored tests, every one with a non-acceptance reason
   (one missing reason fixed in `tests/asset_gallery.rs`); the single
   acceptance-backed ignore (`terrain_ground_detail_justice`) is run with
   `--ignored` by the gate; the audit fails if that step disappears.

### First failing layer (rule)

If the gate fails, it prints `FIRST FAILING LAYER: <layer> (<step>, <kind>)
status=<status> rc=<rc>` plus the last 40 log lines, and stops the table work
there. That is the exact text to hand to the owner of the layer: it names the
step, the rc and the failing assertion, so the owner gets a reproducible command
instead of a symptom.
