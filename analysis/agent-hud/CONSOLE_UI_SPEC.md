# Console HUD — UI specification (landscape 1920×1080 / 2414×1080)

Status: **spec only, no code**. This document is the implementable contract for the console/tablet HUD; it complements `DESIGN_SPEC.md` (phone HUD, portrait + landscape ≤1600 px) and is derived from `GITHUB_UI_RESEARCH.md` (measured WC3 geometry and palette, 2026-11-12).

Owner of implementation: whoever owns `crates/wc3-game/src/{style.rs,hud.rs}` plus a new `console_hud.rs`; `game.rs` wiring is described in §13, not changed here.

---

## 0. Scope

| In scope | Out of scope |
|---|---|
| Landscape console at 1920×1080 and 2414×1080, pointer and touch | Phone portrait layout (see `DESIGN_SPEC.md`) |
| Frame tree, rects, anchors, safe area, z-order | 3-D scene, camera, selection picking |
| Tokens (colour, type, spacing, radii, strokes) | Map/unit data model |
| Component anatomy + state machines | Networking, save/load |
| Input mapping, hit targets | Audio |
| Asset pipeline + atlas contract | Localisation content (only the mechanism) |
| Machine-checkable acceptance tests | Build/packaging |

Non-goal: pixel-perfect reproduction of 2002 WC3 chrome. The grammar is WC3 (corner minimap → portrait → info → 4×3 card, stone + gold), the metrics are native to 1080p.

---

## 1. Why this spec exists — observed defects in the current build

`analysis/agent-hud/mock-console-2414x1080.png` (2414×1080, rendered 2026-11-12 17:36) is the reference failure case. Measured from the PNG and confirmed on 1:1 crops:

| # | Observed | Measurement | Rule that prevents it |
|---|---|---|---|
| D1 | WC3 chrome drawn at **1:1 design px** on a 2414-wide screen | console band top at **y≈734**, band ≈346 px (the 352 px authored height); the middle stone is stretched **×1.51** horizontally while bevels stay 1 px | §2.2: chrome is **authored per profile at the target width**, never stretched; 1 px features stay 1 px |
| D2 | Command cells at native 64–78 px | 4 columns span ≈257 px → cell ≈62 px, gap ≈3 px | §4/§6: cell **72 (P) / 88 (T)**, gap **8 / 10**; icon fixed at 64 |
| D3 | Resource row floats **on the stone parapet** 8 px above the card | gold/coin + `1200`, `340`, `7/12` at **y≈780–805** | §4: resources live in the **top bar** with a defined backing; the card owns its band |
| D4 | Text has **no backing panel** | `ARMOR 0 DAMAGE 14` on bare stone; `TRAINING 43%` in cyan on the arch | §5/§9: every text run sits inside a **panel or scrim** rect; free-floating text is a test failure |
| D5 | Bars overlap their own labels | `HP 200/200` starts at x≈510 inside a bar spanning ≈440–660; mana label clipped to `3AXA 50` | §6.4: bar + label are one component with reserved sub-rects |
| D6 | Label clipped by art | `MINIMAP` drawn over the frame's top bevel, cut at the frame edge | §6.1: labels live in a **header strip inside the frame**, never on the bevel |
| D7 | Portrait is a 96 px square inside a 167×228 frame | art ≈95×95 at (315,835), frame far larger | §6.3: portrait frame is sized **to the art** (120×160 / 140×184) |
| D8 | Debug text shares the HUD layer | `HUMANO4` top-left, `20 FPS` top-right, inside the safe band | §2.4: `Debug` layer above HUD, hidden in release, never inside HUD rects |
| D9 | Multiple overlapping notices | `SELECTED 4/4` drawn on the 4th slot's top border at 8 px | §6.6: status text has a reserved strip **above** the grid |
| D10 | Inventory undefined | 3×2 blue slots painted by art, not by state | §6.7: inventory is an explicit 2×3 module with state |

Every numbered rule below is derived from one of these; §11 turns each into a test.

---

## 2. Design contract

### 2.1 Units

- All geometry in **physical pixels at 1080p** (`H = 1080`). No dp, no percent, no `0.8/0.6` FDF fractions at runtime.
- Both targets are 1080 tall, so **vertical metrics are identical**; only `W` changes. Rects that derive from `W` are computed for each resolution in §4 — there is no runtime percentage.
- Chrome art is authored at the **target width** (`W − safe.left − safe.right`) — see §10.4. Do not scale the WC3 tiles.

### 2.2 Profiles

| Profile | Cell | Gap | Icon | Min hit | Use |
|---|---|---|---|---|---|
| **P** (pointer) | 72 | 8 | 64 | 48 | mouse/trackpad, ≥1920 |
| **T** (touch) | 88 | 10 | 64 | 88 | finger, tablets |

Profile is a **layout input**, not a device guess: it comes from the last input event (`InputSource::Pointer | Touch`), latched for 2 s. Both profiles are valid at both resolutions.

### 2.3 Safe area

`safe = { top ≥ 72, bottom ≥ 24, left ≥ 24, right ≥ 24 }`, supplied by the platform in physical px.

