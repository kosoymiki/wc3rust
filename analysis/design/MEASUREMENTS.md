# Measurements — before vs direction (ledger 544)

Access date 2026-11-18. Device target 2414 × 1080 px = 805 × 360 dp at density 3.0, safe 210/120/90/96 px = 70/40/30/32 dp. Portrait 1080 × 2414 px = 360 × 805 dp.

Method: every number below is printed by a real path or computed by a design-qa script. Nothing is eyeballed from a screenshot.

- current HUD/menu mocks: `scripts/h2-cargo run -p wc3-game --example hud_mock --offline`, `... --example menu_layout --offline`;
- direction geometry: `analysis/design/qa/direction-menu-2414x1080.json` → `design-qa/scripts/layout_audit.py`;
- palettes: `design-qa/scripts/contrast.py palette`;
- direction shots: `python3 analysis/design/tools/render_direction.py`.

## Screens

| shot | what it is | label |
|---|---|---|
| `shots/before-hud-2414x1080.png` | current runtime HUD through `hud_mock` (flat chrome path) | runtime, before |
| `shots/before-menu-campaign-2414x1080.png` | current runtime campaign menu through `hud_mock` | runtime, before |
| `shots/before-menu-main-2414x1080.png` | current runtime main menu | runtime, before |
| `shots/before-hud-1080x2414.png` | current runtime portrait HUD (icon path) | runtime, before |
| `shots/direction-hud-2414x1080.png` | direction composition from `assets/hud` | `procedural reference v1` (design mock, not runtime) |
| `shots/direction-hud-tooltip-2414x1080.png` | direction HUD + command drawer + tooltip | `procedural reference v1` |
| `shots/direction-hud-1080x2414.png` | direction portrait HUD (stacked console) | `procedural reference v1` |
| `shots/direction-menu-campaign-2414x1080.png` | direction campaign page (2 rows/page) | `procedural reference v1` |
| `shots/direction-menu-campaign-1080x2414.png` | direction portrait campaign (6 rows + preview) | `procedural reference v1` |
| `shots/direction-menu-main-2414x1080.png` | direction main menu (2 × 3 entries) | `procedural reference v1` |

The direction shots are design evidence, not hand-drawn final art and not a runtime proof. The runtime proof for line 553 is `cargo run --example hud_mock` after the UI Core lands.

## Geometry — before → direction

| # | item | before (measured) | direction target | evidence |
|---|---|---|---|---|
| 1 | main-menu entries | 5 × 1024 × 150 px at y 526/687/848/1009/1170; OPTIONS + QUIT **off screen** (bottom 1159/1320 > 1080) | 6 entries, 2 × 3 cells of 150 px, all inside 1980 × 870 panel | `menu_layout` output; `layout_audit` rc=1 |
| 2 | main-menu gap | 11 px = **3.7 dp** | 24 px = **8 dp** | `layout_audit`: 4 × target-gap failures |
| 3 | menu row / tab height | campaign mock tabs/rows 54 px = **18 dp** | 150 px = **50 dp** rows; 144 px = **48 dp** tabs | `hud_mock` geometry; direction JSON |
| 4 | menu title band | 184 px = 61.4 dp | 96 px = **32 dp** | `hud_mock` `MENU_MOCK title_band=184`; direction |
| 5 | menu panel aspect | 1188 × 833 = **1.43** | list 1980 × 870 = 2.28; main 1320 × 870 = 1.52 (retail EscMenu is 0.75; mobile list needs width) | `MENU_MOCK panel=1188x833`; direction |
| 6 | console band | **456 / 367 / 384 px** in mock / Game / matrix = 152 / 122 / 128 dp | one source: **456 px = 152 dp** (3 × 48 + 2 × 4) | `HUD_MOCK console=0,624,2414,456`; `GAME_HUD_RECTS console=0,713,2414,367`; matrix `console_px=384` |
| 7 | top strip | 90 / 60 / 58 px = 30 / 20 / 19.3 dp | **60 px = 20 dp** (retail 0.032 = 19.2 dp) | same three paths |
| 8 | command cell | **95 / 96 px = 31.7 / 32 dp** in mock/Game; 144 px only in matrix | **144 px = 48 dp**, gap 12 px = 4 dp | `GAME_HUD_RECTS tile=96.0`; design |
| 9 | minimap | 336 / 288 px = 112 / 96 dp | **336 px = 112 dp** + 112 × 150 dp arch | `HUD_MOCK`; `GAME_HUD_RECTS minimap=16,769,288,288` |
| 10 | portrait | 240 px = 80 dp | **240 px = 80 dp** (retail 50 × 68; enlarged for touch) | `HUD_MOCK portrait=240`; FDF 0.0835 × 114 |
| 11 | 9-slice insets | 24 px for every BLP part (border strip 512 × 64 / 256 × 32) | panel 64, button 32, tab/tooltip/command 16, minimap 32 | `hud_mock ui_part()`; `assets/hud/manifest.json` |
| 12 | panel corner | 24 px = 8 dp | 64 px source → 21.3 dp at 1×, 29.9 dp at 1.4× (retail 28.8 dp) | manifest; FDF `BackdropCornerSize 0.048` |
| 13 | tab label | ~27 px = **9 dp** | 39 px = **13 dp** (Nimrod floor) | `hud_mock` fit; direction §4 |
| 13b | row label | ~22.7 px = **7.6 dp** (`54 × 0.42 / 30`) | 45 px = **15 dp** | `hud_mock` geometry; direction §4 |
| 14 | menu title | 108 px = 36 dp | 66 px = **22 dp** | `MENU_MOCK`; direction §4 |
| 15 | locked row | flat fill `[0.05,0.045,0.035]` / dim plate, no glyph | disabled atlas + padlock glyph + 24 % veil | current `draw_menu_entry`; direction §7 |
| 16 | resource icon | 32 × 32 flat colour square | 96 × 96 shipped BLP icon at 24 dp | before HUD shot; `assets/hud/resources/*` |
| 17 | text contrast | ink 14.99:1, muted 7.58:1, gold 11.77:1, stroke 3.88:1 — **passes** | same tokens; direction 12 pairs pass | `contrast.py` rc=0 both |
| 18 | state coverage | normal/pressed/disabled only on some groups; tabs have no hover/disabled | normal/hover/pressed/disabled/locked/selected for every plate | direction §7 |

