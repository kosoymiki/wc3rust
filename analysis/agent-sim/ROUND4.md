# SIM round 4 — map scene, tech gating, ruleset

Ledger **355**, 2026-11-13. Owner scope: `crates/wc3-sim/**`,
`crates/wc3-game/src/sim_bridge.rs`. `game.rs`/`hud.rs` are HUD-owned and receive
the one-line map hook as a patch.

## Delivered

| Item | Code | Verification |
|---|---|---|
| Tech-tree gating for **buildings** | `requirements_met` `lib.rs:2255` used by `build_instant_impl` and `place_building_impl`; `hcas→hkee`, `ofrt→ostr`, `unp2→unp1`, `etoe→etoa` tier chain regenerated | `building_placement_enforces_the_real_tier_requirements` |
| RoC/TFT ruleset switch | `Ruleset { Roc, Tft }` `lib.rs:58`, `Sim::set_ruleset/ruleset`, supply cap 90/100, upkeep 40/70/90 vs 50/80/100 | `roc_and_tft_rulesets_switch_caps_and_upkeep` |
| Map scene → sim | `sim_bridge::spawn_map_scene(MapBundle)` (`sim_bridge.rs:85`): `war3mapUnits.doo` units (players 0/1, neutral→owner 2), `war3map.doo` trees/destructables, neutral units clustered into camps 500/900; `spawn_map_file`, `spawn_assets_map` | `map_units_spawn_from_human04_scene`, `map_units_spawn_from_human04` |
| Game hook (patch) | `analysis/agent-sim/patches/game-map-scene.patch` calls `spawn_assets_map` after grid creation; dry-run clean | patch dry-run rc0 |
| Lumber HUD (patch) | `analysis/agent-sim/patches/hud-lumber.patch` shows `sim.lumber[player]` | dry-run rc0 |

## rc

| Gate | rc | Result |
|---|---|---|
| `cargo test -p wc3-sim` | **0** | **118 passed / 0 failed / 2 ignored** (`analysis/agent-sim/round4-sim-test.log`) |
| `cargo check -p wc3-game --lib` | **0** | `round4-game-check.log` |
| `cargo test -p wc3-game --lib` | **0** | **95 passed / 0 failed / 18 ignored** (`round4-game-test.log`) |
| HUD/game patches | **0** | all three `patch --dry-run -p1` clean |

No device run.

## Remaining (external owners)

1. HUD applies `game-map-scene.patch` so a real map populates units/creeps/trees
   in the running game; `spawn_assets_map` is already tested in the game crate.
2. HUD applies `hud-lumber.patch` (non-zero lumber pill) and
   `hud-build-menu.patch` (SLK build menu).
3. Assets/world still needs to expose the TFT/RoC origin if the game should
   choose `Ruleset` automatically; sim default is TFT and the API is ready.
