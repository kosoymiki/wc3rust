//! The operator's FINAL HUD asset pack: manifest-driven loader and 9-slice
//! primitives.
//!
//! Source of truth is `analysis/hudskin/final/wc3rust-hud-final/manifest.json`
//! (92 assets, per-asset `w`/`h`/`insets`/`states`, the 2048x2048
//! `commandbuttons` atlas, command-card 4x3 of 64 px cells with a 4 px gap,
//! ten 44 px control groups, the 1080p safe areas, the 768 design height and
//! the five font files).
//!
//! Contract:
//!
//! * every PNG listed in the manifest is decoded and registered in
//!   `Assets::textures`; a missing or mis-sized file fails the load with a
//!   `HUD_PACK missing=<file>` / `HUD_PACK size=...` error - the HUD never
//!   substitutes art silently;
//! * `rect(name)` is the whole texture (the pack ships one PNG per element),
//!   `insets(name)` are the manifest values in source pixels, `atlas_cell(id)`
//!   is a 64x64 cell of the `commandbuttons` sheet;
//! * `nine()` keeps the corner and edge cells at `inset * art_scale` and
//!   stretches only the centre cell; `tile_x`/`tile_center_x` repeat the art
//!   instead of stretching it (console bands, topbar plate);
//! * the panel textures are decoded as straight-alpha sRGB and must be sampled
//!   without mipmaps (`mips: false` in the manifest); the renderer's UI sampler
//!   is the nearest/no-mip path, see [`HudAsset::mips`].
//!
//! No pixel offset in this module is hand-tuned: every number is either a
//! manifest value or `manifest value * design scale`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use glam::Vec2;

use crate::assets::TextureImage;
use crate::hud::{Rect, UiBatch};

/// Environment override: a directory containing `manifest.json`.
pub const PACK_ENV: &str = "WC3RUST_HUD_PACK";
/// Directory name used by the operator's drop and the repo's analysis tree.
pub const PACK_DIR_NAME: &str = "wc3rust-hud-final";

/// One manifest asset (one PNG = one texture).
#[derive(Clone, Debug, PartialEq)]
pub struct HudAsset {
    pub name: String,
    pub file: String,
    pub w: u32,
    pub h: u32,
    /// Manifest 9-slice insets, source pixels `[left, top, right, bottom]`.
    pub insets: [f32; 4],
    pub states: Vec<String>,
    pub atlas: String,
    /// Manifest `mips` flag: panel slices are `false` (never mipmapped).
    pub mips: bool,
    /// Index into `Assets::textures`.
    pub texture: usize,
}

impl HudAsset {
    pub fn size(&self) -> [f32; 2] {
        [self.w as f32, self.h as f32]
    }

    pub fn rect(&self) -> [f32; 4] {
        [0.0, 0.0, self.w as f32, self.h as f32]
    }

    /// A 9-slice needs `l + r < w` and `t + b < h`; anything else has no
    /// stretchable cell and must be drawn as one image.
    pub fn insets_sliceable(&self) -> bool {
        let [l, t, r, b] = self.insets;
        l >= 0.0
            && t >= 0.0
            && r >= 0.0
            && b >= 0.0
            && l + r < self.w as f32
            && t + b < self.h as f32
    }

    /// Assets that are single images by nature: undersized UI frames (icons,
    /// arrows, strips, the minimap header) are drawn 1:1/uniform, never
    /// sliced, even when the manifest carries panel insets for them.
    pub fn single_image(&self) -> bool {
        let name = self.name.as_str();
        name.starts_with("icon_")
            || name.starts_with("tooltip_arrow_")
            || name.starts_with("upkeep_strip_")
            || name.starts_with("minimap_header_")
            || (self.atlas == "ui_frames"
                && (self.w < 40 || self.h < 40)
                && !name.starts_with("topbar_plate_"))
    }

    /// Effective slicing decision: harvest the manifest insets only when both
    /// the geometry guard, the single-image rule and a non-zero inset allow it.
    /// Zero-inset assets are one image by definition (there is no corner to
    /// preserve), so they are not counted as sliced.
    pub fn sliced(&self) -> bool {
        let [l, t, r, b] = self.insets;
        self.insets_sliceable()
            && !self.single_image()
            && (l > 0.0 || t > 0.0 || r > 0.0 || b > 0.0)
    }
}

/// One cell of the `commandbuttons` atlas.
#[derive(Clone, Debug, PartialEq)]
pub struct AtlasCell {
    pub id: String,
    pub texture: usize,
    pub rect: [f32; 4],
}

/// Loader error. `Display` starts with `HUD_PACK ` so a device log line is
/// greppable and a missing file is never mistaken for a successful fallback.
#[derive(Clone, Debug, PartialEq)]
pub enum PackError {
    Io { path: String, error: String },
    Manifest(String),
    Missing { file: String },
    Size { file: String, manifest: [u32; 2], actual: [u32; 2] },
    Texture { file: String, error: String },
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, error } => write!(f, "HUD_PACK io={path} error={error}"),
            Self::Manifest(error) => write!(f, "HUD_PACK manifest_error={error}"),
            Self::Missing { file } => write!(f, "HUD_PACK missing={file}"),
            Self::Size {
                file,
                manifest,
                actual,
            } => write!(
                f,
                "HUD_PACK size={file} manifest={}x{} actual={}x{}",
                manifest[0], manifest[1], actual[0], actual[1]
            ),
            Self::Texture { file, error } => write!(f, "HUD_PACK texture={file} error={error}"),
        }
    }
}

impl std::error::Error for PackError {}

/// The loaded pack. Holds metadata and `Assets::textures` indices; the pixels
/// stay in the asset texture list (a clone of the pack is cheap).
#[derive(Clone, Debug)]
pub struct HudPack {
    pub root: PathBuf,
    pub design_height: f32,
    pub reference_landscape: [u32; 2],
    pub reference_portrait: [u32; 2],
    /// `[left, top, right, bottom]` at the 1080p reference.
    pub safe_landscape: [f32; 4],
    pub safe_portrait: [f32; 4],
    pub card_columns: usize,
    pub card_rows: usize,
    pub card_cell: f32,
    pub card_gap: f32,
    pub group_count: usize,
    pub group_cell: f32,
    pub atlas_columns: u32,
    pub atlas_rows: u32,
    pub atlas_cell_px: f32,
    pub assets: Vec<HudAsset>,
    by_name: HashMap<String, usize>,
    cells: HashMap<String, AtlasCell>,
    pub atlas_texture: Option<usize>,
    pub font_files: Vec<String>,
    /// `Assets::textures` indices of the primary / serif / sans / numeric
    /// glyph atlases (`HudFonts::bind`). Primary is the operator's Nimrod MT
    /// read from the installed archive (or the pack serif when absent).
    pub font_textures: [usize; 4],
    /// Flat column of the topbar plate (U in 0..1), measured from the asset at
    /// load. The bar is extended by stretching this one-pixel column, so the
    /// authored spear/diamond ornaments are never stretched.
    pub topbar_flat_u: f32,
    /// Pack-authored stone colour (average of the console mid art). Fills the
    /// console band before the section 9-slices so a transparent edge never
    /// shows the world through the bar.
    pub console_base: [f32; 4],
    /// Pack-authored bar colour (the measured flat topbar column).
    pub topbar_base: [f32; 4],
}

