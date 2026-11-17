//! World-layer coverage audit (test-only).
//!
//! Scope expansion request (2026-11-12): prove that *every* model, biome tile
//! and command-button icon in the user's `War3.mpq` is parsed, textured and
//! renderable — not a sample. This module walks the archive, imports each MDX
//! with the real `AssetDb` parser, renders a bind/Stand thumbnail with a small
//! deterministic software rasteriser (the same `build_meshes` + `mesh_alphas_for`
//! truth the GPU path uses), and writes per-category contact sheets plus a JSON
//! and Markdown report under `analysis/agent-world/`.
//!
//! Run explicitly:
//! `cargo test -p wc3-game --lib agent_world_audit -- --ignored --nocapture`

use std::collections::BTreeMap;
use std::path::PathBuf;

use glam::{Mat4, Vec3};
use wc3_anim::Animator;
use wc3_assetdb::AssetDb;
use wc3_formats::blp::BlpImage;
use wc3_formats::mdx::{MdxModel, Sequence};

use crate::assets::{build_meshes, mesh_alphas_for};
use crate::terrain_render::{classify, Biome};

const TEAM: [f32; 3] = [0.36, 0.58, 1.0];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn out_dir() -> PathBuf {
    let dir = repo_root().join("analysis/agent-world");
    std::fs::create_dir_all(&dir).expect("create analysis/agent-world");
    dir
}

fn mpq_path() -> Option<PathBuf> {
    [
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
        "/data/data/org.wc3rust.p0/files/War3.mpq",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|path| path.exists())
}

fn open_db() -> Option<AssetDb> {
    let path = mpq_path()?;
    match AssetDb::open_mpq(&path) {
        Ok(db) => Some(db),
        Err(error) => {
            eprintln!("AUDIT open_mpq failed: {error}");
            None
        }
    }
}

fn category(path: &str) -> &'static str {
    let path = path.to_ascii_lowercase();
    if path.starts_with("doodads\\") {
        "Doodads"
    } else if path.starts_with("units\\") {
        "Units"
    } else if path.starts_with("abilities\\") {
        "Abilities"
    } else if path.starts_with("buildings\\") {
        "Buildings"
    } else if path.starts_with("objects\\") {
        "Objects"
    } else if path.starts_with("ui\\") {
        "UI"
    } else if path.starts_with("environment\\") {
        "Environment"
    } else if path.starts_with("sharedmodels\\") {
        "SharedModels"
    } else {
        "Other"
    }
}

fn find_sequence<'a>(model: &'a MdxModel, needle: &str) -> Option<&'a Sequence> {
    model
        .sequences
        .iter()
        .find(|sequence| sequence.name.to_ascii_lowercase().contains(needle))
        .or_else(|| model.sequences.first())
}

/// Keep only the code points that are safe in the hand-written JSON report.
fn json_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

// ------------------------------------------------------------ rasteriser

struct Thumb {
    rgba: Vec<u8>,
    coverage: f32,
}

