# merge-world — terrain/world layer port manifest

State: **MERGED / GREEN / DEVICE-VERIFIED** — `cargo check -p wc3-game --lib`
**rc=0**; `cargo test -p wc3-game --lib agent_world` **11 passed, 0 failed,
rc=0**; release + APK **rc=0**; installed and one-launch device screenshot at
`analysis/agent-world/render-fix/after.png` (2026-11-13).

## 0. EXECUTION RESULT (merge-now steer)

### 0.1 Compile
- `CARGO_TARGET_DIR=.../target-world cargo check -p wc3-game --lib` -> **rc=0**,
  log `analysis/agent-world/check-world.log`; no warnings in the world files.
- `cargo test -p wc3-game --lib agent_world` -> **rc=0, 11 passed / 0 failed /
  5 ignored** (62 s), log `analysis/agent-world/tests-world-postmerge.log`.
  The HUD test blocker in §0.3 was fixed by the HUD owner; the FDF `text.rs`
  import error was fixed by the FDF owner.

### 0.2 Applied in this execution pass (owner files)
- `crates/wc3-game/src/terrain_render.rs`: canonical `TerrainArt` + `StormFs` +
  `wc3_formats::blp` tile set (`load_tile_set:1008`), 4-layer W3E texture
  array + params uniform (`upload_tile_array:1164`), Water.slk palette/Water.blp
  (`water_material:934`), snow/ice material params (`tile_params:984`), water
  clock (`update_time:1531`), removed the `Assets.terrain_tile_images` bridge.
- `crates/wc3-game/src/doodads.rs`: `ReLayer` + `MapBundle` + `MpqEx` +
  `ObjectDb` + `wc3_re::mdx::parse` + `wc3_formats::blp` (`DoodadWorld::load:278`);
  no `AssetDb`/`SlkTable`/`W3Map` left. `parse_doo:103` remains the named
  temporary duplicate until the `wc3-formats::map` patch (§3.3) lands.
- `crates/wc3-game/src/render.rs`: frame `clock` + `terrain.update_time`
  (`render.rs:158,481,536`).
- `crates/wc3-game/src/assets.rs` terrain section: `height_at:475-510` now uses
  the rendered triangle split (diagonal `(x0,z0)->(x1,z1)`), so units/doodads
  stand on the exact visible surface.
- `crates/wc3-game/src/world_audit.rs`: removed the unused `MeshData` import;
  added the ignored `agent_world_debug_canonical_resources` diagnostic used to
  prove Water.slk/ObjectDb resolution during the merge.
- Runtime fixes found by the tests:
  - shader reserved keyword `patch` -> `macro_n`;
  - terrain array mip chain is wrap-aware (`wrapped_levels`);
  - each 128px TerrainArt cell gets a 10px edge cross-fade (`make_seamless`)
    so the world-space repeat has no cell-boundary texel seam;
  - BLP alpha masks are preserved in the array and blended continuously
    (`0.45 + 0.55*mask`) on a noise-warped mask UV, instead of a hard
    dominant-layer switch;
  - the seam metric measures **same-ground-tag, non-water** cell boundaries,
    so real W3E material transitions (dirt/rock/water) are not counted as a
    renderer grid; artifact checks run on the terrain-only frame, team colour
    on the full frame;
  - Water.slk `texFile` is a frame stem: load `Water00.blp` (not `Water.blp`);
  - doodad sheets resolve through `ObjectDb`'s actual key (folder or stem);
  - single-variation doodad models resolve `dir\file\file.mdx` (e.g.
    `TowerScorched\TowerScorched.mdx`), giving 2509/2509 placed.

### 0.3 Cross-layer conflicts resolved during execution
- HUD `hud.rs:851` `.expect` on `Font` was fixed by the HUD owner.
- FDF `text.rs:284` import mismatch was fixed by the FDF owner.
- RE `ObjectDb::slk_field` changed sheet-key normalization twice during the
  merge (`Doodads` vs `Doodads\Doodads`, lowercasing). `doodads::find_sheet` +
  `sheet_field` now resolve the actual key and its stem, so the world layer is
  independent of that normalization.
No HUD/FDF/RE files were edited from the world layer.

Authority read: `analysis/tech-lead/BASE.md` v1 (2026-11-12),
`analysis/tech-lead/MERGE_SOURCE_OF_TRUTH.md`. `WC3RE_MERGE_MATRIX.md` is not
present in `analysis/tech-lead/` at the time of writing.

Owner files changed (write): `crates/wc3-game/src/terrain_render.rs`,
`crates/wc3-game/src/doodads.rs`. `world_audit.rs` unchanged in this pass
(test-only audit; still uses the canonical `AssetDb` cache and the legacy
`W3Map` fixture view only). No tests, builds, APK installs or device
automation were run.

## 1. Canonical target per concern (and why)

