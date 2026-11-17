# DATA LAYER — FINAL PRODUCT REPORT (r2-data, ledger 354)

Date: 2026-11-13 · Layer: `wc3-data` (+ `wc3-formats::gamedata`) · Owner: agent-data
Canonical gates: `cargo check -p wc3-data --lib`, `cargo test -p wc3-data --lib`.

## 1. What this layer delivers

One typed, data-first view of the operator's own WC3 installs (`analysis/war3_data`,
`analysis/war3x_data`), consumed by the game, sim, HUD and QA tools. No second
parser, no invented values, no stubs.

| Concern | API | Source |
|---|---|---|
| Unified object lookup | `GameDatabase::object(id)` (`ObjectInfo`: name/tip/ubertip/icon/hotkey/model/stats) | SLK + profiles + `.wts` + object-data overlay |
| Menus / tech | `menu.rs` (`MenuEntry`, `train_menu`/`build_menu`, `Requires`) | `UnitBalance.Trains/Builds/Requires`, profiles |
| Hero attributes & leveling | `hero.rs` (`HeroDef`, `HeroAttributes`, `hp_at/mana_at/armor_at/damage_bonus_at/skill_points_at`, `HeroRules`, `revive_cost`) | `UnitBalance` (STR/INT/AGI/plus/Primary, `realHP`/`realM`), `Units\MiscData.txt` |
| Hero abilities (#11) | `ability_def.rs` (`HeroAbilityInfo`, `hero_abilities`, `hero_ability_coverage`) | `UnitAbilities.heroAbilList` × `AbilityData` |
| Auras / buffs (#9) | `ability_def.rs` (`AuraNumbers`), `buffs.rs` (`BuffStore`) | `AbilityData`; buff base sheet absent in retail → overlay-only |
| Spell numbers (#10) | `ability_def.rs` (`AbilityNumbers`, `SpellLevel`), `melee_ability_coverage` | `AbilityData` levels 1–N |
| Items (#14) | `items.rs` (`ItemView`: class/price/charges/stock/abilities), `shop.rs` (`ShopDef`, `ShopItem`, `Requirement`, `shops`, `item_requirements`, `item_ability_coverage`) | `ItemData`, unit profile `Sellitems`, `ItemFunc.txt` `Requires`/`DependencyOr` |
| Localization | `localization.rs` (`Localizer`: profiles, `.wts`, overlays, per-level tooltips) | `*Strings.txt`/`*Func.txt`/`war3map.wts` |
| Validation | `validate.rs` (`Report`, severity, artifact classification) | all of the above |

## 2. Tooling (`dump_gamedata`)

`cargo build -p wc3-data --bin dump_gamedata` → `target/debug/dump_gamedata`.

```
dump_gamedata [--root DIR] [--tft DIR] [--listfile FILE] [--wts FILE]
              [--object-data FILE] [--json] <COMMAND> [ID]
```

Commands: `counts`, `validate`, `paths`, `object|unit|ability|item|upgrade|destructable|doodad ID`,
`objectdata`, **`shops`**, **`shop ID`**, **`hero-abilities`**, **`hero-ability ID`**, **`coverage`**
(`--json` supported by `validate|shops|shop|hero-abilities|hero-ability|coverage`).

Measured smoke run (rc=0), RoC + TFT:

```
$ dump_gamedata --root analysis/war3_data --tft analysis/war3x_data coverage
root: dir:analysis/war3_data
melee abilities: 175/176 (99.4%)
spell sheet: 380/380 (100.0%)
ladder heroes: 12/12 (100.0%)
castable abilities: 251/252 (99.6%)
item abilities: 102/106 (96.2%)
items with class: 132/132 (100.0%)
tft root: dir:analysis/war3x_data
tft melee abilities: 301/301 (100.0%)
tft spell sheet: 733/733 (100.0%)
tft ladder heroes: 12/12 (100.0%)
tft castable abilities: 442/442 (100.0%)
tft item abilities: 223/223 (100.0%)
tft items with class: 273/273 (100.0%)

$ dump_gamedata --root analysis/war3_data shop ngme     # 8 items, phea 150g/1 charge/stock 440/3/120
$ dump_gamedata --root analysis/war3_data hero-ability AHhb
  AHhb name=Благодать levels=3 req=1 owners=Hpal,… level1 mana=75 cd=5 range=800 data=[200,…]
```

## 3. Gate ledger (data layer)

| Gate | Value | Status |
|---|---|---|
| `cargo check -p wc3-data --lib` | rc=0 | PASS |
| `cargo test -p wc3-data --lib` | 42 passed, 0 failed | PASS |
| `qa.abilities.coverage` (melee) | RoC 175/176 = 99.4 % (gate ≥60 %) | PASS |
| `qa.abilities.heroes` (#11) | RoC 56/56 = 100 %; 12 ladder heroes × 4 = 48 slots | PASS |
| `qa.buffs` (#9) | 9 RoC auras; buff base sheet absent (overlay-only, listfile-proven) | PASS |
| `qa.items.shop` (#14) | `ngme` 8 items RoC / 11 TFT; TFT 18 gated items via `Requires` | PASS |
| `qa.items.abilities` | RoC 102/106 = 96.2 %, TFT 223/223 = 100 % (≥60 %) | PASS |
| `hero.coverage` (data) | 12/12 both roots (`melee_hero_coverage`) | PASS (data) |
| `cast.coverage` (data) | RoC 251/252 = 99.6 %, TFT 441/442 = 99.8 % (`castable_ability_coverage`) | PASS (data) |
| `items.schema` | RoC 132/132, TFT 273/273 class exposed | PASS (2/2 trees) |
| Full crate suite | 96 passed (42 lib + 2 bin…, now 4 bin) | PASS |

Residuals outside this layer (exact evidence in `merge-data.md` §14.9):
- QA `hero.coverage` counts `wc3_sim::data::ROSTER`; no `Emoo` UnitDef row there
  (`crates/wc3-sim/src/roster_gen.rs:4654` references it in `eaten.trains`).
- QA `cast.coverage` counts `wc3_sim::abilities::Spell::ALL` (11 hand-written variants);
  a generic data-driven cast path is a `wc3-sim` change.

## 4. Evidence paths

- `analysis/agent-data/coverage-report.json` — machine-readable gate counts (round 8 + P0).
- `analysis/agent-data/evidence-top8.txt`, `evidence-round6.txt`, `evidence-round7.txt`,
  `evidence-round8.txt`, `evidence-p0-gap-close.txt`.
- `analysis/tech-lead/merge-data.md` — authoritative manifest (§1–§14.10).
- Tests: `crates/wc3-data/tests/{base_contract,parity,parity_top,real_data,hero_items}.rs`,
  lib tests incl. `gap_close_tests`, bin tests.
- Logs: `/data/data/com.termux/files/usr/tmp/r8-final-{check,lib}.log`, `r8-p0-*`,
  `r8-tool-tests.log`.

## 5. Balance audit + SQLite export (`dump_gamedata balance|sqlite`)

`balance` walks every row of the four balance families and validates the numbers
through the typed accessors (new `crates/wc3-data/src/balance.rs`):

| Root | units | abilities | items | upgrades | rows | checked | errors | warnings |
|---|---|---|---|---|---|---|---|---|
| RoC  | 466 | 397 | 132 | 58 | 1053 | 1053 | 0 | 0 |
| TFT  | 812 | 763 | 273 | 89 | 1937 | 1937 | 0 | 6 (`Area1=-1` sentinels) |

`sqlite PATH` writes `units/abilities/ability_levels/items/upgrades/shops` from
the same typed database via the `sqlite3` CLI and verifies the row counts
(RoC: 466/397/1191/132/58/8, rc=0). Toolchain audit (versions, gaps, installed
engines): `analysis/agent-data/TOOLCHAIN-AUDIT.md`.

## 6. Full-port typed tables (ledger 375, 2026-11-14)

### 6.1 API — `crates/wc3-data/src/defs.rs`

`GameDatabase::defs()` converts every row of the definition tables into typed
structs and reports any unreadable row with its exact accessor error:

`Defs { units: Vec<UnitDef>, weapons: Vec<WeaponDef>, items: Vec<ItemDef>,
upgrades: Vec<UpgradeDef>, effects: Vec<UpgradeEffectDef>, buffs: Vec<BuffDef>,
unparsed: Vec<UnparsedRow> }`

| Struct | Fields (source) |
|---|---|
| `UnitDef` | id/name/race/class, level/hp/mana/armour/speed/food/gold/lumber/build time, `dropItems`, `abilList`, `heroAbilList`, `upgrades` (`UnitBalance.upgrades`) |
| `WeaponDef` | per `UnitWeapons` row: slots 1/2 (`weapTp`, `atkType`, min/max damage, attacks/s, range, projectile art/speed) |
| `ItemDef` | class, `Level` (drop level), gold/lumber, `uses` charges, `cooldownID`, `ignoreCD`, `droppable`, `drop`, `pickRandom`, `powerup`, `sellable`, stock max/regen/start, `abilList` |
| `UpgradeDef` | class/race, `maxlevel`, gold/lumber/time base+per-level, four effect slots (`effectN`/`baseN`/`modN`), `Requires`/`Requirescount` (profile), owning units |
| `UpgradeEffectDef` | `UpgradeEffectMetaData.slk`: id, displayName, dataType, type, min/max |
| `BuffDef` | `*AbilityFunc.txt` sections with `Buffart` (148): id, art, tip |

### 6.2 Counts — real TFT extraction (`analysis/war3x_data`)

| Table | Rows | Unparsed |
|---|---|---|
| units (`UnitData`) | **812** | 0 |
| weapons (`UnitWeapons`) | **812** | 0 |
| items (`ItemData`) | **273** | 0 |
| upgrades (`UpgradeData`) | **89** | 0 |
| upgrade effects (`UpgradeEffectMetaData`) | 52 | 0 |
| buffs (`*AbilityFunc.txt` Buffart sections) | 148 | 0 |

Field coverage: items — class 273/273, level 273/273, `abilList` 240/273,
droppable 272/273; upgrades — levels+cost 89/89, effect slots 52/89,
profile `Requires` 40+.

`dump_gamedata defs` prints the same counts (`crates/wc3-data/src/bin/dump_gamedata.rs:225`).

### 6.3 Callers (file:line)

| Consumer | Call site |
|---|---|
| game object database (names/icons/stats/costs) | `crates/wc3-game/src/assets.rs:24,223,482,493,886,917` (`GameDatabase`, `unit_stats`, `unit_cost`) |
| HUD command-card costs | `crates/wc3-game/src/hud.rs:884,891,910` |
| data tooling | `crates/wc3-data/src/bin/dump_gamedata.rs:205-243` (`shops/shop/hero-abilities/coverage/defs/balance/sqlite`) |
| creep drops (pending apply) | `analysis/agent-data/patches/items-drops-sim.patch` → `crates/wc3-sim/src/lib.rs` `creep_drop` called from `apply_damage` (:2317) and `kill_unit` (:4637) |

### 6.4 DATA_ONLY (exact reasons)

| Item | Why data-only |
|---|---|
| Drop **chance** table | No retail file carries percentages: `ItemData` has only `droppable`+`Level`, no drop-table file in either listfile, `MiscData` has only `DropItemRange`, profile texts have no drop key. WC3 resolves chances in the engine; `wc3_data::drops` exposes the pool (121 droppable items with levels 1–8) and the engine level mapping, and `drops_data_only_reason()` reports the boundary. The sim caller patch supplies its own chance constant. |
| Buff **base sheet** (`AbilityBuffData.slk` / `AbilityBuffMetaData.slk`) | Absent from both installs (listfile-proven); buffs are exposed from `*AbilityFunc.txt` `Buffart` sections (148) and `BuffStore` stays overlay-only (`war3map.w3b`). |
| Item **recipes** | No recipe column/class/metadata and no `Requires` token naming an item in either install; combining is map-script logic (`tests/hero_items.rs::retail_data_has_no_recipe_table`). |
| Item **cooldown groups** | `cooldownID` is a group id (e.g. `stwp` → `AItp`); per-ability cooldown numbers are data (`AbilityData.Cool`), the group resolution itself is engine behavior. |

### 6.5 Mechanic test mapping (TEST_POLICY)

| Mechanic id | Test | Command | rc |
|---|---|---|---|
| `items.data` | `items_data_rows` | `cargo test -p wc3-data --test data_tables items_data_rows` | 0 |
| `upgrades.data` | `upgrades_data_rows` | `cargo test -p wc3-data --test data_tables upgrades_data_rows` | 0 |
| `items.drops` | `items_drops` | `cargo test -p wc3-data --test data_tables items_drops` | 0 |
| `items.shops` | `items_shops` | `cargo test -p wc3-data --test data_tables items_shops` | 0 |

Acceptance: `cargo test -p wc3-data` rc=0, 105 passed
(45 lib + 4 bin + 8 base_contract + 4 data_tables + 6 hero_items + 7 parity
+ 10 parity_top + 20 real_data + 1 doctest), log
`/data/data/com.termux/files/usr/tmp/r375-full.log`.

## 7. Full end-state: items usable/classified, drops applied, shops, upgrades, buffs (ledger 375)

### 7.1 Item usability — all 273 classified (`items.usable`)

`GameDatabase::{item_usage, item_usages, item_usage_counts}` classify each
`ItemData` row from its own columns (`usable` + `abilList`):

| Class | Count | Rule | Runner input |
|---|---|---|---|
| **Usable** | **133** | `usable = 1` and ≥1 ability | every ability resolves to `AbilityNumbers` (mana/cd/area/range/duration/data) |
| **PassiveStat** | **107** | `usable = 0`, ≥1 ability | aura/stat item abilities (`AbilityNumbers` passive path) |
| **DataOnly** | **33** | no ability, `usable = 0` | campaign/quest rows; reason string attached |

273/273 classified, zero unclassified. Samples: `phea` Usable, `ratc`/`ofro`
PassiveStat, `azhr` DataOnly (`ITEM_DATA_ONLY_REASON`). Recipes remain absent
from retail data (no table/class), so no recipe rows exist to classify —
documented, not invented.

Interface note for the RE ledger 373 (file:line only, no edits): the generic
runner consumes `wc3_data::AbilityNumbers` via
`crates/wc3-sim/src/abilities.rs` (`SpellNumbers`/`install_spell_numbers`) and
`crates/wc3-sim/src/lib.rs` cast path; item effects enter through
`ItemDef.ability_id` / `data::ITEMS`.

### 7.2 Drops applied — verified caller + distribution (`items.drops`)

Sim caller merged by tech-lead (verified by reading `crates/wc3-sim/src/lib.rs`):
`creep_drop` at **:2701**, called from `apply_damage` **:2366** and `kill_unit`
**:4783**, using `wc3_data::drops::drop_level_for_creep_level` (:2706) and the
25 % engine constant (:2709).

Drop-level distribution of the 121 droppable items (`droppable = 1`, `Level > 0`):

| Item level | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
|---|---|---|---|---|---|---|---|---|
| Items | 23 | 25 | 18 | 20 | 14 | 14 | 4 | 3 |

Exposed as `GameDatabase::drop_level_distribution()` and pinned by `items_drops`.

### 7.3 Shops — stock/restock/`Sellunits` (`items.shops`)

Typed: `ShopDef.items[].{stock_max, stock_regen (interval), stock_start (initial),
gold, charges, requires}`, `ShopDef.sells_units`, `sold_units(id)`,
`shops_selling_unit(id)`. Measured: `ngme` 11 items (stwp stock 2 / regen 120 /
start 440), `hshy` → `hbot`.

Sim consumption patch: `analysis/agent-data/patches/items-shops-sim.patch`
(86 lines) — adds `data::SHOP_UNITS` (28 `Sellunits=` shop tables generated
from the profiles), `data::sold_units`, and `Sim::{shop_units, buy_unit}`
(gold debit + spawn at the shop). Item buy/restock path already exists
(`register_shop`/`buy_item` in `crates/wc3-sim/src/lib.rs:2946`/`:2975`).

### 7.4 Upgrades — 89/89 with an effect (`upgrades.data`)

| Group | Count | Source |
|---|---|---|
| Stat effect slots | **52** | `UpgradeData.effect1..4` + `baseN`/`modN` (now `f32`: `Rhac` 0.2/level) |
| Ability-unlock upgrades | **37** (of 54 referencing `Requires`) | abilities whose profile `Requires` names the upgrade (`Rhde` → `Adef`, `Rhse` → `AHta/Adts`) |
| Rows with an effect or unlock | **89/89** | assertion in `upgrades_data_rows` |

### 7.5 Buffs — 148 defs mapped (`combat.buffs`)

| Link | Count | Rule |
|---|---|---|
| by id tail | **115** | `B`+tail matches an `AbilityData` alias (`BEah` → `AEah`) |
| unique shared icon | **10** | `Buffart` equals exactly one ability `Art` (`Bdet` → `AItb`) |
| data-only | **23** | no ability row shares the tail or a unique icon (unused/removed abilities) |
| total | **148** | art present 147 (`Barm` ships an empty `Buffart`) |

### 7.6 Mechanic tests (TEST_POLICY)

| Mechanic id | Test | rc |
|---|---|---|
| `items.usable` | `items_usable` | 0 |
| `items.data` | `items_data_rows` | 0 |
| `upgrades.data` | `upgrades_data_rows` | 0 |
| `items.drops` | `items_drops` | 0 |
| `items.shops` | `items_shops` | 0 |
| `combat.buffs` | `combat_buffs_data` | 0 |

Acceptance: `cargo test -p wc3-data` rc=0 — **107 passed** (45 lib + 4 bin +
8 base_contract + 6 data_tables + 6 hero_items + 7 parity + 10 parity_top +
20 real_data + 1 doctest), log `/data/data/com.termux/files/usr/tmp/r375c-full.log`.

Patch precondition: `buy_unit` charges only when `data::def_of(unit_id)` exists;
sold units not yet present in the sim tables (`hbot`, `ubot`, …) return
"no unit def" without debiting gold, so the sim data owner adds those rows when
applying the patch.

### 7.7 Evidence files

`analysis/agent-data/evidence-375.txt` (typed tables + this round),
`analysis/agent-data/patches/{items-drops-sim.patch,items-shops-sim.patch}`,
`crates/wc3-data/tests/data_tables.rs` (6 mechanic tests),
log `/data/data/com.termux/files/usr/tmp/r375c-full.log`.

## 8. Shipped-table consumption + Ponds map resolution (ledger 375, 2026-11-14)

### 8.1 Family counts (typed, unparsed = 0)

`dump_gamedata defs` (rc=0):

```
TFT: units=812 weapons=812 abilities=763 items=273 upgrades=89 effects=52
     buffs=148 destructables=246 doodads=469 unparsed=0
RoC: units=466 weapons=466 abilities=397 items=132 upgrades=58 effects=0
     buffs=84 destructables=128 doodads=278 unparsed=0
```

| Family | TFT rows | Consumed by runtime |
|---|---|---|
| units | 812 | `wc3-game::assets::{GameDatabase,unit_stats,unit_cost}` + sim roster defs |
| weapons | 812 | per-def damage/range/attack rate (sim `UnitDef.stats`) |
| abilities | 763 | generic runner numbers (`AbilityNumbers`), 11 sim `Spell` variants |
| items | 273 | 133 usable / 107 passive-stat / 33 data-only (explicit reason each) |
| upgrades | 89 | sim research path + 52 stat effects / 37 ability unlocks |
| buffs | 148 | buff timers; 115 id-tail + 10 icon links, 23 data-only |
| effects | 52 | `UpgradeEffectMetaData` names for upgrade slots |
| destructables | 246 | map props (trees) → pathing/life |
| doodads | 469 | map props → model/pathing |

### 8.2 Ponds melee map — every placed doodad id (5767 placed / 67 ids)

Test `economy_lumber_doodad_defs` (`crates/wc3-data/tests/data_tables.rs:326`)
parses `war3map.doo` from `donors/War3Net/…/(2)Ponds_v1.4.w3x`:

- **5767 placed doodads, 67 distinct ids** (asserted).
- **65 resolved**: 9 destructables (`LOcg`, `LTba`, `LTbr`, `LTbs`, `LTbx`,
  `LTcr`, `LTex`, `LTlt`, `LTrc`) with model + `pathTex` + life (`HP`), texture
  from `texFile` for `LTlt`, 8 embed the texture in the MDX; **56 doodads** with
  model + `pathTex`.
- **2 unresolved**: `LTrt`, `ZPf0` — present in no shipped SLK (RoC/TFT/full3)
  and Ponds carries no `war3map.w3d`, so the engine has no def for them (inert
  ids: no model, no pathing, nothing renders/blocks).

### 8.3 Map object data (counts)

- Ponds contains **0 of the 6** object-data files (`w3u/w3t/w3b/w3d/w3a/w3q`);
  asserted `applied.is_empty()` and `library.merges().is_empty()`.
- The applied path is proven on the Human04 fixture:
  `crates/wc3-data/tests/base_contract.rs:333`
  (`map_overrides_resolve_through_the_typed_lookup`) applies `war3map.w3u`
  (**4 base + 4 custom entries**) + `war3map.wts` and reads the overridden
  typed values (custom `uC01` name/hp).

### 8.4 Item data-only closure (33)

All 33 data-only items are quest/campaign rows (32 `Campaign`, 1
`Miscellaneous`). Checked both installs: **none carries `abilList` in TFT or
RoC**, and each profile section has only art (no ability/effect data). There is
nothing to close from data; each row keeps
`ITEM_DATA_ONLY_REASON = "no ability and usable=0: campaign/quest row; retail
carries no recipe or effect table for this item"`. Ids: `azhr bzbe bzbf ches
glsk gopr k3m1 k3m2 k3m3 ktrm kybl kygh …` (33 total from `item_usages()`).

### 8.5 Gate

`CARGO_BUILD_JOBS=2 cargo test -p wc3-data --offline` → rc=0, **108 passed**
(45 lib + 4 bin + 8 base_contract + 7 data_tables + 6 hero_items + 7 parity
+ 10 parity_top + 20 real_data + 1 doctest).
Log `/data/data/com.termux/files/usr/tmp/r375e-full.log` (rc=0, 108 passed); dump
`/data/data/com.termux/files/usr/tmp/r375d-defs.txt`.
