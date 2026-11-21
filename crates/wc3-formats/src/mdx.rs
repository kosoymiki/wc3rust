//! Classic MDX model parser (v800).
//!
//! Layouts are taken from the published MDX format and cross-checked against a
//! real `Footman.mdx` fixture; the parser is bounds checked and returns typed
//! errors. Reforged-specific material layouts (v900+) are rejected explicitly.

use glam::{Quat, Vec2, Vec3};

use crate::FormatError;

/// Maximum UV sets a geoset may declare. The shipped models carry one or two;
/// the ceiling exists so a forged `UVAS` count can never size an allocation
/// even when the chunk itself is large (WC3-MDX-001).
const MAX_UV_SETS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    None,
    Linear,
    Hermite,
    Bezier,
}

#[derive(Debug, Clone, Copy)]
pub struct Key<T> {
    pub time: u32,
    pub value: T,
    pub in_tan: T,
    pub out_tan: T,
}

#[derive(Debug, Clone)]
pub struct Track<T> {
    pub interpolation: Interpolation,
    pub global_sequence: Option<u32>,
    pub keys: Vec<Key<T>>,
    pub static_value: Option<T>,
}

pub trait AnimValue: Copy {
    fn lerp(a: Self, b: Self, t: f32) -> Self;
    fn hermite(p0: Self, m0: Self, p1: Self, m1: Self, t: f32) -> Self;
    fn bezier(p0: Self, c0: Self, c1: Self, p1: Self, t: f32) -> Self;
}

impl AnimValue for f32 {
    fn lerp(a: Self, b: Self, t: f32) -> Self {
        a + (b - a) * t
    }
    fn hermite(p0: Self, m0: Self, p1: Self, m1: Self, t: f32) -> Self {
        let t2 = t * t;
        let t3 = t2 * t;
        (2.0 * t3 - 3.0 * t2 + 1.0) * p0
            + (t3 - 2.0 * t2 + t) * m0
            + (-2.0 * t3 + 3.0 * t2) * p1
            + (t3 - t2) * m1
    }
    fn bezier(p0: Self, c0: Self, c1: Self, p1: Self, t: f32) -> Self {
        let u = 1.0 - t;
        u * u * u * p0 + 3.0 * u * u * t * c0 + 3.0 * u * t * t * c1 + t * t * t * p1
    }
}

impl AnimValue for Vec3 {
    fn lerp(a: Self, b: Self, t: f32) -> Self {
        a.lerp(b, t)
    }
    fn hermite(p0: Self, m0: Self, p1: Self, m1: Self, t: f32) -> Self {
        Vec3::new(
            f32::hermite(p0.x, m0.x, p1.x, m1.x, t),
            f32::hermite(p0.y, m0.y, p1.y, m1.y, t),
            f32::hermite(p0.z, m0.z, p1.z, m1.z, t),
        )
    }
    fn bezier(p0: Self, c0: Self, c1: Self, p1: Self, t: f32) -> Self {
        Vec3::new(
            f32::bezier(p0.x, c0.x, c1.x, p1.x, t),
            f32::bezier(p0.y, c0.y, c1.y, p1.y, t),
            f32::bezier(p0.z, c0.z, c1.z, p1.z, t),
        )
    }
}

impl AnimValue for u32 {
    fn lerp(a: Self, b: Self, t: f32) -> Self {
        if t < 0.5 { a } else { b }
    }
    fn hermite(p0: Self, _m0: Self, p1: Self, _m1: Self, t: f32) -> Self {
        if t < 0.5 { p0 } else { p1 }
    }
    fn bezier(p0: Self, _c0: Self, c1: Self, _p1: Self, t: f32) -> Self {
        if t < 0.5 { p0 } else { c1 }
    }
}

impl AnimValue for Quat {
    fn lerp(a: Self, b: Self, t: f32) -> Self {
        a.slerp(b, t)
    }
    /// Quaternion squad interpolation, matching `quat.sqlerp` in the reference
    /// viewers (`donors/mdx-m3-viewer/src/viewer/handlers/mdx/sd.ts:314`): the
    /// tangents are control quaternions, not component-wise slopes. Doing the
    /// hermite per component and normalising instead bends limbs away from the
    /// authored pose (Archer arms/bow overshoot their sequence extents by ~19%).
    fn hermite(p0: Self, m0: Self, p1: Self, m1: Self, t: f32) -> Self {
        squad(p0, m0, m1, p1, t)
    }
    fn bezier(p0: Self, c0: Self, c1: Self, p1: Self, t: f32) -> Self {
        squad(p0, c0, c1, p1, t)
    }
}

/// `squad(q1, q2, q3, q4, t) = slerp(slerp(q1, q4, t), slerp(q2, q3, t), 2t(1-t))`
/// (gl-matrix `quat.sqlerp`).
fn squad(q1: Quat, q2: Quat, q3: Quat, q4: Quat, t: f32) -> Quat {
    let a = q1.slerp(q4, t);
    let b = q2.slerp(q3, t);
    a.slerp(b, 2.0 * t * (1.0 - t)).normalize()
}

impl<T: AnimValue> Track<T> {
    /// Sample the track at a sequence-relative frame.
    ///
    /// `keys` is sorted by **signed** time: the file stores `int32` frame
    /// numbers and shipped models use negative pre-start keys (WC3-I-009).
    /// Keys before frame 0 are outside the sequence and are skipped, as the
    /// engine's per-sequence sampler does. Duplicate timestamps resolve to the
    /// **earlier** key (the first window wins, and equal times are a zero-length
    /// step at that instant).
    pub fn sample(&self, time: u32) -> Option<T> {
        if let Some(value) = self.static_value {
            return Some(value);
        }
        // Keyframe times are `int32` on disk, so a key before frame 0 lies
        // outside every sequence; the engine filters those keys out before
        // sampling (mdx-m3-viewer `viewer/handlers/mdx/sd.ts` keeps only
        // `frame >= start`). Sampling a sequence-relative time therefore uses
        // the keys at frame >= 0 only, and every subtraction is signed, so a
        // pre-start key can never underflow the interpolation span
        // (`0xFFFF_FF9C - 3900` as `u32` was the WC3-I-009 panic).
        let time = time as i64;
        let mut previous: Option<&Key<T>> = None;
        for key in &self.keys {
            let at = key.time as i32 as i64;
            if at < 0 {
                continue;
            }
            let Some(a) = previous else {
                if time <= at {
                    return Some(key.value);
                }
                previous = Some(key);
                continue;
            };
            if time == at {
                // The engine's step scan selects the first key with
                // `frames[i] > frame` (mdx-m3-viewer `sd.ts:95`), so at an exact
                // keyframe the key's own value applies — visible for
                // `DontInterp` tracks, where the earlier key otherwise wins one
                // frame too long.
                return Some(key.value);
            }
            if time < at {
                let from = a.time as i32 as i64;
                let span = (at - from).max(1) as f32;
                let t = ((time - from) as f32 / span).clamp(0.0, 1.0);
                return Some(match self.interpolation {
                    Interpolation::None => a.value,
                    Interpolation::Linear => T::lerp(a.value, key.value, t),
                    Interpolation::Hermite => {
                        T::hermite(a.value, a.out_tan, key.value, key.in_tan, t)
                    }
                    // Bezier control points are (p0, out_tan, in_tan, p1).
                    Interpolation::Bezier => {
                        T::bezier(a.value, a.out_tan, key.in_tan, key.value, t)
                    }
                });
            }
            previous = Some(key);
        }
        // A track whose keys all sit before frame 0 samples to `None`: the
        // caller applies the object's static value, exactly like the engine's
        // per-sequence "no keys in range -> default value".
        previous.map(|key| key.value)
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub object_id: u32,
    pub parent_id: i32,
    pub flags: u32,
    pub translation: Track<Vec3>,
    pub rotation: Track<Quat>,
    pub scaling: Track<Vec3>,
}

#[derive(Debug, Clone)]
pub struct Bone {
    pub node: Node,
    pub geoset_id: i32,
    pub geoset_animation_id: i32,
}

#[derive(Debug, Clone)]
pub struct Sequence {
    pub name: String,
    pub start: u32,
    pub end: u32,
    pub move_speed: f32,
    pub flags: u32,
    pub rarity: f32,
    pub sync_point: u32,
    pub bounds: Extent,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Extent {
    pub radius: f32,
    pub min: Vec3,
    pub max: Vec3,
}

impl Extent {
    fn read(r: &mut Reader<'_>) -> Result<Self, FormatError> {
        Ok(Self {
            radius: r.f32()?,
            min: Vec3::new(r.f32()?, r.f32()?, r.f32()?),
            max: Vec3::new(r.f32()?, r.f32()?, r.f32()?),
        })
    }

    /// True for the exporter's "no geometry in this sequence" sentinel.
    ///
    /// WC3 writes `minimum = +FLT_MAX, maximum = -FLT_MAX` for a geoset that is
    /// not part of a sequence (Footman `gutz`, Archer arrow, TownHall upgrade
    /// shells). The sentinel is the only reliable per-sequence visibility flag
    /// in classic MDX: a geoset with an empty extent is never drawn in that
    /// sequence even when its geoset animation alpha is 1.
    pub fn is_empty(&self) -> bool {
        !self.radius.is_finite()
            || self.min.x > self.max.x
            || self.min.y > self.max.y
            || self.min.z > self.max.z
    }

    pub fn contains(&self, point: Vec3) -> bool {
        point.x >= self.min.x
            && point.y >= self.min.y
            && point.z >= self.min.z
            && point.x <= self.max.x
            && point.y <= self.max.y
            && point.z <= self.max.z
    }

    /// Overshoot of `point` outside the extent (0 when inside). Infinite for an
    /// empty extent, which keeps the audit arithmetic simple.
    pub fn overshoot(&self, point: Vec3) -> f32 {
        if self.is_empty() {
            return f32::INFINITY;
        }
        let dx = (self.min.x - point.x).max(point.x - self.max.x);
        let dy = (self.min.y - point.y).max(point.y - self.max.y);
        let dz = (self.min.z - point.z).max(point.z - self.max.z);
        dx.max(dy).max(dz).max(0.0)
    }

    pub fn expanded(&self, margin: f32) -> Extent {
        if self.is_empty() {
            return *self;
        }
        Extent {
            radius: self.radius + margin,
            min: self.min - Vec3::splat(margin),
            max: self.max + Vec3::splat(margin),
        }
    }

