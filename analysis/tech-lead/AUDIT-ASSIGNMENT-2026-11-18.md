# AUDIT ASSIGNMENT — WC3RUST_FULL_AUDIT_TZ_P0-P4 (2026-11-18)

Source of truth: `/storage/emulated/0/Download/WC3RUST_FULL_AUDIT_TZ_P0-P4.md` (754 lines, 54 207 B).
Every item below maps to exactly one owner. No item is unassigned, no two lanes share a
file for writing.

## Rules that bind every audit lane

1. **No new test suites.** The operator forbade writing tests while the product is broken.
   A finding is proven by: source `file:line`, a *one-shot* command or script that
   provokes it (`cargo run --example …`, a shell/python probe, a device log), and the exact
   output. `Regression test(s)` in the template is filled with the intended test *or*
   `BLOCKED: operator test freeze` + the manual reproducer. Never add files under `tests/`.
2. **Reproducers are evidence, not deliverables.** Store them under
   `analysis/full-audit-2026-11-18/probes/<ID>/` (shell/python/rust example).
3. **Excluded backlog:** AUD-001…AUD-032 must not appear in any finding, alias, or severity
   change. A finding that overlaps must prove a *new independent root cause* or be dropped.
4. **Template + severity** exactly as TZ §6/§7. `Status` ∈ CONFIRMED|NEEDS_REPRO|
   FIXED_ON_HEAD|FALSE_POSITIVE. No invented line numbers, outputs, or benchmarks.
5. **Coverage is per file.** Each lane writes rows into `FILE_COVERAGE.csv`
   (path, subsystem, status, lines, probes, findings, reviewer, notes) for every file it
   owns. `AUDITED` requires at least one concrete observation per file.
6. **Build discipline:** `scripts/h2-cargo` only (FIFO); `H2_CARGO_PRIORITY=1` only for the
   critical-path APK. Never kill another lane's build. Device runs are the tech-lead's.
7. **Report:** per-lane file `analysis/full-audit-2026-11-18/<lane>.md` with the finding
   list, coverage counts, blocked items, and the evidence paths. Close the ledger with
   `H2_PERSISTED` + `SESSION_CLOSED`.

## Ownership matrix (TZ item → owner)

| TZ item | what | owner |
|---|---|---|
| §3.2 | release/QA gate: APK must be byte-identical to the audited source SHA; separate statuses (source build, install identity, first frame, menu visual, in-game visual, gameplay smoke) | **K** |
| §3.3 | fog-mask perf regression: verify stride mapping, edges/odd sizes/0, aliasing vs minimap, rebuild-only-on-change, 4096² cost | **J** |
| FMT-NEW-001…005 | BLP1 paletted mips, BLP2 zero-pad, JPEG dimensions, offset/size validator, JPEG allocation | **G** |
| FMT-NEW-006 | `FogMask` public constructor/index validation | **J** |
| FMT-NEW-007 | `decode_jpeg_stream` truncated output | **G** |
| FMT-NEW-008 | BLP DXT block byte-count arithmetic | **G** |
| 5.1 | workspace, build graph, deps, reproducibility | **K** |
| 5.2 | `wc3-core`, `wc3-path`, math/IDs | **K** |
| 5.3 | `wc3-formats` (BLP/MDX/MDL/SLK/W3E/WPM/doo/w3i/wts) | **G** |
| 5.4 | `wc3-mpq` (archives, crypto, listfile, sectors) | **G** |
| 5.5 | `wc3-bink`, MPEG4, MP3, AVI | **G** |
| 5.6 | `wc3-data`, object data, localization | **H** |
| 5.7 | `wc3-re` map loading, map content, JASS parser/AST | **H** |
| 5.8 | `wc3-sim` determinism, orders, economy, combat | **I** |
| 5.9 | JASS VM + native semantics | **H** |
| 5.10 | `wc3-input` gesture router, touch/mouse | **I** |
| 5.11 | `wc3-game` HUD/menu/assets/game-render bridge | **J** |
| 5.12 | `wc3-render`, `wc3-gpu`, `wc3-vulkan-loader` | **J** |
| 5.13 | Android native, SDL, Java/JNI, lifecycle | **K** |
| 5.14 | `wc3-audio`, `wc3-anim`, `wc3-assetdb`, `wc3-replay` | **I** |
| 5.15 | `wc3-adrenotools`, hooks, linker, Turnip vendor, C tools | **K** |
| 5.16 | QA/test infrastructure and analysis tools | **K** |
| Phase F | Red Team: each lane challenges the findings of its partner (G↔H, I↔J, K↔tech-lead) | all |
| §9 A–G, §10 deliverables | assembled by the tech-lead from lane reports | **TL** |

