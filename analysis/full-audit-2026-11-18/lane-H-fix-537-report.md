# LEDGER 537 — FIX lane H: JASS / engine (the three live-mission P1s)

2026-11-18 · repo `~/src/wc3rust` · owner paths `crates/wc3-re/src/engine.rs`,
`crates/wc3-re/src/jass/**` · no `wc3-game/**`, `wc3-sim/**`, `wc3-formats/**` edits ·
no new tests, no device access.

Input: `analysis/full-audit-2026-11-18/lane-H-data-jass.md`,
`analysis/tech-lead/AUDIT-H-URGENT-2026-11-18.md`.

## What changed

### P1-1 `config` is executed before `main` (WC3-JASS-001)

`Engine::enter_map` now runs the map's `config` entry point on the same runtime and
the same thread, before `main`, and reports it:

- `Runtime::has_function` (new) distinguishes "the map has no `config`" (legal) from
  "`config` failed".
- `ScriptReport::config_ok` / `Engine::script_config_ok` record the outcome.
- Boot line: `ENGINE_SCRIPT config=… main=… functions=… triggers=… actions=… boot_triggers=… steps=…`.
- Retail provenance: `Game.dll` `RunConfig` 0x6f2ccf00 (caller 0x6f28dc58),
  `RunMain` 0x6f2ccf30 (caller 0x6f28ec7c) — `analysis/agent-re/formats/ENGINE_MAP.md` §4.1.

### P1-2 event dispatcher + engine-tick sampler (WC3-JASS-002)

`crates/wc3-re/src/jass/vm.rs`:

- Every registered `TriggerEvent` now has a consumer:
  `fire_region_event` (`EnterRegion`/`LeaveRegion`), `fire_death_event` (`Death`,
  `PlayerUnit(EVENT_PLAYER_UNIT_DEATH=20)`, `UnitEvent(EVENT_UNIT_DEATH=53)`,
  `Player(20)`), `fire_game_state_event`/`notify_state_change` (`GameState`),
  `fire_variable_event`/`notify_variable_change` (`Variable`), `fire_dialog_event`,
  `fire_dialog_button_event`, `fire_game_event` (`Custom { event }`, the
  `gameevent` id), `fire_player_event`.
- One dispatcher (`dispatch` + `triggers_matching`) executes the matched handles and
  records `event_fires[kind]`; `Runtime::event_summary()` prints them.
- `Runtime::update_world_events()` is called from `tick_script`: it diffs region
  membership (`region_members`, rect + cell point test), unit liveness
  (`dead_seen`, one event per death) and the day/night clock crossings.
- `TriggerEvent::Custom` now carries the `gameevent` id; `GameState` carries the
  registering player (`-1` = `TriggerRegisterGameStateEvent`).
- `EventContext::region` + `GetTriggeringRegion`, real `IsPointInRegion` /
  `IsUnitInRegion`.
- `KillUnit` raises death; `DialogDisplay` raises the dialog event; `SetPlayerState`
  raises player-state limit crossings; `SetTimeOfDay` / `SetFloatGameState(
  GAME_STATE_TIME_OF_DAY)` jump the clock; `SetTimeOfDayScale` sets the rate.
- Day/night clock advances in the engine tick (one 24 h cycle per 480 s at scale 1.0),
  which is what makes the 14 `TriggerRegisterGameStateEventTimeOfDay` registrations
  in Prologue01 fire as the clock crosses their limits.

Two suppression defects found while wiring this (ALWAYS-FIX, not in the audit):

1. **`GetTriggeringTrigger()` was null inside trigger conditions.**
   `execute_trigger_depth` published `context.trigger` *after* evaluating the
   conditions, so the canonical JASS guard `IsTriggerEnabled(GetTriggeringTrigger())`
   was false for every registration that used it. Publishing the handle before the
   condition pass lifts the executed-trigger count from 19 to **34** (60 s run).
2. **Script owners were lost to the sim bridge.** `wc3-game::sim_bridge` mirrors the
   simulation owner into `Unit::owner`, squashing seats outside its own map onto 0,
   so `Player(PLAYER_NEUTRAL_AGGRESSIVE)` (12) death registrations never matched.
   `Unit::script_owner` (new) keeps the seat the script authored
   (`CreateUnit`/`SetUnitOwner`/map placement); `Runtime::owner_of` is the single
   value function shared by `GetOwningPlayer`, `GroupEnumUnitsOfPlayer`,
   `GroupEnumUnitsOfTypeCounted` and the event sampler. The seven
   neutral-aggressive quest-death triggers now execute
   (`Track_Creep_Deaths`, `A2_MoreGnolls_Done`, `M1_FoundMurlocs_Done`,
   `M2_UseChainLit_Done`, `O1_SleepingOgre_Done`, `G1_Golem_Done`, `T1_Trolls_Done`).

### P1-3 nested `TriggerSleepAction` parks the whole call chain (WC3-JASS-003)

`Op::Call` now pushes a frame onto the *current* thread for a script callee instead of
starting a new thread (`crates/wc3-re/src/jass/vm.rs`). `Frame::discard_result` keeps a
`call` statement from pushing a value. A `TriggerSleepAction` inside a called function
suspends the caller with it and resumes in retail order. `MAX_CALL_DEPTH` (512) caps
the inline chain; `call_function`/`run_entry_index` still start new threads for the
true thread boundaries (`TriggerExecute`, timer callbacks, `run_function`).

