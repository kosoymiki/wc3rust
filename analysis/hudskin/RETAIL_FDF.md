# Retail console geometry — FDF reference + shipped-tile measurements

Status: **reference / cross-check only**. The shipped HUD design contract is the
final asset pack (ledger 457, manifest-driven layout). This document records what
Blizzard's own UI data and shipped art say about the console, so the pack's design
units can be checked against retail instead of guessed.

Provenance (all observed on 2026-11-16 unless marked otherwise):

| Source | Path | How read |
|---|---|---|
| FDF documents | `analysis/war3_full3/UI/FrameDef/UI/*.fdf` (dump of `War3.mpq`, RoC copy identical: `diff -q` = identical) | file text |
| Skin table | `analysis/war3_data/UI/war3skins.txt` | file text (`ConsoleTexture01..04` per race) |
| Shipped tiles | `analysis/war3_full3/UI/Console/{Human,Orc,Undead,NightElf}/*UITile0{1..4}.blp` | `wc3-formats` `blp_info` decode (BLP1, 512×512 / 64×512) |
| Tile frame measurements | decoded RGBA (alpha key = `(255,0,255)` in the dump) | connected-component analysis of transparent regions, gold-border scan |

## 1. Unit space

WC3 in-game frames use **0.8 UI units = screen width** and **0.6 UI units = screen
height** (the authoring space is 4:3, where one unit is square).

* `px_per_unit_x = screen_w / 0.8`, `px_per_unit_y = screen_h / 0.6`.
* On a 16:9 screen the UI is stretched horizontally by `(w/0.8)/(h/0.6) = 1.6764`
  at 2414×1080 (1.3333 at 1920×1080, 1.0 at 4:3). **Inferred** from the shipped
  data: the four bottom tiles sum to exactly `0.256 + 0.256 + 0.256 + 0.032 = 0.8`
  units, so they only tile the screen edge-to-edge if the X unit is `w/0.8`
  (widescreen retail behaviour; not re-verified against a retail 16:9 capture).
* At 2414×1080: `1 unit_x = 3017.50 px`, `1 unit_y = 1800.00 px`.

## 2. `UI\FrameDef\UI\ConsoleUI.fdf` — the console itself

Nine textures, no frames. Texture order in the file = draw order. FDF
`TexCoord = left, right, top, bottom` (verified against the art: the top slices are
the 64-row strip with the black wells).

### 2.1 Bottom band (4 tiles)

| # | File | Width | Height | TexCoord (l,r,t,b) | Anchor | Rect @2414×1080 | Rect @1920×1080 |
|---|---|---|---|---|---|---|---|
| 6 | ConsoleTexture01 | 0.256 | 0.176 | 0, 1, 0.3125, 1 | BOTTOMLEFT 0, 0 | x 0.00–772.48, y 763.20–1080 | x 0.00–614.40, y 763.20–1080 |
| 7 | ConsoleTexture02 | 0.256 | 0.150 | 0, 1, 0.4140625, 1 | BOTTOMLEFT 0.256, 0 | x 772.48–1544.96, y 810.00–1080 | x 614.40–1228.80, y 810.00–1080 |
| 8 | ConsoleTexture03 | 0.256 | 0.176 | 0, 1, 0.3125, 1 | BOTTOMRIGHT −0.032, 0 | x 1544.96–2317.44, y 763.20–1080 | x 1228.80–1843.20, y 763.20–1080 |
| 9 | ConsoleTexture04 | 0.032 | 0.176 | 0, 1, 0.3125, 1 | BOTTOMRIGHT 0, 0 | x 2317.44–2414.00, y 763.20–1080 | x 1843.20–1920.00, y 763.20–1080 |

* Band height `0.176 units = 316.8 px @1080` (29.3 % of H). Middle tile is
  `0.150 units = 270.0 px` tall and starts 46.8 px lower, but its slice starts at
  `v 0.4140625`, so the tile seams line up exactly (both sample 0.9 px/px).
* Tile source size is **512×512** (tiles 01–03) and **64×512** (tile 04), all four
  races — measured from the BLP headers. On screen: x scale `772.48/512 = 1.50875`,
  y scale `316.8/352 = 0.9` at 2414×1080.
* The middle tile is **mostly transparent** (23–29 % opaque): the open middle is
  where the unit info panel and the control-group buttons live, drawn by native
  GameUI over the world. A solid background plate there is a design decision, not
  a retail constraint.

### 2.2 Top strip (5 textures, `0.032 units = 57.6 px` tall)

