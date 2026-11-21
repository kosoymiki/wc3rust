# WC3 console fonts and text metrics — reference (2026-11-13)

Spec-only research note. Answers three questions for the console HUD: **what sizes WC3 actually uses**, **how the glyphs reach the screen (atlas or TTF)**, and **which colours/shadows the FDF corpus declares** — with measured numbers, not estimates.

Companion docs: `CONSOLE_UI_SPEC.md` §3.2 (our type scale), `GITHUB_UI_RESEARCH.md` §3 (console geometry), `UI_MERGE_MATRIX.md` (what of the UI layer is merged).

---

## 0. Method

| Evidence | Detail |
|---|---|
| Local corpus | 70 `*.fdf` under `analysis/war3_full/UI/FrameDef/`, parsed for `Font`, `FontColor`, `FontShadowColor`, `FontShadowOffset`, `FontJustification*`, `TextLength` — **91 size declarations, 175 `FontColor` declarations, 34 shadow colours + 34 shadow offsets, 221 justification lines** |
| Local assets | `analysis/war3_full/Fonts/` (6 TTF files) and `UI/war3skins.txt` `[Default]` font block |
| TTF metrics | parsed directly from `head`/`hhea`/`maxp`/`cmap`/`loca`/`glyf`/`hmtx` tables (no external tool) |
| Engine code | `WarsmashModEngine` `parsers/fdf/GameUI.java:362-369`; `corepunch/open-realm` `renderer/r_font.c:52-88` |
| Web refs | access date **2026-11-13**, stealth-browser MCP (headless Chromium 157, Termux aarch64). Quoted only what was actually read; a page that merely loaded is marked `reachable`. |
| Status | no code changed; no device run |

---

## 1. There is no bitmap font atlas in the MPQ

WC3 classic ships **TrueType fonts only** and rasterises glyphs at the requested FDF size.

`UI/war3skins.txt` `[Default]` (lines 225-230):

| Skin key | File |
|---|---|
| `MasterFont` | `Fonts\FRIZQT__.TTF` |
| `MessageFont` | `Fonts\FRIZQT__.TTF` |
| `ChatFont` | `Fonts\FRIZQT__.TTF` |
| `EscMenuTextFont` | `Fonts\FRIZQT__.TTF` |
| `InfoPanelTextFont` | `Fonts\FRIZQT__.TTF` |
| `TextTagFont` | `Fonts\FRIZQT__.TTF` |

`Fonts/` contents (observed, sizes on disk):

| File | Size | Role |
|---|---|---|
| `FRIZQT__.TTF` | 140.3 KB | **the UI font** — `name` table says **Nimrod MT** (Monotype 1991-1999), *not* Friz Quadrata despite the filename; Latin + Cyrillic |
| `NIM_____.ttf` | 140.3 KB | **same face as FRIZQT** — identical `name` table (`Nimrod MT`) and metrics (upem 2048, asc 2333 / desc −516, 661 glyphs) |
| `DFHeiMd.ttf` | 2.3 MB | CJK fallback |
| `dffn_b31.ttf` | 4.2 MB | CJK fallback |
| `dfst-m3u.ttf` | 5.9 MB | CJK fallback |
| `tt5500m_.ttf` | 4.8 MB | CJK fallback |

Two independent engine reimplementations confirm runtime rasterisation:

- **Warsmash** (AGPL, Java): `GameUI.java:362-369` — `generateFont(fdfFontSize)` sets `FreeTypeFontParameter.size = (int) convertY(viewport, fdfFontSize)`, regenerates a `BitmapFont` per size, and falls back to `size = 128` when the converted size is 0.
- **open-realm** (MIT, C): `renderer/r_font.c:52-88` — `stbtt_BakeFontBitmap(..., font->size * s, ...)` into a **256×256 atlas per 256-codepoint glyph set**, `#define MAX_GLYPHSET 256`, `#define FONT_SCALE 2` (rasterise at 2× and downscale), `yoff += scaled_ascent`, `xadvance = floor(...)`.

**Consequence for us:** the "font atlas" is a runtime cache keyed by `(face, pixel size)`, not a shipped asset. Any atlas spec is ours to choose; there is no donor atlas to match.

**Licence consequence** (`R2_UI_FONT_LICENSING_REFS.md` §2): the face is **Nimrod MT under Monotype** — "You may not copy or distribute this software." An atlas built offline from the TTFs is a copy of the font and may **not** be committed, bundled or published; it must be built on-device from the user's MPQ and kept in RAM.

---

## 2. Size scale (measured over the 70-FDF corpus)

