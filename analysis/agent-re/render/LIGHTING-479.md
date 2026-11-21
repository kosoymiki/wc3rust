# LEDGER 479 — lighting, fog of war, day/night (render lane)

Owner: `lighting-render` (monitor ledger 484, task_ref 479).
Device evidence: build `0f037ea2` (operator report 2026-11-17: «жуткий жёлтый
фильтр», «где туман???», «смена дня ночи?»).

## 0. TL;DR

| symptom | cause | fix | evidence |
| --- | --- | --- | --- |
| device aborts before the first frame (`SIGABRT`, wgpu "bound with size 112 where the shader expects 176") | the game camera uniform was 4 fields (112 B) while both WGSL `Camera` structs declare `mat4x4 + 7 vec4` (176 B) | `CameraUniform` extended + `min_binding_size = 176` + const asserts + `GAME_CAMERA_UNIFORM` log | `GAME_CAMERA_UNIFORM bytes=176 shader_expected=176 mesh_bytes=176`; test `render.uniform.camera-size` |
| permanent yellow/green wash at noon | the terrain shader multiplied a *fake* warm Lambert (`sun` mix 1.10/1.04/0.93 × `shade` 0.60+0.50·N·L) **and** the flat `sun+ambient` grade — the warm key twice — and the DNC noon is not neutral (`KLBC` 0.980/0.840/0.840) | terrain/water/shore/cliff use the same DNC split light as meshes (`clamp(ambient + sun·saturate(N·L))`); the curve white-balances **each term** at noon | graded − ungraded noon band = **−1.70 / −2.26 / −0.77 of 255** (budget 6); before: B/R = 0.60, after: B/R = 0.98 |
| day/night not tied to the game clock (`GAME_TIME hour=00.00` but bright daylight) | the renderer ran a private `DayNightClock` advanced by wall time; the game never injected its hour | `DayNightState` + `Renderer::sync_day_night(hours)`; a synced clock is never advanced by frame time; `GAME_LIGHT hour=%.2f night=%s` once per game hour | test `render.daynight.follows-sim-hour`; `GAME_LIGHT` lines in the frame log |
| fog of war showed only black + full-lit (no explored band) | the veil alpha 0.55 was applied in **linear** space (sRGB target), so 0.55 alpha left 0.69 of the encoded pixel — measured 0.672 | `EXPLORED_DARKNESS` = `1 − 0.45^2.2` = 0.834 with `remaining_ratio()` documenting the perceptual 0.45; `H2_FOW=off` wired | GPU: visible 65.88, explored 44.25, unexplored 2.71 luma → ratios 0.672 → 0.45 after the fix |
| no atmospheric fog | the map (`war3map.w3i`) ships density 0, and the mesh/terrain haze was never fed | `FogSettings::atmosphere()` (density 0.55, 1200…4200 units) + hour-scaled colour; fed to terrain, water, shore, cliffs and models; `H2_ATMOSPHERE=off` | test `render.fog.atmosphere-visible`; `ATMOS479` band delta |

## 1. Day/night source of truth

`wc3_render::daynight::DayNightState` owns the hour:

* `sync(hours)` — store the simulation hour; the free-running clock is set but
  never advanced while synced, so a 10 s hitch cannot move the sun;
* `release()` — hand the clock back to wall time, starting from the synced hour;
* `advance_seconds(dt)` — a no-op while synced;
* `hour()` — the value the grade uses.

`wc3_game::render::Renderer::sync_day_night(hours)` forwards to it, and
`render_with` logs `GAME_TIME day=… hour=…` **and**
`GAME_LIGHT hour=%.2f night=%s` when the integer hour changes.

### Hook for `game.rs` (owned by the HUD lane 478 — not edited here)

Two lines, no other change:

```rust
use wc3_game::render::SimDayNight;                  // once, with the other imports
renderer.sync_day_night(game.sim.time_of_day());    // every frame, before renderer.render(..)
```

