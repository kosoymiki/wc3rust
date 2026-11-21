//! `ui.selection.box` — the marquee is the rectangle the player drew.
//!
//! `box_selection` filters world positions; the game feeds it the world points
//! under the two screen corners. Under the perspective camera that world box is
//! the axis-aligned hull of a trapezoid, so it is wider than the drawn marquee
//! at the far edge and sweeps in units standing beside the box. These tests pin
//! the screen-space rule (`box_selection_screen`) and the difference from the
//! world path, using projected positions the game already puts in every
//! snapshot (`ui::selection::snapshot` fills `screen`).

use glam::Vec2;
use wc3_input::selection::{UnitSnapshot, box_selection, box_selection_screen};

const PLAYER: u8 = 0;
const OTHER: u8 = 1;
const FOOTMAN: u32 = 1;
const PEON: u32 = 3;
const HALL: u32 = 40;

fn unit(id: u32, owner: u8, kind: u32, world: Vec2, screen: Vec2) -> UnitSnapshot {
    UnitSnapshot::new(id, owner, kind, world).on_screen(screen)
}

/// A 200×200 marquee at (100,100). The far edge is 2× wider on the ground, as
/// the perspective camera produces: world x spans 400 there, not 200.
const MARQUEE_MIN: Vec2 = Vec2::new(100.0, 100.0);
const MARQUEE_MAX: Vec2 = Vec2::new(300.0, 300.0);

fn marquee_units() -> Vec<UnitSnapshot> {
    vec![
        // Inside the drawn box.
        unit(1, PLAYER, FOOTMAN, Vec2::new(1000.0, 1000.0), Vec2::new(200.0, 200.0)),
        // Outside the drawn box (screen x = 350) but inside the world hull of
        // the two drag corners (the far-edge widening): the world rule sweeps
        // it in, the screen rule does not.
        unit(2, PLAYER, FOOTMAN, Vec2::new(1300.0, 1000.0), Vec2::new(350.0, 200.0)),
        // Inside, different kind: still selected (same-owner troops).
        unit(3, PLAYER, PEON, Vec2::new(1050.0, 1200.0), Vec2::new(150.0, 250.0)),
        // Inside the box but an enemy: never selected.
        unit(4, OTHER, FOOTMAN, Vec2::new(1000.0, 1000.0), Vec2::new(200.0, 200.0)),
        // Inside the box but a structure: a tap selects it, a drag does not.
        UnitSnapshot::new(5, PLAYER, HALL, Vec2::new(1000.0, 1000.0))
            .on_screen(Vec2::new(200.0, 200.0))
            .building(),
        // Behind the camera: no projection, never selected by a marquee.
        UnitSnapshot::new(6, PLAYER, FOOTMAN, Vec2::new(1500.0, 1500.0)),
    ]
}

#[test]
fn ui_selection_box_screen_uses_the_drawn_rectangle_not_the_world_hull() {
    let units = marquee_units();
    let picked = box_selection_screen(&units, PLAYER, MARQUEE_MIN, MARQUEE_MAX, false, &[]);
    assert_eq!(picked, vec![1, 3], "order stable, buildings/enemies/off-screen out");

    // The world path over the same corners (world hull of the drag) takes the
    // unit that stands beside the marquee: that is the defect this rule fixes.
    let world_min = Vec2::new(1000.0, 1000.0);
    let world_max = Vec2::new(1300.0, 1300.0);
    let world_picked = box_selection(&units, PLAYER, world_min, world_max, false, &[]);
    assert!(
        world_picked.contains(&2),
        "world AABB sweeps in a unit outside the drawn box: {world_picked:?}"
    );
}

#[test]
fn ui_selection_box_screen_additive_keeps_and_dedupes_the_group() {
    let units = marquee_units();
    let picked = box_selection_screen(&units, PLAYER, MARQUEE_MIN, MARQUEE_MAX, true, &[3, 9]);
    assert_eq!(picked, vec![3, 9, 1], "Shift keeps the group, no duplicate of 3");

    // Shift over an empty area keeps the group untouched.
    let empty = box_selection_screen(&units, PLAYER, Vec2::ZERO, Vec2::new(10.0, 10.0), true, &[7]);
    assert_eq!(empty, vec![7]);
    // A plain drag over an empty area clears it.
    let cleared = box_selection_screen(&units, PLAYER, Vec2::ZERO, Vec2::new(10.0, 10.0), false, &[7]);
    assert!(cleared.is_empty());
}
