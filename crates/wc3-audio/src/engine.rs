//! Block-based audio engine: voice handles, 3D pan, looping, category buses,
//! streaming MP3 music and a render thread.
//!
//! Design constraints that shaped this file:
//!
//! * **The UI thread never blocks or allocates.** Every public call pushes a
//!   `Command` into a bounded `sync_channel` with `try_send`; a full queue
//!   increments a dropped-command counter instead of stalling the caller.
//! * **No per-frame allocation.** Buffers are allocated once at construction
//!   (mix block, MP3 decode buffer, resample scratch). `render_block` only
//!   writes into them.
//! * **Deterministic tests.** `AudioEngine::headless` runs the same renderer
//!   without a thread, so a test can pull exact blocks and assert sample
//!   values, pan energy and voice lifecycle.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::Vec3;

use crate::mp3::Mp3Stream;
use crate::{AudioError, Sound};

pub const COMMAND_QUEUE_CAPACITY: usize = 1024;

/// Shutdown stage log (`H2_AUDIO_SHUTDOWN_LOG=1`, or `H2_AUDIO_LOG=1`).
///
/// One line per stage of the render-thread shutdown lets a hang be attributed
/// to the exact call (`write`, `close`, `join`) without a debugger.
pub(crate) fn shutdown_log(stage: &str) {
    if std::env::var_os("H2_AUDIO_SHUTDOWN_LOG").is_some()
        || std::env::var_os("H2_AUDIO_LOG").is_some()
    {
        eprintln!("AUDIO_SHUTDOWN {stage}");
    }
}

/// Terrain/geometry height sampler used for voice occlusion. The audio engine
/// calls it with a world position on the listener→source segment; returning the
/// solid surface height at that XZ is enough to detect a ridge or cliff between
/// the two. Installed by `Assets::install_audio_occluder` from the map's W3E
/// grid, so an occluded unit behind a cliff is muffled without a physics
/// dependency inside the mixer.
pub type HeightSampler = Arc<dyn Fn(Vec3) -> f32 + Send + Sync>;

/// kira `SpatialTrackBuilder::spatialization_strength` default: 0 disables the
/// stereo image, 1 makes the interaural level difference absolute.
pub const SPATIALIZATION_STRENGTH: f32 = 0.75;

/// kira listener ear spacing (`listener_ear_positions`, track/sub.rs:341-348).
const EAR_DISTANCE: f32 = 0.1;
/// kira ear angle from the head axis (`EAR_ANGLE_FROM_HEAD = FRAC_PI_8`).
const EAR_ANGLE: f32 = std::f32::consts::FRAC_PI_8;

/// Default time span of audio kept queued in the device path.
///
/// Android schedules the render thread against the game/render threads, so a
/// one-block queue (the old behaviour: ~10 ms at 48 kHz) starves on any
/// scheduling jitter and the output crackles. 150 ms absorbs jitter while
/// staying below the threshold where UI feedback feels delayed; override per
/// device with `H2_AUDIO_BUFFER_MS`.
pub const DEFAULT_BUFFER_MS: u32 = 150;

/// Upper bound on how long `Drop`/`shutdown` waits for the render thread.
pub const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_millis(200);

/// Slice of the shutdown budget spent waiting for the backend handoff before
/// attempting the device close.
pub const SHUTDOWN_HANDOFF_TIMEOUT: Duration = Duration::from_millis(100);

/// Upper bound on how long the device close may block the caller. SDL's
/// `SDL_DestroyAudioStream` waits for the device/Pulse mainloop and can block
/// indefinitely on a suspended sink (measured in Termux); past this budget the
/// close thread is detached so the process can exit.
pub const SHUTDOWN_CLOSE_TIMEOUT: Duration = Duration::from_millis(100);

/// Handle to a playing voice. Ids are never reused within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VoiceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Sfx,
    Ui,
    Music,
    Ambient,
    Voice,
}

impl Category {
    pub const COUNT: usize = 5;

    pub fn index(self) -> usize {
        match self {
            Self::Sfx => 0,
            Self::Ui => 1,
            Self::Music => 2,
            Self::Ambient => 3,
            Self::Voice => 4,
        }
    }
}

/// Per-voice playback parameters.
#[derive(Debug, Clone, Copy)]
pub struct PlayParams {
    pub gain: f32,
    /// -1 = hard left, 0 = centre, +1 = hard right (2D sounds).
    pub pan: f32,
    /// 3D source position; when set it overrides `pan` and drives attenuation.
    pub position: Option<Vec3>,
    pub looping: bool,
    pub pitch: f32,
    pub category: Category,
    /// Distance at which a 3D sound is still at unity gain (kira
    /// `SpatialTrackDistances::min_distance`).
    pub reference_distance: f32,
    /// Distance at which a 3D sound is inaudible (kira `max_distance`). 0 keeps
    /// the legacy inverse-square falloff with no cutoff.
    pub max_distance: f32,
    /// Manual occlusion 0..1 added to the engine's terrain raycast occlusion.
    /// 0 = rely on the installed terrain sampler only.
    pub occlusion: f32,
    /// Higher priority wins when the voice pool is full.
    pub priority: u32,
    /// `SoundInfo` channel number (row column `Channel`, `sound_entry+0xdc`,
    /// handed to the mixer as `param[0x13]` in `0x6f10c880`). 0 = ungrouped:
    /// the voice is not subject to a per-channel limit.
    pub channel: u16,
    /// `CHANNELFULLPREEMPT` flag: when the channel is full, stop the
    /// lowest-priority voice of the same channel instead of dropping this one.
    pub preempt: bool,
    /// Stable key for `NODUPLICATES`: a voice with the same key already playing
    /// on the same channel suppresses this one. 0 = no dedup key.
    pub dedup: u64,
    /// Fade-in length in milliseconds (0 = the default 5 ms anti-click ramp).
    ///
    /// Used for music/ambience cross-fades: the new track starts silent and
    /// ramps to full gain over this window while the outgoing track is ramped
    /// down by [`AudioEngine::fade_out`], so a theme change never clicks.
    pub fade_in_ms: u32,
}

impl Default for PlayParams {
    fn default() -> Self {
        Self {
            gain: 1.0,
            pan: 0.0,
            position: None,
            looping: false,
            pitch: 1.0,
            category: Category::Sfx,
            reference_distance: 128.0,
            max_distance: 0.0,
            occlusion: 0.0,
            priority: 0,
            channel: 0,
            preempt: false,
            dedup: 0,
            fade_in_ms: 0,
        }
    }
}

impl PlayParams {
    pub fn at(position: Vec3) -> Self {
        Self {
            position: Some(position),
            ..Default::default()
        }
    }

    pub fn music() -> Self {
        Self {
            category: Category::Music,
            looping: true,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub block_frames: usize,
    pub max_voices: usize,
    /// Voices allowed per `SoundInfo` channel number (0 = unlimited).
    ///
    /// The engine hands the row's `Channel` to the Miles mixer
    /// (`param[0x13]`, `0x6f10c880`) and marks preemptable rows with
    /// `CHANNELFULLPREEMPT`; the numeric per-channel cap itself lives in
    /// Mss32 and is not present in Game.dll (see
    /// `analysis/agent-re/ui_audio/constants.json:voice_limits`), so it is a
    /// tuning constant here. The default of one voice per channel number
    /// reproduces retail's one-ack-line-per-channel behaviour with the shipped
    /// `UnitAckSounds` channels 1..4.
    pub channel_voices: u16,
    pub master_gain: f32,
}

/// Default per-channel voice cap (`AudioConfig::channel_voices`).
pub const DEFAULT_CHANNEL_VOICES: u16 = 1;

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            channels: 2,
            block_frames: 1024,
            max_voices: 64,
            channel_voices: DEFAULT_CHANNEL_VOICES,
            master_gain: 0.8,
        }
    }
}

impl AudioConfig {
    pub fn block_samples(&self) -> usize {
        self.block_frames * self.channels as usize
    }

    /// Queued-audio target in milliseconds (device buffer + stream queue).
    ///
    /// `H2_AUDIO_BUFFER_MS` overrides the default for device tuning; the value
    /// is clamped so a typo cannot create seconds of latency or a 1 ms queue.
    pub fn effective_buffer_ms(&self) -> u32 {
        std::env::var("H2_AUDIO_BUFFER_MS")
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok())
            .map(clamp_buffer_ms)
            .unwrap_or(DEFAULT_BUFFER_MS)
    }
}

/// Clamp a requested queue target to a usable device range (20–1000 ms).
pub fn clamp_buffer_ms(ms: u32) -> u32 {
    ms.clamp(20, 1000)
}

/// Output device. Implementations are owned by the render thread; `write`
/// blocks until the device accepted the block.
pub trait AudioBackend: Send + 'static {
    fn write(&mut self, samples: &[f32]) -> Result<usize, AudioError>;
    fn latency_ms(&self) -> u32 {
        0
    }
    /// Audio currently queued in the device path, when the backend can report
    /// it. `None` means unknown and the engine falls back to wall-clock pacing.
    fn buffered_ms(&self) -> Option<u32> {
        None
    }
    fn name(&self) -> &'static str;
    fn close(&mut self) {}
}

/// Backend that accepts everything and records what it received. Used by tests
/// and by `--no-audio` runs; it also proves the render path produces the
/// expected sample counts.
#[derive(Debug, Default)]
pub struct NullBackend {
    pub written_samples: u64,
    pub blocks: u64,
    pub max_block: usize,
    pub last_peak: f32,
}

impl AudioBackend for NullBackend {
    fn write(&mut self, samples: &[f32]) -> Result<usize, AudioError> {
        self.written_samples += samples.len() as u64;
        self.blocks += 1;
        self.max_block = self.max_block.max(samples.len());
        self.last_peak = samples
            .iter()
            .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        Ok(samples.len())
    }

