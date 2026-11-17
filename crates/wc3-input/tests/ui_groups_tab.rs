//! `ui.groups.tab` — Tab cycles subgroups inside the selection (hero first,
//! then unit types), Shift+Tab reverses, and the whole selection is kept
//! (P0-10, gap-map row `ui.hotkeys`).

use glam::Vec2;
use wc3_input::selection::UnitSnapshot;
use wc3_input::subgroups::Subgroups;

const PLAYER: u8 = 0;
const FOOTMAN: u32 = 1;
const ARCHER: u32 = 2;
const PEASANT: u32 = 3;
const HERO: u32 = 9;

fn unit(id: u32, kind: u32) -> UnitSnapshot {
    UnitSnapshot::new(id, PLAYER, kind, Vec2::new(id as f32 * 10.0, 0.0))
}

#[test]
fn ui_groups_tab_cycles_unit_types_without_a_hero() {
    let units = [
        unit(1, FOOTMAN),
        unit(2, ARCHER),
        unit(3, FOOTMAN),
        unit(4, PEASANT),
    ];
    let selected = vec![1, 2, 3, 4];
    let mut groups = Subgroups::build(&units, &selected);
    // First appearance order, no hero.
    assert_eq!(groups.order(), &[FOOTMAN, ARCHER, PEASANT]);
    assert_eq!(groups.active(), Some(FOOTMAN));
    assert_eq!(groups.active_units(&units, &selected), vec![1, 3]);
    // Tab: archer -> peasant -> wrap to footman.
    assert_eq!(groups.cycle(false), Some(ARCHER));
    assert_eq!(groups.active_units(&units, &selected), vec![2]);
    assert_eq!(groups.cycle(false), Some(PEASANT));
    assert_eq!(groups.active_units(&units, &selected), vec![4]);
    assert_eq!(groups.cycle(false), Some(FOOTMAN));
    // Shift+Tab reverses from the first entry (wraps to the last).
    assert_eq!(groups.cycle(true), Some(PEASANT));
    assert_eq!(groups.cycle(true), Some(ARCHER));
    // The selection itself never changes.
    assert_eq!(selected, vec![1, 2, 3, 4]);
}

#[test]
fn ui_groups_tab_hero_is_first_and_shift_tab_reverses() {
    let units = [
        unit(1, FOOTMAN),
        unit(2, HERO).hero(),
        unit(3, ARCHER),
        unit(4, FOOTMAN),
    ];
    let selected = vec![1, 2, 3, 4];
    let mut groups = Subgroups::build(&units, &selected);
    // Hero first, then the remaining types by first appearance.
    assert_eq!(groups.order(), &[HERO, FOOTMAN, ARCHER]);
    assert_eq!(groups.active(), Some(HERO));
    assert_eq!(groups.active_units(&units, &selected), vec![2]);
    assert_eq!(groups.cycle(false), Some(FOOTMAN));
    assert_eq!(groups.active_units(&units, &selected), vec![1, 4]);
    // Shift+Tab from footman lands back on the hero.
    assert_eq!(groups.cycle(true), Some(HERO));
    // A dead hero is not a subgroup and cannot be active.
    let mut dead_hero = units;
    dead_hero[1] = dead_hero[1].dead();
    let groups = Subgroups::build(&dead_hero, &selected);
    assert_eq!(groups.order(), &[FOOTMAN, ARCHER]);
    assert_eq!(groups.active(), Some(FOOTMAN));
}
