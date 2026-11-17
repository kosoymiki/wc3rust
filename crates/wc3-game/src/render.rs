//! Game renderer facade over `wc3-render`.
//!
//! There is exactly one renderer in the base: `wc3_render::mesh::MeshRenderer`
//! (static geometry + GPU skinning) and `wc3_render::ui::UiRenderer`. This module
//! owns only what is game-specific:
//!
//! * the pass order the Warcraft 3 look depends on — opaque terrain, opaque unit
//!   layers, translucent water, blended/additive layers, decals;
//! * the conversion from game assets (`Assets`, `DoodadWorld`) into the renderer's
//!   data types;
//! * the reusable GPU scene buffer, so a frame does not allocate.
//!
//! Draw order (unchanged from the pre-merge renderer, and the reason water can
//! occlude legs while blended team-colour layers still composite on top):
//! 1. opaque terrain (writes depth),
//! 2. opaque/alpha-test unit and doodad layers (write depth),
//! 3. translucent water (tests depth, never writes it — `donors/mdx-m3-viewer/
//!    src/viewer/handlers/mdx/layer.ts:57`),
//! 4. blended/additive layers,
//! 5. selection rings / move markers and doodad shadow decals.

use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3};

use wc3_render::mesh::{
    BoneWeight as GpuBoneWeight, MeshData as GpuMeshData, MeshLayer as GpuMeshLayer, MeshVertex,
    Model, StaticBatch, StaticDrawSpec,
};
use wc3_render::scene::{
    PassMask, Ring as GpuRing, Scene as GpuScene, UiDraw as GpuUiDraw, UiVertex as GpuUiVertex,
    UnitDraw as GpuUnitDraw,
};
use wc3_render::texture::TextureImage as GpuTexture;
use wc3_render::texture::TextureSource;
use wc3_render::fog::{FogMask, FogRenderer};
use wc3_render::{Counters, MeshRenderer, UiRenderer};

use crate::assets::{Assets, TextureImage};
use crate::doodads::DoodadWorld;
use crate::game::Scene;
use crate::terrain_render::TerrainRenderer;

/// Texture cap for the model/doodad store. Authored sizes are kept (never
/// upscaled); anything larger is capped and mipped.
///
/// This store also carries the UI batch textures (`Assets::textures` is one
/// list), so the cap must not downsample the pack's authored UI art or the
/// glyph atlases: 256 turned a 2048x1024 font atlas into 256x128 (every label
/// read as the same smeared glyph) and squashed the 720x32 topbar. 2048 keeps
/// every authored UI texture at 1:1 while still capping oversized model art.
const MAX_TEXTURE_SIZE: u32 = 2048;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    /// Direction towards the sun (DNC `KGRT`), Y-up.
    light: [f32; 4],
    /// DNC `sun` colour × intensity (linear), `w` reserved for exposure.
    sun: [f32; 4],
    /// DNC `ambient` colour × ambient intensity (linear).
    ambient: [f32; 4],
    /// Flat-surface light `[r, g, b, 1]` (diagnostics / A-B captures).
    day: [f32; 4],
    /// Atmospheric fog colour (linear) and density in `.a`.
    fog: [f32; 4],
    /// `x` = haze start distance, `y` = haze end distance (world units).
    fog_range: [f32; 4],
    /// `xyz` = eye position, `w` = 1.
    eye: [f32; 4],
}

/// Bytes one camera uniform occupies. The terrain shader's WGSL `Camera` is
/// `mat4x4<f32>` + 7 × `vec4<f32>` = 176 B; binding this struct's buffer at any
/// other size is a hard wgpu validation error (build 0f037ea2 aborted with
/// "bound with size 112 where the shader expects 176").
const CAMERA_UNIFORM_BYTES: usize = std::mem::size_of::<CameraUniform>();
const _: () = assert!(
    CAMERA_UNIFORM_BYTES == wc3_render::mesh::CAMERA_UNIFORM_SIZE,
    "the game camera uniform must match wc3-render's 176-byte `Camera`"
);
const _: () = assert!(
    CAMERA_UNIFORM_BYTES == 176,
    "game CameraUniform must stay 176 B (mat4x4 + 7 vec4)"
);

// ---------------------------------------------------------------------------
// Validation-error instrumentation (ledger 480, packet A)
// ---------------------------------------------------------------------------

/// Pass labels, in the order `LAST_PASS` indexes them.
const PASS_ENCODE: &str = "encode";
const PASS_WORLD: &str = "world";
const PASS_FOW: &str = "fow";
const PASS_HUD: &str = "hud";
const PASS_LABELS: [&str; 4] = [PASS_ENCODE, PASS_WORLD, PASS_FOW, PASS_HUD];

/// The pass whose commands are being recorded right now.
///
/// wgpu reports a draw-time validation error against the *encoder* label
/// (`In a CommandEncoder, label = 'game-encoder'`, `analysis/crash-20261117.txt`),
/// never the pass, so the error handler reads this to name the pass.
static LAST_PASS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn note_pass(index: usize) {
    LAST_PASS.store(index, std::sync::atomic::Ordering::Relaxed);
}

fn last_pass() -> &'static str {
    let index = LAST_PASS
        .load(std::sync::atomic::Ordering::Relaxed)
        .min(PASS_LABELS.len() - 1);
    PASS_LABELS[index]
}

/// `H2_PASS_SCOPE=0` disables the per-pass validation scopes; they are on by
/// default because a named pass is what turns an opaque abort into a bug report.
fn pass_scopes_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        !matches!(
            std::env::var("H2_PASS_SCOPE").as_deref(),
            Ok("0" | "off" | "false")
        )
    })
}

/// Log one validation error as `P0_PASS_ERROR pass=<label> …` (bounded: a broken
/// frame can raise thousands).
fn log_pass_error(source: &str, pass: &str, error: &str) {
    static COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    if n <= 8 {
        crate::dev_log!(
            "P0_PASS_ERROR pass={} last={} source={} n={} err={}",
            pass,
            last_pass(),
            source,
            n,
            error
        );
    } else if n == 9 {
        crate::dev_log!(
            "P0_PASS_ERROR_MORE pass={} further validation errors are counted only",
            pass
        );
    }
}

/// Poll a wgpu error-scope future once, without an executor.
///
/// `wgpu-core` resolves the pop future with `ready(scope.error)`, so the first
/// poll is enough; `None` means the scope has not been processed yet (the
/// uncaptured-error handler still catches it).
fn poll_error<F: std::future::Future>(future: F) -> Option<F::Output> {
    use std::task::{Context, Poll, Waker};

    struct Noop;
    impl std::task::Wake for Noop {
        fn wake(self: std::sync::Arc<Self>) {}
    }
    let waker = Waker::from(std::sync::Arc::new(Noop));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}

/// RAII validation scope for one recorded pass.
///
/// Pushing `Validation` around a pass and popping it at the end of the block
/// attributes the error to that pass; without it wgpu's default handler
/// (`default_error_handler` -> `panic!`) aborts the process with only the
/// encoder label, which is what build 0f037ea2 did.
struct PassScope {
    label: &'static str,
    guard: Option<wgpu::ErrorScopeGuard>,
}

impl PassScope {
    fn new(device: &wgpu::Device, label: &'static str) -> Self {
        Self {
            label,
            guard: pass_scopes_enabled()
                .then(|| device.push_error_scope(wgpu::ErrorFilter::Validation)),
        }
    }
}

impl Drop for PassScope {
    fn drop(&mut self) {
        let Some(guard) = self.guard.take() else {
            return;
        };
        if let Some(Some(error)) = poll_error(guard.pop()) {
            log_pass_error("scope", self.label, &error.to_string());
        }
    }
}

/// WC3-style day/night grade for a clock hour in `0..24`.
///
/// The curve is data, not taste: it comes from the shipped DNC light models
/// (`[TerrainLights]` / `[UnitLights]` in `UI/WorldEditData.txt`). See
/// [`wc3_render::daynight`] for the extraction and the frame-order contract.
/// The value is `[tint_r, tint_g, tint_b, brightness]` and is applied to
/// terrain, water, units and doodads; the HUD keeps its own brightness (as in
/// WC3).
pub fn day_night_light(hours: f32) -> [f32; 4] {
    wc3_render::daynight::DayNightCurve::default()
        .sample(hours)
        .terrain
        .grade
}

/// Tick-driven time of day for the simulation.
///
/// WC3's clock is a function of elapsed game time (`DayLength=480` real seconds
/// per 24 game hours, `Units/MiscData.txt:63`), so it needs no mutable state on
/// `Sim`: `time_of_day = tick / 30 / 480 * 24`. `sim.time_of_day()` is the
/// renderer-free half of the clock (the renderer advances the same curve from
/// frame time, and both agree while the game runs at real-time 30 Hz).
pub trait SimDayNight {
    /// Clock hour in `0..24`.
    fn time_of_day(&self) -> f32;
    /// 1-based game day.
    fn game_day(&self) -> u32;
    /// WC3 `daytime` flag inverted (`Dawn=6`, `Dusk=18`).
    fn is_night(&self) -> bool;
}

impl SimDayNight for wc3_sim::Sim {
    fn time_of_day(&self) -> f32 {
        wc3_render::daynight::DayNightClock::new().time_of_day_at_tick(self.tick)
    }

    fn game_day(&self) -> u32 {
        wc3_render::daynight::DayNightClock::new().day_at_tick(self.tick)
    }

    fn is_night(&self) -> bool {
        wc3_render::daynight::is_night(self.time_of_day())
    }
}

/// What to include in a headless frame. The visual regression tests render
/// units without terrain (and terrain without units) to measure them in
/// isolation; the game always uses the default (everything).
#[derive(Clone, Copy, Default)]
pub struct RenderOptions {
    pub skip_terrain: bool,
    pub skip_units: bool,
    pub skip_ui: bool,
    pub skip_doodads: bool,
}

/// Game-side renderer: terrain (world layer) + `wc3-render` world/UI passes.
pub struct Renderer {
    width: u32,
    height: u32,
    camera_buffer: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    terrain: TerrainRenderer,
    mesh: MeshRenderer,
    ui: UiRenderer,
    doodads: Option<StaticBatch>,
    /// Reused scene buffer: refilled in place every frame (no allocation).
    gpu_scene: GpuScene,
    counters: Counters,
    /// Frame clock for the animated Water.slk surface scroll.
    clock: std::time::Instant,
    depth: Option<wgpu::TextureView>,
    fog: FogRenderer,
    fog_mask: Option<FogMask>,
    /// Model index -> unit kind, for the one-shot skinning log.
    model_kinds: Vec<String>,
    /// Clock hour for the day/night grade (`None` = advance with real time).
    day_night: wc3_render::daynight::DayNightState,
    /// DNC curve for the current tileset (Lordaeron by default).
    daynight: wc3_render::daynight::DayNightCurve,
    /// Last frame timestamp, for the free-running clock advance.
    last_frame: std::time::Instant,
    /// Last hour written to `GAME_TIME`/`GAME_LIGHT` (logs once per game hour).
    logged_hour: i32,
    /// Last fog-of-war histogram written to `GAME_FOW`.
    logged_fow: Option<(usize, usize, usize)>,
    /// Render the ungraded reference frame (flat white light). Defaults to
    /// `H2_DAYNIGHT=identity`; the evidence captures switch it per frame.
    identity_light: bool,
    /// The skinning diagnostic is printed once, on the first prepared frame.
    skin_logged: bool,
}

