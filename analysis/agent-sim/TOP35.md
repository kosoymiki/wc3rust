# TOP-3/5 parity increment — lumber, destructables, map units, creeps

Ledger **355** (`r2-sim`), 2026-11-13. Aligned to `analysis/tech-lead/PARITY_MATRIX.md` §7
rows **#3 lumber + destructable HP**, **#4 map-placed units**, **#5 creep camps + neutral AI**.
Simulation owner paths only (`crates/wc3-sim/**`, `crates/wc3-game/src/sim_bridge.rs`);
`game.rs`/`hud.rs` remain HUD-owned and their two-line integrations are patches.

## Matrix rows

| Matrix row | Before | Now | Evidence |
|---|---|---|---|
| G2 lumber economy | `resources` gold only; HUD `"0"`; no tree | `Sim.lumber[2]`, `UnitStats.lumber_cost`, `Order::HarvestLumber`, tree HP, 10 lumber/trip | `lib.rs:409` (lumber), `:1949` (`harvest_lumber`), `:2704` (cycle), `data.rs` `lumber_cost` |
| Destructable HP | art only | 246 real TFT destructables, HP/armor/model, damage, death frees cell | `destructables_gen.rs` 246 rows, `lib.rs:1966` spawn, `:1992` damage |
| G22 map-placed units | fixed scenario only | `Sim::spawn_map_units(&[(owner, def, pos)])` canonical spawn | `lib.rs:1940` |
| G18 creep camps | none | neutral owner 2 defs (18), `spawn_camp`, aggro 500, leash 900, return-home AI | `creeps_gen.rs` 18 rows, `lib.rs:225` `Camp`, `:1903` spawn, `:2142` AI |
| G5 tech costs | gold only for building placement | lumber checked/deducted for build/train/research path; cancellation refunds both | `lib.rs` `build_instant_impl`, `place_building_impl`, `enqueue_production` |

## Tests (rc 0)

| Command | Result |
|---|---|
| `cargo test -p wc3-sim` | **116 passed / 0 failed / 2 ignored** (`analysis/agent-sim/top35-sim-test.log`) |
| `cargo check -p wc3-game --lib` | **0** (`top35-game-check.log`) |
| `cargo test -p wc3-game --lib` | **91 passed / 0 failed / 18 ignored** (`top35-game-test.log`) |

Named gates:
- `crates/wc3-sim/tests/merge.rs::lumber_cycle_chops_a_tree_and_banks_ten_per_trip`
- `destructable_hp_and_cell_release`
- `lumber_costs_are_enforced_for_training_and_building`
- `creep_camp_aggros_within_500_and_leashes_home`
- `map_placed_units_spawn_with_owner_and_position`
- `crates/wc3-sim/tests/map_units.rs::human04_map_units_spawn_count_matches_parser` —
  parses `war3mapUnits.doo` through `wc3-formats::map::parse_units` (dev-dep) and
  proves `Sim::spawn_map_units` count equals the parsed known-def count.

## HUD patches (HUD-owned, dry-run clean)

- `analysis/agent-sim/patches/hud-lumber.patch` — resource pill reads
  `sim.lumber[player]` instead of `"0"`.
- `analysis/agent-sim/patches/hud-build-menu.patch` — worker card consumes the
  SLK build menu (row #? from the previous round; still clean).

## Remaining (external)

1. HUD applies `hud-lumber.patch` so QA `qa.economy.lumber` sees the value.
2. World/game map loader calls `Sim::spawn_map_units` with `war3mapUnits.doo`
   entries, then `spawn_camp` for neutral camps; game-side test
   `map_units_spawn` is in the world/HUD scope (`PARITY_MATRIX` #4/#5).
3. Creep defs are 18 curated camp families; the full 371 neutral roster is a
   data-layer expansion, not required for the camp mechanics here.

## H2

Ledger 355 checkpointed at start/after implementation/before close; capture and
experience IDs recorded in the close row.