FDF sizes are **fractions of 0.6·H**; pixels = `size / 0.6 · H` (`GameUI.java:1580-1585`).

| FDF value | px @1600×1200 | **px @1080** | Count | Where it is used (examples) |
|---|---|---|---|---|
| 0.0080 | 16.0 | **14.4** | 1 | `UI/ReplayPanel.fdf:8` |
| 0.0085 | 17.0 | **15.3** | 2 | `UI/SimpleInfoPanel.fdf:34,69` — info-panel **labels** |
| 0.0090 | 18.0 | **16.2** | 6 | `UI/InfoPanelTemplates.fdf:38,51`, `QuestDialog.fdf:266` — **attributes** |
| 0.0100 | 20.0 | **18.0** | 4 | `UI/ResourceBar.fdf:8,52` — **resource values**; `SimpleInfoPanel.fdf:19` — description |
| 0.0110 | 22.0 | **19.8** | **42** | `ChatDialog.fdf:129`, `EscMenuTemplates.fdf:118,144` — **dominant body size** |
| 0.0130 | 26.0 | **23.4** | 22 | `EscMenuMainPanel.fdf:9`, `SimpleInfoPanel.fdf:9` — **titles** |
| 0.0150 | 30.0 | **27.0** | 13 | `EscMenuTemplates.fdf:131,424`, `BattleNetTemplates.fdf:214` — **headers** |
| 0.0240 | 48.0 | **43.2** | 1 | `Glue/CampaignMenu.fdf:36` — banner |

Ladder relative to the dominant 0.011 body (19.8 px @1080): label 0.009 = **0.82×**, description 0.010 = **0.91×**, title 0.013 = **1.18×**, header 0.015 = **1.36×**. WC3 has no long size ramp — it has one body size and three accents.

Notes:
- The **resource bar uses 0.010 → 18 px @1080**, i.e. WC3 resource values are *smaller* than our console spec's 24 px `Value` role. If fidelity matters more than glanceability, drop the pill value to 20 px.
- 42 of 91 declarations sit on 0.011 — WC3 text is almost all one size; hierarchy comes from colour and position, not from many sizes.
- `TextLength` (a character budget for width) appears **once** in the whole corpus (`= 8`), so widths are measured, not declared.

---

## 3. FRIZQT__.TTF metrics (measured from the file)

| Metric | Value | Ratio |
|---|---|---|
| unitsPerEm | 2048 | — |
| Cap height (`H` yMax) | 1513 | **0.7388 em** |
| x-height (`x` yMax) | 1104 | **0.5391 em** |
| hhea ascender / descender / lineGap | 2333 / −516 / 0 | **line = 1.3911 em** |
| Glyphs | 661 | — |
| Digit advance (0–9) | 1024 each | **0.5000 em, tabular** |
| `H` advance | 1665 | 0.8130 em |
| `x` advance | 1194 | 0.5830 em |
| Coverage | Latin ✓, Cyrillic `U+0410` ✓, CJK ✗, `₽` ✗ | — |

Pixel consequences (cap height / x-height / natural line at 1.3911 em):

| px | cap | x | line |
|---|---|---|---|
| 14 | 10.3 | 7.5 | 19.5 |
| 16 | 11.8 | 8.6 | 22.3 |
| 18 | 13.3 | 9.7 | 25.0 |
| 20 | 14.8 | 10.8 | 27.8 |
| 22 | 16.3 | 11.9 | 30.6 |
| 24 | 17.7 | 12.9 | 33.4 |
| 26 | 19.2 | 14.0 | 36.2 |
| 28 | 20.7 | 15.1 | 39.0 |
| 30 | 22.2 | 16.2 | 41.7 |

Two facts that change implementation:

1. **Digits are tabular by construction** (exactly 0.5 em). HP `200 / 200` and resources do not need a mono fallback, and `measure()` can approximate numerals as `0.5·px` per digit.
2. **Natural line height is 1.391 em**, not 1.2. `CONSOLE_UI_SPEC.md` §3.2 uses 1.25 line heights; that is a deliberate tightening. If WC3 fidelity is wanted, 1.39 em is the donor value (22 px body → 30.6 px line, which is exactly the 30 px row the FDF anchors assume).

---

## 4. Colours and shadows (measured)

### 4.1 `FontColor` (125 declarations)

