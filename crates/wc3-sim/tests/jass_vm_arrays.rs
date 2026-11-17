//! `jass.vm-arrays` — global arrays, array locals (`JASSHelper`-style), the
//! 0..8191 index window and array-backed tables used by map scripts.

mod common;

use wc3_sim::jass::{ScriptBundle, ScriptedSim, Value};

fn run(script: &str, entry: &str) -> (ScriptedSim, Value) {
    let mut bundle = ScriptBundle::new();
    bundle.add_source("test.j", script).expect("parse");
    let sim = common::sandbox(3);
    let mut scripted = ScriptedSim::new(sim, &bundle, 3).expect("compile");
    let value = scripted.vm_mut().run_function(entry).expect("run entry point");
    (scripted, value)
}

#[test]
fn global_arrays_store_and_read_by_index() {
    let (_, value) = run(
        r#"
        globals
            integer array values
            string array names
        endglobals
        function f takes nothing returns string
            set values[0] = 10
            set values[7] = 70
            set names[7] = "seven"
            return I2S(values[0] + values[7]) + names[7]
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Str("80seven".into()));
}

#[test]
fn unset_array_slots_read_as_zero() {
    let (_, value) = run(
        r#"
        globals
            integer array values
        endglobals
        function f takes nothing returns integer
            return values[4095]
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Int(0));
}

#[test]
fn indices_outside_zero_to_8191_collapse_to_slot_zero() {
    // The engine guards the array window instead of trapping; slot 0 is what it
    // addresses. A 64-bit index would silently write elsewhere.
    let (_, value) = run(
        r#"
        globals
            integer array values
        endglobals
        function f takes nothing returns integer
            set values[0] = 1
            set values[9000] = 5
            return values[0]
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Int(5));

    let (_, value) = run(
        r#"
        globals
            integer array values
        endglobals
        function f takes nothing returns integer
            set values[0] = 3
            return values[-1]
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Int(3));
}

#[test]
fn array_index_can_be_an_expression() {
    let (_, value) = run(
        r#"
        globals
            integer array values
        endglobals
        function at takes integer i returns integer
            return values[i * 2 + 1]
        endfunction
        function f takes nothing returns integer
            set values[5] = 55
            return at(2)
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Int(55));
}

#[test]
fn array_locals_are_promoted_like_jasshelper_does() {
    // JASS2 has no local arrays; vJass output relies on the compiler promoting
    // them to globals. The VM does the same so the shipped scripts run.
    let (scripted, value) = run(
        r#"
        function f takes nothing returns integer
            local integer array table
            set table[0] = 4
            set table[1] = 6
            return table[0] * table[1]
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Int(24));
    assert!(scripted.vm().errors.is_empty(), "{:?}", scripted.vm().errors);
}

#[test]
fn arrays_survive_across_function_calls() {
    let (_, value) = run(
        r#"
        globals
            integer array counters
            integer index = 0
        endglobals
        function bump takes nothing returns nothing
            set counters[index] = counters[index] + 1
            set index = index + 1
        endfunction
        function f takes nothing returns integer
            call bump()
            call bump()
            call bump()
            return counters[0] * 100 + counters[1] * 10 + counters[2]
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Int(111));
}

#[test]
fn string_arrays_hold_strings_not_numbers() {
    let (_, value) = run(
        r#"
        globals
            string array label
        endglobals
        function f takes nothing returns string
            set label[3] = "three"
            return label[3] + "|" + label[4]
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Str("three|".into()));
}