/// Project + rasterise one model at Stand t=0. This is intentionally a
/// software path: it renders every model deterministically, without a GPU
/// device per thumbnail, while using the production mesh/alpha/material data.
fn render_thumbnail(
    model: &MdxModel,
    textures: &[Option<BlpImage>],
    size: u32,
) -> Thumb {
    let texture_map: Vec<usize> = (0..model.textures.len())
        .map(|index| if textures.get(index).map(Option::is_some).unwrap_or(false) { index } else { 0 })
        .collect();
    let mut meshes = build_meshes(model, &texture_map);
    let sequence = find_sequence(model, "stand");
    let frame = sequence.map(|s| s.start).unwrap_or(0);
    let alphas = mesh_alphas_for(model, &meshes, sequence, frame);
    let mut animator = Animator::from_model(model);
    if let Some(sequence) = sequence {
        animator.play(sequence);
        animator.update(0.0);
    }
    let nodes = animator.world_matrices();

    // Transform every vertex once, in the same order the GPU path uses.
    let mut transformed: Vec<Vec<Vec3>> = Vec::with_capacity(meshes.len());
    let mut normals: Vec<Vec<Vec3>> = Vec::with_capacity(meshes.len());
    for mesh in &meshes {
        let mut positions = Vec::with_capacity(mesh.positions.len());
        let mut mesh_normals = Vec::with_capacity(mesh.positions.len());
        for (index, position) in mesh.positions.iter().enumerate() {
            let mut local = Vec3::ZERO;
            let mut normal = Vec3::ZERO;
            let mut total = 0.0;
            if let Some(list) = mesh.bones.get(index) {
                for bone in list {
                    if (bone.bone as usize) < nodes.len() && bone.weight > 0.0 {
                        local += nodes[bone.bone as usize].transform_point3(*position) * bone.weight;
                        normal += nodes[bone.bone as usize]
                            .transform_vector3(mesh.normals.get(index).copied().unwrap_or(Vec3::Y))
                            * bone.weight;
                        total += bone.weight;
                    }
                }
            }
            if total > 1e-5 {
                local /= total;
                normal = normal.normalize_or_zero();
            } else {
                local = *position;
                normal = mesh.normals.get(index).copied().unwrap_or(Vec3::Y);
            }
            let (local, normal) = if mesh.z_up {
                (
                    Vec3::new(local.x, local.z, -local.y),
                    Vec3::new(normal.x, normal.z, -normal.y),
                )
            } else {
                (local, normal)
            };
            positions.push(local);
            mesh_normals.push(normal.normalize_or_zero());
        }
        transformed.push(positions);
        normals.push(mesh_normals);
    }

    // Orthographic isometric basis; fit the projected silhouette.
    let direction = Vec3::new(0.78, 0.52, 0.82).normalize();
    let right = Vec3::Y.cross(direction).normalize();
    let up = direction.cross(right).normalize();
    let mut min = glam::Vec2::splat(f32::INFINITY);
    let mut max = glam::Vec2::splat(f32::NEG_INFINITY);
    let mut depth_min = f32::INFINITY;
    let mut depth_max = f32::NEG_INFINITY;
    for (mesh_index, mesh) in meshes.iter().enumerate() {
        if alphas.get(mesh_index).copied().unwrap_or(1.0) <= 0.02 {
            continue;
        }
        for position in &transformed[mesh_index] {
            let p = glam::Vec2::new(position.dot(right), position.dot(up));
            min = min.min(p);
            max = max.max(p);
            let depth = position.dot(direction);
            depth_min = depth_min.min(depth);
            depth_max = depth_max.max(depth);
        }
    }
    if !min.is_finite() || (max - min).max_element() < 1e-3 {
        return Thumb {
            rgba: vec![0; (size * size * 4) as usize],
            coverage: 0.0,
        };
    }
    let span = (max - min).max_element().max(1.0);
    let scale = (size as f32 * 0.86) / span;
    let center = (min + max) * 0.5;
    let depth_span = (depth_max - depth_min).max(1e-3);
    let project = |position: Vec3| -> (f32, f32, f32) {
        let x = (position.dot(right) - center.x) * scale + size as f32 * 0.5;
        let y = size as f32 * 0.5 - (position.dot(up) - center.y) * scale;
        let z = (position.dot(direction) - depth_min) / depth_span;
        (x, y, z)
    };

    let mut color = vec![0u8; (size * size * 3) as usize];
    for pixel in color.chunks_exact_mut(3) {
        pixel[0] = 26;
        pixel[1] = 32;
        pixel[2] = 38;
    }
    let mut zbuf = vec![f32::NEG_INFINITY; (size * size) as usize];
    let mut covered = vec![false; (size * size) as usize];
    let light = Vec3::new(0.42, 0.86, 0.28).normalize();
    let mut texture_cache: Vec<Option<&BlpImage>> = textures.iter().map(Option::as_ref).collect();

    // Two passes: depth-writing opaque/cutout, then blended/additive.
    for pass in 0..2 {
        for (mesh_index, mesh) in meshes.iter().enumerate() {
            let geoset_alpha = alphas.get(mesh_index).copied().unwrap_or(1.0);
            if geoset_alpha <= 0.02 {
                continue;
            }
            for layer in &mesh.layers {
                let blended = layer.blend >= 2;
                if (pass == 0) == blended {
                    continue;
                }
                let layer_alpha = layer_alpha_at(layer, sequence, frame) * geoset_alpha;
                if layer_alpha <= 0.02 {
                    continue;
                }
                let sample = texture_cache
                    .get(layer.texture)
                    .copied()
                    .flatten();
                let tint = if layer.team_color {
                    TEAM
                } else {
                    [1.0, 1.0, 1.0]
                };
                for triangle in mesh.indices.chunks_exact(3) {
                    let (a, b, c) = (
                        triangle[0] as usize,
                        triangle[1] as usize,
                        triangle[2] as usize,
                    );
                    let (Some(&pa), Some(&pb), Some(&pc)) = (
                        transformed[mesh_index].get(a),
                        transformed[mesh_index].get(b),
                        transformed[mesh_index].get(c),
                    ) else {
                        continue;
                    };
                    let (ax, ay, az) = project(pa);
                    let (bx, by, bz) = project(pb);
                    let (cx, cy, cz) = project(pc);
                    let area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
                    if area.abs() < 1e-5 {
                        continue;
                    }
                    // Normalise the barycentric sign so both MDX winding
                    // orders rasterise: many WC3 geosets are clockwise once
                    // projected into our Y-down thumbnail space.
                    let sign = if area < 0.0 { -1.0 } else { 1.0 };
                    let area = area.abs();
                    let min_x = ax.min(bx).min(cx).floor().max(0.0) as i32;
                    let max_x = ax.max(bx).max(cx).ceil().min(size as f32 - 1.0) as i32;
                    let min_y = ay.min(by).min(cy).floor().max(0.0) as i32;
                    let max_y = ay.max(by).max(cy).ceil().min(size as f32 - 1.0) as i32;
                    if min_x > max_x || min_y > max_y {
                        continue;
                    }
                    let uv = |index: usize| -> glam::Vec2 {
                        mesh.uvs.get(index).copied().unwrap_or(glam::Vec2::ZERO)
                    };
                    let (ua, ub, uc) = (uv(a), uv(b), uv(c));
                    let na = normals[mesh_index].get(a).copied().unwrap_or(Vec3::Y);
                    let nb = normals[mesh_index].get(b).copied().unwrap_or(Vec3::Y);
                    let nc = normals[mesh_index].get(c).copied().unwrap_or(Vec3::Y);
                    for y in min_y..=max_y {
                        for x in min_x..=max_x {
                            let px = x as f32 + 0.5;
                            let py = y as f32 + 0.5;
                            let w0 = sign
                                * ((bx - px) * (cy - py) - (by - py) * (cx - px))
                                / area;
                            let w1 = sign
                                * ((cx - px) * (ay - py) - (cy - py) * (ax - px))
                                / area;
                            let w2 = 1.0 - w0 - w1;
                            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                                continue;
                            }
                            let depth = w0 * az + w1 * bz + w2 * cz;
                            let index = (y * size as i32 + x) as usize;
                            if pass == 0 {
                                if depth <= zbuf[index] {
                                    continue;
                                }
                            } else if depth < zbuf[index] - 0.002 {
                                continue;
                            }
                            let uv = ua * w0 + ub * w1 + uc * w2;
                            let texel = match sample {
                                Some(image) if image.width > 0 && image.height > 0 => {
                                    let tx = ((uv.x.rem_euclid(1.0) * image.width as f32) as u32)
                                        .min(image.width - 1);
                                    let ty = ((uv.y.rem_euclid(1.0) * image.height as f32) as u32)
                                        .min(image.height - 1);
                                    let offset = ((ty * image.width + tx) * 4) as usize;
                                    [
                                        image.rgba[offset] as f32 / 255.0,
                                        image.rgba[offset + 1] as f32 / 255.0,
                                        image.rgba[offset + 2] as f32 / 255.0,
                                        image.rgba[offset + 3] as f32 / 255.0,
                                    ]
                                }
                                _ => [1.0, 1.0, 1.0, 1.0],
                            };
                            let alpha = texel[3] * layer_alpha;
                            if pass == 0 && alpha < 0.35 {
                                continue;
                            }
                            if alpha <= 0.01 {
                                continue;
                            }
                            let normal = (na * w0 + nb * w1 + nc * w2).normalize_or_zero();
                            let shade = 0.55 + 0.45 * normal.dot(light).abs();
                            let rgb = [
                                texel[0] * tint[0] * shade,
                                texel[1] * tint[1] * shade,
                                texel[2] * tint[2] * shade,
                            ];
                            let pixel = &mut color[index * 3..index * 3 + 3];
                            if pass == 0 {
                                pixel[0] = (rgb[0] * 255.0) as u8;
                                pixel[1] = (rgb[1] * 255.0) as u8;
                                pixel[2] = (rgb[2] * 255.0) as u8;
                                zbuf[index] = depth;
                                covered[index] = true;
                            } else {
                                let add = alpha * 0.55;
                                pixel[0] = ((pixel[0] as f32 + rgb[0] * 255.0 * add)
                                    .min(255.0)) as u8;
                                pixel[1] = ((pixel[1] as f32 + rgb[1] * 255.0 * add)
                                    .min(255.0)) as u8;
                                pixel[2] = ((pixel[2] as f32 + rgb[2] * 255.0 * add)
                                    .min(255.0)) as u8;
                            }
                        }
                    }
                }
            }
        }
    }
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for (index, pixel) in color.chunks_exact(3).enumerate() {
        rgba[index * 4] = pixel[0];
        rgba[index * 4 + 1] = pixel[1];
        rgba[index * 4 + 2] = pixel[2];
        rgba[index * 4 + 3] = 255;
    }
    let coverage = covered.iter().filter(|value| **value).count() as f32 / (size * size) as f32;
    Thumb { rgba, coverage }
}

