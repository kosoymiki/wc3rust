# Inventory (backpack) panel — `crates/wc3-game/src/hud/inventory.rs`

Ledger 436 · region: the 2×3 item grid beside the command card · date 2026-11-16 ·
`su -c 'id'` = `uid=0(root)` (KernelSU) · cargo via `scripts/h2-cargo`.

## 1. Goal and contract

The operator's shot of the retail HUD (`/storage/emulated/0/Download/images (4).jpeg`)
shows the piece we do not draw: a small framed plate captioned **Inventory**
between the unit card and the 4×3 command card, six item slots in **2 columns ×
3 rows**, each slot an ornamented stone frame with a square icon window. The sim
already carries everything that panel shows (`Unit::items` = 6 `ItemId`s,
`Sim::items` = `Item { id, def_index, pos, owner, charges }`, `Item::def()` =
shipped `ItemDef`); this module is the screen side only.

Public surface (`crate::hud::inventory`):

| item | meaning |
|---|---|
| `SLOTS = 6`, `COLUMNS = 2`, `ROWS = 3` | the grid |
| `TOUCH_MIN = 126.0` | 48 dp at the 2414×1080 device density (≈2.625×) |
| `DRAG_THRESHOLD = 12.0`, `LONG_PRESS_SECS = 0.45` | when the caller arms a drag |
| `ItemSlot { icon: Option<&str>, charges: i32, cooldown: f32 }` | per-slot display state |
| `InventoryParts { frame, header, slots[6], icons[6], charges[6], touches[6] }` | what `draw` resolves |
| `draw(skin, batch, rect, items, scale, tint) -> InventoryParts` | paints plate + header + six frames |
| `plan(rect, scale) -> InventoryParts` | geometry only (hit-test before drawing) |
| `panel_rect(x, y, cell, scale) -> Rect` | the panel whose slots are exactly `cell` |
| `touch_panel_rect(x, y, scale) -> Rect` | the 48 dp panel (`cell = TOUCH_MIN`) |
| `hit(parts, point) -> Option<usize>` | slot under a finger (tests `touches`, not frames) |
| `classify(parts, from, point, Gesture, drop_zone) -> DragAction` | tap / drag classification |
| `drag_target(parts, from, point)` / `drag_target_in(..., drop_zone)` | release classification |
| `Gesture { Tap, Drag }`, `DragAction { Use(i), Move(i), Drop }` | finger vocabulary |
| `touch_size(parts)` / `touch_meets_minimum(parts)` | the 48 dp gate for the device pass |
| `icon_for(item) -> Option<&'static str>` | icon name via the object data |
| `install_from_objects(&GameDatabase)` / `ensure_icons(...)` | publish that table once |
| `charge_label(charges) -> Option<String>` | `3` → `"3"`, `0` (unlimited) → `None` |
| `install_icons(entries)` / `icons_installed()` / `icon_basename(path)` | table plumbing |

Draw order inside the module: plate (`dialog_panel`) → header cartouche
(`wide_panel`) → six slot frames (`slot_gold` occupied / `slot_dark` empty) →
cooldown veil (`tile_dark`, top `t` of the icon window). The caller then paints
the icon into `icons[i]`, the charge count into `charges[i]` and the caption
into `header`. `draw` never emits an icon: the module has no texture index for
one, which the test proves by comparing a six-named-icon panel with six unnamed
ones (identical vertex count and identical returned rects).

## 2. Touch contract (no keyboard, finger-sized)

Phone-first, and the whole console is finger input only: this module has no key
path, no right-click path and no hover path.

| rule | value |
|---|---|
| minimum target | 48 dp = **126 px** at 2414×1080 (density ≈2.625) |
| slot frame | **126.0 px** (`TOUCH_MIN`) — every slot on the touch panel |
| finger target | **131.5 px** = slot + one seam, i.e. the pitch cell |
| touch padding | **2.75 px per side** (half of the 5.5 px seam) |
| seam dead zones | none — the seam is inside both neighbours' targets, which share an edge |
| tap | activate: use the item, or take the buy when a shop row is selected |
| drag | slot → slot = move; released on the origin slot = use |
| long-press drag | arms without travel (`LONG_PRESS_SECS`) and then drops when released off the panel |
| drop | released outside `frame` (world) or on a caller-drawn trash chip |

