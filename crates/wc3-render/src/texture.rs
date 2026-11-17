//! Texture pipeline: sizing policy, sRGB-correct mip chains and GPU accounting.
//!
//! Three rules drive this module.
//!
//! 1. **Never upscale.** WC3 model textures are authored at 64x64, 128x128,
//!    256x256 or 512x512; forcing every one to 256x256 (as the baseline does)
//!    multiplies video memory for small textures and turns 8x8 icons into
//!    blurry 256x256 uploads. [`fit`] downsamples only when a texture exceeds the
//!    budget and leaves everything else at its authored size.
//! 2. **Mips are generated in linear light.** Filtering an sRGB byte texture
//!    directly darkens it; [`MipChain`] decodes with a 256-entry LUT, box-filters
//!    and re-encodes through a 16-bit LUT, so a 256x256 chain costs ~3 ms instead
//!    of ~300 ms of `powf` while staying within one 8-bit step of the exact
//!    formula (see `mip_matches_powf_reference`).
//! 3. **Formats follow meaning.** Colour textures are `Rgba8UnormSrgb`, coverage
//!    (font atlas) is `Rgba8Unorm`, so the sampler does not gamma-shift glyph
//!    edges. The renderer never guesses: the role is passed in.

use std::sync::OnceLock;

/// What a texture stores; decides sRGB-ness and filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Albedo/UI colour: sRGB-encoded, sampled and blended in linear space.
    Color,
    /// Coverage/alpha only (font atlas): linear storage, no gamma decode.
    Coverage,
}

impl Role {
    pub fn format(self) -> wgpu::TextureFormat {
        match self {
            Role::Color => wgpu::TextureFormat::Rgba8UnormSrgb,
            Role::Coverage => wgpu::TextureFormat::Rgba8Unorm,
        }
    }

    pub fn is_srgb(self) -> bool {
        matches!(self, Role::Color)
    }
}

/// Tightly packed RGBA8 image, the CPU-side currency of the asset loader.
#[derive(Clone, PartialEq, Eq)]
pub struct TextureImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl TextureImage {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            width: width.max(1),
            height: height.max(1),
            rgba,
        }
    }

    pub fn white() -> Self {
        Self::new(2, 2, vec![255; 16])
    }

    pub fn bytes(&self) -> usize {
        self.rgba.len()
    }

    /// Decoded bytes the CPU copy holds (same as [`TextureImage::bytes`]).
    pub fn cpu_bytes(&self) -> usize {
        self.rgba.len()
    }

    /// Drop the decoded RGBA, keeping the dimensions.
    ///
    /// Called once the renderer has uploaded the texture: the GPU copy is the
    /// only consumer after that, and the decoded sheet is up to 4 MiB each
    /// (1275 of them in the shipped install). Returns the bytes freed.
    pub fn release_cpu(&mut self) -> usize {
        let freed = self.rgba.len();
        // Replacing the buffer drops the old allocation; Android's scudo frees
        // blocks >= 64 KiB (every 128x128 sheet) back to the OS, so the RSS
        // really falls instead of being reused for the next texture.
        self.rgba = Vec::new();
        freed
    }

    /// `true` when the decoded pixels are gone (uploaded already).
    pub fn cpu_released(&self) -> bool {
        self.rgba.is_empty()
    }

    /// Fit into a `max`x`max` budget without ever upscaling.
    ///
    /// Returns the (possibly downsampled) image and whether it was resized.
    /// Sizes snap to powers of two because every WC3 texture and every mip chain
    /// is pow2, and non-pow2 mip levels waste a level on odd dimensions.
    pub fn fit(&self, max: u32) -> (Self, bool) {
        let max = max.max(1);
        let width = self.width.max(1);
        let height = self.height.max(1);
        if width <= max && height <= max {
            return (self.clone(), false);
        }
        let target_w = pow2_at_most(width.min(max));
        let target_h = pow2_at_most(height.min(max));
        (resize_box(self, target_w, target_h), true)
    }
}

/// Largest power of two `<= value`.
pub fn pow2_at_most(value: u32) -> u32 {
    let value = value.max(1);
    1u32 << (31 - value.leading_zeros())
}

/// Box-filter resize (area averaging). Never upscales: callers fit first.
pub fn resize_box(image: &TextureImage, width: u32, height: u32) -> TextureImage {
    let width = width.max(1);
    let height = height.max(1);
    let src_w = image.width.max(1);
    let src_h = image.height.max(1);
    if width == src_w && height == src_h {
        return image.clone();
    }
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        let sy0 = (y as u64 * src_h as u64 / height as u64) as u32;
        let sy1 = (((y + 1) as u64 * src_h as u64 / height as u64) as u32)
            .max(sy0 + 1)
            .min(src_h);
        for x in 0..width {
            let sx0 = (x as u64 * src_w as u64 / width as u64) as u32;
            let sx1 = (((x + 1) as u64 * src_w as u64 / width as u64) as u32)
                .max(sx0 + 1)
                .min(src_w);
            let mut acc = [0u32; 4];
            let mut count = 0u32;
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let index = ((sy * src_w + sx) * 4) as usize;
                    if index + 3 < image.rgba.len() {
                        for channel in 0..4 {
                            acc[channel] += image.rgba[index + channel] as u32;
                        }
                        count += 1;
                    }
                }
            }
            let dst = ((y * width + x) * 4) as usize;
            if count > 0 {
                for channel in 0..4 {
                    rgba[dst + channel] = (acc[channel] / count) as u8;
                }
            }
        }
    }
    TextureImage::new(width, height, rgba)
}

// ------------------------------------------------------------------ sRGB LUTs

pub(crate) fn decode_lut() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut lut = [0.0f32; 256];
        for (value, slot) in lut.iter_mut().enumerate() {
            let x = value as f32 / 255.0;
            *slot = if x >= 0.040_45 {
                ((x + 0.055) / 1.055).powf(2.4)
            } else {
                x / 12.92
            };
        }
        lut
    })
}

/// 16-bit linear -> 8-bit sRGB encoding table.
///
/// One byte per entry; built once. The 1/65536 quantisation is below one 8-bit
/// code value everywhere on the curve, so results match `powf` within 1 LSB.
fn encode_lut() -> &'static [u8; 65536] {
    static LUT: OnceLock<Box<[u8; 65536]>> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut table = Box::new([0u8; 65536]);
        for (index, slot) in table.iter_mut().enumerate() {
            let x = index as f32 / 65535.0;
            let srgb = if x >= 0.003_130_8 {
                1.055 * x.powf(1.0 / 2.4) - 0.055
            } else {
                12.92 * x
            };
            *slot = (srgb * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        table
    })
}

