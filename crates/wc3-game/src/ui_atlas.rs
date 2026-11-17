//! Chroma-keyed UI atlases: PNG/BLP decode, magenta keying, edge clamp and
//! nine-patch inset measurement.
//!
//! The game's UI art arrives in two shapes. Retail Warcraft III ships separate
//! BLPs (`HumanUITile01..04`, `CommandButtonBorder`, `ToolTipBorder`, ...) that
//! are drawn as full-quad stretches today, which squashes every ornament when a
//! frame changes size. A painter may also hand over one atlas sheet with the
//! elements arranged on a chroma-key background (the operator's fantasy atlas is
//! 1536x1024 with 41% magenta). Both are handled here:
//!
//! * [`Atlas::load`] reads a manifest (exact element pixel rects) plus the image,
//!   keys the background to alpha 0, dilates the surviving colour into the keyed
//!   pixels so linear filtering cannot bleed the key colour back in, and
//!   measures or accepts per-element corner insets.
//! * [`detect_insets`] is the generic measurement used when a texture carries no
//!   authored insets: it finds where the ornament stops being stationary along
//!   each axis, which is exactly the boundary between a fixed corner and a
//!   stretchable band.
//!
//! Nothing in this module hardcodes a pixel rectangle: rectangles come from the
//! manifest, insets from the sidecar or from measurement.
//!
//! Alpha mode: the UI pipeline is straight-alpha (`crates/wc3-render/src/ui.rs`
//! blends with `BlendState::ALPHA_BLENDING` and the fragment shader returns
//! `textureSample(...) * color`), so atlases feeding it must be keyed but *not*
//! premultiplied - a premultiplied sheet would darken every translucent texel a
//! second time. [`AlphaMode::Premultiplied`] exists for offscreen compositing and
//! is exercised by the tests; both modes pass through the same edge clamp.

use std::path::{Path, PathBuf};

use crate::hud::Rect;

/// Largest image dimension accepted from an atlas file.
///
/// A 1536x1024 sheet is 6.3 MiB of RGBA; the cap keeps a hostile or corrupt
/// header from allocating gigabytes.
pub const MAX_ATLAS_DIMENSION: u32 = 8192;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum AtlasError {
    /// The file could not be read.
    Io(String),
    /// The image container could not be decoded.
    Image(String),
    /// The manifest is not valid JSON or misses a required field.
    Manifest(String),
    /// The manifest declares a different size than the image it points at.
    SizeMismatch {
        manifest: (u32, u32),
        image: (u32, u32),
    },
    /// An element rectangle leaves the image.
    ElementOutOfBounds {
        index: usize,
        rect: [f32; 4],
        size: (u32, u32),
    },
    /// The sidecar names a different rectangle than the manifest.
    RectDrift {
        index: usize,
        manifest: [f32; 4],
        sidecar: [f32; 4],
    },
}

impl std::fmt::Display for AtlasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AtlasError::Io(message) => write!(f, "atlas io: {message}"),
            AtlasError::Image(message) => write!(f, "atlas image: {message}"),
            AtlasError::Manifest(message) => write!(f, "atlas manifest: {message}"),
            AtlasError::SizeMismatch { manifest, image } => write!(
                f,
                "atlas size mismatch: manifest {}x{} image {}x{}",
                manifest.0, manifest.1, image.0, image.1
            ),
            AtlasError::ElementOutOfBounds { index, rect, size } => write!(
                f,
                "atlas element {index} rect {rect:?} leaves the {}x{} image",
                size.0, size.1
            ),
            AtlasError::RectDrift {
                index,
                manifest,
                sidecar,
            } => write!(
                f,
                "atlas element {index} rect drift: manifest {manifest:?} sidecar {sidecar:?}"
            ),
        }
    }
}

impl std::error::Error for AtlasError {}

// ---------------------------------------------------------------------------
// Chroma key
// ---------------------------------------------------------------------------

/// A chroma key expressed as channel bounds (`>200,<80,>200` = magenta).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChromaKey {
    pub min_red: u8,
    pub max_green: u8,
    pub min_blue: u8,
}

impl ChromaKey {
    /// Magenta key used by the fantasy UI atlas.
    pub const MAGENTA: Self = Self {
        min_red: 200,
        max_green: 80,
        min_blue: 200,
    };

    pub fn is_key(&self, pixel: [u8; 4]) -> bool {
        // A pixel that is already transparent stays transparent: the key never
        // resurrects alpha, it only removes it.
        pixel[3] != 0
            && pixel[0] > self.min_red
            && pixel[1] < self.max_green
            && pixel[2] > self.min_blue
    }

