//! Coroutine suspension: `TriggerSleepAction`, `TimerStart` and
//! `TriggerRegisterTimerEvent` must suspend and resume a script thread across
//! ticks instead of running the whole sequence inside one call.
//!
//! Every test drives the VM tick by tick and asserts *which* tick the
//! continuation lands on, because that is the observable difference: before the
//! VM grew threads, a `TriggerSleepAction` was an unknown native and the code
//! after it ran in the same tick as the call.

use wc3_re::jass::vm::{Runtime, Slot, Value};

const DT: f64 = 1.0 / 30.0;

/// The natives these tests drive, declared exactly as `Scripts\common.j`
/// declares them. The VM resolves calls through the program's own native
/// declarations, so a test script carries the surface it depends on.
const PROLOGUE: &str = r#"
native CreateTrigger takes nothing returns trigger
native TriggerAddAction takes trigger whichTrigger, code actionFunc returns nothing
native TriggerExecute takes trigger whichTrigger returns nothing
native TriggerSleepAction takes real timeout returns nothing
native TriggerRegisterTimerEvent takes trigger whichTrigger, real timeout, boolean periodic returns boolean
native CreateTimer takes nothing returns timer
native TimerStart takes timer whichTimer, real timeout, boolean periodic, code handlerFunc returns nothing
native GetExpiredTimer takes nothing returns timer
native GetGameTime takes nothing returns real
"#;

fn runtime(script: &str) -> Runtime {
    let source = format!("{PROLOGUE}\n{script}");
    let runtime = Runtime::from_sources(&[("test.j", &source)], 0x5EED).expect("script compiles");
    assert!(
        runtime.errors.is_empty(),
        "the script compiled with no init errors: {:?}",
        runtime.errors
    );
    runtime
}

fn int(rt: &Runtime, name: &str) -> i64 {
    rt.globals
        .get(name)
        .map(Slot::scalar)
        .map(|value| value.as_int())
        .unwrap_or(-1)
}

fn real(rt: &Runtime, name: &str) -> f64 {
    rt.globals
        .get(name)
        .map(Slot::scalar)
        .map(|value| value.as_real())
        .unwrap_or(f64::NAN)
}

fn set_int(rt: &mut Runtime, name: &str, value: i64) {
    rt.globals.insert(name.to_string(), Slot::Scalar(Value::Int(value)));
}

fn ticks(rt: &mut Runtime, count: usize) {
    for tick in 1..=count {
        set_int(rt, "tick", tick as i64);
        rt.advance(DT);
    }
}

/// The headline test: `TriggerSleepAction(2.0)` suspends the thread, the
/// continuation runs on the tick whose clock reaches +2.0 s, and the mission
/// clock advanced in the meantime.
#[test]
fn trigger_sleep_resumes_on_the_right_tick() {
    const SCRIPT: &str = r#"
        globals
            real fired_at = -1.0
            integer fired_tick = -1
            integer fires = 0
            integer tick = 0
        endglobals

        function Continue takes nothing returns nothing
            set fired_at = GetGameTime()
            set fired_tick = tick
            set fires = fires + 1
        endfunction

        function Actions takes nothing returns nothing
            call TriggerSleepAction(2.0)
            call Continue()
        endfunction

        function main takes nothing returns nothing
            local trigger t = CreateTrigger()
            call TriggerAddAction(t, function Actions)
            call TriggerExecute(t)
        endfunction
    "#;
    let mut rt = runtime(SCRIPT);
    rt.run_main().expect("main");

    // The call suspended: nothing after `TriggerSleepAction` ran, and the VM
    // holds exactly one parked thread.
    assert_eq!(int(&rt, "fires"), 0, "the continuation must not run in the calling tick");
    assert_eq!(rt.sleeps, 1, "one sleep request was recorded");
    assert_eq!(rt.threads.len(), 1, "the thread is parked, not discarded");
    assert_eq!(rt.wakes, 0, "nothing woke yet");

    let mut fired_on = None;
    for tick in 1..=90 {
        set_int(&mut rt, "tick", tick);
        rt.advance(DT);
        if fired_on.is_none() && int(&rt, "fires") > 0 {
            fired_on = Some(tick);
        }
    }
    // 2.0 s at 30 Hz is tick 60. The clock is advanced by `advance`, so the
    // mission clock moved with the thread.
    assert_eq!(fired_on, Some(60), "the continuation fires on tick 60, not at tick 1");
    assert_eq!(int(&rt, "fired_tick"), 60, "the thread observed tick 60");
    assert!(
        (real(&rt, "fired_at") - 2.0).abs() < 1e-6,
        "GetGameTime() at the continuation is 2.0 s, got {}",
        real(&rt, "fired_at")
    );
    assert_eq!(int(&rt, "fires"), 1, "the continuation runs exactly once");
    assert_eq!(rt.threads.len(), 0, "the thread finished");
    assert_eq!(rt.wakes, 1, "one wake was recorded");
    assert!(
        (rt.world.game_time - 90.0 * DT).abs() < 1e-9,
        "the mission clock advanced with the ticks: {}",
        rt.world.game_time
    );
}

