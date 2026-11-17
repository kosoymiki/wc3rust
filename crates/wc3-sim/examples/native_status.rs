//! Writes the native coverage report (audit AUD-016 / AUD-027).
//!
//! `cargo run -p wc3-sim --example native_status > analysis/audit/native_status.json`
//! produces the machine-readable table: one row per registry native with its
//! semantic status and the reason it is not `Implemented`.
//!
//! The report is generated from the runtime tables, never hand-written, so it
//! cannot drift from the code the way the old `handled: bool` did.

fn main() {
    let counts = wc3_sim::jass::native_status::counts();
    eprintln!(
        "natives: {} total, {} implemented, {} partial, {} recorded-only, {} no-op, {} unsupported",
        counts.total,
        counts.implemented,
        counts.partial,
        counts.recorded_only,
        counts.no_op,
        counts.unsupported
    );
    print!("{}", wc3_sim::jass::native_status::report_json());
}
