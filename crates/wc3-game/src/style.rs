//! HUD design tokens and the one primitive the console renderer needs.
//!
//! The console is drawn with the real Warcraft III UI BLPs (`HumanUITile01..04`,
//! command-button chrome, resource icons). No procedural chrome, rounded pills
//! or invented glass textures live here: `Chrome` owns one 1×1 white texel used
//! for rectangular fills/bevels, and [`Tokens`] carries the sampled colour,
//! spacing and type scale from the console art.

use crate::assets::TextureImage;
use crate::hud::{NinePatch, Rect, UiBatch};
use crate::ui_atlas;

/// Retail tooltip gold (`|cFFFFCC00` family; TFT `CToolTipWar3` uses
/// `FED312` for the cost/requirement section).
pub const GOLD: [f32; 4] = [0.996, 0.827, 0.070, 1.0];

/// Design tokens shared by the console HUD.
///
/// Colours are sRGB floats sampled from the WC3 console/command-button art:
/// dark iron, warm brown leather, gold trim and parchment text. There is no
/// blue glass palette.
#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub panel: [f32; 4],
    pub panel_hi: [f32; 4],
    pub panel_pressed: [f32; 4],
    pub stroke: [f32; 4],
    pub stroke_hot: [f32; 4],
    pub text: [f32; 4],
    pub text_muted: [f32; 4],
    pub gold: [f32; 4],
    pub lumber: [f32; 4],
    pub supply: [f32; 4],
    pub hp_good: [f32; 4],
    pub hp_warn: [f32; 4],
    pub hp_low: [f32; 4],
    pub danger: [f32; 4],
    pub success: [f32; 4],
    pub disabled: [f32; 4],
    pub shadow: [f32; 4],
    pub radius_sm: f32,
    pub radius_button: f32,
    pub radius_card: f32,
    pub radius_pill: f32,
    pub gap: f32,
    pub margin: f32,
    pub touch_primary: f32,
    pub touch_context: f32,
    pub touch_chip: f32,
    pub type_value: f32,
    pub type_label: f32,
    pub type_micro: f32,
    pub type_title: f32,
    pub type_display: f32,
}

impl Default for Tokens {
    fn default() -> Self {
        Self {
            panel: [0.055, 0.045, 0.035, 0.98],
            panel_hi: [0.10, 0.085, 0.06, 0.98],
            panel_pressed: [0.055, 0.045, 0.03, 0.98],
            stroke: [0.52, 0.42, 0.22, 0.90],
            stroke_hot: [0.72, 0.58, 0.28, 0.95],
            text: [0.93, 0.88, 0.76, 1.0],
            text_muted: [0.68, 0.62, 0.50, 0.95],
            gold: [0.95, 0.76, 0.30, 1.0],
            lumber: [0.48, 0.85, 0.56, 1.0],
            supply: [0.44, 0.71, 1.0, 1.0],
            hp_good: [0.32, 0.86, 0.52, 1.0],
            hp_warn: [0.95, 0.76, 0.30, 1.0],
            hp_low: [1.0, 0.42, 0.40, 1.0],
            danger: [1.0, 0.35, 0.37, 1.0],
            success: [0.36, 0.89, 0.54, 1.0],
            disabled: [0.40, 0.36, 0.28, 0.55],
            shadow: [0.0, 0.0, 0.0, 0.65],
            radius_sm: 0.0,
            radius_button: 0.0,
            radius_card: 0.0,
            radius_pill: 0.0,
            gap: 4.0,
            margin: 12.0,
            touch_primary: 64.0,
            touch_context: 64.0,
            touch_chip: 64.0,
            type_value: 20.0 / 30.0,
            type_label: 13.0 / 30.0,
            type_micro: 11.0 / 30.0,
            type_title: 22.0 / 30.0,
            type_display: 40.0 / 30.0,
        }
    }
}

impl Tokens {
    pub fn hp(&self, ratio: f32) -> [f32; 4] {
        if ratio > 0.55 {
            self.hp_good
        } else if ratio > 0.25 {
            self.hp_warn
        } else {
            self.hp_low
        }
    }
}

/// Renderer handle for the flat HUD fill, the nine-patch table and the element
/// names of an installed UI atlas.
///
/// Every frame texture the asset loader recognises is registered here as a
/// [`NinePatch`] with insets measured from its own alpha/key structure, so the
/// HUD can draw it through [`UiBatch::nine_patch`] instead of stretching the
/// whole quad. Atlases add *named* elements (`button_normal`, `panel_tooltip`,
/// ...) that are addressed by name, which is how the painter's sheet reaches the
/// command card without a pixel constant in the draw code.
pub struct Chrome {
    /// 1×1 opaque white, sampled for rectangular fills and bevel bars.
    pub flat: usize,
    /// Nine-patch geometry per texture index (`Assets::textures`).
    patches: Vec<(usize, NinePatch)>,
    /// Named atlas elements: element name -> patch.
    named: Vec<(String, NinePatch)>,
}

