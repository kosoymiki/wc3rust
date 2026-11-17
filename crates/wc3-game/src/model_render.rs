//! Per-definition model loading (PARITY_MATRIX §7 #1/#2).
//!
//! The renderer used to load four hard-coded MDX files (`assets::MODEL_PATHS`)
//! and mapped every roster definition onto those four meshes. This module owns
//! the real cache: one entry per definition model path (`UnitDef.model`), built
//! from the operator's archive with the canonical asset database, posed once at
//! `Stand` through the existing [`wc3_anim::Animator`], and evicted LRU when the
//! byte budget is exceeded.
//!
//! It is deliberately independent of `Assets`/`Game` so the cache can be used
//! by the headless gallery and by the runtime wiring without touching the sim.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use glam::{Mat4, Vec2, Vec3};
use wc3_anim::{AbilitySequence, AnimatorRegistry, ProjectileSequence};
use wc3_assetdb::AssetDb;
use wc3_formats::blp;
use wc3_formats::mdx::{Geoset, MdxModel};

use crate::assets::{MeshData, MeshLayer, TextureImage};

/// Screen-space height of one model at a world position (visual regression
/// helper used by the terrain tests to prove units are not sub-pixel sprites).
pub fn project_model_height(
    assets: &crate::assets::Assets,
    view_proj: Mat4,
    kind: wc3_sim::UnitKind,
    base: glam::Vec2,
) -> Option<f32> {
    let ground = assets.height_at(base.x, base.y);
    let height = assets.model(kind).height;
    let bottom = view_proj * glam::Vec4::new(base.x, ground, base.y, 1.0);
    let top = view_proj * glam::Vec4::new(base.x, ground + height, base.y, 1.0);
    if bottom.w <= 0.001 || top.w <= 0.001 {
        return None;
    }
    let ndc_y = |v: glam::Vec4| v.y / v.w;
    Some(((ndc_y(top) - ndc_y(bottom)).abs() * 0.5) * 1080.0)
}

/// Model-space vertex budget accounting: positions, normals, uvs, indices and
/// weighted bone influences, plus textures and the Stand palette.
fn mesh_bytes(meshes: &[MeshData]) -> usize {
    let mut bytes = 0usize;
    for mesh in meshes {
        let n = mesh.positions.len();
        bytes += n * (12 + 12 + 8);
        bytes += mesh.indices.len() * 4;
        bytes += mesh.bones.iter().map(|list| list.len() * 8).sum::<usize>();
        bytes += mesh.layers.len() * 32;
        bytes += mesh.texture_paths.iter().map(|path| path.len()).sum::<usize>();
    }
    bytes
}

fn texture_bytes(textures: &[TextureImage]) -> usize {
    textures
        .iter()
        .map(|texture| (texture.width as usize) * (texture.height as usize) * 4)
        .sum()
}

/// One resident model: geometry, textures and the Stand-pose node palette.
#[derive(Clone)]
pub struct ModelEntry {
    pub model: String,
    pub meshes: Vec<MeshData>,
    /// Model-local texture list; layer `texture` indices point into this list.
    pub textures: Vec<TextureImage>,
    /// `Animator::world_matrices()` at the Stand sequence start.
    pub nodes: Vec<Mat4>,
    pub height: f32,
    pub bytes: usize,
    pub stand_sequence: Option<String>,
    /// Sequence the cached geometry is posed with (`Stand` for units, `Flight`
    /// for missiles loaded through [`ModelRegistry::load_projectile`]).
    pub pose_sequence: Option<String>,
    /// `true` when the requested pose only matched a fallback alias.
    pub pose_fallback: bool,
    /// Canonical ability sequences resolved by `wc3-anim` (no per-kind tables).
    pub cast_sequence: Option<String>,
    /// Canonical projectile sequences resolved by `wc3-anim`.
    pub flight_sequence: Option<String>,
    pub birth_sequence: Option<String>,
    pub impact_sequence: Option<String>,
    /// Model-space normal of flat geometry (thin extent < 2% of the largest).
    /// The renderer billboards these so edge-on views keep coverage.
    pub billboard: Option<Vec3>,
}

/// Cache statistics for the parity gates (`qa.models.race`, `perf.memory`).
#[derive(Clone, Debug, Default)]
pub struct RegistryStats {
    pub hits: u64,
    pub loads: u64,
    pub misses: u64,
    pub evictions: u64,
    /// Parsed models with no geosets (particle/emitter-only, e.g. PlagueCloud).
    pub meshless: u64,
    pub failures: u64,
    /// Largest resident byte count observed.
    pub peak_bytes: usize,
}

/// LRU cache of definition models keyed by the real MDX path.
pub struct ModelRegistry {
    db: AssetDb,
    budget: usize,
    used: usize,
    entries: HashMap<String, ModelEntry>,
    order: VecDeque<String>,
    stats: RegistryStats,
    /// Canonical parsed-model/animator cache from `wc3-anim`. The byte LRU above
    /// governs resident GPU geometry; this registry is the single source of
    /// parsed MDX + per-definition animators (`Animator::world_matrices`).
    animators: AnimatorRegistry,
}

impl ModelRegistry {
    pub fn open_dir(root: &Path, budget_bytes: usize) -> Self {
        Self {
            db: AssetDb::open_dir(root),
            budget: budget_bytes.max(8 * 1024 * 1024),
            used: 0,
            entries: HashMap::new(),
            order: VecDeque::new(),
            stats: RegistryStats::default(),
            // Capacity covers every unit/building MDX in the four-race install
            // plus headroom; the byte LRU above decides what stays resident.
            animators: AnimatorRegistry::new(542),
        }
    }

    pub fn budget(&self) -> usize {
        self.budget
    }

    pub fn used_bytes(&self) -> usize {
        self.used
    }

    pub fn resident(&self) -> usize {
        self.entries.len()
    }

    pub fn stats(&self) -> &RegistryStats {
        &self.stats
    }

    /// Parsed models held by the canonical `wc3-anim` registry.
    pub fn animator_resident(&self) -> usize {
        self.animators.len()
    }

    pub fn animator_capacity(&self) -> usize {
        self.animators.capacity()
    }

    pub fn animator_evictions(&self) -> usize {
        self.animators.evictions()
    }

    /// Load (or hit) the model for `UnitDef.model`. Evicts least-recently-used
    /// entries until the new model fits the budget.
    pub fn load(&mut self, model: &str) -> Option<&ModelEntry> {
        match self.load_detailed(model) {
            LoadOutcome::Loaded(entry) => Some(entry),
            _ => None,
        }
    }

    /// Same as [`load`](Self::load) but distinguishes a parsed meshless model
    /// (particle/emitter-only) from a real failure.
    pub fn load_detailed(&mut self, model: &str) -> LoadOutcome<'_> {
        self.load_posed(model, Pose::Stand)
    }

    /// Load a missile model posed through the canonical projectile animator
    /// ([`ProjectileSequence::Flight`]) instead of a generic Stand pose.
    pub fn load_projectile(&mut self, model: &str) -> LoadOutcome<'_> {
        self.load_posed(model, Pose::Projectile(ProjectileSequence::Flight))
    }

    fn load_posed(&mut self, model: &str, pose: Pose) -> LoadOutcome<'_> {
        let key = cache_key(model, pose);
        if self.entries.contains_key(&key) {
            self.stats.hits += 1;
            self.touch(&key);
            return match self.entries.get(&key) {
                Some(entry) => LoadOutcome::Loaded(entry),
                None => LoadOutcome::Failed,
            };
        }
        self.stats.misses += 1;
        let entry = match self.build_posed(model, pose) {
            BuildOutcome::Model(entry) => entry,
            BuildOutcome::Meshless => {
                self.stats.meshless += 1;
                return LoadOutcome::Meshless;
            }
            BuildOutcome::Failed => {
                self.stats.failures += 1;
                return LoadOutcome::Failed;
            }
        };
        let bytes = entry.bytes;
        if bytes > self.budget {
            // A single model larger than the whole budget is still returned
            // once (the caller renders it), but it is not cached.
            self.stats.loads += 1;
            self.stats.peak_bytes = self.stats.peak_bytes.max(bytes);
            self.entries.clear();
            self.order.clear();
            self.used = 0;
            self.entries.insert(key.clone(), entry);
            self.order.push_back(key.clone());
            self.used = bytes;
            return match self.entries.get(&key) {
                Some(entry) => LoadOutcome::Loaded(entry),
                None => LoadOutcome::Failed,
            };
        }
        while self.used + bytes > self.budget {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(evicted) = self.entries.remove(&oldest) {
                self.used = self.used.saturating_sub(evicted.bytes);
                self.stats.evictions += 1;
            }
        }
        self.used += bytes;
        self.stats.loads += 1;
        self.stats.peak_bytes = self.stats.peak_bytes.max(self.used);
        self.entries.insert(key.clone(), entry);
        self.order.push_back(key.clone());
        match self.entries.get(&key) {
            Some(entry) => LoadOutcome::Loaded(entry),
            None => LoadOutcome::Failed,
        }
    }

    /// True when `model` is resident.
    pub fn contains(&self, model: &str) -> bool {
        self.entries.contains_key(&normalize(model))
    }

    /// Resolve an archive path with the registry's asset database (case-insensitive,
    /// listfile-aware). Used by missile-art resolution: WC3 weapon profiles carry
    /// `.mdl` source paths while the install ships `.mdx`.
    pub fn resolve_path(&self, name: &str) -> Option<String> {
        self.db.resolve_path(name)
    }

    /// Canonical ability animator (`Cast`, `Channel`, `Attack`, …) from
    /// `wc3-anim`; spell playback calls this instead of per-kind tables.
    pub fn ability_animator(
        &mut self,
        model: &str,
        kind: AbilitySequence,
    ) -> Option<(wc3_anim::SequenceMatch, wc3_anim::Animator)> {
        self.animators.ability_animator(model, kind)
    }

    /// Canonical projectile animator (`Flight`, `Birth`, `Impact`).
    pub fn projectile_animator(
        &mut self,
        model: &str,
        kind: ProjectileSequence,
    ) -> Option<(wc3_anim::SequenceMatch, wc3_anim::Animator)> {
        self.animators.projectile_animator(model, kind)
    }

    fn touch(&mut self, key: &str) {
        if let Some(position) = self.order.iter().position(|candidate| candidate == key) {
            self.order.remove(position);
        }
        self.order.push_back(key.to_string());
    }

    fn build(&mut self, model: &str) -> BuildOutcome {
        self.build_posed(model, Pose::Stand)
    }

    /// Build geometry with an explicit pose source. `Stand` keeps the gallery
    /// behaviour, `Projectile(Flight)` is what missiles use, and
    /// `Ability(Cast)` is the canonical spell-playback patch.
    fn build_posed(&mut self, model: &str, pose: Pose) -> BuildOutcome {
        let Ok(model_asset) = self.db.model_shared(model) else {
            return BuildOutcome::Failed;
        };
        let mdx: MdxModel = model_asset.model.clone();
        // Hand the parsed model to the canonical anim registry first: one parsed
        // MDX per key, and the registry owns every per-instance animator.
        self.animators.insert(model, mdx.clone());
        let (textures, texture_map) = self.texture_layers(&mdx);
        let mut meshes = crate::assets::build_meshes(&mdx, &texture_map);
        if meshes.is_empty() {
            // Particle/emitter-only model: parsed, nothing to upload.
            return BuildOutcome::Meshless;
        }
        // Flat geometry has zero coverage edge-on; flag it for the renderer.
        let billboard = billboard_normal(&meshes);
        let stand = self
            .animators
            .sequence(model, "stand")
            .or_else(|| mdx.sequences.first().cloned());
        // Canonical sequence resolution from `wc3-anim` — no per-kind tables.
        let cast_sequence = self
            .animators
            .ability_sequence(model, AbilitySequence::Cast)
            .map(|matched| matched.sequence.name.clone());
        let flight_sequence = self
            .animators
            .projectile_sequence(model, ProjectileSequence::Flight)
            .map(|matched| matched.sequence.name.clone());
        let birth_sequence = self
            .animators
            .projectile_sequence(model, ProjectileSequence::Birth)
            .map(|matched| matched.sequence.name.clone());
        let impact_sequence = self
            .animators
            .projectile_sequence(model, ProjectileSequence::Impact)
            .map(|matched| matched.sequence.name.clone());
        let (mut animator, pose_sequence, pose_fallback) = match pose {
            Pose::Stand => {
                let animator = stand
                    .as_ref()
                    .and_then(|sequence| self.animators.animator(model, sequence))
                    .unwrap_or_else(|| wc3_anim::Animator::from_model(&mdx));
                (animator, stand.as_ref().map(|sequence| sequence.name.clone()), false)
            }
            Pose::Ability(kind) => match self.animators.ability_animator(model, kind) {
                Some((matched, animator)) => (
                    animator,
                    Some(matched.sequence.name.clone()),
                    matched.fallback,
                ),
                None => (
                    wc3_anim::Animator::from_model(&mdx),
                    stand.as_ref().map(|sequence| sequence.name.clone()),
                    true,
                ),
            },
            Pose::Projectile(kind) => match self.animators.projectile_animator(model, kind) {
                Some((matched, animator)) => (
                    animator,
                    Some(matched.sequence.name.clone()),
                    matched.fallback,
                ),
                None => (
                    wc3_anim::Animator::from_model(&mdx),
                    stand.as_ref().map(|sequence| sequence.name.clone()),
                    true,
                ),
            },
        };
        animator.update(0.0);
        let nodes = animator.world_matrices();
        pose_meshes(&mut meshes, &nodes);
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for mesh in &meshes {
            for position in &mesh.positions {
                min_y = min_y.min(position.y);
                max_y = max_y.max(position.y);
            }
        }
        let height = if max_y.is_finite() && min_y.is_finite() {
            (max_y - min_y).max(1.0)
        } else {
            1.0
        };
        let bytes = mesh_bytes(&meshes) + texture_bytes(&textures) + nodes.len() * 64;
        BuildOutcome::Model(ModelEntry {
            model: model.to_string(),
            meshes,
            textures,
            nodes,
            height,
            bytes,
            stand_sequence: stand.as_ref().map(|sequence| sequence.name.clone()),
            pose_sequence,
            pose_fallback,
            cast_sequence,
            flight_sequence,
            birth_sequence,
            impact_sequence,
            billboard,
        })
    }

    /// Texture layers for an MDX: replaceable 1/2 are team colour/glow,
    /// everything else is the authored BLP path; missing files use white.
    fn texture_layers(&mut self, mdx: &MdxModel) -> (Vec<TextureImage>, Vec<usize>) {
        let mut textures: Vec<TextureImage> = vec![TextureImage::white()];
        let mut texture_cache: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        texture_cache.insert("__white".to_string(), 0);
        let mut texture_map: Vec<usize> = Vec::with_capacity(mdx.textures.len());
        for texture in &mdx.textures {
            let path = match texture.replaceable_id {
                1 => "ReplaceableTextures\\TeamColor\\TeamColor00.blp".to_string(),
                2 => "ReplaceableTextures\\TeamGlow\\TeamGlow00.blp".to_string(),
                _ => texture.path.trim_end_matches('\0').replace('/', "\\"),
            };
            let key = path.to_ascii_lowercase();
            let index = if path.is_empty() {
                0
            } else if let Some(&found) = texture_cache.get(&key) {
                found
            } else {
                match self.db.texture(&path) {
                    Ok(image) => {
                        let (width, height, rgba) =
                            resize_rgba(&image.rgba, image.width, image.height, 128);
                        textures.push(TextureImage {
                            width,
                            height,
                            rgba,
                        });
                        let index = textures.len() - 1;
                        texture_cache.insert(key, index);
                        index
                    }
                    Err(_) => 0,
                }
            };
            texture_map.push(index);
        }
        (textures, texture_map)
    }

    /// Bind geometry (bones intact) for per-frame skinning during playback.
    fn geometry_for(&mut self, model: &str) -> Option<(Vec<TextureImage>, Vec<MeshData>)> {
        let model_asset = self.db.model_shared(model).ok()?;
        let mdx: MdxModel = model_asset.model.clone();
        let (textures, texture_map) = self.texture_layers(&mdx);
        let meshes = crate::assets::build_meshes(&mdx, &texture_map);
        (!meshes.is_empty()).then_some((textures, meshes))
    }
}

