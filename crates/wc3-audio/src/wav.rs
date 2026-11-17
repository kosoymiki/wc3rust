//! RIFF/WAVE decoding: PCM (8/16/24/32-bit), IEEE float, IMA ADPCM and MS ADPCM.
//!
//! Warcraft III ships 2,696 `.wav` files; all of them are PCM16 in `War3.mpq`,
//! but maps and mods routinely carry ADPCM, so both block codecs are decoded
//! here. The IMA tables and block layout follow the IMA/DVI specification
//! (MultimediaWiki, accessed 2026-11-12) and the MS ADPCM tables follow the
//! Microsoft variant as documented there.
//!
//! `probe_wav` reports header fields and the exact frame count without
//! allocating a sample buffer, which is what the asset catalog uses to validate
//! every sound.

use crate::{AudioError, Sound};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WavCodec {
    Pcm8,
    Pcm16,
    Pcm24,
    Pcm32,
    Float32,
    Float64,
    ImaAdpcm,
    MsAdpcm,
    Unknown(u16),
}

impl WavCodec {
    pub fn name(self) -> &'static str {
        match self {
            Self::Pcm8 => "pcm-u8",
            Self::Pcm16 => "pcm-s16",
            Self::Pcm24 => "pcm-s24",
            Self::Pcm32 => "pcm-s32",
            Self::Float32 => "float32",
            Self::Float64 => "float64",
            Self::ImaAdpcm => "ima-adpcm",
            Self::MsAdpcm => "ms-adpcm",
            Self::Unknown(_) => "unknown",
        }
    }

    pub fn is_adpcm(self) -> bool {
        matches!(self, Self::ImaAdpcm | Self::MsAdpcm)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WavInfo {
    pub codec: WavCodec,
    pub format_tag: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub block_align: u16,
    pub samples_per_block: u16,
    pub data_bytes: usize,
    pub frames: u64,
    pub duration_ms: u64,
    /// RIFF length field disagreed with the file size (truncated/padded file).
    pub riff_mismatch: bool,
    pub chunks: u32,
}

impl WavInfo {
    pub fn duration_seconds(&self) -> f32 {
        self.duration_ms as f32 / 1000.0
    }
}

/// IMA ADPCM step table (89 entries, IMA/DVI specification).
pub(crate) const IMA_STEP: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

pub(crate) const IMA_INDEX: [i8; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

const MS_ADAPT: [i32; 16] = [
    230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230,
];
const MS_COEFF1: [i32; 7] = [256, 512, 0, 192, 240, 460, 392];
const MS_COEFF2: [i32; 7] = [0, -256, 0, 64, 0, -208, -232];

struct Chunk<'a> {
    id: [u8; 4],
    data: &'a [u8],
}

fn chunks(bytes: &[u8]) -> Result<(Vec<Chunk<'_>>, bool), AudioError> {
    if bytes.len() >= 4 && &bytes[..4] != b"RIFF" {
        return Err(AudioError::BadMagic);
    }
    if bytes.len() < 12 {
        return Err(AudioError::TooShort);
    }
    if &bytes[8..12] != b"WAVE" {
        return Err(AudioError::BadMagic);
    }
    let riff_len = u32::from_le_bytes(bytes[4..8].try_into().expect("4")) as usize;
    let mismatch = riff_len + 8 != bytes.len();
    let mut out = Vec::new();
    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let id: [u8; 4] = bytes[pos..pos + 4].try_into().expect("4");
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().expect("4")) as usize;
        pos += 8;
        let end = pos.checked_add(size).ok_or(AudioError::Corrupt("chunk overflow"))?;
        // A truncated final chunk is common in mods; keep what is present.
        let data = &bytes[pos..end.min(bytes.len())];
        out.push(Chunk { id, data });
        pos = end + (size & 1);
    }
    Ok((out, mismatch))
}

