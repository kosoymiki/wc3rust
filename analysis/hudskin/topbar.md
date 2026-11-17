# HUD skin — top bar (lane 2/10)

Code: `crates/wc3-game/src/hudskin/topbar.rs` (this lane's only code write target)
Atlas: `analysis/h2/atlas/elements/`, rects `analysis/h2/ui_atlas_manifest.json`,
insets `analysis/h2/atlas/ui_atlas_insets.json`
Date: 2026-11-15 · ledger 424 · `su -c 'id'` = `uid=0(root)` (KernelSU, `u:r:ksu:s0`)

---

## 1. What the top bar is

Warcraft III's play screen carries one thin strip at the top of the play area: the map name on the
left, the FPS/timer read-out on the right, the strip itself ~14 px tall at 1080p. It sits inside the
safe frame, it is a *band* (stretches horizontally), and its end ornaments keep their size — a
stretched ornament is the classic nine-slice failure, so the two ends are drawn from their own art
piece at natural aspect and the right end is the mirror of the left.

Constants (all multiplied by the caller's `scale`):

| constant | value | meaning |
|---|---|---|
| `BAR_HEIGHT` | 14.0 | strip height at 1080p reference |
| `UNDERLINE_HEIGHT` | 2.0 | gold line on the strip's bottom edge |
| `TEXT_PAD` | 6.0 | ornament → text gap |
| `TEXT_GAP` | 8.0 | map name → fps gap |
| `MIN_SCALE` | 0.05 | floor for a zero/NaN scale |

## 2. Atlas roles and measured insets

This lane owns four atlas roles (index = position in `hudskin::ROLES`):

| index | role | art (atlas px) | element file |
|---|---|---|---|
| 2 | `topbar_center` | 526×130 | `02_topbar_center_526x130.png` |
| 5 | `topbar_left` | 475×78 | `05_topbar_left_475x78.png` |
| 8 | `thin_bar` | 465×38 | `08_thin_bar_465x38.png` |
| 23 | `wide_panel` (lane name `small_panel`) | 216×118 | `23_wide_panel_216x118.png` |

### 2.1 Measurement (observed, 2026-11-15)

Method: `analysis/agent-hud/tools/atlas_insets_author.py` — for each edge, cut at the first position
`k` where the mean colour delta between the two columns (rows) either side of the cut falls to the
noise floor (`min × 1.35`, `min + 2`) and stays there for 3 consecutive positions; chroma-keyed
texels are skipped. The script was re-run against `analysis/h2/atlas/ui_atlas.png` with the manifest
rects of the four roles above.

Result — identical to the shipped sidecar for all four roles (so the constants in the source are the
art's own values, not a copy of a stale file):

| role | L | T | R | B | seam energy (L, T, R, B) |
|---|---|---|---|---|---|
| `topbar_center` | 55 | 36 | 55 | 55 | 15.10, 21.63, 11.41, 15.76 |
| `topbar_left` | 18 | 33 | 33 | 33 | 67.07, 6.04, 57.69, 12.79 |
| `thin_bar` | 16 | 16 | 16 | 16 | 23.58, 35.14, 38.54, 43.38 |
| `wide_panel` | 19 | 46 | 21 | 50 | 15.65, 9.42, 13.04, 15.56 |

Published as `INSETS_TOPBAR_CENTER`, `INSETS_TOPBAR_LEFT`, `INSETS_THIN_BAR`,
`INSETS_SMALL_PANEL` (plus `ART_*` art sizes). The high `topbar_left` left/right energies (67.07 /
57.69) are the art's gold arrow-head caps: that piece is an ornament, so it is drawn at natural
aspect (`Skin::stretch`), never nine-patched — its insets are published for completeness only.

### 2.2 Fitting a 130 px panel onto a 14 px bar

A nine-patch cell is part of the ornament, and in the art its size is proportional to the art's
height. The atlas body is a 130 px panel; the bar is 14 px, so the ornament band is drawn at the
art's uniform scale and only the middle band stretches:

* **source UV lines stay on the art's measured insets** (55 / 36 / 55 / 55 texels), so a corner cell
  shows the art's real corner ornament, uniformly shrunk;
* **destination cells are `inset × bar_height / art_height`** (`× 0.10769` at 14 px):

| cell | measured | on screen at 14 px |
|---|---|---|
| left | 55 | 5.92 |
| top | 36 | 3.88 |
| right | 55 | 5.92 |
| bottom | 55 | 5.92 |
| middle band (stretched) | 130−36−55 = 39 | 4.20 |

The strip therefore reads as gold rim (3.88 px) / stone band (4.20 px) / gold rim (5.92 px), with the
ornaments and the underline painted over it. Nothing samples outside the role's atlas rect.

**Why not `Skin::nine_scaled`.** It is exactly this geometry, but it floors `art_scale` at `0.25`
(`hudskin/mod.rs`, `nine_scaled`), i.e. it cannot shrink a 130 px art below 32.5 px. At 14 px the
fitted cells would be 13.75 / 9.0 px, `nine_scaled` would hit its degenerate guard
(`h ≤ t + b + 1`) and fall back to one stretched quad — the whole panel squashed, which is what the
render comparison in §2.3 shows is the worst option. `topbar::nine_sliced` therefore performs the
same computation with the two inset sets kept apart; at `art_scale = 1` it is quad-for-quad
identical to `Skin::nine`, pinned by `body_matches_the_shared_nine_patch_at_natural_height`.

**Why not shrink the skin's insets and reuse `Skin::nine`.** That variant (clone the skin, set
`insets = measured × 0.1077`) also scales the *source* UV band, so each corner samples only the
outer 5.92 × 3.88 texels of the art at 1:1 and the middle band compresses the whole 130 px panel into
4.2 px, keyed-corner texels included. Rendered side by side, that variant shows fringing and a flat
gold line where the chosen one keeps the corner ornament and the stone body (§2.3).

Each inset is also capped at 45 % of its axis so a pathological scale cannot eat the target, and a
skin whose insets do not describe its own rect (negative, or `l + r ≥ art_w`, `t + b ≥ art_h`) falls
back to the measured `INSETS_TOPBAR_CENTER` instead of a smear.

`thin_bar` is drawn as one stretched quad: at 2 px tall its own insets (16) fit no nine-patch
(`16 + 16 + 1 > 2`), and the correct reading at that height is a plain gold line.

### 2.3 Render comparison (observed)

Three candidate treatments of the 526×130 body were composited over a grey background at
1824 × 14 with the real (chroma-keyed) atlas: fitted two-inset nine-patch (chosen), skin-clone
`Skin::nine`, and `thin_bar` stretched as the body. Result: the chosen variant is the only one that
keeps a continuous gold-edged stone band with the ornaments intact; the clone variant degrades to a
flat gold/dark line with keyed-corner fringing at the ends, and `thin_bar` reads as a heavy gold
divider (centre diamond smeared) rather than a name strip. Scratch artefacts:
`$PREFIX/tmp/topbar_detail.png`, `$PREFIX/tmp/topbar_final_detail.png` (not committed).

## 3. Layout contract

`pub fn layout(skin, safe_frame, scale, map_name_x, fps_x) -> Option<TopBarLayout>`

| rect | formula |
|---|---|
| `bar` | `(safe_frame.x, safe_frame.y, safe_frame.w, 14·scale)` |
| `left_ornament` | `(bar.x, bar.y, 14·scale·475/78, bar.h)` |
| `right_ornament` | `(bar.right − w_orn, bar.y, w_orn, bar.h)` |
| `underline` | `(bar.x, bar.bottom − 2·scale, bar.w, 2·scale)` |
| `map_name` | `(clamp(map_name_x, band_left, fps.x − gap), bar.y, fps.x − gap − x, bar.h − 2·scale)` |
| `fps` | `(clamp(fps_x, band_left, band_right), bar.y, band_right − x, bar.h − 2·scale)` |

with `band_left = bar.x + w_orn + 6·scale`, `band_right = bar.right − w_orn − 6·scale`.

* Ornament width is `bar_height × art aspect` (475/78 = 6.0897) — **85.26 px at scale 1**, 52.86 px at
  the mobile floor `scale = 0.62`. It does not depend on the bar width: widen the screen and the
  ornament is unchanged (`ornaments_keep_their_aspect_and_never_stretch`).
* `map_name_x` / `fps_x` are the caller's requested left edges in screen px, clamped into the band
  between the ornaments; the map-name block never runs into the FPS block, and crossing them yields
  zero-width blocks instead of negative ones.
* Text height is `bar.h − 2·scale`, i.e. the face above the underline, so a font draw inside the
  returned rects cannot paint over the line or leave the strip.
* `underline` ends exactly at `bar.bottom()`: the strip's footprint is `14·scale` and nothing hangs
  into the play area.

`pub fn draw(skin, batch, safe_frame, scale, map_name_x, fps_x, tint) -> Option<(Rect, Rect)>` paints
`body → left ornament → mirrored right ornament → underline` and returns `(map_name, fps)`. `None`
only for a degenerate strip (zero-width safe frame, or a scale that cannot produce a positive
height); a skin missing a role skips that piece instead of failing.

The right end is the left art piece with the U axis flipped. `Skin` (read-only `hudskin/mod.rs`) has no
mirror helper, so `mirror_stretch` writes the same UV rectangle `Skin::stretch` builds with `u0`/`u1`
swapped — no new nine-patch geometry, no sampling outside the role rect.

## 4. Tests

`scripts/h2-cargo test -p wc3-game --lib hudskin::topbar` → **10 passed; 0 failed**.

| test | what it proves |
|---|---|
| `bar_is_fourteen_pixels_at_scale_one_and_scales_uniformly` | bar height 14 at scale 1, 28 at 2, spans `safe_frame` width, underline 2→4 |
| `quads_stay_inside_the_bar_rect` | every vertex inside `bar`, every quad samples one of the three roles, 9 body cells + 2 ornaments + 1 underline |
| `corner_cells_are_identical_at_1x_and_2x` | the 4 corner cells double exactly in both axes, keep their distance to the bar edge, UVs identical; ornament and underline double |
| `ornaments_keep_their_aspect_and_never_stretch` | ornament w/h = 475/78 at any bar width, full bar height, right end UVs are the left's swapped |
| `underline_closes_the_bar_and_never_enters_the_play_area` | underline inside `bar`, `bottom ≤ bar.bottom`, no intersection with the play area, text blocks above it |
| `text_rects_sit_between_the_ornaments` | padding to both ornaments, map name stops before the FPS block, heights = face height |
| `text_rects_clamp_into_the_band` | wild `map_name_x`/`fps_x` and crossed order clamp to non-negative in-band rects |
| `real_atlas_insets_fit_the_fourteen_pixel_bar` | the shipped 55/36/55/55 art at 14 px still nine-patches (9 cells, 5.92/3.88 cells, 4.2 px stone band), ornament 85.26 px |
| `body_matches_the_shared_nine_patch_at_natural_height` | at `art_scale = 1` the local slicer emits the same 9 quads with the same UVs as `Skin::nine` |
| `degenerate_inputs_do_not_emit_or_panic` | zero-width frame → `None`; zero scale → `MIN_SCALE`; unusable skin insets → measured fallback |

No GPU: the tests draw into a `UiBatch` and read back the vertices.

## 5. Limits and notes for the integrator

1. **Bar height source.** `analysis/tech-lead/HUD_REDRAW_TZ.md` §3/§5 asks for a 32 px mobile top bar
   and a 720×32 `topbar_plate`; the WC3 measurement and this lane's brief are ~14 px. The height here
   is `BAR_HEIGHT · scale`. If the mobile design keeps 32 px, the owner of that decision (tech-lead,
   `HUD_REDRAW_TZ.md`) must either pass `scale · 32/14` for this region or change `BAR_HEIGHT` — do
   not silently scale the whole HUD. Note that 32.5 px is exactly where `Skin::nine_scaled`'s 0.25
   floor becomes usable for this art.
2. **`nine_scaled` floor.** `hudskin/mod.rs::nine_scaled` floors `art_scale` at 0.25. That is right for
   panels at mobile scale but blocks any element more than 4× taller than its target; an optional
   `art_scale` floor parameter (or a two-inset variant) would let this module drop `nine_sliced`.
   Reported for the module owner; `mod.rs` is outside this lane's write scope.
3. **Mirror gap.** A `stretch_mirrored`/`nine_mirrored` helper in `hudskin/mod.rs` would remove the
   local UV flip in `mirror_stretch`; same ownership note as 2.
4. **Centre diamond.** The `topbar_center` art carries a gold diamond on the top edge; in a nine-patch
   the top edge is a stretched strip, so at 14 px that diamond reads as a brighter gold segment along
   the top rim rather than a distinct gem (`$PREFIX/tmp/topbar_final_detail.png`, centre crop).
   Removing it would need a sub-rect slice of the role, which `Skin` does not expose.
5. **`wide_panel` / `small_panel`** is a stacked plate (two rows with their own gems), not part of a
   14 px band: it is not drawn here. Its measured insets are published so the lane's ownership table
   is complete and the art is measured once.
