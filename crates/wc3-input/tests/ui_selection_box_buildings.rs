//! `ui.selection.box` — a drag box picks troops only.
//!
//! WC3 rule (operator bug: «здания не должны с юнитами выбираться в армию»):
//! `SelectionBox` accepts units; a structure is selected by a single tap and is
//! never swept into a group by dragging. The policy lives in `wc3-input` so the
//! keyboard and the touch path share one implementation.

use glam::Vec2;
use wc3_input::selection::{box_selection, UnitSnapshot};

const PLAYER: u8 = 0;
const OTHER: u8 = 1;
const FOOTMAN: u32 = 1;
const PEON: u32 = 3;
const HALL: u32 = 40;

fn unit(id: u32, owner: u8, kind: u32, x: f32, y: f32) -> UnitSnapshot {
    UnitSnapshot::new(id, owner, kind, Vec2::new(x, y))
}

fn building(id: u32, owner: u8, x: f32, y: f32) -> UnitSnapshot {
    unit(id, owner, HALL, x, y).building()
}

#[test]
fn ui_selection_box_skips_buildings() {
    let units = [
        unit(1, PLAYER, FOOTMAN, 10.0, 10.0),
        unit(2, PLAYER, PEON, 20.0, 20.0),
        building(3, PLAYER, 30.0, 30.0),
        unit(4, OTHER, FOOTMAN, 15.0, 15.0),
        unit(5, PLAYER, FOOTMAN, 900.0, 900.0),
    ];
    let picked = box_selection(
        &units,
        PLAYER,
        Vec2::new(0.0, 0.0),
        Vec2::new(100.0, 100.0),
        false,
        &[],
    );
    assert_eq!(picked, vec![1, 2], "only friendly troops inside the box");
}

#[test]
fn ui_selection_box_over_only_buildings_selects_nothing() {
    let units = [
        building(3, PLAYER, 30.0, 30.0),
        building(4, PLAYER, 60.0, 60.0),
    ];
    let picked = box_selection(
        &units,
        PLAYER,
        Vec2::new(0.0, 0.0),
        Vec2::new(100.0, 100.0),
        false,
        &[],
    );
    assert!(picked.is_empty(), "a structure is tapped, never boxed");
}

#[test]
fn ui_selection_box_additive_keeps_the_group_and_ignores_buildings() {
    let units = [building(3, PLAYER, 30.0, 30.0), unit(7, PLAYER, PEON, 40.0, 40.0)];
    let picked = box_selection(
        &units,
        PLAYER,
        Vec2::new(0.0, 0.0),
        Vec2::new(100.0, 100.0),
        true,
        &[9],
    );
    assert_eq!(picked, vec![9, 7], "the existing group survives, the hall stays out");
}
