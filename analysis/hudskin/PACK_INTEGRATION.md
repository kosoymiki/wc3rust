# Final HUD asset pack — integration report (ledger 457)

Status: integrated into the live `MobileHud::draw` path; headless + checker
gates green. Device APK rebuilt after the texture-budget fix; see §7.

Source of truth: `analysis/hudskin/final/herofolends-hud-final/manifest.json`
(92 assets, 5 fonts, 2048×2048 commandbutton atlas). The pack is never copied
into the app: the device reads it from
`/data/data/org.herofolends.p0/files/hud_pack` (pushed from this tree), and the
UI font is read from the operator's own `War3.mpq`.

## 1. Composition (compose-C, measured)

The four console PNGs are **four framed sections**, each its own 9-slice with
the manifest insets `(32, 48, 32, 48)`. They are not one tiled band (tiling
repeats the gold frame; stretching the middle smears the basalt noise).

| section | asset | authored | insets | device rect @2414×1080 | top | height |
|---|---|---|---|---|---|---|
| 1 left cap | `console_left_cap_normal_256x288` | 256×288 | 32,48,32,48 | x 0, w 614.0 | 675 | 405 |
| 2 left mid | `console_left_mid_normal_256x288` | 256×288 | 32,48,32,48 | x 614.0, w 834.0 | 675 | 405 |
| 3 right mid | `console_right_mid_normal_256x288` | 256×288 | 32,48,32,48 | x 1448.0, w 395.0 | 675 | 405 |
| 4 right cap | `console_right_cap_normal_256x288` | 256×288 | 32,48,32,48 | x 1843.0, w 571.0 | 675 | 405 |

Widths sum to the 2414 viewport; band = `288 × s` = **405 px** at
`s = 1080/768 = 1.40625`. The pack stone base colour (average of the console
mid art) fills the band before the sections, so the top edge is one straight
line for all four sections and no transparent edge shows the world through.

Per-section content:

* section 1: minimap frame 304 + header 28, control groups 10 × 62 art (90 px
  hit rects) in a 2×5 column;
* section 2: portrait 180 + unit card 337×253 (large portrait fills the plate)
  + stats column;
* section 3: inventory 2×3, slot 149.7×90 (authored 395-section slot width,
  `button_border` insets 12 preserved);
* section 4: command card 4×3, cell `64 × s = 90`, gap `4 × s = 5.625`,
  right-aligned at the safe margin.

## 2. Element table (landscape, s = 1.40625)

| element | art | authored | insets l,t,r,b | drawn px | 9-slice | leaf |
|---|---|---|---|---|---|---|
| section 0..3 | console parts | 256×288 | 32,48,32,48 | 614/834/395/571 × 405 | yes | — |
| minimap | minimap_frame_normal_216 | 216×216 | 24,24,24,24 | 303.8×303.8 | yes | yes |
| minimap header | minimap_header_normal_216x20 | 216×20 | 24,24,24,24 | 303.8×28.1 | no (defect) | — |
| selection panel | selection_panel_normal_240x180 | 240×180 | 24,24,24,24 | 337.5×253.1 | yes | yes |
| portrait | portrait_frame_normal_128 | 128×128 | 24,24,24,24 | 180×180 | yes | — |
| card | — | — | — | 376.9×281.2 | — | — |
| card cell (×12) | button_border_normal_64 | 64×64 | 12,12,12,12 | 90×90 | yes | yes |
| group hit (×10) | — | — | — | 90×90 | — | yes |
| group art (×10) | control_group_button_inactive_44 | 44×44 | 8,8,8,8 | 61.9×61.9 | yes | — |
| army chip (×3) | button_border_empty_64 | 64×64 | 12,12,12,12 | 90×90 | yes | yes |
| inventory slot (×6) | button_border_empty_64 | 64×64 | 12,12,12,12 | 149.7×90 | yes | yes |
| topbar | topbar_plate_normal_720x32 | 720×32 | 16,0,16,0 | 2414×45 | 9-slice + stone fill | — |
| resource pill (×3) | resource_pill_normal_236x32 | 236×32 | 16,8,16,8 | 331.9×45 | no (single image) | — |
| tooltip | tooltip_panel_normal_384x256 | 384×256 | 24,24,24,24 | 540×360 | yes | — |
| toast | tooltip_panel_normal_384x256 | 384×256 | 24,24,24,24 | 540×112.5 | yes | — |
| production bar | build_progress_normal_64x8 | 64×8 | 0 | 90×11.2 | no (scale = 1.0) | — |

`hud.pack.nine-slice-insets` proves every sliced element's corner **source**
size equals its manifest inset (corners are never resampled); the drawn corner
is `inset × art_scale`. `tools/hud_pack_check.py` re-derives the same rule from
the draw trace.

## 3. Manifest inset defects (11 rows, measured)

