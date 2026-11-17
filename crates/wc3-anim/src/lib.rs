//! MDX animation sampling: node tracks, geoset animation and simple particles.
//!
//! Semantics implemented here follow the classic MDX rules that the WC3 engine
//! and the reference viewers (mdx-m3-viewer, War3Net, HiveWE) agree on:
//!
//! * a node's local matrix is `T(pivot) * T(translation) * R(rotation) * S(scaling) * T(-pivot)`
//!   (`donors/mdx-m3-viewer/src/viewer/skeletalnode.ts:182` uses
//!   `mat4.fromRotationTranslationScaleOrigin`),
//! * the pivot of a node is `pivots[node.object_id]`
//!   (`donors/mdx-m3-viewer/src/viewer/handlers/mdx/genericobject.ts:33`),
//! * the bind pose is the node transform with no animation applied, so every
//!   bind world matrix is the identity and skinned vertices equal the stored
//!   vertex positions,
//! * classic matrix-group skinning averages the world matrices of the group
//!   (`donors/mdx-m3-viewer/src/viewer/handlers/mdx/shaders/transforms.glsl.ts:24-46`),
//! * a track is only "active" for a sequence it was authored for; see
//!   [`sample_in_sequence`] for the loop-aware sampling rule.
//!
//! The canonical per-frame entry point for the game and the renderer is
//! [`runtime::ModelRuntime`]: it bundles the skeleton, the resolved skin
//! weights and the per-sequence visibility rule so no consumer keeps its own
//! copy of that math.

pub mod registry;
pub mod runtime;
pub mod state;

pub use registry::{
    AbilitySequence, AnimatorRegistry, GameplayState, ProjectileSequence, SequenceMatch,
    StateResolution, normalize_key,
};
pub use state::{ActivityOrder, UnitActivity, hover_uses_idle, select_state};
pub use runtime::{Frame, ModelRuntime, helper_plane, mesh_alphas};

use glam::{Mat4, Quat, Vec2, Vec3};
use wc3_formats::mdx::{
    AnimValue, Geoset, GeosetAnimation, Interpolation, MdxModel, Node, Sequence, Track,
};

#[derive(Debug, Clone, Copy)]
pub struct NodePose {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scaling: Vec3,
}

impl Default for NodePose {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scaling: Vec3::ONE,
        }
    }
}

impl NodePose {
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scaling, self.rotation, self.translation)
    }
}

pub fn sample_node(node: &Node, time: u32) -> NodePose {
    NodePose {
        translation: node.translation.sample(time).unwrap_or(Vec3::ZERO),
        rotation: node.rotation.sample(time).unwrap_or(Quat::IDENTITY),
        scaling: node.scaling.sample(time).unwrap_or(Vec3::ONE),
    }
}

/// Sample a scalar/vector track the way the engine does for a running sequence.
///
/// * global-sequence tracks are driven by the global counter modulo the global
///   sequence length,
/// * otherwise keys are clipped to the sequence interval and a track without
///   in-range keys yields `default` (never a key from another sequence).
pub fn sample_track_in_sequence<T: AnimValue>(
    track: &Track<T>,
    default: T,
    sequence: Option<&Sequence>,
    global_sequences: &[u32],
    time: u32,
) -> T {
    if let Some(index) = track.global_sequence {
        if let Some(&length) = global_sequences.get(index as usize) {
            if length > 0 {
                return track.sample(time % length).unwrap_or(default);
            }
        }
    }
    match sequence {
        Some(sequence) => sample_in_sequence(track, sequence, time).unwrap_or(default),
        None => track.sample(time).unwrap_or(default),
    }
}

/// [`sample_node`] with WC3 sequence clipping and global sequences.
pub fn sample_node_in_sequence(
    node: &Node,
    sequence: &Sequence,
    global_sequences: &[u32],
    time: u32,
) -> NodePose {
    NodePose {
        translation: sample_track_in_sequence(
            &node.translation,
            Vec3::ZERO,
            Some(sequence),
            global_sequences,
            time,
        ),
        rotation: sample_track_in_sequence(
            &node.rotation,
            Quat::IDENTITY,
            Some(sequence),
            global_sequences,
            time,
        ),
        scaling: sample_track_in_sequence(
            &node.scaling,
            Vec3::ONE,
            Some(sequence),
            global_sequences,
            time,
        ),
    }
}

/// Local matrix of a node around its pivot.
///
/// `T(pivot) * T(translation) * R(rotation) * S(scaling) * T(-pivot)`.
/// With an identity pose this is exactly the identity matrix, which is what
/// makes the stored vertices the bind pose of the model.
pub fn node_local_matrix(pivot: Vec3, pose: &NodePose) -> Mat4 {
    Mat4::from_translation(pivot)
        * pose.matrix()
        * Mat4::from_translation(-pivot)
}

/// Resolve each node's parent to an index, ignoring dangling ids and
/// self-references (`objectId == parentId` means "no parent", see
/// `donors/mdx-m3-viewer/src/viewer/handlers/mdx/genericobject.ts:51`).
pub fn resolve_parents(nodes: &[Node], index_by_id: &[Option<usize>]) -> Vec<Option<usize>> {
    nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let parent = node.parent_id;
            if parent < 0 || parent as u32 == node.object_id {
                return None;
            }
            index_by_id
                .get(parent as usize)
                .copied()
                .flatten()
                .filter(|&parent_index| parent_index != index)
        })
        .collect()
}

/// Dense `object_id -> node index` map; ids are small per model and this keeps
/// per-frame lookups cheap.
pub fn index_nodes_by_object_id(nodes: &[Node]) -> Vec<Option<usize>> {
    let size = nodes
        .iter()
        .map(|node| node.object_id as usize + 1)
        .max()
        .unwrap_or(0);
    let mut map = vec![None; size];
    for (index, node) in nodes.iter().enumerate() {
        if let Some(slot) = map.get_mut(node.object_id as usize) {
            *slot = Some(index);
        }
    }
    map
}

