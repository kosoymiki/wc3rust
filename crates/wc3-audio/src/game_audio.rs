//! Gameplay audio hooks: what the simulation calls, not what the mixer does.
//!
//! The hook layer is deliberately small and data-driven:
//!
//! ```ignore
//! let mut audio = GameAudio::new(engine, SoundInfoDb::load(&mut db), UnitNameMap::load(&mut db), db);
//! audio.unit_ack("hfoo", position);       // FootmanWhat1..4.wav from the archive
//! audio.unit_attack("ogru", position);    // GruntYesAttack1..3.wav
//! audio.ui_click();                       // InterfaceClick.wav
//! audio.music("Human1");                  // Sound\Music\mp3Music\Human1.mp3 (streamed)
//! ```
//!
//! `SoundProvider` decouples the hook layer from the asset database so
//! `wc3-audio` does not depend on `wc3-assetdb`; `wc3-assetdb` implements the
//! trait for `AssetDb` (see `analysis/agent-assets/INTEGRATION.md`).

use std::collections::HashMap;
use std::sync::Arc;

use glam::Vec3;

use crate::ambient::{
    render_ambient, AmbientTable, DlsBank, MidiSong, MAX_AMBIENT_SECONDS, MIDI_SOUNDS_TABLE,
    WORLD_EDIT_DATA,
};
use crate::engine::{AudioEngine, AudioStats, Category, PlayParams, VoiceId};
use crate::events::{AudioEvent, Material, MusicState, Race};
use crate::playlist::MusicPlaylist;
use crate::soundinfo::{SoundDef, SoundInfoDb, UnitNameMap, UnitWeaponSounds};
use crate::{AudioError, Sound};

/// Default ceiling for decoded PCM held by the ambience cache.
///
/// One rendered ambient theme is ~15.9 MB (`MAX_AMBIENT_SECONDS` of 44.1 kHz
/// stereo f32), so 64 MB keeps the theme that is playing plus the one the
/// player just left, and evicts the rest. War3 has 42 themes; caching them all
/// was 670 MB of samples for a game that plays one at a time.
pub const DEFAULT_AMBIENT_PCM_BUDGET: usize = 64 * 1024 * 1024;

/// Byte-bounded LRU of decoded ambient renders.
///
/// Only one theme is audible at a time (`ambient_voice`), so the cache exists
/// to avoid re-rendering the loop when the player walks back and forth over a
/// region border, not to keep the score resident. Eviction is by bytes, never
/// by entry count, because themes differ in length.
struct AmbientPcmCache {
    entries: HashMap<String, Arc<Sound>>,
    /// Least recently used first.
    order: std::collections::VecDeque<String>,
    bytes: usize,
    budget: usize,
    evictions: u64,
}

impl AmbientPcmCache {
    fn new(budget: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: std::collections::VecDeque::new(),
            bytes: 0,
            budget,
            evictions: 0,
        }
    }

    fn get(&mut self, name: &str) -> Option<Arc<Sound>> {
        let sound = self.entries.get(name).cloned()?;
        self.order.retain(|entry| entry != name);
        self.order.push_back(name.to_string());
        Some(sound)
    }

    fn insert(&mut self, name: &str, sound: Arc<Sound>) {
        let size = sound.memory_bytes();
        if let Some(previous) = self.entries.remove(name) {
            self.bytes = self.bytes.saturating_sub(previous.memory_bytes());
        }
        self.order.retain(|entry| entry != name);
        // A single theme larger than the budget is kept on its own: dropping it
        // would re-render the loop on every region change.
        while self.bytes + size > self.budget && !self.order.is_empty() {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(evicted) = self.entries.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(evicted.memory_bytes());
                self.evictions += 1;
            }
        }
        self.entries.insert(name.to_string(), sound);
        self.order.push_back(name.to_string());
        self.bytes += size;
    }

    fn bytes(&self) -> usize {
        self.bytes
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn evictions(&self) -> u64 {
        self.evictions
    }
}

/// What the hook layer needs from the asset layer.
pub trait SoundProvider {
    /// Load and decode a `.wav` from the archive (cached by the implementation).
    fn load_sound(&mut self, path: &str) -> Option<Arc<Sound>>;
    /// Raw bytes for a streamed format (`.mp3`).
    fn load_bytes(&mut self, path: &str) -> Option<Arc<[u8]>>;
    fn exists(&self, path: &str) -> bool;

    /// Map a table path to the path that actually exists in the provider.
    ///
    /// Retail `SoundInfo` tables contain stale directory bases; the asset layer
    /// resolves them through the listfile. The default keeps providers that only
    /// do exact lookups working unchanged.
    fn resolve(&self, path: &str) -> Option<String> {
        if self.exists(path) {
            Some(path.to_string())
        } else {
            None
        }
    }

    /// Drop decoded payload caches (textures, parsed models, raw bytes).
    ///
    /// The game layer copies what it draws while loading, so once the renderer
    /// has uploaded its GPU copies these caches are pure duplication. Returns
    /// the bytes the caches accounted for; providers with no cache return 0.
    fn release_decoded_caches(&mut self) -> usize {
        0
    }
}

/// UI feedback prompts the HUD raises from a `UiAction` outcome.
///
/// Retail plays a distinct cue per outcome (`InterfaceClick`, `InterfaceError`,
/// `GoldMineLow<Race>`, `NoFood<Race>`, `RallyPointPlace`); this enum is the
/// contract between the game's action results and those rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiFeedback {
    /// A button/command card tap was accepted.
    Click,
    /// The action could not be issued (bad target, busy, invalid).
    Error,
    /// Resource warning: not enough gold / gold mine running dry.
    LowGold,
    /// Supply warning: not enough food.
    LowFood,
    /// A move/attack/patrol point was placed.
    Waypoint,
}

impl UiFeedback {
    pub const ALL: [UiFeedback; 5] = [
        UiFeedback::Click,
        UiFeedback::Error,
        UiFeedback::LowGold,
        UiFeedback::LowFood,
        UiFeedback::Waypoint,
    ];

    pub fn id(self) -> &'static str {
        match self {
            UiFeedback::Click => "click",
            UiFeedback::Error => "error",
            UiFeedback::LowGold => "low_gold",
            UiFeedback::LowFood => "low_food",
            UiFeedback::Waypoint => "waypoint",
        }
    }

    /// Classify a `UiAction`/order result string emitted by the game.
    ///
    /// The game reports denials as short messages ("not enough gold", "cannot
    /// place there", "waypoint set"); the mapping is ordered so a resource
    /// denial wins over the generic error and a success is a click.
    pub fn for_result(result: &str) -> Option<UiFeedback> {
        let text = result.trim().to_ascii_lowercase();
        if text.is_empty() {
            return None;
        }
        if text.contains("waypoint") || text.contains("rally") || text.contains("move order")
        {
            return Some(UiFeedback::Waypoint);
        }
        if text.contains("gold") {
            return Some(UiFeedback::LowGold);
        }
        if text.contains("lumber") {
            // Lumber denial shares the generic deny cue; there is no
            // `NoLumber-` UI row in the shipped table.
            return Some(UiFeedback::Error);
        }
        if text.contains("food") || text.contains("supply") {
            return Some(UiFeedback::LowFood);
        }
        if text.contains("error")
            || text.contains("cannot")
            || text.contains("can't")
            || text.contains("deny")
            || text.contains("denied")
            || text.contains("invalid")
            || text.contains("busy")
        {
            return Some(UiFeedback::Error);
        }
        if text.contains("ok") || text.contains("click") || text.contains("select") {
            return Some(UiFeedback::Click);
        }
        None
    }
}

/// Hook tuning. Defaults match the retail tables: acknowledgement lines are
/// voice-priority, music is a looped stream on its own bus.
#[derive(Debug, Clone, Copy)]
pub struct GameAudioConfig {
    pub ack_category: Category,
    pub ack_priority: u32,
    pub music_category: Category,
    pub music_volume: f32,
    pub ui_volume: f32,
    /// Ambience/zone loops sit under the action, so they default quieter.
    pub ambient_volume: f32,
    pub default_reference_distance: f32,
    /// Music cross-fade length (outgoing track ramps down while the incoming
    /// track ramps up). 0 disables the overlap.
    pub crossfade_ms: u32,
    /// Region/ambience cross-fade length.
    pub ambient_crossfade_ms: u32,
    /// Minimum gap between two acknowledgement lines for the same unit.
    ///
    /// WC3 does not spam: a burst of orders on one unit replaces the line on
    /// its voice channel instead of stacking, which reads as one ack per short
    /// window. 0 disables the limiter (diagnostics).
    pub ack_min_interval_ms: u64,
}

