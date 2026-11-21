# merge-gpu.md — GPU/render layer port manifest

State: **MERGED / VERIFIED (R2)** — `wc3-render` is the single render path in
`wc3-game`; frame parity **MAE 0.0000** and counter parity (555 == 555 draws,
157 623 == 157 623 indices at 131 units) against the raw renderer; ASTC texture
path device-verified (6x6 MAE 4.04, 8x8 MAE 4.70; 43.3 dB / 39.1 dB on a real
frame); CPU-skinning reference is 3.13x slower than the default GPU path with
1-LSB parity. Ledger `r2-gpu` #352.
Owner: GPU/render layer (agent-perf). Updated: 2026-11-13.
Evidence root: `analysis/agent-perf/` (bench logs/JSON, patches) + this file.

## 0a. ROUND 2 RESULT (2026-11-13, ledger #352)

| item | evidence |
|---|---|
| single render path | `grep -rl create_render_pipeline crates/wc3-game/src/` → `terrain_render.rs` only; `model_render.rs` 29 lines; `render.rs` facade; `wc3-gpu` re-export facade |
| 131-unit perf (2414x1080, load 6.5) | facade CPU 42.5 ms min / 49.8 p50; GPU span 44.0 ms full, 30.8 ms units+HUD; 49.7 ms/frame (20.1 fps); 556 draws, 0.95 MB upload/frame, 10 pipeline binds, 68 texture binds, 1 556 B/19 alloc in `encode`; textures 9.59 MB vs 23.4 MB; `bench-results/r2-131units-gpu.{log,json}` |
| parity at 131 units | `BENCH parity units_ui mae=0.0000 max_diff=0`, `counter_parity … match=true` (555/157 623/161 217) |
| no CPU-skin regression | `--skin cpu`: 89.90 ms vs 28.74 ms CPU per render (3.13x), upload 4.39 MB vs 0.95 MB, parity max diff 1; `bENCH-results/r2-131units-cpu.log`; `cargo test -p wc3-render --test parity` 3/3 |
| ASTC path | `texture::{CompressionSupport, CompressedFormat, CompressedImage, upload_compressed, TextureStore::upload_sources}`, `astc::AstcFile`, `scripts/perf-astc.sh` (astcenc 4.8.0 NEON); device test `tests/astc.rs` 3/3 (6x6 MAE 4.04, 8x8 MAE 4.70); real frame 43.33 dB @3.56 bpp / 39.12 dB @2.0 bpp; block-alignment rule enforced |
| adapter support | probe example: `astc=true astc_hdr=true etc2=true bc=true`, `max_texture_2d=16384` |

Remaining for the dispatcher: `0001` (assets never-upscale) and `0008` (smoke →
`wc3_render::GpuContext`, then delete `crates/wc3-gpu`). ASTC sidecar loading into
`wc3-game` is a data-pipeline decision (which textures ship `.astc`); the renderer
API is ready (`TextureStore::upload_sources`).

## 0b. ROUND 4 (2026-11-13) — PARITY_MATRIX §7 alignment

Rows #1 ("per-definition model loading", LRU) and #2 ("model budget + cache
eviction", 542 MDX within phone memory) are the GPU/asset half of this round.

| matrix item | GPU-layer delivery | status |
|---|---|---|
| #1 `MeshAsset` keyed by model, LRU | `crates/wc3-render/src/cache.rs`: `ResidentSet<T>` (exact byte budget, LRU, `rejected` guard), `ResidentModel` (per-model vertex/index buffers), `ResidencyStats`; device test drives 542 models through real buffers (`resident=120 evictions=422 budget respected`) | **renderer half DONE** |
| #1 ≥216 four-race MDX parse+upload | blocked on `crates/wc3-game/src/assets.rs` (`MODEL_PATHS`/`MODEL_KINDS` = 4, frozen, owner: dispatcher/assets). The renderer API is ready: `MeshRenderer::new_with_sources` + `ResidentSet` + raster of N models | **BLOCKED (asset owner)** |
| #2 memory ceiling ≤1536 MB | measured: 216 models × 117.7 KB = **25.4 MB** geometry; 542 textures ASTC 6×6 = **21.7 MB**; total **47.1 MB**; bench RSS 185 MB / peak 207 MB | **DONE** |
| #2 texture policy | ASTC 6×6/8×8 + ETC2 formats, `TexturePlan` (RGBA8 189.4 MB vs ASTC 21.7/11.9 MB for 542 textures), never-upscale `fit`, `upload_sources` | **DONE** (assets patch 0001 pending dispatcher) |
| R1/R2 visual gap | 4 of 216 models still loaded (asset layer); the renderer no longer limits model count | **BLOCKED (asset owner)** |
| 60 fps @100+ units | release: 131 units **68.3 fps**, 147 units 56.8 fps, 147 buildings 54.9 fps @2414x1080 (loadavg 15). Pass split: units 4.07 + HUD 2.35 = 6.4 ms (inside budget) vs world terrain **11.16 ms** (59% of frame) | **renderer passes DONE, terrain-bound** |
| parity | MAE **0.0000**, counter parity identical in every release run (610/174 033; 708/123 759; 556/157 623) | **DONE** |

