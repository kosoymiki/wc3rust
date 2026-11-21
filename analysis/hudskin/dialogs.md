# dialogs — main menu, skirmish setup, victory/defeat, game menu, quest log

Lane 10/10 of the HUD-skin split (ledger 432). Owns exactly two files:
`crates/wc3-game/src/hudskin/dialogs.rs` and this document.

Region contract is `crates/wc3-game/src/hudskin/mod.rs` (read-only for this lane):
a module picks an atlas role and a destination rect; nine-patch cells and UVs
live in `Skin::nine_scaled` (`Skin::nine` is the scale-1 alias). Nothing here
draws text — every draw call returns the rect the caller puts the label or the
text into.

## 1. Public surface

```rust
pub enum PanelKind { Menu, Dialog, Quest }

// the three mandated entry points
pub fn draw_panel(skin, batch, rect, kind: PanelKind, scale: f32) -> Rect;   // content box
pub fn draw_button(skin, batch, rect, pressed: bool, enabled: bool, scale: f32) -> Rect; // label box
pub fn draw_rows(skin, batch, content: Rect, count: usize, row_h: f32, scale: f32) -> Vec<Rect>;

// shared geometry the region needs (also used by the compositions)
pub fn nine_scaled(skin, batch, role, rect, scale, tint) -> bool;  // fit-aware wrapper
pub fn border(skin, role, scale) -> [f32; 4];                      // measured border, screen px
pub fn panel_content(skin, rect, kind, scale) -> Rect;
pub fn row_pitch(avail, count, row_h, gap) -> f32;
pub fn layout_buttons(content, count, columns, height, gap) -> Vec<Rect>;

// compositions for the five surfaces in this region
pub fn menu_scale(skin, viewport) -> f32;   pub fn dialog_scale(..) -> f32;  pub fn quest_scale(..) -> f32;
pub fn centred_panel(skin, role, viewport, scale) -> Option<Rect>;
pub fn quest_rect(skin, viewport, scale) -> Option<Rect>;
pub fn draw_banner(skin, batch, rect, scale) -> Rect;                   // title cartouche
pub fn draw_menu(skin, batch, viewport, scale, entries, pressed: Option<usize>) -> Option<MenuLayout>;
pub fn draw_dialog(skin, batch, viewport, scale, buttons, pressed: Option<usize>) -> Option<DialogLayout>;
pub fn draw_quest_log(skin, batch, viewport, scale, quests) -> Option<QuestLogLayout>;
```

`MenuLayout { panel, content, buttons, labels }`,
`DialogLayout { panel, content, banner, banner_text, buttons, labels }`,
`QuestLogLayout { panel, content, rows }`.

## 2. Atlas roles → surfaces

| surface | `PanelKind` | role | slice (atlas px) | measured insets l/t/r/b |
|---|---|---|---|---|
| main menu, skirmish setup | `Menu` | `dialog_wide` | 391x267 | 45 / 113 / 39 / 56 |
| game menu, victory, defeat | `Dialog` | `dialog_panel` | 328x311 | 66 / 68 / 67 / 56 |
| quest log list | `Quest` | `list_panel` | 218x287 | 92 / 68 / 27 / 29 |
| button at rest | — | `button_normal` | 217x119 | 27 / 45 / 24 / 50 |
| button held | — | `menu_button_pressed` | 592x208 | 64 / 88 / 88 / 66 |
| title cartouche | — | `wide_panel` | 216x118 | 19 / 46 / 21 / 50 |

Sizes: `analysis/h2/atlas/slices.json` (indices 21, 3, 19, 20, 22, 23).
Insets: `analysis/h2/atlas/ui_atlas_insets.json` (first-seam measurement,
`analysis/agent-hud/tools/atlas_insets_author.py`), used verbatim in the
`measured_skin()` test fixture so the tests run on the shipped geometry, not only
on the synthetic 12 px fixture.

Note: the lane brief lists `wide_panel` as 592x208. Both `slices.json` (index
23) and `hudskin::ROLES` order give **216x118**; 592x208 is
`menu_button_pressed` (index 22). This module uses the manifest size.

## 3. Mechanism

### 3.1 Nine-patch and the DPI scale