/// Directories searched for the pack, most specific first.
///
/// These are data locations (the app's private files dir, the operator's
/// Download drop, the repo for headless proofs), never art constants.
pub fn pack_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os(PACK_ENV) {
        dirs.push(PathBuf::from(dir));
    }
    dirs.push(PathBuf::from("/data/data/org.wc3rust.p0/files/hud_pack"));
    dirs.push(PathBuf::from(format!(
        "/data/data/org.wc3rust.p0/files/{PACK_DIR_NAME}"
    )));
    dirs.push(PathBuf::from(
        "/storage/emulated/0/Android/data/org.wc3rust.p0/files/hud_pack",
    ));
    dirs.push(PathBuf::from(format!(
        "/storage/emulated/0/Download/{PACK_DIR_NAME}"
    )));
    dirs.push(PathBuf::from("/storage/emulated/0/Download/hud_pack"));
    if let Some(repo) = option_env!("CARGO_MANIFEST_DIR") {
        dirs.push(
            Path::new(repo)
                .join("../..")
                .join("analysis/hudskin/final")
                .join(PACK_DIR_NAME),
        );
    }
    dirs
}

/// First directory that actually carries a `manifest.json`.
pub fn find_installed() -> Option<PathBuf> {
    pack_search_dirs()
        .into_iter()
        .find(|dir| dir.join("manifest.json").is_file())
}

/// Resolve a manifest-controlled file strictly inside the pack. Reject absolute,
/// parent/current components and symlinks (including aliases that stay inside).
fn pack_file(root: &Path, relative: &str, label: &str) -> Result<PathBuf, PackError> {
    let rel = Path::new(relative);
    if relative.is_empty()
        || relative.contains('\\')
        || relative.contains('\0')
        || rel.is_absolute()
        || rel.components().any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(PackError::Manifest(format!("unsafe {label} path: {relative:?}")));
    }
    if root.is_symlink() {
        return Err(PackError::Manifest("pack root must not be a symlink".to_string()));
    }
    let base = root.canonicalize().map_err(|error| PackError::Io {
        path: root.display().to_string(),
        error: error.to_string(),
    })?;
    let mut cursor = root.to_path_buf();
    for component in rel.components() {
        if let std::path::Component::Normal(part) = component {
            cursor.push(part);
            if cursor.is_symlink() {
                return Err(PackError::Manifest(format!("symlink {label} is not allowed: {relative}")));
            }
        }
    }
    let candidate = root.join(rel);
    if !candidate.exists() {
        return Err(PackError::Missing { file: relative.to_string() });
    }
    let resolved = candidate.canonicalize().map_err(|error| PackError::Io {
        path: candidate.display().to_string(),
        error: error.to_string(),
    })?;
    if !resolved.starts_with(&base) || !resolved.is_file() {
        return Err(PackError::Manifest(format!("{label} escapes pack or is not a file: {relative}")));
    }
    Ok(resolved)
}

fn validate_atlas_entries(
    entries: &[serde_json::Value],
    width: u32,
    height: u32,
) -> Result<(), PackError> {
    use std::collections::HashSet;
    let mut ids = HashSet::with_capacity(entries.len());
    let mut rects: Vec<(String, u32, u32, u32, u32)> = Vec::with_capacity(entries.len());
    for entry in entries {
        let id = string(entry, &["id"])?;
        if id.trim().is_empty() || !ids.insert(id.clone()) {
            return Err(PackError::Manifest(format!("empty or duplicate atlas cell id: {id:?}")));
        }
        let mut values = [0u32; 4];
        for (index, key) in ["x", "y", "w", "h"].iter().enumerate() {
            let value = integer(entry, &[*key])?;
            if value < 0 || value > u32::MAX as i64 {
                return Err(PackError::Manifest(format!("atlas cell {id}: {key} outside u32 range")));
            }
            values[index] = value as u32;
        }
        let [x, y, w, h] = values;
        if w == 0 || h == 0 || x.checked_add(w).map_or(true, |right| right > width)
            || y.checked_add(h).map_or(true, |bottom| bottom > height)
        {
            return Err(PackError::Manifest(format!(
                "atlas cell {id} outside {width}x{height} atlas: {x},{y},{w},{h}"
            )));
        }
        for (other_id, ox, oy, ow, oh) in &rects {
            let overlaps = x < *ox + *ow && *ox < x + w && y < *oy + *oh && *oy < y + h;
            if overlaps {
                return Err(PackError::Manifest(format!(
                    "atlas cells {other_id} and {id} overlap"
                )));
            }
        }
        rects.push((id, x, y, w, h));
    }
    if rects.is_empty() {
        return Err(PackError::Manifest("atlas.commandbuttons.entries is empty".to_string()));
    }
    Ok(())
}

impl HudPack {
    /// Load the manifest and every PNG listed in it into `textures`.
    ///
    /// Existing textures are untouched: the pack appends and records the
    /// returned indices.
    pub fn load(root: &Path, textures: &mut Vec<TextureImage>) -> Result<Self, PackError> {
        let manifest_path = pack_file(root, "manifest.json", "manifest")?;
        let text = std::fs::read_to_string(&manifest_path).map_err(|error| PackError::Io {
            path: manifest_path.display().to_string(),
            error: error.to_string(),
        })?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| PackError::Manifest(error.to_string()))?;

        let design_height = number(&value, &["design_height"])?;
        if design_height <= 0.0 {
            return Err(PackError::Manifest("design_height must be positive".to_string()));
        }
        let reference = &value["reference_resolution"];
        let reference_landscape = pair_u32(reference, &["landscape"])?;
        let reference_portrait = pair_u32(reference, &["portrait"])?;
        if reference_landscape[0] <= reference_landscape[1] || reference_portrait[0] >= reference_portrait[1] {
            return Err(PackError::Manifest("reference resolutions must be landscape then portrait".to_string()));
        }
        let safe = &value["safe_area_1080p"];
        let safe_landscape = quad(safe, &["landscape"])?;
        let safe_portrait = quad(safe, &["portrait"])?;
        if safe_landscape[0] + safe_landscape[2] >= reference_landscape[0] as f32
            || safe_landscape[1] + safe_landscape[3] >= reference_landscape[1] as f32
            || safe_portrait[0] + safe_portrait[2] >= reference_portrait[0] as f32
            || safe_portrait[1] + safe_portrait[3] >= reference_portrait[1] as f32
        {
            return Err(PackError::Manifest("safe-area insets consume the reference viewport".to_string()));
        }

        let card = &value["command_card"];
        let card_columns = positive_usize(card, &["columns"])?;
        let card_rows = positive_usize(card, &["rows"])?;
        let card_cell_size = pair_u32(card, &["cell"])?;
        if card_cell_size[0] != card_cell_size[1] {
            return Err(PackError::Manifest("command_card.cell must be square".to_string()));
        }
        let card_cell = card_cell_size[0] as f32;
        let card_gap = number(card, &["gap"])?;
        if card_gap < 0.0 { return Err(PackError::Manifest("command_card.gap must be non-negative".to_string())); }

        let groups = &value["control_groups"];
        let group_count = positive_usize(groups, &["count"])?;
        let group_cell_size = pair_u32(groups, &["cell"])?;
        if group_cell_size[0] != group_cell_size[1] {
            return Err(PackError::Manifest("control_groups.cell must be square".to_string()));
        }
        let group_cell = group_cell_size[0] as f32;

