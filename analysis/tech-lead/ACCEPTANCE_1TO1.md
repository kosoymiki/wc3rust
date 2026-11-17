# ACCEPTANCE — 1:1 Warcraft III, device-verified (hard gate)

No agent may report "done" against this list without the named artifact.

## Terrain (must equal the reference structure, not an approximation)
1. Albedo comes from the map's real `TerrainArt\*.blp` textures: texture array, per-vertex 4 tile layers + 4 W3E alpha weights, shader blends 4 samples. A procedural-noise-only albedo fails.
2. UVs are continuous in world space, 1 tile = 128 units; no per-cell reset, no atlas quadrant seams. Artifact: `analysis/agent-world/terrain-textured.png` + seam metric.
3. Map doodads are instanced from `war3map.doo` (trees, rocks, props) with position/yaw/scale/variation. Minimum on Human04: every doodad in the file, count printed.
4. Device screenshot at 2414x1080 next to `/storage/emulated/0/Download/images (3).jpeg`; grass/dirt/rock read as textures, not flat colour; no visible grid.

## HUD
5. Bottom console occupies 288/1080 px, built from real `HumanUITile01..04` / `ConsoleTexture` / `CommandButtonBorder` BLPs; rectangular bevels, radius 0; palette sampled from those BLPs.
6. Minimap 216 px, portrait 128 px, command card 4x3 (64 px cells, 4 px gaps), resources inside the bar, top strip 32 px. No floating chips, no joystick, no pill-shaped blobs.
7. Artifact: installed APK screenshot `analysis/agent-hud/console-grid.png`.

## Models
8. All 542 unit/building MDX run through `Animator::from_model → play → update → world_matrices → skin`: nodes indexed by MDX object id, every skin index in range, matrices change over time. CSV per model.
9. Archer runs forward, Footman sword attached, no corpse geosets in living sequences, Peasant stands on the ground (feet min-Y ~ 0 at Stand/Walk).

## Process
10. One writer per file. No overlapping agent edits.
11. Tests are not acceptance; only the installed-device artifacts above are.
12. Any claim without its artifact is a failed delivery and must be recorded as a capture.