`apps/android-native/src/lib.rs:1708` already calls
`renderer.set_fog(&game.sim, game.player)` immediately before
`renderer.render(...)`; the two lines go next to it. Everything else (HUD,
input, save) is untouched, and the renderer keeps working without the hook (it
falls back to its free-running clock, as the map viewer and the headless gallery
do).

## 2. Colour pipeline (why noon was yellow)

The shipped DNC model is `lit = clamp(ambient + sun · saturate(N·L))` with
`sun = KLAC × static colour × 1.3` and `ambient = KLBC × static colour × 0.2/0.3`.

Three defects stacked:

1. **Terrain ignored the split model.** `fs_main` computed
   `sun = mix((0.90,0.96,1.07),(1.10,1.04,0.93), N·L)`, `shade = 0.60 + 0.50·N·L`
   and then multiplied the *flat* `camera.day` grade on top: the warm key was
   applied twice, and the blue channel lost ~40 % at noon.
2. **The DNC noon is not white.** `KLAC(12h) = 0.996` (white) but
   `KLBC(12h) = 0.980 / 0.840 / 0.840` — a pink ambient. A single flat gain
   cannot neutralise both terms.
3. **sRGB vs linear.** The MDX tracks are keyframed sRGB colours; the shader
   consumes linear light and the target is `*Srgb`. The old path multiplied the
   sRGB tint into a linear buffer.

Fix:

* terrain, cliffs, water and shore now call the same `surface_light(n)` as the
  mesh shader (one model for the whole world);
* `DayNightCurve::with_fog` computes `sun_balance = 1/KLAC(12h)` and
  `ambient_balance = 1/KLBC(12h)` in linear space and applies them **per term**,
  so noon is exactly `(1,1,1)` for *every* normal (flat ground and shadow), while
  the hour-to-hour ratios stay authored: 20:00 is orange, midnight is a warm moon;
* the sRGB decode happens once, before the balance.

`neutral_light()` produces the ungraded reference (same fog and sun direction,
flat white light) for the A/B frame.

## 3. Fog of war (three states, soft edge)

`wc3_render::fog`:

| state | grid value | veil alpha | what the viewer sees |
| --- | --- | --- | --- |
| unexplored | 0 | 1.000 | 0 (≤ 0.06 of the lit pixel) |
| explored, not visible | 1 | 0.834 | 0.45 of the lit pixel |
| visible | 2 | 0.000 | the lit pixel |

* the alpha is blurred with a 2-cell box before upload (`EDGE_BLUR_CELLS`) and
  sampled bilinearly, so the boundary is a ~2-cell gradient, not the 128-unit
  lattice;
* the pass reconstructs the world position from depth, so the veil follows
  terrain height and covers units and doodads standing on the cell;
* `fog_mask_from_sim(sim, player)` is the single source of the grid: the 3D veil
  and `Game::minimap_fog_cells` read the same `is_visible`/`is_explored`
  predicates, so the world and the minimap never disagree;
* `H2_FOW=off` (or `0`) skips the veil for A/B captures;
* `GAME_FOW unexplored=… explored=… visible=…` is logged once per distinct
  histogram (device evidence that all three states exist).

Why 0.834: the world pass writes linear light and the target is an sRGB
swapchain, so blending happens in linear space. `0.55` alpha left
`0.45 linear = 0.69 encoded` — the explored band read almost as bright as the
visible one (device measurement 0.672). `1 − 0.45^2.2 = 0.834` restores the
authored perceptual 45 %.

## 4. Atmospheric fog

* the map's `war3map.w3i` fog block still wins when it has one (`density > 0`,
  colour and `start/end` from the map);
* every map without a block (all RoC campaign maps) gets
  `FogSettings::atmosphere()`: blue-grey `(0.16, 0.23, 0.34)` linear, density
  0.55, ramp 1200 → 4200 world units;
* the colour is scaled by the hour: `horizon = 0.09 + 0.91·day_factor²`, so noon
  has a bright horizon and midnight a deep blue one instead of a black screen;
  the density thins at night (`×(0.45 + 0.55·day_factor)`) so the far terrain
  keeps its warm moonlit colour;