// ------------------------------------------------------------ galleries

struct Sheet {
    path: PathBuf,
    tile: u32,
    columns: u32,
    rows: u32,
    pixels: Vec<u8>,
    filled: u32,
}

impl Sheet {
    fn new(path: PathBuf, tile: u32, columns: u32, rows: u32) -> Self {
        let width = columns * (tile + 2);
        let height = rows * (tile + 2);
        let mut sheet = Self {
            path,
            tile,
            columns,
            rows,
            pixels: vec![12; (width * height * 3) as usize],
            filled: 0,
        };
        for (index, pixel) in sheet.pixels.chunks_exact_mut(3).enumerate() {
            let x = (index as u32) % width;
            let y = (index as u32) / width;
            if x % (tile + 2) < tile && y % (tile + 2) < tile {
                pixel[0] = 24;
                pixel[1] = 28;
                pixel[2] = 32;
            }
        }
        sheet
    }

    fn put(&mut self, thumb: &Thumb, ok: bool) {
        let index = self.filled;
        self.filled += 1;
        let col = index % self.columns;
        let row = index / self.columns;
        if row >= self.rows {
            return;
        }
        let x0 = col * (self.tile + 2);
        let y0 = row * (self.tile + 2);
        let width = self.columns * (self.tile + 2);
        let border = if ok { [40, 180, 90] } else { [210, 60, 55] };
        for y in 0..self.tile {
            for x in 0..self.tile {
                let src = ((y * self.tile + x) * 4) as usize;
                let dst = (((y0 + y) * width + x0 + x) * 3) as usize;
                self.pixels[dst] = thumb.rgba[src];
                self.pixels[dst + 1] = thumb.rgba[src + 1];
                self.pixels[dst + 2] = thumb.rgba[src + 2];
            }
        }
        for y in 0..self.tile + 2 {
            for x in [0, self.tile + 1] {
                let dst = (((y0 + y) * width + x0 + x) * 3) as usize;
                self.pixels[dst..dst + 3].copy_from_slice(&border);
            }
        }
        for x in 0..self.tile + 2 {
            for y in [0, self.tile + 1] {
                let dst = (((y0 + y) * width + x0 + x) * 3) as usize;
                self.pixels[dst..dst + 3].copy_from_slice(&border);
            }
        }
    }

    fn write(&self) {
        let width = self.columns * (self.tile + 2);
        let height = self.rows * (self.tile + 2);
        let mut out = format!("P6\n{width} {height}\n255\n").into_bytes();
        out.extend_from_slice(&self.pixels);
        let _ = std::fs::write(&self.path, out);
    }
}

// ------------------------------------------------------------ model audit

#[derive(Default)]
struct CategoryStats {
    total: usize,
    parsed: usize,
    failed: Vec<String>,
    unresolved_textures: usize,
    missing_examples: Vec<String>,
    layers: usize,
    geosets: usize,
    sequences: usize,
    low_coverage: usize,
    effect_only: usize,
}

