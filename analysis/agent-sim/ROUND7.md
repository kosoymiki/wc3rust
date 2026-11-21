# SIM ROUND 7 — verification + creep spawn into the game

Ledger **355** (`r2-sim`), 2026-11-13. Scope respected: OWNER `crates/wc3-sim/**`,
`crates/wc3-game/src/sim_bridge.rs`, `analysis/agent-sim/**`. `game.rs` untouched
(read-only this round).

## Goal
Verify ROUND6 deliveries on the CURRENT tree, re-run the layer gates, and wire
creep spawn into the running game if it was still missing.

## Verification of ROUND6 (current tree)
| ROUND6 item | Status | Evidence |
|---|---|---|
| Creep camps aggro ≤500 / leash 900 | intact | `qa_creeps_camps` rc 0 |
| Building tech gating `hcas→hkee` | intact | `build_requires` rc 0 |
| RoC ruleset caps/upkeep | intact | `ruleset_roc` rc 0 |
| TFT ruleset caps/upkeep | intact | `ruleset_tft` rc 0 |
| Sim suite | intact | `cargo test -p wc3-sim --offline` rc 0 |

## p3 — creep spawn into the game (gap found and fixed)
**Defect (observed):** `JassBridge::new` mapped every JASS player except 1 to sim
owner 0 (`owner_of_player = vec![0u8; 16]; owner_of_player[1] = 1`), so
`war3mapUnits.doo` neutral creeps (player 12/13) were adopted as the local
player's units and never became camps. `game.rs::reset_map_runtime` calls
`bridge.attach(&mut runtime.world)` (`game.rs:630`), and `attach` only adopted.

**Fix (whole functions):**
- `crates/wc3-sim/src/lib.rs:1949` `Sim::cluster_owner_camps(owner, radius, aggro, leash) -> usize`
  — greedy proximity grouping over uncamped alive units in stable `UnitId`
  order; `lib.rs:1978` `Sim::cluster_neutral_camps()` (owner 2, radius 600,
  aggro 500, leash 900).
- `crates/wc3-game/src/sim_bridge.rs:294` neutral/unused JASS players now map to
  owner 2 (`vec![2u8; 16]`, players 0/1 kept); `sim_bridge.rs:319`
  `JassBridge::attach` runs `self.sim.cluster_neutral_camps()` after `adopt`, so
  every map reset places its neutral units into real creep camps.

**Tests (new):**
- `crates/wc3-sim/tests/merge.rs:910` `neutral_map_units_cluster_into_creep_camps`
  — 3 proximity groups → 3 camps, second pass idempotent (0), near camp aggros
  ≤500, distant camp stays idle.
- `crates/wc3-game/src/sim_bridge.rs:748`
  `neutral_map_units_are_adopted_as_creep_camps` — JASS owners 12/13 adopt as
  owner 2 (not player 0), two proximity camps, all camp members owner 2.

## Evidence (exact commands + rc)
| Command | rc | Result |
|---|---:|---|
| `cargo test -p wc3-sim --offline` | **0** | **123 passed / 0 failed / 2 ignored** (14 suites; merge 29) — `analysis/agent-sim/round7-sim-test.log` |
| `cargo test -p wc3-sim --test merge build_requires` | **0** | 1 passed, 28 filtered |
| `cargo test -p wc3-sim --test merge qa_creeps_camps` | **0** | 1 passed, 28 filtered |
| `cargo test -p wc3-sim --test merge ruleset_roc` | **0** | 1 passed, 28 filtered |
| `cargo test -p wc3-sim --test merge ruleset_tft` | **0** | 1 passed, 28 filtered |
| `cargo test -p wc3-sim --test merge neutral_map_units_cluster_into_creep_camps` | **0** | 1 passed, 28 filtered |
| `cargo test -p wc3-game --lib neutral_map_units_are_adopted` | **0** | 1 passed, 121 filtered |
| `cargo check -p wc3-game --lib --offline` | **0** | `analysis/agent-sim/round7-game-check.log` |
| `cargo test -p wc3-game --lib --offline` | **0** | **103 passed / 0 failed / 19 ignored** — `analysis/agent-sim/round7-game-test.log` |

No device run (not granted).

## Result
**DONE.** ROUND6 deliveries verified green on the current tree; all four
acceptance gates pass; creep spawn is now wired into the game map runtime
(map neutrals → owner 2 → proximity camps with aggro/leash) with sim- and
game-level tests.

## Risks / Next
- Map **destructables/trees** are still not adopted by `JassBridge` (it adopts
  units only); the pending `game-map-scene.patch` (`spawn_assets_map`) remains
  the path for trees/doodads or a future `adopt` extension.
- Camp clustering uses spatial proximity (600) because editor camp grouping is
  not in the DOO records; units of one editor camp farther than 600 apart would
  split. WC3 camps are placed within ~one camp radius, so this matches the
  observed data.
- `owner_of_player` maps JASS players ≥2 to neutral 2 (2-faction sim). A future
  >2-faction mode needs a wider mapping table.
