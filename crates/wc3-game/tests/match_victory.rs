//! `game.match.victory` — WC3 melee victory: the last seat with a base wins.
//!
//! A hall falling is not enough (a barracks and workers can rebuild); the
//! enemy is beaten when every building it owns is gone *and* no worker is left
//! to put one down. The match then exposes a [`MatchOutcome`] and ends on the
//! victory screen with the `GAME_MATCH_END` / `GAME_SCORE` evidence lines.
use wc3_game::game::{Game, Phase, Screen};
use wc3_game::Assets;

fn game() -> Game {
    let mut game = Game::new(Assets::load(None, None), 0x467);
    game.enter_battle();
    game.resize(1280.0, 720.0);
    game
}

#[test]
fn game_match_victory() {
    let mut game = game();
    let enemy = game.enemy;
    assert!(
        game.match_outcome().is_none(),
        "a fresh match is undecided: {:?}",
        game.match_outcome()
    );

    let doomed: Vec<_> = game
        .sim
        .units
        .iter()
        .filter(|unit| unit.alive && unit.owner == enemy)
        .map(|unit| unit.id)
        .collect();
    assert!(!doomed.is_empty(), "the demo scenario seats an enemy army");
    for id in doomed {
        game.sim.kill_unit(id);
    }
    game.drain_sim_events();

    let outcome = game
        .match_outcome()
        .expect("no enemy building and no enemy worker means the enemy is defeated");
    assert_eq!(outcome.winner, game.player);
    assert_eq!(outcome.defeated, vec![enemy]);
    assert_eq!(outcome.reason, "no-buildings-or-workers");
    assert_eq!(game.phase, Phase::Won);
    assert_eq!(game.screen, Screen::Victory);
    assert!(
        game.log
            .iter()
            .any(|line| line.starts_with("GAME_MATCH_END winner=0")),
        "GAME_MATCH_END line missing: {:?}",
        game.log
    );
    assert!(
        game.log.iter().any(|line| line.starts_with("GAME_SCORE")),
        "GAME_SCORE dump missing: {:?}",
        game.log
    );

    // The match is decided once: a second evaluation cannot flip the winner.
    assert_eq!(game.evaluate_match(), Some(outcome.clone()));
}
