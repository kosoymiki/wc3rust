//! Full asset gallery: every MDX model, every TerrainArt tile, every command
//! icon in the operator's extracted Warcraft III tree.
//!
//! Run on the device (Vulkan/Turnip headless) with:
//!   cargo test --release -p wc3-game --test asset_gallery -- --ignored --nocapture
//!
//! Env:
//!   GALLERY_ROOT  asset tree (default analysis/war3_full)
//!   GALLERY_OUT   output dir (default analysis/agent-qa/gallery)
//!   GALLERY_LIMIT max models per category for a smoke run (0 = all)
//!   GALLERY_ONLY  comma list: models,icons,terrain
//!
//! Outputs:
//!   models/<Category>/<stem>.ppm + models/manifest.jsonl + models/summary.json
//!   icons/<name>.ppm + icons/manifest.jsonl + icons/summary.json
//!   terrain/<tileset>_<stem>.ppm + terrain/manifest.jsonl + terrain/summary.json
//!
//! The thumbnails are renderer output (not file listings): the same wgpu
//! pipeline the game uses, with real MDX geosets and BLP textures, so a missing
//! texture / black material / corpse geoset is visible, not just counted.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use glam::{Mat4, Vec2, Vec3};
use wc3_assetdb::AssetDb;
use wc3_formats::mdx::{MdxModel, Sequence, Texture};
use wc3_game::assets::{MeshData, MeshLayer};
use wc3_game::game::UnitDraw;
use wc3_game::hud::UiBatch;
use wc3_game::{render, Assets, Font, Scene};
use wc3_sim::UnitKind;

const THUMB: u32 = 160;
const ICON: u32 = 64;
const TERRAIN: u32 = 256;

fn env_path(key: &str, default: &str) -> PathBuf {
    std::env::var(key)
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(default)
        })
}

fn walk(root: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(read) = fs::read_dir(root) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => walk(&path, ext, out),
            Ok(kind) if kind.is_file()
                && path
                    .extension()
                    .map(|e| e.to_string_lossy().eq_ignore_ascii_case(ext))
                    .unwrap_or(false) =>
            {
                out.push(path);
            }
            _ => {}
        }
    }
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('/', "\\")
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "model".into())
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

