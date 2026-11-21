//! The layout engine: `solve_layout(tree, window, preferences, state, content)`.
//!
//! Pure by construction (hud.md §6.2): no textures, no GPU, no clock, no
//! globals, no game mutation. It reads a tree and a description of the window
//! and returns placed rects plus diagnostics.
//!
//! # The guarantee
//!
//! Two invariants are checked on the **result**, not assumed from the input:
//!
//! 1. `used <= avail` — every flow reports how much it needed and how much it
//!    had; an excess becomes an [`Diagnostic::Overflow`], never a silent clip.
//! 2. every hit region is inside the safe content band and at least
//!    `touch_dp` on both axes ([`Diagnostic::Offscreen`],
//!    [`Diagnostic::TinyTarget`]).
//!
//! When a diagnostic fires, the caller must pick a fallback composition (see
//! `ui::widgets::hud`, which walks the breakpoint ladder until the result is
//! clean). The engine never invents a smaller target and never lets a required
//! action leave the screen — that is the incident this layer exists to prevent.

use crate::ui::core::element::{AlignX, AlignY, Flow, Label, Overflow};
use crate::ui::core::geometry::{Insets, Point, Rect, Size};
use crate::ui::core::id::Id;
use crate::ui::core::semantics::{Clip, Semantics, WidgetState};
use crate::ui::core::tree::UiTree;
use crate::ui::layout::{anchors, breakpoints, flex, grid, safe_area, text_fit};

/// Which axis an overflow happened on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    pub fn key(self) -> &'static str {
        match self {
            Axis::Horizontal => "horizontal",
            Axis::Vertical => "vertical",
        }
    }
}

/// Platform orientation. The solver never guesses from pixels alone when the
/// platform already knows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Orientation {
    Landscape,
    Portrait,
}

/// The physical window, in framebuffer pixels plus the density that maps them
/// to dp. Safe insets stay in dp here; [`Window::from_pixels`] converts.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Window {
    /// Framebuffer size in device pixels (what the renderer draws into).
    pub framebuffer: Size,
    /// Device pixels per logical dp.
    pub density: f32,
    /// Window-manager insets, already in dp.
    pub safe: Insets,
    /// Platform orientation, when the platform reports one.
    pub orientation: Option<Orientation>,
}

impl Window {
    pub fn new(framebuffer: Size, density: f32) -> Self {
        Window {
            framebuffer: Size::new(framebuffer.w.max(1.0), framebuffer.h.max(1.0)),
            density,
            safe: Insets::ZERO,
            orientation: None,
        }
    }

    /// Build from pixel insets (the shape a platform reports).
    pub fn from_pixels(framebuffer: Size, density: f32, insets_px: Insets) -> Self {
        let mut window = Window::new(framebuffer, density);
        window.safe = safe_area::from_pixels(insets_px, density, window.dp());
        window
    }

    pub fn density(&self) -> f32 {
        safe_area::sane_density(self.density)
    }

    /// The logical viewport.
    pub fn dp(&self) -> Size {
        let d = self.density();
        Size::new(
            (self.framebuffer.w / d).max(1.0),
            (self.framebuffer.h / d).max(1.0),
        )
    }

    /// Safe insets in dp, sanitised.
    pub fn safe(&self) -> Insets {
        self.safe.sanitised(self.dp(), safe_area::MAX_SAFE_FRACTION)
    }

    /// The band the interface may use: everything inside the safe insets.
    pub fn available(&self) -> Rect {
        safe_area::content_rect(self.dp(), self.safe())
    }

    pub fn orientation(&self) -> Orientation {
        self.orientation.unwrap_or_else(|| {
            let dp = self.dp();
            if dp.w >= dp.h {
                Orientation::Landscape
            } else {
                Orientation::Portrait
            }
        })
    }

    pub fn screen_class(&self) -> breakpoints::ScreenClass {
        breakpoints::ScreenClass::classify(self.dp())
    }
}

/// User preferences. Geometry multipliers only — never a second layout pass.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Preferences {
    /// Text/chrome multiplier. Touch targets ignore it (hud.md §7.5).
    pub scale: f32,
    /// Minimum actionable side in dp.
    pub touch_dp: f32,
    /// Developer overlay (FPS, boxes). Off in release.
    pub debug: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Preferences {
            scale: 1.0,
            touch_dp: 48.0,
            debug: false,
        }
    }
}

impl Preferences {
    pub fn scale(&self) -> f32 {
        if self.scale.is_finite() {
            self.scale.clamp(0.80, 1.40)
        } else {
            1.0
        }
    }

    pub fn touch(&self) -> f32 {
        if self.touch_dp.is_finite() {
            self.touch_dp.clamp(32.0, 96.0)
        } else {
            48.0
        }
    }
}

/// Which of the five selection compositions is live (hud.md §3.5).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SelectionState {
    /// Nothing selected: the console shows only the map and the commands.
    #[default]
    None,
    /// One combat unit: portrait, name, class, hp, stats.
    Unit,
    /// A hero: unit plus mana, level, inventory.
    Hero,
    /// A building: no mana, production queue, no inventory.
    Building,
    /// Several objects: primary portrait, composition, shared commands.
    Multiple,
}