    fn name(&self) -> &'static str {
        "null"
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AudioStats {
    pub frames_rendered: u64,
    pub blocks_rendered: u64,
    pub voices_started: u64,
    pub voices_finished: u64,
    pub active_voices: u32,
    pub max_active_voices: u32,
    pub voices_stealed: u64,
    /// Voices refused because their `SoundInfo` channel was full and the row
    /// did not carry `CHANNELFULLPREEMPT` (or had no higher priority).
    pub voices_channel_full: u64,
    /// Voices stopped to admit a higher-priority voice on a full channel
    /// (`CHANNELFULLPREEMPT`).
    pub voices_channel_preempted: u64,
    /// Voices suppressed by `NODUPLICATES` (same key already on the channel).
    pub voices_deduped: u64,
    pub commands_dropped: u64,
    pub backend_errors: u64,
    pub silence_blocks: u64,
    /// Times the device queue was observed empty after the prefill (dropouts).
    pub buffer_underruns: u64,
    /// 3D voices the terrain raycast marked as occluded this session.
    pub occluded_voices: u64,
    /// Blocks in which at least one category was ducked.
    pub ducked_blocks: u64,
}

impl AudioStats {
    pub fn seconds_rendered(&self, sample_rate: u32) -> f64 {
        if sample_rate == 0 {
            0.0
        } else {
            self.frames_rendered as f64 / sample_rate as f64
        }
    }
}

enum Command {
    Play {
        id: VoiceId,
        source: Box<Source>,
        params: PlayParams,
    },
    Stop(VoiceId),
    /// Fade a voice out over N milliseconds, then free it.
    FadeOut(VoiceId, u32),
    StopAll,
    StopCategory(Category),
    SetGain(VoiceId, f32),
    SetPan(VoiceId, f32),
    SetPosition(VoiceId, Option<Vec3>),
    SetLooping(VoiceId, bool),
    SetPitch(VoiceId, f32),
    SetMaster(f32),
    SetCategoryGain(Category, f32),
    SetListener(Vec3, f32),
    SetOccluder(Option<HeightSampler>),
    Duck {
        category: Category,
        amount_db: f32,
        hold_seconds: f32,
    },
}

enum Source {
    Pcm(Arc<Sound>),
    Mp3(Box<Mp3Stream>),
}

impl Source {
    fn sample_rate(&self) -> u32 {
        match self {
            Self::Pcm(sound) => sound.sample_rate,
            Self::Mp3(stream) => stream.sample_rate(),
        }
    }

    /// Decoded sample bytes this source holds (0 for streamed MP3).
    fn pcm_bytes(&self) -> usize {
        match self {
            Self::Pcm(sound) => sound.memory_bytes(),
            Self::Mp3(_) => 0,
        }
    }

    fn channels(&self) -> u16 {
        match self {
            Self::Pcm(sound) => sound.channels.max(1),
            Self::Mp3(stream) => stream.channels(),
        }
    }
}

/// Streaming decode buffer for one MP3 voice (allocated once).
///
/// `start_frame` is the absolute source-frame number of `samples[0]`, so the
/// play head (`slot.phase`) and the buffer index stay in the same units even
/// after compaction. Compaction drops only frames already consumed, so the
/// window is bounded by `samples.len()` no matter how long the track is.
struct Mp3Buffer {
    samples: Vec<f32>,
    /// Decoded frames currently held (from `start_frame`).
    frames: usize,
    start_frame: u64,
}

impl Mp3Buffer {
    fn new(channels: u16) -> Self {
        let frames = 8192;
        Self {
            samples: vec![0.0; frames * channels as usize],
            frames: 0,
            start_frame: 0,
        }
    }

    fn reset(&mut self) {
        self.frames = 0;
        self.start_frame = 0;
    }

    /// Ensure frames `abs` and `abs + 1` are decoded. Returns false at end of
    /// stream (caller decides whether to loop).
    fn ensure(&mut self, stream: &mut Mp3Stream, channels: usize, abs: u64) -> bool {
        if abs < self.start_frame {
            return false;
        }
        loop {
            let rel = (abs - self.start_frame) as usize;
            if rel + 2 <= self.frames {
                return true;
            }
            if rel > 0 {
                let keep = self.frames - rel;
                let source = rel * channels;
                let end = self.frames * channels;
                self.samples.copy_within(source..end, 0);
                self.frames = keep;
                self.start_frame += rel as u64;
                continue;
            }
            // rel == 0: buffer starts at the requested frame but has < 2 frames.
            if self.frames * channels >= self.samples.len() {
                // Full window and still too short: caller is asking for a frame
                // far beyond what one window can hold (should not happen).
                return false;
            }
            let offset = self.frames * channels;
            let n = stream.read(&mut self.samples[offset..]);
            if n == 0 {
                return false;
            }
            self.frames += n / channels;
        }
    }

    fn frame(&self, abs: u64, channels: usize) -> (f32, f32) {
        let rel = (abs - self.start_frame) as usize;
        let base = rel * channels;
        match channels {
            1 => {
                let mono = self.samples.get(base).copied().unwrap_or(0.0);
                (mono, mono)
            }
            _ => (
                self.samples.get(base).copied().unwrap_or(0.0),
                self.samples.get(base + 1).copied().unwrap_or(0.0),
            ),
        }
    }
}

struct VoiceSlot {
    id: VoiceId,
    source: Source,
    mp3_buffer: Option<Mp3Buffer>,
    params: PlayParams,
    /// Fractional source-frame position.
    phase: f64,
    /// Total source frames played (for stats/seeking).
    frames_played: u64,
    env: f32,
    env_target: f32,
    env_step: f32,
    finished: bool,
    /// One-pole low-pass state for occlusion (continuous across blocks).
    lp_l: f32,
    lp_r: f32,
    /// The occlusion counter is incremented once per voice, not per block.
    occluded_logged: bool,
}

impl VoiceSlot {
    fn new(
        id: VoiceId,
        source: Source,
        params: PlayParams,
        out_rate: u32,
        max_frames: usize,
    ) -> Self {
        let out_rate = out_rate.max(1) as f32;
        // A cross-fade stretches the attack ramp for this voice only; every
        // other sound keeps the 5 ms anti-click ramp.
        let env_step = if params.fade_in_ms > 0 {
            1.0 / (params.fade_in_ms as f32 / 1000.0 * out_rate)
        } else {
            1.0 / (0.005 * out_rate)
        };
        let mut slot = Self {
            id,
            source,
            mp3_buffer: None,
            params,
            phase: 0.0,
            frames_played: 0,
            env: 0.0,
            env_target: 1.0,
            env_step,
            finished: false,
            lp_l: 0.0,
            lp_r: 0.0,
            occluded_logged: false,
        };
        if let Source::Mp3(_) = &slot.source {
            slot.mp3_buffer = Some(Mp3Buffer::new(slot.source.channels()));
        }
        // The buffer is sized by `max_frames` at construction to avoid growth.
        let _ = max_frames;
        slot
    }

    fn source_frames(&self) -> Option<u64> {
        match &self.source {
            Source::Pcm(sound) => Some(sound.frames() as u64),
            Source::Mp3(stream) => stream.total_frames(),
        }
    }

    /// Interpolated frame at `index` (absolute source frame).
    fn sample_pcm(sound: &Sound, index: u64) -> (f32, f32) {
        let frames = sound.frames();
        if frames == 0 {
            return (0.0, 0.0);
        }
        let channels = sound.channels.max(1) as usize;
        let i0 = (index as usize).min(frames - 1);
        let i1 = (i0 + 1).min(frames - 1);
        let base0 = i0 * channels;
        let base1 = i1 * channels;
        let (a_l, a_r) = match channels {
            1 => (
                sound.samples.get(base0).copied().unwrap_or(0.0),
                sound.samples.get(base0).copied().unwrap_or(0.0),
            ),
            _ => (
                sound.samples.get(base0).copied().unwrap_or(0.0),
                sound.samples.get(base0 + 1).copied().unwrap_or(0.0),
            ),
        };
        let (b_l, b_r) = match channels {
            1 => (
                sound.samples.get(base1).copied().unwrap_or(0.0),
                sound.samples.get(base1).copied().unwrap_or(0.0),
            ),
            _ => (
                sound.samples.get(base1).copied().unwrap_or(0.0),
                sound.samples.get(base1 + 1).copied().unwrap_or(0.0),
            ),
        };
        let frac = (index as f64 - i0 as f64) as f32;
        (
            a_l + (b_l - a_l) * frac,
            a_r + (b_r - a_r) * frac,
        )
    }
}

struct Core {
    config: AudioConfig,
    slots: Vec<Option<VoiceSlot>>,
    free: Vec<usize>,
    category_gain: [f32; Category::COUNT],
    /// Smoothed duck attenuation per category (linear, 1.0 = not ducked).
    duck_gain: [f32; Category::COUNT],
    /// Duck target and remaining hold in frames; 1.0 / 0 when idle.
    duck_amount: [f32; Category::COUNT],
    duck_hold: [u64; Category::COUNT],
    master_gain: f32,
    listener: Vec3,
    listener_yaw: f32,
    occluder: Option<HeightSampler>,
    stats: Arc<Counters>,
}

#[derive(Default)]
struct Counters {
    frames: AtomicU64,
    blocks: AtomicU64,
    started: AtomicU64,
    finished: AtomicU64,
    active: AtomicU64,
    max_active: AtomicU64,
    stealed: AtomicU64,
    channel_full: AtomicU64,
    channel_preempted: AtomicU64,
    deduped: AtomicU64,
    dropped: AtomicU64,
    backend_errors: AtomicU64,
    silence_blocks: AtomicU64,
    buffer_underruns: AtomicU64,
    occluded: AtomicU64,
    ducked_blocks: AtomicU64,
}

impl Counters {
    fn snapshot(&self) -> AudioStats {
        AudioStats {
            frames_rendered: self.frames.load(Ordering::Relaxed),
            blocks_rendered: self.blocks.load(Ordering::Relaxed),
            voices_started: self.started.load(Ordering::Relaxed),
            voices_finished: self.finished.load(Ordering::Relaxed),
            active_voices: self.active.load(Ordering::Relaxed) as u32,
            max_active_voices: self.max_active.load(Ordering::Relaxed) as u32,
            voices_stealed: self.stealed.load(Ordering::Relaxed),
            voices_channel_full: self.channel_full.load(Ordering::Relaxed),
            voices_channel_preempted: self.channel_preempted.load(Ordering::Relaxed),
            voices_deduped: self.deduped.load(Ordering::Relaxed),
            commands_dropped: self.dropped.load(Ordering::Relaxed),
            backend_errors: self.backend_errors.load(Ordering::Relaxed),
            silence_blocks: self.silence_blocks.load(Ordering::Relaxed),
            buffer_underruns: self.buffer_underruns.load(Ordering::Relaxed),
            occluded_voices: self.occluded.load(Ordering::Relaxed),
            ducked_blocks: self.ducked_blocks.load(Ordering::Relaxed),
        }
    }
}

impl Core {
    fn new(config: AudioConfig, stats: Arc<Counters>) -> Self {
        let mut slots = Vec::with_capacity(config.max_voices);
        for _ in 0..config.max_voices {
            slots.push(None);
        }
        let free: Vec<usize> = (0..config.max_voices).rev().collect();
        Self {
            config,
            slots,
            free,
            category_gain: [1.0; Category::COUNT],
            duck_gain: [1.0; Category::COUNT],
            duck_amount: [1.0; Category::COUNT],
            duck_hold: [0; Category::COUNT],
            master_gain: config.master_gain,
            listener: Vec3::ZERO,
            listener_yaw: 0.0,
            occluder: None,
            stats,
        }
    }

