//! Diagnostic: print the real worker card for the shipped Orc01 melee scenario.
use std::path::Path;
use wc3_game::assets::Assets;
use wc3_game::game::Game;

#[test]
fn card_dump_peon() {
    let map = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3/Maps/Campaign/Orc01.w3m");
    if !map.is_file() {
        eprintln!("skip: map missing");
        return;
    }
    let mut game = Game::new(Assets::load(None, Some(&map)), 0x5EED);
    let peon = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == game.player && unit.def().stats.is_worker)
        .map(|unit| unit.id)
        .expect("peon");
    game.selection = vec![peon];
    game.update(0.0);
    eprintln!("selection={:?} buttons={}", game.selection, game.buttons.len());
    for (index, button) in game.buttons.iter().enumerate() {
        eprintln!(
            "CARD {index} label={} action={:?} enabled={} hotkey='{}' rect=({:.0},{:.0},{:.0},{:.0})",
            button.label, button.action, button.enabled, button.hotkey,
            button.rect.x, button.rect.y, button.rect.w, button.rect.h
        );
    }
}
