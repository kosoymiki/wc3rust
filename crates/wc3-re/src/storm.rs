//! Storm/MPQ virtual filesystem.
//!
//! ## Byte layout (as read from the local archives)
//!
//! `MPQ\x1a` header (32 bytes for version 0, verified on `War3.mpq`,
//! `War3x.mpq`, `War3xlocal.mpq`, `setup.mpq` and every `.w3m`/`.w3x` map):
//!
//! ```text
//! 0x00  char[4] "MPQ\x1a"
//! 0x04  u32     header size (0x20 for v0)
//! 0x08  u32     archive size
//! 0x0C  u16     format version (0 for classic WC3; 1 for Burning Crusade+)
//! 0x0E  u16     sector size shift (512 << shift; WC3 uses 3 => 4096)
//! 0x10  u32     hash table offset   (relative to `base`)
//! 0x14  u32     block table offset  (relative to `base`)
//! 0x18  u32     hash table entries  (power of two)
//! 0x1C  u32     block table entries
//! ```
//!
//! The hash table (16 bytes/entry: name1, name2, locale, block_index) and the
//! block table (16 bytes/entry: file_offset, compressed_size, size, flags) are
//! encrypted with the WC3 crypt table; the hash probe walks entries with a
//! linear probe until an empty slot (`block_index == 0xFFFFFFFF`). This is the
//! exact algorithm reproduced by [`wc3_mpq`] and validated against the local
//! archives (see `wc3-mpq` tests).
//!
//! File flags that matter to the asset layer:
//! `0x80000000` exists, `0x00010000` encrypted, `0x00020000` FIX_KEY,
//! `0x01000000` single unit, `0x00000200` compressed (codec mask in bits
//! 8..16: `0x01` huffman, `0x02` zlib, `0x08` PKWARE, `0x10` bzip2, `0x12`
//! LZMA, `0x40`/`0x80` ADPCM mono/stereo). The sector offset table (one u32 per
//! sector + one for the end) is present only for compressed multi-sector files.
//!
//! This module provides the *multi-archive* view the game uses: patch archives
//! (highest priority) → expansion → base, case-insensitive paths with `/` and
//! `\` interchangeable, plus an optional loose-file tree (`analysis/war3_full3`
//! in this repository) that behaves like the extracted install.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use wc3_mpq::{CodecUsage, FileMeta, MpqArchive};

use crate::error::{ReError, Result};

pub const MPQ_MAGIC: &[u8; 4] = b"MPQ\x1a";
/// RoC campaign maps carry this 512-byte header before the MPQ payload.
pub const W3M_MAGIC: &[u8; 4] = b"HM3W";

/// Canonical archives of a WC3 install, highest priority first.
///
/// `War3x.mpq` is the TFT data archive and `War3xlocal.mpq` its localisation
/// layer (unit/item/ability strings and voice sets); both override the RoC
/// base `War3.mpq`. `War3Patch.mpq`/`patch.mpq` override everything, and
/// `setup.mpq` (the installer payload) is consulted last.
pub const WC3_INSTALL_ORDER: [&str; 6] = [
    "War3Patch.mpq",
    "patch.mpq",
    "War3xlocal.mpq",
    "War3x.mpq",
    "War3.mpq",
    "setup.mpq",
];

/// Case-insensitive lookup of one canonical archive in `root`.
///
/// Retail installs are case-insensitive; Android data dirs are not, so an
/// extraction that wrote `war3x.mpq` must still be found.
fn find_install_archive(root: &Path, name: &str) -> Option<PathBuf> {
    let direct = root.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    let wanted = name.to_ascii_lowercase();
    let mut matches: Vec<PathBuf> = fs::read_dir(root)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .map(|file| file.to_string_lossy().to_ascii_lowercase() == wanted)
                    .unwrap_or(false)
        })
        .collect();
    matches.sort();
    matches.into_iter().next()
}

/// Every `War3*.mpq` in `root` that is not already mounted, sorted by name.
fn extra_war3_archives(root: &Path, mounted: &[PathBuf]) -> Vec<PathBuf> {
    let mut extra: Vec<PathBuf> = fs::read_dir(root)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .map(|file| {
                        let file = file.to_string_lossy().to_ascii_lowercase();
                        file.starts_with("war3") && file.ends_with(".mpq")
                    })
                    .unwrap_or(false)
        })
        .filter(|path| !mounted.iter().any(|mounted| mounted == path))
        .collect();
    extra.sort();
    extra
}

