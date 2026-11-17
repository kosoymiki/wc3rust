# wc3-gpu — deprecated facade over `wc3-render`

Status: **facade only** (base v2 deletes the crate).

The GPU layer lives in `crates/wc3-render` — see `crates/wc3-render/src/lib.rs`
and `analysis/tech-lead/merge-gpu.md`. This crate exists so existing imports
(`wc3_gpu::mesh::…`, `wc3_gpu::GpuContext`, …) keep compiling while branches
migrate. It contains no GPU logic, no shader, no pipeline and no copy of the
renderer.

```rust
// before
use wc3_gpu::mesh::MeshRenderer;
// after (canonical)
use wc3_render::mesh::MeshRenderer;
```

`crates/wc3-gpu/src/lib.rs` is 47 lines of `pub use wc3_render::…`; its only
dependency is `wc3-render`.

Removal checklist (v2):

1. `apps/smoke`: `use wc3_gpu::GpuContext` → `use wc3_render::GpuContext`.
2. Drop `wc3-gpu` from the root workspace members and delete the directory.

Measured behaviour, budgets and the parity evidence for the renderer itself are
in `analysis/agent-perf/REPORT.md` and `analysis/tech-lead/merge-gpu.md`.
