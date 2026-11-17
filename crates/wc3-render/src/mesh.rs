//! Batched mesh pass with GPU skinning.
//!
//! Architecture:
//!
//! * **Static geometry.** Every model's geosets are uploaded once (bind-pose
//!   positions, normals, UVs, vertex colour, bone indices and weights). A steady
//!   frame uploads no vertex or index data at all — only per-draw material
//!   constants and the units' bone palettes.
//! * **GPU skinning.** Each unit's animated bone matrices go into one storage
//!   buffer; the vertex shader blends four weighted matrices per vertex. The CPU
//!   cost per frame drops from "transform every vertex of every unit" to
//!   "transform the units' nodes".
//! * **One draw per (geoset, material layer)** with the layer's world matrix and
//!   tint in a 96-byte dynamic uniform, so a three-layer geoset shares one index
//!   range instead of duplicating geometry per layer.
//! * **State-sorted draws** by pass (depth-writing first), model, pipeline and
//!   texture, so vertex buffers, pipelines and bind groups change rarely.
//! * **CPU skinning stays available** ([`SkinMode::Cpu`]) as the verified
//!   reference path and as the fallback when the adapter exposes no vertex-stage
//!   storage buffers.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3};

use crate::frame::{DynamicBuffer, Scratch};
use crate::perf::Counters;
use crate::scene::{Ring, Scene};
use crate::texture::{Role, TextureStats, TextureStore};

/// How vertices are transformed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkinMode {
    /// Vertex shader blends the unit's bone matrices (default).
    Gpu,
    /// CPU skins into a dynamic vertex buffer each frame (reference path).
    Cpu,
}

/// One-shot per-unit skinning probe for the device diagnostic log.
#[derive(Clone, Debug, Default)]
pub struct SkinProbe {
    /// Index into the renderer's model table.
    pub model: u32,
    /// `unit.nodes.len()` handed to the palette.
    pub nodes: usize,
    /// First palette matrix index used by this unit.
    pub palette_base: u32,
    /// `palette_base * 64` in the storage buffer.
    pub palette_offset: u64,
    /// Bytes reserved for this unit's matrices.
    pub palette_bytes: u64,
    /// First vertex's first bone index and weight.
    pub first_bone: u16,
    /// Largest bone index present in the unit's model.
    pub max_bone: u16,
    pub first_weight: f32,
    /// Dynamic uniform offset of this unit's first draw.
    pub draw_offset: u64,
    /// `palette_base + first_bone` - the exact storage array index the shader
    /// reads for the first vertex.
    pub shader_index: u64,
}

/// Skinning state for the device diagnostic log.
#[derive(Clone, Debug)]
pub struct SkinInfo {
    pub mode: &'static str,
    pub vertex_stride: u64,
    pub bones_format: &'static str,
    pub weights_format: &'static str,
    pub palette_binding: &'static str,
    pub palette_buffer_size: u64,
    pub palette_usage: wgpu::BufferUsages,
    pub palette_len: usize,
    pub palette_bytes: u64,
    pub draw_stride: u64,
    pub draw_buffer_size: u64,
    pub draw_count: usize,
    pub draw_grows: u32,
    pub min_dynamic_offset_alignment: u64,
    pub max_storage_buffers: u32,
}

/// Packed mesh vertex: 52 bytes.
///
/// `bones`/`weights` are static per model; the CPU path leaves them zeroed and
/// uploads already-skinned positions instead.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MeshVertex {
    pub position: [f32; 3],
    /// `TEXCOORD0`: the layer-0 mapping.
    pub uv: [f32; 2],
    /// `TEXCOORD1`: the layer-1 mapping (`TEXCOORDINDEX := unit` in the engine).
    /// Equal to `uv` when the geoset ships a single UV set.
    pub uv2: [f32; 2],
    pub normal: [i16; 4],
    pub color: [u16; 4],
    pub bones: [u16; 4],
    pub weights: [u16; 4],
}

pub const VERTEX_STRIDE: u64 = std::mem::size_of::<MeshVertex>() as u64;

impl MeshVertex {
    /// Static vertex (no bones): position, normal, UV and an RGBA tint.
    ///
    /// Used by the terrain and doodad builders, which bake their transforms and
    /// colours on the CPU and never skin.
    pub fn colored(position: [f32; 3], normal: Vec3, uv: [f32; 2], color: [f32; 4]) -> Self {
        Self {
            position,
            uv,
            uv2: uv,
            normal: pack_normal(normal),
            color: [
                (color[0].clamp(0.0, 1.0) * u16::MAX as f32) as u16,
                (color[1].clamp(0.0, 1.0) * u16::MAX as f32) as u16,
                (color[2].clamp(0.0, 1.0) * u16::MAX as f32) as u16,
                (color[3].clamp(0.0, 1.0) * u16::MAX as f32) as u16,
            ],
            bones: [0; 4],
            weights: [0; 4],
        }
    }
}

/// Engine blend factors per MDX filter mode (TFT Game.dll tables
/// `0x6f767254/0x6f76726c/0x6f76784c/0x6f767864/0x6f766648`, r2 lane 405):
///
/// * mode 1 — `SRC_ALPHA / ONE_MINUS_SRC_ALPHA` (+ alpha test 192/255);
/// * mode 2 — same pair (+ alpha test 4/255);
/// * mode 3 — `SRC_ALPHA / ONE`;
/// * mode 4 — `DST_COLOR / ZERO` (the layer multiplies the framebuffer);
/// * mode 5 — `DST_COLOR / SRC_COLOR`.
///
/// `None` means "no blending" (mode 0 and the internal cutout pipeline).
pub fn blend_state(blend: u32) -> Option<wgpu::BlendState> {
    let state = |src: wgpu::BlendFactor, dst: wgpu::BlendFactor, alpha: (wgpu::BlendFactor, wgpu::BlendFactor)| {
        wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: src,
                dst_factor: dst,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: alpha.0,
                dst_factor: alpha.1,
                operation: wgpu::BlendOperation::Add,
            },
        }
    };
    match blend {
        1 | 2 => Some(state(
            wgpu::BlendFactor::SrcAlpha,
            wgpu::BlendFactor::OneMinusSrcAlpha,
            (wgpu::BlendFactor::One, wgpu::BlendFactor::OneMinusSrcAlpha),
        )),
        3 => Some(state(
            wgpu::BlendFactor::SrcAlpha,
            wgpu::BlendFactor::One,
            (wgpu::BlendFactor::One, wgpu::BlendFactor::One),
        )),
        4 => Some(state(
            wgpu::BlendFactor::Dst,
            wgpu::BlendFactor::Zero,
            (wgpu::BlendFactor::Zero, wgpu::BlendFactor::One),
        )),
        5 => Some(state(
            wgpu::BlendFactor::Dst,
            wgpu::BlendFactor::Src,
            (wgpu::BlendFactor::Zero, wgpu::BlendFactor::One),
        )),
        _ => None,
    }
}

/// One draw in a [`StaticBatch`].
#[derive(Clone, Copy, Debug)]
pub struct StaticDrawSpec {
    pub index_start: u32,
    pub index_count: u32,
    /// Index into the renderer's texture store.
    pub texture: usize,
    /// MDX filter mode: 0 none, 1 transparent, 2 blend, 3/4 additive.
    pub blend: u32,
    /// 0 = world geometry, 1 = blended, 2 = decal.
    pub group: u8,
    /// Per-draw tint (baked vertex colours are multiplied by it).
    pub tint: [f32; 4],
    /// Vertex-colour decode scale: 1.0 for meshes, 2.0 for terrain (whose shade
    /// range 0.75..1.25 is stored halved in `Unorm16` so it fits the format).
    pub color_scale: f32,
}