    pub fn diagonal(&self) -> f32 {
        if self.is_empty() {
            return 0.0;
        }
        (self.max - self.min).length()
    }
}

#[derive(Debug, Clone)]
pub struct Texture {
    pub replaceable_id: u32,
    pub path: String,
    pub flags: u32,
}

#[derive(Debug, Clone)]
pub struct Layer {
    pub blend_mode: u32,
    pub shading_flags: u32,
    pub texture_id: u32,
    pub texture_animation_id: u32,
    pub coord_id: u32,
    pub alpha: f32,
    pub alpha_track: Option<Track<f32>>,
    pub texture_id_track: Option<Track<u32>>,
}

#[derive(Debug, Clone)]
pub struct Material {
    pub priority: i32,
    pub flags: u32,
    pub layers: Vec<Layer>,
}

#[derive(Debug, Clone)]
pub struct Geoset {
    pub vertices: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub face_type_groups: Vec<u32>,
    pub face_groups: Vec<u32>,
    pub faces: Vec<u16>,
    pub vertex_groups: Vec<u8>,
    pub matrix_groups: Vec<u32>,
    pub matrix_indices: Vec<u32>,
    pub material_id: u32,
    pub selection_group: u32,
    pub selection_flags: u32,
    /// Level of detail (`geoset.lod`, MDX v900+; `0` for older files).
    ///
    /// Warcraft III ships several detail levels of the same body: the geosets of
    /// one model carry different `lod` values (0 = highest detail, 1 = next, …),
    /// and a renderer must draw exactly one group per model — drawing every
    /// geoset stacks a second body on the first. Filter with [`Geoset::drawn_at`]
    /// or [`Geoset::lowest_lod`].
    pub lod: u32,
    /// Human readable geoset name (`geoset.lod_name`, MDX v900+).
    pub lod_name: String,
    pub bounds: Extent,
    pub sequence_bounds: Vec<Extent>,
    pub uv_sets: Vec<Vec<Vec2>>,
    pub skin: Vec<u16>,
}

impl Geoset {
    /// The lowest LOD value the model carries, i.e. the detail level a renderer
    /// without a distance-based LOD policy should draw.
    ///
    /// `None` only for an empty geoset list. This is per geoset, so a whole-model
    /// answer is `geosets.iter().map(|g| g.lod).min()`.
    pub fn lowest_lod(geosets: &[Geoset]) -> Option<u32> {
        geosets.iter().map(|geoset| geoset.lod).min()
    }

    /// True when this geoset belongs to detail level `lod`.
    pub fn drawn_at(&self, lod: u32) -> bool {
        self.lod == lod
    }
    /// Extent for `sequence_index`, or `None` when the exporter marked the
    /// geoset as absent from that sequence (`Extent::is_empty`).
    pub fn sequence_extent(&self, sequence_index: usize) -> Option<Extent> {
        self.sequence_bounds
            .get(sequence_index)
            .copied()
            .filter(|extent| !extent.is_empty())
    }

    /// True when the geoset can be drawn in `sequence_index`.
    ///
    /// Missing per-sequence data means "no information", not "absent": models
    /// like `Units/Creeps/TempArt/TempArt.mdx` and `Units/Demon/Infernal/Rock1.MDX`
    /// ship an empty `sequence_bounds` array (or a shorter one than the sequence
    /// list) and must still render. Only an explicit sentinel extent hides a
    /// geoset.
    pub fn present_in_sequence(&self, sequence_index: usize) -> bool {
        match self.sequence_bounds.get(sequence_index) {
            Some(extent) => !extent.is_empty(),
            None => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub node: Node,
    pub path: String,
    pub attachment_id: u32,
}

#[derive(Debug, Clone)]
pub struct GeosetAnimation {
    pub geoset_id: u32,
    pub alpha: f32,
    pub flags: u32,
    /// Geoset colour grade. Stored **BGR** in the file (the exporter writes
    /// `color[2], color[1], color[0]`), so this field is already sizzled to RGB
    /// and ready to multiply vertex colours.
    pub color: Vec3,
    pub alpha_track: Option<Track<f32>>,
    pub color_track: Option<Track<Vec3>>,
}

/// `PREM` flags (`EmitterUsesMDL` / `EmitterUsesTGA`).
pub const PREM_FLAG_USES_MDL: u32 = 0x8000;
pub const PREM_FLAG_USES_TGA: u32 = 0x1_0000;

/// A classic (`PREM`) particle emitter: one sprite per particle, no atlas, no
/// segments. Drawn only by the oldest WC3 effects; `PRE2` is the common case.
#[derive(Debug, Clone)]
pub struct ParticleEmitter {
    pub node: Node,
    pub emission_rate: f32,
    pub gravity: f32,
    pub longitude: f32,
    pub latitude: f32,
    /// Emitter texture path (`EmitterUsesMDL`/`EmitterUsesTGA` models only).
    pub path: String,
    pub life_span: f32,
    pub speed: f32,
    pub emission_rate_track: Option<Track<f32>>,
    pub gravity_track: Option<Track<f32>>,
    pub longitude_track: Option<Track<f32>>,
    pub latitude_track: Option<Track<f32>>,
    pub life_span_track: Option<Track<f32>>,
    pub speed_track: Option<Track<f32>>,
    pub visibility_track: Option<Track<f32>>,
}

/// `PRE2` flags.
pub const PRE2_FLAG_UNSHADED: u32 = 0x8000;
pub const PRE2_FLAG_SORT_PRIMS_FAR_Z: u32 = 0x1_0000;
pub const PRE2_FLAG_LINE_EMITTER: u32 = 0x2_0000;
pub const PRE2_FLAG_UNFOGGED: u32 = 0x4_0000;
pub const PRE2_FLAG_MODEL_SPACE: u32 = 0x8_0000;
pub const PRE2_FLAG_XY_QUAD: u32 = 0x10_0000;

/// `PRE2` filter mode (`Blend`, `Additive`, `Modulate`, `Modulate2x`, `AlphaKey`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticleFilter {
    Blend,
    Additive,
    Modulate,
    Modulate2x,
    AlphaKey,
}

impl ParticleFilter {
    pub fn from_raw(raw: u32) -> Self {
        match raw {
            1 => Self::Additive,
            2 => Self::Modulate,
            3 => Self::Modulate2x,
            4 => Self::AlphaKey,
            _ => Self::Blend,
        }
    }

    /// Blend-mode number the renderer's material pipelines already use.
    pub fn layer_blend(self) -> u32 {
        match self {
            Self::Blend | Self::AlphaKey => 2,
            Self::Additive => 3,
            Self::Modulate => 5,
            Self::Modulate2x => 6,
        }
    }
}

/// `PRE2` head/tail selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadOrTail {
    Head,
    Tail,
    Both,
}

impl HeadOrTail {
    pub fn from_raw(raw: u32) -> Self {
        match raw {
            1 => Self::Tail,
            2 => Self::Both,
            _ => Self::Head,
        }
    }
}

/// A `PRE2` particle emitter: the workhorse of every WC3 spell/impact effect.
///
/// Three-segment colour/scale/UV animation (`time_middle` splits segment 0-1
/// and 1-2), optional tail geometry (`tail_length`), a sprite atlas
/// (`rows` × `columns`) and per-sprite UV intervals (`start`, `end`, `repeat`).
#[derive(Debug, Clone)]
pub struct ParticleEmitter2 {
    pub node: Node,
    pub speed: f32,
    pub variation: f32,
    pub latitude: f32,
    pub gravity: f32,
    pub life_span: f32,
    pub emission_rate: f32,
    pub width: f32,
    pub length: f32,
    pub filter_mode: u32,
    pub rows: u32,
    pub columns: u32,
    pub head_or_tail: u32,
    pub tail_length: f32,
    pub time_middle: f32,
    pub segment_colors: [Vec3; 3],
    pub segment_alphas: [u8; 3],
    pub segment_scaling: [f32; 3],
    /// `[LifeSpanUVAnim, DecayUVAnim]`, each `[start, end, repeat]`.
    pub head_intervals: [[u32; 3]; 2],
    /// `[TailUVAnim, TailDecayUVAnim]`.
    pub tail_intervals: [[u32; 3]; 2],
    pub texture_id: i32,
    pub squirt: u32,
    pub priority_plane: i32,
    pub replaceable_id: u32,
    pub speed_track: Option<Track<f32>>,
    pub variation_track: Option<Track<f32>>,
    pub latitude_track: Option<Track<f32>>,
    pub gravity_track: Option<Track<f32>>,
    pub visibility_track: Option<Track<f32>>,
    pub emission_rate_track: Option<Track<f32>>,
    pub width_track: Option<Track<f32>>,
    pub length_track: Option<Track<f32>>,
}

impl ParticleEmitter2 {
    pub fn filter(&self) -> ParticleFilter {
        ParticleFilter::from_raw(self.filter_mode)
    }

    pub fn head_or_tail(&self) -> HeadOrTail {
        HeadOrTail::from_raw(self.head_or_tail)
    }

    pub fn flag(&self, flag: u32) -> bool {
        self.node.flags & flag != 0
    }

    /// `true` when the emitter draws a camera-facing billboard (default) and
    /// `false` for the `XYQuad` rectangle in the emitter's XY plane.
    pub fn billboarded(&self) -> bool {
        !self.flag(PRE2_FLAG_XY_QUAD)
    }
}

/// A `RIBB` ribbon emitter: a strip of camera-facing quads through the emitted
/// points, `height_above`/`height_below` wide.
#[derive(Debug, Clone)]
pub struct RibbonEmitter {
    pub node: Node,
    pub height_above: f32,
    pub height_below: f32,
    pub alpha: f32,
    pub color: Vec3,
    pub life_span: f32,
    pub texture_slot: u32,
    pub emission_rate: u32,
    pub rows: u32,
    pub columns: u32,
    pub material_id: i32,
    pub gravity: f32,
    pub height_above_track: Option<Track<f32>>,
    pub height_below_track: Option<Track<f32>>,
    pub alpha_track: Option<Track<f32>>,
    pub color_track: Option<Track<Vec3>>,
    pub texture_slot_track: Option<Track<u32>>,
    pub visibility_track: Option<Track<f32>>,
}

/// `KEVT` payload: the **frames** at which the event fires.
///
/// The on-disk array is `count` u32 in ascending order, each one a frame number
/// (`DivineShieldTarget` fires at 833, `FlareTarget` at 33, `BallistaMissile`
/// at 0 and 2233). It is not a dense per-frame flag array: the donor's sampler
/// compares the stored value against the current frame
/// (`mdx-m3-viewer/src/viewer/handlers/mdx/eventobjectemitterobject.ts:143-170`,
/// `if (tracks[i] <= frame) { out[0] = 1; return i; }`).
#[derive(Debug, Clone, Default)]
pub struct EventTrack {
    /// Firing frames, ascending.
    pub frames: Vec<u32>,
    /// `GlobalSeqId`, when the event lives on a global sequence.
    pub global_sequence: Option<u32>,
}

impl EventTrack {
    /// Index of the last event at or before `frame`, if any.
    pub fn index_at(&self, frame: u32) -> Option<usize> {
        match self.frames.binary_search(&frame) {
            Ok(index) => Some(index),
            Err(0) => None,
            Err(index) => Some(index - 1),
        }
    }

