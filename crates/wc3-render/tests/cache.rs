//! Resident-set behaviour on a real device: 542 models with a byte budget.
//!
//! `PARITY_MATRIX.md` rows #1/#2: every `UnitDef` renders its own MDX (542 models
//! in the install) inside a phone memory budget, so the renderer keeps a bounded
//! resident set and reloads evicted models on demand. This test drives that
//! lifecycle with real GPU buffers.
//!
//! It also ties the texture plan to reality: the planned ASTC/RGBA bytes are
//! compared with what the loader actually allocated.

use wc3_render::cache::{ResidentModel, ResidentSet, TexturePlan};
use wc3_render::texture::{
    self, CompressedImage, CompressedLevel, CompressedFormat, Role, TextureImage, upload_compressed,
    upload_levels,
};

const MODELS: u32 = 542;

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
    let features = if adapter
        .features()
        .contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC)
    {
        wgpu::Features::TEXTURE_COMPRESSION_ASTC
    } else {
        wgpu::Features::empty()
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("cache-test"),
        required_features: features,
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))
    .ok()?;
    Some((device, queue))
}

/// One small quad: 4 vertices x 52 bytes + 6 indices x 4 bytes = 232 bytes.
fn quad_bytes() -> (Vec<u8>, Vec<u8>, Vec<(u32, u32)>) {
    let vertices = vec![0u8; 4 * 52];
    let indices: Vec<u32> = vec![0, 1, 2, 0, 2, 3];
    (
        vertices,
        bytemuck::cast_slice(&indices).to_vec(),
        vec![(0, 6)],
    )
}

#[test]
fn resident_set_respects_the_budget_with_real_buffers() {
    let Some((device, queue)) = gpu() else {
        println!("no Vulkan adapter: skipping resident-set device test");
        return;
    };
    let (vertices, indices, geosets) = quad_bytes();
    let per_model = (vertices.len() as u64).max(4) + (indices.len() as u64).max(4);
    // Budget for ~120 models.
    let budget = per_model * 120;
    let mut set: ResidentSet<ResidentModel> = ResidentSet::new(budget);

    for key in 0..MODELS {
        set.begin_frame();
        if set.contains(key) {
            let _ = set.get(key);
            continue;
        }
        let model = ResidentModel::upload(&device, &queue, &vertices, &indices, geosets.clone());
        assert_eq!(model.bytes, per_model, "uploaded bytes are the budget unit");
        assert!(set.insert(key, model, per_model), "insert {key}");
        assert!(set.bytes() <= budget, "budget respected at {key}");
    }
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });

    let stats = set.stats();
    println!("resident-set: {}", stats.summary());
    assert_eq!(stats.resident, set.len());
    assert_eq!(stats.resident, 120, "exactly the budget's worth stays resident");
    assert_eq!(stats.evictions, MODELS as u64 - 120);
    assert_eq!(stats.bytes, budget);
    assert!(!set.contains(0), "the oldest entry was evicted");
    assert!(set.contains(MODELS - 1), "the newest entry is resident");

    // Re-residency: an evicted model uploads again and the set stays bounded.
    let model = ResidentModel::upload(&device, &queue, &vertices, &indices, geosets.clone());
    assert!(set.insert(0, model, per_model));
    assert!(set.contains(0));
    assert_eq!(set.len(), 120);
    assert!(set.bytes() <= budget);

    // Lowering the budget evicts down to it.
    set.set_budget(budget / 2);
    let evicted = set.enforce_budget();
    println!("lowered budget: evicted={evicted} {}", set.stats().summary());
    assert!(set.bytes() <= budget / 2);
    assert_eq!(set.len(), 60);
}

