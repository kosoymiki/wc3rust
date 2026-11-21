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

/// One manifest asset (one PNG = one texture, or one rect of a v2 atlas page).
#[derive(Clone, Debug, PartialEq)]
pub struct HudAsset {
    /// Manifest `id` (v2) / `name` (v1).
    pub name: String,
    pub file: String,
    pub w: u32,
    pub h: u32,
    /// Manifest 9-slice insets, source pixels `[left, top, right, bottom]`.
    pub insets: [f32; 4],
    pub states: Vec<String>,
    /// v2 atlas page id; `None` for a standalone PNG.
    pub atlas: Option<String>,
    /// v1 category string (`ui_frames`, `icons`, ...); not a page.
    pub atlas_category: Option<String>,
    /// v2 atlas rect `[x, y, w, h]` in page pixels.
    pub atlas_rect: Option<[u32; 4]>,
    /// UV denominator: the page size for an atlased asset, the PNG size
    /// otherwise. `w`/`h` always stay the authored source size.
    pub uv_size: [f32; 2],
    /// Manifest `kind` (`frame`, `button`, `icon`, ...).
    pub kind: String,
    pub alpha: bool,
    pub colorspace: String,
    pub scale: String,
    /// `true` when the pack cannot render without this asset (default).
    pub critical: bool,
    pub sha256: Option<String>,
    /// Manifest `mips` flag: panel slices are `false` (never mipmapped).
    pub mips: bool,
    /// Index into `Assets::textures`.
    pub texture: usize,
}

impl HudAsset {
    /// UV denominator (page size for an atlas asset, source size otherwise).
    pub fn size(&self) -> [f32; 2] {
        self.uv_size
    }

    /// Source size of the element in pixels.
    pub fn source_size(&self) -> [f32; 2] {
        [self.w as f32, self.h as f32]
    }

    /// Source pixel rect: the atlas rect when packed, else the whole PNG.
    pub fn rect(&self) -> [f32; 4] {
        match self.atlas_rect {
            Some([x, y, w, h]) => [x as f32, y as f32, w as f32, h as f32],
            None => [0.0, 0.0, self.w as f32, self.h as f32],
        }
    }

    /// Normalised UV rect of this element inside its texture.
    pub fn uv(&self) -> [f32; 4] {
        let [x, y, w, h] = self.rect();
        let [width, height] = self.size();
        let width = width.max(1.0);
        let height = height.max(1.0);
        [x / width, y / height, (x + w) / width, (y + h) / height]
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
            || (self.atlas_category.as_deref() == Some("ui_frames")
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
    /// Page size used as the UV denominator.
    pub page: [f32; 2],
}

/// Loader error. `Display` starts with `HUD_PACK ` so a device log line is
/// greppable and a missing file is never mistaken for a successful fallback.
#[derive(Clone, Debug, PartialEq)]
pub enum PackError {
    Io { path: String, error: String },
    Manifest(String),
    /// A `format_version` the loader does not implement.
    UnsupportedVersion { found: String, supported: String },
    /// A strict §9.3/§9.4 rule failed; the pack is refused, not clamped.
    Invalid {
        element: String,
        rule: String,
        detail: String,
    },
    Missing { file: String },
    Size { file: String, manifest: [u32; 2], actual: [u32; 2] },
    Texture { file: String, error: String },
    Atlas { atlas: String, file: String, error: String },
    /// The pack builder refused its own output or its input.
    Build(String),
    /// A file listed in `SHA256SUMS.txt` does not hash to its recorded value.
    Hash { file: String, expected: String, actual: String },
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, error } => write!(f, "HUD_PACK io={path} error={error}"),
            Self::Manifest(error) => write!(f, "HUD_PACK manifest_error={error}"),
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "HUD_PACK manifest_version={found} supported={supported}"
            ),
            Self::Invalid {
                element,
                rule,
                detail,
            } => write!(
                f,
                "HUD_PACK invalid element={element} rule={rule} detail={detail}"
            ),
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
            Self::Atlas { atlas, file, error } => {
                write!(f, "HUD_PACK atlas={atlas} file={file} error={error}")
            }
            Self::Build(error) => write!(f, "HUD_PACK build_error={error}"),
            Self::Hash {
                file,
                expected,
                actual,
            } => write!(f, "HUD_PACK hash={file} expected={expected} actual={actual}"),
        }
    }
}

impl std::error::Error for PackError {}

/// One loaded atlas page (v2 `atlases[]`), kept for diagnostics and the
/// `HUD_PACK_VALIDATE` line.
#[derive(Clone, Debug, PartialEq)]
pub struct AtlasInfo {
    pub id: String,
    pub file: String,
    pub w: u32,
    pub h: u32,
    pub padding: u32,
    pub gutter: u32,
    pub entries: usize,
    pub texture: usize,
}

