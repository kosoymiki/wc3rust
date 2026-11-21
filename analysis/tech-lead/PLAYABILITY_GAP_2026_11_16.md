# PLAYABILITY GAP — 1v1 melee on `(2)Justice.w3x` (2026-11-16)

Operator verdict (2026-11-16): **«это всё ещё не играбельный огрызок»**.

This document answers one question with evidence: *what exactly stops the port from playing like
Warcraft III on Justice?* It is read-only analysis — no product code was changed. Every claim carries
a `file:line` from the tree at the moment of writing (2026-11-16), or is labelled
`observed | inferred | reported-unverified | unknown`.

Method: corpus gate `ev-8be03f8228cad3a7`; read of `analysis/agent-re/*/{ENGINE_MAP.md,constants.json}`,
`analysis/tech-lead/{ACCEPTANCE_1TO1.md,FULL_PORT_CRITERIA.md,TEST_POLICY.md,MECHANICS_GAP_MAP.md,OPERATOR_BUGS.json}`,
device log `analysis/device-build-probe.log`, map census (`target/debug/examples/map_census` on the real
Justice archive), and the code itself (`crates/wc3-sim/src/lib.rs`, `crates/wc3-game/src/{game,hud,sim_bridge}.rs`,
`crates/wc3-audio`). `cargo test` was **not** re-run in this session: the single-slot cargo lock was held by
other sessions (`/data/data/com.termux/files/usr/tmp/h2-cargo.log`), and every claim below is decidable
statically. Where a claim needs a live run, the command is given in the work list.

---

## 0. Executive summary — why it is not playable

Three defects alone make a match unplayable, and none of them is a rendering or polish issue:

| # | Defect | Evidence |
|---:|---|---|
| **A** | **A human cannot produce a single combat unit.** Every card training action is routed through `Game::selected_hall()`, which returns `Some` only for a *main hall*. A barracks/altar/workshop card renders its units, but pressing the button silently does nothing. | `crates/wc3-game/src/game.rs:4826-4838` → `crates/wc3-game/src/game.rs:2141-2153`; hall trains only `hpea` (`crates/wc3-sim/src/roster_gen.rs:233`); the barracks card is built at `crates/wc3-game/src/hud.rs:1609-1637` with entries from `crates/wc3-game/src/hud.rs:1543-1563` |
| **B** | **No tech tier is reachable.** Town-hall upgrades (Keep/Castle) do not exist as a mechanic, and `hcas` is not in any build list. `hkni` requires `hcas`, so Knights and every tier-2/3 upgrade are dead content for a human. | no `Command::Upgrade*` / `upgrade_building` in `crates/wc3-sim/src/lib.rs` (grep empty); `hpea Builds=htow,hhou,hbar,hbla,hwtw,halt,harm,hars,hlum,hgra` (`analysis/war3_full3/Units/HumanUnitFunc.txt:227`); `hkni.requires = ["hlum","hcas","hbla"]` (`crates/wc3-sim/src/roster_gen.rs`) |
| **C** | **No hero loop.** Skill points cannot be spent, dead heroes never revive, and 4 of 12 heroes have zero castable abilities. | `crates/wc3-game/src/hud.rs:1787-1792` (LEARN slot deliberately empty, no `UiAction::LearnAbility`); `revive_hero` has **no caller** (`crates/wc3-sim/src/lib.rs:4508`); spell coverage 10 of 49 hero ability ids (`crates/wc3-sim/src/abilities.rs:16-41`) |

Everything else in this document is ordered behind those three.

**Scorecard for the 13 required systems** (detail in §2):

| Status | Count | Systems |
|---|---:|---|
| Playable | 4 | fog + minimap, group orders, audio per-def, unit production *from the hall only* |
| Partial — the player can reach part of it | 4 | economy/upkeep (starting pool wrong, P0-5), tech tree (no research, no tier), building construction (no cancel), hotkeys (producer cards dead) |
| Present but unreachable by the player | 3 | hero abilities/level, items/shops, save/load |
| Missing | 2 | creeps camp respawn, score screen |

The mechanical explanation of the verdict: **coverage measures sim methods; it never measures the player's
reach to them.** `mechanic_tests.json` is 95/95 `rc=0`, yet four of the tests that "prove" item/hero/save
mechanics call `sim.*` directly and never touch the card, the tap, or the menu
(`crates/wc3-game/tests/playable_loop.rs:63-83` sets `resources = 5000` and calls `sim.train_def`).

---

## 1. Definition of done — "playable 1v1 melee on Justice"

A match counts as playable when all thirteen hold **from the player's seat** (card/tap/hotkey), on the real
map, with no test-only shortcut:

1. **Economy loop.** Five workers per seat, gold mine 12,500 with depletion, 10 gold/trip, 10 lumber/trip,
   50 lumber/tree, food cap from farms/halls (10/100), upkeep 50/80 with 30 %/60 % gold tax, and *melee
   starting resources* (not a demo pool).
2. **Tech tree with requirements.** Every `Requires=` gate is enforced for training and building, and every
   gate is *reachable*: hall tiers upgrade in place (Keep, Castle), which unlock the tier-2/3 units and
   upgrades of all four races.
3. **Unit production queues.** Up to 7 queued units per producer, cancel with refund, rally point, food-blocked
   units wait and pop when food frees.
4. **Hero with three abilities and a level.** Hire at the altar, level 1–10 with XP from creep/unit kills,
   one skill point per level, ultimate at 6, each ability learnable and castable at its own level, death →
   altar revive (700 g / 150 s).
