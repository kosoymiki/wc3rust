//! Audit lane H probe: the VM `max_steps` budget is runtime-wide and cumulative.
//!
//! `Engine::enter_map` sets `runtime.max_steps = 50_000_000` once (engine.rs),
//! and `Runtime::steps` is only ever incremented. This probe (a) measures the
//! real budget consumed by a campaign boot and 600 ticks, and (b) demonstrates
//! that once the budget is spent every later script entry fails permanently.

use std::path::Path;

use wc3_re::engine::{Engine, EngineConfig};
use wc3_re::jass::{Runtime, parse};

const SPIN: &str = r#"
globals
    integer g = 0
endglobals

function spin takes nothing returns nothing
    local integer i = 0
    loop
        exitwhen i >= 300
        set g = g + 1
        set i = i + 1
    endloop
endfunction

function ping takes nothing returns nothing
    set g = g + 1
endfunction
"#;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
    let map = root.join("Maps/Campaign/Human04.w3m");
    let mut config = EngineConfig::new(&root);
    config.map = Some(map);
    let mut engine = Engine::boot(config).expect("boot");
    let boot_steps = engine.runtime().expect("runtime").steps;
    for _ in 0..600 {
        engine.tick(1.0 / 30.0);
    }
    let runtime = engine.runtime().expect("runtime");
    println!(
        "campaign boot: steps={boot_steps} after_600_ticks={} delta={} max_steps={} sleeps={} parked={} errors={} main_ok={}",
        runtime.steps,
        runtime.steps - boot_steps,
        runtime.max_steps,
        runtime.sleeps,
        runtime.threads.len(),
        runtime.errors.len(),
        engine.script_report().main_ok
    );

    // The budget semantics: one shared counter, no per-call or per-tick reset.
    let mut tiny = Runtime::new(parse(SPIN).expect("parse"), 1);
    tiny.max_steps = 200;
    println!("tiny spin: {:?}", tiny.run_function("spin"));
    println!("tiny ping after spin: {:?}", tiny.run_function("ping"));
    tiny.max_steps = u64::MAX;
    println!("tiny ping after raising the cap: {:?}", tiny.run_function("ping"));
}
