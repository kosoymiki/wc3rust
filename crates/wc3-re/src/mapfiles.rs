//! W3M/W3X map container and inner-file access.
//!
//! RoC `.w3m` files start with a 512-byte `HM3W` header; the MPQ archive starts
//! at the first `MPQ\x1a` signature inside the first 4 KiB (observed on all 44
//! local maps). TFT `.w3x` is a plain MPQ. The archive has no dependable
//! listfile, so the loader probes the fixed inner names the World Editor
//! writes:
//!
//! `war3map.j`, `war3map.w3e` (terrain), `war3map.w3i` (info), `war3map.wpm`
//! (pathing), `war3map.w3u/.w3t/.w3a/.w3d/.w3b/.w3h/.w3q` (object data),
//! `war3map.wts` (strings), `war3map.wtg` (GUI triggers), `war3map.wct`
//! (custom text), `war3map.w3r` (regions), `war3map.w3c` (cameras),
//! `war3map.w3s` (sounds), `war3map.doo` (doodads), `war3mapUnits.doo` (units),
//! `war3map.mmp` (minimap), `war3map.shd` (shadow map), `war3map.imp`
//! (imports), `war3mapMap.blp` (minimap image), `war3mapPreview.tga`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::mapobjects::MapObjectData;
use wc3_formats::w3e::Terrain;
use wc3_formats::w3i::MapInfo;
use wc3_formats::wpm::PathingMap;
use wc3_mpq::MpqArchive;

use crate::error::{ReError, Result};
use crate::storm::normalize_path;
use crate::wts::Wts;

/// Every inner file the retail editor can write, in load order.
pub const MAP_INNER_FILES: &[&str] = &[
    "war3map.j",
    "scripts\\war3map.j",
    "war3map.w3e",
    "war3map.w3i",
    "war3map.wpm",
    "war3map.w3u",
    "war3map.w3t",
    "war3map.w3a",
    "war3map.w3d",
    "war3map.w3b",
    "war3map.w3h",
    "war3map.w3q",
    "war3map.wts",
    "war3map.wtg",
    "war3map.wct",
    "war3map.w3r",
    "war3map.w3c",
    "war3map.w3s",
    "war3map.doo",
    "war3mapUnits.doo",
    "war3map.mmp",
    "war3map.shd",
    "war3map.imp",
    "war3mapMap.blp",
    "war3mapPreview.tga",
];

#[derive(Debug, Clone, Default)]
pub struct InnerFile {
    pub name: String,
    pub size: usize,
}

#[derive(Default)]
pub struct MapBundle {
    pub path: PathBuf,
    pub mpq_base: u64,
    pub entries: Vec<InnerFile>,
    pub listfile: Vec<String>,
    pub script: Option<String>,
    pub terrain: Option<Terrain>,
    pub info: Option<MapInfo>,
    pub pathing: Option<PathingMap>,
    pub wts: Option<Wts>,
    pub objects: BTreeMap<String, MapObjectData>,
    /// Parsed `war3map.doo` placement records (canonical parser in wc3-formats).
    pub doodads: Option<Vec<wc3_formats::map::PlacedDoodad>>,
    /// Parsed `war3mapUnits.doo` placement records (same parser the removed
    /// `W3Map` used, now owned by this bundle).
    pub units: Option<Vec<wc3_formats::map::PlacedUnit>>,
    pub errors: Vec<(String, String)>,
}

