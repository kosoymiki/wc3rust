//! Device test for the fog pass (`rendering.fog`): the mask states must produce
//! opaque black (unexplored), darkened (explored), and untouched (visible)
//! pixels through the real pipeline and a real depth reconstruction.

use glam::Mat4;
use wc3_render::fog::{FogMask, FogRenderer};

const W: u32 = 64;
const H: u32 = 64;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    // wgpu panics inside `request_device` when the platform has no usable
    // Vulkan ICD (observed on Termux: `wgpu_core.rs:1084`), so the `.ok()?`
    // path never runs and the test fails instead of skipping. Catch the panic:
    // on a real GPU the test still runs, elsewhere it reports the skip.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        gpu_inner()
    }))
    .ok()
    .flatten()
}

fn gpu_inner() -> Option<(wgpu::Device, wgpu::Queue)> {
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
        label: Some("fog-test"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))
    .ok()?;
    Some((device, queue))
}

#[test]
#[ignore = "needs a Vulkan ICD inside the test process (Termux harness has none: wgpu panics in request_device); run via `run_layer_gates.sh --ignored` on a GPU host"]
fn rendering_fog_mask_states() {
    let Some((device, queue)) = gpu() else {
        println!("no Vulkan adapter: skipping fog device test");
        return;
    };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fog-color"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fog-depth"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wc3_render::mesh::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

    // Ortho camera covering world x/z in 0..4 so screen x maps to fog cell x
    // (the sim grid also starts at world 0). Depth clear 1.0 is the far plane.
    let view_proj = Mat4::orthographic_lh(0.0, 4.0, 0.0, 4.0, -1.0, 1.0);
    // 4x4 grid, 1 world unit cells: left half unexplored, middle explored,
    // right half visible.
    let mut cells = vec![2u8; 16];
    for y in 0..4 {
        for x in 0..4 {
            cells[y * 4 + x] = if x < 2 { 0 } else if x == 2 { 1 } else { 2 };
        }
    }
    let mask = FogMask::new(4, 4, 1.0, cells);

    let mut fog = FogRenderer::new(&device, FORMAT);
    // `depth_epoch` 0: this test never resizes the depth view, so the bind
    // group is built once (the per-frame path is `FogRenderer::update_params`).
    fog.prepare(
        &device,
        &queue,
        &mask,
        view_proj.inverse(),
        glam::Vec2::new(W as f32, H as f32),
        &depth_view,
        0,
    );

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("fog-test"),
    });
    {
        // White world target + depth clear.
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("world"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
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
    }
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fog"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        fog.draw(&mut pass);
    }
    queue.submit(Some(encoder.finish()));
    let pixels = wc3_render::read_back(&device, &queue, &color, W, H).expect("read back");

    let at = |x: u32, y: u32| -> [u8; 4] {
        let index = ((y * W + x) * 4) as usize;
        [pixels[index], pixels[index + 1], pixels[index + 2], pixels[index + 3]]
    };
    // Screen x maps to world x (0..4): left = cell 0 unexplored (black),
    // centre = cell 2 explored (45% white), right = cell 3 visible (white).
    // y = H/2 maps to world z = 2, inside the grid.
    let left = at(4, H / 2);
    let middle = at(W / 2, H / 2);
    let right = at(W - 5, H / 2);
    println!("fog pixels left={left:?} middle={middle:?} right={right:?}");
    assert!(left[0] < 12 && left[1] < 12 && left[2] < 12, "unexplored must be black: {left:?}");
    assert!(middle[0] > 80 && middle[0] < 180, "explored must be darkened: {middle:?}");
    assert!(right[0] > 235 && right[1] > 235 && right[2] > 235, "visible must be untouched: {right:?}");
}
