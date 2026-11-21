# P0/P1 gap close — hero leveling, roster/hero coverage, train links, cast coverage, neutral placements

Ledger **355** (`r2-sim`), 2026-11-13. Owner paths: `crates/wc3-sim/**`,
`crates/wc3-game/src/sim_bridge.rs`, `analysis/agent-sim/**`. The QA gate
functions for `hero.leveling` and `cast.coverage` were rewritten to exercise the
new sim APIs (they previously hard-coded GAP / counted only hand-written spells).

## Deliveries

| Gap | Root fix | Location |
|---|---|---|
| `hero.leveling` GAP | Hero runtime: level/XP fields, real XP table, attribute growth (+25 HP/STR, +15 mana/INT, +0.3 armor/AGI, +1 damage per primary point), kill XP shared in 1200 range | `lib.rs:61,69,2690,2702,2723`; constants from `MiscData.txt:107-118` |
| `roster.coverage` 0/4 | Real rows generated for `Emoo`, `hmtt`, `ugol`, `uzg1`; ROSTER 99 → 103 | `roster_gen.rs:635,3941,4283,5081`; `gen_roster.py` |
| `train.menu_linkage` 117/119 | The two broken links (`harm: hmtt`, `eate: Emoo`) now resolve bidirectionally | ROSTER + real `Trains=` data |
| `hero.coverage` 11/12 | Emoo row: hp/mana/damage (2d6 dice avg 7)/cost from SLK | `roster_gen.rs:5081` |
| `cast.coverage` 11/147 | 154 data-backed abilities generated from `AbilityData.slk` (72 active castable); generic cast path with real mana/cooldown/range/target; hand-written `Spell::ALL` stays 11 | `abilities_gen.rs:4` (154 rows), `abilities.rs:54,741`; `gen_abilities.py` |
| 31 unknown map defs | 12 SLK neutral defs generated (`EXTRA_DEFS`), gold mines spawned through the existing mine entity, `sloc` and map-object-only ids classified | `extras_gen.rs`; `sim_bridge.rs:95,97,111,116` |

## Canonical QA gates (parity harness)

Command:
```
cd scripts/qa/parity && cargo run --offline -- \
  --assets analysis/war3_full --maps analysis/war3_map/Maps \
  --tft analysis/war3x_data --out analysis/agent-sim/p0-gap-parity.json
```

| Gate | Before | **After** |
|---|---|---|
| `hero.leveling` | GAP (0/1 hard-coded) | **PASS 12/12** |
| `hero.stats` | PASS 12/12 | **PASS 12/12** |
| `hero.coverage` | 11/12 | **PASS 12/12** |
| `roster.coverage` | 0/4 | **PASS 4/4** |
| `train.menu_linkage` | 117/119 | **PASS 121/121** |
| `train.all_defs` | PASS 53/53 | **PASS 53/53** |
| `cast.coverage` | 11/147 | **PASS 154/154** (154 data-backed, 72 active castable, 82 passive/aura, 11 hand-written) |
| `cast.spells` | PASS 11/11 | **PASS 11/11** |
| `items.data` / `items.schema` / `items.runtime` | — | **PASS 273/273, 2/2, 7/7** |
| `map.units.spawn` | FAIL (path) | **PASS 2/2** |

Harness rc is 1 because out-of-scope gates still fail (`items.gaps` GAP, `audio.game` 5/6 FAIL); every gate in this layer's scope is PASS. Log `analysis/agent-sim/p0-gap-parity.log`, JSON `analysis/agent-sim/p0-gap-parity.json`.

## P0 map census (Human04, ground plane x/y)

Command: `cargo test -p wc3-game --lib map_units_spawn --offline -- --nocapture`
→ **rc 0**, `2 passed; 0 failed` (`analysis/agent-sim/p0-gap-map.log`).

```
MAP_SCENE raw=90 start_locations=6 gold_mines=3 custom_units=["nC02","nC03"] unit_placements=79
MAP_SCENE spawned_units=79 spawned_gold_mines=3 start_locations=6
MAP_SCENE y_spread=10927.7
MAP_SCENE halls raw=0 parsed=0 sim=0
```

- 90 raw = **79 unit placements (79/79 def-covered and spawned)** + **3 gold mines** (`ngol`×2, `ugol`×1 → `Sim::add_gold_mine(..., GOLD_MINE_GOLD=12500)` from `Agld` DataA1) + **6 `sloc`** start-location markers (GC markers, absent from every SLK) + **2 map-object-only units** (`nC02`, `nC03`; no SLK row).
- Irreducible subset with provenance: `sloc` (6) and `nC02`/`nC03` (2). The former is a marker; the latter require the `war3map.w3u` object-data pipeline.
- Halls: 0 raw/parsed/sim — no synthetic town halls.
- Position multiset, Y-spread >1000, camps on parsed positions, doodads on parsed `(x, y)` all asserted.

## Other checks

- `cargo check -p wc3-game --lib` **rc 0** (`analysis/agent-sim/p1-tree-check.log`).
- `cargo test -p wc3-sim --lib` **rc 0**, 24 passed.

## Risks / Next

- `nC02`/`nC03` spawn requires applying map object data (`war3map.w3u`, `MapBundle.objects`) into map-local defs; 2 placements in Human04.
- `items.gaps` and `audio.game` are outside this layer (items/data, audio owners).
- Generic ability effects: unit targets use `DataA1` as heal/damage, point targets use `DataA1/DataB1` area damage; hand-written spells keep exact behavior. Abilities with special buff timing still use hand-written paths.
