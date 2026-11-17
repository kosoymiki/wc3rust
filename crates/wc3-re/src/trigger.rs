//! `UI\TriggerData.txt` + `UI\TriggerStrings.txt` registry.
//!
//! TriggerData sections and their entry syntax (verified against the local
//! RoC file, 169,395 bytes, 13 sections):
//!
//! ```text
//! [TriggerCategories]  KEY=display,icon[,hideCategoryName]
//! [TriggerTypes]       type=defaultValue
//! [TriggerTypeDefaults] type=value
//! [TriggerParams]      type=params
//! [TriggerEvents]      FunctionName=argType,argType,...
//! [TriggerConditions]  FunctionName=...
//! [TriggerActions]     FunctionName=...
//! [TriggerCalls]       FunctionName=...
//! ```
//!
//! GUI entries are the script function called by the generated map code; the
//! comma list excludes the implicit first `trigger` argument for events. A
//! companion `_<key>_Defaults`, `_<key>_Limits`, `_<key>_Category` (and, for
//! some editors, `_<key>_Display`) entry carries editor metadata. Every
//! function key has a display layout in TriggerStrings with `~parameter`
//! placeholders, which is what the touch UI needs to render a readable action
//! without the World Editor.

use std::collections::BTreeMap;
use std::path::Path;

use crate::error::Result;
use crate::txt::TextTable;

#[derive(Debug, Clone, Default)]
pub struct GuiFunction {
    pub key: String,
    pub section: String,
    pub args: Vec<String>,
    pub defaults: Vec<String>,
    pub limits: Vec<String>,
    pub category: Option<String>,
    pub display: Option<String>,
    pub use_with: Vec<String>,
}

impl GuiFunction {
    pub fn arg_count(&self) -> usize {
        self.args.len()
    }
}

#[derive(Debug, Clone, Default)]
pub struct TriggerStrings {
    pub events: BTreeMap<String, Vec<String>>,
    pub conditions: BTreeMap<String, Vec<String>>,
    pub actions: BTreeMap<String, Vec<String>>,
    pub calls: BTreeMap<String, Vec<String>>,
}

impl TriggerStrings {
    fn insert(&mut self, section: &str, key: &str, values: Vec<String>) {
        match section {
            "TriggerEventStrings" => self.events.insert(key.to_string(), values),
            "TriggerConditionStrings" => self.conditions.insert(key.to_string(), values),
            "TriggerActionStrings" => self.actions.insert(key.to_string(), values),
            "TriggerCallStrings" => self.calls.insert(key.to_string(), values),
            _ => None,
        };
    }

