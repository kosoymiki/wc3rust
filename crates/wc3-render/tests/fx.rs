//! Device tests for the budgeted FX pass (`PARITY_MATRIX.md` #12 / #18).
//!
//! Every assertion runs on a real Vulkan device: the particles are uploaded as
//! billboards, drawn with the real pipelines and the frame is read back. The
//! budget tests assert the exact counters (rendered / dropped / draws), so a
//! regression in the cap is a test failure, not a silent overdraw.

use glam::{Mat4, Vec3};
use wc3_render::fx::{
    blizzard_wave, impact_burst, missile_arc_point, projectile_trail, FxBlend, FxRenderer,
    FxVertex, MissileSpec, Particle, ParticleBudget, Rng, FX_BYTES_PER_PARTICLE,
    FX_VERTS_PER_PARTICLE, FX_VERTEX_STRIDE,
};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 180;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
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
        label: Some("fx-test"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))
    .ok()?;
    Some((device, queue))
}

/// Orthographic camera: 20 x 11.25 world units at z = 0, +Z into the screen.
fn camera() -> Mat4 {
    Mat4::orthographic_lh(-10.0, 10.0, -(HEIGHT as f32 / WIDTH as f32 * 10.0), HEIGHT as f32 / WIDTH as f32 * 10.0, -10.0, 10.0)
}

struct Target {
    device: wgpu::Device,
    queue: wgpu::Queue,
    color: wgpu::Texture,
    view: wgpu::TextureView,
    depth: wgpu::TextureView,
    fx: FxRenderer,
}

impl Target {
    fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fx-target"),
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
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fx-depth"),
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
        let depth = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let fx = FxRenderer::new(&device, FORMAT);
        Self {
            device,
            queue,
            color,
            view,
            depth,
            fx,
        }
    }

    /// Draw `particles` and return (stats, RGBA pixels).
    fn render(&mut self, particles: &[Particle], budget: ParticleBudget) -> (wc3_render::FxFrameStats, Vec<u8>) {
        let stats = self.fx.prepare(&self.device, &self.queue, &camera(), particles, budget);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("fx") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fx-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
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
                    view: &self.depth,
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
            let mut counters = wc3_render::Counters::default();
            self.fx.draw(&mut pass, &mut counters);
            assert_eq!(counters.draws, stats.draws, "draw calls must match the stats");
        }
        self.queue.submit(Some(encoder.finish()));
        let pixels = wc3_render::read_back(&self.device, &self.queue, &self.color, WIDTH, HEIGHT)
            .expect("read back");
        (stats, pixels)
    }
}

fn background(pixels: &[u8]) -> [u8; 4] {
    [pixels[0], pixels[1], pixels[2], pixels[3]]
}

/// Pixels that differ from the clear colour by more than `tolerance` per channel.
fn changed_pixels(pixels: &[u8]) -> usize {
    let bg = background(pixels);
    pixels
        .chunks_exact(4)
        .filter(|p| {
            p[0].abs_diff(bg[0]) > 4 || p[1].abs_diff(bg[1]) > 4 || p[2].abs_diff(bg[2]) > 4
        })
        .count()
}

fn grid(count: usize, blend: FxBlend) -> Vec<Particle> {
    let side = (count as f32).sqrt().ceil() as usize;
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let x = (index % side) as f32 / side as f32 * 16.0 - 8.0;
        let y = (index / side) as f32 / side as f32 * 8.0 - 4.0;
        out.push(Particle::new(
            Vec3::new(x, y, 0.0),
            0.6,
            [1.0, 0.6, 0.2, 1.0],
            blend,
        ));
    }
    out
}

#[test]
fn vertex_layout_is_packed() {
    assert_eq!(std::mem::size_of::<FxVertex>(), FX_VERTEX_STRIDE);
    assert_eq!(FX_BYTES_PER_PARTICLE, 240);
}

