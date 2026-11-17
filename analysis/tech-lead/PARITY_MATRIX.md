# PARITY_MATRIX.md — PC Warcraft III vs Herofolends-RS

Status: **AUTHORITATIVE SCORECARD** · kind: analysis only, no code changes
> **Layer verification 2026-11-15:** the per-layer acceptance ledger is §11 “LAYER GATES — proof per layer” — 16 steps, **14 PASS / 2 SKIP / 0 FAIL** (the two SKIPs are device inputs: grass frame and death log). Gate: `bash analysis/tech-lead/run_layer_gates.sh`; machine result: `analysis/tech-lead/gate-logs/last-results.json`.
Snapshot: **2026-11-13T07:58Z (10:58 MSK)**, tree `~/src/herofolends-rs` (no git; live merge of 11 layer sessions)
Generated from: `analysis/tech-lead/tools/parity_counts.py` → `analysis/tech-lead/parity_counts.json`,
`analysis/tech-lead/tools/parity_features.py` → `analysis/tech-lead/parity_features.json`,
`analysis/agent-re/scan.json`, `analysis/agent-data/REPORT.md`, `analysis/agent-sim/ROSTER.md`,
`analysis/agent-qa/final/report.json`, `analysis/tech-lead/BASE.md`, `analysis/tech-lead/WC3RE_MERGE_MATRIX.md`,
`analysis/tech-lead/parity-render.md` (sibling render inventory: gallery, portraits, water/weather).

> The tree is being rewritten by the layer owners while this file was produced. Measured examples during the
> session: `SPELL_COUNT` 6 → 11, items 0 → 273, upgrades 0 → 80, `Screen::MainMenu/MapSelect/Loading` added.
> Every "current" number below is a **snapshot at 07:58Z**; the `Verify` column names the command that
> re-measures it. Treat any row older than the last merge as `stale-verify`.

> **RE parity update 2026-11-13T08:35Z** (ledger 350, agent-re): rows G21/G22 and §7 #4/#6/#7 are delivered
> in `crates/wc3-re`/`crates/wc3-jass` (and the `wc3-formats` unit parser):
> `wc3re scan` → `scripts.unimplemented={}` (0 names, 0 call sites), `maps.errors=[]`, 44/44 `main`;
> `wc3re parity` → 44/44 maps, 713 executed triggers, 6 723 `war3mapUnits.doo` placements
> (468 spawned by the engine, 6 255 matched to `CreateAllUnits`), 115 134 doodads. `game.rs` is HUD-owned; the
> patch `analysis/tech-lead/patch-game-parity46.diff` was **applied by HUD on 2026-11-13 12:02 MSK**
> (post-apply SHA-256 `fad4edd2…`) and re-verified by RE: `cargo check -p wc3-game --lib` rc 0,
> `map_units_spawn` 2 passed (90/90 + frame loop), full game lib **95 passed / 18 ignored / 0 failed**.

## 0. Rules used for every number

1. **PC counts** are rows/IDs read from the operator's extracted archives, never from memory or a wiki:
   RoC = `analysis/war3_data` (War3.mpq), TFT = `analysis/war3x_data` (War3x.mpq).
2. **Melee roster** = IDs reachable from the four race `*UnitFunc.txt` files
   (`Units/HumanUnitFunc.txt`, `OrcUnitFunc.txt`, `UndeadUnitFunc.txt`, `NightElfUnitFunc.txt`):
   `Builds=` → buildings, `Trains=` → units (altars train heroes), `Upgrade=` → morph targets
   (hkee/hcas, ostr/ofrt, unp1/unp2, etoa/etoe, tower/ziggurat). Summoned units (water elemental,
   spirit wolf, wards, skeleton) and critters only have art sections and are excluded by construction.
3. **Build side** = what the game can spawn/train/render *from source*, not from docs:
   `crates/wc3-sim/src/roster_gen.rs` (units/buildings), `items_gen.rs`, `upgrades_gen.rs`,
   `crates/wc3-sim/src/abilities.rs` (`SPELL_DEFS`), `crates/wc3-game/src/assets.rs` (`MODEL_PATHS`).
4. **Status**: `FULL` = mechanism + data + evidence; `PARTIAL` = mechanism or data missing; `MISSING` = no mechanism.
   `Coverage` is the measured ratio that justifies the status; section % is the unweighted mean of row coverages
   (a count-weighted aggregate is given in §6.2 for cross-check).
5. **Line numbers** were valid at 07:58Z; hashes of every cited source are in §8, so a moved line is detectable.

## 1. Headline scorecard

| Section | Rows | Coverage (mean of rows) | Count-weighted | Verdict |
|---|---:|---:|---:|---|
| 1. Content (units/buildings/abilities/items/upgrades/buffs/doodads/heroes) | 16 | **52.2 %** | 70.3 % | data layer largely complete for RoC melee; runtime content (meshes, spells, buffs, creeps) is the hole |
| 2. Gameplay (economy, tech tree, combat, heroes, AI, JASS, modes) | 27 | **52.5 %** | — | combat math and both resources exist; heroes/creeps/modes absent |
| 3. Presentation / UI | 17 | **50.6 %** | — | console/HUD is the strongest layer; audio and fog are the weakest |
| 4. Rendering | 10 | **48.5 %** | — | terrain/doodads real; **4 of 216 four-race unit/building meshes** load |
| **Overall (70 rows)** | 70 | **51.4 %** | — | ~half of the shipped feature surface exists in a working, data-grounded form |

Three facts dominate the score:

1. **The data is real and nearly complete; the renderer is not.** 48/50 melee units and 47/49 melee buildings
   exist as data with real stats/models; 273/273 TFT items and 55/55 race upgrades are generated. But the
   renderer loads **4 MDX files** (`assets.rs:41-46`) and maps **99 roster entries onto 4 meshes**
   (only 4/99 render their own model).
2. **Combat math is 1:1; the ability surface is not.** `DAMAGE_MULT_PERMILLE` and the armor formula match
   `Units/MiscGame.txt:103-112` exactly (7 attack × 8 armor), but 11 of 140 melee ability IDs are implemented.
3. **Presentation is deeper than gameplay.** Console (288 px, 4×3 card grid, FDF+skins), minimap with dots,
   portrait icons, upkeep label and touch hotkeys are wired; the lumber *display*, fog rendering, audio and
   the day/night clock are not.

## 2. Content

### 2.1 PC content inventory (measured, RoC / TFT)

| Sheet (`analysis/*/Units/…`) | RoC rows | TFT rows | Notes |
|---|---:|---:|---|
| `UnitData.slk` | 466 | 812 | `race` col 4; heroes = upper-case rawcode (`Hpal`, `Ofar`, `Udea`, `Ekee`) |
| `UnitBalance.slk` | 466 | 812 | HP/armor/cost/food/level/upgrades |
| `UnitWeapons.slk` | 316 | 812 | 150 RoC units have no weapon row (workers/buildings) |
| `UnitAbilities.slk` | 324 | 812 | unit `abilList` + hero `heroAbilList` |
| `unitUI.slk` | 466 | 812 | `isbldg` (RoC only) + model path (both) |
| `AbilityData.slk` | 399 (397 IDs + 2 spacers) | 763 | hero 65/133, item 105/234, unit 227/396 |
| `ItemData.slk` | 132 | 273 | purchasable (`stockMax>0`) 132 / 267 |
| `UpgradeData.slk` | 58 | 89 | human 15/21, orc 13/19, undead 13/18, NE 16/22 |
| `DestructableData.slk` | 128 | 246 | trees/rocks/gates |
| `Doodads/Doodads.slk` | 278 | 469 | props, no gameplay |
| buffs (`*AbilityStrings.txt` `[B…]` keys) | 154 | 154 | no `AbilityBuffData.slk` in this install |

Per race (all rows, incl. campaign/creep; heroes/units/buildings):
RoC human 66 = 12/26/28 · orc 52 = 9/27/16 · undead 47 = 11/18/18 · NE 48 = 11/19/18 · neutral 253 = 0/185/68.
TFT human 108 = 20/42/46 · orc 85 = 17/47/21 · undead 77 = 21/35/21 · NE 78 = 17/30/31 · neutral 464 = 7/371/86.

Melee-only (the set a parity program must ship): **50 units incl. 12 heroes, 49 buildings, 55 researched upgrades,
140 distinct ability IDs**.

### 2.2 Units and buildings per race — PC vs build

| Race | PC melee units (heroes) | In `ROSTER` (trainable) | PC melee buildings | In `ROSTER` (buildable) | Meshes on disk | Meshes loaded |
|---|---:|---:|---:|---:|---:|---:|
| Human | 13 (3) | 12 | 14 | 14 | 47 + 10 | Footman, Peasant, TownHall |
| Orc | 13 (3) | 13 | 11 | 11 | 41 + 9 | — (renders Human meshes) |
| Undead | 12 (3) | 12 | 13 | 11 | 44 + 11 | — (renders Human meshes) |
| Night Elf | 12 (3) | 11 | 11 | 11 | 38 + 16 | Archer |
| **Total** | **50 (12)** | **48** | **49** | **47** | **216** | **4** |

Evidence: PC sets from `*UnitFunc.txt` (`Builds`/`Trains`/`Upgrade`) + `unitUI.isbldg`;
build side `crates/wc3-sim/src/roster_gen.rs` (99 `UnitDef`s, 90 `model_present: true`);
meshes from `analysis/list-War3-clean.txt` (2 048 MDX; 542 under `units*/buildings*`);
loaded meshes `crates/wc3-game/src/assets.rs:41-46`.

