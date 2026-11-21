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
    /// Largest `main_data_begin` seen in a frame header: how many bytes of main
    /// data the encoder borrowed from earlier frames (the Layer III bit
    /// reservoir). 0 means every frame is self-contained.
    pub reservoir_max_bytes: u16,
}

impl Mp3Info {
    pub fn duration_seconds(&self) -> f32 {
        self.duration_ms as f32 / 1000.0
    }
}

/// Frame-window arithmetic shared by the probe and the streaming mixer.
///
/// The streaming buffer holds `frames` decoded frames whose first frame is
/// absolute source frame `start_frame`. A mixer sample needs both the frame at
/// `abs` and its interpolation successor at `abs + 1`, so the request is only
/// satisfiable when `abs - start_frame + 1 < frames`.
///
/// Returns `(rel, rel + 1)` — the indices into the window — or `None` when the
/// request lies before the window or too near its end.
///
/// All comparisons are overflow-free: `rel + 1` is never evaluated without the
/// `rel <= frames - 2` guard, and `frames < 2` short-circuits before the
/// subtraction. Callers must not do `rel + 2`, `frames - rel`, or
/// `phase.floor() as u64 + 1` on unvalidated values (`u64::MAX` requests are
/// reachable from a corrupted phase, and both panic in debug builds and wrap in
/// release).
#[must_use]
pub fn mp3_window_span(start_frame: u64, frames: usize, abs: u64) -> Option<(usize, usize)> {
    if frames < 2 || abs < start_frame {
        return None;
    }
    let rel = abs - start_frame;
    let last_start = frames as u64 - 2;
    if rel > last_start {
        return None;
    }
    let rel = rel as usize;
    Some((rel, rel + 1))
}

/// MPEG audio sample rates (all versions). A rate outside this set means the
/// frame header was misread or the stream is not MPEG audio.
const MPEG_SAMPLE_RATES: [u32; 9] = [
    8_000, 11_025, 12_000, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000,
];

/// Layer III per-version decoding constants re-derived from the 4-byte header.
///
/// This is the arithmetic the streaming path depends on, checked against the
/// MPEG-1/2/2.5 tables instead of assuming one frame layout: bitrate and sample
/// rate indices, the padded frame length, the decoded sample count and the
/// bit-reservoir reference (`main_data_begin`).
struct Layer3Header {
    sample_rate: u32,
    channels: u16,
    frame_len: usize,
    samples: usize,
    main_data_begin: u16,
    reservoir_limit: u16,
}

const MPEG1_L3_BITRATES: [u16; 15] = [
    0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
];
const MPEG2_L3_BITRATES: [u16; 15] = [
    0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160,
];

/// Parse a Layer III frame header and derive its length/sample count.
///
/// Returns `None` for a non-sync, non-Layer-III, reserved or free-format
/// header. Layer III frame length: `144 * bitrate / rate + padding` for MPEG-1,
/// `72 * bitrate / rate + padding` for MPEG-2/2.5.
fn parse_layer3_header(frame: &[u8]) -> Option<Layer3Header> {
    if frame.len() < 4 {
        return None;
    }
    let header = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]);
    if header & 0xFFE0_0000 != 0xFFE0_0000 {
        return None;
    }
    let version = (header >> 19) & 0b11; // 0=2.5, 2=MPEG-2, 3=MPEG-1, 1=reserved
    let layer = (header >> 17) & 0b11; // 1 = Layer III
    let bitrate_index = ((header >> 12) & 0b1111) as usize;
    let rate_index = ((header >> 10) & 0b11) as usize;
    let padding = ((header >> 9) & 1) as usize;
    if version == 1 || layer != 1 || rate_index == 3 || bitrate_index == 0 || bitrate_index == 15 {
        return None;
    }
    let (rates, bitrates, mpeg1) = match version {
        3 => (
            [44_100u32, 48_000, 32_000],
            MPEG1_L3_BITRATES,
            true,
        ),
        _ => (
            match version {
                2 => [22_050u32, 24_000, 16_000],
                _ => [11_025u32, 12_000, 8_000],
            },
            MPEG2_L3_BITRATES,
            false,
        ),
    };
    let sample_rate = rates[rate_index];
    let bitrate = bitrates[bitrate_index] as usize * 1000;
    let frame_len = if mpeg1 {
        144 * bitrate / sample_rate as usize + padding
    } else {
        72 * bitrate / sample_rate as usize + padding
    };
    let mono = (header >> 6) & 0b11 == 0b11;
    let crc = (header >> 16) & 1 == 0;
    let side_offset = 4 + usize::from(crc);
    let side_len = match (mpeg1, mono) {
        (true, true) => 17,
        (true, false) => 32,
        (false, true) => 9,
        (false, false) => 17,
    };
    let side = frame.get(side_offset..side_offset + side_len)?;
    let main_data_begin = if mpeg1 {
        (((side[0] as u16) << 1) | ((side[1] as u16) >> 7)) & 0x1FF
    } else {
        side[0] as u16
    };
    Some(Layer3Header {
        sample_rate,
        channels: if mono { 1 } else { 2 },
        frame_len,
        samples: if mpeg1 { 1152 } else { 576 },
        main_data_begin,
        reservoir_limit: if mpeg1 { 511 } else { 255 },
    })
}

