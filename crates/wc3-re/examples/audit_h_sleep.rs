//! Audit lane H probe: `TriggerSleepAction` suspension semantics.
//!
//! JASS has one thread per script activation: a wait suspends the whole call
//! chain. This probe calls a user function that waits and checks whether the
//! caller continues before the wait completes.

use wc3_re::jass::{Runtime, parse};

const SOURCE: &str = r#"
native TriggerSleepAction takes real timeout returns nothing

globals
    integer g_stage = 0
endglobals

function inner takes nothing returns nothing
    call TriggerSleepAction(2.0)
    set g_stage = 2
endfunction

function main takes nothing returns nothing
    set g_stage = 1
    call inner()
    set g_stage = 3
endfunction
"#;

fn stage(runtime: &Runtime) -> i64 {
    runtime
        .globals
        .get("g_stage")
        .map(|slot| slot.scalar().as_int())
        .unwrap_or(-1)
}

fn main() {
    let program = parse(SOURCE).expect("parse");
    let mut runtime = Runtime::new(program, 1);
    let result = runtime.run_main();
    println!(
        "after main: result={result:?} stage={} sleeps={} parked={} wakes={}",
        stage(&runtime),
        runtime.sleeps,
        runtime.threads.len(),
        runtime.wakes
    );
    let ran = runtime.advance(2.0);
    println!(
        "after advance(2.0): ran={ran} stage={} parked={} wakes={} errors={:?}",
        stage(&runtime),
        runtime.threads.len(),
        runtime.wakes,
        runtime.errors
    );
}