impl StaticDrawSpec {
    /// Opaque draw with a white tint (the common case: geometry that already
    /// carries its colour per vertex).
    pub fn opaque(index_start: u32, index_count: u32, texture: usize, blend: u32, group: u8) -> Self {
        Self {
            index_start,
            index_count,
            texture,
            blend,
            group,
            tint: [1.0, 1.0, 1.0, 1.0],
            color_scale: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct StaticDraw {
    group: u8,
    index_start: u32,
    index_count: u32,
    texture: u32,
    pipeline: u8,
    params: u32,
}

/// Immutable geometry uploaded once and drawn with the canonical mesh
/// pipelines: terrain, merged doodads, props.
pub struct StaticBatch {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    params_bind: wgpu::BindGroup,
    stride: u64,
    draws: Vec<StaticDraw>,
    /// Index count of the stream (diagnostics).
    pub index_count: u32,
}

impl StaticBatch {
    /// GPU bytes held by the batch (vertices + indices + draw constants).
    pub fn bytes(&self) -> u64 {
        self.vertices.size() + self.indices.size() + self.params_buffer.size()
    }

    /// Draw count in one group (diagnostics).
    pub fn draws_in_group(&self, group: u8) -> usize {
        self.draws
            .iter()
            .filter(|draw| draw.group == group && draw.index_count > 0)
            .count()
    }
}
/// Maximum weighted bones per vertex the shader blends.
pub const MAX_BONES_PER_VERTEX: usize = 4;

/// Per-draw material state: world matrix, tint and interpretation flags.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct DrawParams {
    pub world: [[f32; 4]; 4],
    pub tint: [f32; 4],
    /// `x` = vertex colour scale (1.0 meshes, 2.0 terrain whose shade is halved),
    /// `y` = first bone-palette index for this draw (GPU skinning).
    pub flags: [f32; 4],
}

impl DrawParams {
    pub fn new(world: Mat4, tint: [f32; 4], color_scale: f32, palette_base: u32) -> Self {
        Self {
            world: world.to_cols_array_2d(),
            tint,
            // flags.z = engine alpha-test ref (`PipelineKind::alpha_ref`),
            // flags.w = texture-coordinate set (0 = TEXCOORD0, 1 = TEXCOORD1).
            flags: [color_scale, palette_base as f32, 0.0, 0.0],
        }
    }

    /// Per-layer material state the engine carries in its texture stages.
    pub fn with_layer(mut self, alpha_ref: f32, uv_set: u32) -> Self {
        self.flags[2] = alpha_ref;
        self.flags[3] = uv_set.min(1) as f32;
        self
    }

    pub fn identity(tint: [f32; 4]) -> Self {
        Self::new(Mat4::IDENTITY, tint, 1.0, 0)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    /// Direction towards the sun (DNC `KGRT`), Y-up.
    light: [f32; 4],
    /// DNC `sun` colour x intensity (linear), `w` reserved for exposure.
    sun: [f32; 4],
    /// DNC `ambient` colour x ambient intensity (linear).
    ambient: [f32; 4],
    /// Flat-surface equivalent, `[r, g, b, 1]` (diagnostics, CPU-side tests).
    day: [f32; 4],
    /// Atmospheric fog colour (linear) and density in `.a`.
    fog: [f32; 4],
    /// `x` = haze start distance, `y` = haze end distance (world units).
    fog_range: [f32; 4],
    /// `xyz` = eye position, `w` = 1.
    eye: [f32; 4],
}

/// Bytes one camera uniform occupies in a wgpu uniform buffer.
///
/// `mat4x4<f32>` (64) + 7 × `vec4<f32>` (16) = 176. Both this renderer and the
/// terrain shader bind a buffer with this exact size; a 112-byte (7-field)
/// buffer bound against the 176-byte WGSL `Camera` aborted the device build
/// (`bound with size 112 where the shader expects 176`, build 0f037ea2).
pub const CAMERA_UNIFORM_SIZE: usize = std::mem::size_of::<CameraUniform>();
const _: () = assert!(
    CAMERA_UNIFORM_SIZE == 176,
    "CameraUniform must stay 176 B (mat4x4 + 7 vec4) to match the WGSL `Camera`"
);

/// Uniform size (bytes) of the leading `struct <name> { … }` block in a WGSL
/// source, computed from the member declarations with uniform-buffer alignment.
///
/// Only the member types this renderer's camera structs use are accepted;
/// anything else panics, so a field added to one side and not the other cannot
/// pass silently.
pub fn wgsl_struct_size(source: &str, name: &str) -> usize {
    fn align_up(value: usize, align: usize) -> usize {
        value.div_ceil(align) * align
    }
    let header = format!("struct {name} {{");
    let start = source
        .find(&header)
        .unwrap_or_else(|| panic!("shader has no `{header}`"));
    let body = &source[start + header.len()..];
    let end = body.find('}').expect("unterminated WGSL struct");
    let mut offset = 0usize;
    for line in body[..end].lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let field = line.trim_end_matches(',').trim();
        let ty = field.split(':').next().expect("field type").trim();
        let (size, align) = match ty {
            "mat4x4<f32>" => (64, 16),
            "vec4<f32>" => (16, 16),
            "vec3<f32>" => (12, 16),
            "vec2<f32>" => (8, 8),
            "f32" => (4, 4),
            other => panic!("unsupported WGSL member type `{other}` in {name}"),
        };
        offset = align_up(offset, align) + size;
    }
    align_up(offset, 16)
}

/// Uniform size the mesh shader's own `Camera` struct expects.
pub fn camera_uniform_size_from_wgsl() -> usize {
    wgsl_struct_size(MESH_SHADER, "Camera")
}

/// One material layer of a geoset.
#[derive(Clone, Debug)]
pub struct MeshLayer {
    /// Index into the texture store.
    pub texture: usize,
    /// Static layer alpha (`KMTA` fallback).
    pub alpha: f32,
    /// MDX filter mode: 0 none, 1 transparent (alpha test), 2 blend,
    /// 3/4 additive, 5/6 modulate.
    pub blend: u32,
    /// Replaceable/team-colour layer: only here does the unit tint apply.
    pub team_color: bool,
    /// Which MDX texture-coordinate set this layer samples
    /// (`TEXCOORDINDEX := layer_index` in the engine): 0 -> `TEXCOORD0`,
    /// 1 -> `TEXCOORD1`.
    pub uv_set: u32,
}

impl MeshLayer {
    pub fn plain(texture: usize) -> Self {
        Self {
            texture,
            alpha: 1.0,
            blend: 0,
            team_color: false,
            uv_set: 0,
        }
    }
}

/// One (bone, weight) pair of a skinned MDX vertex.
///
/// This is the anim layer's [`wc3_anim::runtime::VertexWeight`] — the single
/// weight type shared by `wc3-anim`, the game asset layer and this renderer
/// (`merge-anim.md` C4). No local copy.
pub use wc3_anim::runtime::VertexWeight as BoneWeight;

/// Geoset geometry in model space (MDX is Z-up unless `z_up` is false).
#[derive(Clone, Default, Debug)]
pub struct MeshData {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    /// `TEXCOORD0` per vertex (`geoset.uv_sets[0]`).
    pub uvs: Vec<Vec2>,
    /// `TEXCOORD1` per vertex (`geoset.uv_sets[1]`, empty when the geoset has
    /// only one set; the renderer then reuses `uvs`).
    pub uvs2: Vec<Vec2>,
    pub indices: Vec<u32>,
    /// Weighted bone list per vertex; the SKIN chunk supplies explicit weights,
    /// otherwise every listed bone weighs the same.
    pub bones: Vec<Vec<BoneWeight>>,
    pub layers: Vec<MeshLayer>,
    pub z_up: bool,
    /// Helper plane the model author hides outside specific sequences.
    pub hide: bool,
}

const MESH_SHADER: &str = r#"
struct Camera {
    view_proj: mat4x4<f32>,
    light: vec4<f32>,
    // DNC `[UnitLights]` split: `sun.rgb` = light colour x intensity (linear),
    // `ambient.rgb` = ambient colour x ambient intensity (linear). The shader
    // evaluates `clamp(ambient + sun * saturate(N.L), 0, 1)`; `day` is the
    // flat-surface equivalent kept for the CPU-side diagnostics shaders.
    sun: vec4<f32>,
    ambient: vec4<f32>,
    day: vec4<f32>,
    // Atmospheric fog: rgb = fog colour (linear), a = density.
    fog: vec4<f32>,
    // x = distance where the haze starts, y = distance where it is full.
    fog_range: vec4<f32>,
    // xyz = eye position, w = 1.
    eye: vec4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct Draw {
    world: mat4x4<f32>,
    tint: vec4<f32>,
    flags: vec4<f32>,
};
@group(2) @binding(0) var<uniform> draw: Draw;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) normal: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) bones: vec4<u32>,
    @location(5) weights: vec4<f32>,
    @location(6) uv2: vec2<f32>,
};

// WC3 binds two texture-coordinate sets and the material layer picks one
// (`TEXCOORDINDEX := unit`): layer 0 reads TEXCOORD0, layer 1 reads TEXCOORD1.
fn layer_uv(input: VsIn) -> vec2<f32> {
    return select(input.uv, input.uv2, draw.flags.w > 0.5);
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) world: vec3<f32>,
};

// GPU skinning: four weighted bone matrices per vertex.
@group(3) @binding(0) var<storage, read> palette: array<mat4x4<f32>>;

@vertex
fn vs_skinned(input: VsIn) -> VsOut {
    var position = input.position;
    var normal = input.normal.xyz;
    var total = 0.0;
    var blended = vec3<f32>(0.0);
    var blended_normal = vec3<f32>(0.0);
    let base = u32(draw.flags.y);
    // Constant indices only: dynamic indexing of a vertex input vector is
    // compiled to an indexed load on some mobile Vulkan drivers, which read
    // the weight lanes as zero and leave the raw bind pose on screen. The four
    // influences are unrolled so every attribute and palette access is a
    // compile-time constant (the Android app additionally defaults to the CPU
    // path until this is verified on the vendor driver).
    let bone0 = input.bones.x;
    let bone1 = input.bones.y;
    let bone2 = input.bones.z;
    let bone3 = input.bones.w;
    let weight0 = input.weights.x;
    let weight1 = input.weights.y;
    let weight2 = input.weights.z;
    let weight3 = input.weights.w;
    if (weight0 > 0.0) {
        let matrix = palette[base + bone0];
        blended = blended + (matrix * vec4<f32>(input.position, 1.0)).xyz * weight0;
        blended_normal = blended_normal + (matrix * vec4<f32>(input.normal.xyz, 0.0)).xyz * weight0;
        total = total + weight0;
    }
    if (weight1 > 0.0) {
        let matrix = palette[base + bone1];
        blended = blended + (matrix * vec4<f32>(input.position, 1.0)).xyz * weight1;
        blended_normal = blended_normal + (matrix * vec4<f32>(input.normal.xyz, 0.0)).xyz * weight1;
        total = total + weight1;
    }
    if (weight2 > 0.0) {
        let matrix = palette[base + bone2];
        blended = blended + (matrix * vec4<f32>(input.position, 1.0)).xyz * weight2;
        blended_normal = blended_normal + (matrix * vec4<f32>(input.normal.xyz, 0.0)).xyz * weight2;
        total = total + weight2;
    }
    if (weight3 > 0.0) {
        let matrix = palette[base + bone3];
        blended = blended + (matrix * vec4<f32>(input.position, 1.0)).xyz * weight3;
        blended_normal = blended_normal + (matrix * vec4<f32>(input.normal.xyz, 0.0)).xyz * weight3;
        total = total + weight3;
    }
    if (total > 0.00001) {
        position = blended / total;
        normal = blended_normal / total;
    }
    var out: VsOut;
    let world = draw.world * vec4<f32>(position, 1.0);
    out.clip = camera.view_proj * world;
    out.uv = layer_uv(input);
    out.color = input.color * draw.tint;
    out.normal = (draw.world * vec4<f32>(normal, 0.0)).xyz;
    out.world = world.xyz;
    return out;
}

// CPU skinning: positions arrive already in model space.
@vertex
fn vs_plain(input: VsIn) -> VsOut {
    var out: VsOut;
    let world = draw.world * vec4<f32>(input.position, 1.0);
    out.clip = camera.view_proj * world;
    out.uv = layer_uv(input);
    out.color = input.color * draw.tint;
    out.normal = (draw.world * vec4<f32>(input.normal.xyz, 0.0)).xyz;
    out.world = world.xyz;
    return out;
}

@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

/// DNC light for this surface: ambient floor + directional sun, clamped per
/// channel. This replaces the old `shade()` x `day_light()` pair, which applied
/// a fake Lambert *and* the full `sun + ambient` sum, so a warm sun tinted every
/// pixel twice.
fn surface_light(normal: vec3<f32>) -> vec3<f32> {
    let n = normalize(normal);
    let l = normalize(camera.light.xyz);
    let ndl = clamp(dot(n, l), 0.0, 1.0);
    return clamp(camera.ambient.rgb + camera.sun.rgb * ndl, vec3<f32>(0.0), vec3<f32>(1.0));
}

/// Atmospheric fog: linear distance ramp from `fog_range.x` to `fog_range.y`,
/// scaled by the map's density. A map with density 0 (the RoC campaign maps)
/// leaves the colour untouched.
fn distance_haze(colour: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    if (camera.fog.a <= 0.001) {
        return colour;
    }
    let span = max(camera.fog_range.y - camera.fog_range.x, 1.0);
    let t = clamp((distance(world, camera.eye.xyz) - camera.fog_range.x) / span, 0.0, 1.0)
        * camera.fog.a;
    return mix(colour, camera.fog.rgb, t);
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, input.uv);
    let scale = draw.flags.x;
    let tint = vec3<f32>(input.color.r * scale, input.color.g * scale, input.color.b * scale);
    let lit = c.rgb * tint * surface_light(input.normal);
    return vec4<f32>(distance_haze(lit, input.world), c.a * input.color.a);
}

