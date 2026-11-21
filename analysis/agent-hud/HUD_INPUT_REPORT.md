# HUD + INPUT layer — round report (ledger 396, 2026-11-15)

Scope: `crates/wc3-game/src/{hud.rs,mobile_hud.rs,fdf_ui.rs,text.rs,style.rs,ui/**,ui.rs,game.rs}`.
Goal: the on-device console must behave like WC3 — selecting a worker shows its whole
build list, selecting a hall/barracks its train list, the unit card shows the unit's own
portrait/name/class/stats, tooltips/hotkeys come from the profile data, and every tap
updates the card immediately.

## 1. Open defect: tree/mine tap produced no income (device test `touch_playability`)

Root causes (measured, two independent defects):

1. **Screen-space intent projection used the y=0 plane.** `near_screen` projected the
   destructable/mine at world height 0 while the tap point comes from the *drawn* model
   at the terrain height. Justice's bases sit at ~627 world units, so the projected hit
   point was hundreds of pixels away from the finger and every resource tap missed
   (`lumber 1000 -> 1000`). Fix: `Game::near_screen_world` projects at
   `assets.height_at(x, z) + lift` (game.rs, `touch_tap_intent` / `resource_tap_intent`).
2. **`pick_unit` ran before the resource intent.** Real melee maps guard every gold mine
   with a neutral creep; tapping the mine picked the creep and issued `Command::Attack`
   (the tech-lead probe measured `order right after mine tap: Attack(UnitId(15))`).
   Fix: `resource_tap_intent` runs first for a worker selection — a tap within the mine/
   tree interaction radius issues `Harvest` / `HarvestLumber` even when a unit stands on
   the resource. Non-worker selections keep the attack behaviour.

Also fixed in the same path:

* The opening camera ignored the local player's seat: `spawn_melee_scenario` centred on a
  "most distant start pair" guess, and the camera model anchors the eye at `distance*0.92`
  above the y=0 plane, so on the raised Justice terrain the base was above the top of the
  screen (the test could not tap its own peon: `world (14576,2000) projects to screen
  (1207,-229)`). The camera now centres on the main hall of `self.player`, distance
  `max(1500, 1.95 * ground)` (clamped 2600) so the base, its workers and the nearby trees
  are inside the viewport.

Evidence (observed):

* `CARGO_BUILD_JOBS=2 cargo test -p wc3-game --test touch_playability --offline -- --nocapture`
  rc=0, 2 passed:
  `PARITY economy.lumber touch_tap: lumber 1000->1010 ... picked_at=Some(100) deposited_at=Some(152)`
  `PARITY economy.gold   touch_tap: gold   1000->1010 ... picked_at=Some(154) deposited_at=Some(235)`
* `cargo run -p wc3-game --example gather_probe --offline` (dev):
  mine tap -> `Harvest { mine: Vec2(14592,1280), phase: ToMine }`, gold 1000 -> 1040;
  tree tap -> `HarvestLumber { tree: Vec2(15040,1408) }`, lumber 1000 -> 1080.

## 2. Selection mutation paths -> card refresh

`refresh_buttons()` is the single card rebuild path. Added the missing calls so the card
never lags a selection change by a frame:

* `activate()` (ARMY, BASE hall-select, STOP, pages, items) — refresh at the end.
* `recall_group()` (group recall from the ARMY chip / keyboard digits).
* `place_building()` (the new hall/site becomes the selection).
* Existing: `tap_action` (friendly tap + box select + same-type), `update()` (death prune,
  production finish), `resize`/`set_safe_insets` (layout change).

## 3. Card parity

* 12 slots, row-major 4x3, `hud::build_buttons`.
* Worker: whole build list (9 slots = the full Orc menu), GATHER / STOP / MORE paging.
* Producer: `Trains=` list from the profile data (hall and every barracks/workshop/altar),
  hotkey letters from the object data (`entry.hotkey` -> `card_hotkey` fallback).
* Hero: four ability ids from `unit.def().ability_ids` at 6/7/10/11 with data hotkeys;
  abilities without a simulated `Spell` show a disabled `DATA` row instead of lying.
* **NEW — inventory**: a selected unit with items shows its six inventory slots in card
  cells 0..5 (shipped icon via the item's WC3 id, charge count as sub-label); tapping
  calls `Sim::use_item` through `UiAction::UseItem(slot)`. Non-item cards keep
  ATTACK/MOVE/STOP/HOLD/PATROL/ARMY.
* **NEW — tooltips from data**: `Game::card_tooltip` renders `Tip` + `Ubertip` from
  `Assets::info_for_def` for build/train/item slots; pressing a card slot shows the
  button's own tooltip (the old code replaced it with the *unit's* tooltip).