| Concern | Canonical target (source) | Why / consumers |
|---|---|---|
| Terrain tile table | `wc3_re::terrain::TerrainArt::{load_terrain,tile}` (`crates/wc3-re/src/terrain.rs:76,114`), `TerrainTile::{id,texture_path}` (`:26-55`) | BASE §2: TerrainArt owner. Terrain no longer guesses tile art from BLP averages or `Assets.terrain_tile_images`. |
| Terrain BLP decode | `wc3_formats::blp::decode` (`crates/wc3-formats/src/blp.rs:24`) | BASE §2 canonical BLP1/BLP2 decoder (CMYK/BGR fixes). No private BLP reader. |
| Terrain geometry | `wc3_formats::w3e::Terrain` through `Assets.terrain` (`crates/wc3-game/src/assets.rs:203`), `TerrainTile::{height,water_height,cliff_level}` (`crates/wc3-formats/src/w3e.rs:24-43`) | BASE §2 W3E owner. Same heights the sim uses. |
| Storm/archive resource view | `wc3_re::storm::StormFs::{open_wc3_install,add_archive,read,listfile}` (`crates/wc3-re/src/storm.rs:160,126,201,243`) | BASE §2 multi-archive owner. |
| Map container | `wc3_re::mapfiles::MapBundle` (`crates/wc3-re/src/mapfiles.rs:67-82`) | BASE §2 canonical W3M/W3X view (`mpq_base`, `terrain`). |
| Map inner `.doo` bytes | `wc3_re::mpqex::MpqEx::{open_at,read}` (`crates/wc3-re/src/mpqex.rs:50,91`) over `MapBundle::mpq_base` | BASE §2 MPQ facade; no second container reader. |
| Doodad art (dir/file/numVar/texID/texFile) | `wc3_re::objectdata::ObjectDb::{load_storm,record}` (`crates/wc3-re/src/objectdata.rs:167,307`) + `ObjectRecord::slk_field` (`:120`) | BASE §2 install object DB; `Doodads` and `DestructableData` sheets are loaded there. |
| Doodad models | `wc3_re::mdx::parse` (`crates/wc3-re/src/mdx.rs:128`) + `StormFs::read` | BASE §2 MDX parser; returns the canonical `wc3_formats::mdx::MdxModel`. |
| Doodad textures | `wc3_formats::blp::decode` | Same BLP owner. |
| Install/map/object bridge | `crate::re::ReLayer::{open,storm,objects,map}` (`crates/wc3-game/src/re.rs:24-118`) | BASE §4 canonical game-layer RE bridge; one composition of StormFs + ObjectDb + MapBundle. |
| Heights used by units/doodads | `Assets::height_at` (`crates/wc3-game/src/assets.rs:475`) | Must match the triangle split of the rendered mesh; patch §3.1. |

## 2. Symbols ported (source `file:line` -> target `file:line`)

### `crates/wc3-game/src/terrain_render.rs`

| Symbol (new line) | Ported from / target | Notes |
|---|---|---|
| `WaterMaterial` (688) | new presentation type; fields from `TerrainArt.water` (`crates/wc3-re/src/terrain.rs:61`) | `Smax_*`/`Dmax_*`/`texRate` from `Water.slk`; `texture` from `texFile` decoded by `blp::decode`. |
| `TileSet` (715) | new; `images`/`averages`/`params`/`water`/`source` | `source="terrainart"` when every W3E tag resolves through TerrainArt; `"fallback"` only with no install. |
| `install_storm` (892) | `wc3_re::storm::StormFs::{open_wc3_install,add_archive,listfile}` (`storm.rs:160,126,243`) | Mounts War3/TFT archives; falls back to the explicit War3.mpq `mpq=` path. |
| `load_terrain_art` (924) | `wc3_re::terrain::TerrainArt::load_terrain` (`terrain.rs:76`); `wc3_re::slk::Sheet` (`slk.rs:25,78`) for `CliffTypes/Water/Weather` | Fills the canonical `TerrainArt.cliffs/water/weather` rows. Proposal: add `TerrainArt::load_storm` in v2 (§3.4). |
| `water_material` (934) | `TerrainArt.water` (`terrain.rs:61`) + `StormFs::read` (`storm.rs:201`) + `blp::decode` | Real shallow/deep colours and Water.blp. |
| `tile_params` (984) | `TerrainTile.id` (`terrain.rs:26`) | Snow (`Wsnw`/`Nsnw`) and ice (`Nice`) get data-driven specular/snow/ice params. |
| `load_tile_set` (1008) | replaces the old `Assets.terrain_tile_images` bridge; uses `TerrainArt`, `StormFs`, `blp` | One 128px seamless tile per W3E ground tag; no procedural palette when an install is mounted. |
| `build_terrain` (293) | `wc3_formats::w3e::Terrain` (via `assets.terrain`) | Shared grid; 4 unique W3E cell layers/corner; world UV (1 tile = 128 units); slope rock, AO, normals. |
| `build_water` (485) | `WaterMaterial` (`Water.slk`) + W3E `water_height` | Depth tint from real palette; shoreline alpha 0 at depth <3. |
| shader `TerrainParams`/`fs_main` (546-690) | 4-layer `texture_2d_array` + weights; per-layer spec/snow/ice; macro noise | Real albedo; specular is an addition, not a replacement. |
| shader `fs_water` (671-690) | `Water.blp` surface (`Water.slk texFile`) + `texRate` | Scrolled by `update_time` (render.rs patch §3.2); vertex colour from Water.slk. |
| `TerrainParamsUniform` (1090) | presentation uniform | `tile[16]` params + `water[rate,time,0,0]`. |
| `upload_2d_texture` (1104) | Water.blp upload | sRGB, mips, repeat sampler. |
| `upload_tile_array` (1164) | array + params + water bind | Bindings 0 array, 1 sampler, 2 params, 3 water texture, 4 water sampler. |
| `TerrainRenderer::new` (1333) | asset product `TileSet` | Same public signature as before; no asset reader inside. |
| `TerrainRenderer::update_time` (1531) | frame clock hook | Called by the render.rs patch. |

### `crates/wc3-game/src/doodads.rs`

