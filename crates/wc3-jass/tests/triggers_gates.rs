//! QA trigger gates for the `wc3-jass` lexer/parser/interpreter.
//!
//! These reproduce the three QA gates that were failing on the parser
//! (`triggers.jass`, `triggers.return`, `triggers.elseif`) without depending on
//! the gameplay crates, so the parser regression gate stays runnable while
//! other layers are mid-change:
//!
//! - `triggers.jass`: synthetic timer trigger runs twice + all 44 retail map
//!   scripts parse (1 synthetic + 44 maps = 45).
//! - `triggers.return`: bare `return` (no value) must terminate at the newline.
//! - `triggers.elseif`: a chain keeps the first true branch (n = 2, not 3).

use std::fs;
use std::path::{Path, PathBuf};

use wc3_jass::{JassRuntime, Value, parse};
use wc3_re::mapfiles::MapBundle;

const SYNTHETIC_TRIGGER: &str = r#"
globals
    trigger t
    integer n = 0
endglobals
function action takes nothing returns nothing
    set n = n + 1
endfunction
function main takes nothing returns nothing
    set t = CreateTrigger()
    call TriggerAddAction(t, function action)
    call TriggerRegisterTimerEvent(t, 0.5, true)
endfunction
"#;

fn map_files(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("w3m" | "w3x" | "w3n")
            ) {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

#[test]
fn qa_trigger_gates() {
    // --- triggers.return -------------------------------------------------
    let return_scripts = [
        (
            "bare return at end",
            "function f takes nothing returns nothing\n    return\nendfunction\n",
        ),
        (
            "bare return before statement",
            "function f takes nothing returns nothing\n    return\n    set x = 1\nendfunction\n",
        ),
        (
            "bare return in if",
            "function f takes nothing returns nothing\n    if a then\n        return\n    endif\nendfunction\n",
        ),
    ];
    let mut pass = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for (label, script) in return_scripts {
        match parse(script) {
            Ok(_) => pass += 1,
            Err(error) => failures.push(format!("{label}: parse -> {error}")),
        }
    }
    println!(
        "GATE triggers.return      {}    pass={pass} fail={} total=3",
        if failures.is_empty() { "PASS" } else { "FAIL" },
        failures.len()
    );
    assert!(failures.is_empty(), "triggers.return: {failures:?}");

    // bare return must not run the statement that follows it
    let script = r#"
globals
    integer n = 0
endglobals
function f takes nothing returns nothing
    return
    set n = 1
endfunction
function main takes nothing returns nothing
    call f()
endfunction
"#;
    let program = parse(script).expect("bare return before set parses");
    let mut runtime = JassRuntime::new(program, 1);
    runtime.run_function("main").expect("run");
    let n = runtime.globals.get("n").cloned().unwrap_or(Value::Int(0));
    assert_eq!(n, Value::Int(0), "statement after bare return must not run");

    // --- triggers.elseif -------------------------------------------------
    let script = r#"
globals
    integer n = 0
endglobals
function main takes nothing returns nothing
    if false then
        set n = 1
    elseif true then
        set n = 2
    elseif true then
        set n = 3
    endif
endfunction
"#;
    let program = parse(script).expect("elseif chain parses");
    let mut runtime = JassRuntime::new(program, 1);
    runtime.run_function("main").expect("run");
    let n = runtime.globals.get("n").cloned();
    println!(
        "GATE triggers.elseif      {}    pass={} fail={} total=1",
        if n == Some(Value::Int(2)) { "PASS" } else { "FAIL" },
        usize::from(n == Some(Value::Int(2))),
        usize::from(n != Some(Value::Int(2)))
    );
    assert_eq!(n, Some(Value::Int(2)), "first true elseif branch wins");

    // --- triggers.jass ---------------------------------------------------
    let mut pass = 0usize;
    let mut total = 0usize;
    let mut failures: Vec<String> = Vec::new();

    total += 1;
    match parse(SYNTHETIC_TRIGGER) {
        Ok(program) => {
            let mut runtime = JassRuntime::new(program, 1);
            match runtime.run_function("main") {
                Ok(_) => {
                    runtime.tick(0.5);
                    runtime.tick(0.5);
                    let count = runtime.globals.get("n").cloned();
                    if count == Some(Value::Int(2)) && runtime.triggers.len() == 1 {
                        pass += 1;
                    } else {
                        failures.push(format!(
                            "synthetic trigger fired count={count:?} triggers={}",
                            runtime.triggers.len()
                        ));
                    }
                }
                Err(error) => failures.push(format!("synthetic trigger main -> {error}")),
            }
        }
        Err(error) => failures.push(format!("synthetic trigger parse -> {error}")),
    }

    let maps_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3/Maps");
    let maps = map_files(&maps_root);
    assert!(
        !maps.is_empty(),
        "retail map corpus missing at {}",
        maps_root.display()
    );
    let mut map_functions = 0usize;
    for path in &maps {
        total += 1;
        let name = path
            .strip_prefix(&maps_root)
            .unwrap_or(path)
            .display()
            .to_string();
        match MapBundle::open(path) {
            Ok(bundle) => match bundle.script.as_deref() {
                Some(script) => match parse(script) {
                    Ok(program) => {
                        pass += 1;
                        map_functions += program.functions.len();
                    }
                    Err(error) => failures.push(format!("{name}: parse -> {error}")),
                },
                None => failures.push(format!("{name}: no war3map.j")),
            },
            Err(error) => failures.push(format!("{name}: MapBundle::open -> {error}")),
        }
    }
    println!(
        "GATE triggers.jass        {}    pass={pass} fail={} total={total} maps={} map_functions={map_functions}",
        if failures.is_empty() { "PASS" } else { "FAIL" },
        failures.len(),
        maps.len()
    );
    assert!(
        failures.is_empty(),
        "triggers.jass failures: {}",
        failures.join(" | ")
    );
    assert_eq!(total, 45, "1 synthetic + 44 retail maps");
}