    /// `true` when the event fires at exactly `frame`.
    pub fn fires_at(&self, frame: u32) -> bool {
        self.frames.binary_search(&frame).is_ok()
    }
}

/// An `EVTS` event object: a node plus the frames at which it fires.
///
/// The name is the event object table key: `"SPN <id>"` spawns model `<id>`,
/// `"SPL <id>"`/`"FPT <id>"` draws splat `<id>`, `"UBR <id>"` draws an ubersplat
/// and `"SND <id>"` plays sound `<id>`.
#[derive(Debug, Clone)]
pub struct EventObject {
    pub node: Node,
    pub track: EventTrack,
}

impl EventObject {
    /// `(kind, id)` decoded from the node name, e.g. `"SPN Head"` -> `("SPN", "Head")`.
    ///
    /// Optimised models separate the prefix with `x` instead of a space
    /// (`"SPNxHBF0"`, `"SPLxHBS0"`, `"SNDXDFOO"` — Footman.mdx), so the id is
    /// everything after the fourth character, exactly like the donor's
    /// `name.substring(0, 3)` / `name.substring(4)`
    /// (`mdx-m3-viewer/src/viewer/handlers/mdx/eventobjectemitterobject.ts:57-58`).
    pub fn kind_and_id(&self) -> (EventObjectKind, String) {
        let name = self.node.name.trim();
        let prefix: String = name.chars().take(3).collect();
        let id: String = name.chars().skip(4).collect();
        let kind = match prefix.as_str() {
            "SPN" => EventObjectKind::SpawnModel,
            "SPL" | "FPT" => EventObjectKind::Splat,
            "UBR" => EventObjectKind::UberSplat,
            "SND" => EventObjectKind::Sound,
            _ => EventObjectKind::Unknown,
        };
        (kind, id.trim().to_string())
    }
}

/// Event object family, decoded from the `EVTS` node name prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventObjectKind {
    /// `SPN`: spawn a model instance (footprints, arrow trails, gibs).
    SpawnModel,
    /// `SPL` / `FPT`: a splat quad from `Splats\SplatData.txt`.
    Splat,
    /// `UBR`: an ubersplat quad.
    UberSplat,
    /// `SND`: a positional sound.
    Sound,
    Unknown,
}


#[derive(Debug, Clone, Default)]
pub struct MdxModel {
    pub version: u32,
    pub name: String,
    pub animation_bounds: (u32, u32),
    pub blend_time: u32,
    pub sequences: Vec<Sequence>,
    pub global_sequences: Vec<u32>,
    pub textures: Vec<Texture>,
    pub materials: Vec<Material>,
    pub geosets: Vec<Geoset>,
    pub geoset_animations: Vec<GeosetAnimation>,
    pub bones: Vec<Bone>,
    pub helpers: Vec<Node>,
    pub attachments: Vec<Attachment>,
    pub events: Vec<Node>,
    /// Event objects with their `KEVT` tracks (same order as [`Self::events`]).
    pub event_objects: Vec<EventObject>,
    /// `PREM` emitters (classic sprite particles).
    pub particle_emitters: Vec<ParticleEmitter>,
    /// `PRE2` emitters (atlas sprite particles with segments/tails).
    pub particle_emitters2: Vec<ParticleEmitter2>,
    /// `RIBB` ribbon emitters.
    pub ribbon_emitters: Vec<RibbonEmitter>,
    pub pivots: Vec<Vec3>,
    pub raw_chunks: Vec<([u8; 4], Vec<u8>)>,
}

impl MdxModel {
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        if bytes.len() < 4 || &bytes[..4] != b"MDLX" {
            return Err(FormatError::UnknownMagic(
                bytes
                    .first_chunk::<4>()
                    .copied()
                    .unwrap_or([0; 4]),
            ));
        }
        let mut model = MdxModel::default();
        let mut reader = Reader::new(bytes, 4);
        while reader.remaining() >= 8 {
            let tag: [u8; 4] = reader.tag()?;
            let size = reader.u32()? as usize;
            let data = reader.bytes(size)?;
            let mut r = Reader::new(data, 0);
            match &tag {
                b"VERS" => model.version = r.u32()?,
                b"MODL" => {
                    model.name = r.string(80)?;
                    model.animation_bounds = (r.u32()?, r.u32()?);
                    model.blend_time = r.u32()?;
                }
                b"SEQS" => {
                    while r.remaining() >= 132 {
                        model.sequences.push(Sequence {
                            name: r.string(80)?,
                            start: r.u32()?,
                            end: r.u32()?,
                            move_speed: r.f32()?,
                            flags: r.u32()?,
                            rarity: r.f32()?,
                            sync_point: r.u32()?,
                            bounds: Extent::read(&mut r)?,
                        });
                    }
                }
                b"GLBS" => {
                    while r.remaining() >= 4 {
                        model.global_sequences.push(r.u32()?);
                    }
                }
                b"TEXS" => {
                    while r.remaining() >= 268 {
                        model.textures.push(Texture {
                            replaceable_id: r.u32()?,
                            path: r.string(260)?,
                            flags: r.u32()?,
                        });
                    }
                }
                b"MTLS" => parse_materials(&mut r, model.version, &mut model.materials)?,
                b"GEOS" => {
                    while r.remaining() > 0 {
                        model.geosets.push(parse_geoset(&mut r, model.version)?);
                    }
                }
                b"GEOA" => {
                    model.geoset_animations = parse_geoset_animations(&mut r)?;
                }
                b"BONE" => {
                    while r.remaining() > 0 {
                        let node = parse_node(&mut r)?;
                        let geoset_id = r.i32()?;
                        let geoset_animation_id = r.i32()?;
                        model.bones.push(Bone {
                            node,
                            geoset_id,
                            geoset_animation_id,
                        });
                    }
                }
                b"HELP" => {
                    while r.remaining() > 0 {
                        model.helpers.push(parse_node(&mut r)?);
                    }
                }
                b"ATCH" => {
                    while r.remaining() > 0 {
                        let outer_start = r.position;
                        let outer_size = r.u32()? as usize;
                        let outer_end = outer_start
                            .checked_add(outer_size)
                            .ok_or(FormatError::Corrupt("attachment size overflow"))?;
                        if outer_end > r.data.len() {
                            return Err(FormatError::Corrupt("attachment size exceeds chunk"));
                        }
                        let node = parse_node(&mut r)?;
                        // ATCH stores a 260 byte path followed by the attachment
                        // id. The previous 256+4+4 split clipped four path
                        // characters and read the path tail as a reserved word.
                        let path = r.string(260)?;
                        let attachment_id = r.u32()?;
                        r.position = outer_end;
                        model.attachments.push(Attachment {
                            node,
                            path,
                            attachment_id,
                        });
                    }
                }
                b"EVTS" => {
                    while r.remaining() > 0 {
                        let node = parse_node(&mut r)?;
                        let tag = r.bytes(4)?;
                        if tag != b"KEVT" {
                            return Err(FormatError::Corrupt("expected KEVT in EVTS"));
                        }
                        let count = r.u32()? as usize;
                        r.check_count(count, 4)?;
                        let global = r.i32()?;
                        // `KEVT` is a list of firing **frames** (one u32 each,
                        // no per-key time word): `count`, `GlobalSeqId`, then
                        // `count` frames (`mdx-m3-viewer/src/parsers/mdlx/
                        // eventobject.ts:29-33`).
                        let mut frames = Vec::with_capacity(count);
                        for _ in 0..count {
                            frames.push(r.u32()?);
                        }
                        model.event_objects.push(EventObject {
                            track: EventTrack {
                                frames,
                                global_sequence: (global != -1).then_some(global as u32),
                            },
                            node: node.clone(),
                        });
                        model.events.push(node);
                    }
                }
                b"PREM" => {
                    while r.remaining() > 0 {
                        let outer_start = r.position;
                        let outer_size = r.u32()? as usize;
                        let outer_end = outer_start
                            .checked_add(outer_size)
                            .ok_or(FormatError::Corrupt("particle emitter size overflow"))?;
                        if outer_end > r.data.len() {
                            return Err(FormatError::Corrupt(
                                "particle emitter size exceeds chunk",
                            ));
                        }
                        let node = parse_node(&mut r)?;
                        let mut emitter = ParticleEmitter {
                            node,
                            emission_rate: r.f32()?,
                            gravity: r.f32()?,
                            longitude: r.f32()?,
                            latitude: r.f32()?,
                            path: r.string(260)?,
                            life_span: r.f32()?,
                            speed: r.f32()?,
                            emission_rate_track: None,
                            gravity_track: None,
                            longitude_track: None,
                            latitude_track: None,
                            life_span_track: None,
                            speed_track: None,
                            visibility_track: None,
                        };
                        while r.position + 8 <= outer_end {
                            let tag: [u8; 4] = r.tag()?;
                            match &tag {
                                b"KPEE" => {
                                    emitter.emission_rate_track = Some(parse_track(&mut r, |r| r.f32())?)
                                }
                                b"KPEG" => emitter.gravity_track = Some(parse_track(&mut r, |r| r.f32())?),
                                b"KPLN" => {
                                    emitter.longitude_track = Some(parse_track(&mut r, |r| r.f32())?)
                                }
                                b"KPLT" => {
                                    emitter.latitude_track = Some(parse_track(&mut r, |r| r.f32())?)
                                }
                                b"KPEL" => {
                                    emitter.life_span_track = Some(parse_track(&mut r, |r| r.f32())?)
                                }
                                b"KPES" => emitter.speed_track = Some(parse_track(&mut r, |r| r.f32())?),
                                b"KPEV" => {
                                    emitter.visibility_track = Some(parse_track(&mut r, |r| r.f32())?)
                                }
                                _ => break,
                            }
                        }
                        r.position = outer_end;
                        model.particle_emitters.push(emitter);
                    }
                }
                b"PRE2" => {
                    while r.remaining() > 0 {
                        let outer_start = r.position;
                        let outer_size = r.u32()? as usize;
                        let outer_end = outer_start
                            .checked_add(outer_size)
                            .ok_or(FormatError::Corrupt("particle emitter 2 size overflow"))?;
                        if outer_end > r.data.len() {
                            return Err(FormatError::Corrupt(
                                "particle emitter 2 size exceeds chunk",
                            ));
                        }
                        let node = parse_node(&mut r)?;
                        let mut emitter = ParticleEmitter2 {
                            node,
                            speed: r.f32()?,
                            variation: r.f32()?,
                            latitude: r.f32()?,
                            gravity: r.f32()?,
                            life_span: r.f32()?,
                            emission_rate: r.f32()?,
                            width: r.f32()?,
                            length: r.f32()?,
                            filter_mode: r.u32()?,
                            rows: r.u32()?,
                            columns: r.u32()?,
                            head_or_tail: r.u32()?,
                            tail_length: r.f32()?,
                            time_middle: r.f32()?,
                            segment_colors: [
                                read_vec3(&mut r)?,
                                read_vec3(&mut r)?,
                                read_vec3(&mut r)?,
                            ],
                            segment_alphas: [r.u8()?, r.u8()?, r.u8()?],
                            segment_scaling: [r.f32()?, r.f32()?, r.f32()?],
                            head_intervals: [
                                [r.u32()?, r.u32()?, r.u32()?],
                                [r.u32()?, r.u32()?, r.u32()?],
                            ],
                            tail_intervals: [
                                [r.u32()?, r.u32()?, r.u32()?],
                                [r.u32()?, r.u32()?, r.u32()?],
                            ],
                            texture_id: r.i32()?,
                            squirt: r.u32()?,
                            priority_plane: r.i32()?,
                            replaceable_id: r.u32()?,
                            speed_track: None,
                            variation_track: None,
                            latitude_track: None,
                            gravity_track: None,
                            visibility_track: None,
                            emission_rate_track: None,
                            width_track: None,
                            length_track: None,
                        };
                        while r.position + 8 <= outer_end {
                            let tag: [u8; 4] = r.tag()?;
                            match &tag {
                                b"KP2S" => emitter.speed_track = Some(parse_track(&mut r, |r| r.f32())?),
                                b"KP2R" => {
                                    emitter.variation_track = Some(parse_track(&mut r, |r| r.f32())?)
                                }
                                b"KP2L" => {
                                    emitter.latitude_track = Some(parse_track(&mut r, |r| r.f32())?)
                                }
                                b"KP2G" => {
                                    emitter.gravity_track = Some(parse_track(&mut r, |r| r.f32())?)
                                }
                                b"KP2V" => {
                                    emitter.visibility_track = Some(parse_track(&mut r, |r| r.f32())?)
                                }
                                b"KP2E" => {
                                    emitter.emission_rate_track = Some(parse_track(&mut r, |r| r.f32())?)
                                }
                                b"KP2N" => emitter.width_track = Some(parse_track(&mut r, |r| r.f32())?),
                                b"KP2W" => emitter.length_track = Some(parse_track(&mut r, |r| r.f32())?),
                                _ => break,
                            }
                        }
                        r.position = outer_end;
                        model.particle_emitters2.push(emitter);
                    }
                }
                b"RIBB" => {
                    while r.remaining() > 0 {
                        let outer_start = r.position;
                        let outer_size = r.u32()? as usize;
                        let outer_end = outer_start
                            .checked_add(outer_size)
                            .ok_or(FormatError::Corrupt("ribbon emitter size overflow"))?;
                        if outer_end > r.data.len() {
                            return Err(FormatError::Corrupt(
                                "ribbon emitter size exceeds chunk",
                            ));
                        }
                        let node = parse_node(&mut r)?;
                        // 52 byte body: 3 floats + colour(3) + lifeSpan +
                        // textureSlot/emissionRate/rows/columns/materialId +
                        // gravity. The donor's `getByteLength()` reports 56;
                        // the file layout (verified on FrostNovaTarget.mdx,
                        // SludgeMissile.mdx) is 52 followed by the KR** tracks.
                        let mut emitter = RibbonEmitter {
                            node,
                            height_above: r.f32()?,
                            height_below: r.f32()?,
                            alpha: r.f32()?,
                            color: read_vec3(&mut r)?,
                            life_span: r.f32()?,
                            texture_slot: r.u32()?,
                            emission_rate: r.u32()?,
                            rows: r.u32()?,
                            columns: r.u32()?,
                            material_id: r.i32()?,
                            gravity: r.f32()?,
                            height_above_track: None,
                            height_below_track: None,
                            alpha_track: None,
                            color_track: None,
                            texture_slot_track: None,
                            visibility_track: None,
                        };
                        while r.position + 8 <= outer_end {
                            let tag: [u8; 4] = r.tag()?;
                            match &tag {
                                b"KRHA" => {
                                    emitter.height_above_track = Some(parse_track(&mut r, |r| r.f32())?)
                                }
                                b"KRHB" => {
                                    emitter.height_below_track = Some(parse_track(&mut r, |r| r.f32())?)
                                }
                                b"KRAL" => emitter.alpha_track = Some(parse_track(&mut r, |r| r.f32())?),
                                b"KRCO" => emitter.color_track = Some(parse_track(&mut r, read_vec3)?),
                                b"KRTX" => emitter.texture_slot_track = Some(parse_track(&mut r, |r| r.u32())?),
                                b"KRVS" => {
                                    emitter.visibility_track = Some(parse_track(&mut r, |r| r.f32())?)
                                }
                                _ => break,
                            }
                        }
                        r.position = outer_end;
                        model.ribbon_emitters.push(emitter);
                    }
                }
                b"PIVT" => {
                    while r.remaining() >= 12 {
                        model.pivots.push(Vec3::new(r.f32()?, r.f32()?, r.f32()?));
                    }
                }
                _ => model.raw_chunks.push((tag, data.to_vec())),
            }
        }
        if model.version != 800 {
            return Err(FormatError::Unsupported(
                "MDX version != 800 (classic only)",
            ));
        }
        Ok(model)
    }

