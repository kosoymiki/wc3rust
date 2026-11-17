# HUD skin — army chips and control groups (lane 9/10)

Status: **implemented** 2026-11-15. Owner file: `crates/wc3-game/src/hudskin/army.rs`.
Read-only contract used: `hudskin/mod.rs` (`Skin::nine` / `Skin::stretch`, atlas
roles), `hudskin/tests_support.rs` (fixture skin), `hud.rs` (`Rect`, `UiBatch`).

## 1. What the region is

Two pieces of the WC3 console that share the chip/plate language:

| Region | Where | Size at scale 1 | Source |
|---|---|---|---|
| Army chips | row of unit chips under the selection panel, one per selected unit | 34 px chip, 5 px gap | `mobile_hud.rs` `army_slots` (line 1414), `game.rs` army panel |
| Control groups | ten `1..9, 0` buttons in the console dead space between selection/minimap and the command card | 40 px button, 6 px gap | `game.rs::group_row_slots` (line 2510) |

## 2. Atlas roles

| Role in `ROLES` | Atlas rect (1536×1024) | Size | Used for |
|---|---|---|---|
| `slot_gold` | 1087, 554 | 102×99 | selected chip frame / active subgroup |
| `slot_dark` | 1203, 554 | 102×99 | normal chip frame |
| `wide_panel` | 267, 792 | 216×118 | control-group plate |

**Name note (observed).** The lane brief calls the 216×118 element `small_panel`.
`hudskin::ROLES` (read-only) has no `small_panel` key: the 216×118 element is
`wide_panel` (index 23). `ROLE_GROUP` therefore resolves to `wide_panel`, and
`ROLE_GROUP_ART_NAME = "small_panel"` keeps the brief's spelling for readers.
`thin_bar` (465×38) is **not** used here — a 38 px bar does not read inside a
34–68 px chip, so the micro HP bar is a solid quad; `thin_bar` belongs to the
topbar/stats lanes.

Measured insets from `analysis/h2/atlas/ui_atlas_insets.json` (seam-energy
method, `agent-hud/tools/atlas_insets_author.py`):

| Element | Insets `[l, t, r, b]` |
|---|---|
| `slot_gold` (`slot_banner`) | 42, 26, 42, 28 |
| `slot_dark` (`slot_plain`) | 21, 27, 21, 29 |
| `wide_panel` (`rows_plain`) | 19, 46, 21, 50 |
| `thin_bar` (`divider`) | 16, 16, 16, 16 |

## 3. Geometry rules (the contract this lane implements)

1. **Chip border** = `max(insets.l, insets.r) × chip.w / 102`, capped at 25 % of
   the chip side so the portrait hole keeps at least half the chip.
   - fixture insets (12 px): 34 → 4 px, 68 → 8 px, 102 → 12 px;
   - measured art (42 px): 34 → 14 px uncapped, **8.5 px after the cap** — the
     ornament is drawn, the portrait hole stays usable at phone sizes.
2. **Icon rect** = chip rect inset by the border on all four sides. Returned by
   `draw_chip` / `chip_icon_rect`; the caller paints the unit portrait there.
3. **Micro HP bar** = `4 px × scale`, never more than 25 % of the icon height,
   pinned to the bottom of the icon and spanning its width. A dark track is
   drawn first, then the fill `bar.w × hp_ratio`. Colour is red → olive → green,
   monotonic in both channels (`hp_color`).
4. **Row layout** — right-aligned: the last chip's right edge is `area.right()`,
   every chip's top is `area.y`. Needed width is `count·chip + (count−1)·gap`.
   When it exceeds `avail`, the `chip : gap` ratio is preserved and the row is
   solved exactly for `avail`:
   `gap = avail / (n·chip/gap + n − 1)`, `chip = (chip/gap)·gap`.
   With `gap = 0` the chips split `avail`. There is no minimum size, so the row
   always fits; zero count draws nothing and returns no rects.
5. **Control-group plate** — nine-patch of `wide_panel`; occupied groups are
   warm and solid, empty ones dim and translucent. The key box returned to the
   caller is the plate inset by the scaled atlas border plus a 3 px pad, capped
   at 25 % of the button, so the glyph never touches the frame.
6. **Selection state in the row** — chip 0 is the active subgroup and takes the
   `slot_gold` frame, the remaining chips take `slot_dark`.

## 4. API

