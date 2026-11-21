---
version: alpha
name: WC3 Mobile HUD — classic fantasy stone and gold
description: Warcraft III-like strategy HUD for 805 × 360 dp at density 3.0. Stone and dark metal frames, navy wells, one gold accent, Nimrod throughout. Every colour and inset below is sampled from the shipped War3.mpq UI art, not from a web palette.
colors:
  canvas: "#05070A"
  panel: "#00091A"
  panel-raised: "#182231"
  panel-sunken: "#05070A"
  stone: "#404342"
  stone-warm: "#35221C"
  stone-shadow: "#151515"
  border: "#6B5A2E"
  border-strong: "#C8A24A"
  gold-glint: "#FFFF66"
  ink: "#EDE0C2"
  ink-muted: "#ADA07F"
  ink-disabled: "#6B6150"
  accent: "#F2C24C"
  accent-hover: "#FFD97A"
  danger: "#FF595E"
  success: "#5CE38A"
  resource-gold: "#F2C24C"
  resource-lumber: "#7AD98F"
  resource-supply: "#70B5FF"
  mana: "#5B8FD6"
  hp: "#52DB85"
typography:
  display: {fontFamily: Nimrod, fontSize: 28px, fontWeight: 700, lineHeight: 1.15}
  title: {fontFamily: Nimrod, fontSize: 22px, fontWeight: 700, lineHeight: 1.2}
  heading: {fontFamily: Nimrod, fontSize: 20px, fontWeight: 700, lineHeight: 1.2}
  body: {fontFamily: Nimrod, fontSize: 15px, fontWeight: 400, lineHeight: 1.3}
  label: {fontFamily: Nimrod, fontSize: 13px, fontWeight: 600, lineHeight: 1.2, letterSpacing: "0.02em"}
  compact: {fontFamily: Nimrod, fontSize: 11px, fontWeight: 600, lineHeight: 1.2}
  numeric: {fontFamily: Nimrod, fontSize: 15px, fontWeight: 700, lineHeight: 1.0, fontFeature: "tnum"}
rounded:
  none: 0px
  sm: 2px
  md: 4px
  lg: 8px
spacing:
  base: 4px
  xs: 4px
  sm: 6px
  md: 8px
  lg: 10px
  xl: 12px
  xxl: 16px
  xxxl: 24px
motion:
  press: {duration: 60ms, timingFunction: ease-out}
  hover: {duration: 100ms, timingFunction: ease-out}
  surface-enter: {duration: 250ms, timingFunction: cubic-bezier(0.2, 0, 0, 1)}
  surface-exit: {duration: 150ms, timingFunction: cubic-bezier(0.3, 0, 1, 1)}
  ready-pulse: {duration: 180ms, timingFunction: ease-out}
  invalid-shake: {duration: 120ms, distance: 3px}
components:
  panel.menu: {backgroundColor: "{colors.panel}", textColor: "{colors.ink}", rounded: "{rounded.none}", padding: 10px}
  panel.tooltip: {backgroundColor: "{colors.panel-raised}", textColor: "{colors.ink}", rounded: "{rounded.none}", padding: 8px}
  pill.resource: {backgroundColor: "{colors.panel-sunken}", textColor: "{colors.ink}", typography: "{typography.numeric}", height: 28px, padding: 4px 8px}
  command-button: {backgroundColor: "{colors.panel-sunken}", textColor: "{colors.ink}", size: 48px}
  command-button-hover: {backgroundColor: "{colors.panel-sunken}", textColor: "{colors.accent-hover}"}
  command-button-pressed: {backgroundColor: "{colors.canvas}", textColor: "{colors.ink}"}
  command-button-disabled: {backgroundColor: "{colors.panel-sunken}", textColor: "{colors.ink-disabled}"}
  row: {backgroundColor: "{colors.panel}", textColor: "{colors.ink}", typography: "{typography.body}", height: 52px, padding: 12px}
  tab: {backgroundColor: "{colors.panel}", textColor: "{colors.ink}", typography: "{typography.label}", height: 48px, padding: 8px}
  tooltip: {backgroundColor: "{colors.panel-raised}", textColor: "{colors.ink}", typography: "{typography.body}", padding: 8px}