    pub fn for_section(&self, section: &str) -> &BTreeMap<String, Vec<String>> {
        match section {
            "TriggerEvents" => &self.events,
            "TriggerConditions" => &self.conditions,
            "TriggerActions" => &self.actions,
            _ => &self.calls,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TriggerData {
    pub path: String,
    pub categories: BTreeMap<String, Vec<String>>,
    pub types: BTreeMap<String, String>,
    pub type_defaults: BTreeMap<String, String>,
    pub params: BTreeMap<String, Vec<String>>,
    pub events: Vec<GuiFunction>,
    pub conditions: Vec<GuiFunction>,
    pub actions: Vec<GuiFunction>,
    pub calls: Vec<GuiFunction>,
    pub strings: TriggerStrings,
}

impl TriggerData {
    /// Load `TriggerData.txt` plus `TriggerStrings.txt` from a directory.
    pub fn load_dir(ui_dir: &Path) -> Result<Self> {
        let data = std::fs::read(ui_dir.join("TriggerData.txt"))?;
        let strings = std::fs::read(ui_dir.join("TriggerStrings.txt"))?;
        let mut trigger = Self::parse(&data)?;
        trigger.path = ui_dir.join("TriggerData.txt").display().to_string();
        trigger.strings = parse_strings(&strings);
        Ok(trigger)
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let table = TextTable::parse("TriggerData.txt", bytes)?;
        let mut out = Self {
            path: "TriggerData.txt".into(),
            ..Self::default()
        };
        for section in &table.sections {
            match section.name.as_str() {
                "TriggerCategories" => {
                    for (key, value) in &section.entries {
                        out.categories
                            .insert(key.clone(), TextTable::split_list_quoted(value));
                    }
                }
                "TriggerTypes" => {
                    for (key, value) in &section.entries {
                        out.types.insert(key.clone(), value.clone());
                    }
                }
                "TriggerTypeDefaults" => {
                    for (key, value) in &section.entries {
                        out.type_defaults.insert(key.clone(), value.clone());
                    }
                }
                "TriggerParams" => {
                    for (key, value) in &section.entries {
                        out.params
                            .insert(key.clone(), TextTable::split_list_quoted(value));
                    }
                }
                "TriggerEvents" | "TriggerConditions" | "TriggerActions" | "TriggerCalls" => {
                    parse_functions(section, &mut out);
                }
                _ => {}
            }
        }
        Ok(out)
    }

    pub fn all_functions(&self) -> impl Iterator<Item = &GuiFunction> {
        self.events
            .iter()
            .chain(self.conditions.iter())
            .chain(self.actions.iter())
            .chain(self.calls.iter())
    }

    /// Number of GUI functions with a matching display layout in
    /// TriggerStrings, and the keys that lack one.
    pub fn string_coverage(&self) -> (usize, usize, Vec<String>) {
        let mut total = 0;
        let mut resolved = 0;
        let mut missing = Vec::new();
        for function in self.all_functions() {
            total += 1;
            let map = self
                .strings
                .for_section(function.section.as_str());
            if map.contains_key(&function.key) {
                resolved += 1;
            } else if missing.len() < 40 {
                missing.push(function.key.clone());
            }
        }
        (total, resolved, missing)
    }

    /// Cross-check every GUI function against the script function tables from
    /// `common.j` + `Blizzard.j` (GUI entries are BJ wrappers or natives).
    pub fn script_coverage(
        &self,
        functions: &std::collections::BTreeSet<String>,
    ) -> (usize, usize, Vec<String>) {
        let mut resolved = 0;
        let mut missing = Vec::new();
        for function in self.all_functions() {
            if functions.contains(&function.key) {
                resolved += 1;
            } else if missing.len() < 40 {
                missing.push(function.key.clone());
            }
        }
        (self.all_functions().count(), resolved, missing)
    }
}

fn parse_functions(section: &crate::txt::Section, out: &mut TriggerData) {
    let target = match section.name.as_str() {
        "TriggerEvents" => &mut out.events,
        "TriggerConditions" => &mut out.conditions,
        "TriggerActions" => &mut out.actions,
        _ => &mut out.calls,
    };
    let mut by_key: BTreeMap<String, GuiFunction> = BTreeMap::new();
    for (key, value) in &section.entries {
        if key.starts_with('_') {
            continue;
        }
        let args = if value.trim().eq_ignore_ascii_case("nothing") || value.trim().is_empty() {
            Vec::new()
        } else {
            TextTable::split_list_quoted(value)
        };
        by_key.insert(
            key.clone(),
            GuiFunction {
                key: key.clone(),
                section: section.name.clone(),
                args,
                ..GuiFunction::default()
            },
        );
    }
    for (key, value) in &section.entries {
        let Some(rest) = key.strip_prefix('_') else {
            continue;
        };
        let Some((base, attr)) = rest.rsplit_once('_') else {
            continue;
        };
        let Some(function) = by_key.get_mut(base) else {
            continue;
        };
        match attr {
            "Defaults" => {
                function.defaults = if value.trim().is_empty() {
                    Vec::new()
                } else {
                    TextTable::split_list_quoted(value)
                };
            }
            "Limits" => {
                function.limits = if value.trim().is_empty() {
                    Vec::new()
                } else {
                    TextTable::split_list_quoted(value)
                };
            }
            "Category" => function.category = Some(value.clone()),
            "Display" => function.display = Some(value.clone()),
            "UseWith" => function.use_with = TextTable::split_list_quoted(value),
            _ => {}
        }
    }
    target.extend(by_key.into_values());
}

fn parse_strings(bytes: &[u8]) -> TriggerStrings {
    let Ok(table) = TextTable::parse("TriggerStrings.txt", bytes) else {
        return TriggerStrings::default();
    };
    let mut out = TriggerStrings::default();
    for section in &table.sections {
        for (key, value) in &section.entries {
            out.insert(&section.name, key, TextTable::split_list_quoted(value));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_trigger_data() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3/UI");
        let data = TriggerData::load_dir(&dir).expect("load TriggerData");
        assert!(data.events.len() > 20, "events {}", data.events.len());
        assert!(data.conditions.len() > 20, "conditions {}", data.conditions.len());
        assert!(data.actions.len() > 300, "actions {}", data.actions.len());
        assert!(data.categories.contains_key("TC_AI") || !data.categories.is_empty());
        let event = data
            .events
            .iter()
            .find(|f| f.key == "TriggerRegisterPlayerChatEvent")
            .expect("chat event");
        assert_eq!(event.args, vec!["player", "string", "chatmatchtype"]);
        assert_eq!(event.category.as_deref(), Some("TC_PLAYER"));
        assert!(!data.strings.events.is_empty());
    }
}
