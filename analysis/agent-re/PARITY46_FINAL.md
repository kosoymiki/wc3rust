# PARITY #4/#6/#7 — map runtime, final report

Date: 2026-11-13 · ledger **350** (`r2-re`, agent-re) · owner paths:
`crates/wc3-re/**`, `crates/wc3-jass/**` (+ the unit parser in
`crates/wc3-formats/src/map.rs`; `game.rs` delivered as a verified patch).

## Result

| gate | rc | artifact |
|---|---:|---|
| `cargo check -p wc3-re -p wc3-jass` | 0 | — |
| `cargo test -p wc3-re -p wc3-jass` | 0 | `analysis/agent-re/test-final.log` — **44 lib** + 5 `merge_parity` + 1 `re_surface` + 2 `real_corpus` + 3 `runtime_wiring` + 6 `wc3-jass` |
| `cargo check -p wc3-game --lib` | 0 | `analysis/agent-re/check-game-r3.log` |
| `wc3re scan analysis/war3_full3` | 0 | `analysis/agent-re/scan-final.json` — `ok=true`, 44 maps, **0 errors**, `scripts.unimplemented={}`, 44/44 `main`, 791 natives, **1 313 713** VM steps, 4 428 WTS |
| `wc3re parity analysis/war3_full3/Maps` | 0 | `analysis/agent-re/qa-jass-map-runtime.json`, `MAP_RUNTIME.md` — 44/44 maps, **713** executed triggers, **6 723** placements, 468 engine-spawned, 6 255 matched, 115 134 doodads |
| `python3 analysis/agent-re/qa_map_runtime.py` (`qa.jass.map_runtime`) | 0 | `analysis/agent-re/qa-jass-map-runtime-gate.json` |
| patched `cargo check -p wc3-game --lib` | 0 | `analysis/agent-re/check-game-patch.log` |
| patched `cargo test -p wc3-game --lib map_units_spawn` | 0 | `analysis/agent-re/game-map-units-spawn.log` — 90 placements, `main_ok`, executed triggers > 0, `world.game_time` advances in `Game::update` |
| `game.rs` restored after verification | — | SHA-256 identical (`7e3c0b9d…`) |

## Phase 1 — `war3mapUnits.doo` → sim (PARITY #4)

- `wc3_formats::map::parse_units` aligned with HiveWE `src/base/units.ixx`
  (observed 2026-11-13): `random_type` payload (`0`=4 B, `1`=8 B,
  `2`=count·8 B, other=0 B), item table + STR/AGI/INT for `subversion >= 11`,
  group/roll/pitch/lights for `version >= 13`; versions 7–13 accepted.
- Human04 (v7/sub9, `random_type=0xFFFF_FFFF`) now parses **90/90** placements
  and ends exactly at file length; every map's unit table parses (6 723
  placements, min 2 / max 291 per map). `MapBundle.errors` is empty for Human04.
- `Engine::map_spawn()`/`MapSpawnReport` (`map_units`, spawned/matched units and
  doodads); placements spawn into the JASS `World` with type+position
  de-duplication against `CreateAllUnits` (468 spawned, 6 255 matched).
- `Engine::map()` gives callers the parsed `MapBundle`.
- Tests: `engine::tests::spawns_map_units_and_doodads_into_jass_world`;
  `merge_parity::map_bundle_reads_terrain_units_objects_and_wts` (90 + 0 errors);
  `runtime_wiring::host_launch_mounts_storm_and_runs_map_script` (every placement
  is present at its type+position in the JASS world).

## Phase 3 — natives (PARITY #7)

`wc3re scan` `scripts.unimplemented = {}` — from 92 names / 3 073 calls to **0**.
Top-weight natives mutate the deterministic `World`; UI/camera/audio/weather/
campaign controls are explicit no-ops. Nine per-native gates in
`jass::natives::parity_tests`:

`TriggerRegisterPlayerStateEvent`, `TriggerRegisterUnitInRange`,
`QuestCreateItem`, `SetResourceAmount`/`GetResourceAmount`,
`AttachSoundToUnit`, camera-bound getters, fog/weather no-ops,
`SetUnitMoveSpeed`, `ForForce` + `GetEnumPlayer` (`GetEnumPlayer` reads the
`ForForce` context and restores it).

## Phase 2 — `Engine` + `JassBridge::step_engine` in `Game::update` (PARITY #6)

`crates/wc3-game/src/game.rs` is HUD-owned; delivered as
[`patch-game-parity46.diff`](../tech-lead/patch-game-parity46.diff), with the
apply/verify/rollback steps in
[`PATCH-GAME-PARITY46.md`](../tech-lead/PATCH-GAME-PARITY46.md).

- Boots `Engine` from the `Assets` map provenance (`map=` line; install root
  from `mpq=` or map ancestors).
- `reset_scenario` re-enters the map and adopts the JASS world through
  `JassBridge::attach`; the fixed demo base is skipped only for a map runtime.
- `update` swaps the canonical `Sim` into `JassBridge::step_engine` each tick
  and back — no clone or allocation per frame, no `sim_bridge` API change.
- Patched `map_units_spawn` (Human04): 90 placements, `spawned+matched == 90`,
  `main_ok`, executed triggers > 0, `world.game_time` advances across one
  `Game::update(TICK_SECONDS)`, sim holds the map units. `game.rs` was restored
  byte-identically after the check.

## Phase 4 — map script artifact + tests

- `wc3re parity` (new CLI): per-map `main_ok`, functions, triggers,
  trigger/executed actions, placements, spawned/matched, world units/doodads →
  `qa-jass-map-runtime.json` + `MAP_RUNTIME.md`.
- `qa_map_runtime.py` is the device-free `qa.jass.map_runtime` gate
  (`spawned+matched == placements`, `main_ok`, triggers, world coverage).
- `wc3re map`/`scan` unchanged in contract; the scan JSON is the native/VM
  census (`scan-final.json`).
