//! Canonical wgpu/Vulkan renderer for the Herofolends base.
//!
//! One renderer, one vertex layout, one pipeline family. It draws **models**
//! (skinned or static), **static batches** (terrain mesh, merged doodads) and the
//! **UI**, and it exposes the per-frame instrumentation the project's budget is
//! measured with.
//!
//! # Why this is the single implementation
//!
//! * Geometry is uploaded once: static per-model streams plus a bone palette per
//!   unit per frame, so a steady frame uploads no vertex data at all.
//! * Skinning runs in the vertex shader (four weighted bone matrices from a
//!   read-only storage buffer); [`mesh::SkinMode::Cpu`] keeps the old CPU path as
//!   a verified reference and as the fallback for adapters without vertex-stage
//!   storage buffers.
//! * Draws are bucketed by group, model, pipeline and texture, so pipeline and
//!   bind-group changes are per material family instead of per draw.
//! * Textures are capped, never upscaled, and mip-mapped in linear light.
//!
//! # Composition
//!
//! Two ways to drive it:
//!
//! * [`Renderer`] — owns the depth target and records a whole frame (terrain,
//!   world groups, UI) in one call. Used by the bench and headless tests.
//! * [`mesh::MeshRenderer`] + [`ui::UiRenderer`] directly — the caller owns the
//!   render pass and decides what goes between the unit groups (the game draws
//!   terrain, water and doodads between them). See
//!   [`mesh::MeshRenderer::prepare`] / [`mesh::MeshRenderer::draw_group`].
//!
//! # Data
//!
//! The crate knows nothing about `wc3-game` types: callers convert their assets
//! into [`mesh::Model`], [`texture::TextureImage`] and a [`scene::Scene`]. Every
//! value still comes from the original game files through the canonical parsers
//! (`wc3-formats`, `wc3-re`, `wc3-anim`); this crate only uploads and draws it.

pub mod astc;
pub mod attachments;
pub mod cache;
pub mod daynight;
pub mod emitters;
pub mod event_objects;
pub mod fog;
pub mod frame;
pub mod fx;
pub mod geoset;
pub mod smoke;
pub mod mesh;
pub mod parity;
pub mod particles;
pub mod perf;
pub mod renderer;
pub mod scene;
pub mod surface;
pub mod terrain;
pub mod terrain_geometry;
pub mod texture;
pub mod ui;

pub use astc::AstcFile;
pub use attachments::{AttachmentPoint, AttachmentSet, AttachmentSlot};
pub use emitters::{
    EmitterContext, EmitterSystem, EmitterTextures, ParticleEmitter2Runtime, ParticleQuality,
    RibbonRuntime, SplatSpec,
};
pub use event_objects::{EventBatch, EventObjectRuntime, EventTrigger};
pub use particles::{
    EmitterBudget, EmitterKind, EmitterParams, ParticleBlend, ParticleCamera, ParticleDraw,
    ParticleInstance, ParticlePass, ParticleStats, EMITTER_PARAMS_STRIDE,
    PARTICLE_INSTANCE_STRIDE,
};
pub use cache::{ResidencyStats, ResidentModel, ResidentSet, TexturePlan};
pub use daynight::{
    game_time_line, is_night, phase, DayNightClock, DayNightCurve, DayNightSample, DncLight,
    DncLightData, FogSettings, Key, MapLighting, Phase, Preset, SurfaceLight, DAY_HOURS,
    DEFAULT_DAWN_HOUR, DEFAULT_DAY_LENGTH_SECONDS, DEFAULT_DUSK_HOUR,
};
pub use fog::{FogMask, FogRenderer};
pub use geoset::{geoset_color, geoset_tint};
pub use fx::{
    blizzard_wave, impact_burst, missile_arc_point, projectile_trail, FxBlend, FxFrameStats,
    FxRenderer, FxVertex, MissileSpec, Particle, ParticleBudget, Rng, FX_BYTES_PER_PARTICLE,
    FX_VERTS_PER_PARTICLE, FX_VERTEX_STRIDE,
};
pub use mesh::{
    BoneWeight, DrawParams, MeshData, MeshLayer, MeshRenderer, MeshVertex, Model, PipelineKind,
    SkinMode, StaticBatch, StaticDrawSpec, VERTEX_STRIDE,
};
pub use parity::{account, frame_hash, max_diff, mae, ssim, SceneBudget};
pub use perf::{Counters, CpuScope, GpuTimers, Stats};
pub use renderer::{read_back, Renderer, TerrainBatch};
pub use surface::{configure_guarded, configure_surface, pick_config, reconfigure, SurfaceSetup};
pub use terrain::{seam_metric, tile_from_sheet, SheetLevel, TileLevel};
pub use terrain_geometry::{
    cliff_blocks, sheet_quadrant, shoreline, water_border, water_depth_color, Corner, Grid,
    Mesh as TerrainOverlayMesh, Vertex as TerrainOverlayVertex,
};
pub use smoke::{GpuContext, GpuError};
pub use scene::{PassMask, Ring, Scene, UiBatch, UiDraw, UiVertex, UnitDraw};
pub use texture::{
    CompressedFormat, CompressedImage, CompressedLevel, CompressionSupport, Role, TextureImage,
    TextureSource, TextureStats,
};
pub use ui::UiRenderer;

/// Renderer identity for diagnostics (`GAME_RENDERER` log lines, manifests).
pub const RENDERER: &str = "wc3-render/batched";

/// Vertex bytes per model vertex (packed layout, see [`mesh::MeshVertex`]).
pub const VERTEX_BYTES: u64 = VERTEX_STRIDE;