/// Which sequence the cached geometry is posed with.
#[derive(Clone, Copy, Debug)]
enum Pose {
    Stand,
    Ability(AbilitySequence),
    Projectile(ProjectileSequence),
}

impl Pose {
    fn tag(self) -> &'static str {
        match self {
            Self::Stand => "stand",
            Self::Ability(AbilitySequence::Cast) => "cast",
            Self::Ability(AbilitySequence::Channel) => "channel",
            Self::Ability(AbilitySequence::Attack) => "attack",
            Self::Ability(AbilitySequence::Victory) => "victory",
            Self::Ability(AbilitySequence::Death) => "death",
            Self::Ability(AbilitySequence::Dissipate) => "dissipate",
            Self::Projectile(ProjectileSequence::Flight) => "flight",
            Self::Projectile(ProjectileSequence::Birth) => "birth",
            Self::Projectile(ProjectileSequence::Impact) => "impact",
        }
    }
}

/// Cache key: the Stand pose keeps the plain model path (existing behaviour),
/// every other pose gets its own resident slot.
fn cache_key(model: &str, pose: Pose) -> String {
    match pose {
        Pose::Stand => normalize(model),
        _ => format!("{}#{}", normalize(model), pose.tag()),
    }
}

/// Model-space normal of flat geometry: `Some(axis)` when the thinnest extent
/// is below 2% of the largest one (planes, cards, billboard missiles).
fn billboard_normal(meshes: &[MeshData]) -> Option<Vec3> {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for mesh in meshes {
        for position in &mesh.positions {
            min = min.min(*position);
            max = max.max(*position);
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return None;
    }
    let extent = max - min;
    let largest = extent.max_element();
    if largest <= 1e-6 {
        return None;
    }
    let thin = extent.min_element();
    if thin / largest >= 0.02 {
        return None;
    }
    Some(if extent.x <= extent.y && extent.x <= extent.z {
        Vec3::X
    } else if extent.y <= extent.z {
        Vec3::Y
    } else {
        Vec3::Z
    })
}

/// CPU-skin bind meshes with a node palette (same math as the cached build).
///
/// Meshes arrive in the compact runtime format, so the pose works on an
/// expanded copy and writes the result back through [`MeshData::pose`].
fn pose_meshes(meshes: &mut [MeshData], nodes: &[Mat4]) {
    for mesh in meshes {
        if mesh.vertex_count() == 0 {
            continue;
        }
        let expanded = mesh.expanded();
        if expanded.bones.is_empty() || expanded.bones.iter().all(|list| list.is_empty()) {
            continue;
        }
        let mut posed = Vec::with_capacity(expanded.positions.len());
        let mut posed_normals = Vec::with_capacity(expanded.positions.len());
        for (index, position) in expanded.positions.iter().enumerate() {
            let mut local = Vec3::ZERO;
            let mut normal = Vec3::ZERO;
            let mut total = 0.0f32;
            if let Some(list) = expanded.bones.get(index) {
                for bone in list {
                    if (bone.bone as usize) < nodes.len() && bone.weight > 0.0 {
                        let node = nodes[bone.bone as usize];
                        local += node.transform_point3(*position) * bone.weight;
                        normal += node
                            .transform_vector3(
                                expanded.normals.get(index).copied().unwrap_or(Vec3::Y),
                            )
                            * bone.weight;
                        total += bone.weight;
                    }
                }
            }
            if total > 1e-5 {
                posed.push(local / total);
                posed_normals.push(normal.normalize_or_zero());
            } else {
                posed.push(*position);
                posed_normals.push(expanded.normals.get(index).copied().unwrap_or(Vec3::Y));
            }
        }
        let count = posed.len();
        mesh.pose(posed, posed_normals, vec![Vec::new(); count]);
    }
}

// ---------------------------------------------------------------- projectiles

/// One attack's missile art from the real weapon profile (`Missileart`).
#[derive(Clone, Debug, PartialEq)]
pub struct MissileArt {
    /// Unit that owns the weapon (`hfoo`, `earc`, …).
    pub unit: String,
    /// Raw profile value; may hold comma-separated impact/missile lists.
    pub art: String,
    /// Weapon projectile speed in world units per second (profile default 900).
    pub speed: f32,
}

/// Enumerate every missile art declared by the real game database.
///
/// Reads `UnitWeapons.slk` + the profile layers through `wc3-data`; no values
/// are invented, and units without a `Missileart` component are skipped.
pub fn missile_arts(db: &wc3_data::GameDatabase) -> Vec<MissileArt> {
    let mut out = Vec::new();
    for unit in db.units() {
        let id = unit.id_str();
        for weapon in unit.weapons() {
            let Some(art) = weapon.projectile() else {
                continue;
            };
            let art = art.trim().to_string();
            if art.is_empty() {
                continue;
            }
            let speed = weapon.projectile_speed().unwrap_or(900) as f32;
            out.push(MissileArt {
                unit: id.clone(),
                art,
                speed,
            });
        }
    }
    out.sort_by(|a, b| a.art.cmp(&b.art).then_with(|| a.unit.cmp(&b.unit)));
    out.dedup();
    out
}

/// Resolve a weapon missile art to an MDX present in the archive.
///
/// WC3 weapon profiles store `.mdl` source paths (and occasionally
/// comma-separated impact/missile lists) while the install ships `.mdx`; the
/// first candidate that exists in the archive wins. Returns `None` when the
/// install genuinely lacks every candidate (TFT-only art in a RoC tree).
pub fn resolve_missile_model(registry: &ModelRegistry, art: &str) -> Option<String> {
    for part in art.split(',') {
        let mut path = part.trim().to_string();
        if path.is_empty() {
            continue;
        }
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".mdl") {
            path.truncate(path.len() - 4);
            path.push_str(".mdx");
        } else if !lower.ends_with(".mdx") {
            path.push_str(".mdx");
        }
        if registry.resolve_path(&path).is_some() {
            return Some(path);
        }
    }
    None
}

/// A missile in flight. Visual-only: the renderer owns motion, spin and fade;
/// gameplay damage stays in the simulation.
#[derive(Clone, Debug)]
pub struct ProjectileFx {
    /// Registry key / asset path of the missile model.
    pub model: String,
    /// Index into `Assets::models` for this projectile model.
    pub model_index: usize,
    pub origin: Vec3,
    pub target: Vec3,
    /// World units per second (weapon profile `Missilespeed`).
    pub speed: f32,
    /// Peak height of the parabolic arc above the straight line.
    pub arc: f32,
    /// Flight progress in `0..=1`.
    pub t: f32,
    /// Per-layer tint; alpha is replaced by the fade curve in [`draw`](Self::draw).
    pub tint: [f32; 4],
    /// Canonical projectile animation driving this missile.
    pub sequence: ProjectileSequence,
}

impl ProjectileFx {
    pub fn new(
        model: impl Into<String>,
        model_index: usize,
        origin: Vec3,
        target: Vec3,
        speed: f32,
    ) -> Self {
        Self {
            model: model.into(),
            model_index,
            origin,
            target,
            speed: speed.max(1.0),
            arc: 0.0,
            t: 0.0,
            tint: [1.0, 1.0, 1.0, 1.0],
            sequence: ProjectileSequence::Flight,
        }
    }

    pub fn with_sequence(mut self, kind: ProjectileSequence) -> Self {
        self.sequence = kind;
        self
    }

    pub fn with_arc(mut self, arc: f32) -> Self {
        self.arc = arc;
        self
    }

    pub fn distance(&self) -> f32 {
        (self.target - self.origin).length().max(1.0)
    }

    /// Flight time in seconds at the weapon's projectile speed.
    pub fn duration(&self) -> f32 {
        (self.distance() / self.speed).max(0.05)
    }

    /// Advance by `dt` seconds. Returns `false` once the missile lands.
    pub fn advance(&mut self, dt: f32) -> bool {
        self.t = (self.t + dt / self.duration()).min(1.0);
        self.t < 1.0
    }

    /// World position at the current `t`, including the parabolic arc.
    pub fn position(&self) -> Vec3 {
        let point = self.origin.lerp(self.target, self.t);
        let lift = self.arc * 4.0 * self.t * (1.0 - self.t);
        Vec3::new(point.x, point.y + lift, point.z)
    }

    /// Alpha curve: quick fade-in, long body, short fade-out on impact.
    pub fn alpha(&self) -> f32 {
        let fade_in = (self.t / 0.08).min(1.0);
        let fade_out = ((1.0 - self.t) / 0.12).min(1.0);
        (fade_in * fade_out).clamp(0.05, 1.0)
    }

