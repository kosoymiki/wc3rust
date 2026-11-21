//! WC3 hotkey remap table (`ui.hotkeys.remap`).
//!
//! WC3 does not hard-code card letters: command hotkeys come from
//! `CommandStrings.txt` (`[CmdAttack] Hotkey=A`) and unit/ability/item/upgrade
//! button letters come from object data (`[hfoo] Hotkey=F`). This module owns
//! that data as a configurable table so the card, the platform and the tests
//! resolve letters through one source instead of per-action constants.

use std::collections::{BTreeMap, BTreeSet};

/// The six fixed command-card actions in WC3 order (A/M/S/H/P + ARMY).
pub const CARD_ACTIONS: [&str; 6] = [
    "CmdAttack",
    "CmdMove",
    "CmdStop",
    "CmdHold",
    "CmdPatrol",
    "CmdSelectArmy",
];

/// WC3 `CommandStrings.txt` encodes a non-printable key as a decimal
/// virtual-key code: the shipped file has `[CmdCancel] Hotkey=512` (Escape).
/// 512 is not the digit five — a letter table must record it as Escape, or
/// pressing control group 5 would fire Cancel.
pub const HOTKEY_ESCAPE_CODE: u32 = 512;

/// Shipped section name -> the action id the port resolves against.
///
/// The retail file names the hold command `[CmdHoldPos]` while the card uses
/// `CmdHold`, and it repeats the build/cancel commands once per race; all of
/// them are one button in the port (`Game::action_id`), so the data has to
/// land on one key instead of five orphan sections.
fn canonical_action(name: &str) -> &str {
    match name {
        "CmdHoldPos" => "CmdHold",
        "CmdBuildHuman" | "CmdBuildOrc" | "CmdBuildNightElf" | "CmdBuildUndead" => "CmdBuild",
        "CmdCancelBuild" | "CmdCancelTrain" | "CmdCancelRevive" => "CmdCancel",
        other => other,
    }
}

/// One parsed `Hotkey=` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HotkeyValue {
    Letter(char),
    /// A WC3 virtual-key code (512 = Escape in the shipped data).
    Code(u32),
}

/// Parse `Hotkey=A` / `Hotkey=512`. A multi-digit value is a key code; a
/// single character is a letter hotkey (a single digit is a legal custom
/// hotkey, a three-digit number is not).
fn parse_hotkey_value(value: &str) -> Option<HotkeyValue> {
    let value = value.trim();
    if value.len() > 1 && value.bytes().all(|byte| byte.is_ascii_digit()) {
        return value.parse::<u32>().ok().map(HotkeyValue::Code);
    }
    value.chars().next().map(HotkeyValue::Letter)
}

/// A letter occupied by two different actions/objects (WC3 allows duplicate
/// letters across *contexts*; inside one table the later binding wins and the
/// displaced one is reported so the caller can warn).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remapped {
    pub previous_action: Option<String>,
    pub previous_object: Option<String>,
}

/// Action (+ object) letter table.
#[derive(Clone, Debug, Default)]
pub struct Remap {
    actions: BTreeMap<String, char>,
    objects: BTreeMap<String, char>,
    /// Actions whose retail hotkey is a virtual-key code (Escape) rather than
    /// a letter; resolved by the host's key-escape path, never by a letter.
    escapes: BTreeSet<String>,
}

impl Remap {
    /// Canonical WC3 defaults for the six card actions. These mirror the
    /// retail `CommandStrings.txt` values the port already used in
    /// `hotkey_label`; `from_command_strings` overrides them from real data.
    pub fn wc3() -> Self {
        let mut remap = Self::default();
        for (action, letter) in [
            ("CmdAttack", 'a'),
            ("CmdMove", 'm'),
            ("CmdStop", 's'),
            ("CmdHold", 'h'),
            ("CmdPatrol", 'p'),
            ("CmdSelectArmy", 'e'),
            ("CmdGather", 'g'),
            ("CmdBuild", 'b'),
        ] {
            remap.actions.insert(action.to_string(), letter);
        }
        remap
    }

    /// Parse a `CommandStrings.txt`-style document:
    ///
    /// ```text
    /// [CmdAttack]
    /// Hotkey=A
    /// ```
    ///
    /// Returns the number of action hotkeys applied.
    pub fn from_command_strings(text: &str) -> Self {
        let mut remap = Self::default();
        remap.apply_command_strings(text);
        remap
    }

