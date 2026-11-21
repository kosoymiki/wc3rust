# HUD skin lane 8/10 — `hudskin::tooltip`

Region: the WC3 tooltip box (name, description, cost/requirement section).
Owner file: `crates/wc3-game/src/hudskin/tooltip.rs` (write scope of this lane).
Read-only contract: `crates/wc3-game/src/hudskin/mod.rs` (`Skin::nine`, `Skin::stretch`),
`crates/wc3-game/src/hudskin/tests_support.rs` (`sample_skin`).

## 1. Goal and invariants

1. A tooltip is **anchored to the hovered command slot**: it floats above the
   slot, flips below when the slot sits near the top edge, and is centred on the
   slot horizontally (clamped to the frame).
2. It **never leaves the safe frame**: box, divider and every text rect are kept
   inside `safe` by construction, including anchors in all four corners and
   degenerate frames.
3. Three sections, in this order: **name** (one line), **description** (whole
   line boxes), **cost/requirement** (one rect per row, gold `FED312`).
4. Width: `min(380 * scale, 42% of the safe width)`, never wider; a caller may
   ask for narrower, down to `MIN_WIDTH`.
5. Height: `visible_lines * line_height` plus fixed padding/divider chrome, with
   `visible_lines <= 9` regardless of how many lines are requested.
6. Cost rows `<= 8` (retail cap), always **below** the description: the title
   line and at least one description line outrank costs.
7. The shell is a nine-patch that keeps its atlas corner size; the divider is the
   `thin_bar` slice stretched across the content width.

## 2. Retail mechanism (observed)

`CToolTipWar3` ctor `0x6f154a20` (`..\UI\CToolTipWar3.cpp`) creates a name frame,
a description frame and a fixed array of **eight** cost/requirement text frames,
plus the gold/supply/… cost glyph frames:

- `analysis/agent-re/ui_audio/ENGINE_MAP.md:200` §3.1 — `in_ECX[0x21] = 8`
  ("up to 8 lines"), composition order **name → description → cost lines**,
  padding `0.005`, backdrop insets `0.01` / `0.0019`.
- `analysis/agent-re/ui_audio/raw/tft.6f154a20.dis.txt:43-58` — the ctor loads
  `ToolTipBorder` / `ToolTipBackground` and writes the float constants
  `0x3c23d70a` (0.01) and `0x3af9096c` (0.0019); `0x6f154a20` also pushes the
  eight per-line frames in the loop the ENGINE_MAP quotes.
- Cost colour: `crates/wc3-game/src/style.rs:15` — `GOLD = [0.996, 0.827, 0.070]`
  = `FED312`, documented there as the `CToolTipWar3` cost/requirement colour.
  The lane exports it as `COST_TINT` so a caller cannot drift from the shipped
  gold.

The live HUD already encodes the same numbers, which this lane mirrors so the
atlas path and the FDF path agree:

- `crates/wc3-game/src/mobile_hud.rs:1533` — `(380.0 * scale).min(safe.w * 0.42)`.
- `crates/wc3-game/src/mobile_hud.rs:1540` — `TOOLTIP_MAX_LINES = 9`.
- `crates/wc3-game/src/mobile_hud.rs:721` — `TOOLTIP_MAX_COST_LINES = 8`.
- `crates/wc3-game/src/mobile_hud.rs:766` — line box `30 * 1.25 * max(scale, 0.8)`.
- `crates/wc3-game/src/mobile_hud.rs:2116-2200` — title in gold, description in
  text colour, cost rows in `style::GOLD`, clipped row by row.

## 3. Geometry (in file, exact)

Constants (`tooltip.rs`): `MAX_WIDTH = 380`, `MAX_SAFE_FRACTION = 0.42`,
`MAX_LINES = 9`, `MAX_COST_LINES = 8`, `MIN_WIDTH = 120`, `DEFAULT_LINES = 2`,
`PAD_X = 10`, `PAD_Y = 6`, `ANCHOR_GAP = 6`, `DIVIDER_HEIGHT = 6` (clamped 2..=10),
`DIVIDER_GAP = 3`, `LINE_BASE = 30`, `LINE_RATIO = 1.25`.