`hit` tests `touches`, so a fingertip on the seam between slots 0 and 1 hits one
of them rather than falling through to the console. Targets are the pitch cells:
they tile the grid and never overlap (`slots_never_overlap_and_stay_inside_the_frame`
asserts it for frames *and* targets), so a tap can never be ambiguous.

`touch_size(&parts)` is the smallest target side and `touch_meets_minimum` is the
gate (`>= 126 - 0.5`). A retail-proportioned panel (80 px slots) fails it on
purpose — the gate has teeth instead of blessing 30 dp targets.

## 3. Rects at 2414×1080 and 1080×2414 (measured)

Landscape `scale = 1.1`, slot 126 px, panel **270.70 × 426.40** at
`(1716.13, 600.80)`:

| piece | x | y | w | h |
|---|---|---|---|---|
| frame | 1716.13 | 600.80 | 270.70 | 426.40 |
| header | 1722.73 | 607.40 | 257.50 | 22.00 |
| slot 0 / 1 | 1722.73 / 1854.23 | 631.60 | 126.00 | 126.00 |
| slot 2 / 3 | 1722.73 / 1854.23 | 763.10 | 126.00 | 126.00 |
| slot 4 / 5 | 1722.73 / 1854.23 | 894.60 | 126.00 | 126.00 |
| icon (each) | slot.x + 20.16 | slot.y + 20.16 | 85.68 | 85.68 |
| charge band | icon.x | icon.bottom − 29.13 | 85.68 | 29.13 |
| touch (each) | slot.x − 2.75 | slot.y − 2.75 | 131.50 | 131.50 |

Portrait `scale = 0.5298`, slot 126 px, panel **261.01 × 412.72** at
`(794.99, 1663.55)` — floating above the console (see §4):

| piece | x | y | w | h |
|---|---|---|---|---|
| frame | 794.99 | 1663.55 | 261.01 | 412.72 |
| header | 798.17 | 1666.73 | 254.65 | 22.00 |
| slot 0 / 1 | 798.17 / 926.82 | 1689.79 | 126.00 | 126.00 |
| slot 2 / 3 | 798.17 / 926.82 | 1818.44 | 126.00 | 126.00 |
| slot 4 / 5 | 798.17 / 926.82 | 1947.09 | 126.00 | 126.00 |
| icon (each) | slot.x + 20.16 | slot.y + 20.16 | 85.68 | 85.68 |
| charge band | icon.x | icon.bottom − 29.13 | 85.68 | 29.13 |
| touch (each) | slot.x − 1.32 | slot.y − 1.32 | 128.65 | 128.65 |

Both tables are reproduced by `touch_panel_fits_the_console_and_clears_card_and_portrait`
(which prints the frame) and by the per-slot assertions in
`every_target_clears_48dp_at_2414x1080` / `six_slots_in_a_two_by_three_grid`.

Grid rule: `pitch = (available + seam) / count` is the exact inverse of
`count * pitch − seam`, so `panel_rect(x, y, cell, scale)` always yields slots of
exactly `cell` — `panel_rect_is_the_exact_inverse_of_the_grid` checks that at
scales 0.62 / 0.5298 / 1.1 / 2.0. Slot aspect is square at both orientations and
also in a deliberately wide or tall plate (`slots_are_square_in_both_orientations`).

## 4. Console fit arithmetic (live `MobileHud::layout` numbers)

Measured 2026-11-16 from `MobileHud::new().layout(&HudView::new(viewport))`
(`mobile_hud.rs::layout`), no skin:

| rect | landscape 2414×1080 | portrait 1080×2414 |
|---|---|---|
| `console` | 48.00, 596.00, **2318.00 × 435.996** | 24.00, 2085.07, **1032.00 × 264.93** |
| `card` | 1995.63, 686.02, 343.97 × 255.97 | 865.34, 2155.89, 165.66 × 123.28 |
| `selection` (portrait column) | 491.18, 624.60, 493.35 × 284.19 → right 984.53 | 260.61, 2098.84, 237.60 × 164.34 |
| `groups_area` | 1240.38, 808.50, 499.40 × 44.00 → right 1739.78 | — |

**Landscape.** Panel right edge = `card.x − 8·scale` = 1995.63 − 8.80 = 1986.83,
so `x = 1986.83 − 270.70 = 1716.13`; vertically centred in the band,
`y = 596.00 + (435.996 − 426.40)/2 = 600.80`, bottom 1027.20.

