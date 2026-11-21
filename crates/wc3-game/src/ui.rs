//! Data-driven WC3 FDF UI framework: skin resolution, template inheritance,
//! anchor layout, fonts/localisation and a renderer-independent layout tree.
//!
//! The modern phone HUD lives in [`crate::style`]/`mobile_hud`; this module is
//! the FDF/legacy layer. It never draws chrome by itself: screens ask for a
//! [`UiLayout`] and either render [`LayoutNode::paint`] quads or use the rects
//! for touch hit-testing and dynamic text placement.
//!
//! Pipeline:
//!
//! ```text
//! FdfDocument (wc3-formats)          war3skins.txt
//!        |                                  |
//!        v                                  v
//! templates -> ResolvedTree  <--- SkinTable + Localization + FontRegistry
//!                     |
//!                     v
//!              UiLayout (rects, text, paints, hit-test)
//! ```
//!
//! Retail facts this layer encodes (observed 2026-11-12, 70 extracted FDFs):
//! * 1294 `INHERITS` statements, 388 of them `WITHCHILDREN`, 120 across
//!   different frame kinds (a `GLUECHECKBOX` may inherit a `CHECKBOX`), so
//!   template resolution must not enforce kind equality.
//! * `SetPoint` is repeated: 1050 statements, so per-axis anchor slots are
//!   resolved instead of keeping only the last property.
//! * `FrameFont`/`Font` names are skin keys; `war3skins.txt [Default]` maps
//!   them to `Fonts\FRIZQT__.TTF`.
//! * `Text` values are game-string keys from `StringList` (`GlobalStrings.fdf`
//!   has 922 entries) or literal text.

#[path = "text.rs"]
pub mod text;

/// Pure UI subsystem (hud.md §6): identity, geometry, constraints, the layout
/// solver, the design tokens and the HUD composition. Nothing here touches the
/// GPU, the game state or the clock.
pub mod core;
pub mod layout;
pub mod theme;
pub mod widgets;

use std::collections::{HashMap, HashSet};
use std::ops::Deref;

use glam::Vec2;
use wc3_assetdb::AssetDb;
use wc3_formats::fdf::{self, FdfDocument, FdfNode, FdfProp, FdfSetPoint};
use wc3_sim::UnitKind;

use self::text::{FontShadow, Localization, TextShaper, TextStyle};
pub use crate::assets::TextureImage;
use crate::game::UiAction;
use crate::hud::{Font, Rect, UiBatch};

pub use self::text::{FontFace, FontRole, FontSource, FontRegistry, ResolvedFace, ShapedText, TextRun};

/// DragonUI-derived font registry (port of DragonUI core/fonts.lua): locale-aware
/// family selection with PRIMARY/ACTIONBAR/NARROW families and system fallback.
pub type DragonUiFontRegistry = FontRegistry;

pub type FrameId = usize;

/// Paths the in-game UI needs. Screens can load more documents on demand via
/// [`UiTheme::load_document`]; includes are followed recursively.
pub const DEFAULT_DOCUMENTS: &[&str] = &[
    "UI\\FrameDef\\UI\\ConsoleUI.fdf",
    "UI\\FrameDef\\UI\\ResourceBar.fdf",
    "UI\\FrameDef\\UI\\SimpleInfoPanel.fdf",
    "UI\\FrameDef\\UI\\InfoPanelUnitDetail.fdf",
    "UI\\FrameDef\\UI\\UpperButtonBar.fdf",
    "UI\\FrameDef\\UI\\EscMenuTemplates.fdf",
    "UI\\FrameDef\\UI\\InfoPanelTemplates.fdf",
    "UI\\FrameDef\\GlobalStrings.fdf",
    "UI\\FrameDef\\DateStrings.fdf",
    "UI\\FrameDef\\NetworkStrings.fdf",
];

/// Command-card icons the gameplay layer asks for directly (not referenced by
/// the FDF chrome, so they are loaded explicitly).
pub const DEFAULT_ICONS: &[&str] = &[
    "BTNStop",
    "BTNMove",
    "BTNAttack",
    "BTNHoldPosition",
    "BTNPatrol",
    "BTNFootman",
    "BTNPeasant",
    "BTNArcher",
    "BTNGatherGold",
    "BTNHumanBuild",
    "BTNCancel",
    "BTNTownHall",
];

/// `war3skins.txt` tooltip art keys loaded by [`UiTheme::load_tooltip_textures`].
pub const TOOLTIP_TEXTURE_KEYS: &[&str] = &[
    "ToolTipBackground",
    "ToolTipBorder",
    "ToolTipGoldIcon",
    "ToolTipLumberIcon",
    "ToolTipStonesIcon",
    "ToolTipManaIcon",
    "ToolTipSupplyIcon",
    "UnitTipBackground",
];

/// Small cap so a broken skin file cannot exhaust phone memory.
const MAX_LOADED_TEXTURES: usize = 1400;

/// Abstraction over the game-data reader. `AssetDb` implements it today; the
/// canonical `wc3-re` resource API can implement it without touching this
/// module (see `analysis/agent-ui-framework/INTEGRATION.md`).
pub trait UiResourceResolver {
    fn read_ui_text(&mut self, path: &str) -> Option<String>;
    fn read_ui_texture(&mut self, path: &str) -> Option<TextureImage>;
}

impl UiResourceResolver for AssetDb {
    fn read_ui_text(&mut self, path: &str) -> Option<String> {
        let normalized = path.replace('/', "\\");
        let bytes = self.read(&normalized).or_else(|_| self.read(path)).ok()?;
        if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
            let units: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            return Some(String::from_utf16_lossy(&units));
        }
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn read_ui_texture(&mut self, path: &str) -> Option<TextureImage> {
        let normalized = path.replace('/', "\\");
        let image = self
            .texture(&normalized)
            .or_else(|_| self.texture(path))
            .ok()?;
        Some(TextureImage {
            width: image.width,
            height: image.height,
            rgba: image.rgba,
        })
    }
}

// ---------------------------------------------------------------------------
// Skin table (war3skins.txt)
// ---------------------------------------------------------------------------

/// `war3skins.txt`: `[Section]` blocks of `Key=Value` entries.
///
/// Lookup order is active race section, then `[Default]`, then `[Main]`. The
/// retail file has 262 `[Default]` keys, 27 `[Human]`, 43 `[Orc]`, 49
/// `[NightElf]` and 50 `[Undead]`; race files only override a subset.
/// HUD-facing view over the canonical `wc3_re::skin::SkinTable`.
///
/// This type contains **no parser**: `parse` delegates byte parsing to
/// `wc3_re::skin::SkinTable` and every lookup delegates to its section map.
/// The wrapper only adds the active-race cursor and the FDF texture-extension
/// normalisation that the renderer needs to turn a skin value into a file path.
#[derive(Debug, Clone)]
pub struct SkinTable {
    inner: wc3_re::skin::SkinTable,
    active: String,
    /// Canonical `SkinTable` folds `[Main]` into `skins`; the FDF/HUD surface
    /// still exposes it as a section for compatibility and diagnostics.
    main_skins: String,
}

impl Default for SkinTable {
    fn default() -> Self {
        Self {
            inner: wc3_re::skin::SkinTable::default(),
            active: "Default".to_string(),
            main_skins: String::new(),
        }
    }
}

impl SkinTable {
    pub fn from_canonical(inner: wc3_re::skin::SkinTable) -> Self {
        let active = if inner.section("Default").is_some() {
            "Default".to_string()
        } else {
            inner
                .sections
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| "Default".to_string())
        };
        let main_skins = inner.skins.join(",");
        Self {
            inner,
            active,
            main_skins,
        }
    }

    pub fn canonical(&self) -> &wc3_re::skin::SkinTable {
        &self.inner
    }

    pub fn parse(text: &str) -> Self {
        Self::from_canonical(wc3_re::skin::SkinTable::parse(text.as_bytes()).unwrap_or_default())
    }

    pub fn section_index(&self, section: &str) -> Option<usize> {
        if section.eq_ignore_ascii_case("Main") {
            return Some(self.inner.sections.len());
        }
        self.inner
            .sections
            .keys()
            .position(|name| name.eq_ignore_ascii_case(section))
    }

    pub fn sections(&self) -> impl Iterator<Item = &str> {
        self.inner
            .sections
            .keys()
            .map(String::as_str)
            .chain(std::iter::once("Main"))
    }

    pub fn active_section(&self) -> &str {
        &self.active
    }

    /// Select a race section (`Human`, `Orc`, `NightElf`, `Undead`).
    pub fn set_race(&mut self, race: &str) -> bool {
        if self.inner.section(race).is_some() {
            self.active = race.to_string();
            true
        } else {
            false
        }
    }

    pub fn set_active_section(&mut self, section: &str) -> bool {
        self.set_race(section)
    }

    pub fn get_in(&self, section: &str, key: &str) -> Option<&str> {
        if section.eq_ignore_ascii_case("Main") {
            return key
                .eq_ignore_ascii_case("Skins")
                .then_some(self.main_skins.as_str());
        }
        self.inner.section(section)?.get(key)
    }

    /// Active race, then `[Default]` (canonical lookup order).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.inner.resolve(&self.active, key)
    }

    /// Resolve a decorated FDF file key. Skin values are paths without an
    /// extension (`ConsoleTexture01=UI\Console\Human\HumanUITile01`), so the
    /// HUD adds the texture extension; the path itself is canonical data.
    pub fn resolve_texture(&self, key: &str) -> Option<String> {
        let value = self.inner.resolve(&self.active, key)?;
        let mut path = value.replace('/', "\\");
        if !has_texture_extension(&path) {
            path.push_str(".blp");
        }
        Some(path)
    }

    pub fn resolve_model(&self, key: &str) -> Option<String> {
        self.inner
            .resolve(&self.active, key)
            .map(|value| value.replace('/', "\\"))
    }

    pub fn font_entries(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let Some(section) = self.inner.section("Default") {
            for (key, value) in &section.entries {
                let lower = value.to_ascii_lowercase();
                if lower.ends_with(".ttf")
                    || lower.ends_with(".otf")
                    || lower.starts_with("fonts\\")
                    || lower.contains("\\fonts\\")
                {
                    out.push((key.clone(), value.clone()));
                }
            }
        }
        out.sort();
        out
    }

    pub fn len(&self) -> usize {
        self.inner
            .sections
            .values()
            .map(|section| section.entries.len())
            .sum::<usize>()
            + 1 // synthetic [Main] Skins entry
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Whether a path already carries a known texture extension.
pub fn has_texture_extension(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".blp")
        || lower.ends_with(".tga")
        || lower.ends_with(".dds")
        || lower.ends_with(".png")
        || lower.ends_with(".jpg")
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TemplateEntry {
    node: FdfNode,
    source: String,
}

#[derive(Debug, Clone)]
pub struct TextureInfo {
    pub index: usize,
    pub path: String,
    pub width: u32,
    pub height: u32,
}

/// One resolved tooltip skin entry (`ToolTipBackground`, `ToolTipGoldIcon`, ...).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TooltipArt {
    pub key: String,
    pub path: Option<String>,
    pub texture: Option<usize>,
}

/// Tooltip art resolved from the active `war3skins.txt` section. Text is
/// resolved separately through [`UiTheme::tooltip_text`]/[`UiTheme::format_tooltip`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TooltipSkin {
    pub background: TooltipArt,
    pub border: TooltipArt,
    pub unit_background: TooltipArt,
    pub gold: TooltipArt,
    pub lumber: TooltipArt,
    pub stones: TooltipArt,
    pub mana: TooltipArt,
    pub supply: TooltipArt,
}

/// Renderer-independent tooltip layout: box + text. The caller draws
/// [`Self::text`] with [`crate::ui::text::TextShaper`] and the `TooltipSkin`
/// art; no FDF chrome is drawn here.
#[derive(Debug, Clone, PartialEq)]
pub struct TooltipLayout {
    pub rect: Rect,
    pub content: Rect,
    pub text: ShapedText,
}

/// Sizing for a WC3-style tooltip. The text defaults to the canonical font and
/// a readable 18 px floor; screens can override the style.
#[derive(Debug, Clone, PartialEq)]
pub struct TooltipStyle {
    pub text: TextStyle,
    pub padding: f32,
    pub min_width: f32,
    pub max_width: f32,
    pub line_gap: f32,
}

impl Default for TooltipStyle {
    fn default() -> Self {
        Self::parity()
    }
}

impl TooltipStyle {
    /// Exact war3/FDF tooltip metrics (`MasterFont` 0.011, no phone floor).
    pub fn parity() -> Self {
        let mut text = TextStyle::default();
        text.size = 0.011;
        text.min_px = 0.0;
        Self {
            text,
            padding: 8.0,
            min_width: 120.0,
            max_width: 420.0,
            line_gap: 2.0,
        }
    }

    /// Phone-readable tooltip (>=24 px) while keeping FDF art/strings.
    pub fn phone() -> Self {
        let mut style = Self::parity();
        style.text.min_px = 24.0;
        style
    }
}

/// One FDF menu/button label resolved from `Text`/`NormalText`/`ButtonText`
/// and the canonical StringList, with the exact FDF text style.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuLabel {
    pub frame: String,
    pub text: String,
    /// Localization key as written in FDF, when the label came from a key.
    pub key: Option<String>,
    pub style: TextStyle,
}

