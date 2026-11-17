//! `ui.hotkeys` — A/M/S/H/P + digits/Ctrl groups produce the expected
//! Selection/ControlGroups state (P0-10 UI/input parity).
//!
//! The input layer owns the key→intent mapping and the control groups; the
//! selection state a recall produces is the same `selection::recall` rule the
//! game uses (stored order, dead units dropped).

use glam::Vec2;
use wc3_input::selection::{self, UnitSnapshot};
use wc3_input::{ControlGroups, Hotkey, scancode_char};

const PLAYER: u8 = 0;

fn units() -> Vec<UnitSnapshot> {
    vec![
        UnitSnapshot::new(1, PLAYER, 1, Vec2::new(100.0, 100.0)), // footman
        UnitSnapshot::new(2, PLAYER, 1, Vec2::new(140.0, 100.0)), // footman
        UnitSnapshot::new(3, PLAYER, 2, Vec2::new(300.0, 300.0)), // archer
        UnitSnapshot::new(4, 1, 1, Vec2::new(500.0, 500.0)),      // enemy footman
    ]
}

#[test]
fn ui_hotkeys_card_letters_resolve_to_the_wc3_slots() {
    // Card slots 0..5 are A/M/S/H/P + ARMY(E); both the character path and the
    // SDL scancode path must resolve to the same card letter. SDL scancodes are
    // A=4 .. Z=29 (SDL_scancode.h), so the physical key is letter - 'a' + 4.
    for letter in ['a', 'm', 's', 'h', 'p', 'e'] {
        let scancode = 4 + (letter as u32 - 'a' as u32);
        assert_eq!(Hotkey::parse(letter), Some(Hotkey::Letter(letter)));
        assert_eq!(
            Hotkey::parse(letter.to_ascii_uppercase()),
            Some(Hotkey::Letter(letter)),
            "Shift must not change the card letter"
        );
        assert_eq!(scancode_char(scancode), Some(letter), "scancode {scancode}");
    }
    for digit in 0..=9u8 {
        let ch = char::from_digit(u32::from(digit), 10).expect("digit");
        assert_eq!(Hotkey::parse(ch), Some(Hotkey::Digit(digit)));
    }
    // Letters are orders: they never mutate the control-group state.
    let groups = ControlGroups::default();
    assert!(groups.is_empty_all());
    for letter in ['a', 'm', 's', 'h', 'p', 'e'] {
        assert_eq!(Hotkey::parse(letter), Some(Hotkey::Letter(letter)));
        assert!(groups.is_empty_all(), "{letter} must not create a group");
    }
}

#[test]
fn ui_hotkeys_ctrl_digit_assigns_and_digit_recalls_the_selection() {
    let units = units();
    let mut groups = ControlGroups::default();
    // Ctrl+4 assigns the selected footmen.
    assert!(groups.assign(4, &[1, 2]));
    assert_eq!(groups.len(4), 2);
    // Digit 4 recalls them in the stored order.
    assert_eq!(selection::recall(&groups, 4, &units), Some(vec![1, 2]));
    // Another assignment does not disturb group 4.
    assert!(groups.assign(5, &[3]));
    assert_eq!(selection::recall(&groups, 4, &units), Some(vec![1, 2]));
    assert_eq!(selection::recall(&groups, 5, &units), Some(vec![3]));
    // Ctrl+4 again replaces the stored contents.
    assert!(groups.assign(4, &[3]));
    assert_eq!(selection::recall(&groups, 4, &units), Some(vec![3]));
    // Ctrl+0 is the tenth group (WC3 1-0 row).
    assert!(groups.assign(0, &[1, 3]));
    assert_eq!(groups.len(0), 2);
    assert_eq!(selection::recall(&groups, 0, &units), Some(vec![1, 3]));
}

#[test]
fn ui_hotkeys_recall_keeps_group_order_and_drops_dead_units() {
    let mut units = units();
    units[0] = units[0].dead(); // footman 1 dies
    let mut groups = ControlGroups::default();
    assert!(groups.assign(2, &[2, 1, 3]));
    // Digit 2 selects the living members in stored order (2, then 3).
    assert_eq!(selection::recall(&groups, 2, &units), Some(vec![2, 3]));
    // Every member dead -> recall reports gone (the game toasts GROUP GONE).
    units[1] = units[1].dead();
    units[2] = units[2].dead();
    assert_eq!(selection::recall(&groups, 2, &units), None);
    // An empty slot has no selection.
    assert_eq!(selection::recall(&groups, 7, &units), None);
    // Cycle/free helpers stay in the WC3 1..9,0 order.
    assert_eq!(groups.next_free(), Some(1));
    let mut cycle = ControlGroups::default();
    cycle.assign(1, &[1]);
    cycle.assign(0, &[2]);
    assert_eq!(cycle.next_non_empty(1), Some(0));
    assert_eq!(cycle.next_non_empty(0), Some(1));
}
