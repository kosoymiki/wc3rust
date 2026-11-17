//! Asset loading for the playable slice.
//!
//! Everything here is read from the *user's* Warcraft III installation at
//! runtime (`War3.mpq` + a map). No Blizzard art is compiled into the binary:
//! without an archive the loader builds small procedural meshes and a
//! procedural map so the game still runs, and reports `source=fallback`.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

use glam::{Vec2, Vec3};
use wc3_assetdb::{AssetDb, AssetSource, DirSource, MpqSource, SearchHit};
use wc3_assetdb::audio_provider::{
    start_game_audio, GameAudioWiring, SharedAssetDb, WiredGameAudio,
};
use wc3_formats::blp::BlpImage;
use wc3_re::mapfiles::MapBundle;
use wc3_formats::mdx::{Geoset, MdxModel, Sequence, Track};
use wc3_formats::slk::SlkTable;
use wc3_formats::w3e::{TILE_SIZE, Terrain, TerrainTile};
use wc3_sim::UnitKind;
use wc3_re::storm::StormFs;

use wc3_data::{GameDatabase, ObjectInfo, ObjectStats};

use crate::style::Chrome;
use crate::terrain_render::Biome;
use crate::ui::UiTheme;

pub const TERRAIN_TILE_SIZE: f32 = 128.0;
/// W3E heights are `(raw - 0x2000) / 512`, roughly -16..16. One cliff level is
/// 128 world units, so this scale makes the real map relief readable.
pub const TERRAIN_HEIGHT_SCALE: f32 = 128.0;
/// World-space resolution of the neutral detail texture (see
/// `terrain_render::DETAIL_SIZE`).
pub const TERRAIN_DETAIL_SIZE: u32 = 512;
pub const MODEL_KINDS: [UnitKind; 4] = [
    UnitKind::Footman,
    UnitKind::Archer,
    UnitKind::Peasant,
    UnitKind::TownHall,
];

const MODEL_PATHS: [&str; 4] = [
    "Units\\Human\\Footman\\Footman.mdx",
    "Units\\NightElf\\Archer\\Archer.mdx",
    "Units\\Human\\Peasant\\peasant.mdx",
    "Buildings\\Human\\TownHall\\TownHall.mdx",
];

/// Decoded RGBA8 texture, re-exported from the render crate.
///
/// The renderer's [`TextureImage`] is the same tight RGBA8 buffer, so the asset
/// layer hands it over without a conversion (and without a copy): the upload
/// path borrows the decoded sheet and the app drops it afterwards.
pub use wc3_render::texture::TextureImage;

/// Bytes released by [`Assets::release_cpu_textures`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuTextureRelease {
    /// Number of surfaces whose decoded RGBA was dropped.
    pub surfaces: usize,
    /// Bytes returned to the allocator.
    pub bytes: usize,
    /// Bytes the asset database's decoded caches accounted for (raw archive
    /// bytes, BLP sheets, parsed MDX). Those caches duplicate what the game
    /// layer already copied out of them.
    pub cache_bytes: usize,
    /// Bytes of mesh vertex data freed after the GPU upload.
    pub mesh_bytes: usize,
}

/// One material layer of a geoset.
///
/// WC3 materials are stacks: a base layer plus team-colour/glow/alpha layers
/// rendered in order over the same geometry. Rendering only the first layer
/// produced white team-colour pads instead of the textured body.
#[derive(Clone)]
pub struct MeshLayer {
    /// Index into [`Assets::textures`].
    pub texture: usize,
    /// Static layer alpha (`KMTA` fallback).
    pub alpha: f32,
    /// Animated alpha track, evaluated only when the track is a real variant
    /// of the current sequence (>= 2 differing keys inside the interval).
    pub alpha_track: Option<Track<f32>>,
    /// MDX filter mode: 0 none, 1 transparent (alpha test), 2 blend,
    /// 3/4 additive, 5/6 modulate.
    pub blend: u32,
    /// Replaceable/team-colour layer: the unit tint applies only here.
    pub team_color: bool,
    /// MDX texture-coordinate set this layer samples (0 or 1). The engine binds
    /// two texture units and sets `TEXCOORDINDEX := unit` per layer.
    pub uv_set: u32,
}

impl MeshLayer {
    pub fn plain(texture: usize) -> Self {
        Self {
            texture,
            alpha: 1.0,
            alpha_track: None,
            blend: 0,
            team_color: false,
            uv_set: 0,
        }
    }
}

/// One (bone, weight) pair of a skinned MDX vertex.
///
/// Canonical definition: `wc3_anim::runtime::VertexWeight` (one weight type for
/// assets, skinning and the GPU - no conversion at any boundary).
pub type BoneWeight = wc3_anim::runtime::VertexWeight;

#[derive(Clone, Default)]
pub struct MeshData {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<Vec2>,
    /// Second authored texture-coordinate set (`geoset.uv_sets[1]`), empty when
    /// the geoset ships one set.
    pub uvs2: Vec<Vec2>,
    pub indices: Vec<u32>,
    /// Weighted bone list per vertex. MDX matrix groups blend several bones;
    /// the SKIN chunk supplies explicit weights, otherwise equal weights are
    /// used. Honouring the weights is what keeps limbs from shearing.
    pub bones: Vec<Vec<BoneWeight>>,
    /// Material layer stack, in render order.
    pub layers: Vec<MeshLayer>,
    /// Primary texture (first layer) for diagnostics.
    pub texture: usize,
    /// Texture paths of every layer (lowercase, map-style separators). Used to
    /// keep corpse/effect geosets out of Stand/Walk/Attack.
    pub texture_paths: Vec<String>,
    /// MDX vertices/nodes are authored in Z-up space; the renderer is Y-up.
    pub z_up: bool,
    /// Index of the MDX geoset this mesh was built from.
    pub geoset_id: u32,
    /// Helper plane (huge 1–2 quad bind-pose geometry) that the game never
    /// displays in Stand; it was the white pyramid in the first builds.
    pub hide: bool,
    /// Vertex data in the runtime format (see [`CompactMesh`]). Present after
    /// [`MeshData::compact`]; the f32 fields above are then empty.
    pub compact: Option<CompactMesh>,
}

/// The runtime vertex format: 20 bytes per vertex instead of ~52.
///
/// Model-space bounds are known when the geoset is built, so positions fit in
/// `i16` (0.005 units of precision on a 300-unit model), normals are unit
/// vectors and fit in two `i8` octahedral components (~1.5° error), and UVs are
/// `u16` fixed point over their own min/max range. Bone influences keep the
/// four heaviest `(bone u8, weight u8)` pairs the source vertex has, and the
/// weight bytes of a weighted vertex sum to 255, so unpacking returns a list
/// that renormalises to exactly 1 (ledger 451: the old two-pair packing dropped
/// the top influences of multi-matrix vertices). A vertex whose matrix group
/// references a bone index above 255 (never on the shipped install) is kept in
/// `bones_extra` with the full `u16` index instead of a wrapped byte.
#[derive(Clone, Debug, Default)]
pub struct CompactMesh {
    /// Model-space min/max used by the `i16` position encoding.
    pub bounds: (Vec3, Vec3),
    pub positions: Vec<[i16; 3]>,
    pub normals: Vec<[i8; 2]>,
    /// UV min/max the `u16` coordinates are relative to.
    pub uv_bounds: (Vec2, Vec2),
    pub uvs: Vec<[u16; 2]>,
    /// Second UV set; empty when the geoset ships one set.
    pub uv2_bounds: (Vec2, Vec2),
    pub uvs2: Vec<[u16; 2]>,
    /// Index buffer: `u16` when the geoset fits 65 536 vertices (every WC3
    /// geoset does), `u32` otherwise.
    pub indices16: Vec<u16>,
    pub indices32: Vec<u32>,
    /// Four bone influences per vertex: `(bone << 8) | weight`, heaviest
    /// first. A zero weight byte marks an empty slot; the live weight bytes of
    /// one vertex sum to 255, so the decoded weights sum to 1.
    pub influences: Vec<[u16; 4]>,
    /// Vertices whose matrix group references a bone index above `u8`.
    pub bones_extra: Vec<(u32, [BoneWeight; 4], u8)>,
    pub vertex_count: u32,
}

/// Bytes one compact vertex record occupies: `i16` position (6), two `i8`
/// octahedral normal (2), `u16` UV pair (4), four packed bone influences (8).
pub const COMPACT_VERTEX_BYTES: usize = 6 + 2 + 4 + 8;
/// Extra bytes per vertex when the geoset ships a second UV set.
pub const COMPACT_UV2_BYTES: usize = 4;

impl CompactMesh {
    pub fn bytes(&self) -> usize {
        let uv2 = if self.uvs2.is_empty() {
            0
        } else {
            COMPACT_UV2_BYTES
        };
        self.positions.len() * (COMPACT_VERTEX_BYTES + uv2)
            + self.indices16.len() * std::mem::size_of::<u16>()
            + self.indices32.len() * std::mem::size_of::<u32>()
            + self.bones_extra.len() * std::mem::size_of::<(u32, [BoneWeight; 4], u8)>()
    }

    /// Runtime index at `slot`, whatever the storage width.
    pub fn index(&self, slot: usize) -> u32 {
        if let Some(&index) = self.indices16.get(slot) {
            return index as u32;
        }
        self.indices32.get(slot).copied().unwrap_or(0)
    }

    /// Number of indices in the runtime index buffer.
    pub fn index_count(&self) -> usize {
        self.indices16.len() + self.indices32.len()
    }

    pub fn position(&self, index: usize) -> Vec3 {
        let Some(&[x, y, z]) = self.positions.get(index) else {
            return Vec3::ZERO;
        };
        let (min, max) = self.bounds;
        let scale = 1.0 / i16::MAX as f32;
        Vec3::new(
            min.x + (max.x - min.x) * (x as f32 * scale * 0.5 + 0.5),
            min.y + (max.y - min.y) * (y as f32 * scale * 0.5 + 0.5),
            min.z + (max.z - min.z) * (z as f32 * scale * 0.5 + 0.5),
        )
    }

    pub fn normal(&self, index: usize) -> Vec3 {
        let Some(&[x, y]) = self.normals.get(index) else {
            return Vec3::Y;
        };
        decode_octahedral(x, y)
    }

    pub fn uv(&self, index: usize) -> Vec2 {
        let Some(&[u, v]) = self.uvs.get(index) else {
            return Vec2::ZERO;
        };
        decode_uv(u, v, self.uv_bounds)
    }

    pub fn uv2(&self, index: usize) -> Option<Vec2> {
        let &[u, v] = self.uvs2.get(index)?;
        Some(decode_uv(u, v, self.uv2_bounds))
    }

    /// Bone influences of one vertex: the list `expanded()` hands to the GPU
    /// upload. At most four pairs, heaviest first, weights renormalised to 1.
    pub fn bones(&self, index: usize) -> Vec<BoneWeight> {
        if let Some((_, list, count)) = self
            .bones_extra
            .iter()
            .find(|(vertex, _, _)| *vertex as usize == index)
        {
            return list[..*count as usize].to_vec();
        }
        let Some(slots) = self.influences.get(index) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(4);
        let mut total = 0.0f32;
        for &slot in slots {
            let weight = (slot & 0xFF) as f32 / 255.0;
            if weight > 0.0 {
                out.push(BoneWeight {
                    bone: slot >> 8,
                    weight,
                });
                total += weight;
            }
        }
        if total > 0.0 && (total - 1.0).abs() > f32::EPSILON {
            for pair in &mut out {
                pair.weight /= total;
            }
        }
        out
    }
}

/// Quantise one vertex's normalised weights into four `u8` slots that sum to
/// exactly 255 (`u8::MAX`).
///
/// Largest-remainder allocation: floor every exact value, then hand the missing
/// units to the largest fractional parts. Each byte stays within one 1/255 step
/// of its authored weight, the decoded list renormalises to 1 with no residual,
/// and a live influence never rounds away to a zero (the empty-slot marker).
fn quantise_weights(weights: &[BoneWeight]) -> [u8; 4] {
    let mut exact = [0f32; 4];
    let mut units = [0i32; 4];
    let mut sum = 0i32;
    for (slot, pair) in weights.iter().take(4).enumerate() {
        exact[slot] = (pair.weight.clamp(0.0, 1.0) * 255.0).max(0.0);
        units[slot] = exact[slot].floor() as i32;
        sum += units[slot];
    }
    let mut deficit = 255 - sum;
    while deficit > 0 {
        let slot = (0..4)
            .filter(|slot| exact[*slot] > 0.0)
            .max_by(|a, b| {
                (exact[*a] - units[*a] as f32).total_cmp(&(exact[*b] - units[*b] as f32))
            })
            .unwrap_or(0);
        units[slot] += 1;
        deficit -= 1;
    }
    // A positive but tiny weight must not become the "empty slot" marker.
    for slot in 0..4 {
        if exact[slot] > 0.0 && units[slot] == 0 {
            let donor = (0..4).max_by_key(|slot| units[*slot]).unwrap_or(0);
            if units[donor] > 1 {
                units[donor] -= 1;
                units[slot] = 1;
            }
        }
    }
    [
        units[0].clamp(0, 255) as u8,
        units[1].clamp(0, 255) as u8,
        units[2].clamp(0, 255) as u8,
        units[3].clamp(0, 255) as u8,
    ]
}

fn encode_uv(value: Vec2, bounds: (Vec2, Vec2)) -> [u16; 2] {
    let span = (bounds.1 - bounds.0).max(Vec2::splat(1e-6));
    let u = ((value.x - bounds.0.x) / span.x).clamp(0.0, 1.0);
    let v = ((value.y - bounds.0.y) / span.y).clamp(0.0, 1.0);
    [
        (u * u16::MAX as f32).round() as u16,
        (v * u16::MAX as f32).round() as u16,
    ]
}

fn decode_uv(u: u16, v: u16, bounds: (Vec2, Vec2)) -> Vec2 {
    let span = bounds.1 - bounds.0;
    Vec2::new(
        bounds.0.x + span.x * (u as f32 / u16::MAX as f32),
        bounds.0.y + span.y * (v as f32 / u16::MAX as f32),
    )
}

/// Octahedral normal encoding: two `i8` for a unit vector.
fn encode_octahedral(normal: Vec3) -> [i8; 2] {
    let n = if normal.length_squared() > 0.0 {
        normal.normalize()
    } else {
        Vec3::Y
    };
    let l1 = n.x.abs() + n.y.abs() + n.z.abs();
    let (mut x, mut y) = if l1 <= 0.0 {
        (0.0, 0.0)
    } else {
        (n.x / l1, n.y / l1)
    };
    if n.z < 0.0 {
        let px = (1.0 - y.abs()) * if x >= 0.0 { 1.0 } else { -1.0 };
        let py = (1.0 - x.abs()) * if y >= 0.0 { 1.0 } else { -1.0 };
        x = px;
        y = py;
    }
    [
        (x.clamp(-1.0, 1.0) * 127.0).round() as i8,
        (y.clamp(-1.0, 1.0) * 127.0).round() as i8,
    ]
}

fn decode_octahedral(x: i8, y: i8) -> Vec3 {
    let x = (x as f32 / 127.0).clamp(-1.0, 1.0);
    let y = (y as f32 / 127.0).clamp(-1.0, 1.0);
    let mut n = Vec3::new(x, y, 1.0 - x.abs() - y.abs());
    if n.z < 0.0 {
        let old_x = n.x;
        n.x = (1.0 - n.y.abs()) * if old_x >= 0.0 { 1.0 } else { -1.0 };
        n.y = (1.0 - old_x.abs()) * if n.y >= 0.0 { 1.0 } else { -1.0 };
    }
    n.normalize_or_zero()
}

impl MeshData {
    /// Bytes this mesh holds on the CPU, in the runtime format.
    ///
    /// This is the number `MEMORY_BUDGET.md` step 2 caps: positions, normals,
    /// UVs, indices, bone weights and the material layer stack.
    pub fn cpu_bytes(&self) -> usize {
        if let Some(compact) = self.compact.as_ref() {
            return compact.bytes()
                + self.layers.capacity() * std::mem::size_of::<MeshLayer>()
                + self.texture_paths.iter().map(|path| path.capacity()).sum::<usize>();
        }
        let vertices = self.positions.len();
        vertices * (std::mem::size_of::<Vec3>() * 2 + std::mem::size_of::<Vec2>())
            + self.uvs2.len() * std::mem::size_of::<Vec2>()
            + self.indices.len() * std::mem::size_of::<u32>()
            + self
                .bones
                .iter()
                .map(|list| list.capacity() * std::mem::size_of::<BoneWeight>())
                .sum::<usize>()
            + self.layers.capacity() * std::mem::size_of::<MeshLayer>()
            + self
                .texture_paths
                .iter()
                .map(|path| path.capacity())
                .sum::<usize>()
    }

    /// Vertex count, whether the mesh is expanded or compact.
    pub fn vertex_count(&self) -> usize {
        match self.compact.as_ref() {
            Some(compact) => compact.vertex_count as usize,
            None => self.positions.len(),
        }
    }

    /// Triangle count of the runtime index buffer.
    pub fn triangle_count(&self) -> usize {
        match self.compact.as_ref() {
            Some(compact) => compact.index_count() / 3,
            None => self.indices.len() / 3,
        }
    }

    /// Bytes of one vertex record (the format the budget caps).
    ///
    /// Compact: `i16` position 6 + two `i8` normals 2 + `u16` UV pair 4 +
    /// four packed influences 8 = 20, plus 4 when a second UV set is authored.
    /// Expanded (f32): positions 12 + normals 12 + UV 8 (+8) = 32.
    pub fn vertex_record_bytes(&self) -> usize {
        match self.compact.as_ref() {
            Some(compact) => {
                COMPACT_VERTEX_BYTES
                    + if compact.uvs2.is_empty() {
                        0
                    } else {
                        COMPACT_UV2_BYTES
                    }
            }
            None => {
                std::mem::size_of::<Vec3>() * 2
                    + std::mem::size_of::<Vec2>()
                    + if self.uvs2.is_empty() {
                        0
                    } else {
                        std::mem::size_of::<Vec2>()
                    }
            }
        }
    }

    /// Bytes per vertex including the index buffer and the material metadata.
    pub fn bytes_per_vertex(&self) -> f64 {
        let vertices = self.vertex_count();
        if vertices == 0 {
            0.0
        } else {
            self.cpu_bytes() as f64 / vertices as f64
        }
    }

    /// Convert the f32 vertex data into [`CompactMesh`] and drop the originals.
    ///
    /// Called once the mesh stops being edited (after the loader computed the
    /// model height/pivot and the pose alphas). Everything the animator and the
    /// material path still need — layers, geoset id, hide flag — stays.
    pub fn compact(&mut self) {
        if self.compact.is_some() || self.positions.is_empty() {
            return;
        }
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for position in &self.positions {
            min = min.min(*position);
            max = max.max(*position);
        }
        if !min.is_finite() || !max.is_finite() {
            min = Vec3::ZERO;
            max = Vec3::ONE;
        }
        // Degenerate extents would divide by zero in the decoder.
        let span = (max - min).max(Vec3::splat(1e-4));
        let max = min + span;

        let mut uv_min = Vec2::splat(f32::INFINITY);
        let mut uv_max = Vec2::splat(f32::NEG_INFINITY);
        for uv in &self.uvs {
            uv_min = uv_min.min(*uv);
            uv_max = uv_max.max(*uv);
        }
        if !uv_min.is_finite() || !uv_max.is_finite() {
            uv_min = Vec2::ZERO;
            uv_max = Vec2::ONE;
        }
        let uv_bounds = (uv_min, uv_min + (uv_max - uv_min).max(Vec2::splat(1e-6)));
        let mut uv2_bounds = uv_bounds;
        if !self.uvs2.is_empty() {
            let mut min2 = Vec2::splat(f32::INFINITY);
            let mut max2 = Vec2::splat(f32::NEG_INFINITY);
            for uv in &self.uvs2 {
                min2 = min2.min(*uv);
                max2 = max2.max(*uv);
            }
            if min2.is_finite() && max2.is_finite() {
                uv2_bounds = (min2, min2 + (max2 - min2).max(Vec2::splat(1e-6)));
            }
        }

        let vertex_count = self.positions.len();
        let mut positions = Vec::with_capacity(vertex_count);
        for position in &self.positions {
            let nx = (position.x - min.x) / (max.x - min.x);
            let ny = (position.y - min.y) / (max.y - min.y);
            let nz = (position.z - min.z) / (max.z - min.z);
            positions.push([
                ((nx * 2.0 - 1.0) * i16::MAX as f32).round() as i16,
                ((ny * 2.0 - 1.0) * i16::MAX as f32).round() as i16,
                ((nz * 2.0 - 1.0) * i16::MAX as f32).round() as i16,
            ]);
        }
        let normals: Vec<[i8; 2]> = self
            .normals
            .iter()
            .map(|normal| encode_octahedral(*normal))
            .collect();
        let uvs: Vec<[u16; 2]> = self
            .uvs
            .iter()
            .map(|uv| encode_uv(*uv, uv_bounds))
            .collect();
        let uvs2: Vec<[u16; 2]> = self
            .uvs2
            .iter()
            .map(|uv| encode_uv(*uv, uv2_bounds))
            .collect();

        let mut influences = Vec::with_capacity(vertex_count);
        let mut bones_extra = Vec::new();
        for (vertex, list) in self.bones.iter().enumerate() {
            let mut list: Vec<BoneWeight> = list
                .iter()
                .copied()
                .filter(|pair| pair.weight > 0.0 && pair.weight.is_finite())
                .collect();
            // Keep the four heaviest influences (the GPU vertex format's slots)
            // and renormalise what survives: dropping a fifth-smallest pair
            // must not scale the kept weights below 1.
            list.sort_by(|a, b| b.weight.total_cmp(&a.weight));
            list.truncate(4);
            let total: f32 = list.iter().map(|pair| pair.weight).sum();
            if total > 0.0 {
                for pair in &mut list {
                    pair.weight /= total;
                }
            }
            if list.iter().any(|pair| pair.bone > u8::MAX as u16) {
                // Bone tables are far below 256 entries on the shipped install;
                // this side table only exists so a wider index can never wrap.
                let mut extra = [BoneWeight {
                    bone: 0,
                    weight: 0.0,
                }; 4];
                for (slot, pair) in list.iter().enumerate() {
                    extra[slot] = *pair;
                }
                bones_extra.push((vertex as u32, extra, list.len() as u8));
                influences.push([0u16; 4]);
                continue;
            }
            // 8-bit weights allocated by largest remainder: the live bytes of
            // a weighted vertex sum to exactly 255, so unpacking renormalises
            // to 1 (the old 5-bit packing left a 0.032 residual) and every
            // byte stays within 1/255 of its authored value.
            let bytes = if list.is_empty() {
                [0u8; 4]
            } else {
                quantise_weights(&list)
            };
            let mut slots = [0u16; 4];
            for (slot, pair) in list.iter().enumerate() {
                slots[slot] = ((pair.bone as u16) << 8) | bytes[slot] as u16;
            }
            influences.push(slots);
        }

        // `u16` indices when the geoset fits the range: WC3 geosets are at most
        // a few thousand vertices, so this halves the index buffer.
        let source_indices = std::mem::take(&mut self.indices);
        let (indices16, indices32) = if vertex_count <= u16::MAX as usize
            && source_indices.iter().all(|&index| index <= u16::MAX as u32)
        {
            (
                source_indices.iter().map(|&index| index as u16).collect(),
                Vec::new(),
            )
        } else {
            (Vec::new(), source_indices)
        };
        self.compact = Some(CompactMesh {
            bounds: (min, max),
            positions,
            normals,
            uv_bounds,
            uvs,
            uv2_bounds,
            uvs2,
            indices16,
            indices32,
            influences,
            bones_extra,
            vertex_count: vertex_count as u32,
        });
        // The f32 copies are gone: everything downstream reads the compact
        // buffer through the accessors.
        self.positions = Vec::new();
        self.normals = Vec::new();
        self.uvs = Vec::new();
        self.uvs2 = Vec::new();
        self.bones = Vec::new();
    }

