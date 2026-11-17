//! Operator bug: «здания не строятся».
//!
//! The sim is healthy (`examples/build_probe.rs`: `can_place` 98.6% around the
//! start, atomic placement completes), so the defect must be in the *tap chain*:
//! select a worker → tap the card's BUILD slot → tap the ground.
//!
//! This test walks that chain through the real public API on the real map and
//! the real 2414x1080 layout with the device insets:
//!   * the card slot is found by `UiAction::BuildDef(_)`, not by index;
//!   * the tap is a pointer down/up at the slot centre and at a world point, so
//!     the touch router (HudHit → TouchEnd::Place → place_building) runs;
//!   * the assertion is the sim state (a new building owned by the player whose
//!     construct lifecycle started), plus the `PLACE_` log lines.

use std::path::Path;

use glam::{Vec2, Vec3};
use wc3_game::{Assets, Game, UiAction};

const MAP: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

fn justice() -> Option<Game> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(MAP);
    if !path.is_file() {
        eprintln!("skip: Justice.w3x missing");
        return None;
    }
    let mut game = Game::new(Assets::load(None, Some(&path)), 0x5EED);
    game.enter_battle();
    game.resize(2414.0, 1080.0);
    game.set_safe_insets(210.0, 120.0, 90.0, 96.0);
    Some(game)
}

fn tap(game: &mut Game, screen: Vec2) {
    game.pointer_down(screen);
    game.pointer_up(screen);
}

#[test]
fn build_via_card_slot_places_a_building() {
    let Some(mut game) = justice() else { return };

    // 1. Select the worker by tapping it in the world (the phone path).
    let worker = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == game.player && unit.def().stats.is_worker)
        .map(|unit| (unit.id, unit.pos))
        .expect("worker on Justice");
    let (worker_id, worker_pos) = worker;
    let screen = game
        .camera
        .world_to_screen(Vec3::new(
            worker_pos.x,
            game.assets.height_at(worker_pos.x, worker_pos.y),
            worker_pos.y,
        ))
        .expect("worker on screen");
    assert!(
        !game.layout.top_bar.contains(screen) && !game.layout.command.contains(screen),
        "the worker must be tappable outside the console"
    );
    tap(&mut game, screen);
    assert!(
        game.selection.contains(&worker_id),
        "tap selects the worker (selection={:?})",
        game.selection
    );

    // 2. The card must expose a build entry for that worker.
    let slot = game
        .buttons
        .iter()
        .position(|button| matches!(button.action, UiAction::BuildDef(_)) && button.enabled)
        .unwrap_or_else(|| {
            panic!(
                "no enabled BuildDef slot on the worker card: {:?}",
                game.buttons
                    .iter()
                    .map(|b| (b.label.clone(), b.enabled))
                    .collect::<Vec<_>>()
            )
        });
    let slot_rect = game.buttons[slot].rect;
    let label = game.buttons[slot].label.clone();
    let def_id = match game.buttons[slot].action {
        UiAction::BuildDef(id) => id,
        _ => unreachable!("selected slot is a build entry"),
    };
    tap(&mut game, slot_rect.centre());

    // 3. Placement mode must be live after the tap.
    assert!(
        game.placing.is_some(),
        "tapping the BUILD slot {slot} ({label}) must enter placement mode"
    );

    // 4. Tap the ground and expect a building.
    let before = game.sim.alive_units().filter(|u| u.is_building()).count();
    // Pick a legal spot with the single placement predicate the game uses for
    // the ghost, the card affordance and the placement itself
    // (`Sim::place_denial`: geometry, unit footprint, gold mine, requirements
    // and cost). `can_place` is geometry only and can accept a spot the
    // placement then refuses ("site occupied" by a unit/mine).
    let base = game
        .sim
        .alive_units()
        .find(|u| u.owner == game.player && u.is_building())
        .map(|u| u.pos)
        .unwrap_or_else(|| Vec2::new(0.0, 0.0));
    let mut spot = None;
    'search: for ring in 1..16 {
        let radius = 260.0 + ring as f32 * 48.0;
        for step in 0..24 {
            let angle = step as f32 / 24.0 * std::f32::consts::TAU;
            let candidate = base + Vec2::new(angle.cos(), angle.sin()) * radius;
            if game.sim.place_denial(game.player, def_id, candidate).is_none() {
                spot = Some(candidate);
                break 'search;
            }
        }
    }
    let spot = spot.expect("a legal build spot near the base");
    let spot_screen = game
        .camera
        .world_to_screen(Vec3::new(
            spot.x,
            game.assets.height_at(spot.x, spot.y),
            spot.y,
        ))
        .expect("build spot on screen");
    tap(&mut game, spot_screen);
    game.update(1.0 / 30.0);

    let log = game.take_log().join("\n");
    let after = game.sim.alive_units().filter(|u| u.is_building()).count();
    assert!(
        after > before,
        "no building appeared.\nlabel={label} spot={spot:?}\nlog:\n{log}"
    );
    assert!(
        log.contains("PLACE_TAP") && log.contains("PLACE_WORLD"),
        "the touch router must report the placement tap.\nlog:\n{log}"
    );
}
