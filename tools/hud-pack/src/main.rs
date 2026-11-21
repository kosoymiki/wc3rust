//! `wc3-hud-pack` — one tool for the HUD asset pipeline:
//!
//! * `build` packs a v2 source (`assets/hud`) into one atlas page + manifest v2;
//! * `validate` runs the device loader's strict manifest rules and, when clean,
//!   decodes the pack exactly like the app does;
//! * `inspect` loads a pack and prints what the runtime sees;
//! * `fonts` prints the glyph-coverage table, tabular-digit proof and metrics
//!   for a TTF (Nimrod by default in the report).
//!
//! The build/validate path calls `wc3_game::hudskin::{manifest, pack_build}`:
//! the same Rust library the device links, so "works in the tool" and "works on
//! the phone" cannot drift.

use std::process::ExitCode;
use std::path::{Path, PathBuf};

use ab_glyph::{Font as AgFont, FontArc, GlyphId, ScaleFont};

use wc3_game::hud::debug::{self, DebugFrame, DebugKind, DebugLayers, DebugRect, HudFrameTrace};
use wc3_game::hud::{Font, Rect, UiBatch};
use wc3_game::hudskin::pack::HudPack;
use wc3_game::hudskin::pack_build::{self, BuildOptions};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        usage();
        return ExitCode::from(2);
    };
    let rest: Vec<String> = args.collect();
    let result = match command.as_str() {
        "build" => build(&rest),
        "validate" => validate(&rest),
        "inspect" => inspect(&rest),
        "fonts" => fonts(&rest),
        "rasterize" => rasterize(&rest),
        "overlay" => overlay(&rest),
        "-h" | "--help" | "help" => {
            usage();
            Ok(0)
        }
        other => Err(format!("unknown command {other:?}")),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("wc3-hud-pack: {error}");
            ExitCode::from(2)
        }
    }
}

fn usage() {
    eprintln!(
        "usage:
  wc3-hud-pack build [--src DIR] [--out DIR] [--page N] [--padding N] [--fonts DIR] [--no-fonts] [--no-log]
  wc3-hud-pack validate <pack-dir>
  wc3-hud-pack inspect <pack-dir>
  wc3-hud-pack fonts --font <file.ttf> [--font <file2.ttf>] [--out report.md]
  wc3-hud-pack rasterize --svg file.svg --out file.png --width W --height H
  wc3-hud-pack rasterize --manifest <pack-dir> --out <png-pack-dir> [--limit N]
  wc3-hud-pack overlay [--matrix analysis/hudskin/hud-layout-matrix.json] [--viewport device-2414x1080] [--out overlay.png]"
    );
}

// ------------------------------------------------------------------- build

fn build(args: &[String]) -> Result<u8, String> {
    let mut src = PathBuf::from("assets/hud");
    let mut out = PathBuf::from("assets/hud/pack");
    let mut page = 1024;
    let mut padding = 2;
    let mut fonts: Option<PathBuf> =
        Some(PathBuf::from("analysis/hudskin/final/wc3rust-hud-final/fonts"));
    let mut log = true;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--src" => {
                src = PathBuf::from(value(args, &mut index, "--src")?);
            }
            "--out" => {
                out = PathBuf::from(value(args, &mut index, "--out")?);
            }
            "--page" => {
                page = value(args, &mut index, "--page")?
                    .parse()
                    .map_err(|_| "--page expects an integer".to_string())?;
            }
            "--padding" => {
                padding = value(args, &mut index, "--padding")?
                    .parse()
                    .map_err(|_| "--padding expects an integer".to_string())?;
            }
            "--fonts" => {
                fonts = Some(PathBuf::from(value(args, &mut index, "--fonts")?));
            }
            "--no-fonts" => {
                fonts = None;
                index += 1;
            }
            "--no-log" => {
                log = false;
                index += 1;
            }
            other => return Err(format!("unknown build option {other:?}")),
        }
    }
    if let Some(dir) = &fonts
        && !dir.is_dir()
    {
        eprintln!(
            "wc3-hud-pack: fonts dir {} is missing; pack without bundled faces",
            dir.display()
        );
        fonts = None;
    }
    let options = BuildOptions {
        src,
        out,
        page,
        padding,
        fonts,
        log,
    };
    let report = pack_build::build(&options).map_err(|error| error.to_string())?;
    println!("{}", report.summary());
    println!(
        "HUD_PACK_BUILD_PROVENANCE assets={} page={}x{} padding={} bleed={}",
        report.entries, report.page[0], report.page[1], report.padding, report.bleed()
    );
    for asset in &report.built {
        println!(
            "HUD_PACK_BUILD_ASSET id={} source={} sha256={} rect={},{},{},{}",
            asset.id, asset.source, asset.sha256, asset.rect[0], asset.rect[1], asset.rect[2], asset.rect[3]
        );
    }
    if let Some(path) = &report.output_log {
        println!("HUD_PACK_BUILD_LOG {}", path.display());
    }
    Ok(0)
}

