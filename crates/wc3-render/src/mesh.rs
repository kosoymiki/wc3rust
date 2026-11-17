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
    /// Contact term for the fragment shader: `x` = world Y of the model's lowest
    /// visible vertex, `y` = band height in world units, `z` = brightness at the
    /// base, `w` = 1 enables it. `[0, 0, 0, 0]` disables the term, which is what
    /// every caller that does not know its geometry (terrain, doodads, rings)
    /// keeps.
    pub contact: [f32; 4],
}

impl DrawParams {
    pub fn new(world: Mat4, tint: [f32; 4], color_scale: f32, palette_base: u32) -> Self {
        Self {
            world: world.to_cols_array_2d(),
            tint,
            // flags.z = engine alpha-test ref (`PipelineKind::alpha_ref`),
            // flags.w = texture-coordinate set (0 = TEXCOORD0, 1 = TEXCOORD1).
            flags: [color_scale, palette_base as f32, 0.0, 0.0],
            contact: [0.0; 4],
        }
    }

    /// Per-layer material state the engine carries in its texture stages.
    pub fn with_layer(mut self, alpha_ref: f32, uv_set: u32) -> Self {
        self.flags[2] = alpha_ref;
        self.flags[3] = uv_set.min(1) as f32;
        self
    }

    /// Attach the contact term: `base_y` and `band` are world-space (the model's
    /// lowest visible vertex and the band height), `strength` is the brightness
    /// the base is scaled to.
    pub fn with_contact(mut self, base_y: f32, band: f32, strength: f32) -> Self {
        self.contact = [base_y, band, strength.clamp(0.0, 1.0), 1.0];
        self
    }

    pub fn identity(tint: [f32; 4]) -> Self {
        Self::new(Mat4::IDENTITY, tint, 1.0, 0)
    }
}

/// Knee of the highlight shoulder in linear light.
///
/// 0.72 is above the confidence level of the mid-tones (a flat up-facing roof
/// panel at noon sits at 1.10 before the shoulder, 0.88 at dawn) and below the
/// DNC peak (1.6), so the shadow side and the wood keep the authored contrast and
/// only the sun-facing top of the range is compressed.
pub const LIGHT_KNEE: f32 = 0.72;

/// Which tonal model the mesh shader runs. `Shoulder` is the shipped default; the
/// other two exist for the A/B evidence frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightModel {
    /// Exponential highlight shoulder, DNC intensities untouched.
    Shoulder,
    /// Sun scaled so `ambient + sun == 1.0` (no shoulder).
    Fit,
    /// Pre-fix behaviour: raw intensities and a hard clamp.
    Legacy,
}

/// CPU mirror of the shader's `shoulder`, used for the uniforms and the tests.
pub fn shoulder_cpu(value: f32, knee: f32) -> f32 {
    if !(knee > 0.0 && knee < 1.0) {
        return value.clamp(0.0, 1.0);
    }
    let span = 1.0 - knee;
    let under = value.min(knee);
    let over = (value - knee).max(0.0);
    (under + span * (1.0 - (-over / span).exp())).clamp(0.0, 1.0)
}

/// `H2_WORLD_ART_LIGHT=shoulder|fit|legacy` selects the tonal model; the default is
/// [`LightModel::Shoulder`]. Cached once per process.
pub fn light_model() -> LightModel {
    static MODEL: std::sync::OnceLock<LightModel> = std::sync::OnceLock::new();
    *MODEL.get_or_init(|| {
        if legacy_light() {
            return LightModel::Legacy;
        }
        match std::env::var("H2_WORLD_ART_LIGHT").as_deref() {
            Ok("fit") => LightModel::Fit,
            Ok("legacy") => LightModel::Legacy,
            _ => LightModel::Shoulder,
        }
    })
}

/// Evidence switches for the world-art before/after capture (packet `B-world`).
///
/// `H2_WORLD_ART_LEGACY_LIGHT=1` restores the raw DNC intensities (no fit) and
/// `H2_WORLD_ART_LEGACY_CONTACT=1` drops the contact band and the ground shadow;
/// `H2_WORLD_ART_LEGACY=1` sets both. With them set this build renders exactly what
/// the base rendered before the flatness fix, so both frames come from one binary
/// and differ only by the renderer change under test. Cached once; normal runs
/// never set them.
fn legacy_switch(name: &'static str, include_legacy_all: bool) -> bool {
    fn read(name: &'static str) -> bool {
        matches!(
            std::env::var(name).as_deref(),
            Ok("1") | Ok("on") | Ok("true")
        )
    }
    let all = read("H2_WORLD_ART_LEGACY");
    read(name) || (include_legacy_all && all)
}

fn legacy_light() -> bool {
    static VALUE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *VALUE.get_or_init(|| legacy_switch("H2_WORLD_ART_LEGACY_LIGHT", true))
}

fn legacy_contact() -> bool {
    static VALUE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *VALUE.get_or_init(|| legacy_switch("H2_WORLD_ART_LEGACY_CONTACT", true))
}