    /// Expanded vertex data (allocates when the mesh is compact).
    ///
    /// The renderer and the pose path need f32 vertices exactly once per mesh;
    /// every other reader wants counts and metadata only.
    pub fn expanded(&self) -> ExpandedMesh {
        if let Some(compact) = self.compact.as_ref() {
            let count = compact.vertex_count as usize;
            let mut positions = Vec::with_capacity(count);
            let mut normals = Vec::with_capacity(count);
            let mut uvs = Vec::with_capacity(count);
            let mut uvs2 = Vec::with_capacity(compact.uvs2.len());
            let mut bones = Vec::with_capacity(compact.influences.len());
            for index in 0..count {
                positions.push(compact.position(index));
                normals.push(compact.normal(index));
                uvs.push(compact.uv(index));
                bones.push(compact.bones(index));
            }
            for index in 0..compact.uvs2.len() {
                uvs2.push(compact.uv2(index).unwrap_or(Vec2::ZERO));
            }
            let mut indices = Vec::with_capacity(compact.index_count());
            for slot in 0..compact.index_count() {
                indices.push(compact.index(slot));
            }
            return ExpandedMesh {
                positions,
                normals,
                uvs,
                uvs2,
                indices,
                bones,
            };
        }
        ExpandedMesh {
            positions: self.positions.clone(),
            normals: self.normals.clone(),
            uvs: self.uvs.clone(),
            uvs2: self.uvs2.clone(),
            indices: self.indices.clone(),
            bones: self.bones.clone(),
        }
    }

    /// Replace the vertex data with a posed copy (Z-up skinning at load time).
    ///
    /// The pose path works on an expanded copy and hands the result back here;
    /// the mesh stays compact afterwards so the runtime budget holds.
    pub fn pose(&mut self, positions: Vec<Vec3>, normals: Vec<Vec3>, bones: Vec<Vec<BoneWeight>>) {
        self.compact = None;
        self.positions = positions;
        self.normals = normals;
        self.bones = bones;
        self.compact();
    }

    /// Drop every vertex buffer now that the GPU owns them.
    ///
    /// Returns the bytes freed. The mesh keeps its material layers, geoset id
    /// and hide flag: those are what the per-frame animation path reads.
    pub fn release_vertices(&mut self) -> usize {
        let freed = if let Some(compact) = self.compact.take() {
            compact.bytes()
        } else {
            self.positions.len() * std::mem::size_of::<Vec3>()
                + self.normals.len() * std::mem::size_of::<Vec3>()
                + self.uvs.len() * std::mem::size_of::<Vec2>()
                + self.uvs2.len() * std::mem::size_of::<Vec2>()
                + self.indices.len() * std::mem::size_of::<u32>()
                + self
                    .bones
                    .iter()
                    .map(|list| list.len() * std::mem::size_of::<BoneWeight>())
                    .sum::<usize>()
        };
        self.positions = Vec::new();
        self.normals = Vec::new();
        self.uvs = Vec::new();
        self.uvs2 = Vec::new();
        self.indices = Vec::new();
        self.bones = Vec::new();
        freed
    }

    /// `true` when the vertex data has been handed to the GPU and dropped.
    pub fn vertices_released(&self) -> bool {
        self.compact.is_none() && self.positions.is_empty()
    }

    /// Death/effect geosets that must not show while a unit is alive, even if
    /// the map's GEOA data is missing or malformed.
    pub fn is_corpse_or_effect(&self) -> bool {
        self.texture_paths.iter().any(|path| {
            let path = path.to_ascii_lowercase();
            ["gutz", "guts", "corpse", "death", "skeleton"]
                .iter()
                .any(|needle| path.contains(needle))
        })
    }
}

/// Expanded (f32) vertex data of a mesh, in render vertex order.
#[derive(Clone, Debug, Default)]
pub struct ExpandedMesh {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<Vec2>,
    pub uvs2: Vec<Vec2>,
    pub indices: Vec<u32>,
    pub bones: Vec<Vec<BoneWeight>>,
}

pub struct ModelAsset {    pub kind: UnitKind,
    pub model: Option<MdxModel>,
    pub meshes: Vec<MeshData>,
    pub sequences: Vec<Sequence>,
    pub idle: Option<usize>,
    pub walk: Option<usize>,
    pub attack: Option<usize>,
    /// Model height in world units after the Z-up conversion.
    pub height: f32,
    /// Model-space base offset (`min z` of the visible Stand geosets, already
    /// scaled). The renderer places the unit at `height_at(x, z) - pivot`, so a
    /// building whose authored base dips below the origin sits ON the terrain
    /// instead of sinking into it.
    pub pivot: f32,
    /// Presentation scale (Warcraft 3 `Art - Scaling Value`). 1.0 keeps the
    /// MDX authoring size.
    pub scale: f32,
    /// Model-space yaw correction. Some WC3 models (Archer) are authored with
    /// the back along +X, so the game applies a 180° offset.
    pub yaw_offset: f32,
    pub fallback: bool,
}

impl ModelAsset {
    pub fn sequence(&self, wanted: SequenceKind) -> Option<usize> {
        self.sequence_with(wanted, Carry::None)
    }

    /// Whether any mesh carries bone weights.
    ///
    /// A skinned model needs the animator's node palette; a static model (the
    /// gold mine's `ngol`, a placement ghost) is drawn with identity transforms
    /// and legitimately has an empty node array. Reads the compact vertex format
    /// when present, because `compact_meshes` moves the influences there.
    pub fn is_skinned(&self) -> bool {
        self.meshes.iter().any(|mesh| {
            if let Some(compact) = mesh.compact.as_ref() {
                return compact
                    .influences
                    .iter()
                    .any(|slots| slots.iter().any(|slot| slot & 0xFF != 0));
            }
            mesh.bones.iter().any(|bones| !bones.is_empty())
        })
    }

    /// Carry-aware sequence pick: `Walk Gold` for a peon hauling gold, `Walk`
    /// when empty. Falls back to the plain variant when the model has no carry
    /// set (soldiers, buildings), so behaviour is unchanged for every non-worker.
    pub fn sequence_with(&self, wanted: SequenceKind, carry: Carry) -> Option<usize> {
        let suffix = match carry {
            Carry::None => None,
            Carry::Gold => Some("gold"),
            Carry::Lumber => Some("lumber"),
        };
        // Shortest matching name wins: `Stand Gold` must beat `stand work gold`
        // and `Walk Gold` must beat `Stand Ready Gold` for the Walk lookup. The
        // model lists carry variants in no useful order (Peon.mdx puts
        // `stand work gold` at index 5 and `Stand Gold` at 18).
        let pick_exact = |name: &str, with: Option<&str>| {
            self.sequences
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    let lower = s.name.to_ascii_lowercase();
                    match with {
                        Some(suffix) => lower.contains(name) && lower.contains(suffix),
                        None => lower.contains(name),
                    }
                })
                .min_by_key(|(_, s)| s.name.len())
                .map(|(index, _)| index)
        };
        // Carrier first, plain variant second: never show "Walk" while the unit
        // visually hauls a sack, and never demand a variant a model lacks.
        let pick = |name: &str| {
            suffix
                .and_then(|suffix| pick_exact(name, Some(suffix)))
                .or_else(|| pick_exact(name, None))
        };
        match wanted {
            // No silent index-0 fallback: on many MDX files sequence 0 is
            // Birth/Death, so playing it as Idle looked like a corpse animation.
            // Carry variants first: the sack is the carry geoset of
            // `Stand Gold` / `Walk Gold` / `attack gold`, so a worker hauling a
            // load must never fall back to the plain sequence.
            SequenceKind::Idle => suffix
                .and_then(|suffix| pick_exact("stand", Some(suffix)))
                .or(self.idle)
                .or_else(|| pick_exact("stand", None)),
            SequenceKind::Walk => suffix
                .and_then(|suffix| pick_exact("walk", Some(suffix)))
                .or(self.walk)
                .or_else(|| pick_exact("walk", None)),
            SequenceKind::Attack => suffix
                .and_then(|suffix| pick_exact("attack", Some(suffix)))
                .or(self.attack)
                .or_else(|| pick_exact("attack", None)),
            SequenceKind::Death => self
                .sequences
                .iter()
                .position(|sequence| {
                    sequence.name.to_ascii_lowercase().contains("death")
                }),
            SequenceKind::Birth => self
                .sequences
                .iter()
                .position(|sequence| {
                    sequence.name.to_ascii_lowercase().contains("birth")
                })
                .or(self.idle),
            SequenceKind::Build => pick_exact("stand work", None)
                .or_else(|| pick_exact("build", None))
                .or(self.attack),
            // Gathering at the resource: WC3 plays `stand work`/`attack gold`
            // with the carry variant, which is also what shows the sack.
            SequenceKind::Work => suffix
                .and_then(|suffix| pick_exact("stand work", Some(suffix)))
                .or_else(|| suffix.and_then(|suffix| pick_exact("attack", Some(suffix))))
                .or_else(|| pick_exact("stand work", None))
                .or_else(|| pick_exact("attack", None))
                .or(self.idle),
        }
        .filter(|index| *index < self.sequences.len().max(1))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SequenceKind {
    Idle,
    Walk,
    Attack,
    /// Worker channel: build/repair (`stand work` / `build` sequences).
    Build,
    /// Building under construction: the model's Birth sequence.
    Birth,
    /// Corpse animation for the short death window after a unit dies.
    Death,
    /// Worker gathering at the resource (`stand work`, `attack gold`).
    Work,
}

/// Which resource a worker is hauling. WC3 ships dedicated sequences for each
/// (`Walk Gold`, `Walk Lumber`, `Stand Gold`, `attack gold`, `stand work gold`,
/// measured on `units\orc\Peon\Peon.mdx`: 22 sequences incl. all six carry
/// variants and `Stand Ready Gold/Lumber`), and the carry geoset — the gold
/// sack — is only visible in those variants.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Carry {
    #[default]
    None,
    Gold,
    Lumber,
}

/// How [`Assets::load`] decided which per-definition MDX files to parse.
///
/// The roster carries 814 unit/building definitions, and parsing all of them
/// kept 169.8 MB of `MdxModel` (bones, node tracks, sequences, materials) alive
/// for a map that shows a fraction of that (`mem_probe --breakdown`). A map
/// scope is the reachable-definition set of one map; the full roster remains
/// the correctness fallback for a map the loader cannot scope (no placements,
/// or a script that creates units through variables).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModelScopeMode {
    /// Every definition in the object database is parsed.
    #[default]
    FullRoster,
    /// Only the definitions the current map can show.
    MapScoped,
}

impl ModelScopeMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::FullRoster => "full",
            Self::MapScoped => "map",
        }
    }
}

/// The reachable-definition set of one map, computed before any MDX is parsed.
///
/// Sources, in the order the scene can show them:
/// 1. every `war3mapUnits.doo` placement (`sloc`/`ngol`/`ugol` are markers, not
///    units) — creeps, shops, halls and workers placed by the author;
/// 2. the tech-tree opening of each seat's race: the hall, the worker, the
///    worker's `Builds=` list, then a fixed point over `Trains=`;
/// 3. the unit ids the map's `war3map.j` creates with a literal FourCC
///    (`CreateUnit(Player(0), 'hfoo', …)`), which is how campaign maps place
///    script-owned units.
///
/// Destructable/doodad art is not part of this set: `DoodadWorld::load` resolves
/// it lazily from `war3map.doo` per instance, so it is map-scoped already.
#[derive(Debug, Clone, Default)]
pub struct ModelScope {
    pub mode: ModelScopeMode,
    /// Why that mode was chosen (`placements+tech`, `campaign-script`, …).
    pub reason: String,
    /// Reachable def ids (lower-case FourCC, as the object database stores them).
    pub ids: BTreeSet<String>,
    /// Ids seeded by `war3mapUnits.doo` placements.
    pub placed: usize,
    /// Ids seeded by `war3map.doo` destructables/doodads (their MDX is
    /// resolved per instance by `DoodadWorld`, but they are part of the map's
    /// reachable content and listed here).
    pub doodads: usize,
    /// Ids seeded by the map script's literal `CreateUnit` calls.
    pub script: usize,
    /// Ids seeded by the seats' tech-tree closure.
    pub tech: usize,
    /// Races of the map's seats, as parsed from `war3map.w3i` and placed halls.
    pub races: Vec<String>,
    /// Unit-creating script call sites.
    pub script_calls: usize,
    /// How many of those carried a literal FourCC.
    pub script_literals: usize,
}

impl ModelScope {
    /// Whether a definition's MDX belongs to this load.
    pub fn wants(&self, id: &str) -> bool {
        self.mode == ModelScopeMode::FullRoster || self.ids.contains(id)
    }

    /// One-line diagnostic (`GAME_MODEL_SCOPE …`).
    pub fn summary(&self) -> String {
        format!(
            "mode={} reason={} ids={} placed={} doodads={} script={} tech={} races={:?} script_calls={} script_literals={}",
            self.mode.label(),
            self.reason,
            self.ids.len(),
            self.placed,
            self.doodads,
            self.script,
            self.tech,
            self.races,
            self.script_calls,
            self.script_literals
        )
    }
}

/// The defs a map can actually show.
///
/// `None` (no map) keeps the full roster: without a map there is no placement
/// or seat information to scope against, and the four-kind demo scene must keep
/// working. A map whose script creates units through variables also keeps the
/// full roster — a static scan cannot prove the reachable set there, and a
/// missing model is a visual bug while a parsed one is only bytes.
fn map_model_scope(map: Option<&MapBundle>) -> ModelScope {
    // A/B switch for the memory gate: `H2_MODEL_SCOPE=full` reproduces the
    // pre-residency behaviour (whole roster) for a same-binary before/after.
    if std::env::var("H2_MODEL_SCOPE")
        .map(|value| value.eq_ignore_ascii_case("full"))
        .unwrap_or(false)
    {
        return ModelScope {
            mode: ModelScopeMode::FullRoster,
            reason: "env-full".to_string(),
            ..ModelScope::default()
        };
    }
    let Some(bundle) = map else {
        return ModelScope {
            mode: ModelScopeMode::FullRoster,
            reason: "no-map".to_string(),
            ..ModelScope::default()
        };
    };
    let mut scope = ModelScope::default();
    let mut races: Vec<wc3_sim::data::Race> = Vec::new();
    if let Some(units) = bundle.units.as_ref() {
        for unit in units {
            let id = String::from_utf8_lossy(&unit.type_id).into_owned();
            if id == "sloc" || id == "ngol" || id == "ugol" || id.len() != 4 {
                continue;
            }
            if let Some(def) = wc3_sim::data::def_of(&id)
                && def.main_hall
                && !races.contains(&def.race)
            {
                races.push(def.race);
            }
            scope.ids.insert(id);
        }
        scope.placed = scope.ids.len();
    }
    for slot in crate::sim_bridge::map_player_slots(bundle) {
        if !races.contains(&slot.race) {
            races.push(slot.race);
        }
    }
    // Campaign maps own units through their script: `CreateUnit(p, 'hfoo', …)`
    // in the `CreateUnitsForPlayer` functions. Literal ids join the scope.
    if let Some(script) = bundle.script.as_ref() {
        let (ids, calls, literals) = script_created_ids(script);
        scope.script_calls = calls;
        scope.script_literals = literals;
        scope.script = ids.len();
        scope.ids.extend(ids);
    }
    let before = scope.ids.len();
    add_seat_tech(&mut scope.ids, &races);
    scope.tech = scope.ids.len().saturating_sub(before);
    // `war3map.doo` destructables/doodads are map content too. Their art is
    // resolved lazily per instance by `DoodadWorld::load` (it reads the same
    // `war3map.doo`), so they only join the id set — that keeps one list of
    // everything the map can show, and a doodad id that is also a unit def
    // still gets its unit model.
    let before_doodads = scope.ids.len();
    if let Some(doodads) = bundle.doodads.as_ref() {
        for doodad in doodads {
            let id = String::from_utf8_lossy(&doodad.type_id).into_owned();
            if id.len() == 4 {
                scope.ids.insert(id);
            }
        }
    }
    scope.doodads = scope.ids.len().saturating_sub(before_doodads);
    scope.races = races.iter().map(|race| format!("{race:?}")).collect();
    let definitions = scope
        .ids
        .iter()
        .filter(|id| wc3_sim::data::def_of(id).is_some())
        .count();
    let unresolved = scope.script_calls.saturating_sub(scope.script_literals);
    if definitions == 0 {
        scope.mode = ModelScopeMode::FullRoster;
        scope.reason = "no-placements".to_string();
    } else if unresolved > 0 {
        // `CreateUnit(u, GetUnitTypeId(someUnit), …)` / `CreateUnitByName`:
        // the created def is only known at runtime.
        scope.mode = ModelScopeMode::FullRoster;
        scope.reason = format!("campaign-script:{unresolved}-variable-ids");
    } else {
        scope.mode = ModelScopeMode::MapScoped;
        scope.reason = if scope.script > 0 {
            "placements+script+tech".to_string()
        } else {
            "placements+tech".to_string()
        };
    }
    scope
}

/// Add the seat tech-tree opening: hall, worker, the worker's `Builds=` list
/// and a fixed point over every reachable producer's `Trains=`.
///
/// The worker build list is the race's placeable buildings (`ROSTER` rows with
/// `buildable`, the same filter as `wc3_sim::data::build_menu`), so a player can
/// build anything the data allows and its model is already resident. A hero's
/// summon abilities are deliberately not expanded: a summon that shows up gets
/// the bounded placeholder instead of pulling a whole school of models.
fn add_seat_tech(ids: &mut BTreeSet<String>, races: &[wc3_sim::data::Race]) {
    for race in races {
        ids.insert(wc3_sim::ai::hall_for_race(*race).to_string());
        ids.insert(wc3_sim::ai::worker_for_race(*race).to_string());
    }
    for def in wc3_sim::data::ROSTER {
        if races.contains(&def.race) && def.is_building() && def.buildable {
            ids.insert(def.id.to_string());
        }
    }
    // Fixed point over `Trains=`: hall -> worker -> producer -> trained unit.
    for _ in 0..6 {
        let snapshot: Vec<String> = ids.iter().cloned().collect();
        let mut added = 0usize;
        for id in &snapshot {
            let Some(def) = wc3_sim::data::def_of(id) else {
                continue;
            };
            for trained in def.trains {
                if ids.insert((*trained).to_string()) {
                    added += 1;
                }
            }
        }
        if added == 0 {
            break;
        }
    }
}

/// Unit type ids a map script creates with a literal FourCC.
///
/// Returns `(ids, call_sites, literal_sites)`. `call_sites > literal_sites`
/// means the script creates units through variables, which a static scan cannot
/// resolve — the caller then keeps the full roster. Bounded per call site: the
/// id has to appear inside the argument list, not somewhere later in the file.
fn script_created_ids(script: &str) -> (BTreeSet<String>, usize, usize) {
    const CREATORS: [&str; 3] = ["createunit", "createnunits", "blzcreateunit"];
    let lower = script.to_ascii_lowercase();
    let bytes = script.as_bytes();
    let mut ids = BTreeSet::new();
    let mut calls = 0usize;
    let mut literals = 0usize;
    let mut counted: Vec<usize> = Vec::new();
    for creator in CREATORS {
        let mut from = 0usize;
        while let Some(hit) = lower[from..].find(creator) {
            let at = from + hit;
            from = at + creator.len();
            if counted.contains(&at) {
                continue;
            }
            // Identifier boundary: `CreateUnitsForPlayer1` is a function name.
            if at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_') {
                continue;
            }
            let mut end = at + creator.len();
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            let mut open = end;
            while open < bytes.len() && bytes[open].is_ascii_whitespace() {
                open += 1;
            }
            if open >= bytes.len() || bytes[open] != b'(' {
                continue;
            }
            counted.push(at);
            calls += 1;
            let window_end = (open + 200).min(bytes.len());
            let mut index = open;
            while index + 5 < window_end {
                if bytes[index] == b'\''
                    && bytes[index + 5] == b'\''
                    && bytes[index + 1..index + 5]
                        .iter()
                        .all(|byte| byte.is_ascii_graphic() && *byte != b'\'')
                {
                    ids.insert(script[index + 1..index + 5].to_string());
                    literals += 1;
                    break;
                }
                index += 1;
            }
        }
    }
    (ids, calls, literals)
}