/// The loaded pack. Holds metadata and `Assets::textures` indices; the pixels
/// stay in the asset texture list (a clone of the pack is cheap).
#[derive(Clone, Debug)]
pub struct HudPack {
    pub root: PathBuf,
    /// Manifest format that was loaded (1 = shipped final pack, 2 = `assets/hud`).
    pub format_version: u32,
    pub theme: String,
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
    /// v2 atlas pages that were loaded (empty for the v1 final pack).
    pub atlases: Vec<AtlasInfo>,
    pub assets: Vec<HudAsset>,
    by_name: HashMap<String, usize>,
    cells: HashMap<String, AtlasCell>,
    /// Last strict validation report (errors empty on a loaded pack).
    pub validation: super::manifest::ValidationReport,
    pub atlas_texture: Option<usize>,
    pub font_files: Vec<String>,
    /// `Assets::textures` indices of the primary / serif / sans / numeric
    /// glyph atlases (`HudFonts::bind`). Primary is the operator's Nimrod MT
    /// read from the installed archive (or the pack serif when absent).
    pub font_textures: [usize; 4],
    /// Files verified against `SHA256SUMS.txt` at load (0 when the pack ships
    /// no sums file; the load log then says `hash=unverified`).
    pub hash_files: usize,
    /// Bytes those files occupy.
    pub hashed_bytes: u64,
    /// Flat column of the topbar plate as a fraction of the element (0..1),
    /// measured from the art at load. The bar is extended by stretching this
    /// one-pixel column, so the authored spear/diamond ornaments are never
    /// stretched. [`HudAsset::uv`] maps it into the page.
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
/// Download drop, the repo for headless proofs), never art constants. Both app
/// ids that the device has shipped are listed because the current install id is
/// `org.herofolends.p0` while an older build used `org.wc3rust.p0`; a renamed
/// package still finds its pack through `WC3RUST_HUD_PACK` or the install root
/// next to `War3.mpq` ([`find_installed_near`]).
pub fn pack_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os(PACK_ENV) {
        dirs.push(PathBuf::from(dir));
    }
    for root in install_roots() {
        dirs.push(root.join("hud_pack"));
        dirs.push(root.join(PACK_DIR_NAME));
    }
    dirs.push(PathBuf::from(
        "/storage/emulated/0/Android/data/org.herofolends.p0/files/hud_pack",
    ));
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

/// Install roots that can hold `<root>/hud_pack`, most specific first.
fn install_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(dir) = std::env::var_os("H2_WC3_INSTALL") {
        roots.push(PathBuf::from(dir));
    }
    roots.push(PathBuf::from("/data/data/org.herofolends.p0/files"));
    roots.push(PathBuf::from("/data/data/org.wc3rust.p0/files"));
    roots
}

/// First directory that actually carries a `manifest.json`.
pub fn find_installed() -> Option<PathBuf> {
    pack_search_dirs()
        .into_iter()
        .find(|dir| dir.join("manifest.json").is_file())
}

/// Pack next to a known install root (`<root>/hud_pack`, `<root>/wc3rust-hud-final`),
/// then the global search. The install root is authoritative when the assets
/// were opened from it, so a renamed package still binds its own pack.
pub fn find_installed_near(install_root: &Path) -> Option<PathBuf> {
    for dir in [install_root.join("hud_pack"), install_root.join(PACK_DIR_NAME)] {
        if dir.join("manifest.json").is_file() {
            return Some(dir);
        }
    }
    find_installed()
}

/// One file whose bytes do not match `SHA256SUMS.txt`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackHashMismatch {
    pub file: String,
    pub expected: String,
    pub actual: String,
}

/// Outcome of the packaged-asset hash check.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackReport {
    /// Manifest asset declarations.
    pub assets: usize,
    /// Manifest font files.
    pub fonts: usize,
    /// Files listed in `SHA256SUMS.txt` and re-hashed.
    pub files: usize,
    /// Total bytes hashed.
    pub hashed_bytes: u64,
    /// `false` when the pack ships no `SHA256SUMS.txt`.
    pub sums_present: bool,
    /// Listed files whose bytes do not hash to the recorded value (or are gone).
    pub mismatches: Vec<PackHashMismatch>,
    /// Count of mismatches.
    pub mismatched: usize,
}

/// Manifest files the HUD draws: asset PNGs, atlas sheets and font files.
/// These are the mandatory members of `SHA256SUMS.txt`; a mismatch there is
/// fatal. Everything else the sums list (README, docs) is reported only.
fn declared_pack_files(manifest: &serde_json::Value) -> std::collections::HashSet<String> {
    let mut files = std::collections::HashSet::new();
    if let Some(assets) = manifest["assets"].as_array() {
        for asset in assets {
            if let Some(file) = asset["file"].as_str() {
                files.insert(normalize_pack_file(file));
            }
        }
    }
    if let Some(atlases) = manifest["atlas"].as_object() {
        for atlas in atlases.values() {
            if let Some(file) = atlas["file"].as_str() {
                files.insert(normalize_pack_file(file));
            }
        }
    }
    // v2 named pages: object map or array.
    match &manifest["atlases"] {
        serde_json::Value::Object(atlases) => {
            for atlas in atlases.values() {
                if let Some(file) = atlas["file"].as_str() {
                    files.insert(normalize_pack_file(file));
                }
            }
        }
        serde_json::Value::Array(atlases) => {
            for atlas in atlases {
                if let Some(file) = atlas["file"].as_str() {
                    files.insert(normalize_pack_file(file));
                }
            }
        }
        _ => {}
    }
    // v2 fonts: object map or array; only bundled files are mandatory.
    let mut add_font = |font: &serde_json::Value| {
        if let Some(file) = font["file"].as_str()
            && !file.is_empty()
        {
            files.insert(normalize_pack_file(file));
            files.insert(normalize_pack_file(&format!("fonts/{file}")));
        }
    };
    match &manifest["fonts"] {
        serde_json::Value::Object(fonts) => {
            for font in fonts.values() {
                add_font(font);
            }
        }
        serde_json::Value::Array(fonts) => {
            for font in fonts {
                add_font(font);
            }
        }
        _ => {}
    }
    if let Some(fonts) = manifest["font_files"].as_array() {
        for font in fonts.iter().filter_map(|font| font.as_str()) {
            files.insert(normalize_pack_file(&format!("fonts/{font}")));
        }
    }
    files
}