// `Modulate` / `Modulate2x`: the texture is the *colour*, not an alpha mask.
// The blend equation does the darkening, so the shader returns the sampled
// colour scaled by the vertex tint and leaves alpha at 1 (WC3 modulates the
// framebuffer, it does not composite).
@fragment
fn fs_modulate(input: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, input.uv);
    let scale = draw.flags.x;
    let tint = vec3<f32>(input.color.r * scale, input.color.g * scale, input.color.b * scale);
    return vec4<f32>(distance_haze(c.rgb * tint, input.world), 1.0);
}

@fragment
fn fs_cutout(input: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, input.uv);
    let alpha = c.a * input.color.a;
    if (alpha < 0.5) {
        discard;
    }
    let scale = draw.flags.x;
    let tint = vec3<f32>(input.color.r * scale, input.color.g * scale, input.color.b * scale);
    let lit = c.rgb * tint * surface_light(input.normal);
    return vec4<f32>(distance_haze(lit, input.world), 1.0);
}
"#;

/// Pipeline slot for one draw. The discriminant is the sort key inside a pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PipelineKind {
    /// Filter mode 0: opaque, no blend, no alpha test.
    Opaque = 0,
    /// Filter mode 1: `SRC_ALPHA / ONE_MINUS_SRC_ALPHA` **plus** alpha test at
    /// 192/255 (`glAlphaFunc(GL_GEQUAL, 192/255)`), depth write on. This is the
    /// body/terrain-art layer of nearly every WC3 unit; treating it as a pure
    /// cutout (`< 0.5`, no blend) darkened every semi-transparent texel.
    AlphaTest = 1,
    /// Filter mode 2: same blend pair, alpha test at 4/255, no depth write
    /// (translucent shadow/detail layers).
    Blend = 2,
    /// Filter mode 3: `SRC_ALPHA / ONE`, alpha test 4/255.
    Additive = 3,
    /// Filter mode 4: `DST_COLOR / ZERO` (the texture multiplies the
    /// framebuffer), alpha test 4/255.
    Modulate = 4,
    /// Filter mode 5: `DST_COLOR / SRC_COLOR`, alpha test 4/255.
    Modulate2x = 5,
    /// Internal hard-cutout pipeline (foliage/particles that ask for it), alpha
    /// test 0.5 with no blending and depth write.
    Cutout = 6,
}

impl PipelineKind {
    /// WC3 MDX filter mode to pipeline.
    pub fn from_blend(blend: u32) -> Self {
        match blend {
            0 => Self::Opaque,
            1 => Self::AlphaTest,
            2 => Self::Blend,
            3 => Self::Additive,
            4 => Self::Modulate,
            5 => Self::Modulate2x,
            _ => Self::Blend,
        }
    }

    /// Engine alpha-test reference for a filter mode:
    /// `glAlphaFunc(GL_GEQUAL, ref/255)` with ref 192 for mode 1 and 4 for
    /// modes 2..5 (TFT Game.dll tables `0x6f7672xx`). Mode 0 has no test.
    pub fn alpha_ref(blend: u32) -> f32 {
        match blend {
            1 => 192.0 / 255.0,
            2..=5 => 4.0 / 255.0,
            _ => 0.0,
        }
    }

    /// Number of pipeline slots (one per [`Self`] variant).
    pub const COUNT: usize = 7;

    /// Draw group: 0 = depth-writing opaque/alpha-test, 1 = blended/additive,
    /// 2 = decals. The game interleaves terrain, water and doodads between these
    /// groups, so the group is part of the sort key and of the draw call.
    pub fn group(self) -> u8 {
        match self {
            Self::Opaque | Self::AlphaTest | Self::Cutout => 0,
            Self::Blend | Self::Additive | Self::Modulate | Self::Modulate2x => 1,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Draw {
    /// 0 = depth-writing world geometry, 1 = blended model layers, 2 = decals.
    group: u8,
    /// WC3 material priority plane (`MTLS` priority). Inside a group the lower
    /// plane draws first, which is the donor's translucent ordering
    /// (`mdx-m3-viewer/src/viewer/handlers/mdx/setupgroups.ts:76-82`).
    priority: i16,
    /// Model index; `u32::MAX` means the frame's dynamic buffer (rings).
    model: u32,
    index_start: u32,
    index_count: u32,
    index_base: u32,
    texture: u32,
    pipeline: u8,
    params: u32,
}

/// One renderer-owned model: the geosets of an MDX model.
pub type Model = Vec<MeshData>;

/// Static GPU geometry for one model.
struct ModelGeometry {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    /// `(index_start, index_count)` per geoset.
    geosets: Vec<(u32, u32)>,
}

/// Batched mesh pass.
pub struct MeshRenderer {
    pipelines_gpu: [wgpu::RenderPipeline; PipelineKind::COUNT],
    pipelines_cpu: [wgpu::RenderPipeline; PipelineKind::COUNT],
    skin_mode: SkinMode,
    skin_info: SkinInfo,
    skin_probes: Vec<SkinProbe>,
    probe_frames: u32,
    camera_buffer: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    /// DNC `sun` (light colour x intensity, linear) for the mesh shader.
    sun_light: [f32; 4],
    /// DNC `ambient` (ambient colour x ambient intensity, linear).
    ambient_light: [f32; 4],
    /// Flat-surface equivalent of `sun_light + ambient_light`, written to the
    /// `day` uniform for the CPU-side diagnostics and A/B captures.
    day_light: [f32; 4],
    /// Direction towards the sun (Y-up), from the DNC `KGRT` track.
    light_dir: [f32; 4],
    /// Map fog colour (linear) + density in `.a`; `None` = no haze.
    fog: Option<([f32; 3], f32, f32, f32)>,
    /// Last eye position, for the distance haze.
    eye: [f32; 3],
    draw_buffer: wgpu::Buffer,
    draw_bind: wgpu::BindGroup,
    draw_stride: u64,
    draw_writes: u32,
    draw_grows: u32,
    palette_layout: wgpu::BindGroupLayout,
    palette_buffer: wgpu::Buffer,
    palette_bind: wgpu::BindGroup,
    vertices: DynamicBuffer,
    indices: DynamicBuffer,
    models: Vec<Model>,
    geometry: Vec<ModelGeometry>,
    textures: TextureStore,
    draw_layout: wgpu::BindGroupLayout,
    // ---- frame scratch (capacity survives across frames)
    vertex_scratch: Scratch<MeshVertex>,
    index_scratch: Scratch<u32>,
    draw_scratch: Scratch<Draw>,
    param_scratch: Scratch<DrawParams>,
    order_scratch: Scratch<u32>,
    bone_scratch: Scratch<Mat4>,
    palette_scratch: Scratch<[f32; 16]>,
    param_staging: Vec<u8>,
    /// WC3 material priority planes, keyed by `(model, geoset, layer)`.
    ///
    /// `MeshLayer` is constructed by the game's asset converter, so the plane is
    /// registered here instead of widening that struct: one call per model after
    /// upload, no per-frame cost.
    layer_priority: std::collections::HashMap<(u32, u32, u32), i16>,
    /// Set when hidden helper planes are culled (visual fidelity switch).
    pub skip_hidden: bool,
    /// Texture memory statistics from load time.
    pub texture_stats: TextureStats,
}

impl MeshRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        models: Vec<Model>,
        images: &[crate::texture::TextureImage],
        max_texture_size: u32,
    ) -> Self {
        let sources: Vec<crate::texture::TextureSource<'_>> = images
            .iter()
            .map(crate::texture::TextureSource::Rgba)
            .collect();
        Self::new_with_sources(device, queue, format, models, &sources, max_texture_size)
    }

    /// Build the renderer from mixed texture sources: RGBA images are capped and
    /// mip-mapped, [`CompressedImage`](crate::texture::CompressedImage) sidecars
    /// (ASTC/ETC2 from the asset pipeline) are uploaded as block data.
    pub fn new_with_sources(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        models: Vec<Model>,
        sources: &[crate::texture::TextureSource<'_>],
        max_texture_size: u32,
    ) -> Self {
        let camera_layout = bind_group_layout_uniform(device, "mesh-camera-layout", false);
        let draw_layout = bind_group_layout_uniform(device, "mesh-draw-layout", true);
        let palette_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh-palette-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let texture_layout = crate::texture::bind_group_layout(device);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mesh-shader"),
            source: wgpu::ShaderSource::Wgsl(MESH_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh-pipeline-layout"),
            bind_group_layouts: &[
                Some(&camera_layout),
                Some(&texture_layout),
                Some(&draw_layout),
                Some(&palette_layout),
            ],
            immediate_size: 0,
        });

        let attributes = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 12,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 20,
                shader_location: 6,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Snorm16x4,
                offset: 28,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Unorm16x4,
                offset: 36,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint16x4,
                offset: 44,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Unorm16x4,
                offset: 52,
                shader_location: 5,
            },
        ];
        // Pipeline table: shader entry, blend state and depth write per engine
        // filter mode (indices match `PipelineKind`). Blend states come from
        // `blend_state`, the single source the `render.blend-mode-table` test
        // asserts against.
        let mut pipelines_gpu = Vec::with_capacity(PipelineKind::COUNT);
        let mut pipelines_cpu = Vec::with_capacity(PipelineKind::COUNT);
        // One pipeline per engine filter mode, in `PipelineKind` order.
        // 0 opaque, 1 alpha-test blend (depth write), 2 blend, 3 additive,
        // 4 modulate, 5 modulate2x, 6 internal cutout.
        let table: [(&str, Option<wgpu::BlendState>, bool); PipelineKind::COUNT] = [
            ("fs_main", None, true),
            ("fs_main", blend_state(1), true),
            ("fs_main", blend_state(2), false),
            ("fs_main", blend_state(3), false),
            ("fs_modulate", blend_state(4), false),
            ("fs_modulate", blend_state(5), false),
            ("fs_cutout", None, true),
        ];
        for (index, (entry, blend, depth_write)) in table.into_iter().enumerate() {
            let label = match index {
                0 => "mesh-opaque",
                1 => "mesh-alpha-test",
                2 => "mesh-blend",
                3 => "mesh-additive",
                4 => "mesh-modulate",
                5 => "mesh-modulate2x",
                _ => "mesh-cutout",
            };
            pipelines_gpu.push(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_skinned"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: VERTEX_STRIDE,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            }));
            pipelines_cpu.push(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_plain"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: VERTEX_STRIDE,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            }));
        }
        let pipelines_gpu: [wgpu::RenderPipeline; PipelineKind::COUNT] =
            pipelines_gpu.try_into().expect("one pipeline per kind");
        let pipelines_cpu: [wgpu::RenderPipeline; PipelineKind::COUNT] =
            pipelines_cpu.try_into().expect("one pipeline per kind");

        // GPU skinning needs read-only storage buffers in the vertex stage.
        // The Android app forces the CPU path after construction (the vendor
        // Vulkan driver leaves skinned models in bind pose); desktop, headless
        // tests and the bench keep this default.
        let skin_mode = if device.limits().max_storage_buffers_per_shader_stage > 0 {
            SkinMode::Gpu
        } else {
            SkinMode::Cpu
        };

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh-camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mesh-camera-bind"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let repeat_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mesh-repeat-sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            // RTS camera looks at the ground at ~45°: 1x anisotropy smears the
            // grass/rock detail into streaks (measured on device), 16x keeps
            // the texel footprint round.
            anisotropy_clamp: 16,
            ..Default::default()
        });
        let textures = TextureStore::upload_sources(
            device,
            queue,
            &texture_layout,
            &repeat_sampler,
            sources,
            max_texture_size,
            true,
        )
        .expect("texture sources");
        let texture_stats = textures.stats;

