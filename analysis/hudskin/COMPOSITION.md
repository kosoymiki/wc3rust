# HUD composition — operator geometry, measured

Owner: lane 434 (`crates/wc3-game/src/mobile_hud.rs`, `hud.rs`).
Reference: operator frame `Лесное королевство_ Ратуша Альянса-3.png` (1877x838)
and the device frame `Screenshot_2026-11-15-21-09-40-54_*.jpg` (2414x1080).
Ground truth for the retail proportions: `UI\FrameDef\UI\ConsoleUI.fdf`
extracted from `War3.mpq` — console `Height 0.176` units, top strip `0.032`,
`0.6 units = screen height` (`analysis/agent-re/ui_audio/ENGINE_MAP.md` §1):
console = 29.3% = 316.8 px, strip = 5.33% = 57.6 px at 1080.

## 0. Default = retail band (tech-lead, 2026-11-16, supersedes 371/106)

`CONSOLE_STYLE = ConsoleStyle::Retail` — one const pair, no layout rewrite:

| | Retail (default) | Comfort (option) |
|---|---|---|
| `console_fraction` | **0.293** (`ConsoleUI.fdf` 0.176/0.6) | 0.344 |
| band at 1080 | **316.4 px (29.3%)** | 371 px |
| `tile_px` | **95** (36 dp) | 106 |
| card 4x3 | **398 x 297** | 442 x 330 |
| group pad 5x2 | **499 x 196** | 554 x 218 |
| top strip | `TOP_STRIP_FRACTION = 0.0533` = **57.6 px at y = 0** | same |
| minimap | 252 px map in a 276 px ring (fits the band) | 300 / 324 |
| portrait | 175 x 191.8 | 175 x 191.8 |
| status | `layout.issues() == 0`, console bottom == `viewport.y` | same |

Rail arithmetic: 2 columns x 2 rows of 95 px with a 6 px gap = 196x196, centred
in the 292 px interior next to the 276 px minimap ring; 3 stacked + 1 round would
need 3x95 + 2x6 + 6 + 95 = 398 px, i.e. 81 px more than the band has — hence the
2x2 grid. Every leaf is >= 90 px: `HUD_LAYOUT_GATE PASS` at 2414x1080 and
1080x2414 (tab button 95x95 = 36.2 dp, card/group 95x95, inventory 97.5x99).

Retail structure mirrored: minimap + narrow button rail on its right, gold
portrait arch, unit info block to its right, **2x3 item grid immediately left of
the 4x3 card**, command card bottom-right, top strip = corner clusters with the
resources at the right (not a full-width plate).

## 1. Operator geometry (interim 371/106 pass, kept as `Comfort`)

Operator: «Худ очень громоздкий. Верхнюю полоску надо сделать уже и прямо на
место статус-бара. Нижний худ огромный, меньше на 15 процентов, прижать его к
основанию экрана снизу. Не остаётся места для геймплея.»

| element | value | source |
|---|---|---|
| console band | **371 px = 34.4% of H**, bottom == `viewport.y` | 436 x 0.85 |
| console interior | 371 - 2x12 pad - 8 (gesture strip) = 339 px usable | no control in the bottom 16 px |
| top strip | **58 px at y = 0**, full width | `ConsoleUI.fdf` 0.032/0.6 = 5.33% |
| tabs | **95x95 x4 in a 2x2 rail grid (196x196)** beside the minimap | retail rail; 3 stacked + round needs 398 px > the 316.4 px band |
| top strip | **read-only telemetry only** (resources/upkeep/FPS/map name/mode) | thin strip + finger targets are only compatible with no interactive leaves there |
| command tile | **106 px** (40.4 dp), gap 6 → pitch 112 | operator (retail 68 px, 82 pitch) |
| command card | 4x3 = **442 x 330** | 4x106 + 3x6 |
| group pad | 5x2 = **554 x 218**, all ten groups visible | 5x106 + 4x6 |
| minimap | **300 px** image in a 324 px ring (12 px 1:1 atlas ring) | reference terrain 234x239 square |
| portrait | **175 x 192** (`frame_medium` 198:217 at 0.88x) | 150/838 of the reference |
| army chips | **106 px**, under the portrait, paginated by `army_panel::layout` | operator |
| width budget | 324 + 8 + (175 + 240) + 8 + 554 + 8 + 442 = 1767 ≤ 2114 safe width | slack spread into the 5 gaps, never into tiles |

Portrait (1080x2414): **696 px = 28.8%** (retail 29.3%), not the old 998 px.
Rows: [minimap 276 + rail 196 + unit card + inventory] -> [army 95] ->
[groups 5x2 | card 4x3]; tight gutters 8/6 px, minimap capped at 252 so the
stack stays under 700 px. `scale` stays 1.10 (no global squeeze).

Crest (defect fix 2026-11-16 03:10): the crest is its **own** band between the
group pad and the inventory in landscape (`1507,792 224.9x260`, plaque inside),
never the group pad rect; it is dropped (zero rect) in portrait where no band
exists. `zone_rects()` therefore never contains duplicates.

Compact switch (authored, not default): `ConsoleStyle::Compact` →
`COMPACT_CONSOLE_FRACTION = 0.293` (316.8 px) and `COMPACT_TILE_PX = 95`
(297 px grid inside 316.8 px). One enum, no rewrite:
`pub const CONSOLE_STYLE: ConsoleStyle = ConsoleStyle::Operator;`

