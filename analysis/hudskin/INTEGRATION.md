# HUD skin integration — ten regions wired into the real HUD draw (ledger 433)

Status: **wired and verified** (2026-11-15). The ten `hudskin` regions are called from
`MobileHud::draw` (`crates/wc3-game/src/mobile_hud.rs`), the atlas `Skin` is built once in
`Assets::load`, and every call site keeps the pre-atlas chrome as its fallback, so a clean
RoC install without `ui_atlas.png` renders exactly as before.

## 1. Skin construction and accessors

| item | file:line | evidence |
| --- | --- | --- |
| `Assets.hud_skin` / `Assets.hud_skin_dialog` fields | `crates/wc3-game/src/assets.rs:846` | 24-role skin + sub-slice dialog skin |
| `build_hud_skin(texture, atlas)` | `crates/wc3-game/src/assets.rs:860` | atlas elements in `hudskin::ROLES` order (the sidecar validates every rect against the manifest) |
| `dialog_sub_slice_skin(skin)` | `crates/wc3-game/src/assets.rs:892` | `list_panel→row_0`, `button_normal→part0`, `menu_button_pressed→part1`, `wide_panel→strip_top`; parent-size guarded |
| built next to the atlas registration | `crates/wc3-game/src/assets.rs:1778` | `hud_skin=roles=24 dialog_subslices=4 texture=N` debug line |
| `Assets::hud_skin()` / `Assets::hud_skin_dialog()` | `crates/wc3-game/src/assets.rs:1377`, `:1383` | `Option<&Skin>` |
| `Game::hud_skin()` / `Game::hud_skin_dialog()` | `crates/wc3-game/src/game.rs:2169`, `:2174` | delegates to `Assets` |
| view plumbing | `crates/wc3-game/src/mobile_hud.rs:503`, `:665` | `HudView::hud_skin`, `.hud_skin(Option<&Skin>)` |
| dialog plumbing | `crates/wc3-game/src/mobile_hud.rs:2956`, `:2999` | `DialogView::hud_skin` + `hud_skin_dialog` |
| layout view carries the skin | `crates/wc3-game/src/game.rs:1593` | `rebuild_layout` -> `MobileHudLayout` matches the drawn art |
| draw view carries the skin | `crates/wc3-game/src/game.rs:5596` | plus `.army(..)` / `.groups(..)` |
| dialog view carries the skins | `crates/wc3-game/src/game.rs:5608` | quest log / hero skills / building panel |

`Skin.texture` is the texture index returned by the same `textures.push(atlas.image.to_texture())`
that `Chrome::register_atlas` uses, so the region quads sample the installed sheet.

## 2. Call sites, in draw order

`MobileHud::draw` — `crates/wc3-game/src/mobile_hud.rs`:

| # | region | call site | what the caller still draws |
| --- | --- | --- | --- |
| 1 | `console::draw` | `:1889` | nothing (caps + centre plate) |
| 2 | `topbar::draw` | `:1937` | map name / FPS into the returned `(map_name, fps)` rects (`:2371`, `:2380`) |
| 3 | `minimap_frame::draw` | `:1966` | minimap texture (`:2004`), fog (`:2007`), dots (`:2065`), camera outline (`:2074`) |
| 4 | `portrait::draw` | `:3243` (`draw_selection_skin`) | portrait icon (`:3255`), name (`:3268`), class (`:3276`) |
| 5 | `stats::draw_hp` | `:3286` | `HP x/y` into the returned number rect |
| 5 | `stats::draw_mana` | `:3297` | `MANA 0/0` |
| 5 | `stats::draw_stats` | `:3322` | `ARMOR/DAMAGE/RANGE` rows into the returned text rects |
| 6 | `pills::draw_pill` | `:2158` | resource icon / value / caption into the returned icon + number/caption boxes |
| 6 | `pills::draw_upkeep` | `:2199` | upkeep string into the returned text box |
| 7 | `tiles::draw_card` | `:2235` | icon + hotkey overlay per slot (`draw_card_overlay`, `:3713`); returned slots replace `layout.card_slots` (`:2249`) |
| 8 | `army::draw_chip` | `:2297` | icon painted first into `army::chip_icon_rect`, then frame + micro HP bar |
| 8 | `army::draw_group` | `:2313` | key glyph + count into the returned text box |
| 9 | `dialogs::draw_banner` (toasts) | `:2427` | toast string |
| 10 | `tooltip::draw_request` | `:2479` | title / wrapped body / gold cost rows into `TooltipParts` |
| 11 | `dialogs::draw_panel` (victory/defeat) | `:2648` | phase title/subtitle |
| 11 | `dialogs::draw_banner` (phase band) | `:2686` | — |