        let alignment = device.limits().min_uniform_buffer_offset_alignment.max(16) as u64;
        let draw_stride = (std::mem::size_of::<DrawParams>() as u64).div_ceil(alignment) * alignment;
        let draw_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh-draw-ring"),
            size: draw_stride * 512,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let draw_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mesh-draw-bind"),
            layout: &draw_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &draw_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<DrawParams>() as u64),
                }),
            }],
        });

        let palette_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh-palette"),
            size: 64 * 64 * 256,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let palette_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mesh-palette-bind"),
            layout: &palette_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: palette_buffer.as_entire_binding(),
            }],
        });

        // Static geometry per model: one vertex and one index buffer each.
        let mut geometry = Vec::with_capacity(models.len());
        for model in &models {
            let mut vertices: Vec<MeshVertex> = Vec::new();
            let mut indices: Vec<u32> = Vec::new();
            let mut geosets = Vec::with_capacity(model.len());
            for mesh in model {
                let base = vertices.len() as u32;
                push_static_vertices(&mut vertices, mesh);
                let start = indices.len() as u32;
                indices.extend(mesh.indices.iter().map(|index| base + index));
                geosets.push((start, mesh.indices.len() as u32));
            }
            geometry.push(ModelGeometry {
                vertices: upload_static(
                    device,
                    queue,
                    bytemuck::cast_slice(&vertices),
                    wgpu::BufferUsages::VERTEX,
                    "model-vertices",
                ),
                indices: upload_static(
                    device,
                    queue,
                    bytemuck::cast_slice(&indices),
                    wgpu::BufferUsages::INDEX,
                    "model-indices",
                ),
                geosets,
            });
        }

        Self {
            pipelines_gpu,
            pipelines_cpu,
            skin_mode,
            skin_info: SkinInfo {
                mode: if skin_mode == SkinMode::Gpu { "gpu" } else { "cpu" },
                vertex_stride: VERTEX_STRIDE,
                bones_format: "Uint16x4",
                weights_format: "Unorm16x4",
                palette_binding: "@group(3) binding(0) storage(read-only) VERTEX",
                palette_buffer_size: palette_buffer.size(),
                palette_usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                palette_len: 0,
                palette_bytes: 0,
                draw_stride,
                draw_buffer_size: draw_buffer.size(),
                draw_count: 0,
                draw_grows: 0,
                min_dynamic_offset_alignment: alignment,
                max_storage_buffers: device.limits().max_storage_buffers_per_shader_stage,
            },
            skin_probes: Vec::new(),
            probe_frames: 0,
            camera_buffer,
            camera_bind,
            sun_light: [0.0, 0.0, 0.0, 1.0],
            ambient_light: [1.0, 1.0, 1.0, 0.0],
            day_light: [1.0, 1.0, 1.0, 1.0],
            light_dir: [0.42, 0.86, 0.28, 0.0],
            fog: None,
            eye: [0.0; 3],
            draw_buffer,
            draw_bind,
            draw_stride,
            draw_writes: 0,
            draw_grows: 0,
            palette_layout,
            palette_buffer,
            palette_bind,
            vertices: DynamicBuffer::new(
                device,
                8 * 1024,
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                "mesh-dynamic-vertices",
            ),
            indices: DynamicBuffer::new(
                device,
                8 * 1024,
                wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                "mesh-dynamic-indices",
            ),
            models,
            geometry,
            textures,
            draw_layout,
            vertex_scratch: Scratch::with_capacity(16 * 1024),
            index_scratch: Scratch::with_capacity(24 * 1024),
            draw_scratch: Scratch::with_capacity(256),
            param_scratch: Scratch::with_capacity(256),
            order_scratch: Scratch::with_capacity(256),
            bone_scratch: Scratch::with_capacity(64),
            palette_scratch: Scratch::with_capacity(1024),
            param_staging: Vec::new(),
            layer_priority: std::collections::HashMap::new(),
            skip_hidden: true,
            texture_stats,
        }
    }

    pub fn skin_mode(&self) -> SkinMode {
        self.skin_mode
    }

    /// Register the WC3 material priority plane of one layer.
    ///
    /// `MTLS` carries one `priority` per material and every layer of a material
    /// inherits it (`mdx-m3-viewer/src/viewer/handlers/mdx/model.ts:130`); the
    /// renderer sorts blended draws by it, so a `priority 1` cape draws over the
    /// `priority 0` body instead of fighting it by upload order.
    pub fn set_layer_priority(
        &mut self,
        model: usize,
        geoset: usize,
        layer: usize,
        priority: i32,
    ) {
        self.layer_priority.insert(
            (model as u32, geoset as u32, layer as u32),
            priority.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        );
    }

    /// Priority plane of one layer (0 when the asset layer never registered one).
    pub fn layer_priority_of(&self, model: usize, geoset: usize, layer: usize) -> i16 {
        self.layer_priority
            .get(&(model as u32, geoset as u32, layer as u32))
            .copied()
            .unwrap_or(0)
    }

    /// Set the DNC surface light for the mesh shaders.
    ///
    /// `sun` is `[light colour x intensity, exposure]` in linear space and
    /// `ambient` is `[ambient colour x ambient intensity, 0]`, exactly the
    /// `[UnitLights]` split from [`crate::daynight::SurfaceLight`]. `flat` is
    /// the up-facing equivalent, kept in the uniform for diagnostics.
    pub fn set_surface_light(&mut self, sun: [f32; 4], ambient: [f32; 4], flat: [f32; 4]) {
        self.sun_light = sun;
        self.ambient_light = ambient;
        self.day_light = flat;
    }

    /// Legacy single-colour entry point: `[r, g, b, brightness]`. Kept for the
    /// gallery/bench callers that have no split light; the shader treats it as a
    /// flat ambient so old diagnostics keep their exact brightness.
    pub fn set_day_light(&mut self, day: [f32; 4]) {
        self.sun_light = [0.0, 0.0, 0.0, 1.0];
        self.ambient_light = [
            day[0] * day[3],
            day[1] * day[3],
            day[2] * day[3],
            0.0,
        ];
        self.day_light = [day[0] * day[3], day[1] * day[3], day[2] * day[3], 1.0];
    }

    /// Atmospheric fog for the model/doodad pass: colour (linear), density and
    /// the `start`/`end` distance ramp. `None` disables the haze.
    pub fn set_atmosphere_fog(
        &mut self,
        color: [f32; 3],
        density: f32,
        start: f32,
        end: f32,
    ) {
        self.fog = if density > 0.0 {
            Some((color, density, start, end))
        } else {
            None
        };
    }

    /// Direction **towards** the sun (Y-up), normally from
    /// [`crate::daynight::DayNightSample::sun_direction`]. Drives the `N · L`
    /// term of the DNC surface light.
    pub fn set_light_direction(&mut self, dir: [f32; 3]) {
        self.light_dir = [dir[0], dir[1], dir[2], 0.0];
    }

    /// Skinning state captured on the first prepared frame (device log).
    pub fn skin_info(&self) -> &SkinInfo {
        &self.skin_info
    }

    /// Per-unit palette probes captured on the first prepared frame.
    pub fn skin_probes(&self) -> &[SkinProbe] {
        &self.skin_probes
    }

    /// Force a skinning path (both are implemented; the bench uses this to
    /// compare them on the same scene).
    pub fn set_skin_mode(&mut self, mode: SkinMode) {
        self.skin_mode = mode;
        self.skin_info.mode = match mode {
            SkinMode::Gpu => "gpu",
            SkinMode::Cpu => "cpu",
        };
    }

    pub fn texture_stats(&self) -> TextureStats {
        self.texture_stats
    }

    pub fn models(&self) -> &[Model] {
        &self.models
    }

    /// Texture bind groups and their load-time statistics, shared with the UI pass.
    pub fn textures(&self) -> &TextureStore {
        &self.textures
    }

    /// Build and upload everything this frame needs: skinned geometry (or the
    /// bone palette), draw constants and the sorted draw list.
    ///
    /// Callers that own their own render pass use this plus [`draw_group`], so
    /// terrain/water/doodad passes can interleave with the unit groups in the
    /// exact order the game needs. [`encode`] is the convenience path.
    ///
    /// [`draw_group`]: Self::draw_group
    /// [`encode`]: Self::encode
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        mask: crate::scene::PassMask,
        stats: &mut Counters,
    ) {
        let prepare_started = std::time::Instant::now();
        let alloc_start = crate::perf::alloc_traffic();
        self.vertex_scratch.clear();
        self.index_scratch.clear();
        self.draw_scratch.clear();
        self.param_scratch.clear();
        self.palette_scratch.clear();
        self.vertices.begin_frame();
        self.indices.begin_frame();
        self.draw_writes = 0;
        self.draw_grows = 0;
        let collect_probes = self.probe_frames == 0;
        if collect_probes {
            self.skin_probes.clear();
        }
        let gpu_skin = self.skin_mode == SkinMode::Gpu;
        // Keep the diagnostic label in sync even if the mode was forced after
        // construction (the Android app selects the CPU path).
        self.skin_info.mode = if gpu_skin { "gpu" } else { "cpu" };

        if mask.units {
            // Disjoint field borrows: the scratch arenas and the static tables
            // are borrowed separately so the loop can build geometry without
            // borrowing the whole renderer.
            let models = &self.models;
            let geometry_table = &self.geometry;
            let skip_hidden = self.skip_hidden;
            let layer_priority = &self.layer_priority;
            let vertex_scratch = &mut self.vertex_scratch;
            let index_scratch = &mut self.index_scratch;
            let draw_scratch = &mut self.draw_scratch;
            let param_scratch = &mut self.param_scratch;
            let palette_scratch = &mut self.palette_scratch;
            let bone_scratch = &mut self.bone_scratch;
            // Running palette cursor; the CPU path does not fill the palette but
            // the device log still reports the layout the GPU path would use.
            let mut palette_cursor = 0u32;
            for unit in &scene.units {
                let Some(model) = models.get(unit.model) else {
                    continue;
                };
                let Some(geometry) = geometry_table.get(unit.model) else {
                    continue;
                };
                // Reserve this unit's palette once; every geoset of the unit
                // indexes into it. In CPU mode the base is still tracked so the
                // device log shows the palette layout the GPU path would use.
                let palette_base = palette_cursor;
                // A unit without an animator carries no palette; GPU skinning
                // would collapse every skinned vertex (measured: an empty
                // `UnitDraw::nodes` renders the unit invisible). Fall back to the
                // CPU bind-pose path for this unit instead of dropping it.
                let unit_gpu_skin = gpu_skin && !unit.nodes.is_empty();
                if unit_gpu_skin {
                    push_palette_f32(palette_scratch, &unit.nodes, true);
                }
                palette_cursor = palette_cursor.saturating_add(unit.nodes.len() as u32);
                if collect_probes && self.skin_probes.len() < 32 {
                    let first = model
                        .iter()
                        .flat_map(|mesh| mesh.bones.iter())
                        .find_map(|list| list.first().copied());
                    let max_bone = model
                        .iter()
                        .flat_map(|mesh| mesh.bones.iter())
                        .flat_map(|list| list.iter())
                        .map(|pair| pair.bone)
                        .max()
                        .unwrap_or(0);
                    let (first_bone, first_weight) = first
                        .map(|pair| (pair.bone, pair.weight))
                        .unwrap_or((0, 0.0));
                    self.skin_probes.push(SkinProbe {
                        model: unit.model as u32,
                        nodes: unit.nodes.len(),
                        palette_base,
                        palette_offset: palette_base as u64 * 64,
                        palette_bytes: unit.nodes.len() as u64 * 64,
                        first_bone,
                        max_bone,
                        first_weight,
                        draw_offset: param_scratch.len() as u64 * self.draw_stride,
                        shader_index: palette_base as u64 + first_bone as u64,
                    });
                }
                for (mesh_index, mesh) in model.iter().enumerate() {
                    let geoset_alpha = unit.mesh_alpha.get(mesh_index).copied().unwrap_or(1.0);
                    if geoset_alpha <= 0.001
                        || mesh.indices.is_empty()
                        || mesh.positions.is_empty()
                        || (skip_hidden && mesh.hide)
                    {
                        continue;
                    }
                    let mut visible = 0u32;
                    for (layer_index, layer) in mesh.layers.iter().enumerate() {
                        let layer_alpha = unit
                            .layer_alpha
                            .get(mesh_index)
                            .and_then(|alphas| alphas.get(layer_index))
                            .copied()
                            .unwrap_or(layer.alpha);
                        if geoset_alpha * layer_alpha * unit.tint[3] > 0.001 {
                            visible += 1;
                        }
                    }
                    if visible == 0 {
                        continue;
                    }
                    // GPU path: static geometry, one index range per geoset.
                    // CPU path: skin once into the dynamic buffer and emit the
                    // geoset's indices once; layers replay the same range.
                    // The engine evaluates a matrix group whole (up to five
                    // matrices); the GPU vertex format carries four bone slots.
                    // A geoset that needs the fifth is skinned on the CPU for
                    // this draw instead of dropping a matrix (which detaches
                    // the limb), so both paths agree on every vertex.
                    let wide_group = mesh
                        .bones
                        .iter()
                        .any(|list| list.len() > MAX_BONES_PER_VERTEX);
                    let (index_start, index_count, draw_model) = if unit_gpu_skin && !wide_group {
                        let (start, count) = geometry
                            .geosets
                            .get(mesh_index)
                            .copied()
                            .unwrap_or((0, 0));
                        (start, count, unit.model as u32)
                    } else {
                        let base_vertex = skin_cpu_into(vertex_scratch, bone_scratch, unit, mesh);
                        let start = index_scratch.len() as u32;
                        for &index in &mesh.indices {
                            index_scratch.push(base_vertex + index);
                        }
                        (start, mesh.indices.len() as u32, u32::MAX)
                    };
                    // `GEOA` colour grade (`KGAC`); empty means white.
                    let geoset_color = unit
                        .mesh_color
                        .get(mesh_index)
                        .copied()
                        .unwrap_or([1.0, 1.0, 1.0, 1.0]);
                    for (layer_index, layer) in mesh.layers.iter().enumerate() {
                        let layer_alpha = unit
                            .layer_alpha
                            .get(mesh_index)
                            .and_then(|alphas| alphas.get(layer_index))
                            .copied()
                            .unwrap_or(layer.alpha);
                        let alpha = geoset_alpha * layer_alpha * unit.tint[3] * geoset_color[3];
                        if alpha <= 0.001 {
                            continue;
                        }
                        // Team-colour layers take the unit tint, every other
                        // layer keeps the geoset grade only.
                        let rgb = if layer.team_color {
                            [
                                unit.tint[0] * geoset_color[0],
                                unit.tint[1] * geoset_color[1],
                                unit.tint[2] * geoset_color[2],
                            ]
                        } else {
                            [geoset_color[0], geoset_color[1], geoset_color[2]]
                        };
                        let tint = [rgb[0], rgb[1], rgb[2], alpha];
                        let pipeline = PipelineKind::from_blend(layer.blend);
                        let params = param_scratch.len() as u32;
                        param_scratch.push(
                            DrawParams::new(unit.world, tint, 1.0, palette_base).with_layer(
                                PipelineKind::alpha_ref(layer.blend),
                                layer.uv_set,
                            ),
                        );
                        let priority = layer_priority
                            .get(&(unit.model as u32, mesh_index as u32, layer_index as u32))
                            .copied()
                            .unwrap_or(0);
                        draw_scratch.push(Draw {
                            group: pipeline.group(),
                            priority,
                            model: draw_model,
                            index_start,
                            index_count,
                            index_base: 0,
                            texture: layer.texture as u32,
                            pipeline: pipeline as u8,
                            params,
                        });
                    }
                }
            }
            for ring in &scene.rings {
                push_ring(
                    ring,
                    vertex_scratch,
                    index_scratch,
                    draw_scratch,
                    param_scratch,
                );
            }
        }

        stats.build_micros += prepare_started.elapsed().as_micros() as u64;
        let upload_started = std::time::Instant::now();

        // ---- uploads: only what really changes per frame
        self.vertices.write(
            device,
            queue,
            bytemuck::cast_slice(self.vertex_scratch.as_slice()),
        );
        self.indices.write(
            device,
            queue,
            bytemuck::cast_slice(self.index_scratch.as_slice()),
        );
        stats.upload_bytes += self.vertices.bytes + self.indices.bytes;
        stats.buffer_writes += self.vertices.writes + self.indices.writes;
        stats.buffer_grows += self.vertices.grows + self.indices.grows + self.draw_grows;

        if gpu_skin {
            let palette_len = self.palette_scratch.len();
            if palette_len > 0 {
                let bytes = (palette_len * 64) as u64;
                self.ensure_palette_capacity(device, bytes);
                queue.write_buffer(
                    &self.palette_buffer,
                    0,
                    bytemuck::cast_slice(self.palette_scratch.as_slice()),
                );
                stats.upload_bytes += bytes;
                stats.buffer_writes += 1;
            }
        }

        let param_bytes = (self.param_scratch.len() * std::mem::size_of::<DrawParams>()) as u64;
        if param_bytes > 0 {
            self.upload_params(device, queue);
        }
        stats.upload_bytes += param_bytes;
        stats.buffer_writes += self.draw_writes;

        let alloc_uploaded = crate::perf::alloc_traffic();
        stats.cpu_alloc_bytes = alloc_uploaded.1.saturating_sub(alloc_start.1);
        stats.cpu_alloc_calls = alloc_uploaded.0.saturating_sub(alloc_start.0);

        self.eye = scene.eye.to_array();
        let (fog_rgb, fog_density, fog_start, fog_end) =
            self.fog.unwrap_or(([0.0, 0.0, 0.0], 0.0, 0.0, 1.0));
        let camera = CameraUniform {
            view_proj: scene.view_proj.to_cols_array_2d(),
            light: self.light_dir,
            sun: self.sun_light,
            ambient: self.ambient_light,
            day: self.day_light,
            fog: [fog_rgb[0], fog_rgb[1], fog_rgb[2], fog_density],
            fog_range: [fog_start, fog_end, 0.0, 0.0],
            eye: [self.eye[0], self.eye[1], self.eye[2], 1.0],
        };
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera));
        stats.upload_bytes += std::mem::size_of::<CameraUniform>() as u64;
        stats.buffer_writes += 1;

        self.skin_info.palette_buffer_size = self.palette_buffer.size();
        self.skin_info.palette_len = self.palette_scratch.len();
        self.skin_info.palette_bytes = (self.palette_scratch.len() * 64) as u64;
        self.skin_info.draw_buffer_size = self.draw_buffer.size();
        self.skin_info.draw_count = self.draw_scratch.len();
        self.skin_info.draw_grows = self.draw_grows;
        if collect_probes {
            self.probe_frames = 1;
        }

        stats.upload_micros += upload_started.elapsed().as_micros() as u64;
        let sort_started = std::time::Instant::now();

        // Sort the index list in place: group, priority plane, pipeline,
        // model, texture. The priority plane only matters inside the blended
        // group (opaque layers all write depth and are order-independent), but
        // sorting by it unconditionally keeps the key a single tuple.
        let len = self.draw_scratch.len();
        self.order_scratch.clear();
        self.order_scratch.resize(len, 0);
        {
            let order = self.order_scratch.as_mut_slice();
            for (slot, value) in order.iter_mut().enumerate() {
                *value = slot as u32;
            }
            let draws = self.draw_scratch.as_slice();
            order.sort_unstable_by_key(|&index| {
                let draw = draws[index as usize];
                (
                    draw.group,
                    draw.priority,
                    draw.pipeline,
                    draw.model,
                    draw.texture,
                )
            });
        }
        stats.sort_micros += sort_started.elapsed().as_micros() as u64;
        stats.prepare_micros += prepare_started.elapsed().as_micros() as u64;
    }

    /// Record one unit group into an existing pass.
    ///
    /// Groups: 0 = depth-writing opaque/alpha-test geometry, 1 = blended and
    /// additive layers, 2 = decals (selection rings). The caller decides what
    /// goes between them (terrain, water, doodads), which is what keeps the
    /// merged renderer's output identical to the pre-merge scene.
    pub fn draw_group(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        group: u8,
        stats: &mut Counters,
    ) {
        let draw_started = std::time::Instant::now();
        let gpu_skin = self.skin_mode == SkinMode::Gpu;
        pass.set_bind_group(0, &self.camera_bind, &[]);
        pass.set_bind_group(3, &self.palette_bind, &[]);
        let mut last_pipeline = u8::MAX;
        let mut last_texture = u32::MAX;
        // Stream id: model index, `u32::MAX` for the dynamic buffer.
        let mut last_stream: Option<u32> = None;
        let draws = self.draw_scratch.as_slice();
        for &draw_index in self.order_scratch.as_slice() {
            let draw = draws[draw_index as usize];
            if draw.index_count == 0 || draw.group != group {
                continue;
            }
            if last_stream != Some(draw.model) {
                if gpu_skin && draw.model != u32::MAX {
                    let geometry = &self.geometry[draw.model as usize];
                    pass.set_vertex_buffer(0, geometry.vertices.slice(..));
                    pass.set_index_buffer(geometry.indices.slice(..), wgpu::IndexFormat::Uint32);
                } else {
                    pass.set_vertex_buffer(0, self.vertices.buffer().slice(..));
                    pass.set_index_buffer(self.indices.buffer().slice(..), wgpu::IndexFormat::Uint32);
                }
                last_stream = Some(draw.model);
            }
            if draw.pipeline != last_pipeline {
                // Draws from the dynamic buffer carry pre-skinned vertices
                // (`vs_plain`) even when the renderer is in GPU mode: this is
                // how a five-matrix geoset is evaluated.
                let pipelines = if gpu_skin && draw.model != u32::MAX {
                    &self.pipelines_gpu
                } else {
                    &self.pipelines_cpu
                };
                pass.set_pipeline(&pipelines[draw.pipeline as usize]);
                last_pipeline = draw.pipeline;
                stats.pipeline_binds += 1;
            }
            if draw.texture != last_texture {
                if let Some(bind) = self.textures.bind_group(draw.texture as usize) {
                    pass.set_bind_group(1, bind, &[]);
                    last_texture = draw.texture;
                    stats.texture_binds += 1;
                }
            }
            pass.set_bind_group(2, &self.draw_bind, &[draw.params * self.draw_stride as u32]);
            pass.draw_indexed(
                draw.index_start..draw.index_start + draw.index_count,
                draw.index_base as i32,
                0..1,
            );
            stats.draws += 1;
            stats.indices += draw.index_count as u64;
            stats.vertices += draw.index_count as u64;
        }
        stats.draw_micros += draw_started.elapsed().as_micros() as u64;
    }

    /// Draw a pre-merged static batch (terrain mesh, doodads) with the canonical
    /// mesh pipelines and vertex layout.
    pub fn draw_static(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        batch: &StaticBatch,
        group: u8,
        stats: &mut Counters,
    ) {
        let gpu_skin = self.skin_mode == SkinMode::Gpu;
        let pipelines = if gpu_skin {
            &self.pipelines_gpu
        } else {
            &self.pipelines_cpu
        };
        pass.set_bind_group(0, &self.camera_bind, &[]);
        pass.set_bind_group(3, &self.palette_bind, &[]);
        pass.set_vertex_buffer(0, batch.vertices.slice(..));
        pass.set_index_buffer(batch.indices.slice(..), wgpu::IndexFormat::Uint32);
        let mut last_pipeline = u8::MAX;
        let mut last_texture = u32::MAX;
        for draw in &batch.draws {
            if draw.group != group || draw.index_count == 0 {
                continue;
            }
            if draw.pipeline != last_pipeline {
                pass.set_pipeline(&pipelines[draw.pipeline as usize]);
                last_pipeline = draw.pipeline;
                stats.pipeline_binds += 1;
            }
            if draw.texture != last_texture {
                if let Some(bind) = self.textures.bind_group(draw.texture as usize) {
                    pass.set_bind_group(1, bind, &[]);
                    last_texture = draw.texture;
                    stats.texture_binds += 1;
                }
            }
            pass.set_bind_group(2, &batch.params_bind, &[draw.params * batch.stride as u32]);
            pass.draw_indexed(
                draw.index_start..draw.index_start + draw.index_count,
                0,
                0..1,
            );
            stats.draws += 1;
            stats.indices += draw.index_count as u64;
            stats.vertices += draw.index_count as u64;
        }
    }

    /// Build a static batch: geometry that never changes (terrain, merged
    /// doodads). The vertex stream uses the canonical [`MeshVertex`] layout, so
    /// the batch shares the mesh pipelines and needs no per-frame upload.
    pub fn create_static_batch(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[MeshVertex],
        indices: &[u32],
        specs: &[StaticDrawSpec],
    ) -> StaticBatch {
        let vertex_buffer = upload_static(
            device,
            queue,
            bytemuck::cast_slice(vertices),
            wgpu::BufferUsages::VERTEX,
            "static-vertices",
        );
        let index_buffer = upload_static(
            device,
            queue,
            bytemuck::cast_slice(indices),
            wgpu::BufferUsages::INDEX,
            "static-indices",
        );
        let stride = self.draw_stride;
        let mut staging = vec![0u8; (specs.len().max(1)) * stride as usize];
        let mut draws = Vec::with_capacity(specs.len());
        for (slot, spec) in specs.iter().enumerate() {
            let params = DrawParams::new(Mat4::IDENTITY, spec.tint, spec.color_scale, 0)
                .with_layer(PipelineKind::alpha_ref(spec.blend), 0);
            let dst = slot * stride as usize;
            staging[dst..dst + std::mem::size_of::<DrawParams>()]
                .copy_from_slice(bytemuck::bytes_of(&params));
            draws.push(StaticDraw {
                group: spec.group,
                index_start: spec.index_start,
                index_count: spec.index_count,
                texture: spec.texture as u32,
                pipeline: PipelineKind::from_blend(spec.blend) as u8,
                params: slot as u32,
            });
        }
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("static-params"),
            size: staging.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&params_buffer, 0, &staging);
        let params_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("static-params-bind"),
            layout: &self.draw_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &params_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<DrawParams>() as u64),
                }),
            }],
        });
        // Same ordering rule as the per-frame draws: group, pipeline, texture,
        // then authoring order, so blending and depth state stay deterministic.
        draws.sort_unstable_by_key(|draw| (draw.group, draw.pipeline, draw.texture, draw.params));
        StaticBatch {
            vertices: vertex_buffer,
            indices: index_buffer,
            params_buffer,
            params_bind,
            stride,
            draws,
            index_count: indices.len() as u32,
        }
    }

    /// Convenience: prepare, open a world pass over `color`/`depth` and draw all
    /// three unit groups. Standalone callers (bench, headless tests) use this;
    /// the game owns its pass and calls [`prepare`](Self::prepare) directly.
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        depth: Option<&wgpu::TextureView>,
        scene: &Scene,
        mask: crate::scene::PassMask,
        stats: &mut Counters,
    ) {
        self.prepare(device, queue, scene, mask, stats);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("world-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
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
            depth_stencil_attachment: depth.map(|view| wgpu::RenderPassDepthStencilAttachment {
                view,
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
        stats.passes += 1;
        for group in 0..3u8 {
            self.draw_group(&mut pass, group, stats);
        }
    }

    /// Upload every draw's material state as one contiguous block.
    fn upload_params(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let stride = self.draw_stride as usize;
        let slot = std::mem::size_of::<DrawParams>();
        let slots = self.param_scratch.len();
        let needed = stride * slots;
        if (self.draw_buffer.size() as usize) < needed {
            let mut capacity = self.draw_buffer.size().max(4096);
            while capacity < (needed * 3 / 2) as u64 {
                capacity *= 2;
            }
            self.draw_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mesh-draw-ring"),
                size: capacity,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            // The dynamic-offset bind group must follow the new allocation, or
            // every later draw keeps reading the previous (frozen) buffer.
            self.draw_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mesh-draw-bind"),
                layout: &self.draw_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.draw_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(std::mem::size_of::<DrawParams>() as u64),
                    }),
                }],
            });
            self.draw_grows += 1;
        }
        self.param_staging.clear();
        self.param_staging.resize(needed, 0);
        {
            let params = self.param_scratch.as_slice();
            let staging = &mut self.param_staging;
            for (index, param) in params.iter().enumerate() {
                let dst = index * stride;
                staging[dst..dst + slot].copy_from_slice(bytemuck::bytes_of(param));
            }
        }
        queue.write_buffer(&self.draw_buffer, 0, &self.param_staging);
        self.draw_writes += 1;
    }

    fn ensure_palette_capacity(&mut self, device: &wgpu::Device, needed: u64) {
        if self.palette_buffer.size() < needed {
            let mut capacity = self.palette_buffer.size().max(256 * 1024);
            while capacity < needed * 3 / 2 {
                capacity *= 2;
            }
            self.palette_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mesh-palette"),
                size: capacity,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            // The bind group must follow the bigger buffer or every skinned
            // draw would read the old, too-small allocation.
            self.palette_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mesh-palette-bind"),
                layout: &self.palette_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.palette_buffer.as_entire_binding(),
                }],
            });
        }
    }
}

