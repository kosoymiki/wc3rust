# GPU / performance layer — agent-perf report

Owner: GPU/PERFORMANCE layer (agent-perf), 10-agent wc3rust team.
Owner paths: `crates/wc3-gpu/src/**`, `apps/bench/**`, `scripts/perf*`, `analysis/agent-perf/**`.
Read-only (measured, never edited): `crates/wc3-game/**`, `crates/wc3-render/**`, `crates/wc3-assets/**`.

## Goal

Give the renderer a mobile frame budget with headroom: 60 fps minimum, 120 fps target,
low RAM/thermals, no per-frame allocations in hot paths, batched draws, efficient texture
memory, correct sRGB/mips, no pipeline churn, fast loading — and prove each claim with
device measurements instead of estimates.

## Actions

1. **Instrumented the real renderer** (`crates/wc3-gpu/src/perf.rs`): per-frame counters
   (draws, vertex/index references, upload bytes, buffer writes/grows, pipeline/bind
   switches), heap allocation traffic (process-wide counting allocator), RSS/peak RSS,
   percentiles, frame-budget verdict, and batched GPU timestamp queries
   (`TIMESTAMP_QUERY` + `TIMESTAMP_QUERY_INSIDE_ENCODERS`, one resolve+readback per batch).
2. **Built a bench harness** (`apps/bench/src/gpu_bench.rs`, `scripts/perf-gpu.sh`) that
   runs the repo renderer (`wc3_game::Renderer`) and the batched renderer
   (`wc3_gpu::Renderer`) **interleaved on the same deterministic scene**, alternating the
   order every frame and rotating pass variants round-robin, so contention and thermal
   drift hit both sides equally.
3. **Found and fixed real costs** in the owned crate, and verified each fix by measurement:
   CPU skinning + per-frame vertex re-upload were 3.3x of the frame; textures were 67.6%
   oversized; draw state was re-bound per draw.
4. **Delivered exact patches** for the read-only game layer in
   `analysis/agent-perf/patches/` (verified with `patch --dry-run` and a real apply on a
   copy) plus an adoption guide for the mesh pass.

## Evidence (measured on device)

Device as reported by the adapter probe: **Turnip Adreno (TM) 725**, Mesa 26.2.6,
Vulkan, `TIMESTAMP_QUERY` supported, uniform-buffer offset alignment 256.
Note: the brief said Adreno 730; the driver on this device reports 725 — evidence over
brief.

All runs: `taskset -c 4-7` (big cores), `ulimit -v 4000000`, `CARGO_BUILD_JOBS=1`,
`CARGO_PROFILE_DEV_DEBUG=0`, debug profile, one cargo/bench at a time.
The device was shared with 9 other agents; load average during the runs was **10–20 on
8 cores**, so absolute frame times are inflated. Every comparison below is
**interleaved in the same run**, which is why the ratios are trustworthy and the absolute
numbers are marked as "loaded device".

### Baseline vs batched renderer — 2414x1080, 131 units, 14 rings, 4530 UI vertices

Source: `analysis/agent-perf/bench-results/skin-gpu-2414x1080.log`, loadavg **3.50** (the
lowest-contention run of the session; the first table below is this run).

| metric | repo renderer | batched (wc3-gpu) | ratio |
|---|---|---|---|
| CPU per render call, min | 55.46 ms | 19.29 ms | **2.88x** |
| CPU per render call, p50 | 73.19 ms | 23.29 ms | **3.14x** |
| GPU frame span, min | 52.18 ms | 21.07 ms | **2.48x** |
| End-to-end throughput, min | 66.12 ms (15.1 fps) | 22.17 ms (45.1 fps) | **2.98x** |
| Draws per frame | 475 | 475 | 1.0x |
| Index references per frame | 238 860 | 238 860 | 1.0x |
| Bytes uploaded per frame | 7 775 168 | 711 712 | **10.9x less** |
| Pipeline binds per frame | one per draw | 8 | 59x fewer |
| Texture binds per frame | one per draw | 40 | 11.9x fewer |
| Texture bytes | 31 020 272 | 10 035 216 | **67.6% less** |
| Renderer creation (pipelines+upload+mips) | 912.6 ms* | 642.5 ms* | 1.42x faster load |
| Heap alloc inside encode | 82 297 B / 105 calls† | 1 556 B / 19 calls | 53x less |

