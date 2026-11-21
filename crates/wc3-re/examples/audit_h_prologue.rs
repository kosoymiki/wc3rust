//! Audit lane H probe: boot the shipped Prologue01 chapter exactly the way
//! `Game::reset_map_runtime` does (32 MB stack, `Engine::boot` → `enter_map`),
//! and report what the script engine actually registered/executed.

use std::path::{Path, PathBuf};

use wc3_re::engine::{Engine, EngineConfig};
use wc3_re::jass::vm::TriggerEvent;
use wc3_re::mapfiles::MapBundle;
use wc3_re::storm::StormFs;

fn main() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let install = repo.join("analysis/agent-assets/wc3-install");
    println!("install={} exists={}", install.display(), install.is_dir());
    let storm = StormFs::open_wc3_install(&install).expect("open install");
    println!("layers={}", storm.layers().len());
    let mut candidates: Vec<String> = storm
        .listfile()
        .into_iter()
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            lower.contains("prologue01") && (lower.ends_with(".w3m") || lower.ends_with(".w3x"))
        })
        .collect();
    candidates.sort();
    println!("prologue candidates={candidates:?}");
    let Some(inner) = candidates.first().cloned() else {
        println!("Prologue01 not in the mounted archives");
        return;
    };
    let bytes = storm.read(&inner).expect("read chapter");
    println!("inner={inner} bytes={}", bytes.len());
    let tmp = PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/audit_h_prologue01.w3m");
    std::fs::write(&tmp, &bytes).expect("write temp map");
    let bundle = MapBundle::open(&tmp).expect("open chapter");
    println!(
        "bundle: script={} entries={} objects={} wts={} errors={:?}",
        bundle.script.as_ref().map_or(0, String::len),
        bundle.entries.len(),
        bundle.object_entries(),
        bundle.wts.as_ref().map_or(0, |wts| wts.entries.len()),
        bundle.errors
    );
    if let Some(info) = &bundle.info {
        println!(
            "w3i: version={} name={:?} players={:?} tail={} bytes",
            info.version,
            info.name,
            info.recommended_players,
            info.raw_tail.len()
        );
    }
    let script = bundle.script.clone().unwrap_or_default();
    for needle in [
        "function config",
        "TriggerSleepAction",
        "SetPlayerAlliance",
        "IsUnitOwnedByPlayer",
        "IsUnitType",
        "TriggerRegisterEnterRegion",
        "TriggerRegisterDeathEvent",
        "EVENT_PLAYER_UNIT_DEATH",
        "GetLocalPlayer",
        "GetPlayerController",
        "CreateAllUnits",
        "CreateAllDestructables",
        "CreateDestructable",
        "CreateDestructableZ",
        "GetLocalizedString",
        "SubString",
        "StringLength",
        "IsUnitAlly",
        "IsUnitEnemy",
        "SetPlayerTechResearched",
        "TriggerRegisterPlayerUnitEvent",
        "TriggerRegisterEnterRect",
        "TriggerRegisterTimerEvent",
    ] {
        println!("script contains {needle:?}: {}", script.matches(needle).count());
    }

    // Same stack size the game uses for the map script thread.
    let handle = std::thread::Builder::new()
        .name("audit-h-prologue".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            let mut config = EngineConfig::new(&install);
            config.map = Some(tmp.clone());
            let mut engine = Engine::boot(config).expect("boot");
            let report = engine.script_report();
            println!("map_spawn={:?}", engine.map_spawn());
            {
                let runtime = engine.runtime().expect("runtime");
                let mut death = 0usize;
                let mut enter = 0usize;
                let mut leave = 0usize;
                let mut player_unit = 0usize;
                let mut unit_event = 0usize;
                let mut timer = 0usize;
                let mut other = 0usize;
                for trigger in runtime.world.triggers.values() {
                    for event in &trigger.events {
                        match event {
                            TriggerEvent::Death { .. } => death += 1,
                            TriggerEvent::EnterRegion { .. } => enter += 1,
                            TriggerEvent::LeaveRegion { .. } => leave += 1,
                            TriggerEvent::PlayerUnit { .. } => player_unit += 1,
                            TriggerEvent::UnitEvent { .. } => unit_event += 1,
                            TriggerEvent::Timer { .. } | TriggerEvent::TimerExpire { .. } => timer += 1,
                            _ => other += 1,
                        }
                    }
                }
                println!(
                    "after_boot: main_ok={} functions={} triggers={} actions={} steps={} sleeps={} parked={} errors={} unimplemented={}",
                    report.main_ok,
                    report.functions,
                    report.triggers,
                    report.trigger_actions,
                    runtime.steps,
                    runtime.sleeps,
                    runtime.threads.len(),
                    runtime.errors.len(),
                    runtime.unimplemented.len()
                );
                println!(
                    "trigger events: death={death} enter_region={enter} leave_region={leave} player_unit={player_unit} unit_event={unit_event} timer={timer} other={other}"
                );
                println!(
                    "config_calls={:?} local_player={} wildcard={} player1_controller={} player1_name={:?}",
                    runtime.call_counts.get("config"),
                    runtime.world.local_player,
                    runtime.world.local_player_any,
                    runtime.world.players[1].controller,
                    runtime.world.players[1].name
                );
                if let Some(error) = runtime.errors.first() {
                    println!("first error: {error}");
                }
                let mut unimplemented: Vec<_> = runtime.unimplemented.iter().collect();
                unimplemented.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
                println!("top unimplemented: {:?}", &unimplemented[..unimplemented.len().min(10)]);
            }
            for _ in 0..600 {
                engine.tick(1.0 / 30.0);
            }
            let runtime = engine.runtime().expect("runtime");
            let executed = runtime
                .world
                .triggers
                .values()
                .filter(|trigger| trigger.exec_count > 0)
                .count();
            let exec_total: u64 = runtime
                .world
                .triggers
                .values()
                .map(|trigger| u64::from(trigger.exec_count))
                .sum();
            println!(
                "after_600_ticks: steps={} sleeps={} wakes={} parked={} executed_triggers={executed} exec_total={exec_total} errors={}",
                runtime.steps,
                runtime.sleeps,
                runtime.wakes,
                runtime.threads.len(),
                runtime.errors.len()
            );
            for error in runtime.errors.iter().take(5) {
                println!("runtime error: {error}");
            }
            // What `config` would have established if the engine called it first.
            let _ = engine.runtime_mut().expect("runtime").run_function("config");
            let runtime = engine.runtime().expect("runtime");
            println!("manual config: calls={:?}", runtime.call_counts.get("config"));
            for player in runtime.world.players.iter().take(8) {
                println!(
                    "  slot={} name={:?} controller={} start={} team={} race={}",
                    player.index,
                    player.name,
                    player.controller,
                    player.start_location,
                    player.team,
                    player.race
                );
            }
        })
        .expect("spawn");
    handle.join().expect("join");
}
