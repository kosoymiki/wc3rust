# SIM FULL GOAL — 1:1 gameplay loop on Ponds (ledger 374)

## Headless 1v1 on the shipped map (mechanic `gameplay.1v1`)

Test: `gameplay_1v1_ponds_melee_loop` (`crates/wc3-sim/tests/gameplay_1v1.rs`).
Map: `donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/(2)Ponds_v1.4.w3x`
(observed: 124 placed units, 5767 doodads, 2 `sloc`, 6 `ngol`, WPM 384×512).
The test loads the map's **own** WPM (native 32-unit cells), tree/doodad field
(4959 known destructables spawned; 3311 blocked pathing cells), 6 `ngol` gold
mines, both `sloc` starts, and the map's neutral units/shops (50 spawned known
defs). It then plays both sides **only through player-issued commands**:
`Harvest`, `harvest_lumber`, `place_building_ordered`, `train_def`, `Attack`,
`Repair`, `LearnAbility` (API list below), for 18,000 ticks = 10 simulated
minutes.

Observed run (`analysis/agent-sim/ponds-debug.log`):

```
PONDS result winner=None tick=18367 gold_events=381 lumber_events=145
  projectiles=41 deaths=7 combat_deaths=7 path_blocked=340 trees=4959
PONDS STALEMATE documented: neither main hall fell within 10 minutes
```

The loop harvests gold+lumber, builds supply/barracks, trains armies, fights
(projectiles, unit deaths) and checks the win condition (enemy hall falls). The
match ends in a **documented stalemate**: both armies stall in the map's dense
forest approach; no hall falls inside 10 minutes. `winner` is accepted as either
a hall fall or this documented stalemate, per the packet.

## Combat parity (mechanic `combat.parity`)

Test: `combat_parity_damage_armor_cooldown_projectile_backswing`
(`crates/wc3-sim/tests/combat_parity.rs`):
- damage/armor table (pierce-heavy 89, normal-medium 133, armor 5→76%),
- cooldown 40 ticks, **backswing 15 ticks parsed from `backSw1`**, acquisition 500,
  collision radius 15.5,
- projectile speed 1900, travel + impact damage,
- guard/leash camp values, collision separation.
Ponds scenario adds: map trees block pathing cells and combat (projectiles/deaths).

## Tech parity
- `melee_race_{orc,human,undead,nightelf}`: every roster building placed and every
  unit trained through the real tech tree with costs; tier gates; supply/upkeep;
  worker repair.
- `upgrades_execution_89_rows`: 89/89 upgrades researched, tier gates.
- `heroes_skill_points_ultimate_at_6`, `heroes_revive_altar_700g_150s`.
- `items_*` (abilities/shop/charges/recipes), `items_drops_creep_loot`.

## AI parity (mechanic `ai.race_builds_and_expands`)
All 4 races on a fixed seed: expands, sends waves, retreats, rebuilds workers,
mixed army, no cheats (AI only uses Sim commands).
`cargo test -p wc3-sim --test ai_race` → 1 passed.

## Player-issued command list
`Command::Move`, `MoveAttack`, `Attack`, `Hold`, `Stop`, `Patrol`, `Harvest`,
`Train` (`train_def`), `Build` (`place_building`/`place_building_ordered`),
`Repair`, `LearnAbility`, `Cast`, plus `cast_data_ability`, `buy_item`,
`use_item`, `revive_hero`, `setup_melee`, `harvest_lumber`.

## Checklist × race (evidence = passing mechanic tests)

| Race | Harvest | Build | Train | Fight | Win |
|---|---|---|---|---|---|
| Orc | AI economy + Ponds loop (gold/lumber events) | `melee_race_orc` | `melee_race_orc` | `ai_race` + combat parity | hall-fall condition implemented; Ponds run stalemate |
| Human | Ponds enemy economy (gold/lumber) | `melee_race_human` | `melee_race_human` | `ai_race` + combat parity | same |
| Undead | AI economy | `melee_race_undead` | `melee_race_undead` | `ai_race` | same |
| NightElf | Ponds player economy | `melee_race_nightelf` | `melee_race_nightelf` | `ai_race` | same |

## ALWAYS-FIX bugs found this round
- Chase/attack units ignored A* paths (beeline into forest) → now follow path.
- Attack orders dropped after 30 stuck ticks → repath instead of idling.
- Buildings could not be attacked from adjacent cells → building attack range bonus.
- Dice-only units (damage flat 0) were skipped by combat → weapon dice counted.
- AI site search accepted footprints overlapping buildings → cell-based occupancy.
- AI could not build when units stood in future footprint → occupancy cell check.
- Worker boxed in by footprints → nearest-standable escape.
- Blizzard channeled with auto-attacks + wave duplicate targets → channel state + dedup.
- Backswing data added from `UnitWeapons.backSw1`.

## Remaining gaps (named)
- Ponds 1v1 ends in documented stalemate (dense forest pathing); no hall falls in
  10 min. Win detection works (hall death/Victory), but a decisive winner needs a
  map-route/pathing pass (long A* through 3311 blocked tree cells).
- Backswing is parsed/tested but damage/launch timing is still cooldown-based, not
  delayed by the backswing window.
- AI does not harvest lumber (gold only); initial lumber is used for buildings.
- Per-race harvest/lumber is only exercised through AI/Ponds scenarios, not four
  separate race loop tests.
- Device run not granted (`H2_DEVICE_SLOT` unset); no screenshots.
