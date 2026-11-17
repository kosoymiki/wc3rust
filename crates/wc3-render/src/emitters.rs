//! CPU simulation of the MDX geometry emitters, feeding [`crate::particles`].
//!
//! Three families live here, all driven by the chunks parsed in
//! `wc3_formats::mdx`:
//!
//! * [`ParticleEmitter2Runtime`] — `PRE2` particles: emission accumulator,
//!   latitude cone, gravity, per-particle lifetime, two-segment colour/scale,
//!   optional stretched tail, sprite-atlas cell and team colour.
//! * [`RibbonRuntime`] — `RIBB` chains: two points per emission (above/below),
//!   gravity, per-point colour/alpha sampled at spawn, atlas slot from `KRTX`.
//! * [`SplatRuntime`] — `SPL`/`UBR` quads spawned by event objects; the splat
//!   table itself lives in `Splats\SplatData.txt` and is supplied by the caller
//!   as a [`SplatSpec`].
//!
//! Everything is deterministic for a given seed: the same sequence of `update`
//! calls produces the same instances, which keeps replays and frame hashes
//! stable (`crate::parity`).
//!
//! # Coordinate spaces
//!
//! WC3 MDX is Z-up. Particles are simulated in the emitter node's **local**
//! space and transformed to world with the matrix captured at spawn; a
//! `ModelSpace` emitter refreshes that matrix every frame so the particles ride
//! the node, every other emitter keeps the spawn matrix so the particles stay
//! where they were born. That is exactly the donor's split
//! (`mdx-m3-viewer/src/viewer/handlers/mdx/particle2.ts:bind`: local location
//! vs `vec3.transformMat4(location, location, node.worldMatrix)`).

use glam::{Mat4, Vec3};

use bytemuck::Zeroable;

use wc3_anim::sample_in_sequence;
use wc3_formats::mdx::{
    HeadOrTail, MdxModel, ParticleEmitter2, RibbonEmitter, Sequence, Track,
};

use crate::fx::Rng;
use crate::geoset::active_in_sequence;
use crate::particles::{
    EmitterKind, EmitterParams, ParticleBlend, ParticleDraw, ParticleInstance,
};

/// WC3 multiplies the emission rate by 2 on the "High" particle setting
/// (`mdx-m3-viewer/src/viewer/handlers/mdx/geometryemitterfuncs.ts:72`,
/// `SETTING_PARTICLES_HIGH`). The original options are Low/Half, Medium/1,
/// High/2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleQuality {
    Low,
    Medium,
    High,
}

impl ParticleQuality {
    pub fn multiplier(self) -> f32 {
        match self {
            Self::Low => 0.5,
            Self::Medium => 1.0,
            Self::High => 2.0,
        }
    }
}

impl Default for ParticleQuality {
    fn default() -> Self {
        Self::High
    }
}

/// Texture slots an emitter needs, resolved by the caller once.
///
/// `model_texture[i]` maps `MdxModel::textures[i]` to the renderer's texture
/// store; `team_color` / `team_glow` are the replaceable textures 1 and 2.
#[derive(Clone, Debug, Default)]
pub struct EmitterTextures {
    pub model_texture: Vec<usize>,
    pub team_color: Option<usize>,
    pub team_glow: Option<usize>,
}

impl EmitterTextures {
    /// Resolve an emitter's texture: `replaceable_id` wins over `texture_id`
    /// (`mdx-m3-viewer/src/viewer/handlers/mdx/particleemitter2object.ts:150-170`).
    ///
    /// Returns `(store index, is_team_coloured)`.
    pub fn resolve(&self, texture_id: i32, replaceable_id: u32) -> (usize, bool) {
        match replaceable_id {
            1 => (self.team_color.unwrap_or(0), true),
            2 => (
                self.team_glow.or(self.team_color).unwrap_or(0),
                true,
            ),
            _ => {
                let index = if texture_id < 0 {
                    0
                } else {
                    *self.model_texture.get(texture_id as usize).unwrap_or(&0)
                };
                (index, false)
            }
        }
    }

    fn model(&self, texture_id: u32) -> usize {
        *self.model_texture.get(texture_id as usize).unwrap_or(&0)
    }
}

/// Per-frame context shared by every emitter of one model instance.
pub struct EmitterContext<'a> {
    /// Model-space node world matrices indexed by node object id.
    pub node_worlds: &'a [Mat4],
    /// Model pivots indexed by node object id.
    pub pivots: &'a [Vec3],
    /// Model-to-world transform (includes the Z-up to Y-up conversion).
    pub world: Mat4,
    /// Running sequence (selects the track variant), if any.
    pub sequence: Option<&'a Sequence>,
    /// Sequence-relative time in milliseconds.
    pub time_ms: u32,
    /// Team colour applied to team-coloured particles.
    pub team_color: [f32; 4],
}

impl EmitterContext<'_> {
    fn node_matrix(&self, object_id: u32) -> Mat4 {
        self.node_worlds
            .get(object_id as usize)
            .copied()
            .unwrap_or(Mat4::IDENTITY)
    }

    fn pivot(&self, object_id: u32) -> Vec3 {
        self.pivots
            .get(object_id as usize)
            .copied()
            .unwrap_or(Vec3::ZERO)
    }

    /// Node-local to world for one emitter.
    fn emitter_matrix(&self, object_id: u32) -> Mat4 {
        self.world * self.node_matrix(object_id)
    }
}

fn sample_scalar(track: &Option<Track<f32>>, ctx: &EmitterContext<'_>, fallback: f32) -> f32 {
    let Some(track) = track else {
        return fallback;
    };
    match ctx.sequence {
        Some(sequence) if active_in_sequence(track, sequence) => {
            sample_in_sequence(track, sequence, ctx.time_ms).unwrap_or(fallback)
        }
        Some(_) => fallback,
        None => track.sample(ctx.time_ms).unwrap_or(fallback),
    }
}

fn sample_vector(track: &Option<Track<Vec3>>, ctx: &EmitterContext<'_>, fallback: Vec3) -> Vec3 {
    let Some(track) = track else {
        return fallback;
    };
    match ctx.sequence {
        Some(sequence) if active_in_sequence(track, sequence) => {
            sample_in_sequence(track, sequence, ctx.time_ms).unwrap_or(fallback)
        }
        Some(_) => fallback,
        None => track.sample(ctx.time_ms).unwrap_or(fallback),
    }
}

