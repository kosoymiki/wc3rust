//! Pure layout: the solver, its primitives and its audits.
//!
//! `solve_layout` is the only entry point an adapter needs. Everything else is
//! a primitive it composes: [`anchors`], [`flex`], [`grid`], [`safe_area`],
//! [`text_fit`] and [`breakpoints`].

pub mod anchors;
pub mod breakpoints;
pub mod engine;
pub mod flex;
pub mod grid;
pub mod safe_area;
pub mod screens;
pub mod text_fit;

pub use breakpoints::{Breakpoint, ScreenClass, Thresholds};
pub use engine::{
    solve_layout, Axis, ContentMetrics, Diagnostic, LayoutResult, Orientation, Placed, Preferences,
    SelectionState, State, Window,
};
pub use flex::{distribute, Distribution, Item};
pub use grid::GridPlan;
pub use text_fit::TextFit;
