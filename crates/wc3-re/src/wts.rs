//! `war3map.wts` string table.
//!
//! Layout: `STRING <n>` line, `{` on its own line, the value (which may span
//! several lines and contain `\n`, `\r`, `\t`, `\"` and `\\` escapes), then `}`
//! on its own line. The editor refers to entries as `TRIGSTR_<n>`; the map
//! loader substitutes them into `war3map.w3i` strings and object data.

use std::collections::BTreeMap;

use crate::error::{ReError, Result};

#[derive(Debug, Clone, Default)]
pub struct Wts {
    pub entries: BTreeMap<u32, String>,
}

impl Wts {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        // The grammar lives in `wc3-formats::gamedata::Wts` (BOM, `STRING n`,
        // `{`…`}` blocks, multi-line values). This layer adds the WC3 escape
        // decoding and the ordered map used by the resource scanner.
        let parsed = wc3_formats::gamedata::Wts::parse(bytes)
            .map_err(|err| ReError::Parse(format!("WTS: {err}")))?;
        let entries = parsed
            .iter()
            .map(|(id, value)| (id, unescape(value)))
            .collect();
        Ok(Self { entries })
    }

    pub fn get(&self, id: u32) -> Option<&str> {
        self.entries.get(&id).map(String::as_str)
    }

    /// Replace every `TRIGSTR_<n>` occurrence in `text`.
    pub fn resolve(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(pos) = rest.find("TRIGSTR_") {
            out.push_str(&rest[..pos]);
            let after = &rest[pos + 8..];
            let digits = after
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after.len());
            if digits == 0 {
                out.push_str("TRIGSTR_");
                rest = after;
                continue;
            }
            let id = after[..digits].parse::<u32>().ok();
            if let Some(value) = id.and_then(|id| self.get(id)) {
                out.push_str(value);
            } else {
                out.push_str(&rest[pos..pos + 8 + digits]);
            }
            rest = &after[digits..];
        }
        out.push_str(rest);
        out
    }
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn resolves_real_map_wts() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Human04/war3map.wts");
        let Ok(bytes) = std::fs::read(path) else {
            eprintln!("skip: war3map.wts missing");
            return;
        };
        let wts = Wts::parse(&bytes).expect("parse");
        assert!(!wts.entries.is_empty());
        let first = *wts.entries.keys().next().unwrap();
        assert!(wts.get(first).is_some());
        assert_eq!(wts.resolve("TRIGSTR_004"), wts.get(4).unwrap_or("TRIGSTR_004"));
    }

    #[test]
    fn escapes_quotes_and_newlines() {
        let wts = Wts::parse(b"STRING 1\n{\nline\\nnext \\\"q\\\"\n}\n").unwrap();
        assert_eq!(wts.get(1), Some("line\nnext \"q\""));
    }
}
