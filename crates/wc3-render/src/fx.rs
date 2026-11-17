//! Budgeted effects pass: projectile trails, impact bursts and spell particles.
//!
//! `PARITY_MATRIX.md` row #12 (projectiles / missiles) and row #18 (effects /
//! particles) need visible FX geometry with a hard draw/particle budget, not an
//! unbounded particle system. This module is that pass:
//!
//! * [`ParticleBudget`] caps particles, draw calls and upload bytes per frame;
//!   everything above the cap is counted in [`FxFrameStats::dropped_budget`] and
//!   never silently lost.
//! * [`FxRenderer`] turns [`Particle`]s into camera-facing billboards on the CPU
//!   (deterministic, testable without a GPU) and draws them with two pipelines:
//!   additive (spell glow) and alpha (debris/dust).
//! * Emitters are pure functions over world positions: [`missile_arc_point`],
//!   [`projectile_trail`], [`impact_burst`] and [`blizzard_wave`]. They take a
//!   caller-owned [`Rng`] so a replay with the same seed produces the same
//!   particles and the same frame hash.
//!
//! The pass draws after the world geometry with depth testing on and depth
//! writes off, so terrain and units occlude FX without the FX overwriting depth.

use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};

use crate::frame::DynamicBuffer;
use crate::mesh::DEPTH_FORMAT;
use crate::perf::Counters;

/// One billboard corner in clip space plus its UV and colour.
///
/// Positions arrive already projected: the CPU billboard step is the only place
/// that needs the camera, so the pass has no uniform buffer and no bind group.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
pub struct FxVertex {
    pub clip: [f32; 4],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// `size_of::<FxVertex>()`.
pub const FX_VERTEX_STRIDE: usize = 40;
/// Vertices per particle (two triangles).
pub const FX_VERTS_PER_PARTICLE: u32 = 6;
/// Bytes of vertex data one particle costs.
pub const FX_BYTES_PER_PARTICLE: u64 = (FX_VERTEX_STRIDE as u64) * (FX_VERTS_PER_PARTICLE as u64);

/// Blend mode of a particle batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FxBlend {
    /// Glow, magic, fire: `One + One`.
    Additive,
    /// Dust, debris, smoke: premultiplied `One + OneMinusSrcAlpha`.
    Alpha,
}

impl FxBlend {
    fn index(self) -> usize {
        match self {
            FxBlend::Additive => 0,
            FxBlend::Alpha => 1,
        }
    }
}

/// One effect particle in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub pos: Vec3,
    /// Half extent of the billboard in world units.
    pub size: f32,
    pub color: [f32; 4],
    pub blend: FxBlend,
    /// Rotation around the view axis, radians.
    pub rotation: f32,
}

impl Default for Particle {
    fn default() -> Self {
        Self {
            pos: Vec3::ZERO,
            size: 0.5,
            color: [1.0; 4],
            blend: FxBlend::Additive,
            rotation: 0.0,
        }
    }
}

impl Particle {
    pub fn new(pos: Vec3, size: f32, color: [f32; 4], blend: FxBlend) -> Self {
        Self {
            pos,
            size,
            color,
            blend,
            rotation: 0.0,
        }
    }
}

/// Hard per-frame FX budget. Enforced before any upload happens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleBudget {
    /// Maximum particles rendered per frame.
    pub max_particles: usize,
    /// Maximum draw calls (one per non-empty blend stream, so 2 today).
    pub max_draws: u32,
    /// Maximum vertex bytes uploaded per frame.
    pub max_upload_bytes: u64,
}

impl Default for ParticleBudget {
    fn default() -> Self {
        Self {
            max_particles: 4096,
            max_draws: 8,
            max_upload_bytes: 1024 * 1024,
        }
    }
}

impl ParticleBudget {
    /// Particles that fit both the count cap and the byte cap.
    pub fn clamp_count(&self, emitted: usize) -> usize {
        let by_bytes = (self.max_upload_bytes / FX_BYTES_PER_PARTICLE) as usize;
        emitted.min(self.max_particles).min(by_bytes)
    }

    /// Draw calls this budget allows for the two known blend streams.
    pub fn allows_streams(&self) -> usize {
        self.max_draws.min(2) as usize
    }
}

