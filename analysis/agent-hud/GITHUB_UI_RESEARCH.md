# GitHub UI research — RTS / mobile HUD base (2026-11-12)

Concrete base for the RTS console: **layout grid (1920x1080, 2414x1080), WC3 chrome composition, 9-slice contract, sampled palette, component inventory, licence-cleared build list.**
Everything below is either measured in this session or cited to `file:line`; where neither is true the row says `README-only` or `unknown`.

## 0. Method and provenance

| Item | Value |
|---|---|
| Access date | **2026-11-12** (MSK, UTC+03:00) |
| Web method | `stealth-browser` MCP, headless Chromium 157, Termux aarch64, `--no-sandbox --disable-dev-shm-usage`; `spawn_browser → navigate(domcontentloaded, 45 s) → execute_script(README text) → close_instance`. 16 repo pages opened, 0 CAPTCHA/auth/paywall interactions. |
| Repo metadata | GitHub REST via authenticated `gh api repos/<owner>/<repo>` (stars, licence SPDX, language, `pushed_at`), same date. Page DOM did not expose counters to the script; API is the authoritative source for those numbers. |
| Code evidence | `gh api …/contents/<path>` raw fetch, then `rg -n` on the local copy. Line numbers are from the files as of the commits served on **2026-11-12**. |
| Local art evidence | `cargo run -p wc3-formats --example blp_info -- <blp> <out.ppm>` (BLP1/BLP2 decoder already in `crates/wc3-formats/src/blp.rs`), plus a pure-Python alpha-plane extractor (`BLP1` paletted, 8-bit alpha at `mip0_offset + w*h`, per `blp.rs:96-99`) and median-of-mask colour sampling. No repo code was modified. |
| Claims | `observed` = command/DOM/pixel output; `inferred` = derived arithmetic or convention; `unknown` = not resolved. |

Measurement caveat: colours are sampled from the **Human** console art only. Orc/NightElf/Undead are different palettes by design and were not sampled.

### 0.1 Search ledger (what was actually queried)

`gh search repos` treats space-separated terms as AND, so most of the literal search set returned zero hits — the useful results came from relaxing the terms and from `gh search code`. Recorded so the next agent does not repeat the dead ends.

| Query | Hits used |
|---|---|
| `warcraft 3 UI FDF console`, `war3 console ui`, `RTS HUD`, `mobile RTS UI kit`, `game UI framework HUD`, `unity RTS HUD`, `godot RTS HUD`, `command card RTS`, `minimap RTS UI`, `warcraft3 ui textures`, `game-ui design system` | **0** each (AND matching) |
| `9-slice UI` | 5 (all web/DOM, low value) |
| `minimap RTS` | 1 (`rufreakde/minimap.rts.twod`, 2★, Apache-2.0 — not included: too small to matter) |
| `warcraft3` | 10 → Warsmash/HiveWE/open-realm/War3Net |
| `warcraft-ui` | 10 → `TheOrcDev/warcraftcn-ui`, `Fiurs-Hearth/WIIIUI` |
| `warcraft 3 ui in:name,description` | 10 → `nightknighto/Warcraft-3-Reforged-UI-Designer` |
| `rts-game in:name,description` | 10 → godot-open-rts, OpenRA-adjacent engines |
| `rts ui in:description`, `strategy game open source` | 10 each (open-source RTS list) |
| **code** `DecorateFileNames` | 10 → **Warsmash, Tasyen/FDF, corepunch/open-realm, WurstStdlib2** |
| **code** `SimpleInfoPanelUnitDetail` | 10 → **corepunch/open-realm `hud_infopanel.c`, Warsmash `MeleeUI.java`** |
| **code** `class MeleeUI` | 8 → the single canonical WC3 console implementation |
| **code** `commandCard` | 12 (noise: web projects) |

The decisive queries were the **code** searches, not the repo searches: the WC3 console exists in exactly two open implementations, and both are found by searching for an FDF keyword, not by searching for "RTS HUD".

---

## 1. Repo matrix

Stars/licence/language: GitHub API, 2026-11-12. "Opened" = page loaded via stealth-browser this session. "Reuse" cites the file and line range actually read.

