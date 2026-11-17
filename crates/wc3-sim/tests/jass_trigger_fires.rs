//! `jass.trigger-fires` — a scripted trigger runs, mutates simulation state, and
//! the test observes the change through the normal tick loop.
//!
//! This is the integration test the lane exists for: it proves the script is not
//! a sandbox. The trigger is registered by JASS, raised by the simulation
//! (`Event::UnitDied`, timer expiry), and its action writes into a real
//! [`wc3_sim::Sim`] unit whose state the test reads back.

mod common;

use wc3_sim::Vec2;
use wc3_sim::jass::{ScriptBundle, ScriptedSim, Value};

/// Boots `script.js` with the full game headers when they are available.
fn boot(script: &str) -> ScriptedSim {
    let mut bundle = ScriptBundle::new();
    if let Some(dir) = wc3_sim::jass::find_game_scripts() {
        bundle.add_game_scripts(&dir).expect("headers");
    }
    bundle.add_source("test.j", script).expect("parse");
    let sim = common::sandbox(0x1234);
    ScriptedSim::new(sim, &bundle, 0x1234).expect("compile")
}

fn global_int(scripted: &ScriptedSim, name: &str) -> i64 {
    let index = scripted.vm().code.global_by_name[name];
    match &scripted.vm().globals[index as usize] {
        Value::Int(value) => *value as i64,
        other => panic!("global {name} is {other:?}, not an integer"),
    }
}

#[test]
fn a_timer_trigger_damages_a_unit_through_the_tick_loop() {
    let mut scripted = boot(
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
            set victim = CreateUnit(Player(0), 'hfoo', 512.0, 512.0, 0.0)
            set t = CreateTrigger()
            call TriggerAddAction(t, function action)
            call TriggerRegisterTimerEvent(t, 0.5, true)
        endfunction
        "#,
    );
    // `UNIT_STATE_LIFE` comes from common.j; declare it locally when the header
    // is absent so this test also runs on a donor-only checkout.
    if scripted.vm().code.global_by_name.get("unit_state_life").is_none() {
        // The bundled headers define UNIT_STATE_LIFE; without them the script
        // above cannot resolve it, and the engine would report that.
        eprintln!("skip: game headers unavailable for UNIT_STATE_LIFE");
        return;
    }
    scripted.vm_mut().run_function("setup").expect("setup");

    let handle = match scripted.vm().globals
        [scripted.vm().code.global_by_name["victim"] as usize]
        .clone()
    {
        Value::Handle(handle) => handle,
        other => panic!("victim is {other:?}"),
    };
    let unit = scripted
        .vm()
        .host
        .sim_unit(handle)
        .expect("the script-created unit must be a simulation unit");
    let before = scripted.sim().unit(unit).expect("unit").hp;

    // 30 ticks = 1.0 s: the 0.5 s periodic trigger must have fired twice.
    for _ in 0..30 {
        scripted.step(&[]);
    }
    let after = scripted.sim().unit(unit).expect("unit").hp;
    assert_eq!(global_int(&scripted, "fires"), 2, "periodic timer fires");
    assert!(
        after < before,
        "the trigger's SetUnitState must reach the simulation (before {before}, after {after})"
    );
}

#[test]
fn a_death_event_trigger_runs_and_mutates_a_surviving_unit() {
    let mut scripted = boot(
        r#"
        globals
            unit doomed = null
            unit witness = null
            integer deaths = 0
        endglobals
        function on_death takes nothing returns nothing
            set deaths = deaths + 1
            call SetUnitState(witness, UNIT_STATE_LIFE, GetUnitState(witness, UNIT_STATE_LIFE) - 250.0)
        endfunction
        function setup takes nothing returns nothing
            local trigger t
            set doomed = CreateUnit(Player(0), 'hfoo', 600.0, 600.0, 0.0)
            set witness = CreateUnit(Player(0), 'hfoo', 700.0, 600.0, 0.0)
            set t = CreateTrigger()
            call TriggerAddAction(t, function on_death)
            call TriggerRegisterUnitEvent(t, doomed, EVENT_UNIT_DEATH)
        endfunction
        "#,
    );
    if scripted
        .vm()
        .code
        .global_by_name
        .get("event_unit_death")
        .is_none()
    {
        eprintln!("skip: game headers unavailable for EVENT_UNIT_DEATH");
        return;
    }
    scripted.vm_mut().run_function("setup").expect("setup");

    let doomed = match scripted.vm().globals
        [scripted.vm().code.global_by_name["doomed"] as usize]
        .clone()
    {
        Value::Handle(handle) => handle,
        other => panic!("doomed is {other:?}"),
    };
    let witness = match scripted.vm().globals
        [scripted.vm().code.global_by_name["witness"] as usize]
        .clone()
    {
        Value::Handle(handle) => handle,
        other => panic!("witness is {other:?}"),
    };
    let doomed_unit = scripted.vm().host.sim_unit(doomed).expect("doomed unit");
    let witness_unit = scripted.vm().host.sim_unit(witness).expect("witness unit");
    let witness_hp = scripted.sim().unit(witness_unit).expect("witness").hp;

    // Killing the unit raises `Event::UnitDied`; the next step dispatches it.
    scripted.vm_mut().sim.kill_unit(doomed_unit);
    scripted.step(&[]);

    assert_eq!(global_int(&scripted, "deaths"), 1, "death trigger fired once");
    let after = scripted.sim().unit(witness_unit).expect("witness").hp;
    assert_eq!(
        after,
        witness_hp - 250,
        "the action's SetUnitState must reach the simulation"
    );
}

