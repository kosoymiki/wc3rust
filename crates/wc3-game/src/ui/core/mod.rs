//! Pure UI core: identity, geometry, constraints, semantics, elements, tree.
//!
//! This layer knows nothing about the game, the GPU, textures or the clock. It
//! answers exactly one question — *where does each element go* — and it answers
//! it for both the renderer and the input router from the same model
//! (hud.md §6.2/§6.3).

pub mod constraints;
pub mod element;
pub mod geometry;
pub mod id;
pub mod semantics;
pub mod tree;

pub use constraints::{Constraints, Extent, Len};
pub use element::{AlignX, AlignY, Anchor, Element, Flow, Label, Overflow};
pub use geometry::{Insets, Point, Rect, Size};
pub use id::Id;
pub use semantics::{Clip, HitMode, Semantics, WidgetState};
pub use tree::UiTree;
