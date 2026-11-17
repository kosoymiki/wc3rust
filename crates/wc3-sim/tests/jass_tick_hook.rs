//! `jass.tick-hook` — a map script attached to the simulation runs inside the
//! normal `Sim::step` tick loop, not through a wrapper type.
//!
//! `ScriptedSim` owns its `Sim`; this test proves the other direction: a `Sim`
//! that has a bundle attached with `Sim::attach_script` advances the JASS VM by
//! one 30 Hz tick per `Sim::step`, with the script mutating the same units the
//! caller reads.  A `Sim` without a bundle stays exactly as before (inert).

mod common;

use wc3_sim::jass::{ScriptBundle, Value};
use wc3_sim::{Sim, Vec2};

fn boot(script: &str) -> (Sim, bool) {
    let mut bundle = ScriptBundle::new();
    let mut headers = false;
    if let Some(dir) = wc3_sim::jass::find_game_scripts() {
        bundle.add_game_scripts(&dir).expect("headers");
        headers = true;
    }
    bundle.add_source("hook.j", script).expect("parse");
    let mut sim = common::sandbox(0x77);
    sim.attach_script(&bundle, 0x77).expect("attach script");
    (sim, headers)
}

#[test]
fn an_attached_script_advances_on_every_sim_step() {
    let (mut sim, headers) = boot(
        r#"
        globals
            unit victim = null
            trigger t = null
            integer fires = 0
        endglobals
        function action takes nothing returns nothing
            set fires = fires + 1
            call SetUnitState(victim, UNIT_STATE_LIFE, GetUnitState(victim, UNIT_STATE_LIFE) - 100.0)
        endfunction
        function setup takes nothing returns nothing
            set victim = CreateUnit(Player(0), 'hfoo', 700.0, 700.0, 0.0)
            set t = CreateTrigger()
            call TriggerAddAction(t, function action)
            call TriggerRegisterTimerEvent(t, 0.5, true)
        endfunction
        "#,
    );
    if !headers {
        eprintln!("skip: game headers unavailable for UNIT_STATE_LIFE");
        return;
    }
    assert!(sim.script_attached(), "bundle attached to the sim");
    assert_eq!(sim.script_time(), Some(0.0), "script clock starts at zero");

    // Run the map entry-point body the way `main` would, through the same hook.
    sim.with_script(|vm| vm.run_function("setup"))
        .expect("script attached")
        .expect("setup");
    let (handle, unit) = sim
        .with_script(|vm| {
            let index = vm.code.global_by_name["victim"] as usize;
            let handle = match &vm.globals[index] {
                Value::Handle(handle) => *handle,
                other => panic!("victim is {other:?}"),
            };
            (handle, vm.host.sim_unit(handle).expect("script unit is a sim unit"))
        })
        .expect("script attached");
    // The script-created unit is visible to the caller through the live sim.
    let before = sim.unit(unit).expect("unit in the live sim").hp;
    assert_eq!(sim.unit(unit).unwrap().pos, Vec2::new(700.0, 700.0));

    // 30 steps = 1.0 s at 30 Hz: a 0.5 s periodic timer fires twice.
    let tick0 = sim.tick;
    for _ in 0..30 {
        sim.step(&[]);
    }
    assert_eq!(sim.tick, tick0 + 30, "the simulation grid stayed 30 Hz");
    let after = sim.unit(unit).expect("unit alive").hp;
    assert!(
        after < before,
        "the trigger's SetUnitState must reach the live sim (before {before}, after {after})"
    );
    let fires = sim
        .with_script(|vm| match vm.globals[vm.code.global_by_name["fires"] as usize] {
            Value::Int(fires) => fires,
            _ => -1,
        })
        .expect("script attached");
    assert_eq!(fires, 2, "periodic timer fires on the tick hook (handle {handle})");
    let clock = sim.script_time().unwrap();
    assert!((clock - 1.0).abs() < 1e-6, "script clock advanced 30 ticks: {clock}");
}

#[test]
fn a_sim_without_a_bundle_does_no_script_work() {
    let mut sim = common::sandbox(0x78);
    assert!(!sim.script_attached());
    assert_eq!(sim.script_time(), None);
    assert!(sim.with_script(|_| ()).is_none(), "no VM to borrow");
    let tick0 = sim.tick;
    for _ in 0..60 {
        sim.step(&[]);
    }
    assert_eq!(sim.tick, tick0 + 60);
}
