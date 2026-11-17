//! MP3 decoding and fast header probing via `nanomp3` (pure Rust, no `unsafe`).
//!
//! Warcraft III's music and many voice lines are MPEG-1 Layer III at 44.1 kHz,
//! 128 or 192 kbit/s (1,140 files in `War3.mpq`). Two paths exist:
//!
//! * [`probe_mp3`] walks frame headers only — no decode, no big allocation —
//!   so the catalog can report exact frame counts and durations for the whole
//!   corpus.
//! * [`Mp3Stream`] decodes one frame at a time into the caller's buffer, so a
//!   four-minute music track never becomes a 40 MB `Vec<f32>` in memory.

use std::io::Cursor;
use std::sync::Arc;

use nanomp3::{frames, SliceReader};

use crate::{AudioError, Sound};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mp3Info {
    pub sample_rate: u32,
    pub channels: u16,
    pub layer: u8,
    pub frames: usize,
    pub samples_per_channel: u64,
    pub duration_ms: u64,
    pub bitrate_avg_kbps: u32,
    pub bitrate_min_kbps: u32,
    pub bitrate_max_kbps: u32,
    pub first_frame_offset: usize,
    pub id3v2_bytes: usize,
    pub has_vbr_tag: bool,
}

impl Mp3Info {
    pub fn duration_seconds(&self) -> f32 {
        self.duration_ms as f32 / 1000.0
    }
}

/// Parse frame headers without decoding audio.
pub fn probe_mp3(bytes: &[u8]) -> Result<Mp3Info, AudioError> {
    let id3v2 = nanomp3::id3v2_len(bytes);
    let table = frames(bytes);
    let mut info = Mp3Info {
        sample_rate: 0,
        channels: 0,
        layer: 0,
        frames: 0,
        samples_per_channel: 0,
        duration_ms: 0,
        bitrate_avg_kbps: 0,
        bitrate_min_kbps: u32::MAX,
        bitrate_max_kbps: 0,
        first_frame_offset: 0,
        id3v2_bytes: id3v2,
        has_vbr_tag: false,
    };
    let mut bitrate_sum: u64 = 0;
    for frame in table {
        if info.frames == 0 {
            info.sample_rate = frame.sample_rate;
            info.channels = match frame.channels {
                nanomp3::Channels::Mono => 1,
                nanomp3::Channels::Stereo => 2,
            };
            info.layer = frame.layer;
            info.first_frame_offset = frame.offset;
        }
        info.frames += 1;
        info.samples_per_channel += frame.samples as u64;
        bitrate_sum += frame.bitrate_kbps as u64;
        info.bitrate_min_kbps = info.bitrate_min_kbps.min(frame.bitrate_kbps);
        info.bitrate_max_kbps = info.bitrate_max_kbps.max(frame.bitrate_kbps);
    }
    if info.frames == 0 || info.sample_rate == 0 {
        return Err(AudioError::Corrupt("no MP3 frames found"));
    }
    info.bitrate_avg_kbps = (bitrate_sum / info.frames as u64) as u32;
    if info.bitrate_min_kbps == u32::MAX {
        info.bitrate_min_kbps = 0;
    }
    info.duration_ms = info.samples_per_channel * 1000 / info.sample_rate as u64;
    // The VBR/Xing header lives in the first frame and is skipped by `frames`.
    info.has_vbr_tag = bytes
        .get(info.first_frame_offset..info.first_frame_offset + 64)
        .map(|window| {
            window.windows(4).any(|w| w == b"Xing" || w == b"Info")
        })
        .unwrap_or(false);
    Ok(info)
}

