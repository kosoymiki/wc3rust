//! `ui.trace` — gesture trace replay fixtures for the input layer: every
//! fixture under `analysis/agent-touch/traces/` must parse and replay with zero
//! divergence, which is exactly what `touch-trace --check` enforces in CI.

use std::fs;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../analysis/agent-touch/traces")
        .join(name)
}

const FIXTURES: [&str; 15] = [
    "box-select.trace",
    "group-row.trace",
    "pinch-pan.trace",
    "type-select.trace",
    "box-additive.trace",
    "orders-targeting.trace",
    "minimap-orders.trace",
    "placement.trace",
    "patrol-two-taps.trace",
    "touch-orders.trace",
    "multitouch-cancel.trace",
    "multitouch-third-finger.trace",
    "rotation-focus.trace",
    "two-finger-tap.trace",
    "key-repeat.trace",
];

#[test]
fn ui_trace_fixtures_replay_without_divergence() {
    for name in FIXTURES {
        let path = fixture(name);
        let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let trace = wc3_input::trace::parse(&text).unwrap_or_else(|error| panic!("{name}: {error}"));
        let report = trace.run();
        assert!(
            report.ok(),
            "{name} diverged: {:?}",
            report.failures
        );
        assert!(!report.events.is_empty(), "{name} produced no events");
    }
}

#[test]
fn ui_trace_check_mode_reports_any_divergence() {
    // A trace whose expectation does not match the router must fail, which is
    // the failure `touch-trace --check` turns into a non-zero exit code.
    let text = "layout\ndown 640 360 0.0\nexpect start=minimap\n";
    let report = wc3_input::trace::parse(text).expect("trace parses").run();
    assert!(!report.ok(), "a divergent expectation must not pass");
    assert!(
        report.failures[0].contains("expected `start=minimap`, got `start=world`"),
        "{:?}",
        report.failures
    );
}
