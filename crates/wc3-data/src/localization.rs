//! Localisation: profile text files (`*UnitStrings.txt`, `*UnitFunc.txt`, …),
//! `war3map.wts` string tables and object-data text overrides.
//!
//! Three text layers can name an object, in priority order:
//! 1. object-data overlay (`war3map.w3u` `unam`, …) — the map's own text;
//! 2. `*Func.txt` (loaded after `*Strings.txt`, so it wins on duplicates);
//! 3. `*Strings.txt`.
//!
//! Two accessors exist because the text files mix two kinds of value:
//! [`Localizer::text`] returns a value **verbatim** (a tooltip keeps its commas),
//! while [`Localizer::component`] indexes a comma separated list for keys the
//! metadata marks as indexed (`Missileart`, `Buttonpos`).
//!
//! A value written as `TRIGSTR_<n>` is resolved through the loaded `.wts` table
//! (this is what the game does when a map references a trigger string).
//!
//! Sections can be supplied as bytes (`add_profile`) or as already-parsed
//! sections (`add_sections`), so a composed database (`wc3-re::objectdata`) can
//! hand over its parsed layers without a second read or a second parser.

use std::collections::BTreeMap;

use wc3_formats::gamedata::{parse_trigstr, ProfileOverlay, TextTable, Wts};

use crate::error::{DataError, Result};

/// One parsed profile text file.
#[derive(Debug, Clone)]
struct ProfileLayer {
    name: String,
    sections: Vec<(String, Vec<(String, String)>)>,
}

impl ProfileLayer {
    /// Last value of `key` in section `id` (case-insensitive), like the game's
    /// "last writer wins" merge of profile files.
    fn value(&self, id: &str, key: &str) -> Option<&str> {
        let (_, entries) = self
            .sections
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(id))?;
        entries
            .iter()
            .rev()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    }

    fn has_section(&self, id: &str) -> bool {
        self.sections
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(id))
    }
}

/// Ordered text layers for one data root.
#[derive(Debug, Default)]
pub struct Localizer {
    profiles: Vec<ProfileLayer>,
    loaded: Vec<String>,
    missing: Vec<String>,
    /// Flat `TRIGSTR_n → text` table. Both the raw `.wts` bytes and the
    /// resource layer's already-unescaped entries land here, so there is one
    /// lookup path and no second string table.
    wts: BTreeMap<u32, String>,
    overlay: ProfileOverlay,
    westrings: std::collections::HashMap<String, String>,
}

