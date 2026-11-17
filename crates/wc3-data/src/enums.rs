//! Editor enum dictionaries (`UI\UnitEditorData.txt`).
//!
//! The shipped data stores many fields as small integers or short codes whose
//! meaning lives in `UnitEditorData.txt`, keyed by the metadata `type` column
//! (e.g. `weaponType`, `attackType`, `defenseType`, `moveType`, `targetList`).
//! Decoding is data driven: nothing here is hardcoded beyond the file format.

use std::collections::HashMap;

use wc3_formats::gamedata::TextTable;

use crate::error::{DataError, Result};

/// One enumerator (`weaponType: missile`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumValue {
    /// Stored value (`data` half of `01=missile,WESTRING_…`).
    pub data: String,
    /// Display reference, usually a `WESTRING_*` key.
    pub display: String,
}

/// Enumerators keyed by metadata `type`.
#[derive(Debug, Clone, Default)]
pub struct EditorEnums {
    values: HashMap<String, Vec<EnumValue>>,
    lookup: HashMap<String, HashMap<String, usize>>,
}

impl EditorEnums {
    /// Parse `UI\UnitEditorData.txt`.
    pub fn parse(text: TextTable) -> Self {
        Self::from_sections(text.sections())
    }

    /// Build from already-parsed sections (ordered `(key, value)` pairs).
    pub fn from_sections(sections: &[(String, Vec<(String, String)>)]) -> Self {
        let mut values: HashMap<String, Vec<EnumValue>> = HashMap::new();
        let mut lookup: HashMap<String, HashMap<String, usize>> = HashMap::new();
        for (section, keys) in sections {
            if section == "global" {
                continue;
            }
            let mut entries: Vec<(u32, String, EnumValue)> = Vec::new();
            for (key, value) in keys {
                if key.eq_ignore_ascii_case("NumValues") || key.eq_ignore_ascii_case("Sort") {
                    continue;
                }
                let Ok(index) = key.trim().parse::<u32>() else {
                    continue;
                };
                let (data, display) = match value.split_once(',') {
                    Some((data, display)) => (data.trim().to_string(), display.trim().to_string()),
                    None => (value.trim().to_string(), String::new()),
                };
                entries.push((
                    index,
                    key.trim().to_string(),
                    EnumValue { data, display },
                ));
            }
            if entries.is_empty() {
                continue;
            }
            entries.sort_by_key(|(index, _, _)| *index);
            let mut index_map = HashMap::with_capacity(entries.len() * 3);
            let mut list: Vec<EnumValue> = Vec::with_capacity(entries.len());
            for (position, (index, key, value)) in entries.into_iter().enumerate() {
                // The stored cell may hold the data word (`missile`), the padded
                // index (`04`) or the bare index (`4`); all three resolve.
                index_map
                    .entry(value.data.to_ascii_lowercase())
                    .or_insert(position);
                index_map.entry(key.to_ascii_lowercase()).or_insert(position);
                index_map.entry(index.to_string()).or_insert(position);
                list.push(value);
            }
            let name = section.to_ascii_lowercase();
            values.insert(name.clone(), list);
            lookup.insert(name, index_map);
        }
        Self { values, lookup }
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn type_names(&self) -> impl Iterator<Item = &str> {
        let mut names: Vec<&str> = self.values.keys().map(String::as_str).collect();
        names.sort_unstable();
        names.into_iter()
    }

    pub fn values(&self, type_name: &str) -> &[EnumValue] {
        self.values
            .get(&type_name.to_ascii_lowercase())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Decode a stored value: the data word (`missile`), the padded index
    /// (`04`) or the bare index (`4`).
    pub fn decode(&self, type_name: &str, data: &str) -> Option<&EnumValue> {
        let map = self.lookup.get(&type_name.to_ascii_lowercase())?;
        let index = *map.get(&data.trim().to_ascii_lowercase())?;
        self.values
            .get(&type_name.to_ascii_lowercase())
            .and_then(|list| list.get(index))
    }

    /// Decode to the display reference (`WESTRING_UE_WEAPONTYPE_MISSILE`).
    pub fn display(&self, type_name: &str, data: &str) -> Option<&str> {
        self.decode(type_name, data).map(|value| value.display.as_str())
    }
}

impl EditorEnums {
    /// Load from a resource source; a missing `UnitEditorData.txt` yields an
    /// empty dictionary (the caller decides whether that is fatal).
    pub fn load(source: &dyn crate::source::Source) -> Result<Option<Self>> {
        let path = "UI\\UnitEditorData.txt";
        if !source.exists(path) {
            return Ok(None);
        }
        let bytes = source.read(path)?;
        let table = TextTable::parse(path, &bytes)
            .map_err(|source| DataError::format(path, source))?;
        Ok(Some(Self::parse(table)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "[weaponType]\nNumValues=8\n00=normal,WESTRING_UE_WEAPONTYPE_NORMAL\n\
01=instant,WESTRING_UE_WEAPONTYPE_INSTANT\n04=missile,WESTRING_UE_WEAPONTYPE_MISSILE\n\
[attackType]\nNumValues=2\n00=unknown,WESTRING_NONE\n01=normal,WESTRING_UE_ATTACKTYPE_NORMAL\n";

    #[test]
    fn decodes_by_data_value_not_position() {
        let table = TextTable::parse("UnitEditorData", TEXT.as_bytes()).unwrap();
        let enums = EditorEnums::parse(table);
        assert_eq!(
            enums.display("weaponType", "missile"),
            Some("WESTRING_UE_WEAPONTYPE_MISSILE")
        );
        assert_eq!(enums.decode("weaponType", "04").unwrap().data, "missile");
        assert!(enums.decode("weaponType", "99").is_none());
        assert_eq!(enums.values("weaponType").len(), 3);
        assert_eq!(enums.len(), 2);
    }
}
