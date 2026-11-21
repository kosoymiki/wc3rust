# READ_ONLY patches for the tech-lead (packet A / ledger 480)

Two files that this packet marks READ_ONLY still carry the weakness that let the
112-vs-176 camera mismatch reach a draw call. I did **not** edit them. Exact
patches below; both are one-liner-shaped and were compile-checked by reading the
call sites (every binding of these layouts binds a full `size_of::<T>()` slice,
so pinning the minimum cannot reject a legitimate bind).

## 1. `crates/wc3-render/src/mesh.rs` — pin the mesh camera/draw minimums

`bind_group_layout_uniform` (`mesh.rs:2133`) hardcodes `min_binding_size: None`,
so a mesh camera that disagrees with `MESH_SHADER`'s `Camera` is only caught at
the first draw ("bound with size N where the shader expects M" → SIGABRT, the
`analysis/crash-20261117.txt` failure mode) instead of at bind-group creation
(`BindingSizeTooSmall`, recoverable and named — wgpu-core
`src/device/resource.rs:3084-3092`).

```diff
 fn bind_group_layout_uniform(
     device: &wgpu::Device,
     label: &str,
     dynamic: bool,
+    min_binding_size: Option<wgpu::BufferSize>,
 ) -> wgpu::BindGroupLayout {
     device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
         label: Some(label),
         entries: &[wgpu::BindGroupLayoutEntry {
             binding: 0,
             visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
             ty: wgpu::BindingType::Buffer {
                 ty: wgpu::BufferBindingType::Uniform,
                 has_dynamic_offset: dynamic,
-                min_binding_size: None,
+                min_binding_size,
             },
             count: None,
         }],
     })
 }
```

and the two call sites (`mesh.rs:787-788`):

```diff
-        let camera_layout = bind_group_layout_uniform(device, "mesh-camera-layout", false);
-        let draw_layout = bind_group_layout_uniform(device, "mesh-draw-layout", true);
+        let camera_layout = bind_group_layout_uniform(
+            device,
+            "mesh-camera-layout",
+            false,
+            wgpu::BufferSize::new(CAMERA_UNIFORM_SIZE as u64),
+        );
+        let draw_layout = bind_group_layout_uniform(
+            device,
+            "mesh-draw-layout",
+            true,
+            wgpu::BufferSize::new(std::mem::size_of::<DrawParams>() as u64),
+        );
```

Safe because: `mesh-camera-bind` (`mesh.rs:984`) binds
`camera_buffer.as_entire_binding()` on a `size_of::<CameraUniform>()` = 176 B
buffer; every `draw_layout` user binds `BufferSize::new(size_of::<DrawParams>())`
= 96 B (`mesh.rs:1035` draw ring, `mesh.rs:1765` static batch).

## 2. `crates/wc3-game/src/terrain_render.rs` — assert the terrain `Camera`

`TERRAIN_SHADER` (`terrain_render.rs:816`) declares the 176-byte `Camera` that
the game camera uniform feeds, and nothing pins it: the const asserts live in
`mesh.rs` (its own copy) and `render.rs` used to compare against
`mesh::CAMERA_UNIFORM_SIZE`, a proxy. `render.rs` now also cross-checks this
shader at `Renderer::new` and fails with `camera uniform mismatch: …` before a
bind group exists, but a compile-time/test-level pin belongs next to the shader:

```diff
 @group(0) @binding(0) var<uniform> camera: Camera;
```

```rust
// next to TERRAIN_SHADER, after the literal:
pub const TERRAIN_CAMERA_UNIFORM_SIZE: usize = 176;
```

plus, in the `#[cfg(test)] mod tests` of `wc3-game`:

```rust
#[test]
fn terrain_camera_uniform_is_176() {
    assert_eq!(
        wc3_render::mesh::wgsl_struct_size(crate::terrain_render::TERRAIN_SHADER, "Camera"),
        crate::terrain_render::TERRAIN_CAMERA_UNIFORM_SIZE
    );
}
```

(Equivalent coverage already exists in `crates/wc3-game/src/render.rs`
`camera_uniform_matches_both_shaders`, which compares `CAMERA_UNIFORM_BYTES`
with this exact WGSL struct — so this patch is defence in depth, not a blocker.)

## 3. Not a patch, a finding: `wc3-render/src/frame.rs::UniformRing` is dead code

`UniformRing` (`frame.rs:202`) is constructed nowhere in `crates/**` or
`apps/**`; its growth path re-creates the buffer with the hardcoded label
`"perf-uniform-ring"` regardless of the ring's purpose. I left it untouched
(a label change would require a new field on a type nothing uses). Either wire
it up or delete it — a dead per-frame ring is a trap for the next perf pass.

## 4. Harness note: `crates/wc3-game/src/render.rs` was edited by two packets

`render.rs` (this packet's OWNER path) was last written at **11:25** by the
lighting work (176-byte `CameraUniform`, `GAME_CAMERA_UNIFORM` log) before this
packet started; my edits (11:5x) build on top of that state. No revert, no
conflict: `scripts/h2-cargo check -p herofolends-android` is green and
`cargo test -p wc3-render --lib` is 109/109.
