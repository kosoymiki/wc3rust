//! Instanced MDX geometry-emitter pass (`PRE2` particles, `RIBB` ribbons,
//! `SPL` splats, `UBR` ubersplats).
//!
//! # Why this pass exists
//!
//! [`crate::fx`] draws camera-facing quads from a CPU-built vertex stream with a
//! single procedural texture and two blends. Classic WC3 effects need more than
//! that: sprite atlases (`rows` × `columns`), per-sprite UV intervals
//! (`start`, `end`, `repeat`), two-segment colour/scale ramps, stretched tails,
//! four blend modes and a per-emitter texture. That is exactly the shape of
//! `mdx-m3-viewer`'s geometry-emitter renderer, and this module is its wgpu
//! port.
//!
//! # Mechanism
//!
//! * One instance = one particle / ribbon segment / splat, packed in
//!   [`PARTICLE_INSTANCE_STRIDE`] bytes (`mdx-m3-viewer/src/viewer/handlers/mdx/
//!   geometryemitterfuncs.ts` `BYTES_PER_OBJECT = 60`; ours is 76 because the
//!   four corner vectors are `vec4` and the tail/UV bytes ride in one word).
//! * The quad is generated in the vertex shader from `vertex_index`, so the
//!   pass uploads **no** vertex data per frame.
//! * Per-emitter parameters (segment colours, scaling, UV intervals, atlas size,
//!   filter mode, team colour) live in a uniform block addressed by a dynamic
//!   offset, so N emitters share one buffer and one bind group.
//! * Blending is fixed-function per pipeline; `AlphaKey`/`Modulate` fragment
//!   discards use the same thresholds as the donor
//!   (`shaders/particles.frag.ts:18-28`).
//!
//! The ordering rule is the donor's too
//! (`viewer/handlers/mdx/setupgroups.ts:76-82`): translucent draws sort by
//! filter mode first, then every emitter family is ordered by its priority
//! plane, so a `PRE2` with `priority_plane = 1` draws over ordinary blended
//! geometry of the same model.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};

use crate::frame::DynamicBuffer;
use crate::mesh::DEPTH_FORMAT;
use crate::perf::Counters;

/// Emitter families the shader knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EmitterKind {
    /// `PRE2`: atlas sprite, optional stretched tail.
    Particle2 = 0,
    /// `RIBB`: camera-facing strip through the emitted points.
    Ribbon = 1,
    /// `SPL` / `FPT`: a splat quad from `Splats\SplatData.txt`.
    Splat = 2,
    /// `UBR`: an ubersplat quad.
    UberSplat = 3,
}

impl EmitterKind {
    pub const COUNT: usize = 4;

    fn index(self) -> usize {
        self as usize
    }
}

/// Blend mode of an emitter draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ParticleBlend {
    /// `SrcAlpha` / `OneMinusSrcAlpha` (particles 0, splats).
    Blend = 0,
    /// `SrcAlpha` / `One` (magic glow, fire).
    Additive = 1,
    /// `Zero` / `SrcColor` (particles 2: darkening dust).
    Modulate = 2,
    /// `DstColor` / `SrcColor` (particles 3).
    Modulate2x = 3,
}

impl ParticleBlend {
    pub const COUNT: usize = 4;

    fn index(self) -> usize {
        self as usize
    }

    /// WC3 `PRE2` filter mode -> blend stream (`filtermode.ts:emitterFilterMode`).
    pub fn from_filter(filter: u32) -> Self {
        match filter {
            1 => Self::Additive,
            2 => Self::Modulate,
            3 => Self::Modulate2x,
            _ => Self::Blend,
        }
    }

    /// WC3 material layer filter mode -> blend stream (0 none, 1 transparent,
    /// 2 blend, 3 additive, 4 add alpha, 5 modulate, 6 modulate 2x).
    pub fn from_layer_blend(blend: u32) -> Self {
        match blend {
            3 | 4 => Self::Additive,
            5 => Self::Modulate,
            6 => Self::Modulate2x,
            _ => Self::Blend,
        }
    }

