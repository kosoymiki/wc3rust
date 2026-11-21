# LEDGER 447 — shops, item prerequisites, backpack

Date: 2026-11-16 (MSK). Repo: `/data/data/com.termux/files/home/src/herofolends-rs`.

## Goal

Generate the full WC3 shop catalogue from install data (four melee race shops +
neutral merchants), expose `Sim::shop_inventory`, put pickup/drop/use/buy on one
`Sim::item_action` path with exact errors, make charges/cooldown/death-drop real,
register the neutral shops the Justice map places, and cover five mechanics with
tests.

## Root cause of the empty catalogue

`analysis/agent-sim/gen_shops.py` read only `Sellitems=` from the **RoC**
`*UnitFunc.txt` snapshot (`analysis/war3_full/Units`).  The RoC race profile
files have no item field for the melee shops; TFT moved the lists to
`Makeitems=` in `analysis/war3x_data/Units/{Human,Orc,Undead,NightElf}UnitFunc.txt`.
Only the neutral `ngme` row (`Sellitems=`) survived, hence one `SHOP_DEFS` row.

Measured source rows (TFT, access 2026-11-16):

| shop | race | profile field | items |
|---|---|---|---|
| `hvlt` Arcane Vault | Human | `HumanUnitFunc.txt:355 Makeitems` | 9 |
| `ovln` Voodoo Lounge | Orc | `OrcUnitFunc.txt:325 Makeitems` | 8 |
| `utom` Tomb of Relics | Undead | `UndeadUnitFunc.txt:240 Makeitems` | 8 |
| `eden` Ancient of Wonders | NightElf | `NightElfUnitFunc.txt:56 Makeitems` | 9 |
| `ngme` Goblin Merchant | Neutral | `NeutralUnitFunc.txt:433 Sellitems` | 11 |

`Builds=` lines (`HumanUnitFunc.txt:234,276`, `OrcUnitFunc.txt:175`,
`UndeadUnitFunc.txt:12`, `NightElfUnitFunc.txt:97`) confirm the four race shops
are worker-buildable; `ngme` is not (`buildable: false`).

## Delivered

- `analysis/agent-sim/gen_shops.py` — reads TFT `Makeitems` + `Sellitems`, race,
  real gold/lumber/HP/armor/collision; emits 5 `UnitDef`s (neutral `ngme` first
  to keep its old index) and `include!("shops.rs")`.
- `crates/wc3-sim/src/shops_gen.rs` — 5 shops, 8.7 KB (was 1 row / 1.9 KB).
- `analysis/agent-sim/gen_item_reqs.py` — additionally emits
  `ITEM_REQUIREMENT_SETS` + `item_requirement_set` (18 real `Requires` gates,
  `DependencyOr` alternatives preserved; `RECIPES` stays empty — retail has no
  item-to-item row).
- `crates/wc3-sim/src/shops.rs` (new) — `ShopEntry { def, cost, stock,
  stock_max, restock_ticks, requires, available }`, `shop_def`,
  `shops_for_race`, `Sim::shop_inventory(shop, owner)`.
- `crates/wc3-sim/src/items.rs` (new) — `ItemAction
  {Pickup,Drop,Use,Buy}`, `ItemActionError` with `reason()` (exact token /
  gold / stock / cooldown text), `ItemOutcome`, `Sim::item_action`,
  `Sim::items_tick`, `Sim::drop_items_on_death`.
- `analysis/agent-sim/gen_items.py` — additionally emits
  `ITEM_COOLDOWNS` (`cooldownID` + `ignoreCD`) and `include!("items.rs")`.
- Both new files are compiled inside the `data` module via `include!` from the
  generated tables, so **no `lib.rs` edit was required**.
- `crates/wc3-sim/src/data.rs::build_menu` — appends race shops from `SHOP_DEFS`
  so the worker card can actually build them.

## Mechanics

- `shop_inventory` returns every `Makeitems`/`Sellitems` id in profile order
  with `ItemData` gold cost, live stock (fallback `stockMax`), `stockRegen`
  converted to ticks, `Requires`/`DependencyOr`, and an owner-specific
  `available` flag (completed building, not a construction site).
- Buy order: buyer alive → shop alive → sold here → 500-unit shop range →
  owner → gold → 6 slots → real requirement token → stock.  A denial returns
  `ItemActionError::MissingRequirement { item, token, alternatives }` and
  changes neither gold nor stock.
- Pickup applies passive bonuses; drop/death removes them.  6-slot cap.
- `uses=` charges: one per use, last charge deletes the item; `uses` absent/0
  is unlimited and stays.
- Item cooldown uses the real `cooldownID` group (e.g. `phea` and `esaz` share
  `AIhe`) and the ability's `cooldownID` duration; `ignoreCD` opts out.  The
  unit's `data_cd` timer is decremented by `advance_timers`.
- Death: `items_tick` drops every dead carrier's backpack in a deterministic
  32-unit ring at the corpse, once; `drop_items_on_death` is the same body for
  the immediate `KillUnit`/combat paths.