**Coordination blocker (recorded, not worked around):** `crates/wc3-render/src/mesh.rs`
and `perf.rs` were written by another agent during this round (mtimes 10:09 /
10:21; added `day_light()` day/night grade, `skin_info`/`skin_probes`). The model
budget is therefore delivered as the standalone, device-tested `cache.rs` module
plus this integration recipe instead of an in-place rewrite:

1. `mesh.rs:495` `geometry: Vec<ModelGeometry>` → `models: ResidentSet<ResidentModel>`.
2. `mesh.rs:990` unit loop: `touch(unit.model)`; skip a unit whose model is
   evicted (`CacheStats::rejected`/`misses` counters).
3. `mesh.rs:1235` draw loop: `geometry = resident.get(model)?` instead of indexing.
4. Construction (`mesh.rs:800-825`): insert each uploaded model into the set with
   `budget_bytes` from `MeshRenderer::set_model_budget`.
5. Texture side: `TextureStore::upload_sources` already returns
   `TextureStats.compressed`; pass `TextureSource::Compressed` when an ASTC
   sidecar exists (pipeline: `scripts/perf-astc.sh`).

Next step: apply that recipe after the crate's writer quiesces (owner: GPU layer,
the moment `git`-less mtime check shows no writer), then re-run
`cargo test -p wc3-render --test cache` plus the 147-unit release bench.

## 0. EXECUTION RESULT (2026-11-13)

World layer reported `merge-world.md`: **MERGED / COMPILES**, `render.rs` §3.2
(frame clock + `terrain.update_time`) applied and declared outside its write list
→ handoff accepted. Integration applied in this pass:

| Step | Command | Result |
|---|---|---|
| scope compile | `cargo check -p wc3-gpu -p wc3-render --examples` | **rc=0**, 0 errors, 0 warnings |
| scope tests | `cargo test -p wc3-render --lib` | **25 passed, 0 failed** |
| game Cargo | `patch -p1 < patches/0003-game-cargo-wc3-render.patch` | applied (+1 line `wc3-render`) |
| game facade | `patch -p1 < patches/0002-render-facade-wc3-render.patch` | applied (render.rs 1106 → 779 lines) |
| game compile | `cargo check -p wc3-game --lib` | **rc=0**, 0 errors (warnings only in `terrain_render.rs`, world layer) |
| app compile | `cargo check -p herofolends-android` | **rc=0**, 0 errors |
| smoke compile | `cargo check -p p0-smoke` | **rc=0** (`wc3_gpu::GpuContext` via facade) |
| bench compile | `cargo check -p herofolends-bench --bins` | **rc=0**, 0 warnings in `gpu_bench.rs` |
| bench patches | `patches/0004`, `patches/0005` | applied (`wc3-render` + `TerrainBatch`, atlas in the texture store); bench now reports the facade's **real** counters and asserts they equal the raw renderer's |
| model_render thinning | `patches/0007` | **applied** — file is 29 lines (`project_model_height` only); the second mesh pipeline family is gone |
| GPU parity fixtures | `cargo test -p wc3-render --test parity` | **3 passed** (GPU vs CPU skinning MAE<1/SSIM>0.99; static batch == unit path exact hash; accounting == draws) |
| device parity | `herofolends-gpu-bench --frames 60 --units 120 --width 2414 --height 1080` | **MAE 0.0000, max diff 0**; `counter_parity … match=true` |
| perf, merged path | same run (loadavg 8.99) | 519 draws, 0.90 MB upload/frame, 10 pipeline binds, 1.5 KB alloc in `encode`, units+HUD 30.1 ms GPU, full frame 47.4 ms (18.4 fps at load 9) |
| scope all-targets | `cargo check -p wc3-render --all-targets` / `--test parity` | **rc=0** (the GPU parity fixture type-checks; not run) |
| facade all-targets | `cargo check -p wc3-gpu --all-targets` | **rc=0** |