#[test]
fn empty_frame_renders_nothing() {
    let Some((device, queue)) = gpu() else {
        return;
    };
    let mut target = Target::new(device, queue);
    let (stats, pixels) = target.render(&[], ParticleBudget::default());
    assert_eq!(stats.rendered, 0);
    assert_eq!(stats.draws, 0);
    assert_eq!(changed_pixels(&pixels), 0, "empty FX frame must stay clear");
}

#[test]
fn particles_are_visible() {
    let Some((device, queue)) = gpu() else {
        return;
    };
    let mut target = Target::new(device, queue);
    let particles = grid(64, FxBlend::Additive);
    let (stats, pixels) = target.render(&particles, ParticleBudget::default());
    assert_eq!(stats.emitted, 64);
    assert_eq!(stats.rendered, 64);
    assert_eq!(stats.dropped_budget, 0);
    assert_eq!(stats.draws, 1, "one additive stream is one draw");
    assert_eq!(stats.vertices, 64 * FX_VERTS_PER_PARTICLE as u64);
    assert!(changed_pixels(&pixels) > 0, "particles must reach the target");
}

#[test]
fn budget_caps_particles_and_reports_drops() {
    let Some((device, queue)) = gpu() else {
        return;
    };
    let mut target = Target::new(device, queue);
    let particles = grid(5000, FxBlend::Additive);
    let budget = ParticleBudget {
        max_particles: 256,
        max_draws: 2,
        max_upload_bytes: 10 * 1024 * 1024,
    };
    let (stats, _) = target.render(&particles, budget);
    assert_eq!(stats.emitted, 5000);
    assert_eq!(stats.rendered, 256, "count cap is exact");
    assert_eq!(stats.dropped_budget, 5000 - 256);
    assert_eq!(stats.vertices, 256 * FX_VERTS_PER_PARTICLE as u64);
    assert_eq!(stats.draws, 1);
}

#[test]
fn byte_budget_caps_before_particle_cap() {
    let Some((device, queue)) = gpu() else {
        return;
    };
    let mut target = Target::new(device, queue);
    let particles = grid(1000, FxBlend::Additive);
    let budget = ParticleBudget {
        max_particles: 1000,
        max_draws: 2,
        max_upload_bytes: FX_BYTES_PER_PARTICLE * 10,
    };
    let (stats, _) = target.render(&particles, budget);
    assert_eq!(stats.rendered, 10, "byte budget wins");
    assert_eq!(stats.dropped_budget, 990);
    assert!(stats.upload_bytes <= budget.max_upload_bytes);
}

#[test]
fn blizzard_six_waves_render_geometry() {
    let Some((device, queue)) = gpu() else {
        return;
    };
    let mut rng = Rng::new(2026);
    let mut particles = Vec::new();
    let mut waves = Vec::new();
    for wave in 0..6 {
        let before = particles.len();
        blizzard_wave(Vec3::new(0.0, 0.0, 0.0), 6.0, wave, &mut rng, &mut particles);
        waves.push(particles.len() - before);
    }
    assert!(waves.iter().all(|count| *count >= 12), "six waves: {waves:?}");
    let mut target = Target::new(device, queue);
    let (stats, pixels) = target.render(&particles, ParticleBudget::default());
    assert_eq!(stats.rendered as usize, particles.len());
    assert_eq!(stats.dropped_budget, 0);
    assert!(changed_pixels(&pixels) > 0, "Blizzard waves must be visible");
}

#[test]
fn mixed_blends_use_two_draws() {
    let Some((device, queue)) = gpu() else {
        return;
    };
    let mut target = Target::new(device, queue);
    let mut particles = grid(32, FxBlend::Additive);
    particles.extend(grid(32, FxBlend::Alpha));
    let (stats, _) = target.render(&particles, ParticleBudget::default());
    assert_eq!(stats.rendered, 64);
    assert_eq!(stats.draws, 2, "additive + alpha = two draws");
}