#[test]
#[ignore = "expensive full-archive audit: needs War3.mpq; run_layer_gates.sh --ignored"]
fn agent_world_audit_all_models() {
    let Some(mut db) = open_db() else {
        eprintln!("SKIP agent_world_audit_all_models: War3.mpq not present");
        return;
    };
    let out = out_dir();
    let files = db.files();
    eprintln!("AUDIT archive files={}", files.len());
    let mut models: Vec<String> = files
        .iter()
        .map(|entry| entry.name.replace('/', "\\"))
        .filter(|name| name.to_ascii_lowercase().ends_with(".mdx"))
        .collect();
    models.sort();
    models.dedup();
    eprintln!("AUDIT mdx={}", models.len());
    if models.is_empty() {
        eprintln!("SKIP agent_world_audit_all_models: archive has no listfile");
        return;
    }

    let mut stats: BTreeMap<&'static str, CategoryStats> = BTreeMap::new();
    let mut sheets: BTreeMap<&'static str, Sheet> = BTreeMap::new();
    let mut index: Vec<(String, &'static str, bool, f32)> = Vec::new();
    let mut count_by_category: BTreeMap<&'static str, usize> = BTreeMap::new();
    for name in &models {
        *count_by_category.entry(category(name)).or_default() += 1;
    }
    for (name, count) in &count_by_category {
        let columns = 16u32;
        let rows = ((*count as u32) + columns - 1) / columns;
        let path = out.join(format!("models-{}.ppm", name.to_ascii_lowercase()));
        sheets.insert(name, Sheet::new(path, 64, columns, rows));
    }

    for (index_number, name) in models.iter().enumerate() {
        let cat = category(name);
        let stats = stats.entry(cat).or_default();
        stats.total += 1;
        let parsed = match db.model_shared(name) {
            Ok(asset) => asset.model.clone(),
            Err(error) => {
                stats.failed.push(format!("{name}: parse {error}"));
                if let Some(sheet) = sheets.get_mut(cat) {
                    sheet.put(
                        &Thumb {
                            rgba: vec![0; 64 * 64 * 4],
                            coverage: 0.0,
                        },
                        false,
                    );
                }
                index.push((name.clone(), cat, false, 0.0));
                if index_number % 200 == 0 {
                    eprintln!("AUDIT progress {index_number}/{}", models.len());
                }
                continue;
            }
        };
        stats.parsed += 1;
        stats.geosets += parsed.geosets.len();
        stats.sequences += parsed.sequences.len();
        let mut unresolved = 0usize;
        let mut textures: Vec<Option<BlpImage>> = Vec::with_capacity(parsed.textures.len());
        for texture in &parsed.textures {
            if texture.replaceable_id != 0 {
                // Team layers are white + tinted, exactly like the game path.
                textures.push(None);
                continue;
            }
            if texture.path.is_empty() {
                textures.push(None);
                continue;
            }
            let path = texture.path.trim_end_matches('\0').to_string();
            match db.texture_shared(&path) {
                Ok(asset) => textures.push(Some(asset.image.clone())),
                Err(error) => {
                    unresolved += 1;
                    if stats.missing_examples.len() < 6 {
                        stats.missing_examples.push(path.clone());
                    }
                    textures.push(None);
                }
            }
        }
        for geoset in &parsed.geosets {
            if let Some(material) = parsed.materials.get(geoset.material_id as usize) {
                stats.layers += material.layers.len();
            }
        }
        stats.unresolved_textures += unresolved;
        let sequence = find_sequence(&parsed, "stand");
        let alphas = mesh_alphas_for(
            &parsed,
            &crate::assets::build_meshes(&parsed, &(0..parsed.textures.len()).collect::<Vec<_>>()),
            sequence,
            sequence.map(|s| s.start).unwrap_or(0),
        );
        let visible_geosets = alphas.iter().filter(|alpha| **alpha > 0.01).count();
        let thumb = render_thumbnail(&parsed, &textures, 64);
        // Effect-only models (particle emitters, spell targets) legitimately
        // have no visible geometry/layer alpha in Stand; counted, not failed.
        let effect_only = visible_geosets == 0
            || parsed.geosets.is_empty()
            || !any_opaque_layer_visible(&parsed, sequence, sequence.map(|s| s.start).unwrap_or(0));
        if effect_only {
            stats.effect_only += 1;
        }
        let ok = effect_only || thumb.coverage > 0.02;
        if !effect_only && thumb.coverage <= 0.02 {
            stats.low_coverage += 1;
            if stats.failed.len() < 40 {
                stats.failed.push(format!(
                    "{name}: visible geosets={visible_geosets} but coverage {:.3}",
                    thumb.coverage
                ));
            }
        }
        if let Some(sheet) = sheets.get_mut(cat) {
            sheet.put(&thumb, ok);
        }
        index.push((name.clone(), cat, ok, thumb.coverage));
        if index_number % 200 == 0 {
            eprintln!("AUDIT progress {index_number}/{}", models.len());
        }
    }

    let mut report = String::new();
    report.push_str("# All-model audit (War3.mpq)\n\n");
    report.push_str(&format!("Models found: **{}**\n\n", models.len()));
    report.push_str("| Category | Models | Parsed | Geosets | Sequences | Layers | Effect-only | Unresolved textures | Low coverage | Failed |\n");
    report.push_str("|---|---|---|---|---|---|---|---|---|---|\n");
    for (cat, stats) in &stats {
        report.push_str(&format!(
            "| {cat} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            stats.total,
            stats.parsed,
            stats.geosets,
            stats.sequences,
            stats.layers,
            stats.effect_only,
            stats.unresolved_textures,
            stats.low_coverage,
            stats.failed.len()
        ));
        for example in &stats.missing_examples {
            report.push_str(&format!("  - missing texture example: `{example}`\n"));
        }
    }
    report.push_str("\n## Failures\n\n");
    let mut failures = 0;
    for (cat, stats) in &stats {
        for failure in &stats.failed {
            failures += 1;
            report.push_str(&format!("- **{cat}**: {failure}\n"));
        }
    }
    if failures == 0 {
        report.push_str("None.\n");
    }
    let _ = std::fs::write(out.join("models-audit.md"), &report);

    let mut json = String::from("{\n  \"models\": ");
    json.push_str(&models.len().to_string());
    json.push_str(",\n  \"categories\": {\n");
    for (category_index, (cat, cat_stats)) in stats.iter().enumerate() {
        json.push_str(&format!(
            "    \"{cat}\": {{\"total\":{},\"parsed\":{},\"geosets\":{},\"sequences\":{},\"layers\":{},\"unresolved_textures\":{},\"low_coverage\":{},\"failures\":{}}}",
            cat_stats.total,
            cat_stats.parsed,
            cat_stats.geosets,
            cat_stats.sequences,
            cat_stats.layers,
            cat_stats.unresolved_textures,
            cat_stats.low_coverage,
            cat_stats.failed.len()
        ));
        if category_index + 1 < stats.len() {
            json.push(',');
        }
        json.push('\n');
    }
    json.push_str("  },\n  \"tiles\": [\n");
    for (tile_index, (name, cat, ok, coverage)) in index.iter().enumerate() {
        json.push_str(&format!(
            "    {{\"model\":\"{}\",\"category\":\"{cat}\",\"pass\":{ok},\"coverage\":{coverage:.4}}}",
            json_escape(name)
        ));
        if tile_index + 1 < index.len() {
            json.push(',');
        }
        json.push('\n');
    }
    json.push_str("  ]\n}\n");
    let _ = std::fs::write(out.join("models-audit.json"), &json);
    for sheet in sheets.values() {
        sheet.write();
    }

    let total: usize = stats.values().map(|stats| stats.total).sum();
    let failed: usize = stats.values().map(|stats| stats.failed.len()).sum();
    println!(
        "AUDIT_MODELS total={total} failed={failed} sheets={} dir={}",
        sheets.len(),
        out.display()
    );
    for (cat, stats) in &stats {
        println!(
            "AUDIT_CATEGORY {cat} total={} parsed={} geosets={} sequences={} layers={} unresolved={} low_coverage={} failed={}",
            stats.total,
            stats.parsed,
            stats.geosets,
            stats.sequences,
            stats.layers,
            stats.unresolved_textures,
            stats.low_coverage,
            stats.failed.len()
        );
    }
}

// ------------------------------------------------------------ biomes/icons

fn average(image: &BlpImage) -> [u8; 3] {
    let mut sum = [0u64; 3];
    let mut count = 0u64;
    for pixel in image.rgba.chunks_exact(4) {
        if pixel[3] > 8 {
            for channel in 0..3 {
                sum[channel] += pixel[channel] as u64;
            }
            count += 1;
        }
    }
    if count == 0 {
        return [0, 0, 0];
    }
    [
        (sum[0] / count) as u8,
        (sum[1] / count) as u8,
        (sum[2] / count) as u8,
    ]
}

fn thumbnail(image: &BlpImage, tile: u32) -> Vec<u8> {
    let mut out = vec![0u8; (tile * tile * 4) as usize];
    if image.width == 0 || image.height == 0 {
        return out;
    }
    for y in 0..tile {
        for x in 0..tile {
            let sx = (x as u64 * image.width as u64 / tile as u64)
                .min(image.width as u64 - 1) as u32;
            let sy = (y as u64 * image.height as u64 / tile as u64)
                .min(image.height as u64 - 1) as u32;
            let src = ((sy * image.width + sx) * 4) as usize;
            let dst = ((y * tile + x) * 4) as usize;
            out[dst..dst + 4].copy_from_slice(&image.rgba[src..src + 4]);
        }
    }
    out
}

fn write_sheet(path: &std::path::Path, tiles: &[(Vec<u8>, bool)], tile: u32, columns: u32) {
    let rows = ((tiles.len() as u32) + columns - 1) / columns;
    let width = columns * (tile + 2);
    let height = rows * (tile + 2);
    let mut pixels = vec![10u8; (width * height * 3) as usize];
    for (index, (rgba, ok)) in tiles.iter().enumerate() {
        let col = index as u32 % columns;
        let row = index as u32 / columns;
        let x0 = col * (tile + 2);
        let y0 = row * (tile + 2);
        let border = if *ok { [40, 180, 90] } else { [210, 60, 55] };
        for y in 0..tile {
            for x in 0..tile {
                let src = ((y * tile + x) * 4) as usize;
                let dst = (((y0 + y) * width + x0 + x) * 3) as usize;
                pixels[dst] = rgba[src];
                pixels[dst + 1] = rgba[src + 1];
                pixels[dst + 2] = rgba[src + 2];
            }
        }
        for y in 0..tile + 2 {
            for x in [0, tile + 1] {
                let dst = (((y0 + y) * width + x0 + x) * 3) as usize;
                pixels[dst..dst + 3].copy_from_slice(&border);
            }
        }
        for x in 0..tile + 2 {
            for y in [0, tile + 1] {
                let dst = (((y0 + y) * width + x0 + x) * 3) as usize;
                pixels[dst..dst + 3].copy_from_slice(&border);
            }
        }
    }
    let mut out = format!("P6\n{width} {height}\n255\n").into_bytes();
    out.extend_from_slice(&pixels);
    let _ = std::fs::write(path, out);
}

#[test]
#[ignore = "expensive full-archive audit: needs War3.mpq; run_layer_gates.sh --ignored"]
fn agent_world_audit_all_biomes_and_icons() {
    let Some(mut db) = open_db() else {
        eprintln!("SKIP agent_world_audit_all_biomes_and_icons: War3.mpq not present");
        return;
    };
    let out = out_dir();
    let files = db.files();
    let mut terrain: Vec<String> = files
        .iter()
        .map(|entry| entry.name.replace('/', "\\"))
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            lower.starts_with("terrainart\\") && lower.ends_with(".blp")
        })
        .collect();
    terrain.sort();
    terrain.dedup();
    let mut icons: Vec<String> = files
        .iter()
        .map(|entry| entry.name.replace('/', "\\"))
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            lower.contains("commandbuttons\\") && lower.ends_with(".blp")
        })
        .collect();
    icons.sort();
    icons.dedup();
    let mut team: Vec<String> = files
        .iter()
        .map(|entry| entry.name.replace('/', "\\"))
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            (lower.contains("teamcolor\\") || lower.contains("teamglow\\"))
                && lower.ends_with(".blp")
        })
        .collect();
    team.sort();
    team.dedup();

    let mut biome_tiles: Vec<(Vec<u8>, bool)> = Vec::new();
    let mut palette: BTreeMap<u8, (u64, u64, u64, u64)> = BTreeMap::new();
    let mut failures = 0usize;
    let mut biome_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for name in &terrain {
        let tag = name
            .rsplit('\\')
            .next()
            .unwrap_or(name)
            .trim_end_matches(".blp")
            .trim_end_matches(".BLP");
        let decoded = db.texture_shared(name);
        match decoded {
            Ok(asset) => {
                let avg = average(&asset.image);
                let biome = classify(tag, avg, false);
                *biome_counts.entry(match biome {
                    Biome::Grass => "Grass",
                    Biome::Dirt => "Dirt",
                    Biome::Rock => "Rock",
                    Biome::Crops => "Crops",
                    Biome::Path => "Path",
                    Biome::Water => "Water",
                })
                .or_default() += 1;
                let entry = palette.entry(biome as u8).or_insert((0, 0, 0, 0));
                entry.0 += avg[0] as u64;
                entry.1 += avg[1] as u64;
                entry.2 += avg[2] as u64;
                entry.3 += 1;
                let ok = asset.image.width > 0
                    && asset.image.height > 0
                    && asset.image.rgba.len() >= 4;
                if !ok {
                    failures += 1;
                }
                biome_tiles.push((thumbnail(&asset.image, 64), ok));
            }
            Err(error) => {
                failures += 1;
                eprintln!("AUDIT biome FAIL {name}: {error}");
                biome_tiles.push((vec![0; 64 * 64 * 4], false));
            }
        }
    }
    write_sheet(&out.join("biomes-contact-sheet.ppm"), &biome_tiles, 64, 12);

    // 6x1 palette strip from the decoded biome averages.
    let mut palette_pixels = vec![0u8; 6 * 64 * 32 * 3];
    for (index, (_, (r, g, b, count))) in palette.iter().enumerate() {
        let rgb = if *count == 0 {
            [90, 90, 90]
        } else {
            [
                (r / count) as u8,
                (g / count) as u8,
                (b / count) as u8,
            ]
        };
        for y in 0..32 {
            for x in 0..64 {
                let dst = ((y * (6 * 64) + index as u32 * 64 + x) * 3) as usize;
                palette_pixels[dst..dst + 3].copy_from_slice(&rgb);
            }
        }
    }
    let mut ppm = format!("P6\n{} {}\n255\n", 6 * 64, 32).into_bytes();
    ppm.extend_from_slice(&palette_pixels);
    let _ = std::fs::write(out.join("biome-palette.ppm"), ppm);

    let mut icon_tiles = Vec::with_capacity(icons.len());
    let mut icon_failures = 0usize;
    for name in &icons {
        match db.texture_shared(name) {
            Ok(asset) => {
                let ok = asset.image.width > 0 && asset.image.height > 0;
                if !ok {
                    icon_failures += 1;
                }
                icon_tiles.push((thumbnail(&asset.image, 48), ok));
            }
            Err(error) => {
                icon_failures += 1;
                eprintln!("AUDIT icon FAIL {name}: {error}");
                icon_tiles.push((vec![0; 48 * 48 * 4], false));
            }
        }
    }
    write_sheet(&out.join("icons-contact-sheet.ppm"), &icon_tiles, 48, 32);

    let mut team_failures = 0usize;
    for name in &team {
        if db.texture_shared(name).is_err() {
            team_failures += 1;
        }
    }

    let mut report = String::from("# Biome / icon audit\n\n");
    report.push_str(&format!(
        "- TerrainArt BLPs: **{}** decoded, {} failed\n",
        terrain.len(),
        failures
    ));
    report.push_str(&format!(
        "- CommandButtons: **{}** decoded, {} failed\n",
        icons.len(),
        icon_failures
    ));
    report.push_str(&format!(
        "- TeamColor/TeamGlow: **{}** decoded, {} failed\n",
        team.len(),
        team_failures
    ));
    report.push_str("\n## Biome distribution (all TerrainArt BLP stems)\n\n");
    for (biome, count) in &biome_counts {
        report.push_str(&format!("- {biome}: {count}\n"));
    }
    report.push_str("\n## Biome palette (mean decoded sRGB)\n\n");
    for (biome, (r, g, b, count)) in &palette {
        if *count > 0 {
            report.push_str(&format!(
                "- {:?}: ({}, {}, {}) from {count} tiles\n",
                Biome::from_index(*biome),
                r / count,
                g / count,
                b / count
            ));
        }
    }
    report.push_str("\n## Terrain tiles (decoded mean sRGB -> biome material)\n\n| # | tile | mean sRGB | biome |\n|---|---|---|---|\n");
    for (index, name) in terrain.iter().enumerate() {
        let tag = name.rsplit('\\').next().unwrap_or(name);
        let avg = db
            .texture_shared(name)
            .map(|asset| average(&asset.image))
            .unwrap_or([0, 0, 0]);
        report.push_str(&format!(
            "| {index} | {tag} | ({}, {}, {}) | {:?} |\n",
            avg[0],
            avg[1],
            avg[2],
            classify(tag.trim_end_matches(".blp"), avg, false)
        ));
    }
    let _ = std::fs::write(out.join("biomes-audit.md"), &report);

    println!(
        "AUDIT_WORLD terrain={} terrain_failed={} icons={} icon_failed={} team={} team_failed={} biomes={biome_counts:?}",
        terrain.len(),
        failures,
        icons.len(),
        icon_failures,
        team.len(),
        team_failures
    );
}