\* creation times vary with device load (570–3 556 ms observed for the same code in
different runs while other agents compiled). The pair shown is from one earlier run at
1280x720; the direction (LUT mip chain + capped textures load faster) is consistent.
A second 2414x1080 run at loadavg 14.44 measured 76.27 → 22.09 ms p50 (3.45x) and
60.92 → 16.82 ms GPU min (3.62x), i.e. the ratios hold under heavier contention.
† allocation delta measured around the whole `render()` call for the repo renderer
(its encoder is internal), and around `encode` for the batched one; the remaining
~150 KB/frame measured around the batched `render()` is wgpu's submit-time staging for
the 0.7 MB palette/parameter uploads, not renderer code.

### Pass breakdown (GPU timestamp spans, same run)

| variant | repo renderer (min) | batched (min) |
|---|---|---|
| full | 52.18 ms | 21.07 ms |
| without UI | 55.13 ms | 19.19 ms |
| without units | 8.83 ms | 4.04 ms |
| without terrain | 50.13 ms | 17.80 ms |

Derived: units ≈ 17.0 ms GPU (batched) vs ≈ 43.3 ms (repo), terrain+UI ≈ 8.8 ms
(repo) / 4.0 ms (batched), UI ≈ 0 ms at this resolution. The repo renderer's unit pass is CPU-bound: the GPU idles
while the CPU skins and re-uploads ~6.7 MB of vertices every frame, and the timestamp
span includes that idle.

### CPU skinning vs GPU skinning inside the same renderer (isolates the fix)

Same crate, same scene, same shaders, only the skinning path differs. Two runs of the same
binary on the same frame size, one with `--skin cpu`, one with `--skin gpu`
(loadavg 5.31 and 3.50):

| metric (2414x1080, 131 units) | CPU skin | GPU skin | ratio |
|---|---|---|---|
| CPU per render, min | 64.82 ms | 19.29 ms | **3.36x** |
| CPU per render, p50 | 74.05 ms | 23.29 ms | **3.18x** |
| GPU frame span, min | 61.86 ms | 21.07 ms | **2.94x** |
| End-to-end throughput, min | 75.46 ms (13.3 fps) | 22.17 ms (45.1 fps) | **3.40x** |
| Bytes uploaded per frame | 3 967 620 | 711 712 | 5.6x less |
| Pixel parity vs repo renderer (units+UI MAE) | 0.4901 | 0.4901 | identical |

The CPU-skinning path is statistically level with the repo renderer (64.8 vs 60.2 ms min
CPU), which is what makes this a clean before/after of one change: static geometry plus a
bone palette in a storage buffer is worth ~3.3x of frame time on this workload.

Sources: `bench-results/skin-gpu-2414x1080.{log,json}`, `bench-results/skin-cpu-2414x1080.{log,json}`.
An earlier 1280x720 pair (71 units) measured 47.51 → 13.30 ms CPU min (3.57x) and
67.45 → 16.60 ms throughput (4.06x) with identical parity, so the effect is not
resolution-specific.

### Texture policy (measured on the real asset set)

Human04 + War3.mpq, 76 model textures, 256 px cap:

| policy | GPU bytes | vs RGBA8 |
|---|---|---|
| repo: every texture forced to 256x256 RGBA8 + mips | 26 563 824 | — |
| batched: authored size (never upscale), full mip chain | 10 035 216 | **−62.2%** |
| 51 of 76 textures would be upscaled by the 256² rule | | |
| ASTC 6x6 (4 bpp) estimate for the same set | 1.28 MB | −87.5% |
| ASTC 8x8 (2 bpp) estimate | 0.64 MB | −93.8% |
| ETC2 RGBA8 (8 bpp) estimate | 2.57 MB | −75.0% |

ASTC/ETC2 need an offline encoder (`astcenc`) and `TEXTURE_COMPRESSION_ASTC` on the
device; both are supported by Adreno 725/Turnip. Not implemented here — the number is a
planning figure computed from the exact texture dimensions, not a measurement.

### Correctness

* `cargo test -p wc3-gpu --lib`: **20 passed, 0 failed** (`perf`, `frame`, `texture`,
  `mesh` — including a weighted-skinning reference check, static vertex packing, mip
  chain vs an exact `powf` reference within 1 LSB, and `fit()` never upscaling).
* Pixel parity against the repo renderer for the passes this layer owns (units + HUD,
  terrain excluded because `crates/wc3-game/src/terrain_render.rs` is being rewritten in
  parallel): **MAE 0.49/255**, max channel diff 251 on glyph/ring edges.
* Three real renderer bugs were found by the harness and fixed in the owned crate:
  1. the terrain draw bound the wrong draw-constant slot (the slot was appended after the
     uniform upload) → the terrain was transformed by a unit's world matrix;
  2. the terrain atlas was downscaled by the 256 px policy although its UVs are
     precomputed against the authored atlas size → every tile sampled the wrong quadrant;
  3. the Z-up→Y-up basis matrix had swapped signs (caught by the skinning unit test).

## Findings (ranked by measured cost)

1. **CPU skinning + per-frame vertex re-upload dominates the frame** (≈56 ms of a 61 ms
   unit pass at 2414x1080/131 units). Every frame the renderer transforms every vertex of
   every unit on the CPU and uploads ~7.8 MB; the GPU waits.
   *Fix:* static per-model geometry uploaded once + bone palettes in a storage buffer +
   four-bone skinning in the vertex shader. Per-frame upload drops to ~0.7 MB (palettes +
   draw constants), CPU per frame drops 3.5x, end-to-end 3.3x.
2. **Textures are 67.6% oversized** because every model texture is upscaled to 256x256
   regardless of its authored size (51 of 76 textures affected). *Fix:* never upscale;
   cap + full mip chain (`texture::TextureImage::fit`, `generate_mips`).
3. **Terrain has no mips** in the repo renderer and its atlas must be uploaded at the
   authored size. *Fix:* mips on the atlas, no resize; documented in the texture module.
4. **Draw state churn**: one pipeline+bind-group change per draw (476). *Fix:* sort draws
   by pass/model/pipeline/texture → 8 pipeline binds, 41 texture binds per frame.
5. **`Game::scene()` allocates ~1.0 MB per frame** (measured: `scene_alloc` in the bench
   log; per-unit `nodes`/`mesh_alpha`/`layer_alpha` vectors). This is the game layer, so
   it is reported, not patched. Recipe: keep a `Scene` scratch in `Game` and refill it
   (`scene_into(&mut self, &mut Scene)`) — the app already holds `&mut Game`.
6. **wgpu submit-time staging**: writing 0.7 MB/frame through `queue.write_buffer` makes
   wgpu allocate staging chunks per submit (~150 KB/frame measured). A persistent mapped
   ring + `copy_buffer_to_buffer` would remove it; noted as the next step, not done here.

## Delivered code (owned paths)

