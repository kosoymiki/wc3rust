//! HUD debug overlay and frame budget (`hud.md` §11 / §10).
//!
//! Everything here is **off by default**. [`HudDebug::from_env`] reads one
//! selector, `WC3RUST_HUD_DEBUG`:
//!
//! ```text
//! WC3RUST_HUD_DEBUG=all
//! WC3RUST_HUD_DEBUG=bounds,hit,ids,budget
//! WC3RUST_HUD_DEBUG=1            # same as all
//! WC3RUST_HUD_DEBUG=0            # off (default when unset)
//! ```
//!
//! Layers: `bounds`, `hit`, `clip`, `safe`, `z`, `ids`, `breakpoint`, `theme`,
//! `state`, `diagnostics`, `budget`. A release build carries the code but the
//! selector is unset, so [`HudDebug::enabled`] is false and [`HudDebug::end`]
//! returns before touching the batch: the production HUD is unchanged.
//!
//! The overlay draws from the *same* [`MobileHudLayout`] the HUD and hit-testing
//! use (`hud.md` §6.3): no second geometry source. Layout bounds, interactive
//! rects and clip regions come from that one solve.
//!
//! The budget records what §10 asks to measure: layout time, UI draw-command
//! preparation time, draw calls, vertices and textures. The numbers are
//! printed as one greppable `HUD_FRAME` line at a bounded cadence and shown in
//! the diagnostics panel; the defaults are ceilings, not measurements.

use std::cell::Cell;
use std::fmt::Write as _;
use std::str::FromStr;
use std::time::Instant;

use crate::hud::mobile_hud::MobileHudLayout;
use crate::hud::{Font, Rect, UiBatch};

/// Environment selector for the debug overlay.
pub const ENV: &str = "WC3RUST_HUD_DEBUG";

/// One bit per overlay layer. Copyable, allocation-free.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DebugLayers(u16);

impl DebugLayers {
    pub const NONE: Self = Self(0);
    pub const BOUNDS: Self = Self(1 << 0);
    pub const HIT: Self = Self(1 << 1);
    pub const CLIP: Self = Self(1 << 2);
    pub const SAFE: Self = Self(1 << 3);
    pub const Z_ORDER: Self = Self(1 << 4);
    pub const IDS: Self = Self(1 << 5);
    pub const BREAKPOINT: Self = Self(1 << 6);
    pub const THEME: Self = Self(1 << 7);
    pub const STATE: Self = Self(1 << 8);
    pub const DIAGNOSTICS: Self = Self(1 << 9);
    pub const BUDGET: Self = Self(1 << 10);
    pub const ALL: Self = Self(0x7FF);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn bits(self) -> u16 {
        self.0
    }

    /// Human names for the panel/log.
    pub fn names(self) -> Vec<&'static str> {
        const NAMES: [(DebugLayers, &str); 11] = [
            (DebugLayers::BOUNDS, "bounds"),
            (DebugLayers::HIT, "hit"),
            (DebugLayers::CLIP, "clip"),
            (DebugLayers::SAFE, "safe"),
            (DebugLayers::Z_ORDER, "z"),
            (DebugLayers::IDS, "ids"),
            (DebugLayers::BREAKPOINT, "breakpoint"),
            (DebugLayers::THEME, "theme"),
            (DebugLayers::STATE, "state"),
            (DebugLayers::DIAGNOSTICS, "diagnostics"),
            (DebugLayers::BUDGET, "budget"),
        ];
        NAMES
            .iter()
            .filter(|(layer, _)| self.contains(*layer))
            .map(|(_, name)| *name)
            .collect()
    }
}

