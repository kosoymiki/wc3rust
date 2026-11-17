# Command tiles — `crates/wc3-game/src/hudskin/tiles.rs`

Lane 3/10 (ledger 425) · region: the 4×3 command card · date 2026-11-15 ·
`su -c 'id'` = `uid=0(root)` (KernelSU) · cargo via `scripts/h2-cargo`.

## 1. Goal and contract

Draw a WC3 command tile out of the fantasy atlas and hand the caller the two
boxes it must fill (icon + hotkey glyph), plus lay out the whole 4×3 card at the
retail pitch in either orientation.

Public surface (all in `hudskin::tiles`):

| item | meaning |
|---|---|
| `TileState { Normal, Hover, Pressed, Disabled, Cooldown(f32), Empty }` | per-slot draw state |
| `TileState::role()` | atlas role the state paints |
| `draw_tile(skin, batch, rect, state, scale, tint) -> TileRects` | frame + cooldown sweep; returns `hotkey` and `icon` rects |
| `draw_card(skin, batch, card_rect, state_for, scale) -> [Rect; 12]` | 4×3 layout, paints every slot, returns the slot rects (row-major) |
| `grid(card_rect) -> [Rect; 12]` | geometry only (hit-testing without a batch) |
| `card_rect(x, y, reference_height)` / `reference_height(viewport)` | the retail plate (`0.1745 × 0.129` of the height) |
| `draw(skin, batch, rect, tint)` | normal-state frame (region default kept for existing callers) |

Draw order: `draw_tile` paints the state frame (and the cooldown sweep); the
caller then paints the ability icon into `TileRects::icon` and the hotkey letter
into `TileRects::hotkey`. The icon rect is the art's own window (§3), so a later
icon never covers the gold ring.

## 2. Retail numbers (r2, observed)

Source: `analysis/agent-re/ui_audio/ENGINE_MAP.md` §1.1–§1.3 (TFT
`Game.dll` sha256 `d1a80c89597e08e0`, base `0x6f000000`, 2003-05-19 build).

* `CCommandBar` ctor `0x6f12ae80`: `[+0x59] = 0xc` (12 buttons), a 4×3
  `PlaceButton(col,row,frame,1)` loop, size setters `0x6f013930` (width) /
  `0x6f013950` (height) with
  `func_0x6f013930((0.0075 + 0.038) * 4.0 - 0.0075)` = `0.1745` and
  `func_0x6f013950((0.0075 + 0.038) * 3.0 - 0.0075)` = `0.129`.
* Constants in the image: `0x6f69ac20 = 0.0075` (gap), `0x6f699c40 = 0.038`
  (button), `0x6f692cbc = 4.0`, `0x6f69565c = 3.0`.
* `CCommandButton` ctor `0x6f12c2b0`: exactly **one** text frame (alloc `0xc4`,
  site `0x20f`), `MasterFont`, texture companion `CommandButtonNumberOverlay`,
  string source `CommandButtonNumber`, anchored to the tile's corner `4` with a
  `-0.000061` x nudge (`0x6f12c3d3`). No name, no cost — those are tooltip
  payload (`ToolTipName`/`ToolTipDesc`/`ToolTipCost`).

Implemented constants: `TILE_H = 0.038`, `GAP_H = 0.0075`,
`PITCH_H = 0.0455` (= size + gap), `CARD_W_H = 0.1745`, `CARD_H_H = 0.129`, all
fractions of the reference height (`reference_height(viewport) = viewport.y`,
the retail unit space where `0.6` units = screen height).

`grid()` derives the pitch from the card rect it is handed
(`pitch = card.h / (3 − gap/pitch)`, `gap = pitch · 0.0075/0.0455`, the narrower
axis wins, the grid is centred), so the retail plate and a plate measured on
screen give the same pitch, tiles stay square, and the grid never leaves the
card.

## 3. Icon window (measured)