| # | File | Width | TexCoord (l,r,t,b) | Anchor | Rect @2414×1080 |
|---|---|---|---|---|---|
| 1 | ConsoleTexture01 | 0.256 | 0, 1, 0, 0.125 | TOPLEFT 0, 0 | x 0.00–772.48 |
| 2 | ConsoleTexture02 | 0.087 | 0, 0.33984375, 0, 0.125 | TOPLEFT 0.256, 0 | x 772.48–1035.00 |
| 3 | ConsoleTexture02 | 0.053 | 0.79296875, 1, 0, 0.125 | TOPRIGHT −0.288, 0 | x 1385.03–1544.96 |
| 4 | ConsoleTexture03 | 0.256 | 0, 1, 0, 0.125 | TOPRIGHT −0.032, 0 | x 1544.96–2317.44 |
| 5 | ConsoleTexture04 | 0.032 | 0, 1, 0, 0.125 | TOPRIGHT 0, 0 | x 2317.44–2414.00 |

The 0.116-unit gap between pieces 2 and 3 (`x 1035.00–1385.03`, 350 px) is the
resource-bar well; the bar frame itself is wider and overlaps pieces 2/4 (§3).

## 3. Other in-game FDFs that place console controls

Only these files exist in the RoC/TFT listfile (72 files under `UI\FrameDef\`, all
enumerated from `analysis/war3_mpq_listing.txt`). `CommandBar.fdf`, `Minimap.fdf`,
`SimpleHeroBar.fdf` and `InventoryPanel.fdf` **do not exist** in `War3.mpq` /
`War3x.mpq` / `War3xlocal.mpq` — the command card, minimap, portrait and inventory
are native GameUI code placed against the baked tile frames (§4). Do not look for a
file that is not there.

### 3.1 `ResourceBar.fdf`

| Element | Width | Height | Offset | @2414×1080 |
|---|---|---|---|---|
| `ResourceBarFrame` | 0.33828125 | 0.0218 | (native anchor, top centre) | 1020.76 × 39.24 px |
| Gold icon | 0.01640625 | 0.01640625 | TOPLEFT 0.0000, −0.003125 | 49.51 × 29.53 px |
| Lumber icon | 0.01640625 | 0.01640625 | TOPLEFT 0.0859375, −0.003125 | +259.32 px from gold |
| Supply icon | 0.01640625 | 0.01640625 | TOPLEFT 0.17265625, −0.003125 | +520.99 px from gold |
| Value text | width 8 chars, `MasterFont` 0.01 | height 0.01640625 | TOPRIGHT −(0.26484375 / 0.178125 / 0.09140625), −0.003125 | right-aligned per column |
| Upkeep text | 0.07578125 | 0.01640625 | TOPRIGHT −0.003125, −0.003125 | 228.67 × 29.53 px, centred |

### 3.2 `UpperButtonBar.fdf` (top-left button row)

| Element | Size | Notes |
|---|---|---|
| `UpperButtonBarFrame` | width 0.34 | button row anchor |
| Button template | 0.085 × 0.022 units | = 256.49 × 39.60 px @2414×1080 |
| Quests / Menu / Allies / Log | `SetPoint LEFT` chain, first at TOPLEFT +0.002 | 4 buttons |

### 3.3 `SimpleInfoPanel.fdf` / `InfoPanelUnitDetail.fdf` (unit info text)

Relative to the info panel's top edge (`InfoPanelUnitDetail` is `SetAllPoints`; the
native code anchors it to the console, the FDF only fixes the inner offsets):

| Element | Offset | Font |
|---|---|---|
| `NameValue` / `SimpleNameValue` | TOP, 0 | InfoPanelTextFont 0.013, centred |
| `SimpleHeroLevelBar` | below name, −0.0015 | height 0.015625 |
| `SimpleProgressIndicator` | below name, −0.0015 | height 0.015625 |
| `ClassValue` | below name, −0.0055 | 0.009, centred |
| `DefenseLabel` | TOPLEFT +0.001875, −0.035625 | 0.009, left |
| `DefenseValue` | left of label +0.04625 | 0.009 |
| `AttackLabel1/2` | 0.001 units apart, stacked | 0.009 |
| `SpeedTitle` | +0.1 units right of DefenseLabel | 0.009 |
| Attribute icons | 0.032 × 0.032, bottom-left +0.0025 | 4 icons, +0.00234375 spacing |

## 4. Baked control frames in the shipped tiles (measured)

Transparent holes = the map, the portrait and the card cells; opaque art = the
inventory well. Coordinates are **source px in the named tile**; screen px are at
2414×1080 with the §2.1 tile transform. The measured minimap well and card grid
rects are identical across all four races; the portrait arch differs per race.

### 4.1 Minimap (tile 01)

| Frame | Source px | @2414×1080 |
|---|---|---|
| Well (map hole) | x 20–295, y 276–496 | x 30.2–445.1, y 867.6–1065.6 (414.9 × 198.0) |
| Header band (location text) | x 0–308, y 219–266 | x 0–464.7, y 816.3–858.6 (42.3 tall) |
| Frame outer edge | flush to screen bottom/left | — |

The map hole is 1.25:1 in UI units (0.1375 × 0.110 units); on 16:9 it is stretched
to 2.1:1 on screen exactly like the rest of the console art.

### 4.2 Minimap button column (tile 01)

| Button | Source px | @2414×1080 |
|---|---|---|
| 1 | x 311–354, y 224–268 | x 469.2–534.1, y 820.8–860.4 |
| 2 | x 311–354, y 268–312 | x 469.2–534.1, y 860.4–900.0 |
| 3 | x 311–354, y 312–356 | x 469.2–534.1, y 900.0–939.6 |
| Round | x 309–355, y 367–408 | x 466.2–535.6, y 949.5–986.4 |

### 4.3 Portrait arch (tiles 01+02, per race)

| Race | Source px | @2414×1080 |
|---|---|---|
| Human | x 431–511 + 0–67 (tile 02), y 289–448 | x 650.3–873.6, y 879.3–1022.4 (223.3 × 143.1) |
| Orc | x 433–511 + 0–64, y 299–450 | x 653.3–869.0, y 888.3–1024.2 (215.8 × 135.9) |
| Undead | x 434–511 + 0–65, y 290–448 | x 654.8–870.6, y 880.2–1022.4 (215.8 × 142.2) |
| Night Elf | x 435–511 + 0–66, y 282–449 | x 656.3–872.1, y 873.0–1023.3 (215.8 × 150.3) |

Two ornamental ledges sit under the arch at y 456–478 and 485–507 (both tiles);
they are decoration, not control wells.

### 4.4 Hero inventory (tile 03, left)

Opaque backpack art; 2 columns × 3 rows.

| Item | Source px | @2414×1080 |
|---|---|---|
| Grid outer frame | x 1–154, y 249–505 | x 1546.5–1777.4, y 843.3–1073.7 |
| Slot frame | x 3–71 / 83–151 (68 wide), rows at y 282 / 359 / 436 (77 pitch) | 102.6 × 69.3 per slot |
| Backpack art inside the frame | ≈62 × 68 (e.g. x 6–67, y 284–351) | ≈93.5 × 61.2 |
| Pitch | 80 × 77 | 120.7 × 69.3 |

### 4.5 Command card (tile 03 + tile 04, right)

Transparent cells, 4 columns × 3 rows; the 4th column is split by the
tile03/tile04 boundary at source x 512.

| Item | Source px | @2414×1080 |
|---|---|---|
| Grid | x 209–548, y 245–499 | x 1860.3–2371.8, y 839.7–1068.3 (511.5 × 228.6) |
| Cell | 80 × 80 | 120.7 × 72.0 |
| Pitch | 87 × 87 (7 px gap) | 131.3 × 78.3 |
| Row rects | y 245–325, 332–412, 419–499 | y 839.7–911.7, 918.0–990.0, 996.3–1068.3 |
| Column rects | x 209–289, 296–376, 383–463, 470–550 | x 1860.3–1980.9, 1991.4–2112.1, 2122.9–2243.6, 2254.4–2374.9 |

### 4.6 Top strip wells (resource slots)

| Tile | Source px | @2414×1080 |
|---|---|---|
| 01 | 3 wells x 6–170 / 175–340 / 345–510, y 2–38 | x 9.1–256.5 / 264.0–513.0 / 520.5–769.5, y 1.8–34.2 |
| 03 | 3 wells x 69–223 / 243–397 / 417–511, y 3–39 | x 1649.1–1881.5 / 1911.6–2144.0 / 2174.1–2317.4, y 2.7–35.1 |
| 04 | well x 0–60, y 3–38 | x 2317.4–2408.0, y 2.7–34.2 |
| 02 | medallion (upkeep) x 245–330, y 4–63 | x 1142.1–1268.4, y 3.6–57.6 |

## 5. Retail vs pack design units (the cross-check 457 asked for)

Retail numbers from §4; pack numbers are the contract as stated by the dispatcher
(64 px cells at a 768 px design height, 9-slice manifest). Percentages are of
screen height; @1080 is `pct × 1080`.

Sizes are width × height (same order as §4); the @1080 column is the height in px
and the parenthesised number is the width at 2414 px (16:9 stretch).

| Control | Retail (UI units) | Retail % of H | Retail @1080 (h; w@2414) | Pack (design px) | Pack % of H | Pack @1080 | Pack/retail |
|---|---|---|---|---|---|---|---|
| Command card cell | 0.0400 × 0.0400 (80 src px) | 6.67 % | 72.0 (120.7) | 64 / 768 | 8.33 % | 90.0 | 1.25× |
| Command card pitch | 0.0435 | 7.25 % | 78.3 (131.3) | — | — | — | — |
| Command card grid 4×3 | 0.1695 × 0.1275 | 21.25 % | 228.6 (511.5) | 4×64 + gaps | — | — | — |
| Inventory cell | 0.0340 × 0.0385 (68×77 src) | 6.42 % | 69.3 (102.6) | (manifest) | — | — | — |
| Inventory pitch | 0.0400 × 0.0385 | 6.42 % | 69.3 (120.7) | — | — | — | — |
| Minimap well | 0.1375 × 0.1100 | 18.33 % | 198.0 (414.9) | (manifest) | — | — | — |
| Portrait arch (Human) | 0.0740 × 0.0795 | 13.25 % | 143.1 (223.3) | (manifest) | — | — | — |
| Minimap button | 0.0220 sq | 3.67 % | 39.6 (66.4) | — | — | — | — |
| Top strip | 0.032 | 5.33 % | 57.6 (full width) | — | — | — | — |
| Console band | 0.176 | 29.33 % | 316.8 (full width) | — | — | — | — |

Reading: the pack's 64/768 cell is **1.25× retail linear** (1.56× area) and the
current shipped mobile HUD uses 95 px = 8.80 % H = **1.32× retail**. The pack is
deliberately larger than retail for touch; retail geometry is the *floor and
proportion reference*, not the pack's target. The 0.038–0.039-unit figure quoted
elsewhere (0.039 is the Warsmash `DEFAULT_COMMAND_CARD_ICON_WIDTH`, reported in
`analysis/agent-hud/GITHUB_UI_RESEARCH.md`, not re-verified today) is within 3 % of
the shipped-art measurement (0.040) — treat 0.040 as the measured value.

## 6. Fallback notes for races the pack may not cover

* All four races ship the same console geometry (minimap well, card grid, tile
  sizes); only the ornamentation and the portrait arch differ (§4.3). A race-neutral
  frame layout with per-race art is therefore safe.
* Skin keys: `war3skins.txt` `[Default]` maps `ConsoleTexture01..04` to the Human
  tiles; `[Human]/[Orc]/[NightElf]/[Undead]` override them (observed lines 248–251,
  20–23, 55–58, 107–110, 166–169). `UiTheme::load_with` loads the tiles once under
  the active `[Default]` section and aliases them `HumanUITile01..04`
  (`ui.rs:466-488`), so `Assets::ui.texture("HumanUITile01")` is the Human tile even
  after `set_race("Orc")` — race tiles must be loaded explicitly if the pack does
  not replace them.
* **Dimension trap:** when `analysis/agent-hud/redraw-v0.1/blp` is the active
  layer (desktop runs), the `HumanUITile01..04` aliases resolve to the
  **256×288 procedural pack tiles** (BLP header observed 256×288, 8-bit alpha), not
  the 512×512 retail tiles. Retail tile geometry (this document) must never be
  applied to those. `game.rs::hud_redraw_skin` already gates on
  `width == 256 && height == 288`; any new console draw should use the same test.
* Tile 04 is 64×512 (not 512×512) in every race; its x scale on screen equals the
  other tiles (`96.56/64 = 1.50875`), so the 4th card column continues seamlessly.

## 7. Reproduction

```bash
# tile dimensions + alpha histogram (retail dump, 512x512 / 64x512)
scripts/h2-cargo run -q -p wc3-formats --example blp_info --offline -- \
  analysis/war3_full3/UI/Console/Human/HumanUITile01.blp /tmp/tile01.ppm

# FDF sources (all 72 files under UI\FrameDef\)
grep -iE "FILE UI.FrameDef" analysis/war3_mpq_listing.txt | sort -u

# absence check (must print nothing)
grep -ih "FILE.*CommandBar\|FILE.*Minimap\.fdf\|FILE.*InventoryPanel\|FILE.*SimpleHeroBar" \
  analysis/war3_mpq_listing.txt analysis/agent-tft/war3x_listing.txt \
  analysis/agent-tft/war3xlocal_listing.txt
```

The frame measurements in §4 were produced by decoding the BLP to RGBA and taking
the bounding boxes of the alpha-keyed regions (map/portrait/card cells) and the
gold-border runs (buttons, inventory wells) at the mapping in §1/§2.1. They are
`observed`; the pack comparison (§5) mixes `observed` (retail) with
`reported-unverified` (pack design numbers as given by the dispatcher).