    /// Parse `"magenta (>200,<80,>200)"` from the manifest.
    ///
    /// The numbers live in the manifest, not in this module: a differently
    /// keyed sheet (green screen, cyan) parses without a code change.
    pub fn parse(spec: &str) -> Option<Self> {
        let open = spec.find('(')?;
        let close = spec[open..].find(')')? + open;
        let mut numbers: Vec<u8> = Vec::with_capacity(3);
        for field in spec[open + 1..close].split(',') {
            let digits: String = field.chars().filter(|ch| ch.is_ascii_digit()).collect();
            numbers.push(digits.parse().ok()?);
        }
        if numbers.len() != 3 {
            return None;
        }
        Some(Self {
            min_red: numbers[0],
            max_green: numbers[1],
            min_blue: numbers[2],
        })
    }
}

/// How the keyed pixels are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaMode {
    /// Straight alpha: `rgb` keeps its colour, `a` carries the coverage. What
    /// the straight-alpha UI pipeline consumes.
    Straight,
    /// `rgb` multiplied by `a`, for offscreen compositing.
    Premultiplied,
}

// ---------------------------------------------------------------------------
// Image container
// ---------------------------------------------------------------------------

/// Tightly packed RGBA8 image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba8 {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Rgba8 {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: vec![0; width as usize * height as usize * 4],
        }
    }

    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, AtlasError> {
        let expected = width as usize * height as usize * 4;
        if rgba.len() != expected {
            return Err(AtlasError::Image(format!(
                "rgba length {} does not match {width}x{height} ({} bytes)",
                rgba.len(),
                expected
            )));
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    #[inline]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y as usize * self.width as usize) + x as usize) * 4;
        [
            self.rgba[offset],
            self.rgba[offset + 1],
            self.rgba[offset + 2],
            self.rgba[offset + 3],
        ]
    }

    #[inline]
    pub fn set_pixel(&mut self, x: u32, y: u32, pixel: [u8; 4]) {
        let offset = ((y as usize * self.width as usize) + x as usize) * 4;
        self.rgba[offset..offset + 4].copy_from_slice(&pixel);
    }

    /// Pixels the key still matches. The acceptance measure for the loader.
    pub fn keyed_pixels(&self, key: ChromaKey) -> usize {
        self.rgba
            .chunks_exact(4)
            .filter(|pixel| key.is_key([pixel[0], pixel[1], pixel[2], pixel[3]]))
            .count()
    }

    /// Convert to the renderer's texture payload.
    pub fn to_texture(&self) -> crate::assets::TextureImage {
        crate::assets::TextureImage {
            width: self.width,
            height: self.height,
            rgba: self.rgba.clone(),
        }
    }
}