/// Same in-range-key rule as `Assets::mesh_layer_alpha`, applied to a raw MDX
/// so the audit sees the same material visibility as the game.
fn layer_alpha_at(
    layer: &crate::assets::MeshLayer,
    sequence: Option<&Sequence>,
    frame: u32,
) -> f32 {
    match (&layer.alpha_track, sequence) {
        (Some(track), Some(sequence)) => {
            let has_key = track
                .keys
                .iter()
                .any(|key| key.time >= sequence.start && key.time <= sequence.end);
            if has_key {
                track.sample(frame).unwrap_or(layer.alpha).clamp(0.0, 1.0)
            } else {
                layer.alpha
            }
        }
        _ => layer.alpha,
    }
}

/// True when at least one **opaque/alpha-test** material layer is visible at
/// `frame`. Models whose only visible layers are blended/additive are effect
/// models: they can legitimately be invisible against a dark thumbnail.
fn any_opaque_layer_visible(model: &MdxModel, sequence: Option<&Sequence>, frame: u32) -> bool {
    model.materials.iter().any(|material| {
        material.layers.iter().any(|layer| {
            layer.blend_mode < 2
                && layer_alpha_at(
                    &crate::assets::MeshLayer {
                        texture: 0,
                        alpha: layer.alpha,
                        alpha_track: layer.alpha_track.clone(),
                        blend: layer.blend_mode,
                        team_color: false,
                        uv_set: 0,
                    },
                    sequence,
                    frame,
                ) > 0.02
        })
    })
}

