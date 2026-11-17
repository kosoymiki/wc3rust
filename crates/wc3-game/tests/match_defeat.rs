//! `game.match.defeat` — the local seat loses under the same WC3 base rule.
//!
//! Every player building and every worker is gone, so the seat is eliminated
//! and the enemy wins. The defeat screen carries the match outcome.
use wc3_game::game::{Game, Phase, Screen};
use wc3_game::Assets;

fn game() -> Game {
    let mut game = Game::new(Assets::load(None, None), 0x468);
    game.enter_battle();
    game.resize(1280.0, 720.0);
    game
}

#[test]
fn game_match_defeat() {
    let mut game = game();
    let player = game.player;
    let enemy = game.enemy;

    let doomed: Vec<_> = game
        .sim
        .units
        .iter()
        .filter(|unit| unit.alive && unit.owner == player)
        .map(|unit| unit.id)
        .collect();
    assert!(!doomed.is_empty(), "the demo scenario seats the player's base");
    for id in doomed {
        game.sim.kill_unit(id);
    }
    game.drain_sim_events();

    let outcome = game
        .match_outcome()
        .expect("a player with no buildings and no workers is defeated");
    assert_eq!(outcome.winner, enemy);
    assert_eq!(outcome.defeated, vec![player]);
    assert_eq!(outcome.reason, "no-buildings-or-workers");
    assert_eq!(game.phase, Phase::Lost);
    assert_eq!(game.screen, Screen::Defeat);
    assert!(
        game.log
            .iter()
            .any(|line| line.starts_with("GAME_MATCH_END winner=1")),
        "GAME_MATCH_END line missing: {:?}",
        game.log
    );
    assert!(game.log.iter().any(|line| line.starts_with("GAME_SCORE")));
}
