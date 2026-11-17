//! RTS camera facade: the canonical implementation lives in `wc3-input` so the
//! game, the renderer and the touch layer share one projection, one pan/zoom
//! path and one screen<->world conversion.
//!
//! `wc3_game::camera::Camera` is the same type as `wc3_input::Camera`
//! (re-export, not a copy), so `lib.rs`'s `pub use camera::Camera` keeps working
//! and every call site is unchanged.

pub use wc3_input::Camera;
