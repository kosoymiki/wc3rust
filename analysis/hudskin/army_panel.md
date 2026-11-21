# HUD skin — army panel (selected-group portrait row) + top-bar tabs

Ledger **437**. Owner file: `crates/wc3-game/src/hud/army_panel.rs` (new, 1497 lines,
14 GPU-free tests). This document is the wiring contract for the HUD integration lane
(ledger 434 owns `hud.rs` / `mobile_hud.rs` / `game.rs`; this module is read-only for them).

Status: **implemented and verified 2026-11-16** — harness `14 passed; 0 failed`,
`runtime probes` below are observed output, not estimates.

---

## 0. Operator correction (2026-11-16): the target is a phone, not a keyboard

The retail reference (`/storage/emulated/0/Download/images (4).jpeg`) shows the top bar as
`Quests (F9) | Menu (F10) | Allies | Log (F12)`. **F-keys do not exist on this device**, so
that framing is replaced:

| Rule | Value | Meaning |
| --- | --- | --- |
| Touch minimum | **48 dp** | 48 × 2.625 density = **126.0 px** on this panel (2414×1080) |
| Tab labels | `КВЕСТЫ`, `МЕНЮ`, `АЛЬЯНС`, `ЖУРНАЛ` (RU) / `QUESTS`, `MENU`, `ALLIES`, `LOG` (EN) | words, never key hints |
| F-keys | tooltip text only (`tab_tooltip` → `"КВЕСТЫ · F9"`) | a phone never presents a key as the way to open a screen |
| Dispatched action | `tab_command()` / `tab_action()` | the *same* `UiAction`/menu branch the key path uses (§3.3) |
| Army chips | paginate, never shrink below 126 px | the old shrink rule is retired (§4.2) |

`ArmyChip` (panel model): `{ icon: Option<&str>, hp: f32, mana: Option<f32>, def: &'static str }`.
The module resolves no textures and no glyphs: the caller paints the portrait into
`chip_icon()`, the labels into `tab_label_rect()`, the page counter into `page_label_rect()`.

---

## 1. What the module draws

| Part | Art (1:1 atlas role) | Notes |
| --- | --- | --- |
| panel plate | `wide_panel` (216×118) nine-patch | dark warm tint, gold hairline on the bottom edge |
| chip frame | `slot_gold` = primary, `slot_dark` = the rest | same roles the army lane already uses (`hudskin/army.rs`); portrait window = `army::chip_icon_rect` (1:1) |
| portrait well | solid quad | `ICON_WELL`, or `ICON_WELL_EMPTY` when `icon == None` |
| HP bar | solid quads | pinned inside the portrait's bottom edge, red→olive→green (`army::hp_color`) |
| mana hairline | solid quads | casters only (`mana: Some`), 55 % of the HP height, above it, inside the portrait |
| primary marker | solid quad | 1.5 px gold line on the frame's bottom edge (in addition to the gold role) |
| page button | `slot_dark` frame | only when `pages > 1`; caller draws `page_label()` inside it |
| tab band | `strip_top` (216×27 crop of `wide_panel`) | 1 quad under the tab row, authored height (29.7 px at scale 1.1) |
| tab tile | `slot_dark` (inactive) / `slot_gold` (active) | square 126×126 touch tile, 102×99 art at 1.24× (no distortion) |

`button_normal` part0/part1 (217×25 / 217×30, **8.7:1**) are *not* used for the tab tiles:
drawn into a square 126 px touch tile that plate would be stretched 5× vertically — the
measured justification for switching the tile art to the near-square `slot_*` frame. The
sub-slice machinery (`sub_slice_uv`, parent-size guard ±4 px, same as
`assets.rs::dialog_sub_slice_skin`) is still used for the `strip_top` band.

---

## 2. Wiring contract

### 2.1 One module line (owner: `hud.rs`, ledger 434)

```rust
// crates/wc3-game/src/hud.rs — resolves to src/hud/army_panel.rs
pub mod army_panel;
```

Nothing else in `hud.rs` changes. The module compiles against the real crate types
(`crate::hud::{Rect, UiBatch}`, `crate::hudskin::Skin`, `crate::game::UiAction`) — verified by
compiling the identical file against the shipped `libwc3_game` rlib (§8.1).

### 2.2 Army row call site

