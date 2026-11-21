# Terrain water / shoreline / cliff — LEDGER 441

Operator row 14: «какие-то проблемы с ландшафтом ещё присутствуют» / «Terrain looks
bad», missing artifact: **a correct water overlay/border frame + a screenshot at the
map edge**.

Map measured: `donors/War3Net/.../(2)Justice.w3x` — 129×97 W3E corners (128×96
cells), 1487 wet corners, cliff layers 2..7, cliff types `CYdi`/`CYsq`. Device frame
2414×1080.

## 1. Observed before the fix

Headless render of the Justice east coast at 2414×1080, same camera and build
with `H2_TERRAIN_OVERLAYS=off` (pre-fix geometry):

* `JUSTICE_EDGE tag=before shot=map-edge target=(16192,2112) distance=1500
  flat_clear=0.3996 water=0.7597` — **40 %** of the frame was exactly the engine
  clear colour `(60,77,93)`; that uniform band is the operator's "flat blue band".
* Water surface had no authored animation (frame 0 only), no shore band and a hard
  polygonal edge at the water/land boundary.
* Cliff faces were the smooth 45° heightfield interpolation with a whole-sheet
  world UV: the rock texture was smeared and the cliff layer was packed with the
  ground-mask semantics (only two of the sixteen sheet cells survived).
* The start view (`before-justice-edge-2414x1080.png`) rendered grass, trees and
  the hall, measured by the acceptance gate:
  `terrain_sharpness ... std=(24.3,27.2,21.5) edge=0.988 -> PASS`.

Artifacts: `analysis/agent-qa/terrain/before-justice-water-cliff-2414x1080.png`,
`analysis/agent-qa/terrain/before-justice-edge-2414x1080.png`,
`analysis/agent-qa/terrain/before-justice-map-edge-2414x1080.png`,
`analysis/agent-qa/terrain/baseline-melee-pond.png`,
`analysis/agent-qa/terrain/baseline-melee-cliff.png`.

## 2. Root causes (file:line, observed)

| # | defect | evidence |
|---|---|---|
| 1 | water ends at the last map cell; beyond it the pass clear colour is the only thing drawn (flat band) | `crates/wc3-game/src/terrain_render.rs` `build_water()` emitted only in-map quads; measured `flat_clear=0.4377` at the coast |
| 2 | no shoreline overlay: Water.slk names `Shoreline`/`ShorelineWave`/`ShorelineOutsideCorner`/`ShorelineInsideCorner` and `ShoreWaveModel` exists in Game.dll, but no shore geometry/texture was ever drawn | `analysis/agent-qa/parity/parity-render.md:79-87` (documented gap); baseline pond shot hard edge |
| 3 | cliff faces were smooth ramps with a whole-sheet UV; the cliff BLP went through the ground variation packer | `terrain_render.rs` `fs_main` steep branch (`cliff_uv = (world.x+world.z)*0.0078, world.y*0.0078`) + `load_tile_set` cliff layer `pack_variation_sheet(...)` |
| 4 | water animation missing: only `Water00.blp` was loaded and merely scrolled; Water.slk `numTex=45`, `texRate=15` | `water_material()` loaded frame 0 only |

## 3. Fix

New pure module `crates/wc3-render/src/terrain_geometry.rs` (no GPU, no archive):

* `water_border(grid, border_cells, fade_cells, tile, deep)` — extrusion of the
  water surface past the map boundary wherever a boundary corner is wet, 8 tiles
  out, outermost 3 rows fades alpha to zero (clear colour becomes distance haze,
  not a hard band edge). UVs are world-space, continuous with the interior water.
* `shoreline(grid, half_width, tile, lift, uv_period)` — one foam band quad per
  lattice edge whose two corners disagree on the W3E water flag, at the wet
  corner's surface height + 1.5 units.
* `cliff_blocks(grid, tile, lift)` — for every cell whose four corner layers
  differ (and that is not a ramp or fully submerged): a flat plateau top at
  `max(corner height) + 1` plus vertical faces down to the neighbour's surface.
  The face carries `[variation, 0, cliff_texture, 0]`, the top the source corner
  for its ground material.
* `water_depth_color(...)` — the WC3 Smin/Smax/Dmin/Dmax depth ramp (shared with
  `water_ramp`), `sheet_quadrant(variation)` — 4×4 sheet quadrant selector.

Wiring (`crates/wc3-game/src/terrain_render.rs`, `render.rs`):

* `build_terrain` builds `border_*`, `shore_*`, `cliff_*` meshes from
  `corner_grid(assets, water_offset)`; `draw_water` draws the border frame with
  the same material; `draw_shore` runs a new `fs_shore` entry sampling the
  authored `Textures\Shoreline1.blp` (new array binding 7); `draw_cliffs` runs
  the cliff block/face geometry through the terrain pipeline with a wall branch
  (`weights.x = 900`, array layer `8 + cliff_texture`, quadrant UV).
