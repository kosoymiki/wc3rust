//! `save.script-world` — a mid-game save restores the JASS world, not just the
//! map entities.
//!
//! Mechanic `save.load` for the script lane: WC3 saves restore triggers, timers,
//! `TriggerSleepAction` threads and every script variable. This test boots the
//! real `(2)Justice.w3x` script (`config` + `main`), registers a probe trigger and
//! a probe thread parked on a sleep, saves at tick 5, restores the simulation
//! **from the save bytes alone**, and proves the restored world continues
//! exactly like the original: the suspended thread resumes on its saved wake
//! tick, the periodic trigger fires on the same ticks, and the VM counters match.
//!
//! The probe script is appended after the map script, so the test also proves
//! the save carries the compiled program (a save cannot be silently re-bound to
//! a different `war3map.j`).

mod common;

use std::path::Path;

use wc3_formats::map::{map_mpq_base, parse_units};
use wc3_formats::w3e::Terrain;
use wc3_mpq::MpqArchive;
use wc3_sim::jass::{Object, ScriptBundle, Value};
use wc3_sim::{Sim, Vec2};

const MAP_REL: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

/// Probe appended to the map script. Names are prefixed so they cannot collide
/// with `war3map.j` or `Blizzard.j`.
const PROBE: &str = r#"
globals
    integer h2_save_probe_fires = 0
    integer h2_save_probe_phase = 0
    real h2_save_probe_slept = 0.0
    trigger h2_save_probe_trigger = null
endglobals

function h2_save_probe_action takes nothing returns nothing
    set h2_save_probe_fires = h2_save_probe_fires + 1
endfunction

function h2_save_probe_wait takes nothing returns nothing
    set h2_save_probe_phase = 1
    set h2_save_probe_slept = TriggerSleepAction(0.5)
    set h2_save_probe_phase = 2
endfunction

function h2_save_probe_setup takes nothing returns nothing
    set h2_save_probe_trigger = CreateTrigger()
    call TriggerAddAction(h2_save_probe_trigger, function h2_save_probe_action)
    call TriggerRegisterTimerEvent(h2_save_probe_trigger, 1.0, true)
    call h2_save_probe_wait()
endfunction
"#;

fn justice_path() -> Option<std::path::PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join(MAP_REL);
    path.is_file().then_some(path)
}

fn extracted_script() -> Option<std::path::PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-jass/extracted");
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "j")
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().contains("Justice"))
        })
        .collect();
    entries.sort();
    entries.into_iter().next()
}

fn probe_int(sim: &Sim, name: &str) -> i64 {
    let vm = sim.script().expect("script attached");
    let index = vm.code.global_by_name[name];
    match &vm.globals[index as usize] {
        Value::Int(value) => *value as i64,
        other => panic!("probe global {name} is {other:?}"),
    }
}

fn probe_real(sim: &Sim, name: &str) -> f32 {
    let vm = sim.script().expect("script attached");
    let index = vm.code.global_by_name[name];
    match &vm.globals[index as usize] {
        Value::Real(value) => *value,
        other => panic!("probe global {name} is {other:?}"),
    }
}

fn trigger_count(sim: &Sim) -> usize {
    sim.script()
        .expect("script attached")
        .host
        .objects
        .values()
        .filter(|object| matches!(object, Object::Trigger(_)))
        .count()
}

fn boot_justice_with_probe() -> Option<Sim> {
    let map_path = justice_path()?;
    let script_path = extracted_script()?;
    let scripts_dir = wc3_sim::jass::find_game_scripts()?;

    let base = map_mpq_base(&map_path).expect("mpq base");
    let archive = MpqArchive::open_at(&map_path, base).expect("open map");
    let terrain = Terrain::parse(&archive.read("war3map.w3e").expect("w3e")).expect("terrain");
    let placed = parse_units(&archive.read("war3mapUnits.doo").expect("units")).expect("units");

    let mut sim = Sim::new(0x5A7E, terrain.width, terrain.height);
    sim.passive_income = false;
    for unit in &placed {
        let owner = match unit.player {
            0 => 0u8,
            1 => 1u8,
            other => other.min(23) as u8,
        };
        let type_id = String::from_utf8_lossy(&unit.type_id).to_string();
        let _ = sim.spawn_def_exact(owner, &type_id, Vec2::new(unit.position.x, unit.position.y));
    }

    let mut bundle = ScriptBundle::new();
    bundle.add_game_scripts(&scripts_dir).expect("headers");
    bundle.add_map_script(&script_path).expect("map script");
    bundle.add_source("h2_save_probe.j", PROBE).expect("probe");
    sim.attach_script(&bundle, 0x5A7E).expect("config + main");
    sim.with_script(|vm| vm.run_function("h2_save_probe_setup"))
        .expect("script attached")
        .expect("probe setup");
    Some(sim)
}