The tile art is a gold ring with a flat window; the icon must land in that
window. Measured on the shipped element art
(`analysis/h2/atlas/elements/`) by scanning the mid row/column of each tile and
locating where the gold band and its inner shadow end:

| element | size | mid-row transition | mid-col transition | window insets (px / fraction) |
|---|---|---|---|---|
| `10_tile_normal_89x90.png` | 89×90 | gold `x 3..10`, shadow `11..12`, plate from `13`; plate to `75`, gold `78..85` | gold `y 2..6`, shadow `7..12`, plate from `13`; plate to `72`, gold `77..85` | `13/89 = 0.146`, `13/90 = 0.144`, `13/89 = 0.146`, `17/90 = 0.189` |
| `15_tile_empty_91x91.png` | 91×91 | dark hole `x 13..78` (row scan rounded to `x 9..81` where the shadow is counted as plate) | hole `y 10..77` | `≈0.14 / 0.11 / 0.13 / 0.14` |

`tile_hover`, `tile_pressed`, `tile_gold`, `tile_dark` share the same silhouette
(same ring, different face lighting — verified by reading the element PNGs), so
one conservative table serves all six states and the icon does not move when the
state changes:

```rust
pub const WINDOW: [f32; 4] = [0.146, 0.144, 0.146, 0.189];
```

