//! Dump every registry native as `name|params|ret` in the `registry_gen.rs`
//! encoding (V/B/I/R/S/C/Hname;), sorted by name.
//!
//! This is the reference side of the byte-level signature check against the
//! shipped `Scripts/common.j`: `scripts/native_sig_compare.py` parses the
//! original declarations and diffs them against this dump.
//!
//! Usage: cargo run -q -p wc3-sim --example native_sig_dump

use wc3_sim::jass::registry;
use wc3_sim::jass::value::Type;

fn encode(types: &[Type]) -> String {
    let mut out = String::new();
    for ty in types {
        match ty {
            Type::Void => out.push('V'),
            Type::Bool => out.push('B'),
            Type::Int => out.push('I'),
            Type::Real => out.push('R'),
            Type::Str => out.push('S'),
            Type::Code => out.push('C'),
            Type::Handle(name) => {
                out.push('H');
                out.push_str(name);
                out.push(';');
            }
        }
    }
    out
}

fn main() {
    let mut rows: Vec<(String, String, String)> = registry::table()
        .iter()
        .map(|def| {
            (
                def.name.to_string(),
                encode(&def.signature.params),
                encode(std::slice::from_ref(&def.signature.ret)),
            )
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, params, ret) in rows {
        println!("{name}|{params}|{ret}");
    }
}