/// Deduplicated `GAME_MODEL_SKIP` log: one line per def id, however many frames
/// ask for the placeholder.
#[derive(Default)]
struct ModelSkipLog {
    seen: HashSet<String>,
    lines: Vec<String>,
}

/// Menu art from the HUD pack (panel, button states, loading frame).
///
/// The main menu was a flat text list on a black screen because the retail
/// `EscMenu*` textures are absent from the data archives; the pack ships them,
/// so the menu uses those. `None` keeps the flat dialog.
#[derive(Clone, Copy, Debug, Default)]
pub struct MenuTextures {
    pub panel: Option<usize>,
    pub button_normal: Option<usize>,
    pub button_hover: Option<usize>,
    pub button_pressed: Option<usize>,
    pub button_disabled: Option<usize>,
    pub loading: Option<usize>,
}

pub struct Assets {
    /// Main-menu art (pack `menus/*`); see [`MenuTextures`].
    pub menu_textures: MenuTextures,
    pub models: Vec<ModelAsset>,
    pub textures: Vec<TextureImage>,
    pub terrain_atlas: Option<TextureImage>,
    /// UV rectangle (u0, v0, u1, v1) per atlas slot.
    pub atlas_uv: Vec<[f32; 4]>,
    /// Atlas slot for each entry of `Terrain::ground_tiles`.
    pub tile_slot: Vec<u8>,
    /// Raw decoded TerrainArt BLP for each entry of `Terrain::ground_tiles`
    /// (native sheet size, before atlas compositing). The terrain renderer
    /// builds its texture array from this canonical asset data.
    pub terrain_tile_images: Vec<TextureImage>,
    /// Biome for each entry of `Terrain::ground_tiles` (see
    /// `terrain_render::Biome`). Indexed by `TerrainTile::texture()`.
    pub tile_biome: Vec<u8>,
    /// Index into `textures` of the seamless neutral terrain detail map.
    pub terrain_detail: usize,
    pub terrain: Option<Terrain>,
    pub terrain_tiles: Vec<TerrainTile>,
    pub pathing: Option<wc3_formats::wpm::PathingMap>,
    /// Roster def id -> index in `models` (per-definition MDX, not the 4 kinds).
    pub model_by_def: HashMap<String, usize>,
    /// Which definitions this load resolved, and why (`GAME_MODEL_SCOPE`).
    pub model_scope: ModelScope,
    /// Definition rows in the object database that carry a model file (814 on
    /// the shipped install). `loaded_model_count()` is the resident subset.
    pub model_roster: usize,
    /// `GAME_MODEL_SKIP` lines, one per skipped def id (deduplicated).
    model_skip: Mutex<ModelSkipLog>,
    /// Texture path -> index in `textures`, reused by [`Assets::ensure_model`].
    tex_cache: HashMap<String, usize>,
    /// `sloc` markers in the map's `war3mapUnits.doo` (>=2 means a real melee map).
    pub start_locations: usize,
    pub map_name: String,
    pub map_width_cells: u32,
    pub map_height_cells: u32,
    pub world_size: Vec2,
    pub minimap: Option<TextureImage>,
    pub source: String,
    pub debug: Vec<String>,
    /// Typed object database built from the same Storm view: unit names,
    /// command-card icons, stats and tooltips come from the shipped data (and
    /// from map object-data overrides), not from hardcoded labels.
    pub objects: Option<GameDatabase>,
    /// FDF UI theme (data/layout semantics and game icons).
    pub ui: UiTheme,
    /// Procedural modern mobile chrome (glass panels, joystick, detail map).
    pub chrome: Chrome,
    /// Gameplay audio hooks (unit acks, UI, music) on the shared database.
    pub audio: Option<WiredGameAudio>,
    /// The fantasy-atlas HUD skin, built once from the installed `ui_atlas`
    /// (24 roles, atlas-manifest order). `None` keeps every HUD draw site on the
    /// retail-FDF/chrome path, so a clean RoC install has no visual regression.
    pub hud_skin: Option<crate::hudskin::Skin>,
    /// Dialog variant of [`Self::hud_skin`]: the multi-variant parent sheets are
    /// replaced by their authored sub-slices (`analysis/h2/atlas/slices.json`
    /// `sub_elements`): list rows use `row_0`, buttons use
    /// `button_normal`/`button_pressed`, the banner strip uses `strip_top`.
    pub hud_skin_dialog: Option<crate::hudskin::Skin>,
}

/// Build the runtime HUD skin from the installed atlas.
///
/// `Skin` is indexed in [`crate::hudskin::ROLES`] order, which is the same
/// element order as `analysis/h2/atlas/slices.json`; the atlas loader preserves
/// the manifest order, so element `i` is role `i` (the sidecar validates every
/// rect against the manifest at load time, `ui_atlas.rs` `check_rects`).
pub(crate) fn build_hud_skin(
    texture: usize,
    atlas: &crate::ui_atlas::Atlas,
) -> Option<crate::hudskin::Skin> {
    let roles = crate::hudskin::ROLES.len();
    if atlas.elements.len() < roles {
        return None;
    }
    let mut skin = crate::hudskin::Skin {
        texture,
        width: atlas.image.width as f32,
        height: atlas.image.height as f32,
        rects: [[0.0; 4]; 24],
        insets: [[0.0; 4]; 24],
        role_textures: Vec::new(),
        role_sizes: Vec::new(),
        role_names: Vec::new(),
        pack: None,
    };
    for (slot, element) in atlas.elements.iter().take(roles).enumerate() {
        skin.rects[slot] = [
            element.rect.x,
            element.rect.y,
            element.rect.w,
            element.rect.h,
        ];
        skin.insets[slot] = element.insets;
    }
    Some(skin)
}

/// The dialog skin: one authored sub-slice per role whose parent sheet mixed
/// several variants (measured in `analysis/h2/atlas/slices.json`
/// `sub_elements`, 2026-11-15). A sub-slice is a whole small plate, so its
/// insets are zero: the quad stretches as one piece instead of slicing the
/// parent's baked lanes.
pub(crate) fn dialog_sub_slice_skin(skin: &crate::hudskin::Skin) -> crate::hudskin::Skin {
    let mut out = skin.clone();
    let mut replace = |slot: usize, parent: usize, expect: [f32; 2], dy: f32, height: f32| {
        if slot >= out.rects.len() || parent >= out.rects.len() {
            return;
        }
        let [x, y, w, h] = out.rects[parent];
        // Only apply the sub-slice when the parent really is the authored
        // sheet the offsets were measured on; another atlas keeps its parent.
        if (w - expect[0]).abs() > 4.0 || (h - expect[1]).abs() > 4.0 {
            return;
        }
        out.rects[slot] = [x, y + dy, w, height];
        out.insets[slot] = [0.0; 4];
    };
    // list_panel -> row_0 (y 105..135 of the 218x287 list sheet).
    replace(19, 19, [218.0, 287.0], 105.0, 30.0);
    // button_normal -> button_normal part0 (217x25 of the 217x119 sheet).
    replace(20, 20, [217.0, 119.0], 15.0, 25.0);
    // menu_button_pressed -> button_normal part1, the gold-highlighted
    // variant (217x30), i.e. the pressed/hover plate.
    replace(22, 20, [217.0, 119.0], 55.0, 30.0);
    // wide_panel -> strip_top (216x27 of the 216x118 plate).
    replace(23, 23, [216.0, 118.0], 17.0, 27.0);
    out
}

/// `AssetSource` over the canonical multi-archive Storm filesystem
/// (`wc3-re::storm::StormFs`): priority, loose trees and listfile search come
/// from the RE layer; `wc3-assetdb` only caches.
struct StormSource {
    fs: StormFs,
    label: String,
    listfile: Vec<String>,
    by_basename: HashMap<String, Vec<String>>,
}

/// Text files the data/UI readers parse as UTF-8.
const TEXT_EXTENSIONS: [&str; 7] = ["txt", "slk", "fdf", "toc", "ai", "j", "ini"];

/// Texture index that replaceable (team-colour/glow) slots bind.
///
/// `Assets::textures[0]` is the 2x2 opaque white texel: the mask the player
/// tint multiplies.
const REPLACEABLE_MASK_TEXTURE: usize = 0;

/// Retail frame textures that get nine-patch geometry at load time.///
/// The console tiles, the command-button chrome, the tooltip art, the portrait
/// mask and the shipped esc-menu panel/border: every one of them is drawn into a
/// rect the panel layout chooses, so each needs its corners measured once here
/// rather than re-derived per frame.
const FRAME_TEXTURE_KEYS: [&str; 10] = [
    "ConsoleTexture01",
    "ConsoleTexture02",
    "ConsoleTexture03",
    "ConsoleTexture04",
    "CommandButtonBorder",
    "TooltipBackground",
    "TooltipBorder",
    "PortraitFrame",
    "EscMenuBackground",
    "EscMenuBorder",
];

/// Windows-1251 code page (the ANSI codepage of a Russian WC3 install).
///
/// Undefined byte `0x98` decodes to `U+FFFD`, matching `encoding_rs`. The table
/// exists so the install reader needs no extra dependency.
const CP1251: [char; 256] = [
    '\u{0000}', '\u{0001}', '\u{0002}', '\u{0003}', '\u{0004}', '\u{0005}', '\u{0006}', '\u{0007}',
    '\u{0008}', '\u{0009}', '\u{000a}', '\u{000b}', '\u{000c}', '\u{000d}', '\u{000e}', '\u{000f}',
    '\u{0010}', '\u{0011}', '\u{0012}', '\u{0013}', '\u{0014}', '\u{0015}', '\u{0016}', '\u{0017}',
    '\u{0018}', '\u{0019}', '\u{001a}', '\u{001b}', '\u{001c}', '\u{001d}', '\u{001e}', '\u{001f}',
    '\u{0020}', '\u{0021}', '\u{0022}', '\u{0023}', '\u{0024}', '\u{0025}', '\u{0026}', '\u{0027}',
    '\u{0028}', '\u{0029}', '\u{002a}', '\u{002b}', '\u{002c}', '\u{002d}', '\u{002e}', '\u{002f}',
    '\u{0030}', '\u{0031}', '\u{0032}', '\u{0033}', '\u{0034}', '\u{0035}', '\u{0036}', '\u{0037}',
    '\u{0038}', '\u{0039}', '\u{003a}', '\u{003b}', '\u{003c}', '\u{003d}', '\u{003e}', '\u{003f}',
    '\u{0040}', '\u{0041}', '\u{0042}', '\u{0043}', '\u{0044}', '\u{0045}', '\u{0046}', '\u{0047}',
    '\u{0048}', '\u{0049}', '\u{004a}', '\u{004b}', '\u{004c}', '\u{004d}', '\u{004e}', '\u{004f}',
    '\u{0050}', '\u{0051}', '\u{0052}', '\u{0053}', '\u{0054}', '\u{0055}', '\u{0056}', '\u{0057}',
    '\u{0058}', '\u{0059}', '\u{005a}', '\u{005b}', '\u{005c}', '\u{005d}', '\u{005e}', '\u{005f}',
    '\u{0060}', '\u{0061}', '\u{0062}', '\u{0063}', '\u{0064}', '\u{0065}', '\u{0066}', '\u{0067}',
    '\u{0068}', '\u{0069}', '\u{006a}', '\u{006b}', '\u{006c}', '\u{006d}', '\u{006e}', '\u{006f}',
    '\u{0070}', '\u{0071}', '\u{0072}', '\u{0073}', '\u{0074}', '\u{0075}', '\u{0076}', '\u{0077}',
    '\u{0078}', '\u{0079}', '\u{007a}', '\u{007b}', '\u{007c}', '\u{007d}', '\u{007e}', '\u{007f}',
    '\u{0402}', '\u{0403}', '\u{201a}', '\u{0453}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{20ac}', '\u{2030}', '\u{0409}', '\u{2039}', '\u{040a}', '\u{040c}', '\u{040b}', '\u{040f}',
    '\u{0452}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{fffd}', '\u{2122}', '\u{0459}', '\u{203a}', '\u{045a}', '\u{045c}', '\u{045b}', '\u{045f}',
    '\u{00a0}', '\u{040e}', '\u{045e}', '\u{0408}', '\u{00a4}', '\u{0490}', '\u{00a6}', '\u{00a7}',
    '\u{0401}', '\u{00a9}', '\u{0404}', '\u{00ab}', '\u{00ac}', '\u{00ad}', '\u{00ae}', '\u{0407}',
    '\u{00b0}', '\u{00b1}', '\u{0406}', '\u{0456}', '\u{0491}', '\u{00b5}', '\u{00b6}', '\u{00b7}',
    '\u{0451}', '\u{2116}', '\u{0454}', '\u{00bb}', '\u{0458}', '\u{0405}', '\u{0455}', '\u{0457}',
    '\u{0410}', '\u{0411}', '\u{0412}', '\u{0413}', '\u{0414}', '\u{0415}', '\u{0416}', '\u{0417}',
    '\u{0418}', '\u{0419}', '\u{041a}', '\u{041b}', '\u{041c}', '\u{041d}', '\u{041e}', '\u{041f}',
    '\u{0420}', '\u{0421}', '\u{0422}', '\u{0423}', '\u{0424}', '\u{0425}', '\u{0426}', '\u{0427}',
    '\u{0428}', '\u{0429}', '\u{042a}', '\u{042b}', '\u{042c}', '\u{042d}', '\u{042e}', '\u{042f}',
    '\u{0430}', '\u{0431}', '\u{0432}', '\u{0433}', '\u{0434}', '\u{0435}', '\u{0436}', '\u{0437}',
    '\u{0438}', '\u{0439}', '\u{043a}', '\u{043b}', '\u{043c}', '\u{043d}', '\u{043e}', '\u{043f}',
    '\u{0440}', '\u{0441}', '\u{0442}', '\u{0443}', '\u{0444}', '\u{0445}', '\u{0446}', '\u{0447}',
    '\u{0448}', '\u{0449}', '\u{044a}', '\u{044b}', '\u{044c}', '\u{044d}', '\u{044e}', '\u{044f}',
];

fn is_text_path(path: &str) -> bool {
    path.rsplit('.')
        .next()
        .map(|extension| TEXT_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn push_cp1251(out: &mut Vec<u8>, bytes: &[u8]) {
    for byte in bytes {
        let mut buffer = [0u8; 4];
        out.extend_from_slice(CP1251[*byte as usize].encode_utf8(&mut buffer).as_bytes());
    }
}

/// Decode install text to UTF-8.
///
/// Retail ships almost every text file as UTF-8, but the expansion
/// localisation archive mixes code pages inside one file: the Russian
/// `UI\MiscUI.txt` of `War3xlocal.mpq` is UTF-8 except for its `QuickSaveFile`
/// line, which is Windows-1251. A whole-file decode therefore rejects the file
/// (`text table is not UTF-8`). Decode line by line: UTF-8 where valid, the
/// ANSI code page where not. Binary assets are never touched.
fn decode_install_text(bytes: Vec<u8>, path: &str) -> Vec<u8> {
    if !is_text_path(path) {
        return bytes;
    }
    if std::str::from_utf8(&bytes).is_ok() {
        return bytes;
    }
    let mut out = Vec::with_capacity(bytes.len() + bytes.len() / 4);
    for (index, line) in bytes.split(|byte| *byte == b'\n').enumerate() {
        if index > 0 {
            out.push(b'\n');
        }
        if std::str::from_utf8(line).is_ok() {
            out.extend_from_slice(line);
        } else {
            push_cp1251(&mut out, line);
        }
    }
    out
}

fn storm_basename(path: &str) -> String {
    path.replace('/', "\\")
        .rsplit('\\')
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase()
}

impl StormSource {
    fn new(fs: StormFs, label: impl Into<String>) -> Self {
        let listfile: Vec<String> = fs
            .listfile()
            .into_iter()
            .map(|name| wc3_re::storm::normalize_path(&name))
            .collect();
        let mut by_basename: HashMap<String, Vec<String>> = HashMap::new();
        for name in &listfile {
            by_basename
                .entry(storm_basename(name))
                .or_default()
                .push(name.clone());
        }
        Self {
            fs,
            label: label.into(),
            listfile,
            by_basename,
        }
    }
}

impl wc3_data::source::Source for StormSource {
    fn read(&self, path: &str) -> Result<Vec<u8>, wc3_data::DataError> {
        self.fs
            .read(path)
            .map(|bytes| decode_install_text(bytes, path))
            .map_err(|err| wc3_data::DataError::Io {
                path: path.to_string(),
                source: std::io::Error::new(std::io::ErrorKind::Other, err.to_string()),
            })
    }

    fn exists(&self, path: &str) -> bool {
        self.fs.info(path).is_some()
    }

    fn describe(&self) -> String {
        format!("storm:{}", self.label)
    }
}

impl AssetSource for StormSource {
    fn label(&self) -> &str {
        &self.label
    }

    fn kind(&self) -> &'static str {
        "storm"
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, wc3_assetdb::AssetError> {
        self.fs
            .read(path)
            .map(|bytes| decode_install_text(bytes, path))
            .map_err(|err| {
                wc3_assetdb::AssetError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    err.to_string(),
                ))
            })
    }

    fn size(&self, path: &str) -> Option<u64> {
        self.fs.info(path).map(|info| info.size as u64)
    }

    fn listfile(&self) -> &[String] {
        &self.listfile
    }

    fn files(&self) -> Vec<(String, u64)> {
        self.listfile
            .iter()
            .map(|name| (name.clone(), self.size(name).unwrap_or(0)))
            .collect()
    }

    fn resolve_exact(&self, path: &str) -> Option<(String, String)> {
        let info = self.fs.info(path)?;
        let canonical = self
            .listfile
            .iter()
            .find(|name| name.eq_ignore_ascii_case(path))
            .cloned()
            .unwrap_or_else(|| wc3_re::storm::normalize_path(path));
        Some((info.archive, canonical))
    }

    fn search_basename(&self, basename: &str, limit: usize) -> Vec<SearchHit> {
        let Some(hits) = self.by_basename.get(&basename.to_ascii_lowercase()) else {
            return Vec::new();
        };
        hits.iter()
            .take(limit)
            .map(|path| SearchHit {
                layer: self
                    .fs
                    .info(path)
                    .map(|info| info.archive)
                    .unwrap_or_else(|| self.label.clone()),
                path: path.clone(),
            })
            .collect()
    }

    fn layer_labels(&self) -> Vec<String> {
        self.fs
            .layers()
            .iter()
            .map(|layer| layer.label.clone())
            .collect()
    }
}

/// HUD redraw v0.1 pack root: BLP1 override layered above the MPQ/Storm data.
/// Candidate order: `H2_HUD_REDRAW_DIR`, `<install>/redraw-v0.1/blp`,
/// `<install>/../redraw-v0.1/blp`, then the repo pack for desktop runs.
fn hud_redraw_dir(root: &Path) -> Option<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Some(dir) = std::env::var_os("H2_HUD_REDRAW_DIR") {
        candidates.push(std::path::PathBuf::from(dir));
    }
    candidates.push(root.join("redraw-v0.1").join("blp"));
    if let Some(parent) = root.parent() {
        candidates.push(parent.join("redraw-v0.1").join("blp"));
    }
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/agent-hud/redraw-v0.1/blp"),
    );
    candidates.into_iter().find(|dir| {
        dir.join("UI")
            .join("Console")
            .join("Human")
            .join("HumanUITile01.blp")
            .is_file()
    })
}

/// Highest-priority asset layer for the redrawn HUD art (None -> retail art).
fn hud_redraw_source(root: &Path, debug: &mut Vec<String>) -> Option<Box<dyn AssetSource>> {
    let dir = hud_redraw_dir(root)?;
    match DirSource::open(&dir) {
        Ok(source) => {
            debug.push(format!("hud_redraw={}", dir.display()));
            Some(Box::new(source))
        }
        Err(error) => {
            debug.push(format!("hud_redraw_failed={error}"));
            None
        }
    }
}

/// Prefer the full install (patch > locale > expansion > base) when the archive
/// sits in a WC3 install directory; fall back to the single archive otherwise.
fn open_asset_db(path: &Path, debug: &mut Vec<String>) -> Option<AssetDb> {
    let root = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    match StormFs::open_wc3_install(root) {
        Ok(fs) if !fs.layers().is_empty() => {
            let has_file = fs
                .layers()
                .iter()
                .any(|layer| layer.label == file_name || layer.path.as_path() == path);
            if has_file {
                let labels: Vec<String> =
                    fs.layers().iter().map(|layer| layer.label.clone()).collect();
                let source = StormSource::new(fs, "wc3-install");
                let mut sources: Vec<Box<dyn AssetSource>> = Vec::new();
                if let Some(redraw) = hud_redraw_source(root, debug) {
                    sources.push(redraw);
                }
                sources.push(Box::new(source));
                match AssetDb::open_sources(sources, "wc3-install+hud-redraw") {
                    Ok(db) => {
                        debug.push(format!("storm_layers={labels:?}"));
                        return Some(db);
                    }
                    Err(err) => debug.push(format!("storm_assetdb_failed={err}")),
                }
            } else {
                debug.push(format!("storm_install_ignored={file_name}"));
            }
        }
        Ok(_) => debug.push("storm_no_layers".into()),
        Err(err) => debug.push(format!("storm_open_failed={err}")),
    }
    let mut sources: Vec<Box<dyn AssetSource>> = Vec::new();
    if let Some(redraw) = hud_redraw_source(root, debug) {
        sources.push(redraw);
    }
    match MpqSource::open(path) {
        Ok(mpq) => {
            sources.push(Box::new(mpq));
            match AssetDb::open_sources(sources, "mpq+hud-redraw") {
                Ok(db) => {
                    debug.push(format!("mpq={}", path.display()));
                    return Some(db);
                }
                Err(error) => debug.push(format!("mpq_layered_failed={error}")),
            }
        }
        Err(error) => debug.push(format!("mpq_open_failed={error}")),
    }
    None
}

