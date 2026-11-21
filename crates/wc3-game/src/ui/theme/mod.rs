//! Design tokens as data (hud.md §5, polish addendum §3).
//!
//! * [`palette`] — colour roles, sampled from the retail console art, with a
//!   WCAG contrast audit over the same values the renderer uses.
//! * [`typography`] — the direction type scale in dp (display 28, title 22,
//!   heading 20, body 15, label 13, compact 11, numeric 15 tabular).
//! * [`metrics`] — every gap, floor and panel size in dp.
//! * [`states`] — the `(surface, state)` style table.
//! * [`tokens`] — the assembled [`Theme`].

pub mod metrics;
pub mod palette;
pub mod states;
pub mod tokens;
pub mod typography;

pub use metrics::Metrics;
pub use palette::{ContrastRow, Palette, Rgba, ROLES};
pub use states::{Surface, StateStyle};
pub use tokens::Theme;
pub use typography::{TypeRole, TypeScale, TypeStyle, Weight};