#[inline]
pub(crate) fn encode_srgb(linear: f32) -> u8 {
    let index = (linear.clamp(0.0, 1.0) * 65535.0) as usize;
    encode_lut()[index]
}

/// Exact reference encoder, used by the tests to police the LUT.
pub fn encode_srgb_reference(linear: f32) -> u8 {
    let x = linear.clamp(0.0, 1.0);
    let srgb = if x >= 0.003_130_8 {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    } else {
        12.92 * x
    };
    (srgb * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Exact reference decoder, used by the tests to police the LUT.
pub fn decode_srgb_reference(value: u8) -> f32 {
    let x = value as f32 / 255.0;
    if x >= 0.040_45 {
        ((x + 0.055) / 1.055).powf(2.4)
    } else {
        x / 12.92
    }
}

// ------------------------------------------------------------------ mip chain

/// One mip level.
#[derive(Clone)]
pub struct Mip {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Number of levels in a full chain for `width`x`height`.
pub fn level_count(width: u32, height: u32) -> u32 {
    let mut w = width.max(1);
    let mut h = height.max(1);
    let mut levels = 1;
    while w > 1 || h > 1 {
        w = (w / 2).max(1);
        h = (h / 2).max(1);
        levels += 1;
    }
    levels
}

/// Bytes a texture occupies including its mip chain, for `bpp` bits per pixel.
pub fn mip_bytes(width: u32, height: u32, bpp: u32) -> u64 {
    let mut w = width.max(1) as u64;
    let mut h = height.max(1) as u64;
    let mut total = 0u64;
    loop {
        total += w * h * bpp as u64 / 8;
        if w == 1 && h == 1 {
            break;
        }
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    total
}

/// Build the full mip chain of an RGBA8 image.
///
/// Colour channels average in linear light (no muddy mid-grey), alpha averages
/// linearly, which is the correct filter for coverage.
pub fn generate_mips(width: u32, height: u32, rgba: &[u8]) -> Vec<Mip> {
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
    let mut mips = Vec::with_capacity(level_count(width, height) as usize);
    mips.push(Mip {
        width,
        height,
        rgba: pixels.clone(),
    });
    let decode = decode_lut();
    let (mut w, mut h) = (width, height);
    while w > 1 || h > 1 {
        let nw = (w / 2).max(1);
        let nh = (h / 2).max(1);
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            let sy0 = (y * 2).min(h - 1);
            let sy1 = (sy0 + 1).min(h - 1);
            for x in 0..nw {
                let sx0 = (x * 2).min(w - 1);
                let sx1 = (sx0 + 1).min(w - 1);
                let i00 = ((sy0 * w + sx0) * 4) as usize;
                let i10 = ((sy0 * w + sx1) * 4) as usize;
                let i01 = ((sy1 * w + sx0) * 4) as usize;
                let i11 = ((sy1 * w + sx1) * 4) as usize;
                let dst = ((y * nw + x) * 4) as usize;
                for channel in 0..3 {
                    let linear = (decode[pixels[i00 + channel] as usize]
                        + decode[pixels[i10 + channel] as usize]
                        + decode[pixels[i01 + channel] as usize]
                        + decode[pixels[i11 + channel] as usize])
                        * 0.25;
                    next[dst + channel] = encode_srgb(linear);
                }
                let alpha = (pixels[i00 + 3] as u32
                    + pixels[i10 + 3] as u32
                    + pixels[i01 + 3] as u32
                    + pixels[i11 + 3] as u32)
                    / 4;
                next[dst + 3] = alpha as u8;
            }
        }
        pixels = next;
        w = nw;
        h = nh;
        mips.push(Mip {
            width: w,
            height: h,
            rgba: pixels.clone(),
        });
    }
    mips
}

// ------------------------------------------------------- compressed formats

/// Largest texture side accepted by the length helpers below
/// (`GL_MAX_TEXTURE_SIZE` on the supported Vulkan/GLES 3 adapters, same value as
/// `astc::MAX_TEXTURE_SIDE`). Compressed container headers store `u32`
/// dimensions, so every block-grid computation caps the side before casting;
/// see [`CompressedFormat::block_grid`].
pub const MAX_TEXTURE_SIDE: u32 = 16_384;

// The two caps must agree: `.astc` parsing and in-memory validation of the same
// texture may not disagree about which dimensions are legal.
const _: () = assert!(MAX_TEXTURE_SIDE == crate::astc::MAX_TEXTURE_SIDE);

/// A GPU-compressed texture format the renderer can sample.
///
/// Only the formats the base actually considers are listed: ASTC (the mobile
/// target's preferred format), ETC2 (the Vulkan 1.0 mandatory fallback) and BC
/// (desktop/CI only, never uploaded on the Adreno target).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressedFormat {
    /// ASTC LDR 6x6: 3.56 bpp, the quality/size compromise for models.
    Astc6x6,
    /// ASTC LDR 8x8: 2.0 bpp, for terrain/UI sheets where detail is not critical.
    Astc8x8,
    /// ETC2 RGBA8 (EAC alpha + ETC1 colour): 8 bpp, Vulkan-1.0 mandatory.
    Etc2Rgba8,
    /// ETC2 RGB: 4 bpp, no alpha.
    Etc2Rgb,
}

impl CompressedFormat {
    /// sRGB texture format; colour textures must be sampled as sRGB.
    pub fn srgb(self) -> wgpu::TextureFormat {
        match self {
            Self::Astc6x6 => wgpu::TextureFormat::Astc {
                block: wgpu::AstcBlock::B6x6,
                channel: wgpu::AstcChannel::UnormSrgb,
            },
            Self::Astc8x8 => wgpu::TextureFormat::Astc {
                block: wgpu::AstcBlock::B8x8,
                channel: wgpu::AstcChannel::UnormSrgb,
            },
            Self::Etc2Rgba8 => wgpu::TextureFormat::Etc2Rgba8UnormSrgb,
            Self::Etc2Rgb => wgpu::TextureFormat::Etc2Rgb8UnormSrgb,
        }
    }

    /// Linear (non-sRGB) variant, for coverage/data textures.
    pub fn linear(self) -> wgpu::TextureFormat {
        match self {
            Self::Astc6x6 => wgpu::TextureFormat::Astc {
                block: wgpu::AstcBlock::B6x6,
                channel: wgpu::AstcChannel::Unorm,
            },
            Self::Astc8x8 => wgpu::TextureFormat::Astc {
                block: wgpu::AstcBlock::B8x8,
                channel: wgpu::AstcChannel::Unorm,
            },
            Self::Etc2Rgba8 => wgpu::TextureFormat::Etc2Rgba8Unorm,
            Self::Etc2Rgb => wgpu::TextureFormat::Etc2Rgb8Unorm,
        }
    }

    /// `(width, height)` of one compressed block in texels.
    pub fn block(self) -> (u32, u32) {
        match self {
            Self::Astc6x6 => (6, 6),
            Self::Astc8x8 => (8, 8),
            _ => (4, 4),
        }
    }

    /// Bytes per compressed block.
    pub fn block_bytes(self) -> u32 {
        match self {
            Self::Astc6x6 | Self::Astc8x8 => 16,
            Self::Etc2Rgba8 => 16,
            Self::Etc2Rgb => 8,
        }
    }

    /// True when `width`x`height` is a whole number of blocks.
    ///
    /// WebGPU (and therefore wgpu) requires compressed textures to be block
    /// aligned; a 64x64 texture cannot be ASTC 6x6 (64 % 6 != 0). The asset
    /// pipeline pads to the next multiple and scales the UVs accordingly.
    pub fn accepts(self, width: u32, height: u32) -> bool {
        let (bw, bh) = self.block();
        width > 0 && height > 0 && width % bw == 0 && height % bh == 0
    }

    /// Rows of blocks for an image of `height` texels (blocks are always whole).
    ///
    /// `height` is clamped to [`MAX_TEXTURE_SIDE`]: no supported adapter holds a
    /// taller texture, and the clamp keeps the block arithmetic inside `u32`
    /// for every length helper that shares the cap (AUD-009-N1).
    pub fn block_rows(self, height: u32) -> u32 {
        height.min(MAX_TEXTURE_SIDE).div_ceil(self.block().1).max(1)
    }

    /// Row pitch in bytes for an image of `width` texels (2048-aligned, the
    /// WebGPU `Queue::write_texture` requirement for block-compressed data).
    ///
    /// `width` is clamped to [`MAX_TEXTURE_SIDE`]; see [`Self::block_rows`].
    pub fn row_pitch(self, width: u32) -> u32 {
        let row = width.min(MAX_TEXTURE_SIDE).div_ceil(self.block().0).max(1) * self.block_bytes();
        row.div_ceil(2048) * 2048
    }

    /// Bytes a full mip chain occupies (block padding included).
    pub fn chain_bytes(self, width: u32, height: u32, levels: u32) -> u64 {
        let mut w = width.max(1);
        let mut h = height.max(1);
        let mut total = 0u64;
        for level in 0..levels {
            let pitch = if level == 0 {
                self.row_pitch(w)
            } else {
                // Only the first level needs the 2048-byte alignment; later
                // levels are uploaded with their natural pitch.
                self.natural_pitch(w)
            };
            total += pitch as u64 * self.block_rows(h) as u64;
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }
        total
    }

    /// Natural (unaligned) row pitch in bytes.
    ///
    /// `width` is clamped to [`MAX_TEXTURE_SIDE`]; see [`Self::block_rows`].
    pub fn natural_pitch(self, width: u32) -> u32 {
        width.min(MAX_TEXTURE_SIDE).div_ceil(self.block().0).max(1) * self.block_bytes()
    }

    /// Block grid `(columns, rows)` for `width`x`height`, checked (AUD-009-N1).
    ///
    /// Single source of truth for compressed lengths, mirroring
    /// `astc::payload_len`: zero dimensions and sides above
    /// [`MAX_TEXTURE_SIDE`] are rejected, and the division is done in `u64` so
    /// a crafted `u32` dimension can never wrap the multiplication that
    /// follows. Rejecting (rather than clamping) is required here because
    /// [`CompressedLevel::is_consistent`] is the validator for external
    /// `.astc`/sidecar data.
    pub fn block_grid(self, width: u32, height: u32) -> Result<(u64, u64), String> {
        if width == 0 || height == 0 {
            return Err(format!("texture: zero dimension {width}x{height}"));
        }
        if width > MAX_TEXTURE_SIDE || height > MAX_TEXTURE_SIDE {
            return Err(format!(
                "texture: {width}x{height} exceeds the {MAX_TEXTURE_SIDE}-texel side limit"
            ));
        }
        let (bw, bh) = self.block();
        Ok((
            (width as u64).div_ceil(bw as u64),
            (height as u64).div_ceil(bh as u64),
        ))
    }
}

/// One mip level of compressed block data (raw, naturally pitched).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompressedLevel {
    pub width: u32,
    pub height: u32,
    /// `natural_pitch(width) * block_rows(height)` bytes, as produced by
    /// `astcenc` (raw `.astc` payload) or any other block encoder.
    pub data: Vec<u8>,
}

impl CompressedLevel {
    /// Bytes a `width`x`height` level occupies for `format`, checked.
    ///
    /// Every step is 64-bit and dimension-capped through
    /// [`CompressedFormat::block_grid`]; a crafted or stale `u32` size is an
    /// `Err`, never a wrapped length that would make an undersized payload look
    /// valid (AUD-009-N1).
    fn expected_bytes(format: CompressedFormat, width: u32, height: u32) -> Result<usize, String> {
        let (blocks_x, blocks_y) = format.block_grid(width, height)?;
        let bytes = blocks_x
            .checked_mul(blocks_y)
            .and_then(|blocks| blocks.checked_mul(format.block_bytes() as u64))
            .ok_or_else(|| {
                format!("texture: {width}x{height} block grid overflows the byte count")
            })?;
        usize::try_from(bytes)
            .map_err(|_| format!("texture: {bytes}-byte level does not fit in this address space"))
    }

    /// True when `data` matches the block layout for `width`x`height`.
    ///
    /// A level whose dimensions are impossible (zero, above
    /// [`MAX_TEXTURE_SIDE`], or not representable in `usize`) is inconsistent
    /// by definition, so this never panics on external data.
    pub fn is_consistent(&self, format: CompressedFormat) -> bool {
        Self::expected_bytes(format, self.width, self.height)
            .is_ok_and(|expected| self.data.len() == expected)
    }
}

/// A block-compressed image with its full mip chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompressedImage {
    pub format: CompressedFormat,
    pub width: u32,
    pub height: u32,
    pub levels: Vec<CompressedLevel>,
}