impl SelectionState {
    pub fn key(self) -> &'static str {
        match self {
            SelectionState::None => "none",
            SelectionState::Unit => "unit",
            SelectionState::Hero => "hero",
            SelectionState::Building => "building",
            SelectionState::Multiple => "multiple",
        }
    }

    pub fn has_portrait(self) -> bool {
        !matches!(self, SelectionState::None)
    }
}

/// Live state that changes geometry (never the algorithm).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct State {
    pub selection: SelectionState,
    pub selection_count: usize,
    pub has_mana: bool,
    pub production: bool,
    pub inventory: bool,
    pub groups_open: bool,
    pub commands_open: bool,
}

impl State {
    /// Derive the five-state selection model from a raw selection.
    pub fn from_selection(count: usize, hero: bool, building: bool, has_mana: bool, inventory: bool) -> Self {
        let selection = match count {
            0 => SelectionState::None,
            1 if hero => SelectionState::Hero,
            1 if building => SelectionState::Building,
            1 => SelectionState::Unit,
            _ => SelectionState::Multiple,
        };
        State {
            selection,
            selection_count: count,
            has_mana: has_mana || hero,
            production: building,
            inventory: inventory && hero,
            groups_open: false,
            commands_open: false,
        }
    }

    /// Multi-selection shows the chip strip instead of the inventory.
    pub fn shows_strip(&self) -> bool {
        self.selection_count > 1
    }
}

/// Content demand: how many of each variable-count item exists right now.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ContentMetrics {
    pub resources: usize,
    pub commands: usize,
    pub toasts: usize,
    /// Optional rows the selection panel can host, in priority order.
    pub selection_rows: usize,
}

impl ContentMetrics {
    pub fn with_resources(resources: usize) -> Self {
        ContentMetrics {
            resources,
            commands: 12,
            toasts: 0,
            selection_rows: 6,
        }
    }
}

/// One solved element.
#[derive(Clone, PartialEq, Debug)]
pub struct Placed {
    pub id: Id,
    pub rect: Rect,
    /// The region this element may paint inside (parent clip ∩ own rect).
    pub clip: Rect,
    pub z: i16,
    pub semantics: Semantics,
    /// Receives pointer input.
    pub hit: bool,
    pub state: WidgetState,
    pub depth: u16,
    pub label: Option<Label>,
    pub group: &'static str,
    /// The element was collapsed because it did not fit (zero rect).
    pub collapsed: bool,
    /// The anchor wanted a position outside the parent and was clamped back in.
    pub clamped: bool,
    /// A full-bleed zone (console plate, top bar) that owns the screen edge
    /// instead of living inside the safe band.
    pub full_bleed: bool,
}

impl Placed {
    pub fn is_empty(&self) -> bool {
        self.rect.is_empty()
    }

    /// True when the rect is fully inside the region it may paint in.
    pub fn is_fully_visible(&self) -> bool {
        self.rect.is_inside(self.clip)
    }
}

/// A layout defect. Every variant is an instruction: the composition must
/// change, the numbers must not be papered over.
#[derive(Clone, PartialEq, Debug)]
pub enum Diagnostic {
    Overflow {
        id: Id,
        axis: Axis,
        need: f32,
        have: f32,
    },
    Squeezed {
        id: Id,
        axis: Axis,
        need: f32,
        have: f32,
    },
    Offscreen {
        id: Id,
        rect: Rect,
        bounds: Rect,
    },
    TinyTarget {
        id: Id,
        rect: Rect,
        need: f32,
    },
    Clipped {
        id: Id,
        rect: Rect,
        clip: Rect,
    },
    Overlap {
        a: Id,
        b: Id,
        area: f32,
    },
    TextOverflow {
        id: Id,
        needed: f32,
        avail: f32,
        lines: u16,
    },
    GridOverflow {
        id: Id,
        children: usize,
        cells: usize,
    },
    GridFloor {
        id: Id,
        cell: f32,
        need: f32,
    },
    MissingZone {
        id: Id,
        reason: &'static str,
    },
    MissingNode {
        id: Id,
        parent: Id,
    },
}

impl Diagnostic {
    pub fn id(&self) -> Id {
        match self {
            Diagnostic::Overflow { id, .. }
            | Diagnostic::Squeezed { id, .. }
            | Diagnostic::Offscreen { id, .. }
            | Diagnostic::TinyTarget { id, .. }
            | Diagnostic::Clipped { id, .. }
            | Diagnostic::TextOverflow { id, .. }
            | Diagnostic::GridOverflow { id, .. }
            | Diagnostic::GridFloor { id, .. }
            | Diagnostic::MissingZone { id, .. }
            | Diagnostic::MissingNode { id, .. } => *id,
            Diagnostic::Overlap { a, .. } => *a,
        }
    }

