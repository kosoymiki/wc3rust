# Agent F — sim / gameplay parity (ledger 602, all-layers push 2026-11-21)

Packet: `analysis/tech-lead/all-layers-20261121/PACKET_F_sim.md`.
Owner paths: `crates/wc3-sim/**`, `analysis/tech-lead/mechanics-20261121/**`.

## Goal

Melee gameplay parity proved against the shipped game where an oracle exists:
cross-map movement (>= 800 px, deterministic), production queues, construction,
win/lose, and unit stats cross-checked against the shipped
`UnitBalance.slk` / `UnitWeapons.slk`; one named test per mechanics id
(`TEST_POLICY.md`), all listed by the single test authority
`analysis/tech-lead/run_mechanic_tests.sh`.

## Defects found and fixed (root cause, oracle, file:line)

| # | Defect (root cause) | Shipped oracle | Fix |
|---:|---|---|---|
| 1 | `PRODUCTION_QUEUE_MAX = 5`: a building refused the sixth queued unit. | Gap map row 20 ("Queue up to 7"); WC3 melee command card holds seven queue rows. | `crates/wc3-sim/src/lib.rs:146` = 7. |
| 2 | `cancel_construction` refunded 100% of gold/lumber. | `ConstructionRefundRate=0.75` — `analysis/war3x_local/Melee_V0/Units/MiscGame.txt:123`. | `lib.rs:6208` refunds `cost * 3 / 4`, `lumber * 3 / 4` (integer truncation). |
| 3 | A construction site nobody works on never lost hit points. | `ConstructionLifeDrainRate=10.0` — `MiscGame.txt:129`. | `lib.rs:6326` drains 1 hp every 3rd tick (30 Hz -> 10 hp/s), floors at 1 hp (the constants do not define 0 hp, recorded as a boundary, no invented death rule). |
| 4 | `Event::Victory` was re-emitted every tick after elimination; the AI could undo a defeat by instantly placing a new hall in the same tick (AI ran before the verdict). | WC3 ends a melee game once, at the moment the last building falls. | `lib.rs` `Sim::victory: Option<u8>` latch (one-shot verdict); `check_victory` now runs after the economy phase and before `advance_ai`; verdict persisted in the save header (format v10) and covered by `state_hash`. |
| 5 | `osld` (Orc Spirit Lodge) carried stale TFT prices 150/150 although the roster is generated from the current install table. | `analysis/war3_full/Units/UnitBalance.slk` osld: `goldcost=225`, `lumbercost=90` (goldRep/lumberRep identical). | `crates/wc3-sim/src/roster_gen.rs:2916-2917` = 225 / 90. |
| 6 | `merge.rs::neutral_map_units_cluster_into_creep_camps` spawned creeps on legacy owner 2 while `cluster_neutral_camps` clusters `NEUTRAL_OWNER` (12) — another agent's in-flight owner migration left the test behind. | `lib.rs:125` `NEUTRAL_OWNER = 12`. | test spawns through `wc3_sim::NEUTRAL_OWNER`. |

## Named tests added / corrected (one per mechanics id)

| Mechanic id (gap map row) | Test | File |
|---|---|---|
| `pathing.flow-field` (10) | `pathing_flow_field_crossmap_800px_replay_identical` — >= 800 px, same seed + log => identical final position and `state_hash` | `tests/movement.rs` |
| `production.queue` (20) | `production_queue_seven_slots_and_full_refund_any_position` — 7 slots, eighth refused, middle entry refunds its own price (`TrainRefundRate=1.0`, MiscGame.txt:126) | `tests/production.rs` |
| `production.queue` (20) | `production_queue_cancel_front_entry_advances_the_queue` (renamed from `cancel_refunds_and_advances_the_queue`) | `tests/production.rs` |
| `buildings.construction` (14) | `buildings_construction_cancel_refunds_seventy_five_percent` (old expectation was a 100% refund) | `tests/production.rs` |
| `buildings.construction` (14) | `buildings_construction_halted_site_drains_ten_hp_per_second` — 90 ticks = exactly 30 hp, resumed site still completes | `tests/production.rs` |
| `buildings.construction` (14) | `buildings_construction_site_completes_and_raises_supply`, `buildings_construction_requires_builder_and_free_ground` (renamed to carry the id) | `tests/production.rs` |
| `melee.matrix` (33) | `melee_matrix_victory_elimination_once_at_1v1_and_4v4` — a side survives on any building, the last building decides, the verdict latches, save/load restores the ended game | `tests/melee_matrix.rs` |
| `combat.damage-table` (1) | `combat_damage_table_unit_stats_match_shipped_slk` — 122 melee roster rows vs the shipped SLK cells (hp/realHP, def, spd, gold, lumber, fused, fmade, bldtm, dmgplus1, dice1, sides1, rangeN1, cool1); Reforged 2.x overlay rows are the only accepted deltas; TFT extract only for rows the install table lacks | `tests/combat.rs` |

