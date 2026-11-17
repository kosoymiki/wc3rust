//! Mip-chain generation for game textures.
//!
//! Ported from Horizon's `mipmap.rs` (`ARGB.meanLinear`): colours are averaged
//! in linear light and converted back to sRGB, so a black/white checker does
//! not turn muddy grey. Alpha is averaged linearly. This removes the shimmering
//! and the dark halos our BLP textures showed when minified.

#[derive(Clone)]
pub struct MipLevel {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn srgb_to_linear(value: u8) -> f32 {
    let x = value as f32 / 255.0;
    if x >= 0.04045 {
        ((x + 0.055) / 1.055).powf(2.4)
    } else {
        x / 12.92
    }
}

fn linear_to_srgb(value: f32) -> u8 {
    let x = value.clamp(0.0, 1.0);
    let srgb = if x >= 0.003_130_8 {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    } else {
        12.92 * x
    };
    (srgb * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Full chain: level 0 is the input, then halved levels down to 1x1.
pub fn generate(width: u32, height: u32, rgba: &[u8]) -> Vec<MipLevel> {
    let width = width.max(1);
    let height = height.max(1);
    let expected = (width * height * 4) as usize;
    let mut pixels = if rgba.len() >= expected {
        rgba[..expected].to_vec()
    } else {
        let mut copy = rgba.to_vec();
        copy.resize(expected, 0);
        copy
    };
    let mut levels = vec![MipLevel {
        width,
        height,
        rgba: pixels.clone(),
    }];
    let (mut w, mut h) = (width, height);
    while w > 1 || h > 1 {
        let nw = (w / 2).max(1);
        let nh = (h / 2).max(1);
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                let sx = (x * 2).min(w - 1);
                let sy = (y * 2).min(h - 1);
                let sx1 = (sx + 1).min(w - 1);
                let sy1 = (sy + 1).min(h - 1);
                let sample = |px: u32, py: u32| -> [u8; 4] {
                    let index = ((py * w + px) * 4) as usize;
                    [
                        pixels[index],
                        pixels[index + 1],
                        pixels[index + 2],
                        pixels[index + 3],
                    ]
                };
                let a = sample(sx, sy);
                let b = sample(sx1, sy);
                let c = sample(sx, sy1);
                let d = sample(sx1, sy1);
                let mut rgb = [0u8; 3];
                for channel in 0..3 {
                    let linear = (srgb_to_linear(a[channel])
                        + srgb_to_linear(b[channel])
                        + srgb_to_linear(c[channel])
                        + srgb_to_linear(d[channel]))
                        * 0.25;
                    rgb[channel] = linear_to_srgb(linear);
                }
                let alpha = ((a[3] as u32 + b[3] as u32 + c[3] as u32 + d[3] as u32) / 4) as u8;
                let index = ((y * nw + x) * 4) as usize;
                next[index..index + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], alpha]);
            }
        }
        pixels = next;
        w = nw;
        h = nh;
        levels.push(MipLevel {
            width: w,
            height: h,
            rgba: pixels.clone(),
        });
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_colour_survives_every_level() {
        let levels = generate(2, 2, &[200, 80, 40, 255].repeat(4));
        for level in &levels {
            assert_eq!(&level.rgba[..3], &[200, 80, 40]);
            assert_eq!(level.rgba[3], 255);
        }
    }

    #[test]
    fn black_white_average_is_linear_not_muddy() {
        // 2x2: two black, two white texels. Linear average is 0.5, whose sRGB
        // encoding is ~188; a naive byte average would give 128.
        let input = [0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255];
        let levels = generate(2, 2, &input);
        let last = levels.last().unwrap();
        assert_eq!((last.width, last.height), (1, 1));
        assert!(
            (last.rgba[0] as i32 - 188).abs() <= 2,
            "linear-light average: {}",
            last.rgba[0]
        );
    }

    #[test]
    fn alpha_is_averaged() {
        let input = [255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let levels = generate(2, 2, &input);
        let last = levels.last().unwrap();
        assert_eq!(last.rgba[3], 63);
    }
}