5. **Items, shops, inventory.** Ground items are drawn and pickable, six-slot inventory with use/drop, shop
   buy/sell with stock/restock, creep drop tables.
6. **Creeps.** Camps aggro (600), leash and return, heal at home, and **respawn** after the camp timer; kills
   pay XP + bounty.
7. **Fog of war + minimap.** Per-player visibility, explored mask, last-seen blips, fog rendered over terrain,
   minimap click-to-camera and minimap orders.
8. **Group orders.** Attack-move, move, stop, hold, patrol — all issued through the command stream, all
   visible in the card and on hotkeys.
9. **Building construction.** Site → worker channels → progress; cancel with partial refund; repair at
   proportional gold/lumber cost; footprint blocks pathing until death.
10. **Win/lose + score screen.** Hall destroyed ends the match; victory/defeat screen carries the WC3 score
    table (units trained/killed/lost, gold/lumber gathered, heroes).
11. **Hotkeys and submenus.** Retail letters from object data; worker build pages; hero ability hotkeys;
    producer submenu per building.
12. **Audio per def.** Each unit uses its own voice set; each ability/creep/building event resolves to a
    retail row; positional and error-free.
13. **Save/load.** Full sim round-trip exposed in the menu (save slot → file → load → identical state hash).

---

## 2. Per-system status with evidence

### 2.1 Economy loop — **PARTIAL**

| Sub-item | Status | Evidence / exact gap |
|---|---|---|
| 5 workers per seat | **works** | `spawn_melee_bases` fills to five workers (`crates/wc3-game/src/sim_bridge.rs:611-625`); device log `GAME_SCENE_SEATS … player_workers=5 enemy_workers=5` (`analysis/device-build-probe.log:4846`) |
| Gold mine 12,500 + depletion | **works** | `GOLD_MINE_GOLD = 12_500` (`crates/wc3-sim/src/lib.rs:90`), decremented per load (`lib.rs:5540-5550`) |
| 10 g / 10 l per trip, 50 hp trees | **works** | `HARVEST_CAPACITY = 10` (`lib.rs:118`), `HARVEST_GATHER_TICKS = 60` (`lib.rs:120`), `HARVEST_LUMBER_CAPACITY = 10` (`lib.rs:122`), tree hp 50 (`crates/wc3-sim/src/destructables_gen.rs:1142`) |
| Food cap from buildings | **works** | `supply_cap` sums `supply_provided` of finished buildings, caps at 100 (`lib.rs:5763-5772`) |
| Upkeep 50/80, 30 %/60 % | **works** | `upkeep()` (`lib.rs:5825-5838`), gold taxed in `deposit_gold` (`lib.rs:5742`), lumber taxed in `deposit_lumber` (`lib.rs:3659`); test `upkeep_tax_boundaries_50_80` (`crates/wc3-sim/tests/upkeep.rs:21`) |
| **Starting resources** | **missing / wrong** | `Sim::new` hardcodes `resources: [1000, 1000]`, `lumber: [1000, 1000]` (`lib.rs:949-950`) and no melee init overrides them (`setup_melee_layout` `lib.rs:4283-4530` never touches `resources`/`lumber`; `sim_bridge.rs` has no assignment). 1000 lumber removes the need to chop for a whole game. TFT melee default is 500 g / 150 l — `reported-unverified` (not present in any extracted SLK; it is an engine melee constant, `analysis/war3x_local/**/Units/MiscGame.txt` has no such key). |
| Upkeep applies to lumber | **wrong vs WC3** (`inferred`) | `lib.rs:3659` multiplies lumber by the same `income_permille`; WC3 taxes gold income only. Confirm against the engine (constraints.json has no upkeep row) before changing. |
| Workers per mine (5 cap) | **partial** | unlimited workers can be ordered onto one mine; no saturation queue (`Command::Harvest` `lib.rs:678-681`) |

### 2.2 Tech tree with requirements — **PARTIAL**

| Sub-item | Status | Evidence / gap |
|---|---|---|
| `Requires=` on training | **works** | `enqueue_production` checks `def.requires` and `def.trained_at` (`lib.rs:4957-4973`); test `tech_requirements_are_enforced_for_data_driven_training` (`crates/wc3-sim/tests/production.rs:79`) |
| `Requires=` on building | **works** | `can_build_now` → `requirements_met` (`lib.rs:5264-5272`) |
| Per-building `Researches=` | **sim only** | `research_upgrade` is complete (`lib.rs:4831-4887`) and tested, but it has **no caller in `wc3-game`** and there is no `UiAction::Research`/`Command::Research` — the 89 upgrades cannot be started by a human (`crates/wc3-game/src/game.rs:206-235` lists every card action) |
| Hall tier upgrade (Keep/Castle) | **missing** | no building-upgrade command anywhere in `lib.rs`; `hkee`/`hcas` exist only as standalone defs (`crates/wc3-sim/src/roster_gen.rs`) and are absent from `hpea Builds=` (`analysis/war3_full3/Units/HumanUnitFunc.txt:227`), so `hkni` (`requires hcas`) and the tier-2 tech are unreachable |
| Upgrade effects | **works** | `apply_upgrade_delta` (`lib.rs:4904-4912`), `UpgradeResearched` event (`lib.rs:4900`) |
| AI uses upgrades | **missing** | `run_ai` has no research step (`crates/wc3-sim/src/ai.rs:139-166`); grep for `research_upgrade` outside `lib.rs` = 0 hits |

### 2.3 Unit production queues — **PARTIAL (blocking defect A)**

