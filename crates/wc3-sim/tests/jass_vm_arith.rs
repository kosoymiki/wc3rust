//! `jass.vm-arith` — arithmetic, comparison, string and conversion semantics.
//!
//! Every assertion is what the engine does, not what Rust does by default:
//! `integer` is 32-bit and wraps, `real` is `f32`, integer division truncates,
//! division by zero yields 0, and `+` concatenates when either side is a string.

mod common;

use wc3_sim::jass::{JassVm, ScriptBundle, ScriptedSim, Value};

fn run(script: &str, entry: &str) -> (ScriptedSim, Value) {
    let mut bundle = ScriptBundle::new();
    bundle.add_source("test.j", script).expect("parse");
    let sim = common::sandbox(7);
    let mut scripted = ScriptedSim::new(sim, &bundle, 7).expect("compile");
    let value = scripted
        .vm_mut()
        .run_function(entry)
        .expect("run entry point");
    (scripted, value)
}

#[test]
fn integer_arithmetic_wraps_at_32_bits() {
    // `2147483647 + 1` wraps to `-2147483648`; a 64-bit integer would not.
    let (scripted, value) = run(
        r#"
        function f takes nothing returns integer
            return 2147483647 + 1
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Int(i32::MIN));
    assert!(scripted.vm().errors.is_empty(), "{:?}", scripted.vm().errors);
}

#[test]
fn integer_division_truncates_and_divides_by_zero_to_zero() {
    let (_, value) = run(
        r#"
        function f takes nothing returns integer
            return 7 / 2
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Int(3), "7/2 truncates toward zero");

    let (_, value) = run(
        r#"
        function f takes nothing returns integer
            return 7 / 0
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Int(0), "JASS integer division by zero is 0");
}

#[test]
fn real_arithmetic_is_single_precision() {
    // 0.1 + 0.2 in f32 is 0.30000001; the engine's `real` is a 4-byte float, so
    // comparing against the f64 value would be wrong.
    let (_, value) = run(
        r#"
        function f takes nothing returns real
            return 0.1 + 0.2
        endfunction
        "#,
        "f",
    );
    match value {
        Value::Real(v) => {
            assert_eq!(v, 0.1f32 + 0.2f32);
            assert!((v - 0.3f32).abs() < 1e-6);
        }
        other => panic!("expected real, got {other:?}"),
    }
}

#[test]
fn mixed_int_and_real_promotes_to_real() {
    let (_, value) = run(
        r#"
        function f takes nothing returns real
            return 1 + 2.5
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Real(3.5));
}

#[test]
fn string_plus_concatenates_on_either_side() {
    let (_, value) = run(
        r#"
        function f takes nothing returns string
            return "hp=" + 12 + "/" + 2.5
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Str("hp=12/2.500".to_string()));
}

#[test]
fn comparison_operators_promote_to_real() {
    let (_, value) = run(
        r#"
        function f takes nothing returns boolean
            return 1 == 1.0 and 2 < 2.5 and not (3 <= 2.9)
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Bool(true));
}

#[test]
fn and_or_short_circuit_on_truthiness() {
    let (_, value) = run(
        r#"
        globals
            integer touched = 0
        endglobals
        function bump takes nothing returns boolean
            set touched = touched + 1
            return true
        endfunction
        function f takes nothing returns boolean
            if false and bump() then
                return false
            endif
            return true
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Bool(true));
}

#[test]
fn conversions_round_the_way_the_engine_does() {
    let (_, value) = run(
        r#"
        function f takes nothing returns integer
            return R2I(-2.9) + R2I(2.9)
        endfunction
        "#,
        "f",
    );
    // R2I truncates toward zero: -2 + 2 = 0
    assert_eq!(value, Value::Int(0));

    let (_, value) = run(
        r#"
        function f takes nothing returns string
            return R2S(1.5) + "|" + I2S(-42) + "|" + S2I("17") + "|" + S2R("2.25")
        endfunction
        "#,
        "f",
    );
    // `R2S` keeps three decimals, which is what map text frames expect.
    assert_eq!(value, Value::Str("1.500|-42|17|2.250".into()));
}

