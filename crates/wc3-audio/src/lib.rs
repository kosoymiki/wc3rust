//! Warcraft III audio: decoding, mixing, an `AudioEngine`, and gameplay hooks.
//!
//! * [`wav`] — RIFF/WAVE PCM (8/16/24/32-bit), float, IMA ADPCM, MS ADPCM.
//! * [`mp3`] — MPEG Layer III via `nanomp3` (pure Rust), with a frame-header
//!   probe and a streaming source that decodes one frame at a time.
//! * [`soundinfo`] — the retail `UI/SoundInfo/*.slk` tables and the unit
//!   sound-set mapping used by the gameplay hooks.
//! * [`engine`] — block-based mixer with voice handles, 3D pan/attenuation,
//!   looping, category buses, and a render thread so the UI never blocks.
//! * [`game_audio`] — `GameAudio`: unit ack/attack/death, UI click/error and
//!   music themes, all resolved from real archive paths.

pub mod engine;
pub mod events;
pub mod game_audio;
pub mod map_sounds;
pub mod mp3;
pub mod playlist;
pub mod soundinfo;
pub mod wav;
pub mod ambient;

pub use engine::{
    AudioBackend, AudioConfig, AudioEngine, AudioStats, Category, HeightSampler, NullBackend,
    PlayParams, VoiceId,
};
pub use ambient::{
    render_ambient, AmbientRender, AmbientTable, AmbientTheme, DlsBank, DlsRegion, DlsSample,
    MidiNote, MidiSong, AMBIENT_SAMPLE_RATE,
};
pub use events::{event_defs, AudioEvent, AudioGroup, Material, MusicState, Race};
pub use map_sounds::{parse_w3r, parse_w3s, MapRegion, MapSound, MapSounds};
pub use game_audio::{GameAudio, GameAudioConfig, SoundProvider, UiFeedback};
pub use mp3::{decode_mp3, decode_mp3_bounded, probe_mp3, Mp3Info, Mp3Stream};
pub use playlist::{fnv1a, hash_order, MusicPlaylist};
pub use soundinfo::{
    SoundDef, SoundInfoDb, UnitNameMap, UnitSoundProfile, UnitWeaponSounds, DEFAULT_UNIT_ALIASES,
};
pub use wav::{decode_wav, probe_wav, WavCodec, WavInfo};

/// Decoded audio, interleaved f32 in `[-1, 1]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Sound {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

impl Sound {
    pub fn frames(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / self.channels as usize
        }
    }

    pub fn duration_seconds(&self) -> f32 {
        if self.sample_rate == 0 || self.channels == 0 {
            0.0
        } else {
            self.samples.len() as f32 / (self.sample_rate as f32 * self.channels as f32)
        }
    }

    pub fn duration_ms(&self) -> u64 {
        (self.duration_seconds() * 1000.0) as u64
    }

    /// Bytes of sample memory (for cache accounting).
    pub fn memory_bytes(&self) -> usize {
        self.samples.capacity() * std::mem::size_of::<f32>()
    }

    /// Peak absolute sample value.
    pub fn peak(&self) -> f32 {
        self.samples.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()))
    }

    pub fn is_silent(&self) -> bool {
        self.peak() < 1e-6
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioError {
    TooShort,
    BadMagic,
    Unsupported(&'static str),
    UnsupportedTag(u16),
    Corrupt(&'static str),
    Backend(String),
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => f.write_str("audio data too short"),
            Self::BadMagic => f.write_str("not a recognised audio container"),
            Self::Unsupported(why) => write!(f, "unsupported audio: {why}"),
            Self::UnsupportedTag(tag) => write!(f, "unsupported WAVE format tag 0x{tag:04x}"),
            Self::Corrupt(why) => write!(f, "corrupt audio: {why}"),
            Self::Backend(why) => write!(f, "audio backend: {why}"),
        }
    }
}

impl std::error::Error for AudioError {}

/// Build a PCM16 WAV in memory (used by tests and generated UI sounds).
pub fn encode_wav_pcm16(sample_rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let data_len = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_generated_wav() {
        let samples = vec![0i16, 1000, -1000, 32767, -32768, 0];
        let wav = encode_wav_pcm16(44100, 1, &samples);
        let sound = decode_wav(&wav).expect("decode");
        assert_eq!(sound.sample_rate, 44100);
        assert_eq!(sound.channels, 1);
        assert_eq!(sound.samples.len(), samples.len());
        assert!((sound.samples[3] - 32767.0 / 32768.0).abs() < 1e-6);
    }

    #[test]
    fn rejects_non_wave() {
        assert!(matches!(decode_wav(b"not a wav"), Err(AudioError::BadMagic)));
    }
}
