# Minimap frame — WC3 console layout (hudskin lane 5/10, ledger 427)

Region: the minimap widget at the far left of the console. Module:
`crates/wc3-game/src/hudskin/minimap_frame.rs`. Date: 2026-11-15.
Author domain: HUD skin geometry (atlas art), Termux/Android aarch64, workspace
`/data/data/com.termux/files/home/src/herofolends-rs`.

## 1. Contract

```rust
pub fn draw(skin: &Skin, batch: &mut UiBatch, origin: Vec2, side: f32,
            scale: f32, tint: [f32; 4]) -> MinimapParts
```

`origin`/`side` describe the **image square** — the 216x216 authored minimap,
i.e. `MobileHudLayout::minimap` (`crates/wc3-game/src/mobile_hud.rs:1349-1355`,
`216.0 * scale` at `console.x + 24*scale`, `console.y + 48*scale`). The ornament
is dilated *outwards*, so the layout's hit rect stays the image and the touch
target does not move when the frame art changes.

| field | meaning | drawn here? |
| --- | --- | --- |
| `frame` | outer ornamented rect, `side + left+right` x `side + top+bottom` | yes — one nine-patch of `dialog_wide` |
| `image` | minimap texture target (terrain + fog + blips), exactly `side x side` | no — returned |
| `header` | band above the frame, `side` wide, carries the word MINIMAP | yes — one stretched `thin_bar` |
| `viewport_rect_slot` | rect the normalised camera rectangle maps into | no — returned |

`camera_rect(slot, [x0, y0, x1, y1])` maps a normalised camera rect into the
slot (corners ordered, clamped to 0..1, non-finite input → empty rect).

Draw order owned by the caller: `frame` + `header` (this module) → minimap
texture into `image` → fog mask → blips → `camera_rect` outline → "MINIMAP"
text into `header`. Nothing in this module samples outside its two atlas
elements (`ui_minimap_quads_stay_inside_the_frame_and_the_atlas_element`).

## 2. Atlas roles

| role | atlas rect | sidecar insets (l,t,r,b) | use |
| --- | --- | --- | --- |
| `dialog_wide` | 391x267 @ (1124,672) | 45, 113, 39, 56 | frame nine-patch |
| `thin_bar` | 465x38 @ (1050,322) | 16, 16, 16, 16 | header band |
| `wide_panel` (called `small_panel` in the lane brief) | 216x118 @ (267,792) | 19, 46, 21, 50 | **not used** |

Sources: roles order `crates/wc3-game/src/hudskin/mod.rs:16-18`; rects
`analysis/h2/atlas/slices.json`; insets
`analysis/h2/atlas/ui_atlas_insets.json` (`panel_dialog` index 21, `divider`
index 8, `rows_plain` index 23).

`small_panel` is not a role name anywhere in the tree (`rg -n small_panel` → no
hits outside the brief); 216x118 is `wide_panel`/`rows_plain`. It is a rows
plate, not a frame: its four cuts are as thick as the dialog panel's
(19/46/21/50), so it cannot wrap a 216 px square either, and a square frame from
a rows plate would show its row seam across the middle. Rejected with evidence,
not by preference.

### Measured border

Two independent measurements of `dialog_wide`:

1. Sidecar (seam cut heuristic, `analysis/agent-hud/tools/atlas_insets_author.py`):
   `45, 113, 39, 56` px.
2. Direct edge-energy scan of `analysis/h2/atlas/ui_atlas.png` (mean |Δluma|
   across the element, first quiet run of 6 samples < 3.0): first quiet column
   from the left **41**, from the right **38**, first quiet row from the bottom
   **44**; the top band is quiet only at **113**.

So the panel carries a ~38-45 px rim on three sides and a 113 px title band on
top. A 216 px square wrapped in those cuts gives a frame of
`216+45+39 = 300` x `216+113+56 = 385` px — taller than the 288 px console
(`CONSOLE_HEIGHT`, `mobile_hud.rs:29`), i.e. it would paint over the world and
the console bevel. That is why the border is budgeted:

```
budget      = max(side * MAX_BORDER_FRACTION, BORDER_FLOOR_PX)   // 0.125, 1.0 px
border[i]   = clamp(skin.insets[role][i], BORDER_FLOOR_PX, budget)
frame       = square dilated by border  (side + left+right, side + top+bottom)
image       = the square itself
header.h    = max(side * bar_aspect, HEADER_MIN_PX * scale)      // 38/465, 12 px
header      = (image.x, frame.y - header.h, side, header.h)
slot        = image inset by VIEWPORT_INSET_PX * scale           // 1.0 px
```

The budget is *load bearing* and tested:
`ui_minimap_oversized_atlas_cuts_stay_inside_the_console_budget` feeds the real
45/113/39/56 cuts and asserts the unbudgeted height (385 px) exceeds the
console while the budgeted frame fits.

