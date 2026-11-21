# Mobile HUD design specification — wc3rust RTS

Status: reference-driven design, 2026-11-12. Implementation targets: `crates/wc3-game/src/{style.rs,hud.rs,mobile_hud.rs}`. Owner paths only; `game.rs` wiring is specified in `INTEGRATION.md`.

Scope: phone portrait and landscape **< 1600 px wide**. For landscape **≥ 1600 px** the governing document is `CONSOLE_UI_SPEC.md`; token precedence between the two is defined in `UI_MERGE_MATRIX.md` §3.

Measured evidence: `REFERENCES.md`, `references/measurements.json`, `references/reference_sheet.png`, `references/comparison_sheet.png`, rejected shot `analysis/user-shot-4.png`.

## 1. Principles

1. **Edge-hugging, centre-clear.** Every persistent control lives in the safe-area corners or edges; the battlefield centre stays unobstructed (Honor of Kings, Art of War 3, Diablo Immortal all follow this).
2. **Thumb-first.** Joystick bottom-left, command cluster bottom-right, context tray directly above the ring. No control in the middle third of the screen.
3. **Safe by construction.** Layout receives an explicit `SafeArea`; every rect is clamped inside it. Top text never assumes y=0.
4. **Mobile RTS, not 2002 WC3.** Glass chrome, rounded cards, 14-24 px type, 132 px primary buttons, WC3 BLP art only as *content* inside modern chrome.
5. **One hierarchy per zone.** Top = resources/minimap/chips; bottom-left = movement; bottom-right = commands; bottom-centre-left = selection; centre = toast/phase only.

## 2. Design tokens

### 2.1 Colour (linear-independent sRGB values handed to the GPU; alpha is premultiplied by the UI shader)

| Token | RGBA | Use |
|---|---|---|
| `panel` | `0.043, 0.071, 0.098, 0.88` | primary glass card |
| `panel_hi` | `0.086, 0.129, 0.173, 0.94` | inner pill, portrait, button face |
| `panel_pressed` | `0.145, 0.255, 0.392, 0.96` | pressed button face |
| `stroke` | `0.561, 0.706, 0.851, 0.24` | 1.5 px hairline |
| `stroke_hot` | `0.400, 0.761, 1.000, 0.90` | primary ring / focus |
| `text` | `0.949, 0.969, 1.000, 1.0` | names, values |
| `text_muted` | `0.663, 0.725, 0.792, 0.92` | class, labels, FPS |
| `gold` | `0.949, 0.757, 0.306, 1.0` | gold, primary accent values |
| `lumber` | `0.482, 0.847, 0.561, 1.0` | lumber |
| `supply` | `0.435, 0.710, 1.000, 1.0` | supply/army |
| `hp_good` | `0.357, 0.890, 0.541, 1.0` | HP > 55% |
| `hp_warn` | `0.949, 0.757, 0.306, 1.0` | HP 25-55% |
| `hp_low` | `1.000, 0.420, 0.400, 1.0` | HP < 25% |
| `danger` | `1.000, 0.353, 0.373, 1.0` | enemy, error toast |
| `disabled` | `0.400, 0.443, 0.490, 0.45` | disabled face/text |
| `shadow` | `0.000, 0.000, 0.000, 0.60` | text/icon outline |

Contrast on `panel` composited over a mid-dark terrain (`#2A3326`): `text` ≈ 14.5:1, `text_muted` ≈ 6.4:1, `gold` ≈ 9.6:1, `supply` ≈ 6.0:1, `hp_good` ≈ 8.7:1. Minimum for readable labels is 4.5:1; disabled states are exempt. The headless acceptance check computes these ratios from the tokens and fails below 4.5:1 for `text`, `text_muted`, `gold`, `supply`, `hp_*`.

### 2.2 Radii

`8` micro chip, `14` buttons/pills, `20` cards, `28` minimap/selection, `999` pill.

### 2.3 Spacing scale

`4, 8, 12, 16, 24, 32` px at the 1080p reference height. Gaps: 8 between resource pills, 12 between context buttons, 16 between zones, 24 outer margin from the safe edge.

### 2.4 Typography

Font atlas is rasterised at 30 px (`Font::load`), so `scale = px / 30`.

