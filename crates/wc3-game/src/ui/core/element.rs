//! One element model for drawing *and* hit-testing (hud.md §6.3).
//!
//! An [`Element`] carries its identity, its semantics, how it is sized, where
//! it sits, how its children flow and what text it holds. The solver turns the
//! tree into [`Placed`](crate::ui::layout::Placed) rects; the renderer and the
//! input router both read those same rects. There is no second rectangle array
//! for clicks and no third index array mapping buttons back to slots.

use super::constraints::Constraints;
use super::geometry::{Insets, Point, Rect};
use super::id::Id;
use super::semantics::{Clip, HitMode, Semantics, WidgetState};

/// Horizontal placement inside the parent.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AlignX {
    #[default]
    Start,
    Center,
    End,
    /// Take the parent's full width.
    Stretch,
}

/// Vertical placement inside the parent.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AlignY {
    #[default]
    Start,
    Center,
    End,
    /// Take the parent's full height.
    Stretch,
}

/// Absolute placement inside the parent (used when the parent's flow is
/// [`Flow::Absolute`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Anchor {
    pub x: AlignX,
    pub y: AlignY,
}

impl Anchor {
    pub const TOP_LEFT: Anchor = Anchor {
        x: AlignX::Start,
        y: AlignY::Start,
    };
    pub const TOP_CENTER: Anchor = Anchor {
        x: AlignX::Center,
        y: AlignY::Start,
    };
    pub const TOP_RIGHT: Anchor = Anchor {
        x: AlignX::End,
        y: AlignY::Start,
    };
    pub const CENTER_LEFT: Anchor = Anchor {
        x: AlignX::Start,
        y: AlignY::Center,
    };
    pub const CENTER: Anchor = Anchor {
        x: AlignX::Center,
        y: AlignY::Center,
    };
    pub const CENTER_RIGHT: Anchor = Anchor {
        x: AlignX::End,
        y: AlignY::Center,
    };
    pub const BOTTOM_LEFT: Anchor = Anchor {
        x: AlignX::Start,
        y: AlignY::End,
    };
    pub const BOTTOM_CENTER: Anchor = Anchor {
        x: AlignX::Center,
        y: AlignY::End,
    };
    pub const BOTTOM_RIGHT: Anchor = Anchor {
        x: AlignX::End,
        y: AlignY::End,
    };
    pub const FILL: Anchor = Anchor {
        x: AlignX::Stretch,
        y: AlignY::Stretch,
    };
}

/// How children are arranged inside an element.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Flow {
    /// Children place themselves with their own [`Anchor`].
    #[default]
    Absolute,
    /// Left to right with a gap.
    Row { gap: u16, align: AlignY, wrap: bool },
    /// Top to bottom with a gap.
    Column { gap: u16, align: AlignX },
    /// Equal cells, row-major, `columns * rows` of them.
    Grid { columns: u8, rows: u8, gap: u16 },
}

/// What happens to a child that cannot get its minimum (a text row in a band
/// that is already full).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Overflow {
    /// Report a diagnostic: the plan is wrong and a fallback composition is due.
    #[default]
    Fail,
    /// Collapse the element to zero (an optional row that does not fit simply
    /// does not exist — never a clipped sliver).
    Collapse,
    /// Let the element take the remaining space and report `clamped` (rare;
    /// only for decorative fills).
    Clamp,
}

/// Text the solver must fit (§5.3 "текст не должен обрезаться").
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Label {
    /// The literal used for the width estimate and the audit dump.
    pub text: &'static str,
    /// Type size in dp.
    pub size_dp: f32,
    /// Numeric values are right-aligned so changing digit counts never jumps.
    pub numeric: bool,
    /// Whether the label may wrap onto a second line.
    pub wrap: bool,
}

impl Label {
    pub const fn new(text: &'static str, size_dp: f32) -> Self {
        Label {
            text,
            size_dp,
            numeric: false,
            wrap: false,
        }
    }

    pub const fn numeric(text: &'static str, size_dp: f32) -> Self {
        Label {
            text,
            size_dp,
            numeric: true,
            wrap: false,
        }
    }

    pub const fn wrapping(mut self) -> Self {
        self.wrap = true;
        self
    }
}

/// One node of the UI tree.
#[derive(Clone, PartialEq, Debug)]
pub struct Element {
    pub id: Id,
    pub semantics: Semantics,
    pub constraints: Constraints,
    pub anchor: Anchor,
    /// Per-child cross-axis override inside a [`Flow::Row`].
    pub valign: Option<AlignY>,
    /// Per-child cross-axis override inside a [`Flow::Column`].
    pub halign: Option<AlignX>,
    pub flow: Flow,
    /// Inset applied to children (not to this element's own rect).
    pub padding: Insets,
    /// Paint order inside the parent; higher draws later (and eats input first).
    pub z: i16,
    pub clip: Clip,
    /// A structural band that spans the screen edge on purpose (the top bar and
    /// the console inner band): its own rect may leave the safe area, its
    /// children are still audited inside it.
    pub bleed: bool,
    pub hit: HitMode,
    pub state: WidgetState,
    pub visible: bool,
    pub overflow: Overflow,
    pub label: Option<Label>,
    /// Diagnostic grouping: `"console"`, `"card"`, `"overlay"`. Overlays are
    /// modal and may cover the zone they belong to.
    pub group: &'static str,
    /// Set by [`UiTree`](super::tree::UiTree), not by the caller.
    pub parent: Id,
    /// Set by [`UiTree`](super::tree::UiTree), not by the caller.
    pub children: Vec<Id>,
}

