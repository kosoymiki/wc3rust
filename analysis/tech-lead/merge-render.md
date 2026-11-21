# merge-render — render/terrain/model layer → canonical merge base

Status: **v1 (2026-11-12 17:0x MSK), draft for the merge owner.** Written per
`analysis/tech-lead/MERGE_SOURCE_OF_TRUTH.md` (read). `WC3RE_MERGE_MATRIX.md`
and `BASE.md` are **not present** in `analysis/tech-lead/` at the time of
writing, so this maps against the public API of `crates/wc3-re` + the current
game asset layer. No code changes, tests, or device runs are being made while
waiting for `BASE.md` / the merge owner's assignment (per the latest order).

Rules applied: no second reader/parser in render/terrain; consume
`wc3-re` for maps/tiles/doodads/models/SLKs, `wc3-formats` for the parsed
W3E/MDX types, and the game asset layer for GPU-facing data. "MOVE" = function
belongs in the canonical base; "PORT" = stays in the game renderer but must be
rewritten against the base API; "DELETE" = duplicate reader removed; "KEEP" =
presentation logic with no base equivalent.

## Canonical base API inventory (targets used below)

| Target | Location | Notes |
|---|---|---|
| `MapBundle::open`, `MapBundle{terrain,pathing,info,wts,objects,entries}` | `crates/wc3-re/src/mapfiles.rs:67-195` | canonical W3M/W3X reader; does **not** expose `war3map.doo` bytes or parsed doodads |
| `MAP_INNER_FILES` | `crates/wc3-re/src/mapfiles.rs:32-58` | includes `war3map.doo`, `war3mapUnits.doo` (only names/sizes exposed) |
| `TerrainArt::load_dir/load_terrain`, `tile`, `texture_paths` | `crates/wc3-re/src/terrain.rs:57-120` | `Terrain.slk` + `CliffTypes.slk` + `Water.slk` + `Weather.slk`; directory-based |
| `TerrainTile::texture_path`, `variant_uv` | `crates/wc3-re/src/terrain.rs:43-55` | canonical 2×2 variant UV rule (0.0/0.5) |
| `StormFs::{add_archive,open_wc3_install,read,read_with_codecs,listfile,search}` | `crates/wc3-re/src/storm.rs:101-278` | canonical multi-archive filesystem |
| `blp::decode`, `decode_first_mip`, `probe`, `DecodedBlp::rgba/mips` | `crates/wc3-re/src/blp.rs:91-233` | canonical BLP decode (CMYK/BGR fixes live here) |
| `mdx::parse`, `textures`, `layers`, `referenced_textures` | `crates/wc3-re/src/mdx.rs:128-160` | canonical MDX parse/chunk view (`wc3_formats::mdx::MdxModel`) |
| `objectdata::ObjectDb::load_storm`, `record`, `ObjectRecord::slk_field`, `known_ids` | `crates/wc3-re/src/objectdata.rs:127-423` | loads `Units/*.slk` (incl. `DestructableData.slk`) + `Doodads/Doodads.slk`; `slk_field("Doodads"/"DestructableData", "dir"/"file"/"numVar"/"texID"/"texFile")` |
| `slk::Sheet::{parse,cell,record,row_index}` | `crates/wc3-re/src/slk.rs:16-91` | canonical SLK reader |
| `mpqex::MpqEx::{open_at,read,listfile}` | `crates/wc3-re/src/mpqex.rs:36-91` | low-level map/archive inner read |
| `scan::{MapScan,TerrainScan,ObjectScan}` | `crates/wc3-re/src/scan.rs:99-135` | canonical audit counters (tiles/cliffs/water/weather/missing textures) |

Current game-layer asset bridge (legacy, to be replaced by the merge base):
`wc3-assetdb::AssetDb` + `wc3_formats::map::W3Map` inside `assets.rs`; the
merge patch `analysis/tech-lead/patches/0001-assets-mapbundle-wts.patch`
already switches `Assets::load` to `MapBundle`. `Assets::terrain_tile_images`
is a temporary bridge added for the terrain array and must not survive the
merge.

## 1. `crates/wc3-game/src/terrain_render.rs` (GPU terrain)