    fn state(self) -> wgpu::BlendState {
        let (src, dst) = match self {
            Self::Blend => (wgpu::BlendFactor::SrcAlpha, wgpu::BlendFactor::OneMinusSrcAlpha),
            Self::Additive => (wgpu::BlendFactor::SrcAlpha, wgpu::BlendFactor::One),
            Self::Modulate => (wgpu::BlendFactor::Zero, wgpu::BlendFactor::Src),
            Self::Modulate2x => (wgpu::BlendFactor::Dst, wgpu::BlendFactor::Src),
        };
        wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: src,
                dst_factor: dst,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: src,
                dst_factor: dst,
                operation: wgpu::BlendOperation::Add,
            },
        }
    }

    /// Depth write is off for every emitter blend (WC3 draws FX after geometry
    /// with `depthMask(false)` — `viewer/handlers/mdx/layer.ts:107`).
    fn depth_write(self) -> bool {
        false
    }
}

/// One emitter instance as the vertex shader consumes it.
///
/// Field meaning depends on [`EmitterKind`], mirroring the donor's union:
///
/// | kind | p0 | p1 | p2 | p3 |
/// |------|----|----|----|----|
/// | `Particle2` head | centre xyz, `w` = facing | `x` = billboard rotation | `x` = scale | unused |
/// | `Particle2` tail | tail start xyz | tail end xyz | `x` = width scale | unused |
/// | `Ribbon` | above (self) | below (self) | above (next) | below (next) |
/// | `Splat`/`UberSplat` | corner 0 | corner 1 | corner 2 | corner 3 |
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
pub struct ParticleInstance {
    pub p0: [f32; 4],
    pub p1: [f32; 4],
    pub p2: [f32; 4],
    pub p3: [f32; 4],
    /// Remaining life in seconds (`Particle2`/`Splat`) or 0 for ribbons.
    pub health: f32,
    /// Straight (non-premultiplied) RGBA, `Unorm8` on the wire.
    pub color: [u8; 4],
    /// `byte0` = tail flag (0 head, 1 tail), `byte1..3` = ribbon `left`,
    /// `right`, `top`, all `0..=255`.
    pub flags: u32,
}

/// `size_of::<ParticleInstance>()` — the instance array stride.
pub const PARTICLE_INSTANCE_STRIDE: usize = 76;

impl ParticleInstance {
    /// Pack the ribbon `left`/`right`/`top` UV bytes (0..=1 floats).
    pub fn pack_ribbon(left: f32, right: f32, top: f32) -> u32 {
        let left = (left.clamp(0.0, 1.0) * 255.0) as u32 & 0xFF;
        let right = (right.clamp(0.0, 1.0) * 255.0) as u32 & 0xFF;
        let top = (top.clamp(0.0, 1.0) * 255.0) as u32 & 0xFF;
        left | (right << 8) | (top << 16)
    }

    /// `true` when byte 0 of [`Self::flags`] marks a tail instance.
    pub fn is_tail(&self) -> bool {
        self.flags & 0xFF != 0
    }
}

/// Per-emitter shader parameters (one 256-byte slot, dynamic offset).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct EmitterParams {
    /// Three segment colours, alpha included (straight RGBA).
    pub colors: [[f32; 4]; 3],
    /// Four UV intervals `[start, end, repeat]`: head life, head decay, tail
    /// life, tail decay.
    pub intervals: [[u32; 4]; 4],
    /// Segment scales.
    pub scaling: [f32; 4],
    /// Team colour applied when `misc2.y` is non-zero (replaceable id 1/2).
    pub team_color: [f32; 4],
    /// `x` = life span, `y` = time middle, `z` = columns, `w` = rows.
    pub misc: [f32; 4],
    /// `x` = filter mode, `y` = team coloured (0/1), `z` = kind, `w` = unused.
    pub misc2: [u32; 4],
    /// Padding to the 256-byte dynamic-offset stride.
    pub _pad: [f32; 16],
}

/// Bytes per [`EmitterParams`] slot (`min_uniform_buffer_offset_alignment`).
pub const EMITTER_PARAMS_STRIDE: u64 = 256;

impl Default for EmitterParams {
    fn default() -> Self {
        Self {
            colors: [[1.0, 1.0, 1.0, 1.0]; 3],
            intervals: [[0, 0, 1, 0]; 4],
            scaling: [1.0, 1.0, 1.0, 0.0],
            team_color: [1.0, 1.0, 1.0, 1.0],
            misc: [1.0, 0.5, 1.0, 1.0],
            misc2: [0, 0, 0, 0],
            _pad: [0.0; 16],
        }
    }
}

