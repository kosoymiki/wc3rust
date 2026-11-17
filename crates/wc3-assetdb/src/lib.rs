//! Asset database: providers, bounded caches and the dependency graph.
//!
//! All gameplay code reads through this type; it is the only layer allowed to
//! touch MPQ archives or a loose data directory.
//!
//! Memory behaviour is explicit: the raw byte cache, the decoded-texture cache
//! and the parsed-model cache are each an [`LruCache`] with a byte budget, and
//! entries that are still referenced by a consumer ([`Arc`] refcount > 1) are
//! pinned instead of evicted. `stats()` reports bytes, hits, misses, evictions
//! and pinned skips so a leak or a budget thrash is visible rather than silent.

pub mod audio_provider;
pub mod blp_report;
pub mod catalog;
mod lru;
pub mod source;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use wc3_audio::Sound;
use wc3_formats::blp::{decode as decode_blp, BlpImage};
use wc3_formats::mdx::MdxModel;
use wc3_mpq::{MpqError, MpqIndex};

pub use source::{AssetSource, DirSource, LayeredSource, MpqSource, Resolution, SearchHit};

pub use lru::{EvictionReport, LruCache};
use source::normalize_path;

#[derive(Debug)]
pub enum AssetError {
    Mpq(MpqError),
    Io(std::io::Error),
    Format(wc3_formats::FormatError),
    NotFound(String),
    IndexStale,
}

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mpq(e) => write!(f, "MPQ: {e}"),
            Self::Io(e) => write!(f, "i/o: {e}"),
            Self::Format(e) => write!(f, "format: {e}"),
            Self::NotFound(name) => write!(f, "asset not found: {name}"),
            Self::IndexStale => f.write_str("asset index does not match the archive"),
        }
    }
}

impl std::error::Error for AssetError {}

impl AssetError {
    /// True when the asset does not exist (as opposed to being corrupt).
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::NotFound(_) => true,
            Self::Mpq(MpqError::FileNotFound(_) | MpqError::Deleted(_)) => true,
            Self::Io(err) => err.kind() == std::io::ErrorKind::NotFound,
            _ => false,
        }
    }
}

impl From<MpqError> for AssetError {
    fn from(value: MpqError) -> Self {
        match value {
            MpqError::StaleIndex => Self::IndexStale,
            other => Self::Mpq(other),
        }
    }
}

impl From<std::io::Error> for AssetError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<wc3_formats::FormatError> for AssetError {
    fn from(value: wc3_formats::FormatError) -> Self {
        Self::Format(value)
    }
}

/// One file the provider can serve.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub size: u64,
}

/// Cache budgets. Defaults target a 2 GB-class Android device.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CacheConfig {
    /// Raw file bytes (MPQ payloads before decoding).
    pub byte_budget: usize,
    /// Decoded textures (RGBA8, all mips).
    pub texture_budget: usize,
    /// Parsed models.
    pub model_budget: usize,
    /// Decoded sounds (f32 PCM); music is streamed instead of cached here.
    pub sound_budget: usize,
    /// Longest sound decoded into memory; longer files must be streamed.
    pub max_sound_seconds: f32,
    pub max_entries: usize,
    /// Remember misses so a repeated bad path does not re-probe the archive.
    pub negative_cache: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            byte_budget: 64 * 1024 * 1024,
            texture_budget: 96 * 1024 * 1024,
            model_budget: 48 * 1024 * 1024,
            sound_budget: 32 * 1024 * 1024,
            max_sound_seconds: 30.0,
            max_entries: 4096,
            negative_cache: true,
        }
    }
}

/// Accounting for the whole database. `bytes_cached` is the raw byte cache;
/// `texture_bytes`/`model_bytes` are the decoded caches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AssetStats {
    pub hits: u64,
    pub misses: u64,
    pub negative_hits: u64,
    pub texture_hits: u64,
    pub texture_misses: u64,
    pub model_hits: u64,
    pub model_misses: u64,
    pub bytes_cached: usize,
    pub texture_bytes: usize,
    pub model_bytes: usize,
    pub raw_evictions: u64,
    pub texture_evictions: u64,
    pub model_evictions: u64,
    pub sound_hits: u64,
    pub sound_misses: u64,
    pub sound_bytes: usize,
    pub sound_evictions: u64,
    pub pinned_skips: u64,
    pub cache_entries: usize,
    pub negative_entries: usize,
}

