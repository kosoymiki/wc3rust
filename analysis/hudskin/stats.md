# HUD skin — HP/mana bars and stats rows (lane 7/10)

Code: `crates/wc3-game/src/hudskin/stats.rs` (this lane's only code write target)
Report: this file (`analysis/hudskin/stats.md`, the lane's only analysis write target)
Atlas: rects/insets read from `analysis/h2/atlas/slices.json` and
`analysis/h2/atlas/ui_atlas_insets.json`; `tests_support::sample_skin()` carries the
same 24 rects with synthetic 12 px insets.
Date: 2026-11-15 · ledger **429** · `su -c 'id'` = `uid=0(root)` (KernelSU, `u:r:ksu:s0`)

---

## 1. What this region is

Warcraft III's unit-info block, top to bottom:

1. **HP bar** — a gold/stone frame with a green fill; the current HP number is drawn *inside* the
   bar (the WC3 read-out is not a label beside it).
2. **Mana bar** — the same frame one bar gap lower, blue fill.
3. **Stats rows** — `ARMOR`, `DAMAGE`, `RANGE`, `SPEED` (and whatever else the caller passes),
   one row per stat on an alternating dark panel strip.

The module emits quads into the existing `UiBatch` and returns the rects the caller renders text
into. It never rasterises text itself, so the font path stays the caller's (`hud::Font`).

## 2. Atlas roles

| ROLES index | role | art (atlas px) | manifest name | insets L/T/R/B (sidecar, observed 2026-11-15) |
|---|---|---|---|---|
| 18 | `hp_bar` | 403×65 | `bar_hp` | 28 / 28 / 17 / 28 |
| 8 | `thin_bar` | 465×38 | `divider` | 16 / 16 / 16 / 16 |
| 23 | `wide_panel` (lane brief: `small_panel`) | 216×118 | `rows_plain` | 19 / 46 / 21 / 50 |

`small_panel` from the lane brief is **not** a `ROLES` string; the manifest calls the same
216×118 element `wide_panel` (`slices.json:217`, `ui_atlas_insets.json` index 23). `ROW_ROLE` uses
`wide_panel`, otherwise `Skin::role_index("small_panel")` returns `None` and the rows would draw
nothing (found by the first test run: `role exists` panic in `role_uv`).

The HP and mana bars share the one bar frame in the atlas — there is no separate mana frame art.
`thin_bar` is sampled whole and stretched; it is a flat fill strip in this atlas, so stretching it
does not smear an ornament.

## 3. Geometry contract

All destination geometry is in screen px; `scale` is the caller's HUD art scale.

### 3.1 Frame → inner band

`draw_hp` / `draw_mana` draw the frame with `Skin::nine(batch, "hp_bar", rect, tint)` — the
read-only helper owns nine-patch geometry, per the `hudskin/mod.rs` contract. The fill must cover
the frame's inner band and never a corner, so `inner_rect(skin, rect)` mirrors exactly what
`Skin::nine` does:

```
if rect.w <= l + r + 1 || rect.h <= t + b + 1:
    inner = rect.inset(1, 1)          # Skin::nine falls back to one stretched quad
else:
    inner = rect.inset_sides(l, t, r, b)
```

The 1 px rim in the fallback branch keeps the stretched frame edge visible; the fill never reaches
the frame border. A skin without the `hp_bar` role has no frame, so `inner = rect`.

`inner_rect` is `pub` so callers (and the tests) can place text or hit-tests against the same band
the fill uses.

### 3.2 Fill

```
ratio = ratio.clamp(0, 1)              # NaN → 0
if ratio == 0 or inner.w <= 0:        no quad
else: Skin::stretch("thin_bar", (inner.x, inner.y, inner.w * ratio, inner.h), tint)
```

* `ratio = 0` emits **no** fill quad (checked by `fill_width_tracks_ratio`).
* `ratio = 1` emits one quad exactly covering `inner` (`ratio_one_fills_the_whole_inner_rect`).
* The fill is left-anchored, full inner height; it can never exceed the frame
  (`fill_never_exceeds_the_frame`, including the 240×22 retail bar, a 30×18 rect smaller than a
  nine-patch, and a 1×1 rect).

Colours: `HP_FILL = [0.22, 0.75, 0.24, 1.0]` (green), `MANA_FILL = [0.25, 0.45, 0.95, 1.0]`
(blue). The HP caller tint is applied to the frame only; the fill colour is the region's own data.

### 3.3 Number rect

The returned rect is the inner band minus a scaled text padding:

```
pad_x = min(2 * scale, inner.w * 0.25)
pad_y = min(1 * scale, inner.h * 0.25)
number = inner.inset_sides(pad_x, pad_y, pad_x, pad_y)
```

It is always inside the inner band, and therefore inside the frame. `draw_mana` returns the same
shape for the mana number (the layout contract in `mobile_hud` draws `MANA n/m` centred there).

### 3.4 Stats rows

```
row_h  = min(ROW_HEIGHT * scale, rect.h / rows.len())    # ROW_HEIGHT = 18.0
row[i] = (rect.x, rect.y + row_h * i, rect.w, row_h)
panel  = Skin::nine("wide_panel", row[i], ROW_TINTS[i % 2])
text[i] = row[i].inset_sides(pad_x, pad_y, pad_x, pad_y)
         pad_x = min(4 * scale, rect.w * 0.25)
         pad_y = min(1 * scale, row.h * 0.25)
```

* `ROW_TINTS = [[0.10, 0.09, 0.07, 0.32], [0.18, 0.16, 0.12, 0.20]]` — dark/lighter, both
  translucent, alternating per row.
* Rows are top-aligned and never overlap (`stats_rows_stay_inside_and_do_not_overlap`); if the
  caller's slot is shorter than `n * ROW_HEIGHT * scale`, `row_h` shrinks so the block still fits
  inside `rect` instead of spilling out.
* Empty `rows` draws no quads and returns an empty `Vec`.
* At 18 px row height `wide_panel`'s 46/50 px top/bottom insets make `Skin::nine` take its
  single-quad fallback (18 ≤ 46+50+1), i.e. a stretched panel strip — the intended low-alpha band,
  not a smear of a gem (see §6.2).

## 4. API

```rust
pub fn inner_rect(skin: &Skin, rect: Rect) -> Rect;
pub fn draw_hp(skin: &Skin, batch: &mut UiBatch, rect: Rect, ratio: f32,
               scale: f32, tint: [f32; 4]) -> Rect;      // -> HP number rect
pub fn draw_mana(skin: &Skin, batch: &mut UiBatch, rect: Rect, ratio: f32,
                 scale: f32) -> Rect;                     // -> mana number rect
pub fn draw_stats(skin: &Skin, batch: &mut UiBatch, rect: Rect,
                  rows: &[(&str, &str)], scale: f32) -> Vec<Rect>;
pub fn draw(skin: &Skin, batch: &mut UiBatch, rect: Rect, tint: [f32; 4]) -> bool; // stub contract
```

`scale` is rejected when zero/NaN/infinite (`sane_scale` → 1.0); target rects with non-finite or
negative extents collapse to zero and emit no fill. Nothing samples outside its role's atlas rect:
the frame goes through `Skin::nine`, the fill through `Skin::stretch`, the rows through `Skin::nine`.

### 4.1 Integration (caller)

```rust
let bar_h = 22.0 * scale;                       // mobile_hud layout.hp_bar height
let hp = Rect::new(x, y, w, bar_h);
let hp_number = draw_hp(&skin, batch, hp, hp_ratio, scale, tokens.frame);
let mana = Rect::new(hp.x, hp.bottom() + BAR_GAP * scale, hp.w, 18.0 * scale);
let mana_number = draw_mana(&skin, batch, mana, mana_ratio, scale);
let rows = draw_stats(&skin, batch, stats_slot,
                      &[("ARMOR", armor), ("DAMAGE", damage), ("RANGE", range), ("SPEED", speed)],
                      scale);
```

`draw_hp`/`draw_mana` position nothing: the caller owns the vertical order (the test asserts a
mana rect placed below the HP rect emits all of its quads strictly under the HP frame).

## 5. Verification (observed)

`scripts/h2-cargo test -p wc3-game --lib hudskin::stats -- --nocapture` → **8 passed; 0 failed**,
`rc=0`, `343 filtered out`, finished in 0.01 s. Test log:
`/data/data/com.termux/files/usr/tmp/hudskin-stats-test.log`.

Measured with the shared fixture (`sample_skin`, 12 px insets) and target
`Rect { x: 100, y: 50, w: 400, h: 120 }`:

| quantity | value |
|---|---|
| inner rect | `Rect { x: 112, y: 62, w: 376, h: 96 }` |
| fill at ratio 0.0 | 0 quads |
| fill at ratio 0.5 | 1 quad, **188.0 px** wide (0.5 × 376) |
| fill at ratio 1.0 | 1 quad, **376.0 px** wide (inner width) |

With the shipped sidecar insets (28/28/17/28) the same target gives
`inner = Rect { x: 128, y: 78, w: 355, h: 64 }` and fills of 0 / 177.5 / 355 px (arithmetic from
the same formula; the in-tree test fixture uses 12 px so the numbers above are the measured ones).

| test | what it proves |
|---|---|
| `fill_width_tracks_ratio` | ratio 0/0.5/1 → fill width 0 / 0.5·inner.w / inner.w (±1 px), fill inside inner, number rect inside frame |
| `ratio_one_fills_the_whole_inner_rect` | ratio 1 covers `inner` exactly (x, y, w, h) |
| `fill_never_exceeds_the_frame` | 4 target sizes × 4 ratios × 2 scales: every quad inside the frame, every fill inside the inner band (incl. 240×22 retail height, 30×18, 1×1) |
| `frame_corners_keep_their_atlas_size` | nine-patch emits 9 pieces; TL/TR/BL corner cells keep atlas inset size and pin to the frame edges |
| `mana_bar_sits_strictly_under_hp` | all mana quads start at/under `hp.bottom`; mana fill = 0.5·inner.w; mana number inside its bar |
| `stats_rows_stay_inside_and_do_not_overlap` | 4 rows: one text rect each, inside `rect`, pairwise non-intersecting, top-to-bottom; panel strips inside the slot and alternate `ROW_TINTS` |
| `stats_empty_rows_draw_nothing` | empty row list → no quads, empty result |
| `region_draws_inside_its_rect` | legacy stub contract: every vertex inside the target, UVs in [0,1] |

## 6. Limits and notes for the integrator

1. **Retail bar height vs frame insets.** `bar_hp` is 65 px tall with 28 px top + 28 px bottom
   insets, so `Skin::nine` cannot keep its corners in a 22 px bar (`22 ≤ 28 + 28 + 1`): the frame
   is drawn as one stretched quad and `inner_rect` then uses the 1 px-rim rule. Fixing that needs a
   *fitted* nine-patch (insets scaled by `bar_h / 65`, like the top bar lane's fitted body) or a
   sub-rect API, both inside read-only `hudskin/mod.rs` — recorded here for the module owner.
   Geometry stays correct either way (fill is inside the frame, number inside the fill).
2. **Stats panel art.** `wide_panel` is a stacked two-row plate with gems (216×118). At an 18 px
   row it necessarily falls back to a stretched quad, so the alternating tint reads as a subtle
   band rather than two stacked plates. That is the honest use of the only row panel in this atlas;
   a dedicated one-row strip role would need new art.
3. **One frame for two bars.** The atlas has no mana frame; HP and mana share `bar_hp`. Colour is
   the only differentiator, which matches the WC3 reading (green over blue in the same chrome).
4. **Number is caller text.** The module returns rects, never glyphs; the font atlas and the
   upper-case HUD style live in `hud.rs`.
5. **No sub-rect slicing.** `Skin` exposes whole roles only; the fill samples the whole
   `thin_bar`, which is correct for a flat strip.