/// Compose world matrices for `nodes` in depth order.
///
/// MDX does not guarantee parent-before-child storage: WC3 skeletons routinely
/// store every BONE record before the HELP records that own them (Footman bones
/// 0..24 are children of helpers 25..39), so processing the vector order gives
/// children an identity parent and limbs detach as soon as a helper moves.
pub fn pose_worlds(
    nodes: &[Node],
    pivots: &[Vec3],
    parents: &[Option<usize>],
    poses: &[NodePose],
) -> Vec<Mat4> {
    let mut depth = vec![0usize; nodes.len()];
    for (index, _) in nodes.iter().enumerate() {
        let mut levels = 0usize;
        let mut parent = parents[index];
        while let Some(parent_index) = parent {
            levels += 1;
            if levels > nodes.len() {
                break;
            }
            parent = parents[parent_index];
        }
        depth[index] = levels;
    }
    let mut order: Vec<usize> = (0..nodes.len()).collect();
    order.sort_by_key(|&index| depth[index]);
    let mut world = vec![Mat4::IDENTITY; nodes.len()];
    for index in order {
        let pivot = pivots
            .get(nodes[index].object_id as usize)
            .copied()
            .unwrap_or(Vec3::ZERO);
        let local = node_local_matrix(pivot, &poses[index]);
        world[index] = match parents[index] {
            Some(parent_index) => world[parent_index] * local,
            None => local,
        };
    }
    world
}

/// WC3 bind pose: every node uses its unanimated transform, so every local
/// matrix is the identity and every world matrix is the identity too.
pub fn bind_worlds(nodes: &[Node], pivots: &[Vec3], parents: &[Option<usize>]) -> Vec<Mat4> {
    let poses = vec![NodePose::default(); nodes.len()];
    pose_worlds(nodes, pivots, parents, &poses)
}

/// A model skeleton in the index space the runtime and the MDX skin data use:
/// bones first (BONE chunk order == matrix-index order), then helpers (which
/// are the actual parents in WC3 skeletons).
#[derive(Debug, Clone)]
pub struct Skeleton {
    pub nodes: Vec<Node>,
    pub pivots: Vec<Vec3>,
    pub parents: Vec<Option<usize>>,
    pub index_by_object_id: Vec<Option<usize>>,
    pub bone_count: usize,
    pub global_sequences: Vec<u32>,
}

impl Skeleton {
    pub fn from_model(model: &MdxModel) -> Self {
        let mut nodes = Vec::with_capacity(model.bones.len() + model.helpers.len());
        for bone in &model.bones {
            nodes.push(bone.node.clone());
        }
        let bone_count = nodes.len();
        nodes.extend(model.helpers.iter().cloned());
        let index_by_object_id = index_nodes_by_object_id(&nodes);
        let parents = resolve_parents(&nodes, &index_by_object_id);
        Self {
            nodes,
            pivots: model.pivots.clone(),
            parents,
            index_by_object_id,
            bone_count,
            global_sequences: model.global_sequences.clone(),
        }
    }

    pub fn poses_at(&self, time_ms: u32) -> Vec<NodePose> {
        self.nodes
            .iter()
            .map(|node| sample_node(node, time_ms))
            .collect()
    }

    /// Poses for a running sequence.
    ///
    /// WC3 clips every track to the current sequence: keys outside the interval
    /// are ignored and a track without in-range keys falls back to the type
    /// default (identity rotation, zero translation, unit scale). Sampling the
    /// global timeline instead leaks poses between sequences - the Archer root
    /// ends up yawed 197 degrees in Walk because that is the last key of the
    /// Death sequence (`Archer.mdx` Bone_Root KGRT key at t=12567).
    pub fn poses_in_sequence(&self, sequence: &Sequence, time_ms: u32) -> Vec<NodePose> {
        self.nodes
            .iter()
            .map(|node| sample_node_in_sequence(node, sequence, &self.global_sequences, time_ms))
            .collect()
    }

    pub fn worlds_at(&self, time_ms: u32) -> Vec<Mat4> {
        pose_worlds(
            &self.nodes,
            &self.pivots,
            &self.parents,
            &self.poses_at(time_ms),
        )
    }

    pub fn worlds_in_sequence(&self, sequence: &Sequence, time_ms: u32) -> Vec<Mat4> {
        pose_worlds(
            &self.nodes,
            &self.pivots,
            &self.parents,
            &self.poses_in_sequence(sequence, time_ms),
        )
    }

    /// World matrices in BONE order (the order `matrix_indices` uses).
    pub fn bone_worlds_at(&self, time_ms: u32) -> Vec<Mat4> {
        let mut worlds = self.worlds_at(time_ms);
        worlds.truncate(self.bone_count);
        worlds
    }

    pub fn bone_worlds_in_sequence(&self, sequence: &Sequence, time_ms: u32) -> Vec<Mat4> {
        let mut worlds = self.worlds_in_sequence(sequence, time_ms);
        worlds.truncate(self.bone_count);
        worlds
    }

    pub fn bind_worlds(&self) -> Vec<Mat4> {
        bind_worlds(&self.nodes, &self.pivots, &self.parents)
    }

    pub fn bind_bone_worlds(&self) -> Vec<Mat4> {
        let mut worlds = self.bind_worlds();
        worlds.truncate(self.bone_count);
        worlds
    }

    /// Root objects of the hierarchy (nodes without a parent), in node order.
    pub fn roots(&self) -> impl Iterator<Item = usize> + '_ {
        self.parents
            .iter()
            .enumerate()
            .filter(|(_, parent)| parent.is_none())
            .map(|(index, _)| index)
    }
}

#[derive(Debug, Clone)]
pub struct Animator {
    pub nodes: Vec<Node>,
    pub pivots: Vec<Vec3>,
    pub global_sequences: Vec<u32>,
    pub sequence: Option<Sequence>,
    pub time_ms: f32,
    pub speed: f32,
}

fn empty_node(object_id: u32) -> Node {
    Node {
        name: String::new(),
        object_id,
        parent_id: -1,
        flags: 0,
        translation: Track {
            interpolation: Interpolation::Linear,
            global_sequence: None,
            keys: Vec::new(),
            static_value: Some(Vec3::ZERO),
        },
        rotation: Track {
            interpolation: Interpolation::Linear,
            global_sequence: None,
            keys: Vec::new(),
            static_value: Some(Quat::IDENTITY),
        },
        scaling: Track {
            interpolation: Interpolation::Linear,
            global_sequence: None,
            keys: Vec::new(),
            static_value: Some(Vec3::ONE),
        },
    }
}