impl Localizer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a profile text file. Files are searched newest-first, so callers add
    /// `*Strings.txt` before `*Func.txt`.
    pub fn add_profile(&mut self, name: impl Into<String>, bytes: &[u8]) -> Result<()> {
        let name = name.into();
        let table = TextTable::parse(name.clone(), bytes)
            .map_err(|source| DataError::format(&name, source))?;
        self.add_sections(name, table.sections().to_vec());
        Ok(())
    }

    /// Add an already-parsed profile layer (ordered sections, duplicate keys
    /// preserved). Used by the composed install database.
    pub fn add_sections(
        &mut self,
        name: impl Into<String>,
        sections: Vec<(String, Vec<(String, String)>)>,
    ) {
        let name = name.into();
        if !name.is_empty() && !self.loaded.iter().any(|existing| existing == &name) {
            self.loaded.push(name.clone());
        }
        self.profiles.push(ProfileLayer { name, sections });
    }

    /// Note a profile file that the data root does not ship (reported, never fatal).
    pub fn note_missing(&mut self, name: impl Into<String>) {
        self.missing.push(name.into());
    }

    /// Attach a `.wts` string table from raw bytes.
    pub fn set_wts(&mut self, bytes: &[u8]) -> Result<()> {
        let wts = Wts::parse(bytes).map_err(|source| DataError::format("war3map.wts", source))?;
        self.wts = wts.iter().map(|(id, text)| (id, text.to_string())).collect();
        Ok(())
    }

    /// Attach already-resolved `TRIGSTR_n → text` entries (the resource layer
    /// decodes WC3 escapes before handing them over).
    pub fn set_wts_entries(&mut self, entries: BTreeMap<u32, String>) {
        self.wts = entries;
    }

    pub fn wts_entries(&self) -> &BTreeMap<u32, String> {
        &self.wts
    }

    pub fn set_overlay(&mut self, overlay: ProfileOverlay) {
        self.overlay = overlay;
    }

    pub fn overlay(&self) -> &ProfileOverlay {
        &self.overlay
    }

    pub fn overlay_mut(&mut self) -> &mut ProfileOverlay {
        &mut self.overlay
    }

    /// Add the `WESTRING_*` dictionary (`UI\WorldEditStrings.txt`).
    pub fn add_westrings(&mut self, bytes: &[u8]) -> Result<()> {
        let table = TextTable::parse("WorldEditStrings", bytes)
            .map_err(|source| DataError::format("UI\\WorldEditStrings.txt", source))?;
        self.add_westring_sections(table.sections());
        Ok(())
    }

    /// Add `WESTRING_*` entries from already-parsed sections.
    pub fn add_westring_sections(&mut self, sections: &[(String, Vec<(String, String)>)]) {
        for (_section, keys) in sections {
            for (key, value) in keys {
                if key.to_ascii_uppercase().starts_with("WESTRING") {
                    self.westrings
                        .entry(key.to_ascii_uppercase())
                        .or_insert_with(|| value.clone());
                }
            }
        }
    }

    pub fn loaded_files(&self) -> &[String] {
        &self.loaded
    }

    pub fn missing_files(&self) -> &[String] {
        &self.missing
    }

    /// Profile sections across all loaded text files (the synthetic `global`
    /// section counted once per file is not an object section).
    pub fn section_count(&self) -> usize {
        self.profiles
            .iter()
            .map(|profile| {
                profile
                    .sections
                    .iter()
                    .filter(|(name, _)| !name.eq_ignore_ascii_case("global"))
                    .count()
            })
            .sum()
    }

    /// Section ids that carry `key` (case-insensitive), across every loaded
    /// profile layer, deduplicated and sorted. Used for buff sections, which are
    /// only identifiable by their `Buffart`/`Bufftip` fields.
    pub fn sections_with_key(&self, key: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for profile in &self.profiles {
            for (name, entries) in &profile.sections {
                if name.eq_ignore_ascii_case("global") {
                    continue;
                }
                if !entries
                    .iter()
                    .any(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
                {
                    continue;
                }
                if !out.iter().any(|existing| existing.eq_ignore_ascii_case(name)) {
                    out.push(name.clone());
                }
            }
        }
        out.sort();
        out
    }

    pub fn has_section(&self, id: &str) -> bool {
        self.profiles
            .iter()
            .any(|profile| profile.has_section(id))
    }

    /// Resolve `WESTRING_*` key to its localised text.
    pub fn westring(&self, key: &str) -> Option<&str> {
        self.westrings
            .get(&key.trim().to_ascii_uppercase())
            .map(String::as_str)
    }

    /// Verbatim profile value (overlay first, then `*Func.txt`, then `*Strings.txt`).
    pub fn raw(&self, id: &str, key: &str) -> Option<String> {
        if let Some(value) = self.overlay.joined(id, key) {
            if !value.is_empty() {
                return Some(value);
            }
        }
        for profile in self.profiles.iter().rev() {
            if let Some(value) = profile.value(id, key) {
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
        None
    }

    /// All comma separated components of a profile value, quote aware.
    ///
    /// The shipped files write per-level tooltips as a quoted list
    /// (`Ubertip="level 1","level 2","level 3"`) while the text itself may
    /// contain commas, so splitting only happens outside quotes.
    pub fn components(&self, id: &str, key: &str) -> Vec<String> {
        if let Some(components) = self.overlay_components(id, key) {
            if !components.is_empty() && components.iter().any(|value| !value.is_empty()) {
                return components;
            }
        }
        for profile in self.profiles.iter().rev() {
            if let Some(value) = profile.value(id, key) {
                if !value.is_empty() {
                    return split_profile_value(value);
                }
            }
        }
        Vec::new()
    }

    /// A list-valued field (`Trains`, `Builds`, `Requires`), trimmed with WC3's
    /// "no object" markers (`_`, `-`) and empty slots dropped.
    pub fn list(&self, id: &str, key: &str) -> Vec<String> {
        self.components(id, key)
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty() && value != "_" && value != "-")
            .collect()
    }

    fn overlay_components(&self, id: &str, key: &str) -> Option<Vec<String>> {
        let mut out = Vec::new();
        let mut index = 0;
        while let Some(value) = self.overlay.get(id, key, index) {
            out.push(value.to_string());
            index += 1;
        }
        if out.is_empty() {
            // The overlay may only carry a later component.
            if let Some(joined) = self.overlay.joined(id, key) {
                return Some(split_profile_value(&joined));
            }
            return None;
        }
        Some(out)
    }

    /// Component `index` of a profile value (0-based).
    ///
    /// Used for list-valued keys (`Buttonpos=0,2`, `Missileart=a.mdl,b.mdl`) and
    /// for per-level tooltips.
    pub fn component(&self, id: &str, key: &str, index: usize) -> Option<String> {
        self.components(id, key).into_iter().nth(index)
    }

    /// Profile value with `TRIGSTR_` resolved and outer quotes removed.
    ///
    /// A quoted value may be a per-level list (`Ubertip="lvl 1","lvl 2"`); its
    /// segments are joined with `,` here, while [`Localizer::text_level`] keeps
    /// them addressable. Unquoted values are returned verbatim (commas and all).
    pub fn text(&self, id: &str, key: &str) -> Option<String> {
        let raw = self.raw(id, key)?;
        Some(self.resolve(&unquote_profile(&raw)))
    }

    /// Per-level text of a quoted list (`Ubertip`), resolved through `.wts`.
    pub fn text_level(&self, id: &str, key: &str, level: u32) -> Option<String> {
        let components = self.components(id, key);
        if components.is_empty() {
            return None;
        }
        let index = (level.saturating_sub(1) as usize).min(components.len() - 1);
        Some(self.resolve(&components[index]))
    }

    /// Resolve a `TRIGSTR_<n>` reference through the loaded `.wts`.
    pub fn resolve(&self, text: &str) -> String {
        match parse_trigstr(text) {
            Some(key) => match self.wts.get(&key) {
                Some(value) => value.clone(),
                None => text.to_string(),
            },
            None => text.to_string(),
        }
    }

    /// `Name` (units, items, abilities, buffs, upgrades).
    pub fn name(&self, id: &str) -> Option<String> {
        self.text(id, "Name")
    }

    /// `Tip` — the command-card tooltip.
    pub fn tip(&self, id: &str) -> Option<String> {
        self.text(id, "Tip")
    }

    /// `Ubertip` — the extended tooltip.
    pub fn ubertip(&self, id: &str) -> Option<String> {
        self.text(id, "Ubertip")
    }

    /// `Description` — used by items and upgrades.
    pub fn description(&self, id: &str) -> Option<String> {
        self.text(id, "Description")
    }

    /// `Art` — the command-card icon of a unit/item/ability.
    ///
    /// Some entries list one icon per race
    /// (`ARal`: `BTNRallyPoint.blp,BTNOrcRallyPoint.blp,…`); the first component
    /// is the default icon, the full list stays available through
    /// [`Localizer::icon_list`].
    pub fn icon(&self, id: &str) -> Option<String> {
        self.component(id, "Art", 0)
    }

    /// Every icon of a race-dependent `Art` value.
    pub fn icon_list(&self, id: &str) -> Vec<String> {
        self.components(id, "Art")
    }

    /// `Hotkey` — the command-card hotkey letter.
    pub fn hotkey(&self, id: &str) -> Option<String> {
        self.text(id, "Hotkey")
    }

    /// `Tooltip` — the descriptive line retail units carry next to `Tip`
    /// (shops, taverns, dragon roosts). It is a distinct key from the
    /// command-card `Tip`; measured: 69 sections in `NeutralUnitStrings.txt`.
    pub fn tooltip(&self, id: &str) -> Option<String> {
        self.text(id, "Tooltip")
    }

    /// `Researchart` — the "learn/research" button icon.
    ///
    /// Hero abilities ship both `Art` (the passive/active icon used in the
    /// field) and `Researchart` (the icon of the learn-skill button), e.g.
    /// `[AHab] Art=PASBTNBrilliance.blp Researchart=BTNBrilliance.blp`.
    /// Upgrades use the same key when they want a research-tab icon that
    /// differs from the effect icon.
    pub fn research_icon(&self, id: &str) -> Option<String> {
        self.component(id, "Researchart", 0)
    }

    /// `Researchtip` — the learn/research card tooltip.
    pub fn research_tip(&self, id: &str) -> Option<String> {
        self.text(id, "Researchtip")
    }

    /// `Researchubertip` — the learn/research card extended tooltip.
    pub fn research_ubertip(&self, id: &str) -> Option<String> {
        self.text(id, "Researchubertip")
    }

    /// `Researchhotkey` — the learn/research hotkey letter.
    pub fn research_hotkey(&self, id: &str) -> Option<String> {
        self.text(id, "Researchhotkey")
    }

    /// `Buffart` — the buff icon (retail ships buff presentation only here).
    pub fn buff_icon(&self, id: &str) -> Option<String> {
        self.component(id, "Buffart", 0)
    }

    /// `Bufftip` — the buff display name (retail ships no buff SLK).
    pub fn buff_tip(&self, id: &str) -> Option<String> {
        self.text(id, "Bufftip")
    }

    /// `Buttonpos` — `"x,y"` command-card slot.
    pub fn button_pos(&self, id: &str) -> Option<(u32, u32)> {
        let x = self.component(id, "Buttonpos", 0)?;
        let y = self.component(id, "Buttonpos", 1)?;
        Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
    }

    /// `Researchbuttonpos` — upgrade/research slot.
    pub fn research_button_pos(&self, id: &str) -> Option<(u32, u32)> {
        let x = self.component(id, "Researchbuttonpos", 0)?;
        let y = self.component(id, "Researchbuttonpos", 1)?;
        Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
    }

    /// Every profile key for an object (overlay keys included).
    pub fn keys(&self, id: &str) -> Vec<String> {
        let mut keys: Vec<String> = Vec::new();
        for profile in self.profiles.iter() {
            for (section, entries) in &profile.sections {
                if !section.eq_ignore_ascii_case(id) {
                    continue;
                }
                for (key, _) in entries {
                    if !keys.iter().any(|existing| existing.eq_ignore_ascii_case(key)) {
                        keys.push(key.clone());
                    }
                }
            }
        }
        if let Some((_, section)) = self
            .overlay
            .sections()
            .find(|(name, _)| name.eq_ignore_ascii_case(id))
        {
            for key in section.keys() {
                if !keys.iter().any(|existing| existing.eq_ignore_ascii_case(key)) {
                    keys.push(key.clone());
                }
            }
        }
        keys
    }

    /// Profile section names carried by one layer (diagnostics).
    pub fn layer_names(&self) -> Vec<&str> {
        self.profiles
            .iter()
            .map(|profile| profile.name.as_str())
            .collect()
    }

    /// Placeholder tokens used in tooltips: `<AIrt,Data11>`.
    pub fn placeholders(text: &str) -> Vec<Placeholder> {
        let mut out = Vec::new();
        let bytes = text.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'<' {
                if let Some(end) = text[index..].find('>') {
                    let body = &text[index + 1..index + end];
                    if let Some((left, right)) = body.split_once(',') {
                        out.push(Placeholder {
                            target: left.trim().to_string(),
                            field: right.trim().to_string(),
                        });
                    }
                    index += end + 1;
                    continue;
                }
            }
            index += 1;
        }
        out
    }
}