hud:
  density: 3.0
  nine_slice:
    panel: {left: 64, right: 64, top: 64, bottom: 64, center: stretch}
    button: {left: 32, right: 32, top: 32, bottom: 32, center: stretch}
    tab: {left: 16, right: 16, top: 16, bottom: 16, center: stretch}
    tooltip: {left: 16, right: 16, top: 16, bottom: 16, center: stretch}
    command: {left: 16, right: 16, top: 16, bottom: 16, center: stretch}
    minimap: {left: 32, right: 32, top: 32, bottom: 32, center: stretch}
  elevation:
    flat: {border: "{colors.border}", shadow: none}
    raised: {border: "{colors.border-strong}", shadow: "0 2px 6px rgba(0,0,0,0.45)"}
    overlay: {border: "{colors.border-strong}", shadow: "0 8px 24px rgba(0,0,0,0.55)"}
  states:
    normal: {filter: none}
    hover: {filter: "brightness(1.18)"}
    pressed: {filter: "brightness(0.92)", offsetY: 1px}
    disabled: {filter: "saturation(0.6) brightness(0.7)"}
    locked: {filter: "grayscale(1) brightness(0.55)", glyph: padlock}
    selected: {outline: "2px {colors.accent}"}
  layout:
    safe_area: {left: 70, right: 40, top: 30, bottom: 32}
    bars: {top: 30, console: 152, toast: 32}
    targets: {min: 48, gap: 8}
    grid: {columns: 4, rows: 3, cell: 48, gap: 4}
---

# HUD direction — WC3 mobile, stone and gold

Source of truth for this document: `/storage/emulated/0/Download/hud.md` (§1, §3, §5, §6, §7, §9, §12) with the operator's three amendments (provenance-based sourcing; no test-freeze violations; control groups stay as a drawer). The look is taken from the shipped BLPs, not from the web/iOS catalogues; the catalogues supply process, grid discipline and the QA gates.