        let atlas = &value["atlas"]["commandbuttons"];
        let atlas_columns = u32::try_from(positive_usize(atlas, &["columns"])?).map_err(|_| PackError::Manifest("atlas columns exceed u32".to_string()))?;
        let atlas_rows = u32::try_from(positive_usize(atlas, &["rows"])?).map_err(|_| PackError::Manifest("atlas rows exceed u32".to_string()))?;
        let atlas_cell_size = pair_u32(atlas, &["cell"])?;
        if atlas_cell_size[0] != atlas_cell_size[1] {
            return Err(PackError::Manifest("atlas.commandbuttons.cell must be square".to_string()));
        }
        let atlas_cell_px = atlas_cell_size[0] as f32;

        let font_values = value["font_files"].as_array()
            .ok_or_else(|| PackError::Manifest("font_files is not an array".to_string()))?;
        if font_values.len() != 5 {
            return Err(PackError::Manifest(format!("font_files must contain five entries, found {}", font_values.len())));
        }
        let mut font_files = Vec::with_capacity(font_values.len());
        let mut seen_fonts = std::collections::HashSet::new();
        for item in font_values {
            let font = item.as_str().ok_or_else(|| PackError::Manifest("font filename must be a string".to_string()))?;
            if font.contains('\\')
                || font.contains('\0')
                || Path::new(font).components().count() != 1
                || !matches!(Path::new(font).components().next(), Some(std::path::Component::Normal(_)))
                || !font.to_ascii_lowercase().ends_with(".ttf")
                || !seen_fonts.insert(font.to_string())
            {
                return Err(PackError::Manifest(format!("unsafe or duplicate font filename: {font:?}")));
            }
            let relative = format!("fonts/{font}");
            pack_file(root, &relative, "font")?;
            font_files.push(font.to_string());
        }

        let declarations = value["assets"]
            .as_array()
            .ok_or_else(|| PackError::Manifest("assets is not an array".to_string()))?;
        if declarations.is_empty() {
            return Err(PackError::Manifest("assets is empty".to_string()));
        }

        let texture_base = textures.len();
        // Decode transactionally: a malformed late asset must not leave the
        // caller's shared texture registry half-populated.
        let mut loaded_textures: Vec<TextureImage> = Vec::with_capacity(declarations.len() + 4);
        let mut assets = Vec::with_capacity(declarations.len());
        let mut by_name = HashMap::with_capacity(declarations.len());
        for declaration in declarations {
            let name = string(declaration, &["name"])?;
            let file = string(declaration, &["file"])?;
            if name.trim().is_empty() {
                return Err(PackError::Manifest("asset name must not be empty".to_string()));
            }
            let dimension = |key: &str| -> Result<u32, PackError> {
                let raw = integer(declaration, &[key])?;
                if raw <= 0 {
                    return Err(PackError::Manifest(format!("{name}: {key} must be positive")));
                }
                u32::try_from(raw).map_err(|_| PackError::Manifest(format!("{name}: {key} exceeds u32")))
            };
            let w = dimension("w")?;
            let h = dimension("h")?;
            let insets = &declaration["insets"];
            let insets = [
                number(insets, &["l"])?,
                number(insets, &["t"])?,
                number(insets, &["r"])?,
                number(insets, &["b"])?,
            ];
            let states = declaration["states"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let atlas = declaration["atlas"].as_str().unwrap_or("standalone").to_string();
            let mips = declaration["mips"].as_bool().unwrap_or(false);

            if w == 0 || h == 0 || (w as u64) * (h as u64) > 64_000_000 {
                return Err(PackError::Manifest(format!("{name}: invalid or excessive dimensions {w}x{h}")));
            }
            if by_name.contains_key(&name) || assets.iter().any(|asset: &HudAsset| asset.file == file) {
                return Err(PackError::Manifest(format!("duplicate asset name or file: {name} ({file})")));
            }
            if insets.iter().any(|value| !value.is_finite() || *value < 0.0) {
                return Err(PackError::Manifest(format!("{name}: insets must be finite and non-negative")));
            }
            let path = pack_file(root, &file, "asset")?;
            let bytes = std::fs::read(&path).map_err(|error| PackError::Io {
                path: path.display().to_string(),
                error: error.to_string(),
            })?;
            let image = crate::ui_atlas::decode_png(&bytes).map_err(|error| PackError::Texture {
                file: file.clone(),
                error: error.to_string(),
            })?;
            if image.width != w || image.height != h {
                return Err(PackError::Size {
                    file: file.clone(),
                    manifest: [w, h],
                    actual: [image.width, image.height],
                });
            }
            let texture = texture_base + loaded_textures.len();
            loaded_textures.push(image.to_texture());
            by_name.insert(name.clone(), assets.len());
            assets.push(HudAsset {
                name,
                file,
                w,
                h,
                insets,
                states,
                atlas,
                mips,
                texture,
            });
        }

        // Manifest audit: name every asset whose declared insets cannot be
        // sliced, once, at load. These are known pack defects (11 rows in the
        // 2026-11-16 delivery); the draw path renders them as single images.
        for asset in &assets {
            if !asset.insets_sliceable() {
                log_insets_ignored(asset);
            }
        }

        // Measure the topbar plate's flat column (min luminance variance over
        // the middle 60% of the width). Stretching this 1 px column extends the
        // bar without touching the authored ornaments at ~10%/~90%.
        let mut topbar_flat_u = 0.5;
        if let Some(asset) = assets
            .iter()
            .find(|asset| asset.name == "topbar_plate_normal_720x32")
            && let Some(texture) = loaded_textures.get(asset.texture - texture_base)
            && texture.width > 8
            && texture.height > 4
        {
            let width = texture.width;
            let height = texture.height;
            let mut best = (f32::MAX, width / 2);
            for x in (width / 5)..(width * 4 / 5) {
                let mut mean = 0.0f32;
                let mut count = 0.0f32;
                for y in 2..height - 2 {
                    let index = ((y * width + x) * 4) as usize;
                    let lum = 0.2126 * texture.rgba[index] as f32
                        + 0.7152 * texture.rgba[index + 1] as f32
                        + 0.0722 * texture.rgba[index + 2] as f32;
                    mean += lum;
                    count += 1.0;
                }
                mean /= count.max(1.0);
                let mut variance = 0.0f32;
                for y in 2..height - 2 {
                    let index = ((y * width + x) * 4) as usize;
                    let lum = 0.2126 * texture.rgba[index] as f32
                        + 0.7152 * texture.rgba[index + 1] as f32
                        + 0.0722 * texture.rgba[index + 2] as f32;
                    variance += (lum - mean) * (lum - mean);
                }
                if variance < best.0 {
                    best = (variance, x);
                }
            }
            topbar_flat_u = (best.1 as f32 + 0.5) / width as f32;
        }
        // Pack-authored base colours for the two edge-anchored bars: the
        // centre of the console mid art and the measured flat topbar column.
        let mut console_base = [0.09f32, 0.085, 0.075, 1.0];
        if let Some(asset) = assets
            .iter()
            .find(|asset| asset.name == "console_left_mid_normal_256x288")
            && let Some(texture) = loaded_textures.get(asset.texture - texture_base)
            && texture.width > 8
            && texture.height > 8
        {
            let (cx, cy) = (texture.width / 2, texture.height / 2);
            let mut sum = [0.0f32; 3];
            let mut count = 0.0f32;
            for y in (cy.saturating_sub(8))..(cy + 8).min(texture.height) {
                for x in (cx.saturating_sub(8))..(cx + 8).min(texture.width) {
                    let index = ((y * texture.width + x) * 4) as usize;
                    sum[0] += texture.rgba[index] as f32 / 255.0;
                    sum[1] += texture.rgba[index + 1] as f32 / 255.0;
                    sum[2] += texture.rgba[index + 2] as f32 / 255.0;
                    count += 1.0;
                }
            }
            if count > 0.0 {
                console_base = [sum[0] / count, sum[1] / count, sum[2] / count, 1.0];
            }
        }
        let mut topbar_base = [0.08f32, 0.075, 0.065, 1.0];
        if let Some(asset) = assets
            .iter()
            .find(|asset| asset.name == "topbar_plate_normal_720x32")
            && let Some(texture) = loaded_textures.get(asset.texture - texture_base)
            && texture.width > 0
            && texture.height > 0
        {
            let x = ((topbar_flat_u * texture.width as f32) as u32)
                .min(texture.width.saturating_sub(1));
            let mut sum = [0.0f32; 3];
            let mut count = 0.0f32;
            for y in 0..texture.height {
                let index = ((y * texture.width + x) * 4) as usize;
                sum[0] += texture.rgba[index] as f32 / 255.0;
                sum[1] += texture.rgba[index + 1] as f32 / 255.0;
                sum[2] += texture.rgba[index + 2] as f32 / 255.0;
                count += 1.0;
            }
            if count > 0.0 {
                topbar_base = [sum[0] / count, sum[1] / count, sum[2] / count, 1.0];
            }
        }

        // The commandbuttons sheet is one of the manifest assets; its cells are
        // addressed by id (`atlas_cell`) without another texture upload.
        let atlas_file = string(atlas, &["file"])?;
        let atlas_asset = assets.iter().find(|asset| asset.file == atlas_file)
            .ok_or_else(|| PackError::Manifest(format!("commandbuttons atlas file is not a declared asset: {atlas_file}")))?;
        let declared_atlas_size = pair_u32(atlas, &["size"])?;
        if declared_atlas_size != [atlas_asset.w, atlas_asset.h] {
            return Err(PackError::Manifest(format!(
                "commandbuttons atlas declared size {}x{} differs from asset {}x{}",
                declared_atlas_size[0], declared_atlas_size[1], atlas_asset.w, atlas_asset.h
            )));
        }
        let atlas_texture = Some(atlas_asset.texture);
        let entries = atlas["entries"].as_array()
            .ok_or_else(|| PackError::Manifest("atlas.commandbuttons.entries is not an array".to_string()))?;
        validate_atlas_entries(entries, atlas_asset.w, atlas_asset.h)?;
        let mut cells = HashMap::with_capacity(entries.len());
        for entry in entries {
            let id = string(entry, &["id"])?;
            let x = number(entry, &["x"])?;
            let y = number(entry, &["y"])?;
            let w = number(entry, &["w"])?;
            let h = number(entry, &["h"])?;
            cells.insert(id.clone(), AtlasCell {
                id, texture: atlas_asset.texture, rect: [x, y, w, h],
            });
        }
        if cells.is_empty() {
            return Err(PackError::Manifest(
                "atlas.commandbuttons.entries is empty".to_string(),
            ));
        }

        // The three authored faces: rasterise once per process, register
        // their glyph atlases as textures, and bind them so one HUD batch can
        // carry serif (names/tooltips), sans (actions) and mono (numbers).
        let mut font_textures = [0usize; 4];
        if let Some(fonts) = crate::hud::pack_fonts(root) {
            let base = texture_base + loaded_textures.len();
            loaded_textures.push(fonts.primary.atlas.clone());
            loaded_textures.push(fonts.serif.atlas.clone());
            loaded_textures.push(fonts.sans.atlas.clone());
            loaded_textures.push(fonts.numeric.atlas.clone());
            font_textures = [base, base + 1, base + 2, base + 3];
            fonts.bind(font_textures);
        } else {
            crate::dev_log!("HUD_PACK fonts=missing root={}", root.display());
        }

        textures.extend(loaded_textures);
        Ok(Self {
            root: root.to_path_buf(),
            design_height,
            reference_landscape,
            reference_portrait,
            safe_landscape,
            safe_portrait,
            card_columns,
            card_rows,
            card_cell,
            card_gap,
            group_count,
            group_cell,
            atlas_columns,
            atlas_rows,
            atlas_cell_px,
            assets,
            by_name,
            cells,
            atlas_texture,
            font_files,
            font_textures,
            topbar_flat_u,
            console_base,
            topbar_base,
        })
    }