fn sample_u32(track: &Option<Track<u32>>, ctx: &EmitterContext<'_>, fallback: u32) -> u32 {
    let Some(track) = track else {
        return fallback;
    };
    match ctx.sequence {
        Some(sequence) if active_in_sequence(track, sequence) => {
            sample_in_sequence(track, sequence, ctx.time_ms).unwrap_or(fallback)
        }
        Some(_) => fallback,
        None => track.sample(ctx.time_ms).unwrap_or(fallback),
    }
}

/// One live `PRE2` particle, in the local space of its emitter node.
#[derive(Clone, Copy, Debug)]
struct LiveParticle {
    pos: Vec3,
    velocity: Vec3,
    /// Remaining life in seconds (the shader ramps on `life_span - health`).
    health: f32,
    gravity: f32,
    /// Sprite cell for team-coloured particles (the shader reads it from the
    /// low `left` byte); the atlas cell is otherwise derived in the shader.
    cell: u8,
    /// Emitter-node-local to world, captured at spawn (`ModelSpace` refreshes).
    space: Mat4,
    /// Billboard rotation around the view axis, radians.
    rotation: f32,
}

/// Runtime state of one `PRE2` emitter.
pub struct ParticleEmitter2Runtime {
    /// Index into `MdxModel::particle_emitters2`.
    pub source: usize,
    pub node_id: u32,
    particles: Vec<LiveParticle>,
    emission: f32,
    capacity: usize,
    rate_scale: f32,
    texture: usize,
    params: EmitterParams,
    priority: i32,
    model_space: bool,
    squirt_done: bool,
    /// Previous `time_ms`; a rewind means a new sequence started.
    last_time_ms: u32,
    rng: Rng,
}

impl ParticleEmitter2Runtime {
    /// Build the runtime for one parsed emitter.
    pub fn new(
        source: usize,
        emitter: &ParticleEmitter2,
        textures: &EmitterTextures,
        quality: ParticleQuality,
    ) -> Self {
        let (texture, team_coloured) =
            textures.resolve(emitter.texture_id, emitter.replaceable_id);
        let rate_scale = quality.multiplier();
        let capacity = (((emitter.emission_rate.max(0.0) * rate_scale)
            * emitter.life_span.max(0.05)) as usize)
            .saturating_add(8)
            .min(4096);
        let mut params = EmitterParams::default();
        for (slot, color) in params.colors.iter_mut().enumerate() {
            let rgb = emitter.segment_colors[slot];
            let alpha = emitter.segment_alphas[slot] as f32 / 255.0;
            *color = [rgb.x, rgb.y, rgb.z, alpha];
        }
        params.scaling = [
            emitter.segment_scaling[0],
            emitter.segment_scaling[1],
            emitter.segment_scaling[2],
            0.0,
        ];
        params.intervals = [
            interval(emitter.head_intervals[0]),
            interval(emitter.head_intervals[1]),
            interval(emitter.tail_intervals[0]),
            interval(emitter.tail_intervals[1]),
        ];
        params.misc = [
            emitter.life_span.max(0.0001),
            emitter.time_middle.clamp(0.0001, 0.9999),
            emitter.columns.max(1) as f32,
            emitter.rows.max(1) as f32,
        ];
        params.misc2 = [
            emitter.filter_mode,
            u32::from(team_coloured),
            EmitterKind::Particle2 as u32,
            0,
        ];
        Self {
            source,
            node_id: emitter.node.object_id,
            particles: Vec::with_capacity(capacity.min(256)),
            emission: 0.0,
            capacity,
            rate_scale,
            texture,
            params,
            priority: emitter.priority_plane,
            model_space: emitter.flag(wc3_formats::mdx::PRE2_FLAG_MODEL_SPACE),
            squirt_done: false,
            last_time_ms: 0,
            rng: Rng::new(0x9E37_79B9_7F4A_7C15 ^ (source as u64 + 1).wrapping_mul(0x2545_F491)),
        }
    }

    /// Live particle count (diagnostics/tests).
    pub fn alive(&self) -> usize {
        self.particles.len()
    }