/// Depth format shared by the world pass and the renderer.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Z-up (MDX authoring) to Y-up (renderer) basis change.
fn z_up_to_y_up() -> Mat4 {
    // Columns: X stays, Z becomes Y, Y becomes -Z.
    Mat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, //
        0.0, 0.0, -1.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ])
}

/// Append the unit's bone matrices as `mat4x4<f32>` storage elements.
fn push_palette_f32(out: &mut Scratch<[f32; 16]>, nodes: &[Mat4], z_up: bool) {
    if z_up {
        let convert = z_up_to_y_up();
        for node in nodes {
            out.push(convert.mul_mat4(node).to_cols_array());
        }
    } else {
        for node in nodes {
            out.push(node.to_cols_array());
        }
    }
}

/// Append the unit's bone matrices as `Mat4`, optionally folding Z-up in.
fn push_palette(out: &mut Scratch<Mat4>, nodes: &[Mat4], z_up: bool) {
    if z_up {
        let convert = z_up_to_y_up();
        for node in nodes {
            out.push(convert.mul_mat4(node));
        }
    } else {
        out.extend_from_slice(nodes);
    }
}

/// Build the static vertex stream for one geoset (bind pose + bone weights).
fn push_static_vertices(out: &mut Vec<MeshVertex>, mesh: &MeshData) {
    for (index, position) in mesh.positions.iter().enumerate() {
        let normal = mesh.normals.get(index).copied().unwrap_or(Vec3::Y);
        let uv = mesh.uvs.get(index).copied().unwrap_or(Vec2::ZERO);
        let mut bones = [0u16; MAX_BONES_PER_VERTEX];
        let mut weights = [0u16; MAX_BONES_PER_VERTEX];
        if let Some(list) = mesh.bones.get(index) {
            let mut total = 0.0f32;
            for pair in list.iter().take(MAX_BONES_PER_VERTEX) {
                if pair.weight > 0.0 {
                    total += pair.weight;
                }
            }
            let total = if total > 0.0 { total } else { 1.0 };
            for (slot, pair) in list.iter().take(MAX_BONES_PER_VERTEX).enumerate() {
                bones[slot] = pair.bone;
                weights[slot] = ((pair.weight.max(0.0) / total) * u16::MAX as f32).round() as u16;
            }
        }
        // Skinned vertices keep raw Z-up positions: the palette already carries
        // the basis change. A vertex without bones has no palette entry, so the
        // conversion has to be baked here (it would otherwise be applied twice).
        let skinned = weights.iter().any(|weight| *weight > 0);
        let (position, normal) = if mesh.z_up && !skinned {
            (
                z_up_to_y_up().transform_point3(*position),
                z_up_to_y_up().transform_vector3(normal),
            )
        } else {
            (*position, normal)
        };
        let uv2 = mesh.uvs2.get(index).copied().unwrap_or(uv);
        out.push(MeshVertex {
            position: position.to_array(),
            uv: uv.to_array(),
            uv2: uv2.to_array(),
            normal: pack_normal(normal),
            color: [u16::MAX; 4],
            bones,
            weights,
        });
    }
}

