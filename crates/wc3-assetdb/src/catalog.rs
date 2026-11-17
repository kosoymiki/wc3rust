//! Full-corpus asset catalogs: every BLP, every MDX, icons and biome tiles.
//!
//! One pass over the provider's file list, one file in memory at a time. The
//! output is a set of exact counts plus per-file provenance rows (hashes,
//! header fields, resolved texture references) so a claim like "3,935 textures
//! validated" is backed by 3,935 rows rather than a sample.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io;
use std::path::Path;
use std::time::Instant;

use wc3_formats::mdx::MdxModel;

use crate::blp_report::{self, HueClass, TextureReport};
use crate::AssetDb;

#[derive(Debug, Clone, Default)]
pub struct CatalogOptions {
    pub limit: Option<usize>,
    pub skip_models: bool,
    pub skip_textures: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TextureTotals {
    pub files: usize,
    pub ok: usize,
    pub header_fail: usize,
    pub decode_fail: usize,
    pub mip_fail: usize,
    pub all_transparent: usize,
    pub constant_color: usize,
    pub with_alpha: usize,
    pub fully_opaque: usize,
    pub jpeg: usize,
    pub paletted: usize,
    pub bytes: u64,
    pub decoded_pixels: u64,
    pub mip_levels_decoded: u64,
    pub mip_levels_declared: u64,
    /// Files whose decoder returned fewer levels than the header declares.
    /// `wc3-formats::blp` currently returns mip 0 for BLP1 (both JPEG and
    /// paletted); the patch in `analysis/tech-lead/merge-assets.md` closes this
    /// and the post-patch gate requires `0`.
    pub mips_incomplete_files: usize,
    pub mips_complete_files: usize,
    pub mip_layout_bad: usize,
    pub min_side: u32,
    pub max_side: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ModelTotals {
    pub files: usize,
    pub parsed: usize,
    pub parse_fail: usize,
    pub texture_refs: usize,
    pub replaceable_refs: usize,
    pub unresolved_refs: usize,
    pub models_with_unresolved: usize,
    pub models_all_resolved: usize,
    pub geosets: usize,
    pub sequences: usize,
    pub nodes: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct IconTotals {
    pub btn: usize,
    pub disabled: usize,
    pub paired: usize,
    pub missing_disabled: usize,
    pub orphan_disabled: usize,
    pub ok: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Default)]
pub struct BiomeRow {
    pub biome: String,
    pub files: usize,
    pub ok: usize,
    pub failed: usize,
    pub bytes: u64,
    pub mean_rgb: [u8; 3],
    pub hues: BTreeMap<String, usize>,
    pub min_side: u32,
    pub max_side: u32,
}

#[derive(Debug, Clone)]
pub struct FailureRow {
    pub name: String,
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct TextureRow {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub mips: u8,
    pub compression: u32,
    pub alpha_bits: u8,
    pub source_len: usize,
    pub sha256_source: String,
    pub hue: &'static str,
    pub alpha_mean: u8,
    pub unique_colors: usize,
    pub ok: bool,
    pub error: String,
}

#[derive(Debug, Clone, Default)]
pub struct ModelRow {
    pub name: String,
    pub textures: usize,
    pub external: usize,
    pub replaceable: usize,
    pub unresolved: usize,
    pub geosets: usize,
    pub sequences: usize,
    pub nodes: usize,
    pub parsed: bool,
    pub error: String,
}

#[derive(Debug, Clone, Default)]
pub struct CatalogReport {
    pub files_scanned: usize,
    pub bytes_scanned: u64,
    pub textures: TextureTotals,
    pub models: ModelTotals,
    pub icons: IconTotals,
    pub biomes: Vec<BiomeRow>,
    pub texture_rows: Vec<TextureRow>,
    pub model_rows: Vec<ModelRow>,
    pub failures: Vec<FailureRow>,
    pub warnings: Vec<FailureRow>,
    pub elapsed_seconds: f64,
}

impl CatalogReport {
    pub fn texture_failures(&self) -> usize {
        self.textures.header_fail + self.textures.decode_fail + self.textures.mip_fail
    }

    pub fn all_ok(&self) -> bool {
        self.texture_failures() == 0
            && self.models.parse_fail == 0
            && self.models.unresolved_refs == 0
            && self.icons.failed == 0
    }
}

fn extension(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((_, ext)) => ext,
        None => "",
    }
}

fn is_icon_path(lower: &str) -> bool {
    lower.contains("replaceabletextures\\commandbuttons")
}

fn icon_is_disabled(lower: &str) -> bool {
    lower.contains("commandbuttonsdisabled")
}

/// Pair key for an icon: the file stem with the `dis` prefix removed, so
/// `BTNFootman.blp` and `DISBTNFootman.blp` share the key `btnfootman`.
fn icon_stem(name: &str, disabled: bool) -> String {
    let base = name.rsplit(['\\', '/']).next().unwrap_or(name);
    let stem = base
        .trim_end_matches(".blp")
        .trim_end_matches(".BLP")
        .to_ascii_lowercase();
    if disabled {
        stem.strip_prefix("dis").map(str::to_string).unwrap_or(stem)
    } else {
        stem
    }
}

fn biome_of(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let rest = lower.strip_prefix("terrainart\\")?;
    let biome = rest.split('\\').next()?;
    if biome.is_empty() {
        None
    } else {
        Some(biome.to_string())
    }
}

/// Validate the whole corpus and build every catalog in one pass.
pub fn run(db: &AssetDb, options: &CatalogOptions) -> CatalogReport {
    let start = Instant::now();
    let files = db.files();
    let mut report = CatalogReport {
        textures: TextureTotals { min_side: u32::MAX, ..Default::default() },
        ..Default::default()
    };
    let mut biome_map: BTreeMap<String, BiomeRow> = BTreeMap::new();
    let mut icon_pairs: BTreeMap<String, (bool, bool)> = BTreeMap::new();
    let mut limit_reached = false;

    for (index, entry) in files.iter().enumerate() {
        if let Some(limit) = options.limit {
            if index >= limit {
                limit_reached = true;
                break;
            }
        }
        let lower = entry.name.to_ascii_lowercase();
        let ext = extension(&lower);
        let is_texture = ext == "blp" && !options.skip_textures;
        let is_model = ext == "mdx" && !options.skip_models;
        if !is_texture && !is_model {
            continue;
        }
        report.files_scanned += 1;
        report.bytes_scanned += entry.size;
        let bytes = match db.read_uncached(&entry.name) {
            Ok(bytes) => bytes,
            Err(err) => {
                report.failures.push(FailureRow {
                    name: entry.name.clone(),
                    kind: "read".into(),
                    detail: err.to_string(),
                });
                if is_texture {
                    report.textures.files += 1;
                    report.textures.decode_fail += 1;
                } else {
                    report.models.files += 1;
                    report.models.parse_fail += 1;
                }
                continue;
            }
        };

        if is_texture {
            let texture = blp_report::validate(&entry.name, &bytes);
            collect_texture(&mut report, &mut biome_map, &texture);
            let icon = is_icon_path(&lower);
            if icon {
                let disabled = icon_is_disabled(&lower);
                let stem = icon_stem(&entry.name, disabled);
                let slot = icon_pairs.entry(stem).or_insert((false, false));
                if disabled {
                    slot.1 = true;
                } else {
                    slot.0 = true;
                }
                if texture.ok() {
                    report.icons.ok += 1;
                } else {
                    report.icons.failed += 1;
                }
            }
        } else if is_model {
            collect_model(&mut report, db, &entry.name, &bytes);
        }
    }

    // Icon pairing: a BTN icon should have its disabled (DIS*) twin.
    for (_stem, (btn, disabled)) in icon_pairs {
        if disabled {
            report.icons.disabled += 1;
        }
        if btn && disabled {
            report.icons.paired += 1;
        } else if btn {
            report.icons.btn += 1;
            report.icons.missing_disabled += 1;
        } else if disabled {
            report.icons.orphan_disabled += 1;
        }
        if btn && disabled {
            report.icons.btn += 1;
        }
    }

    report.biomes = biome_map.into_values().collect();
    if report.textures.min_side == u32::MAX {
        report.textures.min_side = 0;
    }
    report.elapsed_seconds = start.elapsed().as_secs_f64();
    let _ = limit_reached;
    report
}

fn collect_texture(
    report: &mut CatalogReport,
    biomes: &mut BTreeMap<String, BiomeRow>,
    texture: &TextureReport,
) {
    let totals = &mut report.textures;
    totals.files += 1;
    totals.bytes += texture.source_len as u64;
    if let Some(header) = &texture.header {
        if header.compression == 0 && header.magic == *b"BLP1" {
            totals.jpeg += 1;
        } else if header.magic == *b"BLP1" {
            totals.paletted += 1;
        }
    }
    if texture.header.is_none() {
        totals.header_fail += 1;
    }
    if !texture.decoded {
        totals.decode_fail += 1;
    }
    if texture.decoded && !texture.mip_chain_ok {
        totals.mip_fail += 1;
    }
    if texture.all_transparent {
        totals.all_transparent += 1;
    }
    if texture.constant_color {
        totals.constant_color += 1;
    }
    if texture.alpha_min < 255 {
        totals.with_alpha += 1;
    }
    if texture.alpha_min == 255 {
        totals.fully_opaque += 1;
    }
    totals.mip_levels_declared += texture.declared_mips as u64;
    if texture.declared_mips as usize > texture.mip_count as usize {
        totals.mips_incomplete_files += 1;
    } else if texture.decoded {
        totals.mips_complete_files += 1;
    }
    if texture.declared_mips > 0 && !texture.declared_mip_layout_ok {
        totals.mip_layout_bad += 1;
    }
    if texture.decoded {
        totals.decoded_pixels += (texture.width as u64) * (texture.height as u64);
        totals.mip_levels_decoded += texture.mip_count as u64;
        let side = texture.width.min(texture.height);
        let long = texture.width.max(texture.height);
        if totals.min_side == u32::MAX || side < totals.min_side {
            totals.min_side = side;
        }
        if long > totals.max_side {
            totals.max_side = long;
        }
    }
    if !texture.ok() {
        let (kind, detail) = if let Some(err) = &texture.header_error {
            ("blp-header", err.clone())
        } else if let Some(err) = &texture.decode_error {
            ("blp-decode", err.clone())
        } else if !texture.mip_chain_ok {
            ("blp-mips", format!("{} mips inconsistent", texture.mip_count))
        } else {
            ("blp-unknown", "no decode result".to_string())
        };
        report.failures.push(FailureRow {
            name: texture.name.clone(),
            kind: kind.into(),
            detail,
        });
    } else if texture.all_transparent {
        report.warnings.push(FailureRow {
            name: texture.name.clone(),
            kind: "blp-blank".into(),
            detail: "fully transparent surface (valid blank overlay)".into(),
        });
    }
    report.texture_rows.push(TextureRow {
        name: texture.name.clone(),
        width: texture.width,
        height: texture.height,
        mips: texture.mip_count,
        compression: texture.header.as_ref().map(|h| h.compression).unwrap_or(255),
        alpha_bits: texture.header.as_ref().map(|h| h.alpha_bits).unwrap_or(0),
        source_len: texture.source_len,
        sha256_source: texture.sha256_source.clone(),
        hue: texture.hue.name(),
        alpha_mean: texture.alpha_mean,
        unique_colors: texture.unique_colors_sampled,
        ok: texture.ok(),
        error: texture
            .header_error
            .clone()
            .or_else(|| texture.decode_error.clone())
            .unwrap_or_default(),
    });

    if let Some(biome) = biome_of(&texture.name) {
        let row = biomes.entry(biome.clone()).or_insert_with(|| BiomeRow {
            biome: biome.clone(),
            min_side: u32::MAX,
            ..Default::default()
        });
        row.files += 1;
        row.bytes += texture.source_len as u64;
        if texture.ok() {
            row.ok += 1;
        } else {
            row.failed += 1;
        }
        if texture.decoded {
            for (slot, value) in row.mean_rgb.iter_mut().zip(texture.mean_rgb) {
                *slot = (*slot as u16 + value as u16).min(255) as u8;
            }
            let side = texture.width.min(texture.height);
            let long = texture.width.max(texture.height);
            row.min_side = row.min_side.min(side);
            row.max_side = row.max_side.max(long);
        }
        *row.hues.entry(texture.hue.name().to_string()).or_default() += 1;
    }
}

fn collect_model(report: &mut CatalogReport, db: &AssetDb, name: &str, bytes: &[u8]) {
    let totals = &mut report.models;
    totals.files += 1;
    totals.bytes += bytes.len() as u64;
    let model = match MdxModel::parse(bytes) {
        Ok(model) => model,
        Err(err) => {
            totals.parse_fail += 1;
            report.failures.push(FailureRow {
                name: name.to_string(),
                kind: "mdx-parse".into(),
                detail: err.to_string(),
            });
            report.model_rows.push(ModelRow {
                name: name.to_string(),
                parsed: false,
                error: err.to_string(),
                ..Default::default()
            });
            return;
        }
    };
    totals.parsed += 1;
    totals.geosets += model.geosets.len();
    totals.sequences += model.sequences.len();
    totals.nodes += model.pivots.len();
    let mut external = 0usize;
    let mut replaceable = 0usize;
    let mut unresolved = Vec::new();
    for texture in &model.textures {
        totals.texture_refs += 1;
        if texture.replaceable_id != 0 {
            replaceable += 1;
            totals.replaceable_refs += 1;
            continue;
        }
        let path = texture.path.trim_end_matches('\0');
        if path.is_empty() {
            continue;
        }
        external += 1;
        if db.resolve_path(path).is_none() {
            unresolved.push(path.to_string());
            totals.unresolved_refs += 1;
        }
    }
    if unresolved.is_empty() {
        totals.models_all_resolved += 1;
    } else {
        totals.models_with_unresolved += 1;
        for path in &unresolved {
            report.failures.push(FailureRow {
                name: name.to_string(),
                kind: "mdx-texture-missing".into(),
                detail: path.clone(),
            });
        }
    }
    report.model_rows.push(ModelRow {
        name: name.to_string(),
        textures: model.textures.len(),
        external,
        replaceable,
        unresolved: unresolved.len(),
        geosets: model.geosets.len(),
        sequences: model.sequences.len(),
        nodes: model.pivots.len(),
        parsed: true,
        error: String::new(),
    });
}

fn mean_of_sum(sum: [u8; 3], count: usize) -> [u8; 3] {
    if count == 0 {
        return [0, 0, 0];
    }
    [
        (sum[0] as usize / count) as u8,
        (sum[1] as usize / count) as u8,
        (sum[2] as usize / count) as u8,
    ]
}

/// Write the catalog files: JSON summary, Markdown report and four TSV tables.
pub fn write_reports(report: &CatalogReport, dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;

    let mut tsv = String::with_capacity(report.texture_rows.len() * 140);
    tsv.push_str("name\twidth\theight\tmips\tcompression\talpha_bits\tbytes\tsha256_16\thue\talpha_mean\tunique_colors\tok\terror\n");
    for row in &report.texture_rows {
        let _ = writeln!(
            tsv,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            row.name,
            row.width,
            row.height,
            row.mips,
            row.compression,
            row.alpha_bits,
            row.source_len,
            &row.sha256_source[..16.min(row.sha256_source.len())],
            row.hue,
            row.alpha_mean,
            row.unique_colors,
            row.ok,
            row.error.replace('\t', " ")
        );
    }
    std::fs::write(dir.join("texture-catalog.tsv"), tsv)?;

    let mut mtsv = String::with_capacity(report.model_rows.len() * 100);
    mtsv.push_str("name\ttextures\texternal\treplaceable\tunresolved\tgeosets\tsequences\tnodes\tparsed\terror\n");
    for row in &report.model_rows {
        let _ = writeln!(
            mtsv,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            row.name,
            row.textures,
            row.external,
            row.replaceable,
            row.unresolved,
            row.geosets,
            row.sequences,
            row.nodes,
            row.parsed,
            row.error.replace('\t', " ")
        );
    }
    std::fs::write(dir.join("model-catalog.tsv"), mtsv)?;

    let mut biome_tsv = String::with_capacity(report.biomes.len() * 120);
    biome_tsv.push_str("biome\tfiles\tok\tfailed\tbytes\tmean_rgb\tmedian_hue\tmin_side\tmax_side\n");
    for row in &report.biomes {
        let mean = mean_of_sum(row.mean_rgb, row.files);
        let (hue, count) = row
            .hues
            .iter()
            .max_by_key(|(_, count)| **count)
            .map(|(hue, count)| (hue.clone(), *count))
            .unwrap_or_default();
        let _ = writeln!(
            biome_tsv,
            "{}\t{}\t{}\t{}\t{}\t{:?}\t{} ({})\t{}\t{}",
            row.biome, row.files, row.ok, row.failed, row.bytes, mean, hue, count, row.min_side, row.max_side
        );
    }
    std::fs::write(dir.join("biome-catalog.tsv"), biome_tsv)?;

    let json = render_json(report);
    std::fs::write(dir.join("asset-catalog.json"), json)?;
    std::fs::write(dir.join("asset-catalog.md"), render_markdown(report))?;
    Ok(())
}

fn render_json(report: &CatalogReport) -> String {
    let mut s = String::with_capacity(8192);
    s.push_str("{\n");
    s.push_str("  \"textures\": {\n");
    let t = &report.textures;
    let _ = writeln!(s, "    \"files\": {},", t.files);
    let _ = writeln!(s, "    \"ok\": {},", t.ok_count());
    let _ = writeln!(s, "    \"header_fail\": {},", t.header_fail);
    let _ = writeln!(s, "    \"decode_fail\": {},", t.decode_fail);
    let _ = writeln!(s, "    \"mip_fail\": {},", t.mip_fail);
    let _ = writeln!(s, "    \"all_transparent\": {},", t.all_transparent);
    let _ = writeln!(s, "    \"constant_color\": {},", t.constant_color);
    let _ = writeln!(s, "    \"with_alpha\": {},", t.with_alpha);
    let _ = writeln!(s, "    \"fully_opaque\": {},", t.fully_opaque);
    let _ = writeln!(s, "    \"jpeg_blp1\": {},", t.jpeg);
    let _ = writeln!(s, "    \"paletted_blp1\": {},", t.paletted);
    let _ = writeln!(s, "    \"source_bytes\": {},", t.bytes);
    let _ = writeln!(s, "    \"decoded_pixels\": {},", t.decoded_pixels);
    let _ = writeln!(s, "    \"mip_levels_decoded\": {},", t.mip_levels_decoded);
    let _ = writeln!(s, "    \"mip_levels_declared\": {},", t.mip_levels_declared);
    let _ = writeln!(s, "    \"mips_incomplete_files\": {},", t.mips_incomplete_files);
    let _ = writeln!(s, "    \"mips_complete_files\": {},", t.mips_complete_files);
    let _ = writeln!(s, "    \"mip_layout_bad\": {},", t.mip_layout_bad);
    let _ = writeln!(s, "    \"min_side\": {},", t.min_side);
    let _ = writeln!(s, "    \"max_side\": {}", t.max_side);
    s.push_str("  },\n  \"models\": {\n");
    let m = &report.models;
    let _ = writeln!(s, "    \"files\": {},", m.files);
    let _ = writeln!(s, "    \"parsed\": {},", m.parsed);
    let _ = writeln!(s, "    \"parse_fail\": {},", m.parse_fail);
    let _ = writeln!(s, "    \"texture_refs\": {},", m.texture_refs);
    let _ = writeln!(s, "    \"replaceable_refs\": {},", m.replaceable_refs);
    let _ = writeln!(s, "    \"unresolved_refs\": {},", m.unresolved_refs);
    let _ = writeln!(s, "    \"models_with_unresolved\": {},", m.models_with_unresolved);
    let _ = writeln!(s, "    \"models_all_resolved\": {},", m.models_all_resolved);
    let _ = writeln!(s, "    \"geosets\": {},", m.geosets);
    let _ = writeln!(s, "    \"sequences\": {},", m.sequences);
    let _ = writeln!(s, "    \"nodes\": {},", m.nodes);
    let _ = writeln!(s, "    \"source_bytes\": {}", m.bytes);
    s.push_str("  },\n  \"icons\": {\n");
    let i = &report.icons;
    let _ = writeln!(s, "    \"btn\": {},", i.btn);
    let _ = writeln!(s, "    \"disabled\": {},", i.disabled);
    let _ = writeln!(s, "    \"paired\": {},", i.paired);
    let _ = writeln!(s, "    \"missing_disabled\": {},", i.missing_disabled);
    let _ = writeln!(s, "    \"orphan_disabled\": {},", i.orphan_disabled);
    let _ = writeln!(s, "    \"decoded_ok\": {},", i.ok);
    let _ = writeln!(s, "    \"decoded_fail\": {}", i.failed);
    s.push_str("  },\n  \"biomes\": [\n");
    for (n, row) in report.biomes.iter().enumerate() {
        let mean = mean_of_sum(row.mean_rgb, row.files);
        let _ = write!(
            s,
            "    {{\"biome\": \"{}\", \"files\": {}, \"ok\": {}, \"failed\": {}, \"bytes\": {}, \"mean_rgb\": [{}, {}, {}]}}{}",
            row.biome,
            row.files,
            row.ok,
            row.failed,
            row.bytes,
            mean[0],
            mean[1],
            mean[2],
            if n + 1 == report.biomes.len() { "\n" } else { ",\n" }
        );
    }
    s.push_str("  ],\n");
    let _ = writeln!(s, "  \"files_scanned\": {},", report.files_scanned);
    let _ = writeln!(s, "  \"bytes_scanned\": {},", report.bytes_scanned);
    let _ = writeln!(s, "  \"elapsed_seconds\": {:.3},", report.elapsed_seconds);
    let _ = writeln!(s, "  \"failures\": {},", report.failures.len());
    let _ = writeln!(s, "  \"all_ok\": {}", report.all_ok());
    s.push_str("}\n");
    s
}

fn render_markdown(report: &CatalogReport) -> String {
    let mut s = String::new();
    let t = &report.textures;
    let m = &report.models;
    let i = &report.icons;
    s.push_str("# Asset catalog\n\n");
    let _ = writeln!(s, "Scanned {} files / {} bytes in {:.2} s.\n", report.files_scanned, report.bytes_scanned, report.elapsed_seconds);
    s.push_str("## Textures (BLP)\n\n| metric | value |\n|---|---:|\n");
    let _ = writeln!(s, "| files | {} |", t.files);
    let _ = writeln!(s, "| decoded OK | {} |", t.ok_count());
    let _ = writeln!(s, "| header failures | {} |", t.header_fail);
    let _ = writeln!(s, "| decode failures | {} |", t.decode_fail);
    let _ = writeln!(s, "| mip-chain failures | {} |", t.mip_fail);
    let _ = writeln!(s, "| fully transparent | {} |", t.all_transparent);
    let _ = writeln!(s, "| constant colour | {} |", t.constant_color);
    let _ = writeln!(s, "| with alpha | {} |", t.with_alpha);
    let _ = writeln!(s, "| fully opaque | {} |", t.fully_opaque);
    let _ = writeln!(s, "| BLP1 JPEG | {} |", t.jpeg);
    let _ = writeln!(s, "| BLP1 paletted | {} |", t.paletted);
    let _ = writeln!(s, "| decoded pixels | {} |", t.decoded_pixels);
    let _ = writeln!(s, "| mip levels declared / decoded | {} / {} |", t.mip_levels_declared, t.mip_levels_decoded);
    let _ = writeln!(s, "| files with every declared mip decoded | {} |", t.mips_complete_files);
    let _ = writeln!(s, "| files still mip-0-only (needs blp patch) | {} |", t.mips_incomplete_files);
    let _ = writeln!(s, "| mip layout failures | {} |", t.mip_layout_bad);
    let _ = writeln!(s, "| size range (side) | {}..{} |", t.min_side, t.max_side);
    s.push_str("\n## Models (MDX)\n\n| metric | value |\n|---|---:|\n");
    let _ = writeln!(s, "| files | {} |", m.files);
    let _ = writeln!(s, "| parsed | {} |", m.parsed);
    let _ = writeln!(s, "| parse failures | {} |", m.parse_fail);
    let _ = writeln!(s, "| texture references | {} |", m.texture_refs);
    let _ = writeln!(s, "| replaceable refs | {} |", m.replaceable_refs);
    let _ = writeln!(s, "| unresolved texture refs | {} |", m.unresolved_refs);
    let _ = writeln!(s, "| models with unresolved refs | {} |", m.models_with_unresolved);
    let _ = writeln!(s, "| models fully resolved | {} |", m.models_all_resolved);
    let _ = writeln!(s, "| geosets | {} |", m.geosets);
    let _ = writeln!(s, "| sequences | {} |", m.sequences);
    let _ = writeln!(s, "| nodes | {} |", m.nodes);
    s.push_str("\n## Icons\n\n| metric | value |\n|---|---:|\n");
    let _ = writeln!(s, "| BTN icons | {} |", i.btn);
    let _ = writeln!(s, "| disabled icons | {} |", i.disabled);
    let _ = writeln!(s, "| paired BTN + DIS | {} |", i.paired);
    let _ = writeln!(s, "| BTN without DIS | {} |", i.missing_disabled);
    let _ = writeln!(s, "| orphan DIS | {} |", i.orphan_disabled);
    let _ = writeln!(s, "| decoded OK | {} |", i.ok);
    let _ = writeln!(s, "| decoded FAIL | {} |", i.failed);
    s.push_str("\n## Biome tiles\n\n| biome | files | ok | failed | mean rgb |\n|---|---:|---:|---:|---|\n");
    for row in &report.biomes {
        let mean = mean_of_sum(row.mean_rgb, row.files);
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | ({}, {}, {}) |",
            row.biome, row.files, row.ok, row.failed, mean[0], mean[1], mean[2]
        );
    }
    s.push_str("\n## Failures\n\n");
    if report.failures.is_empty() {
        s.push_str("None.\n");
    } else {
        let _ = writeln!(s, "{} total (first 50):\n", report.failures.len());
        for failure in report.failures.iter().take(50) {
            let _ = writeln!(s, "- `{}` [{}] {}", failure.name, failure.kind, failure.detail);
        }
    }
    s
}

impl TextureTotals {
    pub fn ok_count(&self) -> usize {
        self.files
            .saturating_sub(self.header_fail + self.decode_fail + self.mip_fail)
    }
}

/// Hue histogram helper used by reports and tests.
pub fn hue_name(value: HueClass) -> &'static str {
    value.name()
}
