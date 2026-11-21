# mechanic test handoff — ui.hotkeys / ui.selection / ui.trace (ledger 377)

The wc3-input acceptance tests are in place (rc 0, see `cargo test -p wc3-input`).
`analysis/tech-lead/run_mechanic_tests.sh` is tech-lead-owned, so these are the
exact rows to merge (and the gap-map `test` fields to point at them).

## Runner entries (add next to line 182's `ui.hotkeys` entry)

```sh
run ui.hotkeys   wc3-input --test ui_hotkeys                  || status=1
run ui.selection wc3-input --test ui_selection_double --test ui_selection_add || status=1
```

`ui.trace` is not a gap-map row: the fixture replay proves the `ui.selection`
gestures (double-tap type select, Shift/long-press add) at the wire level.
Optionally add one row so the fixtures are counted:

```sh
run ui.trace     wc3-input --test ui_trace                    || status=1
```

## Gap-map `test` fields (MECHANICS_GAP_MAP.md rows 38 and 40)

Row 38 `ui.hotkeys` — `crates/wc3-game/src/game.rs wc3_command_hotkeys_...; sim patrol coverage`
→ `crates/wc3-input/tests/ui_hotkeys.rs (3) + crates/wc3-game/src/game.rs wc3_command_hotkeys_...`

Row 40 `ui.selection` — `crates/wc3-game/src/ui.rs (12)`
→ `crates/wc3-input/tests/ui_selection_double.rs (2) + ui_selection_add.rs (2) + ui_trace.rs (2)`

Row 40 status can move PARTIAL → PORTED once the game.rs hook patch
(`0003-selection-parity.patch`) is applied and the game tests for type-select
run green; the policy itself is already covered.

## Commands and current rc

```text
cargo test -p wc3-input --test ui_hotkeys --test ui_selection_double --test ui_selection_add --test ui_trace
# rc 0 (7 tests)
```

Test names carry the mechanic id: `ui_hotkeys_*`, `ui_selection_double_*`,
`ui_selection_add_*`, `ui_trace_*` (see TEST_POLICY.md rule 2).

## Full-goal additions (2026-11-14) — trunk row → new targets

```sh
run ui.hotkeys  wc3-input --test ui_hotkeys --test ui_hotkeys_remap   || status=1
run ui.selection wc3-input --test ui_selection_double --test ui_selection_add || status=1
run ui.patrol   wc3-input --test ui_orders_targeting                  || status=1
run ui.minimap  wc3-input --test ui_minimap_orders                    || status=1
run ui.trace    wc3-input --test ui_trace                             || status=1
```

Gap-map `test` fields to update: row 38 `ui.hotkeys` → `tests/ui_hotkeys.rs (3) +
tests/ui_hotkeys_remap.rs (3)`; row 39 `ui.patrol` → `tests/ui_orders_targeting.rs (3)`;
row 40 `ui.selection` → `ui_selection_double (2) + ui_selection_add (2) + ui_trace (2)`;
row 42 `ui.minimap` → `tests/ui_minimap_orders.rs (2)`.

Sub-mechanic ids used in test names (runner entries stay on the trunk row):
`ui.hotkeys.remap`, `ui.groups.tab` (row 38), `ui.orders.targeting` (row 39),
`ui.minimap.orders` (row 42), `ui.trace` (fixtures prove rows 38/39/40/42 gestures).

Current rc: `cargo test -p wc3-input --offline` → 0 (50 tests, 0 failed);
`touch-trace --check` 7/7 fixtures PASS.
