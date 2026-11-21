//! CPU rasteriser for the real `UiBatch`, used by the headless HUD proofs.
//!
//! The device path is wgpu; this module mirrors the same painter order
//! (`textured` -> `white` -> `font` -> `minimap`) so a PNG proves what the
//! composer emitted without a GPU. Sampling is nearest (deterministic across
//! Termux/Android); the GPU uses linear filtering, which changes edges only,
//! never geometry.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::assets::TextureImage;
use crate::hud::{self, Font, UiBatch};

/// Temp-file counter: two writes to the same path in one process never share a
/// scratch name, even from different threads.
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

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
///
/// The write is **atomic**: the bytes go to a hidden sibling scratch file in
/// the destination directory and are `rename`d into place only after a
/// complete, synced write. A process killed mid-render therefore leaves the
/// previous artifact intact instead of a 0-byte file — the exact artefact the
/// QA sweep found (`analysis/hudskin/screens/portrait-1080x2340-tooltip-
/// 1080x2340.png`, 0 bytes, 2026-11-21 12:58). A failed write removes the
/// scratch file and returns the error; the destination is never truncated.
pub fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> std::io::Result<()> {
    assert_eq!(rgba.len(), (width * height * 4) as usize);
    write_atomic(path, &encode_png(width, height, rgba))
}

/// Encoded PNG bytes for an RGBA8 frame (the buffer [`write_png`] stores).
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
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
    png
}

/// Hidden scratch path next to `path`, unique per process and per call.
fn scratch_path(path: &Path) -> PathBuf {
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "frame.png".to_string());
    let name = format!(".{file_name}.tmp-{}-{seq}", std::process::id());
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(name),
        _ => PathBuf::from(name),
    }
}

/// Complete-then-rename write: the destination only ever holds a full file.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let scratch = scratch_path(path);
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&scratch)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&scratch, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&scratch);
    }
    result
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// One parsed PNG: dimensions plus a CRC verdict over every chunk.
    #[derive(Debug, PartialEq, Eq)]
    struct ParsedPng {
        width: u32,
        height: u32,
        chunks: usize,
        crc_ok: bool,
        iend_last: bool,
    }

    fn parse_png(bytes: &[u8]) -> Option<ParsedPng> {
        const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        if bytes.len() < SIGNATURE.len() || bytes[..8] != SIGNATURE {
            return None;
        }
        let mut offset = 8usize;
        let mut width = 0u32;
        let mut height = 0u32;
        let mut chunks = 0usize;
        let mut crc_ok = true;
        let mut iend_last = false;
        while offset + 12 <= bytes.len() {
            let len = u32::from_be_bytes(bytes[offset..offset + 4].try_into().ok()?) as usize;
            let kind = &bytes[offset + 4..offset + 8];
            let data_end = offset + 8 + len;
            if data_end + 4 > bytes.len() {
                return None;
            }
            let data = &bytes[offset + 8..data_end];
            let stored = u32::from_be_bytes(bytes[data_end..data_end + 4].try_into().ok()?);
            let mut crc_input = Vec::with_capacity(4 + len);
            crc_input.extend_from_slice(kind);
            crc_input.extend_from_slice(data);
            if crc32(&crc_input) != stored {
                crc_ok = false;
            }
            chunks += 1;
            if kind == b"IHDR" && len >= 8 {
                width = u32::from_be_bytes(data[0..4].try_into().ok()?);
                height = u32::from_be_bytes(data[4..8].try_into().ok()?);
            }
            if kind == b"IEND" {
                iend_last = data_end + 4 == bytes.len();
            }
            offset = data_end + 4;
        }
        Some(ParsedPng {
            width,
            height,
            chunks,
            crc_ok,
            iend_last,
        })
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "h2-soft-raster-{tag}-{}-{}",
            std::process::id(),
            TEMP_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn frame(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
        color
            .iter()
            .copied()
            .cycle()
            .take((width * height * 4) as usize)
            .collect()
    }

    #[test]
    fn soft_raster_write_png_is_atomic_and_leaves_no_scratch_file() {
        let dir = scratch_dir("atomic");
        let path = dir.join("frame.png");
        let red = frame(5, 3, [255, 0, 0, 255]);
        write_png(&path, 5, 3, &red).expect("first write");

        let first = std::fs::read(&path).expect("read");
        let parsed = parse_png(&first).expect("valid PNG");
        assert_eq!((parsed.width, parsed.height), (5, 3), "{parsed:?}");
        assert!(parsed.crc_ok, "every chunk CRC must verify: {parsed:?}");
        assert!(parsed.iend_last, "IEND must close the file: {parsed:?}");
        assert_eq!(parsed.chunks, 3, "IHDR + IDAT + IEND");
        assert!(!first.is_empty());

        // A second write replaces the file wholesale, never appends.
        let blue = frame(5, 3, [0, 0, 255, 255]);
        write_png(&path, 5, 3, &blue).expect("second write");
        let second = std::fs::read(&path).expect("read");
        assert_ne!(first, second, "the 5x3 frame content changed");
        assert_eq!(
            parse_png(&second).map(|png| (png.width, png.height)),
            Some((5, 3))
        );

        let leftovers: Vec<String> = std::fs::read_dir(&dir)
            .expect("list")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "scratch files left behind: {leftovers:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn soft_raster_write_png_failure_keeps_the_previous_artifact() {
        let dir = scratch_dir("fail");
        let path = dir.join("frame.png");
        write_png(&path, 4, 4, &frame(4, 4, [10, 20, 30, 255])).expect("baseline");
        let baseline = std::fs::read(&path).expect("read baseline");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let original = std::fs::metadata(&dir).expect("dir meta").permissions();
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500))
                .expect("chmod read-only");
            // A process that can still create files ignores the mode (root); the
            // failure branch is then untestable and is skipped explicitly.
            let probe = dir.join(".probe");
            let enforced = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&probe)
                .is_err();
            if enforced {
                let error = write_png(&path, 4, 4, &frame(4, 4, [200, 0, 0, 255]))
                    .expect_err("a read-only directory must fail the write");
                assert!(
                    error.kind() == std::io::ErrorKind::PermissionDenied
                        || error.kind() == std::io::ErrorKind::AlreadyExists,
                    "unexpected error kind: {error:?}"
                );
                let after = std::fs::read(&path).expect("read after failure");
                assert_eq!(after, baseline, "a failed write must not touch the artifact");
                assert!(parse_png(&after).is_some(), "the old artifact is still a PNG");
            } else {
                eprintln!("skip: uid can write a 0o500 directory (root), failure branch untested");
                let _ = std::fs::remove_file(&probe);
            }
            std::fs::set_permissions(&dir, original).expect("restore mode");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn soft_raster_encode_png_states_the_exact_dimensions() {
        for (width, height) in [(1_u32, 1_u32), (3, 5), (16, 9)] {
            let bytes = encode_png(width, height, &frame(width, height, [1, 2, 3, 255]));
            let parsed = parse_png(&bytes).expect("valid PNG");
            assert_eq!((parsed.width, parsed.height), (width, height));
            assert!(parsed.crc_ok && parsed.iend_last);
        }
    }
}