impl Renderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        assets: &Assets,
        font_atlas: &TextureImage,
        minimap: Option<&TextureImage>,
    ) -> Result<Self, String> {
        // The camera contract is 176 B on all four sides: this struct, the
        // buffer bound at (0,0), the mesh WGSL `Camera` and the terrain WGSL
        // `Camera`. Build 0f037ea2 shipped with the WGSL side already grown to
        // 176 B while this struct was still the old 112 B shape (`mat4x4` +
        // `light`, `sun`, `ambient`) and died at the first terrain draw
        // (`bound with size 112 where the shader expects 176`). Checking both
        // shader copies here fails with a readable error instead of aborting
        // mid-frame, and covers the terrain copy, which no assert used to read.
        let mesh_camera_bytes = wc3_render::mesh::camera_uniform_size_from_wgsl();
        let terrain_camera_bytes =
            wc3_render::mesh::wgsl_struct_size(crate::terrain_render::TERRAIN_SHADER, "Camera");
        if terrain_camera_bytes != CAMERA_UNIFORM_BYTES || mesh_camera_bytes != CAMERA_UNIFORM_BYTES
        {
            return Err(format!(
                "camera uniform mismatch: bound buffer {} B, mesh WGSL `Camera` {} B, \
                 terrain WGSL `Camera` {} B — the two sides must agree before a bind group is built",
                CAMERA_UNIFORM_BYTES, mesh_camera_bytes, terrain_camera_bytes
            ));
        }
        // Replace wgpu's default error handler (an unconditional abort, the
        // SIGABRT in analysis/crash-20261117.txt) with one that names the pass
        // and keeps the process alive.
        device.on_uncaptured_error(std::sync::Arc::new(|error: wgpu::Error| {
            log_pass_error("uncaptured", last_pass(), &error.to_string());
        }));
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("game-camera-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    // Fail at bind-group creation (clear message) instead of at
                    // draw time (SIGABRT) when the Rust struct and the WGSL
                    // `Camera` drift apart.
                    min_binding_size: std::num::NonZeroU64::new(CAMERA_UNIFORM_BYTES as u64),
                },
                count: None,
            }],
        });
        let texture_layout = wc3_render::texture::bind_group_layout(device);

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("game-camera-buffer"),
            size: CAMERA_UNIFORM_BYTES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // One line of device evidence for the 176-byte contract: the Rust
        // struct, the buffer that is really bound and both shader copies.
        crate::dev_log!(
            "GAME_CAMERA_UNIFORM bytes={} buffer_bytes={} mesh_wgsl={} terrain_wgsl={} terrain_bind=game upholds=176",
            CAMERA_UNIFORM_BYTES,
            camera_buffer.size(),
            mesh_camera_bytes,
            terrain_camera_bytes
        );
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("game-camera-bind"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let doodads = DoodadWorld::load(assets);
        let models = gpu_models(assets);
        // Upload straight from the asset layer: a cloned `Vec<TextureImage>`
        // held a second full copy (203 MB on the shipped install) next to the
        // originals, which is what pushed VmHWM to 1.8 GB on the phone.
        let mut sources: Vec<TextureSource<'_>> = assets
            .textures
            .iter()
            .map(TextureSource::Rgba)
            .collect();
        let doodad_texture_base = sources.len();
        sources.extend(doodads.textures.iter().map(TextureSource::Rgba));
        let gpu_vertices: usize = models
            .iter()
            .map(|model| model.iter().map(|mesh| mesh.positions.len()).sum::<usize>())
            .sum();
        let gpu_indices: usize = models
            .iter()
            .map(|model| model.iter().map(|mesh| mesh.indices.len()).sum::<usize>())
            .sum();

        // One renderer for every mesh in the base: skinned units, merged doodads
        // and the terrain batch all share the pipeline family and vertex layout.
        let mesh = MeshRenderer::new_with_sources(
            device,
            queue,
            format,
            models,
            &sources,
            MAX_TEXTURE_SIZE,
        );
        drop(sources);
        let texture_stats = mesh.texture_stats();
        crate::dev_log!(
            "GAME_MEM gpu_textures={} gpu_texture_mb={:.1} gpu_levels={} gpu_vertices={} gpu_vertex_mb={:.1} gpu_indices={}",
            texture_stats.textures,
            texture_stats.bytes as f64 / (1024.0 * 1024.0),
            texture_stats.levels,
            gpu_vertices,
            (gpu_vertices * wc3_render::VERTEX_STRIDE as usize) as f64 / (1024.0 * 1024.0),
            gpu_indices
        );

        // Doodads are static: merge every instance once, then draw the batch.
        let (doodad_vertices, doodad_indices, doodad_specs) =
            build_doodad_geometry(&doodads, doodad_texture_base);
        let doodad_batch = if doodad_specs.is_empty() {
            None
        } else {
            Some(mesh.create_static_batch(
                device,
                queue,
                &doodad_vertices,
                &doodad_indices,
                &doodad_specs,
            ))
        };
        crate::dev_log!(
            "GAME_DOODADS parsed={} placed={} models={} model_failures={} meshless={} texture_failures={} vertices={} draws={}",
            doodads.stats.parsed,
            doodads.stats.placed,
            doodads.stats.models,
            doodads.stats.model_failures,
            doodads.stats.meshless_models,
            doodads.stats.texture_failures,
            doodad_vertices.len(),
            doodad_specs.len()
        );

        let font_atlas = gpu_texture(font_atlas);
        let minimap = minimap.map(gpu_texture);
        let ui = UiRenderer::new(
            device,
            queue,
            format,
            &texture_layout,
            &font_atlas,
            minimap.as_ref(),
        );

        let terrain = TerrainRenderer::new(
            device,
            queue,
            format,
            &camera_layout,
            &texture_layout,
            assets,
        );

        let model_kinds = assets
            .models
            .iter()
            .map(|model| format!("{:?}", model.kind))
            .collect();

        Ok(Self {
            width: 0,
            height: 0,
            camera_buffer,
            camera_bind,
            terrain,
            mesh,
            ui,
            doodads: doodad_batch,
            gpu_scene: GpuScene::empty(glam::Mat4::IDENTITY, glam::Vec2::ONE),
            counters: Counters::default(),
            clock: std::time::Instant::now(),
            depth: None,
            fog: FogRenderer::new(device, format),
            fog_mask: None,
            model_kinds,
            skin_logged: false,
            day_night: wc3_render::daynight::DayNightState::new(),
            // Map override: `war3map.w3e` carries the tileset that selects the
            // `[TerrainLights]` / `[UnitLights]` DNC pair. Maps without a W3E
            // keep the Lordaeron default (the table's documented fallback).
            daynight: {
                let mut curve = wc3_render::daynight::DayNightCurve::new(
                    assets
                        .terrain
                        .as_ref()
                        .map(|terrain| {
                            wc3_render::daynight::Preset::for_tileset(terrain.tileset as char)
                        })
                        .unwrap_or(wc3_render::daynight::Preset::Lordaeron),
                );
                // Maps without a `war3map.w3i` fog block still get the default
                // atmosphere (`H2_ATMOSPHERE=off` disables it).
                curve.set_atmosphere(wc3_render::daynight::atmosphere_enabled());
                curve
            },
            last_frame: std::time::Instant::now(),
            logged_hour: i32::MIN,
            logged_fow: None,
            identity_light: wc3_render::daynight::identity_light(),
        })
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("game-depth"),
            size: wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wc3_render::mesh::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        self.depth = Some(texture.create_view(&wgpu::TextureViewDescriptor::default()));
    }

    /// Counters of the last recorded frame (draws, upload bytes, binds, …).
    pub fn counters(&self) -> Counters {
        self.counters
    }

    /// Force a skinning path. The Android app selects the CPU path because the
    /// vendor Vulkan driver renders GPU-skinned models in bind pose; headless
    /// frames and the bench keep the GPU default.
    pub fn set_skin_mode(&mut self, mode: wc3_render::mesh::SkinMode) {
        self.mesh.set_skin_mode(mode);
    }

    /// **Day/night source of truth.** Follow the simulation's clock hour.
    ///
    /// Call once per frame with the tick-derived hour. The value is stored, not
    /// advanced: the renderer stops running its own clock, so the grade can no
    /// longer disagree with `GAME_TIME` (the reported defect: the log said
    /// `day=1 hour=00.00` while the frame rendered at the renderer's private
    /// clock, several game hours ahead).
    ///
    /// ```ignore
    /// use wc3_game::render::SimDayNight;
    /// renderer.sync_day_night(game.sim.time_of_day());
    /// ```
    pub fn sync_day_night(&mut self, hours: f32) {
        // No `logged_hour` reset here: this runs every frame, and the once-per-
        // game-hour log is driven by the integer-hour change in `render_with`.
        self.day_night.sync(hours);
    }

    /// Hand the hour back to the free-running clock (tools with no simulation).
    pub fn release_day_night(&mut self) {
        self.day_night.release();
        self.last_frame = std::time::Instant::now();
    }

    /// Hour the grade uses this frame.
    pub fn time_of_day(&self) -> f32 {
        self.day_night.hour()
    }

    /// `true` while the renderer follows an injected simulation hour.
    pub fn day_night_synced(&self) -> bool {
        self.day_night.synced_hour().is_some()
    }

    /// Render the ungraded reference frame (flat white light) at the same hour.
    ///
    /// Used by `render.color.no-yellow-cast` and the evidence captures: the
    /// same camera and materials, only the DNC grade removed, so the per-channel
    /// difference *is* the grade's cast.
    pub fn set_identity_light(&mut self, identity: bool) {
        self.identity_light = identity;
    }

    /// `true` while the ungraded reference light is selected.
    pub fn identity_light(&self) -> bool {
        self.identity_light
    }

    /// Pin the clock hour (0..24) for headless captures and A/B runs.
    ///
    /// This is [`Self::sync_day_night`] under its older name; both stop the
    /// real-time advance and are released by [`Self::resume_day_night_clock`].
    pub fn set_time_of_day(&mut self, hours: f32) {
        self.sync_day_night(hours);
    }

    /// Resume the real-time day/night clock from the pinned hour.
    pub fn resume_day_night_clock(&mut self) {
        self.release_day_night();
    }

    /// Replace the DNC preset (tileset) and/or the map fog override.
    pub fn set_day_night_preset(&mut self, preset: wc3_render::daynight::Preset) {
        self.daynight.set_preset(preset);
    }

    pub fn set_day_night_fog(&mut self, fog: wc3_render::daynight::FogSettings) {
        self.daynight.set_fog(fog);
    }

    /// Apply a `war3map.w3i` lighting override (tileset + fog).
    pub fn apply_map_lighting(&mut self, lighting: &wc3_render::daynight::MapLighting) {
        let mut curve = wc3_render::daynight::DayNightCurve::from_map(lighting);
        curve.set_atmosphere(wc3_render::daynight::atmosphere_enabled());
        self.daynight = curve;
    }

    /// Override how long a full 24 h day takes in real seconds
    /// (`Units/MiscData.txt:63` `DayLength=480`).
    pub fn set_day_length_seconds(&mut self, seconds: f32) {
        let clock = self.day_night.clock().with_seconds_per_day(seconds);
        *self.day_night.clock_mut() = clock;
    }

    /// Jass `SetTimeOfDayScale` (`1.0` = normal).
    pub fn set_time_of_day_scale(&mut self, scale: f32) {
        self.day_night.clock_mut().set_scale(scale);
    }

    /// Rebuild the fog-of-war mask from the simulation's per-player grid.
    /// `cells`: 0 unexplored (black), 1 explored-not-visible (veil alpha 0.834
    /// -> 45 % of the lit pixel on screen), 2 visible (no overlay). The renderer
    /// samples it through the depth buffer, so the mask follows terrain height,
    /// covers units and doodads standing on the cell, and stays under the HUD.
    pub fn set_fog(&mut self, sim: &wc3_sim::Sim, player: u8) {
        self.fog_mask = Some(fog_mask_from_sim(sim, player));
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        scene: &Scene,
    ) {
        self.render_with(device, queue, view, scene, RenderOptions::default());
    }

    pub fn render_with(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        scene: &Scene,
        options: RenderOptions,
    ) {
        // Sample the day/night curve at the authoritative hour. When the game
        // loop called `sync_day_night`, this is exactly the simulation hour and
        // the clock is not advanced; otherwise the free-running clock moves
        // with real frame time (clamped so a paused frame is not replayed).
        let dt = self.last_frame.elapsed().as_secs_f32().min(0.25);
        self.last_frame = std::time::Instant::now();
        let cycle = wc3_render::daynight::enabled();
        if cycle {
            self.day_night.advance_seconds(dt);
        }
        let hour = if !cycle {
            // `H2_DAYNIGHT=off`: A/B against the old fixed noon grade.
            12.0
        } else {
            // `H2_DAYNIGHT=identity` keeps the hour but uses the ungraded
            // reference sample below (flat white light, no DNC tint).
            self.day_night.hour()
        };
        let sample = if cycle && self.identity_light {
            // Ungraded reference: same fog and sun direction, flat white light.
            self.daynight.sample(hour).neutral_light()
        } else {
            self.daynight.sample(hour)
        };
        let day = sample.terrain.grade;
        // Terrain/water read the game camera; models read the mesh renderer's
        // own uniform. WC3 has two DNC models (`SetDayNightModels`), so the unit
        // ambient really is the unit DNC (0.3), not the terrain one (0.2). Both
        // sides now use the same split `ambient + sun · N·L` model — the old
        // flat `grade` multiply tinted every unit pixel with the full sun.
        self.mesh
            .set_surface_light(sample.unit.sun_uniform(), sample.unit.ambient_uniform(), sample.unit.grade);
        self.mesh.set_light_direction(sample.sun_direction);
        // Atmospheric fog: the DNC sky/fog colour, the density in force and the
        // distance ramp. Terrain and models read the same values, so the far
        // hill and the unit standing on it haze together.
        self.mesh.set_atmosphere_fog(
            sample.fog_color,
            sample.fog_density,
            sample.fog_range[0],
            sample.fog_range[1],
        );
        let camera = CameraUniform {
            view_proj: scene.view_proj.to_cols_array_2d(),
            light: [
                sample.sun_direction[0],
                sample.sun_direction[1],
                sample.sun_direction[2],
                0.0,
            ],
            sun: sample.terrain.sun_uniform(),
            ambient: sample.terrain.ambient_uniform(),
            day,
            fog: [
                sample.fog_color[0],
                sample.fog_color[1],
                sample.fog_color[2],
                sample.fog_density,
            ],
            fog_range: [
                sample.fog_range[0],
                sample.fog_range[1],
                0.0,
                0.0,
            ],
            eye: [scene.eye.x, scene.eye.y, scene.eye.z, 1.0],
        };
        if hour as i32 != self.logged_hour {
            self.logged_hour = hour as i32;
            crate::dev_log!(
                "{}",
                wc3_render::daynight::game_time_line(self.day_night.day(), hour)
            );
            crate::dev_log!("{}", wc3_render::daynight::game_light_line(hour));
        }
        // Fog/sky colour comes from the same curve; the fog-of-war pass runs
        // after this pass and multiplies its own black veil, so the night
        // grade can never brighten unexplored ground (see wc3_render::daynight).
        let fog = sample.fog_color;
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera));
        self.terrain
            .update_time(queue, self.clock.elapsed().as_secs_f32());

        self.counters = Counters::default();
        self.fill_gpu_scene(scene);
        let mask = PassMask {
            terrain: !options.skip_terrain,
            units: !options.skip_units,
            ui: !options.skip_ui,
        };
        self.mesh.prepare(
            device,
            queue,
            &self.gpu_scene,
            mask,
            &mut self.counters,
        );

        if !self.skin_logged {
            self.skin_logged = true;
            let info = self.mesh.skin_info();
            let effective_mode = match self.mesh.skin_mode() {
                wc3_render::mesh::SkinMode::Gpu => "gpu",
                wc3_render::mesh::SkinMode::Cpu => "cpu",
            };
            crate::dev_log!(
                "GAME_SKIN mode={} vstride={} bones={} weights={} binding={} palette_buffer={}B usage={:?} palette_len={}B draws={} draw_stride={} draw_buffer={}B draw_grows={} align={} storage_buffers={}",
                effective_mode,
                info.vertex_stride,
                info.bones_format,
                info.weights_format,
                info.palette_binding,
                info.palette_buffer_size,
                info.palette_usage,
                info.palette_bytes,
                info.draw_count,
                info.draw_stride,
                info.draw_buffer_size,
                info.draw_grows,
                info.min_dynamic_offset_alignment,
                info.max_storage_buffers
            );
            // One line per distinct kind: palette range, first vertex's bone
            // and weight, and the exact storage array index the shader reads.
            let mut seen = std::collections::BTreeSet::new();
            for probe in self.mesh.skin_probes() {
                let kind = self
                    .model_kinds
                    .get(probe.model as usize)
                    .map(String::as_str)
                    .unwrap_or("?");
                if !seen.insert(kind.to_string()) {
                    continue;
                }
                crate::dev_log!(
                    "GAME_SKIN_UNIT kind={} model={} nodes={} palette_base={} palette_offset={} palette_bytes={} draw_offset={} bone0={} max_bone={} weight0={:.3} shader_index0={}",
                    kind,
                    probe.model,
                    probe.nodes,
                    probe.palette_base,
                    probe.palette_offset,
                    probe.palette_bytes,
                    probe.draw_offset,
                    probe.first_bone,
                    probe.max_bone,
                    probe.first_weight,
                    probe.shader_index
                );
            }
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("game-encoder"),
        });
        // Scope the whole encode: a validation error the pass scopes miss is
        // still captured here instead of reaching wgpu's aborting default handler.
        note_pass(0);
        let encode_scope = PassScope::new(device, PASS_ENCODE);
        {
            note_pass(1);
            let _scope = PassScope::new(device, PASS_WORLD);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("game-world-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: fog[0] as f64,
                            g: fog[1] as f64,
                            b: fog[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: self.depth.as_ref().map(|depth| {
                    wgpu::RenderPassDepthStencilAttachment {
                        view: depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // Terrain keeps the game's camera bind group (group 0); the batched
            // renderer binds its own camera per group, so both can share a pass.
            pass.set_bind_group(0, &self.camera_bind, &[]);

            if !options.skip_terrain {
                self.terrain.draw_terrain(&mut pass);
                self.terrain.draw_cliffs(&mut pass);
                if let Some(doodads) = &self.doodads {
                    if !options.skip_doodads {
                        self.mesh.draw_static(&mut pass, doodads, 0, &mut self.counters);
                    }
                }
            }

            self.mesh.draw_group(&mut pass, 0, &mut self.counters);

            if !options.skip_terrain {
                // The batched renderer bound its own camera for the unit group;
                // terrain keeps the game camera, so restore it before water.
                pass.set_bind_group(0, &self.camera_bind, &[]);
                self.terrain.draw_water(&mut pass);
                self.terrain.draw_shore(&mut pass);
                if let Some(doodads) = &self.doodads {
                    if !options.skip_doodads {
                        self.mesh.draw_static(&mut pass, doodads, 1, &mut self.counters);
                    }
                }
            }

            self.mesh.draw_group(&mut pass, 1, &mut self.counters);

            if !options.skip_terrain && !options.skip_doodads {
                if let Some(doodads) = &self.doodads {
                    self.mesh.draw_static(&mut pass, doodads, 2, &mut self.counters);
                }
            }

            self.mesh.draw_group(&mut pass, 2, &mut self.counters);
        }
        // Fog-of-war veil. Draw order: world pass (graded + distance-hazed)
        // -> this veil -> HUD. The veil comes *after* the haze so unexplored and
        // explored ground is multiplied down from the final world colour; a veil
        // before the haze would let the haze lift black fog back to grey. The
        // HUD is a later pass and stays unshaded (WC3 keeps the interface lit).
        // `H2_FOW=off` skips the veil; the minimap keeps reading the same sim
        // grid either way.
        if wc3_render::fog::enabled() {
            if let (Some(mask), Some(depth)) = (self.fog_mask.as_ref(), self.depth.as_ref()) {
                // `GAME_FOW unexplored=… explored=… visible=…` once per distinct
                // histogram: device evidence that the three states exist.
                let histogram = mask.histogram();
                if self.logged_fow != Some(histogram) {
                    self.logged_fow = Some(histogram);
                    crate::dev_log!(
                        "GAME_FOW unexplored={} explored={} visible={} cells={} cell_size={:.0}",
                        histogram.0,
                        histogram.1,
                        histogram.2,
                        mask.width * mask.height,
                        mask.cell_size
                    );
                }
                self.fog.prepare(
                    device,
                    queue,
                    mask,
                    scene.view_proj.inverse(),
                    scene.viewport,
                    depth,
                );
                note_pass(2);
                let _scope = PassScope::new(device, PASS_FOW);
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("game-fog-pass"),
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
                self.fog.draw(&mut pass);
            }
        }
        if !options.skip_ui
            && self
                .ui
                .prepare(device, queue, &self.gpu_scene, &mut self.counters)
        {
            note_pass(3);
            let _scope = PassScope::new(device, PASS_HUD);
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
            let textures = self.mesh.textures();
            self.ui.draw(&mut pass, textures, &mut self.counters);
        }
        queue.submit(Some(encoder.finish()));
        // Report anything the encoder raised at finish/submit under `encode`;
        // `last=` keeps the name of the last recorded pass.
        drop(encode_scope);
    }

    /// Refill the reusable GPU scene from the game scene.
    ///
    /// Every vector is cleared and refilled (capacity is kept), so a steady frame
    /// performs no allocation for the renderer's scene view — the game's own
    /// `Scene` construction is a separate, game-layer cost.
    fn fill_gpu_scene(&mut self, scene: &Scene) {
        let gpu = &mut self.gpu_scene;
        gpu.view_proj = scene.view_proj;
        gpu.eye = scene.eye;
        gpu.viewport = scene.viewport;

        if gpu.units.len() < scene.units.len() {
            gpu.units
                .resize_with(scene.units.len(), GpuUnitDraw::default);
        }
        for (slot, unit) in gpu.units.iter_mut().zip(&scene.units) {
            slot.model = unit.model;
            slot.world = unit.world;
            slot.tint = unit.tint;
            slot.nodes.clear();
            slot.nodes.extend_from_slice(&unit.nodes);
            slot.mesh_alpha.clear();
            slot.mesh_alpha.extend_from_slice(&unit.mesh_alpha);
            slot.layer_alpha
                .resize_with(unit.layer_alpha.len(), Vec::new);
            for (dst, src) in slot.layer_alpha.iter_mut().zip(&unit.layer_alpha) {
                dst.clear();
                dst.extend_from_slice(src);
            }
        }
        gpu.units.truncate(scene.units.len());

        if gpu.rings.len() < scene.rings.len() {
            gpu.rings.resize_with(scene.rings.len(), || GpuRing {
                pos: Vec3::ZERO,
                radius: 0.0,
                color: [0.0; 4],
            });
        }
        for (slot, ring) in gpu.rings.iter_mut().zip(&scene.rings) {
            slot.pos = ring.pos;
            slot.radius = ring.radius;
            slot.color = ring.color;
        }
        gpu.rings.truncate(scene.rings.len());

        let ui = &mut gpu.ui;
        ui.white.clear();
        ui.white.extend(scene.ui.white.iter().map(gpu_ui_vertex));
        ui.font.clear();
        ui.font.extend(scene.ui.font.iter().map(gpu_ui_vertex));
        ui.minimap.clear();
        ui.minimap.extend(scene.ui.minimap.iter().map(gpu_ui_vertex));
        ui.textured.resize_with(scene.ui.textured.len(), GpuUiDraw::default);
        for (dst, src) in ui.textured.iter_mut().zip(&scene.ui.textured) {
            dst.texture = src.texture;
            dst.vertices.clear();
            dst.vertices.extend(src.vertices.iter().map(gpu_ui_vertex));
        }
        ui.textured.truncate(scene.ui.textured.len());
    }
}

// ------------------------------------------------------------------ conversion

/// Screen-space vertex: the game and the renderer use the same layout, so the
/// conversion is a field-wise copy (no reinterpret cast).
fn gpu_ui_vertex(vertex: &crate::hud::UiVertex) -> GpuUiVertex {
    GpuUiVertex {
        pos: vertex.pos,
        uv: vertex.uv,
        color: vertex.color,
    }
}

fn gpu_texture(image: &TextureImage) -> GpuTexture {
    GpuTexture::new(image.width, image.height, image.rgba.clone())
}

/// Convert the game's MDX geosets into the renderer's model table.
///
/// The renderer owns one static vertex stream per model plus the bone/weight
/// pairs; the game keeps the authored data untouched. Meshes arrive in the
/// compact runtime format, so this is where the vertices are expanded back to
/// f32 — once, for the GPU upload.
fn gpu_models(assets: &Assets) -> Vec<Model> {
    assets
        .models
        .iter()
        .map(|model| model.meshes.iter().map(gpu_mesh).collect())
        .collect()
}

/// Convert one asset mesh into the renderer's upload record.
///
/// The compact mesh owns the skin data and `expanded()` is the single accessor
/// this upload and the runtime skinning read, so the GPU list can never diverge
/// from `CompactMesh::bones` (ledger 451: the second accessor dropped the
/// heaviest influences of multi-matrix vertices).
fn gpu_mesh(mesh: &crate::assets::MeshData) -> GpuMeshData {
    let expanded = mesh.expanded();
    GpuMeshData {
        positions: expanded.positions,
        normals: expanded.normals,
        uvs: expanded.uvs,
        // The asset layer carries TEXCOORD0 only; an empty second
        // set makes the renderer reuse `uvs` (documented contract).
        uvs2: expanded.uvs2,
        indices: expanded.indices,
        bones: expanded
            .bones
            .into_iter()
            .map(|list| {
                list.into_iter()
                    .map(|pair| GpuBoneWeight {
                        bone: pair.bone,
                        weight: pair.weight,
                    })
                    .collect()
            })
            .collect(),
        layers: mesh
            .layers
            .iter()
            .map(|layer| GpuMeshLayer {
                texture: layer.texture,
                alpha: layer.alpha,
                blend: layer.blend,
                team_color: layer.team_color,
                uv_set: layer.uv_set,
            })
            .collect(),
        z_up: mesh.z_up,
        hide: mesh.hide,
    }
}

/// Merge all doodad instances into one static batch: one draw per
/// (model, mesh, material layer) plus one shadow decal for the whole map.
///
/// The whole forest then costs a few hundred draws and no per-frame CPU work.
fn build_doodad_geometry(
    doodads: &DoodadWorld,
    texture_base: usize,
) -> (Vec<MeshVertex>, Vec<u32>, Vec<StaticDrawSpec>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut specs = Vec::new();
    for (model_index, model) in doodads.models.iter().enumerate() {
        for (mesh_index, mesh) in model.meshes.iter().enumerate() {
            let alpha = model.alphas.get(mesh_index).copied().unwrap_or(1.0);
            if alpha <= 0.01 || mesh.hide || mesh.indices.is_empty() {
                continue;
            }
            for layer in &mesh.layers {
                let layer_alpha = layer.alpha * alpha;
                if layer_alpha <= 0.01 {
                    continue;
                }
                let index_start = indices.len() as u32;
                let mut emitted = false;
                for instance in doodads
                    .instances
                    .iter()
                    .filter(|instance| instance.model == model_index)
                {
                    let base_vertex = vertices.len() as u32;
                    for (index, position) in mesh.positions.iter().enumerate() {
                        let authored_normal =
                            mesh.normals.get(index).copied().unwrap_or(Vec3::Y);
                        let (local, normal) = if mesh.z_up {
                            (
                                Vec3::new(position.x, position.z, -position.y),
                                Vec3::new(
                                    authored_normal.x,
                                    authored_normal.z,
                                    -authored_normal.y,
                                ),
                            )
                        } else {
                            (*position, authored_normal)
                        };
                        let world = instance.world.transform_point3(local);
                        let normal = instance.world.transform_vector3(normal).normalize_or_zero();
                        let color = if layer.team_color {
                            [0.55, 0.72, 1.0, layer_alpha]
                        } else {
                            [1.0, 1.0, 1.0, layer_alpha]
                        };
                        // Two texture units: a layer with `uv_set == 1`
                        // samples TEXCOORD1 (`TEXCOORDINDEX := unit`, r2 lane
                        // 405). Doodad batches duplicate the vertices per
                        // layer, so the set is baked here and the draw keeps
                        // the default TEXCOORD0 select.
                        let uv = match layer.uv_set {
                            1 => mesh.uvs2.get(index).copied(),
                            _ => None,
                        }
                        .or_else(|| mesh.uvs.get(index).copied())
                        .unwrap_or(Vec2::ZERO);
                        vertices.push(MeshVertex::colored(
                            world.to_array(),
                            if normal.length_squared() > 0.0 {
                                normal
                            } else {
                                Vec3::Y
                            },
                            uv.to_array(),
                            color,
                        ));
                    }
                    for &index in &mesh.indices {
                        indices.push(base_vertex + index);
                    }
                    emitted = true;
                }
                if emitted {
                    specs.push(StaticDrawSpec {
                        index_start,
                        index_count: indices.len() as u32 - index_start,
                        texture: layer.texture + texture_base,
                        blend: layer.blend,
                        group: if layer.blend >= 2 { 1 } else { 0 },
                        tint: [1.0, 1.0, 1.0, 1.0],
                        color_scale: 1.0,
                    });
                }
            }
        }
    }

    // Soft blob shadows ground the trees/rocks: a triangle fan with a dark
    // centre and transparent rim, one merged decal draw for the whole map.
    let shadow_index_start = indices.len() as u32;
    let segments = 14u32;
    for instance in &doodads.instances {
        let model = &doodads.models[instance.model];
        let radius = model.footprint * instance.scale * 0.42;
        if radius < 6.0 {
            continue;
        }
        let centre = instance.world.transform_point3(Vec3::ZERO);
        let y = centre.y + 2.0;
        let base = vertices.len() as u32;
        vertices.push(MeshVertex::colored(
            [centre.x, y, centre.z],
            Vec3::Y,
            [0.5, 0.5],
            [0.02, 0.03, 0.04, 0.30],
        ));
        for segment in 0..=segments {
            let angle = segment as f32 / segments as f32 * std::f32::consts::TAU;
            vertices.push(MeshVertex::colored(
                [
                    centre.x + angle.cos() * radius,
                    y,
                    centre.z + angle.sin() * radius,
                ],
                Vec3::Y,
                [0.5, 0.5],
                [0.02, 0.03, 0.04, 0.0],
            ));
        }
        for segment in 0..segments {
            indices.push(base);
            indices.push(base + 1 + segment);
            indices.push(base + 2 + segment);
        }
    }
    if indices.len() as u32 > shadow_index_start {
        specs.push(StaticDrawSpec {
            index_start: shadow_index_start,
            index_count: indices.len() as u32 - shadow_index_start,
            texture: 0,
            blend: 2,
            group: 2,
            tint: [1.0, 1.0, 1.0, 1.0],
            color_scale: 1.0,
        });
    }
    (vertices, indices, specs)
}

/// Build the renderer's fog-of-war mask from the simulation's per-player grid.
///
/// One state per W3E cell: `0` unexplored, `1` explored-but-not-visible,
/// `2` visible. [`Renderer::set_fog`] feeds the 3D veil from this, and the
/// minimap (`Game::minimap_fog_cells`) reads the same `is_visible`/`is_explored`
/// predicates, so the world and the HUD can never disagree about visibility.
pub fn fog_mask_from_sim(sim: &wc3_sim::Sim, player: u8) -> FogMask {
    let width = sim.grid.width;
    let height = sim.grid.height;
    let cell_size = sim.grid.cell_size;
    let mut cells = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let pos = wc3_sim::Vec2::new(
                (x as f32 + 0.5) * cell_size,
                (y as f32 + 0.5) * cell_size,
            );
            cells.push(if sim.is_visible(player, pos) {
                2
            } else if sim.is_explored(player, pos) {
                1
            } else {
                0
            });
        }
    }
    FogMask::new(width, height, cell_size, cells)
}

/// Render one scene into an RGBA8 buffer with a headless Vulkan device.
pub fn render_scene_to_rgba(
    assets: &Assets,
    font_atlas: &TextureImage,
    minimap: Option<&TextureImage>,
    scene: &Scene,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    render_scene_to_rgba_with(
        assets,
        font_atlas,
        minimap,
        scene,
        width,
        height,
        RenderOptions::default(),
    )
}

pub fn render_scene_to_rgba_with(
    assets: &Assets,
    font_atlas: &TextureImage,
    minimap: Option<&TextureImage>,
    scene: &Scene,
    width: u32,
    height: u32,
    options: RenderOptions,
) -> Result<Vec<u8>, String> {
    render_scene_with_skin(
        assets,
        font_atlas,
        minimap,
        scene,
        width,
        height,
        options,
        None,
    )
    .map(|(rgba, _)| rgba)
}

/// Same offscreen frame, with an explicit skinning backend.
///
/// The Android build forces `SkinMode::Cpu` (the Adreno vertex-stage storage
/// palette renders bind pose); every device-only skinning defect therefore has
/// to be reproducible headlessly, which is what this entry point is for.
///
/// `Ok(..).1` is the [`Counters`] snapshot of that frame (`draws` is the number
/// of `draw_indexed` calls the world pass submitted).
pub fn render_scene_with_skin(
    assets: &Assets,
    font_atlas: &TextureImage,
    minimap: Option<&TextureImage>,
    scene: &Scene,
    width: u32,
    height: u32,
    options: RenderOptions,
    skin: Option<wc3_render::mesh::SkinMode>,
) -> Result<(Vec<u8>, Counters), String> {
    render_scene_with_skin_hour(
        assets, font_atlas, minimap, scene, width, height, options, skin, None,
    )
}

/// Render one offscreen frame at a pinned clock hour (0..24).
///
/// The day/night evidence captures use this: same renderer, same assets, only
/// the DNC clock differs. `H2_DAYNIGHT` does not apply here — an explicit hour
/// always wins.
pub fn render_scene_to_rgba_at_hour(
    assets: &Assets,
    font_atlas: &TextureImage,
    minimap: Option<&TextureImage>,
    scene: &Scene,
    width: u32,
    height: u32,
    hour: f32,
) -> Result<Vec<u8>, String> {
    render_scene_to_rgba_with_lighting(
        assets,
        font_atlas,
        minimap,
        scene,
        width,
        height,
        FrameLighting {
            hour: Some(hour),
            ..FrameLighting::default()
        },
    )
}

/// Lighting/fog overrides for one offscreen frame.
///
/// Tests and evidence captures need to pin the hour, switch the default
/// atmosphere off for an A/B, or hand in a fog-of-war mask — all without
/// mutating the process environment (which is shared by the test threads).
#[derive(Clone, Copy, Default)]
pub struct FrameLighting<'a> {
    /// Pinned clock hour; `None` keeps the free-running clock.
    pub hour: Option<f32>,
    /// `Some(false)` renders without the default atmospheric haze.
    pub atmosphere: Option<bool>,
    /// `Some(true)` renders the ungraded reference light (flat white).
    pub identity: Option<bool>,
    /// Fog-of-war veil multiplied over the world pass.
    pub fog: Option<&'a FogMask>,
}