/// Canonical archive path spelling used by the game (`\` separators, no leading
/// slash). The MPQ hash itself is case-insensitive, so casing is preserved.
pub fn normalize_path(path: &str) -> String {
    path.trim()
        .trim_start_matches(['\\', '/'])
        .replace('/', "\\")
}

/// Decoded per-file metadata, including the declared codec mask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub archive: String,
    pub size: u32,
    pub compressed_size: u32,
    pub flags: u32,
    pub mask: u8,
    pub encrypted: bool,
    pub fix_key: bool,
    pub single_unit: bool,
    pub compressed: bool,
    pub codecs: String,
}

impl FileInfo {
    fn from_meta(archive: &str, meta: FileMeta) -> Self {
        Self {
            archive: archive.to_string(),
            size: meta.size,
            compressed_size: meta.compressed_size,
            flags: meta.flags,
            mask: meta.mask,
            encrypted: meta.is_encrypted(),
            fix_key: meta.flags & 0x0002_0000 != 0,
            single_unit: meta.is_single_unit(),
            compressed: meta.is_compressed(),
            codecs: meta.declared_usage().describe(),
        }
    }
}

/// One opened archive layer.
#[derive(Debug)]
pub struct StormLayer {
    pub label: String,
    pub path: PathBuf,
    pub archive: MpqArchive,
}

/// Multi-archive Storm filesystem.
#[derive(Debug)]
pub struct StormFs {
    layers: Vec<StormLayer>,
    /// Loose directories, with a lowercase relative-path index for
    /// case-insensitive lookup matching the archive behaviour.
    loose: Vec<LooseRoot>,
}

#[derive(Debug)]
struct LooseRoot {
    label: String,
    root: PathBuf,
    index: HashMap<String, PathBuf>,
}

impl StormFs {
    pub fn new() -> Self {
        Self {
            layers: Vec::new(),
            loose: Vec::new(),
        }
    }

    /// Open an archive and append it at the *lowest* priority. Callers that
    /// want patch precedence must add the patch first or use
    /// [`StormFs::open_wc3_install`].
    pub fn add_archive(&mut self, label: impl Into<String>, path: impl AsRef<Path>) -> Result<usize> {
        let path = path.as_ref().to_path_buf();
        let base = detect_mpq_base(&path)?;
        let archive = MpqArchive::open_at(&path, base)?;
        let count = archive.listfile().len();
        self.layers.push(StormLayer {
            label: label.into(),
            path,
            archive,
        });
        Ok(count)
    }

    /// Add a loose directory (extracted install). Files are indexed once,
    /// lowercase, with `\` separators; the first root wins.
    pub fn add_loose_dir(&mut self, label: impl Into<String>, root: impl AsRef<Path>) -> Result<usize> {
        let root = root.as_ref().to_path_buf();
        let mut index = HashMap::new();
        index_loose(&root, &root, &mut index)?;
        let count = index.len();
        self.loose.push(LooseRoot {
            label: label.into(),
            root,
            index,
        });
        Ok(count)
    }

