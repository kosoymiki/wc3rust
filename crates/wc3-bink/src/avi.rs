//! RIFF/AVI demultiplexer for the Warcraft III cinematics.
//!
//! The shipped movies are not `.bik` files: each `Movies\*.mpq` (and the
//! Frozen Throne trailers `*.blz`) is a plain AVI container holding
//!
//! * one `vids` stream with handler `divx` and fourcc `BLZ0` — MPEG-4 Part 2
//!   elementary stream, one VOP start code per `##dc` chunk;
//! * one `auds` stream, WAVE tag `0x0055` — MPEG-1 Layer III at 44.1 kHz
//!   stereo, one `##wb` chunk per AVI chunk.
//!
//! Frames are returned as borrowed slices of the file bytes: the demuxer does
//! not copy payloads, so the decoder can seek directly into `&[u8]`.
//!
//! The parser is strictly bounds-checked; a truncated or hostile file returns
//! [`AviError`] instead of panicking or allocating from a bogus size field.

use std::fmt;
use std::path::Path;

/// Streaming error.
#[derive(Debug)]
pub enum AviError {
    Io(std::io::Error),
    /// Not a RIFF/AVI file.
    NotAvi,
    /// A chunk header or size field runs past the end of the file.
    Truncated {
        what: &'static str,
    },
    /// A header field is shorter than the fixed layout this parser reads.
    InvalidHeader {
        what: &'static str,
        len: usize,
        need: usize,
    },
    /// A structural limit (nesting depth or chunk count) was exceeded.
    Limit {
        what: &'static str,
        limit: usize,
    },
    /// Required structure (`hdrl`, a `vids` stream) is missing.
    Missing(&'static str),
}

impl fmt::Display for AviError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::NotAvi => write!(f, "not a RIFF/AVI file"),
            Self::Truncated { what } => write!(f, "truncated AVI while reading {what}"),
            Self::InvalidHeader { what, len, need } => {
                write!(f, "AVI {what} is {len} bytes, needs at least {need}")
            }
            Self::Limit { what, limit } => write!(f, "AVI exceeds the {what} limit of {limit}"),
            Self::Missing(what) => write!(f, "AVI is missing {what}"),
        }
    }
}

impl std::error::Error for AviError {}

impl From<std::io::Error> for AviError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Video stream description from `strh`/`strf`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoInfo {
    /// `strh.fccHandler` (`divx`).
    pub handler: [u8; 4],
    /// `strf.biCompression` (`BLZ0`).
    pub fourcc: [u8; 4],
    pub width: u16,
    pub height: u16,
    pub bit_count: u16,
    /// `strh.dwScale`.
    pub scale: u32,
    /// `strh.dwRate`.
    pub rate: u32,
    /// `strh.dwLength` (frames declared by the stream header).
    pub length: u32,
    /// `avih.dwMicroSecPerFrame`.
    pub micros_per_frame: u32,
    /// `avih.dwTotalFrames`.
    pub total_frames: u32,
}

impl VideoInfo {
    /// Frames per second as a rational (`rate / scale`), matching `strh`.
    pub fn fps(&self) -> f64 {
        if self.scale == 0 {
            0.0
        } else {
            f64::from(self.rate) / f64::from(self.scale)
        }
    }

    pub fn fourcc_str(&self) -> String {
        String::from_utf8_lossy(&self.fourcc).to_string()
    }

    pub fn handler_str(&self) -> String {
        String::from_utf8_lossy(&self.handler).to_string()
    }
}

/// Audio stream description from `strh`/`strf`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioInfo {
    /// WAVE format tag; `0x0055` is MPEG-1 Layer III, `0x0001` PCM.
    pub format_tag: u16,
    pub channels: u16,
    pub sample_rate: u32,
    /// `strh.dwLength` in samples (per the stream header).
    pub length: u32,
}

impl AudioInfo {
    pub fn is_mpeg_layer3(&self) -> bool {
        self.format_tag == 0x0055
    }
}

/// One `##dc` / `##wb` chunk, as an offset into the file bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk {
    /// Stream number from the fourcc (`00dc` -> 0).
    pub stream: u8,
    /// `dc` (compressed video), `wb` (audio) or the raw two-byte code.
    pub kind: [u8; 2],
    pub offset: usize,
    pub len: usize,
}

impl Chunk {
    pub fn is_video(&self) -> bool {
        &self.kind == b"dc"
    }

