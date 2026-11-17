# SIM ROUND 6 — TOP-5/#13/#15 gates (creep camps, tech gating, ruleset)

Ledger **355** (session `ses_e3059541fffdUO0AGBqY6t1juG`), 2026-11-13. Owner scope respected: `crates/wc3-sim/**`, `analysis/agent-sim/**` (and prior `crates/wc3-game/src/sim_bridge.rs` unchanged this round). No other layer’s reader/parser/sim reimplemented.

## Objective (packet)
TOP-5/#13/#15: creep camps + neutral AI, tech-tree gating (hcas requires hkee), RoC/TFT ruleset switch.

## Findings / state
Mechanisms were already present from prior rounds (not repeated): `spawn_camp`/`register_camp`/`advance_creeps` with aggro 500 / leash 900 (`crates/wc3-sim/src/lib.rs`), building `requirements_met` enforcing `UnitDef.requires` incl. `hcas→hkee` (`lib.rs:2255`, `roster_gen.rs` tier chain), and `Ruleset::{Roc,Tft}` with caps/upkeep (`lib.rs:58,2949,2980,3008`). This round added packet-named acceptance gates and verified them.

## Actions (file:line)
- `crates/wc3-sim/tests/merge.rs` (EOF additions): added `qa_creeps_camps` (spawn N creeps, aggro ≤500, leash home), `build_requires` (hcas denied without hkee, allowed with hkee), `ruleset_roc` (upkeep 0-40/41-70/71-90, cap ≤90), `ruleset_tft` (upkeep 0-50/51-80/81-100, cap ≤100). Fixed test supply counts to match real footman supply=2.

## Evidence (commands + rc)
| Command | rc | Result |
|---|---|---|
| `cargo test -p wc3-sim --test merge build_requires` | 0 | 1 passed, 27 filtered out |
| `cargo test -p wc3-sim --test merge qa_creeps_camps` | 0 | 1 passed, 27 filtered out |
| `cargo test -p wc3-sim --test merge ruleset_roc` | 0 | 1 passed, 27 filtered out |
| `cargo test -p wc3-sim --test merge ruleset_tft` | 0 | 1 passed, 27 filtered out |
| `cargo test -p wc3-sim` | 0 | **122 passed / 0 failed / 2 ignored** across 14 suites (`analysis/agent-sim/round6-sim-test.log`) |

Full suite breakdown (tail): 24,9,6,10,3,10,1,1,28,14,(2 ignored),10,6,0 passed.

## Result
**DONE (packet acceptance gates satisfied).** Creep camps aggro/leash, building tech gating (hcas requires hkee), and RoC/TFT ruleset caps/upkeep are implemented and green under packet-named tests.

## Risks / Next
- "RoC-only units vs TFT" roster deltas are not explicitly asserted by a dedicated gate beyond ruleset mechanics; roster is SLK-derived TFT data today. Any RoC-specific roster split needs explicit data/source decision.
- Build card "shows only unlocked tiers" is a HUD/UI concern (HUD-owned patches pending per prior rounds); sim exposes `requirements_met`/def.requires for gating.
- No device run; game-level integration depends on HUD owner applying pending patches (`game-map-scene`, `hud-lumber`, `hud-build-menu`).
