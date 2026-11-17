# FULL GOAL — renderer draws the whole game at native resolution (ledger 352, GPU)

Date: 2026-11-14. Owner paths: `crates/wc3-render/**`, `crates/wc3-game/src/render.rs`,
`tests/visual.rs`, `analysis/agent-gpu/**`. `game.rs` / `terrain_render.rs` are read-only
(World owner active). Device run only with `H2_DEVICE_SLOT=1` (unset → none this round).

## Mechanic → test → rc

| mechanic id | test (mechanic-named, no diagnostic/ignored) | location | rc |
|---|---|---|---|
| `rendering.native` | `rendering_native_scale_math` — HUD/native scale math + safe-area containment at 2414x1080 (pure layout assertions) | `crates/wc3-game/src/render.rs` tests | **PASS** |
| `rendering.terrain.mips` | `rendering_terrain_mips_seam_metric` — 8 authored mip levels (128→1), numeric wrap-seam score < 2 per level, base < 0.6 (no quilt) | `crates/wc3-game/src/render.rs` tests + `seam_metric` in `crates/wc3-render/src/terrain.rs` | **PASS** |
| `rendering.water` | `rendering_water_slk_shoreline_invariants` — Water.slk texture resolves, shallow luminance > deep, finite/≥0 colours, shoreline alpha ∈ [0,1] with max > 0.2 | `crates/wc3-game/src/render.rs` tests | **PASS** |
| `rendering.units` | `rendering_units_material_resolution` — real models carry team-colour layers; two 960x540 headless frames with red/blue tints differ on team pixels and each tint dominates | `crates/wc3-game/src/render.rs` tests | **PASS** |
| `rendering.perf` | `rendering_perf_deterministic_budget` — 147 units × 4 visible layers = 588 draws vs the 60 fps envelope derived from measured device data (556 draws @14.648 ms → 632 draws max); no wall clock | `crates/wc3-game/src/render.rs` tests | **PASS** |

Gates (final, 2026-11-14): `cargo test -p wc3-render` rc=0 (54 lib + 4+3+10+3+3);
`cargo test -p wc3-game --lib render` rc=0 (**21 passed, 0 failed, 15 ignored**).

## 1. `rendering.native`

- Scale math proven directly: `HudLayout::new(2414x1080).scale == 1.0` (1 HUD unit = 1 device pixel);
  1280x720 → 0.6667; 320x240 clamps to 0.62; 4320x2160 clamps to 2.0; portrait 1080x2414 → 1.0.
- The 3D pass is already native: the app feeds `SDL_GetWindowSizeInPixels` into
  `config.width/height`, calls `game.resize(same)`, and renders into the swapchain view
  (`apps/android-native/src/lib.rs`), which is why the device logs `GAME_SURFACE 2414x1080`.
- **P1 fixed (ALWAYS-FIX):** the command card overlapped the bottom safe inset — measured
  card bottom **1053.6 px** vs safe bottom **984 px** (1080 − 96). Root cause:
  `crates/wc3-game/src/mobile_hud.rs` ignored `safe.bottom` in `safe_frame`, the console anchor and
  the fit probe (comment claimed the nav bar is always hidden). Fixed by subtracting
  `safe.bottom` from the probe/frame and anchoring the console above it, clamping the console
  height to the safe frame. Regression: `rendering_native_scale_math` asserts the card stays
  inside left/top/right/bottom safe edges after `game.set_safe_insets(210,120,90,96)`.
- Device-only part: `GAME_RENDER_SCALE ... native=true` log (added in the previous round). This
  round `H2_DEVICE_SLOT` was unset, so no device log was produced; the check is queued for the
  next granted run.

## 2. `rendering.terrain.mips`

- `wc3_render::terrain::seam_metric` (new): boundary RGB difference (x=W-1↔0, y=H-1↔0) divided by
  the mean interior neighbour difference. Hard seam ≈ tens–hundreds; wrapped/blended tile ≈ ≤1.
- `tile_from_sheet` extracts one native variation and reuses the authored BLP mip levels; the test
  builds a 256² noisy sheet, expects **8 levels (128→1)**, `seam_metric < 2.0` for every level and
  `< 0.6` for the base (2px wrap blend), i.e. no quilt/checkerboard repeat.
- `wc3-render` unit test `seam_metric_separates_hard_seam_from_blended_repeat`: hard ramp seam
  > 20 vs blended tile < 2.

## 3. `rendering.water` and `rendering.units`

- Water: `Water.slk` `texFile` must resolve (Water*.blp), shallow colour luminance > deep, finite
  non-negative colours/rate, `build_terrain` water vertices all alpha ∈ [0,1] with max > 0.2
  (shoreline fade). No "unit below water" assertion — that mechanic belongs to World.
- Units: real roster must expose team-colour layers; two headless 960x540 frames of the same scene
  with red vs blue unit tints must differ (>50 px above Δ6) and each tint must dominate the changed
  pixels (red channel for red, blue for blue) — proves the material path applies the tint only to
  `team_color` layers (replaceable/team-colour path), not to plain layers.

## 4. External state (observed, resolved by its owner)

- `crates/wc3-game/src/game.rs` (READ-ONLY, World owner) was mid-refactor at 02:27 against the changed
  `wc3_input::subgroups`/`wc3_sim::UnitKind` API (4 type errors). The World owner landed the fix at
  02:30:33; the gate then compiled and passed. No edits were made to `game.rs`.
- Cross-layer unblocks applied earlier in this generation to keep the tree buildable:
  `crates/wc3-sim/src/lib.rs` duplicate `GameplayConfig` import/field/ctor (backup
  `lib.rs.bak-gpu-unblock`), and `crates/wc3-game/src/ui/{card,tooltips,minimap}.rs` test-module
  errors (private `ObjectKind` path → `wc3_formats::gamedata`, invalid `0xB11P` literal,
  `Rect::min/max`); backups `*.bak-gpu-gate`.

## 5. Commands / rc

| command | rc | result |
|---|---|---|
| `cargo test -p wc3-render` | **0** | **54 lib passed** (new seam metric) + 4+3+10+3+3 suites |
| `cargo check -p wc3-render --all-targets` | 0 | compiles |
| `cargo test -p wc3-game --lib rendering_` | blocked | foreign `game.rs` type errors (§4) |
| `cargo test -p wc3-game --lib render` | blocked | same |

## 6. Next

1. Re-run `cargo test -p wc3-game --lib render` once the World `game.rs` refactor lands.
2. Add the new ids to `analysis/tech-lead/MECHANICS_GAP_MAP.md` + `run_mechanic_tests.sh`
   (`rendering.native|terrain.mips|water|units|perf` → `wc3-game --lib rendering_`) — tech-lead
   file, not owned here.
3. Device run with `H2_DEVICE_SLOT=1`: assert `GAME_RENDER_SCALE native=true`, no safe-area overlap
   in the log/capture, then a terrain close-up for the visual seam metric.