impl CompressedImage {
    /// Validate the block layout of every level (`astcenc` output, `.astc` file
    /// or an asset-pipeline sidecar). Rejects nothing silently.
    pub fn validate(&self) -> Result<(), String> {
        if self.levels.is_empty() {
            return Err("compressed image has no mip levels".into());
        }
        let base = &self.levels[0];
        if base.width != self.width || base.height != self.height {
            return Err(format!(
                "level 0 is {}x{} but the image is {}x{}",
                base.width, base.height, self.width, self.height
            ));
        }
        for (index, level) in self.levels.iter().enumerate() {
            if !self.format.accepts(level.width, level.height) {
                return Err(format!(
                    "level {index} is {}x{}, not a whole number of {:?} blocks;                      pad to a multiple of {:?} before encoding",
                    level.width,
                    level.height,
                    self.format,
                    self.format.block()
                ));
            }
            let expected =
                match CompressedLevel::expected_bytes(self.format, level.width, level.height) {
                    Ok(expected) => expected,
                    Err(why) => {
                        return Err(format!(
                            "level {index} is {}x{}: {why}",
                            level.width, level.height
                        ));
                    }
                };
            if level.data.len() != expected {
                return Err(format!(
                    "level {index} is {}x{} -> {} bytes, expected {expected} for {:?}",
                    level.width,
                    level.height,
                    level.data.len(),
                    self.format
                ));
            }
        }
        Ok(())
    }