// ---------------------------------------------------------------- validate

fn validate(args: &[String]) -> Result<u8, String> {
    let root = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("assets/hud/pack"));
    let (report, textures) = pack_build::validate_pack(&root).map_err(|error| error.to_string())?;
    print!("{report}");
    println!(
        "HUD_PACK_TEXTURES root={} loaded={} errors={}",
        root.display(),
        textures,
        report.errors.len()
    );
    Ok(if report.ok() { 0 } else { 1 })
}

fn inspect(args: &[String]) -> Result<u8, String> {
    let root = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("assets/hud/pack"));
    let mut textures = Vec::new();
    let pack = HudPack::load(Path::new(&root), &mut textures).map_err(|error| error.to_string())?;
    println!(
        "HUD_PACK_INSPECT root={} format={} theme={} assets={} atlases={} atlas_assets={} fonts={} cells={} textures={} hash_files={} hashed_bytes={}",
        root.display(),
        pack.format_version,
        pack.theme,
        pack.assets.len(),
        pack.atlases.len(),
        pack.validation.atlas_assets,
        pack.font_files.len(),
        pack.cell_count(),
        textures.len(),
        pack.hash_files,
        pack.hashed_bytes
    );
    for atlas in &pack.atlases {
        println!(
            "HUD_PACK_ATLAS id={} file={} {}x{} padding={} entries={} texture={}",
            atlas.id, atlas.file, atlas.w, atlas.h, atlas.padding, atlas.entries, atlas.texture
        );
    }
    for asset in pack.assets.iter().take(40) {
        let [x, y, w, h] = asset.rect();
        println!(
            "HUD_PACK_ASSET id={} kind={} source={} atlas={} rect={},{},{},{} uv_size={}x{} texture={}",
            asset.name,
            asset.kind,
            asset.file,
            asset.atlas.as_deref().unwrap_or("standalone"),
            x,
            y,
            w,
            h,
            asset.uv_size[0],
            asset.uv_size[1],
            asset.texture
        );
    }
    Ok(0)
}

// --------------------------------------------------------------- rasterize

