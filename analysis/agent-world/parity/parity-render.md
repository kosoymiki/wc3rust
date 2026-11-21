# PARITY RENDER — units/buildings by race, portraits, day/night, weather, water types, all doodads

Date: 2026-11-13 (MSK). Agent: agent-world, ledger 348 follow-up.
`analysis/tech-lead/PARITY_MATRIX.md` was **not published** at 10:30 MSK; this
document is written to the same row shape and will be aligned when it appears.

## 0. Inventory (observed, from `analysis/war3_full`, 453 MB extracted tree)

| group | human | orc | undead | nightelf | creeps | critters | demon | other |
|---|---|---|---|---|---|---|---|---|
| units `.mdx` | 47 | 41 | 44 | 38 | 173 | 37 | 31 | — |
| unit portraits `*_Portrait.mdx` | 23 | 18 | 20 | 17 | 79 | 15 | 14 | — |
| buildings `.mdx` | 10 | 9 | 11 | 16 | — | — | 8 | 77 |
| building portraits | 0 | 0 | 0 | 0 | — | — | 0 | 0 |

Doodads: `Doodads\` 12 terrain category dirs, **1023 `.mdx`**
(Terrain 446, LordaeronSummer 157, Northrend 85, Barrens 65, Dungeon 58,
Cityscape 54, Felwood 50, Ashenvale 47, Underground 31, Cinematic 13,
Village 8, Dalaran 7). Environment/fire/sky: **45 `.mdx`**.
Total model files walked by the gallery: **2048**.

## 1. Units / buildings by race — load + render

Tool: `crates/wc3-game/tests/asset_gallery.rs` (the project's parity gallery).
It walks every `.mdx`, builds real MDX geosets + BLP textures through
`wc3_assetdb::AssetDb`, and renders each model with the game's own
`render::Renderer` at 160x160.

Fix made for parity: the gallery used to render with `nodes: Vec::new()`, i.e.
bind pose — buildings exploded and skinned units collapsed to the origin. It now
plays the model's `Stand` sequence, evaluates `Animator::world_matrices()` and
passes that node palette, so every unit/building/portrait thumbnail is posed.

Coverage/evidence: `analysis/agent-world/parity/gallery/models/<Category>/`
(`Units`, `Buildings`, `Doodads`, `Environment`, ...) + `manifest.jsonl` +
`summary.json`. Counts per race and portrait counts as in section 0.

## 2. Portraits

WC3 portrait models are `*_Portrait.mdx` next to the unit model
(`Units\Human\Footman\Footman_Portrait.mdx`). Inventory: 223 portrait models
across human/orc/undead/nightelf/creeps/critters/demon (section 0); buildings
have none in this install. The gallery renders them as their own thumbnails
(posed at Stand; portrait cameras are not implemented — the gallery uses the
model bounds camera).
The HUD portrait panel (`game.rs:1974`, `mobile_hud.rs` `Portrait`) currently
shows the unit icon, not a 3D portrait render.

## 3. Day / night

Implemented in `crates/wc3-game/src/render.rs`:
`pub fn day_night_light(hours) -> [r,g,b,brightness]` with WC3-style keys —
00:00 night `[0.52,0.62,0.92] x0.26`, 06:00 dawn `[1.00,0.80,0.64] x0.62`,
12:00 day `[1,1,1] x1.00`, 18:00 dusk `[1.00,0.74,0.52] x0.55`.
`Renderer::set_time_of_day(hours)` writes it into the shared camera uniform
(terrain + water) and into the mesh renderer (`MeshRenderer::set_day_light`),
so terrain, water, units and doodads all grade together; the sky clear colour
is tinted by the same value and the HUD keeps its own brightness (as in WC3).
Test: `agent_world_day_night_cycle` — four shots (00/06/12/18) of Human04 with
mean-luminance ordering day > dusk > dawn/night and night blue-dominant.

## 4. Weather

`TerrainArt\Weather.slk` parses to **19 effects** (e.g. `RAhr`/`RAlr` Ashenvale
rain, `MEds` Dalaran shield, `FDbh` dungeon blue fog, snow `SN*`), each with
`texDir`/`texFile` (12 weather textures in
`ReplaceableTextures\Weather\`), emission rate, particle count, lifespan,
velocity, colour ramps and alpha ramps.
Current renderer: parses + decodes the table but does **not** emit weather
particles (gap, exact next step: a weather pass over the existing mesh pipeline
using the `emrate`/`particles`/`lifespan`/`veloc` fields and the
`ReplaceableTextures\Weather\*.blp` frames).

## 5. Water types

`TerrainArt\Water.slk` parses to **13 water types** (`LSha`, `FSha`, `WSha`,
`NSha`, `ASha`, ...), each with `texFile` frame stem, `numTex=45` frames,
`texRate`, shallow `Smin/Smax` and deep `Dmin/Dmax` colours, alpha modes and
**shoreline doodads** (`shoreDir`/`shoreSFile` = `Shoreline`,
`ShorelineOutsideCorner`, `ShorelineInsideCorner`, `shore*Var`).
Implemented: `water_material` selects the row for the map tileset
(`<Tileset>Sha`), loads frame 0 (`Water00.blp`) and applies the Water.slk
shallow/deep colours and `texRate` scroll; the water surface itself was fixed in
round 2 (shared-corner grid + shoreline fade).
Evidence: `agent_world_water_weather_cliff_tables` decodes all 13 water frame-0
textures, all 19 weather textures and 26 cliff ids (0 failures), and writes
water/weather contact sheets. Gap: the `Shoreline*` doodads named by Water.slk
are not spawned yet; frame animation still scrolls instead of stepping 45 frames.

## 6. All doodads

Round 2 census (8 maps, exact batch vertex counts, 0 model failures after the
meshless-particle fix) plus the gallery's `Doodads/` category (1023+ models
rendered posed). Doodad alpha and team colour were verified in round 2
(`DOODAD_ALPHA` table, Orc07 team layers -> 40 tinted vertices).

## 7. Rows / status

| row | status | evidence |
|---|---|---|
| units per race loaded + rendered | done | gallery `models/Units/<Race>/*.ppm`, 47/41/44/38 unit models |
| buildings per race | done | gallery `models/Buildings/<Race>/*.ppm`, 10/9/11/16 |
| portraits | done (load/render) / HUD 3D gap | 223 portrait thumbnails |
| day/night | done | `day_night_light` + 4 shots test |
| weather | parsed + textures decoded / particles gap | 19 effects, 12 textures |
| water types | done (13 rows, per-tileset) / shore doodads gap | 13 textures + colour table |
| all doodads | done | gallery `models/Doodads/*` 1023 ppm + round-2 census |

## 8. Artifacts

- `analysis/agent-world/parity/gallery/models/**` (thumbnails + manifest + summary)
- `analysis/agent-world/parity/day-*.ppm` (day/night), `water-types.ppm`,
  `weather-types.ppm`
- logs: `analysis/agent-world/parity/gallery.log`, `tests-parity.log`
- this report.

## 9. PARITY_MATRIX §7 gates — per-definition model loading (#1) + LRU budget (#2)

Aligned rows: **R1** (`4/216 = 1.9 %` unit meshes per race) and **R2**
(`1/46 = 2.2 %` buildings), matrix §7 #1/#2.

Implementation (`crates/wc3-game/src/model_render.rs`, owner path):
- `ModelRegistry` keyed by the real **`UnitDef.model` path** (normalised
  `\`-separated, e.g. `Units\Human\Footman\Footman.mdx`), backed by
  `wc3_assetdb::AssetDb` over the operator's extracted install.
- One entry per definition: `meshes` via `crate::assets::build_meshes` (the same
  canonical builder the game uses - no second MDX reader), textures decoded with
  `wc3_formats::blp` and capped to 128 px (never upscaled), and the **`Stand`
  pose evaluated once through `wc3_anim::Animator`** (`world_matrices()`), reusing
  the animator the game already runs instead of a second animation path.
- Exact byte accounting (positions/normals/uvs/indices/bone weights/layers +
  texture RGBA + palette) and **LRU eviction** under a byte budget;
  `RegistryStats` exposes hits/loads/misses/evictions/meshless/failures/peak.
- `LoadOutcome::{Loaded, Meshless, Failed}` separates parsed particle-only
  models (0 geosets, e.g. `Units\Undead\PlagueCloud\PlagueCloud.mdx`) from real
  failures.

Gate results (fresh run after the server restart, `cargo test -p wc3-game --lib
models_per_def -- --ignored --nocapture`, rc=0, 3/3 pass):

```
MODELS_PER_DEF required=216 loaded=214 meshless=2 failures=0 resident=214
  memory_mb=73.4 peak_mb=73.4 evictions=0
  races={"Human": 57, "NightElf": 54, "Orc": 50, "Undead": 53}
MODELS_PER_DEF_LRU budget_mb=32 loads=214 misses=216 resident=116 evictions=98
  used_mb=31.7 peak_mb=32.0
MODELS_PER_DEF_GALLERY races=4 rendered=32 file=analysis/agent-world/parity/models-per-def.ppm
```

- **216/216 four-race MDX** (170 Units + 46 Buildings, portraits included) parse;
  214 upload real geometry, 2 are meshless particle models; **0 failures**.
- Whole cache **73.4 MB peak**, far under the 1536 MB phone budget; a 32 MB
  budget evicts 98 entries and stays at 31.7 MB, so the LRU path is real.
- Headless gallery: 8 posed definitions per race rendered through the game
  renderer (`cargo test -p wc3-game --lib models_per_def_gallery -- --ignored`).

Map roster (Human04 `war3mapUnits.doo` -> `UnitDef.model` -> registry):

```
MAP_ROSTER_MODELS map=Human04 placed_instances=90 distinct=37 resolved_defs=15
  models_loaded=15 unresolved=22
```

- Every **race-defined** placed unit/building resolves and loads its own model
  (15/15, 100 %; previously all of them rendered the 4 starter meshes).
- 21 unresolved ids are neutral/creep units (`nC02`, `nftr`, `ngol`, `sloc`,
  `ugol`, ...) that have no `UnitDefId` yet — the sim layer is adding
  `CREEP_DEFS`; re-run this test after that lands.
- `unp1` (Halls of the Dead) has `model_present=false`: the RoC extracted tree
  has no `Buildings\Undead\HallsOfTheDead` (TFT asset), so this is a truthful
  absence, not a loader failure.

Integration notes / remaining tails:
- `assets_models_min` is still 4 in `analysis/agent-qa/acceptance/spec.json`
  because the app's `GAME_READY models=` still reports `Assets.models.len()`
  (4). Wiring the registry into `Assets`/`Game` (assets/game owners) is the last
  step to raise the floor to 542; the registry API is ready
  (`ModelRegistry::open_dir`, `load`, `stats`, `resident`, `used_bytes`).
- Per-object animator nodes: the registry provides the per-model Stand palette;
  the runtime keeps one `wc3_anim::Animator` per object id (existing
  `UnitVisual`), so no second animation state is introduced.
- Android app already forces `SkinMode::Cpu` (`apps/android-native/src/lib.rs`);
  registry models keep `bones` empty after CPU posing, so both skin modes
  render the cached geometry identically.

Artifacts: `analysis/agent-world/parity/models-per-def.ppm`,
`analysis/agent-world/parity-models.log`, `analysis/agent-world/parity-roster.log`,
`analysis/agent-world/tests-models-per-def.log` (full lib suite: 91 passed,
0 failed, 18 ignored, rc=0).
