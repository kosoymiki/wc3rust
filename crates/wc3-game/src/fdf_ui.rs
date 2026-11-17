//! Shipped-FDF UI screens: coverage, rendering and numeric layout checks.
//!
//! The chrome is driven by the extracted retail FDF corpus (`UI\FrameDef\**`)
//! and `war3skins.txt`, never by hand-built approximations. This module names
//! the screens the game draws, reports which shipped definitions were
//! consumed (documents, frames, required controls, skin keys) and renders them
//! at any resolution.
//!
//! Mechanic ids (tests in `tests/ui_fdf.rs`):
//! * `ui.fdf.frames` — every screen parses and renders from its definitions,
//!   with required controls present.
//! * `ui.fonts` — the shipped fonts load and cover Latin + Cyrillic, no missing
//!   glyphs in the UI strings.
//! * `ui.text` — labels resolve from the shipped StringList/WTS data.
//! * `ui.layout` — anchors/9-slice apply at 720p/1080p/1440p without overflow.

use std::collections::BTreeSet;

use ab_glyph::Font as _;

use crate::hud::{Font, Rect};
use crate::ui::{LayoutConfig, TextBindings, UiLayout, UiTheme};

/// Number of screens covered by `ui.fdf.frames`.
pub const SCREEN_COUNT: usize = 6;

/// A shipped UI screen the game draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UiScreen {
    Console,
    CommandCard,
    Tooltip,
    Minimap,
    Menu,
    QuestLog,
}

impl UiScreen {
    pub const ALL: [UiScreen; SCREEN_COUNT] = [
        UiScreen::Console,
        UiScreen::CommandCard,
        UiScreen::Tooltip,
        UiScreen::Minimap,
        UiScreen::Menu,
        UiScreen::QuestLog,
    ];

    /// Mechanic-level screen id (`ui.fdf.console`, ...).
    pub fn id(self) -> &'static str {
        match self {
            UiScreen::Console => "ui.fdf.console",
            UiScreen::CommandCard => "ui.fdf.command-card",
            UiScreen::Tooltip => "ui.fdf.tooltip",
            UiScreen::Minimap => "ui.fdf.minimap",
            UiScreen::Menu => "ui.fdf.menu",
            UiScreen::QuestLog => "ui.fdf.quest-log",
        }
    }

    pub fn spec(self) -> ScreenSpec {
        match self {
            UiScreen::Console => ScreenSpec {
                id: self.id(),
                documents: &["UI\\FrameDef\\UI\\ConsoleUI.fdf"],
                root: "ConsoleUI",
                required_frames: &["ConsoleUI"],
                skin_keys: &[],
            },
            UiScreen::CommandCard => ScreenSpec {
                id: self.id(),
                documents: &[
                    "UI\\FrameDef\\UI\\SimpleInfoPanel.fdf",
                    "UI\\FrameDef\\UI\\InfoPanelTemplates.fdf",
                ],
                root: "SimpleInfoPanelUnitDetail",
                required_frames: &[
                    "SimpleInfoPanelUnitDetail",
                    "SimpleUnitStatsPanel",
                    "SimpleProgressIndicator",
                    "SimpleInfoPanelIconDamage",
                    "SimpleInfoPanelIconArmor",
                    "SimpleInfoPanelIconFood",
                ],
                skin_keys: &[],
            },
            // Tooltips are skin art + StringList in retail (no tooltip FDF):
            // the shipped definitions are the eight `war3skins.txt` keys.
            UiScreen::Tooltip => ScreenSpec {
                id: self.id(),
                documents: &["UI\\FrameDef\\UI\\InfoPanelUnitDetail.fdf"],
                root: "InfoPanelUnitDetail",
                required_frames: &[],
                skin_keys: &[
                    "ToolTipBackground",
                    "ToolTipBorder",
                    "ToolTipGoldIcon",
                    "ToolTipLumberIcon",
                    "ToolTipStonesIcon",
                    "ToolTipManaIcon",
                    "ToolTipSupplyIcon",
                    "UnitTipBackground",
                ],
            },
            // Minimap chrome is skin art in retail; there is no minimap FDF in
            // the shipped corpus.
            UiScreen::Minimap => ScreenSpec {
                id: self.id(),
                documents: &[],
                root: "",
                required_frames: &[],
                skin_keys: &[
                    "MiniMapTerrainButtonEnabled",
                    "MiniMapTerrainButtonDisabled",
                    "MiniMapAllyButtonEnabled",
                    "MiniMapSignalButtonEnabled",
                ],
            },
            UiScreen::Menu => ScreenSpec {
                id: self.id(),
                documents: &[
                    "UI\\FrameDef\\UI\\EscMenuMainPanel.fdf",
                    "UI\\FrameDef\\UI\\EscMenuTemplates.fdf",
                ],
                root: "EscMenuMainPanel",
                required_frames: &["MainPanel", "PauseButton", "SaveGameButton"],
                skin_keys: &[],
            },
            UiScreen::QuestLog => ScreenSpec {
                id: self.id(),
                documents: &["UI\\FrameDef\\UI\\QuestDialog.fdf"],
                root: "QuestDialog",
                required_frames: &["QuestDialog", "QuestSubtitleValue"],
                skin_keys: &[],
            },
        }
    }
}