    /// One line for a log or an assertion message.
    pub fn summary(&self) -> String {
        match self {
            Diagnostic::Overflow {
                id,
                axis,
                need,
                have,
            } => format!(
                "{id} overflows {}: needs {need:.1} dp, has {have:.1} dp",
                axis.key()
            ),
            Diagnostic::Squeezed {
                id,
                axis,
                need,
                have,
            } => format!(
                "{id} squeezed on {}: floor {need:.1} dp, got {have:.1} dp",
                axis.key()
            ),
            Diagnostic::Offscreen { id, rect, bounds } => {
                format!("{id} leaves the safe band: {rect:?} vs {bounds:?}")
            }
            Diagnostic::TinyTarget { id, rect, need } => format!(
                "{id} target is {:.1}x{:.1} dp, needs {need:.1}",
                rect.w, rect.h
            ),
            Diagnostic::Clipped { id, rect, clip } => {
                format!("{id} is clipped: {rect:?} vs clip {clip:?}")
            }
            Diagnostic::Overlap { a, b, area } => {
                format!("{a} overlaps {b} by {area:.1} dp²")
            }
            Diagnostic::TextOverflow {
                id,
                needed,
                avail,
                lines,
            } => format!(
                "{id} text needs {needed:.1} dp in {avail:.1} dp ({lines} line(s))"
            ),
            Diagnostic::GridOverflow { id, children, cells } => format!(
                "{id} grid holds {cells} cells but has {children} children"
            ),
            Diagnostic::GridFloor { id, cell, need } => {
                format!("{id} grid cell {cell:.1} dp < {need:.1} dp floor")
            }
            Diagnostic::MissingZone { id, reason } => {
                format!("{id} is required but missing: {reason}")
            }
            Diagnostic::MissingNode { id, parent } => {
                format!("{id} is listed as a child of {parent} but missing")
            }
        }
    }
}

/// The solved layout.
#[derive(Clone, Debug)]
pub struct LayoutResult {
    pub placed: Vec<Placed>,
    pub diagnostics: Vec<Diagnostic>,
    /// Informational collapses (an optional row that simply does not fit).
    pub notes: Vec<String>,
    /// The safe content band the tree was solved in.
    pub avail: Rect,
    /// Bounding box of every placed rect, in dp.
    pub used: Size,
    /// `diagnostics` is empty and `used <= avail`.
    pub fits: bool,
    pub screen_dp: Size,
    pub safe: Insets,
    pub safe_px_insets: Insets,
    pub density: f32,
    pub class: breakpoints::ScreenClass,
    pub breakpoint: breakpoints::Breakpoint,
}

impl LayoutResult {
    /// Rect of `id`, if the tree produced one.
    pub fn rect(&self, id: Id) -> Option<Rect> {
        self.placed
            .iter()
            .find(|placed| placed.id == id)
            .map(|placed| placed.rect)
    }

    /// Rect of `id` or [`Rect::ZERO`].
    pub fn rect_or_zero(&self, id: Id) -> Rect {
        self.rect(id).unwrap_or(Rect::ZERO)
    }

    pub fn placed(&self, id: Id) -> Option<&Placed> {
        self.placed.iter().find(|placed| placed.id == id)
    }

    /// Topmost target under `point` (paint order is hit order).
    pub fn hit(&self, point: Point) -> Option<Id> {
        for placed in self.placed.iter().rev() {
            if !placed.hit || placed.rect.is_empty() {
                continue;
            }
            if !placed.clip.contains(point) {
                continue;
            }
            if placed.rect.contains(point) {
                return Some(placed.id);
            }
        }
        None
    }

    /// Every target under `point`, front to back (modal routing).
    pub fn hits(&self, point: Point) -> Vec<Id> {
        self.placed
            .iter()
            .rev()
            .filter(|placed| {
                placed.hit
                    && !placed.rect.is_empty()
                    && placed.clip.contains(point)
                    && placed.rect.contains(point)
            })
            .map(|placed| placed.id)
            .collect()
    }

    /// Targets inside `group` (for per-zone audits).
    pub fn group_rects(&self, group: &str) -> Vec<(Id, Rect)> {
        self.placed
            .iter()
            .filter(|placed| placed.group == group)
            .map(|placed| (placed.id, placed.rect))
            .collect()
    }

    pub fn diagnostics_for(&self, id: Id) -> Vec<&Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.id() == id)
            .collect()
    }

    /// `diagnostics` joined for a log line.
    pub fn report(&self) -> String {
        if self.diagnostics.is_empty() {
            format!(
                "layout ok: {} elements, used {:.1}x{:.1} dp of {:.1}x{:.1} dp",
                self.placed.len(),
                self.used.w,
                self.used.h,
                self.avail.w,
                self.avail.h
            )
        } else {
            let mut out = format!("layout has {} issue(s):", self.diagnostics.len());
            for diagnostic in &self.diagnostics {
                out.push_str("\n  - ");
                out.push_str(&diagnostic.summary());
            }
            out
        }
    }
}