    pub fn is_audio(&self) -> bool {
        &self.kind == b"wb"
    }
}

/// Parsed AVI file.
#[derive(Debug)]
pub struct Avi {
    data: Vec<u8>,
    video: VideoInfo,
    audio: Option<AudioInfo>,
    chunks: Vec<Chunk>,
    idx1: Vec<(u32, u32, usize, usize)>,
    audio_stream: Option<u8>,
    video_stream: u8,
}

impl Avi {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AviError> {
        let data = std::fs::read(path)?;
        Self::from_bytes(data)
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<Self, AviError> {
        if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"AVI " {
            return Err(AviError::NotAvi);
        }
        let riff_end = 8usize
            .checked_add(u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize)
            .unwrap_or(data.len())
            .min(data.len());

        let mut raw_video: Option<(VideoInfo, u8)> = None;
        let mut raw_audio: Option<(AudioInfo, u8)> = None;
        let mut chunks = Vec::new();
        let mut idx1 = Vec::new();

        // Walk the RIFF tree: LIST hdrl carries stream descriptions, LIST movi
        // carries the payload chunks.
        let mut pos = 12usize;
        while pos + 8 <= riff_end {
            let id = &data[pos..pos + 4];
            let size = u32::from_le_bytes([
                data[pos + 4],
                data[pos + 5],
                data[pos + 6],
                data[pos + 7],
            ]) as usize;
            let body = pos + 8;
            let body_end = body.checked_add(size).filter(|e| *e <= riff_end).ok_or(
                AviError::Truncated {
                    what: "RIFF chunk",
                },
            )?;
            if id == b"LIST" {
                if body + 4 > body_end {
                    return Err(AviError::Truncated { what: "LIST type" });
                }
                match &data[body..body + 4] {
                    b"hdrl" => {
                        if raw_video.is_none() {
                            let (v, a) = parse_hdrl(&data, body + 4, body_end)?;
                            raw_video = v;
                            raw_audio = a;
                        }
                    }
                    b"movi" => {
                        parse_movi(&data, body + 4, body_end, &mut chunks)?;
                    }
                    _ => {}
                }
            } else if id == b"idx1" {
                let mut p = body;
                while p + 16 <= body_end {
                    let ckid = u32::from_le_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]]);
                    let flags =
                        u32::from_le_bytes([data[p + 4], data[p + 5], data[p + 6], data[p + 7]]);
                    let off =
                        u32::from_le_bytes([data[p + 8], data[p + 9], data[p + 10], data[p + 11]]);
                    let len = u32::from_le_bytes([
                        data[p + 12],
                        data[p + 13],
                        data[p + 14],
                        data[p + 15],
                    ]);
                    // idx1 offsets are relative to the 'movi' fourcc.
                    idx1.push((ckid, flags, off as usize, len as usize));
                    p += 16;
                }
            }
            pos = body_end + (size & 1);
        }

        let (video, video_stream) = raw_video.ok_or(AviError::Missing("a vids stream"))?;
        let audio_stream = raw_audio.as_ref().map(|(_, s)| *s);
        Ok(Self {
            data,
            video,
            audio: raw_audio.map(|(a, _)| a),
            chunks,
            idx1,
            audio_stream,
            video_stream,
        })
    }

    pub fn video(&self) -> &VideoInfo {
        &self.video
    }

    pub fn audio(&self) -> Option<&AudioInfo> {
        self.audio.as_ref()
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Declared index entries (`idx1`), if the file carries the optional index.
    pub fn idx1(&self) -> &[(u32, u32, usize, usize)] {
        &self.idx1
    }

    /// All payload chunks in file order.
    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    /// Number of video payload chunks actually present in `movi`.
    pub fn video_frame_count(&self) -> usize {
        self.chunks
            .iter()
            .filter(|c| c.stream == self.video_stream && c.is_video())
            .count()
    }

    /// Video frames in stream order, borrowed from the file bytes.
    pub fn video_frames(&self) -> impl Iterator<Item = &[u8]> + '_ {
        self.chunks
            .iter()
            .filter(move |c| c.stream == self.video_stream && c.is_video())
            .map(move |c| &self.data[c.offset..c.offset + c.len])
    }

    /// Frame `index` by stream order.
    pub fn video_frame(&self, index: usize) -> Option<&[u8]> {
        self.chunks
            .iter()
            .filter(move |c| c.stream == self.video_stream && c.is_video())
            .nth(index)
            .map(|c| &self.data[c.offset..c.offset + c.len])
    }

    /// Audio payload chunks in stream order.
    pub fn audio_chunks(&self) -> impl Iterator<Item = &[u8]> + '_ {
        let stream = self.audio_stream;
        self.chunks
            .iter()
            .filter(move |c| Some(c.stream) == stream && c.is_audio())
            .map(move |c| &self.data[c.offset..c.offset + c.len])
    }

    pub fn audio_chunk_count(&self) -> usize {
        let stream = self.audio_stream;
        self.chunks
            .iter()
            .filter(move |c| Some(c.stream) == stream && c.is_audio())
            .count()
    }
}