/// Decode a PNG into RGBA8 (palette and 16-bit inputs are normalised).
pub fn decode_png(bytes: &[u8]) -> Result<Rgba8, AtlasError> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    // Palette -> RGB, tRNS -> alpha, 16-bit -> 8-bit, so every input shape
    // reaches the keyer as straight RGBA8.
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|error| AtlasError::Image(format!("png header: {error}")))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| AtlasError::Image("png: unreasonable output size".to_string()))?;
    let mut buffer = vec![0u8; size];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| AtlasError::Image(format!("png frame: {error}")))?;
    let (width, height) = (info.width, info.height);
    if width == 0 || height == 0 || width > MAX_ATLAS_DIMENSION || height > MAX_ATLAS_DIMENSION {
        return Err(AtlasError::Image(format!(
            "png dimensions {width}x{height} outside 1..={MAX_ATLAS_DIMENSION}"
        )));
    }
    let pixels = width as usize * height as usize;
    let mut rgba = Vec::with_capacity(pixels * 4);
    match info.color_type {
        png::ColorType::Rgba => rgba.extend_from_slice(&buffer[..pixels * 4]),
        png::ColorType::Rgb => {
            for pixel in buffer[..pixels * 3].chunks_exact(3) {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
        }
        png::ColorType::Grayscale => {
            for value in &buffer[..pixels] {
                rgba.extend_from_slice(&[*value, *value, *value, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for pixel in buffer[..pixels * 2].chunks_exact(2) {
                rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
        }
        png::ColorType::Indexed => {
            return Err(AtlasError::Image(
                "png: indexed output was not expanded".to_string(),
            ));
        }
    }
    Rgba8::from_rgba(width, height, rgba)
}

/// Decode a BLP (the shipped Warcraft III texture container).
pub fn decode_blp(bytes: &[u8]) -> Result<Rgba8, AtlasError> {
    let image = wc3_formats::blp::decode(bytes)
        .map_err(|error| AtlasError::Image(format!("blp: {error:?}")))?;
    if image.width == 0 || image.height == 0 {
        return Err(AtlasError::Image("blp: empty image".to_string()));
    }
    Rgba8::from_rgba(image.width, image.height, image.rgba)
}

/// Decode by magic: PNG signature, or `BLP1`/`BLP2`.
pub fn decode_image(bytes: &[u8]) -> Result<Rgba8, AtlasError> {
    const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if bytes.len() >= 8 && bytes[..8] == PNG_MAGIC {
        return decode_png(bytes);
    }
    if bytes.len() >= 4 && (&bytes[..4] == b"BLP1" || &bytes[..4] == b"BLP2") {
        return decode_blp(bytes);
    }
    Err(AtlasError::Image(
        "unrecognised image container (not PNG/BLP1/BLP2)".to_string(),
    ))
}

// ---------------------------------------------------------------------------
// Keying, edge clamp, premultiply
// ---------------------------------------------------------------------------

/// What [`key_chroma`] did, so a caller can report it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyReport {
    /// Pixels matching the manifest's key bounds exactly.
    pub keyed: usize,
    /// Anti-aliased key blend removed next to the keyed area.
    pub fringe: usize,
    /// Transparent texels that borrowed an art colour (edge clamp).
    pub dilated: usize,
}

/// Rings of anti-aliasing removed around the keyed area.
///
/// Three rings: the operator's sheet keeps three texels of magenta blend at a few
/// element tips (measured: 14 873 removed on ring 1, 217 on ring 2, 3 on ring 3),
/// and any of them left in place renders as a purple speck on the silhouette.
const FRINGE_RINGS: u32 = 3;

/// A magenta-dominant pixel: the blend an artist's edge produces between the art
/// and the key. Only pixels *adjacent to* the keyed area are treated as fringe,
/// so genuinely violet art far from the key survives.
fn is_fringe(pixel: [u8; 4]) -> bool {
    pixel[3] != 0
        && pixel[1] < 150
        && pixel[0] > pixel[1].saturating_add(48)
        && pixel[2] > pixel[1].saturating_add(48)
}

/// Replace the key colour with alpha 0 and clamp the edge colour.
///
/// Three steps, in order:
/// 1. every pixel matching the manifest's key bounds loses its alpha;
/// 2. the anti-aliased ring around the keyed area (a magenta-dominant blend) is
///    removed too - left in place, those texels render as a purple fringe on
///    every element edge;
/// 3. keyed texels receive the average colour of their opaque neighbours for
///    `clamp_passes` rings. That is what stops the key colour bleeding back
///    through bilinear filtering: the sampler has an art colour to interpolate
///    toward instead of magenta.
pub fn key_chroma(image: &mut Rgba8, key: ChromaKey, clamp_passes: u32) -> KeyReport {
    let mut report = KeyReport::default();
    for pixel in image.rgba.chunks_exact_mut(4) {
        if key.is_key([pixel[0], pixel[1], pixel[2], pixel[3]]) {
            pixel[3] = 0;
            report.keyed += 1;
        }
    }
    let (width, height) = (image.width as i64, image.height as i64);
    let mut transparent: Vec<bool> = image.rgba.chunks_exact(4).map(|p| p[3] == 0).collect();
    for _ in 0..FRINGE_RINGS {
        let mut removed = 0usize;
        let mut next = transparent.clone();
        for y in 0..height {
            for x in 0..width {
                let index = (y * width + x) as usize;
                if transparent[index] {
                    continue;
                }
                let offset = index * 4;
                let pixel = [
                    image.rgba[offset],
                    image.rgba[offset + 1],
                    image.rgba[offset + 2],
                    image.rgba[offset + 3],
                ];
                if !is_fringe(pixel) {
                    continue;
                }
                let touches = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| {
                    let nx = x + dx;
                    let ny = y + dy;
                    nx >= 0
                        && ny >= 0
                        && nx < width
                        && ny < height
                        && transparent[(ny * width + nx) as usize]
                });
                if !touches {
                    continue;
                }
                image.rgba[offset + 3] = 0;
                next[index] = true;
                removed += 1;
            }
        }
        transparent = next;
        report.fringe += removed;
        if removed == 0 {
            break;
        }
    }
    report.dilated = clamp_keyed_edges(image, clamp_passes);
    report
}

/// Dilate the colour of opaque texels into the transparent ring around them.
///
/// Each pass copies one ring: a transparent texel takes the mean colour of its
/// 4-neighbourhood where at least one neighbour already carries colour. Alpha is
/// never touched, so the silhouette is unchanged.
pub fn clamp_keyed_edges(image: &mut Rgba8, passes: u32) -> usize {
    let (width, height) = (image.width as i32, image.height as i32);
    if width <= 0 || height <= 0 {
        return 0;
    }
    // `filled` marks texels whose colour is valid; alpha 0 texels start unfilled.
    let mut filled: Vec<bool> = image.rgba.chunks_exact(4).map(|p| p[3] != 0).collect();
    let mut filled_count = 0usize;
    for _ in 0..passes {
        let mut next = filled.clone();
        let mut changed = 0usize;
        for y in 0..height {
            for x in 0..width {
                let index = (y * width + x) as usize;
                if filled[index] {
                    continue;
                }
                let mut sum = [0u32; 3];
                let mut count = 0u32;
                for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= width || ny >= height {
                        continue;
                    }
                    let neighbour = (ny * width + nx) as usize;
                    if !filled[neighbour] {
                        continue;
                    }
                    let offset = neighbour * 4;
                    sum[0] += image.rgba[offset] as u32;
                    sum[1] += image.rgba[offset + 1] as u32;
                    sum[2] += image.rgba[offset + 2] as u32;
                    count += 1;
                }
                if count == 0 {
                    continue;
                }
                let offset = index * 4;
                image.rgba[offset] = (sum[0] / count) as u8;
                image.rgba[offset + 1] = (sum[1] / count) as u8;
                image.rgba[offset + 2] = (sum[2] / count) as u8;
                // Alpha stays 0: only the colour is clamped.
                next[index] = true;
                changed += 1;
            }
        }
        filled = next;
        filled_count += changed;
        if changed == 0 {
            break;
        }
    }
    filled_count
}

/// Multiply RGB by alpha (or divide it back out).
pub fn apply_alpha_mode(image: &mut Rgba8, mode: AlphaMode) {
    match mode {
        AlphaMode::Straight => {}
        AlphaMode::Premultiplied => {
            for pixel in image.rgba.chunks_exact_mut(4) {
                let alpha = pixel[3] as u32;
                pixel[0] = ((pixel[0] as u32 * alpha + 127) / 255) as u8;
                pixel[1] = ((pixel[1] as u32 * alpha + 127) / 255) as u8;
                pixel[2] = ((pixel[2] as u32 * alpha + 127) / 255) as u8;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Inset measurement
// ---------------------------------------------------------------------------

/// Measure the corner insets of one element: `[left, top, right, bottom]` in
/// source pixels.
///
/// The stretchable part of a nine-patch is where the art is *stationary* along
/// the stretch axis; the corners are where it is not. For each axis this
/// computes the mean colour delta between neighbouring columns (rows) over the
/// perpendicular middle band - the band that a corner ornament leaves alone -
/// and walks inward from each edge until the profile stays flat for a few
/// samples. Keyed and transparent texels never count as a zero delta, otherwise
/// the chroma margin would look like part of the stretchable band.
pub fn detect_insets(image: &Rgba8, rect: Rect, key: ChromaKey) -> [f32; 4] {
    let x0 = rect.x.max(0.0).floor() as i64;
    let y0 = rect.y.max(0.0).floor() as i64;
    let w = rect.w.floor() as i64;
    let h = rect.h.floor() as i64;
    if w < 3 || h < 3 {
        return [0.0; 4];
    }
    let x1 = (x0 + w).min(image.width as i64);
    let y1 = (y0 + h).min(image.height as i64);
    let w = x1 - x0;
    let h = y1 - y0;
    if w < 3 || h < 3 {
        return [0.0; 4];
    }

    // Middle 60% band: corners live outside it, edge ornaments inside it.
    let band_y0 = y0 + (h as f32 * 0.2) as i64;
    let band_y1 = y0 + (h as f32 * 0.8) as i64;
    let band_x0 = x0 + (w as f32 * 0.2) as i64;
    let band_x1 = x0 + (w as f32 * 0.8) as i64;

    let mut columns = Vec::with_capacity(w as usize - 1);
    for x in x0..x1 - 1 {
        columns.push(column_delta(image, key, x, band_y0, band_y1));
    }
    let mut rows = Vec::with_capacity(h as usize - 1);
    for y in y0..y1 - 1 {
        rows.push(row_delta(image, key, y, band_x0, band_x1));
    }

    let left = first_flat(&columns, w as usize / 2);
    let right = first_flat_reversed(&columns, w as usize / 2);
    let top = first_flat(&rows, h as usize / 2);
    let bottom = first_flat_reversed(&rows, h as usize / 2);
    let mut insets = [
        left as f32,
        top as f32,
        right as f32,
        bottom as f32,
    ];
    // Keep at least one source texel for the stretchable middle, on both axes.
    let max_x = (w - 1).max(1) as f32;
    let max_y = (h - 1).max(1) as f32;
    if insets[0] + insets[2] > max_x {
        let half = (max_x * 0.5).floor();
        insets[0] = half;
        insets[2] = max_x - half;
    }
    if insets[1] + insets[3] > max_y {
        let half = (max_y * 0.5).floor();
        insets[1] = half;
        insets[3] = max_y - half;
    }
    insets
}

/// Mean absolute colour delta between column `x` and column `x + 1`, over rows
/// `y0..y1`. Returns `f32::MAX` when the pair is mostly key (margin, so never a
/// stretchable sample).
fn column_delta(image: &Rgba8, key: ChromaKey, x: i64, y0: i64, y1: i64) -> f32 {
    let mut total = 0.0f32;
    let mut valid = 0usize;
    let samples = (y1 - y0).max(0) as usize;
    for y in y0..y1 {
        if x < 0 || y < 0 || x + 1 >= image.width as i64 || y >= image.height as i64 {
            continue;
        }
        let a = image.pixel(x as u32, y as u32);
        let b = image.pixel(x as u32 + 1, y as u32);
        if (a[3] == 0 || key.is_key(a)) && (b[3] == 0 || key.is_key(b)) {
            continue;
        }
        total += (a[0] as f32 - b[0] as f32).abs()
            + (a[1] as f32 - b[1] as f32).abs()
            + (a[2] as f32 - b[2] as f32).abs();
        valid += 1;
    }
    if samples == 0 || valid * 4 < samples {
        return f32::MAX;
    }
    total / valid as f32
}

/// Mean absolute colour delta between row `y` and row `y + 1`, over columns
/// `x0..x1`.
fn row_delta(image: &Rgba8, key: ChromaKey, y: i64, x0: i64, x1: i64) -> f32 {
    let mut total = 0.0f32;
    let mut valid = 0usize;
    let samples = (x1 - x0).max(0) as usize;
    for x in x0..x1 {
        if x < 0 || y < 0 || x >= image.width as i64 || y + 1 >= image.height as i64 {
            continue;
        }
        let a = image.pixel(x as u32, y as u32);
        let b = image.pixel(x as u32, y as u32 + 1);
        if (a[3] == 0 || key.is_key(a)) && (b[3] == 0 || key.is_key(b)) {
            continue;
        }
        total += (a[0] as f32 - b[0] as f32).abs()
            + (a[1] as f32 - b[1] as f32).abs()
            + (a[2] as f32 - b[2] as f32).abs();
        valid += 1;
    }
    if samples == 0 || valid * 4 < samples {
        return f32::MAX;
    }
    total / valid as f32
}

fn median(values: &[f32]) -> f32 {
    let mut finite: Vec<f32> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return 0.0;
    }
    finite.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    finite[finite.len() / 2]
}

/// First index whose value starts a flat run of at least four samples.
fn first_flat(profile: &[f32], limit: usize) -> usize {
    let limit = limit.min(profile.len());
    let threshold = (median(&profile[..limit]) * 1.6).max(6.0);
    let mut index = 0;
    while index < limit {
        if profile[index] > threshold {
            index += 1;
            continue;
        }
        let mut run = 0;
        let mut cursor = index;
        while cursor < limit && profile[cursor] <= threshold {
            run += 1;
            cursor += 1;
        }
        if run >= 4 {
            return index;
        }
        index = cursor;
    }
    limit
}

/// Same as [`first_flat`], walking from the far edge inward.
fn first_flat_reversed(profile: &[f32], limit: usize) -> usize {
    let limit = limit.min(profile.len());
    let reversed: Vec<f32> = profile.iter().rev().copied().collect();
    first_flat(&reversed, limit)
}

// ---------------------------------------------------------------------------
// Manifest + atlas
// ---------------------------------------------------------------------------

/// What an element is for. Drives which nine-patch axis a consumer stretches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElementKind {
    /// Four-sided frame: stretch both axes.
    Panel,
    /// Horizontal strip: corner caps keep their width, the middle stretches.
    Bar,
    /// Command-button state art.
    Button,
    /// Ability/item slot.
    Slot,
    /// Stacked row list.
    Rows,
    /// Content (icon, fill, marker): never stretched, addressed by rect only.
    Content,
}

impl ElementKind {
    pub fn parse(text: &str) -> Self {
        match text.trim().to_ascii_lowercase().as_str() {
            "bar" => ElementKind::Bar,
            "button" => ElementKind::Button,
            "slot" => ElementKind::Slot,
            "rows" => ElementKind::Rows,
            "content" => ElementKind::Content,
            _ => ElementKind::Panel,
        }
    }

    /// Whether the right/left insets are honoured as fixed corners.
    pub fn stretches_horizontally(self) -> bool {
        !matches!(self, ElementKind::Content)
    }

    pub fn stretches_vertically(self) -> bool {
        matches!(
            self,
            ElementKind::Panel | ElementKind::Slot | ElementKind::Rows | ElementKind::Button
        )
    }
}

/// One named element of an atlas.
#[derive(Clone, Debug, PartialEq)]
pub struct AtlasElement {
    pub name: String,
    pub kind: ElementKind,
    /// Pixel rectangle inside the atlas.
    pub rect: Rect,
    /// `[left, top, right, bottom]` corner size in source pixels.
    pub insets: [f32; 4],
}

/// A loaded, keyed atlas.
#[derive(Clone, Debug)]
pub struct Atlas {
    pub image: Rgba8,
    pub key: ChromaKey,
    pub elements: Vec<AtlasElement>,
    pub source: String,
}

/// The manifest as written on disk (`analysis/h2/ui_atlas_manifest.json`).
#[derive(Clone, Debug)]
pub struct AtlasManifest {
    pub source: String,
    pub size: (u32, u32),
    pub key: ChromaKey,
    pub rects: Vec<Rect>,
}

/// Parse the element manifest. Only the fields the loader needs are read; extra
/// keys are ignored so a richer manifest stays valid.
pub fn parse_manifest(json: &str) -> Result<AtlasManifest, AtlasError> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|error| AtlasError::Manifest(format!("invalid json: {error}")))?;
    let object = value
        .as_object()
        .ok_or_else(|| AtlasError::Manifest("root is not an object".to_string()))?;
    let source = object
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or("atlas")
        .to_string();
    let size = object
        .get("size")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AtlasError::Manifest("missing size".to_string()))?;
    if size.len() != 2 {
        return Err(AtlasError::Manifest("size must be [w, h]".to_string()));
    }
    let width = size[0]
        .as_u64()
        .ok_or_else(|| AtlasError::Manifest("size[0] is not an integer".to_string()))?
        as u32;
    let height = size[1]
        .as_u64()
        .ok_or_else(|| AtlasError::Manifest("size[1] is not an integer".to_string()))?
        as u32;
    let key = object
        .get("chroma_key")
        .and_then(|v| v.as_str())
        .and_then(ChromaKey::parse)
        .ok_or_else(|| AtlasError::Manifest("missing or unparsable chroma_key".to_string()))?;
    let elements = object
        .get("elements")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AtlasError::Manifest("missing elements".to_string()))?;
    let mut rects = Vec::with_capacity(elements.len());
    for (index, element) in elements.iter().enumerate() {
        let rect = element
            .as_object()
            .ok_or_else(|| AtlasError::Manifest(format!("element {index} is not an object")))?;
        let field = |name: &str| -> Result<f32, AtlasError> {
            rect.get(name)
                .and_then(|v| v.as_f64())
                .map(|v| v as f32)
                .ok_or_else(|| {
                    AtlasError::Manifest(format!("element {index} missing {name}"))
                })
        };
        rects.push(Rect::new(
            field("x")?,
            field("y")?,
            field("w")?,
            field("h")?,
        ));
    }
    Ok(AtlasManifest {
        source,
        size: (width, height),
        key,
        rects,
    })
}

