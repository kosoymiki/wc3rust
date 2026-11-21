# HiveWE → wc3-game terrain byte matrix (ledger 384, 2026-11-14)

Donor: `donors/HiveWE` (523 files, C++23 modules; 128 `.cpp/.h/.hpp/.ixx`,
30 245 lines). Consumer: `crates/wc3-game/src/terrain_render.rs` (+ the terrain
height source in `assets.rs`). This is the file-to-file evidence table the H2
donor pipeline requires; every "adopt" row names the exact donor line and the
consumer line that now implements it.

## Files read (byte-level, not README claims)

| Donor file | Size | What it actually contains |
|---|---:|---|
| `src/base/terrain.ixx` | 1611 lines / 54.1 K | W3E loader, final heights, cliff detection, cliff MDX instancing, water/cliff ground-texture selection, minimap image |
| `src/resources/cliff_mesh.ixx` | 109 lines | Instanced cliff/ramp MDX renderer, per-instance `vec4(tile_x, tile_y, base_layer_height, cliff_texture)` |
| `src/resources/ground_texture.ixx` | 117 lines | BLP sheet → 16/32 `128 px` array layers, `extended = width == height*2`, `minimap_color` |
| `data/shaders/terrain.vert` | 43 lines | Per-quad instancing, per-corner `flat uvec4` texture ids with variation in the upper 16 bits |
| `data/shaders/terrain.frag` | 60 lines | Ordered alpha compositing of the four corner layers + lighting |
| `data/shaders/water.vert` | 47 lines | Per-cell water quad, any-corner water test, depth colour ramp |
| `data/shaders/water.frag` | 14 lines | `texture(Water.blp, UV) * Color` |
| `data/shaders/cliff.vert` | 33 lines | MDX rotation `(x,y,z) → (y,-x,z)/128 + instance`, terrain normal blend |
| `data/shaders/cliff.frag` | 28 lines | Cliff texture array sample + lighting |
| `src/base/tilesets.ixx` | 363 lines | `TerrainArt`/`CliffTypes`/`Cliffs` data, `water_offset = Water.slk height` |
| `src/base/pathing_map.ixx` | 322 lines | Cliff/water pathing flags |
| `src/base/terrain_undo.ixx` | 2.0 K | Editor-only undo (out of scope) |
| `src/brush/terrain_operators.cpp/.h`, `src/brush/terrain_brush.cpp/.h` | — | Editor brushes (out of scope for runtime) |
| `src/menus/terrain_palette.*`, `src/resources/pathing_texture.ixx` | — | Editor UI/overlay (out of scope) |

Measured research (our side): `Melee.w3x` W3E is **version 11, 7-byte tiles**,
tileset `Y` (Cityscape), 8 ground tags, corner **layer heights 1..7**
(`layer_hist {1:100, 2:153, 3:739, 4:1673, 5:3952, 6:5741, 7:155}`), 1 487 water
corners, 1 839 cells with at least one water corner (830 of those with a dry
corner), ground −2.54..3.47 tiles. Water.slk row `LSha`:
`height=-0.7`, `Smin=(255,255,255,10)`, `Smax=(117,117,200,219)`,
`Dmin=(117,117,200,219)`, `Dmax=(96,96,192,250)`, `texRate=15`,
`texFile=ReplaceableTextures\Water\Water`.

## Per-layer matrix

