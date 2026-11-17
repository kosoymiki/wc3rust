//! BLP1/BLP2 texture decoder.
//!
//! BLP1 classic textures are either paletted or JPEG-compressed; BLP2 adds
//! paletted, DXT1/3/5 and BGRA variants. Output is tightly packed RGBA8 for the
//! first mip; every stored mip is decoded as well.

use crate::FormatError;

#[derive(Debug, Clone)]
pub struct MipLevel {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct BlpImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub mipmaps: Vec<MipLevel>,
}

pub fn decode(bytes: &[u8]) -> Result<BlpImage, FormatError> {
    if bytes.len() < 4 {
        return Err(FormatError::TooShort);
    }
    match &bytes[..4] {
        b"BLP1" => decode_blp1(bytes),
        b"BLP2" => decode_blp2(bytes),
        other => Err(FormatError::UnknownMagic(
            (*other).try_into().map_err(|_| FormatError::TooShort)?,
        )),
    }
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, FormatError> {
    let end = offset.checked_add(4).ok_or(FormatError::TooShort)?;
    bytes
        .get(offset..end)
        .and_then(|slice| slice.first_chunk::<4>().copied())
        .map(u32::from_le_bytes)
        .ok_or(FormatError::TooShort)
}

/// Hard ceiling on a decoded texture: 16 M px = 64 MiB RGBA. Classic WC3
/// textures are 512² and Reforged 2048², so a header above this is corrupt and
/// must not drive an allocation from a map file.
const MAX_PIXELS: u64 = 16 * 1024 * 1024;

fn check_pixels(width: u32, height: u32) -> Result<(), FormatError> {
    let pixels = width as u64 * height as u64;
    if pixels == 0 || pixels > MAX_PIXELS {
        return Err(FormatError::Corrupt("BLP dimensions out of range"));
    }
    Ok(())
}

fn decode_blp1(bytes: &[u8]) -> Result<BlpImage, FormatError> {
    if bytes.len() < 156 {
        return Err(FormatError::TooShort);
    }
    let content_type = u32_at(bytes, 4)?;
    let alpha_bits = u32_at(bytes, 8)?;
    let width = u32_at(bytes, 12)?;
    let height = u32_at(bytes, 16)?;
    if width == 0 || height == 0 || width > 8192 || height > 8192 {
        return Err(FormatError::Corrupt("invalid BLP1 dimensions"));
    }
    check_pixels(width, height)?;
    let mut offsets = [0u32; 16];
    let mut sizes = [0u32; 16];
    for i in 0..16 {
        offsets[i] = u32_at(bytes, 28 + i * 4)?;
        sizes[i] = u32_at(bytes, 92 + i * 4)?;
    }
    let pos = 156usize;
    if content_type == 0 {
        // JPEG: u32 header size, JPEG header, then one scan per mip. Every level
        // is decoded with the shared header (War3Net `GetJpegFileBytes(mip)`).
        let header_size = u32_at(bytes, pos)? as usize;
        let header_start = pos + 4;
        let header = bytes
            .get(header_start..header_start + header_size)
            .ok_or(FormatError::Corrupt("BLP1 JPEG header truncated"))?;
        let mut mipmaps = Vec::new();
        let mut mip_width = width;
        let mut mip_height = height;
        for level in 0..16 {
            if offsets[level] == 0 || sizes[level] == 0 {
                break;
            }
            let start = offsets[level] as usize;
            let size = sizes[level] as usize;
            let data = bytes
                .get(start..start + size)
                .ok_or(FormatError::Corrupt("BLP1 JPEG mip truncated"))?;
            let mut stream = Vec::with_capacity(header_size + size);
            stream.extend_from_slice(header);
            stream.extend_from_slice(data);
            let image = decode_jpeg_stream(&stream, mip_width, mip_height, alpha_bits)?;
            let level_image = image
                .mipmaps
                .into_iter()
                .next()
                .ok_or(FormatError::Corrupt("BLP1 JPEG mip has no pixels"))?;
            mipmaps.push(level_image);
            mip_width = (mip_width / 2).max(1);
            mip_height = (mip_height / 2).max(1);
        }
        let first = mipmaps
            .first()
            .cloned()
            .ok_or(FormatError::Corrupt("BLP1 JPEG has no mips"))?;
        Ok(BlpImage {
            width,
            height,
            rgba: first.rgba,
            mipmaps,
        })
    } else if content_type == 1 {
        // Paletted: 256 BGRA colors, then per level (indices + alpha plane).
        let palette = bytes.get(pos..pos + 1024).ok_or(FormatError::TooShort)?;
        let mut mipmaps = Vec::new();
        let mut mip_width = width;
        let mut mip_height = height;
        for level in 0..16 {
            if offsets[level] == 0 || sizes[level] == 0 {
                break;
            }
            let count = mip_width as usize * mip_height as usize;
            let start = offsets[level] as usize;
            let data = bytes
                .get(start..start + count)
                .ok_or(FormatError::Corrupt("BLP1 pixel data truncated"))?;
            let alpha_start = start + count;
            let alpha_len = (count * alpha_bits as usize).div_ceil(8);
            let alpha = bytes
                .get(alpha_start..alpha_start + alpha_len)
                .unwrap_or(&[]);
            let mut rgba = Vec::with_capacity(count * 4);
            for (index, &palette_index) in data.iter().enumerate() {
                let p = palette_index as usize * 4;
                let a = match alpha_bits {
                    8 => alpha.get(index).copied().unwrap_or(255),
                    4 => {
                        let byte = alpha.get(index / 2).copied().unwrap_or(0);
                        let nibble = if index % 2 == 0 {
                            byte & 0x0F
                        } else {
                            byte >> 4
                        };
                        nibble * 17
                    }
                    1 => {
                        let byte = alpha.get(index / 8).copied().unwrap_or(0);
                        if byte & (1 << (index % 8)) != 0 {
                            255
                        } else {
                            0
                        }
                    }
                    _ => 255,
                };
                rgba.extend_from_slice(&[
                    palette.get(p + 2).copied().unwrap_or(0),
                    palette.get(p + 1).copied().unwrap_or(0),
                    palette.get(p).copied().unwrap_or(0),
                    a,
                ]);
            }
            mipmaps.push(MipLevel {
                width: mip_width,
                height: mip_height,
                rgba,
            });
            mip_width = (mip_width / 2).max(1);
            mip_height = (mip_height / 2).max(1);
        }
        let first = mipmaps
            .first()
            .cloned()
            .ok_or(FormatError::Corrupt("BLP1 paletted has no mips"))?;
        Ok(BlpImage {
            width,
            height,
            rgba: first.rgba,
            mipmaps,
        })
    } else {
        Err(FormatError::Unsupported("BLP1 content type"))
    }
}

fn decode_blp2(bytes: &[u8]) -> Result<BlpImage, FormatError> {
    if bytes.len() < 148 {
        return Err(FormatError::TooShort);
    }
    let _version = u32_at(bytes, 4)?;
    let header: [u8; 12] = bytes
        .first_chunk::<12>()
        .copied()
        .ok_or(FormatError::TooShort)?;
    let compression = header[8];
    let alpha_bits = header[9];
    let alpha_type = header[10];
    let width = u32_at(bytes, 12)?;
    let height = u32_at(bytes, 16)?;
    if width == 0 || height == 0 || width > 8192 || height > 8192 {
        return Err(FormatError::Corrupt("invalid BLP2 dimensions"));
    }
    check_pixels(width, height)?;
    let mut offsets = [0u32; 16];
    let mut sizes = [0u32; 16];
    for i in 0..16 {
        offsets[i] = u32_at(bytes, 20 + i * 4)?;
        sizes[i] = u32_at(bytes, 84 + i * 4)?;
    }
    match compression {
        1 => {
            let palette = bytes.get(148..148 + 1024).ok_or(FormatError::TooShort)?;
            let alpha_start = 148 + 1024;
            let alpha = bytes
                .get(alpha_start..alpha_start + 1024)
                .ok_or(FormatError::TooShort)?;
            decode_paletted_mips(bytes, &offsets, &sizes, width, height, palette, alpha, true)
        }
        2 => {
            let mut mipmaps = Vec::new();
            let mut w = width;
            let mut h = height;
            for i in 0..16 {
                if offsets[i] == 0 || sizes[i] == 0 {
                    break;
                }
                let start = offsets[i] as usize;
                let end = start + sizes[i] as usize;
                let data = bytes
                    .get(start..end)
                    .ok_or(FormatError::Corrupt("DXT mip out of bounds"))?;
                let rgba = match alpha_bits {
                    0 | 1 if alpha_type == 0 => decode_dxt(data, w, h, DxtKind::Dxt1Opaque)?,
                    1 => decode_dxt(data, w, h, DxtKind::Dxt1Alpha)?,
                    3 | 4 | 5 => decode_dxt(data, w, h, DxtKind::Dxt5)?,
                    8 => decode_dxt(data, w, h, DxtKind::Dxt3)?,
                    _ => decode_dxt(data, w, h, DxtKind::Dxt5)?,
                };
                mipmaps.push(MipLevel {
                    width: w,
                    height: h,
                    rgba,
                });
                w = (w / 2).max(1);
                h = (h / 2).max(1);
            }
            let first = mipmaps
                .first()
                .ok_or(FormatError::Corrupt("no BLP2 mip"))?
                .clone();
            Ok(BlpImage {
                width,
                height,
                rgba: first.rgba,
                mipmaps,
            })
        }
        3 => {
            let start = offsets[0] as usize;
            let end = start + (width as usize * height as usize * 4);
            let data = bytes.get(start..end).ok_or(FormatError::TooShort)?;
            let mut rgba = Vec::with_capacity(data.len());
            for px in data.chunks_exact(4) {
                rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
            }
            Ok(BlpImage {
                width,
                height,
                rgba: rgba.clone(),
                mipmaps: vec![MipLevel {
                    width,
                    height,
                    rgba,
                }],
            })
        }
        other => Err(FormatError::Unsupported(if other == 0 {
            "BLP2 JPEG"
        } else {
            "unknown BLP2 compression"
        })),
    }
}

fn decode_paletted_mips(
    bytes: &[u8],
    offsets: &[u32; 16],
    sizes: &[u32; 16],
    width: u32,
    height: u32,
    palette: &[u8],
    alpha: &[u8],
    _blp2: bool,
) -> Result<BlpImage, FormatError> {
    let mut mipmaps = Vec::new();
    let mut w = width;
    let mut h = height;
    for i in 0..16 {
        if offsets[i] == 0 || sizes[i] == 0 {
            break;
        }
        let start = offsets[i] as usize;
        let size = sizes[i] as usize;
        let data = bytes
            .get(start..start + size)
            .ok_or(FormatError::Corrupt("paletted mip out of bounds"))?;
        let count = (w as usize) * (h as usize);
        let mut rgba = Vec::with_capacity(count * 4);
        for &index in data.iter().take(count) {
            let p = index as usize * 4;
            let a = alpha.get(index as usize * 4).copied().unwrap_or(255);
            rgba.extend_from_slice(&[
                palette.get(p).copied().unwrap_or(0),
                palette.get(p + 1).copied().unwrap_or(0),
                palette.get(p + 2).copied().unwrap_or(0),
                a,
            ]);
        }
        if rgba.len() < count * 4 {
            rgba.resize(count * 4, 0);
        }
        mipmaps.push(MipLevel {
            width: w,
            height: h,
            rgba,
        });
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    let first = mipmaps
        .first()
        .ok_or(FormatError::Corrupt("no paletted mip"))?
        .clone();
    Ok(BlpImage {
        width,
        height,
        rgba: first.rgba,
        mipmaps,
    })
}

/// Warcraft III BLP1 JPEG stores the four JPEG components as **RGB + inverted
/// alpha**: C/M/Y already hold the inverted (display) colour channels, i.e.
/// they are R/G/B, and K is `255 - alpha`. The previous `255 - c` conversion
/// inverted every texture (dirt decoded as teal, the Footman as dark blue) and
/// swapped nothing; donor Warsmash `BlpDecoder.decodeJpeg` and HiveWE's
/// turbojpeg `TJPF_CMYK` output both feed these components to RGBA directly.
fn cmyk_pixel(c: u8, m: u8, y: u8, k: u8, alpha_bits: u32) -> [u8; 4] {
    let a = if alpha_bits > 0 { 255 - k } else { 255 };
    // The four WC3 JPEG components are inverted CMY in BGR order: the stored
    // C/M/Y bytes are 255-B, 255-G, 255-R. Empirically this is the only mapping
    // that renders both Village_Dirt as brown AND Village_GrassShort as green;
    // direct RGB turns dirt brown but grass pink, plain inversion turns grass
    // green but dirt teal. HiveWE's turbojpeg `TJPF_CMYK` output has the same
    // "actually BGRA" note, and Warsmash swaps components 2/1/0.
    [255 - y, 255 - m, 255 - c, a]
}

fn decode_jpeg_stream(
    stream: &[u8],
    width: u32,
    height: u32,
    alpha_bits: u32,
) -> Result<BlpImage, FormatError> {
    let mut decoder = jpeg_decoder::Decoder::new(stream);
    let pixels = decoder
        .decode()
        .map_err(|_| FormatError::Corrupt("JPEG decode failed"))?;
    let info = decoder
        .info()
        .ok_or(FormatError::Corrupt("JPEG info missing"))?;
    if info.width == 0 || info.height == 0 || info.width > 8192 || info.height > 8192 {
        return Err(FormatError::Corrupt("implausible JPEG dimensions"));
    }
    let mut rgba = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => {
            let mut out = Vec::with_capacity(width as usize * height as usize * 4);
            for px in pixels.chunks_exact(3) {
                out.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            out
        }
        jpeg_decoder::PixelFormat::L8 => {
            let mut out = Vec::with_capacity(width as usize * height as usize * 4);
            for &v in &pixels {
                out.extend_from_slice(&[v, v, v, 255]);
            }
            out
        }
        jpeg_decoder::PixelFormat::CMYK32 => {
            // Warcraft III stores BLP1 JPEG art as CMYK where C/M/Y are the
            // inverted colour channels and K is the alpha mask (not a black
            // key). Treating K as black is what turned terrain into a black
            // mess with grass-shaped holes.
            let mut out = Vec::with_capacity(pixels.len());
            for px in pixels.chunks_exact(4) {
                out.extend_from_slice(&cmyk_pixel(px[0], px[1], px[2], px[3], alpha_bits));
            }
            out
        }
        _ => return Err(FormatError::Unsupported("JPEG pixel format")),
    };
    let expected = width as usize * height as usize * 4;
    if rgba.len() < expected {
        rgba.resize(expected, 0);
    } else if rgba.len() > expected {
        rgba.truncate(expected);
    }
    Ok(BlpImage {
        width,
        height,
        rgba: rgba.clone(),
        mipmaps: vec![MipLevel {
            width,
            height,
            rgba,
        }],
    })
}

#[derive(Clone, Copy)]
enum DxtKind {
    Dxt1Opaque,
    Dxt1Alpha,
    Dxt3,
    Dxt5,
}

fn rgb565(c: u16) -> [u8; 3] {
    let r = ((c >> 11) & 0x1F) as u32;
    let g = ((c >> 5) & 0x3F) as u32;
    let b = (c & 0x1F) as u32;
    [
        ((r * 255 + 15) / 31) as u8,
        ((g * 255 + 31) / 63) as u8,
        ((b * 255 + 15) / 31) as u8,
    ]
}

fn decode_dxt(data: &[u8], width: u32, height: u32, kind: DxtKind) -> Result<Vec<u8>, FormatError> {
    let block_size = match kind {
        DxtKind::Dxt1Opaque | DxtKind::Dxt1Alpha => 8,
        DxtKind::Dxt3 | DxtKind::Dxt5 => 16,
    };
    let bw = width.div_ceil(4) as usize;
    let bh = height.div_ceil(4) as usize;
    let needed = bw * bh * block_size;
    if data.len() < needed {
        return Err(FormatError::Corrupt("DXT payload shorter than blocks"));
    }
    let mut out = vec![0u8; width as usize * height as usize * 4];
    let mut offset = 0usize;
    for by in 0..bh {
        for bx in 0..bw {
            let block = data
                .get(offset..offset + block_size)
                .ok_or(FormatError::Corrupt("DXT block out of bounds"))?;
            offset += block_size;
            let (alpha, color) = match kind {
                DxtKind::Dxt1Opaque | DxtKind::Dxt1Alpha => (&[][..], block),
                DxtKind::Dxt3 | DxtKind::Dxt5 => {
                    let (head, tail) = block.split_at(8);
                    (head, tail)
                }
            };
            let color: [u8; 8] = color
                .first_chunk::<8>()
                .copied()
                .ok_or(FormatError::Corrupt("DXT colour block truncated"))?;
            let c0 = u16::from_le_bytes([color[0], color[1]]);
            let c1 = u16::from_le_bytes([color[2], color[3]]);
            let indices = u32::from_le_bytes([color[4], color[5], color[6], color[7]]);
            let e0 = rgb565(c0);
            let e1 = rgb565(c1);
            let mut colors = [[0u8; 4]; 4];
            colors[0] = [e0[0], e0[1], e0[2], 255];
            colors[1] = [e1[0], e1[1], e1[2], 255];
            let four_color = c0 > c1 || !matches!(kind, DxtKind::Dxt1Opaque | DxtKind::Dxt1Alpha);
            if four_color {
                colors[2] = [
                    ((2 * e0[0] as u16 + e1[0] as u16) / 3) as u8,
                    ((2 * e0[1] as u16 + e1[1] as u16) / 3) as u8,
                    ((2 * e0[2] as u16 + e1[2] as u16) / 3) as u8,
                    255,
                ];
                colors[3] = [
                    ((e0[0] as u16 + 2 * e1[0] as u16) / 3) as u8,
                    ((e0[1] as u16 + 2 * e1[1] as u16) / 3) as u8,
                    ((e0[2] as u16 + 2 * e1[2] as u16) / 3) as u8,
                    255,
                ];
            } else {
                colors[2] = [
                    ((e0[0] as u16 + e1[0] as u16) / 2) as u8,
                    ((e0[1] as u16 + e1[1] as u16) / 2) as u8,
                    ((e0[2] as u16 + e1[2] as u16) / 2) as u8,
                    255,
                ];
                colors[3] = [0, 0, 0, 0];
            }
            let alpha_at = |index: usize| -> u8 {
                match kind {
                    DxtKind::Dxt1Opaque => 255,
                    DxtKind::Dxt1Alpha => 255,
                    DxtKind::Dxt3 => {
                        let byte = alpha[index / 2];
                        let nibble = if index % 2 == 0 {
                            byte & 0xF
                        } else {
                            byte >> 4
                        };
                        nibble * 17
                    }
                    DxtKind::Dxt5 => {
                        let a0 = alpha[0] as i32;
                        let a1 = alpha[1] as i32;
                        let mut bits = 0u64;
                        for (i, &b) in alpha[2..8].iter().enumerate() {
                            bits |= (b as u64) << (8 * i);
                        }
                        let code = ((bits >> (3 * index)) & 0x7) as i32;
                        let value = if code == 0 {
                            a0
                        } else if code == 1 {
                            a1
                        } else if a0 > a1 {
                            ((8 - code) * a0 + (code - 1) * a1) / 7
                        } else if code == 6 {
                            0
                        } else if code == 7 {
                            255
                        } else {
                            ((6 - code) * a0 + (code - 1) * a1) / 5
                        };
                        value.clamp(0, 255) as u8
                    }
                }
            };
            for py in 0..4 {
                for px in 0..4 {
                    let x = bx as u32 * 4 + px;
                    let y = by as u32 * 4 + py;
                    if x >= width || y >= height {
                        continue;
                    }
                    let index = ((py * 4 + px) as u32) as usize;
                    let color_index = ((indices >> (2 * index)) & 0x3) as usize;
                    let mut rgba = colors[color_index];
                    if matches!(kind, DxtKind::Dxt1Alpha) && color_index == 3 && c0 <= c1 {
                        rgba = [0, 0, 0, 0];
                    } else if !matches!(kind, DxtKind::Dxt1Opaque | DxtKind::Dxt1Alpha) {
                        rgba[3] = alpha_at(index);
                    }
                    let dest = ((y as usize) * width as usize + x as usize) * 4;
                    out[dest..dest + 4].copy_from_slice(&rgba);
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture(rel: &str) -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_samples")
            .join(rel);
        fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    #[test]
    fn decodes_real_footman_texture() {
        let bytes = fixture("Textures/Footman.blp");
        let image = decode(&bytes).expect("decode BLP1");
        assert_eq!((image.width, image.height), (256, 256));
        assert_eq!(image.rgba.len(), 256 * 256 * 4);
        assert!(
            image
                .rgba
                .chunks_exact(4)
                .any(|px| px[0] > 32 || px[1] > 32 || px[2] > 32)
        );
        assert!(!image.mipmaps.is_empty());
    }

    #[test]
    fn decodes_solid_dxt1_block() {
        // One 4x4 DXT1 block: c0 = pure green, c1 = black, all indices 0.
        let mut block = Vec::new();
        block.extend_from_slice(&0x07E0u16.to_le_bytes());
        block.extend_from_slice(&0u16.to_le_bytes());
        block.extend_from_slice(&0u32.to_le_bytes());
        let rgba = decode_dxt(&block, 4, 4, DxtKind::Dxt1Opaque).unwrap();
        assert_eq!(rgba.len(), 64);
        assert_eq!(&rgba[..4], &[0, 255, 0, 255]);
    }

    #[test]
    fn rejects_short_input() {
        assert!(matches!(decode(b"BLP1"), Err(FormatError::TooShort)));
        assert!(matches!(decode(b"XXXX"), Err(FormatError::UnknownMagic(_))));
    }
}


#[cfg(test)]
mod cmyk_tests {
    use super::cmyk_pixel;

    #[test]
    fn blp_cmyk_uses_k_as_alpha_not_black() {
        // Real Village_GrassShort pixel: C=214 M=152 Y=186 K=255.
        let px = cmyk_pixel(214, 152, 186, 255, 8);
        assert_eq!(&px[..3], &[69, 103, 41], "RGB from inverted BGR components");
        assert_eq!(px[3], 0, "K=255 is transparent, not black");
        let grass = cmyk_pixel(240, 204, 252, 0, 8);
        assert_eq!(&grass[..3], &[3, 51, 15]);
        assert_eq!(grass[3], 255);
    }

    #[test]
    fn blp_cmyk_without_alpha_is_opaque() {
        let px = cmyk_pixel(10, 20, 30, 200, 0);
        assert_eq!(px[3], 255);
    }
}