/// Authored element names, roles and insets (the sidecar file).
#[derive(Clone, Debug, Default)]
pub struct InsetsDoc {
    /// `(index, name, kind, insets)` in manifest order.
    pub entries: Vec<(usize, String, ElementKind, [f32; 4])>,
}

impl InsetsDoc {
    pub fn parse(json: &str) -> Result<Self, AtlasError> {
        let value: serde_json::Value = serde_json::from_str(json)
            .map_err(|error| AtlasError::Manifest(format!("sidecar json: {error}")))?;
        let elements = value
            .get("elements")
            .and_then(|v| v.as_array())
            .ok_or_else(|| AtlasError::Manifest("sidecar missing elements".to_string()))?;
        let mut entries = Vec::with_capacity(elements.len());
        for element in elements {
            let index = element
                .get("index")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| AtlasError::Manifest("sidecar element missing index".to_string()))?
                as usize;
            let name = element
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let kind = ElementKind::parse(
                element.get("kind").and_then(|v| v.as_str()).unwrap_or("panel"),
            );
            let insets = element
                .get("insets")
                .and_then(|v| v.as_array())
                .ok_or_else(|| {
                    AtlasError::Manifest(format!("sidecar element {index} missing insets"))
                })?;
            if insets.len() != 4 {
                return Err(AtlasError::Manifest(format!(
                    "sidecar element {index} insets must be [l, t, r, b]"
                )));
            }
            let mut values = [0.0f32; 4];
            for (slot, value) in values.iter_mut().zip(insets.iter()) {
                *slot = value.as_f64().unwrap_or(0.0) as f32;
            }
            entries.push((index, name, kind, values));
        }
        Ok(Self { entries })
    }

    /// Cross-check the authored rects against the manifest rects.
    ///
    /// The sidecar repeats the rect only so drift is detectable; the manifest
    /// stays the single source of truth.
    pub fn check_rects(&self, json: &str, rects: &[Rect]) -> Result<(), AtlasError> {
        let value: serde_json::Value = serde_json::from_str(json)
            .map_err(|error| AtlasError::Manifest(format!("sidecar json: {error}")))?;
        let elements = value
            .get("elements")
            .and_then(|v| v.as_array())
            .ok_or_else(|| AtlasError::Manifest("sidecar missing elements".to_string()))?;
        for element in elements {
            let index = element
                .get("index")
                .and_then(|v| v.as_u64())
                .unwrap_or(u64::MAX) as usize;
            let Some(expected) = rects.get(index) else {
                return Err(AtlasError::Manifest(format!(
                    "sidecar names element {index} but the manifest has {} elements",
                    rects.len()
                )));
            };
            let Some(rect) = element.get("rect").and_then(|v| v.as_array()) else {
                continue;
            };
            if rect.len() != 4 {
                continue;
            }
            let actual = [
                rect[0].as_f64().unwrap_or(0.0) as f32,
                rect[1].as_f64().unwrap_or(0.0) as f32,
                rect[2].as_f64().unwrap_or(0.0) as f32,
                rect[3].as_f64().unwrap_or(0.0) as f32,
            ];
            let manifest = [expected.x, expected.y, expected.w, expected.h];
            if actual != manifest {
                return Err(AtlasError::RectDrift {
                    index,
                    manifest,
                    sidecar: actual,
                });
            }
        }
        Ok(())
    }
}

