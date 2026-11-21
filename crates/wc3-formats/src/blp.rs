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

/// Alpha depths the paletted branches understand (WC3-FMT-001/007). Any other
/// value used to be read as "fully opaque", which turns a corrupt file into a
/// plausible-looking texture instead of a typed error.
fn checked_alpha_bits(bits: u32) -> Result<u32, FormatError> {
    match bits {
        0 | 1 | 4 | 8 => Ok(bits),
        _ => Err(FormatError::Corrupt("BLP alpha depth")),
    }
}

/// Bytes of the per-mip alpha plane that follows the index plane.
fn alpha_plane_len(count: usize, bits: u32) -> usize {
    match bits {
        0 => 0,
        1 => count.div_ceil(8),
        4 => count.div_ceil(2),
        _ => count,
    }
}

/// One alpha sample. The caller has already validated that the plane holds
/// `alpha_plane_len(count, bits)` bytes, so every index below is in range.
fn alpha_sample(alpha: &[u8], bits: u32, index: usize) -> u8 {
    match bits {
        0 => 255,
        8 => alpha[index],
        4 => {
            let byte = alpha[index / 2];
            let nibble = if index % 2 == 0 { byte & 0x0F } else { byte >> 4 };
            nibble * 17
        }
        1 => {
            let byte = alpha[index / 8];
            if byte & (1 << (index % 8)) != 0 {
                255
            } else {
                0
            }
        }
        _ => 255,
    }
}