/// Command-button icon name of a BLP path (`…\BTNFootman.blp` → `BTNFootman`).
fn icon_name_of(path: &str) -> Option<String> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let basename = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let stem = basename.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(basename);
    (!stem.is_empty()).then(|| stem.to_string())
}

/// FourCC of each starter unit in `wc3-sim` (`UnitKind::from_name` accepts the
/// same ids), so the typed database can be queried from the game's enum.
pub const UNIT_IDS: [&str; 4] = ["hfoo", "hrif", "hpea", "htow"];

/// Build the typed object database from the install view.
///
/// One reader (`wc3-formats::gamedata` via `wc3-data`), one pass: the shipped
/// SLK/metadata/text tables plus the map's object-data overrides and `war3map.wts`
/// entries. `None` when no install is mounted, in which case the game keeps the
/// compiled-in fallbacks.
fn open_object_database(
    path: &Path,
    map: Option<&MapBundle>,
    debug: &mut Vec<String>,
) -> Option<GameDatabase> {
    let root = path.parent().unwrap_or_else(|| Path::new("."));
    let fs = match StormFs::open_wc3_install(root) {
        Ok(fs) if !fs.layers().is_empty() => fs,
        Ok(_) => return None,
        Err(error) => {
            debug.push(format!("objects_storm_failed={error}"));
            return None;
        }
    };
    let source = StormSource::new(fs, "objects");
    let mut database = match GameDatabase::load(&source) {
        Ok(database) => database,
        Err(error) => {
            debug.push(format!("objects_failed={error}"));
            return None;
        }
    };
    let mut overrides = 0usize;
    if let Some(map) = map {
        for (name, data) in &map.objects {
            if database
                .apply_parsed(name.clone(), data.to_typed())
                .is_ok()
            {
                overrides += 1;
            }
        }
        if let Some(wts) = &map.wts {
            database
                .localizer_mut()
                .set_wts_entries(wts.entries.clone());
        }
    }
    // Replace the simulator's hand-written spell numbers with the sheet values.
    let mut spell_numbers = Vec::new();
    for spell in wc3_sim::abilities::Spell::ALL {
        let id = spell.def().id;
        let Some(numbers) = database.ability_numbers(id) else {
            continue;
        };
        let Some(level) = numbers.first_level() else {
            continue;
        };
        spell_numbers.push(wc3_sim::abilities::SpellNumbers {
            id,
            mana: level.mana.unwrap_or(0),
            cooldown_ticks: (level.cooldown.unwrap_or(0.0) * wc3_sim::TICKS_PER_SECOND as f32)
                .round() as u32,
            range: level.range.unwrap_or(0.0),
            radius: level.area.unwrap_or(0.0),
            duration_ticks: (level.duration.unwrap_or(0.0)
                * wc3_sim::TICKS_PER_SECOND as f32)
                .round() as u32,
        });
    }
    if !spell_numbers.is_empty() {
        let installed = wc3_sim::abilities::install_spell_numbers(&spell_numbers);
        debug.push(format!(
            "spell_numbers={} installed={installed}",
            spell_numbers.len()
        ));
    }

    debug.push(format!(
        "objects tables={} metadata={} map_overrides={} wts={}",
        database.counts().len(),
        database.counts().iter().map(|(_, rows)| rows).sum::<usize>(),
        overrides,
        database
            .localizer()
            .wts_entries()
            .len()
    ));
    Some(database)
}

/// `ReplaceableTextures\\CommandButtons\\BTNPeon.blp` -> `BTNPeon`.
fn icon_basename(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .unwrap_or(path)
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(path)
        .to_string()
}

impl Assets {
    /// The fantasy-atlas HUD skin, when the install ships `ui_atlas.png`.
    ///
    /// Built once in [`Assets::load`] next to the atlas registration; every HUD
    /// draw site treats `None` as "keep the retail chrome".
    pub fn hud_skin(&self) -> Option<&crate::hudskin::Skin> {
        self.hud_skin.as_ref()
    }

    /// Dialog variant carrying the authored sub-slices (rows / buttons /
    /// banner strip). See [`dialog_sub_slice_skin`].
    pub fn hud_skin_dialog(&self) -> Option<&crate::hudskin::Skin> {
        self.hud_skin_dialog.as_ref()
    }

    pub fn load(mpq_path: Option<&Path>, map_path: Option<&Path>) -> Self {
        // Every format-layer skip (missing SLK column, corrupt record) must
        // reach logcat, not stderr.
        crate::doodads::install_format_diag_sink();
        let mut debug = Vec::new();
        let mut textures = vec![TextureImage::white()];
        let mut tex_cache: HashMap<String, usize> = HashMap::new();
        let mut db = mpq_path
            .and_then(|path| open_asset_db(path, &mut debug))
            .map(SharedAssetDb::new);
        debug.push(format!("mem_stage=db_open rss_kb={}", crate::mem::rss_kb()));
        // Gameplay audio shares this database (one cache for renderer + hooks).
        let audio = db.as_ref().map(|shared| {
            let (audio, wiring) = start_game_audio(shared.clone());
            // `GAME_AUDIO` is the device-log marker the QA gate greps; it must
            // travel through `dev_log` because SDL does not mirror stdio.
            log_audio_wiring(&wiring);
            debug.push(wiring.summary());
            audio
        });
        // Canonical W3M/W3X reader: all inner files, encrypted-sector fallback
        // and `TRIGSTR_n` resolution through `war3map.wts`.
        let map = map_path.and_then(|path| match MapBundle::open(path) {
            Ok(bundle) => {
                debug.push(format!("map={}", path.display()));
                Some(bundle)
            }
            Err(error) => {
                debug.push(format!("map_open_failed={error}"));
                None
            }
        });
        debug.push(format!("mem_stage=map_open rss_kb={}", crate::mem::rss_kb()));

        let objects = mpq_path.and_then(|path| open_object_database(path, map.as_ref(), &mut debug));

        let terrain = map.as_ref().and_then(|map| map.terrain.clone());
        let pathing = map.as_ref().and_then(|map| map.pathing.clone());
        let mut map_name = map
            .as_ref()
            .and_then(|map| map.localized_info())
            .map(|info| info.name)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Herofolends".to_string());
        if map_name.starts_with("TRIGSTR_") {
            map_name = map_path
                .and_then(|path| path.file_stem())
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or(map_name);
        }

        let mut models = Vec::with_capacity(MODEL_KINDS.len());
        for (index, kind) in MODEL_KINDS.iter().copied().enumerate() {
            let mut asset = match &mut db {
                Some(db) => match db.model(MODEL_PATHS[index]) {
                    Ok(model) => {
                        let tex_map = load_model_textures(db, &mut textures, &mut tex_cache, &model);
                        let meshes = build_meshes(&model, &tex_map);
                        let sequences = model.sequences.clone();
                        let idle = sequences
                            .iter()
                            .position(|s| s.name.to_ascii_lowercase().contains("stand"));
                        let walk = sequences
                            .iter()
                            .position(|s| s.name.to_ascii_lowercase().contains("walk"));
                        let attack = sequences
                            .iter()
                            .position(|s| s.name.to_ascii_lowercase().contains("attack"));
                        // Height for HUD anchoring: only the geosets visible in
                        // the Stand sequence. The TownHall MDX also contains
                        // the Keep/Castle upgrade stages; including them made
                        // the health bar float four tiles above the roof.
                        let stand = idle.and_then(|index| sequences.get(index));
                        let stand_alphas = mesh_alphas_for(&model, &meshes, stand, stand.map(|s| s.start).unwrap_or(0));
                        let mut visible_min = f32::INFINITY;
                        let mut visible_max = f32::NEG_INFINITY;
                        for (mesh_index, mesh) in meshes.iter().enumerate() {
                            if mesh.hide || stand_alphas.get(mesh_index).copied().unwrap_or(1.0) <= 0.01 {
                                continue;
                            }
                            for position in &mesh.positions {
                                visible_min = visible_min.min(position.z);
                                visible_max = visible_max.max(position.z);
                            }
                        }
                        let height = if visible_max.is_finite()
                            && visible_min.is_finite()
                            && visible_max > visible_min
                        {
                            visible_max - visible_min
                        } else {
                            120.0
                        };
                        let pivot = if visible_min.is_finite() { visible_min } else { 0.0 };
                        debug.push(format!(
                            "model={} meshes={} sequences={} height={height:.1}",
                            MODEL_PATHS[index],
                            meshes.len(),
                            sequences.len()
                        ));
                        // Presentation scale: the default phone camera is wide
                        // enough that a 1:1 footman is ~66 px tall at 1080p.
                        // WC3's Art - Scaling Value equivalent of 1.15 brings
                        // infantry back to ~76 px without touching buildings.
                        let scale = if kind == UnitKind::TownHall { 1.0 } else { 1.15 };
                        ModelAsset {
                            kind,
                            model: Some(model),
                            meshes,
                            sequences,
                            idle,
                            walk,
                            attack,
                            height: height * scale,
                            pivot: pivot * scale,
                            scale,
                            yaw_offset: yaw_offset_for(kind),
                            fallback: false,
                        }
                    }
                    Err(error) => {
                        debug.push(format!("model_failed={} {error}", MODEL_PATHS[index]));
                        fallback_model(kind)
                    }
                },
                None => fallback_model(kind),
            };
            if asset.meshes.is_empty() {
                asset = fallback_model(kind);
            }
            models.push(asset);
        }

        // Per-definition models: every roster unit/building renders its own
        // MDX. The four-kind fallback made every building share one texture.
        //
        // Only the definitions this map can show are parsed. The full roster is
        // 814 parsed `MdxModel`s (169.8 MB of bones, node tracks, sequences and
        // materials, `mem_probe --breakdown`); a melee map shows ~150 of them.
        // Every definition the load skips gets its own bounded placeholder, so
        // `model_for_def` never silently borrows another unit's art.
        debug.push(format!(
            "mem_stage=kind_models rss_kb={}",
            crate::mem::rss_kb()
        ));
        let scope = map_model_scope(map.as_ref());
        let mut model_roster = 0usize;
        let mut model_by_def: HashMap<String, usize> = HashMap::new();
        {
            if let (Some(db), Some(objects)) = (db.as_mut(), objects.as_ref()) {
                let mut loaded = 0usize;
                for view in objects.units() {
                    let id = view.id_str();
                    let Some(path) = view.model_file() else { continue; };
                    model_roster += 1;
                    if !scope.wants(&id) {
                        continue;
                    }
                    if model_by_def.contains_key(&id) {
                        continue;
                    }
                    let Ok(model) = db.model(&path) else {
                        debug.push(format!("model_failed={path} id={id}"));
                        continue;
                    };
                    let tex_map = load_model_textures(db, &mut textures, &mut tex_cache, &model);
                    let meshes = build_meshes(&model, &tex_map);
                    if meshes.is_empty() {
                        debug.push(format!("model_meshless={path} id={id}"));
                        continue;
                    }
                    let index = models.len();
                    models.push(def_model_asset(&id, model, meshes));
                    model_by_def.insert(id, index);
                    loaded += 1;
                    if loaded % 64 == 0 {
                        debug.push(format!(
                            "mem_stage=def_{loaded} rss_kb={} textures={} tex_kb={}",
                            crate::mem::rss_kb(),
                            textures.len(),
                            textures
                                .iter()
                                .map(|image| image.cpu_bytes())
                                .sum::<usize>()
                                / 1024
                        ));
                    }
                }
                // Every definition without a parsed model gets its own box,
                // sized from the def. That is the explicit missing-model path:
                // a flat stand-in for a def the map does not need, never
                // another unit's MDX (a silent wrong model).
                let mut placeholders = 0usize;
                for view in objects.units() {
                    let id = view.id_str();
                    if view.model_file().is_none() || model_by_def.contains_key(&id) {
                        continue;
                    }
                    let index = models.len();
                    models.push(placeholder_model(wc3_sim::data::def_of(&id)));
                    model_by_def.insert(id, index);
                    placeholders += 1;
                }
                // A def that is not in the object database at all (a map-only
                // id) still needs a bounded draw: keep one generic box so
                // `model_index_for_def` never falls back to a real unit's MDX.
                if !models.iter().any(|model| model.fallback) {
                    models.push(placeholder_model(None));
                }
                debug.push(format!(
                    "def_models={loaded} placeholders={placeholders} roster={model_roster}"
                ));
                debug.push(format!(
                    "mem_stage=def_models rss_kb={}",
                    crate::mem::rss_kb()
                ));
            }
        }

        // Terrain art atlas (real BLP tiles from the archive) plus the biome
        // classification used by the new vertex-colour terrain. The atlas is
        // kept for diagnostics/tests, not for world rendering: sampling its
        // variation quadrants per cell is what produced the quilt.
        let mut tile_slot = Vec::new();
        let mut atlas_uv = Vec::new();
        let mut terrain_atlas = None;
        let mut tile_biome: Vec<u8> = Vec::new();
        let mut terrain_tile_images: Vec<TextureImage> = Vec::new();
        if let (Some(db), Some(terrain)) = (&mut db, &terrain) {
            let tags: Vec<String> = terrain
                .ground_tiles
                .iter()
                .map(|tag| String::from_utf8_lossy(tag).into_owned())
                .collect();
            let table = db
                .read("TerrainArt\\Terrain.slk")
                .ok()
                .and_then(|bytes| SlkTable::parse(&bytes).ok());
            // One atlas slot per distinct ground texture; the old fixed 4x2
            // grid silently mapped every tile past the 8th onto slot 0.
            let atlas_rows = ((tags.len() as u32 + 3) / 4).max(2);
            let mut atlas = AtlasBuilder::new(4, atlas_rows, 256);
            let mut slots = vec![0u8; tags.len()];
            for (tag_index, tag) in tags.iter().enumerate() {
                let path = table
                    .as_ref()
                    .and_then(|table| terrain_art_path(table, tag))
                    .unwrap_or_else(|| format!("TerrainArt\\Village\\Village_GrassShort.blp"));
                match db.texture(&path) {
                    Ok(raw) => {
                        let source = TextureImage {
                            width: raw.width.max(1),
                            height: raw.height.max(1),
                            rgba: raw.rgba.clone(),
                        };
                        terrain_tile_images.push(source);
                        let image = resize_to(&raw, 256, 256);
                        let average = average_color(&image);
                        if tag_index == 0 {
                            atlas.base = average;
                        }
                        let biome = crate::terrain_render::classify(tag, average, false);
                        tile_biome.push(biome as u8);
                        if let Some(slot) = atlas.push(&image) {
                            slots[tag_index] = slot as u8;
                            debug.push(format!(
                                "tile[{tag_index}]={tag} -> {path} slot={slot} biome={biome:?} avg={average:?}"
                            ));
                        }
                    }
                    Err(error) => {
                        tile_biome.push(Biome::Grass as u8);
                        terrain_tile_images.push(TextureImage {
                            width: 1,
                            height: 1,
                            rgba: vec![94, 124, 57, 255],
                        });
                        debug.push(format!("tile_failed={tag} {path}: {error}"));
                    }
                }
            }
            if let Some((image, uv)) = atlas.finish() {
                terrain_atlas = Some(image);
                atlas_uv = uv;
            }
            tile_slot = slots;
        }

        let terrain_tiles = terrain
            .as_ref()
            .map(|terrain| terrain.tiles.clone())
            .unwrap_or_else(|| procedural_tiles(129, 129));
        let (cells_w, cells_h) = terrain
            .as_ref()
            .map(|terrain| (terrain.width.saturating_sub(1), terrain.height.saturating_sub(1)))
            .unwrap_or((128, 128));
        let world_size = Vec2::new(
            cells_w as f32 * TERRAIN_TILE_SIZE,
            cells_h as f32 * TERRAIN_TILE_SIZE,
        );
        let minimap = Some(build_minimap(
            &terrain_tiles,
            cells_w,
            cells_h,
            pathing.as_ref(),
        ));

        let mut ui = match &mut db {
            Some(db) => UiTheme::load_with(db, &mut textures),
            None => UiTheme::default(),
        };
        debug.push(format!("mem_stage=ui_theme rss_kb={}", crate::mem::rss_kb()));
        // Every object that names a command-button icon becomes resolvable by
        // its WC3 type id, so data-driven card slots draw the shipped icon.
        if let Some(objects) = &objects {
            for unit in objects.units() {
                if let Some(icon) = unit.icon() {
                    if let Some(name) = icon_name_of(&icon) {
                        ui.set_icon_alias(&unit.id_str(), &name);
                    }
                }
            }
            // Preload every icon the portrait panel and the command card can
            // draw. A fixed starter list served an Orc peon the Human peasant
            // portrait: its id had an alias but its texture was never loaded.
            let mut loaded = 0usize;
            let mut missing = 0usize;
            if let Some(db) = db.as_mut() {
                for unit in objects.units() {
                    let id = unit.id_str();
                    let Some(name) = ui.type_icon_name(&id).map(str::to_string) else {
                        continue;
                    };
                    if ui.texture(&name).is_some() {
                        continue;
                    }
                    if ui.load_command_icon(db, &mut textures, &name).is_some() {
                        loaded += 1;
                    } else {
                        missing += 1;
                    }
                }
            }
            debug.push(format!(
                "object_icons_loaded={loaded} object_icons_missing={missing}"
            ));
            debug.push(format!(
                "mem_stage=object_icons rss_kb={}",
                crate::mem::rss_kb()
            ));
        }
        let mut chrome = Chrome::new(&mut textures);
        // Nine-patch geometry for the frames the console draws. The insets are
        // measured from each shipped BLP's own edge structure, so a panel resize
        // keeps the ornament at its authored size instead of squashing it.
        let mut frame_patches = 0usize;
        for key in FRAME_TEXTURE_KEYS {
            if let Some(index) = ui.texture(key) {
                chrome.register_frame(index, &textures[index]);
                frame_patches += 1;
            }
        }
        debug.push(format!("ui_frame_patches={frame_patches}"));
        // The operator's FINAL HUD asset pack is the primary skin: one PNG per
        // element, exact manifest insets and the 768-design-height composition.
        // The pack directory is data (env override, app files dir, Download
        // drop, repo for headless proofs), never a compiled-in art fallback.
        let mut hud_skin = None;
        let mut hud_skin_dialog = None;
        let mut pack_loaded = false;
        let mut menu_textures = MenuTextures::default();
        if let Some(root) = crate::hudskin::pack::find_installed() {
            match crate::hudskin::pack::HudPack::load(&root, &mut textures) {
                Ok(pack) => {
                    // Menu art lives in the same pack; grab it before the skin
                    // consumes the pack reference.
                    menu_textures = MenuTextures {
                        panel: pack.texture("menu_panel_normal_512x384"),
                        button_normal: pack.texture("menu_button_normal_256x64"),
                        button_hover: pack.texture("menu_button_hover_256x64"),
                        button_pressed: pack.texture("menu_button_pressed_256x64"),
                        button_disabled: pack.texture("menu_button_disabled_256x64"),
                        loading: pack.texture("loading_frame_normal_1024x512"),
                    };
                    match crate::hudskin::Skin::from_pack(&pack) {
                    Some(skin) => {
                        debug.push(format!(
                            "HUD_PACK root={} assets={} roles={} fonts={} atlas_cells={} scale1080={:.4}",
                            root.display(),
                            pack.assets.len(),
                            crate::hudskin::ROLES.len(),
                            pack.font_files.len(),
                            pack.cell_count(),
                            pack.scale_for(glam::Vec2::new(1920.0, 1080.0), false),
                        ));
                        hud_skin_dialog = Some(skin.clone());
                        hud_skin = Some(skin);
                        pack_loaded = true;
                    }
                        None => debug.push("HUD_PACK fail=role-mapping".to_string()),
                    }
                },
                Err(error) => {
                    // Loud on purpose: the error names the missing/mis-sized
                    // file; the HUD does not silently swap in other art.
                    crate::dev_log!("{error}");
                    debug.push(error.to_string());
                }
            }
        }
        // The painter's UI atlas is the fallback only: it is used when the pack
        // directory is absent (or failed), with an explicit log line.
        if !pack_loaded && let Some((atlas, files)) = crate::ui_atlas::load_installed() {
            let index = textures.len();
            let (width, height) = (atlas.image.width, atlas.image.height);
            let elements = atlas.elements.len();
            textures.push(atlas.image.to_texture());
            chrome.register_atlas(index, &atlas);
            debug.push(format!(
                "ui_atlas={} {}x{} elements={} patches={} file={}",
                atlas.source,
                width,
                height,
                elements,
                chrome.element_count(),
                files.image.display()
            ));
            if let Some(skin) = build_hud_skin(index, &atlas) {
                hud_skin_dialog = Some(dialog_sub_slice_skin(&skin));
                hud_skin = Some(skin);
                debug.push(format!(
                    "hud_skin=roles={} dialog_subslices=4 texture={index}",
                    crate::hudskin::ROLES.len()
                ));
            } else {
                debug.push(format!(
                    "hud_skin=missing elements={elements} need={}",
                    crate::hudskin::ROLES.len()
                ));
            }
        } else if !pack_loaded {
            debug.push("ui_atlas=missing".to_string());
        }
        if !pack_loaded {
            crate::dev_log!("HUD_PACK fallback=legacy");
            debug.push("HUD_PACK fallback=legacy".to_string());
        }
        let terrain_detail = textures.len();
        textures.push(crate::terrain_render::detail_texture(TERRAIN_DETAIL_SIZE));

        let source = if terrain_atlas.is_some() || models.iter().any(|m| !m.fallback) {
            "user-mpq".to_string()
        } else {
            "fallback".to_string()
        };

        let mut assets = Self {
            menu_textures,
            models,
            textures,
            terrain_atlas,
            atlas_uv,
            tile_slot,
            tile_biome,
            terrain_tile_images,
            terrain_detail,
            terrain,
            terrain_tiles,
            pathing,
            model_by_def,
            model_scope: scope,
            model_roster,
            model_skip: Mutex::new(ModelSkipLog::default()),
            tex_cache,
            start_locations: map
                .as_ref()
                .and_then(|bundle| bundle.units.as_ref())
                .map(|units| {
                    units
                        .iter()
                        .filter(|unit| {
                            String::from_utf8_lossy(&unit.type_id)
                                .eq_ignore_ascii_case("sloc")
                        })
                        .count()
                })
                .unwrap_or(0),
            map_name,
            map_width_cells: cells_w,
            map_height_cells: cells_h,
            world_size,
            minimap,
            source,
            debug,
            objects,
            ui,
            chrome,
            audio,
            hud_skin,
            hud_skin_dialog,
        };
        // Terrain occlusion for the audio mixer: the sampler captures the final
        // W3E heights (cliff layers included), so a unit behind a ridge is
        // muffled by the real map geometry rather than a flat distance rule.
        assets.install_audio_occluder();
        // Vertex data is no longer edited after this point (height/pivot and the
        // pose alphas are computed above), so switch every mesh to the compact
        // runtime format: 20 B/vertex instead of ~52, with four influences.
        assets.compact_meshes();
        // The authored geosets were flattened into those meshes; the animator
        // and the material path never read them again.
        assets.release_mdx_geometry();
        // One aggregate line for the skips the format layer counted while the
        // data set was read. Per-element lines are bounded to 8 per reason;
        // this is the total, so a load report can say "this map needed 12
        // defaults" without grepping the log.
        let skips = wc3_formats::diag::summary();
        if !skips.is_empty() {
            assets.debug.push(skips.clone());
            crate::dev_log!("{skips}");
        }
        // Model residency in one line for the device harness: how many MDX the
        // map scope parsed, how many definitions got a bounded placeholder, and
        // the roster size those numbers are a subset of.
        let model_scope = format!(
            "GAME_MODEL_SCOPE {} loaded={} skipped={} roster={}",
            assets.model_scope.summary(),
            assets.loaded_model_count(),
            assets.placeholder_count(),
            assets.model_roster
        );
        assets.debug.push(model_scope.clone());
        crate::dev_log!("{model_scope}");
        assets
    }