    /// Build the renderer draw for this projectile through the canonical
    /// `UnitDraw` path (same skinning/material/team-tint pipeline as units).
    ///
    /// Flat missile models (see [`ModelEntry::billboard`]) are oriented to face
    /// the camera: an edge-on single quad has zero coverage, so
    /// `ZigguratMissile`-class art would otherwise disappear in flight.
    pub fn draw(&self, entry: &ModelEntry, eye: Vec3) -> crate::game::UnitDraw {
        self.draw_oriented(entry, eye, entry.billboard.is_some())
    }

    /// Draw with an explicit billboard override (used by the edge-on gate).
    pub fn draw_oriented(
        &self,
        entry: &ModelEntry,
        eye: Vec3,
        billboard: bool,
    ) -> crate::game::UnitDraw {
        let position = self.position();
        let direction = (self.target - self.origin).normalize_or_zero();
        let yaw = direction.x.atan2(direction.z);
        let spin = self.t * std::f32::consts::TAU * 2.0;
        let rotation = if billboard {
            match entry.billboard {
                Some(axis) => {
                    let forward = (eye - position).normalize_or_zero();
                    if forward.length_squared() > 0.0 {
                        // The renderer swizzles Z-up geometry to Y-up before the
                        // draw matrix, so align the swizzled normal with the eye.
                        let z_up = entry.meshes.first().map(|mesh| mesh.z_up).unwrap_or(true);
                        let axis_world = if z_up {
                            Vec3::new(axis.x, axis.z, -axis.y)
                        } else {
                            axis
                        };
                        glam::Quat::from_axis_angle(forward, spin)
                            * glam::Quat::from_rotation_arc(axis_world, forward)
                    } else {
                        glam::Quat::from_rotation_y(yaw + spin)
                    }
                }
                None => glam::Quat::from_rotation_y(yaw + spin),
            }
        } else {
            glam::Quat::from_rotation_y(yaw + spin)
        };
        let world = Mat4::from_translation(position) * Mat4::from_quat(rotation);
        let alpha = self.alpha();
        let mut tint = self.tint;
        tint[3] = alpha;
        crate::game::UnitDraw {
            model: self.model_index,
            world,
            tint,
            nodes: Vec::new(),
            mesh_alpha: vec![alpha; entry.meshes.len()],
            layer_alpha: entry
                .meshes
                .iter()
                .map(|mesh| vec![alpha; mesh.layers.len()])
                .collect(),
        }
    }

}

/// Canonical spell playback for one caster model.
///
/// `start` resolves the authored `Cast`/`Channel`/… sequence through
/// [`AnimatorRegistry::ability_animator`] and keeps the bind geometry, so the
/// caller skins a real per-frame pose instead of reading a per-kind table.
pub struct SpellPlayback {
    pub model: String,
    pub kind: AbilitySequence,
    /// Authored sequence name picked by the canonical resolver.
    pub sequence: String,
    /// `true` when only a fallback alias matched (model lacks the animation).
    pub fallback: bool,
    animator: wc3_anim::Animator,
    meshes: Vec<MeshData>,
    textures: Vec<TextureImage>,
    elapsed: f32,
}

impl SpellPlayback {
    /// Start canonical playback for `model` (e.g. `AbilitySequence::Cast`).
    pub fn start(
        registry: &mut ModelRegistry,
        model: &str,
        kind: AbilitySequence,
    ) -> Option<Self> {
        // Ensure the parsed MDX is in the animator registry before resolving.
        let _ = registry.load(model);
        let (matched, animator) = registry.ability_animator(model, kind)?;
        let (textures, meshes) = registry.geometry_for(model)?;
        Some(Self {
            model: model.to_string(),
            kind,
            sequence: matched.sequence.name.clone(),
            fallback: matched.fallback,
            animator,
            meshes,
            textures,
            elapsed: 0.0,
        })
    }

    /// Advance playback by `dt` seconds and step the animator.
    pub fn advance(&mut self, dt: f32) {
        self.elapsed += dt;
        self.animator.update(dt);
    }

    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }

    /// Current node palette (`Animator::world_matrices`).
    pub fn nodes(&self) -> Vec<Mat4> {
        self.animator.world_matrices()
    }

    /// Bind geometry skinned with the current palette.
    pub fn posed_meshes(&self) -> Vec<MeshData> {
        let mut meshes = self.meshes.clone();
        pose_meshes(&mut meshes, &self.nodes());
        meshes
    }

    pub fn textures(&self) -> &[TextureImage] {
        &self.textures
    }
}

/// Result of a registry lookup.
pub enum LoadOutcome<'a> {
    Loaded(&'a ModelEntry),
    /// The MDX parsed but has no geosets (particle/emitter-only).
    Meshless,
    /// Read/parse/build failure.
    Failed,
}

enum BuildOutcome {
    Model(ModelEntry),
    Meshless,
    Failed,
}

fn normalize(model: &str) -> String {
    model.replace('/', "\\")
}

fn resize_rgba(src: &[u8], sw: u32, sh: u32, size: u32) -> (u32, u32, Vec<u8>) {
    if sw == 0 || sh == 0 || src.len() < (sw * sh * 4) as usize {
        return (2, 2, vec![255; 16]);
    }
    if sw == size && sh == size {
        return (sw, sh, src.to_vec());
    }
    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        let sy = (y as u64 * sh as u64 / size as u64).min(sh as u64 - 1) as u32;
        for x in 0..size {
            let sx = (x as u64 * sw as u64 / size as u64).min(sw as u64 - 1) as u32;
            let si = ((sy * sw + sx) * 4) as usize;
            let di = ((y * size + x) * 4) as usize;
            out[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    (size, size, out)
}

/// Every four-race unit/building MDX in `root` (portraits excluded), sorted.
pub fn four_race_models(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for group in ["Units", "Buildings"] {
        for race in ["Human", "Orc", "Undead", "NightElf"] {
            collect_mdx(&root.join(group).join(race), &mut out);
        }
    }
    out.sort();
    out.dedup();
    out
}

fn collect_mdx(dir: &Path, out: &mut Vec<String>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => collect_mdx(&path, out),
            Ok(kind) if kind.is_file() => {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                // The four-race MDX inventory includes `*_Portrait.mdx`
                // (216 = 170 Units + 46 Buildings), exactly the matrix count.
                if name.ends_with(".mdx") {
                    out.push(relative_path(&path));
                }
            }
            _ => {}
        }
    }
}

