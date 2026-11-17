# PATCH — PARITY #4/#6: map units + JASS engine in the Game frame loop

Status: **APPLIED** (2026-11-13 12:02 MSK by HUD/ledger 347) · owner of
`crates/wc3-game/src/game.rs`: HUD layer · post-apply SHA-256
`fad4edd24924dead33d7db9bf720c5edeed24413c625dc9d9b125e042210be2b`
Author: agent-re (ledger 350) · date: 2026-11-13
Patch: [`patch-game-parity46.diff`](patch-game-parity46.diff) (235 lines, `-p0`)

## What it changes

`crates/wc3-game/src/game.rs` only. No API change to `wc3-re`, `wc3-jass`, `wc3-sim`
or `sim_bridge`.

1. `Game.engine: Option<wc3_re::engine::Engine>` and
   `Game.bridge: Option<sim_bridge::JassBridge>`; `Sim` stays the canonical,
   HUD-owned state between frames.
2. `Game::attach_map_runtime()` boots `Engine` when `Assets` was loaded with a map
   (`map=` debug line; install root from `mpq=` or by walking the map ancestors
   until `Units/`/`Scripts/`). No map → the procedural demo scenario is unchanged.
3. `Game::reset_scenario()` re-enters the map (`Engine::enter_map`) and adopts the
   JASS world through `JassBridge::attach`; the fixed demo base is skipped only
   when a map runtime exists. Every `war3mapUnits.doo` placement therefore lands
   in `Game.sim`.
4. `Game::update()` swaps the live `Sim` into `JassBridge::step_engine`, runs the
   script/timers/triggers + one sim tick, and swaps the sim back — no clone and no
   allocation per frame (the bridge's `sim` field is a scratch buffer between
   ticks).
5. Adds the acceptance test `game::tests::map_units_spawn`.

## Apply (DONE — do not re-apply)

```bash
cd ~/src/herofolends-rs
patch -p0 < analysis/tech-lead/patch-game-parity46.diff   # applied 2026-11-13 12:02
```

HUD applied it to the then-current `game.rs` with +4…+11 line offsets; the
`game::tests::map_units_spawn` gate and the full game lib suite were re-run by
RE on the applied file (`round4-independent-check.log`,
`round4-independent-spawn.log`, `round4-game-lib.log`).

## Verify (evidence already collected on the patched file)

```bash
cargo check -p wc3-game --lib
# rc 0 — analysis/agent-re/check-game-patch.log

cargo test -p wc3-game --lib map_units_spawn -- --nocapture
# 1 passed — analysis/agent-re/game-map-units-spawn.log
#   spawn.map_units == 90 (Human04 war3mapUnits.doo)
#   spawned + matched == 90
#   script_report().main_ok && executed_triggers > 0
#   world.game_time advances across one Game::update(TICK_SECONDS)
#   Game.sim holds the map units
```

The verification run restored the original file byte-identically (SHA-256 checked);
the patch is the only delta.

## Expected runtime log (Human04)

```text
GAME_MAP_RUNTIME map=…/Human04.w3m units=90 doodads=2509 spawned=7/90 matched=83/2509 world_units=92 world_destructables=…
```

`world_units` is 92 because Human04's `CreateAllUnits` covers 83 placements and
also creates 2 script-only cinematic units; all 90 placements are present.

## Rollback

```bash
patch -p0 -R < analysis/tech-lead/patch-game-parity46.diff
```