**Exact missing IDs (melee):**

| ID | Name (unitUI) | Category | Why it matters |
|---|---|---|---|
| `Emoo` | Priestess of the Moon | Night Elf hero | third NE hero; Trueshot Aura (`AEar`) + Scout (`AEst`), Starfall (`AEsf`) |
| `hmtt` | Steam Tank | Human unit (TFT) | Workshop third unit; mechanical/siege rules |
| `ugol` | Haunted Gold Mine | Undead building | expansion mechanic for Undead (`Builds=` on acolyte) |
| `uzg1` | Spirit Tower | Undead building | ziggurat upgrade target (`Upgrade=` on `uzig`) |

**IDs in `ROSTER` that the melee tree does not produce** (legitimate, produced by abilities, not `Trains`):
`egol` Entangled Gold Mine, `ehpr` Hippogryph Rider, `hmil` Militia, `ushd` Shade.

**Hard renderer limit:** `MODEL_KINDS`/`MODEL_PATHS` = 4 (`assets.rs:34-46`); every unit draws
`assets.model(kind)` (`game.rs:1756-1788`). 95 of 99 roster entries render a substitute mesh
(e.g. Grunt `ogru` has `kind: UnitKind::Footman`, `roster_gen.rs` → renders `Human/Footman.mdx`).
The MDX inventory is 542 unit/building models (216 four-race + 173 creeps + 37 critters + 39 demon + 77 other).

### 2.3 Abilities, items, upgrades, buffs

| Category | PC count | Build count | Coverage | Status | Missing (exact) |
|---|---:|---:|---:|---|---|
| Melee ability IDs (needed by the 99 melee defs) | 140 | 11 | 7.9 % | PARTIAL | 129 IDs; see 2.3.1 |
| — hero abilities | 48 (12×4) | 10 | 20.8 % | PARTIAL | 38 IDs |
| — unit abilities (non-hero) | 92 | 1 (`Ahea` Heal) | 1.1 % | MISSING | 91 IDs |
| Abilities in data (all) | 397 / 763 | 11 | 2.8 % | PARTIAL | 386 / 752 |
| Items | 132 / 273 | 273 (all TFT) | 100 % data | PARTIAL runtime | — (shop/charges missing) |
| Upgrades (race) | 55 | 55 | 100 % | FULL | — (costs gold+lumber, effects applied) |
| Upgrades (all TFT) | 89 | 80 | 89.9 % | PARTIAL | `Rgfo Rguv Rnam Rnat Rnen Rnsb Rnsi Rnsw Roch` (2 glyph, 6 naga, 1 demon) |
| Buffs | 154 | 0 | 0 % | MISSING | no buff system (`abilities.rs` header: "no buff system yet") |

Implemented spells (`crates/wc3-sim/src/abilities.rs:159+`, `SPELL_COUNT = 11` at `:41`):
`AHhb` Holy Light, `AHds` Divine Shield, `AHbz` Blizzard, `Ahea` Heal, `AEer` Entangling Roots,
`AOcl` Chain Lightning, `AHtb` Storm Bolt, `AHtc` Thunder Clap, `AUdc` Death Coil, `AUfn` Frost Nova,
`AUcs` Carrion Swarm.

#### 2.3.1 Melee ability IDs still missing (129 of 140 — exact list)

Taken from `parity_counts.json → parity.melee_ability_missing` (set difference of the 140 IDs referenced by
`UnitAbilities.slk` for melee units/buildings minus the 11 `SPELL_DEFS` IDs).

**Hero abilities, 38 missing** (12 heroes × 4 slots − 10 implemented):

```
AEah AEar AEev AEfn AEim AEmb AEme AEsf AEst AEtq AHab AHad AHav AHbh AHfa AHmt AHre AHwe
AOae AOcr AOeq AOfs AOmi AOre AOsf AOsh AOwk AOws AOww AUan AUau AUav AUdd AUdp AUdr AUfa
AUin AUsl
```

Verified names (`AbilityData.comments`): `AHab` Brilliance Aura, `AHwe` Summon Water Elemental,
`AHmt` Mass Teleport, `AHav` Avatar, `AHbh` Bash, `AEar` Trueshot Aura, `AEsf` Starfall, `AEst` Scout,
`AHfa` Searing Arrows, `AOae` Endurance Aura.

**Unit abilities, 91 missing** (non-hero `abilList` − 1 implemented `Ahea`):

```
Aadm Aaha Aakb Aams Aap1 Abdl Abds Abgl Abgm Abgs Ablo Abrf Abtl Abun Acan Acoa Acoh Acor
Acri Acrs Acyc Adef Adev Adis Adtn Adts Advc Aeat Aens Aent Aesn Aeye Afae Afla Afrb Afrz
Agyb Agyd Agyv Ahar Ahrl Ahrp Ahwd Aimp Ainf Aivs Alam Alit Alsh Ambt Amed Amel Amgl Amic
Amil Amim Aply Apos Aprg Apts Arai Arav Arej Aren Arep Arev Argl Arlm Aro1 Aro2 Aroa Arst
Asac Asal Ashm Aslo Aspa Aspi Aspo Asta Astd Asth Astn Atol Auhf Auns Aven Awar Aweb Awha Sch2
```

Verified names: `Adef` Defend, `Aply` Polymorph, `Aslo` Slow, `Aens` Ensnare, `Ablo` Bloodlust,
`Ara2`→`Aroa` Roar (alias/code differ), `Ahar` Harvest, `Ahrl` Harvest Lumber, `Aenc` Cargo Hold
(Entangled Gold Mine), `Aloa` Load, `Aast` Ancestral Spirit.
`Ahar` is implemented as an engine mechanic (`HarvestPhase`, `lib.rs:50`) rather than as a `SpellDef`;
`Ahrl` is likewise engine-side: lumber lives in `harvest_lumber()`/`deposit_lumber()`, not in a `SpellDef`.

### 2.4 Doodads, destructables, creep/campaign content

| Item | PC | Build | Status | Evidence |
|---|---:|---:|---|---|
| Doodad types (RoC/TFT) | 278 / 469 | art resolved per map | FULL (art) | `doodads.rs`, Human04 = **2 509/2 509** placed, 86 models (`merge-world.md:301,314`) |
| Destructable types (RoC/TFT) | 128 / 246 | 246 generated + HP/lumber | PARTIAL | `DESTRUCTABLES` 246 defs (`destructables_gen.rs`), `spawn_destructable` (`lib.rs:1966`) with HP + pathing block + `damage_destructable`, lumber harvest (`lib.rs:1949`); **no map placement** (`war3map.doo`) and no game call site |
| Creep units (neutral rows) | 185 / 371 | 0 spawnable | MISSING | no `war3mapUnits.doo` placement, no camp AI |
| Campaign units (`CampaignUnitFunc.txt`) | 97 sections | 0 | MISSING | no campaign loader |
| Portrait models | 223 `*_Portrait.mdx` (12 melee heroes included) | render in gallery, 0 in HUD | PARTIAL | portrait panel = 64×64 command-button icon (`game.rs:1974-1993`) |

## 3. Gameplay

`Owner` values are BASE.md §12 layers (`sim`, `HUD`, `world`, `GPU`, `FDF`, `data`, `RE`, `assets`, `anim`, `touch`, `dispatcher`).
`Gate` is the existing/new check that proves the row (QA gate IDs come from `analysis/agent-qa/final/report.json`).

