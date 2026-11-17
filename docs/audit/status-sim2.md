# Simulation layer (`crates/wc3-sim`) — audit status, lane sim2 (LEDGER 490)

Registry: `/storage/emulated/0/Download/WC3RUST_CODE_AUDIT_TZ_P0-P4.md` §0/0.1/0.2,
mirrored as `docs/audit/WC3RUST_CODE_AUDIT.md`.
Repo: `herofolends-rs`, branch `main`, lane base `dea209d` (2026-11-17).
Fix commit: **`716a8c2`**; test-hardening follow-up: **`0725598`**.
Every cargo invocation went through `scripts/h2-cargo` (single-slot wrapper:
one cargo at a time, memory gate, capped test threads) because other audit lanes
were building on the same device.

Scope of this fragment: AUD-012, AUD-013, AUD-017, AUD-021, AUD-022, plus two
defects met in the same code paths (see "Found while fixing").

| ID | P | Status | Where the fix lives | Regression test |
|----|----|--------|---------------------|-----------------|
| AUD-012 | P2 | **FIXED** | `crates/wc3-sim/src/jass/natives/unit.rs` (`location_arg`, `create_unit_at_loc`, `create_unit_at_loc_by_name`, `set_unit_position_loc`, `issue_point_order_loc`), `crates/wc3-sim/src/jass/natives/cinema.rs` (`issue_point_order_by_id_loc`) | `aud012_invalid_location_handles_are_diagnostics_not_the_world_origin` (`tests/audit_sim2.rs`) |
| AUD-013 | P2 | **FIXED** | `crates/wc3-sim/src/jass/host.rs` (`unit_type` constants, `UnitTypes`, `UnitMeta.types`), `crates/wc3-sim/src/jass/natives/world.rs` (`unit_add_type`/`unit_remove_type`/`set_unit_type`), `crates/wc3-sim/src/jass/natives/unit.rs` (`is_unit_type`, `derived_unit_type`, `unit_type_deltas`), `crates/wc3-sim/src/jass/natives/core.rs` (`get_player_structure_count`), `crates/wc3-sim/src/save.rs` (format v7) | `aud013_unit_add_and_remove_type_change_the_runtime_classification` (`tests/audit_sim2.rs`), `save::tests::version_6_file_loads_with_neutral_unit_types` |
| AUD-017 | P3 | **FIXED** | `crates/wc3-sim/src/jass/vm.rs` (`pending_sleep`, `request_sleep`, `park_thread`, `take_requested_sleep`, `Instr::Native`/`Instr::Unknown` consume the request), `crates/wc3-sim/src/jass/natives/trigger.rs` (`trigger_sleep_action`) | `aud017_indirect_trigger_sleep_action_parks_the_thread` (`tests/audit_sim2.rs`) |
| AUD-021 | P3 | **FIXED** | `crates/wc3-sim/src/spatial.rs` (`query`: `limit == 0` and non-positive/non-finite radius return empty) | `aud021_spatial_hash_query_honours_limit_and_degenerate_inputs` (`tests/audit_sim2.rs`) |
| AUD-022 | P3 | **FIXED** | `crates/wc3-sim/src/nav.rs` (`trace_flow`: checked `width * height`, exact field length, start-cell bounds before indexing) | `aud022_trace_flow_rejects_an_inconsistent_distance_field` (`tests/audit_sim2.rs`) |

New state that has to survive a save (the AUD-013 bitset) is in the versioned
save (format 7, appended to each `UnitMeta` record), so old files keep loading
and restore the neutral "definition only" classification.

---

## Reproduction before the fix

Every test was written first and run against the unpatched tree; the failing run
is the reproduction. The tests use only the public API, so the same file failed
on the old code and passes on the fixed code.

