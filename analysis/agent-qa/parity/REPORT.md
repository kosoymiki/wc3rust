# PARITY QA — feature gates (headless)

Generated: **2026-11-13T19:28:20Z**
Overall: **PASS** — fail=0 gap=0 pass=32 skip=1

Matrix: `analysis/tech-lead/PARITY_MATRIX.md` present=**True** — every gate maps to a PARITY_MATRIX row

| Gate | Area | Status | Pass | Fail | Gap | Total | Detail |
|---|---|---|---:|---:|---:|---:|---|
| `train.all_defs` | train | **PASS** | 53 | 0 | 0 | 53 | 53/53 trainable roster defs trained through the data path |
| `train.menu_linkage` | train | **PASS** | 121 | 0 | 0 | 121 | 121/121 command-card train links are bidirectional and resolvable |
| `build.all_defs` | build | **PASS** | 50 | 0 | 0 | 50 | 50/50 building defs placed, charged and completed |
| `tech.requires` | tech | **PASS** | 2 | 0 | 0 | 2 | 2/2 tech-tree gates enforced |
| `cast.spells` | cast | **PASS** | 11 | 0 | 0 | 11 | 11/11 implemented spells cast with exact mana and an observed effect |
| `cast.coverage` | cast | **PASS** | 154 | 0 | 0 | 154 | 154/154 roster ability ids data-backed (115 active castable, 245 passive/aura) |
| `hero.stats` | hero | **PASS** | 12 | 0 | 0 | 12 | 12/12 Hero armor defs spawn with roster hp/mana/damage/cost |
| `hero.coverage` | hero | **PASS** | 12 | 0 | 0 | 12 | 12/12 melee heroes present (missing=0) |
| `hero.leveling` | hero | **PASS** | 12 | 0 | 0 | 12 | 12/12 heroes level through XP with attribute growth |
| `roster.coverage` | roster | **PASS** | 4 | 0 | 0 | 4 | 4/4 matrix-listed melee IDs present in ROSTER |
| `items.data` | items | **PASS** | 273 | 0 | 0 | 273 | 273/273 TFT item rows carry id/level/cost/usable |
| `items.schema` | items | **PASS** | 2 | 0 | 0 | 2 | 2/2 item trees expose the class column |
| `items.runtime` | items | **PASS** | 7 | 0 | 0 | 7 | 7/7 item runtime checks passed (273 defs) |
| `items.gaps` | items | **PASS** | 4 | 0 | 0 | 4 | 4/4 item gameplay checks passed |
| `economy.lumber` | economy | **PASS** | 3 | 0 | 0 | 3 | 3/3 lumber economy checks passed |
| `creeps.camps` | creeps | **PASS** | 3 | 0 | 0 | 3 | 3/3 creep camp checks passed |
| `map.units.spawn` | maps | **PASS** | 2 | 0 | 0 | 2 | 2/2 map-unit checks passed |
| `triggers.jass` | triggers | **PASS** | 45 | 0 | 0 | 45 | 45/45 JASS scripts parsed (1 synthetic trigger + 44 retail maps) |
| `triggers.return` | triggers | **PASS** | 3 | 0 | 0 | 3 | bare return statements parse (no value consumed from the next line) |
| `triggers.elseif` | triggers | **PASS** | 1 | 0 | 0 | 1 | elseif chains keep the first true branch |
| `audio.game` | audio | **PASS** | 6 | 0 | 0 | 6 | 6/6 audio checks passed (12 tables, 2316 defs, 463 unit names) |
| `menu.fdf` | menu | **PASS** | 15 | 0 | 0 | 15 | 15/15 menu checks passed (10 core FDF, 71 FDF total, 4 build menus) |
| `assets.tree` | assets | **PASS** | 1 | 0 | 0 | 1 | required parity asset roots present |
| `maps.headless` | maps | **PASS** | 44 | 0 | 0 | 44 | 44/44 maps clean; core parse 44/44; war3mapUnits.doo errors 0 |
| `maps.device` | maps | **SKIP** | 0 | 0 | 0 | 0 | device locked (deviceLocked=1); current APK 70c15e91e4f5 not exercised. Last valid cycle APK 4c8b86177a99 (previous round) |
| `box.boundary` | input | **PASS** | 1 | 0 | 0 | 1 | box classification=box_select_observed selected 1->0; gesture rendered, injected_motionevent_reaches_input=True; evidence APK 4c8b86177a99; harness boundary per packet (not product FAIL) |
| `box.getevent` | input | **PASS** | 1 | 0 | 0 | 1 | getevent_ok=True injected_seen=True lines=35; injected input motionevent reaches /dev/input/event5 touchpanel |
| `models.per_def` | models | **PASS** | 216 | 0 | 0 | 216 | 216 roster definitions map to 216 distinct MDX; 214 loaded + 2 meshless, 0 paths missing on disk |
| `models.budget` | models | **PASS** | 2 | 0 | 0 | 2 | full-set peak 73.4 MB <= 1536 MB (pass=true); LRU 32 MB budget loads=214 evictions=98 peak=32.0 MB |
| `jass.engine` | triggers | **PASS** | 44 | 0 | 0 | 44 | 44/44 map scripts run main() in the RE JASS VM; 92 natives unimplemented (3073 call sites) |
| `jass.frame_loop` | triggers | **PASS** | 1 | 0 | 0 | 1 | game.rs constructs JassBridge |
| `hud.resources` | ui | **PASS** | 1 | 0 | 0 | 1 | HUD resource bar renders gold/lumber/supply pills from sim state (GOLD=4, LUM=1, SUP=1) |
| `audio.wired` | audio | **PASS** | 1 | 0 | 0 | 1 | game audio wired (dep=True, refs=11) |