impl FromStr for DebugLayers {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed == "0" || trimmed.eq_ignore_ascii_case("off") {
            return Ok(Self::NONE);
        }
        if trimmed == "1" || trimmed.eq_ignore_ascii_case("on") || trimmed.eq_ignore_ascii_case("all") {
            return Ok(Self::ALL);
        }
        let mut layers = Self::NONE;
        for name in trimmed.split(',') {
            let name = name.trim().to_ascii_lowercase();
            let layer = match name.as_str() {
                "bounds" | "layout" | "layout-bounds" => Self::BOUNDS,
                "hit" | "hits" | "hit-regions" => Self::HIT,
                "clip" | "clipping" => Self::CLIP,
                "safe" | "safe-area" => Self::SAFE,
                "z" | "order" | "z-order" => Self::Z_ORDER,
                "ids" | "id" => Self::IDS,
                "breakpoint" | "bp" => Self::BREAKPOINT,
                "theme" => Self::THEME,
                "state" | "states" => Self::STATE,
                "diagnostics" | "diag" | "log" => Self::DIAGNOSTICS,
                "budget" | "perf" => Self::BUDGET,
                other => return Err(format!("unknown HUD debug layer {other:?}")),
            };
            layers.insert(layer);
        }
        Ok(layers)
    }
}

/// What a debug rectangle is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugKind {
    /// Element/layout bounds.
    Layout,
    /// Interactive/touch region.
    Hit,
    /// Clipping region.
    Clip,
    /// Safe-area frame.
    Safe,
}

/// One rectangle in the overlay. `id` is the element name from the layout, not
/// an index, so a screenshot can be matched to the code.
#[derive(Clone, Debug)]
pub struct DebugRect {
    pub id: String,
    pub rect: Rect,
    pub kind: DebugKind,
    pub z: i32,
    pub state: String,
}

/// One overlay frame: rects, metadata and the measured trace. The game builds
/// it from the solved [`MobileHudLayout`]; a headless proof can build it from
/// the layout matrix. Drawing never reads a second geometry source.
pub struct DebugFrame<'a> {
    pub viewport: Rect,
    pub density: f32,
    pub breakpoint: &'a str,
    pub theme: &'a str,
    pub screen: &'a str,
    pub rects: &'a [DebugRect],
    pub diagnostics: &'a [String],
    pub trace: &'a HudFrameTrace,
    pub budget: &'a HudBudget,
    pub safe: Option<Rect>,
}

/// Frame budget ceilings. Defaults are ceilings for a phone HUD: one texture
/// per role is allowed, but the batch should collapse to a handful of draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudBudget {
    pub max_draws: u32,
    pub max_vertices: u64,
    pub max_textures: usize,
    pub max_layout_micros: u64,
    pub max_prepare_micros: u64,
}

impl Default for HudBudget {
    fn default() -> Self {
        Self {
            max_draws: 96,
            max_vertices: 120_000,
            max_textures: 24,
            max_layout_micros: 2_000,
            max_prepare_micros: 4_000,
        }
    }
}

/// Measured HUD work for one frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HudFrameTrace {
    pub draws: u32,
    pub vertices: u64,
    pub textures: usize,
    pub white_vertices: u64,
    pub font_vertices: u64,
    pub minimap_vertices: u64,
    pub textured_draws: u32,
    pub layout_micros: u64,
    pub prepare_micros: u64,
}

impl HudFrameTrace {
    /// Count the batch channels exactly as the renderer submits them.
    pub fn from_batch(batch: &UiBatch) -> Self {
        let white = batch.white.len() as u64;
        let font = batch.font.len() as u64;
        let minimap = batch.minimap.len() as u64;
        let textured: u64 = batch
            .textured
            .iter()
            .map(|draw| draw.vertices.len() as u64)
            .sum();
        let draws = (white > 0) as u32
            + (font > 0) as u32
            + (minimap > 0) as u32
            + batch.textured.len() as u32;
        let mut textures: Vec<usize> = Vec::new();
        if white > 0 {
            textures.push(usize::MAX);
        }
        if font > 0 {
            textures.push(usize::MAX - 1);
        }
        if minimap > 0 {
            textures.push(usize::MAX - 2);
        }
        for draw in &batch.textured {
            if !textures.contains(&draw.texture) {
                textures.push(draw.texture);
            }
        }
        Self {
            draws,
            vertices: white + font + minimap + textured,
            textures: textures.len(),
            white_vertices: white,
            font_vertices: font,
            minimap_vertices: minimap,
            textured_draws: batch.textured.len() as u32,
            layout_micros: 0,
            prepare_micros: 0,
        }
    }

