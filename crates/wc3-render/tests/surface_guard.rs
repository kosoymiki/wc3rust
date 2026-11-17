//! Guard tests for the surface setup path (P1 tombstone fix).
//!
//! A `Surface::configure` validation failure is fatal unless it is raised inside
//! an error scope: wgpu's default uncaptured-error handler panics, which is the
//! `SIGABRT` in tombstone `org.herofolends.p0`
//! (`Surface does not support the adapter's queue family`). `configure_guarded`
//! uses exactly the scope exercised here; these tests prove the mechanism turns a
//! validation error into a returned value on the real device instead of aborting.

use wc3_render::surface::{configure_guarded, validate_caps};

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
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
    .ok()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("surface-guard"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))
    .ok()?;
    Some((device, queue))
}

/// The exact mechanism used by `configure_guarded`: a validation error inside the
/// scope is returned, not delivered to the default panic handler.
#[test]
fn validation_error_is_captured_not_fatal() {
    let Some((device, queue)) = gpu() else {
        println!("no Vulkan adapter: skipping surface guard test");
        return;
    };
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("guard"),
        size: 64,
        usage: wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    queue.write_buffer(&buffer, 60, &[0u8; 16]);
    let error = pollster::block_on(scope.pop());
    assert!(error.is_some(), "out-of-bounds write must be captured");
    // The process is still healthy: a valid operation and a clean scope work.
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    queue.write_buffer(&buffer, 0, &[1u8; 16]);
    let error = pollster::block_on(scope.pop());
    assert!(error.is_none(), "valid write must not raise an error");
}

/// The pre-configure capability guard rejects the exact condition that crashed:
/// an adapter that reports no surface formats (queue-family mismatch).
#[test]
fn empty_caps_are_rejected_without_touching_the_device() {
    assert!(validate_caps(&[], &[wgpu::PresentMode::Fifo], &[wgpu::CompositeAlphaMode::Auto]).is_err());
    assert!(validate_caps(&[wgpu::TextureFormat::Bgra8Unorm], &[], &[wgpu::CompositeAlphaMode::Auto]).is_err());
    assert!(validate_caps(
        &[wgpu::TextureFormat::Bgra8Unorm],
        &[wgpu::PresentMode::Fifo],
        &[wgpu::CompositeAlphaMode::Auto]
    )
    .is_ok());
}

/// A `configure_guarded` on a real device without a surface is not constructible
/// headlessly; this asserts the helper is exported with the fallible signature the
/// app relies on (compile-time guard against a future `-> ()` regression).
#[test]
fn guarded_configure_signature_is_fallible() {
    let _: fn(&wgpu::Device, &wgpu::Surface<'_>, &wgpu::SurfaceConfiguration) -> Result<(), String> =
        configure_guarded;
}
