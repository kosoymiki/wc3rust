# ROUND 6 — r2-gpu (ledger 352, 2026-11-13)

## p1 — Model residency/eviction counters with 216 models

- `crates/wc3-render/src/cache.rs`: `ResidentSet<T>` (exact byte budget, LRU, `rejected` guard), `ResidentModel` (per-model vertex/index GPU buffers), `TexturePlan`, `ResidencyStats`. Existing module extended in prior rounds; this round added:
  - `cache.rs` tests: `astc_fallback_chain` — `CompressionSupport` selects `Astc6x6` when present, degrades to `Etc2Rgba8` when ASTC absent, returns `None` when no compression (uncompressed RGBA8 fallback remains valid via `TextureImage::fit`).
  - `tests/cache.rs`: `two_hundred_sixteen_models_stay_inside_the_budget` — 216 models at real measured mean 117661 B, 64-model budget: `resident=64 bytes=7522304 budget=7530304 loads=216 evictions=152 rejected=0`, re-residency of evicted model works and stays inside budget. (Device: Turnip Adreno 725.)

## p2 — ASTC/compressed-texture path status + fallback

**Status: implemented and device-verified.**

- Adapter probe (Turnip Adreno 725, Vulkan, wgpu 30): `astc=true astc_hdr=true etc2=true bc=true`, `max_texture_2d=16384`, timestamps true.
- Implementation: `crates/wc3-render/src/astc.rs` (`AstcFile`, block encode/decode, `astc_chain_bytes`), `crates/wc3-render/src/texture.rs` (`CompressionSupport::probe`, `CompressedFormat`/`CompressedImage`/`CompressedLevel`, `upload_compressed`, `TextureStore::upload_sources`/`new_with_sources`, `TextureImage::fit` never-upscale), `scripts/perf-astc.sh` (astcenc 4.8.0 NEON).
- Fallback chain (unit-tested, `cache::tests::astc_fallback_chain`): ASTC LDR → `Astc6x6`/`Astc8x8`; no ASTC + ETC2 → `Etc2Rgba8`/`Etc2Rgb`; no compression → `None` (caller uses RGBA8 upload path). Block-alignment enforced by `CompressedFormat::accepts`.
- Device evidence (`cargo test -p wc3-render --test astc`, 4 passed): standalone 6×6 MAE 4.04/255, 8×8 MAE 4.70/255; mesh-pipeline 8×8 MAE 3.96/255. Real 1280×720 frame (prior): 6×6 43.33 dB @3.56 bpp (410896 B), 8×8 39.12 dB @2.0 bpp (230416 B) vs 3686400 B RGBA8. Texture plan 542×256²: RGBA8 189.4 MB vs ASTC 6×6 21.7 MB vs ASTC 8×8 11.9 MB.
- `TexturePlan::select()` picks ASTC 6×6 when RGBA8 exceeds 64 MB budget and ASTC 8×8 under tighter budget; uncompressed fallback always available.

## p3 — Device perf baselines (JSON with `perf.memory`)

Device: Turnip Adreno (TM) 725, Vulkan, 2414×1080, release (`--release`, thin LTO), `taskset -c 4-7`, `ulimit -v 4000000`. Date 2026-11-13.

| artifact | units (drawn) | draws | throughput min | fps | parity MAE | counter parity | `perf.memory` |
|---|---:|---:|---:|---:|---:|---:|---|
| `analysis/agent-gpu/r6-131u-2414x1080.json` | 147 (131 spawned) | 166 | 6.670 ms | 149.9 | 0.0000 | 165/7980/11046 match | peak 119.4 MB, 216-geom 0.1 + 542-astc6 21.7 = **21.9 MB**, ceiling 1536 |
| `analysis/agent-gpu/r6-147buildings-2414x1080.json` | 163 (147 spawned) | 182 | 6.703 ms | 149.2 | 0.0000 | 181/8556/11718 match | peak 119.7 MB, **21.9 MB**, ceiling 1536 |

`perf.memory` gate: **21.9 MB combined (216 models + 542 ASTC6 textures) << 1536 MB ceiling** in both JSONs.

Notes (observed): these runs used the bench's fallback/simple shape scene (4 models, mean geometry 560 B, `source=fallback`), so frame times and per-model geometry differ from the real-asset r2b baselines (`analysis/agent-perf/bench-results/r2b-147u-2414x1080.json`, `r2b-147buildings-2414x1080.json`) which reported 56.8/54.9 fps on full game assets with mean geometry 117661 B and 610/708 draws. The memory-ceiling projection remains valid by construction: `TexturePlan` uses 542×256² and the 216-model geometry projection uses measured mean bytes (0.1 MB here for shape models; 25.4 MB with real assets per r2b/r4 evidence).

## Test / gate evidence

- `cargo test -p wc3-gpu` → rc=0, `ok. 0 passed; 0 failed` (lib+doc, facade).
- `cargo test -p wc3-render --lib` → 36 passed, 0 failed.
- `cargo test -p wc3-render --test cache` → 3 passed, 0 failed (incl. `two_hundred_sixteen_models_stay_inside_the_budget`: resident=64 evictions=152 rejected=0, budget respected).
- `cargo test -p wc3-render --test parity` → 3 passed, 0 failed.
- `cargo test -p wc3-render --test astc` → 4 passed, 0 failed (6×6 MAE 4.04, 8×8 MAE 4.70, mesh pipeline 8×8 MAE 3.96).
- `cargo check -p wc3-re` → rc=0 (0 errors, 37 warnings) — prior BLOCKED-gpu.md blocker resolved.
- `cargo build --release -p herofolends-bench --bin herofolends-gpu-bench` → rc=0.

## Files changed (owner paths)

- `crates/wc3-render/src/cache.rs`: added `astc_fallback_chain` unit test and `CompressionSupport`/`CompressedFormat` import in tests.
- `crates/wc3-render/tests/cache.rs`: added `two_hundred_sixteen_models_stay_inside_the_budget` device test.
- `apps/bench/src/gpu_bench.rs`: hoisted `model_bytes_mean`/`texture_plan_astc6`, added `perf.memory` block to JSON output.
- `analysis/agent-gpu/{ROUND6.md,r6-131u-2414x1080.{json,log},r6-147buildings-2414x1080.{json,log}}`.

## Rollback

`git -C /data/data/com.termux/files/home/src/herofolends-rs checkout -- crates/wc3-render/src/cache.rs crates/wc3-render/tests/cache.rs apps/bench/src/gpu_bench.rs` (workspace uncommitted; no commit made).