    fn active(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    /// Per-channel admission (`SoundInfo` `Channel` + `CHANNELFULLPREEMPT` +
    /// `NODUPLICATES`).
    ///
    /// The engine gives every voice a channel number and a priority
    /// (`play_instance+0x38` / `+0x70`, mixer `param[0x12]`/`param[0x13]`) and
    /// lets a full channel drop or preempt; `CHANNELFULLPREEMPT` marks the rows
    /// that may stop another voice of the same channel. Channel 0 is
    /// ungrouped and always admitted.
    fn admit_channel(&mut self, params: PlayParams) -> bool {
        let cap = self.config.channel_voices as usize;
        if params.channel == 0 || cap == 0 {
            return true;
        }
        let same_channel: Vec<usize> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                slot.as_ref()
                    .filter(|slot| slot.params.channel == params.channel)
                    .map(|_| index)
            })
            .collect();
        if params.dedup != 0
            && same_channel
                .iter()
                .any(|index| self.slots[*index].as_ref().is_some_and(|s| s.params.dedup == params.dedup))
        {
            self.stats.deduped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        if same_channel.len() < cap {
            return true;
        }
        if !params.preempt {
            self.stats.channel_full.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        // Lowest priority is evicted first; on a tie the OLDEST voice goes,
        // which is the retail channel-full behaviour for ack rows (all 739
        // rows carry priority 1000 + `CHANNELFULLPREEMPT`, so a new line
        // replaces the one currently speaking on that channel).
        let victim = same_channel.into_iter().min_by_key(|index| {
            let slot = self.slots[*index].as_ref().expect("channel member");
            (slot.params.priority, slot.id.0)
        });
        let Some(index) = victim else {
            return true;
        };
        let victim_priority = self.slots[index]
            .as_ref()
            .map(|slot| slot.params.priority)
            .unwrap_or(0);
        if params.priority < victim_priority {
            self.stats.channel_full.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        self.slots[index] = None;
        self.free.push(index);
        self.stats.finished.fetch_add(1, Ordering::Relaxed);
        self.stats.channel_preempted.fetch_add(1, Ordering::Relaxed);
        true
    }

    fn alloc_slot(&mut self, id: VoiceId, source: Source, params: PlayParams) -> Option<usize> {
        if !self.admit_channel(params) {
            return None;
        }
        if let Some(index) = self.free.pop() {
            self.slots[index] = Some(VoiceSlot::new(
                id,
                source,
                params,
                self.config.sample_rate,
                self.config.block_frames,
            ));
            self.stats.started.fetch_add(1, Ordering::Relaxed);
            let active = self.active() as u64;
            self.stats.active.store(active, Ordering::Relaxed);
            self.stats.max_active.fetch_max(active, Ordering::Relaxed);
            return Some(index);
        }
        // Pool full: steal the lowest-priority, quietest voice if the new one
        // has strictly higher priority.
        let mut victim: Option<(usize, u32, f32)> = None;
        for (index, slot) in self.slots.iter().enumerate() {
            let Some(slot) = slot else { continue };
            let key = (slot.params.priority, slot.env);
            match victim {
                Some((_, priority, env)) if (key.0, key.1) >= (priority, env) => {}
                _ => victim = Some((index, key.0, key.1)),
            }
        }
        let (index, priority, _) = victim?;
        if params.priority <= priority {
            return None;
        }
        self.slots[index] = Some(VoiceSlot::new(
            id,
            source,
            params,
            self.config.sample_rate,
            self.config.block_frames,
        ));
        self.stats.stealed.fetch_add(1, Ordering::Relaxed);
        self.stats.started.fetch_add(1, Ordering::Relaxed);
        Some(index)
    }

    fn slot_mut(&mut self, id: VoiceId) -> Option<&mut VoiceSlot> {
        self.slots
            .iter_mut()
            .flatten()
            .find(|slot| slot.id == id)
    }

    fn free_slot(&mut self, index: usize) {
        if self.slots[index].take().is_some() {
            self.free.push(index);
            self.stats.finished.fetch_add(1, Ordering::Relaxed);
            let active = self.active() as u64;
            self.stats.active.store(active, Ordering::Relaxed);
        }
    }

    /// Per-frame envelope step for an explicit stop (5 ms anti-click ramp).
    fn fast_env_step(&self) -> f32 {
        1.0 / (0.005 * self.config.sample_rate.max(1) as f32)
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::Play { id, source, params } => {
                self.alloc_slot(id, *source, params);
            }
            Command::Stop(id) => {
                let fast = self.fast_env_step();
                if let Some(slot) = self.slot_mut(id) {
                    slot.params.looping = false;
                    slot.env_target = 0.0;
                    // "Stop now": a cross-fade voice carries a long fade-in
                    // step, which must not stretch an explicit stop into a
                    // 1.2 s release. Keep only the 5 ms anti-click ramp.
                    slot.env_step = fast;
                    slot.finished = true;
                }
            }
            Command::FadeOut(id, ms) => {
                let rate = self.config.sample_rate.max(1) as f32;
                if let Some(slot) = self.slot_mut(id) {
                    // Slow release: the voice keeps playing while its envelope
                    // ramps to silence, which is the outgoing half of a
                    // cross-fade (the incoming voice has `fade_in_ms` set).
                    slot.params.looping = false;
                    slot.env_target = 0.0;
                    slot.env_step = 1.0 / ((ms.max(1) as f32 / 1000.0) * rate);
                    slot.finished = true;
                }
            }
            Command::StopAll => {
                let fast = self.fast_env_step();
                for slot in self.slots.iter_mut().flatten() {
                    slot.params.looping = false;
                    slot.env_target = 0.0;
                    slot.env_step = fast;
                    slot.finished = true;
                }
            }
            Command::StopCategory(category) => {
                let fast = self.fast_env_step();
                for slot in self.slots.iter_mut().flatten() {
                    if slot.params.category == category {
                        slot.params.looping = false;
                        slot.env_target = 0.0;
                        slot.env_step = fast;
                        slot.finished = true;
                    }
                }
            }
            Command::SetGain(id, gain) => {
                if let Some(slot) = self.slot_mut(id) {
                    slot.params.gain = gain.clamp(0.0, 8.0);
                }
            }
            Command::SetPan(id, pan) => {
                if let Some(slot) = self.slot_mut(id) {
                    slot.params.pan = pan.clamp(-1.0, 1.0);
                }
            }
            Command::SetPosition(id, position) => {
                if let Some(slot) = self.slot_mut(id) {
                    slot.params.position = position;
                }
            }
            Command::SetLooping(id, looping) => {
                if let Some(slot) = self.slot_mut(id) {
                    slot.params.looping = looping;
                }
            }
            Command::SetPitch(id, pitch) => {
                if let Some(slot) = self.slot_mut(id) {
                    slot.params.pitch = pitch.clamp(0.25, 4.0);
                }
            }
            Command::SetMaster(gain) => self.master_gain = gain.clamp(0.0, 4.0),
            Command::SetCategoryGain(category, gain) => {
                self.category_gain[category.index()] = gain.clamp(0.0, 4.0);
            }
            Command::SetListener(position, yaw) => {
                self.listener = position;
                self.listener_yaw = yaw;
            }
            Command::SetOccluder(sampler) => {
                self.occluder = sampler;
            }
            Command::Duck {
                category,
                amount_db,
                hold_seconds,
            } => {
                let amount = 10f32.powf(amount_db.clamp(-60.0, 0.0) / 20.0);
                self.duck_amount[category.index()] = amount;
                self.duck_hold[category.index()] =
                    (hold_seconds.max(0.0) * self.config.sample_rate.max(1) as f32) as u64;
            }
        }
    }

    /// Terrain occlusion along the listener→source segment.
    ///
    /// Samples the installed height field at six points; a sample counts as
    /// blocked when the solid surface is at least 8 units above the straight
    /// sound path (that skips the listener's and source's own ground). The
    /// result is the blocked fraction, which feeds both the gain and a one-pole
    /// low-pass in [`Core::mix_voice`] — the mixer equivalent of kira's track
    /// effects chain, driven by real map geometry.
    fn occlusion_at(&self, source: Vec3) -> f32 {
        let Some(sampler) = self.occluder.as_ref() else {
            return 0.0;
        };
        const SAMPLES: usize = 7;
        let mut blocked = 0usize;
        for step in 1..SAMPLES {
            let t = step as f32 / SAMPLES as f32;
            let point = self.listener.lerp(source, t);
            if sampler(point) > point.y + 8.0 {
                blocked += 1;
            }
        }
        blocked as f32 / (SAMPLES - 1) as f32
    }

    /// Advance the per-category duck envelopes for one block. Ported shape of
    /// kira's track volume tween: a fast attack to the duck target, a held
    /// window, then a slow release back to unity.
    fn advance_ducking(&mut self, frames: usize) {
        let seconds = frames as f32 / self.config.sample_rate.max(1) as f32;
        let attack = 1.0 - (-seconds / 0.008f32).exp();
        let release = 1.0 - (-seconds / 0.250f32).exp();
        let mut ducked = false;
        for category in 0..Category::COUNT {
            let hold = self.duck_hold[category].saturating_sub(frames as u64);
            self.duck_hold[category] = hold;
            let target = if hold > 0 {
                self.duck_amount[category]
            } else {
                1.0
            };
            let coeff = if target < self.duck_gain[category] {
                attack
            } else {
                release
            };
            self.duck_gain[category] += (target - self.duck_gain[category]) * coeff.clamp(0.0, 1.0);
            if self.duck_gain[category] < 0.999 {
                ducked = true;
            }
        }
        if ducked {
            self.stats.ducked_blocks.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn mix_voice(&mut self, index: usize, out: &mut [f32]) {
        let channels = self.config.channels.max(1) as usize;
        let out_rate = self.config.sample_rate.max(1);
        let master = self.master_gain;
        let listener = self.listener;
        let yaw = self.listener_yaw;
        // Occlusion needs the immutable occluder before the slot is borrowed
        // mutably. Manual `PlayParams::occlusion` layers on top for sounds the
        // engine cannot raycast (e.g. scripted ambience).
        let position = self.slots[index]
            .as_ref()
            .and_then(|slot| slot.params.position);
        let manual = self.slots[index]
            .as_ref()
            .map(|slot| slot.params.occlusion)
            .unwrap_or(0.0);
        let geometric = position.map(|p| self.occlusion_at(p)).unwrap_or(0.0);
        let occlusion = (geometric + manual).clamp(0.0, 1.0);
        if !self.slots[index].as_ref().is_some_and(|slot| slot.occluded_logged) && occlusion > 0.0 {
            if let Some(slot) = self.slots[index].as_mut() {
                slot.occluded_logged = true;
            }
            self.stats.occluded.fetch_add(1, Ordering::Relaxed);
        }
        let category_gain = self.category_gain;
        let duck_gain = self.duck_gain;
        let Some(slot) = self.slots[index].as_mut() else {
            return;
        };

        let (gain_l, gain_r) = pan_gains(
            slot,
            listener,
            yaw,
            &category_gain,
            &duck_gain,
            master,
            occlusion,
        );
        // One-pole low-pass for occluded voices: a source behind a cliff loses
        // its high band before it loses level, which is what makes occlusion
        // read as distance through geometry rather than a plain volume drop.
        let cutoff = 14_000.0 * (1.0 - occlusion) * (1.0 - occlusion) + 700.0 * occlusion;
        let lp_coeff = 1.0 - (-std::f32::consts::TAU * cutoff / out_rate as f32).exp();
        let source_rate = slot.source.sample_rate().max(1);
        let step = source_rate as f64 / out_rate as f64 * slot.params.pitch as f64;
        let total_frames = slot.source_frames();
        let frames_out = out.len() / channels;
        let env_step = slot.env_step;

        for frame in 0..frames_out {
            // Envelope: 5 ms ramp towards the target.
            if slot.env < slot.env_target {
                slot.env = (slot.env + env_step).min(slot.env_target);
            } else if slot.env > slot.env_target {
                slot.env = (slot.env - env_step).max(slot.env_target);
            }
            if slot.finished && slot.env <= 0.0 {
                break;
            }
            let base_frame = slot.phase.floor() as u64;
            let (mut left, mut right) = match &mut slot.source {
                Source::Pcm(sound) => {
                    let a = VoiceSlot::sample_pcm(sound, base_frame);
                    let b = VoiceSlot::sample_pcm(sound, base_frame + 1);
                    let frac = (slot.phase - base_frame as f64) as f32;
                    (a.0 + (b.0 - a.0) * frac, a.1 + (b.1 - a.1) * frac)
                }
                Source::Mp3(stream) => {
                    let buffer = match slot.mp3_buffer.as_mut() {
                        Some(buffer) => buffer,
                        None => {
                            slot.finished = true;
                            break;
                        }
                    };
                    let channels_in = stream.channels().max(1) as usize;
                    if !buffer.ensure(stream, channels_in, base_frame) {
                        if slot.params.looping {
                            if stream.rewind().is_ok() {
                                buffer.reset();
                                slot.phase = 0.0;
                                let _ = buffer.ensure(stream, channels_in, 0);
                            } else {
                                slot.finished = true;
                                slot.env_target = 0.0;
                            }
                        } else {
                            slot.finished = true;
                            slot.env_target = 0.0;
                        }
                    }
                    let a = buffer.frame(base_frame.max(buffer.start_frame), channels_in);
                    let next = base_frame + 1;
                    let b = buffer.frame(next.max(buffer.start_frame), channels_in);
                    let frac = (slot.phase - base_frame as f64) as f32;
                    (a.0 + (b.0 - a.0) * frac, a.1 + (b.1 - a.1) * frac)
                }
            };

            // Occlusion low-pass (state kept per voice, so the filter is
            // continuous across block boundaries).
            if occlusion > 0.001 {
                slot.lp_l += lp_coeff * (left - slot.lp_l);
                slot.lp_r += lp_coeff * (right - slot.lp_r);
                left = slot.lp_l;
                right = slot.lp_r;
            }

            let env = slot.env;
            left *= gain_l * env;
            right *= gain_r * env;
            if channels == 1 {
                out[frame] += (left + right) * 0.5;
            } else {
                out[frame * channels] += left;
                out[frame * channels + 1] += right;
                for channel in 2..channels {
                    out[frame * channels + channel] += (left + right) * 0.5;
                }
            }

            slot.phase += step;
            slot.frames_played += 1;
            if let Some(total) = total_frames {
                if slot.phase >= total as f64 {
                    if slot.params.looping && total > 0 {
                        slot.phase -= total as f64;
                    } else {
                        // End of source: fade out over the 5 ms envelope instead
                        // of hard-cutting (prevents a click on one-shots).
                        slot.finished = true;
                        slot.env_target = 0.0;
                    }
                }
            }
        }

        if slot.finished && slot.env <= 0.0 {
            self.free_slot(index);
        }
    }

    fn render(&mut self, out: &mut [f32], commands: impl FnMut(&mut dyn FnMut(Command))) {
        out.fill(0.0);
        let frames = out.len() / self.config.channels.max(1) as usize;
        self.advance_ducking(frames);
        for index in 0..self.slots.len() {
            if self.slots[index].is_some() {
                self.mix_voice(index, out);
            }
        }
        let master = self.master_gain;
        let mut peak = 0.0f32;
        for sample in out.iter_mut() {
            *sample = (*sample * master).clamp(-1.0, 1.0);
            peak = peak.max(sample.abs());
        }
        let frames = frames as u64;
        self.stats.frames.fetch_add(frames, Ordering::Relaxed);
        self.stats.blocks.fetch_add(1, Ordering::Relaxed);
        if peak < 1e-7 {
            self.stats.silence_blocks.fetch_add(1, Ordering::Relaxed);
        }
        self.stats
            .active
            .store(self.active() as u64, Ordering::Relaxed);
        let _ = commands;
    }
}

fn pan_gains(
    slot: &VoiceSlot,
    listener: Vec3,
    yaw: f32,
    category_gain: &[f32; Category::COUNT],
    duck_gain: &[f32; Category::COUNT],
    master: f32,
    occlusion: f32,
) -> (f32, f32) {
    let base = slot.params.gain
        * category_gain[slot.params.category.index()]
        * duck_gain[slot.params.category.index()]
        * master
        * (1.0 - 0.45 * occlusion);
    let (pan_l, pan_r, attenuation) = match slot.params.position {
        Some(position) => {
            let delta = position - listener;
            // WC3 is Z-up: right is (cos yaw, sin yaw, 0), forward is
            // (-sin yaw, cos yaw, 0) for a yaw-rotated listener.
            let right = Vec3::new(yaw.cos(), yaw.sin(), 0.0);
            let forward = Vec3::new(-yaw.sin(), yaw.cos(), 0.0);
            let distance = delta.length();
            let reference = slot.params.reference_distance.max(1.0);
            let cutoff = slot.params.max_distance;
            // Attenuation: unity inside the reference radius. With a real
            // max_distance, use kira's SpatialTrackDistances curve — the
            // amplitude interpolates Decibels::SILENCE -> IDENTITY, i.e.
            // 10^(-3 * relative_distance) (kira track/sub.rs:296-320). With no
            // cutoff keep the legacy inverse-square tail.
            let attenuation = if distance <= reference {
                1.0
            } else if cutoff > reference {
                if distance >= cutoff {
                    // `max_distance` is the far end of the WC3 curve: at and
                    // beyond it the source is inaudible (kira maps the far
                    // distance to `Decibels::SILENCE`). Without this zero the
                    // curve bottoms out at -3 dB * 3 = 1e-3, so a sound
                    // thousands of units past the far end stayed faintly
                    // audible.
                    0.0
                } else {
                    let t = (distance - reference) / (cutoff - reference);
                    10f32.powf(-3.0 * t)
                }
            } else {
                let ratio = reference / distance;
                ratio * ratio
            };
            // kira's ILD spatialisation (track/sub.rs:322-355): two virtual
            // ears offset by EAR_DISTANCE, pointing EAR_ANGLE outward from the
            // head axis. Each ear's gain is (dot(ear_dir, dir_to_source) + 1)/2
            // mixed with a floor of 1 - strength, so a hard-panned source
            // keeps a little of the far channel and a centred one is stable.
            let left_ear = listener - right * EAR_DISTANCE;
            let right_ear = listener + right * EAR_DISTANCE;
            let (sin, cos) = EAR_ANGLE.sin_cos();
            let left_dir = -right * cos + forward * sin;
            let right_dir = right * cos + forward * sin;
            let to_left = (position - left_ear).normalize_or_zero();
            let to_right = (position - right_ear).normalize_or_zero();
            let strength = SPATIALIZATION_STRENGTH;
            let floor = 1.0 - strength;
            let left = floor + strength * ((left_dir.dot(to_left) + 1.0) * 0.5);
            let right_gain = floor + strength * ((right_dir.dot(to_right) + 1.0) * 0.5);
            (left, right_gain, attenuation)
        }
        None => {
            // 2D constant-power pan law: equal perceived loudness across the
            // field (kept for UI/announcer sounds, which have no position).
            let pan = slot.params.pan.clamp(-1.0, 1.0);
            let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
            (angle.cos(), angle.sin(), 1.0)
        }
    };
    let gain = base * attenuation;
    (gain * pan_l, gain * pan_r)
}

/// Audio engine handle. In threaded mode the render loop lives on its own
/// thread; in headless mode the caller pulls blocks with [`AudioEngine::render`].
pub struct AudioEngine {
    config: AudioConfig,
    tx: SyncSender<Command>,
    rx: Option<Receiver<Command>>,
    core: Option<Core>,
    counters: Arc<Counters>,
    running: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    next_id: u32,
    dropped: u64,
    backend_name: &'static str,
    backend_latency_ms: u32,
    /// Last device-queue depth reported by the backend (`u64::MAX` = unknown).
    /// Written by the render thread, read by diagnostics/device logs.
    buffered_ms: Arc<AtomicU64>,
    /// Preallocated command drain queue for headless renders.
    pending: Vec<Command>,
    /// Render thread -> owner handoff of the backend. SDL closes the audio
    /// stream on the thread that opened it, so the thread must not destroy the
    /// device itself (it blocked there indefinitely; see
    /// `AudioEngine::stop_render_thread`).
    backend_rx: Option<Receiver<Box<dyn AudioBackend>>>,
}

impl AudioEngine {
    /// Deterministic engine without a thread or device.
    pub fn headless(config: AudioConfig) -> Self {
        let (tx, rx_storage) = sync_channel(COMMAND_QUEUE_CAPACITY);
        let counters = Arc::new(Counters::default());
        let core = Core::new(config, counters.clone());
        Self {
            config,
            tx,
            rx: Some(rx_storage),
            core: Some(core),
            counters,
            running: Arc::new(AtomicBool::new(true)),
            thread: None,
            next_id: 1,
            dropped: 0,
            backend_name: "headless",
            backend_latency_ms: 0,
            buffered_ms: Arc::new(AtomicU64::new(u64::MAX)),
            pending: Vec::with_capacity(COMMAND_QUEUE_CAPACITY),
            backend_rx: None,
        }
    }

    /// Start the render thread with a real backend.
    ///
    /// The thread keeps `config.effective_buffer_ms()` of audio queued in the
    /// device path and tops it up below the low watermark, so scheduler jitter
    /// on Android cannot empty the device buffer (the source of crackling).
    /// Backends that cannot report queue depth keep the wall-clock pacing.
    pub fn start(config: AudioConfig, mut backend: Box<dyn AudioBackend>) -> Self {
        let (tx, rx) = sync_channel(COMMAND_QUEUE_CAPACITY);
        let counters = Arc::new(Counters::default());
        let running = Arc::new(AtomicBool::new(true));
        let backend_name = backend.name();
        let backend_latency_ms = backend.latency_ms();
        let mut core = Core::new(config, counters.clone());
        let thread_running = running.clone();
        let thread_counters = counters.clone();
        let block_samples = config.block_samples();
        let target_ms = config.effective_buffer_ms() as u64;
        let buffered = Arc::new(AtomicU64::new(u64::MAX));
        let thread_buffered = buffered.clone();
        let (backend_tx, backend_rx) = std::sync::mpsc::channel::<Box<dyn AudioBackend>>();
        let thread = std::thread::Builder::new()
            .name("h2-audio-render".into())
            .spawn(move || {
                shutdown_log("thread_start");
                let mut block = vec![0.0f32; block_samples];
                let block_duration =
                    Duration::from_secs_f64(block_samples as f64 / config.sample_rate.max(1) as f64);
                let block_ms = (block_duration.as_secs_f64() * 1000.0).ceil() as u64;
                // Top-up threshold: start refilling before the queue can empty
                // even if one render iteration is late.
                let low_ms = target_ms
                    .saturating_sub(block_ms.saturating_mul(2))
                    .max(block_ms / 2)
                    .max(1);
                let mut primed = false;
                while thread_running.load(Ordering::Relaxed) {
                    let started = Instant::now();
                    // Commands first so acks/clicks land in the block we render.
                    let mut drained = 0usize;
                    while drained < COMMAND_QUEUE_CAPACITY {
                        match rx.try_recv() {
                            Ok(command) => {
                                core.apply(command);
                                drained += 1;
                            }
                            Err(_) => break,
                        }
                    }
                    let queued_before = backend.buffered_ms();
                    thread_buffered.store(
                        queued_before.map_or(u64::MAX, u64::from),
                        Ordering::Relaxed,
                    );
                    if let Some(queued) = queued_before {
                        if primed && queued == 0 {
                            thread_counters
                                .buffer_underruns
                                .fetch_add(1, Ordering::Relaxed);
                        }
                        if u64::from(queued) >= target_ms {
                            // Queue is at or above target: let the device drain
                            // instead of growing latency without bound.
                            std::thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                    }
                    core.render(&mut block, |_| {});
                    if let Err(_err) = backend.write(&block) {
                        thread_counters
                            .backend_errors
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    primed = true;
                    match backend.buffered_ms() {
                        Some(queued) => {
                            thread_buffered.store(u64::from(queued), Ordering::Relaxed);
                            if u64::from(queued) < low_ms {
                                // Still below the low watermark: top up now.
                            } else {
                                std::thread::sleep(Duration::from_millis((block_ms / 2).max(2)));
                            }
                        }
                        None => {
                            let elapsed = started.elapsed();
                            if elapsed < block_duration {
                                std::thread::sleep(block_duration - elapsed);
                            }
                        }
                    }
                }
                shutdown_log("thread_loop_exit");
                // Hand the backend to the owner (the thread that opened the SDL
                // stream) instead of closing it here: SDL teardown on this
                // thread blocked forever in the measured hang.
                if backend_tx.send(backend).is_err() {
                    shutdown_log("thread_backend_handoff_failed");
                } else {
                    shutdown_log("thread_backend_handed_off");
                }
            })
            .expect("spawn audio render thread");
        Self {
            config,
            tx,
            rx: None,
            core: None,
            counters,
            running,
            thread: Some(thread),
            next_id: 1,
            dropped: 0,
            backend_name,
            backend_latency_ms,
            buffered_ms: buffered,
            pending: Vec::with_capacity(COMMAND_QUEUE_CAPACITY),
            backend_rx: Some(backend_rx),
        }
    }

    pub fn config(&self) -> AudioConfig {
        self.config
    }

    pub fn backend_name(&self) -> &'static str {
        self.backend_name
    }

    pub fn backend_latency_ms(&self) -> u32 {
        self.backend_latency_ms
    }

    /// Last queue depth reported by the backend, in milliseconds (`None` when
    /// the backend cannot report it or the thread has not sampled yet).
    pub fn buffered_ms(&self) -> Option<u32> {
        let value = self.buffered_ms.load(Ordering::Relaxed);
        (value != u64::MAX).then_some(value as u32)
    }

    pub fn stats(&self) -> AudioStats {
        self.counters.snapshot()
    }

    /// Decoded PCM held by voices that are still alive.
    ///
    /// The engine shares `Arc<Sound>` with its owner, so a voice that is playing
    /// a cached clip costs nothing extra; this reports the bytes the mixer keeps
    /// resident for a `mem_probe`-style accounting line.
    pub fn pcm_bytes(&self) -> usize {
        let Some(core) = self.core.as_ref() else {
            return 0;
        };
        let mut seen: Vec<*const Sound> = Vec::new();
        let mut bytes = 0usize;
        for slot in core.slots.iter().flatten() {
            let Source::Pcm(sound) = &slot.source else {
                continue;
            };
            let pointer = Arc::as_ptr(sound);
            if seen.contains(&pointer) {
                continue;
            }
            seen.push(pointer);
            bytes += sound.memory_bytes();
        }
        bytes
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    fn next_id(&mut self) -> VoiceId {
        let id = VoiceId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    /// Non-blocking command submission. Returns false when the queue is full.
    fn send(&mut self, command: Command) -> bool {
        match self.tx.try_send(command) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                self.dropped += 1;
                self.counters.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
            Err(TrySendError::Disconnected(_)) => {
                self.dropped += 1;
                self.counters.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }

    pub fn play(&mut self, sound: Arc<Sound>, params: PlayParams) -> VoiceId {
        let id = self.next_id();
        self.send(Command::Play {
            id,
            source: Box::new(Source::Pcm(sound)),
            params,
        });
        id
    }

    /// Play an MP3 by streaming it (does not keep a decoded copy in memory).
    pub fn play_mp3(
        &mut self,
        bytes: Arc<[u8]>,
        params: PlayParams,
    ) -> Result<VoiceId, AudioError> {
        let stream = Mp3Stream::new(bytes)?;
        let id = self.next_id();
        self.send(Command::Play {
            id,
            source: Box::new(Source::Mp3(Box::new(stream))),
            params,
        });
        Ok(id)
    }

    pub fn stop(&mut self, id: VoiceId) {
        self.send(Command::Stop(id));
    }

    /// Fade a voice to silence over `ms` milliseconds, then release it.
    ///
    /// The outgoing half of a cross-fade: the new track is started with
    /// [`PlayParams::fade_in_ms`] set, so the two overlap without a click.
    pub fn fade_out(&mut self, id: VoiceId, ms: u32) {
        self.send(Command::FadeOut(id, ms));
    }

    pub fn stop_all(&mut self) {
        self.send(Command::StopAll);
    }

    pub fn stop_category(&mut self, category: Category) {
        self.send(Command::StopCategory(category));
    }

    pub fn set_gain(&mut self, id: VoiceId, gain: f32) {
        self.send(Command::SetGain(id, gain));
    }

    pub fn set_pan(&mut self, id: VoiceId, pan: f32) {
        self.send(Command::SetPan(id, pan));
    }

    pub fn set_position(&mut self, id: VoiceId, position: Option<Vec3>) {
        self.send(Command::SetPosition(id, position));
    }

    pub fn set_looping(&mut self, id: VoiceId, looping: bool) {
        self.send(Command::SetLooping(id, looping));
    }

    pub fn set_pitch(&mut self, id: VoiceId, pitch: f32) {
        self.send(Command::SetPitch(id, pitch));
    }

    pub fn set_master_gain(&mut self, gain: f32) {
        self.send(Command::SetMaster(gain));
    }

    pub fn set_category_gain(&mut self, category: Category, gain: f32) {
        self.send(Command::SetCategoryGain(category, gain));
    }

    /// Current bus gain for a category (used by pause/resume to restore it).
    pub fn category_gain(&self, category: Category) -> f32 {
        self.core
            .as_ref()
            .map(|core| core.category_gain[category.index()])
            .unwrap_or(1.0)
    }

    pub fn set_listener(&mut self, position: Vec3, yaw: f32) {
        self.send(Command::SetListener(position, yaw));
    }

    /// Install (or clear) the terrain height sampler used for occlusion.
    pub fn set_occluder(&mut self, sampler: Option<HeightSampler>) {
        self.send(Command::SetOccluder(sampler));
    }

    /// Duck one category by `amount_db` for `hold_seconds`, then release.
    ///
    /// The mixer attacks over 8 ms, holds, and releases over 250 ms, so a voice
    /// line lowers the music/ambient bus without an audible step. This is the
    /// port of kira's track-volume tween/send concept to our flat category
    /// buses.
    pub fn duck(&mut self, category: Category, amount_db: f32, hold_seconds: f32) {
        self.send(Command::Duck {
            category,
            amount_db,
            hold_seconds,
        });
    }

    /// Render one block (headless only). Returns the number of frames rendered.
    pub fn render(&mut self, out: &mut [f32]) -> usize {
        let Some(core) = self.core.as_mut() else {
            return 0;
        };
        self.pending.clear();
        if let Some(rx) = self.rx.as_ref() {
            while self.pending.len() < COMMAND_QUEUE_CAPACITY {
                match rx.try_recv() {
                    Ok(command) => self.pending.push(command),
                    Err(_) => break,
                }
            }
        }
        for command in self.pending.drain(..) {
            core.apply(command);
        }
        core.render(out, |_| {});
        out.len() / self.config.channels.max(1) as usize
    }

    /// Shut the render thread down (idempotent).
    ///
    /// The join is bounded: a backend stuck in a blocking write or in SDL's
    /// audio teardown must not keep the caller (a test binary, the game loop)
    /// alive. After `SHUTDOWN_JOIN_TIMEOUT` the thread is detached; process exit
    /// terminates it, and the device is left in whatever state the backend
    /// reached (SDL's audio subsystem is refcounted and process-scoped).
    pub fn shutdown(mut self) {
        self.stop_render_thread();
    }

    fn stop_render_thread(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        shutdown_log("begin");
        self.running.store(false, Ordering::Relaxed);
        shutdown_log("signalled");
        let started = Instant::now();
        // 1) Wait (bounded) for the render thread to hand the backend back.
        let mut backend: Option<Box<dyn AudioBackend>> = None;
        if let Some(rx) = self.backend_rx.take() {
            if let Ok(handed) = rx.recv_timeout(SHUTDOWN_HANDOFF_TIMEOUT) {
                backend = Some(handed);
                shutdown_log("backend_handed_off");
            } else {
                shutdown_log("handoff_timeout");
            }
        }
        // 2) Close the device on a short-lived thread with a hard budget.
        //
        // The opener thread is not enough: SDL's `SDL_DestroyAudioStream`
        // blocks forever on a suspended Pulse sink even there (measured:
        // `sdl_clear_end` then `sdl_destroy_begin` with no end), so a
        // synchronous close would reintroduce the process hang. The close
        // thread is detached on timeout; process exit reclaims it.
        if let Some(mut backend) = backend {
            let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
            let close_started = Instant::now();
            shutdown_log("close_thread_spawn");
            let handle = std::thread::Builder::new()
                .name("h2-audio-close".into())
                .spawn(move || {
                    backend.close();
                    let _ = done_tx.send(());
                });
            match handle {
                Ok(handle) => match done_rx.recv_timeout(SHUTDOWN_CLOSE_TIMEOUT) {
                    Ok(()) => {
                        let _ = handle.join();
                        shutdown_log("close_done");
                    }
                    Err(_) => {
                        shutdown_log("close_timeout");
                        // Detach the closer; the process may exit now.
                        drop(handle);
                    }
                },
                Err(_) => shutdown_log("close_thread_spawn_failed"),
            }
            shutdown_log(&format!(
                "close_wait_ms={}",
                close_started.elapsed().as_millis()
            ));
        }
        // 3) Join the (already finishing) render thread within the remaining
        // budget. In the normal path it has exited after the handoff, so this
        // costs ~0 ms.
        let deadline = started + SHUTDOWN_JOIN_TIMEOUT;
        while Instant::now() < deadline {
            if thread.is_finished() {
                let _ = thread.join();
                shutdown_log("joined");
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        shutdown_log("join_timeout");
        // Detach: dropping a JoinHandle does not wait; process exit reclaims it.
        drop(thread);
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.stop_render_thread();
    }
}

#[cfg(feature = "sdl")]
pub mod sdl3;

/// Open the platform playback backend, falling back to [`NullBackend`] when the
/// SDL3 library or audio device is unavailable. Returns the backend and its
/// name (`"sdl3"` or `"null"`) so callers can report the actual device path.
///
/// The null fallback is a *device* fallback, not a data fallback: decoding and
/// mixing still run on the real assets.
pub fn open_default_backend(config: &AudioConfig) -> (Box<dyn AudioBackend>, &'static str) {
    // Offline/CI runs (and the asset QA binaries) force the null device with
    // `H2_AUDIO=null`; decoding and mixing still use the real assets.
    if std::env::var_os("H2_AUDIO").is_some_and(|value| value == "null") {
        return (Box::new(NullBackend::default()), "null");
    }
    #[cfg(feature = "sdl")]
    {
        match sdl3::Sdl3Backend::open(config) {
            Ok(backend) => return (Box::new(backend), "sdl3"),
            Err(err) => {
                eprintln!("audio: SDL3 unavailable ({err}); using the null backend");
            }
        }
    }
    (Box::new(NullBackend::default()), "null")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(frames: usize, rate: u32, channels: u16, freq: f32) -> Arc<Sound> {
        let mut samples = Vec::with_capacity(frames * channels as usize);
        for frame in 0..frames {
            let value = (frame as f32 * freq * std::f32::consts::TAU / rate as f32).sin() * 0.5;
            for _ in 0..channels {
                samples.push(value);
            }
        }
        Arc::new(Sound {
            sample_rate: rate,
            channels,
            samples,
        })
    }

    fn config(rate: u32, frames: usize, voices: usize) -> AudioConfig {
        AudioConfig {
            sample_rate: rate,
            channels: 2,
            block_frames: frames,
            max_voices: voices,
            channel_voices: DEFAULT_CHANNEL_VOICES,
            master_gain: 1.0,
        }
    }

    #[test]
    fn plays_and_finishes_a_voice() {
        let mut engine = AudioEngine::headless(config(48_000, 256, 8));
        let sound = sine(512, 48_000, 1, 440.0);
        let id = engine.play(sound, PlayParams::default());
        let mut block = vec![0.0f32; 512];
        engine.render(&mut block);
        assert!(block.iter().any(|s| s.abs() > 0.01), "voice must produce audio");
        let stats = engine.stats();
        assert_eq!(stats.active_voices, 1);
        assert!(stats.frames_rendered >= 256);
        // Render past the end: the voice is freed.
        for _ in 0..4 {
            engine.render(&mut block);
        }
        let stats = engine.stats();
        assert_eq!(stats.active_voices, 0, "voice should finish");
        assert!(stats.voices_finished >= 1);
        let _ = id;
    }

    #[test]
    fn stop_ramps_down_to_silence() {
        let mut engine = AudioEngine::headless(config(48_000, 256, 8));
        let sound = sine(48_000, 48_000, 2, 220.0);
        let id = engine.play(sound, PlayParams::default());
        let mut block = vec![0.0f32; 512];
        engine.render(&mut block);
        assert!(block.iter().any(|s| s.abs() > 0.01));
        engine.stop(id);
        // Let the 5 ms ramp finish (256 frames = 5.3 ms at 48 kHz).
        engine.render(&mut block);
        engine.render(&mut block);
        let stats = engine.stats();
        assert_eq!(stats.active_voices, 0, "stopped voice is released");
    }

    #[test]
    fn gain_and_pan_follow_the_law() {
        let sound = sine(4096, 48_000, 1, 100.0);
        let mut block = vec![0.0f32; 2048];

        let mut engine = AudioEngine::headless(config(48_000, 1024, 8));
        engine.play(
            sound.clone(),
            PlayParams {
                gain: 0.5,
                ..Default::default()
            },
        );
        engine.render(&mut block);
        let centre_peak = block.iter().fold(0.0f32, |p, s| p.max(s.abs()));
        // 0.5 source amplitude * 0.5 gain * cos(pi/4) pan law = 0.1768.
        assert!((centre_peak - 0.1768).abs() < 0.03, "peak {centre_peak}");

        let mut engine = AudioEngine::headless(config(48_000, 1024, 8));
        engine.play(
            sound.clone(),
            PlayParams {
                pan: -1.0,
                ..Default::default()
            },
        );
        engine.render(&mut block);
        let left = block.chunks_exact(2).map(|f| f[0].abs()).fold(0.0f32, f32::max);
        let right = block.chunks_exact(2).map(|f| f[1].abs()).fold(0.0f32, f32::max);
        println!("hard-left: left={left} right={right}");
        assert!(left > 0.3, "left {left}");
        assert!(right < 0.01, "right {right}");

        let mut engine = AudioEngine::headless(config(48_000, 1024, 8));
        engine.play(
            sound,
            PlayParams {
                pan: 1.0,
                ..Default::default()
            },
        );
        engine.render(&mut block);
        let left = block.chunks_exact(2).map(|f| f[0].abs()).fold(0.0f32, f32::max);
        let right = block.chunks_exact(2).map(|f| f[1].abs()).fold(0.0f32, f32::max);
        assert!(right > 0.3 && left < 0.01, "left={left} right={right}");
    }

    #[test]
    fn three_d_position_pans_relative_to_the_listener() {
        let sound = sine(48_000, 48_000, 1, 100.0);
        let mut engine = AudioEngine::headless(config(48_000, 1024, 8));
        // Listener at origin looking +Y: right vector is +X.
        engine.set_listener(Vec3::ZERO, 0.0);
        engine.play(
            sound.clone(),
            PlayParams {
                position: Some(Vec3::new(-100.0, 0.0, 0.0)),
                reference_distance: 1000.0,
                ..Default::default()
            },
        );
        let mut block = vec![0.0f32; 2048];
        engine.render(&mut block);
        let left = block.chunks_exact(2).map(|f| f[0].abs()).fold(0.0f32, f32::max);
        let right = block.chunks_exact(2).map(|f| f[1].abs()).fold(0.0f32, f32::max);
        println!("source on listener left: left={left} right={right}");
        assert!(left > right, "left source must be louder on the left channel");

        // Far away: attenuation makes it quiet.
        let mut engine = AudioEngine::headless(config(48_000, 1024, 8));
        engine.play(
            sound,
            PlayParams {
                position: Some(Vec3::new(0.0, 0.0, 4000.0)),
                reference_distance: 100.0,
                max_distance: 0.0,
                ..Default::default()
            },
        );
        engine.render(&mut block);
        let peak = block.iter().fold(0.0f32, |p, s| p.max(s.abs()));
        assert!(peak < 0.01, "far source must be attenuated: {peak}");
    }

    #[test]
    fn looping_voice_keeps_playing() {
        let mut engine = AudioEngine::headless(config(48_000, 512, 4));
        let sound = sine(1000, 48_000, 1, 200.0);
        engine.play(
            sound,
            PlayParams {
                looping: true,
                ..Default::default()
            },
        );
        let mut block = vec![0.0f32; 1024];
        for _ in 0..50 {
            engine.render(&mut block);
        }
        let stats = engine.stats();
        assert_eq!(stats.active_voices, 1, "looping voice stays alive");
        assert!(block.iter().any(|s| s.abs() > 0.01));
    }

    #[test]
    fn category_gain_applies_to_the_bus() {
        let mut engine = AudioEngine::headless(config(48_000, 512, 4));
        engine.set_category_gain(Category::Music, 0.0);
        let sound = sine(4096, 48_000, 2, 100.0);
        engine.play(
            sound,
            PlayParams {
                category: Category::Music,
                ..Default::default()
            },
        );
        let mut block = vec![0.0f32; 1024];
        engine.render(&mut block);
        let peak = block.iter().fold(0.0f32, |p, s| p.max(s.abs()));
        assert!(peak < 1e-5, "muted music bus must be silent: {peak}");
    }

    #[test]
    fn voice_pool_caps_and_steals_by_priority() {        let mut engine = AudioEngine::headless(config(48_000, 256, 2));
        let sound = sine(4096, 48_000, 1, 100.0);
        let mut block = vec![0.0f32; 512];
        engine.play(sound.clone(), PlayParams::default());
        engine.play(sound.clone(), PlayParams::default());
        engine.render(&mut block);
        assert_eq!(engine.stats().active_voices, 2);
        // Third voice with equal priority is dropped, not queued forever.
        engine.play(sound.clone(), PlayParams::default());
        engine.render(&mut block);
        assert_eq!(engine.stats().active_voices, 2);
        // Higher priority steals a slot.
        engine.play(
            sound,
            PlayParams {
                priority: 100,
                ..Default::default()
            },
        );
        engine.render(&mut block);
        assert_eq!(engine.stats().active_voices, 2);
        assert_eq!(engine.stats().voices_stealed, 1);
    }

    /// `audio.channel-cap`: without `CHANNELFULLPREEMPT` a full channel drops
    /// the newcomer instead of evicting a voice, and channel 0 is unaffected.
    #[test]
    fn channel_cap_drops_without_preempt() {
        let mut engine = AudioEngine::headless(config(48_000, 256, 8));
        let sound = sine(4096, 48_000, 1, 100.0);
        let mut block = vec![0.0f32; 512];
        engine.play(
            sound.clone(),
            PlayParams {
                channel: 1,
                ..Default::default()
            },
        );
        engine.play(
            sound.clone(),
            PlayParams {
                channel: 1,
                ..Default::default()
            },
        );
        engine.render(&mut block);
        assert_eq!(engine.stats().active_voices, 1, "one voice per channel");
        assert_eq!(engine.stats().voices_channel_full, 1);
        assert_eq!(engine.stats().voices_stealed, 0, "the global pool is not full");
        // A different channel and an ungrouped voice both play.
        engine.play(
            sound.clone(),
            PlayParams {
                channel: 2,
                ..Default::default()
            },
        );
        engine.play(sound, PlayParams::default());
        engine.render(&mut block);
        assert_eq!(engine.stats().active_voices, 3);
    }

    /// `audio.channel-preempt`: with `CHANNELFULLPREEMPT` a full channel
    /// replaces its lowest-priority (then oldest) voice; a strictly lower
    /// priority still loses.
    #[test]
    fn channel_full_preempt_stops_the_lowest_priority_voice() {
        let mut engine = AudioEngine::headless(config(48_000, 256, 8));
        let sound = sine(4096, 48_000, 1, 100.0);
        let mut block = vec![0.0f32; 512];
        engine.play(
            sound.clone(),
            PlayParams {
                channel: 3,
                priority: 10,
                ..Default::default()
            },
        );
        engine.render(&mut block);
        // Equal priority + preempt: the old voice of the channel is replaced
        // (retail ack rows are all priority 1000 + CHANNELFULLPREEMPT).
        engine.play(
            sound.clone(),
            PlayParams {
                channel: 3,
                priority: 10,
                preempt: true,
                ..Default::default()
            },
        );
        engine.render(&mut block);
        assert_eq!(engine.stats().active_voices, 1);
        assert_eq!(engine.stats().voices_channel_preempted, 1);
        // Higher priority preempts too.
        engine.play(
            sound.clone(),
            PlayParams {
                channel: 3,
                priority: 20,
                preempt: true,
                ..Default::default()
            },
        );
        engine.render(&mut block);
        assert_eq!(engine.stats().active_voices, 1, "still one voice on the channel");
        assert_eq!(engine.stats().voices_channel_preempted, 2);
        // Lower priority cannot take the channel.
        engine.play(
            sound,
            PlayParams {
                channel: 3,
                priority: 5,
                preempt: true,
                ..Default::default()
            },
        );
        engine.render(&mut block);
        assert_eq!(engine.stats().active_voices, 1);
        assert_eq!(engine.stats().voices_channel_full, 1);
        assert_eq!(engine.stats().voices_started, 3);
    }

    /// `audio.nodup`: `NODUPLICATES` suppresses a second instance of the same
    /// row on the same channel while the first is alive.
    #[test]
    fn noduplicates_suppresses_a_second_copy() {
        let mut engine = AudioEngine::headless(config(48_000, 256, 8));
        let sound = sine(4096, 48_000, 1, 100.0);
        let mut block = vec![0.0f32; 512];
        let params = PlayParams {
            channel: 4,
            dedup: 0xABCD,
            ..Default::default()
        };
        engine.play(sound.clone(), params);
        engine.render(&mut block);
        engine.play(sound, params);
        engine.render(&mut block);
        assert_eq!(engine.stats().active_voices, 1);
        assert_eq!(engine.stats().voices_deduped, 1);
        assert_eq!(engine.stats().voices_channel_full, 0, "dedup runs first");
    }

    #[test]
    fn threaded_engine_writes_blocks_to_the_backend() {
        use std::sync::atomic::AtomicU64;
        struct CountingBackend(Arc<AtomicU64>);
        impl AudioBackend for CountingBackend {
            fn write(&mut self, samples: &[f32]) -> Result<usize, AudioError> {
                self.0.fetch_add(samples.len() as u64, Ordering::Relaxed);
                Ok(samples.len())
            }
            fn name(&self) -> &'static str {
                "counting"
            }
        }
        let written = Arc::new(AtomicU64::new(0));
        let mut engine = AudioEngine::start(
            config(48_000, 256, 4),
            Box::new(CountingBackend(written.clone())),
        );
        assert_eq!(engine.backend_name(), "counting");
        let sound = sine(48_000, 48_000, 1, 440.0);
        engine.play(sound, PlayParams::default());
        std::thread::sleep(Duration::from_millis(120));
        let stats = engine.stats();
        engine.shutdown();
        let total = written.load(Ordering::Relaxed);
        println!("threaded: written={total} stats={stats:?}");
        assert!(total > 0, "backend received no blocks");
        assert!(stats.frames_rendered > 0);
        assert!(stats.blocks_rendered > 0);
    }

    #[test]
    fn dropped_commands_are_counted_not_blocking() {
        let mut engine = AudioEngine::headless(config(48_000, 64, 4));
        let sound = sine(64, 48_000, 1, 100.0);
        for _ in 0..COMMAND_QUEUE_CAPACITY + 50 {
            engine.play(sound.clone(), PlayParams::default());
        }
        let stats = engine.stats();
        assert!(stats.commands_dropped >= 50, "dropped {}", stats.commands_dropped);
    }

    /// kira ILD pan (track/sub.rs:322-355): a centred source is balanced, a
    /// hard-left source is louder on the left ear, and a source behind the
    /// listener keeps the same left/right balance (no front/back blur).
    #[test]
    fn kira_ild_pans_sides_and_keeps_centre_balanced() {
        let sound = sine(48_000, 48_000, 1, 300.0);
        let mut block = vec![0.0f32; 2048];
        let mut measure = |position: Vec3| -> (f32, f32) {
            let mut engine = AudioEngine::headless(config(48_000, 1024, 8));
            engine.set_listener(Vec3::ZERO, 0.0);
            engine.play(
                sound.clone(),
                PlayParams {
                    position: Some(position),
                    reference_distance: 1000.0,
                    ..Default::default()
                },
            );
            engine.render(&mut block);
            let left = block.chunks_exact(2).map(|f| f[0].abs()).fold(0.0f32, f32::max);
            let right = block.chunks_exact(2).map(|f| f[1].abs()).fold(0.0f32, f32::max);
            (left, right)
        };
        let (left, right) = measure(Vec3::new(-400.0, 0.0, 0.0));
        assert!(left > right * 2.0, "left source left={left} right={right}");
        let (left, right) = measure(Vec3::new(400.0, 0.0, 0.0));
        assert!(right > left * 2.0, "right source left={left} right={right}");
        let (left, right) = measure(Vec3::new(0.0, 400.0, 0.0));
        let ratio = (left / right.max(1e-6)).clamp(0.0, 10.0);
        assert!(
            (0.85..1.18).contains(&ratio),
            "centred source must stay balanced: left={left} right={right}"
        );
    }

    /// kira `SpatialTrackDistances` + `Decibels::SILENCE..IDENTITY`
    /// interpolation: with a real max_distance the level follows
    /// `10^(-3 * relative_distance)`, so a half-way source is ~-22.6 dB.
    #[test]
    fn max_distance_uses_the_kira_db_ramp() {
        let sound = Arc::new(Sound {
            sample_rate: 48_000,
            channels: 1,
            samples: vec![0.5f32; 48_000],
        });
        let mut block = vec![0.0f32; 2048];
        let mut peak_at = |distance: f32| -> f32 {
            let mut engine = AudioEngine::headless(config(48_000, 1024, 8));
            engine.play(
                sound.clone(),
                PlayParams {
                    position: Some(Vec3::new(distance, 0.0, 0.0)),
                    reference_distance: 100.0,
                    max_distance: 2100.0,
                    ..Default::default()
                },
            );
            engine.render(&mut block);
            block.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()))
        };
        let near = peak_at(100.0);
        let middle = peak_at(1100.0);
        let far = peak_at(2100.0);
        println!("kira ramp near={near} middle={middle} far={far}");
        assert!(near > middle && middle > far, "monotonic ramp");
        let expected = near * 10f32.powf(-3.0 * 0.5);
        assert!(
            (middle - expected).abs() < expected * 0.15,
            "middle {middle} != kira ramp {expected}"
        );
        assert!(far < near * 0.01, "max_distance is inaudible: far={far}");
    }

    /// Terrain occlusion: a ridge between listener and source blocks the path,
    /// lowering the level and (via the one-pole low-pass) the 8 kHz band much
    /// more than the 300 Hz band.
    #[test]
    fn terrain_occlusion_muffles_the_high_band() {
        let play = |frequency: f32, ridge: bool| -> f32 {
            let mut engine = AudioEngine::headless(config(48_000, 1024, 8));
            if ridge {
                engine.set_occluder(Some(Arc::new(|point: Vec3| {
                    // A ridge across the whole path: every interior sample is
                    // below the solid surface, so the voice is fully occluded.
                    if point.x > 1.0 {
                        900.0
                    } else {
                        0.0
                    }
                })));
            } else {
                engine.set_occluder(Some(Arc::new(|_: Vec3| 0.0)));
            }
            let sound = sine(48_000, 48_000, 1, frequency);
            engine.play(
                sound,
                PlayParams {
                    position: Some(Vec3::new(2000.0, 0.0, 0.0)),
                    reference_distance: 2000.0,
                    looping: true,
                    ..Default::default()
                },
            );
            let mut block = vec![0.0f32; 2048];
            // 20 blocks = 0.85 s: the low-pass settles and the level can be
            // measured on a steady tone.
            for _ in 0..20 {
                engine.render(&mut block);
            }
            let peak = block.iter().fold(0.0f32, |p, s| p.max(s.abs()));
            if ridge {
                assert!(
                    engine.stats().occluded_voices >= 1,
                    "terrain raycast must mark the voice occluded"
                );
            }
            peak
        };
        let low_clear = play(300.0, false);
        let low_ridge = play(300.0, true);
        let high_clear = play(8000.0, false);
        let high_ridge = play(8000.0, true);
        println!(
            "occlusion low_clear={low_clear} low_ridge={low_ridge} high_clear={high_clear} high_ridge={high_ridge}"
        );
        assert!(low_ridge < low_clear * 0.9, "ridge lowers the level");
        assert!(
            high_ridge < high_clear * 0.35,
            "ridge must muffle the high band: high_clear={high_clear} high_ridge={high_ridge}"
        );
        assert!(
            high_ridge / high_clear.max(1e-6) < low_ridge / low_clear.max(1e-6),
            "occlusion must attenuate highs more than lows"
        );
    }

    /// Bus ducking (kira track-volume port): a voice line ducks the music bus
    /// within the 8 ms attack, holds, then releases back to unity.
    #[test]
    fn voice_ducking_lowers_and_recovers_the_music_bus() {
        let mut engine = AudioEngine::headless(config(48_000, 1024, 8));
        let sound = sine(48_000, 48_000, 1, 440.0);
        engine.play(
            sound,
            PlayParams {
                category: Category::Music,
                looping: true,
                ..Default::default()
            },
        );
        let mut block = vec![0.0f32; 2048];
        for _ in 0..3 {
            engine.render(&mut block);
        }
        let rms = |block: &[f32]| -> f32 {
            (block.iter().map(|s| s * s).sum::<f32>() / block.len().max(1) as f32).sqrt()
        };
        let baseline = rms(&block);
        assert!(baseline > 0.05, "music must be audible: {baseline}");
        engine.duck(Category::Music, -12.0, 0.10);
        for _ in 0..2 {
            engine.render(&mut block);
        }
        let ducked = rms(&block);
        let ratio = ducked / baseline;
        println!("ducking baseline={baseline} ducked={ducked} ratio={ratio}");
        assert!(ratio < 0.4, "music must duck towards -12 dB: ratio {ratio}");
        // Hold + release (0.10 s hold + 0.25 s release time constant). The
        // block is 1024 frames (stereo 2048 samples), so 40 blocks = 0.85 s.
        for _ in 0..40 {
            engine.render(&mut block);
        }
        let recovered = rms(&block);
        assert!(
            recovered > baseline * 0.85,
            "music must recover: baseline={baseline} recovered={recovered}"
        );
    }

    /// Listener orientation: with yaw = 90 degrees the right vector becomes +Y,
    /// so a source at +Y is on the listener's right.
    #[test]
    fn listener_yaw_rotates_the_spatial_field() {
        let sound = sine(48_000, 48_000, 1, 300.0);
        let mut engine = AudioEngine::headless(config(48_000, 1024, 8));
        engine.set_listener(Vec3::ZERO, std::f32::consts::FRAC_PI_2);
        engine.play(
            sound,
            PlayParams {
                position: Some(Vec3::new(0.0, 400.0, 0.0)),
                reference_distance: 1000.0,
                ..Default::default()
            },
        );
        let mut block = vec![0.0f32; 2048];
        engine.render(&mut block);
        let left = block.chunks_exact(2).map(|f| f[0].abs()).fold(0.0f32, f32::max);
        let right = block.chunks_exact(2).map(|f| f[1].abs()).fold(0.0f32, f32::max);
        assert!(right > left * 2.0, "rotated listener: left={left} right={right}");
    }
}

#[cfg(test)]
mod distance_probe {
    use super::*;

    #[test]
    fn reference_distance_is_honoured() {
        let config = AudioConfig {
            sample_rate: 48_000,
            channels: 2,
            block_frames: 512,
            max_voices: 8,
            channel_voices: DEFAULT_CHANNEL_VOICES,
            master_gain: 1.0,
        };
        let mut engine = AudioEngine::headless(config);
        let sound = Arc::new(Sound {
            sample_rate: 48_000,
            channels: 1,
            samples: vec![0.5f32; 48_000],
        });
        let id = engine.play(
            sound.clone(),
            PlayParams {
                gain: 1.0,
                position: Some(Vec3::new(300.0, 0.0, 0.0)),
                looping: true,
                reference_distance: 600.0,
                category: Category::Sfx,
                ..Default::default()
            },
        );
        let mut block = vec![0.0f32; 1024];
        engine.render(&mut block);
        let peak = block.iter().fold(0.0f32, |p, s| p.max(s.abs()));
        eprintln!("near peak={peak} id={id:?} stats={:?}", engine.stats());
        assert!(peak > 0.2, "near source too quiet: {peak}");
        engine.stop(id);
        for _ in 0..6 { engine.render(&mut block); }
        engine.play(
            sound,
            PlayParams {
                gain: 1.0,
                position: Some(Vec3::new(2400.0, 0.0, 0.0)),
                looping: true,
                reference_distance: 600.0,
                category: Category::Sfx,
                ..Default::default()
            },
        );
        engine.render(&mut block);
        let far = block.iter().fold(0.0f32, |p, s| p.max(s.abs()));
        eprintln!("far peak={far}");
        assert!(far < 0.05, "far source too loud: {far}");
    }
}