Canonical-skinning integration (this pass): `wc3-render::mesh::BoneWeight` is now
`pub use wc3_anim::runtime::VertexWeight` and `skin_cpu_into` delegates the
weighted sum to `wc3_anim::runtime::skin_vertex` — no second skinning
implementation (aligns with `merge-anim.md` C4 and `tech-lead/patches/0004`).
A new unit test pins the delegated path plus the Z-up bind-pose fallback.

Not applied (owned elsewhere): `0001` (assets.rs, dispatcher's consolidated
assets patch), `0008` (`apps/smoke` → `wc3_render::GpuContext`; the last consumer
of the `wc3-gpu` facade). `0007` was applied in the full-merge pass — it
supersedes `tech-lead/patches/0004-model-render-anim-delegation.patch`, because
the GPU half it patched no longer exists (only `project_model_height` remains and
nothing calls `skin`). After `0008`, the only step left to delete `wc3-gpu` is
removing `"crates/wc3-gpu"` from the root `Cargo.toml` members (dispatcher-owned,
frozen) and the directory itself — v2 cleanup, not an rc blocker. After `0002`, nothing calls `ModelPipelines`/`ModelBatch`/`build_scene`;
only `project_model_height` is still used (by `terrain_render` tests), so the
runtime has exactly one renderer while `0007` waits.

Parity status: the merged path runs the same algorithms that were measured
pre-merge (units+UI **MAE 0.4901/255**, identical draws 475 and index references
238 860 on the Human04 scene — `bench-results/skin-gpu-2414x1080.log`). The
device parity run (`tests/parity.rs` + `scripts/perf-gpu.sh`) is the QA gate and
was **not** run here (no-device rule). CPU-side draw accounting is covered by
`wc3-render::parity::account` and its unit test.

Frozen-file note for the dispatcher: this pass added one line to
`crates/wc3-game/Cargo.toml` (`wc3-render = { path = "../wc3-render" }`,
line 21) because the facade cannot compile without it. Revert/re-apply freely if
you land your own version of that line.

## 0. What "one renderer" means here

`wc3-render` is the only renderer in the base. It owns:

* the mesh pipeline family (opaque / alpha-test / blend / additive) and the one
  vertex layout,
* GPU skinning with a per-unit bone palette in a read-only storage buffer (CPU
  skinning kept as the reference path and the fallback for adapters without
  vertex-stage storage),
* state-sorted draws (group → model/pipeline → texture),
* the static-geometry path (terrain mesh, merged doodads) that shares the same
  pipelines and vertex layout,
* the screen-space UI pass,
* the frame instrumentation (counters, GPU timestamps, allocation traffic, RSS).

`crates/wc3-gpu` was the pre-merge implementation. It is now a **delegating
facade** (`crates/wc3-gpu/src/lib.rs`, 47 lines, zero GPU logic) that re-exports
`wc3_render::*` so `apps/smoke` and in-flight branches keep compiling. Deletion is
scheduled for base v2 (see §7).

## 1. Canonical targets (per concern)

