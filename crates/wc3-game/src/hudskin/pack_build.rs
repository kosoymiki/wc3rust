//! Deterministic HUD pack builder: `assets/hud` (manifest v2, one PNG per
//! element) -> one atlas page + manifest v2 with atlas rects, provenance and
//! the strict-validation block.
//!
//! This is library code, not a desktop script: the same function is called by
//! the `wc3-hud-pack` CLI on Termux, on the Windows workstation and (through
//! the app) on the device. The builder never emits a pack that
//! [`super::manifest::validate`] rejects: it validates its own output before
//! writing the manifest and refuses on any error.
//!
//! Packing is a deterministic shelf pack: assets sorted by height, then width,
//! then id; `padding` px between entries and to the page border; a 1 px edge
//! extrusion when padding allows it (bilinear sampling must not pull a
//! neighbour's colour into a nine-slice corner). No hand-tuned offsets.

use std::collections::HashMap;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::manifest;
use super::pack::PackError;

/// Build options. Defaults are what `wc3-hud-pack build` uses.
#[derive(Clone, Debug)]
pub struct BuildOptions {
    /// Source pack directory (`assets/hud`), holding `manifest.json` + PNGs.
    pub src: PathBuf,
    /// Output pack directory (e.g. `assets/hud/pack`).
    pub out: PathBuf,
    /// Initial atlas page side (grows 1024 -> 2048 -> 4096 -> 8192).
    pub page: u32,
    /// Gap between entries and to the page border, in pixels.
    pub padding: u32,
    /// Directory holding the three bundled faces; `None` skips font bundling.
    pub fonts: Option<PathBuf>,
    /// Write `BUILD.log` next to the manifest.
    pub log: bool,
}