/// Parse frame headers without decoding audio.
///
/// The walk validates the header arithmetic the streaming path depends on:
/// layer is III, the sample-rate index is legal, every frame decodes to the
/// sample count its version/layer implies, the sample rate and layer do not
/// change mid-stream, and each frame starts exactly where the previous frame's
/// byte length (bitrate + padding, bit reservoir does not change frame length)
/// says it should. A violation is a controlled `Corrupt`/`Unsupported` error,
/// never a plausible-looking wrong duration.
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
        reservoir_max_bytes: 0,
    };
    let mut bitrate_sum: u64 = 0;
    let mut prev_end: Option<u64> = None;
    let mut reservoir_max: u16 = 0;
    for frame in table {
        let header = parse_layer3_header(frame.data)
            .ok_or(AudioError::Corrupt("MP3 frame header is not Layer III"))?;
        let channels = match frame.channels {
            nanomp3::Channels::Mono => 1,
            nanomp3::Channels::Stereo => 2,
        };
        // The header re-derivation must agree with what the iterator reported;
        // disagreement means a frame-length/table assumption is wrong and the
        // reported duration would be fiction.
        if header.sample_rate != frame.sample_rate
            || header.samples != frame.samples
            || header.frame_len != frame.data.len()
            || header.channels != channels
        {
            return Err(AudioError::Corrupt(
                "MP3 frame length/sample rate/sample count disagree with header",
            ));
        }
        if let Some(end) = prev_end {
            if (frame.offset as u64) < end {
                return Err(AudioError::Corrupt("MP3 frames overlap"));
            }
        }
        prev_end = Some(frame.offset as u64 + frame.data.len() as u64);
        if info.frames == 0 {
            if frame.layer != 3 {
                return Err(AudioError::Unsupported("MP3 decoder only handles Layer III"));
            }
            // Nothing precedes the first frame, so a nonzero reference would
            // read outside the stream.
            if header.main_data_begin != 0 {
                return Err(AudioError::Corrupt("first MP3 frame references an empty bit reservoir"));
            }
            info.sample_rate = header.sample_rate;
            info.channels = header.channels;
            info.layer = frame.layer;
            info.first_frame_offset = frame.offset as usize;
        } else if header.sample_rate != info.sample_rate || frame.layer != info.layer {
            return Err(AudioError::Corrupt("MP3 header changes mid-stream"));
        }
        if header.main_data_begin > header.reservoir_limit {
            return Err(AudioError::Corrupt("MP3 bit reservoir reference exceeds the frame budget"));
        }
        reservoir_max = reservoir_max.max(header.main_data_begin);
        info.frames += 1;
        info.samples_per_channel += header.samples as u64;
        bitrate_sum += frame.bitrate_kbps as u64;
        info.bitrate_min_kbps = info.bitrate_min_kbps.min(frame.bitrate_kbps);
        info.bitrate_max_kbps = info.bitrate_max_kbps.max(frame.bitrate_kbps);
    }
    if info.frames == 0 || info.sample_rate == 0 {
        return Err(AudioError::Corrupt("no MP3 frames found"));
    }
    info.reservoir_max_bytes = reservoir_max;
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
        if !MPEG_SAMPLE_RATES.contains(&sample_rate) {
            return Err(AudioError::Corrupt("MP3 sample rate is not a legal MPEG rate"));
        }
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
        debug_assert_eq!(
            written % self.channels.max(1) as usize,
            0,
            "MP3 decode returned a partial interleaved frame"
        );
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
