//! `game.pause.stops-ticks` — WC3 single-player pause: the sim stops ticking
//! while the frame still renders (camera/HUD/input), and resume continues from
//! the exact tick. The F10/ESC game menu is the same pause.
use wc3_game::game::{Game, Screen, MENU_ACT_RESUME};
use wc3_game::Assets;

fn game() -> Game {
    let mut game = Game::new(Assets::load(None, None), 0x46B);
    game.enter_battle();
    game.resize(1280.0, 720.0);
    game
}

#[test]
fn game_pause_stops_ticks() {
    let mut game = game();
    for _ in 0..12 {
        game.update(0.034);
    }
    let soldier = game
        .sim
        .units
        .iter()
        .find(|unit| unit.alive && unit.owner == game.player && !unit.is_building())
        .map(|unit| unit.id)
        .expect("player soldier");
    let tick = game.sim.tick;
    let pos = game.sim.unit(soldier).unwrap().pos;
    assert!(tick > 0, "the battle ticks before the pause");

    assert!(game.set_paused(true));
    assert!(game.is_paused());
    let frames = game.frames;
    for _ in 0..90 {
        game.update(0.034);
    }
    assert_eq!(game.sim.tick, tick, "a paused sim must not tick");
    assert_eq!(
        game.sim.unit(soldier).unwrap().pos,
        pos,
        "a paused sim must not move units"
    );
    assert!(game.frames > frames, "frames keep rendering while paused");
    assert!(
        game.log.iter().any(|line| line == "GAME_PAUSE on=true"),
        "pause log missing: {:?}",
        game.log
    );

    assert!(game.set_paused(false));
    for _ in 0..45 {
        game.update(0.034);
    }
    assert!(game.sim.tick > tick, "resume ticks the sim again");
    assert!(
        game.log.iter().any(|line| line == "GAME_PAUSE on=false"),
        "resume log missing: {:?}",
        game.log
    );

    // F10/ESC parity: the game menu is the same pause.
    game.open_game_menu();
    assert_eq!(game.screen, Screen::Paused);
    let menu_tick = game.sim.tick;
    for _ in 0..60 {
        game.update(0.034);
    }
    assert_eq!(game.sim.tick, menu_tick, "the open game menu freezes the sim");
    game.activate_menu(MENU_ACT_RESUME);
    assert_eq!(game.screen, Screen::Playing);
    for _ in 0..45 {
        game.update(0.034);
    }
    assert!(game.sim.tick > menu_tick, "closing the menu resumes the sim");
}
