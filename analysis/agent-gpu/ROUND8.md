# ROUND 8 — GPU layer: projectile / FX rendering budget (#12 / #18), ledger 352 (2026-11-13)

## Goal
PARITY_MATRIX row **#12** (projectiles / missiles need visible travel geometry) and row
**#18** (effects / particles: Blizzard ≥ 6 visible waves, **particle budget ≤ N draws**) on the
single batched renderer, with device perf JSON. Owner paths: `crates/wc3-gpu/**`,
`crates/wc3-render/**`, `analysis/agent-gpu/**`.

## Actions (file:line)
- `crates/wc3-render/src/fx.rs` (new, ~700 lines): `FxRenderer` — CPU billboards → clip space,
  two pipelines (additive `One+One`, premultiplied alpha `One+OneMinusSrcAlpha`), depth test
  `LessEqual` with depth writes off, one dynamic vertex buffer (`frame::DynamicBuffer`), one
  draw per non-empty blend stream. `ParticleBudget { max_particles, max_draws, max_upload_bytes }`
  enforced before upload (`clamp_count`), `FxFrameStats { emitted, rendered, dropped_budget,
  dropped_culled, draws, vertices, upload_bytes, buffer_grows, prepare_micros }`.
  Emitters: `MissileSpec`/`missile_arc_point`/`projectile_trail` (head + fading trail),
  `impact_burst`, `blizzard_wave`, deterministic `Rng` (SplitMix64).
- `crates/wc3-render/src/renderer.rs:11,35-41,88-97,113-128,214-245`: `Renderer` owns
  `FxRenderer` + `fx_budget`, exposes `set_particles` / `particles()` / `fx_stats()`; encodes a
  `fx-pass` after the world pass (color+depth `Load`) when `mask.units` and particles exist.
- `crates/wc3-render/src/lib.rs:41,53-56`: `pub mod fx` + re-exports (`FxRenderer`,
  `ParticleBudget`, `MissileSpec`, emitters, `Rng`, constants).
- `crates/wc3-render/tests/fx.rs` (new): 10 device tests.
- `crates/wc3-render/examples/fx_budget.rs` (new): device perf JSON over the real `Renderer`
  composition (empty, 64 missiles, 6 Blizzard waves, 4096 particle stress, 4096→256 budget cap).
- `analysis/agent-gpu/r8-fx-budget.{json,log}`: device artifact.

Non-breaking integration choice: `Scene`/`PassMask` shapes are unchanged (apps/bench and
wc3-game are read-only this round and construct `GpuScene { … }` literals); particles enter via
`Renderer::set_particles` and the FX pass rides the `mask.units` world mask.

## Evidence (commands + rc + counts)

| command | rc | result |
|---|---|---|
| `cargo check -p wc3-render --all-targets` | 0 | compiles (lib, tests, examples) |
| `cargo test -p wc3-render --lib` | 0 | **44 passed; 0 failed** (8 new fx unit tests) |
| `cargo test -p wc3-render --test fx` | 0 | **10 passed; 0 failed** (device) |
| `cargo test -p wc3-render --test parity` | 0 | 3 passed (no regression) |
| `cargo test -p wc3-render --test cache` | 0 | 3 passed (no regression) |
| `cargo test -p wc3-render --test astc` | 0 | 4 passed (no regression) |
| `cargo check -p wc3-game --lib` | 0 | downstream game crate green |
| `cargo check -p wc3-gpu --all-targets` | 0 | facade green |
| `cargo build --release -p wc3-render --example fx_budget` | 0 | device binary |
| `taskset -c 4-7 ./target/release/examples/fx_budget analysis/agent-gpu/r8-fx-budget.json` | 0 | JSON + log written |

Device tests assert the exact budget counters (not just "it rendered"):
- 5000 emitted, cap 256 → `rendered=256`, `dropped_budget=4744`, `draws=1`, `vertices=1536`;
- byte cap first: `max_upload_bytes = 240×10` → `rendered=10`, `dropped=990`;
- mixed blends → `draws=2`; one-stream cap (`max_draws=1`) → alpha stream dropped and counted;
- Blizzard ≥ 6 waves → ≥ 12 shards per wave, all rendered, pixels changed;
- same particles + camera on a fresh renderer → identical frame hash.

Device perf (Turnip Adreno 725, Vulkan, 2414×1080, release, 120 frames per scenario):

| scenario | emitted | rendered | dropped | draws | p50 | fps | changed px |
|---|---:|---:|---:|---:|---:|---:|---:|
| empty | 0 | 0 | 0 | 0 | 1.87 ms | 533.7 | 0 |
| missiles_64 (trails+impacts) | 1099 | 1099 | 0 | 2 | 3.86 ms | 259.1 | 1 363 237 |
| blizzard_6w | 204 | 204 | 0 | 1 | 2.12 ms | 472.8 | 136 252 |
| stress_4096 | 4096 | 4096 | 0 | 2 | 8.63 ms | 115.8 | 2 391 017 |
| budget_256_from_4096 | 4096 | **256** | **3840** | 2 | 2.33 ms | 429.4 | 184 987 |

Memory: `rss_kb=72032`, `peak_kb=82148` (**80.2 MB** peak, ceiling 1536 MB).
JSON: `analysis/agent-gpu/r8-fx-budget.json` (parsed + asserted: every `rendered <= budget_max_particles`,
cap scenario `dropped_budget == 3840`, `frame_hash` per scenario, `adapter` block).

## Result
Row #12/#18 renderer half delivered and device-proven: projectiles have a visible arc/trail path
(`projectile_trail` + `impact_burst`), spells have a visible ≥6-wave particle path (`blizzard_wave`),
and the FX pass has a hard, machine-checked budget — 2 draws max for both blends, exact drop counters,
4096 particles in 8.63 ms p50 (115.8 fps) and a 256-particle cap in 2.33 ms p50 (429 fps) at 2414×1080.
No regressions: parity/cache/astc device suites and wc3-game/wc3-gpu checks stay green.

Remaining for the game layer (read-only here): the simulation must emit `Particle`s from
`Missileart`/spell events and call `renderer_mesh`-owner's FX entry. The renderer API is
`Renderer::set_particles(Vec<Particle>)`; for the game's own pass loop, create
`FxRenderer::new(device, format)`, call `prepare(device, queue, &scene.view_proj, &particles, budget)`
after the unit groups and `draw(&mut pass, &mut counters)` in a pass with the depth attachment
loaded (recipe mirrored in `crates/wc3-render/src/renderer.rs:214-245`).

## Risks / Next
- `stress_4096` is fill-rate bound (4096 × ~0.35 world-unit billboards at 2414×1080, 2 draws);
  the budget API is the lever (`max_particles`/`max_upload_bytes`), proven by the 256-cap row.
- Game-side emission (sim → particles) is owned by the sim layer; renderer half is ready.
- `wc3-render` still keeps the world pass and FX in separate passes; a future single-pass merge
  could save one depth load/store, measurable via the same JSON (`passes` counter is 2 with FX).

## H2
Preflight `ev-39c6eaadb1695f3f`. Capture/experience/ledger IDs reported with the close-out.