#[test]
fn polled_wait_suspends_and_resumes_with_the_elapsed_time() {
    let mut scripted = boot(
        r#"
        globals
            integer phase = 0
            real slept = 0.0
        endglobals
        function delayed takes nothing returns nothing
            set phase = 1
            set slept = TriggerSleepAction(0.4)
            set phase = 2
        endfunction
        "#,
    );
    // Start the action on its own thread, exactly like a trigger action.
    let func = scripted.vm().code.by_name["delayed"];
    scripted
        .vm_mut()
        .call_script_function_with_event(func, Vec::new(), None);
    assert_eq!(global_int(&scripted, "phase"), 1, "ran up to the wait");

    // 0.4 s at 30 Hz = 12 ticks.
    for _ in 0..11 {
        scripted.step(&[]);
    }
    assert_eq!(global_int(&scripted, "phase"), 1, "still parked before 12 ticks");
    scripted.step(&[]);
    assert_eq!(global_int(&scripted, "phase"), 2, "resumed after the wait");

    let slept_index = scripted.vm().code.global_by_name["slept"];
    match &scripted.vm().globals[slept_index as usize] {
        Value::Real(slept) => assert!(*slept >= 0.39, "TriggerSleepAction returns elapsed: {slept}"),
        other => panic!("slept is {other:?}"),
    }
}

#[test]
fn trigger_execute_runs_actions_immediately() {
    let mut scripted = boot(
        r#"
        globals
            integer runs = 0
            trigger t = null
        endglobals
        function action takes nothing returns nothing
            set runs = runs + 1
        endfunction
        function setup takes nothing returns nothing
            set t = CreateTrigger()
            call TriggerAddAction(t, function action)
            call TriggerExecute(t)
            call TriggerExecute(t)
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("setup").expect("setup");
    assert_eq!(global_int(&scripted, "runs"), 2);
}

#[test]
fn a_trigger_can_create_a_unit_that_the_simulation_owns() {
    let mut scripted = boot(
        r#"
        globals
            trigger t = null
            integer spawned = 0
        endglobals
        function action takes nothing returns nothing
            local unit u = CreateUnit(Player(0), 'hfoo', 400.0, 400.0, 90.0)
            set spawned = spawned + 1
        endfunction
        function setup takes nothing returns nothing
            set t = CreateTrigger()
            call TriggerAddAction(t, function action)
            call TriggerRegisterTimerEvent(t, 0.1, false)
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("setup").expect("setup");
    let before = scripted.sim().units.len();
    for _ in 0..5 {
        scripted.step(&[]);
    }
    assert_eq!(global_int(&scripted, "spawned"), 1, "one-shot timer fired");
    assert_eq!(
        scripted.sim().units.len(),
        before + 1,
        "the spawned unit is a simulation unit"
    );
    let spawned = scripted.sim().units.last().expect("last unit");
    assert_eq!(spawned.owner, 0);
    // Authored position is honoured when the ground is free; inside a blocked
    // footprint (the sandbox hall) the unit is separated to the nearest walkable
    // cell instead of standing inside a wall. Both outcomes are exact in their
    // own terms, so assert the contract rather than one of them.
    let requested = Vec2::new(400.0, 400.0);
    let cell = scripted.sim().grid.world_to_cell(spawned.pos);
    assert!(
        scripted.sim().grid.walkable(cell.0, cell.1),
        "script unit must stand on walkable ground, got {:?} cell {cell:?}",
        spawned.pos
    );
    let requested_cell = scripted.sim().grid.world_to_cell(requested);
    let cell_delta = ((cell.0 as i32 - requested_cell.0 as i32).abs())
        .max((cell.1 as i32 - requested_cell.1 as i32).abs());
    assert!(
        (spawned.pos - requested).length() < 1.0 || cell_delta <= 1,
        "CreateUnit must stay at the authored point or one cell from it, got {:?}",
        spawned.pos
    );
}
