//! Prints the adapter capabilities the renderer cares about.
//!
//! Run on device to answer "can this driver take compressed textures at all?":
//! `cargo run -p wc3-render --example adapter_probe`.
//!
//! The output is the ground truth for [`wc3_render::texture::CompressionSupport`]
//! decisions; nothing here guesses.

fn main() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        flags: wgpu::InstanceFlags::empty(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .expect("no Vulkan adapter");
    let info = adapter.get_info();
    let features = adapter.features();
    let limits = adapter.limits();
    println!(
        "ADAPTER name={:?} backend={:?} driver={} info={}",
        info.name, info.backend, info.driver, info.driver_info
    );
    println!(
        "FEATURES astc={} astc_hdr={} etc2={} bc={} timestamp={} storage_vertex={}",
        features.contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC),
        features.contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC_HDR),
        features.contains(wgpu::Features::TEXTURE_COMPRESSION_ETC2),
        features.contains(wgpu::Features::TEXTURE_COMPRESSION_BC),
        features.contains(wgpu::Features::TIMESTAMP_QUERY),
        limits.max_storage_buffers_per_shader_stage > 0,
    );
    println!(
        "LIMITS max_texture_2d={} max_storage_vertex={} max_bind_groups={} uniform_alignment={}",
        limits.max_texture_dimension_2d,
        limits.max_storage_buffers_per_shader_stage,
        limits.max_bind_groups,
        limits.min_uniform_buffer_offset_alignment
    );
    println!(
        "SUPPORT_TEXTURES astc_ldr={} etc2={} bc={}",
        wc3_render::texture::CompressionSupport::probe(&adapter).astc_ldr,
        wc3_render::texture::CompressionSupport::probe(&adapter).etc2,
        wc3_render::texture::CompressionSupport::probe(&adapter).bc,
    );
}