`MobileHud::draw_dialog` (quest log / hero skills / building panel): `dialogs::draw_panel`
`:3064`, `dialogs::draw_banner` `:3071`, `dialogs::draw_rows` `:3073`,
`dialogs::draw_button` `:3145`.

Supporting skin variants (caller-side, region files untouched):

* `minimap_skin` `:3213` — `dialog_wide` insets cut to the lane's measured 12 px border.
* `portrait_scaled_skin` `:3196` — `portrait_frame` insets × HUD scale (`portrait.md` §6.1 option b),
  so the 176-unit budget keeps a real icon window at every zoom.
* `portrait_hp_skin` `:3362` — same treatment for the `hp_bar` trough.
* `card_tile_state` `:3672` — `CommandButton` → `tiles::TileState` (pressed > disabled > hover >
  cooldown, empty when absent).
* `tile_overlay_rects` `:3694` — icon window + hotkey badge from `tiles::WINDOW` /
  `tiles::HOTKEY_BADGE` (the second pass `draw_card` does not return them).

## 3. Fallback (no atlas)

Every branch is `if let Some(skin) = view.hud_skin { region } else { today's chrome }`.
The fallback paths are byte-identical to the pre-integration code (retail `HumanUITile01..04`
console, `Chrome::element_or` atlas names, redraw pack, flat bevels), and the fallback army
block + group row stay in `game.rs` behind `if self.hud_skin().is_none()` (`:5617`, `:5625`).

## 4. Layout conflicts fixed

### 4a. Portrait: bands beside the frame, frame inside the console

`portrait.md` §5.2 measured the full ornament at `(309.576, 723.994, 208.048, 228.012)`:
7.22 px into the minimap zone and a 58.2 px band stack that had to start above the console top.
The skin layout (`mobile_hud.rs:1465` ff.) now uses the retail TFT shape:

* portrait frame budget: `176 * scale` wide at the frame aspect (`portrait::FRAME_ASPECT`),
  anchored at `console.x + 264*s`, `console.y + 2*s` — fully inside the console
  (`mobile_hud.rs:1465-1473`);
* name / class bands beside the frame at `info_x = portrait.right() + 12*s`
  (`:1482`, `:1500-1501`), then HP `:1502`, mana `:1503`, stats `:1504`,
  production `:1505-1506`;
* info width is clamped by the command card (`min(240*s, card.x - 8*s - info_x)`), so portrait
  orientation (1080x2414) cannot run the text under the card;
* army top is `max(186*s, production.bottom + 8*s, portrait.bottom + 8*s)` (`:1521-1523`)
  and `selection` grows to it, so the chips never sit under the frame.

Result: `portrait ∩ minimap_hit == 0`, `name.x >= portrait.right()`, all skin rects inside the
console, and `MobileHudLayout::issues()` is empty at 2414x1080 and 1080x2414
(test `ui_hudskin_layout_keeps_the_portrait_column_clear`).

### 4b. Minimap hit rect includes the measured 12 px ornament border

* `MINIMAP_BORDER_PX = 12.0` (`mobile_hud.rs:43`, docs `minimap_frame.md` §3/§4);
* `MobileHudLayout::minimap` = image square dilated by 12 px on every side
  (`mobile_hud.rs:1421`); `MobileHudLayout::minimap_image` holds the texture square
  (`:1052`, `:1415`);
* the region draw returns the same frame (`parts.frame`, `:1968`) and the draw code overwrites
  `layout.minimap`/`minimap_image`/`minimap_header` with the returned slots (`:1967-1970`), so
  art and hit rect cannot drift;
* `HudLayout.minimap_image` (`hud.rs:1337`, synced `game.rs:1600`) keeps world mapping on the
  image square: `minimap_to_world` uses it at `game.rs:3751`, `:3785`, `:3856`, `:3941`, while
  the touch zone stays the expanded hit rect.

Tests: `minimap_frame_and_header_are_anchored_to_the_hit_rect` now asserts the 12 px ring is a
`HudHit::Minimap` on all four sides and the header is anchored to the image.

## 5. Sub-slices (tech-lead input, 2026-11-15)

`analysis/h2/atlas/slices.json` `sub_elements` are consumed in `dialog_sub_slice_skin`
(`assets.rs:892`) via parent-relative offsets, with a parent-size guard so another atlas keeps
its parent sheets:

| role slot | sub-slice | use |
| --- | --- | --- |
| 19 `list_panel` | `row_0` (218x30) | `dialogs::draw_rows` rows (alternating tints) |
| 20 `button_normal` | `button_normal` part0 (217x25) | `dialogs::draw_button` normal |
| 22 `menu_button_pressed` | `button_normal` part1 (217x30) | `dialogs::draw_button` pressed/hover |
| 23 `wide_panel` | `strip_top` (216x27) | banner plates (`draw_banner`: toasts, dialog title band, phase band) |

**One-line API change needed (region owner, read-only here):**
`dialogs::draw_rows(skin, batch, content, count, row_h, scale)` hardcodes `ROLE_QUEST`
for every row, so `row_header` / `row_1..row_3` cannot be selected per row. Requested change:

```rust
pub fn draw_rows_roles(
    skin: &Skin,
    batch: &mut UiBatch,
    content: Rect,
    count: usize,
    row_h: f32,
    scale: f32,
    roles: &[&str], // row_header for the first row, row_0..row_3 for the rest
) -> Vec<Rect>
```

Until then the caller passes `hud_skin_dialog` and every row uses the authored `row_0` plate
(alternating tint), which already removes the "framed plate over a plate" artifact from
`dialogs.md` §6. `draw_menu` still uses `MenuSkin`/redraw art; the sub-slice skins are ready
for the menu lane (`strip_top`/`strip_bottom`, button variants) when it is wired.

## 6. Interactions preserved

* Card taps: `draw_card` returns `tiles::grid` slots; the draw stores them in
  `layout.card_slots` (`:2246`) and the test proves they equal the layout's touch grid
  (`ui_hudskin_draw_emits_the_atlas_regions`). `apply_mobile_button_rects` builds touch buttons
  from the same skin-aware layout.
* Army picks: `layout.army_slots` is the skin layout's own slot list; chips are painted over
  them and `ARMY_TARGET_BASE` hit rects come from `rebuild_layout` with the skin installed.
* Group buttons: `GroupView.rect` is `Game::group_row_slots()` (unchanged), so art and
  `GROUP_TARGET_BASE` hit rects are the same rects; the game's fallback painter is skipped only
  when the skin drew them (`game.rs:5617`).
* Tooltips: anchor is still `view.tooltip_anchor` / `tooltip_anchor_rect()`; the region returns
  the box and text slots, the existing font path draws the strings.
* Minimap taps/drags/orders: hit zone = expanded rect, world mapping = image rect (above).

## 7. Verification (observed 2026-11-15)

| command | result |
| --- | --- |
| `scripts/h2-cargo test -p wc3-game --lib hud::` | **74 passed, 0 failed** (rc=0) |
| `scripts/h2-cargo test -p wc3-game --lib hudskin::` | **110 passed, 0 failed** (rc=0) |
| `scripts/h2-cargo test -p wc3-game --lib game::` | **46 passed, 0 failed, 2 ignored** (rc=0) |
| `scripts/h2-cargo check -p wc3-game --lib` | `Finished dev profile` (rc=0) |
| `ui_hudskin_atlas_mock_writes_png` | `HUD_SKIN_MOCK …/analysis/hudskin/hud-integration-2414x1080.png 10430423 bytes`; `HUD_SKIN_DIALOG_MOCK …/questlog-integration-2414x1080.png 10430423 bytes` |

Headless PNGs (software rasterizer of the real `UiBatch`, real `ui_atlas.png`):

* `analysis/hudskin/hud-integration-2414x1080.png` — console plate, top bar, minimap + header,
  portrait frame + name/class/HP/mana/stats beside it, pills + upkeep, 4x3 card with icons and
  hotkeys, army chip, group row, toast. 107 972 distinct RGB colours.
* `analysis/hudskin/questlog-integration-2414x1080.png` — dialog plate, banner title band,
  four sub-slice rows, sub-slice CLOSE button. 23 083 distinct RGB colours.

## 8. Risks / follow-ups

1. `dialogs::draw_rows` per-row roles (row_header + row_1..3) — one-line signature change above;
   owner is the dialogs lane (file is read-only for this integration).
2. `draw_menu` is not skin-wired yet; it keeps `MenuSkin`/redraw art. The sub-slice skins
   already carry `strip_top`/`strip_bottom` + button variants for that lane.
3. Portrait/HP skins scale their atlas insets with the HUD scale (module contract option b).
   If the portrait lane later wants fixed corners at every zoom, its `nine` call would need
   `nine_scaled` + matching `border_for`; recorded in `portrait.md` §6.3.
4. The 10.4 MB PNGs are stored (uncompressed) by the test rasterizer; they are evidence
   artifacts, not shipped assets.