    /// Open the standard WC3 install layout in game priority order.
    ///
    /// Priority (highest first) follows the classic loader: a `patch.mpq` /
    /// `War3Patch.mpq` overrides the expansion archives, which override the
    /// base `War3.mpq`. Locale archives (`War3xlocal.mpq`) are consulted after
    /// the expansion archive for files missing from it.
    pub fn open_wc3_install(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        let mut fs = Self::new();
        let mut mounted: Vec<PathBuf> = Vec::new();
        for candidate in WC3_INSTALL_ORDER {
            let Some(path) = find_install_archive(root, candidate) else {
                continue;
            };
            fs.add_archive(candidate, &path)?;
            mounted.push(path);
        }
        // Some installs ship archives outside the canonical names (a renamed
        // extraction, `War3x.local.mpq`, a second expansion build). Mount every
        // remaining `War3*.mpq` above nothing but below the canonical layers so
        // an expansion/localisation archive is never silently ignored.
        //
        // Only an archive install qualifies: a loose extracted tree may carry
        // asset packs named `War3_*.mpq` (`War3_low.mpq`, `War3_med.mpq`), and
        // mounting one would shadow the loose tree the caller mounts instead.
        // An extra archive that fails to open is skipped: it must not make the
        // whole install unreadable.
        if !mounted.is_empty() {
            for path in extra_war3_archives(root, &mounted) {
                let label = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string());
                let _ = fs.add_archive(label, &path);
            }
        }
        Ok(fs)
    }

    pub fn layers(&self) -> &[StormLayer] {
        &self.layers
    }

    /// True if any archive or loose root exposes the path.
    pub fn exists(&self, path: &str) -> bool {
        self.info(path).is_some() || self.find_loose(path).is_some()
    }

    /// Per-file archive metadata (first archive in priority order that has it).
    pub fn info(&self, path: &str) -> Option<FileInfo> {
        let normalized = normalize_path(path);
        for layer in &self.layers {
            if let Some((_, meta)) = layer.archive.meta(&normalized) {
                return Some(FileInfo::from_meta(&layer.label, meta));
            }
        }
        None
    }

    /// Read a file. Archives win over loose roots; within archives the first
    /// added layer wins.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let normalized = normalize_path(path);
        for layer in &self.layers {
            match layer.archive.read(&normalized) {
                Ok(bytes) => return Ok(bytes),
                Err(wc3_mpq::MpqError::FileNotFound(_)) => continue,
                Err(err) => return Err(ReError::Mpq(err)),
            }
        }
        if let Some(file) = self.find_loose(&normalized) {
            return Ok(fs::read(file)?);
        }
        Err(ReError::NotFound(normalized))
    }

    /// Read a file and return the observed codec tags (archives only).
    pub fn read_with_codecs(&self, path: &str) -> Result<(Vec<u8>, String)> {
        let normalized = normalize_path(path);
        for layer in &self.layers {
            match layer.archive.read_with_info(&normalized) {
                Ok((bytes, usage)) => return Ok((bytes, describe_usage(usage))),
                Err(wc3_mpq::MpqError::FileNotFound(_)) => continue,
                Err(err) => return Err(ReError::Mpq(err)),
            }
        }
        let bytes = self.read(&normalized)?;
        Ok((bytes, "stored".to_string()))
    }

    /// Union of every archive listfile, deduplicated case-insensitively.
    pub fn listfile(&self) -> Vec<String> {
        let mut seen = HashMap::new();
        for layer in &self.layers {
            for name in layer.archive.listfile() {
                seen.entry(name.to_ascii_lowercase()).or_insert_with(|| name.clone());
            }
        }
        for root in &self.loose {
            for key in root.index.keys() {
                seen.entry(key.clone()).or_insert_with(|| key.clone());
            }
        }
        let mut out: Vec<String> = seen.into_values().collect();
        out.sort();
        out
    }

    /// Case-insensitive directory/file listing under `prefix`.
    pub fn search(&self, prefix: &str, suffix: &str, limit: usize) -> Vec<String> {
        let prefix = normalize_path(prefix).to_ascii_lowercase();
        let suffix = suffix.to_ascii_lowercase();
        let mut out = Vec::new();
        for name in self.listfile() {
            let lower = name.to_ascii_lowercase();
            if lower.starts_with(&prefix) && lower.ends_with(&suffix) {
                out.push(name);
                if out.len() >= limit {
                    break;
                }
            }
        }
        out
    }

    /// Total number of files visible through archives (sum, not deduplicated).
    pub fn archive_file_counts(&self) -> Vec<(String, usize, usize)> {
        self.layers
            .iter()
            .map(|layer| {
                (
                    layer.label.clone(),
                    layer.archive.listfile().len(),
                    layer.archive.block_count(),
                )
            })
            .collect()
    }

    fn find_loose(&self, path: &str) -> Option<PathBuf> {
        let key = normalize_path(path).to_ascii_lowercase();
        self.loose
            .iter()
            .find_map(|root| root.index.get(&key).cloned())
    }

    /// Loose root that contains `path` (used by the scanners for fast stat).
    pub fn loose_root_for(&self, path: &str) -> Option<&Path> {
        let key = normalize_path(path).to_ascii_lowercase();
        self.loose
            .iter()
            .find(|root| root.index.contains_key(&key))
            .map(|root| root.root.as_path())
    }
}