/// Parse the WAV header without decoding samples.
pub fn probe_wav(bytes: &[u8]) -> Result<WavInfo, AudioError> {
    let (chunks, riff_mismatch) = chunks(bytes)?;
    let mut format: Option<WavInfo> = None;
    let mut data_bytes = 0usize;
    for chunk in &chunks {
        if &chunk.id == b"fmt " {
            let info = parse_fmt(chunk.data, riff_mismatch, chunks.len() as u32)?;
            format = Some(info);
        } else if &chunk.id == b"data" {
            data_bytes = chunk.data.len();
        }
    }
    let mut info = format.ok_or(AudioError::Corrupt("missing fmt chunk"))?;
    info.data_bytes = data_bytes;
    let frames = match info.codec {
        WavCodec::ImaAdpcm | WavCodec::MsAdpcm => {
            let spb = info.samples_per_block.max(1) as u64;
            let blocks = (data_bytes as u64).div_ceil(info.block_align.max(1) as u64);
            blocks * spb
        }
        _ => {
            let frame_bytes = (info.channels as usize)
                .checked_mul((info.bits_per_sample as usize).div_ceil(8))
                .filter(|v| *v > 0)
                .ok_or(AudioError::Corrupt("zero frame size"))?;
            (data_bytes / frame_bytes) as u64
        }
    };
    info.frames = frames;
    info.duration_ms = if info.sample_rate == 0 {
        0
    } else {
        frames * 1000 / info.sample_rate as u64
    };
    Ok(info)
}

fn parse_fmt(
    data: &[u8],
    riff_mismatch: bool,
    chunk_count: u32,
) -> Result<WavInfo, AudioError> {
    if data.len() < 16 {
        return Err(AudioError::Corrupt("fmt chunk too short"));
    }
    let mut tag = u16::from_le_bytes(data[0..2].try_into().expect("2"));
    let channels = u16::from_le_bytes(data[2..4].try_into().expect("2"));
    let sample_rate = u32::from_le_bytes(data[4..8].try_into().expect("4"));
    let block_align = u16::from_le_bytes(data[12..14].try_into().expect("2"));
    let bits = u16::from_le_bytes(data[14..16].try_into().expect("2"));
    let mut samples_per_block = 0u16;
    if tag == 0xFFFE {
        if data.len() < 40 {
            return Err(AudioError::Corrupt("extensible fmt chunk too short"));
        }
        // cbSize(2) validBits(2) channelMask(4) subformat GUID(16): the first
        // two GUID bytes are the effective format tag.
        tag = u16::from_le_bytes(data[24..26].try_into().expect("2"));
    }
    if tag == 0x11 || tag == 0x02 {
        if data.len() >= 20 {
            samples_per_block = u16::from_le_bytes(data[18..20].try_into().expect("2"));
        }
        if samples_per_block == 0 {
            // Derive it: block payload minus per-channel headers, two samples
            // per byte per channel, plus one initial sample.
            let header = 4u32 * channels.max(1) as u32;
            let payload = (block_align as u32).saturating_sub(header);
            samples_per_block =
                (payload * 2 / channels.max(1) as u32 + 1).min(u16::MAX as u32) as u16;
        }
    }
    if channels == 0 || channels > 8 {
        return Err(AudioError::Unsupported("WAV channel count"));
    }
    if sample_rate == 0 || sample_rate > 384_000 {
        return Err(AudioError::Unsupported("WAV sample rate"));
    }
    let codec = match tag {
        1 => match bits {
            8 => WavCodec::Pcm8,
            16 => WavCodec::Pcm16,
            24 => WavCodec::Pcm24,
            32 => WavCodec::Pcm32,
            _ => return Err(AudioError::Unsupported("PCM bit depth")),
        },
        3 => match bits {
            32 => WavCodec::Float32,
            64 => WavCodec::Float64,
            _ => return Err(AudioError::Unsupported("float bit depth")),
        },
        0x11 => WavCodec::ImaAdpcm,
        0x02 => WavCodec::MsAdpcm,
        other => WavCodec::Unknown(other),
    };
    Ok(WavInfo {
        codec,
        format_tag: tag,
        channels,
        sample_rate,
        bits_per_sample: bits,
        block_align,
        samples_per_block,
        data_bytes: 0,
        frames: 0,
        duration_ms: 0,
        riff_mismatch,
        chunks: chunk_count,
    })
}

