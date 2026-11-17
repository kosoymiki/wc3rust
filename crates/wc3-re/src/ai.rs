//! AI-script access: `Scripts\common.ai` + the 91 race/melee campaign AI files.
//!
//! AI scripts reuse the JASS grammar (`globals`, `constant`, `native`,
//! `function takes/returns`, `if/loop/exitwhen/call/set`) with engine-provided
//! constants such as `PALADIN='Hpal'` and `ARCHMAGE='Hamg'` declared in
//! `common.ai`. `common.ai` also defines `InitAI`; every concrete race AI
//! defines `main` as an endless strategy loop (`Sleep(3)` between passes), so
//! the headless port parses every script and executes `InitAI` (plus any
//! explicitly requested entry point) rather than starting an endless game loop.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{ReError, Result};

use super::jass::{Program, Runtime, parse};

#[derive(Debug, Clone)]
pub struct AiScript {
    pub path: PathBuf,
    pub program: Program,
    pub functions: BTreeSet<String>,
    pub natives: BTreeSet<String>,
    pub globals: usize,
}

impl AiScript {
    pub fn parse_file(path: &Path) -> Result<Self> {
        let source = fs::read_to_string(path)?;
        Self::parse_source(path.to_path_buf(), &source)
    }

    pub fn parse_source(path: PathBuf, source: &str) -> Result<Self> {
        let program = parse(source).map_err(|err| ReError::Parse(format!("{}: {err}", path.display())))?;
        Ok(Self {
            path,
            functions: program.function_names(),
            natives: program
                .natives
                .iter()
                .map(|native| native.name.clone())
                .collect(),
            globals: program.globals.len(),
            program,
        })
    }

    /// Merge with `common.ai` and run `InitAI` under a bounded interpreter.
    pub fn run_init(&self, common_ai: &Program, seed: u64) -> Result<AiRun> {
        let mut program = common_ai.clone();
        program.merge(self.program.clone());
        let mut runtime = Runtime::new(program, seed);
        runtime.max_steps = 2_000_000;
        let result = if runtime.function_names().contains("InitAI") {
            runtime.run_function("initai")
        } else {
            runtime.run_function("main")
        };
        Ok(AiRun {
            ok: result.is_ok(),
            error: result.err().map(|err| err.to_string()),
            steps: runtime.steps,
            output: runtime.output,
            unimplemented: runtime
                .unimplemented
                .iter()
                .map(|(name, count)| (name.clone(), *count))
                .collect(),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct AiRun {
    pub ok: bool,
    pub error: Option<String>,
    pub steps: u64,
    pub output: Vec<String>,
    pub unimplemented: Vec<(String, u64)>,
}

#[derive(Debug, Clone, Default)]
pub struct AiCensus {
    pub scripts: usize,
    pub parsed: usize,
    pub failed: Vec<(String, String)>,
    pub functions: usize,
    pub natives: usize,
    pub global_decls: usize,
    pub common_ai_functions: usize,
    pub init_runs: usize,
    pub init_failures: Vec<(String, String)>,
}

/// Parse every `.ai` file in a directory and execute `InitAI` with `common.ai`.
pub fn scan_ai_dir(dir: &Path, seed: u64) -> Result<AiCensus> {
    let common_path = dir.join("common.ai");
    let common_source = fs::read_to_string(&common_path)?;
    let common_program =
        parse(&common_source).map_err(|err| ReError::Parse(format!("common.ai: {err}")))?;
    let mut census = AiCensus {
        common_ai_functions: common_program.functions.len(),
        ..AiCensus::default()
    };
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "ai"))
        .collect();
    entries.sort();
    for path in entries {
        census.scripts += 1;
        let script = match AiScript::parse_file(&path) {
            Ok(script) => script,
            Err(err) => {
                census.failed.push((path.display().to_string(), err.to_string()));
                continue;
            }
        };
        census.parsed += 1;
        census.functions += script.functions.len();
        census.natives += script.natives.len();
        census.global_decls += script.globals;
        match script.run_init(&common_program, seed) {
            Ok(run) if run.ok => census.init_runs += 1,
            Ok(run) => census
                .init_failures
                .push((path.display().to_string(), run.error.unwrap_or_default())),
            Err(err) => census
                .init_failures
                .push((path.display().to_string(), err.to_string())),
        }
    }
    Ok(census)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_local_ai_scripts() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3/Scripts");
        if !dir.is_dir() {
            eprintln!("skip: Scripts missing");
            return;
        }
        let census = scan_ai_dir(&dir, 11).expect("scan AI");
        assert!(census.scripts >= 90, "scripts {}", census.scripts);
        assert_eq!(census.failed.len(), 0, "{:?}", &census.failed[..census.failed.len().min(3)]);
        assert!(census.common_ai_functions > 50);
    }
}