    /// Total bytes of the chain (what the GPU will hold).
    pub fn bytes(&self) -> u64 {
        self.levels
            .iter()
            .map(|level| level.data.len() as u64)
            .sum()
    }

    /// Build the mip chain from a single encoded level by keeping only level 0
    /// (used when an `.astc` sidecar has no mips: the sampler then clamps LOD).
    pub fn single_level(format: CompressedFormat, width: u32, height: u32, data: Vec<u8>) -> Self {
        Self {
            format,
            width,
            height,
            levels: vec![CompressedLevel {
                width,
                height,
                data,
            }],
        }
    }
}

/// Which compressed formats the adapter can sample.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompressionSupport {
    pub astc_ldr: bool,
    pub astc_hdr: bool,
    pub etc2: bool,
    pub bc: bool,
}

impl CompressionSupport {
    /// Probe the adapter's enabled features.
    pub fn probe(adapter: &wgpu::Adapter) -> Self {
        Self::from_features(adapter.features())
    }

    pub fn from_features(features: wgpu::Features) -> Self {
        Self {
            astc_ldr: features.contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC),
            astc_hdr: features.contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC_HDR),
            etc2: features.contains(wgpu::Features::TEXTURE_COMPRESSION_ETC2),
            bc: features.contains(wgpu::Features::TEXTURE_COMPRESSION_BC),
        }
    }

    /// True when at least one compressed colour format can be sampled.
    pub fn any(&self) -> bool {
        self.astc_ldr || self.etc2 || self.bc
    }

    /// Best format for model textures, by preference: ASTC 6x6 (3.56 bpp) then
    /// ETC2 RGBA8 (8 bpp). `None` when the adapter takes only uncompressed data.
    pub fn best_color(&self) -> Option<CompressedFormat> {
        if self.astc_ldr {
            Some(CompressedFormat::Astc6x6)
        } else if self.etc2 {
            Some(CompressedFormat::Etc2Rgba8)
        } else {
            None
        }
    }

    /// Best small format for terrain/HUD sheets.
    pub fn best_small(&self) -> Option<CompressedFormat> {
        if self.astc_ldr {
            Some(CompressedFormat::Astc8x8)
        } else if self.etc2 {
            Some(CompressedFormat::Etc2Rgb)
        } else {
            None
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "astc_ldr={} astc_hdr={} etc2={} bc={}",
            self.astc_ldr, self.astc_hdr, self.etc2, self.bc
        )
    }
}

// ------------------------------------------------------------------ GPU store

/// A texture the store can upload: raw RGBA8 or pre-encoded block data.
#[derive(Clone, Copy)]
pub enum TextureSource<'a> {
    Rgba(&'a TextureImage),
    Compressed(&'a CompressedImage),
}

/// Aggregate texture memory numbers, reported by the bench.
#[derive(Debug, Default, Clone, Copy)]
pub struct TextureStats {
    pub textures: usize,
    pub levels: usize,
    pub bytes: u64,
    /// Bytes the baseline policy (everything 256x256, same mips) would use.
    pub baseline_bytes: u64,
    pub resized: usize,
    /// Textures uploaded as block-compressed data (ASTC/ETC2 sidecars).
    pub compressed: usize,
    pub upscaled_by_baseline: usize,
    pub upload_micros: u64,
}

impl TextureStats {
    pub fn saved_pct(&self) -> f32 {
        if self.baseline_bytes == 0 {
            return 0.0;
        }
        (1.0 - self.bytes as f32 / self.baseline_bytes as f32) * 100.0
    }
}

// ------------------------------------------------------------ BLP mip chains

/// How an authored BLP mip chain maps onto the renderer's size cap.
///
/// WC3 BLPs ship a full, already-filtered chain. Uploading level 0 and
/// regenerating every level on the CPU (what `upload_sources` does for raw RGBA)
/// throws that work away and re-filters an already filtered image; this plan
/// starts at the authored level that fits the cap instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlpChainPlan {
    /// First authored mip to upload (`0` when the image already fits the cap).
    pub base_level: usize,
    /// Size of `base_level`.
    pub width: u32,
    pub height: u32,
    /// Authored levels available from `base_level` on.
    pub authored: usize,
    /// Levels the texture needs: `log2(max(width, height)) + 1`.
    pub levels: usize,
}

impl BlpChainPlan {
    /// Levels that must be generated because the file stores a short chain.
    pub fn generated(&self) -> usize {
        self.levels.saturating_sub(self.authored)
    }
}