/// One recorded draw: a contiguous instance range with its material.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleDraw {
    pub kind: EmitterKind,
    pub blend: ParticleBlend,
    pub start: u32,
    pub count: u32,
    /// Index into the caller's texture store.
    pub texture: usize,
    /// Slot index inside the params buffer.
    pub params: u32,
    /// WC3 priority plane (draw order inside a blend stream).
    pub priority: i32,
}

/// Per-frame counter block, kept separate from [`Counters`] so a caller can
/// assert the emitter budget without mixing in mesh/UI work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParticleStats {
    pub instances: u32,
    pub dropped_budget: u32,
    pub draws: u32,
    pub vertex_invocations: u32,
    pub upload_bytes: u64,
    pub buffer_grows: u32,
    pub prepare_micros: u64,
}

/// Hard per-frame emitter budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmitterBudget {
    pub max_instances: usize,
    pub max_draws: u32,
    pub max_upload_bytes: u64,
}

impl Default for EmitterBudget {
    fn default() -> Self {
        Self {
            max_instances: 8192,
            max_draws: 256,
            max_upload_bytes: 1024 * 1024,
        }
    }
}

impl EmitterBudget {
    /// Instances that fit both the count and the byte cap.
    pub fn clamp_instances(&self, emitted: usize) -> usize {
        let by_bytes = (self.max_upload_bytes / PARTICLE_INSTANCE_STRIDE as u64) as usize;
        emitted.min(self.max_instances).min(by_bytes)
    }
}

/// Camera vectors the shader needs (billboard axes + view direction).
#[derive(Clone, Copy, Debug)]
pub struct ParticleCamera {
    pub view_proj: Mat4,
    /// World-space camera right (unit).
    pub right: Vec3,
    /// World-space camera up (unit).
    pub up: Vec3,
    /// World-space view direction (unit, pointing away from the camera).
    pub forward: Vec3,
}