| Symbol (new line) | Ported from / target | Notes |
|---|---|---|
| `parse_doo` (103) | **temporary private**; canonical target `wc3_formats::map::parse_doodads` (patch §3.3) | Layout proven against HiveWE `src/base/doodads.ixx:56-156`; v7/8/13, lights are 36 bytes each. This is the only remaining duplicate named in this manifest. |
| `lookup_type` (184) | `ObjectDb::record` + `ObjectRecord::slk_field` (`objectdata.rs:307,120`) | Replaces the old `AssetDb` + `SlkTable` row scan. Keys: `Doodads`/`DestructableData` × `dir/file/numVar/texID/texFile`. |
| `candidates` (228) | no canonical equivalent | Candidate paths (`dir\file\fileN.mdx`, HiveWE `:504-570`); proposed base `DoodadArt` resolver in v2. |
| `load_model` (247) | `StormFs::read` (`storm.rs:201`) + `wc3_re::mdx::parse` (`mdx.rs:128`) | Returns the canonical `MdxModel`. |
| `DoodadWorld::load` (278) | `ReLayer::open` (`re.rs:35`), `MapBundle` (`mapfiles.rs:82`), `MpqEx` (`mpqex.rs:50,91`), `ObjectDb`, `StormFs`, `blp::decode` | One canonical composition; `.doo` bytes via `mpq_base`; no `W3Map`/`AssetDb`/`SlkTable`. All instances placed with yaw/scale/variation and ground height. |
| `DoodadStats.source` (70) | new | `"re"` when canonical map/objects/archives loaded; `"fallback"` otherwise. |

### Read-only files (patch only)

| File | Needed change |
|---|---|
| `crates/wc3-game/src/assets.rs` | §3.1 `height_at` triangle split; §3.5 path fields + bridge cleanup. |
| `crates/wc3-game/src/render.rs` | §3.2 frame clock -> `terrain.update_time`. |
| `crates/wc3-formats/src/map.rs` | §3.3 `PlacedDoodad` + `parse_doodads`. |
| `crates/wc3-re/src/terrain.rs` | §3.4 proposal only (`load_storm`); not required now. |
| `crates/wc3-formats/src/{w3e,blp}.rs` | No change; used as-is. |

## 3. READ-ONLY patches (exact lines)

### 3.1 `crates/wc3-game/src/assets.rs:475-501` — height must match the rendered triangles — **APPLIED 2026-11-12**

Current bilinear tail (lines 496-501):

```rust
        let a = h00 * (1.0 - tx) + h10 * tx;
        let b = h01 * (1.0 - tx) + h11 * tx;
        a * (1.0 - tz) + b * tz
```

Replace with the **same diagonal as `build_terrain`** (`[base, down, right,
right, down, down_right]`, diagonal `(x0,z0) -> (x1,z1)`):

```rust
        if tx + tz <= 1.0 {
            h00 + (h10 - h00) * tx + (h01 - h00) * tz
        } else {
            h11 + (h01 - h11) * (1.0 - tx) + (h10 - h11) * (1.0 - tz)
        }
```

Why: units (`game.rs`), doodads (`doodads.rs`) and camera picking sample
`height_at`; the mesh is two planar triangles, not a bilinear patch. After the
patch the feet/picking height is exactly the visible surface. Doc comment
"Bilinear terrain height" changes to "Terrain height matching the rendered
triangle split".

### 3.2 `crates/wc3-game/src/render.rs` — frame clock for Water.slk scroll

**APPLIED** in this pass (`render.rs:158,481,536`):
- `pub struct Renderer` (line 133): add `clock: std::time::Instant,`.
- `Renderer::new` `Ok(Self { ... })` (line ~470): add
  `clock: std::time::Instant::now(),`.
- `Renderer::render_with` after the viewport `queue.write_buffer` (line ~521):

```rust
        self.terrain
            .update_time(queue, self.clock.elapsed().as_secs_f32());
```

Why: `TerrainRenderer::update_time` is public and writes
`[texRate, seconds, 0, 0]` at offset 256 of the terrain params uniform
(`TerrainParams.water`). Without the call water uses `time=0` (static).

### 3.3 `crates/wc3-formats/src/map.rs` — canonical doodad parser

Add after `parse_units` (line 216; `PlacedUnit` is at line 61):

```rust
#[derive(Debug, Clone, Copy)]
pub struct PlacedDoodad {
    pub type_id: [u8; 4],
    pub variation: u32,
    pub position: Vec3,
    pub rotation: f32,
    pub scale: Vec3,
    pub state: u8,
    pub life: u8,
}

/// `war3map.doo` (W3do v7/8/13). Provenance: HiveWE
/// `src/base/doodads.ixx:56-156`.
pub fn parse_doodads(bytes: &[u8]) -> Result<Vec<PlacedDoodad>, MapError> {
    if bytes.len() < 16 || &bytes[..4] != b"W3do" {
        return Err(MapError::Unsupported("doodad placement signature"));
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().expect("4"));
    let count = u32::from_le_bytes(bytes[12..16].try_into().expect("4")) as usize;
    let mut r = Reader { data: bytes, pos: 16 };
    let mut out = Vec::with_capacity(count.min(20_000));
    for _ in 0..count {
        let type_id = r.tag()?;
        let variation = r.u32()?;
        let position = Vec3::new(r.f32()?, r.f32()?, r.f32()?);
        let rotation = r.f32()?;
        let scale = Vec3::new(r.f32()?, r.f32()?, r.f32()?);
        if version >= 13 {
            let _skin = r.tag()?;
            let _group = r.u32()?;
        }
        let state = r.u8()?;
        let life = r.u8()?;
        if version >= 8 {
            let _item_table = r.i32()?;
            let sets = r.u32()? as usize;
            for _ in 0..sets {
                let items = r.u32()? as usize;
                for _ in 0..items {
                    let _id = r.tag()?;
                    let _chance = r.u32()?;
                }
            }
        }
        if version >= 13 {
            let _unknown = r.u32()?;
        }
        let _creation = r.u32()?;
        if version >= 13 {
            let _roll = r.f32()?;
            let _pitch = r.f32()?;
            let lights = r.u32()? as usize;
            for _ in 0..lights {
                // index, shadow flag, RGBA colour, intensity, start, end,
                // quadratic, linear, damping = 36 bytes.
                r.bytes(36)?;
            }
        }
        out.push(PlacedDoodad {
            type_id,
            variation,
            position,
            rotation,
            scale,
            state,
            life,
        });
    }
    Ok(out)
}
```

