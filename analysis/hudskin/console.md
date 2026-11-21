# HUD skin — console plate (lane 1/10)

Date: 2026-11-15 · Ledger: 423 · Region: bottom console
Owner file: `crates/wc3-game/src/hudskin/console.rs`
Art: `analysis/h2/atlas/elements/{00_console_left_cap_457x202,01_console_right_cap_457x202,22_menu_button_pressed_592x208}.png`
Atlas manifest: `analysis/h2/ui_atlas_manifest.json` (1536x1024, 24 elements)
Layout source: `analysis/agent-re/ui_audio/ENGINE_MAP.md` §1.1, §1.3

## 1. Goal and contract

The console is the full-width band under the play area. This lane draws only that
plate:

* left cap `console_left_cap` (457x202) at the left edge,
* right cap `console_right_cap` (457x202) at the right edge — the sheet stores it
  already mirrored (ornament on the right column), so no UV flip is needed,
* the wide centre plate stretched between the caps,
* nothing outside the caller's rect; the band must stay clear of the safe-area
  insets.

`draw(skin, batch, rect, scale, tint) -> PlateParts { left, centre, right }`
returns the three screen rects so the caller can place the minimap / card.
`layout(screen_w, screen_h, safe)` produces the band (see §5).

Not owned here: topbar, command tiles, portraits, tooltips, dialogs.

## 2. Sources read (observed, 2026-11-15)

| source | what it gave |
|---|---|
| `crates/wc3-game/src/hudskin/mod.rs:47,53` | `Skin::role_index`, `Skin::nine` — the only nine-patch path a module may use; UVs are never computed in a region module |
| `crates/wc3-game/src/hudskin/tests_support.rs` | fixture skin: 1536x1024 atlas, the 24 manifest rects, 12 px insets everywhere |
| `analysis/h2/ui_atlas_manifest.json` | element rects; row 22 = `[511,708,592,208]`, row 23 = `[267,792,216,118]` |
| `analysis/h2/atlas/slices.json` | role names per row: row 22 `menu_button_pressed`, row 23 `wide_panel` |
| `analysis/h2/atlas/elements/*.png` | visual check of the three pieces (see §3, §4) |
| `analysis/agent-hud/ui_atlas_insets.json` | earlier inset measurement (sidecar), rows 0/1/22/23 |
| `analysis/agent-re/ui_audio/ENGINE_MAP.md:54-77,130-137` | command-card constants and derived pixel table (0.129 band) |
| `crates/wc3-game/src/style.rs:160,211` | `Chrome::register_atlas`, `Chrome::frame(art_scale)` — the established art-scale convention |
| `crates/wc3-game/src/hud.rs:139,155` | `UiBatch`, `textured_quad` |

## 3. Role resolution (naming drift, decided by art)

The lane brief names the centre role `wide_panel` with size 592x208. The sheet
disagrees on the name, not on the size:

| manifest row | role in `slices.json` | rect | what the PNG shows |
|---|---|---|---|
| 22 | `menu_button_pressed` | 592x208 | a rectangular plate: gold bevel on all four edges, dark stone centre, a diamond ornament top-centre and a larger one bottom-centre, red/gold end columns |
| 23 | `wide_panel` | 216x118 | two stacked thin rows (gold-topped and grey-topped) — a list/row strip, not a console band |

The inset sidecar files row 22 under the name `console_plate`
(`analysis/agent-hud/ui_atlas_insets.json`, index 22), matching what the art is.
Two independent facts (the 592x208 size in the brief, the art itself) point at
row 22, so `console_index()` selects the centre **by authored size**
(`PLATE_ART = [592, 208]`) among the candidate roles, falling back to the named
`menu_button_pressed` and only then to `wide_panel`. A skin that carries the
plate under another row name still gets the right art; a skin with only the
216x118 strip still draws something.

Constants: `ROLE_CENTRE = "menu_button_pressed"`,
`ROLE_CENTRE_FALLBACK = "wide_panel"`, `PLATE_ART = [592.0, 208.0]`,
`centre_role(skin)`.