| Sub-item | Status | Evidence / gap |
|---|---|---|
| Queue with cost/time | **works** | `enqueue_production` (`lib.rs:4937-4991`), `advance_production` |
| **Train from barracks/altar via card** | **broken** | card publishes `UiAction::TrainDef` for any producer (`crates/wc3-game/src/hud.rs:1609-1637`, fallback list at `hud.rs:1543-1563`), but the handler is `if let Some(hall) = self.selected_hall()` (`game.rs:4826-4828`) and `selected_hall` requires `is_main_hall()` (`game.rs:2141-2153`). For a barracks the `if let` fails silently: no toast, no error, no order. |
| Queue length 7 | **wrong** | `PRODUCTION_QUEUE_MAX = 5` (`lib.rs:114`); WC3 is 7 |
| Cancel with refund | **sim only** | `Command::CancelTrain` (`lib.rs:688`) + test (`crates/wc3-sim/tests/production.rs:28`); no card path — the queue rows are explicitly info-only (`crates/wc3-game/src/hud.rs:1793-1800`) |
| Rally point | **works** | building selected + ground tap (`game.rs:4113-4126`), test `rally_point_sends_new_units_there` (`crates/wc3-sim/tests/production.rs:50`) |
| Food-blocked units wait and pop | **works** | `Production.blocked` set when supply is full (`lib.rs`, `production.rs` tests) |
| AI production | **works** | AI trains from hall/barracks directly via `sim.train_def` (`crates/wc3-sim/src/ai.rs:375-430`, `ai_army`), bypassing the card — so the opponent can build an army while the human cannot |

### 2.4 Hero with three abilities and a level — **MOSTLY UNREACHABLE**

| Sub-item | Status | Evidence / gap |
|---|---|---|
| Hire at altar | **broken (same defect A)** | altar `trains: ["Hamg","Hmkg","Hpal"]` (`crates/wc3-sim/src/roster_gen.rs:1472`); card shows them (`hud.rs:1543-1563`); handler no-ops (`game.rs:4826-4838`) |
| XP from kills, hero range 1200 | **works** | `grant_hero_xp` on death (`lib.rs:3211-3240`), `hero_exp_range` from gameplay constants |
| Attributes per level | **works** | `apply_hero_level` (`lib.rs:4188`), test `heroes_attributes_scale_with_level` (`crates/wc3-sim/tests/heroes.rs:92`) |
| Skill points per level, ultimate reqLevel 6 | **sim works, UI dead** | `learn_ability` (`lib.rs:4475-4505`) + `Command::LearnAbility` (`lib.rs:707`) exist; no `UiAction::LearnAbility` exists (`game.rs:206-235`) and the HUD leaves the slot empty on purpose (`hud.rs:1787-1792`) |
| Learned list has gameplay effect | **no** | `learned` is written (`lib.rs:4501`) and never read; casting is gated by the hardcoded `def.spells` list (`crates/wc3-sim/src/abilities.rs:433`), so learning changes nothing |
| **Ability coverage** | **10 of 49 hero ability ids** | 12 hero defs carry 49 `ability_ids` but only 10 ids map to a `Spell` variant: Blade Master, Tauren Chieftain, Priestess of the Moon and Demon Hunter have **zero** castable abilities (`crates/wc3-sim/src/abilities.rs:16-41`; roster `spells:` fields). The card disables the rest as "DATA" (`hud.rs:1764`) — this is operator bug #8 "not all abilities" |
| Death → altar revive | **missing (no caller)** | `revive_hero` (`lib.rs:4508-4545`) + `advance_revival` (`lib.rs:4547+`) are tested (`crates/wc3-sim/tests/heroes.rs:54`) but **nothing calls `revive_hero`** — a dead hero is dead forever |
| Hero level / XP shown on HUD | **missing** | no `level`/`xp` in the info panel; grep `level` in `crates/wc3-game/src/mobile_hud.rs` returns only layout `expanded()` helpers |
| AI plays a hero | **missing** | no `hero` reference at all in `crates/wc3-sim/src/ai.rs` |

### 2.5 Items, shops, inventory — **UNREACHABLE**

| Sub-item | Status | Evidence / gap |
|---|---|---|
| 273 item rows typed | **works** | `crates/wc3-data/src/defs.rs` items, test `items_data_rows` |
| Creep drop tables | **works** | `creep_drop` on death (`lib.rs:6166`), `wc3-data/src/drops.rs` |
| **Ground item rendering** | **missing** | `Scene` has `units`, `rings`, `health`, `ui`, `overlay` only (`crates/wc3-game/src/game.rs:368-377`) — a dropped item is invisible |
| **Pick up an item** | **missing** | `pickup_item` (`lib.rs:3734-3760`) has no `wc3-game` caller (only JASS natives + tests: `crates/wc3-sim/src/jass/natives/unit.rs:1326`) |
| Inventory use | **works** | card item slots (`hud.rs:1683-1725`) → `UiAction::UseItem` → `sim.use_item` (`game.rs:4878`) |
| Drop from inventory | **missing in UI** | `drop_item` (`lib.rs:3763`) called only from sim internals (`lib.rs:3831,3892`) and JASS |
| **Shop buy/sell** | **missing in UI** | `buy_item` (`lib.rs:3969`) has **no caller**; `register_shop` runs for every spawned shop (`lib.rs:1260`), so the stock state exists but is unreachable |
| Shops on Justice | **present in the map** | `ngme` (Goblin Merchant) ×2, `ngad` ×2, `nmr8` ×2, `ntav` ×1 (`map_census` on the Justice archive); `ngme` sells 11 items (`crates/wc3-sim/src/shops_gen.rs`) |
| Tavern / mercenary hire | **missing in UI** | `ntav` sells heroes `Nngs,Nbrn,Npbm,Nplh,Nbst` and `nmr8` sells units (`crates/wc3-sim/src/data.rs:772-800`); `buy_unit` (`lib.rs:3949`) has no caller; the card has no shop branch (`hud.rs` grep `shop` = comments only) |

