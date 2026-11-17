# TERRAIN RENDER — HiveWE port cycle (ledger 384, 2026-11-14)

Supersedes the visual contract of `analysis/agent-world/terrain-fix/REPORT.md`
(ledger 380): that cycle added authored variations + a cliff texture but left
the three operator-visible defects open (water on dry land, seams/checkerboard,
missing cliff walls). This cycle fixes their roots with HiveWE as the reference.
The full donor matrix is `analysis/agent-world/HIVEWE_TERRAIN_MATRIX.md`.

## Root causes (observed, not inferred)

`Melee.w3x` (the active 1v1 map) W3E dump:

```
version=11 tileset=Y tags=8 ['Ydrt','Ydtr','Yblm','Ysqd','Vcbp','Ygsb','Vrck','Lgrd']
layer_hist {1:100, 2:153, 3:739, 4:1673, 5:3952, 6:5741, 7:155}
water_corners=1487 water_quads_any_corner=1839/12288 quads_with_dry_corner=830
ground range -2.54..3.47 tiles, final (HiveWE) range -1.0..6.78 tiles
```

1. **Missing cliff layer in the height function.** W3E stores a TFT cliff level
   per corner (1..7 on this map). HiveWE `terrain.ixx:269-272` renders
   `corner_height + layer_height - 2.0`; our `terrain_render.rs` rendered
   `corner_height` only. Every cliff on the map was flattened by up to 5.8
   tiles = **740 world units**, and the water plane (absolute level) ended up
   above the lowered ground — the sheet over dry land.
2. **Missing Water.slk offset.** Water.slk `LSha` has `height = -0.7` tiles;
   HiveWE `terrain.ixx:274-276` surfaces at `corner_water_height + offset`.
   We used the raw corner height (plus an invented `+2.0`), putting the surface
   **89.6 units too high** and making the shoreline cut across land.
3. **Water alpha from a broken lookup.** `fs_water` read a per-cell water-level
   texture with world-space UV (`floor(input.uv)` where uv is in world units)
   and multiplied the alpha by an `on_water` test sampled at the clamped map
   corner. Any water cell near the last map cell made the whole water mesh
   opaque; the shoreline fade was a hand-made `adjacency*(0.22+0.54t)`.
4. **Checkerboard/seams.** Two causes: (a) the packer sampled all 16 BLP
   subtextures as full tiles although 14 of them are alpha masks, and the W3E
   variation was read as 4 bits, folding this map's 16/17 onto 0/1; (b) black
   RGB under the alpha mask leaked into the albedo and into the mip chain, and
   `fract(cell_uv)` made the implicit derivative pick the whole-sheet mip at
   every 128-unit border.
5. **Shoreline coverage.** Quads were emitted only when the cell's *bottom-left*
   corner had the water flag; HiveWE emits a quad when **any** of the four
   corners is wet (`water.vert:19-24`). 830 shoreline cells on this map were
   drawn with the wrong coverage.

## Fixes (file:line in `crates/wc3-game/src/terrain_render.rs` unless noted)

| Fix | Where |
|---|---|
| `final_height()` = `(height + cliff_level - 2) * 128`, used by mesh/AO/normals/water bed | `:882-888`; call sites `:296-299`, `:531-535`, `:1093-1104` |
| `final_water_height()` = `(water_height + Water.slk height) * 128` | `:890-894` |
| `WaterMaterial` carries `Smin/Smax/Dmin/Dmax` RGBA + `offset`; parsed from Water.slk | `:1001-1105`, `:1616-1700` |
| `water_ramp()` = WC3 10/64/72-unit colour ramp with authored alpha | `:896-967` |
| `build_water()` = any-corner quad rule, per-corner level, ramp alpha (no `on_water`, no `+2.0`) | `:516-630` |
| `pack_variation_sheet()` = `height/4` tile size, extended 8×4 layout, engine subtextures 0/15 or 16+n, hole fill | `:1024-1113` |
| five-bit variation in the R8 Uint texture; shader maps 16→15, >16→0 | `:1857-1872`, `TERRAIN_SHADER` fs_main (`:697`) |
| explicit LOD from continuous UV (`dpdx/dpdy * 512`) for all 8 layer samples | `TERRAIN_SHADER` fs_main `:703-706` |
| units/doodads stand on the rendered cliff top (`Assets::height_at`) | `crates/wc3-game/src/assets.rs` `height_at` |
| audio occluder sees the same final heights | `crates/wc3-game/src/assets.rs` `install_audio_occluder` |