/// One spelling for pack-relative paths (`\`, `./`, leading `/` all removed).
fn normalize_pack_file(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_string()
}

/// Verify every file the pack ships against `SHA256SUMS.txt`.
///
/// `SHA256SUMS.txt` is the packaged manifest of exact bytes: one
/// `<sha256>  <relative path>` line per file (the standard `sha256sum` format).
/// A pack without the file is reported (`sums_present=false`), not failed; a
/// pack that carries it must match byte for byte, so edited or truncated art is
/// a loud `HUD_PACK hash=` error instead of a silently different HUD.
pub fn verify_pack(root: &Path) -> Result<PackReport, PackError> {
    let manifest_path = root.join("manifest.json");
    let text = std::fs::read_to_string(&manifest_path).map_err(|error| PackError::Io {
        path: manifest_path.display().to_string(),
        error: error.to_string(),
    })?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| PackError::Manifest(error.to_string()))?;
    let mut report = PackReport {
        assets: value["assets"].as_array().map(Vec::len).unwrap_or(0),
        fonts: value["font_files"].as_array().map(Vec::len).unwrap_or(0),
        ..PackReport::default()
    };
    let sums_path = root.join("SHA256SUMS.txt");
    if !sums_path.is_file() {
        return Ok(report);
    }
    let sums = std::fs::read_to_string(&sums_path).map_err(|error| PackError::Io {
        path: sums_path.display().to_string(),
        error: error.to_string(),
    })?;
    report.sums_present = true;
    for line in sums.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // `sha256sum` separates the digest and the name with two spaces (and a
        // leading `*` before the name in binary mode).
        let Some((expected, name)) = line.split_once("  ") else {
            return Err(PackError::Manifest(format!(
                "SHA256SUMS.txt line is not `<sha256>  <path>`: {line:?}"
            )));
        };
        let name = name.trim_start_matches('*');
        let expected = expected.trim().to_ascii_lowercase();
        if expected.len() != 64 || !expected.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(PackError::Manifest(format!(
                "SHA256SUMS.txt digest is not 64 hex chars: {line:?}"
            )));
        }
        let path = root.join(name.replace('/', std::path::MAIN_SEPARATOR_STR));
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => {
                report.mismatches.push(PackHashMismatch {
                    file: name.to_string(),
                    expected: expected.clone(),
                    actual: "missing".to_string(),
                });
                report.mismatched += 1;
                continue;
            }
        };
        let actual = crate::map_registry::sha256_hex(&bytes);
        report.files += 1;
        report.hashed_bytes += bytes.len() as u64;
        if actual != expected {
            report.mismatches.push(PackHashMismatch {
                file: name.to_string(),
                expected,
                actual,
            });
            report.mismatched += 1;
        }
    }
    Ok(report)
}

/// Read one atlas page, verify its declared bytes and register the texture.
fn load_atlas_page(
    root: &Path,
    decl: &super::manifest::AtlasDecl,
    textures: &mut Vec<TextureImage>,
    file_textures: &mut HashMap<String, usize>,
) -> Result<(usize, u32, u32), PackError> {
    if decl.w == 0 || decl.h == 0 {
        return Err(PackError::Invalid {
            element: decl.id.clone(),
            rule: "atlas-size-zero".to_string(),
            detail: format!("page {}x{}", decl.w, decl.h),
        });
    }
    let path = root.join(&decl.file);
    if !path.is_file() {
        return Err(PackError::Missing {
            file: decl.file.clone(),
        });
    }
    let bytes = std::fs::read(&path).map_err(|error| PackError::Io {
        path: path.display().to_string(),
        error: error.to_string(),
    })?;
    if let Some(expected) = &decl.sha256 {
        let actual = crate::map_registry::sha256_hex(&bytes);
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(PackError::Hash {
                file: decl.file.clone(),
                expected: expected.clone(),
                actual,
            });
        }
    }
    if let Some(texture) = file_textures.get(&decl.file) {
        let (w, h) = png_dimensions(&bytes).ok_or_else(|| PackError::Atlas {
            atlas: decl.id.clone(),
            file: decl.file.clone(),
            error: "not a PNG (missing IHDR)".to_string(),
        })?;
        if w != decl.w || h != decl.h {
            return Err(PackError::Size {
                file: decl.file.clone(),
                manifest: [decl.w, decl.h],
                actual: [w, h],
            });
        }
        return Ok((*texture, w, h));
    }
    let image = crate::ui_atlas::decode_png(&bytes).map_err(|error| PackError::Atlas {
        atlas: decl.id.clone(),
        file: decl.file.clone(),
        error: error.to_string(),
    })?;
    if image.width != decl.w || image.height != decl.h {
        return Err(PackError::Size {
            file: decl.file.clone(),
            manifest: [decl.w, decl.h],
            actual: [image.width, image.height],
        });
    }
    let texture = textures.len();
    textures.push(image.to_texture());
    file_textures.insert(decl.file.clone(), texture);
    Ok((texture, decl.w, decl.h))
}

