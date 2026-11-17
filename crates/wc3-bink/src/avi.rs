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
    Truncated { what: &'static str },
    /// Required structure (`hdrl`, a `vids` stream) is missing.
    Missing(&'static str),
}

impl fmt::Display for AviError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::NotAvi => write!(f, "not a RIFF/AVI file"),
            Self::Truncated { what } => write!(f, "truncated AVI while reading {what}"),
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
                        let mut stream_idx = 0u8;
                        parse_movi(&data, body + 4, body_end, &mut stream_idx, &mut chunks)?;
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
            let (v, a) = parse_strl(d, body + 4, body_end)?;
            if let Some(v) = v {
                video = Some(v);
            }
            if let Some(a) = a {
                audio = Some(a);
            }
        }
        pos = body_end + (size & 1);
    }

    if let Some((mut v, s)) = video {
        if let Some(avih) = avih {
            if avih.len() >= 40 {
                v.micros_per_frame = rd_u32(avih, 0);
                v.total_frames = rd_u32(avih, 16);
            }
        }
        Ok((Some((v, s)), audio))
    } else {
        Ok((None, audio))
    }
}

fn parse_strl(
    d: &[u8],
    mut pos: usize,
    end: usize,
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
    let fcc = [strh[0], strh[1], strh[2], strh[3]];
    let handler = [strh[4], strh[5], strh[6], strh[7]];
    let scale = rd_u32(strh, 20);
    let rate = rd_u32(strh, 24);
    let length = rd_u32(strh, 32);
    match &fcc {
        b"vids" => {
            if strf.len() < 20 {
                return Ok((None, None));
            }
            let width = rd_u16(strf, 4);
            let height = rd_u16(strf, 8);
            let bit_count = rd_u16(strf, 14);
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
                    0,
                )),
                None,
            ))
        }
        b"auds" => {
            if strf.len() < 8 {
                return Ok((None, None));
            }
            Ok((
                None,
                Some((
                    AudioInfo {
                        format_tag: rd_u16(strf, 0),
                        channels: rd_u16(strf, 2),
                        sample_rate: rd_u32(strf, 4),
                        length,
                    },
                    1,
                )),
            ))
        }
        _ => Ok((None, None)),
    }
}

fn parse_movi(
    d: &[u8],
    mut pos: usize,
    end: usize,
    stream_idx: &mut u8,
    out: &mut Vec<Chunk>,
) -> Result<(), AviError> {
    while pos + 8 <= end {
        let id = &d[pos..pos + 4];
        let size = u32::from_le_bytes([d[pos + 4], d[pos + 5], d[pos + 6], d[pos + 7]]) as usize;
        let body = pos + 8;
        let body_end = body.checked_add(size).filter(|e| *e <= end).ok_or(AviError::Truncated {
            what: "movi chunk",
        })?;
        if id == b"LIST" {
            // Nested 'rec ' lists are just containers; recurse into them.
            if body + 4 <= body_end && &d[body..body + 4] == b"rec " {
                parse_movi(d, body + 4, body_end, stream_idx, out)?;
            }
        } else {
            let stream = hex_digit(id[0]).zip(hex_digit(id[1])).map(|(h, l)| h * 16 + l);
            let kind = [id[2], id[3]];
            if let Some(stream) = stream {
                if kind == *b"dc" || kind == *b"wb" || kind == *b"db" {
                    out.push(Chunk {
                        stream,
                        kind,
                        offset: body,
                        len: size,
                    });
                    *stream_idx = stream;
                }
            }
        }
        pos = body_end + (size & 1);
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

fn rd_u16(d: &[u8], at: usize) -> u16 {
    if at + 2 > d.len() {
        return 0;
    }
    u16::from_le_bytes([d[at], d[at + 1]])
}

fn rd_u32(d: &[u8], at: usize) -> u32 {
    if at + 4 > d.len() {
        return 0;
    }
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal but structurally valid AVI in memory.
    fn build_avi(video_chunks: &[&[u8]]) -> Vec<u8> {
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

        let mut avih = vec![0u8; 56];
        avih[0..4].copy_from_slice(&41554u32.to_le_bytes());
        avih[16..20].copy_from_slice(&(video_chunks.len() as u32).to_le_bytes());
        avih[32..36].copy_from_slice(&800u32.to_le_bytes());
        avih[36..40].copy_from_slice(&368u32.to_le_bytes());

        let mut vstrh = vec![0u8; 56];
        vstrh[0..4].copy_from_slice(b"vids");
        vstrh[4..8].copy_from_slice(b"divx");
        vstrh[20..24].copy_from_slice(&41554u32.to_le_bytes());
        vstrh[24..28].copy_from_slice(&1_000_000u32.to_le_bytes());

        let mut vstrf = vec![0u8; 40];
        vstrf[0..4].copy_from_slice(&40u32.to_le_bytes());
        vstrf[4..8].copy_from_slice(&800i32.to_le_bytes());
        vstrf[8..12].copy_from_slice(&368i32.to_le_bytes());
        vstrf[12..14].copy_from_slice(&1u16.to_le_bytes());
        vstrf[14..16].copy_from_slice(&24u16.to_le_bytes());
        vstrf[16..20].copy_from_slice(b"BLZ0");

        let mut astrh = vec![0u8; 56];
        astrh[0..4].copy_from_slice(b"auds");
        let mut astrf = vec![0u8; 18];
        astrf[0..2].copy_from_slice(&0x0055u16.to_le_bytes());
        astrf[2..4].copy_from_slice(&2u16.to_le_bytes());
        astrf[4..8].copy_from_slice(&44100u32.to_le_bytes());

        let mut vstrl = Vec::new();
        vstrl.extend(chunk(b"strh", &vstrh));
        vstrl.extend(chunk(b"strf", &vstrf));
        let mut astrl = Vec::new();
        astrl.extend(chunk(b"strh", &astrh));
        astrl.extend(chunk(b"strf", &astrf));

        let mut hdrl_body = chunk(b"avih", &avih);
        hdrl_body.extend(list(b"strl", &vstrl));
        hdrl_body.extend(list(b"strl", &astrl));

        let mut movi_body = Vec::new();
        for (i, payload) in video_chunks.iter().enumerate() {
            let id: [u8; 4] = [b'0', b'0', b'd', b'c'];
            movi_body.extend(chunk(&id, payload));
            // interleave an audio chunk like the retail files do
            let audio = vec![0xA5u8; 4 + (i % 3)];
            movi_body.extend(chunk(b"01wb", &audio));
        }

        let mut body = Vec::new();
        body.extend_from_slice(b"AVI ");
        body.extend(list(b"hdrl", &hdrl_body));
        body.extend(list(b"movi", &movi_body));

        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
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
}