```rust
// mobile_hud.rs:3799 `draw_army_chips` (called from MobileHud::draw at :1973)
let units: Vec<army_panel::ArmyChip> = view.army.iter()
    .map(|chip| army_panel::ArmyChip {
        icon: /* icon_name_for_def(def).as_deref() */ None, // caller's name borrow
        hp: chip.hp,
        mana: view.army_mana(index),      // casters only
        def: /* unit def id */ "hfoo",
    })
    .collect();
let parts = army_panel::draw_page(
    skin, ui, layout.army, &units,
    /* selected: absolute index of the primary unit */ 0,
    /* page: caller state (Game::army_page) */ army_page,
    s, army_panel::TouchMetrics::DEVICE, [1.0; 4],
);
// caller paints the portraits:  parts.chips[i] → army_panel::chip_icon(skin, chip)
```

* `rect = MobileHudLayout::army` (the same rect the current `skin_army::draw_chip` loop uses,
  `mobile_hud.rs:3801-3816`).
* `icon` is the icon **name** (`assets.icon_name_for_def(def_id)`); the portrait *texture*
  stays on the caller's side (`view.army[i].icon: Option<usize>` today).
* The page index must live in `Game` next to `card.page` — it is the only new state.

### 2.3 Hit-test and sim semantics (owner: `game.rs`)

The existing army branch is `ARMY_TARGET_BASE` (`game.rs:3809-3830`): today a chip tap does
`self.selection = vec![id]`. Replace the per-slot index lookup with the panel classify:

```rust
// game.rs, inside the ARMY_TARGET_BASE branch (after army_panel is wired)
let parts = /* the parts the HUD draw stored this frame */;
match army_panel::classify(&parts, pos) {
    ArmyHit::Chip(slot) => {
        let index = parts.unit_index(slot).unwrap();        // page-relative → absolute
        let id = self.selection[index];
        if right_tap_or_long_press {
            self.drop_from_selection(index);                 // remove from the group
        } else {
            self.selection = vec![id];                       // focus one unit (as today)
            if double { self.camera.centre_on(unit.pos); }
            self.push_haptic(HapticCue::Select);
        }
    }
    ArmyHit::Page(_) => {
        self.army_page = (self.army_page + 1) % parts.pages; // wraps
        self.push_haptic(HapticCue::Press);
    }
    ArmyHit::None => {}
}
```

| Tap | Sim effect | Retail equivalent |
| --- | --- | --- |
| chip, left | `selection = vec![that unit]`, `HapticCue::Select`, double-tap centres the camera | click a portrait → focus that unit |
| chip, right / long | remove that unit from the current group; the remaining selection is kept, and the last unit is never dropped (an empty selection has no portrait row) | ctrl-click a portrait → drop it from the group |
| page button | advance one page, wraps at `pages` | Tab subgroup cycling |
| empty plate | nothing | — |

### 2.4 Tabs call site

```rust
// mobile_hud.rs: the top strip block (the `if let Some(skin) = view.hud_skin` branch, :1959)
let tabs = army_panel::draw_top_tabs(skin, ui, layout.safe_frame, s, Some(active_tab), lang, [1.0; 4]);
// the map name must start after the row (it is drawn into layout.map_name, :2061)
layout.map_name.x = tabs[3].right() + 8.0 * s;
layout.map_name.w = (layout.map_name.right() - layout.map_name.x).max(0.0);
```

Tap handling (screen `Playing`):

```rust
if let Some(tab) = army_panel::tab_hit(&tabs, pos) {
    match army_panel::tab_command(tab) {
        Some(TabCommand::QuestLog) => self.activate(UiAction::QuestLog), // == F1/F9, game.rs:3700
        Some(TabCommand::GameMenu) => { if paused { self.resume_battle() } else { self.open_game_menu() } } // == F10, game.rs:3703
        _ => {} // Allies / Journal: disabled, tap skipped (tab_enabled == false)
    }
}
```

`tab_action(TAB_QUESTS) == Some(UiAction::QuestLog)` is asserted by
`tabs_are_touch_buttons_not_key_hints`, so the tile and F9 cannot drift apart.

---

## 3. Tabs: labels, keys, actions

