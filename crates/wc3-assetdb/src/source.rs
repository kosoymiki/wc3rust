//! Asset providers: one archive, one loose tree, or an ordered stack of both.
//!
//! This module owns *where bytes come from*; [`crate::AssetDb`] owns caching.
//! Priority is the layer order (first layer wins), matching the game's loader:
//! patch > locale > expansion > base. The WC3 install *order* is not hardcoded
//! here — `wc3-game` wires `wc3_re::storm::StormFs` through
//! [`crate::AssetSource`] (see `analysis/tech-lead/merge-assets.md`), so this
//! crate never depends on `wc3-re`.
//!
//! Path resolution is listfile-driven on purpose: retail sound tables contain
//! stale directory bases (for example `Buildings\Orc\PigFarm\PigFarmWhat1.wav`
//! while the file ships as `buildings\Other\PigFarm\PigFarmWhat1.wav`). The
//! resolver reports `Exact`, `ListfileMatch`, `Ambiguous` or `Absent` with the
//! layer that answered — it never invents a path.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use wc3_mpq::{MpqArchive, MpqIndex};

use crate::AssetError;

/// One place a path was found: the layer label and the canonical spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub layer: String,
    pub path: String,
}

/// What the resolver found for a requested path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// The exact path exists in this layer.
    Exact { layer: String, path: String },
    /// The exact path is missing but exactly one file with the same basename
    /// exists in the listfile of `layer`; `path` is that canonical spelling.
    ListfileMatch {
        layer: String,
        requested: String,
        path: String,
    },
    /// Several listfile candidates share the basename; no path is chosen.
    Ambiguous {
        requested: String,
        candidates: Vec<SearchHit>,
    },
    /// Nothing in any layer. `searched` lists the layers consulted.
    Absent {
        requested: String,
        searched: Vec<String>,
    },
}

impl Resolution {
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::Exact { path, .. } | Self::ListfileMatch { path, .. } => Some(path),
            _ => None,
        }
    }

    pub fn layer(&self) -> Option<&str> {
        match self {
            Self::Exact { layer, .. } | Self::ListfileMatch { layer, .. } => Some(layer),
            _ => None,
        }
    }

    pub fn is_resolved(&self) -> bool {
        self.path().is_some()
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Exact { .. } => "exact",
            Self::ListfileMatch { .. } => "listfile",
            Self::Ambiguous { .. } => "ambiguous",
            Self::Absent { .. } => "absent",
        }
    }
}

/// A provider of archive-style paths (`\` separators, case-insensitive).
pub trait AssetSource: Send {
    /// Human-readable layer name (`"War3.mpq"`, `"war3_full3"`, ...).
    fn label(&self) -> &str;
    /// `"mpq"` or `"dir"`.
    fn kind(&self) -> &'static str;

    fn read(&self, path: &str) -> Result<Vec<u8>, AssetError>;
    fn size(&self, path: &str) -> Option<u64>;
    /// Every name the source can enumerate (listfile for archives, walk for dirs).
    fn listfile(&self) -> &[String];
    /// `(name, size)` rows for catalogs.
    fn files(&self) -> Vec<(String, u64)>;

    /// Canonical spelling when `path` exists in this layer.
    fn resolve_exact(&self, path: &str) -> Option<(String, String)>;
    /// Files whose basename matches (case-insensitive), up to `limit`.
    fn search_basename(&self, basename: &str, limit: usize) -> Vec<SearchHit>;

    fn exists(&self, path: &str) -> bool {
        self.resolve_exact(path).is_some()
    }

    /// Layers consulted by the resolver (one for a single source).
    fn layer_labels(&self) -> Vec<String> {
        vec![self.label().to_string()]
    }

    /// Root directory for a loose-tree source.
    fn loose_root(&self) -> Option<&Path> {
        None
    }

    /// Prebuilt archive index, when this source was opened through one.
    fn archive_index(&self) -> Option<&MpqIndex> {
        None
    }
}

/// Canonical archive spelling: `\` separators, no leading separator.
pub fn normalize_path(path: &str) -> String {
    path.trim()
        .trim_start_matches(['\\', '/'])
        .replace('/', "\\")
}