| Function (current file:line) | Behavior today | Canonical target | Action / dependency |
|---|---|---|---|
| `Biome`, `Biome::{from_index,srgb,linear,material}` (37-86) | 6-class material classification from W3E tag + BLP average | no base equivalent; source tile IDs from `TerrainArt::tile(id)` (`crates/wc3-re/src/terrain.rs:114`) | PORT; classify by `TerrainTile` fields (`walkable`, `blight_priority`, `texts`) + tag prefix; remove BLP-average heuristic or move it to asset load |
| `classify(tag, average, water)` (88) | tag/color classifier | `TerrainArt::tile` + `TerrainTile` | PORT; tag is `TerrainTile.id` from the base, not a debug string |
| `classify_color(average)` (126) | color fallback | none | DELETE with the color fallback once tags come from `TerrainArt` |
| `srgb_to_linear` (150) | color math | none (utility) | KEEP |
| `TerrainVertex` (167), `TerrainMesh` (176) | 4-layer GPU vertex + geometry | none | KEEP (presentation) |
| `TerrainMesh::cell_colors` diagnostic | biome continuity test only | none | KEEP as test-only data; not used in the acceptance path |
| `hash/noise_periodic/fbm_periodic` (188-245) | CPU noise for the removed procedural albedo | none | DELETE (procedural-only albedo is rejected); GPU `hash21/vnoise` stays for water ripples + macro variation only |
| `detail_texture` (247) | neutral detail texture used only by the procedural fallback | none | DELETE with `procedural_tile`; real TerrainArt layers replace it |
| `smoothstep` (274) | util | none | KEEP |
| `Grid` (279) | height/normal/AO/layer build state | `wc3_formats::w3e::Terrain` from `MapBundle.terrain` | KEEP; input from base terrain |
| `build_terrain` (293) | shared 129×129 grid; 4 unique W3E cell layers/corner + slope rock; world-space UV (`x,z`, 1 tile = 128 units); normal/AO; no per-cell origin reset | `MapBundle.terrain` (W3E) + `TerrainArt` tile IDs | PORT: consume `Terrain` + `TerrainArt` from the asset layer; no new W3E reader |
| `build_water` (485) | per-water-cell quad at `water_height`, depth tint, shoreline alpha 0 at depth<3 | `TerrainArt.water` (`Water.slk`) + `TerrainTile` flags | PORT to use base water settings; geometry stays |
| WGSL `Camera/VsIn/VsOut/vs_main/hash21/vnoise/haze/fs_main/fs_water` (541-652) | terrain material: 4-layer texture array sample + macro noise + slope/AO/light + haze; water ripples | none (GPU) | KEEP |
| `TileSet` (655) | image layers + averages | canonical decoded TerrainArt tiles | PORT; replace `Assets.terrain_tile_images` with base `TerrainArt`/`StormFs` decode |
| `resize_rgba` (660) | box resize | `wc3_re::blp` decode output (`DecodedBlp::rgba`, mips) | MOVE to the canonical texture utility or KEEP as a presentation helper over `DecodedBlp` |
| `composite_tile` (715) | average the BLP's 128px variation quadrants over their own average colour, 128×128 opaque tile | `wc3-re::terrain` (or `TerrainTile` texture loader) | MOVE: variant decoding belongs with `TerrainArt`; do not keep a private BLP-sheet decoder in the renderer |
| `linear_from_srgb` (762) | color math | none | KEEP |
| `procedural_tile` (approx. 800) | fallback tile when archive/`TerrainArt` unavailable | none; merge forbids temporary fallbacks | DELETE once the base guarantees a tile for every W3E tag; keep data-error reporting instead |
| `load_tile_set` (approx. 830) | builds layers from `Assets.terrain_tile_images` | `TerrainArt` + `StormFs::read(tile.texture_path())` + `blp::decode_first_mip` | PORT to canonical asset API; delete the `Assets.terrain_tile_images` bridge |
| `upload_tile_array` (approx. 880) | wgpu texture array (128px layers, mips per layer, repeat sampler) | none (GPU) | KEEP |
| `TerrainRenderer::{new,draw_terrain,draw_water}` (993-1175) | pipelines, buffers, array bind group | asset data from base; no reader | PORT the constructor; draw methods KEEP |

## 2. `crates/wc3-game/src/render.rs` (frame orchestration)