/// `Units\\Human\\Footman\\Footman.mdx` from an absolute extracted-tree path.
fn relative_path(path: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut current = Some(path.to_path_buf());
    while let Some(node) = current {
        if let Some(name) = node.file_name() {
            parts.push(name.to_string_lossy().to_string());
        }
        current = node.parent().map(Path::to_path_buf);
    }
    let mut reversed: Vec<String> = parts.into_iter().rev().collect();
    if let Some(index) = reversed
        .iter()
        .position(|part| part == "Units" || part == "Buildings")
    {
        reversed = reversed.split_off(index);
    }
    reversed.join("\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full")
    }

    fn map_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Maps/Campaign/Human04.w3m")
    }

    /// PARITY_MATRIX §7 #1 gate: every four-race unit/building MDX parses,
    /// builds meshes and uploads to the model cache; the whole set fits the
    /// 1536 MB phone budget.
    #[test]
    #[ignore = "heavy: loads every four-race unit/building model; needs War3.mpq; run_layer_gates.sh --ignored"]
    fn models_per_def() {
        let root = world_root();
        if !root.exists() {
            crate::dev_log!("SKIP models_per_def: {} missing", root.display());
            return;
        }
        let models = four_race_models(&root);
        if models.len() < 216 {
            crate::dev_log!("SKIP models_per_def: only {} models found", models.len());
            return;
        }
        let budget = 1536 * 1024 * 1024;
        let mut registry = ModelRegistry::open_dir(&root, budget);
        let mut loaded = 0usize;
        let mut failures = Vec::new();
        let mut meshless = Vec::new();
        let mut per_race: std::collections::BTreeMap<&str, usize> = Default::default();
        for model in &models {
            match registry.load_detailed(model) {
                LoadOutcome::Loaded(_) => {
                    loaded += 1;
                    for race in ["Human", "Orc", "Undead", "NightElf"] {
                        if model.contains(&format!("\\{race}\\")) {
                            *per_race.entry(race).or_insert(0) += 1;
                        }
                    }
                }
                LoadOutcome::Meshless => meshless.push(model.clone()),
                LoadOutcome::Failed => failures.push(model.clone()),
            }
        }
        let stats = registry.stats().clone();
        let peak_mb = stats.peak_bytes as f64 / 1048576.0;
        crate::dev_log!(
            "MODELS_PER_DEF required={} loaded={} meshless={} failures={} resident={} memory_mb={:.1} peak_mb={:.1} evictions={} races={per_race:?}",
            models.len(),
            loaded,
            stats.meshless,
            failures.len(),
            registry.resident(),
            registry.used_bytes() as f64 / 1048576.0,
            peak_mb,
            stats.evictions
        );
        crate::dev_log!(
            "PERF_MEMORY human04 resident_mb={:.1} peak_mb={peak_mb:.1} budget_mb={} resident_models={} anim_resident={} anim_capacity={} anim_evictions={} pass={}",
            registry.used_bytes() as f64 / 1048576.0,
            budget / 1048576,
            registry.resident(),
            registry.animator_resident(),
            registry.animator_capacity(),
            registry.animator_evictions(),
            peak_mb <= budget as f64 / 1048576.0
        );
        assert!(models.len() >= 216, "four-race MDX inventory");
        assert!(failures.is_empty(), "unloadable models: {failures:?}");
        assert_eq!(meshless.len(), 2, "the two PlagueCloud particle models");
        assert!(
            loaded + meshless.len() == models.len(),
            "parse+upload covers every four-race MDX: loaded={loaded} meshless={}",
            meshless.len()
        );
        assert!(
            stats.peak_bytes <= budget,
            "model cache exceeds the 1536 MB budget"
        );
    }

    /// PARITY_MATRIX §7 #2 gate: the LRU budget evicts instead of growing.
    #[test]
    #[ignore = "heavy: LRU eviction over every four-race model; needs War3.mpq; run_layer_gates.sh --ignored"]
    fn models_per_def_lru_budget() {
        let root = world_root();
        if !root.exists() {
            crate::dev_log!("SKIP models_per_def_lru_budget: tree missing");
            return;
        }
        let models = four_race_models(&root);
        if models.len() < 216 {
            return;
        }
        let budget = 32 * 1024 * 1024;
        let mut registry = ModelRegistry::open_dir(&root, budget);
        for model in &models {
            registry.load(model);
        }
        let stats = registry.stats().clone();
        crate::dev_log!(
            "MODELS_PER_DEF_LRU budget_mb={} loads={} misses={} resident={} evictions={} used_mb={:.1} peak_mb={:.1}",
            budget / 1048576,
            stats.loads,
            stats.misses,
            registry.resident(),
            stats.evictions,
            registry.used_bytes() as f64 / 1048576.0,
            stats.peak_bytes as f64 / 1048576.0
        );
        assert!(stats.evictions > 0, "small budget must evict");
        assert!(
            registry.used_bytes() <= budget,
            "resident bytes stay within the budget"
        );
        assert!(
            stats.loads + stats.meshless >= 216,
            "all models still parsed once: loads={} meshless={}",
            stats.loads,
            stats.meshless
        );
    }

    /// Map roster: every unit placed by Human04 resolves to a definition model
    /// that the cache can load.
    #[test]
    fn map_roster_models() {
        let root = world_root();
        let map = map_path();
        if !root.exists() || !map.exists() {
            crate::dev_log!("SKIP map_roster_models: fixtures missing");
            return;
        }
        let Ok(bundle) = wc3_re::mapfiles::MapBundle::open(&map) else {
            crate::dev_log!("SKIP map_roster_models: map bundle");
            return;
        };
        let Some(units) = bundle.units.as_ref() else {
            crate::dev_log!("SKIP map_roster_models: no war3mapUnits");
            return;
        };
        let mut registry = ModelRegistry::open_dir(&root, 1536 * 1024 * 1024);
        let mut placed = std::collections::BTreeMap::<String, usize>::new();
        for unit in units {
            let id = String::from_utf8_lossy(&unit.type_id).to_string();
            *placed.entry(id).or_insert(0) += 1;
        }
        let mut resolved = 0usize;
        let mut loaded = 0usize;
        let mut unresolved = Vec::new();
        for (id, count) in &placed {
            let Some(def) = wc3_sim::data::UnitDefId::from_id(id).and_then(|def| def.get()) else {
                unresolved.push(format!("{id} x{count}: no definition"));
                continue;
            };
            if !def.model_present {
                unresolved.push(format!("{id} x{count}: model {} absent", def.model));
                continue;
            }
            resolved += 1;
            if registry.load(def.model).is_some() {
                loaded += 1;
            } else {
                unresolved.push(format!("{id} x{count}: {} failed", def.model));
            }
        }
        crate::dev_log!(
            "MAP_ROSTER_MODELS map=Human04 placed_instances={} distinct={} resolved_defs={} models_loaded={} unresolved={}",
            units.len(),
            placed.len(),
            resolved,
            loaded,
            unresolved.len()
        );
        assert!(!placed.is_empty(), "Human04 has placed units");
        assert!(
            loaded * 100 >= resolved * 90,
            "at least 90% of resolved roster models load: {loaded}/{resolved}"
        );
        if !unresolved.is_empty() {
            crate::dev_log!("MAP_ROSTER_UNRESOLVED {unresolved:?}");
        }
    }

    /// Build one frame from registry-loaded models and return RGBA.
    ///
    /// `grid` places the races in four rows in a single frame; otherwise one
    /// row per call (used for the per-race gallery).
    fn render_registry_frame(
        mpq: &Path,
        root: &Path,
        races: &[&str],
        all: &[String],
        grid: bool,
    ) -> Option<(Vec<u8>, u32, u32, usize)> {
        let mut assets = crate::assets::Assets::load(Some(mpq), None);
        let mut registry = ModelRegistry::open_dir(root, 1536 * 1024 * 1024);
        let mut units = Vec::new();
        let mut rendered = 0usize;
        let mut min = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut max = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        for (row, race) in races.iter().enumerate() {
            let models: Vec<&String> = all
                .iter()
                .filter(|model| {
                    model.starts_with(&format!("Units\\{race}\\"))
                        && !model.to_ascii_lowercase().contains("_portrait")
                })
                .collect();
            for (slot, model) in models.iter().take(8).enumerate() {
                let Some(entry) = registry.load(model).cloned() else {
                    continue;
                };
                let base = assets.textures.len();
                assets.textures.extend(entry.textures.iter().cloned());
                let mut meshes = entry.meshes.clone();
                for mesh in &mut meshes {
                    for layer in &mut mesh.layers {
                        layer.texture += base;
                    }
                }
                for mesh in &meshes {
                    for position in &mesh.positions {
                        min = min.min(*position);
                        max = max.max(*position);
                    }
                }
                let index = assets.models.len();
                assets.models.push(crate::assets::ModelAsset {
                    kind: wc3_sim::UnitKind::Footman,
                    model: None,
                    meshes,
                    sequences: Vec::new(),
                    idle: None,
                    walk: None,
                    attack: None,
                    height: entry.height,
            pivot: 0.0,
                    scale: 1.0,
                    yaw_offset: 0.0,
                    fallback: false,
                });
                let x = (slot as f32 - 3.5) * 320.0;
                let z = if grid {
                    (row as f32 - 1.5) * 340.0
                } else {
                    0.0
                };
                units.push(crate::game::UnitDraw {
                    model: index,
                    world: Mat4::from_translation(Vec3::new(x, 0.0, z)),
                    tint: [0.55, 0.72, 1.0, 1.0],
                    nodes: Vec::new(),
                    mesh_alpha: vec![1.0; entry.meshes.len()],
                    layer_alpha: entry
                        .meshes
                        .iter()
                        .map(|mesh| vec![1.0; mesh.layers.len()])
                        .collect(),
                });
                rendered += 1;
            }
        }
        if units.is_empty() {
            return None;
        }
        let centre = (min + max) * 0.5;
        let size = (max - min).max_element().max(120.0);
        let width = 1400u32;
        let height = 760u32;
        let eye = centre + Vec3::new(size * 0.35, size * 1.15, size * 1.55);
        let view_proj = Mat4::perspective_rh(
            42f32.to_radians(),
            width as f32 / height as f32,
            1.0,
            size * 40.0,
        ) * Mat4::look_at_rh(eye, centre, Vec3::Y);
        let scene = crate::game::Scene {
            view_proj,
            eye,
            viewport: Vec2::new(width as f32, height as f32),
            units,
            rings: Vec::new(),
            health: Vec::new(),
            ui: crate::hud::UiBatch::default(),
            overlay: None,
        };
        let font = crate::hud::Font::load();
        let options = crate::render::RenderOptions {
            skip_terrain: true,
            skip_ui: true,
            skip_doodads: true,
            ..Default::default()
        };
        let rgba = crate::render::render_scene_to_rgba_with(
            &assets,
            &font.atlas,
            None,
            &scene,
            width,
            height,
            options,
        )
        .ok()?;
        Some((rgba, width, height, rendered))
    }

    /// Headless gallery: 8 registry-loaded models per race through the game
    /// renderer, one image per race, plus the combined four-race frame.
    #[test]
    #[ignore = "heavy: renders registry models through the game renderer; needs War3.mpq + GPU; run_layer_gates.sh --ignored"]
    fn models_per_def_gallery() {
        let root = world_root();
        let mpq = [
            "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
            "/data/data/org.wc3rust.p0/files/War3.mpq",
        ]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists());
        let (Some(root), Some(mpq)) = (root.exists().then_some(root), mpq) else {
            crate::dev_log!("SKIP models_per_def_gallery: fixtures missing");
            return;
        };
        let all = four_race_models(&root);
        if all.len() < 216 {
            crate::dev_log!("SKIP models_per_def_gallery: only {} models", all.len());
            return;
        }
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-world/parity");
        let _ = std::fs::create_dir_all(&dir);
        let mut total = 0usize;
        for race in ["Human", "Orc", "Undead", "NightElf"] {
            let Some((rgba, width, height, rendered)) =
                render_registry_frame(&mpq, &root, &[race], &all, false)
            else {
                continue;
            };
            let file = dir.join(format!("models-per-def-{race}.ppm"));
            crate::render::write_ppm(file.to_str().unwrap(), width, height, &rgba);
            crate::dev_log!(
                "MODELS_PER_DEF_RACE race={race} rendered={rendered} file={}",
                file.display()
            );
            total += rendered;
        }
        let Some((rgba, width, height, rendered)) =
            render_registry_frame(&mpq, &root, &["Human", "Orc", "Undead", "NightElf"], &all, true)
        else {
            crate::dev_log!("SKIP models_per_def_gallery: grid render");
            return;
        };
        let file = dir.join("models-per-def.ppm");
        crate::render::write_ppm(file.to_str().unwrap(), width, height, &rgba);
        crate::dev_log!(
            "MODELS_PER_DEF_GALLERY races=4 rendered={rendered} total_per_race={total} file={}",
            file.display()
        );
        assert!(total >= 32, "8 models per race rendered");
    }

    /// Roster CSV: every four-race MDX (definition path -> load status) plus the
    /// Human04 map roster (placed unit id -> definition -> load status).
    #[test]
    #[ignore = "heavy: loads every four-race model; needs War3.mpq; run_layer_gates.sh --ignored"]
    fn models_per_def_roster_csv() {
        let root = world_root();
        let map = map_path();
        if !root.exists() {
            crate::dev_log!("SKIP models_per_def_roster_csv: tree missing");
            return;
        }
        let all = four_race_models(&root);
        if all.len() < 216 {
            crate::dev_log!("SKIP models_per_def_roster_csv: only {} models", all.len());
            return;
        }
        let mut registry = ModelRegistry::open_dir(&root, 1536 * 1024 * 1024);
        let mut csv = String::from("kind,race,group,key,status,meshes,nodes,bytes\n");
        for model in &all {
            let race = ["Human", "Orc", "Undead", "NightElf"]
                .iter()
                .find(|race| model.contains(&format!("\\{race}\\")))
                .copied()
                .unwrap_or("?");
            let group = if model.starts_with("Buildings") {
                "building"
            } else {
                "unit"
            };
            let (status, meshes, nodes, bytes) = match registry.load_detailed(model) {
                LoadOutcome::Loaded(entry) => (
                    "loaded",
                    entry.meshes.len(),
                    entry.nodes.len(),
                    entry.bytes,
                ),
                LoadOutcome::Meshless => ("meshless", 0, 0, 0),
                LoadOutcome::Failed => ("failed", 0, 0, 0),
            };
            csv.push_str(&format!(
                "definition,{race},{group},{model},{status},{meshes},{nodes},{bytes}\n"
            ));
        }
        if map.exists() {
            if let Ok(bundle) = wc3_re::mapfiles::MapBundle::open(&map) {
                if let Some(units) = bundle.units.as_ref() {
                    for unit in units {
                        let id = String::from_utf8_lossy(&unit.type_id).to_string();
                        let def = wc3_sim::data::UnitDefId::from_id(&id).and_then(|def| def.get());
                        let model = def.map(|def| def.model.to_string());
                        let status = match model.as_deref() {
                            None => "no_definition",
                            Some(model) => match registry.load_detailed(model) {
                                LoadOutcome::Loaded(entry) => {
                                    csv.push_str(&format!(
                                        "map_unit,,{id},{model},loaded,{},{},0\n",
                                        entry.meshes.len(),
                                        entry.nodes.len()
                                    ));
                                    continue;
                                }
                                LoadOutcome::Meshless => "meshless",
                                LoadOutcome::Failed => "failed",
                            },
                        };
                        csv.push_str(&format!(
                            "map_unit,,{id},{}, {},0,0,0\n",
                            model.unwrap_or_default(),
                            status
                        ));
                    }
                }
            }
        }
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-world/parity");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("roster.csv");
        std::fs::write(&file, csv).expect("roster csv");
        crate::dev_log!("MODELS_PER_DEF_ROSTER_CSV file={}", file.display());
        assert!(all.len() >= 216);
    }

    // ------------------------------------------------------------ projectiles

    /// Real weapon data must yield the four-race missile arts, and every art
    /// the RoC install ships must resolve to an MDX that parses.
    #[test]
    fn projectile_arts_resolve() {
        let root = world_root();
        if !root.exists() {
            crate::dev_log!("SKIP projectile_arts_resolve: {} missing", root.display());
            return;
        }
        let source = wc3_data::source::DirSource::new(root.clone());
        let db = match wc3_data::GameDatabase::load(&source) {
            Ok(db) => db,
            Err(error) => {
                crate::dev_log!("SKIP projectile_arts_resolve: data load failed: {error}");
                return;
            }
        };
        let arts = missile_arts(&db);
        let mut registry = ModelRegistry::open_dir(&root, 256 * 1024 * 1024);
        let mut seen: std::collections::BTreeSet<String> = Default::default();
        let mut loaded = 0usize;
        let mut meshless = 0usize;
        let mut failed = Vec::new();
        let mut absent = 0usize;
        for art in &arts {
            let Some(model) = resolve_missile_model(&registry, &art.art) else {
                absent += 1;
                continue;
            };
            if !seen.insert(model.clone()) {
                continue;
            }
            match registry.load_detailed(&model) {
                LoadOutcome::Loaded(_) => loaded += 1,
                LoadOutcome::Meshless => meshless += 1,
                LoadOutcome::Failed => failed.push(model),
            }
        }
        crate::dev_log!(
            "PROJECTILE_ART entries={} distinct_present={} loaded={} meshless={} absent={} failed={} sample={:?}",
            arts.len(),
            seen.len(),
            loaded,
            meshless,
            absent,
            failed.len(),
            arts.iter().take(4).map(|a| a.art.as_str()).collect::<Vec<_>>()
        );
        assert!(arts.len() >= 50, "weapon profiles expose missile arts");
        assert!(seen.len() >= 35, "most missile arts exist in the RoC tree");
        assert!(failed.is_empty(), "resolved missile art must parse: {failed:?}");
    }

    /// Two-frame flight render through the game renderer: real missile MDX
    /// models, parabolic motion, spin and fade.
    #[test]
    #[ignore = "heavy: renders missile models through the game renderer; needs War3.mpq + GPU; run_layer_gates.sh --ignored"]
    fn projectiles_render() {
        let root = world_root();
        if !root.exists() {
            crate::dev_log!("SKIP projectiles_render: {} missing", root.display());
            return;
        }
        let source = wc3_data::source::DirSource::new(root.clone());
        let Ok(db) = wc3_data::GameDatabase::load(&source) else {
            crate::dev_log!("SKIP projectiles_render: data load failed");
            return;
        };
        let arts = missile_arts(&db);
        let mut registry = ModelRegistry::open_dir(&root, 1536 * 1024 * 1024);
        let mut chosen: Vec<(String, f32)> = Vec::new();
        let mut seen: std::collections::BTreeSet<String> = Default::default();
        for art in &arts {
            if chosen.len() >= 10 {
                break;
            }
            let Some(model) = resolve_missile_model(&registry, &art.art) else {
                continue;
            };
            if !seen.insert(model.clone()) {
                continue;
            }
            if matches!(registry.load_projectile(&model), LoadOutcome::Loaded(_)) {
                chosen.push((model, art.speed));
            }
        }
        if chosen.len() < 4 {
            crate::dev_log!("SKIP projectiles_render: only {} models", chosen.len());
            return;
        }
        let mut assets = crate::assets::Assets::load(None, None);
        let mut entries: Vec<(ModelEntry, ProjectileFx)> = Vec::new();
        for (slot, (model, speed)) in chosen.iter().enumerate() {
            let entry = match registry.load_projectile(model) {
                LoadOutcome::Loaded(entry) => entry.clone(),
                _ => continue,
            };
            let base = assets.textures.len();
            assets.textures.extend(entry.textures.iter().cloned());
            let mut meshes = entry.meshes.clone();
            for mesh in &mut meshes {
                for layer in &mut mesh.layers {
                    layer.texture += base;
                }
            }
            let index = assets.models.len();
            assets.models.push(crate::assets::ModelAsset {
                kind: wc3_sim::UnitKind::Footman,
                model: None,
                meshes,
                sequences: Vec::new(),
                idle: None,
                walk: None,
                attack: None,
                height: entry.height,
            pivot: 0.0,
                scale: 1.0,
                yaw_offset: 0.0,
                fallback: false,
            });
            let x = -640.0 + slot as f32 * 150.0;
            let origin = Vec3::new(x, 30.0, -300.0);
            let target = Vec3::new(x + 210.0, 80.0, 260.0);
            entries.push((
                entry,
                ProjectileFx::new(model.clone(), index, origin, target, *speed).with_arc(110.0),
            ));
        }
        let font = crate::hud::Font::load();
        let width = 1280u32;
        let height = 720u32;
        let centre = Vec3::new(0.0, 110.0, 0.0);
        let eye = centre + Vec3::new(0.0, 430.0, 760.0);
        let render_at = |t: f32| -> Option<Vec<u8>> {
            let mut units = Vec::new();
            for (entry, fx) in &entries {
                let mut fx = fx.clone();
                fx.t = t;
                units.push(fx.draw(entry, eye));
            }
            let view_proj = Mat4::perspective_rh(
                38f32.to_radians(),
                width as f32 / height as f32,
                1.0,
                4000.0,
            ) * Mat4::look_at_rh(eye, centre, Vec3::Y);
            let scene = crate::game::Scene {
                view_proj,
                eye,
                viewport: Vec2::new(width as f32, height as f32),
                units,
                rings: Vec::new(),
                health: Vec::new(),
                ui: crate::hud::UiBatch::default(),
                overlay: None,
            };
            let options = crate::render::RenderOptions {
                skip_terrain: true,
                skip_ui: true,
                skip_doodads: true,
                ..Default::default()
            };
            crate::render::render_scene_to_rgba_with(
                &assets,
                &font.atlas,
                None,
                &scene,
                width,
                height,
                options,
            )
            .ok()
        };
        let Some(frame_a) = render_at(0.18) else {
            crate::dev_log!("SKIP projectiles_render: frame A render failed");
            return;
        };
        let Some(frame_b) = render_at(0.72) else {
            crate::dev_log!("SKIP projectiles_render: frame B render failed");
            return;
        };
        let mut moved = 0usize;
        let mut diff = vec![0u8; frame_a.len()];
        for (index, (a, b)) in frame_a.chunks_exact(4).zip(frame_b.chunks_exact(4)).enumerate() {
            let delta = (a[0] as i32 - b[0] as i32).abs()
                + (a[1] as i32 - b[1] as i32).abs()
                + (a[2] as i32 - b[2] as i32).abs();
            if delta > 24 {
                moved += 1;
            }
            diff[index * 4] = delta.min(255) as u8;
            diff[index * 4 + 1] = delta.min(255) as u8;
            diff[index * 4 + 2] = delta.min(255) as u8;
            diff[index * 4 + 3] = 255;
        }
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-world/parity");
        let _ = std::fs::create_dir_all(&dir);
        let a_path = dir.join("projectiles.ppm");
        let b_path = dir.join("projectiles-frame-b.ppm");
        let d_path = dir.join("projectiles-diff.ppm");
        crate::render::write_ppm(a_path.to_str().unwrap(), width, height, &frame_a);
        crate::render::write_ppm(b_path.to_str().unwrap(), width, height, &frame_b);
        crate::render::write_ppm(d_path.to_str().unwrap(), width, height, &diff);
        let models: Vec<&str> = entries.iter().map(|(entry, _)| entry.model.as_str()).collect();
        crate::dev_log!(
            "PROJECTILE_RENDER models={} frames=2 moved_px={} art={:?} pose={:?} fallback={:?} sequence={:?} frame_a={} frame_b={}",
            entries.len(),
            moved,
            models,
            entries
                .iter()
                .map(|(entry, _)| entry.pose_sequence.clone())
                .collect::<Vec<_>>(),
            entries
                .iter()
                .map(|(entry, _)| entry.pose_fallback)
                .collect::<Vec<_>>(),
            entries
                .iter()
                .map(|(_, fx)| fx.sequence.label())
                .collect::<Vec<_>>(),
            a_path.display(),
            b_path.display()
        );
        assert!(entries.len() >= 4, "at least four missile models rendered");
        assert!(moved >= 500, "two frames must differ (motion/fade): {moved}");
    }

    /// Edge-on flat missile art (ZigguratMissile-class) must stay visible: a
    /// single-quad model projected edge-on has zero area, so the renderer
    /// billboards flat geometry toward the camera.
    #[test]
    #[ignore = "heavy: renders a flat missile edge-on; needs War3.mpq + GPU; run_layer_gates.sh --ignored"]
    fn projectile_billboard_edge_on() {
        let root = world_root();
        if !root.exists() {
            crate::dev_log!("SKIP projectile_billboard_edge_on: {} missing", root.display());
            return;
        }
        let source = wc3_data::source::DirSource::new(root.clone());
        let Ok(db) = wc3_data::GameDatabase::load(&source) else {
            crate::dev_log!("SKIP projectile_billboard_edge_on: data load failed");
            return;
        };
        let arts = missile_arts(&db);
        let mut registry = ModelRegistry::open_dir(&root, 512 * 1024 * 1024);
        let mut picked: Option<(String, ModelEntry)> = None;
        for art in &arts {
            let Some(model) = resolve_missile_model(&registry, &art.art) else {
                continue;
            };
            let entry = match registry.load_projectile(&model) {
                LoadOutcome::Loaded(entry) => entry.clone(),
                _ => continue,
            };
            if entry.billboard.is_none() {
                continue;
            }
            let prefer = model.to_ascii_lowercase().contains("ziggurat");
            if prefer || picked.is_none() {
                picked = Some((model.clone(), entry));
                if prefer {
                    break;
                }
            }
        }
        let Some((model, entry)) = picked else {
            crate::dev_log!("SKIP projectile_billboard_edge_on: no flat missile art in tree");
            return;
        };
        let axis = entry.billboard.expect("flat model");
        let z_up = entry.meshes.first().map(|mesh| mesh.z_up).unwrap_or(true);
        let axis_world = if z_up {
            Vec3::new(axis.x, axis.z, -axis.y)
        } else {
            axis
        };
        let mut assets = crate::assets::Assets::load(None, None);
        let base = assets.textures.len();
        assets.textures.extend(entry.textures.iter().cloned());
        let mut meshes = entry.meshes.clone();
        for mesh in &mut meshes {
            for layer in &mut mesh.layers {
                layer.texture += base;
            }
        }
        let index = assets.models.len();
        assets.models.push(crate::assets::ModelAsset {
            kind: wc3_sim::UnitKind::Footman,
            model: None,
            meshes,
            sequences: Vec::new(),
            idle: None,
            walk: None,
            attack: None,
            height: entry.height,
            pivot: 0.0,
            scale: 1.0,
            yaw_offset: 0.0,
            fallback: false,
        });
        // Flight along +Z (yaw 0) keeps the flat plane's normal in model space.
        let origin = Vec3::new(0.0, 120.0, -200.0);
        let target = Vec3::new(0.0, 120.0, 200.0);
        let mut fx = ProjectileFx::new(model.clone(), index, origin, target, 900.0);
        // Mid-flight: alpha is 1.0 (t=0 starts at the 5% fade-in floor).
        fx.t = 0.5;
        let pos = fx.position();
        // Edge-on means the eye lies inside the plane: pick the axis that is
        // perpendicular to the flat normal.
        let in_plane = if axis_world.y.abs() > 0.9 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        let eye = pos + in_plane * 600.0;
        let font = crate::hud::Font::load();
        let width = 640u32;
        let height = 360u32;
        let render = |billboard: bool| -> Option<Vec<u8>> {
            // Keep the look-at basis valid when the eye lies on the Y axis.
            let view = (eye - pos).normalize_or_zero();
            let up = if view.y.abs() > 0.9 { Vec3::Z } else { Vec3::Y };
            let scene = crate::game::Scene {
                view_proj: Mat4::perspective_rh(
                    35f32.to_radians(),
                    width as f32 / height as f32,
                    1.0,
                    3000.0,
                ) * Mat4::look_at_rh(eye, pos, up),
                eye,
                viewport: Vec2::new(width as f32, height as f32),
                units: vec![fx.draw_oriented(&entry, eye, billboard)],
                rings: Vec::new(),
                health: Vec::new(),
                ui: crate::hud::UiBatch::default(),
                overlay: None,
            };
            let options = crate::render::RenderOptions {
                skip_terrain: true,
                skip_ui: true,
                skip_doodads: true,
                ..Default::default()
            };
            crate::render::render_scene_to_rgba_with(
                &assets,
                &font.atlas,
                None,
                &scene,
                width,
                height,
                options,
            )
            .ok()
        };
        let (Some(before), Some(after)) = (render(false), render(true)) else {
            crate::dev_log!("SKIP projectile_billboard_edge_on: render failed");
            return;
        };
        let coverage = |frame: &[u8]| -> usize {
            let bg = [frame[0], frame[1], frame[2]];
            frame
                .chunks_exact(4)
                .filter(|pixel| {
                    (pixel[0] as i32 - bg[0] as i32).abs()
                        + (pixel[1] as i32 - bg[1] as i32).abs()
                        + (pixel[2] as i32 - bg[2] as i32).abs()
                        > 18
                })
                .count()
        };
        let (px_before, px_after) = (coverage(&before), coverage(&after));
        // Diagnostics: geometry extents and a front view prove the model renders
        // at all in this scene setup (edge-on before is expected to be zero).
        let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for mesh in &entry.meshes {
            for p in &mesh.positions {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
        }
        let front_eye = pos + axis_world * 600.0;
        let front_up = if axis_world.y.abs() > 0.9 { Vec3::Z } else { Vec3::Y };
        let front_scene = crate::game::Scene {
            view_proj: Mat4::perspective_rh(35f32.to_radians(), width as f32 / height as f32, 1.0, 3000.0)
                * Mat4::look_at_rh(front_eye, pos, front_up),
            eye: front_eye,
            viewport: Vec2::new(width as f32, height as f32),
            units: vec![fx.draw_oriented(&entry, front_eye, true)],
            rings: Vec::new(),
            health: Vec::new(),
            ui: crate::hud::UiBatch::default(),
            overlay: None,
        };
        let front = crate::render::render_scene_to_rgba_with(
            &assets,
            &font.atlas,
            None,
            &front_scene,
            width,
            height,
            crate::render::RenderOptions { skip_terrain: true, skip_ui: true, skip_doodads: true, ..Default::default() },
        )
        .ok();
        let px_front = front.as_deref().map(coverage).unwrap_or(0);
        let layers: Vec<(usize, f32, bool, bool, usize)> = entry
            .meshes
            .iter()
            .map(|mesh| {
                (
                    mesh.layers.len(),
                    mesh.layers.first().map(|layer| layer.alpha).unwrap_or(-1.0),
                    mesh.hide,
                    mesh.z_up,
                    mesh.indices.len(),
                )
            })
            .collect();
        crate::dev_log!(
            "PROJECTILE_BILLBOARD_GEOMETRY model={model} meshes={} height={:.2} bbox={:?}..{:?} front_px={px_front} layers={layers:?}",
            entry.meshes.len(),
            entry.height,
            lo.to_array(),
            hi.to_array()
        );
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-world/parity");
        let _ = std::fs::create_dir_all(&dir);
        let b_path = dir.join("projectile-billboard-before.ppm");
        let a_path = dir.join("projectile-billboard-after.ppm");
        crate::render::write_ppm(b_path.to_str().unwrap(), width, height, &before);
        crate::render::write_ppm(a_path.to_str().unwrap(), width, height, &after);
        crate::dev_log!(
            "PROJECTILE_BILLBOARD model={model} axis={axis:?} edge_on_px_before={px_before} edge_on_px_after={px_after} before={} after={}",
            b_path.display(),
            a_path.display()
        );
        assert!(
            px_after > px_before,
            "billboard must add coverage edge-on: before={px_before} after={px_after}"
        );
        assert!(px_after >= 200, "billboarded missile is visible: {px_after}");
    }

    /// Spell playback adopts the canonical `ability_animator(..., Cast)` API:
    /// every hero resolves an authored Cast sequence and the bind pose moves.
    #[test]
    #[ignore = "heavy: starts canonical Cast playback for every hero; needs War3.mpq; run_layer_gates.sh --ignored"]
    fn spell_cast_playback() {
        let root = world_root();
        if !root.exists() {
            crate::dev_log!("SKIP spell_cast_playback: {} missing", root.display());
            return;
        }
        let mut registry = ModelRegistry::open_dir(&root, 1536 * 1024 * 1024);
        // RoC hero roster ids (`roster_gen.rs`), capitalised four-CCs.
        let ids = [
            "Hpal", "Hamg", "Hmkg", "Ofar", "Obla", "Otch", "Udea", "Udre", "Ulic", "Ekee",
            "Emoo", "Edem",
        ];
        let mut exact = 0usize;
        let mut fallback = 0usize;
        let mut missing = 0usize;
        let mut animated = 0usize;
        let mut sample: Vec<String> = Vec::new();
        for id in ids {
            let Some(def) = wc3_sim::data::def_of(id) else {
                missing += 1;
                continue;
            };
            let Some(mut playback) =
                SpellPlayback::start(&mut registry, def.model, AbilitySequence::Cast)
            else {
                missing += 1;
                continue;
            };
            if playback.fallback {
                fallback += 1;
            } else {
                exact += 1;
            }
            sample.push(format!("{id}:{}", playback.sequence));
            let before = playback.posed_meshes();
            for _ in 0..4 {
                playback.advance(0.1);
            }
            let after = playback.posed_meshes();
            let moved = before.iter().zip(&after).any(|(a, b)| {
                a.positions
                    .iter()
                    .zip(&b.positions)
                    .any(|(p, q)| (*p - *q).length() > 0.5)
            });
            if moved {
                animated += 1;
            }
        }
        crate::dev_log!(
            "SPELL_CAST_PLAYBACK heroes={} exact={} fallback={} missing={} animated={} sample={:?}",
            ids.len(),
            exact,
            fallback,
            missing,
            animated,
            &sample[..sample.len().min(8)]
        );
        assert!(
            exact >= 6,
            "canonical Cast sequences resolve for heroes: {sample:?}"
        );
        assert!(animated >= 6, "Cast playback animates the bind pose");
    }

    /// Geoset/LOD table for one MDX read straight from the extracted install.
    ///
    /// WC3 ships several detail levels of the same body as separate geosets
    /// (`geoset.lod`); a renderer must draw exactly one group. This prints the
    /// authored table (id, lod, name, triangles, helper-plane flag, alpha at
    /// the `Stand` sequence) so the draw set can be audited model by model.
    fn geoset_table(relative: &str, bytes: &[u8]) -> Option<MdxModel> {
        let mdx = MdxModel::parse(bytes).ok()?;
        let stand = mdx
            .sequences
            .iter()
            .find(|sequence| sequence.name.eq_ignore_ascii_case("stand"))
            .cloned();
        let mut hidden = vec![false; mdx.geosets.len()];
        for (index, geoset) in mdx.geosets.iter().enumerate() {
            hidden[index] = wc3_anim::runtime::helper_plane(geoset);
        }
        let alphas = wc3_anim::runtime::mesh_alphas(&mdx, &hidden, stand.as_ref(), 0);
        let lowest = Geoset::lowest_lod(&mdx.geosets);
        for (index, geoset) in mdx.geosets.iter().enumerate() {
            let alpha = alphas.get(index).copied().unwrap_or(1.0);
            let material = mdx.materials.get(geoset.material_id as usize);
            let layers: Vec<String> = material
                .map(|material| {
                    material
                        .layers
                        .iter()
                        .map(|layer| {
                            let path = mdx
                                .textures
                                .get(layer.texture_id as usize)
                                .map(|texture| {
                                    format!(
                                        "{} repl={} blend={} alpha={:.2}",
                                        texture.path.trim_end_matches('\0'),
                                        texture.replaceable_id,
                                        layer.blend_mode,
                                        layer.alpha
                                    )
                                })
                                .unwrap_or_default();
                            path
                        })
                        .collect()
                })
                .unwrap_or_default();
            crate::dev_log!(
                "GEOSET model={} id={} lod={} name={:?} tris={} hide={} stand_alpha={:.3} drawn={} material={} layers={:?}",
                relative,
                index,
                geoset.lod,
                geoset.lod_name,
                geoset.faces.len() / 3,
                hidden[index],
                alpha,
                Some(geoset.lod) == lowest,
                geoset.material_id,
                layers
            );
        }
        crate::dev_log!(
            "GEOSET_LOD model={} version={} geosets={} lowest_lod={:?} lods={:?}",
            relative,
            mdx.version,
            mdx.geosets.len(),
            lowest,
            mdx.geosets
                .iter()
                .map(|geoset| geoset.lod)
                .collect::<Vec<_>>()
        );
        Some(mdx)
    }

    /// Peon + Footman + Great Hall in one frame at the Justice aspect, with the
    /// draw counters of that frame: one draw per visible (geoset, layer).
    #[test]
    #[ignore = "heavy: headless trio frame at the Justice camera; needs War3.mpq + GPU; run_layer_gates.sh --ignored"]
    fn unit_trio_justice_frame() {
        let Some((assets, _)) = justice_assets() else {
            crate::dev_log!("SKIP trio: install/Justice missing");
            return;
        };
        let width = 1400u32;
        let height = 700u32;
        let mut camera = crate::Camera::new(assets.world_size);
        camera.viewport = glam::Vec2::new(width as f32, height as f32);
        camera.distance = 900.0;
        let mut focus_y = 0.0f32;
        for _ in 0..3 {
            camera.target = glam::Vec2::new(0.0, focus_y);
            let projected = camera
                .world_to_screen(glam::Vec3::new(0.0, 120.0, 0.0))
                .map(|point| point.y)
                .unwrap_or(height as f32 * 0.5);
            focus_y += (height as f32 * 0.5 - projected) / 40.0;
        }
        camera.target = glam::Vec2::new(0.0, focus_y);
        let placements = [
            ("opeo", -260.0f32, 60.0f32),
            ("hfoo", -120.0, 60.0),
            ("ogre", 160.0, 0.0),
        ];
        let mut units = Vec::new();
        let mut expected = 0usize;
        for (def, x, z) in placements {
            let index = assets.model_index_for_def(def);
            let entry = assets.model_for_def(def);
            let nodes = entry
                .model
                .as_ref()
                .map(|mdx| {
                    let mut animator = wc3_anim::Animator::from_model(mdx);
                    if let Some(idle) = entry.idle.and_then(|idle| entry.sequences.get(idle)) {
                        animator.play(idle);
                    }
                    animator.update(0.0);
                    animator.world_matrices()
                })
                .unwrap_or_default();
            let mesh_alpha = assets.mesh_alpha(index, None, 0);
            let layer_alpha = assets.mesh_layer_alpha(index, None, 0);
            for (mesh_index, mesh) in entry.meshes.iter().enumerate() {
                if mesh.hide || mesh_alpha.get(mesh_index).copied().unwrap_or(1.0) <= 0.001 {
                    continue;
                }
                for (layer_index, layer) in mesh.layers.iter().enumerate() {
                    let alpha = layer_alpha
                        .get(mesh_index)
                        .and_then(|alphas| alphas.get(layer_index))
                        .copied()
                        .unwrap_or(layer.alpha);
                    if alpha > 0.001 {
                        expected += 1;
                    }
                }
            }
            crate::dev_log!(
                "TRIO def={def} model={index} geosets={} visible_layers={}",
                entry.meshes.len(),
                mesh_alpha
                    .iter()
                    .zip(entry.meshes.iter())
                    .filter(|(alpha, mesh)| **alpha > 0.001 && !mesh.hide)
                    .map(|(_, mesh)| mesh.layers.len())
                    .sum::<usize>()
            );
            units.push(crate::game::UnitDraw {
                model: index,
                world: Mat4::from_scale_rotation_translation(
                    Vec3::splat(entry.scale.max(0.01)),
                    glam::Quat::from_rotation_y(entry.yaw_offset + 0.6),
                    Vec3::new(x, -entry.pivot, z),
                ),
                tint: if def == "hfoo" {
                    [1.0, 0.48, 0.42, 1.0]
                } else {
                    [0.55, 0.72, 1.0, 1.0]
                },
                nodes,
                mesh_alpha,
                layer_alpha,
            });
        }
        let font = crate::hud::Font::load();
        let scene = |units: Vec<crate::game::UnitDraw>| crate::Scene {
            view_proj: camera.view_proj(),
            eye: camera.eye(),
            viewport: camera.viewport,
            units,
            rings: Vec::new(),
            health: Vec::new(),
            ui: crate::hud::UiBatch::default(),
            overlay: None,
        };
        let options = crate::render::RenderOptions {
            skip_terrain: true,
            skip_doodads: true,
            skip_ui: true,
            ..Default::default()
        };
        let (_, empty) = crate::render::render_scene_with_skin(
            &assets,
            &font.atlas,
            None,
            &scene(Vec::new()),
            width,
            height,
            options,
            None,
        )
        .expect("trio empty frame");
        let (rgba, frame) = crate::render::render_scene_with_skin(
            &assets,
            &font.atlas,
            None,
            &scene(units),
            width,
            height,
            options,
            None,
        )
        .expect("trio frame");
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-render");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("unit-trio-justice.ppm");
        crate::render::write_ppm(path.to_str().unwrap(), width, height, &rgba);
        let measured = frame.draws.saturating_sub(empty.draws) as usize;
        crate::dev_log!(
            "TRIO_DRAWS measured={measured} expected={expected} file={}",
            path.display()
        );
        assert_eq!(measured, expected, "one draw per visible (geoset, layer)");
    }

    /// A/B frame of the three Justice workers: the old no-sequence geoset set
    /// (every geoset: two extra carry bodies + gore) against the fixed one.
    #[test]
    #[ignore = "heavy: two headless frames of the Justice worker cluster; needs War3.mpq + GPU; run_layer_gates.sh --ignored"]
    fn peon_geoset_ab_frame() {
        let Some((assets, _)) = justice_assets() else {
            crate::dev_log!("SKIP peon_ab: install/Justice missing");
            return;
        };
        let mut game = crate::Game::new(assets, 0x5EED);
        game.update(crate::game::TICK_SECONDS);
        game.screen = crate::game::Screen::Playing;
        game.update(crate::game::TICK_SECONDS);
        let workers: Vec<glam::Vec2> = game
            .sim
            .alive_units()
            .filter(|unit| unit.def().id.eq_ignore_ascii_case("opeo"))
            .map(|unit| unit.pos)
            .collect();
        let Some(first) = workers.first().copied() else {
            crate::dev_log!("SKIP peon_ab: no workers");
            return;
        };
        // Densest trio, framed from the same side as the operator screenshot.
        let mut cluster: Vec<glam::Vec2> = workers
            .iter()
            .copied()
            .filter(|pos| pos.distance(first) < 600.0)
            .collect();
        cluster.sort_by(|a, b| a.distance(first).total_cmp(&b.distance(first)));
        cluster.truncate(3);
        let centre = cluster
            .iter()
            .fold(glam::Vec2::ZERO, |acc, pos| acc + *pos)
            / cluster.len() as f32;
        let width = 1000u32;
        let height = 700u32;
        game.camera.viewport = glam::Vec2::new(width as f32, height as f32);
        game.camera.distance = 420.0;
        game.camera.target = glam::Vec2::ZERO;
        // Studio framing: the workers stand on the y=0 plane so the fixed RTS
        // camera (which looks at y=0) centres them without chasing terrain
        // height. Terrain and doodads are skipped; the frame isolates the
        // geoset draw set.
        let mut focus_y = 0.0f32;
        for _ in 0..2 {
            game.camera.target = glam::Vec2::new(0.0, focus_y);
            let projected = game
                .camera
                .world_to_screen(glam::Vec3::new(0.0, 50.0, 0.0))
                .map(|point| point.y)
                .unwrap_or(height as f32 * 0.5);
            focus_y += (height as f32 * 0.5 - projected) / 40.0;
        }
        game.camera.target = glam::Vec2::new(0.0, focus_y);
        let assets = &game.assets;
        let index = assets.model_index_for_def("opeo");
        let entry = assets.model_for_def("opeo");
        let nodes = entry
            .model
            .as_ref()
            .map(|mdx| {
                let mut animator = wc3_anim::Animator::from_model(mdx);
                if let Some(idle) = entry.idle.and_then(|idle| entry.sequences.get(idle)) {
                    animator.play(idle);
                }
                animator.update(0.0);
                animator.world_matrices()
            })
            .unwrap_or_default();
        let resolved = assets.mesh_alpha(index, None, 0);
        let resolved_layers = assets.mesh_layer_alpha(index, None, 0);
        let legacy = vec![1.0f32; entry.meshes.len()];
        let legacy_layers = entry
            .meshes
            .iter()
            .map(|mesh| {
                mesh.layers
                    .iter()
                    .map(|layer| layer.alpha)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let font = crate::hud::Font::load();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-render");
        let _ = std::fs::create_dir_all(&dir);
        for (label, mesh_alpha, layer_alpha) in [
            ("before", legacy.clone(), legacy_layers.clone()),
            ("after", resolved.clone(), resolved_layers.clone()),
        ] {
            let units = (0..3)
                .map(|unit_index| {
                    let x = (unit_index as f32 - 1.0) * 110.0;
                    let facing = unit_index as f32 * 1.1;
                    crate::game::UnitDraw {
                        model: index,
                        world: Mat4::from_scale_rotation_translation(
                            Vec3::splat(entry.scale.max(0.01)),
                            glam::Quat::from_rotation_y(facing + entry.yaw_offset),
                            Vec3::new(x, 0.0, 0.0),
                        ),
                        tint: [0.55, 0.72, 1.0, 1.0],
                        nodes: nodes.clone(),
                        mesh_alpha: mesh_alpha.clone(),
                        layer_alpha: layer_alpha.clone(),
                    }
                })
                .collect::<Vec<_>>();
            let scene = crate::Scene {
                view_proj: game.camera.view_proj(),
                eye: game.camera.eye(),
                viewport: game.camera.viewport,
                units,
                rings: Vec::new(),
                health: Vec::new(),
                ui: crate::hud::UiBatch::default(),
                overlay: None,
            };
            let options = crate::render::RenderOptions {
                skip_terrain: true,
                skip_doodads: true,
                skip_ui: true,
                ..Default::default()
            };
            let rgba = crate::render::render_scene_to_rgba_with_skin(
                assets,
                &font.atlas,
                None,
                &scene,
                width,
                height,
                options,
                Some(wc3_render::mesh::SkinMode::Cpu),
            )
            .expect("peon A/B render");
            let path = dir.join(format!("peon-mesh-{label}.ppm"));
            crate::render::write_ppm(path.to_str().unwrap(), width, height, &rgba);
            crate::dev_log!(
                "PEON_AB label={label} centre=({:.0},{:.0}) alphas={:?} file={}",
                centre.x,
                centre.y,
                mesh_alpha
                    .iter()
                    .map(|a| (a * 1000.0).round() / 1000.0)
                    .collect::<Vec<_>>(),
                path.display()
            );
        }
    }

    /// Layer isolation for one geoset: which material layer paints the pack.
    ///
    /// The Peon pack geoset has two layers (team colour + diffuse). Rendering
    /// each layer alone proves whether a chevron/ramp pattern comes from the
    /// team-colour texture or from the authored diffuse atlas.
    #[test]
    #[ignore = "heavy: three headless close-ups of one geoset; needs War3.mpq + GPU; run_layer_gates.sh --ignored"]
    fn peon_layer_isolation() {
        let Some((assets, _)) = justice_assets() else {
            crate::dev_log!("SKIP layer_isolation: install/Justice missing");
            return;
        };
        let index = assets.model_index_for_def("opeo");
        let entry = assets.model_for_def("opeo");
        let mut camera = crate::Camera::new(glam::Vec2::splat(1000.0));
        camera.viewport = glam::Vec2::new(500.0, 500.0);
        camera.target = glam::Vec2::ZERO;
        camera.distance = 400.0;
        let nodes = entry
            .model
            .as_ref()
            .map(|mdx| {
                let mut animator = wc3_anim::Animator::from_model(mdx);
                if let Some(idle) = entry.idle.and_then(|idle| entry.sequences.get(idle)) {
                    animator.play(idle);
                }
                animator.update(0.0);
                animator.world_matrices()
            })
            .unwrap_or_default();
        let mesh_alpha = assets.mesh_alpha(index, None, 0);
        let layer_alpha = assets.mesh_layer_alpha(index, None, 0);
        let font = crate::hud::Font::load();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-render");
        let _ = std::fs::create_dir_all(&dir);
        let options = crate::render::RenderOptions {
            skip_terrain: true,
            skip_doodads: true,
            skip_ui: true,
            ..Default::default()
        };
        for (label, keep) in [("layer0", 0usize), ("layer1", 1usize), ("all", 2usize)] {
            let mut alpha = layer_alpha.clone();
            if keep < 2 {
                for (mesh_index, layers) in alpha.iter_mut().enumerate() {
                    for (layer_index, value) in layers.iter_mut().enumerate() {
                        if mesh_index == 1 && layer_index != keep {
                            *value = 0.0;
                        }
                    }
                }
            }
            let unit = crate::game::UnitDraw {
                model: index,
                world: Mat4::IDENTITY,
                tint: [0.55, 0.72, 1.0, 1.0],
                nodes: nodes.clone(),
                mesh_alpha: mesh_alpha.clone(),
                layer_alpha: alpha,
            };
            let scene = crate::Scene {
                view_proj: camera.view_proj(),
                eye: camera.eye(),
                viewport: camera.viewport,
                units: vec![unit],
                rings: Vec::new(),
                health: Vec::new(),
                ui: crate::hud::UiBatch::default(),
                overlay: None,
            };
            let rgba = crate::render::render_scene_to_rgba_with_skin(
                &assets,
                &font.atlas,
                None,
                &scene,
                500,
                500,
                options,
                Some(wc3_render::mesh::SkinMode::Cpu),
            )
            .expect("layer isolation render");
            let path = dir.join(format!("peon-layer-{label}.ppm"));
            crate::render::write_ppm(path.to_str().unwrap(), 500, 500, &rgba);
            crate::dev_log!("PEON_LAYER {label} file={}", path.display());
        }
    }

    /// Draw-count contract: a unit emits exactly one draw per visible
    /// (geoset, material layer) pair — never the model's materials x geosets
    /// cross product (the "two bodies + duplicated pauldron" defect).
    ///
    /// The measurement is `Counters::draws` of the same frame with and without
    /// the unit, so terrain, water and doodads cancel out.
    #[test]
    #[ignore = "heavy: headless render + draw counters; needs War3.mpq + GPU; run_layer_gates.sh --ignored"]
    fn draw_count_is_geoset_layers() {
        let Some((assets, _)) = justice_assets() else {
            crate::dev_log!("SKIP draw_count: install/Justice missing");
            return;
        };
        let width = 320u32;
        let height = 240u32;
        let mut camera = crate::Camera::new(assets.world_size);
        camera.viewport = glam::Vec2::new(width as f32, height as f32);
        camera.target = glam::Vec2::new(2000.0, 2000.0);
        camera.distance = 1500.0;
        let font = crate::hud::Font::load();
        let options = crate::render::RenderOptions {
            skip_ui: true,
            ..Default::default()
        };
        let scene_with = |units: Vec<crate::game::UnitDraw>| crate::Scene {
            view_proj: camera.view_proj(),
            eye: camera.eye(),
            viewport: camera.viewport,
            units,
            rings: Vec::new(),
            health: Vec::new(),
            ui: crate::hud::UiBatch::default(),
            overlay: None,
        };
        let (_, empty) = crate::render::render_scene_with_skin(
            &assets,
            &font.atlas,
            None,
            &scene_with(Vec::new()),
            width,
            height,
            options,
            None,
        )
        .expect("empty frame");
        for (def, label) in [("opeo", "Peon"), ("hfoo", "Footman"), ("ogre", "GreatHall")] {
            let index = assets.model_index_for_def(def);
            if index == 0 && !assets.model_for_def(def).model.is_some() {
                crate::dev_log!("SKIP draw_count: {def} has no model");
                continue;
            }
            let entry = assets.model_for_def(def);
            let mesh_alpha = assets.mesh_alpha(index, None, 0);
            let layer_alpha = assets.mesh_layer_alpha(index, None, 0);
            if def == "opeo" {
                // The Assets layer resolves a missing sequence to Stand; if it
                // ever stops doing that the Peon draws its two carry variants
                // and the gore geoset again ("two bodies" on the device).
                assert_eq!(
                    mesh_alpha,
                    vec![1.0, 1.0, 0.0, 0.0, 0.0],
                    "Peon without a running sequence renders its Stand geoset set"
                );
            }
            let tint = [0.55, 0.72, 1.0, 1.0];
            let mut expected = 0usize;
            for (mesh_index, mesh) in entry.meshes.iter().enumerate() {
                if mesh_alpha.get(mesh_index).copied().unwrap_or(1.0) <= 0.001 || mesh.hide {
                    continue;
                }
                for (layer_index, layer) in mesh.layers.iter().enumerate() {
                    let alpha = layer_alpha
                        .get(mesh_index)
                        .and_then(|alphas| alphas.get(layer_index))
                        .copied()
                        .unwrap_or(layer.alpha);
                    if alpha * tint[3] > 0.001 {
                        expected += 1;
                    }
                }
            }
            crate::dev_log!(
                "DRAW_COUNT model={label} def={def} geosets={} materials_layers={:?} visible_meshes={} expected={expected}",
                entry.meshes.len(),
                entry
                    .meshes
                    .iter()
                    .map(|mesh| mesh.layers.len())
                    .collect::<Vec<_>>(),
                entry
                    .meshes
                    .iter()
                    .enumerate()
                    .filter(|(mesh_index, mesh)| {
                        !mesh.hide
                            && mesh_alpha.get(*mesh_index).copied().unwrap_or(1.0) > 0.001
                    })
                    .count()
            );
            let ground = assets.height_at(2000.0, 2000.0);
            // Posed node palette from the canonical animator (Stand), exactly
            // what `Game::scene` hands the renderer.
            let nodes = entry
                .model
                .as_ref()
                .map(|mdx| {
                    let mut animator = wc3_anim::Animator::from_model(mdx);
                    if let Some(index) = entry.idle.and_then(|idle| entry.sequences.get(idle))
                    {
                        animator.play(index);
                    }
                    animator.update(0.0);
                    animator.world_matrices()
                })
                .unwrap_or_default();
            let unit = crate::game::UnitDraw {
                model: index,
                world: Mat4::from_translation(Vec3::new(2000.0, ground - entry.pivot, 2000.0)),
                tint,
                nodes,
                mesh_alpha,
                layer_alpha,
            };
            let (_, frame) = crate::render::render_scene_with_skin(
                &assets,
                &font.atlas,
                None,
                &scene_with(vec![unit]),
                width,
                height,
                options,
                None,
            )
            .expect("unit frame");
            let measured = frame.draws.saturating_sub(empty.draws) as usize;
            crate::dev_log!(
                "DRAW_COUNT_MEASURED model={label} empty={} with_unit={} measured={measured} expected={expected}",
                empty.draws,
                frame.draws
            );
            assert_eq!(
                measured, expected,
                "{label} must emit one draw per visible geoset layer (measured {measured}, expected {expected})"
            );
        }
    }

    /// Decode one BLP from the install to PPM for art verification.
    #[test]
    #[ignore = "diagnostic: writes a texture PPM; needs War3.mpq; run_layer_gates.sh --ignored"]
    fn dump_texture_ppm() {
        let Some(mpq) = [
            "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
            "/data/data/org.wc3rust.p0/files/War3.mpq",
        ]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
        else {
            return;
        };
        let mut storm = wc3_re::storm::StormFs::new();
        if storm.add_archive("War3", &mpq).is_err() {
            return;
        }
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-render");
        let _ = std::fs::create_dir_all(&dir);
        for (path, label) in [
            ("Units\\Orc\\Peon\\peon.blp", "peon-texture"),
            (
                "ReplaceableTextures\\TeamColor\\TeamColor00.blp",
                "teamcolor00",
            ),
        ] {
            let Ok(bytes) = storm.read(path) else {
                continue;
            };
            let Ok(image) = wc3_formats::blp::decode(&bytes) else {
                continue;
            };
            let out = dir.join(format!("{label}.ppm"));
            crate::render::write_ppm(
                out.to_str().unwrap(),
                image.width,
                image.height,
                &image.rgba,
            );
            crate::dev_log!("TEXTURE_PPM {path} -> {} ({}x{})", out.display(), image.width, image.height);
        }
    }

    fn justice_assets() -> Option<(crate::assets::Assets, std::path::PathBuf)> {
        let mpq = [
            "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
            "/data/data/org.wc3rust.p0/files/War3.mpq",
        ]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())?;
        let map = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
        );
        if !map.exists() {
            return None;
        }
        Some((crate::assets::Assets::load(Some(&mpq), Some(&map)), map))
    }

    /// Close-up of the orc workers on Justice in both skinning modes.
    ///
    /// The device forces `SkinMode::Cpu`; this frame is the headless twin of the
    /// "peons float / double body" screenshot and is the acceptance artifact for
    /// the geoset draw set.
    #[test]
    #[ignore = "heavy: headless GPU close-up of the Justice workers; needs War3.mpq + GPU; run_layer_gates.sh --ignored"]
    fn peon_closeup_skinning_audit() {
        let Some((assets, _map)) = justice_assets() else {
            crate::dev_log!("SKIP peon_closeup: install/Justice missing");
            return;
        };
        let mut game = crate::Game::new(assets, 0x5EED);
        // The retail boot menu owns the first tick; tick it away, then drive
        // the battle through the public `screen` field.
        game.update(crate::game::TICK_SECONDS);
        game.screen = crate::game::Screen::Playing;
        game.update(crate::game::TICK_SECONDS);
        let workers: Vec<_> = game
            .sim
            .alive_units()
            .filter(|unit| unit.def().id.eq_ignore_ascii_case("opeo"))
            .map(|unit| unit.pos)
            .collect();
        let Some(first) = workers.first().copied() else {
            crate::dev_log!("SKIP peon_closeup: no orc workers on the map");
            return;
        };
        // Frame the densest cluster: workers are spread over mine, hall and
        // lumber camp, so the centroid can land on empty grass.
        let mut centre = first;
        let mut best = 0usize;
        for candidate in &workers {
            let near = workers
                .iter()
                .filter(|pos| pos.distance(*candidate) < 420.0)
                .count();
            if near > best {
                best = near;
                centre = *candidate;
            }
        }
        let width = 1000u32;
        let height = 700u32;
        game.camera.viewport = glam::Vec2::new(width as f32, height as f32);
        game.camera.target = centre;
        // The RTS camera looks at the y=0 plane while the map sits at
        // y~=300-600, so a tight 900-unit orbit pushes the units off the top
        // edge; 1700 keeps the whole cluster in frame with ~0.45 px/unit.
        game.camera.distance = 1700.0;
        let scene = game.scene();
        for pos in &workers {
            let screen = game.camera.world_to_screen(glam::Vec3::new(
                pos.x,
                game.assets.height_at(pos.x, pos.y),
                pos.y,
            ));
            crate::dev_log!(
                "PEON_CLOSEUP worker pos=({:.0},{:.0}) screen={:?}",
                pos.x,
                pos.y,
                screen
            );
        }
        crate::dev_log!(
            "PEON_CLOSEUP workers={} cluster={} centre=({:.0},{:.0}) units={}",
            workers.len(),
            best,
            centre.x,
            centre.y,
            scene.units.len()
        );
        let font = crate::hud::Font::load();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-render");
        let _ = std::fs::create_dir_all(&dir);
        for (label, skin) in [
            ("gpu", Some(wc3_render::mesh::SkinMode::Gpu)),
            ("cpu", Some(wc3_render::mesh::SkinMode::Cpu)),
        ] {
            let options = crate::render::RenderOptions {
                skip_ui: true,
                ..Default::default()
            };
            let rgba = crate::render::render_scene_to_rgba_with_skin(
                &game.assets,
                &font.atlas,
                None,
                &scene,
                width,
                height,
                options,
                skin,
            )
            .expect("peon close-up render");
            let path = dir.join(format!("peon-closeup-{label}.ppm"));
            crate::render::write_ppm(path.to_str().unwrap(), width, height, &rgba);
            crate::dev_log!("PEON_CLOSEUP skin={label} file={}", path.display());
        }
    }

    /// Posed bounding box per geoset: proves whether a geoset sits on the body
    /// after skinning or hangs beside it (the "detached shoulder plate").
    #[test]
    fn geoset_posed_bounds_peon() {
        if !world_root().exists() {
            crate::dev_log!("SKIP geoset_posed_bounds: tree missing");
            return;
        }
        let model = "Units\\Orc\\Peon\\peon.mdx";
        let Ok(bytes) = std::fs::read(world_root().join(model.replace('\\', "/"))) else {
            crate::dev_log!("SKIP geoset_posed_bounds: {model} missing");
            return;
        };
        let mdx = MdxModel::parse(&bytes).expect("peon parses");
        let mut animator = wc3_anim::Animator::from_model(&mdx);
        if let Some(stand) = mdx
            .sequences
            .iter()
            .find(|sequence| sequence.name.eq_ignore_ascii_case("stand"))
        {
            animator.play(stand);
        }
        animator.update(0.0);
        let nodes = animator.world_matrices();
        for (index, geoset) in mdx.geosets.iter().enumerate() {
            let skin = wc3_anim::GeosetSkin::from_geoset(geoset);
            let mut local_min = Vec3::splat(f32::INFINITY);
            let mut local_max = Vec3::splat(f32::NEG_INFINITY);
            let mut posed_min = Vec3::splat(f32::INFINITY);
            let mut posed_max = Vec3::splat(f32::NEG_INFINITY);
            for (vertex, position) in geoset.vertices.iter().enumerate() {
                local_min = local_min.min(*position);
                local_max = local_max.max(*position);
                let mut posed = Vec3::ZERO;
                let mut total = 0.0;
                if let Some(list) = skin.influences.get(vertex) {
                    for &(bone, weight) in list {
                        if let Some(node) = nodes.get(bone as usize) {
                            posed += node.transform_point3(*position) * weight;
                            total += weight;
                        }
                    }
                }
                if total > 1e-5 {
                    posed /= total;
                } else {
                    posed = *position;
                }
                posed_min = posed_min.min(posed);
                posed_max = posed_max.max(posed);
            }
            crate::dev_log!(
                "GEOSET_BOUNDS model={model} id={index} local=({:.0},{:.0},{:.0})..({:.0},{:.0},{:.0}) posed=({:.0},{:.0},{:.0})..({:.0},{:.0},{:.0})",
                local_min.x, local_min.y, local_min.z, local_max.x, local_max.y, local_max.z,
                posed_min.x, posed_min.y, posed_min.z, posed_max.x, posed_max.y, posed_max.z
            );
        }
    }

    /// Geoset alpha per sequence: which geosets a model actually draws while it
    /// stands, walks, fights, dies and decays.
    ///
    /// WC3 carries alternative bodies (Peon gold/lumber/working) as separate
    /// geosets hidden by `GEOA` alpha tracks; a wrong sequence extent or time
    /// shows two bodies at once.
    #[test]
    fn geoset_alpha_per_sequence_peon() {
        if !world_root().exists() {
            crate::dev_log!("SKIP geoset_alpha_per_sequence: tree missing");
            return;
        }
        let model = "Units\\Orc\\Peon\\peon.mdx";
        let Ok(bytes) = std::fs::read(world_root().join(model.replace('\\', "/"))) else {
            crate::dev_log!("SKIP geoset_alpha_per_sequence: {model} missing");
            return;
        };
        let mdx = MdxModel::parse(&bytes).expect("peon parses");
        let mut hidden = vec![false; mdx.geosets.len()];
        for (index, geoset) in mdx.geosets.iter().enumerate() {
            hidden[index] = wc3_anim::runtime::helper_plane(geoset);
        }
        // No running sequence: the static GEOA alpha only. Every unit that has
        // no animator entry (or was never played) renders with this set.
        let static_alpha = wc3_anim::runtime::mesh_alphas(&mdx, &hidden, None, 0);
        crate::dev_log!(
            "GEOSET_SEQ model={model} seq=<none> alphas={:?}",
            static_alpha
                .iter()
                .map(|a| (a * 1000.0).round() / 1000.0)
                .collect::<Vec<_>>()
        );
        for name in ["Stand", "Walk", "Attack", "Death", "Decay Flesh", "Work"] {
            let Some(sequence) = mdx
                .sequences
                .iter()
                .find(|sequence| sequence.name.eq_ignore_ascii_case(name))
                .cloned()
            else {
                continue;
            };
            let alpha = wc3_anim::runtime::mesh_alphas(
                &mdx,
                &hidden,
                Some(&sequence),
                sequence.start + 100,
            );
            crate::dev_log!(
                "GEOSET_SEQ model={model} seq={name} alphas={:?}",
                alpha.iter().map(|a| (a * 1000.0).round() / 1000.0).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn geoset_lod_tables_peon_footman() {
        if !world_root().exists() {
            crate::dev_log!("SKIP geoset_lod_tables: {} missing", world_root().display());
            return;
        }
        let models = [
            "Units\\Orc\\Peon\\peon.mdx",
            "Units\\Human\\Footman\\Footman.mdx",
            "Buildings\\Orc\\GreatHall\\GreatHall.mdx",
        ];
        // Run the audit twice: the extracted tree the asset gallery uses and
        // the exact archive the device installs load, so a version/LOD
        // difference between the two cannot hide.
        let mut measured = 0;
        for model in models {
            if let Ok(bytes) = std::fs::read(world_root().join(model.replace('\\', "/"))) {
                if geoset_table(&format!("tree:{model}"), &bytes).is_some() {
                    measured += 1;
                }
            }
        }
        assert_eq!(measured, models.len(), "every audited model parses");
        // Pin the defect this filter exists for: with NO running sequence the
        // runtime returns the raw static alphas, i.e. every geoset of the Peon
        // (body + pack + two carry variants + gore) is drawn at once. The Assets
        // layer resolves `None` to Stand so a unit without an animator entry
        // still shows one body.
        let Ok(bytes) = std::fs::read(world_root().join("Units/Orc/Peon/peon.mdx")) else {
            return;
        };
        let peon = MdxModel::parse(&bytes).expect("peon parses");
        let mut hidden = vec![false; peon.geosets.len()];
        for (index, geoset) in peon.geosets.iter().enumerate() {
            hidden[index] = wc3_anim::runtime::helper_plane(geoset);
        }
        let none = wc3_anim::runtime::mesh_alphas(&peon, &hidden, None, 0);
        let stand = peon
            .sequences
            .iter()
            .find(|sequence| sequence.name.eq_ignore_ascii_case("stand"))
            .cloned();
        let stood = wc3_anim::runtime::mesh_alphas(&peon, &hidden, stand.as_ref(), 0);
        assert_eq!(
            none,
            vec![1.0; peon.geosets.len()],
            "no-sequence static alphas show every Peon geoset"
        );
        assert_eq!(
            stood,
            vec![1.0, 1.0, 0.0, 0.0, 0.0],
            "Stand shows body + pack only"
        );
        let Some(mpq) = [
            "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
            "/data/data/org.wc3rust.p0/files/War3.mpq",
        ]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
        else {
            crate::dev_log!("SKIP GEOSET mpq audit: War3.mpq missing");
            return;
        };
        let mut storm = wc3_re::storm::StormFs::new();
        if storm.add_archive("War3", &mpq).is_err() {
            return;
        }
        for model in models {
            if let Ok(bytes) = storm.read(model) {
                geoset_table(&format!("mpq:{model}"), &bytes);
            }
        }
    }
}
