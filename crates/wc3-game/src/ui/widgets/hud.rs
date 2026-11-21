//! HUD composition (hud.md §3.1–3.6, §3.8) and the fallback ladder (§7.3).
//!
//! The composition is declarative: zones are elements with constraints and the
//! solver places them. If the solved result reports a defect — an overflow, a
//! target under 48 dp, a grid cell under the floor, an off-screen band — the
//! ladder moves to the next composition (wide → compact → minimal, portrait →
//! portrait with a minimap rail) instead of drawing a clipped frame.
//!
//! Panel interiors (the selection rows, the card grid, the tray) are solved by
//! the same primitives and then audited against the zone they live in.

use crate::ui::core::constraints::Constraints;
use crate::ui::core::element::{AlignX, AlignY, Anchor, Element, Flow, Overflow};
use crate::ui::core::geometry::{Insets, Rect, Size};
use crate::ui::core::id::Id;
use crate::ui::core::semantics::Semantics;
use crate::ui::core::tree::UiTree;
use crate::ui::layout::breakpoints::Breakpoint;
use crate::ui::layout::engine::{
    solve_layout, ContentMetrics, Diagnostic, LayoutResult, Preferences, State, Window,
};
use crate::ui::layout::{grid, safe_area};
use crate::ui::theme::Theme;

/// Widest health/mana trough inside the selection card (a unit-card bar, never
/// a line across the console).
pub const HP_BAR_MAX_DP: f32 = 176.0;

/// The composition that produced a solution.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Plan {
    /// Every zone inline (desktop / tablet).
    Wide,
    /// Minimap + selection band; the card and the tray are drawers.
    Compact,
    /// Playfield first: minimap rail, selection strip, both drawers.
    Minimal,
    /// Portrait: summary strip on top, minimap + card row below.
    Portrait,
    /// Narrow portrait: the minimap collapses to a 48 dp control.
    PortraitNarrow,
}

impl Plan {
    pub fn key(self) -> &'static str {
        match self {
            Plan::Wide => "wide",
            Plan::Compact => "compact",
            Plan::Minimal => "minimal",
            Plan::Portrait => "portrait",
            Plan::PortraitNarrow => "portrait-narrow",
        }
    }

    pub fn is_landscape(self) -> bool {
        matches!(self, Plan::Wide | Plan::Compact | Plan::Minimal)
    }

    /// The breakpoint this plan belongs to.
    pub fn breakpoint(self) -> Breakpoint {
        match self {
            Plan::Wide => Breakpoint::Wide,
            Plan::Compact => Breakpoint::Standard,
            Plan::Minimal => Breakpoint::Minimal,
            Plan::Portrait => Breakpoint::Standard,
            Plan::PortraitNarrow => Breakpoint::Compact,
        }
    }

    /// Next composition to try when this one does not fit.
    pub fn fallback(self) -> Option<Plan> {
        match self {
            Plan::Wide => Some(Plan::Compact),
            Plan::Compact => Some(Plan::Minimal),
            Plan::Minimal => None,
            Plan::Portrait => Some(Plan::PortraitNarrow),
            Plan::PortraitNarrow => None,
        }
    }
}

/// Element ids of the composition, in one place so the adapter and the audit
/// cannot drift apart.
pub mod ids {
    use crate::ui::core::id::Id;

    pub const TOP: Id = Id::new("top_bar");
    pub const MAP_NAME: Id = Id::new("map_name");
    pub const UPKEEP: Id = Id::new("upkeep");
    pub const FPS: Id = Id::new("fps");
    pub const RESOURCES: Id = Id::new("resources");
    pub const PILL: Id = Id::new("resource_pill");
    pub const CONSOLE: Id = Id::new("console");
    pub const TOP_CONTENT: Id = Id::new("top_content");
    pub const CONSOLE_CONTENT: Id = Id::new("console_content");
    pub const SUMMARY: Id = Id::new("summary");
    pub const VIEWPORT: Id = Id::new("viewport");
    pub const MINIMAP_HEADER: Id = Id::new("minimap_header");
    pub const MINIMAP_FRAME: Id = Id::new("minimap_frame");
    pub const MINIMAP_IMAGE: Id = Id::new("minimap_image");
    pub const MINIMAP_RAIL: Id = Id::new("minimap_rail");
    pub const SELECTION: Id = Id::new("selection");
    pub const MINIMAP_COLUMN: Id = Id::new("minimap_column");
    pub const PORTRAIT: Id = Id::new("portrait");
    pub const INFO: Id = Id::new("info");
    pub const NAME: Id = Id::new("name");
    pub const CLASS: Id = Id::new("class");
    pub const HP: Id = Id::new("hp_bar");
    pub const MANA: Id = Id::new("mana_bar");
    pub const STATS: Id = Id::new("stats");
    pub const PROD_LABEL: Id = Id::new("production_label");
    pub const PROD_BAR: Id = Id::new("production_bar");
    pub const ARMY: Id = Id::new("army");
    pub const ARMY_SLOT: Id = Id::new("army_slot");
    pub const ARMY_BADGE: Id = Id::new("army_badge");
    pub const INVENTORY: Id = Id::new("inventory");
    pub const INV_SLOT: Id = Id::new("inv_slot");
    pub const GROUPS: Id = Id::new("groups");
    pub const TRAY: Id = Id::new("tray");
    pub const GROUP_SLOT: Id = Id::new("group_slot");
    pub const GROUPS_BUTTON: Id = Id::new("groups_button");
    pub const GROUPS_OVERLAY: Id = Id::new("groups_overlay");
    pub const CARD: Id = Id::new("card");
    pub const CARD_SLOT: Id = Id::new("card_slot");
    pub const COMMANDS_BUTTON: Id = Id::new("commands_button");
    pub const COMMANDS_OVERLAY: Id = Id::new("commands_overlay");
    pub const TOAST: Id = Id::new("toast");
    pub const HINT: Id = Id::new("hint");
    pub const TOOLTIP: Id = Id::new("tooltip");
    pub const PHASE: Id = Id::new("phase");
    pub const MODE: Id = Id::new("mode");
}

/// Every rect the HUD draws or hit-tests, in dp.
#[derive(Clone, Debug, Default)]
pub struct HudRects {
    pub screen: Rect,
    pub safe: Insets,
    pub top_bar: Rect,
    pub map_name: Rect,
    pub upkeep: Rect,
    pub fps: Rect,
    pub resources: Rect,
    pub resource_pills: Vec<Rect>,
    pub console: Rect,
    pub console_h: f32,
    pub content: Rect,
    pub viewport: Rect,
    pub minimap_header: Rect,
    pub minimap_frame: Rect,
    pub minimap_image: Rect,
    pub minimap_rail: Rect,
    pub selection: Rect,
    pub portrait: Rect,
    pub name: Rect,
    pub class: Rect,
    pub hp_bar: Rect,
    pub hp_text: Rect,
    pub mana_bar: Rect,
    pub mana_text: Rect,
    pub stats: Rect,
    pub production_label: Rect,
    pub production_bar: Rect,
    pub army: Rect,
    pub army_slots: Vec<Rect>,
    pub army_badge: Rect,
    pub inventory: Rect,
    pub inventory_slots: Vec<Rect>,
    pub groups_area: Rect,
    pub group_slots: Vec<Rect>,
    pub groups_button: Rect,
    pub groups_overlay: Rect,
    pub card: Rect,
    pub card_slots: Vec<Rect>,
    pub commands_button: Rect,
    pub commands_overlay: Rect,
    pub toast: Rect,
    pub toasts: Vec<Rect>,
    pub hint: Rect,
    pub tooltip: Rect,
    pub phase: Rect,
    pub mode_rect: Rect,
    pub separators: Vec<Rect>,
}