| Function (current file:line) | Behavior today | Canonical target | Action / dependency |
|---|---|---|---|
| `UI_SHADER` (13-67) | UI pass | none | KEEP |
| `CameraUniform` (72), `ViewportUniform` (80) | uniforms | none | KEEP |
| `DynamicBuffer::{new,write}` (80-124) | reusable buffers | none | KEEP |
| `RenderOptions` (126) | headless isolation flags (`skip_terrain/units/ui/doodads`) | none | KEEP |
| `Renderer` fields (133) | pipelines, textures, models, terrain, doodad buffers, scratch | asset models/textures from base | KEEP; `models`/`textures` must be filled by the canonical asset layer |
| `Renderer::new` (160) | creates pipelines/textures; **calls `DoodadWorld::load(assets)` which opens a second `AssetDb`**; builds static doodad geometry | `Assets` with canonical `StormFs`/`MapBundle`/`ObjectDb` doodads; no second archive open | PORT; remove `DoodadWorld::load(assets)` side-loading, consume an asset-provided doodad world |
| `Renderer::resize` (482) | depth texture | none | KEEP |
| `Renderer::render` (505) | default frame | none | KEEP |
| `Renderer::render_with` (515) | camera uniform, model batch, UI scratch, world pass order (terrain → doodads opaque → units opaque → water → doodads blended → units blended → doodad decals → rings), UI pass | none | KEEP |
| `Renderer::draw_group/draw_doodads/draw_batch` (688-748) | sorted per-material draws, buffer rebinding | none | KEEP |
| `build_doodad_geometry` (749) | merges all doodad instances into static vertex/index buffers, one draw per (model, mesh, layer) + blob shadows | canonical doodad instances/models | KEEP; input only |
| `upload_texture` (883) | per-texture upload (sRGB models, linear detail, mips) | `DecodedBlp`/mips from base | KEEP as GPU upload; source `DecodedBlp` instead of `BlpImage` if `wc3-assetdb` is removed |
| `render_scene_to_rgba` / `_with` (959/978) | headless readback | none | KEEP (test/acceptance harness) |
| `write_ppm` (1095) | PPM writer | none | KEEP |

## 3. `crates/wc3-game/src/model_render.rs` (MDX models)

| Function (current file:line) | Behavior today | Canonical target | Action / dependency |
|---|---|---|---|
| `MeshVertex` (27), `ModelDraw`/`ModelDraw::{sort_key,sort_key_public}` (35-60) | GPU vertex/draw | none | KEEP |
| `ModelBatch`/`ModelBatch::{clear,push_unit,skin,push_rings,sort_draws}` (68-240) | weighted MDX SKIN CPU skinning, per-layer draws, AO/team tint, Z-up→Y-up | `wc3_re::mdx` model data; `wc3-anim::Animator` world matrices | KEEP (game renderer); consume canonical `MdxModel`/`MeshData` (no format reader here) |
| `team_tint` (246) | saturate team colour | none | KEEP |
| `append_ring` (257) | selection decal | none | KEEP |
| WGSL `Camera/VsIn/VsOut/vs_main/shade/haze/fs_main/fs_cutout` (289-358) | model material | none | KEEP |
| `ModelPipelines::new` (367) | pipelines (opaque/cutout/blend/additive, LEQUAL depth) | none | KEEP |
| `build_scene` (477) | per-unit batch from `Scene` + `assets.models` + model meta (scale/yaw) | asset models from base | PORT input only; keep the skinning |
| `project_model_height` (497) | HUD/acceptance height measurement | `wc3-anim`/model bounds | KEEP |

## 4. `crates/wc3-game/src/doodads.rs` (world doodads) — **the main duplicate-reader offender**

