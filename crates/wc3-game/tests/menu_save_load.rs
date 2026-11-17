//! `game.menu.save-load-roundtrip` — the pause menu (ESC / F10 parity) carries
//! Resume / Save / Load / Restart / Quit with working actions. Save writes a
//! sim payload + map/grid gate + score sidecar; Load restores the exact board
//! (same save digest) and resumes; Quit returns to the main menu instead of
//! killing the process.
use std::path::PathBuf;

use wc3_game::game::{
    Game, Screen, MENU_ACT_LOAD, MENU_ACT_QUIT, MENU_ACT_RESTART, MENU_ACT_RESUME, MENU_ACT_SAVE,
};
use wc3_game::Assets;

fn game() -> Game {
    let mut game = Game::new(Assets::load(None, None), 0x46A);
    game.enter_battle();
    game.resize(1280.0, 720.0);
    game
}

#[test]
fn game_menu_save_load_roundtrip() {
    let mut game = game();
    let dir = std::env::temp_dir().join(format!("h2-game-menu-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    game.set_save_dir(&dir);
    game.update(0.034);

    game.open_game_menu();
    assert_eq!(game.screen, Screen::Paused);
    assert!(game.is_paused(), "opening the game menu pauses the sim");
    let labels = game.menu_button_labels();
    for expected in ["RESUME", "SAVE GAME", "LOAD GAME", "RESTART", "QUIT TO MENU"] {
        assert!(
            labels.contains(&expected),
            "pause menu is missing {expected}: {labels:?}"
        );
    }

    let tick = game.sim.tick;
    let digest = game.sim.save_digest();
    let units = game.sim.units.iter().filter(|unit| unit.alive).count();

    game.activate_menu(MENU_ACT_SAVE);
    let save = game.save_slot_path(1);
    assert!(save.is_file(), "save payload missing: {}", save.display());
    assert!(
        PathBuf::from(format!("{}.meta", save.display())).is_file(),
        "map/grid gate sidecar missing"
    );
    assert!(
        game.log
            .iter()
            .any(|line| line.starts_with("GAME_MENU action=save slot=1")),
        "save log missing: {:?}",
        game.log
    );
    // The saved battle is resumable: the menu offers LOAD now.
    assert!(game.save_slot_exists(1));
    assert!(game.menu_button_labels().contains(&"LOAD GAME"));

    // Damage the live board so a successful load has to change something.
    let victim = game
        .sim
        .units
        .iter()
        .find(|unit| unit.alive && unit.owner == game.enemy)
        .map(|unit| unit.id)
        .expect("enemy unit");
    game.sim.kill_unit(victim);
    assert_ne!(
        game.sim.units.iter().filter(|unit| unit.alive).count(),
        units,
        "the board changed before loading"
    );

    game.activate_menu(MENU_ACT_LOAD);
    assert_eq!(game.screen, Screen::Playing, "load resumes the battle");
    assert!(!game.is_paused());
    assert_eq!(game.sim.tick, tick, "load restores the saved tick");
    assert_eq!(
        game.sim.save_digest(),
        digest,
        "load restores the exact saved payload"
    );
    assert_eq!(
        game.sim.units.iter().filter(|unit| unit.alive).count(),
        units,
        "load restores the units killed after the save"
    );
    assert!(
        game.log
            .iter()
            .any(|line| line.starts_with("GAME_MENU action=load slot=1")),
        "load log missing: {:?}",
        game.log
    );

    // RESUME closes the menu and resumes ticks.
    game.open_game_menu();
    assert!(game.is_paused());
    game.activate_menu(MENU_ACT_RESUME);
    assert_eq!(game.screen, Screen::Playing);
    assert!(!game.is_paused());
    assert!(
        game.log.iter().any(|line| line == "GAME_PAUSE on=false"),
        "resume pause log missing: {:?}",
        game.log
    );

    // RESTART reseeds the same map and rebuilds the battle.
    let seed = game.seed();
    game.open_game_menu();
    game.activate_menu(MENU_ACT_RESTART);
    assert_ne!(game.seed(), seed, "restart reseeds the battle");
    assert_eq!(game.screen, Screen::Loading, "restart runs the loading screen");
    for _ in 0..150 {
        game.update(0.034);
        if game.screen == Screen::Playing {
            break;
        }
    }
    assert_eq!(game.screen, Screen::Playing);
    assert!(
        game.log
            .iter()
            .any(|line| line.starts_with("GAME_RESTART seed=")),
        "restart log missing: {:?}",
        game.log
    );

    // QUIT confirms once, then leaves the battle for the main menu (the
    // process keeps running; only the main menu's QUIT exits).
    game.open_game_menu();
    game.activate_menu(MENU_ACT_QUIT);
    assert_eq!(game.screen, Screen::Paused, "the first tap only arms quit");
    game.activate_menu(MENU_ACT_QUIT);
    assert_eq!(game.screen, Screen::MainMenu, "pause QUIT returns to the menu");
    assert!(
        game.log
            .iter()
            .any(|line| line.starts_with("GAME_MENU action=quit")),
        "quit log missing: {:?}",
        game.log
    );

    let _ = std::fs::remove_dir_all(&dir);
}