    pub fn sequence(&self, name: &str) -> Option<&Sequence> {
        self.sequences
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    pub fn total_faces(&self) -> usize {
        self.geosets.iter().map(|g| g.faces.len() / 3).sum()
    }

    /// Walk the chunk list without interpreting payloads.
    ///
    /// The single chunk walker of the base: `wc3-re::mdx::chunks` projects this
    /// and the animation audit CLI calls it directly instead of re-walking
    /// bytes.
    pub fn chunks(bytes: &[u8]) -> Result<Vec<Chunk>, FormatError> {
        if bytes.first_chunk::<4>() != Some(b"MDLX") {
            return Err(FormatError::UnknownMagic([0; 4]));
        }
        let mut out = Vec::new();
        let mut position = 4usize;
        while let Some(header) = bytes
            .get(position..)
            .and_then(|rest| rest.first_chunk::<8>().copied())
        {
            let tag: [u8; 4] = [header[0], header[1], header[2], header[3]];
            let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
            let payload = position + 8;
            let end = payload
                .checked_add(size)
                .ok_or(FormatError::Corrupt("chunk size overflow"))?;
            if end > bytes.len() {
                return Err(FormatError::Corrupt("chunk runs past EOF"));
            }
            out.push(Chunk {
                tag,
                offset: payload,
                size,
            });
            position = end;
        }
        Ok(out)
    }

    /// Number of node objects that own a pivot entry.
    ///
    /// Classic MDX stores one pivot for every node object: bones, helpers,
    /// attachments, events, lights, particle emitters, ribbons and collision
    /// shapes. Cameras are not nodes and own no pivot. `PIVT.len()` must equal
    /// this value (542/542 models in the local corpus).
    ///
    /// Layout rules (see the node chunks in [`MdxModel::parse`]):
    /// * `BONE`/`HELP`/`EVTS`/`CLID` start with the generic node block (which
    ///   carries its own inclusive size) followed by type-specific data,
    /// * `ATCH`/`LITE`/`PREM`/`PRE2`/`RIBB`/`CAMS` wrap the node block in an
    ///   outer inclusive size,
    /// * `CAMS` contributes no pivot.
    pub fn node_object_count(bytes: &[u8]) -> Result<usize, FormatError> {
        let mut count = 0usize;
        for chunk in Self::chunks(bytes)? {
            let end = chunk.offset + chunk.size;
            let mut r = Reader::new(bytes, chunk.offset);
            match &chunk.tag {
                b"BONE" | b"HELP" | b"EVTS" | b"CLID" => {
                    while r.position < end {
                        let start = r.position;
                        let node_size = r.u32()? as usize;
                        if node_size < 96 || start.saturating_add(node_size) > end {
                            return Err(FormatError::Corrupt("implausible node size"));
                        }
                        // name[80] + object_id[4]; the id is not needed here.
                        r.skip(84)?;
                        r.position = start + node_size;
                        match &chunk.tag {
                            b"BONE" => {
                                r.skip(8)?; // geoset_id + geoset_animation_id
                            }
                            b"EVTS" => {
                                if r.bytes(4)? != b"KEVT" {
                                    return Err(FormatError::Corrupt("EVTS missing KEVT"));
                                }
                                let frames = r.u32()? as usize;
                                r.check_count(frames, 4)?;
                                r.skip(4 + frames * 4)?; // global sequence id + frames
                            }
                            b"CLID" => {
                                let shape = r.u32()?;
                                r.skip(12)?; // first vertex
                                if shape != 2 {
                                    r.skip(12)?; // second vertex (box/plane/cylinder)
                                }
                                if shape == 2 || shape == 3 {
                                    r.skip(4)?; // bounds radius (sphere/cylinder)
                                }
                            }
                            _ => {}
                        }
                        count += 1;
                    }
                }
                b"ATCH" | b"LITE" | b"PREM" | b"PRE2" | b"RIBB" | b"CAMS" => {
                    while r.position < end {
                        let start = r.position;
                        let outer = r.u32()? as usize;
                        if outer < 96 || start.saturating_add(outer) > end {
                            return Err(FormatError::Corrupt("implausible object size"));
                        }
                        if &chunk.tag != b"CAMS" {
                            count += 1;
                        }
                        r.position = start + outer;
                    }
                }
                _ => {}
            }
        }
        Ok(count)
    }
}

/// One MDX chunk header. `offset` is the payload offset (after tag + size).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk {
    pub tag: [u8; 4],
    pub offset: usize,
    pub size: usize,
}

fn parse_geoset_animations(
    r: &mut Reader<'_>,
) -> Result<Vec<GeosetAnimation>, FormatError> {
    let mut out = Vec::new();
    // GEOA is a sequence of inclusive-sized entries; the first u32 is the
    // first entry's size (not a chunk total).
    while r.remaining() >= 4 {
        let start = r.position;
        let inclusive = r.u32()? as usize;
        if inclusive < 28 {
            return Err(FormatError::Corrupt("geoset animation smaller than its header"));
        }
        let end = start
            .checked_add(inclusive)
            .ok_or(FormatError::Corrupt("geoset animation size overflow"))?;
        if end > r.data.len() {
            return Err(FormatError::Corrupt("geoset animation size exceeds chunk"));
        }
        let alpha = r.f32()?;
        let flags = r.u32()?;
        // Stored BGR on disk, like the exporter's `Color` writer; sizzle once
        // here so every consumer sees RGB (`KGAC` track values are already RGB,
        // which matches the donor's asymmetry).
        let color = sizzle_bgr(read_vec3(r)?);
        let geoset_id = r.u32()?;
        let mut alpha_track = None;
        let mut color_track = None;
        while r.position + 8 <= end {
            let tag: [u8; 4] = r.tag()?;
            match &tag {
                b"KGAO" => alpha_track = Some(parse_track(r, |r| r.f32())?),
                b"KGAC" => color_track = Some(parse_track(r, read_vec3)?),
                _ => {
                    r.position = end;
                    break;
                }
            }
        }
        r.position = end;
        out.push(GeosetAnimation {
            geoset_id,
            alpha,
            flags,
            color,
            alpha_track,
            color_track,
        });
    }
    Ok(out)
}