| # | Feature | PC reference | Current evidence (file:line) | Status | Gap | Owner | Gate |
|---|---|---|---|---|---|---|---|
| G1 | Gold economy (harvest → deposit) | `UnitData/UnitBalance` goldcost; `AbilityData` `Ahar` Data12=10; `MiscGame.txt` | `lib.rs:50` `HARVEST_CAPACITY=10`, `HarvestPhase`, `deposit()` `lib.rs:2886`, `GoldMine` | FULL | 10 gold/trip, mine 20 000, passive stipend extra | sim | `cargo test -p wc3-sim economy` |
| G2 | Lumber economy | `UnitBalance.lumbercost`, `Ahar` Data13=10 | sim: `lumber: [i32;2]` `lib.rs:409`, `carrying_lumber` `:170`, `harvest_lumber()` `:1949`, `deposit_lumber()` `:2019`, lumber cost enforced in train/build/research, refunds on cancel. HUD still prints `lumber_text = "0"` `game.rs:1864` | PARTIAL | sim loop done; HUD not wired; no map trees spawned (G22) | sim + HUD | `cargo test -p wc3-sim lumber_cycle` + `hud.resources` |
| G3 | Supply / food | `UnitBalance.fused` (unit food), `fmade` (provided) | `supply_used` `lib.rs:2768` / `supply_cap` `lib.rs:2776`, `MAX_SUPPLY=100` `lib.rs:40`, supply building per race `data.rs` | PARTIAL | cap hard-coded 100 (TFT); RoC cap 90; HUD shows used only, not `used/cap` | sim + HUD | `cargo test -p wc3-sim supply` + `hud.resources` |
| G4 | Upkeep tiers | `https://warcraft.wiki.gg/wiki/Upkeep` (2026-11-13): TFT 0-50/51-80/81-100 = 100/70/40 %; RoC 0-40/41-70/71-90 | `upkeep_permille()` `lib.rs:2959` = `0..=50 → 1000`, `51..=80 → 700`, `_ → 400`; applied in `deposit()` | PARTIAL | correct for TFT, **wrong for RoC by 10** (no ruleset switch) | sim | `cargo test -p wc3-sim upkeep_tiers` |
| G5 | Tech tree / building prerequisites | `*UnitFunc.txt` `Requires=` / `RequiresN=` | training: `lib.rs:2387` `def.requires` enforced; placement: `place_building_impl` **does not read `requires`** | PARTIAL | buildings placeable without prerequisite; no hall-tier gating in the card; lumber costs absent | sim | `cargo test -p wc3-sim tech_tree_requires` |
| G6 | Production queues / orders | WC3 queue (5 visible, shift-queue) | `PRODUCTION_QUEUE_MAX=5` `lib.rs:46`, `CancelTrain` refunds, `ORDER_QUEUE_MAX=8`, rally `set_rally` | PARTIAL | no shift-click queue UI; no per-item queue display beyond name/progress | sim + HUD | `cargo test -p wc3-sim production_queue` |
| G7 | Construction / placement | build time `bldtm`, pathing footprint | `place_building_impl`, `Construction`, `cancel_construction` refund, `footprint_cells` blocks pathing | PARTIAL | builder need not be a worker; no repair; no build-progress FX | sim | `cargo test -p wc3-sim construction` |
| G8 | Damage table | `MiscGame.txt:105-112` `DamageBonus*` (7×8) | `DAMAGE_MULT_PERMILLE` `data.rs:130-145` — verified equal incl. Hero/Divine columns | FULL | — | sim | `data::tests::damage_table_matches_wc3_values` |
| G9 | Armor types + formula | `MiscGame.txt:103` `DefenseArmor=0.06`; 8 `defType` classes | `ArmorType` 8 variants `data.rs:31-40`; `armor_reduction` + `NEG_ARMOR_MULT_PERMILLE` `data.rs:152-173` | FULL | negative-armor table is derived (Liquipedia), not from shipped data | sim | `data::tests::armor_formula_matches_wc3_table` |
| G10 | Attack types | `weapTp1/2` in `UnitWeapons.slk` | `AttackType` 7 variants incl. `Spells`/`Hero`; `from_slk` maps `missile→Pierce` | FULL | second weapon slot (`weapTp2`) unused | sim | `data::tests` |
| G11 | Projectiles / missiles | `Missileart/Missilespeed` in `*UnitFunc.txt`, `UnitWeapons` | no missile entity; damage applied instantly in `apply_damage` `lib.rs:1757` | MISSING | no travel time, no arc, no homing | sim + GPU | new `qa.combat.projectiles` |
| G12 | Ability framework | `AbilityData` levels/targets/costs | `SpellDef` (mana, cd, range, radius, waves, jumps, decay) + `PendingCast`; 11 spells | PARTIAL | no buff application, no channeling, no autocast, level 2-3 values unused | sim | `abilities` tests |
| G13 | Spell coverage | 140 melee IDs | 11 (`SPELL_DEFS`) | PARTIAL | 129 missing; auras are the biggest block | sim | new `qa.abilities.coverage` |
| G14 | Auras / buffs | 154 buff rawcodes | none | MISSING | — | sim | new `qa.buffs` |
| G15 | Heroes: level, attributes, skill points, revive | `UnitBalance` STR/INT/AGI(+plus), `MiscGame.txt` revive costs | `UnitDef` has no level/XP/attributes; heroes are ordinary units with spells | MISSING | heroes cannot level, learn or revive | sim + data | new `qa.heroes.leveling` |
| G16 | Items | `ItemData` 132/273; `AI*` item abilities | `ITEMS` 273, ground items `spawn_item`, `pickup_item` (6 slots, +dmg/+armor/+hp/+mana), `ItemId`; **RE runtime (ROUND 8)**: JASS `Item` entities with level/charges/flags/owner, 6-slot inventory, `UnitUseItem*`, pickup/drop/use events, shop stock + type slots, item pools; `Engine::use_item/unit_items` | PARTIAL | no sim purchase/recipe flow, no item-ability resolution | sim + HUD | `cargo test -p wc3-re --lib ability_item_tests` (14 passed), `analysis/agent-re/PARITY_R8.md` |
| G17 | Upgrade research | `UpgradeData` + `*UpgradeFunc.txt` | `UPGRADES` 80, `Research`, `start_research`, `apply_upgrade_delta` (6 effect kinds), gold+lumber cost | FULL | `Rgfo…Rnsw` missing (2 glyph, 6 naga, 1 demon) | sim | `cargo test -p wc3-sim research` |
| G18 | Creep camps | `NeutralUnitFunc.txt`, `SpawnData.slk` 62 entries, map `war3mapUnits.doo` | `CREEPS` 18 defs (`creeps_gen.rs`), `spawn_camp()` `lib.rs:1903` (aggro 500 / leash 900), `advance_creeps()`; **no game/map call site** | PARTIAL | sim camp exists; camps not placed from maps, no neutral AI beyond aggro/leash | sim + world | `qa.creeps.camps` |
| G19 | Win / lose conditions | `Scripts/Blizzard.j:6153` `MeleeGetAllyStructureCount(indexPlayer) <= 0` → defeat | `check_victory()` `lib.rs:3129`: wins when the enemy has **no alive unit** | PARTIAL | PC rule is *structures*, not units; HUD text says “TOWN HALL IS DESTROYED” while the sim wins on any last unit; no alliance/observer/defeat delay | sim + HUD | `cargo test -p wc3-sim victory_structures` |
| G20 | AI opponent | `Scripts/*.ai` (91 files, 296 functions) | `AiConfig` (workers, army_supply, first_attack_tick, wave_interval, retreat, composition) + `run_ai` | PARTIAL | one scripted composition per race; no build order, expansions, scouting, hero use | sim | `cargo test -p wc3-sim ai` |
| G21 | Map triggers (JASS) | 44 map scripts; 791 natives; 641 functions; 676 TriggerData functions | `wc3-re::jass` VM: 44/44 `main` ok, 1 313 713 steps, **all 791 natives implemented** (`wc3re scan` `scripts.unimplemented={}`); `Engine`+`JassBridge::step_engine` **applied in `game.rs`** (HUD, 2026-11-13 12:02); `map_units_spawn` re-verified rc 0 on the applied file | **DONE** | 713 triggers executed headless (`wc3re parity`); in-game Human04 asserts `main_ok`, executed triggers > 0, `world.game_time` advances | RE + sim + HUD | `analysis/agent-re/qa-jass-map-runtime.json`, `qa_map_runtime.py` (`qa.jass.map_runtime`); QA `triggers.jass` 45/45, `triggers.return` 3/3, `triggers.elseif` 1/1 PASS (`wc3-jass` parser, ROUND 9, `analysis/agent-qa/parity/report-r9.json`) |
| G22 | Map-placed units | `war3mapUnits.doo` | `wc3-formats/src/map.rs:99` parses v7–v13 incl. `random_type` payload; `Engine` spawns placements into the JASS `World` with type+position de-dup (`MapSpawnReport`); Human04 90/90; game patch asserts 90 placements | **DONE (RE layer + verified patch)** | Game loop still uses the fixed scenario until the HUD owner applies the patch | world + sim | `cargo test -p wc3-re --test runtime_wiring`, `cargo test -p wc3-game --lib map_units_spawn` (patched) |
| G23 | Campaigns | 97 campaign unit sections, `Campaign*.txt`, campaign maps | none | MISSING | — | data + HUD | new `qa.campaign.load` |
| G24 | Multiplayer / skirmish setup | `war3map.w3i` players/forces, W3I forces | `Screen::MapSelect` offers one map, no players/race/difficulty; no network code (`TcpStream/UdpSocket` absent) | MISSING | single-player fixed scenario only | HUD + platform | new `qa.skirmish.setup` |
| G25 | Fog of war | per-player visibility | sim keeps `visibility`, `explored` (`lib.rs:411-398`); **no consumer** in `wc3-game` | PARTIAL | state computed, never rendered or used for targeting | world + sim | new `qa.fog.render` |
| G26 | Day/night clock | `MiscData` day length; `MiscUI.txt:80` time-of-day indicator | `day_night_light()` `render.rs:60` grades terrain+water+units+doodads, tested at 00/06/12/18 (`agent_world_day_night_cycle`); `set_time_of_day()` `:285` is **never called by `game.rs`** | PARTIAL | shading done, clock not wired (fixed noon), no night vision | GPU | `agent_world_day_night_cycle` + `qa.daynight` |
| G27 | Weather | `TerrainArt/Weather.slk` (19 rows) | rows loaded `terrain_render.rs:1179`; no weather emitter | MISSING | — | world | new `qa.weather` |

## 4. Presentation / UI

