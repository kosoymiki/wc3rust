# FULL GOAL 374 — four races fully playable in melee

Ledger **374**, layer sim, 2026-11-14. End state: race completeness, AI
completeness, full tech (89 upgrades), matrix counters, full suite green.

## Per-race completeness (`melee.race_*`)

Each test builds every roster building through the real tech tree
(`Requires=` recursively; top hall denied before the tier-2 hall), trains every
unit from its real producer (gold/lumber cost asserted), verifies supply, upkeep
and worker repair, and asserts produced counts. Buildings include upgrade-only
rows (`uzg1`) placed once its `Requires=` exists. Ability-only units
(`hmil`, `ushd`, `ehpr`) are asserted as ability-produced, not trained.

| Race | Test | Buildings built | Units trained | Ability-only | Upgrades | Repair | Command | rc |
|---|---|---|---:|---:|---:|---:|---|---:|
| Orc | `melee_race_orc` | 11 | 13 | 0 | 19 | yes | `cargo test -p wc3-sim --test melee_races` | 0 (4 passed) |
| Human | `melee_race_human` | 14 | 14 | 1 (`hmil`) | 21 | yes | same | 0 |
| Undead | `melee_race_undead` | 13 | 13 | 1 (`ushd`) | 18 | yes | same | 0 |
| NightElf | `melee_race_nightelf` | 12 | 13 | 1 (`ehpr`) | 22 | yes | same | 0 |

Repair mechanic: `Order::Repair` (`lib.rs:189`), `Command::Repair`, channel in
`advance_repairs` (`lib.rs:4313`) at `REPAIR_COST_PERMILLE=500` (`lib.rs:69`,
gold+lumber charged per healed HP). Supply/upkeep asserted in the same tests.

## AI completeness (`ai.race_builds_and_expands`)

Fixed seed per race; Human player with a defended base vs the race AI.
Asserts for all four races: `expanded=true`, `waves>=1`, ≥3 AI buildings,
unit producer present, ≥5 workers rebuilt, ≥2 distinct combat unit defs and
≥6 army units, and at least one `AiMode::Retreat` transition.

AI fixes: recursive unit/producer `Requires=` tech (`ai_tech`, `ai.rs:322`),
one construction at a time + builder resume (`ai_resume_build` `ai.rs:402`),
expansion worker protected from reassignment, expansion target = nearest mine
>900 from hall, site search keeps structures away from units
(`site_clear_of_units` `ai.rs:863`), camp placement moved away from worker/hall
sites, and an unstuck relocation for units boxed in by footprints
(`nearest_standable` fallback in `advance_movement`).

Command: `cargo test -p wc3-sim --test ai_race` → **rc 0** (1 passed; 4 races).

## Full tech (`upgrades.execution`)

`analysis/agent-sim/gen_upgrades.py` now emits **all 89** shipped UpgradeData
rows (80 melee + 9 neutral/naga/demon/unknown mapped to Neutral). Orphan rows are
researchable at the race main hall (documented fallback); every melee row
researches at its real `Researches=` building with cost/level/event asserted.
Building tiers are checked for all four races.

Command: `cargo test -p wc3-sim --test upgrades_execution` → **rc 0** (1 passed;
89/89 researched, tier gates enforced).

## Melee matrix counters (`melee.matrix`)

`cargo test -p wc3-sim --test melee_matrix melee_matrix_4_races_sizes_1_4`
→ **rc 0** (64 cases). `analysis/tech-lead/melee_matrix.json` now includes
`race_tech` and stays `ok:true`:

```json
"race_tech": {
  "Orc": {"units":13,"buildings":11,"trainable":13,"upgrades":19},
  "Human": {"units":14,"buildings":14,"trainable":14,"upgrades":21},
  "Undead": {"units":13,"buildings":13,"trainable":13,"upgrades":18},
  "NightElf": {"units":13,"buildings":12,"trainable":13,"upgrades":22}
},
"cases":64,"seats":320,"halls":320,"start_mines":320,
"expansion_mines":256,"workers":1600,"creeps":1600,"camps":576,"ok":true
```

## Acceptance evidence

| Check | Command | rc | Counts |
|---|---|---:|---|
| Full sim suite | `cargo test -p wc3-sim --offline` | **0** | **176 passed / 0 failed / 2 ignored** (pre-existing perf only; no added `#[ignore]`) |
| Game lib | `cargo check -p wc3-game --lib --offline` | **0** | — |
| QA parity | 24-gate harness | **0** | **24/24 PASS** (`full-goal-374-parity.log`) |
| Race tests | `cargo test -p wc3-sim --test melee_races` | **0** | 4 passed |
| AI | `cargo test -p wc3-sim --test ai_race` | **0** | 1 passed (4 races) |
| Upgrades | `cargo test -p wc3-sim --test upgrades_execution` | **0** | 1 passed (89 rows) |
| Matrix JSON | matrix test | **0** | `ok:true`, 64 cases |

## Risks / Next

- Multi-seat melee still uses the sim's two owner slots as teams (team-shared
  resources); true >2-owner games need the owner-array refactor.
- AI expansion requires a worker to reach an expansion mine; expansion camps are
  deliberately placed away from hall sites so guards do not kill builders.
- Repair cost/rate have no MiscGame row in this install (observed); constants
  are documented (`REPAIR_COST_PERMILLE=500`, `REPAIR_HP_PER_TICK=8`).
- Device not granted (`H2_DEVICE_SLOT` unset); no screenshots claimed.