impl Default for StormFs {
    fn default() -> Self {
        Self::new()
    }
}

fn detect_mpq_base(path: &Path) -> Result<u64> {
    // Single container-magic implementation shared with `wc3-formats::map`
    // (HM3W prefix scanning included).
    wc3_formats::map::map_mpq_base(path)
        .map_err(|err| ReError::Parse(format!("{}: {err}", path.display())))
}

fn index_loose(root: &Path, base: &Path, index: &mut HashMap<String, PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            index_loose(&path, base, index)?;
        } else if file_type.is_file() {
            let rel = path.strip_prefix(base).unwrap_or(&path);
            let key = normalize_path(&rel.to_string_lossy()).to_ascii_lowercase();
            index.entry(key).or_insert(path);
        }
    }
    Ok(())
}

fn describe_usage(usage: CodecUsage) -> String {
    usage.describe()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_windows_and_posix_separators() {
        assert_eq!(normalize_path("/Units/UnitData.slk"), "Units\\UnitData.slk");
        assert_eq!(normalize_path("\\Scripts\\common.j"), "Scripts\\common.j");
        assert_eq!(normalize_path("UI/FrameDef\\A.fdf"), "UI\\FrameDef\\A.fdf");
    }

    #[test]
    fn opens_real_war3_archive_when_present() {        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis");
        let path = root.join("war3_data/War3.mpq");
        if !path.is_file() {
            eprintln!("skip: {} missing", path.display());
            return;
        }
        let mut fs = StormFs::new();
        fs.add_archive("War3.mpq", &path).expect("open War3.mpq");
        let html = fs.read("(listfile)").expect("listfile present");
        assert!(html.len() > 100);
        let info = fs.info("(listfile)").expect("metadata");
        assert!(info.size > 0);
        assert!(info.codecs.contains("zlib") || info.codecs.contains("stored"));
    }

    /// Android data dirs are case-sensitive, so the canonical install lookup
    /// must match `war3x.mpq` and also mount `War3*.mpq` files the canonical
    /// list does not name. Uses empty files: discovery only, no archive opened.
    #[test]
    fn install_discovery_is_case_insensitive_and_finds_extra_war3_archives() {
        let dir = std::env::temp_dir().join(format!("h2-storm-discovery-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        for name in ["war3.mpq", "WAR3XLOCAL.MPQ", "War3x.mpq", "War3xExtra.mpq", "readme.txt"] {
            fs::write(dir.join(name), b"x").expect("touch");
        }
        assert_eq!(
            find_install_archive(&dir, "War3xlocal.mpq")
                .and_then(|path| path.file_name().map(|name| name.to_string_lossy().into_owned())),
            Some("WAR3XLOCAL.MPQ".to_string())
        );
        let mounted = vec![dir.join("war3.mpq"), dir.join("WAR3XLOCAL.MPQ")];
        let extra: Vec<String> = extra_war3_archives(&dir, &mounted)
            .into_iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(extra, vec!["War3x.mpq".to_string(), "War3xExtra.mpq".to_string()]);
        fs::remove_dir_all(&dir).ok();
    }

    /// A loose extracted tree carries LOD asset packs named `War3_low.mpq` /
    /// `War3_med.mpq`. They are not install archives; mounting one would make
    /// `open_wc3_install` report a layer and the caller would skip the loose
    /// tree, losing every sheet (`Units\UnitData.slk` -> NotFound).
    #[test]
    fn loose_tree_asset_packs_are_not_mounted_as_install_archives() {
        let dir = std::env::temp_dir().join(format!("h2-storm-loose-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join("War3_low.mpq"), b"not an mpq").expect("low pack");
        fs::write(dir.join("War3_med.mpq"), b"not an mpq").expect("med pack");
        let fs = StormFs::open_wc3_install(&dir).expect("open loose root");
        assert!(fs.layers().is_empty(), "asset packs are not install layers");
        fs::remove_dir_all(&dir).ok();
    }
}