| Role | px @1080p | scale | Weight treatment |
|---|---|---|---|
| Phase title | 40 | 1.333 | fill + 2 px shadow |
| Card title / map name | 28 | 0.933 | fill + 1 px shadow |
| Resource value / HP value | 24 | 0.800 | fill + 1 px shadow |
| Button label | 18 | 0.600 | bold simulation (1 px offset double draw) |
| Secondary label / class | 16 | 0.533 | muted, no shadow |
| FPS / micro | 14 | 0.467 | muted |

No text below 14 px; button labels are 18 px (tech-lead requirement 14-16 px **bold** is satisfied at the low end; 18 px is used where space allows). Values are 22-24 px, inside the requested 18-24 px band. Text is truncated with `…` by measured advance, never clipped by a hard scissor.

### 2.5 Touch targets

| Control | Size @1080p | Rationale |
|---|---|---|
| Primary ring button | **132x132** | ≥104 px; 56 dp at a 2.36 px/dp HUD scale |
| Context tray button | **112x112** | same scale, thumb-reachable |
| Quick chip | **112x72** | visual card; the engine expands the hit target to 108 px (`MIN_BUTTON_TARGET`), so the effective target is 108x108 |
| Resource pill | 130x64 | information, not a destructive action |
| Minimap card | 220x176 landscape / 190x156 portrait | matches Art of War 3 / HoK proportion (184-280 px) |
| Joystick | radius 118 landscape / 110 portrait; knob 52 | high-contrast base, 4 direction ticks |
| Selection card | 560x140 landscape / 1000x150 portrait | portrait 96, name 28 px, HP 24 px, 2x2 ability slots |

Android guidance is 48x48 dp minimum; the game's HUD scale is ~2.36 px/dp at 1080p, so 132 px ≈ 56 dp, 112 px ≈ 47.5 dp (context) and the minimap is a large pan target. The chip card is 112x72; the game engine expands its hit rect to `MIN_BUTTON_TARGET` (108 px), so the effective touch target is 108x108. Primary and context buttons are never below 112 px.

## 3. Safe area

`SafeArea { left, top, right, bottom }` in framebuffer pixels. The renderer/app must pass `WindowInsets` (Android) or `Screen.safeArea`/SDL3 safe area; the HUD never derives it from the status bar height.

| Viewport | top | bottom | left | right | source |
|---|---|---|---|---|---|
| 2414x1080 landscape mock | 80 | 48 | 48 | 48 | rejected shot: status bar overlap ≈ 30/492 px → ≥72 px; 80 px chosen |
| 1080x2414 portrait mock | 96 | 64 | 24 | 24 | cutout top; gesture bar bottom |

All zones are computed from `left = safe.left + 16`, `top = safe.top + 16`, `right = w - safe.right - 16`, `bottom = h - safe.bottom - 16`. A rect outside this frame is a layout test failure.

If the app is not truly immersive and the system bar is taller than the supplied inset, the HUD still reserves 80 px at 1080p; the integration must report the actual inset from the window, not guess.

## 4. Landscape layout — 2414x1080 (reference coordinates)

Origin top-left, y down.

| Zone | Rect | Notes |
|---|---|---|
| Safe frame | x 48..2366, y 80..1032 | outer margin 16 applied inside |
| Minimap card | **64, 96, 220x176** | map inset 8 → 204x160; label "MINIMAP" 14 px |
| Resource row | **300, 96, 504x64** | 4 pills, 120x64, gap 8: gold/lumber/supply/army |
| Top meta | **828, 96, 1058x64** | map name left 28 px; FPS right 14 px; reserved, no overlap with chips |
| Quick chips | **1878, 96, 472x72** | 4x112x72, gap 8: ARMY / BASE / ZOOM- / ZOOM+ |
| Toast | centered **927, 292, 560x56** | below minimap band; moves below hint if hint active |
| Hint banner | centered **807, 292, 800x56** | placement/attack-mode text; mutually exclusive with toast position |
| Joystick | center **(182, 898)**, radius 118 | extents 64..300, 780..1016 |
| Selection card | **64, 624, 560x140** | above joystick; portrait 96; 2x2 ability slots |
| Command ring | center **(2140, 826)**, radius 110 | 4 buttons **132x132** at up/right/down/left; extents 1964..2316, 650..1002 |
| Context tray | **1832, 522, 484x112** | 4x112, gap 12, right-aligned to ring |
| Selection/production text | inside selection card | name 28, class 16, HP 24, production 16 |
| Phase card | centered **847, 400, 720x280** | full-screen dim behind |

