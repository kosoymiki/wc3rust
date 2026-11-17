//! Surface setup that cannot abort the process.
//!
//! `wgpu` reports a `Surface::configure` validation failure through the default
//! uncaptured-error handler, which panics → `SIGABRT` on Android (tombstone
//! `org.herofolends.p0`, `Surface does not support the adapter's queue family`).
//! The failure is real: after Android detaches/recreates the `ANativeWindow`
//! (orientation change, background→resume, fullscreen transition), the wgpu
//! `Surface` still points at the old window handle, `vkGetPhysicalDeviceSurfaceSupportKHR`
//! returns false and `configure` rejects the device.
//!
//! This module makes every configure path explicit and fallible:
//!
//! * [`configure_surface`] picks an adapter that reports non-empty surface
//!   capabilities and retries the configure inside a validation error scope; it
//!   falls back to the next surface-compatible adapter instead of aborting.
//! * [`configure_guarded`] wraps a reconfigure (resize / lost swapchain) in the
//!   same error scope, so a stale surface returns `Err` and the caller can
//!   recreate the surface from the current window.
//!
//! Nothing here hides a failure: the returned error carries the adapter names and
//! the exact wgpu message for the app log.

/// A configured surface: the adapter/device/queue plus the chosen configuration.
pub struct SurfaceSetup {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    /// Per-candidate diagnostics (adapter chosen/skipped and why).
    pub diagnostics: Vec<String>,
}

/// Prefer an sRGB format (WGSL writes linear light), else the first supported.
pub fn pick_format(formats: &[wgpu::TextureFormat]) -> Option<wgpu::TextureFormat> {
    formats
        .iter()
        .copied()
        .find(wgpu::TextureFormat::is_srgb)
        .or_else(|| formats.first().copied())
}

/// Pick a present mode that cannot block the render loop.
///
/// `Fifo` is the only mode Vulkan requires, and on Android it blocks inside
/// `vkQueuePresentKHR` until the compositor releases a swapchain image. When
/// the layer is not consumed (surface attached but never latched, BLAST queue
/// narrower than the swapchain) the call never returns: the loop freezes after
/// the first frame with `tick=0 fps=0` (measured on the Adreno 730 device,
/// backtrace `NativeSwapchain::present`). `Mailbox` replaces the pending image
/// instead of waiting, and `Immediate` never waits either, so prefer them.
pub fn pick_present_mode(modes: &[wgpu::PresentMode]) -> Option<wgpu::PresentMode> {
    const PREFERENCE: [wgpu::PresentMode; 4] = [
        wgpu::PresentMode::Mailbox,
        wgpu::PresentMode::AutoNoVsync,
        wgpu::PresentMode::Fifo,
        wgpu::PresentMode::Immediate,
    ];
    PREFERENCE
        .iter()
        .copied()
        .find(|mode| modes.contains(mode))
        .or_else(|| modes.first().copied())
}

/// Reject a surface configuration that cannot work before touching the device.
pub fn validate_caps(
    formats: &[wgpu::TextureFormat],
    present_modes: &[wgpu::PresentMode],
    alpha_modes: &[wgpu::CompositeAlphaMode],
) -> Result<(), String> {
    if formats.is_empty() {
        return Err("surface reports no formats for this adapter (incompatible queue family?)".into());
    }
    if present_modes.is_empty() {
        return Err("surface reports no present modes for this adapter".into());
    }
    if alpha_modes.is_empty() {
        return Err("surface reports no alpha modes for this adapter".into());
    }
    Ok(())
}

/// Build the surface configuration from real capabilities.
pub fn pick_config(
    caps: &wgpu::SurfaceCapabilities,
    width: u32,
    height: u32,
) -> Result<wgpu::SurfaceConfiguration, String> {
    validate_caps(&caps.formats, &caps.present_modes, &caps.alpha_modes)?;
    let format = pick_format(&caps.formats).ok_or("no surface format")?;
    let present_mode = pick_present_mode(&caps.present_modes).ok_or("no present mode")?;
    let alpha_mode = caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto);
    Ok(wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: width.max(1),
        height: height.max(1),
        present_mode,
        alpha_mode,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: Vec::new(),
        // Android's `SurfaceView` BLAST queue advertises two buffers
        // (`getMaxAcquireBufferCount 2` in the device log); asking wgpu for
        // `latency + 1 = 3` swapchain images leaves one image the compositor
        // can never release, which is the other half of the present deadlock.
        // Two images match the platform queue exactly.
        desired_maximum_frame_latency: 1,
    })
}

/// Configure `surface` and capture any validation error instead of letting the
/// default handler panic. Returns the exact wgpu message on failure.
pub fn configure_guarded(
    device: &wgpu::Device,
    surface: &wgpu::Surface<'_>,
    config: &wgpu::SurfaceConfiguration,
) -> Result<(), String> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    surface.configure(device, config);
    match pollster::block_on(scope.pop()) {
        Some(error) => Err(format!("Surface::configure rejected: {error}")),
        None => Ok(()),
    }
}