fn parse_node(r: &mut Reader<'_>) -> Result<Node, FormatError> {
    let start = r.position;
    let inclusive_size = r.u32()? as usize;
    if inclusive_size < 96 {
        return Err(FormatError::Corrupt(
            "node size smaller than the node header",
        ));
    }
    let end = start
        .checked_add(inclusive_size)
        .ok_or(FormatError::Corrupt("node size overflow"))?;
    if end > r.data.len() {
        return Err(FormatError::Corrupt("node size exceeds chunk"));
    }
    let name = r.string(80)?;
    let object_id = r.u32()?;
    let parent_id = r.i32()?;
    let flags = r.u32()?;
    let mut translation = Track {
        interpolation: Interpolation::None,
        global_sequence: None,
        keys: Vec::new(),
        static_value: None,
    };
    let mut rotation = Track {
        interpolation: Interpolation::None,
        global_sequence: None,
        keys: Vec::new(),
        static_value: None,
    };
    let mut scaling = Track {
        interpolation: Interpolation::None,
        global_sequence: None,
        keys: Vec::new(),
        static_value: None,
    };
    while r.position + 4 <= end {
        let tag: [u8; 4] = r.tag()?;
        match &tag {
            b"KGTR" => translation = parse_track(r, read_vec3)?,
            b"KGRT" => rotation = parse_track(r, read_quat)?,
            b"KGSC" => scaling = parse_track(r, read_vec3)?,
            _ => {
                // Node types carry extra tracks (KEVT, KATV, ...); the node's
                // inclusive size lets us skip them safely.
                r.position = end;
                break;
            }
        }
    }
    r.position = end;
    Ok(Node {
        name,
        object_id,
        parent_id,
        flags,
        translation,
        rotation,
        scaling,
    })
}

fn parse_track<T: AnimValue>(
    r: &mut Reader<'_>,
    read: fn(&mut Reader<'_>) -> Result<T, FormatError>,
) -> Result<Track<T>, FormatError> {
    let count = r.u32()? as usize;
    if count > 1_000_000 {
        return Err(FormatError::Corrupt("implausible key count"));
    }
    r.check_count(count, 4)?;
    if count == 0 {
        let value = read(r)?;
        return Ok(Track {
            interpolation: Interpolation::None,
            global_sequence: None,
            keys: Vec::new(),
            static_value: Some(value),
        });
    }
    let interpolation = match r.u32()? {
        0 => Interpolation::None,
        1 => Interpolation::Linear,
        2 => Interpolation::Hermite,
        3 => Interpolation::Bezier,
        _ => return Err(FormatError::Corrupt("invalid interpolation type")),
    };
    let global = r.u32()?;
    let global_sequence = (global != 0xFFFF_FFFF).then_some(global);
    let mut keys = Vec::with_capacity(count);
    for _ in 0..count {
        let time = r.u32()?;
        let value = read(r)?;
        let (in_tan, out_tan) = if matches!(
            interpolation,
            Interpolation::Hermite | Interpolation::Bezier
        ) {
            (read(r)?, read(r)?)
        } else {
            (value, value)
        };
        keys.push(Key {
            time,
            value,
            in_tan,
            out_tan,
        });
    }
    // Keyframe times are `int32` in the file: mdx-m3-viewer reads them with
    // `stream.readInt32()` (`src/parsers/mdlx/animations.ts:40`), and shipped
    // Blizzard models use the negative range — Hydralisk's `BlizParticlePuff`
    // emission track starts at frame `-100` (`0xFFFF_FF9C`) before its first
    // in-sequence key at 3900. Ordering is therefore **signed**: an unsigned
    // comparison turns every pre-start key into an "inversion", and a `u32`
    // sort then moves it past the sequence end, changing what the track emits at
    // the end of an animation.
    //
    // A stable signed sort keeps file order for equal times and is the identity
    // on the shipped corpus after the pre-start keys (`-100`, `-133`, `-167`,
    // `-33`) are read as negative; a genuine inversion is reported, never
    // silently reordered into a different curve.
    let inversion = keys
        .windows(2)
        .find(|pair| (pair[1].time as i32) < (pair[0].time as i32))
        .map(|pair| (pair[0].time as i32, pair[1].time as i32));
    if let Some((previous, next)) = inversion {
        let key_count = keys.len();
        crate::diag::note("mdx_track_unsorted", || {
            format!("keys={key_count} inversion={previous}>{next}")
        });
    }
    keys.sort_by_key(|key| key.time as i32);
    Ok(Track {
        interpolation,
        global_sequence,
        keys,
        static_value: None,
    })
}

fn read_vec3(r: &mut Reader<'_>) -> Result<Vec3, FormatError> {
    Ok(Vec3::new(r.f32()?, r.f32()?, r.f32()?))
}

/// `GEOA` stores its static colour **BGR**, unlike `PRE2`/`RIBB` colours.
///
/// Evidence: `mdx-m3-viewer/src/viewer/handlers/mdx/geosetanimation.ts:16` and
/// `HiveWE/src/file_formats/mdx/mdx_reader.cpp:323` both swap only the geoset
/// animation colour; `mdx-m3-viewer/src/viewer/handlers/mdx/particleemitter2object.ts`
/// uses the emitter's colour words verbatim. Confirmed on the corpus: HolyBolt
/// emitter colour `(1.0, 0.918, 0.376)` is gold as RGB and blue when swapped,
/// WaterElemental `(0.012, 0.408, 1.0)` is water-blue as RGB, FrostNova ribbon
/// `(0.29, 0.671, 0.965)` is frost-blue as RGB. So only `GEOA` is sizzled.
fn sizzle_bgr(color: Vec3) -> Vec3 {
    Vec3::new(color.z, color.y, color.x)
}

fn read_quat(r: &mut Reader<'_>) -> Result<Quat, FormatError> {
    let x = r.f32()?;
    let y = r.f32()?;
    let z = r.f32()?;
    let w = r.f32()?;
    Ok(Quat::from_xyzw(x, y, z, w).normalize())
}

fn parse_materials(
    r: &mut Reader<'_>,
    version: u32,
    out: &mut Vec<Material>,
) -> Result<(), FormatError> {
    if version != 800 {
        return Err(FormatError::Unsupported("materials: MDX version != 800"));
    }
    while r.remaining() >= 4 {
        let material_start = r.position;
        let inclusive = r.u32()? as usize;
        if inclusive < 16 {
            return Err(FormatError::Corrupt(
                "material size smaller than its header",
            ));
        }
        let material_end = material_start
            .checked_add(inclusive)
            .ok_or(FormatError::Corrupt("material size overflow"))?;
        if material_end > r.data.len() {
            return Err(FormatError::Corrupt("material size exceeds chunk"));
        }
        let priority = r.i32()?;
        let flags = r.u32()?;
        let shader = r.bytes(4)?;
        if shader != b"LAYS" {
            return Err(FormatError::Corrupt("expected LAYS in material"));
        }
        let layer_count = r.u32()?;
        r.check_count(layer_count as usize, 4)?;
        let mut layers = Vec::with_capacity(layer_count as usize);
        for _ in 0..layer_count {
            let layer_start = r.position;
            let layer_size = r.u32()? as usize;
            if layer_size < 4 {
                return Err(FormatError::Corrupt("layer size smaller than its header"));
            }
            let layer_end = layer_start
                .checked_add(layer_size)
                .ok_or(FormatError::Corrupt("layer size overflow"))?;
            if layer_end > r.data.len() {
                return Err(FormatError::Corrupt("layer size exceeds chunk"));
            }
            let blend_mode = r.u32()?;
            let shading_flags = r.u32()?;
            let texture_id = r.u32()?;
            let texture_animation_id = r.u32()?;
            let coord_id = r.u32()?;
            let alpha = r.f32()?;
            let mut alpha_track = None;
            let mut texture_id_track = None;
            while r.position + 4 <= layer_end {
                let tag: [u8; 4] = r.tag()?;
                match &tag {
                    b"KMTA" => alpha_track = Some(parse_track(r, |r| r.f32())?),
                    b"KMTF" => texture_id_track = Some(parse_track(r, |r| r.u32())?),
                    _ => return Err(FormatError::Corrupt("unknown layer track")),
                }
            }
            r.position = layer_end;
            layers.push(Layer {
                blend_mode,
                shading_flags,
                texture_id,
                texture_animation_id,
                coord_id,
                alpha,
                alpha_track,
                texture_id_track,
            });
        }
        r.position = material_end;
        out.push(Material {
            priority,
            flags,
            layers,
        });
    }
    Ok(())
}

fn parse_geoset(r: &mut Reader<'_>, version: u32) -> Result<Geoset, FormatError> {
    let start = r.position;
    let inclusive = r.u32()? as usize;
    if inclusive < 4 {
        return Err(FormatError::Corrupt("geoset size smaller than its header"));
    }
    let end = start
        .checked_add(inclusive)
        .ok_or(FormatError::Corrupt("geoset size overflow"))?;
    if end > r.data.len() {
        return Err(FormatError::Corrupt("geoset size exceeds chunk"));
    }

    expect_tag(r, b"VRTX")?;
    let vertex_count = r.u32()? as usize;
    r.check_count(vertex_count, 12)?;
    let mut vertices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        vertices.push(read_vec3(r)?);
    }
    expect_tag(r, b"NRMS")?;
    let normal_count = r.u32()? as usize;
    r.check_count(normal_count, 12)?;
    let mut normals = Vec::with_capacity(normal_count);
    for _ in 0..normal_count {
        normals.push(read_vec3(r)?);
    }
    expect_tag(r, b"PTYP")?;
    let face_type_count = r.u32()? as usize;
    r.check_count(face_type_count, 4)?;
    let mut face_type_groups = Vec::with_capacity(face_type_count);
    for _ in 0..face_type_count {
        face_type_groups.push(r.u32()?);
    }
    expect_tag(r, b"PCNT")?;
    let face_group_count = r.u32()? as usize;
    r.check_count(face_group_count, 4)?;
    let mut face_groups = Vec::with_capacity(face_group_count);
    for _ in 0..face_group_count {
        face_groups.push(r.u32()?);
    }
    expect_tag(r, b"PVTX")?;
    let face_index_count = r.u32()? as usize;
    r.check_count(face_index_count, if version >= 900 { 4 } else { 2 })?;
    let mut faces = Vec::with_capacity(face_index_count);
    if version >= 900 {
        for _ in 0..face_index_count {
            faces.push(r.u32()? as u16);
        }
    } else {
        for _ in 0..face_index_count {
            faces.push(r.u16()?);
        }
    }
    expect_tag(r, b"GNDX")?;
    let group_count = r.u32()? as usize;
    r.check_count(group_count, 1)?;
    let mut vertex_groups = Vec::with_capacity(group_count);
    for _ in 0..group_count {
        vertex_groups.push(r.u8()?);
    }
    expect_tag(r, b"MTGC")?;
    let matrix_group_count = r.u32()? as usize;
    r.check_count(matrix_group_count, 4)?;
    let mut matrix_groups = Vec::with_capacity(matrix_group_count);
    for _ in 0..matrix_group_count {
        matrix_groups.push(r.u32()?);
    }
    expect_tag(r, b"MATS")?;
    let matrix_index_count = r.u32()? as usize;
    r.check_count(matrix_index_count, 4)?;
    let mut matrix_indices = Vec::with_capacity(matrix_index_count);
    for _ in 0..matrix_index_count {
        matrix_indices.push(r.u32()?);
    }
    let material_id = r.u32()?;
    let selection_group = r.u32()?;
    let selection_flags = r.u32()?;
    let (lod, lod_name) = if version > 800 {
        (r.u32()?, r.string(80)?)
    } else {
        (0, String::new())
    };
    let bounds = Extent::read(r)?;
    let extents_count = r.u32()? as usize;
    r.check_count(extents_count, 28)?;
    let mut sequence_bounds = Vec::with_capacity(extents_count);
    for _ in 0..extents_count {
        sequence_bounds.push(Extent::read(r)?);
    }
    let mut tag: [u8; 4] = r.tag()?;
    let mut skin = Vec::new();
    if &tag == b"TANG" {
        let count = r.u32()? as usize;
        r.check_count(count, 16)?;
        r.skip(count * 16)?;
        tag = r.tag()?;
    }
    if &tag == b"SKIN" {
        let count = r.u32()? as usize;
        if version >= 1400 {
            for _ in 0..count {
                skin.push(r.u16()?);
            }
        } else {
            for _ in 0..count {
                skin.push(r.u8()? as u16);
            }
        }
        tag = r.tag()?;
    }
    if &tag != b"UVAS" {
        return Err(FormatError::Corrupt("expected UVAS tag"));
    }
    let uv_set_count = r.u32()? as usize;
    // Every other geoset count is validated against the remaining bytes before
    // it sizes a `Vec`; this one was not, so four forged bytes asked for 103 GB
    // (WC3-MDX-001). Each `UVBS` needs at least its tag and count.
    r.check_count(uv_set_count, 8)?;
    if uv_set_count > MAX_UV_SETS {
        return Err(FormatError::Corrupt("geoset UV set count out of range"));
    }
    let mut uv_sets = Vec::with_capacity(uv_set_count);
    for _ in 0..uv_set_count {
        expect_tag(r, b"UVBS")?;
        let count = r.u32()? as usize;
        r.check_count(count, 8)?;
        let mut uvs = Vec::with_capacity(count);
        for _ in 0..count {
            uvs.push(Vec2::new(r.f32()?, r.f32()?));
        }
        uv_sets.push(uvs);
    }
    r.position = end;
    Ok(Geoset {
        vertices,
        normals,
        face_type_groups,
        face_groups,
        faces,
        vertex_groups,
        matrix_groups,
        matrix_indices,
        material_id,
        selection_group,
        selection_flags,
        lod,
        lod_name,
        bounds,
        sequence_bounds,
        uv_sets,
        skin,
    })
}