/// Loaded FDF documents, templates, skin, strings, fonts and textures.
#[derive(Debug, Clone, Default)]
pub struct UiTheme {
    skin: SkinTable,
    docs: Vec<FdfDocument>,
    templates: HashMap<String, TemplateEntry>,
    template_index_ci: HashMap<String, String>,
    strings: Localization,
    fonts: FontRegistry,
    textures: HashMap<String, TextureInfo>,
    texture_paths: HashMap<String, TextureInfo>,
    /// Nine-patch corner insets of every loaded texture, keyed like `textures`.
    ///
    /// A frame texture (`CommandButtonBorder`, `ToolTipBorder`, `ConsoleTexture01`,
    /// ...) is drawn into whatever rect the FDF layout asks for, so its corners
    /// must be known here, at load time, instead of guessed at draw time. The
    /// insets are measured from the decoded art by `ui_atlas::detect_insets`.
    frame_insets: HashMap<String, [f32; 4]>,
    /// FourCC → command-button icon name (`hfoo` → `BTNFootman`), filled from the
    /// typed object database so data-driven menus resolve their own icons.
    icon_aliases: HashMap<String, String>,
    /// Problems met while resolving (missing include/parse error/template).
    pub warnings: Vec<String>,
}

impl UiTheme {
    /// Load the in-game FDFs, skin and strings from the game data.
    pub fn load<R: UiResourceResolver>(
        db: &mut R,
        textures: &mut Vec<TextureImage>,
    ) -> Self {
        Self::load_with(db, textures)
    }

    /// Resolver-generic loader (the canonical `wc3-re` API can be adapted with
    /// two methods, see [`UiResourceResolver`]).
    pub fn load_with<R: UiResourceResolver>(
        resolver: &mut R,
        textures: &mut Vec<TextureImage>,
    ) -> Self {
        let mut theme = Self::default();
        if let Some(text) = resolver.read_ui_text("UI\\war3skins.txt") {
            theme.skin = SkinTable::parse(&text);
        }
        theme.rebuild_fonts();
        for path in DEFAULT_DOCUMENTS {
            theme.load_document(resolver, path, 0);
        }
        theme.rebuild_strings();
        theme.collect_templates();
        theme.load_missing_textures(resolver, textures);
        for icon in DEFAULT_ICONS {
            theme.load_command_icon(resolver, textures, icon);
        }
        // WC3 console art through the canonical skin: ConsoleTexture01..04 are
        // the race-tinted keys from war3skins.txt; the fallback path is the
        // Human tile that the key resolves to on retail. No parallel parser.
        for index in 1..=4u32 {
            let key = format!("ConsoleTexture{index:02}");
            let alias = format!("HumanUITile{index:02}");
            if let Some(loaded) = theme.load_texture_key(resolver, textures, &key, true)
                && let Some(info) = theme.texture_info(&key).cloned()
            {
                theme.textures.insert(alias, info);
                let _ = loaded;
                continue;
            }
            let fallback = format!("UI\\Console\\Human\\HumanUITile{index:02}.blp");
            if theme
                .load_alias(resolver, textures, &alias, &[fallback.as_str()])
                .is_none()
            {
                theme
                    .warnings
                    .push(format!("console_texture_missing={alias}"));
            }
        }
        for (alias, candidate) in [
            (
                "CommandButtonBorder",
                "UI\\Widgets\\Console\\Human\\CommandButton\\human-multipleselection-border.blp",
            ),
            (
                "PortraitFrame",
                "UI\\Console\\Human\\HumanUIPortraitMask.blp",
            ),
            (
                "TooltipBackground",
                "UI\\Widgets\\ToolTips\\Human\\human-tooltip-background.blp",
            ),
            (
                "TooltipBorder",
                "UI\\Widgets\\ToolTips\\Human\\human-tooltip-border.blp",
            ),
            ("ResourceGold", "UI\\Feedback\\Resources\\ResourceGold.blp"),
            (
                "ResourceLumber",
                "UI\\Feedback\\Resources\\ResourceLumber.blp",
            ),
            (
                "ResourceSupply",
                "UI\\Feedback\\Resources\\ResourceSupply.blp",
            ),
        ] {
            if theme
                .load_alias(resolver, textures, alias, &[candidate])
                .is_none()
            {
                theme
                    .warnings
                    .push(format!("console_texture_missing={alias}"));
            }
        }
        // HUD redraw v0.1 pack aliases (optional). Missing files stay None so
        // the retail art/procedural bevel path is used unchanged.
        for (alias, candidate) in [
            ("HudRedrawTopbar", "UI\\Console\\Human\\HumanTopbarPlate.blp"),
            ("HudRedrawPill", "UI\\Console\\Human\\HumanResourcePill.blp"),
            ("HudRedrawPillWarn", "UI\\Console\\Human\\HumanResourcePillWarn.blp"),
            ("HudRedrawGold", "UI\\Feedback\\Resources\\ResourceGoldRedraw.blp"),
            ("HudRedrawGoldWarn", "UI\\Feedback\\Resources\\ResourceGoldWarnRedraw.blp"),
            ("HudRedrawLumber", "UI\\Feedback\\Resources\\ResourceLumberRedraw.blp"),
            ("HudRedrawLumberWarn", "UI\\Feedback\\Resources\\ResourceLumberWarnRedraw.blp"),
            ("HudRedrawFood", "UI\\Feedback\\Resources\\ResourceFoodRedraw.blp"),
            ("HudRedrawFoodWarn", "UI\\Feedback\\Resources\\ResourceFoodWarnRedraw.blp"),
            (
                "HudRedrawButtonNormal",
                "UI\\Widgets\\Console\\Human\\CommandButton\\human-button-normal.blp",
            ),
            (
                "HudRedrawButtonPressed",
                "UI\\Widgets\\Console\\Human\\CommandButton\\human-button-pressed.blp",
            ),
            (
                "HudRedrawButtonDisabled",
                "UI\\Widgets\\Console\\Human\\CommandButton\\human-button-disabled.blp",
            ),
            (
                "HudRedrawButtonCooldown",
                "UI\\Widgets\\Console\\Human\\CommandButton\\human-button-cooldown.blp",
            ),
            (
                "HudRedrawButtonAutocast",
                "UI\\Widgets\\Console\\Human\\CommandButton\\human-button-autocast.blp",
            ),
            (
                "HudRedrawButtonEmpty",
                "UI\\Widgets\\Console\\Human\\CommandButton\\human-button-empty.blp",
            ),
            (
                "HudRedrawButtonGlow",
                "UI\\Widgets\\Console\\Human\\CommandButton\\human-button-glow.blp",
            ),
            (
                "HudRedrawCooldownRadial",
                "UI\\Widgets\\Console\\Human\\CommandButton\\human-cooldown-radial.blp",
            ),
            ("HudRedrawPortraitNormal", "UI\\Console\\Human\\HumanUIPortraitMaskNormal.blp"),
            ("HudRedrawPortraitHero", "UI\\Console\\Human\\HumanUIPortraitMaskHero.blp"),
            ("HudRedrawPortraitEnemy", "UI\\Console\\Human\\HumanUIPortraitMaskEnemy.blp"),
            ("HudRedrawMinimapFrame", "UI\\Console\\Human\\HumanMinimapFrame.blp"),
            ("HudRedrawMinimapFrameAlert", "UI\\Console\\Human\\HumanMinimapFrameAlert.blp"),
            ("HudRedrawMinimapHeader", "UI\\Console\\Human\\HumanMinimapHeader.blp"),
            ("HudRedrawSelectionPanel", "UI\\Console\\Human\\HumanSelectionPanel.blp"),
            (
                "HudRedrawGroupInactive",
                "UI\\Console\\Human\\HumanControlGroupButtonInactive.blp",
            ),
            (
                "HudRedrawGroupActive",
                "UI\\Console\\Human\\HumanControlGroupButtonActive.blp",
            ),
            (
                "HudRedrawTooltipPanel",
                "UI\\Widgets\\ToolTips\\Human\\human-tooltip-panel.blp",
            ),
            ("HudRedrawProgress", "UI\\Console\\Human\\HumanBuildProgress.blp"),
        ] {
            let _ = theme.load_alias(resolver, textures, alias, &[candidate]);
        }
        theme.load_tooltip_textures(resolver, textures);
        theme
    }

    /// Load the first candidate path and expose it under a stable alias.
    fn load_alias<R: UiResourceResolver>(
        &mut self,
        resolver: &mut R,
        textures: &mut Vec<TextureImage>,
        alias: &str,
        candidates: &[&str],
    ) -> Option<usize> {
        for candidate in candidates {
            if let Some(index) = self.load_texture_key(resolver, textures, candidate, false)
                && let Some(info) = self.texture_info(candidate).cloned()
            {
                self.textures.insert(alias.to_string(), info);
                return Some(index);
            }
        }
        None
    }

    /// Load the `ToolTip*` art from `war3skins.txt` (background, border,
    /// resource icons, unit tooltip background). The keys are canonical skin
    /// keys; no hardcoded Human path list is used here.
    pub fn load_tooltip_textures<R: UiResourceResolver>(
        &mut self,
        resolver: &mut R,
        textures: &mut Vec<TextureImage>,
    ) -> usize {
        let mut loaded = 0usize;
        for key in TOOLTIP_TEXTURE_KEYS {
            let before = self.textures.len();
            self.load_texture_key(resolver, textures, key, true);
            if self.textures.len() > before {
                loaded += 1;
            }
        }
        loaded
    }

    fn tooltip_art(&self, key: &str) -> TooltipArt {
        TooltipArt {
            key: key.to_string(),
            path: self.skin.resolve_texture(key),
            texture: self.texture(key),
        }
    }

    /// Resolve the complete tooltip art set for the active race.
    pub fn tooltip_skin(&self) -> TooltipSkin {
        TooltipSkin {
            background: self.tooltip_art("ToolTipBackground"),
            border: self.tooltip_art("ToolTipBorder"),
            unit_background: self.tooltip_art("UnitTipBackground"),
            gold: self.tooltip_art("ToolTipGoldIcon"),
            lumber: self.tooltip_art("ToolTipLumberIcon"),
            stones: self.tooltip_art("ToolTipStonesIcon"),
            mana: self.tooltip_art("ToolTipManaIcon"),
            supply: self.tooltip_art("ToolTipSupplyIcon"),
        }
    }

    /// Resolve a tooltip game-string key (`REQUIRESTOOLTIP`, ...) through the
    /// canonical `StringList` table. Escape codes remain for the shaper.
    pub fn tooltip_text(&self, key: &str) -> String {
        self.strings.text(key)
    }

    /// [`Self::tooltip_text`] with WC3 colour/newline escapes removed.
    pub fn tooltip_plain(&self, key: &str) -> String {
        self.strings.plain(key)
    }

    /// Resolve a tooltip string and substitute `%s`/`%d`/`%u` arguments in
    /// order (WC3 tooltip formatting). `%%` is a literal percent.
    pub fn format_tooltip(&self, key: &str, args: &[&str]) -> String {
        format_tooltip_text(&self.strings.text(key), args)
    }

    /// Lay out a tooltip box next to `anchor` inside `screen`. The renderer
    /// draws `rect` with `tooltip_skin().background/border` and `content`/text
    /// with [`crate::ui::text::TextShaper`].
    pub fn layout_tooltip(
        &self,
        text: &str,
        anchor: Vec2,
        screen: Vec2,
        font: &Font,
        style: &TooltipStyle,
    ) -> TooltipLayout {
        let shaper = TextShaper::new(font);
        let padding = style.padding.max(0.0);
        let max_text_width = (style.max_width - padding * 2.0).max(32.0);
        let shaped = shaper.shape(text, &style.text, screen, Some(max_text_width));
        let content_width = shaped
            .width
            .max(style.min_width - padding * 2.0)
            .min(max_text_width);
        let content_height = shaped.height.max(style.text.size_px(screen.y));
        let width = (content_width + padding * 2.0).min(screen.x.max(1.0));
        let height = (content_height + padding * 2.0 + style.line_gap).min(screen.y.max(1.0));
        let x = anchor
            .x
            .clamp(0.0, (screen.x - width).max(0.0));
        let y = anchor
            .y
            .clamp(0.0, (screen.y - height).max(0.0));
        TooltipLayout {
            rect: Rect::new(x, y, width, height),
            content: Rect::new(x + padding, y + padding, content_width, content_height),
            text: shaped,
        }
    }