impl ParticleCamera {
    /// Derive the axes from a view-projection matrix.
    ///
    /// Works for perspective and orthographic cameras: the inverse transform
    /// of clip X/Y gives the world axes, and the two clip-Z points give the
    /// view direction.
    pub fn from_view_proj(view_proj: &Mat4) -> Self {
        let inverse = view_proj.inverse();
        let right = (inverse * Vec4::new(1.0, 0.0, 0.0, 0.0))
            .truncate()
            .normalize_or_zero();
        let up = (inverse * Vec4::new(0.0, 1.0, 0.0, 0.0))
            .truncate()
            .normalize_or_zero();
        let near = (inverse * Vec4::new(0.0, 0.0, 0.0, 1.0)).truncate();
        let far = (inverse * Vec4::new(0.0, 0.0, 1.0, 1.0)).truncate();
        let forward = (far - near).normalize_or_zero();
        Self {
            view_proj: *view_proj,
            right: if right.length_squared() > 1e-12 {
                right
            } else {
                Vec3::X
            },
            up: if up.length_squared() > 1e-12 { up } else { Vec3::Y },
            forward: if forward.length_squared() > 1e-12 {
                forward
            } else {
                Vec3::Z
            },
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    right: [f32; 4],
    up: [f32; 4],
    forward: [f32; 4],
    _pad: [f32; 4],
}

const PARTICLES_SHADER: &str = r#"
struct Camera {
    view_proj: mat4x4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    forward: vec4<f32>,
    _pad: vec4<f32>,
};

struct Params {
    colors: array<vec4<f32>, 3>,
    intervals: array<vec4<u32>, 4>,
    scaling: vec4<f32>,
    team_color: vec4<f32>,
    // x = life span, y = time middle, z = columns, w = rows
    misc: vec4<f32>,
    // x = filter mode, y = team coloured, z = kind, w = unused
    misc2: vec4<u32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<uniform> params: Params;
@group(2) @binding(0) var tex: texture_2d<f32>;
@group(2) @binding(1) var samp: sampler;

struct VsIn {
    @location(0) p0: vec4<f32>,
    @location(1) p1: vec4<f32>,
    @location(2) p2: vec4<f32>,
    @location(3) p3: vec4<f32>,
    @location(4) health: f32,
    @location(5) color: vec4<f32>,
    @location(6) flags: u32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

const KIND_PARTICLE2: u32 = 0u;
const KIND_RIBBON: u32 = 1u;
const KIND_SPLAT: u32 = 2u;
const KIND_UBERSPLAT: u32 = 3u;

fn tail_flag(flags: u32) -> f32 { return f32(flags & 0xFFu); }
fn left_byte(flags: u32) -> f32 { return f32((flags >> 8u) & 0xFFu); }
fn right_byte(flags: u32) -> f32 { return f32((flags >> 16u) & 0xFFu); }
fn top_byte(flags: u32) -> f32 { return f32((flags >> 24u) & 0xFFu); }

// Sprite cell for one UV interval `[start, end, repeat]`
// (`shaders/particles.vert.ts:getCell`): `repeat` is the number of times the
// strip plays inside its interval; `0` keeps the first sprite.
fn get_cell(interval: vec3<u32>, factor: f32) -> f32 {
    let start = f32(interval.x);
    let end = f32(interval.y);
    let repeat = f32(interval.z);
    let sprites = end - start;
    if (sprites > 0.0) {
        let index = floor(sprites * repeat * factor);
        let wrapped = index - sprites * floor(index / sprites);
        return min(start + wrapped, max(params.misc.z * params.misc.w - 1.0, 0.0));
    }
    return start;
}

// Quad corner for `vertex_index` (0,1,2,0,2,3 -> the two triangles of a quad).
fn quad_corner(vertex_index: u32) -> vec2<f32> {
    let index = vertex_index % 6u;
    if (index == 0u) { return vec2<f32>(-1.0, -1.0); }
    if (index == 1u) { return vec2<f32>(1.0, -1.0); }
    if (index == 2u) { return vec2<f32>(1.0, 1.0); }
    if (index == 3u) { return vec2<f32>(-1.0, -1.0); }
    if (index == 4u) { return vec2<f32>(1.0, 1.0); }
    return vec2<f32>(-1.0, 1.0);
}

fn cell_uv(cell: f32) -> vec2<f32> {
    let columns = max(params.misc.z, 1.0);
    let rows = max(params.misc.w, 1.0);
    let left = floor(cell % columns);
    let top = floor(cell / columns);
    return vec2<f32>(left / columns, top / rows);
}

fn particle2(input: VsIn, vertex_index: u32) -> VsOut {
    var out: VsOut;
    let quad = quad_corner(vertex_index);
    let life_span = max(params.misc.x, 0.0001);
    let time_middle = clamp(params.misc.y, 0.0001, 0.9999);
    var factor = (life_span - input.health) / life_span;
    var index = 0u;
    if (factor < time_middle) {
        factor = factor / time_middle;
        index = 0u;
    } else {
        factor = (factor - time_middle) / (1.0 - time_middle);
        index = 1u;
    }
    factor = min(max(factor, 0.0), 1.0);

    let scale = mix(params.scaling[index], params.scaling[index + 1u], factor);
    var color = mix(params.colors[index], params.colors[index + 1u], factor);
    let team_coloured = params.misc2.y != 0u;
    if (team_coloured) {
        color = vec4<f32>(color.rgb * params.team_color.rgb, color.a);
    }

    var cell = 0.0;
    let is_tail = tail_flag(input.flags) != 0.0;
    if (team_coloured && !is_tail) {
        cell = left_byte(input.flags);
    } else if (is_tail) {
        cell = get_cell(params.intervals[index + 2u].xyz, factor);
    } else {
        cell = get_cell(params.intervals[index].xyz, factor);
    }

    let columns = max(params.misc.z, 1.0);
    let rows = max(params.misc.w, 1.0);
    let cell_size = vec2<f32>(1.0 / columns, 1.0 / rows);
    let base = cell_uv(cell);
    let right = base.x + cell_size.x;
    let bottom = base.y + cell_size.y;
    out.uv = vec2<f32>(
        select(base.x, right, quad.x > 0.0),
        select(base.y, bottom, quad.y > 0.0)
    );
    out.color = color;

    if (!is_tail) {
        let angle = input.p1.x;
        let cs = cos(angle);
        let sn = sin(angle);
        let v = vec3<f32>(quad.x, quad.y, 1.0);
        let rotated = vec3<f32>(
            v.x * cs - v.y * sn,
            v.x * sn + v.y * cs,
            v.z
        );
        let world = input.p0.xyz + (camera.right.xyz * rotated.x + camera.up.xyz * rotated.y) * scale;
        out.clip = camera.view_proj * vec4<f32>(world, 1.0);
    } else {
        let axis = input.p1.xyz - input.p0.xyz;
        let normal = cross(camera.forward.xyz, normalize(axis));
        let boundary = normal * scale * input.p2.x;
        var position = input.p0.xyz;
        if (quad.x < 0.0 && quad.y < 0.0) {
            position = input.p0.xyz - boundary;
        } else if (quad.x > 0.0 && quad.y < 0.0) {
            position = input.p1.xyz - boundary;
        } else if (quad.x > 0.0 && quad.y > 0.0) {
            position = input.p1.xyz + boundary;
        } else {
            position = input.p0.xyz + boundary;
        }
        out.clip = camera.view_proj * vec4<f32>(position, 1.0);
    }
    return out;
}

fn ribbon(input: VsIn, vertex_index: u32) -> VsOut {
    var out: VsOut;
    let quad = quad_corner(vertex_index);
    let columns = max(params.misc.z, 1.0);
    let rows = max(params.misc.w, 1.0);
    let left = left_byte(input.flags) / 255.0 / columns;
    let right = right_byte(input.flags) / 255.0 / columns;
    let top = top_byte(input.flags) / 255.0 / rows;
    let bottom = top + 1.0 / rows;
    var position = input.p0.xyz;
    if (quad.x > 0.0 && quad.y < 0.0) {
        out.uv = vec2<f32>(right, top);
        position = input.p0.xyz;
    } else if (quad.x > 0.0 && quad.y > 0.0) {
        out.uv = vec2<f32>(right, bottom);
        position = input.p1.xyz;
    } else if (quad.x < 0.0 && quad.y > 0.0) {
        out.uv = vec2<f32>(left, bottom);
        position = input.p2.xyz;
    } else {
        out.uv = vec2<f32>(left, top);
        position = input.p3.xyz;
    }
    out.color = input.color;
    out.clip = camera.view_proj * vec4<f32>(position, 1.0);
    return out;
}

fn splat(input: VsIn, vertex_index: u32, ubersplat: bool) -> VsOut {
    var out: VsOut;
    let quad = quad_corner(vertex_index);
    var position = input.p0.xyz;
    var color = input.color;
    let interval_times = params.scaling;
    let factor = params.misc.x - input.health;
    if (!ubersplat) {
        var index = 0u;
        var t = factor;
        if (t < interval_times.x) {
            t = t / max(interval_times.x, 0.0001);
            index = 0u;
        } else {
            t = (t - interval_times.x) / max(interval_times.y, 0.0001);
            index = 1u;
        }
        t = min(max(t, 0.0), 1.0);
        let cell = get_cell(params.intervals[index].xyz, t);
        let base = cell_uv(cell);
        let cell_size = vec2<f32>(1.0 / max(params.misc.z, 1.0), 1.0 / max(params.misc.w, 1.0));
        let right = base.x + cell_size.x;
        let bottom = base.y + cell_size.y;
        out.uv = vec2<f32>(select(base.x, right, quad.x > 0.0), select(base.y, bottom, quad.y > 0.0));
        color = mix(params.colors[index], params.colors[index + 1u], t);
    } else {
        let birth = max(interval_times.x, 0.0001);
        let pause = max(interval_times.y, 0.0);
        let decay = max(interval_times.z, 0.0001);
        if (factor < interval_times.x) {
            color = mix(params.colors[0], params.colors[1], factor / birth);
        } else if (factor < interval_times.x + interval_times.y) {
            color = params.colors[1];
        } else {
            color = mix(params.colors[1], params.colors[2], (factor - interval_times.x - interval_times.y) / decay);
        }
        let _unused = pause;
        out.uv = vec2<f32>(select(0.0, 1.0, quad.x > 0.0), select(0.0, 1.0, quad.y > 0.0));
    }
    if (quad.x > 0.0 && quad.y < 0.0) {
        position = input.p1.xyz;
    } else if (quad.x > 0.0 && quad.y > 0.0) {
        position = input.p2.xyz;
    } else if (quad.x < 0.0 && quad.y > 0.0) {
        position = input.p3.xyz;
    }
    out.color = color;
    out.clip = camera.view_proj * vec4<f32>(position, 1.0);
    return out;
}

@vertex
fn vs_main(input: VsIn, @builtin(vertex_index) vertex_index: u32) -> VsOut {
    let kind = params.misc2.z;
    if (kind == KIND_PARTICLE2) {
        return particle2(input, vertex_index);
    } else if (kind == KIND_RIBBON) {
        return ribbon(input, vertex_index);
    } else if (kind == KIND_SPLAT) {
        return splat(input, vertex_index, false);
    }
    return splat(input, vertex_index, true);
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    let texel = textureSample(tex, samp, input.uv);
    let color = texel * input.color;
    let filter_mode = params.misc2.x;
    let kind = params.misc2.z;
    if (kind == KIND_RIBBON && filter_mode == 1u && color.a < 0.75) {
        discard;
    }
    if (kind == KIND_PARTICLE2 && (filter_mode == 2u || filter_mode == 3u) && color.a < 0.02) {
        discard;
    }
    if (kind == KIND_PARTICLE2 && filter_mode == 4u && color.a < 0.75) {
        discard;
    }
    return color;
}
"#;

/// GPU state of the emitter pass.
pub struct ParticlePass {
    pipelines: Vec<Vec<wgpu::RenderPipeline>>,
    camera_buffer: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    params_buffer: wgpu::Buffer,
    params_bind: wgpu::BindGroup,
    params_capacity: u32,
    instances: DynamicBuffer,
    /// Recorded draws in pass order.
    draws: Vec<ParticleDraw>,
    stats: ParticleStats,
    layout: wgpu::BindGroupLayout,
}

impl ParticlePass {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particles-shader"),
            source: wgpu::ShaderSource::Wgsl(PARTICLES_SHADER.into()),
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particles-camera-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<CameraUniform>() as u64,
                    ),
                },
                count: None,
            }],
        });
        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particles-params-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<EmitterParams>() as u64
                    ),
                },
                count: None,
            }],
        });
        let texture_layout = crate::texture::bind_group_layout(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particles-pipeline-layout"),
            bind_group_layouts: &[Some(&camera_layout), Some(&params_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let attributes = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 48,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 64,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Unorm8x4,
                offset: 68,
                shader_location: 5,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32,
                offset: 72,
                shader_location: 6,
            },
        ];
        let mut pipelines = Vec::with_capacity(EmitterKind::COUNT);
        for _kind in 0..EmitterKind::COUNT {
            let mut row = Vec::with_capacity(ParticleBlend::COUNT);
            for blend in 0..ParticleBlend::COUNT {
                row.push(
                    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some("particles"),
                        layout: Some(&pipeline_layout),
                        vertex: wgpu::VertexState {
                            module: &shader,
                            entry_point: Some("vs_main"),
                            compilation_options: Default::default(),
                            // Slot 0 is the instance stream: the quad itself is
                            // generated from `vertex_index`, so there is no
                            // per-vertex buffer at all.
                            buffers: &[Some(wgpu::VertexBufferLayout {
                                array_stride: PARTICLE_INSTANCE_STRIDE as u64,
                                step_mode: wgpu::VertexStepMode::Instance,
                                attributes: &attributes,
                            })],
                        },
                        fragment: Some(wgpu::FragmentState {
                            module: &shader,
                            entry_point: Some("fs_main"),
                            compilation_options: Default::default(),
                            targets: &[Some(wgpu::ColorTargetState {
                                format,
                                blend: Some(blend_to_state(blend)),
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
                            depth_write_enabled: Some(blend_depth_write(blend)),
                            depth_compare: Some(wgpu::CompareFunction::LessEqual),
                            stencil: wgpu::StencilState::default(),
                            bias: wgpu::DepthBiasState::default(),
                        }),
                        multisample: wgpu::MultisampleState::default(),
                        multiview_mask: None,
                        cache: None,
                    }),
                );
            }
            pipelines.push(row);
        }
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particles-camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particles-camera-bind"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let params_capacity = 16u32;
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particles-params"),
            size: EMITTER_PARAMS_STRIDE * params_capacity as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particles-params-bind"),
            layout: &params_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &params_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<EmitterParams>() as u64),
                }),
            }],
        });
        let instances = DynamicBuffer::new(
            device,
            1024,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            "particles-instances",
        );
        Self {
            pipelines,
            camera_buffer,
            camera_bind,
            params_buffer,
            params_bind,
            params_capacity,
            instances,
            draws: Vec::new(),
            stats: ParticleStats::default(),
            layout: params_layout,
        }
    }

    pub fn stats(&self) -> ParticleStats {
        self.stats
    }

    pub fn draws(&self) -> &[ParticleDraw] {
        &self.draws
    }

    /// Upload the instances and parameter blocks of one frame.
    ///
    /// `instances` is the concatenation of every batch; `draws` must reference
    /// ranges inside it. Draws are re-ordered here: translucency is already
    /// implied by the blend pipeline, inside a blend stream the priority plane
    /// decides (donor `setupgroups.ts:76-82`).
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: &ParticleCamera,
        instances: &[ParticleInstance],
        params: &[EmitterParams],
        draws: &[ParticleDraw],
        budget: EmitterBudget,
    ) -> ParticleStats {
        let started = std::time::Instant::now();
        self.instances.begin_frame();
        self.draws.clear();
        let allowed = budget.clamp_instances(instances.len());
        let allowed_instances = &instances[..allowed];
        let dropped = instances.len().saturating_sub(allowed);

        let bytes: &[u8] = bytemuck::cast_slice(allowed_instances);
        let uploaded = self.instances.write(device, queue, bytes);

        // Camera block.
        let uniform = CameraUniform {
            view_proj: camera.view_proj.to_cols_array_2d(),
            right: camera.right.extend(1.0).to_array(),
            up: camera.up.extend(1.0).to_array(),
            forward: camera.forward.extend(0.0).to_array(),
            _pad: [0.0; 4],
        };
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));

        // Parameter blocks: grow on demand (256-byte stride).
        let needed = params.len().max(1) as u32;
        if needed > self.params_capacity {
            let capacity = needed.next_power_of_two();
            self.params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("particles-params"),
                size: EMITTER_PARAMS_STRIDE * capacity as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.params_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("particles-params-bind"),
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.params_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(std::mem::size_of::<EmitterParams>() as u64),
                    }),
                }],
            });
            self.params_capacity = capacity;
        }
        for (index, block) in params.iter().enumerate() {
            queue.write_buffer(
                &self.params_buffer,
                index as u64 * EMITTER_PARAMS_STRIDE,
                bytemuck::bytes_of(block),
            );
        }

        // Sort draws: blend stream first (opaque-ish additive before modulate is
        // arbitrary but stable), then priority plane, then kind for batching.
        let mut ordered: Vec<ParticleDraw> = draws
            .iter()
            .copied()
            .filter(|draw| draw.count > 0 && draw.start + draw.count <= allowed as u32)
            .collect();
        ordered.sort_by_key(|draw| {
            (
                draw.blend.index(),
                draw.priority,
                draw.kind.index(),
                draw.texture,
                draw.params,
            )
        });
        if ordered.len() > budget.max_draws as usize {
            ordered.truncate(budget.max_draws as usize);
        }
        self.draws = ordered;

        self.stats = ParticleStats {
            instances: allowed_instances.len() as u32,
            dropped_budget: dropped as u32,
            draws: self.draws.len() as u32,
            vertex_invocations: self.draws.iter().map(|draw| draw.count * 6).sum(),
            upload_bytes: if uploaded { bytes.len() as u64 } else { 0 },
            buffer_grows: self.instances.grows,
            prepare_micros: started.elapsed().as_micros() as u64,
        };
        self.stats
    }

    /// Record the emitter draws into a pass that has the depth attachment bound
    /// with `Load` and the color attachment with `Load`.
    pub fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        textures: &crate::texture::TextureStore,
        stats: &mut Counters,
    ) {
        if self.draws.is_empty() {
            return;
        }
        pass.set_bind_group(0, &self.camera_bind, &[]);
        pass.set_vertex_buffer(0, self.instances.buffer().slice(..));
        for draw in &self.draws {
            let pipeline = &self.pipelines[draw.kind.index()][draw.blend.index()];
            pass.set_pipeline(pipeline);
            let offset = draw.params as u64 * EMITTER_PARAMS_STRIDE;
            pass.set_bind_group(1, &self.params_bind, &[offset as u32]);
            // A missing texture slot falls back to the store's first texture
            // (`TextureStore::first_or`), exactly like the mesh pass: one
            // untextured emitter must never invalidate the whole frame.
            pass.set_bind_group(2, Some(textures.first_or(draw.texture)), &[]);
            pass.draw(0..6, draw.start..draw.start + draw.count);
            stats.draws += 1;
            stats.pipeline_binds += 1;
            stats.vertices += (draw.count * 6) as u64;
        }
    }

    /// Parameter slots still allocated (diagnostics).
    pub fn params_capacity(&self) -> u32 {
        self.params_capacity
    }
}

