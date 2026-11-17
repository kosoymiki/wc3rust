//! Screen-space UI pass: one persistent vertex buffer, one upload per frame and
//! a bind group change only when the texture actually changes.
//!
//! The HUD is already batched per texture by the game (one [`UiDraw`] per icon
//! sheet), so this pass keeps the authoring order (panels must stay behind text)
//! and only removes the per-frame allocation, the per-frame buffer growth and
//! redundant bind group calls.

use crate::frame::{DynamicBuffer, Scratch};
use crate::perf::Counters;
use crate::scene::{Scene, UiVertex};
use crate::texture::{Role, TextureImage, TextureStore, upload_levels, Mip};

const UI_SHADER: &str = r#"
struct Viewport {
    size: vec4<f32>,
};
@group(0) @binding(0) var<uniform> viewport: Viewport;

struct VsIn {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(input: VsIn) -> VsOut {
    var out: VsOut;
    let ndc = vec2<f32>(
        input.position.x / viewport.size.x * 2.0 - 1.0,
        1.0 - input.position.y / viewport.size.y * 2.0
    );
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = input.uv;
    out.color = input.color;
    return out;
}

@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, input.uv);
    return c * input.color;
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ViewportUniform {
    size: [f32; 4],
}

/// UI pass state.
pub struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    viewport_buffer: wgpu::Buffer,
    viewport_bind: wgpu::BindGroup,
    vertices: DynamicBuffer,
    scratch: Scratch<UiVertex>,
    runs: Scratch<(usize, u32, u32)>,
    white_start: u32,
    white_count: u32,
    font_start: u32,
    font_count: u32,
    minimap_start: u32,
    minimap_count: u32,
    /// Texture bind groups for the bitmap font and the minimap.
    font_bind: wgpu::BindGroup,
    minimap_bind: wgpu::BindGroup,
}

