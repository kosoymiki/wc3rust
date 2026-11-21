//! HUD pack manifest: v1 (the shipped 92-asset final pack) and v2 (the
//! operator's `assets/hud` delivery), one parsed model and one strict
//! validator.
//!
//! The loader never trusts a declaration it has not checked. [`parse`] turns
//! either format into [`Manifest`]; [`validate`] runs every rule from
//! `hud.md` §9.3/§9.4 and returns a report. [`crate::hudskin::pack::HudPack::load`]
//! refuses the pack when the report carries an error, naming the element and
//! the rule: there is no silent clamp and no fallback to different art.
//!
//! v1 is the 2026-11-16 delivery (`schema_version: 1`, top-level composition,
//! `insets: {l,t,r,b}`, `font_files`). v2 is the design delivery
//! (`format_version: 2`, `theme`, `assets[].id/kind/alpha/color_space/scale`,
//! `nine_slice` objects, `fonts` with roles/glyphs, `atlases` pages). The two
//! share the runtime model but not the JSON shape.
//!
//! A v1 pack keeps its documented inset defects (11 rows whose declared insets
//! have no stretchable centre) as *warnings*: those rows are single images by
//! nature and are never sliced. A v2 pack gets no such tolerance - a bad inset
//! set is a load error.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use serde_json::Value;

use super::pack::PackError;

pub const FORMAT_VERSION_V1: u32 = 1;
pub const FORMAT_VERSION_V2: u32 = 2;
/// Largest page/asset dimension the runtime accepts (matches
/// `ui_atlas::MAX_ATLAS_DIMENSION` and every mobile GPU limit this project
/// targets).
pub const MAX_DIMENSION: u32 = 8192;

/// One asset declaration. v1 `name` and v2 `id` both land in [`Self::id`].
#[derive(Clone, Debug, PartialEq)]
pub struct AssetDecl {
    pub id: String,
    pub purpose: String,
    pub file: String,
    /// Declared source format (`png` for a packed element, `svg` for a donor
    /// source that the rasterizer converts first).
    pub format: String,
    pub w: u32,
    pub h: u32,
    /// `[left, top, right, bottom]` source pixels.
    pub insets: [f32; 4],
    /// `stretch` | `tile` | `none` (v2 `nine_slice.center`).
    pub center: String,
    pub states: Vec<String>,
    /// v2 optional vertical three-slice: `{axis:"vertical", top, bottom,
    /// width_fixed}`. The runtime draws it through `nine_slice` with `l=r=0`;
    /// the metadata keeps the authored caps explicit.
    pub three_slice: Option<Value>,
    /// v2 atlas page id (`None` for a standalone image).
    pub atlas: Option<String>,
    /// v1 category string (`ui_frames`, `icons`, ...); not a page.
    pub atlas_category: Option<String>,
    /// Explicit atlas rect `[x, y, w, h]`, when the manifest carries one.
    pub atlas_rect: Option<[u32; 4]>,
    pub mips: bool,
    pub alpha: bool,
    pub colorspace: String,
    pub scale: String,
    pub kind: String,
    pub critical: bool,
    pub sha256: Option<String>,
}

impl AssetDecl {
    /// A single image has no stretchable cell: all-zero insets, or (v1 only)
    /// a type that is an icon/strip by nature.
    pub fn single_image(&self) -> bool {
        if self.insets.iter().all(|value| *value <= 0.0) {
            return true;
        }
        let name = self.id.as_str();
        name.starts_with("icon_")
            || name.starts_with("tooltip_arrow_")
            || name.starts_with("upkeep_strip_")
            || name.starts_with("minimap_header_")
            || (self.atlas_category.as_deref() == Some("ui_frames")
                && (self.w < 40 || self.h < 40)
                && !name.starts_with("topbar_plate_"))
    }

    /// Inset geometry is usable: every value finite/non-negative and the
    /// centre cell has at least one pixel on both axes.
    pub fn insets_sliceable(&self) -> bool {
        let [l, t, r, b] = self.insets;
        l.is_finite()
            && t.is_finite()
            && r.is_finite()
            && b.is_finite()
            && l >= 0.0
            && t >= 0.0
            && r >= 0.0
            && b >= 0.0
            && l + r < self.w as f32
            && t + b < self.h as f32
    }

