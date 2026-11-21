# `wc3-data` — typed Warcraft III game database

Typed, validating layer over the shipped WC3 data tables: units/buildings,
abilities, items, upgrades, destructables, doodads, buffs and the misc/UI
configuration files. Parsing of the raw bytes lives in
[`wc3_formats::gamedata`](../wc3-formats/src/gamedata.rs); this crate turns those
tables into a database with typed accessors, derived stats, localisation and
validation.

Nothing here is sampled or synthetic: the tests in `tests/real_data.rs` parse the
extracted retail data in `analysis/war3_data` (RoC) and `analysis/war3x_data`
(TFT) and assert the counts and invariants of those files.

## Quick start

```rust
use wc3_data::source::DirSource;
use wc3_data::GameData;

let source = DirSource::new("analysis/war3_data");
let data = GameData::load(&source)?;

let footman = data.unit("hfoo").expect("footman");
assert_eq!(footman.name().as_deref(), Some("Пехотинец"));
assert_eq!(footman.hp()?, Some(420));
assert_eq!(footman.gold_cost()?, Some(160));
assert_eq!(footman.weapon(wc3_data::units::WeaponSlot::Primary).unwrap().average_damage()?, Some(12.5));
# Ok::<(), wc3_data::DataError>(())
```

## Command line

```console
$ cargo run --bin dump_gamedata -- counts
$ cargo run --bin dump_gamedata -- --listfile ../../analysis/list-War3-clean.txt validate
$ cargo run --bin dump_gamedata -- unit hfoo
$ cargo run --bin dump_gamedata -- ability AHbz
$ cargo run --bin dump_gamedata -- --object-data ../../analysis/war3_map/Human04/war3map.w3u \
                                            --wts ../../analysis/war3_map/Human04/war3map.wts objectdata
```

`validate` exits with code 1 when the data has errors, so it doubles as a gate.
`--root` selects the data root (default `analysis/war3_data`), `--tft` adds the
TFT root to the count report, `--listfile` enables archive path checks.

## What is where

| Module | Contents |
|---|---|
| `source` | `ResourceSource` trait, `DirSource`, `MemorySource`, `Listfile` catalogue |
| `table` | `TableKind` (the ten sheets), `Table` typed cell access |
| `units` | `UnitView`, `WeaponView`, classification, races, hall tiers, models/icons |
| `abilities` | `AbilityView`, `AbilityLevel`, target flags, tooltip resolution |
| `items` | `ItemView` (class, level, charges, stock, cost, abilities) |
| `upgrades` | `UpgradeView`, `UpgradeEffect`, per-level costs |
| `destructables` | `DestructableView` (trees, walls, bridges, debris) |
| `doodads` | `DoodadView` (environment decoration) |
| `buffs` | `BuffStore` — overlay driven; retail ships no `AbilityBuffData.slk` |
| `misc` | `MiscTables` — `MiscData.txt`, `MiscUI.txt`, `WorldEditData.txt`, … |
| `enums` | `EditorEnums` — `UnitEditorData.txt` dictionaries (`weaponType`, `attackType`, …) |
| `localization` | profile text files, `war3map.wts`, object-data overrides |
| `objectdata` | `ObjectDataStore` — loaded `.w3u/.w3a/.w3t/.w3q/.w3b/.w3d/.w3h` files |
| `validate` | full-database report (`Severity`, `Issue`, `PathStat`, `CatalogStat`) |
| `art` | model path normalisation (`dir` + `file`, `.mdx` completion, portraits) |

## Data model notes (all measured on the retail files)

* **Cell values stay text.** Typed readers convert on demand; a cell the engine
  would read as 0 (Excel artifacts such as `0.5;ERC[-2]/400`, FourCC markers such
  as `MBUR`) yields the leading number and is reported by the validator, never
  silently invented.
* **A unit spans five sheets** (`UnitData`, `UnitBalance`, `UnitWeapons`,
  `UnitAbilities`, `unitUI`) joined by FourCC. RoC only has 316 weapon rows and
  324 ability-list rows for 466 units, so every optional sheet is `Option`.
