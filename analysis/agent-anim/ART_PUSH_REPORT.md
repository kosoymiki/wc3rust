# Agent D — animation layer report (art push 2026-11-21, ledger 588)

Repo `~/src/herofolends-rs`. OWNER: `crates/wc3-anim/**`, `analysis/agent-anim/**`.
READ_ONLY: every other crate, `Cargo.toml`, `scripts/**` — untouched.

## Goal + done-criteria
Verify/fix per-sequence geoset visibility so units and the build ghost animate
correctly; pin the `MODEL_LOD_REPORT.md` §3–§5 "all geosets drawn" fix with two
named tests and run `scripts/h2-cargo test -p wc3-anim --offline` rc=0.

## Deep reflection — mechanism, root cause, falsification
- **Mechanism (file:line, observed):** `crates/wc3-anim/src/runtime.rs:53-109`
  `mesh_alphas`: with `Some(sequence)` a geoset must pass the per-sequence extent
  gate (`Geoset::present_in_sequence`) and its KGAO track is sampled only for
  in-range keys (`geoset_alpha_constant_rule`, `lib.rs:977` → `sample_in_sequence`,
  `lib.rs:649`); with `None` only static GEOA alpha applies (no extent gate) —
  exactly the defective state that draws every geoset. The game-layer fix
  (`crates/wc3-game/src/assets.rs:3098-3145`, agent-render / Agent B, READ_ONLY)
  resolves `None` to `model.idle` (Stand) before calling `mesh_alphas_for`.
- **Root cause of the operator defect (measured, not assumed):** v800 MDX has no
  LOD; a unit with no running sequence drew Peon geosets 0..4 (6 draws) instead
  of the Stand set geosets 0/1 (3 draws). No `wc3-anim` runtime defect was found:
  the visibility rule is correct and the game-layer default-sequence resolution
  is already present. No source fix was needed in my owner paths; the existing
  uncommitted `lib.rs` bounded-`NodeIndex` security fix (WC3-I-016) was preserved.
- **Falsification of my first two test failures (exact error → cause → one fix):**
  1. `sequence "Stand" not in [...]` → GreatHall ships `stand` lowercase; fixed
     with case-insensitive fallback in the test helper (no runtime change).
  2. Peon `Decay Flesh` arrays differed at midpoint/end → probed the raw KGAO
     keys with `analysis/agent-anim/probe_mdx.py`: geoset 4 `(43167,0.0) →
     (45533,1.0)`, geosets 0/1 `(43167,1.0) → (103167,0.0)`, flags=1 non-loop.
     The report sampled at `sequence.start`; later frames author a gore reveal
     and body/pack fade. Authored behavior, not the all-geosets bug — encoded
     as exact expected arrays at start / midpoint / end.

## Actions
1. H2 corpus pack `ev-16dec2e99f41fcad` (task_type=sysadmin); read both THICK
   packets, `MODEL_LOD_REPORT.md`, brain START files; root `uid=0(root)
   context=u:r:ksu:s0`, rtk 1.1.0, r2 present; git state inspected.
2. Verified peon fixture byte-identical across trees (sha256
   `0c55af81eb7ec150fa8bc911437ce07e9119e574501aac3a9d145488035e5716`).
3. Added `crates/wc3-anim/tests/artD_animation.rs` with `artD_sequence_alphas`
   and `artD_ghost_sequence` (full implementations, no stubs).
4. Three evidence-driven test iterations (rc=101 → rc=101 → rc=0); each failure
   root-caused to authored data or a test helper assumption before its fix.
5. Ran the full crate suite, `git diff --check`, and owner-path status audit.

