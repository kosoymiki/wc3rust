//! `wc3-audio` provider implementation: lets the gameplay hooks read sounds and
//! sound tables straight out of the asset database.
//!
//! `wc3-audio` stays independent of `wc3-assetdb` (it only knows the
//! `SoundProvider` trait); this module is the wiring point.

use std::sync::Arc;

use wc3_audio::soundinfo::{SoundInfoDb, UnitNameMap};
use wc3_audio::{Sound, SoundProvider};

use crate::AssetDb;

/// Sound tables shipped in `War3.mpq`, in load order.
pub const SOUND_TABLES: &[&str] = &[
    "UI\\SoundInfo\\UnitAckSounds.slk",
    "UI\\SoundInfo\\UnitCombatSounds.slk",
    "UI\\SoundInfo\\UISounds.slk",
    "UI\\SoundInfo\\AnimSounds.slk",
    "UI\\SoundInfo\\AbilitySounds.slk",
    "UI\\SoundInfo\\AmbienceSounds.slk",
    "UI\\SoundInfo\\EnvironmentSounds.slk",
    "UI\\SoundInfo\\MIDISounds.slk",
];

/// Object-data table that names each unit's weapon sound family.
pub const WEAPON_TABLE: &str = "Units\\UnitWeapons.slk";

impl SoundProvider for AssetDb {
    fn load_sound(&mut self, path: &str) -> Option<Arc<Sound>> {
        let canonical = self.sound_path(path)?;
        self.sound_shared(&canonical).ok()
    }

    fn load_bytes(&mut self, path: &str) -> Option<Arc<[u8]>> {
        let canonical = self.sound_path(path)?;
        self.sound_stream_bytes(&canonical).ok()
    }

    fn exists(&self, path: &str) -> bool {
        self.resolve_asset(path).is_resolved()
    }

    /// Retail sound tables name files that ship under a different directory
    /// (`Buildings\\Orc\\PigFarm\\PigFarmWhat1.wav` ->
    /// `buildings\\Other\\PigFarm\\PigFarmWhat1.wav`). Resolve through the
    /// active layers and their listfiles; never synthesise a name.
    fn resolve(&self, path: &str) -> Option<String> {
        self.sound_path(path)
    }
}

/// Load every retail sound table plus the unit-id → name map.
///
/// Returns the database, the unit map, and the number of tables that were
/// present. Missing tables are not an error: a map archive without the retail
/// `UI/SoundInfo` set simply yields fewer definitions.
pub fn load_sound_tables(db: &mut AssetDb) -> (SoundInfoDb, UnitNameMap, usize) {
    let mut sounds = SoundInfoDb::new();
    let mut tables = 0usize;
    for name in SOUND_TABLES {
        let Ok(bytes) = db.read_uncached(name) else {
            continue;
        };
        let table = name.rsplit('\\').next().unwrap_or(name).trim_end_matches(".slk");
        if sounds.add_table(table, &bytes).is_ok() {
            tables += 1;
        }
    }
    let units = db
        .read_uncached("Units\\unitUI.slk")
        .ok()
        .and_then(|bytes| UnitNameMap::parse(&bytes).ok())
        .unwrap_or_default();
    (sounds, units, tables)
}

/// Every path referenced by a sound table, deduplicated, for coverage checks.
pub fn sound_table_paths(sounds: &SoundInfoDb) -> Vec<String> {
    let mut paths = Vec::new();
    for def in sounds.iter() {
        paths.extend(def.paths());
    }
    paths.sort();
    paths.dedup();
    paths
}

// ---------------------------------------------------------------------------
// Shared database + game wiring
// ---------------------------------------------------------------------------

use std::sync::Mutex;

use wc3_audio::engine::{open_default_backend, AudioConfig, AudioEngine};
use wc3_audio::game_audio::{GameAudio, GameAudioStats};
pub use wc3_audio::engine::VoiceId;
use wc3_formats::blp::BlpImage;
use wc3_formats::mdx::MdxModel;

