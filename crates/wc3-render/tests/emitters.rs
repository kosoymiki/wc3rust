//! Device tests for the instanced MDX emitter pass, the `Modulate` material
//! pipelines and BLP mip chains.
//!
//! Every assertion runs on a real Vulkan device: the emitters are simulated from
//! a parsed `FrostNovaTarget.mdx`, uploaded as instances, drawn with the real
//! pipelines and the frame is read back. A pass that silently stops drawing is a
//! test failure, not a blank screen.

use glam::{Mat4, Vec2, Vec3};

use wc3_render::emitters::{EmitterContext, EmitterSystem, EmitterTextures, ParticleQuality};
use wc3_render::mesh::{MeshData, MeshLayer, MeshRenderer};
use wc3_render::particles::{
    EmitterKind, ParticleBlend, ParticleCamera, ParticleDraw, ParticleInstance, ParticlePass,
};
use wc3_render::perf::Counters;
use wc3_render::renderer::read_back;
use wc3_render::scene::{PassMask, Scene, UnitDraw};
use wc3_render::texture::{
    bind_group_layout, blp_chain_plan, blp_levels, Role, TextureImage, TextureStore,
};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 180;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

fn gpu() -> Option<Gpu> {
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
    .ok()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("emitters-test"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))
    .ok()?;
    Some(Gpu { device, queue })
}

fn fixture(relative: &str) -> Option<Vec<u8>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for tree in ["analysis/war3_samples", "analysis/war3_full"] {
        let path = root.join(tree).join(relative);
        if path.exists() {
            return std::fs::read(path).ok();
        }
    }
    None
}

struct Target {
    color: wgpu::Texture,
    view: wgpu::TextureView,
    depth: wgpu::TextureView,
}

fn target(gpu: &Gpu) -> Target {
    let color = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("emitters-target"),
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
        label: Some("emitters-depth"),
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
    let depth = depth.create_view(&wgpu::TextureViewDescriptor::default());
    Target {
        color,
        view,
        depth,
    }
}

fn flat_texture(gpu: &Gpu, count: usize) -> TextureStore {
    let layout = bind_group_layout(&gpu.device);
    let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("emitters-sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    let images: Vec<TextureImage> = (0..count.max(1))
        .map(|_| TextureImage::new(16, 16, vec![255u8; 16 * 16 * 4]))
        .collect();
    TextureStore::upload(
        &gpu.device,
        &gpu.queue,
        &layout,
        &sampler,
        &images,
        Role::Color,
        64,
        true,
    )
}

fn camera() -> ParticleCamera {
    let view = Mat4::look_at_lh(
        Vec3::new(0.0, 60.0, 260.0),
        Vec3::new(0.0, 40.0, 0.0),
        Vec3::Y,
    );
    let proj = Mat4::perspective_lh(50f32.to_radians(), WIDTH as f32 / HEIGHT as f32, 1.0, 2000.0);
    ParticleCamera::from_view_proj(&(proj * view))
}

fn lit_pixels(pixels: &[u8]) -> usize {
    // Clear colour is (0.02, 0.03, 0.05) linear -> compare in sRGB space with a
    // small tolerance so anti-aliased edges count.
    pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[0] > 24 || pixel[1] > 26 || pixel[2] > 32)
        .count()
}