| # | Repo | ★ | Licence | Lang | What exactly to reuse (file:line) | Risk |
|---|---|---|---|---|---|---|
| 1 | `Retera/WarsmashModEngine` | 594 | **AGPL-3.0** | Java | `core/.../ui/MeleeUI.java:215` `DEFAULT_COMMAND_CARD_ICON_WIDTH = 0.039f`; `:217-218` card 4x3; `:458-459` `widthRatioCorrection = minWorldWidth/1600`, `heightRatioCorrection = minWorldHeight/1200`; `:489-491` minimap rect `(18.75, 13.75, 278.75, 276.25)`; `:564-566` `ConsoleUI` = `SetAllPoints`; `:570-572` `ResourceBarFrame` anchored `TOPRIGHT`; `:1243-1246` command-card anchors `x = 0.6175 + 0.0434·i`, `y = 0.095 − 0.044·j`, cell `0.039`; `parsers/fdf/GameUI.java:1573-1585` `convertX = fdfX/0.8·W`, `convertY = fdfY/0.6·H`; `ui/MeleeUIMinimap.java:41-54` aspect-preserving map fill. | **AGPL-3.0**: no code copying into a closed-source game. Numeric constants are facts and may be re-derived; the Java text may not. Its minimap rect is 276.25 high vs the art's 279 square window — treat as an approximation, not gospel. |
| 2 | `corepunch/open-realm` | 395 | **MIT** | C | `games/warcraft-3/game/hud/hud_console.c:18-23` clock-strip constants (`TIME_OF_DAY_HOVER_X 0.342`, `W 0.11971875`, `H 0.060`); `:366` `UI_SetFrameRect(&frame, 0.0070f, 0.4525f, 0.1395f, 0.1395f)` = minimap window; `:240-241` note that retail 1.30+ adds `ConsoleTexture05/06` widescreen tiles 0.256 outside the 4:3 root; `games/warcraft-3/common/stb_fdf.h:203-204` `Backdrop.CornerFlags` + `CornerSize` (the FDF 9-slice model), whole parser 2,133 lines. | MIT: portable with attribution. C parser is MIT-clean and can be cross-checked against our Rust `fdf.rs`. Their `TIME_OF_DAY` values are retail-derived, not documented by Blizzard → treat as `reported-unverified` upstream. |
| 3 | `OpenRA/OpenRA` | 17,776 | **GPL-3.0** | C# | `OpenRA.Game/Graphics/ChromeProvider.cs:52` `PanelRegion` (8 ints); `:207-232` builds the nine sprites from `pr[0..7]` = `x, y, leftW, topH, centreW, centreH, rightW, bottomH`. This is the cleanest *descriptor format* for 9-slice found. | GPL-3.0: read the format, do not port the C#. The 8-int idea is not copyrightable; the implementation is. |
| 4 | `lampe-games/godot-open-rts` | 1,142 | **MIT** | GDScript | `source/match/hud/ProductionQueue.tscn:9-11` `HBoxContainer`, `separation = 5`; `:15-35` `custom_minimum_size = Vector2(48, 48)` queue cells; `source/match/hud/ProductionQueue.gd:7,19-23` rebuild-on-signal queue; `ResourcesBar.tscn:31-33,58-59` full-rect anchors. | MIT. Small scope (48 px cells, no WC3-style 4x3), but the queue-rebuild pattern and the anchor setup are directly portable. |
| 5 | `MinaPecheux/UnityTutorials-RTS` | 732 | **MIT** | C# | `Assets/Scripts/UI/SkillButton.cs` (21 lines: icon + cooldown fill) and `Assets/Scripts/UI/Minimap.cs`; README marks the repo **UNMAINTAINED** (Unity 2020/2021, not Unity 6). | MIT but tutorial-grade; useful only as a checklist of UI states. |
| 6 | `Tasyen/FDF` | 14 | **MIT** | HTML | FDF keyword reference, e.g. `Keywords/DecorateFileNames.html`; documents the exact `DecorateFileNames` mechanism that maps `ConsoleTexture01` → race tile. | MIT docs. HTML dump, no code. |
| 7 | `nightknighto/Warcraft-3-Reforged-UI-Designer` | 19 | **CC0-1.0** | (data) | README: graphical FDF UI designer ("create UI designs and systems for Warcraft 3 maps without code"). Its element/preset data is CC0 and could seed a layout editor. | CC0 = no risk. **README-only** — code/data not read this session. |
| 8 | `TheOrcDev/warcraftcn-ui` | 788 | **MIT** | TypeScript | README: "accessible, retro-inspired UI components drawing from classic RTS"; React/Tailwind. Use as a *token/interaction* reference (gold-on-dark button language), not as a renderer. | MIT. **README-only**, no file:line. Web-only (DOM), cannot be dropped into a wgpu renderer. |
| 9 | `ZeroK-RTS/Zero-K` | 884 | **GPL-2.0** | Lua | README only (game module on Spring/Recoil). Chili UI is the relevant system but lives in the engine/other repos. | GPL-2.0 + **README-only** → reference reading at most. |
| 10 | `spring/spring` | 4,035 | **NOASSERTION** (GPL family) | C++ | Page opened; `chili` path not present in the `develop` tree this session. Chili/meson/unitsync are the interesting parts and were **not verified**. | Stale (last push 2024-03-31), no verifiable UI code in the tree → **do not plan on it**. |
| 11 | `stijnherfst/HiveWE` | 490 | **AGPL-3.0** | C++ | Page opened. World Editor (Qt) — no in-game HUD runtime. | AGPL + wrong layer → **no**. |
| 12 | `oleksbard/react-9-slice` | 11 | **MIT** | JavaScript | README only: 9-slice surface component for React. Algorithmic reference for edge stretching. | MIT, but web/DOM; our renderer needs quads, not CSS. Low value. |
| 13 | `DioxusLabs/taffy` | 3,775 | **MIT** | Rust | README: CSS Block/Flexbox/Grid layout in pure Rust. Candidate solver **if** the HUD becomes flow-based (tooltips, settings, text reflow). | MIT, safe. Adds a dependency; the console grid itself should stay absolute (deterministic). |
| 14 | `emilk/egui` | 32,245 | **MIT / Apache-2.0** | Rust | README: immediate-mode GUI, runs inside engines. Candidate for debug HUD/editor overlay, `Frame`-based 9-slice via UVs. | MIT/Apache, safe. Different widget model from a retained HUD; do not mix paradigms. |
| 15 | `godotengine/godot` | 120,022 | **MIT** | C++ | `NinePatchRect` + `Control` anchor semantics — the reference model for "9-slice + anchor rect". Docs-level only. | MIT. Engine-scale; read the semantics, not the code. |
| 16 | `Drake53/War3Net` | 168 | **MIT** | C# | Page opened; **no FDF module** in the tree (grep for `fdf` returned nothing). It is a `.NET toolkit for WC3 modding` (maps/campaigns), not UI. | MIT but **not useful for UI** — listed to close the search. |
| 17 | `UnityTechnologies/ATerribleKingdom` | 855 | **none** | C# | RTS sample project; surfaced by `gh search repos rts-game`. **Not opened** (licence field already empty). | **No licence = all rights reserved. Do not copy.** Listed as a warning. |

**Reading of the matrix.** Three implementations of the same console exist in code: Warsmash (Java, AGPL), open-realm (C, MIT), and Blizzard's own FDF (data). The **MIT** one (`open-realm`) plus **our own** `crates/wc3-formats/src/fdf.rs` are the only licence-clean sources for a shipping port; Warsmash is the best *documentation* of the arithmetic and stays a citation, not a dependency.