/// Pick the authored BLP mip that fits `max_size` and describe the full chain.
pub fn blp_chain_plan(blp: &wc3_formats::blp::BlpImage, max_size: u32) -> BlpChainPlan {
    let cap = max_size.max(1);
    let mips = &blp.mipmaps;
    let mut base_level = 0usize;
    if !mips.is_empty() {
        // Levels halve, so the first level that fits is the largest one that does.
        base_level = mips
            .iter()
            .position(|mip| mip.width <= cap && mip.height <= cap)
            .unwrap_or(mips.len() - 1);
    }
    let (width, height) = mips
        .get(base_level)
        .map(|mip| (mip.width, mip.height))
        .unwrap_or((blp.width, blp.height));
    let authored = mips.len().saturating_sub(base_level).max(1);
    let levels = level_count(width.max(1), height.max(1)) as usize;
    BlpChainPlan {
        base_level,
        width,
        height,
        authored,
        levels,
    }
}

/// Build the upload chain for a BLP: authored levels first, then a generated
/// tail when the file stores fewer levels than the texture size needs.
pub fn blp_levels(blp: &wc3_formats::blp::BlpImage, plan: &BlpChainPlan) -> Vec<Mip> {
    let mut levels: Vec<Mip> = Vec::with_capacity(plan.levels);
    for mip in blp.mipmaps.iter().skip(plan.base_level) {
        if levels.len() >= plan.levels {
            break;
        }
        levels.push(Mip {
            width: mip.width.max(1),
            height: mip.height.max(1),
            rgba: mip.rgba.clone(),
        });
    }
    if levels.is_empty() {
        levels.push(Mip {
            width: plan.width.max(1),
            height: plan.height.max(1),
            rgba: blp.rgba.clone(),
        });
    }
    if levels.len() < plan.levels {
        // Generate only the missing tail, starting from the last authored level
        // so the filter never crosses a resolution jump.
        let last = levels.last().expect("non-empty");
        let tail = generate_mips(last.width, last.height, &last.rgba);
        for mip in tail.into_iter().skip(1) {
            if levels.len() >= plan.levels {
                break;
            }
            levels.push(mip);
        }
    }
    levels
}

/// Uploads textures with the project's format/mip policy and keeps the bind
/// groups alive for the renderer.
pub struct TextureStore {
    bind_groups: Vec<wgpu::BindGroup>,
    pub stats: TextureStats,
}

impl TextureStore {
    /// Upload `images` in order. `max_size` caps the authored size (256 for WC3).
    pub fn upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        images: &[TextureImage],
        role: Role,
        max_size: u32,
        mipmapped: bool,
    ) -> Self {
        let started = std::time::Instant::now();
        let mut stats = TextureStats::default();
        let mut bind_groups = Vec::with_capacity(images.len());
        for image in images {
            let (fitted, resized) = image.fit(max_size);
            if resized {
                stats.resized += 1;
            }
            let levels = if mipmapped {
                generate_mips(fitted.width, fitted.height, &fitted.rgba)
            } else {
                vec![Mip {
                    width: fitted.width,
                    height: fitted.height,
                    rgba: fitted.rgba.clone(),
                }]
            };
            let bytes: u64 = levels
                .iter()
                .map(|mip| (mip.width * mip.height * 4) as u64)
                .sum();
            stats.textures += 1;
            stats.levels += levels.len();
            stats.bytes += bytes;
            stats.baseline_bytes += mip_bytes(max_size, max_size, 32);
            if image.width < max_size || image.height < max_size {
                stats.upscaled_by_baseline += 1;
            }
            bind_groups.push(upload_levels(device, queue, layout, sampler, role, &levels));
        }
        stats.upload_micros = started.elapsed().as_micros() as u64;
        Self { bind_groups, stats }
    }

    /// Upload decoded BLP images using their **authored** mip chain.
    ///
    /// Identical output to [`Self::upload`] for images that fit the cap, but it
    /// never re-filters a level the exporter already produced and it starts at
    /// the authored mip that fits when the texture is larger than the cap.
    pub fn upload_blp_chains(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        images: &[&wc3_formats::blp::BlpImage],
        max_size: u32,
    ) -> Self {
        let started = std::time::Instant::now();
        let mut stats = TextureStats::default();
        let mut bind_groups = Vec::with_capacity(images.len());
        for blp in images {
            let plan = blp_chain_plan(blp, max_size);
            let levels = blp_levels(blp, &plan);
            let bytes: u64 = levels
                .iter()
                .map(|mip| (mip.width * mip.height * 4) as u64)
                .sum();
            stats.textures += 1;
            stats.levels += levels.len();
            stats.bytes += bytes;
            stats.baseline_bytes += mip_bytes(max_size, max_size, 32);
            if plan.base_level > 0 {
                stats.resized += 1;
            }
            if plan.width < max_size || plan.height < max_size {
                stats.upscaled_by_baseline += 1;
            }
            bind_groups.push(upload_levels(
                device,
                queue,
                layout,
                sampler,
                Role::Color,
                &levels,
            ));
        }
        stats.upload_micros = started.elapsed().as_micros() as u64;
        Self { bind_groups, stats }
    }

    /// Upload a mixed list: RGBA sources are capped/mipped/uploaded as usual,
    /// compressed sources (ASTC sidecars from the asset pipeline) are uploaded
    /// as-is. This is how a game ships ASTC without changing the call sites that
    /// already pass RGBA when no sidecar exists.
    pub fn upload_sources(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        sources: &[TextureSource<'_>],
        max_size: u32,
        mipmapped: bool,
    ) -> Result<Self, String> {
        let started = std::time::Instant::now();
        let mut stats = TextureStats::default();
        let mut bind_groups = Vec::with_capacity(sources.len());
        for source in sources {
            match source {
                TextureSource::Rgba(image) => {
                    let (fitted, resized) = image.fit(max_size);
                    if resized {
                        stats.resized += 1;
                    }
                    let levels = if mipmapped {
                        generate_mips(fitted.width, fitted.height, &fitted.rgba)
                    } else {
                        vec![Mip {
                            width: fitted.width,
                            height: fitted.height,
                            rgba: fitted.rgba.clone(),
                        }]
                    };
                    let bytes: u64 = levels
                        .iter()
                        .map(|mip| (mip.width * mip.height * 4) as u64)
                        .sum();
                    stats.textures += 1;
                    stats.levels += levels.len();
                    stats.bytes += bytes;
                    stats.baseline_bytes += mip_bytes(max_size, max_size, 32);
                    if image.width < max_size || image.height < max_size {
                        stats.upscaled_by_baseline += 1;
                    }
                    bind_groups.push(upload_levels(
                        device,
                        queue,
                        layout,
                        sampler,
                        Role::Color,
                        &levels,
                    ));
                }
                TextureSource::Compressed(image) => {
                    image.validate()?;
                    stats.textures += 1;
                    stats.levels += image.levels.len();
                    stats.bytes += image.bytes();
                    stats.baseline_bytes += mip_bytes(max_size, max_size, 32);
                    stats.compressed += 1;
                    bind_groups.push(upload_compressed(
                        device,
                        queue,
                        layout,
                        sampler,
                        image,
                        Role::Color,
                    ));
                }
            }
        }
        stats.upload_micros = started.elapsed().as_micros() as u64;
        Ok(Self { bind_groups, stats })
    }

    pub fn bind_group(&self, index: usize) -> Option<&wgpu::BindGroup> {
        self.bind_groups.get(index)
    }

    pub fn len(&self) -> usize {
        self.bind_groups.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bind_groups.is_empty()
    }

    pub fn first_or(&self, index: usize) -> &wgpu::BindGroup {
        self.bind_groups
            .get(index)
            .unwrap_or_else(|| &self.bind_groups[0])
    }
}