`# manifest-defect` — the art and the manifest are not rewritten; these rows
are drawn as single images, and the loader logs exactly once per file:
`HUD_PACK insets-ignored <file> (insets l,t,r,b on WxH)`.

| file | size | insets | why |
|---|---|---|---|
| `minimap/minimap_header_normal_216x20.png` | 216×20 | 24,24,24,24 | t+b=48 ≥ 20 |
| `resources/icon_food_normal_32.png` | 32×32 | 16,8,16,8 | l+r=32 ≥ 32 |
| `resources/icon_food_warn_32.png` | 32×32 | 16,8,16,8 | l+r=32 ≥ 32 |
| `resources/icon_gold_normal_32.png` | 32×32 | 16,8,16,8 | l+r=32 ≥ 32 |
| `resources/icon_gold_warn_32.png` | 32×32 | 16,8,16,8 | l+r=32 ≥ 32 |
| `resources/icon_lumber_normal_32.png` | 32×32 | 16,8,16,8 | l+r=32 ≥ 32 |
| `resources/icon_lumber_warn_32.png` | 32×32 | 16,8,16,8 | l+r=32 ≥ 32 |
| `resources/upkeep_strip_roc_720x14.png` | 720×14 | 16,8,16,8 | t+b=16 ≥ 14 |
| `resources/upkeep_strip_tft_720x14.png` | 720×14 | 16,8,16,8 | t+b=16 ≥ 14 |
| `tooltip/tooltip_arrow_down_24x12.png` | 24×12 | 24,24,24,24 | l+r=48 ≥ 24 |
| `tooltip/tooltip_arrow_up_24x12.png` | 24×12 | 24,24,24,24 | l+r=48 ≥ 24 |

Summary (loader + checker agree):
`assets=92 sliced=29 unsliced=63 insets_ignored=11`.

## 4. Fonts — the operator's Warcraft III face

| entry | bytes | internal name | verified |
|---|---|---|---|
| `Fonts\FRIZQT__.TTF` | 143 708 | Nimrod MT | md5 `c8334ae9…` |
| `Fonts\NIM_____.ttf` | 143 708 | Nimrod MT | byte-identical (same md5) |
| `Fonts\DFHeiMd.ttf` | 2 385 036 | DFKGothic-Md | CJK (not used) |
| `Fonts\dffn_b31.ttf` | 4 445 744 | 華康細黑體 | CJK, no Cyrillic |
| `Fonts\dfst-m3u.ttf` | 6 221 576 | Song Light | CJK (not used) |
| `Fonts\tt5500m_.ttf` | 5 034 348 | SquareGothicMd BT | CJK (not used) |

Runtime source (observed on device):
`HUD_FONT primary=archive:/data/data/org.herofolends.p0/files/War3.mpq`.
The loader reads `Fonts\NIM_____.ttf` with `wc3_mpq::MpqArchive::read`, then
`FRIZQT__.TTF`; if the archive is missing it logs
`HUD_FONT fallback=pack reason=archive-missing` once and uses the pack's
`primary_serif_cyrillic.ttf`. Everything (names, tooltips, labels, numbers)
uses Nimrod; the pack sans is the third fallback face.

Coverage from the loaded face: `covered=424 ascii=95 cyrillic=94` for our
charset (full Latin-1 + Latin Extended-A + Cyrillic + `0-9 · : + / - % ×`).
Per-codepoint fallback: Nimrod → pack serif → pack sans; a codepoint missing
everywhere logs `HUD_GLYPH missing=<hex>` once.

Nimrod digit advances (`0..9`, atlas px): **10.782731 × 10** → digits **are
tabular**, so numeric columns use Nimrod; the pack mono is loaded and verified
but not selected (`digits_tabular=true`). Test lines:

```
hud.font.wc3-nimrod PASS source=extracted:.../analysis/war3_full3/Fonts/NIM_____.ttf covered=424 cyrillic=94 ascii=95 digits_tabular=true advances=[10.782731, …]
hud.font.cyrillic-coverage PASS source=extracted:.../NIM_____.ttf labels=5
hud.font.cyrillic-raster PASS ink=773 run_height=11.5px native_bitmap=18.0px min_px=24
```

`MIN_TEXT_PX = 24` is the line-box floor; the inked cap of a 30 px face is
18 px (0.6 em), which is why the raster check asserts the line box ≥ 24 and
the native glyph bitmap ≥ the dense floor.

No shaping stack: `ab_glyph` already rasterises and lays out these fixed HUD
runs; the defect was charset coverage + the texture budget (§7), not shaping.
Research verdicts: `taffy` rejected (manifest rects already exact; a flex
solver adds a dependency without solving a layout problem); `dssim` rejected
(licensing/build weight → SSIM-lite in `hud_pack_check.py`);
`bevy_ninepatch` rejected (Bevy-only); Figma/screenshot MCPs rejected (no
Figma source, desktop-only). Adopted: deterministic checker + OCR
(`vision_ocr_image` MCP + tesseract) + the `hud-pack-integration` skill.