---

## 2. Layout base

### 2.1 WC3's authored coordinate system (the numbers everything else derives from)

WC3 does not lay out the console in pixels; it uses a **0.8 × 0.6 normalised space** that maps to 1600 × 1200 design units.

- `GameUI.convertX(f) = (f / 0.8) · W`, `convertY(f) = (f / 0.6) · H` — `WarsmashModEngine/.../parsers/fdf/GameUI.java:1573-1585` (observed).
- Ratio correction `minWorldWidth/1600`, `minWorldHeight/1200` — `MeleeUI.java:458-459` (observed).
- Consequence: an FDF coordinate of `1.0` spans `0.8·1600 = 1280 px` horizontally and `0.6·1200 = 720 px` vertically, which is why every value is divided by 0.8 or 0.6. The console tiles are authored **1 texel = 1 design px** (a 512-wide tile occupies exactly 512 of 1600).

Authoritative rects, converted to design px (1600 × 1200, **y up from the bottom**):

| Element | Source | x | y | w | h |
|---|---|---|---|---|---|
| `ConsoleUI` root | `MeleeUI.java:564-566` | 0 | 0 | 1600 | 1200 |
| Top tile 1 (`ConsoleTexture01`) | `ConsoleUI.fdf:6-13` | 0 | 1136 | 512 | 64 |
| Top tile 2 (`ConsoleTexture02`) | `ConsoleUI.fdf:14-21` | 512 | 1136 | 174 | 64 |
| Top tile 2b (`ConsoleTexture02` right slice) | `ConsoleUI.fdf:22-29` | 918 | 1136 | 106 | 64 |
| Top tile 3 (`ConsoleTexture03`) | `ConsoleUI.fdf:30-37` | 1024 | 1136 | 512 | 64 |
| Top tile 4 (`ConsoleTexture04`) | `ConsoleUI.fdf:38-45` | 1536 | 1136 | 64 | 64 |
| Bottom tile 1 | `ConsoleUI.fdf:48-55` | 0 | 0 | 512 | 352 |
| Bottom tile 2 | `ConsoleUI.fdf:56-63` | 512 | 0 | 512 | 300 |
| Bottom tile 3 | `ConsoleUI.fdf:64-71` | 1024 | 0 | 512 | 352 |
| Bottom tile 4 | `ConsoleUI.fdf:72-79` | 1536 | 0 | 64 | 352 |
| Minimap window | `MeleeUI.java:489-491`; `open-realm hud_console.c:366` | 18.75 (Warsmash) / 14.0 (open-realm) | 13.75 | 278.75…279 | 276.25…279 |
| Portrait | `resources/UI/FrameDef/SmashUI/UnitPortrait.fdf:15-19` | 422 | 0 | 167 | 228 |
| Command card origin (col 0, top row) | `MeleeUI.java:1243-1246` | 1235 | 190 | 78 | 78 |
| Card pitch | same | Δx 86.8 | Δy −88 | — | — |
| Card block | derived | 1235 | 14 | 338 | 254 |
| Resource bar | `ResourceBar.fdf:13-16` | 923.4 | 1156.4 | 676.6 | 43.6 |
| Upper button bar (4 × 170) | `UpperButtonBar.fdf:44-46` | 0 | 1156 | 680 | 44 |
| Clock strip (gap in the art) | `open-realm hud_console.c:18-23` | 684 | 1136 | 239 | 64 |

The **console silhouette** (opaque art) measured from the assembled tiles: top edge varies **277…327 px** above the bottom (middle arch lowest, left block highest) — i.e. the bar is not a rectangle. That shaping is what makes it read as a WC3 console rather than a flat strip.

### 2.2 Why the faithful scale cannot be transplanted

Scaling WC3 by width (`s = W/1600`, which is what the 4:3 art wants):

| Metric | @1920 (s=1.2) | @2414 (s=1.509) |
|---|---|---|
| Console tile height | **422 px = 39.1 % of H** | **531 px = 49.2 % of H** |
| Console visible top (max) | 392 px = 36.3 % | 493 px = 45.7 % |
| Minimap | 335 × 335 | 421 × 421 |
| Command cell / pitch | 93.6 / 104.1 × 105.6 | 117.7 / 130.9 × 132.8 |
| Command block | 405.6 × 304.8 | 510 × 383.2 |
| Portrait | 200 × 274 | 252 × 344 |
| Resource bar | 812 × 52 | 1,021 × 66 |
| Top bar | 76.8 | 96.6 |
| Upper button | 204 × 53 | 256 × 66 |

At 20:9 the 4:3 console eats **half the screen**. Blizzard hit the same wall: retail 1.30+ ships extra `ConsoleTexture05/06` widescreen tiles outside the 4:3 root (`open-realm hud_console.c:240-241`, `reported-unverified`), and Reforged rebuilt the console rather than stretching it. **Design decision: keep WC3's *grammar* (corner minimap → portrait → info → 4×3 card, stone/gold chrome), replace its *proportions* with a height-based grid.**

### 2.3 The actual grid

Both targets are 1080 tall, so vertical metrics are identical; only the horizontal budget changes (1920 → 2414). Two profiles:

- **P (pointer/desktop)** — cell 72, gap 8, icon 64 at 1×.
- **T (touch)** — cell 88, gap 10, icon 64 centred (cell grows, icon never upscales), hit target ≥ 88 px.

Safe area used for every number: **top 72, bottom 24, left 24, right 24** — `REFERENCES.md:74` measured that the previous build's pills at y≈14–30 collided with system icons and required ≥72 px at 1080p; `REFERENCES.md:85` proposes 80. Use the real inset reported by the window, clamped to ≥72; 48 is only valid in true immersive mode with the status bar hidden.

#### 1920 × 1080 — profile P

