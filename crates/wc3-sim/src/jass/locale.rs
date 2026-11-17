//! Localized strings and map flags: the two data sources `GetLocalizedString`
//! and `IsMapFlagSet` read.
//!
//! ## Where the strings come from
//!
//! A map script never carries user-visible text. `war3map.j` writes
//! `GetLocalizedString("TRIGSTR_029")` or `GetLocalizedString("PLAYER_LEFT_GAME")`
//! and the engine resolves the key against, in order:
//!
//! 1. the map's own `war3map.wts` (a `STRING <n> { text }` table; the script
//!    refers to entry `n` through the zero-padded `TRIGSTR_nnn` key),
//! 2. the install's `UI\FrameDef\GlobalStrings.fdf` (`StringList` block, a
//!    `KEY "value",` table — the same names `Blizzard.j` uses for the game-over
//!    dialogs, `PLAYER_LEFT_GAME`, `GAMEOVER_VICTORY_MSG`, …).
//!
//! A key that resolves nowhere returns the key itself and is recorded **once**
//! as `JASS_STRING missing=<key>`; a silent empty string would make a dialog
//! look translated while showing nothing, which is the failure mode this table
//! exists to prevent.
//!
//! ## Where the flags come from
//!
//! `IsMapFlagSet(flag)` reads the `MapFlags` word of the map's `war3map.w3i`
//! header. The engine's map-flag constants are the word's bits (the Reforged
//! `MAP_FLAG_*` naming, e.g. `MAP_FLAG_HIDE_MINIMAP = 1`); [`MAP_FLAG_BITS`]
//! is that table and is the only set of values the simulator answers. A value
//! outside the table is answered `false` and recorded once as
//! `JASS_MAPFLAG unknown=<n>` instead of being folded into a plausible zero.
//!
//! Provenance for the byte layout: `donors/War3Net/src/War3Net.Build.Core/
//! Serialization/Binary/Info/MapInfo.cs` (`CameraBounds` → 8 floats,
//! `CameraBoundsComplements` → 4 ints, playable width/height → 2 ints, then
//! `MapFlags`), and `Info/MapFlags.cs` for the bit names. Verified against
//! `donors/War3Net/tests/.../TestData/Info/NewMap.w3i` (v28) whose word
//! `0x9c14` sets `MeleeMap` (bit 2) and `analysis/war3_map/Human04/war3map.w3i`
//! (v18) whose word `0x1fe1` clears it — a campaign map.

use std::collections::{BTreeMap, BTreeSet};

/// The engine's map-flag word bits, in the Reforged `MAP_FLAG_*` naming.
///
/// The value *is* the bit mask in the `war3map.w3i` header, so
/// `IsMapFlagSet(MAP_FLAG_HIDE_MINIMAP)` is `word & 1 != 0`.
pub const MAP_FLAG_BITS: &[(i32, &str)] = &[
    (1 << 0, "MAP_FLAG_HIDE_MINIMAP"),
    (1 << 1, "MAP_FLAG_MODIFY_ALLY_PRIORITIES"),
    (1 << 2, "MAP_FLAG_MELEE_MAP"),
    (1 << 3, "MAP_FLAG_PLAYABLE_MAP_SIZE_LARGE"),
    (1 << 4, "MAP_FLAG_MASKED_AREAS_PARTIALLY_VISIBLE"),
    (1 << 5, "MAP_FLAG_FIXED_PLAYER_SETTINGS"),
    (1 << 6, "MAP_FLAG_USE_CUSTOM_FORCES"),
    (1 << 7, "MAP_FLAG_USE_CUSTOM_TECHTREE"),
    (1 << 8, "MAP_FLAG_USE_CUSTOM_ABILITIES"),
    (1 << 9, "MAP_FLAG_USE_CUSTOM_UPGRADES"),
    (1 << 10, "MAP_FLAG_MAP_PROPERTIES_MENU_OPENED"),
    (1 << 11, "MAP_FLAG_WATER_WAVES_ON_CLIFF_SHORES"),
    (1 << 12, "MAP_FLAG_WATER_WAVES_ON_ROLLING_SHORES"),
    (1 << 13, "MAP_FLAG_HAS_TERRAIN_FOG"),
    (1 << 14, "MAP_FLAG_REQUIRES_EXPANSION"),
    (1 << 15, "MAP_FLAG_USE_ITEM_CLASSIFICATION"),
    (1 << 16, "MAP_FLAG_HAS_WATER_TINTING_COLOR"),
    (1 << 17, "MAP_FLAG_ACCURATE_PROBABILITY"),
    (1 << 18, "MAP_FLAG_CUSTOM_ABILITY_SKIN"),
    (1 << 19, "MAP_FLAG_DISABLE_DENY_ICON"),
    (1 << 20, "MAP_FLAG_FORCE_DEFAULT_CAMERA_ZOOM"),
    (1 << 21, "MAP_FLAG_FORCE_MAX_CAMERA_ZOOM"),
    (1 << 22, "MAP_FLAG_FORCE_MIN_CAMERA_ZOOM"),
    (1 << 23, "MAP_FLAG_HAS_WATER_COLOR_HD"),
    (1 << 24, "MAP_FLAG_HAS_ALPHA_TILE_DEFAULT_MINIMAP_COLOR"),
    (1 << 25, "MAP_FLAG_DYNAMIC_MINIMAP"),
    (1 << 29, "MAP_FLAG_FLAG30"),
    (1 << 30, "MAP_FLAG_FLAG31"),
];