/// Guard the W3E tile interpretation itself: every tile in the map must map to
/// a known biome and every water cell must produce water geometry.
#[test]
fn agent_world_tile_biomes_cover_map() {
    let Some(path) = mpq_path() else {
        eprintln!("SKIP agent_world_tile_biomes: War3.mpq not present");
        return;
    };
    let _ = path;
    let map_path = repo_root().join("analysis/war3_map/Maps/Campaign/Human04.w3m");
    if !map_path.exists() {
        eprintln!("SKIP agent_world_tile_biomes: map not present");
        return;
    }
    let bundle = wc3_re::mapfiles::MapBundle::open(&map_path).expect("map parses");
    let terrain = bundle.terrain.as_ref().expect("map terrain");
    let mut seen = std::collections::BTreeMap::new();
    for tile in &terrain.tiles {
        let key = if tile.is_water() {
            "Water".to_string()
        } else {
            format!("tag{}", tile.texture())
        };
        *seen.entry(key).or_insert(0usize) += 1;
    }
    println!(
        "TILE_USAGE textures={} tag_counts={seen:?}",
        terrain.ground_tiles.len()
    );
    assert!(
        terrain.width > 2 && terrain.height > 2,
        "map has a real grid"
    );
}

#[test]
#[ignore = "diagnostic dump: model mesh/alpha census; needs War3.mpq; run_layer_gates.sh --ignored"]
fn agent_world_dump_player_models() {
    let Some((mpq, map)) = crate::world_audit::mpq_path().map(|mpq| {
        let map = repo_root().join("analysis/war3_map/Maps/Campaign/Human04.w3m");
        (mpq, map)
    }) else {
        return;
    };
    let assets = crate::assets::Assets::load(Some(&mpq), Some(&map));
    for kind in crate::assets::MODEL_KINDS {
        let index = assets.models.iter().position(|model| model.kind == kind).unwrap();
        let model = &assets.models[index];
        let sequence = model
            .sequences
            .iter()
            .find(|sequence| sequence.name.to_ascii_lowercase().contains("stand"))
            .cloned();
        let alphas = assets.mesh_alpha(index, sequence.as_ref(), sequence.as_ref().map(|s| s.start).unwrap_or(0));
        println!("MODEL {kind:?} height={:.1} meshes={}", model.height, model.meshes.len());
        for (mesh_index, mesh) in model.meshes.iter().enumerate() {
            println!(
                "  mesh{mesh_index} verts={} tris={} alpha={:.3} hide={} textures={:?}",
                mesh.vertex_count(),
                mesh.triangle_count(),
                alphas.get(mesh_index).copied().unwrap_or(-1.0),
                mesh.hide,
                mesh.texture_paths
            );
        }
    }
}