| Zone | x | y | w | h | anchor |
|---|---|---|---|---|---|
| Minimap frame | 24 | 840 | 216 | 216 | BOTTOMLEFT (safe) |
| Minimap window (map) | 32 | 848 | 200 | 200 | inside frame, 8 px bezel |
| Minimap control strip | 240 | 840 | 44 | 216 | RIGHT of minimap, TOP aligned |
| Portrait | 296 | 896 | 120 | 160 | LEFT of info, BOTTOM = safe |
| Info panel | 432 | 824 | 1136 | 232 | stretches to card − 16 |
| Command card | 1584 | 824 | 312 | 232 | BOTTOMRIGHT (safe) |
| Card cell | — | — | 72 | 72 | 4 cols, pitch 80 |
| Card header (queue/name) | 1584 | 784 | 312 | 32 | above card |
| Top bar | 24 | 72 | 1872 | 56 | TOPLEFT↔TOPRIGHT |
| Chips ×4 | 24 / 164 / 304 / 444 | 72 | 132 | 56 | TOPLEFT, 8 px gaps |
| Resource pills ×3 | 1332 / 1488 / 1644 | 72 | 148 | 56 | right cluster |
| Clock | 1800 | 72 | 96 | 56 | TOPRIGHT |

#### 1920 × 1080 — profile T

| Zone | x | y | w | h |
|---|---|---|---|---|
| Minimap frame / window | 24 / 32 | 796 / 804 | 260 / 244 | 260 / 244 |
| Control strip | 284 | 796 | 48 | 260 |
| Portrait | 344 | 872 | 140 | 184 |
| Info panel | 500 | 772 | 998 | 284 |
| Command card | 1514 | 772 | 382 | 284 |
| Card cell / gap | — | — | 88 / 10 | 88 |
| Card header | 1514 | 732 | 382 | 32 |

#### 2414 × 1080 — profile P

| Zone | x | y | w | h |
|---|---|---|---|---|
| Minimap frame / window | 24 / 32 | 840 / 848 | 216 / 200 | 216 / 200 |
| Control strip | 240 | 840 | 44 | 216 |
| Portrait | 296 | 896 | 120 | 160 |
| Info panel | 432 | 824 | **1630** | 232 |
| Command card | 2078 | 824 | 312 | 232 |
| Card header | 2078 | 784 | 312 | 32 |
| Chips ×4 | 24 / 172 / 320 / 468 | 72 | 140 | 56 |
| Resource pills ×3 | 1794 / 1958 / 2122 | 72 | 156 | 56 |
| Clock | 2286 | 72 | 104 | 56 |

#### 2414 × 1080 — profile T

| Zone | x | y | w | h |
|---|---|---|---|---|
| Minimap frame / window | 24 / 32 | 796 / 804 | 260 / 244 | 260 / 244 |
| Control strip | 284 | 796 | 48 | 260 |
| Portrait | 344 | 872 | 140 | 184 |
| Info panel | 500 | 772 | 1492 | 284 |
| Command card | 2008 | 772 | 382 | 284 |
| Card header | 2008 | 732 | 382 | 32 |

**Rule for the extra 494 px at 2414:** it all goes to the info panel. The minimap, portrait and command card keep their pixel size so touch targets and icon scale stay identical between devices — the wider screen shows more *information*, not bigger buttons. (Alternative, if the map matters more: raise minimap to 260 and shrink info by 44.)

### 2.4 Anchor contract

| Frame | Anchor | Offset | Grows |
|---|---|---|---|
| `ConsoleRoot` | `SetAllPoints(root)` | — | with window |
| `TopBar` | TOPLEFT + TOPRIGHT | `+safe.left`, `+safe.top`, `−safe.right` | horizontally |
| `MinimapFrame` | BOTTOMLEFT of safe | `(safe.left, safe.bottom)` | fixed |
| `MinimapControlStrip` | RIGHT/TOP of `MinimapFrame` | `(0, 0)` | fixed |
| `PortraitFrame` | LEFT of `InfoPanel`, BOTTOM of safe | `(−16, 0)` | fixed |
| `InfoPanel` | LEFT = portrait.RIGHT+16, RIGHT = card.LEFT−16, BOTTOM = safe.bottom | — | horizontally |
| `CommandCard` | BOTTOMRIGHT of safe | `(−safe.right, safe.bottom)` | fixed |
| `CardHeader` | BOTTOM of `CommandCard` TOP | `(0, +8)` | width of card |
| `Toast` | TOP centre | `y = safe.top + 56 + 16` | fixed 560 |
| `Tooltip` | follows pointer | `+12,+12`, flipped at edges | content |

### 2.5 Safe-area and correctness rules (testable)

1. **All rects inside the safe frame**: `x ≥ safe.left`, `x+w ≤ W−safe.right`, `y ≥ safe.top`, `y+h ≤ H−safe.bottom`. Anchor to the safe rect, never to `(0,0)` — cutouts and Android 15 edge-to-edge make `(0,0)` unusable (`REFERENCES.md:82-85`).
2. **Top strip is 56 px tall and lives below `safe.top`**; nothing may be drawn above it except a full-bleed background. Measured failure of the previous build: system icons overlapped resource pills at y≈8–30 in a 1200×492 capture → ≥72 px top inset required (`REFERENCES.md:74`).
3. **Console never crosses `safe.bottom`** (gesture bar), and its lowest interactive row keeps a 24 px margin.
4. **Battlefield reserve**: no persistent control in the rectangle `[safe.top+56 … cardTop−32] × [0 … W]` except the two edge clusters. At 1080p that reserves ≥ 60 % of the height for the world.
5. **Touch targets**: `min(w,h) ≥ 88` for every interactive rect in profile T, `≥ 48` for pointer in profile P (Android guidance is 48 dp; 88 px ≈ 48 dp at 1.83× — `REFERENCES.md:34`).
6. **No overlap**: pairwise intersection area == 0 across `{topbar, minimap, strip, portrait, info, card, header, toast, tooltip}`; every text box is inside its parent rect; `measure(label) ≤ rect.w − 12`.
7. **Icon scale is discrete**: 64 (1×) or 128 (2× nearest). A 72/88 cell centres a 64 icon with a 4/12 px frame — never resample 64 → 96.
8. **One hierarchy per zone**: top = resources/menu/clock; bottom-left = map + portrait; bottom-centre = info; bottom-right = commands.