### 2.6 Creeps — **PARTIAL (respawn missing)**

| Sub-item | Status | Evidence / gap |
|---|---|---|
| Camps registered from Justice | **works** | 22 camps on the device scene (`analysis/device-build-probe.log:4845`), clustering radius 600 (`lib.rs:3520-3530`) |
| Aggro / leash / return home / heal | **works** | `advance_creeps` (`lib.rs:4599-4686`), `retaliate` (`lib.rs:3257`) |
| Kill pays XP + bounty | **works** | `lib.rs:3212-3245` (`grant_hero_xp`, `grant_kill_bounty`); **note: MECHANICS_GAP_MAP row 40 says "no XP/gold reward hook" — that note is stale** |
| **Camp respawn** | **missing** | no respawn timer anywhere (`grep respawn` in `lib.rs` = 0 hits); `Camp.units` keeps dead ids and `advance_creeps` skips dead ones (`lib.rs:4616-4619`), so a cleared map stays empty forever |

### 2.7 Fog of war + minimap — **WORKS**

| Sub-item | Status | Evidence |
|---|---|---|
| Per-player visibility + explored | **works** | `update_fog` (`lib.rs:5898-5925`), `is_visible` (`lib.rs:5950`) |
| Fog hidden models | **works** | `scene()` skips invisible enemies (`game.rs:4986-4990`), test `ui_fog_hides_enemy_models_but_keeps_visible_friendlies` (`game.rs:7821`) |
| Fog overlay rendered | **works** | `Renderer::set_fog` (`crates/wc3-game/src/render.rs:329-349`), called per frame from the app (`apps/android-native/src/lib.rs:1603`) |
| Minimap terrain + blips + last-seen | **works** | `minimap_blips` (`hud.rs:1357`), `blips` filters by visibility and adds faded last-seen (`crates/wc3-game/src/ui/minimap.rs:51-90`) |
| Minimap fog mask | **works** | `minimap_fog_cells` (`game.rs:4601`) |
| Day/night affects sight | **missing** (`inferred`) | day/night is lighting only (`crates/wc3-game/src/render.rs` day-night light); no sight change |

### 2.8 Group orders — **WORKS**

`UiAction::{AttackMove, Move, Stop, Hold, Patrol}` (`game.rs:206-235`) → `Command::{MoveAttack, Move, Stop, Hold}` (`lib.rs:656-716`) or `OrderSpec::Patrol` (`lib.rs:222`); card slots (`hud.rs:1729-1736`); tests `crates/wc3-sim/tests/movement.rs` (14) and `crates/wc3-input/tests/ui_orders_targeting.rs`.

One inconsistency: patrol is issued **immediately** via `sim.issue` instead of through `pending`
(`game.rs:4258-4274`), so it bypasses the tick command batch used by every other order.

### 2.9 Building construction — **PARTIAL**

| Sub-item | Status | Evidence / gap |
|---|---|---|
| Place → worker walks → channels → completes | **works** | `place_building` / `place_building_ordered` (`lib.rs:5294`, `lib.rs:5306`), `advance_construction` (`lib.rs:5434`), test `build_via_card_slot_places_a_building` (`crates/wc3-game/tests/build_via_card.rs:41`) |
| Placement validity | **works** | `can_place` (`lib.rs:5275`), device `PLACE_BLOCKED` lines (`analysis/agent-qa/operator-bugs/logcat.txt`) |
| Footprint blocks pathing | **works** | `blocked_cells` free/occupy (`lib.rs:3195-3210`) |
| **Cancel construction + refund** | **missing in UI** | `cancel_construction` (`lib.rs:5349-5375`) + test (`crates/wc3-sim/tests/production.rs:150`); no `UiAction`, no handler — a misplaced building is a permanent gold loss |
| Repair | **works** | `advance_repairs` (`lib.rs:5381-5433`), intent at `game.rs:4061-4082`; cost 50 % of build value (`REPAIR_COST_PERMILLE = 500`, `lib.rs:86`) vs WC3 35 % (`inferred`) |

### 2.10 Win/lose + score screen — **PARTIAL**

| Sub-item | Status | Evidence / gap |
|---|---|---|
| Defeat on hall destroyed | **works** | `check_victory` (`lib.rs:6173-6189`), `Game::finish` (`game.rs:2958-3005`), stinger audio (`game.rs:2996`) |
| Victory/defeat screen | **works** | `Screen::{Victory,Defeat}` (`game.rs:2971-2974`) with kills/trained counters (`game.rs:2980-2990`) |
| **Score screen (WC3 table)** | **missing (dead button)** | a `SCORE` button exists on the victory screen (`crates/wc3-game/src/mobile_hud.rs:1544`, action 10) but `activate_menu` handles only RESTART/MENU and falls through to `sync_menu()` (`crates/wc3-game/src/game.rs:3643-3649`); no table code exists, the screen prints only `GAME_VICTORY tick= kills= trained=` (`game.rs:2980-2990`) |

### 2.11 Hotkeys and submenus — **PARTIAL**

