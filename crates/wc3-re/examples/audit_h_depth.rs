//! Audit lane H probe: recursive-descent parser depth on map-controlled input.
//!
//! `parse_expr` -> `parse_primary` -> `parse_expr` recurses once per nested
//! parenthesis. A map script is untrusted input; the parser has no depth guard,
//! so the only limit is the thread stack.

use wc3_re::jass::parse;

fn main() {
    let depth: usize = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(100_000);
    let mut source = String::from("function f takes nothing returns nothing\n    local integer x = ");
    for _ in 0..depth {
        source.push('(');
    }
    source.push('1');
    for _ in 0..depth {
        source.push(')');
    }
    source.push_str("\nendfunction\n");
    println!("depth={depth} bytes={}", source.len());
    let program = parse(&source).expect("parse");
    println!("parsed: functions={}", program.functions.len());
}