#[test]
fn frost_nova_emitters_draw_visible_instances() {
    let Some(gpu) = gpu() else {
        eprintln!("skipping: no Vulkan adapter");
        return;
    };
    let Some(bytes) =
        fixture("Abilities/Spells/Undead/FrostNova/FrostNovaTarget.mdx")
    else {
        eprintln!("skipping: FrostNovaTarget.mdx fixture missing");
        return;
    };
    let model = wc3_formats::mdx::MdxModel::parse(&bytes).expect("parse");
    let textures = EmitterTextures {
        model_texture: (0..model.textures.len()).collect(),
        ..Default::default()
    };
    let mut system = EmitterSystem::with_quality(
        &model,
        &textures,
        0x1234_5678,
        ParticleQuality::Medium,
    );
    let mut animator = wc3_anim::Animator::from_model(&model);
    if let Some(sequence) = model.sequences.first() {
        animator.play(sequence);
    }
    animator.update(0.0);
    let worlds = animator.world_matrices();
    // Y-up for the renderer.
    let world = Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    for frame in 0..8 {
        let Some(sequence) = model.sequences.first() else {
            break;
        };
        let context = EmitterContext {
            node_worlds: &worlds,
            pivots: &model.pivots,
            world,
            sequence: Some(sequence),
            time_ms: sequence.start + frame * 50,
            team_color: [1.0; 4],
        };
        system.update(&model, &context, 0.05);
    }
    assert!(!system.instances().is_empty(), "the pass must have instances");
    assert!(
        system
            .draws()
            .iter()
            .any(|draw| draw.kind == EmitterKind::Particle2),
        "PRE2 draw: {:?}",
        system.draws()
    );

    let mut pass = ParticlePass::new(&gpu.device, FORMAT);
    let camera = camera();
    let stats = pass.prepare(
        &gpu.device,
        &gpu.queue,
        &camera,
        system.instances(),
        system.params(),
        system.draws(),
        wc3_render::particles::EmitterBudget::default(),
    );
    assert!(stats.draws > 0, "stats {stats:?}");
    assert_eq!(stats.vertex_invocations, stats.instances * 6, "quad = 6 verts");

    let store = flat_texture(&gpu, model.textures.len());
    let target = target(&gpu);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("emitters-frame"),
        });
    let mut counters = Counters::default();
    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("emitters-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.02,
                        g: 0.03,
                        b: 0.05,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth,
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
        pass.draw(&mut render_pass, &store, &mut counters);
    }
    gpu.queue.submit(Some(encoder.finish()));
    assert_eq!(counters.draws, stats.draws);
    let pixels = read_back(&gpu.device, &gpu.queue, &target.color, WIDTH, HEIGHT)
        .expect("readback");
    let lit = lit_pixels(&pixels);
    assert!(lit > 200, "the emitter pass drew {lit} lit pixels");
}

#[test]
fn ribbon_only_frame_draws_strip_quads() {
    let Some(gpu) = gpu() else {
        eprintln!("skipping: no Vulkan adapter");
        return;
    };
    // One ribbon segment: p0/p1 are the above/below pair of the current point,
    // p2/p3 the below/above pair of the next one (donor corner order).
    let instances = [ribbon_instance(
        Vec3::new(-10.0, 20.0, 0.0),
        Vec3::new(-10.0, 0.0, 0.0),
        Vec3::new(10.0, 0.0, 0.0),
        Vec3::new(10.0, 20.0, 0.0),
    )];
    let params = wc3_render::particles::EmitterParams {
        misc: [1.0, 0.5, 1.0, 1.0],
        misc2: [1, 1, EmitterKind::Ribbon as u32, 0],
        ..Default::default()
    };
    let draws = [ParticleDraw {
        kind: EmitterKind::Ribbon,
        blend: ParticleBlend::Blend,
        start: 0,
        count: 1,
        texture: 0,
        params: 0,
        priority: 0,
    }];
    let mut pass = ParticlePass::new(&gpu.device, FORMAT);
    let stats = pass.prepare(
        &gpu.device,
        &gpu.queue,
        &camera(),
        &instances,
        &[params],
        &draws,
        wc3_render::particles::EmitterBudget::default(),
    );
    assert_eq!(stats.draws, 1);
    assert_eq!(stats.vertex_invocations, 6);

    let store = flat_texture(&gpu, 1);
    let target = target(&gpu);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ribbon-frame"),
        });
    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ribbon-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth,
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
        let mut counters = Counters::default();
        pass.draw(&mut render_pass, &store, &mut counters);
    }
    gpu.queue.submit(Some(encoder.finish()));
    let pixels = read_back(&gpu.device, &gpu.queue, &target.color, WIDTH, HEIGHT)
        .expect("readback");
    let lit = lit_pixels(&pixels);
    assert!(lit > 100, "ribbon quad drew {lit} lit pixels");
}

fn ribbon_instance(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3) -> ParticleInstance {
    ParticleInstance {
        p0: [p0.x, p0.y, p0.z, 0.0],
        p1: [p1.x, p1.y, p1.z, 0.0],
        p2: [p2.x, p2.y, p2.z, 0.0],
        p3: [p3.x, p3.y, p3.z, 0.0],
        health: 0.0,
        color: [80, 200, 255, 255],
        flags: ParticleInstance::pack_ribbon(0.0, 1.0, 0.0),
    }
}