| Sub-item | Status | Evidence / gap |
|---|---|---|
| A/M/S/H/P + control groups | **works** | `hotkey` dispatch (`game.rs:2359`), `wc3-input/src/remap.rs`, tests `ui_hotkeys*` |
| Card letters from object data | **works** | `ability_hotkey` (`hud.rs:1753-1757`), `card_hotkey` fallback |
| Worker build pages | **works** | 9-slot pages + MORE (`hud.rs:1630-1665`) |
| Producer submenus | **broken for all non-hall producers** | the card exists, the action is inert (defect A) |
| Cancel/rally/queue hotkeys | **missing** | queue rows are info-only (`hud.rs:1793-1800`); rally only by ground tap |

### 2.12 Audio per def — **WORKS**

Per-def acknowledgement/attack/death sets (`crates/wc3-audio/src/game_audio.rs:603-660`), 15 gameplay events
14 mapped (`crates/wc3-audio/src/events.rs`), device `underruns=0` (`analysis/agent-qa/operator-bugs/logcat.txt`).
Operator row #4 stays `open` only because the device re-shoot is pending (row #13 is already `verified`); the mapping itself is present.

### 2.13 Save/load — **UNREACHABLE**

`Sim::save_to_path` / `save_digest` exist (`crates/wc3-sim/src/save.rs:5043-5060`), 600-tick round-trip test passes
(`wc3-sim --test save_load`), but the menu says the opposite: *"today `Sim` has no save/load, so the placeholder is
explicit (`SaveStatus::unavailable`)"* (`crates/wc3-game/src/ui/menu.rs:1-8`, `134-140`). No UI caller.

---

## 3. What a human can actually do in a match today

Observed from the code paths above, in the order a player meets them:

1. Start on Justice: hall + 5 workers + **1000 gold / 1000 lumber** (`GAME_STATE … gold=1004` in the device log).
2. Select workers, harvest gold/lumber — works.
3. Build from the 10-entry build menu — works, including barracks, altar, farms, towers.
4. Select the hall, train peasants — works.
5. Select the barracks → the card shows Footman/Rifleman/Knight → **tap does nothing** (defect A).
6. Select the altar → the card shows its three heroes (`roster_gen.rs:1472` Human, `:2947` Orc, `:5956` Night Elf) → **tap does nothing** (defect A).
7. Research an upgrade → **no button exists**.
8. Kill a creep → it drops an item → the item is **invisible and unpickable**.
9. Build a Keep → **no such action**; Knights are unreachable.
10. Lose the hero (if one existed) → **never revives**.
11. Lose the hall → DEFEAT screen with two counters.

So the honest description of the current build on Justice is: **a terrain/economy sandbox with a working
opponent AI and a non-functional army pipeline.**

---

## 4. Mechanic-id table (TEST_POLICY shape)

`today` = the test that exists **now** (it proves the sim method, not the mechanic); `must write` = the test
required by `TEST_POLICY.md` (one mechanic → one test, id in the name, no `#[ignore]`). New ids extend the
existing `MECHANICS_GAP_MAP.md` namespace.

| Mechanic id | Status | Owner layer | Test today | Test that must be written |
|---|---|---|---|---|
| `production.card-producer` | **BROKEN** | wc3-game (`game.rs:4826`) | none (`playable_loop_justice` calls `sim.train_def` at `tests/playable_loop.rs:82`) | `production_card_producer_trains_from_barracks` — tap barracks → tap Footman → 600 ticks → `hfoo` exists, gold debited |
| `production.cancel-queued` | sim-only | wc3-game + hud | `cancel_refunds_and_advances_the_queue` (`tests/production.rs:28`) | `production_cancel_queued_refunds_from_the_card` |
| `production.queue-7` | wrong (5) | wc3-sim (`lib.rs:114`) | `production_queue_is_bounded` (`tests/production.rs:9`) | `production_queue_seven_units` |
| `buildings.upgrade-tier` | **MISSING** | wc3-sim + game | none | `buildings_upgrade_tier_keep_castle_unlocks_knights` |
| `buildings.cancel-construction` | sim-only | wc3-game | `cancel_construction_refunds_and_frees_the_footprint` (`tests/production.rs:150`) | `buildings_cancel_construction_from_the_card_refunds` |
| `upgrades.card` | **MISSING** | wc3-game + hud | `wc3-sim --test upgrades_execution` (sim only) | `upgrades_card_research_from_the_researches_list` |
| `heroes.learn-ability` | sim-only | wc3-game (`hud.rs:1787`) | `heroes_skill_points_ultimate_at_6` (`tests/heroes.rs:6`) | `heroes_learn_ability_from_the_card_grants_level_one` |
| `heroes.ability-level` | **MISSING** | wc3-sim (`abilities.rs:433`) | none | `heroes_ability_level_scales_damage_and_unlocks_ultimate` |
| `heroes.ability-coverage` | **10/49** | wc3-sim | `abilities_execution_typed_re_classification_763` (classification only) | `heroes_ability_coverage_every_roster_id_is_castable_or_data_only_on_the_card` |
| `heroes.revive.ui` | **MISSING** | wc3-game | `heroes_revive_altar_700g_150s` (`tests/heroes.rs:54`) | `heroes_revive_ui_scheduled_by_altar_after_death` |
| `items.ground-render` | **MISSING** | wc3-game (`game.rs:368`) | none | `items_ground_render_drop_is_drawn_and_pickable` |
| `items.pickup` | **MISSING** | wc3-game | `projectiles_items` (sim) | `items_pickup_by_tap_moves_into_inventory` |
| `items.shop-buy` | **MISSING** | wc3-game + hud | `items_shops` (`wc3-data`) | `items_shop_buy_from_the_card_debits_and_stocks` |
| `items.shop-sell` | **MISSING** | wc3-game | none | `items_shop_sell_returns_gold_and_restocks` |
| `melee.tavern-hire` | **MISSING** | wc3-game | none | `melee_tavern_hire_hero_from_ntav` |
| `melee.mercenary-hire` | **MISSING** | wc3-game | none | `melee_mercenary_hire_from_nmr8` |
| `creeps.camp-respawn` | **MISSING** | wc3-sim (`lib.rs:4599`) | none | `creeps_camp_respawn_after_timer` |
| `economy.start-resources` | **WRONG** | wc3-sim (`lib.rs:949`) | none | `economy_start_resources_per_ruleset` |
| `hud.hero-panel` | **MISSING** | wc3-game (mobile_hud) | none | `hud_hero_panel_shows_level_and_xp` |
| `score.screen` | **MISSING** | wc3-game | none | `score_screen_victory_shows_units_and_resources` |
| `persistence.ui` | **MISSING** | wc3-game (`ui/menu.rs`) | `wc3-sim --test save_load` | `persistence_save_slot_round_trips_through_the_menu` |
| `ai.hero` | **MISSING** | wc3-sim (`ai.rs`) | none | `ai_hero_trained_and_leveled` |
| `ai.upgrades` | **MISSING** | wc3-sim (`ai.rs`) | none | `ai_researches_race_upgrades` |
| `ui.patrol-through-command-stream` | partial | wc3-game (`game.rs:4258`) | `ui_orders_targeting` | `ui_patrol_issues_through_pending_commands` |

