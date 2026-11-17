//! Map object-data files (`war3map.w3u/.w3t/.w3a/.w3b/.w3d/.w3h/.w3q`).
//!
//! The on-disk layout is owned by [`wc3_formats::gamedata::ObjectDataFile`]:
//!
//! ```text
//! u32 version                         (1..=3)
//! u32 base_count                      section 1: shipped-object modifications
//! base_count × entry
//! u32 custom_count                    section 2: custom objects
//! custom_count × entry
//! entry = old_id[4] new_id[4]
//!         [v3] u32 sets + sets×u32
//!         u32 mod_count
//!         mod_count × { id[4] u32 type value sanity u32 }
//! ```
//!
//! Strings are NUL-terminated (War3Net `ReadChars()`), not length-prefixed;
//! ability/upgrade/doodad entries carry `level`/`variation` and a data-pointer
//! field before each modification (`extended()` in the typed layer). The
//! previous local parser assumed one table, a length-prefixed string and no
//! extended fields, which is why 38 of 44 campaign maps failed on their object
//! data.
//!
//! This module is the resource-layer projection: it keeps the historical
//! `MapObjectData`/`ModValue` API (used by [`crate::objectdata`] and
//! [`crate::mapfiles`]) while delegating every byte decision to the canonical
//! typed parser.

use wc3_formats::gamedata::{self, ChangeValue, ObjectDataFile, ObjectEntry as TypedEntry};