    /// FourCC of a starter unit kind.
    pub fn unit_id(kind: UnitKind) -> &'static str {
        match kind {
            UnitKind::Footman => UNIT_IDS[0],
            UnitKind::Archer => UNIT_IDS[1],
            UnitKind::Peasant => UNIT_IDS[2],
            UnitKind::TownHall => UNIT_IDS[3],
        }
    }

    /// Typed object info for a starter unit (name, tooltip, icon, model, stats).
    pub fn unit_info(&self, kind: UnitKind) -> Option<ObjectInfo> {
        self.objects.as_ref()?.object(Self::unit_id(kind))
    }

    /// Typed object info for any WC3 type id (`opeo`, `ogre`, `nftr`): the
    /// portrait card and the stats rows are resolved by the unit's own id.
    pub fn info_for_def(&self, def_id: &str) -> Option<ObjectInfo> {
        self.objects.as_ref()?.object(def_id)
    }

    /// Icon basename (`BTNPeon`) for any WC3 type id from the object data.
    pub fn icon_name_for_def(&self, def_id: &str) -> Option<String> {
        if let Some(view) = self.objects.as_ref()?.unit(def_id)
            && let Some(path) = view.icon()
        {
            return Some(icon_basename(&path));
        }
        let path = self.objects.as_ref()?.object(def_id)?.icon?;
        let basename = path
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(&path)
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or(&path);
        Some(basename.to_string())
    }

    /// Localised name for any WC3 type id from the object data.
    pub fn name_for_def(&self, def_id: &str) -> Option<String> {
        if let Some(view) = self.objects.as_ref()?.unit(def_id)
            && let Some(name) = view.name()
            && !name.is_empty()
        {
            return Some(name);
        }
        self.objects.as_ref()?.object(def_id)?.name.clone()
    }

    /// Unit acknowledgement sound set (`UnitUI.unitSound`, e.g. "Peon").
    pub fn unit_sound_set(&self, def_id: &str) -> Option<String> {
        self.objects
            .as_ref()?
            .unit(def_id)?
            .sound_set()
            .map(str::to_string)
    }

    /// Hotkey letter of an ability from the shipped object data.
    pub fn ability_hotkey(&self, ability_id: &str) -> Option<char> {
        self.objects
            .as_ref()?
            .ability(ability_id)?
            .hotkey()
            .and_then(|text| text.chars().next())
    }

    /// Localised unit name, falling back to the compiled-in label.
    pub fn unit_name(&self, kind: UnitKind) -> String {
        self.unit_info(kind)
            .and_then(|info| info.name)
            .unwrap_or_else(|| kind.name().to_string())
    }

    /// Command-card icon path (`ReplaceableTextures\\CommandButtons\\*.blp`).
    pub fn unit_icon_path(&self, kind: UnitKind) -> Option<String> {
        self.unit_info(kind).and_then(|info| info.icon)
    }

    /// Icon basename (`BTNFootman`) for the `UiTheme` texture cache.
    pub fn unit_icon_name(&self, kind: UnitKind) -> Option<String> {
        let path = self.unit_icon_path(kind)?;
        let basename = path
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(&path)
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or(&path);
        Some(basename.to_string())
    }

    /// Typed stats of a starter unit.
    pub fn unit_stats(&self, kind: UnitKind) -> Option<ObjectStats> {
        self.unit_info(kind).map(|info| info.stats)
    }

    /// Tooltip (`Ubertip`, else `Tip`) for a starter unit.
    pub fn unit_tooltip(&self, kind: UnitKind) -> Option<String> {
        self.unit_info(kind).and_then(|info| info.ubertip.or(info.tip))
    }

    /// Trainees of a producer, straight from the profile `Trains=` list.
    /// Trainees of a producer, resolved from its real def id (`htow`, `etol`,
    /// `unpl`, `ogre`), never from the four starter kinds: the map's hall is
    /// whatever race the map places.
    pub fn train_menu(&self, def_id: &str) -> Vec<wc3_data::MenuEntry> {
        self.objects
            .as_ref()
            .map(|objects| objects.train_menu(def_id))
            .unwrap_or_default()
    }

    /// Structures a worker can place, straight from the profile `Builds=` list.
    /// Structures a worker can place, resolved from its real def id.
    pub fn build_menu(&self, def_id: &str) -> Vec<wc3_data::MenuEntry> {
        self.objects
            .as_ref()
            .map(|objects| objects.build_menu(def_id))
            .unwrap_or_default()
    }

    /// Menu entry for any WC3 type id (used by tests and the touch layer).
    pub fn menu_entry(&self, id: &str) -> Option<wc3_data::MenuEntry> {
        self.objects.as_ref()?.menu_entry(id)
    }

    /// Gold cost from the typed data, falling back to the sim roster.
    pub fn unit_cost(&self, kind: UnitKind) -> u32 {
        self.unit_stats(kind)
            .and_then(|stats| stats.gold_cost)
            .filter(|cost| *cost > 0)
            .map(|cost| cost as u32)
            .unwrap_or_else(|| kind.stats().cost.max(0) as u32)
    }

    /// Whether the mounted install carries the Frozen Throne archives
    /// (`War3x.mpq` / `War3xlocal.mpq`).
    ///
    /// The expansion rewrites balance and tech rows (peasant build list gains
    /// `hvlt`, Divine Shield costs 25 mana instead of 125, several gold costs
    /// change), so every number assertion that depends on the patch level reads
    /// this instead of pinning one install.
    pub fn is_expansion(&self) -> bool {
        self.debug.iter().any(|line| {
            if !line.starts_with("storm_layers=") {
                return false;
            }
            let lower = line.to_ascii_lowercase();
            lower.contains("war3x.mpq") || lower.contains("war3xlocal.mpq")
        })
    }

    /// Model index for a roster def id (falls back to the placeholder table).
    pub fn model_index_for_def(&self, def_id: &str) -> usize {
        self.model_by_def.get(def_id).copied().unwrap_or_else(|| {
            self.models.iter().position(|model| model.fallback).unwrap_or(0)
        })
    }

    /// Per-definition model.
    ///
    /// A def this load did not resolve — outside the map's reachable set, or
    /// with no MDX in the install — returns its own bounded placeholder (a flat
    /// box sized from the def) and logs `GAME_MODEL_SKIP def=<id>
    /// reason=<not-reachable|unknown-def>` once per id. It never returns
    /// another unit's model.
    pub fn model_for_def(&self, def_id: &str) -> &ModelAsset {
        let index = self.model_index_for_def(def_id);
        let asset = &self.models[index];
        if asset.fallback {
            let reason = if wc3_sim::data::def_of(def_id).is_some() {
                "not-reachable"
            } else {
                "unknown-def"
            };
            self.note_model_skip(def_id, reason);
        }
        asset
    }

    /// Whether a def currently resolves to the placeholder instead of its MDX.
    pub fn is_placeholder(&self, def_id: &str) -> bool {
        self.models[self.model_index_for_def(def_id)].fallback
    }

    /// `GAME_MODEL_SKIP` lines collected so far (one per def id).
    pub fn model_skip_log(&self) -> Vec<String> {
        self.model_skip
            .lock()
            .map(|log| log.lines.clone())
            .unwrap_or_default()
    }

    /// Record a skipped def once and emit the device-log line the scene skips
    /// are audited by.
    fn note_model_skip(&self, def_id: &str, reason: &str) {
        let Ok(mut log) = self.model_skip.lock() else {
            return;
        };
        if !log.seen.insert(def_id.to_string()) {
            return;
        }
        let line = format!("GAME_MODEL_SKIP def={def_id} reason={reason}");
        log.lines.push(line.clone());
        drop(log);
        crate::dev_log!("{line}");
    }

    /// MDX-backed **definition** models in the table: the resident subset of
    /// [`Self::model_roster`]. The four starter-kind models are excluded, so
    /// `loaded <= roster` always holds and the number compares with the
    /// `GAME_READY models=… roster=…` pair.
    pub fn loaded_model_count(&self) -> usize {
        self.model_by_def
            .values()
            .filter(|index| {
                let model = &self.models[**index];
                !model.fallback && model.model.is_some()
            })
            .count()
    }

    /// Bounded placeholders standing in for defs this load did not resolve.
    pub fn placeholder_count(&self) -> usize {
        self.models.iter().filter(|model| model.fallback).count()
    }

    /// Parse one definition's MDX on demand and append it to the model table.
    ///
    /// [`Self::load`] already resolves every def the current map can show; this
    /// is the escape hatch for a def that only becomes known later (a script
    /// creating a unit the static scan could not see). It appends to
    /// [`Self::models`], so call it **before** `Renderer::new` snapshots the
    /// model table; after that the placeholder path is the defined behaviour.
    /// Returns `true` when the def has a real model afterwards.
    pub fn ensure_model(&mut self, def_id: &str) -> bool {
        let existing = self.model_by_def.get(def_id).copied();
        if existing
            .map(|index| !self.models[index].fallback)
            .unwrap_or(false)
        {
            return true;
        }
        let Some(provider) = self.audio.as_ref().map(|audio| audio.provider().clone()) else {
            return false;
        };
        let Some(path) = self
            .objects
            .as_ref()
            .and_then(|objects| {
                objects
                    .units()
                    .find(|view| view.id_str() == def_id)
                    .and_then(|view| view.model_file())
            })
        else {
            return false;
        };
        let Ok(model) = provider.model(&path) else {
            return false;
        };
        let tex_map = load_model_textures(&provider, &mut self.textures, &mut self.tex_cache, &model);
        let meshes = build_meshes(&model, &tex_map);
        if meshes.is_empty() {
            return false;
        }
        let asset = def_model_asset(def_id, model, meshes);
        // Replace the def's placeholder in place when it has a slot: the model
        // table keeps its indices for the renderer, and `model_index_for_def`
        // starts returning the real entry without any index churn.
        match existing {
            Some(index) => self.models[index] = asset,
            None => {
                let index = self.models.len();
                self.models.push(asset);
                self.model_by_def.insert(def_id.to_string(), index);
            }
        }
        true
    }

    pub fn model(&self, kind: UnitKind) -> &ModelAsset {
        self.models
            .iter()
            .find(|model| model.kind == kind)
            .unwrap_or(&self.models[0])
    }

    /// Bytes of decoded RGBA still held on the CPU (every layer).
    pub fn cpu_texture_bytes(&self) -> usize {
        let mut bytes: usize = self.textures.iter().map(TextureImage::cpu_bytes).sum();
        bytes += self
            .terrain_atlas
            .as_ref()
            .map(TextureImage::cpu_bytes)
            .unwrap_or(0);
        bytes += self
            .terrain_tile_images
            .iter()
            .map(TextureImage::cpu_bytes)
            .sum::<usize>();
        bytes += self
            .minimap
            .as_ref()
            .map(TextureImage::cpu_bytes)
            .unwrap_or(0);
        bytes
    }

    /// Vertices per mesh, for the diagnostics and the load line.
    pub fn vertex_total(&self) -> usize {
        self.models
            .iter()
            .flat_map(|model| model.meshes.iter())
            .map(MeshData::vertex_count)
            .sum()
    }

    /// Convert every mesh to the compact runtime format (20 B/vertex).
    pub fn compact_meshes(&mut self) {
        let mut before = 0usize;
        let mut after = 0usize;
        for mesh in self.models.iter_mut().flat_map(|model| model.meshes.iter_mut()) {
            before += mesh.cpu_bytes();
            mesh.compact();
            after += mesh.cpu_bytes();
        }
        crate::dev_log!(
            "GAME_MEM mesh_cpu_before={:.1} MB mesh_cpu_after={:.1} MB vertex_record={:.0} B bytes_per_vertex={:.1}",
            before as f64 / (1024.0 * 1024.0),
            after as f64 / (1024.0 * 1024.0),
            self.vertex_record_bytes(),
            self.bytes_per_vertex()
        );
    }

    /// Drop the mesh vertex buffers once the renderer owns the GPU copies.
    ///
    /// The animation path reads material layers, geoset ids and the sequence
    /// table — never the vertices — so a released mesh keeps drawing.
    pub fn release_cpu_meshes(&mut self) -> usize {
        let mut freed = 0usize;
        for mesh in self.models.iter_mut().flat_map(|model| model.meshes.iter_mut()) {
            freed += mesh.release_vertices();
        }
        crate::dev_log!(
            "GAME_MEM mesh_cpu_freed={:.1} MB",
            freed as f64 / (1024.0 * 1024.0)
        );
        freed
    }

    /// Drop the authored geometry once the runtime meshes own it.
    ///
    /// `MdxModel.geosets` is the raw f32/skin copy the loader flattened into
    /// [`MeshData`] (22.6 MB across the shipped roster, measured `mem_probe`
    /// 2026-11-15). The animator reads bones, helpers, pivots and the sequence
    /// table; the material path reads `geoset_animations` plus
    /// [`Geoset::sequence_bounds`] (which [`wc3_anim::runtime::mesh_alphas`]
    /// needs to know whether a geoset belongs to the running sequence). So every
    /// heavy field goes and the per-sequence extents stay.
    pub fn release_mdx_geometry(&mut self) -> usize {
        let mut freed = 0usize;
        for model in &mut self.models {
            let Some(mdx) = model.model.as_mut() else {
                continue;
            };
            for geoset in &mut mdx.geosets {
                freed += geoset.vertices.len() * 12
                    + geoset.normals.len() * 12
                    + geoset.faces.len() * 2
                    + geoset.uv_sets.iter().map(|set| set.len() * 8).sum::<usize>()
                    + geoset.skin.len() * 2
                    + geoset.matrix_groups.len() * 4
                    + geoset.matrix_indices.len() * 4
                    + geoset.face_groups.len() * 4
                    + geoset.face_type_groups.len() * 4
                    + geoset.vertex_groups.len();
                geoset.vertices = Vec::new();
                geoset.normals = Vec::new();
                geoset.faces = Vec::new();
                geoset.uv_sets = Vec::new();
                geoset.skin = Vec::new();
                geoset.matrix_groups = Vec::new();
                geoset.matrix_indices = Vec::new();
                geoset.face_groups = Vec::new();
                geoset.face_type_groups = Vec::new();
                geoset.vertex_groups = Vec::new();
            }
            freed += mdx.raw_chunks.iter().map(|(_, data)| data.len()).sum::<usize>();
            mdx.raw_chunks = Vec::new();
        }
        crate::dev_log!(
            "GAME_MEM mdx_geometry_freed={:.1} MB",
            freed as f64 / (1024.0 * 1024.0)
        );
        freed
    }

    /// Bytes of runtime vertex data across every model's meshes.
    pub fn mesh_bytes(&self) -> usize {
        self.models
            .iter()
            .flat_map(|model| model.meshes.iter())
            .map(MeshData::cpu_bytes)
            .sum()
    }

    /// Mean vertex-record size across the asset set (budget: <= 20 B).
    pub fn vertex_record_bytes(&self) -> f64 {
        let mut bytes = 0usize;
        let mut vertices = 0usize;
        for mesh in self.models.iter().flat_map(|model| model.meshes.iter()) {
            bytes += mesh.vertex_record_bytes() * mesh.vertex_count();
            vertices += mesh.vertex_count();
        }
        if vertices == 0 {
            0.0
        } else {
            bytes as f64 / vertices as f64
        }
    }

    /// Mean bytes per vertex including index buffers and material metadata.
    pub fn bytes_per_vertex(&self) -> f64 {
        let mut bytes = 0usize;
        let mut vertices = 0usize;
        for mesh in self.models.iter().flat_map(|model| model.meshes.iter()) {
            bytes += mesh.cpu_bytes();
            vertices += mesh.vertex_count();
        }
        if vertices == 0 {
            0.0
        } else {
            bytes as f64 / vertices as f64
        }
    }

    /// Drop every decoded texture sheet now that the renderer owns the GPU
    /// copies. Dimensions, paths and indices stay, so draw lists keep working;
    /// only the pixels go.
    ///
    /// The asset layer is the single owner of the decoded art, and the shipped
    /// install keeps 203 MB of it (`mem_probe`, 2026-11-15). Nothing after
    /// upload reads `rgba` again: the nine-patch insets, atlas manifests and
    /// HUD canvases are all measured during [`Assets::load`].
    pub fn release_cpu_textures(&mut self) -> CpuTextureRelease {
        let mut report = CpuTextureRelease::default();
        let mut release = |image: &mut TextureImage| {
            let freed = image.release_cpu();
            if freed > 0 {
                report.surfaces += 1;
                report.bytes += freed;
            }
        };
        for image in &mut self.textures {
            release(image);
        }
        if let Some(atlas) = self.terrain_atlas.as_mut() {
            release(atlas);
        }
        for image in &mut self.terrain_tile_images {
            release(image);
        }
        if let Some(minimap) = self.minimap.as_mut() {
            release(minimap);
        }
        // The decoded asset caches hold a second copy of the same art (raw
        // archive bytes, BLP sheets, parsed MDX). The game layer already copied
        // what it draws, so these go now too.
        if let Some(audio) = self.audio.as_mut() {
            report.cache_bytes = audio.release_decoded_caches();
        }
        report.mesh_bytes = self.release_cpu_meshes();
        crate::dev_log!(
            "GAME_MEM textures_cpu_freed={:.1} MB surfaces={} asset_cache_freed={:.1} MB mesh_cpu_freed={:.1} MB",
            report.bytes as f64 / (1024.0 * 1024.0),
            report.surfaces,
            report.cache_bytes as f64 / (1024.0 * 1024.0),
            report.mesh_bytes as f64 / (1024.0 * 1024.0)
        );
        report
    }

    /// Terrain height at a world position, matching the rendered triangle
    /// split (not a bilinear patch).
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let width = self.map_width_cells as usize + 1;
        let height = self.map_height_cells as usize + 1;
        if width < 2 || height < 2 {
            return 0.0;
        }
        let gx = (x / TERRAIN_TILE_SIZE).clamp(0.0, (width - 1) as f32);
        let gz = (z / TERRAIN_TILE_SIZE).clamp(0.0, (height - 1) as f32);
        let x0 = gx.floor() as usize;
        let z0 = gz.floor() as usize;
        let x1 = (x0 + 1).min(width - 1);
        let z1 = (z0 + 1).min(height - 1);
        let tx = gx - x0 as f32;
        let tz = gz - z0 as f32;
        let sample = |cx: usize, cz: usize| -> f32 {
            let tile = self.terrain_tiles.get(cz * width + cx);
            // HiveWE final ground height: the TFT cliff layer is part of the
            // surface (`corner_height + layer_height - 2.0`), so units and
            // doodads stand on the rendered cliff top, not on the raw W3E
            // height under it.
            tile.map(|t| {
                (t.height() + (t.cliff_level() as f32 - 2.0)) * TERRAIN_HEIGHT_SCALE
            })
            .unwrap_or(0.0)
        };
        let h00 = sample(x0, z0);
        let h10 = sample(x1, z0);
        let h01 = sample(x0, z1);
        let h11 = sample(x1, z1);
        // Match the rendered terrain triangles exactly: the cell is split
        // along (x0,z0)->(x1,z1) by build_terrain, so bilinear interpolation
        // would put units/doodads up to a few units above or below the visible
        // surface. Both branches agree on the diagonal.
        if tx + tz <= 1.0 {
            h00 + (h10 - h00) * tx + (h01 - h00) * tz
        } else {
            h11 + (h01 - h11) * (1.0 - tx) + (h10 - h11) * (1.0 - tz)
        }
    }

    pub fn tile_at(&self, x: f32, z: f32) -> Option<&TerrainTile> {
        if self.terrain.is_none() {
            return None;
        }
        let width = self.map_width_cells as usize + 1;
        let gx = (x / TERRAIN_TILE_SIZE).clamp(0.0, self.map_width_cells as f32) as usize;
        let gz = (z / TERRAIN_TILE_SIZE).clamp(0.0, self.map_height_cells as f32) as usize;
        self.terrain_tiles.get(gz * width + gx)
    }

    /// Geoset alpha for each mesh of `model` at animation time `time_ms`.
    ///
    /// Warcraft 3 hides non-current geosets with a `KGAO` track that often has
    /// a **single** key with value 0 inside the sequence (Footman weapon/effect
    /// geosets, Archer effects, Peasant tools). The previous rule demanded two
    /// differing keys, so those single-key tracks were ignored and every
    /// death/effect geoset rendered at once — the floating sword and the
    /// "corpse behind every unit" in the device screenshots.
    ///
    /// Canonical rule (single implementation in `wc3_anim::runtime::mesh_alphas`):
    /// a geoset must be part of the sequence (per-sequence extent is not the
    /// FLT_MAX sentinel), then a track with **any** in-range key is sampled (a
    /// single 0 hides the geoset) and a track with no in-range key keeps alpha
    /// 1.0. There is no "all hidden -> show all" fallback: the TownHall `Stand`
    /// works because geosets 11/13/14 have no in-range key at all.
    pub fn mesh_alpha(
        &self,
        model_index: usize,
        sequence: Option<&Sequence>,
        time_ms: u32,
    ) -> Vec<f32> {
        let Some(model) = self.models.get(model_index) else {
            return Vec::new();
        };
        let Some(mdx) = &model.model else {
            return model
                .meshes
                .iter()
                .map(|mesh| if mesh.hide { 0.0 } else { 1.0 })
                .collect();
        };
        // A model with no running sequence draws its **default (Stand)** geoset
        // set, not the raw static `GEOA` alphas. With `None` every geoset of the
        // model is visible, and the Peon then renders its body plus the two
        // gold/lumber carry variants plus the gore geoset at once: the device
        // "two bodies, duplicated pauldron, coloured blob" defect. The engine's
        // own default frame is Stand, so `None` resolves to `model.idle`.
        let sequence =
            sequence.or_else(|| model.idle.and_then(|index| model.sequences.get(index)));
        mesh_alphas_for(mdx, &model.meshes, sequence, time_ms)
    }

    /// Per-mesh, per-layer alpha at the current sequence time. Static layer
    /// alpha always applies; an animated `KMTA` track applies only when it is
    /// a real variant of the current sequence (same rule as geoset alpha).
    pub fn mesh_layer_alpha(
        &self,
        model_index: usize,
        sequence: Option<&Sequence>,
        time_ms: u32,
    ) -> Vec<Vec<f32>> {
        let Some(model) = self.models.get(model_index) else {
            return Vec::new();
        };
        // Same default-sequence rule as `mesh_alpha`: an un-started unit shows
        // the material alphas of its Stand frame.
        let sequence =
            sequence.or_else(|| model.idle.and_then(|index| model.sequences.get(index)));
        model
            .meshes
            .iter()
            .map(|mesh| {
                mesh.layers
                    .iter()
                    .map(|layer| {
                        wc3_anim::runtime::layer_alpha(
                            layer.alpha_track.as_ref(),
                            layer.alpha,
                            sequence,
                            time_ms,
                        )
                    })
                    .collect()
            })
            .collect()
    }

    pub fn is_water(&self, x: f32, z: f32) -> bool {
        self.tile_at(x, z).map(|tile| tile.is_water()).unwrap_or(false)
    }
}

