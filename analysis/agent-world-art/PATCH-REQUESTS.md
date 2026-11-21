# PATCH REQUESTS — files outside packet `B-world` ownership

Owner path for this work: `crates/wc3-render/src/mesh.rs`, `crates/wc3-render/src/texture.rs`,
`analysis/agent-world-art/**`. The items below need a change in a READ_ONLY file; each one is
measured, not speculative, and comes with the exact edit.

## 1. Terrain shader: same highlight shoulder (`crates/wc3-game/src/terrain_render.rs`)

The mesh shader now compresses its highlight above `LIGHT_KNEE = 0.72`
(`wc3-render/src/mesh.rs`). The terrain shader still evaluates
`clamp(ambient + sun · saturate(N·L), 0, 1)` with the raw terrain DNC light
(`sun 1.3`, `ambient 0.2`), which clips at `N·L = 0.615`. Flat ground at noon sits at
`0.191 + 0.797 = 0.988`, i.e. one percent below the clip, so any slope facing the sun is a
flat white shape. Same remedy, ~10 lines:

```wgsl
// terrain fragment shader, replacing the clamp
fn shoulder(value: vec3<f32>, knee: f32) -> vec3<f32> { … }   // identical to mesh.rs
let lit = shoulder(ambient + sun * ndl, u_light_knee);
```

Consistency matters: with models and terrain on different tonal curves, a unit standing on a
sun-facing slope would be visibly darker than the ground it stands on.

## 2. Doodads: contact band + ground shadow (`crates/wc3-game/src/terrain_render.rs`)

`build_doodad_geometry` emits `StaticDrawSpec`s, and `create_static_batch` builds each
`DrawParams` with `DrawParams::new(...)` — contact disabled. Every tree/rock doodad therefore
keeps the old pasted-on look while units and buildings get grounded.

Two options, smallest first:

* add `contact: [f32; 4]` to `StaticDrawSpec` and call `.with_contact(base_y, band, strength)`
  in `create_static_batch` when `contact[3] > 0`;
* or, better for trees, route doodads through `MeshRenderer::push_ground_shadow` (it is
  `fn`-private today; make it `pub` and pass the doodad's world bounds).

## 3. Terrain shimmer at 45° (`crates/wc3-game/src/terrain_render.rs`)

`crop-trees.png` shows achromatic moiré on the grass at ~45°. Measured: `TERRAIN_TILE_SIZE =
128` world units for a 64×64 tile, i.e. 2 world units per texel. On the device camera
(distance ≈ 1500, 1080p, 45° pitch) one world unit is ≈ 0.55 px at the target, so one texel is
≈ 1.1 px *along* the view but the grazing footprint is several texels across; anisotropy 16
(already set) cannot compensate a 45° footprint of that size, so the far half of every tile
samples between mip 0 and mip 1 and the diagonal pattern repeats.

Not a filtering bug: the sampler and the linear-light mip chain in
`wc3-render/src/texture.rs:269` are correct, and the device log
(`gpu_textures=730 gpu_levels=5498`) confirms mip chains are uploaded. The lever is texel
density — either tile UV repeat 0.5× (a 128×128 world footprint per tile) or a second detail
octave. Both are terrain-side.

## 4. Nothing needed in `render.rs`

`set_surface_light` receives `sample.unit` correctly; `sun.w` is now the shoulder knee and is
set by `MeshRenderer::set_surface_light` itself, so the 1.0 the game passes there is ignored
(the camera uniform layout is unchanged at 176 B).
