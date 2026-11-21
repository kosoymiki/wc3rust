//! `ui.hotkeys.remap` — the shipped `Units/CommandStrings.txt` shape.
//!
//! Retail data is not the tidy subset the synthetic fixture assumes:
//! the hold command is `[CmdHoldPos]`, the build and cancel commands are
//! repeated once per race, `[CmdCancel] Hotkey=512` encodes Escape as a
//! virtual-key code, and `S` is bound to both `CmdStop` and `CmdPurchase`.
//! These tests pin the parser against that shape; the section/letter table
//! below is transcribed from the operator's shipped file
//! (`analysis/war3_full3/Units/CommandStrings.txt`, 19 sections, observed
//! 2026-11-21), and the last test re-reads that file when it is present.

use std::path::PathBuf;

use wc3_input::remap::Remap;

/// Retail sections and their `Hotkey=` values, in file order.
const RETAIL: &str = "\
[CmdMove]
Hotkey=M
[CmdAttack]
Hotkey=A
[CmdAttackGround]
Hotkey=G
[CmdBuild]
Hotkey=B
[CmdBuildHuman]
Hotkey=B
[CmdBuildOrc]
Hotkey=B
[CmdBuildNightElf]
Hotkey=B
[CmdBuildUndead]
Hotkey=B
[CmdCancel]
Hotkey=512
[CmdCancelBuild]
Hotkey=512
[CmdCancelTrain]
Hotkey=512
[CmdCancelRevive]
Hotkey=512
[CmdHoldPos]
Hotkey=H
[CmdPatrol]
Hotkey=P
[CmdPurchase]
Hotkey=S
[CmdRally]
Hotkey=Y
[CmdSelectSkill]
Hotkey=O
[CmdStop]
Hotkey=S
";

#[test]
fn ui_hotkeys_remap_retail_sections_canonicalise_to_the_card() {
    let remap = Remap::from_command_strings(RETAIL);
    // The card resolves from the retail file, including the hold command the
    // data names CmdHoldPos.
    assert_eq!(remap.action_letter("CmdAttack"), Some('a'));
    assert_eq!(remap.action_letter("CmdMove"), Some('m'));
    assert_eq!(remap.action_letter("CmdStop"), Some('s'));
    assert_eq!(remap.action_letter("CmdHold"), Some('h'), "CmdHoldPos lands on Hold");
    assert_eq!(remap.action_letter("CmdPatrol"), Some('p'));
    assert_eq!(remap.action_letter("CmdBuild"), Some('b'), "four race sections, one button");
    assert_eq!(
        remap.card().iter().map(|(action, letter)| (*action, *letter)).collect::<Vec<_>>(),
        vec![
            ("CmdAttack", 'a'),
            ("CmdMove", 'm'),
            ("CmdStop", 's'),
            ("CmdHold", 'h'),
            ("CmdPatrol", 'p'),
        ],
        "ARMY alone has no retail letter and is absent from the card"
    );
}

#[test]
fn ui_hotkeys_remap_retail_escape_code_is_not_the_five_key() {
    let remap = Remap::from_command_strings(RETAIL);
    // `Hotkey=512` is Escape. It must not become the digit five: that key is
    // control group 5 on the platform.
    assert_eq!(remap.action_letter("CmdCancel"), None);
    assert_eq!(remap.action_for('5'), None, "512 is Escape, not '5'");
    assert!(remap.is_escape("CmdCancel"));
    let escapes: Vec<&str> = remap.escape_actions().collect();
    assert_eq!(
        escapes,
        vec!["CmdCancel"],
        "the three race cancel sections collapse into one action"
    );
    // A later letter rebinding takes the action back from the escape set.
    let mut rebound = remap;
    rebound.set_action("CmdCancel", 'x');
    assert!(!rebound.is_escape("CmdCancel"));
    assert_eq!(rebound.action_letter("CmdCancel"), Some('x'));
}

#[test]
fn ui_hotkeys_remap_retail_duplicate_letters_resolve_to_the_visible_command() {
    let remap = Remap::from_command_strings(RETAIL);
    // Stop and Purchase both claim S in retail data; the card command wins so
    // the visible Stop button keeps its key.
    assert_eq!(remap.action_for('s'), Some("CmdStop"));
    assert_eq!(
        remap.actions_for('s'),
        vec!["CmdStop", "CmdPurchase"],
        "both bindings stay reachable for a context-aware caller"
    );
    // B is claimed by five build sections that are one button.
    assert_eq!(remap.action_for('b'), Some("CmdBuild"));
    assert_eq!(remap.actions_for('b'), vec!["CmdBuild"]);
    // The retail-only commands still resolve by their own letters.
    assert_eq!(remap.action_for('g'), Some("CmdAttackGround"));
    assert_eq!(remap.action_for('y'), Some("CmdRally"));
    assert_eq!(remap.action_for('o'), Some("CmdSelectSkill"));
}

#[test]
fn ui_hotkeys_remap_matches_the_shipped_command_strings_file() {
    // Oracle: the operator's shipped data. A fresh clone has no `analysis/`
    // assets, so the test reports the skip instead of pretending to pass.
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../analysis/war3_full3/Units/CommandStrings.txt");
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!(
            "SKIP shipped CommandStrings oracle: {} is absent (transcribed table still covers the shape)",
            path.display()
        );
        return;
    };
    let remap = Remap::from_command_strings(&text);
    for (action, letter) in [
        ("CmdAttack", 'a'),
        ("CmdMove", 'm'),
        ("CmdStop", 's'),
        ("CmdHold", 'h'),
        ("CmdPatrol", 'p'),
        ("CmdBuild", 'b'),
        ("CmdAttackGround", 'g'),
        ("CmdRally", 'y'),
        ("CmdSelectSkill", 'o'),
    ] {
        assert_eq!(remap.action_letter(action), Some(letter), "{action} in {path:?}");
    }
    assert!(remap.is_escape("CmdCancel"), "CmdCancel is Escape in the shipped file");
    assert_eq!(remap.action_for('5'), None);
    assert_eq!(remap.action_for('s'), Some("CmdStop"));
}
