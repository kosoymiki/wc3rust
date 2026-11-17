//! One placement predicate for the player path: `Sim::place_denial`.
//!
//! Ledger 439 regression: `can_place` is geometry only (walkable + blocked
//! cells), while the placement also refuses a unit standing inside the
//! footprint, a gold mine under it, unmet requirements and unpaid cost. The
//! HUD ghost and the card used to check `can_place`, so a "legal" green ghost
//! could become `DENY BUILD site occupied` on the tap. This test pins the
//! strict predicate as the single truth the game layer consumes and proves the
//! two predicates really differ on the live Justice base.

use std::path::Path;

use wc3_game::assets::Assets;
use wc3_game::game::Game;

const MAP: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

#[test]
fn place_denial_is_the_single_player_placement_predicate() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(MAP);
    if !path.is_file() {
        eprintln!("skip: Justice.w3x missing");
        return;
    }
    let mut game = Game::new(Assets::load(None, Some(&path)), 0x5EED);
    game.enter_battle();
    let player = game.player;
    let hall = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == player && unit.is_building())
        .map(|unit| unit.pos)
        .expect("player hall");
    let def_id = "otrb";
    let mut geometry_but_denied = 0usize;
    let mut legal = 0usize;
    for ring in 0..24 {
        let radius = 240.0 + ring as f32 * 32.0;
        for step in 0..32 {
            let angle = step as f32 / 32.0 * std::f32::consts::TAU;
            let candidate = hall + wc3_sim::Vec2::new(angle.cos(), angle.sin()) * radius;
            let can_place = game.sim.can_place(def_id, candidate);
            let denial = game.sim.place_denial(player, def_id, candidate);
            if denial.is_none() {
                legal += 1;
                // The strict predicate never accepts a point geometry rejects.
                assert!(
                    can_place,
                    "place_denial accepted a spot can_place rejects at {candidate:?}"
                );
                assert!(
                    game.sim.can_build_now(player, def_id),
                    "a legal candidate implies the card affordance is live"
                );
            } else if can_place {
                // A worker/peon stands in the footprint, a mine sits under it,
                // or the bank is short: exactly the cases the old ghost missed.
                geometry_but_denied += 1;
            }
        }
    }
    assert!(legal > 0, "the base has at least one legal burrow site");
    assert!(
        geometry_but_denied > 0,
        "the strict predicate must refuse at least one geometry-legal point around a live base"
    );
    eprintln!(
        "PLACEMENT_PREDICATE def={def_id} legal={legal} geometry_but_denied={geometry_but_denied}"
    );
}