No zone overlaps another. The ring and tray are 16 px apart; the tray bottom is 634, ring top 650 (16 px gap). The selection card right edge is 524; the ring left edge is 1964, so the bottom-centre stays clear.

## 5. Portrait layout — 1080x2414 (reference coordinates)

| Zone | Rect | Notes |
|---|---|---|
| Safe frame | x 24..1056, y 96..2350 | outer margin 16 applied inside |
| Minimap card | **40, 112, 190x156** | map 174x140 |
| Resource grid | **246, 112, 562x64** | 4x130x64, gap 8 (one row fits) |
| Quick chips | **808, 112, 232x152** | 2x2 grid, 112x72, gap 8 |
| Top meta | **40, 280, 1000x34** | map name left, FPS right; below minimap, above toast |
| Toast / hint | centered **240, 326, 600x56** | below top meta |
| Selection card | **40, 1724, 1000x150** | full-width, above context tray |
| Context tray | **556, 1890, 484x112** | 4x112, gap 12, directly above ring |
| Command ring | center **(880, 2174)**, radius 96 | 4 buttons **120x120**; extents 724..1036, 2018..2330 |
| Joystick | center **(158, 2216)**, radius 110 | extents 48..268, 2106..2326 |
| Phase card | centered **180, 900, 720x280** | |

Vertical stack (bottom-up): ring bottom 2330, ring top 2018, tray 1890..2002, selection 1724..1874, gap 16. The joystick and ring are separated by 456 px horizontally, so a thumb never crosses the other control.

## 6. Component anatomy

### 6.1 Resource pills

- Card: 20 px radius glass, hairline stroke, 130x64. Icon 30x30 from the WC3 `GoldIcon/LumberIcon/SupplyIcon` at 30 px (content), value 24 px bold, label 14 px muted under/next to value.
- Gold/lumber/supply/army have fixed order; warning state (`value < cost`) turns the value `hp_low` and adds a 2 px left accent.
- No text over icon: icon column 38 px, text starts at +44.

### 6.2 Minimap card

- 220x176 card (portrait 190x156), 28 radius, 8 px inner inset, 1.5 px frame, corner ticks. Label "MINIMAP" 14 px in the header strip (portrait: hidden to save height).
- Unit dots: 4 px friendly `supply`, 5 px buildings, 4 px enemy `danger`, white 1.5 px camera rectangle; clamped inside the map rect.
- Tapping pans the camera; the map is a 200+ px target.

### 6.3 Selection card

- Portrait 96x96 rounded 20 with 2 px stroke; class chip above the name (16 px); name 28 px truncated; HP bar 16 px tall with 24 px "420 / 420" right-aligned; production row 18 px with icon 24 px + "FOOTMAN 42%" + progress bar 6 px.
- Stats row: armor/damage icons + values 16 px; ability slots 4x36 with 64 px BLP icons, disabled slots dim. Empty selection shows "NO SELECTION / TAP A UNIT OR DRAG A BOX" in 18/16 px, no icons.

### 6.4 Command ring + context tray

- Ring: 4 primary buttons on a 220 px diameter circle, each 132x132, circular glass with a 2.5 px rim; center medallion 52 px shows the current mode (move/attack/stop) and is not a button in the first release.
- Icon: 64x64 or 72x72 BLP (never 96), centered, label 18 px bold under the icon inside the button, hotkey 14 px in the top-right corner, cooldown/production overlay as a dark radial wedge + bottom progress bar.
- Pressed: face lightens, 2 px inner shrink, rim `stroke_hot`, icon 0.96 scale. Disabled: 45% alpha, no rim, icon desaturated, label `disabled`. Hover/focus unused on touch; last-pressed flash 120 ms.
- Context tray: 4x112 square cards with 64 px icons and 18 px labels + 14 px cost; gap 12; right-aligned with the ring. Buttons never overlap or clip: each label is measured and truncated inside `rect.w - 12`.
- If fewer than 4 context actions exist, the tray shrinks from the left and keeps right alignment; no empty holes.

### 6.5 Joystick

- Base: 236 px circle `panel` 50% + 3 px `stroke`; inner 72% ring; 4 direction ticks at 0/90/180/270 (18x3 px, `text_muted`), active tick `stroke_hot`.
- Knob: 52 px radius, `text` at 92%, shadow ring 4 px; centre dot 10 px `stroke_hot`.
- Active displacement clamped to `radius - knob_radius - 4`; label "MOVE" 14 px below, never under the knob.