`Skin::nine_scaled(batch, role, rect, tint, art_scale)` scales the *destination*
insets by `art_scale` and keeps the *source* insets at the authored atlas pixels,
so a corner keeps the same texels and UVs while its on-screen size grows with the
HUD scale (12 px at 1x, 24 px at 2x). `dialogs::nine_scaled` adds one rule:

```
effective = min(scale, (rect.w - 2) / (inset.l + inset.r), (rect.h - 2) / (inset.t + inset.b))
```

If the scaled bevel does not leave a cell of centre, the art scale is reduced
until it does, instead of letting the shared code fall back to one stretched quad.
Without that, a quest row shorter than the list bevel would squeeze the whole
218x287 sheet into a strip: `list_panel` keeps 68+29 atlas px of frame, which is
97 px at scale 1 — taller than a normal row.

### 3.2 Panel content

`panel_content` = panel rect inset by `border(role, scale) + CONTENT_PAD*scale`,
capped at half the side minus one cell, so the content box is **always strictly
inside** the panel, even for a 3x3 panel. Without art (role absent) the same
padding-only box is returned and nothing is drawn.

### 3.3 Buttons

* rest → `button_normal`, pressed → `menu_button_pressed` (spec: different roles);
* pressed also takes the WC3 press feedback: rect scaled to 0.96 about the centre
  (2% off each side) and the tint darkened 8% (`BUTTON_PRESSED_TINT`);
* disabled multiplies RGB by `DISABLED_DIM = 0.65` — "tinted down 35%", alpha kept
  at 1.0 so a disabled plate still occludes what is behind it;
* the **label box** is derived from the *normal* plate border with a per-side cap
  of 18% of the side (`LABEL_INSET_FRAC`). `button_normal` is a two-plate sheet,
  so its measured vertical insets (45+50 of 119) would leave a 1 px label on a
  70 px button; the cap keeps the label usable while staying inside the bevel.

### 3.4 Quest rows

`draw_rows` lays out `count` rows inside `content` with `ROW_GAP` between them and
draws each as a `list_panel` nine-patch, alternating `ROW_TINT` (even) and
`ROW_TINT_ALT` (odd, ~18% darker). Shrink rule (`row_pitch`):

```
gap_total = gap * (count - 1)
pitch     = min(row_h, max(0, (avail - gap_total) / count))
```

so an oversubscribed list ends exactly on the content bottom edge instead of
overflowing it; when not even a 1 px row fits, `Vec::new()` is returned (the
caller reduces the count or grows the list) — never overlapping rows.

### 3.5 Composition and the compound atlas

`dialog_wide` (4 baked row guides) and `list_panel` (4 baked row plates) are
*compound* sheets, not clean nine-slices: their centres contain decoration. The
compositions therefore size those panels as `atlas_size * scale` and pass the
same `scale` to the nine-patch — with proportional destination insets a
nine-patch on a proportional rect is a **uniform scale of the art**, so the baked
guides are never smeared. `dialog_panel` has a clean centre and takes any rect.

Panel scales (`menu_scale`, `dialog_scale`, `quest_scale`) fit a fraction of the
viewport (safe frame, not the raw screen), so the menus cover the same share of
the screen at 2414x1080 and 1080x2414:

| surface | landscape fractions | portrait fractions |
|---|---|---|
| menu | 0.50 w / 0.80 h | 0.90 w / 0.42 h |
| dialog | 0.86 w / 0.78 h | same |
| quest | 0.24 w / 0.80 h, right-anchored (2.5% margin) | 0.86 w / 0.80 h, centred |

Menu lanes use `MENU_ROW_GAP = 2` logical px — the plate already carries the row
separators, a wider gap is what makes four lanes look thin.

## 4. Layout evidence

Observed by `layout_evidence_for_2414x1080_and_1080x2414`
(`cargo test -p wc3-game --lib hudskin::dialogs::tests::layout_evidence -- --nocapture`).
Safe frames follow `MobileHudLayout` (`SafeArea::landscape_1080` = 48/80/48/48,
`portrait_1080` = 24/96/24/64, scaled by the 1.10 HUD boost), so
landscape = `(48, 88, 2318, 939.2)`, portrait = `(24, 105.6, 1032, 2238)`.
The menu/dialog/list scales are viewport fits (§3.5) and are deliberately larger
than the 1.10 HUD console scale: a menu covers a large share of the screen.

