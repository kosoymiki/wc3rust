# TEST POLICY — no arbitrary tests (hard, 2026-11-13)

The operator rejects "произвольные тесты". A test exists only to prove a named
mechanic of the port. This policy is enforced in review and by the
`gate.port.mechanic-test-coverage` corpus gate.

## Rules

1. **One test → one mechanic id.** The test belongs to a row in
   `MECHANICS_GAP_MAP.md` (`combat.damage-table`, `economy.upkeep`,
   `melee.matrix`, `heroes.revive`, ...). If there is no row, there is no test.
2. **Name carries the mechanic.** Test function names start with the mechanic
   id in snake case: `upkeep_tax_boundaries_50_80`,
   `melee_matrix_4_races_sizes_1_4`, `heroes_revive_altar_700g_150s`. A reader
   must know what is proven without opening the body.
3. **No exploratory / diagnostic / perf tests** in the main suite. Diagnostics
   may exist only with `#[ignore = "diagnostic: <reason>"]` and must predate
   this policy (existing render diagnostics are grandfathered); new ones are
   rejected. **No blanket ignores:** every ignored test is registered in
   `analysis/tech-lead/acceptance_ignores.tsv` with the environment it needs
   (`War3.mpq`, headless GPU, device artifact, `--release`), and
   `bash analysis/tech-lead/run_layer_gates.sh --ignored` runs exactly that
   manifest, one cargo invocation per test. An `#[ignore]` missing from the
   manifest fails `audit_ignored_tests.py`. Tests that can be made runnable in
   the normal suite should be env-gated (`H2_GPU_TESTS=1`, `H2_PERF_TESTS=1`)
   instead of ignored; the manifest is for tests that genuinely need external
   inputs (real MPQ assets, device frames, a release build).
4. **No test file without a mechanic group.** New files:
   `tests/<mechanic-group>.rs` where the group is a gap-map prefix (`melee`,
   `upkeep`, `heroes`, `items`, `maps`, `save`, `net`). One file per group, not
   one file per experiment.
5. **Evidence is the runner.** `analysis/tech-lead/run_mechanic_tests.sh`
   must list the target; `mechanic_tests.json` must show `rc=0`. A test not in
   the runner does not count as coverage.
6. **Tree stays green.** Every agent finishes an edit with
   `cargo check -p <crate>` rc=0 before reporting. A red tree blocks every
   other agent's builds and is a violation.
7. **No generated file in `src/` without a compile + count test.**
   `include!`-generated data (e.g. `abilities_gen.rs`) must compile, be wired
   by a module/include, and have a test asserting the generated count equals the
   decompiled row count (763 abilities, 812 units, 273 items).
8. **No weakening a gate to pass.** If a criterion cannot pass, report FAIL with
   the exact missing artifact. Never delete/relax an acceptance check.

## Rejected patterns (seen 2026-11-13)

- New `tests/*.rs` for a one-off experiment with no gap-map row.
- `#[test]` that only prints a count "for information".
- Tests that assert internal constants nobody consumes (self-referential proof).
- Marking a mechanic PORTED while its test is `#[ignore]`.
- Editing `crates/wc3-sim` from a non-sim owner while the sim owner is mid-edit
  (red tree for everyone).
