# PARITY ROUND 7 — RE layer verification (ledger 350)

Date: 2026-11-13 · Layer: RE (`crates/wc3-re/**`, `crates/wc3-jass/**`, `analysis/agent-re/**`)
Task: verify ROUND 6 deliveries on the current tree, re-run RE layer gates, refresh
`qa.jass.map_runtime`, produce artifacts. No device/APK run (not granted).

## Result

All RE layer gates PASS on the current tree. No regression found in RE owner paths;
no production code change was required this round (verification-only round).

| # | Gate | Command | rc | Evidence |
|---|---|---|---:|---|
| p1 | wc3-re lib tests | `cargo test -p wc3-re --lib --offline` | **0** | `analysis/agent-re/round7-re-tests.log` — **47 passed**, 0 failed, 0 ignored |
| p1b | wc3re binary rebuild | `cargo build -p wc3-re --bin wc3re --offline` | **0** | `analysis/agent-re/round7-build-wc3re.log` |
| p2 | Corpus scan | `./target/debug/wc3re scan analysis/war3_full3 --json analysis/agent-re/scan-r7.json --md analysis/agent-re/SCAN-r7.md` | **0** | `scan-r7.json`: `ok=true`, `maps.errors=0`, **`scripts.unimplemented={}`**, `main_ok=44/44`, `natives=791`, `steps=1 313 713` |
| p3 | Creep-spawn API tests | `cargo test -p wc3-re --lib --offline creeps` | **0** | `analysis/agent-re/round7-creeps-test.log` — **3 passed** (`camp_defs_clusters_neutral_aggressive`, `resolve_camps_matches_jass_units`, `player_one_units_ignored`) |
| p4 | Map runtime parity | `./target/debug/wc3re parity` | **0** | `analysis/agent-re/round7-parity.json` — 44 maps, `main_ok=44`, `executed_triggers=713`, `map_units=6 723`, `spawned_units=468`, `matched_units=6 255`, `map_doodads=115 134` |
| p4b | `qa.jass.map_runtime` gate | `python3 analysis/agent-re/qa_map_runtime.py` | **0** | `analysis/agent-re/round7-qa-gate.log` — `{"gate":"qa.jass.map_runtime","ok":true,"failures":[]}` |
| p4c | ROUND 6 in-game gate re-verify | `cargo test -p wc3-game --lib --offline map_units_spawn` | **0** | `analysis/agent-re/round7-game-spawn.log` — **2 passed**, 0 failed, 119 filtered (`game::tests::map_units_spawn`, `sim_bridge::tests::map_units_spawn_from_human04_scene`) on current-tree `game.rs` `e7db1c68…` (HUD moved it from `05335b2f…`; patch still integrated) |

Artifact refreshed this round: `analysis/agent-re/qa-jass-map-runtime.json`
(SHA-256 `138cfc862cdc826ac5a2dc7b74394c5173f04142f5395e8f4f34970327374c99`, 19 406 B,
2026-11-13 18:34 MSK).

## ROUND 6 deliveries re-verified

| Delivery | Where | State on current tree |
|---|---|---|
| `war3mapUnits.doo` placement spawn (90/90 Human04) | `crates/wc3-re/src/engine.rs` | `map_units_spawn` re-run (p4c) |
| JASS frame loop (`Engine` + `JassBridge::step_engine`) | `crates/wc3-game/src/game.rs` (HUD-owned, applied) | `game.rs` now `e7db1c68…` (HUD changes since ROUND 6); `map_units_spawn` 2 passed, `attach_map_runtime`/`JassBridge` wiring intact |
| 791 natives, `unimplemented={}` | `crates/wc3-re/src/jass/natives.rs` | scan p2 confirms `scripts.unimplemented={}` |
| Creep-spawn API for sim (#5) | `crates/wc3-re/src/creeps.rs` (`ce1353a9…`), `lib.rs` `pub mod creeps` | p3 tests 3/3 pass |

## Invariants held

- One implementation per concern: no second parser/sim/renderer added; camps resolve to the
  JASS world spawned by `Engine::spawn_map_entities` (no duplicate unit creation).
- Original game data only (MPQ/SLK/FDF/W3E/MDX). No invented values.
- No stubs/TODO; no device run (not granted by the packet).
- Owner paths only: this round wrote only under `crates/wc3-re/**` (none required) and
  `analysis/agent-re/**`.

## Risks / Next

- Sim-layer consumption of `CreepCamp` (`wc3_re::creeps::Engine::creep_camps()` →
  `wc3-sim::Sim::camps`) is a sim-owner integration step; the RE API is ready and tested.
- Device/APK acceptance of the in-game JASS frame loop remains with the device/QA owner
  (not granted here). Headless evidence is complete.
- `qa.jass.map_runtime` is headless by design; it proves `main` + `InitCustomTriggers` +
  every placement, not HUD/screenshot rendering.

## H2

- Capture: `707` (ROUND 6) → ROUND 7 capture recorded on close.
- Experience: `4399` (ROUND 6) → ROUND 7 lesson recorded on close.
- Ledger `350`: ROUND 7 checkpoints at p1/p2/p4, closed with rc + artifacts.