impl Atlas {
    /// Build an atlas from the manifest JSON, the image bytes and an optional
    /// authored-insets sidecar.
    pub fn load(
        manifest_json: &str,
        image_bytes: &[u8],
        insets_json: Option<&str>,
        alpha: AlphaMode,
        clamp_passes: u32,
    ) -> Result<Self, AtlasError> {
        let manifest = parse_manifest(manifest_json)?;
        let mut image = decode_image(image_bytes)?;
        if (image.width, image.height) != manifest.size {
            return Err(AtlasError::SizeMismatch {
                manifest: manifest.size,
                image: (image.width, image.height),
            });
        }
        key_chroma(&mut image, manifest.key, clamp_passes);
        apply_alpha_mode(&mut image, alpha);

        let doc = match insets_json {
            Some(json) => {
                let doc = InsetsDoc::parse(json)?;
                doc.check_rects(json, &manifest.rects)?;
                Some(doc)
            }
            None => None,
        };

        let mut elements = Vec::with_capacity(manifest.rects.len());
        for (index, rect) in manifest.rects.iter().enumerate() {
            if rect.x < 0.0
                || rect.y < 0.0
                || rect.x + rect.w > image.width as f32
                || rect.y + rect.h > image.height as f32
            {
                return Err(AtlasError::ElementOutOfBounds {
                    index,
                    rect: [rect.x, rect.y, rect.w, rect.h],
                    size: (image.width, image.height),
                });
            }
            let authored = doc
                .as_ref()
                .and_then(|doc| doc.entries.iter().find(|entry| entry.0 == index));
            let element = match authored {
                Some((_, name, kind, insets)) => AtlasElement {
                    name: if name.is_empty() {
                        format!("element_{index:02}")
                    } else {
                        name.clone()
                    },
                    kind: *kind,
                    rect: *rect,
                    insets: *insets,
                },
                None => AtlasElement {
                    name: format!("element_{index:02}"),
                    kind: ElementKind::Panel,
                    rect: *rect,
                    insets: detect_insets(&image, *rect, manifest.key),
                },
            };
            elements.push(element);
        }

        Ok(Self {
            image,
            key: manifest.key,
            elements,
            source: manifest.source,
        })
    }

