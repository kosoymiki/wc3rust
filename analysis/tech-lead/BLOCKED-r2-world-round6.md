# BLOCKED — r2-world ROUND6 compile verification (2026-11-13)

## Symptom
`cargo check -p wc3-game --lib` / `cargo test -p wc3-game --lib models_per_def` cannot complete in current tree state.

## Evidence
Command: `ulimit -v 4500000; CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_TARGET_DIR=$PWD/target-world cargo check -p wc3-game --lib`
RC: 101

Errors (outside OWNER_PATHS `crates/wc3-game/src/{terrain_render,render,model_render,doodads}.rs`):
- `crates/wc3-re/src/creeps.rs:16` E0432 unresolved import `glam` (`use glam::Vec2`)
- `crates/wc3-re/src/creeps.rs:155,159,162` E0308 mismatched types: `ReError::Unsupported(&str)` passed where `String` expected
- Result: `error: could not compile wc3-re (lib) due to 4 previous errors`

## Impact
Cannot re-run `models_per_def`/`map_roster_models` gates on the ROUND6-modified `wc3-game/src/model_render.rs` (AnimatorRegistry integration, PERF_MEMORY line, per-race gallery + roster CSV tests) in this generation. Last verified gate run: ROUND5 2026-11-13 12:16-12:17, rc=0 (see `analysis/agent-world/parity-models.log`, `parity-roster.log`).

## Next step (owner: wc3-re/sim maintainers)
1. Add `glam` dependency to `crates/wc3-re/Cargo.toml` or remove `use glam::Vec2` in `crates/wc3-re/src/creeps.rs`.
2. Convert `ReError::Unsupported("...")` to `ReError::Unsupported("...".to_string())` (or adjust variant) at creeps.rs:155,159,162.
3. Re-run: `cargo check -p wc3-game --lib --tests` then `cargo test -p wc3-game --lib models_per_def -- --ignored --nocapture` and `cargo test -p wc3-game --lib models_per_def_roster_csv -- --ignored --nocapture`.
