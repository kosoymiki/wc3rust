//! Audit lane H probe: `SubString`/`StringLength` byte-vs-character indexing.
//!
//! `substring` slices a Rust `String` with JASS integer indices, but the
//! indices are byte offsets here and `StringLength` returns UTF-8 bytes.

use wc3_re::jass::{parse, Runtime};

const SOURCE: &str = r#"
native SubString takes string source, integer start, integer end returns string
native StringLength takes string s returns integer

globals
    integer g_len = -1
    string g_one = ""
endglobals

function main takes nothing returns nothing
    set g_len = StringLength("Привет")
    set g_one = SubString("Привет", 0, 1)
endfunction
"#;

const LEN_ONLY: &str = r#"
native StringLength takes string s returns integer

globals
    integer g_len = -1
endglobals

function main takes nothing returns nothing
    set g_len = StringLength("Привет")
endfunction
"#;

fn main() {
    {
        let mut runtime = Runtime::new(parse(LEN_ONLY).expect("parse"), 1);
        let run = runtime.run_main();
        println!(
            "length only: run={run:?} StringLength(\"Привет\")={:?} (6 characters, 12 UTF-8 bytes)",
            runtime.globals.get("g_len").map(|slot| slot.scalar())
        );
    }
    let program = parse(SOURCE).expect("parse");
    let result = std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let mut runtime = Runtime::new(program, 1);
                let run = runtime.run_main();
                println!(
                    "run={run:?} len={:?} one={:?}",
                    runtime.globals.get("g_len").map(|slot| slot.scalar()),
                    runtime.globals.get("g_one").map(|slot| slot.scalar())
                );
            })
            .join()
    });
    println!("thread result: {:?}", result.is_ok());
}
