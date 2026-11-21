//! JASS2 lexer, parser and interpreter with a native registry and triggers.
//!
//! The VM is deterministic and renderer-free. Handles are plain integer tokens;
//! unit/trigger/timer natives are backed by a small host table so the runtime can
//! be tested without a full game, and can be extended with a game host later.
//!
//! [`canonical`] exposes the complete `wc3-re::jass` engine (used by the install
//! scanner and the 44-map execution sweep) under this crate, so game code has a
//! single authoritative JASS implementation to target.
//!
//! # Unsupported calls are recorded, not swallowed
//!
//! A script may declare its own `native`; the host only implements the subset it
//! models. The runtime never drops such a call silently: every call to a declared
//! native without a host implementation is counted in
//! [`JassRuntime::unimplemented`], returns the value its declared return type
//! produces (not a type-less `Null`), and fails outright under
//! [`JassRuntime::set_strict_natives`]. The same holds for trigger-action
//! failures, which land in [`JassRuntime::errors`] (both were previously
//! discarded with `let _ =`, which let a map "boot" while its trigger did
//! nothing).

pub mod canonical;

use std::collections::{BTreeMap, HashMap};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    Int(i64),
    Real(f64),
    Str(String),
    Newline,
    Punct(&'static str),
    End,
}

pub fn lex(source: &str) -> Result<Vec<Token>, String> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b' ' | b'\t' | b'\r' => i += 1,
            b'\n' => {
                tokens.push(Token::Newline);
                i += 1;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += 1;
                }
                if i >= bytes.len() {
                    return Err("unterminated string".into());
                }
                tokens.push(Token::Str(source[start..i].to_string()));
                i += 1;
            }
            b'\'' => {
                // JASS rawcode literal: 'hfoo', 'LOcg', ...
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b'\'' {
                    i += 1;
                }
                if i >= bytes.len() {
                    return Err("unterminated rawcode".into());
                }
                tokens.push(Token::Str(source[start..i].to_string()));
                i += 1;
            }
            b'0'..=b'9' => {
                let start = i;
                let mut real = false;
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                    if bytes[i] == b'.' {
                        real = true;
                    }
                    i += 1;
                }
                let text = &source[start..i];
                if real {
                    tokens.push(Token::Real(text.parse().map_err(|_| "bad real")?));
                } else {
                    tokens.push(Token::Int(text.parse().map_err(|_| "bad int")?));
                }
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                tokens.push(Token::Ident(source[start..i].to_string()));
            }
            _ => {
                let two = if i + 1 < bytes.len() {
                    &source[i..i + 2]
                } else {
                    ""
                };
                let punct = match two {
                    "==" => Some("=="),
                    "!=" => Some("!="),
                    "<=" => Some("<="),
                    ">=" => Some(">="),
                    _ => None,
                };
                if let Some(p) = punct {
                    tokens.push(Token::Punct(p));
                    i += 2;
                } else {
                    let p = match c {
                        b'(' => "(",
                        b')' => ")",
                        b'[' => "[",
                        b']' => "]",
                        b',' => ",",
                        b'=' => "=",
                        b'+' => "+",
                        b'-' => "-",
                        b'*' => "*",
                        b'/' => "/",
                        b'%' => "%",
                        b'<' => "<",
                        b'>' => ">",
                        b'!' => "!",
                        _ => {
                            let start = i.saturating_sub(24);
                            let end = (i + 24).min(source.len());
                            return Err(format!(
                                "unexpected character {:?} at byte {i} near {:?}",
                                c as char,
                                &source[start..end]
                            ));
                        }
                    };
                    tokens.push(Token::Punct(p));
                    i += 1;
                }
            }
        }
    }
    tokens.push(Token::End);
    Ok(tokens)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Int(i64),
    Real(f64),
    Str(String),
    Bool(bool),
    Null,
    Var(String),
    Array(String, Box<Expr>),
    Call(String, Vec<Expr>),
    FuncRef(String),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Un(char, Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Local(String, Option<Expr>),
    Set(String, Option<Expr>, Expr),
    Call(Expr),
    If(Expr, Vec<Stmt>, Vec<Stmt>),
    Loop(Vec<Stmt>),
    ExitWhen(Expr),
    Return(Option<Expr>),
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<(String, String)>,
    pub returns: String,
    pub body: Vec<Stmt>,
    pub native: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub globals: Vec<Stmt>,
    pub functions: Vec<Function>,
}