| RGBA | Hex | Count | Role |
|---|---|---|---|
| `1.0, 1.0, 1.0, 1.0` | `#ffffff` | **105** | body/values (default) |
| `0.99, 0.827, 0.0705, 1.0` | **`#fcd312`** | **29** | gold labels / descriptions (the WC3 "gold text") |
| `0.764, 0.764, 0.764, 1.0` | `#c3c3c3` | 19 | muted/secondary |
| `1,1,1` (3-component) | `#ffffff` | 6 | body, alpha omitted |
| `0.7,0.7,0.7` | `#b3b3b3` | 6 | muted |
| `0.2,0.2,0.2,1.0` | `#333333` | 4 | **disabled** |
| `1.0, 0.83, 0.07` | `#ffd412` | 1 | gold variant (button highlight text) |
| `1.0, 0.0, 0.0` / `1,0,0,1` | `#ff0000` | 2 | error/warning |
| `0.8, 0.64, 0.0` | `#cca300` | 1 | dark gold |
| `1.0, 0.86, 0.28` | `#ffdb47` | 1 | light gold |
| `0.4, 0.4, 0.4` | `#666666` | 1 | disabled variant |

Gold `#fcd312` on the near-black panels is ≈ 12:1 — WC3's small gold text is safe **because it sits on dark panels**, which is exactly the rule `CONSOLE_UI_SPEC.md` §3.1 encodes (gold ≥18 px on scrim, any size on `panel`).

### 4.2 Shadows (34 declarations)

| Property | Value | Count | px @1600×1200 |
|---|---|---|---|
| `FontShadowColor` | `0,0,0,0.9` | 31 | — |
| `FontShadowColor` | `0,0,0,1.0` | 3 | — |
| `FontShadowOffset` | `0.002, -0.002` | 18 | **(4, −4)** |
| `FontShadowOffset` | `0.001, -0.001` | 16 | **(2, −2)** |

Offsets are converted with `convertX/convertY` (`GameUI.java:658-659`), so the design-pixel offset is `0.002/0.8·1600 = 4 px` horizontally and `0.002/0.6·1200 = 4 px` vertically — a **hard drop shadow of 2–4 px at 1600×1200 (1.8–3.6 px at 1080)**, not a 1 px outline. That is why WC3 text stays legible on stone without a panel.

### 4.3 Justification (221 `FontJustification*` lines)

Of these, 212 use a named constant: `JUSTIFYRIGHT` 59 · `JUSTIFYLEFT` 55 · `JUSTIFYCENTER` 39 · `JUSTIFYMIDDLE` 35 · `JUSTIFYBOTTOM` 17 · `JUSTIFYTOP` 7. Right-alignment is the most common single choice — consistent with resource and HP values being right-aligned numbers.

---

## 5. Atlas / rasterisation contract (what an implementation must specify)

Derived from the two engines and the font metrics; this is the part WC3 leaves to the engine.

| Parameter | Recommendation | Why |
|---|---|---|
| Raster size | `round(fdf_size/0.6·H)`; at 1080 use **14/16/18/22/26/30** | matches the corpus ladder (§2) |
| Scale factor | rasterise at **2×** and sample down (open-realm `FONT_SCALE 2`) | keeps 1 px stems and the 2–4 px shadow clean on a 2× display |
| Atlas page | **1024×1024, A8 or RGBA8**, shelf packer | ~255 glyphs (ASCII + Latin-1 + Cyrillic): at 44 px a page holds 22×22 = 484 slots, at 60 px 16×16 = 256 — one page per raster size is enough |
| Padding | **1 px** per glyph + 1 px gutter | prevents bilinear bleed on neighbouring glyphs |
| Filtering | linear min/mag, no mipmaps while scale = 1 | text is drawn at its raster size |
| Shadow | draw the glyph twice (offset `0.06 em` ≈ 1.3 px at 22) **or** bake 4 px/90 % black as WC3 does | matches §4.2; separate pass avoids duplicating glyphs |
| Digits | use the font's tabular figures (FRIZQT is exactly 0.5 em) | no jitter in HP/resource text |
| Coverage | Latin + Cyrillic from FRIZQT; CJK from `DFHeiMd`/`dfst-m3u` fallback chain | measured: FRIZQT has U+0410, not U+4E2D |
| Upload | sRGB texture, alpha in A or RGBA; premultiply off | consistent with `CONSOLE_UI_SPEC.md` §10.2 |
| SDF option | `msdf-atlas-gen` (padding + `-pxpadding`, outer padding flags) if text must scale smoothly | only if UI scale becomes continuous; not needed for fixed 1080 |

Budget: one 1024×1024 page per raster size (5 sizes at 2×: 28/36/44/52/60 for display 14/18/22/26/30), or a single 2048×1024 atlas for all five if the renderer prefers one binding.