| # | Feature | PC reference | Current evidence (file:line) | Status | Gap | Owner | Gate |
|---|---|---|---|---|---|---|---|
| P1 | Main menu | WC3 main menu (`UI/FrameDef` main) | `Screen::MainMenu/MapSelect/Loading/Playing` `game.rs:35-40`; `menu_view()` `:1093`; `MenuView::for_screen` `mobile_hud.rs:661` | PARTIAL | no options/credits/profile; "QUIT" maps to MainMenu (`activate_menu`) | HUD | `hud.menu` |
| P2 | Map selection | 44 maps discovered by `wc3-re::mapfiles::discover_maps` | `MenuScreen::MapSelect` with START/BACK only; one fixed `menu_map_name` | PARTIAL | no 44-map list, no preview, no player/race slots | HUD + world | new `qa.map.list` |
| P3 | Loading screen | `UI/FrameDef` loading + real asset progress | `Screen::Loading` + `loading_time` animation (1.1 s) | PARTIAL | progress is a timer, not asset loading | HUD | `launch.timing` |
| P4 | Console layout | `UI/FrameDef/Human/Console.fdf`, `war3skins.txt` | `CONSOLE_HEIGHT=288` `mobile_hud.rs:20`, 4×3 cards, FDF+skins `ui.rs` | FULL | 64 px cells = 21.3 dp (advisory 48 dp conflict, QA WARN) | HUD/FDF | `hud.console_height`, `hud.card_grid` |
| P5 | Minimap | minimap art + unit dots + click | `MinimapView` dots/camera `game.rs:2025-2029`, frame texture `UiTexture::Minimap` `mobile_hud.rs:1054`, click→`minimap_to_world` `game.rs:1158` | PARTIAL | no terrain-derived minimap image; dots/camera rect/frame only; no fog on minimap | HUD | `hud.minimap` |
| P6 | Portrait | 3-D portrait model + icon | icon-only: `unit_icon_name` texture + `PortraitFrame` `game.rs:1974-2015` | PARTIAL | no `PortraitAnims` 3-D portrait | HUD + anim | `hud.portrait` |
| P7 | Stats panel | armor, damage, attack type, range, hero attrs | armor/damage `game.rs:1893-1900` (`SelectionView::stats`) | PARTIAL | no attack/armor type, range, hero attributes | HUD | new `qa.hud.stats_full` |
| P8 | Command card | 4×3 with icons/tooltips/costs | `CommandButton`, data-driven `build_menu`, spell slots `game.rs:1262-1280` | PARTIAL | card order/slots are heuristic (`data::build_menu` sorts by cost), not the per-building WC3 layout | HUD | `hud.card_grid` |
| P9 | Tooltips | WC3 tooltip frame + `war3skins` art | `tooltip` string on `HudView` (long-press) | PARTIAL | no border/icon/cost layout from `ToolTipBackground/Border` | HUD/FDF | new `qa.tooltip.layout` |
| P10 | Resource bar | gold, lumber, supply `used/cap` | pills `game.rs:1874-1882`; lumber hard-coded `0` | PARTIAL | lumber fake; supply cap not shown | HUD + sim | `hud.resources` |
| P11 | Upkeep indicator | upkeep icon + % label | `upkeep_label()` `game.rs:701`, wired `:2115` | PARTIAL | label only, thresholds RoC-wrong (G4) | HUD + sim | `hud.resources` |
| P12 | Selection groups | Ctrl/⌘+1..9 with double-tap centre | `ControlGroups` `game.rs:177-261`, digit handling `game.rs:799` | PARTIAL | no on-screen group row | touch/HUD | new `qa.input.groups` |
| P13 | Hotkeys | per-command letters | `activate_hotkey` `game.rs:896`, HUD letters `mobile_hud.rs:273` | FULL | letters are generated from labels, not from `CommandStrings.txt` | HUD | `input.select` |
| P14 | Fog of war (visual) | black mask + explored shading | none (`ui.fog_render` probe empty) | MISSING | — | world | new `qa.fog.render` |
| P15 | Music / ambience | `Sound/Music/*.mp3`, `AmbienceSounds.slk` (60) | `wc3-audio` engine + `game_audio.rs::music` exist; **`wc3-game` has no `wc3-audio` dep** (`Cargo.toml:9-27`) | MISSING | library ready, never started | assets + platform | new `qa.audio.music` |
| P16 | Unit acks / spell sfx | `UnitAckSounds.slk` (747), `UnitCombatSounds.slk` (60) | `GameAudio::unit_ack/unit_attack/ui_click` in library; not called | MISSING | — | assets + platform | new `qa.audio.acks` |
| P17 | 3-D portraits | 223 `*_Portrait.mdx` (`parity-render.md` §2) | gallery renders all 223 posed; HUD shows the 64×64 icon (`game.rs:1974`) | PARTIAL | 223/223 renderable, 0 in HUD | HUD portrait panel must render the model; portrait cameras unimplemented | HUD + anim | `qa.portrait3d` |

## 5. Rendering

| # | Feature | PC reference | Current evidence (file:line) | Status | Coverage | Gap | Owner | Gate |
|---|---|---|---|---|---|---|---|---|
| R1 | Unit meshes per race | 216 four-race unit/building MDX (`list-War3-clean.txt`) | game path: `MODEL_KINDS` 4 / `MODEL_PATHS` 4 `assets.rs:34-46`, 4/99 roster entries render own mesh. Renderer capability already proven: `crates/wc3-game/tests/asset_gallery.rs` renders all 2 048 MDX posed (`parity-render.md` §1) | PARTIAL | **4/216 in-game (1.9 %)**, 216/216 renderable | game loads per `UnitKind`, not per `UnitDef` | world/GPU/assets | `qa.models.race` over the existing gallery |
| R2 | Building meshes | 46 four-race building MDX | only `Buildings/Human/TownHall` in-game; all 10/9/11/16 race buildings render in the gallery (`parity-render.md` §1) | PARTIAL | **1/46 in-game (2.2 %)** | same | world/GPU | `qa.models.buildings` over the gallery |
| R3 | Doodad art per map | 278/469 types | `doodads.rs` resolves `dir/file`+variation+`texFile`; Human04 2 509/2 509 instances, 86 models | FULL | 100 % on the fixture map | other maps unverified | world | `terrain.doodads` |
| R4 | Terrain biomes / tiles | `TerrainArt/Terrain.slk` 98 tiles; 111 TerrainArt BLPs | atlas + `classify()` 6 biomes `terrain_render.rs:37-110`; 4+ layers per cell | PARTIAL | 98/98 tiles loaded; 6/∞ biome classes | biome set is a classifier, not WC3's tile taxonomy | world | `terrain.layers`, `terrain.source` |
| R5 | Water / shore | `TerrainArt/Water.slk` 13 types × 45 frames | `build_water()` `terrain_render.rs:518` + per-tileset `water_material` (shallow/deep colours, `texRate` scroll); all 13 frame-0 textures decode (`parity-render.md` §5) | PARTIAL | 13/13 rows, frame 0 only | 45-frame stepping not implemented; `Shoreline*` doodads not spawned | world | `terrain.seams` + `agent_world_water_weather_cliff_tables` |
| R6 | Cliffs / ramps | `CliffTypes.slk` 26 | height-field + rock tile art only; no cliff mesh | PARTIAL | 0/26 cliff models | real cliff/ramp geometry | world | `terrain.holes` |
| R7 | Effects / particles | `SplatData` 131, `UberSplatData` 43, ability `Missileart` | none in world; blight/glow doodads dropped as meshless (`doodads.rs:441`) | MISSING | 0 % | no particle system, no spell FX | GPU | new `qa.fx.emit` |
| R8 | Shadows | unit/doodad shadows | doodad blob decal `render.rs:704-742` | PARTIAL | doodads only | unit shadows, building shadows | GPU | new `qa.shadows` |
| R9 | Team colour / glow | replaceable ids 1/2 | `replaceable_id == 1 \|\| 2` tint `assets.rs:1089-1090` | FULL | 2/2 ids | — | GPU | `world.units` |
| R10 | Animation sequences | MDX `SEQS` (6 512 sequences, 2 048 models) | `SequenceKind::{Idle,Walk,Attack}` `assets.rs:179-183`; `Animator` samples tracks | PARTIAL | 3 sequence roles of ≥6 | no Death/Birth/Spell/Decay/Stand-Victory | anim | `anim.device_motion` |

## 6. Percentages and method

### 6.1 Row means (the numbers in §1)

| Section | Rows | Sum of coverage | Mean |
|---|---:|---:|---:|
| Content | 16 | 8.355 | **52.2 %** |
| Gameplay | 27 | 14.179 | **52.5 %** |
| Presentation | 17 | 8.600 | **50.6 %** |
| Rendering | 10 | 4.850 | **48.5 %** |
| **All rows** | **70** | **35.984** | **51.4 %** |

Row coverage values (so the mean is reproducible, no hidden weighting):

```
Content   C1 .960  C2 .959  C3 .917  C4 .208  C5 .011  C6 .000  C7 1.000  C8 .500
          C9 1.000 C10 1.000 C11 1.000 C12 .700 C13 .000 C14 .000 C15 .080 C16 .020
Gameplay  G1 1.000 G2 .700  G3 .800  G4 .900  G5 .700  G6 .850  G7 .900  G8 1.000
          G9 1.000 G10 1.000 G11 .000 G12 .800 G13 .079 G14 .000 G15 .100 G16 .500
          G17 1.000 G18 .400 G19 .600 G20 .600 G21 .500 G22 .000 G23 .000 G24 .100
          G25 .350 G26 .300 G27 .000
Present.  P1 .800 P2 .300 P3 .500 P4 1.000 P5 .700 P6 .500 P7 .600 P8 .700 P9 .500
          P10 .600 P11 .800 P12 .700 P13 .900 P14 .000 P15 .000 P16 .000 P17 .000
Render.   R1 .080 R2 .020 R3 1.000 R4 .850 R5 .900 R6 .200 R7 .000 R8 .400 R9 .900 R10 .500
```

### 6.2 Count-weighted cross-check (content only)