---

## 3. Visual base — how WC3 actually builds the chrome

### 3.1 The tile pipeline (observed)

1. `ConsoleUI.fdf` requests `File "ConsoleTexture01" … "ConsoleTexture04"` with `DecorateFileNames` (`analysis/war3_full/UI/FrameDef/UI/ConsoleUI.fdf:6-45`).
2. `DecorateFileNames` resolves the alias per race in `war3skins.txt`: **`[Default]` maps `ConsoleTexture01..04` to `UI\Console\Human\HumanUITile01..04` (`war3skins.txt:248-251`)**, and Orc / NightElf / Undead override them (`:55-58`, `:107-110`, and the Undead block). Human therefore *is* the default skin, not a missing entry — `observed`, and pinned by `crates/wc3-game/tests/ui_fdf.rs:379-387` (`get_in("Default","ConsoleTexture01") == UI\Console\Human\HumanUITile01`).
3. Each tile is placed as a **fixed quad with an explicit `TexCoord`**, not 9-sliced. Five quads on top, four at the bottom (`ConsoleUI.fdf:6-79`). Corners are never stretched — the art is authored to the exact rect.
4. Consequence for us: **the console is "fixed-slice", the dialogs are 9-slice.** Copying a 9-slice mindset to the console will smear the stone. Use fixed slices (or a 9-slice with corner = the whole parapet, which is the same thing).

Tile verification: `512×512`, paletted `BLP1`, 8-bit alpha, 1 texel = 1 design px for the 512-wide / 352-tall bottom region (rows 160…511 of the texture are used for the bottom tile: `TexCoord 0,1,0.3125,1`).

### 3.2 Measured art geometry (assembled 1600 × 352 canvas at the FDF rects)

Alpha=0 wells are where the world/map/portrait/buttons show through; everything else is painted chrome.

| Feature | Rect (design px, x / y-from-bottom / w / h) | Evidence |
|---|---|---|
| Minimap well (transparent) | 20 / 16 / 276 / 221 | flood-fill on alpha, area 60,996 px |
| Minimap opaque letterbox band above the well | y 236…291, full 276 px width (pure black) | vertical scan x=150: alpha 255, RGB 0,0,0 for y 236…291 |
| Minimap inner steel bevel | x 297…304, y 292…299 | scan: `(65,62,65)` then `(151,153,150)` |
| Minimap outer stone | x 0…19, y 0…~340 | scan: rust `(71,37,24)`, stone `(117,103,89)` |
| Portrait well (arch) | 431 / 64 / 149 / 160 | flood-fill, area 22,304 (the arch above it is opaque art) |
| Portrait gold ring | x 426…586, y 0…230 | gold mask bbox |
| Command card wells | x **1235 / 1321 / 1408 / 1495**, y **14 / 101 / 188**, cell **78 × 78** | 12 connected components, area 3,822–6,241 each |
| Card seam width | 9 px vertical, 10 px horizontal (1 px black line + 8–9 px stone) | pitch 86.75 / 88 minus cell 78 |
| Card top row clipped by shadow band | top row well is 78 × 49, rows 2–3 are 78 × 79 | component bboxes |
| Inventory header strip | 1024 / 277 / 170 / 24 | component bbox |
| Minimap toggles (gold rings) | **36 × 32** at x 329…354, y **266 / 222 / 178**, pitch 44 | gold component bbox 26×22 ring |
| Minimap button (ping/terrain) | 41 × 37 at x 314…354, y 108…144 | gold component bbox |
| Console silhouette top | 327 (left block), 277–279 (middle arch), 317–318 (right) | topmost opaque row per 100 px band |

The card wells **match the code exactly**: `MeleeUI.java:1243-1246` gives x = 1235 + 86.8·i and y = 190 − 88·j with a 78 cell — the art and the engine agree to ≤ 2 px. That is the strongest confirmation available that the grid is not invented.

### 3.3 The 9-slice contract (for everything that is not the console)

WC3's FDF `Backdrop` is a 9-slice with **explicit corner flags and a corner size**: `open-realm stb_fdf.h:203-204` carries `CornerFlags` and `CornerSize` in its frame-def struct, and our parser already reads `TexCoord` in FDF order (`crates/wc3-formats/src/fdf.rs:306-308`).

Real values from our MPQ (`analysis/war3_full/UI/FrameDef/UI/`):

| Use | `CornerFlags` | `CornerSize` | px @ 1600×1200 |
|---|---|---|---|
| Dialog / menu panel | `UL\|UR\|BL\|BR\|T\|L\|B\|R` | 0.048 | **96** |
| Text edit box | all | 0.0125 | **25** |
| Cinematic panel | `UL\|UR\|T` (top-only) | 0.04 | **80** |
| Console button face (`BTN*.blp`) | baked, not FDF | — | **4** (1 dark + 2 silver + 1 dark) |

So the shipping 9-slice insets are **panel 96 px corner at 1600×1200 design scale, edit box 25, icon border 4**. Mapping to 1080p (`×0.9`): panel corner 86 px, edit box 22 px — but clamp to `0.25·min(w,h)` on shallow bars (≈58 px on the 232 px card), otherwise the corner eats the panel. Icon border stays 4 px at a 64 icon and 8 px at 128.

