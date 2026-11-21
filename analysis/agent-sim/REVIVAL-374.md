# REVIVAL 374 — P0-5 upkeep / melee matrix, P0-6 heroes, abilities execution

Ledger **374**, layer sim, 2026-11-14. Tree green after every phase; acceptance
command `cargo test -p wc3-sim` **rc 0** (147 passed / 0 failed / 2 pre-existing
perf ignored).

## Phase A — `economy.upkeep`

| Item | Location |
|---|---|
| `UpkeepTier {None, Low, High}` + `tax_permille/income_permille/label` | `crates/wc3-sim/src/lib.rs:110` |
| Thresholds 50/80 food, tax 300/600 permille | `lib.rs:143` |
| `Sim::upkeep(owner)`, `Sim::upkeep_label(owner)` | `lib.rs:4469`, `:4485` |
| Gold deposit taxed | `lib.rs:4382` `deposit_gold` |
| Lumber deposit taxed | `lib.rs:2635` `deposit_lumber` |

Test: `upkeep_tax_boundaries_50_80` (`crates/wc3-sim/tests/upkeep.rs:21`).
Exact deltas for 100 income: 49→100, 50→100, 51→70, 79→70, 80→70, 81→40 for
both gold and lumber; tier + HUD label asserted at each point.
Command: `cargo test -p wc3-sim --test upkeep upkeep_tax_boundaries_50_80 --offline` → **rc 0** (1 passed).

Data note (observed): the extracted `MiscGame.txt` in this install contains
revive/XP constants but **no upkeep row**; 50/80 and 30/60% are the WC3 TFT
gameplay constants, implemented as named constants with provenance. RE
`GameplayConfig` currently has no upkeep fields — interface request noted below.

## Phase B — `melee.matrix`

| Item | Location |
|---|---|
| `SeatController {Human, Ai(AiConfig)}` | `lib.rs:452` |
| `Sim::setup_melee(seats, map_seed)` 1v1..4v4 | `lib.rs:3189` |
| Layout core (halls, start mines, 5 workers/seat, 5 neutral guards/mine, expansions) | `lib.rs:3211` |
| Free-site/footprint search | `lib.rs:3378` |
| `MeleeSetup` counts (seats/halls/mines/workers/creeps/camps) | `lib.rs:481` |

Per seat: main hall, one starting gold mine, **5 workers**, and one camp of
**5 neutral guards** (aggro 400/leash 600). Per side (team): **2 expansion
mines**, each with a 5-creep guarded camp. Seats with `Human` join owner 0,
`Ai` seats join owner 1 (2-owner sim = two teams sharing team resources).

Test: `melee_matrix_4_races_sizes_1_4` (`crates/wc3-sim/tests/melee_matrix.rs:31`).
All 4 player races × 4 AI races × sizes 1..4 = **64 cases**; asserts halls =
start mines = seats, 5 workers/seat, 5 creeps/seat, expansions 4/case, camps
seats+4, no non-building unit inside any building footprint, per-owner hall and
worker counts. Writes `analysis/tech-lead/melee_matrix.json`:
```
{"races":["Orc","Human","Undead","NightElf"],"sizes":[1,2,3,4],
 "cases":64,"seats":320,"halls":320,"start_mines":320,
 "expansion_mines":256,"workers":1600,"creeps":1600,"camps":576,"ok":true}
```
Command: `cargo test -p wc3-sim --test melee_matrix melee_matrix_4_races_sizes_1_4 --offline` → **rc 0** (1 passed).
Companion opening gates: `cargo test -p wc3-sim --test melee_spawn --offline` → **rc 0** (3 passed).

Game-side test expectation patch (read-only path): `analysis/agent-sim/patches/game-melee-camp-count-374.patch` (camps 4→6 in the 1v1 opening), `patch --dry-run -p1` **rc 0**.

## Phase C — `heroes.leveling`, `heroes.revive`