impl Animator {
    /// Build the node array indexed by the MDX **node object id**, not by the
    /// order bones happen to appear in the BONE chunk. Geoset skin/matrix
    /// indices are node indices; a model that interleaves attachments, events
    /// and helpers with bones (Peasant, TownHall, most props) otherwise maps
    /// limb vertices to the wrong node and the mesh hangs in the air.
    pub fn from_model(model: &MdxModel) -> Self {
        let max_id = model
            .bones
            .iter()
            .map(|bone| bone.node.object_id)
            .chain(model.helpers.iter().map(|node| node.object_id))
            .chain(model.attachments.iter().map(|attachment| attachment.node.object_id))
            .chain(model.events.iter().map(|node| node.object_id))
            .max()
            .unwrap_or(0) as usize;
        // Cover both the highest object id and every pivot so matrix indices
        // and pivot lookup always stay in range (some models leave gaps).
        let node_count = (max_id + 1).max(model.pivots.len());
        let mut nodes: Vec<Node> = (0..node_count as u32).map(empty_node).collect();
        for bone in &model.bones {
            nodes[bone.node.object_id as usize] = bone.node.clone();
        }
        for helper in &model.helpers {
            nodes[helper.object_id as usize] = helper.clone();
        }
        for attachment in &model.attachments {
            nodes[attachment.node.object_id as usize] = attachment.node.clone();
        }
        for event in &model.events {
            nodes[event.object_id as usize] = event.clone();
        }
        Self {
            nodes,
            pivots: model.pivots.clone(),
            global_sequences: model.global_sequences.clone(),
            sequence: None,
            time_ms: 0.0,
            speed: 1.0,
        }
    }

    pub fn play(&mut self, sequence: &Sequence) {
        self.sequence = Some(sequence.clone());
        self.time_ms = sequence.start as f32;
    }

    pub fn update(&mut self, dt_seconds: f32) {
        self.time_ms += dt_seconds * 1000.0 * self.speed;
        if let Some(sequence) = &self.sequence {
            // `end - start` can underflow for sentinel sequences (HeroFarSeer
            // ships one named "nothing" with start 0xFFFFFF59 and end 0), so the
            // loop arithmetic works on floats and clamps instead of wrapping.
            let start = sequence.start as f32;
            let end = sequence.end as f32;
            let length = (end - start).max(1.0);
            if self.time_ms > end {
                // Non-looping sequences (`flags & 1`: Death, Birth, attack
                // variants) hold the last frame; wrapping them shows a corpse
                // or a spawned building snapping back into the first pose.
                if sequence.flags & 1 != 0 {
                    self.time_ms = end;
                } else {
                    self.time_ms = start + (self.time_ms - end) % length;
                }
            } else if self.time_ms < start {
                self.time_ms = start;
            }
        }
    }

    pub fn frame_time(&self) -> u32 {
        self.time_ms.max(0.0) as u32
    }

    pub fn poses(&self) -> Vec<NodePose> {
        let time = self.frame_time();
        match &self.sequence {
            // A playing sequence must clip every track to its interval: WC3
            // never samples a key that belongs to another sequence, and a track
            // without keys in this interval means "rest pose".
            Some(sequence) => self
                .nodes
                .iter()
                .map(|node| {
                    sample_node_in_sequence(node, sequence, &self.global_sequences, time)
                })
                .collect(),
            None => self
                .nodes
                .iter()
                .map(|node| sample_node(node, time))
                .collect(),
        }
    }

    /// Local matrices composed into world space using the node parent ids.
    ///
    /// MDX files do not guarantee that a parent node is stored before its
    /// children (Footman: BONE records 0..24 are parented to HELP records
    /// 25..39), so composition resolves parents by object id in depth order and
    /// indexes PIVT by object id like the engine. The actual math lives in
    /// [`pose_worlds`] so the animator and the analysis skeleton cannot drift.
    pub fn world_matrices(&self) -> Vec<Mat4> {
        let index_by_id = index_nodes_by_object_id(&self.nodes);
        let parents = resolve_parents(&self.nodes, &index_by_id);
        pose_worlds(&self.nodes, &self.pivots, &parents, &self.poses())
    }
}

/// Simple deterministic particle emitter state.
#[derive(Debug, Clone)]
pub struct Particle {
    pub position: Vec3,
    pub velocity: Vec3,
    pub age: f32,
    pub lifetime: f32,
    pub size: f32,
}

#[derive(Debug, Clone)]
pub struct ParticleSystem {
    pub particles: Vec<Particle>,
    pub spawn_rate: f32,
    pub accumulator: f32,
    pub gravity: Vec3,
}

impl ParticleSystem {
    pub fn new(spawn_rate: f32, gravity: Vec3) -> Self {
        Self {
            particles: Vec::new(),
            spawn_rate,
            accumulator: 0.0,
            gravity,
        }
    }

    pub fn update(&mut self, dt: f32, origin: Vec3) {
        self.accumulator += self.spawn_rate * dt;
        while self.accumulator >= 1.0 {
            self.accumulator -= 1.0;
            self.particles.push(Particle {
                position: origin,
                velocity: Vec3::new(0.0, 40.0, 0.0),
                age: 0.0,
                lifetime: 1.0,
                size: 8.0,
            });
        }
        for particle in &mut self.particles {
            particle.age += dt;
            particle.velocity += self.gravity * dt;
            particle.position += particle.velocity * dt;
        }
        self.particles.retain(|p| p.age < p.lifetime);
    }
}

/// Sample a raw track at a time (public helper for tests/tools).
pub fn sample_track<T: wc3_formats::mdx::AnimValue>(track: &Track<T>, time: u32) -> Option<T> {
    track.sample(time)
}

/// Sequence-aware track sampling with WC3 loop semantics.
///
/// The game does not sample a global timeline: it clips the track to the
/// sequence's `[start, end]` keys. Looping sequences also interpolate across
/// the sequence boundary (so a Walk cycle does not hitch), while non-looping
/// sequences (`flags & 1`, Death/Birth/Attack variants) hold the first/last
/// in-range key instead. The wrap rule is ported from
/// `donors/mdx-m3-viewer/src/viewer/handlers/mdx/sd.ts` (`SdSequence`); the
/// non-looping clamp is required by real data: the GreatHall `shockwave` scale
/// track is Bezier with a 100.2 control value on its first key, and wrapping
/// `Death` back to that key scales the quad up until vertices sit 9001 units
/// from the building.
///
/// Returns `None` when the track has no usable value for the sequence (the
/// caller supplies the type default: zero translation, identity rotation, one
/// scaling, one alpha).
pub fn sample_in_sequence<T: AnimValue>(
    track: &Track<T>,
    sequence: &Sequence,
    time: u32,
) -> Option<T> {
    if let Some(value) = track.static_value {
        return Some(value);
    }
    if track.keys.is_empty() {
        return None;
    }
    let in_range: Vec<&wc3_formats::mdx::Key<T>> = track
        .keys
        .iter()
        .filter(|key| key.time >= sequence.start && key.time <= sequence.end)
        .collect();
    if in_range.is_empty() {
        return None;
    }
    if in_range.len() == 1 {
        return Some(in_range[0].value);
    }
    // Bit 0 of the sequence flags marks a non-looping animation.
    let looping = sequence.flags & 1 == 0;
    let frame = time as f32;
    let last = in_range.len() - 1;
    if !looping {
        if frame <= in_range[0].time as f32 {
            return Some(in_range[0].value);
        }
        if frame >= in_range[last].time as f32 {
            return Some(in_range[last].value);
        }
    }
    let wrap_span = in_range[0].time as f32 - in_range[last].time as f32
        + (sequence.end as f32 - sequence.start as f32);
    if frame < in_range[0].time as f32 {
        let t = if wrap_span <= 0.0 {
            0.0
        } else {
            (frame - in_range[0].time as f32) / wrap_span
        };
        return Some(interpolate_key(in_range[last], in_range[0], t, track.interpolation));
    }
    if frame >= in_range[last].time as f32 {
        let t = if wrap_span <= 0.0 {
            0.0
        } else {
            (frame - in_range[last].time as f32) / wrap_span
        };
        return Some(interpolate_key(in_range[last], in_range[0], t, track.interpolation));
    }
    for pair in in_range.windows(2) {
        let start = pair[0].time as f32;
        let end = pair[1].time as f32;
        if frame >= start && frame <= end {
            let span = (end - start).max(1.0);
            let t = (frame - start) / span;
            return Some(interpolate_key(pair[0], pair[1], t, track.interpolation));
        }
    }
    Some(in_range[last].value)
}