| # | RU | EN | Key (tooltip only) | `TabCommand` | Dispatches | Enabled |
| --- | --- | --- | --- | --- | --- | --- |
| 0 | `КВЕСТЫ` | `QUESTS` | `F9` | `QuestLog` | `UiAction::QuestLog` (`game.rs:3700`, F1/F9) | **yes** |
| 1 | `МЕНЮ` | `MENU` | `F10` | `GameMenu` | `Game::open_game_menu` / `resume_battle` (`game.rs:3485`, F10/BACK) | **yes** |
| 2 | `АЛЬЯНС` | `ALLIES` | — | `Allies` | no ally/diplomacy screen exists in the sim yet | no (drawn dim) |
| 3 | `ЖУРНАЛ` | `LOG` | `F12` | `Journal` | no log screen exists yet | no (drawn dim) |

The two disabled tabs are drawn with `TAB_DISABLED_TINT` and `tab_enabled()` is false, so the
caller skips the tap: no inert button ships. Unblocking them is one `DialogScreen` variant +
one game-side state each (owner action, §9.2).

Font evidence for the RU labels (`Font::load()`, observed): `line_height = 37.5`,
`КВЕСТЫ = 101.03`, `МЕНЮ = 78.05`, `АЛЬЯНС = 102.11`, `ЖУРНАЛ = 108.57` at scale 1.0; the
label box inside a 126 px tile is 105.84 px wide, so the widest word (`ЖУРНАЛ`) is drawn at
scale 0.975 → **36.6 px** text height, above the `MIN_TEXT_PX = 24` readable floor.

---

## 4. Geometry rules (with the arithmetic)

### 4.1 Chip and touch slot

```text
touch    = TouchMetrics::DEVICE.min_side()      = 126.0 px        (48 dp × 2.625)
visible  = min(max(56 × scale, touch), inner_h)                    art side
gap      = max(visible × 0.15, touch − visible)                    keeps hit rects disjoint
advance  = visible + gap                                           centre-to-centre
hit      = square max(visible, touch) centred on the chip art     ≥ 126 px always
```

The portrait is never stretched: a panel shorter than the target keeps the art at
`inner_h` and pads the **hit** rect to 126 px (allowed by the correction; the padding is
±(126 − 92.4)/2 = ±16.8 px vertically on the measured landscape panel).

### 4.2 Pagination (replaces the old shrink rule)

```text
k1        = floor((inner_w + gap) / advance)          slots that fit
count ≤ k1 → one page, per_page = count
count > k1 → per_page = k1 − 1 (one slot for the page button), pages = ceil(count / per_page)
count > k1 and k1 == 1 → no page button (the row cannot host both), pages = ceil(count/k1)
```

Why the old rule is retired — `chip = min(56·scale, avail/(n + 0.15·(n−1)))` on the measured
landscape panel (inner 479.05 px, scale 1.1):

| n | old chip | in dp | verdict |
| --- | --- | --- | --- |
| 1 | 61.6 px | 23.5 dp | below the 48 dp minimum |
| 5 | 61.6 px | 23.5 dp | below |
| 12 | 35.1 px | 13.4 dp | far below |

A 126 px row of 12 chips needs `12·126 + 11·19 = 1721 px`; the measured panel is 490 px wide,
so pagination is the only way to keep portrait targets: the row is split, not squeezed.

### 4.3 Tabs

```text
tab    = 126 × 126 px square tile (slot_* art, 1.24×)
gap    = min(8 × scale, (safe_w − 4·126) / 3)
band   = strip_top crop, authored 27 px × scale tall, under the row
row_w  = 4·126 + 3·gap        530.4 px landscape / 516.7 px portrait
```

The row starts at `safe_frame`'s top-left corner; `tabs_fit()` reports whether four 48 dp
tiles fit a given safe frame (both orientations: true).

---

## 5. Rects for both orientations (observed probe, 2026-11-15/16)

`MobileHud::new().layout(&HudView::new(viewport))`, `hud_metrics`-style probe:

| viewport | scale | `safe_frame` | `layout.army` |
| --- | --- | --- | --- |
| 2414×1080 | 1.1000 | `(52.8, 88.0, 2308.4, 939.2)` | `(499.28, 914.99, 490.05, 103.41)` |
| 1080×2414 | 0.5273 | `(12.66, 50.62, 1054.69, 2329.63)` | `(249.96, 2298.87, 234.92, 77.17)` |

Top band that hosts the tabs: `top_meta = (52.8, 88.0, 2308.4, 48.4)` landscape,
`(12.66, 50.62, 1054.69, 23.2)` portrait; map name `(63.8, 90.2, 1535.3, 44.0)` landscape must
shift to `x = 583.2 + 8·1.1 = 592.0` once the row is drawn.