    /// Load from files: `<png>`, `<manifest>.json`, optional insets sidecar.
    pub fn load_files(
        image_path: &Path,
        manifest_path: &Path,
        insets_path: Option<&Path>,
        alpha: AlphaMode,
    ) -> Result<Self, AtlasError> {
        let image = std::fs::read(image_path)
            .map_err(|error| AtlasError::Io(format!("{}: {error}", image_path.display())))?;
        let manifest = std::fs::read_to_string(manifest_path).map_err(|error| {
            AtlasError::Io(format!("{}: {error}", manifest_path.display()))
        })?;
        let insets = match insets_path {
            Some(path) => Some(std::fs::read_to_string(path).map_err(|error| {
                AtlasError::Io(format!("{}: {error}", path.display()))
            })?),
            None => None,
        };
        Self::load(&manifest, &image, insets.as_deref(), alpha, 3)
    }

    pub fn element(&self, name: &str) -> Option<&AtlasElement> {
        self.elements
            .iter()
            .find(|element| element.name.eq_ignore_ascii_case(name))
    }

    pub fn element_index(&self, index: usize) -> Option<&AtlasElement> {
        self.elements.get(index)
    }

    /// Pixels still matching the chroma key (must be zero after loading).
    pub fn remaining_key_pixels(&self) -> usize {
        self.image.keyed_pixels(self.key)
    }