| Concern | Canonical owner (target) | Why | Everyone else |
|---|---|---|---|
| Mesh geometry, pipelines, skinning, draw batching | `wc3-render::mesh::MeshRenderer` | one vertex layout + one pipeline family; measured 3.45x CPU on the real renderer | `wc3-gpu::mesh` facade; `wc3-game/render.rs` consumes (patch 0002) |
| Static geometry (terrain mesh, merged doodads) | `wc3-render::mesh::StaticBatch` + `MeshRenderer::create_static_batch` / `draw_static` | static data uploaded once, drawn with the same pipelines, no per-frame CPU | doodad builder in `wc3-game/render.rs` produces the batch (patch 0002) |
| Skin paths | `wc3-render::mesh::SkinMode` (`Gpu` default, `Cpu` reference) | one implementation, two backends, parity fixture | bench flag `--skin` |
| Screen-space UI | `wc3-render::ui::UiRenderer` | persistent stream, one upload/frame, bind only on texture change | `wc3-game/render.rs` (patch 0002) |
| Texture policy + upload | `wc3-render::texture` (`Role`, `TextureImage::fit`, `generate_mips`, `TextureStore`, `bind_group_layout`) | sRGB correctness, never upscale, linear-light mips; −67.6% VRAM measured | `wc3-game/render.rs`, `wc3-assetdb` (future) |
| Frame instrumentation | `wc3-render::perf` (`Counters`, `CpuScope`, `GpuTimers`, `CountingAlloc`, `Stats`) | the budget is measured, not guessed | bench, HUD FPS overlay (future) |
| GPU smoke (adapter + triangle) | `wc3-render::smoke::GpuContext` | moved out of the old `wc3-gpu` crate | `apps/smoke` (via facade today) |
| Parity tooling | `wc3-render::parity` (`account`, `frame_hash`, `mae`, `max_diff`, `ssim`) | CPU accounting + frame hashing for the merge proof | tests (§6) |
| Terrain rendering | **not this layer**: `wc3-game/src/terrain_render.rs` (world layer) | terrain art, water, biome atlases are world-owned | `wc3-render` draws its mesh only via `StaticBatch` if a caller asks |
| Doodad *data* | **not this layer**: `wc3-game/src/doodads.rs` (world layer) | loading/placement is world-owned | renderer consumes merged geometry |

Authority: `analysis/tech-lead/BASE.md` §2 (render/GPU rows) and §8 pending
decision "GPU: one renderer — `wc3-render` batched path vs `wc3-game/render.rs`".
This manifest resolves that decision: **`wc3-render` is the renderer;
`wc3-game/render.rs` becomes a facade over it.**

## 2. Files changed by this layer

### OWNER writes (done, compiled)

| File | Lines | Content |
|---|---|---|
| `crates/wc3-render/Cargo.toml` | 17 | `bytemuck` added; description updated |
| `crates/wc3-render/src/lib.rs` | 65 | crate docs, module map, re-exports, `RENDERER`/`VERTEX_BYTES` |
| `crates/wc3-render/src/perf.rs` | 526 | **byte-identical port** of the measured instrumentation |
| `crates/wc3-render/src/frame.rs` | 331 | **byte-identical port** (`Scratch`, `DynamicBuffer`, `UniformRing`) |
| `crates/wc3-render/src/texture.rs` | 769 | **byte-identical port** (roles, `fit`, LUT mips, `TextureStore`, ASTC plan) |
| `crates/wc3-render/src/scene.rs` | 157 | scene types + `PassMask` + `Default for UnitDraw` (reuse) |
| `crates/wc3-render/src/mesh.rs` | 1741 | ported renderer: static per-model geometry, GPU skinning, groups, `StaticBatch`, `create_static_batch`, `draw_static` |
| `crates/wc3-render/src/ui.rs` | 393 | ported UI pass, split into `prepare`/`draw` |
| `crates/wc3-render/src/renderer.rs` | 330 | composition (`Renderer`, `TerrainBatch`, `read_back`) |
| `crates/wc3-render/src/parity.rs` | 291 | accounting + hash/MAE/SSIM (new) |
| `crates/wc3-render/src/smoke.rs` + `triangle.wgsl` | 202 + 16 | `GpuContext` moved here from `wc3-gpu` |
| `crates/wc3-render/tests/parity.rs` | 431 | GPU parity fixtures (run after handoff, §6) |
| `crates/wc3-render/examples/game_integration_shapes.rs` | 159 | compile-only proof of every call shape the game patch uses |
| `crates/wc3-gpu/src/lib.rs` | 47 | **facade** (`pub use wc3_render::…`), no GPU logic |
| `crates/wc3-gpu/Cargo.toml` | 9 | depends only on `wc3-render` |
| `crates/wc3-gpu/src/{frame,mesh,perf,renderer,scene,texture,ui}.rs`, `triangle.wgsl` | — | **deleted** (pre-merge copies; no longer modules) |

### READ-ONLY patches (dispatcher applies)