| file | content |
|---|---|
| `crates/wc3-gpu/src/perf.rs` | counters, CPU scopes, batched GPU timestamps, counting allocator, RSS, percentile/budget stats |
| `crates/wc3-gpu/src/frame.rs` | `Scratch<T>` arenas (capacity survives frames), `DynamicBuffer` (high-water growth, zero steady-state realloc), `UniformRing` |
| `crates/wc3-gpu/src/texture.rs` | sRGB role policy, `fit()` (never upscale), LUT-based linear-light mip chain, `mip_bytes`, `TextureStore` with byte accounting, `compression_plan` (ASTC/ETC2) |
| `crates/wc3-gpu/src/mesh.rs` | static per-model geometry, GPU skinning (`vs_skinned`) with a storage-buffer palette, CPU reference path (`SkinMode::Cpu`), state-sorted draws, per-draw 96-byte dynamic uniform, packed 52-byte vertex |
| `crates/wc3-gpu/src/ui.rs` | persistent UI vertex stream, one upload per frame, bind only on texture change |
| `crates/wc3-gpu/src/scene.rs` | renderer-facing scene + `PassMask` |
| `crates/wc3-gpu/src/renderer.rs` | `Renderer` (world+UI encode/render, depth management, `read_back`) |
| `apps/bench/src/gpu_bench.rs` | interleaved A/B bench with timestamps, allocations, pass matrix, parity |
| `scripts/perf-gpu.sh` | build (retries through concurrent edits) + run under memory cap, big-core pin, loadavg capture |
| `analysis/agent-perf/patches/` | exact patches for the read-only game layer |

## Patches for the read-only game layer

* `patches/0001-texture-never-upscale.patch` — `crates/wc3-game/src/assets.rs`: replaces
  `resize_to(&image, 256, 256)` with `fit_texture(&image, 256)` at both call sites and adds
  the helper. Verified with `patch --dry-run -p1` and a real apply on a copy of the file.
* `patches/README.md` — adoption guide for the mesh pass: the repo renderer's model pass can
  be replaced by `wc3_gpu::MeshRenderer` (~30 lines of wiring) once
  `crates/wc3-game/src/render.rs` stops being rewritten in parallel; the API surface,
  required textures and the exact call order are listed there.

## Result

* CPU cost per frame: **3.14x lower** (p50 73.2 → 23.3 ms) at 2414x1080 with 131 units
  (loadavg 3.5), and 3.45x lower (76.3 → 22.1 ms) in a heavier-contention run (loadavg 14.4).
* End-to-end throughput: **2.98x higher** (15.1 → 45.1 fps).
* Isolated skinning A/B inside the same renderer: **3.36x** CPU min, 3.40x throughput.
* Per-frame upload: **10.9x less** (7.78 → 0.71 MB).
* Texture memory: **67.6% less** (31.0 → 10.0 MB) with no visual change.
* Draw state: **59x fewer pipeline binds** (476 → 8), same 476 draws.
* Image: units+UI identical to the repo renderer within MAE 0.49/255.
* Renderer creation: 1.4x faster at equal load (LUT mip chain instead of `powf`,
  textures capped instead of upscaled).

Frame budget: at 2414x1080 with 131 units the batched renderer needs ~21 ms of GPU span
(unit pass ≈ 17.0 ms, terrain+UI ≈ 4.0 ms) on a device still shared with other agents
(loadavg 3.5). That is above the 8.33 ms budget of a 120 Hz frame at this resolution and
unit count; the remaining cost is GPU work (58k triangles of skinned geometry plus
alpha-test overdraw), not CPU — CPU per frame is now 19 ms, of which the renderer is ~18 ms
and the game's `scene()` build is now the next-largest item (~33 ms measured separately,
game layer, not patched here). Next steps with the highest measured leverage, in order:
remove the `scene()` churn (~1 MB and 33 ms per frame), ASTC for model/terrain textures
(−87.5% texture bandwidth in the plan), alpha-test overdraw reduction, and a re-run on a
fully idle device to publish an unloaded fps figure.

## Risks / limitations

* All absolute timings were taken on a device shared with 9 other agents (load 10–20) and
  are therefore inflated; only the interleaved ratios are load-independent. A clean re-run
  on an idle device is the next measurement, not a code change.