```text
$ scripts/h2-cargo test -p wc3-sim --offline --test audit_sim2 -- --test-threads=1
running 5 tests
test aud012_invalid_location_handles_are_diagnostics_not_the_world_origin ... FAILED
test aud013_unit_add_and_remove_type_change_the_runtime_classification ... FAILED
test aud017_indirect_trigger_sleep_action_parks_the_thread ... ok          # see note
test aud021_spatial_hash_query_honours_limit_and_degenerate_inputs ... FAILED
test aud022_trace_flow_rejects_an_inconsistent_distance_field ... FAILED

---- aud012_invalid_location_handles_are_diagnostics_not_the_world_origin stdout ----
assertion `left == right` failed: no unit may be created from an invalid location handle
  left: 5
 right: 1
---- aud013_unit_add_and_remove_type_change_the_runtime_classification stdout ----
UnitAddType reports success
---- aud021_spatial_hash_query_honours_limit_and_degenerate_inputs stdout ----
limit 0 must return zero neighbours, got [0]
---- aud022_trace_flow_rejects_an_inconsistent_distance_field stdout ----
index out of bounds: the len is 143 but the index is 143      (crates/wc3-sim/src/nav.rs:259)
test result: FAILED. 1 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out
rc=101
```

Each failure is the defect itself, not a harness artefact:

* AUD-012 — `CreateUnitAtLoc`/`CreateUnitAtLocByName` with a `null` /
  destroyed / wrong-family location spawned a footman at `(0, 0)`: five units
  where one was expected, and a unit at the world origin in a world that had
  none.
* AUD-013 — `UnitAddType` returned `null` (a `boolean` native answering
  nothing) and `IsUnitType` did not change.
* AUD-021 — `query(..., limit = 0, ...)` returned one element (`[0]`) because the
  push happened before the `out.len() >= limit` check.
* AUD-022 — `trace_flow` indexed a one-element-short field at
  `crates/wc3-sim/src/nav.rs:259` and panicked.

AUD-017 note, and the reason the first version of that test passed: the compiler
lowers a direct `call TriggerSleepAction(x)` to the `Wait` instruction, so the
*handler* can only be reached through the runtime name dispatch (a name the
compiler cannot resolve statically — the registry-versus-handler split described
in `natives.rs`). The test was rewritten to exercise that path at the executor
level (an `Instr::Unknown` call naming `TriggerSleepAction`, the exact shape the
compiler emits for a name it cannot see) and re-run on the pre-fix tree:

```text
$ git diff crates/wc3-sim/src/jass/vm.rs crates/wc3-sim/src/jass/natives/trigger.rs > /tmp/aud017_fix.patch
$ git apply -R /tmp/aud017_fix.patch
$ scripts/h2-cargo test -p wc3-sim --offline --test audit_sim2 -- --test-threads=1 aud017
test aud017_indirect_trigger_sleep_action_parks_the_thread ... FAILED
---- aud017_indirect_trigger_sleep_action_parks_the_thread stdout ----
the indirect TriggerSleepAction must suspend before the next statement
  left: 4
 right: 1
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 4 filtered out
rc=101
$ git apply /tmp/aud017_fix.patch        # restore the fix
```

Pre-fix the handler returned `Real(0.0)` without suspending, so all three sleeps
in the staged function ran through in one pass (`phase` was already 4) and no
elapsed time was ever reported.

---

## What each fix does

### AUD-012 — invalid location handles are diagnostics, not the origin

`location_arg()` resolves the `location` argument once and records a typed
diagnostic for each failure mode (`missing`, `null`, wrong family, destroyed /
stale handle) instead of `unwrap_or(Vec2::ZERO)`. The four natives that turned a
location into a *gameplay action* refuse the call (`Value::Null`, or
`Value::Bool(false)` for the order natives) and never touch the simulation.
`Vec2::ZERO` is no longer a sentinel anywhere in the create/move/order paths.

The regression test covers: valid location (a unit is still created there),
`null`, a destroyed handle (`RemoveLocation` first), a wrong-family handle (a
`group` where a `location` is expected), `CreateUnitAtLocByName`, and that
neither `SetUnitPositionLoc` nor the order natives move a unit or spawn one at
`(0, 0)`. Each case asserts the diagnostic is in `vm.errors`.

### AUD-013 — per-instance runtime unit types

`UnitTypes` is a pair of bit words (`added`, `removed`) over the `UNIT_TYPE_*`
ids the shipped `common.j` defines (`0..27`; `UNIT_TYPE_RANGED_ATTACKER` is 8,
`UNIT_TYPE_MELEE_ATTACKER` 7 — `analysis/war3_data/Scripts/common.j:500-501`).
It lives on `UnitMeta`, keyed by the unit handle, so it is per instance: the
shared `UnitDef` is never mutated.