    /// Budget violations, in a stable order.
    pub fn violations(&self, budget: &HudBudget) -> Vec<&'static str> {
        let mut violations = Vec::new();
        if self.draws > budget.max_draws {
            violations.push("draws");
        }
        if self.vertices > budget.max_vertices {
            violations.push("vertices");
        }
        if self.textures > budget.max_textures {
            violations.push("textures");
        }
        if self.layout_micros > budget.max_layout_micros {
            violations.push("layout");
        }
        if self.prepare_micros > budget.max_prepare_micros {
            violations.push("prepare");
        }
        violations
    }

    /// `HUD_FRAME draws=.. vertices=.. textures=.. layout_us=.. prepare_us=..
    /// budget=PASS|OVER[:..]`.
    pub fn line(&self, budget: &HudBudget) -> String {
        let violations = self.violations(budget);
        let status = if violations.is_empty() {
            "PASS".to_string()
        } else {
            format!("OVER:{}", violations.join(","))
        };
        format!(
            "HUD_FRAME draws={} vertices={} textures={} white={} font={} minimap={} textured_draws={} layout_us={} prepare_us={} budget={}",
            self.draws,
            self.vertices,
            self.textures,
            self.white_vertices,
            self.font_vertices,
            self.minimap_vertices,
            self.textured_draws,
            self.layout_micros,
            self.prepare_micros,
            status
        )
    }
}

/// The overlay controller. Cheap when disabled: one `layers.is_empty()` check.
#[derive(Debug)]
pub struct HudDebug {
    layers: DebugLayers,
    budget: HudBudget,
    frames: Cell<u32>,
    layout_micros: Cell<u64>,
    log_every: u32,
}

impl Default for HudDebug {
    fn default() -> Self {
        Self {
            layers: DebugLayers::NONE,
            budget: HudBudget::default(),
            frames: Cell::new(0),
            layout_micros: Cell::new(0),
            log_every: 60,
        }
    }
}

impl HudDebug {
    /// Read `WC3RUST_HUD_DEBUG`. Invalid layer names are logged and ignored
    /// (the overlay must not take the game down); `0`/unset is off.
    pub fn from_env() -> Self {
        let RawValue::Set(value) = RawValue::read(ENV) else {
            return Self::default();
        };
        match DebugLayers::from_str(&value) {
            Ok(layers) => Self {
                layers,
                ..Self::default()
            },
            Err(error) => {
                crate::dev_log!("HUD_DEBUG selector_error={error}");
                Self::default()
            }
        }
    }

    pub fn with_layers(layers: DebugLayers) -> Self {
        Self {
            layers,
            ..Self::default()
        }
    }

    pub fn set_layers(&mut self, layers: DebugLayers) {
        self.layers = layers;
    }

    pub fn layers(&self) -> DebugLayers {
        self.layers
    }

    pub fn enabled(&self) -> bool {
        !self.layers.is_empty()
    }

    pub fn budget(&self) -> &HudBudget {
        &self.budget
    }

    pub fn set_budget(&mut self, budget: HudBudget) {
        self.budget = budget;
    }

    pub fn set_log_every(&mut self, frames: u32) {
        self.log_every = frames.max(1);
    }

    /// Start a stopwatch only when the overlay is on.
    pub fn begin(&self) -> Option<Instant> {
        self.enabled().then(Instant::now)
    }

    /// Stopwatch around the mobile-layout solve. The solve happens in
    /// `Game::rebuild_layout`, the draw-command build in `Game::scene`; keeping
    /// the two timers separate is what makes a layout regression visible.
    pub fn begin_layout(&self) -> Option<Instant> {
        self.begin()
    }

    pub fn end_layout(&self, started: Option<Instant>) {
        self.layout_micros.set(Self::stop(started));
    }

    pub fn last_layout_micros(&self) -> u64 {
        self.layout_micros.get()
    }

    /// Stop a stopwatch started by [`Self::begin`].
    pub fn stop(started: Option<Instant>) -> u64 {
        started
            .map(|start| start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0)
    }

