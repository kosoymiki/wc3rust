//! Typed catalog: abilities (executable classification) + the full
//! unit/weapon/item/upgrade/effect/buff definition tables.
//!
//! `wc3-data` owns the canonical SLK/profile interpretation; this module is the
//! RE-layer entry point that combines it with the RE ability table so callers
//! get every decompiled row in one typed value. No row is dropped: `Defs`
//! reports `unparsed` with family/id/reason.

use std::path::{Path, PathBuf};

use wc3_data::source::{DirSource, Source};
use wc3_data::{Defs, GameDatabase};

use crate::abilities::{AbilityDef, defs_for_root};
use crate::error::{ReError, Result};

#[derive(Debug, Clone)]
pub struct Catalog {
    pub abilities: Vec<AbilityDef>,
    pub defs: Defs,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogSummary {
    pub abilities: usize,
    pub units: usize,
    pub weapons: usize,
    pub items: usize,
    pub upgrades: usize,
    pub effects: usize,
    pub buffs: usize,
    pub unparsed: usize,
}

impl CatalogSummary {
    pub fn from_catalog(catalog: &Catalog) -> Self {
        Self {
            abilities: catalog.abilities.len(),
            units: catalog.defs.units.len(),
            weapons: catalog.defs.weapons.len(),
            items: catalog.defs.items.len(),
            upgrades: catalog.defs.upgrades.len(),
            effects: catalog.defs.effects.len(),
            buffs: catalog.defs.buffs.len(),
            unparsed: catalog.defs.unparsed.len(),
        }
    }
}

pub(crate) fn data_root(root: &Path) -> Result<PathBuf> {
    [
        root.join("analysis/war3x_data"),
        root.join("analysis/war3_full3"),
    ]
    .into_iter()
    .find(|path| path.join("Units/UnitData.slk").is_file())
    .ok_or_else(|| ReError::NotFound("Units/UnitData.slk in war3x_data".into()))
}

/// Read tables from the TFT data root but strings/profiles from the local
/// (localized) tree; the extracted data root alone has no `*Strings.txt`.
struct InstallSource {
    dirs: Vec<DirSource>,
}

impl Source for InstallSource {
    fn read(&self, path: &str) -> std::result::Result<Vec<u8>, wc3_data::DataError> {
        for dir in &self.dirs {
            match dir.read(path) {
                Ok(bytes) => {
                    if path.to_ascii_lowercase().ends_with(".txt")
                        && std::str::from_utf8(&bytes).is_err()
                    {
                        return Ok(String::from_utf8_lossy(&bytes).into_owned().into_bytes());
                    }
                    return Ok(bytes);
                }
                Err(wc3_data::DataError::Missing { .. }) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(wc3_data::DataError::Missing {
            path: path.to_string(),
            root: self.dirs.first().map_or_else(PathBuf::new, |dir| dir.root().to_path_buf()),
        })
    }

    fn exists(&self, path: &str) -> bool {
        self.dirs.iter().any(|dir| dir.exists(path))
    }

    fn describe(&self) -> String {
        self.dirs
            .iter()
            .map(DirSource::describe)
            .collect::<Vec<_>>()
            .join("+")
    }
}

pub fn database(root: &Path) -> Result<GameDatabase> {
    let data = data_root(root)?;
    let mut dirs = vec![DirSource::new(data)];
    for extra in ["analysis/war3x_local", "analysis/war3_full3"] {
        let path = root.join(extra);
        if path.is_dir() {
            dirs.push(DirSource::new(path));
        }
    }
    let source = InstallSource { dirs };
    GameDatabase::load(&source).map_err(|error| ReError::Parse(format!("catalog load: {error}")))
}

/// Load every typed row of the operator's install plus the 763-row ability
/// classification.
pub fn load(root: &Path) -> Result<Catalog> {
    let db = database(root)?;
    let defs = db.defs();
    let abilities = defs_for_root(root)?;
    Ok(Catalog { abilities, defs })
}

pub fn summary_for_root(root: &Path) -> Result<CatalogSummary> {
    let catalog = load(root)?;
    Ok(CatalogSummary::from_catalog(&catalog))
}

pub fn summary() -> Result<CatalogSummary> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    summary_for_root(&root)
}

impl CatalogSummary {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"abilities\":{},\"units\":{},\"weapons\":{},\"items\":{},\"upgrades\":{},\"effects\":{},\"buffs\":{},\"unparsed\":{}}}",
            self.abilities,
            self.units,
            self.weapons,
            self.items,
            self.upgrades,
            self.effects,
            self.buffs,
            self.unparsed
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_data_rows_763_273_89_148() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let catalog = load(&root).expect("catalog");
        let summary = CatalogSummary::from_catalog(&catalog);
        assert_eq!(summary.abilities, 763, "AbilityData rows");
        assert_eq!(summary.items, 273, "ItemData rows");
        assert_eq!(summary.upgrades, 89, "UpgradeData rows");
        assert_eq!(summary.buffs, 148, "typed buff rows");
        assert_eq!(summary.units, 812, "UnitData rows");
        assert_eq!(summary.weapons, 812, "UnitWeapons rows");
        assert_eq!(summary.unparsed, 0, "unparsed rows: {:?}", catalog.defs.unparsed);
        assert!(summary.effects > 0);
        assert!(catalog
            .defs
            .items
            .iter()
            .any(|item| !item.abilities.is_empty()));
    }
}
