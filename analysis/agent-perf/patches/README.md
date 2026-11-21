# Patches for read-only layers (GPU/render merge)

Canonical renderer: **`crates/wc3-render`**. `crates/wc3-gpu` is a delegating
facade (owner-applied). Everything below touches files the GPU layer does not
own and is therefore delivered as a patch for the dispatcher/owner to apply.

Status 2026-11-13: **0002/0003/0004/0005 applied** in the GPU execution pass
(renderer unified, `cargo check -p wc3-game --lib` rc=0). **0001**, **0007** and
**0008** remain for the dispatcher (assets, model_render sequencing, smoke
migration before deleting `crates/wc3-gpu`).

| Patch | File | What it does | Gate |
|---|---|---|---|
| `0001-texture-never-upscale.patch` | `crates/wc3-game/src/assets.rs` | model textures: `resize_to(256,256)` → `fit_texture(256)` (cap, never upscale; −67.6% VRAM measured). Terrain tiles keep their intentional 256² atlas copy. | after assets owner's current edit |
| `0002-render-facade-wc3-render.patch` | `crates/wc3-game/src/render.rs` | `Renderer` becomes a facade over `wc3-render`: asset conversion, pass order (terrain → units → water → blended → decals), doodad `StaticBatch`, UI `prepare`/`draw`, per-frame `Counters`, in-place GPU-scene refill, water clock preserved | after world handoff |
| `0003-game-cargo-wc3-render.patch` | `crates/wc3-game/Cargo.toml` | adds `wc3-render = { path = "../wc3-render" }` | with 0002 |
| `0004-bench-cargo-wc3-render.patch` | `apps/bench/Cargo.toml` | `wc3-gpu` → `wc3-render` | bench owner |
| `0005-bench-source-wc3-render.patch` | `apps/bench/src/gpu_bench.rs` | import rename + `TerrainBatch` (atlas joins the texture store, `color_scale: 2.0`) | with 0004 |
| `0007-model-render-thin.patch` | `crates/wc3-game/src/model_render.rs` | keeps only `project_model_height`; deletes the second mesh pipeline family / CPU batch that nothing references after 0002 | **after world handoff** |
| `0008-smoke-to-wc3-render.patch` | `apps/smoke/src/main.rs`, `apps/smoke/Cargo.toml` | `wc3_gpu::GpuContext` → `wc3_render::GpuContext`; last consumer of the facade | with the `wc3-gpu` deletion |

Apply order: `0003` → `0002` → (`cargo check -p wc3-game --lib`) → `0007` when the
world layer releases the file → `0004` + `0005` before the parity bench run.

## Verification commands

```bash
cd /data/data/com.termux/files/home/src/herofolends-rs
for p in analysis/agent-perf/patches/000*.patch; do
  echo "== $p"; patch --dry-run -p1 < "$p" || echo "FAILED $p"
done
```

## Post-apply parity run (device, after handoff)

```bash
cargo check -p wc3-game --lib
cargo test -p wc3-render            # includes tests/parity.rs (GPU)
scripts/perf-gpu.sh --frames 60 --units 120 --width 2414 --height 1080
# expect: BENCH parity units_ui mae ≈ 0.49 and draws == baseline_plan draws
```

The measured pre-merge reference (same algorithms, `crates/wc3-gpu` before the
port) is in `analysis/agent-perf/REPORT.md` and
`analysis/agent-perf/bench-results/skin-gpu-2414x1080.log`.
