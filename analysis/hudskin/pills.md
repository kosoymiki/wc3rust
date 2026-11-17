# HUD skin lane 4/10 — resource pills (gold, lumber, supply, upkeep)

Date: 2026-11-15 · Ledger row 426 · Owner lane: `crates/wc3-game/src/hudskin/pills.rs`
(read-only region: the rest of `hudskin/`).

## Goal

Implement the WC3 resource shelf region of the fantasy-atlas HUD skin:

- `draw_pill(skin, batch, rect, active, scale, tint) -> Rect` — one framed pill
  (`resource_bar` nine-patched) returning the icon slot;
- `draw_upkeep(skin, batch, rect, scale) -> Rect` — the upkeep band
  (`thin_bar` stretched) returning the label box;
- layout helpers for the right-aligned row, icon, number and caption boxes.

## Atlas elements used (observed)

From `analysis/h2/atlas/slices.json` and `analysis/h2/atlas/ui_atlas_insets.json`:

| role | atlas rect (x,y,w,h) | insets (l,t,r,b) | file |
| --- | --- | --- | --- |
| `resource_bar` | 369, 224, 313, 62 | 27, 23, 27, 27 | `04_resource_bar_313x62.png` |
| `thin_bar` | 1050, 322, 465, 38 | 16, 16, 16, 16 | `08_thin_bar_465x38.png` |

`Skin::nine` / `Skin::stretch` own the UV math (contract in `hudskin/mod.rs`);
this module only picks the role and the target rect. `tests_support::sample_skin`
uses the same rects with 12 px insets everywhere, which is the fixture the
geometry tests run against.

## WC3 layout contract

At the 1080p reference (scale 1):

- pill: `158 x 34` px (`0.147 * 1080 = 158` wide);
- gap between pills: `6` px;
- pills: three, right-aligned inside the resource shelf, bottom-aligned,
  ordered gold, lumber, supply;
- upkeep band: `18` px tall, its bottom edge on the pills' top edge and its
  left/right edges on the row's first/last pill edge;
- inside a pill: icon square at the left (`24 * scale`, inset `4 * scale`),
  number row on top (58% of the inner height) and caption row beneath it, both
  sharing one column to the right of the icon.

All dimensions multiply the caller's HUD scale. The module never shrinks text
below the pill: `pill_row` shrinks the pills when the shelf is narrower than
`3 * 158 * scale + 2 * 6 * scale`, and every returned rect is clamped inside its
parent.

## API

```rust
pub const ROLE: &str = "resource_bar";
pub const UPKEEP_ROLE: &str = "thin_bar";
pub const PILL_COUNT: usize = 3;
pub const PILL_WIDTH: f32 = 158.0;
pub const PILL_HEIGHT: f32 = 34.0;
pub const PILL_GAP: f32 = 6.0;
pub const UPKEEP_HEIGHT: f32 = 18.0;

pub fn pill_row(shelf: Rect, scale: f32) -> [Rect; PILL_COUNT];
pub fn upkeep_band(shelf: Rect, scale: f32) -> Rect;
pub fn pill_icon_rect(pill: Rect, scale: f32) -> Rect;
pub fn pill_text_rects(pill: Rect, scale: f32) -> (Rect, Rect);
pub fn pill_number_rect(pill: Rect, scale: f32) -> Rect;
pub fn pill_caption_rect(pill: Rect, scale: f32) -> Rect;
pub fn pill_tint(active: bool, tint: [f32; 4]) -> [f32; 4];
pub fn draw_pill(skin: &Skin, batch: &mut UiBatch, rect: Rect, active: bool,
                 scale: f32, tint: [f32; 4]) -> Rect;
pub fn upkeep_text_rect(band: Rect, scale: f32) -> Rect;
pub fn draw_upkeep(skin: &Skin, batch: &mut UiBatch, rect: Rect, scale: f32) -> Rect;
```

`active` keeps the caller tint; idle pills use `tint * 0.72` RGB (alpha
unchanged) so the selected/alerted resource reads brighter without a second
atlas pass. Strings and their font quads stay with the caller (the `hudskin`
contract emits chrome quads only).

## Reference geometry (asserted by tests)

Shelf fixture used by the lane tests (right/bottom 24 px inside a 1080p-height
safe frame, 720 px wide at scale 1):