fn atlas_entry_rect(
    manifest: &super::manifest::Manifest,
    page_id: &str,
    asset_id: &str,
) -> Option<[u32; 4]> {
    manifest
        .atlas(page_id)
        .or_else(|| {
            manifest
                .command_atlas
                .as_ref()
                .filter(|atlas| atlas.id == page_id)
        })
        .and_then(|page| page.entries.iter().find(|entry| entry.id == asset_id))
        .map(|entry| [entry.x, entry.y, entry.w, entry.h])
}

/// PNG IHDR width/height without decoding the image data.
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < 24 || bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((width, height))
}

/// Lowest-variance vertical column of an element (the flat band of the topbar
/// plate / console mid). Returns `(fraction within the element, mean colour)`.
fn flat_column(asset: &HudAsset, texture: &TextureImage) -> Option<(f32, [f32; 3])> {
    let [ax, ay, aw, ah] = asset.rect();
    let (aw, ah) = (aw as u32, ah as u32);
    if aw <= 8 || ah <= 4 {
        return None;
    }
    let texture_width = texture.width;
    let texture_height = texture.height;
    let ax = ax as u32;
    let ay = ay as u32;
    let mut best = (f32::MAX, aw / 2, [0.0f32; 3]);
    for x in (aw / 5)..(aw * 4 / 5) {
        let mut sum = [0.0f32; 3];
        let mut count = 0.0f32;
        for y in 2..ah.saturating_sub(2) {
            let px = ax + x;
            let py = ay + y;
            if px >= texture_width || py >= texture_height {
                continue;
            }
            let index = ((py * texture_width + px) * 4) as usize;
            let Some(pixel) = texture.rgba.get(index..index + 4) else {
                continue;
            };
            sum[0] += pixel[0] as f32 / 255.0;
            sum[1] += pixel[1] as f32 / 255.0;
            sum[2] += pixel[2] as f32 / 255.0;
            count += 1.0;
        }
        if count <= 0.0 {
            continue;
        }
        let mean = [sum[0] / count, sum[1] / count, sum[2] / count];
        let mut variance = 0.0f32;
        for y in 2..ah.saturating_sub(2) {
            let px = ax + x;
            let py = ay + y;
            if px >= texture_width || py >= texture_height {
                continue;
            }
            let index = ((py * texture_width + px) * 4) as usize;
            let Some(pixel) = texture.rgba.get(index..index + 4) else {
                continue;
            };
            let luminance = 0.2126 * pixel[0] as f32 / 255.0
                + 0.7152 * pixel[1] as f32 / 255.0
                + 0.0722 * pixel[2] as f32 / 255.0;
            let mean_luminance = 0.2126 * mean[0] + 0.7152 * mean[1] + 0.0722 * mean[2];
            variance += (luminance - mean_luminance) * (luminance - mean_luminance);
        }
        if variance < best.0 {
            best = (variance, x, mean);
        }
    }
    if !best.0.is_finite() {
        return None;
    }
    Some(((best.1 as f32 + 0.5) / aw as f32, best.2))
}

impl HudPack {
    /// Load a v1 or v2 manifest, validate every declaration, then read the art.
    ///
    /// The order is deliberate: format -> strict §9.3/§9.4 validation -> hashes
    /// -> textures. An unsupported version, duplicate id, crossing inset set,
    /// atlas UV outside its page, under-padded atlas entry or mipmapped
    /// nine-slice is refused with `HUD_PACK invalid element=... rule=...`;
    /// there is no silent clamp and the HUD never substitutes different art.
    pub fn load(root: &Path, textures: &mut Vec<TextureImage>) -> Result<Self, PackError> {
        let manifest_path = root.join("manifest.json");
        let text = std::fs::read_to_string(&manifest_path).map_err(|error| PackError::Io {
            path: manifest_path.display().to_string(),
            error: error.to_string(),
        })?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| PackError::Manifest(error.to_string()))?;
        let manifest = super::manifest::parse(root, &value)?;
        let validation = super::manifest::validate(&manifest);
        crate::dev_log!("{}", validation.summary());
        for issue in &validation.errors {
            crate::dev_log!("HUD_PACK_VALIDATE ERROR {issue}");
        }
        for issue in &validation.warnings {
            crate::dev_log!("HUD_PACK_VALIDATE WARN {issue}");
        }
        if let Some(first) = validation.first_error() {
            return Err(PackError::Invalid {
                element: first.element.clone(),
                rule: first.rule.to_string(),
                detail: first.detail.clone(),
            });
        }

