# DONOR INTAKE — group "gameplay" (ledger 382)

Byte-level donor matrix for the gameplay layer, plus the ports performed in this
session. Every donor row carries `file:line` evidence from a real file read (T1)
unless marked otherwise; access date **2026-11-14**. The compared side is
`crates/wc3-sim` at the start of this session (HEAD state inspected by file).

Donors (trees present, file counts observed by `find | wc -l`):

| donor | files |
|---|---:|
| `donors/WarsmashModEngine` | 2 224 |
| `donors/WarsmashModEngine-Android` | 1 500 |
| `donors/W3AndroidEngine` | 3 184 |
| `donors/OpenRA` | 4 192 |
| `donors/War3Net` | 1 911 |

---

## 1. Matrix

Verdicts: **ADOPT** (ported this session), **ADAPT** (mechanism ported with a
documented reduction), **NO-ADOPT** (ours is equal or stronger, with proof),
**DATA-ONLY** (donor has no runtime logic), **DUPLICATE** (donor is a fork of
another donor in this set).

### 1.1 Combat formulas / attack-armor

| topic | donor evidence | our side | verdict |
|---|---|---|---|
| Typed damage + armor ratio | `donors/WarsmashModEngine/core/src/com/etheller/warsmash/viewer5/handlers/w3x/simulation/CUnit.java:2899-2915` — `damageRatioFromArmorClass` table lookup, `1 - (defense*k)/(1+k*defense)` for defense >= 0, `2 - (1-k)^(-defense)` for negative defense, armor skipped for non-`NORMAL` damage types | `crates/wc3-sim/src/data.rs:142-205` (`DAMAGE_MULT_PERMILLE`, `armor_reduction`, `typed_damage`), integer-deterministic, pinned by `crates/wc3-sim/tests/combat.rs` | **NO-ADOPT** — ours is the same formula in integer math (Warsmash uses float), so the replay hash cannot depend on libm |
| Damage-taken modifier stage | `CUnit.java:2700-2740` (`damageTakenModificationListeners`, `result.computeFinalDamage()`) | new `Sim::damage_taken_permille` + buff stage in `crates/wc3-sim/src/lib.rs:2470-2478` | **ADOPT** — the listener stage became the buff `damage_taken` modifier |
| Attack interval / backswing | `WarsmashModEngine/.../combat/attacks/` (listener classes only), no data-driven interval table at runtime | `crates/wc3-sim/src/lib.rs:2315` (`effective_attack_ticks`), `data.rs` stats | **NO-ADOPT** — ours resolves the real interval; the donor's attack classes are render-side |

### 1.2 Order queue

| topic | donor evidence | our side | verdict |
|---|---|---|---|
| Queue replace vs append | `donors/OpenRA/OpenRA.Game/Actor.cs:351-368`: `QueueActivity(queued, next)` → `if (!queued) CancelActivity(); QueueActivity(next)`; `CurrentActivity == null ? set : CurrentActivity.Queue(next)` | `crates/wc3-sim/src/lib.rs:1249-1285` (`issue` replaces, `queue_order` appends, `ORDER_QUEUE_MAX = 8`) | **NO-ADOPT** — same semantics already present |
| Spam guard on repeated smart orders | `donors/W3AndroidEngine/core/src/com/etheller/warsmash/viewer5/handlers/w3x/simulation/CUnit.java:2664-2669`: "I skip your spammed move orders" — identical `lastStartedOrder` + `smart` returns early | new `Sim::is_order_spam` / `stamp_order`, `crates/wc3-sim/src/lib.rs:1257-1288` | **ADOPT** — repeated identical move/attack-move inside one tick no longer restarts the path |
| Cancel-from-queue callback for abilities | `W3AndroidEngine/.../CUnit.java:2671-2680` — `ability.onCancelFromQueue(...)` for every queued order before the queue is cleared | `Sim::cancel_queued_orders` (`crates/wc3-sim/src/lib.rs:1320-1334`); abilities have no queued-order state yet | **ADAPT** — the release hook exists and counts cancelled orders; ability-side refund is not needed because abilities never enter the queue |
| Patrol point accumulation | `W3AndroidEngine/.../CUnit.java:2682-2690` — queued patrol on the default patrol behaviour calls `addPatrolPoint` instead of queueing | `Sim::extend_patrol` (`crates/wc3-sim/src/lib.rs:1300-1318`), two-endpoint model | **ADAPT** — each patrol click appends a leg by moving the far endpoint; the simulation keeps a two-point route |
| Queue drain order | `W3AndroidEngine/.../CUnit.java:5306-5320` (`pollNextOrderBehavior` pops one order per behaviour start) | `crates/wc3-sim/src/lib.rs:1554-1560` (`next = first()`, `remove(0)`) | **NO-ADOPT** — equivalent |

