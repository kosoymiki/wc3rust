//! Screen-space HUD: bitmap text, command-card layout, minimap and panels.
//!
//! Text is rasterised once from the device's system font (`Roboto` on Android)
//! into an atlas; if no font file is readable a compact 5x7 fallback keeps the
//! HUD legible. All HUD text is drawn upper-case, which is both a deliberate
//! RTS style and what the fallback font can represent.

use std::collections::HashMap;

use crate::assets::TextureImage;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, point: glam::Vec2) -> bool {
        point.x >= self.x
            && point.y >= self.y
            && point.x < self.x + self.w
            && point.y < self.y + self.h
    }

    pub fn centre(&self) -> glam::Vec2 {
        glam::Vec2::new(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn inset(&self, dx: f32, dy: f32) -> Self {
        Self::new(
            self.x + dx,
            self.y + dy,
            (self.w - dx * 2.0).max(0.0),
            (self.h - dy * 2.0).max(0.0),
        )
    }

    pub fn inset_sides(&self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self::new(
            self.x + left,
            self.y + top,
            (self.w - left - right).max(0.0),
            (self.h - top - bottom).max(0.0),
        )
    }

    pub fn expanded(&self, dx: f32, dy: f32) -> Self {
        Self::new(
            self.x - dx,
            self.y - dy,
            self.w + dx * 2.0,
            self.h + dy * 2.0,
        )
    }

    /// Grow each side by its own amount (`[left, top, right, bottom]`), the
    /// inverse of [`Rect::inset_sides`]. Atlas frames have asymmetric nine
    /// patch insets, so the art rect around an image is not a uniform
    /// `expanded()`.
    pub fn expanded_sides(&self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self::new(
            self.x - left,
            self.y - top,
            self.w + left + right,
            self.h + top + bottom,
        )
    }

    pub fn with_position(&self, x: f32, y: f32) -> Self {
        Self::new(x, y, self.w, self.h)
    }

    pub fn with_size(&self, w: f32, h: f32) -> Self {
        Self::new(self.x, self.y, w, h)
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    pub fn intersection_area(&self, other: &Rect) -> f32 {
        let w = (self.right().min(other.right()) - self.x.max(other.x)).max(0.0);
        let h = (self.bottom().min(other.bottom()) - self.y.max(other.y)).max(0.0);
        w * h
    }

    pub fn contains_rect(&self, other: &Rect) -> bool {
        other.x >= self.x - 0.01
            && other.y >= self.y - 0.01
            && other.right() <= self.right() + 0.01
            && other.bottom() <= self.bottom() + 0.01
    }

    pub fn is_inside(&self, outer: &Rect) -> bool {
        outer.contains_rect(self)
    }

    /// Shrink a rect to fit inside `outer`, preserving its size where possible.
    pub fn clamped_inside(&self, outer: &Rect) -> Self {
        let w = self.w.min(outer.w);
        let h = self.h.min(outer.h);
        Self::new(
            self.x.clamp(outer.x, outer.right() - w),
            self.y.clamp(outer.y, outer.bottom() - h),
            w,
            h,
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UiTexture {
    White,
    Font,
    Minimap,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// A textured UI draw (console chrome, resource icons, command buttons).
#[derive(Clone, Default)]
pub struct UiDraw {
    pub texture: usize,
    pub vertices: Vec<UiVertex>,
}

#[derive(Default)]
pub struct UiBatch {
    pub white: Vec<UiVertex>,
    pub font: Vec<UiVertex>,
    pub minimap: Vec<UiVertex>,
    pub textured: Vec<UiDraw>,
}

/// sRGB-authored UI colour -> linear.
///
/// The UI pass writes into an `Rgba8UnormSrgb` surface, so the GPU re-encodes
/// every written value to sRGB. Vertex colours must therefore be linear, or a
/// colour authored as sRGB is displayed gamma-brightened: measured on device, a
/// `(36,30,26)` strip rendered as `(104,96,89)` — the washed-out HUD the
/// operator reported. Textures are sRGB-sampled (already linear), so the tint
/// multiply stays correct.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear back to sRGB, for CPU rasterizers that must match the device output.
pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn linear_color(color: [f32; 4]) -> [f32; 4] {
    [
        srgb_to_linear(color[0]),
        srgb_to_linear(color[1]),
        srgb_to_linear(color[2]),
        color[3],
    ]
}

/// Linear vertex colour -> the sRGB-authored colour it came from.
///
/// The exact inverse of [`linear_color`] on the three colour channels, with
/// alpha passed through. This is what an `Rgba8UnormSrgb` surface shows on
/// screen, so a test that wants to speak about the authored tint (the
/// `*.tint` constants are authored in sRGB) must compare through this instead
/// of comparing the linear vertex value to the authored constant.
pub fn authored_color(color: [f32; 4]) -> [f32; 4] {
    [
        linear_to_srgb(color[0]),
        linear_to_srgb(color[1]),
        linear_to_srgb(color[2]),
        color[3],
    ]
}

/// Rec. 709 relative luminance of a **linear** colour.
///
/// "Darker" is a statement about emitted light, i.e. about linear luminance;
/// averaging the sRGB-encoded channels is not the same ranking once the vertex
/// colours are linear (a 0.65 tint multiplier in sRGB is a 0.36 luminance
/// ratio). Alpha is ignored.
pub fn linear_luminance(color: [f32; 4]) -> f32 {
    0.2126 * color[0] + 0.7152 * color[1] + 0.0722 * color[2]
}

impl UiBatch {
    pub fn clear(&mut self) {
        self.white.clear();
        self.font.clear();
        self.minimap.clear();
        self.textured.clear();
    }

    /// Append a textured quad using an index into `Assets::textures`.
    pub fn textured_quad(&mut self, texture: usize, rect: Rect, uv: [f32; 4], color: [f32; 4]) {
        let color = linear_color(color);
        let (x0, y0, x1, y1) = (rect.x, rect.y, rect.x + rect.w, rect.y + rect.h);
        let vertex = |x: f32, y: f32, u: f32, v: f32| UiVertex {
            pos: [x, y],
            uv: [u, v],
            color,
        };
        // Preserve painter order: only the immediately previous draw may be
        // extended. Searching backwards merged later background quads into an
        // earlier draw of the same texture, which made the real console tiles
        // paint over the card plate and icons.
        let target = match self.textured.last_mut() {
            Some(draw) if draw.texture == texture && draw.vertices.len() < 60000 => draw,
            _ => {
                self.textured.push(UiDraw {
                    texture,
                    vertices: Vec::new(),
                });
                self.textured.last_mut().expect("just pushed")
            }
        };
        target.vertices.extend_from_slice(&[
            vertex(x0, y0, uv[0], uv[1]),
            vertex(x1, y0, uv[2], uv[1]),
            vertex(x1, y1, uv[2], uv[3]),
            vertex(x0, y0, uv[0], uv[1]),
            vertex(x1, y1, uv[2], uv[3]),
            vertex(x0, y1, uv[0], uv[3]),
        ]);
    }

    fn push(&mut self, texture: UiTexture, vertices: [UiVertex; 6]) {
        match texture {
            UiTexture::White => self.white.extend_from_slice(&vertices),
            UiTexture::Font => self.font.extend_from_slice(&vertices),
            UiTexture::Minimap => self.minimap.extend_from_slice(&vertices),
        }
    }

    pub fn rect(&mut self, rect: Rect, color: [f32; 4]) {
        self.quad(UiTexture::White, rect, [0.0, 0.0, 1.0, 1.0], color);
    }

    pub fn quad(&mut self, texture: UiTexture, rect: Rect, uv: [f32; 4], color: [f32; 4]) {
        let color = linear_color(color);
        let (x0, y0, x1, y1) = (rect.x, rect.y, rect.x + rect.w, rect.y + rect.h);
        let vertex = |x: f32, y: f32, u: f32, v: f32| UiVertex {
            pos: [x, y],
            uv: [u, v],
            color,
        };
        self.push(
            texture,
            [
                vertex(x0, y0, uv[0], uv[1]),
                vertex(x1, y0, uv[2], uv[1]),
                vertex(x1, y1, uv[2], uv[3]),
                vertex(x0, y0, uv[0], uv[1]),
                vertex(x1, y1, uv[2], uv[3]),
                vertex(x0, y1, uv[0], uv[3]),
            ],
        );
    }

    pub fn border(&mut self, rect: Rect, width: f32, color: [f32; 4]) {
        self.rect(Rect::new(rect.x, rect.y, rect.w, width), color);
        self.rect(
            Rect::new(rect.x, rect.y + rect.h - width, rect.w, width),
            color,
        );
        self.rect(Rect::new(rect.x, rect.y, width, rect.h), color);
        self.rect(
            Rect::new(rect.x + rect.w - width, rect.y, width, rect.h),
            color,
        );
    }

    /// Draw one nine-patch into `rect`.
    ///
    /// The corners keep their source size (`art_scale * insets`), so a panel can
    /// grow or shrink without deforming its ornament; the edge bands and the
    /// centre take up the difference. With `patch.tile` the edge bands repeat
    /// their source pixels instead of stretching (the UI sampler clamps, so the
    /// repeats are emitted as separate quads with a fractional tail).
    pub fn nine_patch(&mut self, rect: Rect, patch: &NinePatch, tint: [f32; 4]) {
        if rect.w <= 0.0 || rect.h <= 0.0 || patch.source.w <= 0.0 || patch.source.h <= 0.0 {
            return;
        }
        let slices = nine_slices(rect, patch);
        for slice in &slices {
            if slice.dest.w <= 0.0 || slice.dest.h <= 0.0 {
                continue;
            }
            // Rows 1 and columns 1 are the stretchable bands; 4 is the centre.
            let row = slice.index / 3;
            let column = slice.index % 3;
            let tile_x = patch.tile && column == 1 && slice.dest.w > 0.0;
            let tile_y = patch.tile && row == 1 && slice.dest.h > 0.0;
            if !tile_x && !tile_y {
                self.textured_quad(patch.texture, slice.dest, slice.uv, tint);
                continue;
            }
            self.tiled_slice(patch, slice, tint, tile_x, tile_y);
        }
    }

    /// Emit one stretchable band as repeated full source periods plus a
    /// fractional tail that shows the leading part of the band.
    fn tiled_slice(
        &mut self,
        patch: &NinePatch,
        slice: &NinePatchSlice,
        tint: [f32; 4],
        tile_x: bool,
        tile_y: bool,
    ) {
        let atlas = [patch.atlas[0].max(1.0), patch.atlas[1].max(1.0)];
        let source_w = ((slice.uv[2] - slice.uv[0]) * atlas[0]).max(1.0);
        let source_h = ((slice.uv[3] - slice.uv[1]) * atlas[1]).max(1.0);
        // One period in destination space is the source band at source scale.
        let period_w = if tile_x { source_w * patch.art_scale.max(0.05) } else { slice.dest.w };
        let period_h = if tile_y { source_h * patch.art_scale.max(0.05) } else { slice.dest.h };
        // Whole periods plus one fractional tail: the band must cover its dest
        // rect exactly (a gap here is the 1 px seam the seam test forbids).
        let columns = if tile_x {
            (slice.dest.w / period_w.max(0.5)).ceil().max(1.0) as u32
        } else {
            1
        };
        let rows = if tile_y {
            (slice.dest.h / period_h.max(0.5)).ceil().max(1.0) as u32
        } else {
            1
        };
        for row in 0..rows {
            for column in 0..columns {
                let x0 = slice.dest.x + column as f32 * period_w;
                let y0 = slice.dest.y + row as f32 * period_h;
                let w = if tile_x {
                    period_w.min(slice.dest.right() - x0)
                } else {
                    slice.dest.w
                };
                let h = if tile_y {
                    period_h.min(slice.dest.bottom() - y0)
                } else {
                    slice.dest.h
                };
                if w <= 0.0 || h <= 0.0 {
                    continue;
                }
                let u1 = if tile_x {
                    slice.uv[0] + (slice.uv[2] - slice.uv[0]) * (w / period_w).clamp(0.0, 1.0)
                } else {
                    slice.uv[2]
                };
                let v1 = if tile_y {
                    slice.uv[1] + (slice.uv[3] - slice.uv[1]) * (h / period_h).clamp(0.0, 1.0)
                } else {
                    slice.uv[3]
                };
                self.textured_quad(
                    patch.texture,
                    Rect::new(x0, y0, w, h),
                    [slice.uv[0], slice.uv[1], u1, v1],
                    tint,
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Nine-patch
// ---------------------------------------------------------------------------

/// A nine-patch source: one rectangle inside an atlas plus the corner size.
///
/// `insets` are [`left`, `top`, `right`, `bottom`] in *source* pixels. They come
/// from the atlas manifest sidecar or from
/// [`crate::ui_atlas::detect_insets`], never from a constant in the draw code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NinePatch {
    /// Index into `Assets::textures`.
    pub texture: usize,
    /// Element rectangle inside that texture, in pixels.
    pub source: Rect,
    /// `[left, top, right, bottom]` corner size in source pixels.
    pub insets: [f32; 4],
    /// Source texture size in pixels, used to normalise the UVs.
    pub atlas: [f32; 2],
    /// Corner size multiplier in destination space (the UI scale).
    pub art_scale: f32,
    /// Repeat the edge bands instead of stretching them.
    pub tile: bool,
}

impl NinePatch {
    /// Full-texture patch (the retail case: one BLP per frame).
    pub fn full(texture: usize, atlas: [f32; 2], insets: [f32; 4]) -> Self {
        Self {
            texture,
            source: Rect::new(0.0, 0.0, atlas[0], atlas[1]),
            insets,
            atlas,
            art_scale: 1.0,
            tile: false,
        }
    }

    /// Patch for one element of a multi-element atlas.
    pub fn element(texture: usize, source: Rect, atlas: [f32; 2], insets: [f32; 4]) -> Self {
        Self {
            texture,
            source,
            insets,
            atlas,
            art_scale: 1.0,
            tile: false,
        }
    }

    pub fn with_art_scale(mut self, art_scale: f32) -> Self {
        self.art_scale = art_scale.max(0.05);
        self
    }

    pub fn tiled(mut self, tile: bool) -> Self {
        self.tile = tile;
        self
    }

    /// UV rectangle of the source element.
    pub fn uv(&self) -> [f32; 4] {
        let width = self.atlas[0].max(1.0);
        let height = self.atlas[1].max(1.0);
        [
            self.source.x / width,
            self.source.y / height,
            (self.source.x + self.source.w) / width,
            (self.source.y + self.source.h) / height,
        ]
    }

    /// Corner sizes in source pixels after clamping to the source element.
    pub fn source_corners(&self) -> [f32; 4] {
        [
            self.insets[0].max(0.0).min(self.source.w.max(1.0)),
            self.insets[1].max(0.0).min(self.source.h.max(1.0)),
            self.insets[2].max(0.0).min(self.source.w.max(1.0)),
            self.insets[3].max(0.0).min(self.source.h.max(1.0)),
        ]
    }

    /// Corner sizes in destination pixels: source corners times `art_scale`,
    /// clamped so the rect keeps at least a 1 px middle band on both axes.
    pub fn dest_corners(&self, rect: Rect) -> [f32; 4] {
        let source = self.source_corners();
        let scale = self.art_scale.max(0.05);
        let mut corners = [
            source[0] * scale,
            source[1] * scale,
            source[2] * scale,
            source[3] * scale,
        ];
        if corners[0] + corners[2] > rect.w - 1.0 {
            let fit = ((rect.w - 1.0).max(0.0)) / (corners[0] + corners[2]).max(1.0);
            corners[0] *= fit;
            corners[2] *= fit;
        }
        if corners[1] + corners[3] > rect.h - 1.0 {
            let fit = ((rect.h - 1.0).max(0.0)) / (corners[1] + corners[3]).max(1.0);
            corners[1] *= fit;
            corners[3] *= fit;
        }
        corners
    }
}

/// One of the nine destination pieces with its source UV rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NinePatchSlice {
    /// Row-major index: 0 top-left, 1 top, 2 top-right, 3 left, 4 centre, 5 right,
    /// 6 bottom-left, 7 bottom, 8 bottom-right.
    pub index: usize,
    pub dest: Rect,
    pub uv: [f32; 4],
}

/// Split `rect` into the nine nine-patch pieces.
///
/// The three columns exactly cover `rect.w` and the three rows exactly cover
/// `rect.h` (no gap, no overlap), which is what the seam test asserts.
pub fn nine_slices(rect: Rect, patch: &NinePatch) -> [NinePatchSlice; 9] {
    let uv = patch.uv();
    let corners = patch.dest_corners(rect);
    let xs = [
        rect.x,
        rect.x + corners[0],
        rect.right() - corners[2],
        rect.right(),
    ];
    let ys = [
        rect.y,
        rect.y + corners[1],
        rect.bottom() - corners[3],
        rect.bottom(),
    ];
    // The UV split follows the *source* insets: a corner quad shows exactly
    // `insets` source texels, independent of how large the panel is.
    let source = patch.source_corners();
    let ax = patch.atlas[0].max(1.0);
    let ay = patch.atlas[1].max(1.0);
    let us = [
        uv[0],
        (patch.source.x + source[0]) / ax,
        (patch.source.x + patch.source.w - source[2]) / ax,
        uv[2],
    ];
    let vs = [
        uv[1],
        (patch.source.y + source[1]) / ay,
        (patch.source.y + patch.source.h - source[3]) / ay,
        uv[3],
    ];
    std::array::from_fn(|index| {
        let row = index / 3;
        let column = index % 3;
        NinePatchSlice {
            index,
            dest: Rect::new(
                xs[column],
                ys[row],
                (xs[column + 1] - xs[column]).max(0.0),
                (ys[row + 1] - ys[row]).max(0.0),
            ),
            uv: [us[column], vs[row], us[column + 1], vs[row + 1]],
        }
    })
}

#[derive(Clone)]
pub struct Glyph {
    pub uv: [f32; 4],
    pub width: f32,
    pub height: f32,
    pub offset: [f32; 2],
    pub advance: f32,
}

/// Where the glyph atlas came from. `Game` is the operator's
/// `Fonts\FRIZQT__.TTF`; `System` is a real system font; `Fallback` is the
/// built-in 5x7 bitmap (ASCII only, no invented Cyrillic glyphs).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FontSource {
    Game,
    System,
    #[default]
    Fallback,
}

/// Coverage report for the atlas: codepoints actually present vs missing.
/// Missing characters are skipped when drawing (no tofu boxes).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GlyphCoverage {
    pub covered: usize,
    pub missing: usize,
    pub ascii_covered: usize,
    pub cyrillic_covered: usize,
    pub missing_chars: Vec<char>,
}

pub struct Font {
    pub atlas: TextureImage,
    glyphs: HashMap<char, Glyph>,
    pub line_height: f32,
    /// Baseline offset from the top of the line box (pixels at atlas scale).
    pub ascent: f32,
    /// Advance used for a codepoint the atlas does not contain.
    pub space_advance: f32,
    pub source: FontSource,
    covered: Vec<char>,
    missing: Vec<char>,
    /// `Some(texture + 1)` routes glyph quads through `UiBatch::textured`
    /// instead of the single `UiBatch::font` channel, so the pack's serif,
    /// sans and mono faces can share one HUD batch. `0` = the font channel.
    external_texture: std::sync::atomic::AtomicUsize,
}

/// Charset rasterised into the atlas: printable ASCII, the full Cyrillic
/// block (the Russian `GlobalStrings.fdf` uses 60 of these) and WC3 UI
/// punctuation. Glyphs absent from the chosen font are recorded in
/// [`GlyphCoverage`] instead of being synthesised.
/// Minimum console/FDF text height in pixels (operator requirement: readable
/// Cyrillic on a phone; FDF's 0.01 fraction is ~10.8 px at 1080p).
pub const MIN_TEXT_PX: f32 = 24.0;

/// Dense-element floor: card badges, counters and chips. Below the prose floor
/// on purpose — a 64 px command slot carries a 2-4 character badge, not prose,
/// and the layout owns the size instead of a global 24 px override.
pub const MIN_DENSE_PX: f32 = 16.0;

/// Charset rasterised into the atlas: printable ASCII, Latin-1 supplement,
/// Latin Extended-A, the full Cyrillic block (the Russian `GlobalStrings.fdf`
/// uses 60 of these), the arithmetic/UI punctuation the HUD prints
/// (`0-9 · : + / - % ×`) and WC3 typographic punctuation. Glyphs absent from
/// the chosen font are recorded in [`GlyphCoverage`] instead of being
/// synthesised.
///
/// Coverage - not shaping - was the Cyrillic defect: fixed HUD labels are
/// already laid out correctly, so the HUD needs every codepoint the pack fonts
/// contain, not a shaping stack.
pub fn ui_charset() -> Vec<char> {
    const EXTRA: [char; 16] = [
        '№', '«', '»', '–', '—', '…', '’', '‘', '“', '”', '·', '×', '±', '≈', '≤', '≥',
    ];
    let mut chars: Vec<char> = (0x20u32..=0x7E).filter_map(char::from_u32).collect();
    // Latin-1 supplement (À..ÿ) and Latin Extended-A (Ā..ſ): the pack's DejaVu
    // faces carry them and map/file names use them.
    chars.extend((0xA0u32..=0xFF).filter_map(char::from_u32));
    chars.extend((0x100u32..=0x17F).filter_map(char::from_u32));
    chars.extend((0x400u32..=0x4FF).filter_map(char::from_u32));
    chars.extend(EXTRA);
    chars.sort_unstable();
    chars.dedup();
    chars
}

impl Font {
    pub fn load() -> Self {
        Self::load_from(None)
    }

    /// Prefer the operator's game font (`Fonts\FRIZQT__.TTF` bytes) when the
    /// assets layer provides them, then a real system font with Cyrillic
    /// coverage, then the ASCII bitmap fallback.
    pub fn load_from(game_font: Option<&[u8]>) -> Self {
        // A font is only usable for the Russian StringLists when it has real
        // Cyrillic glyphs. Prefer the game face when it covers them; otherwise
        // keep it as a style fallback and look for a real Cyrillic system face.
        let mut fallback = None;
        if let Some(bytes) = game_font
            && let Some(font) = Self::rasterize(bytes, 30.0, FontSource::Game)
        {
            if font.coverage().cyrillic_covered >= 60 {
                return font;
            }
            fallback = Some(font);
        }
        const CANDIDATES: [&str; 4] = [
            "/system/fonts/NotoSans-Regular.ttf",
            "/system/fonts/Roboto-Regular.ttf",
            "/system/fonts/RobotoCondensed-Regular.ttf",
            "/data/data/com.termux/files/usr/share/fonts/TTF/DejaVuSans.ttf",
        ];
        for path in CANDIDATES {
            if let Ok(bytes) = std::fs::read(path)
                && let Some(font) = Self::rasterize(&bytes, 30.0, FontSource::System)
            {
                if font.coverage().cyrillic_covered >= 60 {
                    return font;
                }
                if fallback.is_none() {
                    fallback = Some(font);
                }
            }
        }
        fallback.unwrap_or_else(Self::fallback)
    }

    /// Rasterise explicit TTF/OTF bytes (game font or a bundled face).
    pub fn load_bytes(bytes: &[u8]) -> Option<Self> {
        Self::rasterize(bytes, 30.0, FontSource::Game)
    }

    fn rasterize(bytes: &[u8], size: f32, source: FontSource) -> Option<Self> {
        use ab_glyph::{Font as _, FontArc, ScaleFont};

        let font = FontArc::try_from_vec(bytes.to_vec()).ok()?;
        let scaled = font.as_scaled(size);
        // Cyrillic needs roughly twice the glyphs of the old ASCII atlas.
        const ATLAS_WIDTH: u32 = 2048;
        const ATLAS_HEIGHT: u32 = 1024;
        let mut rgba = vec![0u8; (ATLAS_WIDTH * ATLAS_HEIGHT * 4) as usize];
        let mut glyphs = HashMap::new();
        let mut covered = Vec::new();
        let mut missing = Vec::new();
        let mut pen_x = 2i32;
        let mut pen_y = 2i32;
        let mut row_height = 0i32;
        for character in ui_charset() {
            let glyph_id = font.glyph_id(character);
            if glyph_id == ab_glyph::GlyphId(0) {
                missing.push(character);
                continue;
            }
            let glyph = glyph_id.with_scale(size);
            let advance = scaled.h_advance(glyph_id);
            let Some(outlined) = font.outline_glyph(glyph) else {
                // Whitespace (space, NBSP) has an advance but no outline; it
                // is a valid covered glyph, not a missing codepoint.
                glyphs.insert(
                    character,
                    Glyph {
                        uv: [0.0; 4],
                        width: 0.0,
                        height: 0.0,
                        offset: [0.0, 0.0],
                        advance,
                    },
                );
                covered.push(character);
                continue;
            };
            let bounds = outlined.px_bounds();
            let width = bounds.width().ceil().max(1.0) as i32;
            let height = bounds.height().ceil().max(1.0) as i32;
            if pen_x + width + 3 >= ATLAS_WIDTH as i32 {
                pen_x = 2;
                pen_y += row_height + 3;
                row_height = 0;
            }
            if pen_y + height + 2 >= ATLAS_HEIGHT as i32 {
                missing.push(character);
                continue;
            }
            outlined.draw(|x, y, coverage| {
                let px = pen_x + x as i32;
                let py = pen_y + y as i32;
                if px < 0 || py < 0 || px >= ATLAS_WIDTH as i32 || py >= ATLAS_HEIGHT as i32 {
                    return;
                }
                let alpha = (coverage.clamp(0.0, 1.0) * 255.0) as u8;
                let index = ((py as u32 * ATLAS_WIDTH + px as u32) * 4) as usize;
                rgba[index] = 255;
                rgba[index + 1] = 255;
                rgba[index + 2] = 255;
                rgba[index + 3] = alpha;
            });
            glyphs.insert(
                character,
                Glyph {
                    // Half-texel inset: with linear filtering, sampling the
                    // exact bitmap edge bleeds the neighbouring glyph into
                    // the quad (the "garbled text" artefact). Keep the
                    // sampling inside the glyph's own transparent border.
                    uv: [
                        (pen_x as f32 + 0.5) / ATLAS_WIDTH as f32,
                        (pen_y as f32 + 0.5) / ATLAS_HEIGHT as f32,
                        (pen_x + width) as f32 / ATLAS_WIDTH as f32 - 0.5 / ATLAS_WIDTH as f32,
                        (pen_y + height) as f32 / ATLAS_HEIGHT as f32 - 0.5 / ATLAS_HEIGHT as f32,
                    ],
                    width: width as f32,
                    height: height as f32,
                    offset: [bounds.min.x, bounds.min.y],
                    advance,
                },
            );
            covered.push(character);
            pen_x += width + 2;
            row_height = row_height.max(height);
        }
        let space_advance = glyphs
            .get(&' ')
            .map(|glyph| glyph.advance)
            .unwrap_or(size * 0.4);
        Some(Self {
            atlas: TextureImage {
                width: ATLAS_WIDTH,
                height: ATLAS_HEIGHT,
                rgba,
            },
            glyphs,
            line_height: size * 1.25,
            ascent: scaled.ascent(),
            space_advance,
            source,
            covered,
            missing,
            external_texture: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    fn fallback() -> Self {
        // 5x7 column bitmap for the characters the HUD uses.
        const GLYPHS: &[(char, [u8; 5])] = &[
            (' ', [0, 0, 0, 0, 0]),
            ('A', [0x7E, 0x11, 0x11, 0x11, 0x7E]),
            ('B', [0x7F, 0x49, 0x49, 0x49, 0x36]),
            ('C', [0x3E, 0x41, 0x41, 0x41, 0x22]),
            ('D', [0x7F, 0x41, 0x41, 0x22, 0x1C]),
            ('E', [0x7F, 0x49, 0x49, 0x49, 0x41]),
            ('F', [0x7F, 0x09, 0x09, 0x09, 0x01]),
            ('G', [0x3E, 0x41, 0x49, 0x49, 0x7A]),
            ('H', [0x7F, 0x08, 0x08, 0x08, 0x7F]),
            ('I', [0x00, 0x41, 0x7F, 0x41, 0x00]),
            ('J', [0x20, 0x40, 0x41, 0x3F, 0x01]),
            ('K', [0x7F, 0x08, 0x14, 0x22, 0x41]),
            ('L', [0x7F, 0x40, 0x40, 0x40, 0x40]),
            ('M', [0x7F, 0x02, 0x0C, 0x02, 0x7F]),
            ('N', [0x7F, 0x04, 0x08, 0x10, 0x7F]),
            ('O', [0x3E, 0x41, 0x41, 0x41, 0x3E]),
            ('P', [0x7F, 0x09, 0x09, 0x09, 0x06]),
            ('Q', [0x3E, 0x41, 0x51, 0x21, 0x5E]),
            ('R', [0x7F, 0x09, 0x19, 0x29, 0x46]),
            ('S', [0x46, 0x49, 0x49, 0x49, 0x31]),
            ('T', [0x01, 0x01, 0x7F, 0x01, 0x01]),
            ('U', [0x3F, 0x40, 0x40, 0x40, 0x3F]),
            ('V', [0x1F, 0x20, 0x40, 0x20, 0x1F]),
            ('W', [0x7F, 0x20, 0x18, 0x20, 0x7F]),
            ('X', [0x63, 0x14, 0x08, 0x14, 0x63]),
            ('Y', [0x07, 0x08, 0x70, 0x08, 0x07]),
            ('Z', [0x61, 0x51, 0x49, 0x45, 0x43]),
            ('0', [0x3E, 0x51, 0x49, 0x45, 0x3E]),
            ('1', [0x00, 0x42, 0x7F, 0x40, 0x00]),
            ('2', [0x42, 0x61, 0x51, 0x49, 0x46]),
            ('3', [0x21, 0x41, 0x45, 0x4B, 0x31]),
            ('4', [0x18, 0x14, 0x12, 0x7F, 0x10]),
            ('5', [0x27, 0x45, 0x45, 0x45, 0x39]),
            ('6', [0x3C, 0x4A, 0x49, 0x49, 0x30]),
            ('7', [0x01, 0x71, 0x09, 0x05, 0x03]),
            ('8', [0x36, 0x49, 0x49, 0x49, 0x36]),
            ('9', [0x06, 0x49, 0x49, 0x29, 0x1E]),
            ('.', [0x00, 0x60, 0x60, 0x00, 0x00]),
            (':', [0x00, 0x36, 0x36, 0x00, 0x00]),
            ('/', [0x20, 0x10, 0x08, 0x04, 0x02]),
            ('-', [0x08, 0x08, 0x08, 0x08, 0x08]),
            ('+', [0x08, 0x08, 0x3E, 0x08, 0x08]),
            ('(', [0x00, 0x1C, 0x22, 0x41, 0x00]),
            (')', [0x00, 0x41, 0x22, 0x1C, 0x00]),
            ('!', [0x00, 0x00, 0x5F, 0x00, 0x00]),
            ('?', [0x02, 0x01, 0x51, 0x09, 0x06]),
            ('%', [0x23, 0x13, 0x08, 0x64, 0x62]),
        ];
        let atlas_width = 512u32;
        let atlas_height = 256u32;
        let mut rgba = vec![0u8; (atlas_width * atlas_height * 4) as usize];
        let mut glyphs = HashMap::new();
        let cell_w = 14i32;
        let cell_h = 20i32;
        for (index, (character, columns)) in GLYPHS.iter().enumerate() {
            let col = index as i32 % 32;
            let row = index as i32 / 32;
            let x0 = col * cell_w + 2;
            let y0 = row * cell_h + 2;
            for (cx, column) in columns.iter().enumerate() {
                for cy in 0..7 {
                    if column & (1 << cy) == 0 {
                        continue;
                    }
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let px = x0 + cx as i32 * 2 + dx;
                            let py = y0 + cy * 2 + dy;
                            let index = ((py as u32 * atlas_width + px as u32) * 4) as usize;
                            rgba[index] = 255;
                            rgba[index + 1] = 255;
                            rgba[index + 2] = 255;
                            rgba[index + 3] = 255;
                        }
                    }
                }
            }
            glyphs.insert(
                *character,
                Glyph {
                    uv: [
                        x0 as f32 / atlas_width as f32,
                        y0 as f32 / atlas_height as f32,
                        (x0 + 10) as f32 / atlas_width as f32,
                        (y0 + 14) as f32 / atlas_height as f32,
                    ],
                    width: 10.0,
                    height: 14.0,
                    offset: [0.0, 0.0],
                    advance: 12.0,
                },
            );
        }
        let covered: Vec<char> = glyphs.keys().copied().collect();
        let missing: Vec<char> = ui_charset()
            .into_iter()
            .filter(|character| !glyphs.contains_key(character))
            .collect();
        Self {
            atlas: TextureImage {
                width: atlas_width,
                height: atlas_height,
                rgba,
            },
            glyphs,
            line_height: 18.0,
            ascent: 14.0,
            space_advance: 12.0,
            source: FontSource::Fallback,
            covered,
            missing,
            external_texture: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Glyph lookup with a Unicode uppercase fallback, used by FDF text
    /// (WC3 strings are mixed-case Cyrillic) and by the HUD (ASCII labels).
    fn glyph_for(&self, character: char) -> Option<&Glyph> {
        self.glyphs.get(&character).or_else(|| {
            let upper = character.to_uppercase().next()?;
            self.glyphs.get(&upper)
        })
    }

    pub fn has_glyph(&self, character: char) -> bool {
        self.glyph_for(character).is_some()
    }

    /// Advance width of one codepoint in atlas pixels (before `scale`).
    pub fn glyph_advance(&self, character: char) -> Option<f32> {
        self.glyph_for(character).map(|glyph| glyph.advance)
    }

    /// Codepoints missing from this atlas (never drawn as tofu).
    pub fn missing_chars(&self) -> &[char] {
        &self.missing
    }

    pub fn coverage(&self) -> GlyphCoverage {
        let ascii_covered = self
            .covered
            .iter()
            .filter(|character| (0x20..=0x7E).contains(&(**character as u32)))
            .count();
        let cyrillic_covered = self
            .covered
            .iter()
            .filter(|character| (0x400..=0x4FF).contains(&(**character as u32)))
            .count();
        GlyphCoverage {
            covered: self.covered.len(),
            missing: self.missing.len(),
            ascii_covered,
            cyrillic_covered,
            missing_chars: self.missing.clone(),
        }
    }

    /// Numeric labels are tabular when every digit has the same advance, so a
    /// resources/HP column never jitters as the value changes.
    pub fn digits_tabular(&self) -> bool {
        let mut expected: Option<f32> = None;
        for digit in '0'..='9' {
            let Some(glyph) = self.glyphs.get(&digit) else {
                return false;
            };
            match expected {
                None => expected = Some(glyph.advance),
                Some(value) if (glyph.advance - value).abs() <= 0.01 => {}
                Some(_) => return false,
            }
        }
        expected.is_some()
    }

    /// Advance widths of `0..9` (NaN where a glyph is absent), reported by the
    /// font audit so "are digits tabular?" is a measurement, not an opinion.
    pub fn digit_advances(&self) -> [f32; 10] {
        let mut out = [f32::NAN; 10];
        for (index, digit) in ('0'..='9').enumerate() {
            if let Some(glyph) = self.glyphs.get(&digit) {
                out[index] = glyph.advance;
            }
        }
        out
    }

    /// Draw one glyph at `(pen, baseline)` and return its advance, or `None`
    /// when the face has no bitmap for the codepoint. Used by the fallback
    /// chain to mix faces per codepoint.
    pub fn draw_glyph(
        &self,
        batch: &mut UiBatch,
        character: char,
        pen_x: f32,
        baseline: f32,
        scale: f32,
        color: [f32; 4],
    ) -> Option<f32> {
        let glyph = self.glyph_for(character)?;
        if glyph.width > 0.0 {
            let rect = Rect::new(
                pen_x + glyph.offset[0] * scale,
                baseline + glyph.offset[1] * scale,
                glyph.width * scale,
                glyph.height * scale,
            );
            self.glyph_quad(batch, rect, glyph.uv, color);
        }
        Some(glyph.advance * scale)
    }

    /// Rasterise the pack's numeric face (monospaced tabular digits) and
    /// report whether it is usable for HUD numbers.
    pub fn numeric_face_ok(&self) -> bool {
        self.digits_tabular() && self.coverage().cyrillic_covered >= 60
    }

    pub fn measure(&self, text: &str, scale: f32) -> f32 {
        text.chars()
            .map(|character| {
                self.glyph_for(character)
                    .map(|glyph| glyph.advance)
                    .unwrap_or(self.space_advance)
            })
            .sum::<f32>()
            * scale
    }

    /// Route glyph quads through `Assets::textures[texture]` (the pack's
    /// multi-face HUD) instead of the single font channel.
    pub fn use_texture(&self, texture: usize) {
        self.external_texture
            .store(texture + 1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn clear_texture(&self) {
        self.external_texture
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn external_texture(&self) -> Option<usize> {
        match self
            .external_texture
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            0 => None,
            slot => Some(slot - 1),
        }
    }

    fn glyph_quad(
        &self,
        batch: &mut UiBatch,
        rect: Rect,
        uv: [f32; 4],
        color: [f32; 4],
    ) {
        match self.external_texture() {
            Some(texture) => batch.textured_quad(texture, rect, uv, color),
            None => batch.quad(UiTexture::Font, rect, uv, color),
        }
    }

    pub fn text(
        &self,
        batch: &mut UiBatch,
        text: &str,
        x: f32,
        y: f32,
        scale: f32,
        color: [f32; 4],
    ) {
        let mut pen = x;
        let baseline = y + self.ascent * scale;
        for character in text.chars() {
            let character = character.to_ascii_uppercase();
            let Some(glyph) = self.glyph_for(character) else {
                pen += self.space_advance * scale;
                continue;
            };
            if glyph.width > 0.0 {
                let rect = Rect::new(
                    pen + glyph.offset[0] * scale,
                    baseline + glyph.offset[1] * scale,
                    glyph.width * scale,
                    glyph.height * scale,
                );
                self.glyph_quad(batch, rect, glyph.uv, color);
            }
            pen += glyph.advance * scale;
        }
    }

    /// Draw text exactly as authored (no ASCII uppercasing), resolving each
    /// codepoint through the atlas. Used by the FDF text shaper so Russian
    /// `GlobalStrings.fdf` values keep their case; missing codepoints are
    /// skipped with a space advance instead of drawing a tofu box.
    pub fn text_exact(
        &self,
        batch: &mut UiBatch,
        text: &str,
        x: f32,
        y: f32,
        scale: f32,
        color: [f32; 4],
    ) {
        let mut pen = x;
        let baseline = y + self.ascent * scale;
        for character in text.chars() {
            let Some(glyph) = self.glyph_for(character) else {
                pen += self.space_advance * scale;
                continue;
            };
            if glyph.width > 0.0 {
                let rect = Rect::new(
                    pen + glyph.offset[0] * scale,
                    baseline + glyph.offset[1] * scale,
                    glyph.width * scale,
                    glyph.height * scale,
                );
                self.glyph_quad(batch, rect, glyph.uv, color);
            }
            pen += glyph.advance * scale;
        }
    }

    pub fn text_centred(
        &self,
        batch: &mut UiBatch,
        text: &str,
        centre_x: f32,
        y: f32,
        scale: f32,
        color: [f32; 4],
    ) {
        let width = self.measure(text, scale);
        self.text(batch, text, centre_x - width * 0.5, y, scale, color);
    }

    /// Draw text sampling a glyph atlas that lives in `Assets::textures`
    /// (`UiBatch::textured`) instead of the single `UiBatch::font` channel.
    ///
    /// The UI pass binds exactly one font atlas; the pack ships three faces
    /// (serif/sans/mono), so their atlases are registered as external textures
    /// and glyphs go through the textured channel. Geometry is identical to
    /// [`Font::text_exact`].
    pub fn text_textured(
        &self,
        batch: &mut UiBatch,
        texture: usize,
        text: &str,
        x: f32,
        y: f32,
        scale: f32,
        color: [f32; 4],
    ) {
        let mut pen = x;
        let baseline = y + self.ascent * scale;
        for character in text.chars() {
            let Some(glyph) = self.glyph_for(character) else {
                pen += self.space_advance * scale;
                continue;
            };
            if glyph.width > 0.0 {
                let rect = Rect::new(
                    pen + glyph.offset[0] * scale,
                    baseline + glyph.offset[1] * scale,
                    glyph.width * scale,
                    glyph.height * scale,
                );
                batch.textured_quad(texture, rect, glyph.uv, color);
            }
            pen += glyph.advance * scale;
        }
        let _ = UiTexture::Font;
    }

    /// [`Font::text_box`] for an externally registered glyph atlas: same
    /// 24 px floor, truncation, alignment and shadow, different texture
    /// channel.
    pub fn text_box_textured(
        &self,
        batch: &mut UiBatch,
        texture: usize,
        text: &str,
        rect: Rect,
        scale: f32,
        color: [f32; 4],
        align: TextAlign,
        shadow: Option<[f32; 4]>,
    ) -> Rect {
        let scale = scale.max(MIN_TEXT_PX / 30.0);
        let text = sanitize_hud_text(text);
        let text = text.as_str();
        let line_h = self.line_height * scale;
        let y = rect.y + ((rect.h - line_h) * 0.5).max(0.0);
        let fitted = self.truncate(text, scale, rect.w);
        let width = self.measure(&fitted, scale);
        let x = match align {
            TextAlign::Left => rect.x,
            TextAlign::Centre => rect.x + (rect.w - width) * 0.5,
            TextAlign::Right => rect.right() - width,
        };
        if let Some(shadow_color) = shadow {
            let offset = (scale * 30.0 * 0.055).clamp(1.0, 2.0);
            self.text_textured(
                batch,
                texture,
                &fitted,
                x + offset,
                y + offset,
                scale,
                shadow_color,
            );
        }
        self.text_textured(batch, texture, &fitted, x, y, scale, color);
        Rect::new(x, y, width.max(0.0), line_h)
    }

    /// Truncate `text` with a three-dot suffix so its measured advance fits
    /// `max_w`. Text is never clipped by a scissor: it is shortened by the same
    /// font metrics that draw it.
    pub fn truncate(&self, text: &str, scale: f32, max_w: f32) -> String {
        if self.measure(text, scale) <= max_w {
            return text.to_string();
        }
        let suffix = "...";
        let suffix_w = self.measure(suffix, scale);
        if suffix_w >= max_w {
            return String::new();
        }
        let mut out = String::new();
        for character in text.chars() {
            let mut candidate = out.clone();
            candidate.push(character);
            if self.measure(&candidate, scale) + suffix_w > max_w {
                break;
            }
            out.push(character);
        }
        out.push_str(suffix);
        out
    }

    /// Greedy word wrap against the live glyph metrics.
    ///
    /// `text` is sanitised like every other HUD string, `\n` is a hard break,
    /// and a single word longer than `max_w` is split character by character so
    /// nothing ever draws outside the box. This is the only wrapper the HUD
    /// uses: the tooltip and toast bands must match their measured rect.
    pub fn wrap(&self, text: &str, scale: f32, max_w: f32) -> Vec<String> {
        let scale = scale.max(MIN_TEXT_PX / 30.0);
        let mut out = Vec::new();
        for paragraph in text.split('\n') {
            let paragraph = sanitize_hud_text(paragraph);
            let mut line = String::new();
            for word in paragraph.split_whitespace() {
                let candidate = if line.is_empty() {
                    word.to_string()
                } else {
                    format!("{line} {word}")
                };
                if self.measure(&candidate, scale) <= max_w {
                    line = candidate;
                    continue;
                }
                if !line.is_empty() {
                    out.push(std::mem::take(&mut line));
                }
                if self.measure(word, scale) <= max_w {
                    line = word.to_string();
                    continue;
                }
                // A single word wider than the box: break it by chars so the
                // text can never leave the rect.
                let mut chunk = String::new();
                for character in word.chars() {
                    let mut next = chunk.clone();
                    next.push(character);
                    if self.measure(&next, scale) > max_w && !chunk.is_empty() {
                        out.push(std::mem::take(&mut chunk));
                    }
                    chunk.push(character);
                }
                line = chunk;
            }
            out.push(line);
        }
        if out.is_empty() {
            out.push(String::new());
        }
        out.iter().map(|line| line.trim_end().to_string()).collect()
    }

    /// Line box height for `scale` (atlas line height, leading included).
    /// No floor here: the caller owns the element's minimum size, so a badge
    /// can use a 16 px box without the prose floor silently doubling it.
    pub fn line_box(&self, scale: f32) -> f32 {
        self.line_height * scale
    }

    /// Glyph box height (ascent) of one line — the part that is actually
    /// inked. Chips and badges are sized from this, not from the leading.
    pub fn glyph_box(&self, scale: f32) -> f32 {
        self.ascent * scale
    }

    /// Draw wrapped text inside `rect`, one line per wrapped row, clipped to
    /// the rect by construction. Returns the number of lines and their height.
    pub fn paragraph(
        &self,
        batch: &mut UiBatch,
        text: &str,
        rect: Rect,
        scale: f32,
        color: [f32; 4],
        align: TextAlign,
        shadow: Option<[f32; 4]>,
        line_spacing: f32,
    ) -> (usize, f32) {
        let lines = self.wrap(text, scale, rect.w);
        let line_h = self.line_box(scale) * line_spacing.max(1.0);
        let limit = (rect.h / line_h).floor().max(1.0) as usize;
        let drawn = lines.len().min(limit);
        for (index, line) in lines.iter().take(drawn).enumerate() {
            if line.is_empty() {
                continue;
            }
            let row = Rect::new(rect.x, rect.y + index as f32 * line_h, rect.w, line_h);
            self.text_box(batch, line, row, scale, color, align, shadow);
        }
        (drawn, drawn as f32 * line_h)
    }

    /// Draw text inside a rect with a 1-2 px shadow. Returns the actually used
    /// text rect so layout tests can assert it stays inside the parent.
    pub fn text_box(        &self,
        batch: &mut UiBatch,
        text: &str,
        rect: Rect,
        scale: f32,
        color: [f32; 4],
        align: TextAlign,
        shadow: Option<[f32; 4]>,
    ) -> Rect {
        // FDF/war3 sizes are fractions of screen height and land near 11 px
        // at 1080p; the console must stay readable at >=24 px.
        let scale = scale.max(MIN_TEXT_PX / 30.0);
        let text = sanitize_hud_text(text);
        let text = text.as_str();
        let line_h = self.line_height * scale;
        let y = rect.y + ((rect.h - line_h) * 0.5).max(0.0);
        let fitted = self.truncate(text, scale, rect.w);
        let width = self.measure(&fitted, scale);
        let x = match align {
            TextAlign::Left => rect.x,
            TextAlign::Centre => rect.x + (rect.w - width) * 0.5,
            TextAlign::Right => rect.right() - width,
        };
        if let Some(shadow_color) = shadow {
            let offset = (scale * 30.0 * 0.055).clamp(1.0, 2.0);
            self.text(batch, &fitted, x + offset, y + offset, scale, shadow_color);
        }
        self.text(batch, &fitted, x, y, scale, color);
        Rect::new(x, y, width.max(0.0), line_h)
    }

    /// One fit policy for every text run that has a hard rect: measure the run
    /// at the intended size, ellipsize until it fits, and never return a string
    /// wider than `rect.w`. `min_px` is the smallest readable pixel height for
    /// this element class (dense badges 16 px, prose 24 px) — a floor is a
    /// *choice per element*, never a global override of the layout.
    pub fn fit_text(&self, rect: Rect, text: &str, scale: f32, min_px: f32) -> (String, f32) {
        let scale = scale.max(min_px / 30.0);
        let clean = sanitize_hud_text(text);
        let fitted = self.truncate(&clean, scale, rect.w.max(1.0));
        let width = self.measure(&fitted, scale);
        debug_assert!(
            width <= rect.w + 1.0,
            "fit_text overflow: {fitted:?} {width:.1}px > rect {:?}",
            rect
        );
        debug_assert!(
            self.glyph_box(scale) <= rect.h + 1.0 || rect.h <= 0.0,
            "fit_text glyph box {:.1}px > rect height {}px for {fitted:?}",
            self.glyph_box(scale),
            rect.h
        );
        (fitted, scale)
    }

    /// Draw a fitted run inside `rect` (the single HUD text-fit path for
    /// badges, costs and captions). Returns the drawn (clipped) rect.
    pub fn draw_fit(
        &self,
        batch: &mut UiBatch,
        rect: Rect,
        text: &str,
        scale: f32,
        min_px: f32,
        color: [f32; 4],
        align: TextAlign,
        shadow: Option<[f32; 4]>,
    ) -> Rect {
        let (fitted, scale) = self.fit_text(rect, text, scale, min_px);
        if fitted.is_empty() {
            return Rect::new(rect.x, rect.y, 0.0, 0.0);
        }
        let line_h = self.glyph_box(scale);
        let width = self.measure(&fitted, scale);
        let x = match align {
            TextAlign::Left => rect.x,
            TextAlign::Centre => rect.x + (rect.w - width) * 0.5,
            TextAlign::Right => rect.right() - width,
        };
        // Centre the inked glyph box inside the chip, never above it: the chip
        // is sized from this box by its caller, so the glyphs stay inside.
        let y = rect.y + ((rect.h - line_h) * 0.5).max(0.0);
        if let Some(shadow_color) = shadow {
            let offset = (scale * 30.0 * 0.055).clamp(1.0, 2.0);
            self.text(batch, &fitted, x + offset, y + offset, scale, shadow_color);
        }
        self.text(batch, &fitted, x, y, scale, color);
        Rect::new(x, y, width, line_h)
    }
}

// ---------------------------------------------------------------------------
// Screen data contracts (ledger 469: map select, loading, quest log, score,
// allies/journal/game-menu dialogs) and the plate text-fit policy shared by
// every plate that hosts prose (tooltip, toast, dialog rows).
//
// The HUD owns the row shapes so `game.rs` can stay read-only in this lane:
// `Game::quest_rows()` / `Game::score_rows()` (lane 467) return these and the
// screens consume them through `QuestLog` / `ScoreData`.
// ---------------------------------------------------------------------------

/// One quest-log objective row.
///
/// `done` selects the gold completed mark (or the dark pending cell); `hint`
/// is the optional right-hand location/counter column and may be empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuestRow {
    pub text: String,
    pub done: bool,
    pub hint: String,
}

/// One score-screen line: a label and its formatted right-column value
/// (`UNITS TRAINED` / `12`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScoreRow {
    pub label: String,
    pub value: String,
}

/// Ordered quest rows plus the counts the log header prints. This is the
/// single adapter point for the campaign lane: `QuestLog::new(Game::quest_rows())`
/// once lane 467 lands, `from_pairs` until then.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuestLog {
    pub rows: Vec<QuestRow>,
}

impl QuestLog {
    pub fn new(rows: Vec<QuestRow>) -> Self {
        Self { rows }
    }

    /// Build rows from `(text, done)` pairs (the pre-467 campaign shim).
    pub fn from_pairs<I, S>(rows: I) -> Self
    where
        I: IntoIterator<Item = (S, bool)>,
        S: Into<String>,
    {
        Self {
            rows: rows
                .into_iter()
                .map(|(text, done)| QuestRow {
                    text: text.into(),
                    done,
                    hint: String::new(),
                })
                .collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn done_count(&self) -> usize {
        self.rows.iter().filter(|row| row.done).count()
    }

    /// Number of pages for a fixed rows-per-page budget (never zero: an empty
    /// log still draws one page with its empty-state line).
    pub fn page_count(&self, per_page: usize) -> usize {
        let per = per_page.max(1);
        self.rows.len().div_ceil(per).max(1)
    }

    /// Rows of one page, clamped to the last page.
    pub fn page(&self, page: usize, per_page: usize) -> &[QuestRow] {
        let per = per_page.max(1);
        let pages = self.page_count(per);
        let page = page.min(pages - 1);
        let start = (page * per).min(self.rows.len());
        let end = (start + per).min(self.rows.len());
        &self.rows[start..end]
    }
}

/// Score-screen block: title, subtitle and the ordered metric rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScoreData {
    pub title: String,
    pub subtitle: String,
    pub rows: Vec<ScoreRow>,
}

impl ScoreData {
    pub fn new(
        title: impl Into<String>,
        subtitle: impl Into<String>,
        rows: Vec<ScoreRow>,
    ) -> Self {
        Self {
            title: title.into(),
            subtitle: subtitle.into(),
            rows,
        }
    }

    /// Build rows from `(label, value)` pairs.
    pub fn from_pairs<I, S, V>(title: impl Into<String>, subtitle: impl Into<String>, rows: I) -> Self
    where
        I: IntoIterator<Item = (S, V)>,
        S: Into<String>,
        V: Into<String>,
    {
        Self {
            title: title.into(),
            subtitle: subtitle.into(),
            rows: rows
                .into_iter()
                .map(|(label, value)| ScoreRow {
                    label: label.into(),
                    value: value.into(),
                })
                .collect(),
        }
    }
}

/// A text run fitted into a plate: the wrapped lines and the metrics actually
/// used. `width` is the widest line at `scale` and never exceeds the fit width.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlateText {
    pub lines: Vec<String>,
    pub scale: f32,
    pub line_h: f32,
    pub width: f32,
}

impl PlateText {
    pub fn height(&self) -> f32 {
        self.lines.len() as f32 * self.line_h
    }

    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|line| line.trim().is_empty())
    }
}

/// Replace the tail of `line` with an ellipsis so it measures <= `max_w`.
fn ellipsize(font: &Font, line: &str, scale: f32, max_w: f32) -> String {
    if font.measure(line, scale) <= max_w {
        return line.to_string();
    }
    let mut trimmed: Vec<char> = line.chars().collect();
    while !trimmed.is_empty() {
        trimmed.pop();
        let candidate = format!("{}…", trimmed.iter().collect::<String>().trim_end());
        if font.measure(&candidate, scale) <= max_w {
            return candidate;
        }
    }
    String::new()
}

/// Fit a run into a plate's inner rect (the policy every prose plate uses).
///
/// The size is stepped down from `scale` to `min_px` (the readable floor) until
/// the wrapped run fits both the width and the line budget; wrapping is on word
/// boundaries through [`Font::wrap`], which also splits a single over-long word
/// so a line can never be wider than `inner.w`. When even the floor does not
/// fit `max_lines` (or the plate height), the last visible line gets an
/// ellipsis: the text is clipped, never allowed to leave the plate.
///
/// `inner` is the plate's text area **inside** its 9-slice insets, so callers
/// pass the rect the art leaves for text, not the outer plate.
pub fn fit_text_plate(
    font: &Font,
    text: &str,
    inner: Rect,
    scale: f32,
    min_px: f32,
    max_lines: usize,
) -> PlateText {
    let floor = (min_px.max(1.0) / 30.0).max(0.01);
    let start = if scale.is_finite() { scale.max(floor) } else { floor };
    let width = inner.w.max(1.0);
    let height = inner.h.max(1.0);
    let max_lines = max_lines.max(1);
    let mut candidate = start;
    loop {
        let lines = font.wrap(text, candidate, width);
        let line_h = font.line_box(candidate);
        let rows_by_height = (height / line_h.max(0.01)).floor().max(1.0) as usize;
        let allowed = max_lines.min(rows_by_height).max(1);
        if lines.len() <= allowed || candidate <= floor + 1e-4 {
            let mut out = lines;
            let clipped = out.len() > allowed;
            if clipped {
                out.truncate(allowed);
                if let Some(last) = out.last_mut() {
                    *last = ellipsize(font, last, candidate, width);
                }
            }
            // An empty final line from a trailing newline does not need a row.
            while out.len() > 1 && out.last().is_some_and(|line| line.is_empty()) {
                out.pop();
            }
            let widest = out
                .iter()
                .map(|line| font.measure(line, candidate))
                .fold(0.0f32, f32::max)
                .min(width);
            return PlateText {
                lines: out,
                scale: candidate,
                line_h,
                width: widest,
            };
        }
        candidate = (candidate - 0.04).max(floor);
    }
}

/// Draw a fitted run inside `inner` (the counterpart of [`fit_text_plate`]).
/// Every row is clamped to `inner` by construction; returns the block rect the
/// lines occupy.
pub fn draw_plate_text(
    batch: &mut UiBatch,
    font: &Font,
    text: &PlateText,
    inner: Rect,
    color: [f32; 4],
    align: TextAlign,
    shadow: Option<[f32; 4]>,
) -> Rect {
    let rows = (inner.h / text.line_h.max(0.01)).floor().max(0.0) as usize;
    let count = text.lines.len().min(rows);
    for (index, line) in text.lines.iter().take(count).enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row = Rect::new(
            inner.x,
            inner.y + index as f32 * text.line_h,
            inner.w,
            text.line_h,
        );
        font.text_box(batch, line, row, text.scale, color, align, shadow);
    }
    Rect::new(inner.x, inner.y, text.width.min(inner.w), count as f32 * text.line_h)
}

/// Strip WC3 colour/newline escape codes from HUD strings before atlas
/// rendering. Map names and localized labels can carry `|cAARRGGBB`/`|r`,
/// which would otherwise draw as literal pipe glyphs (the "garbled text"
/// symptom). The canonical decoder lives in `wc3-formats::fdf`.
/// Runtime faces for the HUD.
///
/// Primary is the operator's own Warcraft III UI font, `Fonts\NIM_____.ttf`
/// (internal name "Nimrod MT", md5 c8334ae9..., 95/95 Latin + 64/64 Cyrillic),
/// read from the installed archive at runtime - the TTF is never committed or
/// bundled. When the archive is absent the loader falls back to the pack's
/// `primary_serif_cyrillic.ttf` and logs `HUD_FONT fallback=pack
/// reason=archive-missing` once. The pack sans is kept as the third fallback
/// face; the pack mono is used for numeric columns only when Nimrod's digits
/// are not tabular.
pub struct HudFonts {
    pub primary: Font,
    pub serif: Font,
    pub sans: Font,
    pub numeric: Font,
    pub primary_source: String,
    pub paths: [String; 4],
    pub numeric_tabular_primary: bool,
}

impl std::fmt::Debug for HudFonts {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HudFonts")
            .field("primary_source", &self.primary_source)
            .field("paths", &self.paths)
            .field("numeric_tabular_primary", &self.numeric_tabular_primary)
            .finish_non_exhaustive()
    }
}

impl HudFonts {
    /// Manifest font files in the pack, as fallbacks.
    pub fn load_from_root(root: &std::path::Path) -> Option<Self> {
        let serif_path = root.join("fonts/primary_serif_cyrillic.ttf");
        let sans_path = root.join("fonts/actionbar_sans_cyrillic.ttf");
        let numeric_path = root.join("fonts/numeric_tabular.ttf");
        let serif = Font::load_bytes(&std::fs::read(&serif_path).ok()?)?;
        let sans = Font::load_bytes(&std::fs::read(&sans_path).ok()?)?;
        let numeric = Font::load_bytes(&std::fs::read(&numeric_path).ok()?)?;
        let (primary, primary_source) = match Font::load_nimrod_candidates() {
            Some((font, source)) => (font, source),
            None => {
                crate::dev_log!("HUD_FONT fallback=pack reason=archive-missing");
                (
                    Font::load_bytes(&std::fs::read(&serif_path).ok()?)?,
                    format!("pack:{}", serif_path.display()),
                )
            }
        };
        let numeric_tabular_primary = primary.digits_tabular();
        if !numeric_tabular_primary {
            crate::dev_log!(
                "HUD_FONT numeric=pack reason=not-tabular advances={:?}",
                primary.digit_advances()
            );
        }
        Some(Self {
            primary,
            serif,
            sans,
            numeric,
            paths: [
                serif_path.display().to_string(),
                sans_path.display().to_string(),
                numeric_path.display().to_string(),
                primary_source.clone(),
            ],
            primary_source,
            numeric_tabular_primary,
        })
    }

    pub fn load_from_pack(pack: &crate::hudskin::pack::HudPack) -> Option<Self> {
        Self::load_from_root(&pack.root)
    }

    /// Point each face at its registered `Assets::textures` index so one HUD
    /// batch can mix all faces.
    pub fn bind(&self, textures: [usize; 4]) {
        self.primary.use_texture(textures[0]);
        self.serif.use_texture(textures[1]);
        self.sans.use_texture(textures[2]);
        self.numeric.use_texture(textures[3]);
    }

    /// Nimrod -> pack serif -> pack sans, per codepoint.
    pub fn chain(&self) -> FontChain<'_> {
        FontChain::new(&[&self.primary, &self.serif, &self.sans])
    }

    /// Numeric columns: Nimrod when its digits are tabular, else the pack mono
    /// first with Nimrod as the non-digit fallback.
    pub fn number_chain(&self) -> FontChain<'_> {
        if self.numeric_tabular_primary {
            FontChain::new(&[&self.primary, &self.serif])
        } else {
            FontChain::new(&[&self.numeric, &self.primary, &self.serif])
        }
    }

    /// Device pixel height of a text run at `scale` with the global 24 px
    /// floor applied exactly like `Font::text_box`.
    pub fn text_px(scale: f32) -> f32 {
        30.0 * scale.max(MIN_TEXT_PX / 30.0)
    }

    /// Primary and fallback faces must carry the Russian StringLists glyphs;
    /// the numeric face must be tabular when it is used for numbers.
    pub fn verify(&self) -> Result<(), String> {
        for (name, font) in [
            ("primary", &self.primary),
            ("serif", &self.serif),
            ("sans", &self.sans),
            ("numeric", &self.numeric),
        ] {
            let coverage = font.coverage();
            if coverage.cyrillic_covered < 60 {
                return Err(format!(
                    "HUD_FONT {name} cyrillic_covered={} < 60",
                    coverage.cyrillic_covered
                ));
            }
            if coverage.ascii_covered < 90 {
                return Err(format!(
                    "HUD_FONT {name} ascii_covered={} < 90",
                    coverage.ascii_covered
                ));
            }
        }
        if !self.numeric.digits_tabular() {
            return Err("HUD_FONT numeric digits are not tabular".to_string());
        }
        Ok(())
    }
}

impl Font {
    /// Entries tried inside the operator's archive, in order. `NIM_____.ttf`
    /// is the real UI face; `FRIZQT__.TTF` is byte-identical in this build and
    /// is the historical name.
    pub const NIMROD_ENTRIES: [&'static str; 2] =
        ["Fonts\\NIM_____.ttf", "Fonts\\FRIZQT__.TTF"];

    /// Load the Warcraft III UI face from an `MpqArchive` at runtime.
    pub fn load_wc3_nimrod(archive: &std::path::Path) -> Result<Self, String> {
        let archive = wc3_mpq::MpqArchive::open(archive)
            .map_err(|error| format!("open {}: {error:?}", archive.display()))?;
        for entry in Self::NIMROD_ENTRIES {
            let Ok(bytes) = archive.read(entry) else {
                continue;
            };
            let Some(font) = Self::load_bytes(&bytes) else {
                return Err(format!("{entry}: rasterise failed"));
            };
            let coverage = font.coverage();
            if coverage.cyrillic_covered < 60 {
                return Err(format!(
                    "{entry}: cyrillic_covered={} < 60",
                    coverage.cyrillic_covered
                ));
            }
            return Ok(font);
        }
        Err("Fonts\\NIM_____.ttf not found in archive".to_string())
    }

    /// Installed-archive candidates, then the operator's extracted game dump
    /// (headless proofs only; the TTF is never bundled with the app).
    pub fn nimrod_candidates() -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();
        if let Some(path) = std::env::var_os("WC3RUST_WAR3_MPQ") {
            paths.push(std::path::PathBuf::from(path));
        }
        for path in [
            "/data/data/org.wc3rust.p0/files/War3.mpq",
            "/storage/emulated/0/Android/data/org.wc3rust.p0/files/War3.mpq",
            "/storage/emulated/0/Download/War3.mpq",
            "/storage/emulated/0/Download/Warcraft III/War3.mpq",
        ] {
            paths.push(std::path::PathBuf::from(path));
        }
        if let Some(repo) = option_env!("CARGO_MANIFEST_DIR") {
            paths.push(
                std::path::Path::new(repo)
                    .join("../..")
                    .join("analysis/war3_full3/Fonts/NIM_____.ttf"),
            );
        }
        paths
    }

    /// Load Nimrod from the first usable candidate; returns `(font, source)`.
    pub fn load_nimrod_candidates() -> Option<(Self, String)> {
        for path in Self::nimrod_candidates() {
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| {
                ext.eq_ignore_ascii_case("ttf") || ext.eq_ignore_ascii_case("otf")
            }) {
                let Ok(bytes) = std::fs::read(&path) else {
                    continue;
                };
                if let Some(font) = Self::load_bytes(&bytes)
                    && font.coverage().cyrillic_covered >= 60
                {
                    return Some((font, format!("extracted:{}", path.display())));
                }
                continue;
            }
            match Self::load_wc3_nimrod(&path) {
                Ok(font) => return Some((font, format!("archive:{}", path.display()))),
                Err(_) => continue,
            }
        }
        None
    }
}

/// Per-codepoint fallback chain: the first face with the glyph draws it, so a
/// missing Nimrod codepoint comes from the pack serif/sans instead of a tofu
/// box. Missing everywhere logs `HUD_GLYPH missing=<hex>` once per codepoint.
pub struct FontChain<'a> {
    faces: Vec<&'a Font>,
}

impl<'a> FontChain<'a> {
    pub fn new(faces: &[&'a Font]) -> Self {
        Self {
            faces: faces.to_vec(),
        }
    }

    pub fn has_glyph(&self, character: char) -> bool {
        self.faces.iter().any(|font| font.has_glyph(character))
    }

    fn face(&self, character: char) -> Option<(usize, &'a Font)> {
        self.faces
            .iter()
            .enumerate()
            .find(|(_, font)| font.has_glyph(character))
            .map(|(index, font)| (index, *font))
    }

    pub fn measure(&self, text: &str, scale: f32) -> f32 {
        text.chars()
            .map(|character| match self.face(character) {
                Some((_, font)) => font
                    .glyph_advance(character)
                    .unwrap_or(font.space_advance)
                    * scale,
                None => self
                    .faces
                    .first()
                    .map(|font| font.space_advance * scale)
                    .unwrap_or(0.0),
            })
            .sum()
    }

    pub fn truncate(&self, text: &str, scale: f32, max_w: f32) -> String {
        if self.measure(text, scale) <= max_w {
            return text.to_string();
        }
        let suffix = "...";
        let suffix_w = self.measure(suffix, scale);
        let mut out = String::new();
        for character in text.chars() {
            let mut probe = out.clone();
            probe.push(character);
            if self.measure(&probe, scale) + suffix_w > max_w {
                break;
            }
            out = probe;
        }
        out.push_str(suffix);
        out
    }

    pub fn text_box(
        &self,
        batch: &mut UiBatch,
        text: &str,
        rect: Rect,
        scale: f32,
        color: [f32; 4],
        align: TextAlign,
        shadow: Option<[f32; 4]>,
    ) -> Rect {
        let scale = scale.max(MIN_TEXT_PX / 30.0);
        let text = sanitize_hud_text(text);
        let text = text.as_str();
        let line_h = self
            .faces
            .first()
            .map(|font| font.line_height * scale)
            .unwrap_or(scale * 30.0);
        let y = rect.y + ((rect.h - line_h) * 0.5).max(0.0);
        let fitted = self.truncate(text, scale, rect.w);
        let width = self.measure(&fitted, scale);
        let x = match align {
            TextAlign::Left => rect.x,
            TextAlign::Centre => rect.x + (rect.w - width) * 0.5,
            TextAlign::Right => rect.right() - width,
        };
        if let Some(shadow_color) = shadow {
            let offset = (scale * 30.0 * 0.055).clamp(1.0, 2.0);
            self.draw(batch, &fitted, x + offset, y + offset, scale, shadow_color);
        }
        self.draw(batch, &fitted, x, y, scale, color);
        Rect::new(x, y, width.max(0.0), line_h)
    }

    /// Wrapped prose with the same per-codepoint fallback.
    pub fn paragraph(
        &self,
        batch: &mut UiBatch,
        text: &str,
        rect: Rect,
        scale: f32,
        color: [f32; 4],
        align: TextAlign,
        shadow: Option<[f32; 4]>,
        line_spacing: f32,
    ) -> (usize, f32) {
        let scale = scale.max(MIN_TEXT_PX / 30.0);
        let clean = sanitize_hud_text(text);
        let mut lines = Vec::new();
        let mut current = String::new();
        for word in clean.split_whitespace() {
            let mut probe = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if self.measure(&probe, scale) > rect.w && !current.is_empty() {
                lines.push(std::mem::take(&mut current));
                probe = word.to_string();
            }
            current = probe;
        }
        if !current.is_empty() {
            lines.push(current);
        }
        let line_h = self
            .faces
            .first()
            .map(|font| font.line_height * scale)
            .unwrap_or(scale * 30.0)
            * line_spacing.max(1.0);
        let limit = (rect.h / line_h).floor().max(1.0) as usize;
        let drawn = lines.len().min(limit);
        for (index, line) in lines.iter().take(drawn).enumerate() {
            self.text_box(
                batch,
                line,
                Rect::new(rect.x, rect.y + index as f32 * line_h, rect.w, line_h),
                scale,
                color,
                align,
                shadow,
            );
        }
        (drawn, drawn as f32 * line_h)
    }

    fn draw(
        &self,
        batch: &mut UiBatch,
        text: &str,
        x: f32,
        y: f32,
        scale: f32,
        color: [f32; 4],
    ) {
        let primary = self.faces.first();
        let baseline = y + primary.map(|font| font.ascent * scale).unwrap_or(0.0);
        let mut pen = x;
        for character in text.chars() {
            match self.face(character) {
                Some((_, font)) => {
                    if let Some(advance) = font.draw_glyph(batch, character, pen, baseline, scale, color) {
                        pen += advance;
                    } else {
                        pen += font.space_advance * scale;
                    }
                }
                None => {
                    log_missing_glyph(character);
                    pen += primary.map(|font| font.space_advance).unwrap_or(0.0) * scale;
                }
            }
        }
    }
}

fn log_missing_glyph(character: char) {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    static LOGGED: OnceLock<Mutex<HashSet<u32>>> = OnceLock::new();
    let logged = LOGGED.get_or_init(|| Mutex::new(HashSet::new()));
    let Ok(mut logged) = logged.lock() else {
        return;
    };
    if logged.insert(character as u32) {
        crate::dev_log!("HUD_GLYPH missing={:04X}", character as u32);
    }
}

/// Process-wide HUD face set, rasterised once per process from `root` and the
/// operator's installed archive.
pub fn pack_fonts(root: &std::path::Path) -> Option<&'static HudFonts> {
    static FONTS: std::sync::OnceLock<Option<HudFonts>> = std::sync::OnceLock::new();
    FONTS
        .get_or_init(|| {
            let fonts = HudFonts::load_from_root(root)?;
            if let Err(error) = fonts.verify() {
                crate::dev_log!("{error}");
                return None;
            }
            crate::dev_log!(
                "HUD_FONT primary={} serif={} sans={} numeric={} digits_tabular={} min_px={}",
                fonts.primary_source,
                fonts.paths[0],
                fonts.paths[1],
                fonts.paths[2],
                fonts.numeric_tabular_primary,
                MIN_TEXT_PX
            );
            Some(fonts)
        })
        .as_ref()
}

pub fn sanitize_hud_text(text: &str) -> String {
    wc3_formats::fdf::strip_escape_codes(text).replace(['\n', '\r'], " ")
}

/// Split a WC3 string into coloured runs: `|cAARRGGBB` starts a colour, `|r`
/// resets to `base`, every other escape is dropped. Used by the tooltip so the
/// shipped gold cost/requirement section keeps its authored colour.
pub fn color_runs(text: &str, base: [f32; 4]) -> Vec<(String, [f32; 4])> {
    let mut runs: Vec<(String, [f32; 4])> = Vec::new();
    let mut current = String::new();
    let mut color = base;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '|' {
            current.push(character);
            continue;
        }
        match chars.peek().copied() {
            Some('c') => {
                chars.next();
                let mut hex = String::new();
                for _ in 0..8 {
                    match chars.next() {
                        Some(digit) => hex.push(digit),
                        None => break,
                    }
                }
                if let Ok(value) = u32::from_str_radix(&hex, 16) {
                    if !current.is_empty() {
                        runs.push((std::mem::take(&mut current), color));
                    }
                    color = [
                        ((value >> 16) & 0xff) as f32 / 255.0,
                        ((value >> 8) & 0xff) as f32 / 255.0,
                        (value & 0xff) as f32 / 255.0,
                        base[3],
                    ];
                }
            }
            Some('r') | Some('R') => {
                chars.next();
                if !current.is_empty() {
                    runs.push((std::mem::take(&mut current), color));
                }
                color = base;
            }
            Some('n') => {
                chars.next();
                current.push(' ');
            }
            _ => {}
        }
    }
    if !current.is_empty() {
        runs.push((current, color));
    }
    runs
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextAlign {
    Left,
    Centre,
    Right,
}

// The reusable modern mobile HUD lives in its own file but is declared here so
// game.rs keeps importing `hud::{...}` without touching lib.rs (read-only in
// this workstream).
#[path = "mobile_hud.rs"]
pub mod mobile_hud;

// Data-driven UI mechanics (mechanic ids `ui.command-card`, `ui.tooltips`,
// `ui.minimap`, `ui.menu`, `economy.*`). Declared here so `cargo test -p
// wc3-game --lib hud::` runs their mechanic tests with the HUD suite.
#[path = "ui/card.rs"]
pub mod card;
#[path = "ui/menu.rs"]
pub mod menu;
#[path = "ui/minimap.rs"]
pub mod minimap;
#[path = "ui/resources.rs"]
pub mod resources;
#[path = "ui/tooltips.rs"]
pub mod tooltips;
// 48 dp army/inventory HUD lanes (`COMPOSITION.md` §1): the army panel owns the
// paginated chip row and the four top-banner tabs.
#[path = "hud/army_panel.rs"]
pub mod army_panel;
// Hero inventory (2x3 slots of 48 dp, drag/swap contract): declared here so
// `hud::inventory` is compiled and its tests run with the HUD suite.
#[path = "hud/inventory.rs"]
pub mod inventory;
// ---------------------------------------------------------------------------
// Data-driven UI builders used by the game layer (mechanic ids below).
// game.rs is read-only in this workstream: the exact caller patches live in
// analysis/agent-hud/patches/ui-data-driven-game.patch.
// ---------------------------------------------------------------------------

/// Command-card model for the current selection (`ui.command-card`).
pub fn card_model(game: &crate::game::Game) -> card::CardModel {
    // Shipped menu rows first: `hotkey_label()`/`cost_label()` are the card's
    // letters and prices. Fixed commands fill the remaining slots when the
    // selection has no typed menu (STOP/ARMY/BASE/GATHER/...).
    let menu: Vec<wc3_data::MenuEntry> = game
        .menu_actions()
        .into_iter()
        .map(|(entry, _)| entry)
        .collect();
    if !menu.is_empty() {
        return card::from_menu(&menu, card::CardPage::Root);
    }
    let buttons = build_buttons(game);
    let rows: Vec<(String, String, String, bool)> = buttons
        .iter()
        .filter(|button| !button.label.is_empty())
        .map(|button| {
            (
                button.label.clone(),
                if button.hotkey.is_empty() {
                    button.sub.clone()
                } else {
                    button.hotkey.clone()
                },
                button.sub.clone(),
                button.enabled,
            )
        })
        .collect();
    card::from_labeled(&rows, card::CardPage::Root)
}

/// Resource shelf snapshot (`economy.upkeep`, `economy.supply`, idle/alert).
pub fn resource_view(game: &crate::game::Game) -> resources::ResourceView {
    resources::resource_view(&game.sim, game.player)
}

/// Minimap blips for the current sim state (`ui.minimap`).
pub fn minimap_blips(game: &crate::game::Game) -> Vec<minimap::Blip> {
    minimap::blips(&game.sim, game.player, game.assets.world_size)
}

/// Minimap fog mask sampled from the canonical visibility grid (`fog.visibility`).
pub fn minimap_fog(game: &crate::game::Game) -> minimap::Fog {
    minimap::fog(&game.sim, game.player)
}

/// Tooltip for a selected unit (`ui.tooltips`), sim-table backed.
pub fn unit_tooltip(
    game: &crate::game::Game,
    id: wc3_sim::UnitId,
) -> Option<tooltips::TooltipData> {
    let unit = game.sim.unit(id)?;
    Some(tooltips::unit_from_def(unit.def()))
}

/// Single-player menu state seeded with the maps the host discovered
/// (`ui.menu`; save/load placeholder reports the missing `Sim` save API).
pub fn menu_state(maps: Vec<menu::MapChoice>) -> menu::MenuState {
    menu::MenuState::new(maps)
}


/// Compatibility view of the console rects consumed by input. The geometry is
/// authored by [`mobile_hud::MobileHudLayout`]; this struct only carries the
/// fields the touch layer needs and copies them after every layout rebuild.
#[derive(Clone, Debug, Default)]
pub struct HudLayout {
    pub scale: f32,
    /// The whole console, used as the top-level HUD blocker.
    pub top_bar: Rect,
    pub resource_row: Rect,
    pub minimap: Rect,
    /// The minimap texture square inside [`Self::minimap`] (the hit rect also
    /// covers the ornament border, so world mapping must use this rect).
    pub minimap_image: Rect,
    pub selection: Rect,
    /// Command-card plate (12 slots inside).
    pub command: Rect,
    pub button: f32,
}

impl HudLayout {
    pub fn new(viewport: glam::Vec2) -> Self {
        Self {
            scale: (viewport.x.min(viewport.y) / 1080.0).clamp(0.62, 2.0),
            ..Self::default()
        }
    }
}

#[derive(Clone)]
pub struct Button {
    pub rect: Rect,
    pub label: String,
    /// Compact card glyph (`FOOT`, `F`); empty means "derive from the label".
    pub short: String,
    /// Hotkey letter advertised on the card (`F`).
    pub hotkey: String,
    pub sub: String,
    pub action: crate::game::UiAction,
    pub enabled: bool,
}

impl Button {
    /// Empty WC3 card slot: present in the 12-slot vector, drawn as the empty
    /// tile, never dispatched (disabled).
    pub fn empty() -> Self {
        Self {
            rect: Rect::default(),
            label: String::new(),
            short: String::new(),
            hotkey: String::new(),
            sub: String::new(),
            action: crate::game::UiAction::Stop,
            enabled: false,
        }
    }
}

/// Hotkey for a card entry: the shipped profile value when present, otherwise
/// the first letter of the label (deterministic, non-empty for every entry).
fn card_hotkey(label: &str, data: Option<&str>) -> String {
    if let Some(data) = data {
        let first = data.split(',').next().unwrap_or(data).trim();
        if !first.is_empty() {
            return first.to_ascii_uppercase();
        }
    }
    label
        .chars()
        .find(|character| character.is_ascii_alphabetic())
        .map(|character| character.to_ascii_uppercase().to_string())
        .unwrap_or_default()
}

/// Label for a command-card tile. Uses the shipped short name only when it is
/// a real name (letters, no digits/dots, not a truncated prefix of the label);
/// otherwise the full label. Never returns a 3-char code like `KA3`/`GATH`.
pub fn card_label(short: &str, label: &str) -> String {
    let label = label.trim();
    let short = short.trim();
    let short_is_name = !short.is_empty()
        && short.chars().any(|character| character.is_alphabetic())
        && !short
            .chars()
            .any(|character| character.is_ascii_digit() || character == '.')
        && !(short.chars().count() <= 3 && label.chars().count() > short.chars().count());
    let starts_with_short = !short.is_empty()
        && label
            .to_uppercase()
            .starts_with(&short.to_uppercase());
    let text = if short_is_name && (label.is_empty() || !starts_with_short) {
        short
    } else if !label.is_empty() {
        label
    } else {
        short
    };
    text.to_uppercase()
}

/// Worker build page entries: shipped `Builds=` menu when the install provides
/// it, else the SLK roster build menu. Each row is (label, cost, hotkey,
/// action, enabled).
fn worker_build_entries(
    game: &crate::game::Game,
) -> Vec<(String, String, String, crate::game::UiAction, bool)> {
    let mut rows: Vec<(String, String, String, crate::game::UiAction, bool)> = game
        .menu_actions()
        .into_iter()
        .filter(|(_, action)| matches!(action, crate::game::UiAction::BuildDef(_)))
        .map(|(entry, action)| {
            (
                entry.name.to_uppercase(),
                entry.cost_label(),
                card_hotkey(&entry.name, entry.hotkey.as_deref()),
                action,
                game.can_afford_menu(&entry),
            )
        })
        .collect();
    if rows.is_empty() {
        rows = crate::sim_bridge::build_menu(&game.sim, game.player)
            .into_iter()
            .map(|option| {
                let hotkey = card_hotkey(option.label, None);
                (
                    option.label.to_uppercase(),
                    format!("{}G", option.cost),
                    hotkey,
                    crate::game::UiAction::BuildDef(option.def_id),
                    option.enabled,
                )
            })
            .collect();
    }
    rows
}

/// Producer training entries: shipped `Trains=` menu first, else the roster
/// `trains` list of the selected building.
fn train_entries(
    game: &crate::game::Game,
) -> Vec<(String, String, String, crate::game::UiAction, bool)> {
    let mut rows: Vec<(String, String, String, crate::game::UiAction, bool)> = game
        .menu_actions()
        .into_iter()
        .filter(|(_, action)| {
            matches!(
                action,
                crate::game::UiAction::TrainDef(_) | crate::game::UiAction::Train(_)
            )
        })
        .map(|(entry, action)| {
            (
                entry.name.to_uppercase(),
                entry.cost_label(),
                card_hotkey(&entry.name, entry.hotkey.as_deref()),
                action,
                game.can_afford_menu(&entry),
            )
        })
        .collect();
    if rows.is_empty() {
        let trains: Vec<&'static str> = game
            .selection
            .iter()
            .filter_map(|id| game.sim.unit(*id))
            .filter(|unit| unit.is_building())
            .flat_map(|unit| unit.def().trains.iter().copied())
            .collect();
        for def_id in trains {
            let Some(def) = wc3_sim::def_of(def_id) else {
                continue;
            };
            rows.push((
                def.name.to_uppercase(),
                format!("{}G", def.stats.cost),
                card_hotkey(def.name, None),
                crate::game::UiAction::TrainDef(def.id),
                game.can_afford_def(def.id),
            ));
        }
    }
    rows
}

/// Full 12-slot WC3 command card for the current selection.
///
/// Slot indices are exact (row-major); empty slots carry [`Button::empty`] so a
/// present button can never be pushed out of its authored position. The legacy
/// 8 visible slots keep their muscle memory; the appended slots carry MORE,
/// extra abilities/queue entries and camera controls.
pub fn build_buttons(game: &crate::game::Game) -> Vec<Button> {
    let mut slots: [Option<Button>; 12] = std::array::from_fn(|_| None);
    let mut set = |index: usize,
                   label: &str,
                   sub: &str,
                   hotkey: &str,
                   action: crate::game::UiAction,
                   enabled: bool| {
        if index < slots.len() {
            slots[index] = Some(Button {
                rect: Rect::default(),
                label: label.to_string(),
                short: String::new(),
                hotkey: hotkey.to_string(),
                sub: sub.to_string(),
                action,
                enabled,
            });
        }
    };
    let has_army = game.selected_army().is_some();
    let has_hall = game.selected_hall().is_some();
    let has_worker = game.selected_worker().is_some();
    let empty_selection = game.selection.is_empty();
    let hero = game
        .selection
        .iter()
        .filter_map(|id| game.sim.unit(*id))
        .find(|unit| unit.def().hero);
    let building = game
        .selection
        .iter()
        .filter_map(|id| game.sim.unit(*id))
        .find(|unit| unit.is_building());
    // Any producer building (barracks/workshop/altar) uses the training card,
    // not only the main hall: `UnitKind::TownHall` is the render bucket for
    // every building in this port.
    let producer = game
        .selection
        .iter()
        .filter_map(|id| game.sim.unit(*id))
        .find(|unit| unit.is_building() && !unit.def().trains.is_empty());

    // ---------------------------------------------------------- empty selection
    if empty_selection {
        set(4, "ARMY", "A", "A", crate::game::UiAction::SelectArmy, true);
        set(5, "BASE", "B", "B", crate::game::UiAction::CentreBase, true);
        set(8, "ZOOM-", "-", "-", crate::game::UiAction::ZoomOut, true);
        set(9, "ZOOM+", "+", "+", crate::game::UiAction::ZoomIn, true);
        set(10, "QUESTS", "F9", "", crate::game::UiAction::QuestLog, true);
        return slots
            .into_iter()
            .map(|slot| slot.unwrap_or_else(Button::empty))
            .collect();
    }

    // ----------------------------------------------------------- shop card
    // A selected shop/tavern sells items (`Sellitems=` stock) and, for a
    // tavern, hireable units. Without these rows the shop verbs had no touch
    // surface at all (`UI_MISSING buy_item reason=no-shop-card`).
    if let Some(shop) = game.selected_shop() {
        let mut slot = 0usize;
        let gold = game.sim.resources[game.player as usize];
        if let Some(state) = game.sim.shop(shop) {
            for entry in &state.slots {
                if slot > 8 {
                    break;
                }
                let Some(def) = wc3_sim::data::item_def(entry.item) else {
                    continue;
                };
                set(
                    slot,
                    &def.name.to_uppercase(),
                    &format!("{}G", def.cost.max(0)),
                    &card_hotkey(def.name, None),
                    crate::game::UiAction::BuyItem(def.id),
                    gold >= def.cost && entry.stock > 0,
                );
                slot += 1;
            }
        }
        for id in game.sim.shop_units(shop).iter().copied() {
            if slot > 9 {
                break;
            }
            let Some(def) = wc3_sim::def_of(id) else {
                continue;
            };
            set(
                slot,
                &def.name.to_uppercase(),
                &format!("{}G", def.stats.cost.max(0)),
                &card_hotkey(def.name, None),
                crate::game::UiAction::BuyUnit(def.id),
                gold >= def.stats.cost,
            );
            slot += 1;
        }
        set(10, "BASE", "B", "B", crate::game::UiAction::CentreBase, true);
        set(11, "STOP", "S", "S", crate::game::UiAction::Stop, true);
        return slots
            .into_iter()
            .map(|slot| slot.unwrap_or_else(Button::empty))
            .collect();
    }

    // -------------------------------------------------- construction site card
    // A building under construction offers exactly one action: cancel (the
    // refund path already exists in the game layer).
    let site = game.selection.iter().copied().find(|id| {
        game.sim
            .unit(*id)
            .map(|unit| unit.alive && unit.owner == game.player && unit.construct.is_some())
            .unwrap_or(false)
    });
    if site.is_some() {
        set(
            0,
            "CANCEL",
            "C",
            "C",
            crate::game::UiAction::CancelConstruction,
            true,
        );
        set(10, "BASE", "B", "B", crate::game::UiAction::CentreBase, true);
        set(11, "STOP", "S", "S", crate::game::UiAction::Stop, true);
        return slots
            .into_iter()
            .map(|slot| slot.unwrap_or_else(Button::empty))
            .collect();
    }

    // ------------------------------------------------------------- worker card
    // WC3 puts the worker's build list on the main card: nine slots hold the
    // whole Orc list (`opeo` builds 9) and the generic orders share the last
    // row. The old layout showed three builds per page plus a legacy kind-based
    // BUILD button that placed a Human town hall for every race.
    if has_worker && !has_hall {
        let rows = worker_build_entries(game);
        let pages = rows.len().div_ceil(9).max(1);
        let page = game.build_page.min(pages - 1);
        for slot in 0..9usize {
            if let Some((label, cost, hotkey, action, enabled)) = rows.get(page * 9 + slot) {
                set(slot, label, cost, hotkey, *action, *enabled);
            }
        }
        set(9, "GATHER", "G", "G", crate::game::UiAction::Gather, true);
        set(10, "STOP", "S", "S", crate::game::UiAction::Stop, true);
        if pages > 1 {
            set(
                11,
                "MORE",
                &format!("{}/{}", page + 1, pages),
                "",
                crate::game::UiAction::BuildPageNext,
                true,
            );
        } else {
            set(11, "ZOOM+", "+", "+", crate::game::UiAction::ZoomIn, true);
        }
        return slots
            .into_iter()
            .map(|slot| slot.unwrap_or_else(Button::empty))
            .collect();
    }

    // --------------------------------------------------------------- hall card
    if has_hall || producer.is_some() {
        let rows = train_entries(game);
        for (index, (label, cost, hotkey, action, enabled)) in rows.iter().take(6).enumerate() {
            set(index, label, cost, hotkey, *action, *enabled);
        }
        // Shipped `Researches=` rows. They used to live only in the game layer
        // (`UI_MISSING research_upgrade reason=no-card-row`), so a Blacksmith or
        // Lumber Mill had no way to start a research.
        let mut next = rows.len().min(6);
        if let Some(building) = game.selection.iter().copied().find(|id| {
            game.sim
                .unit(*id)
                .map(|unit| unit.alive && unit.owner == game.player && unit.is_building())
                .unwrap_or(false)
        }) {
            let gold = game.sim.resources[game.player as usize];
            let researches = game
                .sim
                .unit(building)
                .map(|unit| unit.def().researches)
                .unwrap_or(&[]);
            for id in researches.iter().copied() {
                if next >= 9 {
                    break;
                }
                let Some(upgrade) = wc3_sim::data::upgrade_def(id) else {
                    continue;
                };
                let level = game.sim.upgrade_level(game.player, id);
                if level >= upgrade.max_level {
                    continue;
                }
                let cost = upgrade.gold_cost(level + 1);
                set(
                    next,
                    &upgrade.name.to_uppercase(),
                    &format!("{cost}G"),
                    &card_hotkey(upgrade.name, None),
                    crate::game::UiAction::Research(upgrade.id),
                    gold >= cost,
                );
                next += 1;
            }
        }
        if next > 6 {
            set(9, "ZOOM+", "+", "+", crate::game::UiAction::ZoomIn, true);
            set(10, "BASE", "B", "B", crate::game::UiAction::CentreBase, true);
            set(11, "STOP", "S", "S", crate::game::UiAction::Stop, true);
        } else {
            set(6, "ARMY", "A", "A", crate::game::UiAction::SelectArmy, true);
            set(7, "STOP", "S", "S", crate::game::UiAction::Stop, true);
            set(8, "BASE", "B", "B", crate::game::UiAction::CentreBase, true);
            set(9, "ZOOM+", "+", "+", crate::game::UiAction::ZoomIn, true);
            set(10, "QUESTS", "F9", "", crate::game::UiAction::QuestLog, true);
        }
        return slots
            .into_iter()
            .map(|slot| slot.unwrap_or_else(Button::empty))
            .collect();
    }

    // ------------------------------------------------------- research-only card
    // A building that trains nothing but researches (Blacksmith, Lumber Mill,
    // Hunter's Hall, …) used to fall through to the empty unit card.
    let research_building = game.selection.iter().copied().find(|id| {
        game.sim
            .unit(*id)
            .map(|unit| {
                unit.alive
                    && unit.owner == game.player
                    && unit.is_building()
                    && !unit.def().researches.is_empty()
            })
            .unwrap_or(false)
    });
    if let Some(building) = research_building {
        let gold = game.sim.resources[game.player as usize];
        let mut slot = 0usize;
        let researches = game
            .sim
            .unit(building)
            .map(|unit| unit.def().researches)
            .unwrap_or(&[]);
        for id in researches.iter().copied() {
            if slot >= 9 {
                break;
            }
            let Some(upgrade) = wc3_sim::data::upgrade_def(id) else {
                continue;
            };
            let level = game.sim.upgrade_level(game.player, id);
            if level >= upgrade.max_level {
                continue;
            }
            let cost = upgrade.gold_cost(level + 1);
            set(
                slot,
                &upgrade.name.to_uppercase(),
                &format!("{cost}G"),
                &card_hotkey(upgrade.name, None),
                crate::game::UiAction::Research(upgrade.id),
                gold >= cost,
            );
            slot += 1;
        }
        set(9, "ZOOM+", "+", "+", crate::game::UiAction::ZoomIn, true);
        set(10, "BASE", "B", "B", crate::game::UiAction::CentreBase, true);
        set(11, "STOP", "S", "S", crate::game::UiAction::Stop, true);
        return slots
            .into_iter()
            .map(|slot| slot.unwrap_or_else(Button::empty))
            .collect();
    }

    // ------------------------------------------------------ unit / hero card
    // WC3 hero inventory: slots 0..5 of the console are the six carried items
    // when the selected unit has any; abilities keep 6/7/10/11.
    let inventory: Vec<(u8, String, i32, bool)> = game
        .selection
        .iter()
        .filter_map(|id| game.sim.unit(*id))
        .find_map(|unit| {
            if unit.items.is_empty() {
                return None;
            }
            Some(
                unit.items
                    .iter()
                    .enumerate()
                    .filter_map(|(index, id)| {
                        game.sim.item(*id).map(|item| {
                            (
                                index as u8,
                                item.def().name.to_string(),
                                item.charges,
                                item.def().usable,
                            )
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default();
    for (index, name, charges, usable) in &inventory {
        let sub = if *charges > 1 {
            format!("x{charges}")
        } else {
            String::new()
        };
        set(
            usize::from(*index),
            &name.to_uppercase(),
            &sub,
            "",
            crate::game::UiAction::UseItem(*index),
            *usable,
        );
    }
    if inventory.is_empty() {
        set(0, "ATTACK", "A", "A", crate::game::UiAction::AttackMove, has_army);
        set(1, "MOVE", "M", "M", crate::game::UiAction::Move, has_army);
        set(2, "STOP", "S", "S", crate::game::UiAction::Stop, true);
        set(3, "HOLD", "H", "H", crate::game::UiAction::Hold, has_army);
        set(4, "PATROL", "P", "P", crate::game::UiAction::Patrol, has_army);
        set(5, "ARMY", "A", "A", crate::game::UiAction::SelectArmy, true);
    }
    // Abilities: a hero gets all four shipped ability ids at 6/7/10/11 with
    // data hotkeys; non-hero casters keep their implemented spell pair.
    let ability_slots = [6usize, 7, 10, 11];
    let hero_abilities: Vec<&'static str> = hero
        .map(|unit| unit.def().ability_ids.to_vec())
        .unwrap_or_default();
    if hero.is_some() && !hero_abilities.is_empty() {
        for (index, ability_id) in hero_abilities.iter().take(4).enumerate() {
            let label = game
                .assets
                .objects
                .as_ref()
                .and_then(|db| db.ability(ability_id))
                .and_then(|view| view.name())
                .unwrap_or_else(|| (*ability_id).to_string())
                .to_uppercase();
            let hotkey = game
                .assets
                .ability_hotkey(ability_id)
                .map(|letter| letter.to_ascii_uppercase().to_string())
                .unwrap_or_else(|| card_hotkey(&label, None));
            let spells = hero.map(|unit| unit.def().spells).unwrap_or(&[]);
            let implemented = spells
                .iter()
                .position(|spell| spell.def().id.eq_ignore_ascii_case(ability_id));
            let slot = ability_slots[index];
            match implemented.and_then(|spell_index| game.ability_button(spell_index)) {
                Some((_, sub, action, enabled)) => {
                    set(slot, &label, &sub, &hotkey, action, enabled);
                }
                None => {
                    // Shipped ability without a simulated Spell: data-only row.
                    set(slot, &label, "DATA", &hotkey, crate::game::UiAction::Stop, false);
                }
            }
        }
    } else {
        for index in 0..2usize {
            let Some((label, sub, action, enabled)) = game.ability_button(index) else {
                continue;
            };
            let hotkey = if let crate::game::UiAction::Cast(spell) = action {
                game.assets
                    .ability_hotkey(spell.def().id)
                    .map(|letter| letter.to_ascii_uppercase().to_string())
                    .unwrap_or_else(|| card_hotkey(&label, None))
            } else {
                card_hotkey(&label, None)
            };
            let slot = match index {
                0 => 10,
                _ => 11,
            };
            set(slot, &label, &sub, &hotkey, action, enabled);
        }
    }
    if hero.is_some() && hero.map(|unit| unit.skill_points).unwrap_or(0) > 0 {
        // LEARN is data-free in the sim's action enum today; the game.rs patch
        // adds `UiAction::LearnAbility` and fills this slot. Until then the
        // slot stays empty instead of pretending to work.
    }
    if building.is_some() {
        // Production queue entries: visible info rows (cancel/rally need the
        // `UiAction::CancelProduction`/`SetRally` patch in game.rs).
        if let Some(unit) = building {
            for (index, production) in unit.production.iter().take(4).enumerate() {
                let name = wc3_sim::def_at(production.def)
                    .map(|def| def.name.to_uppercase())
                    .unwrap_or_else(|| "PRODUCTION".to_string());
                set(
                    6 + index,
                    &name,
                    &format!("{}s", (production.remaining + 29) / 30),
                    "",
                    crate::game::UiAction::Stop,
                    false,
                );
            }
        }
    }
    // Keep the legacy zoom/base pair visible only when no ability occupies the
    // last row (hero abilities already live at 6/7/10/11).
    if !matches!(game.ability_button(0), Some(_)) && hero.is_none() {
        set(10, "BASE", "B", "B", crate::game::UiAction::CentreBase, true);
        set(11, "ZOOM+", "+", "+", crate::game::UiAction::ZoomIn, true);
    }
    slots
        .into_iter()
        .map(|slot| slot.unwrap_or_else(Button::empty))
        .collect()
}
#[cfg(test)]
mod cyrillic_atlas_tests {
    use super::*;

    fn game_font_bytes() -> Option<Vec<u8>> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/Fonts/FRIZQT__.TTF");
        std::fs::read(path).ok()
    }

    #[test]
    fn frizqt_covers_cyrillic_without_tofu() {
        let Some(bytes) = game_font_bytes() else {
            eprintln!("skip: operator FRIZQT__.TTF missing");
            return;
        };
        let font = Font::load_from(Some(&bytes));
        assert_eq!(font.source, FontSource::Game);
        let coverage = font.coverage();
        assert!(coverage.ascii_covered >= 95, "{coverage:?}");
        assert!(coverage.cyrillic_covered >= 60, "{coverage:?}");
        for character in ['Ж', 'я', 'ё'] {
            assert!(font.has_glyph(character), "missing {character:?}");
            assert!(!font.missing_chars().contains(&character));
        }
        let mut batch = UiBatch::default();
        font.text_exact(&mut batch, "Привет, мир", 0.0, 0.0, 1.0, [1.0; 4]);
        assert!(!batch.font.is_empty(), "Cyrillic glyph quads emitted");
        assert_eq!(sanitize_hud_text("|cffffcc00Gold|r\nnext"), "Gold next");
        let box_rect = font.text_box(
            &mut batch,
            "|cffffcc00Привет|r",
            Rect::new(0.0, 0.0, 200.0, 40.0),
            0.4,
            [1.0; 4],
            TextAlign::Left,
            None,
        );
        assert!(box_rect.w > 0.0);
        assert!(
            box_rect.h >= MIN_TEXT_PX,
            "console text floor: {}",
            box_rect.h
        );
    }
}

/// Command-card data-path guards: the HUD consumes the roster's `Trains=` /
/// `trained_at` links and `data::build_menu` (via `sim_bridge::build_menu`).
/// These tests mirror the canonical QA gates `train.menu_linkage` and
/// `build.all_defs` at the layer boundary so a roster/menu edit cannot silently
/// reintroduce an unresolvable card entry.
#[cfg(test)]
mod command_card_linkage_tests {
    use super::*;

    /// Units produced by abilities rather than a `Trains=` link (matrix-listed).
    const ABILITY_PRODUCED: [&str; 3] = ["hmil", "ushd", "ehpr"];

    #[test]
    fn train_links_are_bidirectional_and_resolvable() {
        let mut links = 0usize;
        for def in wc3_sim::ROSTER.iter() {
            for trained in def.trains {
                links += 1;
                assert!(
                    wc3_sim::def_of(trained).is_some(),
                    "{}: Trains entry {trained} does not resolve",
                    def.id
                );
            }
            let ability_only = ABILITY_PRODUCED.contains(&def.id);
            for producer in def.trained_at {
                links += 1;
                if ability_only {
                    continue;
                }
                let building = wc3_sim::def_of(producer)
                    .unwrap_or_else(|| panic!("{}: producer {producer} missing", def.id));
                assert!(
                    building.trains.contains(&def.id),
                    "{}: producer {producer} does not list it in Trains",
                    def.id
                );
            }
        }
        assert!(links >= 119, "card links covered: {links}");
    }

    #[test]
    fn p0_missing_train_links_resolve() {
        // The two QA failures this guard was added for: Workshop -> Steam Tank
        // (`harm` -> `hmtt`) and Altar of Elders -> Priestess (`eate` -> `Emoo`).
        for (producer, unit) in [("harm", "hmtt"), ("eate", "Emoo")] {
            let building = wc3_sim::def_of(producer).expect("producer def");
            let trained = wc3_sim::def_of(unit).expect("train link resolves");
            assert!(building.trains.contains(&unit), "{producer} lists {unit}");
            assert!(
                trained.trained_at.contains(&producer),
                "{unit} lists producer {producer}"
            );
        }
    }

    #[test]
    fn build_menu_lists_only_worker_buildable_defs() {
        for race in [
            wc3_sim::Race::Human,
            wc3_sim::Race::Orc,
            wc3_sim::Race::Undead,
            wc3_sim::Race::NightElf,
        ] {
            let menu = wc3_sim::data::build_menu(race);
            assert!(!menu.is_empty(), "{race:?} build menu");
            for def in &menu {
                assert!(def.is_building(), "{} is a building", def.id);
                assert!(def.buildable, "{} is worker-buildable", def.id);
            }
        }
    }
}

/// Command-card layout acceptance: a present button must be visible with its
/// label/hotkey/cost, and the worker/hall cards must carry the real data rows.
#[cfg(test)]
mod card_layout_tests {
    use super::*;
    use crate::assets::Assets;
    use crate::game::{Game, UiAction};
    use glam::Vec2;

    fn game_with(def_id: &str) -> Game {
        let mut game = Game::new(Assets::load(None, None), 0xCA4D);
        let unit = game
            .sim
            .spawn_def(0, def_id, Vec2::new(600.0, 600.0))
            .expect("unit spawn");
        game.selection = vec![unit];
        game
    }

    #[test]
    fn ui_command_card_labels_never_three_char_codes() {
        assert_eq!(card_label("GATH", "GATHER"), "GATHER");
        assert_eq!(card_label("KA3", "КАЗАРМЫ"), "КАЗАРМЫ");
        assert_eq!(card_label("DP..", "Deep Water"), "DEEP WATER");
        assert_eq!(card_label("BURROW", "Burrow"), "BURROW");
        assert_eq!(card_label("", "SPIRIT LODGE"), "SPIRIT LODGE");
        assert_eq!(card_label("FOOT", "FOOTMAN"), "FOOTMAN");
    }

    #[test]
    fn ui_command_card_hero_has_four_ability_slots_with_hotkeys() {
        let game = game_with("Hpal");
        let buttons = build_buttons(&game);
        let mut abilities = 0;
        let mut castable = 0;
        for slot in [6usize, 7, 10, 11] {
            let button = &buttons[slot];
            assert!(
                !button.label.is_empty(),
                "hero ability slot {slot} is empty"
            );
            assert!(
                !button.hotkey.is_empty(),
                "data hotkey for {}",
                button.label
            );
            assert!(
                button.label.chars().count() > 3,
                "no 3-char code in slot {slot}: {}",
                button.label
            );
            abilities += 1;
            if matches!(button.action, UiAction::Cast(_)) {
                castable += 1;
            }
        }
        assert_eq!(abilities, 4);
        assert!(castable >= 2, "implemented spells: {castable}");
        for slot in [6usize, 7, 10, 11] {
            assert_ne!(buttons[slot].label, "BASE", "ability slot {slot} kept");
            assert_ne!(buttons[slot].sub, "B", "ability slot {slot} kept");
        }
    }

    #[test]
    fn ui_command_card_worker_shows_three_builds_with_hotkeys_and_more() {
        let game = game_with("hpea");
        let buttons = build_buttons(&game);
        assert_eq!(buttons.len(), 12, "12-slot card vector");
        let builds: Vec<&Button> = buttons
            .iter()
            .filter(|button| matches!(button.action, UiAction::BuildDef(_)))
            .collect();
        assert!(
            builds.len() >= 3,
            "worker build entries: {:?}",
            builds.iter().map(|button| &button.label).collect::<Vec<_>>()
        );
        for button in builds {
            assert!(!button.label.is_empty(), "build label");
            assert!(!button.hotkey.is_empty(), "hotkey for {}", button.label);
            assert!(!button.sub.is_empty(), "cost for {}", button.label);
        }
        assert!(
            buttons
                .iter()
                .any(|button| matches!(button.action, UiAction::BuildPageNext)),
            "MORE paging button"
        );
        assert!(
            buttons.iter().any(|button| button.label == "GATHER"),
            "worker gather command"
        );
        // Every authored button has a visible label (no blank tile with data).
        for button in buttons.iter().filter(|button| button.enabled) {
            assert!(
                !button.label.is_empty(),
                "enabled button without a label: action {:?} sub {}",
                button.action, button.sub
            );
        }
    }

    #[test]
    fn ui_command_card_hall_trains_and_producer_has_multiple_entries() {
        let hall = game_with("htow");
        let buttons = build_buttons(&hall);
        let trains: Vec<&Button> = buttons
            .iter()
            .filter(|button| matches!(button.action, UiAction::TrainDef(_)))
            .collect();
        assert!(
            !trains.is_empty(),
            "town hall trains its worker (shipped Trains=hpea)"
        );
        for button in &trains {
            assert!(!button.label.is_empty());
            assert!(!button.hotkey.is_empty(), "hotkey for {}", button.label);
            assert!(!button.sub.is_empty(), "cost for {}", button.label);
        }
        let barracks = game_with("hbar");
        let buttons = build_buttons(&barracks);
        let trains: Vec<&Button> = buttons
            .iter()
            .filter(|button| {
                matches!(
                    button.action,
                    UiAction::TrainDef(_) | UiAction::Train(_)
                )
            })
            .collect();
        assert!(
            trains.len() >= 2,
            "barracks trains multiple units: {:?}",
            trains.iter().map(|button| &button.label).collect::<Vec<_>>()
        );
        for button in trains {
            assert!(!button.label.is_empty());
            assert!(!button.hotkey.is_empty(), "hotkey for {}", button.label);
            assert!(!button.sub.is_empty(), "cost for {}", button.label);
        }
    }
}

// ---------------------------------------------------------------------------
// Nine-patch acceptance tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod nine_patch_tests {
    use super::*;

    /// Sampling model of the reference rasteriser. `Nearest` is the
    /// texel-exact model used to prove the corner mapping; `Linear` mirrors the
    /// UI sampler (`FilterMode::Linear`, `AddressMode::ClampToEdge`) and is used
    /// for the seam and tint checks.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Sample {
        Nearest,
        Linear,
    }

    /// CPU mirror of the UI pass: straight-alpha blend over a transparent canvas
    /// plus a coverage counter per pixel (how many quads wrote it).
    struct Canvas {
        width: usize,
        height: usize,
        rgba: Vec<u8>,
        coverage: Vec<u16>,
    }

    impl Canvas {
        fn new(width: u32, height: u32) -> Self {
            Self {
                width: width as usize,
                height: height as usize,
                rgba: vec![0; width as usize * height as usize * 4],
                coverage: vec![0; width as usize * height as usize],
            }
        }

        fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
            let offset = (y * self.width + x) * 4;
            [
                self.rgba[offset],
                self.rgba[offset + 1],
                self.rgba[offset + 2],
                self.rgba[offset + 3],
            ]
        }

        fn coverage(&self, x: usize, y: usize) -> u16 {
            self.coverage[y * self.width + x]
        }

        /// Blend one straight-alpha texel.
        fn blend(&mut self, x: usize, y: usize, source: [u8; 4]) {
            let offset = (y * self.width + x) * 4;
            let sa = source[3] as f32 / 255.0;
            let da = self.rgba[offset + 3] as f32 / 255.0;
            let out_a = sa + da * (1.0 - sa);
            for channel in 0..3 {
                let src = source[channel] as f32;
                let dst = self.rgba[offset + channel] as f32;
                let value = if out_a <= 0.0 {
                    0.0
                } else {
                    (src * sa + dst * da * (1.0 - sa)) / out_a
                };
                self.rgba[offset + channel] = value.round().clamp(0.0, 255.0) as u8;
            }
            self.rgba[offset + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
            self.coverage[y * self.width + x] += 1;
        }
    }

    #[inline]
    fn texel(atlas: &[u8], width: u32, height: u32, x: i64, y: i64) -> [u8; 4] {
        let x = x.clamp(0, width as i64 - 1) as usize;
        let y = y.clamp(0, height as i64 - 1) as usize;
        let offset = (y * width as usize + x) * 4;
        [
            atlas[offset],
            atlas[offset + 1],
            atlas[offset + 2],
            atlas[offset + 3],
        ]
    }

    fn sample_atlas(
        atlas: &[u8],
        width: u32,
        height: u32,
        u: f32,
        v: f32,
        sample: Sample,
    ) -> [u8; 4] {
        match sample {
            Sample::Nearest => {
                let x = (u * width as f32).floor() as i64;
                let y = (v * height as f32).floor() as i64;
                texel(atlas, width, height, x, y)
            }
            Sample::Linear => {
                // `textureSample`: position in texel space, then bilinear.
                let px = u * width as f32 - 0.5;
                let py = v * height as f32 - 0.5;
                let x0 = px.floor();
                let y0 = py.floor();
                let fx = px - x0;
                let fy = py - y0;
                let x0 = x0 as i64;
                let y0 = y0 as i64;
                let p00 = texel(atlas, width, height, x0, y0);
                let p10 = texel(atlas, width, height, x0 + 1, y0);
                let p01 = texel(atlas, width, height, x0, y0 + 1);
                let p11 = texel(atlas, width, height, x0 + 1, y0 + 1);
                let mut out = [0u8; 4];
                for channel in 0..4 {
                    let top = p00[channel] as f32 * (1.0 - fx) + p10[channel] as f32 * fx;
                    let bottom = p01[channel] as f32 * (1.0 - fx) + p11[channel] as f32 * fx;
                    let value = top * (1.0 - fy) + bottom * fy;
                    out[channel] = value.round().clamp(0.0, 255.0) as u8;
                }
                out
            }
        }
    }

    /// Emit one nine-patch through the production path (`UiBatch::nine_patch`).
    fn emit(patch: &NinePatch, dest: Rect) -> UiBatch {
        let mut batch = UiBatch::default();
        batch.nine_patch(dest, patch, [1.0; 4]);
        batch
    }

    /// Read back the quads `UiBatch` emitted for one texture: (dest, uv).
    fn quads(batch: &UiBatch, texture: usize) -> Vec<(Rect, [f32; 4])> {
        let mut out = Vec::new();
        for draw in &batch.textured {
            if draw.texture != texture {
                continue;
            }
            for chunk in draw.vertices.chunks_exact(6) {
                let origin = chunk[0].pos;
                let right = chunk[1].pos;
                let bottom = chunk[5].pos;
                out.push((
                    Rect::new(
                        origin[0],
                        origin[1],
                        right[0] - origin[0],
                        bottom[1] - origin[1],
                    ),
                    [
                        chunk[0].uv[0],
                        chunk[0].uv[1],
                        chunk[2].uv[0],
                        chunk[2].uv[1],
                    ],
                ));
            }
        }
        out
    }

    /// Rasterise emitted quads the way the UI pass samples them.
    fn rasterize_quads(
        quads: &[(Rect, [f32; 4])],
        canvas: (u32, u32),
        atlas: &[u8],
        atlas_size: [f32; 2],
        sample: Sample,
    ) -> Canvas {
        let mut target = Canvas::new(canvas.0, canvas.1);
        for (dest, uv) in quads {
            if dest.w <= 0.0 || dest.h <= 0.0 {
                continue;
            }
            let x0 = dest.x.max(0.0).floor() as i64;
            let y0 = dest.y.max(0.0).floor() as i64;
            let x1 = (dest.right().min(canvas.0 as f32)).ceil() as i64;
            let y1 = (dest.bottom().min(canvas.1 as f32)).ceil() as i64;
            for y in y0..y1 {
                for x in x0..x1 {
                    let centre_x = x as f32 + 0.5;
                    let centre_y = y as f32 + 0.5;
                    if centre_x < dest.x
                        || centre_y < dest.y
                        || centre_x >= dest.right()
                        || centre_y >= dest.bottom()
                    {
                        continue;
                    }
                    let fx = (centre_x - dest.x) / dest.w;
                    let fy = (centre_y - dest.y) / dest.h;
                    let u = uv[0] + (uv[2] - uv[0]) * fx;
                    let v = uv[1] + (uv[3] - uv[1]) * fy;
                    let pixel = sample_atlas(
                        atlas,
                        atlas_size[0] as u32,
                        atlas_size[1] as u32,
                        u,
                        v,
                        sample,
                    );
                    target.blend(x as usize, y as usize, pixel);
                }
            }
        }
        target
    }

    /// Rasterise one nine-patch exactly the way `UiBatch::nine_patch` emits it.
    fn rasterize(
        patch: &NinePatch,
        dest: Rect,
        canvas: (u32, u32),
        atlas: &[u8],
        sample: Sample,
    ) -> Canvas {
        let batch = emit(patch, dest);
        let quads = quads(&batch, patch.texture);
        rasterize_quads(&quads, canvas, atlas, patch.atlas, sample)
    }

    /// Deterministic synthetic atlas: every texel encodes its own coordinate, so
    /// a wrong UV or a stretched corner is impossible to hide.
    fn synthetic_atlas(width: u32, height: u32) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        for y in 0..height {
            for x in 0..width {
                rgba.push(((x * 7 + y * 3) % 251) as u8);
                rgba.push(((x * 13 + y * 5) % 253) as u8);
                rgba.push(((x * 17 + y * 11) % 255) as u8);
                rgba.push(255);
            }
        }
        rgba
    }

    fn patch_with_inset(inset: f32, source: Rect, art_scale: f32) -> NinePatch {
        NinePatch {
            texture: 0,
            source,
            insets: [inset; 4],
            atlas: [256.0, 256.0],
            art_scale,
            tile: false,
        }
    }

    /// The 40x40 corner block of a 1x frame and the same block of a 2x frame must
    /// be the same pixels: the corner keeps its source size and texels.
    #[test]
    fn hud_corner_block_pixel_identical_at_2x() {
        let atlas = synthetic_atlas(256, 256);
        let source = Rect::new(0.0, 0.0, 100.0, 100.0);
        let patch = patch_with_inset(40.0, source, 1.0);
        let one = rasterize(
            &patch,
            Rect::new(0.0, 0.0, 200.0, 120.0),
            (200, 120),
            &atlas,
            Sample::Nearest,
        );
        let two = rasterize(
            &patch,
            Rect::new(0.0, 0.0, 400.0, 240.0),
            (400, 240),
            &atlas,
            Sample::Nearest,
        );
        // The corner quad keeps the source corner size at both panel sizes.
        let one_slices = nine_slices(Rect::new(0.0, 0.0, 200.0, 120.0), &patch);
        let two_slices = nine_slices(Rect::new(0.0, 0.0, 400.0, 240.0), &patch);
        assert_eq!(one_slices[0].dest.w, 40.0, "1x corner width");
        assert_eq!(one_slices[0].dest.h, 40.0, "1x corner height");
        assert_eq!(two_slices[0].dest.w, 40.0, "2x panel keeps corner width");
        assert_eq!(two_slices[0].dest.h, 40.0, "2x panel keeps corner height");
        assert_eq!(one_slices[0].uv, two_slices[0].uv, "corner UV is scale-free");
        let mut mismatches = 0;
        for y in 0..40 {
            for x in 0..40 {
                if one.pixel(x, y) != two.pixel(x, y) {
                    mismatches += 1;
                }
            }
        }
        assert_eq!(mismatches, 0, "corner texels differ between 1x and 2x");
        // Positive control: the stretched edge band *must* differ, otherwise the
        // frame would not have grown at all.
        assert_ne!(
            one.pixel(150, 60),
            two.pixel(150, 60),
            "middle band stretches with the panel"
        );
    }

    /// When the UI scale doubles, the corner art doubles: every 2x2 destination
    /// block must equal one 1x texel (no resampling of the ornament).
    #[test]
    fn hud_corner_block_texel_exact_at_art_scale_2x() {
        let atlas = synthetic_atlas(256, 256);
        let source = Rect::new(0.0, 0.0, 100.0, 100.0);
        let one = patch_with_inset(40.0, source, 1.0);
        let two = patch_with_inset(40.0, source, 2.0);
        let raster_one = rasterize(&one, Rect::new(0.0, 0.0, 200.0, 120.0), (400, 240), &atlas, Sample::Nearest);
        let raster_two = rasterize(&two, Rect::new(0.0, 0.0, 400.0, 240.0), (400, 240), &atlas, Sample::Nearest);
        let slices = nine_slices(Rect::new(0.0, 0.0, 400.0, 240.0), &two);
        assert_eq!(slices[0].dest.w, 80.0, "art_scale 2 doubles the corner");
        let mut mismatches = 0;
        for y in 0..80 {
            for x in 0..80 {
                let expected = raster_one.pixel(x / 2, y / 2);
                if raster_two.pixel(x, y) != expected {
                    mismatches += 1;
                }
            }
        }
        assert_eq!(mismatches, 0, "2x corner is a texel-exact upscale");
    }

    /// The nine pieces must tile the destination rect exactly once: no gap (a
    /// pixel with coverage 0) and no overlap (coverage 2).
    #[test]
    fn hud_nine_patch_tiles_destination_exactly_once() {
        let atlas = synthetic_atlas(256, 256);
        let patch = patch_with_inset(24.0, Rect::new(16.0, 16.0, 128.0, 96.0), 1.7);
        let raster = rasterize(&patch, Rect::new(10.5, 7.25, 233.75, 141.5), (256, 160), &atlas, Sample::Linear);
        let mut gaps = 0;
        let mut overlaps = 0;
        let inside = |x: usize, y: usize| {
            let cx = x as f32 + 0.5;
            let cy = y as f32 + 0.5;
            cx >= 10.5 && cx < 10.5 + 233.75 && cy >= 7.25 && cy < 7.25 + 141.5
        };
        for y in 0..160 {
            for x in 0..256 {
                if !inside(x, y) {
                    continue;
                }
                match raster.coverage(x, y) {
                    1 => {}
                    0 => gaps += 1,
                    2.. => overlaps += 1,
                    _ => unreachable!(),
                }
            }
        }
        assert_eq!(gaps, 0, "pixels no piece wrote");
        assert_eq!(overlaps, 0, "pixels two pieces wrote");
    }

    /// The eight boundaries between the nine regions must not show a seam: with a
    /// uniform source the rasterised frame has one single colour everywhere.
    #[test]
    fn hud_nine_patch_seams_continuous() {
        let width = 64u32;
        let height = 64u32;
        let mut atlas = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..width * height {
            atlas.extend_from_slice(&[180, 120, 60, 255]);
        }
        let patch = NinePatch {
            texture: 7,
            source: Rect::new(0.0, 0.0, 64.0, 64.0),
            insets: [16.0, 16.0, 16.0, 16.0],
            atlas: [64.0, 64.0],
            art_scale: 1.0,
            tile: false,
        };
        let dest = Rect::new(8.0, 4.0, 200.0, 120.0);
        let raster = rasterize(&patch, dest, (224, 132), &atlas, Sample::Linear);
        let slices = nine_slices(dest, &patch);
        // The eight internal boundaries: x at 16+8 and 8+200-16; y at 4+16 and
        // 4+120-16, checked across the full span.
        let boundaries_x = [slices[1].dest.x as usize, slices[5].dest.x as usize];
        let boundaries_y = [slices[3].dest.y as usize, slices[7].dest.y as usize];
        let mut seams = Vec::new();
        for &bx in &boundaries_x {
            for y in 4..124 {
                let left = raster.pixel(bx - 1, y);
                let right = raster.pixel(bx, y);
                if left != right {
                    seams.push((bx, y, left, right));
                }
            }
        }
        for &by in &boundaries_y {
            for x in 8..208 {
                let top = raster.pixel(x, by - 1);
                let bottom = raster.pixel(x, by);
                if top != bottom {
                    seams.push((x, by, top, bottom));
                }
            }
        }
        assert!(
            seams.is_empty(),
            "seam discontinuities: {:?}",
            &seams[..seams.len().min(4)]
        );
        // No pixel may be left uncovered either side of a boundary.
        for &bx in &boundaries_x {
            for y in 4..124 {
                assert!(raster.pixel(bx - 1, y)[3] > 0, "gap left of x={bx}");
                assert!(raster.pixel(bx, y)[3] > 0, "gap right of x={bx}");
            }
        }
    }

    /// A tiled edge repeats the source band instead of stretching it.
    #[test]
    fn hud_tiled_edges_repeat_source_band() {
        let atlas = synthetic_atlas(64, 64);
        let mut patch = NinePatch {
            texture: 1,
            source: Rect::new(0.0, 0.0, 64.0, 64.0),
            insets: [8.0, 8.0, 8.0, 8.0],
            atlas: [64.0, 64.0],
            art_scale: 1.0,
            tile: false,
        };
        let stretched = rasterize(&patch, Rect::new(0.0, 0.0, 128.0, 64.0), (128, 64), &atlas, Sample::Nearest);
        patch.tile = true;
        let tiled = rasterize(&patch, Rect::new(0.0, 0.0, 128.0, 64.0), (128, 64), &atlas, Sample::Nearest);
        // The top band in tiled mode shows the 48 px source band repeatedly, so
        // the pixel at x=8 equals the pixel one period later (x=56) and both show
        // source texels rather than a spread-out interpolation.
        assert_eq!(tiled.pixel(8, 4), tiled.pixel(56, 4), "period repeats");
        assert_eq!(
            tiled.pixel(8, 4),
            atlas_texel(&atlas, 64, 64, 8, 4),
            "band shows source texels"
        );
        assert_ne!(
            stretched.pixel(8, 4),
            stretched.pixel(40, 4),
            "stretched band spreads the source"
        );
        // Tiling still covers the whole band: whole periods plus the tail.
        for x in 8..120 {
            assert!(tiled.pixel(x, 4)[3] > 0, "tiled band gap at x={x}");
        }
    }

    fn atlas_texel(atlas: &[u8], width: u32, height: u32, x: i64, y: i64) -> [u8; 4] {
        texel(atlas, width, height, x, y)
    }

    /// The loader must leave no pixel matching the chroma key, and no key fringe
    /// may survive in the transparent ring (linear filtering would sample it).
    #[test]
    fn hud_atlas_loader_zero_magenta() {
        use crate::ui_atlas::{AlphaMode, Atlas, ChromaKey};
        // 32x32 sheet: magenta field with two 10x10 art blocks.
        let (width, height) = (32u32, 32u32);
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let art = (4..14).contains(&x) && (4..14).contains(&y)
                    || (18..28).contains(&x) && (18..28).contains(&y);
                if art {
                    rgba.extend_from_slice(&[90, 70, 40, 255]);
                } else {
                    rgba.extend_from_slice(&[255, 0, 255, 255]);
                }
            }
        }
        let png = encode_png(width, height, &rgba);
        let manifest = r#"{
            "source": "synthetic",
            "size": [32, 32],
            "chroma_key": "magenta (>200,<80,>200)",
            "elements": [
                {"x": 4, "y": 4, "w": 10, "h": 10},
                {"x": 18, "y": 18, "w": 10, "h": 10}
            ]
        }"#;
        let atlas = Atlas::load(manifest, &png, None, AlphaMode::Straight, 3)
            .expect("synthetic atlas loads");
        assert_eq!(atlas.remaining_key_pixels(), 0, "key pixels left");
        assert_eq!(atlas.fringe_pixels(), 0, "anti-aliased key fringe left");
        // Transparent ring must carry the art colour, not the key colour.
        for (x, y) in [(3u32, 4u32), (14, 9), (4, 3), (9, 14)] {
            let pixel = atlas.image.pixel(x, y);
            assert_eq!(pixel[3], 0, "ring texel a=0 at {x},{y}");
            assert_ne!(
                [pixel[0], pixel[1], pixel[2]],
                [255, 0, 255],
                "key colour survived in the ring at {x},{y}"
            );
            assert_eq!(
                [pixel[0], pixel[1], pixel[2]],
                [90, 70, 40],
                "ring texel borrowed the art colour at {x},{y}"
            );
        }
        // A fully keyed sheet reports every pixel.
        let mut sheet = crate::ui_atlas::Rgba8::from_rgba(2, 2, vec![[255u8, 0, 255, 255]; 4].concat())
            .expect("2x2 sheet");
        let report = crate::ui_atlas::key_chroma(&mut sheet, ChromaKey::MAGENTA, 1);
        assert_eq!(report.keyed, 4);
        assert_eq!(
            sheet.pixel(0, 0)[3],
            0,
            "keyed texel is transparent"
        );
    }

    /// Every loaded atlas sheet - the fantasy one the operator handed over - must
    /// come out of the loader with zero magenta pixels.
    #[test]
    fn hud_real_atlas_sheet_zero_magenta() {
        use crate::ui_atlas::{AlphaMode, Atlas, AtlasFiles, find_installed_atlas};
        // Installed copy first (the app's own files dir is what the device reads),
        // then the repo copy so the test runs on a host that cannot enter the
        // app's private directory.
        let fallback = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/h2/atlas");
        let files = find_installed_atlas().or_else(|| {
            let image = fallback.join("ui_atlas.png");
            let manifest = fallback.join("ui_atlas.json");
            if !image.is_file() || !manifest.is_file() {
                return None;
            }
            let insets = fallback.join("ui_atlas_insets.json");
            Some(AtlasFiles {
                image,
                manifest,
                insets: insets.is_file().then_some(insets),
            })
        });
        let Some(files) = files else {
            eprintln!("no ui_atlas png/json anywhere - skipped");
            return;
        };
        let atlas = Atlas::load_files(
            &files.image,
            &files.manifest,
            files.insets.as_deref(),
            AlphaMode::Straight,
        )
        .expect("installed atlas loads");
        let magenta = atlas.remaining_key_pixels();
        let fringe = atlas.fringe_pixels();
        assert_eq!(magenta, 0, "magenta survived");
        assert_eq!(fringe, 0, "magenta fringe survived (count {fringe})");
        assert!(!atlas.elements.is_empty(), "manifest produced no elements");
        for element in &atlas.elements {
            assert!(
                element.insets.iter().all(|inset| inset.is_finite() && *inset >= 0.0),
                "element {} insets {:?}",
                element.name,
                element.insets
            );
            assert!(
                element.insets[0] + element.insets[2] <= element.rect.w,
                "element {} horizontal insets exceed the source",
                element.name
            );
            assert!(
                element.insets[1] + element.insets[3] <= element.rect.h,
                "element {} vertical insets exceed the source",
                element.name
            );
        }
        eprintln!(
            "atlas {}: {}x{} elements={} key_pixels=0",
            atlas.source,
            atlas.image.width,
            atlas.image.height,
            atlas.elements.len()
        );
    }

    /// Encode a PNG for the fixtures (the inverse of the loader).
    fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("png header");
            writer.write_image_data(rgba).expect("png data");
        }
        out
    }
}