```rust
pub const ROLE: &str = "slot_gold";          // selected chip frame
pub const ROLE_DARK: &str = "slot_dark";     // normal chip frame
pub const ROLE_GROUP: &str = "wide_panel";   // 216x118 plate (brief: small_panel)
pub const ROLE_GROUP_ART_NAME: &str = "small_panel";
pub const CHIP_UNIT: f32 = 34.0;             // chip side at scale 1
pub const HP_BAR_H: f32 = 4.0;               // bar height at scale 1

pub fn chip_border(skin: &Skin, rect: Rect) -> f32;
pub fn chip_icon_rect(skin: &Skin, rect: Rect) -> Rect;
pub fn chip_hp_rect(skin: &Skin, rect: Rect, scale: f32) -> Rect;
pub fn hp_color(ratio: f32) -> [f32; 4];

pub fn draw_chip(skin, batch, rect, hp_ratio, selected, scale, tint) -> Rect; // -> icon rect
pub fn draw_group(skin, batch, rect, key, occupied, scale) -> Rect;           // -> key box
pub fn group_row(origin: Vec2, count: usize, size: f32, gap: f32) -> Vec<Rect>;
pub fn group_row_centred(area: Rect, count: usize, size: f32, gap: f32) -> Vec<Rect>;
pub fn fit_row(avail: f32, count: usize, chip: f32, gap: f32) -> (f32, f32);
pub fn army_row_rects(area: Rect, count: usize, chip: f32, gap: f32) -> Vec<Rect>;
pub fn draw_army_row_in(skin, batch, area, count, chip, gap, hp) -> Vec<Rect>;
pub fn draw_army_row(skin, batch, origin: Vec2, count, chip, gap, hp) -> Vec<Rect>;
```

`draw_army_row` is the lane-contract entry point: `origin.x` is the row's right
edge, `origin.y` its top, and the available span is the HUD's left edge
(`x = 0`) to `origin.x`. Callers that own the selection rect must use
`draw_army_row_in(layout.army, …)` — same layout, explicit left bound — so the
row stays inside the selection column instead of merely inside the screen.

Draw order inside a chip is **icon first, then `draw_chip`**: the frame and HP
bar are emitted over the portrait, as WC3 draws the health bar across the icon.

## 5. Evidence

Isolated harness (byte copy of `army.rs` + the real `hudskin/mod.rs`
`Skin`/`ROLES` + real `tests_support.rs` + `hud.rs` excerpt for `Rect`/`UiBatch`),
`cargo test --offline`, 2026-11-15:

```
running 17 tests
test hudskin::army::tests::chip_border_keeps_the_icon_usable_with_the_measured_insets ... ok
test hudskin::army::tests::chip_border_scales_with_the_chip ... ok
test hudskin::army::tests::chip_quads_stay_inside_the_chip ... ok
test hudskin::army::tests::chips_never_overlap ... ok
test hudskin::army::tests::draw_army_row_anchors_the_right_edge_at_origin ... ok
test hudskin::army::tests::draw_army_row_shrinks_when_the_screen_is_narrow ... ok
test hudskin::army::tests::group_buttons_are_evenly_spaced ... ok
test hudskin::army::tests::group_plate_tint_reports_occupancy ... ok
test hudskin::army::tests::group_row_shrinks_inside_a_narrow_area ... ok
test hudskin::army::tests::group_text_box_never_collapses ... ok
test hudskin::army::tests::hp_bar_fill_follows_the_ratio ... ok
test hudskin::army::tests::hp_bar_sits_inside_the_chip_and_icon ... ok
test hudskin::army::tests::hp_colour_runs_red_to_green ... ok
test hudskin::army::tests::row_is_right_aligned_and_shrinks_to_fit ... ok
test hudskin::army::tests::selected_and_normal_chips_use_different_roles ... ok
test hudskin::army::tests::zero_count_draws_nothing ... ok
test result: ok. 17 passed; 0 failed
```

In-crate integration run, same day:

```
scripts/h2-cargo test -p wc3-game --lib hudskin::army
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 334 filtered out; finished in 0.01s
```

`rustfmt --edition 2024 --check crates/wc3-game/src/hudskin/army.rs` → clean.

Chip sizes for 5 / 8 / 12 units in a 480 px selection column at UI scale 2
(requested chip 68 px, gap 10 px), printed by
`army_row_chip_sizes_for_5_8_12_units`:

