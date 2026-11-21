# Nine-patch UI pipeline — chroma-key atlas + FDF insets + button states

Layer: NINE-PATCH UI ART PIPELINE (ledger 418). Repo `crates/wc3-game`.

## Why

The HUD drew `ui.rect` flat fills, 1 px `ui.border` lines and **full-quad texture
stretches**. Every ornament therefore deformed with the panel, so the HUD read as a
prototype even with shipped WC3 art. This layer adds the missing mechanism: a texture
element is drawn as nine pieces whose corners keep their authored size while the edge
bands and centre take up the difference.

## Mechanism

| Piece | File | What it does |
|---|---|---|
| Chroma-key loader | `src/ui_atlas.rs` | PNG/BLP decode → key removal → anti-alias defringe → RGB edge clamp → `Atlas { image, elements }` from a **manifest** (no pixel constant in code) |
| Nine-patch geometry | `src/hud.rs` | `NinePatch { texture, source, insets, atlas, art_scale, tile }`, `nine_slices()`, `UiBatch::nine_patch()` |
| Frame table | `src/style.rs` | `Chrome` registers every frame texture with measured insets and every atlas element by name (`button_normal`, `panel_tooltip`, ...) |
| FDF/theme path | `src/ui.rs`, `src/fdf_ui.rs` | `UiTheme::frame_insets/frame_patch`, `FrameArt::resolve`, `nine_slice_sides(rect, [l, t, r, b])` |
| HUD wiring | `src/mobile_hud.rs` | console plate, minimap frame, selection panel, card plate, command tiles (5 states + cooldown sweep + hotkey overlay), tooltip shell, HP/mana bars, army chips, dialog |

### Loader contract

1. **Key** — the manifest states the key (`"magenta (>200,<80,>200)"`); `ChromaKey::parse`
   reads the numbers, nothing is hardcoded.
2. **Defringe** — anti-aliased key blends survive an exact key match. In the operator's
   sheet 15 126 texels are magenta-dominant but do not match the strict bounds; left in
   place they render as a purple fringe on every element edge. Two rings of
   magenta-dominant texels *adjacent to the keyed area* lose their alpha (safely: violet
   art far from the hole is untouched).
3. **Edge clamp** — keyed texels borrow the mean colour of their opaque neighbours for
   three rings, so bilinear filtering interpolates toward art, never toward magenta.
4. **Alpha mode** — the UI pipeline is straight-alpha (`crates/wc3-render/src/ui.rs`
   blends with `BlendState::ALPHA_BLENDING` and returns `textureSample(...) * color`), so
   the HUD path loads `AlphaMode::Straight`. `AlphaMode::Premultiplied` exists for
   offscreen compositing and is covered by tests; premultiplying the HUD sheet would
   darken every translucent texel twice.
5. **Insets** — authored per element in `analysis/agent-hud/ui_atlas_insets.json`
   (measured by `tools/atlas_insets_author.py`); a texture without authored insets gets
   `detect_insets()`, which measures where the art becomes stationary along each axis.
   The sidecar repeats each element rect only so drift against the manifest is detected
   (`AtlasError::RectDrift`); the manifest stays the single source of truth for geometry.

### Atlas element roles (measured from the sheet)

`console_plate` · `console_left/right` · `title_bar` · `title_small` · `plate_stone` ·
`divider` · `frame_large` · `frame_medium` · `panel_tooltip` · `panel_dialog` ·
`parchment` · `button_normal/hover/pressed/disabled/cooldown/empty` · `slot_banner` ·
`slot_plain` · `bar_hp` · `list_frame` · `rows_button` · `rows_plain`

The HUD asks for elements by name through `Chrome::element_or`, so an absent sheet falls
through to the redrawn pack, then to the retail WC3 art, then to a bevel — never to a
panic and never to a stretched ornament.

### Button states

`command_look()` maps a `CommandButton` to state art (`pressed` > `disabled` > `hover` >
`cooldown` > `normal`, `empty` for an absent slot), and the tile keeps the r2 contract:
the only text frame a tile owns is the hotkey overlay. Cooldown draws a quad curtain over
the remaining fraction plus the bottom progress bar.

## Evidence (measured)

* Corner identity: the 40×40 corner block of a 200×120 frame and of the same frame at
  400×240 are the **same pixels** (0 mismatches out of 1600 texels); the corner UV rect is
  scale-free; the stretched middle *does* differ (positive control).
* Art scale 2×: the 80×80 corner is a texel-exact 2× upscale of the 40×40 block (0
  mismatches out of 6400 sampled texels).
* Coverage: every destination pixel is written exactly once (0 gaps, 0 overlaps), so no
  1 px seam can exist by construction; with a uniform source the eight internal
  boundaries show 0 discontinuities.
* Chroma: the loaded sheet reports `remaining_key_pixels = 0` and
  `fringe_pixels = 0` (both asserted on the real file).
* Tiling: a tiled band repeats its source period and still covers the full band.

Device drop: `ui_atlas.png` + `ui_atlas.json` + `ui_atlas_insets.json` installed in
`/data/data/org.herofolends.p0/files/`; `Assets` logs
`ui_atlas=<name> <w>x<h> elements=<n> patches=<m> file=<path>` and
`ui_frame_patches=<k>`.

## Regenerating

```
python3 analysis/agent-hud/tools/atlas_insets_author.py     # measure insets -> sidecar
python3 analysis/agent-hud/tools/atlas_ninepatch_preview.py  # visual check -> previews/
cargo test -p wc3-game --lib --offline hud::                 # acceptance
```