- `top` **must** be the real status-bar/cutout inset clamped to ≥72 (`REFERENCES.md:74`: pills at y≈14–30 collided with system icons; ≥72 px required). 48 is only legal when the status bar is truly hidden and the window reports it.
- Never anchor to `(0,0)`; every frame anchors to `safe` (§4).
- If the platform reports 0 insets while bars are visible, the app must report a defect, not silently use 0.

### 2.4 Layers (z-order, bottom → top)

| z | Layer | Contents | Rule |
|---|---|---|---|
| 0 | World | 3-D scene | — |
| 10 | Console | chrome, minimap map, portrait art, slot faces | never above any other UI |
| 20 | Content | info panel text/bars, resource values, chip labels | inside its own rect |
| 30 | Overlay | tooltips, cooldown text, queue progress | may cross content, never the safe edge |
| 40 | Toast | alerts | max 3, stacked |
| 50 | Debug | FPS, entity id, layout boxes | **not shipped**; toggled by a build flag |

No layer may draw above `safe.top` except the world.

### 2.5 Resolution matrix (vertical metrics are shared)

| Metric | P | T |
|---|---|---|
| Console band height `Hc` | **264** | **320** |
| Band top `y` | 1056−264 = **792** | 1056−320 = **736** |
| Parapet (chrome above card) | 32 | 36 |
| Card block | 312 × 232 | 382 × 284 |
| Minimap frame / window | 216 / 200 | 260 / 244 |
| Control strip width | 44 | 48 |
| Portrait frame | 120 × 160 | 140 × 184 |
| Portrait art | 96 × 96 | 112 × 112 |
| Top bar height | 56 | 56 |

Band as share of height: **24.4 % (P)** and **29.6 % (T)**. T reproduces WC3's authored proportion at 1080p (`352 × 1080/1200 = 316.8 ≈ 320`); P compresses it by 32 px to buy battlefield.

---

## 3. Tokens

### 3.1 Colour (sRGB hex for authoring, linear for the GPU)

| Token | Hex | Linear | Use |
|---|---|---|---|
| `panel` | `#0b1015` | 0.0056,0.0091,0.0132 | solid backing for dialogs, tooltips |
| `scrim` | `#000000` @ 0.72 → 0.00 | — | gradient behind info text (see 5.2) |
| `stroke` | `#d0d3d0` @ 0.24 | — | 1 px hairline |
| `stroke_hot` | `#a78900` @ 1.0 | 0.3864,0.2502,0 | focus/primary rim (gold) |
| `text` | `#f2f6ff` | 0.8879,0.9301,1.0 | names, values |
| `text_muted` | `#a9b4c2` | 0.4072,0.4677,0.5457 | class, labels, hints |
| `gold` | `#a78900` | 0.3864,0.2502,0 | resource values, section titles |
| `gold_lit` | `#f1d320` | 0.8796,0.6514,0.0144 | selection ring, pressed rim |
| `silver` | `#d0d3d0` | 0.6308,0.6514,0.6308 | bevel highlight, icon frame |
| `steel` | `#5c5c5b` | 0.1080,0.1080,0.1057 | lit stone face |
| `steel_dark` | `#3a3a39` | 0.0423,0.0423,0.0410 | stone mass |
| `rust` | `#4b2104` | 0.0704,0.0152,0.0012 | warm shadow |
| `black` | `#000000` | 0 | wells, seams, letterbox |
| `hp_good` | `#41c04a` | 0.0529,0.5271,0.0685 | HP > 55 % |
| `hp_warn` | `#f1d320` | 0.8796,0.6514,0.0144 | HP 25–55 % |
| `hp_low` | `#ff6b66` | 0.6038,0.1441,0.1274 | HP < 25 % |
| `mana` | `#3b9ff5` | 0.0437,0.3467,0.9131 | mana fill |
| `danger` | `#ff5a5f` | 0.6038,0.0975,0.1022 | enemy, error toast |
| `ok` | `#6ee7a0` | 0.3015,0.7912,0.3564 | success toast |
| `disabled` | `#b0b8c2` @ 0.45 | — | disabled text/icon |

Rules: gold is **≤3 % of chrome pixels** (WC3 measured 2.8 %); gold **text only at ≥18 px** (4.26:1 on the worst-case scrim, §5.2 — passes the 3:1 large-text threshold, fails 4.5 at smaller sizes). Body text is `text`/`text_muted` only.

### 3.2 Type scale

| Role | px | Weight | Line | Shadow | Used by |
|---|---|---|---|---|---|
| Display | 28 | 600 | 34 | 1 px `#000` 90 % | unit name, map name |
| Title | 22 | 600 | 28 | 1 px | section titles, dialog headers |
| Value | 24 | 600 | 28 | 1 px | resource values |
| Body | 18 | 500 | 24 | 1 px | card labels, tooltip body |
| Label | 14 | 500 | 18 | none | class, stats, hints |
| Micro | 12 | 500 | 16 | none | hotkeys, queue count, FPS |
| Numeric | 16 | 600 tabular | 20 | 1 px | `200 / 200`, cooldown seconds |

Font atlas is rasterised at 30 px, so `scale = px/30` (same contract as `DESIGN_SPEC.md:50`). Numerals must be tabular — a proportional font makes the HP text jitter every frame.