### 2414x1080

| element | scale | rect (x, y, w, h) |
|---|---|---|
| menu panel | 2.814 | 656.847, 181.920, 1100.306, 751.360 |
| menu content | | 805.993, 522.424, 818.898, 230.755 |
| menu button 0 | | 805.993, 522.424, 818.898, 53.468 |
| menu label 0 | | 910.114, 532.048, 619.098, 34.219 |
| dialog panel | 2.356 | 820.690, 191.312, 772.620, 732.576 |
| dialog content | | 995.001, 370.334, 421.643, 402.799 |
| dialog banner | | 1102.596, 370.334, 206.452, 112.784 |
| dialog button 0 | | 995.001, 716.741, 201.400, 56.392 |
| dialog label 0 | | 1033.830, 727.613, 123.740, 34.647 |
| quest panel | 2.552 | 1751.730, 191.399, 556.320, 732.403 |
| quest content | | 2006.923, 385.345, 211.810, 444.035 |
| quest row 0 | | 2006.923, 385.345, 211.810, 65.499 |
| quest row 5 (last) | | 2006.923, 763.881, 211.810, 65.499 |

### 1080x2414

| element | scale | rect (x, y, w, h) |
|---|---|---|
| menu panel | 2.375 | 75.600, 907.478, 928.800, 634.245 |
| menu content | | 201.499, 1194.907, 691.255, 194.787 |
| menu button 0 | | 201.499, 1194.907, 691.255, 45.134 |
| menu label 0 | | 289.390, 1203.031, 522.598, 28.885 |
| dialog panel | 2.706 | 96.240, 803.840, 887.520, 841.521 |
| dialog content | | 296.473, 1009.485, 484.348, 462.701 |
| dialog banner | | 420.070, 1009.485, 237.154, 129.556 |
| dialog button 0 | | 296.473, 1407.408, 231.351, 64.778 |
| dialog label 0 | | 341.078, 1419.897, 142.142, 39.800 |
| quest panel | 4.071 | 96.240, 640.384, 887.520, 1168.432 |
| quest content | | 503.359, 949.794, 337.909, 708.388 |
| quest row 0 | | 503.359, 949.794, 337.909, 104.494 |
| quest row 5 (last) | | 503.359, 1553.688, 337.909, 104.494 |

Cross-checks on the numbers above: 6 rows × 65.499 + 5 × 10.208 (gap = 4·2.552)
= 444.035 = the landscape content height; the portrait list ends at
1553.688 + 104.494 = 1658.182 = content bottom. Every label box is ≥ 28.9 px tall
(the `MIN_TEXT_PX = 24` floor of `hud.rs` is met at both viewports).

## 5. Tests (16, GPU-free, `UiBatch` only)

| test | pins |
|---|---|
| `panel_content_is_strictly_inside_for_every_kind_and_skin` | 3 kinds × 2 skins × 4 scales × 2 viewports: content strictly inside, every quad inside the panel |
| `panel_content_follows_the_measured_border` | content = rect ∓ (`measured inset + CONTENT_PAD`)·scale, exactly |
| `panel_content_survives_a_tiny_panel` | 3x3 / 20x12 / 200x40 panels: content never escapes, never negative |
| `nine_patch_corner_identity_at_1x_and_2x` | corner UV identical at 1x/2x, corner size = inset·scale, cells partition the rect, corner ≠ edge UV |
| `nine_scaled_matches_skin_nine_at_1x` | the wrapper at 1x is `Skin::nine` for 5 roles (cells, UVs, tint) |
| `nine_scaled_clamps_instead_of_squeezing_the_sheet` | a 300x40 row still yields 9 cells with a usable centre instead of one squeezed quad |
| `panels_use_their_own_role` | every panel quad's UV lies inside its role rect; the three roles differ |
| `pressed_and_normal_use_different_roles` | normal UVs inside `button_normal`, pressed inside `menu_button_pressed`, different art, 0.96 shrink, label inside |
| `disabled_tint_is_strictly_darker` | disabled luminance ratio = 0.65 ± 0.005, alpha kept, pressed+disabled < pressed |
| `buttons_never_overlap_for_3_6_8_rows` | 3/6/8 buttons × 1/2/3 columns: no pairwise intersection, all inside content, 8 rows shrink to the slot, 3 rows keep 64 px |
| `rows_fit_or_shrink_by_the_documented_rule` | 1..12 rows × 40/64/220 px: pitch follows `row_pitch`, no overlap, shrunken list ends flush, 0.5 px list reports empty |
| `rows_alternate_tint` | row 0 ≠ row 1 tint, odd darker, row 2 back to base |
| `menu_composition_stays_inside_the_viewport` | 2 skins × 2 viewports: panel < viewport, content < panel, 4 buttons + labels inside, 3/6/8 entries tile without overlap |
| `dialog_composition_keeps_banner_and_buttons_inside` | banner and 1/2/3 buttons inside the content, banner text inside the banner |
| `quest_log_rows_sit_inside_the_list_frame` | 6 rows inside the list content, no overlap, every quad is `list_panel` art |
| `layout_evidence_for_2414x1080_and_1080x2414` | prints §4 and asserts the contract at both viewports |

