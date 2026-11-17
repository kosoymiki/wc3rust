//! FDF text layer: font registry, localization tables and text shaping.
//!
//! The FDF corpus refers to fonts by *skin key* (`FrameFont "MasterFont",
//! 0.013, ""`), to text by *game-string key* (`Text "COLON_GOLD"`) and to
//! colours with in-string escape codes (`|cffffcc00...|r`). This module owns
//! all three concerns and stays renderer-free apart from the final
//! [`TextShaper::draw`] helper, which writes quads into the existing
//! screen-space [`UiBatch`] using the CPU [`Font`] atlas.
//!
//! Observed facts that drive the implementation (retail corpus, 2026-11-12):
//! * `war3skins.txt [Default]` declares `MasterFont=Fonts\FRIZQT__.TTF` and
//!   friends; the race sections do not repeat them.
//! * `GlobalStrings.fdf` is a `StringList` with 922 keys, already localised
//!   (Russian in the extracted data), including 126 `COLON_*` and 82 `KEY_*`
//!   keys. `Text` values are either one of those keys or literal text.
//! * Eight `Text` values reference keys missing from the shipped StringLists
//!   (`RESTART_MISSION`, `COLON_SUPPLY_PROVIDED`, ...), so resolution must
//!   fall back gracefully instead of dropping the label.
//! * `FrameFont "MasterFont", 0.013, ""` is the modern spelling; nine older
//!   property lines use `Font "MasterFont",0.01`.

use std::collections::HashMap;

use glam::Vec2;
use wc3_formats::fdf::{FdfFontRef, FdfNode};

use crate::hud::{Font, Rect, UiBatch};

/// FDF font sizes are fractions of the screen height; 0.01 is the common
/// body-text size (10.8 px at 1080p in the original game).
pub const DEFAULT_FONT_SIZE: f32 = 0.01;

/// The screen-space floor used when a design-time size would be unreadable on
/// a phone. The default is **0**: FDF/war3 font sizes are exact parity values
/// (`size * screen_height`); phone-readable call sites opt into a floor through
/// [`TextStyle::min_px`] (the HUD console uses 24 px in `hud::text_box`).
pub const DEFAULT_MIN_FONT_PX: f32 = 0.0;

/// UI font role, ported from the DragonUI centralized locale-aware font
/// registry (`DragonUI/core/fonts.lua`, MIT, (c) 2026 NeticSoul — see
/// `analysis/agent-fdf/THIRD_PARTY_NOTICES.md`). WC3 role names and paths come
/// from the operator's `war3skins.txt`; only the *pattern* and the CJK
/// fallback table are donor reference material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontRole {
    /// Unit frames, tooltips, game menu, general text.
    Primary,
    /// Action/command card hotkeys and counters.
    ActionBar,
    /// Narrow options/editor labels.
    Narrow,
    /// `ARIALN` replacement.
    ArialN,
}

impl FontRole {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "primary" => Some(Self::Primary),
            "actionbar" | "action_bar" => Some(Self::ActionBar),
            "narrow" => Some(Self::Narrow),
            "arialn" | "arial_n" => Some(Self::ArialN),
            _ => None,
        }
    }
}

/// Where a registry entry came from. Donor entries are reference/fallback
/// material, never the base data source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontSource {
    /// Operator game font from `war3skins.txt`.
    Game,
    /// Derived from the FDF font-size histogram.
    Fdf,
    /// Donor-attributed fallback (`DragonUI/core/fonts.lua`).
    Donor,
}

/// A font entry from the skin table (`MasterFont=Fonts\FRIZQT__.TTF`).
#[derive(Debug, Clone, PartialEq)]
pub struct FontFace {
    pub name: String,
    pub path: String,
    /// Default size as a fraction of screen height (FDF histogram).
    pub size: f32,
    pub flags: Vec<String>,
    pub source: FontSource,
}

/// A [`TextStyle`] resolved against the registry for a locale: the face to
/// load, the effective design size and where the entry came from. This is what
/// the text/console/tooltip draw paths consume instead of re-deriving paths.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedFace {
    pub name: String,
    pub path: String,
    /// Design size as a fraction of screen height (style size when set, else
    /// the face default).
    pub size: f32,
    pub source: FontSource,
}