* height: **426.40 ≤ 435.996** — `3×126 (378.0) + 2 seams (11.0) + top gap
  (2.2) + header (22.0) + 2 pads (13.2) = 426.4`, **9.60 px to spare**;
* card: `panel.right 1986.83 ≤ card.x 1995.63` — exactly the 8.8 px gutter, no
  overlap;
* portrait column: `panel.x 1716.13 ≥ selection.right 984.53` — **731.6 px**
  clearance, no overlap;
* group row: the live `groups_area` right edge (1739.78) overlaps this panel by
  **23.65 px**. The lane left of the panel is
  `993.33 … 1707.33 = 714.0 px` and the row needs
  `10 × 44 + 9 × 6.6 = 499.4 px`, so the composition re-bounds
  `groups_right = inventory.x − gap` and re-centres: the plates then span
  `1100.63 … 1600.03`, 107.3 px clear of the panel. This is the only layout change
  the panel forces — the row keeps its 44 px plates.

**Portrait.** The band is **264.93 px** and three 126 px rows need **378 px**
(113.07 px short), so a 48 dp slot grid cannot live in the portrait console at
all. The panel therefore floats directly above it:
`y = 2085.07 − 4.24 − 412.72 = 1663.55`, `x = 1080 − 24 − 261.01 = 794.99`,
bottom 2076.27 (4.24 px above the console), right edge on the safe frame.

## 5. Wiring contract for `mobile_hud::draw` (ledger 434)

`mobile_hud.rs`, `game.rs` and `hud.rs` are owned by 434; the module is inert
until the declaration and the call land. The line to add in `hud.rs` next to the
other submodules:

```rust
#[path = "hud/inventory.rs"]
pub mod inventory;
```

Four edits, in this order:

1. **Declare the rect** in `MobileHud::layout`, right after `groups_area` is
   computed (so the lane can be re-bounded):

```rust
// Retail inventory block: 2x3 finger-sized slots right of the group row.
let panel = crate::hud::inventory::touch_panel_rect(0.0, 0.0, scale);
let inventory = Rect::new(
    card.x - 8.0 * scale - panel.w,
    console.y + (console.h - panel.h) * 0.5,
    panel.w,
    panel.h,
);
let groups_right = (inventory.x - 8.0 * scale).max(groups_left);   // replaces `card.x - 8.0 * scale`
```

   and store `inventory` in `MobileHudLayout` (also add `("inventory", self.inventory)`
   to `zone_rects` and the slot rects to `interactive`, so `issues()` and the
   `GAME_HUD_RECTS` device contract see it).

2. **Draw it** in `MobileHud::draw`, immediately after `draw_command_card(...)`
   inside the `Some(skin)` arm:

```rust
let parts = crate::hud::inventory::draw(
    skin, ui, layout.inventory, view.inventory_slots, s, [1.0; 4],
);
// caption
font.draw(ui, "INVENTORY", parts.header, ...);          // centred, upper case
for (index, slot) in view.inventory_slots.iter().enumerate() {
    let Some(slot) = slot else { continue };
    // Texture indices, not names: `MobileHud::draw` has no `Assets` (same reason
    // `CommandButton::icon` is an `Option<usize>`). game.rs resolves the names.
    match view.inventory_icons.get(index).copied().flatten() {
        Some(texture) => ui.textured_quad(texture, parts.icons[index], [0.0, 0.0, 1.0, 1.0], [1.0; 4]),
        // No icon in this install: grey placeholder in the same window.
        None => skin.stretch(ui, "tile_empty", parts.icons[index], [0.55, 0.55, 0.58, 0.9]),
    }
    if let Some(label) = crate::hud::inventory::charge_label(slot.charges) {
        font.draw(ui, &label, parts.charges[index], ...); // right-aligned in the band
    }
}
```

   `HudView` gains two slices — `inventory_slots: &'a [Option<ItemSlot<'a>>]`
   (what `draw` classifies) and `inventory_icons: &'a [Option<usize>]` (texture
   indices, one per slot) — with builder methods `inventory(...)` /
   `inventory_icons(...)`. `game.rs` builds both once per frame from the
   selection: `icon_for(item)` → `assets.ui.texture(name)` (or
   `load_command_icon` when the icon is not loaded yet). Keeping the name and the
   texture index side by side means `ItemSlot::icon` stays available for tooltips
   without a second lookup.

3. **Hit hook** in `MobileHudLayout::hit`, before the `card_slots` loop (the two
   rects are adjacent, never overlapping):

