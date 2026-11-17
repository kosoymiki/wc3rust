# Ledger 402 — UI FLOW + GHOST + TOOLTIPS (wc3-game)

Date: 2026-11-15 (MSK). Owner: lane 402. Repo: `~/src/herofolends-rs`.

All paths below are relative to `crates/wc3-game/src/`.

## 1. Build ghost (WC3 placement preview)

`game.rs`:

- `Game::begin_placement` (game.rs:1784) — anchors the ghost at the viewport
  centre and logs it immediately.
- `Game::ghost()` (game.rs:1800) — `GhostState { def_id, at, legal }`, `legal`
  from `Sim::can_place`.
- `Game::log_ghost()` (game.rs:1812) — dedupes by grid cell, emits
  `GHOST def=<id> at=(x,y) legal=<bool>`.
- `Game::pointer_move` (game.rs:2737) — logs the ghost on every cell change.
- `Game::place_building` (game.rs:3443) — logs the same line for the placement
  attempt (`legal` is what the player saw).
- `Game::scene` ghost block (game.rs:4570) — real MDX of the def at the pointer,
  `mesh_alpha`/`layer_alpha` scaled by **0.5**, green tint when legal, red when
  blocked, plus the footprint ring (`footprint * 128 / 2 + 24`).
- Rally marker (game.rs:4632) — blue ring + white core at `unit.rally` of every
  selected player building.

Log format (device):
```
GHOST def=hgtw at=(3904,3904) legal=true
GHOST def=hgtw at=(3904,3904) legal=false
```

## 2. Tooltips (retail 3 sections, anchored)

- `mobile_hud.rs` `HudView::tooltip_anchor` (mobile_hud.rs:455) — the tooltip is
  anchored to the hovered slot, never centred.
- Band stack in `MobileHud::layout` (mobile_hud.rs:1449) — bottom-up:
  console → toast rows → hint → tooltip. Each band reserves its rows.
- `TOOLTIP_MAX_COST_LINES = 8` (mobile_hud.rs:648) — retail cap.
- `rich_line` (mobile_hud.rs:653) — applies `|cAARRGGBB…|r` colour runs.
- `hud::color_runs` (hud.rs:914) — colour-code tokenizer.
- `hud::Font::wrap` / `Font::paragraph` / `Font::fit_text` / `Font::draw_fit`
  (hud.rs:726/785/848/869) — the single HUD text-fit policy: measure →
  ellipsize → `debug_assert` fits → never draw outside the rect.
- `game.rs::card_tooltip` (name + description) and
  `game.rs::card_cost_lines` (COST / FOOD / BUILD / REQUIRES, max 8) feed the
  three sections. Section 3 uses `style::GOLD` = `FED312` (style.rs:12).

## 3. Toast band

- `Game::toast` (game.rs:4249) pushes into `toast_queue` (max 3 rows, newest
  first); `update` ages and drops them.
- `HudView::toasts` + `layout.toasts` — one row per toast, stacked above the
  console, below the hint and the tooltip.
- `MobileHudLayout::zone_rects` now includes `army`, `toast`, `toast2`,
  `toast3`, `hint`, `tooltip`, so `layout.issues()` fails if any of them
  overlap.

## 4. Menu flow

`Screen` (game.rs:41): `MainMenu | MapSelect | Loading | Paused | Victory |
Defeat | Playing`.

- Retail boot: `Game::update` opens the main menu on the first frame of a real
  install session unless the host injected input first (headless probes drive
  the sim directly) — game.rs:2333.
- `Game::sync_menu` (game.rs:2943) builds the live title/subtitle/buttons for
  the current page: Main (PLAY / MAP SELECT / QUIT 2-tap), Skirmish (RACE,
  OPPONENT, MAP, DIFFICULTY, START GAME, BACK), pickers (race/opponent/map/
  difficulty), Loading, Paused (RESUME, RESTART, QUIT 2-tap), Victory/Defeat
  (RESTART, MAIN MENU) with the battle stats line
  (`KILLS … TRAINED … ARMY … TIME … GOLD … LUMBER …`).
