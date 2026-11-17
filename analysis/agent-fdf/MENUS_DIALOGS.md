# FDF FULL GOAL — menus & dialogs from shipped FDF/war3skins art (ledger 353, 2026-11-14)

Owner: `mobile_hud.rs` menu/dialog code, `fdf_ui.rs`, `analysis/agent-fdf/**`.

## Screen → shipped data → elements → PNG

| Screen | Shipped FDF root / data | Shipped art (`war3skins`/glue) | Elements drawn | PNG (1920×1080) |
|---|---|---|---|---|
| Main | `Glue/MainMenu.fdf` `MainMenuFrame` labels | `EscMenuBackground`, `EscMenuBorder`, `GlueScreen-Button1-BackdropBorder/Background(+Down/Disabled)`, `GlueScreen-Button-KeyboardHighlight` | 9-slice panel+border, title band, title 1.33×, 3 buttons (normal/hover/pressed/disabled states), labels+hotkeys | `mocks/menu_main.png` |
| Single Player | `Glue/SinglePlayerMenu.fdf` `SinglePlayerMenu` | same chrome | panel, band, 3 buttons, disabled state | `mocks/menu_singleplayer.png` |
| Skirmish (race/opponent/map/difficulty) | `Glue/Skirmish.fdf` `SkirmishPopupMenuTemplate` labels | same chrome | panel, band, 6 buttons incl. RACE/OPPONENT/MAP/DIFFICULTY rows | `mocks/menu_skirmish.png` |
| Map select | `Glue/MapListBox.fdf` `MapListBox` | same chrome | panel, band, 2 buttons | `mocks/menu_mapselect.png` |
| Pause / options | `UI/EscMenuMainPanel.fdf` `EscMenuMainPanel` | `EscMenuBackground/Border` + glue buttons | panel, band, 4 buttons (SAVE disabled), hover/pressed states | `mocks/menu_pauseoptions.png` |
| Loading | `MenuScreen::Loading` + shipped chrome | panel/border + loading frame slot | panel, title band, progress bar, status text | `mocks/menu_loading.png` |
| Victory | `Glue/ScoreScreen.fdf` `ScoreScreen` labels | same chrome | panel, band, 2 buttons | `mocks/menu_victory.png` |
| Defeat | same as Victory (DEFEAT copy) | same chrome | panel, band, 2 buttons | `mocks/menu_defeat.png` |
| Quest log | `UI/QuestDialog.fdf` `QuestDialog` | `EscMenuBackground/Border` + glue button | panel, title band, title, 4 body lines, footer, CLOSE button | `mocks/dialog_questlog.png` |
| Hero skills | `UI/InfoPanelTemplates.fdf` / hero panel data | same chrome | panel, title, 4 skill lines, footer, CLOSE | `mocks/dialog_heroskills.png` |
| Building rally/training | `UI/SimpleInfoPanel.fdf` + `InfoPanelTemplates.fdf` | same chrome | panel, title (BARRACKS), 3 training/rally lines, footer, CLOSE | `mocks/dialog_buildingpanel.png` |

Art resolution is data-driven: `fdf_ui::MenuArtPaths::resolve(&SkinTable)` maps `EscMenuBackground`, `EscMenuBorder`, `EscMenuButtonBackground`, `EscMenuButtonMouseOverHighlight`, `EscMenuButtonPushedBackground`, `EscMenuButtonDisabledPushedBackground`; the mock test loads the same BLP paths the FDF/glue references use.

## Code changes

- `crates/wc3-game/src/mobile_hud.rs`
  - `MenuScreen` now covers Main, SinglePlayer, Skirmish, MapSelect, PauseOptions, Loading, Victory, Defeat (`for_screen` gives titles, subtitles and label sets).
  - `MenuSkin` extended with `panel_border`, `title_band`, `button_hover` (plus existing panel/button states/loading frame) — all are `war3skins` art handles.
  - `MenuButtonState { Normal, Hover, Pressed, Disabled }`; `MenuView.hover`/`pressed`; `draw_menu` picks the state texture, draws the hover sheen, 9-slices panel/border/title band, and uses title 1.33× / labels 0.90× / subtitle 0.80× (≥0.8 readability floor).
  - `DialogScreen { QuestLog, HeroSkills, BuildingPanel }`, `DialogView`, `layout_dialog`, `draw_dialog` — same shipped chrome, body lines 0.82×, footer/CLOSE 0.80–0.86×.
  - New headless test `ui_menus_shipped_art_mock_pngs` renders all 11 screens through the real draw path into `analysis/agent-fdf/mocks/`, asserting: panel+border art present, all four button states resolve, no button overlap, title band ≥56 px, PNG >20 KB, and >60 sampled colours with ≥4 textured draws (no flat rectangles).
- `crates/wc3-game/src/fdf_ui.rs`: `MenuArtPaths` (shipped key→path resolution, `resolved()` count) reused by consumers; coverage APIs unchanged.

## Fonts (no overflow)

- `fdf_ui::glyph_coverage` on the shipped `FRIZQT__.TTF`: Latin 95/95, Cyrillic 94/256 range (all Russian letters), **missing 0** over 1782 UI strings (Round FULL GOAL evidence).
- Menu type sizes: title 1.33×s, buttons 0.90×s, subtitles 0.80×s; at 1920×1080 s=1.0 and at 2560×1440 s=1.333, so the 0.8 floor holds at both review resolutions; labels/subtitles are measured against button/panel width in the mock test (no clipping observed).

## Gate evidence

| Gate | Command | rc | Result |
|---|---|---|---|
| ui_fdf | `cargo test -p wc3-game --test ui_fdf` | **0** | 14 passed / 0 failed (`/tmp/final-gate1d.log`) |
| mocks | `cargo test -p wc3-game --lib ui_menus_shipped_art_mock_pngs -- --nocapture` | **0** | 1 passed, 11 PNGs written (`MENU_MOCK … textured_draws=4..7, colors=80..205`) |
| hud | `cargo test -p wc3-game --lib 'hud::'` | **0** (observed) | `hud_rc=0` at attempts 2–5 in `/tmp/final-gate2b.log`; current tree fails to *compile* due to external `wc3-render` API churn: `render.rs:1211 E0063 missing field mesh_color` (render owner) — not menu code |

Mock artifacts: `analysis/agent-fdf/mocks/*.png` (11 files, 1920×1080 RGBA).

## Conflicts / next steps (other owners)

- `game.rs` still builds `MenuSkin` from the old `redraw-v0.1` pack and maps only 3 screens; wiring the shipped art (texture indices from `MenuArtPaths`) and the new screens is a game/HUD-owner step (game.rs/hud.rs are read-only for this layer).
- `assets.rs`/`render.rs` were mid-edit during the run (`assets` scope error, `mesh_color` field); gates were re-run until the tree compiled.