fn basename(path: &str) -> String {
    path.replace('/', "\\")
        .rsplit('\\')
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase()
}

fn basename_index(names: &[String]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for name in names {
        map.entry(basename(name)).or_default().push(name.clone());
    }
    map
}

// ---------------------------------------------------------------------------
// MPQ archive layer
// ---------------------------------------------------------------------------

/// One MPQ archive, optionally opened through a prebuilt [`MpqIndex`].
pub struct MpqSource {
    label: String,
    archive: MpqArchive,
    index: Option<MpqIndex>,
    listfile: Vec<String>,
    by_basename: HashMap<String, Vec<String>>,
}

impl MpqSource {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AssetError> {
        let label = path
            .as_ref()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.as_ref().display().to_string());
        let archive = MpqArchive::open(path)?;
        Ok(Self::from_archive(label, archive, None))
    }

    pub fn open_with_index(
        path: impl AsRef<Path>,
        index: MpqIndex,
    ) -> Result<Self, AssetError> {
        let label = path
            .as_ref()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.as_ref().display().to_string());
        let archive = MpqArchive::open_with_index(path, &index)?;
        Ok(Self::from_archive(label, archive, Some(index)))
    }

    pub fn from_archive(label: impl Into<String>, archive: MpqArchive, index: Option<MpqIndex>) -> Self {
        let listfile: Vec<String> = archive
            .listfile()
            .iter()
            .map(|name| normalize_path(name))
            .collect();
        let by_basename = basename_index(&listfile);
        Self {
            label: label.into(),
            archive,
            index,
            listfile,
            by_basename,
        }
    }

    pub fn index(&self) -> Option<&MpqIndex> {
        self.index.as_ref()
    }

    /// Rename the layer (the archive's own file name is the default).
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
}

impl AssetSource for MpqSource {
    fn label(&self) -> &str {
        &self.label
    }

    fn kind(&self) -> &'static str {
        "mpq"
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, AssetError> {
        Ok(self.archive.read(&normalize_path(path))?)
    }

    fn size(&self, path: &str) -> Option<u64> {
        self.archive
            .meta(&normalize_path(path))
            .map(|(_, meta)| meta.size as u64)
    }

    fn listfile(&self) -> &[String] {
        &self.listfile
    }

    fn files(&self) -> Vec<(String, u64)> {
        match &self.index {
            Some(index) => index
                .entries()
                .iter()
                .map(|entry| (entry.name.clone(), entry.size as u64))
                .collect(),
            None => self
                .archive
                .files()
                .map(|(name, meta)| (normalize_path(name), meta.size as u64))
                .collect(),
        }
    }

    fn resolve_exact(&self, path: &str) -> Option<(String, String)> {
        let normalized = normalize_path(path);
        if let Some(index) = &self.index {
            if let Some(entry) = index.get(&normalized) {
                return Some((self.label.clone(), entry.name.clone()));
            }
        }
        if !self.archive.contains(&normalized) {
            return None;
        }
        // Prefer the listfile spelling so callers keep the archive's casing.
        let canonical = self
            .listfile
            .iter()
            .find(|name| name.eq_ignore_ascii_case(&normalized))
            .cloned()
            .unwrap_or(normalized);
        Some((self.label.clone(), canonical))
    }

