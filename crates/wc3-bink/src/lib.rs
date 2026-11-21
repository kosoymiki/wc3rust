//! Self-contained decoder for the Warcraft III cinematics.
//!
//! # What the files actually are
//!
//! The retail installs ship their cutscenes as `Movies\*.mpq` (Reign of Chaos)
//! and `trailers\*.blz` (The Frozen Throne). Despite the extensions these are
//! **RIFF/AVI** containers, not MPQ archives and not Bink files:
//!
//! * video stream `vids`, handler `divx`, fourcc `BLZ0`;
//! * audio stream `auds`, WAVE tag `0x0055` (MPEG-1 Layer III, 44.1 kHz
//!   stereo).
//!
//! The `BLZ0` payload is an MPEG-4 Part 2 (ISO/IEC 14496-2) elementary stream:
//! every video chunk starts with `00 00 01 B6` (VOP start code) and the first
//! frame carries `00 00 01 00` / `00 00 01 20` (VOS / VOL). `ffprobe` reports
//! `codec_name=mpeg4` for all nine shipped files. Measured evidence and the
//! per-file matrix live in `analysis/media/MOVIES.md`.
//!
//! # Layering
//!
//! ```text
//! avi        RIFF demuxer: hdrl streams, movi chunks, idx1
//! mpeg4      MPEG-4 Part 2 video decoder (I/P/B-VOP, 4:2:0)
//! mp3        MPEG-1 Layer III audio decoder
//! player     frame ring buffer + audio pull in sync with frames
//! ```
//!
//! # No external codec, no FFI
//!
//! The crate has zero dependencies: `cargo tree -p wc3-bink` lists only this
//! package. There is no `ffmpeg`, no `libav*`, no `bindgen`, no `-sys` crate.
//! FFmpeg appears only in the `#[ignore]`d oracle tests under `tests/`, which
//! run the `ffmpeg` binary to compare output; the library never spawns a
//! process and never links against a codec library.
//!
//! # Status (measured, 2026-11-16)
//!
//! | Stage | State |
//! |---|---|
//! | RIFF/AVI demux (streams, chunks, idx1) | complete, census matches the container headers for all nine files |
//! | MPEG-4 VOL/VOP headers | complete for the shipped profile; exotic options return a named `Unsupported` error |
//! | I-VOP decode | matches the oracle sample for sample on the retail intra frames |
//! | P-VOP decode | matches the oracle on short chains; a residual drift concentrates on the right picture edge and grows over long chains |
//! | B-VOP decode | implemented (direct/forward/backward/interpolated); **no shipped file contains a B-frame**, so it is unexercised |
//! | MP3 audio (Layer III) | implemented in `mp3/`; `BinkFile::audio_pcm` returns interleaved i16 PCM. Measured against the oracle in `tests/mp3_oracle.rs` (ignored, needs `ffmpeg` + `H2_MOVIE_SAMPLE`) |
//!
//! Diagnostics for the remaining P-frame work can be enabled with environment
//! variables (all no-ops in production): `WC3_BINK_DEBUG` (per-macroblock
//! traces), `WC3_BINK_MB`/`WC3_BINK_MBX`/`WC3_BINK_MBY` (target one macroblock),
//! `WC3_BINK_DC`/`WC3_BINK_DCPRED` (DC predictor values), `WC3_BINK_TRACE`/
//! `WC3_BINK_TRACE2` (coefficient trace).

pub mod avi;
pub mod bitreader;
pub mod error;
pub mod idct;
pub mod mp3;
pub mod mpeg4;
pub mod tables;
pub mod vlc;

pub use avi::{AudioInfo, Avi, VideoInfo};
pub use error::BinkError;
pub use mpeg4::vol::{PictureType, Vol};

use mpeg4::Mpeg4Decoder;

/// Number of decoded RGBA frames kept resident for the renderer.
pub const FRAME_RING: usize = 3;