impl UiRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        texture_layout: &wgpu::BindGroupLayout,
        font_atlas: &TextureImage,
        minimap: Option<&TextureImage>,
    ) -> Self {
        let clamp_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui-clamp-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let viewport_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-viewport-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    // Early, named failure (`BindingSizeTooSmall`) instead of a
                    // draw-time "bound with size N where the shader expects M".
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<ViewportUniform>() as u64,
                    ),
                },
                count: None,
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui-shader"),
            source: wgpu::ShaderSource::Wgsl(UI_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-pipeline-layout"),
            bind_group_layouts: &[Some(&viewport_layout), Some(texture_layout)],
            immediate_size: 0,
        });
        let attributes = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 8,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 2,
            },
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<UiVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui-viewport"),
            size: std::mem::size_of::<ViewportUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let viewport_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-viewport-bind"),
            layout: &viewport_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });

        let font_bind = upload_levels(
            device,
            queue,
            texture_layout,
            &clamp_sampler,
            Role::Coverage,
            &single_level(font_atlas),
        );
        let minimap_bind = match minimap {
            Some(image) => upload_levels(
                device,
                queue,
                texture_layout,
                &clamp_sampler,
                Role::Color,
                &single_level(image),
            ),
            None => upload_levels(
                device,
                queue,
                texture_layout,
                &clamp_sampler,
                Role::Color,
                &single_level(&TextureImage::white()),
            ),
        };

        Self {
            pipeline,
            viewport_buffer,
            viewport_bind,
            vertices: DynamicBuffer::new(
                device,
                64 * 1024,
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                "ui-vertices",
            ),
            scratch: Scratch::with_capacity(12 * 1024),
            runs: Scratch::with_capacity(16),
            white_start: 0,
            white_count: 0,
            font_start: 0,
            font_count: 0,
            minimap_start: 0,
            minimap_count: 0,
            font_bind,
            minimap_bind,
        }
    }

    /// Build and upload this frame's UI vertices. Returns false when the batch
    /// is empty and the pass can be skipped entirely.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        stats: &mut Counters,
    ) -> bool {
        let ui = &scene.ui;
        self.runs.clear();
        self.scratch.clear();
        if ui.vertex_count() == 0 {
            return false;
        }
        self.vertices.begin_frame();
        // One linear pass over the batch: textured runs (authoring order, panels
        // first), then flat white, font glyphs and the minimap.
        for draw in &ui.textured {
            let start = self.scratch.len() as u32;
            self.scratch.extend_from_slice(&draw.vertices);
            let count = self.scratch.len() as u32 - start;
            if count > 0 {
                self.runs.push((draw.texture, start, count));
            }
        }
        let white_start = self.scratch.len() as u32;
        self.scratch.extend_from_slice(&ui.white);
        let white_count = self.scratch.len() as u32 - white_start;
        let font_start = self.scratch.len() as u32;
        self.scratch.extend_from_slice(&ui.font);
        let font_count = self.scratch.len() as u32 - font_start;
        let minimap_start = self.scratch.len() as u32;
        self.scratch.extend_from_slice(&ui.minimap);
        let minimap_count = self.scratch.len() as u32 - minimap_start;

        self.vertices
            .write(device, queue, bytemuck::cast_slice(self.scratch.as_slice()));
        stats.upload_bytes += self.vertices.bytes;
        stats.buffer_writes += self.vertices.writes;
        stats.buffer_grows += self.vertices.grows;

        let viewport = ViewportUniform {
            size: [scene.viewport.x, scene.viewport.y, 1.0, 1.0],
        };
        queue.write_buffer(&self.viewport_buffer, 0, bytemuck::bytes_of(&viewport));
        stats.upload_bytes += std::mem::size_of::<ViewportUniform>() as u64;
        stats.buffer_writes += 1;

        self.white_start = white_start;
        self.white_count = white_count;
        self.font_start = font_start;
        self.font_count = font_count;
        self.minimap_start = minimap_start;
        self.minimap_count = minimap_count;
        true
    }

    /// Record the UI batch into an existing pass (the game owns the UI pass).
    pub fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        textures: &TextureStore,
        stats: &mut Counters,
    ) {
        pass.set_pipeline(&self.pipeline);
        stats.pipeline_binds += 1;
        pass.set_bind_group(0, &self.viewport_bind, &[]);
        pass.set_vertex_buffer(0, self.vertices.buffer().slice(..));
        stats.passes += 1;

        let mut last_texture = u32::MAX;
        for &(texture, start, count) in self.runs.as_slice() {
            if texture as u32 != last_texture {
                if let Some(bind) = textures.bind_group(texture) {
                    pass.set_bind_group(1, bind, &[]);
                    last_texture = texture as u32;
                    stats.texture_binds += 1;
                }
            }
            pass.draw(start..start + count, 0..1);
            stats.draws += 1;
            stats.vertices += count as u64;
        }
        if self.white_count > 0 {
            if let Some(bind) = textures.bind_group(0) {
                pass.set_bind_group(1, bind, &[]);
                stats.texture_binds += 1;
            }
            pass.draw(
                self.white_start..self.white_start + self.white_count,
                0..1,
            );
            stats.draws += 1;
            stats.vertices += self.white_count as u64;
        }
        if self.font_count > 0 {
            pass.set_bind_group(1, &self.font_bind, &[]);
            stats.texture_binds += 1;
            pass.draw(self.font_start..self.font_start + self.font_count, 0..1);
            stats.draws += 1;
            stats.vertices += self.font_count as u64;
        }
        if self.minimap_count > 0 {
            pass.set_bind_group(1, &self.minimap_bind, &[]);
            stats.texture_binds += 1;
            pass.draw(
                self.minimap_start..self.minimap_start + self.minimap_count,
                0..1,
            );
            stats.draws += 1;
            stats.vertices += self.minimap_count as u64;
        }
    }

    /// Convenience: prepare and record a complete UI pass into `color`.
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        scene: &Scene,
        textures: &TextureStore,
        stats: &mut Counters,
    ) {
        if !self.prepare(device, queue, scene, stats) {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
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
        self.draw(&mut pass, textures, stats);
    }
}

fn single_level(image: &TextureImage) -> Vec<Mip> {
    vec![Mip {
        width: image.width.max(1),
        height: image.height.max(1),
        rgba: image.rgba.clone(),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `render.uniform.ui-viewport`: the Rust `ViewportUniform` and the WGSL
    /// `Viewport` must be the same size, so the HUD pass can never bind a
    /// buffer the shader does not fully cover (the 112-vs-176 camera abort in
    /// `analysis/crash-20261117.txt` was exactly this class of drift).
    #[test]
    fn viewport_uniform_matches_the_wgsl_struct() {
        assert_eq!(std::mem::size_of::<ViewportUniform>(), 16);
        assert_eq!(
            crate::mesh::wgsl_struct_size(UI_SHADER, "Viewport"),
            std::mem::size_of::<ViewportUniform>()
        );
    }
}