---

## 5. Prioritised work list

### P0 — blocks the operator's verdict

P0-1 is the hard blocker (no human army exists); P0-2…P0-8 are what makes a fixed army pipeline still not *Warcraft III*.
Each one is independently shippable and independently falsifiable.

**P0-1 — Route card training to the selected producer, not the selected hall.**
Evidence: `game.rs:4826-4828` + `game.rs:2141-2153`; card population `hud.rs:1543-1637`.
Fix: add `selected_producer()` (building with non-empty `trains`) and use it in both `UiAction::TrainDef`
and `UiAction::Train`; when no producer is selected, toast the reason instead of returning silently.
Acceptance:
```
scripts/h2-cargo test -p wc3-game --test production_card --offline production_card_producer_trains_from_barracks -- --nocapture
```
Falsification: one line in the run's stdout — `GAME_TRAIN hfoo barracks=<id>` followed by a footman owned by
player 0 existing at tick+600, with gold debited by the footman's `stats.cost` (160 in this roster, `roster_gen.rs:83`) and `supply` +2.

**P0-2 — Make the altar train heroes and wire hero learning + revive.**
Evidence: altar band (`roster_gen.rs:1472`), dead handler (P0-1 covers training), `hud.rs:1787-1792`,
`revive_hero` no caller (`lib.rs:4508`).
Fix: (a) training through P0-1; (b) `UiAction::LearnAbility(&'static str)` + a LEARN submenu that lists the
hero's un-learned ids with `req_level` gating → `Command::LearnAbility`; (c) on hero death, schedule the altar
revive automatically (WC3 behaviour: the altar queues it) with the 700 g check.
Acceptance:
```
scripts/h2-cargo test -p wc3-game --test heroes --offline heroes_learn_ability_from_the_card_grants_level_one -- --nocapture
scripts/h2-cargo test -p wc3-game --test heroes --offline heroes_revive_scheduled_by_altar_after_death -- --nocapture
```
Falsification: `HERO_LEARN id=AHhb level=1` in the log and `hero.skill_points` decreasing 1 → 0; a dead hero
returns to `alive=true` at the altar at tick+4500 with gold −700.

**P0-3 — Hero ability coverage: every hero's three basic abilities + ultimate must do something.**
Evidence: 10/49 (`abilities.rs:16-41`), card "DATA" (`hud.rs:1764`), operator bug #8.
Fix: either map the remaining ids onto the generic `GenericAbility` runner (already used by items,
`lib.rs:3818-3826`) or classify them as data-only with a card entry that says so and is disabled — no button
may claim to be an ability and do nothing.
Acceptance:
```
scripts/h2-cargo test -p wc3-game --test heroes --offline heroes_ability_every_roster_id_is_castable_or_data_only_on_the_card -- --nocapture
```
Falsification: for all 12 hero defs, count card slots with `enabled && action != Stop` ≥ 3, and every
`ability_ids` entry resolves to either a `Spell` or an explicit `DATA-ONLY` classification; the test prints the
12×4 matrix.

**P0-4 — Ground items: render, pick up, drop.**
Evidence: `Scene` (`game.rs:368-377`), `pickup_item` no caller (`lib.rs:3734`).
Fix: add `items: Vec<ItemDraw>` to `Scene` (icon quad or item MDX at `item.pos`), tap-to-pickup with a
6-slot-full refusal, card drop action.
Acceptance:
```
scripts/h2-cargo test -p wc3-game --test items --offline items_ground_drop_is_drawn_and_pickable -- --nocapture
```
Falsification: after `creep_drop`, `scene().items.len() == 1`; after the tap, the unit's `items.len() == 1` and
`scene().items.is_empty()`.