Donor note (`FONT_TEXT_METRICS.md` §3–§4): WC3 rasterises `Fonts\FRIZQT__.TTF` per FDF size (`size/0.6·H`), whose digits are already tabular at exactly 0.5 em, and its FDF text carries a **2–4 px black shadow at 90 %** — the 1 px shadow above is a deliberate 1080p reduction, not the donor value.

### 3.3 Spacing / radii / strokes

- Spacing: `4, 8, 12, 16, 24, 32`. Component padding: 8; group gap: 12; zone gap: 16; safe margin: 24.
- Radii: `4` micro, `8` chip/pill, `12` panel, `16` card, `999` pill.
- Strokes: `1` hairline, `2` rim, `4` icon frame (baked).

---

## 4. Frame tree and rects

Every rect below is `(x, y, w, h)`, origin top-left, at 1080p. `flex` = derived from `W`.

```
Root (0,0,W,1080)
├─ SafeFrame (safe.left, safe.top, W−safe.l−safe.r, 1080−safe.t−safe.b)
│  ├─ TopBar        (flex)   y = safe.top, h = 56
│  │  ├─ ChipCluster  (left)
│  │  └─ ResourceCluster (right)
│  ├─ ConsoleBand   (flex)   y = 1056−Hc, h = Hc
│  │  ├─ LeftBlock  (fixed)  minimap + strip + portrait
│  │  ├─ Field      (flex)   info panel
│  │  └─ RightBlock (fixed)  inventory? + card
│  └─ ToastHost     centred, y = safe.top + 56 + 16
└─ Debug (not shipped)
```

### 4.1 1920×1080 — profile P (safe 24/72/24/24)

| Frame | x | y | w | h | right/bottom |
|---|---|---|---|---|---|
| TopBar | 24 | 72 | 1872 | 56 | 1896 / 128 |
| Chips ×4 | 24, 164, 304, 444 | 72 | 132 | 56 | 576 |
| Resource pills ×3 | 1332, 1488, 1644 | 72 | 148 | 56 | 1792 |
| Clock | 1800 | 72 | 96 | 56 | 1896 |
| ConsoleBand | 24 | 792 | 1872 | 264 | 1896 / 1056 |
| MinimapFrame | 24 | 840 | 216 | 216 | 240 / 1056 |
| MinimapWindow | 32 | 848 | 200 | 200 | 232 / 1048 |
| MinimapStrip | 240 | 840 | 44 | 216 | 284 / 1056 |
| PortraitFrame | 296 | 896 | 120 | 160 | 416 / 1056 |
| InfoPanel (with inventory) | 432 | 824 | 1024 | 232 | 1456 |
| InfoPanel (no inventory) | 432 | 824 | 1136 | 232 | 1568 |
| Inventory (optional) | 1472 | 824 | 96 | 232 | 1568 |
| CommandCard | 1584 | 824 | 312 | 232 | 1896 / 1056 |
| CardStatus | 1584 | 792 | 312 | 32 | 1896 / 824 |

### 4.2 1920×1080 — profile T

| Frame | x | y | w | h | right/bottom |
|---|---|---|---|---|---|
| TopBar | 24 | 72 | 1872 | 56 | 1896 / 128 |
| Chips ×4 | 24, 164, 304, 444 | 72 | 132 | 56 | 576 |
| Resource pills ×3 | 1332, 1488, 1644 | 72 | 148 | 56 | 1792 |
| Clock | 1800 | 72 | 96 | 56 | 1896 |
| ConsoleBand | 24 | 736 | 1872 | 320 | 1896 / 1056 |
| MinimapFrame | 24 | 796 | 260 | 260 | 284 / 1056 |
| MinimapWindow | 32 | 804 | 244 | 244 | 276 / 1048 |
| MinimapStrip | 284 | 796 | 48 | 260 | 332 / 1056 |
| PortraitFrame | 344 | 872 | 140 | 184 | 484 / 1056 |
| InfoPanel (with inventory) | 500 | 772 | 870 | 284 | 1370 |
| InfoPanel (no inventory) | 500 | 772 | 998 | 284 | 1498 |
| Inventory (optional) | 1386 | 772 | 112 | 284 | 1498 |
| CommandCard | 1514 | 772 | 382 | 284 | 1896 / 1056 |
| CardStatus | 1514 | 736 | 382 | 36 | 1896 / 772 |

### 4.3 2414×1080 — profile P

| Frame | x | y | w | h | right/bottom |
|---|---|---|---|---|---|
| TopBar | 24 | 72 | 2366 | 56 | 2390 / 128 |
| Chips ×4 | 24, 172, 320, 468 | 72 | 140 | 56 | 608 |
| Resource pills ×3 | 1794, 1958, 2122 | 72 | 156 | 56 | 2278 |
| Clock | 2286 | 72 | 104 | 56 | 2390 |
| ConsoleBand | 24 | 792 | 2366 | 264 | 2390 / 1056 |
| MinimapFrame | 24 | 840 | 216 | 216 | 240 / 1056 |
| MinimapWindow | 32 | 848 | 200 | 200 | 232 / 1048 |
| MinimapStrip | 240 | 840 | 44 | 216 | 284 / 1056 |
| PortraitFrame | 296 | 896 | 120 | 160 | 416 / 1056 |
| InfoPanel (with inventory) | 432 | 824 | 1518 | 232 | 1950 |
| InfoPanel (no inventory) | 432 | 824 | 1630 | 232 | 2062 |
| Inventory (optional) | 1966 | 824 | 96 | 232 | 2062 |
| CommandCard | 2078 | 824 | 312 | 232 | 2390 / 1056 |
| CardStatus | 2078 | 792 | 312 | 32 | 2390 / 824 |