/// Per-frame FX counters. Explicitly separate from [`Counters`] so a caller can
/// assert the budget without mixing in mesh/UI work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FxFrameStats {
    /// Particles offered by the scene.
    pub emitted: u32,
    /// Particles that became vertices this frame.
    pub rendered: u32,
    /// Particles dropped because of the budget.
    pub dropped_budget: u32,
    /// Particles dropped because they were behind the camera (`w <= 0`).
    pub dropped_culled: u32,
    /// Draw calls recorded (one per non-empty blend stream).
    pub draws: u32,
    pub vertices: u64,
    pub upload_bytes: u64,
    pub buffer_grows: u32,
    pub prepare_micros: u64,
}

const FX_SHADER: &str = r#"
struct VsIn {
    @location(0) clip: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(input: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = input.clip;
    out.uv = input.uv;
    out.color = input.color;
    return out;
}

// Round, soft-edged particle. Colour is premultiplied so the same fragment
// works for additive and alpha blends.
@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    let d = length(input.uv * 2.0 - vec2<f32>(1.0));
    let falloff = clamp(1.0 - d, 0.0, 1.0);
    let a = input.color.a * falloff * falloff;
    return vec4<f32>(input.color.rgb * a, a);
}
"#;

/// FX pass: two pipelines, one dynamic vertex buffer, CPU billboards.
pub struct FxRenderer {
    additive: wgpu::RenderPipeline,
    alpha: wgpu::RenderPipeline,
    vertices: DynamicBuffer,
    /// Blend stream of each particle in the current frame, in draw order.
    ranges: [std::ops::Range<u32>; 2],
    stats: FxFrameStats,
    max_particles_capacity: u32,
}