fn rasterize(args: &[String]) -> Result<u8, String> {
    let mut svg: Option<PathBuf> = None;
    let mut out = PathBuf::from("assets/hud/rasterized");
    let mut manifest: Option<PathBuf> = None;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut limit: usize = 0;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--svg" => svg = Some(PathBuf::from(value(args, &mut index, "--svg")?)),
            "--out" => out = PathBuf::from(value(args, &mut index, "--out")?),
            "--manifest" => manifest = Some(PathBuf::from(value(args, &mut index, "--manifest")?)),
            "--width" => {
                width = value(args, &mut index, "--width")?
                    .parse()
                    .map_err(|_| "--width expects an integer".to_string())?
            }
            "--height" => {
                height = value(args, &mut index, "--height")?
                    .parse()
                    .map_err(|_| "--height expects an integer".to_string())?
            }
            "--limit" => {
                limit = value(args, &mut index, "--limit")?
                    .parse()
                    .map_err(|_| "--limit expects an integer".to_string())?
            }
            other => return Err(format!("unknown rasterize option {other:?}")),
        }
    }
    let Some(tool) = pack_build::find_rasterizer() else {
        return Err("no SVG rasterizer found (install rsvg-convert or imagemagick, or set WC3_HUD_RASTERIZER)".to_string());
    };
    println!("HUD_RASTERIZER tool={} path={}", tool.name(), tool.path().display());

    if let Some(svg) = svg {
        if width == 0 || height == 0 {
            return Err("--svg needs --width and --height".to_string());
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let (first, second) = pack_build::rasterize_checked(&svg, &out, width, height)
            .map_err(|error| error.to_string())?;
        println!(
            "HUD_RASTER svg={} png={} size={}x{} sha256={} second={} deterministic={}",
            svg.display(),
            out.display(),
            width,
            height,
            first,
            second,
            first == second
        );
        return Ok(if first == second { 0 } else { 1 });
    }

    let Some(manifest) = manifest else {
        return Err("rasterize needs --svg or --manifest".to_string());
    };
    let manifest_path = if manifest.is_dir() {
        manifest.join("manifest.json")
    } else {
        manifest.clone()
    };
    let root = manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let text = std::fs::read_to_string(&manifest_path).map_err(|error| error.to_string())?;
    let mut value: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let assets = value["assets"]
        .as_array()
        .cloned()
        .ok_or_else(|| "manifest has no assets array".to_string())?;
    std::fs::create_dir_all(&out).map_err(|error| error.to_string())?;
    let mut rendered = Vec::new();
    let mut deterministic = 0usize;
    let mut total = 0usize;
    for asset in assets {
        if limit > 0 && total >= limit {
            break;
        }
        let id = asset["id"]
            .as_str()
            .or_else(|| asset["name"].as_str())
            .unwrap_or_default()
            .to_string();
        let file = asset["file"]
            .as_str()
            .or_else(|| asset["path"].as_str())
            .unwrap_or_default()
            .to_string();
        let w = asset["w"]
            .as_u64()
            .or_else(|| asset["width"].as_u64())
            .unwrap_or(0) as u32;
        let h = asset["h"]
            .as_u64()
            .or_else(|| asset["height"].as_u64())
            .unwrap_or(0) as u32;
        if file.is_empty() || w == 0 || h == 0 {
            return Err(format!("asset {id:?} lacks file/width/height"));
        }
        let safe = id
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '.' || character == '-' || character == '_' {
                    character
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let target = out.join(format!("{safe}.png"));
        let source = root.join(&file);
        let (first, second) = pack_build::rasterize_checked(&source, &target, w, h)
            .map_err(|error| error.to_string())?;
        let same = first == second;
        if same {
            deterministic += 1;
        }
        println!(
            "HUD_RASTER_ASSET id={} source={} png={} size={}x{} sha256={} second={} deterministic={}",
            id,
            file,
            target.display(),
            w,
            h,
            first,
            second,
            same
        );
        let mut entry = asset.clone();
        entry["file"] = serde_json::json!(target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default());
        entry["format"] = serde_json::json!("png");
        entry["w"] = serde_json::json!(w);
        entry["h"] = serde_json::json!(h);
        entry["sha256"] = serde_json::json!(first);
        entry["raster_source"] = serde_json::json!(file);
        entry["raster_sha256"] = serde_json::json!(first);
        entry["raster_tool"] = serde_json::json!(tool.name());
        rendered.push(entry);
        total += 1;
    }
    value["format_version"] = serde_json::json!(2);
    value["generated_by"] = serde_json::json!("wc3-hud-pack rasterize");
    value["rasterizer"] = serde_json::json!(tool.name());
    value["assets"] = serde_json::Value::Array(rendered);
    let output_manifest = out.join("manifest.json");
    std::fs::write(
        &output_manifest,
        serde_json::to_string_pretty(&value).map_err(|error| error.to_string())? + "\n",
    )
    .map_err(|error| error.to_string())?;
    println!(
        "HUD_RASTER_SUMMARY assets={} deterministic={} mismatched={} manifest={}",
        total,
        deterministic,
        total - deterministic,
        output_manifest.display()
    );
    Ok(if deterministic == total { 0 } else { 1 })
}

// ----------------------------------------------------------------- overlay

fn overlay(args: &[String]) -> Result<u8, String> {
    let mut matrix = PathBuf::from("analysis/hudskin/hud-layout-matrix.json");
    let mut viewport_label = "device-2414x1080".to_string();
    let mut out = PathBuf::from("analysis/agent-hud/hud-debug-overlay.png");
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--matrix" => matrix = PathBuf::from(value(args, &mut index, "--matrix")?),
            "--viewport" => viewport_label = value(args, &mut index, "--viewport")?,
            "--out" => out = PathBuf::from(value(args, &mut index, "--out")?),
            other => return Err(format!("unknown overlay option {other:?}")),
        }
    }
    let text = std::fs::read_to_string(&matrix).map_err(|error| error.to_string())?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let rows = value["rows"]
        .as_array()
        .ok_or_else(|| "matrix has no rows".to_string())?;
    let row = rows
        .iter()
        .find(|row| row["label"].as_str() == Some(viewport_label.as_str()))
        .ok_or_else(|| format!("viewport {viewport_label:?} not found in {}", matrix.display()))?;
    let framebuffer = row["framebuffer_px"]
        .as_array()
        .ok_or_else(|| "row has no framebuffer_px".to_string())?;
    let width = framebuffer[0].as_f64().unwrap_or(2414.0) as f32;
    let height = framebuffer[1].as_f64().unwrap_or(1080.0) as f32;
    let density = row["density_scale"].as_f64().unwrap_or(3.0) as f32;
    let viewport = Rect::new(0.0, 0.0, width, height);
    let console_px = row["console_px"].as_f64().unwrap_or(384.0) as f32;
    let topbar_px = row["topbar_dp"].as_f64().unwrap_or(30.0) as f32 * density;
    let safe_px = row["safe_px"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .map(|value| value.as_f64().unwrap_or(0.0) as f32)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![0.0; 4]);
    let safe = Rect::new(
        safe_px[0],
        safe_px[1],
        width - safe_px[0] - safe_px[2],
        height - safe_px[1] - safe_px[3],
    );
    let mut rects: Vec<DebugRect> = Vec::new();
    let mut push = |id: String, rect: Rect, kind: DebugKind, state: &str| {
        if rect.w > 0.0 && rect.h > 0.0 {
            rects.push(DebugRect {
                id,
                rect,
                kind,
                z: rects.len() as i32,
                state: state.to_string(),
            });
        }
    };
    push("top_bar".to_string(), Rect::new(0.0, 0.0, width, topbar_px), DebugKind::Layout, "normal");
    push(
        "console".to_string(),
        Rect::new(0.0, height - console_px, width, console_px),
        DebugKind::Layout,
        "normal",
    );
    push("safe_frame".to_string(), safe, DebugKind::Safe, "normal");
    push(
        "console_clip".to_string(),
        Rect::new(0.0, height - console_px, width, console_px),
        DebugKind::Clip,
        "clip",
    );
    if let Some(state) = row["states"].as_array().and_then(|states| states.first()) {
        if let Some(interactive) = state["interactive"].as_array() {
            for item in interactive {
                let id = item["id"].as_str().unwrap_or("interactive").to_string();
                if let Some(rect) = rect_from_json(&item["rect"]) {
                    push(id, rect, DebugKind::Hit, "normal");
                }
            }
        }
        if let Some(slots) = state["card_slots"].as_array() {
            for (index, item) in slots.iter().enumerate() {
                if let Some(rect) = rect_from_json(&item["rect"]) {
                    push(format!("card_slot_{index}"), rect, DebugKind::Hit, "normal");
                }
            }
        }
        if let Some(slots) = state["group_slots"].as_array() {
            for (index, item) in slots.iter().enumerate() {
                if let Some(rect) = rect_from_json(&item["rect"]) {
                    push(format!("group_slot_{index}"), rect, DebugKind::Hit, "normal");
                }
            }
        }
    }
    let font = std::fs::read("analysis/war3_full3/Fonts/NIM_____.ttf")
        .ok()
        .and_then(|bytes| Font::load_bytes(&bytes));
    let trace = HudFrameTrace {
        draws: 12,
        vertices: 1_284,
        textures: 3,
        white_vertices: 420,
        font_vertices: 360,
        minimap_vertices: 6,
        textured_draws: 8,
        layout_micros: 340,
        prepare_micros: 610,
    };
    let budget = wc3_game::hud::debug::HudBudget::default();
    let diagnostics = vec![
        "groups_open=false commands_open=false tooltip=false".to_string(),
        format!(
            "safe={:.0},{:.0},{:.0},{:.0}",
            safe_px[0], safe_px[1], safe_px[2], safe_px[3]
        ),
    ];
    let frame = DebugFrame {
        viewport,
        density,
        breakpoint: debug::breakpoint_for(viewport, density),
        theme: "classic_fantasy",
        screen: &viewport_label,
        rects: &rects,
        diagnostics: &diagnostics,
        trace: &trace,
        budget: &budget,
        safe: Some(safe),
    };
    let mut batch = UiBatch::default();
    debug::draw_debug(&mut batch, &frame, font.as_ref(), DebugLayers::ALL);
    let pixels = wc3_game::hudskin::soft_raster::rasterize(
        &batch,
        &[],
        font.as_ref(),
        None,
        width as u32,
        height as u32,
    );
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    wc3_game::hudskin::soft_raster::write_png(&out, width as u32, height as u32, &pixels)
        .map_err(|error| error.to_string())?;
    println!(
        "HUD_DEBUG_OVERLAY viewport={} size={}x{} rects={} layers={} out={}",
        viewport_label,
        width,
        height,
        rects.len(),
        DebugLayers::ALL.names().join(","),
        out.display()
    );
    Ok(0)
}

