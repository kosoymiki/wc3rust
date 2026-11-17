//! Standalone composition: depth target + one world pass + one UI pass.
//!
//! The game owns its own pass order (terrain, water and doodads interleave with
//! the unit groups), so it uses [`MeshRenderer::prepare`] / [`draw_group`]
//! directly. This type exists for callers that want the whole frame in one call:
//! the bench, headless tests and tools.
//!
//! [`MeshRenderer::prepare`]: crate::mesh::MeshRenderer::prepare
//! [`draw_group`]: crate::mesh::MeshRenderer::draw_group

use crate::fx::{FxFrameStats, FxRenderer, Particle, ParticleBudget};
use crate::mesh::{MeshRenderer, MeshVertex, Model, StaticBatch, StaticDrawSpec, DEPTH_FORMAT};
use crate::particles::{
    EmitterBudget, EmitterParams, ParticleCamera, ParticleDraw, ParticleInstance, ParticlePass,
    ParticleStats,
};
use crate::perf::Counters;
use crate::scene::Scene;
use crate::texture::TextureImage;
use crate::ui::UiRenderer;

/// One frame of instanced MDX emitters.
#[derive(Default)]
pub struct EmitterFrame {
    pub instances: Vec<ParticleInstance>,
    pub params: Vec<EmitterParams>,
    pub draws: Vec<ParticleDraw>,
}

impl EmitterFrame {
    pub fn is_empty(&self) -> bool {
        self.draws.is_empty()
    }
}

/// Terrain input: packed mesh, index stream and the atlas slot in the texture
/// store. The atlas is part of `images`, so there is exactly one texture store.
pub struct TerrainBatch {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
    /// Index of the terrain atlas inside the texture list passed to
    /// [`Renderer::new`].
    pub atlas_texture: usize,
    /// Vertex-colour decode scale: the terrain builder stores shade in 0.75..1.25
    /// halved, so it passes 2.0 here.
    pub color_scale: f32,
}

/// Everything needed to draw a frame.
pub struct Renderer {
    pub mesh: MeshRenderer,
    ui: UiRenderer,
    fx: FxRenderer,
    /// Per-frame FX budget; callers can tighten it for low-end devices.
    pub fx_budget: ParticleBudget,
    /// Particles submitted for the next frame via [`Renderer::set_particles`].
    particles: Vec<Particle>,
    last_fx: FxFrameStats,
    /// Instanced MDX emitters (`PRE2`/`RIBB`/`SPL`/`UBR`).
    emitters: ParticlePass,
    /// Emitter frame submitted via [`Renderer::set_emitters`].
    emitter_frame: EmitterFrame,
    last_emitters: ParticleStats,
    /// Per-frame emitter budget; callers can tighten it for low-end devices.
    pub emitter_budget: EmitterBudget,
    terrain: Option<StaticBatch>,
    depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    /// Number of times the depth target had to be recreated.
    pub depth_recreates: u32,
}

