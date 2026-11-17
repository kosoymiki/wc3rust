//! `ui.selection.double` — double-tap selects all same-type units **on screen**
//! (WC3 behaviour), excluding other kinds, enemy units, dead units and units
//! outside the viewport (P0-10 UI/input parity, gap-map row `ui.selection`).

use glam::Vec2;
use wc3_input::selection::{self, UnitSnapshot};

const PLAYER: u8 = 0;
const FOOTMAN: u32 = 1;
const ARCHER: u32 = 2;

fn viewport() -> Vec2 {
    Vec2::new(1280.0, 720.0)
}

/// Two living player footmen on screen, one archer, an enemy footman, a
/// footman off screen, a footman behind the camera and a dead footman.
fn snapshot() -> Vec<UnitSnapshot> {
    vec![
        UnitSnapshot::new(1, PLAYER, FOOTMAN, Vec2::new(100.0, 100.0))
            .on_screen(Vec2::new(200.0, 200.0)),
        UnitSnapshot::new(2, PLAYER, FOOTMAN, Vec2::new(140.0, 100.0))
            .on_screen(Vec2::new(300.0, 200.0)),
        UnitSnapshot::new(3, PLAYER, ARCHER, Vec2::new(300.0, 300.0))
            .on_screen(Vec2::new(400.0, 300.0)),
        UnitSnapshot::new(4, 1, FOOTMAN, Vec2::new(500.0, 500.0))
            .on_screen(Vec2::new(500.0, 400.0)),
        UnitSnapshot::new(5, PLAYER, FOOTMAN, Vec2::new(600.0, 600.0))
            .on_screen(Vec2::new(1400.0, 200.0)),
        UnitSnapshot::new(6, PLAYER, FOOTMAN, Vec2::new(700.0, 700.0)),
        UnitSnapshot::new(7, PLAYER, FOOTMAN, Vec2::new(800.0, 800.0))
            .on_screen(Vec2::new(250.0, 250.0))
            .dead(),
    ]
}

#[test]
fn ui_selection_double_selects_the_same_type_on_screen() {
    let selection = selection::same_type_on_screen(&snapshot(), PLAYER, FOOTMAN, viewport());
    // Only the two living, on-screen player footmen; stored order is preserved.
    assert_eq!(selection, vec![1, 2]);
}

#[test]
fn ui_selection_double_never_selects_other_kinds_or_enemies() {
    let units = snapshot();
    assert_eq!(
        selection::same_type_on_screen(&units, PLAYER, ARCHER, viewport()),
        vec![3],
        "the archer double-click selects archers only"
    );
    let footmen = selection::same_type_on_screen(&units, PLAYER, FOOTMAN, viewport());
    assert!(!footmen.contains(&4), "enemy footman must be excluded");
    assert!(
        selection::same_type_on_screen(&units, 9, FOOTMAN, viewport()).is_empty(),
        "an owner with no units selects nothing"
    );
}