### 4.4 2414×1080 — profile T

| Frame | x | y | w | h | right/bottom |
|---|---|---|---|---|---|
| TopBar | 24 | 72 | 2366 | 56 | 2390 / 128 |
| Chips ×4 | 24, 172, 320, 468 | 72 | 140 | 56 | 608 |
| Resource pills ×3 | 1794, 1958, 2122 | 72 | 156 | 56 | 2278 |
| Clock | 2286 | 72 | 104 | 56 | 2390 |
| ConsoleBand | 24 | 736 | 2366 | 320 | 2390 / 1056 |
| MinimapFrame | 24 | 796 | 260 | 260 | 284 / 1056 |
| MinimapWindow | 32 | 804 | 244 | 244 | 276 / 1048 |
| MinimapStrip | 284 | 796 | 48 | 260 | 332 / 1056 |
| PortraitFrame | 344 | 872 | 140 | 184 | 484 / 1056 |
| InfoPanel (with inventory) | 500 | 772 | 1364 | 284 | 1864 |
| InfoPanel (no inventory) | 500 | 772 | 1492 | 284 | 1992 |
| Inventory (optional) | 1880 | 772 | 112 | 284 | 1992 |
| CommandCard | 2008 | 772 | 382 | 284 | 2390 / 1056 |
| CardStatus | 2008 | 736 | 382 | 36 | 2390 / 772 |

### 4.5 Anchor rules (single source of truth)

| Frame | Anchor | Offset |
|---|---|---|
| TopBar | TOPLEFT + TOPRIGHT of safe | `(0,0)` / `(0,0)` |
| ChipCluster | LEFT of TopBar | `(0,0)`, items 8 px apart |
| ResourceCluster | RIGHT of TopBar | `(0,0)`, items 8 px apart, clock last |
| ConsoleBand | BOTTOMLEFT + BOTTOMRIGHT of safe | `(0,0)` / `(0,0)` |
| MinimapFrame | BOTTOMLEFT of ConsoleBand | `(0,0)` → frame bottom = band bottom |
| MinimapStrip | RIGHT of MinimapFrame, TOP-aligned | `(0,0)` |
| PortraitFrame | LEFT of InfoPanel, BOTTOM of band | `(−16, 0)` |
| InfoPanel | LEFT = portrait.RIGHT+16, RIGHT = inventory.LEFT−16 (or card.LEFT−16) | — |
| Inventory | LEFT of CommandCard, BOTTOM of band | `(−16, 0)` |
| CommandCard | BOTTOMRIGHT of ConsoleBand | `(0,0)` |
| CardStatus | TOP of CommandCard | `(0, −32)` P / `(0, −36)` T (y = card.y − status.h) |
| ToastHost | TOP centre of safe | `y = safe.top + 72`, `w = 560` |

Derived widths: `LeftBlock = 216+44+12+120 = 392` (P) / `260+48+12+140 = 460` (T). `RightBlock = card 312` (P) / `382` (T); with inventory add `96+16` (P) / `112+16` (T).

---

## 5. Info panel

### 5.1 Variants

- **Narrow** (`W < 2200`): one column, rows 1–7 stacked.
- **Wide** (`W ≥ 2200`): two columns, 50 % each with a 24 px gutter — identity+bars left, production+stats right.
- The variant depends on **window width only**, never on whether the inventory module is showing: otherwise selecting a unit with items would reflow the whole info panel. At 1920 the info panel is 870–1136 px wide (narrow); at 2414 it is 1364–1630 (wide).
- `x0 = info.x + 12`, `w = info.w − 24` for every row.

### 5.2 Backing (mandatory)

- `scrim`: vertical gradient `#000` alpha **0.72 at the bottom → 0.00 at 60 % height**, over the full InfoPanel rect, drawn at z=20 *under* text, not at console level.
- Worst case is the scrim over `steel` `#979996` → composite ≈ `#2b2b2b`; `text` gives 7.0:1, `text_muted` 4.6:1, `gold` 4.26:1 (hence the ≥18 px rule for gold).
- No text outside a panel/scrim. `CardStatus` is text on the parapet → it gets its own `panel` @ 0.72 strip (32/36 px tall), because the parapet is the brightest chrome.

### 5.3 Row grid (narrow variant; y is absolute for the 232 px panel, add 26 px to every row for the 284 px panel)