impl Chrome {
    pub fn new(textures: &mut Vec<TextureImage>) -> Self {
        let flat = textures.len();
        textures.push(TextureImage::white());
        Self {
            flat,
            patches: Vec::new(),
            named: Vec::new(),
        }
    }

    /// Register a frame texture, measuring its corner insets. The texture is the
    /// whole frame (`ConsoleTexture01`, `CommandButtonBorder`, ...).
    pub fn register_frame(&mut self, texture: usize, image: &TextureImage) {
        if image.width == 0 || image.height == 0 {
            return;
        }
        let Ok(rgba) = ui_atlas::Rgba8::from_rgba(image.width, image.height, image.rgba.clone())
        else {
            return;
        };
        let source = Rect::new(0.0, 0.0, image.width as f32, image.height as f32);
        let insets = ui_atlas::detect_insets(&rgba, source, ui_atlas::ChromaKey::MAGENTA);
        self.insert_patch(
            texture,
            NinePatch::full(texture, [image.width as f32, image.height as f32], insets),
        );
    }

    /// Register a frame texture with authored insets (no measurement).
    pub fn register_frame_insets(&mut self, texture: usize, atlas: [f32; 2], insets: [f32; 4]) {
        self.insert_patch(texture, NinePatch::full(texture, atlas, insets));
    }

    /// Register every element of a loaded atlas by name, and its texture.
    pub fn register_atlas(&mut self, texture: usize, atlas: &ui_atlas::Atlas) {
        let dims = [atlas.image.width as f32, atlas.image.height as f32];
        for element in &atlas.elements {
            if element.kind == ui_atlas::ElementKind::Content {
                // Content rects are addresses, not stretchable chrome.
                continue;
            }
            let patch = NinePatch::element(texture, element.rect, dims, element.insets);
            self.named
                .retain(|(name, _)| !name.eq_ignore_ascii_case(&element.name));
            self.named.push((element.name.clone(), patch));
            self.insert_patch(texture, patch);
        }
    }

    fn insert_patch(&mut self, texture: usize, patch: NinePatch) {
        if let Some(entry) = self.patches.iter_mut().find(|(index, _)| *index == texture) {
            entry.1 = patch;
        } else {
            self.patches.push((texture, patch));
        }
    }

    /// Nine-patch geometry for a frame texture, if it was registered.
    pub fn patch(&self, texture: usize) -> Option<NinePatch> {
        self.patches
            .iter()
            .find(|(index, _)| *index == texture)
            .map(|(_, patch)| *patch)
    }

    /// Named element of the installed atlas (`button_normal`, `panel_tooltip`).
    pub fn element_patch(&self, name: &str) -> Option<NinePatch> {
        self.named
            .iter()
            .find(|(element, _)| element.eq_ignore_ascii_case(name))
            .map(|(_, patch)| *patch)
    }

    /// Texture index of a named element.
    pub fn element_texture(&self, name: &str) -> Option<usize> {
        self.element_patch(name).map(|patch| patch.texture)
    }

    /// Number of registered named elements (atlas coverage evidence).
    pub fn element_count(&self) -> usize {
        self.named.len()
    }

    /// Draw a frame texture as a nine-patch. Returns false when the texture was
    /// never registered, so the caller can fall back to its bevel.
    pub fn frame(
        &self,
        batch: &mut UiBatch,
        rect: Rect,
        texture: Option<usize>,
        art_scale: f32,
        tint: [f32; 4],
    ) -> bool {
        let Some(texture) = texture else {
            return false;
        };
        let Some(patch) = self.patch(texture) else {
            return false;
        };
        batch.nine_patch(rect, &patch.with_art_scale(art_scale), tint);
        true
    }

    /// Draw a named atlas element as a nine-patch. Returns false when the atlas
    /// carries no such element.
    pub fn element(
        &self,
        batch: &mut UiBatch,
        rect: Rect,
        name: &str,
        art_scale: f32,
        tint: [f32; 4],
    ) -> bool {
        let Some(patch) = self.element_patch(name) else {
            return false;
        };
        batch.nine_patch(rect, &patch.with_art_scale(art_scale), tint);
        true
    }

    /// Draw a named element, then every fallback name in order. The first name
    /// the atlas carries wins.
    pub fn element_or<'a>(
        &self,
        batch: &mut UiBatch,
        rect: Rect,
        names: &[&'a str],
        art_scale: f32,
        tint: [f32; 4],
    ) -> Option<&'a str> {
        for name in names {
            if self.element(batch, rect, name, art_scale, tint) {
                return Some(name);
            }
        }
        None
    }

    /// Rectangular fill drawn through the textured batch, so later icons and
    /// text remain above it. Corner radius is always zero in the WC3 console.
    pub fn flat_rect(&self, batch: &mut UiBatch, rect: Rect, color: [f32; 4]) {
        batch.textured_quad(self.flat, rect, [0.0, 0.0, 1.0, 1.0], color);
    }
}