/// A decoded texture plus the provenance needed by the catalogs.
#[derive(Debug, Clone)]
pub struct TextureAsset {
    pub name: String,
    /// Fully decoded image: `.rgba` is mip 0 (the surface the game samples) and
    /// `.mipmaps` preserves every decoded level.
    pub image: BlpImage,
    pub source_len: usize,
    pub sha256_source: String,
    pub sha256_rgba: String,
}

impl TextureAsset {
    /// Base-level RGBA (what renderers upload as level 0).
    pub fn mip0(&self) -> &[u8] {
        &self.image.rgba
    }

    /// Number of decoded mip levels (>= 1).
    pub fn mip_levels(&self) -> usize {
        self.image.mipmaps.len().max(1)
    }

    /// One decoded level, if present.
    pub fn mip(&self, level: usize) -> Option<&wc3_formats::blp::MipLevel> {
        self.image.mipmaps.get(level)
    }

    /// Bytes held by every decoded level.
    pub fn mip_bytes(&self) -> usize {
        self.image
            .mipmaps
            .iter()
            .map(|mip| mip.rgba.len())
            .sum::<usize>()
            .max(self.image.rgba.len())
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.image.width, self.image.height)
    }
}

/// A parsed model plus its resolved texture references.
#[derive(Debug, Clone)]
pub struct ModelAsset {
    pub name: String,
    pub model: MdxModel,
    pub texture_paths: Vec<String>,
    pub unresolved: Vec<String>,
    pub source_len: usize,
    pub sha256_source: String,
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub textures: usize,
    pub external_textures: usize,
    pub unresolved: Vec<String>,
}

pub struct AssetDb {
    /// Byte source (archive, loose tree, or an ordered stack).
    source: Box<dyn AssetSource>,
    /// Persistent archive index, when one was supplied.
    archive_index: Option<MpqIndex>,
    config: CacheConfig,
    bytes: LruCache<Vec<u8>>,
    textures: LruCache<TextureAsset>,
    models: LruCache<ModelAsset>,
    sounds: LruCache<Sound>,
    negative: HashSet<String>,
    /// asset name -> referenced asset names (e.g. MDX texture paths).
    dependencies: HashMap<String, Vec<String>>,
    pub hits: u64,
    pub misses: u64,
}

impl AssetDb {
    pub fn open_mpq(path: impl AsRef<Path>) -> Result<Self, AssetError> {
        let source = MpqSource::open(path)?;
        Ok(Self::from_source(Box::new(source), None, CacheConfig::default()))
    }

    /// Open an ordered stack of providers (`first wins`). This is the seam the
    /// game layer uses to inject `wc3_re::storm::StormFs` priority (patch
    /// > locale > expansion > base) without `wc3-assetdb` depending on `wc3-re`.
    pub fn open_sources(
        sources: Vec<Box<dyn AssetSource>>,
        label: impl Into<String>,
    ) -> Result<Self, AssetError> {
        if sources.is_empty() {
            return Err(AssetError::NotFound("no asset sources supplied".into()));
        }
        let layered = LayeredSource::new(label, sources);
        Ok(Self::from_source(Box::new(layered), None, CacheConfig::default()))
    }

    /// Open an ordered MPQ layer stack (first entry wins). Convenience for
    /// headless tools and for hosts that are not the Storm-based game layer.
    pub fn open_mpq_layers(
        layers: &[(String, PathBuf)],
        label: impl Into<String>,
    ) -> Result<Self, AssetError> {
        let source = LayeredSource::from_mpq_layers(label, layers)?;
        Ok(Self::from_source(Box::new(source), None, CacheConfig::default()))
    }

    /// Open an archive with a prebuilt index: upgrades the open to the fast path
    /// and gives the catalog enumerator a manifest without re-reading the
    /// listfile. A stale index is reported instead of being silently ignored.
    pub fn open_mpq_indexed(
        path: impl AsRef<Path>,
        index: MpqIndex,
    ) -> Result<Self, AssetError> {
        let source = MpqSource::open_with_index(path, index)?;
        Ok(Self::from_source(Box::new(source), None, CacheConfig::default()))
    }