| Function (current file:line) | Behavior today | Canonical target | Action / dependency |
|---|---|---|---|
| `RawDoodad` (25), `DoodadModel` (35), `DoodadInstance` (44), `DoodadWorld` (50), `DoodadStats` (60) | world data model | none | KEEP as data shape; move to base if doodads become an asset-layer product |
| `debug_path` (69) | parses `mpq=`/`map=` from `Assets::debug` strings | `MapBundle.path` + `StormFs`/`Assets` paths | DELETE; pass explicit paths from the asset layer |
| `read_u32`/`read_f32` (76/81) | binary helpers | none | MOVE with `parse_doo` |
| `parse_doo` (89) | private `war3map.doo` reader (W3do v7/8/13) | **base gap**: add `wc3-re` doodad parser (e.g. `wc3_re::doodads::parse(bytes) -> Vec<RawDoodad>` or `MapBundle.doodads`) | MOVE; delete this copy after the base lands. Provenance: HiveWE `src/base/doodads.ixx:56-156` |
| `TypeInfo` (154) | type→model art | `ObjectDb` (`ObjectKind::Doodad`, `ObjectRecord::slk_field`) | PORT |
| `find_col`/`parse_table`/`type_tables` (162-190) | private SLK table loading from `AssetDb` | `wc3_re::slk::Sheet` + `ObjectDb::load_storm` | DELETE; use `ObjectDb` sheets |
| `lookup_type` (191) | row scan for dir/file/numVar/texID/texFile | `ObjectDb::record(id).slk_field("Doodads"/"DestructableData", ...)` | DELETE; base lookup |
| `candidates` (240) | path candidates (`dir\file\fileN.mdx`, …) | `StormFs::exists/resolve` + base doodad-art resolver | MOVE to base or asset layer; no filesystem probing in renderer |
| `load_model` (257) | `AssetDb::model` probing | `StormFs::read` + `wc3_re::mdx::parse` | DELETE/Rewrite; canonical MDX read |
| `DoodadWorld::empty` (267) | empty world | none | KEEP (no-archive map) or remove if base guarantees data |
| `DoodadWorld::load` (278) | opens `MapBundle` + map `AssetDb` for `.doo`, opens War3 `AssetDb`, loads 2 SLK tables, models, textures, builds instances | `MapBundle` (map), base doodad parser, `ObjectDb`, `StormFs`, `blp::decode` | REWRITE as a thin consumer of the base doodad plan; delete the two `AssetDb` opens |

## 5. `crates/wc3-game/src/assets.rs` (terrain/texture parts only)

| Function/field (current file:line) | Behavior today | Canonical target | Action / dependency |
|---|---|---|---|
| `Assets::load` (217) | opens `AssetDb` + `W3Map`; loads 4 unit models; builds terrain atlas + tile biome + `terrain_tile_images`; UI/chrome/minimap | `MapBundle` (merge patch `0001-assets-mapbundle-wts.patch`), `StormFs`, `ObjectDb`, `TerrainArt`, `blp::decode` | PORT (asset-layer owner/merge); renderer must not re-open archives |
| `field terrain_tile_images` (183-194) | raw TerrainArt images for the terrain array (temporary bridge) | base `TerrainArt` decoded tile textures | DELETE after base exposes decoded tiles; currently consumed by `load_tile_set` |
| `field tile_slot`, `atlas_uv`, `terrain_atlas` (183-194) | legacy atlas + per-cell quadrant data | none (acceptance forbids atlas sampling) | DELETE once tests/diagnostics stop using it; minimap uses tiles directly |
| `field tile_biome` (183-194) | biome per ground tag | `TerrainArt::tile` + presentation classifier | PORT; keep as presentation metadata |
| `Assets::{height_at,tile_at,is_water}` (475/503/582) | W3E sampling | `Terrain` from `MapBundle` | KEEP (terrain queries) |
| `AtlasBuilder` (1038) / `push`/`finish` | composites every BLP over one base (old quilt root cause) | none | DELETE; not used by the new terrain |
| `resize_to` (1125) | box resize | `wc3_re::blp` decode/scale utility | MOVE to base texture utility or keep for model texture resize |
| `average_color` (995) | BLP average | `wc3_re::blp`/`DecodedBlp` | MOVE or keep as util |
| `terrain_art_path` (1016) | private `Terrain.slk` lookup | `TerrainArt::tile(id).texture_path()` | DELETE |
| `build_minimap` (1191) | minimap texture | none (UI/world) | KEEP; source tile colours from base tiles |
| `procedural_tiles` (1172), `terrain_tile_count` (1243) | no-archive fallback/diagnostic | none | DELETE/keep test-only per merge decision |
| `load_model_textures` (689), `build_meshes` (730), `build_mesh` (750), `fallback_model` (907), `yaw_offset_for` (932), `cube_mesh` (940) | MDX material/mesh construction for units | `wc3_re::mdx::parse/textures/layers` for parsing; animations owner for visibility | DEPENDENCY: anim/merge owns these; renderer consumes `MeshData` (no duplicate MDX reader in render) |
| `mesh_alpha`, `mesh_layer_alpha`, `mesh_alphas_for`, `geoset_alpha`, `track_active` (527-687) | sequence-scoped GEOA/KGAO visibility (anim fix) | `wc3-re` MDX + anim sequence rules | DEPENDENCY: keep as-is; do not reimplement in render |

## 6. Base gaps that block a clean port (needed from the merge owner)

