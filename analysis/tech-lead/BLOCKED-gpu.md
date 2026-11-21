# BLOCKED-gpu.md — Round 6 ledger 352 (RESOLVED)

## Prior blocker (resolved 2026-11-13)
`crates/wc3-re/src/creeps.rs` compile errors (glam import, ReError::Unsupported &str) blocked wc3-game/bench release builds. **Resolved**: `cargo check -p wc3-re` now rc=0 (0 errors, 37 warnings). Release bench built and r6 baselines produced.

## Current blockers (outside owner scope, recorded)
- ≥216 four-race MDX asset loading: `crates/wc3-game/src/assets.rs` frozen/dispatcher-owned (PARITY_MATRIX #1).
- `crates/wc3-render/src/mesh.rs` integration of `ResidentSet` residency: coordinate with crate writer (integration recipe in `analysis/tech-lead/merge-gpu.md` §0b).
- World terrain pass (~11.16 ms, 59% of frame at 147u real-asset scene) is owner `crates/wc3-game/src/terrain_render.rs`, not this layer.
