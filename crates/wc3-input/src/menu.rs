//! Presentation-layer pointer contract for menus (HUD spec §8).
//!
//! This module is the *input* half of the menu action contract; the typed
//! screen model lives in `wc3_game::hud::menu`. It owns three rules that must
//! hold for mouse, touch and SDL's mirrored synthetic mouse events alike:
//!
//! 1. **Priority (§8.3).** A pointer is routed once, through
//!    modal → active menu → HUD → world. A modal owns every pointer event while
//!    it is up; an active menu owns the pointer before the HUD and the world;
//!    a disabled menu row still *swallows* the tap instead of leaking it to the
//!    battlefield behind the panel.
//! 2. **One physical gesture → at most one activation (§8.2).** A press arms
//!    exactly one target; the activation fires on release inside that target;
//!    a release outside, a lift after dragging off, and a second copy of the
//!    same physical gesture (SDL mirrors touch into a mouse press/release pair)
//!    all produce zero activations. [`ActivationGate`] additionally rejects an
//!    activation whose down point is a near-duplicate of the previous accepted
//!    one — even if the screen changed between the two copies, so the mirror
//!    can never activate a *different* row after a transition.
//! 3. **Touch geometry (§8.5).** Rows are hit with a nearest-centre 48 dp floor
//!    plus a small seam slop, and a press cancels as soon as the finger leaves
//!    the row by more than the drag-off slop. Nothing here invents geometry:
//!    the host feeds the rects it actually drew.
//!
//! Times are seconds on the host's monotonic clock; this module never reads a
//! clock. The host owns rendering state (hover/pressed art) and reads
//! [`MenuPointer::pressed_target`] for it.

use glam::Vec2;

use crate::{Rect, TouchButton};

/// Android's minimum square touch target, in dp (TZ §8 / HUD spec §8.5).
pub const MENU_MIN_TARGET_DP: f32 = 48.0;
/// A second down at the same point inside this window is the SDL mirror of the
/// first gesture, not a second tap. 300 ms covers the measured 10 fps device
/// frame where the mirror arrived, and stays below a deliberate double tap.
pub const MENU_MIRROR_WINDOW_S: f32 = 0.30;
/// Maximum distance, in dp, between the two copies of a mirrored gesture.
pub const MENU_MIRROR_DISTANCE_DP: f32 = 8.0;
/// A press cancels when the finger is farther than this from the row it began
/// on (the same 24 dp release slop the command card uses). Releasing outside
/// the row cancels regardless of this value.
pub const MENU_DRAG_OFF_SLOP_DP: f32 = 24.0;
/// Stationary hold after which the host may show a tooltip. A hold never
/// repeats the activation: the command fires once, on release.
pub const MENU_LONG_PRESS_S: f32 = 0.45;

/// Pointer-visible layers, highest priority first (§8.3). Keyboard shortcuts
/// are a separate path and are only reached when no pointer layer claimed the
/// event.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum InputLayer {
    Modal,
    Menu,
    Hud,
    World,
    GlobalHotkey,
}

/// One tappable item the host actually drew. `id` is the host's stable item id
/// (the typed model uses the legacy action code, so the device driver, the log
/// line and the hit test all name the same thing).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PointerTarget {
    pub id: u32,
    pub rect: Rect,
    pub enabled: bool,
}

impl PointerTarget {
    pub const fn new(id: u32, rect: Rect, enabled: bool) -> Self {
        Self { id, rect, enabled }
    }
}

/// Everything one pointer event may hit, in priority order. The host rebuilds
/// it from the same model the frame was drawn from; this module never queries
/// a screen.
#[derive(Clone, Copy, Debug)]
pub struct PointerContext<'a> {
    /// The top modal, if any. It owns every pointer event while present.
    pub modal: Option<PointerTarget>,
    /// The active menu screen's items, in draw order.
    pub menu: &'a [PointerTarget],
    /// HUD command buttons (same rects the HUD drew).
    pub hud: &'a [TouchButton],
    /// Minimap rect, when the HUD shows one.
    pub minimap: Option<Rect>,
    /// HUD panels / dead space that swallow the gesture.
    pub blocked: &'a [Rect],
    /// True while the game waits for a building-placement tap.
    pub placing: bool,
    /// px per dp (the one density factor).
    pub scale: f32,
}

/// Where the event went. The host applies the effect.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PointerRoute {
    /// A modal owns the pointer; the id is the modal's item.
    Modal(u32),
    /// The active menu owns the pointer; the id is the menu item. Disabled
    /// items still route here: the tap is consumed by the menu, not by the
    /// world behind it.
    Menu(u32),
    Minimap,
    /// HUD command button index.
    Hud(usize),
    /// A HUD panel or dead space swallowed the gesture.
    Blocked,
    /// A building-placement tap.
    Place,
    World,
}

impl PointerRoute {
    pub fn layer(self) -> InputLayer {
        match self {
            Self::Modal(_) => InputLayer::Modal,
            Self::Menu(_) => InputLayer::Menu,
            Self::Minimap | Self::Hud(_) | Self::Blocked => InputLayer::Hud,
            Self::Place | Self::World => InputLayer::World,
        }
    }
}

