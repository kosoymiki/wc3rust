//! `ui.selection.add` — Shift / long-press add keeps the existing group
//! (WC3 Shift-click and Shift-box), while the unmodified gestures replace it
//! (P0-10 UI/input parity, gap-map row `ui.selection`).

use glam::Vec2;
use wc3_input::selection::{self, UnitSnapshot, Mode};

const PLAYER: u8 = 0;
const FOOTMAN: u32 = 1;

fn units() -> Vec<UnitSnapshot> {
    vec![
        UnitSnapshot::new(1, PLAYER, FOOTMAN, Vec2::new(100.0, 100.0)),
        UnitSnapshot::new(2, PLAYER, FOOTMAN, Vec2::new(140.0, 100.0)),
        UnitSnapshot::new(3, PLAYER, FOOTMAN, Vec2::new(300.0, 300.0)),
        UnitSnapshot::new(4, 1, FOOTMAN, Vec2::new(120.0, 120.0)),
        UnitSnapshot::new(5, PLAYER, FOOTMAN, Vec2::new(500.0, 500.0)).dead(),
    ]
}

#[test]
fn ui_selection_add_shift_box_keeps_the_group_and_appends() {
    let units = units();
    let min = Vec2::new(50.0, 50.0);
    let max = Vec2::new(200.0, 200.0);
    let current = vec![9u32];
    // Shift-box: the existing group survives, the boxed living player units are
    // appended, the enemy and the dead unit are ignored.
    let additive = selection::box_selection(&units, PLAYER, min, max, true, &current);
    assert_eq!(additive, vec![9, 1, 2]);
    // The same box without Shift replaces the selection.
    let replace = selection::box_selection(&units, PLAYER, min, max, false, &current);
    assert_eq!(replace, vec![1, 2]);
    // Shift-boxing an already selected unit does not duplicate it.
    let dedupe = selection::box_selection(&units, PLAYER, min, max, true, &[1]);
    assert_eq!(dedupe, vec![1, 2]);
}

#[test]
fn ui_selection_add_shift_tap_and_long_press_keep_the_group() {
    // Shift-tap (and the long-press add that shares this path) appends one unit.
    let mut current = vec![2u32];
    selection::merge(&mut current, 5, Mode::Add);
    assert_eq!(current, vec![2, 5]);
    // Adding an already selected unit keeps the group unchanged.
    selection::merge(&mut current, 2, Mode::Add);
    assert_eq!(current, vec![2, 5]);
    // A plain tap replaces the whole selection.
    selection::merge(&mut current, 7, Mode::Replace);
    assert_eq!(current, vec![7]);
}