fn rect_from_json(value: &serde_json::Value) -> Option<Rect> {
    let list = value.as_array()?;
    if list.len() < 4 {
        return None;
    }
    Some(Rect::new(
        list[0].as_f64().unwrap_or(0.0) as f32,
        list[1].as_f64().unwrap_or(0.0) as f32,
        list[2].as_f64().unwrap_or(0.0) as f32,
        list[3].as_f64().unwrap_or(0.0) as f32,
    ))
}

// ------------------------------------------------------------------- fonts

const ASCII_PRINTABLE: (u32, u32) = (0x20, 0x7E);
const LATIN1: (u32, u32) = (0xA0, 0xFF);
const LATIN_EXT_A: (u32, u32) = (0x100, 0x17F);
const DIACRITICS: (u32, u32) = (0x300, 0x36F);
const CYRILLIC: (u32, u32) = (0x400, 0x4FF);
const CYRILLIC_SUPPLEMENT: (u32, u32) = (0x500, 0x52F);
const RUSSIAN_CYRILLIC: &str =
    "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯабвгдеёжзийклмнопрстуфхцчшщъыьэюя";
const UI_SPECIALS: &[char] = &[
    '№', '«', '»', '–', '—', '…', '’', '‘', '“', '”', '·', '×', '±', '≈', '≤', '≥', '%', '+',
    '−', '/', ':', '.', ',', '!', '?', '(', ')', '[', ']', '{', '}', '<', '>', '#', '@', '&', '*',
    '|', '=', '\\', '₽', '$', '€',
];

