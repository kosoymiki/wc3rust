//! Terrain tile decoding: authored BLP mips, native detail, tileable edges.
//!
//! A WC3 `TerrainArt` BLP is a sheet of 128px variations (`Village_Dirt` is
//! 512x256 = 4x2, `Village_GrassShort` is 256x256 = 2x2). The renderer used to
//! average every variation into one 128px tile, which is exactly the soft
//! "shakal" look: every texel became the mean of 2-16 unrelated samples and the
//! authored JPEG/DXT mip chain was thrown away.
//!
//! [`tile_from_sheet`] fixes the root of that pipeline:
//!
//! 1. one variation is copied at native resolution (the one with the highest
//!    alpha coverage, so layer masks keep their shapes);
//! 2. the sheet's own mip levels are reused - each level extracts the same grid
//!    cell at that level's size, so the GPU filters the artist's chain;
//! 3. only a missing tail of the pyramid is generated, box-filtered in linear
//!    light with wrapped taps so the tile stays seamless;
//! 4. the base level gets a 2px wrap-edge blend instead of the old 10px fade,
//!    which removed a visible seam without smearing the detail.

use crate::texture::{decode_lut, encode_srgb};

/// One authored mip level of a texture sheet (top level first).
pub struct SheetLevel<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
}

/// One level of a terrain tile mip chain (top level first).
#[derive(Clone, PartialEq, Eq)]
pub struct TileLevel {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Alpha-weighted mean colour of a sheet, used to fill layer-mask holes.
fn sheet_base(level: &SheetLevel<'_>) -> [u8; 3] {
    let mut sum = [0u64; 3];
    let mut weight = 0u64;
    for pixel in level.rgba.chunks_exact(4) {
        let alpha = pixel[3] as u64;
        for channel in 0..3 {
            sum[channel] += pixel[channel] as u64 * alpha;
        }
        weight += alpha;
    }
    if weight == 0 {
        return [96, 110, 72];
    }
    [
        (sum[0] / weight) as u8,
        (sum[1] / weight) as u8,
        (sum[2] / weight) as u8,
    ]
}

/// Copy one grid cell of `level`, filling transparent texels with `base`.
fn extract_cell(
    level: &SheetLevel<'_>,
    col: u32,
    row: u32,
    cols: u32,
    rows: u32,
    base: [u8; 3],
) -> TileLevel {
    let width = (level.width / cols.max(1)).max(1);
    let height = (level.height / rows.max(1)).max(1);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    let origin_x = col.min(cols.saturating_sub(1)) * width;
    let origin_y = row.min(rows.saturating_sub(1)) * height;
    for y in 0..height {
        for x in 0..width {
            let sx = (origin_x + x).min(level.width.saturating_sub(1));
            let sy = (origin_y + y).min(level.height.saturating_sub(1));
            let index = ((sy * level.width + sx) * 4) as usize;
            if index + 3 >= level.rgba.len() {
                rgba.extend_from_slice(&[base[0], base[1], base[2], 0]);
                continue;
            }
            let alpha = level.rgba[index + 3] as u32;
            for channel in 0..3 {
                let value = level.rgba[index + channel] as u32;
                let mixed = (value * alpha + base[channel] as u32 * (255 - alpha) + 127) / 255;
                rgba.push(mixed as u8);
            }
            rgba.push(alpha as u8);
        }
    }
    TileLevel {
        width,
        height,
        rgba,
    }
}

/// Cross-fade the four borders toward their shared edge profile.
///
/// `band` texels on each side are blended to the average of the opposite
/// borders, which makes a world-space repeat continuous. Two texels keep the
/// authored centre sharp; the old 10px band was a visible blur strip.
fn make_seamless(level: &mut TileLevel, band: u32) {
    let width = level.width as usize;
    let height = level.height as usize;
    if width < 2 || height < 2 {
        return;
    }
    let band = (band as usize).min(width / 4).min(height / 4).max(1);
    let read = |rgba: &[u8], x: usize, y: usize| -> [f32; 4] {
        let index = (y * width + x) * 4;
        [
            rgba[index] as f32,
            rgba[index + 1] as f32,
            rgba[index + 2] as f32,
            rgba[index + 3] as f32,
        ]
    };
    // Horizontal: left/right edges toward their average.
    for y in 0..height {
        let left = read(&level.rgba, 0, y);
        let right = read(&level.rgba, width - 1, y);
        let edge = [
            (left[0] + right[0]) * 0.5,
            (left[1] + right[1]) * 0.5,
            (left[2] + right[2]) * 0.5,
            (left[3] + right[3]) * 0.5,
        ];
        for i in 0..band {
            let t = 1.0 - i as f32 / band as f32;
            for (x, toward) in [(i, edge), (width - 1 - i, edge)] {
                let current = read(&level.rgba, x, y);
                let index = (y * width + x) * 4;
                for channel in 0..4 {
                    level.rgba[index + channel] =
                        (current[channel] + (toward[channel] - current[channel]) * t)
                            .clamp(0.0, 255.0)
                            .round() as u8;
                }
            }
        }
    }
    // Vertical: bottom/top edges toward their average.
    for x in 0..width {
        let top = read(&level.rgba, x, 0);
        let bottom = read(&level.rgba, x, height - 1);
        let edge = [
            (top[0] + bottom[0]) * 0.5,
            (top[1] + bottom[1]) * 0.5,
            (top[2] + bottom[2]) * 0.5,
            (top[3] + bottom[3]) * 0.5,
        ];
        for i in 0..band {
            let t = 1.0 - i as f32 / band as f32;
            for (y, toward) in [(i, edge), (height - 1 - i, edge)] {
                let current = read(&level.rgba, x, y);
                let index = (y * width + x) * 4;
                for channel in 0..4 {
                    level.rgba[index + channel] =
                        (current[channel] + (toward[channel] - current[channel]) * t)
                            .clamp(0.0, 255.0)
                            .round() as u8;
                }
            }
        }
    }
}

/// One wrapped box-filter step in linear light.
fn half_level(level: &TileLevel) -> TileLevel {
    let next_width = (level.width / 2).max(1);
    let next_height = (level.height / 2).max(1);
    let decode = decode_lut();
    let mut out = vec![0u8; (next_width * next_height * 4) as usize];
    for y in 0..next_height {
        for x in 0..next_width {
            let mut acc = [0f32; 3];
            let mut alpha = 0f32;
            for dy in 0..2u32 {
                for dx in 0..2u32 {
                    let sx = ((x * 2 + dx) % level.width) as usize;
                    let sy = ((y * 2 + dy) % level.height) as usize;
                    let index = (sy * level.width as usize + sx) * 4;
                    for channel in 0..3 {
                        acc[channel] += decode[level.rgba[index + channel] as usize];
                    }
                    alpha += level.rgba[index + 3] as f32;
                }
            }
            let dst = ((y * next_width + x) * 4) as usize;
            for channel in 0..3 {
                out[dst + channel] = encode_srgb(acc[channel] * 0.25);
            }
            out[dst + 3] = (alpha * 0.25).round().clamp(0.0, 255.0) as u8;
        }
    }
    TileLevel {
        width: next_width,
        height: next_height,
        rgba: out,
    }
}

/// Wrap-seam metric of a tile: mean absolute RGB difference across the repeat
/// boundary (x = W-1 vs x = 0 and y = H-1 vs y = 0), divided by the mean interior
/// neighbour difference. A seamless tile scores near or below 1; a hard seam
/// scores tens to hundreds. This is the numeric "no quilt / no seam" check.
pub fn seam_metric(level: &TileLevel) -> f32 {
    if level.width < 2 || level.height < 2 {
        return 0.0;
    }
    let width = level.width as usize;
    let height = level.height as usize;
    let pixel = |x: usize, y: usize| -> [i32; 3] {
        let index = (y * width + x) * 4;
        [
            level.rgba[index] as i32,
            level.rgba[index + 1] as i32,
            level.rgba[index + 2] as i32,
        ]
    };
    let diff = |a: [i32; 3], b: [i32; 3]| -> i64 {
        (a[0] - b[0]).abs() as i64 + (a[1] - b[1]).abs() as i64 + (a[2] - b[2]).abs() as i64
    };
    let mut edge = 0i64;
    let mut interior = 0i64;
    let mut pairs = 0i64;
    for y in 0..height {
        edge += diff(pixel(0, y), pixel(width - 1, y));
        for x in 0..width - 1 {
            interior += diff(pixel(x, y), pixel(x + 1, y));
            pairs += 1;
        }
    }
    for x in 0..width {
        edge += diff(pixel(x, 0), pixel(x, height - 1));
    }
    let boundary = edge as f64 / (height + width) as f64;
    let local = interior as f64 / pairs.max(1) as f64;
    (boundary / local.max(1e-6)) as f32
}

/// Build one terrain tile and its mip chain from an authored sheet.
///
/// `levels` is the sheet's mip chain (top level first, as `wc3_formats::blp`
/// returns it); `variation_px` is the variation cell size (128 for TerrainArt).
/// Returns the tile levels and the sheet's base colour.
pub fn tile_from_sheet(levels: &[SheetLevel<'_>], variation_px: u32) -> (Vec<TileLevel>, [u8; 3]) {
    let Some(base_level) = levels.first() else {
        return (Vec::new(), [0; 3]);
    };
    if base_level.width == 0 || base_level.height == 0 {
        return (Vec::new(), [0; 3]);
    }
    let variation = variation_px
        .min(base_level.width)
        .min(base_level.height)
        .max(1);
    let cols = (base_level.width / variation).max(1);
    let rows = (base_level.height / variation).max(1);

    // Pick the variation with the most opaque texels: WC3 layer masks are
    // alpha shapes, and an opaque cell keeps the mask crisp while needing the
    // least hole filling.
    let mut best = (0u32, 0u32, 0u64);
    for row in 0..rows {
        for col in 0..cols {
            let cell = extract_cell(base_level, col, row, cols, rows, [0, 0, 0]);
            let coverage: u64 = cell.rgba.chunks_exact(4).map(|p| p[3] as u64).sum();
            if coverage > best.2 {
                best = (col, row, coverage);
            }
        }
    }
    let (col, row, _) = best;
    let base = sheet_base(base_level);

    let mut tile_levels = Vec::with_capacity(levels.len().max(1) + 2);
    let mut last_size = None;
    for level in levels {
        if level.width == 0 || level.height == 0 {
            continue;
        }
        let cell = extract_cell(level, col, row, cols, rows, base);
        let size = (cell.width, cell.height);
        if last_size == Some(size) {
            continue;
        }
        last_size = Some(size);
        let done = cell.width == 1 && cell.height == 1;
        tile_levels.push(cell);
        if done {
            break;
        }
    }

    // Authored chains are often complete, but a malformed/short file must still
    // yield a full pyramid through 1x1.
    while let Some(last) = tile_levels.last() {
        if last.width == 1 && last.height == 1 {
            break;
        }
        let next = half_level(last);
        tile_levels.push(next);
    }

    if let Some(first) = tile_levels.first_mut() {
        make_seamless(first, 2);
    }
    (tile_levels, base)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Four 128px variations in one 256x256 sheet: red, green, blue, white.
    fn quad_sheet() -> Vec<u8> {
        let mut rgba = vec![0u8; 256 * 256 * 4];
        let colors = [
            [255u8, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 255, 255, 255],
        ];
        for y in 0..256 {
            for x in 0..256 {
                let index = (y * 256 + x) * 4;
                let quadrant = (y / 128) * 2 + (x / 128);
                rgba[index..index + 4].copy_from_slice(&colors[quadrant]);
            }
        }
        rgba
    }

    #[test]
    fn tile_keeps_authored_detail() {
        let rgba = quad_sheet();
        let levels = [SheetLevel {
            width: 256,
            height: 256,
            rgba: &rgba,
        }];
        let (tile, base) = tile_from_sheet(&levels, 128);
        assert_eq!((tile[0].width, tile[0].height), (128, 128));
        // A single variation, not the average of all four: old composite gave a
        // muddy (bright) mean; this must be one pure authored colour.
        let first = &tile[0].rgba[0..4];
        assert!(
            first == [255, 0, 0, 255] || first == [0, 255, 0, 255] || first == [0, 0, 255, 255],
            "unexpected averaged texel {first:?}"
        );
        assert_eq!(base, [127, 127, 127], "alpha-weighted sheet mean");
        // Every texel of the chosen variation has the same colour.
        let color = [tile[0].rgba[0], tile[0].rgba[1], tile[0].rgba[2]];
        assert!(tile[0]
            .rgba
            .chunks_exact(4)
            .all(|p| [p[0], p[1], p[2]] == color));
    }

    #[test]
    fn tile_picks_the_opaque_variation() {
        let mut rgba = quad_sheet();
        // Make the first variation fully transparent; the third (blue) stays.
        for y in 0..128 {
            for x in 0..128 {
                let index = (y * 256 + x) * 4;
                rgba[index + 3] = 0;
            }
        }
        let levels = [SheetLevel {
            width: 256,
            height: 256,
            rgba: &rgba,
        }];
        let (tile, _) = tile_from_sheet(&levels, 128);
        // The first fully opaque variation in sheet order is the green cell
        // (1,0); later cells tie on coverage and do not replace it.
        assert_eq!(&tile[0].rgba[0..3], &[0, 255, 0]);
    }

    #[test]
    fn authored_mips_are_reused() {
        let base = vec![255u8; 256 * 256 * 4]; // white sheet
        let mip = vec![0u8, 0, 255, 255].repeat(128 * 128); // blue level-1 sheet
        let levels = [
            SheetLevel {
                width: 256,
                height: 256,
                rgba: &base,
            },
            SheetLevel {
                width: 128,
                height: 128,
                rgba: &mip,
            },
        ];
        let (tile, _) = tile_from_sheet(&levels, 128);
        assert_eq!((tile[0].width, tile[0].height), (128, 128));
        assert_eq!((tile[1].width, tile[1].height), (64, 64));
        // The authored mip is blue; a box filter of the white base would be white.
        assert_eq!(&tile[1].rgba[0..3], &[0, 0, 255]);
    }

    #[test]
    fn missing_mips_are_generated_to_one_pixel() {
        let rgba = vec![120u8; 128 * 128 * 4];
        let levels = [SheetLevel {
            width: 128,
            height: 128,
            rgba: &rgba,
        }];
        let (tile, _) = tile_from_sheet(&levels, 128);
        assert_eq!(tile.len(), 8, "128 -> 1 is eight levels");
        assert_eq!((tile.last().unwrap().width, tile.last().unwrap().height), (1, 1));
        assert_eq!(tile.last().unwrap().rgba[0..3], [120, 120, 120]);
    }

    #[test]
    fn alpha_holes_are_filled_with_the_sheet_base() {
        // One 128px variation only, so the hole is inside the chosen cell
        // (a 2x2 sheet would pick the fully opaque variation instead).
        let mut rgba = vec![0u8; 128 * 128 * 4];
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[200, 40, 20, 255]);
        }
        let hole_index = (32 * 128 + 32) * 4;
        rgba[hole_index + 3] = 0;
        rgba[hole_index] = 7; // garbage RGB under zero alpha
        let levels = [SheetLevel {
            width: 128,
            height: 128,
            rgba: &rgba,
        }];
        let (tile, base) = tile_from_sheet(&levels, 128);
        let hole = &tile[0].rgba[(32 * 128 + 32) * 4..(32 * 128 + 32) * 4 + 4];
        assert_eq!(hole[3], 0, "the layer mask alpha is preserved");
        assert_eq!(&hole[0..3], &base[0..3], "RGB is the sheet base");
        assert_eq!(base, [200, 40, 20], "alpha-weighted mean ignores the hole");
    }