/// Dimensions of every stored mip level (each half of the previous, never
/// below 1×1). A BLP carries at most 16 levels.
fn mip_dimensions(width: u32, height: u32) -> [(u32, u32); 16] {
    let mut dims = [(0u32, 0u32); 16];
    let (mut w, mut h) = (width, height);
    for slot in dims.iter_mut() {
        *slot = (w, h);
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    dims
}

/// One validated mip payload: where it starts and how many bytes the branch
/// reads from it (`needed`), which may be less than the declared size because
/// trailing padding is legal.
#[derive(Debug, Clone, Copy)]
struct MipRange {
    start: usize,
    needed: usize,
}

impl MipRange {
    fn payload_end(self) -> usize {
        self.start + self.needed
    }
}

/// The single offset/size contract every BLP branch goes through
/// (WC3-FMT-001/002/004/007):
///
/// * a mip may not start inside its branch's header/palette region;
/// * `must_declare` is the payload the size table is required to cover,
///   `reads` is the extent the decoder will touch. Both are checked against the
///   file with checked arithmetic, and `reads` may extend past `sizes[i]` only
///   where the format puts a separate alpha plane after a declared index plane
///   (BLP2 paletted, WC3-FMT-007);
/// * the extents the decoder reads may not overlap each other;
/// * the chain ends at the first empty slot, as the format declares.
fn validate_mips(
    file_len: usize,
    offsets: &[u32; 16],
    sizes: &[u32; 16],
    header_end: usize,
    mut must_declare: impl FnMut(usize) -> usize,
    mut reads: impl FnMut(usize) -> usize,
) -> Result<Vec<MipRange>, FormatError> {
    let mut ranges: Vec<MipRange> = Vec::new();
    for level in 0..16 {
        if offsets[level] == 0 || sizes[level] == 0 {
            break;
        }
        let start = offsets[level] as usize;
        let declared = sizes[level] as usize;
        let required = must_declare(level);
        let needed = reads(level);
        if start < header_end {
            return Err(FormatError::Corrupt("BLP mip points into the header"));
        }
        if required > declared {
            return Err(FormatError::Corrupt(
                "BLP mip payload shorter than its pixels",
            ));
        }
        let declared_end = start
            .checked_add(declared)
            .ok_or(FormatError::Corrupt("BLP mip range overflow"))?;
        if declared_end > file_len {
            return Err(FormatError::Corrupt("BLP mip out of bounds"));
        }
        let read_end = start
            .checked_add(needed)
            .ok_or(FormatError::Corrupt("BLP mip range overflow"))?;
        if read_end > file_len {
            return Err(FormatError::Corrupt("BLP mip pixels past end of file"));
        }
        let range = MipRange { start, needed };
        if ranges
            .iter()
            .any(|other| start < other.payload_end() && other.start < range.payload_end())
        {
            return Err(FormatError::Corrupt("BLP mip payloads overlap"));
        }
        ranges.push(range);
    }
    if ranges.is_empty() {
        return Err(FormatError::Corrupt("BLP has no mips"));
    }
    Ok(ranges)
}

/// Paletted mips shared by BLP1 (`header_end = 1180`) and BLP2
/// (`header_end = 1172`): `count` index bytes followed by the per-mip alpha
/// plane. There is no global alpha table in either version.
///
/// `alpha_in_size` is the one layout difference: BLP1's size table covers the
/// index plane *and* its alpha plane (measured on all 3 901 local BLP1 files),
/// while BLP2's declares the index plane only and the alpha plane follows it in
/// the file.
fn decode_paletted(
    bytes: &[u8],
    offsets: &[u32; 16],
    sizes: &[u32; 16],
    width: u32,
    height: u32,
    palette: &[u8],
    header_end: usize,
    alpha_bits: u32,
    alpha_in_size: bool,
) -> Result<BlpImage, FormatError> {
    let alpha_bits = checked_alpha_bits(alpha_bits)?;
    let dims = mip_dimensions(width, height);
    let count_of = |level: usize| {
        let (w, h) = dims[level];
        w as usize * h as usize
    };
    let ranges = validate_mips(
        bytes.len(),
        offsets,
        sizes,
        header_end,
        |level| {
            let count = count_of(level);
            if alpha_in_size {
                count + alpha_plane_len(count, alpha_bits)
            } else {
                count
            }
        },
        |level| {
            let count = count_of(level);
            count + alpha_plane_len(count, alpha_bits)
        },
    )?;
    let mut mipmaps = Vec::with_capacity(ranges.len());
    for (level, range) in ranges.iter().enumerate() {
        let (w, h) = dims[level];
        let count = w as usize * h as usize;
        let alpha_len = alpha_plane_len(count, alpha_bits);
        // The range validator guarantees `count + alpha_len` bytes here.
        let indices = &bytes[range.start..range.start + count];
        let alpha = &bytes[range.start + count..range.start + count + alpha_len];
        let mut rgba = Vec::with_capacity(count * 4);
        for (index, &palette_index) in indices.iter().enumerate() {
            let p = palette_index as usize * 4;
            rgba.extend_from_slice(&[
                palette[p + 2],
                palette[p + 1],
                palette[p],
                alpha_sample(alpha, alpha_bits, index),
            ]);
        }
        mipmaps.push(MipLevel {
            width: w,
            height: h,
            rgba,
        });
    }
    let first = mipmaps
        .first()
        .cloned()
        .ok_or(FormatError::Corrupt("BLP paletted has no mips"))?;
    Ok(BlpImage {
        width,
        height,
        rgba: first.rgba,
        mipmaps,
    })
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
        let header_end = header_start
            .checked_add(header_size)
            .ok_or(FormatError::Corrupt("BLP1 JPEG header length"))?;
        let header = bytes
            .get(header_start..header_end)
            .ok_or(FormatError::Corrupt("BLP1 JPEG header truncated"))?;
        // A JPEG mip payload is opaque scan data: the declared size *is* what is
        // appended to the shared header, so the validator checks placement,
        // EOF and overlap rather than the value.
        let dims = mip_dimensions(width, height);
        let ranges = validate_mips(
            bytes.len(),
            &offsets,
            &sizes,
            header_end,
            |level| sizes[level] as usize,
            |level| sizes[level] as usize,
        )?;
        let mut mipmaps = Vec::with_capacity(ranges.len());
        for (level, range) in ranges.iter().enumerate() {
            let (mip_width, mip_height) = dims[level];
            let data = &bytes[range.start..range.start + range.needed];
            let mut stream = Vec::with_capacity(header.len() + data.len());
            stream.extend_from_slice(header);
            stream.extend_from_slice(data);
            let image = decode_jpeg_stream(&stream, mip_width, mip_height, alpha_bits)?;
            let level_image = image
                .mipmaps
                .into_iter()
                .next()
                .ok_or(FormatError::Corrupt("BLP1 JPEG mip has no pixels"))?;
            mipmaps.push(level_image);
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
        let header_end = pos + 1024;
        let palette = bytes
            .get(pos..header_end)
            .ok_or(FormatError::TooShort)?;
        decode_paletted(
            bytes,
            &offsets,
            &sizes,
            width,
            height,
            palette,
            header_end,
            alpha_bits,
            true,
        )
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
    // Byte 0x0A is the pixel/alpha-encoding field (`preferredFormat` in the
    // published layout, `alphaEncoding` in War3Net): PIXEL_DXT1 = 0,
    // PIXEL_DXT3 = 1, PIXEL_DXT5 = 7, PIXEL_UNSPECIFIED = 8.
    let preferred_format = header[10];
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
    // BLP2 header (148 B) + the 1024-byte extended data region (palette, JPEG
    // header or padding) precede every compression's mip data.
    let header_end = 148 + 1024;
    match compression {
        1 => {
            let palette = bytes.get(148..header_end).ok_or(FormatError::TooShort)?;
            decode_paletted(
                bytes,
                &offsets,
                &sizes,
                width,
                height,
                palette,
                header_end,
                u32::from(alpha_bits),
                false,
            )
        }
        2 => {
            let kind = dxt_kind(alpha_bits, preferred_format)?;
            let block_size = kind.block_size();
            let dims = mip_dimensions(width, height);
            let block_bytes = |level: usize| {
                let (w, h) = dims[level];
                w.div_ceil(4) as usize * h.div_ceil(4) as usize * block_size
            };
            // The published layout documents that *lower* compressed mips may
            // carry a wrong `sizes[i]` and that the correct block count must be
            // zero-filled ("Some lower mipmap levels in compressed textures have
            // incorrect size ... the rest bytes in buffer can be filled with
            // zeros"). Mip 0 is the visible surface and must be complete.
            let must_declare = |level: usize| if level == 0 { block_bytes(0) } else { 0 };
            let reads = |level: usize| {
                let needed = block_bytes(level);
                needed.min(sizes[level] as usize)
            };
            let ranges = validate_mips(
                bytes.len(),
                &offsets,
                &sizes,
                header_end,
                must_declare,
                reads,
            )?;
            let mut mipmaps = Vec::with_capacity(ranges.len());
            for (level, range) in ranges.iter().enumerate() {
                let (w, h) = dims[level];
                let data = &bytes[range.start..range.start + range.needed];
                let stored = block_bytes(level);
                if data.len() < stored {
                    // Documented retail padding: zero-fill the blocks the size
                    // table under-declared for a lower mip.
                    let mut padded = vec![0u8; stored];
                    padded[..data.len()].copy_from_slice(data);
                    let rgba = decode_dxt(&padded, w, h, kind)?;
                    mipmaps.push(MipLevel {
                        width: w,
                        height: h,
                        rgba,
                    });
                    continue;
                }
                let rgba = decode_dxt(data, w, h, kind)?;
                mipmaps.push(MipLevel {
                    width: w,
                    height: h,
                    rgba,
                });
            }
            let first = mipmaps
                .first()
                .cloned()
                .ok_or(FormatError::Corrupt("no BLP2 mip"))?;
            Ok(BlpImage {
                width,
                height,
                rgba: first.rgba,
                mipmaps,
            })
        }
        3 => {
            // ARGB8888 payload: the extended-region `preferred_format` byte must
            // name an uncompressed format (PIXEL_ARGB8888 = 2, its duplicate = 4,
            // or PIXEL_UNSPECIFIED = 8). A DXT/BC value on a raw payload is a
            // tuple no decoder can honour (WC3-I-001).
            if !matches!(preferred_format, 2 | 4 | 8) {
                return Err(FormatError::Unsupported("BLP2 ARGB8888 pixel format"));
            }
            let dims = mip_dimensions(width, height);
            let raw_bytes = |level: usize| {
                let (w, h) = dims[level];
                w as usize * h as usize * 4
            };
            let ranges = validate_mips(
                bytes.len(),
                &offsets,
                &sizes,
                header_end,
                raw_bytes,
                raw_bytes,
            )?;
            let mut mipmaps = Vec::with_capacity(ranges.len());
            for (level, range) in ranges.iter().enumerate() {
                let (w, h) = dims[level];
                let data = &bytes[range.start..range.start + range.needed];
                let mut rgba = Vec::with_capacity(data.len());
                for px in data.chunks_exact(4) {
                    rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
                }
                mipmaps.push(MipLevel {
                    width: w,
                    height: h,
                    rgba,
                });
            }
            let first = mipmaps
                .first()
                .cloned()
                .ok_or(FormatError::Corrupt("no BLP2 mip"))?;
            Ok(BlpImage {
                width,
                height,
                rgba: first.rgba,
                mipmaps,
            })
        }
        other => Err(FormatError::Unsupported(if other == 0 {
            "BLP2 JPEG"
        } else {
            "unknown BLP2 compression"
        })),
    }
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
    // WC3-FMT-005: read the frame header first and refuse a JPEG that does not
    // describe this exact mip *before* any pixel work. Without this the
    // library's default `usize::MAX` buffer limit lets a 147 KB file announce
    // 5000x5000 and spend 205 MB / 7 s decoding pixels that are then thrown
    // away.
    let expected = width as usize * height as usize * 4;
    let mut decoder = jpeg_decoder::Decoder::new(stream);
    decoder
        .read_info()
        .map_err(|_| FormatError::Corrupt("JPEG header decode failed"))?;
    let info = decoder
        .info()
        .ok_or(FormatError::Corrupt("JPEG info missing"))?;
    // WC3-FMT-003: the embedded frame must be the mip the BLP declares.
    if info.width as u32 != width || info.height as u32 != height {
        return Err(FormatError::Corrupt(
            "JPEG dimensions do not match the BLP mip",
        ));
    }
    // Belt and braces: the decoder's own plane budget can never exceed the
    // RGBA budget of the mip we validated.
    decoder.set_max_decoding_buffer_size(expected);
    let pixels = decoder
        .decode()
        .map_err(|_| FormatError::Corrupt("JPEG decode failed"))?;
    let rgba = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => {
            let mut out = Vec::with_capacity(expected);
            for px in pixels.chunks_exact(3) {
                out.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            out
        }
        jpeg_decoder::PixelFormat::L8 => {
            let mut out = Vec::with_capacity(expected);
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
            let mut out = Vec::with_capacity(expected);
            for px in pixels.chunks_exact(4) {
                out.extend_from_slice(&cmyk_pixel(px[0], px[1], px[2], px[3], alpha_bits));
            }
            out
        }
        _ => return Err(FormatError::Unsupported("JPEG pixel format")),
    };
    // WC3-FMT-006: a short decode is corruption, not blank pixels at the edge.
    if rgba.len() != expected {
        return Err(FormatError::Corrupt(
            "JPEG pixel buffer does not match the mip",
        ));
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

/// Pick the DXT variant from the full header tuple (WC3-I-001), not from
/// `alpha_bits` alone.
///
/// Sources (accessed 2026-11-18):
/// * `wowdev.wiki/BLP`, "Struct definition" — `BLPPixelFormat`:
///   `PIXEL_DXT1 = 0`, `PIXEL_DXT3 = 1`, `PIXEL_ARGB8888 = 2`,
///   `PIXEL_DXT5 = 7`, `PIXEL_UNSPECIFIED = 8`, `PIXEL_BC5 = 11`, and the
///   note "Alpha channel bit depth: 0, 1 or 8" plus "bit depth = 4 are found
///   in Interface\Glues\Credits\*.blp".
/// * War3Net `War3Net.Drawing.Blp/BlpFile.cs` (reference decoder for the same
///   container): `alphaDepth > 1 ? (alphaEncoding == 7 ? DXT5 : DXT3) : DXT1`.
///
/// The old code mapped `alpha_bits` 8 → DXT3 and 4 → DXT5, i.e. it decoded
/// every 8-bit-alpha DXT5 texture as DXT3 (chunky 4-bit alpha) and a 4-bit
/// credits texture as DXT5. Unknown tuples are a typed error, never a guess.
fn dxt_kind(alpha_bits: u8, preferred_format: u8) -> Result<DxtKind, FormatError> {
    match preferred_format {
        0 | 1 | 7 | 8 => {}
        _ => return Err(FormatError::Unsupported("BLP2 DXT pixel format")),
    }
    match alpha_bits {
        0 => Ok(DxtKind::Dxt1Opaque),
        1 => Ok(DxtKind::Dxt1Alpha),
        4 | 8 => Ok(if preferred_format == 7 {
            DxtKind::Dxt5
        } else {
            DxtKind::Dxt3
        }),
        _ => Err(FormatError::Unsupported("BLP2 DXT alpha depth")),
    }
}

impl DxtKind {
    fn block_size(self) -> usize {
        match self {
            DxtKind::Dxt1Opaque | DxtKind::Dxt1Alpha => 8,
            DxtKind::Dxt3 | DxtKind::Dxt5 => 16,
        }
    }
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
    let block_size = kind.block_size();
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