| Row | y | h | Contents |
|---|---|---|---|
| 1 Name | 836 | 30 | name 28/34, level badge 28×28 right |
| 2 Class | 870 | 20 | class 14 upper `text_muted`; type chip 20 px |
| 3 HP | 898 | 20 | bar 16 px, width 360; text `200 / 200` 16 numeric, right-inside |
| 4 Mana | 922 | 14 | bar 10 px, width 360; hidden if max = 0 |
| 5 Stats | 944 | 32 | 4 cells × 108: armor / damage / range / speed (icon 20 + label 12 + value 16) |
| 6 Queue | 984 | 44 | up to 7 cells 40×40 gap 4 + 4 px progress; `QUEUE` 12 px |
| 7 Hint | 1032 | 16 | contextual hint 12 px `text_muted`, right-aligned |

Wide variant: rows 1–4 and 7 left column; rows 5–6 right column (stats 2×2, queue 7 cells).

---

## 6. Components

Format: **geometry → anatomy → states → rules.**

### 6.1 Minimap frame + window

- Frame **216×216 (P) / 260×260 (T)**; window **200 / 244**; bezel 8 px: 1 px black, 2 px silver, 1 px black, 4 px stone.
- Header strip **inside** the frame: 20 px tall at the top of the bezel, label `MINIMAP` 12 px `text_muted`, left 8 px. Never draw the label over the bevel (D6).
- Window content: map texture, aspect preserved, centred, `black` behind (letterbox).
- Dots: friendly 4 px in the player's team colour, buildings 5 px, enemy 4 px `danger`, hero 6 px with a 1 px white ring. Camera rect 1.5 px white 0.9.
- States: `idle`, `panning` (cursor grab), `ping` (600 ms expanding ring `gold_lit`), `disabled` (0.4 alpha, no input).
- Input: click/tap = move camera; drag = pan; wheel = zoom (clamped); minimap never scrolls the world under the pointer.

### 6.2 Minimap control strip

- **44 (P) / 48 (T)** wide, full band height, right of the frame.
- 3 toggles **36×36 (P) / 40×40 (T)**, pitch 44/48, top-aligned with 8 px inset; then a 40/44 px "ping" button bottom-aligned.
- Toggle states: off (face `panel` 0.6, glyph `text_muted`), on (`stroke_hot` 2 px rim, glyph `text`), pressed (face +8 % luminance), disabled (0.35 alpha).
- Glyphs: terrain, allies, ping — 20 px, 1 px `black` shadow.

### 6.3 Portrait

- Frame **120×160 (P) / 140×184 (T)**; art **96×96 / 112×112** anchored bottom-centre with 8 px inset; 2 px `gold` ring + 1 px `black` inside; arch header **48 px (P) / 64 px (T)** above the art with the hero level (numeric 16) and rank glyph.
- States: `normal`, `dead` (art 40 % luminance, desaturated, no ring), `targeted` (`gold_lit` 2 px ring pulsing 1.2 s).
- Click selects the portrait unit; right-click centres the camera. Hit target = whole frame.

### 6.4 HP / mana bar

- One component, two instances. Width **360 (P) / 420 (T)**, HP h **16**, mana h **10**, gap 6.
- Anatomy: 1 px `black` frame, 1 px inner `#000` 60 % shadow at the top, fill (HP tint per value, mana `mana`), 1 px `silver` 0.35 top highlight.
- Label: `200 / 200` numeric 16, right-aligned **inside** the bar with 6 px padding and a 1 px `black` shadow. If the bar is < 50 % full the label moves to the right *outside* the bar (same baseline) so it never sits on the empty track (D5).
- Colors: `hp_good` > 55 %, `hp_warn` 25–55 %, `hp_low` < 25 %; thresholds evaluated on `current/max` with hysteresis 2 %.
- When `max == 0` the mana bar is drawn as an **empty 10 px track at 25 % alpha** — the row never collapses, because a state change must not move any rect (§7).

### 6.5 Command slot (4×3)

- Cell **72×72 (P) / 88×88 (T)**; gap **8 / 10**; icon **64** (native, never upscaled); face radius 4.
- Grid origin: card BOTTOMRIGHT, row 0 = **top**; `x = card.x + i·(cell+gap)`, `y = card.y + j·(cell+gap)`.
- Anatomy: face fill `panel` 0.55; icon 64 centred; **baked border** from the icon (4 px = 1 black + 2 silver + 1 black) is the only frame at 72. In the 88 cell the icon keeps 64 px (12 px gutter each side) and the face adds a 2 px `stroke` 0.24 ring so the gutter reads as a bevel, not as padding.
- Hotkey glyph 12 px at top-right inside a 16×16 `panel` 0.7 square (not directly on the art).
- Cooldown: radial wedge from 12 o'clock, `#000` 0.62, plus `numeric 16` seconds centred on a 32×32 `panel` 0.6 disc.
- Charge count: 12 px bottom-right on a `panel` 0.7 square.
- **States** (all must keep the same rect — no layout shift):