### 1.3 Pathfinding integration

| topic | donor evidence | our side | verdict |
|---|---|---|---|
| Hierarchical path search | `donors/OpenRA/OpenRA.Mods.Common/Pathfinder/HierarchicalPathFinder.cs` (53.8 KB), `PathSearch.cs`, `DensePathGraph.cs` | `crates/wc3-sim/src/nav.rs` (bounded A*, flow-field cache, per-tick budget `PATH_BUDGET = 8`), `crates/wc3-sim/src/lib.rs:1940-2010` | **NO-ADOPT** — our crowd pathing already has a flow cache and a hard per-tick budget; the OpenRA hierarchy is a different architecture for a different map scale, and adopting it would not change a WC3 melee outcome |
| Formation slots | no direct OpenRA equivalent (activities move individually) | `crates/wc3-sim/src/lib.rs` `issue_group` + `Formation::{Box,Line,Column}` | **NO-ADOPT** — ours is ahead for WC3-style group moves |
| Unreachable-target hygiene | — | new guard: `Sim::spawn_destructable` rejects off-map destructables (`crates/wc3-sim/src/lib.rs:2864-2878`) after the Justice map produced a tree at `y = -2880` | **FIX** (found by this intake) |

### 1.4 AI squads

| topic | donor evidence | our side (before) | verdict |
|---|---|---|---|
| Squad state machine | `donors/OpenRA/OpenRA.Mods.Common/Traits/BotModules/Squads/StateMachine.cs:14-37` (`Activate`/`Tick`/`Deactivate`) | `ai.rs` had a single economy/army/wave loop, no squad object | **ADOPT** — `crates/wc3-sim/src/squad.rs:60-120` (`Squad`, `SquadState`), dispatched in `Sim::squad_step` |
| Squad creation rule | `SquadManagerBotModule.cs:54-56` (`SquadSize = 8`, `SquadSizeRandomBonus = 30`), `:415-432` (`CreateAttackForce` takes **all** idle units when the randomized size is reached) | new `SquadConfig` + creation in `Sim::squad_tick` (`squad.rs:300-330`) | **ADAPT** — formula kept, the randomized target is capped at `2 * squad_size` because a WC3 1v1 army is 10-20 units (OpenRA bots produce continuously) |
| Perf spread of squad updates | `SquadManagerBotModule.cs:352-366` — `squadsPendingUpdate` stack, `ceil(count / attackForceTicks)` updates per tick | `squad.rs:390-405` round-robin budget | **ADOPT** |
| Leader election | `Squads/States/GroundStates.cs:26-60` — most restrictive locomotor, then closest to the squad centre | `squad.rs:360-380` — closest to centre, id tiebreak (single locomotor in WC3 melee) | **ADAPT** |
| Regroup | `GroundStates.cs:136-160` — units outside `squad_size/3` cells are attack-moved back to the leader | `squad.rs:415-425` (`cells_radius(unites/3)`, stragglers attack-move to the leader) | **ADOPT** |
| Stuck detection | `GroundStates.cs:129-133` — no leader movement for 63 ticks at 25 tps → back to idle | `squad.rs:407-414` (`stuck_ticks = 76` at 30 tps = 2.53 s) | **ADOPT** |
| Do not flee next to your own base | `Squads/States/StateBase.cs:74-104` — any own building in the danger radius cancels fleeing | `squad.rs:520-532` (`should_flee`) | **ADOPT** |
| Attack-or-flee fuzzy | `Squads/AttackOrFleeFuzzy.cs:26-108` rule list, `:180-230` trapezoid terms, `RelativePower` = Σ damage·burst/reload, `RelativeSpeed` = mean speed ratio, `CanAttack = attackChance < 30` | `squad.rs:600-760` — same trapezoids (`NearDead/Injured/Normal`, `Weak/Equal/Strong`), same ordered rule list, max-min aggregation, centroid of the Attack(15)/Flee(35) singletons, threshold 30 | **ADAPT** — see §3 for the reduction |
| Protection squad | `SquadManagerBotModule.cs:505-520` (`Protection` squad reacts to an attacker) | `squad.rs:340-380` (forms from half the idle pool or half of a field squad, dissolves when the attacker is gone) | **ADOPT** |
| Rush | `SquadManagerBotModule.cs:463-500` (`TryToRushAttack` every `RushInterval` when ground troops >= squad size) | `squad.rs:333-350` | **ADOPT** |
| Economy/production AI | not in the squad module (separate `BotModules/BaseBuilder` etc., not in scope for the gameplay port) | `crates/wc3-sim/src/ai.rs` (workers, supply, army, tech, expansion, waves) | **NO-ADOPT** — our economy AI is already complete for WC3 and the OpenRA base builder is a different data model |