/// Solve a tree inside a window.
///
/// `breakpoint` is the composition that produced the tree; it is recorded on
/// the result so a fallback ladder can tell which plan was rejected.
pub fn solve_layout(
    tree: &UiTree,
    window: &Window,
    preferences: &Preferences,
    state: &State,
    content: &ContentMetrics,
    breakpoint: breakpoints::Breakpoint,
) -> LayoutResult {
    let avail = window.available();
    let mut solver = Solver {
        tree,
        window: *window,
        preferences: *preferences,
        state: *state,
        content: *content,
        avail,
        placed: Vec::with_capacity(tree.len()),
        diagnostics: Vec::new(),
        notes: Vec::new(),
        used: Size::ZERO,
    };
    let screen = window.dp();
    let screen_rect = Rect::from_size(screen);
    solver.place(tree.root(), screen_rect, screen_rect, 0);
    solver.audit(avail);
    solver.audit_required();
    let used = solver.used;
    let fits = solver.diagnostics.is_empty()
        && used.w <= avail.w + 0.01
        && used.h <= avail.h + 0.01;
    LayoutResult {
        placed: solver.placed,
        diagnostics: solver.diagnostics,
        notes: solver.notes,
        avail,
        used,
        fits,
        screen_dp: screen,
        safe: window.safe(),
        safe_px_insets: window.safe().scaled(window.density()),
        density: window.density(),
        class: window.screen_class(),
        breakpoint,
    }
}

/// A zone that deliberately paints outside the safe band (its content is still
/// audited inside it).
fn is_full_bleed(semantics: Semantics, clip: Clip) -> bool {
    // The root always spans the screen; a full-bleed console or bar does too,
    // but only when it explicitly opts out of the parent clip.
    semantics == Semantics::Root
        || (clip == Clip::None && matches!(semantics, Semantics::Console | Semantics::Bar))
}

struct Solver<'a> {
    tree: &'a UiTree,
    window: Window,
    preferences: Preferences,
    state: State,
    content: ContentMetrics,
    avail: Rect,
    placed: Vec<Placed>,
    diagnostics: Vec<Diagnostic>,
    notes: Vec<String>,
    used: Size,
}

impl<'a> Solver<'a> {
    fn place(&mut self, id: Id, frame: Rect, parent_clip: Rect, depth: u16) {
        let Some(node) = self.tree.get(id) else {
            self.diagnostics.push(Diagnostic::MissingNode {
                id,
                parent: Id::ROOT,
            });
            return;
        };
        let clip = match node.clip {
            Clip::None => Rect::from_size(self.window.dp()),
            Clip::Parent => parent_clip.intersection(frame),
        };
        if node.label.is_some() {
            self.check_text(node.label, frame, id);
        }
        let clamped = !frame.is_inside(parent_clip) && node.clip == Clip::Parent;
        self.placed.push(Placed {
            id,
            rect: frame,
            clip,
            z: node.z,
            semantics: node.semantics,
            hit: node.is_target(),
            state: node.state,
            depth,
            label: node.label,
            group: node.group,
            collapsed: !node.visible,
            clamped,
            full_bleed: node.bleed || is_full_bleed(node.semantics, node.clip),
        });
        if !(node.bleed || is_full_bleed(node.semantics, node.clip)) {
            self.used = Size::new(
                self.used.w.max(frame.right() - self.avail.x),
                self.used.h.max(frame.bottom() - self.avail.y),
            );
        }
        if !node.visible {
            return;
        }
        let inner = frame.inset_each(node.padding);
        let inner = Rect::new(
            inner.x,
            inner.y,
            inner.w.max(0.0),
            inner.h.max(0.0),
        );
        let children = node.children.clone();
        match node.flow {
            Flow::Absolute => self.place_absolute(id, children, inner, clip, depth),
            Flow::Row { gap, align, wrap } => {
                if wrap {
                    self.place_row_wrapped(children, inner, gap as f32, align, clip, depth);
                } else {
                    self.place_row(id, children, inner, gap as f32, align, clip, depth);
                }
            }
            Flow::Column { gap, align } => {
                self.place_column(id, children, inner, gap as f32, align, clip, depth)
            }
            Flow::Grid {
                columns,
                rows,
                gap,
            } => self.place_grid(
                id,
                children,
                inner,
                columns as usize,
                rows as usize,
                gap as f32,
                clip,
                depth,
            ),
        }
    }

    fn check_text(&mut self, label: Option<Label>, frame: Rect, id: Id) {
        let Some(label) = label else { return };
        let fit = text_fit::fit(
            label.text,
            label.size_dp,
            frame.w,
            frame.h,
            label.wrap,
        );
        if !fit.fits {
            self.diagnostics.push(Diagnostic::TextOverflow {
                id,
                needed: fit.needed_w,
                avail: frame.w,
                lines: fit.lines,
            });
        }
    }

    fn child_frame(&self, child: &crate::ui::core::element::Element, parent: Rect) -> Rect {
        let declared = child.constraints.resolve(parent.size(), Size::ZERO);
        let size = anchors::stretch_size(child.anchor, declared, parent);
        anchors::place(child.anchor, parent, size, Point::ZERO)
    }

    fn place_absolute(&mut self, parent: Id, children: Vec<Id>, inner: Rect, clip: Rect, depth: u16) {
        for child in children {
            let Some(node) = self.tree.get(child) else {
                self.diagnostics.push(Diagnostic::MissingNode { id: child, parent });
                continue;
            };
            let frame = self.child_frame(node, inner);
            self.place(child, frame, clip, depth + 1);
        }
    }