/// Name of a known map-flag value (`MAP_FLAG_HIDE_MINIMAP`, …).
pub fn map_flag_name(flag: i32) -> Option<&'static str> {
    MAP_FLAG_BITS
        .iter()
        .find(|(value, _)| *value == flag)
        .map(|(_, name)| *name)
}

/// True when `flag` is one of the engine's map-flag bits.
pub fn is_known_map_flag(flag: i32) -> bool {
    map_flag_name(flag).is_some()
}

/// Reads the `MapFlags` word out of a `war3map.w3i` payload.
///
/// Returns `None` when the version or the layout cannot be decoded (the v8 RoC
/// beta header carries six extra fields before the camera bounds), so a caller
/// never mistakes a misread offset for a flag word.
pub fn map_flags_from_w3i(bytes: &[u8]) -> Option<u32> {
    let version = u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?);
    // v15 introduced the camera-bounds complements; the shipped maps and the
    // donor fixtures are v18/v25/v28, and Reforged is v31.
    if !(15..=40).contains(&version) {
        return None;
    }
    let mut pos = 4usize;
    if version >= 18 {
        // saves + editorVersion, and from v27 on a 16-byte game version.
        pos = if version >= 27 { 28 } else { 12 };
    }
    // name, author, description, recommended players
    for _ in 0..4 {
        let len = bytes.get(pos..)?.iter().position(|byte| *byte == 0)?;
        pos += len + 1;
    }
    // CameraBounds: 4 corners as 2 floats each = 32 bytes.
    // CameraBoundsComplements: 4 ints = 16 bytes.
    // PlayableMapAreaWidth/Height: 2 ints = 8 bytes.
    let flags = pos + 32 + 16 + 8;
    let word = bytes.get(flags..flags + 4)?;
    Some(u32::from_le_bytes(word.try_into().ok()?))
}

/// `war3map.wts` + `GlobalStrings.fdf` lookups for `GetLocalizedString`.
#[derive(Debug, Clone, Default)]
pub struct LocalizedStrings {
    /// `war3map.wts` entries, keyed by the trigger-string number (no padding).
    map: BTreeMap<u32, String>,
    /// `GlobalStrings.fdf` entries, keyed by their identifier.
    globals: BTreeMap<String, String>,
    /// One `JASS_STRING missing=<key>` line per unresolved key, in first-seen
    /// order of the sorted key set when read back.
    pub log: Vec<String>,
    missing: BTreeSet<String>,
}

impl LocalizedStrings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads `war3map.wts` text. Returns the number of entries kept.
    pub fn load_wts(&mut self, text: &str) -> usize {
        let entries = parse_wts(text);
        let count = entries.len();
        self.map.extend(entries);
        count
    }

    /// Loads `GlobalStrings.fdf` text (the `StringList` block). Returns the
    /// number of entries kept.
    pub fn load_global_strings(&mut self, text: &str) -> usize {
        let entries = parse_global_strings_fdf(text);
        let count = entries.len();
        self.globals.extend(entries);
        count
    }

    /// Number of map (`wts`) entries loaded.
    pub fn map_len(&self) -> usize {
        self.map.len()
    }

    /// Number of global (`fdf`) entries loaded.
    pub fn globals_len(&self) -> usize {
        self.globals.len()
    }

    /// Keys that resolved nowhere, in sorted order.
    pub fn missing_keys(&self) -> Vec<&str> {
        self.missing.iter().map(String::as_str).collect()
    }

    /// Resolves a script key, exactly like the engine: map trigger string
    /// first, global string table second, the key itself last (recorded once).
    pub fn resolve(&mut self, key: &str) -> String {
        if key.is_empty() {
            return String::new();
        }
        if let Some(number) = trigger_string_number(key)
            && let Some(text) = self.map.get(&number)
        {
            return text.clone();
        }
        if let Some(text) = self.globals.get(key) {
            return text.clone();
        }
        if self.missing.insert(key.to_string()) {
            self.log.push(format!("JASS_STRING missing={key}"));
        }
        key.to_string()
    }
}