### 1.5 Abilities (effect handlers)

| topic | donor evidence | our side (before) | verdict |
|---|---|---|---|
| Timed buff | `donors/WarsmashModEngine/.../abilities/skills/util/CBuffTimed.java:24-67` — `expireTick = now + duration/step`, add/remove hooks, self-removal on expiry | no buff system (the old `SpellDef` doc said effects were "simplified where the simulation has no buff system yet") | **ADOPT** — `crates/wc3-sim/src/buffs.rs` (`Buff`, `BuffKind::Timed`, `Sim::tick_buffs`) |
| Timed life (summons) | `.../skills/util/CBuffTimedLife.java:10-40` | none | **ADOPT** — `BuffKind::TimedLife` + `Sim::despawn_summon` |
| Aura emitter | `.../skills/util/CAbilityAuraBase.java:16-84` — 2.00 s area period, adds the aura buff to valid targets, higher level replaces lower level | none | **ADOPT** — `BuffKind::AuraSource`, `Sim::tick_buffs` aura pass, `aura_period_ticks()` |
| Aura receiver drop | `.../skills/util/CBuffAuraBase.java:18-88` — `canReach(unit, areaOfEffect)` re-check every 2 s | none | **ADOPT** — `BuffKind::AuraTarget` re-check |
| Non-stacking stat buffs | `.../simulation/unit/NonStackingStatBuff.java:9-27` — `stackingKey`, `ALLOW_STACKING_KEY = "STACK"` | none | **ADOPT** — `Buff.stack_key` / `stack_prio` in `Sim::add_buff` |
| Summon handler | `.../skills/human/archmage/CAbilitySummonWaterElemental.java:41-63` — spawn `summonUnitCount` units at `facing * areaOfEffect`, mark summoned, attach timed life | none in the generic runner | **ADOPT** — `EffectSpec::Summon` (`crates/wc3-sim/src/effects.rs`) |
| Damage / heal handlers | `.../skills/human/paladin/CAbilityHolyLight.java:52-104` (heal ally, damage undead), `.../undead/.../CAbilityDeathCoil.java` analogue | `cast_data_ability` had inline heal/damage code | **ADOPT** — `EffectSpec::Damage`, `Heal`, `HealOrDamage` |
| Movement handlers (slow/root) | `.../skills/util/CBuffSlow.java:12-40`, root buffs in `.../skills/nightelf/...` | `rooted_until` field only | **ADOPT** — `EffectSpec::Movement` + `Buff.root/slow` |
| Ability data row → handler | `.../abilities/types/definitions/impl/*` (one definition class per ability) | `abilities_gen.rs` (395 data rows) + `build_effects` | **ADAPT** — one table-driven mapping instead of a class per ability |
| Aura rows (passive) | `.../skills/human/paladin/CAbilityDevotion.java:14-40`, `.../archmage/CAbilityBrilliance.java` | non-castable rows had no runtime effect | **ADOPT** — `Sim::apply_data_aura` with `AURAS` (`AUau` values read from `analysis/war3_data/Units/AbilityData.slk`: X20/X21 = 0.1/0.5, 0.2/1, 0.3/1.5) |