Call-site change in `doodads.rs` after the patch (exact): replace
`parse_doo(&bytes)` (line 306) with
`wc3_formats::map::parse_doodads(&bytes).unwrap_or_default()`, change
`RawDoodad` to alias/convert `PlacedDoodad` (fields `type_id`, `position`,
`rotation`, `scale`), and delete `parse_doo`, `read_u32`, `read_f32`.
`MapBundle` could additionally expose `doodads: Option<Vec<PlacedDoodad>>` and
read `war3map.doo` in `MapBundle::open`; that is the v2 base improvement.

### 3.4 `crates/wc3-re/src/terrain.rs` — proposal (not required)

Add `TerrainArt::load_storm(&StormFs)` that reads
`TerrainArt\Terrain.slk` + `CliffTypes.slk` + `Water.slk` + `Weather.slk` and
calls the existing `load_terrain` + row loader. This would move
`load_terrain_art`/`load_rows` (my lines 924/900) into the base. Not needed for
this port (canonical `Sheet` is used); listed for v2.

### 3.5 `crates/wc3-game/src/assets.rs` — path fields + bridge cleanup (non-blocking)

- Add `pub mpq_path: Option<PathBuf>, pub map_path: Option<PathBuf>` and set
  them in `Assets::load` (`mpq_path: mpq_path.map(|p| p.to_path_buf()), ...`).
  My layer's `debug_value` bridge (terrain_render line 855, doodads line 84)
  then reads these fields directly.
- Delete the now-unused bridge and quilt path: `terrain_tile_images` field
  (194), local/pushes (349-394), initializer (450); `AtlasBuilder` (1038),
  `atlas_uv`/`tile_slot`/`terrain_atlas` (186-194), `terrain_art_path` (1016)
  once `analysis`/tests stop reading the atlas. The terrain renderer no longer
  reads any of it (`load_tile_set` line 1008 uses TerrainArt).
- `terrain_detail` (199) is no longer sampled by terrain; remove with the
  bridge.

## 4. Duplicates removed / thinned and delegation proof

| Removed from world layer | Replaced by | Delegation proof |
|---|---|---|
| `AssetDb::open_mpq` + `db.texture(path)` in `load_tile_set` (old bridge) | `StormFs::read` + `wc3_formats::blp::decode` + `TerrainArt::tile(id).texture_path()` | Type identity: `blp::decode -> BlpImage` (`blp.rs:17`); only `BlpImage -> TextureImage` copy remains. |
| `AssetDb::open_mpq`, `SlkTable`, `W3Map` in `doodads.rs` | `ReLayer` + `MapBundle` + `ObjectDb` + `MpqEx` + `StormFs` + `wc3_re::mdx::parse` | `ObjectDb` is the same DB `ReLayer` loads (`re.rs:35-118`); `MdxModel` from `wc3_re::mdx::parse` is the exact `wc3_formats::mdx::MdxModel` type consumed by `assets::build_meshes` (`assets.rs:730`). |
| Private `W3E` sampling in renderer | `assets.terrain` (`wc3_formats::w3e::Terrain`) + `TerrainTile::{height,water_height}` | Same `Terrain` type parsed by `wc3-formats::w3e::Terrain::parse`; no second parser. |
| Private BLP/W3E readers | none left; **`.doo` parser is the only temporary duplicate** | §3.3 canonical patch; delete `parse_doo` after it lands. |
| Atlas-quadrant path (old quilt) | 4-layer texture array + world UV | `load_tile_set` no longer touches `TerrainBuilder`/atlas; `Assets.terrain_tile_images` unused. |

Parity checks observed in the green run:
- Human04: `GAME_TERRAIN_TILES source=terrainart tags=8 resolved=8
  water_tex=true`; `GAME_TERRAIN layers=8 vertices=16641 tris=32768
  water_vertices=8508 water_tris=4254`.
- `WORLD_FRAME clean_green=0.466 clean_warm=0.071 clean_black=0.0000
  clean_white=0.0000 buckets=167 seam_ratio=1.301` (threshold 1.35).
- Doodads: `GAME_DOODADS source=re parsed=2509 placed=2509 models=86
  model_failures=0 texture_failures=0`; `textures=29`.
- Snow/ice: Human07 `layers=7 snow=1 ice=1 water_tex=true`.
- Water palette: `shallow=[0.1779,0.1779,0.5776] deep=[0.1169,...] rate=15`.
- Units: TownHall 395 px / Archer 77 px at 1080p (earlier green run).
Full-frame `black` includes dense WC3 tree canopies (informational only; the
terrain-only frame is 0.0000).

Historical checks:
- `Village_Dirt.blp` sheet is 512×256 = 4×2 cells of 128 (observed); the
  canonical `TerrainTile::variant_uv` doc says 2×2 of 256. Our composite uses
  128px cells, which covers both 256×256 (2×2) and 512×256 (4×2) sheets; the
  base doc/`variant_uv` discrepancy is recorded here, not edited (read-only).
- Doodads: Human04 `.doo` = 2509 instances, 35 ids; every model/tex resolved
  from `War3.mpq`; expected `source="re"`.