```
line_height = 30 * 1.25 * max(scale, 24/30)          # readable prose floor
divider_h   = clamp(6 * scale, 2, 10);  gap = 3 * scale;  chrome = divider_h + 2*gap
lines       = clamp(requested_lines, 1, min(9, floor((safe.h - chrome - 2*pad_y) / line_height)))
costs       = min(requested_costs, 8, lines - 2)      # title + one body line win
body_lines  = lines - 1 - costs
width       = clamp(requested_width or 380*scale, min(120*scale, cap), cap)
cap         = min(380 * scale, 0.42 * safe.w)
height      = lines * line_height + chrome + 2 * pad_y
```

Placement:

```
x = clamp(slot.centre.x - width/2, safe.x, safe.right - width)
space_above = slot.y - safe.y;   space_below = safe.bottom - slot.bottom
above = space_above >= height+gap ? true
      : space_below >= height+gap ? false
      : space_above >= space_below
y = (above ? slot.y - gap - height : slot.bottom + gap) clamped into safe
box = Rect(x, y, width, height).clamped_inside(safe)   # final safety net
```

Sections, computed from the final box (so clamping can never desynchronise them):

```
inner   = box.inset_sides(pad_x, pad_y, pad_x, pad_y)
title   = (inner.x, inner.y, inner.w, line_height)
divider = (inner.x, title.bottom + gap, inner.w, divider_h)
body    = (inner.x, divider.bottom + gap, inner.w, body_lines * line_height)
cost[i] = (inner.x, inner.bottom - costs*line_height + i*line_height, inner.w, line_height)
```

`inner` is exactly `lines * line_height + chrome` tall, so the sections tile it
without a gap and without overlap: `body.bottom == cost[0].y` and
`cost[i+1].y == cost[i].bottom`.

## 4. API

```rust
pub fn draw(skin: &Skin, batch: &mut UiBatch, anchor: Rect, desired: Rect,
            safe: Rect, scale: f32, tint: [f32; 4]) -> TooltipParts
```

`desired.w` = preferred width (`<= 0` → `380 * scale`), `desired.h` = measured
height of every text line (`<= 0` → `DEFAULT_LINES`), so a caller that measured
its text passes `Rect::new(0.0, 0.0, measured_w, lines * line_height)`.

`TooltipParts { box_rect, title_rect, body_rect, cost_rects, divider_rect,
line_height, lines, drawn, above }`; `is_empty()` is true when the safe frame has
no area (nothing drawn, caller skips text).

Two companions, used by `draw` and available to callers that already know their
counts:

- `TooltipRequest { width, lines, cost_lines }` + `from_desired(desired, scale)`;
- `draw_request(skin, batch, anchor, request, safe, scale, tint)`.
- `layout(anchor, request, safe, scale)` — pure geometry, no GPU types touched.

The lane draws exactly two things: the `tooltip_box` nine-patch
(`Skin::nine`) and the `thin_bar` divider (`Skin::stretch`, one stretched quad
because a divider has no corner ornament). The caller draws the font into the
returned rects with `COST_TINT` for `cost_rects` and `style::GOLD` for the title.

## 5. Test evidence

`scripts/h2-cargo test -p wc3-game --lib hudskin::tooltip` — see §7 for the run.

| test (mechanic `ui.tooltips`) | proves |
|---|---|
| `ui_tooltips_box_stays_inside_safe_at_four_corner_anchors` | box + all sections inside `safe` for all four corner slots; exact rects below |
| `ui_tooltips_anchor_near_top_flips_the_box_below` | top-edge slot → box below; bottom-edge slot → box above |
| `ui_tooltips_width_is_380_at_1080p` | 380 px at scale 1, 760 at scale 2, 42 % cap on a 700 px frame, caller-narrow honoured |
| `ui_tooltips_cost_rows_never_overlap_the_body` | 7 cost rows at the 9-line cap, zero intersection with the body, contiguous stack, 2 requested → 2 rows |
| `ui_tooltips_nine_patch_corners_keep_atlas_pixel_size` | corner dest `12x12`, uv span `12/1536 x 12/1024`, opposite corner identity |
| `ui_tooltips_line_cap_holds_for_twenty_requested_lines` | 20 requested lines → 9 visible lines, box height formula, one body line kept |
| `ui_tooltips_divider_sits_between_title_and_body` | `thin_bar` slice UVs, divider between title and description, starts at the inset |
| `ui_tooltips_cost_tint_is_retail_gold` | `COST_TINT` rounds to `FE D3 12 FF` |
| `ui_tooltips_degenerate_inputs_are_safe` | no frame → empty; no slot → centred box; `scale = NaN` → finite rects; 60x40 frame still contains the box |
| `ui_tooltips_draw_matches_layout_from_desired` | `draw` width/lines/anchor match `layout` for the same measured request |