Target: **2414 × 1080 px = 805 × 360 dp at density 3.0**, safe insets 210/120/90/96 px = **70/40/30/32 dp** (measured on the operator's phone). Portrait: **1080 × 2414 px = 360 × 805 dp**, safe 24/24/96/64 px = 8/8/32/21 dp. The compact landscape console is the device's real mode.

## 1. What is wrong with the current screens (measured)

The current implementation is not a cosmetic failure: three composers disagree, and the glue menu can place buttons outside the screen. Numbers below are from the real paths, not from a screenshot guess.

### 1.1 Glue menu overflow (device path)

`scripts/h2-cargo run -p wc3-game --example menu_layout --offline` (2414 × 1080) prints five main-menu entries:

| entry | rect (px) | bottom (px) | state |
|---|---|---|---|
| CAMPAIGN | 695, 526, 1024, 150 | 676 | on screen |
| CUSTOM GAME | 695, 687, 1024, 150 | 837 | on screen |
| LOAD GAME | 695, 848, 1024, 150 | 998 | on screen, 9 px inside the bottom safe edge |
| OPTIONS | 695, 1009, 1024, 150 | 1159 | **off screen** |
| QUIT | 695, 1170, 1024, 150 | 1320 | **off screen** |

The 5 × 150 px stack starts at y = 526 and ends at 1320 in a 1080 px frame. The layout applied its 150 px ceiling per row but never solved the total against the available height. `design-qa/scripts/layout_audit.py` reports exactly this (rc = 1):

```
FAIL main_menu_entry_load: safe-area — rect exceeds inset L210 R120 T90 B96 px
FAIL main_menu_entry_options: off-screen — rect [695,1009,1024,150] outside 2414×1080
FAIL main_menu_entry_options: safe-area — rect exceeds inset L210 R120 T90 B96 px
FAIL main_menu_entry_quit: off-screen — rect [695,1170,1024,150] outside 2414×1080
FAIL main_menu_entry_quit: safe-area — rect exceeds inset L210 R120 T90 B96 px
FAIL …: target-gap — 3.7 dp < 8.0 dp   (×4 consecutive pairs)
audited 5 elements at 2414×1080 px @ 3.0x: 20 pass checks, 9 failures, 0 overlapping pair(s), 6 gap pair(s)
```

The 3.7 dp gap comes from the fixed 11 px slot gap, not from the design tokens.

### 1.2 Three HUD composers, three answers

| quantity | `hud_mock` (px) | `menu_layout` Game path (px) | `hud_layout` matrix (px) |
|---|---|---|---|
| console band | 456 (152 dp) | 367 (122 dp) | 384 (128 dp) |
| top strip | 90 (30 dp) | 60 (20 dp) | 58 (19 dp) |
| minimap | 336 (112 dp) | 288 (96 dp) | 336 (112 dp) |
| command tile | 95 (31.7 dp) | 96 (32 dp) | 144 (48 dp) |

This is the §1.2/§1.3 defect in numbers: the same screen has no single layout source, so the same element moves by 32 dp between paths. The new UI Core must own one `LayoutResult` per screen; `mobile_hud.rs` becomes a renderer of that result.

### 1.3 Targets below the touch rule

- Menu tabs in the current campaign mock: 6 cells of 168 × 54 px = **56 × 18 dp**; the label is drawn at ~27 px (9 dp), below the 13 dp Nimrod floor.
- Command tiles: 95–96 px = **31.7–32 dp** in two of three paths (48 dp is the rule; 3 × 48 + 2 × 4 = 152 dp exactly fits the console band).
- Toast band: 32 dp tall, but its text is fitted down to 24 px (8 dp) in the current fallback.

### 1.4 Missing art reads as placeholder

- Resource numbers are preceded by **flat 32 × 32 colour squares**; the shipped resource icons (`UI/Feedback/Resources/ResourceGold|ResourceLumber|ResourceSupply.blp`) are not in the path.
- The console is a flat black rectangle with 2 px gold borders; the four `HumanUITile01..04` textures exist but `HudView.console_tiles` is empty in the mock path.
- Menu 9-slices pass **24 px insets for every BLP part**. The menu border strip is 512 × 64 and the button strip 256 × 32; the retail `Backdrop` contract is 8 equal segments with corner = 0.048 units (86.4 px at 1080p) for the panel and 0.0125 units (22.5 px) for a button. The mismatch is what smears the corners in the current campaign mock.
- Panel aspect: 1188 × 833 px = **1.43**, while the retail EscMenu backdrop is 0.288 × 0.384 = **0.75**. The menu reads as a web card, not a Warcraft panel.
- Title band 184 px (61 dp) with a 108 px title plus a subtitle on the same band; two type scales compete inside 61 dp.

### 1.5 Contrast is not the defect (and that is worth stating)

`design-qa/scripts/contrast.py palette` on the current tokens returns rc = 0:

```
PASS ink: 14.99:1   PASS ink_muted: 7.58:1   PASS gold: 11.77:1
PASS hp_good: 11.05:1   PASS supply: 9.10:1   PASS danger: 6.39:1
PASS stroke: 3.88:1 (ui)
checked 8 pair(s)  rc=0
```

The palette is legible. The defects are geometry, targets, state art and the missing shipped textures. Fixing contrast would not fix the screen.

## 2. Composition

### 2.1 Landscape 805 × 360 dp

```
y = 0    ┌──────────────────────────────────────────────────────────┐
         │ system status (safe.top 30 dp, never drawn by the HUD)   │
y = 30   ├──────────────────────────────────────────────────────────┤
         │ top bar 30 dp: map name · upkeep · gold · lumber · supply│
y = 60   │                                                          │
         │                    playfield 178 dp                      │
y = 208  ├──────────────┬───────────────────────┬───────────────────┤
         │ portrait 80  │ unit card 376 × 128   │ minimap 112 × 112 │
         │              │ name/class/bars/stats  │ (arch rises 38 dp)│
y = 360  └──────────────┴───────────────────────┴───────────────────┘
         console band 152 dp, full-bleed; content inset 12 dp
```

Budget: `30 (safe) + 30 (top) + 178 (world) + 152 (console) = 390` — 30 dp too many. The top bar is **inside** the playfield (WC3 `ConsoleUI` top strip is 0.032 of the 0.6-unit screen = 19.2 dp at 1080p), so the world is `360 − 30 − 152 − 32 = 146 dp` below the safe top and the bar overlays its upper 20 dp. A 146 dp world strip is the honest consequence of a 48 dp command grid on a 360 dp-tall phone; do not shrink the targets to buy world back.

Horizontal: `70 (safe left) + 12 + 80 (portrait) + 12 + 376 (card) + 12 + 112 (minimap) + 40 (safe right) = 714 dp`, leaving 91 dp of slack. Spend it on the card (up to 440 dp), never on more permanent panels.

Top bar: 30 dp tall; resource pills 28 dp high, 20 dp icon, 15 dp numeral, 6 dp gap, right-aligned inside the safe frame. Upkeep is one 13 dp line to the left of the pills; the map name is 22 dp at the left edge.

The command grid and control groups are **drawers**, not permanent trays:
- closed: two 48 dp buttons (ARMY, CMD) at the right edge of the console;
- open: a 232 × 176 dp drawer above the console's right corner, 4 × 3 cells of 48 dp with 8 dp gaps (4 × 48 + 3 × 8 + 2 × 8 = 232; 3 × 48 + 2 × 8 + 16 = 176);
- control groups: the same drawer pattern, 5 × 2 of 48 dp, hidden by default (operator amendment §3.4).

### 2.2 Portrait 360 × 805 dp

Safe frame 344 × 752 dp. Same tokens, stacked console:
- top bar 30 dp;
- playfield 432 dp;
- console 290 dp: unit card full width × 120, minimap 112 at left, command drawer below (4 × 3 × 48 = 152);
- menus: panel 328 × 736; tabs 2 × 3 (two rows of 48); rows 6 × 50 + 5 × 8 = 340; nav 48; title 32; total 548 dp inside 716 dp content.

### 2.3 Menu geometry (landscape, the failing screen)

Frame 705 × 298 dp (safe). Panel `660 × 282` for list screens, `440 × 282` for action screens (main/pause/options), centred. Content inset 10 dp.

```
title row 32 dp   CAMPAIGN                                HUMAN 1/3
gap 8
tabs 48 dp        [PROLOGUE][HUMAN][UNDEAD][ORC][NIGHT ELF][BONUS]
gap 8
rows 2 × 50 dp    EXODUS OF THE HORDE                 CHAPTER 1
                  THE LONG MARCH                      LOCKED  🔒
gap 8
nav 48 dp         [PREV] [NEXT] [START] [BACK]
```

Height: `32 + 8 + 48 + 8 + 108 + 8 + 48 = 260 dp` ≤ 262 dp content. Main menu drops the tab row and uses **2 columns × 3 rows of 50 dp** for its six entries (`3 × 50 + 2 × 8 = 166 dp`). Pause uses 2 × 2 of 50 dp. Every target is ≥ 48 dp and every gap is 8 dp (6 dp is the absolute floor and must be justified per screen; the campaign page does not need it).

Rows paginate: capacity is solved before placement, `visible_rows` is published, page label is `HUMAN 1/3`, PREV/NEXT page. Six chapters on a 360 dp-tall phone cannot be six visible rows and 48 dp targets at the same time; pretending otherwise is the current overflow.

## 3. Palette (sampled from the shipped art)

All values are bytes read from the decoded BLPs. `avg` = average of opaque pixels in the named region.

| role | hex | source | how measured |
|---|---|---|---|
| canvas | `#05070A` | world gap; no art | chosen below the darkest panel, never carries text |
| panel (menu) | `#00091A` | `UI/Widgets/EscMenu/Human/human-options-menu-background.blp` | centre pixel `(0,14,33)`, avg `#00091A`, alpha 191/255 |
| panel raised (tooltip) | `#182231` | `UI/Widgets/ToolTips/Human/human-tooltip-background.blp` | flat 64×64 fill `(24,34,49)`, alpha 195/255 |
| panel sunken (wells) | `#05070A` | console tile wells `HumanUITile01/02/03` | interior pixels are `(0,0,0,0)` over the world; the HUD fills them with `#05070A` |
| stone | `#404342` | `HumanUITile02.blp` top beam `x200..456 y236..300` | avg `(64,67,66)` |
| stone warm | `#35221C` | `HumanUITile01.blp` left stone `x0..120 y300..500` | avg `(53,34,28)` |
| stone shadow | `#151515` | `HumanUITile03.blp` card frame `x200..512 y232..300` | avg `(21,21,21)` |
| border | `#6B5A2E` | console button strip `human-console-button-up.blp` | avg `(44,34,9)` lifted to the visible gold line |
| border strong | `#C8A24A` | menu border gold | mid of the `#FFFF66`…`#ECB715` range |
| gold glint | `#FFFF66` | `human-options-menu-border.blp` | brightest opaque pixels `(255,255,102)` |
| gold art | `#ECB715` | `human-tooltip-border.blp` | avg `(236,183,21)` |
| ink | `#EDE0C2` | current `Tokens::default` `text` | `(0.93,0.88,0.76)`; 14.99:1 on panel |
| ink muted | `#ADA07F` | current `Tokens::default` `text_muted` | 7.58:1 on panel |
| accent | `#F2C24C` | current `Tokens::default` `gold` | 11.77:1 on panel; tooltip cost gold is `#FED312` in `ToolTip.fdf` |
| accent hover | `#FFD97A` | `#FFFF66` glint mixed 50 % with `#F2C24C` | one step brighter, same hue |
| danger | `#FF595E` | current `danger` | 6.39:1 on panel |
| success | `#5CE38A` | current `success` | 11:1 class |
| hp | `#52DB85` | `UI/Feedback/HPBarConsole/human-healthbar-fill.blp` | the BLP is a grayscale fill (avg `#929292`); the green is the HUD tint over it |
| mana | `#5B8FD6` | `UI/Feedback/ManaBarConsole/human-manabar-fill.blp` | art fill avg `#3176A7`; `#5B8FD6` is the brighter HUD tint |
| resource icons | BLP art | `UI/Feedback/Resources/ResourceGold|Lumber|Supply|Upkeep|ManaStone.blp` | 32 × 32 icon atlas; the number stays `ink`, the icon carries the identity |

Rules: a status is never colour alone (icon + number + shape); the fill never stands in for the icon; gold is reserved for the one primary action per screen and for selection.

## 4. Typography — Nimrod at 3×

Face: `Fonts/NIM_____.ttf` (Nimrod MT) from the operator's `War3.mpq`; the pack's `primary_serif_cyrillic.ttf` is the styled fallback. Cyrillic coverage is required (`Орда01`, `Альянс04` are already in the mocks).

| role | dp | px @3× | use | line-height |
|---|---|---|---|---|
| display | 28 | 84 | glue screen title / campaign header | 1.15 |
| title | 22 | 66 | menu title, panel heading | 1.20 |
| heading | 20 | 60 | unit name, dialog title | 1.20 |
| body | 15 | 45 | row labels, tooltip body | 1.30 |
| label | 13 | 39 | tabs, tags, costs, columns | 1.20 |
| compact | 11 | 33 | hotkey, page counter, micro-labels | 1.20 |
| numeric | 15 | 45 | gold/lumber/supply, HP/mana, timers — tabular figures | 1.00 |

Floors: **13 dp** for labels, **15 dp** for body, **22 dp** for titles. A 48 dp command tile carries an icon + one 13 dp hotkey overlay, never a 24 px name (the retail contract, and the reason the old tiles overflowed). All numbers use tabular figures so a value change never moves the layout.

Measured: 13 dp = 39 px, 15 dp = 45 px, 22 dp = 66 px, 48 dp = 144 px, 50 dp = 150 px. The old tab label was ~27 px = 9 dp — below the floor; the old command tile carried 24 px text = 8 dp.

## 5. Spacing and shape

Base ladder: **4 dp** (`4, 6, 8, 10, 12, 16, 24`). Use 8 dp between rows/tabs, 8 dp between groups, 10 dp panel inset, 12 dp console content inset, 16 dp section padding, 24 dp screen edge on desktop.

Corners are art, not radii: the 9-slice corner is the shape. No CSS-style rounded rectangles on top of the stone frames.

## 6. 9-slice table (the machine contract)

Insets are **source pixels**; the renderer scales them by the draw factor. All assets are in `assets/hud/` with `manifest.json`.

| id | source | insets L/T/R/B | dp at 3× | centre | min render | art border |
|---|---|---|---|---|---|---|
| `panel.menu` | 192×192 | 64 / 64 / 64 / 64 | 21.3 | stretch | 180×120 dp | 20 px stone + 3 px gold inner line |
| `button.normal` (4 states) | 96×96 | 32 / 32 / 32 / 32 | 10.7 | stretch | 48×48 dp | 2 px cyan edge + inner bevel |
| `tab` / `tab.active` | 48×48 | 16 / 16 / 16 / 16 | 5.3 | stretch | 64×48 dp | 3 px gold/metal |
| `tooltip` | 48×48 | 16 / 16 / 16 / 16 | 5.3 | stretch | 120×48 dp | 2 px gold line on black |
| `command.normal` (3 states) | 48×48 | 16 / 16 / 16 / 16 | 5.3 | stretch | 48×48 dp | 3 px metal frame |
| `minimap.frame` | 96×96 | 32 / 32 / 32 / 32 | 10.7 | stretch | 112×112 dp | stone beam + 2 px gold inner line |
| `minimap.arch` | 232×312 | — (fixed) | — | — | 112×150 dp | retail arch, stitched from tile01/02 |
| `portrait.frame` | 300×284 | — (fixed) | — | — | 80×76 dp | console portrait cutout |

Validation rules (line 555 owns the validator): positive sizes; `l + r < w` and `t + b < h`; centre has no border/ornament; corners identical at min and max; border survives as an integer ≥ 1 px; no mipmaps on straight-alpha panels; no UV outside the source rect; no duplicate ids.

Retail anchors (from `/storage/emulated/0/Download/hud.md` §9 and the shipped FDF):

- panel corner `0.048` units → 86.4 px at 1080p = **28.8 dp**; our 64 px source at 1× = **21.3 dp** and at 1.4× = **29.9 dp** — the draw scale lands on the retail corner.
- button corner `0.0125` units → 22.5 px = **7.5 dp**; our 32 px source at 1.1× = 35 px = **11.7 dp** (finger-first, deliberately thicker).
- tooltip corner `0.008` units → 14.4 px = **4.8 dp**; our 16 px source at 1× = **5.3 dp**.
- console `0.176` / `0.032` of the 0.6-unit screen → **317 / 57.6 px** at 1080p = 105.6 / 19.2 dp. The mobile band is 152 dp because three 48 dp command rows + two 4 dp gaps = 152 dp; the top strip is 20 dp, within 0.8 dp of retail.

## 7. Elevation and states

Three levels only: flat (docked), raised (tooltip/hover), overlay (menu/pause). Depth is tonal steps + borders first; shadows only reinforce.

| state | contract | implementation |
|---|---|---|
| normal | source art | as authored |
| hover | +18 % brightness, border → `accent-hover` | brightness filter, no new art |
| pressed | down atlas, 1 px down | pressed atlas + offset |
| disabled | 60 % saturation, 70 % brightness, `ink-disabled` | disabled atlas (already desaturated at build) |
| locked | disabled atlas + padlock glyph + 24 % veil | glyph drawn as two rects; never a colour square |
| selected | 2 px accent inner line | accent stroke inside the plate |
| focus | 2 px accent ring at +2 px | ring, not a fill |
| cooldown | radial sweep + numeric seconds | mask + numeric token |
| ready | one 180 ms brighten | motion token, no loop |
| error | danger border + shake ≤ 3 px once | motion token |

No hue rotation on skinned art; no state changes the layout box; every state must be reachable in the mock.

## 8. Motion

Press feedback ≤ 60 ms; hover 100 ms ease-out; surface enter 250 ms `cubic-bezier(0.2,0,0,1)`; exit 150 ms; ready pulse 180 ms; invalid shake 3 px once. Nothing loops while the player reads. Reduced motion rests everything and keeps all content visible.

## 9. HUD regions and the single model

The UI Core (line 553) owns one `UiTree → LayoutResult` per screen. `mobile_hud.rs` may keep the draw code, but every rect it draws must come from that result; hit-testing uses the same element list. The current `hud_layout` matrix, `hud_mock` and `Game::menu_layout` must collapse to one source — otherwise the 456/367/384 px console drift will return.

Region responsibilities:

| region | content | geometry source |
|---|---|---|
| top bar | map name, upkeep, gold/lumber/supply pills, clock | layout solver, safe frame |
| playfield | world; top bar and console overlay it | safe frame minus console |
| console | portrait, unit card, minimap, drawers | 152 dp band, 12 dp inset |
| unit card | portrait 80, name 20 dp, class 13 dp, HP/mana 8 dp bars, stats 13 dp | card rect |
| command drawer | 4 × 3 × 48 dp cells, hotkey corner 12 dp | drawer rect |
| group drawer | 5 × 2 × 48 dp, hidden by default | drawer rect |
| minimap | 112 dp square + 112 × 150 arch, camera rect, dots | minimap rect |
| tooltips | title/body/cost/hotkey/reason, clamped to safe frame | anchor + solver |
| toasts | 32 dp rows, 2–3 s, danger/accent tones | toast lane above console |

### 9.1 Selection states (five)

| state | card composition | commands |
|---|---|---|
| nothing | card collapsed to a 48 dp "no selection" plate; portrait well empty | global commands only (menu, idle worker, hero) |
| single unit | portrait 80, name 22, class 13, HP 8 dp bar, mana if present, armor/damage 13 | that unit's real command grid |
| hero | single unit + level badge, mana, 2 × 3 inventory slots | hero abilities + inventory + move/attack |
| building | portrait/building icon, name, HP, production queue 3 × 48 | train/research/rally/cancel, only if implemented |
| multiple | primary portrait + count chip "×6", aggregate HP bar, up to 6 mini portraits | shared commands only; no per-unit ability unless all share it |

The card never grows below 48 dp touch height and never shows an empty inventory panel for a unit without inventory.

### 9.2 Command and tooltip content

A command carries `CommandId`, icon, label, hotkey, enabled, disabled_reason, cooldown, resource_cost, target_type, action. Visual states: normal, hover, pressed, selected, disabled, cooldown, unavailable, waiting-for-target. A button that cannot execute is never drawn as enabled; the tooltip states the reason with the number ("Requires Barracks", "Not enough lumber (40 needed)").

Tooltip order: name 20 dp, description 15 dp, cost 13 dp gold, requirement/reason 13 dp muted, hotkey 11 dp compact. It is clamped to the safe frame, never covers the command that spawned it, and does not flicker on touch: a tap opens it, a second tap on the same cell executes.

## 10. Menu screens

One typed model (`ScreenId`, `ScreenState`, `NavigationAction`, `BackAction`), one layout, one hit list. No screen may invent its own geometry.

- **Main**: 440 dp panel, 2 × 3 entries of 50 dp (Continue / Single Player / Load / Settings / Exit — only real actions), title 22 dp, subtitle 11 dp.
- **Campaign**: 660 dp panel, 6 tabs of 48 dp in one row, 2 chapter rows of 50 dp per page, PREV/NEXT/START/BACK 48 dp, page label 13 dp.
- **Custom game**: 660 dp panel, 6 tabs (Melee/Race/Map/Difficulty), 3 map rows of 50 dp, START/BACK.
- **Options**: 440 dp panel, 2 × 3 category entries of 50 dp (Graphics/Audio/Controls/Gameplay/Interface/Back), values shown in the row tag.
- **Pause**: 440 dp panel, 2 × 2 entries of 50 dp (Resume/Settings/Save/Quit), dark veil 0.55, simulation actually paused.
- **Loading**: title + 12 dp progress bar + map name; no buttons except Cancel if the load is cancellable.
- **Victory/Defeat**: title, 2 entries (Continue/Score, Retry/Quit), 50 dp.

Back/Escape returns to the logical previous screen, not always to Main. A tap is one command; double taps are debounced by the model, not by the renderer.

## 11. Assets delivered

`assets/hud/manifest.json` (format 2) lists every file with id, purpose, size, insets, states, atlas, alpha, colour space, scale and provenance (`source`, `sha256`, `transform`, `label`). Extracted assets are labelled `extracted`; composites assembled from several crops are labelled `procedural reference v1`. Nothing generated is presented as final hand-drawn art.

Set:
- `panels/panel_menu_9s.png` — 192×192, 64 px insets, stone/gold frame + `#00091A` interior.
- `buttons/button_{normal,hover,pressed,disabled}_9s.png` — 96×96, 32 px insets, four authored states.
- `tabs/tab_9s.png`, `tabs/tab_active_9s.png` — 48×48, 16 px insets.
- `tooltip/tooltip_9s.png` — 48×48, 16 px insets, gold line + `#182231`.
- `command/command_{normal,pressed,disabled}_9s.png` — 48×48, 16 px insets.
- `minimap/minimap_frame_9s.png` (96×96, 32 px) and `minimap/minimap_arch.png` (232×312, fixed).
- `portraits/portrait_frame.png` — 300×284 fixed cutout.
- `resources/resource_{gold,lumber,supply,upkeep,mana}_96.png` — 96×96 icons trimmed to alpha, rendered at 24 dp.

Provenance: see `analysis/design/ASSETS.md` and the manifest; every source BLP sha256 is recorded. Web donors are not quoted for look — only for process and QA.

## 12. Acceptance

- **G1 geometry**: `layout_audit.py` rc = 0 at 2414 × 1080 and 1080 × 2414; no off-screen rect, no unintended overlap, no element outside its parent panel.
- **G2 text fit**: every declared string measured with Nimrod; no label below 13 dp; no clipped descender at 2× zoom.
- **G3 contrast**: `contrast.py palette` rc = 0 for ink/muted/accent/danger/borders (current tokens already pass; keep them).
- **G4 targets**: every interactive element ≥ 48 dp; gap ≥ 8 dp.
- **G5 states**: normal/hover/pressed/disabled/locked/selected present in the mock; loading/empty/error exist where the surface can be empty or fail.
- **Retail proportions**: panel corner lands on 28.8 dp ± 4; console 152 dp exactly; top strip 20 dp ± 1; command cell 48 dp; minimap 112 dp; portrait 80 dp.

### Measured in this lane

```
# current menu geometry (analysis/design/qa/current-menu-2414x1080.json)
audited 5 elements at 2414×1080 px @ 3.0x: 20 pass checks, 9 failures, 0 overlapping pair(s), 6 gap pair(s)
rc=1

# direction menu geometry (analysis/design/qa/direction-menu-2414x1080.json)
audited 14 elements at 2414×1080 px @ 3.0x: 75 pass checks, 0 failures, 0 overlapping pair(s), 66 gap pair(s)
rc=0

# current tokens (8 pairs) and direction palette (12 pairs)
rc=0 both; ink 15.22:1 on panel, muted 7.70:1, accent 11.95:1, border/stone 4.15:1
```

The direction is done when the mock matrix at both orientations passes G1–G5 and a reviewer can point to the shipped BLP behind every colour and every frame.