/// Name -> font file mapping with roles and locale fallback. Lookups are
/// case-insensitive because FDF files and skins are not perfectly consistent
/// about case.
#[derive(Debug, Clone)]
pub struct FontRegistry {
    faces: Vec<FontFace>,
    index: HashMap<String, usize>,
    roles: HashMap<FontRole, String>,
    default_name: String,
}

impl Default for FontRegistry {
    fn default() -> Self {
        Self::with_builtin_fonts()
    }
}

impl FontRegistry {
    pub fn new() -> Self {
        Self {
            faces: Vec::new(),
            index: HashMap::new(),
            roles: HashMap::new(),
            default_name: "MasterFont".to_string(),
        }
    }

    /// The retail `war3skins.txt [Default]` font keys, role assignments and
    /// locale fallback table. Used when the game data is unavailable so a
    /// screen can still be built. CJK paths are donor reference fallback only;
    /// ruRU is the operator's FRIZQT__ (it carries Cyrillic).
    pub fn with_builtin_fonts() -> Self {
        let mut registry = Self::new();
        for (role, name, size) in [
            (FontRole::Primary, "MasterFont", 0.013_f32),
            (FontRole::ActionBar, "TextTagFont", 0.010),
            (FontRole::Narrow, "EscMenuTextFont", 0.011),
            (FontRole::ArialN, "MessageFont", 0.013),
        ] {
            registry.insert_face(
                role,
                name,
                "Fonts\\FRIZQT__.TTF",
                size,
                Vec::new(),
                FontSource::Game,
            );
        }
        for name in ["ChatFont", "InfoPanelTextFont"] {
            registry.insert_face(
                FontRole::Primary,
                name,
                "Fonts\\FRIZQT__.TTF",
                0.011,
                Vec::new(),
                FontSource::Game,
            );
        }
        // Locale fallback (donor reference; not part of the operator data).
        for (locale, key, path) in [
            ("ruRU", "FRIZQT__", "Fonts\\FRIZQT__.TTF"),
            ("koKR", "2002", "Fonts\\2002.TTF"),
            ("zhCN", "ZYKai_T", "Fonts\\ZYKai_T.TTF"),
            ("zhTW", "bLEI00D", "Fonts\\bLEI00D.TTF"),
        ] {
            registry.insert_locale(locale, key, path);
        }
        registry
    }

    fn insert_face(
        &mut self,
        role: FontRole,
        name: impl Into<String>,
        path: impl Into<String>,
        size: f32,
        flags: Vec<String>,
        source: FontSource,
    ) {
        let name = name.into();
        self.roles.entry(role).or_insert_with(|| name.clone());
        let key = name.to_ascii_lowercase();
        if let Some(existing) = self.index.get(&key).copied() {
            self.faces[existing].path = path.into();
            self.faces[existing].size = size;
            self.faces[existing].flags = flags;
            self.faces[existing].source = source;
            return;
        }
        self.index.insert(key, self.faces.len());
        self.faces.push(FontFace {
            name,
            path: path.into(),
            size,
            flags,
            source,
        });
    }

    fn insert_locale(&mut self, locale: &str, key: &str, path: &str) {
        let name = format!("{key} ({locale})");
        let path = path.to_string();
        // ruRU ships the operator's Cyrillic-capable FRIZQT__, so it stays a
        // game face; the CJK keys are donor-attributed fallback references.
        let source = if locale == "ruRU" {
            FontSource::Game
        } else {
            FontSource::Donor
        };
        let key_lower = key.to_ascii_lowercase();
        if let Some(existing) = self.index.get(&key_lower).copied() {
            self.faces[existing].path = path;
            self.faces[existing].source = source;
            return;
        }
        self.index.insert(key_lower, self.faces.len());
        self.faces.push(FontFace {
            name,
            path,
            size: 0.013,
            flags: vec!["locale-fallback".to_string()],
            source,
        });
    }

    pub fn insert(&mut self, name: impl Into<String>, path: impl Into<String>) {
        let name = name.into();
        let key = name.to_ascii_lowercase();
        if let Some(existing) = self.index.get(&key).copied() {
            self.faces[existing].path = path.into();
            return;
        }
        self.index.insert(key, self.faces.len());
        self.faces.push(FontFace {
            name,
            path: path.into(),
            size: 0.013,
            flags: Vec::new(),
            source: FontSource::Game,
        });
    }