#[cfg(test)]
mod rect_sides_tests {
    use super::Rect;

    /// Atlas frames carry asymmetric nine-patch insets, so the art rect around
    /// an image is grown per side, never by a single uniform amount.
    #[test]
    fn expanded_sides_is_the_inverse_of_inset_sides() {
        let image = Rect::new(100.0, 50.0, 330.0, 330.0);
        let frame = image.expanded_sides(21.0, 27.0, 21.0, 29.0);
        assert_eq!(frame, Rect::new(79.0, 23.0, 372.0, 386.0));
        assert_eq!(frame.inset_sides(21.0, 27.0, 21.0, 29.0), image);
        // A uniform expansion is the same call with equal sides.
        assert_eq!(image.expanded(8.0, 8.0), image.expanded_sides(8.0, 8.0, 8.0, 8.0));
    }
}

#[cfg(test)]
mod color_space_tests {
    use super::*;

    /// The UI surface is `Rgba8UnormSrgb`: vertex colours must be linear or the
    /// authored colour is displayed gamma-brightened. Measured on device: a
    /// (36,30,26) strip rendered as (104,96,89) before this conversion.
    #[test]
    fn ui_colours_are_written_in_linear_space() {
        let s = 36.0 / 255.0;
        let linear = srgb_to_linear(s);
        assert!(
            (linear - 0.017_642).abs() < 0.001,
            "srgb_to_linear({s}) = {linear}"
        );
        // Round trip: the surface re-encodes linear back to the authored byte.
        let back = linear_to_srgb(linear) * 255.0;
        assert!((back - 36.0).abs() < 0.5, "round trip gave {back}");

        let mut batch = UiBatch::default();
        batch.textured_quad(
            0,
            Rect::new(0.0, 0.0, 10.0, 10.0),
            [0.0, 0.0, 1.0, 1.0],
            [s, 30.0 / 255.0, 26.0 / 255.0, 0.5],
        );
        let color = batch.textured[0].vertices[0].color;
        assert!(
            (color[0] - linear).abs() < 1e-4,
            "vertex colour must be linear, got {}",
            color[0]
        );
        assert_eq!(color[3], 0.5, "alpha is never gamma-converted");
    }