/// One `<target,field>` token inside a tooltip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder {
    /// Object whose field is referenced (`AIrt`).
    pub target: String,
    /// Field or column referenced (`Data11`).
    pub field: String,
}

/// Remove the outer quotes of a profile value.
///
/// Quoted lists (`"a","b"`) are split first so each segment loses its own quote
/// pair; unquoted text keeps every character, including commas.
fn unquote_profile(value: &str) -> String {
    if !value.starts_with('"') {
        return value.to_string();
    }
    split_profile_value(value).join(",")
}

/// Split a profile value into quoted/unquoted comma separated components.
fn split_profile_value(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                out.push(current.trim().to_string());
                current.clear();
            }
            other => current.push(other),
        }
    }
    out.push(current.trim().to_string());
    if out.len() > 1 {
        while out.last().is_some_and(|last| last.is_empty()) {
            out.pop();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_priority_and_components() {
        let mut localizer = Localizer::new();
        localizer
            .add_profile("UnitStrings", b"[hfoo]\nName=Footman\nButtonpos=0,1\n" as &[u8])
            .unwrap();
        localizer
            .add_profile("UnitFunc", b"[hfoo]\nArt=btn.blp\n" as &[u8])
            .unwrap();
        assert_eq!(localizer.name("hfoo").as_deref(), Some("Footman"));
        assert_eq!(localizer.icon("hfoo").as_deref(), Some("btn.blp"));
        assert_eq!(localizer.button_pos("hfoo"), Some((0, 1)));
        assert!(localizer.missing_files().is_empty());
    }

    #[test]
    fn parsed_sections_match_parsed_bytes() {
        let bytes = b"[hfoo]\nName=Footman\nTip=Melee unit\n";
        let mut from_bytes = Localizer::new();
        from_bytes.add_profile("UnitStrings", bytes).unwrap();
        let table = TextTable::parse("UnitStrings", bytes).unwrap();
        let mut from_sections = Localizer::new();
        from_sections.add_sections("UnitStrings", table.sections().to_vec());
        assert_eq!(from_bytes.name("hfoo"), from_sections.name("hfoo"));
        assert_eq!(from_bytes.section_count(), from_sections.section_count());
        assert_eq!(from_sections.loaded_files(), ["UnitStrings"]);
    }

    #[test]
    fn commas_in_text_are_not_split() {
        let mut localizer = Localizer::new();
        localizer
            .add_profile(
                "UnitStrings",
                "[hfoo]\nUbertip=Deals damage, then slows, then stuns\n".as_bytes(),
            )
            .unwrap();
        assert_eq!(
            localizer.ubertip("hfoo").as_deref(),
            Some("Deals damage, then slows, then stuns")
        );
        // ...while a list-valued key is addressable by component.
        localizer
            .add_profile("UnitFunc", "[hfoo]\nMissileart=a.mdl,b.mdl\n".as_bytes())
            .unwrap();
        assert_eq!(
            localizer.component("hfoo", "Missileart", 1).as_deref(),
            Some("b.mdl")
        );
    }

    #[test]
    fn trigstr_is_resolved_through_wts() {
        let mut localizer = Localizer::new();
        localizer.set_wts(b"STRING 87\n{\nCustom Unit\n}\n").unwrap();
        localizer
            .add_profile("x", b"[uC01]\nName=TRIGSTR_087\n" as &[u8])
            .unwrap();
        assert_eq!(localizer.name("uC01").as_deref(), Some("Custom Unit"));
    }

    #[test]
    fn overlay_beats_profile_text() {
        let mut localizer = Localizer::new();
        localizer
            .add_profile("x", b"[hfoo]\nName=Footman\n" as &[u8])
            .unwrap();
        let mut overlay = ProfileOverlay::default();
        overlay.set("hfoo", "Name", 0, "Пехотинец");
        localizer.set_overlay(overlay);
        assert_eq!(localizer.name("hfoo").as_deref(), Some("Пехотинец"));
    }

    #[test]
    fn parses_tooltip_placeholders() {
        let tokens = Localizer::placeholders("Deals <AHbz,Data11> damage for <AHbz,Data12> seconds");
        assert_eq!(
            tokens,
            vec![
                Placeholder {
                    target: "AHbz".into(),
                    field: "Data11".into()
                },
                Placeholder {
                    target: "AHbz".into(),
                    field: "Data12".into()
                }
            ]
        );
    }

    #[test]
    fn list_values_drop_the_no_object_markers() {
        let mut localizer = Localizer::new();
        localizer
            .add_profile(
                "x",
                "[hfoo]\nTrains=hfoo, _, -,,hpea\nResearches=Rhme,Rhar\n".as_bytes(),
            )
            .unwrap();
        assert_eq!(localizer.list("hfoo", "Trains"), ["hfoo", "hpea"]);
        assert_eq!(localizer.list("hfoo", "Researches"), ["Rhme", "Rhar"]);
        assert!(localizer.list("hfoo", "Builds").is_empty());
        // The raw component list keeps the slots, `list` is the cleaned view.
        assert_eq!(localizer.components("hfoo", "Trains").len(), 5);
    }

    #[test]
    fn research_buff_and_tooltip_keys_are_addressable() {
        let mut localizer = Localizer::new();
        localizer
            .add_profile(
                "x",
                "[AHab]\nArt=PASBTNBrilliance.blp\nResearchart=BTNBrilliance.blp\n\
Researchtip=Learn\nResearchubertip=Learn more\nResearchhotkey=B\n\
[Nmer]\nTooltip=Hire mercenaries\n[BHwe]\nBufftip=Water Spirit\nBuffart=BTNWater.blp\n"
                    .as_bytes(),
            )
            .unwrap();
        assert_eq!(
            localizer.icon("AHab").as_deref(),
            Some("PASBTNBrilliance.blp")
        );
        assert_eq!(
            localizer.research_icon("AHab").as_deref(),
            Some("BTNBrilliance.blp")
        );
        assert_eq!(localizer.research_tip("AHab").as_deref(), Some("Learn"));
        assert_eq!(
            localizer.research_ubertip("AHab").as_deref(),
            Some("Learn more")
        );
        assert_eq!(localizer.research_hotkey("AHab").as_deref(), Some("B"));
        assert_eq!(
            localizer.tooltip("Nmer").as_deref(),
            Some("Hire mercenaries")
        );
        assert_eq!(localizer.buff_tip("BHwe").as_deref(), Some("Water Spirit"));
        assert_eq!(localizer.buff_icon("BHwe").as_deref(), Some("BTNWater.blp"));
        assert!(localizer.research_icon("BHwe").is_none());
    }
}
