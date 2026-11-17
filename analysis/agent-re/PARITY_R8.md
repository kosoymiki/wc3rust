# PARITY ROUND 8 — RE layer: hero abilities (#11) + items (#14) runtime

Date: 2026-11-13 · Layer: RE (`crates/wc3-re/**`, `crates/wc3-jass/**`, `analysis/agent-re/**`)
Ledger: 350 · Model: opencode-go/deepseek-v4.1-flash#max

## Goal

Give the RE layer real runtime support for PARITY #11 (hero abilities) and #14
(items/shops): ability state and cast events, item entities with inventory,
charges, use/drop, shop stock, and item pools — with tests, and with the
canonical scan/parity gates green on the current tree.

## Result

All RE layer gates PASS. 14 new runtime tests, 61 total `wc3-re` lib tests.
No stubs or no-op substitutes: every implemented native mutates or reads the
deterministic `World`, and every event fires through the canonical trigger
executor.

| Gate | Command | rc | Evidence |
|---|---|---:|---|
| RE lib compile | `cargo check -p wc3-re --lib --offline` | **0** | `analysis/agent-re/round8-check-all.log` context |
| RE all targets | `cargo check -p wc3-re --all-targets --offline` | **0** | `analysis/agent-re/round8-check-all.log` |
| RE lib tests | `cargo test -p wc3-re --lib --offline` | **0** | **61 passed**, 0 failed — `analysis/agent-re/round8-re-lib.log` |
| Ability/item tests | `cargo test -p wc3-re --lib --offline ability_item_tests` | **0** | **14 passed** — `analysis/agent-re/round8-re-lib.log` |
| wc3-jass | `cargo test -p wc3-jass --lib --offline` | **0** | 6 passed — `analysis/agent-re/round8-jass-tests.log` |
| Corpus scan | `wc3re scan analysis/war3_full3` | **0** | `ok=true`, `maps.errors=0`, **`scripts.unimplemented={}`**, `main_ok=44/44`, `natives=791`, `steps=1 313 749` — `analysis/agent-re/scan-r8.json` |
| Map runtime parity | `wc3re parity` | **0** | 44 maps, `main_ok=44`, `executed_triggers=713`, `map_units=6 723`, `spawned=468`, `matched=6 255` — `analysis/agent-re/round8-parity.json` |
| `qa.jass.map_runtime` | `python3 analysis/agent-re/qa_map_runtime.py` | **0** | `{"ok":true,"failures":[]}` — `analysis/agent-re/round8-qa-gate.log` |

## Implemented runtime (file:line)

### `crates/wc3-re/src/jass/vm.rs`

- `AbilityState { level, permanent, cooldown, cooldown_max }`; `Unit.abilities`
  is now `BTreeMap<String, AbilityState>` (was `BTreeSet<String>`).
- `Item` now carries `owner` (player index for `GetItemPlayer`), `held_by`
  (unit handle), `charges`, `level`, `visible`, `droppable`, `pawnable`,
  `invulnerable`, `powerup`, `sellable`, `drop_on_death`, `drop_id`, with a
  manual `Default` (visible/droppable/pawnable, level 1, charges 1).
- `StockEntry { current, max }`, `ItemPool { entries }`;
  `Unit.stock_items/stock_units/item_type_slots`;
  `World.item_pools/stock_items_default/stock_units_default/item_type_slots_default`.
- `EventContext` spell/item fields: `ability`, `ability_level`,
  `spell_target_unit/item/destructable/point`, `manipulated_item`, `sold_item`,
  `enum_item`.
- Canonical event ids (`analysis/war3x_data/Scripts/common.j`):
  `EVENT_PLAYER_UNIT_{DROP,PICKUP,USE}_ITEM` 48/49/50,
  `EVENT_UNIT_*` 85/86/87, `EVENT_PLAYER_UNIT_SPELL_{CAST,EFFECT,FINISH}`
  273/274/275, `EVENT_UNIT_SPELL_*` 290/291/292.
- `CastTarget { None, Unit, Item, Destructable, Point }`, `AbilityCastReport`.
- `Runtime::fire_unit_event(event, unit)` — matches player-unit registrations
  for the owner (with optional unit-type filter), unit-event registrations for
  the unit or the any-unit sentinel, and player events; executes triggers.
- `Runtime::item_event(event, unit, item)` — sets `GetManipulatedItem` context.
- `Runtime::cast_ability(caster, ability, target)` — validates the caster owns
  the ability, publishes spell context, fires cast/effect/finish player+unit
  events, restores context, returns level + trigger count.
- `Runtime::use_item(unit, item, target)` — validates inventory membership,
  fires use events with `GetManipulatedItem`, consumes one charge, removes the
  item and clears its slot at zero charges.
- `grant_ability` / `ability_level` / `unit_items` / `unit_abilities`.

### `crates/wc3-re/src/jass/natives.rs`

- **Abilities**: `UnitAddAbility` (bool), `UnitRemoveAbility` (bool),
  `GetUnitAbilityLevel` (real level), `SetUnitAbilityLevel`,
  `IncUnitAbilityLevel`, `DecUnitAbilityLevel`, `UnitMakeAbilityPermanent`
  (`unit, permanent, abilityId`), `UnitResetCooldown`, `SelectHeroSkill`,
  `SetPlayerAbilityAvailable` (`Player.disabled_abilities`), `AbilityId`,
  `AbilityId2String`, `GetSpellAbilityId/Unit`, `GetSpellAbility`,
  `GetSpellTargetUnit/Item/Destructable/X/Y/Loc`.