| Units | Chip | Gap | Row width | Shrunk |
|---|---|---|---|---|
| 5 | 68.00 | 10.00 | 380.00 | no (fits with 100 px to spare) |
| 8 | 53.16 | 7.82 | 480.00 | yes (ratio 6.8 preserved) |
| 12 | 35.25 | 5.18 | 480.00 | yes (ratio 6.8 preserved) |

At the live portrait HUD the army column floor is `240·scale` wide
(`mobile_hud.rs`, `selection_w.max(240·scale)`), i.e. 480 px at scale 2 — the
same column used above, so 8+ units already exercise the shrink path.

## 6. Tests and what each proves

| Test | Invariant |
|---|---|
| `chip_border_scales_with_the_chip` | border is proportional to the art (34→4, 68→8, 102→12 on the fixture) and a 4 px chip keeps a positive icon |
| `chip_border_keeps_the_icon_usable_with_the_measured_insets` | with the real 42 px `slot_gold` inset the 25 % cap keeps the icon ≥ half the chip (8.5 px border at 34 px) |
| `hp_bar_sits_inside_the_chip_and_icon` | 24/34/40/68/102 px chips: bar inside chip **and** icon, pinned to the icon bottom, ≤ ¼ icon height |
| `hp_bar_fill_follows_the_ratio` | track always drawn, fill width = `bar.w × ratio`, no fill at 0 |
| `hp_colour_runs_red_to_green` | 0 % red-dominant, 100 % green-dominant, monotonic channels, clamped |
| `selected_and_normal_chips_use_different_roles` | UVs come from `slot_gold` vs `slot_dark`; corner cell keeps the 12 px inset |
| `chip_quads_stay_inside_the_chip` | every vertex of every frame quad inside the chip, UVs in `[0,1]`, HP pieces inside |
| `chips_never_overlap` | counts 1..=12 × four chip/gap pairs: no overlap, every chip inside the area |
| `row_is_right_aligned_and_shrinks_to_fit` | right edge = area right, row inside the area, 12 chips shrink, ratio preserved; a row that fits keeps exact sizes |
| `army_row_chip_sizes_for_5_8_12_units` | the evidence table above; width = 380 px (fits) or exactly 480 px (shrunk) |
| `draw_army_row_anchors_the_right_edge_at_origin` | the required entry point anchors right/top at `origin`, draws frames and 2 HP quads per chip, chip 0 is gold |
| `draw_army_row_shrinks_when_the_screen_is_narrow` | the required entry point applies the same shrink rule when `origin.x` is the only bound |
| `zero_count_draws_nothing` | count 0: no rects, no textured quads, no white quads, `fit_row` returns zeros |
| `group_buttons_are_evenly_spaced` | 10 buttons: constant pitch = size+gap, centred, key box inside each button |
| `group_row_shrinks_inside_a_narrow_area` | 10 × 40 px in a 300 px area shrink instead of overflowing |
| `group_plate_tint_reports_occupancy` | occupied plate more solid and warmer than an empty one |
| `group_text_box_never_collapses` | 24/40/64/120 px buttons: key box positive and strictly inside the plate |

## 7. Risks / integration notes

1. **Stretched frame at small sizes (observed behaviour of `Skin::nine`).** With
   the measured 42 px `slot_gold` inset, a 34 px chip hits the degenerate rule
   (`w <= l + r + 1`) and `Skin::nine` emits one stretched quad instead of nine
   slices. That is the documented `mod.rs` fallback and is correct for the art
   at that size; a 34 px chip cannot show a 42 px corner. Worth a device
   screenshot once the lane is wired.
2. **Row bound.** `draw_army_row` can only bound the row by the screen edge;
   integration must call `draw_army_row_in(layout.army, …)` to keep chips inside
   the selection column.
3. **Key glyphs are caller-drawn.** The hudskin contract is rects + quads, so
   `draw_group` returns the key box and does not own a `Font`; the caller draws
   with `Font::draw_fit(..., MIN_DENSE_PX)`.
4. **Draw order.** `draw_chip` paints the frame and HP bar over the icon; the
   portrait must be drawn into the returned icon rect first.
5. **HP smoothing.** The bar follows the passed ratio literally (no animation);
   WC3 smooths the chip bar over ~200 ms. If the integration wants that, it must
   interpolate before calling — this module stays stateless.