/// Route one pointer position through the priority order (§8.3).
///
/// `modal` wins even when its item is disabled: a visible modal is modal. Menus
/// beat the HUD, HUD beats the world, and the minimap keeps its place inside
/// the HUD layer.
pub fn route_pointer(point: Vec2, ctx: &PointerContext<'_>) -> PointerRoute {
    if let Some(modal) = ctx.modal {
        return PointerRoute::Modal(modal.id);
    }
    if let Some(id) = hit_menu(point, ctx.menu, ctx.scale) {
        return PointerRoute::Menu(id);
    }
    if let Some(rect) = ctx.minimap
        && rect.contains(point)
    {
        return PointerRoute::Minimap;
    }
    if let Some(index) = crate::hit_button(
        point,
        &crate::TouchLayout {
            minimap: None,
            blocked: &[],
            buttons: ctx.hud,
            placing: false,
            scale: ctx.scale,
        },
    ) {
        return PointerRoute::Hud(index);
    }
    if ctx.blocked.iter().any(|rect| rect.contains(point)) {
        return PointerRoute::Blocked;
    }
    if ctx.placing {
        return PointerRoute::Place;
    }
    PointerRoute::World
}

/// Menu hit test: an exact rect first, then a finger-sized band, nearest centre
/// wins so two rows never both claim one touch. Disabled rows are returned too —
/// the route consumes the tap and the activation reports the disabled reason.
pub fn hit_menu(point: Vec2, items: &[PointerTarget], scale: f32) -> Option<u32> {
    if let Some(item) = items.iter().find(|item| item.rect.contains(point)) {
        return Some(item.id);
    }
    let seam = 4.0 * scale.max(0.01);
    let mut best: Option<(u32, f32)> = None;
    for item in items {
        let rect = item.rect;
        if rect.w <= 0.0 || rect.h <= 0.0 {
            continue;
        }
        let pad_x = ((MENU_MIN_TARGET_DP * scale - rect.w) * 0.5).max(seam);
        let pad_y = ((MENU_MIN_TARGET_DP * scale - rect.h) * 0.5).max(seam);
        if !rect.expanded(pad_x, pad_y).contains(point) {
            continue;
        }
        let distance = rect.centre().distance(point);
        if best
            .map(|(_, best_distance)| distance < best_distance)
            .unwrap_or(true)
        {
            best = Some((item.id, distance));
        }
    }
    best.map(|(id, _)| id)
}

/// Whether a rect meets the 48 dp floor (craft/diagnostic check; the hit box
/// above already pads smaller rects, this reports the drawn one).
pub fn meets_touch_floor(rect: Rect, scale: f32) -> bool {
    rect.w + 1e-3 >= MENU_MIN_TARGET_DP * scale && rect.h + 1e-3 >= MENU_MIN_TARGET_DP * scale
}

/// Why a press produced no command.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CancelReason {
    /// The finger/cursor left the row before release (§8.2 "уход за пределы").
    DragOff,
    /// The host cancelled the gesture (modal opened, screen replaced, app
    /// backgrounded).
    Host,
    /// A newer press superseded this one.
    Replaced,
}

/// The press currently held. Public read-only shape for pressed art.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MenuPressState {
    pub target: u32,
    pub rect: Rect,
    pub down: Vec2,
    pub started: f32,
    pub gesture: u64,
    pub held: bool,
    pub cancelled: bool,
}

/// What a press did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PressOutcome {
    /// A fresh gesture armed one target.
    Armed { target: u32, gesture: u64 },
    /// The event was the SDL mirror of the previous physical press; nothing was
    /// armed, so a transient screen change between the copies cannot activate
    /// a different row.
    MirrorSuppressed { gesture: u64 },
    /// A press was already active and was replaced by this one.
    Replaced { target: u32, gesture: u64 },
}

/// What a release did. Exactly one variant can activate.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ReleaseOutcome {
    Activate {
        target: u32,
        gesture: u64,
        held_s: f32,
    },
    Cancel {
        target: u32,
        reason: CancelReason,
    },
    NoPress,
}

/// Press/release state machine for one pointer on a menu screen.
///
/// The host calls [`press`](Self::press) on pointer down, [`move_to`](Self::move_to)
/// on motion, and [`release`](Self::release) on lift. Activation is produced
/// only by `release` and only when the lift is inside the armed row, so a tap
/// can never fire twice (press + tap) or leak into a world order while a menu
/// is up.
#[derive(Clone, Debug, Default)]
pub struct MenuPointer {
    press: Option<MenuPressState>,
    last_down: Option<(Vec2, f32)>,
    gesture: u64,
}

