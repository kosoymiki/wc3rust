//! Audit lane H probe: is the map script's `config` entry point executed?
//!
//! Retail Game.dll runs `config` before `main`
//! (`analysis/agent-re/formats/ENGINE_MAP.md` §4.1: 0x6f2ccf00 RunConfig /
//! 0x6f28dc58 caller, then 0x6f2ccf30 RunMain / 0x6f28ec7c caller).
//! `Engine::enter_map` only calls `runtime.run_main()`.

use std::path::Path;

use wc3_re::engine::{Engine, EngineConfig};

fn dump(label: &str, runtime: &wc3_re::jass::Runtime) {
    println!(
        "{label}: steps={} sleeps={} errors={} unimplemented={} config_calls={:?}",
        runtime.steps,
        runtime.sleeps,
        runtime.errors.len(),
        runtime.unimplemented.len(),
        runtime.call_counts.get("config")
    );
    for player in runtime.world.players.iter().take(6) {
        println!(
            "  {label} slot={} name={:?} controller={} start={} team={} race={}",
            player.index,
            player.name,
            player.controller,
            player.start_location,
            player.team,
            player.race
        );
    }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
    let map = root.join("Maps/Campaign/Human04.w3m");
    println!(
        "install={} map={} exists={}",
        root.display(),
        map.display(),
        map.is_file()
    );

    let mut config = EngineConfig::new(&root);
    config.map = Some(map);
    let mut engine = Engine::boot(config).expect("boot");

    let report = engine.script_report();
    println!(
        "boot: main_ok={} functions={} triggers={} actions={}",
        report.main_ok, report.functions, report.triggers, report.trigger_actions
    );
    {
        let runtime = engine.runtime().expect("runtime");
        println!(
            "program has config function: {}",
            runtime
                .function_names()
                .iter()
                .any(|name| name.eq_ignore_ascii_case("config"))
        );
        dump("after_boot", runtime);
    }

    // What the script itself does when the engine calls `config` in retail.
    let result = engine
        .runtime_mut()
        .expect("runtime")
        .run_function("config");
    println!("manual config result: {result:?}");
    let runtime = engine.runtime().expect("runtime");
    dump("after_manual_config", runtime);
}
