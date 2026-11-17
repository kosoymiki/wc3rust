//! Resource access abstraction and listfile catalogue.
//!
//! This crate never opens an MPQ archive: it consumes bytes through
//! [`Source`]. The reverse-engineering layer
//! (`crates/wc3-re`, owner: RE agent) is expected to provide an implementation
//! backed by StormLib/CascLib-compatible archive access; [`DirSource`] covers the
//! extracted trees in `analysis/war3_data` / `analysis/war3x_data`, and
//! [`MemorySource`] lets callers hand over archive bytes directly.
//!
//! See `analysis/agent-data/INTEGRATION.md` for the API the RE layer must expose.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{DataError, Result};

/// Source of WC3 resource bytes addressed by in-game path (`Units\UnitData.slk`).
///
/// This is the `Source` entry point named in `analysis/tech-lead/BASE.md`. The
/// trait carries no archive knowledge: `wc3-re` (or a test fixture) supplies the
/// bytes and this crate owns the typed interpretation.
pub trait Source {
    /// Read a resource. `path` uses either `\` or `/` separators.
    fn read(&self, path: &str) -> Result<Vec<u8>>;

    /// Whether the resource exists.
    fn exists(&self, path: &str) -> bool {
        self.read(path).is_ok()
    }

    /// Human readable description of where bytes come from (used in reports).
    fn describe(&self) -> String;
}

/// Filesystem tree of extracted game files.
#[derive(Debug, Clone)]
pub struct DirSource {
    root: PathBuf,
    /// Case-insensitive directory listing cache (`dir` → `lowercase name → real name`).
    listings: std::cell::RefCell<HashMap<PathBuf, HashMap<String, String>>>,
}

impl DirSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            listings: std::cell::RefCell::new(HashMap::new()),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve a WC3 path to an on-disk path, tolerating case differences
    /// (`unitUI.slk` vs `unitui.slk`), which matter on Android/Linux.
    pub fn resolve(&self, path: &str) -> Option<PathBuf> {
        let normalized = normalize_wc3_path(path);
        let direct = self.root.join(&normalized);
        if direct.is_file() {
            return Some(direct);
        }
        let mut current = self.root.clone();
        for component in normalized.split('/') {
            if component.is_empty() {
                continue;
            }
            let exact = current.join(component);
            if exact.exists() {
                current = exact;
                continue;
            }
            let real = self.lookup_ci(&current, component)?;
            current = current.join(real);
        }
        current.is_file().then_some(current)
    }

    fn lookup_ci(&self, dir: &Path, name: &str) -> Option<String> {
        let mut cache = self.listings.borrow_mut();
        let entry = cache.entry(dir.to_path_buf()).or_insert_with(|| {
            let mut map = HashMap::new();
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        map.insert(name.to_ascii_lowercase(), name.to_string());
                    }
                }
            }
            map
        });
        entry.get(&name.to_ascii_lowercase()).cloned()
    }
}

impl Source for DirSource {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        let resolved = self.resolve(path).ok_or_else(|| DataError::Missing {
            path: path.to_string(),
            root: self.root.clone(),
        })?;
        std::fs::read(&resolved).map_err(|source| DataError::Io {
            path: resolved.display().to_string(),
            source,
        })
    }

    fn exists(&self, path: &str) -> bool {
        self.resolve(path).is_some()
    }

    fn describe(&self) -> String {
        format!("dir:{}", self.root.display())
    }
}

/// In-memory resource set, for archive-backed callers and tests.
#[derive(Debug, Clone, Default)]
pub struct MemorySource {
    files: HashMap<String, Arc<Vec<u8>>>,
}

impl MemorySource {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a resource; the path is normalised to lowercase `a/b/c` form.
    pub fn insert(&mut self, path: &str, bytes: Vec<u8>) {
        self.files
            .insert(normalize_wc3_path(path).to_ascii_lowercase(), Arc::new(bytes));
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}

impl Source for MemorySource {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        let key = normalize_wc3_path(path).to_ascii_lowercase();
        self.files
            .get(&key)
            .map(|bytes| bytes.as_ref().clone())
            .ok_or_else(|| DataError::Missing {
                path: path.to_string(),
                root: PathBuf::from("<memory>"),
            })
    }