impl MenuPointer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Arm one row. `rect` must be the rect that was drawn for `target`; `scale`
    /// is px per dp, used for the mirror distance.
    pub fn press(
        &mut self,
        target: u32,
        rect: Rect,
        at: Vec2,
        now: f32,
        scale: f32,
    ) -> PressOutcome {
        if let Some((last, when)) = self.last_down
            && self.press.is_none()
            && last.distance(at) <= MENU_MIRROR_DISTANCE_DP * scale.max(0.01)
            && now - when <= MENU_MIRROR_WINDOW_S
        {
            // The page did not move between the two copies; keep the original
            // gesture id and arm nothing.
            return PressOutcome::MirrorSuppressed {
                gesture: self.gesture,
            };
        }
        let replaced = self.press.is_some();
        self.gesture = self.gesture.wrapping_add(1);
        self.last_down = Some((at, now));
        self.press = Some(MenuPressState {
            target,
            rect,
            down: at,
            started: now,
            gesture: self.gesture,
            held: false,
            cancelled: false,
        });
        if replaced {
            PressOutcome::Replaced {
                target,
                gesture: self.gesture,
            }
        } else {
            PressOutcome::Armed {
                target,
                gesture: self.gesture,
            }
        }
    }

    /// Track motion. Returns `Some(DragOff)` the first time the pointer leaves
    /// the pressed row by more than the drag-off slop; the press is then dead
    /// even if the finger returns.
    pub fn move_to(&mut self, at: Vec2, scale: f32) -> Option<CancelReason> {
        let press = self.press.as_mut()?;
        if press.cancelled {
            return None;
        }
        let slop = MENU_DRAG_OFF_SLOP_DP * scale.max(0.01);
        if !press.rect.expanded(slop, slop).contains(at) {
            press.cancelled = true;
            return Some(CancelReason::DragOff);
        }
        None
    }

    /// Mark the press as a hold (tooltip); it still activates once on release.
    pub fn mark_held(&mut self, now: f32) {
        if let Some(press) = self.press.as_mut() {
            press.held = now - press.started >= MENU_LONG_PRESS_S;
        }
    }

    /// Finish the gesture. Fires `Activate` only when the lift is inside the
    /// armed row and the press was not cancelled.
    pub fn release(&mut self, at: Vec2, now: f32) -> ReleaseOutcome {
        let Some(press) = self.press.take() else {
            return ReleaseOutcome::NoPress;
        };
        if press.cancelled {
            return ReleaseOutcome::Cancel {
                target: press.target,
                reason: CancelReason::DragOff,
            };
        }
        if !press.rect.contains(at) {
            return ReleaseOutcome::Cancel {
                target: press.target,
                reason: CancelReason::DragOff,
            };
        }
        ReleaseOutcome::Activate {
            target: press.target,
            gesture: press.gesture,
            held_s: (now - press.started).max(0.0),
        }
    }

    /// Host cancel (modal opened, screen replaced, app backgrounded).
    pub fn cancel(&mut self) -> Option<CancelReason> {
        self.press.take().map(|press| {
            if press.cancelled {
                CancelReason::DragOff
            } else {
                CancelReason::Host
            }
        })
    }

    pub fn pressed_target(&self) -> Option<u32> {
        self.press
            .as_ref()
            .filter(|press| !press.cancelled)
            .map(|press| press.target)
    }

    pub fn press_state(&self) -> Option<MenuPressState> {
        self.press
    }

    pub fn is_active(&self) -> bool {
        self.press.is_some()
    }
}

/// Why an accepted-by-the-menu activation was still refused.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GateDecision {
    Allow,
    /// World-space duplicate of the previous accepted activation (SDL mirror,
    /// or the same event delivered twice by the platform).
    MirrorSuppressed {
        since_s: f32,
    },
    /// Same command re-issued faster than its declared cooldown (§8.1).
    Cooldown {
        remaining_s: f32,
    },
}

/// Command-level guard: the second and last line of defence against a doubled
/// activation. It is deliberately **independent of the target id** for the
/// mirror window: after a transition the mirror may land on a different row,
/// and "a different command" is still the same physical gesture. The cooldown
/// is per-target and comes from the model's `CommandSpec`.
#[derive(Clone, Debug, Default)]
pub struct ActivationGate {
    last: Option<Acceptance>,
}

#[derive(Clone, Copy, Debug)]
struct Acceptance {
    target: u32,
    at: Vec2,
    at_s: f32,
}

impl ActivationGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn admit(
        &mut self,
        target: u32,
        at: Vec2,
        now: f32,
        cooldown_s: f32,
        scale: f32,
    ) -> GateDecision {
        if let Some(last) = self.last {
            let elapsed = now - last.at_s;
            if last.at.distance(at) <= MENU_MIRROR_DISTANCE_DP * scale.max(0.01)
                && elapsed <= MENU_MIRROR_WINDOW_S
            {
                return GateDecision::MirrorSuppressed {
                    since_s: elapsed.max(0.0),
                };
            }
            if last.target == target && elapsed < cooldown_s {
                return GateDecision::Cooldown {
                    remaining_s: cooldown_s - elapsed,
                };
            }
        }
        self.last = Some(Acceptance {
            target,
            at,
            at_s: now,
        });
        GateDecision::Allow
    }

    /// Forget the last acceptance (new screen, resumed app, fresh host).
    pub fn reset(&mut self) {
        self.last = None;
    }
}