    pub fn asset(&self, name: &str) -> Option<&HudAsset> {
        self.by_name.get(name).map(|index| &self.assets[*index])
    }

    pub fn texture(&self, name: &str) -> Option<usize> {
        self.asset(name).map(|asset| asset.texture)
    }

    pub fn rect(&self, name: &str) -> Option<[f32; 4]> {
        self.asset(name).map(HudAsset::rect)
    }

    pub fn insets(&self, name: &str) -> Option<[f32; 4]> {
        self.asset(name).map(|asset| asset.insets)
    }

    pub fn atlas_cell(&self, id: &str) -> Option<&AtlasCell> {
        self.cells.get(id)
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Absolute path of a bundled font face.
    pub fn font_path(&self, file: &str) -> PathBuf {
        self.root.join("fonts").join(file)
    }

    pub fn font_file_named(&self, needle: &str) -> Option<&str> {
        self.font_files
            .iter()
            .map(String::as_str)
            .find(|file| file.contains(needle))
    }

    /// Design scale for a viewport.
    ///
    /// Landscape maps the 768 design height to the screen height (2414x1080 ->
    /// 1.40625, the manifest's own example). The portrait console is a stacked
    /// column bounded by the 1080-wide reference, so it maps the reference
    /// width instead; both ideas come from `manifest.json`
    /// (`design_height`, `reference_resolution`).
    pub fn scale_for(&self, viewport: Vec2, portrait: bool) -> f32 {
        let raw = if portrait {
            viewport.x / self.reference_portrait[0].max(1) as f32
        } else {
            viewport.y / self.design_height.max(1.0)
        };
        raw.clamp(0.62, 2.0)
    }

    /// Safe insets `[l, t, r, b]` at the design scale.
    pub fn safe_area(&self, portrait: bool, scale: f32) -> [f32; 4] {
        let source = if portrait {
            self.safe_portrait
        } else {
            self.safe_landscape
        };
        source.map(|value| value * scale)
    }

    /// Emit `name` stretched to `rect` (single quad).
    pub fn stretch(&self, ui: &mut UiBatch, name: &str, rect: Rect, tint: [f32; 4]) -> bool {
        let Some(asset) = self.asset(name) else {
            crate::dev_log!("HUD_PACK lookup_missing={name}");
            return false;
        };
        if rect.w <= 0.0 || rect.h <= 0.0 {
            return false;
        }
        ui.textured_quad(asset.texture, rect, [0.0, 0.0, 1.0, 1.0], tint);
        true
    }

    /// 9-slice with the manifest insets. Corners and edge strips keep
    /// `inset * art_scale` source pixels; only the centre cell stretches.
    pub fn nine(
        &self,
        ui: &mut UiBatch,
        name: &str,
        rect: Rect,
        tint: [f32; 4],
        art_scale: f32,
    ) -> bool {
        let Some(asset) = self.asset(name) else {
            crate::dev_log!("HUD_PACK lookup_missing={name}");
            return false;
        };
        // Manifest defect guard: an inset set with no stretchable cell (for
        // example 24/24/24/24 on a 216x20 header) cannot be sliced. Such an
        // asset is drawn as one image at the requested size and the defect is
        // logged once; the art and the manifest are never rewritten.
        if !asset.sliced() {
            // Only a geometry guard failure is a manifest defect; assets that
            // are single images by nature (icons, strips, the resource pill)
            // simply stretch without a log.
            if !asset.insets_sliceable() {
                log_insets_ignored(asset);
            }
            return self.stretch(ui, name, rect, tint);
        }
        let [aw, ah] = asset.size();
        let scale = if art_scale.is_finite() {
            art_scale.max(0.01)
        } else {
            1.0
        };
        let [l, t, r, b] = asset.insets.map(|inset| inset * scale);
        if rect.w <= l + r + 1.0 || rect.h <= t + b + 1.0 {
            return self.stretch(ui, name, rect, tint);
        }
        let [al, at, ar, ab] = asset.insets;
        let sx = [0.0, al / aw, (aw - ar).max(al) / aw, 1.0];
        let sy = [0.0, at / ah, (ah - ab).max(at) / ah, 1.0];
        let dx = [rect.x, rect.x + l, rect.right() - r, rect.right()];
        let dy = [rect.y, rect.y + t, rect.bottom() - b, rect.bottom()];
        for row in 0..3 {
            for col in 0..3 {
                let cell = Rect::new(
                    dx[col],
                    dy[row],
                    dx[col + 1] - dx[col],
                    dy[row + 1] - dy[row],
                );
                if cell.w <= 0.0 || cell.h <= 0.0 {
                    continue;
                }
                ui.textured_quad(
                    asset.texture,
                    cell,
                    [sx[col], sy[row], sx[col + 1], sy[row + 1]],
                    tint,
                );
            }
        }
        true
    }

    /// Repeat `name` horizontally at its authored size (`w * art_scale`),
    /// cropping the last tile at `rect.right()`. Used for the console middle
    /// pieces: they tile, they never stretch.
    pub fn tile_x(
        &self,
        ui: &mut UiBatch,
        name: &str,
        rect: Rect,
        tint: [f32; 4],
        art_scale: f32,
    ) -> bool {
        let Some(asset) = self.asset(name) else {
            crate::dev_log!("HUD_PACK lookup_missing={name}");
            return false;
        };
        let scale = art_scale.max(0.01);
        let tile_w = asset.w as f32 * scale;
        if rect.w <= 0.0 || rect.h <= 0.0 || tile_w <= 0.5 {
            return false;
        }
        let tiles = (rect.w / tile_w).ceil().max(1.0) as usize;
        for index in 0..tiles {
            let x = rect.x + index as f32 * tile_w;
            let width = tile_w.min(rect.right() - x);
            if width <= 0.0 {
                break;
            }
            let u1 = (width / tile_w).clamp(0.0, 1.0);
            ui.textured_quad(
                asset.texture,
                Rect::new(x, rect.y, width, rect.h),
                [0.0, 0.0, u1, 1.0],
                tint,
            );
        }
        true
    }

    /// 9-slice horizontally with a **tiled** centre strip, so the end
    /// ornaments stay intact and the flat middle repeats instead of stretching
    /// (the topbar plate). Vertically the element maps 1:1 onto `rect.h`.
    pub fn tile_center_x(
        &self,
        ui: &mut UiBatch,
        name: &str,
        rect: Rect,
        tint: [f32; 4],
        art_scale: f32,
    ) -> bool {
        let Some(asset) = self.asset(name) else {
            crate::dev_log!("HUD_PACK lookup_missing={name}");
            return false;
        };
        let scale = art_scale.max(0.01);
        let [al, _, ar, _] = asset.insets;
        let left = al * scale;
        let right = ar * scale;
        if rect.w <= left + right + 1.0 || rect.h <= 0.0 {
            return self.stretch(ui, name, rect, tint);
        }
        let aw = asset.w as f32;
        // Left ornament.
        ui.textured_quad(
            asset.texture,
            Rect::new(rect.x, rect.y, left, rect.h),
            [0.0, 0.0, al / aw, 1.0],
            tint,
        );
        // Tiled middle.
        let middle_src = (aw - al - ar).max(1.0);
        let tile_w = middle_src * scale;
        let middle_dst = rect.w - left - right;
        let tiles = (middle_dst / tile_w).ceil().max(1.0) as usize;
        let mut x = rect.x + left;
        let end = rect.right() - right;
        for _ in 0..tiles {
            let width = tile_w.min(end - x);
            if width <= 0.0 {
                break;
            }
            let u_span = width / tile_w;
            ui.textured_quad(
                asset.texture,
                Rect::new(x, rect.y, width, rect.h),
                [al / aw, 0.0, (al + middle_src * u_span) / aw, 1.0],
                tint,
            );
            x += width;
        }
        // Right ornament.
        ui.textured_quad(
            asset.texture,
            Rect::new(rect.right() - right, rect.y, right, rect.h),
            [(aw - ar) / aw, 0.0, 1.0, 1.0],
            tint,
        );
        true
    }

    /// Full-width topbar: a measured flat column extends the bar, with the
    /// authored plate kept at both ends (mirrored on the right). The spear and
    /// diamond ornaments are never stretched. Narrow screens split the plate
    /// into two halves instead of overlapping them.
    pub fn draw_topbar(&self, ui: &mut UiBatch, rect: Rect, scale: f32) {
        let Some(asset) = self.asset("topbar_plate_normal_720x32") else {
            crate::dev_log!("HUD_PACK lookup_missing=topbar_plate_normal_720x32");
            return;
        };
        if rect.w <= 0.0 || rect.h <= 0.0 {
            return;
        }
        let tint = [1.0; 4];
        let width = asset.w as f32;
        // Base: one flat column stretched across the full bar.
        ui.textured_quad(
            asset.texture,
            rect,
            [
                self.topbar_flat_u,
                0.0,
                self.topbar_flat_u + 1.0 / width,
                1.0,
            ],
            tint,
        );
        let plate_w = width * scale;
        if rect.w >= plate_w * 2.0 + 4.0 {
            ui.textured_quad(
                asset.texture,
                Rect::new(rect.x, rect.y, plate_w, rect.h),
                [0.0, 0.0, 1.0, 1.0],
                tint,
            );
            ui.textured_quad(
                asset.texture,
                Rect::new(rect.right() - plate_w, rect.y, plate_w, rect.h),
                [1.0, 0.0, 0.0, 1.0],
                tint,
            );
        } else {
            let half = rect.w * 0.5;
            ui.textured_quad(
                asset.texture,
                Rect::new(rect.x, rect.y, half, rect.h),
                [0.0, 0.0, 1.0, 1.0],
                tint,
            );
            ui.textured_quad(
                asset.texture,
                Rect::new(rect.x + half, rect.y, half, rect.h),
                [1.0, 0.0, 0.0, 1.0],
                tint,
            );
        }
    }

    /// Draw one `commandbuttons` atlas cell.
    pub fn draw_atlas_cell(
        &self,
        ui: &mut UiBatch,
        id: &str,
        rect: Rect,
        tint: [f32; 4],
    ) -> bool {
        let Some(cell) = self.atlas_cell(id) else {
            crate::dev_log!("HUD_PACK atlas_missing={id}");
            return false;
        };
        let Some(asset) = self
            .assets
            .iter()
            .find(|asset| asset.texture == cell.texture)
        else {
            return false;
        };
        let width = asset.w as f32;
        let height = asset.h as f32;
        let [x, y, w, h] = cell.rect;
        ui.textured_quad(
            cell.texture,
            rect,
            [x / width, y / height, (x + w) / width, (y + h) / height],
            tint,
        );
        true
    }

    /// Four console section assets, left to right (the pack's own order).
    pub const SECTION_ASSETS: [&'static str; 4] = [
        "console_left_cap_normal_256x288",
        "console_left_mid_normal_256x288",
        "console_right_mid_normal_256x288",
        "console_right_cap_normal_256x288",
    ];

    /// Console shell as four framed sections: each asset is its own 9-slice
    /// with the manifest insets `(32, 48, 32, 48)`, edge to edge.
    ///
    /// This is the operator's composited `compose-C.png` reading: the four
    /// PNGs are separate framed panels, never one continuous tiled band (a
    /// tiled middle repeats the gold frame every 360 px and reads as
    /// separate panels).
    pub fn draw_console_sections(&self, ui: &mut UiBatch, sections: &[Rect; 4], scale: f32) {
        for (rect, name) in sections.iter().zip(Self::SECTION_ASSETS) {
            if rect.w <= 0.0 || rect.h <= 0.0 {
                continue;
            }
            self.nine(ui, name, *rect, [1.0; 4], scale);
        }
    }

    /// The four corner quads of a 9-slice, as `(dst, src)` pairs in TL, TR, BL,
    /// BR order. The invariant `hud.pack.nine-slice-insets` asserts
    /// `src.w == inset_l/r` and `src.h == inset_t/b`.
    pub fn corner_quads(
        &self,
        name: &str,
        rect: Rect,
        art_scale: f32,
    ) -> Option<[(Rect, Rect); 4]> {
        let asset = self.asset(name)?;
        let scale = art_scale.max(0.01);
        let [al, at, ar, ab] = asset.insets;
        let [aw, ah] = asset.size();
        let (l, t, r, b) = (al * scale, at * scale, ar * scale, ab * scale);
        if rect.w <= l + r + 1.0 || rect.h <= t + b + 1.0 {
            return None;
        }
        let corner = |x: f32, y: f32, w: f32, h: f32, u: f32, v: f32, uw: f32, vh: f32| {
            (
                Rect::new(x, y, w, h),
                Rect::new(u * aw, v * ah, uw * aw, vh * ah),
            )
        };
        Some([
            corner(rect.x, rect.y, l, t, 0.0, 0.0, al / aw, at / ah),
            corner(
                rect.right() - r,
                rect.y,
                r,
                t,
                (aw - ar) / aw,
                0.0,
                ar / aw,
                at / ah,
            ),
            corner(
                rect.x,
                rect.bottom() - b,
                l,
                b,
                0.0,
                (ah - ab) / ah,
                al / aw,
                ab / ah,
            ),
            corner(
                rect.right() - r,
                rect.bottom() - b,
                r,
                b,
                (aw - ar) / aw,
                (ah - ab) / ah,
                ar / aw,
                ab / ah,
            ),
        ])
    }
}

/// Manifest inset audit for the loader, the checker and the report:
/// `assets=92 sliced=<n> unsliced=<n> insets_ignored=<n>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SliceSummary {
    pub assets: usize,
    pub sliced: usize,
    pub unsliced: usize,
    pub insets_ignored: usize,
}

