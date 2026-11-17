//! ASTC texture path, verified on device.
//!
//! Fixtures (generated once by `scripts/perf-astc.sh` with `astcenc 4.8.0`):
//!
//! * `fixtures/gradient.rgba` — the 64x64 RGBA8 source pattern.
//! * `fixtures/gradient8x8.astc` — the 64x64 image at 8x8 (2.0 bpp, aligned).
//! * `fixtures/gradient66.rgba` — the same pattern edge-padded to 66x66.
//! * `fixtures/gradient6x6.astc` — the padded image at 6x6 (3.56 bpp; 66 = 11*6,
//!   because WebGPU requires block-aligned compressed dimensions).
//!
//! The test samples both the uncompressed and the compressed texture through the
//! same shader and compares the frames: it proves the renderer actually decodes
//! ASTC on this GPU (feature, format, block layout, sampler) instead of merely
//! accepting the upload.

use wc3_render::texture::{Role, CompressionSupport, CompressedFormat, upload_compressed};
use wc3_render::{astc, TextureImage};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;

const SOURCE: &[u8] = include_bytes!("fixtures/gradient.rgba");
const SOURCE_66: &[u8] = include_bytes!("fixtures/gradient66.rgba");
const ASTC_6X6: &[u8] = include_bytes!("fixtures/gradient6x6.astc");
const ASTC_8X8: &[u8] = include_bytes!("fixtures/gradient8x8.astc");

const SHADER: &str = r#"
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    let p = positions[index];
    var out: VsOut;
    out.clip = vec4<f32>(p, 0.0, 1.0);
    // Same orientation as the source buffer (row 0 at v=0).
    out.uv = vec2<f32>((p.x + 1.0) * 0.5, (p.y + 1.0) * 0.5);
    return out;
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, input.uv);
}
"#;

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    target: wgpu::Texture,
    view: wgpu::TextureView,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}

/// `None` when the adapter does not expose ASTC: the test then reports a skip
/// instead of failing on a device that legitimately cannot sample the format.
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
    .expect("adapter");
    let support = CompressionSupport::probe(&adapter);
    println!("ASTC probe: {}", support.summary());
    if !support.astc_ldr {
        return None;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("astc-test"),
        required_features: wgpu::Features::TEXTURE_COMPRESSION_ASTC,
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))
    .expect("device");
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("astc-target"),
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
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("astc-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("astc-shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("astc-pipeline-layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("astc-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("astc-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });
    Some(Gpu {
        device,
        queue,
        target,
        view,
        layout,
        pipeline,
        sampler,
    })
}

impl Gpu {
    fn draw(&self, bind: &wgpu::BindGroup) -> Vec<u8> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("astc") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("astc-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        wc3_render::read_back(&self.device, &self.queue, &self.target, WIDTH, HEIGHT)
            .expect("read back")
    }

    fn reference_bind(&self, width: u32, height: u32, rgba: &[u8]) -> wgpu::BindGroup {
        let image = TextureImage::new(width, height, rgba.to_vec());
        let mips = wc3_render::texture::generate_mips(image.width, image.height, &image.rgba);
        wc3_render::texture::upload_levels(
            &self.device,
            &self.queue,
            &self.layout,
            &self.sampler,
            Role::Color,
            &mips,
        )
    }

    fn compressed_bind(
        &self,
        bytes: &[u8],
        expect: CompressedFormat,
        size: (u32, u32),
    ) -> wgpu::BindGroup {
        let image = astc::decode(bytes).expect("decode astc");
        assert_eq!(image.format, expect, "fixture block size");
        assert_eq!((image.width, image.height), size);
        image.validate().expect("astc block layout");
        upload_compressed(
            &self.device,
            &self.queue,
            &self.layout,
            &self.sampler,
            &image,
            Role::Color,
        )
    }
}

fn mae(a: &[u8], b: &[u8]) -> f64 {
    let mut sum = 0u64;
    let mut count = 0u64;
    for (x, y) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        for channel in 0..3 {
            sum += (x[channel] as i32 - y[channel] as i32).unsigned_abs() as u64;
            count += 1;
        }
    }
    sum as f64 / count.max(1) as f64
}

fn max_diff(a: &[u8], b: &[u8]) -> i32 {
    let mut worst = 0i32;
    for (x, y) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        for channel in 0..3 {
            worst = worst.max((x[channel] as i32 - y[channel] as i32).abs());
        }
    }
    worst
}

#[test]
fn fixture_is_the_expected_source_size() {
    assert_eq!(SOURCE.len(), (64 * 64 * 4) as usize);
    assert_eq!(SOURCE_66.len(), (66 * 66 * 4) as usize);
    assert!(ASTC_6X6.len() > 16, "6x6 fixture present");
    assert!(ASTC_8X8.len() > 16, "8x8 fixture present");
}

#[test]
fn block_alignment_is_enforced() {
    // 64 % 6 != 0: WebGPU rejects such a texture, so validation must too.
    assert!(!CompressedFormat::Astc6x6.accepts(64, 64));
    assert!(CompressedFormat::Astc6x6.accepts(66, 66));
    assert!(CompressedFormat::Astc8x8.accepts(64, 64));
    assert!(!CompressedFormat::Astc8x8.accepts(66, 66));
}