### 5.1 Tab row

| viewport | tab 0 | tab 1 | tab 2 | tab 3 | row width |
| --- | --- | --- | --- | --- | --- |
| 2414×1080 | `(52.8, 88, 126, 126)` | `(187.6, 88, 126, 126)` | `(322.4, 88, 126, 126)` | `(457.2, 88, 126, 126)` | 530.4 |
| 1080×2414 | `(12.66, 50.62, 126, 126)` | `(142.87, 50.62, 126, 126)` | `(273.09, 50.62, 126, 126)` | `(403.31, 50.62, 126, 126)` | 516.7 |

Label box per tile: `tab.inset(8 %)` = `105.84 × 105.84` (landscape tab 0: `(62.88, 98.08)`).

### 5.2 Army row — measured panel (what the lane ships today)

| viewport | visible | advance | chip 0 art | chip 0 hit | page art | page hit |
| --- | --- | --- | --- | --- | --- | --- |
| 2414×1080 | 92.41 | 126.0 | `(504.78, 920.49, 92.41, 92.41)` | `(487.99, 903.69, 126, 126)` | `(891.42, 920.49, 92.41, 92.41)` | `(874.62, 903.69, 126, 126)` |
| 1080×2414 | 71.89 | 126.0 | `(252.60, 2301.50, 71.89, 71.89)` | `(225.55, 2274.45, 126, 126)` | `(410.35, 2301.50, 71.89, 71.89)` | `(383.30, 2274.45, 126, 126)` |

### 5.3 Army row — recommended band (one line in `rebuild_layout`, owner 434)

```rust
// landscape: from the selection column's right edge to the command card's left edge
let army = Rect::new(1040.0, 890.0, 940.0, 126.0);   // 2026-11-15: selection.right()=989, card.x=1991
// portrait: the console band above the card, full width
let army = Rect::new(12.6, 2170.0, 1054.7, 126.0);
```

| viewport | visible | advance | chip 0 art | chip 0 hit | page art | page hit |
| --- | --- | --- | --- | --- | --- | --- |
| band landscape | 115.0 | 132.25 | `(1045.5, 895.5, 115, 115)` | `(1040, 890, 126, 126)` | `(1859.5, 895.5, 115, 115)` | `(1854, 890, 126, 126)` |
| band portrait | 120.73 | 138.77 | `(15.24, 2172.64, 120.73, 120.73)` | `(12.6, 2170, 126, 126)` | `(943.94, 2172.64, 120.73, 120.73)` | `(941.3, 2170, 126, 126)` |

---

## 6. Chip table for 1 / 5 / 12 units (observed)

| panel | n | visible | advance | per_page | pages | page button |
| --- | --- | --- | --- | --- | --- | --- |
| measured landscape 490×103 | 1 | 92.41 | 126.0 | 1 | 1 | no |
| measured landscape 490×103 | 5 | 92.41 | 126.0 | 3 | 2 | yes |
| measured landscape 490×103 | 12 | 92.41 | 126.0 | 3 | 4 | yes |
| measured portrait 235×77 | 1 | 71.89 | 126.0 | 1 | 1 | no |
| measured portrait 235×77 | 5 | 71.89 | 126.0 | 1 | **5** | yes |
| measured portrait 235×77 | 12 | 71.89 | 126.0 | 1 | **12** | yes |
| band landscape 940×126 | 1 | 115.0 | 132.25 | 1 | 1 | no |
| band landscape 940×126 | 5 | 115.0 | 132.25 | 5 | 1 | no |
| band landscape 940×126 | 12 | 115.0 | 132.25 | 6 | 2 | yes |
| band portrait 1055×126 | 1 | 120.73 | 138.77 | 1 | 1 | no |
| band portrait 1055×126 | 5 | 120.73 | 138.77 | 5 | 1 | no |
| band portrait 1055×126 | 12 | 120.73 | 138.77 | 6 | 2 | yes |

**Read this table as the blocker report the correction asked for:** the *measured* portrait
army panel (235×77 px) cannot host 48 dp portraits — it gives one portrait and 12 pages for a
12-unit selection. Its arithmetic: a chip needs 126 px and the panel gives
`inner_w = 229.7`, so `floor((229.7 + 54.1) / 126) = 2` slots = 1 chip + the page button.
Widening it to the recommended 1054.7 px band gives 6 per page. Landscape measured is
usable-but-cramped (3 per page); the recommended 940 px band gives 6 per page.

