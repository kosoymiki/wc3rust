# WORLD RENDER GAPS — 2026-11-18 (LEDGER 502, lane: world render)

Acceptance basis: **Orc01** (`Maps\Campaign\Orc01.w3m`, first RoC orc chapter),
frames at 2414x1080 at dawn/noon/dusk, plus the FoW three states.

Method per row: mechanism (file:line) → reproduce on a real frame → one fix →
re-measure. All numbers come from `crates/wc3-game/examples/world_frame.rs`: one
`Assets::load`, one headless device, one `Renderer`, many shots, so the two halves
of every A/B share camera, map, seed and renderer state.

## 1. Evidence baseline (observed)

Full install (`analysis/agent-assets/wc3-install`: `War3.mpq` + `War3x.mpq` +
`War3xlocal.mpq`), Orc01, 2414x1080, one process:

| shot | draws | tris | band rgb | band luma | full luma | frame ms |
|---|---|---|---|---|---|---|
| dawn 05:30 | 263 | 251 536 | 96.11,55.94,28.98 | 62.54 | 57.74 | 3220 |
| noon 12:00 | 263 | 251 536 | 97.52,69.48,42.51 | 73.50 | 68.34 | 3450 |
| dusk 20:00 | 263 | 251 536 | 94.49,53.43,23.56 | 60.00 | 55.48 | 3810 |
| night 23:00 | 263 | 251 536 | 85.57,41.46,14.84 | 48.91 | 46.14 | 4092 |
| fow visible | 263 | 251 536 | 97.52,69.48,42.51 | 73.50 | 68.34 | 3272 |
| fow explored | 263 | 251 536 | 41.38,27.94,15.87 | 29.93 | 32.26 | 3190 |
| fow unexplored | 263 | 251 536 | 4.21,3.67,2.70 | 3.71 | 10.52 | 486 |
| fow simulated | 263 | 251 536 | 93.88,66.65,39.95 | 70.51 | 58.72 | 3372 |

Derived:
* **Day/night**: dusk keeps **81.6 %** of noon luminance and warms
  (R/B 3.40 → 4.01); dawn keeps **85.1 %** (R/B 3.32); night keeps **66.6 %**
  (R/B 5.77). The authored DNC keys explain it: `KLAC` is full white from
  `at=0.25` (06:00) to `at=0.75` (18:01), the warm key sits at `at=0.804`
  (19:17) and midnight is the orange `(0.799,0.533,0.313)` key
  (`wc3-render/src/daynight.rs:1357-1389`). 06:00 and 12:00 are therefore the
  same key — "dawn" must be captured at 05:30, "dusk" at 20:00.
* **FoW three states**: explored keeps **40.7 %** of visible, unexplored
  **5.0 %**; the veil gradient across the boundary is a ~320 px ramp (measured
  ratio scan 0.0 → 1.0 over 16 samples of 20 px), i.e. the documented 2-cell
  blur + bilinear tap is real.
* `Assets::load`: **1.7 s** with `War3.mpq` only (the test fixture), **86.5 s**
  with all three archives under a contended device.

## 2. Gap table