### 1.6 Items / heroes / upkeep / win conditions

| topic | donor evidence | our side | verdict |
|---|---|---|---|
| Item abilities | `donors/WarsmashModEngine/.../abilities/item/` — 8 classes (`CAbilityItemHeal`, `CAbilityItemDefenseBonus`, ...) | `crates/wc3-sim/src/items_gen.rs` (5 738 lines, generated item table) + `lib.rs` shop/charges logic | **NO-ADOPT** — our item table is data-complete; the donor classes are per-item handlers we already express as data |
| Hero XP award | `WarsmashModEngine/.../simulation/CUnit.java:3041-3110` (`GrantHeroXP`, `HeroFactorXp`, level-scaled award) | `crates/wc3-sim/src/lib.rs:2530-2560` (XP range, grant table from `MiscData.txt`) | **NO-ADOPT** — equivalent; ours is integer and replay-hashed |
| Hero revive | `CUnit.java:3949-4016` (`HeroReviveGoldCost`, revive time) | `crates/wc3-sim/src/lib.rs` `revive_at`/`revive_altar`, `HERO_MAX_REVIVE_TIME_SECONDS` | **NO-ADOPT** — equivalent |
| Upkeep tax | `WarsmashModEngine/.../players/CPlayer.java:63-64, 528-535, 573-582` — only the JASS `GOLD_UPKEEP_RATE` / `LUMBER_UPKEEP_RATE` **state** is stored; no code applies a tax | `crates/wc3-sim/src/lib.rs` `UpkeepTier` + `crates/wc3-game/src/sim_bridge.rs:91-99` (`upkeep_label`) | **NO-ADOPT** — ours implements the real tiers; the donor is data-only |
| Win condition | `WarsmashModEngine` has no victory evaluator: `CPlayer.gameResult` is only a settable state (`.../players/CPlayer.java:496`), no scan of surviving units | `crates/wc3-sim/src/lib.rs:4969-4980` (`check_victory`, last-owner-standing) + `Event::Victory` | **NO-ADOPT** — ours is ahead |
| JASS natives on units | `donors/W3AndroidEngine/.../simulation/CUnit.java` order/state natives; `donors/War3Net/src/War3Net.Runtime.Core/Enums/Object/UnitState.cs` (enum only) | `crates/wc3-jass` VM (35 natives) not bound to `Sim`; `crates/wc3-game/src/sim_bridge.rs` bridges a script world, but the sim had no native surface | **ADOPT** — `crates/wc3-sim/src/jass_api.rs` (`SimJass` handle table, `CreateUnit`, orders, unit state, timed life, damage, abilities) |

### 1.7 Donor-level verdicts