/// Decode a WAV file to interleaved f32 samples in `[-1, 1]`.
pub fn decode_wav(bytes: &[u8]) -> Result<Sound, AudioError> {
    let (chunks, _) = chunks(bytes)?;
    let mut fmt: Option<&[u8]> = None;
    let mut data: &[u8] = &[];
    for chunk in &chunks {
        if &chunk.id == b"fmt " {
            fmt = Some(chunk.data);
        } else if &chunk.id == b"data" {
            data = chunk.data;
        }
    }
    let fmt = fmt.ok_or(AudioError::Corrupt("missing fmt chunk"))?;
    let info = parse_fmt(fmt, false, 0)?;
    match info.codec {
        WavCodec::Pcm8 => Ok(Sound {
            sample_rate: info.sample_rate,
            channels: info.channels,
            samples: data.iter().map(|&b| (b as f32 - 128.0) / 128.0).collect(),
        }),
        WavCodec::Pcm16 => Ok(Sound {
            sample_rate: info.sample_rate,
            channels: info.channels,
            samples: data
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes(c.try_into().expect("2")) as f32 / 32768.0)
                .collect(),
        }),
        WavCodec::Pcm24 => Ok(Sound {
            sample_rate: info.sample_rate,
            channels: info.channels,
            samples: data
                .chunks_exact(3)
                .map(|c| {
                    let raw = c[0] as i32 | (c[1] as i32) << 8 | (c[2] as i32) << 16;
                    let signed = if raw & 0x0080_0000 != 0 {
                        raw - 0x0100_0000
                    } else {
                        raw
                    };
                    signed as f32 / 8_388_608.0
                })
                .collect(),
        }),
        WavCodec::Pcm32 => Ok(Sound {
            sample_rate: info.sample_rate,
            channels: info.channels,
            samples: data
                .chunks_exact(4)
                .map(|c| i32::from_le_bytes(c.try_into().expect("4")) as f32 / 2_147_483_648.0)
                .collect(),
        }),
        WavCodec::Float32 => Ok(Sound {
            sample_rate: info.sample_rate,
            channels: info.channels,
            samples: data
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes(c.try_into().expect("4")).clamp(-1.0, 1.0))
                .collect(),
        }),
        WavCodec::Float64 => Ok(Sound {
            sample_rate: info.sample_rate,
            channels: info.channels,
            samples: data
                .chunks_exact(8)
                .map(|c| {
                    (f64::from_le_bytes(c.try_into().expect("8")) as f32).clamp(-1.0, 1.0)
                })
                .collect(),
        }),
        WavCodec::ImaAdpcm => decode_ima(&info, data),
        WavCodec::MsAdpcm => decode_ms(&info, data),
        WavCodec::Unknown(tag) => Err(AudioError::UnsupportedTag(tag)),
    }
}