/// CPU skinning reference path: model space, world applied by the shader.
fn skin_cpu_into(
    vertices: &mut Scratch<MeshVertex>,
    bones_scratch: &mut Scratch<Mat4>,
    unit: &crate::scene::UnitDraw,
    mesh: &MeshData,
) -> u32 {
        let base = vertices.len() as u32;
        bones_scratch.clear();
        push_palette(bones_scratch, &unit.nodes, mesh.z_up);
        let bones = bones_scratch.as_slice();
        let bone_count = bones.len();
        let convert = z_up_to_y_up();
        for (index, position) in mesh.positions.iter().enumerate() {
            let normal = mesh.normals.get(index).copied().unwrap_or(Vec3::Y);
            let empty: &[BoneWeight] = &[];
            let list = mesh.bones.get(index).map_or(empty, |list| list.as_slice());
            // A vertex is skinned only when at least one influence has weight and
            // a bone inside the palette; otherwise the bind pose is used and the
            // Z-up basis change has to be applied here (the palette already
            // carries it for skinned vertices).
            let skinned = list
                .iter()
                .any(|pair| pair.weight > 0.0 && (pair.bone as usize) < bone_count);
            let (mut local, mut world_normal) = wc3_anim::runtime::skin_vertex(
                *position,
                normal,
                list.iter().map(|pair| (pair.bone, pair.weight)),
                bones,
            );
            if !skinned {
                if mesh.z_up {
                    local = convert.transform_point3(*position);
                    world_normal = convert.transform_vector3(normal);
                } else {
                    local = *position;
                    world_normal = normal;
                }
            }
            let (position, normal) = (local, world_normal);
            let uv = mesh.uvs.get(index).copied().unwrap_or(Vec2::ZERO);
            let uv2 = mesh.uvs2.get(index).copied().unwrap_or(uv);
            vertices.push(MeshVertex {
                position: position.to_array(),
                uv: uv.to_array(),
                uv2: uv2.to_array(),
                normal: pack_normal(normal),
                color: [u16::MAX; 4],
                bones: [0; 4],
                weights: [0; 4],
            });
        }
        base
}