    /// Merge a `CommandStrings.txt`-style document into the table.
    ///
    /// Section names are canonicalised (`CmdHoldPos` → `CmdHold`, the four
    /// race build sections → `CmdBuild`, the three cancel sections →
    /// `CmdCancel`), and a `Hotkey=` value that is a WC3 virtual-key code is
    /// recorded as an Escape binding instead of being bound to its first digit.
    ///
    /// Returns the number of letter hotkeys applied.
    pub fn apply_command_strings(&mut self, text: &str) -> usize {
        let mut section = String::new();
        let mut applied = 0;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
                section = canonical_action(name.trim()).to_string();
                continue;
            }
            if let Some(value) = line.strip_prefix("Hotkey=")
                && !section.is_empty()
                && let Some(hotkey) = parse_hotkey_value(value)
            {
                match hotkey {
                    HotkeyValue::Letter(letter) => {
                        self.escapes.remove(&section);
                        self.actions.insert(section.clone(), letter.to_ascii_lowercase());
                        applied += 1;
                    }
                    HotkeyValue::Code(code) => {
                        // Any letter the action claimed before is stale once
                        // the data says the key is non-printable.
                        self.actions.remove(&section);
                        if code == HOTKEY_ESCAPE_CODE {
                            self.escapes.insert(section.clone());
                        } else {
                            self.escapes.remove(&section);
                        }
                    }
                }
            }
        }
        applied
    }

    /// Merge object-data hotkeys (`Hotkey=` inside a `[object id]` block).
    /// Returns the number of objects with a letter.
    pub fn apply_object_data(&mut self, text: &str) -> usize {
        let mut object = String::new();
        let mut applied = 0;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
                object = name.trim().to_string();
                continue;
            }
            if let Some(value) = line.strip_prefix("Hotkey=")
                && !object.is_empty()
                && let Some(HotkeyValue::Letter(letter)) = parse_hotkey_value(value)
            {
                self.objects
                    .insert(object.clone(), letter.to_ascii_lowercase());
                applied += 1;
            }
        }
        applied
    }

    /// Bind an action letter (last binding wins); returns the displaced pair.
    /// Re-binding a non-letter (Escape) action to a letter makes the letter
    /// authoritative again and drops the escape binding.
    pub fn set_action(&mut self, action: &str, key: char) -> Remapped {
        let key = key.to_ascii_lowercase();
        let previous_action = self
            .actions
            .iter()
            .find(|(name, letter)| **letter == key && name.as_str() != action)
            .map(|(name, _)| name.clone());
        let previous_object = self
            .objects
            .iter()
            .find(|(_, letter)| **letter == key)
            .map(|(name, _)| name.clone());
        if let Some(previous) = &previous_action {
            self.actions.remove(previous);
        }
        if let Some(previous) = &previous_object {
            self.objects.remove(previous);
        }
        self.escapes.remove(action);
        self.actions.insert(action.to_string(), key);
        Remapped {
            previous_action,
            previous_object,
        }
    }

    /// Bind an object letter (train/build/cast button).
    pub fn set_object(&mut self, object: &str, key: char) -> Remapped {
        let key = key.to_ascii_lowercase();
        let previous_object = self
            .objects
            .iter()
            .find(|(name, letter)| **letter == key && name.as_str() != object)
            .map(|(name, _)| name.clone());
        let previous_action = self
            .actions
            .iter()
            .find(|(_, letter)| **letter == key)
            .map(|(name, _)| name.clone());
        if let Some(previous) = &previous_object {
            self.objects.remove(previous);
        }
        if let Some(previous) = &previous_action {
            self.actions.remove(previous);
        }
        self.objects.insert(object.to_string(), key);
        Remapped {
            previous_action,
            previous_object,
        }
    }

    pub fn action_letter(&self, action: &str) -> Option<char> {
        self.actions.get(action).copied()
    }

    /// The action a letter fires. Card actions win over the rest, then the
    /// table is alphabetical: the shipped data binds `S` to both `CmdStop` and
    /// `CmdPurchase`, and an alphabetical scan would have handed the visible
    /// Stop button's key to a shop command that is not on the card.
    pub fn action_for(&self, key: char) -> Option<&str> {
        let key = key.to_ascii_lowercase();
        CARD_ACTIONS
            .iter()
            .copied()
            .find(|action| self.actions.get(*action).is_some_and(|letter| *letter == key))
            .or_else(|| {
                self.actions
                    .iter()
                    .find(|(_, letter)| **letter == key)
                    .map(|(name, _)| name.as_str())
            })
    }

    /// Every action bound to `key`, in resolve order (card first). A letter
    /// that the data binds twice is legitimate across contexts; the caller
    /// picks the one on the visible card.
    pub fn actions_for(&self, key: char) -> Vec<&str> {
        let key = key.to_ascii_lowercase();
        let mut actions: Vec<&str> = CARD_ACTIONS
            .iter()
            .copied()
            .filter(|action| self.actions.get(*action).is_some_and(|letter| *letter == key))
            .collect();
        actions.extend(
            self.actions
                .iter()
                .filter(|(name, letter)| {
                    **letter == key && !CARD_ACTIONS.contains(&name.as_str())
                })
                .map(|(name, _)| name.as_str()),
        );
        actions
    }

    /// True when the shipped data binds this action to a non-letter key
    /// (Escape in retail `CommandStrings.txt`: `[CmdCancel] Hotkey=512`).
    pub fn is_escape(&self, action: &str) -> bool {
        self.escapes.contains(action)
    }

    /// Actions whose retail hotkey is Escape, alphabetically.
    pub fn escape_actions(&self) -> impl Iterator<Item = &str> {
        self.escapes.iter().map(String::as_str)
    }

    pub fn object_letter(&self, object: &str) -> Option<char> {
        self.objects.get(object).copied()
    }

    pub fn object_for(&self, key: char) -> Option<&str> {
        let key = key.to_ascii_lowercase();
        self.objects
            .iter()
            .find(|(_, letter)| **letter == key)
            .map(|(name, _)| name.as_str())
    }

    /// The six card actions in WC3 order with their resolved letters.
    pub fn card(&self) -> Vec<(&str, char)> {
        CARD_ACTIONS
            .iter()
            .filter_map(|action| self.action_letter(action).map(|letter| (*action, letter)))
            .collect()
    }
}
