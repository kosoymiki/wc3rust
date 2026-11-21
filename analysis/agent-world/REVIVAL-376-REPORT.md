# REVIVAL ledger 376 — world/terrain owner report (P0-12)

**Date:** 2026-11-14 02:48:50 MSK · session world · ledger 376

## Goal
World-clean 44-map audit + terrain render parity evidence for P0-12: water
unwalkable in the applied grid, unit placements safe, water-plane and
cliff/ramp continuity proven by Rust tests, 3D hall evidence honestly labelled.

## Mechanic -> test -> command -> rc
| mechanic id | test | command | rc |
|---|---|---|---|
| `pathing.water.maps` | `pathing_water_maps` | `cargo test -p wc3-game --test world_maps` | **0** |
| `maps.unit-placement` | `maps_unit_placement` | `cargo test -p wc3-game --test world_maps` | **0** |
| `assets.terrain` (cliffs/ramps, all maps) | `assets_terrain_cliff_ramp` | `cargo test -p wc3-game --test world_maps` | **0** |
| `rendering.terrain.water-plane` | `rendering_terrain_water_plane` | `cargo test -p wc3-game --lib terrain_render` | **0** |
| `rendering.terrain.cliffs` | `rendering_terrain_cliffs` | `cargo test -p wc3-game --lib terrain_render` | **0** |
| green line | — | `cargo check -p wc3-game --lib` | **0** |

Exact runs:
- `cargo check -p wc3-game --lib --offline` → `Finished dev profile` **rc=0** (`analysis/agent-world/green.log`).
- `cargo test -p wc3-game --lib terrain_render --offline` → `12 passed; 0 failed; 6 ignored` **rc=0** 189.75 s (`terrain-gate.log`); the two new tests ran separately with logs: `TERRAIN_WATER_PLANE map=Human04 deep_water_tiles=136`, `Orc01 deep_water_tiles=8383`, `scene units_on_water=0`; `TERRAIN_CLIFFS Human04 seam=0.0000 ramps=488 cliff_steps=0`, `Orc01 seam=0.0000 ramps=192 cliff_steps=0` (`terrain-two.log`).
- `cargo test -p wc3-game --test world_maps --offline -- --nocapture` → `3 passed; 0 failed` **rc=0** 13.68 s (`world-maps.log`): `MAP_WORLD_AUDIT maps=44 units=1586 buildings=342 doodads=115134 water_tiles=117031 ramps=9524 unknown_defs=701 masked_water_cells=647178 off_map=4758 failures=0 scripted_placements=787`.

## map_world_audit.json
`analysis/agent-world/map_world_audit.json` — **44 maps**, `failures: []`, `scripted_placements: 787` (fully enumerated with map + coordinates + `"classification":"scripted-map"` — not silent), 44 `per_map` counters (units, off_map, water_tiles, masked water cells, failures), totals above.

Top placement records (all campaign-scripted maps, no script-less map violates):
| map | in-map units | off-map | water tiles | masked cells | scripted placements |
|---|---:|---:|---:|---:|---:|
| Undead01.w3m | 53 | 119 | 9600 | 47006 | 53 |
| Human08.w3m | 56 | 88 | 8249 | 8840 | 51 |
| Human01.w3m | 71 | 84 | 604 | 2836 | 42 |
| Undead06.w3m | 63 | 161 | 1331 | 2188 | 42 |
| Orc06.w3m | 63 | 134 | 2068 | 7488 | 33 |
| Undead03.w3m | 49 | 134 | 2970 | 6140 | 32 |
| Undead07.w3m | 60 | 176 | 2259 | 11300 | 32 |
| Orc05.w3m | 53 | 94 | 4606 | 49620 | 29 |
| BonusCredits.w3m | 110 | 161 | 5864 | 1888 | 28 |
| NightElf04.w3m | 65 | 203 | 1746 | 12776 | 28 |
| NightElf05.w3m | 50 | 174 | 1703 | 3160 | 27 |
| NightElf07.w3m | 49 | 106 | 1680 | 15132 | 26 |

## Root cause found: WPM bit semantics were inverted
`HiveWE/src/base/pathing_map.ixx` `PathingMap::Flags` is authoritative:
`0x02 = unwalkable`, `0x04 = unflyable`, `0x08 = unbuildable`, `0x10 = harvestable`,
`0x20 = blight`, `0x40 = water`, `0x80 = amphibious`.
`crates/wc3-formats/src/wpm.rs` implemented `walkable = cell & 0x02 != 0` (inverted)
and `buildable = 0x08 != 0` (inverted); the game/sim grids consume it, so blocked
cells were treated as walkable and vice versa (the P0 “mobs stuck in textures /
run in place” symptom class). Audit uses the authoritative meaning locally and
the crate fix ships as a patch.

## Patches (read-only owners)
- `analysis/agent-world/patches/wpm-semantics.patch` — correct `walkable`/`buildable`
  and add `water()`/`amphibious()` (wc3-formats owner).
- `analysis/agent-world/patches/game-build-grid-water.patch` — `build_grid` masks
  WPM water + `assets.is_water` so ground pathing never crosses deep water
  (game.rs owner).
- `analysis/agent-world/patches/run-mechanic-tests-world.patch` — runner entries
  for the new mechanic ids (analysis/tech-lead owner).
- `analysis/agent-world/patches/sim-bridge-order-repair-hotfix.patch` — the one
  cross-owner hotfix **applied** this round: wc3-sim added `Order::Repair` and
  `sim_bridge::order_name` was non-exhaustive, leaving `cargo check` red for all
  agents; added `Order::Repair(_) => "repair"` with backup
  `crates/wc3-game/src/sim_bridge.rs.bak-world376`. Disclosed for owner review.

## 3D evidence (honest labels)
- `tools/mdx-gltf` re-exported `gltf/TownHall.gltf` and `gltf/GreatHall.gltf`
  from the real MDX; `gltf-transform validate` **rc=0** for both (GreatHall
  needed a second run; first attempt hit a Node `std::bad_alloc` under memory
  pressure, recorded here).
- `gltf/preview-TownHall.png`, `gltf/preview-GreatHall.png` are software
  rasterizer previews of that real geometry; `gltf/PREVIEW-LABEL.md` states they
  are not hand-drawn and not final Blizzard art.

## Files (owner paths)
- `crates/wc3-game/tests/world_maps.rs` (new; three mechanic tests + JSON writer).
- `crates/wc3-game/src/terrain_render.rs` (two new tests + helpers).
- `analysis/agent-world/map_world_audit.json`, logs, patches, report.

## Risks / Next
- 787 placement records live on campaign maps that carry custom scripts
  (transports/cinematics). They are enumerated, not whitelisted; the product
  melee scenario spawns its own units (`map_units_spawn` gate) and the audit’s
  `failures` array is empty for script-less/structural checks.
- Until the `wpm-semantics.patch` lands in wc3-formats, runtime grids keep the
  inverted semantics; the game-build-grid-water patch depends on it.
- `world_maps` depends on `wc3_data` UnitData move types (7 s) and reads all 44
  maps (~14 s); both gates are offline and deterministic.