**P0-5 — Melee starting resources.**
Evidence: `lib.rs:949-950`, no melee init.
Fix: a `melee_start(owner)` on the sim (or `setup_melee*`) that sets gold/lumber to the melee constants, with
the ruleset variant (RoC/TFT), applied from `sim_bridge::spawn_melee_bases`/`setup_melee_layout`.
Acceptance:
```
scripts/h2-cargo test -p wc3-sim --test melee_matrix --offline economy_start_resources_per_ruleset -- --nocapture
```
Falsification: `GAME_SCENE_SEATS … gold=… lumber=…` in the log shows the constants, and the value is < 1000 for
lumber, so that a farm (80 lumber) is a real decision in the first minute.

**P0-6 — Tech tier reachability (hall upgrade).**
Evidence: no upgrade command; `hkni requires hcas`; `hcas` not buildable.
Fix: `Command::UpgradeBuilding { building, to_def }` (cost/time/construct lifecycle reuse), card entry on the
hall for `hkee`/`hcas`, `requires` chain intact; AI uses the same command.
Acceptance:
```
scripts/h2-cargo test -p wc3-sim --test melee_races --offline buildings_upgrade_tier_keep_castle_unlocks_knights -- --nocapture
```
Falsification: `sim.upgrade_level`-style log line `HALL_UPGRADE htow->hkee` then `hkee->hcas`, and `train_def(hbar,"hkni")`
returning `Ok(())` only after `hcas` exists.

**P0-7 — Upgrades researchable from the card.**
Evidence: `research_upgrade` no caller (`lib.rs:4831`), no `UiAction`.
Fix: a `RESEARCH` submenu listing the selected building's `researches` with cost/level/requirements →
`Command::Research` → `research_upgrade`.
Acceptance:
```
scripts/h2-cargo test -p wc3-game --test upgrades --offline upgrades_card_research_from_the_researches_list -- --nocapture
```
Falsification: `RESEARCH rhme1 … complete` and `upgrade_level(0,"rhme1") == 1` with existing units' damage +1.

**P0-8 — Cancel production and cancel construction from the card.**
Evidence: `CancelTrain` (`lib.rs:688`), `cancel_construction` (`lib.rs:5349`), queue rows info-only (`hud.rs:1793-1800`).
Fix: queue rows become buttons (tap → cancel + refund); a selected construction site shows a CANCEL button.
Acceptance:
```
scripts/h2-cargo test -p wc3-game --test production_card --offline production_cancel_queued_refunds_from_the_card -- --nocapture
scripts/h2-cargo test -p wc3-game --test production_card --offline buildings_cancel_construction_from_the_card_refunds -- --nocapture
```
Falsification: gold returns to the pre-order value and the queue/site disappears in the same tick.

### P1 — visibly wrong vs WC3 (the match plays, but not like WC3)

| # | Item | Evidence | Fix | Acceptance |
|---:|---|---|---|---|
| P1-1 | Queue max 5 | `lib.rs:114` | 7 | `production_queue_seven_units` |
| P1-2 | Shops/tavern/mercenary unusable | `buy_item`/`buy_unit` no caller (`lib.rs:3949,3969`); `ntav`/`nmr8`/`ngme` on Justice | shop card page (stock, price, cooldown) | `items_shop_buy_from_the_card_debits_and_stocks`, `melee_tavern_hire_hero_from_ntav` |
| P1-3 | Creep camps never respawn | `lib.rs:4599` | camp timer (WC3 `CreepRespawn`) | `creeps_camp_respawn_after_timer` |
| P1-4 | Dead hero never revives | `lib.rs:4508` | auto-schedule at the altar | `heroes_revive_ui_scheduled_by_altar_after_death` |
| P1-5 | AI has no hero, no upgrades | `ai.rs:138-166` (`run_ai`) | altar first, learn on level, research at hall | `ai_hero_trained_and_leveled`, `ai_researches_race_upgrades` |
| P1-6 | No hero level/XP on HUD | `mobile_hud.rs` (no level field) | portrait plate: level + XP bar + skill-point badge | `hud_hero_panel_shows_level_and_xp` |
| P1-7 | Save/load says "unavailable" while `save.rs` exists | `ui/menu.rs:1-8` | wire slots to `save_to_path`/`from_bytes` | `persistence_save_slot_round_trips_through_the_menu` |
| P1-8 | Repair cost 50 % vs 35 % (`inferred`) | `REPAIR_COST_PERMILLE = 500` (`lib.rs:86`) | confirm from engine, then align | covered by P1 row's test |
| P1-9 | Patrol bypasses the command stream | `game.rs:4258-4274` | push a `Command::Patrol` | `ui_patrol_issues_through_pending_commands` |
| P1-10 | Upkeep taxes lumber (`inferred`) | `lib.rs:3659` | gold only | `economy_upkeep_gold_only` |

### P2 — polish / fidelity

- Queue length 7 already covered above; production **queue display** shows only 4 rows (`hud.rs:1797`).
- Score screen with the WC3 table (`score.screen`) — wire the existing but inert `SCORE` button (`mobile_hud.rs:1544`, `game.rs:3643-3649`).
- Day/night affects sight radius, not only lighting.
- Worker saturation: 5 per mine, extras queue.
- Build ghost tint by validity (`placing_at` exists, `ghost_cell` only).
- Creep camp respawn keeps the camp anchor/leash (no drift).
- Minimap creep dots distinguish camp levels.

---

## 6. Falsification summary (one measurement per P0)