fn interpolate_key<T: AnimValue>(
    a: &wc3_formats::mdx::Key<T>,
    b: &wc3_formats::mdx::Key<T>,
    t: f32,
    interpolation: wc3_formats::mdx::Interpolation,
) -> T {
    match interpolation {
        wc3_formats::mdx::Interpolation::None => a.value,
        wc3_formats::mdx::Interpolation::Linear => T::lerp(a.value, b.value, t),
        wc3_formats::mdx::Interpolation::Hermite => T::hermite(a.value, a.out_tan, b.value, b.in_tan, t),
        wc3_formats::mdx::Interpolation::Bezier => T::bezier(a.value, a.out_tan, b.in_tan, b.value, t),
    }
}

/// Per-vertex bone influences resolved from a geoset's matrix groups.
///
/// Classic models store `GNDX` (group per vertex), `MTGC` (matrix count per
/// group) and `MATS` (flattened bone indices). Reforged models may instead ship
/// a packed `SKIN` array of four `(bone, weight)` byte pairs per vertex. Weights
/// are normalised; classic groups use equal weights, exactly like the engine
/// (`transforms.glsl.ts` divides the summed matrices by the bone count).
#[derive(Debug, Clone, Default)]
pub struct GeosetSkin {
    /// `(bone index, weight)` per vertex.
    pub influences: Vec<Vec<(u16, f32)>>,
    pub max_influences: usize,
}

/// Maximum bone influences one vertex is evaluated with.
///
/// The TFT engine evaluates a classic matrix group **whole**: `IMatrixGroup`
/// (`CMatrixGroup`, 20 bytes `{first MATS index, count, group slot, -1, -1}`)
/// feeds every matrix of the group into the skinning loop, and the measured
/// corpus maximum is five (`Archer` geoset 0; r2 lane 405, `fcn.6f032c50`).
/// The old cap of four silently dropped the fifth matrix and renormalised the
/// rest, which detaches exactly those vertices from their limb.
///
/// The mesh renderer keeps four slots in the GPU vertex format; a geoset whose
/// group needs five is skinned on the CPU for that draw (see
/// `wc3_render::mesh::prepare`), so the result is still the full five-matrix
/// average.
pub const MAX_INFLUENCES: usize = 5;

impl GeosetSkin {
    /// Resolve every vertex to at most [`MAX_INFLUENCES`] bones with normalised
    /// weights (the invariant asserted by `wc3-game`'s
    /// `geoaset_skinning_weights_are_normalised`).
    pub fn from_geoset(geoset: &Geoset) -> Self {
        Self::from_geoset_with_limit(geoset, MAX_INFLUENCES)
    }

    /// Same resolution with an explicit influence budget (`wc3-gpu` uploads at
    /// most four; an analysis tool may pass a larger budget to inspect raw
    /// groups).
    pub fn from_geoset_with_limit(geoset: &Geoset, limit: usize) -> Self {
        let limit = limit.max(1);
        let vertex_count = geoset.vertices.len();
        let mut influences = vec![Vec::new(); vertex_count];
        let packed_skin = geoset.skin.len() >= vertex_count * 8;
        if packed_skin {
            for (vertex, slot) in influences.iter_mut().enumerate() {
                let base = vertex * 8;
                let mut total = 0.0f32;
                let mut pairs = Vec::new();
                for pair in 0..4 {
                    let bone = geoset.skin[base + pair * 2];
                    let weight = geoset.skin[base + pair * 2 + 1] as f32;
                    if weight > 0.0 {
                        pairs.push((bone, weight));
                        total += weight;
                    }
                }
                if total > 0.0 {
                    pairs.sort_by(|a, b| b.1.total_cmp(&a.1));
                    pairs.truncate(limit);
                    let kept: f32 = pairs.iter().map(|(_, weight)| *weight).sum();
                    if kept > 0.0 {
                        *slot = pairs
                            .into_iter()
                            .map(|(bone, weight)| (bone, weight / kept))
                            .collect();
                    }
                }
            }
        } else if !geoset.matrix_groups.is_empty() && !geoset.vertex_groups.is_empty() {
            let mut offsets = Vec::with_capacity(geoset.matrix_groups.len());
            let mut offset = 0usize;
            for size in &geoset.matrix_groups {
                offsets.push(offset);
                offset += *size as usize;
            }
            for (vertex, slot) in influences.iter_mut().enumerate() {
                let group = geoset.vertex_groups.get(vertex).copied().unwrap_or(0) as usize;
                let Some(&group_offset) = offsets.get(group) else {
                    continue;
                };
                let level = geoset.matrix_groups[group] as usize;
                if level == 0 {
                    continue;
                }
                // Clamp the group to the influence budget. Classic groups use
                // equal weights, so keeping the first `keep` matrices and
                // renormalising by `1 / keep` preserves the average the engine
                // computes for the slots it actually evaluates.
                let keep = level.min(limit);
                let weight = 1.0 / keep as f32;
                for index in group_offset..group_offset + keep {
                    if let Some(&bone) = geoset.matrix_indices.get(index) {
                        slot.push((bone as u16, weight));
                    }
                }
            }
        }
        let max_influences = influences.iter().map(Vec::len).max().unwrap_or(0);
        Self {
            influences,
            max_influences,
        }
    }