fn push_ring(
    ring: &Ring,
    vertex_scratch: &mut Scratch<MeshVertex>,
    index_scratch: &mut Scratch<u32>,
    draw_scratch: &mut Scratch<Draw>,
    param_scratch: &mut Scratch<DrawParams>,
) {
        const SEGMENTS: usize = 32;
        let base = vertex_scratch.len() as u32;
        let inner = ring.radius * 0.82;
        let outer = ring.radius;
        for segment in 0..SEGMENTS {
            let a0 = (segment as f32 / SEGMENTS as f32) * std::f32::consts::TAU;
            let a1 = ((segment + 1) as f32 / SEGMENTS as f32) * std::f32::consts::TAU;
            let p = |angle: f32, radius: f32| {
                [
                    ring.pos.x + angle.cos() * radius,
                    ring.pos.y,
                    ring.pos.z + angle.sin() * radius,
                ]
            };
            let i0 = p(a0, inner);
            let o0 = p(a0, outer);
            let o1 = p(a1, outer);
            let i1 = p(a1, inner);
            for position in [i0, o0, o1, i0, o1, i1] {
                vertex_scratch.push(MeshVertex {
                    position,
                    uv: [0.0, 0.0],
                    uv2: [0.0, 0.0],
                    normal: pack_normal(Vec3::Y),
                    color: [u16::MAX; 4],
                    bones: [0; 4],
                    weights: [0; 4],
                });
            }
        }
        let index_start = index_scratch.len() as u32;
        for index in 0..(SEGMENTS as u32 * 6) {
            index_scratch.push(base + index);
        }
        let params = param_scratch.len() as u32;
        param_scratch.push(DrawParams::identity(ring.color).with_layer(0.0, 0));
        draw_scratch.push(Draw {
            group: 2,
            priority: 0,
            model: u32::MAX,
            index_start,
            index_count: SEGMENTS as u32 * 6,
            index_base: 0,
            texture: 0,
            pipeline: PipelineKind::Blend as u8,
            params,
        });
    }

fn pack_normal(normal: Vec3) -> [i16; 4] {
    let normal = normal.normalize_or(Vec3::Y);
    [
        (normal.x.clamp(-1.0, 1.0) * 32767.0) as i16,
        (normal.y.clamp(-1.0, 1.0) * 32767.0) as i16,
        (normal.z.clamp(-1.0, 1.0) * 32767.0) as i16,
        0,
    ]
}

fn upload_static(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bytes: &[u8],
    usage: wgpu::BufferUsages,
    label: &str,
) -> wgpu::Buffer {
    let size = bytes.len().max(4) as u64;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if !bytes.is_empty() {
        queue.write_buffer(&buffer, 0, bytes);
    }
    buffer
}