/// Per-geoset alpha for an arbitrary parsed MDX.
///
/// Delegates to `wc3_anim::runtime::mesh_alphas` (per-sequence extent gate +
/// any in-range `KGAO` key + helper plane). `meshes` is a projection of
/// `model.geosets` (`build_meshes`, which drops empty geosets), so the result is
/// re-indexed through `mesh.geoset_id`.
pub fn mesh_alphas_for(
    model: &MdxModel,
    meshes: &[MeshData],
    sequence: Option<&Sequence>,
    time_ms: u32,
) -> Vec<f32> {
    let mut hidden = vec![false; model.geosets.len()];
    for mesh in meshes {
        if let Some(slot) = hidden.get_mut(mesh.geoset_id as usize) {
            *slot = mesh.hide;
        }
    }
    let alphas = wc3_anim::runtime::mesh_alphas(model, &hidden, sequence, time_ms);
    meshes
        .iter()
        .map(|mesh| alphas.get(mesh.geoset_id as usize).copied().unwrap_or(1.0))
        .collect()
}

/// One geoset's visibility, delegating to the canonical rule.
///
/// Kept because the world audit and the asset tests address meshes directly;
/// `_living` is retained for signature compatibility (the corpse-texture
/// heuristic was removed - the sequence extent is authoritative).
pub fn geoset_alpha(
    model: &MdxModel,
    mesh: &MeshData,
    _living: bool,
    sequence: Option<&Sequence>,
    time_ms: u32,
) -> f32 {
    let mut hidden = vec![false; model.geosets.len()];
    if let Some(slot) = hidden.get_mut(mesh.geoset_id as usize) {
        *slot = mesh.hide;
    }
    wc3_anim::runtime::mesh_alphas(model, &hidden, sequence, time_ms)
        .get(mesh.geoset_id as usize)
        .copied()
        .unwrap_or(1.0)
}

/// Emit the one device-visible `GAME_AUDIO` line for QA (logcat marker).
fn log_audio_wiring(wiring: &GameAudioWiring) {
    crate::dev_log!("GAME_AUDIO {}", wiring.summary());
}

impl Assets {
    /// Gameplay audio hooks sharing this asset database.
    pub fn audio_mut(&mut self) -> Option<&mut WiredGameAudio> {
        self.audio.as_mut()
    }

    /// Direct handle on the `wc3-audio` engine behind the wired facade.
    ///
    /// The facade ([`WiredGameAudio`]) is the shared-database instantiation of
    /// `wc3_audio::game_audio::GameAudio`; HUD/QA controls that need the raw
    /// engine (voice statistics, category volumes) go through here.
    pub fn game_audio_mut(
        &mut self,
    ) -> Option<&mut wc3_audio::game_audio::GameAudio<SharedAssetDb>> {
        self.audio.as_mut()
    }

    /// Update the 3D listener used by positional sounds.
    pub fn update_audio_listener(&mut self, position: Vec3, yaw: f32) {
        if let Some(audio) = self.audio.as_mut() {
            audio.set_listener(position, yaw);
        }
    }

    /// Install the terrain height sampler used by the audio engine's occlusion.
    ///
    /// The grid holds the *final* W3E heights (cliff layers included, the same
    /// surface the terrain mesh renders), so the mixer's listener→source
    /// raycast sees real ridges and cliff walls. Runs once per asset load; the
    /// closure owns an `Arc` snapshot, so it stays valid for the session.
    pub fn install_audio_occluder(&mut self) {
        let Some(audio) = self.audio.as_mut() else {
            return;
        };
        let width = self.map_width_cells.max(1) as usize + 1;
        let height = self.map_height_cells.max(1) as usize + 1;
        if width < 2 || height < 2 || self.terrain_tiles.len() < width * height {
            return;
        }
        let mut heights = Vec::with_capacity(width * height);
        for z in 0..height {
            for x in 0..width {
                let tile = &self.terrain_tiles[z * width + x];
                heights.push(
                    (tile.height() + (tile.cliff_level() as f32 - 2.0)) * TERRAIN_HEIGHT_SCALE,
                );
            }
        }
        let max_x = (width - 1) as f32;
        let max_z = (height - 1) as f32;
        let heights = std::sync::Arc::new(heights);
        let sampler: wc3_audio::HeightSampler =
            std::sync::Arc::new(move |point: Vec3| -> f32 {
                let gx = (point.x / TERRAIN_TILE_SIZE).clamp(0.0, max_x);
                let gz = (point.z / TERRAIN_TILE_SIZE).clamp(0.0, max_z);
                let x0 = gx.floor() as usize;
                let z0 = gz.floor() as usize;
                let x1 = (x0 + 1).min(width - 1);
                let z1 = (z0 + 1).min(height - 1);
                let tx = gx - x0 as f32;
                let tz = gz - z0 as f32;
                let h00 = heights[z0 * width + x0];
                let h10 = heights[z0 * width + x1];
                let h01 = heights[z1 * width + x0];
                let h11 = heights[z1 * width + x1];
                let top = h00 + (h10 - h00) * tx;
                let bottom = h01 + (h11 - h01) * tx;
                top + (bottom - top) * tz
            });
        audio.set_occluder(Some(sampler));
    }

    /// UI feedback through the wired hooks (button click).
    pub fn play_ui_click(&mut self) {
        if let Some(audio) = self.audio.as_mut() {
            let _ = audio.ui_click();
        }
    }

    /// Music theme override (retail theme name).
    pub fn set_music(&mut self, theme: &str) {
        if let Some(audio) = self.audio.as_mut() {
            let _ = audio.music(theme);
        }
    }
}

impl crate::ui::UiResourceResolver for SharedAssetDb {
    fn read_ui_text(&mut self, path: &str) -> Option<String> {
        let normalized = path.replace('/', "\\");
        let bytes = self.read(&normalized).or_else(|_| self.read(path)).ok()?;
        if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
            let units: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            return Some(String::from_utf16_lossy(&units));
        }
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn read_ui_texture(&mut self, path: &str) -> Option<TextureImage> {
        let normalized = path.replace('/', "\\");
        let image = self
            .texture(&normalized)
            .or_else(|_| self.texture(path))
            .ok()?;
        Some(TextureImage {
            width: image.width,
            height: image.height,
            rgba: image.rgba,
        })
    }
}

fn load_model_textures(
    db: &SharedAssetDb,
    textures: &mut Vec<TextureImage>,
    cache: &mut HashMap<String, usize>,
    model: &MdxModel,
) -> Vec<usize> {
    let mut map = Vec::with_capacity(model.textures.len());
    for texture in &model.textures {
        // MDX `ReplaceableId`: 1 = team colour, 2 = team glow. The *player*
        // colour travels in the unit tint (`UnitDraw::tint`), which the shader
        // multiplies into this layer only (`MeshLayer::team_color`), so the slot
        // binds the white mask (texture 0). Binding
        // `ReplaceableTextures\TeamColor\TeamColor<NN>.blp` here and keeping the
        // tint would double-tint: those sheets already carry the final colour.
        // Ids above 2 (selection/weather overlays) also take the white mask and
        // are not tinted.
        if texture.replaceable_id != 0 {
            map.push(REPLACEABLE_MASK_TEXTURE);
            continue;
        }
        let path = texture.path.trim_end_matches('\0').replace('/', "\\");
        if path.is_empty() {
            map.push(0);
            continue;
        }
        let key = path.to_ascii_lowercase();
        if let Some(&index) = cache.get(&key) {
            map.push(index);
            continue;
        }
        match db.texture(&path) {
            Ok(image) => {
                let image = resize_to(&image, 256, 256);
                textures.push(image);
                let index = textures.len() - 1;
                cache.insert(key, index);
                map.push(index);
            }
            Err(_) => map.push(0),
        }
    }
    map
}

pub(crate) fn build_meshes(model: &MdxModel, texture_map: &[usize]) -> Vec<MeshData> {
    let mut meshes = Vec::new();
    for (geoset_id, geoset) in model.geosets.iter().enumerate() {
        let mut mesh = build_mesh(geoset, model, texture_map);
        mesh.geoset_id = geoset_id as u32;
        if !mesh.indices.is_empty() && !mesh.positions.is_empty() {
            // Helper-plane rule (canonical: `wc3_anim::runtime::helper_plane`).
            mesh.hide = wc3_anim::runtime::helper_plane(geoset);
            meshes.push(std::mem::take(&mut mesh));
        }
    }
    meshes
}

fn build_mesh(geoset: &Geoset, model: &MdxModel, texture_map: &[usize]) -> MeshData {
    // WC3 materials are layer stacks rendered in order. Keep every layer with
    // its texture, static alpha, animated alpha and filter mode; selecting only
    // layer 0 turned the Footman's team-colour geoset into an untextured white
    // blob and dropped the diffuse layer entirely.
    let mut layers: Vec<MeshLayer> = Vec::new();
    let mut texture_paths: Vec<String> = Vec::new();
    if let Some(material) = model.materials.get(geoset.material_id as usize) {
        for (layer_index, layer) in material.layers.iter().enumerate() {
            let Some(&texture) = texture_map.get(layer.texture_id as usize) else {
                continue;
            };
            let source = model.textures.get(layer.texture_id as usize);
            // Only replaceable ids 1/2 are team colour / team glow; ids 21+
            // are selection/weather overlays that must not be tinted.
            let team_color = source
                .map(|texture| texture.replaceable_id == 1 || texture.replaceable_id == 2)
                .unwrap_or(false);
            if let Some(source) = source {
                let path = source.path.trim_end_matches('\0').replace('/', "\\");
                texture_paths.push(path.to_ascii_lowercase());
                if team_color {
                    texture_paths.push("teamcolor".to_string());
                }
            }
            layers.push(MeshLayer {
                texture,
                alpha: layer.alpha,
                alpha_track: layer.alpha_track.clone(),
                blend: layer.blend_mode,
                team_color,
                // Engine two-unit contract: layer *n* samples TEXCOORD set *n*
                // (`TEXCOORDINDEX := unit`; r2 lane 405). The geoset carries the
                // sets; a missing second set falls back to the first in the
                // renderer.
                uv_set: layer_index.min(1) as u32,
            });
        }
    }
    if layers.is_empty() {
        layers.push(MeshLayer::plain(0));
    }
    let texture = layers[0].texture;
    // Canonical weight resolution (`wc3_anim::GeosetSkin`): matrix groups use
    // equal weights, SKIN pairs use their 0..255 weights, both normalised per
    // vertex. The engine evaluates a matrix group whole (up to five matrices,
    // `CMatrixGroup`); the renderer keeps the full list and skins geosets with a
    // fifth influence on the CPU instead of dropping it.
    let skin = wc3_anim::GeosetSkin::from_geoset(geoset);
    let mut bones: Vec<Vec<BoneWeight>> = vec![Vec::new(); geoset.vertices.len()];
    for (vertex, list) in skin.influences.iter().enumerate() {
        if let Some(slot) = bones.get_mut(vertex) {
            *slot = list
                .iter()
                .map(|&(bone, weight)| BoneWeight { bone, weight })
                .collect();
        }
    }
    let mut normals = geoset.normals.clone();
    if normals.len() != geoset.vertices.len() {
        normals = vec![Vec3::Y; geoset.vertices.len()];
        for triangle in geoset.faces.chunks_exact(3) {
            let (a, b, c) = (
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            );
            let (Some(&va), Some(&vb), Some(&vc)) = (
                geoset.vertices.get(a),
                geoset.vertices.get(b),
                geoset.vertices.get(c),
            ) else {
                continue;
            };
            let normal = (vb - va).cross(vc - va).normalize_or_zero();
            for index in [a, b, c] {
                if let Some(slot) = normals.get_mut(index) {
                    *slot += normal;
                }
            }
        }
        for normal in &mut normals {
            *normal = normal.normalize_or_zero();
            if normal.length_squared() < 0.01 {
                *normal = Vec3::Y;
            }
        }
    }
    let uvs = geoset
        .uv_sets
        .first()
        .cloned()
        .unwrap_or_else(|| vec![Vec2::ZERO; geoset.vertices.len()]);
    let uvs = if uvs.len() == geoset.vertices.len() {
        uvs
    } else {
        vec![Vec2::ZERO; geoset.vertices.len()]
    };
    // TEXCOORD1: the second authored set (layer 1 samples it). Empty when the
    // geoset ships a single set; the renderer then reuses `uvs`.
    let uvs2 = geoset
        .uv_sets
        .get(1)
        .filter(|set| set.len() == geoset.vertices.len())
        .cloned()
        .unwrap_or_default();
    // MDX face lists contain padding/degenerate entries (0xFFFF, repeated
    // corners). Passing those to the GPU reads vertices from the next mesh or
    // ring in the shared buffer and draws the long stray triangles seen on the
    // first screenshots.
    let vertex_count = geoset.vertices.len() as u32;
    let mut indices = Vec::with_capacity(geoset.faces.len());
    for triangle in geoset.faces.chunks_exact(3) {
        let (a, b, c) = (
            triangle[0] as u32,
            triangle[1] as u32,
            triangle[2] as u32,
        );
        if a < vertex_count && b < vertex_count && c < vertex_count && a != b && b != c && a != c {
            indices.extend_from_slice(&[a, b, c]);
        }
    }
    MeshData {
        positions: geoset.vertices.clone(),
        normals,
        uvs,
        uvs2,
        indices,
        bones,
        layers,
        texture,
        texture_paths,
        z_up: true,
        geoset_id: 0,
        hide: false,
        compact: None,
    }
}

fn fallback_model(kind: UnitKind) -> ModelAsset {
    let (scale, height) = match kind {
        UnitKind::Footman => (Vec3::new(48.0, 110.0, 48.0), 110.0),
        UnitKind::Archer => (Vec3::new(42.0, 105.0, 42.0), 105.0),
        UnitKind::Peasant => (Vec3::new(40.0, 95.0, 40.0), 95.0),
        UnitKind::TownHall => (Vec3::new(280.0, 240.0, 280.0), 240.0),
    };
    let mesh = cube_mesh(scale);
    ModelAsset {
        kind,
        model: None,
        meshes: vec![mesh],
        sequences: Vec::new(),
        idle: None,
        walk: None,
        attack: None,
        height,
        pivot: 0.0,
        scale: 1.0,
        yaw_offset: yaw_offset_for(kind),
        fallback: true,
    }
}

/// Wrap a parsed MDX into the runtime [`ModelAsset`]: the compact meshes, the
/// Stand/Walk/Attack sequence picks and the presentation height/pivot.
///
/// Shared by the map-scoped load and [`Assets::ensure_model`], so a model
/// parsed on demand is indistinguishable from one loaded with the map.
fn def_model_asset(id: &str, model: MdxModel, meshes: Vec<MeshData>) -> ModelAsset {
    let sequences = model.sequences.clone();
    let idle = sequences
        .iter()
        .position(|s| s.name.to_ascii_lowercase().contains("stand"));
    let walk = sequences
        .iter()
        .position(|s| s.name.to_ascii_lowercase().contains("walk"));
    let attack = sequences
        .iter()
        .position(|s| s.name.to_ascii_lowercase().contains("attack"));
    let stand = idle.and_then(|index| sequences.get(index));
    let stand_alphas = mesh_alphas_for(&model, &meshes, stand, stand.map(|s| s.start).unwrap_or(0));
    let mut visible_min = f32::INFINITY;
    let mut visible_max = f32::NEG_INFINITY;
    for (mesh_index, mesh) in meshes.iter().enumerate() {
        if mesh.hide || stand_alphas.get(mesh_index).copied().unwrap_or(1.0) <= 0.01 {
            continue;
        }
        for position in &mesh.positions {
            visible_min = visible_min.min(position.z);
            visible_max = visible_max.max(position.z);
        }
    }
    let height = if visible_max.is_finite()
        && visible_min.is_finite()
        && visible_max - visible_min > 8.0
        && visible_max - visible_min < 600.0
    {
        visible_max - visible_min
    } else {
        120.0
    };
    // WC3 models put their origin at the feet. A geometry min outside a sane
    // band comes from hidden Birth/rubble geosets and used to lift units or
    // stretch foundations.
    let pivot = if visible_min.is_finite() && visible_min > -64.0 && visible_min < 64.0 {
        visible_min
    } else {
        0.0
    };
    let kind = wc3_sim::data::def_of(id)
        .map(|def| def.kind)
        .unwrap_or(UnitKind::Footman);
    ModelAsset {
        kind,
        model: Some(model),
        meshes,
        sequences,
        idle,
        walk,
        attack,
        height,
        pivot,
        scale: 1.0,
        yaw_offset: yaw_offset_for(kind),
        fallback: false,
    }
}

/// Bounded stand-in for a definition whose MDX is not resident.
///
/// One flat box, sized from the def's own footprint/kind (a 1×1 unit is a
/// 48×110 box, a 4×4 building a 256-wide block), so a unit the map scope did
/// not expect stays visible and correctly placed instead of wearing another
/// unit's art. Bounded: no def can produce a box wider than 320 world units.
fn placeholder_model(def: Option<&wc3_sim::data::UnitDef>) -> ModelAsset {
    let kind = def.map(|def| def.kind).unwrap_or(UnitKind::Footman);
    let footprint = def.map(|def| def.footprint.max(1) as f32).unwrap_or(1.0);
    let building = def.map(|def| def.is_building()).unwrap_or(false);
    let (width, height) = if building {
        (
            (footprint * 64.0).clamp(32.0, 320.0),
            (64.0 + footprint * 32.0).clamp(64.0, 300.0),
        )
    } else {
        let height = match kind {
            UnitKind::Footman => 110.0,
            UnitKind::Archer => 105.0,
            UnitKind::Peasant => 95.0,
            UnitKind::TownHall => 240.0,
        };
        (48.0, height)
    };
    let mesh = cube_mesh(Vec3::new(width, height, width));
    ModelAsset {
        kind,
        model: None,
        meshes: vec![mesh],
        sequences: Vec::new(),
        idle: None,
        walk: None,
        attack: None,
        height,
        pivot: 0.0,
        scale: 1.0,
        yaw_offset: yaw_offset_for(kind),
        fallback: true,
    }
}
/// Model-space yaw correction for WC3 models authored with the back along +X.
/// The Archer runs backwards without it (device screenshot 2026-11-12).
fn yaw_offset_for(_kind: UnitKind) -> f32 {
    // Sequence-scoped sampling fixed the real "Archer runs backwards" bug:
    // the global timeline clamped Walk to a Death-only root yaw of 197°. With
    // the root track clipped to the playing sequence the authored facing is
    // correct, so no model needs a presentation yaw offset.
    0.0
}

