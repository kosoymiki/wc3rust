# Bench artifacts — how to read them

All numbers are produced by `apps/bench/src/gpu_bench.rs` on this device:

* adapter: **Turnip Adreno (TM) 725**, Mesa 26.2.6, Vulkan, timestamp queries supported
* run under `taskset -c 4-7` (big cores), `ulimit -v 4000000`, debug profile
* the device is shared with the rest of the 10-agent team; every run records the load
  average (`"loadavg"` in the JSON) because the box was frequently at load 10–20 on 8 cores
* baseline = the repo renderer (`wc3_game::Renderer`), optimized = `wc3_gpu::Renderer`;
  both render the **same scene** in the **same run**, alternating order every frame and
  rotating pass variants round-robin, so contention cancels out

## Files

| file | content |
|---|---|
| `skin-gpu-2414x1080.log` / `.json` | headline run: GPU skinning, 120 units spawned (131 drawn incl. buildings), 2414x1080 |
| `skin-cpu-2414x1080.log` / `.json` | same scene with the renderer's CPU skinning path — isolates the skinning fix |
| `gpu-skin-2414x1080.log` / `.json` | earlier GPU-skin run kept for the loadavg spread |
| `cpu-skin-2414x1080.log` | earlier CPU-skin run at 1280x720 (build race, log only) |
| `bench-*.json` | earlier runs used while developing the harness |
| `parity.*.ppm` / `parity.*.png` | pixel-parity frames (units + HUD; terrain excluded while `terrain_render.rs` is rewritten in parallel) |
| `final-run.log`, `final-build.log` | the measurement script's own log (build retries through other agents' edits) |

## What each number means

* `cpu_ms` — wall time of one `render()` call (encode + submit), interleaved A/B.
* `gpu_ms` — GPU timestamp span around one submission. Batched into one resolve per run.
  If the CPU cannot feed the GPU the span includes the idle gap, which is exactly the
  effect being measured; the `no_units`/`no_terrain` variants separate the passes.
* `throughput` — end-to-end ms/frame for `N` back-to-back submissions followed by a single
  `device.poll(Wait)`, four alternating runs, minimum reported.
* `alloc_bytes_per_frame` / `alloc_calls_per_frame` — heap traffic of the render call from
  the process-wide counting allocator. `encode_alloc_bytes` (logged separately) is the part
  inside the batched renderer's own code: 1 556 B / 19 calls per frame.
* `draws`, `vertices`, `indices`, `upload_bytes`, `buffer_writes`, `buffer_grows`,
  `pipeline_binds`, `texture_binds` — exact counters for the batched renderer. For the repo
  renderer the draw/vertex/upload numbers are replayed from its own loop (`inferred`),
  cross-checked against identical index counts (238 860 on both sides) and against the
  measured allocation traffic.
* `parity` — mean/max channel difference of the units+HUD passes between the two renderers
  (RGB 0–255, alpha excluded).

## Reproduce

```bash
cd /data/data/com.termux/files/home/src/herofolends-rs
scripts/perf-gpu.sh --frames 60 --units 120 --width 2414 --height 1080 --skin gpu
scripts/perf-gpu.sh --frames 60 --units 120 --width 2414 --height 1080 --skin cpu
```

`scripts/perf-final.sh` runs both and writes `skin-{gpu,cpu}-2414x1080.{log,json}`.
