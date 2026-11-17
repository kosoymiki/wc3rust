//! `ui.orders.targeting` — Attack (A), Move (M), Patrol (P), Hold (H) and
//! Stop (S) arm an order mode; the next world tap consumes it exactly once and
//! Escape cancels without consuming (P0-10, gap-map row `ui.hotkeys` /
//! `ui.patrol`).

use wc3_input::orders::{OrderArmer, OrderKind, order_for_hotkey};
use wc3_input::Hotkey;
use wc3_input::trace;

fn armed_after(hotkey: Hotkey) -> OrderArmer {
    let mut armer = OrderArmer::default();
    let kind = order_for_hotkey(hotkey).expect("card letter arms an order");
    armer.arm(kind);
    armer
}

#[test]
fn ui_orders_targeting_each_card_letter_arms_its_mode() {
    for (letter, id) in [
        ('a', "attack_move"),
        ('m', "move"),
        ('p', "patrol"),
        ('h', "hold"),
        ('s', "stop"),
    ] {
        let armer = armed_after(Hotkey::Letter(letter));
        assert_eq!(armer.armed().map(OrderKind::id), Some(id), "{letter}");
        // Letters outside the card arm nothing.
        assert_eq!(order_for_hotkey(Hotkey::Letter('z')), None);
        assert_eq!(order_for_hotkey(Hotkey::Digit(1)), None);
        assert_eq!(order_for_hotkey(Hotkey::Escape), None);
    }
}

#[test]
fn ui_orders_targeting_tap_consumes_once_and_escape_cancels() {
    // Exactly-once consumption: arm, take, second take is empty.
    let mut armer = armed_after(Hotkey::Letter('a'));
    assert_eq!(armer.take(), Some(OrderKind::AttackMove));
    assert_eq!(armer.take(), None);
    assert_eq!(armer.armed(), None);
    // Re-arming replaces the pending mode (only one can be armed).
    armer.arm(OrderKind::Move);
    armer.arm(OrderKind::Patrol);
    assert_eq!(armer.armed(), Some(OrderKind::Patrol));
    // Escape cancels without consuming; the next tap gets nothing.
    assert_eq!(armer.cancel(), Some(OrderKind::Patrol));
    assert_eq!(armer.cancel(), None);
    assert_eq!(armer.take(), None);
}

#[test]
fn ui_orders_targeting_trace_replays_arm_consume_and_cancel() {
    let text = "\
layout scale=1.0
key a
expect order=armed:attack_move
down 640 360 1.0
expect start=world
up 640 360 1.1
expect up=tap:640.0:360.0:single:order=attack_move
down 700 420 2.0
expect start=world
up 700 420 2.1
expect up=tap:700.0:420.0:single

key p
expect order=armed:patrol
escape
expect order=cancelled:patrol
down 760 460 3.0
up 760 460 3.1
expect up=tap:760.0:460.0:single
";
    let trace = trace::parse(text).expect("trace parses");
    let report = trace.run();
    assert!(report.ok(), "{:?}", report.failures);
}

#[test]
fn ui_orders_targeting_patrol_is_two_ground_taps() {
    let mut plan = wc3_input::orders::PatrolPlan::default();
    assert!(!plan.is_armed());
    plan.arm();
    // First tap stores the near point (the route is not issued yet).
    assert_eq!(plan.tap(glam::Vec2::new(600.0, 300.0)), None);
    assert!(plan.is_armed());
    // Second tap completes the route.
    assert_eq!(
        plan.tap(glam::Vec2::new(900.0, 500.0)),
        Some((glam::Vec2::new(600.0, 300.0), glam::Vec2::new(900.0, 500.0)))
    );
    assert!(!plan.is_armed(), "the plan is consumed once");
    // Escape between the taps cancels the pending plan.
    plan.arm();
    plan.tap(glam::Vec2::new(700.0, 320.0));
    assert!(plan.cancel());
    assert!(!plan.is_armed());
}