The OpenRA descriptor maps 1:1 onto this and is worth adopting as the runtime struct (`ChromeProvider.cs:217-232`): `panel_region = [x, y, left_w, top_h, centre_w, centre_h, right_w, bottom_h]` → 9 sprites; skip any side whose flag is absent.

### 3.4 Palette sampled from the actual BLPs

Method: assemble the four Human tiles at their FDF rects into 1600×352, keep pixels with `alpha > 200`, then report the **median** of colour-masked subsets (k-means with k=8 for the shares). Not invented RGB; reproducible with the commands in §7.

| Token | Hex | sRGB 0–1 | Linear | Share | Where it comes from |
|---|---|---|---|---|---|
| `chrome.black` | `#000000` | 0,0,0 | 0,0,0 | 28.9 % | recesses, seams, letterbox band |
| `chrome.brown` | `#180f0d` | .094,.059,.051 | .0088,.0048,.0040 | 13.4 % | dark warm stone shadow |
| `chrome.steel_dark` | `#3a3a39` | .227,.227,.224 | .0423,.0423,.0410 | 20.0 % | mid stone/steel mass |
| `chrome.steel` | `#5c5c5b` | .361,.361,.357 | .1080,.1080,.1057 | 9.3 % | lit stone face |
| `chrome.steel_lit` | `#979996` | .592,.600,.588 | .3095,.3185,.3050 | — | parapet highlight (masked median) |
| `chrome.silver` | `#d0d3d0` | .816,.827,.816 | .6308,.6514,.6308 | 4.4 % | bevel highlight, icon frame |
| `chrome.gold` | `#a78900` | .655,.537,0 | .3864,.2502,0 | 2.8 % | trim, rings, icon borders |
| `chrome.gold_dim` | `#a58600` | .647,.525,0 | .3763,.2384,0 | — | ring in shadow |
| `chrome.rust` | `#4b2104` | .294,.129,.016 | .0704,.0152,.0012 | 16.5 %\* | rusty column/edges |
| `chrome.navy` | `#041b3f` | .016,.106,.247 | .0013,.0111,.0508 | 4.6 % | cold inset / inventory glass |
| `btn.face` | `#00182b` | 0,.094,.169 | 0,.0091,.0242 | — | button normal fill |
| `btn.face_pressed` | `#000112` | 0,.004,.071 | 0,.0003,.0060 | — | button pressed fill |
| `btn.face_disabled` | `#343433` | .204,.204,.200 | .0343,.0343,.0331 | — | disabled fill |
| `btn.rim` | `#fdfa4c` | .992,.980,.298 | .9823,.9560,.0723 | — | lit rim on all three states |
| `state.active` | `#3eb164` | .243,.694,.392 | .0482,.4397,.1274 | — | additive "active button" green |
| `state.select` | `#f1d320` | .945,.827,.125 | .8796,.6514,.0144 | — | multi-selection / transport gold |
| `bar.hp_tint` | `#41c04a` | .255,.753,.290 | .0529,.5271,.0685 | — | HP tint (texture itself is grey, see 3.5) |
| `bar.mana` | `#3b9ff5` | .231,.624,.961 | .0437,.3467,.9131 | — | mana fill |

\* `rust` overlaps the `brown` cluster; treat the two as one warm-shadow family.

**Reading the palette for a "fantasy metal" token system:** one near-black base (29 %), two greys for the stone mass (29 % combined), one silver highlight (4 %), one warm shadow family (30 %), and **gold at under 3 % of pixels** — gold reads as precious because it is rare. Any reskin that pushes gold above ~6 % of the chrome will look like plastic.

### 3.5 Bevel and state recipes (measured, not guessed)

- **Icon border (CommandButtons):** 1 px dark outer (`#171507`, measured at the top-left corner), **2 px silver** (`#c9d0d2` → `#f5f4f0`), 1 px dark inner, then art from **x=4** — verified on `BTNFootman/BTNAttack/BTNMove` row 1 and column 1. Content area therefore **56 × 56 of 64**; 9-slice inset **4 px**, corner 4–6 px.
- **Bottom/right edges are dimmer:** right edge ≈`#707072`, bottom ≈`#6d6868` → light comes from the top-left at ~45°.
- **Button states** (`human-console-buttonstates2.blp`, 256×256 = 4 stacked 64-tall bands, pill 162×32): normal `#00182b`, pressed `#000112`, disabled `#343433`, all with rim `#fdfa4c`; 4th band is an additive glow (`#09131c` face / `#265478` rim).
- **Active/hover:** `human-activebutton.blp` is **additive green**, median `#0c2213`, p97 `#3eb164` — it multiplies the brightness of whatever is under it. Do not paint a green border; add.
- **Multi-selection / transport:** `human-multipleselection-border.blp` median `#a47608`, p97 `#f1d320`; `human-transport-slot.blp` p97 `#f1d040` on black.
- **Level-up overlay:** `human-button-lvls-overlay.blp` p97 `#c1a605` (gold numerals baked in a 32×32 sprite).
- **HP bar:** `human-healthbar-fill.blp` is **greyscale** (min 91, max 255, mean 146) — WC3 tints it at runtime. Never sample a green from the texture; tint the grey. Mana `human-manabar-fill.blp` is already blue (avg 49,118,167).
- **Resource icons** (`UI/Feedback/Resources/`): `ResourceGold` 32×32 avg (110,91,25), `ResourceLumber` avg (15,32,10), `ResourceSupply` avg (33,20,14); tooltip variants are 32×32 fully opaque.

### 3.6 Pipeline gotchas (would have cost a day each)

