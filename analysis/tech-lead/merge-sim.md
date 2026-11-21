# SIMULATION merge map — one base, one sim

Status: **2026-11-13**, FULL MERGE executed under BASE.md §12.
Owner: `agent-sim`. Execution paths: `crates/wc3-sim/**`,
`crates/wc3-game/src/sim_bridge.rs`, `analysis/agent-sim/**`, this file.
For the FULL MERGE request the HUD-owned `game.rs`, `hud.rs` and `ui.rs` were
also wired (the HUD owner is informed through this manifest; no other layer's
reader/parser/renderer was touched).

**State: DONE**
- `cargo check -p wc3-sim` → **rc 0**; `cargo test -p wc3-sim --offline` →
  **123 passed / 0 failed / 2 perf ignored** (`round7-sim-test.log`).
- `cargo check -p wc3-game --lib` → **rc 0**; `cargo test -p wc3-game --lib` →
  **103 passed / 0 failed / 19 ignored** (`round7-game-test.log`, current tree).
- Supply, farm, AI, build menu and abilities are wired into the game:
  `Game::new` spawns a Paladin (Holy Light, Divine Shield), an Archmage
  (Blizzard), a Far Seer for the AI (Chain Lightning) and two starting farms;
  the worker card exposes TOWER/FARM/HALL; ability buttons appear for casters;
  unit/point spells enter tap-target mode; all effects call `Sim::cast`.
- `cargo check -p wc3-game --lib` and the game test log are at
  `analysis/agent-sim/{game-check.log,game-test.log}`.
- No device, screenshot, APK install or full-workspace suite.

## FULL MERGE (2026-11-13)

| Gate | Command | rc | Evidence |
|---|---|---|---|
| sim check | `cargo check -p wc3-sim` | **0** | `resume-check.log` |
| sim tests | `cargo test -p wc3-sim --offline` | **0** | 123 passed / 0 failed / 2 ignored; `round7-sim-test.log` |
| game check | `cargo check -p wc3-game --lib` | **0** | `game-check.log` |
| game tests | `cargo test -p wc3-game --lib` | **0** | 95 passed / 0 failed / 18 ignored; `round4-game-test.log` |
| bridge tests | `cargo test -p wc3-game --lib sim_bridge` | **0** | 6 passed (4 facade + 2 JASS) |

### Game wiring added

- `crates/wc3-game/src/game.rs`
  - scenario: `Hpal` + `Hamg` (player), `Ofar` (enemy), two `hhou` farms;
    supply cap 24 / used 17 so training and abilities are usable immediately;
  - `UiAction::Cast(Spell)` + `Game.targeting`; `caster_for`,
    `ability_button`, `perform_cast`;
  - `tap_action` consumes unit/point spell targets before selection/orders;
  - supply/upkeep/AI/placement/AiWave toast delegated to `sim_bridge`.
- `crates/wc3-game/src/sim_bridge.rs`
  - facade (setup/supply/upkeep/afford/place/wave/capabilities);
  - canonical `JassBridge` (adopt/mirror/orders/script mutations) using the
    real `ScriptOrder` world; one `Sim`, no second unit/economy table.
- `crates/wc3-game/src/hud.rs`
  - worker command card: TOWER (hgtw) and FARM (hhou) in the middle row, FARM at
    slot 8, HALL at slot 9;
  - ability buttons at slots 10/11 for the selected caster (label, mana sub,
    enabled by mana/cooldown), replacing zoom only while a unit is selected.
- `crates/wc3-game/src/ui.rs`
  - icons for Farm/Guard Tower/Barracks, roster train actions and the four
    implemented spells (Holy Bolt, Divine Shield, Blizzard, Chain Lightning).

### PRODUCT BUG `input.build` — placement now charges gold atomically

Root cause: the hall card went through `sim_bridge::place_from_card`, which
returned a deferred `Command::Build`; the game queued it for the next sim tick.
The charge was therefore not tied to the accepted placement, and QA sampled
gold without a guaranteed charge/spawn pair.  Fix: the bridge now executes the
canonical atomic API:

- `crates/wc3-sim/src/lib.rs:1820` `Sim::build_instant(builder, kind, at)`:
  validates builder, reads `def.stats.cost`, rejects with
  `Event::Error("DENY BUILD gold")` and no state change when gold < cost,
  otherwise deducts exactly once and spawns before returning.
- `crates/wc3-sim/src/lib.rs:1829` shared `build_instant_impl` — the legacy
  `Command::Build` calls the same cost/spawn path with its historical
  `not enough gold` event, so there is one implementation.
- `crates/wc3-game/src/sim_bridge.rs:78` `place_from_card` hall branch calls
  `build_instant` and returns `Placement::Built(id)`; no deferred command.
- `crates/wc3-game/src/game.rs:1221` handles `Placement::Built`, selects the
  spawned hall, and logs `PLACE_BUILT htow cost=450 gold=<after>`;
  insufficient gold logs `DENY BUILD gold` before rejecting.

Tests added/adjusted:

- `crates/wc3-sim/tests/merge.rs::legacy_build_command_charges_the_hall_once_and_rejects_when_broke`
  — 1000 → 550, units +1, second with 400 rejected at 400 (note: with the real
  450 cost, 500 is affordable, so the negative fixture uses 400; the reported
  "500 rejected" conflicts with the 450 WC3 value).
- `crates/wc3-game/src/sim_bridge.rs::tests::placement_charges_the_hall_and_keeps_farm_as_site`
  — bridge 1000 → 550 + unit; 400 rejected, `DENY BUILD gold` event.
- `crates/wc3-game/src/game.rs::tests::hall_placement_from_card_charges_gold_once`
  — game harness: 1000 → 550, units +1, `PLACE_BUILT htow`, 400 rejected with
  `DENY BUILD gold` log.
- Existing placement/UI tests updated to assert the immediate spawn/log instead
  of a pending `Command::Build`.

Verification: `cargo test -p wc3-sim` → **100 passed / 0 failed**;
`cargo test -p wc3-game --lib` → **95 passed / 0 failed / 18 ignored**;
`cargo check` rc 0 for both. No device run; QA can re-run `input.build`
(gold drop 450, units +1, log `BUTTON BUILD` → `PLACE_BUILT htow`).

