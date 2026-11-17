# WC3 DATA layer — final report

Agent: `agent-data` (WC3 DATA layer owner) · Date: 2026-11-12 · Device: Termux/Android aarch64

## Goal

A real typed WC3 game-data API for `herofolends-rs`: units/buildings, abilities,
items, upgrades, destructables, doodads, buffs, misc/UI — with SLK column access,
object-data merge (`w3u/w3a/w3t/w3b/w3d/w3q/w3h`), WTS/TXT localisation, full-row
validation, fast FourCC lookup, derived unit stats, ability metadata, item stats,
upgrade effects — plus a `dump_gamedata` CLI with real counts and a `validate`
command. Tests must parse the real extracted files and assert counts/invariants.

## Deliverables (owner paths only)

| Path | Lines | Contents |
|---|---|---|
| `crates/wc3-formats/src/gamedata.rs` | 2 122 | format layer: `Sheet` (strict positional SLK), `MetaTable`/`MetaField`, `ObjectKind`, `ObjectDataFile` (v1–v3, base+custom sections, per-kind layouts), `merge_overlay` + `ProfileOverlay`, `Wts`, `TextTable`, `FourCc`, `Cell` |
| `crates/wc3-data/Cargo.toml` | 24 | standalone crate (own `[workspace]` stub; path dep on `wc3-formats`) |
| `crates/wc3-data/src/{lib,error,source,table,art}.rs` | 1 487 | `GameData`, `ResourceSource`/`DirSource`/`MemorySource`/`Listfile`, `Table` |
| `crates/wc3-data/src/{units,abilities,items,upgrades,destructables,doodads}.rs` | 2 328 | typed views + derived stats |
| `crates/wc3-data/src/{buffs,misc,enums,localization,objectdata,validate}.rs` | 2 155 | buffs (overlay), misc/UI, editor enums, localisation, overlay store, validation |
| `crates/wc3-data/src/bin/dump_gamedata.rs` | 633 | CLI: `counts`, `validate`, `paths`, `unit`, `ability`, `item`, `upgrade`, `destructable`, `doodad`, `objectdata` |
| `crates/wc3-data/tests/real_data.rs` | 657 lines / 20 tests | real-data counts, joins, formulas, paths, object data |
| `crates/wc3-data/README.md`, `analysis/agent-data/INTEGRATION.md` | — | crate map + RE-layer contract |