impl std::fmt::Display for SliceSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "assets={} sliced={} unsliced={} insets_ignored={}",
            self.assets, self.sliced, self.unsliced, self.insets_ignored
        )
    }
}

impl HudPack {
    pub fn slice_summary(&self) -> SliceSummary {
        let mut summary = SliceSummary {
            assets: self.assets.len(),
            sliced: 0,
            unsliced: 0,
            insets_ignored: 0,
        };
        for asset in &self.assets {
            if !asset.insets_sliceable() {
                summary.insets_ignored += 1;
            }
            if asset.sliced() {
                summary.sliced += 1;
            } else {
                summary.unsliced += 1;
            }
        }
        summary
    }
}

/// Log a manifest inset defect exactly once per file. The line is the
/// machine-checkable evidence `tools/hud_pack_check.py` looks for:
/// `HUD_PACK insets-ignored <file> (insets l,t,r,b on WxH)`.
pub fn log_insets_ignored(asset: &HudAsset) {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    static LOGGED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let logged = LOGGED.get_or_init(|| Mutex::new(HashSet::new()));
    let Ok(mut logged) = logged.lock() else {
        return;
    };
    if !logged.insert(asset.file.clone()) {
        return;
    }
    crate::dev_log!(
        "HUD_PACK insets-ignored {} (insets {},{},{},{} on {}x{})",
        asset.file,
        asset.insets[0],
        asset.insets[1],
        asset.insets[2],
        asset.insets[3],
        asset.w,
        asset.h
    );
}

