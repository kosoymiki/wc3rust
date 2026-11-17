//! `ui.minimap.orders` — minimap press intents: short tap jumps the camera,
//! long-press issues an attack-move and the mouse right button issues a move
//! order for the selection (P0-10, gap-map row `ui.minimap`).

use wc3_input::orders::{MinimapAction, minimap_action};
use wc3_input::trace;

#[test]
fn ui_minimap_orders_tap_hold_and_right_click_map_to_intents() {
    // Short tap: camera jump (WC3 left-click).
    assert_eq!(minimap_action(false, false), MinimapAction::Camera);
    // Long-press: attack-move for the selection.
    assert_eq!(minimap_action(true, false), MinimapAction::AttackMove);
    // Right button: move order (desktop parity), even with a hold.
    assert_eq!(minimap_action(false, true), MinimapAction::Move);
    assert_eq!(minimap_action(true, true), MinimapAction::Move);
}

#[test]
fn ui_minimap_orders_trace_replays_camera_attack_move_and_move() {
    let text = "\
layout scale=1.0 minimap=234,840,216,216
blocked 210,792,2114,288
down 300 900 0.0
expect start=minimap
up 300 900 0.1
expect up=minimap:300.0:900.0:hold:false:action:camera
down 300 900 1.0
expect start=minimap
up 300 900 1.6
expect up=minimap:300.0:900.0:hold:true:action:attack_move
right on
expect right=on
down 300 900 2.0
expect start=minimap
up 300 900 2.1
expect up=minimap:300.0:900.0:hold:false:action:move
right off
expect right=off
";
    let trace = trace::parse(text).expect("trace parses");
    let report = trace.run();
    assert!(report.ok(), "{:?}", report.failures);

    // The tapped minimap point maps to world coordinates (the game's camera
    // jump path) — the same helper the trace exercises.
    let world = wc3_input::minimap_to_world(
        glam::Vec2::new(300.0, 900.0),
        wc3_input::Rect::new(234.0, 840.0, 216.0, 216.0),
        glam::Vec2::new(4096.0, 4096.0),
    );
    assert!(world.x > 0.0 && world.y > 0.0, "{world:?}");
}

#[test]
fn ui_minimap_orders_two_finger_tap_is_the_right_click() {
    // A pinch that never pans/zooms is a two-finger tap: the touch right-click
    // that issues the minimap move order.
    let mut input = wc3_input::TouchInput::new();
    input.pinch_begin(200.0, glam::Vec2::new(300.0, 900.0), 1500.0);
    assert_eq!(
        input.pinch_end(),
        Some(glam::Vec2::new(300.0, 900.0)),
        "stationary two-finger tap"
    );
    // A real pan is not a tap.
    let mut input = wc3_input::TouchInput::new();
    input.pinch_begin(200.0, glam::Vec2::new(300.0, 900.0), 1500.0);
    input.pinch_update(200.0, glam::Vec2::new(360.0, 920.0));
    assert_eq!(input.pinch_end(), None, "panned pinch is not a right-click");
}