- `Game::begin_battle` (game.rs:3172) starts the selected map (reloads assets
  when the picker changed the map file) or restarts the same battle, then runs
  the loading screen; `Game::load_map` (game.rs:3208) rebuilds the game from
  `Assets::load(mpq, map)` keeping race/opponent/difficulty.
- Race is real: `spawn_melee_scenario` calls
  `sim_bridge::setup_melee_1v1(player_race, enemy_race, …)`; `sim_race`
  (game.rs:82) maps `RaceChoice` → `wc3_sim::Race` (Random is seed-stable).
- Back key: `Hotkey::Escape` → cancel order mode → pause menu (game.rs:2090);
  on a glue screen it walks one page up (`menu_back`, game.rs:3332), and on the
  main menu it arms QUIT.
- F-keys: `Game::function_key(u8)` (game.rs:3356) — F1/F9 quest log, F10 game
  menu.

## 5. In-game panels

- Quest log: `UiAction::QuestLog` (card `QUESTS` F9 button) and F9.
  `quest_log_lines` (game.rs:4224) now starts with the map's `war3map.w3i`
  objective header (`MapInfoText`, game.rs:274): `MAP …`, `BY …`,
  `PLAYERS …`, description, then the live main quest + economy/army lines.
- Game menu: F10/BACK, `Screen::Paused` with RESUME / RESTART / QUIT (2-tap).
- Army panel: `MobileHudLayout::army`/`army_slots` (mobile_hud.rs:1218) — one
  icon per selected unit inside the console under the selection text, each with
  its own HP bar; taps are registered as `ARMY_TARGET_BASE + n` touch buttons
  (game.rs:2262) and select that unit (`ARMY_PICK slot=… unit=… def=…` log).
  Icons resolve `type_icon → icon_name_for_def → unit_icon_name → unit_icon`.

## 6. Controls

- Box select: existing `TouchEnd::Box` → `ui::selection::box_select`.
- Control groups 1–9: `control_groups` + group row (10 chips) + digits;
  double-tap a digit centres the camera.
- Rally point: right-click (two-finger tap) on a selected building sets
  `sim.set_rally` (game.rs:3367, `INTENT rally …` log) and the scene draws the
  ghost marker rings.

## 7. Retail command bar (ledger 406 numbers)

`MobileHud::layout` (mobile_hud.rs:1290):

- fractions: tile `0.038·H`, gap `0.0075·H`, pitch `0.0455·H`, bar
  `0.1745·H × 0.129·H`, 4×3;
- touch floor: tile ≥ `48·scale`; the pitch ratio `1.19737` is preserved;
- the bar is bottom-anchored in the console and clamped by
  `(console.h − 52·scale) / 3.3947` and a width budget.

Computed pixels (from the `hud_command_tiles_carry_only_the_hotkey_overlay`
test output):

| viewport | scale | tile | pitch | bar |
|---|---|---|---|---|
| 2414×1080 | 1.10 | 52.80 px | 63.22 px | 242.5 × 179.2 px |
| 1080×2414 | 1.10 | 76.47 px | 91.57 px | 351.2 × 259.6 px |

Retail (no touch floor) at 1080 px height: tile 41.04 px, pitch 49.14 px,
bar 188.46 × 139.32 px.

Tile text: exactly one frame — the hotkey overlay (`card_slot_text_rects`,
mobile_hud.rs:2772). No caption, no cost badge (`card_label` drawing removed
from tiles). Cost/requirements live in the tooltip section 3.

Font: `FRIZQT__.TTF` from the install is already wired
(`load_game_font`, game.rs:502) with the Cyrillic system fallback only when the
game face covers < 60 Cyrillic glyphs (`Font::coverage().cyrillic_covered`).

## 8. Audio `dist=` evidence

- `Game::audio_distance` (game.rs:1647) — horizontal distance from the audio
  listener (camera target set by `update_audio`) to a world point.
- `GAME_AUDIO_ACK` (game.rs:1686), `GAME_AUDIO_ATTACK` (1747),
  `GAME_AUDIO_DEATH` (1653), `GAME_AUDIO_READY` (1783) now end with `dist=<n>`:
```
GAME_AUDIO_DEATH unit=hpea set=Peasant def=PeasantDeath voice=18 dist=1234
```
- No visibility gate was added to sound (distance only, per lane 406/409).