    /// `ui.colour.linear-space` — every vertex path writes an sRGB-authored
    /// colour as linear, and `authored_color` maps it back to the authored
    /// value the surface displays. The invariant is the round trip, so it holds
    /// for any authored byte instead of pinning one linear constant.
    #[test]
    fn ui_colour_linear_space_quad_paths_convert_and_round_trip() {
        let authored = [
            // Measured on device: the strip that displayed as (104,96,89).
            [36.0 / 255.0, 30.0 / 255.0, 26.0 / 255.0, 1.0],
            // A HUD fill constant (`stats::HP_FILL`), authored in sRGB.
            [0.22, 0.75, 0.24, 1.0],
            // A translucent tint: alpha must survive untouched.
            [1.0, 0.93, 0.90, 0.5],
            [0.0, 0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0, 1.0],
        ];
        for color in authored {
            let mut batch = UiBatch::default();
            batch.textured_quad(0, Rect::new(0.0, 0.0, 8.0, 8.0), [0.0, 0.0, 1.0, 1.0], color);
            let rect = Rect::new(0.0, 0.0, 8.0, 8.0);
            batch.quad(UiTexture::White, rect, [0.0, 0.0, 1.0, 1.0], color);
            batch.quad(UiTexture::Font, rect, [0.0, 0.0, 1.0, 1.0], color);
            batch.quad(UiTexture::Minimap, rect, [0.0, 0.0, 1.0, 1.0], color);
            let vertices: Vec<_> = batch
                .textured
                .iter()
                .flat_map(|draw| draw.vertices.iter().cloned())
                .chain(batch.white.iter().cloned())
                .chain(batch.font.iter().cloned())
                .chain(batch.minimap.iter().cloned())
                .collect();
            assert_eq!(vertices.len(), 4 * 6, "four paths, six vertices each");
            for vertex in vertices {
                for channel in 0..3 {
                    assert!(
                        (vertex.color[channel] - srgb_to_linear(color[channel])).abs() < 1e-6,
                        "channel {channel} of {color:?} is written as {}, not linear {}",
                        vertex.color[channel],
                        srgb_to_linear(color[channel])
                    );
                    assert!(
                        (authored_color(vertex.color)[channel] - color[channel]).abs() < 1e-6,
                        "channel {channel} does not round-trip to the authored {color:?}"
                    );
                }
                assert_eq!(vertex.color[3], color[3], "alpha is not gamma-converted");
            }
        }
    }