    /// Number of vertices that deform (have at least one valid bone).
    pub fn skinned_vertices(&self) -> usize {
        self.influences
            .iter()
            .filter(|list| !list.is_empty())
            .count()
    }
}

/// Skinned model-space positions for a geoset.
///
/// `bone_worlds` must be in BONE chunk order (the order `matrix_indices` and
/// the runtime's bone array use). Vertices without influences stay in bind pose.
pub fn skin_positions(geoset: &Geoset, skin: &GeosetSkin, bone_worlds: &[Mat4]) -> Vec<Vec3> {
    // One implementation of the weighted sum lives in `runtime::skin_point`;
    // the renderer path (`runtime::skin_vertex`) shares it through the same
    // accumulator, so the game and the audit cannot drift.
    geoset
        .vertices
        .iter()
        .enumerate()
        .map(|(vertex, position)| match skin.influences.get(vertex) {
            Some(list) if !list.is_empty() => {
                runtime::skin_point(*position, list.iter().copied(), bone_worlds)
            }
            _ => *position,
        })
        .collect()
}

/// True when `track` contains a real animation inside `sequence`: at least two
/// in-range keys with different values. WC3 only applies such a track to the
/// sequence it was authored for; single-key tracks are constants and sequences
/// without keys fall back to the object's static value.
pub fn track_active_in_sequence(track: &Track<f32>, sequence: &Sequence) -> bool {
    let mut first: Option<u32> = None;
    for key in &track.keys {
        if key.time < sequence.start || key.time > sequence.end {
            continue;
        }
        match first {
            None => first = Some(key.value.to_bits()),
            Some(bits) if bits != key.value.to_bits() => return true,
            Some(_) => {}
        }
    }
    false
}

/// Geoset alpha for a sequence, following the same rule the game uses for
/// geoset-animation variants.
///
/// * animated track that is a real variant of this sequence -> sampled value,
/// * otherwise the geoset animation's static alpha applies.
///
/// The distinction matters on real models: the Footman `gutz` geoset ships a
/// single `0` key per non-decay sequence and must stay hidden there, while the
/// TownHall relies on the static alpha of its construction geosets.
pub fn geoset_alpha(
    animations: &[GeosetAnimation],
    geoset_id: u32,
    sequence: Option<&Sequence>,
    time_ms: u32,
) -> f32 {
    let Some(animation) = animations
        .iter()
        .find(|animation| animation.geoset_id == geoset_id)
    else {
        return 1.0;
    };
    match (&animation.alpha_track, sequence) {
        (Some(track), Some(sequence)) if track_active_in_sequence(track, sequence) => {
            track.sample(time_ms).unwrap_or(animation.alpha)
        }
        _ => animation.alpha,
    }
}

/// Sampled geoset alpha that honours single-key tracks as constants.
///
/// `geoset_alpha` skips non-variant tracks entirely; this variant returns the
/// constant value for a sequence that has exactly one key (and the static alpha
/// when the sequence has no keys at all). Both behaviours exist in the wild;
/// the audit tool reports which geosets differ.
pub fn geoset_alpha_constant_rule(
    animations: &[GeosetAnimation],
    geoset_id: u32,
    sequence: Option<&Sequence>,
    time_ms: u32,
) -> f32 {
    let Some(animation) = animations
        .iter()
        .find(|animation| animation.geoset_id == geoset_id)
    else {
        return 1.0;
    };
    let Some(track) = animation.alpha_track.as_ref() else {
        return animation.alpha;
    };
    match sequence {
        Some(sequence) => sample_in_sequence(track, sequence, time_ms).unwrap_or(animation.alpha),
        None => track.sample(time_ms).unwrap_or(animation.alpha),
    }
}

/// Convert an MDX (Z-up, right-handed) vector to the renderer's Y-up space.
pub fn z_up_to_y_up(value: Vec3) -> Vec3 {
    Vec3::new(value.x, value.z, -value.y)
}