/// Decode a whole MP3 into memory. Intended for short one-shots; music must use
/// [`Mp3Stream`] to stay inside a memory budget.
pub fn decode_mp3(bytes: &[u8]) -> Result<Sound, AudioError> {
    let decoded = nanomp3::decode_all::<f32>(bytes);
    let channels = match decoded.channels {
        Some(nanomp3::Channels::Mono) => 1,
        Some(nanomp3::Channels::Stereo) => 2,
        None => return Err(AudioError::Corrupt("MP3 has no audio frames")),
    };
    if decoded.samples.is_empty() {
        return Err(AudioError::Corrupt("MP3 decoded to zero samples"));
    }
    Ok(Sound {
        sample_rate: decoded.sample_rate,
        channels,
        samples: decoded.samples,
    })
}

/// Fully decode an MP3 but stop after `max_samples_per_channel` samples: the
/// bounded path for previews and sound tests.
pub fn decode_mp3_bounded(
    bytes: &[u8],
    max_samples_per_channel: usize,
) -> Result<(Sound, bool), AudioError> {
    let mut reader = SliceReader::<f32>::new(bytes);
    let channels = match reader.channels() {
        Some(nanomp3::Channels::Mono) => 1u16,
        Some(nanomp3::Channels::Stereo) => 2u16,
        None => return Err(AudioError::Corrupt("MP3 has no audio frames")),
    };
    let sample_rate = reader.sample_rate();
    if sample_rate == 0 {
        return Err(AudioError::Corrupt("MP3 sample rate is zero"));
    }
    let want = max_samples_per_channel.saturating_mul(channels as usize);
    let mut samples = Vec::with_capacity(want.min(1 << 20));
    let mut scratch = vec![0f32; 4096];
    let mut truncated = false;
    while samples.len() < want {
        let n = reader.read(&mut scratch).map_err(|_| AudioError::Corrupt("MP3 decode failed"))?;
        if n == 0 {
            break;
        }
        samples.extend_from_slice(&scratch[..n]);
    }
    if samples.len() > want {
        samples.truncate(want);
        truncated = true;
    }
    if samples.len() < want {
        truncated = false;
    }
    Ok((
        Sound {
            sample_rate,
            channels,
            samples,
        },
        truncated,
    ))
}

/// Streaming MP3 source: owns the compressed bytes and decodes on demand.
pub struct Mp3Stream {
    reader: nanomp3::Reader<Cursor<Arc<[u8]>>, f32>,
    channels: u16,
    sample_rate: u32,
    total_samples: Option<u64>,
    /// Scratch buffer reused for every `read`; allocated once.
    scratch: Vec<f32>,
    scratch_frames: usize,
    scratch_pos: usize,
    scratch_len: usize,
    finished: bool,
}

impl std::fmt::Debug for Mp3Stream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mp3Stream")
            .field("channels", &self.channels)
            .field("sample_rate", &self.sample_rate)
            .field("total_samples", &self.total_samples)
            .field("finished", &self.finished)
            .finish()
    }
}