| viewport | scale | shelf (x,y,w,h) | gold | lumber | supply | upkeep band |
| --- | --- | --- | --- | --- | --- | --- |
| 2414x1080 | 1.0 | 1670, 1022, 720, 34 | 1904, 1022, 158, 34 | 2068, 1022, 158, 34 | 2232, 1022, 158, 34 | 1904, 1004, 486, 18 |
| 1080x2414 | 1.0 | 336, 2356, 720, 34 | 570, 2356, 158, 34 | 734, 2356, 158, 34 | 898, 2356, 158, 34 | 570, 2338, 486, 18 |

`scale` follows `hud::HudLayout::new(viewport).scale` (`min(w, h) / 1080`,
clamped `0.62..2.0`). At 2414x1080 and 1080x2414 that is exactly 1.0, so both
rows are 486 px wide and end on the shelf's right edge.

The module also satisfies the real console shelf
(`hud::mobile_hud::MobileHudLayout::resources`) in both orientations: its test
builds the actual `MobileHud` layout and checks every pill inside the shelf, the
row's right edge on the shelf's right edge, and no pill overlap (portrait
shelf is ~351 px wide at its internal scale, so the row shrinks instead of
escaping).

## Tests (all pass)

`scripts/h2-cargo test -p wc3-game hudskin::pills -- --test-threads=1`
→ `test result: ok. 10 passed; 0 failed` (observed 2026-11-15):

1. `hudskin_pills_row_right_aligned_2414x1080` — exact landscape rects, right
   edge on the shelf, all pills inside.
2. `hudskin_pills_row_right_aligned_1080x2414` — exact portrait rects.
3. `hudskin_pills_row_never_overlaps` — gap ≥ `6 * scale`, no intersections,
   four viewports.
4. `hudskin_pills_row_clamps_to_narrow_shelf` — 240 px shelf shrinks the row
   (76 px pills, gap kept); zero-sized shelf yields empty rects, never inverted.
5. `hudskin_pills_icon_inside_pill` — returned icon equals the helper, is
   `24 * scale` square, inside the pill; all pill vertices inside the pill and
   UVs in `[0, 1]`.
6. `hudskin_pills_nine_patch_corner_identity` — nine quads (54 vertices);
   top-left corner keeps the 12 px atlas width/height and the atlas UV corner;
   the bottom-right quad lands on the target corner with the atlas bottom-right
   UV.
7. `hudskin_pills_text_rects_inside` — number/caption inside the pill, positive
   area, same column, no overlap, right of the icon.
8. `hudskin_pills_active_state_is_brighter` — active keeps the tint exactly,
   idle is dimmed, alpha unchanged.
9. `hudskin_pills_upkeep_band_above_row` — band rests on the row, matches the
   row width, `18 * scale` tall; `draw_upkeep` emits one stretched quad with
   the full `thin_bar` UV rect; text box inside and equal to the helper.
10. `hudskin_pills_fit_mobile_hud_resource_shelf` — integration with the real
    `MobileHud` resource shelf in both orientations.

`scripts/h2-cargo check -p wc3-game --lib` → rc=0, no warning names
`hudskin/pills.rs` (observed 2026-11-15).

## Limitations / risks (observed)

- `resource_bar` insets are `27, 23, 27, 27`; at scale 1 the 34 px pill is
  shorter than `23 + 27 + 1`, so `Skin::nine` takes its documented degenerate
  path (`hudskin/mod.rs:59-61`) and emits one stretched quad instead of nine.
  The nine-patch path engages above ~1.5x scale; the atlas's central diamond
  then stretches with the centre cell (the element is a bar with a central
  ornament, not a plain frame). The lane brief fixes the role, so this is an
  atlas-authoring limitation, not a code choice.
- The runtime does not yet call `hudskin::pills`; `mobile_hud` still paints its
  own pills. Wiring the skin modules into the console draw path is the
  orchestrator's merge step, not this lane's write scope.
- Text boxes are layout only; number/caption/upkeep font quads are the
  caller's (font atlas, `hud::Font`).

## Reproduction

```sh
cd ~/src/herofolends-rs
scripts/h2-cargo check -p wc3-game --lib
scripts/h2-cargo test -p wc3-game hudskin::pills -- --test-threads=1
rustfmt --edition 2021 --check crates/wc3-game/src/hudskin/pills.rs
```
