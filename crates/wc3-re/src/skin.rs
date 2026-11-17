//! UI skin resolution (`UI\war3skins.txt` + FDF `DecorateFileNames`).
//!
//! `war3skins.txt` is a `[Section] key=value` table:
//! `[Main] Skins=Orc,Human,NightElf,Undead` lists the race skins; each race
//! section maps logical UI names to archives paths (`Console=UI\Console\Orc\
//! OrcUi.mdl`, `ConsoleTexture01=...`, icon overrides, music, fonts). `[Default]`
//! holds the fallback for every key a race section does not define. Values that
//! list multiple entries use `;` (music playlists).
//!
//! FDF frames carry `DecorateFileNames,`; for those frames a texture string is
//! not a literal path but a skin key. Resolution order is exactly:
//! requested skin section → `[Default]` → literal path (if the string looks
//! like a path). This is how the same FDF renders a Human, Orc, Night Elf or
//! Undead console without editing the frame definitions.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::error::Result;
use crate::txt::TextTable;

#[derive(Debug, Clone, Default)]
pub struct SkinSection {
    pub name: String,
    pub entries: BTreeMap<String, String>,
}

impl SkinSection {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Clone, Default)]
pub struct SkinTable {
    pub skins: Vec<String>,
    pub sections: BTreeMap<String, SkinSection>,
}

impl SkinTable {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path)?;
        Self::parse(&bytes)
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let table = TextTable::parse("war3skins.txt", bytes)?;
        let mut out = Self::default();
        for section in &table.sections {
            let mut entries = BTreeMap::new();
            for (key, value) in &section.entries {
                entries.insert(key.clone(), value.clone());
            }
            if section.name.eq_ignore_ascii_case("Main") {
                if let Some(skins) = entries.get("Skins") {
                    out.skins = skins
                        .split(',')
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(str::to_string)
                        .collect();
                }
                continue;
            }
            out.sections.insert(
                section.name.clone(),
                SkinSection {
                    name: section.name.clone(),
                    entries,
                },
            );
        }
        Ok(out)
    }

    pub fn section(&self, skin: &str) -> Option<&SkinSection> {
        self.sections
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(skin))
            .map(|(_, section)| section)
    }

    /// Race skin → `[Default]` fallback.
    pub fn resolve(&self, skin: &str, key: &str) -> Option<&str> {
        self.section(skin)
            .and_then(|section| section.get(key))
            .or_else(|| self.section("Default").and_then(|section| section.get(key)))
    }

    /// Semicolon-separated playlist/value list with the same fallback.
    pub fn resolve_list(&self, skin: &str, key: &str) -> Vec<&str> {
        self.resolve(skin, key)
            .map(|value| {
                value
                    .split(';')
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// FDF `DecorateFileNames` resolution: a skin key wins; otherwise the
    /// literal string is returned unchanged.
    pub fn decorate(&self, skin: &str, name: &str) -> String {
        self.resolve(skin, name)
            .map(str::to_string)
            .unwrap_or_else(|| name.to_string())
    }

    /// All keys that point at a texture archive path.
    pub fn texture_entries(&self) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        for (section_name, section) in &self.sections {
            for (key, value) in &section.entries {
                if value.to_ascii_lowercase().ends_with(".blp")
                    || value.to_ascii_lowercase().ends_with(".mdl")
                    || value.to_ascii_lowercase().ends_with(".tga")
                {
                    out.push((section_name.clone(), key.clone(), value.clone()));
                }
            }
        }
        out.sort();
        out
    }
}

/// Audit `DecorateFileNames` usage in the FDF corpus: how many frames use it
/// and how many texture strings resolve through `war3skins`.
#[derive(Debug, Clone, Default)]
pub struct DecorateAudit {
    pub documents: usize,
    pub decorate_nodes: usize,
    pub texture_strings: usize,
    pub resolved_keys: usize,
    pub sample: Vec<(String, String)>,
}

pub fn audit_fdf_decorate(fdf_dir: &Path, skins: &SkinTable, skin: &str) -> DecorateAudit {
    let mut audit = DecorateAudit::default();
    let Ok(entries) = fs::read_dir(fdf_dir) else {
        return audit;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "fdf") {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let Ok(document) = wc3_formats::fdf::FdfDocument::parse_named(&text, &path.display().to_string())
        else {
            continue;
        };
        audit.documents += 1;
        document.walk(&mut |node| {
            if !node.flag("DecorateFileNames") {
                return;
            }
            audit.decorate_nodes += 1;
            for prop in &node.props {
                for value in prop.strings() {
                    if !value.to_ascii_lowercase().ends_with(".blp")
                        && !value.to_ascii_lowercase().ends_with(".tga")
                    {
                        continue;
                    }
                    audit.texture_strings += 1;
                    let key = value.rsplit('\\').next().unwrap_or(value);
                    let key = key.rsplit('/').next().unwrap_or(key);
                    if skins.resolve(skin, key).is_some() {
                        audit.resolved_keys += 1;
                        if audit.sample.len() < 20 {
                            audit.sample.push((key.to_string(), skins.decorate(skin, key)));
                        }
                    }
                }
            }
        });
    }
    audit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_race_skin_and_default() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/UI/war3skins.txt");
        let skins = SkinTable::load(&path).expect("load skins");
        assert!(skins.skins.contains(&"Human".to_string()));
        assert_eq!(
            skins.resolve("Human", "CommandBasicStruct"),
            Some("ReplaceableTextures\\CommandButtons\\BTNHumanBuild.blp")
        );
        assert!(skins.resolve("Human", "ConsoleBackground").is_some());
        assert!(skins.resolve("NoSuchRace", "MasterFont").is_some());
        assert!(!skins.texture_entries().is_empty());
    }
}