impl HudRects {
    /// Zones that must not partially overlap (§7.3).
    pub fn zones(&self) -> Vec<(&'static str, Rect)> {
        let mut zones = vec![
            ("top_bar", self.top_bar),
            ("minimap", self.minimap_frame),
            ("selection", self.selection),
            ("card", self.card),
            ("groups", self.groups_area),
            ("tooltip", self.tooltip),
        ];
        if self.minimap_rail.w > 0.0 {
            zones.push(("minimap_rail", self.minimap_rail));
        }
        if self.groups_button.w > 0.0 {
            zones.push(("groups_button", self.groups_button));
        }
        if self.commands_button.w > 0.0 {
            zones.push(("commands_button", self.commands_button));
        }
        for (index, toast) in self.toasts.iter().enumerate() {
            zones.push((
                match index {
                    0 => "toast",
                    1 => "toast2",
                    _ => "toast3",
                },
                *toast,
            ));
        }
        zones
    }

    /// Every rect that dispatches an action; all must be at least 48 dp.
    pub fn interactive(&self) -> Vec<(&'static str, Rect)> {
        let mut rects: Vec<(&'static str, Rect)> = Vec::new();
        for slot in &self.card_slots {
            rects.push(("card", *slot));
        }
        for slot in &self.group_slots {
            rects.push(("group", *slot));
        }
        for slot in &self.army_slots {
            rects.push(("army", *slot));
        }
        for slot in &self.inventory_slots {
            rects.push(("inventory", *slot));
        }
        for (name, rect) in [
            ("minimap", self.minimap_frame),
            ("minimap_rail", self.minimap_rail),
            ("groups_button", self.groups_button),
            ("commands_button", self.commands_button),
        ] {
            if rect.w > 0.0 && rect.h > 0.0 {
                rects.push((name, rect));
            }
        }
        rects.retain(|(_, rect)| rect.w > 0.0 && rect.h > 0.0);
        rects
    }
}

/// One rung of the ladder.
#[derive(Clone, Debug)]
pub struct Attempt {
    pub plan: Plan,
    pub diagnostics: usize,
    pub fits: bool,
    /// The first few defects of this rung (why it was rejected).
    pub problems: Vec<String>,
}

/// The solved HUD.
#[derive(Clone, Debug)]
pub struct HudSolution {
    pub plan: Plan,
    pub rects: HudRects,
    pub layout: LayoutResult,
    /// Defects that force a fallback (empty on the accepted plan).
    pub diagnostics: Vec<Diagnostic>,
    /// Labels the renderer must ellipsize (soft: the plan is still usable).
    pub text_overflows: Vec<Diagnostic>,
    pub fits: bool,
    pub attempts: Vec<Attempt>,
}

impl HudSolution {
    /// One-line proof for a log or the report.
    pub fn report(&self) -> String {
        format!(
            "HUD plan={} fits={} elements={} used={:.1}x{:.1} of {:.1}x{:.1} dp diagnostics={} text={} ladder={:?}",
            self.plan.key(),
            self.fits,
            self.layout.placed.len(),
            self.layout.used.w,
            self.layout.used.h,
            self.layout.avail.w,
            self.layout.avail.h,
            self.diagnostics.len(),
            self.text_overflows.len(),
            self.attempts
                .iter()
                .map(|attempt| (attempt.plan.key(), attempt.diagnostics))
                .collect::<Vec<_>>()
        )
    }

