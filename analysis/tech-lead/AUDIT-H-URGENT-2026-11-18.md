# URGENT to tech-lead — Lane H (ledger 528): three P1 defects on the live mission script path
2026-11-18 · audited worktree `d7af459` + uncommitted `crates/wc3-re/{engine.rs,jass/natives.rs,jass/vm.rs}` · probe outputs in `analysis/full-audit-2026-11-18/probes/`

Every item below is proven by a one-shot probe and a source line. All three sit on the
path the product actually runs (`Game::reset_map_runtime` → `Engine::enter_map` →
`Runtime::tick_script`); the `wc3-sim` JASS VM is **not** attached in the game
(`Sim::attach_script` has no non-test caller).

## H-P1-1 — `war3map.j` `config` is never executed
- Source: `crates/wc3-re/src/engine.rs:305` calls `runtime.run_main()` only.
  `Runtime::run_main` = `run_function("main")` (`crates/wc3-re/src/jass/vm.rs:1116-1118`).
- Retail: `Game.dll` runs `config` first, then `main` — `analysis/agent-re/formats/ENGINE_MAP.md`
  §4.1 (observed disassembly): `0x6f2ccf00 RunConfig: CallByName(script,"config",…)`,
  caller `0x6f28dc58`; `0x6f2ccf30 RunMain`, caller `0x6f28ec7c`; sequence at map start
  "run config → start-up/first-frame → run main".
- Probe `audit_h_config` (Human04): `after_boot config_calls=None`, every player
  `name="Player N" controller=1 start=0 team=0`. After the manual `run_function("config")`:
  slot 1 `controller=0` (`MAP_CONTROL_USER`), `start=1`, slot 0 `controller=3`, slot 5
  `team=1`.
- Impact: `InitCustomPlayerSlots`/`InitCustomTeams`/`SetMapName`/`DefineStartLocation`
  never run. `SetPlayerController(...,MAP_CONTROL_USER)` never marks the seat, so the
  documented seat fallback `crates/wc3-game/src/campaign.rs:3329-3333`
  ("slot the script marked MAP_CONTROL_USER") is dead code; campaign falls back to the
  `sloc`/camera heuristic or to `war3map.w3i`. Player names/teams/start locations are
  defaults for every script/trigger that reads them.
- Fix: in `enter_map`, run `config` before `main` (same runtime, same thread), then `main`;
  surface a parse/VM error for either.

## H-P1-2 — death / region / game-state / variable / dialog trigger events are registered but never dispatched
- Registrations exist: `crates/wc3-re/src/jass/natives.rs:1412` (`Death`), `:1420`
  (`EnterRegion`), `:1428` (`LeaveRegion`), `:1465` (`GameState`), `:1475` (`Variable`),
  `:1483/:1492` (`Dialog`), `:3034/:3043/:3058` (state/unit/custom).
- Dispatch: `Runtime::fire_unit_event` (`vm.rs:1748-1798`) matches only `PlayerUnit`,
  `UnitEvent`, `Player`. `grep 'TriggerEvent::' crates/wc3-re/src` shows `Death`,
  `EnterRegion`, `LeaveRegion`, `GameState`, `Variable`, `Dialog`, `DialogButton`,
  `Custom` have **no** consumer anywhere. `IsPointInRegion` / `IsUnitInRegion` are
  hardcoded `false` (`natives.rs:1277-1278`).
- `fire_unit_event` itself has only spell/item callers (`vm.rs:1811,1931,1975-1976`);
  no `wc3-game` call site (`grep` across `crates/wc3-game/src` = 0). The sim bridge
  mirrors unit state (`sim_bridge.rs:1326+`) but never raises a death/region event.
- Impact: every campaign quest/death/region trigger is inert. Prologue01/Human01-style
  maps gate quests on units entering regions and on unit death; those never run.
- Fix: tick a region/death diff in the bridge (positions already mirrored) and call
  `Runtime::fire_unit_event` / a new `fire_region_event` / `fire_death_event`; match the
  existing `TriggerEvent` arms in one dispatcher.

## H-P1-3 — `TriggerSleepAction` inside a called function does not suspend the caller
- Source: `vm.rs:1437-1441` checks the runtime-wide `pending_sleep` after every op; a
  `call inner()` compiles to `Op::Call` → `call_function` → `run_entry_index` → **new
  Thread** (`vm.rs:1321-1340`, `1180-1197`). The inner thread consumes `pending_sleep`
  and is parked; the caller's loop continues.
- Probe `audit_h_sleep`: `after main: stage=3 parked=1`; `after advance(2.0): stage=2`.
  Retail JASS has one thread per activation: the whole chain waits, so the expected
  order is inner post-wait (`2`) then caller post-call (`3`) → final `3`.
- Impact: any cutscene/quest helper that waits and is called from `main`/a trigger action
  returns immediately; the caller's next statements run before the wait, and the helper's
  post-wait writes land later out of order. This is the class of defect that makes an
  opening play "partially" (camera beats fire, sequence state wrong).

## Also confirmed (report will carry them as P2/P3)
- Parser recursion has no depth limit: 20 000 nested `(` = SIGSEGV (rc 139, 8 MB main
  stack); the game already works around it with a 32 MB scoped thread
  (`crates/wc3-game/src/game.rs:1759-1777`). Untrusted `war3map.j` can still abort.
- Non-ASCII JASS strings: lexer byte→Latin-1 (`lexer.rs:105-107`), `StringLength` =
  UTF-8 bytes (24 for a 6-char word), `SubString` byte slice panics at
  `natives.rs:157`. Release has `panic = "abort"` (`Cargo.toml:50-54`), so a localized
  map that slices text aborts the APK.
- `SetPlayerAlliance` is a no-op arm (`natives.rs:323-326`), `IsUnitAlly`/`IsUnitEnemy`/
  `IsUnitOwnedByPlayer`/`IsUnitType` are always `false`, `IsUnit`/`IsUnitInRange` always
  `true` (`natives.rs:852-858`). Human04 calls `SetPlayerAllianceBJ` 39×.
- Array index unbounded (`vm.rs:2064-2090`, BTreeMap): 200 000 entries from a 200k loop;
  negative and >8191 accepted.
- `MapBundle::open` eagerly reads every listfile entry (`mapfiles.rs:98-117`), up to the
  MPQ 512 MB per-file cap.
- Non-ASCII JASS string literals are byte→Latin-1 mojibake (`lexer.rs:105-107`): UTF-8
  `"Привет"` becomes `"Ð\u{9f}Ñ…"`.
- `max_steps` is a runtime-lifetime counter (`vm.rs:1015`, `engine.rs:304` 50 M): measured
  boot 15 335 ops, 600 ticks +2 306; after the cap every later entry fails permanently.

Contact: lane H auditor (ledger 528). Full report: `analysis/full-audit-2026-11-18/lane-H-data-jass.md`.
