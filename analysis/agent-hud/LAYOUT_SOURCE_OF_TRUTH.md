# HUD layout — one source of truth (ledger 521)

Status: implemented in `crates/wc3-game/src/mobile_hud.rs`; the render proof is
`crates/wc3-game/examples/hud_mock.rs`; PNGs in `analysis/agent-hud/`.

## The contract

`HudLayout::compute(WindowMetrics, UiPreferences, HudLayoutState) -> HudLayout`
is the only function that solves a zone rectangle. `MobileHudLayout` is its
single `dp * density` conversion (`HudLayout::to_px`), and every draw/hit path
consumes it:

```
WindowMetrics (px, density, insets, orientation)
        │
        ▼
HudLayout::compute ──► compose_landscape (wide | compact) ─┐
        │              compose_portrait ────────────────────┤
        │              compose_selection  ◄── the unit card │ (both modes)
        │              compose_bands                        │
        ▼                                                   ▼
HudLayout (dp: console, content, minimap*, selection, card, groups, topbar…)
        │
        ▼
HudLayout::to_px(density) ──► MobileHudLayout (px: the same fields × density)
        │
        ├── MobileHud::draw        (atlas / fallback path)
        ├── MobileHud::draw_pack   (final pack path)
        ├── MobileHudLayout::hit   (touch routing, uses the same rects)
        └── MobileHudLayout::issues/zone_rects (device QA)
```

Rules that follow from this and are enforced by the composer + review:

1. A draw function may only *place content inside* a rect it was given. It may
   not grow, inset-by-art, or re-derive a zone.
2. `compose_selection` is the only unit-card composer. Portrait mode places its
   summary strip (`selection`) and calls it; landscape bands do the same. The
   old portrait-only rows (fixed `16/11/27/37` offsets) are gone.
3. The minimap frame drawn is `layout.minimap_frame`. The old path re-expanded
   `minimap_image` by 12 dp per side inside `draw_minimap_frame`, so the art,
   the hit test and the content used three different squares.
4. Art insets scale *inside* the composer's rect (`nine_slice`, `menu_part_quads`),
   never against the parent zone.

## Golden numbers — measured phone, 2414×1080 px, density 3.0, insets 210/120/90/96 px

All of these are printed by `hud_mock` (and by the device QA table); the starred
ones are the acceptance numbers from the packet.

| zone | value | note |
|---|---|---|
| screen | 804.7 × 360.0 dp | 2414/3 × 1080/3 |
| safe insets | 70 / 40 / 30 / 32 dp | left / top / right / bottom |
| content | 689 × 112 dp | safe width minus 2 × 8 dp pad |
| mode | `LandscapeCompact` | 689 dp content < 820 dp |
| top bar | 30 dp | resource pills ≥ 18 dp tall |
| console | 152 dp = 42.2 % | compact base 128 dp + 24 dp gesture strip |
| selection zone | 456.7 × 112 dp | between minimap and the drawer buttons |
| **portrait** | **80 × 80 dp** ★ | `min(80, zone.h − 4)`; the old 0.55 cap gave 61.6 |
| **HP/mana trough** | **≤ 150 dp** ★ | `HP_BAR_MAX_DP`; was 383 dp before the cap |
| text rows | name 18, class 14, hp 18, mana 14, stats 14, production 14 dp | zero when the band cannot host 10 dp |
| **minimap (compact)** | **112 dp** ★ | compact ceiling; the 144 dp TZ target only fits when the band has 144 dp of content (wide/tablet/portrait-tablet) |
| minimap image | 100 dp | frame inset by a 6 dp ring |
| **card** | **224 × 176 dp** ★ | 4 × 48 + 3 × 8 + 2 × 8; a drawer in compact, anchored to the safe right edge |
| console HUD budget | ≤ 55 % of height | top bar + console; compact = 50.6 % on the device |
| **wide-console** | **0.34 × height**, clamped 176..220 dp | `LandscapeWide` (content ≥ 820 dp) |
| minimap (wide) | 144 dp | `MINIMAP_DP` |

Why 112 and not 144 on this phone: the band is `0.42 × 360 = 151 dp` plus the
32 dp gesture inset, leaving 112 dp of drawable content between the console pad
and the safe bottom. A 144 dp square there would overlap the gesture area or the
minimap header. The composer uses `min(144, band height)`, so a taller band
(tablet, portrait-tablet) gets the retail 144.

## Menu (glue) — 2414×1080