| # | Layer | HiveWE source (file:line) | Ours before | Verdict | Our implementation |
|---:|---|---|---|---|---|
| 1 | **Corner heights** | `terrain.ixx:411` corner height `(u16-8192)/512`; `:269-272` `corner_final_ground_height = corner_height + layer_height - 2.0` (tile units) | `terrain_render.rs:308` used `tile.height()*128` only — **cliff layer ignored** | **ADOPT (root fix)** | `final_height()` (`terrain_render.rs:882-888`), used by the mesh, water bed, AO, normals, `assets.rs::height_at` and the audio occluder grid |
| 2 | **Cliff level** | `terrain.ixx:441` `layer_height = misc & 0x0F`; `:1561-1573` cell is cliff when its 4 corner layers differ; `update_ground_exists` hides the ground quad for cliff cells | unused | **ADOPT (data)** / **ADAPT (geometry)** | per-corner layer heights in the mesh; WC3 cliffs are per-cell rounded slopes rendered by authored MDX (`CliffsAAAB0.mdx`: 128×128 footprint, 0→128 height, S-profile 128/110/17/0), which our per-corner layer heights reproduce as a 45° textured face (see Limitations) |
| 3 | **Cliff walls** | `terrain.ixx:1091-1215` models `Cliffs<4-letter diff><var>.mdx`; `:626-636` instance `(tile, base=min layer, cliff texture)`; `cliff.vert:13-17` rotation/scale | missing entirely (flat map) | **ADOPT (heights)** / **DEFER (MDX instancing)** | layer heights restore the walls as terrain faces; the authored `Cliff0.blp` layer (`terrain_render.rs:1919-1969`) shades them by slope with vertical UV. MDX cliff meshes need the MDX→Vulkan skinned-mesh pipeline and are recorded as the next visual step (not required for "no missing walls": the relief is present and textured) |
| 4 | **Ramps** | `terrain.ixx:869-879` `is_corner_ramp_entrance`; `:943-990` ramp entrance raises base corners by +0.5 | `is_ramp()` parsed but unused for geometry | **ADOPT (data)** | W3E ramp cells are real height slopes after `final_height`; the +0.5 lip is cosmetic and deferred with the MDX cliff meshes |
| 5 | **Per-cell texture layers** | `terrain.ixx:1010-1042` four corner texture ids per cell + 16-bit variation; `terrain.frag:22-33` | 8 fixed global layers with per-vertex weights | **KEEP ours** (same result, more materials per map) | 8 array layers + per-vertex weights, cliff layer 8 |
| 6 | **Variation selection** | `terrain.ixx:428-429` variation is **5 bits**; `:795-810` non-extended: 0→tile 0, else tile 15; extended: n→layer 16+n | `variation() & 0x0F` (4 bits) and `variation % 4` quadrant — sampled mask tiles, black holes | **ADOPT (fix)** | `terrain_render.rs:1701-1713` stores `variation_data & 0x1F`; packer maps quadrant→engine layer (`pack_variation_sheet`, `:1024-1113`); shader maps 0..15→quadrant, 16→15, >16→0 (`TERRAIN_SHADER`, fs_main) |
| 7 | **Tile sheet layout** | `ground_texture.ixx:74-84` `tile_size = height/4`, `extended = width == height*2`, layers 16/32, `tile_x = (layer%16)%4 + (layer>=16?4:0)` | fixed 128 px quadrants, row-major flattening | **ADOPT** | `pack_variation_sheet` derives tile size from `height/4`, handles the 8×4 extended half exactly, and fills alpha-mask holes with the base colour so mips do not darken (the checkerboard) |
| 8 | **Tile blending** | `terrain.frag:27-33` ordered `mix(next, over, over.a)` for 4 layers | sum of `weight*(0.35+0.65*alpha)` with black holes leaking | **ADAPT** | hole-filled RGB + mask alpha in the weights; no black leak, no cell rectangle; cliff layer by slope |
| 9 | **Tile variation anti-seam** | per-variation array layer (`ground_texture.ixx:96-120`) → world UV repeats exactly | `fract(cell_uv)*0.25` + implicit-mip derivative jump at every cell border | **ADAPT** | explicit LOD computed from the continuous cell UV (`dpdx/dpdy * 512`) in `fs_main`; the `fract` jump can no longer select the whole-sheet mip |
| 10 | **Water geometry** | `water.vert:19-24` quad per cell kept when **any** of 4 corners has the water flag; `:31-38` per-corner `water_height + water_offset` | quads only when the bottom-left corner was water; shared corners averaged; `+2.0` fudge | **ADOPT (fix)** | `build_water` (`terrain_render.rs:516-630`): any-corner test, per-corner level `(water_height + offset)*128`, no +2 |
| 11 | **Water offset** | `tilesets.ixx:288` `water_offset = Water.slk height`; `terrain.ixx:274-276` `corner_final_water_height = corner_water_height + water_offset` | missing → surface **89.6 units too high** (water on dry land) | **ADOPT (root fix)** | `WaterMaterial.offset` read from `height` (`terrain_render.rs:1524-1530`); `final_water_height()` (`:890-894`) |
| 12 | **Water colour ramp** | `water.vert:9-11` `min_depth=10/128`, `deeplevel=64/128`, `maxdepth=72/128`; `:41-47` `Smin→Smax`, `Dmin→Dmax` with alpha | invented `smoothstep(0,220)` + procedural alpha `0.22+0.54t` → pale lavender | **ADOPT (fix)** | four Water.slk RGBA rows (`shallow_min/max`, `deep_min/max`), `water_ramp()` (`:896-967`) evaluated per corner |
| 13 | **Water blend state** | `terrain.ixx:168-174` water pipeline: blend SRC_ALPHA/ONE_MINUS_SRC_ALPHA, cull NONE, depth test on, write off | same pipeline settings | **KEEP** | `TerrainRenderer::new` water pipeline (`:2432-2442`) unchanged; the alpha now comes from Water.slk instead of the old fudge |
| 14 | **Water texture** | `terrain.ixx:1580+` `ReplaceableTextures\Water\Water` frames, `texRate` | Water.slk `texFile` + frame `00`, `texRate` scroll | **KEEP** | `water_material()` + `fs_water` |
| 15 | **Minimap** | `terrain.ixx:884-901` ground-texture average colour per corner, cliffs grey (128), water colour | uses the map's own `war3map.mm`; no synthetic minimap | **KEEP** (map-authored is more faithful) | documented only |
| 16 | **Pathing** | `terrain.ixp`/`pathing.ixx` cliff/water flags, 4×4 pathing cells | `wc3-formats::wpm` flags, fixed in ledgers 376/380 | **KEEP** | not re-ported |
| 17 | **Shadows** | `src/render/shadow_map.ixx` (editor-only, no terrain shadow pass) | none | **REJECT** | out of runtime scope |