    pub fn merge<I, K, V>(&mut self, entries: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (name, path) in entries {
            self.insert(name, path);
        }
    }

    pub fn get(&self, name: &str) -> Option<&FontFace> {
        self.index
            .get(&name.to_ascii_lowercase())
            .map(|&index| &self.faces[index])
    }

    /// Resolve a name to a font file path, falling back to the default face.
    pub fn resolve(&self, name: &str) -> &str {
        self.get(name)
            .or_else(|| self.get(&self.default_name))
            .map(|face| face.path.as_str())
            .unwrap_or("")
    }

    pub fn default_name(&self) -> &str {
        &self.default_name
    }

    pub fn set_default_name(&mut self, name: impl Into<String>) {
        self.default_name = name.into();
    }

    pub fn len(&self) -> usize {
        self.faces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// Locale override key: ruRU uses FRIZQT__ (Cyrillic), CJK uses the
    /// donor fallback face. Returns `None` for Latin locales.
    pub fn locale_override_key(locale: &str) -> Option<&'static str> {
        match locale {
            "ruRU" => Some("FRIZQT__"),
            "koKR" => Some("2002"),
            "zhCN" => Some("ZYKai_T"),
            "zhTW" => Some("bLEI00D"),
            _ => None,
        }
    }

    /// Whether the locale needs a system/donor face beyond the game skin.
    pub fn needs_system_font(locale: &str) -> bool {
        matches!(locale, "koKR" | "zhCN" | "zhTW")
    }

    pub fn role_default_size(role: FontRole) -> f32 {
        match role {
            FontRole::Primary => 0.013,
            FontRole::ActionBar => 0.010,
            FontRole::Narrow => 0.011,
            FontRole::ArialN => 0.013,
        }
    }

    /// Resolve a role for a locale: locale override first, then the role's
    /// game face, then the default face.
    pub fn role_face(&self, locale: &str, role: FontRole) -> Option<&FontFace> {
        if let Some(key) = Self::locale_override_key(locale)
            && let Some(face) = self.get(key)
        {
            return Some(face);
        }
        self.roles
            .get(&role)
            .and_then(|name| self.get(name))
            .or_else(|| self.get(&self.default_name))
    }

    /// Resolve a font key for a locale. A game-sourced face (the operator's
    /// FRIZQT__ family, i.e. `MasterFont`/`ChatFont`/`MessageFont`/...) is
    /// replaced by the locale face: ruRU keeps the Cyrillic FRIZQT__, CJK uses
    /// the system face, because the game faces carry no CJK glyphs. Explicit
    /// donor faces (already localised) and unknown keys resolve as written.
    pub fn face_for(&self, name: &str, locale: &str) -> Option<&FontFace> {
        let requested = self.get(name)?;
        if requested.source == FontSource::Game
            && let Some(key) = Self::locale_override_key(locale)
            && let Some(locale_face) = self.get(key)
        {
            return Some(locale_face);
        }
        Some(requested)
    }

    pub fn faces(&self) -> &[FontFace] {
        &self.faces
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

impl HAlign {
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::to_ascii_uppercase).as_deref() {
            Some("JUSTIFYCENTER") | Some("CENTER") => Self::Center,
            Some("JUSTIFYRIGHT") | Some("RIGHT") => Self::Right,
            _ => Self::Left,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

impl VAlign {
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::to_ascii_uppercase).as_deref() {
            Some("JUSTIFYTOP") | Some("TOP") => Self::Top,
            Some("JUSTIFYBOTTOM") | Some("BOTTOM") => Self::Bottom,
            _ => Self::Middle,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontShadow {
    /// Offset in screen fractions (`FontShadowOffset 0.001 -0.001`).
    pub offset: [f32; 2],
    pub color: [f32; 4],
}

/// Fully resolved text style of one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    /// Skin font key (`MasterFont`).
    pub font: String,
    /// Size as a fraction of the screen height.
    pub size: f32,
    pub color: [f32; 4],
    pub halign: HAlign,
    pub valign: VAlign,
    pub shadow: Option<FontShadow>,
    /// `FIXEDSIZE`, `PASSWORDFIELD`, ... (kept for screens that need them).
    pub flags: Vec<String>,
    /// Readability floor in pixels (phone screens).
    pub min_px: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: "MasterFont".to_string(),
            size: DEFAULT_FONT_SIZE,
            color: [1.0, 1.0, 1.0, 1.0],
            halign: HAlign::Left,
            valign: VAlign::Middle,
            shadow: None,
            flags: Vec::new(),
            min_px: DEFAULT_MIN_FONT_PX,
        }
    }
}

impl TextStyle {
    /// Exact FDF/war3 size in pixels (`size` is a fraction of screen height).
    pub fn exact_size_px(&self, screen_height: f32) -> f32 {
        self.size * screen_height
    }