/// Parses the `TRIGSTR_nnn` form a map script uses for trigger strings.
/// Zero padding is ignored (`TRIGSTR_029` is entry 29).
pub fn trigger_string_number(key: &str) -> Option<u32> {
    let rest = key
        .get(..8)
        .filter(|prefix| prefix.eq_ignore_ascii_case("TRIGSTR_"))
        .map(|_| &key[8..])?;
    if rest.is_empty() || !rest.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    rest.parse::<u32>().ok()
}

/// Parses a `war3map.wts` table.
///
/// Format (Blizzard's own, unchanged since RoC):
/// ```text
/// STRING 1
/// {
/// Some text, may span lines
/// }
/// ```
/// `// comment` lines between entries are ignored; keys are stored numeric so
/// `TRIGSTR_001` and `TRIGSTR_1` address the same entry.
pub fn parse_wts(text: &str) -> BTreeMap<u32, String> {
    let mut out = BTreeMap::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start_matches('\u{feff}').trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        let Some(rest) = strip_prefix_ci(trimmed, "STRING") else {
            continue;
        };
        let Ok(number) = rest.trim().parse::<u32>() else {
            continue;
        };
        // The value is everything up to the line that closes the block.
        let mut body = String::new();
        let mut opened = false;
        for entry in lines.by_ref() {
            let entry_trimmed = entry.trim();
            if !opened {
                if entry_trimmed.starts_with('{') {
                    opened = true;
                    // The text may start on the brace's own line.
                    let inline = entry_trimmed.trim_start_matches('{').trim();
                    if !inline.is_empty() && !inline.starts_with('}') {
                        body.push_str(inline);
                    }
                }
                continue;
            }
            if entry_trimmed.starts_with('}') {
                break;
            }
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(entry);
        }
        if opened {
            out.insert(number, body);
        }
    }
    out
}

/// Parses the `StringList { K "v", … }` block of `UI\FrameDef\GlobalStrings.fdf`.
///
/// The FDF grammar is small but not line-oriented: entries may share a line,
/// values may contain `//` and escaped quotes, and block comments wrap the
/// whole header. This is a token scan over the `StringList` block only — the
/// frame definitions in the same file tree are a different node type and are
/// not part of the string table.
pub fn parse_global_strings_fdf(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut tokens = FdfTokens::new(text);
    let mut in_list = false;
    let mut depth = 0usize;
    let mut pending: Option<String> = None;
    while let Some((kind, token)) = tokens.next_token() {
        match kind {
            Token::Ident => {
                if token.eq_ignore_ascii_case("StringList") {
                    in_list = true;
                    depth = 0;
                    pending = None;
                } else if in_list {
                    pending = Some(token);
                }
            }
            Token::String => {
                if in_list
                    && let Some(key) = pending.take()
                {
                    out.insert(key, token);
                }
            }
            Token::Open => {
                if in_list {
                    depth += 1;
                }
            }
            Token::Close => {
                if in_list {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        in_list = false;
                        pending = None;
                    }
                }
            }
            Token::Other => {
                pending = None;
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Ident,
    String,
    Open,
    Close,
    Other,
}

struct FdfTokens<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> FdfTokens<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text: text.trim_start_matches('\u{feff}'),
            pos: 0,
        }
    }

    /// Next token. Works on `char` boundaries, because every string value in
    /// `GlobalStrings.fdf` is UTF-8 (`|Cffffcc00Ловкость|R`): a byte-wise scan
    /// would split a multi-byte character into mojibake.
    fn next_token(&mut self) -> Option<(Token, String)> {
        loop {
            let rest = &self.text[self.pos..];
            let ch = rest.chars().next()?;
            if ch.is_whitespace() {
                self.pos += ch.len_utf8();
                continue;
            }
            if rest.starts_with("/*") {
                match rest[2..].find("*/") {
                    Some(end) => self.pos += 2 + end + 2,
                    None => self.pos = self.text.len(),
                }
                continue;
            }
            if rest.starts_with("//") {
                match rest.find('\n') {
                    Some(end) => self.pos += end + 1,
                    None => self.pos = self.text.len(),
                }
                continue;
            }
            break;
        }
        let rest = &self.text[self.pos..];
        let ch = rest.chars().next()?;
        match ch {
            '{' => {
                self.pos += 1;
                Some((Token::Open, "{".to_string()))
            }
            '}' => {
                self.pos += 1;
                Some((Token::Close, "}".to_string()))
            }
            '"' => {
                self.pos += 1;
                let mut value = String::new();
                while let Some(inner) = self.text[self.pos..].chars().next() {
                    match inner {
                        '\\' => {
                            self.pos += 1;
                            if let Some(escaped) = self.text[self.pos..].chars().next() {
                                value.push(escaped);
                                self.pos += escaped.len_utf8();
                            }
                        }
                        '"' => {
                            self.pos += 1;
                            break;
                        }
                        other => {
                            value.push(other);
                            self.pos += other.len_utf8();
                        }
                    }
                }
                Some((Token::String, value))
            }
            first if first.is_alphabetic() || first == '_' => {
                let start = self.pos;
                while let Some(inner) = self.text[self.pos..].chars().next() {
                    if inner.is_alphanumeric() || inner == '_' || inner == '.' {
                        self.pos += inner.len_utf8();
                    } else {
                        break;
                    }
                }
                Some((Token::Ident, self.text[start..self.pos].to_string()))
            }
            other => {
                self.pos += other.len_utf8();
                Some((Token::Other, other.to_string()))
            }
        }
    }
}

fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .map(|_| &text[prefix.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jass_natives_get_localized_string_wts_entries_are_addressable_by_trigstr() {
        let wts = "STRING 1\n{\nHello\n}\n\nSTRING 29\n// comment\n{\nTwo\nlines\n}\n";
        let parsed = parse_wts(wts);
        assert_eq!(parsed.get(&1).map(String::as_str), Some("Hello"));
        assert_eq!(parsed.get(&29).map(String::as_str), Some("Two\nlines"));
        assert_eq!(trigger_string_number("TRIGSTR_001"), Some(1));
        assert_eq!(trigger_string_number("TRIGSTR_29"), Some(29));
        assert_eq!(trigger_string_number("trigstr_029"), Some(29));
        assert_eq!(trigger_string_number("PLAYER_LEFT_GAME"), None);
    }

    #[test]
    fn jass_natives_get_localized_string_resolution_order_and_missing_log() {
        let mut table = LocalizedStrings::new();
        assert_eq!(table.load_wts("STRING 7\n{\nfrom the map\n}\n"), 1);
        table.load_global_strings(
            "StringList {\n    PLAYER_LEFT_GAME \"Игрок %s вышел из игры.\",\n}\n",
        );
        assert_eq!(table.resolve("TRIGSTR_7"), "from the map");
        assert_eq!(table.resolve("PLAYER_LEFT_GAME"), "Игрок %s вышел из игры.");
        // An unresolved key is returned verbatim, never as an empty string,
        // and is recorded exactly once.
        assert_eq!(table.resolve("NO_SUCH_KEY"), "NO_SUCH_KEY");
        assert_eq!(table.resolve("NO_SUCH_KEY"), "NO_SUCH_KEY");
        assert_eq!(table.log, vec!["JASS_STRING missing=NO_SUCH_KEY"]);
        assert_eq!(table.missing_keys(), vec!["NO_SUCH_KEY"]);
    }

    #[test]
    fn jass_natives_get_localized_string_fdf_parser_skips_comments_and_sibling_nodes() {
        let fdf = "\u{feff}/* header */\nStringList {\n    A \"one\",\n    // note\n    B \"two \\\"quoted\\\"\",\n    C \"Игрок %s вышел из игры.\",\n}\nFrame \"FRAME\" {\n    D \"not a string-table entry\",\n}\n";
        let parsed = parse_global_strings_fdf(fdf);
        assert_eq!(parsed.get("A").map(String::as_str), Some("one"));
        assert_eq!(parsed.get("B").map(String::as_str), Some("two \"quoted\""));
        // UTF-8 values survive byte-for-byte: the install's table is localized.
        assert_eq!(
            parsed.get("C").map(String::as_str),
            Some("Игрок %s вышел из игры.")
        );
        assert!(!parsed.contains_key("D"));
    }

    #[test]
    fn jass_natives_is_map_flag_set_reads_the_w3i_word() {
        // v28 melee map fixture: the word is 0x9c14, so MeleeMap (bit 2) is set.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Info/NewMap.w3i",
        );
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("skip: {} missing", path.display());
            return;
        };
        let word = map_flags_from_w3i(&bytes).expect("flags word");
        assert_eq!(word, 0x9c14, "NewMap.w3i flags word");
        assert_eq!(word & (1 << 2), 1 << 2, "MeleeMap bit");
        assert!(is_known_map_flag(1));
        assert!(is_known_map_flag(1 << 2));
        assert_eq!(map_flag_name(1), Some("MAP_FLAG_HIDE_MINIMAP"));
        assert!(!is_known_map_flag(1 << 3 | 1));
        assert!(!is_known_map_flag(0));
    }

    #[test]
    fn jass_natives_is_map_flag_set_campaign_map_clears_the_melee_bit() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Human04/war3map.w3i");
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("skip: {} missing", path.display());
            return;
        };
        let word = map_flags_from_w3i(&bytes).expect("flags word");
        assert_eq!(word, 0x1fe1, "Human04 is a campaign header");
        assert_eq!(word & (1 << 2), 0, "a campaign map is not a melee map");
    }
}