## design-qa output — current menu (rc = 1)

```
$ python3 ~/.config/opencode/skills/design-qa/scripts/layout_audit.py \
    analysis/design/qa/current-menu-2414x1080.json
FAIL main_menu_entry_load: safe-area — rect exceeds inset L210 R120 T90 B96 px
FAIL main_menu_entry_options: off-screen — rect [695,1009,1024,150] outside 2414.0×1080.0
FAIL main_menu_entry_options: safe-area — rect exceeds inset L210 R120 T90 B96 px
FAIL main_menu_entry_quit: off-screen — rect [695,1170,1024,150] outside 2414.0×1080.0
FAIL main_menu_entry_quit: safe-area — rect exceeds inset L210 R120 T90 B96 px
FAIL main_menu_entry_continue ↔ main_menu_entry_custom: target-gap — 3.7 dp < 8.0 dp
FAIL main_menu_entry_custom ↔ main_menu_entry_load: target-gap — 3.7 dp < 8.0 dp
FAIL main_menu_entry_load ↔ main_menu_entry_options: target-gap — 3.7 dp < 8.0 dp
FAIL main_menu_entry_options ↔ main_menu_entry_quit: target-gap — 3.7 dp < 8.0 dp
audited 5 elements at 2414×1080 px @ 3.0x: 20 pass checks, 9 failures, 0 overlapping pair(s), 6 gap pair(s)
rc=1
```

## design-qa output — direction menu (rc = 0)

```
$ python3 ~/.config/opencode/skills/design-qa/scripts/layout_audit.py \
    analysis/design/qa/direction-menu-2414x1080.json
audited 14 elements at 2414×1080 px @ 3.0x: 75 pass checks, 0 failures, 0 overlapping pair(s), 66 gap pair(s)
rc=0
```

## design-qa output — palettes (rc = 0 both)

Current tokens (8 pairs): ink 14.99:1, muted 7.58:1, gold 11.77:1, hp 11.05:1, supply 9.10:1, danger 6.39:1, stroke 3.88:1 → `rc=0`.

Direction palette (12 pairs): ink/panel 15.22:1, muted/panel 7.70:1, accent/panel 11.95:1, ink/tooltip 12.23:1, muted/tooltip 6.18:1, accent/tooltip 9.60:1, danger/panel 6.49:1, hp 11.22:1, mana 6.02:1, border/panel 8.28:1, border/stone 4.15:1 → `rc=0`.

Conclusion: contrast was already fine. The defects are geometry, one-source-of-truth, missing shipped art, and under-sized targets. The direction fixes those without touching the palette's contrast.

## What line 553 must wire (not my lane)

1. One layout result per screen (UI Core). The three console heights must collapse to the 456 px source; the 367/384 px paths become stale.
2. Safe area and density are inputs, not `None` defaults (hud.md §1.4/§7.4).
3. Menu model: one typed screen model; `MenuView::for_screen` + `Game::sync_menu` + `menu_view` collapse to one source (hud.md §1.2).
4. Load `assets/hud/manifest.json` through the pack loader; no silent FDF fallback for a missing themed part (hud.md §1.5/§9.3).
5. Theme tokens from `HUD-DIRECTION.md` YAML; no per-widget colours.
6. Command grid 48 dp and the group drawer hidden by default (operator amendment §3.4).
7. The runtime proof after wiring is `cargo run --example hud_mock` at 2414×1080 and 1080×2414 + `layout_audit.py` rc=0 on the published layout dump.