Probe `audit_h_sleep`: `after main: stage=1`, `after advance(2.0): stage=3` — the
retail order (callee post-wait 2, then caller 3).

### P2 fixes (same files)

- **`SubString` bounds panic (WC3-JASS-004).** `SubString` slices by character index
  with clamping, `StringLength` counts characters, `StringHash` hashes one char per
  JASS byte, `r2sw` clamps width/precision to 64. The lexer decodes a string literal
  as UTF-8 when the bytes are valid, otherwise byte→`char` (CP1251/Latin-1 single-byte
  scripts), so character indices are the retail byte indices for a single-byte script
  and the real text for a UTF-8 one.
  Probe: `StringLength("Привет")=6`, `SubString("Привет",0,1)="П"`, no panic.
- **Parser recursion (WC3-JASS-005).** `Parser::depth` bounds expression/block
  nesting at 256; a crafted script now returns
  `ParseError { "nesting deeper than 256 levels" }` instead of SIGSEGV at 20 000 `(`.
- **Unbounded array index (WC3-JASS-006).** `get_array`/`set_array` paths reject
  `index < 0 || index >= 8192` through the VM error channel (first 8 recorded,
  counted in `array_bounds_hits`); the map stays bounded.
  Probe: `a[-5]=None a[9000]=None`, growth capped at `len=8192`.

## Evidence (observed)

`scripts/h2-cargo check -p wc3-re --lib` → rc=0.

`scripts/h2-cargo run -q -p wc3-game --example campaign_report -- --mission Prologue01 --frames 120`

```
ENGINE_SCRIPT config=ok main=ok functions=1380 triggers=183 actions=183 boot_triggers=9 steps=16323
ENGINE_EVENTS first_tick=21 distinct=11 regions_with_units=17/65 sample=enter_region:0x2b4:Trig_Victory_Found_Medivh_Actions enter_region:0x348:Trig_S2_MoveThrall_Done_Actions …
ENGINE_TICK t=1.00 triggers_executed=11/183 actions=11 events=enter_region=21,game_state=1 vetoes=30 array_bounds=0 threads=1 units=66
ENGINE_TICK t=4.00 triggers_executed=11/183 actions=14 events=enter_region=21,game_state=1 vetoes=57 array_bounds=0 threads=1 units=66
CAMPAIGN_MISSION_STATE id=Prologue01 script_booted=true time=4.00 victory=false defeat=false completed=false
```

Longer windows (same command, `--frames` only):

| frames | mission time | triggers executed | actions | events |
|---|---|---|---|---|
| 120 | 4 s | 11/183 | 14 | enter_region=21, game_state=1 |
| 600 | 20 s | 17/184 | 36 | + game_state=3 |
| 900 | 30 s | **32/184** | 61 | + death=7 |
| 1200 | 40 s | **34/184** | 73 | death=7 |
| 1800 | 60 s | 34/184 | 95 | death=13 |

Audited baseline (`lane-H-data-jass.md` §0): 15 of 183 triggers in 600 ticks,
`exec_total=33`, no region/death event ever raised. The 4-second window cannot exceed
the boot set: 9 initialization triggers + the 1 s waterfall timer + 1 game-state
crossing, because the rest of the map's region triggers are gated on the mission's
actor (`GetEnteringUnit() == udg_Thrall`) reaching a spot.

## Open item handed off (outside this lane's owner paths)

The mission still stalls after the intro cinematic:
`ENGINE_TICK t=40.00 … events=enter_region=21,death=7,game_state=3` — region
membership never changes after boot across 65 watched regions and 66 units, while the
script's own `SetUnitPositionLoc` teleports do move world units in bursts. The
cinematic issues `IssuePointOrderLocBJ(udg_Thrall, "move", …)` → `world.orders` →
`sim_bridge::apply_orders` → `sim.issue(Move)`; the simulation does not translate the
units, so the actor never enters `gg_rct_Spot01`-class regions and the quest gates stay
closed (`vetoes=567` at 60 s). That is `wc3-game`/`wc3-sim` (tech-lead / ledger 539),
not this lane. The dispatcher is now the prerequisite that will execute those triggers
the moment the actor moves.

`map_spawn` on Prologue01: `map_units=64 spawned_units=8 matched_units=56` (8 placed
units the script does not create — separate observation for lane J/I).

## Files

- `crates/wc3-re/src/engine.rs` — config-before-main, `ENGINE_SCRIPT`/`ENGINE_EVENTS`/`ENGINE_TICK`
  evidence lines, `script_config_ok`, `script_owner` on spawned units.
- `crates/wc3-re/src/jass/vm.rs` — dispatcher, tick sampler, inline call frames,
  array bounds, script owner, trigger-context-before-conditions, day/night clock.
- `crates/wc3-re/src/jass/natives.rs` — region membership, registrations carrying their
  payload, string natives, state/dialog/death/clock event sources.
- `crates/wc3-re/src/jass/parser.rs` — recursion limit.
- `crates/wc3-re/src/jass/lexer.rs` — UTF-8-when-valid string decoding.
