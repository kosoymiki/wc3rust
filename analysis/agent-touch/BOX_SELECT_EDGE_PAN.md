# Box-select and edge-pan — physical path and verification

Round 2, ledger `r2-touch` (id 356), 2026-11-13. Owner: touch layer.

This document pins the *physical* touch path from an SDL3 finger event to the
selection/placement effects, and the edge-pan path, with `file:line` anchors. No
device run was made in this round (coordinated separately); the synthetic tests
drive the same `Game::pointer_*` API the Android app calls, so the only untested
layer is SDL's own event delivery.

## 1. One finger, from glass to selection

```text
SDL3 finger event (normalized 0..1)
  apps/android-native/src/lib.rs:876  SDL_EVENT_FINGER_DOWN  -> game.pointer_down
  apps/android-native/src/lib.rs:890  SDL_EVENT_FINGER_MOTION -> game.pointer_move
  apps/android-native/src/lib.rs:903  SDL_EVENT_FINGER_UP/CANCELED -> game.pointer_up
  (second finger: wc3-input pinch path, src/lib.rs:887/897/912)
        |
        v
Game::pointer_down                     crates/wc3-game/src/game.rs:896
  builds TouchLayout from the live HUD snapshot
    refresh_buttons()                  game.rs:510   (build + mobile reflow + TouchButton[])
    blocked_rects()                    game.rs:520   (top_bar/console, command/card, selection)
    touch_rect()                       game.rs:1846  (hud::Rect -> wc3_input::Rect)
        |
        v
TouchInput::pointer_down               crates/wc3-input/src/lib.rs:201
  route(): minimap -> hit_button -> HUD blockers -> placement -> world
    route                              wc3-input/src/lib.rs:454
    hit_button                         wc3-input/src/lib.rs:473  (exact + 10*scale band, min 108*scale, nearest centre)
        |
        v
TouchInput::pointer_move               crates/wc3-input/src/lib.rs:235
  world zone, crossing 16*scale before 0.30 s -> DragMode::Pan -> TouchMove::Pan(delta)
  world zone, crossing 16*scale after  0.30 s -> DragMode::Box  (lib.rs:266)
  Pan delta -> Game::pointer_move -> Camera::pan_pixels          game.rs:933 -> wc3-input/src/lib.rs:560
        |
        v
TouchInput::pointer_up                 crates/wc3-input/src/lib.rs:283
  DragMode::Box + moved -> TouchEnd::Box { start, end }          lib.rs:315
  DragMode::Pan        -> None (camera already moved)
  not moved            -> TouchEnd::Tap { at, double }
        |
        v
Game::pointer_up                       crates/wc3-game/src/game.rs:945
  TouchEnd::Box { start, end } -> Game::box_select(start, end)   game.rs:993
        |
        v
Game::box_select                       crates/wc3-game/src/game.rs:1163
  screen_to_world(a) and screen_to_world(b) with terrain height sampling
  min/max in world space -> select every alive player unit inside the rect
```

Semantics kept from the pre-merge game (WC3-like):

* `tap` = select one unit (double tap within 0.36 s / 36 px selects the same kind).
* `tap on ground with army selected` = move; with an enemy under the finger = attack.
* `hold ≥ 0.30 s then drag` = selection box; `quick drag` = camera pan.
* `placement mode` = the next world tap places; taps on the HUD never place.
* `minimap`: tap jumps, drag scrubs, 0.45 s hold on release issues a move order.

## 2. Edge pan while boxing

```text
Game::update(dt)                       crates/wc3-game/src/game.rs
  -> Game::drive_camera(dt)            game.rs:713
       bounds = safe-area rect (safe_insets, viewport)
       delta  = TouchInput::edge_pan(dt, bounds, layout.scale)    game.rs:720 -> wc3-input/src/lib.rs:389
       Camera::pan_pixels(delta)                                   wc3-input/src/lib.rs:560
```

`edge_pan` (wc3-input/src/lib.rs:389) only acts while the pointer zone is `World`
and the drag mode is `Box` (lib.rs:393). Inside a `30 * scale` band it returns
`-depth * 720 * scale * dt` pixels; outside it returns zero. The camera applies
the usual sub-pixel accumulation (`pan_pixels`, wc3-input/src/lib.rs:560) and the
world clamp. The box corners stay fixed at the down point; only the finger moves,
so the marquee grows while the world scrolls.

## 3. Console-layout interaction

The merged HUD is a bottom console: `layout.top_bar` is the console and
`layout.command` is the command card inside it. Consequences verified in tests:

* A tap/drag/placement tap that starts on console dead space is `Blocked`
  (`route`, wc3-input/src/lib.rs:454) — no selection, no box, no order, and
  `placing` stays armed.
* A box drag must start in the world zone; starting on the console cannot become
  a box (the pointer is not stored for `Blocked`).
* Card slots still win over the console blocker (buttons are routed before
  `blocked`), so command presses work.
* Edge pan uses the safe-area rect, not the console, so scrolling still reaches
  the screen edges behind the console only while the finger is in the world zone.

## 4. Verification (rc)

Run 2026-11-13 03:22 and re-run for round 2 (see `analysis/tech-lead/merge-touch.md`
§9): `cargo test -p wc3-game --lib game::tests:: -- --test-threads=1` → **rc 0,
18/18**, including:

| Test | What it proves |
|---|---|
| `tap_selects_moves_and_places_a_building` | tap-select above the console, ground move, arm BUILD, place outside the console |
| `console_layout_blocks_world_clicks_and_placement` | console tap/drag/placement taps are swallowed; the same gestures above build |
| `drag_box_selects_the_starting_army_after_the_hold` | hold + drag selects the base army through `box_select` |
| `box_drag_at_the_edge_auto_scrolls_the_camera` | right-edge box drag pans east through `edge_pan` |
| `quick_drag_pans_the_camera_instead_of_boxing` | short drag pans, never boxes |
| `second_finger_lift_resumes_panning_with_the_first` | one finger left after pinch keeps panning |

`cargo test -p wc3-input` → rc 0, 19/19 (`held_drag_becomes_a_box_and_edge_scrolls`
covers the router-level box + edge delta; `pinch_zooms_and_two_finger_pan_moves_the_camera`
covers the two-finger path).

Not covered here (needs a coordinated device run): SDL's finger event delivery,
the actual finger-to-pixel normalization on the phone, and visual confirmation of
the marquee rectangle.