    /// Parity size, optionally raised to [`Self::min_px`] for phone screens.
    pub fn size_px(&self, screen_height: f32) -> f32 {
        self.exact_size_px(screen_height).max(self.min_px)
    }

    /// Resolve this style's face for a locale (`enUS`, `ruRU`, `koKR`, ...).
    /// The style's own size wins; a zero size falls back to the face default.
    pub fn resolve_face(&self, registry: &FontRegistry, locale: &str) -> Option<ResolvedFace> {
        let face = registry
            .face_for(&self.font, locale)
            .or_else(|| registry.role_face(locale, FontRole::Primary))
            .or_else(|| registry.faces().first())?;
        Some(ResolvedFace {
            name: face.name.clone(),
            path: face.path.clone(),
            size: if self.size > 0.0 { self.size } else { face.size },
            source: face.source,
        })
    }

    /// Build a style from the typed FDF properties of a frame. Inheritance is
    /// expected to be resolved by the caller (`ResolvedNode` in `ui.rs`).
    pub fn from_parts(
        font: Option<FdfFontRef>,
        color: Option<[f32; 4]>,
        halign: Option<&str>,
        valign: Option<&str>,
        shadow: Option<FontShadow>,
        flags: Option<&str>,
    ) -> Self {
        let mut style = Self::default();
        if let Some(font) = font {
            style.font = font.name;
            style.size = font.size;
        }
        if let Some(color) = color {
            style.color = color;
        }
        style.halign = HAlign::parse(halign);
        style.valign = VAlign::parse(valign);
        style.shadow = shadow;
        if let Some(flags) = flags {
            style.flags = flags
                .split('|')
                .map(|flag| flag.trim().to_ascii_uppercase())
                .filter(|flag| !flag.is_empty())
                .collect();
        }
        style
    }

    pub fn from_fdf(node: &FdfNode) -> Self {
        let shadow = match (node.font_shadow_offset(), node.font_shadow_color()) {
            (Some(offset), Some(color)) => Some(FontShadow { offset, color }),
            _ => None,
        };
        Self::from_parts(
            node.font(),
            node.font_color(),
            node.font_justification_h(),
            node.font_justification_v(),
            shadow,
            node.font_flags(),
        )
    }

    pub fn with_min_px(mut self, min_px: f32) -> Self {
        self.min_px = min_px;
        self
    }
}

// ---------------------------------------------------------------------------
// Localization (canonical ordered text view)
// ---------------------------------------------------------------------------

/// The ordered FDF game-string table and its retail resolution rules live in
/// `wc3-formats::fdf::FdfText` (one parser, declaration order/duplicates,
/// `COLON_`/`KEY_`, `TRIGSTR_n`, escape decoding). `wc3-re::txt` re-exports the
/// same type as its canonical text view; this alias keeps HUD/FDF call sites on
/// the single implementation.
pub use wc3_formats::fdf::{FdfText as Localization, strip_escape_codes};

// ---------------------------------------------------------------------------
// Shaping
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub color: [f32; 4],
    pub width: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapedLine {
    pub runs: Vec<TextRun>,
    pub width: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapedText {
    pub lines: Vec<ShapedLine>,
    pub width: f32,
    pub height: f32,
    pub line_height: f32,
    pub scale: f32,
    /// Pixel-space shadow, resolved from the style + screen height.
    pub shadow: Option<([f32; 2], [f32; 4])>,
}

impl ShapedText {
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for (index, line) in self.lines.iter().enumerate() {
            if index > 0 {
                out.push('\n');
            }
            for run in &line.runs {
                out.push_str(&run.text);
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|line| line.runs.is_empty())
    }
}

/// CPU text shaper over the HUD font atlas.
///
/// `Font` rasterises a single system font at 30 px and uppercases ASCII; the
/// shaper therefore reports `scale` for screens that want to draw the same
/// geometry themselves (or with a later localised font atlas).
pub struct TextShaper<'a> {
    font: &'a Font,
}

impl<'a> TextShaper<'a> {
    pub fn new(font: &'a Font) -> Self {
        Self { font }
    }