        // The packaged bytes are checked before any art is registered: a
        // truncated or edited asset is a loud `HUD_PACK hash=` error, never a
        // HUD that silently draws different pixels than the manifest describes.
        let hashes = verify_pack(root)?;
        if hashes.mismatched > 0 {
            let declared = declared_pack_files(&value);
            for mismatch in &hashes.mismatches {
                crate::dev_log!(
                    "HUD_PACK hash_mismatch={} expected={} actual={}",
                    mismatch.file,
                    mismatch.expected,
                    mismatch.actual
                );
            }
            if let Some(first) = hashes
                .mismatches
                .iter()
                .find(|mismatch| declared.contains(&normalize_pack_file(&mismatch.file)))
            {
                return Err(PackError::Hash {
                    file: first.file.clone(),
                    expected: first.expected.clone(),
                    actual: first.actual.clone(),
                });
            }
        }
        if !hashes.sums_present {
            crate::dev_log!("HUD_PACK hash=unverified root={}", root.display());
        }

        // Bundled font files must be readable before the pack binds.
        for file in &manifest.font_files {
            let direct = root.join(file);
            let nested = root.join("fonts").join(file);
            if !direct.is_file() && !nested.is_file() {
                return Err(PackError::Missing {
                    file: format!("fonts/{file}"),
                });
            }
        }

        // Every atlas page is decoded once, before the assets that reference it.
        let mut atlases = Vec::new();
        let mut atlas_pages: HashMap<String, (usize, u32, u32)> = HashMap::new();
        let mut file_textures: HashMap<String, usize> = HashMap::new();
        for decl in manifest.atlases.iter().chain(manifest.command_atlas.iter()) {
            let (texture, page_w, page_h) =
                load_atlas_page(root, decl, textures, &mut file_textures)?;
            atlases.push(AtlasInfo {
                id: decl.id.clone(),
                file: decl.file.clone(),
                w: page_w,
                h: page_h,
                padding: decl.padding,
                gutter: decl.gutter,
                entries: decl.entries.len(),
                texture,
            });
            atlas_pages.insert(decl.id.clone(), (texture, page_w, page_h));
        }
        let command_atlas = manifest.command_atlas.as_ref().and_then(|decl| {
            atlas_pages
                .get(&decl.id)
                .map(|(texture, _, _)| (*texture, decl.id.clone()))
        });

        let mut assets = Vec::with_capacity(manifest.assets.len());
        let mut by_name = HashMap::with_capacity(manifest.assets.len());
        for declaration in &manifest.assets {
            let path = root.join(&declaration.file);
            if !path.is_file() {
                return Err(PackError::Missing {
                    file: declaration.file.clone(),
                });
            }
            let bytes = std::fs::read(&path).map_err(|error| PackError::Io {
                path: path.display().to_string(),
                error: error.to_string(),
            })?;
            if let Some(expected) = &declaration.sha256 {
                let actual = crate::map_registry::sha256_hex(&bytes);
                if !actual.eq_ignore_ascii_case(expected) {
                    return Err(PackError::Hash {
                        file: declaration.file.clone(),
                        expected: expected.clone(),
                        actual,
                    });
                }
            }

            let (texture, uv_size, atlas_rect) = match &declaration.atlas {
                Some(page_id) => {
                    let Some((page_texture, page_w, page_h)) = atlas_pages.get(page_id) else {
                        return Err(PackError::Invalid {
                            element: declaration.id.clone(),
                            rule: "atlas-missing".to_string(),
                            detail: format!("atlas page {page_id:?} was not loaded"),
                        });
                    };
                    let (w, h) = png_dimensions(&bytes).ok_or_else(|| PackError::Texture {
                        file: declaration.file.clone(),
                        error: "not a PNG (missing IHDR)".to_string(),
                    })?;
                    if w != declaration.w || h != declaration.h {
                        return Err(PackError::Size {
                            file: declaration.file.clone(),
                            manifest: [declaration.w, declaration.h],
                            actual: [w, h],
                        });
                    }
                    // The rect is the manifest's, validated against the page.
                    let rect = declaration
                        .atlas_rect
                        .or_else(|| atlas_entry_rect(&manifest, page_id, &declaration.id))
                        .ok_or_else(|| PackError::Invalid {
                            element: declaration.id.clone(),
                            rule: "atlas-entry-missing".to_string(),
                            detail: format!("no entry in atlas {page_id:?}"),
                        })?;
                    (
                        *page_texture,
                        [*page_w as f32, *page_h as f32],
                        Some(rect),
                    )
                }
                None => {
                    let texture = if let Some(texture) = file_textures.get(&declaration.file) {
                        *texture
                    } else {
                        let image =
                            crate::ui_atlas::decode_png(&bytes).map_err(|error| PackError::Texture {
                                file: declaration.file.clone(),
                                error: error.to_string(),
                            })?;
                        if image.width != declaration.w || image.height != declaration.h {
                            return Err(PackError::Size {
                                file: declaration.file.clone(),
                                manifest: [declaration.w, declaration.h],
                                actual: [image.width, image.height],
                            });
                        }
                        let texture = textures.len();
                        textures.push(image.to_texture());
                        file_textures.insert(declaration.file.clone(), texture);
                        texture
                    };
                    (
                        texture,
                        [declaration.w as f32, declaration.h as f32],
                        None,
                    )
                }
            };

            by_name.insert(declaration.id.clone(), assets.len());
            assets.push(HudAsset {
                name: declaration.id.clone(),
                file: declaration.file.clone(),
                w: declaration.w,
                h: declaration.h,
                insets: declaration.insets,
                states: declaration.states.clone(),
                atlas: declaration.atlas.clone(),
                atlas_category: declaration.atlas_category.clone(),
                atlas_rect,
                uv_size,
                kind: declaration.kind.clone(),
                alpha: declaration.alpha,
                colorspace: declaration.colorspace.clone(),
                scale: declaration.scale.clone(),
                critical: declaration.critical,
                sha256: declaration.sha256.clone(),
                mips: declaration.mips,
                texture,
            });
        }

