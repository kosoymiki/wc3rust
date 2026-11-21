//! WC3-I-018 / I-019 probe: texture pre-resize and NPOT mip footprints.
//!
//! 1. **I-018** — `TextureImage::fit*` used to average sRGB bytes directly, so a
//!    black/white checkerboard downsized to mid-grey 128 instead of the
//!    linear-light answer (≈188). `fit_for(.., Role::Color)` now decodes first,
//!    `Role::Coverage` keeps the raw byte average.
//! 2. **I-019** — `generate_mips` averaged a fixed 2x2 block and used `w/2`, so a
//!    3x3 source to 1x1 read only the top-left four texels and the last
//!    row/column never entered the chain. The area-footprint resampler covers
//!    the whole source; the probe puts the only white texel in the last row and
//!    column and checks the 1x1 level.
//!
//! Run: `scripts/h2-cargo run --release -p wc3-game --example audit_texture_probe`.

use wc3_render::texture::{
    decode_srgb_reference, encode_srgb_reference, generate_mips, resize_box, resize_box_srgb, Role,
    TextureImage,
};

fn checker(size: u32) -> TextureImage {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let value = if (x + y) % 2 == 0 { 0 } else { 255 };
            let index = ((y * size + x) * 4) as usize;
            rgba[index] = value;
            rgba[index + 1] = value;
            rgba[index + 2] = value;
            rgba[index + 3] = 255;
        }
    }
    TextureImage::new(size, size, rgba)
}

fn main() {
    println!("== I-018: sRGB pre-resize ==");
    let board = checker(64);
    let (linear, _) = board.fit_for(32, Role::Color);
    let (raw, _) = board.fit_for(32, Role::Coverage);
    let reference = encode_srgb_reference((decode_srgb_reference(0) + decode_srgb_reference(255)) * 0.5);
    println!(
        "checker 64->32 center: linear-light={} raw-srgb={} reference={}",
        linear.rgba[0], raw.rgba[0], reference
    );
    // A pure black/white 2x2 -> 1x1 is the smallest reproducer of the darkening.
    let tiny = checker(2);
    let (tiny_linear, _) = tiny.fit_for(1, Role::Color);
    let (tiny_raw, _) = tiny.fit_for(1, Role::Coverage);
    println!(
        "checker 2->1: linear={} raw={} (the raw byte average is ~60 levels darker)",
        tiny_linear.rgba[0], tiny_raw.rgba[0]
    );
    assert!((tiny_linear.rgba[0] as i32 - reference as i32).abs() <= 1);
    // Raw byte average: (0+0+255+255)/4 = 127 (integer division).
    assert_eq!(tiny_raw.rgba[0], 127);

    println!("== I-019: NPOT mip footprint ==");
    let cases: &[(&str, u32, u32, (u32, u32))] = &[
        ("3x3 last-row/col white", 3, 3, (2, 2)),
        ("5x3 last-row/col white", 5, 3, (4, 2)),
        ("7x1 last column white", 7, 1, (6, 0)),
        ("1x7 last row white", 1, 7, (0, 6)),
        ("5x5 corner white", 5, 5, (4, 4)),
    ];
    for &(label, width, height, white) in cases {
        let image = one_pixel_ext(width, height, white);
        let mips = generate_mips(width, height, &image.rgba);
        let last = mips.last().expect("chain");
        // The old 2x2-only filter: source texel (0,0)..(1,1) for the first level,
        // so a white texel outside that block never entered the chain.
        let legacy_in = white.0 < 2 && white.1 < 2;
        println!(
            "{label}: levels={} last={}x{} last_r={} legacy_2x2_would_see_white={legacy_in}",
            mips.len(),
            last.width,
            last.height,
            last.rgba[0],
        );
        assert!(
            last.rgba[0] > 0,
            "{label}: the last row/column must influence the chain"
        );
    }
    // POT control: 4x4 with one white of sixteen reads the same either way.
    let pot = one_pixel_ext(4, 4, (3, 3));
    let mips = generate_mips(4, 4, &pot.rgba);
    let last = mips.last().expect("chain");
    let expected = encode_srgb_reference(1.0 / 16.0);
    println!(
        "POT 4x4 corner white -> 1x1 r={} expected_linear_light={expected}",
        last.rgba[0]
    );
    assert!((last.rgba[0] as i32 - expected as i32).abs() <= 1);

    println!("TEXTURE_PROBE PASS");
}

fn one_pixel_ext(width: u32, height: u32, white: (u32, u32)) -> TextureImage {
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    let index = ((white.1 * width + white.0) * 4) as usize;
    rgba[index..index + 3].fill(255);
    rgba[index + 3] = 255;
    TextureImage::new(width, height, rgba)
}