    /// `ui.colour.linear-space` — the device measurement that motivated the fix.
    /// The authored (36,30,26) strip must display as (36,30,26); writing the
    /// authored bytes as linear displayed (104,96,89).
    #[test]
    fn ui_colour_linear_space_measured_strip_shows_the_authored_bytes() {
        let authored = [36.0 / 255.0, 30.0 / 255.0, 26.0 / 255.0, 1.0];
        let mut batch = UiBatch::default();
        batch.rect(Rect::new(0.0, 0.0, 4.0, 4.0), authored);
        let shown = authored_color(batch.white[0].color);
        for (channel, byte) in [36.0_f32, 30.0, 26.0].into_iter().enumerate() {
            let displayed = (shown[channel] * 255.0).round();
            assert!(
                (displayed - byte).abs() < 1.0,
                "channel {channel} displays byte {displayed}, authored {byte}"
            );
        }
        // Falsification: the pre-fix value (authored written straight into the
        // linear surface) is the washed-out 104..89 range the operator saw.
        let pre_fix = (linear_to_srgb(authored[0]) * 255.0).round();
        assert!(
            (104.0..=105.0).contains(&pre_fix),
            "the gamma-doubled red channel must be ~104 (measured), got {pre_fix}"
        );
    }

    /// `ui.colour.linear-space` — "darker" is lower emitted light. Linear
    /// luminance must rank the same authored ramp, and a 0.65 sRGB dim has to
    /// lower it (the naive sum of linear channels drops by 0.38 instead of
    /// 0.65, which is why a ratio assertion on it is wrong).
    #[test]
    fn ui_colour_linear_luminance_ranks_darker() {
        let white = linear_color([1.0, 1.0, 1.0, 1.0]);
        let mid = linear_color([0.5, 0.5, 0.5, 1.0]);
        let dark = linear_color([0.25, 0.25, 0.25, 1.0]);
        let black = linear_color([0.0, 0.0, 0.0, 1.0]);
        assert!(linear_luminance(black) < linear_luminance(dark));
        assert!(linear_luminance(dark) < linear_luminance(mid));
        assert!(linear_luminance(mid) < linear_luminance(white));
        assert!((linear_luminance(white) - 1.0).abs() < 1e-6);

        let tint = [0.9, 0.8, 0.7];
        let full = linear_color([tint[0], tint[1], tint[2], 1.0]);
        let dim = linear_color([tint[0] * 0.65, tint[1] * 0.65, tint[2] * 0.65, 1.0]);
        assert!(linear_luminance(dim) < linear_luminance(full), "dim is darker");
        let ratio = linear_luminance(dim) / linear_luminance(full);
        // The sRGB curve has an offset and a linear toe, so a 0.65 encoded
        // dim removes more light than a 0.65 linear multiply would (0.65) and
        // less than the pure 2.4 power law alone (0.65^2.4 = 0.356). That band
        // is the invariant; the exact ratio is a property of the transfer
        // function, not an authored byte.
        let power_law = 0.65_f32.powf(2.4);
        assert!(
            ratio < 0.65 && ratio > power_law,
            "a 0.65 sRGB dim must land in ({power_law:.3}, 0.65), got {ratio:.4}"
        );
        // The authored value is the one that carries the 0.65 factor.
        for channel in 0..3 {
            assert!(
                (authored_color(dim)[channel] / authored_color(full)[channel] - 0.65).abs() < 1e-6,
                "channel {channel} lost the authored dim"
            );
        }
    }