        // Manifest audit: name every asset whose declared insets cannot be
        // sliced, once, at load. v1's 11 known rows are drawn as single images;
        // v2 refuses such a pack in `validate` before this point.
        for asset in &assets {
            if !asset.insets_sliceable() {
                log_insets_ignored(asset);
            }
        }

        // Measure the topbar plate's flat column (min luminance variance over
        // the middle 60% of the element). Stretching this 1 px column extends
        // the bar without touching the authored ornaments at ~10%/~90%.
        let mut topbar_flat_u = 0.5;
        if let Some(asset) = assets
            .iter()
            .find(|asset| asset.name == "topbar_plate_normal_720x32")
            && let Some(texture) = textures.get(asset.texture)
            && let Some((fraction, _)) = flat_column(asset, texture)
        {
            topbar_flat_u = fraction;
        }
        // Pack-authored base colours for the two edge-anchored bars: the
        // centre of the console mid art and the measured flat topbar column.
        let mut console_base = [0.09f32, 0.085, 0.075, 1.0];
        if let Some(asset) = assets
            .iter()
            .find(|asset| asset.name == "console_left_mid_normal_256x288")
            && let Some(texture) = textures.get(asset.texture)
            && let Some((_, color)) = flat_column(asset, texture)
        {
            console_base = [color[0], color[1], color[2], 1.0];
        }
        let mut topbar_base = [0.08f32, 0.075, 0.065, 1.0];
        if let Some(asset) = assets
            .iter()
            .find(|asset| asset.name == "topbar_plate_normal_720x32")
            && let Some(texture) = textures.get(asset.texture)
            && let Some((_, color)) = flat_column(asset, texture)
        {
            topbar_base = [color[0], color[1], color[2], 1.0];
        }

        // Command-icon cells. v1 declares them in `atlas.commandbuttons`; a v2
        // component pack may have no command sheet at all (empty map is valid).
        let mut cells = HashMap::new();
        if let Some((texture, page_id)) = &command_atlas
            && let Some(decl) = manifest.command_atlas.as_ref()
        {
            let (_, page_w, page_h) = atlas_pages.get(page_id).copied().unwrap_or((*texture, 0, 0));
            for entry in &decl.entries {
                cells.insert(
                    entry.id.clone(),
                    AtlasCell {
                        id: entry.id.clone(),
                        texture: *texture,
                        rect: [
                            entry.x as f32,
                            entry.y as f32,
                            entry.w as f32,
                            entry.h as f32,
                        ],
                        page: [page_w as f32, page_h as f32],
                    },
                );
            }
        }

        // The three authored faces: rasterise once per process, register
        // their glyph atlases as textures, and bind them so one HUD batch can
        // carry serif (names/tooltips), sans (actions) and mono (numbers).
        let mut font_textures = [0usize; 4];
        if let Some(fonts) = crate::hud::pack_fonts(root) {
            let base = textures.len();
            textures.push(fonts.primary.atlas.clone());
            textures.push(fonts.serif.atlas.clone());
            textures.push(fonts.sans.atlas.clone());
            textures.push(fonts.numeric.atlas.clone());
            font_textures = [base, base + 1, base + 2, base + 3];
            fonts.bind(font_textures);
        } else {
            crate::dev_log!("HUD_PACK fonts=missing root={}", root.display());
        }