## Visual defects → root cause → fix (file:line)

| Defect (operator) | Root cause (observed) | Fix |
|---|---|---|
| Water covering dry land | water surface at `water_height*128` (missing Water.slk `-0.7` tiles) and terrain bed at `height*128` (missing `layer_height-2` tiles); the old alpha also used a per-cell lookup with world UV instead of cell UV | `final_water_height` + `final_height`; per-corner ramp alpha (dry corner = Smin α10/255); removed the `on_water` lookup |
| Terrain seams / grid | black alpha-mask holes averaged into mips + implicit-mip derivative jump on `fract(cell_uv)` at every 128-unit border | hole-filled packer + explicit LOD from the continuous UV |
| Dark checkerboard squares | sampling all 16 subtextures as albedo; 14 of them are masks (HiveWE uses only 0/15 or 16+n) + 4-bit variation truncation (16/17→0/1) | engine subtexture mapping + five-bit variation |
| Missing cliff walls | `cliff_level()` never entered the height function → map rendered flat (−2.5..3.5 tiles instead of −1..6.8) | `final_height` everywhere (terrain, water, normals, AO, units via `assets.height_at`, audio occluder) |

## Limitations / next visual step (honest)

- WC3 cliff faces are authored MDX models (`Cliffs*.mdx`) with an S-profile
  (128→110→17→0 over the cell) and a `Clifftrans` ramp mesh. Our terrain mesh
  renders the same corner heights as a linear 45° face textured by the real
  `Cliff0.blp`; the silhouette is correct, the authored bevel is not. Full
  parity needs the MDX geometry instanced like `cliff_mesh.ixx:626-636`, which
  depends on the model→GPU path rather than 20 lines in the terrain pass.
- Editor-only HiveWE modules (brush, undo, palette, regions, brush overlay,
  shadow map, pathing overlay) were read for completeness and rejected as
  runtime scope.

## Evidence

- `cargo test -p wc3-game --lib terrain_render` → rc=0, see
  `analysis/agent-world/terrain-hivewe/gate.log`.
- Headless close-ups: `analysis/agent-world/terrain-hivewe/*.ppm` (cliff, ramp,
  shoreline) and the 2414×1080 device frame.
- Device: `analysis/agent-world/terrain-hivewe/device-before.jpg`,
  `device-after.jpg`, logcat `GAME_TERRAIN …` (`analysis/agent-world/terrain-hivewe/device.log`).
- HiveWE cliff mesh geometry: `mdx-gltf` export of
  `Doodads\Terrain\Cliffs\CliffsAAAB0.mdx` (20 verts, 12 tris, bbox
  `x -128..0, y 0..128, z -128..0`).
