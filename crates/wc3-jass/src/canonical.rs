//! Canonical JASS2 front end and interpreter (adapter to `wc3-re`).
//!
//! `wc3-re::jass` is the implementation used by the install scanner: it parses
//! `Scripts\common.j`, `Scripts\Blizzard.j`, all 44 campaign map scripts and 91
//! AI scripts, and it carries the complete native registry plus the
//! unimplemented-native census. This module exposes that engine under the
//! `wc3-jass` crate so game-side callers can use the canonical VM without
//! duplicating it, while the legacy [`crate::JassRuntime`] remains available
//! for the small `wc3-sim` host model.
//!
//! The types are re-exported with `Canonical*` aliases to keep them distinct
//! from the legacy types in the crate root.

pub use wc3_re::jass::ast::{
    BinOp as CanonicalBinOp, Expr as CanonicalExpr, FunctionDecl as CanonicalFunction,
    GlobalDecl as CanonicalGlobal, LocalDecl as CanonicalLocal, NativeDecl as CanonicalNative,
    Program as CanonicalProgram, Stmt as CanonicalStmt, TypeDecl as CanonicalType, UnOp as CanonicalUnOp,
};
pub use wc3_re::jass::vm::{Runtime as CanonicalRuntime, Value as CanonicalValue};

/// Parse source with the canonical engine.
pub fn parse(source: &str) -> Result<CanonicalProgram, String> {
    wc3_re::jass::parse(source).map_err(|error| error.to_string())
}

/// Parse with a source name attached (used in diagnostics).
pub fn parse_named(source: &str, name: &str) -> Result<CanonicalProgram, String> {
    wc3_re::jass::parser::parse_named(source, name).map_err(|error| error.to_string())
}

/// Merge programs in order (common.j → Blizzard.j → map/AI script).
pub fn merge(programs: impl IntoIterator<Item = CanonicalProgram>) -> CanonicalProgram {
    let mut out = CanonicalProgram::default();
    for program in programs {
        out.merge(program);
    }
    out
}

/// One execution result from the canonical runtime.
#[derive(Debug, Clone, Default)]
pub struct RunReport {
    /// Entry-point return value rendered as text (`""` for `nothing`).
    pub value: String,
    /// Interpreter steps used (bounded by the caller).
    pub steps: u64,
    /// `DisplayText*`/debug output captured by the runtime.
    pub output: Vec<String>,
    /// Natives that were called but are outside the headless port, with counts.
    pub unimplemented: Vec<(String, u64)>,
    /// Non-fatal runtime errors (global initializers, etc.).
    pub errors: Vec<String>,
}

/// Execute `entry` (case-insensitive) in a fresh runtime.
pub fn run(source: &str, entry: &str, seed: u64) -> Result<RunReport, String> {
    run_program(parse(source)?, entry, seed)
}

/// Execute `entry` in a program assembled from several sources.
pub fn run_sources(
    sources: &[(&str, &str)],
    entry: &str,
    seed: u64,
) -> Result<RunReport, String> {
    let mut program = CanonicalProgram::default();
    for (name, source) in sources {
        program.merge(parse_named(source, name)?);
    }
    run_program(program, entry, seed)
}

/// Execute `entry` in an already parsed program.
pub fn run_program(program: CanonicalProgram, entry: &str, seed: u64) -> Result<RunReport, String> {
    let mut runtime = CanonicalRuntime::new(program, seed);
    let value = runtime
        .run_function(entry)
        .map_err(|error| error.to_string())?;
    Ok(RunReport {
        value: value.as_str(),
        steps: runtime.steps,
        output: runtime.output,
        unimplemented: runtime
            .unimplemented
            .iter()
            .map(|(name, count)| (name.clone(), *count))
            .collect(),
        errors: runtime.errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = r#"
        globals
            integer counter = 0
        endglobals

        function Add takes integer a, integer b returns integer
            return a + b
        endfunction

        function main takes nothing returns nothing
            local integer i = 0
            loop
                exitwhen i >= 3
                set i = i + 1
                set counter = counter + Add(i, 10)
            endloop
        endfunction
    "#;

    #[test]
    fn canonical_engine_runs_the_shared_script() {
        let report = run(SCRIPT, "main", 1).expect("run");
        assert_eq!(report.steps > 0, true);
        // counter = (1+10) + (2+10) + (3+10) = 36; the value is observable
        // through a second function because `main` returns nothing.
        let program = parse(SCRIPT).unwrap();
        let mut runtime = CanonicalRuntime::new(program, 1);
        runtime.run_function("main").unwrap();
        assert_eq!(
            runtime.globals.get("counter").map(|slot| slot.scalar().as_int()),
            Some(36)
        );
    }

    #[test]
    fn legacy_and_canonical_engines_agree_on_arithmetic() {
        // The legacy engine is kept for `wc3-sim`; both must agree on the
        // language core (progress guard against silent divergence).
        let legacy_program = crate::parse(SCRIPT).expect("legacy parse");
        let mut legacy = crate::JassRuntime::new(legacy_program, 1);
        legacy.run_function("main").expect("legacy run");
        assert_eq!(
            legacy.globals.get("counter").map(crate::Value::as_int),
            Some(36)
        );
        let program = parse(SCRIPT).unwrap();
        let mut canonical = CanonicalRuntime::new(program, 1);
        canonical.run_function("main").unwrap();
        assert_eq!(
            canonical
                .globals
                .get("counter")
                .map(|slot| slot.scalar().as_int()),
            Some(36)
        );
    }
}