Corner placements (scale 1, 1920x1080, 6 lines, 3 cost rows, 380 x 249 box):

| slot (x,y,w,h) | placement | box (x,y,w,h) |
|---|---|---|
| 40, 20, 64, 64 | flipped below | 0, 90, 380, 249 |
| 1816, 20, 64, 64 | flipped below, right-clamped | 1540, 90, 380, 249 |
| 40, 996, 64, 64 | above | 0, 741, 380, 249 |
| 1816, 996, 64, 64 | above, right-clamped | 1540, 741, 380, 249 |

## 6. Integration notes

`mobile_hud.rs` currently computes the same box inline (lines 1533-1556). Moving
it onto this lane is a caller change outside this lane's write scope; the API was
shaped so the swap is mechanical:

```rust
let parts = hudskin::tooltip::draw_request(
    &skin, ui, anchor, TooltipRequest::new(0.0, title_lines + body_lines + costs, costs),
    safe_frame, s, [1.0; 4],
);
// parts.title_rect, parts.body_rect, parts.cost_rects, COST_TINT
```

## 7. Run log

```
$ scripts/h2-cargo test -p wc3-game --lib hudskin::tooltip -- --nocapture
running 10 tests
test hudskin::tooltip::tests::ui_tooltips_anchor_near_top_flips_the_box_below ... ok
test hudskin::tooltip::tests::ui_tooltips_cost_rows_never_overlap_the_body ... ok
test hudskin::tooltip::tests::ui_tooltips_cost_tint_is_retail_gold ... ok
test hudskin::tooltip::tests::ui_tooltips_box_stays_inside_safe_at_four_corner_anchors ... ok
test hudskin::tooltip::tests::ui_tooltips_divider_sits_between_title_and_body ... ok
test hudskin::tooltip::tests::ui_tooltips_draw_matches_layout_from_desired ... ok
test hudskin::tooltip::tests::ui_tooltips_line_cap_holds_for_twenty_requested_lines ... ok
test hudskin::tooltip::tests::ui_tooltips_nine_patch_corners_keep_atlas_pixel_size ... ok
test hudskin::tooltip::tests::ui_tooltips_width_is_380_at_1080p ... ok
test hudskin::tooltip::tests::ui_tooltips_degenerate_inputs_are_safe ... ok
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 341 filtered out
```

Printed placement evidence (scale 1, 1920x1080, `TooltipRequest::new(0.0, 6, 3)`):

```
top-left:      anchor (40,20,64,64)     -> box x=0.0    y=90.0  w=380.0 h=249.0 above=false lines=6 costs=3
top-right:     anchor (1816,20,64,64)   -> box x=1540.0 y=90.0  w=380.0 h=249.0 above=false lines=6 costs=3
bottom-left:   anchor (40,996,64,64)    -> box x=0.0    y=741.0 w=380.0 h=249.0 above=true  lines=6 costs=3
bottom-right:  anchor (1816,996,64,64)  -> box x=1540.0 y=741.0 w=380.0 h=249.0 above=true  lines=6 costs=3
```

`scripts/h2-cargo check -p wc3-game --lib` → rc=0 (the module compiles outside the
test target as well).

## 8. Environment note

The first run failed with `No space left on device` (os error 28) while writing
`target/debug/deps/.../full.rmeta`: the workspace disk was at 100 % (380 kB
free), `target/` held 24 GB (16 GB `deps`, 2.6 GB stale `incremental`).
`scripts/h2-cargo clean -p wc3-game` removed 7.7 GiB of that package's
regenerable artifacts (491 files), bringing the volume to 92 % / 18 GB free and
unblocking every hudskin lane. A second failure was a sibling lane's
`hudskin/army.rs:462` (`index out of bounds` on a 2-element UV array, denied at
compile time by `unconditional_panic`); the owning lane fixed it and the run
went green — no change was made outside this lane's write scope.
