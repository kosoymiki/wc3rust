//! Object data files (`war3map.w3u`, `w3t`, `w3a`, `w3d`, `w3b`, `w3h`, `w3q`).
//!
//! This module is the compact compatibility view kept for callers of the
//! removed `W3Map` reader; new code uses `wc3-formats::gamedata::ObjectDataFile`.
//! The canonical parser is [`crate::gamedata::ObjectDataFile`], which implements
//! the real on-disk layout: two sections (base modifications, then custom
//! objects), NUL-terminated strings, per-kind extended records and the v3 set
//! list. The previous local reader assumed one section and length-prefixed
//! strings, which is why 38 of the 44 campaign maps failed to load their object
//! data. Keeping only a projection here means one parser, no divergence.
//!
//! `ObjectData::parse` keeps its historical signature and reads unit data;
//! containers that know the file kind must call [`ObjectData::parse_as`] so
//! ability/upgrade/doodad records decode their `level`/`data pointer` fields.

use crate::gamedata::{self, ChangeValue, GamedataError, ObjectKind};
use crate::FormatError;

#[derive(Debug, Clone, PartialEq)]
pub enum ModValue {
    Int(i32),
    Real(f32),
    Unreal(f32),
    String(String),
}

#[derive(Debug, Clone)]
pub struct Modification {
    pub id: [u8; 4],
    pub value: ModValue,
}

#[derive(Debug, Clone)]
pub struct ObjectEntry {
    pub old_id: [u8; 4],
    pub new_id: [u8; 4],
    pub variation: u32,
    pub modifications: Vec<Modification>,
}

#[derive(Debug, Clone, Default)]
pub struct ObjectData {
    pub version: u32,
    pub entries: Vec<ObjectEntry>,
}

impl ObjectData {
    /// Parse as unit object data (historical default).
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        Self::parse_as(ObjectKind::Unit, bytes)
    }

    /// Parse with the file kind, so extended records (ability/upgrade/doodad)
    /// consume their `level` and data-pointer fields instead of misreading the
    /// next record.
    pub fn parse_as(kind: ObjectKind, bytes: &[u8]) -> Result<Self, FormatError> {
        let file = gamedata::ObjectDataFile::parse(kind, bytes).map_err(map_error)?;
        let mut entries = Vec::with_capacity(file.len());
        for entry in file.entries() {
            let variation = entry
                .changes
                .iter()
                .map(|change| change.level)
                .max()
                .unwrap_or(0);
            entries.push(ObjectEntry {
                old_id: entry.old_id.0,
                new_id: entry.new_id.0,
                variation,
                modifications: entry
                    .changes
                    .iter()
                    .map(|change| Modification {
                        id: crate::gamedata::FourCc::parse(&change.id)
                            .map(|fourcc| fourcc.0)
                            .unwrap_or([0; 4]),
                        value: project_value(&change.value),
                    })
                    .collect(),
            });
        }
        Ok(Self {
            version: file.version,
            entries,
        })
    }

    pub fn entry(&self, new_id: &[u8; 4]) -> Option<&ObjectEntry> {
        self.entries.iter().find(|e| &e.new_id == new_id)
    }

    pub fn field(&self, new_id: &[u8; 4], field: &[u8; 4]) -> Option<&ModValue> {
        self.entry(new_id)?
            .modifications
            .iter()
            .find(|m| &m.id == field)
            .map(|m| &m.value)
    }
}

/// Project the typed change value onto the legacy four-value enum. `Bool` and
/// the obsolete `Char` type are represented as integers because the legacy
/// consumers only compare/print them; no information that affects map loading
/// is lost.
fn project_value(value: &ChangeValue) -> ModValue {
    match value {
        ChangeValue::Int(v) => ModValue::Int(*v),
        ChangeValue::Real(v) => ModValue::Real(*v),
        ChangeValue::Unreal(v) => ModValue::Unreal(*v),
        ChangeValue::String(v) => ModValue::String(v.clone()),
        ChangeValue::Bool(v) => ModValue::Int(i32::from(*v)),
        ChangeValue::Char(v) => ModValue::Int(*v),
    }
}

fn map_error(error: GamedataError) -> FormatError {
    match error {
        GamedataError::Format(inner) => inner,
        GamedataError::UnsupportedVersion { .. } => {
            FormatError::Unsupported("object data version")
        }
        GamedataError::CountLimit { .. } => FormatError::Corrupt("object data count"),
        _ => FormatError::Corrupt("object data"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_real_map_units_file() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Human04/war3map.w3u");
        let Ok(bytes) = fs::read(path) else {
            eprintln!("skipping: w3u fixture missing");
            return;
        };
        let data = ObjectData::parse_as(ObjectKind::Unit, &bytes).expect("parse w3u");
        assert!((1..=3).contains(&data.version));
        assert!(!data.entries.is_empty());
    }

    #[test]
    fn parses_synthetic_two_section_entry() {
        // v1 layout: version, base count, base entries, custom count, custom entries.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes()); // base section
        bytes.extend_from_slice(&1u32.to_le_bytes()); // custom section
        bytes.extend_from_slice(b"hfoo");
        bytes.extend_from_slice(b"h001");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(b"uhpm");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&777i32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let data = ObjectData::parse(&bytes).unwrap();
        assert_eq!(data.version, 1);
        assert_eq!(data.field(b"h001", b"uhpm"), Some(&ModValue::Int(777)));
    }

    #[test]
    fn parses_ability_file_with_extended_records() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Human04/war3map.w3a");
        let Ok(bytes) = fs::read(path) else {
            eprintln!("skipping: w3a fixture missing");
            return;
        };
        let data = ObjectData::parse_as(ObjectKind::Ability, &bytes).expect("parse w3a");
        assert!(!data.entries.is_empty());
        assert!(data.entries.iter().any(|entry| entry.variation > 0));
    }
}