impl Default for GameAudioConfig {
    fn default() -> Self {
        Self {
            ack_category: Category::Voice,
            ack_priority: 8,
            music_category: Category::Music,
            music_volume: 0.7,
            ui_volume: 0.9,
            ambient_volume: 0.55,
            default_reference_distance: 600.0,
            crossfade_ms: 1200,
            ambient_crossfade_ms: 800,
            ack_min_interval_ms: 150,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct GameAudioStats {
    // (fields below)
    pub acks_requested: u64,
    pub acks_started: u64,
    pub acks_missing: u64,
    /// Acknowledgements dropped by the per-unit `ack_min_interval_ms` limiter.
    pub acks_limited: u64,
    pub sounds_missing: u64,
    pub ui_events: u64,
    /// UI feedback prompts played through [`GameAudio::ui_feedback`].
    pub ui_feedback: u64,
    pub music_started: u64,
    pub music_missing: u64,
    /// Theme changes played as a cross-fade (music and ambience).
    pub music_crossfades: u64,
    /// Pause/resume transitions of the music+ambience buses.
    pub pauses: u64,
    pub ambient_crossfades: u64,
    /// Tileset ambience loops that started / could not be resolved.
    pub ambient_started: u64,
    pub ambient_missing: u64,
    /// Gameplay events routed through [`GameAudio::play_event`].
    pub events_played: u64,
    pub events_missing: u64,
    /// Weapon-impact rows started through [`GameAudio::attack_impact`].
    pub impacts_played: u64,
    /// Selection taps handled by [`GameAudio::selection_tap`].
    pub selection_taps: u64,
    /// Unit voices dropped because the source was outside the audible radius.
    pub voices_gated: u64,
    /// Acknowledgements dropped by the row's `DistanceCutoff` (not a gap).
    pub acks_gated: u64,
    /// Gameplay events dropped by the same gate (not a data gap).
    pub events_gated: u64,
}

impl GameAudioStats {
    /// Placeholder accessor kept out of the public field list; always zero.
    pub fn acs_missing_placeholder(&self) -> u64 {
        0
    }
}

/// One deterministic per-command voice resolution record.
///
/// Appended in command order when a unit command is played through one of the
/// def-id entry points ([`GameAudio::unit_ack`], [`GameAudio::unit_attack`],
/// [`GameAudio::unit_death`], [`GameAudio::unit_pissed`]). The pair
/// (`def_id`, `set`) is the evidence for operator bugs #4/#13: every unit
/// definition must speak with its own shipped `unitSound` set instead of a
/// shared race default. No audio recording is needed to audit it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitVoiceRecord {
    /// Unit definition id that received the command (lower-cased, e.g. `hpea`).
    pub def_id: String,
    /// Command kind: `ack`, `attack`, `death`, `pissed`, or a `*_limited`/`*_missing` marker.
    pub kind: &'static str,
    /// Resolved shipped `UnitUI.unitSound` set (e.g. `Peasant`); `none` when unresolved.
    pub set: String,
    /// `SoundInfo` row that actually answered (e.g. `PeasantWhat`).
    pub ack_def: String,
    /// Archive path of the clip that started for this command, if any. This is
    /// the file half of the deterministic def -> set -> row -> file record.
    pub file: Option<String>,
    /// Voice handle that started, if any (`VoiceId.0` is a `u32` index).
    pub voice: Option<u32>,
}

pub struct GameAudio<P: SoundProvider> {
    engine: AudioEngine,
    sounds: SoundInfoDb,
    units: UnitNameMap,
    /// `weapType1`/`weapType2` per unit (`Units/UnitWeapons.slk`), for the
    /// `UnitCombatSounds` weapon-impact rows.
    weapons: UnitWeaponSounds,
    provider: P,
    config: GameAudioConfig,
    /// Music themes: display name -> archive path (`.mp3`).
    themes: Vec<(String, String)>,
    /// Race rotation currently playing (hash-ordered, see [`crate::playlist`]).
    playlist: Option<MusicPlaylist>,
    /// Sim clock at which the current track started and its length, so
    /// [`GameAudio::tick_music`] can rotate the playlist when it ends.
    music_started_ms: u64,
    music_track_ms: u64,
    rng: u64,
    pub(crate) stats: GameAudioStats,
    music_voice: Option<VoiceId>,
    /// Tileset ambience: the loaded `MIDISounds`/`WorldEditData` tables, the
    /// currently playing loop and the rendered PCM per theme.
    ambient_table: Option<AmbientTable>,
    ambient_voice: Option<VoiceId>,
    /// Decoded bytes of the ambience voice currently playing. It shares its
    /// `Arc<Sound>` with the cache, so [`GameAudio::pcm_bytes`] subtracts this
    /// from the engine's resident PCM to avoid counting it twice.
    ambient_voice_bytes: usize,
    ambient_cache: AmbientPcmCache,
    /// Region/tileset/day-night key the running ambience loop belongs to.
    ambient_key: Option<(String, char, bool)>,
    /// Listener pose mirrored from [`GameAudio::set_listener`] for the
    /// audibility gate (the engine owns its own copy for panning).
    listener: Vec3,
    /// Unit voices farther than this from the listener are not started.
    /// 0 disables the gate (tests/offline renders).
    audible_radius: f32,
    /// Set when the last unit-voice attempt was dropped by the gate, so event
    /// accounting reports a gate instead of a missing row.
    pub(crate) gated: bool,
    /// Clip/theme names that could not be resolved, with the reason
    /// (`music:Human1`, `ambient:LordaeronSummerDay midi=...`).
    unresolved: Vec<String>,
    /// Consecutive selection taps per unit id (retail "pissed" counter).
    selection_taps: HashMap<String, u32>,
    /// Last acknowledgement time per unit id, in [`GameAudio::clock_ms`].
    ack_times: HashMap<String, u64>,
    /// Deterministic gameplay clock used by the ack limiter (milliseconds).
    clock_ms: u64,
    /// Music/ambience are muted while the game is paused.
    paused: bool,
    /// Bus gains captured when the pause started, restored on resume.
    paused_gains: Option<(f32, f32)>,
    /// Gameplay events that had no sound, already reported.
    ///
    /// `AUDIO_EVENT missing=<event>` is emitted once per event, never once per
    /// frame: a silent gap must be visible in the device log without flooding it.
    missing_events: Vec<&'static str>,
    /// The last event rendered as one line (device verification asserts it).
    last_event: Option<String>,
    /// Deterministic def id -> ack set log, one record per unit command.
    voice_log: Vec<UnitVoiceRecord>,
    /// Archive path of the last clip that actually started (event -> file
    /// evidence for the device log and the event table probe).
    pub(crate) last_sound_file: Option<String>,
    /// Emit `AUDIO_*` event lines on stderr (QA/device-log gate).
    event_log: bool,
}

impl<P: SoundProvider> GameAudio<P> {
    pub fn new(
        engine: AudioEngine,
        sounds: SoundInfoDb,
        units: UnitNameMap,
        provider: P,
    ) -> Self {
        Self {
            engine,
            sounds,
            units,
            weapons: UnitWeaponSounds::default(),
            provider,
            config: GameAudioConfig::default(),
            themes: Vec::new(),
            playlist: None,
            music_started_ms: 0,
            music_track_ms: 0,
            rng: 0x2545_F491_4F6C_DD1D,
            stats: GameAudioStats::default(),
            music_voice: None,
            ambient_table: None,
            ambient_voice: None,
            ambient_voice_bytes: 0,
            ambient_cache: AmbientPcmCache::new(DEFAULT_AMBIENT_PCM_BUDGET),
            ambient_key: None,
            listener: Vec3::ZERO,
            audible_radius: 0.0,
            gated: false,
            unresolved: Vec::new(),
            selection_taps: HashMap::new(),
            ack_times: HashMap::new(),
            clock_ms: 0,
            paused: false,
            paused_gains: None,
            missing_events: Vec::new(),
            last_event: None,
            voice_log: Vec::new(),
            last_sound_file: None,
            event_log: std::env::var_os("H2_AUDIO_LOG").is_some(),
        }
    }

    /// Enable or disable `AUDIO_*` event lines (the QA device-log gate greps
    /// `AUDIO_ACK` / `AUDIO_MUSIC`).
    pub fn set_event_log(&mut self, enabled: bool) {
        self.event_log = enabled;
    }

    pub(crate) fn emit(&mut self, kind: &str, detail: &str, voice: Option<VoiceId>) {
        let line = match voice {
            Some(id) => format!("AUDIO_{kind} {detail} voice={}", id.0),
            None => format!("AUDIO_{kind} {detail} voice=none"),
        };
        self.last_event = Some(line.clone());
        if self.event_log {
            eprintln!("{line}");
        }
    }

    /// Record a failure/diagnostic clip name so the device log can list every
    /// unresolved theme/sound instead of a silent gap.
    pub(crate) fn note_unresolved(&mut self, what: impl Into<String>) {
        let what = what.into();
        if self.unresolved.len() < 64 && !self.unresolved.contains(&what) {
            self.unresolved.push(what);
        }
    }

    /// One-line device verification summary: honest hook counters plus the
    /// engine queue/loss counters the device log asserts.
    pub fn stats_line(&self) -> String {
        let engine = self.engine.stats();
        let stats = &self.stats;
        let (cache_entries, cache_bytes, cache_evictions, cache_budget) = self.ambient_cache_stats();
        format!(
            "GAME_AUDIO_HOOKS device={} acks={}/{} limited={} missing={} acks_gated={} gated={} ui={} feedback={} events={} events_gated={} impacts={} music={}/{} crossfades={} ambient={}/{} ambient_crossfades={} taps={} unresolved={} pcm_mb={:.1} cache={}/{}_mb evicted={} budget_mb={} buffered_ms={:?} underruns={} dropped={} occluded={} ducked_blocks={} soft_lim={} voices={} max_active={} chan_full={} chan_preempt={} deduped={}",
            self.engine.backend_name(),
            stats.acks_started,
            stats.acks_requested,
            stats.acks_limited,
            stats.acks_missing,
            stats.acks_gated,
            stats.voices_gated,
            stats.ui_events,
            stats.ui_feedback,
            stats.events_played,
            stats.events_gated,
            stats.impacts_played,
            stats.music_started,
            stats.music_missing,
            stats.music_crossfades,
            stats.ambient_started,
            stats.ambient_missing,
            stats.ambient_crossfades,
            stats.selection_taps,
            self.unresolved.len(),
            self.pcm_bytes() as f32 / (1024.0 * 1024.0),
            cache_entries,
            cache_bytes / (1024 * 1024),
            cache_evictions,
            cache_budget / (1024 * 1024),
            self.engine.buffered_ms(),
            engine.buffer_underruns,
            engine.commands_dropped,
            engine.occluded_voices,
            engine.ducked_blocks,
            engine.soft_limited_samples,
            engine.active_voices,
            engine.max_active_voices,
            engine.voices_channel_full,
            engine.voices_channel_preempted,
            engine.voices_deduped,
        )
    }

    /// Clip/theme names that did not resolve (music, ambience, event rows), in
    /// first-failure order.
    pub fn unresolved(&self) -> &[String] {
        &self.unresolved
    }

    /// Record an integration gap (a table the wiring expected to load), so the
    /// device report names it instead of silently going without.
    pub fn note_wiring_gap(&mut self, what: impl Into<String>) {
        self.note_unresolved(format!("wiring:{}", what.into()));
    }

    /// Last `AUDIO_*` line (also emitted on stderr when `H2_AUDIO_LOG` is set).
    pub fn last_event_line(&self) -> Option<&str> {
        self.last_event.as_deref()
    }

    /// Append one per-command resolution to the deterministic voice log.
    fn record_unit_voice(
        &mut self,
        def_id: &str,
        kind: &'static str,
        set: &str,
        ack_def: &str,
        voice: Option<VoiceId>,
    ) {
        // `play_def_with` sets `last_sound_file` only when the clip started, and
        // every entry point records immediately after its own play call, so the
        // file belongs to this command; a missing/gated command records none.
        let file = voice.and_then(|_| self.last_sound_file.clone());
        self.voice_log.push(UnitVoiceRecord {
            def_id: def_id.to_ascii_lowercase(),
            kind,
            set: set.to_string(),
            ack_def: ack_def.to_string(),
            file,
            voice: voice.map(|id| id.0),
        });
    }

    /// Resolve the ack set and `SoundInfo` row a def id would use for a unit
    /// command, without starting a voice. This is the pure resolver the log
    /// records and the per-def distinctness test asserts.
    pub fn resolve_unit_voice(&self, def_id: &str) -> Option<(String, String)> {
        let (set, def) = self.unit_ack_definition(def_id)?;
        Some((set, def.name))
    }

    /// Deterministic def id -> ack set log, one record per unit command in the
    /// order the commands were played. Per-def distinctness is asserted from
    /// this log without an audio recording.
    pub fn unit_voice_log(&self) -> &[UnitVoiceRecord] {
        &self.voice_log
    }

    /// Clear the log at a test/phase boundary.
    pub fn clear_unit_voice_log(&mut self) {
        self.voice_log.clear();
    }

    /// Archive path of the clip that started last (`event -> file` evidence).
    pub fn last_sound_file(&self) -> Option<&str> {
        self.last_sound_file.as_deref()
    }

    pub fn with_config(mut self, config: GameAudioConfig) -> Self {
        self.config = config;
        self
    }

    pub fn engine(&mut self) -> &mut AudioEngine {
        &mut self.engine
    }

    pub fn stats(&self) -> GameAudioStats {
        self.stats.clone()
    }

    pub fn audio_stats(&self) -> AudioStats {
        self.engine.stats()
    }

    /// Playback queue depth in milliseconds (device diagnostics).
    ///
    /// `None` when the backend cannot report queue depth (headless/null).
    pub fn buffered_ms(&self) -> Option<u32> {
        self.engine.buffered_ms()
    }

    pub fn sound_table(&self) -> &SoundInfoDb {
        &self.sounds
    }

    /// The asset provider (diagnostics and cache release).
    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Mutable asset provider (cache release after the renderer uploads).
    pub fn provider_mut(&mut self) -> &mut P {
        &mut self.provider
    }

    /// Decoded PCM the hook layer keeps resident: the ambience LRU plus the
    /// clips the mixer is currently playing (unit acks are short; the ambient
    /// loop is the long one). This is the number the memory budget caps.
    ///
    /// The playing ambience loop shares its `Arc<Sound>` with the cache, so its
    /// bytes are subtracted from the engine's resident total — counting the
    /// same samples twice would make the 64 MB cap look exceeded while the
    /// process holds exactly one copy.
    pub fn pcm_bytes(&self) -> usize {
        self.ambient_cache.bytes()
            + self
                .engine
                .pcm_bytes()
                .saturating_sub(self.ambient_voice_bytes)
    }

    /// Replace the decoded-PCM ceiling for the ambience cache (bytes).    ///
    /// The cache evicts least-recently-used themes until the next theme fits;
    /// a single theme larger than the budget is kept on its own so the loop is
    /// not re-rendered every region change. The ceiling is reported by
    /// [`GameAudio::ambient_cache_stats`] and asserted by the budget test.
    pub fn set_ambient_pcm_budget(&mut self, bytes: usize) {
        self.ambient_cache.budget = bytes.max(1);
    }

    /// Ambient PCM ceiling in bytes.
    pub fn ambient_pcm_budget(&self) -> usize {
        self.ambient_cache.budget
    }

    /// Pause the score: music and ambience are muted, gameplay UI/voices keep
    /// working (retail pauses the soundtrack, not the interface).
    ///
    /// The previous bus gains are captured and restored on resume, so a custom
    /// mix is not clobbered by a pause.
    pub fn set_paused(&mut self, paused: bool) {
        if paused == self.paused {
            return;
        }
        self.paused = paused;
        if paused {
            let music = self.engine.category_gain(Category::Music);
            let ambient = self.engine.category_gain(Category::Ambient);
            self.paused_gains = Some((music, ambient));
            self.engine.set_category_gain(Category::Music, 0.0);
            self.engine.set_category_gain(Category::Ambient, 0.0);
            self.stats.pauses += 1;
        } else {
            let (music, ambient) = self.paused_gains.take().unwrap_or((1.0, 1.0));
            self.engine
                .set_category_gain(Category::Music, music.max(1e-6));
            self.engine
                .set_category_gain(Category::Ambient, ambient.max(1e-6));
        }
        self.emit("PAUSE", &format!("paused={}", paused as u8), None);
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Ambience cache accounting: `(entries, bytes, evictions, budget)`.
    pub fn ambient_cache_stats(&self) -> (usize, usize, u64, usize) {
        (
            self.ambient_cache.len(),
            self.ambient_cache.bytes(),
            self.ambient_cache.evictions,
            self.ambient_cache.budget,
        )
    }

    /// Ask the provider to drop its decoded caches (called once the renderer
    /// has uploaded the textures). Returns the bytes the provider accounted for.
    pub fn release_decoded_caches(&mut self) -> usize {
        self.provider.release_decoded_caches()
    }

    pub fn unit_names(&self) -> &UnitNameMap {
        &self.units
    }

    /// Load `Units/UnitWeapons.slk` (unit id -> `weapType1` family).
    ///
    /// Returns the number of rows with a real weapon family. The table is used
    /// only by [`GameAudio::attack_impact`]; a missing table leaves impacts
    /// silent-and-reported, never guessed.
    pub fn load_weapon_table(&mut self, bytes: &[u8]) -> usize {
        match UnitWeaponSounds::parse(bytes) {
            Ok(table) => {
                let rows = table.len();
                self.weapons = table;
                rows
            }
            Err(_) => {
                self.note_unresolved("weapon_table_parse:UnitWeapons.slk".to_string());
                0
            }
        }
    }

    /// Unit id -> weapon sound family (diagnostics / coverage).
    pub fn weapon_table(&self) -> &UnitWeaponSounds {
        &self.weapons
    }

    /// Weapon impact on a target (`UnitCombatSounds` `<weapType><material>`).
    ///
    /// `attacker` supplies the weapon family from `UnitWeapons.slk`, `target`
    /// supplies the material class from `unitUI.slk` (`Flesh` default when the
    /// target is unknown — the shipped table's most common class). The row that
    /// started is logged as `AUDIO_IMPACT unit=… target=… row=…`, so a missing
    /// combat row is a named gap rather than a silent hit.
    pub fn attack_impact(
        &mut self,
        attacker: Option<&str>,
        target: Option<&str>,
        position: Option<Vec3>,
    ) -> Option<VoiceId> {
        let weapon = attacker
            .and_then(|id| self.weapons.weapon_type(id))
            .map(str::to_string);
        let material = target
            .and_then(|id| self.units.material(id))
            .unwrap_or(Material::Flesh);
        let Some(weapon) = weapon else {
            self.report_missing_event(
                AudioEvent::AttackImpact,
                &format!(
                    "reason=no_weapon unit={} target={}",
                    attacker.unwrap_or("-"),
                    target.unwrap_or("-")
                ),
            );
            return None;
        };
        let row = format!("{weapon}{}", material.suffix());
        let voice = self.play_sound(&row, position);
        if voice.is_some() {
            self.stats.impacts_played += 1;
        } else if !self.gated {
            self.report_missing_event(
                AudioEvent::AttackImpact,
                &format!("row={row} target={}", target.unwrap_or("-")),
            );
        }
        self.emit(
            "IMPACT",
            &format!(
                "unit={} target={} row={row} material={}",
                attacker.unwrap_or("-"),
                target.unwrap_or("-"),
                material.suffix()
            ),
            voice,
        );
        voice
    }

    /// Report a gameplay event that produced no sound, once per event id.
    ///
    /// Emits `AUDIO_EVENT missing=<event> …` the first time the gap is hit and
    /// counts every occurrence in `events_missing`, so a silent event is
    /// visible in the device log without flooding it every frame.
    pub(crate) fn report_missing_event(&mut self, event: AudioEvent, detail: &str) {
        self.stats.events_missing += 1;
        let id = event.id();
        if self.missing_events.contains(&id) {
            return;
        }
        self.missing_events.push(id);
        self.note_unresolved(format!("event:{id}"));
        self.emit("EVENT", &format!("missing={id} {detail}"), None);
    }

    /// Event ids that have hit the missing path this session (report/QA).
    pub fn missing_events(&self) -> &[&'static str] {
        &self.missing_events
    }

    /// Deterministic gameplay clock in milliseconds.
    ///
    /// The ack limiter reads this clock, never the wall clock: replays, tests
    /// and the device run must take the same decision for the same input.
    pub fn clock_ms(&self) -> u64 {
        self.clock_ms
    }

    /// Set the gameplay clock (idempotent; call once per frame with sim time).
    pub fn set_clock_ms(&mut self, ms: u64) {
        self.clock_ms = ms;
    }

    /// Advance the gameplay clock by a frame/tick delta.
    pub fn advance_ms(&mut self, ms: u64) {
        self.clock_ms = self.clock_ms.saturating_add(ms);
    }

    /// Play a `war3map.w3s` custom sound (MP3 streamed, WAV decoded).
    ///
    /// Map sounds are position-driven: pass the region centre (or trigger
    /// point) so the normal 3D attenuation applies.
    pub fn play_map_sound(
        &mut self,
        sound: &crate::map_sounds::MapSound,
        position: Option<Vec3>,
        looping: bool,
    ) -> Option<VoiceId> {
        let path = self.provider.resolve(&sound.file)?;
        let params = PlayParams {
            gain: 1.0,
            position,
            looping,
            category: Category::Sfx,
            ..Default::default()
        };
        let id = if sound.file.to_ascii_lowercase().ends_with(".mp3") {
            let bytes = self.provider.load_bytes(&path)?;
            self.engine.play_mp3(bytes, params).ok()?
        } else {
            let decoded = self.provider.load_sound(&path)?;
            self.engine.play(decoded, params)
        };
        self.emit("MAP", &format!("{} loop={looping}", sound.name), Some(id));
        Some(id)
    }

    /// Play a map sound at its region centre (ambient loop by default).
    pub fn play_map_ambient(
        &mut self,
        sound: &crate::map_sounds::MapSound,
        region: &crate::map_sounds::MapRegion,
    ) -> Option<VoiceId> {
        self.play_map_sound(sound, Some(region.center()), true)
    }

    pub fn set_master_gain(&mut self, gain: f32) {
        self.engine.set_master_gain(gain);
    }

    /// Deterministic variant pick (xorshift): retail picks a random file from
    /// the sound's list, and tests need to reproduce it.
    fn next_rand(&mut self) -> usize {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        (x >> 32) as usize
    }

    /// Play a sound-table definition with an explicit category/loop mode.
    fn play_def_with(
        &mut self,
        def: &SoundDef,
        position: Option<Vec3>,
        category: Category,
        looping: bool,
        priority: u32,
    ) -> Option<VoiceId> {
        let paths = def.paths();
        if paths.is_empty() {
            self.stats.sounds_missing += 1;
            return None;
        }
        // WC3 distance fields (measured on the shipped `UnitAckSounds` /
        // `AnimSounds` rows): `MinDistance` is the near radius (full volume),
        // `MaxDistance` is the far end of the attenuation curve (silence).
        let (reference, far) = def.playback_distances(self.config.default_reference_distance);
        // Retail audibility rule (`StartSound` 0x6f102980, `fcom` 0x6f102b0b):
        // a 3-D sound never starts when `dist² > DistanceCutoff²`. The row's
        // own cutoff is authoritative; a 0 row parses as the SoundData default
        // 3000. `audible_radius` is an optional extra cap for diagnostics
        // (0 = off) and is never the primary rule.
        self.gated = false;
        if let Some(position) = position {
            let distance_squared = (position - self.listener).length_squared();
            let cutoff = def.audible_cutoff();
            let row_denied = distance_squared > cutoff * cutoff;
            let radius_denied = self.audible_radius > 0.0
                && distance_squared > self.audible_radius * self.audible_radius;
            if row_denied || radius_denied {
                self.gated = true;
                self.stats.voices_gated += 1;
                self.emit(
                    "GATED",
                    &format!(
                        "{} distance={:.1} cutoff={:.1} radius={:.1}",
                        def.name,
                        distance_squared.sqrt(),
                        cutoff,
                        self.audible_radius
                    ),
                    None,
                );
                return None;
            }
        }
        let pick = self.next_rand() % paths.len();
        // Try the picked variant, then the rest, so one missing file does not
        // silence the sound.
        for offset in 0..paths.len() {
            let requested = &paths[(pick + offset) % paths.len()];
            let Some(path) = self.provider.resolve(requested) else {
                continue;
            };
            if let Some(sound) = self.provider.load_sound(&path) {
                self.last_sound_file = Some(path.clone());
                let params = PlayParams {
                    gain: def.volume.clamp(0.0, 1.0),
                    position,
                    looping,
                    category,
                    priority,
                    reference_distance: reference,
                    max_distance: far,
                    // Engine voice grouping: the row's `Channel` is the mixer
                    // channel, `CHANNELFULLPREEMPT` allows evicting a
                    // lower-priority voice of the same channel, and
                    // `NODUPLICATES` suppresses a second copy of the same row.
                    channel: def.channel_number(),
                    preempt: def.has_flag("CHANNELFULLPREEMPT"),
                    dedup: def.dedup_key(),
                    ..Default::default()
                };
                let id = self.engine.play(sound, params);
                // Group ducking: a spoken line pulls the music and ambience
                // down (fast attack, held, slow release inside the mixer), so
                // dialogue stays intelligible without a manual bus mix.
                if category == Category::Voice {
                    self.engine.duck(Category::Music, -8.0, 1.0);
                    self.engine.duck(Category::Ambient, -3.0, 1.0);
                }
                return Some(id);
            }
        }
        self.stats.sounds_missing += 1;
        None
    }

    fn play_def(&mut self, def: &SoundDef, position: Option<Vec3>, priority: u32) -> Option<VoiceId> {
        let category = self.config.ack_category;
        self.play_def_with(def, position, category, false, priority)
    }

    /// Spell/ability sound from the retail `AbilitySounds` table
    /// (e.g. `BlizzardWave`, `RainOfFireWave`, `HumanFireMedium`).
    pub fn spell(&mut self, sound_name: &str, position: Option<Vec3>) -> Option<VoiceId> {
        let def = self.sounds.get(sound_name)?.clone();
        let id = self.play_def_with(&def, position, Category::Sfx, false, def.priority.max(6));
        self.emit("SPELL", sound_name, id);
        id
    }

    /// Looping ambience/zone loop from the retail `AmbienceSounds` table
    /// (e.g. `AmbientSoundRain`, `WaterfallLoop`, `CauldronLoop`).
    pub fn ambient(&mut self, sound_name: &str) -> Option<VoiceId> {
        let def = self.sounds.get(sound_name)?.clone();
        let gain = (def.volume * self.config.ambient_volume).clamp(0.0, 1.0);
        let paths = def.paths();
        for requested in &paths {
            let Some(path) = self.provider.resolve(requested) else {
                continue;
            };
            if let Some(sound) = self.provider.load_sound(&path) {
                let params = PlayParams {
                    gain,
                    position: None,
                    looping: true,
                    category: Category::Ambient,
                    priority: def.priority,
                    channel: def.channel_number(),
                    preempt: def.has_flag("CHANNELFULLPREEMPT"),
                    dedup: def.dedup_key(),
                    ..Default::default()
                };
                let id = self.engine.play(sound, params);
                self.stats.ambient_started += 1;
                self.emit("AMBIENT", sound_name, Some(id));
                return Some(id);
            }
        }
        self.stats.sounds_missing += 1;
        self.stats.ambient_missing += 1;
        self.note_unresolved(format!("ambient:{sound_name}"));
        None
    }

    /// Stop one voice (ack, ambience, music, ...).
    pub fn stop_voice(&mut self, id: VoiceId) {
        self.engine.stop(id);
    }

    /// Unit acknowledgement ("What?" / "Yes?"), resolved through
    /// unit id -> own `unitSound` set (`{set}What` / `{set}Yes`).
    ///
    /// The id is the four-letter definition id, so every unit speaks with its
    /// own shipped set (`hpea` -> Peasant, `opeo` -> Peon, ...), including map
    /// aliases (`hgrunt` -> `ogru` -> Grunt).
    pub fn unit_ack(&mut self, unit_id: &str, position: Option<Vec3>) -> Option<VoiceId> {
        self.stats.acks_requested += 1;
        if let Some(gap) = self.ack_gap_ms(unit_id)
            && gap < self.config.ack_min_interval_ms
        {
            self.stats.acks_limited += 1;
            self.emit(
                "ACK",
                &format!("{unit_id} limited=1 gap_ms={gap}"),
                None,
            );
            self.record_unit_voice(unit_id, "ack_limited", "none", "none", None);
            return None;
        }
        let Some((set, def)) = self.unit_ack_definition(unit_id) else {
            self.stats.acks_missing += 1;
            self.note_unresolved(format!("unit:{unit_id}"));
            self.emit("ACK", &format!("{unit_id} set=none"), None);
            self.record_unit_voice(unit_id, "ack_missing", "none", "none", None);
            return None;
        };
        self.mark_ack(unit_id);
        let id = self.play_def(&def, position, self.config.ack_priority);
        let gated = self.gated;
        if id.is_some() {
            self.stats.acks_started += 1;
        } else if gated {
            self.stats.acks_gated += 1;
        } else {
            self.stats.acks_missing += 1;
            self.note_unresolved(format!("ack_def:{}", def.name));
        }
        self.emit(
            "ACK",
            &format!("{unit_id} set={set} def={} gated={}", def.name, gated as u8),
            id,
        );
        self.record_unit_voice(unit_id, "ack", &set, &def.name, id);
        id
    }

    /// Milliseconds since `unit_id` last acknowledged (`None` = never).
    fn ack_gap_ms(&self, unit_id: &str) -> Option<u64> {
        let key = unit_id.to_ascii_lowercase();
        self.ack_times
            .get(&key)
            .map(|last| self.clock_ms.saturating_sub(*last))
    }

    fn mark_ack(&mut self, unit_id: &str) {
        self.ack_times
            .insert(unit_id.to_ascii_lowercase(), self.clock_ms);
    }

    /// Resolve the acknowledgement row the hooks would play for a unit id.
    ///
    /// Walks the unit's own object-data sets ([`UnitNameMap::sound_set_candidates`]:
    /// TFT `unitSound` first, display-name set second for the RoC schema) and
    /// returns the first set that owns a `UnitAckSounds` row. Returns the set
    /// and the row, so device logs, probes and tests can name both the
    /// `UnitUI.slk` cell and the `UnitAckSounds.slk` row that selected the file.
    pub fn unit_ack_definition(&self, unit_id: &str) -> Option<(String, SoundDef)> {
        for set in self.units.sound_set_candidates(unit_id) {
            if let Some(def) = self.sounds.unit_ack(set) {
                return Some((set.to_string(), def.clone()));
            }
        }
        None
    }

    /// Attack acknowledgement from the unit's own set (`{set}YesAttack`).
    ///
    /// Shares the per-unit ack limiter with [`GameAudio::unit_ack`]: a unit
    /// stuck in an attack loop repeats the line on its voice channel, it does
    /// not stack a new one every frame.
    pub fn unit_attack(&mut self, unit_id: &str, position: Option<Vec3>) -> Option<VoiceId> {
        if let Some(gap) = self.ack_gap_ms(unit_id)
            && gap < self.config.ack_min_interval_ms
        {
            self.stats.acks_limited += 1;
            self.emit(
                "ATTACK",
                &format!("{unit_id} limited=1 gap_ms={gap}"),
                None,
            );
            self.record_unit_voice(unit_id, "attack_limited", "none", "none", None);
            return None;
        }
        let Some((set, def)) = self.unit_attack_definition(unit_id) else {
            self.record_unit_voice(unit_id, "attack_missing", "none", "none", None);
            return None;
        };
        self.mark_ack(unit_id);
        let id = self.play_def(&def, position, self.config.ack_priority);
        self.emit("ATTACK", &format!("{unit_id} set={set} def={}", def.name), id);
        self.record_unit_voice(unit_id, "attack", &set, &def.name, id);
        id
    }

    /// Resolve the attack-ack row through the unit's own object-data sets.
    pub fn unit_attack_definition(&self, unit_id: &str) -> Option<(String, SoundDef)> {
        for set in self.units.sound_set_candidates(unit_id) {
            if let Some(def) = self.sounds.unit_attack(set) {
                return Some((set.to_string(), def.clone()));
            }
        }
        None
    }

    /// Death line from the unit's own set (`{set}Death` in `AnimSounds`).
    pub fn unit_death(&mut self, unit_id: &str, position: Option<Vec3>) -> Option<VoiceId> {
        let Some((set, def)) = self.unit_death_definition(unit_id) else {
            self.record_unit_voice(unit_id, "death_missing", "none", "none", None);
            return None;
        };
        let id = self.play_def(&def, position, self.config.ack_priority - 1);
        self.emit("DEATH", &format!("{unit_id} set={set} def={}", def.name), id);
        self.record_unit_voice(unit_id, "death", &set, &def.name, id);
        id
    }

    /// Resolve the death row through the unit's own object-data sets.
    pub fn unit_death_definition(&self, unit_id: &str) -> Option<(String, SoundDef)> {
        for set in self.units.sound_set_candidates(unit_id) {
            if let Some(def) = self.sounds.unit_death(set) {
                return Some((set.to_string(), def.clone()));
            }
        }
        None
    }

    /// "Pissed" line from the unit's own set (`{set}Pissed`).
    pub fn unit_pissed(&mut self, unit_id: &str, position: Option<Vec3>) -> Option<VoiceId> {
        let Some((set, def)) = self.unit_pissed_definition(unit_id) else {
            self.record_unit_voice(unit_id, "pissed_missing", "none", "none", None);
            return None;
        };
        let id = self.play_def(&def, position, self.config.ack_priority);
        self.emit("PISSED", &format!("{unit_id} set={set} def={}", def.name), id);
        self.record_unit_voice(unit_id, "pissed", &set, &def.name, id);
        id
    }

    /// Resolve the pissed row through the unit's own object-data sets.
    pub fn unit_pissed_definition(&self, unit_id: &str) -> Option<(String, SoundDef)> {
        for set in self.units.sound_set_candidates(unit_id) {
            if let Some(def) = self.sounds.unit_pissed(set) {
                return Some((set.to_string(), def.clone()));
            }
        }
        None
    }

    /// Retail selection tap: the first taps answer with the unit's own
    /// acknowledgement, every fourth consecutive tap on the same unit plays
    /// its `Pissed` line.
    ///
    /// This is the single entry point the selection code should call; the game
    /// must not also acknowledge from its per-frame selection tracking or the
    /// line plays twice.
    pub fn selection_tap(&mut self, unit_id: &str, position: Option<Vec3>) -> Option<VoiceId> {
        self.stats.selection_taps += 1;
        let key = unit_id.to_ascii_lowercase();
        let count = {
            let entry = self.selection_taps.entry(key).or_insert(0);
            *entry += 1;
            *entry
        };
        if count % 4 == 0 {
            self.unit_pissed(unit_id, position)
        } else {
            self.unit_ack(unit_id, position)
        }
    }

    /// Reset the consecutive-tap counters (selection changed to another unit).
    pub fn selection_reset(&mut self) {
        self.selection_taps.clear();
    }

    /// `Pissed` line for a unit that already has a resolved sound set.
    pub fn unit_pissed_set(
        &mut self,
        set: &str,
        position: Option<Vec3>,
    ) -> Option<(String, VoiceId)> {
        let name = format!("{set}Pissed");
        let def = self.sounds.get(&name)?.clone();
        let voice = self.play_def(&def, position, self.config.ack_priority)?;
        self.emit("PISSED", &format!("set={set} def={name}"), Some(voice));
        Some((name, voice))
    }

    /// Play a named sound-table entry (UI, ambient, spell, ...).
    pub fn play_sound(&mut self, sound_name: &str, position: Option<Vec3>) -> Option<VoiceId> {
        let def = self.sounds.get(sound_name)?.clone();
        let id = self.play_def(&def, position, def.priority);
        // A row culled by its `DistanceCutoff` already emitted the specific
        // `AUDIO_GATED ... cutoff=...` line. Keep that diagnostic instead of
        // overwriting it with a generic `AUDIO_SFX ... voice=none`.
        if id.is_some() || !self.gated {
            self.emit("SFX", sound_name, id);
        }
        id
    }

    /// Retail UI click definitions in preference order.
    ///
    /// `UISounds.slk` ships several click entries. `InterfaceClick` is the
    /// standard one (`Sound\Interface\MouseClick1.wav`); the others cover
    /// installs where that wav is absent (partial extracts, localized or
    /// patch archives), so a missing variant must not silence the UI.
    const UI_CLICK_DEFS: [&'static str; 4] = [
        "InterfaceClick",
        "GlueScreenClick",
        "MenuButtonClick",
        "AutoCastButtonClick",
    ];

    pub fn ui_click(&mut self) -> Option<VoiceId> {
        self.stats.ui_events += 1;
        for name in Self::UI_CLICK_DEFS {
            let Some(def) = self.sounds.get(name).cloned() else {
                continue;
            };
            let params = PlayParams {
                gain: def.volume * self.config.ui_volume,
                category: Category::Ui,
                priority: 4,
                ..Default::default()
            };
            for requested in def.paths() {
                let Some(resolved) = self.provider.resolve(&requested) else {
                    continue;
                };
                let Some(sound) = self.provider.load_sound(&resolved) else {
                    continue;
                };
                self.last_sound_file = Some(resolved);
                let id = self.engine.play(sound, params);
                self.emit("UI", &format!("click {name}"), Some(id));
                return Some(id);
            }
        }
        None
    }

    pub fn ui_error(&mut self) -> Option<VoiceId> {
        self.stats.ui_events += 1;
        let def = self
            .sounds
            .get("InterfaceError")
            .or_else(|| self.sounds.get("ErrorMessage"))?
            .clone();
        let paths = def.paths();
        let params = PlayParams {
            gain: def.volume * self.config.ui_volume,
            category: Category::Ui,
            priority: 6,
            ..Default::default()
        };
        for requested in paths {
            let Some(path) = self.provider.resolve(&requested) else {
                continue;
            };
            if let Some(sound) = self.provider.load_sound(&path) {
                self.last_sound_file = Some(path);
                let id = self.engine.play(sound, params);
                self.emit("UI", "error", Some(id));
                return Some(id);
            }
        }
        None
    }

    /// Play the UI feedback prompt for a `UiAction` result.
    ///
    /// The HUD already decides *what* happened (click accepted, action denied,
    /// waypoint placed, resource warning); this maps that outcome onto the
    /// shipped `UISounds.slk` / `UnitAckSounds.slk` rows and keeps the counters
    /// and the `AUDIO_UI` device line in one place.
    pub fn ui_feedback(&mut self, feedback: UiFeedback, race: Option<Race>) -> Option<VoiceId> {
        self.stats.ui_feedback += 1;
        let voice = match feedback {
            UiFeedback::Click => self.ui_click(),
            UiFeedback::Error => self.ui_error(),
            UiFeedback::Waypoint => self.play_event(AudioEvent::Waypoint, None, None, None),
            UiFeedback::LowGold => self.play_event(AudioEvent::LowGold, None, race, None),
            UiFeedback::LowFood => self.play_event(AudioEvent::LowFood, None, race, None),
        };
        self.emit(
            "UI",
            &format!(
                "feedback={} race={}",
                feedback.id(),
                race.map(Race::id).unwrap_or("-")
            ),
            voice,
        );
        voice
    }

    /// Register a music theme from data (`UI/TriggerData.txt` `musictheme` rows
    /// or an explicit table).
    pub fn add_theme(&mut self, name: impl Into<String>, path: impl Into<String>) {
        self.themes.push((name.into(), path.into()));
    }

    /// Parse `musictheme` rows from `UI/TriggerData.txt`.
    ///
    /// Retail format: `MusicThemeComradeship=musictheme,`Sound\\Music\\...\\Comradeship.mp3`,WESTRING_...`
    /// so the theme key is before `=` and the trigger type follows it.
    pub fn load_themes_from_trigger_data(&mut self, bytes: &[u8]) -> usize {
        let text = String::from_utf8_lossy(bytes);
        let mut added = 0usize;
        for line in text.lines() {
            let line = line.trim();
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.starts_with("//") {
                continue;
            }
            // RoC: `MusicThemeFoo=musictheme,`path``.
            // TFT adds the expansion flag first: `MusicThemeFoo=1,musictheme,`path``.
            let is_theme = value
                .split(',')
                .take(3)
                .any(|field| field.trim().eq_ignore_ascii_case("musictheme"));
            if !is_theme {
                continue;
            }
            let Some(start) = value.find('`') else { continue };
            let Some(end) = value[start + 1..].find('`') else { continue };
            let path = value[start + 1..start + 1 + end].replace("\\\\", "\\");
            let stem = path
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or(&path)
                .trim_end_matches(".mp3")
                .trim_end_matches(".MP3")
                .to_string();
            let key = key.trim().trim_start_matches("MusicTheme").to_string();
            if !key.is_empty() {
                self.add_theme(key, path.clone());
            }
            self.add_theme(stem, path);
            added += 1;
        }
        added
    }

    pub fn themes(&self) -> &[(String, String)] {
        &self.themes
    }

    /// Start (or replace) a looped music track. The file is streamed, not
    /// decoded into memory, so a four-minute track costs a few kilobytes.
    pub fn music(&mut self, theme: &str) -> Option<VoiceId> {
        let Some(requested) = self.resolve_theme(theme) else {
            self.stats.music_missing += 1;
            self.note_unresolved(format!("music:{theme}"));
            self.emit("MUSIC", &format!("theme={theme} path=none"), None);
            return None;
        };
        let path = match self.provider.resolve(&requested) {
            Some(path) => path,
            None => {
                self.stats.music_missing += 1;
                self.note_unresolved(format!("music_path:{requested}"));
                self.emit("MUSIC", &format!("theme={theme} path={requested} resolved=none"), None);
                return None;
            }
        };
        let bytes = match self.provider.load_bytes(&path) {
            Some(bytes) => bytes,
            None => {
                self.stats.music_missing += 1;
                self.note_unresolved(format!("music_bytes:{path}"));
                return None;
            }
        };
        // Validate the stream before touching the running theme: a corrupt
        // track must not fade out the score and leave silence.
        let Some(info) = crate::probe_mp3(&bytes).ok() else {
            self.stats.music_missing += 1;
            self.note_unresolved(format!("music_probe:{path}"));
            self.emit("MUSIC", &format!("theme={theme} path={path} probe=fail"), None);
            return None;
        };
        let duration_ms = info.duration_ms;
        let params = PlayParams {
            gain: self.config.music_volume,
            category: self.config.music_category,
            looping: true,
            fade_in_ms: self.config.crossfade_ms,
            ..Default::default()
        };
        match self.engine.play_mp3(bytes, params) {
            Ok(id) => {
                // Cross-fade: the outgoing theme ramps down while the incoming
                // one ramps up, so a race/state change never clicks or cuts the
                // score. The new voice is queued first, so a failed start above
                // leaves the running theme untouched.
                if let Some(previous) = self.music_voice.take() {
                    if self.config.crossfade_ms > 0 {
                        self.engine.fade_out(previous, self.config.crossfade_ms);
                        self.stats.music_crossfades += 1;
                    } else {
                        self.engine.stop(previous);
                    }
                }
                self.last_sound_file = Some(path.clone());
                self.music_started_ms = self.clock_ms;
                self.music_track_ms = duration_ms;
                self.music_voice = Some(id);
                self.stats.music_started += 1;
                self.emit("MUSIC", &format!("theme={theme} path={path}"), Some(id));
                Some(id)
            }
            Err(err) => {
                let _ = err;
                self.stats.music_missing += 1;
                self.note_unresolved(format!("music_decode:{path}"));
                None
            }
        }
    }

    /// Start the race's hash-ordered theme playlist (the in-game score).
    ///
    /// Retail plays `Human1..3` (etc.) and cycles through the set; the order is
    /// derived from a stable hash of `(race, track)` so replays and tests hear
    /// the same rotation. The first track that resolves starts with a cross-fade;
    /// tracks whose MP3 is absent are skipped, and the playlist only becomes
    /// current when a track actually started.
    pub fn start_race_playlist(&mut self, race: Race) -> Option<VoiceId> {
        let mut playlist = MusicPlaylist::build(race, MusicState::Playing);
        let mut started = None;
        for index in 0..playlist.len() {
            playlist.set_cursor(index);
            if let Some(track) = playlist.current()
                && let Some(id) = self.music(track)
            {
                started = Some(id);
                break;
            }
        }
        self.playlist = started.map(|_| playlist);
        started
    }

    /// Advance the current race playlist to its next theme (cross-faded).
    pub fn playlist_advance(&mut self) -> Option<VoiceId> {
        let mut playlist = self.playlist.clone()?;
        for _ in 0..playlist.len() {
            let track = playlist.next()?;
            if let Some(id) = self.music(track) {
                self.playlist = Some(playlist);
                return Some(id);
            }
        }
        None
    }

    /// Whether a music voice is on the mixer (rotation never leaves a gap).
    pub fn music_active(&self) -> bool {
        self.music_voice.is_some()
    }

    /// Whether the region ambience loop is on the mixer.
    pub fn ambient_active(&self) -> bool {
        self.ambient_voice.is_some()
    }

    /// Current race playlist (diagnostics / QA).
    pub fn playlist(&self) -> Option<&MusicPlaylist> {
        self.playlist.as_ref()
    }

    /// Stop the rotation without stopping the current theme.
    ///
    /// An explicit theme selection (victory/defeat stinger, menu music) owns the
    /// score from that point; the match playlist must not advance over it when
    /// the previous track length elapses.
    pub(crate) fn clear_playlist(&mut self) {
        self.playlist = None;
    }

    /// Rotate the race playlist when the current theme has played out.
    ///
    /// Called once per frame after [`GameAudio::set_clock_ms`]. The theme length
    /// comes from the streamed MP3 header (`probe_mp3`), so a track is replaced
    /// when it ends rather than on a guessed timer; a theme with an unknown
    /// length is left looping (never rotates early).
    pub fn tick_music(&mut self) -> Option<VoiceId> {
        if self.playlist.is_none() || self.music_track_ms == 0 {
            return None;
        }
        let elapsed = self.clock_ms.saturating_sub(self.music_started_ms);
        if elapsed < self.music_track_ms {
            return None;
        }
        self.playlist_advance()
    }

    fn resolve_theme(&self, theme: &str) -> Option<String> {        let lower = theme.to_ascii_lowercase();
        for (name, path) in &self.themes {
            if name.to_ascii_lowercase() == lower {
                return Some(path.replace('/', "\\"));
            }
        }
        // Fall back to the canonical music directory.
        let candidate = format!("Sound\\Music\\mp3Music\\{theme}.mp3");
        if self.provider.exists(&candidate) {
            Some(candidate)
        } else {
            None
        }
    }

    /// Load `MIDISounds.slk` + `WorldEditData.txt` through the provider (once).
    fn ensure_ambient_table(&mut self) -> bool {
        if self.ambient_table.is_some() {
            return true;
        }
        let Some(path) = self.provider.resolve(MIDI_SOUNDS_TABLE) else {
            self.note_unresolved(format!("ambient_table:{MIDI_SOUNDS_TABLE}"));
            return false;
        };
        let Some(bytes) = self.provider.load_bytes(&path) else {
            self.note_unresolved(format!("ambient_table_bytes:{MIDI_SOUNDS_TABLE}"));
            return false;
        };
        let mut table = match AmbientTable::parse_midi_sounds(&bytes) {
            Ok(table) => table,
            Err(_) => {
                self.note_unresolved(format!("ambient_table_parse:{MIDI_SOUNDS_TABLE}"));
                return false;
            }
        };
        if let Some(path) = self.provider.resolve(WORLD_EDIT_DATA)
            && let Some(bytes) = self.provider.load_bytes(&path)
        {
            table.parse_world_edit_data(&bytes);
        } else {
            self.note_unresolved(format!("ambient_tilesets:{WORLD_EDIT_DATA}"));
        }
        self.ambient_table = Some(table);
        true
    }

    /// Play the retail day/night ambience for a W3E tileset letter.
    ///
    /// `tileset` is `Terrain::tileset` (`'L'` = LordaeronSummer for Justice);
    /// the MIDI score is rendered once through the shipped DLS bank and cached,
    /// then looped on the Ambient bus. Unresolved themes are recorded in
    /// [`GameAudio::unresolved`] and counted in `ambient_missing`.
    pub fn ambient_for_tileset(&mut self, tileset: char, night: bool) -> Option<VoiceId> {
        if !self.ensure_ambient_table() {
            self.stats.ambient_missing += 1;
            return None;
        }
        let Some(theme) = self
            .ambient_table
            .as_ref()
            .and_then(|table| table.theme_for_tileset(tileset, night))
        else {
            self.note_unresolved(format!(
                "ambient_tileset:{} night={night}",
                tileset.to_ascii_uppercase()
            ));
            self.stats.ambient_missing += 1;
            return None;
        };
        let name = theme.name.clone();
        let midi_path = theme.midi_path();
        let dls_path = theme.dls_path();
        let volume = theme.volume;
        let priority = theme.priority;
        if let Some(cached) = self.ambient_cache.get(&name) {
            let id = self.start_ambient_voice(&name, cached, volume, priority);
            self.emit(
                "AMBIENT",
                &format!(
                    "tileset={} night={night} theme={name} midi={midi_path} dls={dls_path} cached=1",
                    tileset.to_ascii_uppercase()
                ),
                id,
            );
            return id;
        }
        let Some(midi_file) = self.provider.resolve(&midi_path) else {
            self.note_unresolved(format!("ambient_midi:{midi_path}"));
            self.stats.ambient_missing += 1;
            return None;
        };
        let Some(dls_file) = self.provider.resolve(&dls_path) else {
            self.note_unresolved(format!("ambient_dls:{dls_path}"));
            self.stats.ambient_missing += 1;
            return None;
        };
        let (Some(midi_bytes), Some(dls_bytes)) = (
            self.provider.load_bytes(&midi_file),
            self.provider.load_bytes(&dls_file),
        ) else {
            self.note_unresolved(format!("ambient_bytes:{name}"));
            self.stats.ambient_missing += 1;
            return None;
        };
        let Ok(song) = MidiSong::parse(&midi_bytes) else {
            self.note_unresolved(format!("ambient_midi_parse:{name}"));
            self.stats.ambient_missing += 1;
            return None;
        };
        let Ok(bank) = DlsBank::parse(&dls_bytes) else {
            self.note_unresolved(format!("ambient_dls_parse:{name}"));
            self.stats.ambient_missing += 1;
            return None;
        };
        let Some(render) = render_ambient(&song, &bank, MAX_AMBIENT_SECONDS) else {
            self.note_unresolved(format!("ambient_render:{name}"));
            self.stats.ambient_missing += 1;
            return None;
        };
        if render.peak < 1e-4 {
            self.note_unresolved(format!("ambient_silent:{name}"));
            self.stats.ambient_missing += 1;
            return None;
        }
        let seconds = render.seconds;
        let notes = render.notes;
        let peak = render.peak;
        let sound = Arc::new(render.sound);
        self.ambient_cache.insert(&name, sound.clone());
        let id = self.start_ambient_voice(&name, sound, volume, priority);
        self.emit(
            "AMBIENT",
            &format!(
                "tileset={} night={night} theme={name} midi={midi_path} dls={dls_path} seconds={seconds:.1} samples={notes} peak={peak:.3}",
                tileset.to_ascii_uppercase()
            ),
            id,
        );
        id
    }

    /// Tileset ambience keyed by the player's current region.
    ///
    /// The loop starts once for a `(region, tileset, day/night)` key, keeps
    /// playing while the player stays in that region (no restart, no double
    /// voice), and is stopped/replaced when the key changes. Ambient stays
    /// global (no distance gate).
    pub fn ambient_for_region(
        &mut self,
        region: &str,
        tileset: char,
        night: bool,
    ) -> Option<VoiceId> {
        let key = (region.to_string(), tileset.to_ascii_uppercase(), night);
        if self.ambient_key.as_ref() == Some(&key) && self.ambient_voice.is_some() {
            return self.ambient_voice;
        }
        // The new loop cross-fades from the running one (`start_ambient_voice`
        // owns the outgoing voice); if the new theme cannot be resolved the old
        // loop is stopped, so another region's ambience never leaks.
        let voice = self.ambient_for_tileset(tileset, night);
        if voice.is_none() && self.ambient_voice.is_some() {
            self.stop_ambient();
        }
        self.ambient_key = if voice.is_some() { Some(key) } else { None };
        voice
    }

    /// Player left the ambient region: stop the loop until the next region.
    pub fn leave_ambient_region(&mut self) {
        self.stop_ambient();
    }

    /// Region key of the running ambience loop (diagnostics).
    pub fn ambient_region(&self) -> Option<&str> {
        self.ambient_key.as_ref().map(|(region, _, _)| region.as_str())
    }

    fn start_ambient_voice(
        &mut self,
        name: &str,
        sound: Arc<Sound>,
        volume: f32,
        priority: u32,
    ) -> Option<VoiceId> {
        let bytes = sound.memory_bytes();
        if let Some(previous) = self.ambient_voice.take() {
            // Region change: the old loop ramps down under the new one.
            if self.config.ambient_crossfade_ms > 0 {
                self.engine.fade_out(previous, self.config.ambient_crossfade_ms);
                self.stats.ambient_crossfades += 1;
            } else {
                self.engine.stop(previous);
            }
        }
        let params = PlayParams {
            gain: (volume * self.config.ambient_volume).clamp(0.0, 1.0),
            position: None,
            looping: true,
            category: Category::Ambient,
            priority,
            fade_in_ms: self.config.ambient_crossfade_ms,
            ..Default::default()
        };
        let id = self.engine.play(sound, params);
        self.ambient_voice = Some(id);
        self.ambient_voice_bytes = bytes;
        self.stats.ambient_started += 1;
        let _ = name;
        Some(id)
    }

    /// Stop the tileset ambience loop (music and voices keep playing).
    pub fn stop_ambient(&mut self) {
        self.ambient_key = None;
        if let Some(id) = self.ambient_voice.take() {
            self.engine.fade_out(id, self.config.ambient_crossfade_ms.max(200));
        }
        self.ambient_voice_bytes = 0;
    }

    /// Theme names loaded from `MIDISounds.slk` (loads the table on first call).
    pub fn ambient_theme_names(&mut self) -> Vec<String> {
        if !self.ensure_ambient_table() {
            return Vec::new();
        }
        self.ambient_table
            .as_ref()
            .map(|table| table.themes().map(|theme| theme.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Tileset letter -> theme name for the current day/night state.
    pub fn ambient_theme_for_tileset(&mut self, tileset: char, night: bool) -> Option<String> {
        if !self.ensure_ambient_table() {
            return None;
        }
        self.ambient_table
            .as_ref()
            .and_then(|table| table.theme_name_for_tileset(tileset, night))
            .map(str::to_string)
    }

    pub fn stop_music(&mut self) {
        self.clear_playlist();
        if let Some(id) = self.music_voice.take() {
            self.engine.stop(id);
        }
    }

    pub fn stop_all(&mut self) {
        self.engine.stop_all();
        self.music_voice = None;
        self.ambient_voice = None;
        self.ambient_voice_bytes = 0;
    }

    pub fn set_listener(&mut self, position: Vec3, yaw: f32) {
        self.listener = position;
        self.engine.set_listener(position, yaw);
    }

    /// Last listener position passed to [`GameAudio::set_listener`].
    pub fn listener(&self) -> Vec3 {
        self.listener
    }

    /// Optional diagnostic cap on unit voices (`0` = off, the default).
    ///
    /// Audibility is governed by each row's `DistanceCutoff` (retail rule);
    /// this cap is only for QA experiments and must not replace it.
    pub fn set_audible_radius(&mut self, units: f32) {
        self.audible_radius = units.max(0.0);
    }

    pub fn audible_radius(&self) -> f32 {
        self.audible_radius
    }

    /// Whether `position` is inside the optional diagnostic cap (see
    /// [`GameAudio::set_audible_radius`]). With the default radius 0 this is
    /// always true: per-row `DistanceCutoff` is the retail rule.
    pub fn is_within_earshot(&self, position: Vec3) -> bool {
        if self.audible_radius <= 0.0 {
            return true;
        }
        let dx = position.x - self.listener.x;
        let dz = position.z - self.listener.z;
        dx * dx + dz * dz <= self.audible_radius * self.audible_radius
    }


    /// Install (or clear) the terrain height sampler used for voice occlusion.
    pub fn set_occluder(&mut self, sampler: Option<crate::engine::HeightSampler>) {
        self.engine.set_occluder(sampler);
    }

    /// Duck a category bus (ported from kira's track-volume tween concept).
    pub fn duck(&mut self, category: Category, amount_db: f32, hold_seconds: f32) {
        self.engine.duck(category, amount_db, hold_seconds);
    }

    /// Render one block when the engine is headless (tests, offline mixing).
    pub fn render(&mut self, out: &mut [f32]) -> usize {
        self.engine.render(out)
    }

    pub fn shutdown(self) {
        self.engine.shutdown();
    }
}

/// Play an MP3 that is already in memory (e.g. a jingle from a map archive).
pub fn play_music_bytes(
    engine: &mut AudioEngine,
    bytes: Arc<[u8]>,
    looping: bool,
    gain: f32,
) -> Result<VoiceId, AudioError> {
    engine.play_mp3(
        bytes,
        PlayParams {
            gain,
            category: Category::Music,
            looping,
            ..Default::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AudioConfig;

    /// Minimal `UISounds.slk` carrying the two click definitions the fallback
    /// chain relies on (names/files are the retail ones).
    const UI_SLK: &str = r#"ID;PWXL;N;E
C;Y1;X1;K"Name"
C;X2;K"FileNames"
C;X3;K"DirectoryBase"
C;Y2;X1;K"InterfaceClick"
C;X2;K"MouseClick1.wav"
C;X3;K"Sound\Interface\"
C;Y3;X1;K"AutoCastButtonClick"
C;X2;K"AutoCastButtonClick1.wav"
C;X3;K"Sound\Interface\"
"#;

    struct FakeProvider {
        available: Vec<String>,
        loaded: std::cell::RefCell<Vec<String>>,
    }

    impl FakeProvider {
        fn new(available: &[&str]) -> Self {
            Self {
                available: available.iter().map(|path| path.to_ascii_lowercase()).collect(),
                loaded: std::cell::RefCell::new(Vec::new()),
            }
        }
    }

    impl SoundProvider for FakeProvider {
        fn load_sound(&mut self, path: &str) -> Option<Arc<Sound>> {
            if !self.exists(path) {
                return None;
            }
            self.loaded.borrow_mut().push(path.to_string());
            Some(Arc::new(Sound {
                sample_rate: 22_050,
                channels: 1,
                samples: vec![0.1; 64],
            }))
        }

        fn load_bytes(&mut self, _path: &str) -> Option<Arc<[u8]>> {
            None
        }

        fn exists(&self, path: &str) -> bool {
            let key = path.to_ascii_lowercase();
            self.available.iter().any(|available| *available == key)
        }
    }

    fn game(available: &[&str]) -> GameAudio<FakeProvider> {
        let mut sounds = SoundInfoDb::new();
        let added = sounds
            .add_table("UISounds", UI_SLK.as_bytes())
            .expect("parse ui sounds");
        assert_eq!(added, 2, "both click definitions ingested");
        GameAudio::new(
            AudioEngine::headless(AudioConfig::default()),
            sounds,
            UnitNameMap::default(),
            FakeProvider::new(available),
        )
    }

    #[test]
    fn ui_click_prefers_the_standard_variant() {
        let mut audio = game(&["Sound\\Interface\\MouseClick1.wav"]);
        assert!(audio.ui_click().is_some());
        assert_eq!(
            audio.provider.loaded.borrow().as_slice(),
            ["Sound\\Interface\\MouseClick1.wav"],
            "InterfaceClick is tried first"
        );
    }

    /// A Voice-category line played through the gameplay path must duck the
    /// music bus (kira track-volume mechanism) and the device stats must count
    /// the ducked blocks.
    #[test]
    fn gameplay_voice_line_ducks_the_music_bus() {
        let mut audio = game(&["Sound\\Interface\\MouseClick1.wav"]);
        let music = Arc::new(Sound {
            sample_rate: 48_000,
            channels: 1,
            samples: vec![0.5f32; 48_000],
        });
        audio.engine().play(
            music,
            PlayParams {
                category: Category::Music,
                looping: true,
                ..Default::default()
            },
        );
        let mut block = vec![0.0f32; 2048];
        let rms = |block: &[f32]| -> f32 {
            (block.iter().map(|s| s * s).sum::<f32>() / block.len().max(1) as f32).sqrt()
        };
        for _ in 0..3 {
            audio.render(&mut block);
        }
        let before = rms(&block);
        assert!(before > 0.05, "music must be audible: {before}");
        // `play_sound` goes through `play_def` (ack_category = Voice), the same
        // path the game's unit acknowledgements use.
        assert!(audio.play_sound("InterfaceClick", None).is_some());
        for _ in 0..3 {
            audio.render(&mut block);
        }
        let ducked = rms(&block);
        assert!(
            ducked < before * 0.6,
            "voice line must duck music: before={before} ducked={ducked}"
        );
        assert!(
            audio.audio_stats().ducked_blocks > 0,
            "ducked_blocks counter must move"
        );
        // 40 x 1024 frames = 0.85 s: hold (1 s) has not fully expired; the
        // release is 250 ms after that. Render a bit more and require recovery.
        for _ in 0..80 {
            audio.render(&mut block);
        }
        let after = rms(&block);
        assert!(
            after > before * 0.8,
            "music must recover after the hold: before={before} after={after}"
        );
    }

    #[test]
    fn ui_click_falls_back_when_the_standard_variant_is_missing() {
        let mut audio = game(&["Sound\\Interface\\AutoCastButtonClick1.wav"]);
        assert!(audio.ui_click().is_some(), "partial installs still click");
        assert_eq!(
            audio.provider.loaded.borrow().as_slice(),
            ["Sound\\Interface\\AutoCastButtonClick1.wav"],
            "the shipped AutoCastButtonClick wav is used"
        );
    }

    #[test]
    fn ui_click_is_silent_when_no_variant_exists() {
        let mut audio = game(&[]);
        assert!(audio.ui_click().is_none());
    }

    /// `memory.audio`: the decoded-PCM LRU keeps the process under 64 MB.
    ///
    /// One rendered ambient theme is ~15.9 MB, and War3 ships 42 of them. Before
    /// the cache had a byte budget, walking the map (region changes) rendered
    /// one theme per tileset/night and kept every one: 670 MB of samples for a
    /// score that plays one loop at a time. The cache evicts by bytes, so the
    /// resident set stays under the cap no matter how long the session runs.
    #[test]
    fn ambient_pcm_cache_stays_under_its_byte_budget() {
        let mut cache = AmbientPcmCache::new(64 * 1024 * 1024);
        // 15.9 MB per theme, ten themes: 159 MB pushed through a 64 MB cache.
        let theme_bytes = 15 * 1024 * 1024 + 900 * 1024;
        for index in 0..10 {
            let sound = Arc::new(Sound {
                sample_rate: 44_100,
                channels: 2,
                samples: vec![0.0; theme_bytes / std::mem::size_of::<f32>()],
            });
            cache.insert(&format!("theme{index}"), sound);
            assert!(
                cache.bytes() <= cache.budget,
                "after theme {index}: {} B > {} B",
                cache.bytes(),
                cache.budget
            );
        }
        assert!(cache.evictions() > 0, "the cache must evict to stay in budget");
        assert_eq!(cache.len(), 4, "64 MB holds four 15.9 MB themes");
        // The newest theme survived; the oldest was evicted first (LRU).
        assert!(cache.get("theme9").is_some(), "newest theme kept");
        assert!(cache.get("theme0").is_none(), "oldest theme evicted");
    }

    /// A theme larger than the whole budget is still kept (one loop must play).
    #[test]
    fn ambient_pcm_cache_keeps_a_single_oversized_theme() {
        let mut cache = AmbientPcmCache::new(1024);
        let sound = Arc::new(Sound {
            sample_rate: 44_100,
            channels: 2,
            samples: vec![0.0; 4096],
        });
        cache.insert("huge", sound);
        assert_eq!(cache.len(), 1, "an oversized theme is not dropped");
        assert!(cache.get("huge").is_some());
    }
}
