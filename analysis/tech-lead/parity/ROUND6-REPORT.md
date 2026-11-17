# r2-world ROUND6 — per-definition model loading / budget (ledger 348)

**Date:** 2026-11-13 18:32 MSK (session ses_e30595423ffdGkcohq9P8MIxiL, agent-world)

## Goal
TOP-1/#2: verify per-definition model loading (>=216 four-race MDX parse+upload) and model budget/eviction (<=1536 MB) with AnimatorRegistry from anim.

## p1 — Artifacts verified (re-stat)
- `analysis/agent-world/parity/models-per-def.ppm` 4320016 B, 2026-11-13 12:17:06 (observed)
- `analysis/agent-world/parity-models.log` 1547 B, 2026-11-13 12:17:30 (rc=0 gate run)
- `analysis/agent-world/parity-roster.log` 1047 B, 2026-11-13 12:17:51 (rc=0)
- `analysis/tech-lead/parity-render.md` present

## p2 — AnimatorRegistry from wc3-anim (code, owner path)
File: `crates/wc3-game/src/model_render.rs` (backed up to `.bak-round6`)
- `use wc3_anim::AnimatorRegistry;` added (L17)
- `ModelRegistry.animators: AnimatorRegistry` field (canonical parsed-model/animator cache)
- `AnimatorRegistry::new(542)` in constructor
- `build()`: clones parsed MDX once, `self.animators.insert(model, mdx.clone())`, poses via `self.animators.sequence/animator(model,"Stand")` → `world_matrices()` (single parsed MDX per key; per-instance animator creation via registry)
- Accessors: `animator_resident/capacity/evictions`

## p3 — Memory ceiling / perf
- `models_per_def` (ROUND5 fresh, rc=0): `required=216 loaded=214 meshless=2 failures=0 resident=214 memory_mb=73.4 peak_mb=73.4 evictions=0`
- 1536 MB budget ceiling: **peak 73.4 MB <= 1536 MB** (asserted in test)
- LRU budget 32 MB: `loads=214 resident=116 evictions=98 used_mb=31.7 peak_mb=32.0` (eviction exercised)
- Code now emits `PERF_MEMORY human04 resident_mb=... peak_mb=... budget_mb=... resident_models=... anim_resident=... pass=<peak<=budget>` inside `models_per_def` test.

## p4 — Gallery per race + roster CSV
- Gallery PNG (combined 4-race): `analysis/agent-world/parity/models-per-def.png` (212.6K, converted from verified PPM via ffmpeg)
- Per-race PNGs (cropped from the same verified rendered frame): `models-per-def-Human.png`, `models-per-def-Orc.png`, `models-per-def-Undead.png`, `models-per-def-NightElf.png`
- Roster CSV: `analysis/agent-world/parity/roster.csv` — 216 definition rows (170 Units + 46 Buildings, portraits included, matches matrix) from `analysis/war3_full`; 214 loaded + 2 meshless (`Units\Undead\PlagueCloud\PlagueCloud.mdx`, `PlagueCloudTarget.mdx`) consistent with ROUND5 evidence. Map roster note appended (Human04 90/37/15/15/22 from parity-roster.log rc=0).
- Mirrored to: `analysis/tech-lead/parity/`, `analysis/agent-qa/parity/`, `~/analysis/agent-world/parity/`

## Acceptance: assets_models_min raised 4->542
- `analysis/agent-qa/acceptance/spec.json:58` currently `assets_models_min: 4` — **outside OWNER_PATHS**.
- Patch delivered: `analysis/agent-world/parity/PATCH-assets_models_min.diff` (4→542).
- Runtime GAME_READY models count (`apps/android-native/src/lib.rs:820`, legacy `assets.models.len()`) outside owner; patch note: `analysis/agent-world/parity/PATCH-android-models-count.md`.

## Compile verification (blocked, external)
`cargo check -p wc3-game --lib` RC=101 due to errors in `crates/wc3-re/src/creeps.rs` (outside owner paths): unresolved `glam` import + 3× `ReError::Unsupported(&str)` type mismatches. Prevents re-running tests on the ROUND6-modified crate in this tree state. Details: `analysis/tech-lead/BLOCKED-r2-world-round6.md`.

## Evidence (commands/logs)
- `analysis/agent-world/parity-models.log`: ROUND5 rc=0, 3 passed (59.8s) — MODELS_PER_DEF 216/214/2/0/73.4MB, LRU 32MB 98 evictions, gallery 32 rendered.
- `analysis/agent-world/parity-roster.log`: ROUND5 rc=0 — Human04 90 placed/37 distinct/15 resolved/15 loaded.
- `ffmpeg -y -i models-per-def.ppm models-per-def.png` rc=0 (212.6K).
- Roster generation from `analysis/war3_full`: 216 definitions enumerated (observed).

## Result
**Partial (verified artifacts + code complete; compile re-gate blocked by external wc3-re break).** p1/p3/p4 artifacts present with rc=0 historical gate evidence; p2 AnimatorRegistry integration implemented; patches for out-of-owner acceptance items delivered. Re-gate required once wc3-re compiles.

## Risks/Next
- Re-run `cargo test -p wc3-game --lib models_per_def -- --ignored` and `models_per_def_roster_csv` after wc3-re fix to confirm ROUND6 code compiles and emits PERF_MEMORY/per-race PNGs/roster via tests.
- Apply `PATCH-assets_models_min.diff` in qa/acceptance owner; wire registry count into GAME_READY (android/assets owners).
- 21 creep/neutral Human04 ids pending CREEP_DEFS; `unp1` TFT model absent in RoC tree (known tails).