    /// Pure white and pure black are fixed points: existing untinted art keeps
    /// rendering exactly as before.
    #[test]
    fn white_and_black_are_fixed_points() {
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-6);
        assert!(srgb_to_linear(0.0).abs() < 1e-9);
        assert!((linear_to_srgb(1.0) - 1.0).abs() < 1e-6);
        assert!(linear_to_srgb(0.0).abs() < 1e-9);
    }
}

#[cfg(test)]
mod hud_font_tests {
    use super::*;

    /// `hud.font.wc3-nimrod`: the operator's own UI face loads from the
    /// installed archive (or the extracted game dump on a headless host).
    /// Skips with a reason when neither is present.
    #[test]
    fn hud_font_wc3_nimrod() {
        let Some((font, source)) = Font::load_nimrod_candidates() else {
            eprintln!("hud.font.wc3-nimrod SKIP reason=archive-and-extracted-font-absent");
            return;
        };
        let coverage = font.coverage();
        let advances = font.digit_advances();
        let tabular = font.digits_tabular();
        eprintln!(
            "hud.font.wc3-nimrod PASS source={source} covered={} cyrillic={} ascii={} digits_tabular={tabular} advances={advances:?}",
            coverage.covered, coverage.cyrillic_covered, coverage.ascii_covered
        );
        assert!(coverage.cyrillic_covered >= 60, "cyrillic {}", coverage.cyrillic_covered);
        assert!(coverage.ascii_covered >= 90, "ascii {}", coverage.ascii_covered);
        if tabular {
            let first = advances[0];
            for value in advances {
                assert!((value - first).abs() <= 0.01, "tabular flag but {advances:?}");
            }
        } else {
            assert!(
                advances.iter().any(|value| (value - advances[0]).abs() > 0.01),
                "non-tabular flag but advances are equal {advances:?}"
            );
        }
    }