/// Largest picture the movie path will allocate, in pixels (WC3-BINK-001).
///
/// The ring is three RGBA frames, so this caps one movie at 48 MiB of frame
/// buffers. The largest shipped cinematic is 800x368 (294 400 px) and 1080p is
/// 2 073 600 px, so the ceiling leaves room for any real movie while a
/// 338-byte file can no longer declare 65 535x65 535 (17 GB per frame).
pub const MAX_MOVIE_PIXELS: usize = 2048 * 2048;

/// Descriptor of a decoded frame; the pixels are in [`BinkFile::frame_rgba`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoFrame {
    pub index: usize,
    pub width: usize,
    pub height: usize,
}

impl VideoFrame {
    pub fn byte_len(&self) -> usize {
        self.width * self.height * 4
    }
}

/// A movie file: AVI container + MPEG-4 video (+ MP3 audio).
pub struct BinkFile {
    avi: Avi,
    decoder: Mpeg4Decoder,
    /// Ring of decoded RGBA frames; `slot_frame[i]` is `usize::MAX` when free.
    slot_rgba: [Vec<u8>; FRAME_RING],
    slot_frame: [usize; FRAME_RING],
    next_slot: usize,
    /// Next video chunk to feed the decoder (frames must be decoded in order).
    next_frame: usize,
    last_decoded: Option<VideoFrame>,
    /// Whole-track PCM, decoded once by [`BinkFile::audio_pcm`].
    audio: Option<Vec<i16>>,
}