    /// Opaque texels that still look like the key's anti-aliased blend, measured
    /// next to the transparent area. Must be zero after loading.
    pub fn fringe_pixels(&self) -> usize {
        let image = &self.image;
        let (width, height) = (image.width as i64, image.height as i64);
        let mut count = 0usize;
        for y in 0..height {
            for x in 0..width {
                let pixel = image.pixel(x as u32, y as u32);
                if !is_fringe(pixel) {
                    continue;
                }
                let near_transparent = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| {
                    let nx = x + dx;
                    let ny = y + dy;
                    nx >= 0
                        && ny >= 0
                        && nx < width
                        && ny < height
                        && image.pixel(nx as u32, ny as u32)[3] == 0
                });
                if near_transparent {
                    count += 1;
                }
            }
        }
        count
    }
}

// ---------------------------------------------------------------------------
// Installed-atlas discovery
// ---------------------------------------------------------------------------

/// A resolved set of atlas files on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtlasFiles {
    pub image: PathBuf,
    pub manifest: PathBuf,
    pub insets: Option<PathBuf>,
}

/// Environment override: `<dir>` containing `ui_atlas.png`, `ui_atlas.json`,
/// `ui_atlas_insets.json`.
pub const ATLAS_ENV: &str = "WC3RUST_UI_ATLAS";

