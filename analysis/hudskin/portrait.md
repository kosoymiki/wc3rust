# HUD skin — selection portrait (lane 6/10, ledger 428)

Module: `crates/wc3-game/src/hudskin/portrait.rs` (owner).
Read-only dependencies: `hudskin/mod.rs` (`Skin::nine`/`Skin::stretch`, role table),
`hudskin/tests_support.rs` (synthetic skin), `hud.rs` (`Rect`, `UiBatch`).

## 1. What the module does

`draw(skin, batch, rect, scale, frame, tint) -> PortraitParts` draws the framed
unit portrait of the selection column and reports the three rects the HUD needs:

| field | meaning |
|---|---|
| `portrait_rect` | the frame window — draw the unit portrait texture here |
| `name_rect` | unit-name band, above the frame (`0.022 H` tall) |
| `class_rect` | class line, between the name band and the frame (`0.019 H`) |

`PortraitFrameKind { Normal, Hero, Enemy }` multiplies the caller's tint
component-wise: `Normal = [1, 1, 1, 1]`, `Hero = [1.0, 0.88, 0.58, 1.0]`
(gold: red survives, blue drops), `Enemy = [0.92, 0.60, 0.58, 1.0]`
(red rim, darker). The caller tint still multiplies through unchanged
(verified with a 0.5 grey tint).

Geometry rule, in order:

1. `rect` is a *budget*: the nine-patch is fitted to the role's authored
   198:217 aspect and centred inside it (`fit_aspect`), so the ornament never
   stretches.
2. `Skin::nine(batch, "portrait_frame", frame_rect, frame_tint)` emits the
   3x3 cells; corners keep their atlas pixel size at every `scale`.
3. `portrait_rect` is the nine-patch **centre cell** (`frame_rect` inset by
   `Skin::insets`), clamped to ≥1 px per side so it is strictly inside even
   when the budget cannot host the ornaments.
4. Bands stack bottom-up from the frame top: class line, then the name band.
   Both are `max(frame.w, 380 * scale)` wide (the retail name row is wider
   than the frame; `MobileHudLayout::name` uses the same 380-unit width).

`frame_rect_for(image_box)` converts a layout portrait-image box (the HUD
`layout.portrait`, 128x128 at scale 1) into the frame budget that hosts it at
the authored opening ratio.

## 2. Measured geometry (observed)

Atlas role `portrait_frame` = element 6, `[378, 300, 198, 217]`
(`analysis/h2/atlas/slices.json`), slice
`analysis/h2/atlas/elements/06_portrait_frame_198x217.png`.

Measured on the slice: the interior fill (alpha > 200, luma < 45, longest
contiguous run ≥ 50 % of the perpendicular span) occupies columns 34..=162 and
rows 49..=182.

| quantity | value |
|---|---|
| authored frame | 198 x 217 px, aspect 0.91244 |
| opening | 129 x 134 px (aspect 0.96269) |
| border around the opening (L/T/R/B) | 34 / 49 / 35 / 34 px |
| nine-patch insets at runtime (sidecar `frame_medium`) | 49 / 65 / 50 / 48 px |

The sidecar (`analysis/h2/atlas/ui_atlas_insets.json`, seam-energy method,
`analysis/agent-hud/tools/atlas_insets_author.py`) is the partition the runtime
skin carries, because those are the ornament's stretch boundaries; the visual
opening is measured here only to document the art. Consequence: with the
shipped insets the window is `198-49-50 = 99` x `217-65-48 = 104` px at the
authored frame size (verified by `shipped_insets_keep_a_real_window`), and a
frame budget smaller than `100 x 114` px degenerates to a single stretched quad.

## 3. Constants

```text
FRAME_ASPECT   = 198 / 217
NAME  fraction = 0.022  of 1080 px reference height  (23.76 px at scale 1)
CLASS fraction = 0.019  of 1080 px reference height  (20.52 px at scale 1)
GAP   fraction = 0.004  of 1080 px reference height  ( 4.32 px at scale 1)
BAND_WIDTH     = 380    units  (x scale)
```

## 4. Tests

`scripts/h2-cargo test -p wc3-game hudskin::portrait`
→ `test result: ok. 7 passed; 0 failed; 0 ignored` (observed 2026-11-15).

| test | invariant |
|---|---|
| `inner_rect_strictly_inside_the_frame` | four budgets (128x128, 198x217, 400x120, 37x211): every window edge strictly inside the drawn frame, frame inside the budget, window == centre cell (`l/t/r/b` offsets exact) |
| `name_and_class_bands_do_not_overlap_the_portrait` | `!intersects` against the portrait and against each other, class below the frame top, retail band heights, bands ≥ frame width |
| `frame_keeps_aspect` | 500x120 and 100x500 budgets: emitted bbox aspect == `FRAME_ASPECT`, fitted by height then width, centred; `frame_rect_for` keeps the ratio |
| `corner_identity_1x_vs_2x` | top-left cell is 12x12 px (the fixture inset) at both 1x (198x217) and 2x (396x434) budgets, identical UVs; the opposite corner samples different texels |
| `hero_and_enemy_tints_differ` | Normal passes the tint through; Hero r>g>b; Enemy green ≥0.1 below Hero and overall darker; caller tint (0.5) multiplies through |
| `shipped_insets_keep_a_real_window` | with 49/65/50/48 the authored-size frame yields a 99x104 window; a 90x100 budget clamps to a 1 px interior instead of inverting |
| `draw_reports_parts_for_the_retail_layout` | runs the real `MobileHud::layout` at 2414x1080 and prints the rects below |

`scripts/h2-cargo check -p wc3-game --lib` → 0 errors.
`rustfmt --edition 2021 --check crates/wc3-game/src/hudskin/portrait.rs` → rc=0.