Oracle note: the sim roster is generated from the current install tables
(`analysis/war3_full`) with the TFT extract as a fallback, and then the
Reforged 2.0.2/2.0.4 overlay is applied. The cross-check reads both SLK files
through `wc3_formats::gamedata::Sheet` (a path independent of the generator) and
accepts a cell only when it equals the install-table cell, or the TFT cell for a
row the install table does not carry. 5 overlay deltas were exercised
(e.g. Archer hp 260 -> 275), 19 rows resolved through the TFT fallback.

## Verification (exact commands and rc)

```
cargo check -p wc3-sim --offline                      => rc=0 (Finished dev profile)
cargo test -p wc3-sim --offline --test production     => 11 passed, 0 failed
cargo test -p wc3-sim --offline --test movement       => 16 passed, 0 failed
cargo test -p wc3-sim --offline --test melee_matrix   =>  2 passed, 0 failed
cargo test -p wc3-sim --offline --test combat         => 11 passed, 0 failed
cargo test -p wc3-sim --offline --test save_load      => 14 passed, 0 failed
cargo test -p wc3-sim --offline --test save_script    =>  2 passed, 0 failed
cargo test -p wc3-sim --offline --test determinism    =>  3 passed, 0 failed
cargo test -p wc3-sim --offline --test roster         =>  8 passed, 0 failed
cargo test -p wc3-sim --offline --test melee_races    =>  4 passed, 0 failed
cargo test -p wc3-sim --offline --test economy        => 19 passed, 0 failed
cargo test -p wc3-sim --offline --test upkeep         =>  1 passed, 0 failed
cargo test -p wc3-sim --offline --test sim_parity_501 => 10 passed, 0 failed
cargo test -p wc3-sim --offline --test merge          => 29 passed, 0 failed
cargo test -p wc3-sim --offline --test content        => 16 passed, 0 failed
cargo test -p wc3-sim --offline --test ai             =>  8 passed, 0 failed
cargo test -p wc3-sim --offline --test melee_spawn    =>  3 passed, 0 failed
cargo test -p wc3-sim --offline --test physics_1v1    =>  4 passed, 0 failed
cargo test -p wc3-sim --offline --test projectiles_items => 5 passed, 0 failed
cargo test -p wc3-sim --offline --test heroes         =>  3 passed, 0 failed
cargo test -p wc3-sim --offline --test gameplay_config => 2 passed, 0 failed
cargo test -p wc3-sim --offline --test items_round9   =>  4 passed, 0 failed
```

`bash analysis/tech-lead/run_mechanic_tests.sh` (full, 131 rows) finished with
`rows=131 failing=26` (rc=1, inherited; see reconciliation). My rows
(`analysis/tech-lead/mechanic_tests.json`):

```
combat.damage-table    rc=0 passed=11 failed=0
combat.attack-speed    rc=0 passed=11 failed=0
movement.speed-clamp   rc=0 passed=4  failed=0
movement.turn-rate     rc=0 passed=16 failed=0
movement.collision     rc=0 passed=3  failed=0
pathing.wpm            rc=0 passed=16 failed=0
pathing.flow-field     rc=0 passed=16 failed=0
buildings.exit         rc=0 passed=3  failed=0
buildings.footprint    rc=0 passed=3  failed=0
buildings.construction rc=0 passed=11 failed=0
economy.gold           rc=0 passed=19 failed=0
economy.lumber         rc=0 passed=19 failed=0
economy.supply         rc=0 passed=11 failed=0
economy.upkeep         rc=0 passed=1  failed=0
production.queue       rc=0 passed=11 failed=0
production.tech-tree   rc=0 passed=8  failed=0
melee.matrix           rc=0 passed=2  failed=0
heroes.leveling        rc=0 passed=3  failed=0
heroes.xp              rc=0 passed=2  failed=0
heroes.revive          rc=0 passed=1  failed=0
creeps.camps           rc=0 passed=3  failed=0
determinism            rc=0 passed=3  failed=0
persistence.save       rc=0 passed=14 failed=0
save.load              rc=0 passed=14 failed=0
save.versioning        rc=0 passed=2  failed=0
```

