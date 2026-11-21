//! Semantic roles, clipping, widget state and hit-test policy.
//!
//! The semantic type is what lets one model serve three consumers at once
//! (hud.md §6.3): the renderer picks how to paint, the input router decides
//! what is a target, and the audit maps every rect onto the vocabulary of the
//! geometry auditor (`panel`, `button`, `label`, `icon`, `target`, `row`).

/// What an element *is* in the interface.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Semantics {
    /// Tree root / screen background.
    #[default]
    Root,
    /// A decorative or structural surface with no action of its own.
    Panel,
    /// The resource strip across the top.
    Bar,
    /// The bottom console plate.
    Console,
    /// The minimap frame (interactive: tap and drag move the camera).
    Minimap,
    /// The selected-object zone.
    Selection,
    /// The portrait plate inside the selection zone.
    Portrait,
    /// The command card panel.
    Card,
    /// One command slot inside the card.
    Slot,
    /// One control-group slot.
    Group,
    /// One inventory slot.
    Inventory,
    /// A push button (menu, dialog, drawer toggle).
    Button,
    /// Static text.
    Label,
    /// A right-aligned numeric value.
    Value,
    /// A progress/health/mana bar.
    Progress,
    /// An icon or portrait image.
    Icon,
    /// A tooltip panel.
    Tooltip,
    /// A transient notice.
    Toast,
    /// The contextual hint line.
    Hint,
    /// A 1 dp separator.
    Divider,
    /// A modal drawer (commands / groups).
    Drawer,
    /// A modal overlay layer.
    Overlay,
}

impl Semantics {
    /// Elements that dispatch an action and therefore owe the touch floor.
    pub fn is_interactive(self) -> bool {
        matches!(
            self,
            Semantics::Button
                | Semantics::Slot
                | Semantics::Group
                | Semantics::Inventory
                | Semantics::Minimap
                | Semantics::Drawer
        )
    }

    /// Zones that must not partially overlap a sibling (§7.3).
    pub fn is_zone(self) -> bool {
        matches!(
            self,
            Semantics::Bar
                | Semantics::Console
                | Semantics::Minimap
                | Semantics::Selection
                | Semantics::Card
                | Semantics::Drawer
                | Semantics::Overlay
                | Semantics::Tooltip
                | Semantics::Toast
                | Semantics::Hint
        )
    }

    /// Name used by `layout_audit.py` (`kind` in the dump).
    pub fn audit_kind(self) -> &'static str {
        match self {
            Semantics::Root => "panel",
            Semantics::Panel => "panel",
            Semantics::Bar => "row",
            Semantics::Console => "panel",
            Semantics::Minimap => "target",
            Semantics::Selection => "panel",
            Semantics::Portrait => "icon",
            Semantics::Card => "panel",
            Semantics::Slot => "button",
            Semantics::Group => "button",
            Semantics::Inventory => "button",
            Semantics::Button => "button",
            Semantics::Label => "label",
            Semantics::Value => "label",
            Semantics::Progress => "panel",
            Semantics::Icon => "icon",
            Semantics::Tooltip => "panel",
            Semantics::Toast => "panel",
            Semantics::Hint => "label",
            Semantics::Divider => "panel",
            Semantics::Drawer => "panel",
            Semantics::Overlay => "panel",
        }
    }
}

/// Clipping rule for an element's own quads and its children.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Clip {
    /// Not clipped (backgrounds that own the whole screen).
    None,
    /// Clip to the parent's clip region (§3.7 "правильный clipping").
    #[default]
    Parent,
}

/// Which visual state a widget is in (§5.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WidgetState {
    #[default]
    Normal,
    Hover,
    Pressed,
    Focused,
    Selected,
    Disabled,
    Loading,
    Error,
}

impl WidgetState {
    pub fn key(self) -> &'static str {
        match self {
            WidgetState::Normal => "normal",
            WidgetState::Hover => "hover",
            WidgetState::Pressed => "pressed",
            WidgetState::Focused => "focused",
            WidgetState::Selected => "selected",
            WidgetState::Disabled => "disabled",
            WidgetState::Loading => "loading",
            WidgetState::Error => "error",
        }
    }

    /// Disabled/error/loading slots still occupy space but never dispatch.
    pub fn is_actionable(self) -> bool {
        !matches!(
            self,
            WidgetState::Disabled | WidgetState::Loading | WidgetState::Error
        )
    }
}

/// Whether the element receives pointer input.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum HitMode {
    /// Not a target.
    #[default]
    None,
    /// The element's own rect is the hit region.
    Rect,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_roles_owe_the_touch_floor() {
        for role in [
            Semantics::Button,
            Semantics::Slot,
            Semantics::Group,
            Semantics::Inventory,
            Semantics::Minimap,
        ] {
            assert!(role.is_interactive(), "{role:?}");
        }
        assert!(!Semantics::Label.is_interactive());
        assert!(!Semantics::Card.is_interactive());
    }

    #[test]
    fn audit_vocabulary_matches_the_geometry_auditor() {
        // layout_audit.py knows: panel, button, label, icon, tab, target, row.
        let known = ["panel", "button", "label", "icon", "tab", "target", "row"];
        for role in [
            Semantics::Root,
            Semantics::Panel,
            Semantics::Bar,
            Semantics::Console,
            Semantics::Minimap,
            Semantics::Selection,
            Semantics::Portrait,
            Semantics::Card,
            Semantics::Slot,
            Semantics::Group,
            Semantics::Inventory,
            Semantics::Button,
            Semantics::Label,
            Semantics::Value,
            Semantics::Progress,
            Semantics::Icon,
            Semantics::Tooltip,
            Semantics::Toast,
            Semantics::Hint,
            Semantics::Divider,
            Semantics::Drawer,
            Semantics::Overlay,
        ] {
            assert!(known.contains(&role.audit_kind()), "{role:?}");
        }
    }

    #[test]
    fn disabled_widgets_never_dispatch() {
        assert!(!WidgetState::Disabled.is_actionable());
        assert!(!WidgetState::Error.is_actionable());
        assert!(WidgetState::Pressed.is_actionable());
    }
}