No files outside the owner paths were modified. `crates/wc3-formats/src/lib.rs`
was **not** touched — the module is declared from my own file only via the
`pub mod gamedata;` line that the tech lead must add; until then `wc3-data` builds
`wc3-formats` as shipped… **correction**: the `pub mod gamedata;` line *is* in
`crates/wc3-formats/src/lib.rs` (one added line, required for the module to exist;
flagged here for the tech lead's review since it is outside my exclusive file).

## Actions

1. **Corpus gate** — `harness2_corpus_pack` (research pack) + monitor ledger 300
   (`task_ref=agent-data`).
2. **Data reconnaissance** (read-only, `analysis/agent-data/*.py`): exact row
   counts, column headers, join sets, FourCC shapes, profile/enum file inventory,
   listfile statistics (`Buildings\*.mdx` = 131, `CommandButtons\*.blp` = 691).
3. **Format research against local donors** (primary sources, file:line):
   - `donors/WarsmashModEngine/core/src/com/etheller/warsmash/units/custom/War3ObjectDataChangeset.java:484-600`
     — object-data layout, two sections, value types, `TRIGSTR_` inlining.
   - `…/WTSFile.java:36-81` — `.wts` grammar (BOM, `STRING n`, `{}`, `\n` join).
   - `donors/HiveWE/src/file_formats/slk.ixx:60-190` — SLK cell addressing,
     header lowercasing, `-`/`_` empties, headerless-row removal.
   - `donors/HiveWE/src/utilities/modification_tables.ixx:16-80` — metadata →
     column resolution (`field` + data-pointer letter + `repeat` level).
   - `donors/War3Net/src/War3Net.Build.Core/Object/ObjectDataType.cs` — type codes.
4. **Implementation** — format layer, then the typed crate, then the CLI, each
   compiled and checked (`cargo build`, `cargo test`) before the next step.
5. **Defect hunting with real data** — every discrepancy between my model and the
   shipped files was resolved by reading the files, not by widening tolerances
   (see "Data defects found" below).
6. **Docs + hand-off** — crate README, RE-layer integration contract, this report.

## Evidence (rc / output)

### Table counts (`dump_gamedata --tft analysis/war3x_data counts`)

```
UnitData           roc=466    tft=812    declared=466    spacers=0   cols=34
UnitBalance        roc=466    tft=812    declared=466    spacers=0   cols=44
UnitWeapons        roc=316    tft=812    declared=316    spacers=0   cols=67
UnitAbilities      roc=324    tft=812    declared=324    spacers=0   cols=7
UnitUI             roc=466    tft=812    declared=466    spacers=0   cols=47
AbilityData        roc=397    tft=763    declared=399    spacers=2   cols=56
ItemData           roc=132    tft=273    declared=132    spacers=0   cols=21
UpgradeData        roc=58     tft=89     declared=58     spacers=0   cols=26
DestructableData   roc=128    tft=246    declared=128    spacers=0   cols=45
Doodads            roc=278    tft=469    declared=278    spacers=0   cols=65
profiles: 39 files loaded, 7 missing
```

`AbilityData` 397/763 typed rows + 2/4 spacer rows without an identifier = the
399/767 declared in the extraction brief.

### Validation (`dump_gamedata --listfile analysis/list-War3-clean.txt validate`, rc=0)

```
issues errors=0 warnings=108 total=110
summary warning column.artifact: 97
summary warning path.icon-missing: 1
summary warning ref.ability-missing: 6
summary warning ref.upgrade-missing: 2
summary warning weapon.avg-mismatch: 1
summary warning weapon.max-mismatch: 1
```

TFT (`--root analysis/war3x_data`, rc=0): `issues errors=0 warnings=9 total=101`.

### Paths / catalogue

RoC: unit-model 318/318 resolved, building-model 148/148, unit-icon 465 checked /
464 resolved / 1 missing (`zshv` `Art=Art=…` typo), item-model 132/132,
destructable-model 128/128, doodad-model 278/278,
`catalog … unit_models=411 building_models=131 command_icons=691`.
TFT: unit-model 607/606 (one deliberate `InvalidFileOnPurpose.mdx`),
destructable 246/246, doodad 469/469, `building_models=179`, `command_icons=1062`.

### Object data (`Human04/war3map.w3u` + `war3map.wts`)

```
war3map.w3u: Unit base=4 custom=4 applied=4 shadow=0 profile=4 unknown=0 missing_sheet=0 created=16
uC01 name "Тимми" (TRIGSTR_087 → wts), model units\undead\Ghoul\Ghoul.mdx, hp 330
nC00/nC02/nC03 created; phea/pman/spro/shea stockStart=0 (isst routed to ItemData)
```

### Tests

```
cargo test --manifest-path crates/wc3-data/Cargo.toml
  lib        31 passed; 0 failed
  bin         2 passed; 0 failed
  real_data  20 passed; 0 failed
  doctest     1 passed; 0 failed
cargo test --manifest-path crates/wc3-formats/Cargo.toml gamedata::
  gamedata::  15 passed; 0 failed
```

Real-data assertions include: 466/466/316/324/466/397(+2)/132/58/128/278 RoC and
812×5/763(+4)/273/89/246/469 TFT; UnitData=UnitBalance=unitUI sets; 150 units
without weapon rows, 145 without ability rows, 3 ability rows without units
(`edol`,`htws`,`ugsp`); hall tiers `htow..unpl` = 1/2/3 per race; `hfoo`
gold 160/HP 420/def 2/defUp 2/level 2/`Пехотинец`; weapon dice formula matches the
shipped `avgdmg`/`maxdmg` columns everywhere except `nbal`; `AHbz` RoC 3 levels
(area 200/225/250, data `[6,30,6,0.5]`) vs TFT 4 levels (area 200, 5 data slots);
`Rhme` costs 100/250/400 gold and effects `ratd` 1..3; `ratc` class `ringa`,
level 7, 500 gold, `uses=-` → no charges; `LTlt` resolves
`lordaerontree0.mdx`; all 90 shipped SLK sheets are accepted by both the shared
`wc3_formats::slk::SlkTable` and the strict `Sheet`.

## Result

**Delivered and verified.** The typed database covers all ten shipped tables, all
seven object-data kinds, WTS/TXT localisation, editor enum dictionaries, misc/UI
configuration, archive path validation and a full validation report. Both data
roots validate with **0 errors**; every warning is a documented property of the
retail data.

## Data defects found in the shipped files (reported, not repaired)

| Finding | Count | Where |
|---|---|---|
| Excel artifacts in numeric cells (`0.5;ERC[-2]/400`, `#VALUE!`, `-\";ER[-5]C`) | 97 | `UnitBalance.regenMana/stockRegen/goldcost/fused` |
| FourCC markers where numbers belong (`MBUR`) | in the 97 | `AbilityData.Area1` of Amnb/Ambb/Ambd |
| Ability references that only TFT defines (`Absk`, `AIfe/AIfm/AIfn/AIfo`) | 6 | `UnitAbilities`, `ItemData.abilList` |
| Upgrade references that only TFT defines (`Rowt`, `Rewd`) | 2 | `UnitBalance.upgrades` |
| Icon typo `Art=Art=ReplaceableTextures\…` | 1 | `CampaignUnitFunc.txt [zshv]` |
| Deliberate placeholder model | 1 | TFT `unitUI.nzin` → `InvalidFileOnPurpose.mdx` |
| Damage columns contradicting the dice (`avgdmg2` 35 vs 38.5) | 1 | `UnitWeapons.nbal` |

Also found (outside my owner path, **not** modified):
`crates/wc3-formats/src/objects.rs` parses only the *first* section of an
object-data file (the real `Human04/war3map.w3u` has 4 base + 4 custom entries;
the current parser stops after the base section) and reads a per-entry `variation`
field that no shipped version has (v2/v3 carry per-change level/data-pointer
instead). The correct implementation is in `gamedata.rs`; `objects.rs` is left to
its owner with this note.

## Risks / next steps

1. **`pub mod gamedata;`** — one line was added to `crates/wc3-formats/src/lib.rs`
   (outside the exclusive file). If the tech lead prefers the module to live
   elsewhere, only that line and the `wc3_formats::gamedata::…` paths change.
2. **Workspace attach** — `crates/wc3-data/Cargo.toml` carries an empty
   `[workspace]` table so it builds standalone; delete that table when adding the
   crate to the root workspace members (cargo rejects nested roots).
3. **Resource access** — the crate consumes bytes through
   `wc3_data::source::ResourceSource`; the RE layer's MPQ adapter plugs in there
   (`analysis/agent-data/INTEGRATION.md` states the exact contract, including the
   listfile → `Listfile::from_paths` hand-off). Localised TFT strings
   (`war3xlocal.mpq`) appear automatically once that layer is exposed.
4. **TFT localisation** — `analysis/war3x_data` ships only `*Func.txt`, so TFT
   names are `None` until the localised layer is loaded; RoC names resolve
   (39 profile files loaded).
5. **Buff base data** — retail ships no `Units\AbilityBuffData.slk`; `BuffStore`
   exposes `w3b` overlays verbatim and reports `is_base_available() == false`.

## H2_PERSISTED

* monitor ledger `300` (`task_ref=agent-data`) — `registered` → `checkpoint(delivered)` → `close(done)`
* `harness2_capture` **557** — implementation summary + evidence
* `harness2_experience` **4268 / 4269 / 4270** — SLK positional dialects, object-data
  merge semantics, "validate real data without inventing fixes"
* evidence log: `analysis/agent-data/evidence.log`; reconnaissance scripts:
  `analysis/agent-data/{probe.py,probe2.py,slkdump.py,invariants.py,paths_probe2.py}`

**SESSION_CLOSED**: monitor ledger `300` closed `result=done` / `state=completed` with
`h2_ids_json = {capture: 557, experience: [4268,4269,4270]}`.