/// Render one offscreen frame at a pinned hour with explicit lighting/fog.
///
/// `tests/lighting_479.rs` uses this for the four evidence hours, the yellow
/// cast before/after pair and the fog-of-war three-state proof; the same
/// renderer, camera and assets are used for every frame so the means are
/// comparable.
#[allow(clippy::too_many_arguments)]
pub fn render_scene_to_rgba_with_lighting(
    assets: &Assets,
    font_atlas: &TextureImage,
    minimap: Option<&TextureImage>,
    scene: &Scene,
    width: u32,
    height: u32,
    lighting: FrameLighting<'_>,
) -> Result<Vec<u8>, String> {
    render_scene_with_skin_lighting(
        assets,
        font_atlas,
        minimap,
        scene,
        width,
        height,
        RenderOptions::default(),
        None,
        lighting,
    )
    .map(|(rgba, _)| rgba)
}

#[allow(clippy::too_many_arguments)]
fn render_scene_with_skin_hour(
    assets: &Assets,
    font_atlas: &TextureImage,
    minimap: Option<&TextureImage>,
    scene: &Scene,
    width: u32,
    height: u32,
    options: RenderOptions,
    skin: Option<wc3_render::mesh::SkinMode>,
    hour: Option<f32>,
) -> Result<(Vec<u8>, Counters), String> {
    render_scene_with_skin_lighting(
        assets,
        font_atlas,
        minimap,
        scene,
        width,
        height,
        options,
        skin,
        FrameLighting {
            hour,
            ..FrameLighting::default()
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn render_scene_with_skin_lighting(
    assets: &Assets,
    font_atlas: &TextureImage,
    minimap: Option<&TextureImage>,
    scene: &Scene,
    width: u32,
    height: u32,
    options: RenderOptions,
    skin: Option<wc3_render::mesh::SkinMode>,
    lighting: FrameLighting<'_>,
) -> Result<(Vec<u8>, Counters), String> {
    let headless = HeadlessTarget::new(width, height)?;
    let mut renderer = Renderer::new(
        &headless.device,
        &headless.queue,
        headless.format,
        assets,
        font_atlas,
        minimap,
    )?;
    if let Some(mode) = skin {
        renderer.set_skin_mode(mode);
    }
    if let Some(hour) = lighting.hour {
        renderer.set_time_of_day(hour);
    }
    if let Some(atmosphere) = lighting.atmosphere {
        renderer.daynight.set_atmosphere(atmosphere);
    }
    if let Some(identity) = lighting.identity {
        renderer.set_identity_light(identity);
    }
    if let Some(fog) = lighting.fog {
        renderer.fog_mask = Some(fog.clone());
    }
    renderer.resize(&headless.device, headless.width, headless.height);
    headless.render(&mut renderer, scene, options);
    let rgba = headless.read()?;
    Ok((rgba, renderer.counters()))
}

/// Offscreen colour target with its own device.
///
/// Shared by the headless PNG tests, the asset gallery and the memory probes:
/// one object so a caller can render several frames with the *same* renderer.
/// Building a renderer per frame re-uploads every texture, which would hide the
/// state a release/eviction test is trying to observe.
pub struct HeadlessTarget {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,
    pub target: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

impl HeadlessTarget {
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        let width = width.max(1);
        let height = height.max(1);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            // No indirect draws in this renderer. wgpu's default release flags
            // still build a validation compute pipeline at device creation,
            // which Turnip 26.3 fails to compile (ralloc corruption in
            // ir3_optimize_loop).
            flags: wgpu::InstanceFlags::empty(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            }))
            .map_err(|error| format!("request_adapter: {error}"))?;
        // Device evidence: the pipeline runs on the real Vulkan adapter (Adreno
        // on the phone, lavapipe on CI). Logged once per offscreen renderer so
        // gallery and projectile artifacts carry their GPU provenance.
        {
            let info = adapter.get_info();
            crate::dev_log!(
                "RENDER_ADAPTER name={:?} backend={:?} driver={:?} device_type={:?}",
                info.name,
                info.backend,
                info.driver,
                info.device_type
            );
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("wc3-game-headless-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .map_err(|error| format!("request_device: {error}"))?;
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("wc3-game-headless-target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(Self {
            device,
            queue,
            format,
            target,
            view,
            width,
            height,
        })
    }

    /// Draw one frame into the offscreen target.
    pub fn render(&self, renderer: &mut Renderer, scene: &Scene, options: RenderOptions) {
        renderer.render_with(&self.device, &self.queue, &self.view, scene, options);
    }

    /// Copy the target back into a tightly packed RGBA buffer.
    pub fn read(&self) -> Result<Vec<u8>, String> {
        let (width, height) = (self.width, self.height);
        let bytes_per_row = width * 4;
        let padded_row = bytes_per_row.div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wc3-game-headless-readback"),
            size: (padded_row as u64) * height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wc3-game-headless-copy"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.target,
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
        self.queue.submit(Some(encoder.finish()));
        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel::<Result<(), wgpu::BufferAsyncError>>();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|error| format!("poll: {error}"))?;
        receiver
            .recv()
            .map_err(|error| format!("map channel: {error}"))?
            .map_err(|error| format!("map: {error}"))?;
        let mapped = slice
            .get_mapped_range()
            .map_err(|error| format!("mapped range: {error}"))?;
        let mut out = Vec::with_capacity((bytes_per_row * height) as usize);
        for row in 0..height {
            let start = (row * padded_row) as usize;
            out.extend_from_slice(&mapped[start..start + bytes_per_row as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(out)
    }
}

/// RGBA-only wrapper around [`render_scene_with_skin`] for call sites that do
/// not need the frame counters.
pub fn render_scene_to_rgba_with_skin(
    assets: &Assets,
    font_atlas: &TextureImage,
    minimap: Option<&TextureImage>,
    scene: &Scene,
    width: u32,
    height: u32,
    options: RenderOptions,
    skin: Option<wc3_render::mesh::SkinMode>,
) -> Result<Vec<u8>, String> {
    render_scene_with_skin(assets, font_atlas, minimap, scene, width, height, options, skin)
        .map(|(rgba, _)| rgba)
}

/// Write an RGBA buffer as a binary PPM (readable by every image viewer and by
/// the repo's analysis scripts without an image crate dependency).
pub fn write_ppm(path: &str, width: u32, height: u32, rgba: &[u8]) {
    let mut out = format!("P6\n{width} {height}\n255\n").into_bytes();
    for pixel in rgba.chunks_exact(4) {
        out.extend_from_slice(&pixel[..3]);
    }
    let _ = std::fs::write(path, out);
}

/// Write an RGBA buffer as a PNG artifact (operator/QA reports are consumed as
/// PNG, and the sharpness gate's default frame is a PNG).
pub fn write_png(path: &str, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|error| format!("create {path}: {error}"))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width.max(1), height.max(1));
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|error| format!("png header {path}: {error}"))?;
    writer
        .write_image_data(rgba)
        .map_err(|error| format!("png data {path}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `render.uniform.camera-size` (game half): the camera the game binds at
    /// `@group(0) @binding(0)`, the mesh WGSL `Camera` and the terrain WGSL
    /// `Camera` that actually consumes it must all be 176 B.
    ///
    /// Build 0f037ea2 shipped the shaders at 176 B while this struct was still
    /// the old 112 B shape (`mat4x4` + `light`/`sun`/`ambient`) and aborted on
    /// the first world frame: `In bind group index 0, the buffer bound at
    /// binding index 0 is bound with size 112 where the shader expects 176`
    /// (`analysis/crash-20261117.txt`). The terrain copy used to be checked only
    /// through `mesh::CAMERA_UNIFORM_SIZE`, a proxy in another crate.
    #[test]
    fn camera_uniform_matches_both_shaders() {
        assert_eq!(CAMERA_UNIFORM_BYTES, 176);
        assert_eq!(
            CAMERA_UNIFORM_BYTES,
            wc3_render::mesh::camera_uniform_size_from_wgsl()
        );
        assert_eq!(
            CAMERA_UNIFORM_BYTES,
            wc3_render::mesh::wgsl_struct_size(crate::terrain_render::TERRAIN_SHADER, "Camera")
        );
    }

    /// The instrumentation must be able to pop the scopes it pushes on the
    /// render thread: wgpu's error scopes are thread-local and LIFO, so a leaked
    /// or mis-ordered pop panics inside `drop`, not at the call site.
    #[test]
    fn pass_labels_are_stable() {
        note_pass(1);
        assert_eq!(last_pass(), PASS_WORLD);
        note_pass(2);
        assert_eq!(last_pass(), PASS_FOW);
        note_pass(3);
        assert_eq!(last_pass(), PASS_HUD);
        note_pass(usize::MAX);
        assert_eq!(last_pass(), PASS_HUD, "out-of-range index is clamped");
        note_pass(0);
        assert_eq!(last_pass(), PASS_ENCODE);
    }

    fn fixtures() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
        let mpq = [
            "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
            "/data/data/org.herofolends.p0/files/War3.mpq",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|path| path.exists())?;
        let map = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Maps/Campaign/Human04.w3m");
        map.exists().then_some((mpq, map))
    }

    /// `render.skinning.upload-contract`: the GPU upload consumes exactly the
    /// compact influence list (`CompactMesh::bones`) — same pairs, same
    /// weights, at most four — so the upload path and the runtime skinning
    /// share one contract.
    #[test]
    fn upload_consumes_the_compact_influence_list() {
        let mut mesh = crate::assets::MeshData::default();
        mesh.positions = vec![Vec3::ZERO, Vec3::X, Vec3::Y];
        mesh.normals = vec![Vec3::Y; 3];
        mesh.uvs = vec![Vec2::ZERO; 3];
        mesh.indices = vec![0, 1, 2];
        mesh.bones = vec![
            vec![
                GpuBoneWeight { bone: 7, weight: 0.6 },
                GpuBoneWeight { bone: 11, weight: 0.3 },
                GpuBoneWeight { bone: 2, weight: 0.1 },
            ],
            vec![GpuBoneWeight { bone: 4, weight: 1.0 }],
            vec![
                GpuBoneWeight { bone: 9, weight: 0.4 },
                GpuBoneWeight { bone: 3, weight: 0.4 },
                GpuBoneWeight { bone: 5, weight: 0.2 },
            ],
        ];
        mesh.compact();
        let compact = mesh.compact.as_ref().expect("compact mesh");
        let gpu = gpu_mesh(&mesh);
        assert_eq!(gpu.bones.len(), compact.vertex_count as usize);
        for vertex in 0..gpu.bones.len() {
            let expected = compact.bones(vertex);
            assert_eq!(gpu.bones[vertex].len(), expected.len(), "vertex {vertex}");
            assert!(expected.len() <= 4, "vertex {vertex} keeps at most four");
            for (uploaded, runtime) in gpu.bones[vertex].iter().zip(&expected) {
                assert_eq!(uploaded.bone, runtime.bone, "vertex {vertex}");
                assert!(
                    (uploaded.weight - runtime.weight).abs() < 1e-6,
                    "vertex {vertex} weight"
                );
            }
            let sum: f32 = gpu.bones[vertex].iter().map(|pair| pair.weight).sum();
            assert!((sum - 1.0).abs() <= 1e-3, "vertex {vertex} sums to {sum}");
        }
        // The known authored pair survives the round trip with its index.
        assert_eq!(gpu.bones[0][0].bone, 7);
        assert!((gpu.bones[0][0].weight - 0.6).abs() < 5e-3);
    }

    /// `memory.instancing`: 100 units on screen share one mesh copy.
    ///
    /// `UnitDraw` carries a model *index*, and `Assets::models` holds one entry
    /// per roster definition, so drawing 100 footmen must not allocate 100 mesh
    /// copies. The check is the asset byte count before/after the spawn: a
    /// per-unit clone would multiply it.
    #[test]
    fn hundred_units_share_one_mesh_copy() {
        let Some((mpq, map)) = fixtures() else {
            crate::dev_log!("SKIP memory.instancing: fixtures missing");
            return;
        };
        let assets = Assets::load(Some(&mpq), Some(&map));
        let mut game = crate::game::Game::new(assets, 0x5EED);
        game.resize(960.0, 540.0);
        for _ in 0..10 {
            game.update(1.0 / 30.0);
        }
        let mesh_before = game.assets.mesh_bytes();
        let record_before = game.assets.vertex_record_bytes();
        let models_before = game.assets.models.len();
        // A hundred soldiers of one definition, spread over the start area.
        let base = game.base_pos;
        for index in 0..100 {
            let x = base.x + (index % 10) as f32 * 48.0 - 240.0;
            let z = base.y + (index / 10) as f32 * 48.0 - 240.0;
            game.sim.spawn(
                game.player,
                wc3_sim::UnitKind::Footman,
                glam::Vec2::new(x, z),
            );
        }
        for _ in 0..10 {
            game.update(1.0 / 30.0);
        }
        let scene = game.scene();
        let footman_model = game.assets.model_index_for_def("hfoo");
        let drawn = scene
            .units
            .iter()
            .filter(|unit| unit.model == footman_model)
            .count();
        assert!(
            drawn >= 100,
            "the frame must draw the hundred spawned footmen (drawn={drawn})"
        );
        assert_eq!(
            game.assets.models.len(),
            models_before,
            "spawning units adds no model entries"
        );
        assert_eq!(
            game.assets.mesh_bytes(),
            mesh_before,
            "spawning units adds no mesh bytes"
        );
        assert_eq!(
            game.assets.vertex_record_bytes(),
            record_before,
            "the vertex format does not change per instance"
        );
        println!(
            "MEM_INSTANCING units={} footmen={} mesh_mb={:.1} models={}",
            scene.units.len(),
            drawn,
            mesh_before as f64 / (1024.0 * 1024.0),
            game.assets.models.len()
        );
    }

    /// `memory.textures`: the asset layer drops every decoded sheet once the
    /// renderer owns the GPU copies, and the same frame still renders.
    ///
    /// This is the memory budget's step 1 (`analysis/h2/MEMORY_BUDGET.md`): the
    /// CPU RGBA is dead weight after upload (208.9 MB on the shipped install),
    /// so the only correct test is a frame comparison — the released texture
    /// list must not change a single drawn pixel.
    #[test]
    fn releasing_cpu_textures_keeps_the_frame_identical() {
        let Some((mpq, map)) = fixtures() else {
            crate::dev_log!("SKIP memory.textures: fixtures missing");
            return;
        };
        let assets = Assets::load(Some(&mpq), Some(&map));
        let font = crate::hud::Font::load();
        let mut game = crate::game::Game::new(assets, 0x5EED);
        game.resize(480.0, 270.0);
        for _ in 0..10 {
            game.update(1.0 / 30.0);
        }
        let scene = game.scene();
        let headless = match HeadlessTarget::new(480, 270) {
            Ok(target) => target,
            Err(error) => {
                crate::dev_log!("SKIP memory.textures: no GPU ({error})");
                return;
            }
        };
        let mut renderer = match Renderer::new(
            &headless.device,
            &headless.queue,
            headless.format,
            &game.assets,
            &font.atlas,
            game.assets.minimap.as_ref(),
        ) {
            Ok(renderer) => renderer,
            Err(error) => {
                crate::dev_log!("SKIP memory.textures: renderer ({error})");
                return;
            }
        };
        renderer.resize(&headless.device, headless.width, headless.height);
        headless.render(&mut renderer, &scene, RenderOptions::default());
        let before = headless.read().expect("readback before release");
        let cpu_before = game.assets.cpu_texture_bytes();
        assert!(
            cpu_before > 0,
            "the shipped install must load decoded textures"
        );

        let released = game.assets.release_cpu_textures();
        assert_eq!(
            game.assets.cpu_texture_bytes(),
            0,
            "every decoded sheet is dropped"
        );
        assert!(
            released.surfaces > 0 && released.bytes == cpu_before,
            "release accounting: surfaces={} bytes={} cpu_before={}",
            released.surfaces,
            released.bytes,
            cpu_before
        );

        // Same renderer, same scene: the GPU textures are already resident, so
        // the frame must be byte-identical.
        headless.render(&mut renderer, &scene, RenderOptions::default());
        let after = headless.read().expect("readback after release");
        assert_eq!(before.len(), after.len(), "same frame size");
        let differing = before.iter().zip(&after).filter(|(a, b)| a != b).count();
        assert_eq!(
            differing, 0,
            "released CPU textures changed {} of {} bytes",
            differing,
            before.len()
        );
        println!(
            "MEM_RELEASE surfaces={} freed_mb={:.1} cache_mb={:.1} pixels_identical={} draws={}",
            released.surfaces,
            released.bytes as f64 / (1024.0 * 1024.0),
            released.cache_bytes as f64 / (1024.0 * 1024.0),
            differing == 0,
            renderer.counters().draws
        );
    }

    /// `memory.textures`: the release is idempotent and never resurrects data.
    #[test]
    fn releasing_cpu_textures_twice_is_a_noop() {
        let mut assets = Assets::load(None, None);
        let first = assets.release_cpu_textures();
        let second = assets.release_cpu_textures();
        assert!(first.bytes > 0, "the fallback base still owns CPU art");
        assert_eq!(second.bytes, 0, "nothing left to free");
        assert_eq!(second.surfaces, 0, "second release frees no surface");
        assert_eq!(assets.cpu_texture_bytes(), 0);
        assert_eq!(second.cache_bytes, 0, "no asset db without an archive");
    }

    /// `rendering.native`: native window pixels map 1:1 to the 3D pass and the
    /// HUD scales from the same viewport; safe-area insets keep the command card
    /// inside the usable rect (no overlap with the gesture bars/cutout).
    #[test]
    fn rendering_native_scale_math() {
        let native = crate::hud::HudLayout::new(glam::Vec2::new(2414.0, 1080.0));
        assert!((native.scale - 1.0).abs() < 1e-6, "native scale {}", native.scale);
        let hd = crate::hud::HudLayout::new(glam::Vec2::new(1280.0, 720.0));
        assert!((hd.scale - 720.0 / 1080.0).abs() < 1e-6, "hd scale {}", hd.scale);
        let small = crate::hud::HudLayout::new(glam::Vec2::new(320.0, 240.0));
        assert!((small.scale - 0.62).abs() < 1e-6, "small scale {}", small.scale);
        let large = crate::hud::HudLayout::new(glam::Vec2::new(4320.0, 2160.0));
        assert!((large.scale - 2.0).abs() < 1e-6, "large scale {}", large.scale);
        let portrait = crate::hud::HudLayout::new(glam::Vec2::new(1080.0, 2414.0));
        assert!((portrait.scale - 1.0).abs() < 1e-6, "portrait scale {}", portrait.scale);

        let Some((mpq, map)) = fixtures() else {
            crate::dev_log!("SKIP rendering.native: fixtures missing");
            return;
        };
        let assets = Assets::load(Some(&mpq), Some(&map));
        let mut game = crate::game::Game::new(assets, 0x5EED);
        game.resize(2414.0, 1080.0);
        game.set_safe_insets(210.0, 120.0, 90.0, 96.0);
        assert_eq!(game.viewport, glam::Vec2::new(2414.0, 1080.0));
        let (left, top) = (game.safe_insets.left, game.safe_insets.top);
        let (right, bottom) = (game.safe_insets.right, game.safe_insets.bottom);
        let card = game.layout.command;
        assert!(card.x >= left - 1.0, "card x {} < safe left {left}", card.x);
        assert!(card.y >= top - 1.0, "card y {} < safe top {top}", card.y);
        assert!(
            card.x + card.w <= 2414.0 - right + 1.0,
            "card right {} overlaps safe right",
            card.x + card.w
        );
        assert!(
            card.y + card.h <= 1080.0 - bottom + 1.0,
            "card bottom {} overlaps safe bottom",
            card.y + card.h
        );
    }

    /// `rendering.terrain.mips`: tile mip selection quality with the numeric
    /// wrap-seam metric. Authored BLP levels are reused, the pyramid reaches
    /// 1x1, and every level repeats without a hard seam (no quilt).
    #[test]
    fn rendering_terrain_mips_seam_metric() {
        use wc3_render::terrain::{seam_metric, tile_from_sheet, SheetLevel};
        let mut rgba = vec![0u8; 256 * 256 * 4];
        let mut state = 0x1234_5678u32;
        for pixel in rgba.chunks_exact_mut(4) {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let value = (state >> 24) as u8;
            pixel.copy_from_slice(&[value, value.wrapping_mul(3), value / 2, 255]);
        }
        let sheet = [SheetLevel {
            width: 256,
            height: 256,
            rgba: &rgba,
        }];
        let (levels, _) = tile_from_sheet(&sheet, 128);
        assert_eq!(levels.len(), 8, "128 -> 1 is eight levels");
        for (index, level) in levels.iter().enumerate() {
            assert_eq!(level.width, 128u32 >> index, "mip {index} width");
            assert_eq!(level.height, 128u32 >> index, "mip {index} height");
            let seam = seam_metric(level);
            assert!(seam < 2.0, "mip {index} seam {seam} (quilt)");
        }
        let base_seam = seam_metric(&levels[0]);
        assert!(base_seam < 0.6, "base seam {base_seam} (hard repeat)");
    }

    /// `rendering.water`: Water.slk depth colours and texture drive the surface,
    /// and the shoreline vertex alpha fades in without leaving the 0..1 range.
    #[test]
    fn rendering_water_slk_shoreline_invariants() {
        let Some((mpq, map)) = fixtures() else {
            crate::dev_log!("SKIP rendering.water: fixtures missing");
            return;
        };
        let assets = Assets::load(Some(&mpq), Some(&map));
        let tiles = crate::terrain_render::load_tile_set(&assets);
        let water = &tiles.water;
        assert!(water.texture.is_some(), "Water.slk texFile must resolve");
        let luminance = |color: [f32; 3]| 0.2126 * color[0] + 0.7152 * color[1] + 0.0722 * color[2];
        assert!(
            luminance(water.shallow) > luminance(water.deep),
            "shallow {:?} must be brighter than deep {:?}",
            water.shallow,
            water.deep
        );
        assert!(water.shallow.iter().all(|v| v.is_finite() && *v >= 0.0));
        assert!(water.deep.iter().all(|v| v.is_finite() && *v >= 0.0));
        assert!(water.rate.is_finite() && water.rate >= 0.0);
        let mesh = crate::terrain_render::build_terrain(&assets, &tiles);
        assert!(!mesh.water_vertices.is_empty(), "water surface vertices");
        let mut max_alpha = 0.0f32;
        for vertex in &mesh.water_vertices {
            let alpha = vertex.color[3];
            assert!(alpha.is_finite() && (0.0..=1.0).contains(&alpha), "alpha {alpha}");
            max_alpha = max_alpha.max(alpha);
        }
        assert!(max_alpha > 0.2, "shoreline blend max alpha {max_alpha}");
    }

    /// `rendering.units`: the material path applies the player tint only to
    /// team-colour layers. Two frames of the same scene with different tints
    /// must differ, and each tint must dominate the changed pixels.
    #[test]
    fn rendering_units_material_resolution() {
        let Some((mpq, map)) = fixtures() else {
            crate::dev_log!("SKIP rendering.units: fixtures missing");
            return;
        };
        let assets = Assets::load(Some(&mpq), Some(&map));
        let mut team_layers = 0usize;
        for model in &assets.models {
            for mesh in &model.meshes {
                for layer in &mesh.layers {
                    if layer.team_color {
                        team_layers += 1;
                    }
                }
            }
        }
        assert!(team_layers > 0, "real models must carry team-colour layers");
        let mut game = crate::game::Game::new(assets, 0x5EED);
        game.resize(960.0, 540.0);
        for _ in 0..30 {
            game.update(1.0 / 30.0);
        }
        let mut scene = game.scene();
        assert!(!scene.units.is_empty(), "scene needs units for the tint path");
        // Keep the units that actually show team-coloured geometry in this
        // frame. A unit whose team geoset is alpha-gated off by its Stand
        // animation would make the comparison vacuous: the pixel diff can only
        // be non-zero where a team layer is drawn.
        let visible_team_layers = |unit: &crate::game::UnitDraw, assets: &Assets| {            let Some(model) = assets.models.get(unit.model) else {
                return 0usize;
            };
            let mut count = 0usize;
            for (mesh_index, mesh) in model.meshes.iter().enumerate() {
                let geoset_alpha = unit.mesh_alpha.get(mesh_index).copied().unwrap_or(1.0);
                if geoset_alpha <= 0.001 || mesh.indices.is_empty() {
                    continue;
                }
                for (layer_index, layer) in mesh.layers.iter().enumerate() {
                    if !layer.team_color {
                        continue;
                    }
                    let layer_alpha = unit
                        .layer_alpha
                        .get(mesh_index)
                        .and_then(|alphas| alphas.get(layer_index))
                        .copied()
                        .unwrap_or(layer.alpha);
                    if geoset_alpha * layer_alpha > 0.001 {
                        count += 1;
                    }
                }
            }
            count
        };
        let team_units: Vec<usize> = scene
            .units
            .iter()
            .enumerate()
            .filter(|(_, unit)| visible_team_layers(*unit, &game.assets) > 0)
            .map(|(index, _)| index)
            .collect();
        assert!(
            !team_units.is_empty(),
            "the frame must draw at least one team-coloured layer (units={})",
            scene.units.len()
        );
        let mut keep = vec![false; scene.units.len()];
        for index in &team_units {
            keep[*index] = true;
        }
        println!(
            "TEAM_TINT units_in_frame={} units_with_team_art={}",
            scene.units.len(),
            team_units.len()
        );
        let minimap = game.assets.minimap.clone();
        for unit in &mut scene.units {
            unit.tint = [1.0, 0.15, 0.10, 1.0];
        }
        // Units are measured without terrain and HUD: both would own every
        // pixel and hide the unit pass (this file's `RenderOptions` contract).
        let units_only = RenderOptions {
            skip_terrain: true,
            skip_doodads: true,
            skip_ui: true,
            ..RenderOptions::default()
        };
        // Device-faithful skinning: the Android build forces `SkinMode::Cpu`
        // (see `render_scene_with_skin`'s contract). The GPU palette path needs
        // a filled `UnitDraw::nodes`, and a scene whose animators were not built
        // carries none - that would collapse every unit into the origin and the
        // comparison below would be vacuous for reasons unrelated to tinting.
        let skin = Some(wc3_render::mesh::SkinMode::Cpu);
        let red = render_scene_with_skin(
            &game.assets,
            &game.font.atlas,
            minimap.as_ref(),
            &scene,
            960,
            540,
            units_only,
            skin,
        )
        .expect("headless red-tint render")
        .0;
        for unit in &mut scene.units {
            unit.tint = [0.12, 0.25, 1.0, 1.0];
        }
        let blue = render_scene_with_skin(
            &game.assets,
            &game.font.atlas,
            minimap.as_ref(),
            &scene,
            960,
            540,
            units_only,
            skin,
        )
        .expect("headless blue-tint render")
        .0;
        let mut changed = 0usize;
        let mut red_red = 0i64;
        let mut blue_red = 0i64;
        let mut red_blue = 0i64;
        let mut blue_blue = 0i64;
        for (a, b) in red.chunks_exact(4).zip(blue.chunks_exact(4)) {
            let delta = (a[0] as i32 - b[0] as i32).abs()
                + (a[1] as i32 - b[1] as i32).abs()
                + (a[2] as i32 - b[2] as i32).abs();
            if delta > 6 {
                changed += 1;
                red_red += a[0] as i64;
                blue_red += b[0] as i64;
                red_blue += a[2] as i64;
                blue_blue += b[2] as i64;
            }
        }
        assert!(changed > 50, "tint must change team pixels, got {changed}");
        assert!(red_red > blue_red, "red tint must dominate changed pixels");
        assert!(blue_blue > red_blue, "blue tint must dominate changed pixels");
    }

    /// `rendering.perf`: deterministic frame-budget proof. No wall clock: the
    /// scene accounting is checked against the 60 fps envelope derived from the
    /// measured device runs (`analysis/agent-perf/bench-results/`):
    /// 131 units + HUD + terrain = 556 draws at 14.648 ms; a 16.667 ms budget
    /// allows 556 * 16.667 / 14.648 = 632 draws. The synthetic frame keeps
    /// 147 units x 4 visible layers = 588 draws, inside the envelope.
    #[test]
    fn rendering_perf_deterministic_budget() {
        use glam::Mat4;
        use wc3_render::parity::account;
        let quad = |layers: Vec<GpuMeshLayer>| GpuMeshData {
            positions: vec![
                Vec3::new(-1.0, 0.0, -1.0),
                Vec3::new(1.0, 0.0, -1.0),
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(-1.0, 0.0, 1.0),
            ],
            normals: vec![Vec3::Y; 4],
            uvs: vec![glam::Vec2::ZERO; 4],
            uvs2: Vec::new(),
            indices: vec![0, 1, 2, 0, 2, 3],
            bones: vec![Vec::new(); 4],
            layers,
            z_up: false,
            hide: false,
        };
        let layers = || {
            vec![
                GpuMeshLayer::plain(0),
                GpuMeshLayer {
                    texture: 0,
                    alpha: 1.0,
                    blend: 0,
                    team_color: true,
                    uv_set: 0,
                },
            ]
        };
        let models: Vec<Model> = vec![vec![quad(layers()), quad(layers())]];
        let mut scene = GpuScene::empty(Mat4::IDENTITY, glam::Vec2::new(2414.0, 1080.0));
        for _ in 0..147 {
            scene.units.push(GpuUnitDraw {
                model: 0,
                world: Mat4::IDENTITY,
                tint: [1.0; 4],
                nodes: Vec::new(),
                mesh_alpha: vec![1.0, 1.0],
                layer_alpha: vec![vec![1.0, 1.0], vec![1.0, 1.0]],
                // No GEOA colour animation in this synthetic budget scene;
                // empty is the documented "white" default.
                mesh_color: Vec::new(),
            });
        }
        let budget = account(&scene, &models);
        assert_eq!(budget.units, 147);
        assert_eq!(budget.draws, 147 * 4, "two geosets x two layers per unit");
        let max_draws = (556.0f64 * (1000.0f64 / 60.0) / 14.648f64).floor() as usize;
        assert!(
            budget.draws <= max_draws,
            "draws {} exceed the 60 fps envelope {} (556 draws @14.648ms)",
            budget.draws,
            max_draws
        );
        assert!(budget.vertices <= 300_000, "vertices {}", budget.vertices);
    }

    fn campaign_map(name: &str) -> Option<std::path::PathBuf> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_data/Maps/Campaign")
            .join(name);
        path.exists().then_some(path)
    }

    /// Round 2: doodad material alpha and team colour. Audits every campaign
    /// map in the list for alpha/team layers, then proves the merged static
    /// batch emits exactly the visible (geoset alpha x layer alpha x instance)
    /// vertex count, carries per-layer alpha, and applies the team tint only to
    /// team-colour layers.
    #[test]
    #[ignore = "round2: doodad alpha/teamcolor audit"]
    fn agent_world_doodad_alpha_teamcolor() {
        let Some((mpq, _)) = fixtures() else {
            crate::dev_log!("SKIP doodad alpha: fixtures missing");
            return;
        };
        let mut maps = Vec::new();
        for name in [
            "Human04.w3m",
            "Orc01.w3m",
            "Undead01.w3m",
            "NightElf01.w3m",
            "NightElf04.w3m",
            "Orc07.w3m",
            "Undead07.w3m",
            "Human08.w3m",
        ] {
            if let Some(map) = campaign_map(name) {
                maps.push((name, map));
            }
        }
        let mut any_team = false;
        for (name, map) in &maps {
            let assets = Assets::load(Some(&mpq), Some(map));
            let doodads = DoodadWorld::load(&assets);
            if doodads.stats.source != "re" {
                crate::dev_log!("DOODAD_ALPHA map={name} source={}", doodads.stats.source);
                continue;
            }
            let mut alpha_layers = 0usize;
            let mut team_layers = 0usize;
            let mut hidden_meshes = 0usize;
            let mut partial_alpha = 0usize;
            for (model_index, model) in doodads.models.iter().enumerate() {
                for (mesh_index, mesh) in model.meshes.iter().enumerate() {
                    let geoset_alpha = model.alphas.get(mesh_index).copied().unwrap_or(1.0);
                    if geoset_alpha <= 0.01 || mesh.hide {
                        hidden_meshes += 1;
                        continue;
                    }
                    let instances = doodads
                        .instances
                        .iter()
                        .filter(|instance| instance.model == model_index)
                        .count();
                    for layer in &mesh.layers {
                        let alpha = geoset_alpha * layer.alpha;
                        if alpha <= 0.01 {
                            continue;
                        }
                        if alpha < 0.995 {
                            alpha_layers += 1;
                            partial_alpha += instances * mesh.positions.len();
                        }
                        if layer.team_color {
                            team_layers += 1;
                        }
                    }
                }
            }
            let (vertices, indices, specs) = build_doodad_geometry(&doodads, 0);
            // 16 shadow-decal vertices per instance are appended after the
            // model geometry (one merged decal draw for the whole map); tiny
            // footprints are skipped by the same radius gate as the builder.
            let shadow_vertices: usize = doodads
                .instances
                .iter()
                .filter(|instance| {
                    doodads
                        .models
                        .get(instance.model)
                        .map(|model| model.footprint * instance.scale * 0.42 >= 6.0)
                        .unwrap_or(false)
                })
                .count()
                * 16;
            let expected: usize = shadow_vertices
                + doodads
                .models
                .iter()
                .enumerate()
                .map(|(model_index, model)| {
                    let instances = doodads
                        .instances
                        .iter()
                        .filter(|instance| instance.model == model_index)
                        .count();
                    model
                        .meshes
                        .iter()
                        .enumerate()
                        .map(|(mesh_index, mesh)| {
                            let geoset_alpha = model.alphas.get(mesh_index).copied().unwrap_or(1.0);
                            if geoset_alpha <= 0.01 || mesh.hide || mesh.indices.is_empty() {
                                return 0;
                            }
                            let layers = mesh
                                .layers
                                .iter()
                                .filter(|layer| geoset_alpha * layer.alpha > 0.01)
                                .count();
                            instances * mesh.positions.len() * layers
                        })
                        .sum::<usize>()
                })
                .sum::<usize>();
            let alpha_vertices = vertices
                .iter()
                .filter(|vertex| (vertex.color[3] as u32) < (u16::MAX as u32) * 99 / 100)
                .count();
            let team_vertices = vertices
                .iter()
                .filter(|vertex| {
                    let r = vertex.color[0] as f32 / u16::MAX as f32;
                    let g = vertex.color[1] as f32 / u16::MAX as f32;
                    let b = vertex.color[2] as f32 / u16::MAX as f32;
                    (r - 0.55).abs() < 0.02 && (g - 0.72).abs() < 0.02 && (b - 1.0).abs() < 0.02
                })
                .count();
            crate::dev_log!(
                "DOODAD_ALPHA map={name} models={} instances={} specs={} vertices={} model_failures={} meshless={} texture_failures={} alpha_layers={alpha_layers} team_layers={team_layers} hidden_meshes={hidden_meshes} partial_alpha_vertices={partial_alpha} alpha_vertices={alpha_vertices} team_vertices={team_vertices}",
                doodads.models.len(),
                doodads.instances.len(),
                specs.len(),
                vertices.len(),
                doodads.stats.model_failures,
                doodads.stats.meshless_models,
                doodads.stats.texture_failures
            );
            assert_eq!(
                vertices.len(),
                expected,
                "{name}: static batch emits exactly the visible vertices"
            );
            assert!(!vertices.is_empty(), "{name}: doodad batch non-empty");
            assert_eq!(indices.len() % 3, 0, "{name}: triangle list");
            if alpha_layers > 0 {
                assert!(
                    alpha_vertices > 0,
                    "{name}: per-layer alpha reaches the vertex stream"
                );
            }
            if team_layers > 0 {
                any_team = true;
                assert!(
                    team_vertices > 0,
                    "{name}: team-colour layers carry the team tint"
                );
            }
        }
        crate::dev_log!("DOODAD_ALPHA_TEAM any_team_layers={any_team}");
    }

    /// Deterministic proof of the two material paths the map audit counts:
    /// a team-colour layer gets the team tint, a translucent layer keeps its
    /// alpha in the merged vertex stream (no texture/fixture needed).
    #[test]
    fn doodad_batch_applies_layer_alpha_and_team_tint() {
        use crate::assets::{MeshData, MeshLayer};
        use crate::doodads::{DoodadInstance, DoodadModel, DoodadWorld};
        use glam::{Mat4, Vec2, Vec3};

        let mesh = MeshData {
            positions: vec![Vec3::ZERO, Vec3::X, Vec3::Z],
            normals: vec![Vec3::Y; 3],
            uvs: vec![Vec2::ZERO; 3],
            uvs2: Vec::new(),
            indices: vec![0, 1, 2],
            bones: vec![Vec::new(); 3],
            layers: vec![
                MeshLayer {
                    texture: 0,
                    alpha: 0.5,
                    alpha_track: None,
                    blend: 2,
                    team_color: false,
                    uv_set: 0,
                },
                MeshLayer {
                    texture: 0,
                    alpha: 1.0,
                    alpha_track: None,
                    blend: 0,
                    team_color: true,
                    uv_set: 0,
                },
            ],
            texture: 0,
            texture_paths: Vec::new(),
            z_up: false,
            geoset_id: 0,
            hide: false,
            compact: None,
        };
        let doodads = DoodadWorld {
            models: vec![DoodadModel {
                meshes: vec![mesh],
                alphas: vec![1.0],
                footprint: 100.0,
            }],
            instances: vec![DoodadInstance {
                model: 0,
                world: Mat4::IDENTITY,
                scale: 1.0,
            }],
            textures: Vec::new(),
            stats: Default::default(),
        };
        let (vertices, _indices, specs) = build_doodad_geometry(&doodads, 0);
        assert_eq!(specs.len(), 3, "two layers + one shadow decal");
        let translucent = vertices
            .iter()
            .filter(|vertex| vertex.color[3] < u16::MAX / 2 + 2000)
            .count();
        let team = vertices
            .iter()
            .filter(|vertex| {
                (vertex.color[0] as f32 / u16::MAX as f32 - 0.55).abs() < 0.01
                    && (vertex.color[1] as f32 / u16::MAX as f32 - 0.72).abs() < 0.01
                    && vertex.color[2] == u16::MAX
            })
            .count();
        crate::dev_log!(
            "DOODAD_BATCH vertices={} translucent={translucent} team_tinted={team}",
            vertices.len()
        );
        assert!(translucent >= 3, "layer alpha reaches the vertex stream");
        assert!(team >= 3, "team-colour layer gets the team tint");
    }
}