    /// Advance one emitter by `dt` seconds and append its instances.
    pub fn update(
        &mut self,
        emitter: &ParticleEmitter2,
        ctx: &EmitterContext<'_>,
        dt: f32,
        out: &mut Vec<ParticleInstance>,
    ) {
        if ctx.time_ms < self.last_time_ms {
            self.squirt_done = false;
            self.emission = 0.0;
            for particle in &mut self.particles {
                particle.health = 0.0;
            }
        }
        self.last_time_ms = ctx.time_ms;

        let pivot = ctx.pivot(self.node_id);
        let emitter_matrix = ctx.emitter_matrix(self.node_id);
        let visible = sample_scalar(&emitter.visibility_track, ctx, 1.0) > 0.0;

        // ---- simulate the live particles
        for particle in &mut self.particles {
            particle.health -= dt;
            if particle.health <= 0.0 {
                continue;
            }
            particle.velocity.z -= particle.gravity * dt;
            particle.pos += particle.velocity * dt;
            if self.model_space {
                particle.space = emitter_matrix;
            }
        }
        self.particles.retain(|particle| particle.health > 0.0);

        // ---- spawn
        if visible && dt > 0.0 {
            let rate = sample_scalar(&emitter.emission_rate_track, ctx, emitter.emission_rate)
                .max(0.0)
                * self.rate_scale;
            if emitter.squirt != 0 {
                if !self.squirt_done {
                    self.emission += rate.max(1.0);
                    self.squirt_done = true;
                }
            } else {
                self.emission += rate * dt;
            }
            // `PRE2` has no life-span track: the life span is static.
            let life = emitter.life_span.max(0.001);
            let speed = sample_scalar(&emitter.speed_track, ctx, emitter.speed);
            let variation = sample_scalar(&emitter.variation_track, ctx, emitter.variation);
            let latitude = sample_scalar(&emitter.latitude_track, ctx, emitter.latitude);
            let gravity = sample_scalar(&emitter.gravity_track, ctx, emitter.gravity);
            let width = sample_scalar(&emitter.width_track, ctx, emitter.width);
            let length = sample_scalar(&emitter.length_track, ctx, emitter.length);
            let line_emitter = emitter.flag(wc3_formats::mdx::PRE2_FLAG_LINE_EMITTER);
            let y_quad = emitter.flag(wc3_formats::mdx::PRE2_FLAG_XY_QUAD);
            let head_or_tail = emitter.head_or_tail();
            let half_w = width * 0.5;
            let half_l = length * 0.5;
            let latitude_rad = latitude.to_radians().abs();
            let space = emitter_matrix;
            while self.emission >= 1.0 && self.particles.len() < self.capacity {
                self.emission -= 1.0;
                let local = pivot
                    + Vec3::new(self.rng.range(-half_w, half_w), self.rng.range(-half_l, half_l), 0.0);
                // Donor cone: rotate +Z by Y(±latitude) then X(±latitude) unless
                // this is a line emitter (`particle2.ts:66-78`).
                let around = self.rng.range(-latitude_rad, latitude_rad);
                let elevation = if line_emitter {
                    0.0
                } else {
                    self.rng.range(-latitude_rad, latitude_rad)
                };
                let dir = Vec3::new(
                    around.sin() * elevation.cos(),
                    -elevation.sin(),
                    around.cos() * elevation.cos(),
                );
                let speed = speed * (1.0 + self.rng.range(-variation, variation));
                let velocity = dir * speed;
                let world_velocity = space.transform_vector3(velocity);
                let rotation = if y_quad {
                    world_velocity.z.atan2(world_velocity.x) - std::f32::consts::PI
                        + std::f32::consts::PI / 8.0
                } else {
                    self.rng.range(0.0, std::f32::consts::TAU)
                };
                self.particles.push(LiveParticle {
                    pos: local,
                    velocity,
                    health: life,
                    gravity,
                    cell: (self.rng.next_u64() & 0xFF) as u8,
                    space,
                    rotation,
                });
            }
            if matches!(head_or_tail, HeadOrTail::Head | HeadOrTail::Both) && self.particles.len() > self.capacity {
                self.particles.truncate(self.capacity);
            }
        }

        // ---- instances
        let tail_length = emitter.tail_length.max(0.0);
        let want_head = matches!(
            emitter.head_or_tail(),
            HeadOrTail::Head | HeadOrTail::Both
        );
        let want_tail = matches!(
            emitter.head_or_tail(),
            HeadOrTail::Tail | HeadOrTail::Both
        ) && tail_length > 0.0;
        for particle in &self.particles {
            let world = particle.space.transform_point3(particle.pos);
            if want_head {
                let mut instance = ParticleInstance::zeroed();
                instance.p0 = [world.x, world.y, world.z, 0.0];
                instance.p1 = [particle.rotation, 0.0, 0.0, 0.0];
                instance.p2 = [1.0, 0.0, 0.0, 0.0];
                instance.health = particle.health;
                instance.color = [255; 4];
                instance.flags = (particle.cell as u32) << 8;
                out.push(instance);
            }
            if want_tail {
                let head = world;
                let start = head - particle.space.transform_vector3(particle.velocity) * tail_length;
                let mut instance = ParticleInstance::zeroed();
                instance.p0 = [start.x, start.y, start.z, 0.0];
                instance.p1 = [head.x, head.y, head.z, 0.0];
                instance.p2 = [1.0, 0.0, 0.0, 0.0];
                instance.health = particle.health;
                instance.color = [255; 4];
                instance.flags = 1;
                out.push(instance);
            }
        }
    }

    /// Emit the draw for this emitter.
    pub fn draw(&self, start: u32, count: u32, params: u32) -> ParticleDraw {
        ParticleDraw {
            kind: EmitterKind::Particle2,
            blend: ParticleBlend::from_filter(self.params.misc2[0]),
            start,
            count,
            texture: self.texture,
            params,
            priority: self.priority,
        }
    }

    /// Parameter block with the instance's team colour filled in.
    pub fn params(&self, team_color: [f32; 4]) -> EmitterParams {
        let mut params = self.params;
        params.team_color = team_color;
        params
    }
}

fn interval(values: [u32; 3]) -> [u32; 4] {
    [values[0], values[1], values[2], 0]
}

/// One ribbon sample point (above/below pair in emitter-local space).
#[derive(Clone, Copy, Debug)]
struct RibbonPoint {
    above: Vec3,
    below: Vec3,
    color: [u8; 4],
    slot: u32,
    health: f32,
    space: Mat4,
}

/// Runtime state of one `RIBB` ribbon emitter.
pub struct RibbonRuntime {
    pub source: usize,
    pub node_id: u32,
    points: Vec<RibbonPoint>,
    emission: f32,
    capacity: usize,
    texture_slot: u32,
    texture: usize,
    blend: ParticleBlend,
    params: EmitterParams,
    last_time_ms: u32,
}

impl RibbonRuntime {
    pub fn new(
        source: usize,
        emitter: &RibbonEmitter,
        model: &MdxModel,
        textures: &EmitterTextures,
    ) -> Self {
        // A ribbon's texture and blend come from the first layer of its
        // material (`ribbonemitterobject.ts:37`).
        let layer = model
            .materials
            .get(emitter.material_id.max(0) as usize)
            .and_then(|material| material.layers.first());
        let texture = layer.map(|layer| textures.model(layer.texture_id)).unwrap_or(0);
        let blend = ParticleBlend::from_layer_blend(layer.map(|layer| layer.blend_mode).unwrap_or(2));
        let life = emitter.life_span.max(0.05);
        let capacity = ((emitter.emission_rate as f32 * life) as usize + 2).min(512);
        let mut params = EmitterParams::default();
        params.colors = [[emitter.color.x, emitter.color.y, emitter.color.z, 1.0]; 3];
        params.misc = [
            life,
            0.5,
            emitter.columns.max(1) as f32,
            emitter.rows.max(1) as f32,
        ];
        // Ribbon alpha-test uses filter mode 1 (`particles.frag.ts:18`), the
        // colour comes from the per-point instance, not from the segments.
        params.misc2 = [1, 1, EmitterKind::Ribbon as u32, 0];
        params.scaling = [1.0, 1.0, 1.0, 0.0];
        Self {
            source,
            node_id: emitter.node.object_id,
            points: Vec::with_capacity(capacity.min(256)),
            emission: 0.0,
            capacity,
            texture_slot: emitter.texture_slot,
            texture,
            blend,
            params,
            last_time_ms: 0,
        }
    }

