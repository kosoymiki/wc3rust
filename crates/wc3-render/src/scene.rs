//! Renderer-facing scene description.
//!
//! These are plain data types with no dependency on the game crate, so the GPU
//! layer can be benchmarked and tested without an asset database, a simulation or
//! a window. `apps/bench` converts the game's scene into these once per frame.

use glam::{Mat4, Vec2, Vec3};

/// One skinned model instance.
#[derive(Clone, Debug)]
pub struct UnitDraw {
    /// Index into the renderer's model table.
    pub model: usize,
    /// Model-to-world transform.
    pub world: Mat4,
    /// RGBA tint applied to team-colour layers.
    pub tint: [f32; 4],
    /// Bone (node) world matrices, model space, already animated.
    pub nodes: Vec<Mat4>,
    /// Per-mesh visibility (geoset alpha animation).
    pub mesh_alpha: Vec<f32>,
    /// Per-mesh, per-layer material alpha animation.
    pub layer_alpha: Vec<Vec<f32>>,
    /// Per-mesh geoset colour from `GEOA` (`KGAC`), multiplied into the vertex
    /// colours. An empty vector means "white", the common case, so callers that
    /// do not sample geoset colours keep working unchanged.
    pub mesh_color: Vec<[f32; 4]>,
}

impl Default for UnitDraw {
    fn default() -> Self {
        Self {
            model: 0,
            world: Mat4::IDENTITY,
            tint: [1.0; 4],
            nodes: Vec::new(),
            mesh_alpha: Vec::new(),
            layer_alpha: Vec::new(),
            mesh_color: Vec::new(),
        }
    }
}

/// Ground ring (selection marker, move order, gold mine).
pub struct Ring {
    pub pos: Vec3,
    pub radius: f32,
    pub color: [f32; 4],
}

/// Screen-space UI vertex.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// Textured UI run: one texture, many quads.
#[derive(Clone, Default)]
pub struct UiDraw {
    pub texture: usize,
    pub vertices: Vec<UiVertex>,
}

/// Screen-space UI batches: flat white, bitmap font, minimap and textured chrome.
#[derive(Default)]
pub struct UiBatch {
    pub white: Vec<UiVertex>,
    pub font: Vec<UiVertex>,
    pub minimap: Vec<UiVertex>,
    pub textured: Vec<UiDraw>,
}

impl UiBatch {
    pub fn vertex_count(&self) -> usize {
        self.white.len()
            + self.font.len()
            + self.minimap.len()
            + self.textured.iter().map(|draw| draw.vertices.len()).sum::<usize>()
    }

    /// Number of draws this batch needs (one per textured run + up to three
    /// fixed batches).
    pub fn draw_count(&self) -> usize {
        let fixed = [self.white.len(), self.font.len(), self.minimap.len()]
            .iter()
            .filter(|len| **len > 0)
            .count();
        self.textured
            .iter()
            .filter(|draw| !draw.vertices.is_empty())
            .count()
            + fixed
    }
}

/// Which passes to record. Lets a bench time terrain, units and UI separately
/// and lets a caller render a HUD-only or world-only frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PassMask {
    pub terrain: bool,
    pub units: bool,
    pub ui: bool,
}

impl Default for PassMask {
    fn default() -> Self {
        Self {
            terrain: true,
            units: true,
            ui: true,
        }
    }
}

impl PassMask {
    pub const NONE: Self = Self {
        terrain: false,
        units: false,
        ui: false,
    };

    pub fn all() -> Self {
        Self::default()
    }
}

/// Everything the renderer needs for one frame.
pub struct Scene {
    pub view_proj: Mat4,
    /// Camera eye in world space: the distance haze in the mesh shader needs it
    /// (`distance(world, camera.eye.xyz)`). Not serialized, not compared.
    pub eye: Vec3,
    pub viewport: Vec2,
    pub units: Vec<UnitDraw>,
    pub rings: Vec<Ring>,
    pub ui: UiBatch,
}

impl Scene {
    pub fn empty(view_proj: Mat4, viewport: Vec2) -> Self {
        Self {
            view_proj,
            eye: Vec3::ZERO,
            viewport,
            units: Vec::new(),
            rings: Vec::new(),
            ui: UiBatch::default(),
        }
    }

    /// Total input geometry: vertices the CPU has to skin this frame.
    pub fn skin_vertices(&self, models: &[Vec<crate::mesh::MeshData>]) -> usize {
        self.units
            .iter()
            .map(|unit| {
                models
                    .get(unit.model)
                    .map(|meshes| meshes.iter().map(|mesh| mesh.positions.len()).sum::<usize>())
                    .unwrap_or(0)
            })
            .sum()
    }
}
