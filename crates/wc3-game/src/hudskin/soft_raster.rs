//! CPU rasteriser for the real `UiBatch`, used by the headless HUD proofs.
//!
//! The device path is wgpu; this module mirrors the same painter order
//! (`textured` -> `white` -> `font` -> `minimap`) so a PNG proves what the
//! composer emitted without a GPU. Sampling is nearest (deterministic across
//! Termux/Android); the GPU uses linear filtering, which changes edges only,
//! never geometry.

use std::io::Write;
use std::path::Path;

use crate::assets::TextureImage;
use crate::hud::{self, Font, UiBatch};

/// Rasterise one HUD batch onto a dark vertical-gradient playfield.
pub fn rasterize(
    batch: &UiBatch,
    textures: &[TextureImage],
    font: Option<&Font>,
    minimap: Option<&TextureImage>,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut canvas = Canvas::new(width, height);
    canvas.background();
    for draw in &batch.textured {
        if let Some(texture) = textures.get(draw.texture) {
            for chunk in draw.vertices.chunks_exact(6) {
                canvas.textured_quad(chunk, texture, false);
            }
        }
    }
    for chunk in batch.white.chunks_exact(6) {
        canvas.solid_quad(chunk);
    }
    if let Some(font) = font {
        for chunk in batch.font.chunks_exact(6) {
            canvas.textured_quad(chunk, &font.atlas, true);
        }
    }
    if let Some(minimap) = minimap {
        for chunk in batch.minimap.chunks_exact(6) {
            canvas.textured_quad(chunk, minimap, false);
        }
    }
    canvas.rgba
}

/// Write an RGBA8 buffer as PNG (stored deflate blocks: no extra dependency,
/// byte-stable across hosts).
pub fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> std::io::Result<()> {
    assert_eq!(rgba.len(), (width * height * 4) as usize);
    let mut raw = Vec::with_capacity(((width * 4 + 1) * height) as usize);
    for row in 0..height {
        raw.push(0);
        let start = (row * width * 4) as usize;
        raw.extend_from_slice(&rgba[start..start + (width * 4) as usize]);
    }
    let mut zlib = vec![0x78, 0x01];
    let mut offset = 0usize;
    while offset < raw.len() {
        let end = (offset + 65535).min(raw.len());
        let block = &raw[offset..end];
        zlib.push(if end == raw.len() { 1 } else { 0 });
        zlib.extend_from_slice(&(block.len() as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        zlib.extend_from_slice(block);
        offset = end;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &zlib);
    chunk(&mut png, b"IEND", &[]);
    let mut file = std::fs::File::create(path)?;
    file.write_all(&png)
}

/// Linear vertex colour -> sRGB bytes, matching the `Rgba8UnormSrgb` surface.
fn encode(color: [f32; 4]) -> [f32; 4] {
    [
        hud::linear_to_srgb(color[0].clamp(0.0, 1.0)),
        hud::linear_to_srgb(color[1].clamp(0.0, 1.0)),
        hud::linear_to_srgb(color[2].clamp(0.0, 1.0)),
        color[3],
    ]
}

struct Canvas {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Canvas {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: vec![0u8; (width * height * 4) as usize],
        }
    }

    /// Neutral dark playfield: the proof is about HUD art, not a mock map.
    fn background(&mut self) {
        for y in 0..self.height {
            let t = y as f32 / self.height.max(1) as f32;
            let shade = 0.055 + t * 0.035;
            for x in 0..self.width {
                let index = ((y * self.width + x) * 4) as usize;
                self.rgba[index] = (shade * 0.75 * 255.0) as u8;
                self.rgba[index + 1] = (shade * 0.85 * 255.0) as u8;
                self.rgba[index + 2] = (shade * 0.70 * 255.0) as u8;
                self.rgba[index + 3] = 255;
            }
        }
    }

    fn blend(&mut self, x: i32, y: i32, color: [f32; 4]) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let alpha = color[3].clamp(0.0, 1.0);
        if alpha <= 0.001 {
            return;
        }
        let index = ((y as u32 * self.width + x as u32) * 4) as usize;
        for channel in 0..3 {
            let dst = self.rgba[index + channel] as f32 / 255.0;
            let out = color[channel] * alpha + dst * (1.0 - alpha);
            self.rgba[index + channel] = (out.clamp(0.0, 1.0) * 255.0) as u8;
        }
        self.rgba[index + 3] = 255;
    }

    fn solid_quad(&mut self, vertices: &[hud::UiVertex]) {
        let color = encode(vertices[0].color);
        let (x0, y0, x1, y1) = bounds(vertices);
        for y in y0.floor() as i32..y1.ceil() as i32 {
            for x in x0.floor() as i32..x1.ceil() as i32 {
                self.blend(x, y, color);
            }
        }
    }

    fn textured_quad(&mut self, vertices: &[hud::UiVertex], texture: &TextureImage, font: bool) {
        let (x0, y0, x1, y1) = bounds(vertices);
        if x1 <= x0 || y1 <= y0 || texture.width == 0 || texture.height == 0 {
            return;
        }
        let v0 = vertices[0];
        let v2 = vertices[2];
        let color = encode(v0.color);
        for y in y0.floor() as i32..y1.ceil() as i32 {
            let fy = ((y as f32 + 0.5 - y0) / (y1 - y0)).clamp(0.0, 1.0);
            for x in x0.floor() as i32..x1.ceil() as i32 {
                let fx = ((x as f32 + 0.5 - x0) / (x1 - x0)).clamp(0.0, 1.0);
                let u = v0.uv[0] + (v2.uv[0] - v0.uv[0]) * fx;
                let v = v0.uv[1] + (v2.uv[1] - v0.uv[1]) * fy;
                let tx = (u * texture.width as f32)
                    .floor()
                    .clamp(0.0, texture.width as f32 - 1.0) as u32;
                let ty = (v * texture.height as f32)
                    .floor()
                    .clamp(0.0, texture.height as f32 - 1.0) as u32;
                let index = ((ty * texture.width + tx) * 4) as usize;
                let texel = [
                    texture.rgba[index] as f32 / 255.0,
                    texture.rgba[index + 1] as f32 / 255.0,
                    texture.rgba[index + 2] as f32 / 255.0,
                    texture.rgba[index + 3] as f32 / 255.0,
                ];
                let out = if font {
                    [color[0], color[1], color[2], color[3] * texel[3]]
                } else {
                    [
                        color[0] * texel[0],
                        color[1] * texel[1],
                        color[2] * texel[2],
                        color[3] * texel[3],
                    ]
                };
                self.blend(x, y, out);
            }
        }
    }
}

fn bounds(vertices: &[hud::UiVertex]) -> (f32, f32, f32, f32) {
    let mut x0 = f32::INFINITY;
    let mut y0 = f32::INFINITY;
    let mut x1 = f32::NEG_INFINITY;
    let mut y1 = f32::NEG_INFINITY;
    for vertex in vertices {
        x0 = x0.min(vertex.pos[0]);
        y0 = y0.min(vertex.pos[1]);
        x1 = x1.max(vertex.pos[0]);
        y1 = y1.max(vertex.pos[1]);
    }
    (x0, y0, x1, y1)
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = if crc & 1 != 0 { 0xEDB8_8320 } else { 0 };
            crc = (crc >> 1) ^ mask;
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let mut a = 1u32;
    let mut b = 0u32;
    for byte in data {
        a = (a + *byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}