---

## 6. Mapping to our two specs

| Role | WC3 @1080 (measured) | `CONSOLE_UI_SPEC` §3.2 | Delta |
|---|---|---|---|
| Banner (campaign menu only) | 43.2 px (0.024) | — | not used in-game |
| Menu header | 27.0 (0.015) | Title 22 | −5 px |
| Unit name / dialog title | 23.4 (0.013) | Display 28 | +4.6 px |
| Body | 19.8 (0.011) | 18 px | −1.8 px |
| Resource value | 18.0 (0.010) | 24 px | **+6 px (deliberate)** |
| Label | 15.3–16.2 (0.0085–0.009) | 14 px | −1.5 px |
| Micro | 14.4 (0.008) | 12 px | −2.4 px |

Verdict: measured against WC3, our scale **magnifies the unit name** (28 vs 23.4) and **shrinks every other role** to 0.81–0.92× (title 22 vs 27, body 18 vs 19.8, label 14 vs 15.3, micro 12 vs 14.4). That widens the gap between the name and the rest — the right call for a 1080p tablet at arm's length. Keep it, but two donor facts are worth importing verbatim: **tabular digits** (FRIZQT: 0.5 em exactly) and the **2–4 px black drop shadow** (our 1 px is thinner than WC3's).

---

## 7. References

| # | Source | URL / path | Access | Read |
|---|---|---|---|---|
| 1 | Warsmash `GameUI.java` (AGPL-3.0) | `raw.githubusercontent.com/Retera/WarsmashModEngine/master/core/src/.../parsers/fdf/GameUI.java` | 2026-11-13 | `:362-369` FreeType per-size font; `:1573-1585` size→px conversion; `:658-659` shadow offset conversion |
| 2 | open-realm `r_font.c` (MIT) | `raw.githubusercontent.com/corepunch/open-realm/main/renderer/r_font.c` | 2026-11-13 | `:10-13` `MAX_GLYPHSET 256`, `FONT_SCALE 2`; `:52-88` `stbtt_BakeFontBitmap` 256×256, yoff/xadvance fix-up |
| 3 | Local `war3skins.txt` | `analysis/war3_full/UI/war3skins.txt:225-230` | 2026-11-13 | six font roles → `Fonts\FRIZQT__.TTF` |
| 4 | Local FDF corpus | `analysis/war3_full/UI/FrameDef/**/*.fdf` (70 files) | 2026-11-13 | 91 sizes, 125 colours, 34 shadows, 212 justifications |
| 5 | Local fonts | `analysis/war3_full/Fonts/*.ttf` | 2026-11-13 | 6 faces; FRIZQT/NIM metrics parsed from tables |
| 6 | `stb_truetype.h` v1.28 | `raw.githubusercontent.com/nothings/stb/master/stb_truetype.h` | 2026-11-13 | header quote: "bitmaps with antialiasing (box filter) … SDF bitmaps"; public domain |
| 7 | `msdf-atlas-gen` | `github.com/Chlumsky/msdf-atlas-gen` | 2026-11-13 | README: `-pxpadding`, `-outerpxpadding` flags (quoted) |
| 8 | libGDX `FreeTypeFontParameter` | `javadoc.io/doc/com.badlogicgames.gdx/gdx/latest/.../FreeTypeFontParameter.html` | 2026-11-13 | **reachable only** — page title "gdx 1.14.2 javadoc"; field list not extracted, so not quoted |
| 9 | `Tasyen/FDF` (MIT) | `github.com/Tasyen/FDF` | 2026-11-13 | reachable; used for the local FDF vocabulary, not for these numbers |

Limitations: the web pages in rows 7–9 are JS-rendered; only rows 1–2 and 6 produced quotable text. Everything numeric in §2–§4 comes from local files, which are the primary source (`observed`), not from secondary write-ups.

---

## 8. Open questions

1. **Reforged fonts**: the extraction in `analysis/war3_full` is classic-era. Reforged ships a different face set; not sampled → `unknown`.
2. **Hinting / antialias mode**: engine-internal in classic WC3 (no config found in the MPQ text files) → `unknown`; our atlases are FreeType/stb defaults.
3. **Russian build**: FRIZQT covers Cyrillic (`U+0410` present), so no font substitution is needed for a RU build — `observed` from the cmap, not from a running RU client.
4. **`NIM_____.ttf` purpose**: identical metrics to FRIZQT; not referenced by `war3skins.txt`. Probably a placeholder/duplicate → `unknown`.