```rust
if self.inventory.contains(point) {
    if let Some(parts) = self.inventory_parts {
        if let Some(slot) = crate::hud::inventory::hit(&parts, point) {
            return Some(HudHit::Inventory(slot));
        }
    }
}
```

4. **Gesture routing** in `game.rs` touch handling (no keyboard anywhere):

```rust
// press: remember the slot the finger started on, if it started on one
let from = layout.inventory_parts.and_then(|p| inventory::hit(&p, press_point));
// moved: arm after DRAG_THRESHOLD px or LONG_PRESS_SECS
// release:
let gesture = if armed { Gesture::Drag } else { Gesture::Tap };
match inventory::classify(&parts, from, release, gesture, trash_rect) { ... }
```

### Which sim call a finger action maps to

| finger action | classify | sim call (`Sim`) | error strings the HUD shows as a toast |
|---|---|---|---|
| **tap** a slot with an item | `Use(i)` | `use_item(unit, item)` (`lib.rs:3788`) — or `use_item_on(unit, item, Some(target), Some(point))` (`lib.rs:3794`) when a target/point is armed; a successful use spends one charge and removes the item at 0 (`lib.rs:3820-3834`) | `"item not carried"`, `"item has no implemented effect"`, `"item ability has no active effect"` |
| **tap** an empty slot while a shop stock row is selected | `Use(i)` | `buy_item(buyer, shop, item_id)` (`lib.rs:3969`) — returns the new `ItemId`, appends it to `unit.items` and applies the bonuses | `"inventory full"`, `"not enough gold"`, `"out of stock"`, `"shop too far"`, `"item not sold here"`, `"missing shop requirement"`, `"not a shop"` |
| **drag** slot `i` → slot `j` | `Move(j)` | `sim.unit_mut(unit).items.swap(i, j)` (`lib.rs:1278`) — the sim has no `move_item`; order only reorders the backpack, bonuses were applied at pickup and `Item::pos` is untouched, so a swap is safe | — (guard `i < len && j < len`) |
| **drag** off the panel / onto the trash chip | `Drop` | `drop_item(unit, item)` (`lib.rs:3763`) | `"item not carried"`, `"item not droppable"` (quest items) |
| **long-press** then drag to the world | `Gesture::Drag` → `Drop` | same as above | same |
| drag released on the header / a seam | `Move(i)` | none — the item stays in its slot | — |

Note on `buy_item`: the sim always appends to the **first free slot**, so an
empty-slot tap is the *confirmation target* for the selected shop row, not an
insert-at-index. Making the tapped slot authoritative would need a sim change
(`buy_into_slot`), which is outside this lane (sim read-only).

## 6. Icon resolution path

The object data is the source of truth (`Assets::objects: Option<GameDatabase>`),
and it stores art as a full BLP path:

1. `install_from_objects(&GameDatabase)` (or `ensure_icons(...)` once per frame
   until it returns non-zero) walks `wc3_sim::data::ITEMS`, asks
   `GameDatabase::object(def.id)` for `ObjectInfo.icon`, reduces the path to its
   basename (`icon_basename`: `ReplaceableTextures\CommandButtons\BTNAnkh.blp` →
   `BTNAnkh`) and stores it under the item's **`ITEMS` index**, which is exactly
   the `def_index` the sim keeps on `Item`.
2. `icon_for(&Item)` looks that index up and returns `Option<&'static str>`
   (names are interned once, matched case-insensitively, so re-installing never
   leaks a second copy). Items with no `Art` in the install return `None`.
3. The caller turns the name into a texture — `assets.ui.texture(name)` for an
   already loaded icon, or `assets.ui.load_command_icon(&mut db, &mut textures, name)`
   to load it on demand (item icons are **not** preloaded by
   `assets.rs::build` — only `UnitData` icons are, `assets.rs:1995-2020`) — and
   paints it into `parts.icons[i]`. `None` → the grey `tile_empty` placeholder.

Why a table and not a live lookup: `icon_for` takes only `&Item` (the packet's
signature) and the sim's `ItemDef` (`wc3-sim/src/data.rs:684`) carries no icon
column — the art lives in the object data, which needs an install handle. The
registry is that handle, published once; `icon_for` stays allocation-free.

## 7. Tests and evidence

GPU-free (`UiBatch::default()` + `hudskin::tests_support::sample_skin`), run with:

