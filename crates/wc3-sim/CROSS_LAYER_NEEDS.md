# WC3 SIM — economy parity: root causes and cross-layer needs

Layer: SIM (ledger 393). Owner path: `crates/wc3-sim/**`.
Measured 2026-11-15 on the installed `(2)Justice.w3x` (Orc player, Human enemy),
native WPM pathing, `war3map.doo` trees, `ngol` mines.

## 1. Fake income — FIXED (sim)

`Sim::new` defaulted `passive_income: true` and the stipend at
`advance_economy` paid **both players** 1 gold / 15 ticks. That is 2.0 gold/s in
a game where no worker had ever delivered; it masked every harvest defect and
made the HUD lie.

* `crates/wc3-sim/src/lib.rs` — `Sim::new` now `passive_income: false`; the
  field doc states it is opt-in for scripted demo tests.
* `crates/wc3-sim/src/net.rs` — `MatchSetup::melee_1v1` / `MatchSetup::on_map`
  now `passive_income: false` (real matches never use the stipend).
* Test `economy_idle_workers_do_not_create_gold` proves: 300 ticks with idle
  workers, gold and lumber do not move.

## 2. Gather arrival gates — FIXED (sim)

Fixed gates (`pos.distance(mine) <= 64`, tree `<= 72`, hall `<= 210`) are
smaller than a blocked footprint. A worker stops on the nearest **walkable**
cell centre; a 4x4 blocked mine footprint puts that cell 80+ world units from
the centre, so `ToMine` could never be satisfied on maps whose WPM blocks the
mine cell.

* `Sim::blocked_reach(target)` — distance from the target centre to the far
  edge of the blocked cells around it (half a cell diagonal included).
* `Sim::gather_gate(target, stop)` = `max(stop + 8, blocked_reach + 8)`.
* Gold `ToMine`, the Gathering mine lookup, lumber `ToMine` and both hall
  drop-off gates use it.
* Test `economy_gather_targets_blocked_mine_footprint_is_reachable` builds a
  4x4 blocked footprint and proves a peasant still gathers and pays.

## 3. Dropped gather orders — FIXED (sim)

`process_path_requests` set `Order::Idle` on three failure paths with no event.
A gather order whose path fails now goes through `gather_path_failed`:
in-range → `Gathering`; otherwise `Event::Error("gather unreachable")` + Idle.
Test `economy_gather_targets_unreachable_mine_reports_error` walls a mine off
and asserts the event.

## 4. Lumber loop across a felled tree — FIXED (sim)

When the ordered tree died, the worker dropped to `Idle` (measured on Justice:
first tree = five loads, then the loop stopped at 1050 lumber). WC3 retargets
the next tree. `advance_economy` now retargets within
`HARVEST_RETARGET_RADIUS = 600` in both `ToMine` (tree died on the way) and
`Gathering` (tree died mid-chop). Test
`economy_tree_retarget_keeps_lumber_loop` proves +140 lumber across a dead
tree.

## 5. WC3 smart order (worker move onto a resource) — ADDED (sim)

`Command::Move` for a worker whose destination is inside a mine/tree
interaction gate becomes `Harvest` / `HarvestLumber`; non-workers in the same
group still move. Tests `economy_gather_targets_worker_move_onto_{mine,tree}_gathers`.

## 6. AI economy — FIXED (sim)

The AI sent every idle worker to the nearest mine and never chopped lumber, so
its build order (farms/barracks/towers cost lumber) could not complete.

* `MINE_SATURATION = 5` — at most five workers per live mine.
* Surplus crew chops the nearest tree within 1500 units (`lumber_target =
  max(2, workers/3)`); workers beyond both targets idle near the hall.
* Test `ai_economy_lumber_and_mine_saturation`: 12 workers, 60 s →
  gold_crew 5, lumber_crew 4, gold +1080, lumber +840.

## 7. Build/train id resolution — VERIFIED (sim)

Gap report said `otto`/`otol` were missing. Measured: `otto` = Orc Tauren Totem
resolves (`roster_gen.rs:2958`); `otol` is not a WC3 type id (the Orc peon
`Builds=` line is `ogre,otrb,obar,ofor,oalt,obea,osld,otto,owtw`). Test
`data_build_train_resolution_every_race` asserts every build-menu entry and
every `trains`/`trained_at`/`requires`/`sells_items` id resolves through
`data::def_of` for all four races;
`data_build_train_resolution_orc_peon_all_nine_builds` spawns and completes all
nine Orc peon build entries.

## 8. Starting units from the map's w3i race — GAME LAYER (already correct)

`wc3_game::sim_bridge::spawn_melee_bases` resolves each seat's race from the
`sloc` creator slot's w3i player record and logs
`GAME_MELEE_RACE ... source=w3i` (observed on Justice: owner 0 slot 1 Orc,
owner 1 slot 0 Human). `wc3-sim::setup_melee*` takes explicit races, so no
4-kind enum reaches a starting base.

Cross-layer note (wc3-game, read-only here): the melee **fallback** in
`spawn_melee_scenario` (`game.rs`, `for (owner, base, hall_def) in
[(player,"ogre"), (enemy,"htow")]`) hard-codes Orc/Human when the map has no
two `sloc` markers. It only runs when `spawn_melee_bases` returned `None`, so
Justice is unaffected, but a sloc-less custom map would ignore w3i.

## Measured rates (60 s, passive drip off, Justice)

| crew | gold | lumber |
|---|---|---|
| 1 worker | +70…80 (1.17–1.33 /s), 7–8 trips × 10 | +100…130 (1.7–2.2 /s), 10–13 trips × 10 |
| 5 workers | +320 (5.33 /s) | +420 (7.0 /s) |

Touch path on the real install (`crates/wc3-game/tests/touch_playability.rs`,
read-only): tap mine → `Harvest GOLD phase=ToTarget`, first deposit at tick 232
(+10); tap tree → `HarvestLumber phase=ToTarget`, first deposit at tick 149
(+10); both suites pass.

## Mechanic targets the runner should list (analysis/tech-lead, read-only here)

* `economy.gold` → `cargo test -p wc3-sim --offline --test economy`
* `economy.lumber` → `cargo test -p wc3-sim --offline --test economy`
* `melee.matrix` → `cargo test -p wc3-sim --offline --test roster`
* `ai.economy` → `cargo test -p wc3-sim --offline --test ai`