## 4. Measured nine-slice insets (observed)

`INSETS = [56, 32, 56, 64]` (left, top, right, bottom) in atlas px, measured on
`22_menu_button_pressed_592x208.png`.

Method (reproducible):

1. Ornament mask: `alpha > 200 && r >= 60 && r - b >= 25` — captures the gold
   bevel and the red gem columns; the neutral/cool stone interior does not match.
2. For left/right, take the perpendicular middle band (rows 25%..75%) so the
   horizontal bevel is not sampled; for top/bottom take columns 25%..75% so the
   vertical gems are not sampled.
3. Walk inward from each edge to the last ornament-free boundary. The first
   boundary whose *inner rectangle* is completely ornament-free is the inset.

Measurement script (kept here so the numbers can be re-derived and checked):

```python
from PIL import Image
import numpy as np
a = np.asarray(Image.open('analysis/h2/atlas/elements/22_menu_button_pressed_592x208.png')
               .convert('RGBA')).astype(np.int16)
r, g, b, al = a[..., 0], a[..., 1], a[..., 2], a[..., 3]
o = (al > 200) & (r >= 60) & (r - b >= 25)             # gold bevel + red gem
h, w = o.shape
insets = [56, 32, 56, 64]                              # [l, t, r, b]
def inner(v):
    l, t, r, b = v
    return int(o[t:h - b, l:w - r].sum())
assert inner(insets) == 0                              # plain-stone centre
for i in range(4):                                     # each side is tight
    v = insets.copy(); v[i] -= 1
    assert inner(v) > 0, (i, inner(v))                 # 1 / 165 / 1 / 3 px
```

Verification of the result (falsifiable):

| check | result |
|---|---|
| inner rect `[56..536] x [32..144]` (478x112 = 53 536 px) warm pixels | **0** |
| shrink left by 1 px (`[55,32,56,64]`) | 1 warm pixel appears |
| shrink top by 1 px (`[56,31,56,64]`) | 165 warm pixels appear |
| shrink right by 1 px (`[56,32,55,64]`) | 1 warm pixel appears |
| shrink bottom by 1 px (`[56,32,56,63]`) | 3 warm pixels appear |

So every side is individually tight: no side can move inward without pulling
ornament into the stretchable centre. The boundary is not unique to the
algorithm — a variant that trades 6 px of gem fringe for 6 px of top bevel,
`[50,38,50,64]`, also leaves a clean centre — but the values above follow the
gold edge on both axes (top/bottom bevel exactly 32/64, gem fringe fully inside
the left/right fixed cells), so the corner split matches the art's own corners.
Alpha bbox of the element is `0,0..591,207` — the art fills its manifest rect
exactly.