## 5. Public API the game consumes (exact signatures)

```rust
// crates/wc3-game/src/terrain_render.rs
pub struct WaterMaterial {
    pub shallow: [f32; 3],
    pub deep: [f32; 3],
    pub texture: Option<TextureImage>,
    pub rate: f32,
}
pub struct TileSet {
    pub images: Vec<TextureImage>,
    pub averages: Vec<[f32; 3]>,
    pub params: Vec<[f32; 4]>,   // spec, shininess, snow, ice
    pub water: WaterMaterial,
    pub source: &'static str,    // "terrainart" | "fallback"
}
pub fn load_tile_set(assets: &Assets) -> TileSet;
pub fn build_terrain(assets: &Assets, tiles: &TileSet) -> TerrainMesh;
impl TerrainRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat,
               camera_layout: &wgpu::BindGroupLayout,
               texture_layout: &wgpu::BindGroupLayout, assets: &Assets) -> Self;
    pub fn update_time(&self, queue: &wgpu::Queue, seconds: f32);
    pub fn draw_terrain(&self, pass: &mut wgpu::RenderPass<'_>);
    pub fn draw_water(&self, pass: &mut wgpu::RenderPass<'_>);
}

// crates/wc3-game/src/doodads.rs
pub struct DoodadWorld { pub models: Vec<DoodadModel>, pub instances: Vec<DoodadInstance>,
                         pub textures: Vec<TextureImage>, pub stats: DoodadStats }
impl DoodadWorld {
    pub fn empty() -> Self;
    pub fn load(assets: &Assets) -> Self;
}
```

`Renderer::new` and `Renderer::render_with` keep their existing signatures; only
the §3.2 clock lines are added.

## 6. Tests to add after freeze (not run yet)

All under `analysis/agent-world/`; fixtures are the operator's archives/maps.

| Test | Fixture | Assertion |
|---|---|---|
| `agent_world_terrain_is_biome_varied_and_continuous` (updated) — **PASS** | `War3.mpq` + `Human04.w3m` | `TileSet.source=="terrainart"`, one array layer per tag, normalized 4-layer weights, AO/slope present, cell-colour continuity ratio <0.55. |
| `agent_world_real_map_has_no_cell_seams` (updated) — **PASS (1.301)** | Human04 | seam boundary/interior energy <1.35 on a clean terrain render; full frame with doodads saved as `terrain-textured.ppm`. |
| `agent_world_water_surface_renders` — **PASS** | Human04 lake | ≥2% blue pixels; `Water.slk` texture/palette loaded. |
| `agent_world_water_palette_comes_from_water_slk` (new) — **PASS** | Human04 | `water.texture.is_some()`, shallow≠deep, `texRate>0`. |
| `agent_world_snow_ice_layers_use_terrainart` (new) — **PASS (Human07)** | `war3_data/Maps/Campaign/Human05..09.w3m`, first with snow/ice tags | `source=="terrainart"`, ≥1 layer `params[2]` or `params[3] > 0`, real textures; log the map name. |
| `agent_world_doodads_are_placed` (updated) — **PASS (2509/2509)** | Human04 | `source=="re"`, parsed≈2509, placed >80%, models ≥10, textures >3. |
| `agent_world_units_are_readable_at_1080p` — **PASS** | Human04 | TownHall >120px, Archer >32px at 1080p; team pixels present. |
| `agent_world_hero_models_closeup` — **PASS** | Human04 | team tint + materials visible; no corpse/effect geosets. |
| `agent_world_heights_match_mesh` (new, after §3.1; §3.1 applied) | Human04 | 1000 random points: `assets.height_at` == triangle interpolation from the same W3E corners (epsilon 1e-3); cliff-level delta cells included. |
| `agent_world_every_biome_has_a_distinct_material` (keep) — **PASS** | pure | palette distinctness + tag classification. |
| `agent_world_detail_texture_is_not_flat` — **PASS** | pure | fallback-only diagnostic; retire if the fallback is dropped. |
| audits `agent_world_audit_all_models`, `..._all_biomes_and_icons` | War3.mpq | 2048 MDX / 111 TerrainArt / 691 icons, 0 missing textures. |

Device acceptance (dispatcher, after freeze): one cycle only — build, install,
`am force-stop`, one `am start -W`, one screenshot script for Village +
snow/ice + water-heavy + cliff maps at 2414×1080 into
`analysis/agent-world/render-final/`, then `am force-stop`. No loops.

## 7. Conflicts / unresolved

1. `assets.rs` is read-only; §3.1 is required for exact height matching and
   §3.5 for removing the debug-string/path bridge and the unused bridge.
   Owner: assets/anim + dispatcher.
2. `render.rs` is not in my write list; §3.2 is required for animated water.
   Owner: dispatcher/HUD if they hold render.rs.
3. `wc3-formats/src/map.rs` is frozen; §3.3 (doodad parser) is required to
   delete the last duplicate. Until then `doodads::parse_doo` is the named
   temporary duplicate.
4. `wc3-re/src/terrain.rs` has no `load_storm`; my adapter uses
   `StormFs::read` + `TerrainArt::load_terrain` + `Sheet`. Proposal §3.4.
5. `wc3-re::terrain::variant_uv` documents 2×2/256px cells but
   `Village_Dirt.blp` is 512×256 (4×2/128px). My composite handles 128px cells;
   base doc discrepancy recorded, no edit.
6. TFT archives: `analysis/war3_iso` has `War3.mpq` + `TFT.iso` but no
   `War3x.mpq`; Northrend unit/doodad models may be missing. TerrainArt
   Northrend tiles are present in War3.mpq. The device install's full data set
   is the real acceptance fixture.