fn parse_hdrl(
    d: &[u8],
    mut pos: usize,
    end: usize,
) -> Result<(Option<(VideoInfo, u8)>, Option<(AudioInfo, u8)>), AviError> {
    let mut video = None;
    let mut audio = None;
    let mut avih: Option<&[u8]> = None;
    let mut stream_index = 0u8;
    while pos + 8 <= end {
        let id = &d[pos..pos + 4];
        let size = u32::from_le_bytes([d[pos + 4], d[pos + 5], d[pos + 6], d[pos + 7]]) as usize;
        let body = pos + 8;
        let body_end = body.checked_add(size).filter(|e| *e <= end).ok_or(AviError::Truncated {
            what: "hdrl chunk",
        })?;
        if id == b"avih" {
            avih = Some(&d[body..body_end]);
        } else if id == b"LIST" && body + 4 <= body_end && &d[body..body + 4] == b"strl" {
            // AVI stream numbers are the position of the `strl` list in `hdrl`,
            // not the stream type: a file that lists `auds` first owns stream 0
            // and its video chunks carry the `01dc` prefix. Passing the index
            // through keeps the census correct for those files.
            let (v, a) = parse_strl(d, body + 4, body_end, stream_index)?;
            stream_index = stream_index.saturating_add(1);
            if video.is_none() {
                video = v;
            }
            if audio.is_none() {
                audio = a;
            }
        }
        pos = body_end + (size & 1);
    }

    if let Some((mut v, s)) = video {
        if let Some(avih) = avih {
            if avih.len() >= 40 {
                v.micros_per_frame = u32::from_le_bytes([avih[0], avih[1], avih[2], avih[3]]);
                v.total_frames = u32::from_le_bytes([avih[16], avih[17], avih[18], avih[19]]);
            }
        }
        Ok((Some((v, s)), audio))
    } else {
        Ok((None, audio))
    }
}

/// Bytes of `AVISTREAMHEADER` (`strh`) this parser reads: `fccType` +
/// `fccHandler` (0..8), `dwScale`/`dwRate` (20..28) and `dwLength` (32..36).
/// A shorter body is rejected before any indexing.
const STRH_MIN_LEN: usize = 36;
/// `BITMAPINFOHEADER` bytes read for a `vids` stream, through `biCompression`.
const STRF_VIDS_MIN_LEN: usize = 20;
/// `WAVEFORMATEX` bytes read for an `auds` stream, through `nSamplesPerSec`.
const STRF_AUDS_MIN_LEN: usize = 8;