    /// Hard diagnostics as text.
    pub fn problems(&self) -> Vec<String> {
        self.diagnostics
            .iter()
            .map(|diagnostic| diagnostic.summary())
            .collect()
    }
}

/// Solve the HUD, walking the fallback ladder until a composition fits.
pub fn solve_hud(
    window: &Window,
    preferences: &Preferences,
    state: &State,
    content: &ContentMetrics,
    theme: &Theme,
) -> HudSolution {
    let mut attempts = Vec::new();
    let mut best: Option<HudSolution> = None;
    let mut plan = first_plan(window, theme);
    loop {
        let built = build(plan, window, preferences, state, content, theme);
        let mut layout = solve_layout(
            &built.tree,
            window,
            preferences,
            state,
            content,
            plan.breakpoint(),
        );
        // A band that does not fit the safe area is a hard defect even though
        // it is computed from solved zones.
        layout.diagnostics.extend(built.extra_diagnostics);
        let hard: Vec<Diagnostic> = layout
            .diagnostics
            .iter()
            .filter(|diagnostic| !matches!(diagnostic, Diagnostic::TextOverflow { .. }))
            .cloned()
            .collect();
        let text: Vec<Diagnostic> = layout
            .diagnostics
            .iter()
            .filter(|diagnostic| matches!(diagnostic, Diagnostic::TextOverflow { .. }))
            .cloned()
            .collect();
        let fits = hard.is_empty()
            && layout.used.w <= layout.avail.w + 0.01
            && layout.used.h <= layout.avail.h + 0.01;
        attempts.push(Attempt {
            plan,
            diagnostics: hard.len(),
            fits,
            problems: hard.iter().take(4).map(|d| d.summary()).collect(),
        });
        let effective = built.effective;
        let solution = HudSolution {
            plan: effective,
            rects: built.rects,
            layout,
            diagnostics: hard,
            text_overflows: text,
            fits,
            attempts: attempts.clone(),
        };
        let better = best
            .as_ref()
            .map(|current| solution.diagnostics.len() < current.diagnostics.len())
            .unwrap_or(true);
        if better {
            best = Some(solution);
        }
        if fits {
            break;
        }
        match plan.fallback() {
            Some(next) => plan = next,
            None => break,
        }
    }
    best.expect("at least one attempt")
}

/// The first composition for this window.
pub fn first_plan(window: &Window, theme: &Theme) -> Plan {
    if window.orientation() == crate::ui::layout::Orientation::Portrait {
        return Plan::Portrait;
    }
    let content_w = content_width(window, theme);
    if content_w >= theme.metrics.wide_min_content {
        Plan::Wide
    } else {
        Plan::Compact
    }
}

/// Safe content width in dp (the number breakpoints are measured on).
pub fn content_width(window: &Window, theme: &Theme) -> f32 {
    let dp = window.dp();
    let safe = window.safe();
    (dp.w - safe.horizontal() - 2.0 * theme.metrics.pad).max(1.0)
}

struct Built {
    tree: UiTree,
    rects: HudRects,
    extra_diagnostics: Vec<Diagnostic>,
    /// The composition actually built: a portrait band that cannot host the
    /// inline card degrades to [`Plan::PortraitNarrow`] while building.
    effective: Plan,
}

fn build(
    plan: Plan,
    window: &Window,
    preferences: &Preferences,
    state: &State,
    content: &ContentMetrics,
    theme: &Theme,
) -> Built {
    let metrics = theme.metrics;
    let screen = window.dp();
    let safe = window.safe();
    let avail = window.available();
    let pad = metrics.pad;
    let gap = metrics.gap;
    let portrait = !plan.is_landscape();
    // The bar plate spans the screen edge; its content lives below the status
    // bar, so the safe top inset grows the plate instead of being ignored.
    let bar_h = if portrait {
        metrics.top_bar_portrait
    } else {
        metrics.top_bar
    };
    let top_h = bar_h + safe.top;
    let top_bar = Rect::new(0.0, 0.0, screen.w, top_h);
    let bottom_extra = (safe.bottom - pad).max(0.0);
    let content_w = (screen.w - safe.left - safe.right - 2.0 * pad).max(1.0);
    let (console_h, console) = match plan {
        Plan::Wide => {
            let base = (0.34 * screen.h).clamp(metrics.console_min, metrics.console_max);
            let h = base + bottom_extra;
            (h, Rect::new(0.0, screen.h - h, screen.w, h))
        }
        Plan::Compact | Plan::Minimal => {
            let base = (0.42 * screen.h)
                .clamp(metrics.console_compact_min, metrics.console_compact_max);
            // The band may grow to the HUD ceiling when the selection card and
            // its chip strip cannot both fit: an unreadable card is worse than
            // 5% of playfield, and the ladder still owns the hard limits. The
            // ceiling is on the *total* console (`bottom_extra` included):
            // adding it after the clamp let a 1280x720@3 session reach 59.2 %
            // against the 50 % contract (HUD-GAME-003).
            let selection_need = 2.0 * metrics.pad + 26.0 + 18.0 + metrics.touch + 4.0;
            let ceiling = (screen.h * metrics.hud_max_fraction - bottom_extra).max(1.0);
            let grown = base.max(selection_need).min(ceiling);
            let h = grown + bottom_extra;
            (h, Rect::new(0.0, screen.h - h, screen.w, h))
        }
        Plan::Portrait => {
            let h = metrics.portrait_summary + gap + metrics.card_h + 2.0 * pad + bottom_extra;
            (h, Rect::new(0.0, screen.h - h, screen.w, h))
        }
        Plan::PortraitNarrow => {
            let h = metrics.portrait_summary
                + gap
                + metrics.touch
                + 2.0 * pad
                + bottom_extra;
            (h, Rect::new(0.0, screen.h - h, screen.w, h))
        }
    };
    // The 50% contract is a normal-phone rule. Enforce it on the total band
    // (console + top bar) for screens tall enough to host the card; the 240 dp
    // degenerate class keeps its documented 60/62% budget above.
    let max_console = (screen.h * metrics.hud_max_fraction - top_bar.h).max(1.0);
    let (console_h, console) = if screen.h > 260.0 && console.h > max_console {
        (
            max_console,
            Rect::new(0.0, screen.h - max_console, screen.w, max_console),
        )
    } else {
        (console_h, console)
    };
    let content_bottom = screen.h - safe.bottom.max(pad);
    let content_rect = Rect::new(
        safe.left + pad,
        console.y + pad,
        content_w,
        (content_bottom - (console.y + pad)).max(1.0),
    );
    let viewport = Rect::new(
        0.0,
        top_bar.bottom(),
        screen.w,
        (console.y - top_bar.bottom()).max(1.0),
    );

    let mut tree = UiTree::new(
        Element::new(Id::ROOT, Semantics::Root)
            .with_anchor(Anchor::FILL)
            .with_constraints(Constraints::fill())
            .transparent(),
    );
    let mut extra_diagnostics = Vec::new();
    let mut effective = plan;

    // ---------------------------------------------------------------- top bar
    tree.push(Id::ROOT, Element::new(ids::TOP, Semantics::Bar)
        .with_anchor(Anchor::TOP_LEFT)
        .with_constraints(Constraints {
            width: crate::ui::core::Extent::fill(),
            height: crate::ui::core::Extent::fixed(top_h),
            ..Constraints::fill()
        })
        .transparent());
    let pill_h = (bar_h - 8.0).clamp(18.0, 24.0);
    let pill_gap = 4.0 * theme.type_scale.scale_30(crate::ui::theme::TypeRole::Numeric, 1.0) / 1.0;
    let _ = pill_gap;
    let pill_gap = 4.0;
    let upkeep_w = if portrait {
        (72.0 * preferences.scale()).clamp(56.0, 84.0)
    } else {
        (58.0 * preferences.scale()).clamp(46.0, 68.0)
    };
    let fps_w = 52.0;
    let top_left = safe.left + (bar_h * 1.6).max(24.0);
    let top_right = safe.right + pad * 0.5;
    let row_w = (screen.w - top_left - top_right).max(1.0);
    let budget =
        row_w - upkeep_w - if preferences.debug { fps_w + gap } else { 0.0 } - gap * 2.0;
    // Numeric pills keep a 40 dp floor (the 45 px numeric token needs ~37 dp for
    // four digits). When the bar cannot hold three of them the strip shows two
    // instead of overflowing — the map name is already the first to go.
    let pill_count = if budget + pill_gap >= 3.0 * 40.0 + 2.0 * pill_gap {
        3
    } else if budget + pill_gap >= 2.0 * 40.0 + pill_gap {
        2
    } else {
        1
    };
    let pill_w = (((budget) - (pill_count - 1) as f32 * pill_gap) / pill_count as f32)
        .clamp(40.0, 92.0);
    let res_w = pill_count as f32 * pill_w + (pill_count - 1) as f32 * pill_gap;
    let top_pad = Insets::new(
        safe.left + (bar_h * 1.6).max(24.0),
        safe.top,
        safe.right + pad * 0.5,
        0.0,
    );
    tree.push(ids::TOP, Element::new(ids::TOP_CONTENT, Semantics::Panel)
        .with_anchor(Anchor::FILL)
        .with_constraints(Constraints::fill())
        .with_bleed()
        .with_padding(top_pad)
        .with_flow(Flow::Row { gap: gap as u16, align: AlignY::Center, wrap: false }));
    tree.push(ids::TOP_CONTENT, Element::panel(ids::MAP_NAME)
        .with_constraints(Constraints {
            width: crate::ui::core::Extent::fill(),
            height: crate::ui::core::Extent::fixed(pill_h),
            ..Constraints::fill()
        })
        // The map name is the one top-bar item that may disappear: the resource
        // strip never shrinks below a readable numeric column.
        .with_overflow(Overflow::Collapse));
    tree.push(ids::TOP_CONTENT, Element::panel(ids::UPKEEP)
        .with_constraints(Constraints::fixed(upkeep_w, pill_h))
        .with_overflow(Overflow::Collapse));
    if preferences.debug {
        tree.push(ids::TOP_CONTENT, Element::panel(ids::FPS)
            .with_constraints(Constraints::fixed(fps_w, pill_h)));
    }
    tree.push(ids::TOP_CONTENT, Element::panel(ids::RESOURCES)
        .with_constraints(Constraints::fixed(res_w, pill_h))
        .with_flow(Flow::Row { gap: pill_gap as u16, align: AlignY::Stretch, wrap: false }));
    let pills = content.resources.clamp(1, pill_count);
    for index in 0..pills {
        tree.push(ids::RESOURCES, Element::panel(ids::PILL.at(index as u16))
            .with_constraints(Constraints::fixed(pill_w, pill_h)));
    }

    // ---------------------------------------------------------------- console
    tree.push(Id::ROOT, Element::new(ids::CONSOLE, Semantics::Console)
        .with_anchor(Anchor::BOTTOM_LEFT)
        .with_constraints(Constraints {
            width: crate::ui::core::Extent::fill(),
            height: crate::ui::core::Extent::fixed(console_h),
            ..Constraints::fill()
        })
        .transparent());

    match plan {
        Plan::Wide => build_wide(&mut tree, state, content, theme, content_rect, safe),
        Plan::Compact => build_compact(&mut tree, state, content, theme, content_rect, false, safe),
        Plan::Minimal => build_compact(&mut tree, state, content, theme, content_rect, true, safe),
        Plan::Portrait => {
            effective = build_portrait(&mut tree, state, content, theme, content_rect, false, safe);
        }
        Plan::PortraitNarrow => {
            build_portrait(&mut tree, state, content, theme, content_rect, true, safe);
        }
    }

    // The tree is solved here so the bands can be derived from solved zones.
    let probe = solve_layout(
        &tree,
        window,
        preferences,
        state,
        content,
        plan.breakpoint(),
    );
    let mut rects = HudRects {
        screen: Rect::from_size(screen),
        safe,
        top_bar,
        console,
        console_h,
        content: content_rect,
        viewport,
        ..HudRects::default()
    };
    read_top(&probe, &mut rects);
    read_console(plan, &probe, &mut rects, state, content, theme);
    compose_bands(
        plan,
        &mut rects,
        state,
        theme,
        screen,
        safe,
        gap,
        &mut extra_diagnostics,
    );
    audit_rects(plan, &rects, content_rect, &mut extra_diagnostics, theme);
    let _ = avail;
    Built {
        tree,
        rects,
        extra_diagnostics,
        effective,
    }
}

/// Content padding of the console: the plate is full-bleed, its controls are
/// not — the safe insets and the console padding both apply here, once.
fn console_padding(safe: Insets, pad: f32) -> Insets {
    Insets::new(
        safe.left + pad,
        pad,
        safe.right + pad,
        safe.bottom.max(pad),
    )
}

fn build_wide(
    tree: &mut UiTree,
    state: &State,
    content: &ContentMetrics,
    theme: &Theme,
    content_rect: Rect,
    safe: Insets,
) {
    let m = theme.metrics;
    tree.push(ids::CONSOLE, Element::new(ids::CONSOLE_CONTENT, Semantics::Panel)
        .with_anchor(Anchor::FILL)
        .with_constraints(Constraints::fill())
        .with_bleed()
        .with_padding(console_padding(safe, m.pad))
        .with_flow(Flow::Row { gap: m.gap as u16, align: AlignY::Start, wrap: false }));
    let frame = (content_rect.h - 20.0).clamp(m.minimap_min.min(content_rect.h), m.minimap);
    tree.push(ids::CONSOLE_CONTENT, Element::new(ids::MINIMAP_COLUMN, Semantics::Panel)
        .with_constraints(Constraints {
            width: crate::ui::core::Extent::fixed(frame),
            height: crate::ui::core::Extent::fill(),
            ..Constraints::fill()
        })
        .with_flow(Flow::Column { gap: 0, align: AlignX::Stretch }));
    tree.push(ids::MINIMAP_COLUMN, Element::panel(ids::MINIMAP_HEADER)
        .with_constraints(Constraints::fixed(frame, 20.0)));
    tree.push(ids::MINIMAP_COLUMN, Element::target(ids::MINIMAP_FRAME, Semantics::Minimap)
        .with_constraints(Constraints::fixed(frame, frame).with_aspect(1.0)));
    tree.push(ids::CONSOLE_CONTENT, Element::new(ids::SELECTION, Semantics::Selection)
        .with_constraints(Constraints::fill()));
    tree.push(ids::CONSOLE_CONTENT, Element::new(ids::GROUPS, Semantics::Panel)
        .with_constraints(Constraints::fixed(m.groups_tray + m.gap + m.touch, m.groups_tray))
        .with_valign(AlignY::Center)
        .with_flow(Flow::Row { gap: m.gap as u16, align: AlignY::Start, wrap: false }));
    tree.push(ids::GROUPS, Element::new(ids::TRAY, Semantics::Panel)
        .with_constraints(Constraints::fixed(m.groups_tray, m.groups_tray))
        .with_flow(Flow::Grid { columns: 3, rows: 3, gap: m.gap as u16 }));
    for index in 0..9u16 {
        tree.push(ids::TRAY, Element::target(ids::GROUP_SLOT.at(index), Semantics::Group)
            .with_constraints(Constraints::fill()));
    }
    tree.push(ids::GROUPS, Element::target(ids::GROUP_SLOT.at(9), Semantics::Group)
        .with_constraints(Constraints::fixed(m.touch, m.touch))
        .with_valign(AlignY::Center));
    let card_h = m.card_h.min(content_rect.h);
    let (card_gap, card_pad) = card_grid(card_h, &m);
    tree.push(ids::CONSOLE_CONTENT, Element::new(ids::CARD, Semantics::Card)
        .with_constraints(Constraints::fixed(m.card_w, card_h))
        .with_valign(AlignY::Center)
        .with_padding(Insets::splat(card_pad))
        .with_flow(Flow::Grid {
            columns: m.card_columns as u8,
            rows: m.card_rows as u8,
            gap: card_gap as u16,
        }));
    for index in 0..m.card_slots() as u16 {
        tree.push(ids::CARD, Element::target(ids::CARD_SLOT.at(index), Semantics::Slot)
            .with_constraints(Constraints::fill()));
    }
}

fn build_compact(
    tree: &mut UiTree,
    state: &State,
    content: &ContentMetrics,
    theme: &Theme,
    content_rect: Rect,
    minimal: bool,
    safe: Insets,
) {
    let m = theme.metrics;
    tree.push(ids::CONSOLE, Element::new(ids::CONSOLE_CONTENT, Semantics::Panel)
        .with_anchor(Anchor::FILL)
        .with_constraints(Constraints::fill())
        .with_bleed()
        .with_padding(console_padding(safe, m.pad))
        .with_flow(Flow::Row { gap: m.gap as u16, align: AlignY::Start, wrap: false }));
    let frame = content_rect.h.min(m.minimap_compact).max(1.0);
    if minimal {
        tree.push(ids::CONSOLE_CONTENT, Element::target(ids::MINIMAP_RAIL, Semantics::Minimap)
            .with_constraints(Constraints::fixed(m.touch, m.touch))
            .with_valign(AlignY::Center));
    } else {
        tree.push(ids::CONSOLE_CONTENT, Element::target(ids::MINIMAP_FRAME, Semantics::Minimap)
            .with_constraints(Constraints::fixed(frame, frame).with_aspect(1.0)));
    }
    tree.push(ids::CONSOLE_CONTENT, Element::new(ids::SELECTION, Semantics::Selection)
        .with_constraints(Constraints::fill()));
    tree.push(ids::CONSOLE_CONTENT, Element::target(ids::GROUPS_BUTTON, Semantics::Button)
        .with_constraints(Constraints::fixed(m.touch, m.touch))
        .with_valign(AlignY::Center));
    tree.push(ids::CONSOLE_CONTENT, Element::target(ids::COMMANDS_BUTTON, Semantics::Button)
        .with_constraints(Constraints::fixed(m.touch, m.touch))
        .with_valign(AlignY::Center));
    if state.groups_open {
        // A modal drawer floats over the playfield, above the console.
        tree.push(Id::ROOT, Element::new(ids::GROUPS_OVERLAY, Semantics::Drawer)
            .with_anchor(Anchor::BOTTOM_LEFT)
            .with_constraints(Constraints::fixed(
                m.groups_tray + m.gap + m.touch,
                m.groups_tray,
            ))
            .with_flow(Flow::Row { gap: m.gap as u16, align: AlignY::Start, wrap: false })
            .in_group("overlay")
            .with_z(10));
        tree.push(ids::GROUPS_OVERLAY, Element::new(ids::TRAY, Semantics::Panel)
            .with_constraints(Constraints::fixed(m.groups_tray, m.groups_tray))
            .with_flow(Flow::Grid { columns: 3, rows: 3, gap: m.gap as u16 }));
        for index in 0..9u16 {
            tree.push(ids::TRAY, Element::target(ids::GROUP_SLOT.at(index), Semantics::Group)
                .with_constraints(Constraints::fill()));
        }
        tree.push(ids::GROUPS_OVERLAY, Element::target(ids::GROUP_SLOT.at(9), Semantics::Group)
            .with_constraints(Constraints::fixed(m.touch, m.touch))
            .with_valign(AlignY::Center));
    }
    if state.commands_open {
        // The compact command drawer opens over the console it belongs to, not
        // over the telemetry strip: TOP_LEFT put its 12 cells at (8,8), above
        // the safe screen, and every card12/cooldown/commands/tooltip row failed
        // the `outside safe screen` gate.
        tree.push(Id::ROOT, Element::new(ids::COMMANDS_OVERLAY, Semantics::Drawer)
            .with_anchor(Anchor::BOTTOM_LEFT)
            .with_constraints(Constraints::fixed(m.card_w, m.card_h))
            .with_padding(Insets::splat(m.gap))
            .with_flow(Flow::Grid {
                columns: m.card_columns as u8,
                rows: m.card_rows as u8,
                gap: m.gap as u16,
            })
            .in_group("overlay")
            .with_z(11));
        for index in 0..m.card_slots() as u16 {
            tree.push(ids::COMMANDS_OVERLAY, Element::target(ids::CARD_SLOT.at(index), Semantics::Slot)
                .with_constraints(Constraints::fill()));
        }
    }
}

fn build_portrait(
    tree: &mut UiTree,
    state: &State,
    content: &ContentMetrics,
    theme: &Theme,
    content_rect: Rect,
    narrow: bool,
    safe: Insets,
) -> Plan {
    let m = theme.metrics;
    tree.push(ids::CONSOLE, Element::new(ids::CONSOLE_CONTENT, Semantics::Panel)
        .with_anchor(Anchor::FILL)
        .with_constraints(Constraints::fill())
        .with_bleed()
        .with_padding(console_padding(safe, m.pad))
        .with_flow(Flow::Column { gap: m.gap as u16, align: AlignX::Stretch }));
    // Summary strip: the selection card plus the GROUPS control.
    tree.push(ids::CONSOLE_CONTENT, Element::new(ids::SUMMARY, Semantics::Panel)
        .with_constraints(Constraints {
            width: crate::ui::core::Extent::fill(),
            height: crate::ui::core::Extent::fixed(m.portrait_summary),
            ..Constraints::fill()
        })
        .with_flow(Flow::Row { gap: m.gap as u16, align: AlignY::Start, wrap: false }));
    tree.push(ids::SUMMARY, Element::new(ids::SELECTION, Semantics::Selection)
        .with_constraints(Constraints::fill()));
    if state.selection_count > 1 {
        // Thumbnails sit beside the strip (never a separate column), so the
        // strip shrinks to their left edge and no row can run underneath them.
        for index in 0..2u16 {
            tree.push(ids::SUMMARY, Element::target(ids::ARMY_SLOT.at(index), Semantics::Button)
                .with_constraints(Constraints::fixed(m.touch, m.touch))
                .with_valign(AlignY::Center));
        }
    }
    tree.push(ids::SUMMARY, Element::target(ids::GROUPS_BUTTON, Semantics::Button)
        .with_constraints(Constraints::fixed(m.touch, m.touch))
        .with_valign(AlignY::Center));
    // Lower row: minimap (or rail) and the inline card (or a drawer control).
    // The minimap gives way to the card before the card gives way to a drawer.
    let room = (content_rect.w - m.card_w - m.gap).max(0.0);
    // The card stays inline as long as the minimap can keep a 48 dp target:
    // the map gives way first, the card only moves into its drawer after that.
    let narrow = narrow || room < m.touch;
    let map_side = room.min(m.minimap_compact).max(m.touch);
    let row_h = if narrow { m.touch } else { m.card_h };
    tree.push(ids::CONSOLE_CONTENT, Element::new(ids::ARMY, Semantics::Panel)
        .with_constraints(Constraints {
            width: crate::ui::core::Extent::fill(),
            height: crate::ui::core::Extent::fixed(row_h.max(m.touch)),
            ..Constraints::fill()
        })
        .with_flow(Flow::Row { gap: m.gap as u16, align: AlignY::Start, wrap: false }));
    if narrow {
        tree.push(ids::ARMY, Element::target(ids::MINIMAP_RAIL, Semantics::Minimap)
            .with_constraints(Constraints::fixed(m.touch, m.touch)));
        tree.push(ids::ARMY, Element::target(ids::COMMANDS_BUTTON, Semantics::Button)
            .with_constraints(Constraints::fixed(m.touch, m.touch)));
    } else {
        tree.push(ids::ARMY, Element::new(ids::MINIMAP_COLUMN, Semantics::Panel)
            .with_constraints(Constraints::fixed(map_side, map_side + 16.0))
            .with_flow(Flow::Column { gap: 0, align: AlignX::Stretch }));
        tree.push(ids::MINIMAP_COLUMN, Element::panel(ids::MINIMAP_HEADER)
            .with_constraints(Constraints::fixed(map_side, 16.0)));
        tree.push(ids::MINIMAP_COLUMN, Element::target(ids::MINIMAP_FRAME, Semantics::Minimap)
            .with_constraints(Constraints::fixed(map_side, map_side)));
        tree.push(ids::ARMY, Element::new(ids::CARD, Semantics::Card)
            .with_halign(AlignX::End)
            .with_constraints(Constraints::fixed(m.card_w, m.card_h.min(row_h.max(m.touch))))
            .with_padding(Insets::splat(m.gap))
            .with_flow(Flow::Grid {
                columns: m.card_columns as u8,
                rows: m.card_rows as u8,
                gap: m.gap as u16,
            }));
        for index in 0..m.card_slots() as u16 {
            tree.push(ids::CARD, Element::target(ids::CARD_SLOT.at(index), Semantics::Slot)
                .with_constraints(Constraints::fill()));
        }
    }
    let _ = content_rect;
    if state.groups_open {
        tree.push(Id::ROOT, Element::new(ids::GROUPS_OVERLAY, Semantics::Drawer)
            .with_anchor(Anchor::BOTTOM_LEFT)
            .with_constraints(Constraints::fixed(
                m.groups_tray + m.gap + m.touch,
                m.groups_tray,
            ))
            .with_flow(Flow::Row { gap: m.gap as u16, align: AlignY::Start, wrap: false })
            .in_group("overlay")
            .with_z(10));
        tree.push(ids::GROUPS_OVERLAY, Element::new(ids::TRAY, Semantics::Panel)
            .with_constraints(Constraints::fixed(m.groups_tray, m.groups_tray))
            .with_flow(Flow::Grid { columns: 3, rows: 3, gap: m.gap as u16 }));
        for index in 0..9u16 {
            tree.push(ids::TRAY, Element::target(ids::GROUP_SLOT.at(index), Semantics::Group)
                .with_constraints(Constraints::fill()));
        }
        tree.push(ids::GROUPS_OVERLAY, Element::target(ids::GROUP_SLOT.at(9), Semantics::Group)
            .with_constraints(Constraints::fixed(m.touch, m.touch))
            .with_valign(AlignY::Center));
    }
    if state.commands_open {
        // The compact command drawer opens over the console it belongs to, not
        // over the telemetry strip: TOP_LEFT put its 12 cells at (8,8), above
        // the safe screen, and every card12/cooldown/commands/tooltip row failed
        // the `outside safe screen` gate.
        tree.push(Id::ROOT, Element::new(ids::COMMANDS_OVERLAY, Semantics::Drawer)
            .with_anchor(Anchor::BOTTOM_LEFT)
            .with_constraints(Constraints::fixed(m.card_w, m.card_h))
            .with_padding(Insets::splat(m.gap))
            .with_flow(Flow::Grid {
                columns: m.card_columns as u8,
                rows: m.card_rows as u8,
                gap: m.gap as u16,
            })
            .in_group("overlay")
            .with_z(11));
        for index in 0..m.card_slots() as u16 {
            tree.push(ids::COMMANDS_OVERLAY, Element::target(ids::CARD_SLOT.at(index), Semantics::Slot)
                .with_constraints(Constraints::fill()));
        }
    }

    if narrow {
        Plan::PortraitNarrow
    } else {
        Plan::Portrait
    }
}

fn read_top(layout: &LayoutResult, rects: &mut HudRects) {
    rects.map_name = layout.rect_or_zero(ids::MAP_NAME);
    rects.upkeep = layout.rect_or_zero(ids::UPKEEP);
    rects.fps = layout.rect_or_zero(ids::FPS);
    rects.resources = layout.rect_or_zero(ids::RESOURCES);
    rects.resource_pills = (0..3)
        .map(|index| layout.rect_or_zero(ids::PILL.at(index)))
        .filter(|rect| !rect.is_empty())
        .collect();
}

fn read_console(
    plan: Plan,
    layout: &LayoutResult,
    rects: &mut HudRects,
    state: &State,
    content: &ContentMetrics,
    theme: &Theme,
) {
    let m = theme.metrics;
    rects.minimap_header = layout.rect_or_zero(ids::MINIMAP_HEADER);
    rects.minimap_frame = layout.rect_or_zero(ids::MINIMAP_FRAME);
    rects.minimap_rail = layout.rect_or_zero(ids::MINIMAP_RAIL);
    rects.selection = layout.rect_or_zero(ids::SELECTION);
    rects.card = layout.rect_or_zero(ids::CARD);
    rects.groups_area = layout.rect_or_zero(ids::GROUPS);
    rects.groups_button = layout.rect_or_zero(ids::GROUPS_BUTTON);
    rects.groups_overlay = layout.rect_or_zero(ids::GROUPS_OVERLAY);
    if !rects.groups_overlay.is_empty() {
        rects.groups_area = rects.groups_overlay;
    }
    rects.commands_button = layout.rect_or_zero(ids::COMMANDS_BUTTON);
    rects.commands_overlay = layout.rect_or_zero(ids::COMMANDS_OVERLAY);
    // An open command drawer *is* the card: the same twelve ids, the same grid,
    // only anchored above the console instead of inside it.
    if !rects.commands_overlay.is_empty() {
        rects.card = rects.commands_overlay;
    }
    rects.portrait = layout.rect_or_zero(ids::PORTRAIT);
    rects.name = layout.rect_or_zero(ids::NAME);
    rects.class = layout.rect_or_zero(ids::CLASS);
    rects.hp_bar = layout.rect_or_zero(ids::HP);
    rects.hp_text = rects.hp_bar;
    rects.mana_bar = layout.rect_or_zero(ids::MANA);
    rects.mana_text = rects.mana_bar;
    rects.stats = layout.rect_or_zero(ids::STATS);
    rects.production_label = layout.rect_or_zero(ids::PROD_LABEL);
    rects.production_bar = layout.rect_or_zero(ids::PROD_BAR);

    // Minimap image inside its ring.
    if !rects.minimap_frame.is_empty() {
        let ring: f32 = if plan.is_landscape() { 8.0 } else { 6.0 };
        let ring = ring.min(rects.minimap_frame.w * 0.1);
        rects.minimap_image = rects.minimap_frame.inset(ring, ring);
    }

    // ---- selection panel interior (§3.5): portrait, info column, bottom band
    //
    // The zone is solved; its interior is a fixed-row budget with a readable
    // floor. A row that cannot host its own line collapses instead of being
    // clipped (the old `row()` rule), and every rect is audited below.
    if !rects.selection.is_empty() {
        // Portrait summary: thumbnails are siblings of the strip, so the info
        // card shrinks before its interior is placed (never drawn underneath).
        let from_tree: Vec<Rect> = (0..2)
            .map(|index| layout.rect_or_zero(ids::ARMY_SLOT.at(index)))
            .filter(|rect| !rect.is_empty())
            .collect();
        if !from_tree.is_empty() {
            let solved = rects.selection;
            rects.selection = Rect::new(
                solved.x,
                solved.y,
                (from_tree[0].x - m.gap - solved.x).max(1.0),
                solved.h,
            );
            rects.army_slots = from_tree;
        }
        let zone = rects.selection;
        // The portrait image box follows the shipped frame's authored size at
        // the current HUD scale (128 px at scale 1): a fixed 80 px box left only
        // a 19 px window once the shipped 49/65/50/48 ornaments were applied
        // (`hudskin::portrait` retail test). The info gap still shrinks on a
        // narrow zone, and the box never eats the text column.
        let art_scale = (rects.console.h / 300.0).clamp(0.62, 2.4);
        let (portrait_side, info_gap) = if zone.w >= 260.0 {
            (
                (128.0 * art_scale)
                    .min((zone.h - 4.0).max(1.0))
                    .min((zone.w * 0.45).max(1.0)),
                8.0,
            )
        } else if zone.w >= 200.0 {
            (
                (112.0 * art_scale)
                    .min((zone.h - 4.0).max(1.0))
                    .min((zone.w * 0.45).max(1.0)),
                6.0,
            )
        } else {
            (
                (96.0 * art_scale)
                    .min((zone.h - 4.0).max(1.0))
                    .min((zone.w * 0.5).max(1.0)),
                5.0,
            )
        };
        rects.portrait = Rect::new(zone.x + 2.0, zone.y + 2.0, portrait_side, portrait_side);
        let info_x = rects.portrait.right() + info_gap;
        let info_w = (zone.right() - info_x - 2.0).max(1.0);
        let band_fits = zone.h >= m.touch * 1.6;
        let band_h = if band_fits { m.touch } else { 0.0 };
        let band_top = zone.bottom() - band_h;
        let strip = state.selection_count > 1 && band_fits;
        let inv = state.inventory && !strip && band_fits;
        let has_band = strip || inv;
        let column_bottom = if has_band { band_top - 4.0 } else { zone.bottom() };
        // Readable floor for a row: the interface never shrinks a glyph line
        // below 10 dp (30 px at the measured 3x density).
        let min_row_dp = 10.0;
        let bar_w = info_w.min(HP_BAR_MAX_DP);
        let mut y = zone.y + 2.0;
        let mut row = |h: f32, y: &mut f32| -> Rect {
            let available = (column_bottom - *y).max(0.0);
            if available + 0.01 < (h.min(min_row_dp)).max(1.0) {
                return Rect::ZERO;
            }
            let height = h.min(available);
            let rect = Rect::new(info_x, *y, info_w, height);
            *y += height;
            rect
        };
        let mut bar_row = |h: f32, y: &mut f32| -> Rect {
            let available = (column_bottom - *y).max(0.0);
            if available + 0.01 < 10.0 {
                return Rect::ZERO;
            }
            let height = h.min(available);
            let rect = Rect::new(info_x, *y, bar_w, height);
            *y += height;
            rect
        };
        // Row heights come from the type tokens: a 60 px heading needs a 75 px
        // row at 3x, a 39 px label a 51 px row. A row that cannot afford its
        // own line collapses instead of clipping the glyphs (audited below).
        let name_h = theme.type_scale.heading.line_dp.ceil() + 1.0;
        let label_h = theme.type_scale.label.line_dp.ceil() + 1.0;
        rects.name = row(name_h, &mut y);
        rects.class = row(label_h, &mut y);
        rects.hp_bar = bar_row(18.0, &mut y);
        rects.hp_text = rects.hp_bar;
        if state.has_mana {
            let mana = bar_row(label_h.min(18.0), &mut y);
            rects.mana_bar = mana;
            rects.mana_text = mana;
        }
        rects.stats = row(label_h, &mut y);
        if state.production {
            rects.production_label = row(label_h, &mut y);
            if !rects.production_label.is_empty() {
                rects.production_bar = Rect::new(
                    rects.production_label.x,
                    rects.production_label.bottom(),
                    64.0_f32.min(rects.production_label.w),
                    4.0,
                );
            }
        }
        if has_band {
            rects.army = Rect::new(zone.x + 2.0, band_top, (zone.w - 4.0).max(1.0), band_h);
            let pitch = band_h + m.gap;
            if strip {
                let count = state.selection_count.min(6);
                let badge_w = 34.0;
                let room = (rects.army.w - badge_w + m.gap).max(pitch);
                let cells = ((room / pitch).floor() as usize).clamp(1, count);
                for index in 0..cells {
                    rects.army_slots.push(Rect::new(
                        rects.army.x + index as f32 * pitch,
                        rects.army.y,
                        band_h,
                        band_h,
                    ));
                }
                if count > cells {
                    rects.army_badge =
                        Rect::new(rects.army.right() - badge_w, rects.army.y, badge_w, band_h);
                }
            } else {
                let cells = 6usize;
                let grid_w = cells as f32 * band_h + (cells - 1) as f32 * m.gap;
                let grid = Rect::new(
                    rects.army.x + (rects.army.w - grid_w).max(0.0),
                    rects.army.y,
                    grid_w.min(rects.army.w),
                    band_h,
                );
                // A six-slot hero inventory that does not fit the band (240 dp
                // hero row) would put its last slots outside the safe content.
                // Hide the strip rather than emit unreachable targets.
                if grid_w <= rects.army.w + 0.5 && rects.content.contains_rect(grid) {
                    rects.inventory = grid;
                    let pitch = band_h + m.gap;
                    for index in 0..cells {
                        rects.inventory_slots.push(Rect::new(
                            rects.inventory.x + index as f32 * pitch,
                            rects.inventory.y,
                            band_h,
                            band_h,
                        ));
                    }
                } else {
                    rects.inventory = Rect::default();
                }
            }
        }
    }

    // Card slots and the group tray: solved grids, never a shrunk cell.
    rects.card_slots = card_slots(rects.card, &m);
    rects.group_slots = tray_slots(rects.groups_area, &m);
    let _ = content;
}

/// Card trim and slot gap for a panel `available_h` deep.
///
/// A 4 x 3 field of 48 dp targets needs `3*48 + 2*gap + 2*pad`. The gap and the
/// trim shrink (8/8 -> 6/4 -> 4/2) before the targets do: a plate may get
/// tighter, a thumb target may not get smaller (§7.5).
pub fn card_grid(available_h: f32, m: &crate::ui::theme::Metrics) -> (f32, f32) {
    for (gap, pad) in [(m.gap, m.gap), (6.0, 4.0), (4.0, 2.0)] {
        let need = m.card_rows as f32 * m.touch + (m.card_rows - 1) as f32 * gap + 2.0 * pad;
        if need <= available_h + 0.01 {
            return (gap, pad);
        }
    }
    // Nothing fits: keep the smallest trim so the floor is reported honestly.
    (4.0, 2.0)
}

/// 4 x 3 card field. The solver already reported a `GridFloor` if the field is
/// too small, so this only reads the plan back.
pub fn card_slots(panel: Rect, m: &crate::ui::theme::Metrics) -> Vec<Rect> {
    if panel.w <= 0.0 || panel.h <= 0.0 {
        return Vec::new();
    }
    let (gap, pad) = card_grid(panel.h, m);
    let field = panel.inset(pad, pad);
    grid::plan(field, m.card_columns, m.card_rows, gap, m.touch, false).cells()
}

/// The ten control groups in a 4 x 3 field: IDs 1..9 row-major, then the tenth
/// (`0`) slot. The painter and the solver read the same grid, so a group can
/// never be drawn where it is not hittable.
pub fn tray_slots(area: Rect, m: &crate::ui::theme::Metrics) -> Vec<Rect> {
    if area.w <= 0.0 || area.h <= 0.0 {
        return Vec::new();
    }
    let mut slots = Vec::with_capacity(m.group_count);
    let pitch = m.touch + m.gap;
    for row in 0..3 {
        for column in 0..3 {
            slots.push(Rect::new(
                area.x + column as f32 * pitch,
                area.y + row as f32 * pitch,
                m.touch,
                m.touch,
            ));
        }
    }
    // The tenth slot (ID `0`) sits after the 3x3 block, vertically centred.
    slots.push(Rect::new(
        area.x + m.groups_tray + m.gap,
        area.y + (m.groups_tray - m.touch) * 0.5,
        m.touch,
        m.touch,
    ));
    slots
}

fn compose_bands(
    plan: Plan,
    rects: &mut HudRects,
    state: &State,
    theme: &Theme,
    screen: Size,
    safe: Insets,
    gap: f32,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let modal = state.groups_open || state.commands_open;
    let band_w: f32 = 420.0_f32
        .min(screen.w - safe.left - safe.right - gap * 2.0)
        .max(1.0);
    let band_h = 44.0;
    let mut bottom = rects.console.y - gap;
    if !modal {
        for _ in 0..3 {
            if bottom - band_h < rects.top_bar.bottom() + gap {
                break;
            }
            rects.toasts.push(Rect::new(
                ((screen.w + safe.left - safe.right) * 0.5 - band_w * 0.5)
                    .max(safe.left + gap),
                bottom - band_h,
                band_w,
                band_h,
            ));
            bottom -= band_h + gap;
        }
    }
    rects.toast = rects.toasts.first().copied().unwrap_or(Rect::ZERO);
    let tooltip_room = (bottom - rects.top_bar.bottom() - gap).max(0.0);
    let tooltip_h = 150.0_f32.min(tooltip_room);
    let tooltip_w = 360.0_f32
        .min(screen.w - safe.left - safe.right - gap * 2.0)
        .max(1.0);
    rects.tooltip = if !modal && tooltip_h >= 48.0 {
        Rect::new(
            ((screen.w + safe.left - safe.right) * 0.5 - tooltip_w * 0.5)
                .max(safe.left + gap),
            bottom - tooltip_h,
            tooltip_w,
            tooltip_h,
        )
    } else {
        Rect::ZERO
    };
    let hint_w = (band_w * 0.8).max(1.0);
    rects.hint = if rects.viewport.h >= 60.0 {
        Rect::new(
            safe.left + gap * 2.0,
            rects.top_bar.bottom() + gap,
            hint_w,
            32.0,
        )
    } else {
        Rect::ZERO
    };
    let phase_h =
        132.0_f32.min((rects.console.y - rects.top_bar.bottom() - gap * 2.0).max(0.0));
    let phase_w = 320.0_f32.min(screen.w - safe.left - safe.right);
    rects.phase = if phase_h >= 64.0 && phase_w >= 160.0 {
        Rect::new(
            ((screen.w - phase_w) * 0.5).max(safe.left),
            (rects.console.y - gap - phase_h).max(rects.top_bar.bottom() + gap),
            phase_w,
            phase_h,
        )
    } else {
        Rect::ZERO
    };
    rects.mode_rect = Rect::new(safe.left + gap, rects.top_bar.bottom() + gap, 120.0, 24.0);
    let _ = plan;
    let _ = theme;
    // Bands are derived, not solved: audit them against the safe band.
    let bounds = safe_area::content_rect(screen, safe);
    for (name, rect) in [
        ("toast", rects.toast),
        ("hint", rects.hint),
        ("tooltip", rects.tooltip),
        ("phase", rects.phase),
        ("mode", rects.mode_rect),
    ] {
        if rect.is_empty() {
            continue;
        }
        if !rect.is_inside(bounds) {
            diagnostics.push(Diagnostic::Offscreen {
                id: Id::new(match name {
                    "toast" => "toast",
                    "hint" => "hint",
                    "tooltip" => "tooltip",
                    "phase" => "phase",
                    _ => "mode",
                }),
                rect,
                bounds,
            });
        }
    }
    // Separators between console zones.
    let content = rects.content;
    let y = content.y + 2.0;
    let h = (content.h - 4.0).max(1.0);
    let mut xs: Vec<f32> = Vec::new();
    if !rects.minimap_frame.is_empty() {
        xs.push(rects.minimap_frame.right() + gap * 0.5);
    }
    if !rects.minimap_rail.is_empty() {
        xs.push(rects.minimap_rail.right() + gap * 0.5);
    }
    if !rects.groups_area.is_empty() {
        xs.push(rects.groups_area.x - gap * 0.5);
    }
    if !rects.groups_button.is_empty() {
        xs.push(rects.groups_button.x - gap * 0.5);
    }
    if !rects.card.is_empty() && plan.is_landscape() {
        xs.push(rects.card.x - gap * 0.5);
    }
    for x in xs {
        if x > content.x + 1.0 && x < content.right() - 1.0 {
            rects.separators.push(Rect::new(x - 0.5, y, 1.0, h));
        }
    }
}

fn audit_rects(
    plan: Plan,
    rects: &HudRects,
    content_rect: Rect,
    diagnostics: &mut Vec<Diagnostic>,
    theme: &Theme,
) {
    let m = theme.metrics;
    for (name, rect) in rects.interactive() {
        if rect.w.min(rect.h) + 0.01 < m.touch {
            diagnostics.push(Diagnostic::TinyTarget {
                id: Id::new(match name {
                    "card" => "card_slot",
                    "group" => "group_slot",
                    "army" => "army_slot",
                    "inventory" => "inv_slot",
                    "minimap" => "minimap_frame",
                    "minimap_rail" => "minimap_rail",
                    "groups_button" => "groups_button",
                    _ => "commands_button",
                }),
                rect,
                need: m.touch,
            });
        }
        if rect.bottom() <= rects.console.y + 0.5 && !rect.is_inside(content_rect) {
            diagnostics.push(Diagnostic::Offscreen {
                id: Id::new("content"),
                rect,
                bounds: content_rect,
            });
        }
    }
    // Card and tray cells tile; a gap that collapsed is a defect.
    for pair in rects.card_slots.windows(2) {
        if pair[0].intersection_area(pair[1]) > 0.01 {
            diagnostics.push(Diagnostic::Overlap {
                a: Id::new("card_slot"),
                b: Id::new("card_slot"),
                area: pair[0].intersection_area(pair[1]),
            });
            break;
        }
    }
    let _ = plan;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::layout::{SelectionState, State};

    fn theme() -> Theme {
        Theme::classic_fantasy()
    }

    fn solve(px: Size, density: f32) -> HudSolution {
        let window = Window::new(px, density);
        solve_hud(
            &window,
            &Preferences::default(),
            &State::default(),
            &ContentMetrics::with_resources(3),
            &theme(),
        )
    }

    #[test]
    fn the_device_takes_the_compact_composition() {
        let solution = solve(Size::new(2414.0, 1080.0), 3.0);
        assert_eq!(solution.plan, Plan::Compact, "{}", solution.report());
        assert!(solution.fits, "{}", solution.report());
    }

    #[test]
    fn a_desktop_takes_the_wide_composition_with_twelve_slots() {
        let solution = solve(Size::new(1920.0, 1080.0), 1.0);
        assert_eq!(solution.plan, Plan::Wide, "{}", solution.report());
        assert_eq!(solution.rects.card_slots.len(), 12);
        for slot in &solution.rects.card_slots {
            assert!(slot.w + 0.01 >= 48.0 && slot.h + 0.01 >= 48.0, "{slot:?}");
        }
    }

    #[test]
    fn portrait_solves_its_own_composition() {
        let solution = solve(Size::new(1080.0, 2400.0), 3.0);
        assert!(matches!(
            solution.plan,
            Plan::Portrait | Plan::PortraitNarrow
        ));
        assert!(solution.fits, "{}", solution.report());
        // 360 dp of dp width still hosts the inline card: the minimap gives way
        // (104 dp) before the card does.
        assert_eq!(solution.plan, Plan::Portrait, "{}", solution.report());
        assert_eq!(solution.rects.card_slots.len(), 12);
        assert!(solution.rects.card.y >= solution.rects.selection.bottom() - 0.01);
        assert!(solution.rects.minimap_frame.w >= 96.0);
        // A 300 dp-wide portrait cannot host a 232 dp card: it moves behind the
        // 48 dp COMMANDS control instead of shrinking.
        let narrow = solve(Size::new(900.0, 1920.0), 3.0);
        assert_eq!(narrow.plan, Plan::PortraitNarrow, "{}", narrow.report());
        assert!(narrow.rects.card_slots.is_empty());
        assert!(narrow.rects.commands_button.w + 0.01 >= 48.0);
        assert!(narrow.rects.commands_button.y >= narrow.rects.selection.bottom());
    }

    #[test]
    fn a_narrow_band_moves_the_card_into_a_drawer_instead_of_clipping() {
        // 640 dp cannot host minimap + selection + tray + a 232 dp card inline,
        // so the breakpoint picks the drawer composition before solving.
        let theme = theme();
        let window = Window::new(Size::new(640.0, 360.0), 1.0);
        assert_eq!(first_plan(&window, &theme), Plan::Compact);
        let solution = solve(Size::new(640.0, 360.0), 1.0);
        assert!(solution.fits, "{}", solution.report());
        assert!(solution.rects.card_slots.is_empty(), "card is a drawer here");
        assert!(solution.rects.commands_button.w >= 48.0);
        // The same window with the drawer open keeps all twelve ids.
        let mut state = State::default();
        state.commands_open = true;
        let open = solve_hud(
            &window,
            &Preferences::default(),
            &state,
            &ContentMetrics::with_resources(3),
            &theme,
        );
        assert_eq!(open.rects.card_slots.len(), 12, "{}", open.report());
        for slot in &open.rects.card_slots {
            assert!(slot.w + 0.01 >= 48.0 && slot.h + 0.01 >= 48.0, "{slot:?}");
        }
    }

    #[test]
    fn every_accepted_target_is_at_least_48_dp() {
        for (px, density) in [
            (Size::new(2414.0, 1080.0), 3.0),
            (Size::new(1920.0, 1080.0), 1.0),
            (Size::new(1080.0, 2400.0), 3.0),
            (Size::new(2560.0, 1080.0), 1.0),
            (Size::new(1024.0, 768.0), 1.0),
            (Size::new(800.0, 600.0), 1.0),
        ] {
            let solution = solve(px, density);
            for (name, rect) in solution.rects.interactive() {
                assert!(
                    rect.w + 0.01 >= 48.0 && rect.h + 0.01 >= 48.0,
                    "{name} at {px:?}/{density} is {rect:?} ({})",
                    solution.report()
                );
            }
        }
    }

    #[test]
    fn the_card_grid_is_row_major_with_an_8_dp_gap() {
        let solution = solve(Size::new(1920.0, 1080.0), 1.0);
        let slots = &solution.rects.card_slots;
        assert_eq!(slots.len(), 12);
        for index in 1..slots.len() {
            let prev = slots[index - 1];
            let slot = slots[index];
            if index % 4 == 0 {
                assert!(slot.y > prev.y);
            } else {
                assert!((slot.y - prev.y).abs() < 0.01);
                assert!((slot.x - prev.x - 56.0).abs() < 0.01);
            }
        }
    }

    #[test]
    fn a_live_selection_keeps_its_zone_in_every_plan() {
        for (px, density) in [
            (Size::new(2414.0, 1080.0), 3.0),
            (Size::new(1920.0, 1080.0), 1.0),
            (Size::new(1080.0, 2400.0), 3.0),
        ] {
            let mut state = State::default();
            state.selection = SelectionState::Hero;
            state.selection_count = 1;
            state.has_mana = true;
            let window = Window::new(px, density);
            let solution = solve_hud(
                &window,
                &Preferences::default(),
                &state,
                &ContentMetrics::with_resources(3),
                &theme(),
            );
            assert!(solution.fits, "{}", solution.report());
            assert!(!solution.rects.selection.is_empty());
            assert!(!solution.rects.name.is_empty(), "name row must exist");
        }
    }
}
