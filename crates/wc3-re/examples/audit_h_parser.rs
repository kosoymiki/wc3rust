//! Audit lane H probe: malformed JASS that the parser accepts silently, plus
//! string-literal decoding of non-ASCII map text.

use wc3_re::jass::{lex, parse};

fn main() {
    // 1. Unterminated function: no `endfunction`, body has one statement.
    let truncated = "function f takes nothing returns nothing\n    set x = 1\n";
    match parse(truncated) {
        Ok(program) => println!(
            "unterminated function ACCEPTED: functions={} body={:?}",
            program.functions.len(),
            program
                .functions
                .first()
                .map(|function| function.body.len())
        ),
        Err(error) => println!("unterminated function rejected: {error}"),
    }

    // 2. Unterminated function with an empty body is rejected (control case).
    let empty = "function f takes nothing returns nothing\n";
    match parse(empty) {
        Ok(program) => println!("empty-body truncated ACCEPTED: {program:?}"),
        Err(error) => println!("empty-body truncated rejected: {error}"),
    }

    // 3. UTF-8 string literal from a localized map script.
    let utf8 = "function f takes nothing returns nothing\n    call BJDebugMsg(\"Привет\")\nendfunction\n";
    let program = parse(utf8).expect("parse");
    if let Some(call) = program.functions.first().and_then(|f| f.body.first()) {
        println!("utf8 literal AST: {call:?}");
    }
    let tokens = lex("function f takes nothing returns nothing\n    call BJDebugMsg(\"Привет\")\nendfunction\n")
        .expect("lex");
    for token in tokens {
        if let wc3_re::jass::TokenKind::Str(value) = token.kind {
            println!(
                "lexer string: chars={:?} bytes={} utf8_roundtrip={}",
                value.chars().collect::<Vec<_>>(),
                value.len(),
                value == "Привет"
            );
        }
    }
}
