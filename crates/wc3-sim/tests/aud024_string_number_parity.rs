//! AUD-024 — `S2I`/`S2R` parity with the shipped engine.
//!
//! Parity source (observed 2026-11-17):
//! `https://raw.githubusercontent.com/lep/jassdoc/master/common.j`
//! (JassDoc annotation of the 1.32 `common.j`; the block above
//! `native S2I takes string s returns integer` and the block above
//! `native S2R takes string s returns real`).
//!
//! Documented engine behaviour, quoted from those blocks:
//! * `S2I`: "For values too big or too small, returns max/min integer
//!   respectively. For an empty string or text that doesn't start with a
//!   number, returns 0." Examples: `S2I("") == 0`, `S2I("-123") == -123`,
//!   `S2I("123abc") == 123`, `S2I("abc123") == 0`.
//! * Both: "The parser stops at the first non-number character `[0-9.]`" and
//!   "does not support comma `,` as a decimal point"; `S2R(".123asd") == 0.123`.
//! * `S2R`: "Returns 0 for: values too big or too small, an empty string or
//!   text that doesn't start with a number."
//!
//! The previous implementation (`str::parse().unwrap_or(0)`) diverged on every
//! junk-suffix and decimal-point case — it returned 0 where the engine returns
//! the numeric prefix. These cases are the golden set; the exactness notes are
//! limited to what the source documents (a second decimal point and a leading
//! `+` are inferred from the same `[0-9.]` scan rule and are marked as such).

mod common;

use wc3_sim::jass::Value;

fn int_of(scripted: &wc3_sim::jass::ScriptedSim, name: &str) -> i32 {
    let index = scripted.vm().code.global_by_name[name];
    scripted.vm().globals[index as usize].int()
}

fn real_of(scripted: &wc3_sim::jass::ScriptedSim, name: &str) -> f32 {
    let index = scripted.vm().code.global_by_name[name];
    scripted.vm().globals[index as usize].real()
}