    fn search_basename(&self, basename: &str, limit: usize) -> Vec<SearchHit> {
        self.by_basename
            .get(&basename.to_ascii_lowercase())
            .map(|hits| {
                hits.iter()
                    .take(limit)
                    .map(|path| SearchHit {
                        layer: self.label.clone(),
                        path: path.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn archive_index(&self) -> Option<&MpqIndex> {
        self.index.as_ref()
    }
}

// ---------------------------------------------------------------------------
// Loose directory layer
// ---------------------------------------------------------------------------

/// An extracted tree (`analysis/war3_full3`, a TFT extract, ...).
pub struct DirSource {
    label: String,
    root: PathBuf,
    /// lowercase archive-style name -> real path
    index: HashMap<String, PathBuf>,
    /// lowercase archive-style name -> canonical archive-style name
    canonical: HashMap<String, String>,
    sizes: HashMap<String, u64>,
    listfile: Vec<String>,
    by_basename: HashMap<String, Vec<String>>,
}

impl DirSource {
    /// A source with no files: used when a configured tree is missing so the
    /// database reports `NotFound` instead of silently reading elsewhere.
    pub fn empty(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            root: PathBuf::new(),
            index: HashMap::new(),
            canonical: HashMap::new(),
            sizes: HashMap::new(),
            listfile: Vec::new(),
            by_basename: HashMap::new(),
        }
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self, AssetError> {
        let root = root.as_ref().to_path_buf();
        if !root.is_dir() {
            return Err(AssetError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("not a directory: {}", root.display()),
            )));
        }
        let label = root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string());
        let mut index = HashMap::new();
        let mut canonical = HashMap::new();
        let mut sizes = HashMap::new();
        let mut listfile = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in read.flatten() {
                let path = entry.path();
                match entry.file_type() {
                    Ok(kind) if kind.is_dir() => stack.push(path),
                    Ok(kind) if kind.is_file() => {
                        let relative = normalize_path(&path
                            .strip_prefix(&root)
                            .unwrap_or(&path)
                            .to_string_lossy());
                        let key = relative.to_ascii_lowercase();
                        let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
                        listfile.push(relative.clone());
                        canonical.insert(key.clone(), relative);
                        index.insert(key.clone(), path);
                        sizes.insert(key, size);
                    }
                    _ => {}
                }
            }
        }
        listfile.sort();
        let by_basename = basename_index(&listfile);
        Ok(Self {
            label,
            root,
            index,
            canonical,
            sizes,
            listfile,
            by_basename,
        })
    }
}

impl AssetSource for DirSource {
    fn label(&self) -> &str {
        &self.label
    }

    fn kind(&self) -> &'static str {
        "dir"
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, AssetError> {
        let key = normalize_path(path).to_ascii_lowercase();
        let file = self
            .index
            .get(&key)
            .ok_or_else(|| AssetError::NotFound(path.to_string()))?;
        std::fs::read(file).map_err(AssetError::Io)
    }

    fn size(&self, path: &str) -> Option<u64> {
        self.sizes
            .get(&normalize_path(path).to_ascii_lowercase())
            .copied()
    }

    fn listfile(&self) -> &[String] {
        &self.listfile
    }

    fn files(&self) -> Vec<(String, u64)> {
        self.listfile
            .iter()
            .map(|name| {
                let size = self
                    .sizes
                    .get(&name.to_ascii_lowercase())
                    .copied()
                    .unwrap_or(0);
                (name.clone(), size)
            })
            .collect()
    }

    fn resolve_exact(&self, path: &str) -> Option<(String, String)> {
        let key = normalize_path(path).to_ascii_lowercase();
        self.canonical
            .get(&key)
            .map(|canonical| (self.label.clone(), canonical.clone()))
    }