/// Shipped definitions one screen consumes.
#[derive(Clone, Copy, Debug)]
pub struct ScreenSpec {
    pub id: &'static str,
    pub documents: &'static [&'static str],
    /// Root frame laid out for the screen; empty for skin-only screens.
    pub root: &'static str,
    pub required_frames: &'static [&'static str],
    pub skin_keys: &'static [&'static str],
}

/// What the theme actually consumed for one screen.
#[derive(Clone, Debug, Default)]
pub struct ScreenCoverage {
    pub screen: &'static str,
    pub documents_expected: usize,
    pub documents_loaded: usize,
    pub root: &'static str,
    pub root_found: bool,
    pub frames: usize,
    pub required_present: usize,
    pub required_missing: Vec<&'static str>,
    pub skin_present: usize,
    pub skin_missing: Vec<&'static str>,
}

impl ScreenCoverage {
    pub fn complete(&self) -> bool {
        self.required_missing.is_empty()
            && self.skin_missing.is_empty()
            && self.documents_loaded == self.documents_expected
            && (self.root.is_empty() || self.root_found)
    }
}

fn normalize(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}

/// Whether a shipped document declares a frame/control with this name.
fn document_has_frame(theme: &UiTheme, name: &str) -> bool {
    theme.documents().iter().any(|document| {
        let mut found = false;
        document.walk(&mut |node: &wc3_formats::fdf::FdfNode| {
            if node.name.as_deref() == Some(name) {
                found = true;
            }
        });
        found
    })
}

/// Coverage of one screen against the loaded theme.
pub fn coverage(theme: &UiTheme, screen: UiScreen) -> ScreenCoverage {
    let spec = screen.spec();
    let loaded: BTreeSet<String> = theme
        .documents()
        .iter()
        .map(|doc| normalize(&doc.includer))
        .collect();
    let documents_loaded = spec
        .documents
        .iter()
        .filter(|path| loaded.contains(&normalize(path)))
        .count();
    let tree = (!spec.root.is_empty())
        .then(|| theme.resolve_tree(spec.root))
        .flatten();
    let frames = tree.as_ref().map(|tree| tree.len()).unwrap_or(0);
    let mut required_present = 0usize;
    let mut required_missing = Vec::new();
    for name in spec.required_frames {
        let in_tree = tree.as_ref().is_some_and(|tree| tree.find(name).is_some());
        if in_tree || document_has_frame(theme, name) {
            required_present += 1;
        } else {
            required_missing.push(*name);
        }
    }
    let mut skin_present = 0usize;
    let mut skin_missing = Vec::new();
    for key in spec.skin_keys {
        if theme.skin_string(key).is_some() {
            skin_present += 1;
        } else {
            skin_missing.push(*key);
        }
    }
    ScreenCoverage {
        screen: spec.id,
        documents_expected: spec.documents.len(),
        documents_loaded,
        root: spec.root,
        root_found: tree.is_some(),
        frames,
        required_present,
        required_missing,
        skin_present,
        skin_missing,
    }
}