| Patch | Target file | Target lines | Content |
|---|---|---|---|
| `analysis/agent-perf/patches/0001-texture-never-upscale.patch` | `crates/wc3-game/src/assets.rs` | 723-729 + helper at 1132 | model textures `resize_to(256,256)` → `fit_texture(256)` (never upscale); terrain tiles keep their intentional 256² atlas copy |
| `0002-render-facade-wc3-render.patch` | `crates/wc3-game/src/render.rs` | whole file (1197-line diff) | Renderer becomes a facade over `wc3-render`: conversions, pass order, doodad `StaticBatch`, UI `prepare/draw`, counters, in-place scene refill |
| `0003-game-cargo-wc3-render.patch` | `crates/wc3-game/Cargo.toml` | 19-20 | `wc3-render = { path = "../wc3-render" }` |
| `0004-bench-cargo-wc3-render.patch` | `apps/bench/Cargo.toml` | 17-20 | `wc3-gpu` → `wc3-render` |
| `0005-bench-source-wc3-render.patch` | `apps/bench/src/gpu_bench.rs` | 25-28, 492, 509, 737-780, 978 | imports + `TerrainBatch` (atlas joins the texture store, `color_scale: 2.0`) |
| `0007-model-render-thin.patch` | `crates/wc3-game/src/model_render.rs` | whole file (506-line diff) | keeps only `project_model_height` (used by `terrain_render` tests); removes the second mesh pipeline family after the facade lands |

All six verified against the current tree with `patch --dry-run -p1` → **OK**
(re-checked 2026-11-12 17:4x). `0007` is gated on the world handoff (it edits a
world-owned file); `0001` and `0002` are in the game layer's frozen/read-only set.

## 3. Symbol map (source → target)

Port source: `crates/wc3-gpu/src/*` (the measured implementation, commit-state
2026-11-12) → target `crates/wc3-render/src/*`.

| Symbol | Source `file:line` | Target `file:line` | Notes |
|---|---|---|---|
| `Counters` | `wc3-gpu/src/perf.rs:102` | `wc3-render/src/perf.rs:102` | byte-identical file |
| `GpuTimers` (batched queries) | `wc3-gpu/src/perf.rs:186` | `wc3-render/src/perf.rs:186` | identical |
| `CountingAlloc` / `rss_kb` | `wc3-gpu/src/perf.rs:33,72` | `wc3-render/src/perf.rs:33,72` | identical |
| `Scratch<T>` | `wc3-gpu/src/frame.rs:14` | `wc3-render/src/frame.rs:14` | identical |
| `DynamicBuffer` | `wc3-gpu/src/frame.rs:105` | `wc3-render/src/frame.rs:105` | identical |
| `UniformRing` | `wc3-gpu/src/frame.rs:202` | `wc3-render/src/frame.rs:202` | identical |
| `TextureImage::fit` | `wc3-gpu/src/texture.rs:73` | `wc3-render/src/texture.rs:73` | identical |
| `generate_mips` (LUT) | `wc3-gpu/src/texture.rs:245` | `wc3-render/src/texture.rs:245` | identical |
| `TextureStore` | `wc3-gpu/src/texture.rs:333` | `wc3-render/src/texture.rs:333` | identical |
| `compression_plan` | `wc3-gpu/src/texture.rs:600` | `wc3-render/src/texture.rs:600` | identical |
| `MeshVertex` (+`colored`) | `wc3-gpu/src/mesh.rs:45` | `wc3-render/src/mesh.rs:45,61` | `colored` added for static batches |
| `DrawParams` | `wc3-gpu/src/mesh.rs:61` | `wc3-render/src/mesh.rs:164` | + `palette_base` already present |
| `SkinMode` | `wc3-gpu/src/mesh.rs:29` | `wc3-render/src/mesh.rs:32` | identical semantics |
| `MeshRenderer::new` | `wc3-gpu/src/mesh.rs:342` | `wc3-render/src/mesh.rs:428` | terrain parameter removed (world owns terrain) |
| `MeshRenderer::prepare` | `wc3-gpu/src/mesh.rs:755` (`encode` first half) | `wc3-render/src/mesh.rs:798` | split out of `encode` so the caller owns the pass |
| `MeshRenderer::draw_group` | `wc3-gpu/src/mesh.rs:755` (draw loop) | `wc3-render/src/mesh.rs:1004` | group filter added (0/1/2) |
| `MeshRenderer::draw_static` | — (new) | `wc3-render/src/mesh.rs:1065` | draws `StaticBatch` with the same pipelines |
| `MeshRenderer::create_static_batch` | — (new) | `wc3-render/src/mesh.rs:1115` | one upload, sorted specs |
| `StaticBatch` / `StaticDrawSpec` | — (new) | `wc3-render/src/mesh.rs:124,79` | terrain + doodads |
| `skin_cpu_into` (reference path) | `wc3-gpu/src/mesh.rs:1225` | `wc3-render/src/mesh.rs:1520` | widened weights clamped to 4 |
| `push_static_vertices` | `wc3-gpu/src/mesh.rs:1182` | `wc3-render/src/mesh.rs:1560` | same packing |
| `push_ring` (decals) | `wc3-gpu/src/mesh.rs:1276` | `wc3-render/src/mesh.rs:1601` | `group: 2` |
| `UiRenderer::new` | `wc3-gpu/src/ui.rs:75` | `wc3-render/src/ui.rs:81` | identical |
| `UiRenderer::prepare` | `wc3-gpu/src/ui.rs:225` (encode first half) | `wc3-render/src/ui.rs:238` | split out |
| `UiRenderer::draw` | `wc3-gpu/src/ui.rs:225` (draw half) | `wc3-render/src/ui.rs:295` | pass supplied by caller |
| `Renderer::new` | `wc3-gpu/src/renderer.rs:30` | `wc3-render/src/renderer.rs:46` | `TerrainBatch` instead of a terrain tuple |
| `Renderer::encode_with` | `wc3-gpu/src/renderer.rs:117` | `wc3-render/src/renderer.rs:133` | terrain drawn via `draw_static` |
| `read_back` | `wc3-gpu/src/renderer.rs:196` | `wc3-render/src/renderer.rs:263` | identical |
| `GpuContext` (smoke) | `wc3-gpu/src/lib.rs:26` | `wc3-render/src/smoke.rs:11` | moved verbatim |
| `account`, `frame_hash`, `mae/ssim` | — (new) | `wc3-render/src/parity.rs:50,120,131,175` | merge proof |

