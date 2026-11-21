//! Headless main-menu layout dump (device-QA regression guard).
//!
//! Boots a `Game` with no map and resizes to the reference landscape viewport.
//! `Game::sync_menu`/`resize` publish `GAME_MENU_LAYOUT` + one `GAME_MENU_ENTRY`
//! line per tappable rect; `scripts/test-all.sh` runs this through
//! `tests/gates/menu-layout-gate.sh` and fails when the main menu has fewer than
//! six entries. That is the guard for the "menu dead on device" defect: the
//! activation used to live only on the mouse-press path, so the published rects
//! and the hit test could diverge unnoticed.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-game --example menu_layout --offline
//! ```

use wc3_game::assets::Assets;
use wc3_game::game::Game;

fn main() {
    let mut game = Game::new(Assets::load(None, None), 0x5EED);
    // Same numbers as the reference row in analysis/hudskin/hud-layout-matrix.json;
    // resize republishes the rects, which is also the rotation path.
    game.resize(2414.0, 1080.0);
    // `Game::new` boots straight into the melee session when no map is loaded,
    // so the menu screen has to be opened explicitly; `open_main_menu` runs the
    // same sync + publication the device does from the pause screen.
    game.open_main_menu();
    let labels = game.menu_button_labels();
    println!("MENU_LABELS count={} labels={:?}", labels.len(), labels);
}