`scale` multiplies that border (`1.0` = the art's own proportion) **and** is
passed to `Skin::nine_scaled`, so the frame ornament (12 px bevel at `1.0`)
grows together with the window border and the icon stays in the ring at every
zoom. The two per-axis insets are clamped so the icon always keeps at least
`MIN_ICON_SHARE = 10 %` of the tile side, which makes `icon` strictly inside the
frame for any tile size and any `scale`.

The hotkey badge is the top-left corner square, `HOTKEY_BADGE = 0.42` of the
tile (the engine's single text frame is anchored to corner `4`); the badge is
returned, not drawn, so the caller can pick the glyph and the font size.

## 4. Cooldown sweep

`Cooldown(t)` paints `tile_normal` and then `skin.stretch("tile_dark", ...)` over
the **top `t` of the tile** (`0.0..=1.0`, clamped), tinted
`COOLDOWN_DARKEN = [0.25, 0.25, 0.30, 0.85] × tint`.

The sweep direction is a **choice, not a proven engine fact**: the map proves the
`CommandButtonCooldown` texture frame exists (`0x6f12c3f0`, lines `0x218/0x21c`)
but not the order in which it is shrunk, so this file anchors the remaining-time
fraction to the top edge and keeps the sweep anchored/parameterised in one
place. `t <= 0` draws nothing; `t >= 1` covers the tile.

## 5. Evidence

`scripts/h2-cargo test -p wc3-game --lib hudskin::tiles -- --nocapture` →
`8 passed; 0 failed` (`343 filtered out`), 2026-11-15.

Printed geometry (`--nocapture`):

```
viewport [2414.0, 1080.0]: card 188.46x139.32 pitch 49.140 gap 8.100 tile 41.040
viewport [1080.0, 2414.0]: card 421.24x311.41 pitch 109.837 gap 18.105 tile 91.732
```

Both match the contract exactly: `2414×1080` → `pitch = 0.0455·1080 = 49.140`,
`tile = 0.038·1080 = 41.040`, `card = 0.1745·1080 × 0.129·1080 = 188.46 × 139.32`;
`1080×2414` → `pitch = 0.0455·2414 = 109.837`, `tile = 0.038·2414 = 91.732`,
`card = 421.24 × 311.41`. Slot origins (card placed at `(120, 700)`):

| viewport | slot row/col origins (x) | slot origins (y) | last slot far edge |
|---|---|---|---|
| 2414×1080 | `120.000, 169.140, 218.280, 267.420` | `700.000, 749.140, 798.280` | `308.460 × 839.320` = card far edge |
| 1080×2414 | `120.000, 229.837, 339.674, 449.511` | `700.000, 809.837, 919.674` | `541.243 × 1011.406` = card far edge |

Tests (all in `tiles.rs`, no GPU):

1. `grid_is_twelve_slots_on_the_retail_pitch` — 12 slots, `pitch = 0.0455H`,
   `tile = 0.038H`, square tiles, row/column pitch, grid exactly fills the card
   and never leaves it, both viewports.
2. `every_quad_stays_inside_its_tile` — all 6 states × 12 slots × 2 viewports:
   every vertex inside its tile, UVs in `0..=1`, icon/badge inside the tile.
3. `icon_and_hotkey_are_strictly_inside_the_frame` — icon strictly inside at
   `scale ∈ {0.5, 1, 2, 50}` for every state, ≥ 10 % share, badge top-left
   anchored at 0.42, and at `scale = 1` the icon is exactly `WINDOW`.
4. `nine_patch_corners_keep_their_pixels_at_2x` — the top-left corner quad keeps
   identical vertices and UVs at 68 px and 136 px (12 px measured corner), with a
   positive control that the far edge moved by exactly the tile delta; at the same
   68 px tile `scale = 2` doubles the corner to 24 px at identical UV texels
   (the `Skin::nine_scaled` wiring).
5. `cooldown_sweep_height_is_proportional_to_t` — sweep height `= t·h` for
   `0.25/0.5/0.75/1`, `0.75/0.25 = 3`, one quad exactly when `t > 0`, top edge and
   full width, clamping at `t > 1` and `t < 0`.
6. `state_picks_its_atlas_role` — Normal/Hover/Pressed/Disabled/Empty/Cooldown map
   to `tile_normal/hover/pressed/dark/empty/normal`, verified by the emitted UV
   against `Skin::rects`.
7. `card_draws_every_slot_and_reports_its_rects` — 12 nine-patched tiles
   (648 vertices), `state_for(5) = Cooldown(0.5)` adds exactly one sweep quad,
   returned rects equal `grid(card)`, zero-area slot cannot panic.
8. `region_draws_inside_its_rect` — the pre-existing region invariant.

## 6. Risks / notes

* **H fraction vs the retail unit space.** This lane's contract fixes the tile
  at `0.038·H`; the engine numbers are in a unit space where `0.6` units = the
  screen height, so retail pixels at 1080p are `0.038/0.6·1080 ≈ 68.4 px` on an
  `81.9 px` pitch (`ENGINE_MAP` §1.3). `card_rect`/`grid` take the card rect or
  the reference height explicitly, so a caller that wants retail pixels passes
  `H/0.6` (or a measured plate) and gets the retail pitch from the same code —
  no constant changes. `observed`: both paths are exercised by the tests through
  the card rect.
* **Nine-patch at tile sizes (`observed`, benign).** The seam-energy insets
  shipped for the tile roles are thick (`tile_normal [17,38,20,38]` on an 89×90
  art), so `Skin::nine`'s documented fallback (`rect.h <= t + b + 1`) turns a
  retail-height tile into one uniformly stretched quad. At `0.76×` that is the
  better degradation (the ring keeps its proportions instead of growing to 38 px
  corners), and the callers' quads still stay inside the tile — covered by test
  2. If the tile family ever needs true nine-slicing, the atlas insets for
  `tile_*` must be re-cut to the measured window border (`§3`), which is atlas
  data, not draw code.
* **Cooldown direction** is a stated choice (§4), not a decompiled fact.
* **`scale` semantics.** `scale` is the HUD art scale: it is handed to
  `Skin::nine_scaled` (frame ornament grows with zoom) and multiplies the
  measured window border, so both grow together and the icon keeps the art's
  ring. The 10 % floor keeps the icon valid at any value. Before `nine_scaled`
  landed in `hudskin/mod.rs` this module used `Skin::nine` (fixed insets), which
  shrank the border relative to the tile at zoom — the wiring lane's change is
  now used here.