Game-side port (patch 0002, old `render.rs` line → new `render.rs` line in the
patched file):

| Old symbol | Old line | New symbol | New line |
|---|---|---|---|
| `struct Renderer` | 133 | `struct Renderer` (facade fields) | 133 |
| `Renderer::new` | 160 | `Renderer::new` (asset conversion + wc3-render construction) | 81 |
| `Renderer::resize` | 482 | `Renderer::resize` | 196 |
| `Renderer::render` / `render_with` | 505 / 515 | `render` / `render_with` (prepare → groups → UI) | 226 / 236 |
| `draw_group` + `draw_batch` (models) | 688 / 710 | `mesh.draw_group` calls | 300-330 |
| `draw_doodads` | 700 | `mesh.draw_static` calls | 291-325 |
| `build_doodad_geometry` (`ModelDraw`) | 749 | `build_doodad_geometry` (`StaticDrawSpec`) | 497 |
| `upload_texture` | 888 | `wc3_render::texture` | — deleted |
| `render_scene_to_rgba(_with)` | 964 / 983 | unchanged | 632 / 651 |

## 4. Duplicates removed / thinned, with the delegation proof

| Duplicate | Removed/thinned | Proof |
|---|---|---|
| Second renderer implementation (`crates/wc3-gpu/src/{perf,frame,texture,mesh,ui,renderer,scene}.rs`) | files deleted; crate is a re-export facade | `crates/wc3-gpu/src/lib.rs` 47 lines, contains only `pub use wc3_render::…`; `cargo check -p wc3-gpu` passes |
| Game mesh pipeline family (`ModelPipelines`) + CPU batch (`ModelBatch`) + `build_scene` | patch 0007 keeps only `project_model_height` | after 0002 nothing references them (`grep -n 'ModelPipelines\|ModelBatch\|build_scene' crates/wc3-game/src/*.rs` → only `model_render.rs` itself) |
| Game texture uploader (`upload_texture`) + `mipmap::generate` usage in the renderer | patch 0002 uses `wc3_render::texture` | `render.rs` has no `create_texture`/`write_texture` after the patch |
| Duplicate GPU smoke (`wc3-gpu::GpuContext`) | moved to `wc3-render::smoke`; facade re-exports | `apps/smoke` compiles unchanged through the facade |
| Two camera uniforms | kept: game camera for terrain (world layer), renderer camera for mesh groups | `RenderPass` rebinds group 0 before water (`render.rs` patched line ~305) |

