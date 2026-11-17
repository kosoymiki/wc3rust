//! wc3rust playable game layer.
//!
//! `Assets` loads the user's Warcraft III data, `Game` runs the deterministic
//! simulation with real commands, AI, economy and win/lose conditions, and
//! `Renderer` draws the real MDX units, terrain and HUD through wgpu.

pub mod assets;
pub mod mem;
pub mod re;
pub mod camera;
pub mod campaign;
pub mod cinematic;
pub mod game;
pub mod sim_bridge;
pub mod hud;
pub mod hudskin;
pub mod ui_atlas;
pub mod style;
pub mod fdf_ui;
pub mod ui;
pub mod mipmap;
pub mod render;
pub mod model_render;
pub mod doodads;
pub mod map_registry;
pub mod terrain_render;
#[cfg(test)]
pub mod world_audit;

pub use assets::Assets;
pub use camera::Camera;
pub use campaign::{Campaign, Cinematic, Mission, Objective};
pub use game::{Game, Phase, Scene, UiAction};
pub use hud::{Button, Font, HudLayout, UiBatch};
pub use render::Renderer;
/// Skinning path selector (`Gpu` = vertex-stage palette, `Cpu` = reference).
pub use wc3_render::mesh::SkinMode;

use std::sync::OnceLock;

static LOG_SINK: OnceLock<fn(&str)> = OnceLock::new();

/// Route library diagnostics to the platform logger.
///
/// The Android app forwards them to `__android_log_print` (library stdout /
/// stderr is not mirrored by SDL), so `GAME_*` device diagnostics stay
/// observable. Headless builds leave the sink unset and print to stdout.
pub fn set_log_sink(sink: fn(&str)) {
    let _ = LOG_SINK.set(sink);
}

/// Print one library diagnostic line.
pub fn dev_log(message: String) {
    match LOG_SINK.get() {
        Some(sink) => sink(&message),
        None => println!("{message}"),
    }
}

/// `eprintln!`-shaped macro that goes through [`dev_log`].
#[macro_export]
macro_rules! dev_log {
    ($($arg:tt)*) => {
        $crate::dev_log(format!($($arg)*))
    };
}