fn cube_mesh(scale: Vec3) -> MeshData {
    let h = scale * 0.5;
    let positions = vec![
        Vec3::new(-h.x, 0.0, -h.z),
        Vec3::new(h.x, 0.0, -h.z),
        Vec3::new(h.x, 0.0, h.z),
        Vec3::new(-h.x, 0.0, h.z),
        Vec3::new(-h.x, h.y, -h.z),
        Vec3::new(h.x, h.y, -h.z),
        Vec3::new(h.x, h.y, h.z),
        Vec3::new(-h.x, h.y, h.z),
    ];
    let normals = vec![
        Vec3::Y,
        Vec3::Y,
        Vec3::Y,
        Vec3::Y,
        Vec3::Y,
        Vec3::Y,
        Vec3::Y,
        Vec3::Y,
    ];
    let uvs = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 1.0),
    ];
    let indices = vec![
        0, 2, 1, 0, 3, 2, // bottom
        4, 5, 6, 4, 6, 7, // top
        0, 1, 5, 0, 5, 4, // front
        1, 2, 6, 1, 6, 5, // right
        2, 3, 7, 2, 7, 6, // back
        3, 0, 4, 3, 4, 7, // left
    ];
    MeshData {
        positions,
        normals,
        uvs,
        uvs2: Vec::new(),
        indices,
        bones: vec![Vec::new(); 8],
        layers: vec![MeshLayer::plain(0)],
        texture: 0,
        texture_paths: Vec::new(),
        z_up: false,
        geoset_id: 0,
        hide: false,
        compact: None,
    }
}

fn average_color(image: &TextureImage) -> [u8; 3] {
    let mut sum = [0u64; 3];
    let mut count = 0u64;
    for pixel in image.rgba.chunks_exact(4) {
        if pixel[3] > 8 {
            for channel in 0..3 {
                sum[channel] += pixel[channel] as u64;
            }
            count += 1;
        }
    }
    if count == 0 {
        return [96, 110, 72];
    }
    [
        (sum[0] / count) as u8,
        (sum[1] / count) as u8,
        (sum[2] / count) as u8,
    ]
}

fn terrain_art_path(table: &SlkTable, tag: &str) -> Option<String> {
    let dir_col = table
        .columns
        .iter()
        .position(|column| column.eq_ignore_ascii_case("dir"))?;
    let file_col = table
        .columns
        .iter()
        .position(|column| column.eq_ignore_ascii_case("file"))?;
    for row in &table.rows {
        if row.iter().any(|cell| cell == tag) {
            let dir = row.get(dir_col)?;
            let file = row.get(file_col)?;
            if dir.is_empty() || file.is_empty() {
                continue;
            }
            return Some(format!("{dir}\\{file}.blp"));
        }
    }
    None
}

struct AtlasBuilder {
    width: u32,
    height: u32,
    tile: u32,
    cols: u32,
    rows: u32,
    rgba: Vec<u8>,
    count: u32,
    uv: Vec<[f32; 4]>,
    /// Ground colour under transparent texels (WC3 terrain layers are alpha
    /// masks over the ground; rendering only the RGB showed black holes).
    base: [u8; 3],
}

impl AtlasBuilder {
    fn new(cols: u32, rows: u32, tile: u32) -> Self {
        Self {
            width: cols * tile,
            height: rows * tile,
            tile,
            cols,
            rows,
            rgba: vec![0; (cols * tile * rows * tile * 4) as usize],
            count: 0,
            uv: Vec::new(),
            base: [90, 80, 60],
        }
    }

    fn push(&mut self, image: &TextureImage) -> Option<usize> {
        let capacity = (self.cols * self.rows) as usize;
        if self.count as usize >= capacity {
            return None;
        }
        let slot = self.count as usize;
        let col = slot as u32 % self.cols;
        let row = slot as u32 / self.cols;
        let dst_x = col * self.tile;
        let dst_y = row * self.tile;
        for y in 0..self.tile {
            for x in 0..self.tile {
                let src_x = (x as u64 * image.width as u64 / self.tile as u64) as u32;
                let src_y = (y as u64 * image.height as u64 / self.tile as u64) as u32;
                let src = ((src_y.min(image.height - 1) * image.width + src_x.min(image.width - 1))
                    * 4) as usize;
                let dst = (((dst_y + y) * self.width + dst_x + x) * 4) as usize;
                let alpha = image.rgba[src + 3];
                if alpha == 0 {
                    self.rgba[dst..dst + 3].copy_from_slice(&self.base);
                } else {
                    let a = alpha as u16;
                    for channel in 0..3 {
                        let src_color = image.rgba[src + channel] as u16;
                        let base = self.base[channel] as u16;
                        self.rgba[dst + channel] =
                            ((src_color * a + base * (255 - a)) / 255) as u8;
                    }
                }
                self.rgba[dst + 3] = 255;
            }
        }
        // A one-texel inset stops linear filtering from bleeding between tiles.
        let inset = 1.0 / self.width as f32;
        let u0 = dst_x as f32 / self.width as f32 + inset;
        let v0 = dst_y as f32 / self.height as f32 + inset;
        let u1 = (dst_x + self.tile) as f32 / self.width as f32 - inset;
        let v1 = (dst_y + self.tile) as f32 / self.height as f32 - inset;
        self.uv.push([u0, v0, u1, v1]);
        self.count += 1;
        Some(slot)
    }

    fn finish(self) -> Option<(TextureImage, Vec<[f32; 4]>)> {
        if self.count == 0 {
            return None;
        }
        Some((
            TextureImage {
                width: self.width,
                height: self.height,
                rgba: self.rgba,
            },
            self.uv,
        ))
    }
}

fn resize_to(image: &BlpImage, width: u32, height: u32) -> TextureImage {
    if image.width == width && image.height == height {
        return TextureImage {
            width,
            height,
            rgba: image.rgba.clone(),
        };
    }
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let sx0 = (x as u64 * image.width as u64 / width as u64) as u32;
            let sx1 = (((x + 1) as u64 * image.width as u64 / width as u64) as u32)
                .max(sx0 + 1)
                .min(image.width);
            let sy0 = (y as u64 * image.height as u64 / height as u64) as u32;
            let sy1 = (((y + 1) as u64 * image.height as u64 / height as u64) as u32)
                .max(sy0 + 1)
                .min(image.height);
            let mut acc = [0u32; 4];
            let mut count = 0u32;
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let index = ((sy * image.width + sx) * 4) as usize;
                    if index + 3 < image.rgba.len() {
                        for channel in 0..4 {
                            acc[channel] += image.rgba[index + channel] as u32;
                        }
                        count += 1;
                    }
                }
            }
            let dst = ((y * width + x) * 4) as usize;
            if count > 0 {
                for channel in 0..4 {
                    rgba[dst + channel] = (acc[channel] / count) as u8;
                }
            }
        }
    }
    TextureImage {
        width,
        height,
        rgba,
    }
}

fn procedural_tiles(width: u32, height: u32) -> Vec<TerrainTile> {
    let mut tiles = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        for x in 0..width {
            let edge = x == 0 || y == 0 || x + 1 == width || y + 1 == height;
            let low = (x as f32 * 0.12).sin() + (y as f32 * 0.09).cos();
            let heights = 8192.0 + low * 90.0;
            tiles.push(TerrainTile {
                height_data: heights as u16,
                water_data: 0x2000,
                texture_flags: if edge { 0x08 } else { 0 },
                variation_data: 0,
                cliff_data: 0,
            });
        }
    }
    tiles
}

fn build_minimap(
    tiles: &[TerrainTile],
    cells_w: u32,
    cells_h: u32,
    pathing: Option<&wc3_formats::wpm::PathingMap>,
) -> TextureImage {
    const SIZE: u32 = 256;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let width = cells_w + 1;
    let height = cells_h + 1;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let tx = (x as u64 * width as u64 / SIZE as u64) as u32;
            let ty = (y as u64 * height as u64 / SIZE as u64) as u32;
            let tile = tiles.get((ty * width + tx) as usize);
            let (r, g, b) = match tile {
                Some(tile) if tile.is_water() => (32, 86, 138),
                Some(tile) => {
                    let height = tile.height();
                    let normalized = ((height + 4.0) / 8.0).clamp(0.0, 1.0);
                    let green = (78.0 + normalized * 76.0) as u8;
                    let red = (52.0 + normalized * 54.0) as u8;
                    let blue = (34.0 + normalized * 26.0) as u8;
                    (red, green, blue)
                }
                None => (40, 60, 40),
            };
            let (mut r, mut g, mut b) = (r, g, b);
            if let Some(pathing) = pathing {
                let px = (x as u64 * pathing.width as u64 / SIZE as u64) as u32;
                let py = (y as u64 * pathing.height as u64 / SIZE as u64) as u32;
                if !pathing.walkable(px, py) {
                    r = (r as u16 * 3 / 4) as u8;
                    g = (g as u16 * 3 / 4) as u8;
                    b = (b as u16 * 3 / 4) as u8;
                }
            }
            let index = ((y * SIZE + x) * 4) as usize;
            rgba[index] = r;
            rgba[index + 1] = g;
            rgba[index + 2] = b;
            rgba[index + 3] = 255;
        }
    }
    TextureImage {
        width: SIZE,
        height: SIZE,
        rgba,
    }
}

/// Number of terrain tiles in the parsed fixture (used by tests).
pub fn terrain_tile_count(terrain: &Terrain) -> usize {
    let expected = terrain.width as usize * terrain.height as usize;
    let _ = TILE_SIZE;
    expected
}

#[cfg(test)]
mod geoaset_tests {
    use super::*;
    use std::path::PathBuf;

    fn data() -> Option<(PathBuf, PathBuf)> {
        let mpq = [
            "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
            "/data/data/org.herofolends.p0/files/War3.mpq",
        ]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())?;
        let map = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Maps/Campaign/Human04.w3m");
        if !map.exists() {
            return None;
        }
        Some((mpq, map))
    }

    fn index_of(assets: &Assets, kind: UnitKind) -> usize {
        assets
            .models
            .iter()
            .position(|model| model.kind == kind)
            .unwrap_or_else(|| panic!("{kind:?} model present"))
    }

    fn stand(assets: &Assets, kind: UnitKind) -> Sequence {
        let model = assets.model(kind);
        model
            .sequences
            .iter()
            .find(|sequence| sequence.name.eq_ignore_ascii_case("Stand"))
            .or_else(|| {
                model
                    .sequences
                    .iter()
                    .find(|sequence| sequence.name.to_ascii_lowercase().contains("stand"))
            })
            .cloned()
            .unwrap_or_else(|| panic!("{kind:?} Stand sequence"))
    }

    #[test]
    fn spell_numbers_come_from_the_sheet() {
        let Some((mpq, map)) = data() else {
            eprintln!("SKIP spell_numbers_come_from_the_sheet: install/map not present");
            return;
        };
        let assets = Assets::load(Some(&mpq), Some(&map));
        let database = assets.objects.as_ref().expect("typed database");

        // Every simulated spell must exist in AbilityData with generated numbers.
        for spell in wc3_sim::abilities::Spell::ALL {
            let id = spell.def().id;
            let numbers = database
                .ability_numbers(id)
                .unwrap_or_else(|| panic!("{id} missing from AbilityData"));
            let level = numbers.first_level().unwrap_or_else(|| panic!("{id} levels"));
            let def = spell.def();
            assert_eq!(def.mana, level.mana.unwrap_or(0), "{id} mana");
            let ticks = (level.cooldown.unwrap_or(0.0) * wc3_sim::TICKS_PER_SECOND as f32).round();
            assert_eq!(def.cooldown_ticks as f32, ticks, "{id} cooldown");
            assert_eq!(def.radius, level.area.unwrap_or(0.0), "{id} radius");
            assert_eq!(def.range, level.range.unwrap_or(0.0), "{id} range");
        }

        // Spot values straight from `AbilityData.slk`. The expansion rewrites
        // several costs, so the expectation follows the mounted install and is
        // additionally cross-checked against the sheet row in the loop above.
        let expansion = assets.is_expansion();
        let blizzard = wc3_sim::abilities::Spell::from_id("AHbz").expect("blizzard");
        assert_eq!(blizzard.def().mana, 75, "Blizzard Cost1 (RoC and TFT)");
        assert_eq!(blizzard.def().cooldown_ticks, 180, "Cool1 = 6 s");
        assert_eq!(blizzard.def().radius, 200.0, "Area1");
        assert_eq!(blizzard.def().range, 800.0, "Rng1");
        let holy_light = wc3_sim::abilities::Spell::from_id("AHhb").expect("holy light");
        assert_eq!(holy_light.def().mana, 75, "Holy Light Cost1 (RoC and TFT)");
        assert_eq!(holy_light.def().cooldown_ticks, 150, "Cool1 = 5 s");
        let shield = wc3_sim::abilities::Spell::from_id("AHds").expect("divine shield");
        let shield_cost = if expansion { 25 } else { 125 };
        assert_eq!(
            shield.def().mana,
            shield_cost,
            "Divine Shield Cost1 (TFT 25 / RoC 125)"
        );
        let sheet_mana = database
            .ability_numbers("AHds")
            .and_then(|numbers| numbers.first_level().map(|level| level.mana))
            .flatten();
        assert_eq!(
            Some(shield_cost),
            sheet_mana,
            "Divine Shield Cost1 must be the loaded sheet row"
        );
        // `Dur1` is 10 s in RoC and 15 s in TFT.
        let shield_ticks = if expansion { 450 } else { 300 };
        assert_eq!(
            shield.def().duration_ticks,
            shield_ticks,
            "Divine Shield Dur1 (TFT 15 s / RoC 10 s)"
        );
        let sheet_duration = database
            .ability_numbers("AHds")
            .and_then(|numbers| numbers.first_level().map(|level| level.duration))
            .flatten();
        let sheet_ticks = (sheet_duration.unwrap_or(0.0)
            * wc3_sim::TICKS_PER_SECOND as f32)
            .round() as u32;
        assert_eq!(
            shield.def().duration_ticks, sheet_ticks,
            "Divine Shield Dur1 must be the loaded sheet row"
        );
    }

    /// The gold sack is the carry geoset of `Walk Gold` / `Stand Gold` /
    /// `stand work gold` (Peon.mdx ships all of them). Selecting the plain
    /// sequence made a hauling worker look empty-handed.
    #[test]
    fn worker_carry_sequences_resolve_to_the_gold_and_lumber_variants() {
        let Some((mpq, map)) = data() else {
            eprintln!("SKIP worker_carry_sequences: install/map not present");
            return;
        };
        let assets = Assets::load(Some(&mpq), Some(&map));
        let peon = assets.model_for_def("opeo");
        let name = |kind: SequenceKind, carry: Carry| {
            peon.sequence_with(kind, carry)
                .and_then(|index| peon.sequences.get(index))
                .map(|sequence| sequence.name.to_ascii_lowercase())
                .unwrap_or_default()
        };
        assert!(name(SequenceKind::Walk, Carry::Gold).contains("walk gold"), "{}", name(SequenceKind::Walk, Carry::Gold));
        assert!(name(SequenceKind::Walk, Carry::Lumber).contains("walk lumber"));
        assert!(name(SequenceKind::Idle, Carry::Gold).contains("stand gold"), "{}", name(SequenceKind::Idle, Carry::Gold));
        assert!(name(SequenceKind::Work, Carry::Gold).contains("work gold"));
        assert!(name(SequenceKind::Work, Carry::Lumber).contains("work lumber"));
        // Empty-handed workers keep the plain clips.
        assert_eq!(name(SequenceKind::Walk, Carry::None), "walk");
        // A soldier has no carry set: the lookup must fall back, not vanish.
        let footman = assets.model_for_def("hfoo");
        let empty = footman
            .sequence_with(SequenceKind::Walk, Carry::Gold)
            .and_then(|index| footman.sequences.get(index))
            .map(|sequence| sequence.name.to_ascii_lowercase())
            .unwrap_or_default();
        assert!(empty.contains("walk"), "footman walk fallback: {empty}");
    }

    #[test]
    fn menus_are_driven_by_the_typed_database() {
        let Some((mpq, map)) = data() else {
            eprintln!("SKIP menus_are_driven_by_the_typed_database: install/map not present");
            return;
        };
        let assets = Assets::load(Some(&mpq), Some(&map));

        // Town hall -> peasants (`HumanUnitFunc.txt: [htow] Trains = hpea`).
        let hall = assets.train_menu("htow");
        assert_eq!(hall.len(), 1, "town hall trains one entry: {hall:?}");
        assert_eq!(hall[0].id, "hpea");
        assert!(!hall[0].name.is_empty());
        assert!(hall[0].gold.is_some(), "peasant cost");
        assert_eq!(hall[0].hotkey_label().len(), 1, "hotkey letter");

        // Peasant -> build list (`[hpea] Builds = htow,hhou,…`). The entry count
        // follows the mounted install: the expansion appends `hvlt` (Магическая
        // лавка) to the ten base structures.
        let build = assets.build_menu("hpea");
        let ids: Vec<&str> = build.iter().map(|entry| entry.id.as_str()).collect();
        assert_eq!(ids[0], "htow", "first structure: {ids:?}");
        for required in ["hhou", "hbar", "hlum"] {
            assert!(
                ids.contains(&required),
                "{required} is buildable by a peasant: {ids:?}"
            );
        }
        // RoC ships ten; TFT adds the Arcane Vault. Both counts must match the
        // install that is actually mounted, not one pinned patch level.
        let expansion = assets.is_expansion();
        assert_eq!(
            ids.contains(&"hvlt"),
            expansion,
            "Arcane Vault follows the mounted expansion: {ids:?}"
        );
        assert_eq!(
            build.len(),
            if expansion { 11 } else { 10 },
            "peasant build list: {ids:?}"
        );
        for entry in &build {
            assert!(entry.is_complete(), "{} has name/icon/gold", entry.id);
        }

        // Barracks -> three trainees with hotkeys from the profile text.
        let barracks = assets.menu_entry("hbar").expect("barracks");
        assert!(barracks.is_complete());

        // The icon alias table makes typed ids resolvable by the command card.
        assert!(
            assets.ui.type_icon("hfoo").is_some(),
            "footman icon alias"
        );
        assert!(assets.ui.type_icon("hhou").is_some(), "farm icon alias");
    }

    #[test]
    fn typed_object_data_feeds_game_assets() {
        let Some((mpq, map)) = data() else {
            eprintln!("SKIP typed_object_data_feeds_game_assets: install/map not present");
            return;
        };
        let assets = Assets::load(Some(&mpq), Some(&map));
        let database = assets
            .objects
            .as_ref()
            .expect("typed object database from the install");
        assert!(
            database.counts().iter().map(|(_, rows)| rows).sum::<usize>() > 2_000,
            "object tables loaded: {:?}",
            database.counts()
        );
        println!("OBJECTS {:?}", assets.debug.last());

        // FourCC bridge: the game enum resolves to the shipped object id.
        assert_eq!(Assets::unit_id(UnitKind::Footman), "hfoo");
        assert_eq!(Assets::unit_id(UnitKind::TownHall), "htow");

        // Names come from `*UnitStrings.txt` (Russian install).
        let name = assets.unit_name(UnitKind::Footman);
        assert!(
            name == "Пехотинец" || name == "Footman",
            "footman name: {name}"
        );

        // Icons come from the `Art` profile field.
        let icon = assets
            .unit_icon_name(UnitKind::Footman)
            .expect("footman icon");
        assert_eq!(icon, "BTNFootman");
        assert!(assets
            .unit_icon_path(UnitKind::Footman)
            .expect("icon path")
            .to_ascii_lowercase()
            .ends_with("btnfootman.blp"));

        // Stats come from `UnitBalance`/`UnitWeapons`. TFT cut the Footman to
        // 135 gold, RoC keeps 160, so the expectation follows the mounted
        // install (the database row itself is asserted through `unit_cost`).
        let stats = assets.unit_stats(UnitKind::Footman).expect("footman stats");
        assert_eq!(stats.hp, Some(420), "HP from UnitBalance");
        let footman_cost: u32 = if assets.is_expansion() { 135 } else { 160 };
        assert_eq!(
            stats.gold_cost,
            Some(footman_cost as i32),
            "cost from UnitBalance (TFT 135 / RoC 160)"
        );
        assert_eq!(stats.armor, Some(2), "armour from UnitBalance");
        assert_eq!(assets.unit_cost(UnitKind::Footman), footman_cost);

        // Tooltips come from `Ubertip`/`Tip`.
        let tooltip = assets
            .unit_tooltip(UnitKind::Footman)
            .expect("footman tooltip");
        assert!(
            tooltip.contains("солдат") || tooltip.contains("melee") || !tooltip.is_empty(),
            "tooltip: {tooltip}"
        );
        println!("FOOTMAN name={name} icon={icon} stats={stats:?}");
    }

    #[test]
    fn geoaset_single_key_tracks_hide_effect_geosets() {
        let Some((mpq, map)) = data() else {
            eprintln!("SKIP geoaset_single_key_tracks: user War3.mpq/map not present");
            return;
        };
        let assets = Assets::load(Some(&mpq), Some(&map));
        let footman = index_of(&assets, UnitKind::Footman);
        let footman_stand = stand(&assets, UnitKind::Footman);
        let alphas = assets.mesh_alpha(footman, Some(&footman_stand), footman_stand.start);
        println!("FOOTMAN stand alphas={alphas:?}");
        assert!(alphas.len() >= 5, "Footman geosets: {}", alphas.len());
        assert!(
            alphas[0] > 0.9 && alphas[1] > 0.9,
            "body geosets visible: {alphas:?}"
        );
        assert!(
            alphas.iter().skip(2).filter(|alpha| **alpha <= 0.01).count() >= 2,
            "single-key 0 tracks must hide weapon/effect geosets: {alphas:?}"
        );

        let archer = index_of(&assets, UnitKind::Archer);
        let archer_stand = stand(&assets, UnitKind::Archer);
        let archer_alphas = assets.mesh_alpha(archer, Some(&archer_stand), archer_stand.start);
        println!("ARCHER stand alphas={archer_alphas:?}");
        assert!(
            archer_alphas.first().copied().unwrap_or(0.0) > 0.9,
            "Archer body visible: {archer_alphas:?}"
        );
        if archer_alphas.len() > 3 {
            assert!(
                archer_alphas[2] <= 0.01 && archer_alphas[3] <= 0.01,
                "Archer effect geosets hidden: {archer_alphas:?}"
            );
        }

        let peasant = index_of(&assets, UnitKind::Peasant);
        let peasant_stand = stand(&assets, UnitKind::Peasant);
        let peasant_alphas = assets.mesh_alpha(peasant, Some(&peasant_stand), peasant_stand.start);
        println!("PEASANT stand alphas={peasant_alphas:?}");
        assert!(
            peasant_alphas.iter().skip(2).filter(|alpha| **alpha <= 0.01).count() >= 2,
            "Peasant tool/effect geosets hidden: {peasant_alphas:?}"
        );

        let town_hall = index_of(&assets, UnitKind::TownHall);
        let town_stand = stand(&assets, UnitKind::TownHall);
        let town_alphas = assets.mesh_alpha(town_hall, Some(&town_stand), town_stand.start);
        println!(
            "TOWNHALL stand visible={} total={}",
            town_alphas.iter().filter(|alpha| **alpha > 0.99).count(),
            town_alphas.len()
        );
        assert!(
            town_alphas.iter().filter(|alpha| **alpha > 0.99).count() >= 3,
            "TownHall must stay visible in Stand: {town_alphas:?}"
        );
    }

    #[test]
    fn geoaset_skinning_weights_are_normalised() {
        let Some((mpq, map)) = data() else {
            eprintln!("SKIP geoaset_skinning_weights: user War3.mpq/map not present");
            return;
        };
        let assets = Assets::load(Some(&mpq), Some(&map));
        for kind in MODEL_KINDS {
            let model = assets.model(kind);
            if model.fallback {
                continue;
            }
            let mut weighted = 0usize;
            let mut multi_bone = 0usize;
            let mut worst_sum = 0.0f32;
            for mesh in &model.meshes {
                // `Assets::load` compacts every mesh before this test runs, so
                // the emptied f32 fields are not the skin data anymore: the
                // compact record is exactly what the GPU upload reads.
                let expanded = mesh.expanded();
                assert_eq!(
                    expanded.bones.len(),
                    mesh.vertex_count(),
                    "{kind:?} one influence list per vertex"
                );
                for list in &expanded.bones {
                    if list.is_empty() {
                        continue;
                    }
                    weighted += 1;
                    if list.len() > 1 {
                        multi_bone += 1;
                    }
                    let total: f32 = list.iter().map(|bone| bone.weight).sum();
                    worst_sum = worst_sum.max((total - 1.0).abs());
                    assert!(
                        (total - 1.0).abs() <= 1e-3,
                        "{kind:?} compact vertex weights sum to {total}"
                    );
                    // The GPU vertex format carries four slots; the compact
                    // format keeps the four heaviest authored influences and
                    // never sends a fifth as a truncated list.
                    assert!(
                        list.len() <= 4,
                        "{kind:?} keeps at most 4 influences (got {})",
                        list.len()
                    );
                    assert!(
                        list.iter().all(|bone| bone.weight > 0.0),
                        "{kind:?} influence list keeps only live weights: {list:?}"
                    );
                }
            }
            println!(
                "WEIGHTS {kind:?} weighted={weighted} multi_bone={multi_bone} \
                 worst_sum_err={worst_sum:.7}"
            );
            assert!(weighted > 0, "{kind:?} has weighted vertices");
        }

        // Known source anchor: MDX geoset 0 of
        // `Units\Human\Footman\Footman.mdx` binds vertex 0 to bone 4 at
        // weight 1.0 (matrix group 0 -> MATS[0] = 4), and vertex 1 blends
        // bones 4 and 5. A test that only counts weights can pass on an empty
        // mesh or a wrong decode; these pairs pin the actual authored skinning.
        let footman = assets.model(UnitKind::Footman);
        let body = footman
            .meshes
            .iter()
            .find(|mesh| mesh.geoset_id == 0)
            .expect("Footman body geoset");
        let compact = body.compact.as_ref().expect("Footman body is compact");
        let first = compact.bones(0);
        assert_eq!(first.len(), 1, "Footman v0 is a single-bone vertex: {first:?}");
        assert_eq!(first[0].bone, 4, "Footman v0 follows the authored bone 4");
        assert!((first[0].weight - 1.0).abs() <= 1e-3);
        let second = compact.bones(1);
        assert_eq!(
            second.iter().map(|pair| pair.bone).collect::<Vec<_>>(),
            vec![4, 5],
            "Footman v1 blends the authored bones 4 and 5"
        );
        assert!((second.iter().map(|pair| pair.weight).sum::<f32>() - 1.0).abs() <= 1e-3);
    }

    #[test]
    fn model_yaw_and_team_materials() {
        let Some((mpq, map)) = data() else {
            eprintln!("SKIP model_yaw_and_team_materials: user War3.mpq/map not present");
            return;
        };
        let assets = Assets::load(Some(&mpq), Some(&map));
        let archer = assets.model(UnitKind::Archer);
        // Sequence-scoped sampling removed the need for a presentation yaw
        // flip: the old 197 degree root yaw came from a Death-only track
        // leaking into Walk. Every model now uses the authored facing.
        assert_eq!(archer.yaw_offset, 0.0);
        assert_eq!(assets.model(UnitKind::Footman).yaw_offset, 0.0);
        for kind in MODEL_KINDS {
            let model = assets.model(kind);
            if model.fallback {
                continue;
            }
            let team_layers = model
                .meshes
                .iter()
                .flat_map(|mesh| mesh.layers.iter())
                .filter(|layer| layer.team_color)
                .count();
            println!(
                "{kind:?} height={:.1} scale={:.2} team_layers={team_layers}",
                model.height, model.scale
            );
            assert!(team_layers > 0, "{kind:?} has a team-colour layer");
            assert!(
                model.height > 40.0,
                "{kind:?} height {:.1} is a real model",
                model.height
            );
        }
    }
}