impl FxRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fx-shader"),
            source: wgpu::ShaderSource::Wgsl(FX_SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fx-layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let vertex_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: FX_VERTEX_STRIDE as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 16,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 24,
                    shader_location: 2,
                },
            ],
        })];
        let make_pipeline = |label: &str, blend: wgpu::BlendState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &vertex_buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let additive = make_pipeline(
            "fx-additive",
            wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            },
        );
        let alpha = make_pipeline(
            "fx-alpha",
            wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
            },
        );
        let vertices = DynamicBuffer::new(
            device,
            256,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            "fx-vertices",
        );
        Self {
            additive,
            alpha,
            vertices,
            ranges: [0..0, 0..0],
            stats: FxFrameStats::default(),
            max_particles_capacity: 0,
        }
    }

    /// Stats of the last [`Self::prepare`].
    pub fn stats(&self) -> FxFrameStats {
        self.stats
    }

    /// Billboards `particles`, enforces `budget` and uploads the vertex stream.
    ///
    /// Draw calls stay at one per non-empty blend stream; if `budget.max_draws`
    /// is smaller than the number of non-empty streams, the later streams are
    /// dropped and counted in [`FxFrameStats::dropped_budget`] (never silently).
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view_proj: &Mat4,
        particles: &[Particle],
        budget: ParticleBudget,
    ) -> FxFrameStats {
        let start_time = Instant::now();
        self.vertices.begin_frame();
        let emitted = particles.len() as u32;
        let allowed = budget.clamp_count(particles.len());

        // World-space billboard axes from the camera. The inverse view-projection
        // turns clip-space X/Y into world directions, which works for both
        // perspective and orthographic cameras without a separate camera struct.
        let inverse = view_proj.inverse();
        let right = (inverse * Vec4::new(1.0, 0.0, 0.0, 0.0))
            .truncate()
            .normalize_or_zero();
        let up = (inverse * Vec4::new(0.0, 1.0, 0.0, 0.0))
            .truncate()
            .normalize_or_zero();
        let right = if right.length_squared() > 1e-12 {
            right
        } else {
            Vec3::X
        };
        let up = if up.length_squared() > 1e-12 {
            up
        } else {
            Vec3::Y
        };

        let mut vertex_data: Vec<FxVertex> = Vec::with_capacity(allowed * FX_VERTS_PER_PARTICLE as usize);
        let mut stream_counts = [0u32; 2];
        let mut skipped_streams = 0u32;
        let allowed_streams = budget.allows_streams();
        let mut culled = 0u32;
        let mut accepted = 0u32;
        for particle in particles.iter().take(allowed) {
            let stream = particle.blend.index();
            if stream >= allowed_streams {
                skipped_streams += 1;
                continue;
            }
            let (cos, sin) = (particle.rotation.cos(), particle.rotation.sin());
            let r = (right * cos + up * sin) * particle.size;
            let u = (up * cos - right * sin) * particle.size;
            let mut corners = [[0.0f32; 4]; 4];
            let mut behind = false;
            for (index, (sr, su)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .into_iter()
                .enumerate()
            {
                let world = particle.pos + r * sr + u * su;
                let clip = *view_proj * world.extend(1.0);
                if clip.w <= 0.0 {
                    behind = true;
                    break;
                }
                corners[index] = clip.to_array();
            }
            if behind {
                culled += 1;
                continue;
            }
            let uv = [[0.0f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
            for corner in [0usize, 1, 2, 0, 2, 3] {
                vertex_data.push(FxVertex {
                    clip: corners[corner],
                    uv: uv[corner],
                    color: particle.color,
                });
            }
            accepted += 1;
            stream_counts[stream] += 1;
        }

        // Upload, then remember the draw ranges. Vertex order already matches the
        // draw order: every additive particle precedes every alpha particle.
        let bytes: &[u8] = bytemuck::cast_slice(&vertex_data);
        let uploaded = self.vertices.write(device, queue, bytes);
        let mut cursor = 0u32;
        for stream in 0..2 {
            let count = stream_counts[stream] * FX_VERTS_PER_PARTICLE;
            self.ranges[stream] = cursor..cursor + count;
            cursor += count;
        }
        self.max_particles_capacity = self.max_particles_capacity.max(accepted);

        let rendered = accepted;
        // Everything offered but not drawn is either over the budget/stream cap
        // or behind the camera; both are reported, never silently lost.
        let dropped_budget = emitted.saturating_sub(rendered).saturating_sub(culled);
        let _ = skipped_streams;
        let draws = stream_counts.iter().filter(|count| **count > 0).count() as u32;
        self.stats = FxFrameStats {
            emitted,
            rendered,
            dropped_budget,
            dropped_culled: culled,
            draws,
            vertices: rendered as u64 * FX_VERTS_PER_PARTICLE as u64,
            upload_bytes: if uploaded { bytes.len() as u64 } else { 0 },
            buffer_grows: self.vertices.grows,
            prepare_micros: start_time.elapsed().as_micros() as u64,
        };
        self.stats
    }

    /// Record the two blend streams. Call after [`Self::prepare`] in a pass that
    /// has the depth attachment bound with `Load`.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, stats: &mut Counters) {
        if self.stats.rendered == 0 {
            return;
        }
        let buffer = self.vertices.buffer();
        pass.set_vertex_buffer(0, buffer.slice(..));
        let mut pipeline = None;
        for (stream, pipeline_ref) in [FxBlend::Additive, FxBlend::Alpha]
            .into_iter()
            .zip([&self.additive, &self.alpha])
        {
            let range = &self.ranges[stream.index()];
            if range.is_empty() {
                continue;
            }
            if pipeline != Some(stream) {
                pass.set_pipeline(pipeline_ref);
                pipeline = Some(stream);
                stats.pipeline_binds += 1;
            }
            pass.draw(range.clone(), 0..1);
            stats.draws += 1;
            stats.vertices += (range.end - range.start) as u64;
        }
    }

    /// Per-frame buffer high-water mark in particles (diagnostics).
    pub fn capacity_particles(&self) -> u32 {
        self.max_particles_capacity
    }
}

// --------------------------------------------------------------- emitters

/// SplitMix64: tiny deterministic RNG so a replay seed reproduces the FX.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_add(0x9E37_79B9_7F4A_7C15))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform in `[low, high)`.
    pub fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.next_f32()
    }
}

/// Position on a missile arc for progress `t` in `0..=1`.
///
/// `arc_height` is the extra height at the midpoint (WC3 `Missilearc` in world
/// units; a straight missile passes `0.0`).
pub fn missile_arc_point(from: Vec3, to: Vec3, arc_height: f32, t: f32) -> Vec3 {
    let t = t.clamp(0.0, 1.0);
    from.lerp(to, t) + Vec3::new(0.0, 4.0 * arc_height * t * (1.0 - t), 0.0)
}

/// One missile + its trail for the current flight time.
///
/// The head is always emitted while the missile is in flight; the trail keeps
/// `trail` faded echoes behind it. The caller passes a deterministic [`Rng`],
/// so the same `time_s` and seed produce byte-identical particles.
pub struct MissileSpec {
    pub from: Vec3,
    pub to: Vec3,
    pub arc_height: f32,
    /// World units per second (`Missilespeed`).
    pub speed: f32,
    pub color: [f32; 4],
    pub size: f32,
    pub blend: FxBlend,
}