    fn exists(&self, path: &str) -> bool {
        self.files
            .contains_key(&normalize_wc3_path(path).to_ascii_lowercase())
    }

    fn describe(&self) -> String {
        format!("memory:{} files", self.files.len())
    }
}

/// Backwards-compatible alias: `Source` is the canonical name.
pub use self::Source as ResourceSource;

/// Convert `Units\UnitData.slk` / `units/unitdata.slk` to `Units/UnitData.slk`.
pub fn normalize_wc3_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut last_slash = true;
    for ch in path.chars() {
        if ch == '\\' || ch == '/' {
            if !last_slash {
                out.push('/');
            }
            last_slash = true;
        } else {
            out.push(ch);
            last_slash = false;
        }
    }
    while out.ends_with('/') {
        out.pop();
    }
    out
}

/// Catalogue of paths known to the archives (`(listfile)` / `war3_mpq_listing.txt`).
///
/// Path validation for model/icon references goes through this type so the
/// answer always comes from the game's own path list, never from a guess.
#[derive(Debug, Clone, Default)]
pub struct Listfile {
    paths: HashSet<String>,
    origin: String,
    entries: usize,
}

impl Listfile {
    /// Parse a listfile (`war3_mpq_listing.txt`, `list-War3-clean.txt`, …).
    ///
    /// One path per line; `//` comments, blank lines and the `war3x.txt` marker
    /// line used by the extracted listfiles are ignored.
    pub fn parse(origin: impl Into<String>, text: &str) -> Self {
        let mut paths = HashSet::new();
        let mut entries = 0usize;
        for line in text.lines() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() || line.starts_with("//") || line.starts_with(';') {
                continue;
            }
            entries += 1;
            paths.insert(normalize_wc3_path(line).to_ascii_lowercase());
        }
        Self {
            paths,
            origin: origin.into(),
            entries,
        }
    }

    /// Load a listfile from disk.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| DataError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Ok(Self::parse(path.display().to_string(), &text))
    }

    /// Build from any iterator of WC3 paths.
    pub fn from_paths<I, S>(origin: impl Into<String>, paths: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut set = HashSet::new();
        let mut entries = 0usize;
        for path in paths {
            entries += 1;
            set.insert(normalize_wc3_path(path.as_ref()).to_ascii_lowercase());
        }
        Self {
            paths: set,
            origin: origin.into(),
            entries,
        }
    }

    /// Merge another listfile into this one (RoC + TFT + localised archives).
    pub fn extend(&mut self, other: &Listfile) {
        self.paths.extend(other.paths.iter().cloned());
        self.entries = self.paths.len();
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Number of lines read (before de-duplication).
    pub fn entries(&self) -> usize {
        self.entries
    }

    /// Number of unique paths.
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Exact path membership (case-insensitive, separator-insensitive).
    pub fn contains(&self, path: &str) -> bool {
        self.paths
            .contains(&normalize_wc3_path(path).to_ascii_lowercase())
    }

    /// Resolve a data path that may omit its extension.
    ///
    /// `unitUI.file` stores model paths without `.mdx` (`units\nightelf\Archer\Archer`),
    /// so a bare path is completed with `.mdx` — the only extension the column
    /// ever needs (`observed`: all 466 RoC / 812 TFT rows resolve this way and 0
    /// stay unresolved).
    pub fn resolve_model(&self, path: &str) -> Option<String> {
        let normalized = normalize_wc3_path(path).to_ascii_lowercase();
        if normalized.is_empty() {
            return None;
        }
        if self.paths.contains(&normalized) {
            return Some(normalized);
        }
        if Path::new(&normalized).extension().is_none() {
            let with_ext = format!("{normalized}.mdx");
            if self.paths.contains(&with_ext) {
                return Some(with_ext);
            }
        }
        // Item models are stored as `.mdl` while the archive ships `.mdx`
        // (`observed`: `treasurechest.mdl` in `ItemData.slk`, `.mdx` in the listfile).
        if let Some(stem) = normalized.strip_suffix(".mdl") {
            let with_mdx = format!("{stem}.mdx");
            if self.paths.contains(&with_mdx) {
                return Some(with_mdx);
            }
        }
        None
    }

    /// Resolve a texture/icon path, which always carries its extension.
    pub fn resolve_file(&self, path: &str) -> Option<String> {
        let normalized = normalize_wc3_path(path).to_ascii_lowercase();
        self.paths.contains(&normalized).then_some(normalized)
    }

    /// Count of paths matching a `prefix` + `extension` pair, e.g.
    /// (`buildings/`, `.mdx`) → 131 in the RoC listfile.
    pub fn count(&self, prefix: &str, extension: &str) -> usize {
        // A prefix always means a directory, so the separator is enforced:
        // `ReplaceableTextures\CommandButtons` must not also match
        // `ReplaceableTextures\CommandButtonsDisabled`.
        let mut prefix = normalize_wc3_path(prefix).to_ascii_lowercase();
        if !prefix.is_empty() && !prefix.ends_with('/') {
            prefix.push('/');
        }
        let extension = extension.to_ascii_lowercase();
        self.paths
            .iter()
            .filter(|path| path.starts_with(&prefix) && path.ends_with(&extension))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_separators_and_duplicates() {
        assert_eq!(normalize_wc3_path("Units\\UnitData.slk"), "Units/UnitData.slk");
        assert_eq!(normalize_wc3_path("units//unitdata.slk"), "units/unitdata.slk");
        assert_eq!(normalize_wc3_path("a\\b\\"), "a/b");
    }

    #[test]
    fn listfile_membership_is_case_insensitive() {
        let list = Listfile::parse(
            "test",
            "Units\\NightElf\\Archer\\Archer.mdx\n// comment\n\nwar3x.txt\n",
        );
        assert_eq!(list.entries(), 2);
        assert_eq!(list.len(), 2);
        assert!(list.contains("units/nightelf/archer/archer.mdx"));
        assert!(!list.contains("Units\\NightElf\\Archer\\Archer"));
    }

    #[test]
    fn model_resolution_accepts_mdl_alias() {
        let list =
            Listfile::parse("test", "Objects\\InventoryItems\\TreasureChest\\treasurechest.mdx\n");
        assert_eq!(
            list.resolve_model("Objects\\InventoryItems\\TreasureChest\\treasurechest.mdl")
                .as_deref(),
            Some("objects/inventoryitems/treasurechest/treasurechest.mdx")
        );
    }

    #[test]
    fn model_resolution_completes_missing_extension() {
        let list = Listfile::parse("test", "Units\\NightElf\\Archer\\Archer.mdx\n");
        assert_eq!(
            list.resolve_model("Units\\NightElf\\Archer\\Archer").as_deref(),
            Some("units/nightelf/archer/archer.mdx")
        );
        assert_eq!(list.resolve_model("Units\\Nothing").as_deref(), None);
    }

    #[test]
    fn counts_by_prefix_and_extension() {
        let list = Listfile::parse(
            "test",
            "Buildings\\Human\\TownHall\\TownHall.mdx\nBuildings\\Human\\Keep\\Keep.mdx\n\
             ReplaceableTextures\\CommandButtons\\BTNPeasant.blp\n",
        );
        assert_eq!(list.count("Buildings\\", ".mdx"), 2);
        assert_eq!(list.count("Buildings", ".mdx"), 2);
        assert_eq!(list.count("ReplaceableTextures\\CommandButtons\\", ".blp"), 1);
    }

    #[test]
    fn memory_source_serves_paths() {
        let mut source = MemorySource::new();
        source.insert("Units\\UnitData.slk", b"ID;PWXL;N;E".to_vec());
        assert!(source.exists("units/unitdata.slk"));
        assert_eq!(source.read("UNITS\\UNITDATA.SLK").unwrap(), b"ID;PWXL;N;E");
        assert!(source.read("Units\\Nope.slk").is_err());
    }
}