/// Shared bind group layout for `(texture, sampler)` pairs.
///
/// Both passes create their bind groups against this layout, so a texture bind
/// group built for the mesh pass is valid in the UI pass without a second copy.
pub fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("texture-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// Upload a texture at its authored size (atlases have precomputed UVs, so
/// resizing them would move every tile), optionally with a full mip chain.
pub fn upload_texture_native(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    image: &TextureImage,
    role: Role,
    mipmapped: bool,
) -> wgpu::BindGroup {
    let levels = if mipmapped {
        generate_mips(image.width, image.height, &image.rgba)
    } else {
        vec![Mip {
            width: image.width.max(1),
            height: image.height.max(1),
            rgba: image.rgba.clone(),
        }]
    };
    upload_levels(device, queue, layout, sampler, role, &levels)
}

/// Fit, mip and upload one texture, returning its bind group.
pub fn upload_texture_mipped(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    image: &TextureImage,
    role: Role,
    max_size: u32,
) -> wgpu::BindGroup {
    let (fitted, _) = image.fit(max_size);
    let levels = generate_mips(fitted.width, fitted.height, &fitted.rgba);
    upload_levels(device, queue, layout, sampler, role, &levels)
}

/// Upload a block-compressed image (whole mip chain) and bind it.
///
/// `bytes_per_row` for compressed data must be the natural pitch (a multiple of
/// the block size); the 256-byte alignment rule applies to buffer copies, not to
/// `Queue::write_texture`.
pub fn upload_compressed(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    image: &CompressedImage,
    role: Role,
) -> wgpu::BindGroup {
    let format = match role {
        Role::Color => image.format.srgb(),
        Role::Coverage => image.format.linear(),
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("astc"),
        size: wgpu::Extent3d {
            width: image.width.max(1),
            height: image.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: image.levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, mip) in image.levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &mip.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.format.natural_pitch(mip.width)),
                rows_per_image: Some(image.format.block_rows(mip.height)),
            },
            wgpu::Extent3d {
                width: mip.width,
                height: mip.height,
                depth_or_array_layers: 1,
            },
        );
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("astc-bind"),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
        layout,
    })
}