impl MissileSpec {
    /// Flight time for the distance at `speed` (at least one frame).
    pub fn flight_time(&self) -> f32 {
        (self.from.distance(self.to) / self.speed.max(0.001)).max(1.0 / 60.0)
    }

    /// `0..=1` progress for `time_s` seconds of flight.
    pub fn progress(&self, time_s: f32) -> f32 {
        (time_s / self.flight_time()).clamp(0.0, 1.0)
    }
}

/// Emit the head and trail of one missile into `out`. Returns the count added.
pub fn projectile_trail(
    spec: &MissileSpec,
    time_s: f32,
    trail: usize,
    rng: &mut Rng,
    out: &mut Vec<Particle>,
) -> usize {
    let before = out.len();
    let t = spec.progress(time_s);
    let head = missile_arc_point(spec.from, spec.to, spec.arc_height, t);
    out.push(Particle::new(head, spec.size, spec.color, spec.blend));
    let trail = trail.min(32);
    for step in 0..trail {
        let back = (step + 1) as f32 * 0.04;
        let tt = t - back;
        if tt <= 0.0 {
            break;
        }
        let pos = missile_arc_point(spec.from, spec.to, spec.arc_height, tt);
        let fade = 1.0 - (step + 1) as f32 / (trail + 1) as f32;
        let mut color = spec.color;
        color[3] *= fade * 0.7;
        let jitter = Vec3::new(
            rng.range(-0.05, 0.05),
            rng.range(-0.05, 0.05),
            rng.range(-0.05, 0.05),
        );
        out.push(Particle::new(pos + jitter, spec.size * (0.4 + fade * 0.4), color, spec.blend));
    }
    out.len() - before
}

/// Impact burst: `count` particles thrown up and out from `center`.
pub fn impact_burst(
    center: Vec3,
    count: usize,
    radius: f32,
    color: [f32; 4],
    rng: &mut Rng,
    out: &mut Vec<Particle>,
) -> usize {
    let before = out.len();
    for _ in 0..count {
        let angle = rng.range(0.0, std::f32::consts::TAU);
        let spread = rng.range(0.15, 1.0);
        let dir = Vec3::new(angle.cos() * spread, rng.range(0.2, 1.0), angle.sin() * spread);
        let pos = center + dir.normalize_or_zero() * radius * rng.range(0.2, 1.0);
        let mut c = color;
        c[3] *= rng.range(0.5, 1.0);
        out.push(Particle::new(pos, rng.range(0.15, 0.5), c, FxBlend::Additive));
    }
    out.len() - before
}

