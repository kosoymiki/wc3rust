//! AUD-018 — `ClearTextMessages` is not a no-op.
//!
//! The host keeps one display model (`Host::display_log`), written by
//! `DisplayTextToPlayer`/`DisplayTimedTextToPlayer`. `ClearTextMessages` is the
//! local client's "clear my message area": it removes what the local player can
//! see (broadcasts and messages addressed to that slot) and leaves the
//! transcript (`JassVm::text_log`) intact.
//!
//! Parity source: `analysis/war3_full/Scripts/Blizzard.j:1334-1337`
//! (`ClearTextMessagesBJ` gates the call per force, so the native itself is the
//! per-client clear) and `common.j:1451` (the native takes no player).

mod common;

use wc3_sim::jass::Value;

fn global_bool(scripted: &wc3_sim::jass::ScriptedSim, name: &str) -> bool {
    let index = scripted.vm().code.global_by_name[name];
    matches!(scripted.vm().globals[index as usize], Value::Bool(true))
}

/// Two messages → clear → the display model is empty; the transcript is not.
#[test]
fn aud018_clear_text_messages_clears_the_display_model_and_keeps_the_transcript() {
    let mut scripted = common::scripted(
        r#"
        function show takes nothing returns nothing
            call DisplayTextToPlayer(Player(0), 0.0, 0.0, "first")
            call DisplayTimedTextToPlayer(Player(1), 0.0, 0.0, 3.0, "second")
        endfunction
        function clear takes nothing returns nothing
            call ClearTextMessages()
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("show").expect("show");
    assert_eq!(
        scripted.vm().host.display_log,
        vec![
            (Some(0), "first".to_string()),
            (Some(1), "second".to_string())
        ],
        "both messages are on the display, with their own text"
    );
    assert_eq!(scripted.vm().text_log.len(), 2, "the transcript saw both");

    scripted.vm_mut().run_function("clear").expect("clear");
    assert!(
        scripted.vm().host.display_log.is_empty(),
        "ClearTextMessages must empty the display model: {:?}",
        scripted.vm().host.display_log
    );
    assert_eq!(
        scripted.vm().text_log.len(),
        2,
        "clearing the screen must not delete the transcript"
    );

    // A message shown after the clear behaves normally.
    scripted.vm_mut().run_function("show").expect("show again");
    assert_eq!(scripted.vm().host.display_log.len(), 2);
}

/// Player scope: with a declared local player the clear removes that player's
/// view (broadcasts + own messages), not another player's message.
#[test]
fn aud018_clear_text_messages_keeps_messages_addressed_to_another_player() {
    let mut scripted = common::scripted(
        r#"
        globals
            boolean cleared = false
        endglobals
        function setup takes nothing returns nothing
            // MAP_CONTROL_USER = 2 (older internal enum) declares slot 1 as the
            // local player, which also makes GetLocalPlayer() answer Player(1).
            call SetPlayerController(Player(1), 2)
            call DisplayTextToPlayer(Player(1), 0.0, 0.0, "mine")
            call DisplayTextToPlayer(Player(0), 0.0, 0.0, "theirs")
            call DisplayTextToPlayer(Player(1), 0.0, 0.0, "mine again")
            call ClearTextMessages()
            set cleared = true
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("setup").expect("setup");
    assert!(global_bool(&scripted, "cleared"));
    assert_eq!(
        scripted.vm().host.local_player,
        Some(1),
        "the test relies on a declared local player"
    );
    assert_eq!(
        scripted.vm().host.display_log,
        vec![(Some(0), "theirs".to_string())],
        "only the local player's view is cleared"
    );
}