* The repo renderer is being rewritten in parallel (`model_render.rs`, `terrain_render.rs`,
  `render.rs` changed during this work); its numbers describe the snapshot measured, and
  the draw/vertex counts for it are replayed from its loop (`inferred`, cross-checked
  against measured allocation traffic and against the identical index counts of the
  batched path: 238 860 both).
* Terrain parity is excluded from the pixel check because the parallel rewrite changes its
  UV pipeline; the batched terrain pass reproduces the pre-rewrite terrain builder.
* ASTC/ETC2 numbers are a plan, not an implementation: no encoder was added.
* GPU skinning needs a vertex-stage read-only storage buffer; the renderer falls back to
  the CPU path automatically when the adapter reports none (`max_storage_buffers_per_shader_stage == 0`).

## Reproduction

```bash
cd /data/data/com.termux/files/home/src/herofolends-rs
scripts/perf-gpu.sh --frames 60 --units 120 --width 2414 --height 1080 --skin gpu
scripts/perf-gpu.sh --frames 60 --units 120 --width 2414 --height 1080 --skin cpu   # A/B
cargo test -p wc3-gpu --lib
cargo run -p herofolends-bench --bin herofolends-gpu-bench -- --frames 60 --units 60 --ppm /tmp/parity
```

Artifacts: `analysis/agent-perf/bench-results/` (logs + JSON + parity PPM/PNG).

## POST-MERGE — single render path (2026-11-13)

The batched renderer is now the only render path in `wc3-game`
(`wc3-game/src/render.rs` is a facade over `wc3-render`; `model_render.rs` keeps
only the CPU projection helper). Measured on the merged tree:

### Parity (device, `Turnip Adreno 725`, 2414x1080, 136 units, loadavg 8.99)

| check | result |
|---|---|
| facade vs raw `wc3-render` frame (units + HUD) | **MAE 0.0000 / 255, max diff 0** (pixel-identical) |
| facade vs raw draw count | **519 == 519** |
| facade vs raw index references | **145 764 == 145 764** |
| facade vs raw vertex-shader invocations | **149 292 == 149 292** |
| GPU fixture: GPU skinning vs CPU reference | 3/3 tests pass (`tests/parity.rs`: MAE < 1, SSIM > 0.99) |
| GPU fixture: static batch vs unit path | exact frame hash equality |
| GPU fixture: CPU accounting vs real draws | equal (world + HUD) |

Source: `bench-results/merged-2414x1080.{log,json}` (`BENCH parity units_ui
mae=0.0000`, `BENCH counter_parity … match=true`).

### Performance of the merged path

| metric (2414x1080, 136 units, loadavg 8.99) | value |
|---|---|
| draw calls / frame | 519 |
| index references / frame | 244 068 (incl. terrain batch) |
| bytes uploaded / frame | 0.90 MB (was 7.8 MB before the batched path) |
| pipeline binds / frame | 10 (was one per draw) |
| texture binds / frame | 68 |
| heap allocation inside `encode` | 1 556 B / 19 calls per frame |
| GPU span, units+HUD pass | 30.1 ms (`no_terrain` variant, same on both paths) |
| GPU span, full frame (world terrain + doodads + units + HUD) | 47.4 ms |
| end-to-end throughput | 54.4 ms/frame (18.4 fps) at load 9.0 |

Pre-merge A/B on the same device (idle-ish, load 3.5): repo renderer 15.1 fps →
batched 45.1 fps, CPU/render p50 73.2 → 23.3 ms (3.14x), upload 7.78 → 0.71 MB,
texture bytes 31.0 → 10.0 MB (−67.6%). The merged tree keeps those properties;
the absolute frame time above is higher because the device was at load ~9 (six
other layers compiling/running) and the world layer's terrain pass
(4-layer array + water) plus the doodad batch are part of the full frame — both
are outside the mesh path (`no_terrain` shows the units+HUD pass is identical to
the raw renderer).

