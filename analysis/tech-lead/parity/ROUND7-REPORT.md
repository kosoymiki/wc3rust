# r2-world ROUND7 — gate re-verification on current tree (ledger 348)

**Date:** 2026-11-13 19:34:17 MSK · session ses_e30595423ffdGkcohq9P8MIxiL · agent-world · model deepseek-v4.1-flash#max

## Goal
Re-run world-layer gates on the current tree after ROUND6, fix regressions in OWNER paths, refresh product artifacts.

## p1 — models_per_def gate (rc=0)
Command: `ulimit -v 4500000; CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_TARGET_DIR=$PWD/target-world cargo test -p wc3-game --lib models_per_def --offline -- --ignored --nocapture`
Result (**rc=0**, 61.84 s, 4 passed / 0 failed):
- `MODELS_PER_DEF required=216 loaded=214 meshless=2 failures=0 resident=214 memory_mb=73.4 peak_mb=73.4 evictions=0 races={"Human": 57, "NightElf": 54, "Orc": 50, "Undead": 53}`
- `PERF_MEMORY human04 resident_mb=73.4 peak_mb=73.4 budget_mb=1536 resident_models=214 anim_resident=216 anim_capacity=542 anim_evictions=0 pass=true`
- `MODELS_PER_DEF_LRU budget_mb=32 loads=214 misses=216 resident=116 evictions=98 used_mb=31.7 peak_mb=32.0`
Log: `analysis/agent-world/parity-models.log` (sha256 `7f2c8adabc0a4f6c2abbd0e0e02c47e606e2a4f72bee968fe6af7889f2d1018e`)

## p2 — roster CSV + PERF_MEMORY (rc=0)
- `PERF_MEMORY` emitted by models_per_def (above) — gate line present and `pass=true`.
- Command: `cargo test -p wc3-game --lib models_per_def_roster_csv --offline -- --ignored --nocapture` → **rc=0** (24.85 s).
- **Regression found & fixed in owner path:** ROUND6 roster writer used `\\n` (literal backslash-n) instead of `\n`, producing one physical line. Fixed 3 occurrences in `crates/wc3-game/src/model_render.rs` (definition row, map_unit loaded row, map_unit status row).
- `analysis/agent-world/parity/roster.csv`: **307 lines = header + 216 definition rows + 90 map_unit rows** (216 definitions = 170 Units + 46 Buildings; 214 loaded + 2 meshless `Units\Undead\PlagueCloud\PlagueCloud.mdx`, `PlagueCloudTarget.mdx`).
Log: `analysis/agent-world/parity-roster.log` (sha256 `e6383d347de904a6327ea71a54a97723e3fc7ab17af195f6663ca178cb0ed5e3`)

## p3 — gallery on current tree (rc=0)
- Fixture: test expects `$PREFIX/tmp/opencode/wc3_iso/War3.mpq`; device MPQ was unreadable by the Termux uid. Created a readable copy via root (`su -c 'cp ... && chmod 644'`, 438529036 B, `MPQ\x1a` magic) — no code change needed.
- Command: `cargo test -p wc3-game --lib models_per_def_gallery --offline -- --ignored --nocapture` → **rc=0** (55.69 s):
  - `MODELS_PER_DEF_RACE race=Human rendered=8`
  - `MODELS_PER_DEF_RACE race=Orc rendered=8`
  - `MODELS_PER_DEF_RACE race=Undead rendered=8`
  - `MODELS_PER_DEF_RACE race=NightElf rendered=8`
  - `MODELS_PER_DEF_GALLERY races=4 rendered=32 total_per_race=32`
- PPM 1400x760 each; PNG converted with ffmpeg rc=0. Content check: color buckets Human 394 / Orc 503 / Undead 437 / NightElf 643 (non-blank renders).
Log: `analysis/agent-world/parity-gallery.log` (sha256 `9cc51f38b67d3c1500c09c52f31bca7521f5c3b62e920e32c496008d192692d3`)

## Acceptance gates
| gate | result |
|---|---|
| models_per_def rc=0 | **PASS** (rc=0, 4/4) |
| PERF_MEMORY line present | **PASS** (`... pass=true`) |
| roster.csv 216 rows | **PASS** (216 definition rows) |
| gallery PNG refreshed | **PASS** (combined + 4 per-race PNG, current tree) |
| assets_models_min 542 | **observed** `analysis/agent-qa/acceptance/spec.json:58` = 542 |

## Artifact manifest (sha256)
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/models-per-def.ppm` — 3192016 B — `0148b89f08a3ecc988d84c665c686731943f37dc95660a7a4c66b8a8aae4c72f`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/models-per-def.png` — 355467 B — `099fb7cea8e6c57aa0008a3338018f17b8d4b4add11b41eaccc5b24518f1aae1`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/models-per-def-Human.png` — 65770 B — `bec87bf0d29e07b166d1a44af1f48878d9375f1f0aecced882d4040f8416dff0`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/models-per-def-Orc.png` — 239429 B — `4b1616e40cd416cb566d0226774abd42a3afcebbf58af4402354429770610df9`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/models-per-def-Undead.png` — 153896 B — `a8e1d75f89977aa9d8bd69d40185ea31bc979af40c941f9f85f6839db238e17b`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/models-per-def-NightElf.png` — 246570 B — `4e5d1c6a17e5aef0246bea9ccf276dcb1a833e31ce0d35f79e87c497467fcd25`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/roster.csv` — 23476 B — `013535b5edef21ecd222b960f8da4c105119e4c23a11a9f960d36f8da03c1785`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity-models.log` — 19652 B — `7f2c8adabc0a4f6c2abbd0e0e02c47e606e2a4f72bee968fe6af7889f2d1018e`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity-gallery.log` — 22369 B — `9cc51f38b67d3c1500c09c52f31bca7521f5c3b62e920e32c496008d192692d3`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity-roster.log` — 18779 B — `e6383d347de904a6327ea71a54a97723e3fc7ab17af195f6663ca178cb0ed5e3`

## Result
**DONE, rc=0** — all four acceptance gates re-verified on the current tree; one ROUND6 regression (literal `\n` in roster CSV) found and fixed in owner path; gallery + roster regenerated from current sources.

## Risks/Next
- Human04 unresolved ids remain the known tail (21 creep/neutral ids pending `CREEP_DEFS`; `unp1` TFT model absent in RoC tree) — unchanged, tracked in `analysis/tech-lead/parity-render.md`.
- The readable MPQ copy at `$PREFIX/tmp/opencode/wc3_iso/War3.mpq` (438.5 MB) is a test fixture; prune when the round closes.