1. **Doodad instances**: `MapBundle` lists `war3map.doo` but neither exposes its
   bytes nor parses it (`crates/wc3-re/src/mapfiles.rs:32-78`). Need
   `MapBundle.doodads: Option<Vec<RawDoodad>>` or
   `wc3_re::doodads::parse(bytes)` with proven HiveWE layout. This is the only
   format gap that forces a duplicate reader today.
2. **Doodad art resolution**: `ObjectDb` has the `Doodads`/`DestructableData`
   sheets but no `dir/file/numVar/texID/texFile` accessor. Need
   `ObjectDb::doodad_art(id) -> Option<DoodadArt>` (or documented `slk_field`
   keys) so the renderer does not scan sheets.
3. **Runtime TerrainArt**: `TerrainArt` only has `load_dir`/`load_terrain`;
   need `TerrainArt::load_storm(&StormFs)` (Terrain/CliffTypes/Water/Weather)
   and a decoded tile accessor (`TerrainTile` + `StormFs` + `blp::decode` →
   opaque variant tile) so the terrain array consumes the base.
4. **Map inner read**: `MapBundle` exposes only `InnerFile{name,size}`; add
   `read_inner(name)` (or typed fields) for `.doo`/custom inner files.
5. **Assets ↔ base**: `Assets::load` still uses `wc3-assetdb::AssetDb` +
   `W3Map`; the merge patch switches to `MapBundle`, but `Renderer::new` still
   re-opens `AssetDb` for doodads. The base should hand the renderer a single
   `WorldAssets { models, textures, terrain_tiles, doodads }` product.
6. **Canonical decoded texture type**: `wc3-assetdb::BlpImage` (runtime) vs
   `wc3_re::blp::DecodedBlp` (base). Pick one (`DecodedBlp`) and make
   `upload_texture` consume it; delete `BlpImage` constructions in assets.
7. **Terrain cliff meshes**: `TerrainArt.cliffs` (CliffTypes.slk) provides
   cliff/ramp model directories; no base API for instancing them. The renderer
   can only slope-shade cliffs today. Need a base `CliffPlan` or an explicit
   decision that cliffs stay slope-shaded.
8. **`BASE.md` / `WC3RE_MERGE_MATRIX.md`**: not published yet. This map is
   against the current `crates/wc3-re` public API; file:line targets may move.

## 7. Port order (once BASE.md is published)

1. Land the canonical `WorldAssets` product in `assets.rs`/merge: `MapBundle`
   + `StormFs` + `ObjectDb` + `TerrainArt` + decoded `BlpImage`-free tiles +
   unit models + doodad plan.
2. Replace `terrain_render::{load_tile_set, composite_tile, procedural_tile}`
   with the base tile accessor; keep `build_terrain`/`build_water`/GPU.
3. Replace `doodads.rs` readers (`parse_doo`, `type_tables`, `lookup_type`,
   `candidates`, `load_model`, `debug_path`) with the base doodad plan +
   `StormFs`/`mdx`; keep `DoodadWorld` data + `build_doodad_geometry`.
4. Rewrite `Renderer::new` to consume `WorldAssets`; remove the second
   `AssetDb`/`W3Map` opens. Keep the pass order and per-material sorting.
5. Keep `model_render` skinning/draws; switch parsing to `wc3_re::mdx`.
6. Delete the atlas bridge (`AtlasBuilder`, `atlas_uv`, `tile_slot`,
   `terrain_atlas`, `terrain_tile_images`) once tests/diagnostics are ported.
7. Re-run acceptance only after the device/APK owner is ready (one cycle).

## 8. What is safe to keep from the current work

- Shared-grid terrain mesh with 4 W3E layers/corner, world-space UV (1 tile =
  128 units), slope/AO/light, water depth tint/shoreline fade.
- Terrain GPU pipeline: 4-layer `texture_2d_array` + weights, 3-sampler-free
  samples per pixel, no per-cell atlas quadrants.
- Doodad static merge (all instances per model/mesh/layer, blob shadows).
- MDX weighted skinning, sequence-scoped GEOA visibility, team tint, yaw/scale
  meta, per-material draw sorting.
- Headless seam metric + biome/water/unit/doodad regression tests (test-only).

## 9. Waiting state

Paused: no feature work, no tests, no device runs. Next action is the merge
owner's assignment after `BASE.md` / `WC3RE_MERGE_MATRIX.md` (and the base
doodad/terrain APIs in §6) are published. This document is the
"every function that moves" artifact for that handoff.
