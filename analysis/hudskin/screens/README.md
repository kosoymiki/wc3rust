# Ledger 469 — full-screen HUD screens and command-card legibility

Owned files: `crates/wc3-game/src/mobile_hud.rs`, `crates/wc3-game/src/hud.rs`.
Everything here draws from the operator's FINAL pack
(`analysis/hudskin/final/herofolends-hud-final/manifest.json`); the 24 px
manifest insets are used verbatim, only the nine-slice centre cell stretches.

## Screens delivered

| screen | data | drawing | evidence |
|---|---|---|---|
| Map select (38 maps) | `MapSelectData` from `map_registry::Registry` (`analysis/maps/REGISTRY.json`, 38 rows) | `MobileHud::draw_map_select`, paginated 90 px rows, large PLAY | `map-select-{2414x1080,1080x2414}.png` |
| Loading | `LoadingView` (map, race, progress, tip) | `draw_loading`: `loading_frame_normal_1024x512` + `build_progress_normal_64x8` fill | `loading-*.png` |
| Quest log | `hud::QuestLog` / `QuestRow` (`Game::quest_rows()` adapter point) | `draw_quest_log`: panel, 90 px rows, done/not-done marks, pager | `quest-log-*.png` |
| Score | `hud::ScoreData` / `ScoreRow` (`Game::score_rows()` adapter point) | `draw_score`: `score_panel_normal_1024x512`, label/value columns | `score-*.png` |
| Allies | `DialogScreen::Allies` | `draw_dialog` (pack plate + rows, 90 px CLOSE) | `allies-*.png` |
| Journal | `DialogScreen::Journal` | `draw_dialog` | `journal-*.png` |
| Game menu | `DialogScreen::GameMenu` + `GAME_MENU_ACTIONS` (Resume 7 / Save 14 / Load 15 / Restart 9 / Quit 2) | `draw_dialog` action rows (>= 90 px each) | `game-menu-*.png` |
| Command card (addendum) | `CommandButton` (`mana_cost`, `autocast`, `cooldown`, `cost`, `affordable`, `page`) | `draw_card_cell` | `command-card-*.png` |

Pack assets used by name (`HudPack::nine` → manifest insets):

* `menus/menu_panel_normal_512x384` — panels (24 px insets)
* `menus/menu_button_{normal,hover,pressed,disabled}_256x64` — rows and buttons (24 px insets)
* `menus/loading_frame_normal_1024x512` — loading frame (24 px insets)
* `menus/score_panel_normal_1024x512` — score panel (24 px insets)
* `commandcard/button_border_empty_64` — empty card cells (12 px insets)
* `build/build_progress_normal_64x8` — progress fill (flat strip, stretched, no corners)
* `tooltip/tooltip_panel_normal_384x256` — tooltip/toast parchment (24 px insets)

## Rules enforced

* **Touch floor**: every interactive row/button is `>= 90 px`. Content that
  does not fit is paged, never shrunk. Map select: 6 rows/page on
  2414x1080 (7 pages for 38 maps), 12 rows/page on 1080x2414 (4 pages).
* **9-slice corners**: the pack `nine` helper keeps `inset * scale` source
  pixels for corners/edges (24 px authored, 26.4 px at the device scale 1.1)
  and only stretches the centre cell. No magenta tint is emitted; the headless
  test fails on any magenta pixel.
* **Text fit** (`hud::fit_text_plate`): step the font down from the authored
  size to `MIN_TEXT_PX` (24 px), wrap on word boundaries with live glyph
  metrics, clip to the plate's inner rect (last line ellipsized). A 147-char
  toast/tooltip run is proven inside both plates, classic and pack parchment.
* **Hotkey badge**: opaque plate (`0.015` sRGB, alpha 0.94) at the cell's
  top-left, 1.5 px black outline, letter `tokens.text` at the 24 px floor.
  Measured 29.4 x 29.4 px in a 90 x 90 cell (26.4 px letter), contrast over a
  pure-white icon 14.25:1 (floor 3:1). Painted in the textured pass *after*
  the icon so pack-font glyphs are never covered.
* **Empty cell**: `button_border_empty_64` over a card-plate well
  (38,39,40 sRGB, measured luma 0.152) plus a 1 px top light (luma 0.235) —
  no pure-black interior.
* **Ability cell**: icon 1:1 in the authored window, radial cooldown wedge from
  the top (radius = half the cell diagonal, clipped to the cell so 50 % covers
  exactly half the area — measured 0.500), gold autocast ring, disabled dim,
  mana cost bottom-right, build cost bottom-left (gold vs `tokens.danger`),
  MORE page (`1/2`) centred.

## Pixel measurements at the device scale

Both reference viewports resolve to `s = 1.1` (`min(w,h)/1080 * 1.1`).

| element | px |
|---|---|
| card cell (fixed, unchanged) | 90 x 90 |
| hotkey badge (plate) | 29.4 x 29.4 |
| hotkey letter | 26.4 (0.80 * s; floor 24) |
| mana/cost row | 28.6 tall, text 24 (clamped floor) |
| map-select row (6/page landscape, 12/page portrait) | 94.6 tall |
| map-select title / subtitle | 38.9 / 24 |
| map row name / players+tileset / size | 28.4 / 25.7 / 24 |
| map-select PLAY / BACK / PREV-NEXT | 275 x 121 / 143 x 121 / 121 x 121 |
| quest row / done mark | 94.6 / 49.2 |
| loading frame (landscape / portrait) | 1650 x 825 / 1005.6 x 502.8 |
| loading progress bar | 19.8 tall |
| score panel (landscape / portrait) | 1540 x 770 / 996.8 x 498.4 |

## Tests (all green)

| id | Rust test |
|---|---|
| `hud.screen.map-select` | `hud_screen_map_select_lists_38_maps_with_pagination` |
| `hud.screen.loading` | `hud_screen_loading_frame_progress_and_tips` |
| `hud.screen.quest-log` | `hud_screen_quest_log_rows_and_marks` |
| `hud.screen.score` | `hud_screen_score_columns_from_rows` |
| `hud.text.fits-plate` | `hud_text_fits_plate_keeps_120_chars_inside` |
| `hud.card.hotkey-badge` | `hud_card_hotkey_badge_is_readable_over_a_bright_icon` |
| `hud.card.empty-slot-art` | `hud_card_empty_slot_art_is_not_a_black_well` |
| `hud.card.cooldown-sweep` | `hud_card_cooldown_sweep_covers_half_the_cell` |
| `hud.card.cost-red-when-unaffordable` | `hud_card_cost_turns_red_when_unaffordable` |
| `hud.card.ability-states` | `hud_card_ability_states_draw_distinct_cues` |
| headless PNGs | `hud_screen_headless_pngs_for_every_screen` |

Regenerate:

```text
scripts/h2-cargo test -p wc3-game --lib --offline hud_screen -- --nocapture
scripts/h2-cargo test -p wc3-game --lib --offline hud_card -- --nocapture
scripts/h2-cargo test -p wc3-game --lib --offline hud_text_fits_plate -- --nocapture
scripts/h2-cargo run  -p wc3-game --example hud_layout --offline   # HUD_LAYOUT_GATE PASS
```

`preview/` holds downscaled JPEGs of the landscape frames for quick review; the
full RGBA PNGs are the evidence.