## 5. Device review — measured defects and fixes

| # | defect (operator's device frame) | root cause | fix |
|---|---|---|---|
| 1 | section stone tops 747/785/998 | each section's art has its own inner bevel; transparent edges showed the world through the band | pack stone base fill under the band; section rects all `y=675`, `h=405` |
| 2 | topbar gap/offset | texture store capped every UI texture at 256 → the 720×32 plate was squashed | `MAX_TEXTURE_SIZE 2048`; stone fill + 9-slice at exactly `0..32*s` |
| 3 | all group labels "1" | font atlas 2048×1024 was downsampled to 256×128, so every glyph quad sampled the same texels | 2048 cap keeps the atlas 1:1; labels are `1..9,0` |
| 4 | minimap flat green/blue | real map texture is drawn via `UiTexture::Minimap`; frame/well now measured | interior std 10.5 (>10) on the current frame; fog/blips pass kept |
| 5 | empty black selection panel | panel art framed nothing | selected portrait drawn large in the plate interior |
| 6 | dark grey unit info, flat HP bar | `text_muted` on stone; bar had no border | ivory `[0.94,0.90,0.80]` + gold name; gold 1 px border + pack progress plate + green fill |
| 7 | tooltip/toast text spills | text drawn to the plate rect, not its inset | wrap to `rect − manifest insets`; toast inner width/height |
| 8 | card right column / hotkeys | font atlas defect hid the glyphs; right edge checked | hotkey OCR 8/8 (S,A,B,G,M,H,F,R); card right 2346.5 < section inner 2369 |

Card-cell cross-check: retail `0.0400` units = 72 px @1080; pack `64/768` = 90 px
= **1.25× retail linear** (shipped 95 px was 1.32×). The pack path never calls
`UiTheme`; it uses `HudPack`/`Skin::from_pack` only. The legacy redraw path's
256×288 dimension trap stays gated in `game.rs::hud_redraw_skin` (retail tiles
are 512×512/64×512).

## 6. Gates (headless)

| gate | result |
|---|---|
| `HUD_LAYOUT_GATE` (`examples/hud_layout.rs`) | PASS |
| `HUD_PACK_CHECK` (manifest/insets/sections/leaves/text/safe/pixels) | PASS |
| `HUD_TEXT_CHECK` landscape (tesseract + MCP JSON) | PASS |
| `HUD_TEXT_CHECK` portrait (tesseract) | PASS |
| `hud.pack.loads-every-asset` | PASS |
| `hud.pack.nine-slice-insets` | PASS |
| `hud.pack.card-cells-90px` | PASS |
| `hud.pack.no-legacy-stone` | PASS |
| `hud.pack.inset-guard` | PASS (`assets=92 sliced=29 unsliced=63 insets_ignored=11`) |
| `hud.font.wc3-nimrod`, `.cyrillic-coverage`, `.cyrillic-raster` | PASS |

Artifacts: `analysis/hudskin/pack-2414x1080.png` (10 430 423 B),
`pack-1080x2414.png` (10 431 757 B), `pack-layout-*.json`, `pack-run.log`,
`text-check-*.log`, raw OCR (`ocr/`).

Raw OCR (landscape): `Альянс04` / `NO UPKEEP` / `ARCHER` /
`S A B G M H F R`; portrait: the same strings.

## 7. Device APK

Build: `scripts/h2-cargo build -p herofolends-android --release` +
`scripts/build-apk.sh`. The first device frame (19:27) exposed the 256 px
texture cap: the font atlas and topbar plate were downsampled inside the mesh
texture store. Cap raised to 2048 (UI art authored ≤ 2048; model art is
unchanged) and all eight measured defects addressed. APK SHA-256 and the new
frame are in `analysis/hudskin/apk-build2.log` and
`analysis/hudskin/device-pack-2414x1080.png`.

Device log evidence:

```
HUD_PACK insets-ignored <11 files>            (once each)
HUD_FONT primary=archive:/data/data/org.herofolends.p0/files/War3.mpq … digits_tabular=true min_px=24
HUD_PACK assets=92 sliced=29 unsliced=63 insets_ignored=11
```

## 8. Risks / next

* The topbar plate's manifest insets (16) do not cover the spear ornaments at
  ~10%/90%; the operator directed 9-slice across the full width, so the centre
  strip stretches those ornaments. A future pack revision should raise the
  l/r insets (~90) or author a flat centre.
* Portrait uses the stacked four-section composition (full-width sections);
  landscape keeps the measured 614/834/395/571 split.
* The minimap texture is the generated map texture; real fog/blips depend on
  the live `HudView` data and are drawn every frame.