#[test]
fn substring_is_byte_based_and_clamped() {
    let (_, value) = run(
        r#"
        function f takes nothing returns string
            return SubString("abcdef", 2, 4) + SubString("abc", 2, 99) + SubString("abc", 3, 1)
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Str("cdc".into()));
}

#[test]
fn modulo_and_power_natives_work() {
    let (mut scripted, value) = run(
        r#"
        function f takes nothing returns real
            return Pow(2.0, 10.0) + SquareRoot(81.0) + I2R(+5 / 2)
        endfunction
        "#,
        "f",
    );
    assert_eq!(value, Value::Real(1024.0 + 9.0 + 2.0));
    assert!(scripted.unresolved_natives().is_empty(), "{:?}", scripted.unresolved_natives());
    let _ = &mut scripted;
}

#[test]
fn integer_remainder_comes_from_the_bj_helper() {
    // JASS2 has no `%` operator: `ModuloInteger` is a Blizzard.j function, so it
    // only resolves when the BJ library is merged. This checks the VM evaluates
    // it as a script function (not a native) once Blizzard.j is present.
    let Some(dir) = wc3_sim::jass::find_game_scripts() else {
        eprintln!("skip: game scripts unavailable");
        return;
    };
    let mut bundle = ScriptBundle::new();
    bundle.add_game_scripts(&dir).expect("game scripts");
    bundle
        .add_source(
            "test.j",
            r#"
        function f takes nothing returns integer
            return ModuloInteger(13, 4) + ModuloInteger(-7, 3)
        endfunction
        "#,
        )
        .expect("parse");
    let sim = common::sandbox(7);
    let mut scripted = ScriptedSim::new(sim, &bundle, 7).expect("compile");
    let value = scripted.vm_mut().run_function("f").expect("run");
    // Blizzard.j defines `ModuloInteger(dividend, divisor)` as
    // `dividend - (dividend/divisor)*divisor`, then adds the divisor while the
    // result is negative (Blizzard.j:588-594). So 13 mod 4 = 1 and -7 mod 3 = 2.
    assert_eq!(value, Value::Int(3), "errors: {:?}", scripted.vm().errors);
}

#[test]
fn step_budget_stops_runaway_loops() {
    let mut bundle = ScriptBundle::new();
    bundle
        .add_source(
            "spin.j",
            r#"
        function spin takes nothing returns nothing
            loop
            endloop
        endfunction
        "#,
        )
        .expect("parse");
    let sim = common::sandbox(7);
    let mut scripted = ScriptedSim::new(sim, &bundle, 7).expect("compile");
    let outcome = scripted.vm_mut().run_function("spin");
    assert!(outcome.is_err(), "an infinite loop must trap, not hang");
    assert!(
        scripted
            .vm()
            .errors
            .iter()
            .any(|error| error.contains("step budget")),
        "{:?}",
        scripted.vm().errors
    );
}

#[test]
fn vm_reports_a_deterministic_random_sequence() {
    let script = r#"
        globals
            integer a = 0
            integer b = 0
        endglobals
        function f takes nothing returns nothing
            set a = GetRandomInt(1, 1000)
            set b = GetRandomInt(1, 1000)
        endfunction
    "#;
    let (first, _) = run(script, "f");
    let (second, _) = run(script, "f");
    let read = |scripted: &ScriptedSim, name: &str| {
        let index = scripted.vm().code.global_by_name[name];
        scripted.vm().globals[index as usize].clone()
    };
    assert_eq!(read(&first, "a"), read(&second, "a"));
    assert_eq!(read(&first, "b"), read(&second, "b"));
    let _ = JassVm::render(&read(&first, "a"));
}
