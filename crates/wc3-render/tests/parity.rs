//! Merge parity fixtures (run after the base freeze).
//!
//! These tests need a GPU, so they are the first thing the GPU layer runs once
//! `BASE.md` freezes. They prove the properties the merge depends on:
//!
//! 1. GPU skinning and the CPU reference path produce the same frame.
//! 2. The static-batch path (terrain/doodads) and the unit path agree when fed
//!    the same triangles.
//! 3. CPU draw accounting equals the renderer's real draw counter.
//!
//! They use synthetic geometry on purpose: a fixture must not depend on the
//! operator's install, and `wc3-render` never invents base data — the base's own
//! values arrive through `wc3-formats`/`wc3-re` at runtime.

use glam::{Mat4, Vec2, Vec3};
use wc3_render::mesh::{BoneWeight, MeshData, MeshLayer, MeshVertex, SkinMode, StaticDrawSpec};
use wc3_render::parity::{account, frame_hash, mae, ssim};
use wc3_render::scene::{PassMask, Ring, Scene, UiBatch, UiDraw, UiVertex, UnitDraw};
use wc3_render::texture::TextureImage;
use wc3_render::{Counters, Renderer};

const WIDTH: u32 = 192;
const HEIGHT: u32 = 192;

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    target: wgpu::Texture,
    view: wgpu::TextureView,
}

fn gpu() -> Gpu {
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
    .expect("adapter");
    let features = wgpu::Features::empty();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("parity-device"),
        required_features: features,
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))
    .expect("device");
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("parity-target"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    Gpu {
        device,
        queue,
        target,
        view,
    }
}

/// Two triangles forming a quad, plus a skinned quad driven by two bones.
fn fixture_models() -> Vec<wc3_render::Model> {
    let quad = |z: f32| MeshData {
        positions: vec![
            Vec3::new(-40.0, -40.0, z),
            Vec3::new(40.0, -40.0, z),
            Vec3::new(40.0, 40.0, z),
            Vec3::new(-40.0, 40.0, z),
        ],
        normals: vec![Vec3::Z; 4],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ],
        uvs2: Vec::new(),
        indices: vec![0, 1, 2, 0, 2, 3],
        bones: vec![Vec::new(); 4],
        layers: vec![MeshLayer::plain(0)],
        z_up: false,
        hide: false,
    };
    let mut skinned = quad(10.0);
    // Weighted skinning: half bone 0, half bone 1.
    skinned.bones = vec![
        vec![BoneWeight { bone: 0, weight: 1.0 }],
        vec![BoneWeight { bone: 1, weight: 1.0 }],
        vec![
            BoneWeight { bone: 0, weight: 0.5 },
            BoneWeight { bone: 1, weight: 0.5 },
        ],
        vec![BoneWeight { bone: 0, weight: 0.25 }],
    ];
    skinned.z_up = true;
    vec![vec![quad(0.0), skinned]]
}

fn fixture_textures() -> Vec<TextureImage> {
    // 4x4 checker, opaque white / mid grey.
    let mut rgba = Vec::with_capacity(4 * 4 * 4);
    for y in 0..4 {
        for x in 0..4 {
            let value = if (x + y) % 2 == 0 { 255u8 } else { 128 };
            rgba.extend_from_slice(&[value, value, value, 255]);
        }
    }
    let checker = TextureImage::new(4, 4, rgba);
    let white = TextureImage::white();
    vec![checker, white]
}

fn fixture_ui() -> UiBatch {
    let quad = |x0: f32, y0: f32, x1: f32, y1: f32| {
        vec![
            UiVertex { pos: [x0, y0], uv: [0.0, 0.0], color: [1.0; 4] },
            UiVertex { pos: [x1, y0], uv: [1.0, 0.0], color: [1.0; 4] },
            UiVertex { pos: [x1, y1], uv: [1.0, 1.0], color: [1.0; 4] },
            UiVertex { pos: [x0, y0], uv: [0.0, 0.0], color: [1.0; 4] },
            UiVertex { pos: [x1, y1], uv: [1.0, 1.0], color: [1.0; 4] },
            UiVertex { pos: [x0, y1], uv: [0.0, 1.0], color: [1.0; 4] },
        ]
    };
    UiBatch {
        white: quad(8.0, 8.0, 60.0, 40.0),
        font: Vec::new(),
        minimap: Vec::new(),
        textured: vec![UiDraw {
            texture: 0,
            vertices: quad(100.0, 8.0, 180.0, 60.0),
        }],
    }
}