| # | Defect | Mechanism (file:line) | Reference behaviour | Fix | Status |
|---|---|---|---|---|---|
| D1 | White polygons (~33 k px, RGB 247,247,247) where doodads stand | RoC doodad skins live in the base archive; with only `War3x.mpq` mounted the decode fails and `doodads.rs:443,448` fall back to texture index `0` (`textures[0]` = white stub), drawn by `render.rs` | Retail draws the authored skin; a missing one is never pure white | (a) evidence/acceptance frames mount the full install; (b) a decode miss now uses a neutral 8x8 earth material + one bounded `GAME_DOODAD_TEX_MISS path=…` log line instead of the white stub | **fixed — measured** (near-white 33 356 px → 468 px, 1.28 % → 0.02 %) |
| D2 | Cliff plateau tops looked like flat untextured slabs | `terrain_geometry.rs:323-392` builds one flat plate per cliff cell (`height + CLIFF_TOP_LIFT`), walls per shared edge; `terrain_render.rs:769-783` copies the ground weights | WC3 cliffs are blocky too, but the top carries the ground material and a lit lip | Investigation: the tops ARE textured (patch sd 25/24, same as ground 25) and the `--no-overlays` A/B removes them entirely (`orc01-nov-noon-2414x1080.png`): they come from `cliff_blocks`, not from a flat-shaded terrain path. Reclassified **P2 (shape/tuning)**, not broken | investigated, downgraded |
| D3 | Achromatic ~45° moiré on the far half of the frame | Per-cell authored variation: each 128-unit cell picks its own 16x16 quadrant (`terrain_render.rs:975-1004`) with no distance falloff; a mip chain collapses each quadrant to one flat colour, so the far field becomes a grid of flat colours that differs cell by cell | WC3 has no mip chain, so its variation never aliases | Slide the quadrant toward one reference quadrant with LOD; `params.water.w` carries the fade start, `H2_TERRAIN_VARIATION_FADE=0` disables it (single-binary A/B) | fixed, A/B pending |
| D4 | Doodad ground shadows used a single centre→rim fan | `render.rs` (doodad batch): alpha 0.30 at the centre, 0 at the rim — zero exactly where the blob becomes visible outside the footprint; the unit path already uses a three-ring profile (`mesh.rs:2384-2412`) | Every WC3 model gets a soft blob with a visible core | Three-ring fan (core 0.34, plateau 0.27 at 60 % radius, rim 0), 20 segments | fixed, A/B pending |
| D5 | "Dawn" at 06:00 is pixel-identical to noon | `KLAC` reaches full white at `at=0.25` = 06:00 (`daynight.rs:1366-1369`) | WC3's ramp is 05:20–06:00 | Capture dawn at 05:30 and stamp the hour in the artifact name | **resolved by measurement** (dawn 05:30 = 62.54 vs noon 73.50) |
| D6 | "Dusk" at 18:00 is only 5 % darker than noon | the dusk ramp starts at `at=0.7506` = 18:01 and the warm key is `at=0.804` = 19:17 | retail dusk is the warm phase after 19:17 | Capture dusk at 20:00 | **resolved by measurement** (dusk 20:00 = 60.00, R/B 4.01) |
| D7 | The synthetic three-band FoW artifact frame reads as fully unexplored | `world_frame.rs` banded mask splits the *map* into thirds while the camera looks diagonally, so one band covers the whole view | — (artifact only) | Use the real sim mask for the visual artifact and the uniform masks for the ratios | fixed in the probe |
| D8 | Terrain array layers are capped at 8; extra tags fold into the nearest by average colour | `terrain_render.rs:325-353` | Retail supports every tag | Count the tags of every acceptance map before raising the cap | open (needs `--stats`) |
| D9 | Orc01 water: surface renders and is data-faithful; **the shore still reads as a hard water/land cut** — no visible foam line at the edge | `build_water` `terrain_render.rs:550`, `build_shoreline` `:719`, `fs_shore` `:1102-1115` | Retail draws `Shoreline.blp` foam along the edge with a soft alpha ramp | Proved the surface: 12 214 water cells, ramp `Smax=(117,117,200,219)` straight out of `Water.slk` (`analysis/war3_full/TerrainArt/Water.slk:211-243`), wave frames animated. The foam band needs a close-up render pass to finish | surface **proved**, foam open |
| D10 | `cargo test -p wc3-game --lib terrain_render::` held the shared cargo lock >15 min | default harness runs 8 tests at once; each does a full `Assets::load` (2-3 GB transient), and four GPU tests held the device mutex *across* the load | A filtered run must be bounded | Keyed shared read-only load per fixture (`shared_assets`), GPU lock taken after the load, the device-only close-up marked `#[ignore]`, `rendering_terrain_water_plane` on the shared load instead of two extra loads | fixed (re-measured: see section 4) |
| D11 | `cargo test -p wc3-game --lib` did not compile at all | `assets.rs:4781-4786` called `source.exists(..)` where `StormSource` implements two traits with that method (E0034) | — | Name the trait: `wc3_data::source::Source::exists(&source, ..)` | **fixed** (compiler-suggested disambiguation) |

## 3. Artifacts (2414x1080)

Full install, pre-fix:

* `orc01-pre-dawn-2414x1080.png`, `orc01-pre-noon-…`, `orc01-pre-dusk-…`,
  `orc01-pre-night-…`
* `orc01-pre-fog-{visible,explored,unexplored,sim}-2414x1080.png`
* `orc01-fullpre-noon-2414x1080.png` (D1 after: no white polygons)
* `orc01-nov-noon-2414x1080.png` (`H2_TERRAIN_OVERLAYS=off` A/B for D2)
* `crop-cliff-full-4x.png`, `crop-white-4x.png`, `crop-trees.png`,
  `crop-ground-far.png`

Standalone `War3x.mpq` (D1 before): `orc01-noon-2414x1080.png`,
`orc01-dawn-…`, `orc01-dusk-…`, `orc01-fog-*`.

## 4. Tests (one per mechanic) — measured

`scripts/h2-cargo test -p wc3-game --lib terrain_render:: --offline -- --test-threads=1`:

```text
test result: ok. 20 passed; 0 failed; 11 ignored; 0 measured; 432 filtered out; finished in 509.30s
TEST_RC=0 wall_s=728
```

`scripts/h2-cargo test -p wc3-game --lib render_doodad_shadow_profile`:

```text
test render::tests::render_doodad_shadow_profile ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 462 filtered out; finished in 34.62s
```

Two context numbers for the stall row (D10): before the fix the same filter held the
lock for **>15 min** and two tests never returned; the pre-fix filter had already
measured **98 s** on an idle device (cargo log 01:40-01:41). The 509 s above is the
run on a device with four other cargo jobs and ~400 MB free; the remaining cost is
the three deliberately-kept headless-GPU regressions
(`agent_world_real_map_has_no_cell_seams`, `agent_world_units_are_readable_at_1080p`,
`agent_world_water_surface_renders`) plus three fixture loads. Every pure data test
in the filter now returns in seconds, and the two tests the tech-lead reported as
hanging return immediately (`agent_world_doodads_are_placed ... ok`) or are
`#[ignore]`d with a stated reason (`agent_world_hero_models_closeup`).

### Coverage rows

| test | id | asserts |
|---|---|---|
| `render_cliff_top_material` | `render.cliff.top-material` | every plateau top carries ground weights (no empty weight vector) and a cell-space UV |
| `render_terrain_variation_fade` | `render.terrain.variation-fade` | the shader keeps the fade, and `H2_TERRAIN_VARIATION_FADE=0` disables it |
| `render_doodad_shadow_profile` | `render.doodad.shadow-profile` | the uploaded doodad batch has a shadow spec with a ≥0.30 core and a plateau ring |
| `render_fow_three_states` (existing) | `render.fow.three-states` | explored 35–55 %, unexplored ≤6 % |
| `render_lighting_hour_changes` (existing) | `render.lighting.hour-changes` | the DNC curve moves the light |