fn parse_strl(
    d: &[u8],
    mut pos: usize,
    end: usize,
    stream: u8,
) -> Result<(Option<(VideoInfo, u8)>, Option<(AudioInfo, u8)>), AviError> {
    let mut strh: Option<&[u8]> = None;
    let mut strf: Option<&[u8]> = None;
    while pos + 8 <= end {
        let id = &d[pos..pos + 4];
        let size = u32::from_le_bytes([d[pos + 4], d[pos + 5], d[pos + 6], d[pos + 7]]) as usize;
        let body = pos + 8;
        let body_end = body.checked_add(size).filter(|e| *e <= end).ok_or(AviError::Truncated {
            what: "strl chunk",
        })?;
        match id {
            b"strh" => strh = Some(&d[body..body_end]),
            b"strf" => strf = Some(&d[body..body_end]),
            _ => {}
        }
        pos = body_end + (size & 1);
    }
    let (Some(strh), Some(strf)) = (strh, strf) else {
        return Ok((None, None));
    };
    if strh.len() < 4 {
        return Err(AviError::InvalidHeader {
            what: "strh (fccType)",
            len: strh.len(),
            need: 4,
        });
    }
    let fcc = [strh[0], strh[1], strh[2], strh[3]];
    if &fcc != b"vids" && &fcc != b"auds" {
        return Ok((None, None));
    }
    if strh.len() < STRH_MIN_LEN {
        return Err(AviError::InvalidHeader {
            what: "strh",
            len: strh.len(),
            need: STRH_MIN_LEN,
        });
    }
    let handler = [strh[4], strh[5], strh[6], strh[7]];
    let scale = u32::from_le_bytes([strh[20], strh[21], strh[22], strh[23]]);
    let rate = u32::from_le_bytes([strh[24], strh[25], strh[26], strh[27]]);
    let length = u32::from_le_bytes([strh[32], strh[33], strh[34], strh[35]]);
    match &fcc {
        b"vids" => {
            if strf.len() < STRF_VIDS_MIN_LEN {
                return Err(AviError::InvalidHeader {
                    what: "strf",
                    len: strf.len(),
                    need: STRF_VIDS_MIN_LEN,
                });
            }
            let width = u16::from_le_bytes([strf[4], strf[5]]);
            let height = u16::from_le_bytes([strf[8], strf[9]]);
            let bit_count = u16::from_le_bytes([strf[14], strf[15]]);
            let fourcc = [strf[16], strf[17], strf[18], strf[19]];
            Ok((
                Some((
                    VideoInfo {
                        handler,
                        fourcc,
                        width,
                        height,
                        bit_count,
                        scale,
                        rate,
                        length,
                        micros_per_frame: 0,
                        total_frames: 0,
                    },
                    stream,
                )),
                None,
            ))
        }
        b"auds" => {
            if strf.len() < STRF_AUDS_MIN_LEN {
                return Err(AviError::InvalidHeader {
                    what: "strf",
                    len: strf.len(),
                    need: STRF_AUDS_MIN_LEN,
                });
            }
            Ok((
                None,
                Some((
                    AudioInfo {
                        format_tag: u16::from_le_bytes([strf[0], strf[1]]),
                        channels: u16::from_le_bytes([strf[2], strf[3]]),
                        sample_rate: u32::from_le_bytes([strf[4], strf[5], strf[6], strf[7]]),
                        length,
                    },
                    stream,
                )),
            ))
        }
        _ => Ok((None, None)),
    }
}

/// Deepest `LIST rec ` nesting accepted inside `movi`.
///
/// Retail cinematics nest one level; the limit only stops a crafted file from
/// turning the walk into unbounded recursion.
pub const MAX_MOVI_DEPTH: u32 = 32;
/// Hard cap on the payload chunks collected from one `movi` list.
pub const MAX_MOVI_CHUNKS: usize = 1_048_576;