Taking only the countable rows (units 48/50, buildings 47/49, heroes 11/12, hero abilities 10/48,
unit abilities 1/92, buffs 0/154, items 273/273, upgrades 80/89, doodads 278/278, destructables 128/128,
creep 0/185, campaign 0/97 — all *data/art* coverage; destructable gameplay is row C12, not here): **931/1 325 = 70.3 %**. The gap between 52.2 % and 70.3 % is exactly the
difference between "the data exists" and "the game uses it": items/upgrades/doodads are large and complete,
while the rows that *use* them (abilities, buffs, creeps, meshes) are near zero. **Plan by the row mean,
not the count-weighted number.**

### 6.3 Sub-metrics worth tracking on their own

| Metric | Now | Target |
|---|---:|---:|
| Unit/building MDX rendered / available | 4 / 542 (0.74 %) | 542/542 |
| Roster entries rendering their own model | 4 / 99 (4.0 %) | 99/99 |
| Melee spells implemented | 11 / 140 (7.9 %) | 140/140 |
| Hero abilities implemented | 10 / 48 (20.8 %) | 48/48 |
| JASS natives implemented | **791 / 791 (100 %)** | 791/791 |
| JASS call sites hitting unimplemented natives | **0** (`scripts.unimplemented={}`) | 0 |
| Map scripts executed by the game | **44 / 44 headless** (`wc3re parity`; Game-loop patch verified `map_units_spawn`) | 44/44 |
| Melee units spawned from the map | **6 723 placements / 468 engine-spawned + 6 255 script-matched** (44 maps) | = `war3mapUnits.doo` count |
| Audio hooks wired | 0 | acks + music + ui |

## 7. Top-20 ordered program increments

Ordered by (parity gain × unblocks) / effort. Each gate is runnable without a device unless stated.