#[test]
#[ignore = "diagnostic pose render: needs War3.mpq + GPU; run_layer_gates.sh --ignored"]
fn agent_world_debug_poses() {
    let Some((mpq, map)) = crate::world_audit::mpq_path().map(|mpq| {
        (mpq, repo_root().join("analysis/war3_map/Maps/Campaign/Human04.w3m"))
    }) else {
        return;
    };
    let assets = crate::assets::Assets::load(Some(&mpq), Some(&map));
    for kind in [wc3_sim::UnitKind::Footman, wc3_sim::UnitKind::Archer] {
        let index = assets.models.iter().position(|model| model.kind == kind).unwrap();
        let model = &assets.models[index];
        println!(
            "POSE {kind:?} sequences={:?}",
            model.sequences.iter().map(|s| (s.name.clone(), s.start, s.end)).collect::<Vec<_>>()
        );
        for (label, time) in [("t0", 0.0f32), ("t400", 0.4)] {
            for (flip, facing) in [("front", std::f32::consts::PI), ("back", 0.0)] {
                let sequence = model
                    .sequence(crate::assets::SequenceKind::Idle)
                    .and_then(|sequence_index| model.sequences.get(sequence_index).cloned());
                let mut animator = wc3_anim::Animator::from_model(model.model.as_ref().unwrap());
                if let Some(sequence) = &sequence {
                    animator.play(sequence);
                    animator.update(time);
                }
                let nodes = animator.world_matrices();
                let frame = animator.frame_time();
                let unit = crate::game::UnitDraw {
                    model: index,
                    world: Mat4::from_scale_rotation_translation(
                        Vec3::ONE,
                        glam::Quat::from_rotation_y(-facing),
                        Vec3::new(2000.0, 0.0, 2000.0),
                    ),
                    tint: [0.55, 0.72, 1.0, 1.0],
                    nodes,
                    mesh_alpha: assets.mesh_alpha(index, sequence.as_ref(), frame),
                    layer_alpha: assets.mesh_layer_alpha(index, sequence.as_ref(), frame),
                };
                let mut camera = crate::Camera::new(assets.world_size);
                camera.viewport = glam::Vec2::new(640.0, 640.0);
                camera.target = glam::Vec2::new(2000.0, 2000.0);
                camera.distance = 620.0;
                let scene = crate::Scene {
                    view_proj: camera.view_proj(),
                    eye: camera.eye(),
                    viewport: camera.viewport,
                    units: vec![unit],
                    rings: Vec::new(),
                    health: Vec::new(),
                    ui: crate::hud::UiBatch::default(),
                    overlay: None,
                };
                let rgba = crate::render::render_scene_to_rgba_with(
                    &assets,
                    &crate::assets::TextureImage::white(),
                    None,
                    &scene,
                    640,
                    640,
                    crate::render::RenderOptions { skip_terrain: true, skip_ui: true, ..Default::default() },
                )
                .expect("headless");
                let path = out_dir().join(format!("pose-{kind:?}-{label}-{flip}.ppm").to_lowercase());
                crate::render::write_ppm(path.to_str().unwrap(), 640, 640, &rgba);
                println!("POSE wrote {} frame={frame}", path.display());
            }
        }
    }
}

#[test]
#[ignore = "diagnostic: canonical doodad art and Water.slk; needs War3.mpq; run_layer_gates.sh --ignored"]
fn agent_world_debug_canonical_resources() {
    let Some((mpq, map_path)) = crate::world_audit::mpq_path().map(|mpq| {
        (
            mpq,
            repo_root().join("analysis/war3_map/Maps/Campaign/Human04.w3m"),
        )
    }) else {
        return;
    };
    let assets = crate::assets::Assets::load(Some(&mpq), Some(&map_path));
    let install = mpq.parent().expect("mpq parent");
    let layer = crate::re::ReLayer::open(install, Some(&map_path));
    println!("NOTES {:?}", layer.notes);
    if let Some(objects) = layer.objects.as_ref() {
        println!("OBJECTS count={} sheets={}", objects.objects.len(), objects.sheets.len());
        for id in ["VTlt", "LTlt", "LPrs", "LRrk"] {
            match objects.record(id) {
                Some(record) => {
                    let keys: Vec<&String> = record.slk_fields.keys().take(12).collect();
                    println!(
                        "RECORD {id} kind={:?} dir={:?} file={:?} numVar={:?} texID={:?} texFile={:?} keys={:?}",
                        record.kind,
                        record.slk_field("Doodads", "dir"),
                        record.slk_field("Doodads", "file"),
                        record.slk_field("Doodads", "numVar"),
                        record.slk_field("DestructableData", "texID"),
                        record.slk_field("DestructableData", "texFile"),
                        keys
                    );
                    println!(
                        "  destructable dir={:?} file={:?} numVar={:?}",
                        record.slk_field("DestructableData", "dir"),
                        record.slk_field("DestructableData", "file"),
                        record.slk_field("DestructableData", "numVar")
                    );
                }
                None => println!("RECORD {id} MISSING"),
            }
        }
    } else {
        println!("OBJECTS MISSING");
    }
    let storm = &layer.storm;
    for path in [
        "Doodads\\Terrain\\LordaeronTree\\LordaeronTree6.mdx",
        "Doodads\\Terrain\\LordaeronTree6.mdx",
        "Doodads\\Terrain\\LordaeronTree\\LordaeronTree0.mdx",
        "Doodads\\Terrain\\LordaeronTree\\LordaeronTree6.mdl",
        "Doodads\\LordaeronSummer\\Plants\\RiverRushes\\RiverRushes0.mdx",
    ] {
        println!(
            "READ {path} exists={} err={:?}",
            storm.exists(path),
            storm.read(path).err().map(|error| error.to_string())
        );
    }
    match storm.read("TerrainArt\\Water.slk") {
        Ok(bytes) => {
            println!("WATER_SLK bytes={}", bytes.len());
            match wc3_re::slk::Sheet::parse("Water", &bytes) {
                Ok(sheet) => {
                    println!("WATER_SHEET rows={} cols={:?}", sheet.len(), sheet.column_names().iter().take(8).collect::<Vec<_>>());
                    for row in 0..sheet.len().min(8) {
                        let record = sheet.record(row);
                        println!(
                            "WATER_ROW {row} id={:?} texFile={:?} texRate={:?}",
                            record.get("waterID"),
                            record.get("texFile"),
                            record.get("texRate")
                        );
                    }
                    for row in 0..sheet.len() {
                        if sheet.cell(row, "waterID") == Some("VSha") {
                            println!("WATER_VSHA {:?}", sheet.record(row));
                        }
                    }
                }
                Err(error) => println!("WATER_SHEET ERR {error}"),
            }
        }
        Err(error) => println!("WATER_SLK ERR {error}"),
    }
    for path in [
        "ReplaceableTextures\\Water\\Water.blp",
        "ReplaceableTextures\\Water\\Water00.blp",
        "ReplaceableTextures\\Water\\V_Water00.blp",
        "ReplaceableTextures\\Water\\L_Water00.blp",
        "ReplaceableTextures\\Water\\A_Water00.blp",
        "ReplaceableTextures\\Water\\N_Water00.blp",
        "ReplaceableTextures\\Water\\B_Water00.blp",
        "TerrainArt\\Water\\Water.blp",
    ] {
        println!(
            "WATER_BLP {path} exists={} err={:?}",
            storm.exists(path),
            storm.read(path).err().map(|error| error.to_string())
        );
    }
    let _ = assets;
}