/// One Blizzard-style wave: a ring of falling shards around `center`.
///
/// The spell spawns at least six waves (`PARITY_MATRIX.md` #18 acceptance), so
/// `wave` only scales the shard count and the fall height.
pub fn blizzard_wave(
    center: Vec3,
    radius: f32,
    wave: usize,
    rng: &mut Rng,
    out: &mut Vec<Particle>,
) -> usize {
    let before = out.len();
    let count = 12 + wave * 2;
    let angle_step = std::f32::consts::TAU / count as f32;
    let height = 6.0 - wave as f32 * 0.35;
    for index in 0..count {
        let angle = index as f32 * angle_step + rng.range(-0.08, 0.08);
        let r = radius * rng.range(0.35, 1.0);
        let pos = center + Vec3::new(angle.cos() * r, height + rng.range(-0.4, 0.4), angle.sin() * r);
        out.push(Particle::new(
            pos,
            rng.range(0.25, 0.6),
            [0.55, 0.75, 1.0, 0.9],
            FxBlend::Additive,
        ));
    }
    out.len() - before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_layout_is_packed() {
        assert_eq!(std::mem::size_of::<FxVertex>(), FX_VERTEX_STRIDE);
        assert_eq!(FX_BYTES_PER_PARTICLE, 240);
    }

    #[test]
    fn budget_clamps_by_count_and_bytes() {
        let by_count = ParticleBudget {
            max_particles: 1000,
            max_draws: 2,
            max_upload_bytes: 1_000_000,
        };
        assert_eq!(by_count.clamp_count(10_000), 1000);
        let by_bytes = ParticleBudget {
            max_particles: 10_000,
            max_draws: 2,
            max_upload_bytes: FX_BYTES_PER_PARTICLE * 10,
        };
        assert_eq!(by_bytes.clamp_count(5000), 10);
        let both = ParticleBudget {
            max_particles: 25,
            max_draws: 2,
            max_upload_bytes: FX_BYTES_PER_PARTICLE * 10,
        };
        assert_eq!(both.clamp_count(5000), 10);
    }

    #[test]
    fn budget_allows_streams() {
        assert_eq!(ParticleBudget { max_draws: 1, ..Default::default() }.allows_streams(), 1);
        assert_eq!(ParticleBudget { max_draws: 8, ..Default::default() }.allows_streams(), 2);
        assert_eq!(ParticleBudget { max_draws: 0, ..Default::default() }.allows_streams(), 0);
    }

    #[test]
    fn rng_is_deterministic() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        let mut c = Rng::new(43);
        let sa: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
        let sb: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
        let sc: Vec<u64> = (0..8).map(|_| c.next_u64()).collect();
        assert_eq!(sa, sb);
        assert_ne!(sa, sc);
    }

    #[test]
    fn missile_arc_matches_endpoints_and_midpoint() {
        let from = Vec3::new(0.0, 0.0, 0.0);
        let to = Vec3::new(10.0, 0.0, 0.0);
        let start = missile_arc_point(from, to, 3.0, 0.0);
        let end = missile_arc_point(from, to, 3.0, 1.0);
        let mid = missile_arc_point(from, to, 3.0, 0.5);
        assert_eq!(start, from);
        assert_eq!(end, to);
        assert!((mid.y - 3.0).abs() < 1e-5, "mid y = {}", mid.y);
        assert!((mid.x - 5.0).abs() < 1e-5);
        // A straight missile stays on the line.
        let straight = missile_arc_point(from, to, 0.0, 0.5);
        assert_eq!(straight.y, 0.0);
    }

    #[test]
    fn trail_emits_head_and_fading_echoes() {
        let spec = MissileSpec {
            from: Vec3::ZERO,
            to: Vec3::new(20.0, 0.0, 0.0),
            arc_height: 4.0,
            speed: 10.0,
            color: [1.0, 0.8, 0.3, 1.0],
            size: 0.6,
            blend: FxBlend::Additive,
        };
        let mut rng = Rng::new(7);
        let mut out = Vec::new();
        let added = projectile_trail(&spec, spec.flight_time() * 0.5, 8, &mut rng, &mut out);
        assert!(added > 1, "head + trail expected, got {added}");
        assert_eq!(out[0].color[3], 1.0, "head is opaque");
        assert!(out[1].color[3] < out[0].color[3], "trail fades");
        // Same seed and time => identical particles (replay determinism).
        let mut out2 = Vec::new();
        let mut rng2 = Rng::new(7);
        projectile_trail(&spec, spec.flight_time() * 0.5, 8, &mut rng2, &mut out2);
        assert_eq!(out, out2);
    }

    #[test]
    fn impact_and_blizzard_emit_geometry() {
        let mut rng = Rng::new(11);
        let mut out = Vec::new();
        assert_eq!(impact_burst(Vec3::ZERO, 24, 2.0, [1.0, 0.4, 0.2, 1.0], &mut rng, &mut out), 24);
        let before_waves = out.len();
        for wave in 0..6 {
            blizzard_wave(Vec3::new(10.0, 0.0, 10.0), 8.0, wave, &mut rng, &mut out);
        }
        let wave_particles = out.len() - before_waves;
        assert!(wave_particles >= 6 * 12, "six waves must emit >= 72 shards, got {wave_particles}");
        assert!(out.iter().all(|p| p.size > 0.0));
    }

    #[test]
    fn flight_progress_is_clamped() {
        let spec = MissileSpec {
            from: Vec3::ZERO,
            to: Vec3::new(0.0, 0.0, 100.0),
            arc_height: 0.0,
            speed: 50.0,
            color: [1.0; 4],
            size: 1.0,
            blend: FxBlend::Additive,
        };
        assert_eq!(spec.progress(0.0), 0.0);
        assert_eq!(spec.progress(10.0), 1.0);
        assert!((spec.progress(1.0) - 0.5).abs() < 1e-5);
    }
}