/// A sleep must also suspend the *rest of a trigger's action list*: actions are
/// one thread, not one thread each.
#[test]
fn sleep_suspends_the_remaining_trigger_actions() {
    const SCRIPT: &str = r#"
        globals
            integer first = 0
            integer after_sleep = 0
            integer last_action = 0
            integer tick = 0
        endglobals

        function A1 takes nothing returns nothing
            set first = 1
            call TriggerSleepAction(1.0)
            set after_sleep = 1
        endfunction

        function A2 takes nothing returns nothing
            set last_action = 1
        endfunction

        function main takes nothing returns nothing
            local trigger t = CreateTrigger()
            call TriggerAddAction(t, function A1)
            call TriggerAddAction(t, function A2)
            call TriggerExecute(t)
        endfunction
    "#;
    let mut rt = runtime(SCRIPT);
    rt.run_main().expect("main");
    assert_eq!(int(&rt, "first"), 1, "the actions before the sleep ran");
    assert_eq!(int(&rt, "after_sleep"), 0, "the code after the sleep waits");
    assert_eq!(
        int(&rt, "last_action"),
        0,
        "the next action of the same trigger waits for the sleeping thread"
    );

    ticks(&mut rt, 29);
    assert_eq!(int(&rt, "after_sleep"), 0, "29/30 s is not yet 1 s");
    assert_eq!(int(&rt, "last_action"), 0, "the queued action still waits");
    rt.advance(DT);
    assert_eq!(int(&rt, "after_sleep"), 1, "tick 30 (1.0 s) resumes the thread");
    assert_eq!(
        int(&rt, "last_action"),
        1,
        "the next action of the same trigger runs on the wake tick"
    );
    assert_eq!(rt.threads.len(), 0, "the trigger thread finished");
}

/// `TriggerRegisterTimerEvent(trig, 0.5, true)` fires once per elapsed period
/// *across ticks*; the VM tick loop is what makes it fire at all.
#[test]
fn periodic_timer_event_fires_across_ticks() {
    const SCRIPT: &str = r#"
        globals
            integer beats = 0
        endglobals

        function Beat takes nothing returns nothing
            set beats = beats + 1
        endfunction

        function main takes nothing returns nothing
            local trigger t = CreateTrigger()
            call TriggerAddAction(t, function Beat)
            call TriggerRegisterTimerEvent(t, 0.5, true)
        endfunction
    "#;
    let mut rt = runtime(SCRIPT);
    rt.run_main().expect("main");
    assert_eq!(int(&rt, "beats"), 0, "registering a timer event does not fire it");

    // 0.25 s steps, 0.5 s period: fires on ticks 2, 4, 6, 8, 10.
    let mut fired = Vec::new();
    for tick in 1..=10 {
        let before = int(&rt, "beats");
        rt.advance(0.25);
        if int(&rt, "beats") > before {
            fired.push(tick);
        }
    }
    assert_eq!(fired, vec![2, 4, 6, 8, 10], "periodic events fire on their ticks");
    assert_eq!(rt.world.timer_events.len(), 1, "the schedule is recorded");
    assert_eq!(rt.world.timer_events[0].fires, 5, "five periods elapsed");
}

/// A one-shot timer event fires exactly once, even over a long run.
#[test]
fn one_shot_timer_event_fires_once() {
    const SCRIPT: &str = r#"
        globals
            integer beats = 0
        endglobals

        function Beat takes nothing returns nothing
            set beats = beats + 1
        endfunction

        function main takes nothing returns nothing
            local trigger t = CreateTrigger()
            call TriggerAddAction(t, function Beat)
            call TriggerRegisterTimerEvent(t, 0.75, false)
        endfunction
    "#;
    let mut rt = runtime(SCRIPT);
    rt.run_main().expect("main");
    for _ in 0..300 {
        rt.advance(DT);
    }
    assert_eq!(int(&rt, "beats"), 1, "a one-shot timer event fires once");
    assert!(rt.world.timer_events[0].done, "the schedule is spent");
}

/// `TimerStart` callbacks run on their period and `GetExpiredTimer()` answers
/// the timer inside the callback.
#[test]
fn timer_start_callback_runs_periodically() {
    const SCRIPT: &str = r#"
        globals
            timer clock = null
            integer pulses = 0
            integer expired_matches = 0
        endglobals

        function Pulse takes nothing returns nothing
            set pulses = pulses + 1
            if GetExpiredTimer() == clock then
                set expired_matches = expired_matches + 1
            endif
        endfunction

        function main takes nothing returns nothing
            set clock = CreateTimer()
            call TimerStart(clock, 0.5, true, function Pulse)
        endfunction
    "#;
    let mut rt = runtime(SCRIPT);
    rt.run_main().expect("main");
    assert_eq!(rt.world.timers.len(), 1);
    // 0.5 s steps: one pulse per tick.
    for _ in 0..6 {
        rt.advance(0.5);
    }
    assert_eq!(int(&rt, "pulses"), 6, "six periods ran");
    assert_eq!(
        int(&rt, "expired_matches"),
        6,
        "GetExpiredTimer() returned the running timer"
    );
}

/// Two runtimes driven with the same ticks suspend and resume identically: the
/// scheduler is deterministic.
#[test]
fn suspension_is_deterministic() {
    const SCRIPT: &str = r#"
        globals
            integer fired_tick = -1
            integer tick = 0
        endglobals

        function Actions takes nothing returns nothing
            call TriggerSleepAction(0.5)
            set fired_tick = tick
        endfunction

        function main takes nothing returns nothing
            local trigger t = CreateTrigger()
            call TriggerAddAction(t, function Actions)
            call TriggerExecute(t)
        endfunction
    "#;
    let run = || {
        let mut rt = runtime(SCRIPT);
        rt.run_main().expect("main");
        ticks(&mut rt, 30);
        int(&rt, "fired_tick")
    };
    assert_eq!(run(), run(), "same script, same tick, same wake");
    assert_eq!(run(), 15, "0.5 s at 30 Hz is tick 15");
}