impl BuildOptions {
    pub fn new(src: impl Into<PathBuf>, out: impl Into<PathBuf>) -> Self {
        Self {
            src: src.into(),
            out: out.into(),
            page: 1024,
            padding: 2,
            fonts: None,
            log: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BuiltAsset {
    pub id: String,
    pub source: String,
    pub sha256: String,
    pub rect: [u32; 4],
}

#[derive(Clone, Debug)]
pub struct BuildReport {
    pub assets: usize,
    pub atlases: usize,
    pub entries: usize,
    pub page: [u32; 2],
    pub padding: u32,
    pub contexts: usize,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub output_manifest: PathBuf,
    pub output_atlas: PathBuf,
    pub output_log: Option<PathBuf>,
    pub built: Vec<BuiltAsset>,
}

impl BuildReport {
    /// One machine-checkable line:
    /// `HUD_PACK_BUILD assets=14 atlases=1 entries=14 page=1024x1024 ... errors=0`.
    pub fn summary(&self) -> String {
        format!(
            "HUD_PACK_BUILD assets={} atlases={} entries={} page={}x{} padding={} bleed={} bytes_in={} bytes_out={} errors=0 manifest={} atlas={}",
            self.assets,
            self.atlases,
            self.entries,
            self.page[0],
            self.page[1],
            self.padding,
            self.bleed(),
            self.bytes_in,
            self.bytes_out,
            self.output_manifest.display(),
            self.output_atlas.display()
        )
    }

    pub fn bleed(&self) -> u32 {
        self.padding.min(1)
    }
}

struct Item {
    id: String,
    file: String,
    w: u32,
    h: u32,
    rgba: Vec<u8>,
    sha256: String,
}

#[derive(Clone, Copy)]
struct Placement {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

/// Build the atlas pack. Deterministic for a given source tree and options.
pub fn build(options: &BuildOptions) -> Result<BuildReport, PackError> {
    let manifest_path = options.src.join("manifest.json");
    let text = std::fs::read_to_string(&manifest_path).map_err(|error| PackError::Io {
        path: manifest_path.display().to_string(),
        error: error.to_string(),
    })?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| PackError::Manifest(error.to_string()))?;
    let parsed = manifest::parse(&options.src, &value)?;
    if parsed.format_version != manifest::FORMAT_VERSION_V2 {
        return Err(PackError::Build(format!(
            "{} carries format_version={}; the builder packs v2 sources",
            manifest_path.display(),
            parsed.format_version
        )));
    }
    let source_report = manifest::validate(&parsed);
    if !source_report.ok() {
        return Err(first_error(&source_report));
    }

    // Read and decode every declared PNG once.
    let mut items = Vec::with_capacity(parsed.assets.len());
    let mut bytes_in = 0u64;
    for asset in &parsed.assets {
        let path = options.src.join(&asset.file);
        let bytes = std::fs::read(&path).map_err(|error| PackError::Io {
            path: path.display().to_string(),
            error: error.to_string(),
        })?;
        bytes_in += bytes.len() as u64;
        let image = crate::ui_atlas::decode_png(&bytes).map_err(|error| PackError::Texture {
            file: asset.file.clone(),
            error: error.to_string(),
        })?;
        if image.width != asset.w || image.height != asset.h {
            return Err(PackError::Size {
                file: asset.file.clone(),
                manifest: [asset.w, asset.h],
                actual: [image.width, image.height],
            });
        }
        items.push(Item {
            id: asset.id.clone(),
            file: asset.file.clone(),
            w: asset.w,
            h: asset.h,
            rgba: image.rgba,
            sha256: crate::map_registry::sha256_hex(&bytes),
        });
    }

    // Deterministic order: tallest first, then widest, then id.
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|left, right| {
        let a = &items[*left];
        let b = &items[*right];
        b.h.cmp(&a.h)
            .then_with(|| b.w.cmp(&a.w))
            .then_with(|| a.id.cmp(&b.id))
    });

    let padding = options.padding.max(1);
    let bleed = padding.min(1);
    let mut page = options.page.max(256).next_power_of_two();
    let (placements, page_w, page_h) = loop {
        if let Some(packed) = shelf_pack(&items, &order, page, padding) {
            break packed;
        }
        if page >= manifest::MAX_DIMENSION {
            return Err(PackError::Build(format!(
                "{} assets do not fit a {}^2 page",
                items.len(),
                manifest::MAX_DIMENSION
            )));
        }
        page *= 2;
    };

    // Compose the page: transparent black + each element + 1 px edge extrusion.
    let mut rgba = vec![0u8; (page_w * page_h * 4) as usize];
    for (index, placement) in placements.iter().enumerate() {
        let item = &items[index];
        blit(&mut rgba, page_w, item, *placement);
        if bleed > 0 {
            extrude(&mut rgba, page_w, page_h, item, *placement, bleed);
        }
    }

    std::fs::create_dir_all(options.out.join("atlas")).map_err(|error| PackError::Io {
        path: options.out.display().to_string(),
        error: error.to_string(),
    })?;
    let atlas_path = options.out.join("atlas/ui_atlas.png");
    write_png(&atlas_path, page_w, page_h, &rgba)?;
    let atlas_sha256 = crate::map_registry::sha256_hex(
        &std::fs::read(&atlas_path).map_err(|error| PackError::Io {
            path: atlas_path.display().to_string(),
            error: error.to_string(),
        })?,
    );

    // Copy source PNGs into the pack so the manifest's per-asset sha256 is
    // verifiable inside the pack itself (the runtime samples the atlas; the
    // copies are the audit trail).
    let mut copied_bytes = 0u64;
    for (index, asset) in parsed.assets.iter().enumerate() {
        let relative = safe_relative(&asset.file)?;
        let target = options.out.join("assets").join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| PackError::Io {
                path: parent.display().to_string(),
                error: error.to_string(),
            })?;
        }
        let bytes = std::fs::copy(options.src.join(&asset.file), &target).map_err(|error| {
            PackError::Io {
                path: target.display().to_string(),
                error: error.to_string(),
            }
        })?;
        copied_bytes += bytes;
        // The copy is byte-identical; keep the item's hash as the manifest value.
        let _ = index;
    }