`UnitTypes::classify(derived)` is the single rule — an instance removal wins,
then an instance add, then the value derived from the definition — and both
consumers go through it:

* `IsUnitType` (`jass/natives/unit.rs`) — getter path;
* `GetPlayerStructureCount` (`jass/natives/core.rs`) — native filter path,
  which previously answered `unit.def().is_building()` and would have disagreed
  with `IsUnitType(u, UNIT_TYPE_STRUCTURE)` for a unit whose structure bit the
  map changed.

`UnitAddType`/`UnitRemoveType` return `bool` (the header declares `returns
boolean`): `true` when the call applied (an idempotent re-add included), `false`
with a recorded diagnostic for a non-unit handle or a `unittype` outside the
header's enum. State is persisted in save format 7.

The regression test covers: add, remove, idempotent re-add, a second instance of
the same `UnitDef` staying unchanged, `IsUnitType` for hero / ranged (the 7/8
constant mix-up is listed under "Found while fixing"), the structure count
before/after, an out-of-range `unittype` being refused with a diagnostic, and a
save/load round trip (`state_hash` equal, the bits restored, the removed bit
stays removed).

### AUD-017 — a real sleep on the indirect path

The native now asks the executor to park the current thread
(`JassVm::request_sleep`), and the executor consumes that request in both
dispatch instructions (`Instr::Native` and `Instr::Unknown`) right after the
handler returns: `take_requested_sleep` parks the thread through the same
`park_thread` helper the `Wait` instruction uses (same `ceil`-to-tick
quantization, minimum one tick, same `wake_at`/`wake_tick`/`wait_started`
fields), so the two spellings cannot drift. On resume the scheduler pushes the
elapsed seconds, which is the call's value — exactly the `Wait` contract.

The request carries the thread index, so a nested thread can never park its
parent; a handler invoked with no running thread records
`TriggerSleepAction(<s>): no running script thread to suspend` and returns 0.0
instead of claiming a sleep that did not happen.

The regression test asserts both paths: the direct call (compiled to `Wait`)
still parks and returns the elapsed time, and the indirect dispatch parks for
0.5 s (15 ticks at 30 Hz), then for a negative timeout (1 tick), then for zero
(1 tick), releases the finished thread, and reproduces the same `state_hash` in
an independent run with the same seed.

### AUD-021 — `SpatialHash::query` honours `limit`

`query` returns before touching a bucket when `limit == 0`, and treats a
non-positive or non-finite `radius` as "no query circle" (a NaN radius used to
fall through to bucket `(0, 0)`). `out` is still cleared first, so a caller
cannot observe stale results. The regression test covers `limit` 0 / 1 / 3 /
8 / `usize::MAX`, an empty hash at two limits, `radius` 0 / negative / NaN /
infinite, and `clear()`.

### AUD-022 — `trace_flow` validates the field

The grid/field contract is checked before the first index:
`width.checked_mul(height)` (a wrap is refused), `field.len()` must be exactly
one value per cell (short *and* oversized buffers are refused — the latter means
the caller mixed two grids), and the start cell must be inside the grid and
walkable. `None` is the documented answer; the function has no panic path.
The regression test covers exact length, one short, empty, one extra, a field
for a different grid, a start cell outside the grid, and `max_steps == 0`.

---

## Found while fixing (same code paths, same evidence standard)

Two defects were met in the code being fixed and are closed here rather than
left as tails:

| Found while fixing | What was wrong | Fix | Test |
|--------------------|----------------|-----|------|
| `IssuePointOrderLoc` / `IssuePointOrderByIdLoc` | Same `unwrap_or(Vec2::ZERO)` as AUD-012: an invalid `location` issued a move to `(0, 0)`, i.e. a real order at the origin | `location_arg()` + `Value::Bool(false)`, no order issued | `aud012_...` asserts the unit stays `Order::Idle` for both natives |
| `IsUnitType` constant 7 | The handler answered "ranged" (range > 100) for `UNIT_TYPE_MELEE_ATTACKER` (7) and `false` for `UNIT_TYPE_RANGED_ATTACKER` (8); the shipped `common.j:500-501` assigns 7 = melee, 8 = ranged, so every script asking the real constants got the wrong answer | `unit_type::MELEE_ATTACKER` → `range <= 100`, `unit_type::RANGED_ATTACKER` → `range > 100`, with the ids named by constant instead of literals | `aud013_...` asserts a footman (range 90) is melee and not ranged before any `UnitAddType` |