The hudskin skin table is authored at 12 px uniformly
(`crates/wc3-game/src/hudskin/tests_support.rs:40`), which is inside the
budget at every tested size, so `frame = side + 24` there.

## 3. Geometry, measured (scale 1.1, `HUD_SCALE_BOOST`)

`cargo test -p wc3-game --lib hudskin::minimap_frame -- --nocapture`, printed by
the two orientation tests:

2414x1080 (landscape):

```
console = (52.8, 710.4, 2308.4, 316.8)   card = (2092.3, 839.2, 242.5, 179.2)
frame   = (67.2, 751.2,  261.6, 261.6)   image = (79.2, 763.2, 237.6, 237.6)
header  = (79.2, 731.8,  237.6,  19.4)
```

1080x2414 (portrait):

```
console = (26.4, 2026.8, 1027.2, 316.8)  card = (676.0, 2075.2, 351.2, 259.6)
frame   = (40.8, 2067.6, 261.6, 261.6)   image = (52.8, 2079.6, 237.6, 237.6)
header  = (52.8, 2048.2, 237.6,  19.4)
```

Both orientations: frame ⊂ console (14.4 px slack at the bottom, the console's
bevel), header ⊂ console, `frame ∩ card = 0 px²` (frame right 328.8 vs card x
2092.3 landscape; 302.4 vs 676.0 portrait), image = the layout hit rect, and
image/frame width = 237.6/261.6 = 0.908 (the ornament is a border, not a body).
At 2414x1080 the image is 237.6 px because the layout scales the authored 216 px
square by 1.1 — the *border* stays 12 px, see §4.

## 4. Invariants proven by the tests

`cargo test -p wc3-game --lib hudskin::minimap_frame` → `14 passed; 0 failed`,
rc=0. Mechanic id `ui.minimap` (`analysis/tech-lead/MECHANICS_GAP_MAP.md:80`),
test prefix `ui_minimap_`.

| test | proves |
| --- | --- |
| `ui_minimap_frame_wraps_the_square_with_the_measured_border` | frame = square + 2x12 at origin (100,50): frame (88,38,240,240), image (100,50,216,216), slot (101,51,214,214), band on the frame top edge |
| `ui_minimap_image_rect_is_strictly_inside_the_frame` | strict inequality on all four edges for sides 64/216/480 x scales 0.62/1/2; band never intersects frame or image |
| `ui_minimap_nine_patch_centre_cell_is_the_image_rect` | the emitted nine-patch centre (quad 4 of 9) **is** the image rect, and the four corners stay 12x12 |
| `ui_minimap_frame_keeps_the_square_aspect` | frame.w == frame.h and image == side x side for 96/216/512 |
| `ui_minimap_asymmetric_ornament_keeps_the_square_image` | insets 10/14/8/6 → frame (w+18, h+20) while the image stays square, centre cell == image |
| `ui_minimap_corner_identity_is_stable_at_1x_and_2x` | side 216 vs 432: identical corner UVs (1e-6) and identical 12x12 corner size; frame grows by exactly 216 |
| `ui_minimap_oversized_atlas_cuts_stay_inside_the_console_budget` | real cuts 45/113/39/56 → frame = side + 2*budget, fits the console, unbudgeted would not |
| `ui_minimap_square_at_2414x1080_clears_the_console_and_the_card` | real `MobileHudLayout` rects, landscape: frame/header inside console, `intersection_area(frame, card) == 0` |
| `ui_minimap_square_at_1080x2414_clears_the_console_and_the_card` | same in portrait |
| `ui_minimap_header_band_spans_the_square_and_keeps_the_bar_aspect` | band 216 x 17.65 (465:38 aspect), full `thin_bar` UVs, legibility floor 12 px at side 40 |
| `ui_minimap_degenerate_side_draws_nothing` | side 0/-5/NaN/INF and NaN origin → `MinimapParts::default()`, no quads |
| `ui_minimap_quads_stay_inside_the_frame_and_the_atlas_element` | every quad inside frame/header, every UV inside its own element rect and 0..1 |
| `ui_minimap_camera_rect_maps_into_the_viewport_slot` | full/partial/unordered/overhanging cameras, NaN → empty, mapped rect stays inside slot ⊂ image |
| `ui_minimap_viewport_slot_scales_with_the_hud` | slot inset follows `scale` (2 vs 4 px), frame identical at 1x and 2x |

## 5. Known limits and follow-ups

1. **`Skin::nine` slices at atlas pixels — the ornament does not scale.**
   `crates/wc3-game/src/hudskin/mod.rs:53-81` uses `self.insets[index]` directly
   as destination corner sizes, while the sibling mechanism
   `NinePatch::with_art_scale` (`crates/wc3-game/src/hud.rs:376-379`) multiplies
   the corners by `art_scale`. At HUD scale 1.1 the frame border is therefore
   12 px instead of 13.2 px (observed in §3: 261.6 - 237.6 = 24 = 2x12). Not
   fixable from this module: `mod.rs` is another lane's file. Recommendation for
   the wiring lane: add `Skin.art_scale` (mirroring `NinePatch`) and multiply
   the insets in `nine()`; the module needs no change because it derives the
   frame from the same insets `nine()` slices at.