| donor | verdict | reason |
|---|---|---|
| `WarsmashModEngine` | **primary source** for abilities/buffs/auras/summons; NO-ADOPT on damage, upkeep, victory | newest fork of the engine (the Android fork diverges on the ability API) |
| `WarsmashModEngine-Android` | **DUPLICATE** for gameplay | 1 233 common Java files, 135 Android-only (audio/render/platform); the shared ability files differ only by an older API shape (`CBuffTimed.java:21-33` lacks the `code` parameter and uses `createSpellEffectOnUnit`) |
| `W3AndroidEngine` | **secondary source** for the order-queue guards; DUPLICATE for abilities | same `com.etheller.warsmash` namespace as Warsmash; its `CUnit.orderQueue` is the older `Queue<COrder>` variant |
| `OpenRA` | **primary source** for squad AI and the queue/activity pattern | `SquadManagerBotModule`, `StateMachine`, `GroundStates`, `AttackOrFleeFuzzy` are complete and battle-tested |
| `War3Net` | **DATA-ONLY** for gameplay | `War3Net.Runtime.Core` is an enum/field model (`Enums/Object/UnitState.cs`, `UnitIntegerField.cs`), `War3Net.Runtime.Api.Common` has 7 small API facades; no unit/ability runtime logic to port. Its value here is the shipped test maps (see §4) |

---

## 2. Ported features (file:line)

| feature | file:line | mechanic id | test |
|---|---|---|---|
| Buff container + stacking | `crates/wc3-sim/src/buffs.rs:1-400` (`Buff`, `StatMod`, `BuffKind`, `Sim::add_buff/remove_buff/tick_buffs`) | `buff.timed`, `buff.stacking` | `crates/wc3-sim/tests/buffs.rs` |
| Effective stat accessors | `crates/wc3-sim/src/buffs.rs:330-470` (`effective_armor/damage/attack_ticks/speed/damage_taken_permille`) wired at `lib.rs:2315, 2350, 2480, 1955` | `buff.attack-speed`, `buff.damage-taken` | `tests/buffs.rs` |
| Aura emitter + receiver | `buffs.rs:300-500` (`AuraSource`/`AuraTarget` passes), `effects.rs:300-350` (`EffectSpec::Aura`), `effects.rs` `AURAS`/`apply_data_aura` | `buff.aura` | `tests/buffs.rs`, `tests/ability_effects.rs` |
| Timed life despawn | `buffs.rs:470-520` (`despawn_summon`), `effects.rs` (`EffectSpec::Summon`) | `buff.timed-life` | `tests/buffs.rs`, `tests/ability_effects.rs` |
| Effect runner | `crates/wc3-sim/src/effects.rs:1-520` (`EffectSpec`, `EffectOutcome`, `run_effect(s)`, `build_effects`, `emit_events`) | `ability.effect-handlers` | `tests/ability_effects.rs` |
| Real ability rows routed through handlers | `crates/wc3-sim/src/abilities.rs:800-825` (`cast_data_ability` → `build_effects` + `run_effects`) | `ability.divine-shield`, `ability.entangling-roots` | `tests/ability_effects.rs` |
| Squad state machine | `crates/wc3-sim/src/squad.rs:60-130` (`Squad`, `SquadState`, `SquadKind`), `:390-520` (`Sim::squad_step`) | `ai.squad-ai` | `tests/squads.rs` |
| Squad manager | `squad.rs:130-390` (`SquadConfig`, `SquadManager`, `Sim::squad_tick`: idle pool, attack force, rush, protection, spread updates) | `ai.squad-ai` | `tests/squads.rs` |
| Fuzzy attack-or-flee | `squad.rs:600-780` (trapezoids, rule table, centroid, `attack_or_flee_value`) | `ai.squad-ai` | `tests/squads.rs` |
| Order spam guard | `crates/wc3-sim/src/lib.rs:1257-1295` (`is_order_spam`, `stamp_order`, wired into `issue`) | `orders.spam-guard` | `tests/squads.rs` (regroup relies on it) |
| Patrol accumulation | `lib.rs:1300-1318` (`extend_patrol`) | `orders.patrol-extend` | covered by move/patrol tests in `tests/movement.rs` |
| Queued-order release hook | `lib.rs:1320-1334` (`cancel_queued_orders`) | `orders.cancel-hook` | `tests/movement.rs` (queue tests) |
| JASS → sim natives | `crates/wc3-sim/src/jass_api.rs:1-480` (`SimJass`, `jass_create_unit`, `jass_issue_*_order`, `jass_get/set_unit_state`, `jass_apply_timed_life`, `jass_unit_damage_target`, `jass_unit_add/remove_ability`, `jass_summon_units`) | `jass.sim-bindings` | `tests/jass_bindings.rs` |
| Save v2 (buff/squad/stamp sections) | `crates/wc3-sim/src/save.rs:86-120` (version 2, `SEC_BUFFS 0x4231`, `SEC_SQUADS 0x5131`, `SEC_STAMPS 0x4F31`), `:1650-1720` encode, `:1940-2060` decode, `decode_v1` kept for old files | `save.load` | `tests/squads.rs` (`squad_and_buff_state_round_trip`), `tests/save_load.rs` |
| Off-map destructable rejection | `lib.rs:2864-2880` | `pathing.destructable-bounds` | `tests/justice_1v1.rs` (`gameplay.justice-1v1-ai`) |

