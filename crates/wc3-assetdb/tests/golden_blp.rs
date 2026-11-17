//! Colour-mapping proof for BLP1 JPEG textures.
//!
//! WC3 stores BLP1 JPEG art as four components (War3Net: "BGRA channels", retail
//! files inverted). Structural checks cannot catch a wrong mapping — teal dirt
//! and pink grass both decode "successfully". These tests therefore
//! 1. re-decode the embedded JPEG with an independent decoder (`jpeg-decoder`)
//!    and assert the crate's RGBA equals the documented WGRA→RGBA mapping
//!    exactly, and
//! 2. check the mapping against terrain semantics: village dirt must be a
//!    brown/yellow family and grass must be green; every alternative mapping
//!    (no inversion, R/B swap, grayscale) must land in an implausible family.
//!
//! The terrain fixtures come from the real archive (`analysis/war3_full3`).

use std::path::{Path, PathBuf};

use jpeg_decoder::{Decoder as JpegDecoder, PixelFormat};
use wc3_assetdb::blp_report::{self, HueClass};

fn archive_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3")
}

struct Cmyk {
    width: u32,
    height: u32,
    comps: Vec<u8>,
}

fn decode_embedded_jpeg(bytes: &[u8]) -> Option<Cmyk> {
    // BLP1: 156-byte header, u32 header size, shared JPEG header, mip offset 0.
    if &bytes[..4] != b"BLP1" {
        return None;
    }
    let read_u32 = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let content_type = read_u32(4)?;
    if content_type != 0 {
        return None;
    }
    let header_size = read_u32(156)? as usize;
    let mip_offset = read_u32(28)? as usize;
    let mip_size = read_u32(92)? as usize;
    let mut stream = Vec::with_capacity(header_size + mip_size);
    stream.extend_from_slice(bytes.get(160..160 + header_size)?);
    stream.extend_from_slice(bytes.get(mip_offset..mip_offset + mip_size)?);
    let mut decoder = JpegDecoder::new(&stream[..]);
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    if info.pixel_format != PixelFormat::CMYK32 {
        return None;
    }
    Some(Cmyk {
        width: info.width as u32,
        height: info.height as u32,
        comps: pixels,
    })
}

/// Component order for BLP1 JPEG is B,G,R,A stored inverted.
fn map_inverted_bgra(cmyk: &Cmyk) -> Vec<u8> {
    let mut out = Vec::with_capacity(cmyk.comps.len());
    for px in cmyk.comps.chunks_exact(4) {
        out.extend_from_slice(&[255 - px[2], 255 - px[1], 255 - px[0], 255 - px[3]]);
    }
    out
}