    /// Finish one HUD frame: measure, log on cadence, draw the selected layers.
    ///
    /// `layout_start` / `prepare_start` come from [`Self::begin`] around the
    /// layout solve and the draw-command build. The batch is untouched when
    /// the selector is off.
    #[allow(clippy::too_many_arguments)]
    pub fn end(
        &self,
        batch: &mut UiBatch,
        viewport: Rect,
        density: f32,
        layout: &MobileHudLayout,
        font: Option<&Font>,
        screen: &str,
        theme: &str,
        prepare_start: Option<Instant>,
    ) {
        if !self.enabled() {
            return;
        }
        let mut trace = HudFrameTrace::from_batch(batch);
        trace.layout_micros = self.layout_micros.get();
        trace.prepare_micros = Self::stop(prepare_start);
        if self.layers.contains(DebugLayers::BUDGET)
            || self.layers.contains(DebugLayers::DIAGNOSTICS)
        {
            let frame = self.frames.get().wrapping_add(1);
            self.frames.set(frame);
            if self.log_every > 0 && frame % self.log_every == 1 {
                crate::dev_log!("{}", trace.line(&self.budget));
            }
        }
        let rects = collect_layout(layout);
        let diagnostics = vec![
            format!(
                "groups_open={} commands_open={} tooltip={}",
                layout.groups_overlay.w > 0.0,
                layout.commands_overlay.w > 0.0,
                layout.tooltip_anchor.is_some()
            ),
            format!(
                "safe={:.0},{:.0},{:.0},{:.0}",
                layout.safe.left, layout.safe.top, layout.safe.right, layout.safe.bottom
            ),
        ];
        let frame = DebugFrame {
            viewport,
            density,
            breakpoint: breakpoint_for(viewport, density),
            theme,
            screen,
            rects: &rects,
            diagnostics: &diagnostics,
            trace: &trace,
            budget: &self.budget,
            safe: Some(layout.safe_frame),
        };
        draw_debug(batch, &frame, font, self.layers);
    }
}

/// Draw one overlay frame. Public so a headless proof (the pack tool) renders
/// the same code the device runs, with an explicit selector.
pub fn draw_debug(batch: &mut UiBatch, frame: &DebugFrame<'_>, font: Option<&Font>, layers: DebugLayers) {
    draw_overlay(batch, frame, font, layers);
}

enum RawValue {
    Set(String),
}

impl RawValue {
    fn read(name: &str) -> Self {
        match std::env::var(name) {
            Ok(value) => RawValue::Set(value),
            Err(_) => RawValue::Set(String::new()),
        }
    }
}

/// Breakpoint label from the framebuffer size and density (dp = px / density).
pub fn breakpoint_for(viewport: Rect, density: f32) -> &'static str {
    let density = density.max(0.01);
    let width = viewport.w / density;
    let height = viewport.h / density;
    let short = width.min(height);
    let portrait = height > width;
    let class = if short < 360.0 {
        "compact"
    } else if short < 600.0 {
        "phone"
    } else {
        "tablet"
    };
    match (portrait, class) {
        (true, "compact") => "portrait-compact",
        (true, "phone") => "portrait-phone",
        (true, _) => "portrait-tablet",
        (false, "compact") => "landscape-compact",
        (false, "phone") => "landscape-phone",
        (false, _) => "landscape-tablet",
    }
}