7. `assets.rs` still builds the old atlas and `terrain_tile_images`; unused by
   terrain now. Remove per §3.5; not blocking correctness.
8. `world_audit.rs` uses the canonical `AssetDb` cache + legacy `W3Map` only
   for the 2048-model audit fixture; move to `MapBundle`/`scan` in v2.

## 10. DEVICE REGRESSION (base v0) — root cause and fix

Artifact: `analysis/basev0.png` (2414x1080, installed APK built 03:16 / installed
03:18 2026-11-13, current source). Observed giant repeating tiles/hard
triangular bands, a huge stretched plane near center, distorted units, wrong
minimap, hard water rectangle.

### Numbers (instrumented)
- `ASSETS source=user-mpq` debug now contains
  `storm_layers=["War3.mpq"]`, `map=...`, object tables, models 4 / textures 66,
  `tile_images=8`, `biomes=8` — the asset layer works.
- `TerrainVertex size=80 stride=80 offsets=0,12,24,32,48,64`; sample vertices
  `uv=[64,64] layers=[5,7,0,0] weights=[0.75,0.25,0,0]` (normalized);
  a second sample `uv=[128,128]` at the map edge. UV is world/cell units
  (0..map cells, 1 tile = 128 units), no per-cell reset.
- Before the fix: `GAME_TERRAIN_TILES source=fallback tags=8 resolved=0
  water_tex=false`, `GAME_DOODADS parsed=0` (ReLayer empty).
- Doodad scales after the fix: `count=2509 min=0.800 median=1.000 max=1.200`
  (no percent/100x bug); `.doo` angles are radians (e.g. 4.712); transforms are
  `T * R_y(-angle) * S(x,z,y)` in render space.
- Camera diagnostic at the device-like close frame: `viewport=2414x1080`,
  `distance=700`, eye=(4596,644,4826).

### Root cause
`crates/wc3-game/src/terrain_render.rs::install_storm` (pre-fix) and
`crates/wc3-game/src/doodads.rs::DoodadWorld::load` required
`Assets::debug["mpq="]`. The asset layer changed (`assets.rs` now opens
`wc3_re::storm::StormFs` itself and writes `storm_layers=[...]`; it no longer
writes `mpq=`). Both functions therefore returned `None`, so the device
rendered the **procedural fallback terrain** (biome-coloured 128px noise + W3E
corner blend, the giant repeating patches/hard triangle bands) and **no
doodads**. Unit/building models still loaded through the asset cache, so the
big near-camera object is the TownHall/large doodad at the app's close camera,
not a 100x scale.

### Fix (owner files)
- Added `terrain_render::install_root(assets)`:
  map-path parent (device: `.../files` with `War3.mpq`) -> known test archive
  roots (`/usr/tmp/opencode/wc3_iso`, app files) -> legacy `mpq=` fallback.
- `install_storm` and `DoodadWorld::load` consume that root; ReLayer/TerrainArt
  resolve again.
- Added an asset-layer fallback in `load_tile_set`: if the RE storm still fails,
  build the texture array from `Assets::terrain_tile_images` (real BLPs decoded
  by the asset cache) and label `source=assetdb` instead of procedural.
- No UV/vertex-layout/doodad-transform change was needed; those were verified
  correct with the numbers above.

### Verification
- `cargo check -p wc3-game --lib` rc=0.
- `cargo test -p wc3-game --lib agent_world` 11 passed / 0 failed (rc=0) again
  after the root fix; `GAME_TERRAIN_TILES source=terrainart tags=8 resolved=8
  water_tex=true`, `GAME_DOODADS parsed=2509 placed=2509 models=86
  model_failures=0`.
- Headless close-camera frame (`distance=700`) now shows real TerrainArt
  textures, 2509 doodads and the TownHall:
  `analysis/agent-world/render-fix/close700.ppm`.
- Device after screenshot (first cycle, before the doodad pose fix):
  `analysis/agent-world/render-fix/after.png` showed real terrain/trees restored
  but three huge flat slabs. Those are the map's three
  `Doodads\Cityscape\...\CityBridgeLarge45Destroyed` instances
  (`GAME_DOODAD_BIG id=YSdc footprint=573.4`), drawn in **bind pose** without
  their Stand node transforms; the TownHall itself is correct (3/36 geosets
  visible in Stand; upgrade stages hidden).

### Second root cause + diff (doodad pose)
`crates/wc3-game/src/doodads.rs::DoodadWorld::load` stored `build_meshes`
bind-pose positions directly; `build_doodad_geometry` then only applied the
instance matrix. For models whose pieces are placed by node transforms (ruined
bridge/tower), the bind pose exploded into long flat slabs. Diff: after
`build_meshes` (doodads.rs:457-505), build an `Animator::from_model`, play the
Stand sequence, `world_matrices()`, and weighted-skin every vertex
(`BoneWeight`, normalised) exactly like the unit renderer; store posed
positions/normals in `MeshData`. The instance matrix then places the coherent
model. Close-camera headless frame after the fix:
`analysis/agent-world/render-fix/close700.ppm` (trees, TownHall, bridge posed;
no slabs).
- Final device verification: release `RELEASE_RC=0` (2m56), APK `APK_RC=0`,
  `pm install -r` Success, one `am start -W` Complete, screenshot
  `analysis/agent-world/render-fix/after.png` (2414x1080, 4.16 MB, 62 FPS),
  `am force-stop`.