    pub fn font(&self) -> &'a Font {
        self.font
    }

    /// Atlas glyphs are rasterised at 30 px; the fallback bitmap at 14.4 px.
    fn em_size(&self) -> f32 {
        (self.font.line_height / 1.25).max(1.0)
    }

    pub fn scale(&self, size_px: f32) -> f32 {
        size_px / self.em_size()
    }

    pub fn line_height(&self, style: &TextStyle, screen_height: f32) -> f32 {
        let size_px = style.size_px(screen_height);
        (self.font.line_height * self.scale(size_px)).max(size_px)
    }

    /// Shape text for a viewport. `max_width` enables word wrapping; pass
    /// `None` for single-line/unwrapped text.
    pub fn shape(
        &self,
        text: &str,
        style: &TextStyle,
        screen: Vec2,
        max_width: Option<f32>,
    ) -> ShapedText {
        let size_px = style.size_px(screen.y);
        let scale = self.scale(size_px);
        let line_height = (self.font.line_height * scale).max(size_px);
        let shadow = style.shadow.map(|shadow| {
            (
                [shadow.offset[0] * screen.x, -shadow.offset[1] * screen.y],
                shadow.color,
            )
        });
        let mut builder = LineBuilder {
            font: self.font,
            scale,
            style,
            max_width,
            lines: Vec::new(),
            current: Vec::new(),
            current_width: 0.0,
            pending_space: false,
        };
        builder.push_text(text);
        builder.finish();
        let width = builder
            .lines
            .iter()
            .map(|line| line.width)
            .fold(0.0f32, f32::max);
        let height = (builder.lines.len() as f32 * line_height).max(line_height);
        ShapedText {
            lines: builder.lines,
            width,
            height,
            line_height,
            scale,
            shadow,
        }
    }

    pub fn measure(&self, text: &str, style: &TextStyle, screen: Vec2) -> Vec2 {
        let shaped = self.shape(text, style, screen, None);
        Vec2::new(shaped.width, shaped.height)
    }

    /// Draw shaped text into the UI batch, aligned inside `rect`.
    pub fn draw(&self, batch: &mut UiBatch, shaped: &ShapedText, rect: Rect, style: &TextStyle) {
        if shaped.is_empty() {
            return;
        }
        let total_height = shaped.height;
        let top = match style.valign {
            VAlign::Top => rect.y,
            VAlign::Middle => rect.y + (rect.h - total_height) * 0.5,
            VAlign::Bottom => rect.y + rect.h - total_height,
        };
        for (index, line) in shaped.lines.iter().enumerate() {
            let x = match style.halign {
                HAlign::Left => rect.x,
                HAlign::Center => rect.x + (rect.w - line.width) * 0.5,
                HAlign::Right => rect.x + rect.w - line.width,
            };
            let y = top + index as f32 * shaped.line_height;
            if let Some((offset, color)) = shaped.shadow {
                let mut pen = x;
                for run in &line.runs {
                    self.font.text_exact(
                        batch,
                        &run.text,
                        pen + offset[0],
                        y + offset[1],
                        shaped.scale,
                        color,
                    );
                    pen += run.width;
                }
            }
            let mut pen = x;
            for run in &line.runs {
                self.font
                    .text_exact(batch, &run.text, pen, y, shaped.scale, run.color);
                pen += run.width;
            }
        }
    }
}

struct LineBuilder<'a, 'b> {
    font: &'a Font,
    scale: f32,
    style: &'b TextStyle,
    max_width: Option<f32>,
    lines: Vec<ShapedLine>,
    current: Vec<TextRun>,
    current_width: f32,
    pending_space: bool,
}