### 6.6 Chips

- 104x72 glass, 18 radius, 14 px label + 24 px icon; pressed/disabled states identical to buttons. Used for ARMY / BASE / ZOOM- / ZOOM+.
- Right/top edge alignment; never under system icons (safe.top applies).

### 6.7 Toast and phase overlay

- Toast: 560x56 pill, icon dot 10 px + 18 px text; tones info/success/danger; fades by caller; never overlaps top text because it is placed below the minimap band (landscape) or top meta (portrait).
- Phase: full-screen 58% dim, card 720x280, title 40 px, subtitle 18 px, action hint 18 px; buttons are not placed inside the card (tap anywhere), so there is no hidden touch target.

## 7. Icon pipeline (all 691 CommandButtons)

- Source: `analysis/war3_full3/ReplaceableTextures/CommandButtons/*.blp` — **691 files** (689 `BTN*.blp` + 2 `DISBTN*.blp`), 64x64.
- Decode with `wc3-formats::blp::decode`: BLP1 palette + 8/4/1-bit alpha, BLP1 JPEG, BLP2 palette/alpha, DXT1/3/5, BGRA. Output is tightly packed RGBA8; palette BGRA is expanded to RGBA; alpha is preserved exactly (no threshold except 1-bit sources).
- Colour: BLP texel data is authored as sRGB. The renderer must upload the atlas as an sRGB texture (`Rgba8UnormSrgb`); the HUD shader must not apply a second gamma conversion. Validation rejects icons whose alpha is all-zero or all-255 where the source has transparent corners; validates edge alpha gradient on a sample of 64 icons.
- Atlas: 1024 px wide, 4 px gutter, 64x64 cells, power-of-two height; one texture. Metadata maps file stem → UV rect. No placeholder icon is generated: a missing icon is a hard error reported with the file name.
- Display size: **64 px at 1x, 72 px at 1.125x max**. Never stretch to 96 px (would blur the 64x64 source); use nearest-neighbour for exact 1x/2x, linear only for 1.125x. Selection portraits use 96 px card with 64 px icon centred, not upscaled art.
- Contact sheet: every icon rendered at 64 px on a checkerboard + dark background, 32 columns; `analysis/agent-hud/icons/contact_sheet.png`. Alpha edges are inspected on a magenta/green checkerboard to reveal fringes.

## 8. Acceptance checks (headless, no "looks modern" claims)

The test renders `MobileHud` at 2414x1080 and 1080x2414 through a CPU rasteriser, saves `analysis/agent-hud/mock-2414x1080.png` and `mock-1080x2414.png`, and computes:

1. **Touch targets**: every `Interactive` rect has `min(w,h) >= 104` at 1080p (primary >=132, context >=112); fail otherwise.
2. **Safe area**: every zone rect is inside the safe frame; no top text above `safe.top`; fail with the offending rect.
3. **Overlap**: pairwise intersection area == 0 for `{minimap, resources, chips, top_meta, toast, hint, selection, command, joystick}`; text boxes are inside their parent rects.
4. **Text fit**: `font.measure(text, scale) <= rect.w - padding` for every label; uses the real font atlas when available, otherwise the deterministic fallback widths.
5. **Contrast**: WCAG ratio >= 4.5 for `text`, `text_muted`, `gold`, `supply`, `hp_good`, `hp_warn`, `hp_low` on the composited panel colour.
6. **Icon integrity**: 691/691 decoded, 0 missing, 0 all-transparent, 64x64 each; report min/max alpha and edge sample.
7. **Screenshot diff**: the headless mock must show no status-bar band (top 80 px contains no HUD pixels other than optional background), command labels fully inside their buttons, and all four ring buttons present.

Commands: `cargo test -p wc3-game mobile_hud -- --nocapture` and the icon-catalog tool. The final report includes the PNG paths and the JSON metrics.

## 9. Explicit non-goals

- No WC3 stone FDF chrome stretched over the phone; FDF data remains available for content names, not chrome.
- No 96 px blurry icon upscaling, no placeholder art, no text baked into textures.
- No status-bar assumptions: if the app cannot go immersive, the HUD still starts below 80 px, and the integration reports the real inset.
- No MOBA-style 5-skill wheel: the RTS grammar is 4 primary commands + 4 context actions.
