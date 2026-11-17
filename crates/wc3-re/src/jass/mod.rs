//! JASS2 front end and interpreter.
//!
//! The lexer/parser implement the language as shipped in `Scripts\common.j`,
//! `Scripts\Blizzard.j` and every `war3map.j` in the local maps:
//!
//! - declarations: `type X extends Y`, `native`, `globals`/`endglobals`,
//!   `constant` globals, `array` slots, `function`/`endfunction`
//! - statements: `local`, `set` (scalar and array), `call`, `if`/`elseif`/
//!   `else`/`endif`, `loop`/`endloop`, `exitwhen`, `return`, `debug` prefix
//! - expressions: integer (decimal, `0x`, `$` hex), real, string, boolean,
//!   `null`, four-character rawcode (`'hfoo'` = `0x68666F6F`), array indexing,
//!   function values (`function Foo`), calls, `and`/`or`/`not`, comparisons,
//!   arithmetic, and `+` as string concatenation
//!
//! The interpreter is deterministic (fixed-seed RNG, ordered containers) and
//! exposes its native registry plus an unimplemented-native census, so the
//! map scripts can be executed without pretending a stub is a game.

pub mod ast;
pub mod lexer;
pub mod natives;
pub mod parser;
pub mod vm;

pub use ast::{Expr, FunctionDecl, GlobalDecl, LocalDecl, NativeDecl, Program, Stmt, TypeDecl};
pub use lexer::{LexError, Token, TokenKind, lex};
pub use parser::{ParseError, parse};
pub use vm::{Runtime, Slot, Value, Wc3Rng};