/// Directories searched for an installed atlas, most specific first.
///
/// These are data locations, not art constants: the Android app's private
/// files dir, its external files dir and the operator's download drop.
pub fn atlas_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os(ATLAS_ENV) {
        dirs.push(PathBuf::from(dir));
    }
    dirs.push(PathBuf::from("/data/data/org.wc3rust.p0/files"));
    dirs.push(PathBuf::from(
        "/storage/emulated/0/Android/data/org.wc3rust.p0/files",
    ));
    dirs.push(PathBuf::from("/storage/emulated/0/Download"));
    dirs
}

/// Resolve the installed atlas files, if any.
pub fn find_installed_atlas() -> Option<AtlasFiles> {
    for dir in atlas_search_dirs() {
        let image = dir.join("ui_atlas.png");
        let manifest = dir.join("ui_atlas.json");
        if !image.is_file() || !manifest.is_file() {
            continue;
        }
        let insets = dir.join("ui_atlas_insets.json");
        return Some(AtlasFiles {
            image,
            manifest,
            insets: insets.is_file().then_some(insets),
        });
    }
    None
}

/// Load the installed atlas in the straight-alpha mode the UI pipeline expects.
pub fn load_installed() -> Option<(Atlas, AtlasFiles)> {
    let files = find_installed_atlas()?;
    match Atlas::load_files(&files.image, &files.manifest, files.insets.as_deref(), AlphaMode::Straight)
    {
        Ok(atlas) => Some((atlas, files)),
        Err(error) => {
            crate::dev_log!("GAME_UI_ATLAS_FAIL {error}");
            None
        }
    }
}
