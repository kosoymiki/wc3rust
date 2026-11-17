//! `ui.hotkeys.remap` — card letters come from WC3 data
//! (`CommandStrings.txt` for commands, object data `Hotkey=` for
//! unit/ability buttons) and the remap table is configurable; every card
//! button's letter resolves through it (P0-10, gap-map row `ui.hotkeys`).

use wc3_input::remap::{CARD_ACTIONS, Remap};

/// The retail `CommandStrings.txt` shape (only the fields this port consumes).
const COMMAND_STRINGS: &str = "\
[CmdAttack]
Hotkey=A
[CmdMove]
Hotkey=M
[CmdStop]
Hotkey=S
[CmdHold]
Hotkey=H
[CmdPatrol]
Hotkey=P
[CmdSelectArmy]
Hotkey=E
[CmdGather]
Hotkey=G
[CmdBuild]
Hotkey=B
";

/// Object-data buttons (UnitData/AbilityData `Hotkey=`), the same shape
/// `crates/wc3-data/src/object.rs` parses.
const OBJECT_DATA: &str = "\
[hfoo]
Hotkey=F
[harc]
Hotkey=R
[hpea]
Hotkey=P
[htow]
Hotkey=B
";

#[test]
fn ui_hotkeys_remap_card_letters_come_from_command_strings() {
    let remap = Remap::from_command_strings(COMMAND_STRINGS);
    // Every card action resolves to the WC3 letter, in card order.
    for (action, letter) in remap.card() {
        assert_eq!(remap.action_for(letter), Some(action), "{action} -> {letter}");
        assert_eq!(remap.action_letter(action), Some(letter));
    }
    let card: Vec<(&str, char)> = remap.card();
    assert_eq!(
        card.iter().map(|(action, _)| *action).collect::<Vec<_>>(),
        CARD_ACTIONS.to_vec(),
        "card order is A/M/S/H/P + ARMY"
    );
    assert_eq!(
        card.iter().map(|(_, letter)| *letter).collect::<Vec<_>>(),
        vec!['a', 'm', 's', 'h', 'p', 'e']
    );
}

#[test]
fn ui_hotkeys_remap_is_configurable_and_reports_the_displaced_binding() {
    let mut remap = Remap::from_command_strings(COMMAND_STRINGS);
    // Remap Move M -> K; the old letter stops resolving and K resolves instead.
    let displaced = remap.set_action("CmdMove", 'k');
    assert_eq!(displaced.previous_action, None, "nobody else held K");
    assert_eq!(remap.action_letter("CmdMove"), Some('k'));
    assert_eq!(remap.action_for('k'), Some("CmdMove"));
    assert_eq!(remap.action_for('m'), None, "M was freed by the remap");
    // Stealing a letter from another action displaces that action explicitly.
    let displaced = remap.set_action("CmdHold", 'a');
    assert_eq!(displaced.previous_action, Some("CmdAttack".to_string()));
    assert_eq!(remap.action_for('a'), Some("CmdHold"));
    assert_eq!(remap.action_letter("CmdAttack"), None);
    // Every remaining card button still resolves.
    for (action, letter) in remap.card() {
        assert_eq!(remap.action_for(letter), Some(action));
    }
}

#[test]
fn ui_hotkeys_remap_object_data_letters_drive_the_train_card() {
    let mut remap = Remap::from_command_strings(COMMAND_STRINGS);
    let applied = remap.apply_object_data(OBJECT_DATA);
    assert_eq!(applied, 4, "four object hotkeys parsed");
    for (object, letter) in [("hfoo", 'f'), ("harc", 'r'), ("hpea", 'p'), ("htow", 'b')] {
        assert_eq!(remap.object_letter(object), Some(letter), "{object}");
        assert_eq!(remap.object_for(letter), Some(object), "{letter}");
    }
    // Remapping a unit button keeps the table authoritative.
    let displaced = remap.set_object("hfoo", 't');
    assert_eq!(displaced.previous_object, None, "nobody else held T");
    assert_eq!(remap.object_letter("hfoo"), Some('t'));
    assert_eq!(remap.object_for('f'), None);
    assert_eq!(remap.object_for('t'), Some("hfoo"));
    // Stealing another unit's letter reports the displaced object.
    let displaced = remap.set_object("harc", 't');
    assert_eq!(displaced.previous_object, Some("hfoo".to_string()));
    assert_eq!(remap.object_letter("hfoo"), None);
    assert_eq!(remap.object_for('t'), Some("harc"));
}