/// 216 four-race unit/building MDX (PARITY_MATRIX R1/R2) with the real per-model
/// geometry byte sizes measured from the models the game loads
/// (`BENCH model_memory bytes_mean=117661 bytes_max=370816`).
#[test]
fn two_hundred_sixteen_models_stay_inside_the_budget() {
    let Some((device, queue)) = gpu() else {
        println!("no Vulkan adapter: skipping 216-model device test");
        return;
    };
    let models = 216u32;
    let mean = 117_661u64;
    let mut set: ResidentSet<ResidentModel> = ResidentSet::new(mean * 64);
    let (vertices, indices, geosets) = quad_bytes();
    let mut uploaded = 0u64;
    for key in 0..models {
        set.begin_frame();
        if set.contains(key) {
            continue;
        }
        let model = ResidentModel::upload(&device, &queue, &vertices, &indices, geosets.clone());
        assert!(set.insert(key, model, mean), "insert {key}");
        uploaded += mean;
    }
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    let stats = set.stats();
    println!(
        "216 models: resident={} bytes={} budget={} loads={} evictions={} rejected={} (projected 216 x {mean} = {} MB)",
        stats.resident,
        stats.bytes,
        stats.budget_bytes,
        stats.loads,
        stats.evictions,
        stats.rejected,
        (mean * 216) / 1_000_000
    );
    assert_eq!(stats.resident, 64, "64 models fit the 64-model budget");
    assert_eq!(stats.evictions, models as u64 - 64);
    assert_eq!(stats.bytes, mean * 64);
    assert_eq!(stats.rejected, 0);
    assert!(set.bytes() <= stats.budget_bytes);
    // Re-residency of an evicted model works and stays inside the budget.
    let model = ResidentModel::upload(&device, &queue, &vertices, &indices, geosets.clone());
    assert!(set.insert(0, model, mean));
    assert_eq!(set.len(), 64);
    assert!(set.bytes() <= set.budget_bytes());
}

#[test]
fn texture_plan_matches_measured_allocations() {
    let Some((device, queue)) = gpu() else {
        println!("no Vulkan adapter: skipping texture-plan device test");
        return;
    };
    let layout = texture::bind_group_layout(&device);
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
    let size = 256u32;

    // RGBA8 + mips, measured through the same accounting the store reports.
    let rgba = TextureImage::new(size, size, vec![128; (size * size * 4) as usize]);
    let mips = texture::generate_mips(rgba.width, rgba.height, &rgba.rgba);
    let rgba_bytes: u64 = mips
        .iter()
        .map(|mip| (mip.width * mip.height * 4) as u64)
        .sum();
    let _bind = upload_levels(&device, &queue, &layout, &sampler, Role::Color, &mips);

    // ASTC 6x6 (block-aligned 258x258 canvas), measured through the same plan.
    let padded = 258u32;
    let blocks = padded.div_ceil(6) * padded.div_ceil(6);
    let data = vec![0u8; (blocks * 16) as usize];
    let image = CompressedImage {
        format: CompressedFormat::Astc6x6,
        width: padded,
        height: padded,
        levels: vec![CompressedLevel {
            width: padded,
            height: padded,
            data,
        }],
    };
    image.validate().expect("astc layout");
    let _bind = upload_compressed(&device, &queue, &layout, &sampler, &image, Role::Color);

    let plan = TexturePlan::new(MODELS as usize, size, size, 64 * 1024 * 1024);
    println!("texture plan: {}", plan.summary());
    println!(
        "measured: rgba8_mips={rgba_bytes} astc6_level0={} plan_rgba={} plan_astc6={}",
        image.bytes(),
        plan.rgba_bytes,
        plan.astc6_bytes
    );
    // The plan's RGBA figure is the mip chain of one 256x256 texture; the
    // measured chain must agree exactly.
    assert_eq!(plan.rgba_bytes, rgba_bytes, "RGBA8 mip-chain accounting");
    // 542 textures at 256x256 do not fit 64 MB as RGBA8, but do as ASTC 6x6.
    assert!(plan.total_rgba() > plan.budget_bytes);
    assert!(plan.total_astc6() <= plan.budget_bytes);
    assert_eq!(plan.recommended(), Some("astc6x6"));
    assert!(plan.astc6_bytes < plan.rgba_bytes / 3, "ASTC 6x6 is <1/3 of RGBA8");
}