/// UV set selected by a material layer's `coord_id` (0 primary, 1 secondary).
pub fn layer_uv<'a>(geoset: &'a Geoset, coord_id: u32) -> Option<&'a Vec<Vec2>> {
    geoset.uv_sets.get(coord_id as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use wc3_formats::mdx::{Interpolation, Key, Track};

    fn model_path(relative: &str) -> Option<std::path::PathBuf> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for tree in ["analysis/war3_samples", "analysis/war3_full"] {
            let candidate = root.join(tree).join(relative);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        None
    }

    fn load(relative: &str) -> Option<MdxModel> {
        let path = model_path(relative)?;
        let bytes = std::fs::read(path).ok()?;
        Some(MdxModel::parse(&bytes).expect("parse fixture"))
    }

    fn worst_extent_ratio(
        model: &MdxModel,
        skeleton: &Skeleton,
        skins: &[GeosetSkin],
        sequence_index: usize,
        times: &[u32],
    ) -> f32 {
        let sequence = &model.sequences[sequence_index];
        let mut worst = 0.0f32;
        for &time in times {
            let worlds = skeleton.bone_worlds_in_sequence(sequence, time);
            for (index, geoset) in model.geosets.iter().enumerate() {
                let Some(extent) = geoset.sequence_extent(sequence_index) else {
                    continue;
                };
                let diagonal = extent.diagonal().max(1.0);
                for position in skin_positions(geoset, &skins[index], &worlds) {
                    assert!(position.is_finite(), "non-finite skinned vertex");
                    worst = worst.max(extent.overshoot(position) / diagonal);
                }
            }
        }
        worst
    }

    fn yaw_of(matrix: &Mat4) -> f32 {
        matrix.y_axis.x.atan2(matrix.x_axis.x).to_degrees()
    }

    #[test]
    fn bind_pose_is_identity_on_real_models() {
        for relative in [
            "Units/Human/Footman/Footman.mdx",
            "Units/NightElf/Archer/Archer.mdx",
            "Units/Human/Peasant/peasant.mdx",
            "Buildings/Human/TownHall/TownHall.mdx",
        ] {
            let Some(model) = load(relative) else {
                eprintln!("skipping {relative}: fixture missing");
                continue;
            };
            let skeleton = Skeleton::from_model(&model);
            for (index, matrix) in skeleton.bind_worlds().iter().enumerate() {
                let deviation = (*matrix - Mat4::IDENTITY)
                    .to_cols_array()
                    .iter()
                    .fold(0.0f32, |acc, value| acc.max(value.abs()));
                assert!(
                    deviation < 1e-4,
                    "{relative} node {index} ({}) bind world deviates by {deviation}",
                    skeleton.nodes[index].name
                );
            }
            // Bind skinning must reproduce the stored vertices exactly.
            let skins: Vec<GeosetSkin> = model.geosets.iter().map(GeosetSkin::from_geoset).collect();
            let bind = skeleton.bind_bone_worlds();
            for (index, geoset) in model.geosets.iter().enumerate() {
                for (vertex, position) in skin_positions(geoset, &skins[index], &bind)
                    .into_iter()
                    .enumerate()
                {
                    let expected = geoset.vertices[vertex];
                    assert!(
                        (position - expected).length() < 1e-3,
                        "{relative} geoset {index} vertex {vertex} moved at bind pose"
                    );
                }
            }
        }
    }

    #[test]
    fn footman_and_archer_animation_matches_authored_extents() {
        for (relative, wanted) in [
            ("Units/Human/Footman/Footman.mdx", vec!["Stand", "Walk", "Attack", "Death"]),
            ("Units/NightElf/Archer/Archer.mdx", vec!["Stand", "Walk", "Attack", "Death"]),
            ("Units/Human/Peasant/peasant.mdx", vec!["Stand", "Walk", "Attack", "Death"]),
            ("Buildings/Human/TownHall/TownHall.mdx", vec!["Stand", "Death"]),
        ] {
            let Some(model) = load(relative) else {
                eprintln!("skipping {relative}: fixture missing");
                continue;
            };
            let skeleton = Skeleton::from_model(&model);
            let skins: Vec<GeosetSkin> = model.geosets.iter().map(GeosetSkin::from_geoset).collect();
            for needle in wanted {
                let (sequence_index, sequence) = model
                    .sequences
                    .iter()
                    .enumerate()
                    .find(|(_, sequence)| {
                        sequence
                            .name
                            .to_ascii_lowercase()
                            .contains(&needle.to_ascii_lowercase())
                    })
                    .unwrap_or_else(|| panic!("{relative}: no sequence containing {needle}"));
                let times = [sequence.start, (sequence.start + sequence.end) / 2, sequence.end];
                let worst =
                    worst_extent_ratio(&model, &skeleton, &skins, sequence_index, &times);
                assert!(
                    worst < 0.05,
                    "{relative} {}: skinned vertices overshoot their authored extents by {:.1}%",
                    sequence.name,
                    worst * 100.0
                );
            }
        }
    }

    #[test]
    fn archer_walk_does_not_inherit_the_death_root_rotation() {
        let Some(model) = load("Units/NightElf/Archer/Archer.mdx") else {
            eprintln!("skipping: Archer fixture missing");
            return;
        };
        let skeleton = Skeleton::from_model(&model);
        let index = skeleton
            .nodes
            .iter()
            .position(|node| node.name == "Bone_Root")
            .expect("Archer Bone_Root");
        let walk = model.sequence("Walk").expect("Walk sequence");
        let death = model.sequence("Death").expect("Death sequence");

        // Death really does yaw the root: the key at t=12333 (and 12567) is ~197
        // degrees, so the middle of the Death sequence is already fully rotated.
        let death_mid = death.start + (death.end - death.start) / 2;
        let death_worlds = skeleton.worlds_in_sequence(death, death_mid);
        let death_yaw = yaw_of(&death_worlds[index]);
        assert!(
            death_yaw.abs() > 90.0,
            "expected the Death pose to rotate the root, got {death_yaw}"
        );

        // Walk has no root keys of its own, so the pose must fall back to rest
        // instead of clamping to the last Death key (the old global sampler did
        // that and laid the whole archer down against its extents).
        let walk_worlds = skeleton.worlds_in_sequence(walk, walk.start);
        let walk_yaw = yaw_of(&walk_worlds[index]);
        assert!(
            walk_yaw.abs() < 1.0,
            "Walk inherited a {walk_yaw} degree root yaw from Death"
        );

        let global_worlds = skeleton.worlds_at(walk.start);
        let leaked_yaw = yaw_of(&global_worlds[index]);
        assert!(
            leaked_yaw.abs() > 90.0,
            "global sampling should be the buggy path, got {leaked_yaw}"
        );
    }

    #[test]
    fn townhall_visibility_comes_from_sequence_extents() {
        let Some(model) = load("Buildings/Human/TownHall/TownHall.mdx") else {
            eprintln!("skipping: TownHall fixture missing");
            return;
        };
        let stand_index = model
            .sequences
            .iter()
            .position(|sequence| sequence.name == "Stand")
            .expect("TownHall Stand");
        let stand = &model.sequences[stand_index];
        let present: Vec<usize> = model
            .geosets
            .iter()
            .enumerate()
            .filter(|(_, geoset)| geoset.present_in_sequence(stand_index))
            .map(|(index, _)| index)
            .collect();
        assert!(
            !present.is_empty() && present.len() < model.geosets.len(),
            "expected a subset of TownHall geosets in Stand, got {}",
            present.len()
        );
        assert_eq!(
            present,
            vec![11, 13, 14],
            "TownHall Stand is built from exactly these geosets"
        );
        // The shell geoset carries a single-key KGAO value of 0 for Stand, which
        // the variant rule ignores (static alpha 1.0): the extent sentinel, not
        // alpha, is what keeps the castle tiers out of the base building.
        let shell = 0usize;
        assert!(
            !model.geosets[shell].present_in_sequence(stand_index),
            "TownHall shell geoset should not be part of Stand"
        );
        assert_eq!(
            geoset_alpha(
                &model.geoset_animations,
                shell as u32,
                Some(stand),
                stand.start
            ),
            1.0,
            "single-key KGAO is not a variant; alpha alone would show the geoset"
        );
        // Birth does not show the shell either: the base town hall is geoset 4-10.
        let birth_index = model
            .sequences
            .iter()
            .position(|sequence| sequence.name == "Birth")
            .expect("TownHall Birth");
        assert!(!model.geosets[shell].present_in_sequence(birth_index));
        let birth_present: Vec<usize> = model
            .geosets
            .iter()
            .enumerate()
            .filter(|(_, geoset)| geoset.present_in_sequence(birth_index))
            .map(|(index, _)| index)
            .collect();
        assert!(
            birth_present.contains(&4) && birth_present.len() > present.len(),
            "Birth builds the town hall from more geosets than Stand: {birth_present:?}"
        );
    }

    #[test]
    fn footman_effect_geosets_are_sequence_scoped() {
        let Some(model) = load("Units/Human/Footman/Footman.mdx") else {
            eprintln!("skipping: Footman fixture missing");
            return;
        };
        let stand = model
            .sequences
            .iter()
            .position(|sequence| sequence.name.starts_with("Stand - 1"))
            .expect("Footman Stand");
        let decay = model
            .sequences
            .iter()
            .position(|sequence| sequence.name == "Decay Flesh")
            .expect("Footman Decay Flesh");
        // geoset 4 is the gutz/blood shell: absent in Stand, present while decaying.
        assert!(!model.geosets[4].present_in_sequence(stand));
        assert!(model.geosets[4].present_in_sequence(decay));
        // geoset 2 is the sword glint that only animates during "Stand - 4".
        assert!(!model.geosets[2].present_in_sequence(stand));
        let stand4 = model
            .sequences
            .iter()
            .position(|sequence| sequence.name == "Stand - 4")
            .expect("Footman Stand - 4");
        assert!(model.geosets[2].present_in_sequence(stand4));
        assert!(model.geosets[3].present_in_sequence(stand4));
    }

    #[test]
    fn skin_influences_are_resolved_from_matrix_groups() {
        let Some(model) = load("Units/Human/Footman/Footman.mdx") else {
            eprintln!("skipping: Footman fixture missing");
            return;
        };
        let skin = GeosetSkin::from_geoset(&model.geosets[0]);
        assert_eq!(skin.max_influences, 3);
        assert_eq!(skin.skinned_vertices(), model.geosets[0].vertices.len());
        for (vertex, list) in skin.influences.iter().enumerate() {
            let total: f32 = list.iter().map(|(_, weight)| weight).sum();
            assert!(
                (total - 1.0).abs() < 1e-4,
                "vertex {vertex} weights sum to {total}"
            );
            for &(bone, _) in list {
                assert!(
                    (bone as usize) < model.bones.len(),
                    "vertex {vertex} references bone {bone} of {}",
                    model.bones.len()
                );
            }
        }
        if let Some(archer) = load("Units/NightElf/Archer/Archer.mdx") {
            // Raw matrix groups declare five bones for some vertices; the
            // canonical extraction clamps to the GPU palette (4) and
            // renormalises. The raw count stays observable with an explicit
            // budget for diagnostics.
            let raw = GeosetSkin::from_geoset_with_limit(&archer.geosets[0], 8);
            assert_eq!(raw.max_influences, 5, "Archer raw group has 5 matrices");
            let skin = GeosetSkin::from_geoset(&archer.geosets[0]);
            assert_eq!(skin.max_influences, MAX_INFLUENCES, "five matrices kept whole");
            for (vertex, list) in skin.influences.iter().enumerate() {
                assert!(list.len() <= MAX_INFLUENCES, "vertex {vertex}");
                let total: f32 = list.iter().map(|(_, weight)| weight).sum();
                assert!((total - 1.0).abs() < 1e-4, "vertex {vertex} sum {total}");
            }
        }
    }

    fn node_with_tracks() -> Node {
        Node {
            name: "bone".into(),
            object_id: 0,
            parent_id: -1,
            flags: 0x100,
            translation: Track {
                interpolation: Interpolation::Linear,
                global_sequence: None,
                keys: vec![
                    Key {
                        time: 0,
                        value: Vec3::ZERO,
                        in_tan: Vec3::ZERO,
                        out_tan: Vec3::ZERO,
                    },
                    Key {
                        time: 1000,
                        value: Vec3::new(100.0, 0.0, 0.0),
                        in_tan: Vec3::ZERO,
                        out_tan: Vec3::ZERO,
                    },
                ],
                static_value: None,
            },
            rotation: Track {
                interpolation: Interpolation::None,
                global_sequence: None,
                keys: Vec::new(),
                static_value: Some(Quat::IDENTITY),
            },
            scaling: Track {
                interpolation: Interpolation::None,
                global_sequence: None,
                keys: Vec::new(),
                static_value: Some(Vec3::ONE),
            },
        }
    }

    #[test]
    fn samples_translation_and_builds_matrix() {
        let node = node_with_tracks();
        let pose = sample_node(&node, 500);
        assert!((pose.translation.x - 50.0).abs() < 1e-3);
        let matrix = pose.matrix();
        assert!((matrix.w_axis.x - 50.0).abs() < 1e-3);
    }

    #[test]
    fn update_survives_sentinel_sequences() {
        // HeroFarSeer.mdx ships a filler sequence "nothing" with
        // start = 0xFFFFFF59 and end = 0; the old loop subtractor underflowed.
        let sequence = Sequence {
            name: "nothing".into(),
            start: 4_294_967_129u32,
            end: 0,
            move_speed: 0.0,
            flags: 0,
            rarity: 0.0,
            sync_point: 0,
            bounds: Default::default(),
        };
        let mut animator = Animator {
            nodes: Vec::new(),
            pivots: Vec::new(),
            global_sequences: Vec::new(),
            sequence: None,
            time_ms: 0.0,
            speed: 1.0,
        };
        animator.play(&sequence);
        animator.update(0.5);
        assert!(animator.time_ms.is_finite());
        assert!(animator.time_ms >= 0.0);
    }

    #[test]
    fn runtime_animator_path_animates_real_models() {
        // Mirrors what crates/wc3-game/src/game.rs does every frame:
        // Animator::from_model -> play(sequence) -> update(dt) ->
        // world_matrices() -> per-vertex skinning with mesh.bones indices.
        for (relative, stand) in [
            ("Units/Human/Footman/Footman.mdx", "Stand - 1"),
            ("Units/NightElf/Archer/Archer.mdx", "Stand"),
            ("Units/Human/Peasant/peasant.mdx", "Stand"),
            ("Buildings/Human/TownHall/TownHall.mdx", "Stand"),
        ] {
            let Some(model) = load(relative) else {
                eprintln!("skipping {relative}: fixture missing");
                continue;
            };
            let mut animator = Animator::from_model(&model);
            let sequence = model.sequence(stand).expect("stand sequence").clone();
            animator.play(&sequence);
            let half = (sequence.end - sequence.start) as f32 / 2000.0;
            for _ in 0..10 {
                animator.update(half / 10.0);
            }
            let worlds = animator.world_matrices();
            // Node indices are MDX object ids, so the array spans every pivot
            // (bones, helpers, attachments, events), not just bones+helpers.
            assert_eq!(
                worlds.len(),
                model.pivots.len(),
                "{relative}: animator nodes must be indexed by object id"
            );
            assert!(
                model
                    .bones
                    .iter()
                    .all(|bone| (bone.node.object_id as usize) < worlds.len()),
                "{relative}: bone object id outside the node array"
            );
            assert!(
                worlds
                    .iter()
                    .all(|matrix| matrix.to_cols_array().iter().all(|v| v.is_finite())),
                "{relative}: non-finite node matrix"
            );
            let sequence_index = model
                .sequences
                .iter()
                .position(|candidate| candidate.name == stand)
                .expect("sequence index");
            let skins: Vec<GeosetSkin> = model.geosets.iter().map(GeosetSkin::from_geoset).collect();
            let mut worst = 0.0f32;
            for (index, geoset) in model.geosets.iter().enumerate() {
                let Some(extent) = geoset.sequence_extent(sequence_index) else {
                    continue;
                };
                let diagonal = extent.diagonal().max(1.0);
                for position in skin_positions(geoset, &skins[index], &worlds) {
                    assert!(position.is_finite(), "{relative}: non-finite vertex");
                    worst = worst.max(extent.overshoot(position) / diagonal);
                }
            }
            assert!(
                worst < 0.05,
                "{relative} {stand}: runtime animator path overshoots extents by {:.1}%",
                worst * 100.0
            );
        }
    }

    #[test]
    fn animator_holds_non_looping_death_at_the_last_frame() {
        // WC3 keeps a corpse on the last death pose; the animator must not wrap
        // a non-looping sequence (flags & 1) back to its start.
        let Some(path) = model_path("Units/Human/Footman/Footman.mdx") else {
            eprintln!("skipping: Footman fixture missing");
            return;
        };
        let bytes = std::fs::read(&path).expect("read footman");
        let model = MdxModel::parse(&bytes).expect("parse footman");
        let death = model
            .sequences
            .iter()
            .find(|sequence| sequence.name.eq_ignore_ascii_case("Death"))
            .cloned()
            .expect("footman death sequence");
        assert_eq!(death.flags & 1, 1, "Death must be authored non-looping");
        let mut animator = Animator::from_model(&model);
        animator.play(&death);
        let length = (death.end - death.start) as f32 / 1000.0;
        animator.update(length * 2.0 + 0.5);
        assert_eq!(
            animator.frame_time(),
            death.end,
            "non-looping Death must hold at the sequence end"
        );
    }

    #[test]
    fn looping_sequences_wrap_but_non_looping_sequences_hold() {
        // A single translation track with keys at 100 and 900 inside a sequence
        // of 0..1000: 0.5 of the way from the last key to the sequence end the
        // looping sampler has wrapped to 0.5, the non-looping one is still at 1.0.
        let track = Track {
            interpolation: Interpolation::Linear,
            global_sequence: None,
            keys: vec![
                Key {
                    time: 100,
                    value: 0.0f32,
                    in_tan: 0.0,
                    out_tan: 0.0,
                },
                Key {
                    time: 900,
                    value: 1.0f32,
                    in_tan: 0.0,
                    out_tan: 0.0,
                },
            ],
            static_value: None,
        };
        let looping = Sequence {
            name: "Walk".into(),
            start: 0,
            end: 1000,
            move_speed: 0.0,
            flags: 0,
            rarity: 0.0,
            sync_point: 0,
            bounds: wc3_formats::mdx::Extent::default(),
        };
        let non_looping = Sequence {
            flags: 1,
            name: "Death".into(),
            ..looping.clone()
        };
        // t=950 is 50/200 of the wrapped span (900 -> 100 across the 1000 ms loop).
        let wrapped = sample_in_sequence(&track, &looping, 950).expect("sample");
        assert!(
            (wrapped - 0.75).abs() < 1e-4,
            "looping sample should wrap towards the first key, got {wrapped}"
        );
        let held = sample_in_sequence(&track, &non_looping, 950).expect("sample");
        assert_eq!(held, 1.0, "non-looping sample must hold the last key");
        // Inside the range both agree.
        let mid = sample_in_sequence(&track, &looping, 500).expect("sample");
        assert!((mid - 0.5).abs() < 1e-4, "mid sample {mid}");
    }

    #[test]
    fn track_without_in_range_keys_falls_back_to_rest() {
        let track = Track {
            interpolation: Interpolation::Linear,
            global_sequence: None,
            keys: vec![Key {
                time: 5000,
                value: 3.0f32,
                in_tan: 0.0,
                out_tan: 0.0,
            }],
            static_value: None,
        };
        let sequence = Sequence {
            name: "Walk".into(),
            start: 100,
            end: 900,
            move_speed: 0.0,
            flags: 0,
            rarity: 0.0,
            sync_point: 0,
            bounds: wc3_formats::mdx::Extent::default(),
        };
        assert!(sample_in_sequence(&track, &sequence, 200).is_none());
    }

    #[test]
    fn animator_loops_sequence() {
        let node = node_with_tracks();
        let sequence = wc3_formats::mdx::Sequence {
            name: "Stand".into(),
            start: 0,
            end: 1000,
            move_speed: 0.0,
            flags: 0,
            rarity: 0.0,
            sync_point: 0,
            bounds: Default::default(),
        };
        let mut animator = Animator {
            nodes: vec![node],
            pivots: vec![Vec3::ZERO],
            global_sequences: Vec::new(),
            sequence: None,
            time_ms: 0.0,
            speed: 1.0,
        };
        animator.play(&sequence);
        animator.update(1.5);
        assert!(animator.frame_time() <= 1000);
        assert_eq!(animator.poses().len(), 1);
    }

    #[test]
    fn world_matrices_resolve_parent_stored_after_child() {
        let mut child = node_with_tracks();
        child.name = "child".into();
        child.object_id = 0;
        child.parent_id = 1;
        child.translation = Track {
            interpolation: Interpolation::None,
            global_sequence: None,
            keys: Vec::new(),
            static_value: Some(Vec3::ZERO),
        };
        let mut parent = node_with_tracks();
        parent.name = "parent".into();
        parent.object_id = 1;
        parent.parent_id = -1;
        parent.translation = Track {
            interpolation: Interpolation::None,
            global_sequence: None,
            keys: Vec::new(),
            static_value: Some(Vec3::new(10.0, 0.0, 0.0)),
        };
        let animator = Animator {
            // Deliberately child-first: the MDX chunk order used to produce an
            // identity parent and detach every bone parented to a helper.
            nodes: vec![child, parent],
            pivots: vec![Vec3::ZERO, Vec3::ZERO],
            global_sequences: Vec::new(),
            sequence: None,
            time_ms: 0.0,
            speed: 1.0,
        };
        let world = animator.world_matrices();
        assert!((world[0].w_axis.x - 10.0).abs() < 1e-3);
    }

    #[test]
    fn particles_age_out() {
        let mut system = ParticleSystem::new(10.0, Vec3::new(0.0, -9.8, 0.0));
        for _ in 0..120 {
            system.update(1.0 / 30.0, Vec3::ZERO);
        }
        assert!(system.particles.len() < 20);
    }
}
