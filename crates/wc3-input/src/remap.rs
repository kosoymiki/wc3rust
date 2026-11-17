//! WC3 hotkey remap table (`ui.hotkeys.remap`).
//!
//! WC3 does not hard-code card letters: command hotkeys come from
//! `CommandStrings.txt` (`[CmdAttack] Hotkey=A`) and unit/ability/item/upgrade
//! button letters come from object data (`[hfoo] Hotkey=F`). This module owns
//! that data as a configurable table so the card, the platform and the tests
//! resolve letters through one source instead of per-action constants.

use std::collections::BTreeMap;

/// The six fixed command-card actions in WC3 order (A/M/S/H/P + ARMY).
pub const CARD_ACTIONS: [&str; 6] = [
    "CmdAttack",
    "CmdMove",
    "CmdStop",
    "CmdHold",
    "CmdPatrol",
    "CmdSelectArmy",
];

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
    pub fn apply_command_strings(&mut self, text: &str) -> usize {
        let mut section = String::new();
        let mut applied = 0;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
                section = name.trim().to_string();
                continue;
            }
            if let Some(value) = line.strip_prefix("Hotkey=")
                && let Some(letter) = value.trim().chars().next()
                && !section.is_empty()
            {
                self.actions
                    .insert(section.clone(), letter.to_ascii_lowercase());
                applied += 1;
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
                && let Some(letter) = value.trim().chars().next()
                && !object.is_empty()
            {
                self.objects
                    .insert(object.clone(), letter.to_ascii_lowercase());
                applied += 1;
            }
        }
        applied
    }

    /// Bind an action letter (last binding wins); returns the displaced pair.
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

    pub fn action_for(&self, key: char) -> Option<&str> {
        let key = key.to_ascii_lowercase();
        self.actions
            .iter()
            .find(|(_, letter)| **letter == key)
            .map(|(name, _)| name.as_str())
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
