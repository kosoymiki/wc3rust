use std::fs;
use std::path::PathBuf;

use wc3_core::{Fixed, PlayerId};
use wc3_gpu::GpuContext;
use wc3_platform::Sdl3;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Core smoke: deterministic fixed-point math and player bounds.
    let one = Fixed::from_int(1);
    assert_eq!(one.to_f32(), 1.0);
    let p = PlayerId::new(0)?;
    assert_eq!(p.raw(), 0);

    // SDL3 platform smoke (offscreen video driver on headless Termux).
    let sdl = Sdl3::init()?;
    println!("SDL3_VERSION={}", sdl.version_string());
    // SDL3 window/surface creation is P0-remaining on the offscreen host driver;
    // the real Android SDLActivity window path is validated in the APK slice.

    // Real Vulkan path: high-performance adapter, WGSL triangle, GPU readback.
    let gpu = GpuContext::new_headless(256, 256)?;
    let info = &gpu.adapter_info;
    println!(
        "GPU_ADAPTER name={:?} backend={:?} device_type={:?} driver={:?} driver_info={:?}",
        info.name, info.backend, info.device_type, info.driver, info.driver_info
    );

    let pixels = gpu.render_triangle()?;
    let expected = 256usize * 256 * 4;
    if pixels.len() != expected {
        return Err(format!("readback size {} != {}", pixels.len(), expected).into());
    }
    let (pixels, rest) = pixels.as_chunks::<4>();
    if !rest.is_empty() {
        return Err("readback length is not RGBA8".into());
    }
    let lit = pixels
        .iter()
        .filter(|px| px[1] > 100 && px[2] > 100)
        .count();
    if lit < 1_000 {
        return Err(format!("triangle not visible: lit pixels = {lit}").into());
    }

    let ppm = PathBuf::from("target/p0_triangle.ppm");
    if let Some(parent) = ppm.parent() {
        fs::create_dir_all(parent)?;
    }
    write_ppm(&ppm, 256, 256, pixels)?;
    println!("P0_GPU_TRIANGLE_OK lit_pixels={lit} ppm={}", ppm.display());
    Ok(())
}

fn write_ppm(path: &PathBuf, width: u32, height: u32, pixels: &[[u8; 4]]) -> std::io::Result<()> {
    let mut out = Vec::with_capacity((width * height * 3 + 32) as usize);
    out.extend_from_slice(format!("P6\n{width} {height}\n255\n").as_bytes());
    for px in pixels {
        out.extend_from_slice(&px[..3]);
    }
    fs::write(path, out)
}