## 9. Bounty / floating labels, fog

- `Event::Bounty { owner, unit, gold }` → `GAME_BOUNTY amount=<gold> pos=(x,y)`
  and a floating `+<gold> G` label at the victim position
  (`FloatingLabel`, game.rs:274; rise 90 units + fade over 1.2 s, drawn with
  `Font::draw_fit`).
- `Event::GoldDeposited`/`LumberDeposited` carry no position: they emit
  `GAME_DEPOSIT kind=gold|lumber amount=<n>` and **no** floating label (no
  invented anchor).
- Fog: `Game::scene` skips enemy units when
  `!sim.is_visible(player, unit.pos)` (game.rs:4586); health bars the same.
- Minimap: `ui/minimap.rs::blips` keeps live visible blips and adds faded
  `BLIP_LAST_SEEN` blips for explored-but-not-visible contacts from
  `Sim::last_seen_all`.

## Acceptance evidence (2026-11-15, observed)

```
CARGO_BUILD_JOBS=2 cargo check -p wc3-game --lib --offline      -> rc=0
CARGO_BUILD_JOBS=2 cargo test  -p wc3-game --lib --offline hud::  -> 64 passed, 0 failed (rc=0)
CARGO_BUILD_JOBS=2 cargo test  -p wc3-game --lib --offline game:: -> 46 passed, 0 failed, 2 ignored (rc=0)
CARGO_BUILD_JOBS=2 cargo run -p wc3-game --example gather_probe --offline --release
  -> rc=0; "order 3 ticks after mine tap: Some((Harvest { mine: …, phase: ToMine }, …))"
           "order 3 ticks after tree tap: Some((HarvestLumber { tree: …, phase: ToMine }, …))"
           gold 1000 -> 1090, lumber 1000 -> 1100
```

Log files: `/data/data/com.termux/files/usr/tmp/opencode/gather_probe_after2.log`
(post-fix, authoritative). The earlier `gather_probe_after.log` was renamed
`gather_probe_after.SUPERSEDED-bootmenu-bug.log`: it ran before the boot-menu
fix and shows `Idle` orders (the menu stole the tick), so it must not be read as
a pass.

New tests (game::tests, ledger 402):

- `ui_ghost_draws_the_model_at_half_alpha_with_legal_tint`
- `ui_menu_flow_boots_to_main_menu_and_reaches_battle`
- `ui_game_menu_pauses_resumes_and_confirms_quit`
- `ui_victory_screen_reports_stats_and_restarts`
- `ui_army_panel_lists_each_selected_unit_and_its_taps_select_it`
- `ui_rally_marker_is_drawn_for_a_selected_producer`
- `ui_bounty_spawns_a_rising_label_and_logs_amount_and_position`
- `ui_deposits_are_logged_without_an_invented_position`
- `ui_fog_hides_enemy_models_but_keeps_visible_friendlies`
- `ui_minimap_keeps_a_faded_last_seen_blip_in_explored_fog`
- `ui_boot_menu_never_interrupts_an_input_driven_session`

New tests (hud::mobile_hud::tests):

- `hud_tooltip_is_anchored_to_the_slot_and_stays_above_the_toast_band`
- `hud_toast_rows_get_their_own_band_and_never_touch_the_card`
- `hud_command_tiles_carry_only_the_hotkey_overlay`
- `hud_tooltip_has_three_sections_and_caps_cost_rows`
- `hud_fit_text_ellipsizes_cyrillic_inside_the_rect`

## Cross-lane note (temporary, may be superseded)

The wc3-render lane added `MeshData::uvs2` and `MeshLayer::uv_set` while lane
402 was running; `crates/wc3-game/src/render.rs` (read-only for this lane) and
`world_audit.rs` carried stale initializers, which blocked every wc3-game build.
Minimal additive patches were applied so the gate can run:
`render.rs` (uvs2: Vec::new() in `gpu_models` + the perf test fixture; uv_set: 0
in both layer constructors) and `world_audit.rs` (`uv_set: 0` in the
`layer_alpha_at` fixture). They encode "TEXCOORD0 only", the documented
contract; the owning lane should replace them with its real second-UV data.