fn fixture_scene() -> Scene {
    Scene {
        // WebGPU clip space: z in [0, 1], so push the scene to z = 0.5 (in front
        // of the near plane) and scale the 80-unit quad to the full viewport.
        view_proj: Mat4::from_translation(Vec3::new(0.0, 0.0, 0.5))
            * Mat4::from_scale(Vec3::new(0.025, 0.025, 1.0)),
        viewport: Vec2::new(WIDTH as f32, HEIGHT as f32),
        units: vec![UnitDraw {
            model: 0,
            world: Mat4::from_rotation_z(0.25),
            tint: [0.6, 0.8, 1.0, 1.0],
            nodes: vec![
                Mat4::from_translation(Vec3::new(0.0, 0.0, 0.0)),
                Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)),
            ],
            mesh_alpha: vec![1.0, 1.0],
            layer_alpha: vec![vec![1.0], vec![1.0]],
            mesh_color: Vec::new(),
        }],
        rings: vec![Ring {
            pos: Vec3::new(0.0, 0.0, 0.0),
            radius: 30.0,
            color: [0.3, 1.0, 0.4, 0.8],
        }],
        ui: fixture_ui(),
    }
}

fn font_atlas() -> TextureImage {
    TextureImage::new(2, 2, vec![255; 16])
}

#[test]
fn gpu_and_cpu_skinning_render_the_same_frame() {
    let gpu = gpu();
    let models = fixture_models();
    let textures = fixture_textures();
    let scene = fixture_scene();
    let atlas = font_atlas();
    let mut renderer = Renderer::new(
        &gpu.device,
        &gpu.queue,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        WIDTH,
        HEIGHT,
        models.clone(),
        &textures,
        256,
        None,
        &atlas,
        None,
    );

    renderer.mesh.set_skin_mode(SkinMode::Gpu);
    let mut gpu_stats = Counters::default();
    renderer.render(&gpu.device, &gpu.queue, &gpu.view, &scene, &mut gpu_stats);
    let gpu_pixels = wc3_render::read_back(&gpu.device, &gpu.queue, &gpu.target, WIDTH, HEIGHT)
        .expect("gpu readback");

    renderer.mesh.set_skin_mode(SkinMode::Cpu);
    let mut cpu_stats = Counters::default();
    renderer.render(&gpu.device, &gpu.queue, &gpu.view, &scene, &mut cpu_stats);
    let cpu_pixels = wc3_render::read_back(&gpu.device, &gpu.queue, &gpu.target, WIDTH, HEIGHT)
        .expect("cpu readback");

    assert_eq!(gpu_stats.draws, cpu_stats.draws, "same draw count");
    assert!(
        gpu_pixels.iter().any(|byte| *byte != 0),
        "frame is not empty"
    );
    let distance = mae(&gpu_pixels, &cpu_pixels);
    let similarity = ssim(&gpu_pixels, &cpu_pixels, WIDTH, HEIGHT);
    assert!(
        distance < 1.0,
        "gpu vs cpu skinning MAE {distance:.3} (hash {:x} vs {:x})",
        frame_hash(&gpu_pixels),
        frame_hash(&cpu_pixels)
    );
    assert!(similarity > 0.99, "gpu vs cpu SSIM {similarity:.5}");
}

#[test]
fn draw_accounting_matches_the_renderer() {
    let gpu = gpu();
    let models = fixture_models();
    let textures = fixture_textures();
    let scene = fixture_scene();
    let atlas = font_atlas();
    let mut renderer = Renderer::new(
        &gpu.device,
        &gpu.queue,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        WIDTH,
        HEIGHT,
        models.clone(),
        &textures,
        256,
        None,
        &atlas,
        None,
    );
    let budget = account(&scene, &models);
    // World pass only: the accounting function models unit/ring draws, so the
    // HUD pass is excluded here and checked separately below.
    let mut world = Counters::default();
    renderer.render_with(
        &gpu.device,
        &gpu.queue,
        &gpu.view,
        &scene,
        PassMask {
            terrain: false,
            units: true,
            ui: false,
        },
        &mut world,
    );
    assert_eq!(
        world.draws as usize, budget.draws,
        "renderer draws {} vs accounting {} ({})",
        world.draws,
        budget.draws,
        budget.to_json()
    );

    // The HUD batch is one textured run plus the flat-white run in this fixture.
    let mut full = Counters::default();
    renderer.render_with(
        &gpu.device,
        &gpu.queue,
        &gpu.view,
        &scene,
        PassMask {
            terrain: false,
            ..PassMask::all()
        },
        &mut full,
    );
    let ui_draws = (full.draws - world.draws) as usize;
    assert_eq!(
        ui_draws,
        scene.ui.draw_count(),
        "HUD draws {} vs expected {}",
        ui_draws,
        scene.ui.draw_count()
    );
}

