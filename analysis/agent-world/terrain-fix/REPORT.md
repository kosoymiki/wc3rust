# TERRAIN RENDER FIX — real tile variations + cliff layer (ledger 380)

> **Superseded by ledger 384** (`analysis/agent-world/terrain-hivewe/REPORT.md`):
> the remaining gaps listed at the bottom of this file (pale water, missing
> cliff walls, shore coverage) were root-caused against HiveWE and fixed
> (cliff layer heights, Water.slk -0.7 offset + four-colour ramp, any-corner
> water quads, 5-bit variation/subtexture packing, hole fill, explicit LOD).

## Root cause
Before: each TerrainArt BLP was composited into ONE averaged 128px tile per
ground tag (`wc3_render::terrain::tile_from_sheet`) and sampled with world UV,
so every 128-unit cell showed the same blurred average: flat brown ground with
no authored variation, and steep cliff quads were textured like flat ground.

## Fixes (file:line, crates/wc3-game/src/terrain_render.rs)
- `:916` `pack_variation_sheet()` — packs every authored BLP variation into a
  4x4 sheet of 128px quadrants (512px) with a wrapped mip chain, preserving the
  real alpha mask (no averaged single tile).
- `:861` `TileSet.variants/variant_width/variant_height`; `load_tile_set`
  (`:1410`, `:1451`, `:1500`) builds the layers with `pack_variation_sheet`,
  normalizes every layer to 512px, reads the per-cell W3E `variation()` nibble
  into `variants`, and appends a real cliff layer.
- `:1501` cliff layer: `ReplaceableTextures\Cliff\Cliff0.blp` decoded and
  packed as array layer 8 (`GAME_TERRAIN layers=9`).
- `:1600`/upload: R8Uint per-cell variation texture (`binding 5`) uploaded in
  `upload_tile_array`, bind-group entry + layout entry.
- Shader `:651` `@binding(5) variants: texture_2d<u32>`; `fs_main` computes the
  cell, loads the authored variation, and samples all 8 ground layers with the
  selected 4x4 quadrant UV (fract(world_uv) mapped to 0.25 quadrants).
- Shader `:765`: steep terrain (slope from the interpolated normal) mixes the
  real cliff layer with vertical world UV.

## Evidence
- `cargo test -p wc3-game --lib terrain_render --offline` → **rc=0**
  `12 passed; 0 failed; 6 ignored` (86.79 s) — `terrain-fix/terrain-gate.log`.
- `cargo check -p wc3-game --lib` → **rc=0** (release APK built from same tree).
- Device (RMX3709, H2_DEVICE_SLOT=1, current APK):
  `GAME_TERRAIN_TILES source=terrainart tags=8 resolved=8 water_tex=true`
  `GAME_TERRAIN layers=9 vertices=29025 tris=57344 water_vertices=13163`
  `GAME_READY map="Орда01"` — 93 FPS frame captured.
- Screenshots:
  - before: `terrain-fix/before.png` (flat averaged brown ground)
  - after: `terrain-fix/shot.png` = `after1.png` (authored dirt variation,
    visible detail/patchwork, rocks/trees crisp)
  - additional: `terrain-fix/after2.png` (map edge/shoreline view)
  `analysis/tech-lead/device-shots` does not exist on this tree, so the
  comparison baseline is our captured `before.png`.

## What changed visually
Ground is no longer one stretched average: each cell samples its authored
TerrainArt variation, so dirt/grass patches and fine detail appear; the ground
grain is authored (not procedural noise), and steep slopes mix the real
Cliff0.blp layer instead of flat ground texture.

## Remaining visual gaps (exact)
- Water plane at the shoreline in `after2.png` reads pale/lavender rather than
  blue Water.blp detail; depth tint + `Water.slk` shallow/deep colours need a
  follow-up diagnostic (log the resolved shallow/deep RGB and water tex path).
- Cliffs are steep terrain quads with the cliff texture mixed by slope; there is
  no dedicated vertical cliff-wall geometry pass, so very steep seams can still
  show stretched ground where the slope threshold is not met.
- Map border: terrain ends at the last W3E cell (sky beyond), which can read as
  a hole with edge doodads floating over the clear colour; an edge skirt/water
  skirt is a candidate next fix.
- Only one fix cycle was used (limit 3); no further device iterations were made.

## Water follow-up (steer on Melee.w3x)
- Root cause of the "whole map submerged" look: the translucent grey-blue was
  the **aerial haze** (`haze()` mixed 60 % grey-blue from 2200 units), not the
  water mesh. The water mesh emits quads only for `cell.is_water()` cells and
  the new gate asserts `land_tris=0` on Human04, Orc01 and the reported
  `(2)Ponds_v1.4.w3x` (761 water cells, 1522 water tris, 0 land tris).
- Water now uses the canonical `final_water_height(tile, Water.slk offset)`
  per corner, the per-cell water-level texture (R32Float, binding 6) drives
  underwater darkening and per-cell alpha (`on_water`), so dry land cannot get
  water alpha; borders keep the dark clear colour (no skirt).
- Haze reduced to `smoothstep(3500, 9000) * 0.28`, so distant terrain no longer
  reads as a grey-blue sheet.
- Gate: `cargo test -p wc3-game --lib terrain_render` rc=0 (12 passed, 6 ignored).
- Device: current staged `Melee.w3x` is now the "Justice" map (129x97, center
  -8192/-10752); the 3D scene renders very dark (terrain only as dark green,
  HUD bright) with no water-over-land veil; `GAME_TERRAIN layers=9
  water_vertices=2342 water_tris=3678`, `GAME_SCENARIO melee-ready map=Justice`.
  Screenshot: `analysis/agent-world/terrain-fix/water-fix.png` (HUD + dark
  scene). The dark scene is a cross-owner issue (map/scenario lighting after the
  map swap), not water; the earlier bright Melee frame is `water-before.png`.
- Remaining: re-shoot a lit pond view once the Justice-map lighting/scene is
  fixed by its owner; verify the pond shoreline alpha visually.