Earlier sidecar (`analysis/agent-hud/ui_atlas_insets.json`, row 22,
`console_plate`) reports `[64,88,88,66]` from a different algorithm
(`analysis/agent-hud/tools/atlas_insets_author.py`, "first seamless cut per
edge", seam energy `[8.15,2.77,5.96,10.03]`). Its inner rect is also clean but
conservative: top 88 would park ~56 px of plain stone plus bevel shape in the
fixed corner. This lane publishes the tight boundary; `Skin::nine` uses whatever
`skin.insets` the loader supplies, and `draw` substitutes `INSETS` when the skin
carries none.

Caps: `console_left_cap` / `console_right_cap` are single ornaments, not slices.
In the same middle band the ornament spans columns 4..452 (left) and 4..447
(right) with **no 40-px flat run**, i.e. there is no stationary band to stretch
horizontally. Their target width is already derived from the authored aspect
(`h * 457 / 202`), so the plate draws each cap as one quad through
`Skin::nine` (insets zeroed for the duration of the draw → `nine` takes its
one-cell path). Slicing them with fixed pixel insets would keep corners at
authored size and deform the frame at other band heights.

## 5. Layout and geometry

`HEIGHT_FRACTION = 129/1000` (`0.129`), from the retail command-bar arithmetic
`3 * 0.0455 - 0.0075 = 0.129` (`ENGINE_MAP.md:54-77`).

* `layout(screen_w, screen_h, safe)`: `h = min(0.129 * screen_h, screen_h - safe.top - safe.bottom)`,
  `w = screen_w - safe.left - safe.right`, `y = screen_h - safe.bottom - h`.
  The band sits directly above the bottom inset; the height is unchanged unless
  the safe area leaves less room than the retail band.
* 1080p: `h = 139.32`; 2414 px: `h = 311.406`.
* `cap_width(skin, role, h) = h * atlas_w / atlas_h` (457/202 for both caps).
  Caps may not exceed half the band each; when the band is narrower than the two
  natural caps, the cap shrinks uniformly (width *and* height) and is centred
  vertically, so the `457:202` aspect survives the clamp.
* `centre = [left.right() .. right.x] x rect.y .. rect.bottom()`.
* Draw order: left cap → centre → right cap.
* `scale` = screen px per atlas px (same notion as `Chrome::frame`'s
  `art_scale`). It multiplies the centre insets: callers pass the band's art
  scale (e.g. `rect.h / 208`) so the bevel keeps its authored proportion on a
  denser screen; `1.0` draws the atlas 1:1. Caps ignore `scale` — their geometry
  is fully determined by `rect.h` and the authored aspect.
* Guards: `scale` non-finite or `<= 0` → `1.0`; a zero/negative target draws
  nothing; a skin without a role draws the remaining pieces and returns a
  zero-width rect for the missing one.

## 6. Height unit caveat (evidence, not applied)

`ENGINE_MAP.md:130-137` states the unit space: `0.6 units = screen height`, so
the retail band `0.129 units` is `0.215 * screen_h` (232 px at 1080p) if read
literally in that space. The lane contract fixes the console at `0.129 *
screen_h` (129/1000) and the acceptance test pins 1080/2414 to that fraction.
The implementation follows the lane contract; `HEIGHT_FRACTION` is the single
knob if a fidelity pass decides the unit-space reading. Marked as a known
discrepancy, not silently resolved.

## 7. Tests (`#[cfg(test)]` in `console.rs`)

| test | invariant |
|---|---|
| `nine_patch_corners_are_identical_at_1x_and_2x_target` | centre emits 3x3 cells; the four corner quads keep the same px size and UVs when the target width doubles |
| `no_quad_outside_the_target_rect_or_its_element` | every vertex inside `rect`, every UV inside `[0,1]` and inside one of the three element rects (no atlas bleed) |
| `caps_keep_the_authored_aspect_within_one_pixel` | `w - h*457/202 <= 1 px` for 1080p, 2414, 2560-wide and the clamped narrow band; caps flush to the band edges; centre spans the gap; plate spans the band |
| `console_height_is_129_of_1000_of_the_screen` | `l.h == 139.32` (1080) and `311.406` (2414), equal to `screen_h * 129/1000` |
| `layout_keeps_the_plate_off_the_safe_area` | `[24,0,24,48]` insets: width 1032, bottom `screen_h-48`, height unchanged; tall safe area clamps the height inside the free band |
| `measured_insets_are_used_when_the_skin_has_none` | a skin with zero insets still slices 9 cells at 56x32 / 56x64 — `INSETS` is live, not documentation |
| `scale_multiplies_the_centre_insets` | `scale 1.5` gives 84x48 corner cells |
| `degenerate_rect_draws_nothing_and_stays_in_bounds` | zero-size targets emit nothing; a 100-px-wide band clamps the caps and stays inside |

## 8. Risks / limitations

* **Neighbour lanes**: `cargo test --lib` compiles every region's test module.
  During this lane's run, `pills.rs` (E0432 `crate::mobile_hud`), `topbar.rs`
  (E0599 `textured_len_for_test`) and `tiles.rs` (E0599 `role_index` used as a
  method) failed to compile. Those files are read-only for this lane; the
  console module itself compiles and its test results are recorded in §9 once
  the tree is green. This is a shared-tree race, not a console defect.
* **Portrait screens**: at 1080x2414 the retail band is 311 px tall while each
  natural cap is ~705 px wide; the aspect lock clamps both caps to 540 px
  (238 px tall, vertically centred) and the centre band becomes zero-width. The
  invariant (no stretched ornament, nothing outside the rect) holds, but a
  portrait layout needs a different console composition — out of this lane's
  scope.
* **Unit-space caveat** in §6 is unresolved by design (lane contract wins).
* **Cap override**: the module zeroes cap insets. If a future sheet ships a true
  three-piece cap with a stretchable middle (a repeating band, not one ornament),
  that override must be revisited.
* **Scale coupling**: `scale` must describe the same mapping as `rect`; passing
  `1.0` with an already-scaled rect compresses the centre band (the insets stay
  at atlas px).
* **Nine-patch seams**: cells share exact edges/UVs; at fractional scales a
  GPU's top-left fill rule covers the boundary, so no cracks are expected, but
  this is not device-verified in this lane (headless tests only).

## 9. Verification

Shared-tree compile of the non-test module (observed):

```
scripts/h2-cargo check -p wc3-game --lib     -> Finished dev profile ... RC=0
rustfmt --edition 2021 --check crates/wc3-game/src/hudskin/console.rs -> rc=0
```

Test runs (`scripts/h2-cargo test -p wc3-game --lib hudskin::console`) first
failed to *compile* on other lanes' test code in the same test target:
`hudskin/pills.rs` (E0432 `crate::mobile_hud`), `hudskin/topbar.rs` (E0599
`textured_len_for_test`), `hudskin/tiles.rs` (E0599 `role_index` called as a
method), `hudskin/army.rs:450` (`#[deny(unconditional_panic)]`, `uv` has length
2 and is indexed at 2). Those files are read-only for this lane.

To verify this module while the shared tree is mid-flight, the same file was
compiled and tested in an isolated harness:

* `crates/wc3-game/src/hudskin/console.rs` and `tests_support.rs` were
  **symlinked** into the harness (`console.rs` sha256
  `48b6a490e971b3213fad08308231534682a2379d2ae6cd86846d1038922519f4`);
* `mod.rs` = verbatim `Skin`/`ROLES` with the other region `pub mod` lines
  stripped; `Rect`/`UiVertex`/`UiDraw`/`UiBatch::textured_quad` = verbatim
  excerpt of `hud.rs`; dependencies `glam 0.30` + `bytemuck 1` (the workspace's).

```
cargo test   -> running 8 tests
  hudskin::console::tests::no_quad_outside_the_target_rect_or_its_element ... ok
  hudskin::console::tests::console_height_is_129_of_1000_of_the_screen ... ok
  hudskin::console::tests::caps_keep_the_authored_aspect_within_one_pixel ... ok
  hudskin::console::tests::nine_patch_corners_are_identical_at_1x_and_2x_target ... ok
  hudskin::console::tests::degenerate_rect_draws_nothing_and_stays_in_bounds ... ok
  hudskin::console::tests::layout_keeps_the_plate_off_the_safe_area ... ok
  hudskin::console::tests::scale_multiplies_the_centre_insets ... ok
  hudskin::console::tests::measured_insets_are_used_when_the_skin_has_none ... ok
test result: ok. 8 passed; 0 failed; 0 ignored; RC=0
```

Final shared-tree rerun (same file, once the neighbour lanes' test code
compiled):

```
scripts/h2-cargo test -p wc3-game --lib hudskin::console
  test hudskin::console::tests::caps_keep_the_authored_aspect_within_one_pixel ... ok
  ... (all 8) ...
  test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 343 filtered out; finished in 0.01s
  RC=0
```

`console.rs` sha256 at delivery:
`48b6a490e971b3213fad08308231534682a2379d2ae6cd86846d1038922519f4`.