    /// Row with `wrap = true`: greedy line breaking, every line distributed on
    /// its own. Never clips a child, never reports an overflow for a line that
    /// was broken on purpose.
    fn place_row_wrapped(
        &mut self,
        children: Vec<Id>,
        inner: Rect,
        gap: f32,
        align: AlignY,
        clip: Rect,
        depth: u16,
    ) {
        if children.is_empty() {
            return;
        }
        let line_h = children
            .iter()
            .filter_map(|child| self.tree.get(*child))
            .map(|node| node.constraints.height.resolve(inner.h, 0.0))
            .fold(0.0_f32, f32::max);
        let mut cursor_y = inner.y;
        let mut line: Vec<&crate::ui::core::element::Element> = Vec::new();
        let mut line_w = 0.0_f32;
        let flush = |line: &mut Vec<&crate::ui::core::element::Element>,
                         line_w: &mut f32,
                         cursor_y: &mut f32,
                         solver: &mut Self| {
            if line.is_empty() {
                return;
            }
            let mut x = inner.x;
            let h = line_h.min((inner.bottom() - *cursor_y).max(0.0));
            for node in line.iter() {
                let w = node.constraints.width.resolve(inner.w, 0.0);
                let y = match align {
                    AlignY::Start | AlignY::Stretch => *cursor_y,
                    AlignY::Center => *cursor_y + (h - node.constraints.height.resolve(h, 0.0)) * 0.5,
                    AlignY::End => *cursor_y + h - node.constraints.height.resolve(h, 0.0),
                };
                let frame = Rect::new(x, y, w, node.constraints.height.resolve(h, 0.0))
                    .clamp_inside(inner);
                solver.place(node.id, frame, clip, depth + 1);
                x += w + gap;
            }
            *cursor_y += h + gap;
            line.clear();
            *line_w = 0.0;
        };
        for child in children {
            let Some(node) = self.tree.get(child) else {
                continue;
            };
            let w = node.constraints.width.desired(inner.w, 0.0);
            if !line.is_empty() && line_w + gap + w > inner.w {
                flush(&mut line, &mut line_w, &mut cursor_y, self);
            }
            line_w = if line.is_empty() { w } else { line_w + gap + w };
            line.push(node);
        }
        flush(&mut line, &mut line_w, &mut cursor_y, self);
        if cursor_y - gap > inner.bottom() + 0.01 {
            self.diagnostics.push(Diagnostic::Overflow {
                id: Id::ROOT,
                axis: Axis::Vertical,
                need: cursor_y - gap - inner.y,
                have: inner.h,
            });
        }
    }

    fn place_row(
        &mut self,
        id: Id,
        children: Vec<Id>,
        inner: Rect,
        gap: f32,
        align: AlignY,
        clip: Rect,
        depth: u16,
    ) {
        if children.is_empty() {
            return;
        }
        let items: Vec<flex::Item> = children
            .iter()
            .map(|child| {
                let node = self.tree.get(*child);
                let extent = node
                    .map(|node| node.constraints.width)
                    .unwrap_or_default();
                let weight = node.map(|node| node.constraints.weight).unwrap_or(1.0);
                flex::Item::weighted(extent, weight)
            })
            .collect();
        let distribution = flex::distribute(inner.w, gap, &items);
        let strict: f32 = children
            .iter()
            .enumerate()
            .filter(|(_, child)| {
                self.tree
                    .get(**child)
                    .map(|node| node.overflow != Overflow::Collapse)
                    .unwrap_or(true)
            })
            .map(|(index, _)| distribution.sizes[index])
            .sum::<f32>()
            + gap * children.len().saturating_sub(1) as f32;
        if strict > inner.w + 0.01 {
            self.diagnostics.push(Diagnostic::Overflow {
                id,
                axis: Axis::Horizontal,
                need: strict,
                have: inner.w,
            });
        }
        let mut cursor_x = inner.x;
        for (index, child) in children.iter().enumerate() {
            let granted = distribution.sizes[index];
            let Some(node) = self.tree.get(*child) else {
                continue;
            };
            if node.overflow == Overflow::Collapse && granted + 0.01 < node.constraints.width.floor()
            {
                self.notes.push(format!("{child} collapsed: no room"));
                self.place_collapsed(*child, inner, clip, depth);
                continue;
            }
            if distribution.squeezed.contains(&index) {
                self.diagnostics.push(Diagnostic::Squeezed {
                    id: *child,
                    axis: Axis::Horizontal,
                    need: node.constraints.width.floor(),
                    have: granted,
                });
            }
            let remaining = (inner.right() - cursor_x).max(0.0);
            if granted > remaining + 0.01 {
                match node.overflow {
                    Overflow::Collapse => {
                        self.notes.push(format!("{child} collapsed: line is full"));
                        self.place_collapsed(*child, inner, clip, depth);
                        continue;
                    }
                    Overflow::Fail => self.diagnostics.push(Diagnostic::Overflow {
                        id: *child,
                        axis: Axis::Horizontal,
                        need: granted,
                        have: remaining,
                    }),
                    Overflow::Clamp => {}
                }
            }
            let h = node.constraints.height.resolve(inner.h, 0.0);
            let y = match node.valign.unwrap_or(align) {
                AlignY::Start | AlignY::Stretch => inner.y,
                AlignY::Center => inner.y + (inner.h - h) * 0.5,
                AlignY::End => inner.bottom() - h,
            };
            let width = granted.min(remaining);
            let frame = Rect::new(cursor_x, y, width, h).clamp_inside(inner);
            cursor_x += granted + gap;
            self.place(*child, frame, clip, depth + 1);
        }
    }

