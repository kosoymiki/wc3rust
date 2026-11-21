//! Audit lane H probe: JASS array index validation.
//!
//! JASS arrays are 8192 elements; the interpreter stores them in a
//! `BTreeMap<i64, Value>` and accepts any index (negative included), so a map
//! script can grow one array without a bound.

use wc3_re::jass::{Runtime, parse};

const SOURCE: &str = r#"
globals
    integer array g_a
endglobals

function main takes nothing returns nothing
    set g_a[-5] = 7
    set g_a[9000] = 9
    set g_a[100000] = 11
endfunction
"#;

const GROW: &str = r#"
globals
    integer array g_b
endglobals

function main takes nothing returns nothing
    local integer i = 0
    loop
        exitwhen i >= 200000
        set g_b[i] = i
        set i = i + 1
    endloop
endfunction
"#;

fn array_len(runtime: &Runtime, name: &str) -> Option<usize> {
    match runtime.globals.get(name) {
        Some(wc3_re::jass::vm::Slot::Array(map)) => Some(map.len()),
        _ => None,
    }
}

fn main() {
    let mut runtime = Runtime::new(parse(SOURCE).expect("parse"), 1);
    runtime.run_main().expect("run");
    println!(
        "oob writes accepted: len={:?} a[-5]={:?} a[9000]={:?}",
        array_len(&runtime, "g_a"),
        runtime.globals.get("g_a").map(|slot| match slot {
            wc3_re::jass::vm::Slot::Array(map) => map.get(&-5).cloned(),
            _ => None,
        }),
        runtime.globals.get("g_a").map(|slot| match slot {
            wc3_re::jass::vm::Slot::Array(map) => map.get(&9000).cloned(),
            _ => None,
        })
    );

    let mut grow = Runtime::new(parse(GROW).expect("parse"), 1);
    grow.run_main().expect("run");
    println!(
        "growth: len={:?} steps={} errors={:?}",
        array_len(&grow, "g_b"),
        grow.steps,
        grow.errors
    );
}