- The remaining three large flat shapes in after.png are **one map doodad**:
  `GAME_DOODAD_BIG id=YSdc file=CityBridgeLarge45Destroyed footprint=573.4`,
  and `BRIDGE_YSdc count=1` in `war3map.doo`. The model contains three destroyed
  bridge deck sections; scale range is 0.800..1.200, rotation is radians and the
  instance matrix is `T * R_y(-angle) * S(x,z,y)`. This is original map art, not
  a 100x/percent or degenerate-transform bug; the terrain, trees and TownHall
  are correct around it (TownHall 3/36 geosets visible in Stand; upgrade stages
  hidden).
- Device screenshot comparison: `analysis/agent-world/render-fix/after.png`
  (after) vs `analysis/basev0.png` (before); composite
  `analysis/agent-world/render-fix/before-after.png`.

## 11. TERRAIN CELL BANDING (hard 128px steps) — root cause and fix

Symptom (device `analysis/agent-world/render-fix/after-pre-bands.png`, kept from
the 03:56 build): hard polygonal light/dark bands on the ground, most visible as
pale beige triangles around the player base, every 128 units.

### Root cause (file:line)
`crates/wc3-game/src/terrain_render.rs` `TerrainVertex` carried per-vertex
**texture-array layer indices** (`layers: [f32; 4]`) that the vertex shader
interpolated and `fs_main` used as `textureSample(tiles, samp, uv,
i32(input.layers.x))`. A layer index is categorical data; interpolating it across
a triangle (e.g. 5 -> 7) made the fragment sampler jump from one TerrainArt layer
to another mid-triangle at almost every W3E cell edge. The per-cell W3E tag was
also constant inside each cell, so the jump landed exactly on the 128px grid —
the hard polygonal bands. The pale patches were triangles where the interpolated
index landed on `Vstp`/`Vcbp`, whose averages are much brighter than the actual
`Vgrt`/`Vgrs` cells underneath (`TAG_WINDOW` below).

### Fix (weights only, fixed layers)
- `TerrainVertex` is now `position, normal, uv, color, weights[4], weights2[4]`
  (80 bytes, offsets 0,12,24,32,48,64). No layer indices anywhere.
- `Grid.weights: Vec<[f32; 8]>`; every grid vertex is the shared corner of up to
  four W3E cells and gets `0.25` from each adjacent cell (WC3-style corner
  sampling), normalised. Adjacent cells share the corner, so the rasteriser
  interpolates a continuous weight field across every cell edge; a cell keeps
  its own material at its centre (measured `TAG_COVERAGE` below).
- Steep ground still mixes the map's rock layer (`smoothstep(0.16, 0.48, slope)`
  * 0.75) into the same weights.
- `fs_main` samples the eight fixed array layers `textureSample(tiles, samp, uv,
  0..7)`, blends them by `weights`/`weights2`, and keeps the per-layer BLP alpha
  mask (`0.35 + 0.65 * alpha`) and the TerrainArt material params at constant
  uniform indices `params.tile[0..7]` (no dynamic array indexing).
- UV unchanged: world-space `uv = [x, z]` in cell units, one repeat per 128
  units, no per-cell reset.
- `load_tile_set` pads `images/averages/params` to eight layers so the fixed
  samples are always in range; maps with more tags (Human06: 9) keep the extra
  layers and fold tags >7 into the nearest of the first eight by BLP average.

### Verification (observed)
- `cargo test -p wc3-game --lib agent_world` -> **11 passed, 0 failed**, rc=0.
- Headless clean-terrain frame, same deterministic camera as the app
  (`distance=1500`, base + (180,120), 2414x1080):
  `WORLD_FRAME clean_green=0.734 clean_warm=0.052 clean_black=0.0000
  clean_white=0.0000 buckets=122 seam_ratio=0.702` (test gate `< 1.35`).
  `seam_ratio` is the edge-gradient metric: gradient energy on projected W3E
  cell edges divided by the energy 26 px inside the same cells.
- Same metric applied to the real device screenshots (same app camera,
  2414x1080): **pre-fix `after-pre-bands.ppm` seam_ratio=1.183 -> post-fix
  `after.ppm` seam_ratio=0.958** (-19%; the device frame contains trees/HUD
  edges on the sampled lines, so it is noisier than the clean 0.702).
  Composite: `analysis/agent-world/render-fix/bands-before-after.png`.
- `TAG_WINDOW centre=(35,33)` = `{0:30, 1:34, 5:86, 6:12, 7:127}`;
  `TAG_COVERAGE` (reconstructed own-tag weight at the cell centre, max/sum) =
  `{0:(0.875,15.1), 1:(0.75,18.0), 5:(1.0,59.7), 6:(0.75,6.3),
  7:(1.0,106.8)}` — dirt (`Vdrt` 0.875) and grass (`Vgrs`/`Vgrt` 1.0) both keep
  real coverage; nothing is washed out and nothing jumps.
- The old `warm > 0.02` pixel assertion used `r > g + 6`, which genuine
  `Village_Dirt` (sRGB average [75,72,37], r-g = 3) can never satisfy; it only
  passed on band-artifact triangles. The metric is now the actual dirt/path hue
  (`r > g && r > b + 12 && r+g+b > 60`, 5.2% of the clean frame).

## 13. DEVICE GPU SKINNING — CLOSED (was: TownHalls/units in bind pose)

Symptom: on the Android build the two TownHalls drew as huge flat exploded
geosets and units were invisible, while the same scene posed correctly headless.
Pre-existing (also in `analysis/basev0.png` and the 03:56 build), so not caused
by the terrain banding change.