| State | Face | Icon | Rim | Extra |
|---|---|---|---|---|
| `normal` | `panel` 0.55 | 1.0 | none | — |
| `hover` (pointer only) | `panel` 0.70 | 1.0 | 1 px `stroke_hot` | tooltip after 400 ms |
| `pressed` | `panel` 0.85 | 0.96 scale | 2 px `gold_lit` | 2 px inset shadow |
| `active` (targeting) | `panel` 0.70 | 1.0 | 2 px `gold_lit` pulse 1.2 s | cursor changes |
| `disabled` | `panel` 0.35 | 0.45 alpha, 0.2 desat | none | no tooltip delay (immediate reason) |
| `insufficient` | `panel` 0.55 | 1.0 | none | cost text `hp_low`; face unchanged |
| `cooldown` | `panel` 0.55 | 0.8 | none | wedge + seconds |
| `queued` | `panel` 0.55 | 0.9 | 1 px `stroke` | position index 12 px |
| `levelup` | `panel` 0.55 | 1.0 | 2 px `gold_lit` | `human-button-lvls-overlay` glyph 32 px |

- Rules: pressed face lightens **and** the icon shrinks 0.96 (touch feedback needs both); disabled never uses colour alone (also 45 % alpha + desaturation).

### 6.6 Card status strip

- **312×32 (P) / 382×36 (T)** directly above the card, `panel` 0.72, radius 8, 1 px `stroke` 0.16.
- Contents: left `SELECTED 4/4` micro 12 `text_muted`; right: subgroup tabs (max 4, 24×24) or `QUEUE n` micro. Never overlaps a slot (D9).

### 6.7 Inventory module (optional)

- **96×232 (P) / 112×284 (T)**, left of the card, gap 16.
- 2 columns × 3 rows, cell **44×44 (P) / 52×52 (T)**, gap 4; icon 40/48; same state machine as 6.5 minus `queued`/`levelup`.
- Header 20 px `ITEMS` micro, and a 24 px gold total at the bottom.
- Hidden entirely when the selected unit has no inventory; `InfoPanel` then widens by **112 (P) / 128 (T)** (module + gap) — the tables in §4 list both widths.

### 6.8 Resource pill

- **148×56 (P) / 156×56 (T)**, radius 12, `panel` 0.80, 1 px `stroke` 0.20.
- Anatomy: icon **32** at x 12; value `Value 24` right-aligned to `w−12` with its baseline at `y+34`; label 12 `text_muted` right-aligned under it at `y+46`. Labels are short nouns (`GOLD`, `LUMBER`, `SUPPLY`) and are never abbreviated into the value.
- States: `normal`, `warning` (value < cheapest visible cost → value `hp_low` + 2 px left accent), `capped` (supply at max → value `gold`), `flash` (delta on gain: 120 ms `ok`).
- Order: gold, lumber, supply (`7/12` as one numeric, no space).

### 6.9 Top chip

- **132×56 (P) / 140×56 (T)**, radius 12, `panel` 0.72.
- 20 px glyph + 14 px label, centred; hotkey micro 12 in the top-right corner.
- States: `normal`, `pressed` (`panel` 0.9, 2 px inset), `disabled` (0.35), `badge` (red dot 8 px top-right).

### 6.10 Clock

- **96×56 (P) / 104×56 (T)**; sun/moon 20 px + `HH:MM` numeric 16.
- States: day, night (glyph swap, no colour change of the text).

### 6.11 Tooltip

- Max width **360**, padding 8, radius 8, `panel` 0.94, 1 px `stroke` 0.20.
- Title 14 `gold` (allowed: it sits on near-black `panel`, 9.6:1), body 12 `text`, cost line 12 `gold`; hotkey 12 `text_muted` right.
- Position: pointer +12/+12, flipped when it would cross the safe edge; never covers the pointer; max height 40 % of screen.
- Timing: 400 ms hover delay, immediate on keyboard focus, hide on press/leave.

### 6.12 Toast

- **560×56**, radius 12, `panel` 0.92, 1 px `danger`/`ok`/`stroke` left accent 4 px, icon 20 + text 18.
- Stack: max 3, 8 px apart, newest on top; `info`/`ok` 3 s, `danger` 5 s; fade 120 ms in/out, slide 8 px.
- Position: centred, `y = safe.top + 72` (below the top bar), never over the console band.

### 6.13 Selection marquee / world widgets

- Out of scope for the HUD layout, but the HUD owns the **drag threshold** (4 px pointer / 12 px touch) so a tap never becomes a 1 px box.

### 6.14 Debug overlay (not shipped)

- Top-left: entity id, profile, safe insets; top-right: FPS, frame ms, draw calls.
- `12` px mono, `#0f0` on `#000` 0.6, in the `Debug` layer, toggled by a build flag. Must not be drawn inside any HUD rect (D8) — place it at `x = safe.left + 8`, `y = safe.top + 64` (below the 56 px top bar), never in the top bar itself.

---

## 7. State machines and timings

| Component | States | Transitions | Timing |
|---|---|---|---|
| Command slot | 9 (6.5) | hover→pressed on down; pressed→normal on up-outside; cooldown→normal at t=0 | press 60 ms, release 120 ms, hover fade 80 ms |
| Resource pill | 4 | normal↔warning on affordability; flash on delta | flash 120 ms |
| Minimap | 4 | idle↔panning on drag; ping one-shot | ping 600 ms |
| Portrait | 3 | normal↔dead on HP; targeted on hover/focus | pulse 1.2 s |
| Toast | 3 tones | push/pop; oldest dropped when >3 | 3 s / 5 s |
| Tooltip | hidden/shown | 400 ms delay; instant on focus | 80 ms fade |