impl Renderer {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        models: Vec<Model>,
        images: &[TextureImage],
        max_texture_size: u32,
        terrain: Option<TerrainBatch>,
        font_atlas: &TextureImage,
        minimap: Option<&TextureImage>,
    ) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let mesh = MeshRenderer::new(
            device,
            queue,
            format,
            models,
            images,
            max_texture_size,
        );
        let terrain = terrain.map(|batch| {
            let specs = [StaticDrawSpec {
                index_start: 0,
                index_count: batch.indices.len() as u32,
                texture: batch.atlas_texture,
                blend: 0,
                group: 0,
                tint: [1.0; 4],
                color_scale: batch.color_scale,
            }];
            mesh.create_static_batch(device, queue, &batch.vertices, &batch.indices, &specs)
        });
        let texture_layout = crate::texture::bind_group_layout(device);
        let ui = UiRenderer::new(device, queue, format, &texture_layout, font_atlas, minimap);
        let fx = FxRenderer::new(device, format);
        let emitters = ParticlePass::new(device, format);
        let (depth, depth_view) = create_depth(device, width, height);
        Self {
            mesh,
            ui,
            fx,
            fx_budget: ParticleBudget::default(),
            particles: Vec::new(),
            last_fx: FxFrameStats::default(),
            emitters,
            emitter_frame: EmitterFrame::default(),
            last_emitters: ParticleStats::default(),
            emitter_budget: EmitterBudget::default(),
            terrain,
            depth,
            depth_view,
            width,
            height,
            format,
            depth_recreates: 0,
        }
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// FX counters of the last frame (particles emitted/rendered/dropped, draws).
    pub fn fx_stats(&self) -> FxFrameStats {
        self.last_fx
    }

    /// Submit projectile trails, impact bursts and spell particles for the next
    /// frame. The list is consumed by the FX pass and cleared by the caller on
    /// the next frame with a fresh list.
    pub fn set_particles(&mut self, particles: Vec<Particle>) {
        self.particles = particles;
    }

    /// Particles submitted for the next frame.
    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }

    /// Submit the emitter frame (instances, parameter blocks and draws) built by
    /// [`crate::emitters::EmitterSystem`] for the next frame.
    pub fn set_emitters(
        &mut self,
        instances: Vec<ParticleInstance>,
        params: Vec<EmitterParams>,
        draws: Vec<ParticleDraw>,
    ) {
        self.emitter_frame = EmitterFrame {
            instances,
            params,
            draws,
        };
    }

    /// Emitter counters of the last frame.
    pub fn emitter_stats(&self) -> ParticleStats {
        self.last_emitters
    }

    /// Access the emitter pass directly (tests, tools).
    pub fn emitter_pass(&self) -> &ParticlePass {
        &self.emitters
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        let (depth, depth_view) = create_depth(device, width, height);
        self.depth = depth;
        self.depth_view = depth_view;
        self.depth_recreates += 1;
    }

    /// Record the whole frame (terrain + world groups + UI) into `encoder`.
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        scene: &Scene,
        stats: &mut Counters,
    ) {
        self.encode_with(device, queue, encoder, view, scene, crate::scene::PassMask::all(), stats);
    }

    /// Record selected passes, for per-pass timing or partial frames.
    pub fn encode_with(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        scene: &Scene,
        mask: crate::scene::PassMask,
        stats: &mut Counters,
    ) {
        if mask.terrain || mask.units {
            self.mesh.prepare(device, queue, scene, mask, stats);
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("world-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
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
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
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
                if mask.terrain {
                    if let Some(terrain) = &self.terrain {
                        self.mesh.draw_static(&mut pass, terrain, 0, stats);
                    }
                }
                for group in 0..3u8 {
                    self.mesh.draw_group(&mut pass, group, stats);
                }
            }
        }
        if mask.units && !self.particles.is_empty() {
            self.last_fx = self.fx.prepare(
                device,
                queue,
                &scene.view_proj,
                &self.particles,
                self.fx_budget,
            );
            if self.last_fx.rendered > 0 {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("fx-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                stats.passes += 1;
                self.fx.draw(&mut pass, stats);
            }
        }
        if mask.units && !self.emitter_frame.is_empty() {
            let camera = ParticleCamera::from_view_proj(&scene.view_proj);
            self.last_emitters = self.emitters.prepare(
                device,
                queue,
                &camera,
                &self.emitter_frame.instances,
                &self.emitter_frame.params,
                &self.emitter_frame.draws,
                self.emitter_budget,
            );
            if self.last_emitters.draws > 0 {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("emitter-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                stats.passes += 1;
                self.emitters
                    .draw(&mut pass, self.mesh.textures(), stats);
            }
            // The frame is consumed: a caller that does not resubmit clears FX.
            self.emitter_frame = EmitterFrame::default();
        }
        if mask.ui {
            let textures = self.mesh.textures();
            if self.ui.prepare(device, queue, scene, stats) {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("ui-pass"),
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
                self.ui.draw(&mut pass, textures, stats);
            }
        }
    }

    /// Convenience: create an encoder, record and submit one frame.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        scene: &Scene,
        stats: &mut Counters,
    ) {
        self.render_with(device, queue, view, scene, crate::scene::PassMask::all(), stats);
    }

    /// Convenience: record the selected passes and submit.
    pub fn render_with(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        scene: &Scene,
        mask: crate::scene::PassMask,
        stats: &mut Counters,
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame"),
        });
        self.encode_with(device, queue, &mut encoder, view, scene, mask, stats);
        queue.submit(Some(encoder.finish()));
    }
}

fn create_depth(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Read a rendered target back as tightly packed RGBA8.
pub fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let bytes_per_row = width * 4;
    let padded_row = bytes_per_row.div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: padded_row as u64 * height as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("readback-copy"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel::<Result<(), wgpu::BufferAsyncError>>();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .map_err(|error| format!("poll: {error}"))?;
    receiver
        .recv()
        .map_err(|error| format!("map channel: {error}"))?
        .map_err(|error| format!("map: {error}"))?;
    let mut out = Vec::with_capacity((bytes_per_row * height) as usize);
    {
        let mapped = slice
            .get_mapped_range()
            .map_err(|error| format!("mapped range: {error}"))?;
        for row in 0..height {
            let start = (row * padded_row) as usize;
            out.extend_from_slice(&mapped[start..start + bytes_per_row as usize]);
        }
    }
    buffer.unmap();
    Ok(out)
}