/// Collect layout bounds, hit regions and clip regions from the one solved
/// layout. Allocation only happens when the debug overlay is enabled.
pub fn collect_layout(layout: &MobileHudLayout) -> Vec<DebugRect> {
    let mut rects = Vec::with_capacity(96);
    let mut push = |id: &str, rect: Rect, kind: DebugKind, state: &str| {
        if rect.w > 0.0 && rect.h > 0.0 {
            rects.push(DebugRect {
                id: id.to_string(),
                rect,
                kind,
                z: rects.len() as i32,
                state: state.to_string(),
            });
        }
    };
    // Layout bounds (the regions from `hud.md` §3).
    push("console", layout.console, DebugKind::Layout, "normal");
    push("console_inner", layout.console_inner, DebugKind::Layout, "normal");
    push("safe_frame", layout.safe_frame, DebugKind::Safe, "normal");
    push("minimap_frame", layout.minimap_frame, DebugKind::Layout, "normal");
    push("minimap", layout.minimap, DebugKind::Hit, "normal");
    push("selection", layout.selection, DebugKind::Layout, "normal");
    push("selection_panel", layout.panel, DebugKind::Layout, "normal");
    push("portrait", layout.portrait, DebugKind::Layout, "normal");
    push("name", layout.name, DebugKind::Layout, "normal");
    push("class", layout.class, DebugKind::Layout, "normal");
    push("hp_bar", layout.hp_bar, DebugKind::Layout, "normal");
    push("mana_bar", layout.mana_bar, DebugKind::Layout, "normal");
    push("stats", layout.stats, DebugKind::Layout, "normal");
    push("resources", layout.resources, DebugKind::Layout, "normal");
    push("upkeep", layout.upkeep, DebugKind::Layout, "normal");
    push("card", layout.card, DebugKind::Layout, "normal");
    push("groups", layout.groups_area, DebugKind::Layout, "normal");
    push("army", layout.army, DebugKind::Layout, "normal");
    push("tooltip", layout.tooltip, DebugKind::Layout, "normal");
    push("crest", layout.crest, DebugKind::Layout, "normal");
    push("mode", layout.mode, DebugKind::Layout, "normal");
    push("hint", layout.hint, DebugKind::Layout, "normal");
    push("fps", layout.fps, DebugKind::Layout, "normal");
    // Clipping regions.
    push("clip_console", layout.console, DebugKind::Clip, "clip");
    push("clip_minimap", layout.minimap_image, DebugKind::Clip, "clip");
    // Interactive regions: the same rects the input path tests.
    for (index, slot) in layout.card_slots.iter().enumerate() {
        push(&format!("card_slot_{index}"), *slot, DebugKind::Hit, "normal");
    }
    for (index, slot) in layout.group_slots.iter().enumerate() {
        push(&format!("group_slot_{index}"), *slot, DebugKind::Hit, "normal");
    }
    for (index, pill) in layout.resource_pills.iter().enumerate() {
        push(&format!("resource_{index}"), *pill, DebugKind::Hit, "normal");
    }
    for (index, slot) in layout.army_slots.iter().enumerate() {
        push(&format!("army_slot_{index}"), *slot, DebugKind::Hit, "normal");
    }
    for (index, tab) in layout.tabs.iter().enumerate() {
        push(&format!("tab_{index}"), *tab, DebugKind::Hit, "normal");
    }
    for (index, touch) in layout.inventory_parts.touches.iter().enumerate() {
        push(&format!("inventory_slot_{index}"), *touch, DebugKind::Hit, "normal");
    }
    push("groups_button", layout.groups_button, DebugKind::Hit, "normal");
    push("commands_button", layout.commands_button, DebugKind::Hit, "normal");
    push("groups_overlay", layout.groups_overlay, DebugKind::Hit, "open");
    push("commands_overlay", layout.commands_overlay, DebugKind::Hit, "open");
    if let Some(anchor) = layout.tooltip_anchor {
        push("tooltip_anchor", anchor, DebugKind::Hit, "hover");
    }
    rects
}

