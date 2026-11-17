//! WC3 text tables: `TriggerData.txt`, `war3skins.txt`, `*Func.txt`,
//! `*Strings.txt`, `MiscData.txt`, `UnitEditorData.txt`.
//!
//! Format: optional UTF-8 BOM, `[Section]` headers, `key=value` entries,
//! `//` line comments, blank lines. Values may contain `=` and `,`; the first
//! `=` splits. Duplicate keys are preserved in source order (the game's loader
//! applies them in order, so the last one is the effective value; both views
//! are exposed). Keys and section names are matched case-insensitively by the
//! accessors because the retail files mix `Art` / `art` spellings.

use std::collections::BTreeMap;

use crate::error::Result;

#[derive(Debug, Clone, Default)]
pub struct Section {
    pub name: String,
    pub entries: Vec<(String, String)>,
}

impl Section {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    pub fn get_all(&self, key: &str) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
            .collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct TextTable {
    pub path: String,
    pub preamble: Vec<(String, String)>,
    pub sections: Vec<Section>,
    section_index: BTreeMap<String, usize>,
}

impl TextTable {
    pub fn parse(path: impl Into<String>, bytes: &[u8]) -> Result<Self> {
        // Parsing is owned by `wc3-formats::gamedata::TextTable` (BOM, ordered
        // sections, duplicate keys, `//`/`;` comments, first `=` split). This
        // resource view adds the case-insensitive section index, separates the
        // pre-section preamble and exposes quoted list splitting.
        let path: String = path.into();
        // The canonical `gamedata::TextTable` parser requires UTF-8. One
        // shipped BattleNet chaticon file is Windows-1252 (0x96 at byte 2513,
        // an en dash); normalize only that encoding failure, then keep the
        // canonical parser as the only grammar implementation.
        let normalized;
        let bytes = if std::str::from_utf8(bytes).is_ok() {
            bytes
        } else {
            normalized = decode_windows_1252(bytes);
            normalized.as_bytes()
        };
        let parsed = wc3_formats::gamedata::TextTable::parse(path.clone(), bytes)
            .map_err(|err| crate::error::ReError::Parse(format!("{path}: {err}")))?;
        let mut table = Self {
            path,
            ..Self::default()
        };
        for (index, (name, entries)) in parsed.sections().iter().enumerate() {
            if index == 0 && name == "global" {
                table.preamble.extend(entries.iter().cloned());
                continue;
            }
            table.push_section(Section {
                name: name.clone(),
                entries: entries.clone(),
            });
        }
        Ok(table)
    }

    fn push_section(&mut self, section: Section) {
        let key = section.name.to_ascii_lowercase();
        self.section_index
            .entry(key)
            .or_insert(self.sections.len());
        self.sections.push(section);
    }

    pub fn section(&self, name: &str) -> Option<&Section> {
        self.section_index
            .get(&name.to_ascii_lowercase())
            .and_then(|&index| self.sections.get(index))
    }

    pub fn section_names(&self) -> Vec<&str> {
        self.sections.iter().map(|s| s.name.as_str()).collect()
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.section(section)?.get(key)
    }

    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.sections
            .iter()
            .flat_map(|section| section.entries.iter())
            .map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Split a comma-separated WC3 list value, trimming whitespace. Quoted
    /// values keep their quotes (needed by TriggerData string layouts).
    pub fn split_list(value: &str) -> Vec<&str> {
        value.split(',').map(str::trim).collect()
    }

    /// Comma split that does not break inside double quotes. TriggerData uses
    /// quoted raw text in layouts (`"~Name", ", "`), so a naive split loses
    /// argument boundaries.
    pub fn split_list_quoted(value: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        for ch in value.chars() {
            match ch {
                '"' => {
                    in_quotes = !in_quotes;
                    current.push(ch);
                }
                ',' if !in_quotes => {
                    out.push(current.trim().to_string());
                    current.clear();
                }
                _ => current.push(ch),
            }
        }
        let tail = current.trim();
        if !tail.is_empty() || !out.is_empty() {
            out.push(tail.to_string());
        }
        out
    }
}

fn decode_windows_1252(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &byte in bytes {
        let ch = match byte {
            0x80 => '\u{20ac}',
            0x82 => '\u{201a}',
            0x83 => '\u{0192}',
            0x84 => '\u{201e}',
            0x85 => '\u{2026}',
            0x86 => '\u{2020}',
            0x87 => '\u{2021}',
            0x88 => '\u{02c6}',
            0x89 => '\u{2030}',
            0x8a => '\u{0160}',
            0x8b => '\u{2039}',
            0x8c => '\u{0152}',
            0x8e => '\u{017d}',
            0x91 => '\u{2018}',
            0x92 => '\u{2019}',
            0x93 => '\u{201c}',
            0x94 => '\u{201d}',
            0x95 => '\u{2022}',
            0x96 => '\u{2013}',
            0x97 => '\u{2014}',
            0x98 => '\u{02dc}',
            0x99 => '\u{2122}',
            0x9a => '\u{0161}',
            0x9b => '\u{203a}',
            0x9c => '\u{0153}',
            0x9e => '\u{017e}',
            0x9f => '\u{0178}',
            0x81 | 0x8d | 0x8f | 0x90 | 0x9d => '\u{fffd}',
            _ => char::from(byte),
        };
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parses_war3skins_sections() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/UI/war3skins.txt");
        let Ok(bytes) = std::fs::read(path) else {
            eprintln!("skip: war3skins.txt missing");
            return;
        };
        let table = TextTable::parse("war3skins.txt", &bytes).expect("parse");
        assert!(table.section("Human").is_some());
        assert!(table.section("Default").is_some());
        assert_eq!(
            table.get("Human", "CommandBasicStruct"),
            Some("ReplaceableTextures\\CommandButtons\\BTNHumanBuild.blp")
        );
    }

    #[test]
    fn decodes_cp1252_fallback_for_battlenet_text() {
        let table = TextTable::parse("cp1252", b"[S]\nk=caf\x96\n").expect("parse");
        assert_eq!(table.get("s", "k"), Some("caf\u{2013}"));
    }

    #[test]
    fn preserves_equals_inside_value() {
        let table = TextTable::parse("x", b"[S]\nk=a=b\n").unwrap();
        assert_eq!(table.get("s", "K"), Some("a=b"));
    }
}