pub fn parse(source: &str) -> Result<Program, String> {
    let tokens = lex(source)?;
    let mut p = Parser { tokens, pos: 0 };
    p.program()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn next(&mut self) -> Token {
        let token = self.peek().clone();
        self.pos += 1;
        token
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Token::Newline) {
            self.pos += 1;
        }
    }

    fn eat_ident(&mut self, expected: &str) -> Result<(), String> {
        self.skip_newlines();
        match self.next() {
            Token::Ident(id) if id.eq_ignore_ascii_case(expected) => Ok(()),
            other => Err(format!("expected {expected}, got {other:?}")),
        }
    }

    fn take_ident(&mut self) -> Result<String, String> {
        self.skip_newlines();
        match self.next() {
            Token::Ident(id) => Ok(id),
            other => Err(format!("expected identifier, got {other:?}")),
        }
    }

    fn program(&mut self) -> Result<Program, String> {
        let mut program = Program::default();
        loop {
            self.skip_newlines();
            match self.peek() {
                Token::End => break,
                Token::Ident(id) if id.eq_ignore_ascii_case("globals") => {
                    self.next();
                    program.globals = self.globals_block()?;
                    self.eat_ident("endglobals")?;
                }
                Token::Ident(id) if id.eq_ignore_ascii_case("native") => {
                    self.next();
                    let name = self.take_ident()?;
                    let params = self.params()?;
                    self.eat_ident("returns")?;
                    let returns = self.take_ident()?;
                    program.functions.push(Function {
                        name,
                        params,
                        returns,
                        body: Vec::new(),
                        native: true,
                    });
                }
                Token::Ident(id) if id.eq_ignore_ascii_case("constant") => {
                    self.next();
                }
                Token::Ident(id) if id.eq_ignore_ascii_case("type") => {
                    self.next();
                    let _name = self.take_ident()?;
                    self.eat_ident("extends")?;
                    let _base = self.take_ident()?;
                }
                Token::Ident(id) if id.eq_ignore_ascii_case("function") => {
                    self.next();
                    let name = self.take_ident()?;
                    let params = self.params()?;
                    self.eat_ident("returns")?;
                    let returns = self.take_ident()?;
                    let body = self.block(&["endfunction"])?;
                    self.eat_ident("endfunction")?;
                    program.functions.push(Function {
                        name,
                        params,
                        returns,
                        body,
                        native: false,
                    });
                }
                other => return Err(format!("unexpected top-level token {other:?}")),
            }
        }
        Ok(program)
    }

    fn globals_block(&mut self) -> Result<Vec<Stmt>, String> {
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            if let Token::Ident(id) = self.peek()
                && id.eq_ignore_ascii_case("endglobals")
            {
                break;
            }
            if matches!(self.peek(), Token::End) {
                break;
            }
            if let Token::Ident(id) = self.peek()
                && id.eq_ignore_ascii_case("constant")
            {
                self.next();
            }
            let _ty = self.take_ident()?;
            let first = self.take_ident()?;
            let (name, init) = if first.eq_ignore_ascii_case("array") {
                (self.take_ident()?, None)
            } else {
                self.skip_newlines();
                let init = if matches!(self.peek(), Token::Punct("=")) {
                    self.next();
                    Some(self.expr()?)
                } else {
                    None
                };
                (first, init)
            };
            stmts.push(Stmt::Local(name, init));
        }
        Ok(stmts)
    }

    fn params(&mut self) -> Result<Vec<(String, String)>, String> {
        self.eat_ident("takes")?;
        self.skip_newlines();
        if let Token::Ident(id) = self.peek()
            && id.eq_ignore_ascii_case("nothing")
        {
            self.next();
            return Ok(Vec::new());
        }
        let mut params = Vec::new();
        loop {
            let ty = self.take_ident()?;
            let name = self.take_ident()?;
            params.push((ty, name));
            self.skip_newlines();
            if matches!(self.peek(), Token::Punct(",")) {
                self.next();
            } else {
                break;
            }
        }
        Ok(params)
    }

    fn block(&mut self, terminators: &[&str]) -> Result<Vec<Stmt>, String> {
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            if let Token::Ident(id) = self.peek()
                && terminators.iter().any(|t| id.eq_ignore_ascii_case(t))
            {
                break;
            }
            if matches!(self.peek(), Token::End) {
                break;
            }
            stmts.push(self.stmt()?);
        }
        Ok(stmts)
    }

    fn stmt(&mut self) -> Result<Stmt, String> {
        self.skip_newlines();
        let Token::Ident(keyword) = self.next() else {
            return Err("expected statement".into());
        };
        match keyword.to_ascii_lowercase().as_str() {
            "local" => {
                let _ty = self.take_ident()?;
                let mut name = self.take_ident()?;
                self.skip_newlines();
                if name.eq_ignore_ascii_case("array") {
                    name = self.take_ident()?;
                    return Ok(Stmt::Local(name, None));
                }
                self.skip_newlines();
                let init = if matches!(self.peek(), Token::Punct("=")) {
                    self.next();
                    Some(self.expr()?)
                } else {
                    None
                };
                Ok(Stmt::Local(name, init))
            }
            "set" => {
                let name = self.take_ident()?;
                self.skip_newlines();
                let index = if matches!(self.peek(), Token::Punct("[")) {
                    self.next();
                    let e = self.expr()?;
                    self.expect_punct("]")?;
                    Some(e)
                } else {
                    None
                };
                self.expect_punct("=")?;
                let value = self.expr()?;
                Ok(Stmt::Set(name, index, value))
            }
            "call" => Ok(Stmt::Call(self.expr()?)),
            "if" => {
                let cond = self.expr()?;
                self.eat_ident("then")?;
                let then = self.block(&["elseif", "else", "endif"])?;
                // Collect every elseif body and the optional final else, then
                // build the else-chain from the last branch backwards. The
                // earlier implementation overwrote `else_branch` per elseif,
                // so only the last branch survived and the first-true branch
                // semantics were lost.
                let mut branches: Vec<(Expr, Vec<Stmt>)> = Vec::new();
                let mut final_else: Vec<Stmt> = Vec::new();
                loop {
                    self.skip_newlines();
                    let Token::Ident(id) = self.peek() else { break };
                    if id.eq_ignore_ascii_case("elseif") {
                        self.next();
                        let cond = self.expr()?;
                        self.eat_ident("then")?;
                        let body = self.block(&["elseif", "else", "endif"])?;
                        branches.push((cond, body));
                    } else if id.eq_ignore_ascii_case("else") {
                        self.next();
                        final_else = self.block(&["endif"])?;
                        break;
                    } else {
                        break;
                    }
                }
                self.eat_ident("endif")?;
                let mut else_branch = final_else;
                for (cond, body) in branches.into_iter().rev() {
                    else_branch = vec![Stmt::If(cond, body, else_branch)];
                }
                Ok(Stmt::If(cond, then, else_branch))
            }
            "loop" => {
                let body = self.block(&["endloop"])?;
                self.eat_ident("endloop")?;
                Ok(Stmt::Loop(body))
            }
            "exitwhen" => Ok(Stmt::ExitWhen(self.expr()?)),
            "return" => {
                // A bare `return` is terminated by the newline. Do not skip
                // newlines here: doing so consumed the next statement's first
                // token as the return expression (`endfunction`, `set`, …).
                if matches!(self.peek(), Token::Newline | Token::End) {
                    return Ok(Stmt::Return(None));
                }
                if let Token::Ident(id) = self.peek()
                    && id.eq_ignore_ascii_case("nothing")
                {
                    self.next();
                    return Ok(Stmt::Return(None));
                }
                Ok(Stmt::Return(Some(self.expr()?)))
            }
            other => Err(format!("unknown statement {other}")),
        }
    }

    fn expect_punct(&mut self, punct: &str) -> Result<(), String> {
        self.skip_newlines();
        match self.next() {
            Token::Punct(p) if p == punct => Ok(()),
            other => Err(format!("expected {punct}, got {other:?}")),
        }
    }

    fn expr(&mut self) -> Result<Expr, String> {
        self.expr_bp(0)
    }

    fn expr_bp(&mut self, min_bp: u8) -> Result<Expr, String> {
        let mut lhs = self.unary()?;
        loop {
            self.skip_newlines();
            let Token::Ident(id) = self.peek() else { break };
            let lower = id.to_ascii_lowercase();
            let (op, bp) = match lower.as_str() {
                "or" => (BinOp::Or, 1),
                "and" => (BinOp::And, 2),
                _ => break,
            };
            if bp < min_bp {
                break;
            }
            self.next();
            let rhs = self.expr_bp(bp + 1)?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        loop {
            self.skip_newlines();
            let Token::Punct(p) = self.peek() else { break };
            let (op, bp) = match *p {
                "==" => (BinOp::Eq, 3),
                "!=" => (BinOp::Ne, 3),
                "<" => (BinOp::Lt, 3),
                "<=" => (BinOp::Le, 3),
                ">" => (BinOp::Gt, 3),
                ">=" => (BinOp::Ge, 3),
                "+" => (BinOp::Add, 4),
                "-" => (BinOp::Sub, 4),
                "*" => (BinOp::Mul, 5),
                "/" => (BinOp::Div, 5),
                "%" => (BinOp::Mod, 5),
                _ => break,
            };
            if bp < min_bp {
                break;
            }
            self.next();
            let rhs = self.expr_bp(bp + 1)?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        self.skip_newlines();
        if let Token::Punct(p) = self.peek() {
            if *p == "-" || *p == "!" {
                let op = if *p == "-" { '-' } else { '!' };
                self.next();
                let inner = self.unary()?;
                return Ok(Expr::Un(op, Box::new(inner)));
            }
        }
        if let Token::Ident(id) = self.peek()
            && id.eq_ignore_ascii_case("not")
        {
            self.next();
            let inner = self.unary()?;
            return Ok(Expr::Un('!', Box::new(inner)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Expr, String> {
        self.skip_newlines();
        match self.next() {
            Token::Int(v) => Ok(Expr::Int(v)),
            Token::Real(v) => Ok(Expr::Real(v)),
            Token::Str(s) => Ok(Expr::Str(s)),
            Token::Punct("(") => {
                let e = self.expr()?;
                self.expect_punct(")")?;
                Ok(e)
            }
            Token::Ident(id) if id.eq_ignore_ascii_case("true") => Ok(Expr::Bool(true)),
            Token::Ident(id) if id.eq_ignore_ascii_case("false") => Ok(Expr::Bool(false)),
            Token::Ident(id) if id.eq_ignore_ascii_case("null") => Ok(Expr::Null),
            Token::Ident(id) if id.eq_ignore_ascii_case("function") => {
                Ok(Expr::FuncRef(self.take_ident()?))
            }
            Token::Ident(id) => {
                self.skip_newlines();
                if matches!(self.peek(), Token::Punct("(")) {
                    self.next();
                    let mut args = Vec::new();
                    self.skip_newlines();
                    if !matches!(self.peek(), Token::Punct(")")) {
                        loop {
                            args.push(self.expr()?);
                            self.skip_newlines();
                            if matches!(self.peek(), Token::Punct(",")) {
                                self.next();
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect_punct(")")?;
                    Ok(Expr::Call(id, args))
                } else if matches!(self.peek(), Token::Punct("[")) {
                    self.next();
                    let index = self.expr()?;
                    self.expect_punct("]")?;
                    Ok(Expr::Array(id, Box::new(index)))
                } else {
                    Ok(Expr::Var(id))
                }
            }
            other => Err(format!("unexpected expression token {other:?}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Real(f64),
    Bool(bool),
    Str(String),
    Handle(u64),
    Code(String),
    Null,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(v) => write!(f, "{v}"),
            Self::Real(v) => write!(f, "{v}"),
            Self::Bool(v) => write!(f, "{v}"),
            Self::Str(v) => f.write_str(v),
            Self::Handle(v) => write!(f, "handle:{v}"),
            Self::Code(v) => write!(f, "code:{v}"),
            Self::Null => f.write_str("null"),
        }
    }
}

impl Value {
    pub fn as_bool(&self) -> bool {
        match self {
            Self::Bool(v) => *v,
            Self::Int(v) => *v != 0,
            Self::Real(v) => *v != 0.0,
            Self::Str(v) => !v.is_empty(),
            Self::Null => false,
            _ => true,
        }
    }
    pub fn as_int(&self) -> i64 {
        match self {
            Self::Int(v) => *v,
            Self::Real(v) => *v as i64,
            Self::Bool(v) => *v as i64,
            Self::Handle(v) => *v as i64,
            _ => 0,
        }
    }
    pub fn as_real(&self) -> f64 {
        match self {
            Self::Real(v) => *v,
            Self::Int(v) => *v as f64,
            _ => 0.0,
        }
    }
    pub fn as_str(&self) -> String {
        self.to_string()
    }
}

#[derive(Debug, Clone)]
pub struct UnitState {
    pub owner: u8,
    pub kind: String,
    pub x: f32,
    pub y: f32,
    pub hp: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Trigger {
    pub actions: Vec<String>,
    pub period_ms: Option<f32>,
    pub elapsed: f32,
    pub periodic: bool,
}

#[derive(Debug, Clone)]
pub struct JassRuntime {
    pub globals: HashMap<String, Value>,
    pub arrays: HashMap<String, HashMap<i64, Value>>,
    pub functions: HashMap<String, Function>,
    pub units: HashMap<u64, UnitState>,
    pub triggers: Vec<Trigger>,
    pub output: Vec<String>,
    pub next_handle: u64,
    pub rng: u64,
    pub now: f64,
    /// Declared natives (`native Foo takes … returns …`) the host does not
    /// implement, with the number of calls made, in name order.
    pub unimplemented: BTreeMap<String, u64>,
    /// Trigger-action and timer-callback failures, in call order.
    pub errors: Vec<String>,
    /// When set, an unimplemented native is a hard error instead of a recorded
    /// call that returns its declared type's default.
    pub strict_natives: bool,
    natives: HashMap<&'static str, fn(&mut JassRuntime, &[Value]) -> Result<Value, String>>,
}

impl JassRuntime {
    pub fn new(program: Program, seed: u64) -> Self {
        let mut runtime = Self {
            globals: HashMap::new(),
            arrays: HashMap::new(),
            functions: HashMap::new(),
            units: HashMap::new(),
            triggers: Vec::new(),
            output: Vec::new(),
            next_handle: 1,
            rng: seed | 1,
            now: 0.0,
            unimplemented: BTreeMap::new(),
            errors: Vec::new(),
            strict_natives: false,
            natives: HashMap::new(),
        };
        runtime.register_natives();
        for function in program.functions {
            runtime.functions.insert(function.name.clone(), function);
        }
        runtime
    }

    fn register_natives(&mut self) {
        let natives: &[(
            &'static str,
            fn(&mut JassRuntime, &[Value]) -> Result<Value, String>,
        )] = &[
            ("I2R", |_, a| Ok(Value::Real(a[0].as_int() as f64))),
            ("R2I", |_, a| Ok(Value::Int(a[0].as_real().trunc() as i64))),
            ("I2S", |_, a| Ok(Value::Str(a[0].as_int().to_string()))),
            ("R2S", |_, a| {
                Ok(Value::Str(format!("{:.3}", a[0].as_real())))
            }),
            ("S2I", |_, a| {
                Ok(Value::Int(a[0].as_str().parse().unwrap_or(0)))
            }),
            ("I2B", |_, a| Ok(Value::Bool(a[0].as_int() != 0))),
            ("B2I", |_, a| Ok(Value::Int(a[0].as_bool() as i64))),
            ("StringLength", |_, a| {
                Ok(Value::Int(a[0].as_str().chars().count() as i64))
            }),
            ("SubString", |_, a| {
                let s = a[0].as_str();
                let start = a[1].as_int().max(0) as usize;
                let end = a[2].as_int().max(0) as usize;
                Ok(Value::Str(
                    s.chars()
                        .skip(start)
                        .take(end.saturating_sub(start))
                        .collect(),
                ))
            }),
            ("ModuloInteger", |_, a| {
                let d = a[1].as_int();
                Ok(Value::Int(if d == 0 { 0 } else { a[0].as_int() % d }))
            }),
            ("ModuloReal", |_, a| {
                let d = a[1].as_real();
                Ok(Value::Real(if d == 0.0 { 0.0 } else { a[0].as_real() % d }))
            }),
            ("Pow", |_, a| {
                Ok(Value::Real(a[0].as_real().powf(a[1].as_real())))
            }),
            ("SquareRoot", |_, a| Ok(Value::Real(a[0].as_real().sqrt()))),
            ("Sin", |_, a| Ok(Value::Real(a[0].as_real().sin()))),
            ("Cos", |_, a| Ok(Value::Real(a[0].as_real().cos()))),
            ("Atan2", |_, a| {
                Ok(Value::Real(a[0].as_real().atan2(a[1].as_real())))
            }),
            ("GetRandomInt", |rt, a| {
                let min = a[0].as_int();
                let max = a[1].as_int();
                if max <= min {
                    return Ok(Value::Int(min));
                }
                rt.rng ^= rt.rng << 13;
                rt.rng ^= rt.rng >> 7;
                rt.rng ^= rt.rng << 17;
                Ok(Value::Int(min + (rt.rng % (max - min + 1) as u64) as i64))
            }),
            ("CreateUnit", |rt, a| {
                let owner = a[0].as_int().clamp(0, 23) as u8;
                let kind = a[1].as_str();
                let x = a[2].as_real() as f32;
                let y = a[3].as_real() as f32;
                let handle = rt.next_handle;
                rt.next_handle += 1;
                rt.units.insert(
                    handle,
                    UnitState {
                        owner,
                        kind,
                        x,
                        y,
                        hp: 100,
                    },
                );
                Ok(Value::Handle(handle))
            }),
            ("KillUnit", |rt, a| {
                if let Some(unit) = rt.units.get_mut(&(a[0].as_int() as u64)) {
                    unit.hp = 0;
                }
                Ok(Value::Null)
            }),
            ("RemoveUnit", |rt, a| {
                rt.units.remove(&(a[0].as_int() as u64));
                Ok(Value::Null)
            }),
            ("GetUnitX", |rt, a| {
                Ok(Value::Real(
                    rt.units
                        .get(&(a[0].as_int() as u64))
                        .map(|u| u.x as f64)
                        .unwrap_or(0.0),
                ))
            }),
            ("GetUnitY", |rt, a| {
                Ok(Value::Real(
                    rt.units
                        .get(&(a[0].as_int() as u64))
                        .map(|u| u.y as f64)
                        .unwrap_or(0.0),
                ))
            }),
            ("SetUnitPosition", |rt, a| {
                if let Some(unit) = rt.units.get_mut(&(a[0].as_int() as u64)) {
                    unit.x = a[1].as_real() as f32;
                    unit.y = a[2].as_real() as f32;
                }
                Ok(Value::Null)
            }),
            ("GetUnitState", |rt, a| {
                let value = rt
                    .units
                    .get(&(a[0].as_int() as u64))
                    .map(|u| u.hp)
                    .unwrap_or(0);
                Ok(Value::Real(value as f64))
            }),
            ("SetUnitState", |rt, a| {
                if let Some(unit) = rt.units.get_mut(&(a[0].as_int() as u64)) {
                    unit.hp = a[2].as_int();
                }
                Ok(Value::Null)
            }),
            ("GetUnitTypeId", |rt, a| {
                let code = rt
                    .units
                    .get(&(a[0].as_int() as u64))
                    .map(|u| u.kind.clone())
                    .unwrap_or_default();
                Ok(Value::Str(code))
            }),
            ("GetOwningPlayer", |rt, a| {
                let owner = rt
                    .units
                    .get(&(a[0].as_int() as u64))
                    .map(|u| u.owner as i64)
                    .unwrap_or(0);
                Ok(Value::Int(owner))
            }),
            ("DisplayTextToPlayer", |rt, a| {
                rt.output.push(a[2].as_str());
                Ok(Value::Null)
            }),
            ("BJDebugMsg", |rt, a| {
                rt.output.push(a[0].as_str());
                Ok(Value::Null)
            }),
            ("CreateTrigger", |rt, _| {
                let handle = rt.next_handle;
                rt.next_handle += 1;
                rt.triggers.push(Trigger {
                    actions: Vec::new(),
                    period_ms: None,
                    elapsed: 0.0,
                    periodic: false,
                });
                Ok(Value::Handle(handle))
            }),
            ("TriggerAddAction", |rt, a| {
                let index = (a[0].as_int() as usize).saturating_sub(1);
                if let Some(trigger) = rt.triggers.get_mut(index)
                    && let Value::Code(name) = &a[1]
                {
                    trigger.actions.push(name.clone());
                }
                Ok(Value::Null)
            }),
            ("TriggerRegisterTimerEvent", |rt, a| {
                let index = (a[0].as_int() as usize).saturating_sub(1);
                if let Some(trigger) = rt.triggers.get_mut(index) {
                    trigger.period_ms = Some((a[1].as_real() * 1000.0) as f32);
                    trigger.periodic = a[2].as_bool();
                    trigger.elapsed = 0.0;
                }
                Ok(Value::Null)
            }),
            ("TriggerExecute", |rt, a| {
                let index = (a[0].as_int() as usize).saturating_sub(1);
                if let Some(trigger) = rt.triggers.get(index).cloned() {
                    for action in trigger.actions {
                        if let Err(error) = rt.call_function(&action, &[]) {
                            rt.errors
                                .push(format!("TriggerExecute action {action}: {error}"));
                        }
                    }
                }
                Ok(Value::Null)
            }),
            ("PolledWait", |_, a| Ok(Value::Real(a[0].as_real()))),
            ("GetGameTime", |rt, _| Ok(Value::Real(rt.now))),
        ];
        for (name, function) in natives {
            self.natives.insert(name, *function);
        }
    }

    pub fn run_function(&mut self, name: &str) -> Result<Value, String> {
        self.call_function(name, &[])
    }

    fn call_function(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        if let Some(native) = self.natives.get(name).copied() {
            return native(self, args);
        }
        let function = self
            .functions
            .get(name)
            .cloned()
            .ok_or_else(|| format!("unknown function {name}"))?;
        if function.native {
            return self.unimplemented_native(name, &function);
        }
        let mut locals: HashMap<String, Value> = HashMap::new();
        for (index, (_, param)) in function.params.iter().enumerate() {
            locals.insert(
                param.clone(),
                args.get(index).cloned().unwrap_or(Value::Null),
            );
        }
        let mut control = Control::Normal;
        let result = self.exec_block(&function.body, &mut locals, &mut control)?;
        Ok(result.unwrap_or(Value::Null))
    }

    /// A declared native the host has no handler for.
    ///
    /// Counted in [`Self::unimplemented`] and answered with its declared
    /// return type's default, so the script continues with a correctly typed
    /// value rather than a `null` that silently changes every comparison it
    /// feeds. Under [`Self::set_strict_natives`] the call is an error instead,
    /// which is how a caller proves that a run used only implemented natives.
    fn unimplemented_native(&mut self, name: &str, function: &Function) -> Result<Value, String> {
        *self.unimplemented.entry(name.to_string()).or_insert(0) += 1;
        if self.strict_natives {
            let params = function
                .params
                .iter()
                .map(|(ty, name)| format!("{ty} {name}"))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "unimplemented native {name} (takes {params} returns {})",
                function.returns
            ));
        }
        Ok(default_for_type(&function.returns))
    }

    /// Turns calls to unimplemented declared natives into hard errors.
    pub fn set_strict_natives(&mut self, strict: bool) {
        self.strict_natives = strict;
    }

    /// Total number of calls to natives the host does not implement.
    pub fn unimplemented_calls(&self) -> u64 {
        self.unimplemented.values().sum()
    }

    /// Drains the recorded trigger/timer failures (leaving the ledger empty).
    pub fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.errors)
    }

    fn exec_block(
        &mut self,
        statements: &[Stmt],
        locals: &mut HashMap<String, Value>,
        control: &mut Control,
    ) -> Result<Option<Value>, String> {
        for stmt in statements {
            if !matches!(control, Control::Normal) {
                break;
            }
            match stmt {
                Stmt::Local(name, init) => {
                    let value = if let Some(expr) = init {
                        self.eval(expr, locals)?
                    } else {
                        Value::Null
                    };
                    locals.insert(name.clone(), value);
                }
                Stmt::Set(name, index, value) => {
                    let value = self.eval(value, locals)?;
                    if let Some(index) = index {
                        let index = self.eval(index, locals)?.as_int();
                        let is_local = locals.contains_key(name);
                        if is_local {
                            // local arrays are not part of JASS2 classic; use globals
                            self.arrays
                                .entry(name.clone())
                                .or_default()
                                .insert(index, value);
                        } else {
                            self.arrays
                                .entry(name.clone())
                                .or_default()
                                .insert(index, value);
                        }
                    } else if locals.contains_key(name) {
                        locals.insert(name.clone(), value);
                    } else {
                        self.globals.insert(name.clone(), value);
                    }
                }
                Stmt::Call(expr) => {
                    self.eval(expr, locals)?;
                }
                Stmt::If(cond, then, otherwise) => {
                    if self.eval(cond, locals)?.as_bool() {
                        let result = self.exec_block(then, locals, control)?;
                        if result.is_some() {
                            return Ok(result);
                        }
                    } else {
                        let result = self.exec_block(otherwise, locals, control)?;
                        if result.is_some() {
                            return Ok(result);
                        }
                    }
                }
                Stmt::Loop(body) => {
                    let mut guard = 0u64;
                    loop {
                        guard += 1;
                        if guard > 10_000_000 {
                            return Err("loop guard exceeded".into());
                        }
                        let mut inner = Control::Normal;
                        let result = self.exec_block(body, locals, &mut inner)?;
                        if result.is_some() {
                            return Ok(result);
                        }
                        if matches!(inner, Control::Break) {
                            break;
                        }
                    }
                }
                Stmt::ExitWhen(expr) => {
                    if self.eval(expr, locals)?.as_bool() {
                        *control = Control::Break;
                    }
                }
                Stmt::Return(expr) => {
                    let value = if let Some(expr) = expr {
                        self.eval(expr, locals)?
                    } else {
                        Value::Null
                    };
                    return Ok(Some(value));
                }
            }
        }
        Ok(None)
    }

    fn eval(&mut self, expr: &Expr, locals: &HashMap<String, Value>) -> Result<Value, String> {
        Ok(match expr {
            Expr::Int(v) => Value::Int(*v),
            Expr::Real(v) => Value::Real(*v),
            Expr::Str(v) => Value::Str(v.clone()),
            Expr::Bool(v) => Value::Bool(*v),
            Expr::Null => Value::Null,
            Expr::FuncRef(name) => Value::Code(name.clone()),
            Expr::Var(name) => locals
                .get(name)
                .or_else(|| self.globals.get(name))
                .cloned()
                .unwrap_or(Value::Null),
            Expr::Array(name, index) => {
                let index = self.eval(index, locals)?.as_int();
                self.arrays
                    .get(name)
                    .and_then(|map| map.get(&index))
                    .cloned()
                    .unwrap_or(Value::Null)
            }
            Expr::Call(name, args) => {
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(arg, locals)?);
                }
                self.call_function(name, &values)?
            }
            Expr::Bin(op, lhs, rhs) => {
                let l = self.eval(lhs, locals)?;
                let r = self.eval(rhs, locals)?;
                match op {
                    BinOp::Add if matches!(l, Value::Str(_)) || matches!(r, Value::Str(_)) => {
                        Value::Str(format!("{}{}", l.as_str(), r.as_str()))
                    }
                    BinOp::Add => match (&l, &r) {
                        (Value::Real(_), _) | (_, Value::Real(_)) => {
                            Value::Real(l.as_real() + r.as_real())
                        }
                        _ => Value::Int(l.as_int() + r.as_int()),
                    },
                    BinOp::Sub => match (&l, &r) {
                        (Value::Real(_), _) | (_, Value::Real(_)) => {
                            Value::Real(l.as_real() - r.as_real())
                        }
                        _ => Value::Int(l.as_int() - r.as_int()),
                    },
                    BinOp::Mul => match (&l, &r) {
                        (Value::Real(_), _) | (_, Value::Real(_)) => {
                            Value::Real(l.as_real() * r.as_real())
                        }
                        _ => Value::Int(l.as_int() * r.as_int()),
                    },
                    BinOp::Div => {
                        if r.as_real() == 0.0 {
                            Value::Real(0.0)
                        } else {
                            Value::Real(l.as_real() / r.as_real())
                        }
                    }
                    BinOp::Mod => {
                        let d = r.as_int();
                        Value::Int(if d == 0 { 0 } else { l.as_int() % d })
                    }
                    BinOp::Eq => Value::Bool(
                        l == r
                            || (l.as_real() == r.as_real()
                                && matches!((&l, &r), (Value::Real(_), _) | (_, Value::Real(_)))),
                    ),
                    BinOp::Ne => Value::Bool(!(l == r)),
                    BinOp::Lt => Value::Bool(l.as_real() < r.as_real()),
                    BinOp::Le => Value::Bool(l.as_real() <= r.as_real()),
                    BinOp::Gt => Value::Bool(l.as_real() > r.as_real()),
                    BinOp::Ge => Value::Bool(l.as_real() >= r.as_real()),
                    BinOp::And => Value::Bool(l.as_bool() && r.as_bool()),
                    BinOp::Or => Value::Bool(l.as_bool() || r.as_bool()),
                }
            }
            Expr::Un(op, inner) => {
                let value = self.eval(inner, locals)?;
                match op {
                    '-' => match value {
                        Value::Real(v) => Value::Real(-v),
                        Value::Int(v) => Value::Int(-v),
                        _ => Value::Int(-value.as_int()),
                    },
                    '!' => Value::Bool(!value.as_bool()),
                    _ => value,
                }
            }
        })
    }

    /// Advance timers/triggers by `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        self.now += dt as f64;
        let due: Vec<String> = self
            .triggers
            .iter_mut()
            .filter_map(|trigger| {
                let period = trigger.period_ms? / 1000.0;
                trigger.elapsed += dt;
                if period > 0.0 && trigger.elapsed + f32::EPSILON >= period {
                    trigger.elapsed = if trigger.periodic {
                        0.0
                    } else {
                        trigger.elapsed
                    };
                    Some(trigger.actions.clone())
                } else {
                    None
                }
            })
            .flatten()
            .collect();
        for action in due {
            if let Err(error) = self.call_function(&action, &[]) {
                self.errors.push(format!("timer action {action}: {error}"));
            }
        }
    }
}

/// The value a JASS type produces when nothing assigns one.
///
/// A declared native with no host implementation answers with this instead of a
/// type-less [`Value::Null`]: `integer`/`real`/`boolean`/`string` compare and
/// print the way the language defines an empty result, and a handle is `null`.
fn default_for_type(returns: &str) -> Value {
    match returns.trim().to_ascii_lowercase().as_str() {
        "boolean" => Value::Bool(false),
        "integer" => Value::Int(0),
        "real" => Value::Real(0.0),
        "string" => Value::Str(String::new()),
        "code" | "nothing" => Value::Null,
        _ => Value::Handle(0),
    }
}

enum Control {
    Normal,
    Break,
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
            call BJDebugMsg("counter=" + I2S(counter))
        endfunction
    "#;

    #[test]
    fn runs_loops_calls_and_strings() {
        let program = parse(SCRIPT).expect("parse");
        let mut rt = JassRuntime::new(program, 1);
        rt.run_function("main").expect("run");
        assert_eq!(rt.output, vec!["counter=36"]);
        assert_eq!(rt.globals.get("counter"), Some(&Value::Int(36)));
    }

    #[test]
    fn arrays_and_math_natives() {
        let script = r#"
            globals
                integer array values
            endglobals
            function main takes nothing returns nothing
                set values[2] = 7
                set values[3] = ModuloInteger(10, 4)
                call BJDebugMsg(I2S(values[2] + values[3]))
            endfunction
        "#;
        let program = parse(script).expect("parse");
        let mut rt = JassRuntime::new(program, 1);
        rt.run_function("main").expect("run");
        assert_eq!(rt.output, vec!["9"]);
    }

    #[test]
    fn triggers_fire_on_timer() {
        let script = r#"
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
        let program = parse(script).expect("parse");
        let mut rt = JassRuntime::new(program, 1);
        rt.run_function("main").expect("run");
        rt.tick(0.5);
        rt.tick(0.5);
        assert_eq!(rt.globals.get("n"), Some(&Value::Int(2)));
    }

    #[test]
    fn bare_return_terminates_at_the_newline() {
        parse("function f takes nothing returns nothing\n    return\nendfunction\n")
            .expect("bare return at end");
        parse("function f takes nothing returns nothing\n    return\n    set x = 1\nendfunction\n")
            .expect("bare return before statement");
        parse("function f takes nothing returns nothing\n    if a then\n        return\n    endif\nendfunction\n")
            .expect("bare return in if");
    }

    #[test]
    fn bare_return_skips_following_statements_at_runtime() {
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
        let program = parse(script).expect("parse");
        let mut rt = JassRuntime::new(program, 1);
        rt.run_function("main").expect("run");
        assert_eq!(
            rt.globals.get("n").cloned().unwrap_or(Value::Int(0)),
            Value::Int(0),
            "the statement after a bare return must not run"
        );
    }

    #[test]
    fn elseif_keeps_the_first_true_branch() {
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
        let program = parse(script).expect("parse");
        let mut rt = JassRuntime::new(program, 1);
        rt.run_function("main").expect("run");
        assert_eq!(rt.globals.get("n"), Some(&Value::Int(2)));
    }

    #[test]
    fn elseif_chain_falls_through_to_else_once() {
        let script = r#"
            globals
                integer n = 0
            endglobals
            function main takes nothing returns nothing
                if false then
                    set n = 1
                elseif false then
                    set n = 2
                elseif false then
                    set n = 3
                else
                    set n = 4
                endif
            endfunction
        "#;
        let program = parse(script).expect("parse");
        let mut rt = JassRuntime::new(program, 1);
        rt.run_function("main").expect("run");
        assert_eq!(rt.globals.get("n"), Some(&Value::Int(4)));
    }

    #[test]
    fn elseif_chain_runs_a_middle_branch() {
        let script = r#"
            globals
                integer n = 0
            endglobals
            function main takes nothing returns nothing
                if false then
                    set n = 1
                elseif false then
                    set n = 2
                elseif true then
                    set n = 3
                elseif true then
                    set n = 4
                endif
            endfunction
        "#;
        let program = parse(script).expect("parse");
        let mut rt = JassRuntime::new(program, 1);
        rt.run_function("main").expect("run");
        assert_eq!(rt.globals.get("n"), Some(&Value::Int(3)));
    }

    #[test]
    fn creates_units() {
        let script = r#"
            function main takes nothing returns nothing
                local integer u = CreateUnit(0, "hfoo", 100.0, 200.0, 0)
                call SetUnitPosition(u, 300.0, 400.0)
                call BJDebugMsg(R2S(GetUnitX(u)))
            endfunction
        "#;
        let program = parse(script).expect("parse");
        let mut rt = JassRuntime::new(program, 1);
        rt.run_function("main").expect("run");
        assert_eq!(rt.output, vec!["300.000"]);
        assert_eq!(rt.units.len(), 1);
    }

    /// A script may declare natives the host does not model (`GetWidgetLife` on a
    /// bare VM). The call must be recorded and answer its declared type, never a
    /// `null` that silently changes every comparison downstream.
    const DECLARED_NATIVES: &str = r#"
        native GetWidgetLife takes widget whichWidget returns real
        native GetTileType takes integer x, integer y returns integer
        native IsVisible takes nothing returns boolean
        native GetLastCreatedUnit takes nothing returns unit
        native ShowUnitHide takes unit whichUnit returns nothing

        globals
            real life = -1.0
            integer tile = -1
            boolean seen = true
            integer last = -1
        endglobals

        function main takes nothing returns nothing
            set life = GetWidgetLife(null)
            set tile = GetTileType(1, 2)
            set seen = IsVisible()
            set last = GetLastCreatedUnit()
            call ShowUnitHide(null)
        endfunction
    "#;

    #[test]
    fn unimplemented_native_is_recorded_with_typed_default() {
        let program = parse(DECLARED_NATIVES).expect("parse");
        let mut rt = JassRuntime::new(program, 1);
        rt.run_function("main").expect("run");
        let counts: Vec<(&str, u64)> = rt
            .unimplemented
            .iter()
            .map(|(name, count)| (name.as_str(), *count))
            .collect();
        assert_eq!(
            counts,
            vec![
                ("GetLastCreatedUnit", 1),
                ("GetTileType", 1),
                ("GetWidgetLife", 1),
                ("IsVisible", 1),
                ("ShowUnitHide", 1),
            ],
            "every declared-but-unimplemented native is counted"
        );
        assert_eq!(rt.unimplemented_calls(), 5);
        assert_eq!(rt.globals.get("life"), Some(&Value::Real(0.0)));
        assert_eq!(rt.globals.get("tile"), Some(&Value::Int(0)));
        assert_eq!(rt.globals.get("seen"), Some(&Value::Bool(false)));
        // A handle-returning native answers null, not a fabricated handle.
        assert_eq!(rt.globals.get("last"), Some(&Value::Handle(0)));
        assert!(rt.errors.is_empty(), "{:?}", rt.errors);
    }

    #[test]
    fn strict_natives_turn_unimplemented_into_an_error() {
        let program = parse(DECLARED_NATIVES).expect("parse");
        let mut rt = JassRuntime::new(program, 1);
        rt.set_strict_natives(true);
        let error = rt.run_function("main").expect_err("strict run must fail");
        assert!(
            error.contains("unimplemented native GetWidgetLife"),
            "error names the native and its declared signature: {error}"
        );
        assert!(
            error.contains("takes widget whichWidget returns real"),
            "{error}"
        );
        assert_eq!(rt.unimplemented.get("GetWidgetLife"), Some(&1));
    }

    const FAILING_ACTION: &str = r#"
        globals
            trigger t
            integer fired = 0
        endglobals

        function action takes nothing returns nothing
            set fired = fired + 1
            call NoSuchFunction()
        endfunction

        function main takes nothing returns nothing
            set t = CreateTrigger()
            call TriggerAddAction(t, function action)
            call TriggerRegisterTimerEvent(t, 0.5, true)
        endfunction
    "#;

    #[test]
    fn timer_action_failure_is_recorded_not_dropped() {
        let program = parse(FAILING_ACTION).expect("parse");
        let mut rt = JassRuntime::new(program, 1);
        rt.run_function("main").expect("main");
        rt.tick(0.5);
        assert_eq!(rt.globals.get("fired"), Some(&Value::Int(1)));
        assert_eq!(rt.errors.len(), 1, "{:?}", rt.errors);
        assert!(
            rt.errors[0].starts_with("timer action action: unknown function NoSuchFunction"),
            "{}",
            rt.errors[0]
        );
        let drained = rt.take_errors();
        assert_eq!(drained.len(), 1);
        assert!(rt.errors.is_empty(), "take_errors drains the ledger");
    }

    #[test]
    fn trigger_execute_failure_is_recorded_not_dropped() {
        let script = r#"
            globals
                trigger t
            endglobals

            function action takes nothing returns nothing
                call MissingInAction()
            endfunction

            function main takes nothing returns nothing
                set t = CreateTrigger()
                call TriggerAddAction(t, function action)
                call TriggerExecute(t)
            endfunction
        "#;
        let program = parse(script).expect("parse");
        let mut rt = JassRuntime::new(program, 1);
        rt.run_function("main").expect("main");
        assert_eq!(rt.errors.len(), 1, "{:?}", rt.errors);
        assert!(
            rt.errors[0]
                .starts_with("TriggerExecute action action: unknown function MissingInAction"),
            "{}",
            rt.errors[0]
        );
    }
}