#[allow(clippy::too_many_arguments)]
fn draw_overlay(batch: &mut UiBatch, frame: &DebugFrame<'_>, font: Option<&Font>, layers: DebugLayers) {
    let viewport = frame.viewport;
    let density = frame.density;
    let mut labels: Vec<(&str, Rect, [f32; 4])> = Vec::new();
    for rect in frame.rects.iter().take(160) {
        let (color, width) = match rect.kind {
            DebugKind::Layout => ([0.35, 0.72, 1.0, 0.85], 1.0),
            DebugKind::Hit => ([1.0, 0.28, 0.32, 0.90], 1.2),
            DebugKind::Clip => ([1.0, 0.35, 1.0, 0.85], 1.0),
            DebugKind::Safe => ([0.45, 1.0, 0.55, 0.90], 1.5),
        };
        let visible = match rect.kind {
            DebugKind::Layout => layers.contains(DebugLayers::BOUNDS),
            DebugKind::Hit => layers.contains(DebugLayers::HIT),
            DebugKind::Clip => layers.contains(DebugLayers::CLIP),
            DebugKind::Safe => layers.contains(DebugLayers::SAFE),
        };
        if !visible {
            continue;
        }
        let color = if layers.contains(DebugLayers::Z_ORDER) {
            let step = (rect.z % 8) as f32 / 8.0;
            [0.35 + 0.6 * step, 0.85, 1.0 - 0.6 * step, 0.9]
        } else {
            color
        };
        if rect.kind == DebugKind::Hit {
            batch.rect(rect.rect, [color[0], color[1], color[2], 0.10]);
        }
        batch.border(rect.rect, width, color);
        if layers.contains(DebugLayers::IDS) {
            labels.push((rect.id.as_str(), rect.rect, color));
        }
    }
    if let Some(font) = font {
        for (id, rect, color) in labels.iter().take(64) {
            font.text_exact(batch, id, rect.x + 2.0, rect.y + 1.0, 0.5, *color);
        }
    }

    let show_panel = layers.contains(DebugLayers::DIAGNOSTICS)
        || layers.contains(DebugLayers::BREAKPOINT)
        || layers.contains(DebugLayers::THEME)
        || layers.contains(DebugLayers::STATE)
        || layers.contains(DebugLayers::BUDGET);
    if !show_panel {
        return;
    }
    let mut text = String::new();
    let _ = writeln!(text, "HUD DEBUG  {}", frame.screen);
    if layers.contains(DebugLayers::BREAKPOINT) {
        let _ = writeln!(
            text,
            "bp={} viewport={:.0}x{:.0} dp={:.0}x{:.0} density={:.2}",
            frame.breakpoint,
            viewport.w,
            viewport.h,
            viewport.w / density.max(0.01),
            viewport.h / density.max(0.01),
            density
        );
    }
    if layers.contains(DebugLayers::THEME) {
        let _ = writeln!(text, "theme={}", frame.theme);
    }
    if layers.contains(DebugLayers::STATE) {
        for line in frame.diagnostics.iter().take(4) {
            let _ = writeln!(text, "{line}");
        }
    }
    if layers.contains(DebugLayers::BUDGET) {
        let violations = frame.trace.violations(frame.budget);
        let status = if violations.is_empty() {
            "PASS".to_string()
        } else {
            format!("OVER:{}", violations.join(","))
        };
        let _ = writeln!(text, "budget={status}");
        let _ = writeln!(
            text,
            "draws={}/{} verts={}/{} tex={}/{}",
            frame.trace.draws,
            frame.budget.max_draws,
            frame.trace.vertices,
            frame.budget.max_vertices,
            frame.trace.textures,
            frame.budget.max_textures
        );
        let _ = writeln!(
            text,
            "layout_us={}/{} prep_us={}/{}",
            frame.trace.layout_micros,
            frame.budget.max_layout_micros,
            frame.trace.prepare_micros,
            frame.budget.max_prepare_micros
        );
    }
    if layers.contains(DebugLayers::DIAGNOSTICS) {
        let _ = writeln!(
            text,
            "rects={} layers={}",
            frame.rects.len(),
            layers.names().join(",")
        );
        if let Some(safe) = frame.safe {
            let _ = writeln!(
                text,
                "safe_rect={:.0},{:.0},{:.0},{:.0}",
                safe.x, safe.y, safe.w, safe.h
            );
        }
    }
    let line_height = font.map(|font| font.line_height * 0.55).unwrap_or(14.0);
    let height = (text.lines().count() as f32 * line_height + 10.0).min(viewport.h);
    let width = (text
        .lines()
        .map(|line| line.len())
        .max()
        .unwrap_or(24) as f32
        * 7.2
        + 12.0)
        .min(viewport.w);
    let origin = Rect::new(
        viewport.x + 8.0,
        viewport.y + 8.0,
        width.max(220.0).min((viewport.w - 16.0).max(1.0)),
        height.max(40.0),
    );
    batch.rect(origin, [0.02, 0.02, 0.03, 0.82]);
    batch.border(origin, 1.0, [0.9, 0.8, 0.35, 0.9]);
    if let Some(font) = font {
        for (index, line) in text.lines().enumerate() {
            font.text_exact(
                batch,
                line,
                origin.x + 5.0,
                origin.y + 4.0 + index as f32 * line_height,
                0.55,
                [0.95, 0.92, 0.80, 1.0],
            );
        }
    }
}