### Files changed in FULL MERGE

- `crates/wc3-sim/src/{lib,data,ai,roster_gen}.rs`
- `crates/wc3-sim/tests/merge.rs`
- `crates/wc3-game/src/sim_bridge.rs`
- `crates/wc3-game/src/game.rs`
- `crates/wc3-game/src/hud.rs`
- `crates/wc3-game/src/ui.rs`
- `analysis/agent-sim/{resume-check.log,resume-test.log,game-check.log,game-test.log}`
- `analysis/tech-lead/merge-sim.md`

### Acceptance notes

- No second simulation: game AI/economy/supply/abilities all call `wc3_sim`;
  `JassBridge` owns one `Sim` and mirrors the JASS `World`.
- No placeholders: missing caster/target/mana/cooldown return real refusal
  toasts; construction sites and training use canonical costs/tech.
- The `wc3-re` `ScriptOrder` patch is already applied, so the JASS bridge is
  compiled and tested in `sim_bridge.rs` (not a deferred patch anymore).
- `wc3-game` warnings are pre-existing dead-code/unused warnings in other
  modules; zero errors.

## 0. EXECUTION RESULT (2026-11-12)

| Gate | Command | rc | Output |
|---|---|---|---|
| sim compile | `cargo check -p wc3-sim` | **0** | Finished dev profile in 33.06 s |
| sim all targets | `cargo check -p wc3-sim --all-targets` | **0** | 1 crate compiled, finished 2.76 s |
| sim tests | `cargo test -p wc3-sim` | **0** | 99 passed / 0 failed / 2 perf ignored; log `analysis/agent-sim/merge-test.log` |
| bridge standalone syntax | `rustfmt --edition 2024 --check crates/wc3-game/src/sim_bridge.rs` | 0 | no diff |
| bridge facade type check | `rustc --edition 2024 --crate-type lib` on the facade half against the current `wc3_sim` rlib | **0** | HUD-facing API (setup/supply/upkeep/place/wave/hall/worker) compiles; 2 expected unused-import warnings from the stripped JASS half |
| game facade type check | `rustc --edition 2024 --crate-type lib crates/wc3-game/src/sim_bridge.rs` | **0** | facade has no unresolved imports |
| game facade tests | `rustc --edition 2024 --test crates/wc3-game/src/sim_bridge.rs` + run | **0** | 4 passed / 0 failed; log `analysis/agent-sim/bridge-test.log` |
| JASS bridge (deferred) | full bridge at `analysis/agent-sim/patches/sim_bridge_jass.rs` | pending | needs `wc3-re-script-order-{vm,natives}.patch`; otherwise `E0432: unresolved import wc3_re::jass::vm::ScriptOrder` |
| game lib (not sim's gate) | `cargo check -p wc3-game --lib` | not run | HUD-owned file set still has pre-existing `ui.rs` errors; sim does not block it |

The sim scope is self-contained and green. The only remaining compile conflict
inside the new bridge file is the `ScriptOrder` type from the RE patch; after
that patch is applied the bridge is the next compile unit (then the HUD shim).

### 0.1 Observed game integration state (HUD-owned, not edited here)

`crates/wc3-game/src/game.rs` at resume already calls the canonical sim at
`:332` (`self.sim.enable_ai`), `:610` (`pub fn supply` →
`sim.supply_used/supply_cap`), `:1251` (`UiAction::BuildDef`),
`:1264` (`UiAction::TrainDef`); no `AiState`/`ai_step` matches remain. The HUD
shim only needs to keep those call sites (or delegate them to
`crate::sim_bridge::{setup_scenario,supply,upkeep_label,place_from_card,
wave_toast,is_main_hall,is_worker}`) and add the one-line module declaration.

### 0.2 Files changed in execution mode

- `crates/wc3-sim/src/lib.rs` — `Event::AiWave`, `place_building_ordered`,
  `main_hall`, `supply_building_for_owner`, `upkeep_permille`,
  `set_unit_hp/mana/owner`, `teleport_unit`, `kill_unit` (lines in §2.3).
- `crates/wc3-sim/src/data.rs` — `UnitDef.main_hall`, `is_main_hall`,
  `supply_building_for` (lines in §2.1).
- `crates/wc3-sim/src/roster_gen.rs` — regenerated 98 defs with `main_hall`.
- `crates/wc3-sim/src/ai.rs` — main-hall identity, canonical supply building,
  `Event::AiWave` (lines in §2.4).
- `crates/wc3-sim/tests/merge.rs` — **new** 6 tests; all passed.
- `crates/wc3-game/src/sim_bridge.rs` — **new** 189-line HUD facade
  (compiles, 4/4 tests); JASS bridge moved to
  `analysis/agent-sim/patches/sim_bridge_jass.rs` (441 lines) pending the RE
  `ScriptOrder` patch.
- `analysis/agent-sim/patches/sim_bridge_jass.rs` — 441-line JASS bridge to
  append after the RE `ScriptOrder` patches.
- `analysis/agent-sim/patches/wc3-game-add-sim-bridge.patch` — adds
  `pub mod sim_bridge;` to the HUD-owned `lib.rs` (one line).
- `analysis/agent-sim/patches/game-sim-shim.patch` — HUD-owned `game.rs` shim:
  replaces the inline mine/AI block with `sim_bridge::setup_scenario`, delegates
  supply/upkeep/affordability/placement/AiWave toast to `sim_bridge`, and drops
  the now-unused `AiConfig` import. `patch --dry-run -p1` clean.
- Not touched in execution mode: `game.rs`, `hud.rs`, `ui.rs`, `wc3-re/**`,
  `wc3-jass/**`, `Cargo.toml`.

### 0.2 New `sim_bridge.rs` API (for the HUD shim)

```rust
pub fn setup_scenario(sim: &mut Sim, enemy: u8, mines: [Vec2; 2]);
pub fn supply(sim: &Sim, owner: u8) -> (i32, i32);
pub fn upkeep_label(sim: &Sim, owner: u8) -> &'static str;
pub fn supply_building(sim: &Sim, owner: u8) -> Option<&'static wc3_sim::UnitDef>;
pub fn can_afford_def(sim: &Sim, owner: u8, def_id: &str) -> bool;
pub enum Placement { HallCommand(Command), Site(UnitId) }
pub fn place_from_card(sim: &mut Sim, builder: UnitId, def_id: &'static str, at: Vec2)
    -> Result<Placement, &'static str>;
pub fn wave_toast(event: &Event, enemy: u8) -> Option<String>;
pub fn is_main_hall(unit: &wc3_sim::Unit) -> bool;
pub fn is_worker(unit: &wc3_sim::Unit) -> bool;

```

HUD shim mapping: apply `wc3-game-add-sim-bridge.patch` (module declaration)
and `game-sim-shim.patch` (call sites). The shim then uses
`sim_bridge::{setup_scenario, supply, upkeep_label, place_from_card,
wave_toast, is_main_hall, is_worker}` and keeps no game-local AI/supply state.

JASS bridge (pending RE patch): append
`analysis/agent-sim/patches/sim_bridge_jass.rs` to the same module after
`wc3-re-script-order-{vm,natives}.patch` is applied. It exposes:

```rust
pub struct JassBridge { pub sim: Sim, pub owner_of_player: Vec<u8>, pub missing_units: Vec<String>, ... }
pub fn JassBridge::new(sim: Sim) -> Self;
pub fn JassBridge::bind(&mut self, handle: u32, unit: UnitId);
pub fn JassBridge::attach(&mut self, world: &mut World);
pub fn JassBridge::step_engine(&mut self, engine: &mut Engine, dt: f64, commands: &[Command]);
pub fn JassBridge::step_world(&mut self, world: &mut World, commands: &[Command]);
```

`wc3-jass` itself needs no new module; the earlier
`wc3-jass-add-sim-bridge.patch` and `patches/sim_bridge.rs` are superseded by
`crates/wc3-game/src/sim_bridge.rs` + `patches/sim_bridge_jass.rs` and must not
be applied twice.

---

## 1. Canonical target per concern

| Concern | Canonical target | Why |
|---|---|---|
| Unit/building stats, supply, armor/attack, abilities, income | `wc3-sim` (`data.rs` generated roster, `lib.rs`, `abilities.rs`) | Already the only simulation with deterministic tick tests and a data-driven four-race roster; BASE §2 names `wc3-sim` as simulation owner. |
| Pathing primitives | `wc3-path::Grid` (consumed by `wc3-sim`) | BASE §1/§2; no second grid reader. |
| Opponent AI, workers, tech, waves, retreat, expansion | `wc3-sim::ai` (`Sim::enable_ai`, `AiState`) | The old `Game::ai_step` was a second, thinner AI; the sim version is tested (6 AI tests) and data-driven. |
| Economy, harvest cycle, upkeep | `wc3-sim::Sim::advance_economy` / `deposit` | One economy; the game only issues `Command::Harvest` and reads `resources`/supply. |
| Supply/food | `Sim::supply_used/supply_cap/upkeep_permille` + generated `UnitDef::main_hall` | Removes the hardcoded `12` and army-count upkeep ladder in the HUD path. |
| Construction | `Sim::place_building`, `place_building_ordered` | One construction-site lifecycle; farms/towers go through it, halls keep the legacy instant path only for the scripted opening. |
| JASS script state | `wc3-re::jass` (`World`, `Runtime`, `ScriptOrder`) | BASE §2 names `wc3-re::jass` as the only parser/VM/natives owner. |
| JASS ↔ simulation translation | `wc3-jass::sim_bridge::SimBridge` (new, adapter crate that may depend on both) | `wc3-re` cannot depend on `wc3-sim` and `wc3-sim` cannot depend on `wc3-re` (BASE §1). The adapter is the only legal meeting point. |
| Trigger/timer state | `wc3-re::jass` only | No second trigger store is introduced; the bridge never stores triggers. |

---

## 2. OWNER files changed

### 2.1 `crates/wc3-sim/src/data.rs`

| Change | Line | Purpose |
|---|---|---|
| `UnitDef.main_hall: bool` | 301 | Distinguish halls from farms/burrows/ziggurats/moon wells that also give supply. |
| `UnitDef::is_main_hall()` | 313 | Capability query used by game victory/base selection and sim AI. |
| `data::supply_building_for(race)` | 319 | Canonical farm/burrow/ziggurat/moon-well selection (cheapest non-hall supply building). |

### 2.2 `crates/wc3-sim/src/roster_gen.rs` (generated)

- Regenerated by `analysis/agent-sim/gen_roster.py`; every one of the 98 defs now
  emits `main_hall: true|false` (MAIN_HALLS set: htow/hkee/hcas, ogre/ostr/ofrt,
  unpl/unp1/unp2, etol/etoa/etoe). Stats unchanged.

### 2.3 `crates/wc3-sim/src/lib.rs`

| Change | Line | Purpose |
|---|---|---|
| `Event::AiWave { owner, wave }` | 297 | Lets the game/HUD show wave feedback without a second AI state. |
| `Sim::place_building_ordered` | 1846 | Player/JASS build order: site may be far, worker walks and channels (WC3 `IssueBuildOrder`). |
| `place_building_impl(..., require_near)` | 1855 | Shared validation/footprint/cost path; `place_building` keeps the old near-check for tests. |
| `Sim::main_hall` | 2125 | Canonical hall lookup (`is_main_hall`), not `kind == TownHall`. |
| `Sim::supply_building_for_owner` | 2140 | Race-correct supply building for UI/AI. |
| `Sim::upkeep_permille` | 2151 | Single source of the 1000/700/400 upkeep tiers (used by `deposit`). |
| `Sim::set_unit_hp/mana/owner`, `teleport_unit`, `kill_unit` | 2243-2300 | Script-facing mutations used by the JASS bridge; no direct field pokes from adapter code. |

### 2.4 `crates/wc3-sim/src/ai.rs`

| Change | Line | Purpose |
|---|---|---|
| `first_hall`/`first_hall_id` use `def().is_main_hall()` | 159, 172 | A farm can no longer be mistaken for a hall. |
| `ai_supply` uses `main_hall` + `data::supply_building_for(race)` | 238 | Duplicate roster filter removed; one supply-building rule. |
| `ai_expand` uses `main_hall` | 441 | Expansion upgrades the real hall line. |
| `Event::AiWave` on Build→Attack transition | 584 | One wave event source. |

### 2.5 `crates/wc3-game/src/game.rs` (HUD-owned — integration state from the pre-execution manifest phase, NOT an execution-mode edit)

| Change | Line | Replaces |
|---|---|---|
| `use wc3_sim::{AiConfig, ...}` | 9 | — |
| `UiAction::{TrainDef, BuildDef}` | 56, 58 | `UiAction::{Train, Build}` only (kept for HUD compatibility). |
| `Game.placing: Option<&'static str>` | 172 | `Option<UnitKind>`; allows farm/tower/any roster building. |
| `sim.add_gold_mine(...)`, `sim.enable_ai(enemy, AiConfig{...})` | 370-392 | deleted `struct AiState` + `Game.ai` field + `Game::ai_step` (pre-merge `:121-124`, `:717-760`). |
| `Event::AiWave` toast | 763 | old `toast("ENEMY WAVE")` inside `ai_step`. |
| `Game::can_afford_def` | 622 | `can_afford(UnitKind)` only. |
| `Game::supply()` | 629 | hardcoded `format!("{}/{}", army_count(), 12)` (pre-merge `:1479`). |
| `Game::upkeep_label()` | 636 | army-count ladder (pre-merge `:1640-1646`). |
| `player_hall` → `sim.main_hall` | 644 | `kind == UnitKind::TownHall` (pre-merge `:602-609`). |
| `selected_hall` → `def().is_main_hall()` | 586 | same kind check. |
| `selected_worker` → `def().stats.is_worker` | 609 | `kind == Peasant`. |
| victory checks → `def().is_main_hall()` | 779, 783 | kind check that would also match farms (all buildings share `UnitKind::TownHall`). |
| `card_button_indices` worker card `[..,8,9]` | 497 | worker card now shows farm + hall. |
| `place_building` def-id path + `place_building_ordered` | 1282-1330 | `Option<UnitKind>` + legacy `Command::Build` for every building. Hall keeps legacy path; farm/tower use construction sites. |
| `activate(UiAction::BuildDef/TrainDef)` | 1417, 1430 | new data-driven actions. |
| HUD upkeep label from `sim.upkeep_permille` | 1741 | removed local ladder. |

No other game file was edited. `game.rs` keeps the legacy `Command::Build` path
only for the Town Hall so the opening base and its existing tests retain their
instant-placement semantics; every other building uses the canonical
construction lifecycle.

---

## 3. READ-ONLY patches (ready, exact ranges)

Apply with `patch -p1 < <file>` from the repo root (repo is not a git worktree,
so `git apply` is not required).

### 3.1 `crates/wc3-re/src/jass/vm.rs` (RE owner / dispatcher)
Patch: `analysis/agent-sim/patches/wc3-re-script-order-vm.patch`

- hunk `@@ -364,6 +364,25 @@` — adds `ScriptOrder { unit, order, point, target, id }` before `World`.
- hunk `@@ -395,7 +414,7 @@` — `World.orders: Vec<(u32, String)>` → `Vec<ScriptOrder>`.
- `World::default()` `orders: Vec::new()` is unchanged (type inferred).

Reason: `IssuePointOrder*` currently throws away x/y, and `IssueTargetOrder*`
throws away the target widget. Without these fields JASS cannot drive the sim's
move/attack/build orders. No other file reads `.orders` (grep: only the three
push sites), so the type change is contained.

### 3.2 `crates/wc3-re/src/jass/natives.rs` (RE owner / dispatcher)
Patch: `analysis/agent-sim/patches/wc3-re-script-order-natives.patch`

- hunk `@@ -11,7 +11,7 @@` — import `ScriptOrder`.
- hunk `@@ -556,28 +556,119 @@` — replace the three tuple pushes with
  `ScriptOrder` records and add `issuebuildorder`/`issuebuildorderbyid`
  recording (`id` = rawcode, `point` = x/y). String variants keep strings;
  `*byid` variants decode rawcode ids (`OrderId(...)` / build unit id).
- `issueimmediateorder*` keeps updating `unit.order` as before.

### 3.3 `crates/wc3-jass/src/lib.rs` (dispatcher) — SUPERSEDED
Patch: `analysis/agent-sim/patches/wc3-jass-add-sim-bridge.patch`
- **Do not apply in execution mode:** the canonical bridge now lives in the
execution-owned `crates/wc3-game/src/sim_bridge.rs`. Keeping this module in
`wc3-jass` as well would be a second bridge. Retained only as history.

### 3.4 JASS bridge append (sim owner — waits on 3.1/3.2)
File: `analysis/agent-sim/patches/sim_bridge_jass.rs` (441 lines).
Append it to `crates/wc3-game/src/sim_bridge.rs` after the RE patches land and
add the `wc3_re` imports it carries.  It must not be added while `ScriptOrder`
is absent: the current game facade deliberately has no `wc3_re` dependency so
the HUD shim can compile today.  `analysis/agent-sim/patches/sim_bridge.rs`
(the earlier wc3-jass-targeted copy) is superseded by this file and by
`crates/wc3-game/src/sim_bridge.rs`.

`JassBridge` owns the one `wc3_sim::Sim` and a `handle ↔ UnitId` map:

| API | Behaviour |
|---|---|
| `JassBridge::new(sim)` | binds player 1 → sim owner 1, others → 0. |
| `bind(handle, UnitId)` | associates a pre-existing scenario unit with a JASS handle. |
| `attach(&mut World)` | adopts script-created units, mirrors sim state. |
| `step_engine(&mut Engine, dt, commands)` | script mutations/orders → `Engine::tick` (timers/triggers) → sim step → mirror. |
| `step_world(&mut World, commands)` | same boundary without timers (headless/tests). |
| `missing_units` | rawcodes not in the sim roster; reported, never invented. |

Translation table: `move`; `attack`/`attk`/`attackground` (target handle or point);
`smart`/`smartmove`/`attackmove`; `stop`; `hold`/`holdposition`; `harvest`;
`build` (+ rawcode id); `patrol`. Script-side mutations (`SetUnitX/Y`,
`SetUnitPosition*`, `SetUnitState`, `KillUnit`, `RemoveUnit`, `SetUnitOwner`,
`CreateUnit`) are diffed against the last mirror and applied through
`Sim::{set_unit_hp,set_unit_mana,teleport_unit,kill_unit,set_unit_owner}` or
`spawn_def`; the script `World` is then overwritten from the Sim. Trigger/timer
state stays only in `wc3-re::jass`.

### 3.5 `crates/wc3-game/src/hud.rs` (HUD owner)
Patch: `analysis/agent-sim/patches/hud-farm-button.patch`

- hunk `@@ -630,6 +630,8 @@` — derive `farm_sub` from
  `wc3_sim::def_of("hhou").stats.cost`.
- hunk `@@ -676,13 +678,26 @@` — when a worker is selected (and no hall),
  card slot 8 becomes **FARM** (`UiAction::BuildDef("hhou")`,
  `game.can_afford_def("hhou")`); otherwise it remains PEASANT training.
  Slot 9 remains BUILD hall. `Game::card_button_indices` already routes the
  worker card to slots 8+9 (owner change above), so no other HUD edit is needed.

Until this patch lands the farm exists in the sim/API but has no phone button;
that is the only user-visible gap in this layer.

---

## 4. Ported symbols (source `file:line` → target `file:line`)

| Ported behaviour | Source (pre-merge) | Target |
|---|---|---|
| Enemy production rotation (Footman/Archer/Peasant every 45 ticks) | `crates/wc3-game/src/game.rs:722-740` | `crates/wc3-sim/src/ai.rs:273` (`ai_army`, composition cycle) |
| Wave timer (330 ticks) + `MoveAttack` all non-attacking units | `crates/wc3-game/src/game.rs:741-760` | `crates/wc3-sim/src/ai.rs:518` (`ai_attack`), `:584` (`Event::AiWave`) |
| Enemy opening harvest assignment | `crates/wc3-game/src/game.rs:351-365` | `crates/wc3-sim/src/lib.rs:1989` (`advance_economy`), game `:370` (`add_gold_mine`) |
| Player/Enemy supply display `army_count/12` | `crates/wc3-game/src/game.rs:1479` | `game.rs:629` + `wc3-sim/src/lib.rs:2151` (`upkeep_permille`), `wc3-sim` supply methods |
| Upkeep thresholds by army count | `crates/wc3-game/src/game.rs:1640-1646` | `crates/wc3-sim/src/lib.rs:2151` (`upkeep_permille`) |
| Hall identity (`kind == TownHall`) for victory/base | `crates/wc3-game/src/game.rs:602-609, 769-779` | `crates/wc3-sim/src/data.rs:313` (`is_main_hall`), `lib.rs:2125` (`main_hall`), game `:784` |
| Worker identity (`kind == Peasant`) | `crates/wc3-game/src/game.rs:327, 611-625` | `UnitDef.stats.is_worker` via generated roster (`data.rs` `UnitStats`) |
| Building placement (hall only) | `crates/wc3-game/src/game.rs:1248-1276` | `game.rs:1287` + `wc3-sim/src/lib.rs:1846` (`place_building_ordered`) |
| Farm/supply building choice | absent | `wc3-sim/src/data.rs:319` + `lib.rs:2140` |
| Unit state mutation for scripts | absent | `wc3-sim/src/lib.rs:2243-2300` |
| JASS order capture | `wc3-re` tuple drops target data | `analysis/agent-sim/patches/wc3-re-script-order-{vm,natives}.patch`, consumed by `sim_bridge.rs` |

---

## 5. Duplicates removed / thinned (delegation proof)

1. **Second AI deleted.** `struct AiState` and `Game.ai` are removed from
   `game.rs`; `Game::update` no longer calls `ai_step`; the only AI is
   `wc3_sim::ai::advance_ai` (`crates/wc3-sim/src/ai.rs:118`), invoked from
   `Sim::step`. Proof: `rg "AiState|ai_step" crates/wc3-game/src/game.rs` → no
   matches; `Game::supply()` / `main_hall()` are direct `self.sim` calls.
2. **Second economy deleted.** No `Game` field stores gold/food; the HUD reads
   `self.sim.resources` and `Game::supply()`. Harvest remains a `Command` into
   the one `Sim::advance_economy` cycle.
3. **Supply constant deleted.** `12` no longer appears in the game's supply
   text; `Sim::supply_used/supply_cap` are the only counters.
4. **Hall/worker kind checks thinned.** `kind == UnitKind::TownHall` remains
   only in the legacy `Command::Build` branch and tests; runtime identity is
   `UnitDef::is_main_hall()` / `UnitStats::is_worker`.
5. **JASS parallel state bounded.** `World` is the script projection; the
   bridge overwrites unit/player gameplay fields from `Sim` after every step and
   applies script mutations back before the step. The legacy
   `wc3_jass::JassRuntime` host table remains only for the old parser tests
   (`apps/headless`); it is not reachable from `canonical`/`sim_bridge` and is
   flagged for removal once `apps/headless` moves to `SimBridge` (see §8).
   Delegation proof for the canonical path: `wc3-jass::canonical` already runs
   `wc3_re::jass::Runtime`; `sim_bridge` owns exactly one `wc3_sim::Sim` and
   performs no combat/economy tick of its own.

---

## 6. Public API the game/base consumes (exact signatures)

```rust
// wc3-sim
pub fn Sim::enable_ai(&mut self, owner: u8, config: AiConfig);
pub fn Sim::add_gold_mine(&mut self, pos: Vec2, amount: i32);
pub fn Sim::supply_used(&self, owner: u8) -> i32;
pub fn Sim::supply_cap(&self, owner: u8) -> i32;
pub fn Sim::upkeep_permille(&self, owner: u8) -> i32;
pub fn Sim::main_hall(&self, owner: u8) -> Option<UnitId>;
pub fn Sim::supply_building_for_owner(&self, owner: u8) -> Option<&'static UnitDef>;
pub fn Sim::place_building_ordered(&mut self, builder: UnitId, def_id: &str, at: Vec2)
    -> Result<UnitId, &'static str>;
pub fn Sim::train_def(&mut self, building: UnitId, def_id: &str) -> Result<(), &'static str>;
pub fn Sim::set_unit_hp(&mut self, id: UnitId, hp: i32);
pub fn Sim::set_unit_mana(&mut self, id: UnitId, mana: i32);
pub fn Sim::set_unit_owner(&mut self, id: UnitId, owner: u8);
pub fn Sim::teleport_unit(&mut self, id: UnitId, pos: Vec2);
pub fn Sim::kill_unit(&mut self, id: UnitId);
pub fn data::def_of(id: &str) -> Option<&'static UnitDef>;
pub fn UnitDef::is_main_hall(&self) -> bool;
pub fn UnitStats { is_worker: bool, supply: i32, supply_provided: i32, ... }
pub enum Event { AiWave { owner: u8, wave: u32 }, GoldDeposited { owner: u8, amount: i32 }, ... }

// wc3-game
pub enum UiAction { TrainDef(&'static str), BuildDef(&'static str), ... }
pub fn Game::can_afford_def(&self, def_id: &str) -> bool;
pub fn Game::supply(&self) -> (i32, i32);
pub fn Game::upkeep_label(&self) -> &'static str;

// wc3-jass (patch)
pub struct SimBridge { pub sim: Sim, pub owner_of_player: Vec<u8>, pub missing_units: Vec<String> }
pub fn SimBridge::new(sim: Sim) -> Self;
pub fn SimBridge::bind(&mut self, handle: u32, unit: UnitId);
pub fn SimBridge::attach(&mut self, world: &mut World);
pub fn SimBridge::step_engine(&mut self, engine: &mut Engine, dt: f64, commands: &[Command]);
pub fn SimBridge::step_world(&mut self, world: &mut World, commands: &[Command]);
```

`Game` keeps the old API as well: `army_count`, `enemy_count`, `can_afford`,
`selected_hall`, `selected_worker`, `player_hall`, `issue`, `activate_action`.

---

## 7. Tests to add after freeze (not run now)

`wc3-sim`:
1. `main_hall_and_supply_building_are_data_driven` — htow is a hall, hhou is
   not; `supply_building_for(Human) == hhou`, Orc `otrb`, Undead `uzig`,
   NightElf `emow`.
2. `place_building_ordered_walks_a_far_worker` — worker 1,500 units away,
   `place_building_ordered(hhou)` returns Ok, site exists, worker eventually
   channels and the farm completes.
3. `place_building_still_rejects_a_far_worker_for_the_legacy_api` — preserve
   `Err("builder too far")`.
4. `script_unit_mutations_clamp_and_kill` — `set_unit_hp(0)` kills and frees the
   footprint; `teleport_unit` clears the path; `set_unit_mana` clamps.
5. `ai_wave_event_is_emitted_once_per_wave` — enable AI, run until
   `Event::AiWave`, assert `wave == 1` and the event is not repeated for the same
   wave.

`wc3-game` (`game.rs` tests):
6. `farm_button_raises_supply_cap` — `Game::new`, select a peasant,
   `activate_action(UiAction::BuildDef("hhou"))`, place at a free cell, run 1,500
   ticks, assert `game.supply().1 == 18`.
7. `supply_pill_matches_sim` — `game.supply() == (sim.supply_used(player),
   sim.supply_cap(player))` after training.
8. `enemy_is_driven_by_sim_ai` — `game.sim.ai` contains owner 1; after 1,200
   ticks `game.sim.army_units(enemy).len() >= 2` or `sim.ai_waves(enemy) >= 1`.
9. `ai_wave_event_toasts` — push a fake `Event::AiWave` through
   `process_events` (or run until one) and assert the toast text.
10. `main_hall_victory_ignores_farms` — spawn a farm only, kill the enemy hall,
    assert victory still fires; farm alone is not a hall.

`wc3-jass` (patch):
11. `script_unit_creation_maps_to_one_sim_unit` (already in the patch).
12. `script_order_drives_the_sim` (already in the patch): `IssuePointOrder`
    `move` → `Order::Move`; add `KillUnit`/`SetUnitPosition` parity cases.
13. `engine_step_keeps_world_and_sim_in_sync` — `Engine::enter_map` on
    Human04, attach bridge, step 300 ticks, assert every live sim unit has a
    World handle with the same hp/pos and `world.players[0].gold ==
    sim.resources[0]`.

`wc3-re`:
14. `script_orders_capture_point_and_target` — call the natives through a small
    JASS snippet and assert `World.orders[0].point/target`.

Fixtures: `analysis/war3_full3/Maps/Campaign/Human04.w3m` (existing), plus the
game's `Assets::load(None,None)` fallback for game tests.

---

## 8. Conflicts / boundaries / next step

1. **HUD farm button** — `crates/wc3-game/src/hud.rs` is HUD-owned. Patch in
   §3.5; owner: HUD layer. Until it lands, farm placement is reachable only via
   `Game::activate_action(UiAction::BuildDef("hhou"))`.
2. **RE patches** — `crates/wc3-re/src/jass/{vm,natives}.rs` are write-frozen
   (BASE §6). Patches in §3.1/§3.2; owner: RE agent/dispatcher.
3. **JASS adapter** — `crates/wc3-jass/**` is dispatcher-owned. Patch + new file
   in §3.3/§3.4.
4. **Legacy JASS host** — `wc3_jass::JassRuntime` still owns a unit table used
   only by `apps/headless` (`crates/wc3-jass/src/lib.rs:668+`). It is not on the
   canonical path. Next step (owner dispatcher/apps): move headless to
   `canonical` + `SimBridge` and delete the legacy VM host; not done here because
   `apps/headless` is outside this layer and the freeze forbids verification.
5. **`wc3-data`** — BASE §1 lists `wc3-data` as a workspace member but the
   workspace `Cargo.toml` does not include it. Not a sim file; dispatcher action.
6. **Rendering of new defs** — the game still selects a model by `UnitKind`
   (`Game::scene` → `assets.model(unit.kind)`), and the roster maps every
   building to `UnitKind::TownHall`. A farm therefore renders the hall model
   until the render/data layer maps `UnitDef.kind`/`model` per def. The sim API
   exposes `Unit::def().model`; no render file was touched here.
7. **Replay** — `crates/wc3-replay/src/lib.rs::encode_command` still lacks
   `Harvest`/`CancelTrain` arms (pre-existing; `Command` was not changed in this
   continuation). If replays should record farm orders, the replay owner must add
   those arms (two encode + two decode tags).
8. **Pre-existing game build blockers** — `crates/wc3-game/src/ui.rs`
   (`crate::text`, moved `current_layer`) belong to the HUD/UI merge and may
   block `cargo check -p wc3-game` before this layer's code is even reached.
   Post-freeze order: UI owner fixes those, then the sim compile gate.
9. **Next step (exact):** after freeze, apply the four patches in
   `analysis/agent-sim/patches/`, then run
   `cargo check -p wc3-sim -p wc3-re -p wc3-jass` and `cargo check -p wc3-game --lib`,
   fix any type drift in the bridge (the only uncompiled new file), then run the
   test list in §7. If `ScriptOrder` cannot land, the fallback is for
   `sim_bridge` to consume the old tuple and support immediate orders only —
   recorded here as the degraded path, not silently chosen.

---

## 9. Evidence

- Owner-path edits: `crates/wc3-sim/src/{lib,data,ai}.rs`,
  `crates/wc3-game/src/game.rs`; generated `roster_gen.rs` regenerated by
  `analysis/agent-sim/gen_roster.py` (98 defs, `main_hall` column).
- Patch files: `analysis/agent-sim/patches/{hud-farm-button,ui-roster-icons,
  wc3-re-script-order-vm, wc3-re-script-order-natives,
  wc3-game-add-sim-bridge}.patch`; `wc3-jass-add-sim-bridge.patch` and the
  wc3-jass copy of `sim_bridge.rs` are SUPERSEDED by the game-local bridge.
- Syntax gate: `rustfmt --edition 2024 --check` on every changed/new Rust file
  (`wc3-sim/{lib,ai,data}.rs`, `wc3-game/src/game.rs`, `sim_bridge.rs`) returned
  rc=0, no diff. No cargo test/build was run, per BASE rule 4.
- Patch applicability: `patch --dry-run -p1` succeeded for
  `hud-farm-button`, `ui-roster-icons`, `wc3-jass-add-sim-bridge`,
  `wc3-re-script-order-vm`, `wc3-re-script-order-natives`,
  `wc3-game-add-sim-bridge` and `game-sim-shim`; no read-only file was
  modified.
- The previous sim layer evidence (93 tests, 400 units @0.531 ms/tick) remains
  valid for the unchanged mechanics; capture 552/556, monitor 302.
- H2 for this continuation: capture/experience/monitor checkpoint recorded at
  completion (IDs in the final reply).

## ROUND 2 — ledger 355 (2026-11-13)

Owner paths for this round are `crates/wc3-sim/**` and
`crates/wc3-game/src/sim_bridge.rs` (ledger 355); `game.rs`/`hud.rs`/`ui.rs`
remain read-only and changes for them are delivered as patches.

### Build menu generated from the SLK roster

- `crates/wc3-sim/src/data.rs:319` `main_hall_for(race)`.
- `crates/wc3-sim/src/data.rs:333` `build_menu(race)` — supply building,
  defence, producer, main hall, then remaining positive-cost buildings sorted
  by WC3 gold cost. No building id/price is authored in the menu.
- `crates/wc3-sim/src/data.rs:317` `UnitDef::trains_units()`; the generated
  roster gained `UnitDef.trains` from `*UnitFunc.txt` `Trains=` (e.g. `hbar`
  → hfoo,hrif,hkni; `hars` → hmpr,hsor).
- `crates/wc3-sim/src/lib.rs:2176` `Sim::build_menu_for_owner(owner)`.
- `crates/wc3-game/src/sim_bridge.rs:65` `BuildOption` +
  `crates/wc3-game/src/sim_bridge.rs:75` `build_menu(sim, owner)` — label/cost
  from `UnitDef`, `enabled` from current gold.
- HUD consumption patch (HUD-owned, not applied):
  `analysis/agent-sim/patches/hud-build-menu.patch` replaces the hardcoded
  hhou/hgtw labels with `sim_bridge::build_menu` in the four worker card slots;
  `patch --dry-run -p1` clean.

Data correction found by the menu test: `Mechanical` was used as a building
marker, which misclassified mechanical units (Gyrocopter, Mortar Team) as
buildings and made `hgyr` appear as a producer. Building detection now uses
`TownHall/Ancient/Structure` type, `movetp == "_"`, or a `PathTextures` path;
the roster was regenerated (98 defs, counts unchanged: Human 26, Orc 24,
Undead 24, NightElf 24).

### Win/lose

- `crates/wc3-sim/tests/merge.rs::victory_and_defeat_follow_the_main_hall`:
  killing the enemy main hall emits `Victory(0)`; killing the player hall emits
  `Victory(1)`. Game-level win/lose tests remain green
  (`destroying_enemy_hall_wins`, `losing_own_hall_defeats`).

### Replay

- `crates/wc3-sim/tests/merge.rs::replaying_the_same_command_log_reproduces_the_state_hash`
  — a 600-tick command log replayed from the same seed produces the identical
  `state_hash` (deterministic replay core).
- `cargo test -p wc3-replay` → **4 passed / 0 failed**:
  `replay_roundtrip_and_verify`, `command_encoding_roundtrip`,
  `lockstep_delivers_after_delay`, `detects_desync`
  (`analysis/agent-sim/r2-replay-test.log`). The canonical replay container
  remains `wc3-replay`; no second format was added.
- Game-level recording needs `wc3-game/Cargo.toml` to add `wc3-replay`
  (dispatcher-owned); with the dependency it is a 3-line recorder around
  `sim.step` using `Replay::from_sim`. Not added here because the file is
  read-only for this ledger.

### Round-2 rc

| Gate | rc | Result |
|---|---|---|
| `cargo check -p wc3-sim` | **0** | `r2-sim-test.log` build |
| `cargo test -p wc3-sim` | **0** | **103 passed / 0 failed / 2 ignored** (`r2-sim-test.log`) |
| `cargo check -p wc3-game --lib` | **0** | |
| `cargo test -p wc3-game --lib` | **0** | **95 passed / 0 failed / 18 ignored** (`r2-game-test.log`) |
| `cargo test -p wc3-replay` | **0** | **4 passed / 0 failed** (`r2-replay-test.log`) |
| HUD build-menu patch | — | `patch --dry-run -p1` clean; HUD owner applies |

Artifacts: `analysis/agent-sim/{r2-sim-test.log,r2-game-test.log,r2-replay-test.log}`,
`analysis/agent-sim/patches/hud-build-menu.patch`, this manifest.
H2: ledger 355 (heartbeat/checkpoint), capture/experience IDs in the close
record.

## TOP-3/5 — lumber, destructables, map units, creeps (2026-11-13, ledger 355)

Aligned to `analysis/tech-lead/PARITY_MATRIX.md` §7 rows #3/#4/#5. Full artifact:
`analysis/agent-sim/TOP35.md`.

- **Lumber economy**: `Sim.lumber[2]` (`lib.rs:409`), `UnitStats.lumber_cost`
  from `lumbercost` in every generated def, lumber checked/refunded in
  training, research and construction; `Order::HarvestLumber`, tree HP, 10
  lumber per trip and hall return (`lib.rs:1949`, `:2704`).
- **Destructables**: 246 real TFT rows (`destructables_gen.rs`) with HP/armor/
  model; `spawn_destructable` (`lib.rs:1966`) blocks a cell, `damage_destructable`
  (`:1992`) frees it and emits `DestructableDestroyed`.
- **Map-placed units**: `Sim::spawn_map_units(&[(owner, def, pos)])`
  (`lib.rs:1940`) is the canonical sink for `war3mapUnits.doo`;
  `tests/map_units.rs` parses Human04 through `wc3-formats::map::parse_units`
  and asserts the spawned count equals the parsed known-def count. The
  game-loader call remains in world/HUD scope.
- **Creep camps**: 18 real neutral defs (`creeps_gen.rs`, appended after the 99
  roster, indices stable), `Camp { anchor, aggro=500, leash=900 }`, `spawn_camp`
  (`lib.rs:1903`) and `advance_creeps` (`:2142`): aggro at ≤500, return home
  beyond the leash, no camp-vs-camp attacks.
- **HUD patches** (dry-run clean): `hud-lumber.patch` (real lumber pill),
  `hud-build-menu.patch` (SLK build menu).
- **rc** (TOP-3/5 round): `cargo test -p wc3-sim` **116 passed / 0 failed**
  (logs `top35-sim-test.log`); `cargo check -p wc3-game --lib` **0**; `cargo test
  -p wc3-game --lib` **95 passed / 0 failed / 18 ignored** (`top35-game-test.log`).
  No device run.

## SIM ROUND 4 — map scene, tech gating, ruleset (2026-11-13, ledger 355)

Artifact: `analysis/agent-sim/ROUND4.md`.

- **Building tech gating**: `requirements_met` (`lib.rs:2255`) enforces
  `UnitDef.requires` in both `build_instant_impl` and `place_building_impl`;
  generated tier chain `hcas→hkee`, `ofrt→ostr`, `unp2→unp1`, `etoe→etoa`.
  Test: `building_placement_enforces_the_real_tier_requirements`.
- **RoC/TFT switch**: `Ruleset::{Roc,Tft}` (`lib.rs:58`), `set_ruleset`,
  supply caps 90/100 and upkeep 40/70/90 vs 50/80/100.
  Test: `roc_and_tft_rulesets_switch_caps_and_upkeep`.
- **Map/creep spawn**: `sim_bridge::spawn_map_scene` consumes the canonical
  `wc3_re::mapfiles::MapBundle` — units (neutral player→owner 2), trees/
  destructables from `war3map.doo`, neutral units clustered into camps
  (aggro 500/leash 900). `spawn_map_file` + `spawn_assets_map` (reads
  `Assets.debug` `map=`), with a Human04 test in the game crate.
  Game one-line hook patch: `patches/game-map-scene.patch` (dry-run clean).
- **rc** (ROUND 4): sim **118 passed / 0 failed** (`round4-sim-test.log`); game check
  **0**; game tests **95 passed / 0 failed / 18 ignored** (`round4-game-test.log`).
  HUD lumber/build-menu patches still dry-run clean. No device run.

## SIM ROUND 7 — verification + creep spawn in game (2026-11-13, ledger 355)

Artifact: `analysis/agent-sim/ROUND7.md`.

- **ROUND6 re-verified on current tree**: `build_requires`, `qa_creeps_camps`,
  `ruleset_roc`, `ruleset_tft` rc 0; `cargo test -p wc3-sim --offline` rc 0
  (**123 passed / 0 failed / 2 ignored**, `round7-sim-test.log`).
- **p3 creep spawn defect fixed**: `JassBridge` mapped JASS players 12/13
  (neutral hostile/passive) to owner 0, so map creeps were adopted as player
  units. Now players ≥2 → owner 2 (`sim_bridge.rs:294`) and
  `Sim::cluster_neutral_camps` (`lib.rs:1978`) runs in `JassBridge::attach`
  (`sim_bridge.rs:319`, reached from `game.rs:630`), producing camps with
  aggro 500 / leash 900.
- **Tests**: `neutral_map_units_cluster_into_creep_camps`
  (`tests/merge.rs:910`, 3 camps/idempotent/aggro) and
  `neutral_map_units_are_adopted_as_creep_camps`
  (`sim_bridge.rs:748`, owners 12/13 → owner 2, two camps) rc 0.
- **rc**: `cargo check -p wc3-game --lib --offline` **0**; `cargo test -p
  wc3-game --lib --offline` **103 passed / 0 failed / 19 ignored**
  (`round7-game-test.log`).
- **Open**: map destructables still not adopted by `JassBridge` (units only);
  `game-map-scene.patch` remains the trees path. No device run.