* **NEW — quest log**: `UiAction::QuestLog` (`QUESTS` card slot, empty/hall card) opens a
  dialog drawn by `MobileHud::draw_dialog` with **live** rows (objective, gold, lumber,
  supply/upkeep, trained/kills/army, idle workers, hero level). Tap closes it.
* **NEW — minimap fog colours**: `MinimapView::fog` grid (32x32, `Game::minimap_fog_cells`)
  draws never-seen cells black and explored-but-hidden cells dimmed; the camera viewport
  rectangle was already drawn.
* **NEW — victory/defeat dialog**: the phase overlay now uses the shipped esc-menu
  nine-slice panel/border/band (`EscMenuBackground`/`EscMenuBorder`/`EscMenuTitleBand`
  when present) with the flat bevel as an honest fallback.
* Minimap viewport rectangle, army row (icons + HP bars) and resource pills: already
  present from the previous round, verified in `hud_view`.
* Building rally: a selected friendly building + a ground tap sets the rally point
  (`INTENT rally`), unchanged and logged.

## 4. Observability added for the device probe (tech-lead steer)

* `status_line()` (game.rs:1484-1520) prints `gold=<n> lumber=<n> sel_defs=[<def ids, max 6>]`
  — the app's periodic `GAME_STATE` line (apps/android-native/src/lib.rs:1461) now carries
  both resources and the selected def ids.
* `resource_tap_intent` (game.rs:2700-2748) runs before `pick_unit` at `tap_action`
  (game.rs:2886-2896) and logs `INTENT harvest_gold pos=(x,y) units=N` /
  `INTENT harvest_lumber pos=(x,y) units=N` before pushing the command.
* `touch_tap_intent` (game.rs:2750-2812) logs `INTENT repair target=<id>`,
  `INTENT return_resources pos=(x,y)`, `INTENT none world=(x,y)`, `INTENT rally pos=(x,y)`,
  and the tap fallthrough logs `INTENT attack target=<id>` (game.rs:2920) /
  `INTENT move units=N at=(x,y)` (game.rs:2976).
* The pre-existing `TAP screen=... world=... picked=... sel=...` line is unchanged.
* Env-gated debug (`H2_TAP_DEBUG=1`) prints `[tap]`/`[intent]`/`[trace]`/`[scene]` to
  stderr for terminal probes; the game log keeps the permanent lines.

## 5. Audio hooks (handoff from the audio layer, `analysis/agent-audio/GAME_RS_HOOKS.md`)

Applied in game.rs: race-based music + tileset ambient (`AUDIO_AMBIENT`), selection tap
dedupe (`audio_last_selection` + `selection_reset`), `Event::UpgradeResearched` ->
`UpgradeComplete<Race>` voice, `BuildingComplete` now logs the voice id.

## 6. Verification

* `cargo check -p wc3-game --offline` — 0 errors.
* `cargo test -p wc3-game --lib --offline hud::` — 58 passed.
* `cargo test -p wc3-game --test touch_playability --offline` — 2 passed (lumber + gold).
* `gather_probe` (real touch path, real MPQ) — gather orders + income for both resources.

## 7. Needed changes outside this layer (report, do not patch)

1. `crates/wc3-game/src/assets.rs`: item icons are not preloaded — only *unit* icons get
   an alias + texture. `UiTheme`/`Assets::load` should alias `ItemData.Art` for every
   item id, otherwise inventory cells fall back to the flat tile.
2. `crates/wc3-sim/src/sim_bridge.rs`: the w3i seat/race assignment shuffled between two
   runs of the same map (`owner=0 Human` vs `owner=0 Orc`); `spawn_melee_bases` picks a
   race by `slots.find(slot == creator)` over an unordered source. Deterministic order is
   needed for reproducible device parity.
3. `wc3-input::Camera`: the eye is anchored at `distance*0.92` above y=0 while the sim
   lives at terrain height; game.rs compensates with a ground-scaled start distance, but
   the camera itself should track the ground height under its target (a map with a
   >1300u plateau would still clip).
4. `apps/android-native` (owner of process exit): the quit **confirm** needs the app to
   open the dialog on the back gesture and poll a `take_quit_request()` result before
   `SDL_Quit` (the lifecycle already raises `effects.quit` at lifecycle.rs:300). The
   dialog chrome and text primitives are in `MobileHud::draw_dialog`; this layer does not
   own the process-exit path, so no half-wired quit button was added.
5. Tests owned by the verify layer: `touch_playability` now depends on the start camera
   centring the player's hall; keep that invariant if the test is reshaped.
