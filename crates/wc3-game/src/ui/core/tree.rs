//! The element tree.
//!
//! Nodes are stored in insertion order and referenced by [`Id`]; each node owns
//! its child list and its parent link. The solver walks depth-first from the
//! root, so paint order and hit order are both "parent, then children in
//! declaration order, then higher `z`" — one order, derived from one structure.

use super::element::Element;
use super::id::Id;

/// A tree of UI elements.
#[derive(Clone, Debug)]
pub struct UiTree {
    nodes: Vec<Element>,
    root: Id,
}

impl UiTree {
    /// Start a tree with its root element.
    pub fn new(mut root: Element) -> Self {
        let root_id = root.id;
        root.parent = root_id;
        UiTree {
            nodes: vec![root],
            root: root_id,
        }
    }

    /// Root id (always [`Id::ROOT`] for the HUD composition).
    pub fn root(&self) -> Id {
        self.root
    }

    /// Append `element` as the last child of `parent`.
    ///
    /// A duplicate id replaces the earlier node in place, so a rebuilt tree can
    /// never end up with two nodes that the renderer and the input router
    /// disagree about.
    pub fn add(&mut self, parent: Id, mut element: Element) -> Id {
        let id = element.id;
        if let Some(slot) = self.nodes.iter_mut().find(|node| node.id == id) {
            // A rebuilt node keeps its place in the hierarchy: identity is the
            // id, and the tree (not the caller) owns the links.
            element.parent = slot.parent;
            element.children = std::mem::take(&mut slot.children);
            *slot = element;
            return id;
        }
        element.parent = parent;
        self.nodes.push(element);
        if let Some(host) = self.nodes.iter_mut().find(|node| node.id == parent) {
            host.children.push(id);
        }
        id
    }

    /// Convenience alias used by composition code.
    pub fn push(&mut self, parent: Id, element: Element) {
        self.add(parent, element);
    }

    pub fn get(&self, id: Id) -> Option<&Element> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn get_mut(&mut self, id: Id) -> Option<&mut Element> {
        self.nodes.iter_mut().find(|node| node.id == id)
    }

    /// Depth-first visit: `(element, depth)`, parents before children.
    pub fn walk(&self) -> Vec<(&Element, u16)> {
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut stack: Vec<(Id, u16)> = vec![(self.root, 0)];
        while let Some((id, depth)) = stack.pop() {
            if let Some(node) = self.get(id) {
                out.push((node, depth));
                for child in node.children.iter().rev() {
                    stack.push((*child, depth + 1));
                }
            }
        }
        out
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Declared children of `parent`, in order.
    pub fn children(&self, parent: Id) -> Vec<Id> {
        self.get(parent)
            .map(|node| node.children.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::core::element::{AlignY, Element, Flow};
    use crate::ui::core::semantics::Semantics;

    fn tree() -> UiTree {
        let mut tree = UiTree::new(Element::new(Id::ROOT, Semantics::Root));
        tree.push(
            Id::ROOT,
            Element::new(Id::new("console"), Semantics::Console).with_flow(Flow::Row {
                gap: 8,
                align: AlignY::Center,
                wrap: false,
            }),
        );
        tree.push(Id::new("console"), Element::new(Id::new("card"), Semantics::Card));
        tree.push(Id::new("console"), Element::new(Id::new("minimap"), Semantics::Minimap));
        tree
    }

    #[test]
    fn children_are_listed_in_declaration_order() {
        let tree = tree();
        let names: Vec<&str> = tree
            .children(Id::new("console"))
            .iter()
            .map(|id| id.name())
            .collect();
        assert_eq!(names, ["card", "minimap"]);
        assert_eq!(tree.children(Id::ROOT).len(), 1);
    }

    #[test]
    fn walk_is_depth_first_and_parents_come_first() {
        let tree = tree();
        let order: Vec<&str> = tree.walk().iter().map(|(node, _)| node.id.name()).collect();
        assert_eq!(order, ["root", "console", "card", "minimap"]);
        assert_eq!(tree.len(), 4);
    }

    #[test]
    fn duplicate_ids_replace_in_place() {
        let mut tree = tree();
        let before = tree.len();
        tree.push(Id::ROOT, Element::new(Id::new("card"), Semantics::Card));
        assert_eq!(tree.len(), before, "no duplicate node");
        assert_eq!(tree.get(Id::new("card")).unwrap().semantics, Semantics::Card);
        // The replaced node keeps its declared parent link.
        assert_eq!(tree.get(Id::new("card")).unwrap().parent, Id::new("console"));
    }
}
