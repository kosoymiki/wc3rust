# App startup patch — show HUD parity menu (owner: dispatcher/app)

`game.rs` already exposes `Game::open_main_menu()` (`game.rs:1088`) and draws
`MenuScreen::{Main,MapSelect,Loading}` via `MobileHud::draw_menu`. The host must
open it once after constructing the game.

File: `apps/android-native/src/lib.rs` (read-only for HUD)

At the point where the game is created (currently `let mut game = Game::new(assets, 0xC0FFEE);`),
add:

```rust
    game.open_main_menu();
```

Then the first frame shows WIN / MAP SELECT / QUIT; PLAY → LOADING → PLAYING.
Tests that construct `Game` directly remain in `Screen::Playing`, so this is
host-only behavior.

Optional: expose back navigation by forwarding Escape/Back to
`game.activate_menu(3)` while `game.screen == Screen::MapSelect`; the method is
currently private, make it `pub fn activate_menu(&mut self, action: u8)` if the
platform needs it.