    /// `hud.font.cyrillic-coverage`: every letter of the shipped Russian labels
    /// and every digit resolves in the primary face.
    #[test]
    fn hud_font_cyrillic_coverage() {
        let Some((font, source)) = Font::load_nimrod_candidates() else {
            eprintln!("hud.font.cyrillic-coverage SKIP reason=archive-and-extracted-font-absent");
            return;
        };
        let labels = ["ЖУРНАЛ", "ДОМ ВОЖДЕЙ", "ЛЕСОПИЛКА", "АЛЬЯНС", "0123456789"];
        for label in labels {
            for character in label.chars() {
                if character == ' ' {
                    continue;
                }
                assert!(
                    font.has_glyph(character),
                    "{source}: missing {character:?} (U+{:04X}) in {label}",
                    character as u32
                );
            }
        }
        eprintln!("hud.font.cyrillic-coverage PASS source={source} labels={} ", labels.len());
    }

    /// Raster proof: a Cyrillic run has ink in the atlas and its line box is at
    /// least `MIN_TEXT_PX` tall at the device scale.
    #[test]
    fn hud_font_cyrillic_raster() {
        let Some((font, _source)) = Font::load_nimrod_candidates() else {
            eprintln!("hud.font.cyrillic-raster SKIP reason=archive-and-extracted-font-absent");
            return;
        };
        let scale = MIN_TEXT_PX / font.line_height;
        assert!(
            font.line_height * scale >= MIN_TEXT_PX - 0.01,
            "line box {} < {MIN_TEXT_PX}",
            font.line_height * scale
        );
        let mut batch = UiBatch::default();
        font.text_exact(&mut batch, "ЖУРНАЛ", 0.0, 0.0, scale, [1.0; 4]);
        assert!(!batch.font.is_empty(), "no glyph quads emitted");
        let mut ink = 0usize;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for chunk in batch.font.chunks_exact(6) {
            for vertex in chunk {
                min_y = min_y.min(vertex.pos[1]);
                max_y = max_y.max(vertex.pos[1]);
            }
            let u0 = chunk.iter().map(|v| v.uv[0]).fold(f32::MAX, f32::min);
            let u1 = chunk.iter().map(|v| v.uv[0]).fold(f32::MIN, f32::max);
            let v0 = chunk.iter().map(|v| v.uv[1]).fold(f32::MAX, f32::min);
            let v1 = chunk.iter().map(|v| v.uv[1]).fold(f32::MIN, f32::max);
            let x0 = (u0 * font.atlas.width as f32).floor().max(0.0) as u32;
            let x1 = (u1 * font.atlas.width as f32).ceil() as u32;
            let y0 = (v0 * font.atlas.height as f32).floor().max(0.0) as u32;
            let y1 = (v1 * font.atlas.height as f32).ceil() as u32;
            for y in y0..y1.min(font.atlas.height) {
                for x in x0..x1.min(font.atlas.width) {
                    let index = ((y * font.atlas.width + x) * 4) as usize + 3;
                    if font.atlas.rgba.get(index).copied().unwrap_or(0) > 16 {
                        ink += 1;
                    }
                }
            }
        }
        assert!(ink > 200, "cyrillic run ink pixels {ink}");
        // `MIN_TEXT_PX` is the line box; the inked cap of a 30 px face is
        // ~0.6 em, so the rasterised run must clear the dense floor and the
        // line box must clear 24 px (asserted above).
        assert!(
            max_y - min_y >= MIN_DENSE_PX * 0.65,
            "inked run height {:.1} px",
            max_y - min_y
        );
        let native_height = (max_y - min_y) / scale;
        assert!(
            native_height >= MIN_DENSE_PX,
            "atlas glyph bitmap {native_height:.1} px < dense floor"
        );
        eprintln!(
            "hud.font.cyrillic-raster PASS ink={ink} run_height={:.1}px native_bitmap={native_height:.1}px min_px={MIN_TEXT_PX}",
            max_y - min_y
        );
    }
}
