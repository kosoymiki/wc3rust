//! Compile-time proof that the game integration shapes type-check.
//!
//! `crates/wc3-game/src/render.rs` is read-only for the GPU layer, so its
//! dispatcher patch cannot be compiled here. This example mirrors every call
//! shape that patch uses (constructor, per-frame prepare/draw split, static
//! batch, UI prepare/draw, scene refill) against the real `wc3-render` API, so
//! `cargo check -p wc3-render --examples` catches any signature drift before the
//! patch is applied. It is never run: every function takes its GPU handles as
//! arguments.

#![allow(dead_code)]

use glam::{Mat4, Vec2, Vec3};
use wc3_render::mesh::{MeshLayer as GpuMeshLayer, Model, MeshVertex, StaticBatch, StaticDrawSpec};
use wc3_render::scene::{
    PassMask, Ring as GpuRing, Scene as GpuScene, UiDraw as GpuUiDraw, UiVertex as GpuUiVertex,
    UnitDraw as GpuUnitDraw,
};
use wc3_render::texture::TextureImage as GpuTexture;
use wc3_render::{Counters, MeshRenderer, UiRenderer};

/// Mirrors `Renderer::new` in the game facade.
fn construct(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    models: Vec<Model>,
    images: &[GpuTexture],
    terrain_mesh: (&[MeshVertex], &[u32]),
    terrain_atlas: usize,
    font_atlas: &GpuTexture,
    minimap: Option<&GpuTexture>,
) -> (MeshRenderer, UiRenderer, StaticBatch) {
    let mesh = MeshRenderer::new(device, queue, format, models, images, 256);
    let terrain = mesh.create_static_batch(
        device,
        queue,
        terrain_mesh.0,
        terrain_mesh.1,
        &[StaticDrawSpec {
            index_start: 0,
            index_count: terrain_mesh.1.len() as u32,
            texture: terrain_atlas,
            blend: 0,
            group: 0,
            tint: [1.0; 4],
            color_scale: 2.0,
        }],
    );
    let layout = wc3_render::texture::bind_group_layout(device);
    let ui = UiRenderer::new(device, queue, format, &layout, font_atlas, minimap);
    (mesh, ui, terrain)
}

/// Mirrors the per-frame world pass of the game facade.
fn frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    mesh: &mut MeshRenderer,
    ui: &mut UiRenderer,
    doodads: &StaticBatch,
    scene: &GpuScene,
) {
    let mut counters = Counters::default();
    mesh.prepare(
        device,
        queue,
        scene,
        PassMask::default(),
        &mut counters,
    );
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("game-world-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
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
        mesh.draw_static(&mut pass, doodads, 0, &mut counters);
        mesh.draw_group(&mut pass, 0, &mut counters);
        mesh.draw_static(&mut pass, doodads, 1, &mut counters);
        mesh.draw_group(&mut pass, 1, &mut counters);
        mesh.draw_static(&mut pass, doodads, 2, &mut counters);
        mesh.draw_group(&mut pass, 2, &mut counters);
    }
    if ui.prepare(device, queue, scene, &mut counters) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("game-ui-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let textures = mesh.textures();
        ui.draw(&mut pass, textures, &mut counters);
    }
    let _ = counters.draws;
}

/// Mirrors `fill_gpu_scene`: refill in place, no per-frame allocation churn.
fn fill(gpu: &mut GpuScene, view_proj: Mat4, viewport: Vec2, units: &[GpuUnitDraw]) {
    gpu.view_proj = view_proj;
    gpu.viewport = viewport;
    if gpu.units.len() < units.len() {
        gpu.units.resize_with(units.len(), GpuUnitDraw::default);
    }
    for (slot, unit) in gpu.units.iter_mut().zip(units) {
        slot.model = unit.model;
        slot.world = unit.world;
        slot.tint = unit.tint;
        slot.nodes.clear();
        slot.nodes.extend_from_slice(&unit.nodes);
        slot.mesh_alpha.clear();
        slot.mesh_alpha.extend_from_slice(&unit.mesh_alpha);
        slot.layer_alpha.resize_with(unit.layer_alpha.len(), Vec::new);
        for (dst, src) in slot.layer_alpha.iter_mut().zip(&unit.layer_alpha) {
            dst.clear();
            dst.extend_from_slice(src);
        }
    }
    gpu.units.truncate(units.len());
    gpu.rings.resize_with(1, || GpuRing {
        pos: Vec3::ZERO,
        radius: 0.0,
        color: [1.0; 4],
    });
    gpu.ui.textured.resize_with(1, GpuUiDraw::default);
    gpu.ui.white.push(GpuUiVertex {
        pos: [0.0, 0.0],
        uv: [0.0, 0.0],
        color: [1.0; 4],
    });
}

/// Mirrors the doodad builder's material conversion.
fn layer(texture: usize, blend: u32) -> GpuMeshLayer {
    GpuMeshLayer {
        texture,
        alpha: 1.0,
        blend,
        team_color: blend == 0,
        uv_set: 0,
    }
}

fn main() {
    // Type-level only: the example is compiled, never run.
    let _ = (construct as fn(_, _, _, _, _, _, _, _, _) -> _, frame as fn(_, _, _, _, _, _, _, _, _), fill as fn(_, _, _, _), layer as fn(_, _) -> _);
}