use crate::{AssetError, AssetStats, ModelAsset, TextureAsset};

/// One [`AssetDb`] shared by the renderer and the gameplay audio hooks.
///
/// This is the wiring seam for the game: a single cache (bytes/textures/models/
/// sounds) is reached through `Arc<Mutex<..>>`, so audio playback and asset
/// loading never duplicate decoded data. The facade methods mirror the ones the
/// game already calls (`read`, `texture`, `model`, `resolve_path`, ...).
#[derive(Clone)]
pub struct SharedAssetDb {
    inner: Arc<Mutex<AssetDb>>,
}

impl std::fmt::Debug for SharedAssetDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedAssetDb").finish_non_exhaustive()
    }
}

impl SharedAssetDb {
    pub fn new(db: AssetDb) -> Self {
        Self {
            inner: Arc::new(Mutex::new(db)),
        }
    }

    pub fn from_arc(inner: Arc<Mutex<AssetDb>>) -> Self {
        Self { inner }
    }

    pub fn arc(&self) -> Arc<Mutex<AssetDb>> {
        self.inner.clone()
    }

    /// Run a closure with the locked database.
    pub fn with_db<R>(&self, f: impl FnOnce(&mut AssetDb) -> R) -> R {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut guard)
    }

    pub fn read(&self, name: &str) -> Result<Arc<Vec<u8>>, AssetError> {
        self.with_db(|db| db.read(name))
    }

    pub fn read_uncached(&self, name: &str) -> Result<Vec<u8>, AssetError> {
        self.with_db(|db| db.read_uncached(name))
    }

    pub fn texture(&self, name: &str) -> Result<BlpImage, AssetError> {
        self.with_db(|db| db.texture(name))
    }

    pub fn texture_shared(&self, name: &str) -> Result<Arc<TextureAsset>, AssetError> {
        self.with_db(|db| db.texture_shared(name))
    }

    pub fn model(&self, name: &str) -> Result<MdxModel, AssetError> {
        self.with_db(|db| db.model(name))
    }

    pub fn model_shared(&self, name: &str) -> Result<Arc<ModelAsset>, AssetError> {
        self.with_db(|db| db.model_shared(name))
    }

    pub fn resolve_path(&self, name: &str) -> Option<String> {
        self.with_db(|db| db.resolve_path(name))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.with_db(|db| db.contains(name))
    }

    pub fn file_count(&self) -> usize {
        self.with_db(|db| db.file_count())
    }

    /// Owned copy: the lock cannot be held across the caller's lifetime.
    pub fn listfile(&self) -> Vec<String> {
        self.with_db(|db| db.listfile().to_vec())
    }

    pub fn stats(&self) -> AssetStats {
        self.with_db(|db| db.stats())
    }

    /// Drop the decoded caches once the game owns runtime copies.
    pub fn clear_decoded_caches(&self) -> usize {
        self.with_db(|db| db.clear_decoded_caches())
    }

    pub fn source_label(&self) -> String {
        self.with_db(|db| db.source_label().to_string())
    }

    pub fn total_bytes(&self) -> usize {
        self.with_db(|db| db.total_bytes())
    }
}

impl SoundProvider for SharedAssetDb {
    fn load_sound(&mut self, path: &str) -> Option<Arc<Sound>> {
        self.with_db(|db| {
            let canonical = db.sound_path(path)?;
            db.sound_shared(&canonical).ok()
        })
    }

    /// Drop the decoded asset caches (textures/models/raw bytes) once the game
    /// layer owns the runtime copies. Called by `Assets::release_cpu_textures`.
    fn release_decoded_caches(&mut self) -> usize {
        self.clear_decoded_caches()
    }

    fn load_bytes(&mut self, path: &str) -> Option<Arc<[u8]>> {
        self.with_db(|db| {
            let canonical = db.sound_path(path)?;
            db.sound_stream_bytes(&canonical).ok()
        })
    }

    fn exists(&self, path: &str) -> bool {
        self.with_db(|db| db.resolve_asset(path).is_resolved())
    }

