//! JASS abstract syntax tree.

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Real(f64),
    Bool(bool),
    Str(String),
    Null,
    Var(String),
    /// `name[index]`
    Array(String, Box<Expr>),
    Call(String, Vec<Expr>),
    /// `function Name` used as a `code` value.
    Code(String),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    Pos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalDecl {
    pub ty: String,
    pub name: String,
    pub array: bool,
    pub init: Option<Expr>,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Local(LocalDecl),
    /// `set name = expr` / `set name[index] = expr`
    Set {
        name: String,
        index: Option<Expr>,
        value: Expr,
        line: u32,
    },
    Call {
        name: String,
        args: Vec<Expr>,
        line: u32,
    },
    If {
        branches: Vec<(Expr, Vec<Stmt>)>,
        else_body: Option<Vec<Stmt>>,
        line: u32,
    },
    Loop {
        body: Vec<Stmt>,
        line: u32,
    },
    ExitWhen {
        cond: Expr,
        line: u32,
    },
    Return {
        value: Option<Expr>,
        line: u32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDecl {
    pub name: String,
    pub extends: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeDecl {
    pub name: String,
    pub params: Vec<(String, String)>,
    pub ret: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlobalDecl {
    pub ty: String,
    pub name: String,
    pub array: bool,
    pub constant: bool,
    pub init: Option<Expr>,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<(String, String)>,
    pub ret: String,
    pub locals: Vec<LocalDecl>,
    pub body: Vec<Stmt>,
    pub line: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub source: String,
    pub types: Vec<TypeDecl>,
    pub globals: Vec<GlobalDecl>,
    pub natives: Vec<NativeDecl>,
    pub functions: Vec<FunctionDecl>,
}

impl Program {
    /// Merge another program into this one (common.j + Blizzard.j + map).
    pub fn merge(&mut self, other: Program) {
        self.types.extend(other.types);
        self.globals.extend(other.globals);
        self.natives.extend(other.natives);
        self.functions.extend(other.functions);
    }

    pub fn function(&self, name: &str) -> Option<&FunctionDecl> {
        self.functions
            .iter()
            .find(|function| function.name.eq_ignore_ascii_case(name))
    }

    pub fn native(&self, name: &str) -> Option<&NativeDecl> {
        self.natives
            .iter()
            .find(|native| native.name.eq_ignore_ascii_case(name))
    }

    pub fn function_names(&self) -> std::collections::BTreeSet<String> {
        self.functions
            .iter()
            .map(|function| function.name.clone())
            .collect()
    }
}