/// Render one screen from its shipped definitions at the configured size.
pub fn render(
    theme: &UiTheme,
    screen: UiScreen,
    config: &LayoutConfig,
    bindings: &TextBindings,
    font: Option<&Font>,
) -> Option<UiLayout> {
    let spec = screen.spec();
    if spec.root.is_empty() {
        return None;
    }
    theme.build_layout(spec.root, config, bindings, font)
}

/// One frame whose pixel rectangle leaves the screen.
#[derive(Clone, Debug, PartialEq)]
pub struct OverflowFrame {
    pub name: String,
    pub rect: Rect,
}

/// Frames (visible, non-empty) that leave the screen bounds by more than
/// `EPS` pixels. Empty means the anchors fit at that resolution.
pub fn overflow(layout: &UiLayout) -> Vec<OverflowFrame> {
    const EPS: f32 = 0.5;
    let (width, height) = (layout.screen.x, layout.screen.y);
    let mut out = Vec::new();
    for node in &layout.nodes {
        let rect = node.rect;
        if !node.visible || rect.w <= 0.0 || rect.h <= 0.0 {
            continue;
        }
        if rect.x < -EPS
            || rect.y < -EPS
            || rect.x + rect.w > width + EPS
            || rect.y + rect.h > height + EPS
        {
            out.push(OverflowFrame {
                name: node.name.clone().unwrap_or_default(),
                rect,
            });
        }
    }
    out
}

/// Nine-slice segments of `rect` with `inset` corners, row-major
/// (top-left, top, top-right, left, centre, right, bottom-left, bottom,
/// bottom-right). The inset is clamped to half of the smaller dimension so a
/// small frame still tiles without overlaps.
pub fn nine_slice(rect: Rect, inset: f32) -> [Rect; 9] {
    nine_slice_sides(rect, [inset; 4])
}

/// Per-edge nine-slice of `rect`, row-major (top-left, top, top-right, left,
/// centre, right, bottom-left, bottom, bottom-right).
///
/// `insets` are `[left, top, right, bottom]` in destination pixels; each side is
/// clamped to half of its axis so the three bands always cover `rect` exactly
/// (no gap, no overlap).
pub fn nine_slice_sides(rect: Rect, insets: [f32; 4]) -> [Rect; 9] {
    const GAP_EPS: f32 = 0.01;
    let left = insets[0].max(0.0).min(rect.w * 0.5);
    let right = insets[2].max(0.0).min(rect.w * 0.5);
    let top = insets[1].max(0.0).min(rect.h * 0.5);
    let bottom = insets[3].max(0.0).min(rect.h * 0.5);
    let xs = [rect.x, rect.x + left, rect.x + rect.w - right, rect.x + rect.w];
    let ys = [rect.y, rect.y + top, rect.y + rect.h - bottom, rect.y + rect.h];
    let mut segments = [Rect::new(0.0, 0.0, 0.0, 0.0); 9];
    let mut index = 0;
    for row in 0..3 {
        for column in 0..3 {
            let w = xs[column + 1] - xs[column];
            let h = ys[row + 1] - ys[row];
            segments[index] = Rect::new(xs[column], ys[row], w.max(0.0), h.max(0.0));
            index += 1;
        }
    }
    // The three bands must cover the full rect exactly (no gaps, no overlap).
    debug_assert!((segments[0].w + segments[1].w + segments[2].w - rect.w).abs() <= GAP_EPS);
    debug_assert!((segments[0].h + segments[3].h + segments[6].h - rect.h).abs() <= GAP_EPS);
    segments
}