/// Decode IMA ADPCM blocks. Each channel starts a block with
/// `(predictor i16, step_index u8, reserved u8)`, then nibbles alternate
/// channel by channel.
fn decode_ima(info: &WavInfo, data: &[u8]) -> Result<Sound, AudioError> {
    let channels = info.channels as usize;
    let block_align = info.block_align.max(4 * channels as u16) as usize;
    if block_align == 0 {
        return Err(AudioError::Corrupt("IMA ADPCM block size is zero"));
    }
    let mut out: Vec<f32> = Vec::with_capacity(info.frames as usize * channels);
    for block in data.chunks(block_align) {
        if block.len() < 4 * channels {
            break;
        }
        let mut predictor = [0i32; 8];
        let mut index = [0usize; 8];
        for ch in 0..channels {
            let base = ch * 4;
            predictor[ch] = i16::from_le_bytes([block[base], block[base + 1]]) as i32;
            index[ch] = (block[base + 2] as usize).min(88);
        }
        // First sample of each channel is the header predictor.
        for ch in 0..channels {
            out.push(predictor[ch] as f32 / 32768.0);
        }
        let payload = &block[4 * channels..];
        let mut ch = 0usize;
        for &byte in payload {
            for nibble in [byte & 0x0F, byte >> 4] {
                let step = IMA_STEP[index[ch]];
                let mut diff = step >> 3;
                if nibble & 4 != 0 {
                    diff += step;
                }
                if nibble & 2 != 0 {
                    diff += step >> 1;
                }
                if nibble & 1 != 0 {
                    diff += step >> 2;
                }
                if nibble & 8 != 0 {
                    predictor[ch] -= diff;
                } else {
                    predictor[ch] += diff;
                }
                predictor[ch] = predictor[ch].clamp(-32768, 32767);
                index[ch] = (index[ch] as i8 + IMA_INDEX[nibble as usize]).clamp(0, 88) as usize;
                out.push(predictor[ch] as f32 / 32768.0);
                ch += 1;
                if ch == channels {
                    ch = 0;
                }
            }
        }
    }
    Ok(Sound {
        sample_rate: info.sample_rate,
        channels: info.channels,
        samples: out,
    })
}