    fn search_basename(&self, basename: &str, limit: usize) -> Vec<SearchHit> {
        self.by_basename
            .get(&basename.to_ascii_lowercase())
            .map(|hits| {
                hits.iter()
                    .take(limit)
                    .map(|path| SearchHit {
                        layer: self.label.clone(),
                        path: path.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn loose_root(&self) -> Option<&Path> {
        Some(self.root.as_path())
    }
}

// ---------------------------------------------------------------------------
// Ordered stack
// ---------------------------------------------------------------------------

/// Layers in priority order: index 0 wins. Composed by the caller; the WC3
/// install order lives in the game layer (`StormFs`) and in analysis tooling.
pub struct LayeredSource {
    label: String,
    layers: Vec<Box<dyn AssetSource>>,
    listfile: Vec<String>,
    by_basename: HashMap<String, Vec<(usize, String)>>,
}

impl LayeredSource {
    pub fn new(label: impl Into<String>, layers: Vec<Box<dyn AssetSource>>) -> Self {
        let mut listfile: Vec<String> = Vec::new();
        let mut seen: HashMap<String, ()> = HashMap::new();
        let mut by_basename: HashMap<String, Vec<(usize, String)>> = HashMap::new();
        for (slot, layer) in layers.iter().enumerate() {
            for name in layer.listfile() {
                let key = name.to_ascii_lowercase();
                if seen.insert(key, ()).is_none() {
                    listfile.push(name.clone());
                }
                by_basename
                    .entry(basename(name))
                    .or_default()
                    .push((slot, name.clone()));
            }
        }
        listfile.sort();
        Self {
            label: label.into(),
            layers,
            listfile,
            by_basename,
        }
    }

    pub fn layers(&self) -> &[Box<dyn AssetSource>] {
        &self.layers
    }

    /// Build a priority stack from MPQ archives (first entry wins).
    ///
    /// Layer policy stays with the caller: `wc3-game` uses
    /// `wc3_re::storm::StormFs`, analysis tools pass the classic install order
    /// (`War3Patch/patch` > `War3xlocal` > `War3x` > `War3`). Missing files are
    /// skipped so a base-only install still opens.
    pub fn from_mpq_layers(
        label: impl Into<String>,
        layers: &[(String, PathBuf)],
    ) -> Result<Self, AssetError> {
        let mut sources: Vec<Box<dyn AssetSource>> = Vec::with_capacity(layers.len());
        for (name, path) in layers {
            if !path.is_file() {
                continue;
            }
            let source = MpqSource::open(path)?.with_label(name.clone());
            sources.push(Box::new(source));
        }
        if sources.is_empty() {
            return Err(AssetError::NotFound("no archive layers found".into()));
        }
        Ok(Self::new(label, sources))
    }
}

impl AssetSource for LayeredSource {
    fn label(&self) -> &str {
        &self.label
    }

    fn kind(&self) -> &'static str {
        "layered"
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, AssetError> {
        let mut last = AssetError::NotFound(path.to_string());
        for layer in &self.layers {
            match layer.read(path) {
                Ok(bytes) => return Ok(bytes),
                Err(AssetError::NotFound(_)) => continue,
                Err(err) => last = err,
            }
        }
        Err(last)
    }

    fn size(&self, path: &str) -> Option<u64> {
        self.layers.iter().find_map(|layer| layer.size(path))
    }

    fn listfile(&self) -> &[String] {
        &self.listfile
    }

    fn files(&self) -> Vec<(String, u64)> {
        self.listfile
            .iter()
            .map(|name| {
                let size = self.size(name).unwrap_or(0);
                (name.clone(), size)
            })
            .collect()
    }

    fn resolve_exact(&self, path: &str) -> Option<(String, String)> {
        self.layers
            .iter()
            .find_map(|layer| layer.resolve_exact(path))
    }

    fn search_basename(&self, basename: &str, limit: usize) -> Vec<SearchHit> {
        let Some(hits) = self.by_basename.get(&basename.to_ascii_lowercase()) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(hits.len().min(limit));
        for (slot, path) in hits {
            if out.len() >= limit {
                break;
            }
            let Some(layer) = self.layers.get(*slot) else {
                continue;
            };
            out.push(SearchHit {
                layer: layer.label().to_string(),
                path: path.clone(),
            });
        }
        out
    }

    fn layer_labels(&self) -> Vec<String> {
        self.layers
            .iter()
            .map(|layer| layer.label().to_string())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(rows: &[&str]) -> Vec<String> {
        rows.iter().map(|row| row.to_string()).collect()
    }

    #[test]
    fn basename_index_is_case_insensitive() {
        let list = names(&["Units\\Human\\Footman\\FootmanWhat1.wav", "buildings\\Other\\PigFarm\\PigFarmWhat1.wav"]);
        let index = basename_index(&list);
        assert_eq!(index.get("footmanwhat1.wav").map(Vec::len), Some(1));
        assert_eq!(index.get("pigfarmwhat1.wav").map(Vec::len), Some(1));
    }

    #[test]
    fn normalize_path_matches_archive_spelling() {
        assert_eq!(normalize_path("/Units/Human/Footman.mdx"), "Units\\Human\\Footman.mdx");
        assert_eq!(normalize_path("units\\human\\footman.mdx"), "units\\human\\footman.mdx");
    }

    #[test]
    fn resolution_kinds_are_stable() {
        let exact = Resolution::Exact {
            layer: "War3.mpq".into(),
            path: "a\\b.wav".into(),
        };
        assert!(exact.is_resolved());
        assert_eq!(exact.kind(), "exact");
        let absent = Resolution::Absent {
            requested: "x".into(),
            searched: vec!["War3.mpq".into()],
        };
        assert!(!absent.is_resolved());
        assert_eq!(absent.kind(), "absent");
    }

    /// Install-order fixture: locale > expansion > base, each a loose dir.
    fn install_layers(root: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        let high = root.join("War3xlocal");
        let mid = root.join("War3x");
        let low = root.join("War3");
        for dir in [&high, &mid, &low] {
            std::fs::create_dir_all(dir.join("Units/Naga")).expect("mkdir");
            std::fs::create_dir_all(dir.join("Sound")).expect("mkdir");
        }
        // The same archive path exists in every layer with different bytes.
        std::fs::write(high.join("Units/Naga/Thing.wav"), b"locale").expect("write");
        std::fs::write(mid.join("Units/Naga/Thing.wav"), b"expansion").expect("write");
        std::fs::write(low.join("Units/Naga/Thing.wav"), b"base").expect("write");
        // One file that exists only in the expansion layer, one only in base.
        std::fs::write(mid.join("Sound/MidOnly.wav"), b"mid-only").expect("write");
        std::fs::write(low.join("Sound/BaseOnly.wav"), b"base-only").expect("write");
        (high, mid, low)
    }

    #[test]
    fn layered_stack_resolves_from_the_first_matching_layer() {
        let root = std::env::temp_dir().join("h2-assetdb-layer-precedence");
        let _ = std::fs::remove_dir_all(&root);
        let (high, mid, low) = install_layers(&root);
        let stack = LayeredSource::new(
            "wc3-install",
            vec![
                Box::new(DirSource::open(&high).expect("high")),
                Box::new(DirSource::open(&mid).expect("mid")),
                Box::new(DirSource::open(&low).expect("low")),
            ],
        );

        // Shadowed path: bytes and provenance both name the locale layer.
        assert_eq!(stack.read("Units\\Naga\\Thing.wav").expect("read"), b"locale");
        let (layer, canonical) = stack
            .resolve_exact("units\\naga\\thing.wav")
            .expect("resolve");
        assert_eq!(layer, "War3xlocal");
        assert_eq!(canonical, "Units\\Naga\\Thing.wav");
        assert_eq!(stack.size("Units\\Naga\\Thing.wav"), Some(6));

        // Files present in exactly one layer still resolve from that layer.
        assert_eq!(stack.read("Sound\\MidOnly.wav").expect("mid"), b"mid-only");
        assert_eq!(stack.read("Sound\\BaseOnly.wav").expect("base"), b"base-only");

        // The merged listfile holds each archived name once, even when shadowed.
        let names = stack.listfile().to_vec();
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(names.len(), 3, "shadowed entries are deduplicated");
        assert_eq!(names, unique);

        // Basename hits keep layer priority order.
        let hits = stack.search_basename("Thing.wav", 8);
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].layer, "War3xlocal");
        assert_eq!(hits[1].layer, "War3x");
        assert_eq!(hits[2].layer, "War3");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn asset_db_open_sources_keeps_caller_priority() {
        let root = std::env::temp_dir().join("h2-assetdb-layer-db");
        let _ = std::fs::remove_dir_all(&root);
        let (high, mid, low) = install_layers(&root);
        let mut db = crate::AssetDb::open_sources(
            vec![
                Box::new(DirSource::open(&high).expect("high")),
                Box::new(DirSource::open(&mid).expect("mid")),
                Box::new(DirSource::open(&low).expect("low")),
            ],
            "wc3-install",
        )
        .expect("open sources");

        // The game injects Storm layers through this seam: first source wins.
        assert_eq!(
            db.read("Units\\Naga\\Thing.wav").expect("read").as_slice(),
            b"locale"
        );
        match db.resolve_asset("Units\\Naga\\Thing.wav") {
            Resolution::Exact { layer, path } => {
                assert_eq!(layer, "War3xlocal");
                assert_eq!(path, "Units\\Naga\\Thing.wav");
            }
            other => panic!("expected an exact locale hit, got {other:?}"),
        }
        // A base-only path is still reachable through the stack.
        assert_eq!(
            db.read("Sound\\BaseOnly.wav").expect("base").as_slice(),
            b"base-only"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