    pub fn alive(&self) -> usize {
        self.points.len()
    }

    pub fn update(
        &mut self,
        emitter: &RibbonEmitter,
        ctx: &EmitterContext<'_>,
        dt: f32,
        out: &mut Vec<ParticleInstance>,
    ) {
        if ctx.time_ms < self.last_time_ms {
            for point in &mut self.points {
                point.health = 0.0;
            }
            self.emission = 0.0;
        }
        self.last_time_ms = ctx.time_ms;

        let pivot = ctx.pivot(self.node_id);
        let space = ctx.emitter_matrix(self.node_id);
        let above = sample_scalar(&emitter.height_above_track, ctx, emitter.height_above);
        let below = sample_scalar(&emitter.height_below_track, ctx, emitter.height_below);
        let alpha = sample_scalar(&emitter.alpha_track, ctx, emitter.alpha).clamp(0.0, 1.0);
        let visible = sample_scalar(&emitter.visibility_track, ctx, 1.0) > 0.0 && alpha > 0.001;
        let color = sample_vector(&emitter.color_track, ctx, emitter.color);
        let slot = sample_u32(&emitter.texture_slot_track, ctx, self.texture_slot);
        let gravity = emitter.gravity;

        for point in &mut self.points {
            point.health -= dt;
            if point.health > 0.0 {
                point.above.z -= gravity * dt;
                point.below.z -= gravity * dt;
            }
        }
        self.points.retain(|point| point.health > 0.0);

        if visible && dt > 0.0 {
            self.emission += emitter.emission_rate as f32 * dt;
            let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0) as u8;
            while self.emission >= 1.0 && self.points.len() < self.capacity {
                self.emission -= 1.0;
                self.points.push(RibbonPoint {
                    above: Vec3::new(pivot.x, pivot.y, pivot.z + above),
                    below: Vec3::new(pivot.x, pivot.y, pivot.z - below),
                    color: [
                        byte(color.x),
                        byte(color.y),
                        byte(color.z),
                        byte(alpha),
                    ],
                    slot,
                    health: emitter.life_span.max(0.02),
                    space,
                });
            }
        }

        let columns = emitter.columns.max(1) as f32;
        let rows = emitter.rows.max(1) as f32;
        let alive = self.points.len();
        if alive < 2 {
            return;
        }
        let chain = 1.0 / (alive as f32 - 1.0);
        for index in 0..alive - 1 {
            let point = self.points[index];
            let next = self.points[index + 1];
            let above_now = point.space.transform_point3(point.above);
            let below_now = point.space.transform_point3(point.below);
            let above_next = next.space.transform_point3(next.above);
            let below_next = next.space.transform_point3(next.below);
            // Each point owns one texture column, shifted by its place in the
            // chain so the strip slides through the atlas
            // (`geometryemitterfuncs.ts:248-252`; the donor adds
            // `chainLengthFactor` to `left`, we add `1/columns` so the quad
            // spans exactly one cell).
            let cell = (point.slot % emitter.columns.max(1)) as f32;
            let left = (cell + (1.0 - index as f32 * chain - chain)) / columns;
            let right = left + 1.0 / columns;
            let top = (point.slot as f32 / columns) / rows;
            let mut instance = ParticleInstance::zeroed();
            // Donor corner order (`geometryemitterfuncs.ts:bindRibbonEmitterBuffer`):
            // p0 = above(self), p1 = below(self), p2 = below(next), p3 = above(next).
            instance.p0 = [above_now.x, above_now.y, above_now.z, 0.0];
            instance.p1 = [below_now.x, below_now.y, below_now.z, 0.0];
            instance.p2 = [below_next.x, below_next.y, below_next.z, 0.0];
            instance.p3 = [above_next.x, above_next.y, above_next.z, 0.0];
            instance.color = point.color;
            instance.health = point.health;
            instance.flags = ParticleInstance::pack_ribbon(
                left.clamp(0.0, 1.0),
                right.clamp(0.0, 1.0),
                top.clamp(0.0, 1.0),
            );
            out.push(instance);
        }
    }

    pub fn draw(&self, start: u32, count: u32, params: u32) -> ParticleDraw {
        ParticleDraw {
            kind: EmitterKind::Ribbon,
            blend: self.blend,
            start,
            count,
            texture: self.texture,
            params,
            priority: 0,
        }
    }

    pub fn params(&self) -> EmitterParams {
        self.params
    }
}

/// Splat table row (`Splats\SplatData.txt`), supplied by the caller.
///
/// Field mapping (`eventobjectemitterobject.ts:105-130`):
/// * splat: `birth` = `Lifespan`, `decay` = `Decay`, `pause` unused;
/// * ubersplat: `birth` = `BirthTime`, `pause` = `PauseTime`, `decay` = `Decay`.
#[derive(Clone, Debug)]
pub struct SplatSpec {
    pub life_span: f32,
    pub birth: f32,
    pub pause: f32,
    pub decay: f32,
    pub colors: [[f32; 4]; 3],
    pub scale: f32,
    pub columns: u32,
    pub rows: u32,
    pub texture: usize,
    pub blend: ParticleBlend,
    /// `[UVLifespanStart, UVLifespanEnd, LifespanRepeat]`,
    /// `[UVDecayStart, UVDecayEnd, DecayRepeat]`.
    pub intervals: [[u32; 3]; 2],
    pub ubersplat: bool,
}

impl Default for SplatSpec {
    fn default() -> Self {
        Self {
            life_span: 1.0,
            birth: 0.2,
            pause: 0.3,
            decay: 0.5,
            colors: [[1.0, 1.0, 1.0, 1.0]; 3],
            scale: 64.0,
            columns: 1,
            rows: 1,
            texture: 0,
            blend: ParticleBlend::Blend,
            intervals: [[0, 0, 1]; 2],
            ubersplat: false,
        }
    }
}