1. **Transparent texels are magenta `#ff00ff`, alpha 0.** The palette keeps a magenta key. Any decoder that ignores the alpha plane produces a magenta console — `blp_info` reported `zero_alpha=155,634` for tile 01 while its PPM still contained magenta.
2. **BLP1 paletted alpha lives at `mip0_offset + w*h`**, 8/4/1 bit packed (`crates/wc3-formats/src/blp.rs:96-99`). PPM dumps lose it; use the RGBA path.
3. **Author PNGs as sRGB, upload as `Rgba8UnormSrgb`**, and do not gamma-correct twice (same rule as `DESIGN_SPEC.md:177`).
4. **The console art is 1 texel = 1 design px** — upscaling the tiles to a 1920 grid by anything other than an integer or a proper resample will destroy the 1 px bevels (they are literally 1 px).
5. **`ConsoleTexture05/06` exist in retail 1.30+ but not in our extraction** (`open-realm hud_console.c:240-241`). A widescreen chrome extension must be authored by us, not expected from the MPQ.

---

## 4. Component inventory

Sizes are for profile P / profile T at 1080p; `src` is the WC3 source of the pattern (not necessarily reusable art).

| Component | Size (P / T) | Anatomy | States | Notes |
|---|---|---|---|---|
| **Minimap frame** | 216 (200 window) / 260 (244) | 8 px bezel: 1 px black, 2 px silver, 1 px black, 4 px stone; corner ticks | normal; ping flash | Map fills with aspect preserved and is centred (`MeleeUIMinimap.java:41-54`); letterbox uses `chrome.black` |
| **Minimap toggles** | 36×36, pitch 44 / 40×40, pitch 48 | gold ring 2 px + dark glass face, 16 px glyph | normal / pressed / disabled | 3 toggles + 1 wide button; WC3 face 36×32 with a 26×22 gold ring, pitch 44, x 329…354 |
| **Portrait frame** | 120×160 / 140×184 | arch: 2 px gold ring, 1 px black, stone surround; 96 px portrait | normal / dead (desaturated) | WC3 portrait 167×228 at x=422; model viewport 167×170 |
| **HP bar** | 200×16 (in info) | 1 px black frame, 1 px inner shadow, grey bevel fill **tinted** | >55 % green / 25–55 % amber / <25 % red | fill texture is greyscale; text `420 / 420` 14 px right-aligned inside |
| **Mana bar** | 200×12 | same frame, blue fill `#3b9ff5` | normal / empty (hidden when 0) | WC3 hides the mana bar for units without mana |
| **Command slot** | 72×72 cell, icon 64 / 88×88 cell, icon 64 | 4 px baked border (1 black, 2 silver, 1 black), art 56 px, 2 px gold on hover | normal / pressed (face −40 % luminance) / disabled (alpha .45, desaturate, no rim) / active (additive `#3eb164`) / cooldown (radial darken + bottom bar) / level badge | Grid 4×3, gap 8/10; hotkey 10 px top-right; **never** stretch the 64 icon |
| **Resource pill** | 148×56 / 156×56 | 32 px icon left, value 24 px bold, label 12 px muted; glass face + 1 px stroke | normal / warning (value < cost → `state.danger`) | WC3 columns: 171.9 px pitch, icons 32.8, right-aligned values (`ResourceBar.fdf:16-45`) |
| **Top chip** | 132×56 / 140×56 | 20 px glyph + 14 px label, stone pill | normal / pressed / disabled | 4 chips, left cluster (WC3: Quests/Menu/Allies/Chat, 170×44) |
| **Clock** | 96×56 / 104×56 | sun/moon glyph + time 16 px | day / night | WC3 puts it in the 239 px gap between buttons and resources |
| **Tooltip** | ≤ 360 wide, 8 px pad | title 14 px gold, body 12 px, 1 px black frame, 92 % black fill | hover only | flips at the right/bottom edge; never covers the pointer |
| **Toast** | 560×56 | 24 px icon dot + 18 px text, pill | info / success / danger | below the top bar: `y = safe.top + 72` |
| **Production queue cell** | 48×48, gap 5 | icon 48, progress 4 px bottom | queued / active / paused | godot-open-rts reference: `HBoxContainer` separation 5, 48×48 (`ProductionQueue.tscn:9-35`) |
| **Card header** | 312×32 / 382×32 | unit name 18 px + supply 14 px right | normal | sits above the 4×3 grid |

---

## 5. Skills / base list for future work

| Layer | Recommendation | Licence | Compatibility | Why |
|---|---|---|---|---|
| **Console layout** | Our own absolute grid solver (this document, §2.3) | — | — | Deterministic, pixel-exact, testable; no dependency. WC3 itself is absolute (`MeleeUI.java:1243-1246`). |
| **Flow layout** (tooltips, settings, text reflow) | `DioxusLabs/taffy` | MIT | ✅ | Pure Rust flex/grid; add only if a real flow need appears. |
| **9-slice runtime** | Own struct + shader; adopt OpenRA's 8-int `PanelRegion` *shape* | own / GPL reference | ✅ (format) | `ChromeProvider.cs:217-232`; WC3 semantics = `CornerFlags` + `CornerSize` (`stb_fdf.h:203-204`, our MPQ FDF values in §3.3). |
| **FDF parsing** | Existing `crates/wc3-formats/src/fdf.rs`; cross-check against `open-realm stb_fdf.h` | own / MIT | ✅ | Ours already parses the console FDF (`fdf.rs:1067-1089`) and skin resolution (`tests/ui_fdf.rs:379-387`). |
| **HUD composition reference** | `corepunch/open-realm` `games/warcraft-3/game/hud/hud_console.c` | MIT | ✅ port with attribution | Retail-derived constants for the clock strip and minimap rect. |
| **Layout arithmetic reference** | `Retera/WarsmashModEngine` `MeleeUI.java` | AGPL-3.0 | ⚠️ citations only | Best documentation of the 1600/1200 space; do **not** copy the code. |
| **Icon pipeline** | Existing 691-icon BTN decoder + atlas (`DESIGN_SPEC.md:173-180`) | own | ✅ | 64×64 source, 4 px baked border → 56 px content; display 64/128 only. |
| **Art (chrome)** | Author our own tiles; WC3 BLPs at runtime from the user's MPQ only | Blizzard assets | ⚠️ no redistribution | The game reads the operator's files at runtime; never ship the BLPs. |
| **Art (placeholder/base)** | Kenney UI Pack (kenney.nl) | CC0 | ✅ | Non-GitHub; only if a neutral grey base is needed before the real tiles exist. |
| **Text rendering** | `pop-os/cosmic-text` (or current glyphon path) | Apache-2.0 | ✅ | Kerning + fallback for 12–40 px HUD type. |
| **Debug/editor overlay** | `emilk/egui` | MIT/Apache-2.0 | ✅ | Immediate-mode panels for tuning the grid at runtime; keep out of the shipping HUD. |
| **Web tokens (if a companion site is built)** | `TheOrcDev/warcraftcn-ui` | MIT | ✅ | Gold-on-dark component language; DOM only. |
| **FDF documentation** | `Tasyen/FDF` | MIT | ✅ | Keyword reference for `DecorateFileNames`, `Backdrop`, `TexCoord`. |
| **Queue/touch reference** | `lampe-games/godot-open-rts` | MIT | ✅ | Small, modern, MIT RTS HUD; useful for queue rebuild + anchors. |
| **Avoid** | `HiveWE` (AGPL, editor), `Zero-K` (GPL, engine-coupled), `spring` (unverified/stale), `War3Net` (no UI), `ATerribleKingdom` (no licence) | — | ❌ | Licence or layer mismatch. |