#[test]
fn modulate_layer_darkens_the_framebuffer() {
    let Some(gpu) = gpu() else {
        eprintln!("skipping: no Vulkan adapter");
        return;
    };
    // A full-screen quad with filter mode 5 (Modulate): `Zero / SrcColor`
    // multiplies the destination by the sampled colour, so a mid-grey texture
    // must darken the clear colour and never brighten it.
    let quad = MeshData {
        uvs2: Vec::new(),
        positions: vec![
            Vec3::new(-0.9, -0.9, 0.0),
            Vec3::new(0.9, -0.9, 0.0),
            Vec3::new(0.9, 0.9, 0.0),
            Vec3::new(-0.9, 0.9, 0.0),
        ],
        normals: vec![Vec3::Z; 4],
        uvs: vec![
            glam::Vec2::new(0.0, 0.0),
            glam::Vec2::new(1.0, 0.0),
            glam::Vec2::new(1.0, 1.0),
            glam::Vec2::new(0.0, 1.0),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        bones: vec![Vec::new(); 4],
        layers: vec![
            MeshLayer::plain(0),
            MeshLayer {
                texture: 0,
                alpha: 1.0,
                blend: 5,
                team_color: false,
                uv_set: 0,
            },
        ],
        z_up: false,
        hide: false,
    };
    let image = TextureImage::new(4, 4, vec![128u8; 4 * 4 * 4]);
    let mut mesh = MeshRenderer::new(&gpu.device, &gpu.queue, FORMAT, vec![vec![quad]], &[image], 64);
    mesh.set_skin_mode(wc3_render::mesh::SkinMode::Cpu);
    mesh.set_day_light([1.0, 1.0, 1.0, 1.0]);
    let target = target(&gpu);
    let view_proj = Mat4::from_translation(Vec3::new(0.0, 0.0, 0.5));
    let mut scene = Scene::empty(view_proj, Vec2::new(WIDTH as f32, HEIGHT as f32));
    scene.units.push(UnitDraw {
        model: 0,
        world: Mat4::IDENTITY,
        tint: [1.0; 4],
        nodes: Vec::new(),
        mesh_alpha: vec![1.0],
        layer_alpha: vec![vec![1.0, 1.0]],
        mesh_color: Vec::new(),
    });
    let mut counters = Counters::default();
    mesh.prepare(&gpu.device, &gpu.queue, &scene, PassMask::all(), &mut counters);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("modulate-frame"),
        });
    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("modulate-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.8,
                        g: 0.8,
                        b: 0.8,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth,
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
        for group in 0..3u8 {
            mesh.draw_group(&mut render_pass, group, &mut counters);
        }
    }
    gpu.queue.submit(Some(encoder.finish()));
    let pixels = read_back(&gpu.device, &gpu.queue, &target.color, WIDTH, HEIGHT)
        .expect("readback");
    let center = ((HEIGHT / 2) * WIDTH + WIDTH / 2) as usize * 4;
    let clear = (0.8f32 * 255.0) as u8;
    let sampled = pixels[center];
    assert!(
        sampled < clear,
        "modulate must darken: {sampled} vs clear {clear}"
    );
    assert!(counter_draws(&counters) >= 2, "opaque + modulate layers both draw");
}

fn counter_draws(counters: &Counters) -> u32 {
    counters.draws
}

#[test]
fn blp_authored_mip_chain_uploads_without_regeneration() {
    use wc3_formats::blp::{BlpImage, MipLevel};
    let Some(gpu) = gpu() else {
        eprintln!("skipping: no Vulkan adapter");
        return;
    };
    let size = 8u32;
    let mut mipmaps = Vec::new();
    let mut dimension = size;
    while dimension >= 1 {
        mipmaps.push(MipLevel {
            width: dimension,
            height: dimension,
            rgba: vec![255u8; (dimension * dimension * 4) as usize],
        });
        if dimension == 1 {
            break;
        }
        dimension /= 2;
    }
    let blp = BlpImage {
        width: size,
        height: size,
        rgba: vec![255u8; (size * size * 4) as usize],
        mipmaps,
    };
    let plan = blp_chain_plan(&blp, 256);
    assert_eq!(plan.base_level, 0);
    assert_eq!(plan.authored, 4);
    assert_eq!(plan.levels, 4);
    assert_eq!(plan.generated(), 0);
    let levels = blp_levels(&blp, &plan);
    assert_eq!(levels.len(), 4);

    let layout = bind_group_layout(&gpu.device);
    let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor::default());
    let store = TextureStore::upload_blp_chains(
        &gpu.device,
        &gpu.queue,
        &layout,
        &sampler,
        &[&blp],
        256,
    );
    assert_eq!(store.stats.textures, 1);
    assert_eq!(store.stats.levels, 4, "four authored levels, none generated");
    assert_eq!(store.len(), 1);
    assert!(store.bind_group(0).is_some());
}

#[test]
fn scene_probe_uses_the_camera_vectors() {
    // Guard the camera helper the emitter pass depends on: the axes must be
    // orthonormal or billboards shear.
    let camera = camera();
    assert!((camera.right.length() - 1.0).abs() < 1e-4);
    assert!((camera.up.length() - 1.0).abs() < 1e-4);
    assert!(camera.right.dot(camera.up).abs() < 1e-4);
    assert!((camera.forward.length() - 1.0).abs() < 1e-4);
}