struct FontReport {
    path: PathBuf,
    sha256: String,
    blocks: Vec<BlockReport>,
    digits: Vec<(char, f32)>,
    tabular: bool,
    metrics: Metrics,
    missing: Vec<char>,
}

struct BlockReport {
    name: &'static str,
    required: usize,
    covered: usize,
    missing: Vec<char>,
}

struct Metrics {
    units_per_em: f32,
    ascent: f32,
    descent: f32,
    line_gap: f32,
    height_30px: f32,
    space_advance: f32,
}

fn fonts(args: &[String]) -> Result<u8, String> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut out: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--font" => {
                files.push(PathBuf::from(value(args, &mut index, "--font")?));
            }
            "--out" => {
                out = Some(PathBuf::from(value(args, &mut index, "--out")?));
            }
            other => return Err(format!("unknown fonts option {other:?}")),
        }
    }
    if files.is_empty() {
        files.push(PathBuf::from("analysis/war3_full3/Fonts/NIM_____.ttf"));
    }
    let mut reports = Vec::new();
    for file in &files {
        reports.push(analyse_font(file)?);
    }
    for report in &reports {
        print_font_report(report);
    }
    if let Some(path) = &out {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(path, markdown(&reports)).map_err(|error| error.to_string())?;
        println!("HUD_FONT_REPORT {}", path.display());
    }
    let failed = reports.iter().any(|report| {
        report.blocks.iter().any(|block| {
            matches!(
                block.name,
                "Basic Latin" | "Latin-1 Supplement" | "Latin Extended-A" | "Cyrillic (Russian)" | "Digits"
            ) && block.covered < block.required
        }) || !report.tabular
    });
    Ok(if failed { 1 } else { 0 })
}

