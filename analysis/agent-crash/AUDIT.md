# AUDIT — every `@group(0) @binding(0)` buffer in the HeroFolends frame

Ledger 480, packet A (`analysis/tech-lead/packets/20261117/A-crash.md`).
Auditor: crash-hunter. Date 2026-11-17. Source snapshot mtimes recorded below.

Method: `rg` over `crates/**` + `apps/**` for `create_bind_group`, `set_bind_group(0`,
`@group(0) @binding(0)` and `var<uniform>`; each group-0 binding is then traced to
(a) the `BufferDescriptor` that creates its buffer and (b) the WGSL struct the
shader module declares for that binding. Sizes are the ones the compiler computes
(`std::mem::size_of`, pinned by the `const _: () = assert!` blocks at
`crates/wc3-render/src/mesh.rs:313` and `crates/wc3-game/src/render.rs:78`).

## Table — pass → pipeline → group 0 / binding 0 buffer

| # | pass (render pass label) | pipeline / shader | group-0 bind group created at | buffer (label) created at | buffer size | WGSL struct | WGSL size | min_binding_size |
|---|---|---|---|---|---|---|---|---|
| 1 | `game-world-pass` (terrain: `draw_terrain`, `draw_cliffs`) | `terrain-pipeline-*` / `TERRAIN_SHADER` (`terrain_render.rs:816`) | `render.rs:224` (`game-camera-bind`) | `render.rs:207` (`game-camera-buffer`) | **176** | `Camera` (`terrain_render.rs:818`) | **176** | `Some(176)` (`render.rs:200`) |
| 2 | `game-world-pass` (terrain: `draw_water`, `draw_shore`) | `terrain-pipeline-*` / `TERRAIN_SHADER` | `render.rs:224`, re-set at `render.rs:725` | `render.rs:207` | **176** | `Camera` | **176** | `Some(176)` |
| 3 | `game-world-pass` (units, doodads, rings via `MeshRenderer`) | `mesh-opaque` … `mesh-cutout` / `MESH_SHADER` (`mesh.rs:420`) | `mesh.rs:984` (`mesh-camera-bind`) | `mesh.rs:978` (`mesh-camera`) | **176** (`size_of::<CameraUniform>()`) | `Camera` (`mesh.rs:421`) | **176** | `None` (`bind_group_layout_uniform`, `mesh.rs:2146`) — **READ_ONLY, patch reported** |
| 4 | particles (no pass of its own; drawn by callers) | `particles-pipeline` / `PARTICLES_SHADER` (`particles.rs:324`) | `particles.rs:752` (`particles-camera-bind`) | `particles.rs:746` (`particles-camera`) | **128** | `Camera` (`particles.rs:325`) | **128** | `Some(128)` (`particles.rs:624`) |
| 5 | `game-fog-pass` | `fog-pipeline` / `FOG_SHADER` (`fog.rs:133`) | `fog.rs:370` (`fog-bind`) | `fog.rs:285` (`fog-uniform`) | **96** | `FogParams` (`fog.rs:134`) | **96** | `None` (`fog.rs:216`) → **hardened in this pass to `Some(96)`** |
| 6 | `game-ui-pass` | `ui-pipeline` / `UI_SHADER` (`ui.rs:14`) | `ui.rs:177` (`ui-viewport-bind`) | `ui.rs:171` (`ui-viewport`) | **16** | `Viewport` (`ui.rs:15`) | **16** | `None` (`ui.rs:103`) → **hardened in this pass to `Some(16)`** |

Group-0 layouts that are **not** buffers (excluded by construction): none of the
six group-0 layouts declares a texture or sampler at binding 0.

Not at group 0, cross-checked so they cannot be confused with the camera:

| buffer | binding | size | note |
|---|---|---|---|
| `mesh-draw-ring` (`mesh.rs:1021`) | group 2 / binding 0 | slice of **96** (`DrawParams`) | dynamic offset, `size: BufferSize::new(96)` |
| `mesh-palette` (`mesh.rs:1040`) | group 3 / binding 0 | 1 MiB | storage, not uniform |
| `terrain-params-buffer` (`terrain_render.rs:2484`) | group 1 / binding 2 | **16** (`TerrainParamsUniform`) | `water: vec4` |
| `particles-params` (`particles.rs:761`) | group 1 / binding 0 | `EMITTER_PARAMS_STRIDE` slots | dynamic offset |

## What the crash message pins down

`112 B = mat4x4<f32> (64) + 3 × vec4<f32> (48)`. In this tree the **only**
group-0/binding-0 consumer that expects 176 B is the `Camera` of the terrain
shader (rows 1–2) and of the mesh shader (row 3). A 112-byte producer therefore
cannot be fog (96), UI (16), particles (128) or terrain params (16): those all
report their own, different sizes.

The 112-byte shape is the **pre-lighting game camera**: `view_proj, light, sun,
ambient` — exactly the four fields the game-side `CameraUniform`
(`crates/wc3-game/src/render.rs:55`) had before `day`, `fog`, `fog_range` and
`eye` were appended to make it match the shaders' 176-byte `Camera`.
`crates/wc3-game/src/render.rs` was last written at **11:25** and
`crates/wc3-render/src/mesh.rs`/`fog.rs` at **11:28**, i.e. *after* both recorded
aborts (11:13:54 and 11:20:35) and after the APK that is installed right now was
built (**11:18**). The WGSL side had already grown to 176 B in that build.