| Item | Location |
|---|---|
| Skill point per level (start 1, +1/level) | `lib.rs` Unit `skill_points`, `apply_hero_level` |
| `Command::LearnAbility` + apply | `lib.rs:561`, `:1150` |
| `Sim::learn_ability` (ability must be on hero, `reqLevel` gate) | `lib.rs:3400` |
| Generic runner gates unlearned hero abilities | `crates/wc3-sim/src/abilities.rs` `cast_data_ability` |
| Altar revive 700 g / 150 s + scheduled revival | constants `lib.rs:61,63`; `revive_hero` `:3433`; `advance_revival` `:3467` |
| Race altar lookup (`halt/oalt/uaod/eate`) | `crates/wc3-sim/src/data.rs:460` |

Ultimates come from `AbilityData.slk` `reqLevel` = 6 (`AHre`, `AHav`, `AUan`,
`AEtq`), now emitted into `GenericAbility.req_level` by
`analysis/agent-sim/gen_abilities.py`.
Revive constants read from `analysis/war3x_local/Units/MiscGame.txt:8-10`
(`HeroMaxReviveCostGold=700`, `HeroMaxReviveTime=150`).

Tests:
- `heroes_skill_points_ultimate_at_6` (`tests/heroes.rs:6`) — learn via command, effect runner gating, ultimate denied before 6 / allowed at 6, level-scaled HP/mana/damage.
- `heroes_revive_altar_700g_150s` (`tests/heroes.rs:54`) — cost 700, 4500 ticks, hero alive at altar, `HeroRevived` event.
Command: `cargo test -p wc3-sim --test heroes --offline` → **rc 0** (2 passed).

## Phase D — `abilities.execution`

RE typed classification is available and consumed **read-only**:
`crates/wc3-re/src/abilities.rs:81` `AbilityClass`, `:114` `AbilityDef`,
`:419` `defs_for_root`, `:445` `summary_for_root`.

| Consumption | Location |
|---|---|
| `typed_ability_defs(root)` | `crates/wc3-sim/src/abilities.rs:80` |
| `typed_ability_summary(root)` | `abilities.rs:86` |
| Generated data table | `abilities_gen.rs` 763 rows (all non-empty AbilityData ids) |

Test: `abilities_execution_typed_re_classification_763`
(`tests/abilities_execution.rs:9`): RE rows = 763, `executable=428`,
`data_only=335`, every generated id present in RE typed rows, generic castable
subset = 332. Command: `cargo test -p wc3-sim --test abilities_execution --offline` → **rc 0** (1 passed).

Interface needed next (no RE edit made): runtime effect dispatch by
`AbilityDef.effect_kind()`/`effect_params` (RE `abilities.rs:131,142`) so the
generic runner applies the classified effect instead of the current DataA1
heuristic.

## Acceptance evidence

| Check | Command | rc | Counts |
|---|---|---:|---|
| sim suite | `cargo test -p wc3-sim --offline` | **0** | **147 passed / 0 failed / 2 ignored** (pre-existing perf; no `#[ignore]` added) |
| game lib check | `cargo check -p wc3-game --lib --offline` | **0** | — |
| QA parity | parity harness (24 gates) | **0** | **24/24 PASS** (`revival-374-parity.log`) |
| matrix artifact | `analysis/tech-lead/melee_matrix.json` | — | `ok:true`, 64 cases |

## Risks / Next

- Upkeep thresholds/rates are not present in the extracted `MiscGame.txt`
  (observed). Ask ledgers 350/373 to expose `GameplayConstants` upkeep fields in
  `GameplayConfig`; sim constants `UPKEEP_LOW_FOOD/HIGH_FOOD/_TAX_PERMILLE` are
  ready to be replaced by config values.
- Multi-seat uses the sim's two owner slots as two teams sharing resources; true
  >2-owner games need the owner array refactor (`resources/lumber/upgrades`
  arrays) — not in this round.
- Game test `sim_bridge::tests::melee_1v1_...` expects 4 camps; owner applies
  `patches/game-melee-camp-count-374.patch` (dry-run clean).
- Device not granted (`H2_DEVICE_SLOT` unset); no screenshots claimed.
