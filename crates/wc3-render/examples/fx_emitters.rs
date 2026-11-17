//! Headless proof frame for the MDX geometry emitters.
//!
//! Loads a real spell model, builds the emitter runtimes from the parsed `PRE2`
//! and `RIBB` chunks, simulates a few frames, draws the instanced emitter pass
//! plus an attachment marker and writes the frame as a PNG.
//!
//! ```text
//! cargo run -p wc3-render --example fx_emitters -- \
//!     --mdx analysis/war3_full/Abilities/Spells/Undead/FrostNova/FrostNovaTarget.mdx \
//!     --textures analysis/war3_full --png analysis/agent-gpu/fx-emitters.png
//! ```
//!
//! The summary line (`FX_EMITTERS ...`) is the machine-readable evidence: it
//! reports the emitter counts, the instance/draw counters of the pass and how
//! many pixels differ from the clear colour, so an empty PNG cannot pass as a
//! success.

use std::path::{Path, PathBuf};

use glam::{Mat4, Vec3};

use wc3_render::attachments::AttachmentSet;
use wc3_render::emitters::{EmitterContext, EmitterSystem, EmitterTextures};
use wc3_render::fx::{FxBlend, Particle};
use wc3_render::particles::{EmitterBudget, ParticleCamera, ParticlePass};
use wc3_render::perf::Counters;
use wc3_render::renderer::read_back;
use wc3_render::texture::{bind_group_layout, TextureStore};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut mdx: Option<PathBuf> = None;
    let mut attach_mdx: Option<PathBuf> = None;
    let mut textures_root: Option<PathBuf> = None;
    let mut png: Option<PathBuf> = None;
    let mut frames = 12usize;
    let mut dt = 0.05f32;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mdx" => mdx = args.next().map(PathBuf::from),
            "--attach-mdx" => attach_mdx = args.next().map(PathBuf::from),
            "--textures" => textures_root = args.next().map(PathBuf::from),
            "--png" => png = args.next().map(PathBuf::from),
            "--frames" => {
                frames = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(frames)
            }
            "--dt" => {
                dt = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(dt)
            }
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mdx = mdx.unwrap_or_else(|| {
        root.join("analysis/war3_full/Abilities/Spells/Undead/FrostNova/FrostNovaTarget.mdx")
    });
    let textures_root =
        textures_root.unwrap_or_else(|| root.join("analysis/war3_full"));
    let png = png.unwrap_or_else(|| root.join("analysis/agent-gpu/fx-emitters.png"));
    let attach_mdx = attach_mdx
        .unwrap_or_else(|| root.join("analysis/war3_full/Units/Human/Footman/Footman.mdx"));

    let bytes = std::fs::read(&mdx).unwrap_or_else(|error| {
        eprintln!("read {}: {error}", mdx.display());
        std::process::exit(1);
    });
    let model = wc3_formats::mdx::MdxModel::parse(&bytes).unwrap_or_else(|error| {
        eprintln!("parse {}: {error:?}", mdx.display());
        std::process::exit(1);
    });
    if model.particle_emitters2.is_empty() && model.ribbon_emitters.is_empty() {
        eprintln!("{} has no PRE2/RIBB emitters", mdx.display());
        std::process::exit(1);
    }

    // ---- textures: every model texture that exists on disk, BLPs with their
    // authored mip chain; missing entries fall back to a soft white sprite.
    let mut blps: Vec<(String, wc3_formats::blp::BlpImage)> = Vec::new();
    for texture in &model.textures {
        let relative = texture.path.trim_start_matches("ReplaceableTextures\\").replace('\\', "/");
        let candidate = textures_root.join(&relative);
        match std::fs::read(&candidate)
            .ok()
            .and_then(|bytes| wc3_formats::blp::decode(&bytes).ok())
        {
            Some(image) => blps.push((texture.path.clone(), image)),
            None => {
                let size = 32u32;
                let mut rgba = vec![0u8; (size * size * 4) as usize];
                for y in 0..size {
                    for x in 0..size {
                        let dx = (x as f32 - size as f32 / 2.0) / (size as f32 / 2.0);
                        let dy = (y as f32 - size as f32 / 2.0) / (size as f32 / 2.0);
                        let d = (dx * dx + dy * dy).sqrt().min(1.0);
                        let alpha = ((1.0 - d) * 255.0) as u8;
                        let index = ((y * size + x) * 4) as usize;
                        rgba[index] = 255;
                        rgba[index + 1] = 255;
                        rgba[index + 2] = 255;
                        rgba[index + 3] = alpha;
                    }
                }
                blps.push((
                    texture.path.clone(),
                    wc3_formats::blp::BlpImage {
                        width: size,
                        height: size,
                        rgba: rgba.clone(),
                        mipmaps: vec![wc3_formats::blp::MipLevel {
                            width: size,
                            height: size,
                            rgba,
                        }],
                    },
                ));
            }
        }
    }

    let gpu = Gpu::new();
    let layout = bind_group_layout(&gpu.device);
    let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("fx-emitters-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    let blp_refs: Vec<&wc3_formats::blp::BlpImage> = blps.iter().map(|(_, blp)| blp).collect();
    let texture_store: TextureStore =
        TextureStore::upload_blp_chains(&gpu.device, &gpu.queue, &layout, &sampler, &blp_refs, 256);

    // ---- animation + attachments
    let mut animator = wc3_anim::Animator::from_model(&model);
    let sequence = model
        .sequence("Stand")
        .or_else(|| model.sequences.first())
        .cloned();
    if let Some(sequence) = &sequence {
        animator.play(sequence);
    }
    let start_ms = sequence.as_ref().map(|sequence| sequence.start).unwrap_or(0);
    animator.update(0.0);
    let worlds = animator.world_matrices();
    // MDX is Z-up; the renderer's camera is Y-up.
    let world = Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let attachments = AttachmentSet::from_model(&model, &worlds, world);

    let textures = EmitterTextures {
        model_texture: (0..model.textures.len()).collect(),
        team_color: None,
        team_glow: None,
    };
    let mut system = EmitterSystem::with_quality(
        &model,
        &textures,
        0xC0FF_EE00_1234_5678,
        wc3_render::emitters::ParticleQuality::Medium,
    );

    let budget = EmitterBudget::default();

    // ---- camera: look at the effect from a 3/4 angle
    let eye = Vec3::new(300.0, 300.0, 380.0);
    let target = Vec3::new(-60.0, 20.0, 0.0);
    let view = Mat4::look_at_lh(eye, target, Vec3::Y);
    let proj = Mat4::perspective_lh(46f32.to_radians(), WIDTH as f32 / HEIGHT as f32, 1.0, 4000.0);
    let view_proj = proj * view;
    let camera = ParticleCamera::from_view_proj(&view_proj);
    let mut pass = ParticlePass::new(&gpu.device, FORMAT);

    // ---- simulate and draw
    let mut stats = Counters::default();
    let mut time_ms = start_ms;
    let mut last_stats = wc3_render::particles::ParticleStats::default();
    let mut max_particles = 0usize;
    let mut max_ribbon_points = 0usize;
    for frame in 0..frames {
        time_ms = start_ms + (frame as f32 * dt * 1000.0) as u32;
        let ctx = EmitterContext {
            node_worlds: &worlds,
            pivots: &model.pivots,
            world,
            sequence: sequence.as_ref(),
            time_ms,
            team_color: [1.0, 1.0, 1.0, 1.0],
        };
        system.update(&model, &ctx, dt);
        max_particles = max_particles.max(system.instances().len());
        for index in 0..model.particle_emitters2.len() {
            max_particles = max_particles.max(system.particle_alive(index).unwrap_or(0));
        }
        for index in 0..model.ribbon_emitters.len() {
            max_ribbon_points = max_ribbon_points.max(system.ribbon_alive(index).unwrap_or(0));
        }
        last_stats = pass.prepare(
            &gpu.device,
            &gpu.queue,
            &camera,
            system.instances(),
            system.params(),
            system.draws(),
            budget,
        );
    }

    // ---- attachment donor: a unit model with `ATCH` sockets. Spell models have
    // no attachments, so the frame needs a second model to prove that side.
    let attach_bytes = std::fs::read(&attach_mdx).ok();
    let attach_model = attach_bytes
        .as_deref()
        .and_then(|bytes| wc3_formats::mdx::MdxModel::parse(bytes).ok());
    let dump_sockets = std::env::var("FX_DUMP_SOCKETS").is_ok();
    let mut attach_note = format!("{}", attach_mdx.display());
    if let Some(model) = &attach_model {
        attach_note = format!("{} ({} sockets)", model.name, model.attachments.len());
    }
    let attach_set = {
        let mut worlds = Vec::new();
        if let Some(model) = &attach_model {
            let mut animator = wc3_anim::Animator::from_model(model);
            if let Some(sequence) = model.sequence("Stand").or_else(|| model.sequences.first()) {
                animator.play(sequence);
            }
            animator.update(0.0);
            worlds = animator.world_matrices();
        }
        // Push the donor to the left so it does not sit inside the spell effect.
        let place = Mat4::from_translation(Vec3::new(-190.0, 0.0, 0.0));
        match &attach_model {
            Some(model) => AttachmentSet::from_model(model, &worlds, place * world),
            None => AttachmentSet::default(),
        }
    };

    // ---- socket markers: one glowing quad per attachment point, drawn through
    // the splat path of the emitter pass, plus a red dot on the weapon socket.
    // This is the visual proof that `ATCH` nodes resolve to world space.
    system.set_splat_specs(vec![wc3_render::emitters::SplatSpec {
        life_span: (frames as f32 * dt) + 0.5,
        birth: 0.05,
        pause: 0.05,
        decay: 0.1,
        colors: [
            [0.2, 1.0, 0.6, 0.35],
            [0.2, 1.0, 0.6, 0.9],
            [0.1, 0.6, 0.4, 0.0],
        ],
        scale: 30.0,
        columns: 1,
        rows: 1,
        texture: 0,
        blend: wc3_render::particles::ParticleBlend::Additive,
        intervals: [[0, 0, 1]; 2],
        ubersplat: false,
    }]);
    for point in attach_set.points() {
        if dump_sockets {
            println!(
                "SOCKET name=\"{}\" slot={} node={} pos=({:.2}, {:.2}, {:.2})",
                point.name,
                point.slot.name(),
                point.node_id,
                point.position().x,
                point.position().y,
                point.position().z
            );
        }
        system.spawn_splat(0, point.position());
    }
    // Rebuild the frame with `dt = 0` so the fresh splat quads are instanced
    // (splats are turned into instances by `update`), then re-upload.
    {
        let ctx = EmitterContext {
            node_worlds: &worlds,
            pivots: &model.pivots,
            world,
            sequence: sequence.as_ref(),
            time_ms,
            team_color: [1.0, 1.0, 1.0, 1.0],
        };
        system.update(&model, &ctx, 0.0);
        last_stats = pass.prepare(
            &gpu.device,
            &gpu.queue,
            &camera,
            system.instances(),
            system.params(),
            system.draws(),
            budget,
        );
    }
    let mut markers = Vec::new();
    for slot in [
        wc3_render::attachments::AttachmentSlot::Weapon,
        wc3_render::attachments::AttachmentSlot::HandRight,
        wc3_render::attachments::AttachmentSlot::HandLeft,
        wc3_render::attachments::AttachmentSlot::Overhead,
        wc3_render::attachments::AttachmentSlot::Origin,
        wc3_render::attachments::AttachmentSlot::Chest,
        wc3_render::attachments::AttachmentSlot::Head,
        wc3_render::attachments::AttachmentSlot::FootLeft,
        wc3_render::attachments::AttachmentSlot::FootRight,
    ] {
        if let Some(point) = attach_set.slot(slot) {
            markers.push(Particle::new(
                point.position(),
                8.0,
                [1.0, 0.95, 0.3, 1.0],
                FxBlend::Additive,
            ));
        }
    }
    if let Some(point) = attach_set.weapon() {
        markers.push(Particle::new(
            point.position(),
            9.0,
            [1.0, 0.15, 0.15, 1.0],
            FxBlend::Additive,
        ));
    }
    let _ = &attachments;

    // ---- draw the emitter pass into an offscreen target
    let color = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fx-emitters-target"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fx-emitters-depth"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wc3_render::mesh::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("fx-emitters-frame"),
        });
    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fx-emitters-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.03,
                        g: 0.04,
                        b: 0.08,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.draw(&mut render_pass, &texture_store, &mut stats);
    }
    // FX markers: reuse the FxRenderer for the attachment dots.
    let mut fx = wc3_render::fx::FxRenderer::new(&gpu.device, FORMAT);
    let mut budget = wc3_render::fx::ParticleBudget::default();
    budget.max_particles = 64;
    let fx_stats = fx.prepare(&gpu.device, &gpu.queue, &view_proj, &markers, budget);
    if fx_stats.rendered > 0 {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fx-marker-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        fx.draw(&mut render_pass, &mut stats);
    }
    gpu.queue.submit(Some(encoder.finish()));

    // ---- second frame: a close-up of the attachment sockets, so the `ATCH`
    // port is legible on its own (the wide frame above proves the emitters).
    let socket_png = png.with_file_name("fx-attachments.png");
    let socket_color = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fx-sockets-target"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let socket_view = socket_color.create_view(&wgpu::TextureViewDescriptor::default());
    let socket_depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fx-sockets-depth"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wc3_render::mesh::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let socket_depth_view = socket_depth.create_view(&wgpu::TextureViewDescriptor::default());
    let socket_view_proj = Mat4::perspective_lh(
        40f32.to_radians(),
        WIDTH as f32 / HEIGHT as f32,
        1.0,
        4000.0,
    ) * Mat4::look_at_lh(
        Vec3::new(-120.0, 90.0, 230.0),
        Vec3::new(-190.0, 45.0, 0.0),
        Vec3::Y,
    );
    let mut socket_stats = Counters::default();
    let socket_fx = {
        let mut fx = wc3_render::fx::FxRenderer::new(&gpu.device, FORMAT);
        let mut budget = wc3_render::fx::ParticleBudget::default();
        budget.max_particles = 64;
        let stats = fx.prepare(&gpu.device, &gpu.queue, &socket_view_proj, &markers, budget);
        (fx, stats)
    };
    let mut socket_encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("fx-sockets-frame"),
        });
    if socket_fx.1.rendered > 0 {
        let mut render_pass = socket_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fx-sockets-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &socket_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.03,
                        g: 0.04,
                        b: 0.08,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &socket_depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        socket_fx.0.draw(&mut render_pass, &mut socket_stats);
    }
    gpu.queue.submit(Some(socket_encoder.finish()));
    let socket_pixels = read_back(
        &gpu.device,
        &gpu.queue,
        &socket_color,
        WIDTH,
        HEIGHT,
    )
    .unwrap_or_else(|error| {
        eprintln!("socket readback: {error}");
        std::process::exit(1);
    });
    if let Some(error) = write_png(&socket_png, WIDTH, HEIGHT, &socket_pixels) {
        eprintln!("{error}");
        std::process::exit(1);
    }

    let pixels = read_back(&gpu.device, &gpu.queue, &color, WIDTH, HEIGHT)
        .unwrap_or_else(|error| {
            eprintln!("readback: {error}");
            std::process::exit(1);
        });
    // The target is `Rgba8UnormSrgb`, so the clear colour is sRGB-encoded in the
    // readback: compare against the encoded value, not the linear one.
    let encode = |linear: f64| -> u8 {
        let value = if linear <= 0.003_130_8 {
            linear * 12.92
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        };
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    let background = [encode(0.03), encode(0.04), encode(0.08)];
    let lit = pixels
        .chunks_exact(4)
        .filter(|pixel| {
            (pixel[0] as i32 - background[0] as i32).abs() > 2
                || (pixel[1] as i32 - background[1] as i32).abs() > 2
                || (pixel[2] as i32 - background[2] as i32).abs() > 2
        })
        .count();
    if let Some(error) = write_png(&png, WIDTH, HEIGHT, &pixels) {
        eprintln!("{error}");
        std::process::exit(1);
    }
    let mut kind_instances = [0u32; 4];
    for draw in system.draws() {
        kind_instances[draw.kind as usize] += draw.count;
    }
    println!(
        "FX_EMITTERS particles={} ribbons={} splats={} ubersplats={} model=\"{}\" pre2={} ribb={} attachments={} attach_model=\"{}\" instances={} draws={} vertex_invocations={} max_instances={} max_ribbon_points={} markers={} lit_pixels={} png=\"{}\"",
        kind_instances[0],
        kind_instances[1],
        kind_instances[2],
        kind_instances[3],
        model.name,
        model.particle_emitters2.len(),
        model.ribbon_emitters.len(),
        attach_set.len(),
        attach_note,
        last_stats.instances,
        last_stats.draws,
        last_stats.vertex_invocations,
        max_particles,
        max_ribbon_points,
        markers.len() + system.splat_alive(),
        lit,
        png.display()
    );
    println!(
        "FX_ATTACHMENTS sockets={} markers={} lit_pixels={} png=\"{}\"",
        attach_set.len(),
        markers.len(),
        socket_pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[0] > 80 || pixel[1] > 80)
            .count(),
        socket_png.display()
    );
    if last_stats.draws == 0 || lit == 0 {
        eprintln!("frame is empty: draws={} lit_pixels={lit}", last_stats.draws);
        std::process::exit(1);
    }
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl Gpu {
    fn new() -> Self {
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
        .unwrap_or_else(|error| {
            eprintln!("no Vulkan adapter: {error}");
            std::process::exit(1);
        });
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("fx-emitters"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .unwrap_or_else(|error| {
            eprintln!("request_device: {error}");
            std::process::exit(1);
        });
        Self { device, queue }
    }
}

// ------------------------------------------------------------------ PNG writer

/// Minimal RGBA8 PNG encoder (no external crate): signature, IHDR, IDAT with a
/// zlib stream built from stored deflate blocks, IEND.
fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Option<String> {
    if rgba.len() != (width * height * 4) as usize {
        return Some(format!(
            "pixel buffer is {} bytes, expected {}",
            rgba.len(),
            width * height * 4
        ));
    }
    if let Some(parent) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            return Some(format!("create {}: {error}", parent.display()));
        }
    }
    let mut raw = Vec::with_capacity((width * height * 4 + height) as usize);
    for row in 0..height {
        raw.push(0); // filter: none
        let start = (row * width * 4) as usize;
        raw.extend_from_slice(&rgba[start..start + width as usize * 4]);
    }
    let mut zlib = Vec::new();
    zlib.extend_from_slice(&[0x78, 0x01]); // zlib header, no preset dict
    let mut offset = 0usize;
    while offset < raw.len() {
        let chunk = (raw.len() - offset).min(65_535);
        let last = offset + chunk >= raw.len();
        zlib.push(if last { 1 } else { 0 }); // BFINAL, BTYPE=00 (stored)
        let len = chunk as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(&raw[offset..offset + chunk]);
        offset += chunk;
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw {
        a = (a + *byte as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    let adler = (b << 16) | a;
    zlib.extend_from_slice(&adler.to_be_bytes());

    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    push_chunk(&mut png, b"IHDR", &ihdr);
    push_chunk(&mut png, b"IDAT", &zlib);
    push_chunk(&mut png, b"IEND", &[]);
    match std::fs::write(path, &png) {
        Ok(()) => None,
        Err(error) => Some(format!("write {}: {error}", path.display())),
    }
}

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = 0xFFFF_FFFFu32;
    for byte in kind.iter().chain(data.iter()) {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    out.extend_from_slice(&(!crc).to_be_bytes());
}