fn analyse_font(path: &Path) -> Result<FontReport, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let font = FontArc::try_from_vec(bytes.clone())
        .map_err(|error| format!("{}: {error:?}", path.display()))?;
    let scaled = font.as_scaled(1000.0);
    let mut blocks = Vec::new();
    let mut missing_all = Vec::new();
    let mut block = |name: &'static str, chars: Vec<char>| {
        let mut missing = Vec::new();
        let mut covered = 0;
        for character in &chars {
            if covers(&font, *character) {
                covered += 1;
            } else {
                missing.push(*character);
            }
        }
        missing_all.extend(missing.iter().copied());
        BlockReport {
            name,
            required: chars.len(),
            covered,
            missing,
        }
    };
    blocks.push(block("Basic Latin", range(ASCII_PRINTABLE.0, ASCII_PRINTABLE.1)));
    blocks.push(block("Latin-1 Supplement", range(LATIN1.0, LATIN1.1)));
    blocks.push(block("Latin Extended-A", range(LATIN_EXT_A.0, LATIN_EXT_A.1)));
    blocks.push(block("Combining Diacritics", range(DIACRITICS.0, DIACRITICS.1)));
    blocks.push(block("Cyrillic (Russian)", RUSSIAN_CYRILLIC.chars().collect()));
    blocks.push(block("Cyrillic", range(CYRILLIC.0, CYRILLIC.1)));
    blocks.push(block(
        "Cyrillic Supplement",
        range(CYRILLIC_SUPPLEMENT.0, CYRILLIC_SUPPLEMENT.1),
    ));
    blocks.push(block("UI specials", UI_SPECIALS.to_vec()));
    blocks.push(block("Digits", range('0' as u32, '9' as u32)));

    let digits: Vec<(char, f32)> = ('0'..='9')
        .map(|digit| {
            let id = font.glyph_id(digit);
            (digit, scaled.h_advance(id))
        })
        .collect();
    let tabular = digits
        .iter()
        .map(|(_, advance)| *advance)
        .fold((f32::MAX, f32::MIN), |(low, high), advance| {
            (low.min(advance), high.max(advance))
        })
        .pipe(|(low, high)| high - low <= 0.01);

    let units_per_em = font.units_per_em().unwrap_or(1000.0);
    let ascent = font.ascent_unscaled();
    let descent = font.descent_unscaled();
    let line_gap = font.line_gap_unscaled();
    let scale = 30.0 / units_per_em.max(1.0);
    let metrics = Metrics {
        units_per_em,
        ascent,
        descent,
        line_gap,
        height_30px: (ascent - descent + line_gap) * scale,
        space_advance: scaled.h_advance(font.glyph_id(' ')),
    };
    missing_all.sort_unstable();
    missing_all.dedup();
    Ok(FontReport {
        path: path.to_path_buf(),
        sha256: wc3_game::map_registry::sha256_hex(&bytes),
        blocks,
        digits,
        tabular,
        metrics,
        missing: missing_all,
    })
}

fn covers(font: &FontArc, character: char) -> bool {
    font.glyph_id(character) != GlyphId(0)
}

fn range(start: u32, end: u32) -> Vec<char> {
    (start..=end).filter_map(char::from_u32).collect()
}

fn print_font_report(report: &FontReport) {
    println!(
        "HUD_FONT font={} sha256={}",
        report.path.display(),
        report.sha256
    );
    println!("{:<24} {:>8} {:>8} {:>8}  status", "block", "required", "covered", "missing");
    for block in &report.blocks {
        let status = if block.covered == block.required {
            "PASS"
        } else if block.covered == 0 {
            "FAIL"
        } else {
            "PARTIAL"
        };
        println!(
            "{:<24} {:>8} {:>8} {:>8}  {}",
            block.name,
            block.required,
            block.covered,
            block.missing.len(),
            status
        );
    }
    println!(
        "HUD_FONT_DIGITS tabular={} advances={:?}",
        report.tabular,
        report
            .digits
            .iter()
            .map(|(digit, advance)| format!("{digit}:{advance:.2}"))
            .collect::<Vec<_>>()
    );
    println!(
        "HUD_FONT_METRICS units_per_em={} ascent={} descent={} line_gap={} height_30px={:.2} space_advance_1000={:.2}",
        report.metrics.units_per_em,
        report.metrics.ascent,
        report.metrics.descent,
        report.metrics.line_gap,
        report.metrics.height_30px,
        report.metrics.space_advance
    );
    println!(
        "HUD_FONT_BREAK wrap=word space={} hyphen={} underscore={} line_height_30px={:.2}",
        covers_str(report, ' '),
        covers_str(report, '-'),
        covers_str(report, '_'),
        report.metrics.height_30px
    );
    println!(
        "HUD_FONT_COVERAGE font={} latin={}/{} latin_ext={}/{} diacritics={}/{} cyrillic_russian={}/{} cyrillic={}/{} cyrillic_supp={}/{} specials={}/{} digits={}/{} missing_total={}",
        report.path.display(),
        block(report, "Basic Latin").covered,
        block(report, "Basic Latin").required,
        block(report, "Latin-1 Supplement").covered,
        block(report, "Latin-1 Supplement").required,
        block(report, "Combining Diacritics").covered,
        block(report, "Combining Diacritics").required,
        block(report, "Cyrillic (Russian)").covered,
        block(report, "Cyrillic (Russian)").required,
        block(report, "Cyrillic").covered,
        block(report, "Cyrillic").required,
        block(report, "Cyrillic Supplement").covered,
        block(report, "Cyrillic Supplement").required,
        block(report, "UI specials").covered,
        block(report, "UI specials").required,
        block(report, "Digits").covered,
        block(report, "Digits").required,
        report.missing.len()
    );
    if !report.missing.is_empty() {
        let sample: Vec<String> = report
            .missing
            .iter()
            .take(32)
            .map(|character| format!("U+{:04X}", *character as u32))
            .collect();
        let specials: Vec<String> = UI_SPECIALS
            .iter()
            .filter(|character| report.missing.contains(character))
            .map(|character| format!("U+{:04X} ({character})", *character as u32))
            .collect();
        println!(
            "HUD_FONT_MISSING font={} count={} specials_missing={} sample={}",
            report.path.display(),
            report.missing.len(),
            specials.join(","),
            sample.join(",")
        );
    }
}

