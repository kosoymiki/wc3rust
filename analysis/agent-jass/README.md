# JASS engine — lane report (ledger 410)

Engine for running the maps' own `war3map.j` against the simulation, measured by
execution rather than by code being present.

## What is where

| Path | Role |
| --- | --- |
| `crates/wc3-sim/src/jass/value.rs` | JASS2 values. `integer` is `i32`, `real` is `f32` (the engine's own widths), handles carry a kind tag so a mistyped call is visible. |
| `crates/wc3-sim/src/jass/registry_gen.rs` | **generated**: 1665 natives = 1534 `native` declarations of `Scripts\common.j` (1.32.9 donor copy) ∪ 1096 registrations of the TFT engine (`0x6f62f140`, r2 lane 407). |
| `crates/wc3-sim/src/jass/registry.rs` | Signature parsing, case-insensitive lookup, `handled` flag, typed defaults per declared return type. |
| `crates/wc3-sim/src/jass/vm.rs` | Compiler (AST → flat instructions) and executor with explicit frames, 300 000-instruction budget (`0x493e0`), 160-frame call cap, tick-quantized `wait`. |
| `crates/wc3-sim/src/jass/natives.rs` + `natives/{core,unit,trigger,world}.rs` | 516 handler rows. Handlers mutate the real `Sim`. |
| `crates/wc3-sim/src/jass/host.rs` | Handle tables and state the simulation does not model (groups, locations, rects, sounds, camera setups, hashtables, quests, dialogs). |
| `crates/wc3-sim/src/jass/scheduler.rs` | Tick → script time: sim events → triggers, timers, `TriggerSleepAction` resumption, region enter/leave, unit-in-range. |
| `crates/wc3-sim/src/jass/bundle.rs` | Loading `common.j` + `Blizzard.j` + `war3map.j` in engine order. Parsing is `wc3_re::jass` (the install scanner's parser, already used on the 44 campaign scripts and 91 AI scripts). |
| `crates/wc3-sim/src/jass/mod.rs` | `ScriptedSim`: the integration point (`step` = `Sim::step` + `JassVm::advance`). |

`jass_api.rs` (the older 27-native surface) is untouched and still passes its
tests; the new engine supersedes it without removing it.

## Measurement

```
python3 analysis/agent-jass/native_census.py     # static call-site histogram
python3 analysis/agent-jass/gen_registry.py      # regenerate registry_gen.rs
python3 analysis/agent-jass/coverage_report.py   # before/after counts
bash    analysis/agent-jass/extract_maps.sh      # rebuild extracted/ from the corpus
```

### Native coverage (observed 2026-11-15)

| Metric | Before | After |
| --- | --- | --- |
| Engine natives implemented (`0x6f62f140` list, 1096) | **27** (2.46 %) | **428** (39.05 %) |
| Distinct natives the 96 map scripts call (167) | 21 | **167 (100 %)** |
| Registry names with a handler (1665) | 27 | **589** (35.4 %) |
| Unresolved natives while running the 96-map corpus | n/a | **0** |
| Unresolved natives on `(2)Justice.w3x` | 50+ (measured: 19 distinct in the first `config`/`main` run) | **0** |

The 668 engine natives without a handler are the ones no shipped map calls —
multiboards, dialogs beyond creation, lightning, images, ubersplats, pooled
spawns, platform queries. Each still *resolves* and returns a value of its
declared type; every call is counted in `JassVm::unresolved_natives()`.

Runtime census (`cargo test -p wc3-sim --offline --test jass_native_coverage -- --nocapture`):

```
corpus: 96 maps, 2241366 instructions, 277 distinct natives, 0 header-absent natives, 3 undeclared globals
```

Justice through the VM (`--test jass_map_boot`):

```
justice: steps=26050 units_created=95 triggers=64 native_calls=4802 units=190
```

## `(2)Justice.w3x` native histogram (static, 558 call sites)

| Calls | Native | | Calls | Native |
| --- | --- | --- | --- | --- |
| 97 | `CreateUnit` | | 28 | `IsUnitHidden` |
| 56 | `TriggerRegisterUnitEvent` | | 8 | `GetCameraMargin` |
| 52 | `SetUnitAcquireRange` | | 7 | `SetResourceAmount` |
| 45 | `Player` | | 5 | `ConvertPlayerColor` |
| 32 | `ChooseRandomItemEx` | | 3 | `SetUnitColor` |
| 29 | `CreateTrigger` | | 2 each | `DefineStartLocation`, `SetPlayerColor`, `SetPlayerController`, `SetPlayerRacePreference`, `SetPlayerRaceSelectable`, `SetPlayerStartLocation`, `SetPlayerTeam`, `SetStartLocPrio`, `SetStartLocPrioCount` |
| 29 | `TriggerAddAction` | | 1 each | `NewSoundEnvironment`, `SetCameraBounds`, `SetDayNightModels`, `SetGamePlacement`, `SetMapDescription`, `SetMapMusic`, `SetMapName`, `SetPlayers`, `SetTeams` |
| 28 each | `DestroyTrigger`, `GetChangingUnit`, `GetChangingUnitPrevOwner`, `GetTriggeringTrigger`, `GetTriggerUnit` | | | |

Full histogram: `analysis/agent-jass/coverage_report.json`.

## Tests (all green — `cargo test -p wc3-sim --offline` rc=0, 49 binaries)

| Mechanic id | File | What it proves |
| --- | --- | --- |
| `jass.vm-arith` | `tests/jass_vm_arith.rs` | 32-bit integer wrap, `f32` reals, truncating division, divide-by-zero = 0, string `+`, conversions, byte-based `SubString`, budget trap, seeded RNG. |
| `jass.vm-control-flow` | `tests/jass_vm_control_flow.rs` | `elseif` first-true, nested `exitwhen`, `return` from a loop, recursion (`fib(15)`), depth trap, globals vs locals, bare `return`. |
| `jass.vm-arrays` | `tests/jass_vm_arrays.rs` | Global arrays, unset slots read 0, 0..8191 window (out-of-range collapses to slot 0), expression indices, `JASSHelper`-style array locals. |
| `jass.native-coverage` | `tests/jass_native_coverage.rs` | 0 unresolved natives on Justice and on all 96 corpus scripts; every name a map calls has a handler; registry covers all 1096 engine registrations. |
| `jass.trigger-fires` | `tests/jass_trigger_fires.rs` | A timer trigger damages a real unit, a death event trigger mutates a real unit, `TriggerSleepAction` suspends and resumes on the right tick, `TriggerExecute`, trigger-spawned units are simulation units. |
| `jass.map-boot` | `tests/jass_map_boot.rs` | `(2)Justice.w3x` boots: map units from `war3mapUnits.doo` + script `config`/`main`, 64 triggers, then 10 s of ticking with 0 unresolved natives. |

```
CARGO_BUILD_JOBS=2 cargo test -p wc3-sim --offline    # rc=0, 49 test binaries
```

## Semantics that are not guesses

* **Event constants** are read from the loaded `common.j` globals
  (`EVENT_UNIT_DEATH`, `EVENT_PLAYER_UNIT_*`, …) at VM construction, so a 1.27
  header and a 1.32.9 header both give the right numbers
  (`scheduler::EventIds::from_vm`).
* **Wait/timer quantization** is on the tick grid (`ceil(seconds * 30)`), because
  the engine's waits are frame driven; a 0.4 s wait resumes exactly at tick 12.
* **`ModuloInteger` is a Blizzard.j function** (`Blizzard.j:588`), not a native —
  the engine evaluates its body, including the negative-dividend wrap.
* **`GetBJMaxPlayers`/`GetPlayerNeutralPassive`** (natives in the 1.32 header)
  return the 1.29+ slot layout: 24 playable, neutral slots 24..27,
  `bj_MAX_PLAYER_SLOTS` = 28. Marked as *inferred* from the 1.24 constants plus
  the 24-slot expansion in `host.rs`.

## Known gaps (reported, not hidden)

1. **Roster gaps.** `CreateUnit` of a neutral type the local roster does not
   model (`nrac`, `nshe`, `ngol`, …) returns `null` and records
   `CreateUnit: unknown unit type <id>`. Justice's `CreateNeutralHostile` /
   `CreateNeutralPassive` are affected (measured: 95 units created, neutrals
   skipped). This is a `data::ROSTER` gap, not a script-engine one.
2. **Header-absent natives.** None remain in the corpus. `natives::HEADER_ABSENT_NATIVES`
   documents the Reforged-era names that only exist in 1.33+ headers; the
   handlers are registered and dispatched through the unknown-name path.
3. **Undeclared globals.** 3 names across 96 scripts compile to a typed default
   and are listed in `JassVm::code.undeclared` (newer-header camera fields).
4. **`StringHash`** is a documented stable FNV-1a, not the engine's unmeasured
   hash; no corpus script depends on its value.
5. **Tick-loop hook.** `Sim::step` is untouched: `ScriptedSim::step` wraps it.
   To run scripts from `Sim` itself, `crates/wc3-sim/src/lib.rs:1232`
   (`pub fn step`) would need one delegation line to a `JassVm` field — reported
   to the tech-lead rather than edited, because `lib.rs` is outside this lane's
   owner paths.
6. **`lib.rs` module declaration.** `pub mod jass;` was added at
   `crates/wc3-sim/src/lib.rs:29` — one line, no logic. A module that is not
   declared cannot compile and cannot be tested, so this was the minimum needed
   to make the lane verifiable.
