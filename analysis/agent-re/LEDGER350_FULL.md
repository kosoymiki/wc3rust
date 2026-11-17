# LEDGER 350 — full decompilation gap close (RE)

Date: 2026-11-14. Owner: `crates/wc3-re/**`, `analysis/agent-re/**`,
`analysis/tech-lead/map_boot_matrix.json`. `wc3-sim`/`wc3-game`/`wc3-data` read-only.

## 1. Consumption cross-check — every listfile entry

`wc3_re::consumption` (`lib.rs:36`; CLI `wc3re consumption` at
`bin/wc3re.rs:846`; test `consumption.rs:...`) enumerates the same 18 MPQ
archives as the byte manifest and classifies every listfile row by the actual
consumer code (evidence strings carry `file:line`; raw `rg` hits in
`analysis/agent-re/phase0/CONSUMPTION_EVIDENCE.md`).

Artifact: `analysis/agent-re/phase0/consumption.json` + `CONSUMPTION.md`.

| class | entries |
|---|---:|
| runtime | **7 156** |
| data-only | **295** |
| unused | **0** |
| **total** | **7 451** |

| format | entries |
|---|---:|
| BLP | 3 056 |
| MDX | 1 457 |
| MP3 | 1 284 |
| WAV | 1 075 |
| TXT | 143 |
| JASS | 129 |
| IMAGE | 65 |
| PLD | 62 |
| SLK | 59 |
| FDF | 39 |
| MAP | 41 |
| MPQ | 21 |
| AUDIO_DATA (DLS/MID) | 12 |
| MRF | 6 |
| IFL | 2 |

Non-runtime rows are enumerated in `CONSUMPTION.md` with their data-only reason
(cinematic map files, installer/host binaries, DirectMusic data, model-reference
lists, water-flip lists, nested MPQ metadata). No row is unclassified.

## 2. Art completeness (roster families)

`wc3_re::art` (`lib.rs:34`; CLI `wc3re art` at `bin/wc3re.rs:738`; test
`art.rs:321`) audits the roster ids from the sim-generated tables
(`roster_gen`, `creeps_gen`, `shops_gen`, `destructables_gen`) against
`wc3-data`/`unitUI`/`DestructableData`. Tooltip falls back to `name` and scale
falls back to 1.0; every other field is read from the object data.

Artifact: `analysis/agent-re/phase0/art.json` + `ART.md`.

| family | total | model | icon | sound | tooltip | scale |
|---|---:|---|---|---|---|---|
| units | 53 | 53/53 | 53/53 | 53/53 | 52 have + 1 closed | 53/53 |
| buildings | 50 | 50/50 | 50/50 | 50/50 | 49 have + 1 closed | 50/50 |
| creeps | 18 | 18/18 | 18/18 | 18/18 | 18/18 | 18/18 |
| shops | 1 | 1/1 | 1/1 | 1/1 | 1/1 | 1/1 |
| destructables | 246 | 246/246 | 246/246 | 246/246 | 246/246 | 246/246 |

**Missing after object-data fallbacks: 0.** Closed: 2 tooltips (name fallback).

## 3. Ability / item / upgrade / buff typed rows

`wc3_re::catalog` (`lib.rs:35`; CLI `wc3re catalog` at `bin/wc3re.rs:793`;
test `catalog.rs:154`) combines the 763-row `wc3_re::abilities` classification
with `wc3-data::Defs` (units/weapons/items/upgrades/effects/buffs).

Artifact: `analysis/agent-re/phase0/catalog.json`.

| table | rows |
|---|---:|
| abilities (typed effect params/classification) | **763** |
| units | 812 |
| weapons | 812 |
| items (ability ids, charges, stock, cost) | **273** |
| upgrades (levels, costs, effects, requires, units/unlocks) | **89** |
| upgrade effects | 52 |
| buffs (art, tip, ability link) | **148** |
| unparsed | **0** |

Ability effect kinds: damage 244, summon 83, heal 38, aura 28, buff 28,
movement 7; 335 data-only rows carry explicit reasons (item_ability 234,
passive 27, missing_effect_params 33, no_effect_payload 12, no_levels 18,
no_target 11). Item abilities are exposed through `Defs.items[].abilities`.

## 4. Map content — 44 maps + melee proof

`wc3_re::mapcontent` (`lib.rs:47`; CLI `wc3re map-content` at
`bin/wc3re.rs:890`; test `mapcontent.rs:366`) extracts W3I starts, gold mines
(`ngol`), shops, neutral-aggressive creeps, units, doodads, regions, cameras,
sounds and upgrade overrides.

Artifact: `analysis/agent-re/phase0/mapcontent.json` (+ `MAPCONTENT.md`),
`ponds.json`.

- 44 campaign maps: **6 723 units**, **115 134 doodads**, **91 gold mines**,
  **22 shop placements**.
- Library melee `(2)Ponds_v1.4.w3x`: **2 starts, 6 gold mines, 2 shop
  placements, 0 neutral-aggressive creeps, 124 units, 5 767 doodads, 24
  regions**, 0 cameras/sounds (none in that W3I/w3c/w3s).

## 5. Merge rule / callsites

| module | `pub mod` | caller | test |
|---|---|---|---|
| `consumption` | `lib.rs:36` | `wc3re consumption` (`bin/wc3re.rs:846`) | `assets_mpq_consumption_covers_7451_entries` (`consumption.rs`) |
| `art` | `lib.rs:34` | `wc3re art` (`bin/wc3re.rs:738`) | `art_completeness_families_have_required_fields` (`art.rs:321`) |
| `catalog` | `lib.rs:35` | `wc3re catalog` (`bin/wc3re.rs:793`) | `catalog_data_rows_763_273_89_148` (`catalog.rs:154`) |
| `mapcontent` | `lib.rs:47` | `wc3re map-content` (`bin/wc3re.rs:890`) | `mapcontent_melee_ponds_2_starts_6_mines_124_units_5767_doodads` (`mapcontent.rs:366`) |

## Gates

```
cargo test -p wc3-re --lib --offline          # 80 passed, rc 0
cargo test -p wc3-re --offline                # 80 lib + integration 5/1/2/3; isolated CARGO_TARGET_DIR; rc 0
wc3re map-boot --json analysis/tech-lead/map_boot_matrix.json \
  --native-json analysis/agent-re/native_coverage.json   # 44/44 ok, ticks=100, rc 0
wc3re consumption                             # 7451 entries, rc 0
wc3re art                                     # 5 families, missing 0, rc 0
wc3re catalog                                 # 763/273/89/148, unparsed 0, rc 0
wc3re map-content --json ...                  # 44 maps, rc 0
```

## Honest notes

- `unused=0`: every MPQ listfile entry resolves to a runtime reader or a typed
  data-only API in this workspace; host DLLs/EXEs are not present in the shipped
  data MPQs.
- `consumption` runtime classification is format/API-level (BLP loader, MDX
  renderer, data tables, audio, JASS, map loader); per-object roster references
  are covered by the art audit, not duplicated in the 7 451 rows.
- `shops` roster count is 1 (`ngme` from `shops_gen`); map content reports 22
  shop placements across campaign maps.
- `wc3re map-boot` matrix was produced by the current runner (100 ticks, 44/44);
  the new modules do not alter map booting.

## H2

- Capture **832**; experience **4491**.
- Ledger **350** closed `completed`, `verification.ok=true`, warnings 0.