/// The nine-patch geometry of one WC3 frame: which texture, how big it is and how
/// large its corners are (in source pixels).
///
/// This is the FDF/skin side of the nine-patch path: a frame key that the theme
/// loaded (`CommandButtonBorder`, `TooltipBorder`, `ConsoleTexture01`, ...)
/// resolves to geometry the renderer can draw directly, so the HUD never
/// stretches a frame as a single quad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameArt {
    pub texture: usize,
    pub atlas: [f32; 2],
    /// `[left, top, right, bottom]` in source pixels.
    pub insets: [f32; 4],
}

impl FrameArt {
    /// Resolve a loaded frame key through the theme.
    pub fn resolve(theme: &UiTheme, key: &str) -> Option<Self> {
        let info = theme.texture_info(key)?;
        let insets = theme.frame_insets(key)?;
        Some(Self {
            texture: info.index,
            atlas: [info.width as f32, info.height as f32],
            insets,
        })
    }

    /// The patch the renderer draws (`UiBatch::nine_patch`).
    pub fn patch(self) -> crate::hud::NinePatch {
        crate::hud::NinePatch::full(self.texture, self.atlas, self.insets)
    }

    /// Corner sizes in destination pixels at `scale`, clamped to `rect`.
    pub fn corners(self, rect: Rect, scale: f32) -> [f32; 4] {
        self.patch().with_art_scale(scale).dest_corners(rect)
    }

    /// Row-major destination segments of `rect` for this frame's corners.
    pub fn slices(self, rect: Rect, scale: f32) -> [Rect; 9] {
        nine_slice_sides(rect, self.corners(rect, scale))
    }
}

/// Glyph coverage of the shipped font against the UI strings.
#[derive(Clone, Debug, Default)]
pub struct GlyphCoverage {
    pub latin_total: usize,
    pub latin_present: usize,
    pub cyrillic_total: usize,
    pub cyrillic_present: usize,
    pub strings_checked: usize,
    pub chars_checked: usize,
    pub missing: Vec<char>,
}

/// Count Latin/Cyrillic glyphs in `font_bytes` and find UI string characters
/// the font cannot draw. `Err` when the font file itself cannot be parsed.
pub fn glyph_coverage(font_bytes: &[u8], strings: &[String]) -> Result<GlyphCoverage, String> {
    let font = ab_glyph::FontRef::try_from_slice(font_bytes).map_err(|e| format!("font: {e}"))?;
    let present = |ch: char| font.glyph_id(ch) != ab_glyph::GlyphId(0);
    let latin: Vec<char> = ('\u{20}'..='\u{7e}').collect();
    let cyrillic: Vec<char> = ('\u{400}'..='\u{4ff}').collect();
    let missing: BTreeSet<char> = strings
        .iter()
        .flat_map(|text| text.chars())
        .filter(|ch| !ch.is_control() && *ch != '\u{feff}')
        .filter(|ch| !present(*ch))
        .collect();
    Ok(GlyphCoverage {
        latin_total: latin.len(),
        latin_present: latin.iter().filter(|ch| present(**ch)).count(),
        cyrillic_total: cyrillic.len(),
        cyrillic_present: cyrillic.iter().filter(|ch| present(**ch)).count(),
        strings_checked: strings.len(),
        chars_checked: strings.iter().map(|text| text.chars().count()).sum(),
        missing: missing.into_iter().collect(),
    })
}

/// Strings the game UI actually shows: shipped StringList values, menu labels
/// of every FDF screen and the resolved skin labels.
pub fn ui_strings(theme: &UiTheme) -> Vec<String> {
    let mut strings: Vec<String> = theme
        .localization()
        .entries()
        .map(|(_, value)| value.to_string())
        .collect();
    for screen in UiScreen::ALL {
        let spec = screen.spec();
        if spec.root.is_empty() {
            continue;
        }
        for label in theme.menu_labels(spec.root) {
            strings.push(label.text);
        }
    }
    strings
}