    fn resolve(&self, path: &str) -> Option<String> {
        self.with_db(|db| db.sound_path(path))
    }
}

/// `GameAudio` parameterised by the shared database.
pub type WiredGameAudio = GameAudio<SharedAssetDb>;

/// What the audio wiring actually attached to, for startup logs and QA.
#[derive(Debug, Clone)]
pub struct GameAudioWiring {
    pub device: &'static str,
    pub source: String,
    pub layers: Vec<String>,
    pub tables: usize,
    pub definitions: usize,
    pub units: usize,
    pub themes: usize,
}

impl GameAudioWiring {
    pub fn summary(&self) -> String {
        format!(
            "audio device={} source={} layers={:?} tables={} defs={} units={} themes={}",
            self.device,
            self.source,
            self.layers,
            self.tables,
            self.definitions,
            self.units,
            self.themes
        )
    }
}

/// Load the retail sound tables through a shared database.
pub fn load_sound_tables_shared(db: &SharedAssetDb) -> (SoundInfoDb, UnitNameMap, usize) {
    let mut sounds = SoundInfoDb::new();
    let mut tables = 0usize;
    for name in SOUND_TABLES {
        let Ok(bytes) = db.read_uncached(name) else {
            continue;
        };
        let table = name.rsplit('\\').next().unwrap_or(name).trim_end_matches(".slk");
        if sounds.add_table(table, &bytes).is_ok() {
            tables += 1;
        }
    }
    let units = db
        .read_uncached("Units\\unitUI.slk")
        .ok()
        .and_then(|bytes| UnitNameMap::parse(&bytes).ok())
        .unwrap_or_default();
    (sounds, units, tables)
}

/// Build `GameAudio` on an existing engine (tests, benches, custom backends).
///
/// Sound tables and music themes are read from the same shared database, so the
/// hooks and the renderer never hold two copies of the archive data. The unit
/// weapon families (`Units/UnitWeapons.slk`) are attached here because the
/// weapon-impact rows (`UnitCombatSounds`) are only playable with them.
pub fn build_game_audio(
    shared: SharedAssetDb,
    engine: AudioEngine,
    device: &'static str,
) -> (WiredGameAudio, GameAudioWiring) {
    let source = shared.source_label();
    let (sounds, units, tables) = load_sound_tables_shared(&shared);
    let definitions = sounds.len();
    let units_count = units.len();
    let mut audio = GameAudio::new(engine, sounds, units, shared.clone());
    if let Ok(bytes) = shared.read_uncached(WEAPON_TABLE) {
        let rows = audio.load_weapon_table(&bytes);
        if rows == 0 {
            audio.note_wiring_gap(WEAPON_TABLE);
        }
    } else {
        audio.note_wiring_gap(WEAPON_TABLE);
    }
    let themes = match shared.read_uncached("UI\\TriggerData.txt") {
        Ok(bytes) => audio.load_themes_from_trigger_data(&bytes),
        Err(_) => 0,
    };
    let wiring = GameAudioWiring {
        device,
        source,
        layers: shared.with_db(|db| db.layer_labels()),
        tables,
        definitions,
        units: units_count,
        themes,
    };
    (audio, wiring)
}

/// Start gameplay audio for the game: opens the platform backend (SDL3, with a
/// documented null fallback), loads the retail tables from the same shared
/// database the renderer uses, and returns the hooks plus a wiring summary.
pub fn start_game_audio(shared: SharedAssetDb) -> (WiredGameAudio, GameAudioWiring) {
    let config = AudioConfig {
        sample_rate: 48_000,
        channels: 2,
        block_frames: 512,
        max_voices: 64,
        channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES,
        master_gain: 0.8,
    };
    let (backend, device) = open_default_backend(&config);
    let engine = AudioEngine::start(config, backend);
    build_game_audio(shared, engine, device)
}

/// Stats of the hooks, for QA and the startup log.
pub fn game_audio_stats(audio: &WiredGameAudio) -> GameAudioStats {
    audio.stats()
}