| P0 | Single measurement that proves it fixed |
|---:|---|
| P0-1 | A footman owned by player 0 exists after a *card* tap on the barracks (not after `sim.train_def`), and `GAME_TRAIN` is in the log. |
| P0-2 | `hero.skill_points` drops 1→0 on a card tap and `HERO_LEARN` names the ability; a dead hero is alive again at the altar after the revive timer with 700 g gone. |
| P0-3 | The 12×4 card matrix has ≥3 enabled ability slots per hero and every `ability_ids` entry resolves to a `Spell` or a `DATA-ONLY` tag. |
| P0-4 | `scene().items.len()` 1 → 0 across a pickup tap, `unit.items.len()` 0 → 1. |
| P0-5 | `GAME_SCENE_SEATS` prints the melee constants for both seats; lumber < farm cost × 2. |
| P0-6 | `HALL_UPGRADE htow->hkee->hcas` in the log and `train_def(hbar,"hkni")` becomes `Ok`. |
| P0-7 | `upgrade_level(0,"rhme1") == 1` after a card tap, existing footman damage +1. |
| P0-8 | Gold returns to the exact pre-order/pre-site value and the entity disappears in the same tick. |

---

## 7. Corrections to existing artifacts (do not trust these as-is)

| Artifact | Claim | Reality (2026-11-16) |
|---|---|---|
| `MECHANICS_GAP_MAP.md` row 40 `creeps.creep-jacking` | "no XP/gold reward hook on creep kill" | **stale** — XP and bounty are granted (`lib.rs:3212-3245`) |
| `MECHANICS_GAP_MAP.md` row 18 note | "taxed in gold deposit :4382 and lumber :2635" | line drift; actual `lib.rs:5742` and `lib.rs:3659` |
| `MECHANICS_GAP_MAP.md` row 24 `heroes.leveling` PORTED | "skill points, ultimate at 6" | true in the sim, **unreachable from the card** (`hud.rs:1787-1792`) |
| `MECHANICS_GAP_MAP.md` row 25 `heroes.revive` PORTED | "altar revive" | `revive_hero` has **no caller** |
| `MECHANICS_GAP_MAP.md` row 30 `items.shops` PORTED | "buy_unit debits gold and spawns" | `buy_item`/`buy_unit` have **no game caller** |
| `MECHANICS_GAP_MAP.md` row 72/93 `save.load`/`persistence.save` PORTED | "round-trip" | sim API only; the menu still says unavailable (`ui/menu.rs:1-8`) |
| `FULL_PORT_CRITERIA.md` P0-5 "4 races playable" | gap map `economy.upkeep` + `melee.matrix` | measures **spawns**, not playability; a human cannot produce one combat unit |
| `FULL_PORT_CRITERIA.md` P0-6/P0-7 PASS | "hero completeness", "items/shops/drops" | sim rows only — see §2.4/§2.5 |
| `wc3-sim/tests/justice_1v1.rs:195` `gameplay_justice_1v1_ai_match_has_a_winner` | "a match resolves" | the test **hands side 0 eight free footmen** and forces an aggressive AI config (`justice_1v1.rs:265-275`); it proves the sim can resolve, not that a player can |
| `crates/wc3-game/tests/playable_loop.rs:63-83` | "train + build loop" | sets `resources = 5000` and calls `sim.train_def` directly, bypassing the card path that is broken |

`analysis/agent-re/MELEE_VERIFY_JUSTICE.md` is also stale on shops/creeps ("5 shops, runtime used 0") — the
2026-11-16 device census shows `ngad` 2, `nmr8` 2, `ntav` 1, `ngme` 2 spawned and 22 camps
(`analysis/device-build-probe.log:4740-4846`).

---

## 8. Sequencing for the next generation

1. **P0-1** alone converts the build from "sandbox" to "you can fight back". It is a ~30-line change in
   `game.rs` + one new test file.
2. **P0-5** (one constant block) removes the fake economy before anyone tunes AI pacing.
3. **P0-2 + P0-3** make heroes exist; they are the largest WC3 identity block and the one the operator
   named explicitly in OPERATOR_BUGS #8.
4. **P0-4** unblocks items, which in turn makes creep camps worth clearing (P1-3 respawn).
5. **P0-6 + P0-7** restore the tech tree; only then does `melee.race_*` mean what it says.
6. **P0-8** last among P0s: it is a recovery affordance, not a blocker.

Each P0 lands with its named test, and `run_mechanic_tests.sh --check` must show the new id `rc=0` before the
row flips to PORTED — otherwise the same gap re-opens silently.

---

## Appendix A — environment evidence used

- Device census (2026-11-16): `GAME_SCENE_CENSUS map=TRIGSTR_010 unit_records=99 sloc=2 mines=7
  unit_placed=88 unit_unknown=2`, `GAME_SCENE_MAP … units=88 mines=7 destructables=1997 starts=2 camps=22`,
  `GAME_SCENE_SEATS … player=Orc enemy=Human … workers=5 enemy_workers=5 sim_units=100 sim_mines=7`
  (`analysis/device-build-probe.log:4740-4846`).
- Map census on the Justice archive: `ngme ×2, ngad ×2, nmr8 ×2, ntav ×1, ngol ×7, sloc ×2`
  (`target/debug/examples/map_census "<Justice.w3x>"`).
- Cached mechanic runner: 95 rows, all `rc=0` (`analysis/tech-lead/mechanic_tests.json`, `generated_unix 1794641085`).
- Cargo lock contention prevented a live re-run: `h2-cargo.log` shows `cargo test -p wc3-game --lib` held by
  another session at the time of this audit.