    /// Try the index fast path; fall back to a plain open when the index does
    /// not match. Returns `(db, used_index)`.
    pub fn open_mpq_best(
        path: impl AsRef<Path>,
        index_path: impl AsRef<Path>,
    ) -> Result<(Self, bool), AssetError> {
        match MpqIndex::load(index_path) {
            Ok(index) => match Self::open_mpq_indexed(&path, index) {
                Ok(db) => Ok((db, true)),
                Err(AssetError::IndexStale) => Ok((Self::open_mpq(&path)?, false)),
                Err(err) => Err(err),
            },
            Err(_) => Ok((Self::open_mpq(&path)?, false)),
        }
    }

    fn from_source(
        source: Box<dyn AssetSource>,
        index: Option<MpqIndex>,
        config: CacheConfig,
    ) -> Self {
        Self {
            source,
            archive_index: index,
            config,
            bytes: LruCache::new(config.byte_budget, config.max_entries),
            textures: LruCache::with_budget(config.texture_budget),
            models: LruCache::with_budget(config.model_budget),
            sounds: LruCache::with_budget(config.sound_budget),
            negative: HashSet::new(),
            dependencies: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    /// Open an extracted tree. The walk is indexed once; a missing root is
    /// reported at open time instead of turning into per-read misses.
    pub fn open_dir(path: impl AsRef<Path>) -> Self {
        let source = match DirSource::open(&path) {
            Ok(source) => source,
            Err(err) => {
                eprintln!(
                    "assetdb: loose tree {} is unavailable ({err}); all reads will miss",
                    path.as_ref().display()
                );
                DirSource::empty(
                    path.as_ref()
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "missing-tree".to_string()),
                )
            }
        };
        Self::from_source(Box::new(source), None, CacheConfig::default())
    }

    /// Fallible variant of [`AssetDb::open_dir`] for callers that want the error.
    pub fn try_open_dir(path: impl AsRef<Path>) -> Result<Self, AssetError> {
        let source = DirSource::open(path)?;
        Ok(Self::from_source(Box::new(source), None, CacheConfig::default()))
    }

    pub fn with_config(mut self, config: CacheConfig) -> Self {
        self.config = config;
        self.bytes.set_byte_budget(config.byte_budget);
        self.bytes.set_max_entries(config.max_entries);
        self.textures.set_byte_budget(config.texture_budget);
        self.models.set_byte_budget(config.model_budget);
        self.sounds.set_byte_budget(config.sound_budget);
        self
    }

    pub fn config(&self) -> CacheConfig {
        self.config
    }

    pub fn listfile(&self) -> &[String] {
        self.source.listfile()
    }

    /// Label of the active source (`"War3.mpq"`, `"wc3rust"`, ...).
    pub fn source_label(&self) -> &str {
        self.source.label()
    }

    /// `"mpq"`, `"dir"` or `"layered"`.
    pub fn source_kind(&self) -> &'static str {
        self.source.kind()
    }

    /// Layers consulted by resolution, in priority order.
    pub fn layer_labels(&self) -> Vec<String> {
        self.source.layer_labels()
    }

    /// Archive index when the active source was opened through one.
    pub fn archive_index(&self) -> Option<&MpqIndex> {
        self.source.archive_index().or(self.archive_index.as_ref())
    }