fn covers_str(report: &FontReport, character: char) -> &'static str {
    if report.missing.contains(&character) {
        "missing"
    } else {
        "covered"
    }
}

fn block<'a>(report: &'a FontReport, name: &str) -> &'a BlockReport {
    report
        .blocks
        .iter()
        .find(|block| block.name == name)
        .expect("block exists")
}

fn markdown(reports: &[FontReport]) -> String {
    let mut text = String::new();
    text.push_str("# HUD font coverage\n\n");
    text.push_str("Generated by `tools/hud-pack fonts` (same library the device links).\n\n");
    for report in reports {
        text.push_str(&format!("## {}\n\n", report.path.display()));
        text.push_str(&format!("- SHA-256: `{}`\n", report.sha256));
        text.push_str(&format!(
            "- units/em `{}`, ascent `{}`, descent `{}`, line gap `{}`, 30 px line box `{:.2}` px\n",
            report.metrics.units_per_em,
            report.metrics.ascent,
            report.metrics.descent,
            report.metrics.line_gap,
            report.metrics.height_30px
        ));
        let diacritics = block(report, "Combining Diacritics");
        text.push_str(&format!(
            "- combining diacritics: {}/{}; precomposed Latin-1/Extended-A: {}/{} and {}/{} (the runtime chains Nimrod -> pack serif -> pack sans -> system DejaVu for any codepoint Nimrod lacks)\n",
            diacritics.covered,
            diacritics.required,
            block(report, "Latin-1 Supplement").covered,
            block(report, "Latin-1 Supplement").required,
            block(report, "Latin Extended-A").covered,
            block(report, "Latin Extended-A").required
        ));
        text.push_str(&format!(
            "- digits: tabular **{}**; advances (per 1000 px): {}\n\n",
            report.tabular,
            report
                .digits
                .iter()
                .map(|(digit, advance)| format!("`{digit}`={advance:.2}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        text.push_str("| block | required | covered | missing | status |\n|---|---:|---:|---:|---|\n");
        for block in &report.blocks {
            let status = if block.covered == block.required {
                "PASS"
            } else if block.covered == 0 {
                "FAIL"
            } else {
                "PARTIAL"
            };
            text.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                block.name,
                block.required,
                block.covered,
                block.missing.len(),
                status
            ));
        }
        text.push('\n');
        if report.missing.is_empty() {
            text.push_str("Missing codepoints: none.\n\n");
        } else {
            text.push_str(&format!(
                "Missing codepoints ({}): {}\n\n",
                report.missing.len(),
                report
                    .missing
                    .iter()
                    .map(|character| format!("U+{:04X} ({character})", *character as u32))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    text
}

// ------------------------------------------------------------------ helpers

fn value(args: &[String], index: &mut usize, option: &str) -> Result<String, String> {
    *index += 1;
    let Some(value) = args.get(*index) else {
        return Err(format!("{option} expects a value"));
    };
    *index += 1;
    Ok(value.clone())
}

trait Pipe: Sized {
    fn pipe<T>(self, function: impl FnOnce(Self) -> T) -> T {
        function(self)
    }
}

impl<T> Pipe for T {}