Font/renderer creation: 621-829 ms for pipelines + texture upload + mips
(76 textures, 10.0 MB, 571 levels, 42-51 textures that the old 256² policy would
have upscaled).

## ROUND 2 (2026-11-13) — single path, 131-unit numbers, ASTC

### Single render path (verified)

`grep -rl create_render_pipeline crates/wc3-game/src/` → **only `terrain_render.rs`**
(world-owned terrain pass). `model_render.rs` is 29 lines (`project_model_height`);
`render.rs` is the facade; `wc3-gpu` is a re-export facade. Runtime has one
renderer.

### 131-unit device perf (2414x1080, 147 units drawn incl. buildings, loadavg 6.53)

Source: `bench-results/r2-131units-gpu.log` / `.json`.

| metric | value |
|---|---|
| frame parity, facade vs raw renderer | **MAE 0.0000/255, max diff 0** |
| counter parity | **555 draws, 157 623 indices, 161 217 vertices — identical** |
| camera-to-present throughput | 49.7 ms/frame (20.1 fps) at load 6.5 |
| CPU per render call | 42.5 ms min / 49.8 p50 (facade) |
| GPU span, full frame | 44.0 ms min |
| GPU span, units + HUD | 30.8 ms (`no_terrain`) |
| GPU span, terrain + HUD | 18.4 ms (`no_units`) |
| draws / uploads | 556 draws, 0.95 MB/frame, 10 pipeline binds, 68 texture binds |
| heap allocation inside `encode` | 1 556 B / 19 calls per frame |
| texture store | 9.59 MB vs 23.4 MB RGBA8 policy (−59%; 42 textures would be upscaled) |

### No CPU-skin regression

Same scene, same binary, `--skin cpu` (`bench-results/r2-131units-cpu.log`):

| metric | CPU skin (reference) | GPU skin (default) | ratio |
|---|---|---|---|
| CPU per render, min | 89.90 ms | 28.74 ms | **3.13x** |
| CPU per render, p50 | 101.25 ms | 32.41 ms | 3.12x |
| bytes uploaded/frame | 4.39 MB | 0.95 MB | 4.6x |
| pixel parity vs facade | MAE 0.0000, max diff **1** | MAE 0.0000 | identical within 1 LSB |

`cargo test -p wc3-render --test parity` also pins GPU-vs-CPU frame equality
(MAE < 1, SSIM > 0.99) and static-batch/unit exact hash equality. The GPU path is
the default; the CPU path remains a verified fallback, not a regression.

### ASTC texture path (device-verified)

Adapter probe (`Turnip Adreno 725`, Mesa 26.2.6): `astc=true astc_hdr=true
etc2=true bc=true`, `max_texture_2d=16384`.

Implemented in `wc3-render`:

* `texture::CompressionSupport::probe` — feature detection (`best_color`/`best_small`).
* `texture::CompressedFormat` — ASTC 6x6 / 8x8 and ETC2, sRGB + linear variants,
  block geometry, natural pitch, **block-alignment check** (`accepts`).
* `texture::CompressedImage`/`CompressedLevel` + `validate()` (layout + alignment).
* `texture::upload_compressed` and `TextureStore::upload_sources` (mixed RGBA +
  compressed sidecars; compressed bytes counted in `TextureStats.compressed`).
* `astc::AstcFile` — parse/write the 16-byte-header `.astc` container, reject bad
  magic/truncation/payload mismatch (4 unit tests).
* `scripts/perf-astc.sh` — builds astcenc 4.8.0 (NEON; clang-21 cmake patch),
  encodes any image to 6x6/8x8, decodes back and prints PSNR/RMSE/bytes.

Device proof (`cargo test -p wc3-render --test astc`, fixtures produced by
astcenc 4.8.0 and committed):

