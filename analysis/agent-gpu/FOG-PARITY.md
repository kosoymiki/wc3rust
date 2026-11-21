# GPU FULL GOAL — render parity (ledger 352, 2026-11-14)

Task: pink/stretched verify, fog-of-war mask, neutral lighting, terrain variation grid,
device before/after screenshots in `analysis/agent-gpu/fog-fix/`.

## 1. Pink / exploded triangles — verified gone (before shot)

- Device before: `analysis/agent-gpu/fog-fix/before.png` + `before-units.png` (crop, 2×).
- Observed: TownHall/Peasant/units render with real BLP textures and team colours; **no magenta,
  no exploded/skewed triangles** in the full frame or the zoomed unit crop. The per-def animator fix
  holds on the installed build; after-build re-check queued with the fog/lights changes.
- Units visible in crop: Peasant with lumber (team-coloured blue cloth, brown lumber), TownHall
  stone/roof/wood textures intact.

## 2. Fog of war — implemented in the renderer (owner paths)

Root: the 3D renderer had **no fog pass at all**; only the minimap sampled the sim grid
(`crates/wc3-game/src/ui/minimap.rs`). The grey veil in the screenshot came from the old terrain
distance haze (World owner has since reduced it), not from fog.

Fix:
- `crates/wc3-render/src/fog.rs` (new): `FogMask` (0 unexplored / 1 explored / 2 visible) and
  `FogRenderer` — a depth-aware full-screen pass. Depth is loaded to reconstruct the world
  position (`inv_view_proj * vec4(ndc, depth, 1)`), the mask is sampled per 128-unit cell, and the
  shader emits opaque black for unexplored, 55% black for explored-not-visible, and `discard` for
  visible. One pipeline, one mask texture, one uniform; no bind groups leak into the HUD.
- `crates/wc3-game/src/render.rs`: `Renderer::set_fog(&sim, player)` builds the mask from the
  **sim's** `is_visible`/`is_explored` grid (`sim.grid.{width,height,cell_size}`); the fog pass is
  recorded after world/water/decals and before the UI pass; depth texture now carries
  `TEXTURE_BINDING`.
- `apps/android-native/src/lib.rs`: `renderer.set_fog(&game.sim, game.player)` once per frame (the
  wiring the app needs to feed the sim grid; no game.rs change).

Tests: `wc3-render` unit test `rendering_fog_mask_states` (three states, out-of-bounds unexplored,
visible fraction) — **PASS**; device test `rendering_fog_mask_states` (`tests/fog.rs`, real Vulkan
pipeline + depth reconstruction) — **PASS**: unexplored `[0,0,0]`, explored `[179,179,179]`,
visible `[255,255,255]` at 64x64.

## 3. Lighting — neutral noon

- `render.rs::day_night_light(12.0)` is `[1.0, 1.0, 1.0]` at brightness **1.0** (neutral); night/dawn
  keys keep WC3 warmth. The milky look in the before shot is the **old terrain haze** (the World
  owner changed `haze()` from a 60% mix starting at 2200 units to 28% starting at 3500 units).
- After-build screenshot will verify the neutral grade; no owner-path change needed beyond the
  mask (terrain shader is World-owned).

## 4. Terrain variation grid — status

- The before shot shows a regular checkerboard (the stale build predates the authored-mip tile
  extraction). Current source (`terrain.rs::tile_from_sheet` + 2px wrap blend, per-vertex shared
  corner weights in `build_terrain`) is rebuilt next; if any regular grid remains, the fix belongs
  to the **World-owned** `terrain_render.rs` (variation-nibble bilinear mask) and will be delivered
  as a patch with file:line, per the READ_ONLY rule.
- Numeric guard already in the tree: `wc3_render::terrain::seam_metric` (hard seam >20 vs blended
  <2) and `rendering_terrain_mips_seam_metric`.

## 5. Device evidence (before/after captured)

- Before: `analysis/agent-gpu/fog-fix/before.png`, `before-small.png`, `before-units.png`
  (2414×1080, `GAME_FIRST_FRAME surface=2414x1080`). Numeric: world magenta **0 px** / 2.08 M,
  mean saturation 0.531, no fog (map fully visible), old terrain haze.
- After: `analysis/agent-gpu/fog-fix/after.png`, `after-small.png`, `after-zoom.png`
  (APK built 13:35, installed, launched). Device log:
  `GAME_RENDER_SCALE swapchain=2414x1080 scene=2414x1080 native=true`;
  `GAME_READY source=user-mpq map="Justice" models=469 textures=533`;
  `P0_SURFACE_RECONFIGURED reason=lifecycle 2414x1080`; 42–62 units, 47–61 fps.
  Frame stats (world region, 2.08 M px): **black 37.6%** (unexplored), **dark 53.2%**
  (explored-not-visible), **visible 9.2%** — all three fog states present; **magenta 0 px**;
  mean saturation on non-black 0.505, no milky veil; zoom shows natural terrain variation, no
  regular checkerboard, correct team colours on Peasant/Footman and TownHall textures.

## 6. Gates / rc

| command | rc | result |
|---|---|---|
| `cargo test -p wc3-render` | **0** | **74 lib passed** (incl. fog + seam + event_objects/particles) + 4 astc + 3 fog + 10 fx + 3 parity + 3 cache |
| `cargo test -p wc3-render --test fog` | 0 | `rendering_fog_mask_states` device pass (black/179/255) |
| `cargo test -p wc3-render --test fog` | 0 | device pass: black / 179 / 255 |
| after build/APK/install | 0 / 0 / Success | `app_build_try=1 rc=0`, `apk_rc=0`, install Success |
| device after screenshot | **DONE** | `after.png` + `after-logcat.txt` captured at 13:36 |

Device commands (H2_DEVICE_SLOT=1 grant for this task):
`cargo build --release -p herofolends-android` → `bash scripts/build-apk.sh` → `pm install -r` →
`am start` → `screencap` → `analysis/agent-gpu/fog-fix/after.png`.