fn bind_group_layout_uniform(
    device: &wgpu::Device,
    label: &str,
    dynamic: bool,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{UiBatch, UnitDraw};

    /// `render.uniform.camera-size` (lib half): the Rust uniform, the mesh
    /// shader's WGSL `Camera` and the 176-byte device contract must agree. Build
    /// 0f037ea2 aborted because a 112-byte buffer was bound to this struct.
    #[test]
    fn camera_uniform_layout_matches_wgsl() {
        assert_eq!(std::mem::size_of::<CameraUniform>(), 176);
        assert_eq!(CAMERA_UNIFORM_SIZE, 176);
        assert_eq!(camera_uniform_size_from_wgsl(), 176);
        // Field-by-field: mat4x4 = 64, seven vec4 = 112.
        let fields = [
            ("view_proj", "mat4x4<f32>", 64usize),
            ("light", "vec4<f32>", 16),
            ("sun", "vec4<f32>", 16),
            ("ambient", "vec4<f32>", 16),
            ("day", "vec4<f32>", 16),
            ("fog", "vec4<f32>", 16),
            ("fog_range", "vec4<f32>", 16),
            ("eye", "vec4<f32>", 16),
        ];
        assert_eq!(fields.iter().map(|field| field.2).sum::<usize>(), 176);
        assert_eq!(
            wgsl_struct_size(MESH_SHADER, "Camera"),
            fields.iter().map(|field| field.2).sum::<usize>()
        );
    }

    fn unit(nodes: Vec<Mat4>) -> UnitDraw {
        UnitDraw {
            model: 0,
            world: Mat4::from_translation(Vec3::new(100.0, 20.0, -40.0)),
            tint: [0.5, 0.7, 1.0, 1.0],
            nodes,
            mesh_alpha: vec![1.0],
            layer_alpha: vec![vec![1.0]],
            mesh_color: Vec::new(),
        }
    }

    fn mesh(positions: Vec<Vec3>, bones: Vec<Vec<BoneWeight>>, z_up: bool) -> MeshData {        MeshData {
            normals: vec![Vec3::Y; positions.len()],
            uvs: vec![Vec2::ZERO; positions.len()],
            uvs2: Vec::new(),
            indices: (0..positions.len() as u32).collect(),
            positions,
            bones,
            layers: vec![MeshLayer::plain(0)],
            z_up,
            hide: false,
        }
    }

    /// Reference skinning: weighted average of bone transforms in model space.
    fn reference_vertex(
        position: Vec3,
        normal: Vec3,
        list: &[BoneWeight],
        nodes: &[Mat4],
        z_up: bool,
    ) -> (Vec3, Vec3) {
        let mut local = Vec3::ZERO;
        let mut accumulated = Vec3::ZERO;
        let mut weight = 0.0f32;
        for pair in list {
            if let Some(node) = nodes.get(pair.bone as usize) {
                local += node.transform_point3(position) * pair.weight;
                accumulated += node.transform_vector3(normal) * pair.weight;
                weight += pair.weight;
            }
        }
        if weight > 0.0 {
            local /= weight;
            accumulated = accumulated.normalize_or_zero();
        } else {
            local = position;
            accumulated = normal;
        }
        if z_up {
            (
                Vec3::new(local.x, local.z, -local.y),
                Vec3::new(accumulated.x, accumulated.z, -accumulated.y),
            )
        } else {
            (local, accumulated)
        }
    }

    #[test]
    fn cpu_skin_uses_weighted_palette_in_model_space() {
        let nodes = vec![
            Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)),
            Mat4::from_rotation_z(0.5),
        ];
        let positions = vec![
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(-4.0, 0.5, 2.0),
            Vec3::new(0.0, 0.0, 0.0),
        ];
        let bones = vec![
            vec![BoneWeight {
                bone: 0,
                weight: 1.0,
            }],
            vec![BoneWeight {
                bone: 1,
                weight: 1.0,
            }],
            vec![
                BoneWeight {
                    bone: 0,
                    weight: 0.25,
                },
                BoneWeight {
                    bone: 1,
                    weight: 0.75,
                },
            ],
        ];
        let mut palette = Scratch::new();
        push_palette(&mut palette, &nodes, true);
        for (index, position) in positions.iter().enumerate() {
            let (expected, _) = reference_vertex(
                *position,
                Vec3::Y,
                &bones[index],
                &nodes,
                true,
            );
            // CPU path with the folded palette equals skinning then converting.
            let matrix_sum: Vec3 = bones[index]
                .iter()
                .map(|pair| {
                    palette.as_slice()[pair.bone as usize].transform_point3(*position) * pair.weight
                })
                .fold(Vec3::ZERO, |a, b| a + b);
            let total: f32 = bones[index].iter().map(|pair| pair.weight).sum();
            let actual = matrix_sum / total;
            assert!(
                (actual - expected).length() < 1.0e-4,
                "vertex {index}: {actual:?} vs {expected:?}"
            );
        }
    }

    #[test]
    fn cpu_skin_delegates_to_the_canonical_skin_vertex() {
        // Two vertices, Z-up authored: one skinned by bone 0, one with no bones
        // (must receive the Z-up basis change instead of staying Z-up).
        let nodes = vec![Mat4::from_translation(Vec3::new(5.0, 0.0, 0.0))];
        let positions = vec![Vec3::new(1.0, 2.0, 3.0), Vec3::new(0.0, 4.0, 0.0)];
        let bones = vec![
            vec![BoneWeight {
                bone: 0,
                weight: 1.0,
            }],
            Vec::new(),
        ];
        let data = mesh(positions.clone(), bones.clone(), true);
        let instance = unit(nodes.clone());
        let mut vertices = Scratch::new();
        let mut bone_scratch = Scratch::new();
        skin_cpu_into(&mut vertices, &mut bone_scratch, &instance, &data);
        assert_eq!(vertices.len(), 2);

        // Skinned vertex: canonical skin_vertex over the Z-up-folded palette.
        let mut palette = Scratch::new();
        push_palette(&mut palette, &nodes, true);
        let (expected, _) = wc3_anim::runtime::skin_vertex(
            positions[0],
            Vec3::Y,
            bones[0].iter().map(|pair| (pair.bone, pair.weight)),
            palette.as_slice(),
        );
        let actual = Vec3::from_array(vertices.as_slice()[0].position);
        assert!((actual - expected).length() < 1.0e-4, "{actual:?} vs {expected:?}");

        // Bone-less vertex: bind pose with the Z-up conversion applied.
        let expected = Vec3::new(0.0, 0.0, -4.0);
        let actual = Vec3::from_array(vertices.as_slice()[1].position);
        assert!((actual - expected).length() < 1.0e-4, "{actual:?} vs {expected:?}");
    }

    #[test]
    fn static_vertex_packs_weights_normalised() {
        let data = MeshData {
            positions: vec![Vec3::ZERO],
            normals: vec![Vec3::Y],
            uvs: vec![Vec2::ZERO],
            uvs2: Vec::new(),
            indices: vec![0],
            bones: vec![vec![
                BoneWeight {
                    bone: 3,
                    weight: 2.0,
                },
                BoneWeight {
                    bone: 7,
                    weight: 2.0,
                },
            ]],
            layers: vec![MeshLayer::plain(0)],
            z_up: false,
            hide: false,
        };
        let mut vertices = Vec::new();
        push_static_vertices(&mut vertices, &data);
        let vertex = vertices[0];
        assert_eq!(vertex.bones[..2], [3, 7]);
        let sum: u32 = vertex.weights[..2].iter().map(|w| *w as u32).sum();
        assert!((sum as i32 - u16::MAX as i32).abs() <= 2, "weights sum to 1: {sum}");
    }

    #[test]
    fn hidden_and_zero_alpha_geosets_are_skipped() {
        let data = mesh(vec![Vec3::ZERO], vec![Vec::new()], false);
        let mut hidden = data.clone();
        hidden.hide = true;
        assert!(hidden.hide);
        assert!(!data.hide);
        let scene = Scene {
            view_proj: Mat4::IDENTITY,
            eye: Vec3::ZERO,
            viewport: Vec2::new(1280.0, 720.0),
            units: vec![unit(Vec::new())],
            rings: Vec::new(),
            ui: UiBatch::default(),
        };
        assert_eq!(scene.units.len(), 1);
    }

    /// Two texture units (`TEXCOORDINDEX := unit`, r2 lane 405): the static
    /// vertex stream carries both authored sets and the per-layer draw selects
    /// one. A model with a single set reuses set 0 for unit 1.
    #[test]
    fn render_two_uv_sets() {
        let quad = |uvs2: Vec<Vec2>| MeshData {
            positions: vec![Vec3::ZERO, Vec3::X, Vec3::Z],
            normals: vec![Vec3::Y; 3],
            uvs: vec![Vec2::new(0.125, 0.25); 3],
            uvs2,
            indices: vec![0, 1, 2],
            bones: vec![Vec::new(); 3],
            layers: vec![MeshLayer::plain(0)],
            z_up: false,
            hide: false,
        };
        // Two authored sets: unit 0 keeps set 0, unit 1 gets set 1.
        let mut vertices = Vec::new();
        push_static_vertices(&mut vertices, &quad(vec![Vec2::new(0.75, 0.875); 3]));
        assert_eq!(vertices[0].uv, [0.125, 0.25], "TEXCOORD0 = uv_sets[0]");
        assert_eq!(vertices[0].uv2, [0.75, 0.875], "TEXCOORD1 = uv_sets[1]");
        // A single-set model reuses set 0 (the engine has no second set to bind).
        let mut single = Vec::new();
        push_static_vertices(&mut single, &quad(Vec::new()));
        assert_eq!(single[0].uv2, single[0].uv, "unit 1 falls back to set 0");
        // The draw selects the set per material layer.
        let params = DrawParams::new(Mat4::IDENTITY, [1.0; 4], 1.0, 0).with_layer(0.0, 1);
        assert_eq!(params.flags[3], 1.0, "layer 1 selects TEXCOORD1");
        let params = DrawParams::new(Mat4::IDENTITY, [1.0; 4], 1.0, 0).with_layer(0.0, 0);
        assert_eq!(params.flags[3], 0.0, "layer 0 selects TEXCOORD0");
        assert!(
            MESH_SHADER.contains("select(input.uv, input.uv2"),
            "the vertex shader must select the per-layer texture-coordinate set"
        );
        // Static batches (terrain, doodads) go through `MeshVertex::colored`,
        // which duplicates set 0 into set 1 so a two-layer material keeps a
        // valid attribute and applies no second transform.
        let colored = MeshVertex::colored([0.0; 3], Vec3::Y, [0.3, 0.7], [1.0; 4]);
        assert_eq!(colored.uv2, colored.uv, "static vertices reuse TEXCOORD0");
    }

    /// Engine alpha-test contract (r2 lane 405): `glAlphaFunc(GL_GEQUAL,
    /// ref/255)` with ref 192 for filter mode 1 and 4 for modes 2..5. The old
    /// renderer used a single `< 0.5` test for every mode.
    #[test]
    fn render_alpha_test_thresholds() {
        assert_eq!(PipelineKind::alpha_ref(0), 0.0, "mode 0 has no test");
        assert_eq!(PipelineKind::alpha_ref(1), 192.0 / 255.0, "mode 1 ref 192");
        for mode in 2..=5 {
            assert_eq!(
                PipelineKind::alpha_ref(mode),
                4.0 / 255.0,
                "mode {mode} ref 4"
            );
        }
        // GL_GEQUAL: a texel with alpha == ref passes, one just below fails.
        let passes = |alpha: f32, mode: u32| alpha >= PipelineKind::alpha_ref(mode);
        assert!(passes(192.0 / 255.0, 1));
        assert!(!passes(191.0 / 255.0, 1));
        assert!(passes(4.0 / 255.0, 2));
        assert!(!passes(3.0 / 255.0, 5));
        // Filter mode -> pipeline: mode 1 is blend+test (never the hard cutout).
        assert_eq!(PipelineKind::from_blend(0), PipelineKind::Opaque);
        assert_eq!(PipelineKind::from_blend(1), PipelineKind::AlphaTest);
        assert_eq!(PipelineKind::from_blend(2), PipelineKind::Blend);
        assert_eq!(PipelineKind::from_blend(3), PipelineKind::Additive);
        assert_eq!(PipelineKind::from_blend(4), PipelineKind::Modulate);
        assert_eq!(PipelineKind::from_blend(5), PipelineKind::Modulate2x);
        assert_eq!(
            PipelineKind::AlphaTest.group(),
            0,
            "mode 1 writes depth like the engine"
        );
    }

    /// Engine blend-factor table (r2 lane 405) for the five filter modes.
    #[test]
    fn render_blend_mode_table() {
        let state = |blend: u32| blend_state(blend).expect("blended mode");
        let check = |blend: u32, src, dst| {
            let state = state(blend);
            assert_eq!(state.color.src_factor, src, "mode {blend} src");
            assert_eq!(state.color.dst_factor, dst, "mode {blend} dst");
            assert_eq!(state.color.operation, wgpu::BlendOperation::Add);
        };
        check(
            1,
            wgpu::BlendFactor::SrcAlpha,
            wgpu::BlendFactor::OneMinusSrcAlpha,
        );
        check(
            2,
            wgpu::BlendFactor::SrcAlpha,
            wgpu::BlendFactor::OneMinusSrcAlpha,
        );
        check(3, wgpu::BlendFactor::SrcAlpha, wgpu::BlendFactor::One);
        check(4, wgpu::BlendFactor::Dst, wgpu::BlendFactor::Zero);
        check(5, wgpu::BlendFactor::Dst, wgpu::BlendFactor::Src);
        assert!(blend_state(0).is_none(), "mode 0 is opaque");
    }

    /// A classic matrix group is evaluated **whole**: five matrices produce five
    /// influences at equal weight, not four renormalised ones (`Archer` geoset 0
    /// is the corpus case; engine `fcn.6f032c50`).
    #[test]
    fn render_matrix_group_cap() {
        let geoset = wc3_formats::mdx::Geoset {
            vertices: vec![Vec3::ZERO; 3],
            normals: vec![Vec3::Z; 3],
            face_type_groups: Vec::new(),
            face_groups: Vec::new(),
            faces: vec![0, 1, 2],
            vertex_groups: vec![0, 0, 0],
            matrix_groups: vec![5],
            matrix_indices: vec![0, 1, 2, 3, 4],
            material_id: 0,
            selection_group: 0,
            selection_flags: 0,
            lod: 0,
            lod_name: String::new(),
            bounds: wc3_formats::mdx::Extent {
                radius: 0.0,
                min: Vec3::ZERO,
                max: Vec3::ZERO,
            },
            sequence_bounds: Vec::new(),
            uv_sets: Vec::new(),
            skin: Vec::new(),
        };
        let skin = wc3_anim::GeosetSkin::from_geoset(&geoset);
        assert_eq!(skin.max_influences, 5, "five-matrix group kept whole");
        for list in &skin.influences {
            assert_eq!(list.len(), 5, "every vertex evaluates all five matrices");
            let total: f32 = list.iter().map(|(_, weight)| *weight).sum();
            assert!((total - 1.0).abs() < 1e-4, "weights renormalised: {total}");
            for (index, (bone, weight)) in list.iter().enumerate() {
                assert_eq!(*bone as usize, index, "matrix order preserved");
                assert!((weight - 0.2).abs() < 1e-4, "equal weights: {weight}");
            }
        }
        // The GPU vertex carries four slots, so this geoset must be skinned on
        // the CPU (`prepare` routes wide groups to `skin_cpu_into`).
        assert_eq!(MAX_BONES_PER_VERTEX, 4);
        assert!(
            skin.max_influences > MAX_BONES_PER_VERTEX,
            "wide groups take the CPU path"
        );
    }

    #[test]
    fn vertex_layout_is_packed() {
        // position 12 + uv 8 + uv2 8 + normal 8 + colour 8 + bones 8 +
        // weights 8 = 60 bytes (two texture-coordinate sets, as the engine's
        // two texture units require).
        assert_eq!(std::mem::size_of::<MeshVertex>(), 60);
        assert_eq!(std::mem::size_of::<DrawParams>(), 96);
        assert_eq!(VERTEX_STRIDE, 60);
    }

    #[test]
    fn pack_normal_round_trips() {
        for normal in [
            Vec3::X,
            Vec3::Y,
            Vec3::Z,
            -Vec3::X,
            Vec3::new(0.3, -0.5, 0.81).normalize(),
        ] {
            let packed = pack_normal(normal);
            let unpacked = Vec3::new(
                packed[0] as f32 / 32767.0,
                packed[1] as f32 / 32767.0,
                packed[2] as f32 / 32767.0,
            )
            .normalize();
            assert!(unpacked.dot(normal) > 0.999);
        }
    }

    #[test]
    fn draw_params_carry_palette_base() {
        let params = DrawParams::new(Mat4::IDENTITY, [1.0; 4], 1.0, 42);
        assert_eq!(params.flags[1] as u32, 42);
        assert_eq!(params.flags[0], 1.0);
    }
}
