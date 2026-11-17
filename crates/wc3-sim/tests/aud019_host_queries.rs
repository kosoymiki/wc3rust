//! AUD-019 — `IsPlayerObserver` / `IsFogEnabled` / `IsFogMaskEnabled` answer
//! real host state instead of constants.
//!
//! * Fog: the setters `FogEnable`/`FogMaskEnable` (`world.rs`) already wrote
//!   `Host::fog_enabled`/`fog_mask_enabled` (and the save round-trips them);
//!   the getters now read the same fields. Defaults are the melee ones (`true`),
//!   which is what `Blizzard.j`'s cinematic mode saves and restores:
//!   `common.j:1347-1349`, `Blizzard.j:4553-4554`.
//! * Observer: `PLAYER_STATE_OBSERVER = ConvertPlayerState(11)`
//!   (`analysis/war3_full/Scripts/common.j:294`) is one value in the player
//!   state map, so `GetPlayerState`, `SetPlayerState`,
//!   `TriggerRegisterPlayerStateEvent` and `IsPlayerObserver` cannot disagree.
//!   No lobby assigns observers in a headless boot, so the flag is set by the
//!   script (registry status `Partial`, see `native_status.rs`).

mod common;

use wc3_sim::jass::Value;

fn global_bool(scripted: &wc3_sim::jass::ScriptedSim, name: &str) -> bool {
    let index = scripted.vm().code.global_by_name[name];
    matches!(scripted.vm().globals[index as usize], Value::Bool(true))
}

fn global_int(scripted: &wc3_sim::jass::ScriptedSim, name: &str) -> i32 {
    let index = scripted.vm().code.global_by_name[name];
    scripted.vm().globals[index as usize].int()
}

const SCRIPT: &str = r#"
globals
    boolean fog_default = false
    boolean fog_off = false
    boolean fog_on_again = false
    boolean mask_default = false
    boolean mask_off = false
    boolean mask_on_again = false
    boolean observer_default = false
    boolean observer_set = false
    boolean observer_cleared = false
    boolean observer_null = false
    integer observer_state = 0
endglobals

function setup takes nothing returns nothing
    set fog_default = IsFogEnabled()
    set mask_default = IsFogMaskEnabled()

    call FogEnable(false)
    set fog_off = IsFogEnabled()
    call FogEnable(true)
    set fog_on_again = IsFogEnabled()

    call FogMaskEnable(false)
    set mask_off = IsFogMaskEnabled()
    call FogMaskEnable(true)
    set mask_on_again = IsFogMaskEnabled()

    set observer_default = IsPlayerObserver(Player(2))
    call SetPlayerState(Player(2), ConvertPlayerState(11), 1)
    set observer_set = IsPlayerObserver(Player(2))
    set observer_state = GetPlayerState(Player(2), ConvertPlayerState(11))
    call SetPlayerState(Player(2), ConvertPlayerState(11), 0)
    set observer_cleared = IsPlayerObserver(Player(2))
    set observer_null = IsPlayerObserver(null)
endfunction
"#;

#[test]
fn aud019_fog_queries_read_the_host_state_the_setters_write() {
    let mut scripted = common::scripted(SCRIPT);
    scripted.vm_mut().run_function("setup").expect("setup");

    assert!(global_bool(&scripted, "fog_default"), "fog of war is on by default (melee)");
    assert!(!global_bool(&scripted, "fog_off"), "FogEnable(false) is visible to IsFogEnabled");
    assert!(global_bool(&scripted, "fog_on_again"));

    assert!(global_bool(&scripted, "mask_default"));
    assert!(!global_bool(&scripted, "mask_off"), "FogMaskEnable(false) is visible to IsFogMaskEnabled");
    assert!(global_bool(&scripted, "mask_on_again"));

    // The two natives are separate state: the last call was FogMaskEnable(true),
    // and fog stayed enabled through the mask toggles.
    assert!(scripted.vm().host.fog_enabled);
    assert!(scripted.vm().host.fog_mask_enabled);
}

#[test]
fn aud019_observer_flag_is_one_value_shared_by_the_state_native() {
    let mut scripted = common::scripted(SCRIPT);
    scripted.vm_mut().run_function("setup").expect("setup");

    assert!(!global_bool(&scripted, "observer_default"), "a fresh slot is not an observer");
    assert!(global_bool(&scripted, "observer_set"), "PLAYER_STATE_OBSERVER=1 makes it one");
    assert_eq!(
        global_int(&scripted, "observer_state"),
        1,
        "GetPlayerState reads the same flag"
    );
    assert!(!global_bool(&scripted, "observer_cleared"), "clearing the state clears the answer");
    assert!(!global_bool(&scripted, "observer_null"), "a null player is not an observer");
}