* **Ability levels** are discovered from the row, not from the sheet width: the
  `levels` cell is authoritative (RoC: 357 rows at 1 level, 40 at 3; TFT: 18
  engine pseudo-abilities at 0, 648 at 1, 6 at 2, 77 at 3, 14 at 4). The sheet
  itself has three level column sets in RoC (`Data11..Data34`) and four in TFT
  (`DataA1..DataE4`), and the extra columns can carry filled values the row does
  not declare; `AbilityView::max_level` returns the row's count and
  `AbilityView::column_levels` reports the schema width. The column probe is
  only the fallback when a sheet/override has no parseable `levels` cell.
* **Object data** merges over the shipped rows without mutating them: overlay
  values live in a shadow layer and win on read, so a map's `.w3u` can be applied
  and still audited. Unit object data may also edit `ItemData` (`isst`), which the
  merge target selection handles through the metadata `slk` column.
* **Building tiers** are only derivable for the twelve main halls: their
  `unitUI.unitClass` is `HBuilding01..03`, `OBuilding01..03`, `EBuilding01..03`,
  `UBuilding01..03` and the suffix is the tier. No general tier field exists, so
  `hall_tier()` returns `None` for everything else.
* **Models**: unit/item models are stored without `.mdx` (completed on read);
  destructable/doodad models are split into `dir` + `file` and carry a variation
  suffix (`AshenShrooms` → `AshenShrooms0.mdx`…), so existence is checked across
  all `numVar` variations.

## Tests

```console
$ cargo test --manifest-path crates/wc3-data/Cargo.toml        # lib + bin + real data
$ cargo test --manifest-path crates/wc3-formats/Cargo.toml gamedata::   # format layer
```

Measured on the extracted data (`cargo test`, 2026-11-12):

| root | UnitData | UnitBalance | UnitWeapons | UnitAbilities | unitUI | AbilityData | ItemData | UpgradeData | DestructableData | Doodads |
|---|---|---|---|---|---|---|---|---|---|---|
| RoC | 466 | 466 | 316 | 324 | 466 | 397 (+2 spacers, 399 declared) | 132 | 58 | 128 | 278 |
| TFT | 812 | 812 | 812 | 812 | 812 | 763 (+4 spacers, 767 declared) | 273 | 89 | 246 | 469 |

## Workspace note

`crates/wc3-data` is a member of the repository workspace (root `Cargo.toml`,
next to `crates/wc3-formats`), so both invocations work:

```console
$ cargo test -p wc3-data                                   # from the repository root
$ cargo test --manifest-path crates/wc3-data/Cargo.toml    # explicit manifest
```

The crate depends only on `wc3-formats` (path dependency); it never links an
archive reader, renderer or game crate. `crates/wc3-data/Cargo.lock` is a leftover
from the standalone-root period and is ignored by cargo while the crate is a
member.

## Heroes, shops and item gating (round 8)

| API | Data source | What it returns |
|---|---|---|
| `GameDatabase::hero_unit_ids()` | `UnitBalance.Primary` | hero units (same predicate as `hero()`) |
| `GameDatabase::hero_ability_ids()` / `hero_ability_coverage()` | `UnitAbilities.heroAbilList` × `AbilityData` | catalog ids and `(with all levels, total)` |
| `GameDatabase::hero_abilities()` | same | `HeroAbilityInfo` (owner heroes, `levels`/`reqLevel`/`levelSkip`, per-level mana/cd/range/area/duration/data) |
| `GameDatabase::shops()` / `shop(id)` | unit profile `Sellitems` + `ItemData` | `ShopDef` with per-item price/stock/charges/class/requirements |
| `GameDatabase::shops_selling(item)` | same | shop unit ids |
| `GameDatabase::item_requirements(item)` | `ItemFunc.txt` `Requires`, `DependencyOr` | `Requirement` tokens with alternatives (`TWN2` → `hkee,ostr,etoa,unp1`) |
| `GameDatabase::item_ability_ids()` / `item_ability_coverage()` | `ItemData.abilList` × `AbilityData` | item ability ids and coverage |
| `ItemView::{class_name,sellable,powerup,requirements,ability_views}` | `ItemData` / profiles | per-item accessors (RoC `itemClass` vs TFT `class` handled) |

Recipes are not a retail data structure (no recipe column/class/metadata, no
`Requires` token naming an item); combining items is map-script logic, so the
crate exposes the real shop/charge/ability data and does not invent a table.