---

## 7. API

```rust
// model
pub struct ArmyChip<'a> { pub icon: Option<&'a str>, pub hp: f32, pub mana: Option<f32>, pub def: &'static str }
pub struct ArmyParts {
    pub frame: Rect,
    pub chips: Vec<Rect>,        // visible art, one per unit on the page
    pub hits: Vec<Rect>,         // ≥ 126 px touch rect per chip (parallel to chips)
    pub order_buttons: Vec<Rect>,// page button art (empty on one page)
    pub order_hits: Vec<Rect>,   // ≥ 126 px touch rect
    pub page: usize, pub pages: usize, pub first: usize,
}
impl ArmyParts { len, is_empty, chip(slot), hit_rect(slot), unit_index(slot), page_label() -> "1/3" }
pub enum ArmyHit { Chip(usize), Page(usize), None }

// touch policy
pub struct TouchMetrics { pub min_side: f32 }
impl TouchMetrics { const DEVICE: Self; fn from_dp(dp, density) -> Self; fn min_side(self) -> f32 }

// geometry (pure)
pub fn layout(rect, count, page, scale, touch) -> ArmyLayout
pub fn chip_icon(skin, chip) -> Rect          // 1:1 portrait window
pub fn chip_hp(skin, chip, scale) -> Rect
pub fn chip_mana(skin, chip, scale) -> Rect   // zero rect when the chip is too small
pub fn page_label_rect(art) -> Rect
pub fn draw(skin, batch, rect, units, selected, scale, tint) -> ArmyParts          // page 0
pub fn draw_page(skin, batch, rect, units, selected, page, scale, touch, tint) -> ArmyParts

// hit testing
pub fn hit(parts, point) -> Option<usize>      // page slot
pub fn hit_order(parts, point) -> Option<usize>
pub fn classify(parts, point) -> ArmyHit

// tabs
pub fn tab_rects(safe_frame, touch, scale) -> [Rect; 4]    // ≥ 126 px tiles
pub fn tab_hit(tabs, point) -> Option<usize>
pub fn tab_label(lang, index) / tab_key_label(i) / tab_tooltip(lang, i) / tab_label_rect(tab)
pub fn tab_command(i) -> Option<TabCommand> / tab_action(i) -> Option<UiAction> / tab_enabled(i) -> bool
pub fn tabs_width(touch, scale) -> f32 / tabs_fit(safe_frame, touch, scale) -> bool
pub fn draw_top_tabs(skin, batch, safe_frame, scale, active, lang, tint) -> [Rect; 4]
```

Draw order inside a chip: portrait well → frame (`slot_*`) → HP (track, fill) → mana (track,
fill) → primary marker. The caller paints the portrait **first** into `chip_icon()`, exactly
like the current `skin_army::draw_chip` contract (`army.md` §4).

---

## 8. Verification (observed)

### 8.1 Harness (the owner's `hud.rs` is untouched)

`hud.rs` is owned by ledger 434, so the module is compiled against the **shipped crate** with a
3-line shim (`#[path = ".../hud/army_panel.rs"] pub mod army_panel;` plus
`pub use wc3_game::{game, hud, hudskin};`), `rustc --test` + the workspace `libwc3_game` rlib:

```
$ rustc --edition 2021 --test -L dependency=target/debug/deps \
      --extern wc3_game=…libwc3_game-a116931e1e06ab8a.rlib --extern glam=…rlib harness.rs
running 14 tests
test army_panel::tests::army_row_paginates_instead_of_shrinking_below_48dp ... ok
test army_panel::tests::measured_army_rect_still_yields_touch_targets_but_costs_pages ... ok
test army_panel::tests::every_interactive_rect_is_at_least_48dp_at_2414x1080 ... ok
test army_panel::tests::page_window_maps_slots_to_absolute_units ... ok
test army_panel::tests::hp_bar_stays_inside_the_chip_and_follows_hp ... ok
test army_panel::tests::mana_bar_only_for_casters_and_inside_the_icon ... ok
test army_panel::tests::primary_chip_uses_the_gold_frame_and_a_marker ... ok
test army_panel::tests::hit_test_finds_every_chip_centre_and_misses_the_gaps ... ok
test army_panel::tests::zero_units_draws_only_the_frame ... ok
test army_panel::tests::tabs_are_touch_sized_inside_the_safe_frame_and_ordered ... ok
test army_panel::tests::tab_hit_maps_each_tab_and_the_gaps ... ok
test army_panel::tests::tabs_are_touch_buttons_not_key_hints ... ok
test army_panel::tests::draw_top_tabs_emits_the_band_and_four_tiles ... ok
test army_panel::tests::tiny_panels_and_bad_scales_draw_the_frame_without_panics ... ok
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
```

