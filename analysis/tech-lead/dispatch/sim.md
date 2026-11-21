# THICK CONTINUATION — SIMULATION layer

You are reactivated by the DISPATCHER (tech-lead root). The operator's order: one base, not two. Every layer keeps working while merging.

## Authority
- `analysis/tech-lead/BASE.md` (v1, published 2026-11-12) is the canonical target map: crate graph, one owner per concern, public API, original-game data provenance, write-freeze list.
- `analysis/tech-lead/WC3RE_MERGE_MATRIX.md` records the RE merge decisions.
- `crates/wc3-re` is the public RE/engine API, not a second base. `crates/wc3-mpq` owns MPQ+codecs; `crates/wc3-formats` owns format primitives; `wc3-re` owns the resource/engine layer. Consume them; do not create a parallel reader.

## Hard constraints (operator)
1. Finish your layer, then port it onto the single base. No second implementation of any reader/parser/renderer/sim.
2. DO NOT run `cargo test`, `cargo build`, APK installs, screenshots or device automation until the dispatcher announces BASE.md freeze. Implementation, patches, docs and manifests continue NOW.
3. One writer per file. You own only your layer paths (below). Everything else is READ-ONLY: deliver a ready patch + exact lines in your manifest instead of editing.
4. No stubs/TODO/placeholders. Full implementations. Evidence only: source `file:line` -> target `file:line`.
5. No invented/procedural data in the base. Read the original files through canonical parsers; label any fallback `source=fallback`.
6. H2: capture/experience + monitor checkpoint at the end; report IDs if available.

## Your layer goal
Merge sim into the game API (supply/farm/AI) with no second sim. `wc3-sim` is canonical; `wc3-game/game.rs` must consume it, not reimplement combat/economy. Port the stronger mechanics into `wc3-sim` and leave a facade. JASS can drive the canonical sim through `wc3-re::jass` world/engine; no parallel unit/trigger state.

## OWNER paths (write)
crates/wc3-sim/**, crates/wc3-game/src/game.rs, analysis/agent-sim/**, analysis/tech-lead/merge-sim.md

## READ-ONLY (patch-only)
crates/wc3-re/src/jass/**, crates/wc3-game/src/re.rs

## Deliverable — port manifest (write it now, keep it current)
`analysis/tech-lead/merge-sim.md` with:
- canonical target per concern (crate/module/type/function) and why
- OWNER files changed; READ-ONLY patches with exact line ranges
- every symbol ported: source `file:line` -> target `file:line`
- duplicates removed/thinned and the delegation proof (type identity, parity fixture, or diff)
- public API the game/base consumes (exact signatures)
- tests to add after freeze (names + fixtures), not run yet
- conflicts with other layers/files; exact next step
- state: DONE | PARTIAL | BLOCKED + evidence paths

## Operating loop
Inspect the current tree and BASE.md -> implement the missing piece fully -> write/refresh the manifest -> reply with the manifest path and one-line state. If you find a conflict, do not edit the other owner's file: put the patch in the manifest and name the file/owner.