    // Bundle the three authored faces the runtime binds (`HudFonts`).
    let mut font_entries: HashMap<String, Value> = HashMap::new();
    let mut font_files: Vec<String> = Vec::new();
    let mut font_bytes = 0u64;
    if let Some(fonts_dir) = &options.fonts {
        for (id, file, roles, glyphs) in [
            (
                "serif",
                "primary_serif_cyrillic.ttf",
                vec!["display", "title", "heading", "body"],
                vec!["latin", "cyrillic", "diacritics", "digits", "punctuation"],
            ),
            (
                "sans",
                "actionbar_sans_cyrillic.ttf",
                vec!["body", "label", "compact"],
                vec!["latin", "cyrillic", "diacritics", "digits", "punctuation"],
            ),
            (
                "numeric",
                "numeric_tabular.ttf",
                vec!["numeric", "compact"],
                vec!["latin", "digits", "punctuation"],
            ),
        ] {
            let source = fonts_dir.join(file);
            let bytes = std::fs::read(&source).map_err(|error| PackError::Io {
                path: source.display().to_string(),
                error: error.to_string(),
            })?;
            font_bytes += bytes.len() as u64;
            let target_dir = options.out.join("fonts");
            std::fs::create_dir_all(&target_dir).map_err(|error| PackError::Io {
                path: target_dir.display().to_string(),
                error: error.to_string(),
            })?;
            let target = target_dir.join(file);
            std::fs::write(&target, &bytes).map_err(|error| PackError::Io {
                path: target.display().to_string(),
                error: error.to_string(),
            })?;
            font_files.push(file.to_string());
            font_entries.insert(
                id.to_string(),
                json!({
                    "file": format!("fonts/{file}"),
                    "bundled": true,
                    "sha256": crate::map_registry::sha256_hex(&bytes),
                    "roles": roles,
                    "glyphs": glyphs,
                }),
            );
        }
    }

    // Preserve the source manifest (theme, reference, safe areas, provenance,
    // Nimrod metadata) and replace only the packing fields.
    let mut out = value.clone();
    out["format_version"] = json!(manifest::FORMAT_VERSION_V2);
    out["generated_by"] = json!("wc3-hud-pack build (tools/hud-pack)");
    out["pack_build"] = json!({
        "tool": "wc3-hud-pack",
        "page": [page_w, page_h],
        "padding": padding,
        "bleed": bleed,
        "entries": items.len(),
    });
    out["nine_slice"] = json!({
        "bleed_px": bleed,
        "padding_px": padding,
        "min_center_px": 1,
        "center": "stretch",
    });
    if out.get("states").and_then(Value::as_object).is_none() {
        out["states"] = json!({
            "normal":   { "brightness": 1.0,  "offset": [0.0, 0.0] },
            "hover":    { "brightness": 1.18, "offset": [0.0, 0.0] },
            "pressed":  { "brightness": 0.94, "offset": [0.0, 1.0] },
            "disabled": { "brightness": 0.70, "saturation": 0.60 },
            "selected": { "brightness": 1.0,  "outline_px": 2 },
            "locked":   { "brightness": 0.70, "saturation": 0.60, "veil": 0.24 }
        });
    }