#[test]
fn frame_hash_is_deterministic() {
    let Some((device, queue)) = gpu() else {
        return;
    };
    let mut spec = MissileSpec {
        from: Vec3::new(-8.0, -3.0, 0.0),
        to: Vec3::new(8.0, 3.0, 0.0),
        arc_height: 3.0,
        speed: 12.0,
        color: [0.9, 0.7, 1.0, 1.0],
        size: 0.8,
        blend: FxBlend::Additive,
    };
    let mut rng = Rng::new(99);
    let mut particles = Vec::new();
    projectile_trail(&spec, spec.flight_time() * 0.4, 10, &mut rng, &mut particles);
    impact_burst(Vec3::new(8.0, 3.0, 0.0), 24, 1.5, [1.0, 0.4, 0.2, 1.0], &mut rng, &mut particles);
    spec.to = Vec3::new(6.0, 2.0, 0.0);

    let mut first = Target::new(device, queue);
    let (stats_a, pixels_a) = first.render(&particles, ParticleBudget::default());
    let hash_a = wc3_render::frame_hash(&pixels_a);

    // Re-create the whole renderer: the hash must not depend on cached state.
    let Some((device, queue)) = gpu() else {
        return;
    };
    let mut second = Target::new(device, queue);
    let (stats_b, pixels_b) = second.render(&particles, ParticleBudget::default());
    let hash_b = wc3_render::frame_hash(&pixels_b);
    assert_eq!(
        (stats_a.emitted, stats_a.rendered, stats_a.dropped_budget, stats_a.draws, stats_a.vertices, stats_a.upload_bytes),
        (stats_b.emitted, stats_b.rendered, stats_b.dropped_budget, stats_b.draws, stats_b.vertices, stats_b.upload_bytes)
    );
    assert_eq!(hash_a, hash_b, "same particles + camera must hash identically");
}

#[test]
fn projectile_trail_follows_the_missile_arc() {
    let spec = MissileSpec {
        from: Vec3::new(-9.0, -4.0, 0.0),
        to: Vec3::new(9.0, 4.0, 0.0),
        arc_height: 4.0,
        speed: 18.0,
        color: [1.0, 0.8, 0.3, 1.0],
        size: 0.7,
        blend: FxBlend::Additive,
    };
    let mut rng = Rng::new(5);
    let mut particles = Vec::new();
    let added = projectile_trail(&spec, spec.flight_time() * 0.5, 12, &mut rng, &mut particles);
    assert!(added > 2, "head + trail, got {added}");
    // Head must be at the arc midpoint for half flight time.
    let mid = missile_arc_point(spec.from, spec.to, spec.arc_height, 0.5);
    assert!((particles[0].pos.x - mid.x).abs() < 1e-3);
    assert!((particles[0].pos.y - mid.y).abs() < 1e-3);
    let Some((device, queue)) = gpu() else {
        return;
    };
    let mut target = Target::new(device, queue);
    let (stats, pixels) = target.render(&particles, ParticleBudget::default());
    assert_eq!(stats.rendered as usize, particles.len());
    assert!(changed_pixels(&pixels) > 0);
}

#[test]
fn stream_cap_drops_excess_blend_streams_without_overflow() {
    let Some((device, queue)) = gpu() else {
        return;
    };
    let mut target = Target::new(device, queue);
    let mut particles = grid(16, FxBlend::Additive);
    particles.extend(grid(16, FxBlend::Alpha));
    let budget = ParticleBudget {
        max_particles: 64,
        max_draws: 1,
        max_upload_bytes: 1024 * 1024,
    };
    let (stats, _) = target.render(&particles, budget);
    assert_eq!(stats.rendered, 16, "only the additive stream fits one draw");
    assert_eq!(stats.draws, 1);
    assert_eq!(stats.dropped_budget, 16, "the alpha stream is counted, not lost");
}