fn resize_rgba(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    if sw == 0 || sh == 0 {
        return vec![0; (dw * dh * 4) as usize];
    }
    let mut out = vec![0u8; (dw * dh * 4) as usize];
    for y in 0..dh {
        let sy = (y as u64 * sh as u64 / dh as u64).min(sh as u64 - 1) as u32;
        for x in 0..dw {
            let sx = (x as u64 * sw as u64 / dw as u64).min(sw as u64 - 1) as u32;
            let si = ((sy * sw + sx) * 4) as usize;
            let di = ((y * dw + x) * 4) as usize;
            out[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    out
}

fn write_rgb_ppm(path: &Path, w: u32, h: u32, rgba: &[u8], bg: [u8; 3]) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    for px in rgba.chunks_exact(4) {
        let a = px[3] as f32 / 255.0;
        for channel in 0..3 {
            out.push((px[channel] as f32 * a + bg[channel] as f32 * (1.0 - a)) as u8);
        }
    }
    let _ = fs::write(path, out);
}

fn write_json_line(file: &mut fs::File, value: &str) {
    use std::io::Write;
    let _ = writeln!(file, "{value}");
}

fn json_str(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn texture_index(
    db: &mut AssetDb,
    textures: &mut Vec<wc3_game::assets::TextureImage>,
    cache: &mut HashMap<String, usize>,
    texture: &Texture,
) -> (usize, Option<String>) {
    let path = match texture.replaceable_id {
        1 => "ReplaceableTextures\\TeamColor\\TeamColor00.blp".to_string(),
        2 => "ReplaceableTextures\\TeamGlow\\TeamGlow00.blp".to_string(),
        _ => texture.path.trim_end_matches('\0').replace('/', "\\"),
    };
    if path.is_empty() {
        return (0, None);
    }
    let key = path.to_ascii_lowercase();
    if let Some(&index) = cache.get(&key) {
        return (index, None);
    }
    match db.texture(&path) {
        Ok(image) => {
            let rgba = resize_rgba(&image.rgba, image.width, image.height, 128, 128);
            textures.push(wc3_game::assets::TextureImage {
                width: 128,
                height: 128,
                rgba,
            });
            let index = textures.len() - 1;
            cache.insert(key, index);
            (index, None)
        }
        Err(_) => (0, Some(path)),
    }
}

fn build_meshes(db: &mut AssetDb, textures: &mut Vec<wc3_game::assets::TextureImage>, cache: &mut HashMap<String, usize>, model: &MdxModel) -> (Vec<MeshData>, Vec<String>, Vec<usize>) {
    let mut texture_map = Vec::with_capacity(model.textures.len());
    let mut missing = Vec::new();
    for texture in &model.textures {
        let (index, absent) = texture_index(db, textures, cache, texture);
        if let Some(path) = absent {
            missing.push(path);
        }
        texture_map.push(index);
    }
    let mut meshes = Vec::new();
    for (geoset_id, geoset) in model.geosets.iter().enumerate() {
        let mut layers: Vec<MeshLayer> = Vec::new();
        let mut texture_paths: Vec<String> = Vec::new();
        if let Some(material) = model.materials.get(geoset.material_id as usize) {
            for layer in &material.layers {
                let Some(&texture) = texture_map.get(layer.texture_id as usize) else {
                    continue;
                };
                let source = model.textures.get(layer.texture_id as usize);
                let team_color = source
                    .map(|t| t.replaceable_id == 1 || t.replaceable_id == 2)
                    .unwrap_or(false);
                if let Some(source) = source {
                    texture_paths.push(
                        source
                            .path
                            .trim_end_matches('\0')
                            .replace('/', "\\")
                            .to_ascii_lowercase(),
                    );
                    if team_color {
                        texture_paths.push("teamcolor".to_string());
                    }
                }
                layers.push(MeshLayer {
                    texture,
                    alpha: layer.alpha,
                    alpha_track: layer.alpha_track.clone(),
                    blend: layer.blend_mode,
                    team_color,
                    // `wc3_formats::mdx::Layer` names the UV set `coord_id`.
                    uv_set: layer.coord_id,
                });
            }
        }
        if layers.is_empty() {
            layers.push(MeshLayer::plain(0));
        }
        let texture = layers[0].texture;
        let uvs = geoset
            .uv_sets
            .first()
            .cloned()
            .unwrap_or_else(|| vec![Vec2::ZERO; geoset.vertices.len()]);
        let vertex_count = geoset.vertices.len() as u32;
        let mut indices = Vec::with_capacity(geoset.faces.len());
        for triangle in geoset.faces.chunks_exact(3) {
            let (a, b, c) = (
                triangle[0] as u32,
                triangle[1] as u32,
                triangle[2] as u32,
            );
            if a < vertex_count && b < vertex_count && c < vertex_count && a != b && b != c && a != c {
                indices.extend_from_slice(&[a, b, c]);
            }
        }
        if indices.is_empty() || geoset.vertices.is_empty() {
            continue;
        }
        let mesh = MeshData {
            uvs2: geoset
                .uv_sets
                .get(1)
                .map(|extra| {
                    extra
                        .iter()
                        .copied()
                        .chain(std::iter::repeat(geoset.uv_sets[0].get(0).copied().unwrap_or_default()))
                        .take(geoset.vertices.len())
                        .collect()
                })
                .unwrap_or_default(),
            positions: geoset.vertices.clone(),
            normals: geoset
                .normals
                .iter()
                .copied()
                .chain(std::iter::repeat(Vec3::Y))
                .take(geoset.vertices.len())
                .collect(),
            uvs,
            indices,
            bones: vec![Vec::new(); geoset.vertices.len()],
            layers,
            texture,
            texture_paths,
            z_up: true,
            geoset_id: geoset_id as u32,
            hide: false,
            compact: None,
        };
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for p in &mesh.positions {
            min = min.min(*p);
            max = max.max(*p);
        }
        let span = max - min;
        let hide = mesh.positions.len() <= 4 && span.max_element() > 400.0;
        meshes.push(MeshData { hide, ..mesh });
    }
    (meshes, missing, texture_map)
}

fn stand_sequence(model: &MdxModel) -> Option<&Sequence> {
    model
        .sequences
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case("Stand") || s.name.to_ascii_lowercase().contains("stand"))
        .or_else(|| model.sequences.first())
}

fn corpse_effect_visible(model: &MdxModel, meshes: &[MeshData]) -> Vec<u32> {
    let Some(stand) = stand_sequence(model) else {
        return Vec::new();
    };
    let alphas = wc3_game::assets::mesh_alphas_for(model, meshes, Some(stand), stand.start);
    let mut out = Vec::new();
    for (index, mesh) in meshes.iter().enumerate() {
        if !mesh.is_corpse_or_effect() || mesh.hide {
            continue;
        }
        if alphas.get(index).copied().unwrap_or(0.0) > 0.5 {
            out.push(mesh.geoset_id);
        }
    }
    out
}

fn facing(model: &MdxModel) -> (String, Option<[f32; 2]>) {
    let Some(walk) = model
        .sequences
        .iter()
        .find(|s| s.name.to_ascii_lowercase().contains("walk"))
    else {
        return ("unknown".into(), None);
    };
    let mut best: Option<(f32, Vec3)> = None;
    for bone in &model.bones {
        if bone.node.parent_id != -1 {
            continue;
        }
        let Some(start) = bone.node.translation.sample(walk.start) else { continue };
        let Some(end) = bone.node.translation.sample(walk.end.max(walk.start + 1)) else { continue };
        let delta = end - start;
        let len = delta.length();
        if len > 1.0 && best.map(|(l, _)| len > l).unwrap_or(true) {
            best = Some((len, delta));
        }
    }
    if let Some((_, delta)) = best {
        let dir = Vec2::new(delta.x, delta.y).normalize_or_zero();
        // WC3 model-space front is +X (see assets.rs yaw_offset_for: Archer is
        // authored backwards and needs PI).
        let score = dir.x;
        return (
            if score < -0.35 {
                "backward".into()
            } else if score > 0.35 {
                "forward".into()
            } else {
                "sideways".into()
            },
            Some([dir.x, dir.y]),
        );
    }
    ("unknown".into(), None)
}

fn model_bounds(meshes: &[MeshData]) -> (Vec3, Vec3) {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for mesh in meshes {
        for p in &mesh.positions {
            let p = Vec3::new(p.x, p.z, -p.y);
            min = min.min(p);
            max = max.max(p);
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return (Vec3::ZERO, Vec3::ONE);
    }
    (min, max)
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    target: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    width: u32,
    height: u32,
}

impl Gpu {
    fn new(width: u32, height: u32) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            flags: wgpu::InstanceFlags::empty(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|e| format!("request_adapter: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("gallery-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .map_err(|e| format!("request_device: {e}"))?;
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gallery-target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let padded = width * 4;
        let padded = padded.div_ceil(256) * 256;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gallery-readback"),
            size: (padded as u64) * height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Ok(Self { device, queue, target, view, readback, width, height })
    }

    fn read(&self) -> Result<Vec<u8>, String> {
        let bytes_per_row = self.width * 4;
        let padded = bytes_per_row.div_ceil(256) * 256;
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("gallery-copy") });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
        self.queue.submit(Some(encoder.finish()));
        let slice = self.readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel::<Result<(), wgpu::BufferAsyncError>>();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::Wait { submission_index: None, timeout: None })
            .map_err(|e| format!("poll: {e}"))?;
        receiver
            .recv()
            .map_err(|e| format!("map channel: {e}"))?
            .map_err(|e| format!("map: {e}"))?;
        let mapped = slice.get_mapped_range().map_err(|e| format!("mapped: {e}"))?;
        let mut out = Vec::with_capacity((bytes_per_row * self.height) as usize);
        for row in 0..self.height {
            let start = (row * padded) as usize;
            out.extend_from_slice(&mapped[start..start + bytes_per_row as usize]);
        }
        drop(mapped);
        self.readback.unmap();
        Ok(out)
    }
}

fn render_model(
    gpu: &mut Gpu,
    renderer: &mut render::Renderer,
    index: usize,
    meshes: &[MeshData],
    model: &MdxModel,
    font: &ocs_font::FontAtlas,
) -> Result<(), String> {
    // Pose at Stand: buildings assemble through node transforms and the game
    // renderer skins every unit from the palette, so an empty node list leaves
    // skinned geosets at the origin (bind pose / invisible).
    let stand = model
        .sequences
        .iter()
        .find(|sequence| sequence.name.to_ascii_lowercase().contains("stand"))
        .or_else(|| model.sequences.first());
    let mut animator = wc3_anim::Animator::from_model(model);
    if let Some(sequence) = stand {
        animator.play(sequence);
        animator.update(0.0);
    }
    let nodes = animator.world_matrices();
    let (min, max) = model_bounds(meshes);
    let centre = (min + max) * 0.5;
    let size = (max - min).max_element().max(32.0);
    let eye = centre + Vec3::new(size * 1.15, size * 0.95, size * 1.45);
    let view_proj = Mat4::perspective_rh(42f32.to_radians(), 1.0, 1.0, size * 40.0)
        * Mat4::look_at_rh(eye, centre, Vec3::Y);
    let scene = Scene {
        view_proj,
        eye,
        viewport: Vec2::new(gpu.width as f32, gpu.height as f32),
        units: vec![UnitDraw {
            model: index,
            world: Mat4::IDENTITY,
            tint: [0.72, 0.80, 1.0, 1.0],
            nodes,
            mesh_alpha: vec![1.0; meshes.len()],
            layer_alpha: meshes.iter().map(|m| vec![1.0; m.layers.len()]).collect(),
        }],
        rings: Vec::new(),
        health: Vec::new(),
        ui: UiBatch::default(),
        overlay: None,
    };
    renderer.render(&gpu.device, &gpu.queue, &gpu.view, &scene);
    let _ = font;
    Ok(())
}

// A tiny alias so this test does not depend on wc3_game re-exporting Font.
mod ocs_font {
    pub type FontAtlas = wc3_game::assets::TextureImage;
}

fn main_gallery() {
    let root = env_path("GALLERY_ROOT", "analysis/war3_full");
    let out = env_path("GALLERY_OUT", "analysis/agent-qa/gallery");
    let limit: usize = std::env::var("GALLERY_LIMIT").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let only: Vec<String> = std::env::var("GALLERY_ONLY")
        .unwrap_or_else(|_| "models,icons,terrain".into())
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if !root.exists() {
        eprintln!("SKIP gallery: asset root missing: {}", root.display());
        return;
    }
    let mpq = crate::mpq_path();
    let Some(mpq) = mpq else {
        eprintln!("SKIP gallery: War3.mpq missing");
        return;
    };
    let font = Font::load();
    let mut gpu = match Gpu::new(THUMB, THUMB) {
        Ok(gpu) => gpu,
        Err(error) => {
            eprintln!("SKIP gallery: headless Vulkan unavailable: {error}");
            return;
        }
    };

    let mut mdx = Vec::new();
    walk(&root, "mdx", &mut mdx);
    mdx.sort();
    let mut categories: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for path in &mdx {
        let relative = rel(&root, path);
        let category = relative.split('\\').next().unwrap_or("Other").to_string();
        categories.entry(category).or_default().push(path.clone());
    }
    let total_models: usize = categories.values().map(Vec::len).sum();
    println!("GALLERY models={total_models} categories={}", categories.len());

    let db = AssetDb::open_dir(&root);
    let mut db = db;
    let models_dir = out.join("models");
    let _ = fs::create_dir_all(&models_dir);
    let mut manifest = fs::File::create(models_dir.join("manifest.jsonl")).expect("manifest");
    let mut summary = String::from("{\n  \"models\": {\n");
    let mut grand_parse = 0usize;
    let mut grand_meshes = 0usize;
    let mut grand_missing = 0usize;
    let mut grand_corpse = 0usize;
    let mut grand_backward = 0usize;
    let mut grand_unknown_facing = 0usize;
    let mut first = true;

    if only.iter().any(|v| v == "models") {
        for (category, paths) in &categories {
            let catsafe = sanitize(category);
            let cat_dir = models_dir.join(&catsafe);
            let _ = fs::create_dir_all(&cat_dir);
            let mut assets = Assets::load(Some(&mpq), None);
            let mut cache: HashMap<String, usize> = HashMap::new();
            let mut built: Vec<(usize, PathBuf, Vec<MeshData>, Vec<String>, MdxModel, (String, Option<[f32; 2]>), Vec<u32>)> = Vec::new();
            let take = if limit == 0 { paths.len() } else { paths.len().min(limit) };
            for path in paths.iter().take(take) {
                let relative = rel(&root, path);
                match db.model_shared(&relative) {
                    Ok(model) => {
                        let (meshes, missing, _map) = build_meshes(&mut db, &mut assets.textures, &mut cache, &model.model);
                        let face = facing(&model.model);
                        let corpse = corpse_effect_visible(&model.model, &meshes);
                        let sequences = model.model.sequences.clone();
                        let index = assets.models.len();
                        assets.models.push(wc3_game::assets::ModelAsset {
                            kind: UnitKind::Footman,
                            model: Some(model.model.clone()),
                            meshes: meshes.clone(),
                            sequences,
                            idle: None,
                            walk: None,
                            attack: None,
                            height: 1.0,
                            pivot: 0.0,
                            scale: 1.0,
                            yaw_offset: 0.0,
                            fallback: false,
                        });
                        built.push((index, path.clone(), meshes, missing, model.model.clone(), face, corpse));
                    }
                    Err(error) => {
                        write_json_line(
                            &mut manifest,
                            &format!(
                                "{{\"kind\":\"model\",\"path\":{},\"category\":{},\"ok\":false,\"error\":{}}}",
                                json_str(&relative),
                                json_str(category),
                                json_str(&format!("{error}"))
                            ),
                        );
                        grand_parse += 1;
                    }
                }
            }
            let mut renderer = match render::Renderer::new(
                &gpu.device,
                &gpu.queue,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                &assets,
                &font.atlas,
                None,
            ) {
                Ok(renderer) => renderer,
                Err(error) => {
                    eprintln!("SKIP category {category}: renderer: {error}");
                    continue;
                }
            };
            renderer.resize(&gpu.device, THUMB, THUMB);
            let mut rendered = 0usize;
            let mut missing_count = 0usize;
            let mut corpse_count = 0usize;
            let mut backward = 0usize;
            let mut unknown = 0usize;
            for (index, path, meshes, missing, model, face, corpse) in &built {
                if meshes.is_empty() {
                    continue;
                }
                if let Err(error) =
                    render_model(&mut gpu, &mut renderer, *index, meshes, model, &font.atlas)
                {
                    eprintln!("render {}: {error}", path.display());
                    continue;
                }
                let rgba = match gpu.read() {
                    Ok(rgba) => rgba,
                    Err(error) => {
                        eprintln!("readback {}: {error}", path.display());
                        continue;
                    }
                };
                let name = sanitize(&stem(path));
                let ppm = cat_dir.join(format!("{name}.ppm"));
                write_rgb_ppm(&ppm, THUMB, THUMB, &rgba, [18, 22, 30]);
                let (min, max) = model_bounds(meshes);
                let (face_kind, face_vec) = face;
                if !missing.is_empty() {
                    missing_count += 1;
                }
                if !corpse.is_empty() {
                    corpse_count += 1;
                }
                match face_kind.as_str() {
                    "backward" => backward += 1,
                    "unknown" => unknown += 1,
                    _ => {}
                }
                write_json_line(
                    &mut manifest,
                    &format!(
                        "{{\"kind\":\"model\",\"path\":{},\"category\":{},\"thumb\":{},\"ok\":true,\"meshes\":{},\"vertices\":{},\"missing\":{},\"unresolved\":{},\"corpse_effect_stand\":{},\"facing\":{},\"facing_vec\":{},\"bounds_min\":[{:.2},{:.2},{:.2}],\"bounds_max\":[{:.2},{:.2},{:.2}]}}",
                        json_str(&rel(&root, path)),
                        json_str(category),
                        json_str(&format!("models/{catsafe}/{name}.ppm")),
                        meshes.len(),
                        meshes.iter().map(|m| m.positions.len()).sum::<usize>(),
                        json_str(&missing.join(";")),
                        model.textures.len(),
                        json_str(&corpse.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",")),
                        json_str(face_kind),
                        face_vec
                            .map(|v| format!("[{:.3},{:.3}]", v[0], v[1]))
                            .unwrap_or_else(|| "null".into()),
                        min.x, min.y, min.z, max.x, max.y, max.z
                    ),
                );
                rendered += 1;
            }
            println!(
                "GALLERY category={category} mdx={} rendered={rendered} parse_fail={} missing={missing_count} corpse_stand={corpse_count} backward={backward} facing_unknown={unknown}",
                paths.len(),
                built.len() - rendered
            );
            grand_parse += built.len() - rendered;
            grand_meshes += rendered;
            grand_missing += missing_count;
            grand_corpse += corpse_count;
            grand_backward += backward;
            grand_unknown_facing += unknown;
            if !first {
                summary.push_str(",\n");
            }
            first = false;
            summary.push_str(&format!(
                "    \"{}\": {{\"mdx\": {}, \"rendered\": {}, \"missing_textures\": {}, \"corpse_stand\": {}, \"backward\": {}, \"facing_unknown\": {}}}",
                json_str(category).trim_matches('"'),
                paths.len(),
                rendered,
                missing_count,
                corpse_count,
                backward,
                unknown
            ));
        }
    }

    summary.push_str(&format!(
        "\n  }},\n  \"totals\": {{\"mdx\": {total_models}, \"rendered\": {grand_meshes}, \"parse_or_render_fail\": {grand_parse}, \"missing_texture_models\": {grand_missing}, \"corpse_stand_models\": {grand_corpse}, \"backward_models\": {grand_backward}, \"facing_unknown\": {grand_unknown_facing}}}\n}}\n"
    ));
    let _ = fs::write(models_dir.join("summary.json"), summary);

    if only.iter().any(|v| v == "icons") {
        gallery_icons(&root, &out, &mut db, limit);
    }
    if only.iter().any(|v| v == "terrain") {
        gallery_terrain(&root, &out, &mut db, limit);
    }
}