/// One live splat / ubersplat quad.
#[derive(Clone, Copy, Debug)]
pub struct LiveSplat {
    pub spec: usize,
    pub position: Vec3,
    pub health: f32,
}

/// The whole emitter set of one model instance.
pub struct EmitterSystem {
    particle2: Vec<ParticleEmitter2Runtime>,
    ribbons: Vec<RibbonRuntime>,
    splats: Vec<LiveSplat>,
    splat_specs: Vec<SplatSpec>,
    instances: Vec<ParticleInstance>,
    params: Vec<EmitterParams>,
    draws: Vec<ParticleDraw>,
}

impl EmitterSystem {
    /// Build the runtime set for a model.
    pub fn new(model: &MdxModel, textures: &EmitterTextures, seed: u64) -> Self {
        Self::with_quality(model, textures, seed, ParticleQuality::default())
    }

    pub fn with_quality(
        model: &MdxModel,
        textures: &EmitterTextures,
        seed: u64,
        quality: ParticleQuality,
    ) -> Self {
        let mut particle2 = Vec::with_capacity(model.particle_emitters2.len());
        for (index, emitter) in model.particle_emitters2.iter().enumerate() {
            let mut runtime = ParticleEmitter2Runtime::new(index, emitter, textures, quality);
            // Fold the instance seed in so two instances of the same model do
            // not emit identical particles.
            runtime.rng = Rng::new(seed ^ (index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
            particle2.push(runtime);
        }
        let ribbons = model
            .ribbon_emitters
            .iter()
            .enumerate()
            .map(|(index, emitter)| RibbonRuntime::new(index, emitter, model, textures))
            .collect();
        Self {
            particle2,
            ribbons,
            splats: Vec::new(),
            splat_specs: Vec::new(),
            instances: Vec::new(),
            params: Vec::new(),
            draws: Vec::new(),
        }
    }

    /// Emitters modelled by this system.
    pub fn emitter_count(&self) -> usize {
        self.particle2.len() + self.ribbons.len()
    }

    /// Instances built by the last [`Self::update`].
    pub fn instances(&self) -> &[ParticleInstance] {
        &self.instances
    }

    /// Parameter blocks, one per draw.
    pub fn params(&self) -> &[EmitterParams] {
        &self.params
    }

    /// Recorded draws referencing [`Self::instances`].
    pub fn draws(&self) -> &[ParticleDraw] {
        &self.draws
    }

    /// Register the splat table (once per asset database).
    pub fn set_splat_specs(&mut self, specs: Vec<SplatSpec>) {
        self.splat_specs = specs;
    }

    /// Spawn a splat/ubersplat from an event object. `true` when the spec exists.
    pub fn spawn_splat(&mut self, spec: usize, position: Vec3) -> bool {
        let Some(row) = self.splat_specs.get(spec) else {
            return false;
        };
        self.splats.push(LiveSplat {
            spec,
            position,
            health: row.life_span.max(0.01),
        });
        true
    }

    /// Number of live splat quads.
    pub fn splat_alive(&self) -> usize {
        self.splats.len()
    }

    /// Advance every emitter and rebuild the instance/param/draw lists.
    pub fn update(&mut self, model: &MdxModel, ctx: &EmitterContext<'_>, dt: f32) {
        self.instances.clear();
        self.params.clear();
        self.draws.clear();

        for runtime in &mut self.particle2 {
            let Some(emitter) = model.particle_emitters2.get(runtime.source) else {
                continue;
            };
            let start = self.instances.len() as u32;
            runtime.update(emitter, ctx, dt, &mut self.instances);
            let count = self.instances.len() as u32 - start;
            if count == 0 {
                continue;
            }
            let params = self.params.len() as u32;
            self.params.push(runtime.params(ctx.team_color));
            self.draws.push(runtime.draw(start, count, params));
        }

        for runtime in &mut self.ribbons {
            let Some(emitter) = model.ribbon_emitters.get(runtime.source) else {
                continue;
            };
            let start = self.instances.len() as u32;
            runtime.update(emitter, ctx, dt, &mut self.instances);
            let count = self.instances.len() as u32 - start;
            if count == 0 {
                continue;
            }
            let params = self.params.len() as u32;
            self.params.push(runtime.params());
            self.draws.push(runtime.draw(start, count, params));
        }

        // Splats: ground-aligned quads at the event position.
        for splat in &mut self.splats {
            splat.health -= dt;
            if splat.health <= 0.0 {
                continue;
            }
            let Some(spec) = self.splat_specs.get(splat.spec) else {
                continue;
            };
            let start = self.instances.len() as u32;
            let half = spec.scale * 0.5;
            let p = splat.position;
            let corners = [
                Vec3::new(p.x - half, p.y, p.z - half),
                Vec3::new(p.x + half, p.y, p.z - half),
                Vec3::new(p.x + half, p.y, p.z + half),
                Vec3::new(p.x - half, p.y, p.z + half),
            ];
            let mut instance = ParticleInstance::zeroed();
            instance.p0 = [corners[0].x, corners[0].y, corners[0].z, 0.0];
            instance.p1 = [corners[1].x, corners[1].y, corners[1].z, 0.0];
            instance.p2 = [corners[2].x, corners[2].y, corners[2].z, 0.0];
            instance.p3 = [corners[3].x, corners[3].y, corners[3].z, 0.0];
            instance.color = [
                (spec.colors[1][0].clamp(0.0, 1.0) * 255.0) as u8,
                (spec.colors[1][1].clamp(0.0, 1.0) * 255.0) as u8,
                (spec.colors[1][2].clamp(0.0, 1.0) * 255.0) as u8,
                (spec.colors[1][3].clamp(0.0, 1.0) * 255.0) as u8,
            ];
            instance.health = splat.health;
            self.instances.push(instance);

            let mut params = EmitterParams::default();
            params.colors = spec.colors;
            params.misc = [
                spec.life_span.max(0.01),
                spec.birth.max(0.0),
                spec.columns.max(1) as f32,
                spec.rows.max(1) as f32,
            ];
            // `scaling` carries the interval times for splats/ubersplats.
            params.scaling = if spec.ubersplat {
                [spec.birth.max(0.0001), spec.pause.max(0.0), spec.decay.max(0.0001), 0.0]
            } else {
                [spec.birth.max(0.0001), spec.decay.max(0.0001), 0.0, 0.0]
            };
            params.intervals = [
                interval(spec.intervals[0]),
                interval(spec.intervals[1]),
                [0; 4],
                [0; 4],
            ];
            params.misc2 = [
                match spec.blend {
                    ParticleBlend::Blend => 0,
                    ParticleBlend::Additive => 1,
                    ParticleBlend::Modulate => 2,
                    ParticleBlend::Modulate2x => 3,
                },
                0,
                if spec.ubersplat {
                    EmitterKind::UberSplat as u32
                } else {
                    EmitterKind::Splat as u32
                },
                0,
            ];
            let params_index = self.params.len() as u32;
            self.params.push(params);
            self.draws.push(ParticleDraw {
                kind: if spec.ubersplat {
                    EmitterKind::UberSplat
                } else {
                    EmitterKind::Splat
                },
                blend: spec.blend,
                start,
                count: 1,
                texture: spec.texture,
                params: params_index,
                priority: 0,
            });
        }
        self.splats.retain(|splat| splat.health > 0.0);
    }

    /// Live particles of one emitter (tests).
    pub fn particle_alive(&self, index: usize) -> Option<usize> {
        self.particle2.get(index).map(|runtime| runtime.alive())
    }

    /// Live ribbon points of one emitter (tests).
    pub fn ribbon_alive(&self, index: usize) -> Option<usize> {
        self.ribbons.get(index).map(|runtime| runtime.alive())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use wc3_formats::mdx::{
        Interpolation, Material, Node, ParticleEmitter, ParticleEmitter2, RibbonEmitter, Track,
    };

    fn empty_vec3() -> Track<Vec3> {
        Track {
            interpolation: Interpolation::None,
            global_sequence: None,
            keys: Vec::new(),
            static_value: None,
        }
    }

    fn node(name: &str, object_id: u32, flags: u32) -> Node {
        Node {
            name: name.to_string(),
            object_id,
            parent_id: -1,
            flags,
            translation: empty_vec3(),
            rotation: Track {
                interpolation: Interpolation::None,
                global_sequence: None,
                keys: Vec::new(),
                static_value: None,
            },
            scaling: empty_vec3(),
        }
    }

    fn emitter2(flags: u32, replaceable: u32) -> ParticleEmitter2 {
        ParticleEmitter2 {
            node: node("BlizParticle", 0, flags),
            speed: 10.0,
            variation: 0.0,
            latitude: 0.0,
            gravity: 0.0,
            life_span: 1.0,
            emission_rate: 10.0,
            width: 4.0,
            length: 4.0,
            filter_mode: 1,
            rows: 2,
            columns: 2,
            head_or_tail: 0,
            tail_length: 0.0,
            time_middle: 0.5,
            segment_colors: [
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ],
            segment_alphas: [255, 128, 0],
            segment_scaling: [1.0, 0.5, 0.0],
            head_intervals: [[0, 3, 1], [0, 3, 1]],
            tail_intervals: [[0, 0, 0], [0, 0, 0]],
            texture_id: 0,
            squirt: 0,
            priority_plane: 1,
            replaceable_id: replaceable,
            speed_track: None,
            variation_track: None,
            latitude_track: None,
            gravity_track: None,
            visibility_track: None,
            emission_rate_track: None,
            width_track: None,
            length_track: None,
        }
    }

    fn model(emitter: ParticleEmitter2) -> MdxModel {
        let mut model = MdxModel::default();
        model.pivots.push(Vec3::new(10.0, 0.0, 5.0));
        model.particle_emitters2.push(emitter);
        model
    }

    fn ctx<'a>(worlds: &'a [Mat4], pivots: &'a [Vec3]) -> EmitterContext<'a> {
        EmitterContext {
            node_worlds: worlds,
            pivots,
            world: Mat4::IDENTITY,
            sequence: None,
            time_ms: 0,
            team_color: [0.2, 0.4, 1.0, 1.0],
        }
    }

    #[test]
    fn emitter2_emits_at_the_configured_rate_and_caps_at_capacity() {
        let model = model(emitter2(0, 0));
        let textures = EmitterTextures {
            model_texture: vec![0],
            ..Default::default()
        };
        // Medium quality keeps the configured rate (High doubles it).
        let mut system = EmitterSystem::with_quality(
            &model,
            &textures,
            7,
            ParticleQuality::Medium,
        );
        let worlds = [Mat4::IDENTITY];
        let mut context = ctx(&worlds, &model.pivots);
        // 10 particles/s for 0.35 s -> 3 or 4 particles (accumulator).
        context.time_ms = 0;
        system.update(&model, &context, 0.35);
        let alive = system.particle_alive(0).expect("one emitter");
        assert!(alive >= 3 && alive <= 4, "alive {alive}");
        // One instance per particle (head only, no tail).
        assert_eq!(system.instances().len(), alive);
        assert_eq!(system.draws().len(), 1);
        assert_eq!(system.draws()[0].priority, 1);
        assert_eq!(system.draws()[0].blend, ParticleBlend::Additive);
        assert_eq!(system.draws()[0].kind, EmitterKind::Particle2);
        // Capacity is `rate * life + 8`.
        for step in 0..100 {
            context.time_ms = (step as f32 * 100.0) as u32;
            system.update(&model, &context, 0.1);
        }
        let alive = system.particle_alive(0).expect("one emitter");
        assert!(alive <= 18, "capacity must cap the emitter, got {alive}");
    }

    #[test]
    fn emitter2_gravity_pulls_particles_down_in_local_z() {
        let mut emitter = emitter2(0, 0);
        emitter.gravity = 100.0;
        emitter.emission_rate = 4.0;
        emitter.speed = 0.0;
        let model = model(emitter);
        let textures = EmitterTextures {
            model_texture: vec![0],
            ..Default::default()
        };
        let mut system = EmitterSystem::new(&model, &textures, 3);
        let worlds = [Mat4::IDENTITY];
        let mut context = ctx(&worlds, &model.pivots);
        // Spawn one particle, then let it fall for a while.
        context.time_ms = 0;
        system.update(&model, &context, 0.3);
        let first = system.instances().first().expect("particle").p0[2];
        for step in 1..6 {
            context.time_ms = step * 100;
            system.update(&model, &context, 0.1);
        }
        let last = system
            .instances()
            .iter()
            .map(|instance| instance.p0[2])
            .fold(f32::INFINITY, f32::min);
        assert!(last < first - 0.5, "particle must fall: {first} -> {last}");
    }

    #[test]
    fn team_coloured_emitter_carries_the_instance_tint() {
        let model = model(emitter2(0, 1));
        let textures = EmitterTextures {
            model_texture: vec![0],
            team_color: Some(2),
            ..Default::default()
        };
        let mut system = EmitterSystem::new(&model, &textures, 5);
        let worlds = [Mat4::IDENTITY];
        let context = ctx(&worlds, &model.pivots);
        system.update(&model, &context, 0.5);
        let params = system.params();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].misc2[1], 1, "team coloured");
        assert_eq!(params[0].team_color, [0.2, 0.4, 1.0, 1.0]);
        assert_eq!(system.draws()[0].texture, 2, "team colour texture slot");
    }

    #[test]
    fn visibility_track_zero_stops_emission() {
        let mut emitter = emitter2(0, 0);
        emitter.visibility_track = Some(Track {
            interpolation: Interpolation::Linear,
            global_sequence: None,
            keys: Vec::new(),
            static_value: Some(0.0),
        });
        let model = model(emitter);
        let textures = EmitterTextures {
            model_texture: vec![0],
            ..Default::default()
        };
        let mut system = EmitterSystem::new(&model, &textures, 9);
        let worlds = [Mat4::IDENTITY];
        let context = ctx(&worlds, &model.pivots);
        system.update(&model, &context, 1.0);
        assert_eq!(system.particle_alive(0), Some(0));
        assert!(system.instances().is_empty());
    }

    #[test]
    fn tail_emitter_emits_two_instances_per_particle() {
        let mut emitter = emitter2(0, 0);
        emitter.head_or_tail = 2; // Both
        emitter.tail_length = 0.5;
        let model = model(emitter);
        let textures = EmitterTextures {
            model_texture: vec![0],
            ..Default::default()
        };
        let mut system = EmitterSystem::new(&model, &textures, 11);
        let worlds = [Mat4::IDENTITY];
        let mut context = ctx(&worlds, &model.pivots);
        context.time_ms = 0;
        system.update(&model, &context, 0.3);
        let alive = system.particle_alive(0).expect("emitter");
        assert!(alive > 0);
        assert_eq!(system.instances().len(), alive * 2, "head + tail");
        let tails = system
            .instances()
            .iter()
            .filter(|instance| instance.is_tail())
            .count();
        assert_eq!(tails, alive);
    }

    #[test]
    fn simulation_is_deterministic_for_a_seed() {
        let model = model(emitter2(0, 0));
        let textures = EmitterTextures {
            model_texture: vec![0],
            ..Default::default()
        };
        let run = |seed: u64| {
            let mut system = EmitterSystem::new(&model, &textures, seed);
            let worlds = [Mat4::IDENTITY];
            let mut context = ctx(&worlds, &model.pivots);
            for step in 0..8 {
                context.time_ms = step * 60;
                system.update(&model, &context, 0.06);
            }
            system.instances().to_vec()
        };
        assert_eq!(run(42), run(42));
        assert_ne!(run(42), run(43), "different seeds must differ");
    }

    fn ribbon_model() -> MdxModel {
        let mut model = MdxModel::default();
        model.pivots.push(Vec3::new(0.0, 0.0, 40.0));
        model.materials.push(Material {
            priority: 0,
            flags: 0,
            layers: vec![wc3_formats::mdx::Layer {
                blend_mode: 2,
                shading_flags: 0,
                texture_id: 0,
                texture_animation_id: 0,
                coord_id: 0,
                alpha: 1.0,
                alpha_track: None,
                texture_id_track: None,
            }],
        });
        model.textures.push(wc3_formats::mdx::Texture {
            replaceable_id: 0,
            path: "Textures\\Ribbon.blp".to_string(),
            flags: 0,
        });
        model.ribbon_emitters.push(RibbonEmitter {
            node: node("BlizRibbon", 0, 0),
            height_above: 10.0,
            height_below: 10.0,
            alpha: 0.8,
            color: Vec3::new(0.3, 0.7, 1.0),
            life_span: 0.5,
            texture_slot: 0,
            emission_rate: 20,
            rows: 1,
            columns: 1,
            material_id: 0,
            gravity: 0.0,
            height_above_track: None,
            height_below_track: None,
            alpha_track: None,
            color_track: None,
            texture_slot_track: None,
            visibility_track: None,
        });
        model
    }

    #[test]
    fn ribbon_chain_builds_quads_between_consecutive_points() {
        let model = ribbon_model();
        let textures = EmitterTextures {
            model_texture: vec![0],
            ..Default::default()
        };
        let mut system = EmitterSystem::new(&model, &textures, 17);
        let worlds = [Mat4::IDENTITY];
        let mut context = ctx(&worlds, &model.pivots);
        // 20 points/s for 0.4 s -> 8 points -> 7 quads.
        for step in 0..4 {
            context.time_ms = step * 100;
            system.update(&model, &context, 0.1);
        }
        let points = system.ribbon_alive(0).expect("ribbon");
        assert!(points >= 6, "points {points}");
        assert_eq!(system.instances().len(), points - 1, "one quad per gap");
        assert_eq!(system.draws().len(), 1);
        let draw = system.draws()[0];
        assert_eq!(draw.kind, EmitterKind::Ribbon);
        assert_eq!(draw.blend, ParticleBlend::Blend, "layer blend mode 2");
        // Each quad's `p0`/`p1` are the above/below pair of one point.
        let instance = system.instances()[0];
        let above = Vec3::new(instance.p0[0], instance.p0[1], instance.p0[2]);
        let below = Vec3::new(instance.p1[0], instance.p1[1], instance.p1[2]);
        assert!((above - below).length() > 19.9, "height above + below = 20: {above:?} {below:?}");
    }

    #[test]
    fn ribbon_points_expire_after_their_life_span() {
        let mut model = ribbon_model();
        if let Some(emitter) = model.ribbon_emitters.first_mut() {
            // Hidden emitter: nothing new is emitted after the first frame.
            emitter.visibility_track = Some(Track {
                interpolation: Interpolation::Linear,
                global_sequence: None,
                keys: Vec::new(),
                static_value: Some(0.0),
            });
        }
        let textures = EmitterTextures {
            model_texture: vec![0],
            ..Default::default()
        };
        let mut system = EmitterSystem::new(&model, &textures, 19);
        let worlds = [Mat4::IDENTITY];
        let mut context = ctx(&worlds, &model.pivots);
        // Seed points with the track visible, then hide it and run past the life span.
        if let Some(emitter) = model.ribbon_emitters.first_mut() {
            emitter.visibility_track = None;
        }
        system.update(&model, &context, 0.2);
        let alive = system.ribbon_alive(0).expect("ribbon");
        assert!(alive > 0);
        if let Some(emitter) = model.ribbon_emitters.first_mut() {
            emitter.visibility_track = Some(Track {
                interpolation: Interpolation::Linear,
                global_sequence: None,
                keys: Vec::new(),
                static_value: Some(0.0),
            });
        }
        for step in 0..10 {
            context.time_ms += 100 + step;
            system.update(&model, &context, 0.1);
        }
        assert_eq!(system.ribbon_alive(0), Some(0), "points outlive their life span");
    }

    #[test]
    fn splats_spawn_expire_and_draw_their_own_quad() {
        let model = ribbon_model();
        let textures = EmitterTextures {
            model_texture: vec![0],
            ..Default::default()
        };
        let mut system = EmitterSystem::new(&model, &textures, 23);
        system.set_splat_specs(vec![SplatSpec {
            life_span: 0.3,
            scale: 40.0,
            ..Default::default()
        }]);
        let worlds = [Mat4::IDENTITY];
        let mut context = ctx(&worlds, &model.pivots);
        assert!(system.spawn_splat(0, Vec3::new(5.0, 0.0, 5.0)));
        assert!(!system.spawn_splat(7, Vec3::ZERO), "unknown spec is rejected");
        system.update(&model, &context, 0.0);
        assert_eq!(system.splat_alive(), 1);
        let splat_draw = system
            .draws()
            .iter()
            .find(|draw| draw.kind == EmitterKind::Splat)
            .copied()
            .expect("splat draw");
        assert_eq!(splat_draw.count, 1);
        let quad = system.instances()[splat_draw.start as usize];
        // A 40-unit quad centred on (5,*,5) spans 25..45 on both axes.
        assert!((quad.p0[0] - -15.0).abs() < 1e-3, "{:?}", quad.p0);
        assert!((quad.p2[0] - 25.0).abs() < 1e-3, "{:?}", quad.p2);
        system.update(&model, &context, 0.4);
        assert_eq!(system.splat_alive(), 0, "splat expires with its life span");
    }

    #[test]
    fn classic_particle_emitters_are_parsed_but_not_simulated() {
        // `PREM` emitters are recorded in the model (52 in the corpus) but the
        // instanced pass only implements `PRE2`/`RIBB`; the runtime must ignore
        // them instead of mis-instancing them.
        let mut model = MdxModel::default();
        model.particle_emitters.push(ParticleEmitter {
            node: node("Classic", 0, 0),
            emission_rate: 10.0,
            gravity: 0.0,
            longitude: 0.0,
            latitude: 0.0,
            path: "Textures\\Classic.blp".to_string(),
            life_span: 1.0,
            speed: 1.0,
            emission_rate_track: None,
            gravity_track: None,
            longitude_track: None,
            latitude_track: None,
            life_span_track: None,
            speed_track: None,
            visibility_track: None,
        });
        let textures = EmitterTextures::default();
        let mut system = EmitterSystem::new(&model, &textures, 1);
        let worlds = [Mat4::IDENTITY];
        let context = ctx(&worlds, &model.pivots);
        system.update(&model, &context, 1.0);
        assert_eq!(system.emitter_count(), 0);
        assert!(system.instances().is_empty());
    }

    #[test]
    fn real_frost_nova_emitters_produce_instances() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut path = None;
        for tree in ["analysis/war3_samples", "analysis/war3_full"] {
            let candidate = root.join(tree).join("Abilities/Spells/Undead/FrostNova/FrostNovaTarget.mdx");
            if candidate.exists() {
                path = Some(candidate);
                break;
            }
        }
        let Some(path) = path else {
            eprintln!("skipping: FrostNovaTarget.mdx fixture missing");
            return;
        };
        let bytes = std::fs::read(path).expect("read fixture");
        let model = MdxModel::parse(&bytes).expect("parse");
        let textures = EmitterTextures {
            model_texture: (0..model.textures.len()).collect(),
            ..Default::default()
        };
        let mut system = EmitterSystem::new(&model, &textures, 0xC0FF_EE00);
        let mut animator = wc3_anim::Animator::from_model(&model);
        if let Some(sequence) = model.sequences.first() {
            animator.play(sequence);
        }
        animator.update(0.0);
        let worlds = animator.world_matrices();
        let mut total = 0usize;
        for frame in 0..10 {
            let Some(sequence) = model.sequences.first() else {
                break;
            };
            let context = EmitterContext {
                node_worlds: &worlds,
                pivots: &model.pivots,
                world: Mat4::IDENTITY,
                sequence: Some(sequence),
                time_ms: sequence.start + frame * 50,
                team_color: [1.0; 4],
            };
            system.update(&model, &context, 0.05);
            total = total.max(system.instances().len());
        }
        assert!(total > 0, "FrostNovaTarget must emit particles/ribbons");
        assert!(
            system.draws().iter().any(|draw| draw.kind == EmitterKind::Particle2),
            "PRE2 draws: {:?}",
            system.draws()
        );
        assert!(
            system.draws().iter().any(|draw| draw.kind == EmitterKind::Ribbon),
            "RIBB draws: {:?}",
            system.draws()
        );
    }
}