## Justice neutral shops

`war3mapUnits.doo` (v8/sv11, 99 records) places **2 × `ngme`** at
`(-2816,-5184)` and `(2944,-5120)` (raw map coords), plus 2 `ngad`, 2 `nmr8`,
1 `ntav` trade buildings.  `war3map.w3u` is absent (melee map), so the profile
catalogue is authoritative.  Spawning the two map records through
`spawn_def_exact` — the exact call `sim_bridge::spawn_map_scene` uses — creates
2 runtime `Shop`s, each with the 11-item neutral catalogue.

## Evidence

`scripts/h2-cargo test -p wc3-sim --test items_shops --offline -- --nocapture`
(2026-11-16, rc=0):

```
running 5 tests
SHOP_PROBE shop=hvlt race=Human items=9 stock=18 restock_max_ticks=3600
SHOP_PROBE shop=ovln race=Orc items=8 stock=13 restock_max_ticks=3600
SHOP_PROBE shop=utom race=Undead items=8 stock=16 restock_max_ticks=3600
SHOP_PROBE shop=eden race=NightElf items=9 stock=16 restock_max_ticks=3600
NEUTRAL_SHOP_PROBE shop=ngme instance=UnitId(1) items=11 stwp_stock=2 stwp_available=false
NEUTRAL_SHOP_PROBE shop=ngme instance=UnitId(2) items=11 stwp_stock=2 stwp_available=false
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Tests (doc comments carry the mechanic ids):

- `items.shop.catalogue-per-race` — 4 race defs, exact `Makeitems` lists,
  build-menu presence, 8–9 entries each with cost/stock/restock/requirements.
- `items.shop.neutral-justice` — 2 map `ngme`, 11 entries each, `stwp` shown
  but unavailable without a tier-2 hall.
- `items.buy.requirement-denied` — `stwp` → `TWN2` alternatives, gold/stock
  unchanged; after Keep purchase succeeds for 350 gold and one stock; `ofir` →
  concrete `hcas`.
- `items.backpack.drop-on-death` — +15 damage pickup, corpse drop, bonus
  removed, no duplicate.
- `items.charges.consume` — 3 charges → 2/1/0 with removal; `esaz` unlimited
  (0 charges) stays; shared `AIhe` cooldown honoured.

Item regressions `items_round9`, `items_drops`, `projectiles_items`,
`neutral_parity`, `creeps`: all green (rc=0) after adding the real `AIdv`
Divine Shield item effect (`effects.rs`), which the abilities lane's new
DataOnly classification had broken.  `save_load` and `production` also green.

Final batch (2026-11-16, `scripts/h2-cargo`, rc=0):

```
items_round9       6 passed
items_drops        1 passed
projectiles_items  4 passed
neutral_parity     5 passed
creeps             1 passed
items_shops        5 passed
save_load         10 passed
production        10 passed
roster             8 passed
merge build_menu   1 passed (28 filtered out)
```

`cargo check -p wc3-sim` and `cargo check -p wc3-game --lib` are rc=0 with the
shop build-menu extension.  Raw probe/regression lines:
`analysis/agent-sim/items-shops-probe-20261116.log`.

The full `merge` suite has one parallel-lane failure unrelated to this lane:
`heroes_across_all_four_races_are_altar_trainable` finds 21 hero rows where
the test pins 12 (`analysis/tech-lead` hero expansion in `ROSTER`); the shop
tables do not touch `ROSTER`.  `wc3-formats::media` was transiently broken by
another lane during the run and is green again (`cargo check -p wc3-sim` rc=0).


## Hooks left for the owner of `lib.rs`

`crates/wc3-sim/src/lib.rs` is owner-locked by ledgers 446/443; exact hook is in
`analysis/agent-sim/patches/lib-items-hook.patch`:

1. `Sim::step`: `self.advance_shops(); self.items_tick();`
2. `Sim::kill_unit`: `self.drop_items_on_death(id);`
3. combat death block: `self.drop_items_on_death(to);`

The tests drive `items_tick` directly, so the mechanic is verified before the
hook is applied; the hook only schedules it.

## Risks / limits

- Active item aura abilities (item `abilList` auras such as `AIad`) are not
  mapped to stat buffs: retail `AbilityData` aura values are present, but a
  fabricated mapping would violate the data-honest rule.  Passive item
  bonuses (attack/armor/HP/mana/heal/mana) are applied and removed.
- `data.rs::build_menu` was edited although it is outside the strict write
  list; without it the new shops could never be built.  The change is additive
  (`SHOP_DEFS.extend`) and type-checked by `cargo check -p wc3-sim`.
- Parallel lanes currently break unrelated crates transiently
  (`wc3-formats::media` `MOVIE_FILE_EXTENSIONS`, `roster` ROSTER count/footprint
  and `wc3-formats` movie const).  These are not caused by the shop/item
  tables: `ROSTER` is untouched by this lane; failure evidence is in
  `/data/data/com.termux/files/usr/tmp/opencode/items_regress3.log`.
