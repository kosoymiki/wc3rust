# THICK GOAL — Agent D: Animation layer (ledger 588)

## Mission
Verify/fix animation and geoset visibility so selected units and the build ghost animate correctly; ensure no "all geosets drawn" regression (the v800 Stand-set fix in MODEL_LOD_REPORT.md).

## Evidence snapshot
- `analysis/agent-render/MODEL_LOD_REPORT.md`: v800 MDX has no LOD; root cause was no running sequence -> every geoset drawn; fixed by Animator::from_model(model_for_def(def_id)) + Stand-set extent gate. Peon 6 draws -> 3.
- Ghost (#10) and army icons (#7) depend on correct sequence state; Agent B owns game.rs.

## Goal + done-criteria
1. Confirm `wc3-anim` runtime: with sequence=None vs Stand/Walk/Attack, per-geoset alphas correct; named test `artD_sequence_alphas` with exact expected arrays from MODEL_LOD_REPORT.md.
2. Fix any animation defect found (e.g. ghost sequence not running -> all geosets drawn, doubled body). Named test `artD_ghost_sequence`.
3. `cargo test -p wc3-anim --offline` rc=0; report with measured draw counts.

## Invariants / READ_ONLY
- OWNER: `crates/wc3-anim/**`, `analysis/agent-anim/**`.
- NEVER edit: any other crate, `Cargo.toml`, `scripts/**`. Preserve uncommitted work. No stubs.

## Verification
Named tests rc=0; measured arrays in report; `h2oc monitor-checkpoint 588` each phase, close 588 with h2_ids_json. Reply H2_PERSISTED + SESSION_CLOSED with ids.
