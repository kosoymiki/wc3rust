//! Fog of war overlay: unexplored black, explored dark, visible untouched.
//!
//! The game's simulation owns the per-player visibility/explored grid
//! (`wc3_sim::Sim::is_visible` / `is_explored`). The 3D renderer draws the WC3
//! mask as a depth-aware full-screen pass:
//!
//! * depth is read back to reconstruct the world position of every pixel, so the
//!   mask follows terrain height and never paints over the HUD;
//! * unexplored cells are opaque black ([`UNEXPLORED_DARKNESS`], luminance 0);
//! * explored-but-not-visible cells keep [`EXPLORED_REMAINING`] (45 %) of the
//!   lit pixel as seen on an sRGB target — the veil alpha is
//!   [`EXPLORED_DARKNESS`], compensated for linear-space blending;
//! * visible cells are left untouched (the shader discards);
//! * the state grid is blurred with a 2-cell box before upload, so the boundary
//!   is a soft gradient instead of a per-cell staircase (bilinear sampling of a
//!   nearest mask still shows the 128-unit lattice).
//!
//! `H2_FOW=off` (or `0`) disables the pass for A/B captures; the HUD and the
//! minimap keep reading the same simulation grid either way, so the two views
//! never disagree about visibility (`Game::minimap_fog_cells` uses the same
//! `is_visible`/`is_explored` predicates and the same 0/1/2 encoding).

/// Darkness of a never-seen cell: opaque black.
pub const UNEXPLORED_DARKNESS: f32 = 1.0;
/// Fraction of the lit pixel an explored-but-not-visible cell keeps **as seen**
/// on screen (WC3's 35…55 % band).
pub const EXPLORED_REMAINING: f32 = 0.45;
/// Veil alpha that produces [`EXPLORED_REMAINING`] on an sRGB target.
///
/// The world pass writes linear light and the target is `*Srgb`, so blending
/// happens in linear space: a 0.55 alpha leaves 0.45 *linear*, which encodes to
/// 0.69 — the explored band read almost as bright as the visible one on the
/// device (measured 0.672 of the lit frame). `1 - 0.45^2.2 = 0.834` restores
/// the authored perceptual 45 %.
pub const EXPLORED_DARKNESS: f32 = 0.834;
/// Radius of the state blur in cells (2 -> 5x5 box, ~2-cell gradient).
pub const EDGE_BLUR_CELLS: i32 = 2;

/// Fraction of the lit pixel a viewer sees under a veil alpha `darkness`.
///
/// Encodes the sRGB target: the blend is linear, the frame is not, so the
/// perceptual remainder is `(1 - alpha)^(1/2.2)`.
pub fn remaining_ratio(darkness: f32) -> f32 {
    (1.0 - darkness.clamp(0.0, 1.0)).powf(1.0 / 2.2)
}

/// Per-player fog grid. `cells` values: 0 unexplored, 1 explored, 2 visible.
#[derive(Clone, Debug)]
pub struct FogMask {
    pub width: u32,
    pub height: u32,
    pub cell_size: f32,
    pub cells: Vec<u8>,
}

impl FogMask {
    pub fn new(width: u32, height: u32, cell_size: f32, cells: Vec<u8>) -> Self {
        debug_assert_eq!(cells.len(), (width * height) as usize);
        Self {
            width,
            height,
            cell_size,
            cells,
        }
    }

    pub fn state_at(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.cells[(y * self.width + x) as usize]
    }

    pub fn visible_fraction(&self) -> f32 {
        if self.cells.is_empty() {
            return 0.0;
        }
        let visible = self.cells.iter().filter(|state| **state == 2).count();
        visible as f32 / self.cells.len() as f32
    }

    /// `(unexplored, explored, visible)` cell counts, for logs and evidence.
    pub fn histogram(&self) -> (usize, usize, usize) {
        let mut counts = (0, 0, 0);
        for state in &self.cells {
            match state {
                2 => counts.2 += 1,
                1 => counts.1 += 1,
                _ => counts.0 += 1,
            }
        }
        counts
    }

    /// `true` when the three states differ from `previous` (the renderer only
    /// rebuilds the mask texture when the simulation grid actually changed).
    pub fn differs_from(&self, previous: &FogMask) -> bool {
        self.width != previous.width
            || self.height != previous.height
            || self.cell_size != previous.cell_size
            || self.cells != previous.cells
    }
}

/// `H2_FOW=off|0` disables the fog-of-war pass (diagnostics / A-B captures).
pub fn enabled() -> bool {
    !matches!(
        std::env::var("H2_FOW"),
        Ok(value) if value.eq_ignore_ascii_case("off") || value == "0"
    )
}