## 5. Rects at 2414x1080 (observed)

Layout context from `MobileHud::layout(HudView::new(2414x1080))`:
`scale = 1.1000`, `console = (52.8, 710.4, 2308.4, 316.8)`,
`minimap = (79.2, 763.2, 237.6, 237.6)`, `layout.portrait = (343.2, 767.6, 140.8, 140.8)`,
`layout.name = (343.2, 712.6, 418.0, 24.2)`, `layout.class = (343.2, 739.0, 418.0, 24.2)`.

### 5.1 Synthetic skin (12 px insets), budget = `layout.portrait`

| rect | x | y | w | h |
|---|---|---|---|---|
| frame (emitted bbox) | 349.364 | 767.600 | 128.472 | 140.800 |
| `portrait_rect` | 361.364 | 779.600 | 104.472 | 116.800 |
| `name_rect` | 349.364 | 709.388 | 418.000 | 26.136 |
| `class_rect` | 349.364 | 740.276 | 418.000 | 22.572 |

### 5.2 Shipped insets (49/65/50/48), budget = `frame_rect_for(layout.portrait)`

`budget = (309.576, 723.994, 208.048, 228.012)`, fitted exactly.

| rect | x | y | w | h |
|---|---|---|---|---|
| frame (emitted bbox) | 309.576 | 723.994 | 208.048 | 228.012 |
| `portrait_rect` | 358.576 | 788.994 | 109.048 | 115.012 |
| `name_rect` | 309.576 | 665.782 | 418.000 | 26.136 |
| `class_rect` | 309.576 | 696.670 | 418.000 | 22.572 |

### 5.3 Shipped insets, budget = the raw `layout.portrait` box

`frame = (349.364, 767.600, 128.472, 140.800)`,
`portrait_rect = (398.364, 832.600, 29.472, 27.800)` — a 29x28 px window: the
shipped ornaments do not fit a 140.8 px frame, which is why §5.2 is the
recommended call shape.

## 6. Risks / integration notes

1. **Frame budget vs console height.** At 2414x1080 the console is 316.8 px tall.
   The shipped frame at the authored ratio from the 140.8 px portrait box is
   208.048 x 228.012 px; placed at y = 723.994 it leaves 13.594 px above the
   frame inside the console, while the name + class stack needs 58.2 px. The
   bands therefore start at y = 665.782, 44.618 px above the console top, and
   would draw over the game world. The layout's own portrait box leaves 57.2 px
   above it — exactly the band stack — so the integration must choose one:
   (a) portrait window ≈ 99 x 104 px inside a 198 x 217 px frame with the
   bands beside the frame (retail TFT selection-panel shape);
   (b) build the runtime skin with insets scaled by the HUD scale so a ~140.8 px
   frame keeps the ornament inside the console (this module copies the skin's
   insets verbatim, so both work); or
   (c) cap the frame budget to the console height above `layout.class`.
   Owner: HUD layout/integration lane (`mobile_hud.rs`); this module's contract
   is budget-in / rects-out.
2. **Frame overhang vs the minimap.** In §5.2 the frame starts at x = 309.576,
   the minimap zone ends at x = 316.8: 7.22 px of horizontal overlap at
   2414x1080. The layout gives the portrait image box only 26.4 px of left
   clearance while the ornament needs 33.62 px per side.
3. **Fixed-pixel insets.** `Skin::nine` partitions by `skin.insets` in screen px
   (contract in `mod.rs`), so ornament size does not track `scale`. That is what
   makes corners identical at 1x and 2x, and it also means a budget below
   `100 x 114` px cannot hold the shipped ornaments (§5.3, a 29.47 x 27.80 px
   window). `Skin::nine_scaled(..., art_scale)` now exists for callers that want
   the ornament to grow with the HUD; this module deliberately calls `nine`
   (art_scale 1.0) because the lane contract pins corner identity across scales.
   Whoever needs scaled ornaments should either pass a skin whose insets are
   `insets * scale` or switch this call site to `nine_scaled(.., scale)` together
   with scaling `border_for` — the window must always match the partition that
   was actually drawn.
4. **Portrait aspect.** The window aspect is the centre cell's, not the image
   box's (109.048 x 115.012 ≈ 0.948 in §5.2). Retail draws the 128x128 portrait
   stretched into that taller window; do not pre-letterbox the texture.
5. **Band origin.** With the synthetic skin the name band starts at 709.388 vs
   `layout.name.y` 712.6 (3.21 px higher, bands 26.136/22.572 tall vs the
   layout's 24.2). Consume `name_rect`/`class_rect` as a pair; mixing them with
   the layout's own text rects would misalign the rows.
6. **Environment.** The first gate runs hit `ENOSPC` while ten lanes compiled
   (rmeta write failure). Quota reclaim (root `su -c 'id'` = `uid=0`):
   removed regenerable scratch (`$PREFIX/tmp` npm/node-gyp dot dirs and
   `pip-install-*` leftovers, `target/debug/examples`); `df` moved from
   `11G` to `20G` available (91 % used) and the runs above are from after the
   cleanup. No source or donor data was touched.

## 7. Provenance

* `analysis/h2/atlas/slices.json` — role rects (`portrait_frame` = element 6).
* `analysis/h2/atlas/elements/06_portrait_frame_198x217.png` — opening/border
  measurement (§2).
* `analysis/h2/atlas/ui_atlas_insets.json` — runtime insets `frame_medium`.
* `crates/wc3-game/src/hud.rs`, `hudskin/mod.rs`, `mobile_hud.rs` — read-only
  contract, fixture and layout reference.
* Commands and results: §4; rect numbers: §5 (test printout of
  `draw_reports_parts_for_the_retail_layout`).