impl<'a, 'b> LineBuilder<'a, 'b> {
    fn push_text(&mut self, text: &str) {
        let mut color = self.style.color;
        let mut current_word = String::new();
        let mut chars = text.chars().peekable();
        while let Some(character) = chars.next() {
            match character {
                '|' => match chars.next() {
                    Some('n') | Some('N') => {
                        self.flush_word(&mut current_word, color);
                        self.newline();
                    }
                    Some('c') | Some('C') => {
                        self.flush_word(&mut current_word, color);
                        let mut hex = String::with_capacity(8);
                        for _ in 0..8 {
                            if let Some(digit) = chars.next() {
                                hex.push(digit);
                            }
                        }
                        if let Some(parsed) = parse_argb(&hex) {
                            color = parsed;
                        }
                    }
                    Some('r') | Some('R') => {
                        self.flush_word(&mut current_word, color);
                        color = self.style.color;
                    }
                    Some('|') => current_word.push('|'),
                    Some(other) => {
                        current_word.push('|');
                        current_word.push(other);
                    }
                    None => current_word.push('|'),
                },
                '\n' => {
                    self.flush_word(&mut current_word, color);
                    self.newline();
                }
                ' ' | '\t' => {
                    self.flush_word(&mut current_word, color);
                    self.pending_space = true;
                }
                other => current_word.push(other),
            }
        }
        self.flush_word(&mut current_word, color);
    }

    fn flush_word(&mut self, word: &mut String, color: [f32; 4]) {
        if word.is_empty() {
            return;
        }
        let text = std::mem::take(word);
        let width = self.font.measure(&text, self.scale);
        let space = if self.pending_space && !self.current.is_empty() {
            self.font.measure(" ", self.scale)
        } else {
            0.0
        };
        if let Some(max_width) = self.max_width
            && !self.current.is_empty()
            && self.current_width + space + width > max_width
        {
            self.newline();
        }
        if self.pending_space && !self.current.is_empty() {
            self.current_width += space;
            self.push_run(" ".to_string(), color, space);
        }
        self.pending_space = false;
        self.current_width += width;
        self.push_run(text, color, width);
    }

    fn push_run(&mut self, text: String, color: [f32; 4], width: f32) {
        if let Some(last) = self.current.last_mut()
            && last.color == color
        {
            last.text.push_str(&text);
            last.width += width;
            return;
        }
        self.current.push(TextRun { text, color, width });
    }

    fn newline(&mut self) {
        self.pending_space = false;
        let width = self.current_width;
        self.lines.push(ShapedLine {
            runs: std::mem::take(&mut self.current),
            width,
        });
        self.current_width = 0.0;
    }

    fn finish(&mut self) {
        if !self.current.is_empty() || self.lines.is_empty() {
            let width = self.current_width;
            self.lines.push(ShapedLine {
                runs: std::mem::take(&mut self.current),
                width,
            });
        }
    }
}