impl Element {
    pub fn new(id: Id, semantics: Semantics) -> Self {
        Element {
            id,
            semantics,
            constraints: Constraints::fill(),
            anchor: Anchor::TOP_LEFT,
            valign: None,
            halign: None,
            flow: Flow::Absolute,
            padding: Insets::ZERO,
            z: 0,
            clip: Clip::Parent,
            bleed: false,
            hit: HitMode::None,
            state: WidgetState::Normal,
            visible: true,
            overflow: Overflow::Fail,
            label: None,
            group: "",
            parent: id,
            children: Vec::new(),
        }
    }

    /// A passive panel.
    pub fn panel(id: Id) -> Self {
        Self::new(id, Semantics::Panel)
    }

    /// An element that receives input.
    pub fn target(id: Id, semantics: Semantics) -> Self {
        let mut element = Self::new(id, semantics);
        element.hit = HitMode::Rect;
        element
    }

    pub const fn with_constraints(mut self, constraints: Constraints) -> Self {
        self.constraints = constraints;
        self
    }

    pub const fn with_anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    pub const fn with_valign(mut self, align: AlignY) -> Self {
        self.valign = Some(align);
        self
    }

    pub const fn with_halign(mut self, align: AlignX) -> Self {
        self.halign = Some(align);
        self
    }

    pub const fn with_flow(mut self, flow: Flow) -> Self {
        self.flow = flow;
        self
    }

    pub const fn with_padding(mut self, padding: Insets) -> Self {
        self.padding = padding;
        self
    }

    pub const fn with_z(mut self, z: i16) -> Self {
        self.z = z;
        self
    }

    pub const fn with_state(mut self, state: WidgetState) -> Self {
        self.state = state;
        self
    }

    pub const fn with_label(mut self, label: Label) -> Self {
        self.label = Some(label);
        self
    }

    pub const fn with_overflow(mut self, overflow: Overflow) -> Self {
        self.overflow = overflow;
        self
    }

    pub const fn in_group(mut self, group: &'static str) -> Self {
        self.group = group;
        self
    }

    pub const fn with_bleed(mut self) -> Self {
        self.bleed = true;
        self.transparent()
    }

    pub const fn transparent(mut self) -> Self {
        self.clip = Clip::None;
        self
    }

    /// Whether this element can receive a pointer this frame.
    pub fn is_target(&self) -> bool {
        self.visible && self.hit == HitMode::Rect && self.state.is_actionable()
    }

    /// The rect of this element inside `parent`, honouring anchor and size.
    pub fn anchored(&self, parent: Rect, size: super::geometry::Size) -> Rect {
        let x = match self.anchor.x {
            AlignX::Start => parent.x,
            AlignX::Center => parent.x + (parent.w - size.w) * 0.5,
            AlignX::End => parent.right() - size.w,
            AlignX::Stretch => parent.x,
        };
        let y = match self.anchor.y {
            AlignY::Start => parent.y,
            AlignY::Center => parent.y + (parent.h - size.h) * 0.5,
            AlignY::End => parent.bottom() - size.h,
            AlignY::Stretch => parent.y,
        };
        Rect::new(x, y, size.w, size.h)
    }

    /// Offset used by the solver for anchored children.
    pub fn anchored_at(&self, parent: Rect, size: super::geometry::Size, offset: Point) -> Rect {
        let rect = self.anchored(parent, size);
        rect.translate(offset.x, offset.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::core::geometry::Size;

    #[test]
    fn anchors_pin_all_nine_positions() {
        let parent = Rect::new(0.0, 0.0, 100.0, 50.0);
        let size = Size::new(20.0, 10.0);
        let at = |anchor: Anchor| Element::new(Id::new("x"), Semantics::Panel).with_anchor(anchor);
        assert_eq!(at(Anchor::TOP_LEFT).anchored(parent, size), Rect::new(0.0, 0.0, 20.0, 10.0));
        assert_eq!(at(Anchor::TOP_RIGHT).anchored(parent, size), Rect::new(80.0, 0.0, 20.0, 10.0));
        assert_eq!(at(Anchor::BOTTOM_LEFT).anchored(parent, size), Rect::new(0.0, 40.0, 20.0, 10.0));
        assert_eq!(at(Anchor::CENTER).anchored(parent, size), Rect::new(40.0, 20.0, 20.0, 10.0));
        assert_eq!(
            at(Anchor::BOTTOM_RIGHT).anchored(parent, size),
            Rect::new(80.0, 40.0, 20.0, 10.0)
        );
    }

    #[test]
    fn targets_require_visibility_and_an_actionable_state() {
        let mut e = Element::target(Id::new("stop"), Semantics::Button);
        assert!(e.is_target());
        e.state = WidgetState::Disabled;
        assert!(!e.is_target());
        e.state = WidgetState::Normal;
        e.visible = false;
        assert!(!e.is_target());
        assert!(!Element::panel(Id::new("plate")).is_target());
    }
}