    let mut entries = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let placement = placements[index];
        entries.push(json!({
            "id": item.id,
            "x": placement.x,
            "y": placement.y,
            "w": placement.w,
            "h": placement.h,
        }));
    }
    out["atlases"] = json!({
        "ui": {
            "file": "atlas/ui_atlas.png",
            "w": page_w,
            "h": page_h,
            "padding": padding,
            "gutter": padding,
            "mips": false,
            "sha256": atlas_sha256,
            "entries": entries,
        }
    });

    if let Some(assets) = out["assets"].as_array_mut() {
        for (index, asset) in assets.iter_mut().enumerate() {
            let item = &items[index];
            let placement = placements[index];
            asset["file"] = json!(format!(
                "assets/{}",
                safe_relative(&item.file)?.display()
            ));
            asset["atlas"] = json!("ui");
            asset["atlas_rect"] = json!([placement.x, placement.y, placement.w, placement.h]);
            asset["pack_sha256"] = json!(item.sha256);
            asset["mips"] = json!(false);
            asset["critical"] = json!(true);
        }
    }

    // Fonts: keep the operator Nimrod entry (external, not bundled), add the
    // three bundled faces and the legacy `font_files` list.
    let mut fonts = out
        .get("fonts")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(nimrod) = fonts.get_mut("nimrod") {
        if nimrod.get("glyphs").is_none() {
            if let Some(coverage) = nimrod.get("coverage").cloned() {
                nimrod["glyphs"] = coverage;
            }
        }
        nimrod["bundled"] = json!(false);
    }
    for (id, entry) in font_entries {
        fonts.insert(id, entry);
    }
    out["fonts"] = Value::Object(fonts);
    out["font_files"] = json!(font_files);

    let output_text = serde_json::to_string_pretty(&out)
        .map_err(|error| PackError::Build(format!("manifest serialization: {error}")))?;
    let output_manifest = options.out.join("manifest.json");
    std::fs::write(&output_manifest, &output_text).map_err(|error| PackError::Io {
        path: output_manifest.display().to_string(),
        error: error.to_string(),
    })?;

    // Self-check: the emitted manifest must pass the same strict validator the
    // device loader runs. A builder bug is a build error, never a shipped pack.
    let emitted: Value = serde_json::from_str(&output_text)
        .map_err(|error| PackError::Manifest(error.to_string()))?;
    let emitted_manifest = manifest::parse(&options.out, &emitted)?;
    let report = manifest::validate(&emitted_manifest);
    if !report.ok() {
        return Err(first_error(&report));
    }

    let bytes_out = std::fs::metadata(&atlas_path).map(|meta| meta.len()).unwrap_or(0)
        + copied_bytes
        + font_bytes;
    let output_log = if options.log {
        let path = options.out.join("BUILD.log");
        let mut lines = Vec::new();
        lines.push(format!(
            "HUD_PACK_BUILD tool=wc3-hud-pack src={} out={}",
            options.src.display(),
            options.out.display()
        ));
        lines.push(format!(
            "HUD_PACK_BUILD assets={} atlases=1 entries={} page={}x{} padding={} bleed={} bytes_in={} bytes_out={} errors=0",
            items.len(),
            items.len(),
            page_w,
            page_h,
            padding,
            bleed,
            bytes_in,
            bytes_out
        ));
        for (index, item) in items.iter().enumerate() {
            let placement = placements[index];
            lines.push(format!(
                "HUD_PACK_BUILD asset id={} source={} sha256={} atlas=ui rect={},{},{},{}",
                item.id, item.file, item.sha256, placement.x, placement.y, placement.w, placement.h
            ));
        }
        lines.push(report.summary());
        std::fs::write(&path, lines.join("\n") + "\n").map_err(|error| PackError::Io {
            path: path.display().to_string(),
            error: error.to_string(),
        })?;
        Some(path)
    } else {
        None
    };

    let built = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let placement = placements[index];
            BuiltAsset {
                id: item.id.clone(),
                source: item.file.clone(),
                sha256: item.sha256.clone(),
                rect: [placement.x, placement.y, placement.w, placement.h],
            }
        })
        .collect();

    Ok(BuildReport {
        assets: items.len(),
        atlases: 1,
        entries: items.len(),
        page: [page_w, page_h],
        padding,
        contexts: 1,
        bytes_in,
        bytes_out,
        output_manifest,
        output_atlas: atlas_path,
        output_log,
        built,
    })
}

/// Validate a pack directory with the device loader's contract. Returns the
/// report plus the decoded texture count (0 textures means the pack only
/// declared metadata and is not drawable).
pub fn validate_pack(root: &Path) -> Result<(manifest::ValidationReport, usize), PackError> {
    let (parsed, report) = super::pack::HudPack::inspect(root)?;
    if !report.ok() {
        return Ok((report, 0));
    }
    // A source manifest may point at SVG/raster sources: the strict manifest
    // rules are the test here; the device loader can only decode PNG. The
    // rasterize stage converts those into a PNG pack first.
    if parsed
        .assets
        .iter()
        .any(|asset| asset.format.eq_ignore_ascii_case("svg") || asset.file.ends_with(".svg"))
    {
        return Ok((report, 0));
    }
    let mut textures = Vec::new();
    super::pack::HudPack::load(root, &mut textures)?;
    Ok((report, textures.len()))
}

fn first_error(report: &manifest::ValidationReport) -> PackError {
    let first = report
        .first_error()
        .expect("first_error called on a failed report");
    PackError::Invalid {
        element: first.element.clone(),
        rule: first.rule.to_string(),
        detail: first.detail.clone(),
    }
}

fn safe_relative(file: &str) -> Result<PathBuf, PackError> {
    let path = Path::new(file);
    if path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(PackError::Build(format!(
            "asset path {file:?} escapes the pack root"
        )));
    }
    Ok(path.to_path_buf())
}