* Water.slk frames `Water00.blp … Water44.blp` are uploaded as one
  `texture_2d_array` and stepped at `texRate` fps (`fs_water`).
* Every map cliff type (`war3map.w3e` cliff list → `CliffTypes.slk` texDir/texFile)
  is uploaded as its own array layer (8, 9, …); the steep-slope branch now samples
  one quadrant of that sheet instead of the whole sheet.

No `assets.rs` change was needed: the cliff and shoreline textures are read
through the same `StormFs` the terrain material already uses. Nothing in HUD or
sim was touched.

## 4. Proof

* `crates/wc3-render/tests/terrain.rs`:
  * `render_terrain_water_shoreline` — depth-ramp alpha 10/255 at the shore and
    250/255 in deep water, one foam quad per wet/dry lattice edge, border only on
    wet map sides, border opacity fade to zero at the outer row, border starts at
    the W3E water level.
  * `render_terrain_cliff_face` — a layer transition yields a flat plateau top at
    the upper layer with source-corner material, vertical faces with `v = 0` at
    the top lip and `v > 0.9` at the bottom, no block for flat/ramp/open-water
    cells, quadrant selector `variation 5 -> (0.25, 0.25)`.
* `scripts/h2-cargo test -p wc3-render --offline` → **RC=0** (96 + 4 + 3 + 5 +
  10 + 3 + 3 + 2 tests passed; `tests/terrain.rs` 2/2).
* `python3 analysis/tech-lead/terrain_sharpness.py` → **PASS** on the acceptance
  frame (`std=(29.3,25.4,24.6) edge=0.974 -> PASS`, rc=0), and PASS on the new
  headless edge frame (`std=(24.3,27.2,21.5) edge=0.988`, rc=0).
* Headless Justice frames (2414×1080, `wc3-render/examples/terrain_edge.rs`,
  device frame + map-edge frame + water/cliff frame):
  * before: same build with `H2_TERRAIN_OVERLAYS=off` (pre-fix geometry) —
    `analysis/agent-qa/terrain/before-justice-map-edge-2414x1080.png`,
    `before-justice-water-cliff-2414x1080.png`, `before-justice-edge-2414x1080.png`;
  * after: `analysis/agent-qa/terrain/after-justice-map-edge-2414x1080.png` (the
    operator artifact), `after-justice-water-cliff-2414x1080.png`,
    `after-justice-edge-2414x1080.png`.
  * measured A/B at the same camera (`target (16192,2112)`, distance 1500):
    `flat_clear 0.3996 → 0.0101`, `water 0.7597 → 0.6801`; the mesh carries
    `border_vertices=5152 border_tris=2576 shore_tris=1688 cliff_tris=9410`
    (`GAME_TERRAIN` log line, `layers=10` = 8 ground + 2 map cliff textures).
  * the after map-edge frame shows the animated water surface continuing past
    the map boundary, the water border frame and the vertical rock faces of the
    coastal cliffs (`tag[1]=1` plateau tops, `tag[1]=0` walls).
* `scripts/h2-cargo test -p wc3-game --lib --offline terrain` → **RC=0**
  (`test result: ok. 19 passed; 0 failed; 8 ignored; 396 filtered out`).
* Mechanic registration (analysis/tech-lead is read-only here; exact rows for the
  tech lead):
  * `mechanic_targets.tsv`:
    `render.terrain.water-shoreline\twc3-render\t--test terrain render_terrain_water_shoreline`
    and `render.terrain.cliff-face\twc3-render\t--test terrain render_terrain_cliff_face`.
  * `mechanics_gap_map.json` rows: `{"id":"render.terrain.water-shoreline",
    "status":"PORTED","impl":"crates/wc3-render/src/terrain_geometry.rs shoreline/water_border",
    "test":"render_terrain_water_shoreline"}` and the same shape for
    `render.terrain.cliff-face` (`cliff_blocks`).
* No `assets.rs` change was needed (the cliff/shore textures are read through the
  same `StormFs` the terrain material already uses) and no HUD/sim file was
  touched.


## 5. Risks / next

* The cliff block is a data-driven stand-in for the engine's authored
  `Cliffs<letters><variation>.mdx` models: same footprint, plateau and vertical
  face, but a rectangular face instead of the model's rounded lip.
* The map-edge water frame is a presentation fix for the mobile camera, which can
  zoom past the WC3 camera bounds; the original client never shows that region.
* The shoreline particles (`Shoreline*.mdx` PRE2 emitters) are not spawned; the
  wave strip (`ShorelineWave`/`Shoreline1.blp`) is.