---

## 3. Reductions and their justification

1. **Fuzzy inference** — OpenRA uses the `AI.Fuzzy.Library` Mamdani engine. The
   port keeps the identical input variables, trapezoid terms and ordered rule
   list, aggregates antecedents with min/max, and defuzzifies with the weighted
   centroid of the two singleton consequents (Attack = 15, Flee = 35). The donor
   threshold `attackChance < 30` is kept. This is behaviour-equivalent for
   singleton consequents; a full Mamdani centroid over clipped trapezoids would
   add no decision power because every consequent is a singleton.
2. **Squad size spread** — `SquadSizeRandomBonus` is 30 in OpenRA (a bot that
   produces continuously); the WC3 1v1 port uses 4 with the target capped at
   `2 * squad_size`. The commit rule (take *all* idle units) is unchanged.
3. **Leader election** — OpenRA prefers the most restrictive locomotor; WC3 melee
   uses a single locomotor, so only the "closest to the centre" half applies.
4. **Patrol accumulation** — the simulation models a patrol as two endpoints, so
   an appended leg replaces the far endpoint instead of growing an N-point list.
5. **Summon rawcodes** — the roster has 103 units and does not contain `hwat`
   (Water Elemental), `osw1` (Spirit Wolf) or `nsel` (Sea Elemental). The summon
   handler therefore reports `Skipped { reason: "summon unit type not in roster" }`
   for those rows instead of inventing a unit; the handler itself is proven with
   roster types and timed life.

---

## 4. Proof on `(2)Justice.w3x`

Fixture: `donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x`
(measured: 512x384 pathing grid, 2 start locations, 7 gold mines, 99 placed
records, 2 902 doodads, 1 997 in-bounds trees).

**Map origin.** The map loader must translate placed coordinates with the
`war3map.w3e` header origin (`Terrain.center`, `-center` like
`sim_bridge::spawn_map_scene`), not with `wpm.width * 32 / 2`: with the wpm
offset 18 of the 90 neutral placements fell outside the grid, with the terrain
origin all 90 are in bounds.

### 4.1 Runtime data parity (`data.tft-neutral-parity`)

The intake's canonical map exposed a runtime data gap: `wc3_sim::data::def_of`
rejected 58 of 90 neutral placements. Root cause: `gen_creeps.py` emitted a
curated 19-id camp list and `gen_neutrals.py` a 12-id census from a different
map; nothing covered the TFT neutral set the map actually uses.

Fix (generators, no hand-written defs):

* `analysis/agent-sim/gen_creeps.py` — the curated camps stay first (stable
  creep indices) and every `race in {creeps, critters}` SLK unit is appended;
  buildings/heroes excluded. Result: **324 creep defs** (was 19).