/// Decode MS ADPCM blocks: per-channel predictor/delta/sample1/sample2
/// preamble, then high nibble first for channel 0, low nibble for channel 1.
fn decode_ms(info: &WavInfo, data: &[u8]) -> Result<Sound, AudioError> {
    let channels = info.channels as usize;
    if channels > 2 {
        return Err(AudioError::Unsupported("MS ADPCM channels > 2"));
    }
    let header = 7 * channels;
    let block_align = (info.block_align as usize).max(header);
    let mut out: Vec<f32> = Vec::with_capacity(info.frames as usize * channels);
    let mut coeff1 = MS_COEFF1;
    let mut coeff2 = MS_COEFF2;
    // Coefficients can be overridden by the fmt chunk; the standard 7 are used
    // for predictor indices 0..6, which covers every retail file.
    let _ = (&mut coeff1, &mut coeff2);
    for block in data.chunks(block_align) {
        if block.len() < header {
            break;
        }
        let mut predictor_idx = [0usize; 2];
        let mut delta = [0i32; 2];
        let mut sample1 = [0i32; 2];
        let mut sample2 = [0i32; 2];
        for ch in 0..channels {
            let base = ch * 7;
            predictor_idx[ch] = (block[base] as usize).min(coeff1.len() - 1);
            delta[ch] = i16::from_le_bytes([block[base + 1], block[base + 2]]) as i32;
            sample1[ch] = i16::from_le_bytes([block[base + 3], block[base + 4]]) as i32;
            sample2[ch] = i16::from_le_bytes([block[base + 5], block[base + 6]]) as i32;
        }
        // Preamble samples are emitted in reverse order (sample2 then sample1).
        for ch in 0..channels {
            out.push(sample2[ch] as f32 / 32768.0);
        }
        for ch in 0..channels {
            out.push(sample1[ch] as f32 / 32768.0);
        }
        for &byte in &block[header..] {
            let nibbles = [byte >> 4, byte & 0x0F];
            for (slot, nibble) in nibbles.iter().enumerate() {
                if channels == 2 && slot >= channels {
                    break;
                }
                let ch = slot.min(channels - 1);
                let signed = if *nibble >= 8 {
                    *nibble as i32 - 16
                } else {
                    *nibble as i32
                };
                let c1 = coeff1[predictor_idx[ch]];
                let c2 = coeff2[predictor_idx[ch]];
                let mut predicted = (sample1[ch] * c1 + sample2[ch] * c2) / 256;
                predicted += signed * delta[ch];
                predicted = predicted.clamp(-32768, 32767);
                out.push(predicted as f32 / 32768.0);
                sample2[ch] = sample1[ch];
                sample1[ch] = predicted;
                delta[ch] = (MS_ADAPT[*nibble as usize] * delta[ch] / 256).max(16);
            }
        }
    }
    Ok(Sound {
        sample_rate: info.sample_rate,
        channels: info.channels,
        samples: out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_pcm16(sample_rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        let data_len = samples.len() as u32 * 2;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&sample_rate.to_le_bytes());
        out.extend_from_slice(&(sample_rate * channels as u32 * 2).to_le_bytes());
        out.extend_from_slice(&(channels * 2).to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for sample in samples {
            out.extend_from_slice(&sample.to_le_bytes());
        }
        out
    }

    fn wav_ima(sample_rate: u32, channels: u16, block_align: u16, samples_per_block: u16, blocks: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(4 + 8 + 20 + 8 + blocks.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&20u32.to_le_bytes());
        out.extend_from_slice(&0x11u16.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&sample_rate.to_le_bytes());
        out.extend_from_slice(&(sample_rate * channels as u32 * 2).to_le_bytes());
        out.extend_from_slice(&block_align.to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&samples_per_block.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(blocks.len() as u32).to_le_bytes());
        out.extend_from_slice(blocks);
        out
    }

    #[test]
    fn probes_and_decodes_pcm16() {
        let samples = [0i16, 1000, -1000, 32767, -32768, 0];
        let bytes = wav_pcm16(44100, 1, &samples);
        let info = probe_wav(&bytes).expect("probe");
        assert_eq!(info.codec, WavCodec::Pcm16);
        assert_eq!(info.frames, 6);
        assert_eq!(info.duration_ms, 0);
        let sound = decode_wav(&bytes).expect("decode");
        assert_eq!(sound.samples.len(), 6);
        assert!((sound.samples[3] - 32767.0 / 32768.0).abs() < 1e-6);
    }

    #[test]
    fn probes_duration_for_one_second() {
        let samples = vec![0i16; 22_050];
        let bytes = wav_pcm16(22_050, 1, &samples);
        let info = probe_wav(&bytes).expect("probe");
        assert_eq!(info.frames, 22_050);
        assert_eq!(info.duration_ms, 1000);
    }

    fn wav_custom(tag: u16, channels: u16, sample_rate: u32, bits: u16, block_align: u16, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(4 + 8 + 16 + 8 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&sample_rate.to_le_bytes());
        out.extend_from_slice(&(sample_rate * channels as u32 * (bits as u32 / 8)).to_le_bytes());
        out.extend_from_slice(&block_align.to_le_bytes());
        out.extend_from_slice(&bits.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn decodes_8_24_and_float_formats() {
        // 8-bit unsigned: 128 is silence, 255 is +1.
        let wav8 = wav_custom(1, 1, 8000, 8, 1, &[128, 255]);
        let sound = decode_wav(&wav8).expect("8-bit");
        assert!(sound.samples[0].abs() < 1e-6);
        assert!(sound.samples[1] > 0.98);

        // 24-bit signed: 0x400000 is 0.5.
        let wav24 = wav_custom(1, 1, 8000, 24, 3, &[0x00, 0x00, 0x40]);
        let sound = decode_wav(&wav24).expect("24-bit");
        assert!((sound.samples[0] - 0.5).abs() < 1e-6, "{}", sound.samples[0]);

        // 32-bit signed: 0x40000000 is 0.5.
        let wav32 = wav_custom(1, 1, 8000, 32, 4, &0x4000_0000i32.to_le_bytes());
        let sound = decode_wav(&wav32).expect("32-bit");
        assert!((sound.samples[0] - 0.5).abs() < 1e-6);

        // IEEE float32: 0.25.
        let wavf = wav_custom(3, 1, 8000, 32, 4, &0.25f32.to_le_bytes());
        let sound = decode_wav(&wavf).expect("float32");
        assert!((sound.samples[0] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn decodes_ima_adpcm_block_exactly() {
        // One mono block: predictor 0, step index 0 => step 7.
        // Nibbles: 0 (diff +0), 7 (diff +7/8*step), 8 (negative), 15.
        let block = [0x00, 0x00, 0x00, 0x00, 0x70, 0x8F];
        let bytes = wav_ima(8000, 1, 6, 5, &block);
        let info = probe_wav(&bytes).expect("probe");
        assert_eq!(info.codec, WavCodec::ImaAdpcm);
        assert_eq!(info.samples_per_block, 5);
        let sound = decode_wav(&bytes).expect("decode");
        assert_eq!(sound.samples.len(), 5);
        // Sample 0 is the predictor (0). Then compute the documented sequence.
        let mut predictor = 0i32;
        let mut index = 0usize;
        // Byte 0x70 => low nibble 0 then high 7; byte 0x8F => low F then high 8.
        let expected: Vec<i32> = [0x0, 0x7, 0xF, 0x8]
            .iter()
            .map(|nibble| {
                let step = IMA_STEP[index];
                let mut diff = step >> 3;
                if nibble & 4 != 0 {
                    diff += step;
                }
                if nibble & 2 != 0 {
                    diff += step >> 1;
                }
                if nibble & 1 != 0 {
                    diff += step >> 2;
                }
                if nibble & 8 != 0 {
                    predictor -= diff;
                } else {
                    predictor += diff;
                }
                predictor = predictor.clamp(-32768, 32767);
                index = (index as i8 + IMA_INDEX[*nibble as usize]).clamp(0, 88) as usize;
                predictor
            })
            .collect();
        let decoded: Vec<i32> = sound
            .samples
            .iter()
            .map(|s| (s * 32768.0).round() as i32)
            .collect();
        assert_eq!(decoded[0], 0);
        assert_eq!(&decoded[1..], &expected[..], "IMA nibble decode");
    }

    #[test]
    fn decodes_ms_adpcm_block_exactly() {
        // Mono block: predictor 0 (coeff 256/0), delta 16, sample1 1000, sample2 500.
        // One byte: high nibble 1, then low nibble 0.
        let block = [0x00, 0x10, 0x00, 0xE8, 0x03, 0xF4, 0x01, 0x10];
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(4 + 8 + 20 + 8 + block.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&20u32.to_le_bytes());
        out.extend_from_slice(&0x02u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&8000u32.to_le_bytes());
        out.extend_from_slice(&(8000u32 * 2).to_le_bytes());
        out.extend_from_slice(&8u16.to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(block.len() as u32).to_le_bytes());
        out.extend_from_slice(&block);
        let info = probe_wav(&out).expect("probe");
        assert_eq!(info.codec, WavCodec::MsAdpcm);
        assert_eq!(info.samples_per_block, 4);
        let sound = decode_wav(&out).expect("decode");
        assert_eq!(sound.samples.len(), 4, "2 preamble + 2 nibbles");
        // Preamble: sample2 then sample1.
        assert!((sound.samples[0] - 500.0 / 32768.0).abs() < 1e-6);
        assert!((sound.samples[1] - 1000.0 / 32768.0).abs() < 1e-6);
        // nibble 1: predicted = (1000*256 + 500*0)/256 + 1*16 = 1016.
        let third = (sound.samples[2] * 32768.0).round() as i32;
        assert_eq!(third, 1016);
    }

    #[test]
    fn rejects_foreign_and_truncated_files() {
        assert!(matches!(decode_wav(b"not a wav"), Err(AudioError::BadMagic)));
        assert!(matches!(decode_wav(b"RIFF"), Err(AudioError::TooShort)));
    }
}