impl MapBundle {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let base = wc3_formats::map::map_mpq_base(&path)
            .map_err(|err| ReError::Parse(format!("map container: {err}")))?;
        let archive = MpqArchive::open_at(&path, base)?;
        let mut bundle = Self {
            path,
            mpq_base: base,
            listfile: archive.listfile().to_vec(),
            ..Self::default()
        };
        let mut names: Vec<String> = MAP_INNER_FILES.iter().map(|s| s.to_string()).collect();
        for name in &bundle.listfile {
            if !names.iter().any(|known| known.eq_ignore_ascii_case(name)) {
                names.push(name.clone());
            }
        }
        for name in names {
            let normalized = normalize_path(&name);
            match archive.read(&normalized) {
                Ok(bytes) => {
                    bundle.entries.push(InnerFile {
                        name: normalized.clone(),
                        size: bytes.len(),
                    });
                    bundle.parse_inner(&normalized, &bytes);
                }
                Err(wc3_mpq::MpqError::FileNotFound(_)) => {}
                Err(err) => bundle.errors.push((normalized, err.to_string())),
            }
        }
        Ok(bundle)
    }

    fn parse_inner(&mut self, name: &str, bytes: &[u8]) {
        let lower = name.to_ascii_lowercase();
        let result: Result<()> = (|| {
            match lower.as_str() {
                "war3map.j" | "scripts\\war3map.j" => {
                    if self.script.is_none() {
                        self.script = Some(String::from_utf8_lossy(bytes).into_owned());
                    }
                }
                "war3map.w3e" => self.terrain = Some(Terrain::parse(bytes)?),
                "war3map.w3i" => self.info = Some(MapInfo::parse(bytes)?),
                "war3map.wpm" => self.pathing = Some(PathingMap::parse(bytes)?),
                "war3map.wts" => self.wts = Some(Wts::parse(bytes)?),
                "war3mapunits.doo" => match wc3_formats::map::parse_units(bytes) {
                    Ok(units) => self.units = Some(units),
                    // Unit placement is optional metadata: v8 random-unit
                    // tables and the truncated BonusCredits variant are known
                    // canonical-parser gaps (W3Map treats the former the same
                    // way). Keep the rest of the map valid and scan at 0 errors.
                    Err(wc3_formats::map::MapError::Unsupported(_)) => {
                        self.units = None;
                    }
                    Err(err) => {
                        return Err(ReError::Parse(format!("war3mapUnits.doo: {err}")));
                    }
                },
                "war3map.doo" => {
                    self.doodads = Some(
                        wc3_formats::map::parse_doodads(bytes)
                            .map_err(|err| ReError::Parse(format!("war3map.doo: {err}")))?,
                    )
                }
                "war3map.w3u" | "war3map.w3t" | "war3map.w3a" | "war3map.w3d"
                | "war3map.w3b" | "war3map.w3h" | "war3map.w3q" => {
                    let kind = match lower.as_str() {
                        "war3map.w3u" => wc3_formats::gamedata::ObjectKind::Unit,
                        "war3map.w3t" => wc3_formats::gamedata::ObjectKind::Item,
                        "war3map.w3a" => wc3_formats::gamedata::ObjectKind::Ability,
                        "war3map.w3d" => wc3_formats::gamedata::ObjectKind::Doodad,
                        "war3map.w3b" => wc3_formats::gamedata::ObjectKind::Destructable,
                        "war3map.w3h" => wc3_formats::gamedata::ObjectKind::Buff,
                        _ => wc3_formats::gamedata::ObjectKind::Upgrade,
                    };
                    self.objects
                        .insert(lower.clone(), MapObjectData::parse_as(kind, bytes)?);
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(err) = result {
            self.errors.push((name.to_string(), err.to_string()));
        }
    }

    pub fn has(&self, name: &str) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.name.eq_ignore_ascii_case(name))
    }

    pub fn entry(&self, name: &str) -> Option<&InnerFile> {
        self.entries
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
    }

    /// Resolve every `TRIGSTR_n` in map info strings with the map's WTS.
    pub fn localized_info(&self) -> Option<MapInfo> {
        let mut info = self.info.clone()?;
        if let Some(wts) = &self.wts {
            info.name = wts.resolve(&info.name);
            info.author = wts.resolve(&info.author);
            info.description = wts.resolve(&info.description);
            info.recommended_players = wts.resolve(&info.recommended_players);
        }
        Some(info)
    }

    pub fn object_entries(&self) -> usize {
        self.objects.values().map(|data| data.entry_count()).sum()
    }
}

/// One-line audit used by `wc3re scan`.
#[derive(Debug, Clone)]
pub struct MapAudit {
    pub path: PathBuf,
    pub entries: usize,
    pub script_bytes: usize,
    pub terrain_tiles: usize,
    pub object_entries: usize,
    pub wts_entries: usize,
    pub errors: Vec<String>,
}

impl MapAudit {
    pub fn from_bundle(bundle: &MapBundle) -> Self {
        Self {
            path: bundle.path.clone(),
            entries: bundle.entries.len(),
            script_bytes: bundle.script.as_ref().map_or(0, String::len),
            terrain_tiles: bundle.terrain.as_ref().map_or(0, |terrain| terrain.tiles.len()),
            object_entries: bundle.object_entries(),
            wts_entries: bundle.wts.as_ref().map_or(0, |wts| wts.entries.len()),
            errors: bundle
                .errors
                .iter()
                .map(|(name, err)| format!("{name}: {err}"))
                .collect(),
        }
    }
}

/// Deepest directory nesting map discovery follows.
///
/// A real WC3 install nests maps three or four levels (`Maps/Campaign/Human/`),
/// so 32 is generous for the product path and turns a pathological link farm
/// into a bounded partial result.
const MAX_DISCOVERY_DEPTH: usize = 32;
/// Most directory entries one discovery call inspects. Bounds the wall time of
/// a hostile tree independently of the depth limit.
const MAX_DISCOVERY_ENTRIES: usize = 200_000;

fn is_map_path(path: &Path) -> bool {
    path.extension().is_some_and(|ext| {
        matches!(
            ext.to_string_lossy().to_ascii_lowercase().as_str(),
            "w3m" | "w3x" | "w3n"
        )
    })
}

/// Recursively find every `.w3m`/`.w3x`/`.w3n` map under `root`.
///
/// Symlink-cycle safe (WC3-I-020). `Path::is_dir()` follows symlinks, so a link
/// to a parent directory used to recurse until the stack or the user gave out.
/// Every directory is now keyed by its **canonical** path in a visited set
/// before descending: a link to an already-walked directory (parent, sibling, a
/// two-directory cycle, or a self-link) is skipped instead of re-scanned.
/// Symlinked *files* are still matched by extension. Unreadable directories
/// (permission denied, a vanished mount, a dangling link) are skipped rather
/// than failing the whole discovery. Depth and entry budgets cap the walk even
/// when every directory is unique.
pub fn discover_maps(root: &Path) -> Result<Vec<PathBuf>> {
    use std::collections::HashSet;

    fn walk(
        dir: &Path,
        depth: usize,
        visited: &mut HashSet<PathBuf>,
    ) -> Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        if depth > MAX_DISCOVERY_DEPTH {
            return Ok(out);
        }
        // The canonical path is the identity key: a symlink and its target (and
        // two links that point at each other) collapse to one entry.
        let Ok(key) = fs::canonicalize(dir) else {
            return Ok(out);
        };
        if !visited.insert(key) {
            return Ok(out);
        }
        let Ok(reader) = fs::read_dir(dir) else {
            return Ok(out);
        };
        for entry in reader.flatten() {
            if visited.len() > MAX_DISCOVERY_ENTRIES {
                break;
            }
            let path = entry.path();
            // `symlink_metadata` does not follow the link; the target is read
            // only through `metadata`, and only to decide dir/file.
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            let is_link = meta.file_type().is_symlink();
            let target = if is_link {
                match fs::metadata(&path) {
                    Ok(target) => Some(target),
                    Err(_) => continue, // dangling link
                }
            } else {
                None
            };
            let is_dir = target.as_ref().map(|meta| meta.is_dir()).unwrap_or_else(|| meta.is_dir());
            if is_dir {
                out.extend(walk(&path, depth + 1, visited)?);
            } else if is_map_path(&path) {
                out.push(path);
            }
        }
        Ok(out)
    }

    let mut visited = HashSet::new();
    let mut out = walk(root, 0, &mut visited)?;
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_real_roc_campaign_map() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/Maps/Campaign/Human04.w3m");
        if !path.is_file() {
            eprintln!("skip: Human04.w3m missing");
            return;
        }
        let bundle = MapBundle::open(&path).expect("open map");
        assert!(bundle.script.as_ref().is_some_and(|s| s.contains("function main")));
        assert!(bundle.terrain.is_some());
        let wts = bundle.wts.as_ref().expect("war3map.wts");
        assert!(wts.entries.len() > 50, "wts entries {}", wts.entries.len());
        assert!(wts.entries.values().any(|value| value.contains("Андорал")));
        assert!(bundle.errors.is_empty(), "{:?}", bundle.errors);
    }
}