/// Per-cell darkness for the fog pass: 0 = visible, [`EXPLORED_DARKNESS`] =
/// explored, [`UNEXPLORED_DARKNESS`] = unexplored, then one
/// [`EDGE_BLUR_CELLS`]-radius box blur. The blur is the whole point: the mask is
/// sampled bilinearly, so a hard 0/1/2 lattice turned the fog boundary into a
/// 128-unit staircase ("the shadow scatters into pixels"). Softening it on the
/// CPU gives the ~2-cell gradient WC3 has.
pub fn darkness_grid(mask: &FogMask) -> Vec<f32> {
    let (gw, gh) = (mask.width.max(1) as usize, mask.height.max(1) as usize);
    let mut darkness = vec![0.0f32; gw * gh];
    for (index, state) in mask.cells.iter().enumerate().take(gw * gh) {
        darkness[index] = match state {
            2 => 0.0,
            1 => EXPLORED_DARKNESS,
            _ => UNEXPLORED_DARKNESS,
        };
    }
    let mut blurred = darkness.clone();
    let radius = EDGE_BLUR_CELLS.max(0);
    for y in 0..gh {
        for x in 0..gw {
            let mut sum = 0.0f32;
            let mut count = 0.0f32;
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && ny >= 0 && (nx as usize) < gw && (ny as usize) < gh {
                        sum += darkness[ny as usize * gw + nx as usize];
                        count += 1.0;
                    }
                }
            }
            blurred[y * gw + x] = sum / count.max(1.0);
        }
    }
    blurred
}

const FOG_SHADER: &str = r#"
struct FogParams {
    inv_view_proj: mat4x4<f32>,
    // [viewport_w, viewport_h, grid_w, grid_h]
    view: vec4<f32>,
    // [cell_size, 0, 0, 0]
    grid: vec4<f32>,
};
@group(0) @binding(0) var<uniform> params: FogParams;
@group(0) @binding(1) var fog_tex: texture_2d<f32>;
@group(0) @binding(2) var depth_tex: texture_depth_2d;
@group(0) @binding(3) var fog_samp: sampler;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One oversized triangle covering the viewport.
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[index], 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = pixel.xy / params.view.xy;
    let depth = textureLoad(depth_tex, vec2<i32>(pixel.xy), 0);
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let world4 = params.inv_view_proj * vec4<f32>(ndc, depth, 1.0);
    if (world4.w <= 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let world = world4.xyz / world4.w;
    let cell = floor(world.xz / params.grid.x);
    if (cell.x < 0.0 || cell.y < 0.0 || cell.x >= params.view.z || cell.y >= params.view.w) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    // The mask stores darkness (0 = fully visible, 0.55 = explored, 1 =
    // unexplored) and is sampled **bilinearly**: a per-cell nearest lookup made
    // the fog boundary a hard 128-unit staircase ("shadow scatters into pixels").
    // The CPU side already blurred the grid by two cells, so this bilinear tap
    // finishes the gradient instead of creating it.
    // Texel centres: world/cell + 0.5 over the grid extent.
    let mask_uv = (world.xz / params.grid.x + vec2<f32>(0.5)) / params.view.zw;
    let darkness = textureSampleLevel(fog_tex, fog_samp, mask_uv, 0.0).r;
    if (darkness <= 0.004) {
        discard; // visible: full colour
    }
    return vec4<f32>(0.0, 0.0, 0.0, clamp(darkness, 0.0, 1.0));
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FogParams {
    inv_view_proj: [f32; 16],
    view: [f32; 4],
    grid: [f32; 4],
}

/// Full-screen fog pass. One pipeline, one R8 mask texture, one uniform.
pub struct FogRenderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    mask: Option<wgpu::Texture>,
    mask_size: (u32, u32),
    sampler: wgpu::Sampler,
    bind: Option<wgpu::BindGroup>,
}