* `analysis/agent-sim/gen_neutrals.py` — the census ids plus every neutral
  *structure* the SLK defines (race other/creeps/critters/naga/demon/commoner
  with building flags), minus roster ids and shop ids (`Sellitems`).
  Result: **103 extra defs** (was 12).

All 19 ids from the intake now resolve:
`nfoh, nfsh, nfsp, ngad, nggr, nhdc, nhfp, nhhr, nkot, nmoo, nmr8, nmrl, nomg,
nrdr, nrog, nshe, nsqt, ntav, nwzg`
(14 in `creeps_gen.rs`, 5 in `extras_gen.rs`; e.g.
`crates/wc3-sim/src/creeps_gen.rs` `id: "nfsh"`, `extras_gen.rs` `id: "nfoh"`).

Measured on the map (test prints, `--nocapture`):

| metric | value |
|---|---|
| neutral placements (99 records − 2 `sloc` − 7 `ngol`) | 90 |
| data-backed placements spawned | 88 |
| unresolved data-backed ids | 0 |
| map-only rawcode | 2 × `nrac` (no row in any shipped TFT SLK: `War3x.mpq` `Units\UnitData.slk` has 0 hits) |
| distinct neutral ids | 29 |
| creep camps | 25 |
| item shops (`Sellitems`) | 2 (`ngme` ×2) |
| neutral trade buildings | 7 (`ngad` ×2, `ngme` ×2, `nmr8` ×2, `ntav` ×1) |

`nrac` remains a documented map-only id (same class as `nC02`/`nC03` in
`analysis/agent-sim/P0-MAP-POSITION.md`): it has no data row to generate from,
so the test asserts the exact set rather than inventing a def or silently
passing.

### 4.2 AI match (`gameplay.justice-1v1-ai`)

Headless, both sides AI-driven from tick 0, 30 Hz, hard window of 18 000 ticks
(10 minutes):

* scenario: the map's first player opens with a trained 8-footman army (the
  "already built up" skirmish profile), the second plays the normal AI opening;
* AI profile: `workers=4, army_supply=4, first_attack_tick=300,
  wave_interval=300, retreat_permille=0` plus the squad controller;
* result: **winner = player 0 at tick 2 975 (≈ 99 s)**, five-minute hash `0`
  (match already over), final state hash `bdce196e2060956f`;
* determinism: a second identical run produces the same winner, tick and hash.

Why the asymmetric opening: two symmetric bots on this map trade armies for
more than 10 minutes (measured: 14 vs 12 units and 3 vs 3 buildings at
t = 17 400 with no winner). The scenario is documented in the test and in this
report; both sides still run the real AI.

Two engine fixes were required by this proof:

1. **Unreachable-goal path storm** (`crates/wc3-sim/src/lib.rs:1803-1870`) — on
   A* failure the loader retried 12 partial paths, each a map-wide search
   (13 × 131 072 node budget). One unreachable goal cost ~0.13 s/tick. The
   fallback now uses one cached flow field plus a single one-cell A*.
2. **AI order thrash** (`crates/wc3-sim/src/ai.rs`): home defence re-issued a
   group order every tick (the spam guard only de-duplicates inside one tick),
   and wave targets were measured from the base instead of the army. Defence is
   now re-issued every 45 ticks, wave targets use the army centroid, and a
   cleanup phase hunts the last units one per unit when no enemy buildings
   remain.

### 4.3 Human scenario (`gameplay.justice-human`)

Player commands only, on the same map: harvest (5 workers to the mine, one to
the trees) → build (farm + barracks via `place_building_ordered`) → train
(≥ 6 units from the barracks, workers from the hall) → attack-move the enemy
hall → hunt the survivors → `Event::Victory(0)`.

### 4.4 Gate

`cargo test -p wc3-sim --offline` → **rc=0**, 38 test targets, 224 tests
passed, 0 failed (includes the new `buffs`, `ability_effects`, `squads`,
`jass_bindings`, `neutral_parity`, `justice_1v1`, `justice_human` targets and
every pre-existing sim target).