/// Label resolution for one screen root.
#[derive(Clone, Debug, Default)]
pub struct StringCoverage {
    pub labels: usize,
    pub resolved_keys: usize,
    pub literals: usize,
    /// Keys that fell back to the key text instead of a shipped string.
    pub unresolved: Vec<String>,
}

/// Whether a label value has the shape of a StringList/WTS key (no spaces,
/// uppercase token like `GOLD`, `COLON_GOLD`, `KEY_MENU`). Retail FDFs also
/// ship spaced runtime placeholders (`Text "A Quest"` — filled by game code),
/// which are literals, not keys.
pub fn is_localization_key(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && !text.chars().any(char::is_whitespace)
        && text.chars().any(|ch| ch.is_ascii_uppercase())
        && text.chars().all(|ch| {
            ch.is_ascii_uppercase() || ch.is_ascii_digit() || matches!(ch, '_' | '|' | '%' | ':')
        })
}

/// Resolve every label of a screen root through the shipped StringList/WTS.
/// `unresolved` counts only key-shaped values that fell back to the key text;
/// spaced runtime placeholders are reported as literals.
pub fn string_coverage(theme: &UiTheme, root: &str) -> StringCoverage {
    let mut coverage = StringCoverage::default();
    for label in theme.menu_labels(root) {
        coverage.labels += 1;
        match label.key {
            Some(ref key) if label.text == *key => {
                if is_localization_key(key) {
                    coverage.unresolved.push(key.clone());
                } else {
                    coverage.literals += 1;
                }
            }
            Some(_) => coverage.resolved_keys += 1,
            None => coverage.literals += 1,
        }
    }
    coverage
}

// ---------------------------------------------------------------------------
// Shipped menu/dialog art (war3skins keys used by the WC3 menus and dialogs).
// ---------------------------------------------------------------------------

/// Shipped art paths for one menu/dialog chrome, resolved from `war3skins.txt`.
/// The drawing lives in the HUD menu code; this only names the shipped art so
/// every consumer (in-game menu, dialogs, mocks) uses the same definitions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuArtPaths {
    pub panel_background: Option<String>,
    pub panel_border: Option<String>,
    pub title_band: Option<String>,
    pub button_normal: Option<String>,
    pub button_hover: Option<String>,
    pub button_pressed: Option<String>,
    pub button_disabled: Option<String>,
    pub loading_frame: Option<String>,
}

impl MenuArtPaths {
    pub fn resolve(skin: &crate::ui::SkinTable) -> MenuArtPaths {
        let key = |name: &str| skin.get(name).map(|value| decorate_blp(value));
        MenuArtPaths {
            panel_background: key("EscMenuBackground"),
            panel_border: key("EscMenuBorder"),
            title_band: key("EscMenuTitleBand")
                .or_else(|| key("GlueScreenButtonBorder")),
            button_normal: key("EscMenuButtonBackground"),
            button_hover: key("EscMenuButtonMouseOverHighlight"),
            button_pressed: key("EscMenuButtonPushedBackground"),
            button_disabled: key("EscMenuButtonDisabledPushedBackground")
                .or_else(|| key("EscMenuButtonDisabledBackground")),
            loading_frame: key("LoadingFrame"),
        }
    }

    /// Number of resolved shipped art entries (0..=8).
    pub fn resolved(&self) -> usize {
        [
            &self.panel_background,
            &self.panel_border,
            &self.title_band,
            &self.button_normal,
            &self.button_hover,
            &self.button_pressed,
            &self.button_disabled,
            &self.loading_frame,
        ]
        .iter()
        .filter(|entry| entry.is_some())
        .count()
    }
}

/// `war3skins.txt` texture values omit the extension; the loader appends `.blp`.
fn decorate_blp(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.to_ascii_lowercase().ends_with(".blp") {
        trimmed.to_string()
    } else {
        format!("{trimmed}.blp")
    }
}
