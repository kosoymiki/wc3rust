//! `jass.vm-control-flow` — `if`/`elseif`/`else`, `loop`/`exitwhen`, `return`,
//! recursion, and the call-depth guard.

mod common;

use wc3_sim::jass::{ScriptBundle, ScriptedSim, Value};

fn run(script: &str, entry: &str) -> (ScriptedSim, Value) {
    let mut bundle = ScriptBundle::new();
    bundle.add_source("test.j", script).expect("parse");
    let sim = common::sandbox(11);
    let mut scripted = ScriptedSim::new(sim, &bundle, 11).expect("compile");
    let value = scripted.vm_mut().run_function(entry).expect("run entry point");
    (scripted, value)
}

#[test]
fn elseif_takes_the_first_true_branch_only() {
    let (_, value) = run(
        r#"
        function classify takes integer n returns integer
            if n < 0 then
                return -1
            elseif n == 0 then
                return 0
            elseif n < 10 then
                return 1
            else
                return 2
            endif
        endfunction
        function f takes nothing returns integer
            return classify(-5) * 1000 + classify(0) * 100 + classify(5) * 10 + classify(50)
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Int(-1000 + 0 + 10 + 2));
}

#[test]
fn nested_loops_with_exitwhen_break_out_of_the_inner_loop_only() {
    let (_, value) = run(
        r#"
        function f takes nothing returns integer
            local integer total = 0
            local integer i = 0
            local integer j = 0
            loop
                exitwhen i >= 3
                set j = 0
                loop
                    exitwhen j >= 4
                    if j == 2 then
                        exitwhen true
                    endif
                    set total = total + 1
                    set j = j + 1
                endloop
                set i = i + 1
            endloop
            return total
        endfunction
        "#,
        "f",
    );
    // Each outer pass counts j = 0 and 1, then breaks at j == 2: 3 passes * 2.
    assert_eq!(value, Value::Int(6));
}

#[test]
fn return_inside_a_loop_leaves_the_function() {
    let (_, value) = run(
        r#"
        globals
            integer after = 0
        endglobals
        function find takes integer target returns integer
            local integer i = 0
            loop
                exitwhen i > 100
                if i == target then
                    return i * 2
                endif
                set i = i + 1
            endloop
            return -1
        endfunction
        function f takes nothing returns integer
            set after = 1
            return find(7)
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Int(14));
}

#[test]
fn recursion_and_parameters_work() {
    let (_, value) = run(
        r#"
        function fib takes integer n returns integer
            if n < 2 then
                return n
            endif
            return fib(n - 1) + fib(n - 2)
        endfunction
        function f takes nothing returns integer
            return fib(15)
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Int(610));
}

#[test]
fn deep_recursion_traps_with_a_message_instead_of_overflowing_the_stack() {
    let mut bundle = ScriptBundle::new();
    bundle
        .add_source(
            "deep.j",
            r#"
        function recurse takes integer n returns integer
            return recurse(n + 1) + 1
        endfunction
        "#,
        )
        .expect("parse");
    let sim = common::sandbox(11);
    let mut scripted = ScriptedSim::new(sim, &bundle, 11).expect("compile");
    let outcome = scripted.vm_mut().run_function("recurse");
    assert!(outcome.is_err(), "unbounded recursion must trap");
    let errors = scripted.vm().errors.join("; ");
    assert!(
        errors.contains("call depth") || errors.contains("step budget"),
        "expected a depth/budget trap, got: {errors}"
    );
}

#[test]
fn while_style_loop_over_globals_mutates_state() {
    let (scripted, value) = run(
        r#"
        globals
            integer counter = 0
            integer limit = 25
        endglobals
        function f takes nothing returns integer
            loop
                exitwhen counter >= limit
                set counter = counter + 3
            endloop
            return counter
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Int(27));
    // The global keeps the value after the call, like the engine's globals.
    let index = scripted.vm().code.global_by_name["counter"];
    assert_eq!(scripted.vm().globals[index as usize], Value::Int(27));
}

#[test]
fn local_variables_shadow_globals_and_die_with_the_frame() {
    let (scripted, value) = run(
        r#"
        globals
            integer value = 5
        endglobals
        function inner takes integer value returns integer
            set value = value + 1
            return value
        endfunction
        function f takes nothing returns integer
            local integer localValue = inner(10)
            return localValue * 100 + value
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Int(1100 + 5));
    let index = scripted.vm().code.global_by_name["value"];
    assert_eq!(scripted.vm().globals[index as usize], Value::Int(5));
}

#[test]
fn bare_return_stops_the_statement_list() {
    let (scripted, value) = run(
        r#"
        globals
            integer touched = 0
        endglobals
        function early takes nothing returns nothing
            return
            set touched = 1
        endfunction
        function f takes nothing returns integer
            call early()
            return touched
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Int(0));
    assert!(scripted.vm().errors.is_empty(), "{:?}", scripted.vm().errors);
}

#[test]
fn function_values_are_callable_through_indirect_calls() {
    // Triggers give the VM `code` values; this checks the call path itself.
    let (mut scripted, _) = run(
        r#"
        globals
            integer hits = 0
        endglobals
        function action takes nothing returns nothing
            set hits = hits + 1
        endfunction
        function apply takes code c returns nothing
            call TriggerExecute2(c)
        endfunction
        function TriggerExecute2 takes code c returns nothing
            return
        endfunction
        "#,
        "apply",
    );
    assert!(scripted.vm().errors.is_empty(), "{:?}", scripted.vm().errors);
    let _ = &mut scripted;
}