| item | value |
|---|---|
| panel | 1188 × 833 px, centred in the safe frame (210/120/90/96 px insets) |
| content | 1113 × 758 px (24 + 10 px authored inset × scale) |
| **title band** | **184 px** = `panel.w × 0.155` clamped 84..190, capped at 26 % of content height ★ |
| title / subtitle | 56 % / 30 % of the band, fitted whole-word |
| **tabs** | **6 across 1113 px → 178.2 px cells** ★; wrap to 2×3 when a cell would fall below 90 px |
| tab label | full cell width (no 30 % tag reservation) — "PROLOGUE", "NIGHT ELF" fit without an ellipsis ★ |
| rows | 114.4 px each, page capacity 2 of 6 with the full cutout insets (4 on a no-cutout 1080 frame); `MenuLayout::visible_rows` publishes the count ★ |
| nav | 4 cells of 271.7 × 114.4 px |
| locked row | dimmed plate: shipped disabled art at a grey tint, or a bevelled stone plate with a recessed veil when no art exists ★ (never a flat colour square) |

Rows below the page capacity stay in the caller's list and are reached through
PREV/NEXT; the layout never publishes an empty page (`visible_rows >= 1`).

## Divergences removed (file:line at the time of the fix)

| symptom | cause | fix |
|---|---|---|
| portrait/name/class/HP solved twice with different answers (61.6 dp portrait, rows at fixed offsets) | `compose_portrait` (`mobile_hud.rs:1993`) wrote the card itself instead of calling `compose_selection` | portrait places `selection` and delegates to `compose_selection` (`:2074`) |
| drawn minimap frame 24 dp wider than the map content and the hit test | `draw_minimap_frame` (`:7829`) re-expanded `minimap_image` | draw `layout.minimap_frame`; insets clamp inside it |
| info plate 8 px outside the selection zone | `draw_selection_fallback` used `selection.expanded(4*s)` (`:8649`) | uses `layout.selection` |
| tab labels truncated ("PROLOG", "NIGHT") | `draw_menu_entry` reserved 30 % for a tag that compact tabs never draw (`:4704`) | compact labels use the full plate; `fit_text_plate` steps to a word-safe floor |
| 14-row campaign page pushed rows out of the panel | `layout_menu` solved one `row_h` for every row (`:4272`) | page capacity + `visible_rows`; tabs wrap instead of forcing 64 px cells |
| tiny title band | `title_h = content.h × 0.14` clamped 96..150 (`:4306`) | `panel.w × 0.155` clamped 84..190 |
| three empty toast bands published with zero live toasts | `compose_bands` reserves three rows unconditionally | `hud_layout` truncates `toasts` to the live count (`toast` or `toasts`) before publishing |
| classic tooltip ignored the hovered command slot | `layout()` only copied `tooltip_anchor`, the centred rect stayed | the tooltip rect is floated above the anchor and clamped to the safe frame; the pack path keeps using the same anchor |
| single-line labels clipped mid-word at the floor | `fit_text_plate` (`hud.rs:1622`) ellipsized at `min_px` | single-line runs step to a 0.72 × floor hard floor and are measured whole; prose keeps the strict floor + ellipsis |

## Verification (render artifact, no test runner)

Per the operator's 2026-11-18 order this lane ships **no new tests**: the proof
is the compile plus the rendered mock.

```
scripts/h2-cargo check -p wc3-game --lib --offline     # rc=0
scripts/h2-cargo run -p wc3-game --example hud_mock --offline
```

`hud_mock` drives the real `MobileHud::draw` / `MobileHud::draw_menu` paths at
2414×1080 d3 and prints the composed numbers (`HUD_MOCK … portrait=… hp=…`,
`MENU_MOCK … tabs=… rows=… title_band=…`) next to the two PNGs. The layout
invariants (no overlap, no escape, bar caps, whole words, page capacity) are
enforced by the composer itself: `MobileHudLayout::issues` runs on the device QA
path and every rect in the draw path comes from the same `HudLayout` snapshot.

## Mocks

* HUD: `analysis/agent-hud/mock-console-2414x1080.png`.
* Campaign page: `analysis/agent-hud/mock-menu-campaign.png` (shipped BLP
  panel/button art, six tabs, the published chapter page, nav, locked rows).

Both come from `crates/wc3-game/examples/hud_mock.rs`
(`scripts/h2-cargo run -p wc3-game --example hud_mock`), which drives the real
`MobileHud::draw` / `draw_menu` paths and rasterises the `UiBatch` with
`hudskin::soft_raster` — a render artifact, not a test.

The mocks are procedural renders of the shipped art through the real draw path;
they are reference captures, not hand-drawn final art.