### Trade-off recorded
48 dp = 126 px was measured on 2026-11-15 (density 2.625). At 126 px the row
needs 2005 px and the console 60%+ of the height, leaving no playfield — the
operator rejected that build. Tabs therefore drop to 58 px (27 dp) **on
purpose**, and the command/group tiles to 106 px (40.4 dp). Nothing is
allowed below 100 px (`issues()` enforces it).

## 2. Colour (device frame 02:19, sRGB 0-255)

| surface | before | target | now |
|---|---|---|---|
| console stone | (99, 88, 78) sampled, (90,86,71) mean | (58, 52, 47) | `STONE_FIELD` 0.227/0.204/0.184 + grain + 8% middle vignette |
| card plate behind the grid | black voids | (38, 39, 40) | `PLATE_FILL` 0.150/0.153/0.157 + 1 px top light |
| top strip | same grey as the console | (36, 30, 26) | `BANNER_FIELD` 0.141/0.118/0.102 + 1 px gold rail |
| bevel | 2 px wide light band | 1 px | `draw_bevel_ring` |

## 3. Art scale rule

No atlas piece is drawn above **1.15x** its authored pixels; big areas are flat
fills, never a stretched plate. `atlas_pieces_inside_the_console_never_exceed_115_percent`
walks every atlas quad inside the console/strip in the rendered batch and fails
above the ceiling (it reported `worst=1.100x` before the 1:1 pass, `1.000x`
after: slot frames are drawn at 102x99 art centred in the 106 px cell, the
outer margin is hit area).

## 4. Empty slots and text

* Empty command/group/tab/inventory slots: nine-patch frame at 1:1 + the inner
  rect filled with `PLATE_FILL` + a 1 px top light — never a black void.
* Unit card: the name is fitted with an ellipsis (`Font::draw_fit`, floor 24 px);
  the class line is de-duplicated (`BUILDING · BUILDING` → `BUILDING`).
* Hotkey glyph: top-left badge at >= 24 px (`draw_card_overlay`), icon 1:1 in
  the measured window, cost/charges bottom-right.

## 5. Evidence / mock parity

`composition_mock_writes_pngs` writes `analysis/hudskin/composition-2414x1080.png`
and `composition-1080x2414.png` from the same `MobileHud::draw` the game calls.
The chrome's 1x1 flat texture must be created in the **same** texture vector the
rasterizer samples; a clone loses the flat index and every fill is skipped
(bare terrain). Fixed in both mock tests (`let mut fixture = fixture; Chrome::new(&mut fixture.textures)`).

## 6. Wiring hooks (outside this lane's OWNER_PATHS)

1. **Group hit rects** — done in `game.rs::group_row_slots`: returns
   `mobile_layout.group_slots` (ten 106 px plates) when present.
2. **Inventory input** — `layout.inventory_parts` (an `inventory::InventoryParts`)
   is published by `MobileHud::layout`. Three lines in the input layer:

   ```rust
   if let Some(action) = inventory::drag_target(&parts, from, point) { /* Use/Move */ }
   else if let Some(slot) = inventory::hit(&parts, point) { /* begin drag on slot */ }
   // Use(slot)  -> Sim::use_item / Sim::buy_item when a shop row is selected
   // Move(slot) -> Sim::items swap (Unit::items[from] <-> items[slot])
   // Drag out   -> Sim::drop_item(from)
   ```
3. **Card submenu (`MORE`)** — WC3 keeps 12 buttons per bar and pages the rest.
   `draw_command_card` renders slot 11 as `MORE` with a `1/2` label when
   `view.commands.len() > 12`; the page state belongs to `game.rs`
   (`card_button_indices`), one `card_page: u8` field.
4. **Minimap overlays** — `wc3-render/src/ui.rs:draw` paints the minimap texture
   *after* the white and font lists, so fog/blips/camera quads are hidden.
   Fix: draw the minimap run before `white_count` (one ordering change), or add a
   post-minimap overlay list to `UiBatch`. Reported, not edited (not my path).

## 7. Measured acceptance (2414x1080, safe 210/120/90/96)

Retail default (shipped):

```
scale=1.100 console=316.4 px (0.293 of H) bottom=1080 (flush)
strip=57.6 px at y=0  tabs 4x  minimap=252.4 in 276.4 ring
portrait=175.0x191.8  tile=95.00 pitch=101.00
card=398.0x297.0  groups=499x196 (10 plates)  army=95.0  inventory beside the card
layout.issues() == 0   (both orientations)
```

Comfort option (`ConsoleStyle::Comfort`): console 371 px (0.344), tile 106 /
pitch 112, card 442x330, groups 554x218, minimap 300 in 324.

Device-frame pixel means after the colour pass (mock 2414x1080):
console band (64.6, 58.1, 41.8) mean, stone field (58.7, 49.4, 39.3)
≈ the (58,52,47) target, top strip (47.9, 40.8, 31.9) darker than the console
with the 1 px gold rail. Card-plate interior = `PLATE_FILL` 0.150/0.153/0.157
(38,39,40), so the empty slots are inset wells, not black voids.

Tests: `mobile_hud::` 36/36, `hudskin::` 111/111, `hud::inventory` 16/16.
