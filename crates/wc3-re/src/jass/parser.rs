//! Recursive-descent JASS2 parser.

use std::fmt;

use super::ast::*;
use super::lexer::{Token, TokenKind, lex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: u32,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parse one JASS source file.
pub fn parse(source: &str) -> Result<Program, ParseError> {
    let tokens = lex(source).map_err(|err| ParseError {
        line: err.line,
        message: err.message,
    })?;
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
        source_name: String::new(),
    };
    parser.parse_program()
}

/// Parse with a diagnostic source name.
pub fn parse_named(source: &str, name: impl Into<String>) -> Result<Program, ParseError> {
    let mut program = parse(source)?;
    program.source = name.into();
    Ok(program)
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    source_name: String,
}

impl<'a> Parser<'a> {
    fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut program = Program {
            source: self.source_name.clone(),
            ..Program::default()
        };
        self.skip_newlines();
        while !self.is_eof() {
            if self.check_word("type") {
                program.types.push(self.parse_type()?);
            } else if self.check_word("native") {
                program.natives.push(self.parse_native()?);
            } else if self.check_word("constant") {
                if self.check_word_at(1, "native") {
                    self.advance();
                    program.natives.push(self.parse_native()?);
                } else {
                    program.globals.push(self.parse_global_decl()?);
                }
            } else if self.check_word("globals") {
                self.advance();
                program.globals.extend(self.parse_globals()?);
            } else if self.check_word("function") {
                program.functions.push(self.parse_function()?);
            } else {
                let token = self.peek();
                return Err(self.error_at(token, "expected type, native, globals or function"));
            }
            self.skip_newlines();
        }
        Ok(program)
    }

    fn parse_type(&mut self) -> Result<TypeDecl, ParseError> {
        let line = self.advance().line;
        let name = self.expect_ident("type name")?;
        self.expect_word("extends")?;
        let extends = self.expect_ident("base type")?;
        self.expect_newline()?;
        Ok(TypeDecl {
            name,
            extends,
            line,
        })
    }

    fn parse_native(&mut self) -> Result<NativeDecl, ParseError> {
        let line = self.advance().line;
        let name = self.expect_ident("native name")?;
        self.expect_word("takes")?;
        let params = self.parse_params()?;
        self.expect_word("returns")?;
        let ret = self.expect_ident("return type")?;
        self.expect_newline()?;
        Ok(NativeDecl {
            name,
            params,
            ret,
            line,
        })
    }

    fn parse_params(&mut self) -> Result<Vec<(String, String)>, ParseError> {
        if self.check_word("nothing") {
            self.advance();
            return Ok(Vec::new());
        }
        let mut params = Vec::new();
        loop {
            let ty = self.expect_ident("parameter type")?;
            let name = self.expect_ident("parameter name")?;
            params.push((ty, name));
            if self.eat(TokenKind::Comma) {
                continue;
            }
            break;
        }
        Ok(params)
    }

    fn parse_globals(&mut self) -> Result<Vec<GlobalDecl>, ParseError> {
        let mut globals = Vec::new();
        loop {
            self.skip_newlines();
            if self.is_eof() {
                return Err(self.error_here("unterminated globals block"));
            }
            if self.check_word("endglobals") {
                self.advance();
                self.expect_newline()?;
                break;
            }
            globals.push(self.parse_global_decl()?);
        }
        Ok(globals)
    }

    fn parse_global_decl(&mut self) -> Result<GlobalDecl, ParseError> {
        let line = self.peek().line;
        let mut constant = false;
        if self.check_word("constant") {
            self.advance();
            constant = true;
        }
        let ty = self.expect_ident("global type")?;
        if self.check_word("constant") {
            self.advance();
            constant = true;
        }
        let array = self.eat_word("array");
        let name = self.expect_ident("global name")?;
        let array = array || self.eat_word("array");
        let init = if self.eat(TokenKind::Eq) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect_newline()?;
        Ok(GlobalDecl {
            ty,
            name,
            array,
            constant,
            init,
            line,
        })
    }

    fn parse_function(&mut self) -> Result<FunctionDecl, ParseError> {
        let line = self.advance().line;
        let name = self.expect_ident("function name")?;
        self.expect_word("takes")?;
        let params = self.parse_params()?;
        self.expect_word("returns")?;
        let ret = self.expect_ident("return type")?;
        self.expect_newline()?;
        let mut locals = Vec::new();
        let mut body = Vec::new();
        self.skip_newlines();
        while !self.is_eof() {
            if self.check_word("endfunction") {
                self.advance();
                self.expect_newline()?;
                break;
            }
            let stmt = self.parse_stmt()?;
            if let Stmt::Local(local) = &stmt {
                locals.push(local.clone());
            }
            body.push(stmt);
            self.skip_newlines();
        }
        if self.is_eof() && !self.previous_is_word("endfunction") {
            // The loop exits at EOF; only accept it if endfunction was eaten.
            if !matches!(body.last(), Some(_)) {
                return Err(self.error_here("unterminated function"));
            }
        }
        Ok(FunctionDecl {
            name,
            params,
            ret,
            locals,
            body,
            line,
        })
    }

    fn parse_block(&mut self, stops: &[&str]) -> Result<Vec<Stmt>, ParseError> {
        let mut body = Vec::new();
        loop {
            self.skip_newlines();
            if self.is_eof() {
                return Err(self.error_here("unterminated block"));
            }
            if stops.iter().any(|stop| self.check_word(stop)) {
                break;
            }
            body.push(self.parse_stmt()?);
        }
        Ok(body)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        let token = self.peek().clone();
        if self.check_word("local") {
            self.advance();
            let ty = self.expect_ident("local type")?;
            let array_before = self.eat_word("array");
            let name = self.expect_ident("local name")?;
            let array = array_before || self.eat_word("array");
            let init = if self.eat(TokenKind::Eq) {
                Some(self.parse_expr()?)
            } else {
                None
            };
            self.expect_newline()?;
            return Ok(Stmt::Local(LocalDecl {
                ty,
                name,
                array,
                init,
                line: token.line,
            }));
        }
        if self.check_word("set") {
            self.advance();
            let name = self.expect_ident("assignment target")?;
            let index = if self.eat(TokenKind::LBracket) {
                let expr = self.parse_expr()?;
                self.expect(TokenKind::RBracket, "']'")?;
                Some(expr)
            } else {
                None
            };
            self.expect(TokenKind::Eq, "'='")?;
            let value = self.parse_expr()?;
            self.expect_newline()?;
            return Ok(Stmt::Set {
                name,
                index,
                value,
                line: token.line,
            });
        }
        if self.check_word("call") {
            self.advance();
            let name = self.expect_ident("called function")?;
            let args = self.parse_args()?;
            self.expect_newline()?;
            return Ok(Stmt::Call {
                name,
                args,
                line: token.line,
            });
        }
        if self.check_word("if") {
            return self.parse_if();
        }
        if self.check_word("loop") {
            self.advance();
            let body = self.parse_block(&["endloop"])?;
            self.expect_word("endloop")?;
            self.expect_newline()?;
            return Ok(Stmt::Loop {
                body,
                line: token.line,
            });
        }
        if self.check_word("exitwhen") {
            self.advance();
            let cond = self.parse_expr()?;
            self.expect_newline()?;
            return Ok(Stmt::ExitWhen {
                cond,
                line: token.line,
            });
        }
        if self.check_word("return") {
            self.advance();
            let value = if self.at_newline_or_eof() {
                None
            } else {
                Some(self.parse_expr()?)
            };
            self.expect_newline()?;
            return Ok(Stmt::Return {
                value,
                line: token.line,
            });
        }
        if self.check_word("debug") {
            self.advance();
            return self.parse_stmt();
        }
        Err(self.error_at(&token, "expected a statement"))
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        let line = self.advance().line;
        let mut branches = Vec::new();
        let cond = self.parse_expr()?;
        self.expect_word("then")?;
        let body = self.parse_block(&["elseif", "else", "endif"])?;
        branches.push((cond, body));
        let mut else_body = None;
        loop {
            if self.check_word("elseif") {
                self.advance();
                let cond = self.parse_expr()?;
                self.expect_word("then")?;
                let body = self.parse_block(&["elseif", "else", "endif"])?;
                branches.push((cond, body));
                continue;
            }
            if self.check_word("else") {
                self.advance();
                else_body = Some(self.parse_block(&["endif"])?);
                continue;
            }
            break;
        }
        self.expect_word("endif")?;
        self.expect_newline()?;
        Ok(Stmt::If {
            branches,
            else_body,
            line,
        })
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>, ParseError> {
        self.expect(TokenKind::LParen, "'('")?;
        let mut args = Vec::new();
        if self.eat(TokenKind::RParen) {
            return Ok(args);
        }
        loop {
            args.push(self.parse_expr()?);
            if self.eat(TokenKind::Comma) {
                continue;
            }
            break;
        }
        self.expect(TokenKind::RParen, "')'")?;
        Ok(args)
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while self.check_word("or") {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::Binary(BinOp::Or, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_cmp()?;
        while self.check_word("and") {
            self.advance();
            let right = self.parse_cmp()?;
            left = Expr::Binary(BinOp::And, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_cmp(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_add()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::NotEq => BinOp::Ne,
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Le => BinOp::Le,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::Ge => BinOp::Ge,
                _ => break,
            };
            self.advance();
            let right = self.parse_add()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_mul()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_mul()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if self.check_word("not") {
            self.advance();
            return Ok(Expr::Unary(UnOp::Not, Box::new(self.parse_unary()?)));
        }
        if self.eat(TokenKind::Minus) {
            return Ok(Expr::Unary(UnOp::Neg, Box::new(self.parse_unary()?)));
        }
        if self.eat(TokenKind::Plus) {
            return Ok(Expr::Unary(UnOp::Pos, Box::new(self.parse_unary()?)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let token = self.peek().clone();
        match &token.kind {
            TokenKind::Int(value) => {
                self.advance();
                Ok(Expr::Int(*value))
            }
            TokenKind::Real(value) => {
                self.advance();
                Ok(Expr::Real(*value))
            }
            TokenKind::Str(value) => {
                self.advance();
                Ok(Expr::Str(value.clone()))
            }
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                match name.as_str() {
                    "true" => Ok(Expr::Bool(true)),
                    "false" => Ok(Expr::Bool(false)),
                    "null" => Ok(Expr::Null),
                    "function" => {
                        let target = self.expect_ident("function reference")?;
                        Ok(Expr::Code(target))
                    }
                    _ => {
                        if self.check(TokenKind::LParen) {
                            let args = self.parse_args()?;
                            Ok(Expr::Call(name, args))
                        } else if self.eat(TokenKind::LBracket) {
                            let index = self.parse_expr()?;
                            self.expect(TokenKind::RBracket, "']'")?;
                            Ok(Expr::Array(name, Box::new(index)))
                        } else {
                            Ok(Expr::Var(name))
                        }
                    }
                }
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(TokenKind::RParen, "')'")?;
                Ok(expr)
            }
            _ => Err(self.error_at(&token, "expected an expression")),
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn advance(&mut self) -> &Token {
        let index = self.pos.min(self.tokens.len() - 1);
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        &self.tokens[index]
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.peek().kind == kind
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn check_word(&self, word: &str) -> bool {
        matches!(&self.peek().kind, TokenKind::Ident(name) if name.eq_ignore_ascii_case(word))
    }

    fn check_word_at(&self, offset: usize, word: &str) -> bool {
        matches!(
            self.tokens.get(self.pos + offset).map(|token| &token.kind),
            Some(TokenKind::Ident(name)) if name.eq_ignore_ascii_case(word)
        )
    }

    fn eat_word(&mut self, word: &str) -> bool {
        if self.check_word(word) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_word(&mut self, word: &str) -> Result<(), ParseError> {
        if self.eat_word(word) {
            Ok(())
        } else {
            Err(self.error_here(&format!("expected '{word}'")))
        }
    }

    fn expect_ident(&mut self, what: &str) -> Result<String, ParseError> {
        match self.peek().kind.clone() {
            TokenKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            _ => Err(self.error_here(&format!("expected {what}"))),
        }
    }

    fn expect(&mut self, kind: TokenKind, what: &str) -> Result<(), ParseError> {
        if self.eat(kind) {
            Ok(())
        } else {
            Err(self.error_here(&format!("expected {what}")))
        }
    }

    fn expect_newline(&mut self) -> Result<(), ParseError> {
        if self.is_eof() || self.check(TokenKind::Newline) {
            self.skip_newlines();
            Ok(())
        } else {
            Err(self.error_here("expected end of line"))
        }
    }

    fn at_newline_or_eof(&self) -> bool {
        self.is_eof() || self.check(TokenKind::Newline)
    }

    fn skip_newlines(&mut self) {
        while self.check(TokenKind::Newline) {
            self.advance();
        }
    }

    fn is_eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn previous_is_word(&self, word: &str) -> bool {
        self.pos > 0
            && matches!(&self.tokens[self.pos - 1].kind, TokenKind::Ident(name) if name.eq_ignore_ascii_case(word))
    }

    fn error_at(&self, token: &Token, message: &str) -> ParseError {
        ParseError {
            line: token.line,
            message: format!("{message}, found {:?}", token.kind),
        }
    }

    fn error_here(&self, message: &str) -> ParseError {
        self.error_at(self.peek(), message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_core_constructs() {
        let source = r#"
type mytype extends handle
native Foo takes integer a, real b returns boolean
globals
    constant integer MAX = 4
    unit array udg_units
endglobals
function Bar takes nothing returns nothing
    local integer i = 0
    loop
        exitwhen i >= MAX
        if i == 2 then
            call BJDebugMsg("two")
        elseif i > 2 then
            set i = i + 1
        else
            set i = i + 1
        endif
    endloop
endfunction
"#;
        let program = parse(source).expect("parse");
        assert_eq!(program.types.len(), 1);
        assert_eq!(program.natives.len(), 1);
        assert_eq!(program.globals.len(), 2);
        assert_eq!(program.functions.len(), 1);
    }

    #[test]
    fn bare_return_does_not_consume_the_next_statement() {
        let source = "function f takes nothing returns nothing\n    return\n    set x = 1\nendfunction\n";
        let program = parse(source).expect("parse");
        let body = &program.functions[0].body;
        assert_eq!(body.len(), 2, "return + set are two statements: {body:?}");
        assert!(matches!(body[0], Stmt::Return { value: None, .. }));
        assert!(matches!(body[1], Stmt::Set { .. }));
    }

    #[test]
    fn elseif_chain_keeps_every_branch_and_the_else() {
        let source = r#"
function f takes nothing returns nothing
    if a then
        set n = 1
    elseif b then
        set n = 2
    elseif c then
        set n = 3
    else
        set n = 4
    endif
endfunction
"#;
        let program = parse(source).expect("parse");
        match &program.functions[0].body[0] {
            Stmt::If { branches, else_body, .. } => {
                assert_eq!(branches.len(), 3, "first branch + two elseifs");
                assert_eq!(branches[0].0, Expr::Var("a".into()));
                assert_eq!(branches[1].0, Expr::Var("b".into()));
                assert_eq!(branches[2].0, Expr::Var("c".into()));
                assert!(else_body.is_some(), "else body retained");
            }
            other => panic!("expected if, got {other:?}"),
        }
    }

    #[test]
    fn parses_real_common_and_map_script() {
        for path in [
            "../../analysis/war3_full3/Scripts/common.j",
            "../../analysis/war3_full3/Scripts/Blizzard.j",
            "../../analysis/war3_map/Human04/war3map.j",
        ] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
            let Ok(source) = std::fs::read_to_string(&path) else {
                eprintln!("skip: {} missing", path.display());
                continue;
            };
            let program = parse(&source).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
            assert!(!program.functions.is_empty() || !program.natives.is_empty());
        }
    }
}
