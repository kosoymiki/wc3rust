# P1 items gap — data-backed item abilities, shop gating, recipe engine

Ledger **355** (`r2-sim`), 2026-11-13. Owner paths: `crates/wc3-sim/**`,
`crates/wc3-game/src/sim_bridge.rs`, `analysis/agent-sim/**`.

## Gap
`items.gaps` was a hard-coded GAP (1/1) with metrics `ability_items=240`,
`shop=absent`, `recipes=absent`; `items.runtime` also lost a check once item
abilities were routed.

## Root fixes

| Mechanism | Implementation | Provenance |
|---|---|---|
| Item abilities | `GenericAbility` table extended from roster **and** item `abilList` ids: 360 rows, **115 active castable**. `use_item_on` falls back to `Sim::cast_data_ability` for active item abilities (charge consumed on success) | `abilities.rs:54,741`; `abilities_gen.rs`; `gen_abilities.py` |
| Multi-ability items | `abilList` comma lists now resolve to the first real ability id (previously stored `"Arel,AIl1"` verbatim and missed the sheet) | `gen_items.py`; `items_gen.rs` (273 rows) |
| Shop purchase | `buy_item` enforces `Requires=`/`DependencyOr=` building requirements, debits gold, consumes stock, sets charges and restock timer | `lib.rs` `item_requirements_met` + `buy_item` |
| Requirements table | 18 real `ItemFunc.txt` rows with `DependencyOr` alternatives (`TWN2` → hkee/ostr/etoa/unp1) | `item_reqs_gen.rs` (18 rows); `gen_item_reqs.py` |
| Recipes | `Sim::combine_items` engine (ingredient validation, bonuses, single result) driven by generated `RECIPES`; retail ships **0** item→item `Requires` rows, so the table is empty and the engine refuses instead of fabricating recipes | `lib.rs combine_items`; `item_reqs_gen.rs` (`RECIPES = []`) |

## Evidence (canonical gates)

Command (QA parity harness, real asset roots):
```
cd scripts/qa/parity && cargo run --offline -- \
  --assets analysis/war3_full --maps analysis/war3_map/Maps \
  --tft analysis/war3x_data --out analysis/agent-sim/p1-items-parity.json
```
Result: **rc 0 — all 24 gates PASS** (`analysis/agent-sim/p1-items-parity.log`).

| Gate | Before | After |
|---|---|---|
| `items.gaps` | **GAP 0/1** | **PASS 4/4** |
| `items.runtime` | PASS 7/7 (regressed then fixed) | **PASS 7/7** |
| `items.data` | PASS 273/273 | **PASS 273/273** |
| `items.schema` | PASS 2/2 | **PASS 2/2** |
| `cast.coverage` | PASS 154/154 | **PASS 154/154** |
| all other gates | — | **PASS (24/24 harness)** |

`items.gaps` metrics after: `ability_items=240`, `ability_covered=240`,
`active_castable=115`, `shop=ngme/stwp TWN2 gating + stock/charges`,
`requirements=18`, `recipe_rows=0`, `recipes=0 retail rows; Sim::combine_items
engine present`.

## Tests

`cargo test -p wc3-sim --test items_round9` → **rc 0, 4 passed**:
- every item with `abilList` has `AbilityData` numbers (240/240);
- shop: stwp denied without TWN2, allowed after `hkee`, gold −350, stock 2→1, charges 1;
- `pdiv` (AIdv, 1800-tick cooldown) used through the data-backed item ability path, last charge removes the item;
- `Requires`/`DependencyOr` rows = 18 and `RECIPES = []` with `combine_items` refusing (`no recipe data`).

`cargo test -p wc3-sim --test projectiles_items` → **rc 0, 5 passed** (shop test
now spawns the required `hkee`).

## Result
**DONE** — item gameplay implemented at the sim root and the gate is a real
check, not a hard-coded GAP; harness rc 0.

## Risks / Next
- Retail has **zero** recipe rows; `combine_items` is a ready engine with an
  empty generated table. Custom-map recipes would require map object data or
  triggers, not `ItemFunc`.
- Generic item ability effects use the shared data heuristic (self/no-target
  abilities pay cost/cooldown; heals/damage where `DataA1` is numeric); special
  item effects (teleport, summon) still resolve to hand-written or script paths.
- Shop requirement gating checks completed buildings only (WC3 upgrade levels
  beyond building id are not yet modelled).