fn parse_argb(hex: &str) -> Option<[f32; 4]> {
    if hex.len() != 8 {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    let a = ((value >> 24) & 0xFF) as f32 / 255.0;
    let r = ((value >> 16) & 0xFF) as f32 / 255.0;
    let g = ((value >> 8) & 0xFF) as f32 / 255.0;
    let b = (value & 0xFF) as f32 / 255.0;
    Some([r, g, b, a])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_registry_is_case_insensitive_and_has_defaults() {
        let registry = FontRegistry::with_builtin_fonts();
        assert_eq!(
            registry.get("masterfont").unwrap().path,
            "Fonts\\FRIZQT__.TTF"
        );
        assert_eq!(registry.resolve("EscMenuTextFont"), "Fonts\\FRIZQT__.TTF");
        assert_eq!(registry.resolve("MissingFont"), "Fonts\\FRIZQT__.TTF");
        assert!(registry.len() >= 6);
    }

    #[test]
    fn localization_resolves_keys_colon_and_key_prefixes() {
        let mut localization = Localization::new();
        localization.insert("GOLD", "Gold");
        localization.insert("COLON_GOLD", "Gold:");
        localization.insert("KEY_MENU", "Menu");
        assert_eq!(localization.resolve("GOLD"), Some("Gold"));
        assert_eq!(localization.resolve("COLON_GOLD"), Some("Gold:"));
        assert_eq!(localization.resolve("KEY_MENU"), Some("Menu"));
        assert_eq!(localization.resolve("COLON_UNKNOWN"), None);
        assert_eq!(localization.text("COLON_UNKNOWN"), "COLON_UNKNOWN");
        assert_eq!(localization.text("KEY_MENU"), "Menu");
    }

    #[test]
    fn localization_colon_fallback_appends_colon() {
        let mut localization = Localization::new();
        localization.insert("SUPPLY", "Supply");
        assert_eq!(
            localization.resolve_owned("COLON_SUPPLY").as_deref(),
            Some("Supply:")
        );
    }

    #[test]
    fn wts_parsing_and_trigstr() {
        let mut localization = Localization::new();
        let count = localization
            .parse_wts("STRING 1\n{\nHello |nworld\n}\n\nSTRING 2 // note\n{\nSecond\n}\n");
        assert_eq!(count, 2);
        assert_eq!(localization.wts(1), Some("Hello |nworld"));
        assert_eq!(localization.text("TRIGSTR_2"), "Second");
        assert_eq!(localization.text("TRIGSTR_9"), "TRIGSTR_9");
    }

    #[test]
    fn escape_codes_are_stripped() {
        assert_eq!(strip_escape_codes("|cffffcc00Gold|r"), "Gold");
        assert_eq!(strip_escape_codes("a|nb"), "a\nb");
        assert_eq!(strip_escape_codes("a||b"), "a|b");
    }

    #[test]
    fn text_style_from_fdf_parts() {
        let style = TextStyle::from_parts(
            Some(FdfFontRef {
                name: "MasterFont".into(),
                size: 0.013,
                extra: Some(String::new()),
                property: "FrameFont",
            }),
            Some([0.99, 0.827, 0.0705, 1.0]),
            Some("JUSTIFYCENTER"),
            Some("JUSTIFYMIDDLE"),
            Some(FontShadow {
                offset: [0.001, -0.001],
                color: [0.0, 0.0, 0.0, 0.9],
            }),
            Some("FIXEDSIZE"),
        );
        assert_eq!(style.font, "MasterFont");
        assert_eq!(style.size, 0.013);
        assert_eq!(style.halign, HAlign::Center);
        assert_eq!(style.valign, VAlign::Middle);
        assert_eq!(style.flags, vec!["FIXEDSIZE".to_string()]);
    }

    #[test]
    fn fdf_font_size_is_exact_parity() {
        let style = TextStyle::default();
        assert!((style.exact_size_px(1080.0) - 10.8).abs() < 1e-4);
        assert!((style.size_px(1080.0) - 10.8).abs() < 1e-4);
        assert!((style.clone().with_min_px(24.0).size_px(1080.0) - 24.0).abs() < 1e-4);
    }

    #[test]
    fn shaping_wraps_and_splits_colors() {
        let font = Font::load();
        let shaper = TextShaper::new(&font);
        let style = TextStyle::default().with_min_px(10.0);
        let screen = Vec2::new(1280.0, 720.0);
        let shaped = shaper.shape("HELLO WORLD", &style, screen, None);
        assert_eq!(shaped.lines.len(), 1);
        assert!(shaped.width > 0.0);
        let wrapped = shaper.shape(
            "HELLO WORLD AGAIN AND AGAIN",
            &style,
            screen,
            Some(shaped.width * 0.6),
        );
        assert!(wrapped.lines.len() > 1);
        let colored = shaper.shape("|cffff0000RED|r WHITE", &style, screen, None);
        assert_eq!(colored.lines[0].runs.len(), 2);
        assert_eq!(colored.lines[0].runs[0].color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(colored.lines[0].runs[0].text, "RED");
    }

    #[test]
    fn shaping_handles_newlines() {
        let font = Font::load();
        let shaper = TextShaper::new(&font);
        let style = TextStyle::default().with_min_px(10.0);
        let screen = Vec2::new(1280.0, 720.0);
        let shaped = shaper.shape("A|nB\nC", &style, screen, None);
        assert_eq!(shaped.lines.len(), 3);
        assert_eq!(shaped.lines[2].runs[0].text, "C");
    }

    #[test]
    fn cyrillic_round_trip_localization() {
        // UTF-8 Cyrillic survives insert/resolve/owned/WTS parsing unchanged.
        let mut loc = Localization::new();
        let single = "Одиночная игра";
        let colon = "Золото:";
        loc.insert("SINGLEPLAYER", single);
        loc.insert("COLON_GOLD", colon);
        assert_eq!(loc.resolve("SINGLEPLAYER"), Some(single));
        assert_eq!(loc.text("COLON_GOLD"), colon);
        assert_eq!(loc.resolve_owned("SINGLEPLAYER").as_deref(), Some(single));
        // WTS-style round-trip through parser.
        let mut wts = Localization::new();
        let parsed = wts.parse_wts("STRING 1\n{\nОдиночная игра\n}\nSTRING 2\n{\nЗолото: %d\n}\n");
        assert_eq!(parsed, 2);
        assert_eq!(wts.wts(1), Some("Одиночная игра"));
        assert_eq!(wts.text("TRIGSTR_2"), "Золото: %d");
    }

    #[test]
    fn font_registry_roles_and_locale_fallback() {
        let registry = FontRegistry::with_builtin_fonts();
        // Primary role resolves to MasterFont (operator/game path).
        let primary = registry.role_face("enUS", FontRole::Primary).unwrap();
        assert_eq!(primary.name, "MasterFont");
        assert_eq!(primary.path, "Fonts\\FRIZQT__.TTF");
        assert_eq!(primary.source, FontSource::Game);
        // ruRU keeps Cyrillic-capable FRIZQT__.
        assert_eq!(FontRegistry::locale_override_key("ruRU"), Some("FRIZQT__"));
        assert_eq!(FontRegistry::needs_system_font("ruRU"), false);
        let ru = registry.role_face("ruRU", FontRole::Primary).unwrap();
        assert!(ru.path.contains("FRIZQT__"));
        // CJK locales use donor fallback key mapping.
        assert_eq!(FontRegistry::needs_system_font("koKR"), true);
        assert_eq!(FontRegistry::locale_override_key("koKR"), Some("2002"));
        assert_eq!(FontRegistry::locale_override_key("zhCN"), Some("ZYKai_T"));
        assert_eq!(FontRegistry::locale_override_key("zhTW"), Some("bLEI00D"));
        assert!(FontRegistry::role_default_size(FontRole::Primary) > 0.0);
    }

    #[test]
    fn locale_aware_face_resolution_table() {
        let registry = FontRegistry::with_builtin_fonts();
        let style = TextStyle::default(); // MasterFont, 0.013
        let en = style.resolve_face(&registry, "enUS").unwrap();
        assert_eq!(en.name, "MasterFont");
        assert_eq!(en.path, "Fonts\\FRIZQT__.TTF");
        assert_eq!(en.source, FontSource::Game);
        let ru = style.resolve_face(&registry, "ruRU").unwrap();
        assert!(ru.path.contains("FRIZQT__"), "ruRU -> {}", ru.path);
        assert_eq!(ru.source, FontSource::Game);
        let ko = style.resolve_face(&registry, "koKR").unwrap();
        assert!(ko.path.ends_with("2002.TTF"), "koKR -> {}", ko.path);
        let zh = style.resolve_face(&registry, "zhCN").unwrap();
        assert!(zh.path.ends_with("ZYKai_T.TTF"), "zhCN -> {}", zh.path);
        // Unknown key falls back to the role default, never empty.
        let mut missing = TextStyle::default();
        missing.font = "NoSuchFont".into();
        let fallback = missing.resolve_face(&registry, "enUS").unwrap();
        assert_eq!(fallback.name, "MasterFont");
        // A zero style size takes the face default; an explicit size wins.
        let mut sized = TextStyle::default();
        sized.size = 0.0;
        assert!((sized.resolve_face(&registry, "enUS").unwrap().size - 0.013).abs() < 1e-6);
        sized.size = 0.02;
        assert!((sized.resolve_face(&registry, "enUS").unwrap().size - 0.02).abs() < 1e-6);
    }
}