## Determinism

`Sim::state_hash` is unchanged by this work: the new state is per-unit script
metadata (`UnitMeta.types`) and a transient executor field (`pending_sleep`),
neither of which is hashed; no `Sim` field was added and no ordering changed
(containers stay `BTreeMap`/`Vec` in entity order). Evidence: `--lib` (includes
`tests::simulation_is_deterministic`), `tests/determinism.rs` (14 tests), and
`aud013_...`'s save round trip asserting `restored.state_hash() ==
sim.state_hash()` plus `aud017_...`'s same-seed replay comparison.

---

## Gates (commands, rc, counts)

All runs on the patched working tree, 2026-11-17, `scripts/h2-cargo`, cargo
`--offline`, `--test-threads=1` (device shared with other lanes).

```text
$ scripts/h2-cargo test -p wc3-sim --offline --test audit_sim2 -- --test-threads=1
test aud012_invalid_location_handles_are_diagnostics_not_the_world_origin ... ok
test aud013_unit_add_and_remove_type_change_the_runtime_classification ... ok
test aud017_indirect_trigger_sleep_action_parks_the_thread ... ok
test aud021_spatial_hash_query_honours_limit_and_degenerate_inputs ... ok
test aud022_trace_flow_rejects_an_inconsistent_distance_field ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
rc=0

$ scripts/h2-cargo test -p wc3-sim --offline --lib -- --test-threads=1
test result: ok. 64 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
rc=0
    (includes save::tests::version_6_file_loads_with_neutral_unit_types,
     save::tests::version_5_file_loads_with_neutral_ui_state,
     save::tests::version_2_file_loads_with_ethereal_false,
     spatial::tests::query_is_bounded_and_indexes_units,
     tests::simulation_is_deterministic, the native-status module tests)

$ scripts/h2-cargo test -p wc3-sim --offline --test jass_natives --test jass_trigger_fires \
    --test save_script --test save_load --test determinism --test script_roster \
    --test fog --test combat --test gameplay_1v1 -- --test-threads=1
test result: ok. 10 passed / 3 / 4 / 1 / 14 / 5 / 14 / 2 / 2 passed; 0 failed in every binary
rc=0

$ scripts/h2-cargo test -p wc3-sim --offline --test jass_native_coverage -- --test-threads=1
test convert_natives_resolve_through_the_identity_rule ... ok
test every_native_the_justice_script_calls_has_a_handler ... ok
test justiice_script_runs_with_zero_unresolved_natives ... ok
test registry_covers_every_native_name_the_engine_registers ... ok
test the_whole_extracted_corpus_runs_with_zero_unresolved_natives ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
rc=0
    (the 96-map corpus still runs with zero unresolved natives and no new
     script errors after the location/type changes)
```

Self-contained commit check: a detached worktree at `716a8c2` (i.e. the commit
without any other lane's uncommitted work) compiles:

```text
$ git worktree add --detach <tmp> 716a8c2
$ cd <tmp> && CARGO_TARGET_DIR=<repo>/target scripts/h2-cargo check -p wc3-sim --offline
    Finished `dev` profile ... rc=0
$ git worktree remove --force <tmp>
```

`0725598` is the follow-up that makes the two header-dependent tests skip
(`find_game_scripts()?` → explicit skip message) when `common.j`/`Blizzard.j`
are absent, matching the rest of the JASS-native suite; AUD-017/021/022 need no
game data and always run.

Device note: a full `cargo test -p wc3-sim` (all 390+ tests in one process) was
killed twice by the shared device while other lanes were running their own test
binaries (the h2-cargo wrapper gates memory at start, not during a run), so the
suite was run per target — the four commands above cover every test file that
touches the changed code paths plus the whole corpus.
