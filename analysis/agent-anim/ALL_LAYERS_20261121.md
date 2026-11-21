# ALL-LAYERS 2026-11-21 — Agent D: animation / geoset

Ledger 600 · owners `crates/wc3-anim/**`, `crates/wc3-anim/tests/artD_*` · repo `~/src/herofolends-rs`
(`~/src/wc3rust`). Oracle: the shipped v800 tree `analysis/war3_full3` (2 048 MDX, byte-identical to
`analysis/war3_full`; verified with sha256 on the pinned models).

## Goal

Prove per-sequence geoset visibility and the build-ghost geoset set against the shipped v800 MDX —
sequence alpha sets, `SEQS` record counts (`SEQS`/132 B), no "all geosets drawn" regression — and fix
what the evidence shows broken. A layer is done only when named tests pass and the oracle agrees.

## Mechanism (read, file:line)

| what | where |
|---|---|
| per-geoset visibility rule | `crates/wc3-anim/src/runtime.rs:53-116` — extent gate (`present_in_sequence`) then in-range `KGAO` sample, else static alpha; no sequence → static alpha only |
| sequence-scoped track sampling | `crates/wc3-anim/src/lib.rs:649-711` (`sample_in_sequence`: clip to `[start,end]`, non-looping clamp on `flags & 1`, looping wrap), `713-725` (step/linear/hermite/bezier) |
| KGAO constant rule | `crates/wc3-anim/src/lib.rs:971-996` (single in-range key is a constant; none → static) |
| per-sequence extent flag | `crates/wc3-formats/src/mdx.rs:335-354` (`Extent::is_empty` sentinel `min > max` or non-finite radius; missing data = present) |
| `SEQS` parse | `crates/wc3-formats/src/mdx.rs:681-693` — `while r.remaining() >= 132`; record = name[80] + interval[2] + moveSpeed + flags + rarity + syncPoint + extent(radius+min+max = 28 B) = **132 B** |
| ghost input | `crates/wc3-game/src/game.rs:7281-7286` calls `Assets::mesh_alpha(model, None, 0)`; `crates/wc3-game/src/assets.rs:3114-3122` resolves `None` → `model.idle` (first sequence whose name contains `stand`) → Stand geoset set |

## What the cross-check found

An independent Python parser (`analysis/agent-anim/probe_mdx.py`, byte-level, not the Rust crate) parses
every shipped MDX; the port's view is dumped by `crates/wc3-anim/examples/artD_dump.rs`; the Python side
recomputes the documented alpha rule from the raw bytes (helper plane, extent sentinel, `KGAO` step/
linear/hermite/bezier/global, static alpha) and compares every sequence at start/mid/end.

| check | result |
|---|---|
| models parsed by both sides | **2 048** |
| parse errors | **0** |
| `SEQS` payload % 132 / parsed sequence count | **0 mismatches**, 6 512 sequences |
| `TEXS` payload % 268 | **0 mismatches**, 5 216 textures |
| `GEOS`/`GEOA` self-sized record counts vs parser | **0 mismatches**, 5 127 geosets / 3 093 GEOA |
| per-sequence alpha sets (start, mid, end) | **0 mismatches** over 33 354 (sequence, geoset) slots |
| `KGAO` track mix (all compared) | step 2 837, linear 47, bezier 10, global 3, hermite 0 |
| buildings with a Stand frame (ghost default) | 125 checked, 82 extent-gated, **0 excluded-geoset leaks** |
| buildings that legitimately draw all geosets in Stand | 8 (`excluded_from_stand = 0`; fountains/gates — no upgrade stages authored) |

Model counts agree with the earlier layer audits: TownHall 36 geosets / 14 sequences (engine map T-9),
GreatHall 19/14, Footman 5/13, Archer 4/13, Peon 5/22.

Pinned ghost/Stand frames: GreatHall visible **geosets [2, 3, 13] of 19** (3 geosets, 4 draws),
Peon Stand `[1.0, 1.0, 0.0, 0.0, 0.0]` (2 geosets, 3 draws) — the "after" rows of
`analysis/agent-render/MODEL_LOD_REPORT.md` §3.

Oracle fixture: `crates/wc3-anim/tests/fixtures/artD/crosscheck.tsv` (63 rows: Peon, Footman,
GreatHall, TownHall × every sequence × 3 sample times; sha256
`9aa0e2ae9f74e4b61251f14b097fe7da27eea6c1fadeefd8e53c49f73f8d946b`). The Rust test checks the exact
alpha trajectory without running Python.

## Defects fixed (in owner paths)

1. **Copied `Sequence` could draw every geoset** — `mesh_alphas` resolved the sequence only by
   `(start, name)`; any reconstructed/copied sequence (saved state, synthetic interval) missed the
   lookup, took the no-sequence branch and returned static alphas = all geosets, recreating the exact
   regression the rule exists to prevent. Fix: fall back to name resolution before the static path
   (`crates/wc3-anim/src/runtime.rs`, `mesh_alphas`), pinned by `artD_foreign_sequence_gate`
   (Peon: carry/gore stay 0; Footman: no geoset outside the Stand extent becomes visible).