fn expect_tag(r: &mut Reader<'_>, expected: &[u8; 4]) -> Result<(), FormatError> {
    let tag: [u8; 4] = r.tag()?;
    if &tag != expected {
        return Err(FormatError::Corrupt("unexpected geoset tag"));
    }
    Ok(())
}

struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8], position: usize) -> Self {
        Self { data, position }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.position)
    }

    fn check_count(&self, count: usize, stride: usize) -> Result<(), FormatError> {
        let bytes = count
            .checked_mul(stride)
            .ok_or(FormatError::Corrupt("count overflow"))?;
        if bytes > self.remaining() {
            return Err(FormatError::Corrupt("count exceeds remaining chunk data"));
        }
        Ok(())
    }

    fn bytes(&mut self, len: usize) -> Result<&'a [u8], FormatError> {
        let end = self
            .position
            .checked_add(len)
            .ok_or(FormatError::Corrupt("offset overflow"))?;
        let slice = self
            .data
            .get(self.position..end)
            .ok_or(FormatError::Corrupt("read past end of data"))?;
        self.position = end;
        Ok(slice)
    }

    /// Read exactly `N` bytes as an array.
    ///
    /// The `first_chunk` split is total: the length cannot lie, so no caller
    /// needs an `expect` after a checked read.
    fn arr<const N: usize>(&mut self) -> Result<[u8; N], FormatError> {
        let slice = self.bytes(N)?;
        slice
            .first_chunk::<N>()
            .copied()
            .ok_or(FormatError::Corrupt("short read"))
    }

    fn skip(&mut self, len: usize) -> Result<(), FormatError> {
        self.bytes(len).map(|_| ())
    }

    fn u8(&mut self) -> Result<u8, FormatError> {
        Ok(self.arr::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, FormatError> {
        Ok(u16::from_le_bytes(self.arr::<2>()?))
    }

    fn u32(&mut self) -> Result<u32, FormatError> {
        Ok(u32::from_le_bytes(self.arr::<4>()?))
    }

    fn i32(&mut self) -> Result<i32, FormatError> {
        Ok(i32::from_le_bytes(self.arr::<4>()?))
    }

    fn f32(&mut self) -> Result<f32, FormatError> {
        Ok(f32::from_le_bytes(self.arr::<4>()?))
    }

    /// Four-byte chunk tag (`VERS`, `MODL`, …).
    fn tag(&mut self) -> Result<[u8; 4], FormatError> {
        self.arr::<4>()
    }

    fn string(&mut self, len: usize) -> Result<String, FormatError> {
        let bytes = self.bytes(len)?;
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Fixture lookup used by the merge-parity tests: `analysis/war3_samples`
    /// first (small tree), then the full extracted MPQ tree.
    fn fixture_path(relative: &str) -> Option<std::path::PathBuf> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for tree in ["analysis/war3_samples", "analysis/war3_full"] {
            let candidate = root.join(tree).join(relative);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        None
    }

    fn fixture() -> Option<Vec<u8>> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_samples/Units/Human/Footman/Footman.mdx");
        fs::read(path).ok()
    }

    #[test]
    fn parses_real_footman_model() {
        let Some(bytes) = fixture() else {
            eprintln!("skipping: Footman.mdx fixture missing");
            return;
        };
        let model = MdxModel::parse(&bytes).expect("parse MDX");
        assert_eq!(model.version, 800);
        assert_eq!(model.name, "Footman");
        assert!(!model.sequences.is_empty());
        assert!(!model.textures.is_empty());
        assert!(!model.materials.is_empty());
        assert!(!model.geosets.is_empty());
        assert!(!model.bones.is_empty());
        assert!(!model.pivots.is_empty());
        let geoset = &model.geosets[0];
        assert!(geoset.vertices.len() > 100);
        assert!(geoset.faces.len() > 100);
        assert!(model.total_faces() > 100);
    }

    #[test]
    fn chunk_list_covers_the_whole_file() {
        // `MdxModel::chunks` is the single walker of the base (`wc3-re::mdx`
        // projects it). It must describe the file exactly: MDLX magic, then
        // contiguous header+payload records, no gap and no overlap.
        for relative in [
            "Units/Human/Footman/Footman.mdx",
            "Buildings/Human/TownHall/TownHall.mdx",
        ] {
            let Some(path) = fixture_path(relative) else {
                eprintln!("skipping {relative}: fixture missing");
                continue;
            };
            let bytes = fs::read(path).expect("read fixture");
            let chunks = MdxModel::chunks(&bytes).expect("chunk walk");
            assert!(!chunks.is_empty(), "{relative}");
            let mut expected = 4usize;
            for chunk in &chunks {
                assert_eq!(chunk.offset, expected + 8, "{relative}: payload offset");
                assert!(
                    chunk.tag.iter().all(|byte| byte.is_ascii_graphic()),
                    "{relative}: tag {:?}",
                    chunk.tag
                );
                expected = chunk.offset + chunk.size;
            }
            assert_eq!(expected, bytes.len(), "{relative}: chunk list must reach EOF");
        }
    }

    #[test]
    fn particle_emitters2_parse_with_tracks_and_atlas() {
        // FrostNovaTarget: 4 `PRE2` emitters, one `KP2E` (emission rate) track,
        // additive filter, 0.5 s life span, 1x1 atlas on the first emitter.
        let Some(path) = fixture_path("Abilities/Spells/Undead/FrostNova/FrostNovaTarget.mdx")
        else {
            eprintln!("skipping: FrostNovaTarget.mdx fixture missing");
            return;
        };
        let bytes = fs::read(path).expect("read fixture");
        let model = MdxModel::parse(&bytes).expect("parse");
        assert_eq!(model.particle_emitters2.len(), 4, "PRE2 emitter count");
        let emitter = &model.particle_emitters2[0];
        assert!(emitter.node.name.starts_with("BlizParticle"), "{}", emitter.node.name);
        assert_eq!(emitter.filter(), ParticleFilter::Additive);
        assert_eq!(emitter.head_or_tail(), HeadOrTail::Tail);
        assert!((emitter.life_span - 0.5).abs() < 1e-6);
        assert!((emitter.time_middle - 0.75).abs() < 1e-6);
        assert!(emitter.rows >= 1 && emitter.columns >= 1);
        assert!(
            emitter.emission_rate_track.is_some(),
            "KP2E must survive parsing (it drives the emission accumulator)"
        );
        assert!(emitter.billboarded(), "no XYQuad flag on this emitter");
        // Colours are stored RGB (not BGR): the frost wave is blue.
        let blue = emitter.segment_colors[1];
        assert!(blue.z > blue.x, "blue channel must dominate: {blue:?}");
        for emitter in &model.particle_emitters2 {
            assert!(emitter.columns > 0 && emitter.rows > 0);
            assert!(emitter.life_span > 0.0);
        }
    }

    #[test]
    fn ribbon_emitters_parse_the_52_byte_body() {
        // The donor's `getByteLength()` reports 56 bytes for a ribbon emitter,
        // the on-disk body is 52 (verified on two corpus models); this test
        // fails with a chunk-overrun error if the field order or size drifts.
        let Some(path) = fixture_path("Abilities/Spells/Undead/FrostNova/FrostNovaTarget.mdx")
        else {
            eprintln!("skipping: FrostNovaTarget.mdx fixture missing");
            return;
        };
        let bytes = fs::read(path).expect("read fixture");
        let model = MdxModel::parse(&bytes).expect("parse");
        assert_eq!(model.ribbon_emitters.len(), 4, "RIBB count");
        let ribbon = &model.ribbon_emitters[0];
        assert!((ribbon.height_above - 18.0).abs() < 1e-4);
        assert!((ribbon.height_below - 18.0).abs() < 1e-4);
        assert!((ribbon.alpha - 0.7).abs() < 1e-4);
        assert!((ribbon.life_span - 0.4).abs() < 1e-4);
        assert_eq!(ribbon.emission_rate, 30);
        assert_eq!(ribbon.columns, 1);
        assert_eq!(ribbon.material_id, 0);
        assert!(ribbon.visibility_track.is_some(), "KRVS must be parsed");
        assert!(ribbon.color.z > ribbon.color.x, "frost ribbon is blue");
        assert_eq!(model.materials.len() > ribbon.material_id as usize, true);
    }

    #[test]
    fn event_objects_keep_their_keyed_track() {
        // Footman ships six event objects: one sound (`SNDXDFOO`) and five
        // splat/spawn references, each with a dense `KEVT` array.
        let Some(path) = fixture_path("Units/Human/Footman/Footman.mdx") else {
            eprintln!("skipping: Footman.mdx fixture missing");
            return;
        };
        let bytes = fs::read(path).expect("read fixture");
        let model = MdxModel::parse(&bytes).expect("parse");
        assert_eq!(model.event_objects.len(), 6, "EVTS object count");
        assert_eq!(model.event_objects.len(), model.events.len(), "1:1 with events");
        let mut kinds = Vec::new();
        for event in &model.event_objects {
            assert!(!event.track.frames.is_empty(), "KEVT frames must survive");
            // The frames are ascending, so the last one at or before a frame is
            // well defined (`EventTrack::index_at`).
            assert!(
                event.track.frames.windows(2).all(|pair| pair[0] <= pair[1]),
                "KEVT frames must be sorted: {:?}",
                event.track.frames
            );
            let (kind, id) = event.kind_and_id();
            assert_ne!(kind, EventObjectKind::Unknown, "name {}", event.node.name);
            assert!(!id.is_empty(), "event object id from {}", event.node.name);
            kinds.push(kind);
        }
        assert!(kinds.contains(&EventObjectKind::Sound), "SNDXDFOO is a sound");
        assert!(kinds.contains(&EventObjectKind::Splat), "FPT/SPL footprints");
        assert!(kinds.contains(&EventObjectKind::SpawnModel), "SPN spawns a model");
        assert!(
            model
                .event_objects
                .iter()
                .all(|event| event.track.index_at(u32::MAX).is_some()),
            "every event object exposes its firing frames"
        );
    }

    #[test]
    fn four_core_models_parse_with_effect_chunks_present() {
        // Regression guard for the `KEVT` layout: it is a dense u32 array with
        // no per-key time word. Reading it as a `(time, value)` track pushed the
        // reader 4*count bytes past the event object, so the next `parse_node`
        // read a garbage inclusive size and every model with an `EVTS` chunk
        // failed with "node size exceeds chunk" (the game logged
        // `model_failed=Units\Human\Footman\Footman.mdx: corrupt data`).
        for (relative, pivots, min_geosets, min_sequences) in [
            ("Units/Human/Footman/Footman.mdx", 57usize, 5usize, 10usize),
            ("Units/Human/Peasant/peasant.mdx", 42, 6, 10),
            ("Units/NightElf/Archer/Archer.mdx", 68, 4, 10),
            ("Buildings/Human/TownHall/TownHall.mdx", 81, 30, 10),
        ] {
            let Some(path) = fixture_path(relative) else {
                eprintln!("skipping {relative}: fixture missing");
                continue;
            };
            let bytes = fs::read(path).expect("read fixture");
            let model = MdxModel::parse(&bytes)
                .unwrap_or_else(|error| panic!("{relative}: {error:?}"));
            assert!(
                model.bones.len() >= 10,
                "{relative}: {} bones",
                model.bones.len()
            );
            assert!(
                model.geosets.len() >= min_geosets,
                "{relative}: {} geosets (>= {min_geosets})",
                model.geosets.len()
            );
            assert!(
                model.sequences.len() >= min_sequences,
                "{relative}: {} sequences (>= {min_sequences})",
                model.sequences.len()
            );
            // Exact pivot count: a parser that stops early or runs past an
            // object loses pivots, so this number is the cheapest whole-file
            // integrity check (57/42/68/81 observed in `war3_full`).
            assert_eq!(model.pivots.len(), pivots, "{relative}: pivot count");
            // The chunk walk must still reach EOF after the effect chunks parse.
            let chunks = MdxModel::chunks(&bytes).expect("chunks");
            let end = chunks.last().map(|chunk| chunk.offset + chunk.size).unwrap_or(0);
            assert_eq!(end, bytes.len(), "{relative}: chunk walk reaches EOF");
        }
    }

    #[test]
    fn classic_particle_emitters_parse() {
        let Some(path) = fixture_path("Units/Human/Footman/Footman.mdx") else {
            eprintln!("skipping: Footman.mdx fixture missing");
            return;
        };
        let bytes = fs::read(path).expect("read fixture");
        let model = MdxModel::parse(&bytes).expect("parse");
        // The Footman has no emitters; the assertion that matters is that the
        // chunk set stays empty instead of being mis-parsed from a stray tag.
        assert!(model.particle_emitters.is_empty());
    }

    #[test]
    fn node_object_count_matches_pivot_count_on_real_models() {
        // Pivot contract proven on 542/542 local models by the audit sweep:
        // every node object (bones, helpers, attachments, events, lights,
        // particles, ribbons, collision shapes; cameras excluded) owns a pivot.
        for relative in [
            "Units/Human/Footman/Footman.mdx",
            "Units/NightElf/Archer/Archer.mdx",
            "Units/Human/Peasant/peasant.mdx",
            "Buildings/Human/TownHall/TownHall.mdx",
        ] {
            let Some(path) = fixture_path(relative) else {
                eprintln!("skipping {relative}: fixture missing");
                continue;
            };
            let bytes = fs::read(path).expect("read fixture");
            let model = MdxModel::parse(&bytes).expect("parse");
            let count = MdxModel::node_object_count(&bytes).expect("node object count");
            assert_eq!(
                count,
                model.pivots.len(),
                "{relative}: {count} node objects vs {} pivots",
                model.pivots.len()
            );
        }
    }

    #[test]
    fn footman_material_layers_preserve_order() {
        let Some(bytes) = fixture() else {
            eprintln!("skipping: Footman.mdx fixture missing");
            return;
        };
        let model = MdxModel::parse(&bytes).expect("parse MDX");
        // Material 1 is the team-colour body: layer 0 is the replaceable
        // (team colour) texture with blend mode 0, layer 1 is the diffuse
        // Footman.blp with blend mode 2. Selecting only layer 0 (the old bug)
        // renders an untextured white blob.
        let material = &model.materials[1];
        assert_eq!(material.layers.len(), 2, "team-colour material must keep both layers");
        assert_eq!(material.layers[0].texture_id, 1);
        assert_eq!(material.layers[0].blend_mode, 0);
        assert_eq!(material.layers[1].texture_id, 0);
        assert_eq!(material.layers[1].blend_mode, 2);
        assert!(model.textures[1].replaceable_id != 0, "texture 1 is replaceable");
        assert_eq!(model.textures[0].path, "Textures\\Footman.blp");
    }

    #[test]
    fn sequence_extents_track_every_sequence() {
        let Some(bytes) = fixture() else {
            eprintln!("skipping: Footman.mdx fixture missing");
            return;
        };
        let model = MdxModel::parse(&bytes).expect("parse MDX");
        for (index, geoset) in model.geosets.iter().enumerate() {
            assert_eq!(
                geoset.sequence_bounds.len(),
                model.sequences.len(),
                "geoset {index} lost per-sequence extents"
            );
        }
        // The gutz shell is absent from Stand (extent sentinel) and present in
        // Decay Flesh; the body is present in both.
        let stand = model
            .sequences
            .iter()
            .position(|sequence| sequence.name.starts_with("Stand - 1"))
            .expect("Stand - 1");
        let decay = model
            .sequences
            .iter()
            .position(|sequence| sequence.name == "Decay Flesh")
            .expect("Decay Flesh");
        assert!(model.geosets[0].present_in_sequence(stand));
        assert!(!model.geosets[4].present_in_sequence(stand));
        assert!(model.geosets[4].present_in_sequence(decay));
        assert!(model.geosets[4].sequence_bounds[stand].is_empty());
        assert!(!model.geosets[0].sequence_bounds[stand].is_empty());
    }

    #[test]
    fn attachment_block_reads_260_byte_path() {
        // Minimal MDLX: VERS + one ATCH entry. The attachment path is 260 bytes
        // followed by the attachment id; reading 256+4+4 shifted both fields.
        let path = "Abilities\\Weapons\\FootmanSword\\FootmanSword.mdl";
        let mut chunk = Vec::new();
        let node_size = 96u32;
        let outer_size = 4 + node_size + 260 + 4;
        chunk.extend_from_slice(&outer_size.to_le_bytes());
        chunk.extend_from_slice(&node_size.to_le_bytes());
        let mut name = [0u8; 80];
        name[..9].copy_from_slice(b"Weapon Re");
        chunk.extend_from_slice(&name);
        chunk.extend_from_slice(&3u32.to_le_bytes()); // object id
        chunk.extend_from_slice(&(-1i32).to_le_bytes()); // parent
        chunk.extend_from_slice(&0x100u32.to_le_bytes()); // flags
        let mut path_field = [0u8; 260];
        path_field[..path.len()].copy_from_slice(path.as_bytes());
        chunk.extend_from_slice(&path_field);
        chunk.extend_from_slice(&7u32.to_le_bytes()); // attachment id

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MDLX");
        bytes.extend_from_slice(b"VERS");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&800u32.to_le_bytes());
        bytes.extend_from_slice(b"ATCH");
        bytes.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&chunk);

        let model = MdxModel::parse(&bytes).expect("parse synthetic ATCH");
        assert_eq!(model.attachments.len(), 1);
        let attachment = &model.attachments[0];
        assert_eq!(attachment.path, path);
        assert_eq!(attachment.attachment_id, 7);
        assert_eq!(attachment.node.object_id, 3);
        assert_eq!(attachment.node.parent_id, -1);
        assert_eq!(attachment.node.flags, 0x100);
    }

    #[test]
    fn quat_curves_use_squad_and_hit_their_endpoints() {
        let identity = Quat::IDENTITY;
        let target = Quat::from_rotation_z(2.0);
        // squad(q1, q2, q3, q4, t) = slerp(slerp(q1, q4, t), slerp(q2, q3, t), 2t(1-t)).
        // With identity control quaternions the midpoint is therefore the half
        // of the half rotation (0.5 rad), not the plain slerp midpoint.
        let mid = Quat::hermite(identity, identity, target, identity, 0.5);
        let expected = Quat::from_rotation_z(0.5);
        assert!(
            mid.dot(expected).abs() > 0.9999,
            "squad midpoint {mid:?} != {expected:?}"
        );
        // Non-identity control quaternions must change the curve: with control
        // quaternions equal to a 1 radian turn the midpoint is that turn.
        let control = Quat::from_rotation_z(1.0);
        let bent = Quat::hermite(identity, control, target, control, 0.5);
        assert!(
            bent.dot(control).abs() > 0.9999,
            "control quaternions did not bend the curve, got {bent:?}"
        );
        let start = Quat::hermite(identity, identity, target, identity, 0.0);
        let end = Quat::hermite(identity, identity, target, identity, 1.0);
        assert!(start.dot(identity).abs() > 0.9999);
        assert!(end.dot(target).abs() > 0.9999);
    }

    #[test]
    fn bezier_uses_the_in_tangent_as_the_second_control_point() {
        // p0 = (0,0,0), out tangent = (10,0,0), in tangent = (0,0,0),
        // p1 = (10,0,0). B(0.5) = 0.125*0 + 0.375*10 + 0.375*0 + 0.125*10 = 5.
        let track = Track {
            interpolation: Interpolation::Bezier,
            global_sequence: None,
            keys: vec![
                Key {
                    time: 0,
                    value: Vec3::ZERO,
                    in_tan: Vec3::ZERO,
                    out_tan: Vec3::new(10.0, 0.0, 0.0),
                },
                Key {
                    time: 1000,
                    value: Vec3::new(10.0, 0.0, 0.0),
                    in_tan: Vec3::ZERO,
                    out_tan: Vec3::ZERO,
                },
            ],
            static_value: None,
        };
        let sampled = track.sample(500).expect("sample");
        assert!(
            (sampled.x - 5.0).abs() < 1e-4,
            "bezier midpoint {} != 5.0 (control points are swapped)",
            sampled.x
        );
    }

    #[test]
    fn track_sampling_is_monotonic() {
        let track = Track {
            interpolation: Interpolation::Linear,
            global_sequence: None,
            keys: vec![
                Key {
                    time: 0,
                    value: 0.0f32,
                    in_tan: 0.0,
                    out_tan: 0.0,
                },
                Key {
                    time: 1000,
                    value: 10.0f32,
                    in_tan: 0.0,
                    out_tan: 0.0,
                },
            ],
            static_value: None,
        };
        assert_eq!(track.sample(0), Some(0.0));
        assert!((track.sample(500).unwrap() - 5.0).abs() < 1e-4);
        assert_eq!(track.sample(2000), Some(10.0));
    }

    #[test]
    fn rejects_short_and_bad_magic() {
        assert!(MdxModel::parse(b"MDLX").is_err());
        assert!(MdxModel::parse(b"XXXX").is_err());
    }

    /// The geoset `lod`/`lod_name` fields exist from MDX v900 on. The public
    /// `parse` gate is classic-only (v800), so the branch is proven directly on
    /// the geoset reader: a v901 geoset must expose its detail level, and a v800
    /// geoset must report `0` (the field is simply not in the file).
    #[test]
    fn geoset_lod_fields_follow_the_file_version() {
        fn geoset_bytes(version: u32, lod: u32, lod_name: &str) -> Vec<u8> {
            let mut body = Vec::new();
            body.extend_from_slice(b"VRTX");
            body.extend_from_slice(&3u32.to_le_bytes());
            for _ in 0..3 {
                for component in [0.0f32, 0.0, 0.0] {
                    body.extend_from_slice(&component.to_le_bytes());
                }
            }
            body.extend_from_slice(b"NRMS");
            body.extend_from_slice(&3u32.to_le_bytes());
            for _ in 0..3 {
                for component in [0.0f32, 0.0, 1.0] {
                    body.extend_from_slice(&component.to_le_bytes());
                }
            }
            body.extend_from_slice(b"PTYP");
            body.extend_from_slice(&1u32.to_le_bytes());
            body.extend_from_slice(&4u32.to_le_bytes());
            body.extend_from_slice(b"PCNT");
            body.extend_from_slice(&1u32.to_le_bytes());
            body.extend_from_slice(&3u32.to_le_bytes());
            body.extend_from_slice(b"PVTX");
            body.extend_from_slice(&3u32.to_le_bytes());
            for index in [0u16, 1, 2] {
                if version >= 900 {
                    body.extend_from_slice(&(index as u32).to_le_bytes());
                } else {
                    body.extend_from_slice(&index.to_le_bytes());
                }
            }
            body.extend_from_slice(b"GNDX");
            body.extend_from_slice(&3u32.to_le_bytes());
            body.extend_from_slice(&[0, 0, 0]);
            body.extend_from_slice(b"MTGC");
            body.extend_from_slice(&1u32.to_le_bytes());
            body.extend_from_slice(&1u32.to_le_bytes());
            body.extend_from_slice(b"MATS");
            body.extend_from_slice(&1u32.to_le_bytes());
            body.extend_from_slice(&0u32.to_le_bytes());
            body.extend_from_slice(&0u32.to_le_bytes()); // material
            body.extend_from_slice(&0u32.to_le_bytes()); // selection group
            body.extend_from_slice(&0u32.to_le_bytes()); // selection flags
            if version > 800 {
                body.extend_from_slice(&lod.to_le_bytes());
                let mut name = [0u8; 80];
                name[..lod_name.len()].copy_from_slice(lod_name.as_bytes());
                body.extend_from_slice(&name);
            }
            body.extend_from_slice(&1.0f32.to_le_bytes());
            for _ in 0..6 {
                body.extend_from_slice(&0.0f32.to_le_bytes());
            }
            body.extend_from_slice(&0u32.to_le_bytes()); // no extents
            body.extend_from_slice(b"UVAS");
            body.extend_from_slice(&1u32.to_le_bytes());
            body.extend_from_slice(b"UVBS");
            body.extend_from_slice(&3u32.to_le_bytes());
            for _ in 0..3 {
                body.extend_from_slice(&0.0f32.to_le_bytes());
                body.extend_from_slice(&0.0f32.to_le_bytes());
            }
            let mut out = Vec::new();
            out.extend_from_slice(&((body.len() + 4) as u32).to_le_bytes());
            out.extend_from_slice(&body);
            out
        }

        let bytes = geoset_bytes(901, 1, "LOD1");
        let mut reader = Reader::new(&bytes, 0);
        let geoset = parse_geoset(&mut reader, 901).expect("v901 geoset");
        assert_eq!(geoset.lod, 1, "lod index");
        assert_eq!(geoset.lod_name, "LOD1", "lod name");
        assert_eq!(geoset.faces, [0, 1, 2]);
        assert!(geoset.drawn_at(1));
        assert!(!geoset.drawn_at(0));
        assert_eq!(Geoset::lowest_lod(std::slice::from_ref(&geoset)), Some(1));

        let bytes = geoset_bytes(800, 0, "");
        let mut reader = Reader::new(&bytes, 0);
        let geoset = parse_geoset(&mut reader, 800).expect("v800 geoset");
        assert_eq!(geoset.lod, 0, "v800 files have no lod field");
        assert!(geoset.lod_name.is_empty());
        assert_eq!(geoset.faces, [0, 1, 2]);
    }

    /// The public entry point is classic-only: a v900+ file is rejected rather
    /// than half-parsed, which is why no geoset LOD can be the cause of a
    /// doubled body in this codebase.
    #[test]
    fn parse_rejects_non_classic_versions() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MDLX");
        bytes.extend_from_slice(b"VERS");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&901u32.to_le_bytes());
        match MdxModel::parse(&bytes) {
            Err(FormatError::Unsupported(_)) => {}
            other => panic!("v901 must be reported as unsupported, got {other:?}"),
        }
    }

    #[test]
    fn chunk_walk_and_node_pivot_count_match_real_model() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/Units/Human/Footman/Footman.mdx");
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("skip: Footman.mdx missing");
            return;
        };
        let model = MdxModel::parse(&bytes).expect("parse");
        let chunks = MdxModel::chunks(&bytes).expect("chunks");
        assert!(chunks.iter().any(|chunk| &chunk.tag == b"VERS"));
        assert!(chunks.iter().any(|chunk| &chunk.tag == b"GEOS"));
        // Every node object owns one PIVT entry; the audited corpus invariant
        // (542/542 models) starts with this fixture.
        let node_objects = MdxModel::node_object_count(&bytes).expect("node count");
        assert_eq!(node_objects, model.pivots.len());
    }

    fn linear_track(keys: &[(u32, f32)]) -> Track<f32> {
        Track {
            interpolation: Interpolation::Linear,
            global_sequence: None,
            keys: keys
                .iter()
                .map(|(time, value)| Key {
                    time: *time,
                    value: *value,
                    in_tan: *value,
                    out_tan: *value,
                })
                .collect(),
            static_value: None,
        }
    }

    /// WC3-I-009: keyframe times are `int32`; a pre-start key (`-100` =
    /// `0xFFFF_FF9C`) is filtered out of sequence sampling instead of being
    /// sorted past the end of the track, and the signed span never underflows.
    #[test]
    fn assets_models_mdx_signed_key_times() {
        // Hydralisk `BlizParticlePuff` emission track: -100, then 3900..10000.
        let track = linear_track(&[(0xFFFF_FF9C, 0.0), (3900, 1.0), (10000, 2.0)]);
        assert_eq!(track.sample(0), Some(1.0), "pre-start key is filtered");
        assert_eq!(track.sample(3900), Some(1.0));
        let mid = track.sample(6950).expect("mid-interval sample");
        assert!((mid - 1.5).abs() < 1e-6, "midpoint {mid}");
        assert_eq!(track.sample(10000), Some(2.0));
        assert_eq!(
            track.sample(999_999),
            Some(2.0),
            "past the last in-sequence key the track holds its value, not -100's"
        );

        // A track that only carries pre-start keys is empty for a sequence:
        // the caller applies the object's static value (engine: default value).
        let only_pre_start = linear_track(&[(0xFFFF_FF9C, 7.0)]);
        assert_eq!(only_pre_start.sample(0), None);

        // Genuine inversions are put in signed order by the parser; equal times
        // keep file order (the earlier key wins at the shared instant).
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        for (time, value) in [(500u32, 1.0f32), (100, 2.0)] {
            bytes.extend_from_slice(&time.to_le_bytes());
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        let mut reader = Reader::new(&bytes, 0);
        let parsed = parse_track(&mut reader, |reader| reader.f32()).expect("parse track");
        let times: Vec<u32> = parsed.keys.iter().map(|key| key.time).collect();
        assert_eq!(times, vec![100, 500], "signed sort");
        assert_eq!(parsed.sample(50), Some(2.0));

        // `DontInterp` steps at the key itself: the engine's scan selects the
        // first key with `frames[i] > frame`, so frame 100 reads the key at 100.
        let step = Track {
            interpolation: Interpolation::None,
            keys: vec![
                Key {
                    time: 0,
                    value: 1.0f32,
                    in_tan: 1.0,
                    out_tan: 1.0,
                },
                Key {
                    time: 100,
                    value: 0.0,
                    in_tan: 0.0,
                    out_tan: 0.0,
                },
            ],
            ..linear_track(&[])
        };
        assert_eq!(step.sample(0), Some(1.0));
        assert_eq!(step.sample(99), Some(1.0));
        assert_eq!(step.sample(100), Some(0.0), "step lands exactly on the key");
        assert_eq!(step.sample(101), Some(0.0));
    }
}