fn blend_to_state(blend: usize) -> wgpu::BlendState {
    match blend {
        0 => ParticleBlend::Blend.state(),
        1 => ParticleBlend::Additive.state(),
        2 => ParticleBlend::Modulate.state(),
        _ => ParticleBlend::Modulate2x.state(),
    }
}

fn blend_depth_write(blend: usize) -> bool {
    match blend {
        0 => ParticleBlend::Blend.depth_write(),
        1 => ParticleBlend::Additive.depth_write(),
        2 => ParticleBlend::Modulate.depth_write(),
        _ => ParticleBlend::Modulate2x.depth_write(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_layout_matches_the_stride() {
        assert_eq!(std::mem::size_of::<ParticleInstance>(), PARTICLE_INSTANCE_STRIDE);
        assert_eq!(PARTICLE_INSTANCE_STRIDE, 76);
        // Every attribute offset must land inside the stride.
        for offset in [0usize, 16, 32, 48, 64, 68, 72] {
            assert!(offset + 4 <= PARTICLE_INSTANCE_STRIDE);
        }
    }

    #[test]
    fn params_fit_the_dynamic_offset_stride() {
        let size = std::mem::size_of::<EmitterParams>() as u64;
        assert!(size <= EMITTER_PARAMS_STRIDE, "params {size} > 256");
        assert_eq!(size % 16, 0, "std140 alignment");
    }

    #[test]
    fn filter_modes_map_to_the_donor_blend_streams() {
        assert_eq!(ParticleBlend::from_filter(0), ParticleBlend::Blend);
        assert_eq!(ParticleBlend::from_filter(1), ParticleBlend::Additive);
        assert_eq!(ParticleBlend::from_filter(2), ParticleBlend::Modulate);
        assert_eq!(ParticleBlend::from_filter(3), ParticleBlend::Modulate2x);
        assert_eq!(ParticleBlend::from_filter(4), ParticleBlend::Blend);
        assert_eq!(ParticleBlend::from_layer_blend(2), ParticleBlend::Blend);
        assert_eq!(ParticleBlend::from_layer_blend(3), ParticleBlend::Additive);
        assert_eq!(ParticleBlend::from_layer_blend(5), ParticleBlend::Modulate);
        assert_eq!(ParticleBlend::from_layer_blend(6), ParticleBlend::Modulate2x);
    }

    #[test]
    fn ribbon_bytes_pack_and_unpack() {
        let packed = ParticleInstance::pack_ribbon(0.0, 0.5, 1.0);
        assert_eq!(packed & 0xFF, 0);
        assert_eq!((packed >> 8) & 0xFF, 127);
        assert_eq!((packed >> 16) & 0xFF, 255);
        // Out-of-range inputs clamp: left 1.5 -> 255, right -1.0 -> 0.
        assert_eq!(ParticleInstance::pack_ribbon(1.5, -1.0, 0.25), 255 | (63 << 16));
    }

    #[test]
    fn camera_axes_follow_the_view_direction() {
        let eye = Vec3::new(0.0, 0.0, 10.0);
        let target = Vec3::new(0.0, 0.0, 0.0);
        let view = Mat4::look_at_lh(eye, target, Vec3::Y);
        let proj = Mat4::perspective_lh(60f32.to_radians(), 16.0 / 9.0, 0.1, 100.0);
        let camera = ParticleCamera::from_view_proj(&(proj * view));
        assert!(camera.right.length() > 0.9, "right {:?}", camera.right);
        assert!(camera.up.length() > 0.9, "up {:?}", camera.up);
        // Looking down -Z from +Z means the view direction is -Z.
        assert!(camera.forward.z < -0.9, "forward {:?}", camera.forward);
    }

    #[test]
    fn budget_clamps_by_count_and_bytes() {
        let budget = EmitterBudget {
            max_instances: 100,
            max_draws: 4,
            max_upload_bytes: u64::MAX,
        };
        assert_eq!(budget.clamp_instances(1000), 100);
        let bytes = EmitterBudget {
            max_instances: 10_000,
            max_draws: 4,
            max_upload_bytes: PARTICLE_INSTANCE_STRIDE as u64 * 7,
        };
        assert_eq!(bytes.clamp_instances(1000), 7);
    }
}