2. **`artD_dump` panicked on shipped data** — some v800 sequences carry `end < start`; the example's
   `end - start` overflowed `u32` (rc=101 at the 88th model). Fix: saturating span in the dump tool.
   The crate itself already treated the interval as plain times (no panic, tests green).

No other defect: per-sequence visibility, `SEQS`/`TEXS` record counts and the whole shipped alpha
matrix already agreed with the bytes.

## Tests (named, run in this session)

| test | asserts |
|---|---|
| `artD_seq_records_132` | raw chunk walk over 2 048 MDX: clean MDLX stream, VERS 800, `SEQS` 132 B records == parsed count, `TEXS` 268 B, `GEOS`/`GEOA` self-sized record counts == parsed; totals 6 512 / 5 127 / 3 093 / 5 216 |
| `artD_sequence_alpha_sets_match_oracle` | 63-row Python-oracle fixture: exact per-sequence alpha trajectories at start/mid/end for Peon, Footman, GreatHall, TownHall; `ModelRuntime::alphas == mesh_alphas` |
| `artD_sequence_extent_gate` | whole tree, 6 512 sequences × 3 samples: helper planes never visible, extent-excluded geosets never visible, the 3 098 gated sequences never draw every geoset |
| `artD_ghost_no_all_geosets` | 125 buildings: ghost Stand frame leaks no excluded geoset, 82 gated buildings draw a strict subset; GreatHall `[2,3,13]`, Peon `[1,1,0,0,0]` |
| `artD_foreign_sequence_gate` | copied `Sequence` resolves extents by name instead of the all-geosets static fallback |
| `artD_sequence_alphas`, `artD_ghost_sequence` (existing) | Peon/GreatHall/TownHall tables and 6↔3 / 20↔4 draw counts |

## Verification (exact rc)

```
scripts/h2-cargo test -p wc3-anim --offline
RC=0   12 test binaries, 45 passed, 0 failed, 2 ignored
       artD_animation: 2 passed
       artD_crosscheck: 5 passed (seq_records_132, sequence_alpha_sets_match_oracle,
                                  sequence_extent_gate, ghost_no_all_geosets, foreign_sequence_gate)
       lib: 28 passed
scripts/h2-cargo test -p wc3-anim --offline --test game_path_sweep --test object_id_indexing -- --ignored
RC=0   every_model_animates_through_the_runtime ok (542-model roster), object_id_indexing_holds_for_every_model ok
scripts/h2-cargo run -p wc3-anim --offline --example artD_dump -- analysis/war3_full3   RC=0 (2 048 M records, 6 512 S records)
python3 analysis/agent-anim/artD_crosscheck_20261121.py --tree analysis/war3_full3 --dump … RC=0 (0 mismatches)
```

Logs: `/data/data/com.termux/files/usr/tmp/opencode/artD_full_20261121.log`,
`…/artD_crosscheck_20261121.log`, `…/artD_ignored_sweeps.log`; audit JSON
`analysis/agent-anim/artD_audit_20261121.json`; dump `…/artD_dump_20261121.tsv`.

## Result

**Done.** Per-sequence geoset visibility and the ghost Stand set are proved against 2 048 shipped v800
MDX files / 6 512 sequences / 33 354 alpha slots with 0 mismatches; `SEQS/132` and `TEXS/268` record
counts match the bytes; the "all geosets drawn" regression is guarded at three levels (extent gate over
the whole tree, ghost Stand over 125 buildings, copied-sequence resolution). `wc3-anim` suite rc=0.

## Risks / next

- **HONEST ART:** headless evidence only (parser/runtime/oracle). No device frame was captured here;
  the device slot is owned by agent G. The pinned draw counts (Peon 3, GreatHall 4) are the renderer's
  geoset×layer arithmetic, not a screenshot claim.
- **Cross-layer note (not edited, owners unchanged):** the ghost contract depends on
  `Assets::mesh_alpha(None) → model.idle` (`assets.rs:3114-3122`) and `model.idle` being the first
  name containing `stand`. Both were observed correct; if agent B changes that resolution, keep
  `artD_ghost_no_all_geosets` green.
- 8 buildings draw all geosets in Stand because the MDX authors no excluded stage geosets for them
  (`excluded_from_stand = 0`) — correct, not a leak.
- Some shipped tracks carry unsorted keys (`GAME_DATA_SKIP reason=mdx_track_unsorted`, 8 diagnostics in
  the run); the sampler handles them (sequence clip + clamp/wrap) and the cross-check compared those
  models with 0 mismatches.

## H2 persisted

- corpus preflight: `ev-9fb369198fa5da5f`
- capture: **1243** (`alllayer-animation-20261121`)
- experience: **4902** (SEQS/132 cross-check loop), **4903** (copied Sequence all-geosets fix), **4904** (ghost idle/Stand contract numbers)
- monitor ledger **600**: checkpoints at corpus/baseline, dump rc=101 fix, tests-green, final close
