//! Order modes and minimap intents (`ui.orders.targeting`, `ui.minimap.orders`).
//!
//! Attack (A), Move (M), Patrol (P), Hold (H) and Stop (S) arm an order mode;
//! the next world tap consumes it exactly once, and Escape cancels it. The
//! minimap tap is a camera jump, long-press is an attack-move and the mouse
//! right button (desktop path) is a move order.

use crate::Hotkey;

/// The five WC3 order modes that can be armed from the card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderKind {
    AttackMove,
    Move,
    Patrol,
    Hold,
    Stop,
}

impl OrderKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::AttackMove => "attack_move",
            Self::Move => "move",
            Self::Patrol => "patrol",
            Self::Hold => "hold",
            Self::Stop => "stop",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "attack_move" => Some(Self::AttackMove),
            "move" => Some(Self::Move),
            "patrol" => Some(Self::Patrol),
            "hold" => Some(Self::Hold),
            "stop" => Some(Self::Stop),
            _ => None,
        }
    }
}

/// At most one armed order mode. `take` is the only consumer, so a mode can be
/// applied to exactly one tap; `cancel` (Escape) clears it without consuming.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OrderArmer {
    armed: Option<OrderKind>,
}

impl OrderArmer {
    pub fn arm(&mut self, kind: OrderKind) -> Option<OrderKind> {
        self.armed.replace(kind)
    }

    pub fn armed(&self) -> Option<OrderKind> {
        self.armed
    }

    pub fn take(&mut self) -> Option<OrderKind> {
        self.armed.take()
    }

    pub fn cancel(&mut self) -> Option<OrderKind> {
        self.armed.take()
    }
}

/// Keyboard letter -> armed order; WC3 card slots A/M/P/H/S.
pub fn order_for_hotkey(hotkey: Hotkey) -> Option<OrderKind> {
    match hotkey {
        Hotkey::Letter('a') => Some(OrderKind::AttackMove),
        Hotkey::Letter('m') => Some(OrderKind::Move),
        Hotkey::Letter('p') => Some(OrderKind::Patrol),
        Hotkey::Letter('h') => Some(OrderKind::Hold),
        Hotkey::Letter('s') => Some(OrderKind::Stop),
        _ => None,
    }
}

/// SDL_SCANCODE_TAB (SDL3 scancode.h) — the subgroup cycle key.
pub fn is_tab_scancode(scancode: u32) -> bool {
    scancode == 43
}

/// What a minimap press should do for the current selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MinimapAction {
    /// Short tap: jump the camera (WC3 left-click).
    Camera,
    /// Mouse right button: move order for the selection (WC3 right-click).
    Move,
    /// Touch long-press: attack-move for the selection.
    AttackMove,
}

impl MinimapAction {
    pub fn id(self) -> &'static str {
        match self {
            Self::Camera => "camera",
            Self::Move => "move",
            Self::AttackMove => "attack_move",
        }
    }
}

/// Minimap intent: right-click is a move, long-press an attack-move, a short
/// tap a camera jump.
pub fn minimap_action(hold: bool, right_click: bool) -> MinimapAction {
    if right_click {
        MinimapAction::Move
    } else if hold {
        MinimapAction::AttackMove
    } else {
        MinimapAction::Camera
    }
}

/// WC3 patrol is two ground taps: the first marks the near point, the second
/// completes the route. `PatrolPlan` is the small state machine the touch path
/// (and the trace replay) drives after `P` arms patrol.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PatrolPlan {
    #[default]
    Idle,
    Armed,
    First(glam::Vec2),
}

impl PatrolPlan {
    pub fn arm(&mut self) {
        *self = Self::Armed;
    }

    /// Feed a ground tap. `None` after the first tap (the plan now waits for the
    /// second point), `Some((from, to))` when the route is complete.
    pub fn tap(&mut self, at: glam::Vec2) -> Option<(glam::Vec2, glam::Vec2)> {
        match *self {
            Self::Armed => {
                *self = Self::First(at);
                None
            }
            Self::First(from) => {
                *self = Self::Idle;
                Some((from, at))
            }
            Self::Idle => None,
        }
    }

    pub fn is_armed(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    pub fn cancel(&mut self) -> bool {
        let was_armed = self.is_armed();
        *self = Self::Idle;
        was_armed
    }
}
