//! INI-style configuration parser (`war3mapMisc.txt`, `Units\UnitBalance.slk` neighbours).

use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct Ini {
    sections: Vec<(String, BTreeMap<String, String>)>,
}

impl Ini {
    pub fn parse(text: &str) -> Self {
        let mut ini = Ini::default();
        let mut current = String::from("global");
        ini.sections.push((current.clone(), BTreeMap::new()));
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with("//") || line.starts_with(';') {
                continue;
            }
            if let Some(inner) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                current = inner.trim().to_string();
                if !ini.sections.iter().any(|(name, _)| name == &current) {
                    ini.sections.push((current.clone(), BTreeMap::new()));
                }
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let value = value.trim().trim_matches('"').to_string();
                if let Some((_, map)) = ini.sections.iter_mut().find(|(name, _)| name == &current) {
                    map.insert(key.trim().to_lowercase(), value);
                }
            }
        }
        ini
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(section))
            .and_then(|(_, map)| map.get(&key.to_lowercase()))
            .map(String::as_str)
    }

    pub fn section(&self, section: &str) -> Option<&BTreeMap<String, String>> {
        self.sections
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(section))
            .map(|(_, map)| map)
    }

    pub fn sections(&self) -> &[(String, BTreeMap<String, String>)] {
        &self.sections
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sections_and_comments() {
        let ini = Ini::parse("[General]\n// comment\nKey = value\n[Misc]\nA=1\n");
        assert_eq!(ini.get("general", "key"), Some("value"));
        assert_eq!(ini.get("MISC", "a"), Some("1"));
        assert_eq!(ini.sections().len(), 3);
    }
}