fn parse_movi(
    d: &[u8],
    pos: usize,
    end: usize,
    out: &mut Vec<Chunk>,
) -> Result<(), AviError> {
    /// One open `LIST rec ` range; `depth` counts the containers around it.
    struct Frame {
        pos: usize,
        end: usize,
        depth: u32,
    }

    // Explicit range stack instead of recursion: the enclosing frame advances
    // past a nested list *before* the child is descended into, so the cursor is
    // monotonic, the child's chunks are collected in file order, and every
    // chunk's word padding is skipped exactly once.
    let mut stack = vec![Frame { pos, end, depth: 0 }];
    while let Some(top) = stack.last() {
        let (p, frame_end, depth) = (top.pos, top.end, top.depth);
        if frame_end.saturating_sub(p) < 8 {
            stack.pop();
            continue;
        }
        let id = [d[p], d[p + 1], d[p + 2], d[p + 3]];
        let size = u32::from_le_bytes([d[p + 4], d[p + 5], d[p + 6], d[p + 7]]) as usize;
        let body = p + 8;
        let body_end = body
            .checked_add(size)
            .filter(|e| *e <= frame_end)
            .ok_or(AviError::Truncated { what: "movi chunk" })?;
        stack.last_mut().expect("stack is non-empty").pos = body_end + (size & 1);
        if id == *b"LIST" {
            if body_end - body >= 4 && &d[body..body + 4] == b"rec " {
                let child = depth + 1;
                if child > MAX_MOVI_DEPTH {
                    return Err(AviError::Limit {
                        what: "movi rec nesting depth",
                        limit: MAX_MOVI_DEPTH as usize,
                    });
                }
                stack.push(Frame {
                    pos: body + 4,
                    end: body_end,
                    depth: child,
                });
            }
            // Any other LIST type is a container this demuxer does not read.
        } else if let Some(stream) = hex_digit(id[0])
            .zip(hex_digit(id[1]))
            .map(|(high, low)| high * 16 + low)
        {
            let kind = [id[2], id[3]];
            if kind == *b"dc" || kind == *b"wb" || kind == *b"db" {
                if out.len() >= MAX_MOVI_CHUNKS {
                    return Err(AviError::Limit {
                        what: "movi payload chunks",
                        limit: MAX_MOVI_CHUNKS,
                    });
                }
                out.push(Chunk {
                    stream,
                    kind,
                    offset: body,
                    len: size,
                });
            }
        }
    }
    Ok(())
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One RIFF chunk: `id`, declared size, body, word padding.
    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(8 + body.len() + 1);
        v.extend_from_slice(id);
        v.extend_from_slice(&(body.len() as u32).to_le_bytes());
        v.extend_from_slice(body);
        if body.len() & 1 == 1 {
            v.push(0);
        }
        v
    }

    fn list(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut inner = Vec::new();
        inner.extend_from_slice(kind);
        inner.extend_from_slice(body);
        chunk(b"LIST", &inner)
    }

    /// `AVISTREAMHEADER` body of exactly `len` bytes for `fcc`, with every field
    /// the parser reads filled when it fits (a shorter body leaves the tail off,
    /// which is what a truncated untrusted file looks like).
    fn stream_header(fcc: &[u8; 4], len: usize) -> Vec<u8> {
        let mut h = vec![0u8; len];
        if len >= 4 {
            h[0..4].copy_from_slice(fcc);
        }
        if *fcc == *b"vids" && len >= 8 {
            h[4..8].copy_from_slice(b"divx");
        }
        if len >= 24 {
            h[20..24].copy_from_slice(&41_554u32.to_le_bytes());
        }
        if len >= 28 {
            h[24..28].copy_from_slice(&1_000_000u32.to_le_bytes());
        }
        if len >= 36 {
            h[32..36].copy_from_slice(&7u32.to_le_bytes());
        }
        h
    }

    fn strf_video() -> Vec<u8> {
        let mut s = vec![0u8; 40];
        s[0..4].copy_from_slice(&40u32.to_le_bytes());
        s[4..8].copy_from_slice(&800i32.to_le_bytes());
        s[8..12].copy_from_slice(&368i32.to_le_bytes());
        s[12..14].copy_from_slice(&1u16.to_le_bytes());
        s[14..16].copy_from_slice(&24u16.to_le_bytes());
        s[16..20].copy_from_slice(b"BLZ0");
        s
    }

    fn strf_audio() -> Vec<u8> {
        let mut s = vec![0u8; 18];
        s[0..2].copy_from_slice(&0x0055u16.to_le_bytes());
        s[2..4].copy_from_slice(&2u16.to_le_bytes());
        s[4..8].copy_from_slice(&44_100u32.to_le_bytes());
        s
    }

    /// `movi` body with the video frames interleaved with audio, like retail.
    fn movi_body(video_chunks: &[&[u8]]) -> Vec<u8> {
        let mut body = Vec::new();
        for (i, payload) in video_chunks.iter().enumerate() {
            body.extend(chunk(b"00dc", payload));
            let audio = vec![0xA5u8; 4 + (i % 3)];
            body.extend(chunk(b"01wb", &audio));
        }
        body
    }

    /// Build a minimal but structurally valid AVI in memory, with caller-chosen
    /// stream headers and `movi` body.
    fn build_avi_with(
        vstrh: &[u8],
        vstrf: &[u8],
        astrh: &[u8],
        astrf: &[u8],
        movi: &[u8],
        total_frames: u32,
    ) -> Vec<u8> {
        let mut avih = vec![0u8; 56];
        avih[0..4].copy_from_slice(&41554u32.to_le_bytes());
        avih[16..20].copy_from_slice(&total_frames.to_le_bytes());
        avih[32..36].copy_from_slice(&800u32.to_le_bytes());
        avih[36..40].copy_from_slice(&368u32.to_le_bytes());

        let mut vstrl = Vec::new();
        vstrl.extend(chunk(b"strh", vstrh));
        vstrl.extend(chunk(b"strf", vstrf));
        let mut astrl = Vec::new();
        astrl.extend(chunk(b"strh", astrh));
        astrl.extend(chunk(b"strf", astrf));

        let mut hdrl_body = chunk(b"avih", &avih);
        hdrl_body.extend(list(b"strl", &vstrl));
        hdrl_body.extend(list(b"strl", &astrl));

        let mut body = Vec::new();
        body.extend_from_slice(b"AVI ");
        body.extend(list(b"hdrl", &hdrl_body));
        body.extend(list(b"movi", movi));

        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    /// Build a minimal but structurally valid AVI in memory.
    fn build_avi(video_chunks: &[&[u8]]) -> Vec<u8> {
        build_avi_with(
            &stream_header(b"vids", 56),
            &strf_video(),
            &stream_header(b"auds", 56),
            &strf_audio(),
            &movi_body(video_chunks),
            video_chunks.len() as u32,
        )
    }

    /// Parse a bare `movi` body (the content of `LIST movi`, without its type).
    fn movi_parse(body: &[u8]) -> Result<Vec<Chunk>, AviError> {
        let mut out = Vec::new();
        parse_movi(body, 0, body.len(), &mut out)?;
        Ok(out)
    }

    /// Wrap `leaf` in `depth` nested `LIST rec ` containers, built inside-out
    /// without quadratic copying.
    fn nested_rec(depth: usize, leaf: &[u8]) -> Vec<u8> {
        let mut sizes = Vec::with_capacity(depth);
        let mut inner_len = leaf.len();
        for _ in 0..depth {
            sizes.push((inner_len + 4) as u32); // body = 'rec ' + inner
            inner_len += 12; // LIST header (8) + 'rec ' (4)
        }
        let mut out = Vec::with_capacity(inner_len);
        for size in sizes.iter().rev() {
            out.extend_from_slice(b"LIST");
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(b"rec ");
        }
        out.extend_from_slice(leaf);
        out
    }

    #[test]
    fn parses_streams_and_payloads() {
        let avi_bytes = build_avi(&[b"first", b"second", b"third"]);
        let avi = Avi::from_bytes(avi_bytes).unwrap();
        let v = avi.video();
        assert_eq!(v.fourcc, *b"BLZ0");
        assert_eq!(v.handler, *b"divx");
        assert_eq!(v.width, 800);
        assert_eq!(v.height, 368);
        assert_eq!(v.total_frames, 3);
        assert!((v.fps() - 1_000_000.0 / 41_554.0).abs() < 1e-9);
        let a = avi.audio().unwrap();
        assert_eq!(a.format_tag, 0x0055);
        assert_eq!(a.channels, 2);
        assert_eq!(a.sample_rate, 44_100);
        assert!(a.is_mpeg_layer3());

        let frames: Vec<&[u8]> = avi.video_frames().collect();
        assert_eq!(frames, vec![&b"first"[..], &b"second"[..], &b"third"[..]]);
        assert_eq!(avi.video_frame(1).unwrap(), b"second");
        assert!(avi.video_frame(3).is_none());
        assert_eq!(avi.audio_chunk_count(), 3);
        assert_eq!(avi.video_frame_count(), 3);
    }

    #[test]
    fn rejects_non_avi_and_truncated_input() {
        assert!(matches!(
            Avi::from_bytes(b"NOTARIFF".to_vec()),
            Err(AviError::NotAvi)
        ));
        let mut good = build_avi(&[b"x"]);
        good.truncate(good.len() / 2);
        // Either a truncation error or a missing-stream error is correct; the
        // contract is "no panic, no silent garbage".
        match Avi::from_bytes(good) {
            Err(AviError::Truncated { .. }) | Err(AviError::Missing(_)) => {}
            other => panic!("expected error, got {other:?}"),
        }
        // A chunk size that overflows must be rejected, not allocated.
        let mut evil = build_avi(&[b"x"]);
        let n = evil.len();
        evil[n - 8..n - 4].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
        assert!(Avi::from_bytes(evil).is_err());
    }

    #[test]
    fn fuzz_never_panics() {
        let base = build_avi(&[b"frame-one", b"frame-two"]);
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..64 {
            let mut data = base.clone();
            for _ in 0..8 {
                let idx = (next() as usize) % data.len();
                data[idx] = next() as u8;
            }
            let _ = Avi::from_bytes(data);
        }
        for len in 0..64 {
            let data: Vec<u8> = (0..len).map(|_| next() as u8).collect();
            let _ = Avi::from_bytes(data);
        }
    }

    /// AUD-010: every `strh` shorter than the layout the parser reads (36
    /// bytes) must return a typed error, never index out of bounds.
    #[test]
    fn rejects_every_short_strh_length() {
        for len in 0..36usize {
            let avi = build_avi_with(
                &stream_header(b"vids", len),
                &strf_video(),
                &stream_header(b"auds", 56),
                &strf_audio(),
                &movi_body(&[b"x"]),
                1,
            );
            match Avi::from_bytes(avi) {
                Err(AviError::InvalidHeader { what, len: got, need }) => {
                    assert_eq!(got, len, "reported strh length");
                    assert!(need > len && need <= STRH_MIN_LEN, "{what}: need {need}");
                }
                other => panic!("strh len {len}: expected InvalidHeader, got {other:?}"),
            }
        }
    }

    /// AUD-010: the audio stream header is length-checked the same way.
    #[test]
    fn rejects_short_audio_strh() {
        let avi = build_avi_with(
            &stream_header(b"vids", 56),
            &strf_video(),
            &stream_header(b"auds", 20),
            &strf_audio(),
            &movi_body(&[b"x"]),
            1,
        );
        assert!(matches!(
            Avi::from_bytes(avi),
            Err(AviError::InvalidHeader { what: "strh", len: 20, need: STRH_MIN_LEN })
        ));
    }

    /// AUD-010: the exact minimum header and an odd (padded) header both parse,
    /// and the pad byte does not desynchronise the `strl` walk.
    #[test]
    fn accepts_minimum_and_padded_strh() {
        for len in [STRH_MIN_LEN, STRH_MIN_LEN + 1, 56] {
            let avi = build_avi_with(
                &stream_header(b"vids", len),
                &strf_video(),
                &stream_header(b"auds", 56),
                &strf_audio(),
                &movi_body(&[b"frame"]),
                1,
            );
            let parsed = Avi::from_bytes(avi).unwrap_or_else(|e| panic!("strh len {len}: {e}"));
            assert_eq!(parsed.video().scale, 41_554);
            assert_eq!(parsed.video().rate, 1_000_000);
            assert_eq!(parsed.video().length, 7);
            assert_eq!(parsed.video_frame(0), Some(&b"frame"[..]));
            assert_eq!(parsed.audio().map(|a| a.sample_rate), Some(44_100));
        }
    }

    /// AUD-010: a `strf` shorter than the fields read is a typed error, not a
    /// silent zero-filled stream.
    #[test]
    fn rejects_short_strf() {
        let avi = build_avi_with(
            &stream_header(b"vids", 56),
            &strf_video()[..19],
            &stream_header(b"auds", 56),
            &strf_audio(),
            &movi_body(&[b"x"]),
            1,
        );
        assert!(matches!(
            Avi::from_bytes(avi),
            Err(AviError::InvalidHeader { what: "strf", len: 19, need: 20 })
        ));
        let avi = build_avi_with(
            &stream_header(b"vids", 56),
            &strf_video(),
            &stream_header(b"auds", 56),
            &strf_audio()[..7],
            &movi_body(&[b"x"]),
            1,
        );
        assert!(matches!(
            Avi::from_bytes(avi),
            Err(AviError::InvalidHeader { what: "strf", len: 7, need: 8 })
        ));
    }

    /// AUD-011: nested `rec ` lists are containers only; payload chunks keep
    /// file order and offsets stay correct across the nesting boundary.
    #[test]
    fn nested_rec_lists_keep_file_order() {
        let mut body = chunk(b"00dc", b"aa");
        body.extend(nested_rec(1, &chunk(b"00dc", b"bb")));
        body.extend(chunk(b"01wb", b"cccc"));
        body.extend(nested_rec(2, &chunk(b"00dc", b"dd")));
        let chunks = movi_parse(&body).expect("parse");
        let payloads: Vec<&[u8]> = chunks
            .iter()
            .map(|c| &body[c.offset..c.offset + c.len])
            .collect();
        assert_eq!(
            payloads,
            vec![&b"aa"[..], &b"bb"[..], &b"cccc"[..], &b"dd"[..]]
        );
        assert!(chunks.windows(2).all(|w| w[0].offset < w[1].offset));
    }

    /// AUD-011: depth 0 / 1 / max parse; max + 1 is a typed limit error.
    #[test]
    fn enforces_the_rec_depth_limit() {
        let leaf = chunk(b"00dc", b"leaf");
        for depth in [0usize, 1, MAX_MOVI_DEPTH as usize] {
            let body = nested_rec(depth, &leaf);
            let chunks = movi_parse(&body).unwrap_or_else(|e| panic!("depth {depth}: {e}"));
            assert_eq!(chunks.len(), 1, "depth {depth}");
        }
        let body = nested_rec(MAX_MOVI_DEPTH as usize + 1, &leaf);
        assert!(matches!(
            movi_parse(&body),
            Err(AviError::Limit {
                what: "movi rec nesting depth",
                ..
            })
        ));
    }

    /// AUD-011: an absurd nesting depth returns an error instead of recursing
    /// until the stack is gone.
    #[test]
    fn deep_nesting_does_not_exhaust_the_stack() {
        let body = nested_rec(200_000, &chunk(b"00dc", b"leaf"));
        assert!(matches!(movi_parse(&body), Err(AviError::Limit { .. })));
    }

    /// AUD-011: empty nested `LIST rec ` containers are skipped.
    #[test]
    fn empty_nested_lists_are_skipped() {
        let body = nested_rec(3, b"");
        assert!(movi_parse(&body).expect("parse").is_empty());
    }

    /// AUD-011: a declared size past the enclosing frame/end is `Truncated`.
    #[test]
    fn rejects_malformed_declared_sizes() {
        let mut body = nested_rec(1, &chunk(b"00dc", b"x"));
        let too_big = (body.len() as u32 + 64).to_le_bytes();
        body[4..8].copy_from_slice(&too_big);
        assert!(matches!(
            movi_parse(&body),
            Err(AviError::Truncated { what: "movi chunk" })
        ));

        let mut body = chunk(b"00dc", b"x");
        body[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            movi_parse(&body),
            Err(AviError::Truncated { what: "movi chunk" })
        ));
    }

    /// AUD-011: many siblings keep order, offsets and the word padding.
    #[test]
    fn keeps_many_sibling_chunks_in_order() {
        let mut body = Vec::new();
        for i in 0..10_000u32 {
            body.extend(chunk(b"00dc", &i.to_le_bytes()));
        }
        let chunks = movi_parse(&body).expect("parse");
        assert_eq!(chunks.len(), 10_000);
        for (i, c) in chunks.iter().enumerate() {
            assert_eq!(&body[c.offset..c.offset + c.len], &(i as u32).to_le_bytes());
        }
        assert!(chunks.windows(2).all(|w| w[0].offset < w[1].offset));

        // Odd-sized chunks must skip the RIFF pad byte.
        let mut body = chunk(b"00dc", b"odd");
        body.extend(chunk(b"00dc", b"even"));
        let chunks = movi_parse(&body).expect("parse");
        assert_eq!(chunks.len(), 2);
        assert_eq!(&body[chunks[1].offset..chunks[1].offset + 2], b"ev");
    }

    /// AUD-011: a full file with a nested `rec ` list yields its frames.
    #[test]
    fn full_avi_with_nested_rec_lists() {
        let mut movi = chunk(b"00dc", b"first");
        movi.extend(nested_rec(2, &chunk(b"00dc", b"second")));
        let avi = build_avi_with(
            &stream_header(b"vids", 56),
            &strf_video(),
            &stream_header(b"auds", 56),
            &strf_audio(),
            &movi,
            2,
        );
        let parsed = Avi::from_bytes(avi).expect("parse");
        assert_eq!(parsed.video_frame(0), Some(&b"first"[..]));
        assert_eq!(parsed.video_frame(1), Some(&b"second"[..]));
        assert_eq!(parsed.video_frame_count(), 2);
    }

    /// AUD-011: random `movi` bodies never panic.
    #[test]
    fn movi_fuzz_never_panics() {
        let base = nested_rec(2, &chunk(b"00dc", b"payload"));
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..256 {
            let mut data = base.clone();
            for _ in 0..4 {
                let idx = (next() as usize) % data.len();
                data[idx] = next() as u8;
            }
            let _ = movi_parse(&data);
        }
        for len in 0..96 {
            let data: Vec<u8> = (0..len).map(|_| next() as u8).collect();
            let _ = movi_parse(&data);
        }
    }
}