// ---------------------------------------------------------------------------
// JSON helpers (serde_json is already a dependency; no derive feature needed)
// ---------------------------------------------------------------------------

fn walk<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn number(value: &serde_json::Value, path: &[&str]) -> Result<f32, PackError> {
    let parsed = walk(value, path).and_then(|item| item.as_f64())
        .ok_or_else(|| PackError::Manifest(format!("missing number {}", path.join("."))))?;
    let parsed = parsed as f32;
    if !parsed.is_finite() {
        return Err(PackError::Manifest(format!("non-finite number {}", path.join("."))));
    }
    Ok(parsed)
}

fn integer(value: &serde_json::Value, path: &[&str]) -> Result<i64, PackError> {
    walk(value, path)
        .and_then(|item| item.as_i64())
        .ok_or_else(|| PackError::Manifest(format!("missing integer {}", path.join("."))))
}

fn positive_usize(value: &serde_json::Value, path: &[&str]) -> Result<usize, PackError> {
    let raw = integer(value, path)?;
    if raw <= 0 {
        return Err(PackError::Manifest(format!("{} must be positive", path.join("."))));
    }
    usize::try_from(raw).map_err(|_| PackError::Manifest(format!("{} exceeds usize", path.join("."))))
}

fn string(value: &serde_json::Value, path: &[&str]) -> Result<String, PackError> {
    walk(value, path)
        .and_then(|item| item.as_str())
        .map(str::to_string)
        .ok_or_else(|| PackError::Manifest(format!("missing string {}", path.join("."))))
}

fn pair_u32(value: &serde_json::Value, path: &[&str]) -> Result<[u32; 2], PackError> {
    let list = walk(value, path)
        .and_then(|item| item.as_array())
        .ok_or_else(|| PackError::Manifest(format!("missing pair {}", path.join("."))))?;
    if list.len() != 2 {
        return Err(PackError::Manifest(format!("pair {} must have exactly 2 items", path.join("."))));
    }
    let parse = |index: usize| -> Result<u32, PackError> {
        let raw = list[index].as_u64().ok_or_else(|| PackError::Manifest(format!("pair {} values must be unsigned integers", path.join("."))))?;
        u32::try_from(raw).map_err(|_| PackError::Manifest(format!("pair {} value exceeds u32", path.join("."))))
    };
    let pair = [parse(0)?, parse(1)?];
    if pair.contains(&0) {
        return Err(PackError::Manifest(format!("pair {} values must be positive", path.join("."))));
    }
    Ok(pair)
}