#[test]
fn aud024_s2i_matches_the_documented_engine_semantics() {
    let mut scripted = common::scripted(
        r#"
        globals
            integer empty = 1
            integer text = 1
            integer junk_suffix = 0
            integer negative = 0
            integer plus = 0
            integer saturated_high = 0
            integer saturated_low = 0
            integer spaces = 0
            integer decimal = 0
            integer exponent = 0
            integer leading_dot = 0
        endglobals
        function setup takes nothing returns nothing
            set empty = S2I("")
            set text = S2I("abc123")
            set junk_suffix = S2I("123abc")
            set negative = S2I("-123")
            set plus = S2I("+7")
            set saturated_high = S2I("99999999999999999999")
            set saturated_low = S2I("-99999999999999999999")
            set spaces = S2I(" 42 ")
            set decimal = S2I("12.9")
            set exponent = S2I("1e3")
            set leading_dot = S2I(".5")
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("setup").expect("setup");

    assert_eq!(int_of(&scripted, "empty"), 0, "documented: S2I(\"\") == 0");
    assert_eq!(int_of(&scripted, "text"), 0, "documented: S2I(\"abc123\") == 0");
    assert_eq!(
        int_of(&scripted, "junk_suffix"),
        123,
        "documented: S2I(\"123abc\") == 123 (prefix parse)"
    );
    assert_eq!(int_of(&scripted, "negative"), -123, "documented: S2I(\"-123\") == -123");
    assert_eq!(int_of(&scripted, "plus"), 7, "inferred: optional sign, same scan");
    assert_eq!(
        int_of(&scripted, "saturated_high"),
        i32::MAX,
        "documented: too big saturates at max integer"
    );
    assert_eq!(
        int_of(&scripted, "saturated_low"),
        i32::MIN,
        "documented: too small saturates at min integer"
    );
    assert_eq!(int_of(&scripted, "spaces"), 42, "inferred: the scan skips leading whitespace");
    assert_eq!(int_of(&scripted, "decimal"), 12, "the integer part of the numeric prefix");
    assert_eq!(
        int_of(&scripted, "exponent"),
        1,
        "documented scan set [0-9.] stops at 'e': S2I(\"1e3\") == 1"
    );
    assert_eq!(int_of(&scripted, "leading_dot"), 0, "0.5 truncated to integer is 0");
}

#[test]
fn aud024_s2r_matches_the_documented_engine_semantics() {
    let mut scripted = common::scripted(
        r#"
        globals
            real empty = 1.0
            real text = 1.0
            real junk_suffix = 0.0
            real leading_dot = 0.0
            real exponent = 0.0
            real comma = 0.0
            real two_dots = 0.0
            real negative = 0.0
            real overflow = 1.0
            real plain = 0.0
        endglobals
        function setup takes nothing returns nothing
            set empty = S2R("")
            set text = S2R("abc")
            set junk_suffix = S2R("1.5abc")
            set leading_dot = S2R(".123asd")
            set exponent = S2R("1e3")
            set comma = S2R("1,5")
            set two_dots = S2R("1.5.5")
            set negative = S2R("-2.25")
            set overflow = S2R("999999999999999999999999999999999999999999")
            set plain = S2R("2.5")
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("setup").expect("setup");

    assert_eq!(real_of(&scripted, "empty"), 0.0, "documented: empty string is 0");
    assert_eq!(real_of(&scripted, "text"), 0.0, "documented: no leading number is 0");
    assert!(
        (real_of(&scripted, "junk_suffix") - 1.5).abs() < 1e-6,
        "junk suffix keeps the prefix, got {}",
        real_of(&scripted, "junk_suffix")
    );
    assert!(
        (real_of(&scripted, "leading_dot") - 0.123).abs() < 1e-6,
        "documented: S2R(\".123asd\") == 0.123, got {}",
        real_of(&scripted, "leading_dot")
    );
    assert_eq!(
        real_of(&scripted, "exponent"),
        1.0,
        "documented scan set [0-9.] stops at 'e'"
    );
    assert_eq!(
        real_of(&scripted, "comma"),
        1.0,
        "documented: ',' is not a decimal point, the scan stops at it"
    );
    assert_eq!(
        real_of(&scripted, "two_dots"),
        1.5,
        "inferred: a second decimal point ends the number like strtod"
    );
    assert!((real_of(&scripted, "negative") + 2.25).abs() < 1e-6);
    assert_eq!(
        real_of(&scripted, "overflow"),
        0.0,
        "documented: a value too big for the real range returns 0"
    );
    assert!((real_of(&scripted, "plain") - 2.5).abs() < 1e-6, "no regression on plain input");
}

/// The value the old strict parser produced for the same strings is *not* the
/// answer: `"123abc"` was 0, the engine says 123. This pins the divergence that
/// made the change necessary.
#[test]
fn aud024_junk_suffix_is_the_documented_divergence() {
    let strict: i32 = "123abc".parse().unwrap_or(0);
    assert_eq!(strict, 0, "Rust's strict parse rejects the suffix");
    let mut scripted = common::scripted(
        r#"
        globals
            integer value = 0
        endglobals
        function setup takes nothing returns nothing
            set value = S2I("123abc")
        endfunction
        "#,
    );
    scripted.vm_mut().run_function("setup").expect("setup");
    assert_eq!(
        int_of(&scripted, "value"),
        123,
        "the runtime now matches the engine, not the strict parser"
    );
}

#[test]
fn aud024_registry_marks_the_conversions_implemented() {
    assert_eq!(
        wc3_sim::jass::registry::lookup("S2I").map(|def| def.status),
        Some(wc3_sim::jass::NativeStatus::Implemented)
    );
    assert_eq!(
        wc3_sim::jass::registry::lookup("S2R").map(|def| def.status),
        Some(wc3_sim::jass::NativeStatus::Implemented)
    );
}