#[test]
fn static_batch_matches_the_unit_path() {
    let gpu = gpu();
    // A model with exactly one static geoset, so the unit path emits the same
    // triangles the static batch will.
    let models = vec![vec![fixture_static_quad()]];
    let textures = fixture_textures();
    let atlas = font_atlas();
    let mut renderer = Renderer::new(
        &gpu.device,
        &gpu.queue,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        WIDTH,
        HEIGHT,
        models.clone(),
        &textures,
        256,
        None,
        &atlas,
        None,
    );

    let scene = Scene {
        view_proj: fixture_scene().view_proj,
        viewport: Vec2::new(WIDTH as f32, HEIGHT as f32),
        units: vec![UnitDraw {
            model: 0,
            world: Mat4::IDENTITY,
            tint: [1.0; 4],
            nodes: Vec::new(),
            mesh_alpha: vec![1.0],
            layer_alpha: vec![vec![1.0]],
            mesh_color: Vec::new(),
        }],
        rings: Vec::new(),
        ui: UiBatch::default(),
    };
    let mut unit_stats = Counters::default();
    renderer.render_with(
        &gpu.device,
        &gpu.queue,
        &gpu.view,
        &scene,
        PassMask {
            terrain: false,
            units: true,
            ui: false,
        },
        &mut unit_stats,
    );
    let unit_pixels = wc3_render::read_back(&gpu.device, &gpu.queue, &gpu.target, WIDTH, HEIGHT)
        .expect("unit readback");

    // The same geometry as a static batch, drawn the way the game draws merged
    // doodads: its own pass, its own depth target, the renderer's pipelines.
    let mesh = &models[0][0];
    let mut vertices = Vec::with_capacity(mesh.positions.len());
    for (index, position) in mesh.positions.iter().enumerate() {
        vertices.push(MeshVertex::colored(
            position.to_array(),
            mesh.normals[index],
            mesh.uvs[index].to_array(),
            [1.0, 1.0, 1.0, 1.0],
        ));
    }
    let spec = StaticDrawSpec::opaque(0, mesh.indices.len() as u32, 0, 0, 0);
    let batch = renderer
        .mesh
        .create_static_batch(&gpu.device, &gpu.queue, &vertices, &mesh.indices, &[spec]);

    let depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("parity-depth"),
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
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("static") });
    let mut static_stats = Counters::default();
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("static-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &gpu.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.045,
                        g: 0.075,
                        b: 0.11,
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
        renderer
            .mesh
            .draw_static(&mut pass, &batch, 0, &mut static_stats);
    }
    gpu.queue.submit(Some(encoder.finish()));
    let static_pixels = wc3_render::read_back(&gpu.device, &gpu.queue, &gpu.target, WIDTH, HEIGHT)
        .expect("static readback");

    assert_eq!(static_stats.draws, 1, "one static draw");
    assert_eq!(
        static_stats.draws,
        unit_stats.draws - 0,
        "unit path draws the same single geoset"
    );
    assert_eq!(
        frame_hash(&unit_pixels),
        frame_hash(&static_pixels),
        "static batch and unit path must produce the same frame (MAE {:.4})",
        mae(&unit_pixels, &static_pixels)
    );
}

/// One static quad in the same shape the skinned fixture uses.
fn fixture_static_quad() -> MeshData {
    MeshData {
        positions: vec![
            Vec3::new(-40.0, -40.0, 0.0),
            Vec3::new(40.0, -40.0, 0.0),
            Vec3::new(40.0, 40.0, 0.0),
            Vec3::new(-40.0, 40.0, 0.0),
        ],
        normals: vec![Vec3::Z; 4],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ],
        uvs2: Vec::new(),
        indices: vec![0, 1, 2, 0, 2, 3],
        bones: vec![Vec::new(); 4],
        layers: vec![MeshLayer::plain(0)],
        z_up: false,
        hide: false,
    }
}