The water fragment shader is now HiveWE `water.frag` exactly:
`texture(Water.blp, uv) * vertex_color` with the vertex colour from the depth
ramp, alpha-blended (SRC_ALPHA / ONE_MINUS_SRC_ALPHA, depth test on, write off).

## Evidence

- Gate: `H2_AUDIO=null cargo test -p wc3-game --lib terrain_render --offline -- --test-threads=1`
  → **`test result: ok. 17 passed; 0 failed; 7 ignored`**, `GATE_RC=0`
  (2026-11-14T20:23:05+03:00, `gate.log`); the earlier run is
  `gate-run1.log` (16/1 before the TownHall recalibration below). The gate runs in
  the documented CI null-audio mode because Termux PulseAudio aborts the test
  process when ~20 parallel tests each open an SDL device
  (`pthread_mutex_unlock() failed … pulsecore/mutex-posix.c:114`); the audio
  path is covered by `cargo test -p wc3-audio` (34+8+1, rc=0).
- Recalibration: `agent_world_units_are_readable_at_1080p` measured
  `townhall_height=120 world=113px` — the per-definition MDX on its real cliff
  layer, not the old 497-unit fallback model on a flattened map. The bound is
  now 90 px (still 8.3 % of the 1080p frame); `archer_height=136px` unchanged.
- New contract tests:
  `hivewe_cliff_layer_heights_raise_the_terrain`,
  `hivewe_water_uses_slk_offset_and_four_colour_ramp`,
  `hivewe_variation_packing_uses_engine_subtextures`,
  `hivewe_alpha_holes_are_filled_not_black`,
  `hivewe_variation_is_five_bits`,
  plus the updated `rendering_terrain_water_plane` /
  `rendering_terrain_cliffs` / `agent_world_cliffs_ramps_shoreline`.
- Device (Melee.w3x, APK sha256 `44f2cda8…`, `herofolends-384.apk`):
  `device-before.jpg` (the reported checkerboard on the same map),
  `device-after.png` (clean grass, no checkerboard, no grid, real doodads),
  `device-pond2.png` (zoomed-out base: dry ground, no translucent sheet),
  `device-markers.log`:
  `GAME_TERRAIN layers=9 vertices=12513 tris=24576 water_vertices=2342 water_tris=3678`
  `GAME_SCENARIO melee … units=88 doodads=1997` and
  `GAME_AUDIO_STATS … underruns=0 … occluded=4 ducked_blocks=466`.
- Water on device: the app camera starts inland and the minimap click path
  currently jumps the camera outside the rendered terrain (black viewport,
  reproducible, `game.rs` camera/minimap mapping — outside this task's
  read-only file scope). Water on **Melee.w3x** is evidenced by headless
  close-ups rendered from the same mesh the device draws:
  - `melee-pond.png` — the pond at (44,13): blue water confined to the basin,
    cobblestone/dirt shore dry right up to the edge (`water_blue=0.178`);
  - `melee-cliff.png` — the largest layer step at (64,71): a real vertical
    cliff face with the authored cliff texture, flat ground below;
  - `water-after.png` (Human04) and `terrain-seams.png` (river + cliff bank).

## Honest limitations

- WC3 cliff faces are authored MDX models with a rounded S-profile; our mesh
  renders the same per-corner layer heights as a linear face with the real
  `Cliff0.blp` cliff layer. The wall is present and textured (the defect was
  "missing"), the authored bevel is a follow-up that needs MDX instancing.
- Ramp entrance +0.5 lip (HiveWE `terrain.ixx:943-990`) is not applied; ramp
  cells are already smooth slopes through their corner heights.
- Fog-of-war is unchanged; the earlier "fog washes the map" report was the
  water sheet, now removed at the root.
- Observed during device triage: tapping the minimap moves the camera to a
  fully black viewport (no terrain, HUD only) on this build. That is a
  camera/minimap-coordinate bug in `game.rs` (read-only here), recorded for the
  HUD/camera owner; it is unrelated to the terrain mesh (the same frame at the
  base renders correctly).
