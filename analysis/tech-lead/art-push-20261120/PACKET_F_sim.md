# THICK GOAL — Agent F: Sim/gameplay layer (ledger 590)

## Mission
Backstop gameplay parity that the art push depends on: ensure mechanics P0 rows that gate a "Blizzard-level" playable level (movement across map #1, production queues, construction, win/lose) are actually executable and tested. Do NOT edit render/HUD/audio.

## Evidence snapshot
- `analysis/agent-qa/operator-bugs/REPORT.md` #1 partial: ">=800 px cross-map order with arrival screenshot and 10 s tracking" missing.
- `analysis/tech-lead/MECHANICS_GAP_MAP.md` + `run_mechanic_tests.sh` is the single test authority (per TEST_POLICY), 95 rows / 79 ported / 0 failing at last audit.
- `analysis/tech-lead/PLAYABILITY_GAP_2026_11_16.md`: economy/tech tree/production queues partial.

## Goal + done-criteria
1. Add a named sim test `artF_crossmap_move` proving a unit crosses >=800 px and arrives (state-based, deterministic; device screenshot delegated to Agent G). Verify with `scripts/run_mechanic_tests.sh` rc=0 (no new failing rows).
2. Fix root cause if the test fails (goal snap / A* budget / path request) inside `crates/wc3-sim/**` only.
3. Report exact test row id from MECHANICS_GAP_MAP and rc.

## Invariants / READ_ONLY
- OWNER: `crates/wc3-sim/**`, `analysis/tech-lead/mechanics-20261121/**`.
- NEVER edit: `crates/wc3-game/**`, `crates/wc3-render/**`, `scripts/**` (read-only runner ok), `Cargo.toml`, gap map files. Preserve uncommitted work. No stubs; one test = one mechanic id with id in the name (TEST_POLICY).

## Verification
`bash analysis/tech-lead/run_mechanic_tests.sh` rc=0 with new row passing; checkpoint 590 each phase, close 590 with h2_ids_json. Reply H2_PERSISTED + SESSION_CLOSED with ids.
