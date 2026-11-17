//! P0-1 acceptance (ledger 439): a producer card must actually train.
//!
//! Audit `analysis/tech-lead/PLAYABILITY_GAP_2026_11_16.md` §0 defect A:
//! the barracks card rendered `UiAction::TrainDef` rows but the handler was
//! gated on `selected_hall()` (`is_main_hall()`), so the tap silently no-oped
//! and a human could never produce a combat unit.
//!
//! This test walks the player path on the real Justice map: build/complete a
//! barracks, select it, tap the FOOTMAN card slot, and assert the sim result
//! (unit + gold + supply) and the `GAME_TRAIN hfoo barracks=<id>` log line.

use std::path::Path;

use glam::Vec2;
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

#[test]
fn production_card_producer_trains_from_barracks() {
    let Some(mut game) = justice() else { return };
    let player = game.player;
    let worker = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == player && unit.def().stats.is_worker)
        .map(|unit| (unit.id, unit.pos))
        .expect("player worker");
    let hall = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == player && unit.is_building())
        .map(|unit| unit.pos)
        .expect("player hall");
    // A legal site for the barracks, chosen with the single placement
    // predicate the ghost/card use (geometry + units + mine + tech + cost).
    let mut site = None;
    'search: for ring in 1..20 {
        let radius = 300.0 + ring as f32 * 48.0;
        for step in 0..36 {
            let angle = step as f32 / 36.0 * std::f32::consts::TAU;
            let candidate = hall + Vec2::new(angle.cos(), angle.sin()) * radius;
            if game.sim.place_denial(player, "hbar", candidate).is_none() {
                site = Some(candidate);
                break 'search;
            }
        }
    }
    let site = site.expect("a legal barracks site near the hall");
    let barracks = game
        .sim
        .place_building_ordered(worker.0, "hbar", site)
        .expect("barracks site placed");
    // Construction completes through the real channel; tick until the site is
    // a finished building (build_ticks = 1800 at 30 ticks/s).
    for _ in 0..2_600 {
        if game
            .sim
            .unit(barracks)
            .map(|unit| unit.construct.is_none())
            .unwrap_or(false)
        {
            break;
        }
        game.update(1.0 / 30.0);
    }
    assert!(
        game.sim
            .unit(barracks)
            .map(|unit| unit.construct.is_none() && unit.alive)
            .unwrap_or(false),
        "barracks finished construction: {barracks:?}"
    );

    // Select it through the public selection state and rebuild the card the way
    // a tap does (activate_action ends with refresh_buttons).
    game.selection = vec![barracks];
    game.activate_action(UiAction::Stop);
    let slot = game
        .buttons
        .iter()
        .position(|button| button.action == UiAction::TrainDef("hfoo"))
        .unwrap_or_else(|| {
            panic!(
                "barracks card must expose FOOTMAN: {:?}",
                game.buttons
                    .iter()
                    .map(|button| (button.label.clone(), button.enabled))
                    .collect::<Vec<_>>()
            )
        });
    assert!(
        game.buttons[slot].enabled,
        "FOOTMAN slot is affordable and affordable rows are enabled: label={} sub={}",
        game.buttons[slot].label,
        game.buttons[slot].sub
    );

    game.sim.resources[player as usize] = 1_000;
    let gold_before = game.sim.resources[player as usize];
    let supply_before = game.supply();
    let footmen_before = game
        .sim
        .alive_units()
        .filter(|unit| unit.owner == player && unit.def().id == "hfoo")
        .count();

    let rect = game.buttons[slot].rect;
    game.pointer_down(rect.centre());
    game.pointer_up(rect.centre());

    let transaction = format!("GAME_TRAIN hfoo barracks={}", barracks.0);
    assert!(
        game.log.iter().any(|line| line == &transaction),
        "the card tap logs the producer: {transaction:?}\nlog tail: {:?}",
        &game.log[game.log.len().saturating_sub(6)..]
    );
    assert_eq!(
        game.sim.resources[player as usize],
        gold_before - 160,
        "FOOTMAN costs 160 gold and is charged when queued"
    );

    // train_ticks = 600; the unit pops and its supply is counted.
    for _ in 0..900 {
        game.update(1.0 / 30.0);
        let footmen = game
            .sim
            .alive_units()
            .filter(|unit| unit.owner == player && unit.def().id == "hfoo")
            .count();
        if footmen > footmen_before {
            break;
        }
    }
    let footmen_after = game
        .sim
        .alive_units()
        .filter(|unit| unit.owner == player && unit.def().id == "hfoo")
        .count();
    assert!(
        footmen_after > footmen_before,
        "the barracks produced a footman: {footmen_before} -> {footmen_after}"
    );
    assert_eq!(
        game.supply().0,
        supply_before.0 + 2,
        "a footman uses 2 supply: {:?} -> {:?}",
        supply_before,
        game.supply()
    );
    eprintln!(
        "PRODUCTION_CARD ok barracks={} line={:?} gold={gold_before}->{} supply={:?}->{:?} footmen={footmen_before}->{footmen_after}",
        barracks.0,
        transaction,
        game.sim.resources[player as usize],
        supply_before,
        game.supply()
    );
}