## Evidence (rc / measured arrays / paths)
- `scripts/h2-cargo test -p wc3-anim --test artD_animation --offline -- --nocapture`
  → **rc=0, 2 passed; 0 failed**. Log `analysis/agent-anim/artD-named-tests.log`.
  Exact expected arrays from `MODEL_LOD_REPORT.md` §4, measured at `sequence.start`:
  ```
  ARTD_SEQUENCE_ALPHAS seq=Stand        alphas=[1.0,1.0,0.0,0.0,0.0] visible=[0,1] draws=3 PASS
  ARTD_SEQUENCE_ALPHAS seq=Walk         alphas=[1.0,1.0,0.0,0.0,0.0] visible=[0,1] draws=3 PASS
  ARTD_SEQUENCE_ALPHAS seq=Attack       alphas=[1.0,1.0,0.0,0.0,0.0] visible=[0,1] draws=3 PASS
  ARTD_SEQUENCE_ALPHAS seq=Death        alphas=[1.0,1.0,0.0,0.0,0.0] visible=[0,1] draws=3 PASS
  ARTD_SEQUENCE_ALPHAS seq=Decay Flesh  alphas=[1.0,1.0,0.0,0.0,0.0] visible=[0,1] draws=3 PASS
  ARTD_SEQUENCE_ALPHAS seq=<none>       alphas=[1.0,1.0,1.0,1.0,1.0] visible=[0,1,2,3,4] draws=6 PASS
  ```
  Decay Flesh authored progression also pinned: midpoint `[1,1,0,0,1]`, end
  `[0,0,0,0,1]`. Carry variants (geosets 2/3) stay hidden at every sample time.
- `artD_ghost_sequence` (same rc=0 run):
  ```
  GreatHall: none_visible=[0,1,2,3,4,5,6,7,8,9,10,11,13,14,15,16,17] none_draws=20
             stand_visible=[2,3,13] stand_draws=4 leaked_hidden=14 geosets -> PASS
  TownHall:  stand_visible=[11,13,14] none_visible=[0,2,4,...,35] -> PASS
  ```
  Confirms a running sequence hides the upgrade/rubble geosets that would
  double the build ghost; `None` draws all of them.
- `scripts/h2-cargo test -p wc3-anim --offline` → **rc=0**; aggregate
  **40 passed; 0 failed; 2 ignored** across lib + 9 integration binaries +
  doc-tests. Log `analysis/agent-anim/artD-full-crate-tests.log`.
- `git diff --check -- crates/wc3-anim analysis/agent-anim` → **rc=0**.
- Owner-path status: `M crates/wc3-anim/src/lib.rs` (PRE-EXISTING security fix,
  preserved), `?? crates/wc3-anim/tests/artD_animation.rs` (new), `?? analysis/agent-anim/`
  (this report dir). No edits to any other crate / Cargo.toml / scripts.

## Result
**DONE.** No animation runtime defect found; the exact alpha contract is pinned
and both named tests plus the full `wc3-anim` suite pass rc=0. Ghost correctness
at the anim layer is proven on both town halls; Agent B's `art10_ghost_visible`
(rc=0) covers the game.rs draw path (50% alpha, tint, ring).

## Risks / Next
- Device frame evidence is NOT produced here: `H2_DEVICE_SLOT` is serial and
  owned by Agent G. This report is headless numeric evidence, not a screenshot;
  no false "Blizzard-level art" claim.
- On this fallback install the selected ghost def `hgtw` has no reachable model
  (`GAME_MODEL_SKIP def=hgtw reason=not-reachable`), so Agent B's ghost test
  tolerates a geometry-less placeholder; the anim-layer contract is model-generic
  and proven on the two shipped town halls. Retail MPQ run remains owner-side.
- Pre-existing warnings (`unused variable track` in runtime.rs:84, non_snake_case
  on the two required test names) are harmless; test names must stay exactly
  `artD_sequence_alphas` / `artD_ghost_sequence` per packet.

## H2_PERSISTED
- corpus `ev-16dec2e99f41fcad` (2026-11-21); captures **1208** (phase-1 failure
  evidence), **1212** (final); experiences **4884** (KGAO/start-sampling lesson),
  **4887** (final); monitor **588** checkpoint each phase; root `uid=0 u:r:ksu:s0`.

## SESSION_CLOSED
- Ledger **588** closed with `result=done` and `h2_ids_json` containing the
  capture/experience IDs, rc values, and artifact paths above.