Global rules: no state may change a rect; animations are opacity/scale/UV only; all timings are wall-clock and pause with the game when paused.

---

## 8. Input model

| Gesture | Pointer | Touch | Result |
|---|---|---|---|
| Primary click/tap | LMB | tap | select / activate slot |
| Secondary | RMB | long-press on world (450 ms) | move/attack command |
| Drag on world | LMB drag | drag after 12 px | selection box |
| Drag on minimap | LMB drag | drag | pan camera |
| Wheel / pinch | wheel | 2-finger pinch | zoom |
| Double click/tap | 2×LMB | 2×tap | centre camera on unit |
| Hover | pointer move | — (long-press instead) | tooltip |

- Hotkeys: card grid `Q W E R / A S D F / Z X C V` (row-major), Esc = cancel targeting, Tab = next subgroup, F1–F3 = control groups. Hotkey label must match the bound key, not a fixed string.
- Hit slop: pointer +8 px around each interactive rect; touch 0 (cells are ≥88).
- Input is consumed by the **topmost** interactive rect at the point; the HUD exposes a hit-test list, and the world only receives events not consumed.

---

## 9. Text and localisation

- One string table keyed by id; no literal text in draw calls.
- Truncation: ellipsis at measured width; budget `rect.w − 2·padding − 8`. Cyrillic is ≈8 % wider — budget at 0.92 of the measured Latin width.
- Numbers: integers below 10 000 with a thin-space group separator (`1 200`); ≥10 000 as `12.3k`; HP as `current / max`; supply as `used/max` with no spaces (the slash is the delimiter).
- No text baked into textures; no text drawn without a backing rect (§5.2).
- Right-to-left: out of scope; layout is LTR-only for now (documented, not silently broken).

---

## 10. Asset pipeline

### 10.1 Icons

- Source: 691 `BTN*.blp` at 64×64 (`analysis/war3_full3/ReplaceableTextures/CommandButtons/`), decoded by `wc3-formats::blp` to RGBA8.
- Atlas: **1024×2816**, 16 columns × 44 rows, 64 px cells, no gutter (icons are opaque with a baked border). UV: `u = (i%16)·64/1024`, `v = ⌊i/16⌋·64/2816`.
- Upload `Rgba8UnormSrgb`; sample **nearest** at 1× and 2×, linear only for non-integer UI scale.
- Licence: the atlas is derived art (Blizzard icons) **and** the glyph atlas is a font copy (Nimrod MT / Monotype). Both are built on-device from the user's data and kept in RAM — never written to disk, never committed (`R2_UI_FONT_LICENSING_REFS.md` §2.4).
- Display size: 64 (1×) or 128 (2×). Never 96 (D2 shows 62 px cells reading as mush at 2414).
- Missing icon = hard error naming the file; no placeholder.

### 10.2 BLP decoding

- Paletted BLP1: alpha plane at `mip0_offset + w·h`, 8/4/1-bit packed; **transparent texels keep magenta `#ff00ff` in RGB** — use alpha, never the colour key (`GITHUB_UI_RESEARCH.md` §3.6).
- sRGB data: upload sRGB, no second gamma.

### 10.3 9-slice (panels only)

- Descriptor: `panel_region = [x, y, left_w, top_h, centre_w, centre_h, right_w, bottom_h]` (OpenRA shape) → 9 sprites; skip sides whose flag is absent.
- Insets: dialog 86 px at 1080 (96 × 0.9), edit box 22, clamped to `0.25·min(w,h)` on shallow panels; icon frame is baked at 4 px (never 9-sliced).
- Source of truth for FDF-authored panels: `CornerFlags` + `CornerSize` (our `fdf.rs` already parses `TexCoord`; `stb_fdf.h:203-204` is the reference model).

### 10.4 Console chrome (fixed-slice, authored per profile)