impl FogRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fog-shader"),
            source: wgpu::ShaderSource::Wgsl(FOG_SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fog-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        // Fail at bind-group creation with `BindingSizeTooSmall`
                        // (named, recoverable) instead of at the first draw with
                        // "bound with size N where the shader expects M" — the
                        // class of abort that killed build 0f037ea2.
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<FogParams>() as u64,
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fog-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fog-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
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
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fog-uniform"),
            size: std::mem::size_of::<FogParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fog-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        Self {
            pipeline,
            layout,
            uniform,
            mask: None,
            mask_size: (0, 0),
            sampler,
            bind: None,
        }
    }

    /// Upload the mask and parameters for this frame.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mask: &FogMask,
        inv_view_proj: glam::Mat4,
        viewport: glam::Vec2,
        depth: &wgpu::TextureView,
    ) {
        if self.mask_size != (mask.width, mask.height) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("fog-mask"),
                size: wgpu::Extent3d {
                    width: mask.width.max(1),
                    height: mask.height.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                // One channel is all the pass reads. The Rgba8Unorm texture the
                // first version uploaded wasted 4x the bandwidth per frame
                // (256 KB instead of 64 KB on a 128x128 grid) for three bytes
                // the shader never touched.
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.mask = Some(texture);
            self.mask_size = (mask.width, mask.height);
        }
        let texture = self.mask.as_ref().expect("fog mask");
        let (gw, gh) = (mask.width.max(1) as usize, mask.height.max(1) as usize);
        let blurred = darkness_grid(mask);
        let mut bytes = Vec::with_capacity(gw * gh);
        for value in &blurred {
            bytes.push((value.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(mask.width.max(1)),
                rows_per_image: Some(mask.height.max(1)),
            },
            wgpu::Extent3d {
                width: mask.width.max(1),
                height: mask.height.max(1),
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fog-bind"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        }));
        let params = FogParams {
            inv_view_proj: inv_view_proj.to_cols_array(),
            view: [viewport.x, viewport.y, mask.width as f32, mask.height as f32],
            grid: [mask.cell_size.max(1.0), 0.0, 0.0, 0.0],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&params));
    }

    /// Record the overlay into a colour pass. The depth texture is bound as a
    /// texture view by the caller (see [`Self::set_depth_view`]).
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(bind) = &self.bind else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// `render.uniform.fog`: the Rust `FogParams` and the WGSL `FogParams` the
    /// fog shader declares must be the same 96 B, or the pass aborts at draw time
    /// ("bound with size N where the shader expects M", `analysis/crash-20261117.txt`).
    #[test]
    fn fog_uniform_matches_the_wgsl_struct() {
        assert_eq!(std::mem::size_of::<FogParams>(), 96);
        assert_eq!(
            crate::mesh::wgsl_struct_size(FOG_SHADER, "FogParams"),
            std::mem::size_of::<FogParams>()
        );
    }

    /// Mechanic `rendering.fog`: the mask keeps the three WC3 states distinct and
    /// the world-to-cell lookup is bounds-safe.
    /// Mechanic `rendering.fog-edge`: the fog boundary is a gradient, not a
    /// per-cell staircase. A fully visible grid stays 0 everywhere; an edge
    /// between visible and unexplored must produce intermediate values.
    #[test]
    fn fog_edge_is_a_gradient() {
        // 9x1: cells 2 2 2 2 2 0 0 0 0  (visible … then unexplored).
        // The grid has to be wider than the 2-cell blur radius, or even the
        // leftmost cell averages in the dark side and reads as 0.33 dark.
        let mask = FogMask::new(9, 1, 128.0, vec![2, 2, 2, 2, 2, 0, 0, 0, 0]);
        let grid = darkness_grid(&mask);
        assert_eq!(grid.len(), 9);
        assert!(grid.iter().all(|v| (0.0..=1.0).contains(v)));
        // Far left of the edge stays visible, far right of it is fully dark.
        assert!(grid[0] < 0.05, "visible cell must stay bright: {}", grid[0]);
        assert!(grid[8] > 0.7, "unexplored cell must stay dark: {}", grid[8]);
        // The edge itself is a blend, never a single 0 -> 1 jump.
        let blended = grid.iter().filter(|v| **v > 0.05 && **v < 0.95).count();
        assert!(
            blended >= 2,
            "the edge must blend over cells, got {grid:?} (blended={blended})"
        );
    }

    #[test]
    fn visible_grid_has_no_fog() {
        let mask = FogMask::new(3, 3, 128.0, vec![2; 9]);
        assert!(darkness_grid(&mask).iter().all(|v| *v < 1e-6));
    }

    #[test]
    fn rendering_fog_mask_states() {
        let mask = FogMask::new(2, 2, 128.0, vec![0, 1, 2, 0]);
        assert_eq!(mask.state_at(0, 0), 0, "unexplored");
        assert_eq!(mask.state_at(1, 0), 1, "explored");
        assert_eq!(mask.state_at(0, 1), 2, "visible");
        assert_eq!(mask.state_at(9, 9), 0, "out of bounds is unexplored");
        assert!((mask.visible_fraction() - 0.25).abs() < 1e-6);
        assert_eq!(mask.cell_size, 128.0);
    }
}