/// Fit the DNC directional term into the range a fixed-function pipeline can
/// represent without clipping.
///
/// The shipped DNC *unit* light is calibrated for the terrain ambient level:
/// `sun_intensity 1.3` × `KLAC` at noon is 1.289 linear against an ambient of
/// 0.287/0.204/0.204, so `ambient + sun · N·L` passes 1.0 at `N·L = 0.55`. The
/// per-channel clamp in the shader then gives every surface within ~56° of the
/// sun the same white value, and a domed roof (`N·L ∈ 0.4..0.9` across its
/// panels) renders as one flat mass — measured on the Great Hall: 84 % of its
/// vertices clip, the roof band has 0.03 linear-luma spread.
///
/// The fit keeps the ambient floor untouched — that is the level the DNC author
/// authored the shadow side with — and scales the directional term by one scalar
/// until `ambient + sun == 1.0` on the brightest channel. One scalar for all
/// three channels preserves hue; nothing is brightened; the sun side gets its
/// gradient back instead of a plateau.
pub fn fit_directional(sun: [f32; 3], ambient: [f32; 3]) -> [f32; 3] {
    let mut scale = 1.0f32;
    for channel in 0..3 {
        let sun_channel = sun[channel];
        if sun_channel <= 0.0 {
            continue;
        }
        let headroom = (1.0 - ambient[channel]).max(0.0);
        scale = scale.min(headroom / sun_channel);
    }
    let scale = scale.clamp(0.0, 1.0);
    [sun[0] * scale, sun[1] * scale, sun[2] * scale]
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
        let ty = field
            .split_once(':')
            .map(|(_, ty)| ty.trim())
            .unwrap_or_else(|| panic!("WGSL member without a type: `{field}`"));
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
    // x = world Y of the model's lowest visible vertex, y = contact band height
    // in world units, z = brightness at the base, w = 1 enables the term.
    contact: vec4<f32>,
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

/// DNC light for this surface: sky/ground-shaped ambient + directional sun.
///
/// The DNC model is one constant ambient (`KLBC` × `ambient_intensity`) plus one
/// directional sun (`KLAC` × `light_intensity`), and the shader used to evaluate
/// `clamp(ambient + sun · saturate(N·L), 0, 1)`. `sun` arrives already fitted by
/// [`MeshRenderer::set_surface_light`] so the sum cannot pass 1.0; before that
/// fit it did, and every surface within ~56° of the sun clipped to the same
/// white value (39 % of the Great Hall's vertices, its whole roof).
///
/// A constant ambient is also what makes a model read as a cut-out: the roof
/// panel and the underside of the eaves used to get the same fill. The fill is
/// shaped here instead — an up-facing surface sees the whole ambient term, a
/// down-facing one sees [`GROUND_AMBIENT`] of it. That constant is the one
/// shading choice in this term; every colour still comes from the DNC.
const GROUND_AMBIENT: f32 = 0.55;

fn ambient_light(normal: vec3<f32>) -> vec3<f32> {
    let up = clamp(normal.y * 0.5 + 0.5, 0.0, 1.0);
    return camera.ambient.rgb * mix(GROUND_AMBIENT, 1.0, up);
}

/// Highlight shoulder: identity up to the knee, then an exponential approach to
/// 1.0. `camera.sun.w` carries the knee (0 disables it).
///
/// The shipped DNC intensities put `ambient + sun · N·L` at 1.6 on a surface
/// facing the sun, so a hard clamp flattens everything above `N·L ≈ 0.54` — the
/// whole domed roof of the Great Hall, one value. A shoulder keeps the shadow
/// side and the mid-tones exactly as authored (identity below the knee, so the
/// dark ribs keep their contrast) and turns the clipped top into a gradient that
/// still approaches full brightness instead of losing 25 % of it.
fn shoulder(value: vec3<f32>) -> vec3<f32> {
    let knee = camera.sun.w;
    if (knee <= 0.0 || knee >= 1.0) {
        return clamp(value, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    let knee3 = vec3<f32>(knee);
    let span = vec3<f32>(1.0 - knee);
    let under = min(value, knee3);
    let over = max(value - knee3, vec3<f32>(0.0));
    return clamp(under + span * (vec3<f32>(1.0) - exp(-over / span)), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn surface_light(normal: vec3<f32>) -> vec3<f32> {
    let n = normalize(normal);
    let l = normalize(camera.light.xyz);
    let ndl = clamp(dot(n, l), 0.0, 1.0);
    return shoulder(ambient_light(n) + camera.sun.rgb * ndl);
}

/// Contact darkening at the bottom of a model.
///
/// `draw.contact` is derived from the model's own bounding box by
/// [`MeshRenderer::prepare`]: a 4x4 building gets a band proportional to its
/// height, a unit gets a short one at its feet. It is a contact term, not baked
/// occlusion — it darkens the last few percent of the silhouette so the model
/// sits on the ground instead of hovering over it, which is what the shipped art
/// cannot carry on its own (MDX geosets have no per-vertex colour and no AO map).
fn contact_scale(world_y: f32) -> f32 {
    if (draw.contact.w <= 0.0) {
        return 1.0;
    }
    let t = clamp((world_y - draw.contact.x) / max(draw.contact.y, 0.001), 0.0, 1.0);
    let eased = t * t * (3.0 - 2.0 * t);
    return mix(draw.contact.z, 1.0, eased);
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
    let lit = c.rgb * tint * surface_light(input.normal) * contact_scale(input.world.y);
    return vec4<f32>(distance_haze(lit, input.world), c.a * input.color.a);
}

/// Ground-shadow decal. The alpha ramp is carried by the vertex colour (a fan
/// from the shadow centre fading to zero at the rim), so this entry ignores the
/// texture entirely: the draw binds no image of its own and sampling whatever the
/// shadow quad happened to reference would modulate its alpha with unrelated art.
@fragment
fn fs_shadow(input: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, input.color.a);
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
    let lit = c.rgb * tint * surface_light(input.normal) * contact_scale(input.world.y);
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
    /// Internal ground-shadow decal: no texture, vertex alpha, `SRC_ALPHA /
    /// ONE_MINUS_SRC_ALPHA`, no depth write. Never produced by
    /// [`Self::from_blend`]: only the renderer's own contact shadows use it.
    Shadow = 7,
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
    pub const COUNT: usize = 8;

    /// Draw group: 0 = depth-writing opaque/alpha-test, 1 = blended/additive,
    /// 2 = decals. The game interleaves terrain, water and doodads between these
    /// groups, so the group is part of the sort key and of the draw call.
    pub fn group(self) -> u8 {
        match self {
            Self::Opaque | Self::AlphaTest | Self::Cutout => 0,
            Self::Blend | Self::Additive | Self::Modulate | Self::Modulate2x => 1,
            Self::Shadow => 2,
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
    /// Bounds of the visible geometry in the Y-up model space the vertex shader
    /// produces (`0..0` for an empty model). Drives the contact band and the
    /// ground shadow.
    min: Vec3,
    max: Vec3,
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
            ("fs_shadow", blend_state(2), false),
        ];
        for (index, (entry, blend, depth_write)) in table.into_iter().enumerate() {
            let label = match index {
                0 => "mesh-opaque",
                1 => "mesh-alpha-test",
                2 => "mesh-blend",
                3 => "mesh-additive",
                4 => "mesh-modulate",
                5 => "mesh-modulate2x",
                6 => "mesh-cutout",
                _ => "mesh-shadow",
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
            let mut min = Vec3::splat(f32::MAX);
            let mut max = Vec3::splat(f32::MIN);
            for mesh in model {
                // The contact band and the ground shadow are authored from the
                // geometry itself: the bounds of the *visible* vertices in the
                // Y-up space the vertex shader ends in (MDX Z-up becomes Y, so a
                // model-space point is `(x, z, -y)`; `hide` helper planes are
                // ignored because they are never drawn and would stretch the
                // bounds across the whole model).
                if !mesh.hide {
                    for position in &mesh.positions {
                        let local = if mesh.z_up {
                            Vec3::new(position.x, position.z, -position.y)
                        } else {
                            *position
                        };
                        min = min.min(local);
                        max = max.max(local);
                    }
                }
                let base = vertices.len() as u32;
                push_static_vertices(&mut vertices, mesh);
                let start = indices.len() as u32;
                indices.extend(mesh.indices.iter().map(|index| base + index));
                geosets.push((start, mesh.indices.len() as u32));
            }
            let (min, max) = if min.x <= max.x {
                (min, max)
            } else {
                (Vec3::ZERO, Vec3::ZERO)
            };
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
                min,
                max,
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
    ///
    /// The directional term is fitted by [`fit_directional`] before it reaches the
    /// uniform: the shipped DNC intensities (`sun 1.3`, `ambient 0.3`) push
    /// `ambient + sun · N·L` past 1.0 over half the sphere, and the shader clamp
    /// then flattens every one of those surfaces to the same value. The fit keeps
    /// the ambient floor and scales the sun down so the sum peaks at 1.0, which is
    /// the calibration the DNC numbers were authored for (`daynight`: a flat
    /// up-facing surface lands on ~1.0 at noon).
    pub fn set_surface_light(&mut self, sun: [f32; 4], ambient: [f32; 4], flat: [f32; 4]) {
        // `sun.w` in the uniform is the shoulder knee; the caller's `w` (1.0 =
        // exposure placeholder) is not used. See `light_model` for the evidence
        // switch that reproduces the pre-fix render.
        let model = light_model();
        let (directional, knee) = match model {
            LightModel::Legacy => ([sun[0], sun[1], sun[2]], sun[3]),
            LightModel::Shoulder => ([sun[0], sun[1], sun[2]], LIGHT_KNEE),
            LightModel::Fit => {
                let fitted = fit_directional(
                    [sun[0], sun[1], sun[2]],
                    [ambient[0], ambient[1], ambient[2]],
                );
                ([fitted[0], fitted[1], fitted[2]], 0.0)
            }
        };
        self.sun_light = [directional[0], directional[1], directional[2], knee];
        self.ambient_light = ambient;
        // Diagnostics: the light of a flat up-facing surface (`N · L` =
        // `FLAT_NDL`) as the shader will evaluate it, not the raw curve sample.
        let flat_ndl = crate::daynight::FLAT_NDL;
        self.day_light = if model == LightModel::Legacy {
            flat
        } else {
            [
                shoulder_cpu(ambient[0] + directional[0] * flat_ndl, knee),
                shoulder_cpu(ambient[1] + directional[1] * flat_ndl, knee),
                shoulder_cpu(ambient[2] + directional[2] * flat_ndl, knee),
                flat[3],
            ]
        };
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
            let light_dir = self.light_dir;
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
                if !legacy_contact() {
                    push_ground_shadow(
                        vertex_scratch,
                        index_scratch,
                        draw_scratch,
                        param_scratch,
                        unit,
                        geometry,
                        light_dir,
                    );
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
                    // Contact band from this model's own bounding box, in world
                    // space so units standing on a cliff still darken at their
                    // feet. `CONTACT_BAND` is the share of the model height the
                    // term covers; `CONTACT_MIN_BAND` keeps a small unit readable.
                    let (contact_base, contact_band, contact_strength) = if legacy_contact() {
                        (0.0, 0.0, 0.0)
                    } else {
                        let (base, band) = contact_band(unit.world, geometry.min.y, geometry.max.y);
                        (base, band, CONTACT_STRENGTH)
                    };
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
                            DrawParams::new(unit.world, tint, 1.0, palette_base)
                                .with_layer(
                                    PipelineKind::alpha_ref(layer.blend),
                                    layer.uv_set,
                                )
                                .with_contact(contact_base, contact_band, contact_strength),
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

/// WC3 draws a soft shadow blob under every unit and building: it is engine
/// geometry (`Shadow` texture quad in the original), not model data — MDX geosets
/// carry no shadow, which is why the shipped Great Hall has none and read as
/// pasted onto the ground.
///
/// The blob is a triangle fan from the model's own footprint: the centre vertex
/// carries [`SHADOW_ALPHA`], the rim carries zero, so the alpha ramp is
/// interpolated per pixel and no shadow texture is needed. The fan is offset
/// away from the sun by a fraction of the model height, so the shadow sits on
/// the side the light does not reach, and lifted [`SHADOW_LIFT`] world units so it
/// does not z-fight with the terrain under the model.
fn push_ground_shadow(
    vertex_scratch: &mut Scratch<MeshVertex>,
    index_scratch: &mut Scratch<u32>,
    draw_scratch: &mut Scratch<Draw>,
    param_scratch: &mut Scratch<DrawParams>,
    unit: &crate::scene::UnitDraw,
    geometry: &ModelGeometry,
    light: [f32; 4],
) {
    const SEGMENTS: usize = 24;
    if geometry.max.y <= geometry.min.y {
        return;
    }
    let scale = Vec3::new(
        unit.world.transform_vector3(Vec3::X).length(),
        unit.world.transform_vector3(Vec3::Y).length(),
        unit.world.transform_vector3(Vec3::Z).length(),
    );
    let half_x = (geometry.max.x - geometry.min.x).max(0.0) * 0.5 * scale.x;
    let half_z = (geometry.max.z - geometry.min.z).max(0.0) * 0.5 * scale.z;
    if half_x <= 0.0 || half_z <= 0.0 {
        return;
    }
    let height = (geometry.max.y - geometry.min.y) * scale.y;
    let centre_local = Vec3::new(
        (geometry.min.x + geometry.max.x) * 0.5,
        geometry.min.y,
        (geometry.min.z + geometry.max.z) * 0.5,
    );
    let mut centre = unit.world.transform_point3(centre_local);
    // Away from the sun: `light` points *towards* it, so the shadow goes the
    // other way. A high sun casts a short shadow, a low sun a long one.
    let sun = Vec3::new(light[0], light[1], light[2]);
    let flat = Vec3::new(sun.x, 0.0, sun.z).normalize_or_zero();
    centre -= flat * (height * SHADOW_OFFSET);
    centre.y += SHADOW_LIFT;
    let radius = Vec3::new(
        half_x * (1.0 + SHADOW_SPREAD),
        0.0,
        half_z * (1.0 + SHADOW_SPREAD),
    );
    // Three-ring profile: opaque core (hidden under the model anyway), a plateau
    // ring out to `SHADOW_PLATEAU` of the radius, then a fade to zero. A plain
    // centre-to-rim fan has zero alpha exactly where the blob becomes visible
    // outside the footprint, which is why the first version measured as "no
    // shadow" on the frame.
    let base = vertex_scratch.len() as u32;
    let mut push = |offset: Vec3, alpha: f32| {
        vertex_scratch.push(MeshVertex::colored(
            [centre.x + offset.x, centre.y, centre.z + offset.z],
            Vec3::Y,
            [0.5, 0.5],
            [0.0, 0.0, 0.0, alpha],
        ));
    };
    push(Vec3::ZERO, SHADOW_ALPHA);
    for ring in [SHADOW_PLATEAU, 1.0] {
        let alpha = if ring < 1.0 {
            SHADOW_ALPHA * 0.82
        } else {
            0.0
        };
        for segment in 0..=SEGMENTS {
            let angle = (segment as f32 / SEGMENTS as f32) * std::f32::consts::TAU;
            push(
                Vec3::new(
                    angle.cos() * radius.x * ring,
                    0.0,
                    angle.sin() * radius.z * ring,
                ),
                alpha,
            );
        }
    }
    let index_start = index_scratch.len() as u32;
    let ring1 = base + 1;
    let ring2 = ring1 + SEGMENTS as u32 + 1;
    for segment in 0..SEGMENTS as u32 {
        index_scratch.push(base);
        index_scratch.push(ring1 + segment);
        index_scratch.push(ring1 + segment + 1);
        let (a, b) = (ring1 + segment, ring1 + segment + 1);
        let (c, d) = (ring2 + segment, ring2 + segment + 1);
        index_scratch.push(a);
        index_scratch.push(c);
        index_scratch.push(d);
        index_scratch.push(a);
        index_scratch.push(d);
        index_scratch.push(b);
    }
    let params = param_scratch.len() as u32;
    param_scratch.push(DrawParams::identity([1.0, 1.0, 1.0, 1.0]));
    draw_scratch.push(Draw {
        group: 2,
        priority: 0,
        model: u32::MAX,
        index_start,
        index_count: SEGMENTS as u32 * 9,
        index_base: 0,
        // `fs_shadow` never samples the texture.
        texture: 0,
        pipeline: PipelineKind::Shadow as u8,
        params,
    });
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

/// Share of the model height the contact term covers, and the brightness the
/// lowest sliver of the silhouette is scaled to.
///
/// 8 % of the height is the depth of a real contact shadow on the ground plane at
/// the RTS camera angle (~50°); 0.55 keeps the base readable in daylight instead
/// of turning it into a black slab (WC3's own units use a ~0.6 shadow blob).
const CONTACT_BAND: f32 = 0.08;
const CONTACT_STRENGTH: f32 = 0.55;
/// World units of contact band a model gets at minimum, so a foot-sized mesh
/// still darkens where it meets the ground.
const CONTACT_MIN_BAND: f32 = 6.0;

/// Ground-shadow shape: alpha at the centre of the blob, the share of the model
/// height the shadow is pushed away from the sun, the world-unit lift above the
/// model's base plane (prevents z-fighting with the terrain) and how far the blob
/// spreads beyond the footprint.
const SHADOW_ALPHA: f32 = 0.40;
const SHADOW_OFFSET: f32 = 0.22;
const SHADOW_LIFT: f32 = 5.0;
const SHADOW_SPREAD: f32 = 0.22;
const SHADOW_PLATEAU: f32 = 0.62;

/// World-space contact band for one unit: `(base_y, band_height)`.
///
/// The model's `min_y`/`max_y` are Y-up model space (the vertex shader's space
/// before `draw.world`). A unit is placed by translation + Y rotation, so the
/// transformed lowest point is the ground contact and the Y scale is the length of
/// the transformed basis vector.
fn contact_band(world: Mat4, min_y: f32, max_y: f32) -> (f32, f32) {
    let scale_y = world.transform_vector3(Vec3::Y).length();
    let base_y = world.transform_point3(Vec3::new(0.0, min_y, 0.0)).y;
    let height = (max_y - min_y).max(0.0) * scale_y;
    let band = (height * CONTACT_BAND).max(CONTACT_MIN_BAND);
    (base_y, band)
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
        // Per-draw uniform: world 64 + tint 16 + flags 16 + contact 16 = 112,
        // which is also what the WGSL `Draw` struct costs (`draw_uniform_matches_wgsl`).
        assert_eq!(std::mem::size_of::<DrawParams>(), 112);
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

    /// The per-draw uniform grew a contact block; it must still match the WGSL
    /// `Draw` struct byte for byte, or the vertex shader reads a stale world
    /// matrix and every model collapses to the origin.
    #[test]
    fn draw_uniform_matches_wgsl() {
        assert_eq!(std::mem::size_of::<DrawParams>(), 112);
        assert_eq!(wgsl_struct_size(MESH_SHADER, "Draw"), 112);
        let params = DrawParams::identity([1.0; 4])
            .with_contact(-12.5, 40.0, 0.55);
        assert_eq!(params.contact, [-12.5, 40.0, 0.55, 1.0]);
        // Callers that never set a band keep the term off.
        assert_eq!(DrawParams::identity([1.0; 4]).contact, [0.0; 4]);
    }

    /// `fit_directional`: the DNC sun is scaled so `ambient + sun == 1.0` on the
    /// brightest channel, hand-computed from the shipped unit-light numbers.
    #[test]
    fn fit_directional_normalises_the_dnc_unit_light() {
        // Lordaeron unit DNC at noon: KLAC white x intensity 1.3, KLBC x 0.3.
        let sun = [1.3, 1.3, 1.3];
        let ambient = [0.3, 0.3, 0.3];
        let fitted = fit_directional(sun, ambient);
        // headroom = 1 - 0.3 = 0.7; scale = 0.7 / 1.3 = 0.5384615.
        for channel in 0..3 {
            assert!(
                (fitted[channel] - 0.7).abs() < 1e-6,
                "sun[{channel}] = {}",
                fitted[channel]
            );
        }
        // A flat, up-facing surface lands on the DNC calibration value:
        // 0.3 + 0.7 * 0.615662 = 0.7309634 (it was clipped to 1.0 before).
        let flat = 0.3 + fitted[0] * crate::daynight::FLAT_NDL;
        assert!((flat - 0.730_963_4).abs() < 1e-5, "flat = {flat}");
        // Nothing on the sphere can clip any more.
        let peak = 0.3 + fitted[0];
        assert!((peak - 1.0).abs() < 1e-6, "peak = {peak}");
    }

    /// The fit never brightens a channel, never darkens the ambient floor, and
    /// keeps one scalar for all three channels (hue is preserved).
    #[test]
    fn fit_directional_keeps_hue_and_headroom() {
        let sun = [1.0, 2.0, 4.0];
        let ambient = [0.5, 0.2, 0.0];
        let fitted = fit_directional(sun, ambient);
        // Headroom ratios: 0.5/1, 0.8/2, 1.0/4 -> the tightest is 0.25.
        assert!((fitted[0] - 0.25).abs() < 1e-6);
        assert!((fitted[1] - 0.50).abs() < 1e-6);
        assert!((fitted[2] - 1.00).abs() < 1e-6);
        for channel in 0..3 {
            assert!(fitted[channel] <= sun[channel]);
            assert!(ambient[channel] + fitted[channel] <= 1.0 + 1e-6);
        }
        // One scalar: the channel ratios of the sun survive.
        assert!(((fitted[1] / fitted[0]) - (sun[1] / sun[0])).abs() < 1e-6);
        assert!(((fitted[2] / fitted[0]) - (sun[2] / sun[0])).abs() < 1e-6);
    }

    #[test]
    fn fit_directional_handles_degenerate_lights() {
        // Night lights are already inside 1.0: untouched.
        assert_eq!(
            fit_directional([0.6, 0.4, 0.2], [0.1, 0.1, 0.1]),
            [0.6, 0.4, 0.2]
        );
        // A light with no directional term stays ambient-only.
        assert_eq!(fit_directional([0.0; 3], [0.4; 3]), [0.0; 3]);
        // The identity/neutral diagnostic light is full ambient: flat white stays
        // flat white (the fit cannot add a gradient where the caller asked for
        // none).
        assert_eq!(fit_directional([1.0; 3], [1.0; 3]), [0.0; 3]);
    }

    /// CPU mirror of the fragment shader's light, used by the tests below.
    fn surface_light_cpu(
        sun: [f32; 3],
        ambient: [f32; 3],
        normal: Vec3,
        light: Vec3,
        knee: f32,
    ) -> [f32; 3] {
        const GROUND_AMBIENT: f32 = 0.55;
        let n = normal.normalize_or(Vec3::Y);
        let ndl = n.dot(light.normalize_or(Vec3::Y)).clamp(0.0, 1.0);
        let up = (n.y * 0.5 + 0.5).clamp(0.0, 1.0);
        let ambient_scale = GROUND_AMBIENT + (1.0 - GROUND_AMBIENT) * up;
        let mut out = [0.0f32; 3];
        for channel in 0..3 {
            out[channel] = shoulder_cpu(ambient[channel] * ambient_scale + sun[channel] * ndl, knee);
        }
        out
    }

    #[test]
    fn shoulder_is_identity_below_the_knee_and_approaches_one() {
        for value in [0.0f32, 0.1, 0.3, 0.5, LIGHT_KNEE] {
            assert!((shoulder_cpu(value, LIGHT_KNEE) - value).abs() < 1e-6, "value {value}");
        }
        // Above the knee it keeps rising but never reaches 1.0 (f32 rounds the
        // asymptote to 1.0 past ~3, so the sweep stops inside the useful range).
        let mut previous = LIGHT_KNEE;
        for value in [0.8f32, 1.0, 1.2, 1.6, 2.0, 2.6] {
            let mapped = shoulder_cpu(value, LIGHT_KNEE);
            assert!(mapped > previous, "{value} -> {mapped}");
            assert!(mapped < 1.0, "{value} -> {mapped}");
            previous = mapped;
        }
        // The DNC peak (1.6) lands just under full brightness, not at 0.62.
        assert!((shoulder_cpu(1.6, LIGHT_KNEE) - 0.987).abs() < 2e-3);
        // A disabled knee is the old hard clamp.
        assert_eq!(shoulder_cpu(1.6, 0.0), 1.0);
        assert_eq!(shoulder_cpu(1.6, 1.0), 1.0);
    }

    /// Hand-computed values of the shader's light for the four surfaces that make
    /// the Great Hall read flat or not: the roof (up), the sun side, the shadow
    /// side and a downward face under the eaves.
    #[test]
    fn surface_light_values_are_hand_computable() {
        // Shipped Lordaeron unit DNC at noon, as `DayNightCurve::sample` delivers
        // it: the per-term white balance normalises the tracks to exactly 1.0, so
        // `sun = sun_intensity` and `ambient = ambient_intensity`.
        let sun = [1.3, 1.3, 1.3];
        let ambient = [0.3, 0.3, 0.3];
        let knee = LIGHT_KNEE;
        let light = Vec3::from_array([-0.675_456, 0.615_662, 0.405_856]);
        // Roof, facing up: raw 0.3 + 1.3 * 0.615662 = 1.100361, shoulder -> 0.9279.
        let up = surface_light_cpu(sun, ambient, Vec3::Y, light, knee);
        assert!((up[0] - 0.928_022).abs() < 1e-4, "up = {up:?}");
        // Straight at the sun: raw 0.274057 + 1.3 = 1.574057, shoulder -> 0.9867.
        let toward = surface_light_cpu(sun, ambient, light, light, knee);
        assert!((toward[0] - 0.986_731).abs() < 1e-4, "toward = {toward:?}");
        // Facing away from the sun: ambient only, below the knee, untouched.
        let away = surface_light_cpu(sun, ambient, -light, light, knee);
        assert!((away[0] - 0.190_943).abs() < 1e-4, "away = {away:?}");
        // Straight down (under the eaves): no sun, ground-weighted ambient.
        let down = surface_light_cpu(sun, ambient, -Vec3::Y, light, knee);
        assert!((down[0] - 0.165).abs() < 1e-6, "down = {down:?}");
        // Form: brightness rises with N·L and nothing saturates (see
        // `roof_band_is_not_a_plateau` for the whole-band proof).
        let mut previous = 0.0;
        for step in 0..=10 {
            let ndl = step as f32 / 10.0;
            let value = shoulder_cpu(ambient[0] + sun[0] * ndl, knee);
            assert!(value > previous || step == 0, "ndl {ndl}: {value} <= {previous}");
            previous = value;
        }
        assert!(toward[0] > up[0] && up[0] > away[0]);
    }

    /// The visible face of the Great Hall (everything with `n.y > 0.6`, i.e. the
    /// roof) used to collapse onto a handful of values because the light clipped;
    /// with the shoulder every normal gets its own value again.
    #[test]
    fn roof_band_is_not_a_plateau() {
        let sun = [1.3, 1.3, 1.3];
        let ambient = [0.3, 0.3, 0.3];
        let light = Vec3::new(-0.675_456, 0.615_662, 0.405_856).normalize();
        let mut legacy_values: std::collections::BTreeMap<u32, usize> =
            std::collections::BTreeMap::new();
        let mut shoulder_values: std::collections::BTreeMap<u32, usize> =
            std::collections::BTreeMap::new();
        let mut up_facing = 0usize;
        let mut legacy_clipped = 0usize;
        let mut shoulder_clipped = 0usize;
        for yaw in 0..72 {
            for pitch in 0..24 {
                let angle = yaw as f32 / 72.0 * std::f32::consts::TAU;
                // Stay off the exact pole: every azimuth there is the same
                // normal, which would look like a plateau of its own.
                let elevation = 0.62 + 0.33 * (pitch as f32 / 23.0);
                let normal = Vec3::new(
                    angle.cos() * (1.0 - elevation * elevation).sqrt(),
                    elevation,
                    angle.sin() * (1.0 - elevation * elevation).sqrt(),
                );
                if normal.y <= 0.6 {
                    continue;
                }
                up_facing += 1;
                let ndl = normal.dot(light).clamp(0.0, 1.0);
                // Legacy: constant ambient, raw sun, hard clamp.
                let raw = ambient[0] + sun[0] * ndl;
                if raw > 1.0 {
                    legacy_clipped += 1;
                }
                *legacy_values
                    .entry((raw.clamp(0.0, 1.0) * 10_000.0) as u32)
                    .or_insert(0) += 1;
                // Shipped: shaped ambient + shoulder. "Clipped" now means the
                // shader's own guard was reached, i.e. the tonal curve saturated —
                // the shoulder asymptote keeps every one of these below it.
                let shaped = surface_light_cpu(sun, ambient, normal, light, LIGHT_KNEE)[0];
                if shaped >= 1.0 {
                    shoulder_clipped += 1;
                }
                *shoulder_values
                    .entry((shaped * 10_000.0) as u32)
                    .or_insert(0) += 1;
            }
        }
        assert!(up_facing > 500, "sample grid too small: {up_facing}");
        let legacy_clipped = legacy_clipped as f32 / up_facing as f32;
        assert!(
            legacy_clipped > 0.30,
            "the old light clipped {legacy_clipped:.3} of the roof band"
        );
        assert_eq!(shoulder_clipped, 0, "the shoulder must not clip");
        // A plateau is one value shared by a large share of the band: the clipped
        // legacy light puts almost half of it on exactly 1.0, the shoulder puts
        // every normal on its own value (only quantisation groups them).
        let peak = |values: &std::collections::BTreeMap<u32, usize>| {
            values.values().copied().max().unwrap_or(0)
        };
        let legacy_peak = peak(&legacy_values);
        let shoulder_peak = peak(&shoulder_values);
        assert!(
            legacy_peak * 20 > up_facing,
            "the old light should flatten a large share of the roof: {legacy_peak}/{up_facing}"
        );
        assert!(
            shoulder_peak * 20 <= up_facing,
            "the shoulder still flattens {shoulder_peak}/{up_facing} samples onto one value"
        );
    }

    /// Nothing in the light path can clip any more: the shoulder's image of the
    /// whole unit sphere stays strictly inside `[0, 1]` and the sun side still
    /// reaches nearly full brightness.
    #[test]
    fn shoulder_light_never_clips() {
        let sun = [1.3, 1.3, 1.3];
        let ambient = [0.3, 0.3, 0.3];
        let light = Vec3::new(-0.675_456, 0.615_662, 0.405_856).normalize();
        let mut highest = 0.0f32;
        for yaw in 0..64 {
            for pitch in 0..32 {
                let angle = yaw as f32 / 64.0 * std::f32::consts::TAU;
                let elevation = (pitch as f32 / 31.0 - 0.5) * std::f32::consts::PI;
                let normal = Vec3::new(
                    angle.cos() * elevation.cos(),
                    elevation.sin(),
                    angle.sin() * elevation.cos(),
                );
                let value = surface_light_cpu(sun, ambient, normal, light, LIGHT_KNEE);
                for channel in value {
                    highest = highest.max(channel);
                }
            }
        }
        assert!(highest < 1.0, "highest = {highest}");
        assert!(highest > 0.95, "the lit side must still reach full light");
    }

    /// Contact band from real geometry: a 4x4 building gets a band proportional to
    /// its height, a foot-sized mesh gets the floor.
    #[test]
    fn contact_band_comes_from_the_model_bounds() {
        let world = Mat4::from_translation(Vec3::new(100.0, 617.7, -40.0));
        // 380 model units tall (the Great Hall's visible bounds), scale 2.
        let scaled = world * Mat4::from_scale(Vec3::splat(2.0));
        let (base, band) = contact_band(scaled, -190.0, 190.0);
        assert!((base - (617.7 - 380.0)).abs() < 1e-3, "base = {base}");
        assert!((band - 60.8).abs() < 1e-3, "band = {band}");
        // A 30-unit mesh keeps the 6-unit floor, not a 2.4-unit band.
        let (_, small) = contact_band(Mat4::IDENTITY, 0.0, 30.0);
        assert!((small - CONTACT_MIN_BAND).abs() < 1e-6);
        // Empty geometry cannot produce a band.
        let (_, empty) = contact_band(Mat4::IDENTITY, 0.0, 0.0);
        assert!((empty - CONTACT_MIN_BAND).abs() < 1e-6);
    }

    /// The shader must actually call both new terms; a refactor that deletes the
    /// call keeps the tests above green but ships the old flat look.
    #[test]
    fn shader_calls_light_and_contact_terms() {
        assert!(
            MESH_SHADER.contains("let lit = c.rgb * tint * surface_light(input.normal) * contact_scale(input.world.y);"),
            "fs_main must apply the contact term"
        );
        assert!(
            MESH_SHADER.contains("ambient_light(n) + camera.sun.rgb * ndl"),
            "surface_light must use the shaped ambient"
        );
        assert!(
            MESH_SHADER.contains("const GROUND_AMBIENT: f32 = 0.55;"),
            "the ambient split constant must match the CPU mirror"
        );
    }
}

/// World-art evidence harness (ledger 481, packet `B-world`).
///
/// These tests own the flatness investigation for the melee world: they read the
/// shipped `GreatHall.mdx` through the game asset layer, evaluate the *same*
/// light math the mesh shader runs on the real vertex normals, and render the
/// real map frame offscreen. Diagnostics print `WORLD_ART …` lines so a report
/// can quote numbers instead of adjectives.
#[cfg(test)]
mod world_art {
    use super::*;

    /// Installed archives: the phone install first, the dev iso second.
    fn install() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
        let mpq = [
            "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
            "/data/data/org.wc3rust.p0/files/War3.mpq",
            "/data/data/com.termux/files/home/src/wc3rust/analysis/war3_iso/War3.mpq",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|path| path.is_file())?;
        let map = [
            "/data/data/com.termux/files/home/src/wc3rust/donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
            "/data/data/com.termux/files/home/src/wc3rust/analysis/war3_map/Maps/Melee.w3x",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|path| path.is_file())?;
        Some((mpq, map))
    }

    /// Evidence directory (the world-art agent's owner path).
    fn out_dir() -> std::path::PathBuf {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/agent-world-art");
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    fn luma(rgb: [f32; 3]) -> f32 {
        0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
    }

    /// Histogram percentile of a sorted slice.
    fn percentile(sorted: &[f32], q: f32) -> f32 {
        if sorted.is_empty() {
            return 0.0;
        }
        let index = ((sorted.len() - 1) as f32 * q.clamp(0.0, 1.0)).round() as usize;
        sorted[index]
    }

    /// Per-mesh light summary: how much of one geoset the clamp flattens.
    fn mesh_light_line(
        index: usize,
        mesh: &wc3_game::assets::MeshData,
        sun: [f32; 3],
        ambient: [f32; 3],
        dir: Vec3,
    ) -> String {
        let convert = z_up_to_y_up();
        let normals = mesh.expanded().normals;
        let fitted = fit_directional(sun, ambient);
        let mut clamped = 0usize;
        let mut ndl_sum = 0.0f32;
        let mut luma_before = Vec::new();
        let mut luma_after = Vec::new();
        for normal in &normals {
            let n = if mesh.z_up {
                convert.transform_vector3(*normal)
            } else {
                *normal
            }
            .normalize_or(Vec3::Y);
            let ndl = n.dot(dir).clamp(0.0, 1.0);
            ndl_sum += ndl;
            let mut before = [0.0f32; 3];
            let mut after = [0.0f32; 3];
            let mut clipped = false;
            for channel in 0..3 {
                let raw = ambient[channel] + sun[channel] * ndl;
                clipped |= raw > 1.0;
                before[channel] = raw.clamp(0.0, 1.0);
                after[channel] = (ambient[channel] + fitted[channel] * ndl).clamp(0.0, 1.0);
            }
            if clipped {
                clamped += 1;
            }
            luma_before.push(luma(before));
            luma_after.push(luma(after));
        }
        let count = normals.len().max(1) as f32;
        luma_before.sort_by(|a, b| a.partial_cmp(b).unwrap());
        luma_after.sort_by(|a, b| a.partial_cmp(b).unwrap());
        format!(
            "WORLD_ART mesh#{index:<2} verts={:<5} tex={:<40} ndl={:.3} clipped={:.3} spread_before={:.3} spread_after={:.3}",
            normals.len(),
            mesh.texture_paths
                .first()
                .map(|path| path.rsplit('\\').next().unwrap_or(path).to_string())
                .unwrap_or_else(|| "-".to_string()),
            ndl_sum / count,
            clamped as f32 / count,
            percentile(&luma_before, 0.95) - percentile(&luma_before, 0.05),
            percentile(&luma_after, 0.95) - percentile(&luma_after, 0.05),
        )
    }

    /// Vertex-level light distribution of one model.
    ///
    /// Uses the shipped normals (or the computed fallback), converts the model's
    /// Z-up basis the way the renderer does, and evaluates the shader's
    /// `surface_light` at the current and at the new (fitted) DNC light.
    struct LightReport {
        vertices: usize,
        y_only: usize,
        clamped_before: f32,
        clamped_after: f32,
        spread_before: f32,
        spread_after: f32,
        mean_before: f32,
        mean_after: f32,
        ndl_bins: [usize; 10],
        up_facing: f32,
    }

    fn light_report(
        models: &[(Vec<Vec3>, bool)],
        sun: [f32; 3],
        ambient: [f32; 3],
        dir: Vec3,
    ) -> LightReport {
        let convert = z_up_to_y_up();
        let dir = dir.normalize();
        let fitted = fit_directional(sun, ambient);
        let mut luma_before = Vec::new();
        let mut luma_after = Vec::new();
        let mut ndl_bins = [0usize; 10];
        let mut vertices = 0usize;
        let mut y_only = 0usize;
        let mut clamped_before = 0usize;
        let mut clamped_after = 0usize;
        let mut up_facing = 0usize;
        for (normals, z_up) in models {
            for normal in normals {
                let n = if *z_up {
                    convert.transform_vector3(*normal)
                } else {
                    *normal
                }
                .normalize_or(Vec3::Y);
                let ndl = n.dot(dir).clamp(0.0, 1.0);
                let bin = ((ndl * 9.999) as usize).min(9);
                ndl_bins[bin] += 1;
                vertices += 1;
                if (n.y - 1.0).abs() < 0.005 {
                    y_only += 1;
                }
                if n.y > 0.7 {
                    up_facing += 1;
                }
                let mut before = [0.0f32; 3];
                let mut after = [0.0f32; 3];
                let mut before_clipped = false;
                let mut after_clipped = false;
                for channel in 0..3 {
                    let raw = ambient[channel] + sun[channel] * ndl;
                    before[channel] = raw.clamp(0.0, 1.0);
                    before_clipped |= raw > 1.0;
                    let raw_after = ambient[channel] + fitted[channel] * ndl;
                    after[channel] = raw_after.clamp(0.0, 1.0);
                    after_clipped |= raw_after > 1.0;
                }
                if before_clipped {
                    clamped_before += 1;
                }
                if after_clipped {
                    clamped_after += 1;
                }
                luma_before.push(luma(before));
                luma_after.push(luma(after));
            }
        }
        luma_before.sort_by(|a, b| a.partial_cmp(b).unwrap());
        luma_after.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let count = vertices.max(1) as f32;
        LightReport {
            vertices,
            y_only,
            clamped_before: clamped_before as f32 / count,
            clamped_after: clamped_after as f32 / count,
            spread_before: percentile(&luma_before, 0.95) - percentile(&luma_before, 0.05),
            spread_after: percentile(&luma_after, 0.95) - percentile(&luma_after, 0.05),
            mean_before: luma_before.iter().sum::<f32>() / count,
            mean_after: luma_after.iter().sum::<f32>() / count,
            ndl_bins,
            up_facing: up_facing as f32 / count,
        }
    }

    /// Diagnostic: what the shipped Great Hall geometry does to the DNC light.
    ///
    /// `scripts/h2-cargo test -p wc3-render --lib world_art -- --nocapture`
    #[test]
    fn great_hall_light_distribution() {
        let Some((mpq, map)) = install() else {
            println!("SKIP world_art: install/map missing");
            return;
        };
        let assets = wc3_game::Assets::load(Some(&mpq), Some(&map));
        let asset = assets.model_for_def("ogre");
        let models: Vec<(Vec<Vec3>, bool)> = asset
            .meshes
            .iter()
            .map(|mesh| (mesh.expanded().normals, mesh.z_up))
            .collect();
        let curve = crate::daynight::DayNightCurve::default();
        let sample = curve.sample(12.0);
        let sun = [sample.unit.sun[0], sample.unit.sun[1], sample.unit.sun[2]];
        let ambient = [
            sample.unit.ambient[0],
            sample.unit.ambient[1],
            sample.unit.ambient[2],
        ];
        let dir = Vec3::from_array(sample.sun_direction);
        let report = light_report(&models, sun, ambient, dir);
        println!(
            "WORLD_ART greathall meshes={} verts={} y_only={} up_facing={:.3}",
            models.len(),
            report.vertices,
            report.y_only,
            report.up_facing
        );
        println!(
            "WORLD_ART sun={sun:?} ambient={ambient:?} dir={dir:?} bins={:?}",
            report.ndl_bins
        );
        println!(
            "WORLD_ART before clamped={:.3} mean={:.4} spread={:.4}",
            report.clamped_before, report.mean_before, report.spread_before
        );
        println!(
            "WORLD_ART after  clamped={:.3} mean={:.4} spread={:.4}",
            report.clamped_after, report.mean_after, report.spread_after
        );
        // Texture-independent form of the roof: the light span across the
        // up-facing surfaces (n.y > 0.6), which is where the dome lives. A
        // clipped light makes this zero no matter how good the texture is.
        let convert = z_up_to_y_up();
        let fitted = fit_directional(sun, ambient);
        let mut span_before = (f32::MAX, f32::MIN);
        let mut span_after = (f32::MAX, f32::MIN);
        let mut up_vertices = 0usize;
        for mesh in &asset.meshes {
            let normals = mesh.expanded().normals;
            for normal in &normals {
                let n = if mesh.z_up {
                    convert.transform_vector3(*normal)
                } else {
                    *normal
                }
                .normalize_or(Vec3::Y);
                if n.y <= 0.6 {
                    continue;
                }
                up_vertices += 1;
                let ndl = n.dot(dir).clamp(0.0, 1.0);
                let before: f32 = (0..3)
                    .map(|c| (ambient[c] + sun[c] * ndl).clamp(0.0, 1.0))
                    .fold(0.0f32, f32::max);
                let after: f32 = (0..3)
                    .map(|c| (ambient[c] + fitted[c] * ndl).clamp(0.0, 1.0))
                    .fold(0.0f32, f32::max);
                span_before = (span_before.0.min(before), span_before.1.max(before));
                span_after = (span_after.0.min(after), span_after.1.max(after));
            }
        }
        println!(
            "WORLD_ART up_facing_verts={up_vertices} light_span_before={:.4} light_span_after={:.4}",
            span_before.1 - span_before.0,
            span_after.1 - span_after.0
        );
        for (index, mesh) in asset.meshes.iter().enumerate() {
            println!(
                "{}",
                mesh_light_line(index, mesh, sun, ambient, dir)
            );
        }
    }

    /// One offscreen frame of the real map, plus a background frame of the same
    /// camera with the unit/doodad pass disabled so the building mask is exact.
    struct HallFrame {
        rgba: Vec<u8>,
        background: Vec<u8>,
        width: u32,
        height: u32,
        /// Screen rectangle of the model's own bounding box (x0, y0, x1, y1).
        /// Band limits come from here, not from the difference mask: the contact
        /// shadow grows the mask, and bands scaled by the mask would move between
        /// the before and the after frame.
        geometry_box: (usize, usize, usize, usize),
        /// Screen row where the model meets the ground (projected base centre).
        /// The contact ring is sampled below it: everything above is building and
        /// sky, where a ground shadow cannot land.
        base_row: usize,
    }

    /// Metrics of one building frame, split by the bands of the silhouette.
    ///
    /// Bands are taken from the Great Hall's own connected component, so the
    /// peons and the neighbouring hut on the same base never enter the numbers.
    #[derive(Debug, Default)]
    struct FrameMetrics {
        building_pixels: usize,
        roof_mean: f32,
        roof_spread: f32,
        /// Large-scale shading gradient: max - min of eight horizontal strip
        /// means across the roof band. Texture noise never moves this; a light
        /// model that loses its `N · L` gradient collapses it.
        roof_form: f32,
        roof_gradient: f32,
        /// p90 - p50 of the roof band: the shading difference between the lit and
        /// the average roof panel (texture-only variation included).
        roof_panel_spread: f32,
        wall_mean: f32,
        wall_spread: f32,
        wall_gradient: f32,
        /// Lowest 7 % of the silhouette against the 45..60 % band: the contact
        /// term, as a ratio (1.0 = the base is lit exactly like the wall above).
        base_ratio: f32,
        /// Share of the ground ring around the model that the unit pass darkens by
        /// more than 5 %: 0 = no shadow at all, 1 = the whole ring is shadowed.
        shadow_coverage: f32,
        /// Mean brightness of the shadowed part of that ring relative to the same
        /// ground without the unit pass (1.0 = nothing darkens the ground).
        ground_contact: f32,
        grass_mean: f32,
    }

    fn linear_luma(rgba: &[u8], index: usize) -> f32 {
        let channel = |offset: usize| {
            let value = rgba[index + offset] as f32 / 255.0;
            if value >= 0.040_45 {
                ((value + 0.055) / 1.055).powf(2.4)
            } else {
                value / 12.92
            }
        };
        0.2126 * channel(0) + 0.7152 * channel(1) + 0.0722 * channel(2)
    }

    /// Largest 4-connected component of the difference mask inside `window`,
    /// which is the building under the camera focus.
    fn largest_component(
        mask: &[bool],
        width: usize,
        height: usize,
        window: (usize, usize, usize, usize),
    ) -> Vec<bool> {
        let (x0, y0, x1, y1) = window;
        let mut visited = vec![false; mask.len()];
        let mut best: Vec<usize> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                let start = y * width + x;
                if !mask[start] || visited[start] {
                    continue;
                }
                let mut members = Vec::new();
                stack.clear();
                stack.push(start);
                visited[start] = true;
                while let Some(index) = stack.pop() {
                    members.push(index);
                    let (cx, cy) = (index % width, index / width);
                    for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                        let nx = cx as i32 + dx;
                        let ny = cy as i32 + dy;
                        if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                            continue;
                        }
                        let next = ny as usize * width + nx as usize;
                        if mask[next] && !visited[next] {
                            visited[next] = true;
                            stack.push(next);
                        }
                    }
                }
                if members.len() > best.len() {
                    best = members;
                }
            }
        }
        let mut out = vec![false; mask.len()];
        for index in best {
            out[index] = true;
        }
        out
    }

    fn metrics(frame: &HallFrame) -> FrameMetrics {
        let (width, height) = (frame.width as usize, frame.height as usize);
        let stride = width * 4;
        let (box_x0, box_y0, box_x1, box_y1) = frame.geometry_box;
        let mut raw = vec![false; width * height];
        for y in 0..height {
            for x in 0..width {
                let index = y * stride + x * 4;
                let diff = (0..3)
                    .map(|c| {
                        (frame.rgba[index + c] as i32 - frame.background[index + c] as i32).abs()
                    })
                    .max()
                    .unwrap_or(0);
                if diff > 6 {
                    raw[y * width + x] = true;
                }
            }
        }
        // Window around the camera focus: the shot is composed so the hall fills
        // the centre, and the component filter drops every other unit.
        let half = (height as f32 * 0.45) as usize;
        let window = (
            (width / 2).saturating_sub(half),
            (height / 2).saturating_sub(half),
            ((width / 2) + half).min(width),
            ((height / 2) + half).min(height),
        );
        let mut mask = largest_component(&raw, width, height, window);
        // The shadow belongs to the ground, not to the building: keep only the
        // mask inside the model's own projected box so the bands and the contact
        // annulus both stay comparable between frames.
        for y in 0..height {
            for x in 0..width {
                let index = y * width + x;
                if mask[index] && (x < box_x0 || x > box_x1 || y < box_y0 || y > box_y1) {
                    mask[index] = false;
                }
            }
        }
        let mut out = FrameMetrics::default();
        let mut min_y = height;
        let mut max_y = 0usize;
        for y in 0..height {
            for x in 0..width {
                if mask[y * width + x] {
                    out.building_pixels += 1;
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            }
        }
        if out.building_pixels == 0 || max_y <= min_y {
            return out;
        }
        // Bands from the geometry box: roof = top 40 %, wall = bottom 35 %.
        let span = (box_y1.saturating_sub(box_y0) + 1) as f32;
        let roof_end = box_y0 + ((span * 0.40) as usize).max(1);
        let wall_start = box_y1.saturating_sub(((span * 0.35) as usize).max(1));
        let band = |y0: usize, y1: usize| -> (Vec<f32>, Vec<f32>) {
            let mut luma = Vec::new();
            let mut gradient = Vec::new();
            for y in y0..y1.min(height) {
                for x in 0..width {
                    if !mask[y * width + x] {
                        continue;
                    }
                    let index = y * stride + x * 4;
                    let value = linear_luma(&frame.rgba, index);
                    luma.push(value);
                    let sample = |dx: i32, dy: i32| -> Option<f32> {
                        let (sx, sy) = (x as i32 + dx, y as i32 + dy);
                        if sx < 0 || sy < 0 || sx >= width as i32 || sy >= height as i32 {
                            return None;
                        }
                        if !mask[sy as usize * width + sx as usize] {
                            return None;
                        }
                        Some(linear_luma(&frame.rgba, sy as usize * stride + sx as usize * 4))
                    };
                    let gx = match (sample(1, 0), sample(-1, 0)) {
                        (Some(a), Some(b)) => (a - b).abs(),
                        (Some(a), None) | (None, Some(a)) => a,
                        (None, None) => 0.0,
                    };
                    let gy = match (sample(0, 1), sample(0, -1)) {
                        (Some(a), Some(b)) => (a - b).abs(),
                        (Some(a), None) | (None, Some(a)) => a,
                        (None, None) => 0.0,
                    };
                    gradient.push(gx.max(gy));
                }
            }
            (luma, gradient)
        };
        let (roof, roof_gradient) = band(min_y, roof_end);
        let (wall, wall_gradient) = band(wall_start, max_y + 1);
        let summarise = |values: &[f32]| -> (f32, f32) {
            if values.is_empty() {
                return (0.0, 0.0);
            }
            let mut sorted = values.to_vec();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mean = sorted.iter().sum::<f32>() / sorted.len() as f32;
            (
                mean,
                percentile(&sorted, 0.95) - percentile(&sorted, 0.05),
            )
        };
        let (roof_mean, roof_spread) = summarise(&roof);
        // Bright half of the roof band: the panels, without the dark ribs. A light
        // model that clips the whole dome at 1.0 collapses this range even though
        // the full-band spread (which the ribs dominate) looks healthy.
        let roof_panel = if roof.is_empty() {
            0.0
        } else {
            let mut sorted = roof.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            percentile(&sorted, 0.90) - percentile(&sorted, 0.50)
        };
        out.roof_panel_spread = roof_panel;
        let (wall_mean, wall_spread) = summarise(&wall);
        out.roof_mean = roof_mean;
        out.roof_spread = roof_spread;
        out.wall_mean = wall_mean;
        out.wall_spread = wall_spread;
        out.roof_gradient =
            roof_gradient.iter().sum::<f32>() / roof_gradient.len().max(1) as f32;
        out.wall_gradient =
            wall_gradient.iter().sum::<f32>() / wall_gradient.len().max(1) as f32;
        // Large-scale form: a 6x6 grid over the roof band of the geometry box. A directional light
        // puts the lit and shadow halves of a dome in different cells, so the
        // cell-mean range is exactly "how much of the form survives"; texture
        // noise averages out inside a cell.
        let mut cells = [[0.0f32; 6]; 6];
        let mut counts = [[0usize; 6]; 6];
        let mut min_x = width;
        let mut max_x = 0usize;
        for y in min_y..roof_end {
            for x in 0..width {
                if mask[y * width + x] {
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                }
            }
        }
        let across = (max_x.saturating_sub(min_x) + 1).max(1) as f32;
        for y in box_y0.max(min_y)..roof_end.min(height) {
            let row = (((y - box_y0) as f32 / span) * 6.0) as usize;
            let row = row.min(5);
            for x in 0..width {
                if !mask[y * width + x] {
                    continue;
                }
                let column = (((x.saturating_sub(min_x)) as f32 / across) * 6.0) as usize;
                let column = column.min(5);
                cells[row][column] += linear_luma(&frame.rgba, y * stride + x * 4);
                counts[row][column] += 1;
            }
        }
        let mut low = f32::MAX;
        let mut high = f32::MIN;
        for row in 0..6 {
            for column in 0..6 {
                if counts[row][column] < 40 {
                    continue;
                }
                let mean = cells[row][column] / counts[row][column] as f32;
                low = low.min(mean);
                high = high.max(mean);
            }
        }
        out.roof_form = if high > low { high - low } else { 0.0 };
        // Contact: lowest 7 % of the silhouette against the 45..60 % band.
        let base_start = max_y.saturating_sub(((span * 0.07) as usize).max(1));
        let ref_start = min_y + ((span * 0.45) as usize);
        let ref_end = min_y + ((span * 0.60) as usize).max(1);
        let band_mean = |y0: usize, y1: usize| -> f32 {
            let mut sum = 0.0f32;
            let mut count = 0usize;
            for y in y0..y1.min(height) {
                for x in 0..width {
                    if mask[y * width + x] {
                        sum += linear_luma(&frame.rgba, y * stride + x * 4);
                        count += 1;
                    }
                }
            }
            if count == 0 {
                0.0
            } else {
                sum / count as f32
            }
        };
        let base = band_mean(base_start, max_y + 1);
        let reference = band_mean(ref_start, ref_end);
        out.base_ratio = if reference > 1e-4 { base / reference } else { 0.0 };
        // Ground contact: the terrain ring just outside the model's projected box
        // (a silhouette cannot leave its own bounding box), compared pixel by
        // pixel with the same frame rendered without the unit pass. Two numbers:
        // `shadow_coverage` is the share of that ring the unit pass darkens by
        // more than 5 %, `ground_contact` is how dark it gets there. Pixels that
        // get *brighter* (a peon standing in the ring) are excluded from both, so
        // the measure is about the shadow, not about the units.
        let margin_x = ((box_x1 - box_x0) as f32 * 0.30) as usize + 2;
        let margin_y = ((box_y1 - box_y0) as f32 * 0.30) as usize + 2;
        let outer = (
            box_x0.saturating_sub(margin_x),
            box_y0.saturating_sub(margin_y),
            (box_x1 + margin_x).min(width - 1),
            (box_y1 + margin_y).min(height - 1),
        );
        let mut darkened = 0.0f32;
        let mut ring = 0.0f32;
        let mut darkened_pixels = 0usize;
        let ground_start = frame.base_row.saturating_sub(6);
        for y in outer.1.max(ground_start)..=outer.3 {
            for x in outer.0..=outer.2 {
                if x >= box_x0 && x <= box_x1 && y >= box_y0 && y <= box_y1 {
                    continue;
                }
                let index = y * width + x;
                let full = linear_luma(&frame.rgba, index * 4);
                let without = linear_luma(&frame.background, index * 4);
                ring += 1.0;
                if without > 1e-4 && full < without * 0.95 {
                    darkened += full / without;
                    darkened_pixels += 1;
                }
            }
        }
        out.shadow_coverage = if ring > 0.0 {
            darkened_pixels as f32 / ring
        } else {
            0.0
        };
        out.ground_contact = if darkened_pixels > 0 {
            darkened / darkened_pixels as f32
        } else {
            1.0
        };
        // Grass control: the background frame under the building's own rows.
        let mut grass = 0.0f32;
        let mut grass_count = 0usize;
        for y in (min_y..=max_y).step_by(3) {
            for x in (0..width).step_by(3) {
                if mask[y * width + x] {
                    continue;
                }
                grass += linear_luma(&frame.background, y * stride + x * 4);
                grass_count += 1;
            }
        }
        out.grass_mean = if grass_count == 0 {
            0.0
        } else {
            grass / grass_count as f32
        };
        out
    }

    /// Render the melee map's Orc Great Hall offscreen.
    ///
    /// The frame is the evidence artifact for packet `B-world`: same camera, same
    /// clock hour, one PNG + one metric line, so a before/after pair differs only
    /// by the renderer change under test.
    fn render_hall(tag: &str) -> Option<(HallFrame, FrameMetrics)> {
        use wc3_game::render::{HeadlessTarget, RenderOptions, Renderer};

        let (mpq, map) = install()?;
        let mut game = wc3_game::Game::new(wc3_game::Assets::load(Some(&mpq), Some(&map)), 0x5EED);
        game.enter_battle();
        game.update(wc3_game::game::TICK_SECONDS);
        let hall = game
            .sim
            .alive_units()
            .find(|unit| unit.owner == game.player && unit.def().is_main_hall())?;
        let hall_id = hall.def().id;
        let target = hall.pos;
        let (width, height) = (720u32, 720u32);
        game.camera.viewport = glam::Vec2::new(width as f32, height as f32);
        game.camera.target = target;
        let ground = game.assets.height_at(target.x, target.y);
        // Aim the shot at the building itself: the game camera always looks at
        // y = 0, which is 600 units under this map's Orc base, so the frame is
        // composed here (45° pitch, the device's RTS angle) instead.
        let eye = Vec3::new(target.x, ground + 900.0, target.y + 780.0);
        let focus = Vec3::new(target.x, ground + 150.0, target.y);
        let mut scene = game.scene();
        scene.view_proj = Mat4::perspective_rh(45f32.to_radians(), 1.0, 10.0, 40_000.0)
            * Mat4::look_at_rh(eye, focus, Vec3::Y);
        scene.eye = eye;
        scene.rings.clear();
        println!(
            "WORLD_ART hall def={hall_id} target={:?} ground={ground:.1} eye={:?} draws={}",
            target,
            eye,
            scene.units.len()
        );
        let mut buffer = HeadlessTarget::new(width, height).ok()?;
        let mut renderer = Renderer::new(
            &buffer.device,
            &buffer.queue,
            buffer.format,
            &game.assets,
            &game.font.atlas,
            None,
        )
        .ok()?;
        renderer.set_time_of_day(12.0);
        renderer.resize(&buffer.device, width, height);
        let full = {
            // Pin the clock for *both* frames: `render_with` advances the
            // free-running day/night clock by the wall time since the previous
            // frame, and the first frame's delta includes the whole asset upload
            // (~30 s = 1.5 game hours), so an unpinned before/after pair would be
            // lit at different times of day.
            renderer.set_time_of_day(12.0);
            buffer.render(
                &mut renderer,
                &scene,
                RenderOptions {
                    skip_doodads: true,
                    skip_ui: true,
                    ..Default::default()
                },
            );
            buffer.read().ok()?
        };
        let background = {
            renderer.set_time_of_day(12.0);
            buffer.render(
                &mut renderer,
                &scene,
                RenderOptions {
                    skip_units: true,
                    skip_doodads: true,
                    skip_ui: true,
                    ..Default::default()
                },
            );
            buffer.read().ok()?
        };
        // Screen box of the Great Hall's own geometry: the 8 corners of the
        // model's visible bounds through the unit's world matrix and the frame's
        // projection. Band limits and the contact annulus are derived from it.
        let model_index = game.assets.model_index_for_def("ogre");
        let hall_unit = scene
            .units
            .iter()
            .find(|unit| {
                unit.model == model_index
                    && (unit.world.w_axis.x - target.x).abs() < 1.0
                    && (unit.world.w_axis.z - target.y).abs() < 1.0
            })
            .or_else(|| scene.units.iter().find(|unit| unit.model == model_index))?;
        let bounds = local_bounds(&game.assets, model_index, &hall_unit.mesh_alpha);
        let geometry_box = screen_box(bounds, hall_unit.world, scene.view_proj, width, height);
        let base_world = hall_unit
            .world
            .transform_point3(Vec3::new(
                (bounds.0.x + bounds.1.x) * 0.5,
                bounds.0.y,
                (bounds.0.z + bounds.1.z) * 0.5,
            ));
        let base_clip = scene.view_proj * base_world.extend(1.0);
        let base_row = if base_clip.w > 0.0 {
            let ndc = base_clip.truncate() / base_clip.w;
            (((0.5 - ndc.y * 0.5) * height as f32).clamp(0.0, height as f32 - 1.0)) as usize
        } else {
            geometry_box.3
        };
        let frame = HallFrame {
            rgba: full,
            background,
            width,
            height,
            geometry_box,
            base_row,
        };
        let metrics = metrics(&frame);
        if tag != "none" {
            let path = out_dir().join(format!("hall-{tag}.png"));
            let _ = wc3_game::render::write_png(
                path.to_str().unwrap(),
                frame.width,
                frame.height,
                &frame.rgba,
            );
            println!("WORLD_ART png={}", path.display());
        }
        Some((frame, metrics))
    }

    /// Y-up model-space bounds of one model's visible geometry, exactly as
    /// `MeshRenderer` computes them for the contact band.
    fn local_bounds(
        assets: &wc3_game::Assets,
        model: usize,
        mesh_alpha: &[f32],
    ) -> (Vec3, Vec3) {
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        if let Some(asset) = assets.models.get(model) {
            for (index, mesh) in asset.meshes.iter().enumerate() {
                // Only what this frame draws: a building model contains every
                // upgrade stage, and the hidden stages would stretch the box far
                // past the visible silhouette.
                if mesh.hide || mesh_alpha.get(index).copied().unwrap_or(1.0) <= 0.001 {
                    continue;
                }
                let expanded = mesh.expanded();
                for position in &expanded.positions {
                    let local = if mesh.z_up {
                        Vec3::new(position.x, position.z, -position.y)
                    } else {
                        *position
                    };
                    min = min.min(local);
                    max = max.max(local);
                }
            }
        }
        if min.x <= max.x {
            (min, max)
        } else {
            (Vec3::ZERO, Vec3::ZERO)
        }
    }

    /// Screen rectangle of a model-space AABB under `world` and `view_proj`.
    fn screen_box(
        bounds: (Vec3, Vec3),
        world: Mat4,
        view_proj: Mat4,
        width: u32,
        height: u32,
    ) -> (usize, usize, usize, usize) {
        let (min, max) = bounds;
        let mut x0 = width as f32;
        let mut y0 = height as f32;
        let mut x1 = 0.0f32;
        let mut y1 = 0.0f32;
        for corner in 0..8 {
            let local = Vec3::new(
                if corner & 1 == 0 { min.x } else { max.x },
                if corner & 2 == 0 { min.y } else { max.y },
                if corner & 4 == 0 { min.z } else { max.z },
            );
            let clip = view_proj * (world * local.extend(1.0));
            if clip.w <= 0.0 {
                continue;
            }
            let ndc = clip.truncate() / clip.w;
            let x = (ndc.x * 0.5 + 0.5) * width as f32;
            let y = (0.5 - ndc.y * 0.5) * height as f32;
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        (
            x0.max(0.0) as usize,
            y0.max(0.0) as usize,
            (x1.min(width as f32 - 1.0)).max(0.0) as usize,
            (y1.min(height as f32 - 1.0)).max(0.0) as usize,
        )
    }

    fn print_metrics(tag: &str, metrics: &FrameMetrics) {
        println!(
            "WORLD_ART {tag} pixels={} roof_mean={:.4} roof_spread={:.4} roof_form={:.4} roof_panel={:.4} roof_grad={:.4} wall_mean={:.4} wall_spread={:.4} wall_grad={:.4} base_ratio={:.3} shadow={:.3} ground_contact={:.3} grass={:.4}",
            metrics.building_pixels,
            metrics.roof_mean,
            metrics.roof_spread,
            metrics.roof_form,
            metrics.roof_panel_spread,
            metrics.roof_gradient,
            metrics.wall_mean,
            metrics.wall_spread,
            metrics.wall_gradient,
            metrics.base_ratio,
            metrics.shadow_coverage,
            metrics.ground_contact,
            metrics.grass_mean
        );
    }

    /// Diagnostic: baseline/after frame of the Great Hall.
    ///
    /// `H2_WORLD_ART_TAG=before scripts/h2-cargo test -p wc3-render --lib
    /// world_art_hall_frame -- --nocapture`
    #[test]
    fn world_art_hall_frame() {
        let tag = std::env::var("H2_WORLD_ART_TAG").unwrap_or_else(|_| "frame".to_string());
        let legacy = std::env::var("H2_WORLD_ART_LEGACY").is_ok()
            || std::env::var("H2_WORLD_ART_LIGHT").as_deref() == Ok("legacy");
        let Some((_frame, metrics)) = render_hall(&tag) else {
            println!("SKIP world_art_hall_frame: install/map/GPU missing");
            return;
        };
        print_metrics(&tag, &metrics);
        if legacy {
            // Evidence frame only: the pre-fix renderer has no contact term and no
            // ground shadow, and that is exactly what this state must show.
            println!("WORLD_ART legacy frame: no contrast assertions");
            return;
        }
        // Regression gate on the real frame (packet `B-world` acceptance): the
        // Great Hall must be grounded by the unit pass and must keep shading
        // detail on its roof.
        assert!(
            metrics.building_pixels > 20_000,
            "the Great Hall must fill the close-up: {} px",
            metrics.building_pixels
        );
        // Measured on the shipped install (Turnip Adreno 725, 720x720, hour 12):
        // before  shadow 0.003 base 0.765;  after  shadow 0.027 base 0.713.
        assert!(
            metrics.shadow_coverage > 0.015,
            "the unit pass must darken the ground beside the hall (contact shadow), coverage {:.3}",
            metrics.shadow_coverage
        );
        assert!(
            metrics.ground_contact < 0.85,
            "the ground shadow must be visible, got {:.3} of the unshadowed luma",
            metrics.ground_contact
        );
        assert!(
            metrics.base_ratio < 0.75,
            "the base of the silhouette must be darker than its middle (contact band), got {:.3}",
            metrics.base_ratio
        );
        assert!(
            metrics.roof_panel_spread > 0.20,
            "the roof band must carry shading detail, got {:.3}",
            metrics.roof_panel_spread
        );
        assert!(
            metrics.roof_form > 0.10,
            "the roof must show large-scale form, got {:.3}",
            metrics.roof_form
        );
    }
}