    /// The runtime draws this element as a nine-patch only when the manifest
    /// supplies a usable, non-zero inset set and the element is not a
    /// single-image type.
    pub fn sliced(&self) -> bool {
        self.insets_sliceable()
            && !self.single_image()
            && self.insets.iter().any(|value| *value > 0.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AtlasEntry {
    pub id: String,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AtlasDecl {
    pub id: String,
    pub file: String,
    pub w: u32,
    pub h: u32,
    /// Minimum gap to the page border.
    pub padding: u32,
    /// Minimum gap between two entries (donor field; defaults to `padding`).
    pub gutter: u32,
    pub mips: bool,
    pub entries: Vec<AtlasEntry>,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FontDecl {
    pub id: String,
    pub file: String,
    pub bundled: bool,
    pub roles: Vec<String>,
    pub glyphs: Vec<String>,
    pub sha256: Option<String>,
}

/// v2 top-level `nine_slice` defaults (the pack builder writes the values it
/// packed with, so a reader can explain the padding rule without guessing).
#[derive(Clone, Debug, PartialEq)]
pub struct NineSliceDefaults {
    pub bleed_px: u32,
    pub padding_px: u32,
    pub min_center_px: u32,
}

impl Default for NineSliceDefaults {
    fn default() -> Self {
        Self {
            bleed_px: 0,
            padding_px: 0,
            min_center_px: 1,
        }
    }
}

/// Parsed manifest, independent of the JSON flavour.
#[derive(Clone, Debug)]
pub struct Manifest {
    pub format_version: u32,
    /// `false` for a donor manifest without `format_version`/`schema_version`
    /// (accepted as v2-shaped, reported as a warning).
    pub version_declared: bool,
    pub name: String,
    pub version: String,
    pub style: String,
    pub license: String,
    pub notes: String,
    pub theme: String,
    pub design_height: f32,
    pub density: f32,
    pub reference_landscape: [u32; 2],
    pub reference_portrait: [u32; 2],
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
    /// `atlas.commandbuttons` (both formats): the command-icon sheet.
    pub command_atlas: Option<AtlasDecl>,
    /// v2 `atlases`: every page the assets reference.
    pub atlases: Vec<AtlasDecl>,
    pub assets: Vec<AssetDecl>,
    pub fonts: Vec<FontDecl>,
    /// Bundled font files in load order (v1 `font_files`, v2 bundled fonts).
    pub font_files: Vec<String>,
    /// v2 `states`: named state definitions, preserved for tooling.
    pub states: BTreeMap<String, Value>,
    pub nine_slice: NineSliceDefaults,
}

impl Manifest {
    pub fn legacy(&self) -> bool {
        self.format_version == FORMAT_VERSION_V1
    }

    pub fn asset(&self, id: &str) -> Option<&AssetDecl> {
        self.assets.iter().find(|asset| asset.id == id)
    }

    pub fn atlas(&self, id: &str) -> Option<&AtlasDecl> {
        self.atlases.iter().find(|atlas| atlas.id == id)
    }
}

/// One refused rule. `element` is the manifest id (never a file path alone),
/// so a device log names exactly what to fix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationIssue {
    pub element: String,
    pub rule: &'static str,
    pub detail: String,
}

impl std::fmt::Display for ValidationIssue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "element={} rule={} detail={}",
            self.element, self.rule, self.detail
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct ValidationReport {
    pub format_version: u32,
    pub theme: String,
    pub assets: usize,
    pub atlases: usize,
    pub atlas_assets: usize,
    pub fonts: usize,
    pub states: usize,
    pub sliced: usize,
    pub standalone: usize,
    pub errors: Vec<ValidationIssue>,
    pub warnings: Vec<ValidationIssue>,
}

impl ValidationReport {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn first_error(&self) -> Option<&ValidationIssue> {
        self.errors.first()
    }

    /// One greppable line: `HUD_PACK_VALIDATE ... errors=N warnings=M`.
    pub fn summary(&self) -> String {
        format!(
            "HUD_PACK_VALIDATE format={} theme={} assets={} atlases={} atlas_assets={} fonts={} states={} sliced={} standalone={} errors={} warnings={}",
            self.format_version,
            self.theme,
            self.assets,
            self.atlases,
            self.atlas_assets,
            self.fonts,
            self.states,
            self.sliced,
            self.standalone,
            self.errors.len(),
            self.warnings.len()
        )
    }
}

impl std::fmt::Display for ValidationReport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(formatter, "{}", self.summary())?;
        for issue in &self.errors {
            writeln!(formatter, "ERROR {issue}")?;
        }
        for issue in &self.warnings {
            writeln!(formatter, "WARN {issue}")?;
        }
        Ok(())
    }
}

// ------------------------------------------------------------------ parse

/// Parse a v1 or v2 manifest. `root` is the pack directory; it is only used to
/// decide whether a declared v2 font is bundled (a file inside `fonts/`).
pub fn parse(root: &Path, value: &Value) -> Result<Manifest, PackError> {
    let declared = value
        .get("format_version")
        .and_then(Value::as_u64)
        .or_else(|| value.get("schema_version").and_then(Value::as_u64));
    // A donor/design manifest may omit the version while using the v2 asset
    // shape (`id`/`path`/`width`/`height`). Accept it as v2 and report the
    // omission; a *declared* unsupported version is still a hard error.
    let format_version = match declared {
        Some(version) => version as u32,
        None if looks_like_v2(value) => FORMAT_VERSION_V2,
        None => FORMAT_VERSION_V1,
    };
    if format_version != FORMAT_VERSION_V1 && format_version != FORMAT_VERSION_V2 {
        return Err(PackError::UnsupportedVersion {
            found: format_version.to_string(),
            supported: format!("{FORMAT_VERSION_V1},{FORMAT_VERSION_V2}"),
        });
    }
    let version_declared = declared.is_some();
    if format_version == FORMAT_VERSION_V1 {
        let mut manifest = parse_v1(value)?;
        manifest.version_declared = version_declared;
        Ok(manifest)
    } else {
        let mut manifest = parse_v2(root, value)?;
        manifest.version_declared = version_declared;
        Ok(manifest)
    }
}

fn looks_like_v2(value: &Value) -> bool {
    let Some(asset) = value["assets"].as_array().and_then(|assets| assets.first()) else {
        return false;
    };
    asset.get("id").is_some()
        || asset.get("path").is_some()
        || asset.get("width").is_some()
        || asset.get("nine_slice").is_some()
}

fn parse_v1(value: &Value) -> Result<Manifest, PackError> {
    let design_height = number(value, &["design_height"]).unwrap_or(768.0);
    let reference = &value["reference_resolution"];
    let reference_landscape = pair_u32(reference, &["landscape"]).unwrap_or([1920, 1080]);
    let reference_portrait = pair_u32(reference, &["portrait"]).unwrap_or([1080, 1920]);
    let safe = &value["safe_area_1080p"];
    let safe_landscape = quad(safe, &["landscape"]).unwrap_or([0.0; 4]);
    let safe_portrait = quad(safe, &["portrait"]).unwrap_or([0.0; 4]);
    let card = &value["command_card"];
    let card_columns = integer(card, &["columns"]).unwrap_or(4) as usize;
    let card_rows = integer(card, &["rows"]).unwrap_or(3) as usize;
    let card_cell = pair_u32(card, &["cell"]).unwrap_or([64, 64])[0] as f32;
    let card_gap = number(card, &["gap"]).unwrap_or(4.0);
    let groups = &value["control_groups"];
    let group_count = integer(groups, &["count"]).unwrap_or(10) as usize;
    let group_cell = pair_u32(groups, &["cell"]).unwrap_or([44, 44])[0] as f32;

    let command_atlas = parse_command_atlas(&value["atlas"]["commandbuttons"])?;
    let (atlas_columns, atlas_rows, atlas_cell_px) = match &command_atlas {
        Some(_atlas) => {
            let columns = atlas_columns(&value["atlas"]["commandbuttons"]).unwrap_or(0);
            let rows = atlas_rows(&value["atlas"]["commandbuttons"]).unwrap_or(0);
            let cell = pair_u32(&value["atlas"]["commandbuttons"], &["cell"]).unwrap_or([64, 64]);
            (columns, rows, cell[0] as f32)
        }
        None => (0, 0, 64.0),
    };

    let mut assets = Vec::new();
    let declarations = value["assets"]
        .as_array()
        .ok_or_else(|| PackError::Manifest("assets is not an array".to_string()))?;
    for declaration in declarations {
        let id = text(declaration, &["name"]).unwrap_or_default();
        if id.is_empty() {
            return Err(PackError::Manifest("v1 asset without a name".to_string()));
        }
        let file = text(declaration, &["file"]).unwrap_or_default();
        let w = integer(declaration, &["w"]).unwrap_or(0) as u32;
        let h = integer(declaration, &["h"]).unwrap_or(0) as u32;
        let insets_value = &declaration["insets"];
        let insets = [
            number(insets_value, &["l"]).unwrap_or(0.0),
            number(insets_value, &["t"]).unwrap_or(0.0),
            number(insets_value, &["r"]).unwrap_or(0.0),
            number(insets_value, &["b"]).unwrap_or(0.0),
        ];
        let states = string_list(&declaration["states"]);
        let category = text(declaration, &["atlas"])
            .filter(|value| !value.is_empty() && *value != "standalone")
            .map(str::to_string);
        let mips = declaration["mips"].as_bool().unwrap_or(false);
        let kind = infer_kind(&id, category.as_deref());
        assets.push(AssetDecl {
            id: id.to_string(),
            purpose: text(declaration, &["purpose"]).unwrap_or_default().to_string(),
            file: file.to_string(),
            format: "png".to_string(),
            w,
            h,
            insets,
            center: if insets.iter().any(|value| *value > 0.0) {
                "stretch".to_string()
            } else {
                "none".to_string()
            },
            states,
            three_slice: None,
            atlas: None,
            atlas_category: category,
            atlas_rect: None,
            mips,
            alpha: true,
            colorspace: "sRGB".to_string(),
            scale: "1x".to_string(),
            kind,
            critical: true,
            sha256: None,
        });
    }

    let mut fonts = Vec::new();
    let mut font_files = Vec::new();
    for file in string_list(&value["font_files"]) {
        if file.is_empty() {
            continue;
        }
        font_files.push(file.clone());
        fonts.push(FontDecl {
            id: file
                .rsplit('/')
                .next()
                .unwrap_or(&file)
                .trim_end_matches(".ttf")
                .trim_end_matches(".TTF")
                .to_string(),
            file,
            bundled: true,
            roles: Vec::new(),
            glyphs: Vec::new(),
            sha256: None,
        });
    }

    Ok(Manifest {
        format_version: FORMAT_VERSION_V1,
        version_declared: true,
        name: text(value, &["project"]).unwrap_or_default().to_string(),
        version: String::new(),
        style: String::new(),
        license: String::new(),
        notes: text(value, &["notes"]).unwrap_or_default().to_string(),
        theme: text(value, &["project"])
            .unwrap_or("legacy")
            .to_string(),
        design_height,
        density: 1.0,
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
        command_atlas,
        atlases: Vec::new(),
        assets,
        fonts,
        font_files,
        states: BTreeMap::new(),
        nine_slice: NineSliceDefaults {
            bleed_px: 0,
            padding_px: 0,
            min_center_px: 1,
        },
    })
}

fn parse_v2(root: &Path, value: &Value) -> Result<Manifest, PackError> {
    let theme = text(value, &["theme"]).unwrap_or("classic_fantasy").to_string();
    let density = number(value, &["density"]).unwrap_or_else(|| {
        let px = pair_u32(&value["reference"], &["landscape_px"]).unwrap_or([0, 0]);
        let dp = pair_u32(&value["reference"], &["landscape_dp"]).unwrap_or([0, 0]);
        if dp[1] > 0 {
            px[1] as f32 / dp[1] as f32
        } else {
            1.0
        }
    });
    let reference_landscape =
        pair_u32(&value["reference"], &["landscape_px"]).unwrap_or([1920, 1080]);
    let reference_portrait =
        pair_u32(&value["reference"], &["portrait_px"]).unwrap_or([1080, 1920]);
    let dp_landscape = pair_u32(&value["reference"], &["landscape_dp"]).unwrap_or([0, 0]);
    let design_height = number(value, &["design_height"]).unwrap_or_else(|| {
        if dp_landscape[1] > 0 {
            dp_landscape[1] as f32
        } else {
            768.0
        }
    });
    let (safe_landscape, safe_portrait) =
        if let (Some(landscape), Some(portrait)) = (
            quad(&value["safe_area_1080p"], &["landscape"]),
            quad(&value["safe_area_1080p"], &["portrait"]),
        ) {
            (landscape, portrait)
        } else {
            let to_px = |quad: [f32; 4]| quad.map(|value| value * density);
            (
                quad(&value["safe_area_dp"], &["landscape"])
                    .map(to_px)
                    .unwrap_or([0.0; 4]),
                quad(&value["safe_area_dp"], &["portrait"])
                    .map(to_px)
                    .unwrap_or([0.0; 4]),
            )
        };

    let card = &value["command_card"];
    let card_columns = integer(card, &["columns"]).unwrap_or(4) as usize;
    let card_rows = integer(card, &["rows"]).unwrap_or(3) as usize;
    let card_cell = pair_u32(card, &["cell"]).unwrap_or([64, 64])[0] as f32;
    let card_gap = number(card, &["gap"]).unwrap_or(4.0);
    let groups = &value["control_groups"];
    let group_count = integer(groups, &["count"]).unwrap_or(10) as usize;
    let group_cell = pair_u32(groups, &["cell"]).unwrap_or([44, 44])[0] as f32;

    let nine_slice_value = &value["nine_slice"];
    let nine_slice = NineSliceDefaults {
        bleed_px: integer(nine_slice_value, &["bleed_px"]).unwrap_or(0).max(0) as u32,
        padding_px: integer(nine_slice_value, &["padding_px"]).unwrap_or(2).max(0) as u32,
        min_center_px: integer(nine_slice_value, &["min_center_px"])
            .unwrap_or(1)
            .max(1) as u32,
    };

    let command_atlas = parse_command_atlas(&value["atlas"]["commandbuttons"])?;
    let (atlas_columns, atlas_rows, atlas_cell_px) = match &command_atlas {
        Some(_atlas) => {
            let columns = atlas_columns(&value["atlas"]["commandbuttons"]).unwrap_or(0);
            let rows = atlas_rows(&value["atlas"]["commandbuttons"]).unwrap_or(0);
            let cell = pair_u32(&value["atlas"]["commandbuttons"], &["cell"]).unwrap_or([64, 64]);
            (columns, rows, cell[0] as f32)
        }
        None => (0, 0, 64.0),
    };

    let mut atlases = Vec::new();
    match &value["atlases"] {
        Value::Object(map) => {
            for (id, item) in map {
                atlases.push(parse_atlas_page(id, item, nine_slice.padding_px)?);
            }
        }
        Value::Array(list) => {
            for item in list {
                let id = text(item, &["id"]).unwrap_or_default();
                if id.is_empty() {
                    return Err(PackError::Manifest(
                        "v2 atlases[] entry without an id".to_string(),
                    ));
                }
                atlases.push(parse_atlas_page(&id, item, nine_slice.padding_px)?);
            }
        }
        Value::Null => {}
        _ => {
            return Err(PackError::Manifest(
                "v2 atlases is neither an object nor an array".to_string(),
            ));
        }
    }

    let declarations = value["assets"]
        .as_array()
        .ok_or_else(|| PackError::Manifest("assets is not an array".to_string()))?;
    let mut assets = Vec::new();
    for declaration in declarations {
        let id = text(declaration, &["id"])
            .or_else(|| text(declaration, &["name"]))
            .unwrap_or_default();
        if id.is_empty() {
            return Err(PackError::Manifest("v2 asset without an id".to_string()));
        }
        // Donor/design aliases: `path` for `file`, `width`/`height` for
        // `w`/`h`. The canonical emitted keys stay `file`/`w`/`h`.
        let file = text(declaration, &["file"])
            .or_else(|| text(declaration, &["path"]))
            .unwrap_or_default();
        let w = integer(declaration, &["w"])
            .or_else(|| integer(declaration, &["width"]))
            .unwrap_or(0) as u32;
        let h = integer(declaration, &["h"])
            .or_else(|| integer(declaration, &["height"]))
            .unwrap_or(0) as u32;
        let (insets, center) = parse_nine_slice(&declaration["nine_slice"], &declaration["insets"]);
        let atlas = text(declaration, &["atlas"])
            .filter(|value| !value.is_empty() && *value != "standalone")
            .map(str::to_string);
        let atlas_rect = declaration
            .get("atlas_rect")
            .and_then(value_quad_u32)
            .or_else(|| {
                declaration
                    .get("atlas_location")
                    .and_then(value_quad_u32)
            });
        let states = string_list(&declaration["states"]);
        let mips = declaration["mips"].as_bool().unwrap_or(false);
        let alpha = declaration["alpha"].as_bool().unwrap_or(true);
        let colorspace = text(declaration, &["color_space"])
            .or_else(|| text(declaration, &["colorspace"]))
            .unwrap_or("sRGB")
            .to_string();
        let scale = match &declaration["scale"] {
            Value::String(text) => text.clone(),
            Value::Number(number) => format!("{number}x"),
            _ => "1x".to_string(),
        };
        let kind = text(declaration, &["kind"])
            .map(str::to_string)
            .unwrap_or_else(|| infer_kind(&id, None));
        let critical = declaration["critical"].as_bool().unwrap_or(true);
        let sha256 = text(declaration, &["sha256"])
            .filter(|value| is_sha256(value))
            .map(str::to_string);
        assets.push(AssetDecl {
            id: id.to_string(),
            purpose: text(declaration, &["purpose"]).unwrap_or_default().to_string(),
            file: file.to_string(),
            format: text(declaration, &["format"]).unwrap_or("png").to_string(),
            w,
            h,
            insets,
            center,
            states,
            three_slice: declaration
                .get("three_slice")
                .filter(|value| value.is_object())
                .cloned(),
            atlas,
            atlas_category: None,
            atlas_rect,
            mips,
            alpha,
            colorspace,
            scale,
            kind,
            critical,
            sha256,
        });
    }

    let mut fonts = Vec::new();
    let mut font_files = Vec::new();
    match &value["fonts"] {
        Value::Object(map) => {
            for (id, item) in map {
                let file = text(item, &["file"]).unwrap_or_default().to_string();
                let bundled = item["bundled"]
                    .as_bool()
                    .unwrap_or_else(|| !file.is_empty() && root.join(&file).is_file());
                if bundled && !file.is_empty() {
                    font_files.push(file.clone());
                }
                fonts.push(FontDecl {
                    id: id.clone(),
                    file,
                    bundled,
                    roles: string_list(&item["roles"]),
                    glyphs: string_list(&item["glyphs"]),
                    sha256: text(item, &["sha256"])
                        .filter(|value| is_sha256(value))
                        .map(str::to_string),
                });
            }
        }
        Value::Array(list) => {
            for item in list {
                let id = text(item, &["id"]).unwrap_or_default();
                let file = text(item, &["file"]).unwrap_or_default().to_string();
                let bundled = item["bundled"]
                    .as_bool()
                    .unwrap_or_else(|| !file.is_empty() && root.join(&file).is_file());
                if bundled && !file.is_empty() {
                    font_files.push(file.clone());
                }
                fonts.push(FontDecl {
                    id: id.to_string(),
                    file,
                    bundled,
                    roles: string_list(&item["roles"]),
                    glyphs: string_list(&item["glyphs"]),
                    sha256: text(item, &["sha256"])
                        .filter(|value| is_sha256(value))
                        .map(str::to_string),
                });
            }
        }
        Value::Null => {}
        _ => {
            return Err(PackError::Manifest(
                "v2 fonts is neither an object nor an array".to_string(),
            ));
        }
    }
    if font_files.is_empty() {
        // v1 compatibility key: a v2 pack may still list bundled faces here.
        for file in string_list(&value["font_files"]) {
            if !file.is_empty() {
                font_files.push(file);
            }
        }
    }

    let states = match &value["states"] {
        Value::Object(map) => map
            .iter()
            .map(|(id, definition)| (id.clone(), definition.clone()))
            .collect(),
        _ => BTreeMap::new(),
    };

    Ok(Manifest {
        format_version: FORMAT_VERSION_V2,
        version_declared: true,
        name: text(value, &["name"]).unwrap_or_default().to_string(),
        version: text(value, &["version"]).unwrap_or_default().to_string(),
        style: text(value, &["style"]).unwrap_or_default().to_string(),
        license: text(value, &["license"]).unwrap_or_default().to_string(),
        notes: text(value, &["notes"]).unwrap_or_default().to_string(),
        theme,
        design_height,
        density,
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
        command_atlas,
        atlases,
        assets,
        fonts,
        font_files,
        states,
        nine_slice,
    })
}

fn parse_nine_slice(nine: &Value, fallback: &Value) -> ([f32; 4], String) {
    let read = |value: &Value| -> [f32; 4] {
        [
            number(value, &["left"])
                .or_else(|| number(value, &["l"]))
                .unwrap_or(0.0),
            number(value, &["top"])
                .or_else(|| number(value, &["t"]))
                .unwrap_or(0.0),
            number(value, &["right"])
                .or_else(|| number(value, &["r"]))
                .unwrap_or(0.0),
            number(value, &["bottom"])
                .or_else(|| number(value, &["b"]))
                .unwrap_or(0.0),
        ]
    };
    let insets = match nine {
        Value::Object(_) => read(nine),
        Value::Array(list) if list.len() == 4 => [
            list[0].as_f64().unwrap_or(0.0) as f32,
            list[1].as_f64().unwrap_or(0.0) as f32,
            list[2].as_f64().unwrap_or(0.0) as f32,
            list[3].as_f64().unwrap_or(0.0) as f32,
        ],
        _ => match fallback {
            Value::Object(_) => read(fallback),
            Value::Array(list) if list.len() == 4 => [
                list[0].as_f64().unwrap_or(0.0) as f32,
                list[1].as_f64().unwrap_or(0.0) as f32,
                list[2].as_f64().unwrap_or(0.0) as f32,
                list[3].as_f64().unwrap_or(0.0) as f32,
            ],
            _ => [0.0; 4],
        },
    };
    let center = text(nine, &["center"])
        .map(str::to_string)
        .unwrap_or_else(|| {
            if insets.iter().any(|value| *value > 0.0) {
                "stretch".to_string()
            } else {
                "none".to_string()
            }
        });
    (insets, center)
}

fn parse_command_atlas(value: &Value) -> Result<Option<AtlasDecl>, PackError> {
    if value.is_null() {
        return Ok(None);
    }
    let Some(file) = text(value, &["file"]).filter(|file| !file.is_empty()) else {
        return Ok(None);
    };
    let (w, h) = if let Some(size) = pair_u32(value, &["size"]) {
        (size[0], size[1])
    } else {
        (
            integer(value, &["w"]).unwrap_or(0) as u32,
            integer(value, &["h"]).unwrap_or(0) as u32,
        )
    };
    let padding = integer(value, &["padding"]).unwrap_or(0).max(0) as u32;
    let mips = value["mips"].as_bool().unwrap_or(false);
    let mut entries = Vec::new();
    for entry in value["entries"].as_array().into_iter().flatten() {
        let id = text(entry, &["id"]).unwrap_or_default();
        let x = integer(entry, &["x"]).unwrap_or(0).max(0) as u32;
        let y = integer(entry, &["y"]).unwrap_or(0).max(0) as u32;
        let ew = integer(entry, &["w"]).unwrap_or(0).max(0) as u32;
        let eh = integer(entry, &["h"]).unwrap_or(0).max(0) as u32;
        if id.is_empty() {
            continue;
        }
        entries.push(AtlasEntry {
            id: id.to_string(),
            x,
            y,
            w: ew,
            h: eh,
        });
    }
    Ok(Some(AtlasDecl {
        id: "commandbuttons".to_string(),
        file: file.to_string(),
        w,
        h,
        padding,
        gutter: integer(value, &["gutter"]).unwrap_or(padding as i64).max(0) as u32,
        mips,
        entries,
        sha256: text(value, &["sha256"])
            .filter(|value| is_sha256(value))
            .map(str::to_string),
    }))
}

fn parse_atlas_page(id: &str, value: &Value, default_padding: u32) -> Result<AtlasDecl, PackError> {
    let file = text(value, &["file"]).unwrap_or_default().to_string();
    let (w, h) = if let Some(size) = pair_u32(value, &["size"]) {
        (size[0], size[1])
    } else {
        (
            integer(value, &["w"]).unwrap_or(0).max(0) as u32,
            integer(value, &["h"]).unwrap_or(0).max(0) as u32,
        )
    };
    let padding = integer(value, &["padding"])
        .unwrap_or(default_padding as i64)
        .max(0) as u32;
    let mut entries = Vec::new();
    for entry in value["entries"].as_array().into_iter().flatten() {
        let entry_id = text(entry, &["id"]).unwrap_or_default();
        if entry_id.is_empty() {
            return Err(PackError::Manifest(format!(
                "atlas {id} entry without an id"
            )));
        }
        entries.push(AtlasEntry {
            id: entry_id.to_string(),
            x: integer(entry, &["x"]).unwrap_or(0).max(0) as u32,
            y: integer(entry, &["y"]).unwrap_or(0).max(0) as u32,
            w: integer(entry, &["w"]).unwrap_or(0).max(0) as u32,
            h: integer(entry, &["h"]).unwrap_or(0).max(0) as u32,
        });
    }
    Ok(AtlasDecl {
        id: id.to_string(),
        file,
        w,
        h,
        padding,
        gutter: integer(value, &["gutter"]).unwrap_or(padding as i64).max(0) as u32,
        mips: value["mips"].as_bool().unwrap_or(false),
        entries,
        sha256: text(value, &["sha256"])
            .filter(|value| is_sha256(value))
            .map(str::to_string),
    })
}

// --------------------------------------------------------------- validate

/// Every strict rule from §9.3/§9.4 that can be decided from the manifest.
/// File existence, PNG dimensions and hashes are enforced by
/// [`crate::hudskin::pack::HudPack::load`] while it reads the bytes.
pub fn validate(manifest: &Manifest) -> ValidationReport {
    let mut report = ValidationReport {
        format_version: manifest.format_version,
        theme: manifest.theme.clone(),
        assets: manifest.assets.len(),
        atlases: manifest.atlases.len(),
        fonts: manifest.fonts.len(),
        states: manifest.states.len(),
        ..ValidationReport::default()
    };

    if !manifest.version_declared {
        report.warnings.push(issue(
            "manifest",
            "format-version-missing",
            "no format_version/schema_version declared; parsed as v2 because the assets use the v2 shape",
        ));
    }
    // A donor source may be SVG; the pipeline rasterises it before packing.
    for asset in &manifest.assets {
        if !matches!(
            asset.format.to_ascii_lowercase().as_str(),
            "png" | "svg" | "webp" | "jpg" | "jpeg"
        ) {
            report.warnings.push(issue(
                &asset.id,
                "format-unknown",
                format!("asset format {:?} is not a known source format", asset.format),
            ));
        }
    }
    if manifest.assets.is_empty() {
        report.errors.push(issue(
            "manifest",
            "assets-empty",
            "a pack must declare at least one asset",
        ));
    }
    if manifest.design_height <= 0.0 || !manifest.design_height.is_finite() {
        report.errors.push(issue(
            "manifest",
            "design-height",
            format!(
                "design_height={} must be positive",
                manifest.design_height
            ),
        ));
    }
    if manifest.reference_landscape[0] == 0 || manifest.reference_landscape[1] == 0 {
        report.warnings.push(issue(
            "manifest",
            "reference-resolution",
            "reference.landscape is zero; scale diagnostics are meaningless",
        ));
    }

    // Unique asset ids.
    let mut ids = HashSet::new();
    for asset in &manifest.assets {
        if !ids.insert(asset.id.clone()) {
            report.errors.push(issue(
                &asset.id,
                "duplicate-id",
                "two assets declare the same id",
            ));
        }
    }

    // Per-asset geometry, atlas association and mipmap rules.
    let mut atlas_entries: HashMap<(String, String), (u32, u32, u32, u32)> = HashMap::new();
    for atlas in manifest.atlases.iter().chain(manifest.command_atlas.iter()) {
        for entry in &atlas.entries {
            atlas_entries.insert(
                (atlas.id.clone(), entry.id.clone()),
                (entry.x, entry.y, entry.w, entry.h),
            );
        }
    }

    for asset in &manifest.assets {
        if asset.file.is_empty() {
            report.errors.push(issue(
                &asset.id,
                "file-empty",
                "asset has no file path",
            ));
        }
        if asset.w == 0 || asset.h == 0 {
            report.errors.push(issue(
                &asset.id,
                "size-zero",
                format!("declared size {}x{}", asset.w, asset.h),
            ));
        }
        if asset.w > MAX_DIMENSION || asset.h > MAX_DIMENSION {
            report.errors.push(issue(
                &asset.id,
                "size-limit",
                format!(
                    "{}x{} exceeds the {MAX_DIMENSION} px limit",
                    asset.w, asset.h
                ),
            ));
        }

        let [l, t, r, b] = asset.insets;
        if ![l, t, r, b].iter().all(|value| value.is_finite()) {
            report.errors.push(issue(
                &asset.id,
                "insets-finite",
                format!("insets [{l},{t},{r},{b}] must be finite"),
            ));
        } else if l < 0.0 || t < 0.0 || r < 0.0 || b < 0.0 {
            report.errors.push(issue(
                &asset.id,
                "insets-negative",
                format!("insets [{l},{t},{r},{b}] must be >= 0"),
            ));
        } else if asset.insets.iter().any(|value| *value > 0.0) {
            let cross_x = l + r >= asset.w as f32;
            let cross_y = t + b >= asset.h as f32;
            if cross_x || cross_y {
                let detail = format!(
                    "insets [{l},{t},{r},{b}] leave no centre cell on {}x{}",
                    asset.w, asset.h
                );
                if manifest.legacy() && asset.single_image() {
                    report.warnings.push(issue(
                        &asset.id,
                        "insets-ignored",
                        format!("{detail}; v1 known defect, drawn as one image"),
                    ));
                } else {
                    report.errors.push(issue(&asset.id, "insets-cross-centre", detail));
                }
            }
            if asset.center != "stretch" && asset.center != "tile" && asset.center != "vertical" {
                report.errors.push(issue(
                    &asset.id,
                    "nine-slice-center",
                    format!("nine_slice.center={:?} is not stretch|tile|vertical", asset.center),
                ));
            }
        }

        if let Some(three) = &asset.three_slice {
            let axis = three.get("axis").and_then(Value::as_str).unwrap_or("vertical");
            let top = three.get("top").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            let bottom = three.get("bottom").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            if axis != "vertical" {
                report.errors.push(issue(
                    &asset.id,
                    "three-slice-axis",
                    format!("three_slice.axis={axis:?}; only vertical is supported"),
                ));
            }
            if top <= 0.0 || bottom <= 0.0 || top + bottom >= asset.h as f32 {
                report.errors.push(issue(
                    &asset.id,
                    "three-slice-caps",
                    format!(
                        "top={top} bottom={bottom} leave no stretchable middle on h={}",
                        asset.h
                    ),
                ));
            }
        }

        if asset.mips {
            if asset.atlas.is_some() {
                report.errors.push(issue(
                    &asset.id,
                    "mips-atlas-bleed",
                    "a mipmapped asset cannot be packed into a shared atlas (mip bleeding)",
                ));
            }
            if asset.insets.iter().any(|value| *value > 0.0) {
                report.errors.push(issue(
                    &asset.id,
                    "mips-nine-slice",
                    "straight-alpha nine-slice panels must not be mipmapped",
                ));
            }
            if asset.w == 0 || asset.h == 0 {
                // already reported; avoid a second rule on the same element.
            }
        }

        if !matches!(
            asset.colorspace.to_ascii_lowercase().as_str(),
            "srgb" | "linear" | "rgba8" | "rgba8_srgb"
        ) {
            report.errors.push(issue(
                &asset.id,
                "colorspace",
                format!("unknown colour space {:?}", asset.colorspace),
            ));
        }

        match &asset.atlas {
            Some(page_id) => {
                report.atlas_assets += 1;
                let Some(page) = manifest.atlas(page_id) else {
                    report.errors.push(issue(
                        &asset.id,
                        "atlas-missing",
                        format!("atlas page {page_id:?} is not declared"),
                    ));
                    continue;
                };
                if page.w == 0 || page.h == 0 {
                    report.errors.push(issue(
                        &page_id,
                        "atlas-size-zero",
                        "atlas page size is zero",
                    ));
                }
                if page.mips {
                    report.errors.push(issue(
                        &page_id,
                        "atlas-mips",
                        "mipmapped atlas pages are unsupported (use standalone assets)",
                    ));
                }
                if page.file.is_empty() {
                    report.errors.push(issue(
                        &page_id,
                        "atlas-file-empty",
                        "atlas page has no file path",
                    ));
                }
                let rect = asset
                    .atlas_rect
                    .or_else(|| {
                        atlas_entries
                            .get(&(page.id.clone(), asset.id.clone()))
                            .copied()
                            .map(|(x, y, w, h)| [x, y, w, h])
                    });
                match rect {
                    Some([x, y, w, h]) => {
                        if w == 0 || h == 0 {
                            report.errors.push(issue(
                                &asset.id,
                                "atlas-entry-zero",
                                format!("atlas entry {w}x{h}"),
                            ));
                        }
                        if w != asset.w || h != asset.h {
                            report.errors.push(issue(
                                &asset.id,
                                "atlas-entry-size",
                                format!(
                                    "atlas entry {w}x{h} != declared source {}x{}",
                                    asset.w, asset.h
                                ),
                            ));
                        }
                        if x + w > page.w || y + h > page.h {
                            report.errors.push(issue(
                                &asset.id,
                                "atlas-uv-outside",
                                format!(
                                    "rect [{x},{y},{w},{h}] outside page {}x{}",
                                    page.w, page.h
                                ),
                            ));
                        }
                        if x < page.padding
                            || y < page.padding
                            || x + w + page.padding > page.w
                            || y + h + page.padding > page.h
                        {
                            report.errors.push(issue(
                                &asset.id,
                                "atlas-bleed-padding",
                                format!(
                                    "rect [{x},{y},{w},{h}] is closer than {} px to the page border",
                                    page.padding
                                ),
                            ));
                        }
                    }
                    None => report.errors.push(issue(
                        &asset.id,
                        "atlas-entry-missing",
                        format!("no atlas entry for {page_id:?}"),
                    )),
                }
            }
            None => report.standalone += 1,
        }

        if asset.sliced() {
            report.sliced += 1;
        }
    }

    // Atlas page rules: unique ids, entries inside, padding between entries.
    let mut atlas_ids = HashSet::new();
    for atlas in manifest.atlases.iter().chain(manifest.command_atlas.iter()) {
        if !atlas_ids.insert(atlas.id.clone()) {
            report.errors.push(issue(
                &atlas.id,
                "atlas-duplicate-id",
                "two atlas pages declare the same id",
            ));
        }
        if atlas.w > MAX_DIMENSION || atlas.h > MAX_DIMENSION {
            report.errors.push(issue(
                &atlas.id,
                "atlas-size-limit",
                format!("{}x{} exceeds {MAX_DIMENSION}", atlas.w, atlas.h),
            ));
        }
        let mut entry_ids = HashSet::new();
        for entry in &atlas.entries {
            if !entry_ids.insert(entry.id.clone()) {
                report.errors.push(issue(
                    &format!("{}:{}", atlas.id, entry.id),
                    "atlas-duplicate-entry",
                    "two atlas entries share an id",
                ));
            }
            if entry.w == 0 || entry.h == 0 {
                report.errors.push(issue(
                    &format!("{}:{}", atlas.id, entry.id),
                    "atlas-entry-zero",
                    "atlas entry has a zero dimension",
                ));
            }
            if entry.x + entry.w > atlas.w || entry.y + entry.h > atlas.h {
                report.errors.push(issue(
                    &format!("{}:{}", atlas.id, entry.id),
                    "atlas-entry-outside",
                    format!(
                        "rect [{},{},{},{}] outside {}x{}",
                        entry.x, entry.y, entry.w, entry.h, atlas.w, atlas.h
                    ),
                ));
            }
        }
        if atlas.gutter > 0 {
            for (index, first) in atlas.entries.iter().enumerate() {
                for second in atlas.entries.iter().skip(index + 1) {
                    let dx = gap(first.x, first.w, second.x, second.w);
                    let dy = gap(first.y, first.h, second.y, second.h);
                    if dx < atlas.gutter && dy < atlas.gutter {
                        report.errors.push(issue(
                            &format!("{}:{}", atlas.id, first.id),
                            "atlas-entry-overlap",
                            format!(
                                "gap to {} is {dx}x{dy} px, needs {} px on one axis",
                                second.id, atlas.gutter
                            ),
                        ));
                    }
                }
            }
        }
    }

    // Font declarations.
    let mut font_ids = HashSet::new();
    for font in &manifest.fonts {
        if !font_ids.insert(font.id.clone()) {
            report.errors.push(issue(
                &font.id,
                "font-duplicate-id",
                "two font entries share an id",
            ));
        }
        if font.bundled && font.file.is_empty() {
            report.errors.push(issue(
                &font.id,
                "font-file-empty",
                "bundled font without a file",
            ));
        }
        if font.roles.is_empty() {
            report.warnings.push(issue(
                &font.id,
                "font-roles-empty",
                "font declares no typography roles",
            ));
        }
        if font.glyphs.is_empty() {
            report.warnings.push(issue(
                &font.id,
                "font-glyphs-empty",
                "font declares no glyph blocks",
            ));
        }
        if let Some(sha256) = &font.sha256
            && !is_sha256(sha256)
        {
            report.errors.push(issue(
                &font.id,
                "font-sha256",
                "sha256 is not 64 hex characters",
            ));
        }
    }

    // Assets that point at a v2 page must reference a page, not a category.
    if manifest.format_version == FORMAT_VERSION_V2 {
        for asset in &manifest.assets {
            if asset.atlas_category.is_some() {
                report.errors.push(issue(
                    &asset.id,
                    "atlas-category-v2",
                    "v2 assets must name an atlas page or null, not a category",
                ));
            }
        }
    }

    report
}

fn gap(a_start: u32, a_len: u32, b_start: u32, b_len: u32) -> u32 {
    let a_end = a_start.saturating_add(a_len);
    let b_end = b_start.saturating_add(b_len);
    if a_end <= b_start {
        b_start - a_end
    } else if b_end <= a_start {
        a_start - b_end
    } else {
        0
    }
}

fn issue(element: &str, rule: &'static str, detail: impl Into<String>) -> ValidationIssue {
    ValidationIssue {
        element: element.to_string(),
        rule,
        detail: detail.into(),
    }
}

// ---------------------------------------------------------------- helpers

fn text<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    walk(value, path).and_then(Value::as_str)
}

fn number(value: &Value, path: &[&str]) -> Option<f32> {
    walk(value, path)
        .and_then(Value::as_f64)
        .map(|value| value as f32)
}

fn integer(value: &Value, path: &[&str]) -> Option<i64> {
    walk(value, path).and_then(Value::as_i64)
}

fn pair_u32(value: &Value, path: &[&str]) -> Option<[u32; 2]> {
    let list = walk(value, path)?.as_array()?;
    if list.len() < 2 {
        return None;
    }
    Some([
        list[0].as_u64().unwrap_or(0) as u32,
        list[1].as_u64().unwrap_or(0) as u32,
    ])
}

fn quad(value: &Value, path: &[&str]) -> Option<[f32; 4]> {
    let list = walk(value, path)?.as_array()?;
    if list.len() < 4 {
        return None;
    }
    Some([
        list[0].as_f64().unwrap_or(0.0) as f32,
        list[1].as_f64().unwrap_or(0.0) as f32,
        list[2].as_f64().unwrap_or(0.0) as f32,
        list[3].as_f64().unwrap_or(0.0) as f32,
    ])
}

fn value_quad_u32(value: &Value) -> Option<[u32; 4]> {
    let list = value.as_array()?;
    if list.len() < 4 {
        return None;
    }
    Some([
        list[0].as_u64().unwrap_or(0) as u32,
        list[1].as_u64().unwrap_or(0) as u32,
        list[2].as_u64().unwrap_or(0) as u32,
        list[3].as_u64().unwrap_or(0) as u32,
    ])
}

fn walk<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn string_list(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn infer_kind(id: &str, category: Option<&str>) -> String {
    let name = id.to_ascii_lowercase();
    let kind = if name.starts_with("icon_")
        || name.contains(".icon")
        || name.starts_with("resource.")
        || name.starts_with("infocard.")
    {
        "icon"
    } else if name.contains("button") {
        "button"
    } else if name.contains("tab") {
        "tab"
    } else if name.contains("tooltip") {
        "tooltip"
    } else if name.contains("minimap") {
        "minimap"
    } else if name.contains("portrait") {
        "portrait"
    } else if name.contains("panel") || name.contains("frame") || name.contains("dialog") {
        "frame"
    } else if name.contains("text") || name.contains("font") {
        "typography"
    } else {
        "element"
    };
    match category {
        Some(category) if kind == "element" => category.to_string(),
        _ => kind.to_string(),
    }
}

fn atlas_columns(value: &Value) -> Option<u32> {
    integer(value, &["columns"]).map(|columns| columns.max(0) as u32)
}

fn atlas_rows(value: &Value) -> Option<u32> {
    integer(value, &["rows"]).map(|rows| rows.max(0) as u32)
}