impl Mp3Stream {
    pub fn new(bytes: Arc<[u8]>) -> Result<Self, AudioError> {
        let reader = nanomp3::Reader::<_, f32>::new(Cursor::new(bytes))
            .map_err(|_| AudioError::Corrupt("MP3 stream open failed"))?;
        let channels = match reader.channels() {
            Some(nanomp3::Channels::Mono) => 1u16,
            Some(nanomp3::Channels::Stereo) => 2u16,
            None => return Err(AudioError::Corrupt("MP3 stream has no frames")),
        };
        let sample_rate = reader.sample_rate();
        let total_samples = reader.total_samples();
        Ok(Self {
            reader,
            channels,
            sample_rate,
            total_samples,
            scratch: vec![0f32; 4096],
            scratch_frames: 0,
            scratch_pos: 0,
            scratch_len: 0,
            finished: false,
        })
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn total_frames(&self) -> Option<u64> {
        self.total_samples
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn rewind(&mut self) -> Result<(), AudioError> {
        self.reader
            .seek(0)
            .map_err(|_| AudioError::Corrupt("MP3 rewind failed"))?;
        self.scratch_frames = 0;
        self.scratch_pos = 0;
        self.scratch_len = 0;
        self.finished = false;
        Ok(())
    }

    /// Fill `out` with interleaved f32 samples. Returns the number written;
    /// zero means end of stream. Never allocates after construction.
    pub fn read(&mut self, out: &mut [f32]) -> usize {
        let mut written = 0usize;
        while written < out.len() {
            if self.scratch_pos >= self.scratch_len {
                if self.finished {
                    break;
                }
                match self.reader.read(&mut self.scratch) {
                    Ok(0) => {
                        self.finished = true;
                        break;
                    }
                    Ok(n) => {
                        self.scratch_pos = 0;
                        self.scratch_len = n;
                        self.scratch_frames = n / self.channels as usize;
                    }
                    Err(_) => {
                        self.finished = true;
                        break;
                    }
                }
            }
            let take = (self.scratch_len - self.scratch_pos).min(out.len() - written);
            out[written..written + take]
                .copy_from_slice(&self.scratch[self.scratch_pos..self.scratch_pos + take]);
            self.scratch_pos += take;
            written += take;
        }
        written
    }

    /// Skip `frames` frames of output (used when resuming a paused voice).
    pub fn skip_frames(&mut self, mut frames: u64) -> u64 {
        let mut scratch = [0f32; 512];
        let mut skipped = 0u64;
        while frames > 0 {
            let chunk = (frames as usize).min(scratch.len());
            let n = self.read(&mut scratch[..chunk]);
            if n == 0 {
                break;
            }
            let produced = (n / self.channels as usize) as u64;
            skipped += produced;
            frames = frames.saturating_sub(produced);
        }
        skipped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_mp3() -> Option<PathBuf> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/Sound/Music/mp3Music/Mainscreen.mp3");
        path.is_file().then_some(path)
    }

    #[test]
    fn probes_real_music_headers() {
        let Some(path) = sample_mp3() else {
            eprintln!("skipping: extracted music not available");
            return;
        };
        let bytes = std::fs::read(&path).expect("read");
        let info = probe_mp3(&bytes).expect("probe");
        println!("{info:?}");
        assert_eq!(info.layer, 3, "MPEG-1 Layer III");
        assert_eq!(info.sample_rate, 44_100);
        assert_eq!(info.channels, 2);
        assert!(info.frames > 1000, "frames {}", info.frames);
        assert!(info.bitrate_avg_kbps >= 96, "bitrate {}", info.bitrate_avg_kbps);
        // Mainscreen.mp3 is ~69 s at 112 kbit/s (945 KB).
        assert!(
            (60_000..80_000).contains(&info.duration_ms),
            "duration {} ms",
            info.duration_ms
        );
        assert!(info.first_frame_offset < 4096);
    }

    #[test]
    fn decodes_bounded_and_streaming_with_same_samples() {
        let Some(path) = sample_mp3() else {
            return;
        };
        let bytes = Arc::<[u8]>::from(std::fs::read(&path).expect("read"));
        let (bounded, truncated) = decode_mp3_bounded(&bytes, 44_100).expect("bounded decode");
        assert!(truncated, "one second of a longer track must truncate");
        assert_eq!(bounded.samples.len(), 44_100 * 2);
        let mut stream = Mp3Stream::new(bytes.clone()).expect("stream");
        let mut out = vec![0f32; 44_100 * 2];
        let n = stream.read(&mut out);
        assert_eq!(n, 44_100 * 2);
        // Both paths decode the same first second.
        let diff = bounded
            .samples
            .iter()
            .zip(&out)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        println!("bounded vs streaming max diff {diff}");
        assert!(diff < 1e-6, "decode paths disagree: {diff}");
    }

    #[test]
    fn rejects_non_mp3() {
        assert!(probe_mp3(b"not an mp3").is_err());
        assert!(decode_mp3(b"\x00\x01\x02\x03").is_err());
    }
}
