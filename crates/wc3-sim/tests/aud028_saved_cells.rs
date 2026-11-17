//! AUD-028 — the five typed `RemoveSaved*` natives delete the same cell.
//!
//! The audit suspected a behavioural defect because
//! `RemoveSavedBoolean/Real/String` share `remove_saved_integer`. Parity check:
//! the engine deletes a hashtable entry by `(parent, child)` key; the stored
//! value's type is not part of the key (`common.j` declares all five removes
//! with the same `(hashtable, integer, integer)` signature, and the engine's
//! `RemoveSaved*` is a key deletion — the type only names the cell the script
//! expects). The runtime keeps one cell map per hashtable keyed by
//! `(parent, child)`, so one handler is the correct implementation.
//!
//! These tests delete one cell of every type and one cross-typed cell. If the
//! shared handler were wrong, at least one of them would keep its value.

mod common;

use wc3_sim::jass::Value;

fn global_int(scripted: &wc3_sim::jass::ScriptedSim, name: &str) -> i64 {
    let index = scripted.vm().code.global_by_name[name];
    scripted.vm().globals[index as usize].int() as i64
}

fn global_real(scripted: &wc3_sim::jass::ScriptedSim, name: &str) -> f32 {
    let index = scripted.vm().code.global_by_name[name];
    scripted.vm().globals[index as usize].real()
}

fn global_bool(scripted: &wc3_sim::jass::ScriptedSim, name: &str) -> bool {
    let index = scripted.vm().code.global_by_name[name];
    matches!(scripted.vm().globals[index as usize], Value::Bool(true))
}

fn global_string(scripted: &wc3_sim::jass::ScriptedSim, name: &str) -> String {
    let index = scripted.vm().code.global_by_name[name];
    scripted.vm().globals[index as usize].text()
}

fn global_handle(scripted: &wc3_sim::jass::ScriptedSim, name: &str) -> Value {
    let index = scripted.vm().code.global_by_name[name];
    scripted.vm().globals[index as usize].clone()
}

#[test]
fn aud028_each_typed_remove_deletes_its_own_cell_and_leave_no_trace() {
    let mut scripted = common::scripted(
        r#"
        globals
            hashtable ht = InitHashtable()
            integer int_after = -1
            boolean bool_after = true
            real real_after = -1.0
            string str_after = "sentinel"
            unit unit_after = null
            boolean int_present = true
            boolean bool_present = true
            boolean real_present = true
            boolean str_present = true
            boolean unit_present = true
        endglobals

        function setup takes nothing returns nothing
            call SaveInteger(ht, 1, 1, 7)
            call SaveBoolean(ht, 1, 2, true)
            call SaveReal(ht, 1, 3, 2.5)
            call SaveStr(ht, 1, 4, "hi")
            call SaveUnitHandle(ht, 1, 5, CreateUnit(Player(0), 'hfoo', 0.0, 0.0, 0.0))

            call RemoveSavedInteger(ht, 1, 1)
            call RemoveSavedBoolean(ht, 1, 2)
            call RemoveSavedReal(ht, 1, 3)
            call RemoveSavedString(ht, 1, 4)
            call RemoveSavedHandle(ht, 1, 5)

            set int_after = LoadInteger(ht, 1, 1)
            set bool_after = LoadBoolean(ht, 1, 2)
            set real_after = LoadReal(ht, 1, 3)
            set str_after = LoadStr(ht, 1, 4)
            set unit_after = LoadUnitHandle(ht, 1, 5)

            set int_present = HaveSavedInteger(ht, 1, 1)
            set bool_present = HaveSavedBoolean(ht, 1, 2)
            set real_present = HaveSavedReal(ht, 1, 3)
            set str_present = HaveSavedString(ht, 1, 4)
            set unit_present = HaveSavedHandle(ht, 1, 5)
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("setup").expect("setup");

    assert_eq!(global_int(&scripted, "int_after"), 0, "integer cell is gone");
    assert!(!global_bool(&scripted, "bool_after"), "boolean cell is gone");
    assert_eq!(global_real(&scripted, "real_after"), 0.0, "real cell is gone");
    assert_eq!(global_string(&scripted, "str_after"), "", "string cell is gone");
    assert!(
        matches!(global_handle(&scripted, "unit_after"), Value::Null),
        "handle cell is gone: {:?}",
        global_handle(&scripted, "unit_after")
    );
    assert!(!global_bool(&scripted, "int_present"));
    assert!(!global_bool(&scripted, "bool_present"));
    assert!(!global_bool(&scripted, "real_present"));
    assert!(!global_bool(&scripted, "str_present"));
    assert!(!global_bool(&scripted, "unit_present"));
}

/// Cross-typed deletion is the behaviour the shared handler predicts: the key
/// is what matters, so `RemoveSavedReal` removes a cell that was saved as an
/// integer.
#[test]
fn aud028_remove_is_a_key_deletion_not_a_typed_deletion() {
    let mut scripted = common::scripted(
        r#"
        globals
            hashtable ht = InitHashtable()
            integer after = -1
            integer neighbour = -1
            boolean present_after = true
        endglobals
        function setup takes nothing returns nothing
            call SaveInteger(ht, 2, 1, 11)
            call SaveInteger(ht, 2, 2, 22)
            call RemoveSavedReal(ht, 2, 1)
            set after = LoadInteger(ht, 2, 1)
            set neighbour = LoadInteger(ht, 2, 2)
            set present_after = HaveSavedInteger(ht, 2, 1)
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("setup").expect("setup");
    assert_eq!(global_int(&scripted, "after"), 0, "the key was removed");
    assert!(!global_bool(&scripted, "present_after"));
    assert_eq!(
        global_int(&scripted, "neighbour"),
        22,
        "only the addressed key is removed"
    );
}

/// The five names really share one handler, which is why the parity above holds
/// for all of them at once.
#[test]
fn aud028_the_five_typed_removes_share_one_handler() {
    let names = [
        "RemoveSavedInteger",
        "RemoveSavedBoolean",
        "RemoveSavedReal",
        "RemoveSavedString",
        "RemoveSavedHandle",
    ];
    let mut handlers = Vec::new();
    for name in names {
        let def = wc3_sim::jass::registry::lookup(name).expect(name);
        assert_eq!(
            def.status,
            wc3_sim::jass::NativeStatus::Implemented,
            "{name} is a real key deletion"
        );
        let entry = wc3_sim::jass::natives::HANDLERS
            .iter()
            .find(|(handler, _)| handler.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| panic!("{name} must be in the dispatch table"));
        handlers.push(entry.1 as usize);
    }
    assert!(
        handlers.windows(2).all(|pair| pair[0] == pair[1]),
        "all five removes dispatch to the same function: {handlers:?}"
    );
}