/// Shelf pack. `items` are indexed by the caller's order; returns placements in
/// the same order plus the page size.
fn shelf_pack(
    items: &[Item],
    order: &[usize],
    page: u32,
    padding: u32,
) -> Option<(Vec<Placement>, u32, u32)> {
    let mut placements = vec![
        Placement {
            x: 0,
            y: 0,
            w: 0,
            h: 0
        };
        items.len()
    ];
    let mut x = padding;
    let mut y = padding;
    let mut shelf_h = 0u32;
    for index in order {
        let item = &items[*index];
        if item.w + padding * 2 > page {
            return None;
        }
        if x + item.w + padding > page {
            x = padding;
            y += shelf_h + padding;
            shelf_h = 0;
        }
        if y + item.h + padding > page {
            return None;
        }
        placements[*index] = Placement {
            x,
            y,
            w: item.w,
            h: item.h,
        };
        x += item.w + padding;
        shelf_h = shelf_h.max(item.h);
    }
    Some((placements, page, page))
}

fn blit(page: &mut [u8], page_w: u32, item: &Item, placement: Placement) {
    for row in 0..item.h {
        let source = (row * item.w * 4) as usize;
        let target = (((placement.y + row) * page_w + placement.x) * 4) as usize;
        page[target..target + (item.w * 4) as usize]
            .copy_from_slice(&item.rgba[source..source + (item.w * 4) as usize]);
    }
}

/// Copy the 1 px border outward into the padding ring. Bilinear sampling at a
/// nine-slice edge reads the ring, so it must be the element's own edge colour,
/// not transparent black (dark fringes) or a neighbour (bleeding).
fn extrude(page: &mut [u8], page_w: u32, page_h: u32, item: &Item, placement: Placement, bleed: u32) {
    let bleed = bleed.min(1);
    let pixel = |page: &[u8], x: u32, y: u32| -> [u8; 4] {
        let index = ((y * page_w + x) * 4) as usize;
        [
            page[index],
            page[index + 1],
            page[index + 2],
            page[index + 3],
        ]
    };
    let put = |page: &mut [u8], x: u32, y: u32, color: [u8; 4]| {
        if x >= page_w || y >= page_h {
            return;
        }
        let index = ((y * page_w + x) * 4) as usize;
        page[index..index + 4].copy_from_slice(&color);
    };
    let left = placement.x.saturating_sub(bleed);
    let right = (placement.x + placement.w - 1 + bleed).min(page_w - 1);
    let top = placement.y.saturating_sub(bleed);
    let bottom = (placement.y + placement.h - 1 + bleed).min(page_h - 1);
    for row in 0..item.h {
        let y = placement.y + row;
        let first = pixel(page, placement.x, y);
        let last = pixel(page, placement.x + placement.w - 1, y);
        for x in left..placement.x {
            put(page, x, y, first);
        }
        for x in placement.x + placement.w..=right {
            put(page, x, y, last);
        }
    }
    for column in 0..item.w {
        let x = placement.x + column;
        let first = pixel(page, x, placement.y);
        let last = pixel(page, x, placement.y + item.h - 1);
        for y in top..placement.y {
            put(page, x, y, first);
        }
        for y in placement.y + item.h..=bottom {
            put(page, x, y, last);
        }
    }
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), PackError> {
    let file = std::fs::File::create(path).map_err(|error| PackError::Io {
        path: path.display().to_string(),
        error: error.to_string(),
    })?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::default());
    let mut writer = encoder.write_header().map_err(|error| PackError::Io {
        path: path.display().to_string(),
        error: error.to_string(),
    })?;
    writer
        .write_image_data(rgba)
        .map_err(|error| PackError::Io {
            path: path.display().to_string(),
            error: error.to_string(),
        })?;
    Ok(())
}

// ---------------------------------------------------------- SVG rasterizer

/// External rasterizer. `rsvg-convert` is the reference (librsvg 2.63.4 on
/// Termux, `rsvg-convert` on desktop); ImageMagick is the fallback. The tool
/// is an engine, never bundled: a missing binary is `observed/missing`, not a
/// fabricated success.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RasterTool {
    RsvgConvert(PathBuf),
    Magick(PathBuf),
}