#[test]
#[ignore = "diagnostic: close camera + terrain vertex layout + doodad scales; needs War3.mpq; run_layer_gates.sh --ignored"]
fn agent_world_debug_close_camera() {
    let Some((mpq, map_path)) = crate::world_audit::mpq_path().map(|mpq| {
        (
            mpq,
            repo_root().join("analysis/war3_map/Maps/Campaign/Human04.w3m"),
        )
    }) else {
        return;
    };
    let assets = crate::assets::Assets::load(Some(&mpq), Some(&map_path));
    println!(
        "ASSETS source={} debug={:?} models={} textures={} fallback_models={} tile_images={} biomes={}",
        assets.source,
        assets.debug,
        assets.models.len(),
        assets.textures.len(),
        assets.models.iter().filter(|model| model.fallback).count(),
        assets.terrain_tile_images.len(),
        assets.tile_biome.len()
    );
    let tiles = crate::terrain_render::load_tile_set(&assets);
    let mesh = crate::terrain_render::build_terrain(&assets, &tiles);
    println!(
        "LAYOUT TerrainVertex size={} stride={} offsets=0,12,24,32,48,64",
        std::mem::size_of::<crate::terrain_render::TerrainVertex>(),
        std::mem::size_of::<crate::terrain_render::TerrainVertex>()
    );
    for index in [0usize, mesh.vertices.len() / 2, mesh.vertices.len() - 1] {
        let vertex = &mesh.vertices[index];
        println!(
            "TVERT {index} pos={:?} uv={:?} weights={:?} weights2={:?} color={:?}",
            vertex.position, vertex.uv, vertex.weights, vertex.weights2, vertex.color
        );
    }
    let doodads = crate::doodads::DoodadWorld::load(&assets);
    let mut scales: Vec<f32> = doodads.instances.iter().map(|instance| instance.scale).collect();
    scales.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = scales.get(scales.len() / 2).copied().unwrap_or(0.0);
    let footprints = doodads
        .models
        .iter()
        .map(|model| model.footprint)
        .fold(0.0f32, f32::max);
    println!(
        "DOODAD_SCALES count={} min={:.3} median={:.3} max={:.3} max_footprint={:.1}",
        scales.len(),
        scales.first().copied().unwrap_or(0.0),
        median,
        scales.last().copied().unwrap_or(0.0),
        footprints
    );
    let town_index = assets
        .models
        .iter()
        .position(|model| model.kind == wc3_sim::UnitKind::TownHall)
        .expect("town hall");
    let town_model = &assets.models[town_index];
    let town_stand = town_model
        .sequences
        .iter()
        .find(|sequence| sequence.name.eq_ignore_ascii_case("Stand"))
        .or_else(|| town_model.sequences.first())
        .cloned();
    if let Some(stand) = town_stand.as_ref() {
        let alphas = assets.mesh_alpha(town_index, Some(stand), stand.start);
        let visible = alphas.iter().filter(|alpha| **alpha > 0.01).count();
        println!(
            "TOWNHALL alphas visible={visible}/{} {:?}",
            alphas.len(),
            alphas
        );
    }
    if let Ok(bundle) = wc3_re::mapfiles::MapBundle::open(&map_path) {
        if let Ok(archive) = wc3_re::mpqex::MpqEx::open_at(&map_path, bundle.mpq_base) {
            if let Ok(bytes) = archive.read("war3map.doo")
                && let Ok(raw) = crate::doodads::parse_doo(&bytes)
            {
                let count = raw.iter().filter(|doodad| &doodad.id == b"YSdc").count();
                let total = raw.len();
                println!("BRIDGE_YSdc count={count} total_doodads={total}");
            }
        }
    }
    let mut game = crate::game::Game::new(assets, 0x5EED);
    game.resize(2414.0, 1080.0);
    game.camera.distance = 700.0;
    game.camera
        .centre_on(game.base_pos + glam::Vec2::new(180.0, 120.0));
    for _ in 0..30 {
        game.update(1.0 / 30.0);
    }
    let scene = game.scene();
    println!(
        "CAMERA viewport={:?} distance={} eye={:?}",
        scene.viewport, game.camera.distance, scene.eye
    );
    let rgba = match crate::render::render_scene_to_rgba(
        &game.assets,
        &game.font.atlas,
        game.assets.minimap.as_ref(),
        &scene,
        2414,
        1080,
    ) {
        Ok(rgba) => rgba,
        Err(error) => {
            eprintln!("SKIP close camera: {error}");
            return;
        }
    };
    let path = repo_root().join("analysis/agent-world/render-fix/close700.ppm");
    crate::render::write_ppm(path.to_str().unwrap(), 2414, 1080, &rgba);
    println!("WROTE {}", path.display());
}