- **Not 9-sliced at runtime.** One texture per profile: `chrome_p_1920.png` (1872×264), `chrome_p_2414.png` (2366×264), `chrome_t_1920.png` (1872×320), `chrome_t_2414.png` (2366×320).
- Built offline from three parts — `LeftFrame` (392 / 460 px), `Field` (stretch), `RightFrame` (card width) — so the stone is authored at the final width and **1 px bevels stay 1 px** (fixes D1).
- The field's stone texture tiles/repeats horizontally; the parapet silhouette is authored per width so the arch never stretches.
- WC3 tiles remain a **runtime fallback skin** (read from the user's MPQ), drawn only if the authored set is missing, and then at 0.9 scale with a warning.
- Alpha: console art has **transparent wells** (minimap, portrait, slots); the renderer draws chrome first, then content through the wells.

### 10.5 Export contract

- PNG, sRGB, no colour profile, no premultiplied alpha; 8-bit.
- Naming: `chrome_<profile>_<width>.png`, `panel_<name>_9s.png`, `icon_atlas_1024x2816.png`.
- Every 1 px feature must be axis-aligned integer pixels; no resampling after export.
- QA: exported texture must be byte-identical on re-export from source (hash recorded).

---

## 11. Acceptance tests (machine-checkable)

Each test renders headlessly at both resolutions and both profiles, then asserts:

| # | Test | Threshold | Fixes |
|---|---|---|---|
| T1 | Every zone rect inside `safe` | 0 violations | D1 |
| T2 | Pairwise overlap of `{topbar, minimap, strip, portrait, info, inventory, card, cardstatus, toast}` | area == 0 | D3, D9 |
| T3 | Every text run inside a panel/scrim rect | 0 violations | D4 |
| T4 | Text fit: `measure(text) ≤ rect.w − 16` | 0 clipped | D5, D6 |
| T5 | Bars: label rect ∩ fill rect == 0 when fill < 50 % | 0 | D5 |
| T6 | Min hit target: `min(w,h) ≥ 88` (T) / `≥ 48` (P) | 0 failures | D2 |
| T7 | Icon integrity: 691 decoded, 64×64, alpha not all-zero, display ∈ {64,128} | exact | D2 |
| T8 | Chrome slice integrity: bevel rows present at the authored y; column diff across the field seam < 2/255 | pass | D1 |
| T9 | Contrast on composited backing ≥ 4.5 (`text`, `text_muted`, `hp_*`), ≥ 3.0 for ≥18 px `gold` | pass | D4 |
| T10 | State coverage: every component renders all states with an unchanged rect (hash of rect) | exact | D2 |
| T11 | Z-order: debug ∉ HUD rects in release; console < content < overlay < toast | pass | D8 |
| T12 | Golden mocks: `mock-console-1920x1080-P.png`, `…-T.png`, `…-2414x1080-P/T.png` | pixel diff ≤ 0.1 % vs accepted | all |

Plus a regression test per defect D1–D10 that renders the failing case and asserts the rule (named `defect_dN_*`).

Targets, not tests: band ≤ 30 % of height, reserved battlefield ≥ 60 % of height, gold pixels ≤ 3 % of chrome.

---

## 12. Decisions and open items

| # | Decision | Status |
|---|---|---|
| 1 | 4×3 command card (was 4×2; `mobile_hud` now ships 4×3 as of 2026-11-13T07:58Z — `PARITY_MATRIX.md` P4/P8) | **decided** — WC3 grammar, 12 slots, room for abilities + commands |
| 2 | No virtual joystick on the console profile | **decided** — pointer/touch drag on world; joystick only in the phone profile |
| 3 | Resources top-right (not above the card, as in the mock) | **decided** — the card owns its band; affordability is read from the slot cost text, not eye travel |
| 4 | Console chrome authored per profile, WC3 tiles fallback only | **decided** — 1.51× stretch is unacceptable (D1) |
| 5 | Inventory 2×3 left of the card, optional | **decided** — matches WC3, keeps the band fixed |
| 6 | Portrait 2-D render vs live 3-D | **open** — spec assumes a 96/112 px render target; 3-D needs a scissor + camera, add 1 draw |
| 7 | Gold text ≥18 px only | **decided** — measured 4.26:1 on the worst-case scrim |
| 8 | Toast below the top bar vs above the console | **decided** — top; the console band is fully occupied |
| 9 | Phone portrait stays on `DESIGN_SPEC.md` | **decided** — no merge of the two layouts |

Open input needed: item 6 only (art pipeline owner), everything else is specified.

---

## 13. Ownership map (no code in this change)

| Concern | File | Action |
|---|---|---|
| Tokens (colour/type/spacing) | `crates/wc3-game/src/style.rs` | extend with §3 values; keep existing panel/bar/knob API |
| Rect/text primitives | `crates/wc3-game/src/hud.rs` | reuse `Rect`, `inset`, `intersects`, `text_box`, `truncate` |
| Console layout + draw | **new** `crates/wc3-game/src/console_hud.rs` | implement §4–§6; `SafeArea`, `ConsoleProfile` inputs |
| Hit testing | same file | expose `ConsoleLayout` with named rects + `hit(x,y) -> Zone` |
| Wiring | `crates/wc3-game/src/game.rs` | select profile by input source; feed `HudView` |
| Chrome texture | `crates/wc3-game/src/assets.rs` | load `chrome_<profile>_<width>.png`; WC3 fallback |
| Tests | `crates/wc3-game/tests/console_hud.rs` | T1–T12 |
| Art | `analysis/agent-hud/` | export contract §10.5 |

No change is required to `mobile_hud.rs`; the two profiles are separate modules selected at runtime.

---

### Quick reference (the numbers an implementer needs first)

```
safe          top 72, bottom 24, left 24, right 24
top bar       h 56 at y=safe.top; chips 132/140×56; pills 148/156×56; clock 96/104×56
console band  264 (P) / 320 (T), bottom at 1056
minimap       216/260 frame, 200/244 window, strip 44/48
portrait      120×160 (P) / 140×184 (T), art 96/112
card          312×232 (P) 4×3 @72 gap 8 | 382×284 (T) 4×3 @88 gap 10
inventory     2×3, 44/52 cells, left of card (optional)
info panel    flex; rows 30/20/20/14/32/44/16, scrim 0.72→0
icons         64 native, display 64 or 128, atlas 1024×2816
gold          ≤3 % of chrome, text ≥18 px only
```