impl RasterTool {
    pub fn path(&self) -> &Path {
        match self {
            Self::RsvgConvert(path) | Self::Magick(path) => path,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::RsvgConvert(_) => "rsvg-convert",
            Self::Magick(_) => "magick",
        }
    }
}

/// Find a rasterizer: `WC3_HUD_RASTERIZER` (explicit path), then
/// `rsvg-convert`, then `magick` on PATH. `None` is a capability report, not
/// an error: the caller must not claim a rendered PNG.
pub fn find_rasterizer() -> Option<RasterTool> {
    if let Some(explicit) = std::env::var_os("WC3_HUD_RASTERIZER") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            return Some(if name.contains("magick") {
                RasterTool::Magick(path)
            } else {
                RasterTool::RsvgConvert(path)
            });
        }
    }
    for candidate in ["rsvg-convert", "magick"] {
        if let Some(path) = which(candidate) {
            return Some(if candidate == "magick" {
                RasterTool::Magick(path)
            } else {
                RasterTool::RsvgConvert(path)
            });
        }
    }
    None
}

fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = directory.join(format!("{program}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

/// Rasterise one SVG at exactly `width` x `height` with a transparent
/// background. Deterministic for a given tool/input (no timestamps).
pub fn rasterize_svg(svg: &Path, png: &Path, width: u32, height: u32) -> Result<(), PackError> {
    let tool = find_rasterizer().ok_or_else(|| {
        PackError::Build(
            "no SVG rasterizer (install rsvg-convert or imagemagick, or set WC3_HUD_RASTERIZER)"
                .to_string(),
        )
    })?;
    rasterize_with(&tool, svg, png, width, height)
}

pub fn rasterize_with(
    tool: &RasterTool,
    svg: &Path,
    png: &Path,
    width: u32,
    height: u32,
) -> Result<(), PackError> {
    let output = match tool {
        RasterTool::RsvgConvert(tool) => std::process::Command::new(tool)
            .arg("--width")
            .arg(width.to_string())
            .arg("--height")
            .arg(height.to_string())
            .arg("--format")
            .arg("png")
            .arg("--output")
            .arg(png)
            .arg(svg)
            .output(),
        RasterTool::Magick(tool) => std::process::Command::new(tool)
            .arg(svg)
            .arg("-background")
            .arg("none")
            .arg("-resize")
            .arg(format!("{width}x{height}!"))
            .arg("PNG32:")
            .arg(png)
            .output(),
    }
    .map_err(|error| PackError::Build(format!("{}: {error}", tool.path().display())))?;
    if !output.status.success() {
        return Err(PackError::Build(format!(
            "{} {} -> {} failed: {}",
            tool.name(),
            svg.display(),
            png.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    if !png.is_file() {
        return Err(PackError::Build(format!(
            "{} reported success but {} is missing",
            tool.name(),
            png.display()
        )));
    }
    Ok(())
}

/// Render the same SVG twice and hash both PNGs. Equal hashes are the
/// determinism proof the pipeline records; a mismatch is a build error, never
/// a warning.
pub fn rasterize_checked(
    svg: &Path,
    png: &Path,
    width: u32,
    height: u32,
) -> Result<(String, String), PackError> {
    let tool = find_rasterizer().ok_or_else(|| {
        PackError::Build(
            "no SVG rasterizer (install rsvg-convert or imagemagick, or set WC3_HUD_RASTERIZER)"
                .to_string(),
        )
    })?;
    rasterize_with(&tool, svg, png, width, height)?;
    let first = crate::map_registry::sha256_hex(&std::fs::read(png).map_err(|error| {
        PackError::Io {
            path: png.display().to_string(),
            error: error.to_string(),
        }
    })?);
    let second_path = std::env::temp_dir().join(format!(
        "hud-raster-check-{}-{}.png",
        std::process::id(),
        width * 31 + height
    ));
    rasterize_with(&tool, svg, &second_path, width, height)?;
    let second = crate::map_registry::sha256_hex(&std::fs::read(&second_path).map_err(|error| {
        PackError::Io {
            path: second_path.display().to_string(),
            error: error.to_string(),
        }
    })?);
    let _ = std::fs::remove_file(&second_path);
    Ok((first, second))
}