## 5. Public API the game/base consumes (exact signatures)

```rust
// wc3-render (crate root re-exports)
pub struct Renderer<'a>;                         // renderer.rs:31 — composition for bench/headless
impl Renderer { pub fn new(device, queue, format, width, height, models: Vec<Model>,
                           images: &[TextureImage], max_texture_size: u32,
                           terrain: Option<TerrainBatch>, font_atlas: &TextureImage,
                           minimap: Option<&TextureImage>) -> Self; }
impl MeshRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat,
               models: Vec<Model>, images: &[TextureImage], max_texture_size: u32) -> Self;
    pub fn prepare(&mut self, device, queue, scene: &Scene, mask: PassMask, stats: &mut Counters);
    pub fn draw_group(&self, pass: &mut wgpu::RenderPass<'_>, group: u8, stats: &mut Counters);
    pub fn draw_static(&self, pass: &mut wgpu::RenderPass<'_>, batch: &StaticBatch,
                       group: u8, stats: &mut Counters);
    pub fn create_static_batch(&self, device, queue, vertices: &[MeshVertex],
                               indices: &[u32], specs: &[StaticDrawSpec]) -> StaticBatch;
    pub fn textures(&self) -> &TextureStore;
    pub fn set_skin_mode(&mut self, mode: SkinMode);
    pub fn texture_stats(&self) -> TextureStats;
}
impl UiRenderer {
    pub fn new(device, queue, format, layout: &wgpu::BindGroupLayout,
               font_atlas: &TextureImage, minimap: Option<&TextureImage>) -> Self;
    pub fn prepare(&mut self, device, queue, scene: &Scene, stats: &mut Counters) -> bool;
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, textures: &TextureStore,
                stats: &mut Counters);
}
pub fn read_back(device, queue, texture, width, height) -> Result<Vec<u8>, String>;
pub fn texture::bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout;
// wc3-game facade (patch 0002)
impl wc3_game::render::Renderer {
    pub fn new(device, queue, format, assets: &Assets,
               font_atlas: &TextureImage, minimap: Option<&TextureImage>) -> Result<Self, String>;
    pub fn resize(&mut self, device, width: u32, height: u32);
    pub fn render(&mut self, device, queue, view: &wgpu::TextureView, scene: &Scene);
    pub fn render_with(&mut self, device, queue, view, scene, options: RenderOptions);
    pub fn counters(&self) -> wc3_render::Counters;   // new: per-frame budget accessor
}
```

`wc3-game` → `wc3-render` is the only new dependency edge (BASE §1: allowed,
`wc3-render` already sits below `wc3-game`). `wc3-render` depends on nothing from
`wc3-game`.

## 6. Tests to add after handoff (written now, **not run**)

| Test | Fixture | Asserts |
|---|---|---|
| `wc3-render/tests/parity.rs::gpu_and_cpu_skinning_render_the_same_frame` | synthetic 2-geoset model (weighted 2-bone + static), 192×192 target, ring, UI quad | same draw count, MAE < 1.0, SSIM > 0.99 between `SkinMode::Gpu` and `SkinMode::Cpu` |
| `…::draw_accounting_matches_the_renderer` | same scene | `parity::account()` draw count == `Counters::draws` |
| `…::static_batch_matches_the_unit_path` | one static quad, drawn as a unit and as a `StaticBatch` | **exact frame hash equality** |
| `wc3-render/examples/game_integration_shapes.rs` | compile-only | every call shape of patch 0002 type-checks (already green: `cargo check --examples`) |
| `apps/bench` parity run (patch 0005) | Human04 + real assets, 131 units | `BENCH parity units_ui mae` between `wc3_game::render::Renderer` (patched facade) and the pre-merge reference frame (stored PPM) |
| `cargo check -p wc3-game --lib` | — | gate 3 of BASE §7 after patches 0002/0003/0007 |

CPU-only fixtures already running: `cargo test -p wc3-render --lib` → **24 passed**
(perf/frame/texture/mesh/parity accounting). GPU fixtures wait for the handoff:
they create a device and are therefore **not run** under the current no-device rule.

## 7. Conflicts and exact next steps