## Lanes (all read the TZ first, then this file)

### G — formats & containers (ledger 526)
Files: `crates/wc3-formats/**`, `crates/wc3-mpq/**`, `crates/wc3-bink/**`.
Start with FMT-NEW-001…005/007/008 — each is a concrete candidate; verify, do not
paraphrase. Then 5.3/5.4/5.5 file-by-file. Highest-value question: can a hostile `.blp`,
`.mpq` or `.mdx` cause OOB read, unbounded allocation, or a silently wrong texture?
Deliverable: `analysis/full-audit-2026-11-18/lane-G-formats.md` + coverage rows + probes.

### H — data, map loading, JASS (ledger 527)
Files: `crates/wc3-data/**`, `crates/wc3-re/**` (except `src/jass/vm.rs` wait-free parts you
own entirely), `crates/wc3-re/src/jass/**`.
Scope: 5.6, 5.7, 5.9. Focus: untrusted map/object data → parser/VM; native arity and
argument types; `TriggerSleepAction` suspension correctness; map content (doo/w3u/w3i/wct).
Deliverable: `lane-H-data-jass.md`.

### I — simulation, input, media (ledger 528)
Files: `crates/wc3-sim/**`, `crates/wc3-input/**`, `crates/wc3-audio/**`,
`crates/wc3-anim/**`, `crates/wc3-assetdb/**`, `crates/wc3-replay/**`.
Scope: 5.8, 5.10, 5.14. Focus: determinism (state_hash across save/load and seat binding),
order/command edge cases, gesture router mirroring, audio budget, asset cache eviction.
Deliverable: `lane-I-sim-input-media.md`.

### J — game, HUD, render (ledger 529)
Files: `crates/wc3-game/**`, `crates/wc3-render/**`, `crates/wc3-gpu/**`,
`crates/wc3-vulkan-loader/**`.
Scope: 5.11, 5.12, §3.3, FMT-NEW-006. Focus: the same defects the operator sees (black
world, placeholder models, HUD proportions) *as audit items* with impact chains, plus fog
mask validation and the camera-uniform 176 B guard.
Deliverable: `lane-J-game-render.md`.

### K — platform, build, tooling (ledger 530)
Files: `apps/**`, `third_party/wgpu-hal/**` (vendor), `scripts/**`, `Cargo.toml`,
`.cargo/**`, `tools/**`, Android Java/Manifest, C/C++ helpers.
Scope: 5.1, 5.2, 5.13, 5.15, 5.16, §3.2. Focus: stale-APK gate, reproducibility, unsafe/FFI
inventory, lifecycle, Turnip/hook loader, QA scripts that can lie.
Deliverable: `lane-K-platform-build.md`.

## Assembly (tech-lead)

After lanes report: `FINDINGS_P0-P4.md` (merged, deduped, severity-checked),
`FILE_COVERAGE.csv` (concatenated), `TEST_MATRIX.md` (what actually ran),
`SECURITY_AND_UNSAFE.md`, `PERFORMANCE_REPORT.md`, `RED_TEAM_REPORT.md`,
`FIX_PLAN.md`, `findings.json`, `FULL_AUDIT_TZ_RESULT.md` (COMPLETE/INCOMPLETE with the
SHA actually audited). Red Team phase: each lane challenges its partner's P0/P1 before the
merge; I own the final severity and the GO/NO-GO.

## Coordination with the product lanes (already running)

Product lanes keep priority on the operator's P0 product path and do not touch audit files:
518 seats (done: `MAX_PLAYERS=8`, `bind_human_seat`, save v9), 519 campaign titles/seat,
520 models/world, 521 HUD/menu art, 522 campaign content defs, 523 closed.
An audit finding that blocks those fixes goes to the tech-lead immediately.