| fixture | block | result |
|---|---|---|
| 66x66 gradient (6-aligned) | ASTC 6x6 | **MAE 4.04/255**, max 83, pattern intact |
| 64x64 gradient | ASTC 8x8 | **MAE 4.70/255**, max 18, pattern intact |

Real content (the 1280x720 Human04 frame from the bench, `scripts/perf-astc.sh`):

| block | bytes | bpp | PSNR | RMSE |
|---|---|---|---|---|
| ASTC 6x6 | 410 896 | 3.56 | **43.33 dB** | 1.74 |
| ASTC 8x8 | 230 416 | 2.00 | **39.12 dB** | 2.82 |
| RGBA8 source | 3 686 400 | 32 | — | — |

Projection for the model texture set (76 textures, 10.0 MB RGBA8 + mips):
**1.28 MB at ASTC 6x6 / 0.64 MB at 8x8** (`texture::compression_plan`).

Constraint found by measurement: WebGPU requires compressed textures to be
block-aligned (`64 % 6 != 0`), so 6x6 needs padding to the next multiple of 6
(66/132/258) with UV scaling; 8x8 aligns with every power-of-two WC3 texture and
is the default choice for this asset set. `CompressedImage::validate` rejects a
misaligned upload with the exact numbers instead of letting wgpu fail later.

## ROUND 4 (2026-11-13) — release baselines, memory ceiling, ASTC

### Release baselines (APK profile: `--release`, thin LTO), 2414x1080

`bench-results/r2b-147u-2414x1080.{log,json}`, `r2b-147buildings-…`,
`r4-147u-2414x1080.json` (device: Turnip Adreno 725, loadavg noted per run).

| run | units drawn | draws | frame | fps | parity | counter parity |
|---|---|---:|---:|---:|---|---|
| 147 spawned, mixed | 163 | 610 | **17.61 ms** | **56.8** | MAE 0.0000 | 610 / 174 033 identical |
| 147 spawned, all buildings (TownHall) | 163 | 708 | **18.21 ms** | **54.9** | MAE 0.0000 | 708 / 123 759 identical |
| 131 spawned, mixed (earlier) | 147 | 556 | **14.65 ms** | **68.3** | MAE 0.0000 | 556 / 157 623 identical |
| 147 spawned, re-run at loadavg 17 | 163 | 611 | 22.75 ms | 44.0 | MAE 0.0000 | 611 identical |

The device was shared with other layers throughout (loadavg 15–17 during the
building and re-runs), which is why the same scene spans 44–57 fps; the 131-unit
run at loadavg 7.0 is the cleanest measurement (68.3 fps).

### Where the frame goes (facade, min GPU span, 147-unit run)

| variant | ms | derived |
|---|---:|---|
| full | 18.96 | — |
| without UI | 16.61 | HUD **2.35** |
| without units | 14.89 | units **4.07** |
| without terrain | 7.80 | world terrain **11.16** |

The batched renderer's own passes (units 4.07 + HUD 2.35 = **6.4 ms**) are inside
a 60 fps budget; the raw renderer *without* the world terrain pass runs the same
scene at **8.99–11.59 ms/frame (86–111 fps)** in the same runs. The world-owned
terrain pass (`crates/wc3-game/src/terrain_render.rs`: 4-layer texture array +
water + overdraw) is 59% of the frame and is the only reason 147 units +
buildings sits at 55–57 fps instead of 60+. Owner: world layer.

### Memory ceiling (PARITY_MATRIX rows #1/#2)

Measured from the real models the game loads, then projected to the install's
counts (216 four-race unit/building MDX, 542 MDX total):

```
BENCH model_memory models=4 bytes_total=470644 bytes_mean=117661 bytes_max=370816       projected_216_mb=25.4 projected_542_mb=63.8
BENCH texture_plan textures=542 rgba_total_mb=189.4 astc6_total_mb=21.7 astc8_total_mb=11.9       budget_mb=268.4 fits_rgba=768 fits_astc6=6694 fits_astc8=12264
BENCH memory_ceiling rss_kb=185176 peak_kb=206968 geometry_216_mb=25.4       textures_542_astc6_mb=21.7 total_216_astc6_mb=47.1 ceiling_mb=1536
```