```
scripts/h2-cargo test -p wc3-game --lib hud::inventory -- --nocapture --test-threads=1
```

`16 passed; 0 failed; 0 ignored; 367 filtered out`. Coverage of the acceptance
points:

| requirement | test |
|---|---|
| 6 slots in a 2×3 grid, square, exact side | `six_slots_in_a_two_by_three_grid` |
| no overlap, all inside the frame, icons 1:1 and inside | `slots_never_overlap_and_stay_inside_the_frame` |
| every slot **and** hit rect ≥ 126 px at 2414×1080 | `every_target_clears_48dp_at_2414x1080` (slot 126.0, target 131.5, padding 2.75; retail 80 px panel fails the gate) |
| panel fits the console, clears card and portrait column, group lane arithmetic | `touch_panel_fits_the_console_and_clears_card_and_portrait` |
| hit-test round trip for every slot centre (+ corners, seams, header) | `hit_test_round_trip_for_every_slot` |
| tap = Use, drag → other slot = Move, drag → outside = Drop, long-press drag = Drop, trash chip wins | `tap_drag_and_long_press_classify_correctly`, `drag_from_slot_zero_to_slot_five_is_a_move`, `drag_outside_the_panel_drops` |
| zero items draws the frame and no icons | `zero_items_paints_the_frame_and_no_icons` (equal vertex count named vs unnamed icons; all vertices inside the plate) |
| cooldown veil covers only the icon window, `0`/NaN = ready | `cooldown_veil_covers_only_the_icon_window` |
| slot aspect square at 2414×1080 and 1080×2414 (and wide/tall plates) | `slots_are_square_in_both_orientations` |
| `panel_rect` is the exact inverse of the grid at 5 scales | `panel_rect_is_the_exact_inverse_of_the_grid` |
| degenerate/NaN rects and scales cannot poison the batch | `degenerate_rects_do_not_poison_the_batch` |
| charge labels, icon basenames, icon table + placeholder case | `charge_label_shows_charged_items_only`, `icon_basename_strips_path_and_extension`, `icon_for_reads_the_installed_table` |

The module is verified against the current tree: `hud.rs` was patched with the
three-line declaration of §5 for the run and reverted afterwards, and the file is
byte-identical to its pre-patch state (`sha256 21941289c408c7dc59293e2a7d441d11faf0439c92afac96761ce6201dadd8b4`,
`cmp` against a copy taken before the patch). `scripts/h2-cargo check -p wc3-game
--lib` is green in the delivered state. Log:
`/data/data/com.termux/files/usr/tmp/inventory-tests-final.log`. Numbers in
§3/§4 come from a temporary diagnostic test that printed `MobileHud::layout` for
both viewports (removed after the run) and are re-asserted by the committed
tests, so the report cannot drift from the code silently.

Concurrent-state note (not this lane): while the run was being repeated, the
`--tests` profile failed in other lanes' in-flight work
(`mobile_hud.rs` tests referencing moved `TILE_SIDE`/`TILE_PITCH`, and
`tests/asset_gallery.rs` missing `MeshData.compact`). The `--lib` target these
tests live in compiled and passed; nothing in this lane touches those files.

## 8. Risks and next steps

* **Composition, not geometry:** the panel fits the band and clears the card and
  the portrait column, but it overlaps today's `groups_area` by 23.65 px. Unless
  434 re-bounds `groups_right` to `inventory.x − gap` (§4/§5.1), the group plates
  will paint under the backpack frame. The lane arithmetic proves the row still
  fits at 44 px per plate.
* **Portrait has no console room** for three 48 dp rows (264.93 px band vs
  378 px of slots); the floating rect of §4 is the recommendation. If 434 prefers
  a docked portrait panel, it must accept < 48 dp targets or a paged 2-row grid —
  both are worse; the module will report it through `touch_meets_minimum`.
* **Item icon preload** is not in place: `assets.rs` preloads only unit icons, so
  the first frame that shows an item pays one `load_command_icon` per distinct
  icon (132 item icons exist in the shipped install). A preload loop next to the
  unit-icon loop (`assets.rs:1990-2020`) is the follow-up if the stutter shows.
* **`buy_item` appends**: the empty-slot tap cannot choose the slot. Fine for the
  6-slot retail inventory (the sim fills the first free slot), but a "buy into
  this exact slot" flow needs a sim-side `buy_into_slot`.