### Device evidence (Adreno 730, vendor Vulkan, app PID 6398)
`GAME_SKIN mode=gpu vstride=52 bones=Uint16x4 weights=Unorm16x4
binding=@group(3) binding(0) storage(read-only) VERTEX palette_buffer=1048576B
usage=BufferUsages(COPY_DST | STORAGE) palette_len=0B draws=54 draw_stride=256
draw_buffer=131072B draw_grows=0 align=256 storage_buffers=8`
(`palette_len=0B` + the posed screenshot prove the effective path is CPU; the
`mode=` label caveat is explained at the end of this section.)
Per-unit probes:
- `GAME_SKIN_UNIT kind=TownHall model=3 nodes=81 palette_base=0
  palette_offset=0 palette_bytes=5184 draw_offset=0 bone0=37 max_bone=46
  weight0=0.500 shader_index0=37`
- `kind=Peasant nodes=42 palette_base=81 offset=5184 bytes=2688 draw_offset=1024
  bone0=18 max_bone=21 weight0=1.000 shader_index0=99`
- `kind=Footman nodes=57 palette_base=165 offset=10560 bytes=3648
  draw_offset=2560 bone0=4 max_bone=24 weight0=1.000 shader_index0=169`
- `kind=Archer nodes=68 palette_base=222 offset=14208 bytes=4352
  draw_offset=3328 bone0=9 max_bone=32 weight0=0.500 shader_index0=231`

So the inputs are correct on device: nodes are non-empty, every bone index is
inside its unit's palette range (`max_bone < nodes`), weights are non-zero, the
palette buffer is a 1 MiB `STORAGE | COPY_DST` allocation, the bind group is the
vertex storage binding, the dynamic uniform stride (256 = Adreno's
`min_uniform_buffer_offset_alignment`) never grew (`draw_grows=0`). The bind
pose therefore comes from the vertex-stage storage-palette execution on the
vendor Vulkan driver, not from the palette data, bind group or draw offsets.

### Fix
- `apps/android-native/src/lib.rs:847` forces the verified CPU skinning path on
  the device right after renderer construction:
  `renderer.set_skin_mode(wc3_game::SkinMode::Cpu);`
  `crates/wc3-render/src/mesh.rs::skin_cpu_into` (line 1553) skips the storage
  palette entirely and uploads posed vertices every frame; the desktop/headless
  default stays `SkinMode::Gpu` (`wc3-render/src/mesh.rs` constructor).
- GPU shader portability hardening (`wc3-render/src/mesh.rs:311` `vs_skinned`):
  the four influences are unrolled with constant indices
  (`palette[base + bone0]` ... `bone3`) instead of `for`/`weights[index]`, the
  pattern mobile drivers most often miscompile. GPU remains the default
  off-device; the device keeps CPU until the unrolled path is verified there.
- Latent bug found during the audit and fixed: `upload_params`
  (`wc3-render/src/mesh.rs:1397`) replaced `draw_buffer` on growth but left
  `draw_bind` pointing at the old allocation; it now recreates the bind group
  (line 1413).
- Observability fix: library diagnostics used `eprintln!`, which Android/SDL
  does not mirror to logcat. `wc3-game` now exposes `set_log_sink`
  (`crates/wc3-game/src/lib.rs`) and `dev_log!`; the app installs the sink at
  startup, so `GAME_SKIN`, `GAME_TERRAIN_TILES` and `GAME_DOODADS` are visible on
  device.

### Verification (device, one fix + one log-capture cycle)
- `analysis/agent-world/render-fix/skin-fixed.png` (2414x1080, 61 FPS): both
  TownHalls upright with roofs/walls/stone bases, Peasant and Footman standing
  posed on the rocks; the bind-pose slabs are gone.
- Two-frame animation diff: `skin-frame-b.png` captured 1.3 s later;
  `skin-animation-diff.png` shows 2352 changed pixels (bbox 632,146 - 1514,459,
  max channel delta 221) concentrated on the units - they are animating, not
  frozen.
- `cargo test -p wc3-game --lib agent_world`: 11 passed / 0 failed, rc=0 (GPU
  path, headless); `cargo test -p wc3-render`: 25 + 3 parity passed.
- Release rc=0, APK rc=0, `pm install` Success, `am start -W` Complete.

Note: the `GAME_SKIN mode=` field printed the value captured at renderer
construction, so the captured device log says `gpu` while `palette_len=0B` and
the posed screenshot prove CPU. `set_skin_mode`/`prepare` now refresh the label
(`wc3-render/src/mesh.rs:881`) and the game prints the effective
`mesh.skin_mode()` (`wc3-game/src/render.rs:298`); the next device build reports
`mode=cpu` without another code change.

## 14. Next step (after the banding fix)

- Apply §3.3 (`wc3-formats::map` `parse_doodads`) and delete the private
  `doodads::parse_doo`; optionally §3.5 (assets bridge/atlas cleanup,
  dispatcher-owned `Assets::load`). Then freeze the base.
- Any later terrain change must keep the invariant proved here: per-vertex
  weights only (no interpolated layer indices), eight fixed array layers, world
  UV in cell units, `seam_ratio < 1.35` on the clean frame, and the
  `agent_world` suite green.
- Device acceptance for this fix: one cycle (release -> APK -> install ->
  `am start -W` -> screenshot -> `am force-stop`), screenshot at
  `analysis/agent-world/render-fix/after.png`; pre-fix device frame preserved as
  `after-pre-bands.png/.ppm` for the seam-ratio comparison.

Evidence paths: this manifest;
`crates/wc3-game/src/{terrain_render,doodads,world_audit}.rs`;
`analysis/agent-world/tests-world-bands.log`;
`analysis/agent-world/{terrain-seams,terrain-textured}.ppm`;
`analysis/agent-world/render-fix/close700.ppm`.