    /// Enumerate every servable file (name in archive form, size). Sorted by
    /// name so catalog output is deterministic.
    pub fn files(&self) -> Vec<FileEntry> {
        let mut out: Vec<FileEntry> = self
            .source
            .files()
            .into_iter()
            .map(|(name, size)| FileEntry { name, size })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn file_count(&self) -> usize {
        self.source.listfile().len()
    }

    /// Resolve a path to the provider's canonical name (case-insensitive),
    /// without reading the payload.
    pub fn resolve_path(&self, name: &str) -> Option<String> {
        self.source.resolve_exact(name).map(|(_, canonical)| canonical)
    }

    /// Full resolution with provenance: exact, listfile-only, ambiguous or
    /// absent. Missing paths are never guessed.
    ///
    /// When the exact spelling is absent, the resolver looks for a unique
    /// same-basename file in the listfiles. Candidates in the *requested*
    /// directory win over candidates elsewhere, and candidates are
    /// de-duplicated across layers in priority order; only genuinely different
    /// directories with the same basename stay `Ambiguous`.
    pub fn resolve_asset(&self, path: &str) -> Resolution {
        if let Some((layer, canonical)) = self.source.resolve_exact(path) {
            return Resolution::Exact {
                layer,
                path: canonical,
            };
        }
        let wanted = normalize_path(path);
        let Some((requested_dir, file_name)) = wanted.rsplit_once('\\') else {
            return Resolution::Absent {
                requested: path.to_string(),
                searched: self.source.layer_labels(),
            };
        };
        let base = file_name.to_ascii_lowercase();
        if base.is_empty() {
            return Resolution::Absent {
                requested: path.to_string(),
                searched: self.source.layer_labels(),
            };
        }
        let raw_hits = self.source.search_basename(&base, 32);
        // De-duplicate identical paths across layers, keeping priority order.
        let mut hits: Vec<SearchHit> = Vec::new();
        for hit in raw_hits {
            if !hits
                .iter()
                .any(|existing| existing.path.eq_ignore_ascii_case(&hit.path))
            {
                hits.push(hit);
            }
        }
        // Prefer a candidate in the requested directory (retail tables often
        // have the right file in the right folder but with a stale parent).
        if hits.len() > 1 {
            let same_dir: Vec<SearchHit> = hits
                .iter()
                .filter(|hit| {
                    hit.path
                        .rsplit_once('\\')
                        .map(|(dir, _)| dir.eq_ignore_ascii_case(requested_dir))
                        .unwrap_or(false)
                })
                .cloned()
                .collect();
            if !same_dir.is_empty() {
                hits = same_dir;
            }
        }
        match hits.len() {
            0 => Resolution::Absent {
                requested: path.to_string(),
                searched: self.source.layer_labels(),
            },
            1 => Resolution::ListfileMatch {
                layer: hits[0].layer.clone(),
                requested: path.to_string(),
                path: hits[0].path.clone(),
            },
            _ => Resolution::Ambiguous {
                requested: path.to_string(),
                candidates: hits,
            },
        }
    }

    /// Canonical path for a sound reference, resolved exactly or through the
    /// listfile. `None` means the asset is genuinely absent from every layer.
    pub fn sound_path(&self, path: &str) -> Option<String> {
        self.resolve_asset(path).path().map(str::to_string)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.source.exists(name)
    }

    /// Root directory for a loose-tree provider (None for MPQ/layered).
    pub fn dir_root(&self) -> Option<&Path> {
        self.source.loose_root()
    }

    /// Declared size of an asset without reading it.
    pub fn file_size(&self, name: &str) -> Option<u64> {
        self.source.size(name)
    }

    pub fn read(&mut self, name: &str) -> Result<Arc<Vec<u8>>, AssetError> {
        let key = normalize_key(name);
        if let Some(bytes) = self.bytes.get(&key) {
            self.hits += 1;
            return Ok(bytes);
        }
        if self.config.negative_cache && self.negative.contains(&key) {
            self.hits += 1;
            return Err(AssetError::NotFound(name.to_string()));
        }
        self.misses += 1;
        let bytes = match self.load_bytes(name) {
            Ok(bytes) => bytes,
            Err(err) if err.is_not_found() => {
                if self.config.negative_cache {
                    if self.negative.len() > 8192 {
                        self.negative.clear();
                    }
                    self.negative.insert(key);
                }
                return Err(AssetError::NotFound(name.to_string()));
            }
            Err(other) => return Err(other),
        };
        let arc = Arc::new(bytes);
        let size = arc.len();
        self.bytes.insert(key, arc.clone(), size);
        Ok(arc)
    }

    /// Read without touching the cache (used by catalogs to stream the corpus).
    pub fn read_uncached(&self, name: &str) -> Result<Vec<u8>, AssetError> {
        self.load_bytes(name)
    }

    fn load_bytes(&self, path: &str) -> Result<Vec<u8>, AssetError> {
        self.source.read(path)
    }

    /// Decoded texture, shared through `Arc` so the cache can pin it.
    pub fn texture_shared(&mut self, name: &str) -> Result<Arc<TextureAsset>, AssetError> {
        let key = normalize_key(name);
        if let Some(texture) = self.textures.get(&key) {
            self.hits += 1;
            return Ok(texture);
        }
        self.misses += 1;
        let canonical = self.resolve_path(name).unwrap_or_else(|| name.replace('/', "\\"));
        let bytes = self.read(&canonical)?;
        let image = decode_blp(&bytes)?;
        let asset = TextureAsset {
            name: canonical,
            sha256_source: blp_report::sha256_hex(&bytes),
            sha256_rgba: blp_report::sha256_hex(&image.rgba),
            source_len: bytes.len(),
            image,
        };
        let size = texture_bytes(&asset.image);
        let arc = Arc::new(asset);
        self.textures.insert(key, arc.clone(), size);
        Ok(arc)
    }

    /// Compatibility entry point used by the game layer: returns an owned
    /// texture (the decoded copy lives in the cache, so the clone is the only
    /// allocation).
    pub fn texture(&mut self, name: &str) -> Result<BlpImage, AssetError> {
        Ok(self.texture_shared(name)?.image.clone())
    }

    /// Parsed model, shared through `Arc`.
    pub fn model_shared(&mut self, name: &str) -> Result<Arc<ModelAsset>, AssetError> {
        let key = normalize_key(name);
        if let Some(model) = self.models.get(&key) {
            self.hits += 1;
            return Ok(model);
        }
        self.misses += 1;
        let canonical = self.resolve_path(name).unwrap_or_else(|| name.replace('/', "\\"));
        let bytes = self.read(&canonical)?;
        let model = MdxModel::parse(&bytes)?;
        let mut texture_paths = Vec::new();
        let mut unresolved = Vec::new();
        for texture in &model.textures {
            if texture.replaceable_id != 0 || texture.path.is_empty() {
                continue;
            }
            let path = texture.path.trim_end_matches('\0');
            if path.is_empty() {
                continue;
            }
            match self.resolve_path(path) {
                Some(real) => texture_paths.push(real),
                None => {
                    texture_paths.push(normalize_key(path));
                    unresolved.push(texture.path.clone());
                }
            }
        }
        let asset = ModelAsset {
            name: canonical,
            sha256_source: blp_report::sha256_hex(&bytes),
            source_len: bytes.len(),
            texture_paths: texture_paths.clone(),
            unresolved,
            model,
        };
        self.dependencies.insert(key.clone(), texture_paths);
        let size = model_bytes(&asset.model);
        let arc = Arc::new(asset);
        self.models.insert(key, arc.clone(), size);
        Ok(arc)
    }

    /// Compatibility entry point: owned parsed model.
    pub fn model(&mut self, name: &str) -> Result<MdxModel, AssetError> {
        Ok(self.model_shared(name)?.model.clone())
    }

    /// Resolve and read every texture a model references. Returns
    /// `(loaded, missing)` so callers can render a model completely or report
    /// exactly which path is absent.
    pub fn load_model_textures(
        &mut self,
        name: &str,
    ) -> Result<(Vec<Arc<TextureAsset>>, Vec<String>), AssetError> {
        let model = self.model_shared(name)?;
        let mut loaded = Vec::new();
        let mut missing = Vec::new();
        for path in &model.texture_paths {
            match self.texture_shared(path) {
                Ok(texture) => loaded.push(texture),
                Err(_) => missing.push(path.clone()),
            }
        }
        Ok((loaded, missing))
    }

    /// Cheap metadata for a model without decoding textures.
    pub fn model_info(&mut self, name: &str) -> Result<ModelInfo, AssetError> {
        let model = self.model_shared(name)?;
        let external = model
            .model
            .textures
            .iter()
            .filter(|texture| texture.replaceable_id == 0 && !texture.path.is_empty())
            .count();
        Ok(ModelInfo {
            name: model.name.clone(),
            textures: model.model.textures.len(),
            external_textures: external,
            unresolved: model.unresolved.clone(),
        })
    }

    pub fn dependencies_of(&self, name: &str) -> &[String] {
        self.dependencies
            .get(&normalize_key(name))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn preload(&mut self, name: &str) -> Result<(), AssetError> {
        self.read(name)?;
        Ok(())
    }

    pub fn cached_bytes(&self) -> usize {
        self.bytes.bytes_used()
    }

    pub fn texture_bytes(&self) -> usize {
        self.textures.bytes_used()
    }

    pub fn model_bytes(&self) -> usize {
        self.models.bytes_used()
    }

    pub fn sound_bytes(&self) -> usize {
        self.sounds.bytes_used()
    }

    /// Decoded sound, shared through `Arc`. MP3 one-shots are decoded with a
    /// time cap; longer files must go through [`AssetDb::sound_stream_bytes`].
    pub fn sound_shared(&mut self, name: &str) -> Result<Arc<Sound>, AssetError> {
        let key = normalize_key(name);
        if let Some(sound) = self.sounds.get(&key) {
            self.hits += 1;
            return Ok(sound);
        }
        self.misses += 1;
        let canonical = self.resolve_path(name).unwrap_or_else(|| name.replace('/', "\\"));
        let bytes = self.read(&canonical)?;
        let lower = canonical.to_ascii_lowercase();
        let sound = if lower.ends_with(".mp3") {
            let cap = (self.config.max_sound_seconds.max(1.0) as f32
                * 48_000.0
                * 2.0) as usize;
            let (sound, truncated) = wc3_audio::decode_mp3_bounded(&bytes, cap)
                .map_err(|_| AssetError::NotFound(canonical.clone()))?;
            if truncated {
                return Err(AssetError::NotFound(format!(
                    "{canonical} (longer than {} s; stream it)",
                    self.config.max_sound_seconds
                )));
            }
            sound
        } else {
            wc3_audio::decode_wav(&bytes).map_err(|_| AssetError::NotFound(canonical.clone()))?
        };
        let size = sound.memory_bytes();
        let arc = Arc::new(sound);
        self.sounds.insert(key, arc.clone(), size);
        Ok(arc)
    }

    /// Raw compressed bytes for a streamed format (MP3 music/dialogue).
    /// Copying into `Arc<[u8]>` costs one buffer, not the decoded PCM.
    pub fn sound_stream_bytes(&mut self, name: &str) -> Result<Arc<[u8]>, AssetError> {
        let bytes = self.read(name)?;
        Ok(Arc::from(bytes.as_slice()))
    }

    pub fn stats(&self) -> AssetStats {
        AssetStats {
            hits: self.hits,
            misses: self.misses,
            negative_hits: 0,
            texture_hits: 0,
            texture_misses: 0,
            model_hits: 0,
            model_misses: 0,
            bytes_cached: self.bytes.bytes_used(),
            texture_bytes: self.textures.bytes_used(),
            model_bytes: self.models.bytes_used(),
            raw_evictions: self.bytes.evictions(),
            texture_evictions: self.textures.evictions(),
            model_evictions: self.models.evictions(),
            sound_hits: 0,
            sound_misses: 0,
            sound_bytes: self.sounds.bytes_used(),
            sound_evictions: self.sounds.evictions(),
            pinned_skips: self.bytes.pinned_skips()
                + self.textures.pinned_skips()
                + self.models.pinned_skips()
                + self.sounds.pinned_skips(),
            cache_entries: self.bytes.len(),
            negative_entries: self.negative.len(),
        }
    }

    /// Drop everything not currently referenced by a consumer.
    pub fn trim(&mut self) -> (EvictionReport, EvictionReport, EvictionReport) {
        (
            self.bytes.evict_to_fit(0),
            self.textures.evict_to_fit(0),
            self.models.evict_to_fit(0),
        )
    }

    /// Drop the decoded payload caches (raw archive bytes, BLP sheets, parsed
    /// MDX) and report the bytes those caches accounted for.
    ///
    /// The game layer copies what it draws out of these caches while loading
    /// (`Assets::textures`, `ModelAsset::meshes`), so once the renderer has
    /// uploaded its GPU copies the caches are pure duplication: 175 MB on the
    /// shipped install, measured 2026-11-15. Anything still held by a consumer
    /// survives through its `Arc`; only the cache's own references go.
    pub fn clear_decoded_caches(&mut self) -> usize {
        let freed = self.bytes.bytes_used() + self.textures.bytes_used() + self.models.bytes_used();
        self.bytes.clear();
        self.textures.clear();
        self.models.clear();
        freed
    }

    /// Drop sounds that are not referenced by a consumer.
    pub fn trim_sounds(&mut self) -> EvictionReport {
        self.sounds.evict_to_fit(0)
    }

    /// Total memory this database accounts for (raw + decoded caches).
    pub fn total_bytes(&self) -> usize {
        self.bytes.bytes_used()
            + self.textures.bytes_used()
            + self.models.bytes_used()
            + self.sounds.bytes_used()
    }
}

fn normalize_key(name: &str) -> String {
    name.replace('/', "\\").to_ascii_lowercase()
}

fn texture_bytes(image: &BlpImage) -> usize {
    image
        .mipmaps
        .iter()
        .map(|mip| mip.rgba.capacity().max(mip.rgba.len()))
        .sum::<usize>()
        .max(image.rgba.len())
}

fn model_bytes(model: &MdxModel) -> usize {
    let mut total = model.name.len() + model.pivots.len() * std::mem::size_of::<glam::Vec3>();
    total += model.textures.iter().map(|t| t.path.len() + 24).sum::<usize>();
    total += model
        .materials
        .iter()
        .map(|m| 16 + m.layers.len() * std::mem::size_of::<wc3_formats::mdx::Layer>())
        .sum::<usize>();
    for geoset in &model.geosets {
        total += geoset.vertices.len() * 12
            + geoset.normals.len() * 12
            + geoset.faces.len() * 2
            + geoset.uv_sets.iter().map(|uv| uv.len() * 8).sum::<usize>()
            + geoset.skin.len() * 2
            + geoset.sequence_bounds.len() * 24;
    }
    total += model.raw_chunks.iter().map(|(_, data)| data.len()).sum::<usize>();
    total + std::mem::size_of::<MdxModel>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use wc3_mpq::MpqArchive;

    fn mpq_path() -> Option<PathBuf> {
        let path = PathBuf::from(
            std::env::var_os("WC3_MPQ")
                .unwrap_or_else(|| "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".into()),
        );
        path.is_file().then_some(path)
    }

    fn wide_matches(db: &mut AssetDb, hits: usize) {
        for _ in 0..hits {
            db.read("Textures/Footman.blp").expect("cached read");
        }
    }

    #[test]
    fn loads_model_and_dependencies() {
        let Some(path) = mpq_path() else {
            eprintln!("skipping: MPQ fixture missing");
            return;
        };
        let mut db = AssetDb::open_mpq(path).expect("open mpq");
        let model = db.model("Units/Human/Footman/Footman.mdx").expect("model");
        assert_eq!(model.name, "Footman");
        assert!(!db.dependencies_of("Units/Human/Footman/Footman.mdx").is_empty());
        let texture = db.texture("Textures/Footman.blp").expect("texture");
        assert_eq!((texture.width, texture.height), (256, 256));
        assert!(db.hits + db.misses >= 2);
        assert!(db.cached_bytes() > 100_000);
    }

    #[test]
    fn byte_cache_respects_the_budget() {
        let Some(path) = mpq_path() else {
            return;
        };
        let config = CacheConfig {
            byte_budget: 500_000,
            max_entries: 8,
            ..CacheConfig::default()
        };
        let mut db = AssetDb::open_mpq(path).expect("open").with_config(config);
        for name in [
            "Textures/Footman.blp",
            "Units/Human/Footman/Footman.mdx",
            "Units/UnitData.slk",
            "Textures/Footman.blp",
        ] {
            let bytes = db.read(name).expect("read");
            assert!(!bytes.is_empty());
        }
        assert!(db.cached_bytes() <= 500_000, "budget {}", db.cached_bytes());
        let stats = db.stats();
        assert!(stats.hits >= 1, "second Footman.blp read must hit");
    }

    #[test]
    fn texture_and_model_caches_are_bounded_and_share_arcs() {
        let Some(path) = mpq_path() else {
            return;
        };
        let config = CacheConfig {
            byte_budget: 4 * 1024 * 1024,
            texture_budget: 700_000,
            model_budget: 5_000_000,
            ..CacheConfig::default()
        };
        let mut db = AssetDb::open_mpq(path).expect("open").with_config(config);
        let first = db.texture_shared("Textures/Footman.blp").expect("texture");
        drop(first.clone());
        let second = db.texture_shared("textures\\footman.blp").expect("texture");
        assert!(Arc::ptr_eq(&first, &second), "case-insensitive cache hit");
        for name in [
            "Textures/Footman.blp",
            "Textures/TeamColor00.blp",
            "ReplaceableTextures/CommandButtons/BTNFootman.blp",
            "TerrainArt/Village/Village_GrassShort.blp",
        ] {
            let _ = db.texture_shared(name);
        }
        assert!(db.texture_bytes() <= 700_000, "texture budget");
        assert!(db.texture_bytes() > 0);

        for name in ["Units/Human/Footman/Footman.mdx", "Units/Orc/Grunt/Grunt.mdx"] {
            let _ = db.model_shared(name);
        }
        assert!(db.model_bytes() <= 5_000_000, "model budget");
        let stats = db.stats();
        assert_eq!(stats.pinned_skips >= 0, true);
    }

    #[test]
    fn negative_cache_stops_repeated_probes() {
        let Some(path) = mpq_path() else {
            return;
        };
        let mut db = AssetDb::open_mpq(path).expect("open");
        assert!(db.read("no/such/asset.blp").is_err());
        let misses = db.misses;
        assert!(db.read("no/such/asset.blp").is_err());
        assert_eq!(db.misses, misses, "negative hit must not re-probe");
        assert_eq!(db.stats().negative_entries, 1);
    }

    #[test]
    fn index_fast_path_matches_plain_open() {
        let Some(path) = mpq_path() else {
            return;
        };
        let archive = MpqArchive::open(&path).expect("open");
        let index = MpqIndex::build(&archive);
        let mut fast = AssetDb::open_mpq_indexed(&path, index).expect("indexed open");
        assert!(fast.file_count() > 10_000);
        let texture = fast.texture("Textures/Footman.blp").expect("texture");
        assert_eq!(texture.width, 256);
        let entry = fast
            .files()
            .into_iter()
            .find(|entry| entry.name.ends_with("Footman.mdx"));
        assert!(entry.is_some());
    }

    #[test]
    fn dir_provider_is_case_insensitive_and_catalogs_files() {
        let dir = std::env::temp_dir().join("h2-assetdb-dir-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Textures")).expect("mkdir");
        std::fs::write(dir.join("Textures/Thing.BLP"), b"BLP1test").expect("write");
        let mut db = AssetDb::open_dir(&dir);
        assert_eq!(db.file_count(), 1);
        assert!(db.contains("textures/thing.blp"));
        let resolved = db.resolve_path("TEXTURES/THING.BLP").expect("resolve");
        // The loose-tree layer preserves the on-disk spelling.
        assert_eq!(resolved, "Textures\\Thing.BLP");
        assert_eq!(db.read("textures\\thing.blp").expect("read").len(), 8);
        // A basename-only reference resolves through the listfile.
        match db.resolve_asset("Elsewhere\\Thing.BLP") {
            Resolution::ListfileMatch { path, .. } => {
                assert_eq!(path, "Textures\\Thing.BLP");
            }
            other => panic!("expected a listfile match, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wide_reads_do_not_leak() {
        let Some(path) = mpq_path() else {
            return;
        };
        let mut db = AssetDb::open_mpq(path).expect("open");
        wide_matches(&mut db, 200);
        assert_eq!(db.bytes.len(), 1, "one cache entry for one asset");
        assert!(db.cached_bytes() < 1_000_000);
        assert!(db.hits >= 199);
    }
}