### The offending pipeline: terrain, first draw of the first world frame

Order inside `Renderer::render_with` (`render.rs:672-744`):

1. `set_bind_group(0, &self.camera_bind)` — the game camera (`render.rs:708`),
2. `terrain.draw_terrain` / `draw_cliffs` (`render.rs:711-712`) — **first draw
   that consumes group 0**. With a 112-byte game camera and a 176-byte terrain
   `Camera` this is the first validation failure in the frame,
3. only later `mesh.draw_group` swaps in the mesh renderer's own 176-byte camera.

Live confirmation from the installed (11:18) build, `analysis/device-qa-20261117/logs/launch-01.log`:

```
11-17 11:39:12.411 I Herofolends: GAME_TIME day=1 hour=00.00     <- first world frame
11-17 11:39:12.412 I Herofolends: GAME_SKIN mode=cpu ... draws=49
11-17 11:39:12.423 F libc    : Fatal signal 6 (SIGABRT) ... (SDLThread)
11-17 11:39:13.101 F DEBUG   : In bind group index 0, the buffer bound at
                               binding index 0 is bound with size 112 where the
                               shader expects 176.
```

`GAME_CAMERA_UNIFORM` occurs **0 times** in that build's log; the line was added
together with the 176-byte fix at 11:25.

Timeline of that launch (`apps/android-native/src/lib.rs`): `Renderer::new` is
called once, at `lib.rs:1303`, after the assets are decoded; the single render
call site is `lib.rs:1712`:

```
11:39:03.816  P0_SURFACE configured adapter Adreno (TM) 730        (device + swapchain)
11:39:09.607  GAME_READY ... models=93 textures=800                (assets decoded)
11:39:09.652  GAME_SCENE_CENSUS ... unit_placed=88                  (sim ready)
11:39:12.411  GAME_TIME day=1 hour=00.00                            (first frame, day/night sampled)
11:39:12.412  GAME_SKIN mode=cpu ... draws=49                       (first prepared frame)
11:39:12.423  F libc : Fatal signal 6 (SIGABRT) ... (SDLThread)
```

`GAME_SKIN` is emitted once per process (`skin_logged`) from inside
`render_with`, ~10 lines before the encoder is created. So the abort is the
**first frame the renderer ever recorded** — the ~3 s between `GAME_READY` and
`GAME_TIME` is `Renderer::new` uploading the 800 textures, not frames.

**Not intermittent — build-dependent.** The three aborts (11:13:54 pid 24928,
11:20:35 pid 30635 in install dir `…iFU4tA…`, 11:39:12 pid 13189) come from
*different builds* of the same refactor window; each one aborts on the first
rendered frame, deterministically. The 11:39 launch is the APK built at 11:18
that is installed right now, i.e. a third independent confirmation, not a coin
flip. The "same APK ran fine with `tick=684`" run predates the WGSL edit (see
`EVIDENCE.md`, section "intermittency").

## Gaps this audit closes (root cause is the *class*, not the one number)

The 176-byte contract was protected in `mesh.rs` only, against its own WGSL copy,
and `render.rs` asserted against `mesh::CAMERA_UNIFORM_SIZE` — a proxy, not
against the `TERRAIN_SHADER` that actually consumes the game camera. Terrain
could drift again with every check still green. Fixed in this pass (owner paths):

1. `render.rs` — runtime cross-check of `CAMERA_UNIFORM_BYTES` against
   `wgsl_struct_size(TERRAIN_SHADER, "Camera")`, before the bind group is
   created; mismatch returns `Err` with both sizes instead of aborting later.
2. `render.rs` — startup line `GAME_CAMERA_UNIFORM` now carries the game bytes,
   the buffer bytes, the mesh-WGSL bytes and the terrain-WGSL bytes.
3. `render.rs` — per-pass `push_error_scope(Validation)` / `pop()` +
   `on_uncaptured_error` handler: `P0_PASS_ERROR pass=<label> source=… err=…`.
4. `particles.rs`, `fog.rs`, `ui.rs` — `min_binding_size` pinned to the Rust
   struct (early, named failure) + tests that compare the Rust struct with the
   WGSL struct the shader module really declares.
5. `mesh.rs` — `min_binding_size: None` on the camera layout is in a READ_ONLY
   file: exact patch in `READONLY_PATCHES.md` for the tech-lead.

## Result of the audit (see `EVIDENCE.md` for the outputs)

* The shipped 11:18 `libmain.so` was disassembled: `Renderer::new` creates
  `game-camera-buffer` with `{size: 112, usage: 0x48}` and the bind group binds
  `BufferBinding{size: 112}` — the exact number in the abort. The 12:00 build of
  the same function creates it with `{size: 176, usage: 0x48}`.
* Five cold starts of the 12:00 APK (`sha256 82a8b672…`) each printed
  `GAME_CAMERA_UNIFORM bytes=176 buffer_bytes=176 mesh_wgsl=176 terrain_wgsl=176`,
  then `GAME_SKIN`, `GAME_RENDER_SCALE swapchain=2414x1080 …` and
  `GAME_FIRST_FRAME surface=2414x1080 … units=13`, with zero `P0_PASS_ERROR` and
  zero FATAL in the window.
* `cargo test -p wc3-render --lib`: 110 passed / 0 failed.