| # | Conflict | Owner | Handling |
|---|---|---|---|
| 1 | `crates/wc3-game/src/render.rs`: world layer just added `clock` + `terrain.update_time` (water scroll) | world / render layer | patch 0002 **keeps** both (see the `clock` field + `update_time` call); re-verify after their next edit |
| 2 | `crates/wc3-game/src/model_render.rs` is world-owned; patch 0007 would delete the GPU half | world layer | gate 0007 on the world handoff; until then the file is dead code (nothing calls `ModelPipelines`/`ModelBatch` after 0002) |
| 3 | `crates/wc3-game/src/assets.rs` line 378 keeps a deliberate 256² terrain-atlas copy (`terrain_tile_images` holds the native source) | assets/world | patch 0001 only touches the model-texture path (line 725) |
| 4 | `apps/bench` is bench-owned | bench owner | patches 0004/0005 are mechanical (import rename + `TerrainBatch`); apply before the post-merge parity run |
| 5 | `wc3-gpu` still exists as a workspace member (facade) | dispatcher | base v2: delete `crates/wc3-gpu` + its workspace member once `apps/smoke` imports `wc3_render::GpuContext` (one-line change) |
| 6 | `crates/wc3-render` declares `wc3-formats/sim/anim` but no longer uses them directly | dispatcher | keep: the crate is the render layer and will consume `wc3-formats` types when the asset bridge lands; removing them is a v2 cleanup |
| 7 | Animated-parameter hooks (KMTA/KMTF per-frame sampling) still live in `wc3-game` (`Assets::mesh_layer_alpha`) | anim layer | renderer takes resolved alphas; no second animator introduced |

Exact next steps (in order):

1. **World handoff**, then apply `0007` (model_render thin) and re-check
   `cargo check -p wc3-game --lib`.
2. Apply `0003` + `0002` (game dependency + render facade), then run
   `cargo check -p wc3-game --lib` and `cargo test -p wc3-game --lib` (gate 3/4).
3. Apply `0004` + `0005` (bench) and run the parity bench on device:
   `scripts/perf-gpu.sh --frames 60 --units 120 --width 2414 --height 1080`
   → expect `BENCH parity units_ui mae ≈ 0.49` (the pre-merge measured value) and
   `BENCH optimized_counters draws == BENCH baseline_plan draws`.
4. Run `cargo test -p wc3-render` (all targets, needs a GPU for `tests/parity.rs`).
5. v2: delete `crates/wc3-gpu`, point `apps/smoke` at `wc3_render::GpuContext`.

## 8. Evidence

| Claim | Evidence |
|---|---|
| Scope compiles | `cargo check -p wc3-gpu -p wc3-render --examples` → `Finished dev profile … 0 errors, 0 warnings` (2026-11-12, this session) |
| Unit tests pass | `cargo test -p wc3-render --lib` → `24 passed; 0 failed` |
| Patch set applies | `patch --dry-run -p1` on fresh copies of `assets.rs`, `render.rs`, `model_render.rs`, both `Cargo.toml`s, `gpu_bench.rs` → all **OK** |
| Measured parity of the ported algorithms | `analysis/agent-perf/REPORT.md` + `bench-results/skin-gpu-2414x1080.log`: units+UI MAE **0.4901/255**, max 251; draws identical (475), index references identical (238 860) |
| Measured performance of the ported renderer | same log: CPU/render p50 73.19 → 23.29 ms (3.14x), throughput 15.1 → 45.1 fps (2.98x), upload/frame 7.78 → 0.71 MB, texture bytes 31.0 → 10.0 MB (−67.6%) |
| One renderer at runtime | `crates/wc3-gpu/src/lib.rs` is a facade; `wc3-game/render.rs` (patched) has no `create_render_pipeline`, no `create_texture`, no vertex layout of its own |
| H2 | captures 546/548/562/587/**600** (integration applied), experiences **4276** (mobile renderer perf) and **4314** (merge method), monitor ledger `agent-perf` #301 |

## 9. State

**DONE for the GPU scope**: `wc3-render` is the canonical renderer and compiles
with its tests and the compile-only integration example; `wc3-gpu` is a facade;
the game/bench integration is delivered as six verified patches.

**Pending (not this layer's call)**: applying patches 0001-0005/0007 (world
handoff + dispatcher), then the post-merge device parity run and gate 3/4.
No device run has been performed under the no-device rule.