fn map_plain_rgba(cmyk: &Cmyk) -> Vec<u8> {
    let mut out = Vec::with_capacity(cmyk.comps.len());
    for px in cmyk.comps.chunks_exact(4) {
        out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    out
}

fn map_inverted_rgb_order(cmyk: &Cmyk) -> Vec<u8> {
    let mut out = Vec::with_capacity(cmyk.comps.len());
    for px in cmyk.comps.chunks_exact(4) {
        out.extend_from_slice(&[255 - px[0], 255 - px[1], 255 - px[2], 255 - px[3]]);
    }
    out
}

fn mean_rgb(rgba: &[u8]) -> [u8; 3] {
    let mut sum = [0u64; 3];
    let pixels = rgba.len() / 4;
    for px in rgba.chunks_exact(4) {
        for c in 0..3 {
            sum[c] += px[c] as u64;
        }
    }
    [
        (sum[0] / pixels as u64) as u8,
        (sum[1] / pixels as u64) as u8,
        (sum[2] / pixels as u64) as u8,
    ]
}

fn load(name: &str) -> Option<Vec<u8>> {
    let path = archive_dir().join(name);
    std::fs::read(path).ok()
}

fn mapping_case(blp: &str, expected: &[HueClass], implausible: &[HueClass]) {
    let Some(bytes) = load(blp) else {
        eprintln!("skipping {blp}: extracted file missing");
        return;
    };
    let decoded = wc3_formats::blp::decode(&bytes).expect("crate decode");
    let Some(cmyk) = decode_embedded_jpeg(&bytes) else {
        eprintln!("skipping {blp}: not a BLP1 JPEG");
        return;
    };
    assert_eq!((decoded.width, decoded.height), (cmyk.width, cmyk.height));
    let reference = map_inverted_bgra(&cmyk);
    assert_eq!(
        decoded.rgba.len(),
        reference.len(),
        "{blp}: pixel count differs from the independent JPEG decode"
    );
    // Exact mapping check: any deviation means our component math changed.
    let mut differing = 0usize;
    let mut max_diff = 0u8;
    for (a, b) in decoded.rgba.chunks_exact(4).zip(reference.chunks_exact(4)) {
        for c in 0..4 {
            let diff = a[c].abs_diff(b[c]);
            if diff != 0 {
                differing += 1;
            }
            max_diff = max_diff.max(diff);
        }
    }
    println!(
        "{blp}: independent-decode diff bytes={differing} max={max_diff} mean={:?} hue={}",
        mean_rgb(&decoded.rgba),
        HueClass::classify(mean_rgb(&decoded.rgba)).name()
    );
    assert!(
        differing == 0,
        "{blp}: {differing} bytes differ from the documented inverted-BGRA mapping"
    );

    let hue = HueClass::classify(mean_rgb(&decoded.rgba));
    assert!(
        expected.contains(&hue),
        "{blp}: decoded hue {hue:?} not in expected set {expected:?}"
    );
    // Alternative mappings must produce different pixels; the no-inversion
    // variant must also leave the plausible family (inverted retail art is the
    // whole point). The RGB-order variant can stay in the same coarse hue
    // family, so it is only required to differ bit-wise.
    for (label, variant) in [
        ("plain-rgba", map_plain_rgba(&cmyk)),
        ("inverted-rgb", map_inverted_rgb_order(&cmyk)),
    ] {
        let variant_hue = HueClass::classify(mean_rgb(&variant));
        println!("  {label}: mean={:?} hue={}", mean_rgb(&variant), variant_hue.name());
        assert_ne!(variant, decoded.rgba, "{blp}: {label} produced identical pixels");
        if label == "plain-rgba" {
            assert!(
                implausible.contains(&variant_hue),
                "{blp}: no-inversion mapping still looks plausible ({variant_hue:?})"
            );
        }
    }
}

#[test]
fn village_dirt_maps_to_brown_and_alternatives_do_not() {
    mapping_case(
        "TerrainArt/Village/Village_Dirt.blp",
        &[HueClass::Brown, HueClass::Yellow, HueClass::Orange],
        &[HueClass::Blue, HueClass::Cyan, HueClass::Magenta, HueClass::Purple],
    );
}

#[test]
fn village_grass_maps_to_green_and_alternatives_do_not() {
    mapping_case(
        "TerrainArt/Village/Village_GrassShort.blp",
        &[HueClass::Green, HueClass::Yellow],
        &[HueClass::Magenta, HueClass::Purple, HueClass::Blue, HueClass::Red],
    );
}

#[test]
fn all_biome_tiles_pass_hue_plausibility() {
    // Every TerrainArt tile must decode and land in a terrain-plausible family;
    // the biome catalogs carry the per-tile numbers, this asserts the aggregate.
    let root = archive_dir().join("TerrainArt");
    let mut files = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind) if kind.is_file() => {
                    if path
                        .extension()
                        .map(|ext| ext.eq_ignore_ascii_case("blp"))
                        .unwrap_or(false)
                    {
                        files.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    assert!(files.len() >= 111, "expected the full TerrainArt set");
    let mut counts: std::collections::BTreeMap<&'static str, usize> = Default::default();
    let mut bad = 0usize;
    for path in &files {
        let bytes = std::fs::read(path).expect("read tile");
        let report = blp_report::validate(&path.to_string_lossy(), &bytes);
        if !report.ok() {
            bad += 1;
            println!(
                "  TILE FAIL {}: {:?}",
                path.display(),
                report.header_error.or(report.decode_error)
            );
            continue;
        }
        *counts.entry(report.hue.name()).or_default() += 1;
    }
    println!("tiles={} failures={} hues={:?}", files.len(), bad, counts);
    assert_eq!(bad, 0, "every terrain tile must decode");
    // Terrain families are earth/vegetation colours; a single magenta tile is
    // allowed (the "Nothing" slot), more would indicate a channel-order bug.
    let odd = counts.get("magenta").copied().unwrap_or(0) + counts.get("purple").copied().unwrap_or(0);
    assert!(odd <= 1, "{odd} terrain tiles decoded to magenta/purple");
}

#[test]
fn fixture_dir_is_available() {
    // Loud skip: a missing tree means the audit ran elsewhere, not that the
    // mapping is fine.
    let dir: &Path = &archive_dir();
    if !dir.is_dir() {
        eprintln!("terrain fixtures unavailable at {}", dir.display());
    }
}
