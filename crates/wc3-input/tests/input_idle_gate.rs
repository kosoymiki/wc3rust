//! Mechanic `input.idle-gate` — the host must not poll for an active finger.

use wc3_input::gate::InputGate;

#[test]
fn input_idle_gate_one_finger_no_tick() {
    let mut gate = InputGate::new();
    assert!(!gate.needs_tick(0), "empty gate must not tick");
    gate.finger_down();
    assert_eq!(gate.fingers(), 1);
    // A finger on the glass is not a wake-up reason: the touch stream itself
    // wakes the host.
    assert!(!gate.needs_tick(0));
    assert!(!gate.needs_tick(10_000));
    assert_eq!(gate.next_deadline_ms(), None, "None means 'block', not 'due now'");
    gate.finger_up();
    assert_eq!(gate.fingers(), 0);
    assert!(!gate.needs_tick(10_000));
}

#[test]
fn input_deadline_exact_and_earliest_wins() {
    let mut gate = InputGate::new();
    gate.schedule(1_000);
    assert_eq!(gate.next_deadline_ms(), Some(1_000));
    assert!(!gate.needs_tick(999));
    assert!(gate.needs_tick(1_000), "exactly at the deadline is due");
    // A later schedule must not push an armed deadline back.
    gate.schedule(2_000);
    assert_eq!(gate.next_deadline_ms(), Some(1_000));
    // An earlier one replaces it.
    gate.schedule(500);
    assert_eq!(gate.next_deadline_ms(), Some(500));
    assert!(gate.needs_tick(500));
}

#[test]
fn input_deadline_take_due_consumes_once() {
    let mut gate = InputGate::new();
    gate.schedule(120);
    assert!(!gate.take_due(119), "not due yet: must stay armed");
    assert_eq!(gate.next_deadline_ms(), Some(120));
    assert!(gate.take_due(120));
    assert_eq!(gate.next_deadline_ms(), None, "a consumed deadline is cleared");
    assert!(!gate.take_due(120), "the same tick must not fire twice");
    assert!(!gate.needs_tick(121));
}

#[test]
fn input_deadline_cancel_clears_everything() {
    let mut gate = InputGate::new();
    gate.finger_down();
    gate.finger_down();
    gate.schedule(50);
    gate.cancel();
    assert_eq!(gate.fingers(), 0);
    assert_eq!(gate.next_deadline_ms(), None);
    assert!(!gate.needs_tick(50));
}

#[test]
fn input_visual_dirty_once_per_change() {
    let mut gate = InputGate::new();
    assert!(!gate.take_visual_dirty(), "nothing changed yet");
    gate.mark_visual_dirty();
    assert!(gate.take_visual_dirty());
    assert!(!gate.take_visual_dirty(), "one change publishes once");
    gate.mark_visual_dirty();
    gate.mark_visual_dirty();
    assert!(gate.take_visual_dirty());
    assert!(!gate.take_visual_dirty(), "two marks before a publish are one publish");
}