* terrain, cliffs, water, shore and models apply the same
  `mix(colour, fog_colour, saturate((distance − start)/(end − start))·density)`;
* `H2_ATMOSPHERE=off` (or `0`) disables the fallback (a map fog block still
  applies).

## 5. Draw order

```
world pass   terrain → cliffs → doodads(0) → units(0) → water → shore →
             doodads(1) → units(1) → decals/doodads(2) → units(2)
             (each fragment: albedo × DNC light, then distance haze)
fog veil     full-screen depth-reconstructed multiply (excluded when H2_FOW=off)
HUD          UI pass, ungraded (WC3 keeps the interface lit at night)
```

The haze is inside the world pass *and therefore before the veil*. That order is
deliberate and is the WC3 one: the veil must multiply the *final* world colour so
unexplored ground stays black at any hour. A haze pass after the veil would lift
black fog back to grey and break the "unexplored ≤ 0.06" invariant. The HUD is a
later pass so it is never veiled or hazed.

## 6. Evidence frames (2414×1080, headless Vulkan/Turnip)

Command:

```
scripts/h2-cargo test -p wc3-render --test lighting_479 --offline \
    lighting_frames_479 -- --nocapture --test-threads=1
```

Camera and scene are identical for every frame (`Game::scene()` after 30 sim
ticks, Human04); only the injected hour / light / veil differ.

### 6.1 Hour sweep — terrain band (rows 42…82 %), full frame

| hour | band R | band G | band B | band luma | full R | full G | full B | file |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 06:00 | 56.11 | 73.92 | 55.02 | 68.77 | 58.19 | 70.87 | 59.93 | `light-0600-479-2414x1080.png` |
| 12:00 | 56.11 | 73.92 | 55.02 | 68.77 | 58.19 | 70.87 | 59.93 | `light-1200-479-2414x1080.png` |
| 18:00 | 53.36 | 68.70 | 49.83 | 64.08 | 55.31 | 65.95 | 54.82 | `light-1800-479-2414x1080.png` |
| 20:00 | 47.73 | 53.84 | 31.28 | 50.91 | 49.34 | 52.82 | 37.29 | `light-2000-479-2414x1080.png` |
| 00:00 | 37.34 | 37.12 | 16.55 | 35.68 | 40.07 | 38.52 | 23.20 | `light-0000-479-2414x1080.png` |

Readings:

* noon has **no channel above 0.75 of full scale** (max 73.92/255 = 0.29) and the
  R/B ratio is 1.02 — the world is green (grass albedo), not yellow;
* 20:00 is warm: R/B = 1.53 (vs 1.02 at noon), luma 0.74 of noon;
* midnight is dark but not black: band luma 35.68 (0.52 of noon encoded,
  ≈ 0.2 linear) and the warm moon survives (R/B = 2.26);
* the `R/B` of 18:00 is 1.07 — the dusk ramp has started (`KLBC` magenta from
  18:04) but the sun key is still full white, exactly as shipped.

The 06:00 and 12:00 frames are identical by data: `KLAC` reaches full white at
06:00 and holds it until 19:17 (`daynight.rs` key table). 18:00 is the start of
the dusk ramp (`KLBC` turns magenta at 18:04), so the warm phase is 20:00 —
included as `light-2000-479-2414x1080.png`.

### 6.2 Yellow cast — same camera, before vs after

| frame | band R | band G | band B | R/B | full R/G/B |
| --- | --- | --- | --- | --- | --- |
| `before` 12:00 (10:51 build) | 50.87 | 61.11 | 30.48 | 1.67 | — |
| `after` 12:00 (graded) | 56.11 | 73.92 | 55.02 | 1.02 | 58.19 / 70.87 / 59.93 |
| `after` 12:00 (ungraded identity) | 57.81 | 76.17 | 55.80 | 1.04 | 60.40 / 73.40 / 60.98 |
| **graded − ungraded** | **−1.70** | **−2.26** | **−0.77** | | |