Run:

```sh
scripts/h2-cargo test -p wc3-game --lib hudskin::dialogs
scripts/h2-cargo test -p wc3-game --lib hudskin::dialogs::tests::layout_evidence -- --nocapture
```

## 6. Atlas limitations found (evidence, not fixes)

1. `button_normal` (217x119) is a **two-variant sheet**: the top plate is the
   stone button, the bottom is the same button lit gold (`gold rows` at y=3,
   49-52, 62-67, 71-73, 103, 111-115 in the slice). Its measured insets
   (45 top / 50 bottom) describe both plates, not one button, so a single button
   drawn from the sheet is a compressed pair of rails. The fix is art, not code:
   `HUD_REDRAW_TZ.md` §5 item 19 already specifies a 256x64 `menu_button` with
   16 px insets — one plate per slice. Same for `wide_panel` (216x118, two
   stacked plain plates) and `menu_button_pressed` (592x208, one wide plate).
2. `dialog_wide` and `list_panel` bake 4 row guides into their centres, and the
   measured top inset of `dialog_wide` (113 of 267) is *deeper* than the guide
   pitch: the art's own lanes do not fit inside the art's own measured content
   box. Consequence: menu buttons tile the content box and read as framed plates
   over the stone plate rather than filling the baked lanes. Exact alignment
   needs redrawn art with a header-only top inset (`dialog_wide` again).
3. `list_panel`'s measured left inset (92 of 218) is a left spine, so the quest
   content is 45% of the plate width (211.8 of 556.3 px in landscape). That is
   faithful to the slice; a wider list needs a wider slice.

## 7. Findings outside this lane's write scope

* `hudskin/mod.rs` `contract_tests::nine_scaled_grows_corners_not_uvs` originally
  asserted `batch.textured.len() == 9` after `Skin::nine`. `UiBatch::textured_quad`
  (`hud.rs:166-175`) appends to the previous `UiDraw` when the texture matches, so
  nine cells land in **one** `UiDraw` with 54 vertices. Observed during this lane,
  **fixed by the mod.rs owner in the same session** (the test now asserts one
  merged draw with `9 * 6` vertices); the full `hudskin::` suite is green after it.
* During this lane the shared workspace was edited concurrently: `army.rs:462`
  (`gold_u[2]` on a `[f32; 2]`), `wc3-render/fog.rs:213`
  (`FilterMode` vs `MipmapFilterMode`) and `hudskin/mod.rs:208` (mirror test
  indexing per-vertex `[f32; 2]` UVs as a quad UV box) each broke
  `cargo test` for a few minutes and cleared on retry. One did not: `assets.rs`
  added `MeshData.compact: Option<CompactMesh>` and left the test literal at
  `render.rs:1727` without it, blocking every lane's verification for >10 min;
  this lane added the single `compact: None` line there (mechanical, test-only,
  same semantics as the pre-change struct) and the test
  `render::tests::doodad_batch_applies_layer_alpha_and_team_tint` passes.
