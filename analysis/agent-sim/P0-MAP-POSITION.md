# P0 — map spawn position root cause (ground plane z → y)

Ledger **355**, layer sim, 2026-11-13. Owner paths: `crates/wc3-game/src/sim_bridge.rs`,
`crates/wc3-sim/**` (exact spawn helper), `analysis/agent-sim/**`.

## Symptom
Device: 3 town halls and mobs with broken/clumped spawn.

## Root cause (observed)
`sim_bridge::spawn_map_scene` built 2D sim positions from `war3mapUnits.doo` /
`war3map.doo` as `Vec2(x, z)`. The sim/game ground plane is `(x, y)` — the engine
keeps `x/y` as ground and `z` as height (`engine.rs:462-478`), and the renderer
uses `Vec3(x, height, y)` (`game.rs:373/849/1752`). Feeding `z` collapsed every
map unit and doodad onto a line at Y≈height, so halls/mobs/trees clumped.

A second, independent defect surfaced by the position gate: `Sim::spawn_def`
snaps non-buildings to the nearest walkable cell, so map units whose parsed cell
was covered by an earlier building footprint were moved (e.g. `nmrm` parsed
placement relocated to `(64, 192)`). Map placements are authoritative.

## Fix (whole functions, owner paths)
| Change | file:line |
|---|---|
| Units use ground `y` | `crates/wc3-game/src/sim_bridge.rs:114` |
| Doodads/trees use ground `y` | `crates/wc3-game/src/sim_bridge.rs:129` |
| Exact spawn without walkable snap (`spawn_def_index_at(..., snap_walkable)`; public `spawn_def_exact`) | `crates/wc3-sim/src/lib.rs:604` |
| Map scene + JASS `adopt` use exact spawns | `sim_bridge.rs` (`spawn_map_scene`, `adopt`) |
| Regression test (rewritten) | `crates/wc3-game/src/sim_bridge.rs:814` `map_units_spawn_from_human04_scene` |

## Regression gate (real map `analysis/war3_map/Maps/Campaign/Human04.w3m`)
Canonical command:

```
cargo test -p wc3-game --lib map_units_spawn --offline -- --nocapture
```

Result **rc 0** — `2 passed; 0 failed; 0 ignored; 124 filtered out` (`analysis/agent-sim/p0-map-units.log`).

Observed scene numbers (`MAP_SCENE` lines):

| Metric | Value |
|---|---|
| raw Human04 placements | **90** |
| def-covered placements | **59** |
| spawned from map scene | **59 / 59** (exact `(parsed.x, parsed.y)`, <1.0 tolerance multiset) |
| Y spread (max−min) | **10927.7** (> 1000 on the 129×129 map — not a line) |
| halls `htow` raw / parsed / sim | **0 / 0 / 0** (no synthetic halls; equal to the map) |
| destructables | **1982** parsed, all sim positions on parsed `(x, y)` |
| creep camps | **20**, every camp unit on its parsed placement and owner 2 |
| unknown defs in raw placements | **31** (e.g. `ngna`, `ngnh`, `sloc`, `ngol`, `nvlw`) |

Assertions: raw==90; spawned==def-covered; exact position multiset match; Y spread
>1000; sim halls==raw `htow`; doodads at parsed positions; camp units at parsed
positions with owner 2.

## Result
**FIXED.** Ground plane is `(x, y)` for units and doodads; map placements spawn at
their exact parsed positions; no synthetic halls. Canonical gate rc 0.

## Risks / Next
- **31 raw placements** use neutral/campaign def ids not present in the curated
  `ROSTER`/`CREEP_DEFS`/`SHOP_DEFS` tables (`ngna`, `ngnh`, `ngol`, `nvlw`,
  `ngni`, …). They are honestly skipped, not replaced; the next data increment is
  to generate the full neutral-unit table so those placements spawn too.
- ROUND8 (`#12` projectiles, `#14` shop) is implemented in `wc3-sim` but its
  integration test was not run under the P0 test discipline; `cargo test -p
  wc3-sim --lib` was rc 0 (24 passed). Next round: run
  `cargo test -p wc3-sim --test projectiles_items` and wire HUD shop/projectile
  rendering.
- No device run in this gate (not granted).