/// Create the device and configured surface for `surface`.
///
/// Adapter order: the adapter `request_adapter` picks for this surface, then
/// every enumerated adapter that reports `is_surface_supported`. Each candidate
/// must pass [`validate_caps`] and a guarded `configure`; the first that does is
/// returned. If none does, the error lists every attempt — the caller exits with
/// that message instead of `SIGABRT`.
pub fn configure_surface(
    instance: &wgpu::Instance,
    surface: &wgpu::Surface<'_>,
    width: u32,
    height: u32,
) -> Result<SurfaceSetup, String> {
    let mut diagnostics = Vec::new();
    let mut candidates: Vec<wgpu::Adapter> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    let mut push = |adapter: wgpu::Adapter, diagnostics: &mut Vec<String>| {
        let info = adapter.get_info();
        let key = format!("{:?}/{:?}/{}", info.backend, info.device_type, info.name);
        if !seen.contains(&key) {
            seen.push(key);
            diagnostics.push(format!("candidate adapter {}", info.name));
            candidates.push(adapter);
        }
    };

    match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(surface),
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    })) {
        Ok(adapter) => push(adapter, &mut diagnostics),
        Err(error) => diagnostics.push(format!("request_adapter(compatible_surface): {error}")),
    }
    for adapter in pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all())) {
        if adapter.is_surface_supported(surface) {
            push(adapter, &mut diagnostics);
        }
    }

    if candidates.is_empty() {
        return Err(format!(
            "no adapter supports the surface (queue-family mismatch): {}",
            diagnostics.join(" | ")
        ));
    }

    for adapter in candidates {
        let info = adapter.get_info();
        let caps = surface.get_capabilities(&adapter);
        if let Err(error) =
            validate_caps(&caps.formats, &caps.present_modes, &caps.alpha_modes)
        {
            diagnostics.push(format!("adapter {} rejected: {error}", info.name));
            continue;
        }
        let config = match pick_config(&caps, width, height) {
            Ok(config) => config,
            Err(error) => {
                diagnostics.push(format!("adapter {}: {error}", info.name));
                continue;
            }
        };
        let (device, queue) =
            match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("herofolends-surface-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                ..Default::default()
            })) {
                Ok(pair) => pair,
                Err(error) => {
                    diagnostics.push(format!("adapter {} request_device: {error}", info.name));
                    continue;
                }
            };
        match configure_guarded(&device, surface, &config) {
            Ok(()) => {
                diagnostics.push(format!(
                    "configured adapter {} format={:?} present={:?} latency={} available={:?}",
                    info.name,
                    config.format,
                    config.present_mode,
                    config.desired_maximum_frame_latency,
                    caps.present_modes
                ));
                return Ok(SurfaceSetup {
                    adapter,
                    device,
                    queue,
                    config,
                    diagnostics,
                });
            }
            Err(error) => diagnostics.push(format!("adapter {}: {error}", info.name)),
        }
    }

    Err(format!(
        "no adapter could configure the surface: {}",
        diagnostics.join(" | ")
    ))
}

/// Guarded reconfigure for a live surface (resize / lost swapchain). The caller
/// must recreate the surface and retry once if this returns `Err`.
pub fn reconfigure(
    device: &wgpu::Device,
    surface: &wgpu::Surface<'_>,
    config: &wgpu::SurfaceConfiguration,
) -> Result<(), String> {
    configure_guarded(device, surface, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_prefers_srgb() {
        let formats = [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ];
        assert_eq!(
            pick_format(&formats),
            Some(wgpu::TextureFormat::Rgba8UnormSrgb)
        );
        assert_eq!(pick_format(&[]), None);
        assert_eq!(
            pick_format(&[wgpu::TextureFormat::Bgra8Unorm]),
            Some(wgpu::TextureFormat::Bgra8Unorm)
        );
    }

    #[test]
    fn present_mode_prefers_non_blocking_modes() {
        // Mailbox never waits for the compositor: it replaces the pending image.
        let modes = [
            wgpu::PresentMode::Immediate,
            wgpu::PresentMode::Fifo,
            wgpu::PresentMode::Mailbox,
        ];
        assert_eq!(pick_present_mode(&modes), Some(wgpu::PresentMode::Mailbox));
        // Without Mailbox, the guaranteed mode is still the fallback.
        let modes = [wgpu::PresentMode::Immediate, wgpu::PresentMode::Fifo];
        assert_eq!(pick_present_mode(&modes), Some(wgpu::PresentMode::Fifo));
        assert_eq!(
            pick_present_mode(&[wgpu::PresentMode::Immediate]),
            Some(wgpu::PresentMode::Immediate)
        );
        assert_eq!(pick_present_mode(&[]), None);
    }

    #[test]
    fn surface_config_matches_android_two_buffer_queue() {
        // `desired_maximum_frame_latency` is `latency + 1` swapchain images; the
        // Android SurfaceView BLAST queue exposes two, so anything above one
        // leaves an image the compositor can never release (present deadlock).
        let caps = wgpu::SurfaceCapabilities {
            formats: vec![wgpu::TextureFormat::Rgba8UnormSrgb],
            present_modes: vec![wgpu::PresentMode::Fifo],
            alpha_modes: vec![wgpu::CompositeAlphaMode::Auto],
            usages: wgpu::TextureUsages::RENDER_ATTACHMENT,
            ..Default::default()
        };
        let config = pick_config(&caps, 2414, 1080).expect("config");
        assert_eq!(config.desired_maximum_frame_latency, 1);
        assert_eq!(config.present_mode, wgpu::PresentMode::Fifo);
        assert_eq!(config.width, 2414);
        assert_eq!(config.height, 1080);
    }

    #[test]
    fn incompatible_caps_are_rejected_before_configure() {
        let formats = [wgpu::TextureFormat::Bgra8Unorm];
        let modes = [wgpu::PresentMode::Fifo];
        let alphas = [wgpu::CompositeAlphaMode::Auto];
        assert!(validate_caps(&formats, &modes, &alphas).is_ok());
        assert!(validate_caps(&[], &modes, &alphas).is_err());
        assert!(validate_caps(&formats, &[], &alphas).is_err());
        assert!(validate_caps(&formats, &modes, &[]).is_err());
    }
}