### Failing-row reconciliation (26 rows, none introduced by this change)

| Group | Rows | Root cause (evidence) |
|---|---:|---|
| wc3-game lib-test compile | 18 (`pathing.water`, `script.jass`, `ui.hotkeys`, `ui.command-card`, `ui.menu`, `ui.resources`, `ui.minimap`, `ui.tooltips`, `rendering.*` x8, `buildings.exit.real-map`, `melee.human04-contract`) | `error[E0277] crates/wc3-game/src/campaign.rs:3841` — `.then(|| path.file_stem()?…)` uses `?` in a `String` closure. Another agent's in-flight edit; `wc3-game` is outside my OWNER paths and `wc3-sim` does not depend on it. Each row logs `could not compile 'wc3-game' (lib test)`, `passed=0 failed=0`. |
| wc3-game test failures | 2 (`maps.registry.all-load`, `campaign.flow`) | `crates/wc3-game/tests/map_registry.rs:126`, `crates/wc3-game/tests/campaign.rs:733` — both in the known pre-existing list. |
| wc3-sim unrelated binaries | 4 (`ability.effect-handlers`, `ability.divine-shield`, `ability.entangling-roots`, `jass.natives`) | known pre-existing rows; the tests are in `ability_effects.rs` / `jass_natives.rs`, untouched by this change. |
| net.multiplayer | 1 | `donors/War3Net/tests/.../(2)Ponds_v1.4.w3x` is absent (`donors/` does not exist); 13 of 16 net tests pass, lockstep/desync/replay are green. |
| wc3-re | 1 (`maps.jass.natives`) | `crates/wc3-re/src/mapboot.rs:385` — known pre-existing row. |

No failing row belongs to `crates/wc3-sim` tests touched here; the sim targets
listed above are all `rc=0`.

## Result

PASS for the assigned scope: the six defects are fixed with shipped-oracle
evidence, every touched mechanic carries a policy-compliant named test, and the
single test authority reports all sim rows green. The runner-wide rc is 1 only
because of the 26 rows reconciled above (another layer's compile error plus the
known pre-existing rows) — no gate was relaxed and no PASS was fabricated.

## Risks / Next

- `wc3-game` lib tests do not compile (`campaign.rs:3841`): owner of `crates/wc3-game/**` must fix; until then 18 runner rows report `passed=0 failed=0`.
- `ConstructionLifeDrainRate`: the shipped constants stop at "10 hp per second"; the 1-hp floor is a documented boundary, not a retail-verified death rule. If a retail oracle for 0 hp appears, the floor becomes a kill.
- The victory latch is persisted as save format v10 (`decode_v9` now pins version 9 so old files still read). Any agent adding another save field must add its own arm.
- `osld` was corrected in the generated table by hand (the generator run would also rewrite `TECH_DEFS`, another agent's uncommitted table); a future `gen_roster.py` run must reproduce 225/90.
- `PRODUCTION_QUEUE_MAX` raised 5 -> 7 changes production pacing: `melee.race_*`, `ai.race_builds_and_expands`, `production`, `economy` all re-ran green.

## H2 persistence

- corpus preflight: `observed` OK (`harness2_corpus_pack`, task_type general, must gates read)
- capture: **1251** (session evidence: defects, fixes, rc values, failing-row reconciliation)
- experience: **4914** (SLK oracle extract discipline), **4915** (victory pipeline order + latch), **4916** (save `decode_vN` must pin N), **4917** (MiscGame refund/drain rates are acceptance oracles)
- monitor ledger: **602** (checkpoints: recon, implement, verify; closed with this report)
- report: this file