#[test]
fn astc_textures_decode_and_sample_on_device() {
    let Some(gpu) = gpu() else {
        println!("ASTC not supported by this adapter: skipping the device check");
        return;
    };
    for (bytes, source, format, size, limit) in [
        (ASTC_6X6, SOURCE_66, CompressedFormat::Astc6x6, (66, 66), 12.0),
        (ASTC_8X8, SOURCE, CompressedFormat::Astc8x8, (64, 64), 20.0),
    ] {
        let reference = gpu.draw(&gpu.reference_bind(size.0, size.1, source));
        let bind = gpu.compressed_bind(bytes, format, size);
        let compressed = gpu.draw(&bind);
        let distance = mae(&reference, &compressed);
        let worst = max_diff(&reference, &compressed);
        println!(
            "ASTC {:?}: MAE {distance:.2}/255 max {worst} vs uncompressed reference",
            format
        );
        assert!(
            distance < limit,
            "{format:?} mean error {distance:.2} exceeds {limit} (max {worst})"
        );
        // The compressed image must not be a solid colour: the pattern survived.
        let mut min = 255u8;
        let mut max = 0u8;
        for pixel in compressed.chunks_exact(4) {
            min = min.min(pixel[0]);
            max = max.max(pixel[0]);
        }
        assert!(
            max.saturating_sub(min) > 100,
            "{format:?} decoded to a flat image ({min}..{max})"
        );
    }
}

// ---------------------------------------------------------------- mesh pass

/// ASTC through the real batched mesh pipeline: the same quad drawn once with
/// the RGBA texture and once with its ASTC 8x8 sidecar.
#[test]
fn astc_texture_in_the_mesh_pipeline() {
    use glam::{Mat4, Vec2, Vec3};
    use wc3_render::mesh::{MeshData, MeshLayer};
    use wc3_render::scene::{PassMask, Scene, UiBatch, UnitDraw};
    use wc3_render::texture::{CompressedImage, TextureSource};

    let Some(gpu) = gpu() else {
        println!("ASTC not supported by this adapter: skipping the mesh-pipeline check");
        return;
    };

    let quad = || MeshData {
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
    };
    let models = vec![vec![quad()]];
    let scene = Scene {
        // WebGPU clip space: z in [0, 1] (see tests/parity.rs).
        view_proj: Mat4::from_translation(Vec3::new(0.0, 0.0, 0.5))
            * Mat4::from_scale(Vec3::new(0.025, 0.025, 1.0)),
        // `Scene::empty` uses the world origin; the haze only needs a finite eye.
        eye: Vec3::ZERO,
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
    let mask = PassMask {
        terrain: false,
        units: true,
        ui: false,
    };

    let rgba_image = TextureImage::new(WIDTH, HEIGHT, SOURCE.to_vec());
    let mut reference = wc3_render::Renderer::new(
        &gpu.device,
        &gpu.queue,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        WIDTH,
        HEIGHT,
        models.clone(),
        std::slice::from_ref(&rgba_image),
        256,
        None,
        &TextureImage::white(),
        None,
    );
    let mut stats = wc3_render::Counters::default();
    reference.render_with(&gpu.device, &gpu.queue, &gpu.view, &scene, mask, &mut stats);
    let expected = wc3_render::read_back(&gpu.device, &gpu.queue, &gpu.target, WIDTH, HEIGHT)
        .expect("rgba readback");

    let astc_image = astc::decode(ASTC_8X8).expect("fixture");
    let compressed = TextureSource::Compressed(&astc_image);
    let mut mesh = wc3_render::MeshRenderer::new_with_sources(
        &gpu.device,
        &gpu.queue,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        models,
        std::slice::from_ref(&compressed),
        256,
    );
    let depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("astc-mesh-depth"),
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
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("astc-mesh") });
    mesh.prepare(&gpu.device, &gpu.queue, &scene, mask, &mut stats);
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("astc-mesh-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &gpu.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
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
        mesh.draw_group(&mut pass, 0, &mut stats);
    }
    gpu.queue.submit(Some(encoder.finish()));
    println!(
        "mesh path counters: draws={} indices={} vertices={} textures={}",
        stats.draws,
        stats.indices,
        stats.vertices,
        mesh.textures().len()
    );
    let actual = wc3_render::read_back(&gpu.device, &gpu.queue, &gpu.target, WIDTH, HEIGHT)
        .expect("astc readback");

    let mean = |frame: &[u8]| -> f64 {
        let mut sum = 0u64;
        for pixel in frame.chunks_exact(4) {
            sum += pixel[0] as u64 + pixel[1] as u64 + pixel[2] as u64;
        }
        sum as f64 / (frame.len() / 4 * 3) as f64
    };
    println!(
        "mesh-pipeline mean: rgba={:.1} astc={:.1} centre rgba={:?} astc={:?}",
        mean(&expected),
        mean(&actual),
        &expected[(32 * WIDTH as usize + 32) * 4..][..3],
        &actual[(32 * WIDTH as usize + 32) * 4..][..3],
    );
    let distance = mae(&expected, &actual);
    println!("ASTC 8x8 in the mesh pipeline: MAE {distance:.2}/255 vs RGBA8");
    assert!(distance > 0.0, "the ASTC frame is not the RGBA frame (sanity)");
    assert!(distance < 8.0, "mesh-pipeline ASTC MAE {distance:.2} too high");
}