#[cfg(test)]
mod audio_wiring_tests {
    use super::*;
    use std::sync::Mutex;

    static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn capture(line: &str) {
        LINES.lock().expect("lines").push(line.to_string());
    }

    /// The QA gate greps `GAME_AUDIO` in the device log; the line must be
    /// emitted through `dev_log` (the app's logcat sink), not `println!`.
    #[test]
    fn wiring_emits_one_game_audio_line() {
        crate::set_log_sink(capture);
        let wiring = GameAudioWiring {
            device: "null",
            source: "wc3-install".into(),
            layers: vec!["War3xlocal".into(), "War3x".into(), "War3".into()],
            tables: 4,
            definitions: 2025,
            units: 814,
            themes: 42,
        };
        log_audio_wiring(&wiring);
        let lines = LINES.lock().expect("lines");
        let line = lines
            .iter()
            .find(|line| line.starts_with("GAME_AUDIO "))
            .expect("GAME_AUDIO line captured");
        assert!(line.contains("device=null"), "{line}");
        assert!(line.contains("layers=[\"War3xlocal\""), "{line}");
        assert!(line.contains("defs=2025"), "{line}");
        assert!(line.contains("themes=42"), "{line}");
    }
}

/// HUD redraw pack integration: the BLP1 override layer must win over the MPQ
/// for the six verified textures and expose the extra button/portrait/minimap
/// states the pack adds. Skipped when the pack is not checked out.
#[cfg(test)]
mod hud_redraw_tests {
    use super::*;

    fn pack_dir() -> Option<std::path::PathBuf> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/agent-hud/redraw-v0.1/blp");
        dir.is_dir().then_some(dir)
    }

    #[test]
    fn blp1_override_wins_and_decodes() {
        let Some(pack) = pack_dir() else {
            eprintln!("skip: redraw pack missing");
            return;
        };
        let retail = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
        let mut sources: Vec<Box<dyn AssetSource>> = vec![Box::new(
            DirSource::open(&pack).expect("redraw dir source"),
        )];
        if retail.is_dir() {
            sources.push(Box::new(DirSource::open(&retail).expect("retail dir source")));
        }
        let mut db = AssetDb::open_sources(sources, "redraw-test").expect("layered db");

        let tile = db
            .texture("UI\\Console\\Human\\HumanUITile01.blp")
            .expect("pack tile decodes");
        assert_eq!((tile.width, tile.height), (256, 288), "pack tile wins");
        let border = db
            .texture("UI\\Widgets\\Console\\Human\\CommandButton\\human-multipleselection-border.blp")
            .expect("pack border decodes");
        assert_eq!((border.width, border.height), (64, 64));
        let portrait = db
            .texture("UI\\Console\\Human\\HumanUIPortraitMask.blp")
            .expect("pack portrait decodes");
        assert_eq!((portrait.width, portrait.height), (128, 128));

        for state in ["normal", "pressed", "disabled", "cooldown", "autocast", "empty"] {
            let path = format!(
                "UI\\Widgets\\Console\\Human\\CommandButton\\human-button-{state}.blp"
            );
            let image = db.texture(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
            assert_eq!((image.width, image.height), (64, 64), "{path}");
        }
        for name in [
            "HumanUIPortraitMaskNormal.blp",
            "HumanUIPortraitMaskHero.blp",
            "HumanUIPortraitMaskEnemy.blp",
            "HumanMinimapFrame.blp",
            "HumanMinimapFrameAlert.blp",
            "HumanMinimapHeader.blp",
            "HumanTopbarPlate.blp",
            "HumanResourcePill.blp",
            "HumanResourcePillWarn.blp",
            "HumanSelectionPanel.blp",
            "HumanControlGroupButtonInactive.blp",
            "HumanControlGroupButtonActive.blp",
        ] {
            let path = format!("UI\\Console\\Human\\{name}");
            assert!(db.texture(&path).is_ok(), "{path}");
        }
        assert!(
            db.texture("UI\\Widgets\\ToolTips\\Human\\human-tooltip-panel.blp")
                .is_ok(),
            "pack tooltip panel"
        );
    }

    #[test]
    fn redraw_dir_discovery_prefers_install_root() {
        let temp = std::env::temp_dir().join("h2-redraw-discovery");
        let pack = temp.join("redraw-v0.1").join("blp");
        std::fs::create_dir_all(pack.join("UI").join("Console").join("Human")).expect("mkdir");
        std::fs::write(
            pack.join("UI").join("Console").join("Human").join("HumanUITile01.blp"),
            b"BLP1",
        )
        .expect("tile marker");
        let found = hud_redraw_dir(&temp).expect("install-root redraw dir");
        assert_eq!(found, pack);
        std::fs::remove_dir_all(&temp).ok();
    }
}

/// Gate `assets.tft-archives-layer`.
///
/// The install view must mount `War3x.mpq` (TFT data) and `War3xlocal.mpq`
/// (TFT localisation) above the RoC `War3.mpq`, and the TFT rows must resolve
/// through the same `GameDatabase` reader the game uses. The test measures the
/// real install root, then stages a RoC-only and a full view to state the delta
/// explicitly. Skips (with a line) when no install is mounted.
#[cfg(test)]
mod tft_archives {
    use super::*;
    use std::path::{Path, PathBuf};
    use wc3_data::TableKind;

    /// TFT-only four-character codes (absent from the RoC sheets, observed in
    /// `War3x.mpq`): Naga Spirit Warden, Blood Mage, Staff of the Ancients,
    /// Scroll of Arcane Knowledge.
    const TFT_UNIT_IDS: [&str; 2] = ["nwad", "Hblm"];
    const TFT_ITEM_IDS: [&str; 2] = ["axas", "arsc"];

    #[derive(Debug)]
    struct Measurement {
        layers: Vec<String>,
        unit_rows: usize,
        item_rows: usize,
        ability_rows: usize,
        ability_named: usize,
        ability_icons: usize,
        units_named: usize,
        sound_defs: usize,
        sound_tables: usize,
        missing_tables: usize,
        names: Vec<(String, Option<String>)>,
    }

    impl Measurement {
        fn is_cyrillic(&self, id: &str) -> bool {
            self.names
                .iter()
                .any(|(name_id, name)| {
                    name_id == id
                        && name
                            .as_deref()
                            .map(|name| !name.is_empty() && name.chars().any(|c| !c.is_ascii()))
                            .unwrap_or(false)
                })
        }

        fn name_of(&self, id: &str) -> Option<&str> {
            self.names
                .iter()
                .find(|(name_id, _)| name_id == id)
                .and_then(|(_, name)| name.as_deref())
        }

        fn print(&self, label: &str) {
            println!(
                "{label}: layers={:?} units={} items={} abilities={} ability_names={} ability_icons={} unit_names={} sounds={} sound_tables={} missing_tables={}",
                self.layers,
                self.unit_rows,
                self.item_rows,
                self.ability_rows,
                self.ability_named,
                self.ability_icons,
                self.units_named,
                self.sound_defs,
                self.sound_tables,
                self.missing_tables,
            );
            for (id, name) in &self.names {
                println!("{label}: {id} -> {name:?}");
            }
        }
    }

    /// Case-insensitive archive lookup (the app data dir is case-sensitive).
    fn archive_named(root: &Path, name: &str) -> Option<PathBuf> {
        let wanted = name.to_ascii_lowercase();
        let mut found: Vec<PathBuf> = std::fs::read_dir(root)
            .ok()?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .file_name()
                        .map(|file| file.to_string_lossy().to_ascii_lowercase() == wanted)
                        .unwrap_or(false)
            })
            .collect();
        found.sort();
        found.into_iter().next()
    }

    fn install_root() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("H2_WC3_INSTALL") {
            let dir = PathBuf::from(dir);
            if archive_named(&dir, "War3.mpq").is_some() {
                return Some(dir);
            }
        }
        let app = PathBuf::from("/data/data/org.herofolends.p0/files");
        if archive_named(&app, "War3.mpq").is_some() {
            return Some(app);
        }
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-assets/wc3-install");
        archive_named(&repo, "War3.mpq").map(|_| repo)
    }

    /// Stage a view of `root` with only `names` present, linked not copied.
    fn stage(root: &Path, names: &[&str], label: &str) -> Option<PathBuf> {
        let dir = std::env::temp_dir().join(format!(
            "h2-tft-{label}-{}-{}",
            std::process::id(),
            names.len()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).ok()?;
        for name in names {
            let Some(source) = archive_named(root, name) else {
                continue;
            };
            let target = dir.join(name);
            #[cfg(unix)]
            if std::os::unix::fs::symlink(&source, &target).is_err() {
                return None;
            }
            #[cfg(not(unix))]
            if std::fs::hard_link(&source, &target).is_err() {
                return None;
            }
        }
        Some(dir)
    }

    fn measure(root: &Path) -> Measurement {
        let fs = StormFs::open_wc3_install(root).expect("mount WC3 install");
        let layers: Vec<String> = fs.layers().iter().map(|layer| layer.label.clone()).collect();
        let source = StormSource::new(fs, "tft-test");
        let db = GameDatabase::load(&source).expect("load game database");
        let mut unit_rows = 0;
        let mut item_rows = 0;
        let mut ability_rows = 0;
        for (kind, rows) in db.counts() {
            match kind {
                TableKind::UnitData => unit_rows = rows,
                TableKind::ItemData => item_rows = rows,
                TableKind::AbilityData => ability_rows = rows,
                _ => {}
            }
        }
        let ability_named = db.abilities().filter(|ability| ability.resolved_name().is_some()).count();
        let ability_icons = db.abilities().filter(|ability| ability.resolved_icon().is_some()).count();
        let units_named = db.units().filter(|unit| unit.name().is_some()).count();
        let names: Vec<(String, Option<String>)> = TFT_UNIT_IDS
            .iter()
            .chain(TFT_ITEM_IDS.iter())
            .map(|id| {
                let name = db
                    .unit(id)
                    .and_then(|unit| unit.name())
                    .or_else(|| db.item(id).and_then(|item| item.name()));
                ((*id).to_string(), name)
            })
            .collect();
        let mut asset_db = AssetDb::open_sources(vec![Box::new(source)], "tft-sounds")
            .expect("open asset db for sound tables");
        let (sounds, _units, sound_tables) = wc3_assetdb::audio_provider::load_sound_tables(&mut asset_db);
        let sound_defs = sounds.len();
        Measurement {
            layers,
            unit_rows,
            item_rows,
            ability_rows,
            ability_named,
            ability_icons,
            units_named,
            sound_defs,
            sound_tables,
            missing_tables: db.missing().len(),
            names,
        }
    }

    #[test]
    fn tft_archives_layer() {
        let Some(root) = install_root() else {
            println!("skip: no WC3 install");
            return;
        };
        println!("install_root={}", root.display());
        let has_tft =
            archive_named(&root, "War3x.mpq").is_some() && archive_named(&root, "War3xlocal.mpq").is_some();

        let Some(roc_dir) = stage(&root, &["War3.mpq"], "roc") else {
            println!("skip: cannot stage a RoC-only view");
            return;
        };
        let roc = measure(&roc_dir);
        roc.print("roc");
        assert_eq!(roc.layers, vec!["War3.mpq".to_string()], "RoC view is one layer");
        assert!(roc.unit_rows > 400, "RoC unit rows: {}", roc.unit_rows);
        assert!(roc.ability_named > 300, "RoC abilities with names: {}", roc.ability_named);
        assert!(roc.name_of(TFT_UNIT_IDS[0]).is_none(), "nwad is TFT-only");
        assert!(roc.name_of(TFT_UNIT_IDS[1]).is_none(), "Hblm is TFT-only");
        assert!(roc.name_of(TFT_ITEM_IDS[0]).is_none(), "axas is TFT-only");
        assert!(roc.name_of(TFT_ITEM_IDS[1]).is_none(), "arsc is TFT-only");

        let app = measure(&root);
        app.print("install");
        if !has_tft {
            // RoC-only install: the layer must degrade to the base archive and
            // still load every table.
            assert_eq!(app.layers, vec!["War3.mpq".to_string()], "{:?}", app.layers);
            assert_eq!(app.unit_rows, roc.unit_rows, "RoC fallback keeps base rows");
            assert_eq!(app.ability_rows, roc.ability_rows);
            println!("result=roc-only (no War3x.mpq in the install root)");
            return;
        }

        assert!(app.layers.iter().any(|layer| layer.eq_ignore_ascii_case("War3x.mpq")), "{:?}", app.layers);
        assert!(
            app.layers.iter().any(|layer| layer.eq_ignore_ascii_case("War3xlocal.mpq")),
            "{:?}",
            app.layers
        );

        let Some(tft_dir) = stage(&root, &["War3xlocal.mpq", "War3x.mpq", "War3.mpq"], "tft") else {
            println!("skip: cannot stage a TFT view");
            return;
        };
        let tft = measure(&tft_dir);
        tft.print("tft");
        assert_eq!(
            tft.layers,
            vec![
                "War3xlocal.mpq".to_string(),
                "War3x.mpq".to_string(),
                "War3.mpq".to_string()
            ],
            "TFT layer order"
        );

        // TFT deltas, measured on the same reader.
        assert!(tft.unit_rows > roc.unit_rows, "units {} -> {}", roc.unit_rows, tft.unit_rows);
        assert!(tft.item_rows > roc.item_rows, "items {} -> {}", roc.item_rows, tft.item_rows);
        assert!(
            tft.ability_rows > roc.ability_rows,
            "abilities {} -> {}",
            roc.ability_rows,
            tft.ability_rows
        );
        assert!(
            tft.ability_named > roc.ability_named,
            "ability names {} -> {}",
            roc.ability_named,
            tft.ability_named
        );
        assert!(tft.units_named > roc.units_named, "unit names {} -> {}", roc.units_named, tft.units_named);
        assert!(
            tft.sound_defs > roc.sound_defs,
            "sound defs {} -> {}",
            roc.sound_defs,
            tft.sound_defs
        );
        assert_eq!(tft.missing_tables, 0, "TFT view must provide every table");

        // TFT-only rows resolve, with the localisation archive layered in.
        for id in TFT_UNIT_IDS.iter().chain(TFT_ITEM_IDS.iter()) {
            assert!(
                tft.name_of(id).is_some(),
                "{id} must carry a name on the TFT content set"
            );
        }
        for id in TFT_UNIT_IDS.iter().chain(TFT_ITEM_IDS.iter()) {
            assert!(
                tft.is_cyrillic(id),
                "{id} name must come from War3xlocal.mpq: {:?}",
                tft.name_of(id)
            );
        }

        // The app install root must already be the TFT content set.
        assert!(app.unit_rows >= tft.unit_rows, "install units {} vs staged {}", app.unit_rows, tft.unit_rows);
        assert_eq!(app.unit_rows, tft.unit_rows, "install and staged views agree");
        assert_eq!(app.ability_named, tft.ability_named);
        assert_eq!(app.sound_defs, tft.sound_defs);
        assert!(app.is_cyrillic(TFT_UNIT_IDS[1]), "install resolves TFT names");
        println!(
            "result=tft units {}->{} items {}->{} abilities {}->{} names {}->{} sounds {}->{}",
            roc.unit_rows,
            tft.unit_rows,
            roc.item_rows,
            tft.item_rows,
            roc.ability_rows,
            tft.ability_rows,
            roc.ability_named,
            tft.ability_named,
            roc.sound_defs,
            tft.sound_defs
        );
        std::fs::remove_dir_all(&roc_dir).ok();
        std::fs::remove_dir_all(&tft_dir).ok();
    }
}