## Failures and gaps

## Evidence

- `tech.requires`: `analysis/tech-lead/PARITY_MATRIX.md G5: placement does not read Requires (PARTIAL)`, `crates/wc3-sim/src/lib.rs place_building_impl / enqueue_production`
- `cast.coverage`: `crates/wc3-sim/src/abilities_gen.rs from analysis/war3x_data/Units/AbilityData.slk`, `crates/wc3-sim/src/abilities.rs Sim::cast_data_ability`
- `hero.coverage`: `analysis/tech-lead/PARITY_MATRIX.md §2.2: 12 melee heroes, Emoo missing`
- `roster.coverage`: `analysis/tech-lead/PARITY_MATRIX.md §2.2 exact missing IDs: Emoo hmtt ugol uzg1`
- `items.data`: `analysis/war3x_data/Units/ItemData.slk (TFT, primary)`, `analysis/war3_data/Units/ItemData.slk (RoC)`
- `items.schema`: `crates/wc3-data/src/items.rs:99-101: class() reads the RoC column name `itemClass``, `analysis/war3x_data/Units/ItemData.slk: TFT column is `class``
- `items.runtime`: `crates/wc3-sim/src/lib.rs:1814 spawn_item / :1835 pickup_item / :1869 drop_item / :1899 use_item`, `crates/wc3-sim/src/items_gen.rs (273 generated defs)`
- `items.gaps`: `analysis/war3x_data/Units/ItemFunc.txt Requires/DependencyOr (18 gated items, 0 item->item)`, `crates/wc3-sim/src/abilities_gen.rs + Sim::cast_data_ability`, `crates/wc3-sim/src/lib.rs buy_item/use_item_on/combine_items`
- `economy.lumber`: `crates/wc3-sim/src/lib.rs HarvestLumber / LumberDeposited / deposit_lumber`, `analysis/tech-lead/PARITY_MATRIX.md G2 (lumber economy)`
- `creeps.camps`: `crates/wc3-sim/src/lib.rs spawn_camp / Camp / advance_creeps`, `analysis/tech-lead/PARITY_MATRIX.md G18 (creep camps)`
- `map.units.spawn`: `crates/wc3-sim/src/lib.rs spawn_map_units; crates/wc3-game/src/model_render.rs:563 consumes bundle.units`, `analysis/tech-lead/PARITY_MATRIX.md G22 (map-placed units)`
- `triggers.jass`: `crates/wc3-jass/src/lib.rs:448-460 (bare return consumes the next line)`, `runtime engine: analysis/agent-re/scan.json map_main_ok 44/44 (wc3-re::jass; separate component)`
- `triggers.return`: `crates/wc3-jass/src/lib.rs:448-460: skip_newlines() precedes the Newline check`, `wc3-jass is the standalone lexer/parser/interpreter; the game engine uses wc3-re::jass`
- `triggers.elseif`: `crates/wc3-jass/src/lib.rs:431: else_branch is overwritten per elseif`, `wc3-jass is the standalone lexer/parser/interpreter; the game engine uses wc3-re::jass`
- `audio.game`: `analysis/war3_full3/UI/SoundInfo/*.slk`, `analysis/war3_full3/Sound/**.wav (real archive files)`
- `maps.headless`: `analysis/agent-qa/r2/report.json`, `analysis/agent-qa/r2/maps/*.log`
- `maps.device`: `analysis/agent-qa/parity/device-cycle/device-blocked.json`, `analysis/agent-qa/parity/device-cycle/STATUS-ROUND7.md`, `analysis/agent-qa/parity/device-cycle/cycle-blocked-shade.json`
- `box.boundary`: `analysis/agent-qa/parity/device-cycle/cycle.json`, `analysis/agent-qa/parity/device-cycle/screens/box-before.png`, `analysis/agent-qa/parity/device-cycle/screens/box-after.png`, `analysis/agent-qa/parity/device-cycle/getevent.log`, `analysis/agent-qa/final/focus-gates.json`
- `box.getevent`: `analysis/agent-qa/parity/device-cycle/getevent.log`
- `models.per_def`: `analysis/agent-world/parity/roster.csv`, `crates/wc3-game/src/model_render.rs:477-560`, `analysis/agent-qa/parity/models-per-def.log`
- `models.budget`: `analysis/agent-qa/parity/models-per-def.log`, `crates/wc3-game/src/model_render.rs:519-560`
- `jass.engine`: `analysis/agent-re/scan.json`
- `jass.frame_loop`: `crates/wc3-game/src/sim_bridge.rs`, `crates/wc3-game/src/game.rs`
- `hud.resources`: `crates/wc3-game/src/game.rs:2238-2250`, `analysis/agent-qa/parity/device-cycle/screens/start.png`
- `audio.wired`: `crates/wc3-game/Cargo.toml`, `crates/wc3-game/src/*.rs`

## Artifacts

- `analysis/agent-qa/parity/report.json`
- `analysis/agent-qa/parity/REPORT.md`
- `analysis/agent-qa/parity/report-raw.json` (Rust probe output)
- `scripts/qa/parity/` (standalone probe crate, QA owner)