| # | Increment | Why now | Acceptance gate |
|---|---|---|---|
| 1 | **Per-definition model loading** (`MeshAsset` keyed by `UnitDef.model`, LRU, reuse `Animator`) | turns 99 unit types from 4 meshes into their real MDX; the single largest visible gap (R1/R2) | `cargo test -p wc3-game --lib models_per_def` proves ≥ 216 four-race MDX parse+upload; reuse the existing `crates/wc3-game/tests/asset_gallery.rs` harness and have `qa.models.race` report `rendered/required` per race |
| 2 | **Model budget + cache eviction** (from #1) | 542 MDX × textures must fit phone memory | `perf.memory` ≤ 1 536 MB on Human04 with all race models resident-or-evicted; `assets_models_min` raised from 4 to 542 |
| 3 | **Wire lumber to HUD + spawn map destructables** | sim loop and tree HP landed; the HUD still prints 0 and no map tree exists (G2/C12/P10) | `hud.resources` shows real lumber; `qa.economy.lumber` runs a peasant → map tree → 10 lumber → hall cycle; `war3map.doo` trees become destructables |
| 4 | **Map-placed units** (`war3mapUnits.doo` → sim) — **DONE RE** | unlocks creeps, pre-placed bases, campaign maps (G22) | `cargo test -p wc3-game --lib map_units_spawn` asserts in-game unit count == parsed `war3mapUnits.doo` count for Human04 · **delivered**: `wc3-formats` parses v7/sub9 `random_type=-1`, Human04 90/90; `Engine::map_spawn`; patch `patch-game-parity46.diff` passes the gate (`analysis/agent-re/game-map-units-spawn.log`) |
| 5 | **Creep camps + neutral AI** | needs #4; adds the PvE half of the game (G18) | `qa.creeps.camps`: a camp of N creeps spawns, aggros at ≤ 500 units, leashes home |
| 6 | **JASS in the frame loop** (`Engine` + `JassBridge::step_engine` owned by `Game`) — **APPLIED** | 44/44 map scripts already run headless; wiring them makes maps real (G21) | `qa.jass.map_runtime`: Human04 `main` runs in-game, ≥ 1 `TriggerRegisterTimerEvent` action fires, sim state mutates via script · **delivered**: patch boots `Engine` from `Assets` map provenance, swaps the sim into `JassBridge::step_engine` each tick; `map_units_spawn` asserts `game_time` advances + triggers executed; 44/44 headless artifact (`qa-jass-map-runtime.json`) |
| 7 | **Implement the 92 missing natives** (by call-site weight: `TriggerRegisterPlayerStateEvent` 138, `UnitShareVision` 136, `QuestCreateItem` 129, …) — **DONE** | 3 073 call sites currently no-op | `wc3re scan` `scripts.unimplemented` == 0; per-native unit tests · **delivered**: `scripts.unimplemented={}` (0 calls) and 9 per-native gates in `jass::natives::parity_tests` (`TriggerRegisterPlayerStateEvent`, `TriggerRegisterUnitInRange`, `QuestCreateItem`, `SetResourceAmount`, `AttachSoundToUnit`, camera bounds, fog no-ops, `SetUnitMoveSpeed`, `ForForce`/`GetEnumPlayer`) |
| 8 | **Hero leveling, attributes, skill points, revive** | heroes are the genre's core; data columns already read (STR/INT/AGI/plus) | `qa.heroes.leveling`: hero kills → level 2, +1 skill point, HP/mana per `UnitBalance`, revive at Altar costs `MiscGame` values |
| 9 | **Buff/aura framework** | 154 buffs, 0 implemented; blocks 40+ auras and every DoT/slow (G14) | `qa.buffs`: Devotion Aura changes `apply_damage` armor; slow/burn durations tick and expire |
| 10 | **Data-driven SpellDef from `AbilityData`** (levels 1-3, area/cost/cool columns) | converts "hand-written 11 spells" into "any ability row" (G13) | `qa.abilities.coverage` ≥ 60 % of melee IDs with a `SpellDef` generated from SLK, not hard-coded |
| 11 | **Remaining hero abilities (38)** | follows #9/#10; closes C4 · **RE runtime delivered (ROUND 8)**: ability state (level/permanent/cooldown), cast event context + `Engine::cast_ability`, `GetSpell*` natives; 14 ability/item tests pass | `qa.abilities.heroes`: 48/48 hero abilities castable with SLK mana/cd/range |
| 12 | **Projectiles / missiles** | visual + mechanical parity for ranged/siege (G11) | `qa.combat.projectiles`: missile travels `Missilespeed`, damage lands on impact, arc from `Missilearc` |
| 13 | **Tech-tree gating for buildings + hall tiers** | placement ignores `Requires`; Castle without Keep is possible (G5) | `cargo test -p wc3-sim build_requires`: `hcas` denied without `hkee`; card shows only unlocked tiers |
| 14 | **Items: shop, charges, recipes, item abilities** | data is 273/273 · **RE runtime delivered (ROUND 8)**: inventory slots/capacity, charges/use/removal, pickup/drop/use events, item flags, shop stock/slots, item pools (G16); remaining: sim purchase flow, recipes, item abilities | `qa.items.shop`: buy from Goblin Merchant, gold debited, charge decrements, recipe combines |
| 15 | **RoC/TFT ruleset switch** (upkeep 40/70/90 + food cap 90 + RoC-only units) | the app loads the operator's install; rules currently hard-code TFT | `cargo test -p wc3-sim ruleset_roc` vs `ruleset_tft` (thresholds, cap, roster deltas) |
| 16 | **Fog of war rendering + targeting** | state exists, nothing consumes it (G25/P14) | `qa.fog.render`: unit in unexplored area not drawn; attacked-only visibility; screenshot diff |
| 17 | **Audio runtime** (`GameAudio` wired: acks, ui click, spell cast, music, ambience) | library is complete (3 254 lines); zero calls (P15/P16) | `qa.audio.acks`: device log shows `AUDIO_ACK hfoo`, `AUDIO_MUSIC Human1`; `AudioEngine` voice count > 0 |
| 18 | **Effects/particles** (spell FX, splats, blight) | every spell currently resolves with no visual (R7) | `qa.fx.emit`: Blizzard spawns ≥ 6 waves of visible geometry; particle budget ≤ N draws |
| 19 | **Day/night clock + weather** | `set_time_of_day` never called; `Weather.slk` unused (G26/G27) | `qa.daynight`: 24-min cycle changes tint; `qa.weather`: `Weather.slk` row emits |
| 20 | **Campaign + skirmish setup** (map list, players/races/difficulty, then campaigns) | turns a demo scenario into a game shell (G23/G24/P2) | `qa.skirmish.setup`: pick race/opponent/difficulty; `qa.campaign.load`: Human01 loads with objectives |

Dependency notes: 2←1, 5←4, 7←6, 10←9, 11←9, 12←10, 19←18 (weather needs the FX path), 20←4/6.

## 8. Evidence index (snapshot 2026-11-13T07:58Z)

### 8.1 Reproduce the counts

```
cd ~/src/herofolends-rs
python3 analysis/tech-lead/tools/parity_counts.py  --json analysis/tech-lead/parity_counts.json
python3 analysis/tech-lead/tools/parity_features.py --json analysis/tech-lead/parity_features.json
wc3re scan                       # 3 935 BLP / 2 048 MDX / 33 SLK / 67 TXT / 71 FDF / 44 maps / 91 AI
cargo test -p wc3-sim --lib      # combat table, armor, ability, production, research
cargo test -p wc3-game --lib     # HUD/console/world/animator
python3 analysis/agent-qa/acceptance/run_acceptance.py   # 33 PASS / 4 WARN / 0 FAIL (final run)
```

Recorded transcript of the last verification (07:58Z): `analysis/tech-lead/parity-verify-20261113.log`
(roster 99 / items 273 / upgrades 80, abilities 140→11, missing units/buildings, hashes).

`verify_citations.py` contract: pass = every cited **mechanism** still exists in the cited file.
Exact line ⇒ `ok`; symbol moved by a merge ⇒ `moved … => line N` (matrix keeps the snapshot line);
symbol disappeared ⇒ `GONE` and rc=1. So a green run means "nothing was deleted", and the
`moved` list is the mechanical refresh list after each merge — no silent rot.

### 8.2 Source hashes (SHA-256 at 07:58Z — any change invalidates the line numbers)

| file | sha256 (first 16) |
|---|---|
| `crates/wc3-sim/src/lib.rs` | `db6f4a4b13d2a4e9` |
| `crates/wc3-sim/src/data.rs` | `949306b27208ef04` |
| `crates/wc3-sim/src/abilities.rs` | `a38013cfb4f39da5` |
| `crates/wc3-sim/src/roster_gen.rs` | `dca1718dc550caaa` |
| `crates/wc3-sim/src/items_gen.rs` | `32edae874a4e7653` |
| `crates/wc3-sim/src/upgrades_gen.rs` | `dc212e39c5a418fa` |
| `crates/wc3-game/src/game.rs` | `575438de6a7fbba7` |
| `crates/wc3-game/src/assets.rs` | `01b090f52b37c57f` |
| `crates/wc3-game/src/sim_bridge.rs` | `fa7e3e97e7453287` |
| `crates/wc3-game/src/mobile_hud.rs` | `783cbe9db6e18496` |
| `crates/wc3-game/src/terrain_render.rs` | `3c560fd0dae95835` |
| `crates/wc3-game/src/doodads.rs` | `606ef94ca147ee20` |
| `crates/wc3-game/src/render.rs` | `c22506b4c53c414f` |
| `crates/wc3-game/Cargo.toml` | `891164c2fbe30695` |
| `analysis/agent-re/scan.json` | `6fd74210483a655f` |

### 8.3 Web references (stealth-browser MCP, access date 2026-11-13)

| Reference | URL | Used for | Completeness |
|---|---|---|---|
| Warcraft Wiki — Upkeep | `https://warcraft.wiki.gg/wiki/Upkeep` | TFT 0-50/51-80/81-100 = 100/70/40 %; RoC thresholds −10 | full article text captured |
| Liquipedia — Upkeep | `https://liquipedia.net/warcraft/Upkeep` | **not used**: Cloudflare Turnstile challenge, no bypass attempted | blocked (CAPTCHA) |
| Warcraft Wiki — Gold mine | `https://warcraft.wiki.gg/wiki/Gold_mine` | lore only; gold-per-trip taken from `AbilityData.Ahar` instead | partial (not used for numbers) |

Local primary sources replace web citations wherever they exist: `Units/MiscGame.txt:103-112` (armor + damage
tables), `Scripts/Blizzard.j:6153` (melee defeat = no structures), `Units/AbilityData.slk` `Ahar`
(Data12/13 = 10/10), `UI/FrameDef/GlobalStrings.fdf:754-755` (upkeep tooltip format).

### 8.5 Sibling parity documents to fold into v2 of this matrix

| Document | Scope | Overlap with this matrix |
|---|---|---|
| `analysis/tech-lead/parity-render.md` | model gallery (2 048 MDX rendered posed), 223 portraits, 13 water types, 19 weather effects, day/night | R1/R2/P17/R5/G26/G27 — already folded in above |
| `analysis/agent-hud/PARITY_REFS_WC3_PC.md` | console/FDF/skin references | §4 (P4, P9, P11) |
| `analysis/agent-touch/PARITY_TOUCH.md` | touch/gesture parity | §4 (P12) |
| `analysis/agent-ui-framework/PARITY_ROWS.md` | UI framework rows | §4 |

### 8.4 Live-tree churn observed during this session (proof the snapshot matters)

| Observation | First read (≈07:05Z) | Snapshot (07:58Z) |
|---|---|---|
| `SPELL_COUNT` (`abilities.rs`) | 6 | 11 |
| items in sim | 0 (`sim_has_items=false`) | 273 (`items_gen.rs`) |
| upgrades in sim | 0 | 80 (`upgrades_gen.rs`) |
| menu screens | none (`mobile_hud` only) | `Screen::{MainMenu,MapSelect,Loading}` in `game.rs` |
| `ItemId`/`inventory` | absent | present (`lib.rs:178,245`) |
| roster entries | 98 (`ROSTER.md`) | 99 (`roster_gen.rs`) |
| lumber economy | absent (HUD literal `0`) | sim loop + destructable HP + lumber costs (`lib.rs:1949-2027`) |
| creeps / destructables | absent | `CREEPS` 18 + `CAMP` spawn/aggro (`lib.rs:1903`), `DESTRUCTABLES` 246 (`destructables_gen.rs`) |
| spell count in `SPELL_DEFS` | 6 | 11 |

Consequence: rows G16, G17, P1, P2, C7 must be re-measured after the freeze; the counts in §2 are stable
(SLK files do not change), the "build" columns are the moving part.

## 9. Result / risks

**Result:** a complete, number-backed parity matrix: 70 scored rows across content, gameplay, presentation and
rendering, every count traceable to an SLK/scan row or a source line, exact missing IDs listed for units,
buildings, upgrades and abilities, and a 20-step ordered program with acceptance gates.

**Risks**

1. **Concurrent edits** — the tree changes under the analysis (see §8.4). The matrix carries hashes and a
   `Verify` command per row; re-run §8.1 after the freeze before quoting a percentage externally.
2. **Two meanings of "PC count"** — all-rows (e.g. 466/812 units, incl. campaign/creep) vs melee-reachable
   (50 units). Both are listed; use melee for parity, all-rows for data-layer coverage.
3. **Heuristic build menu** — `data::build_menu` picks supply/tower/producer/hall by cost, so "building exists
   in `ROSTER`" ≠ "appears in the WC3 command card in the right slot". P8/G5 capture that gap.
4. **No device run in this analysis** — every rendering/HUD status is code+QA-report based; device gates are
   named but were not executed here (last recorded QA run: 33 PASS / 4 WARN / 0 FAIL).

---

## 10. ROUND6/7 delivery log (2026-11-13 18:00–19:30 MSK, ledgers 346–356)

> Evidence is on disk per row; this log records what the layer owners delivered after the parity snapshot above.
> Status values: `DELIVERED` = artifact + rc evidence present; `VERIFIED` = gates re-run on the current tree.

| # | Increment | Status | Evidence (paths) |
|---|---|---|---|
| 1 | Per-definition model loading (216 four-race MDX) | DELIVERED | `analysis/agent-world/parity/roster.csv` (216 rows: 214 loaded + 2 meshless PlagueCloud), `models-per-def{,-Human,-Orc,-Undead,-NightElf}.png`, `crates/wc3-game/src/model_render.rs` (AnimatorRegistry, 542 cap), `ROUND6-REPORT.md` |
| 2 | Model budget + eviction + ASTC | DELIVERED | `analysis/agent-gpu/ROUND6.md`: 216-model residency (64-model budget, 152 evictions, rejected=0), ASTC 6×6 MAE 4.04 / 8×8 4.70, texture plan 542×256² RGBA8 189.4 MB vs ASTC 21.7/11.9 MB |
| 3 | Lumber → HUD + build menu | DELIVERED | `analysis/agent-hud/round6-check.log` (sim.lumber at game.rs:2238, SLK build menu sim_bridge::build_menu at hud.rs:829 slot9), `mock-console-2414x1080.png`, `icons/commandbuttons_atlas.png` (11.5 MB), `icons/catalog.tsv` |
| 5 | Creep camps + neutral AI | VERIFIED | `analysis/agent-sim/ROUND6.md`, `round6-sim-test.log`: creep aggro ≤500 / leash, 122 passed / 0 failed |
| 8 | Hero leveling/attributes/skill points/revive | DELIVERED | `analysis/agent-data/evidence-round6.txt`, `coverage-report.json` (hero level/attr/skill/revive tests PASS) |
| 9 | Buff/aura framework | DELIVERED | `analysis/agent-data/coverage-report.json`: auras derived from AbilityData numbers, buff store tests PASS |
| 10 | Data-driven SpellDef from AbilityData | VERIFIED | `coverage-report.json`: melee ability coverage 175/176 = **99.4 %** (gate ≥60 %) |
| 13 | Tech-tree gating (hcas requires hkee) | VERIFIED | `crates/wc3-sim/tests/merge.rs` `build_requires` rc=0 |
| 15 | RoC/TFT ruleset switch | VERIFIED | `crates/wc3-sim/tests/merge.rs` `ruleset_roc` / `ruleset_tft` rc=0 |
| 17 | Audio runtime hooks | DELIVERED | `analysis/agent-assets/ROUND7_DEVICE_PRODUCT.md`: `patches/0004-game-audio-hooks.patch` applied in-tree, `cargo check -p wc3-game --lib` rc=0 |
| — | DragonUI donor intake + font registry | DELIVERED | `analysis/tech-lead/donor-dragonui/DRAGONUI_INTAKE.md`, `analysis/agent-fdf/R7_VERIFY.md` (fdf 13 + text 11 + ui 11 + Cyrillic 1, all rc=0) |
| — | Touch control groups + box-select | DELIVERED | `analysis/agent-touch/PARITY_TOUCH.md` (14.1 KB), `crates/wc3-input/src/lib.rs` 10-slot ControlGroups, Hotkey 0..=9 |
| — | RE map units + JASS + natives | VERIFIED | `analysis/agent-re/scan-r6.json` (`scripts.unimplemented={}`, 44/44 main), `map_units_spawn` 2 passed, `crates/wc3-re/src/creeps.rs` API |
| — | Acceptance spec models count | APPLIED | `analysis/agent-qa/acceptance/spec.json:58` `assets_models_min: 542` |

Open tails after this log: QA device cycle on the rebuilt APK, parity percentage re-measure, and the h2oc artifact-verifier base-path false-negative (`verify_artifact_path` resolves repo-relative paths against CWD).

## LAYER GATES — proof per layer (2026-11-15)

Purpose: **no layer may claim done without a command that proves it on the real
map.** This section is the per-layer acceptance ledger; it replaces "the tests
pass" with a named command, a machine-readable result and the open rows.

Run it:

```bash
bash analysis/tech-lead/run_layer_gates.sh                 # all 16 steps, PASS/FAIL table
bash analysis/tech-lead/run_layer_gates.sh game render      # aliases: game=4 tests, render=3 steps
JOBS=2 STEP_TIMEOUT=1800 bash analysis/tech-lead/run_layer_gates.sh
```

Exit codes: `0` every step PASS, `1` at least one FAIL/TIMEOUT/NO-TESTS, `2`
no failures but a SKIP (a real-map gate that did not run the real map, or a
device input not captured: grass frame / death log). Artifacts: `analysis/tech-lead/gate-logs/last-summary.txt`
and `last-results.json` (one record per step: kind, status, rc, passed, failed,
ignored, skipped_tests, elapsed).

Rules the script enforces:

* `NO-TESTS` is FAIL — a filter that matches nothing prints `0 passed` and
  exits 0; that trap is what let `playability_real_map.rs` look green.
* A real-map test that prints `skip:` is `SKIP`, never `PASS`.
* Game tests run with `SDL_AUDIODRIVER=dummy`: the SDL device is not part of a
  test, and two concurrent runs (another agent running the same test) block on
  it in `futex` with 0 CPU — observed 2026-11-15 04:10 for 20 minutes. With the
  dummy driver `melee_map_paths` went from 90 s to **10.3 s**; `--test-threads=1`
  is added only when `MemAvailable < 4 GB` (two Games ≈ 2 GB).
* `policy_ignored` fails when an acceptance test hides behind `#[ignore]` or
  when `run_layer_gates.sh` stops running `terrain_ground_detail` with
  `--ignored`.

### Gate table — final run 2026-11-15 (see `gate-logs/run_final3_2026-11-15.log`)

| Step | Layer | Criterion (done means) | Proof command | Observed 2026-11-15 | Status |
|---|---|---|---|---|---|
| sim | sim | economy, combat, AI, movement, determinism on the shipped maps | `cargo test -p wc3-sim` | PASS rc=0: 235 passed / 0 failed / 2 ignored / 0 skipped (336s) | **PASS** |
| data | data | every typed table parsed and consumed by the sim | `cargo test -p wc3-data` | PASS rc=0: 130 passed / 0 failed / 0 ignored / 0 skipped (60s) | **PASS** |
| formats | formats | 44 shipped maps parse (W3E/WPM/DOO), real doodads/trees | `cargo test -p wc3-formats` | PASS rc=0: 76 passed / 0 failed / 0 ignored / 0 skipped (20s) | **PASS** |
| audio | audio | race themes, per-def voice sets, mixer queue policy, audibility radius/cut-off (ledger 400) | `cargo test -p wc3-audio` | PASS rc=0: 57 passed / 0 failed / 1 ignored / 0 skipped (7s) | **PASS** |
| audio_earshot | audio-device | device kill log: every `GAME_AUDIO_DEATH` carries `dist=` and is inside the 4000 earshot | `python3 analysis/tech-lead/check_audio_earshot.py` | SKIP (rc=2, 0s) | **SKIP** |
| anim | anim | every roster def resolves id → model → sequences | `cargo test -p wc3-anim` | PASS rc=0: 38 passed / 0 failed / 2 ignored / 0 skipped (5s) | **PASS** |
| render_ground | render | headless GPU render of the Justice start view; writes `terrain-film-after.ppm` and prints the patch stats | `cargo test -p wc3-game --lib terrain_ground_detail -- --ignored` | PASS rc=0: 1 passed / 0 failed / 0 ignored / 0 skipped (100s) | **PASS** |
| render_sharp | render | terrain sharpness on the rendered frame (device criterion) | `python3 analysis/tech-lead/terrain_sharpness.py` | PASS (rc=0, 0s) | **PASS** |
| render_sharp_dev | render-device | same measurement on a **device** grass frame | `terrain_sharpness.py --image analysis/agent-qa/terrain/device-grass.png` | SKIP (rc=2, 0s) | **SKIP** |
| hud | hud | card follows selection: 12 slots, labels, hotkeys, touch layout | `cargo test -p wc3-game --lib hud::` | PASS rc=0: 59 passed / 0 failed / 0 ignored / 0 skipped (53s) | **PASS** |
| playable_loop | game | real match: gather → train → build → fight | `cargo test -p wc3-game --test playable_loop` | PASS rc=0: 1 passed / 0 failed / 0 ignored / 0 skipped (127s) | **PASS** |
| touch | game-touch | pointer taps select → build card → tree=lumber, mine=gold | `cargo test -p wc3-game --test touch_playability` | PASS rc=0: 2 passed / 0 failed / 0 ignored / 0 skipped (111s) | **PASS** |
| map_paths | game-map | cross-map move >800 u, ghost anchor, gold+lumber on Justice | `cargo test -p wc3-game --test melee_map_paths` | PASS rc=0: 7 passed / 0 failed / 0 ignored / 0 skipped (16s) | **PASS** |
| real_map | game-real-map | Orc01 6000 u walk (script-only campaign map, no economy); Justice harvest + building placement | `cargo test -p wc3-game --test playability_real_map` | PASS rc=0: 3 passed / 0 failed / 0 ignored / 0 skipped (15s) | **PASS** |
| economy_probe | game-touch-probe | phone pointer path: mine tap = `Harvest` (never `Attack`/`Idle`), tree tap = `HarvestLumber`, income = whole 10-per-trip loads | `bash analysis/tech-lead/run_economy_probe.sh` | PASS (rc=0, 198s) | **PASS** |
| policy | policy | every `#[ignore]` has a non-acceptance reason; acceptance-backed ignores are run by this gate | `python3 analysis/tech-lead/audit_ignored_tests.py` | PASS (rc=0, 0s) | **PASS** |

### Terrain sharpness acceptance (tech-lead thresholds, 2026-11-15)

Criterion for a grass patch on the **device** frame: **edge ≥ 0.12 and channel
std ≥ 18 on at least two channels** (Sobel magnitude > 12.0 over the patch
interior; patch 200×140 at a 2414×1080 frame, anchored 26 % x / 34 % y — the
geometry `analysis/agent-render/film_stats.py` measured with).

Reported by the tech-lead (2026-11-15, provenance: their measurement message):

| Frame | channel std | p5/p50/p95 | Sobel edge |
|---|---|---|---|
| device baseline (2414×1080 grass patch) | (5.5, 12.9, 5.5) — "soap film" | 47/58/66 | 0.051 |
| headless after LOD/specular fix | (26.9, 27.4, 20.2) | 9/48/83 | **0.381** |
| headless before | — | — | 0.098 |

Recomputed in this session with `analysis/agent-render/film_stats.py`
(observed 2026-11-15 01:47 MSK, same metric) — the numbers do **not** match the
quoted ones exactly, so both are recorded rather than pretending one set:

| Frame | channel std (centre patch) | p5/p50/p95 | edge | verdict |
|---|---|---|---|---|
| `terrain-film-before.png` | (14.9, 11.7, 11.2) | 53/63/72 | 0.445 | below the after-thresholds |
| `terrain-film-after.png` | (29.3, 25.4, 24.6) | 32/58/99 | **0.974** | PASS (`film_stats.py` rc=0) |
| `patch-before.png` | (38.5, 25.7, 25.9) | 59/67/150 | 0.381 | matches the quoted *edge* only |
| `patch-after.png` | (58.8, 43.0, 44.1) | 42/71/184 | 0.983 | PASS |

`terrain_ground_detail_justice` itself (2026-11-15 01:58, rc=0, 177 s) prints the
same measurement it wrote to the PPM:
`TERRAIN_GROUND tag=after std=(29.3,25.4,24.6) p5=32 p50=58 p95=99 edge=0.974
blue_excess=-5.26` — the render and the metric agree end-to-end, so the sharpness
PASS is reproducible from the artifact, not inherited from a claim.

**Device frame is not captured yet** (`analysis/agent-qa/terrain/device-grass.png`);
the `render_sharp_dev` step is SKIP until the tech-lead runs the device slot.
A headless PASS is not a device PASS: the operator's complaint was on the phone.

### Device economy touch parity — the actual numbers

`gather_probe` (2026-11-15 01:57, Justice + real MPQ), after the sim/HUD fixes:

```
order 3 ticks after mine tap: Some((Harvest { mine: Vec2(14592.0, 1280.0), phase: ToMine }, ...))
order 3 ticks after tree tap: Some((HarvestLumber { tree: Vec2(15040.0, 1408.0), phase: ToMine }, ...))
gold 1000 -> 1040 (4 trips)      lumber 1000 -> 1080 (8 trips)
PROBE PASS
```

Before the fix the same probe printed `Some(Attack(UnitId(15)))` for the mine
(a creep guarding the mine stole the tap) and `Idle` for the tree, with gold
rising only by the 60-per-30 s passive drip — a working economy system that the
phone could not reach.

`touch_playability` (2026-11-15 01:47, rc=0, 207 s, real Justice + MPQ):

```
PARITY economy.lumber touch_tap: lumber 1000->1010 picked_at=100 deposited_at=152 tick=152 order=HarvestLumber tree=(14976,2048) phase=ToTarget
PARITY economy.gold   touch_tap: gold   1000->1010 picked_at=154 deposited_at=235 tick=235 order=Harvest GOLD mine=(14592,1280) phase=ToTarget
```

The device scenario log (`analysis/agent-qa/operator-bugs/scenarios.json`) has
`"harvest": {"tap": null, "lumber_before": null, "lumber_after": null}` — the
device harvest path was **never exercised** on the 2026-11-14 build, which is
why a unit-test-only scorecard could call the economy green. The next device
cycle must record `tap`, `lumber_before/after` and `gold_before/after`.

### Layer closures folded in (tech-lead updates, 2026-11-15)

**SIM (ledger 393, closed).** Fake income removed — `passive_income: false` at
`crates/wc3-sim/src/lib.rs:845`; gather gates from blocked footprints
(`blocked_reach` / `gather_gate`, `lib.rs:2146/2169`); no silent order drops
(`gather_path_failed`, `lib.rs:1963`); tree retarget within 600 u; WC3 smart
order (Move onto a mine/tree becomes a harvest, `lib.rs:1148`); AI economy
(`ai.rs:262`). Measured: `cargo test -p wc3-sim` **235 passed / 0 failed /
2 ignored rc=0** (the gate measured the same 235 before the ledger closed); the
touch path deposits for real (`touch_playability`: mine +10 at tick 232, tree
+10 at tick ~149) and 5 workers produce +320 gold / +420 lumber per 60 s.

**AUDIO (ledger 400, closed).** `reference_distance` from `MinDistance`
(`soundinfo.rs:79 playback_distances`), hard `DistanceCutoff` cull
(`game_audio.rs:363-385`), zero attenuation past the far end (`engine.rs:952`),
audibility gate `set_audible_radius` / `is_within_earshot` / `gate_voice`
(`game_audio.rs:1003-1039`, default 4000), region-scoped ambient
(`game_audio.rs:906`). Measured: 39 lib + 17 integration + 1 no_alloc rc=0;
12000-unit energy 0.0. Gate: the `audio` step plus `audio_earshot`, which today
**SKIPs** because `device_audio.log` has no `GAME_AUDIO_DEATH` lines and
`game.rs:1440` does not log a distance — the next device log must carry
`dist=<units to camera>` for the check to become a real PASS.

**Campaign maps are not melee maps.** `Orc01.w3m` has `"gold_mines":0`
(`analysis/agent-re/phase0/mapcontent.json`) and starts units on unwalkable
cells (3); its content is script-created. `playability_real_map` therefore uses
it **only** for the 6000 u terrain/path walk and runs the harvest/build checks on
Justice. A campaign map must never be used as a melee economy fixture.

### Mechanic targets — per-target rc (2026-11-15 04:42)

`analysis/tech-lead/mechanic_targets.tsv` already carries the sim economy and
AI targets the sim owner asked for; measured after the tree settled:

| Mechanic | Target | Command | rc | passed | failed |
|---|---|---|---:|---:|---:|
| `economy.gold` | `economy` | `cargo test -p wc3-sim --offline --test economy` | **0** | 18 | 0 |
| `economy.lumber` | `economy` | same | **0** | 18 | 0 |
| `melee.matrix` | `melee_matrix` | `cargo test -p wc3-sim --offline --test melee_matrix` | **0** | 1 | 0 |
| `ai.skirmish` (owns the AI-economy test) | `ai` | `cargo test -p wc3-sim --offline --test ai` | **0** | 7 | 0 |

Global consistency: `bash analysis/tech-lead/run_mechanic_tests.sh --check` →
`rows=95 ported=79 failing=[] no_target=[]` `MECHANIC_TESTS_OK` rc=0.

Two target-map facts, with evidence:

* `melee.matrix` stays on `--test melee_matrix`: the test
  `melee_matrix_4_races_sizes_1_4` is defined in
  `crates/wc3-sim/tests/melee_matrix.rs:31`, not in `roster.rs`. Repointing it to
  `--test roster` would run roster tests and silently drop the matrix assertion
  — a fake green, so it was not done.
* `ai.economy` is **not a gap-map mechanic id**. The AI economy test is
  `ai_economy_lumber_and_mine_saturation` (`crates/wc3-sim/tests/ai.rs:196`) and
  is covered by the existing `ai.skirmish → --test ai` target. A distinct
  `ai.economy` row needs an annotation in the gap-map generator; inventing a
  target id without a row would fail `no_row` in `accept_full_port.sh`.

Transient red observed while measuring (recorded, not hidden): at 04:24 and
04:26 `economy_worker_delivers_gold_and_lumber` failed with
`seat 0: gold only comes from deliveries — left 76, right 70` while the sim
owner was mid-edit (`lib.rs` mtime 04:34, `data.rs` 04:41). The re-run at 04:42
after the edit settled is 18/0 rc=0. A one-off failure on a live tree is
re-checked after the owner's mtime stops moving, never reported as a stable
regression — and never silently dropped.

### Device rows behind the P0 verdicts (OPERATOR_BUGS.json v2)

`OPERATOR_BUGS.json` now uses `verified | open | blocked` with an owner and the
exact missing artifact per row: **8 verified, 10 open, 0 blocked**. The
acceptance script gates P0-10/11/12 on these rows, so a device regression fails
the P0 row instead of hiding behind a unit test:

| P0 | Device rows | Open today | Effect |
|---|---|---|---|
| P0-10 UI/input | 5,7,8,9,18 | #7 army icons, #8 hero abilities, #9 labels, #18 truncation | P0-10 FAIL |
| P0-11 audio voices | 4,13 | #4, #13 per-def voice capture | P0-11 FAIL |
| P0-12 render | 10,11,12,14,15,16,17 | #10 ghost invisible, #14 water band | P0-12 FAIL |

Device evidence spans two builds: screens/logcat `d3efe82e…`, audio run
`72627294…`, current APK `2e070490…` (rebuilt 01:52 during this session) — recorded in
`device_evidence`; a row that needs both artifacts is verified only after one
build carries both.

### Honesty fixes shipped with this section

1. `playability_real_map.rs` looked for `Maps/Orc01.w3m`; the extracted tree has
   `Maps/Campaign/Orc01.w3m`. All three tests had been returning `skip` with
   rc=0 since they were written. Fixed (both paths probed; a missing map inside
   an existing tree is now a hard failure). Its harvest/build checks moved to
   Justice because **Orc01 genuinely has 0 gold mines**
   (`analysis/agent-re/phase0/mapcontent.json`: `"gold_mines":0`), so the old
   "no gold mine near the base" expectation could never hold there.
2. The building test now searches for a site with `can_place` (the first probe
   landed on an occupied footprint: `Err("site occupied")`); `walkable_pos_near`
   alone never proved a building could be placed.
3. `melee_map_paths` now asserts the worker's spawn cell is walkable (operator
   bug #3) instead of printing it.
4. `wc3-data` doctest failure (`coverage.rs:16`, `Field` not imported) was caught
   by the gate at 01:22 and fixed by the data owner at 01:53 (`--doc` rc=0).
5. A transient red tree during the 01:19 run (`wc3-sim/src/lib.rs` calling a
   not-yet-defined `gather_path_failed`) was reported with the exact compile
   error; the sim owner's fix landed and the following run compiled.
6. `touch_playability` failure messages name the tap log, the issued order, the
   phase, path length, carry, timer and every `Event::Error`; the two failure
   classes observed while building it were (a) camera not centred → off-viewport
   tap, (b) card expected hard-coded Orc builds although the w3i seats the local
   player in either slot. Both assumptions are gone. Note: `Game::pending` is
   private, so the "pending command" is named through the `TAP` log and the
   post-tap `Order`, not by printing the queue.
7. `SDL_AUDIODRIVER=dummy` for game steps: a 20-minute futex hang with 0 CPU was
   another agent's identical test holding the SDL audio device. The gate now
   runs game tests with the dummy driver (10× faster) and serialises only under
   4 GB free RAM.
8. `#[ignore]` audit: 35 ignored tests, every one with a non-acceptance reason
   (one missing reason fixed in `tests/asset_gallery.rs`); the single
   acceptance-backed ignore (`terrain_ground_detail_justice`) is run with
   `--ignored` by the gate; the audit fails if that step disappears.

### First failing layer (rule)

If the gate fails, it prints `FIRST FAILING LAYER: <layer> (<step>, <kind>)
status=<status> rc=<rc>` plus the last 40 log lines, and stops the table work
there. That is the exact text to hand to the owner of the layer: it names the
step, the rc and the failing assertion, so the owner gets a reproducible command
instead of a symptom.
