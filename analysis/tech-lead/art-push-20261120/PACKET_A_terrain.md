# THICK GOAL PACKET — Agent A: terrain/water + world-art light patches (bugs #14 + world-art)

## Mission
Close operator bug **#14 (Terrain looks bad — map-edge water flat blue band)** and apply the three
ready, measured world-art patches, then prove them on device screenshots. Repo: `~/src/herofolends-rs`.

## Parity row
Parent ledger **584** (`wc3rust-art-blizzard-20261120`). You are **Agent A**. Sibling Agent B owns
army icons + build ghost — do NOT touch its files.

## Evidence snapshot (observed, 2026-11-20)
- `analysis/agent-qa/operator-bugs/REPORT.md` row #14: "map edge water still a flat blue band (06 left edge)" — partial.
- `analysis/agent-world-art/PATCH-REQUESTS.md` lists three measured, ready patches (terrain highlight shoulder, doodad contact+shadow, terrain shimmer via texel density).
- `analysis/agent-world-art/AFTER.md`: shipped mesh light shoulder + contact + ground shadow improved `roof form 0.1833 -> 0.2777`, ground shadow coverage `0.003 -> 0.027`. Terrain shader still uses raw clamp — inconsistent.
- P0-12 rendering parity = **FAIL** in `FULL_PORT_CRITERIA.md`.

## Goal + done-criteria
1. Terrain fragment shader uses the same `shoulder(knee=0.72)` tonal curve as `mesh.rs` so sun-facing slopes do not clip flat.
2. Doodads get contact band + ground shadow (via `.with_contact` in static batch or `push_ground_shadow`), so trees/rocks are grounded like units.
3. Map-edge water: no flat blue band — water texture/edge pass produces a non-flat shallow/deep transition. Fix root cause, do not paper over.
4. Terrain shimmer at 45° mitigated (texel density / detail octave) if measurable.
5. Device screenshot evidence at real size (2414x1080) showing before/after for water edge and lighting; named tests pass; `cargo check -p wc3-game`/`wc3-render` rc=0.

## Invariants
- HONEST ART STANDARD: procedural/filter fixes are NOT "hand-drawn Blizzard art" — label as such; no false finished claims.
- No stubs/TODO-as-deferred; full implementation; tests per change.
- Do NOT touch files outside OWNER_PATHS. Several agents' uncommitted work is in the tree — leave it.

## OWNER_PATHS (exclusive — you are the only writer)
- `crates/wc3-game/src/terrain_render.rs`
- `crates/wc3-render/src/mesh.rs`
- `crates/wc3-render/src/texture.rs`
- `crates/wc3-render/src/terrain.rs` (if needed)
- `crates/wc3-game/tests/**` (new terrain tests only, prefix `art14_`)
- `crates/wc3-render/tests/**` (prefix `art14_`)
- `analysis/agent-world-art/**` (your report dir)

## READ_ONLY (do not edit)
- `crates/wc3-game/src/game.rs`, `hud.rs`, `mobile_hud.rs`, `assets.rs` (Agent B + others)
- `analysis/tech-lead/FULL_PORT_CRITERIA.md`, `operator-bugs/REPORT.md`
- `Cargo.toml`, `scripts/**`

## Phase plan + verification
1. Read PATCH-REQUESTS.md + current `terrain_render.rs` shader and `mesh.rs` shoulder impl. Confirm exact insertion points.
2. Apply shoulder to terrain shader; add named test `art14_terrain_shoulder` comparing clip vs shoulder on a sun-facing slope. Verify `cargo test -p wc3-game --lib art14_terrain_shoulder`.
3. Ground doodads: add `contact` to `StaticDrawSpec` / call `with_contact` or make `push_ground_shadow` pub. Test `art14_doodad_contact`.
4. Water edge: find the flat-band root cause in water/edge pass; fix; test `art14_water_edge`.
5. Render before/after via existing harness (`scripts/h2-cargo`, world_frame example; device GPU Turnip Adreno 725) at 2414x1080; save PNGs + per-channel means in `analysis/agent-world-art/`.
6. `cargo check -p wc3-game -p wc3-render --offline` rc=0; named tests pass; write `analysis/agent-world-art/ART_PUSH_REPORT.md`.

## Hints/known pitfalls
- `DrawParams` is 112 B with `contact: [f32;4]`; `draw_uniform_matches_wgsl` asserts layout — update WGSL if you change it.
- Terrain shader DNC: sun 1.3, ambient 0.2; flat noon = 0.988 near clip. `LIGHT_KNEE=0.72`.
- Water pass was restricted by World agent (#17 fixed) — do not reintroduce the white veil.
- android.jar for rebuilds lives in `~/src/xray-mgr-android/android-35/android.jar` (not needed for pure Rust).

## Intervention protocol
Tech lead may steer with corrections/new evidence via `subagent(sessionID=...)`. Report blockers with exact rc/output — no silent waiting. Checkpoint every phase in ledger.

## Report format
`Goal | Actions | Evidence (rc/output/screenshot paths/means) | Result | Risks/Next | H2_PERSISTED (capture/experience/ledger id) | SESSION_CLOSED`. Include your ledger id.