fn gallery_icons(root: &Path, out: &Path, db: &mut AssetDb, limit: usize) {
    let mut files = Vec::new();
    walk(root, "blp", &mut files);
    let mut icons: Vec<PathBuf> = files
        .into_iter()
        .filter(|p| {
            let r = rel(root, p);
            r.contains("CommandButtons") && !r.contains("CommandButtonsDisabled")
        })
        .collect();
    icons.sort();
    let total = icons.len();
    let dir = out.join("icons");
    let _ = fs::create_dir_all(&dir);
    let mut manifest = fs::File::create(dir.join("manifest.jsonl")).expect("icons manifest");
    let mut rendered = 0usize;
    let mut failed = 0usize;
    let mut blank = 0usize;
    let mut with_alpha = 0usize;
    let mut pink = 0usize;
    for path in icons.iter().take(if limit == 0 { total } else { total.min(limit) }) {
        let relative = rel(root, path);
        let name = sanitize(&stem(path));
        match db.texture_shared(&relative) {
            Ok(texture) => {
                let image = &texture.image;
                let rgba = resize_rgba(&image.rgba, image.width, image.height, ICON, ICON);
                let transparent = rgba.chunks_exact(4).filter(|p| p[3] < 32).count();
                let near_white = rgba
                    .chunks_exact(4)
                    .filter(|p| p[0] > 240 && p[1] > 240 && p[2] > 240)
                    .count();
                let near_black = rgba
                    .chunks_exact(4)
                    .filter(|p| p[0] < 16 && p[1] < 16 && p[2] < 16 && p[3] > 32)
                    .count();
                let magenta = rgba
                    .chunks_exact(4)
                    .filter(|p| p[0] > 180 && p[2] > 180 && p[1] < 80)
                    .count();
                let pixels = (ICON * ICON) as usize;
                if transparent > pixels / 2 {
                    blank += 1;
                }
                if transparent > pixels / 50 {
                    with_alpha += 1;
                }
                if magenta > pixels / 100 {
                    pink += 1;
                }
                write_rgb_ppm(&dir.join(format!("{name}.ppm")), ICON, ICON, &rgba, [24, 26, 32]);
                write_json_line(
                    &mut manifest,
                    &format!(
                        "{{\"kind\":\"icon\",\"path\":{},\"thumb\":{},\"ok\":true,\"size\":[{},{}],\"transparent_px\":{},\"white_px\":{},\"black_px\":{},\"pink_px\":{}}}",
                        json_str(&relative),
                        json_str(&format!("icons/{name}.ppm")),
                        image.width,
                        image.height,
                        transparent,
                        near_white,
                        near_black,
                        magenta
                    ),
                );
                rendered += 1;
            }
            Err(error) => {
                failed += 1;
                write_json_line(
                    &mut manifest,
                    &format!(
                        "{{\"kind\":\"icon\",\"path\":{},\"ok\":false,\"error\":{}}}",
                        json_str(&relative),
                        json_str(&format!("{error}"))
                    ),
                );
            }
        }
    }
    let summary = format!(
        "{{\n  \"kind\": \"icons\",\n  \"total\": {total},\n  \"rendered\": {rendered},\n  \"load_fail\": {failed},\n  \"mostly_transparent\": {blank},\n  \"has_alpha\": {with_alpha},\n  \"pink_alpha\": {pink}\n}}\n"
    );
    let _ = fs::write(dir.join("summary.json"), summary);
    println!("GALLERY icons={total} rendered={rendered} fail={failed} blank={blank} alpha={with_alpha} pink={pink}");
}