impl BinkFile {
    /// Open a movie from disk.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, BinkError> {
        let avi = Avi::open(path)?;
        Self::from_avi(avi)
    }

    /// Build from bytes already in memory (e.g. read out of an archive).
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, BinkError> {
        let avi = Avi::from_bytes(bytes)?;
        Self::from_avi(avi)
    }

    fn from_avi(avi: Avi) -> Result<Self, BinkError> {
        if avi.video_frame_count() == 0 {
            return Err(BinkError::Corrupt("AVI has no video frames"));
        }
        let (w, h) = (avi.video().width as usize, avi.video().height as usize);
        if w == 0 || h == 0 {
            return Err(BinkError::Corrupt("AVI video stream has zero dimensions"));
        }
        // Validate the untrusted `strf` dimensions before they size the ring
        // (WC3-BINK-001): three `w*h*4` buffers is 48 MiB at most.
        let pixels = w
            .checked_mul(h)
            .ok_or(BinkError::Corrupt("AVI video dimensions overflow"))?;
        if pixels > MAX_MOVIE_PIXELS {
            return Err(BinkError::Corrupt("AVI video dimensions out of range"));
        }
        let rgba_len = pixels * 4;
        Ok(Self {
            avi,
            decoder: Mpeg4Decoder::new(),
            slot_rgba: std::array::from_fn(|_| vec![0u8; rgba_len]),
            slot_frame: [usize::MAX; FRAME_RING],
            next_slot: 0,
            next_frame: 0,
            last_decoded: None,
            audio: None,
        })
    }

    /// Video stream description.
    pub fn video(&self) -> &VideoInfo {
        self.avi.video()
    }

    /// Audio stream description, when the file has one.
    pub fn audio_info(&self) -> Option<&AudioInfo> {
        self.avi.audio()
    }

    /// Frames declared by the container.
    pub fn frame_count(&self) -> usize {
        self.avi.video_frame_count()
    }

    /// `(width, height)` in pixels.
    pub fn size(&self) -> (u32, u32) {
        (
            u32::from(self.avi.video().width),
            u32::from(self.avi.video().height),
        )
    }

    /// Nominal frame rate as a rational `(numerator, denominator)` seconds.
    pub fn fps(&self) -> (u32, u32) {
        let v = self.avi.video();
        (v.rate, v.scale)
    }

    /// Duration in seconds, from the declared frame count and rate.
    pub fn duration_secs(&self) -> f64 {
        let v = self.avi.video();
        if v.rate == 0 {
            return 0.0;
        }
        self.frame_count() as f64 * f64::from(v.scale) / f64::from(v.rate)
    }

    /// The MPEG-4 VOL header once the first frame has been decoded.
    pub fn vol(&self) -> Option<&Vol> {
        self.decoder.vol()
    }

    /// Macroblocks the MPEG-4 decoder concealed after a damaged block.
    ///
    /// A damaged frame is reconstructed from the motion-compensated reference
    /// (the same recovery the reference decoder performs) and counted here, so
    /// a caller can tell "decoded" from "partially concealed".
    pub fn concealed_macroblocks(&self) -> u64 {
        self.decoder.concealed_macroblocks()
    }

    /// `(mb_x, mb_y, reason)` of the last concealed macroblock, if any.
    pub fn last_decode_error(&self) -> Option<(usize, usize, &str)> {
        self.decoder.last_error()
    }

    /// The decoded picture of the frame decoded last (YUV planes + side data).
    pub fn picture(&self) -> &mpeg4::frame::Picture {
        self.decoder.picture()
    }

    /// The VOP header of the frame decoded last (`None` before the first).
    pub fn last_vop(&self) -> Option<&mpeg4::vol::Vop> {
        self.decoder.last_vop()
    }

    /// Decode frame `index` (in presentation order) into the ring buffer.
    ///
    /// Decoding is sequential: requesting a frame beyond the last decoded one
    /// decodes everything in between, which is what a player does anyway.
    /// A frame already in the ring is returned without re-decoding.
    pub fn decode_frame(&mut self, index: usize) -> Result<VideoFrame, BinkError> {
        if index >= self.frame_count() {
            return Err(BinkError::NoSuchFrame(index));
        }
        if let Some(frame) = self.cached(index) {
            self.last_decoded = Some(frame);
            return Ok(frame);
        }
        // A request behind the window forces a restart from the last keyframe
        // (the first frame), which keeps seeking correct without an index map.
        if index < self.next_frame {
            self.reset();
        }
        while self.next_frame <= index {
            let payload = self
                .avi
                .video_frame(self.next_frame)
                .ok_or(BinkError::NoSuchFrame(self.next_frame))?
                .to_vec();
            self.decoder.decode(&payload)?;
            // The ring is sized from the AVI header, the picture from the
            // stream's VOL: a disagreement is a corrupt movie, not a panic
            // (WC3-BINK-002).
            let (pw, ph) = {
                let pic = self.decoder.picture();
                (pic.width, pic.height)
            };
            let (aw, ah) = self.size();
            if pw != aw as usize || ph != ah as usize {
                return Err(BinkError::Corrupt("AVI/VOL dimension mismatch"));
            }
            let slot = self.next_slot;
            self.next_slot = (self.next_slot + 1) % FRAME_RING;
            {
                let pic = self.decoder.picture();
                pic.to_rgba(&mut self.slot_rgba[slot])?;
            }
            self.slot_frame[slot] = self.next_frame;
            self.next_frame += 1;
        }
        let frame = self
            .cached(index)
            .ok_or(BinkError::NoSuchFrame(index))?;
        self.last_decoded = Some(frame);
        Ok(frame)
    }

    /// Decode one payload in isolation (no reference frames).
    ///
    /// Only meaningful for I-frames, which carry everything they need; used by
    /// the analysis tools to inspect a mid-stream intra frame without decoding
    /// the whole prefix. Returns [`BinkError::Corrupt`] if the frame needs a
    /// reference.
    pub fn decode_frame_isolated(&mut self, index: usize) -> Result<VideoFrame, BinkError> {
        let payload = self
            .avi
            .video_frame(index)
            .ok_or(BinkError::NoSuchFrame(index))?
            .to_vec();
        self.decoder.decode(&payload)?;
        let (pw, ph) = {
            let pic = self.decoder.picture();
            (pic.width, pic.height)
        };
        let (aw, ah) = self.size();
        if pw != aw as usize || ph != ah as usize {
            return Err(BinkError::Corrupt("AVI/VOL dimension mismatch"));
        }
        let slot = self.next_slot;
        self.next_slot = (self.next_slot + 1) % FRAME_RING;
        {
            let pic = self.decoder.picture();
            pic.to_rgba(&mut self.slot_rgba[slot])?;
        }
        self.slot_frame[slot] = index;
        let (w, h) = self.size();
        let frame = VideoFrame {
            index,
            width: w as usize,
            height: h as usize,
        };
        self.last_decoded = Some(frame);
        Ok(frame)
    }

    fn cached(&self, index: usize) -> Option<VideoFrame> {
        for &frame in self.slot_frame.iter() {
            if frame == index {
                let (w, h) = self.size();
                return Some(VideoFrame {
                    index,
                    width: w as usize,
                    height: h as usize,
                });
            }
        }
        None
    }

    /// RGBA8 pixels of a frame currently in the ring buffer.
    pub fn frame_rgba(&self, index: usize) -> &[u8] {
        for (slot, &frame) in self.slot_frame.iter().enumerate() {
            if frame == index {
                return &self.slot_rgba[slot];
            }
        }
        &[]
    }

    /// The most recently decoded frame.
    pub fn last_frame(&self) -> Option<VideoFrame> {
        self.last_decoded
    }

    /// Drop decoder state and the frame ring (used by seeking).
    pub fn reset(&mut self) {
        self.decoder = Mpeg4Decoder::new();
        self.slot_frame = [usize::MAX; FRAME_RING];
        self.next_slot = 0;
        self.next_frame = 0;
        self.last_decoded = None;
    }

    /// Access to the demuxer (audio chunks live here).
    pub fn avi(&self) -> &Avi {
        &self.avi
    }

    /// Decode the whole MPEG-1 Layer III track to interleaved `i16` PCM.
    ///
    /// The result is cached: repeated calls return the same samples. The
    /// sample rate is `audio_info().sample_rate` and the audio runs for
    /// `len() / channels` frames per channel.
    pub fn audio_pcm(&mut self) -> Result<&[i16], BinkError> {
        if self.audio.is_none() {
            if let Some(info) = self.avi.audio() {
                if info.format_tag != 0x0055 {
                    return Err(BinkError::Unsupported("audio: not MPEG Layer III"));
                }
            }
            let mut stream: Vec<u8> = Vec::new();
            for chunk in self.avi.audio_chunks() {
                stream.extend_from_slice(chunk);
            }
            let mut decoder = mp3::Mp3Decoder::new();
            let mut pcm = Vec::with_capacity(stream.len() * 4);
            let mut rest: &[u8] = &stream;
            while !rest.is_empty() {
                match decoder.decode(rest, &mut pcm) {
                    Ok(used) if used > 0 && used <= rest.len() => rest = &rest[used..],
                    Ok(_) => break,
                    // A trailing partial frame is the normal end of an AVI
                    // audio stream; anything earlier is a real defect.
                    Err(BinkError::Corrupt("mp3: no frame sync")) if rest.len() < 4096 => break,
                    Err(err) => return Err(err),
                }
            }
            self.audio = Some(pcm);
        }
        self.audio
            .as_deref()
            .ok_or(BinkError::Corrupt("audio cache missing"))
    }

    /// Sample rate of [`BinkFile::audio_pcm`], or 0 when there is no audio.
    pub fn audio_sample_rate(&self) -> u32 {
        self.avi.audio().map_or(0, |a| a.sample_rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_garbage_bytes() {
        assert!(BinkFile::from_bytes(vec![0u8; 16]).is_err());
        assert!(BinkFile::from_bytes(b"RIFF\0\0\0\0AVI ".to_vec()).is_err());
    }
}