#[test]
fn save_restores_the_justice_script_world_from_bytes() {
    let Some(mut sim) = boot_justice_with_probe() else {
        eprintln!("skip: (2)Justice.w3x or its extracted war3map.j is missing");
        return;
    };

    // Five ticks: the sleep (0.5 s = 15 ticks) is parked, the 1 s timer has not
    // fired yet.
    for _ in 0..5 {
        sim.step(&[]);
    }
    assert_eq!(probe_int(&sim, "h2_save_probe_phase"), 1, "thread parked on the wait");
    assert_eq!(probe_int(&sim, "h2_save_probe_fires"), 0, "timer not due yet");
    assert_eq!(sim.script_time(), Some(5.0 / 30.0));

    let saved_triggers = trigger_count(&sim);
    assert!(saved_triggers >= 20, "Justice registers its triggers: {saved_triggers}");
    let saved_functions = sim.script().unwrap().code.functions.len();
    let saved_threads = sim.script().unwrap().threads.len();
    assert!(saved_threads >= 1, "the probe thread is parked at save time");

    let bytes = sim.to_save_bytes();
    assert_eq!(
        u16::from_le_bytes([bytes[4], bytes[5]]),
        wc3_sim::save::SAVE_FORMAT_VERSION,
        "a mid-game save carries the current format version"
    );
    let mut restored = Sim::from_save(&bytes).expect("restore the script world from bytes");
    assert!(restored.script_attached(), "the VM came back from the save");
    assert_eq!(restored.save_digest(), sim.save_digest(), "byte-identical re-encode");

    // The parked thread kept its frames, locals and wake tick.
    let (wake_tick, frames, instruction_pointers) = {
        let vm = restored.script().expect("script attached");
        let thread = vm
            .threads
            .iter()
            .find(|thread| thread.name.contains("h2_save_probe"))
            .expect("the probe thread is restored");
        (
            thread.wake_tick,
            thread.frames.len(),
            thread.frames.iter().map(|frame| frame.pc).collect::<Vec<_>>(),
        )
    };
    assert_eq!(wake_tick, Some(15), "wake tick survives the save");
    assert_eq!(frames, 2, "setup + wait frames survive the save");
    assert!(
        instruction_pointers.iter().all(|pc| *pc > 0),
        "each frame keeps its instruction pointer: {instruction_pointers:?}"
    );
    assert_eq!(restored.script().unwrap().code.functions.len(), saved_functions);
    assert_eq!(restored.script().unwrap().threads.len(), saved_threads);
    assert_eq!(trigger_count(&restored), saved_triggers, "every trigger object restored");

    // Tick both worlds to the same instant. The original is the reference: the
    // restored world must match it at every checkpoint.
    for _ in 0..10 {
        sim.step(&[]);
        restored.step(&[]);
    }
    assert_eq!(probe_int(&restored, "h2_save_probe_phase"), 2, "sleep resumed at tick 15");
    assert_eq!(probe_int(&sim, "h2_save_probe_phase"), 2);
    assert!(
        (probe_real(&restored, "h2_save_probe_slept") - 0.5).abs() < 1e-3,
        "TriggerSleepAction returns the elapsed time: {}",
        probe_real(&restored, "h2_save_probe_slept")
    );

    for _ in 0..15 {
        sim.step(&[]);
        restored.step(&[]);
    }
    assert_eq!(probe_int(&restored, "h2_save_probe_fires"), 1, "1 s timer fired at tick 30");
    assert_eq!(probe_int(&sim, "h2_save_probe_fires"), 1);
    assert_eq!(restored.script_time(), sim.script_time());
    assert_eq!(restored.save_digest(), sim.save_digest(), "the worlds stay identical");

    for _ in 0..30 {
        sim.step(&[]);
        restored.step(&[]);
    }
    assert_eq!(probe_int(&restored, "h2_save_probe_fires"), 2, "periodic timer keeps firing");
    assert_eq!(probe_int(&sim, "h2_save_probe_fires"), 2);
    assert_eq!(restored.save_digest(), sim.save_digest(), "still identical at tick 60");

    // Peak RSS of the heavy Justice load + save/restore (device OOM budget:
    // `VmHWM` is the process high-water mark, not the current RSS).
    if let Ok(status) = std::fs::read_to_string("/proc/self/status")
        && let Some(line) = status.lines().find(|line| line.starts_with("VmHWM:"))
    {
        eprintln!("SAVE_SCRIPT_PEAK {line}");
    }
}

#[test]
fn save_without_a_script_round_trips_and_stays_scriptless() {
    let mut sim = common::sandbox(0x5A7F);
    sim.step(&[]);
    assert!(!sim.script_attached());
    let bytes = sim.to_save_bytes();
    assert_eq!(
        u16::from_le_bytes([bytes[4], bytes[5]]),
        wc3_sim::save::SAVE_FORMAT_VERSION
    );
    let restored = Sim::from_save(&bytes).expect("round trip");
    assert!(!restored.script_attached(), "no script section means no VM");
    assert_eq!(restored.state_hash(), sim.state_hash());
    assert_eq!(restored.to_save_bytes(), bytes);
}