fn quad(value: &serde_json::Value, path: &[&str]) -> Result<[f32; 4], PackError> {
    let list = walk(value, path)
        .and_then(|item| item.as_array())
        .ok_or_else(|| PackError::Manifest(format!("missing quad {}", path.join("."))))?;
    if list.len() != 4 {
        return Err(PackError::Manifest(format!("quad {} must have exactly 4 items", path.join("."))));
    }
    let mut result = [0.0f32; 4];
    for (index, slot) in result.iter_mut().enumerate() {
        let raw = list[index].as_f64().ok_or_else(|| PackError::Manifest(format!("quad {} values must be numbers", path.join("."))))?;
        *slot = raw as f32;
        if !slot.is_finite() || *slot < 0.0 {
            return Err(PackError::Manifest(format!("quad {} values must be finite and non-negative", path.join("."))));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    /// The pack is an in-repo artefact (`analysis/hudskin/final/...`), so a
    /// test failure here means the operator's delivery is missing, not that
    /// the machine lacks an optional install.
    pub(crate) fn shared() -> &'static (HudPack, Vec<TextureImage>) {
        static CELL: OnceLock<(HudPack, Vec<TextureImage>)> = OnceLock::new();
        CELL.get_or_init(|| {
            let root = find_installed().expect("HUD_PACK missing=manifest.json (pack not found)");
            let mut textures = Vec::new();
            let pack = HudPack::load(&root, &mut textures).expect("HUD_PACK load");
            (pack, textures)
        })
    }

    /// `hud.pack.inset-guard`: the 2026-11-16 manifest has 11 rows whose insets
    /// cannot be sliced. They are classified unsliced (single images), never
    /// "fixed" by inventing numbers; the summary is the report line.
    #[test]
    fn hud_pack_inset_guard_counts_known_defects() {
        let (pack, _) = shared();
        let summary = pack.slice_summary();
        assert_eq!(
            summary.to_string(),
            "assets=92 sliced=29 unsliced=63 insets_ignored=11",
            "manifest slice audit"
        );
        // # manifest-defect (2026-11-16 delivery; flip when the pack is fixed)
        let known = [
            "minimap_header_normal_216x20",
            "icon_food_normal_32",
            "icon_food_warn_32",
            "icon_gold_normal_32",
            "icon_gold_warn_32",
            "icon_lumber_normal_32",
            "icon_lumber_warn_32",
            "upkeep_strip_roc_720x14",
            "upkeep_strip_tft_720x14",
            "tooltip_arrow_down_24x12",
            "tooltip_arrow_up_24x12",
        ];
        for name in known {
            let asset = pack.asset(name).unwrap_or_else(|| panic!("asset {name}"));
            assert!(
                !asset.insets_sliceable(),
                "{name} is in the defect table but its insets pass the guard"
            );
            assert!(!asset.sliced(), "{name} must be drawn unsliced");
        }
        let guard_failures: Vec<&str> = pack
            .assets
            .iter()
            .filter(|asset| !asset.insets_sliceable())
            .map(|asset| asset.name.as_str())
            .collect();
        assert_eq!(guard_failures.len(), known.len(), "{guard_failures:?}");
        for name in guard_failures {
            assert!(known.contains(&name), "unlisted inset defect {name}");
        }
        // A 9-slice call on a defective asset must not panic and must fall
        // back to one uniform quad.
        let mut batch = UiBatch::default();
        assert!(pack.nine(
            &mut batch,
            "minimap_header_normal_216x20",
            Rect::new(0.0, 0.0, 304.0, 28.0),
            [1.0; 4],
            1.40625,
        ));
        assert_eq!(batch.textured.len(), 1, "header is one image");
        assert_eq!(batch.textured[0].vertices.len(), 6, "one quad, no slicing");
    }

    /// `hud.pack.loads-every-asset`: the manifest contract is complete.
    #[test]
    fn hud_pack_loads_every_asset() {
        let (pack, textures) = shared();
        assert_eq!(pack.assets.len(), 92, "manifest asset count");
        assert_eq!(pack.font_files.len(), 5, "font_files");
        for asset in &pack.assets {
            assert!(textures.get(asset.texture).is_some(), "texture for {}", asset.name);
            let texture = &textures[asset.texture];
            assert_eq!(
                (texture.width, texture.height),
                (asset.w, asset.h),
                "{} texture size",
                asset.name
            );
            assert_eq!(texture.rgba.len(), (asset.w * asset.h * 4) as usize);
        }
        // Every manifest row is reachable by name and the lookups agree.
        for asset in &pack.assets {
            assert_eq!(pack.texture(&asset.name), Some(asset.texture));
            assert_eq!(pack.rect(&asset.name), Some(asset.rect()));
            assert_eq!(pack.insets(&asset.name), Some(asset.insets));
        }
        assert_eq!(pack.cells.len(), 20, "commandbuttons atlas cells");
        assert_eq!((pack.card_columns, pack.card_rows), (4, 3));
        assert_eq!((pack.group_count, pack.group_cell), (10, 44.0));
        assert_eq!(pack.design_height, 768.0);
        assert_eq!(pack.safe_landscape, [48.0, 80.0, 48.0, 48.0]);
        assert_eq!(pack.safe_portrait, [24.0, 96.0, 24.0, 64.0]);
        let font = pack.font_file_named("numeric").expect("numeric font");
        assert!(pack.font_path(font).is_file());
    }

    /// `hud.pack.nine-slice-insets`: every authored panel's corner quad uses
    /// the manifest inset as its source size (corners are never resampled).
    #[test]
    fn hud_pack_nine_slice_insets() {
        let (pack, _) = shared();
        let panels = [
            "console_left_cap_normal_256x288",
            "console_left_mid_normal_256x288",
            "console_right_cap_normal_256x288",
            "console_right_mid_normal_256x288",
            "minimap_frame_normal_216",
            "portrait_frame_normal_128",
            "portrait_frame_hero_128",
            "portrait_frame_enemy_128",
            "selection_panel_normal_240x180",
            "tooltip_panel_normal_384x256",
            "menu_panel_normal_512x384",
            "menu_button_normal_256x64",
            "loading_frame_normal_1024x512",
            "score_panel_normal_1024x512",
            "resource_pill_normal_236x32",
            "button_border_normal_64",
            "control_group_button_active_44",
        ];
        for (index, name) in panels.iter().enumerate() {
            let asset = pack.asset(name).unwrap_or_else(|| panic!("asset {name}"));
            let scale = 1.0 + index as f32 * 0.13;
            let rect = Rect::new(10.0, 20.0, 900.0 + index as f32 * 20.0, 500.0);
            let corners = pack
                .corner_quads(name, rect, scale)
                .unwrap_or_else(|| panic!("corners for {name}"));
            let [al, at, ar, ab] = asset.insets;
            assert!((corners[0].1.w - al).abs() < 1e-4, "{name} TL src w");
            assert!((corners[0].1.h - at).abs() < 1e-4, "{name} TL src h");
            assert!((corners[1].1.w - ar).abs() < 1e-4, "{name} TR src w");
            assert!((corners[2].1.h - ab).abs() < 1e-4, "{name} BL src h");
            // The drawn corner scales with the art scale while the source
            // rectangle stays the authored inset.
            assert!((corners[0].0.w - al * scale).abs() < 1e-3, "{name} TL dst w");
            assert!((corners[1].0.w - ar * scale).abs() < 1e-3, "{name} TR dst w");
        }
    }

    /// `hud.pack.card-cells-90px`: the 4x3 card at the 2414x1080 device scale.
    #[test]
    fn hud_pack_card_cells_90px() {
        let (pack, _) = shared();
        let viewport = Vec2::new(2414.0, 1080.0);
        let scale = pack.scale_for(viewport, false);
        assert!((scale - 1.40625).abs() < 1e-5, "device scale {scale}");
        let cell = pack.card_cell * scale;
        assert!((cell - 90.0).abs() < 1e-4, "card cell {cell}");
        let gap = pack.card_gap * scale;
        let card_w = pack.card_columns as f32 * cell + (pack.card_columns - 1) as f32 * gap;
        let card_h = pack.card_rows as f32 * cell + (pack.card_rows - 1) as f32 * gap;
        assert!((card_w - 376.875).abs() < 1e-3, "card width {card_w}");
        assert!((card_h - 281.25).abs() < 1e-3, "card height {card_h}");
        // Right-aligned inside the console at the safe-area margin.
        let safe = pack.safe_area(false, scale);
        let card_right = viewport.x - safe[2];
        assert!((card_right - 2346.5).abs() < 1e-3, "card right {card_right}");
        assert!(card_w >= 4.0 * 90.0 - 1.0, "every cell is a >= 90 px leaf");
        let groups = pack.group_cell * scale;
        assert!((groups - 61.875).abs() < 1e-4, "group art {groups}");
        assert!(pack.group_count == 10);
    }

    /// `hud.pack.no-legacy-stone`: the console is real pack art with a neutral
    /// tint, drawn as four framed sections, never the procedural stone field
    /// or a magenta slot wash.
    #[test]
    fn hud_pack_no_legacy_stone() {
        let (pack, _) = shared();
        let console = Rect::new(0.0, 675.0, 2414.0, 405.0);
        let widths = [614.0, 834.0, 395.0, 571.0];
        let mut x = 0.0;
        let sections: [Rect; 4] = std::array::from_fn(|index| {
            let rect = Rect::new(x, console.y, widths[index], console.h);
            x += widths[index];
            rect
        });
        assert_eq!(sections[3].right(), 2414.0, "sections tile the viewport");
        let mut batch = UiBatch::default();
        pack.draw_console_sections(&mut batch, &sections, 1.40625);
        assert!(batch.white.is_empty(), "console shell must not use flat quads");
        assert!(batch.font.is_empty() && batch.minimap.is_empty());
        assert!(!batch.textured.is_empty(), "console shell drew nothing");
        let pieces = HudPack::SECTION_ASSETS.map(|name| pack.texture(name).unwrap());
        let mut covered = 0.0f32;
        for draw in &batch.textured {
            assert!(
                pieces.contains(&draw.texture),
                "console shell sampled texture {} outside the four sections",
                draw.texture
            );
            for vertex in &draw.vertices {
                let [r, g, b, a] = vertex.color;
                assert!((a - 1.0).abs() < 1e-6, "neutral tint alpha {a}");
                assert!(
                    !(r > 0.5 && b > 0.5 && g < 0.1),
                    "magenta tint leaked into the console: {r},{g},{b}"
                );
            }
        }
        // Each section is a 9-slice: 9 quads = 54 vertices.
        for draw in &batch.textured {
            assert_eq!(
                draw.vertices.len() % 54,
                0,
                "section {} is not a 9-slice",
                draw.texture
            );
        }
        for draw in &batch.textured {
            for chunk in draw.vertices.chunks_exact(6) {
                let xs: Vec<f32> = chunk.iter().map(|v| v.pos[0]).collect();
                let ys: Vec<f32> = chunk.iter().map(|v| v.pos[1]).collect();
                let w = xs.iter().cloned().fold(f32::MIN, f32::max)
                    - xs.iter().cloned().fold(f32::MAX, f32::min);
                let h = ys.iter().cloned().fold(f32::MIN, f32::max)
                    - ys.iter().cloned().fold(f32::MAX, f32::min);
                covered += w * h;
            }
        }
        assert!(
            covered >= console.w * console.h * 0.85,
            "console shell coverage {covered:.0} < 85% of the band"
        );
    }

    #[test]
    fn pack_file_rejects_traversal_and_symlink_components() {
        let root=std::env::temp_dir().join(format!("hud-pack-path-{}",std::process::id()));
        let _=std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("fonts")).expect("create root");
        std::fs::write(root.join("fonts/font.ttf"),b"font").expect("font");
        assert!(pack_file(&root,"../outside.ttf","font").is_err());
        assert!(pack_file(&root,"fonts\\font.ttf","font").is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("fonts"),root.join("alias")).expect("symlink");
            assert!(pack_file(&root,"alias/font.ttf","font").is_err());
        }
        assert!(pack_file(&root,"fonts/font.ttf","font").is_ok());
        let _=std::fs::remove_dir_all(&root);
    }

    #[test]
    fn atlas_validator_rejects_bounds_overlap_and_duplicate_ids() {
        let valid=vec![
            serde_json::json!({"id":"one","x":0,"y":0,"w":8,"h":8}),
            serde_json::json!({"id":"two","x":8,"y":0,"w":8,"h":8}),
        ];
        assert!(validate_atlas_entries(&valid,16,8).is_ok());
        let overlap=vec![
            serde_json::json!({"id":"one","x":0,"y":0,"w":9,"h":8}),
            serde_json::json!({"id":"two","x":8,"y":0,"w":8,"h":8}),
        ];
        assert!(validate_atlas_entries(&overlap,16,8).unwrap_err().to_string().contains("overlap"));
        let outside=vec![serde_json::json!({"id":"outside","x":15,"y":0,"w":2,"h":8})];
        assert!(validate_atlas_entries(&outside,16,8).is_err());
        let duplicate=vec![
            serde_json::json!({"id":"same","x":0,"y":0,"w":4,"h":4}),
            serde_json::json!({"id":"same","x":4,"y":0,"w":4,"h":4}),
        ];
        assert!(validate_atlas_entries(&duplicate,16,8).unwrap_err().to_string().contains("duplicate"));
    }

    #[test]
    fn numeric_manifest_helpers_reject_invalid_values() {
        let doc=serde_json::json!({"pair":[64,64],"bad_pair":[64],"negative":-1,"quad":[0.0,1.0,-1.0,0.0]});
        assert_eq!(pair_u32(&doc,&["pair"]).unwrap(),[64,64]);
        assert!(pair_u32(&doc,&["bad_pair"]).is_err());
        assert!(positive_usize(&doc,&["negative"]).is_err());
        assert!(quad(&doc,&["quad"]).is_err());
    }

    #[test]
    fn hud_pack_missing_file_is_loud() {
        let root = std::env::temp_dir().join("hud-pack-loud-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp");
        std::fs::create_dir_all(root.join("fonts")).expect("fonts dir");
        for font in ["f.ttf","f2.ttf","f3.ttf","f4.ttf","f5.ttf"] {
            std::fs::write(root.join("fonts").join(font), b"").expect("placeholder font");
        }
        std::fs::write(
            root.join("manifest.json"),
            r#"{"design_height":768,"reference_resolution":{"landscape":[1920,1080],"portrait":[1080,1920]},"safe_area_1080p":{"landscape":[48,80,48,48],"portrait":[24,96,24,64]},"command_card":{"columns":4,"rows":3,"cell":[64,64],"gap":4},"control_groups":{"count":10,"cell":[44,44]},"atlas":{"commandbuttons":{"file":"a.png","size":[64,64],"cell":[64,64],"columns":1,"rows":1,"entries":[{"id":"x","x":0,"y":0,"w":64,"h":64}]}},"assets":[{"name":"a","file":"a.png","w":64,"h":64,"insets":{"l":0,"t":0,"r":0,"b":0},"states":[],"atlas":"standalone","mips":false}],"font_files":["f.ttf","f2.ttf","f3.ttf","f4.ttf","f5.ttf"]}"#,
        )
        .expect("manifest");
        let error = HudPack::load(&root, &mut Vec::new()).expect_err("missing a.png");
        assert_eq!(
            error.to_string(),
            "HUD_PACK missing=a.png",
            "a missing asset must name the file"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