use crate::error::{ReError, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum ModValue {
    Int(i32),
    Real(f32),
    Unreal(f32),
    String(String),
    Int64(i64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Modification {
    pub id: [u8; 4],
    pub value: ModValue,
    /// Ability/upgrade level or doodad variation (0 for simple kinds).
    pub level: u32,
    /// Ability data slot pointer (1-based `DataA`…; 0 when unused).
    pub data_pointer: u32,
    /// Value of the trailing u32. The canonical typed layer does not expose it
    /// (retail files write 0); the field stays for API compatibility.
    pub sanity: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectEntry {
    pub old_id: [u8; 4],
    pub new_id: [u8; 4],
    pub modifications: Vec<Modification>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapObjectData {
    pub version: u32,
    /// File kind: ability/upgrade/doodad records are extended, so the merge needs
    /// the kind to read and re-emit them.
    pub kind: gamedata::ObjectKind,
    pub base_entries: Vec<ObjectEntry>,
    pub new_entries: Vec<ObjectEntry>,
}

impl Default for MapObjectData {
    fn default() -> Self {
        Self {
            version: 0,
            kind: gamedata::ObjectKind::Unit,
            base_entries: Vec::new(),
            new_entries: Vec::new(),
        }
    }
}

impl MapObjectData {
    /// Parse as unit data (historical default; simple records).
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        Self::parse_as(gamedata::ObjectKind::Unit, bytes)
    }

    /// Parse with the file kind. Ability/upgrade/doodad records are extended,
    /// so the kind must match the map file for the stream to stay aligned.
    pub fn parse_as(kind: gamedata::ObjectKind, bytes: &[u8]) -> Result<Self> {
        let file = ObjectDataFile::parse(kind, bytes).map_err(map_error)?;
        Ok(Self {
            version: file.version,
            kind,
            base_entries: file.base.iter().map(project_entry).collect(),
            new_entries: file.custom.iter().map(project_entry).collect(),
        })
    }

    pub fn entries(&self) -> impl Iterator<Item = &ObjectEntry> {
        self.base_entries.iter().chain(self.new_entries.iter())
    }

    pub fn entry_count(&self) -> usize {
        self.base_entries.len() + self.new_entries.len()
    }

    /// Project back onto the canonical model, preserving level/data-pointer so
    /// `gamedata::merge_overlay` can address per-level columns.
    pub fn to_typed(&self) -> ObjectDataFile {
        ObjectDataFile {
            kind: self.kind,
            version: self.version,
            base: self.base_entries.iter().map(to_typed_entry).collect(),
            custom: self.new_entries.iter().map(to_typed_entry).collect(),
        }
    }

    /// All fields of an object as `fourcc -> value`, merging base and custom
    /// entries for the requested id.
    pub fn fields(&self, id: &[u8; 4]) -> std::collections::BTreeMap<String, ModValue> {
        let mut fields = std::collections::BTreeMap::new();
        for entry in self.entries() {
            if entry.old_id == *id || entry.new_id == *id {
                for modification in &entry.modifications {
                    fields.insert(
                        crate::fourcc_str(&modification.id),
                        modification.value.clone(),
                    );
                }
            }
        }
        fields
    }
}

fn project_entry(entry: &TypedEntry) -> ObjectEntry {
    ObjectEntry {
        old_id: entry.old_id.0,
        new_id: entry.new_id.0,
        modifications: entry
            .changes
            .iter()
            .map(|change| Modification {
                id: id_bytes(&change.id),
                value: project_value(&change.value),
                level: change.level,
                data_pointer: change.data_pointer,
                sanity: 0,
            })
            .collect(),
    }
}

fn to_typed_entry(entry: &ObjectEntry) -> TypedEntry {
    TypedEntry {
        old_id: gamedata::FourCc::new(entry.old_id),
        new_id: gamedata::FourCc::new(entry.new_id),
        changes: entry
            .modifications
            .iter()
            .map(|modification| gamedata::ObjectChange {
                id: crate::fourcc_str(&modification.id),
                value: unproject_value(&modification.value),
                level: modification.level,
                data_pointer: modification.data_pointer,
            })
            .collect(),
    }
}

fn unproject_value(value: &ModValue) -> ChangeValue {
    match value {
        ModValue::Int(v) => ChangeValue::Int(*v),
        ModValue::Real(v) => ChangeValue::Real(*v),
        ModValue::Unreal(v) => ChangeValue::Unreal(*v),
        ModValue::String(v) => ChangeValue::String(v.clone()),
        ModValue::Int64(v) => ChangeValue::Int(*v as i32),
    }
}

fn id_bytes(id: &str) -> [u8; 4] {
    let mut out = [0u8; 4];
    let bytes = id.as_bytes();
    let take = bytes.len().min(4);
    out[..take].copy_from_slice(&bytes[..take]);
    out
}

fn project_value(value: &ChangeValue) -> ModValue {
    match value {
        ChangeValue::Int(v) => ModValue::Int(*v),
        ChangeValue::Real(v) => ModValue::Real(*v),
        ChangeValue::Unreal(v) => ModValue::Unreal(*v),
        ChangeValue::String(v) => ModValue::String(v.clone()),
        ChangeValue::Bool(v) => ModValue::Int64(i64::from(*v)),
        ChangeValue::Char(v) => ModValue::Int64(i64::from(*v)),
    }
}

fn map_error(error: gamedata::GamedataError) -> ReError {
    match error {
        gamedata::GamedataError::UnsupportedVersion { file, version } => {
            ReError::Unsupported(format!("{file} object data version {version}"))
        }
        other => ReError::Parse(format!("object data: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parses_v1_campaign_w3u_with_two_tables() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/agent-re/tmp/h01.w3u");
        if !path.is_file() {
            eprintln!("skip: h01.w3u missing");
            return;
        }
        let data = MapObjectData::parse(&std::fs::read(path).unwrap()).expect("parse");
        assert_eq!(data.version, 1);
        assert_eq!(data.base_entries.len(), 11);
        assert_eq!(data.new_entries.len(), 28);
        let first = &data.base_entries[0];
        assert_eq!(crate::fourcc_str(&first.old_id), "nban");
        assert_eq!(crate::fourcc_str(&first.new_id), "\0\0\0\0");
        assert_eq!(crate::fourcc_str(&first.modifications[0].id), "uabi");
        assert_eq!(
            first.modifications[0].value,
            ModValue::String(String::new())
        );
        let custom = &data.new_entries[0];
        assert_eq!(crate::fourcc_str(&custom.old_id), "nvlk");
        assert_eq!(crate::fourcc_str(&custom.new_id), "nC00");
        assert_eq!(
            custom.modifications[0].value,
            ModValue::String("TRIGSTR_219".into())
        );
    }

    #[test]
    fn parses_human04_w3u() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Human04/war3map.w3u");
        if !path.is_file() {
            return;
        }
        let data = MapObjectData::parse(&std::fs::read(path).unwrap()).expect("parse");
        assert!(data.entry_count() > 0);
    }

    #[test]
    fn parses_extended_ability_entries() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Human04/war3map.w3a");
        if !path.is_file() {
            return;
        }
        let data = MapObjectData::parse_as(
            gamedata::ObjectKind::Ability,
            &std::fs::read(path).unwrap(),
        )
        .expect("parse w3a");
        assert!(data.entry_count() > 0);
        // Extended records carry level/data-pointer fields; the projection
        // keeps the modification value, so a non-empty abil field is proof the
        // stream stayed aligned.
        assert!(data.entries().any(|entry| !entry.modifications.is_empty()));
    }
}