- **Items**: `CreateItem`, `RemoveItem` (clears inventory slots), `GetItemTypeId`,
  `GetItemType`, `GetItemPlayer` (player index), `SetItemPlayer`,
  `GetItemCharges`/`SetItemCharges`, `GetItemLevel`,
  `SetItemDropOnDeath`, `SetItemDroppable`, `SetItemPawnable`,
  `SetItemInvulnerable`/`IsItemInvulnerable`, `SetItemVisible`/`IsItemVisible`,
  `IsItemOwned`, `IsItemPowerup`, `IsItemSellable`, `IsItemPawnable`,
  `IsItemIdPowerup/Sellable/Pawnable`, `SetItemDropID`, `EnumItemsInRect`
  (`GetEnumItem`/`GetFilterItem`), `GetManipulatedItem`, `GetSoldItem`.
- **Inventory**: `UnitAddItem` (6-slot capacity, duplicate no-op),
  `UnitAddItemById` (returns item handle), `UnitAddItemToSlotById`,
  `UnitRemoveItem`, `UnitRemoveItemFromSlot` (returns item, slot preserved as 0),
  `UnitHasItem`, `UnitItemInSlot`, `UnitUseItem/Point/Target`,
  `UnitInventorySize`/`GetInventorySize` = 6; pickup/drop/use events fire with
  contextual `GetManipulatedItem`.
- **Shops**: `AddItemToStock`, `AddUnitToStock`, `RemoveItemFromStock`,
  `RemoveUnitFromStock`, `AddItemToAllStock`, `AddUnitToAllStock`,
  `RemoveItemFromAllStock`, `RemoveUnitFromAllStock`, `SetItemTypeSlots`,
  `SetAllItemTypeSlots`.
- **Item pools**: `CreateItemPool`, `DestroyItemPool`, `ItemPoolAddItemType`,
  `ItemPoolRemoveItemType`, `PlaceRandomItem` (weighted, deterministic RNG),
  `ChooseRandomItem`, `ChooseRandomItemEx`.
- **Trigger registration fix**: `TriggerRegisterPlayerUnitEvent` now reads the
  unit-type **filter** from argument 3 (it previously decoded the event id) and
  treats `null`/zero rawcode as “no filter”, so
  `TriggerRegisterAnyUnitEventBJ` registrations actually match.

### `crates/wc3-re/src/engine.rs`

- `Engine::cast_ability(caster, ability, target) -> AbilityCastReport`
- `Engine::use_item(unit, item, target) -> bool`
- `Engine::grant_ability(unit, ability, level)`
- `Engine::unit_abilities(unit)`, `Engine::unit_items(unit)` (read-only views for
  the sim/HUD bridge).

## Tests (14 new, `jass::natives::ability_item_tests`)

`ability_level_roundtrip`, `ability_permanent_and_cooldown_reset`,
`ability_id_roundtrip`, `select_hero_skill_and_player_availability`,
`spell_cast_fires_effect_and_unit_triggers_with_target_context`,
`spell_cast_requires_the_ability`, `item_create_and_inventory_roundtrip`,
`inventory_rejects_a_seventh_item_and_keeps_one_stack`,
`item_charges_and_use_consume_then_remove`,
`item_events_carry_the_manipulated_item`, `item_flags_and_owner_are_writable`,
`shop_stock_and_slots_are_recorded`,
`item_pool_places_a_known_type_at_the_requested_point`,
`enum_items_in_rect_calls_the_action_for_items_inside_only`.

## Corpus note (observed)

`analysis/war3_full3/Scripts/common.j` (113 082 B) is a **trimmed** variant: it
omits the spell-event constant block, `UnitAddAbility`, all ability-level
natives, item-charge natives and shop natives (the same bytes exist in
`war3_full/`, `war3_full2/`, `war3_data/`). The canonical values/APIs were taken
from `analysis/war3x_data/Scripts/common.j` (136 697 B, 972 natives), which has
spell events, `UnitAddAbility`, shop and pool natives. `war3x_data` also omits
the ability-level and item-charge declarations, so those follow the standard
common.j signatures.

Consequence: `scripts.unimplemented={}` is a census of natives **called by this
map corpus**, not proof that the whole WC3 native surface is implemented. The
ROUND 8 tests close the ability/item/shop surface regardless of corpus calls.
Recommend the tech-lead use `war3x_data/Scripts/common.j` as the native
universe for future census gates.

## External red (not RE)

`cargo check -p wc3-game --lib` rc **101**: `crates/wc3-game/src/sim_bridge.rs`
calls `Sim::spawn_def_exact`, which is absent from `wc3-sim` on the current
tree. That file is READ_ONLY for RE and is an in-flight sim-layer change; RE
did not modify it. `cargo check -p wc3-re --all-targets` is green.

## Risks / Next

- Sim owner: call `Engine::cast_ability` / `Engine::use_item` when the canonical
  `wc3-sim` resolves a cast/pickup/use, and mirror sim hero abilities with
  `Engine::grant_ability`; keep `wc3-sim` as the single gameplay owner.
- HUD/QA: shop purchase UI and `qa.items.shop` (gold debit, stock decrement,
  recipe combine) are product-layer gates still open; RE supplies the state and
  natives.
- `UnitInventorySize` returns 6 for all units; per-unit inventory data (2/6 via
  abilities) belongs to the data/sim layer.
- Item abilities are represented as rawcodes on items but not executed; item
  effect resolution remains sim-owned.

## H2

Capture **756**; experience **4422**; ledger **350** `completed` (`verification.ok=true`, warnings 0).
