# LEDGER 453 — AI v2 + Reforged balance overlay (2026-11-16)

## Mission A — deterministic AI v2 (`crates/wc3-sim/src/ai.rs`)

Contract: the AI only issues orders through the public `Sim` API; every
decision is derived from live state each cycle, so there is no stored
"already built" flag and no RNG.

- **Data-driven build order** (`ai_next_build_def`, `ai_plan`): supply building
  from `data::supply_building_for`, producer from the composition unit's
  `trained_at`, prerequisites from `requires` (recursive `ai_next_missing`),
  hero tech from `data::altar_for`. Re-plan is implicit: a destroyed building
  is simply missing again on the next cycle; the main hall is rebuilt around
  the last known `state.home` (`ai_rebuild_hall`).
- **Combat**: `Build` gathers the wave at `regroup_point` (600 units from the
  hall toward the enemy base) and only attack-moves once 75% of the army is
  inside 420 units of it; `Attack` retreats below `retreat_permille`, defends
  the base (`ai_defends_its_base`), cleans up stragglers, and every 60 ticks
  focuses the whole army on the lowest-HP enemy within 900 of the army centre.
- **Economy**: five workers per live mine (`MINE_SATURATION`), lumber crew
  `2 + (queued + planned lumber)/80` capped at 6 and trained on top of
  `config.workers`; damaged buildings are repaired by one free worker
  (`ai_repair`); lost workers are retrained to the crew target.
- **Expansion**: only after the main crew saturates (`crew_target =
  min(config.workers, 5 * main mines)`), picks the **safest** free mine
  (`distance + 2 * max(0, 1800 - nearest enemy)`), and builds the hall on the
  base side of the mine (`expansion_anchor`, 320 units) so the builder's
  approach is not blocked by the mine's own cells.
- **Heroes**: `ai_hero` trains the race hero from a finished altar; `ai_spells`
  already casts through `Sim::cast` (heal/nuke/shield/point spells).

### Evidence

Per-race build order (`H2_AI_DEBUG=1 cargo test --test ai_v2
ai_build_order_per_race -- --nocapture`):

```
Human:    ["htow", "hhou", "hbar", "hbla", "halt"]
Orc:      ["ogre", "otrb", "obar", "ofor", "oalt"]
Undead:   ["unpl", "uzig", "usep", "ugrv", "uaod"]
NightElf: ["etol", "emow", "eaom", "edob", "eate"]
```

Placement trace excerpt (Human):

```
[ai] t=60   race=Human plan=supply   def=hhou at=(6976,12736)
[ai] t=1140 race=Human plan=producer def=hbar at=(6592,12480)
[ai] t=2940 race=Human plan=tech     def=hbla at=(6848,12480)
[ai] t=6120 race=Human plan=altar    def=halt at=(6592,12736)
```

Expansion trace (`H2_AI_DEBUG=1 ... --test ai_race`):

```
[ai] t=8580 expand: site=(11456,8256) worker_alive=true denial=None
[ai] t=8580 expand: place result=None
[ai] t=9120 expand: site=(11328,8256) worker_alive=true denial=None
[ai] t=9120 expand: place result=None
```

Tailwind requirement: `setup_melee` lays out halls/mines/camps but no forest;
the 22k per-race fixture now spawns a two-ring forest around both bases so the
lumber crew target is exercised against real wood.

### Test evidence (20 tests, all green)

```
ai (8):              ai_defends_its_base, ai_economy_lumber_and_mine_saturation,
                     ai_expand_site_progresses, ai_expands_to_a_second_mine,
                     ai_harvests_builds_tech_and_an_army,
                     ai_is_deterministic_for_the_same_seed,
                     ai_retreats_when_the_wave_is_broken,
                     ai_sends_waves_that_reach_the_player_base
ai_race (1):         ai_race_builds_and_expands (22 000 ticks, 4 races)
ai_v2 (7):           ai.build-order.per-race, ai.build-order.replan-after-loss,
                     ai.combat.regroup-then-attack, ai.combat.focus-fire,
                     ai.economy.saturation, ai.hero.casts,
                     ai.expansion.safest-mine
balance_overlay (1): balance.overlay.applies
determinism (3):     ai_run_is_reproducible, different_seed_changes_the_damage_rolls,
                     scripted_command_log_is_bit_identical
```

## Mission B — Reforged 2.x overlay (`analysis/balance/`)

- `analysis/balance/reforged-2.x.json`: 7 roster rows, 10 ability rows,
  6 upgrade rows; each row carries `tft`, `reforged`, source id and quote.
- Sources: official Blizzard forum patch notes 2.0.2
  (https://us.forums.blizzard.com/en/warcraft3/t/35355, published 2025-04-15)
  and 2.0.4 (https://us.forums.blizzard.com/en/warcraft3/t/36567, published
  2026-01-26), accessed 2026-11-16 via stealth-browser MCP; verbatim excerpts
  in `analysis/balance/sources/`. 2.0.1 has no balance section.
- `analysis/agent-sim/gen_roster.py` reads the JSON, verifies each row against
  the live generator value / `abilities.rs` SpellDef / `upgrades_gen.rs`
  UpgradeDef, applies roster numbers before emission, and emits
  `ROSTER_OVERLAY`/`ABILITY_OVERLAY`/`UPGRADE_OVERLAY` into `roster_gen.rs`.
  Orphan probe: injecting `zzzz.stats.hp` aborts with
  `orphaned roster overlay rows (no matching def/field): zzzz.stats.hp` and
  `rc=1`.
- `analysis/balance/DIFF.md` holds the field/TFT/Reforged/source table plus the
  not-applied list (no ROSTER def or no owner field: neutral heroes, creeps,
  items, behavioural rules, and five units whose patch-note baseline conflicts
  with the local extract).

Three known applied values asserted by `balance.overlay.applies`:
`earc.stats.hp = 275` (was 260), `Ekee.stats.hero_strength_gain = 2.0` (was
1.8), `esen.stats.speed = 340.0` (was 350).

## Files

- `crates/wc3-sim/src/ai.rs` — AI v2.
- `crates/wc3-sim/tests/ai.rs` — two fixtures moved to the supply-first budget.
- `crates/wc3-sim/tests/ai_race.rs` — forest fixture + deterministic retreat
  stimulus.
- `crates/wc3-sim/tests/ai_v2.rs` — seven mechanic tests.
- `crates/wc3-sim/tests/balance_overlay.rs` — overlay test.
- `analysis/balance/{reforged-2.x.json,DIFF.md,sources/*}`.
- `analysis/agent-sim/gen_roster.py` — overlay load/validate/apply/emit.
- `crates/wc3-sim/src/roster_gen.rs` — regenerated (deterministic: same SHA on
  repeated runs).

## Risks / next

- The workspace was edited concurrently by other ledgers; the tree is a moving
  target. This report's evidence is from a green window of the full 20-test
  batch above.
- Not applied by design (different owner or no field): Piercing-vs-Heavy
  100%→90% damage-table constant, neutral-hero/creep/item rows, and five unit
  values whose note baseline contradicts the local extract (listed in
  `DIFF.md`).
- `setup_melee` maps have no forest; the 22k test now supplies one. A real
  map loader must keep spawning trees for the lumber economy.