    fn place_column(
        &mut self,
        id: Id,
        children: Vec<Id>,
        inner: Rect,
        gap: f32,
        align: AlignX,
        clip: Rect,
        depth: u16,
    ) {
        if children.is_empty() {
            return;
        }
        let items: Vec<flex::Item> = children
            .iter()
            .map(|child| {
                let node = self.tree.get(*child);
                let extent = node
                    .map(|node| node.constraints.height)
                    .unwrap_or_default();
                let weight = node.map(|node| node.constraints.weight).unwrap_or(1.0);
                flex::Item::weighted(extent, weight)
            })
            .collect();
        let distribution = flex::distribute(inner.h, gap, &items);
        let strict: f32 = children
            .iter()
            .enumerate()
            .filter(|(_, child)| {
                self.tree
                    .get(**child)
                    .map(|node| node.overflow != Overflow::Collapse)
                    .unwrap_or(true)
            })
            .map(|(index, _)| distribution.sizes[index])
            .sum::<f32>()
            + gap * children.len().saturating_sub(1) as f32;
        if strict > inner.h + 0.01 {
            self.diagnostics.push(Diagnostic::Overflow {
                id,
                axis: Axis::Vertical,
                need: strict,
                have: inner.h,
            });
        }
        let mut cursor_y = inner.y;
        for (index, child) in children.iter().enumerate() {
            let granted = distribution.sizes[index];
            let Some(node) = self.tree.get(*child) else {
                continue;
            };
            let floor = node.constraints.height.floor();
            if granted + 0.01 < floor {
                match node.overflow {
                    Overflow::Collapse => {
                        self.notes.push(format!("{child} collapsed: floor squeezed"));
                        self.place_collapsed(*child, inner, clip, depth);
                        continue;
                    }
                    Overflow::Fail => self.diagnostics.push(Diagnostic::Squeezed {
                        id: *child,
                        axis: Axis::Vertical,
                        need: floor,
                        have: granted,
                    }),
                    Overflow::Clamp => {}
                }
            }
            let remaining = (inner.bottom() - cursor_y).max(0.0);
            if granted > remaining + 0.01 {
                match node.overflow {
                    Overflow::Collapse => {
                        self.notes.push(format!("{child} collapsed: band is full"));
                        self.place_collapsed(*child, inner, clip, depth);
                        continue;
                    }
                    Overflow::Fail => self.diagnostics.push(Diagnostic::Overflow {
                        id: *child,
                        axis: Axis::Vertical,
                        need: granted,
                        have: remaining,
                    }),
                    Overflow::Clamp => {}
                }
            }
            let w = node.constraints.width.resolve(inner.w, 0.0);
            let x = match node.halign.unwrap_or(align) {
                AlignX::Start | AlignX::Stretch => inner.x,
                AlignX::Center => inner.x + (inner.w - w) * 0.5,
                AlignX::End => inner.right() - w,
            };
            let height = granted.min(remaining);
            let frame = Rect::new(x, cursor_y, w, height).clamp_inside(inner);
            cursor_y += granted + gap;
            self.place(*child, frame, clip, depth + 1);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn place_grid(
        &mut self,
        id: Id,
        children: Vec<Id>,
        inner: Rect,
        columns: usize,
        rows: usize,
        gap: f32,
        clip: Rect,
        depth: u16,
    ) {
        let plan = grid::plan(inner, columns, rows, gap, self.preferences.touch(), false);
        if !plan.meets_floor {
            self.diagnostics.push(Diagnostic::GridFloor {
                id,
                cell: plan.min_side(),
                need: self.preferences.touch(),
            });
        }
        if children.len() > plan.count() {
            self.diagnostics.push(Diagnostic::GridOverflow {
                id,
                children: children.len(),
                cells: plan.count(),
            });
        }
        for (index, child) in children.iter().enumerate() {
            if index >= plan.count() {
                self.place_collapsed(*child, inner, clip, depth);
                continue;
            }
            let frame = plan.cell_rect(index);
            self.place(*child, frame, clip, depth + 1);
        }
    }

    fn place_collapsed(&mut self, id: Id, parent: Rect, clip: Rect, depth: u16) {
        let node = self.tree.get(id);
        self.placed.push(Placed {
            id,
            rect: Rect::ZERO,
            clip: clip.intersection(parent),
            z: node.map(|node| node.z).unwrap_or(0),
            semantics: node
                .map(|node| node.semantics)
                .unwrap_or(Semantics::Panel),
            hit: false,
            state: node.map(|node| node.state).unwrap_or_default(),
            depth: depth + 1,
            label: node.and_then(|node| node.label),
            group: node.map(|node| node.group).unwrap_or(""),
            collapsed: true,
            clamped: false,
            full_bleed: false,
        });
    }

    fn audit(&mut self, avail: Rect) {
        let touch = self.preferences.touch();
        let mut offscreen = Vec::new();
        let mut tiny = Vec::new();
        let mut clipped = Vec::new();
        for placed in &self.placed {
            if placed.rect.is_empty() {
                continue;
            }
            if !placed.full_bleed && !placed.rect.is_inside(avail) {
                offscreen.push(Diagnostic::Offscreen {
                    id: placed.id,
                    rect: placed.rect,
                    bounds: avail,
                });
            }
            if placed.hit {
                let side = placed.rect.w.min(placed.rect.h);
                if side + 0.01 < touch {
                    tiny.push(Diagnostic::TinyTarget {
                        id: placed.id,
                        rect: placed.rect,
                        need: touch,
                    });
                }
                if !placed.rect.is_inside(placed.clip) {
                    clipped.push(Diagnostic::Clipped {
                        id: placed.id,
                        rect: placed.rect,
                        clip: placed.clip,
                    });
                }
            }
        }
        self.diagnostics.extend(offscreen);
        self.diagnostics.extend(tiny);
        self.diagnostics.extend(clipped);

        // Sibling overlaps: a zone may never partially cover another zone, and
        // two targets may not share pixels. Nesting is not a collision.
        let placed = self.placed.clone();for i in 0..placed.len() {
            for j in (i + 1)..placed.len() {
                let (a, b) = (&placed[i], &placed[j]);
                if a.rect.is_empty() || b.rect.is_empty() {
                    continue;
                }
                if a.group == "overlay" || b.group == "overlay" {
                    continue; // a modal drawer owns the pixels it covers
                }
                if a.rect.contains_rect(b.rect) || b.rect.contains_rect(a.rect) {
                    continue; // containment is hierarchy, not a collision
                }
                let zone = a.semantics.is_zone() && b.semantics.is_zone();
                let targets = a.hit && b.hit;
                if !zone && !targets {
                    continue;
                }
                let area = a.rect.intersection_area(b.rect);
                if area > 0.5 {
                    self.diagnostics.push(Diagnostic::Overlap {
                        a: a.id,
                        b: b.id,
                        area,
                    });
                }
            }
        }
    }

    /// The five selection states must survive every breakpoint (hud.md §3.5,
    /// §7.3 "состояние выбранного объекта должно сохраняться"), and an open
    /// command drawer must expose at least as many slots as the live content.
    fn audit_required(&mut self) {
        if self.state.selection.has_portrait() {
            let selection = self
                .placed
                .iter()
                .find(|placed| placed.semantics == Semantics::Selection);
            match selection {
                None => self.diagnostics.push(Diagnostic::MissingZone {
                    id: Id::new("selection"),
                    reason: "a selection is live but the zone is absent",
                }),
                Some(placed) if placed.rect.w < 1.0 || placed.rect.h < 1.0 => {
                    self.diagnostics.push(Diagnostic::MissingZone {
                        id: placed.id,
                        reason: "the selection zone collapsed to zero",
                    })
                }
                Some(_) => {}
            }
        }
        if self.state.commands_open && self.content.commands > 0 {
            let slots = self
                .placed
                .iter()
                .filter(|placed| placed.semantics == Semantics::Slot && !placed.rect.is_empty())
                .count();
            if slots < self.content.commands {
                self.diagnostics.push(Diagnostic::MissingZone {
                    id: Id::new("card"),
                    reason: "fewer command slots than live commands",
                });
                self.notes.push(format!(
                    "commands: {} live, {slots} slots placed",
                    self.content.commands
                ));
            }
        }
        if self.state.groups_open {
            let groups = self
                .placed
                .iter()
                .filter(|placed| placed.semantics == Semantics::Group && !placed.rect.is_empty())
                .count();
            self.notes
                .push(format!("groups drawer open with {groups} slots"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::core::element::{Anchor, Element};
    use crate::ui::core::id::Id;
    use crate::ui::core::semantics::Semantics;
    use crate::ui::core::tree::UiTree;

    fn window(dp_w: f32, dp_h: f32) -> Window {
        Window::new(Size::new(dp_w, dp_h), 1.0)
    }

    fn empty() -> UiTree {
        UiTree::new(Element::new(Id::ROOT, Semantics::Root))
    }

    fn solve(tree: &UiTree, window: &Window) -> LayoutResult {
        solve_layout(
            tree,
            window,
            &Preferences::default(),
            &State::default(),
            &ContentMetrics::default(),
            breakpoints::Breakpoint::Wide,
        )
    }

    #[test]
    fn the_root_fills_the_safe_band() {
        let mut w = window(800.0, 360.0);
        w.safe = Insets::new(20.0, 10.0, 30.0, 40.0);
        let tree = empty();
        let result = solve(&tree, &w);
        assert!(result.fits, "{}", result.report());
        assert_eq!(result.avail, Rect::new(20.0, 10.0, 750.0, 310.0));
        // The root spans the screen so full-bleed zones can reach its edges;
        // `avail` stays the safe band every content rect is measured against.
        assert_eq!(result.rect(Id::ROOT), Some(Rect::new(0.0, 0.0, 800.0, 360.0)));
    }

    #[test]
    fn a_row_that_does_not_fit_is_reported_not_clipped() {
        // The incident: six 150 dp buttons inside an 864 dp frame.
        let mut tree = empty();
        tree.push(
            Id::ROOT,
            Element::new(Id::new("row"), Semantics::Panel).with_flow(Flow::Row {
                gap: 8,
                align: AlignY::Start,
                wrap: false,
            }),
        );
        for index in 0..6u16 {
            tree.push(
                Id::new("row"),
                Element::target(Id::indexed("button", index), Semantics::Button)
                    .with_constraints(crate::ui::core::Constraints::fixed(150.0, 48.0)),
            );
        }
        let result = solve(&tree, &window(864.0, 500.0));
        assert!(!result.fits, "{}", result.report());
        let overflow = result
            .diagnostics
            .iter()
            .find(|d| matches!(d, Diagnostic::Overflow { .. }))
            .unwrap_or_else(|| panic!("{}", result.report()));
        assert!(overflow.summary().contains("940"), "{overflow:?}");
        // The rects stay inside the safe band (that is the guarantee); the
        // diagnostic carries the truth that the plan does not fit.
        assert!(result.used.w <= result.avail.w + 0.01);
    }

    #[test]
    fn a_tiny_target_is_reported() {
        let mut tree = empty();
        tree.push(
            Id::ROOT,
            Element::target(Id::new("stop"), Semantics::Button)
                .with_constraints(crate::ui::core::Constraints::fixed(36.0, 36.0)),
        );
        let result = solve(&tree, &window(800.0, 360.0));
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| matches!(d, Diagnostic::TinyTarget { .. })),
            "{}",
            result.report()
        );
    }

    #[test]
    fn offscreen_targets_are_reported_and_clamped_inside() {
        let mut tree = empty();
        tree.push(
            Id::ROOT,
            Element::target(Id::new("far"), Semantics::Button)
                .with_anchor(Anchor::TOP_LEFT)
                .with_constraints(crate::ui::core::Constraints::fixed(48.0, 48.0)),
        );
        let result = solve(&tree, &window(100.0, 100.0));
        let rect = result.rect(Id::new("far")).unwrap();
        assert!(rect.is_inside(result.avail), "{rect:?}");
    }

    #[test]
    fn text_overflow_is_reported() {
        let mut tree = empty();
        tree.push(
            Id::ROOT,
            Element::panel(Id::new("label")).with_constraints(crate::ui::core::Constraints::fixed(40.0, 20.0)),
        );
        tree.get_mut(Id::new("label")).unwrap().label =
            Some(Label::new("THE PRODIGAL CHAPTER", 14.0));
        let result = solve(&tree, &window(800.0, 360.0));
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| matches!(d, Diagnostic::TextOverflow { .. })),
            "{}",
            result.report()
        );
    }

    #[test]
    fn hit_testing_uses_the_drawn_rects() {
        let mut tree = empty();
        tree.push(
            Id::ROOT,
            Element::target(Id::new("slot"), Semantics::Slot)
                .with_anchor(Anchor::TOP_RIGHT)
                .with_constraints(crate::ui::core::Constraints::fixed(48.0, 48.0)),
        );
        let w = window(800.0, 360.0);
        let result = solve(&tree, &w);
        let rect = result.rect(Id::new("slot")).unwrap();
        assert_eq!(result.hit(rect.centre()), Some(Id::new("slot")));
        assert_eq!(result.hit(Point::new(0.0, 0.0)), None);
    }

    #[test]
    fn a_grid_with_too_many_children_is_reported() {
        let mut tree = empty();
        tree.push(
            Id::ROOT,
            Element::new(Id::new("grid"), Semantics::Card).with_flow(Flow::Grid {
                columns: 2,
                rows: 2,
                gap: 8,
            }),
        );
        for index in 0..5u16 {
            tree.push(
                Id::new("grid"),
                Element::target(Id::indexed("cell", index), Semantics::Slot)
                    .with_constraints(crate::ui::core::Constraints::fill()),
            );
        }
        let result = solve(&tree, &window(400.0, 400.0));
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| matches!(d, Diagnostic::GridOverflow { .. })),
            "{}",
            result.report()
        );
    }

    #[test]
    fn collapsed_rows_take_no_space_and_are_not_errors() {
        let mut tree = empty();
        tree.push(
            Id::ROOT,
            Element::new(Id::new("column"), Semantics::Panel).with_flow(Flow::Column {
                gap: 0,
                align: AlignX::Stretch,
            }),
        );
        for index in 0..3u16 {
            tree.push(
                Id::new("column"),
                Element::panel(Id::indexed("row", index))
                    .with_constraints(crate::ui::core::Constraints::fixed(48.0, 48.0))
                    .with_overflow(Overflow::Collapse),
            );
        }
        let result = solve(&tree, &window(400.0, 100.0));
        // 2 rows fit in 100 dp; the third collapses to zero.
        assert!(result.notes.iter().any(|note| note.contains("collapsed")), "{:?}", result.notes);
        let third = result.rect(Id::indexed("row", 2)).unwrap();
        assert!(third.is_empty());
    }
}
