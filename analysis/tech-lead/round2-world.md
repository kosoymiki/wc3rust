# ROUND 2 — world / biome validation (ledger 348, task_ref r2-world)

Date: 2026-11-13 (MSK). Agent: agent-world (layer owner).
Owner paths: `crates/wc3-game/src/{terrain_render,render,model_render,doodads}.rs`.
Fixtures: `analysis/war3_data/Maps/Campaign` (44 campaign maps, read-only) and
`/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq` (read-only).

Goal: prove every biome (water/snow/ice maps, cliffs/ramps, shoreline), the full
TerrainArt catalogue (111 BLPs) and doodad alpha/team-colour handling with
headless evidence, and fix what the evidence shows is broken.

## 1. TerrainArt catalogue — 111 BLPs

Test: `agent_world_terrainart_catalog_all_tiles` (ignored, round2).
Command: `cargo test -p wc3-game --lib agent_world_terrainart_catalog_all_tiles -- --ignored --nocapture`

Observed:
- `TerrainArt\Terrain.slk`: **98 ground-tile rows**, one distinct BLP per row.
- `TerrainArt\` tree: **111 BLPs** = 98 ground + 12 blight overlays
  (`TerrainArt\Blight\*.blp`) + `TerrainArt\Nothing\nothing.blp`.
- All **111 decoded, 0 failures**; `CliffTypes.slk` parses with **26 cliff ids**.
- Classifier over the 98 ground tiles: grass 33, dirt 45, rock 10, crops 4, water 6.
- Artifacts: `analysis/agent-world/round2/terrainart-111.ppm` (12x10 contact
  sheet, tiles in SLK order, 64 px cells) and `terrainart-111.txt` (index, id,
  archive path, average RGB, biome, ground/special).

## 2. Per-biome matrix — 13 tileset families, headless screenshots

Test: `agent_world_biome_matrix_screenshots` (ignored, round2; 285 s).
Each shot renders the real map through `Game` + `render_scene_to_rgba`
(2414-style pipeline, 1200x675) and asserts `source=terrainart`, >=8 fixed
layers, snow/ice on N/W maps, a Water.slk surface when the map has water cells,
`non_black > 0.55` and >=40 colour buckets. The frame is centred on the nearest
dry 5x5 neighbourhood of the scenario base (some campaign bases land in water).

| tileset | map | tags | layers | snow | ice | water cells | non_black | buckets | shot |
|---|---|---|---|---|---|---|---|---|---|
| L Lordaeron Summer | Human01 | 6 | 8 | 0 | 0 | 604 | 0.936 | 649 | biome-L-Human01.ppm |
| V Village | Human04 | 8 | 8 | 0 | 0 | 2133 | 0.844 | 646 | biome-V-Human04.ppm |
| X Dalaran | Undead07 | 9 | 9 | 0 | 0 | 2259 | 0.945 | 609 | biome-X-Undead07.ppm |
| Y Cityscape | Human06 | 9 | 9 | 0 | 0 | 1938 | 0.980 | 719 | biome-Y-Human06.ppm |
| N Northrend | Human09 | 7 | 8 | 1 | 1 | 0 | 0.981 | 716 | biome-N-Human09.ppm |
| A Ashenvale | NightElf01 | 8 | 8 | 0 | 0 | 3775 | 0.681 | 743 | biome-A-NightElf01.ppm |
| G Underground | NightElf05 | 8 | 8 | 0 | 0 | 1703 | 0.995 | 669 | biome-G-NightElf05.ppm |
| C Felwood | NightElf06 | 7 | 8 | 0 | 0 | 10035 | 0.997 | 217 | biome-C-NightElf06.ppm |
| B Barrens | Orc01 | 8 | 8 | 0 | 0 | 12214 | 0.946 | 1170 | biome-B-Orc01.ppm |
| D Dungeon | Orc07 | 8 | 8 | 0 | 0 | 1564 | 0.989 | 614 | biome-D-Orc07.ppm |
| F Lordaeron Fall | Undead01 | 6 | 8 | 0 | 0 | 9600 | 0.878 | 604 | biome-F-Undead01.ppm |
| Q Dalaran Ruins | Undead02 | 8 | 8 | 0 | 0 | 1253 | 0.817 | 561 | biome-Q-Undead02.ppm |
| W Lordaeron Winter | Undead06 | 6 | 8 | 1 | 0 | 1331 | 0.945 | 818 | biome-W-Undead06.ppm |

`BIOME_MATRIX tilesets=13 {'A','B','C','D','F','G','L','N','Q','V','W','X','Y'}`
— every tileset family on this install rendered. The N shot is white snow/ice
(Human09); the W shot is snow with winter trees (Undead06).

## 3. Water / snow / ice / shoreline

Water flag/level audit (diagnostic `agent_world_water_flag_distribution`):
- Human04: 2127 flagged cells, 161 wet (depth > 3), 1966 dry.
- Human07 (N): 1741 flagged, **0 wet** — its water cells are dry; it cannot
  show a shoreline (used only for the snow/ice layer test).
- Human08 (N): 8173 flagged, all wet (median depth 259.5).
- Orc01 (B): 12161 flagged, 8602 wet (median 130).
- NightElf06 (C): 9861 flagged, 8637 wet.
- Undead06 (W): 1331 flagged, 208 wet.

Snow/ice materials come from `Terrain.slk` (`snow`/`ice` params per layer):
Human07/08/09 and Undead06 report snow=1; Human07/08/09 report ice=1.

Shoreline fix (root cause + diff):
`terrain_render::build_water` emitted one quad **per water cell** with its own
water level and alpha (`alpha = 0` only when depth < 3). Neighbouring cells did
not share vertices, so the surface ended on opaque cell rectangles and every
cell could step its level; the close-up showed a stair-stepped edge.
Diff: the surface is now built on the shared grid. Each grid corner samples the
water level from the up-to-four water cells that touch it, and the alpha fades
with `smoothstep(0, 48, depth)` **and** by the corner's water-cell fraction
(`water_cells / 4`), so the last cell is 25–75 % transparent and the surface
blends into the shore instead of ending on a cell boundary. Both the level and
the fade are interpolated by the rasteriser.
Evidence: `analysis/agent-world/round2/shoreline.ppm` (Human08 coast); the old
build produced a 1-cell opaque staircase, the new one a depth/adjacency ramp.

## 4. Cliffs and ramps

Test: `agent_world_cliffs_ramps_shoreline` (ignored, round2) on Human08.
Sites found from the W3E data (10-cell margin, away from map edges):
- cliff `(81,108)`: height step **142.5 world units** (>= 115).
- ramp `(60,91)`: **100.0 world units** over one cell.
- shoreline `(93,16)`: wet water cell (depth > 20) adjacent to land.
Mesh check: the built terrain vertex height equals `Assets::height_at` at every
corner of those cells — `CLIFF_MESH max_corner_error=0.0000` (no cracks, no
coordinate split error).
Shots: `round2/cliff.ppm`, `round2/ramp.ppm`, `round2/shoreline.ppm`
(terrain-only, 1000x700, `skip_units/skip_ui/skip_doodads`).
Known limitation: cliffs are rendered as the heightfield's steep faces; WC3's
separate cliff/ramp meshes (`CliffTypes.slk` cliffModelDir/rampModelDir and the
cliff textures) are not instantiated yet. The height step, ramp slope and mesh
continuity are correct; the cliff *art* is the remaining visual gap.

## 5. Doodad alpha and team colour

Tests:
- `agent_world_doodad_alpha_teamcolor` (ignored, round2) — audits 8 maps.
- `doodad_batch_applies_layer_alpha_and_team_tint` (fast unit test) — synthetic
  layer with alpha 0.5 and a team-colour layer; proves the merged static batch
  keeps the alpha in the vertex stream and applies the team tint
  (`DOODAD_BATCH vertices=22 translucent=19 team_tinted=3`).

Audit (merged static batch vertex counts must equal the visible
(geoset alpha x layer alpha x instance) sum; verified exactly):

| map | models | instances | vertices | model failures | meshless | texture failures | alpha layers | team layers | team-tinted vertices |
|---|---|---|---|---|---|---|---|---|---|
| Human04 | 86 | 2509 | 159125 | 0 | 0 | 0 | 3 | 0 | 0 |
| Orc01 | 94 | 3958 | 297761 | 0 | 0 | 0 | 3 | 0 | 0 |
| Undead01 | 103 | 2289 | 189123 | 0 | 0 | 0 | 4 | 0 | 0 |
| NightElf01 | 68 | 4312 | 310042 | 0 | 0 | 0 | 3 | 0 | 0 |
| NightElf04 | 130 | 6276 | 400682 | 0 | 1 | 2 | 8 | 0 | 0 |
| Orc07 | 116 | 1658 | 196769 | 0 | 0 | 11 | 7 | **2** | **40** |
| Undead07 | 54 | 1652 | 72705 | 0 | 0 | 1 | 4 | 0 | 0 |
| Human08 | 77 | 1805 | 149641 | 0 | 0 | 2 | 0 | 0 | 0 |

Orc07 carries the only team-coloured doodad layers (2) and they reach the batch
as 40 vertices with the exact `[0.55, 0.72, 1.0]` tint. Hidden geosets are
excluded (23/11/30/5/17/24/17/10 per map).

Fix found by the audit: `BlightDoodad` parses but has **no geosets** (a
particle-only model). It was counted as `model_failures=1`; `doodads.rs` now
separates parsed-meshless models into `stats.meshless_models` (logged as
`meshless=`) and does not report them as failures. NightElf04 now reads
`model_failures=0 meshless=1` (confirmed on the re-run).

## 6. Tests / commands

- `cargo test -p wc3-game --lib agent_world -- --nocapture` -> **11 passed,
  0 failed** (13 round-2 tests ignored by default), rc=0
  (`analysis/agent-world/tests-world-round2.log`).
- Round-2 heavy tests (explicitly run):
  - catalogue: 98 rows / 111 BLPs / 11 decoded per tile, 59 s, PASS.
  - biome matrix: 13 tilesets, 285 s, PASS
    (`analysis/agent-world/round2/biome-matrix.log`, `MATRIX_RC=0`).
  - cliff/ramp/shoreline: 133 s, PASS.
  - doodad alpha/teamcolor: 150 s, PASS.
  - synthetic doodad batch tint: PASS (instant).
- `cargo test -p wc3-game --lib` full module suite: result in
  `analysis/agent-world/tests-all-round2.log`.

## 7. Known gaps (reported, not hidden)

1. Cliff/ramp **meshes** and cliff textures from `CliffTypes.slk` are not
   instantiated; cliffs are heightfield faces (see section 4).
2. Blight **overlays** are decoded in the catalogue (12 BLPs) but not applied as
   a terrain overlay; blight areas use the base ground textures.
3. Scenario base placement can land in water on some campaign maps (Orc01,
   Human06); the biome shots compensate by framing dry land. The scenario
   `find_spot` itself is a sim-layer item.
4. A few doodad **textures** fail to decode on some maps (audit: Orc07 11,
   NightElf04 2, Undead07 1, Human08 2; other audited maps 0) and fall back
   to white. The affected materials are non-terrain props. Not fixed this
   round.

## 8. Artifacts

`analysis/agent-world/round2/`:
`terrainart-111.ppm`, `terrainart-111.txt`, `biome-<T>-<Map>.ppm` (13),
`cliff.ppm`, `ramp.ppm`, `shoreline.ppm`, `biome-matrix.log`
(`biome-N-Human07.ppm` is the earlier Northrend attempt, superseded by
`biome-N-Human09.ppm`; kept for audit, not part of the 13-family set);
`analysis/agent-world/{tests-world-round2.log,tests-all-round2.log}`;
this report.
