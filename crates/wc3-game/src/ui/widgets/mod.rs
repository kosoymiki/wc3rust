//! Widget models and the HUD composition.
//!
//! * [`command`] — the typed command card (id, icon, label, hotkey, enabled,
//!   reason, cooldown, cost, target, action) and its slot states.
//! * [`minimap`] — the world ↔ minimap transform, gestures and fog tiles.
//! * [`portrait`] — portrait model / icon / letter resolution with a
//!   once-per-def diagnostic.
//! * [`tooltip`] — content model and placement that never leaves the viewport.
//! * [`hud`] — the HUD composition and the fallback ladder.

pub mod command;
pub mod hud;
pub mod minimap;
pub mod portrait;
pub mod tooltip;

pub use command::{
    CommandAction, CommandCard, CommandId, CommandSlot, Cooldown, DisabledReason, ResourceCost,
    TargetType,
};
pub use hud::{solve_hud, HudRects, HudSolution, Plan};
pub use minimap::{Gesture, MinimapAction, MinimapTransform, WorldRect};
pub use portrait::PortraitSource;
pub use tooltip::{Tooltip, TooltipLayout};