What the 14 tests prove (brief → test):

| Brief / correction | Test |
| --- | --- |
| 1/5/12 units fit, never overlap, both orientations | `army_row_paginates_instead_of_shrinking_below_48dp` |
| every interactive rect ≥ 48 dp at 2414×1080 | `every_interactive_rect_is_at_least_48dp_at_2414x1080` |
| tab row inside the safe frame and ordered | `tabs_are_touch_sized_inside_the_safe_frame_and_ordered` |
| HP bar inside the chip, follows the value | `hp_bar_stays_inside_the_chip_and_follows_hp` |
| mana hairline casters only, inside the portrait | `mana_bar_only_for_casters_and_inside_the_icon` |
| primary chip visually distinct (gold role + marker) | `primary_chip_uses_the_gold_frame_and_a_marker` |
| hit-test for every chip centre, gaps dead | `hit_test_finds_every_chip_centre_and_misses_the_gaps` |
| zero units → frame only | `zero_units_draws_only_the_frame` |
| touch tabs, keys only in tooltips, one action path | `tabs_are_touch_buttons_not_key_hints` |
| `strip_top` band + `slot_*` tiles really drawn | `draw_top_tabs_emits_the_band_and_four_tiles` |
| page window maps to absolute units, clamps | `page_window_maps_slots_to_absolute_units` |
| measured panel still gets touch targets | `measured_army_rect_still_yields_touch_targets_but_costs_pages` |
| tiny panels / NaN scale never panic | `tiny_panels_and_bad_scales_draw_the_frame_without_panics` |

The 48 dp gate reads the **returned rects**, not a claim: every `hits`/`order_hits` rect and
every tab tile is asserted `min(w, h) ≥ 126 − ε`, and the RU/EN labels are measured with the
real `Font` (`≥ MIN_TEXT_PX` inside `tab_label_rect`).

---

## 9. Risks / next steps

1. **Module line** (`hud.rs`, owner 434): `pub mod army_panel;` — until then the file is not
   part of the crate build (the shim compiles the identical bytes against the same rlib).
2. **Allies / Journal are disabled tabs** (`tab_enabled == false`, drawn dim, tap skipped):
   they need a `DialogScreen` variant + game state (ally roster from the sim; a console
   `Game::log` ring). This is the only product gap the module reports; no inert button ships.
3. **Portrait army panel is too small** for 48 dp portraits: `layout.army` = 235×77 → 1 chip +
   12 pages for 12 units. Recommended one-line change in `rebuild_layout` (§5.3) gives 6 per
   page; landscape measured 490×103 gives 3 per page (usable, costly).
4. **Map name shift** (`mobile_hud.rs:2061`): once the tab row is drawn, `layout.map_name.x`
   must start at `tabs[3].right() + 8·s`, or the map name renders under the tiles.
5. **Hit rects pad outside the panel** when the panel is shorter than 126 px (±16.8 px on the
   measured landscape panel). The padding vertical only; the caller's army hit branch must
   accept taps in that padded band (it is console chrome, not another control).
6. `selected` is the **absolute** unit index; if the primary unit is on another page, no chip on
   the visible page is highlighted. The page label is the caller's cue; a "primary page" dot is
   a future polish item.

---

## 10. H2

* Ledger: the requested row `437` was not present in the monitor ledger (`ledger_not_found`),
  so the task was registered and is tracked as **ledger 436** (`wc3rust/hud-army-panel`).
* `harness2_capture` **914** — trajectory, measured layout, pagination table, gaps.
* `harness2_experience` **4570** — "phone HUD: 48 dp floor + pagination + one action path".
* Corpus rules applied: pure `layout` + thin emitter (dream `hud-skin-lane-nine-patch-region`),
  nine-patch roles via `Skin::nine`, sub-slice crops with the parent-size guard
  (`assets.rs::dialog_sub_slice_skin`), fixture skin `hudskin::tests_support::sample_skin`,
  GPU-free `UiBatch` vertex evidence.