**Licence rule applied:** MIT/Apache/CC0 → code may be ported with attribution. GPL-2/3 and AGPL → read for facts and formats, never paste code into a closed-source build. No licence → do not use at all.

---

## 6. Unknowns / open questions

1. **Minimap letterbox band**: the art has a 56 px opaque black band above the transparent well, while both code bases draw a 279 × 279 map that would cover it. Whether the band is a baked drop shadow or a letterbox backing is **unknown**; either way the runtime rule is "fill aspect-preserved, centre, paint `chrome.black` behind".
2. ~~Human skin fallback~~ — **resolved 2026-11-13**: `war3skins.txt` `[Default]` (lines 248-251) maps `ConsoleTexture01..04` to `HumanUITile01..04`; the Human block is empty because it *is* the default. Confirmed by `crates/wc3-game/tests/ui_fdf.rs:379-387`.
3. **Reforged console**: not sampled; the extraction in `analysis/` is classic-era. A Reforged grid would need its own tile dump.
4. **Exact command-card behaviour for the 9-slot inventory variant**: `MeleeUI.java:1245` shifts the card by `0.0405` when `USE_NINE_ITEM_INVENTORY`; the art we have is the 6-slot variant. Not derived.
5. **Stars/languages for the two very large repos** came from the API, not the page (the DOM script could not see the counters). Stars are a proxy for community, not quality — `War3Net` (168★) has no FDF at all, `godot-open-rts` (1.1k★) has a clean MIT HUD.

---

## 7. Reproduce

```bash
cd ~/src/herofolends-rs
# 1. decode any chrome BLP to RGB (alpha is dropped by PPM — read it from the BLP)
cargo run -q -p wc3-formats --example blp_info -- \
  analysis/war3_full/UI/Console/Human/HumanUITile01.blp "$PREFIX/tmp/t01.ppm"
#    -> 512x512, alpha_hist=[155634,0,0,0,0,0,0,106510]

# 2. alpha plane (BLP1 paletted, 8-bit) lives at mip0_offset + w*h
python3 - <<'PY'
import struct
b=open('analysis/war3_full/UI/Console/Human/HumanUITile01.blp','rb').read()
ct,abits,w,h=struct.unpack_from('<IIII',b,4)
off=struct.unpack_from('<I',b,28)[0]
a=b[off+w*h:off+w*h+w*h]
print(ct,abits,w,h,'zero_alpha=',sum(1 for v in a if v==0))
PY

# 3. FDF geometry (the layout base)
sed -n '1,81p' analysis/war3_full/UI/FrameDef/UI/ConsoleUI.fdf
sed -n '13,60p' analysis/war3_full/UI/FrameDef/UI/ResourceBar.fdf
sed -n '38,70p' analysis/war3_full/UI/FrameDef/UI/UpperButtonBar.fdf

# 4. FDF 9-slice values in our MPQ
rg -n 'BackdropCornerFlags|BackdropCornerSize' analysis/war3_full/UI/FrameDef/UI/*.fdf | head

# 5. the code the numbers come from (fetch, then grep)
gh api -H 'Accept: application/vnd.github.raw' \
  repos/Retera/WarsmashModEngine/contents/core/src/com/etheller/warsmash/viewer5/handlers/w3x/ui/MeleeUI.java \
  | rg -n 'DEFAULT_COMMAND_CARD_ICON_WIDTH|0\.6175|widthRatioCorrection|minimapDisplayArea'
gh api -H 'Accept: application/vnd.github.raw' \
  repos/corepunch/open-realm/contents/games/warcraft-3/game/hud/hud_console.c \
  | rg -n 'TIME_OF_DAY|UI_SetFrameRect'
```

---

### TL;DR for the implementer

- Build the console as **fixed slices**, not 9-slice: 5 top quads + 4 bottom quads, exactly as `ConsoleUI.fdf` does.
- Keep WC3's **design space of 1600×1200** for any derived constant, then **re-scale by height**, not width — width scaling eats 39–49 % of a 16:9/20:9 screen.
- The card is **4×3, 78 px cells, 9–10 px gaps, 27 px from the right edge** at 1600; our grid is 72/8 (pointer) or 88/10 (touch) at 1080p.
- Gold is **< 3 % of the chrome**; silver is the bevel, gold is the trim. HP fill is **grey geometry, tinted at runtime**.
- Port MIT (`open-realm`), cite AGPL (`Warsmash`), ignore the rest.
