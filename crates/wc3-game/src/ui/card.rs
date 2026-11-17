//! Command-card model (mechanic `ui.card`).
//!
//! The WC3 command card is a 4x3 grid. Slots are row-major (`slot = row*4+col`).
//! Entries are data-driven: labels/hotkeys/costs come from the shipped
//! `MenuEntry` rows (`hotkey_label()`/`cost_label()`), and abilities use the
//! shipped `Buttonpos` when the data provides it. Submenus are modelled as
//! pages (`WorkerBuild`, `HeroAbilities`) so the card can never exceed 12
//! slots and the root card is always restorable.

use wc3_data::menu::MenuEntry;

pub const CARD_COLUMNS: usize = 4;
pub const CARD_ROWS: usize = 3;
pub const CARD_SLOTS: usize = CARD_COLUMNS * CARD_ROWS;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CardPage {
    #[default]
    Root,
    WorkerBuild,
    HeroAbilities,
}

impl CardPage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Root => "COMMANDS",
            Self::WorkerBuild => "BUILD",
            Self::HeroAbilities => "ABILITIES",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CardEntry {
    /// 0..11, row-major.
    pub slot: usize,
    pub label: String,
    pub hotkey: String,
    pub cost: String,
    pub enabled: bool,
    /// Opening this entry switches the card to the given submenu.
    pub submenu: Option<CardPage>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CardModel {
    pub page: CardPage,
    pub slots: [Option<CardEntry>; CARD_SLOTS],
}

impl Default for CardModel {
    fn default() -> Self {
        Self::new(CardPage::Root)
    }
}

impl CardModel {
    pub fn new(page: CardPage) -> Self {
        Self {
            page,
            slots: std::array::from_fn(|_| None),
        }
    }

    pub fn filled(&self) -> usize {
        self.slots.iter().filter(|entry| entry.is_some()).count()
    }

    pub fn entry(&self, slot: usize) -> Option<&CardEntry> {
        self.slots.get(slot).and_then(Option::as_ref)
    }

    /// Place an entry at its slot when it is a valid WC3 position.
    pub fn place(&mut self, entry: CardEntry) {
        let slot = entry.slot;
        if slot < CARD_SLOTS {
            self.slots[slot] = Some(entry);
        }
    }

    /// Place an entry at the first empty slot (used when data has no Buttonpos).
    pub fn push(&mut self, mut entry: CardEntry) {
        if let Some(slot) = self.slots.iter().position(Option::is_none) {
            entry.slot = slot;
            self.slots[slot] = Some(entry);
        }
    }

    /// Switch to a submenu page, keeping the root model available through
    /// [`CardModel::open_page`] callers (the game rebuilds the root on Back).
    pub fn open_page(&mut self, page: CardPage) {
        self.page = page;
    }

    pub fn root(&mut self) {
        self.page = CardPage::Root;
    }
}

/// `Buttonpos=(col,row)` → row-major slot. Only a 4x3 position is valid.
pub fn slot_from_button_pos(pos: (u32, u32)) -> Option<usize> {
    let (column, row) = pos;
    if column < CARD_COLUMNS as u32 && row < CARD_ROWS as u32 {
        Some(row as usize * CARD_COLUMNS + column as usize)
    } else {
        None
    }
}

/// Data-driven card from shipped `MenuEntry` rows. Order follows the profile
/// list (WC3 order as shipped); labels/hotkeys/costs come from the entry.
pub fn from_menu(entries: &[MenuEntry], page: CardPage) -> CardModel {
    let mut model = CardModel::new(page);
    for entry in entries {
        model.push(CardEntry {
            slot: 0,
            label: entry.name.to_uppercase(),
            hotkey: entry.hotkey_label(),
            cost: entry.cost_label(),
            enabled: true,
            submenu: None,
        });
    }
    model
}

/// Card from `(label, hotkey, cost, enabled)` rows in WC3 order (row-major).
pub fn from_labeled(rows: &[(String, String, String, bool)], page: CardPage) -> CardModel {
    let mut model = CardModel::new(page);
    for (label, hotkey, cost, enabled) in rows {
        model.push(CardEntry {
            slot: 0,
            label: label.to_uppercase(),
            hotkey: hotkey.clone(),
            cost: cost.clone(),
            enabled: *enabled,
            submenu: None,
        });
    }
    model
}

/// Card from shipped abilities: `(label, Buttonpos, hotkey)`.
pub fn from_abilities(
    abilities: &[(String, Option<(u32, u32)>, String)],
    page: CardPage,
) -> CardModel {
    let mut model = CardModel::new(page);
    for (label, pos, hotkey) in abilities {
        let entry = CardEntry {
            slot: 0,
            label: label.to_uppercase(),
            hotkey: hotkey.clone(),
            cost: String::new(),
            enabled: true,
            submenu: None,
        };
        match pos.and_then(|pos| slot_from_button_pos(pos)) {
            Some(slot) => model.place(CardEntry { slot, ..entry }),
            None => model.push(entry),
        }
    }
    model
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu_entry(id: &str, name: &str, hotkey: &str, gold: i32, food: i32) -> MenuEntry {
        MenuEntry {
            id: id.to_string(),
            name: name.to_string(),
            icon: None,
            hotkey: Some(hotkey.to_string()),
            gold: Some(gold),
            lumber: Some(0),
            food: Some(food),
            build_time: Some(20),
            tooltip: Some("tip".to_string()),
            requires: vec![],
            requires_names: vec![],
            kind: wc3_formats::gamedata::ObjectKind::Unit,
        }
    }

    #[test]
    fn ui_command_card_always_twelve_slots_in_wc3_order() {
        let rows: Vec<(String, String, String, bool)> = (0..7)
            .map(|index| (format!("UNIT{index}"), "F".to_string(), "100G".to_string(), true))
            .collect();
        let card = from_labeled(&rows, CardPage::Root);
        assert_eq!(card.slots.len(), CARD_SLOTS);
        assert_eq!(card.filled(), 7);
        assert_eq!(card.entry(0).map(|entry| entry.label.as_str()), Some("UNIT0"));
        assert_eq!(card.entry(6).map(|entry| entry.label.as_str()), Some("UNIT6"));
        assert!(card.entry(7).is_none());
        for slot in 0..CARD_SLOTS {
            if let Some(entry) = card.entry(slot) {
                assert_eq!(entry.slot, slot, "entry stores its slot");
            }
        }
    }

    #[test]
    fn ui_command_card_button_pos_maps_to_row_major_slots() {
        assert_eq!(slot_from_button_pos((0, 0)), Some(0));
        assert_eq!(slot_from_button_pos((3, 0)), Some(3));
        assert_eq!(slot_from_button_pos((0, 1)), Some(4));
        assert_eq!(slot_from_button_pos((2, 2)), Some(10));
        assert_eq!(slot_from_button_pos((3, 2)), Some(11));
        assert_eq!(slot_from_button_pos((4, 0)), None);
        assert_eq!(slot_from_button_pos((0, 3)), None);
    }

    #[test]
    fn ui_command_card_menu_entries_use_shipped_hotkey_and_cost_labels() {
        let entries = vec![
            menu_entry("hfoo", "Footman", "F,F,F", 160, 2),
            menu_entry("hhou", "Farm", "B", 80, 0),
        ];
        let card = from_menu(&entries, CardPage::Root);
        let footman = card.entry(0).expect("footman");
        assert_eq!(footman.label, "FOOTMAN");
        assert_eq!(footman.hotkey, "F", "first hotkey level");
        assert_eq!(footman.cost, "160G 2F");
        let farm = card.entry(1).expect("farm");
        assert_eq!(farm.hotkey, "B");
        assert_eq!(farm.cost, "80G");
    }

    #[test]
    fn ui_command_card_abilities_use_buttonpos_and_next_free_slot() {
        let abilities = vec![
            ("Holy Light".to_string(), Some((2, 1)), "L".to_string()),
            ("Divine Shield".to_string(), Some((3, 1)), "D".to_string()),
            ("NoPos".to_string(), None, "N".to_string()),
        ];
        let card = from_abilities(&abilities, CardPage::HeroAbilities);
        assert_eq!(card.page, CardPage::HeroAbilities);
        assert_eq!(card.entry(6).map(|entry| entry.label.as_str()), Some("HOLY LIGHT"));
        assert_eq!(card.entry(7).map(|entry| entry.label.as_str()), Some("DIVINE SHIELD"));
        assert_eq!(card.entry(0).map(|entry| entry.label.as_str()), Some("NOPOS"));
    }

    #[test]
    fn ui_command_card_worker_build_and_hero_ability_submenus() {
        let root_rows = vec![
            ("BUILD".to_string(), "B".to_string(), String::new(), true),
            ("GATHER".to_string(), "G".to_string(), String::new(), true),
        ];
        let mut card = from_labeled(&root_rows, CardPage::Root);
        if let Some(entry) = card.slots[0].as_mut() {
            entry.submenu = Some(CardPage::WorkerBuild);
        }
        assert_eq!(card.page, CardPage::Root);
        assert_eq!(card.entry(0).and_then(|entry| entry.submenu), Some(CardPage::WorkerBuild));
        card.open_page(CardPage::WorkerBuild);
        assert_eq!(card.page, CardPage::WorkerBuild);
        assert_eq!(card.page.label(), "BUILD");
        card.open_page(CardPage::HeroAbilities);
        assert_eq!(card.page.label(), "ABILITIES");
        card.root();
        assert_eq!(card.page, CardPage::Root);
        assert_eq!(card.filled(), 2, "root entries survive a submenu visit");
    }
}
