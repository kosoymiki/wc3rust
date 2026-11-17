//! Run a device-less touch trace and print the router's decisions.
//!
//! ```text
//! cargo run -p wc3-input --example touch-trace -- analysis/agent-touch/traces/box-select.trace
//! cargo run -p wc3-input --example touch-trace -- --check analysis/agent-touch/traces/box-additive.trace
//! ```
//!
//! `--check` is the CI form: no event dump, one PASS/FAIL line, non-zero exit on
//! any divergence between the recorded gestures and the router.
//!
//! Exit codes: 0 pass, 1 expectation mismatch, 2 usage/parse error.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut check = false;
    let mut path = None;
    for arg in std::env::args().skip(1) {
        if arg == "--check" {
            check = true;
        } else if path.is_none() {
            path = Some(arg);
        } else {
            eprintln!("usage: touch-trace [--check] <trace-file>");
            return ExitCode::from(2);
        }
    }
    let Some(path) = path else {
        eprintln!("usage: touch-trace [--check] <trace-file>");
        return ExitCode::from(2);
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("{path}: {error}");
            return ExitCode::from(2);
        }
    };
    let trace = match wc3_input::trace::parse(&text) {
        Ok(trace) => trace,
        Err(error) => {
            eprintln!("{path}: {error}");
            return ExitCode::from(2);
        }
    };
    let report = trace.run();
    if !check {
        for (index, event) in report.events.iter().enumerate() {
            println!("{:02} {event}", index + 1);
        }
    }
    if !report.ok() {
        for failure in &report.failures {
            eprintln!("FAIL {path}: {failure}");
        }
        return ExitCode::FAILURE;
    }
    println!("PASS {path} events={}", report.events.len());
    ExitCode::SUCCESS
}