        Ok(Self {
            root: root.to_path_buf(),
            format_version: manifest.format_version,
            theme: manifest.theme.clone(),
            design_height: manifest.design_height,
            reference_landscape: manifest.reference_landscape,
            reference_portrait: manifest.reference_portrait,
            safe_landscape: manifest.safe_landscape,
            safe_portrait: manifest.safe_portrait,
            card_columns: manifest.card_columns,
            card_rows: manifest.card_rows,
            card_cell: manifest.card_cell,
            card_gap: manifest.card_gap,
            group_count: manifest.group_count,
            group_cell: manifest.group_cell,
            atlas_columns: manifest.atlas_columns,
            atlas_rows: manifest.atlas_rows,
            atlas_cell_px: manifest.atlas_cell_px,
            atlases,
            assets,
            by_name,
            cells,
            validation,
            atlas_texture: command_atlas.map(|(texture, _)| texture),
            font_files: manifest.font_files,
            font_textures,
            hash_files: hashes.files,
            hashed_bytes: hashes.hashed_bytes,
            topbar_flat_u,
            console_base,
            topbar_base,
        })
    }

    /// Load and validate a manifest without registering any texture. Used by
    /// the pack builder/validator so a tool and the device share one contract.
    pub fn inspect(root: &Path) -> Result<(super::manifest::Manifest, super::manifest::ValidationReport), PackError> {
        let manifest_path = root.join("manifest.json");
        let text = std::fs::read_to_string(&manifest_path).map_err(|error| PackError::Io {
            path: manifest_path.display().to_string(),
            error: error.to_string(),
        })?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| PackError::Manifest(error.to_string()))?;
        let manifest = super::manifest::parse(root, &value)?;
        let report = super::manifest::validate(&manifest);
        Ok((manifest, report))
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

    /// Pack design scale for a viewport (diagnostics only).
    ///
    /// Landscape maps the 768 design height to the screen height (2414x1080 ->
    /// 1.40625, the manifest's own example). The portrait console is a stacked
    /// column bounded by the 1080-wide reference, so it maps the reference
    /// width instead; both ideas come from `manifest.json`
    /// (`design_height`, `reference_resolution`).
    ///
    /// **Not a geometry authority.** Draw geometry and hit rects use the one
    /// live scale (`mobile_hud::MobileHudLayout::scale`, published as
    /// `hud::HudLayout::scale`); this value only describes how pack art maps to
    /// its authored design space and is kept crate-private so a caller cannot
    /// draw the HUD at a scale the hit rects do not share (WC3-GAME-003).
    pub(crate) fn scale_for(&self, viewport: Vec2, portrait: bool) -> f32 {
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
        ui.textured_quad(asset.texture, rect, asset.uv(), tint);
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
        let [aw, ah] = asset.source_size();
        let [u0, v0, u1, v1] = asset.uv();
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
        let map_u = |fraction: f32| u0 + fraction * (u1 - u0);
        let map_v = |fraction: f32| v0 + fraction * (v1 - v0);
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
                    [
                        map_u(sx[col]),
                        map_v(sy[row]),
                        map_u(sx[col + 1]),
                        map_v(sy[row + 1]),
                    ],
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
        let [u0, v0, u1, v1] = asset.uv();
        for index in 0..tiles {
            let x = rect.x + index as f32 * tile_w;
            let width = tile_w.min(rect.right() - x);
            if width <= 0.0 {
                break;
            }
            let u_fraction = (width / tile_w).clamp(0.0, 1.0);
            ui.textured_quad(
                asset.texture,
                Rect::new(x, rect.y, width, rect.h),
                [u0, v0, u0 + (u1 - u0) * u_fraction, v1],
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
        let [u0, v0, u1, v1] = asset.uv();
        let span = u1 - u0;
        // Left ornament.
        ui.textured_quad(
            asset.texture,
            Rect::new(rect.x, rect.y, left, rect.h),
            [u0, v0, u0 + span * (al / aw), v1],
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
                [
                    u0 + span * (al / aw),
                    v0,
                    u0 + span * ((al + middle_src * u_span) / aw),
                    v1,
                ],
                tint,
            );
            x += width;
        }
        // Right ornament.
        ui.textured_quad(
            asset.texture,
            Rect::new(rect.right() - right, rect.y, right, rect.h),
            [u1 - span * (ar / aw), v0, u1, v1],
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
        let [u0, v0, u1, v1] = asset.uv();
        let span = u1 - u0;
        // Base: one flat column (a fraction of the element, not of the page)
        // stretched across the full bar.
        let step = span / width.max(1.0);
        ui.textured_quad(
            asset.texture,
            rect,
            [
                u0 + self.topbar_flat_u * span,
                v0,
                u0 + self.topbar_flat_u * span + step,
                v1,
            ],
            tint,
        );
        let plate_w = width * scale;
        if rect.w >= plate_w * 2.0 + 4.0 {
            ui.textured_quad(
                asset.texture,
                Rect::new(rect.x, rect.y, plate_w, rect.h),
                [u0, v0, u1, v1],
                tint,
            );
            ui.textured_quad(
                asset.texture,
                Rect::new(rect.right() - plate_w, rect.y, plate_w, rect.h),
                [u1, v0, u0, v1],
                tint,
            );
        } else {
            let half = rect.w * 0.5;
            ui.textured_quad(
                asset.texture,
                Rect::new(rect.x, rect.y, half, rect.h),
                [u0, v0, u1, v1],
                tint,
            );
            ui.textured_quad(
                asset.texture,
                Rect::new(rect.x + half, rect.y, half, rect.h),
                [u1, v0, u0, v1],
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
        let width = cell.page[0].max(1.0);
        let height = cell.page[1].max(1.0);
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
        let [aw, ah] = asset.source_size();
        let [ox, oy, _, _] = asset.rect();
        let (l, t, r, b) = (al * scale, at * scale, ar * scale, ab * scale);
        if rect.w <= l + r + 1.0 || rect.h <= t + b + 1.0 {
            return None;
        }
        let corner = |x: f32, y: f32, w: f32, h: f32, u: f32, v: f32, uw: f32, vh: f32| {
            (
                Rect::new(x, y, w, h),
                Rect::new(ox + u * aw, oy + v * ah, uw * aw, vh * ah),
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
    walk(value, path)
        .and_then(|item| item.as_f64())
        .map(|item| item as f32)
        .ok_or_else(|| PackError::Manifest(format!("missing number {}", path.join("."))))
}

fn integer(value: &serde_json::Value, path: &[&str]) -> Result<i64, PackError> {
    walk(value, path)
        .and_then(|item| item.as_i64())
        .ok_or_else(|| PackError::Manifest(format!("missing integer {}", path.join("."))))
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
    if list.len() < 2 {
        return Err(PackError::Manifest(format!(
            "pair {} has {} items",
            path.join("."),
            list.len()
        )));
    }
    Ok([
        list[0].as_u64().unwrap_or(0) as u32,
        list[1].as_u64().unwrap_or(0) as u32,
    ])
}

fn quad(value: &serde_json::Value, path: &[&str]) -> Result<[f32; 4], PackError> {
    let list = walk(value, path)
        .and_then(|item| item.as_array())
        .ok_or_else(|| PackError::Manifest(format!("missing quad {}", path.join("."))))?;
    if list.len() < 4 {
        return Err(PackError::Manifest(format!(
            "quad {} has {} items",
            path.join("."),
            list.len()
        )));
    }
    Ok([
        list[0].as_f64().unwrap_or(0.0) as f32,
        list[1].as_f64().unwrap_or(0.0) as f32,
        list[2].as_f64().unwrap_or(0.0) as f32,
        list[3].as_f64().unwrap_or(0.0) as f32,
    ])
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
    fn hud_pack_missing_file_is_loud() {
        let root = std::env::temp_dir().join("hud-pack-loud-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp");
        std::fs::create_dir_all(root.join("fonts")).expect("fonts dir");
        std::fs::write(root.join("fonts/f.ttf"), b"").expect("placeholder font");
        std::fs::write(
            root.join("manifest.json"),
            r#"{"design_height":768,"reference_resolution":{"landscape":[1920,1080],"portrait":[1080,1920]},"safe_area_1080p":{"landscape":[48,80,48,48],"portrait":[24,96,24,64]},"command_card":{"columns":4,"rows":3,"cell":[64,64],"gap":4},"control_groups":{"count":10,"cell":[44,44]},"atlas":{"commandbuttons":{"file":"a.png","size":[64,64],"cell":[64,64],"columns":1,"rows":1,"entries":[{"id":"x","x":0,"y":0,"w":64,"h":64}]}},"assets":[{"name":"a","file":"a.png","w":64,"h":64,"insets":{"l":0,"t":0,"r":0,"b":0},"states":[],"atlas":"standalone","mips":false}],"font_files":["f.ttf"]}"#,
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

    /// A manifest asset whose bytes differ from `SHA256SUMS.txt` fails the load
    /// with the file, the expected digest and the observed one.
    #[test]
    fn hud_pack_hash_mismatch_is_loud() {
        let root = std::env::temp_dir().join("hud-pack-hash-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp");
        std::fs::create_dir_all(root.join("fonts")).expect("fonts dir");
        std::fs::write(root.join("fonts/f.ttf"), b"").expect("font");
        std::fs::write(
            root.join("manifest.json"),
            r#"{"design_height":768,"reference_resolution":{"landscape":[1920,1080],"portrait":[1080,1920]},"safe_area_1080p":{"landscape":[48,80,48,48],"portrait":[24,96,24,64]},"command_card":{"columns":4,"rows":3,"cell":[64,64],"gap":4},"control_groups":{"count":10,"cell":[44,44]},"atlas":{"commandbuttons":{"file":"a.png","size":[64,64],"cell":[64,64],"columns":1,"rows":1,"entries":[{"id":"x","x":0,"y":0,"w":64,"h":64}]}},"assets":[{"name":"a","file":"a.png","w":64,"h":64,"insets":{"l":0,"t":0,"r":0,"b":0},"states":[],"atlas":"standalone","mips":false}],"font_files":["f.ttf"]}"#,
        )
        .expect("manifest");
        std::fs::write(root.join("a.png"), b"x").expect("tampered asset");
        let wrong = "0".repeat(64);
        std::fs::write(
            root.join("SHA256SUMS.txt"),
            format!("{wrong}  a.png\n"),
        )
        .expect("sums");
        let report = verify_pack(&root).expect("report");
        assert_eq!(report.files, 1);
        assert_eq!(report.mismatched, 1);
        let error = HudPack::load(&root, &mut Vec::new()).expect_err("tampered asset");
        assert!(
            error.to_string().starts_with("HUD_PACK hash=a.png expected=0000"),
            "unexpected error: {error}"
        );
        // A docs-only mismatch is reported, and it is not a declared asset:
        // packaging notes must not take the HUD down.
        std::fs::write(root.join("README.md"), b"notes").expect("readme");
        let good = crate::map_registry::sha256_hex(b"x");
        std::fs::write(
            root.join("SHA256SUMS.txt"),
            format!("{good}  a.png\n{wrong}  README.md\n"),
        )
        .expect("sums");
        let report = verify_pack(&root).expect("report");
        assert_eq!(report.files, 2);
        assert_eq!(report.mismatched, 1);
        assert_eq!(report.mismatches[0].file, "README.md");
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("manifest.json")).unwrap())
                .unwrap();
        let declared = declared_pack_files(&manifest);
        assert!(declared.contains("a.png"));
        assert!(declared.contains("fonts/f.ttf"));
        assert!(!declared.contains("README.md"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
