//! Misc/UI configuration tables (`UI\MiscData.txt`, `UI\MiscUI.txt`,
//! `Units\MiscData.txt`, `UI\WorldEditData.txt`, `UI\UnitEditorData.txt`).
//!
//! These files are INI-style key/value documents. Every section and key is
//! exposed verbatim plus typed accessors; nothing is renamed. Sections can be
//! supplied as bytes or as already-parsed layers, so a composed database can
//! hand over what it already read.

use wc3_formats::gamedata::TextTable;

use crate::error::{DataError, Result};

/// One loaded misc file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiscStat {
    pub file: String,
    pub sections: usize,
    pub keys: usize,
}

/// One parsed misc layer.
#[derive(Debug, Clone)]
struct MiscLayer {
    name: String,
    sections: Vec<(String, Vec<(String, String)>)>,
}

/// Registry of INI-style configuration files.
#[derive(Debug, Default)]
pub struct MiscTables {
    layers: Vec<MiscLayer>,
}

impl MiscTables {
    /// Files loaded by [`MiscTables::load`], in load order.
    pub const FILES: [&'static str; 5] = [
        "UI\\MiscData.txt",
        "UI\\MiscUI.txt",
        "UI\\WorldEditData.txt",
        "Units\\MiscData.txt",
        "UI\\UnitEditorData.txt",
    ];

    pub fn new() -> Self {
        Self::default()
    }

    /// Add one file from bytes (parsed by `wc3_formats::gamedata::TextTable`).
    pub fn add(&mut self, name: impl Into<String>, bytes: &[u8]) -> Result<()> {
        let name = name.into();
        let table = TextTable::parse(name.clone(), bytes)
            .map_err(|source| DataError::format(&name, source))?;
        self.add_sections(name, table.sections().to_vec());
        Ok(())
    }

    /// Add one already-parsed layer.
    pub fn add_sections(
        &mut self,
        name: impl Into<String>,
        sections: Vec<(String, Vec<(String, String)>)>,
    ) {
        self.layers.push(MiscLayer {
            name: name.into(),
            sections,
        });
    }

    /// Load every file that exists in the source (missing files are skipped).
    pub fn load(source: &dyn crate::source::Source) -> Result<Self> {
        let mut tables = Self::new();
        for name in Self::FILES {
            if source.exists(name) {
                let bytes = source.read(name)?;
                tables.add(name, &bytes)?;
            }
        }
        Ok(tables)
    }

    pub fn files(&self) -> Vec<&str> {
        self.layers.iter().map(|layer| layer.name.as_str()).collect()
    }

    pub fn stats(&self) -> Vec<MiscStat> {
        self.layers
            .iter()
            .map(|layer| MiscStat {
                file: layer.name.clone(),
                sections: layer
                    .sections
                    .iter()
                    .filter(|(name, _)| !name.eq_ignore_ascii_case("global"))
                    .count(),
                keys: layer.sections.iter().map(|(_, keys)| keys.len()).sum(),
            })
            .collect()
    }

    pub fn section_count(&self) -> usize {
        self.stats().iter().map(|stat| stat.sections).sum()
    }

    pub fn key_count(&self) -> usize {
        self.stats().iter().map(|stat| stat.keys).sum()
    }

    fn layer(&self, file: &str) -> Option<&MiscLayer> {
        self.layers
            .iter()
            .find(|layer| layer.name.eq_ignore_ascii_case(file))
    }

    fn layer_value<'a>(layer: &'a MiscLayer, section: &str, key: &str) -> Option<&'a str> {
        let (_, entries) = layer
            .sections
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(section))?;
        entries
            .iter()
            .rev()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    }

    /// Raw value from a named file.
    pub fn get(&self, file: &str, section: &str, key: &str) -> Option<&str> {
        Self::layer_value(self.layer(file)?, section, key)
    }

    /// Search every file for `section`/`key` (first hit wins).
    pub fn find(&self, section: &str, key: &str) -> Option<(&str, &str)> {
        for layer in &self.layers {
            if let Some(value) = Self::layer_value(layer, section, key) {
                return Some((layer.name.as_str(), value));
            }
        }
        None
    }

    pub fn int(&self, file: &str, section: &str, key: &str) -> Result<Option<i32>> {
        match self.get(file, section, key) {
            None => Ok(None),
            Some(raw) => raw.trim().parse::<i32>().map(Some).map_err(|_| DataError::BadCell {
                table: file.to_string(),
                row: section.to_string(),
                column: key.to_string(),
                value: raw.to_string(),
            }),
        }
    }

    pub fn real(&self, file: &str, section: &str, key: &str) -> Result<Option<f32>> {
        match self.get(file, section, key) {
            None => Ok(None),
            Some(raw) => raw.trim().parse::<f32>().map(Some).map_err(|_| DataError::BadCell {
                table: file.to_string(),
                row: section.to_string(),
                column: key.to_string(),
                value: raw.to_string(),
            }),
        }
    }

    /// Section names of a file, for enumeration in tools.
    pub fn sections(&self, file: &str) -> Vec<&str> {
        self.layer(file)
            .map(|layer| layer.sections.iter().map(|(name, _)| name.as_str()).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemorySource;

    #[test]
    fn loads_and_queries_misc_files() {
        let mut source = MemorySource::new();
        source.insert(
            "UI\\MiscData.txt",
            b"[General]\nMaxUnitLevel=15\nAllyColor=3\n".to_vec(),
        );
        source.insert(
            "Units\\MiscData.txt",
            b"[Misc]\nBuildingKillGold=1.5\n".to_vec(),
        );
        let tables = MiscTables::load(&source).expect("load");
        assert_eq!(tables.files().len(), 2);
        assert_eq!(
            tables.get("UI\\MiscData.txt", "General", "MaxUnitLevel"),
            Some("15")
        );
        assert_eq!(
            tables.int("UI\\MiscData.txt", "General", "MaxUnitLevel").unwrap(),
            Some(15)
        );
        assert_eq!(
            tables.real("Units\\MiscData.txt", "Misc", "BuildingKillGold").unwrap(),
            Some(1.5)
        );
        assert_eq!(
            tables.find("General", "AllyColor").map(|(file, _)| file),
            Some("UI\\MiscData.txt")
        );
    }

    #[test]
    fn parsed_sections_match_parsed_bytes() {
        let bytes = b"[Misc]\nGoldTextLifetime=2\n";
        let mut from_bytes = MiscTables::new();
        from_bytes.add("UI\\MiscData.txt", bytes).unwrap();
        let table = TextTable::parse("UI\\MiscData.txt", bytes).unwrap();
        let mut from_sections = MiscTables::new();
        from_sections.add_sections("UI\\MiscData.txt", table.sections().to_vec());
        assert_eq!(
            from_bytes.get("UI\\MiscData.txt", "Misc", "GoldTextLifetime"),
            from_sections.get("UI\\MiscData.txt", "Misc", "GoldTextLifetime")
        );
        assert_eq!(from_bytes.stats()[0].sections, from_sections.stats()[0].sections);
    }
}