/// Upload one texture with an explicit mip chain.
pub fn upload_levels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    role: Role,
    levels: &[Mip],
) -> wgpu::BindGroup {
    let base = &levels[0];
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("tex"),
        size: wgpu::Extent3d {
            width: base.width.max(1),
            height: base.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: role.format(),
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, mip) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &mip.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(mip.width * 4),
                rows_per_image: Some(mip.height),
            },
            wgpu::Extent3d {
                width: mip.width,
                height: mip.height,
                depth_or_array_layers: 1,
            },
        );
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("tex-bind"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

// ------------------------------------------------------- compression planning

/// One texture as seen by the compression planner.
#[derive(Debug, Clone, Copy)]
pub struct TexInfo {
    pub width: u32,
    pub height: u32,
    pub rgba_bytes: u64,
}

/// Bytes per pixel of the candidate formats, times 256 to stay integral.
#[derive(Debug, Clone, Copy)]
pub struct FormatCost {
    pub name: &'static str,
    /// Bits per pixel.
    pub bpp: u32,
    /// GPU support on the Adreno 730 / Turnip target.
    pub supported: bool,
}

/// Formats worth considering for a mobile Adreno target.
///
/// ASTC LDR and ETC2 are core in Vulkan 1.0+; `wgpu` exposes them behind the
/// `TEXTURE_COMPRESSION_ASTC` / `TEXTURE_COMPRESSION_ETC2` features, so a plan
/// can be executed by the asset pipeline without a renderer change.
pub const CANDIDATE_FORMATS: &[FormatCost] = &[
    FormatCost {
        name: "rgba8",
        bpp: 32,
        supported: true,
    },
    FormatCost {
        name: "astc-6x6",
        bpp: 4,
        supported: true,
    },
    FormatCost {
        name: "astc-8x8",
        bpp: 2,
        supported: true,
    },
    FormatCost {
        name: "etc2-rgba8",
        bpp: 8,
        supported: true,
    },
    FormatCost {
        name: "etc2-rgb",
        bpp: 4,
        supported: true,
    },
];

/// Estimated GPU bytes for `info` in `bpp`, including mips.
pub fn plan_bytes(info: &TexInfo, bpp: u32) -> u64 {
    mip_bytes(info.width, info.height, bpp)
}

/// Human-readable plan table over all loaded textures.
pub fn compression_plan(images: &[TexInfo], max_size: u32) -> String {
    let mut fitted = Vec::with_capacity(images.len());
    for info in images {
        let (width, height) = (
            info.width.min(max_size).max(1),
            info.height.min(max_size).max(1),
        );
        fitted.push(TexInfo {
            width: pow2_at_most(width),
            height: pow2_at_most(height),
            rgba_bytes: info.rgba_bytes,
        });
    }
    let mut out = String::new();
    out.push_str("format      gpu_mbps   MiB   vs_rgba8  supported\n");
    let base: u64 = fitted.iter().map(|info| plan_bytes(info, 32)).sum();
    for format in CANDIDATE_FORMATS {
        let total: u64 = fitted.iter().map(|info| plan_bytes(info, format.bpp)).sum();
        out.push_str(&format!(
            "{:<10}  {:>8.2}  {:>5.2}  {:>7.1}%  {}\n",
            format.name,
            total as f32 / 1.0e6,
            total as f64 / (1024.0 * 1024.0),
            (1.0 - total as f32 / base.max(1) as f32) * 100.0,
            if format.supported { "yes" } else { "no" }
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_never_upscales() {
        let image = TextureImage::new(64, 64, vec![9u8; 64 * 64 * 4]);
        let (fitted, resized) = image.fit(256);
        assert!(!resized);
        assert_eq!((fitted.width, fitted.height), (64, 64));
    }

    #[test]
    fn fit_downsamples_oversized_to_pow2() {
        let image = TextureImage::new(300, 200, vec![3u8; 300 * 200 * 4]);
        let (fitted, resized) = image.fit(256);
        assert!(resized);
        assert_eq!((fitted.width, fitted.height), (256, 128));
        assert_eq!(fitted.rgba.len(), 256 * 128 * 4);
    }

    #[test]
    fn mip_matches_powf_reference() {
        // Deterministic gradient + alpha ramp exercises the whole curve.
        let (w, h) = (32u32, 32u32);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                rgba.push((x * 8) as u8);
                rgba.push((y * 8) as u8);
                rgba.push(((x + y) * 4) as u8);
                rgba.push(((x * 13 + y * 7) % 256) as u8);
            }
        }
        let mips = generate_mips(w, h, &rgba);
        let reference = reference_mips(w, h, &rgba);
        assert_eq!(mips.len(), reference.len());
        for (level, (ours, theirs)) in mips.iter().zip(reference.iter()).enumerate() {
            assert_eq!((ours.width, ours.height), (theirs.width, theirs.height));
            let worst = ours
                .rgba
                .iter()
                .zip(theirs.rgba.iter())
                .map(|(a, b)| (*a as i32 - *b as i32).abs())
                .max()
                .unwrap_or(0);
            assert!(worst <= 1, "level {level} differs by {worst}");
        }
    }

    fn reference_mips(width: u32, height: u32, rgba: &[u8]) -> Vec<Mip> {
        let mut pixels = rgba.to_vec();
        let (mut w, mut h) = (width, height);
        let mut out = vec![Mip {
            width,
            height,
            rgba: pixels.clone(),
        }];
        while w > 1 || h > 1 {
            let nw = (w / 2).max(1);
            let nh = (h / 2).max(1);
            let mut next = vec![0u8; (nw * nh * 4) as usize];
            for y in 0..nh {
                for x in 0..nw {
                    let sx0 = (x * 2).min(w - 1);
                    let sy0 = (y * 2).min(h - 1);
                    let sx1 = (sx0 + 1).min(w - 1);
                    let sy1 = (sy0 + 1).min(h - 1);
                    let sample = |px: u32, py: u32, channel: usize| -> u8 {
                        pixels[((py * w + px) * 4) as usize + channel]
                    };
                    for channel in 0..3 {
                        let linear = (decode_srgb_reference(sample(sx0, sy0, channel))
                            + decode_srgb_reference(sample(sx1, sy0, channel))
                            + decode_srgb_reference(sample(sx0, sy1, channel))
                            + decode_srgb_reference(sample(sx1, sy1, channel)))
                            * 0.25;
                        next[((y * nw + x) * 4) as usize + channel] = encode_srgb_reference(linear);
                    }
                    let alpha = (sample(sx0, sy0, 3) as u32
                        + sample(sx1, sy0, 3) as u32
                        + sample(sx0, sy1, 3) as u32
                        + sample(sx1, sy1, 3) as u32)
                        / 4;
                    next[((y * nw + x) * 4) as usize + 3] = alpha as u8;
                }
            }
            pixels = next;
            w = nw;
            h = nh;
            out.push(Mip {
                width: w,
                height: h,
                rgba: pixels.clone(),
            });
        }
        out
    }

    #[test]
    fn solid_colour_survives_every_level() {
        let mips = generate_mips(8, 8, &[200, 80, 40, 255].repeat(64));
        for mip in &mips {
            assert_eq!(&mip.rgba[..3], &[200, 80, 40]);
            assert_eq!(mip.rgba[3], 255);
        }
    }

    fn blp(width: u32, height: u32, levels: &[(u32, u32)]) -> wc3_formats::blp::BlpImage {
        use wc3_formats::blp::{BlpImage, MipLevel};
        let mipmaps = levels
            .iter()
            .map(|(w, h)| MipLevel {
                width: *w,
                height: *h,
                rgba: vec![0u8; (w * h * 4) as usize],
            })
            .collect();
        BlpImage {
            width,
            height,
            rgba: vec![0u8; (width * height * 4) as usize],
            mipmaps,
        }
    }

    #[test]
    fn blp_plan_uses_authored_levels_under_the_cap() {
        // 64x64 with a full chain, cap 256: nothing is dropped or generated.
        let image = blp(
            64,
            64,
            &[(64, 64), (32, 32), (16, 16), (8, 8), (4, 4), (2, 2), (1, 1)],
        );
        let plan = blp_chain_plan(&image, 256);
        assert_eq!(plan.base_level, 0);
        assert_eq!((plan.width, plan.height), (64, 64));
        assert_eq!(plan.authored, 7);
        assert_eq!(plan.levels, 7);
        assert_eq!(plan.generated(), 0);

        // 512x512, cap 256: start at the authored 256x256 level.
        let image = blp(512, 512, &[(512, 512), (256, 256), (128, 128), (1, 1)]);
        let plan = blp_chain_plan(&image, 256);
        assert_eq!(plan.base_level, 1);
        assert_eq!((plan.width, plan.height), (256, 256));
        assert_eq!(plan.authored, 3);
        assert_eq!(plan.levels, 9);
        // 3..=1 jumps, so the tail is generated.
        assert!(plan.generated() > 0);
    }

    #[test]
    fn blp_levels_are_a_complete_halving_chain() {
        // A short chain (only 4 levels for a 16x16 image) must be completed.
        let image = blp(16, 16, &[(16, 16), (8, 8), (4, 4), (2, 2)]);
        let plan = blp_chain_plan(&image, 256);
        assert_eq!(plan.levels, 5);
        let levels = blp_levels(&image, &plan);
        assert_eq!(levels.len(), 5, "16,8,4,2,1");
        let sizes: Vec<(u32, u32)> = levels.iter().map(|mip| (mip.width, mip.height)).collect();
        assert_eq!(sizes, vec![(16, 16), (8, 8), (4, 4), (2, 2), (1, 1)]);
        for mip in &levels {
            assert_eq!(mip.rgba.len(), (mip.width * mip.height * 4) as usize);
        }
    }

    #[test]
    fn blp_plan_without_mipmaps_falls_back_to_level_zero() {
        let image = blp(32, 32, &[]);
        let plan = blp_chain_plan(&image, 256);
        assert_eq!(plan.base_level, 0);
        assert_eq!((plan.width, plan.height), (32, 32));
        assert_eq!(plan.levels, 6);
        let levels = blp_levels(&image, &plan);
        assert_eq!(levels.len(), 6);
        assert_eq!((levels[0].width, levels[0].height), (32, 32));
    }

    #[test]
    fn level_count_is_full_chain() {
        assert_eq!(level_count(1, 1), 1);
        assert_eq!(level_count(256, 256), 9);
        assert_eq!(level_count(256, 128), 9);
        assert_eq!(level_count(64, 16), 7);
    }

    #[test]
    fn mip_bytes_matches_geometric_series() {
        // 4x4 RGBA8: 16+4+1 texels * 4 levels? 4x4 -> 2x2 -> 1x1 = 3 levels.
        assert_eq!(mip_bytes(4, 4, 32), (16 + 4 + 1) * 4);
    }

    #[test]
    fn pow2_at_most_is_exact() {
        assert_eq!(pow2_at_most(1), 1);
        assert_eq!(pow2_at_most(255), 128);
        assert_eq!(pow2_at_most(256), 256);
        assert_eq!(pow2_at_most(300), 256);
    }

    #[test]
    fn compression_plan_prefers_astc() {
        let images = vec![TexInfo {
            width: 256,
            height: 256,
            rgba_bytes: 256 * 256 * 4,
        }];
        let plan = compression_plan(&images, 256);
        assert!(plan.contains("astc-8x8"));
        assert!(plan.contains("rgba8"));
    }

    #[test]
    fn expected_bytes_accepts_a_legitimate_large_level() {
        // 16384x16384 is the adapter maximum; ASTC 6x6 has 2731x2731 blocks.
        let expected = CompressedLevel::expected_bytes(CompressedFormat::Astc6x6, 16_384, 16_384)
            .expect("the adapter maximum is a legal level");
        assert_eq!(expected, 2731 * 2731 * 16);
        // ETC2 RGBA8 uses 4x4 blocks and 16 bytes per block.
        let etc2 = CompressedLevel::expected_bytes(CompressedFormat::Etc2Rgba8, 16_384, 16_384)
            .expect("the adapter maximum is a legal level");
        assert_eq!(etc2, 4096 * 4096 * 16);
    }

    #[test]
    fn expected_bytes_rejects_dimensions_that_would_overflow_u32() {
        // The pre-fix code multiplied the pitch by the block rows in `u32`:
        // these sizes wrapped (release) or panicked (debug) before validation.
        for (width, height) in [
            (u32::MAX, u32::MAX),
            (u32::MAX, 1),
            (1, u32::MAX),
            (MAX_TEXTURE_SIDE + 1, 1),
            (65_536, 65_536),
        ] {
            let err = CompressedLevel::expected_bytes(CompressedFormat::Astc6x6, width, height)
                .expect_err("oversized side must be rejected before the multiplication");
            assert!(
                err.contains("side limit"),
                "{width}x{height}: unexpected error {err}"
            );
        }
    }

    #[test]
    fn expected_bytes_rejects_zero_dimensions() {
        for (width, height) in [(0, 16), (16, 0), (0, 0)] {
            let err = CompressedLevel::expected_bytes(CompressedFormat::Astc8x8, width, height)
                .expect_err("zero dimension has no block grid");
            assert!(err.contains("zero dimension"), "unexpected error {err}");
        }
    }

    #[test]
    fn is_consistent_is_false_for_impossible_levels_instead_of_panicking() {
        let impossible = CompressedLevel {
            width: u32::MAX,
            height: u32::MAX,
            data: vec![0u8; 16],
        };
        assert!(!impossible.is_consistent(CompressedFormat::Astc6x6));
        // 32766 is a whole number of 6x6 blocks but above the adapter cap: the
        // old arithmetic wrapped here, so 16 bytes could pass a length check.
        let oversized = CompressedLevel {
            width: 32_766,
            height: 32_766,
            data: vec![0u8; 16],
        };
        assert!(!oversized.is_consistent(CompressedFormat::Astc6x6));
        let image = CompressedImage {
            format: CompressedFormat::Astc6x6,
            width: 32_766,
            height: 32_766,
            levels: vec![oversized],
        };
        let err = image.validate().expect_err("impossible level is invalid");
        assert!(err.contains("side limit"), "unexpected error {err}");
    }

    #[test]
    fn a_level_one_byte_short_or_long_is_inconsistent() {
        let expected = CompressedLevel::expected_bytes(CompressedFormat::Astc8x8, 64, 64)
            .expect("64x64 ASTC 8x8 is legal");
        let level = |len: usize| CompressedLevel {
            width: 64,
            height: 64,
            data: vec![0u8; len],
        };
        assert!(level(expected).is_consistent(CompressedFormat::Astc8x8));
        assert!(!level(expected - 1).is_consistent(CompressedFormat::Astc8x8));
        assert!(!level(expected + 1).is_consistent(CompressedFormat::Astc8x8));
    }
}

/// Bytes of an ASTC mip chain for a `width`x`height` image at `block_w`x`block_h`.
///
///
/// Every level is `ceil(w/bw) * ceil(h/bh) * 16` bytes; the 2048-byte row
/// alignment applies only to buffer copies, not to `Queue::write_texture`.
pub fn astc_chain_bytes(width: u32, height: u32, block_w: u32, block_h: u32) -> u64 {
    let bw = block_w.max(1);
    let bh = block_h.max(1);
    let mut w = width.max(1);
    let mut h = height.max(1);
    let mut total = 0u64;
    loop {
        total += w.div_ceil(bw) as u64 * h.div_ceil(bh) as u64 * 16;
        if w == 1 && h == 1 {
            break;
        }
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    total
}
