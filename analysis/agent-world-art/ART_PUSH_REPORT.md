# Agent A — world/terrain art push (ledger 585, 2026-11-21)

## Goal
Close operator bug #14's flat map-edge water band and apply the measured world-art
lighting/grounding patches, with named tests and 2414×1080 before/after evidence.
Only PACKET_A OWNER_PATHS were written; other agents' uncommitted work was preserved.

## Actions
1. Read `PACKET_A_terrain.md`, `PATCH-REQUESTS.md`, `AFTER.md`, the terrain/mesh shaders
   and the static-batch + doodad paths. Corpus pack `ev-79f52a1e5a051332`.
2. `git diff` baseline: terrain `shoulder()` (knee via `camera.sun.w`) and the LOD
   variation-fade shimmer mitigation were **already present** from earlier lanes. They
   were not re-implemented; `art14_terrain_shoulder` now asserts them.
3. `crates/wc3-render/src/mesh.rs`: added `MeshRenderer::static_pipeline` (decal group 2
   → `PipelineKind::Shadow` / `fs_shadow`, not `fs_main` with texture 0), pure
   `static_contact_band` (8 % of the spec's indexed Y extent, clamp 6..24, strength
   0.55), wired both into `create_static_batch`, plus `H2_STATIC_GROUNDING_LEGACY` A/B.
4. `crates/wc3-game/src/terrain_render.rs`: added pure `water_edge_colour` and wired it
   into `build_water_border`: full-frame shallow→deep ramp + 10 % world-space long-wave
   tint, palette-bounded, with `H2_WATER_EDGE_LEGACY` A/B.
5. Added `crates/wc3-render/tests/art14_doodad.rs` and
   `crates/wc3-game/tests/art14_terrain.rs` (4 named tests).
6. Rendered real 2414×1080 A/B frames with `crates/wc3-game/examples/world_frame.rs`
   at an edge-facing camera, from the **same rebuilt binary**, differing only by the
   two A/B switches.

## Evidence (observed)
- `scripts/h2-cargo test -p wc3-render --test art14_doodad --offline` → **rc=0**, 2 passed.
- `scripts/h2-cargo test -p wc3-game --test art14_terrain --offline` → **rc=0**, 2 passed.
- `scripts/h2-cargo test -p wc3-render --lib --offline` → **rc=0**, 132 passed, 0 failed.
- `cargo check -p wc3-game -p wc3-render --offline` → **rc=0**.
- `scripts/h2-cargo test -p wc3-game --lib --offline` → **rc=101**, 490 passed / 33 failed /
  35 ignored. Every failure is in `game.rs` or `mobile_hud.rs` (e.g. `game.rs:10096
  "Human04 is the melee arena"`, `mobile_hud.rs:10930` minimap-frame assertion,
  `mobile_hud.rs:10454 "card slot drift"`). **No** terrain_render, mesh or art14 test
  failed. Those files are Agent B's OWNER_PATHS and were never edited by me.
- Root cause proven by `art14_water_edge`: legacy deep-edge colour spread `<1e-6`
  (one value across all 8 cells) vs shipped `>0.01`, still inside the authored palette;
  authored alpha fade still reaches 0. `art14_doodad_shadow_pipeline`: `group=2` spec
  now maps to `PipelineKind::Shadow`; `art14_doodad_contact`: base_y/band/strength exact
  (10.0 / 9.6 / 0.55).

### 2414×1080 same-binary A/B (Orc01 proxy map, 12214 water cells / 4976 border tris,
camera target `14000,200`, distance 2600, hour 12, no HUD, `--no-hud`)
| frame | band RGB | band luma | full RGB | full luma | max flat-colour run |
|---|---|---|---|---|---|
| BEFORE (`H2_WATER_EDGE_LEGACY=1 H2_STATIC_GROUNDING_LEGACY=1`) | 70.01, 85.23, 106.65 | 83.54 | 80.47, 97.13, 119.77 | 95.22 | 107/403 = 26.55 % |
| AFTER (shipped) | 70.67, 86.95, 108.20 | 85.02 | 82.98, 103.92, 125.84 | 101.05 | 78/403 = 19.35 % |

Both rc=0. PNGs: `art-push-20261120/art14-BEFORE-edge-2414x1080.png` and
`art14-AFTER-edge-2414x1080.png`. The flat-band run fell 26.55 % → 19.35 % (−27 %
relative) and full-frame blue rose 119.77 → 125.84. Additional diagnostic frames
(`art14-baseline-*`, `art14-legacy-*`, `art14-after-*`, `probe-*`) are in the same dir;
the stale prebuilt-baseline frame is labelled diagnostic only — it predates other
agents' current tree and is **not** used as the A/B baseline.
`art-push-20261120/qa-edge-measure.json` measures the shipped QA Justice frames:
`before-justice-map-edge-2414x1080.png` has a 155/403 (38.46 %) identical-colour run
along its blue row — the reported defect. Those frames predate this push and establish
the defect, not this fix.

## Honest art standard
These are **procedural rendering fixes**, not hand-drawn Blizzard art. The authored
BLP albedo, Water.slk palette and animated water surface are untouched. The tint is a
10 % long-world-wave modulation of existing colours; no new art is claimed, and no
finished/hand-drawn claim is made.

## Result
Done: both root causes fixed in owner paths, 4/4 named tests green, `cargo check` green,
wc3-render lib green, real same-binary 2414×1080 A/B frames with per-channel means and
a measured flat-band reduction. The wc3-game lib suite is **not** green (33 failures in
Agent B's files); this does not block my scope but remains an open item for that owner.

## Risks / Next
- The exact operator map (`Melee.w3x`/Justice) and `War3.mpq` are absent here; the A/B
  uses the available Orc01 campaign map with water present. Device re-capture on
  Justice by the QA lane is the remaining confirmation step.
- The 10 % wave tint is procedural; tune amplitude against a Justice frame if the
  operator wants a flatter/deeper open sea.
- Agent B must fix its 33 HUD/game test failures before the repo-wide suite is green.

## H2_PERSISTED
- corpus evidence `ev-79f52a1e5a051332`
- capture `1200` (baseline/context), capture `1203` (implementation + tests),
  capture `1209` (final same-binary 2414×1080 A/B evidence)
- experience `4882` (water-flat-band + shadow-pipeline diagnostic pattern)
- monitor ledger `585` checkpoints at every phase.

## SESSION_CLOSED
ledger `585` closed with result in the monitor record (see final report IDs).