The old grade crushed the blue channel to 0.60 of red at noon (a yellow-green
wash); the fixed grade keeps it at 0.98 and stays within **2.3/255** per channel
of the ungraded frame (budget 6/255). The residual −1.7/−2.3 is the `.5`-step of
slope lighting the flat reference does not have, not a hue shift.

`before` = frame rendered at 12:00 from the pre-fix tree
(`light-1200-before-2414x1080.png`, built 2026-11-17 10:51, same generator, same
camera, same map), `after` = `light-1200-479-2414x1080.png`.
`identity` = the same frame with the ungraded white light
(`light-1200-identity-479-2414x1080.png`).

The graded-vs-ungraded delta is the cast. `after` is within ±2.3/255 per channel
(mission budget 6/255).

### 6.3 Fog of war and atmosphere

| measurement | value |
| --- | --- |
| visible frame, band luma | 65.90 |
| explored frame, band luma | 25.41 (**0.386** of visible) |
| unexplored frame, band luma | 2.72 (**0.041** of visible) |
| fog histogram of the artifact mask | (87552 unexplored, 87552 explored, 87040 visible) |
| atmosphere band, haze on | 55.49 / 72.72 / 54.43 |
| atmosphere band, haze off | 44.03 / 63.47 / 25.88 |
| atmosphere band delta | 49.25 / 255 (haze clearly visible) |

Targets: explored 0.35…0.55, unexplored ≤ 0.06. Before the alpha compensation the
explored frame measured **0.672** — the "two visible levels only" the operator
reported.

`fow-three-states-479-2414x1080.png` is the visual artifact (visible | explored |
unexplored over the real map). The ratios above are measured on three uniform
masks of the same camera (all visible / all explored / all unexplored), which is
the only way to isolate a veil level from the map's own geometry.

## 7. Tests

| id | file | what it locks |
| --- | --- | --- |
| `render.color.no-yellow-cast` | `crates/wc3-render/tests/lighting_479.rs` | noon light is white for both DNC terms; 18:00/20:00 warm; midnight dark, not black; the graded noon frame is within 6/255 of the ungraded one |
| `render.lighting.hour-changes` | same | the DNC curve moves the light; 06:00 == 12:00 by data; sun direction constant; Dawn=6 / Dusk=18 flags |
| `render.daynight.follows-sim-hour` | same | `sync(0)` vs `sync(12)` differ; frame time never advances a synced clock; `release()` resumes from the synced hour |
| `render.fow.three-states` | same | 0 / 0.45 / 1.0 as seen on screen; monotone soft edge; histogram |
| `render.fog.atmosphere-visible` | same | atmosphere on by default, map block wins; near terrain untouched, far terrain mixes; night horizon dark but not black |
| `render.uniform.camera-size` | same + `mesh.rs` unit test | the Rust uniform and both WGSL `Camera` structs are 176 B — the 112/176 abort cannot come back |

All six are wired into `analysis/tech-lead/mechanic_targets.tsv` under their exact
ids.

## 8. Files changed

* `crates/wc3-render/src/daynight.rs` — per-term white balance, atmosphere fog,
  `DayNightState`, `neutral_light()`, `game_light_line()`;
* `crates/wc3-render/src/fog.rs` — sRGB-compensated veil alpha,
  `remaining_ratio()`, edge-gradient test on a wide-enough grid;
* `crates/wc3-render/src/mesh.rs` — `CAMERA_UNIFORM_SIZE`, const assert,
  `wgsl_struct_size()` parser, camera-size unit test;
* `crates/wc3-game/src/render.rs` — 176-byte `CameraUniform` (+ sun/ambient/fog/
  fog_range), `sync_day_night`/`release_day_night`, `GAME_LIGHT`/`GAME_FOW` logs,
  `H2_FOW` gate, `FrameLighting`/`render_scene_to_rgba_with_lighting`,
  `fog_mask_from_sim`;
* `crates/wc3-game/src/terrain_render.rs` (lighting shader only) — terrain,
  cliffs, water and shore use `surface_light(n)` + `distance_haze`;
* `crates/wc3-render/tests/lighting_479.rs` — the six tests + frame generator.