    /// Draw a tooltip with the resolved `war3skins` art and the FDF/Font text
    /// shaper. Text is always drawn; art textures are used when loaded.
    pub fn draw_tooltip(
        &self,
        batch: &mut UiBatch,
        layout: &TooltipLayout,
        skin: &TooltipSkin,
        font: &Font,
        style: &TooltipStyle,
    ) {
        if let Some(index) = skin.background.texture {
            batch.textured_quad(index, layout.rect, [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
        }
        if let Some(index) = skin.border.texture {
            batch.textured_quad(index, layout.rect, [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
        }
        let shaper = TextShaper::new(font);
        shaper.draw(batch, &layout.text, layout.content, &style.text);
    }

    // -- Menu strings / skin values ----------------------------------------

    /// Raw `war3skins.txt` value for a key (e.g. `MainMenu`, `MainMenuButton`,
    /// `MenuBackground`). Decoration of texture keys stays in
    /// [`Self::resolve_texture_key`].
    pub fn skin_string(&self, key: &str) -> Option<&str> {
        self.skin.get(key)
    }

    /// Every FDF label in a screen: `Text` on text frames and
    /// `NormalText`/`ButtonText` on buttons, resolved through StringList.
    /// This is the menu-string source for the Glue screens (MainMenu,
    /// OptionsMenu, ...), not hardcoded English.
    pub fn menu_labels(&self, root: &str) -> Vec<MenuLabel> {
        let Some(tree) = self.resolve_tree(root) else {
            return Vec::new();
        };
        // A button's `ButtonText` child is rendered as the button label; do not
        // report the same string twice in the menu-string list.
        let mut button_text_targets = HashSet::new();
        for id in tree.preorder() {
            if let Some(target) = tree.node(id).button_text() {
                button_text_targets.insert(target.to_string());
            }
        }
        let mut labels = Vec::new();
        for id in tree.preorder() {
            let node = tree.node(id);
            if is_text_kind(&node.kind)
                && node
                    .name
                    .as_deref()
                    .is_some_and(|name| button_text_targets.contains(name))
            {
                continue;
            }
            if let Some(label) = self.node_menu_label(&tree, node) {
                labels.push(label);
            }
        }
        labels
    }

    /// Resolve one frame label by name inside `root`.
    pub fn menu_label(&self, root: &str, frame: &str) -> Option<MenuLabel> {
        let tree = self.resolve_tree(root)?;
        let id = tree.find(frame)?;
        self.node_menu_label(&tree, tree.node(id))
    }

    fn node_menu_label(
        &self,
        tree: &ResolvedTree,
        node: &crate::ui::ResolvedNode,
    ) -> Option<MenuLabel> {
        let frame = node
            .name
            .as_deref()
            .filter(|name| !name.is_empty())?
            .to_string();
        if is_button_kind(&node.kind) {
            let (text, labels) = label_for_button(
                self,
                tree,
                node,
                &TextBindings::new(),
            );
            let key = node
                .button_text()
                .map(str::to_string)
                .or_else(|| node.normal_text().map(|(_, key)| key.to_string()));
            // `ButtonText` labels are owned/drawn by the child TEXT frame; the
            // button metadata still carries the string for menu parity.
            let (text, style) = if let Some(text) = text {
                (text.text, text.style)
            } else {
                let text = labels.normal?;
                let style = node
                    .button_text()
                    .and_then(|target| tree.find(target))
                    .map(|target| {
                        style_from_node(
                            tree.node(target),
                            crate::ui::text::DEFAULT_MIN_FONT_PX,
                        )
                    })
                    .unwrap_or_else(|| {
                        style_from_node(node, crate::ui::text::DEFAULT_MIN_FONT_PX)
                    });
                (text, style)
            };
            return Some(MenuLabel {
                frame,
                text,
                key,
                style,
            });
        }
        if is_text_kind(&node.kind) {
            let raw = node.text()?;
            return Some(MenuLabel {
                frame,
                text: self.strings.text(raw),
                key: Some(raw.to_string()),
                style: style_from_node(node, crate::ui::text::DEFAULT_MIN_FONT_PX),
            });
        }
        None
    }

    /// Build a theme from already parsed documents (tests, tools, custom
    /// screens loaded outside the MPQ).
    pub fn from_documents(documents: Vec<FdfDocument>, skin: SkinTable) -> Self {
        let mut theme = Self {
            skin,
            docs: documents,
            ..Self::default()
        };
        theme.rebuild_fonts();
        theme.rebuild_strings();
        theme.collect_templates();
        theme
    }

    pub fn skin(&self) -> &SkinTable {
        &self.skin
    }

    pub fn skin_mut(&mut self) -> &mut SkinTable {
        &mut self.skin
    }

    pub fn set_skin(&mut self, skin: SkinTable) {
        self.skin = skin;
        self.rebuild_fonts();
    }

    /// Race-specific skin selection (`Human`, `Orc`, `NightElf`, `Undead`).
    pub fn set_race(&mut self, race: &str) -> bool {
        self.skin.set_race(race)
    }

    pub fn localization(&self) -> &Localization {
        &self.strings
    }

    pub fn localization_mut(&mut self) -> &mut Localization {
        &mut self.strings
    }

    pub fn fonts(&self) -> &FontRegistry {
        &self.fonts
    }

    /// Resolve a text style to the face path/design size for a locale
    /// (`enUS`, `ruRU`, `koKR`, `zhCN`, `zhTW`). This consumes the centralized
    /// registry (DragonUI-pattern port) so FDF frames, console text and
    /// tooltips share one locale-aware font source instead of re-deriving
    /// font paths.
    pub fn resolve_text_face(&self, style: &TextStyle, locale: &str) -> Option<ResolvedFace> {
        style.resolve_face(&self.fonts, locale)
    }

    /// Console/chat text style from retail `war3skins.txt` (`ChatFont`) at the
    /// exact FDF console size (0.011 of screen height). No phone floor here:
    /// the HUD draw path applies its own 24 px readability floor.
    pub fn console_text_style(&self) -> TextStyle {
        let mut style = TextStyle::default();
        style.font = "ChatFont".to_string();
        style.size = 0.011;
        style.min_px = 0.0;
        style
    }

    /// Console/chat face for a locale — ruRU keeps the Cyrillic FRIZQT__,
    /// CJK resolves through the donor fallback key.
    pub fn console_text_face(&self, locale: &str) -> Option<ResolvedFace> {
        self.resolve_text_face(&self.console_text_style(), locale)
    }

    /// Tooltip text face for a locale. `TooltipStyle::parity()` keeps the
    /// exact 0.011 `MasterFont` size through resolution.
    pub fn tooltip_text_face(&self, style: &TooltipStyle, locale: &str) -> Option<ResolvedFace> {
        self.resolve_text_face(&style.text, locale)
    }

    pub fn documents(&self) -> &[FdfDocument] {
        &self.docs
    }

    pub fn doc_count(&self) -> usize {
        self.docs.len()
    }

    pub fn template_count(&self) -> usize {
        self.templates.len()
    }

    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    pub fn texture_info(&self, name: &str) -> Option<&TextureInfo> {
        self.textures
            .get(name)
            .or_else(|| self.textures.get(&name.to_ascii_lowercase()))
    }

    /// Parse and add one FDF plus its include graph. Returns `true` when the
    /// document itself parsed.
    pub fn load_document<R: UiResourceResolver>(
        &mut self,
        resolver: &mut R,
        path: &str,
        depth: usize,
    ) -> bool {
        let normalized = path.replace('/', "\\");
        if depth > 8
            || self
                .docs
                .iter()
                .any(|doc| doc.includer.eq_ignore_ascii_case(&normalized))
        {
            return false;
        }
        let Some(text) = resolver.read_ui_text(&normalized) else {
            if depth == 0 {
                self.warnings.push(format!("FDF not found: {normalized}"));
            }
            return false;
        };
        let Ok(mut document) = FdfDocument::parse_named(&text, &normalized) else {
            self.warnings
                .push(format!("FDF parse failed: {normalized}"));
            return false;
        };
        document.includer = normalized.clone();
        let includes = document.includes.clone();
        self.docs.push(document);
        for include in includes {
            self.load_document(resolver, &include, depth + 1);
        }
        true
    }

    pub fn add_document(&mut self, document: FdfDocument) {
        self.docs.push(document);
        self.rebuild_strings();
        self.collect_templates();
    }

    pub fn rebuild_strings(&mut self) {
        self.strings = Localization::new();
        for document in &self.docs {
            self.strings.parse_string_list(document);
        }
    }

    pub fn collect_templates(&mut self) {
        self.templates.clear();
        self.template_index_ci.clear();
        for document in &self.docs {
            for node in &document.nodes {
                let Some(name) = node.name.as_deref().filter(|name| !name.is_empty()) else {
                    continue;
                };
                if self.templates.contains_key(name) {
                    continue;
                }
                let key = name.to_string();
                self.template_index_ci
                    .entry(key.to_ascii_lowercase())
                    .or_insert_with(|| key.clone());
                self.templates.insert(
                    key,
                    TemplateEntry {
                        node: node.clone(),
                        source: document.includer.clone(),
                    },
                );
            }
        }
    }

    fn rebuild_fonts(&mut self) {
        self.fonts = FontRegistry::with_builtin_fonts();
        self.fonts.merge(self.skin.font_entries());
    }

    pub fn template(&self, name: &str) -> Option<&FdfNode> {
        self.templates
            .get(name)
            .map(|entry| &entry.node)
            .or_else(|| {
                self.template_index_ci
                    .get(&name.to_ascii_lowercase())
                    .and_then(|key| self.templates.get(key))
                    .map(|entry| &entry.node)
            })
    }

    /// Fully resolved properties of a template (used for `NormalText`
    /// template styles and diagnostics). Children are not resolved here.
    pub fn template_props(&self, name: &str) -> Option<Vec<FdfProp>> {
        self.template_props_inner(name, &mut Vec::new())
    }

    fn template_props_inner(&self, name: &str, stack: &mut Vec<String>) -> Option<Vec<FdfProp>> {
        if stack.iter().any(|entry| entry.eq_ignore_ascii_case(name)) {
            return None;
        }
        let node = self.template(name)?;
        let mut props = Vec::new();
        if let Some(base) = &node.inherits
            && let Some(base_props) = self.template_props_inner(base, stack)
        {
            props = base_props;
        }
        merge_props(&mut props, &node.props);
        Some(props)
    }

    pub fn find_root_node(&self, name: &str) -> Option<(&FdfNode, &str)> {
        for document in &self.docs {
            if let Some(node) = document.find(name).or_else(|| document.find_ci(name)) {
                return Some((node, document.includer.as_str()));
            }
        }
        None
    }

    pub fn has_root(&self, name: &str) -> bool {
        self.find_root_node(name).is_some()
    }

    pub fn texture(&self, name: &str) -> Option<usize> {
        self.texture_info(name).map(|info| info.index)
    }

    /// Authored nine-patch corner insets of a loaded frame texture
    /// (`[left, top, right, bottom]` in source pixels).
    pub fn frame_insets(&self, name: &str) -> Option<[f32; 4]> {
        self.frame_insets.get(name).copied()
    }

    /// Nine-patch geometry of a frame texture: index, atlas size and insets.
    pub fn frame_patch(&self, name: &str) -> Option<crate::hud::NinePatch> {
        let info = self.texture_info(name)?;
        let insets = self.frame_insets(name)?;
        Some(crate::hud::NinePatch::full(
            info.index,
            [info.width as f32, info.height as f32],
            insets,
        ))
    }

    /// Resolve an FDF texture key. `decorate` means the key goes through the
    /// skin table (`ConsoleTexture01` -> `UI\Console\Human\HumanUITile01.blp`).
    pub fn resolve_texture_key(&self, key: &str, decorate: bool) -> Option<String> {
        if key.trim().is_empty() {
            return None;
        }
        if decorate {
            return self.skin.resolve_texture(key).or_else(|| {
                let mut path = key.replace('/', "\\");
                if !has_texture_extension(&path) {
                    path.push_str(".blp");
                }
                Some(path)
            });
        }
        let mut path = key.replace('/', "\\");
        if !has_texture_extension(&path) {
            path.push_str(".blp");
        }
        Some(path)
    }

    /// Load one skin key or path into the external texture vector.
    pub fn load_texture_key<R: UiResourceResolver>(
        &mut self,
        resolver: &mut R,
        textures: &mut Vec<TextureImage>,
        key: &str,
        decorate: bool,
    ) -> Option<usize> {
        let path = self.resolve_texture_key(key, decorate)?;
        // The key may already be loaded for a different race: re-resolve the
        // skin path and reload when the race section changed it
        // (ConsoleTexture01 -> Human/Orc/NightElf/Undead tiles).
        if let Some(existing) = self.texture_info(key)
            && existing.path.eq_ignore_ascii_case(&path)
        {
            return Some(existing.index);
        }
        if let Some(info) = self.texture_paths.get(&path.to_ascii_lowercase()) {
            let info = info.clone();
            let index = info.index;
            self.textures.insert(key.to_string(), info);
            return Some(index);
        }
        if self.texture_paths.len() >= MAX_LOADED_TEXTURES {
            return None;
        }
        let image = resolver.read_ui_texture(&path)?;
        let index = textures.len();
        let info = TextureInfo {
            index,
            path: path.clone(),
            width: image.width,
            height: image.height,
        };
        if let Ok(rgba) =
            crate::ui_atlas::Rgba8::from_rgba(image.width, image.height, image.rgba.clone())
        {
            let rect = crate::hud::Rect::new(
                0.0,
                0.0,
                image.width as f32,
                image.height as f32,
            );
            let insets = crate::ui_atlas::detect_insets(
                &rgba,
                rect,
                crate::ui_atlas::ChromaKey::MAGENTA,
            );
            self.frame_insets.insert(key.to_string(), insets);
            self.frame_insets.insert(key.to_ascii_lowercase(), insets);
        }
        textures.push(image);
        self.textures.insert(key.to_string(), info.clone());
        self.textures.insert(key.to_ascii_lowercase(), info.clone());
        self.texture_paths
            .insert(path.to_ascii_lowercase(), info.clone());
        Some(index)
    }

    /// Load every texture referenced by the currently loaded documents.
    pub fn load_missing_textures<R: UiResourceResolver>(
        &mut self,
        resolver: &mut R,
        textures: &mut Vec<TextureImage>,
    ) -> usize {
        let mut references = Vec::new();
        for document in &self.docs {
            for node in &document.nodes {
                collect_texture_refs(node, false, &mut references);
            }
        }
        references.sort();
        references.dedup();
        let mut loaded = 0usize;
        for (key, decorate) in references {
            let before = self.textures.len();
            self.load_texture_key(resolver, textures, &key, decorate);
            if self.textures.len() > before {
                loaded += 1;
            }
        }
        loaded
    }

    /// Canonical command-button path (retail install layout). Object-data can
    /// override it; the HUD uses this as the single default, not a search tree.
    pub fn command_icon_paths(name: &str) -> Vec<String> {
        vec![format!("ReplaceableTextures\\CommandButtons\\{name}.blp")]
    }

    /// Load a command-card icon by name, probing the classic paths.
    pub fn load_command_icon<R: UiResourceResolver>(
        &mut self,
        resolver: &mut R,
        textures: &mut Vec<TextureImage>,
        name: &str,
    ) -> Option<usize> {
        if let Some(index) = self.texture(name) {
            return Some(index);
        }
        self.load_texture_key(resolver, textures, name, false);
        if let Some(index) = self.texture(name) {
            return Some(index);
        }
        if self.texture_paths.len() >= MAX_LOADED_TEXTURES {
            return None;
        }
        for path in Self::command_icon_paths(name) {
            if let Some(info) = self.texture_paths.get(&path.to_ascii_lowercase()).cloned() {
                self.textures.insert(name.to_string(), info.clone());
                return Some(info.index);
            }
            let Some(image) = resolver.read_ui_texture(&path) else {
                continue;
            };
            let index = textures.len();
            let info = TextureInfo {
                index,
                path: path.clone(),
                width: image.width,
                height: image.height,
            };
            textures.push(image);
            self.textures.insert(name.to_string(), info.clone());
            self.textures
                .insert(name.to_ascii_lowercase(), info.clone());
            self.texture_paths
                .insert(path.to_ascii_lowercase(), info.clone());
            return Some(index);
        }
        None
    }

    pub fn unit_icon(&self, kind: UnitKind) -> Option<usize> {
        let name = match kind {
            UnitKind::Footman => "BTNFootman",
            UnitKind::Archer => "BTNArcher",
            UnitKind::Peasant => "BTNPeasant",
            UnitKind::TownHall => "BTNTownHall",
        };
        self.texture(name)
    }

    /// Register the command-button icon of a WC3 type id.
    pub fn set_icon_alias(&mut self, id: &str, icon_name: &str) {
        self.icon_aliases
            .insert(id.to_ascii_lowercase(), icon_name.to_string());
    }

    /// Command-button icon name of a WC3 type id (`hhou` → `BTNFarm`).
    pub fn type_icon_name(&self, id: &str) -> Option<&str> {
        self.icon_aliases
            .get(&id.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// Command-button icon of a WC3 type id, when the texture is loaded.
    pub fn type_icon(&self, id: &str) -> Option<usize> {
        let name = self.type_icon_name(id)?;
        self.texture(name)
    }

    pub fn command_icon(&self, action: &UiAction) -> Option<usize> {
        // Data-driven card slots carry a WC3 type id; the alias table is filled
        // from `UnitData`/profile `Art` at load time.
        match action {
            UiAction::TrainDef(id) | UiAction::BuildDef(id) => {
                if let Some(icon) = self.type_icon(id) {
                    return Some(icon);
                }
            }
            _ => {}
        }
        let name = match action {
            UiAction::Stop => "BTNStop",
            UiAction::SelectArmy => "BTNMove",
            UiAction::CentreBase => "BTNHumanBuild",
            UiAction::Gather => "BTNGatherGold",
            UiAction::AttackMove => "BTNAttack",
            UiAction::Hold => "BTNHoldPosition",
            UiAction::Train(UnitKind::Footman) => "BTNFootman",
            UiAction::Train(UnitKind::Archer) => "BTNArcher",
            UiAction::Train(UnitKind::Peasant) => "BTNPeasant",
            UiAction::Build(_) => "BTNHumanBuild",
            UiAction::BuildDef("hhou") => "BTNFarm",
            UiAction::BuildDef("hgtw") => "BTNGuardTower",
            UiAction::BuildDef("hbar") => "BTNHumanBarracks",
            UiAction::TrainDef("hfoo") => "BTNFootman",
            UiAction::TrainDef("hrif") => "BTNArcher",
            UiAction::TrainDef("hpea") => "BTNPeasant",
            UiAction::Cast(wc3_sim::Spell::HolyLight) => "BTNHolyBolt",
            UiAction::Cast(wc3_sim::Spell::DivineShield) => "BTNDivineShield",
            UiAction::Cast(wc3_sim::Spell::Blizzard) => "BTNBlizzard",
            UiAction::Cast(wc3_sim::Spell::ChainLightning) => "BTNChainLightning",
            UiAction::Cast(_) => "BTNSpell",
            _ => return None,
        };
        self.texture(name)
    }

    // -- resolved tree ------------------------------------------------------

    pub fn resolve_tree(&self, root: &str) -> Option<ResolvedTree> {
        let (node, source) = self.find_root_node(root)?;
        Some(self.resolve_node_tree(node, source))
    }

    pub fn resolve_node_tree(&self, node: &FdfNode, source: &str) -> ResolvedTree {
        let mut builder = TreeBuilder {
            theme: self,
            nodes: Vec::new(),
            by_name: HashMap::new(),
            by_name_ci: HashMap::new(),
            warnings: Vec::new(),
        };
        let mut stack = Vec::new();
        let subtree = builder.build_subtree(node, source, &mut stack);
        let root_id = builder.emit(subtree, None);
        ResolvedTree {
            nodes: builder.nodes,
            roots: vec![root_id],
            by_name: builder.by_name,
            by_name_ci: builder.by_name_ci,
            warnings: builder.warnings,
        }
    }

    /// Resolve a document's frames into one tree per top-level frame.
    pub fn resolve_document(&self, document: &FdfDocument) -> ResolvedTree {
        let mut builder = TreeBuilder {
            theme: self,
            nodes: Vec::new(),
            by_name: HashMap::new(),
            by_name_ci: HashMap::new(),
            warnings: Vec::new(),
        };
        let mut roots = Vec::new();
        for node in &document.nodes {
            if node.name.as_deref().is_none_or(str::is_empty) {
                continue;
            }
            let mut stack = Vec::new();
            let subtree = builder.build_subtree(node, &document.includer, &mut stack);
            roots.push(builder.emit(subtree, None));
        }
        ResolvedTree {
            nodes: builder.nodes,
            roots,
            by_name: builder.by_name,
            by_name_ci: builder.by_name_ci,
            warnings: builder.warnings,
        }
    }

    // -- layout -------------------------------------------------------------

    /// Resolve `root` and lay it out against the configured screen.
    pub fn build_layout(
        &self,
        root: &str,
        config: &LayoutConfig,
        bindings: &TextBindings,
        font: Option<&Font>,
    ) -> Option<UiLayout> {
        let tree = self.resolve_tree(root)?;
        let root_id = *tree.roots.first()?;
        Some(self.layout_tree(&tree, root_id, config, bindings, font))
    }

    /// Lay out one resolved node (and its subtree).
    pub fn layout_tree(
        &self,
        tree: &ResolvedTree,
        root: FrameId,
        config: &LayoutConfig,
        bindings: &TextBindings,
        font: Option<&Font>,
    ) -> UiLayout {
        build_layout(self, tree, root, config, bindings, font)
    }
}

fn collect_texture_refs(node: &FdfNode, inherited_decorate: bool, out: &mut Vec<(String, bool)>) {
    let decorate = inherited_decorate || node.has("DecorateFileNames");
    for key in [
        "File",
        "BackdropBackground",
        "BackdropEdgeFile",
        "BackdropCornerFile",
        "BackdropLeftFile",
        "BackdropRightFile",
        "BackdropTopFile",
        "BackdropBottomFile",
        "HighlightAlphaFile",
        "StatusBarTexture",
    ] {
        if let Some(value) = node.string(key) {
            out.push((value.to_string(), decorate));
        }
    }
    for child in &node.children {
        collect_texture_refs(child, decorate, out);
    }
}

// ---------------------------------------------------------------------------
// Resolved tree
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ResolvedNode {
    pub id: FrameId,
    pub parent: Option<FrameId>,
    /// Resolved child ids (the dereferenced [`FdfNode::children`] is empty).
    pub children: Vec<FrameId>,
    /// Template this node inherited from, if any.
    pub template: Option<String>,
    pub source: String,
    pub node: FdfNode,
}

impl Deref for ResolvedNode {
    type Target = FdfNode;

    fn deref(&self) -> &FdfNode {
        &self.node
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResolvedTree {
    pub nodes: Vec<ResolvedNode>,
    pub roots: Vec<FrameId>,
    by_name: HashMap<String, Vec<FrameId>>,
    by_name_ci: HashMap<String, Vec<FrameId>>,
    pub warnings: Vec<String>,
}

impl ResolvedTree {
    pub fn node(&self, id: FrameId) -> &ResolvedNode {
        &self.nodes[id]
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// First node with this name (exact, then case-insensitive).
    pub fn find(&self, name: &str) -> Option<FrameId> {
        self.by_name
            .get(name)
            .and_then(|ids| ids.first().copied())
            .or_else(|| {
                self.by_name_ci
                    .get(&name.to_ascii_lowercase())
                    .and_then(|ids| ids.first().copied())
            })
    }

    pub fn find_all(&self, name: &str) -> &[FrameId] {
        self.by_name.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Depth-first pre-order ids.
    pub fn preorder(&self) -> Vec<FrameId> {
        let mut out = Vec::with_capacity(self.nodes.len());
        for root in &self.roots {
            self.push_preorder(*root, &mut out);
        }
        out
    }

    fn push_preorder(&self, id: FrameId, out: &mut Vec<FrameId>) {
        out.push(id);
        for child in &self.nodes[id].children {
            self.push_preorder(*child, out);
        }
    }

    pub fn descendants(&self, id: FrameId) -> Vec<FrameId> {
        let mut out = Vec::new();
        self.push_preorder(id, &mut out);
        out.remove(0);
        out
    }
}

#[derive(Clone)]
struct Subtree {
    kind: String,
    name: Option<String>,
    template: Option<String>,
    source: String,
    props: Vec<FdfProp>,
    children: Vec<Subtree>,
}

struct TreeBuilder<'a> {
    theme: &'a UiTheme,
    nodes: Vec<ResolvedNode>,
    by_name: HashMap<String, Vec<FrameId>>,
    by_name_ci: HashMap<String, Vec<FrameId>>,
    warnings: Vec<String>,
}

impl<'a> TreeBuilder<'a> {
    fn build_subtree(&mut self, node: &FdfNode, source: &str, stack: &mut Vec<String>) -> Subtree {
        let mut base = None;
        if let Some(name) = &node.inherits {
            if stack.iter().any(|entry| entry.eq_ignore_ascii_case(name)) {
                self.warnings
                    .push(format!("recursive INHERITS cycle at {name}"));
            } else if let Some(template) = self.theme.templates.get(name) {
                stack.push(name.clone());
                base = Some(self.build_subtree(&template.node, &template.source, stack));
                stack.pop();
            } else {
                self.warnings.push(format!("missing template {name}"));
            }
        }
        let mut props = base
            .as_ref()
            .map(|base| base.props.clone())
            .unwrap_or_default();
        merge_props(&mut props, &node.props);
        let mut children = if node.inherit_children {
            base.as_ref()
                .map(|base| base.children.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        for child in &node.children {
            let resolved = self.build_subtree(child, source, stack);
            let name = resolved
                .name
                .as_deref()
                .filter(|name| !name.is_empty())
                .map(str::to_string);
            if let Some(name) = name
                && let Some(position) = children.iter().position(|existing| {
                    existing
                        .name
                        .as_deref()
                        .is_some_and(|candidate| candidate == name)
                })
            {
                let inherited = children.remove(position);
                children.insert(position, merge_subtrees(inherited, resolved));
                continue;
            }
            children.push(resolved);
        }
        Subtree {
            kind: node.kind.clone(),
            name: node.name.clone(),
            template: node.inherits.clone(),
            source: source.to_string(),
            props,
            children,
        }
    }

    fn emit(&mut self, subtree: Subtree, parent: Option<FrameId>) -> FrameId {
        let id = self.nodes.len();
        self.nodes.push(ResolvedNode {
            id,
            parent,
            children: Vec::new(),
            template: subtree.template.clone(),
            source: subtree.source.clone(),
            node: FdfNode {
                kind: subtree.kind,
                name: subtree.name.clone(),
                inherits: subtree.template,
                inherit_children: false,
                props: subtree.props,
                children: Vec::new(),
            },
        });
        if let Some(name) = subtree.name.as_deref().filter(|name| !name.is_empty()) {
            self.by_name.entry(name.to_string()).or_default().push(id);
            self.by_name_ci
                .entry(name.to_ascii_lowercase())
                .or_default()
                .push(id);
        }
        for child in subtree.children {
            let child_id = self.emit(child, Some(id));
            self.nodes[id].children.push(child_id);
        }
        id
    }
}

/// Merge local properties over inherited ones. Repeated list properties
/// (`MenuItem`) append; a local key replaces all inherited values of that key
/// while keeping the inherited position. Local keys that repeat among
/// themselves (`SetPoint`, `Anchor`) are all kept in source order.
fn merge_props(base: &mut Vec<FdfProp>, local: &[FdfProp]) {
    let mut overridden: Vec<String> = Vec::new();
    for prop in local {
        let key = prop.key.to_ascii_lowercase();
        if prop.key.eq_ignore_ascii_case("MenuItem") {
            base.push(prop.clone());
            continue;
        }
        if overridden.contains(&key) {
            base.push(prop.clone());
            continue;
        }
        if let Some(position) = base
            .iter()
            .position(|existing| existing.key.eq_ignore_ascii_case(&prop.key))
        {
            base.retain(|existing| !existing.key.eq_ignore_ascii_case(&prop.key));
            base.insert(position.min(base.len()), prop.clone());
        } else {
            base.push(prop.clone());
        }
        overridden.push(key);
    }
}

/// `base` is the implicit template (WITHCHILDREN clone); `top` is the local
/// frame. Local properties and same-name children win.
fn merge_subtrees(base: Subtree, top: Subtree) -> Subtree {
    let mut props = base.props;
    merge_props(&mut props, &top.props);
    let mut children = base.children;
    for child in top.children {
        let name = child
            .name
            .as_deref()
            .filter(|name| !name.is_empty())
            .map(str::to_string);
        if let Some(name) = name
            && let Some(position) = children.iter().position(|existing| {
                existing
                    .name
                    .as_deref()
                    .is_some_and(|candidate| candidate == name)
            })
        {
            let inherited = children.remove(position);
            children.insert(position, merge_subtrees(inherited, child));
            continue;
        }
        children.push(child);
    }
    Subtree {
        kind: top.kind,
        name: top.name,
        template: top.template,
        source: top.source,
        props,
        children,
    }
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct LayoutConfig {
    /// Logical screen in pixels.
    pub screen: Vec2,
    /// Readability floor for FDF font sizes.
    pub min_font_px: f32,
    /// Derive missing text-frame sizes from the font atlas.
    pub measure_text: bool,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            screen: Vec2::new(1920.0, 1080.0),
            min_font_px: self::text::DEFAULT_MIN_FONT_PX,
            measure_text: true,
        }
    }
}

impl LayoutConfig {
    pub fn new(screen: Vec2) -> Self {
        Self {
            screen,
            ..Self::default()
        }
    }
}

/// Dynamic text values keyed by frame name. Screens update bindings every
/// frame; the FDF layer resolves everything else from `Text`/`StringList`.
#[derive(Debug, Clone, Default)]
pub struct TextBindings {
    values: HashMap<String, String>,
}

impl TextBindings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, frame: impl Into<String>, value: impl Into<String>) {
        self.values.insert(frame.into(), value.into());
    }

    pub fn with(mut self, frame: impl Into<String>, value: impl Into<String>) -> Self {
        self.set(frame, value);
        self
    }

    pub fn get(&self, frame: &str) -> Option<&str> {
        self.values.get(frame).map(String::as_str).or_else(|| {
            self.values
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(frame))
                .map(|(_, value)| value.as_str())
        })
    }

    pub fn remove(&mut self, frame: &str) {
        self.values.remove(frame);
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaMode {
    Blend,
    Add,
    AlphaKey,
    Disable,
}

impl AlphaMode {
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::to_ascii_uppercase).as_deref() {
            Some("ADD") => Self::Add,
            Some("ALPHAKEY") => Self::AlphaKey,
            Some("DISABLE") => Self::Disable,
            _ => Self::Blend,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintKind {
    Texture,
    Backdrop,
    Highlight,
    Sprite,
    Model,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackdropPaint {
    pub background: Option<String>,
    pub corner_file: Option<String>,
    pub left_file: Option<String>,
    pub right_file: Option<String>,
    pub top_file: Option<String>,
    pub bottom_file: Option<String>,
    pub corner_flags: Option<String>,
    /// `BackdropBackgroundInsets left,right,top,bottom` in screen fractions.
    pub insets: [f32; 4],
    pub corner_size: f32,
    pub tile_background: bool,
    pub blend_all: bool,
    pub mirrored: bool,
    pub half_sides: bool,
}

impl Default for BackdropPaint {
    fn default() -> Self {
        Self {
            background: None,
            corner_file: None,
            left_file: None,
            right_file: None,
            top_file: None,
            bottom_file: None,
            corner_flags: None,
            insets: [0.0; 4],
            corner_size: 0.0,
            tile_background: false,
            blend_all: false,
            mirrored: false,
            half_sides: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutPaint {
    pub kind: PaintKind,
    /// FDF texture key (skin key when decorated).
    pub key: Option<String>,
    /// Resolved game path (`.blp` appended when needed).
    pub file: Option<String>,
    /// Index into `Assets::textures` when the texture was loaded.
    pub texture: Option<usize>,
    pub uv: [f32; 4],
    pub color: [f32; 4],
    pub blend: AlphaMode,
    pub layer: Option<String>,
    pub backdrop: Option<BackdropPaint>,
}

impl Default for LayoutPaint {
    fn default() -> Self {
        Self {
            kind: PaintKind::Texture,
            key: None,
            file: None,
            texture: None,
            uv: [0.0, 0.0, 1.0, 1.0],
            color: [1.0; 4],
            blend: AlphaMode::Blend,
            layer: None,
            backdrop: None,
        }
    }
}

/// Interaction wiring declared by FDF (`ControlShortcutKey`, `TabFocusNext`,
/// `PopupMenuFrame`, `ControlStyle`, `HighlightText`). WC3 binds the actual
/// click handlers from script by frame name, so the framework exposes the
/// metadata plus focus/shortcut navigation instead of inventing callbacks.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameInteraction {
    /// `ControlShortcutKey "Q"`.
    pub shortcut: Option<String>,
    /// `TabFocusNext "OtherFrame"`.
    pub tab_next: Option<String>,
    pub tab_default: bool,
    pub tab_push: bool,
    /// `PopupMenuFrame "Menu"` and its optional title/arrow children.
    pub popup_menu: Option<String>,
    pub popup_title: Option<String>,
    pub popup_arrow: Option<String>,
    /// `ControlStyle "AUTOTRACK|HIGHLIGHTONMOUSEOVER"`.
    pub control_style: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutText {
    pub text: String,
    pub style: TextStyle,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ButtonLabels {
    pub normal: Option<String>,
    pub pushed: Option<String>,
    pub disabled: Option<String>,
    /// `HighlightText "Template" "KEY"` (hover label).
    pub highlight: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LayoutNode {
    pub frame: FrameId,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub kind: String,
    pub name: Option<String>,
    pub source: String,
    /// Pixel rectangle in the configured screen.
    pub rect: Rect,
    /// The same rectangle as screen fractions `[x, y, w, h]`.
    pub norm: [f32; 4],
    pub visible: bool,
    pub interactive: bool,
    pub text: Option<LayoutText>,
    pub button: Option<ButtonLabels>,
    pub interaction: FrameInteraction,
    pub paint: Vec<LayoutPaint>,
    /// `ControlBackdrop`/`ControlDisabledBackdrop` child name.
    pub control_backdrop: Option<String>,
    pub layer: Option<String>,
    pub decorate: bool,
}

impl LayoutNode {
    pub fn contains(&self, point: Vec2) -> bool {
        self.visible
            && self.rect.w > 0.0
            && self.rect.h > 0.0
            && point.x >= self.rect.x
            && point.y >= self.rect.y
            && point.x < self.rect.x + self.rect.w
            && point.y < self.rect.y + self.rect.h
    }

    pub fn centre(&self) -> Vec2 {
        self.rect.centre()
    }
}

#[derive(Debug, Clone)]
pub struct UiLayout {
    pub screen: Vec2,
    pub nodes: Vec<LayoutNode>,
    pub roots: Vec<usize>,
    by_name: HashMap<String, Vec<usize>>,
}

impl UiLayout {
    pub fn node(&self, index: usize) -> &LayoutNode {
        &self.nodes[index]
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).and_then(|ids| ids.first().copied())
    }

    pub fn find_all(&self, name: &str) -> &[usize] {
        self.by_name.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn named(&self, name: &str) -> Option<&LayoutNode> {
        self.find(name).map(|index| &self.nodes[index])
    }

    pub fn interactive_nodes(&self) -> Vec<usize> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.interactive && node.visible)
            .map(|(index, _)| index)
            .collect()
    }

    /// Keyboard/gamepad focus ring: start at the `TabFocusDefault` frame and
    /// follow `TabFocusNext` names, then append any remaining controls in
    /// layout order.
    pub fn focus_order(&self) -> Vec<usize> {
        let mut order = Vec::new();
        let mut seen = HashSet::new();
        let mut current = self
            .nodes
            .iter()
            .position(|node| node.interactive && node.visible && node.interaction.tab_default)
            .or_else(|| {
                self.nodes
                    .iter()
                    .position(|node| node.interactive && node.visible)
            });
        let mut guard = 0usize;
        while let Some(index) = current {
            guard += 1;
            if guard > self.nodes.len() || !seen.insert(index) {
                break;
            }
            let node = &self.nodes[index];
            if !node.interactive {
                break;
            }
            order.push(index);
            current = node
                .interaction
                .tab_next
                .as_deref()
                .and_then(|name| self.find(name));
        }
        for (index, node) in self.nodes.iter().enumerate() {
            if node.interactive && node.visible && seen.insert(index) {
                order.push(index);
            }
        }
        order
    }

    pub fn tab_next(&self, index: usize) -> Option<usize> {
        let name = self.nodes.get(index)?.interaction.tab_next.as_deref()?;
        self.find(name)
    }

    /// `ControlShortcutKey` lookup (case-insensitive, single character or key
    /// name as written in FDF).
    pub fn shortcut_target(&self, key: &str) -> Option<usize> {
        let key = key.trim();
        self.nodes.iter().position(|node| {
            node.interactive
                && node.visible
                && node
                    .interaction
                    .shortcut
                    .as_deref()
                    .is_some_and(|shortcut| shortcut.eq_ignore_ascii_case(key))
        })
    }

    /// `PopupMenuFrame` target for a button/menu frame.
    pub fn popup_target(&self, index: usize) -> Option<usize> {
        let name = self.nodes.get(index)?.interaction.popup_menu.as_deref()?;
        self.find(name)
    }

    /// Topmost frame containing `point` (children above parents, later
    /// siblings above earlier ones).
    pub fn hit_test(&self, point: Vec2) -> Option<usize> {
        self.hit_test_filtered(point, |_| true)
    }

    /// Topmost interactive frame (buttons, edit boxes, sliders, ...).
    pub fn hit_test_interactive(&self, point: Vec2) -> Option<usize> {
        self.hit_test_filtered(point, |node| node.interactive)
    }

    pub fn hit_test_filtered(
        &self,
        point: Vec2,
        filter: impl Fn(&LayoutNode) -> bool,
    ) -> Option<usize> {
        for root in self.roots.iter().rev() {
            if let Some(hit) = self.hit_node(*root, point, &filter) {
                return Some(hit);
            }
        }
        None
    }

    fn hit_node(
        &self,
        index: usize,
        point: Vec2,
        filter: &impl Fn(&LayoutNode) -> bool,
    ) -> Option<usize> {
        let node = &self.nodes[index];
        for child in node.children.iter().rev() {
            if let Some(hit) = self.hit_node(*child, point, filter) {
                return Some(hit);
            }
        }
        if node.contains(point) && filter(node) {
            return Some(index);
        }
        None
    }

    /// Every hit, topmost first (diagnostics / multi-touch routing).
    pub fn hit_test_all(&self, point: Vec2) -> Vec<usize> {
        let mut out = Vec::new();
        for root in self.roots.iter().rev() {
            self.collect_hits(*root, point, &mut out);
        }
        out
    }

    fn collect_hits(&self, index: usize, point: Vec2, out: &mut Vec<usize>) {
        let node = &self.nodes[index];
        if node.contains(point) {
            out.push(index);
        }
        for child in node.children.iter().rev() {
            self.collect_hits(*child, point, out);
        }
    }

    pub fn texts(&self) -> impl Iterator<Item = (usize, &LayoutText)> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| node.text.as_ref().map(|text| (index, text)))
    }

    pub fn paints(&self) -> impl Iterator<Item = (usize, &LayoutPaint)> {
        self.nodes
            .iter()
            .enumerate()
            .flat_map(|(index, node)| node.paint.iter().map(move |paint| (index, paint)))
    }

    /// DFS pre-order paint order (later draws on top).
    pub fn paint_order(&self) -> Vec<usize> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.visible && !node.paint.is_empty())
            .map(|(index, _)| index)
            .collect()
    }
}

/// Format a WC3 tooltip string with `%s`/`%d`/`%u` arguments in order.
/// `%%` is a literal percent; missing arguments leave the placeholder.
pub fn format_tooltip_text(text: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut args = args.iter();
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '%' {
            out.push(character);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some(kind @ ('s' | 'd' | 'u' | 'f')) => {
                if let Some(value) = args.next() {
                    out.push_str(value);
                } else {
                    out.push('%');
                    out.push(kind);
                }
            }
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

fn is_text_kind(kind: &str) -> bool {
    matches!(
        kind.to_ascii_uppercase().as_str(),
        "TEXT" | "STRING" | "TEXTAREA" | "TIMEDTEXTFRAME" | "EDITBOX" | "CHATDISPLAY" | "GLUETEXT"
    )
}

fn is_button_kind(kind: &str) -> bool {
    matches!(
        kind.to_ascii_uppercase().as_str(),
        "BUTTON"
            | "TEXTBUTTON"
            | "GLUEBUTTON"
            | "GLUETEXTBUTTON"
            | "SIMPLEBUTTON"
            | "GLUECHECKBOX"
            | "CHECKBOX"
            | "SIMPLECHECKBOX"
            | "SLIDER"
            | "SCROLLBAR"
            | "POPUPMENU"
            | "MENU"
            | "LISTBOX"
            | "DIALOG"
            | "EDITBOX"
    )
}

fn is_interactive_kind(kind: &str) -> bool {
    is_button_kind(kind)
}

fn style_from_node(node: &ResolvedNode, min_px: f32) -> TextStyle {
    let shadow = match (node.font_shadow_offset(), node.font_shadow_color()) {
        (Some(offset), Some(color)) => Some(FontShadow { offset, color }),
        _ => None,
    };
    TextStyle::from_parts(
        node.font(),
        node.font_color(),
        node.font_justification_h(),
        node.font_justification_v(),
        shadow,
        node.font_flags(),
    )
    .with_min_px(min_px)
}

fn style_from_props(props: &[FdfProp], min_px: f32) -> TextStyle {
    let node = FdfNode {
        props: props.to_vec(),
        ..FdfNode::default()
    };
    let shadow = match (node.font_shadow_offset(), node.font_shadow_color()) {
        (Some(offset), Some(color)) => Some(FontShadow { offset, color }),
        _ => None,
    };
    TextStyle::from_parts(
        node.font(),
        node.font_color(),
        node.font_justification_h(),
        node.font_justification_v(),
        shadow,
        node.font_flags(),
    )
    .with_min_px(min_px)
}

fn text_from_node<'a>(
    theme: &'a UiTheme,
    tree: &'a ResolvedTree,
    node: &'a ResolvedNode,
    bindings: &'a TextBindings,
) -> Option<String> {
    let name = node.name.as_deref().filter(|name| !name.is_empty());
    if let Some(name) = name
        && let Some(value) = bindings.get(name)
    {
        return Some(value.to_string());
    }
    if let Some(raw) = node.text() {
        return Some(theme.localization().text(raw));
    }
    // Text frames exist as templates only and are filled by game code; keep
    // them addressable through bindings (e.g. `ResourceBarGoldText`).
    if is_text_kind(&node.kind) {
        let _ = tree;
        return None;
    }
    None
}

fn prop_text(node: &ResolvedNode, key: &str) -> Option<String> {
    let prop = node.prop(key)?;
    prop.ident(0).or_else(|| prop.string(0)).map(str::to_string)
}

fn label_for_button(
    theme: &UiTheme,
    tree: &ResolvedTree,
    node: &ResolvedNode,
    bindings: &TextBindings,
) -> (Option<LayoutText>, ButtonLabels) {
    let min_px = self::text::DEFAULT_MIN_FONT_PX;
    let mut labels = ButtonLabels::default();
    let mut normal_style = style_from_node(node, min_px);
    let mut normal_text = None;

    if let Some((template, key)) = node.normal_text() {
        let text = theme.localization().text(key);
        if let Some(props) = theme.template_props(template) {
            normal_style = style_from_props(&props, min_px);
        }
        normal_text = Some(text);
    }
    if let Some((template, key)) = node.disabled_text() {
        labels.disabled = Some(theme.localization().text(key));
        let _ = template;
    }
    if let Some((_template, key)) = node.pushed_text() {
        labels.pushed = Some(theme.localization().text(key));
    }
    if let Some(prop) = node.prop("HighlightText")
        && let Some(key) = prop.string(1)
    {
        labels.highlight = Some(theme.localization().text(key));
    }

    let mut text_from_child_frame = false;
    if normal_text.is_none()
        && let Some(target) = node.button_text()
    {
        // `ButtonText` names a child TEXT frame (sometimes declared later).
        if let Some(target_id) = tree.find(target) {
            let target_node = tree.node(target_id);
            normal_style = style_from_node(target_node, min_px);
            normal_text = text_from_node(theme, tree, target_node, bindings)
                .or_else(|| target_node.text().map(|raw| theme.localization().text(raw)));
            text_from_child_frame = normal_text.is_some();
        }
        if normal_text.is_none() {
            normal_text = theme.localization().resolve_owned(target).or_else(|| {
                if target.contains(' ') || target.chars().any(|c| c.is_lowercase()) {
                    None
                } else {
                    Some(target.to_string())
                }
            });
        }
    }
    if normal_text.is_none()
        && let Some(name) = node.name.as_deref().filter(|name| !name.is_empty())
    {
        normal_text = bindings.get(&format!("{name}.normal")).map(str::to_string);
    }
    labels.normal = normal_text.clone();
    // When the label is a real child TEXT frame, that child already carries
    // its own LayoutText; rendering the button text too would double-draw it.
    let text = if text_from_child_frame {
        None
    } else {
        normal_text.map(|text| LayoutText {
            text,
            style: normal_style,
        })
    };
    (text, labels)
}

fn build_paint(
    theme: &UiTheme,
    node: &ResolvedNode,
    decorate: bool,
    layer: Option<&str>,
) -> Vec<LayoutPaint> {
    let mut paints = Vec::new();
    let kind = node.kind.to_ascii_uppercase();
    let alpha_mode = node.alpha_mode();
    let blend = AlphaMode::parse(alpha_mode);
    let mut color = node
        .prop("Color")
        .and_then(|prop| {
            Some([
                prop.number(0)?,
                prop.number(1)?,
                prop.number(2)?,
                prop.number(3).unwrap_or(1.0),
            ])
        })
        .unwrap_or([1.0; 4]);
    // Some FDF dialects use 0..255 components; normalise those.
    if color[..3].iter().any(|component| *component > 1.5) {
        for component in &mut color[..3] {
            *component /= 255.0;
        }
    }
    if let Some(alpha) = node.number("Alpha") {
        color[3] *= alpha.clamp(0.0, 1.0);
    }
    let uv = node.texture_uv().unwrap_or([0.0, 0.0, 1.0, 1.0]);
    let make_texture = |kind: PaintKind, key: Option<&str>| {
        let mut paint = LayoutPaint {
            kind,
            key: key.map(str::to_string),
            uv,
            color,
            blend,
            layer: layer.map(str::to_string),
            ..LayoutPaint::default()
        };
        if let Some(key) = key {
            paint.file = theme.resolve_texture_key(key, decorate);
            paint.texture = theme.texture(key);
        }
        paint
    };
    match kind.as_str() {
        "TEXTURE" => {
            if let Some(key) = node.string("File") {
                paints.push(make_texture(PaintKind::Texture, Some(key)));
            }
        }
        "SPRITE" => {
            if let Some(key) = node.string("BackgroundArt") {
                paints.push(make_texture(PaintKind::Sprite, Some(key)));
            }
        }
        "MODEL" => {
            if let Some(key) = node.string("BackgroundArt") {
                paints.push(make_texture(PaintKind::Model, Some(key)));
            }
        }
        "HIGHLIGHT" => {
            let key = node
                .string("HighlightAlphaFile")
                .or_else(|| node.string("HighlightAlphaMode"));
            if let Some(key) = key {
                paints.push(make_texture(PaintKind::Highlight, Some(key)));
            } else if let Some(highlight) = node.prop("HighlightColor") {
                let color = [
                    highlight.number(0).unwrap_or(1.0),
                    highlight.number(1).unwrap_or(1.0),
                    highlight.number(2).unwrap_or(1.0),
                    highlight.number(3).unwrap_or(0.2),
                ];
                paints.push(LayoutPaint {
                    kind: PaintKind::Highlight,
                    color,
                    layer: layer.map(str::to_string),
                    ..LayoutPaint::default()
                });
            }
        }
        "BACKDROP" | "SIMPLESTATUSBAR" | "STATUSBAR" => {
            let backdrop = BackdropPaint {
                background: node.string("BackdropBackground").map(str::to_string),
                corner_file: node.string("BackdropCornerFile").map(str::to_string),
                left_file: node.string("BackdropLeftFile").map(str::to_string),
                right_file: node.string("BackdropRightFile").map(str::to_string),
                top_file: node.string("BackdropTopFile").map(str::to_string),
                bottom_file: node.string("BackdropBottomFile").map(str::to_string),
                corner_flags: node.string("BackdropCornerFlags").map(str::to_string),
                insets: node
                    .prop("BackdropBackgroundInsets")
                    .map(|prop| {
                        [
                            prop.number(0).unwrap_or(0.0),
                            prop.number(1).unwrap_or(0.0),
                            prop.number(2).unwrap_or(0.0),
                            prop.number(3).unwrap_or(0.0),
                        ]
                    })
                    .unwrap_or([0.0; 4]),
                corner_size: node.number("BackdropCornerSize").unwrap_or(0.0),
                tile_background: node.flag("BackdropTileBackground"),
                blend_all: node.flag("BackdropBlendAll"),
                mirrored: node.flag("BackdropMirrored"),
                half_sides: node.flag("BackdropHalfSides"),
            };
            let key = backdrop
                .background
                .as_deref()
                .or_else(|| node.string("BackdropEdgeFile"));
            let mut paint = LayoutPaint {
                kind: PaintKind::Backdrop,
                key: key.map(str::to_string),
                uv,
                color,
                blend: if backdrop.blend_all {
                    AlphaMode::Blend
                } else {
                    blend
                },
                layer: layer.map(str::to_string),
                backdrop: Some(backdrop.clone()),
                ..LayoutPaint::default()
            };
            if let Some(key) = key {
                paint.file = theme.resolve_texture_key(key, decorate);
                paint.texture = theme.texture(key);
            }
            paints.push(paint);
        }
        _ => {}
    }
    paints
}

struct LayoutBuilder<'a> {
    theme: &'a UiTheme,
    tree: &'a ResolvedTree,
    config: &'a LayoutConfig,
    bindings: &'a TextBindings,
    font: Option<&'a Font>,
    nodes: Vec<LayoutNode>,
    frames: Vec<FrameId>,
    by_name: HashMap<String, Vec<usize>>,
    intrinsic: Vec<Option<Vec2>>,
}

struct AxisPoint {
    position: f32,
    fraction: f32,
}

#[derive(Default)]
struct Axis {
    min: Option<AxisPoint>,
    mid: Option<AxisPoint>,
    max: Option<AxisPoint>,
}

impl Axis {
    fn apply(&mut self, point: AxisPoint) {
        let slot = if point.fraction <= 0.0 {
            0
        } else if point.fraction >= 1.0 {
            2
        } else {
            1
        };
        match slot {
            0 => {
                self.min = Some(point);
                if self.mid.is_some() {
                    self.mid = None;
                }
            }
            2 => {
                self.max = Some(point);
                if self.mid.is_some() {
                    self.mid = None;
                }
            }
            _ => {
                if self.min.is_none() && self.max.is_none() {
                    self.mid = Some(point);
                }
            }
        }
    }

    fn solve(&self, size: f32, default_position: f32) -> (f32, f32) {
        if let Some(mid) = &self.mid {
            return (mid.position - size * 0.5, size);
        }
        if let (Some(min), Some(max)) = (&self.min, &self.max) {
            if max.position >= min.position {
                return (min.position, max.position - min.position);
            }
            return (max.position, min.position - max.position);
        }
        if let Some(max) = &self.max {
            return (max.position - size, size);
        }
        if let Some(min) = &self.min {
            return (min.position, size);
        }
        (default_position, size)
    }

    fn used(&self) -> bool {
        self.min.is_some() || self.mid.is_some() || self.max.is_some()
    }
}

impl<'a> LayoutBuilder<'a> {
    fn flatten(
        &mut self,
        frame: FrameId,
        parent: Option<usize>,
        layer: Option<String>,
        decorate: bool,
    ) -> usize {
        let resolved = self.tree.node(frame);
        let index = self.nodes.len();
        let current_layer = if resolved.kind.eq_ignore_ascii_case("Layer") {
            resolved.name.clone().or(layer.clone())
        } else {
            layer.clone()
        };
        let decorate = decorate || resolved.has("DecorateFileNames");
        let kind = resolved.kind.clone();
        let text = if is_button_kind(&kind) {
            None
        } else if is_text_kind(&kind) {
            text_from_node(self.theme, self.tree, resolved, self.bindings).map(|text| LayoutText {
                text,
                style: style_from_node(resolved, self.config.min_font_px),
            })
        } else {
            None
        };
        let (button_text, button) = if is_button_kind(&kind) {
            let (text, labels) = label_for_button(self.theme, self.tree, resolved, self.bindings);
            (text, Some(labels))
        } else {
            (None, None)
        };
        let paint = build_paint(self.theme, resolved, decorate, current_layer.as_deref());
        let visible = !resolved.flag("Hidden");
        let interactive = is_interactive_kind(&kind)
            || resolved.has("ControlBackdrop")
            || resolved.has("ButtonText")
            || resolved.has("OnClick");
        let interaction = FrameInteraction {
            shortcut: prop_text(resolved, "ControlShortcutKey"),
            tab_next: prop_text(resolved, "TabFocusNext"),
            tab_default: resolved.flag("TabFocusDefault"),
            tab_push: resolved.flag("TabFocusPush"),
            popup_menu: prop_text(resolved, "PopupMenuFrame"),
            popup_title: prop_text(resolved, "PopupTitleFrame"),
            popup_arrow: prop_text(resolved, "PopupArrowFrame"),
            control_style: prop_text(resolved, "ControlStyle"),
        };
        let control_backdrop = resolved
            .string("ControlBackdrop")
            .or_else(|| resolved.string("ControlDisabledBackdrop"))
            .or_else(|| resolved.string("ControlPushedBackdrop"))
            .map(str::to_string);
        self.nodes.push(LayoutNode {
            frame,
            parent,
            children: Vec::new(),
            kind,
            name: resolved.name.clone(),
            source: resolved.source.clone(),
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            norm: [0.0; 4],
            visible,
            interactive,
            text: text.or(button_text),
            button,
            interaction,
            paint,
            control_backdrop,
            layer: current_layer.clone(),
            decorate,
        });
        self.frames.push(frame);
        if let Some(name) = self.nodes[index]
            .name
            .clone()
            .filter(|name| !name.is_empty())
        {
            self.by_name.entry(name).or_default().push(index);
        }
        let intrinsic = self.intrinsic_size(index);
        self.intrinsic.push(intrinsic);
        for child in &resolved.children {
            let child_index = self.flatten(*child, Some(index), current_layer.clone(), decorate);
            self.nodes[index].children.push(child_index);
        }
        index
    }

    fn intrinsic_size(&self, index: usize) -> Option<Vec2> {
        if !self.config.measure_text {
            return None;
        }
        let node = &self.nodes[index];
        let text = node.text.as_ref()?;
        let font = self.font?;
        let explicit_width = self
            .tree
            .node(node.frame)
            .width()
            .map(|width| width * self.config.screen.x);
        let max_width = explicit_width.filter(|width| *width > 0.0);
        let shaped =
            TextShaper::new(font).shape(&text.text, &text.style, self.config.screen, max_width);
        Some(Vec2::new(shaped.width, shaped.height))
    }

    fn rect_of(
        &mut self,
        index: usize,
        rects: &mut [Option<Rect>],
        computing: &mut [bool],
    ) -> Rect {
        if let Some(rect) = rects[index] {
            return rect;
        }
        if computing[index] {
            return self.parent_rect(index, rects);
        }
        computing[index] = true;
        let frame = self.frames[index];
        let resolved = self.tree.node(frame);
        let parent_rect = self.parent_rect(index, rects);
        let screen = self.config.screen;
        let mut width = resolved.width().unwrap_or(0.0) * screen.x;
        let mut height = resolved.height().unwrap_or(0.0) * screen.y;
        let explicit_width = resolved.width();
        let explicit_height = resolved.height();
        let props = resolved.props.clone();
        let is_root = self.nodes[index].parent.is_none();
        if let Some(intrinsic) = self.intrinsic[index] {
            if width <= 0.0 {
                width = intrinsic.x;
            }
            if height <= 0.0 {
                height = intrinsic.y;
            }
        }
        let mut x_axis = Axis::default();
        let mut y_axis = Axis::default();
        // Property order matters: SetAllPoints, Anchor and SetPoint are applied
        // in source order, exactly like the engine's fdf actions.
        for prop in &props {
            if prop.key.eq_ignore_ascii_case("SetAllPoints") {
                let target = prop
                    .string(0)
                    .and_then(|name| self.target_rect(index, name, rects, computing))
                    .unwrap_or(parent_rect);
                x_axis.apply(AxisPoint {
                    position: target.x,
                    fraction: 0.0,
                });
                x_axis.apply(AxisPoint {
                    position: target.x + target.w,
                    fraction: 1.0,
                });
                y_axis.apply(AxisPoint {
                    position: target.y,
                    fraction: 0.0,
                });
                y_axis.apply(AxisPoint {
                    position: target.y + target.h,
                    fraction: 1.0,
                });
            } else if prop.key.eq_ignore_ascii_case("SetPoint") {
                if let Some(point) = FdfSetPoint::parse(prop) {
                    self.apply_point(
                        index,
                        &point,
                        None,
                        rects,
                        computing,
                        &mut x_axis,
                        &mut y_axis,
                    );
                }
            } else if prop.key.eq_ignore_ascii_case("Anchor") {
                if let (Some(point), Some(x), Some(y)) =
                    (prop.ident(0), prop.number(1), prop.number(2))
                {
                    let point = FdfSetPoint {
                        point: point.to_ascii_uppercase(),
                        target: None,
                        target_point: point.to_ascii_uppercase(),
                        x,
                        y,
                    };
                    self.apply_point(
                        index,
                        &point,
                        Some(parent_rect),
                        rects,
                        computing,
                        &mut x_axis,
                        &mut y_axis,
                    );
                }
            }
        }
        let (x, solved_width) = x_axis.solve(width, 0.0);
        let (y, solved_height) = y_axis.solve(height, 0.0);
        let mut rect = Rect::new(x, y, solved_width.max(0.0), solved_height.max(0.0));
        // A screen-space FDF root without any authored geometry is the canvas.
        if is_root
            && !x_axis.used()
            && !y_axis.used()
            && explicit_width.is_none()
            && explicit_height.is_none()
        {
            rect = Rect::new(0.0, 0.0, screen.x, screen.y);
        }
        rects[index] = Some(rect);
        computing[index] = false;
        rect
    }

    fn parent_rect(&self, index: usize, rects: &[Option<Rect>]) -> Rect {
        match self.nodes[index].parent {
            Some(parent) => rects[parent]
                .unwrap_or_else(|| Rect::new(0.0, 0.0, self.config.screen.x, self.config.screen.y)),
            None => Rect::new(0.0, 0.0, self.config.screen.x, self.config.screen.y),
        }
    }

    fn apply_point(
        &mut self,
        index: usize,
        point: &FdfSetPoint,
        forced_target: Option<Rect>,
        rects: &mut [Option<Rect>],
        computing: &mut [bool],
        x_axis: &mut Axis,
        y_axis: &mut Axis,
    ) {
        let target = match forced_target {
            Some(rect) => rect,
            None => point
                .target
                .as_deref()
                .and_then(|name| self.target_rect(index, name, rects, computing))
                .unwrap_or_else(|| self.parent_rect(index, rects)),
        };
        let screen = self.config.screen;
        let (fx, fy) = fdf::point_fraction(&point.target_point);
        let position_x = target.x + target.w * fx + point.x * screen.x;
        let position_y = target.y + target.h * fy - point.y * screen.y;
        let (own_x, own_y) = fdf::point_fraction(&point.point);
        x_axis.apply(AxisPoint {
            position: position_x,
            fraction: own_x,
        });
        y_axis.apply(AxisPoint {
            position: position_y,
            fraction: own_y,
        });
    }

    fn target_rect(
        &mut self,
        index: usize,
        name: &str,
        rects: &mut [Option<Rect>],
        computing: &mut [bool],
    ) -> Option<Rect> {
        let candidates = self.by_name.get(name)?.clone();
        let mut best = None;
        let mut best_distance = usize::MAX;
        for candidate in &candidates {
            if *candidate == index {
                continue;
            }
            let distance = candidate.abs_diff(index);
            if distance < best_distance {
                best = Some(*candidate);
                best_distance = distance;
            }
        }
        best.map(|candidate| self.rect_of(candidate, rects, computing))
    }
}

fn build_layout(
    theme: &UiTheme,
    tree: &ResolvedTree,
    root: FrameId,
    config: &LayoutConfig,
    bindings: &TextBindings,
    font: Option<&Font>,
) -> UiLayout {
    let mut builder = LayoutBuilder {
        theme,
        tree,
        config,
        bindings,
        font,
        nodes: Vec::new(),
        frames: Vec::new(),
        by_name: HashMap::new(),
        intrinsic: Vec::new(),
    };
    builder.flatten(root, None, None, false);
    let mut rects = vec![None; builder.nodes.len()];
    let mut computing = vec![false; builder.nodes.len()];
    for index in 0..builder.nodes.len() {
        builder.rect_of(index, &mut rects, &mut computing);
    }
    let screen = config.screen;
    for (index, node) in builder.nodes.iter_mut().enumerate() {
        let rect = rects[index].unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
        node.rect = rect;
        node.norm = [
            rect.x / screen.x,
            rect.y / screen.y,
            rect.w / screen.x,
            rect.h / screen.y,
        ];
    }
    UiLayout {
        screen,
        nodes: builder.nodes,
        roots: vec![0],
        by_name: builder.by_name,
    }
}

// ---------------------------------------------------------------------------
// Game-state bindings
// ---------------------------------------------------------------------------

/// Values the game updates every frame (resources, selection, FPS).
///
/// `UiState` is the bridge between the simulation and FDF text frames; it does
/// not render anything and does not own the modern HUD.
pub struct UiState<'a> {
    pub gold: u32,
    pub lumber: u32,
    pub supply: u32,
    pub supply_max: u32,
    pub army: u32,
    pub enemy: u32,
    pub fps: f32,
    pub map_name: &'a str,
    pub selected_name: &'a str,
    pub selected_class: &'a str,
}

impl UiState<'_> {
    /// Frame-name -> text bindings for [`UiTheme::build_layout`].
    pub fn bindings(&self) -> TextBindings {
        let mut bindings = TextBindings::new();
        bindings.set("ResourceBarGoldText", self.gold.to_string());
        bindings.set("ResourceBarLumberText", self.lumber.to_string());
        bindings.set(
            "ResourceBarSupplyText",
            format!("{}/{}", self.supply, self.supply_max),
        );
        bindings.set("ResourceBarUpkeepText", "");
        bindings.set("SimpleNameValue", self.selected_name);
        bindings.set("NameValue", self.selected_name);
        bindings.set("SimpleClassValue", self.selected_class);
        bindings.set("ClassValue", self.selected_class);
        bindings.set("SimpleHoldNameValue", self.selected_name);
        bindings.set("MapNameValue", self.map_name);
        bindings.set("ArmyValue", self.army.to_string());
        bindings.set("EnemyValue", self.enemy.to_string());
        bindings.set("FpsValue", format!("{:.0}", self.fps));
        bindings
    }

    pub fn bindings_into(&self, out: &mut TextBindings) {
        for (key, value) in self.bindings().values {
            out.values.insert(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skin_lookup_falls_back_to_default() {
        let table = SkinTable::parse(
            "[Human]\nCursor=UI\\Cursor\\HumanCursor.mdl\n[Default]\nConsoleTexture01=UI\\Console\\Human\\HumanUITile01\nMasterFont=Fonts\\FRIZQT__.TTF\n",
        );
        assert_eq!(
            table.resolve_texture("ConsoleTexture01").as_deref(),
            Some("UI\\Console\\Human\\HumanUITile01.blp")
        );
        assert_eq!(
            table.font_entries(),
            vec![("MasterFont".to_string(), "Fonts\\FRIZQT__.TTF".to_string())]
        );
        let mut table = table;
        assert_eq!(
            table.get("Cursor"),
            None,
            "race keys are not visible from Default"
        );
        assert!(!table.set_race("Orc"));
        assert!(table.set_race("Human"));
        assert_eq!(table.get("Cursor"), Some("UI\\Cursor\\HumanCursor.mdl"));
    }

    struct FakeResolver {
        texts: HashMap<String, String>,
        textures: HashMap<String, TextureImage>,
        requested: Vec<String>,
    }

    impl UiResourceResolver for FakeResolver {
        fn read_ui_text(&mut self, path: &str) -> Option<String> {
            self.texts.get(path).cloned()
        }

        fn read_ui_texture(&mut self, path: &str) -> Option<TextureImage> {
            self.requested.push(path.to_string());
            self.textures.get(path).cloned()
        }
    }

    fn tiny_texture() -> TextureImage {
        TextureImage {
            width: 1,
            height: 1,
            rgba: vec![255, 255, 255, 255],
        }
    }

    #[test]
    fn race_switch_reloads_decorated_textures() {
        let skin = SkinTable::parse(
            "[Orc]\nConsoleTexture01=UI\\Console\\Orc\\OrcUITile01\n\
             [Default]\nConsoleTexture01=UI\\Console\\Human\\HumanUITile01\n",
        );
        let mut theme = UiTheme::from_documents(Vec::new(), skin);
        let mut resolver = FakeResolver {
            texts: HashMap::new(),
            textures: HashMap::from([
                (
                    "UI\\Console\\Human\\HumanUITile01.blp".to_string(),
                    tiny_texture(),
                ),
                (
                    "UI\\Console\\Orc\\OrcUITile01.blp".to_string(),
                    tiny_texture(),
                ),
            ]),
            requested: Vec::new(),
        };
        let mut textures = Vec::new();
        let human = theme
            .load_texture_key(&mut resolver, &mut textures, "ConsoleTexture01", true)
            .expect("human texture");
        assert_eq!(
            theme.texture_info("ConsoleTexture01").unwrap().path,
            "UI\\Console\\Human\\HumanUITile01.blp"
        );
        assert!(theme.set_race("Orc"));
        let orc = theme
            .load_texture_key(&mut resolver, &mut textures, "ConsoleTexture01", true)
            .expect("orc texture");
        assert_ne!(human, orc, "a new Assets::textures entry is appended");
        assert_eq!(
            theme.texture_info("ConsoleTexture01").unwrap().path,
            "UI\\Console\\Orc\\OrcUITile01.blp"
        );
        assert_eq!(textures.len(), 2);
    }

    #[test]
    fn command_icon_probes_replaceable_textures() {
        let mut theme = UiTheme::default();
        let mut resolver = FakeResolver {
            texts: HashMap::new(),
            textures: HashMap::from([(
                "ReplaceableTextures\\CommandButtons\\BTNStop.blp".to_string(),
                tiny_texture(),
            )]),
            requested: Vec::new(),
        };
        let mut textures = Vec::new();
        let index = theme
            .load_command_icon(&mut resolver, &mut textures, "BTNStop")
            .expect("icon");
        assert_eq!(index, 0);
        assert_eq!(
            theme.texture_info("BTNStop").unwrap().path,
            "ReplaceableTextures\\CommandButtons\\BTNStop.blp"
        );
        assert!(
            resolver
                .requested
                .iter()
                .any(|path| path.contains("CommandButtons"))
        );
    }

    #[test]
    fn menu_labels_come_from_fdf_and_stringlist() {
        let document = FdfDocument::parse(
            r#"
            StringList {
                KEY_PLAY "Play",
                KEY_OPTIONS "Options",
            }
            Frame "FRAME" "Root" {
                Frame "GLUETEXTBUTTON" "PlayButton" {
                    Width 0.2, Height 0.05,
                    ButtonText "PlayText",
                    Frame "TEXT" "PlayText" {
                        Text "KEY_PLAY",
                    }
                }
                Frame "TEXT" "OptionsLabel" {
                    Text "KEY_OPTIONS",
                }
            }
        "#,
        )
        .expect("fdf");
        let skin = SkinTable::parse(
            "[Default]\nMenuBackground=UI\\Glues\\MenuBackground.blp\n",
        );
        let theme = UiTheme::from_documents(vec![document], skin);
        assert_eq!(
            theme.skin_string("MenuBackground"),
            Some("UI\\Glues\\MenuBackground.blp")
        );
        let labels = theme.menu_labels("Root");
        assert_eq!(
            labels.len(),
            2,
            "button label + standalone text, no duplicate: {:?}",
            labels.iter().map(|label| (&label.frame, &label.text)).collect::<Vec<_>>()
        );
        let play = labels
            .iter()
            .find(|label| label.frame == "PlayButton")
            .expect("button label");
        assert_eq!(play.text, "Play");
        assert_eq!(play.key.as_deref(), Some("PlayText"));
        assert!((play.style.exact_size_px(1080.0) - 10.8).abs() < 1e-4);
        let options = theme.menu_label("Root", "OptionsLabel").expect("text label");
        assert_eq!(options.text, "Options");
        assert_eq!(options.key.as_deref(), Some("KEY_OPTIONS"));
    }

    #[test]
    fn tooltip_styles_have_exact_and_phone_sizes() {
        let parity = TooltipStyle::parity();
        assert!((parity.text.exact_size_px(1080.0) - 11.88).abs() < 1e-4);
        assert!((parity.text.size_px(1080.0) - 11.88).abs() < 1e-4);
        let phone = TooltipStyle::phone();
        assert!((phone.text.size_px(1080.0) - 24.0).abs() < 1e-4);
    }

    #[test]
    fn retail_main_menu_labels_and_tooltip_strings_parity() {
        use std::path::Path;

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
        if !root.is_dir() {
            eprintln!("skip: analysis/war3_full3 missing");
            return;
        }
        let read = |relative: &str| {
            let path = root.join(relative);
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            FdfDocument::parse_named(&text, relative.replace('/', "\\")).expect("parse FDF")
        };
        let documents = vec![
            read("UI/FrameDef/Glue/MainMenu.fdf"),
            read("UI/FrameDef/Glue/StandardTemplates.fdf"),
            read("UI/FrameDef/GlobalStrings.fdf"),
        ];
        let theme = UiTheme::from_documents(documents, SkinTable::default());
        let label = theme
            .menu_label("MainMenuFrame", "SinglePlayerButton")
            .expect("SinglePlayerButton");
        assert_ne!(label.text, "KEY_SINGLE_PLAYER");
        assert!(!label.text.is_empty());
        assert!(
            label.text.chars().any(|character| character as u32 >= 0x400),
            "expected localized menu text, got {:?}",
            label.text
        );
        assert!((label.style.exact_size_px(1080.0) - 14.04).abs() < 1e-3);

        let tooltip = theme.tooltip_text("REQUIRESTOOLTIP");
        assert!(tooltip.contains("|C") || tooltip.contains("|c"));
        let plain = theme.tooltip_plain("REQUIRESTOOLTIP");
        assert!(!plain.is_empty());
        assert!(!plain.contains("|C"));
    }

    #[test]
    fn tooltip_skin_resolves_war3skins_art() {
        let skin = SkinTable::parse(
            "[Default]\nToolTipBackground=UI\\Widgets\\ToolTips\\Human\\human-tooltip-background\nToolTipBorder=UI\\Widgets\\ToolTips\\Human\\human-tooltip-border\nToolTipGoldIcon=UI\\Widgets\\ToolTips\\Human\\ToolTipGoldIcon\nUnitTipBackground=UI\\Widgets\\ToolTips\\Human\\human-tooltip-background2\n",
        );
        let theme = UiTheme::from_documents(Vec::new(), skin);
        let tooltip = theme.tooltip_skin();
        assert_eq!(tooltip.background.key, "ToolTipBackground");
        assert_eq!(
            tooltip.background.path.as_deref(),
            Some("UI\\Widgets\\ToolTips\\Human\\human-tooltip-background.blp")
        );
        assert_eq!(
            tooltip.border.path.as_deref(),
            Some("UI\\Widgets\\ToolTips\\Human\\human-tooltip-border.blp")
        );
        assert_eq!(
            tooltip.gold.path.as_deref(),
            Some("UI\\Widgets\\ToolTips\\Human\\ToolTipGoldIcon.blp")
        );
        assert_eq!(
            tooltip.unit_background.path.as_deref(),
            Some("UI\\Widgets\\ToolTips\\Human\\human-tooltip-background2.blp")
        );
    }

    #[test]
    fn tooltip_format_and_layout_are_renderer_independent() {
        let mut theme = UiTheme::from_documents(Vec::new(), SkinTable::default());
        theme
            .localization_mut()
            .insert("REQUIRESTOOLTIP", "|Cffffff00Requires: %s|R");
        let formatted = theme.format_tooltip("REQUIRESTOOLTIP", &["Keep"]);
        assert_eq!(formatted, "|Cffffff00Requires: Keep|R");
        assert_eq!(format_tooltip_text("100%% %d %s", &[]), "100% %d %s");
        let font = Font::load();
        let style = TooltipStyle::default();
        let screen = Vec2::new(1280.0, 720.0);
        let layout = theme.layout_tooltip(&formatted, Vec2::new(1200.0, 700.0), screen, &font, &style);
        assert!(layout.rect.w > 0.0 && layout.rect.h > 0.0);
        assert!(layout.rect.x + layout.rect.w <= screen.x + 0.01);
        assert!(layout.rect.y + layout.rect.h <= screen.y + 0.01);
        assert!(layout.content.x >= layout.rect.x);
        assert!(layout.content.y >= layout.rect.y);
        let skin = theme.tooltip_skin();
        let mut batch = UiBatch::default();
        theme.draw_tooltip(&mut batch, &layout, &skin, &font, &style);
        assert!(!batch.font.is_empty(), "tooltip text quads emitted");
    }

    #[test]
    fn template_merge_keeps_property_order_and_children() {
        let base = FdfDocument::parse(
            r#"
            Frame "TEXT" "Base" {
                FrameFont "MasterFont", 0.01, "",
                SetPoint TOPLEFT, "Root", TOPLEFT, 0.0, 0.0,
                Frame "TEXT" "Label" { Text "from base", }
            }
        "#,
        )
        .unwrap();
        let child = FdfDocument::parse(
            r#"
            Frame "TEXT" "Child" INHERITS WITHCHILDREN "Base" {
                SetPoint TOPRIGHT, "Root", TOPRIGHT, 0.0, 0.0,
                Frame "TEXT" "Label" { Text "from child", }
            }
        "#,
        )
        .unwrap();
        let theme = UiTheme::from_documents(vec![base, child], SkinTable::default());
        let tree = theme.resolve_tree("Child").unwrap();
        let root = tree.node(tree.roots[0]);
        assert_eq!(root.set_points().len(), 1);
        assert_eq!(root.set_points()[0].point, "TOPRIGHT");
        assert_eq!(root.font().unwrap().name, "MasterFont");
        assert_eq!(root.children.len(), 1, "same-name child merged");
        let label = tree.node(root.children[0]);
        assert_eq!(label.text(), Some("from child"));
    }

    #[test]
    fn anchor_layout_solves_opposing_points() {
        let document = FdfDocument::parse(
            r#"
            Frame "SIMPLEFRAME" "Root" {
                Frame "BACKDROP" "Panel" {
                    SetAllPoints,
                }
                Frame "TEXTURE" "Stretched" {
                    SetPoint TOPLEFT, "Panel", TOPLEFT, 0.0, 0.0,
                    SetPoint BOTTOMRIGHT, "Panel", BOTTOMRIGHT, 0.0, 0.0,
                }
            }
        "#,
        )
        .unwrap();
        let theme = UiTheme::from_documents(vec![document], SkinTable::default());
        let layout = theme
            .build_layout(
                "Root",
                &LayoutConfig::new(Vec2::new(1000.0, 500.0)),
                &TextBindings::new(),
                None,
            )
            .unwrap();
        let stretched = layout.named("Stretched").unwrap();
        assert_eq!(stretched.rect.x, 0.0);
        assert_eq!(stretched.rect.w, 1000.0);
        assert_eq!(stretched.rect.h, 500.0);
    }

    #[test]
    fn hit_test_prefers_topmost_child() {
        let document = FdfDocument::parse(
            r#"
            Frame "SIMPLEFRAME" "Root" {
                Frame "BACKDROP" "Background" { SetAllPoints, }
                Frame "TEXTBUTTON" "Button" {
                    Width 0.25, Height 0.25,
                    SetPoint TOPLEFT, "Root", TOPLEFT, 0.5, -0.5,
                }
            }
        "#,
        )
        .unwrap();
        let theme = UiTheme::from_documents(vec![document], SkinTable::default());
        let layout = theme
            .build_layout(
                "Root",
                &LayoutConfig::new(Vec2::new(1000.0, 500.0)),
                &TextBindings::new(),
                None,
            )
            .unwrap();
        let button = layout.named("Button").unwrap();
        let hit = layout.hit_test(button.centre()).unwrap();
        assert_eq!(layout.node(hit).name.as_deref(), Some("Button"));
        assert_eq!(layout.hit_test(Vec2::new(-1.0, -1.0)), None);
    }

    #[test]
    fn console_text_and_tooltip_faces_are_locale_aware() {
        let theme = UiTheme::from_documents(Vec::new(), SkinTable::default());
        let console = theme.console_text_style();
        assert_eq!(console.font, "ChatFont");
        assert!((console.size - 0.011).abs() < 1e-6);
        let en = theme.console_text_face("enUS").unwrap();
        assert!(en.path.contains("FRIZQT__"), "enUS -> {}", en.path);
        assert_eq!(en.source, FontSource::Game);
        let ru = theme.console_text_face("ruRU").unwrap();
        assert!(ru.path.contains("FRIZQT__"), "ruRU -> {}", ru.path);
        let ko = theme.console_text_face("koKR").unwrap();
        assert!(ko.path.ends_with("2002.TTF"), "koKR -> {}", ko.path);
        // Tooltip parity style keeps its exact FDF size through resolution.
        let tip = TooltipStyle::parity();
        let tip_ru = theme.tooltip_text_face(&tip, "ruRU").unwrap();
        assert!(tip_ru.path.contains("FRIZQT__"), "tooltip ruRU -> {}", tip_ru.path);
        assert!((tip_ru.size - 0.011).abs() < 1e-6);
        let tip_en = theme.tooltip_text_face(&tip, "enUS").unwrap();
        assert!((tip_en.size - 0.011).abs() < 1e-6);
    }
}

// ---------------------------------------------------------------- selection

/// Game-side bridge for the [`wc3_input::selection`] policy (P0-10
/// `ui.selection.double` / `ui.selection.add`).
///
/// The policy in `wc3-input` owns the rules (same-type on screen, Shift/long-
/// press add, box merge); this module only turns the canonical sim/camera into
/// snapshots and writes the resulting ids back into `Game::selection`. The
/// tap/box/long-press handlers in `game.rs` call these three functions — the
/// exact hook is `analysis/agent-touch/patches/0003-selection-parity.patch`.
pub mod selection {
    use glam::{Vec2, Vec3};
    use wc3_input::selection::{self as policy, UnitSnapshot};
    use wc3_sim::{UnitId, UnitKind};

    use crate::game::Game;

    /// One snapshot per living unit with its ground projection. WC3 judges the
    /// "on screen" test on the unit's footprint, so the ground plane (height 0)
    /// is the projection point, not the model top used for picking.
    pub fn snapshot(game: &Game) -> Vec<UnitSnapshot> {
        game.sim
            .alive_units()
            .map(|unit| {
                let snapshot = if unit.is_building() {
                    UnitSnapshot::new(unit.id.0, unit.owner, unit.kind as u32, unit.pos).building()
                } else {
                    UnitSnapshot::new(unit.id.0, unit.owner, unit.kind as u32, unit.pos)
                };
                match game
                    .camera
                    .world_to_screen(Vec3::new(unit.pos.x, 0.0, unit.pos.y))
                {
                    Some(screen) => snapshot.on_screen(screen),
                    None => snapshot,
                }
            })
            .collect()
    }

    /// WC3 double-click: every living unit of the same type visible on screen.
    pub fn type_select_on_screen(game: &mut Game, kind: UnitKind) {
        let units = snapshot(game);
        game.selection = policy::same_type_on_screen(
            &units,
            game.player,
            kind as u32,
            game.viewport,
        )
        .into_iter()
        .map(UnitId)
        .collect();
    }

    /// One tap: replace, or append when the host modifier (Shift) is held.
    pub fn tap_select(game: &mut Game, id: UnitId, additive: bool) {
        let mut ids: Vec<u32> = game.selection.iter().map(|unit| unit.0).collect();
        let mode = if additive {
            policy::Mode::Add
        } else {
            policy::Mode::Replace
        };
        policy::merge(&mut ids, id.0, mode);
        game.selection = ids.into_iter().map(UnitId).collect();
    }

    /// Box select over world corners; `additive` keeps the current group.
    pub fn box_select(game: &mut Game, min: Vec2, max: Vec2, additive: bool) {
        let units = snapshot(game);
        let current: Vec<u32> = game.selection.iter().map(|unit| unit.0).collect();
        game.selection = policy::box_selection(&units, game.player, min, max, additive, &current)
            .into_iter()
            .map(UnitId)
            .collect();
    }

    /// Long-press add: the picked friendly unit joins the existing group.
    pub fn long_press_add(game: &mut Game, id: UnitId) {
        tap_select(game, id, true);
    }
}

// ---------------------------------------------------------------- hotkeys

/// Game-side hotkey-table bridge (`ui.hotkeys.remap`). `game.rs` keeps one
/// [`wc3_input::remap::Remap`] on `Game` and resolves card input through it;
/// this module maps `UiAction` to the WC3 action ids used by
/// `CommandStrings.txt` sections and object-data `Hotkey=` entries.
pub mod hotkeys {
    use wc3_input::remap::Remap;

    use crate::game::UiAction;

    /// WC3 action id for a card action (the `[Cmd…]` section names).
    pub fn action_id(action: UiAction) -> &'static str {
        match action {
            UiAction::AttackMove => "CmdAttack",
            UiAction::Move => "CmdMove",
            UiAction::Stop => "CmdStop",
            UiAction::Hold => "CmdHold",
            UiAction::Patrol => "CmdPatrol",
            UiAction::Gather => "CmdGather",
            UiAction::SelectArmy => "CmdSelectArmy",
            UiAction::CentreBase => "CmdBuild",
            UiAction::BuildPageNext => "CmdBuildPage",
            UiAction::Build(_) | UiAction::BuildDef(_) => "CmdBuild",
            UiAction::Train(_) | UiAction::TrainDef(_) => "CmdTrain",
            UiAction::UseItem(_) => "CmdUseItem",
            UiAction::QuestLog => "CmdQuestLog",
            UiAction::Cast(_) => "CmdCast",
            UiAction::Research(_) => "CmdResearch",
            UiAction::CancelConstruction => "CmdCancel",
            UiAction::PickupNearestItem => "CmdPickupItem",
            UiAction::BuyItem(_) => "CmdBuyItem",
            UiAction::BuyUnit(_) => "CmdBuyUnit",
            UiAction::ZoomIn => "CmdZoomIn",
            UiAction::ZoomOut => "CmdZoomOut",
            UiAction::Restart => "CmdRestart",
        }
    }

    /// Default table: the WC3 `CommandStrings.txt` letters. A caller holding
    /// the operator's data can `apply_command_strings` / `apply_object_data`
    /// on it before installing it on the game.
    pub fn game_remap() -> Remap {
        Remap::wc3()
    }
}
