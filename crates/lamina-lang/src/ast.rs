//! AST for Lamina (0.1 MVP + 0.2 modules / mounts).

use crate::span::Span;

#[derive(Debug, Clone)]
pub struct Module {
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Item {
    Use(UseDecl),
    Arg(ArgDecl),
    Const(ConstDecl),
    Let(LetDecl),
    Fn(FnDecl),
    Target(TargetDecl),
}

#[derive(Debug, Clone)]
pub struct UseDecl {
    /// Path string as written: `"./lib.lam"` or `"std/golang.lam"`.
    pub path: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ArgDecl {
    pub name: String,
    pub default: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ConstDecl {
    pub name: String,
    pub ty: Type,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct LetDecl {
    pub name: String,
    pub ty: Option<Type>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FnDecl {
    pub is_pub: bool,
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Type,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TargetDecl {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    String,
    Int,
    Bool,
    Stage,
    Mount,
    List(Box<Type>),
}

impl Type {
    pub fn as_str(&self) -> String {
        match self {
            Type::String => "String".into(),
            Type::Int => "Int".into(),
            Type::Bool => "Bool".into(),
            Type::Stage => "Stage".into(),
            Type::Mount => "Mount".into(),
            Type::List(t) => format!("List[{}]", t.as_str()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<BlockStmt>,
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum BlockStmt {
    Let(LetDecl),
    /// `name = expr;` — reassign an existing binding (e.g. accumulate a Stage in a loop).
    Assign {
        name: String,
        value: Expr,
        span: crate::span::Span,
    },
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    String(String),
    StringInterp(Vec<InterpPart>),
    Int(i64),
    Bool(bool),
    Ident(String),
    List(Vec<Expr>),
    Call {
        callee: String,
        args: Vec<Expr>,
    },
    Method {
        recv: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    StageFrom {
        image: Box<Expr>,
    },
    StageFromArg {
        name: String,
    },
    /// `Mount.cache(...)` / `Mount.secret(...)` / `Mount.ssh(...)` / `Mount.bind(...)`
    MountCtor {
        kind: String,
        args: Vec<Expr>,
    },
    Param {
        name: String,
        default: Option<Box<Expr>>,
    },
    BinaryAdd {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    If {
        cond: Box<Expr>,
        then_block: Block,
        else_block: Block,
    },
    For {
        var: String,
        iter: Box<Expr>,
        body: Block,
    },
    Block(Block),
}

#[derive(Debug, Clone)]
pub enum InterpPart {
    Lit(String),
    Ident(String),
}
