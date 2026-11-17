//! Deprecated compatibility facade over [`wc3_render`].
//!
//! The GPU layer lives in `wc3-render` (BASE.md §2: one renderer, one vertex
//! layout, one pipeline family). This crate keeps the previous import paths
//! working for `apps/smoke` and for branches that still spell `wc3_gpu::…`; it
//! contains **no GPU logic of its own**.
//!
//! Migration: replace `wc3_gpu::X` with `wc3_render::X` and drop the dependency.
//! The crate is scheduled for deletion in base v2 once `apps/smoke` imports
//! `wc3_render` directly (tracked in `analysis/tech-lead/merge-gpu.md`).

/// Re-exported modules: identical paths, identical types.
pub mod frame {
    pub use wc3_render::frame::*;
}
pub mod mesh {
    pub use wc3_render::mesh::*;
}
pub mod parity {
    pub use wc3_render::parity::*;
}
pub mod perf {
    pub use wc3_render::perf::*;
}
pub mod renderer {
    pub use wc3_render::renderer::*;
}
pub mod scene {
    pub use wc3_render::scene::*;
}
pub mod texture {
    pub use wc3_render::texture::*;
}
pub mod ui {
    pub use wc3_render::ui::*;
}

pub use wc3_render::{
    account, frame_hash, mae, max_diff, read_back, ssim, BoneWeight, Counters, CpuScope, DrawParams,
    GpuContext, GpuError, GpuTimers, MeshData, MeshLayer, MeshRenderer, MeshVertex, Model, PassMask,
    PipelineKind, Renderer, Ring, Role, Scene, SceneBudget, SkinMode, StaticBatch, StaticDrawSpec,
    Stats, TerrainBatch, TextureImage, TextureStats, UiBatch, UiDraw, UiRenderer, UiVertex,
    UnitDraw, VERTEX_STRIDE,
};