    #[test]
    fn seam_metric_separates_hard_seam_from_blended_repeat() {
        let mut ramp = vec![0u8; 128 * 128 * 4];
        for y in 0..128 {
            for x in 0..128 {
                let index = (y * 128 + x) * 4;
                let value = (x * 2).min(255) as u8;
                ramp[index..index + 4].copy_from_slice(&[value, value, value, 255]);
            }
        }
        let hard = TileLevel {
            width: 128,
            height: 128,
            rgba: ramp.clone(),
        };
        let hard_metric = seam_metric(&hard);
        assert!(hard_metric > 20.0, "hard seam metric {hard_metric}");
        let sheet = [SheetLevel {
            width: 128,
            height: 128,
            rgba: &ramp,
        }];
        let (levels, _) = tile_from_sheet(&sheet, 128);
        let blended = seam_metric(&levels[0]);
        assert!(blended < 2.0, "blended seam metric {blended}");
        assert!(blended < hard_metric / 10.0);
    }

    #[test]
    fn base_level_is_seamless() {
        // A vertical ramp would show a hard seam between x=127 and x=0; the
        // 2px edge blend must make the border texels meet closer.
        let mut rgba = vec![0u8; 128 * 128 * 4];
        for y in 0..128 {
            for x in 0..128 {
                let index = (y * 128 + x) * 4;
                let value = (x * 2).min(255) as u8;
                rgba[index..index + 4].copy_from_slice(&[value, value, value, 255]);
            }
        }
        let levels = [SheetLevel {
            width: 128,
            height: 128,
            rgba: &rgba,
        }];
        let (tile, _) = tile_from_sheet(&levels, 128);
        let left = tile[0].rgba[(64 * 128) * 4] as i32;
        let right = tile[0].rgba[(64 * 128 + 127) * 4] as i32;
        let gap = (left - right).abs();
        let authored_gap = 254; // 0 vs 254 without the blend
        assert!(
            gap < authored_gap / 2,
            "edge gap {gap} must be much smaller than {authored_gap}"
        );
    }
}