2. **Tiny minimaps degrade to a stretched panel.** With the real dialog cuts a
   frame smaller than `l + r + 1` px trips `Skin::nine`'s degenerate branch
   (`mod.rs:60-62`) and the whole panel art is squashed. Budgeted frames do not
   hit it at the authored sizes (side >= ~48 px), but a caller that shrinks the
   minimap far below 48 px will see it. Keep the square at the layout's
   216 x scale.
3. **The ornament lies outside the hit rect.** `MobileHudLayout::hit` returns
   `HudHit::Minimap` for `layout.minimap` only, so a tap on the 12 px ornament
   misses the minimap. This module is drawn-only; if the ornament should be
   clickable, the layout (not this file) must expand the hit rect by the border.
4. `small_panel`/`wide_panel` stays unused by this region (see §2), so a future
   `header` plate must not be invented without art evidence.

## 6. Environment note (this session)

Two `cargo test` attempts failed with `mold: failed to write to an output file.
Disk full?` / `clang: Bus error` — the device had 180 MB free. Pruning
`target/debug/incremental` and `target/debug/examples` (regenerable build
outputs, no sources, no other lane's data) raised free space to 11 GB and both
`cargo check -p wc3-game --lib` (rc=0) and the test run succeeded. All cargo
runs went through `scripts/h2-cargo` (single-slot lock + 3.2 GB MemAvailable
floor); no parallel cargo was started.

A second attempt was blocked for ~5 minutes by a cross-lane compile error in
`crates/wc3-game/src/hudskin/army.rs:450` (`gold_u[2]` on a `[f32; 2]` uv,
`deny(unconditional_panic)`) which made the whole lib-test target unbuildable
for every lane. The army lane was still writing that file, so this lane did not
touch it; the defect was reported through the H2 supervisor ledger
(checkpoint 427) and the owner fixed it. Nothing in this lane's files depended
on it.

## 7. Colour-space pass 2026-11-16 (ledger 438) — console defect found

The linear-colour change (`hud::srgb_to_linear` + `linear_color` in
`UiBatch::textured_quad`/`UiBatch::quad`, ledger 438) does not touch this
module's geometry: `draw` writes no colour constant of its own, it passes the
caller's tint through `Skin::nine`/`Skin::stretch`, so both chrome quads now
carry linear vertex colours and display as the authored atlas tint.

Re-running `ui_minimap_square_at_1080x2414_clears_the_console_and_the_card`
against the current portrait composition exposed one **real console defect**
that is not in this module and not in this lane's owner paths
(`crates/wc3-game/src/mobile_hud.rs` is read-only here):

```
1080x2414 scale=1.1
console  { x: 26.4, y: 1718.0, w: 1027.2, h: 696.0 }
card     { x: 647.6, y: 2109.0, w: 398.0,  h: 297.0 }
frame    { x: 34.4, y: 1726.0, w: 276.0,  h: 276.0 }   // image 252x252 at (46.4,1738)
header   { x: 46.4, y: 1705.4065, w: 252.0, h: 20.5935 }
```

`header.y = 1705.41 < console.y = 1718.00`: the MINIMAP caption band lands
**12.59 px above the console band**, i.e. on the world, because the portrait
composition starts the ring at `ppad = 8 * k` (`mobile_hud.rs:1916-1918`,
`ring_rect = Rect::new(console.x + ppad, console.y + ppad, ring_w, ring_h)`)
while the chrome above the ring is `border_top + header_h = 12 + 20.5935 =
32.59 px`. The landscape composition reserves `>= 40 * k` above the console top
(`mobile_hud.rs:1821`), which is why the 2414x1080 twin of this test
passes; portrait reserves 8.

Exact fix (owner: mobile_hud.rs, 2 lines, no magic numbers):

```rust
// after `minimap_side` is known
let header_h = (minimap_side * 38.0 / 465.0).max(12.0 * k);   // thin_bar aspect / HEADER_MIN_PX
let ppad = 8.0 * k + header_h;                                 // was: 8.0 * k
// and the portrait stack's leading gap in `console_h` (mobile_hud.rs:1714):
//   8.0 * k + ring_h + ...  ->  ppad + ring_h + ...
```

Effect: console 696.0 -> 708.59 px (29.35% of 2414), `header.y` 1705.41 ->
1713.41, i.e. 8 px inside the console top, card and card pitch unchanged.
Until that lands, `ui_minimap_square_at_1080x2414_clears_the_console_and_the_card`
stays red by design: the assertion is the composition contract, not a
tolerance to loosen.