fn gallery_terrain(root: &Path, out: &Path, db: &mut AssetDb, limit: usize) {
    let mut files = Vec::new();
    walk(root, "blp", &mut files);
    let mut tiles: Vec<PathBuf> = files
        .into_iter()
        .filter(|p| rel(root, p).contains("TerrainArt"))
        .collect();
    tiles.sort();
    let total = tiles.len();
    let dir = out.join("terrain");
    let _ = fs::create_dir_all(&dir);
    let mut manifest = fs::File::create(dir.join("manifest.jsonl")).expect("terrain manifest");
    let mut rendered = 0usize;
    let mut failed = 0usize;
    let mut blackish = 0usize;
    let mut whitish = 0usize;
    for path in tiles.iter().take(if limit == 0 { total } else { total.min(limit) }) {
        let relative = rel(root, path);
        let name = sanitize(&relative.replace('\\', "_").trim_end_matches(".blp").to_string());
        match db.texture_shared(&relative) {
            Ok(texture) => {
                let image = &texture.image;
                let rgba = resize_rgba(&image.rgba, image.width, image.height, TERRAIN, TERRAIN);
                let pixels = rgba.chunks_exact(4).collect::<Vec<_>>();
                let black = pixels
                    .iter()
                    .filter(|p| p[0] < 20 && p[1] < 20 && p[2] < 20 && p[3] > 32)
                    .count();
                let white = pixels
                    .iter()
                    .filter(|p| p[0] > 240 && p[1] > 240 && p[2] > 240)
                    .count();
                if black > pixels.len() / 3 {
                    blackish += 1;
                }
                if white > pixels.len() / 3 {
                    whitish += 1;
                }
                write_rgb_ppm(&dir.join(format!("{name}.ppm")), TERRAIN, TERRAIN, &rgba, [24, 26, 32]);
                write_json_line(
                    &mut manifest,
                    &format!(
                        "{{\"kind\":\"terrain\",\"path\":{},\"thumb\":{},\"ok\":true,\"size\":[{},{}],\"black_px\":{},\"white_px\":{}}}",
                        json_str(&relative),
                        json_str(&format!("terrain/{name}.ppm")),
                        image.width,
                        image.height,
                        black,
                        white
                    ),
                );
                rendered += 1;
            }
            Err(error) => {
                failed += 1;
                write_json_line(
                    &mut manifest,
                    &format!(
                        "{{\"kind\":\"terrain\",\"path\":{},\"ok\":false,\"error\":{}}}",
                        json_str(&relative),
                        json_str(&format!("{error}"))
                    ),
                );
            }
        }
    }
    let summary = format!(
        "{{\n  \"kind\": \"terrain\",\n  \"total\": {total},\n  \"rendered\": {rendered},\n  \"load_fail\": {failed},\n  \"blackish_tiles\": {blackish},\n  \"whitish_tiles\": {whitish}\n}}\n"
    );
    let _ = fs::write(dir.join("summary.json"), summary);
    println!("GALLERY terrain={total} rendered={rendered} fail={failed} black={blackish} white={whitish}");
}

fn mpq_path() -> Option<PathBuf> {
    for candidate in [
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
        "/data/data/com.termux/files/home/src/wc3rust/analysis/war3_iso/War3.mpq",
    ] {
        if Path::new(candidate).exists() {
            return Some(PathBuf::from(candidate));
        }
    }
    None
}

#[test]
#[ignore = "heavy: builds the full asset gallery contact sheet from the MPQ; run explicitly"]
fn build_asset_gallery() {
    main_gallery();
}
