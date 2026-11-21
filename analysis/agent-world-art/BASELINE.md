# BASELINE — world/building art flatness (packet `B-world`, ledger 481)

Measured on the shipped install, headless, on the device GPU (Turnip Adreno 725,
Vulkan, `RENDER_ADAPTER` line in the run log). Reproduce with:

```
H2_WORLD_ART_LEGACY=1 H2_WORLD_ART_TAG=before \
  scripts/h2-cargo test -p wc3-render --lib world_art_hall_frame --offline -- --nocapture
```

`H2_WORLD_ART_LEGACY=1` restores the pre-fix light and drops the contact term, so
one binary renders the before frame and the after frame with everything else
identical.

## Frame

* Map: `(2)Justice.w3x` (melee), Orc seat — the player's main hall is `ogre`
  ("Great Hall", `Buildings\Orc\GreatHall\GreatHall.mdx`).
* Camera: 45° pitch at the building (the device RTS angle), eye 900 world units
  above and 780 in front of the hall, `720x720`, hour pinned to 12.0.
* Skinning: default GPU path. Lighting is a fragment-shader term, identical on
  the CPU path the Android build forces.
* Building mask: the frame is rendered twice (world pass with, then without the
  unit/doodad pass); the building is the largest 4-connected difference component
  inside the camera-focus window, clamped to the model's projected bounding box.
  The peons and the neighbouring hut never enter the numbers, and the box keeps the
  band limits in the same place for the before and the after frame (measured with
  the final metric code — earlier revisions of this file quoted numbers from an
  older mask-bbox metric, which are not comparable and have been replaced).

## Before (base renderer)

```
WORLD_ART before pixels=53213 roof_mean=0.2485 roof_spread=0.5404 roof_form=0.1833
  roof_panel=0.3036 roof_grad=0.1502 wall_mean=0.1506 wall_spread=0.4120
  wall_grad=0.1246 base_ratio=0.765 shadow=0.003 ground_contact=0.352 grass=0.0567
```

All luances are linear (`sRGB` decoded), so 0.2485 is roughly sRGB 137.

* `shadow = 0.003` — only 0.3 % of the ground ring around the building is darkened
  by the unit pass at all (203 px of 72 498, i.e. peon edges and dithering).
  **There is no ground shadow and no contact darkening.** This is the
  "pasted-on" cue.
* `base_ratio = 0.765` — the lowest 7 % of the silhouette against the 45..60 % band.
  The ratio comes from the palisade being dark wood; the renderer does not darken
  it.
* `roof_form = 0.1833`, `roof_panel = 0.3036` — mostly texture contrast between the
  pale shingles and the dark ribs.

## Vertex-level light distribution (texture-independent)

```
scripts/h2-cargo test -p wc3-render --lib great_hall_light_distribution -- --nocapture
```

```
WORLD_ART greathall meshes=19 verts=3654 y_only=337 up_facing=0.284
WORLD_ART sun=[1.3, 1.3, 1.3] ambient=[0.3, 0.3, 0.3] dir=(-0.675456, 0.615662, 0.405856)
WORLD_ART before clamped=0.392 mean=0.6719 spread=0.7000
WORLD_ART after  clamped=0.000 mean=0.5457 spread=0.5978
```

* The shipped Lordaeron **unit** DNC light as the curve delivers it is
  `sun = (1.3, 1.3, 1.3)`, `ambient = (0.3, 0.3, 0.3)` (the curve's per-term white
  balance normalises noon to exactly white).
* `ambient + sun · N·L` therefore passes 1.0 at `N·L = 0.538`, and **39.2 % of the
  Great Hall's 3654 vertices clip** to the same value. On the domed roof those are
  the panels: the whole visible roof is one clipped mass, which is why it reads as
  a flat pale shape.
* Every mesh of the model is affected, 26 % (the palisade, `stronghold.blp`) to
  54 % (`base.blp` trims) of its vertices.
* Unit test `roof_band_is_not_a_plateau` measures the same thing on a normal sweep:
  the legacy light puts ~46 % of the up-facing band (`n.y > 0.6`) on exactly one
  value.

## Root causes (file:line)

1. **No directional gradient on the sun side.**
   `crates/wc3-render/src/mesh.rs` `surface_light()` evaluated
   `clamp(ambient.rgb + sun.rgb * saturate(N·L), 0, 1)` with the raw DNC
   intensities. `sun 1.3` against `ambient 0.3` clips every surface within ~57° of
   the sun; a faceted dome has no shading left.
2. **No ground contact.** WC3 renders a soft shadow blob under every unit and
   building (engine geometry, not MDX data — `GreatHall.mdx` has no shadow
   geoset), and the base renderer had no such pass. The mesh path had no contact
   term either, so the bottom of every silhouette was lit exactly like the ground.
3. **Constant ambient.** Even without clipping, a single ambient colour gives an
   up-facing panel and the underside of the eaves the same fill; the shipped
   textures carry no baked AO, so the domed roof had nothing to read its form
   from.

Not a cause (checked, ruled out):

* Vertex colours: MDX geosets have none; the renderer writes white and applies the
  `GEOA`/`KGAC` grade through the draw tint (`[u16::MAX; 4]` in
  `push_static_vertices`), no double application.
* Team colour: `MeshLayer::team_color` only swaps in the unit tint for the
  team-colour layer; the roof/wood layers keep the geoset grade.
* Texture sampling: mips are generated in linear light, sRGB colour format,
  anisotropy 16; the device log shows `gpu_textures=730 gpu_levels=5498`, i.e. raw
  RGBA with mip chains (no ASTC blur on the device).
* Fog: the hall sits inside `ATMOSPHERE_START` (1200) of the 1100-unit close-up.
* Day/night clock drift: both frames of every pair are pinned with
  `set_time_of_day(12.0)` immediately before rendering (without it the first
  frame's delta includes the whole asset upload ≈ 1.5 game hours and the pair is lit
  differently).
