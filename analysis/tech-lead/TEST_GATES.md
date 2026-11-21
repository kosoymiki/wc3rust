# CANONICAL TEST GATES (tech-lead, 2026-11-13)

Agents run ONLY the gates listed here or explicitly named in their packet. No workspace-wide
`cargo test`, no exploratory suites, no duplicate runs. Every gate reports: exact command + rc + counts.

| Layer | Gate command | Pass criterion |
|---|---|---|
| sim | `cargo test -p wc3-sim --lib` | 0 failed |
| sim/map | `cargo test -p wc3-game --lib map_units_spawn` | 2 passed |
| world/models | `cargo test -p wc3-game --lib models_per_def` | rc 0, PERF_MEMORY line |
| anim | `cargo test -p wc3-anim` | 0 failed |
| formats/fdf | `cargo test -p wc3-formats --lib fdf` | 0 failed |
| data | `cargo test -p wc3-data --lib` | 0 failed |
| re | `cargo test -p wc3-re --lib` | 0 failed |
| gpu/render | `cargo test -p wc3-render --test astc` | 0 failed |
| build | `cargo check -p wc3-game --lib` | rc 0 |
| RE scan | `target/release/wc3re scan analysis/war3_full3` | 0 errors, scripts.unimplemented={} |
| acceptance | `python3 analysis/agent-qa/acceptance/run_acceptance.py` | 0 FAIL |
| layer gate (all in order) | `bash analysis/tech-lead/run_layer_gates.sh` | PASS/FAIL table, rc 0 only when every requested step PASS |
| layer gate (one step) | `bash analysis/tech-lead/run_layer_gates.sh sim game_hud` | selected steps only, same criteria |
| ignored acceptance | `bash analysis/tech-lead/run_layer_gates.sh --ignored` | every row of `acceptance_ignores.tsv` executed with `--ignored`, 1 passed each, rc 0 |
| ignored subset | `bash analysis/tech-lead/run_layer_gates.sh --ignored peon` | manifest rows whose test name contains `peon` |

Rule: a new gate is added to this file and to the agent packet before it is run.
All gate commands run cargo through `scripts/h2-cargo` (device-wide flock,
`MemAvailable >= 3.2 GB`, `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`,
`RUST_TEST_THREADS=2`) after the 2026-11-15 OOM incident; never call `cargo`
directly while another agent may be running a suite.

## Device slot (hard rule, 2026-11-13)
- One device cycle at a time, only with an explicit tech-lead grant (`H2_DEVICE_SLOT=1`).
- Guarded scripts: `scripts/game-selftest.sh`, `scripts/qa/device_p0_cycle.py`,
  `scripts/qa/device_cycle.py`, `analysis/agent-touch/device-hotkeys.sh` — without the env they exit 3
  with `DEVICE_SLOT: not granted`.
- Never launch `org.herofolends.p0` on the shared device while the operator is using it.
- After a granted cycle: force-stop the app and return the device to Termux.
- `game-selftest.sh` launches exactly once (no relaunch loop).

## Layer gate semantics (2026-11-15)

`run_layer_gates.sh` runs sim, data, formats, audio, anim, `hud::`, and the four
real-map game tests in order with `JOBS=2 --offline`, writes one log per step in
`analysis/tech-lead/gate-logs/`, and prints a PASS/FAIL table with rc.

- A step that executes **zero** tests is `NO-TESTS` (FAIL): a filter that matches
  nothing prints "0 passed" and exits 0 — the old trap.
- A real-map step that prints `skip:` is `SKIP` (exit 2 if nothing else failed),
  never PASS. `playability_real_map.rs` was silently skipping until 2026-11-15
  because it looked for `Maps/Orc01.w3m` instead of `Maps/Campaign/Orc01.w3m`.
- A skipped test inside a crate step is counted and printed; the step still
  passes when the rest of the crate is green.
- Machine-readable result: `analysis/tech-lead/gate-logs/last-results.json`.