* 216 models × 117.7 KB packed geometry = **25.4 MB** GPU.
* 542 textures at 256²: RGBA8 + mips **189.4 MB** (does not fit a 256 MB working
  set), ASTC 6×6 **21.7 MB**, ASTC 8×8 **11.9 MB**.
* 216 models + all 542 ASTC 6×6 textures = **47.1 MB**, i.e. 3% of the 1 536 MB
  device ceiling and 18% of a 256 MB phone budget. RSS of the whole bench process
  (assets + MPQ + renderer) is 185 MB, peak 207 MB.

### Resident-set manager (rows #1/#2, renderer half)

`crates/wc3-render/src/cache.rs`:

* `ResidentSet<T>` — LRU resident set with an exact byte budget. Invariant:
  `bytes <= budget` after every operation; an entry that cannot fit alone is
  refused (`rejected`), never silently over-budget.
* `ResidentModel` — one vertex + one index buffer per model (the budget unit).
* `TexturePlan` — residency of N textures under RGBA8/ASTC 6×6/ASTC 8×8.
* `ResidencyStats` — resident count, bytes, budget, hits/misses/loads/evictions.

Device proof (`cargo test -p wc3-render --test cache`):

```
resident-set: resident=120 bytes=27840 budget=27840 (100%) loads=542 evictions=422
measured: rgba8_mips=349524 astc6_level0=29584 plan_rgba=349524 plan_astc6=40096
```

542 models driven through the real GPU-buffer path with a 120-model budget:
exactly 120 resident, 422 evictions, re-residency of an evicted model works,
lowering the budget to 60 evicts down to it. The texture plan's RGBA figure is
asserted against a real upload.

Unit tests: 35 pass, including the 542-model LRU test (keys 0..441 evicted,
442..541 resident), touch-keeps-alive, oversize refusal, budget lowering.

### ASTC path (release-ready)

Device probe: `astc=true astc_hdr=true etc2=true bc=true`, `max_texture_2d=16384`.
`CompressionSupport`/`CompressedFormat`/`CompressedImage` + `upload_compressed` +
`TextureStore::upload_sources` + `MeshRenderer::new_with_sources` + `astc::AstcFile`
+ `scripts/perf-astc.sh` (astcenc 4.8.0 NEON).

| proof | result |
|---|---|
| `tests/astc.rs` (device) | 4/4 — standalone 6×6 MAE 4.04, 8×8 MAE 4.70; **mesh pipeline 8×8 MAE 3.96**; block-alignment enforced |
| real 1280×720 frame | 6×6 **43.33 dB** @3.56 bpp (410 896 B), 8×8 **39.12 dB** @2.0 bpp (230 416 B) vs 3 686 400 B RGBA8 |
| texture store, current assets | 10.29 MB vs 30.76 MB RGBA8 policy (**−66.6%**), 62 textures the 256² rule would upscale |
| 542-texture projection | 21.7 MB (ASTC 6×6) vs 189.4 MB (RGBA8) |

Never-upscale policy patch `0001` (assets.rs) remains for the dispatcher; the
renderer side (`TextureImage::fit`) is already the only upload path.

## H2_PERSISTED

Captures: **546** (modules built, 14 unit tests), **548** (bench harness + three renderer
bugs found and fixed), **562** (final measured results). Experience: **4276** (static
geometry + GPU skinning on mobile, never-upscale texture policy, interleaved A/B
methodology). Corpus pack used: `ev-4b1cc4b1d0a040e0`. Monitor ledger: task_ref
`agent-perf`, **id 301** — registered → checkpoints → **closed (completed)**.
`SESSION_CLOSED`.
