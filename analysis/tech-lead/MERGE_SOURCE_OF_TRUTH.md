# MERGE = SOURCE OF TRUTH (hard, all agents)

Effective 2026-11-12. The merge agent (`ses_e2ffefc8cffe6MFNhz0SGVhu8E`) owns the canonical base.

1. Before writing code, read `analysis/tech-lead/WC3RE_MERGE_MATRIX.md`. Any module that exists there must be consumed, not re-implemented.
2. No second reader/parser/VM/renderer copy. If you need behavior that lives in the merged base, call it or extend it there.
3. Full refactor only: no stubs, no TODO, no placeholder, no "temporary" fallback paths. Changed functions are rewritten whole.
4. One writer per file. HUD owns mobile_hud/game/hud/ui; render owns terrain_render/render/model_render; anim owns wc3-anim/wc3-formats-mdx; merge owns the canonical crates.
5. Acceptance is device artifacts for visual work; for base/refactor work it is tests + `wc3re scan` counts + `cargo check -p wc3-game` + APK build.
6. The UI/UX GitHub research agent is exempt from the merge dependency; its output feeds the HUD design only.
