# AFTER — world/building art (packet `B-world`, ledger 481)

Same harness as [`BASELINE.md`](BASELINE.md): the Orc Great Hall (`ogre`) on
`(2)Justice.w3x`, 720×720, 45° pitch, hour pinned to 12.0, device GPU
(Turnip Adreno 725, Vulkan), building isolated as the largest 4-connected
difference component inside the camera-focus window and clamped to the model's own
projected box (so a contact shadow can never move the measurement bands).

Reproduce:

```
H2_WORLD_ART_LEGACY=1  H2_WORLD_ART_TAG=before    scripts/h2-cargo test -p wc3-render --lib world_art_hall_frame --offline -- --nocapture
H2_WORLD_ART_TAG=after                            scripts/h2-cargo test -p wc3-render --lib world_art_hall_frame --offline -- --nocapture
H2_WORLD_ART_LIGHT=shoulder H2_WORLD_ART_LEGACY_CONTACT=1 H2_WORLD_ART_TAG=lightonly …   (light only)
```

## What changed (`crates/wc3-render/src/mesh.rs`)

1. **Highlight shoulder instead of a hard clip.** `surface_light()` now runs
   `shoulder(ambient_light(n) + sun · saturate(N·L))` with `LIGHT_KNEE = 0.72`
   (identity below the knee, exponential approach to 1.0 above it). The raw DNC
   intensities are kept, so the shadow side and the dark ribs keep their authored
   contrast; only the top of the range, which used to clip flat, is compressed.
2. **Shaped ambient.** `ambient_light(n)` weights the single DNC ambient colour by
   `0.55 + 0.45 · (0.5 + 0.5 · N.y)`: an up-facing panel sees the whole sky term, a
   down-facing one sees 55 % of it. That is what gives the eaves and the roof
   underside a value distinct from the roof itself.
3. **Contact band from geometry.** `ModelGeometry` now carries the visible
   geometry bounds; each unit draw passes `with_contact(base_y, band, strength)`
   (`8 %` of the model height, `6` world-unit floor, `0.55` at the base) and the
   fragment shader darkens the bottom of the silhouette with a smoothstep.
4. **Ground shadow (the WC3 engine's own blob).** `push_ground_shadow()` emits a
   three-ring triangle fan under every unit and building — core, plateau to 0.62 R,
   fade to zero — sized from the model's footprint, offset away from the sun by
   18 % of the model height and lifted 5 world units. `PipelineKind::Shadow` +
   `fs_shadow` draw it with vertex alpha and no texture; doodad/terrain/ring draws
   keep the term disabled (`contact = [0; 4]`).
5. `DrawParams` grew `contact: [f32; 4]` (96 → 112 B). `draw_uniform_matches_wgsl`
   asserts the WGSL `Draw` struct still matches byte for byte.
6. Evidence switches (`H2_WORLD_ART_LEGACY`, `…_LEGACY_LIGHT`, `…_LEGACY_CONTACT`,
   `H2_WORLD_ART_LIGHT=shoulder|fit|legacy`) render any of the states from one
   binary. Default is `shoulder` + contact + shadow.

## Before / after (linear luma, same frame, same metrics)

| metric | before | light only | shipped (light+contact+shadow) |
|---|---|---|---|
| building pixels | 53213 | 53211 | 55790 |
| roof mean | 0.2485 | 0.2379 | 0.2188 |
| roof p95−p5 | 0.5404 | 0.5147 | 0.5078 |
| roof form (6×6 cell range) | 0.1833 | 0.1806 | **0.2777** |
| roof panel p90−p50 | 0.3036 | 0.2897 | 0.2977 |
| roof detail (mean ∇) | 0.1502 | 0.1435 | 0.1337 |
| wall mean | 0.1506 | 0.1416 | 0.1416 |
| base vs mid-wall | 0.765 | 0.789 | **0.713** |
| **shadow coverage of the ground ring** | 0.003 | 0.003 | **0.027** |
| shadowed ground luma / unshadowed | 0.352 (203 px noise) | 0.361 | **0.683** |

Vertex-level, texture-independent (`great_hall_light_distribution`, 3654 vertices of
the shipped model):

| | before | shipped |
|---|---|---|
| vertices clipped to 1.0 | **39.2 %** | **0.0 %** |
| up-facing band (n.y > 0.6, 1131 samples) on a single value | 46 % of the band | no value shared by more than 5 % |

## Reading the numbers honestly

* **The grounding is the visible win.** `shadow coverage 0.003 → 0.027` (9×) with
  the shadowed ground 32 % darker, and the silhouette base `0.765 → 0.713`. In the
  PNGs the palisade now sits in its own shadow instead of on top of the grass
  (`hall-before.png` vs `hall-after.png`).
* **The light fix is real but subtle on this building.** It removes a 39 % clip
  plateau and 46 % of the roof band collapsing onto one value, and the roof's
  large-scale form metric rises 0.1833 → 0.2777 — but the frame is only ~4 % darker
  at the roof, because the DNC peak (1.6) is *just* above the clip point (1.0) and
  the Great Hall's roof albedo is already a pale grey. Two of the three numbers that
  moved most (`roof_form`) also include the contact/shadow pixels entering the
  building mask, so they are an upper bound, not a pure light measurement.
* **`roof panel p90−p50` went *down* (0.3036 → 0.2897 with the light alone).** That
  is expected and not a regression: a plateau *inflates* p90−p50 (a large share of
  pixels sitting at the maximum). The analytic test (`roof_band_is_not_a_plateau`)
  is the one that measures the plateau directly.
* **`fit` (scaling the sun so `ambient + sun == 1.0`) was rejected by measurement:**
  it darkens the model 25 % (`roof_mean` 0.177) and lowers every form metric
  (`roof_form` 0.2553, `roof_panel` 0.2480). The shoulder keeps 96–99 % of the
  brightness and wins on all of them.
* **What is not fixed here:** the Great Hall's roof is a *pale* flat-faceted dome
  whose atlas carries most of its own shading; making it read as strongly lit as a
  hand-painted WC3 screenshot needs either a different tonal pipeline for the whole
  world (terrain included, so they stay consistent) or denser roof texel detail.
  Both are outside this packet's files — see [`PATCH-REQUESTS.md`](PATCH-REQUESTS.md).

## Tests (`scripts/h2-cargo test -p wc3-render --lib --offline`)

`132 passed; 0 failed`. New:

* `shoulder_is_identity_below_the_knee_and_approaches_one` — hand-computed curve behaviour.
* `surface_light_values_are_hand_computable` — roof / sun side / shadow side /
  under-eaves values computed by hand from the shipped DNC numbers
  (`sun 1.3, ambient 0.3, sun_dir (-0.6755, 0.6157, 0.4059)`).
* `shoulder_light_never_clips` — sweep over the whole unit sphere stays `< 1.0`.
* `roof_band_is_not_a_plateau` — the legacy light puts ~46 % of the up-facing band on
  one value, the shipped light on none.
* `contact_band_comes_from_the_model_bounds`, `draw_uniform_matches_wgsl`.
* `world_art_hall_frame` — the real offscreen frame with assertions on shadow
  coverage, base darkening and roof detail (and it prints the numbers above).

## Artifacts

* `hall-before.png` — legacy renderer (no shoulder, no contact, no shadow).
* `hall-after.png` — shipped renderer.
* `hall-lightonly.png` — shoulder light, legacy contact (isolates the light change).
* `compare-before-shoulder.png`, `compare-base2.png` — 2× side-by-side crops.

These are **engine renders of the shipped art**, not hand-drawn art and not a
procedural substitute for it.
