//! Object-data store: the map's own overrides (`war3map.w3u`, `.w3a`, `.w3t`,
//! `.w3q`, `.w3b`, `.w3d`, `.w3h`).
//!
//! Parsing lives in [`wc3_formats::gamedata`]; this module keeps the loaded files
//! together with their provenance so validation and tools can report what a map
//! overrides without re-reading bytes.

use wc3_formats::gamedata::{ObjectDataFile, ObjectKind};

use crate::error::{DataError, Result};

/// One loaded object-data file.
#[derive(Debug, Clone)]
pub struct ObjectDataEntry {
    /// Path or label the bytes came from.
    pub source: String,
    pub file: ObjectDataFile,
}

/// All object-data files loaded for a data set.
#[derive(Debug, Default, Clone)]
pub struct ObjectDataStore {
    files: Vec<ObjectDataEntry>,
}

impl ObjectDataStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse and keep a file. `source` is used only in diagnostics.
    pub fn add(&mut self, source: impl Into<String>, kind: ObjectKind, bytes: &[u8]) -> Result<()> {
        let source = source.into();
        let file = ObjectDataFile::parse(kind, bytes)
            .map_err(|error| DataError::format(source.clone(), error))?;
        self.files.push(ObjectDataEntry { source, file });
        Ok(())
    }

    /// Keep an already parsed file.
    pub fn add_parsed(&mut self, source: impl Into<String>, file: ObjectDataFile) {
        self.files.push(ObjectDataEntry {
            source: source.into(),
            file,
        });
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn entries(&self) -> &[ObjectDataEntry] {
        &self.files
    }

    pub fn by_kind(&self, kind: ObjectKind) -> impl Iterator<Item = &ObjectDataEntry> {
        self.files.iter().filter(move |entry| entry.file.kind == kind)
    }

    /// `(kind, base entries, custom entries, files)` summary.
    pub fn counts(&self) -> Vec<(ObjectKind, usize, usize, usize)> {
        let mut out = Vec::new();
        for kind in ObjectKind::ALL {
            let mut base = 0;
            let mut custom = 0;
            let mut files = 0;
            for entry in self.by_kind(kind) {
                base += entry.file.base.len();
                custom += entry.file.custom.len();
                files += 1;
            }
            if files > 0 {
                out.push((kind, base, custom, files));
            }
        }
        out
    }

    /// Total base/custom entries across all files.
    pub fn totals(&self) -> (usize, usize) {
        self.files.iter().fold((0, 0), |(base, custom), entry| {
            (
                base + entry.file.base.len(),
                custom + entry.file.custom.len(),
            )
        })
    }

    /// Prototype id of a custom object (`new_id` → `old_id`).
    ///
    /// A custom row stores only the fields the map changed; every other field is
    /// the prototype's value at lookup time (that is what `old_id` means in the
    /// format). The last loaded writer wins, matching the merge order.
    pub fn prototype_of(&self, id: &str) -> Option<String> {
        let mut found = None;
        for entry in &self.files {
            for record in &entry.file.custom {
                if record.new_id.as_str().eq_ignore_ascii_case(id) {
                    found = Some(record.old_id.as_str().into_owned());
                }
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_files_and_counts() {
        // Version 2, no base entries, one custom unit entry (no changes).
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(b"hfoo");
        bytes.extend_from_slice(b"h001");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let mut store = ObjectDataStore::new();
        store.add("war3map.w3u", ObjectKind::Unit, &bytes).expect("parse");
        assert_eq!(store.len(), 1);
        let counts = store.counts();
        assert_eq!(counts, vec![(ObjectKind::Unit, 0, 1, 1)]);
        assert_eq!(store.totals(), (0, 1));
        // The custom row records its prototype; the prototype itself has none.
        assert_eq!(store.prototype_of("h001").as_deref(), Some("hfoo"));
        assert_eq!(store.prototype_of("H001").as_deref(), Some("hfoo"));
        assert_eq!(store.prototype_of("hfoo"), None);
    }
}
