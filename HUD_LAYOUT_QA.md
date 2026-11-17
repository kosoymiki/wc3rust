# HUD_LAYOUT_QA — mobile relayout (TZ `HEROFOLENDS_HUD_MOBILE_LAYOUT_ENGINEERING_TZ.md`)

Status: **layout engine complete and gated; device screenshots pending** (the
current device build is being replaced by the one that fixes the Android
acquire freeze — see §7).

## 1. Device metrics (observed)

```
wm size      Physical 1240x2772   Override 1080x2414
wm density   Physical 560 dpi     Override 480 dpi   => density_scale = 3.0
display      "Встроенный экран", 1240x2772 @560 dpi, cutout top 138 px,
             rounded corners 60 px
landscape    2414x1080 px = 804.67 x 360.0 dp
portrait     1080x2414 px = 360.0 x 804.7 dp
```

`density_scale` is taken from the platform window metrics; the layout never
guesses it from the screenshot.

## 2. Root-zone arithmetic (device, landscape)

| item | dp | px | share of height |
| --- | --- | --- | --- |
| screen | 360.0 | 1080 | 100 % |
| top bar | 30.0 | 90 | 8.3 % |
| console (compact band) | 128.0 | 384 | 35.6 % |
| **world viewport** | **202.0** | **606** | **56.1 %** |
| HUD total (top bar + console) | 158.0 | 474 | 43.9 % |

The TZ §5.2 formula alone (`clamp(0.34*H, 176, 220)`) would give 176 dp =
528 px = 48.9 % of the height on this device and 57.6 % on the operator's
2048x916 frame. That is why `LandscapeCompact` keeps only the minimap + the
selection strip in the band and moves the 4x3 command grid into a 48 dp-target
overlay drawer (`COMMANDS`), with `GROUPS` opening the 3x3 group drawer. Action
IDs, row-major order, hotkey labels and gestures are unchanged.

## 3. Viewport matrix (`HUD_LAYOUT_GATE`, 2026-11-17)

```
viewport                     dens         dp           mode    console     HUD %  targets  gate
device-2414x1080             3.00  804.7x360 landscape-compact    128.0dp     43.9%     48.0  ok
user-2048x916                3.00  682.7x305 landscape-compact    128.0dp     51.7%     48.0  ok
phone-2340x1080              3.00  780.0x360 landscape-compact    128.0dp     43.9%     48.0  ok
phone-2400x1080              3.00  800.0x360 landscape-compact    128.0dp     43.9%     48.0  ok
reference-1920x1080          3.00  640.0x360 landscape-compact    128.0dp     43.9%     48.0  ok
small-1280x720               3.00  426.7x240 landscape-compact    100.8dp     54.5%     48.0  ok
portrait-1080x2340           3.00  360.0x780       portrait    248.0dp     37.4%     48.0  ok
narrow-portrait-320dp        3.00  320.0x780       portrait    248.0dp     37.4%     48.0  ok
tablet-2560x1600             2.00 1280.0x800 landscape-wide    220.0dp     31.2%     48.0  ok
HUD_LAYOUT_GATE PASS
```

Known limitations, stated rather than hidden:

- `user-2048x916`: HUD 51.7 % (console 41.9 %, top bar 9.8 %). The console is
  already in compact mode; the remaining cost is the 30 dp top bar plus the
  128 dp band. A further reduction would need the minimap to move into a drawer,
  which costs a tap on every glance at the map.
- `small-1280x720`: HUD 54.5 % at a 240 dp-tall window. Same trade-off; the
  window is below the phone target.

## 4. Invariants (tests, `crates/wc3-game/tests/hud_layout.rs`)

`hud_layout_console_edge_to_edge`, `hud_layout_viewport_ends_at_console`,
`hud_layout_targets_48dp`, `hud_layout_no_ambiguous_overlap`,
`hud_layout_command_4x3_order`, `hud_layout_groups_nine`,
`hud_layout_minimap_square`, `hud_layout_selection_states`,
`hud_layout_multiselect_collapses`, `hud_layout_portrait_mode`,
`hud_layout_single_px_conversion`, `hud_layout_relayout_on_metric_change`,
`hud_layout_compact_at_device_width`, `hud_layout_matches_legacy_zone_names`,
`hud_layout_zero_console_gap`.

Result on 2026-11-17: **15 passed, 0 failed**.

## 5. Composition changes (TZ §6)

- One console background (`x=0`, `right=W`, `bottom=H`); sections separated by
  recessed seams, no per-section gold frames.
- Minimap frame is a border around the map content, 1:1 content.
- Selection panel: composed empty state (`Нет выделения` + hint), no
  `NO UNIT / TAP A UNIT` plaque, no fake portrait frame; HP and mana are two
  bars with current/max text; production only when it exists.
- Multi-selection strip appears only when the selection count > 1 and collapses
  back.
- Command card is exactly 4x3, row-major, 48 dp targets, 8 dp gaps; states
  normal / disabled / cooldown (+number) / autocast / active / build / cancel /
  empty; hotkey glyph in a fixed corner.
- Top bar: full width, map identity on the left, resources right, FPS hidden
  unless the developer overlay is on.

## 6. Touch routing

`HudLayout` is the single source for draw rects, clip rects, text boxes, minimap
transform and touch targets (`to_px` converts dp → px once). `GAME_HUD_RECTS`
publishes the same rects for the device harness. `TAP_SLOP` is expressed in dp.

## 7. Device verification (pending)

The build installed on 2026-11-17 11:20 aborted before the first frame
(`wgpu` camera uniform 112 B vs shader 176 B), so no device frame existed to
measure. The fix is committed (`45b5485`), and the current build additionally
caps the Android `vkAcquireNextImageKHR` wait at 2 s (vendored
`third_party/wgpu-hal`, see its README) because a detached buffer queue used to
freeze the render thread forever.

Screenshots still to capture on device, per TZ §12: no selection, one worker,
building with production queue, hero, 6 and 12+ selection, full 12-cell command
card, cooldown/disabled, `GROUPS` drawer, `COMMANDS` drawer, tooltip; plus the
hit-test pass (tap each cell, assert the action fires).

## 8. Provenance

- Layout: `crates/wc3-game/src/mobile_hud.rs` (`HudLayout`, `WindowMetrics`,
  `HudLayoutMode`), `crates/wc3-game/examples/hud_layout.rs` (gate),
  `crates/wc3-game/tests/hud_layout.rs`.
- Commits: `c372783` (layout), `45b5485` (lighting), `81281d1` (day/night hook),
  plus the acquire-timeout patch commit.
- No `.git` existed before 2026-11-17; the repository is private
  `github.com/kosoymiki/wc3rust`.
