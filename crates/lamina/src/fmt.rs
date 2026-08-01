//! Pretty-printer for Lamina sources (`lamina fmt`).

use crate::ast::*;
use crate::diag::Result;
use crate::parser::parse;
use crate::span::{FileId, SourceFile};

/// Format `src`; returns reformatted text.
pub fn format_source(name: &str, src: &str) -> Result<String> {
    let file = SourceFile::new(FileId(0), name, src);
    let module = parse(&file)?;
    Ok(format_module(&module))
}

pub fn format_module(module: &Module) -> String {
    let mut out = String::new();
    for (i, item) in module.items.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        format_item(&mut out, item);
        out.push('\n');
    }
    out
}

fn format_item(out: &mut String, item: &Item) {
    match item {
        Item::Use(u) => {
            out.push_str("use ");
            write_string(out, &u.path);
            out.push(';');
        }
        Item::Arg(a) => {
            out.push_str("arg ");
            write_string(out, &a.name);
            if let Some(d) = &a.default {
                out.push_str(", ");
                write_string(out, d);
            }
            out.push(';');
        }
        Item::Const(c) => {
            out.push_str("const ");
            out.push_str(&c.name);
            out.push_str(": ");
            out.push_str(&c.ty.as_str());
            out.push_str(" = ");
            format_expr(out, &c.value, 0);
            out.push(';');
        }
        Item::Let(l) => {
            out.push_str("let ");
            out.push_str(&l.name);
            if let Some(t) = &l.ty {
                out.push_str(": ");
                out.push_str(&t.as_str());
            }
            out.push_str(" = ");
            format_expr(out, &l.value, 0);
            out.push(';');
        }
        Item::Fn(f) => {
            if f.is_pub {
                out.push_str("pub ");
            }
            out.push_str("fn ");
            out.push_str(&f.name);
            out.push('(');
            for (i, p) in f.params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&p.name);
                out.push_str(": ");
                out.push_str(&p.ty.as_str());
            }
            out.push_str(") -> ");
            out.push_str(&f.ret.as_str());
            out.push(' ');
            format_block(out, &f.body, 0);
        }
        Item::Target(t) => {
            out.push_str("pub target ");
            out.push_str(&t.name);
            out.push_str(" = ");
            format_expr(out, &t.value, 0);
            out.push(';');
        }
    }
}

fn format_block(out: &mut String, block: &Block, indent: usize) {
    out.push_str("{\n");
    let ind = "  ".repeat(indent + 1);
    for stmt in &block.stmts {
        out.push_str(&ind);
        match stmt {
            BlockStmt::Let(l) => {
                out.push_str("let ");
                out.push_str(&l.name);
                if let Some(t) = &l.ty {
                    out.push_str(": ");
                    out.push_str(&t.as_str());
                }
                out.push_str(" = ");
                format_expr(out, &l.value, indent + 1);
                out.push(';');
            }
            BlockStmt::Expr(e) => {
                format_expr(out, e, indent + 1);
                out.push(';');
            }
        }
        out.push('\n');
    }
    if let Some(tail) = &block.tail {
        out.push_str(&ind);
        format_expr(out, tail, indent + 1);
        out.push('\n');
    }
    out.push_str(&"  ".repeat(indent));
    out.push('}');
}

fn format_expr(out: &mut String, expr: &Expr, indent: usize) {
    match &expr.kind {
        ExprKind::String(s) => write_string(out, s),
        ExprKind::StringInterp(parts) => {
            out.push('"');
            for p in parts {
                match p {
                    InterpPart::Lit(s) => push_escaped(out, s),
                    InterpPart::Ident(n) => {
                        out.push_str("${");
                        out.push_str(n);
                        out.push('}');
                    }
                }
            }
            out.push('"');
        }
        ExprKind::Int(n) => out.push_str(&n.to_string()),
        ExprKind::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        ExprKind::Ident(n) => out.push_str(n),
        ExprKind::List(els) => {
            out.push('[');
            for (i, e) in els.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_expr(out, e, indent);
            }
            out.push(']');
        }
        ExprKind::Call { callee, args } => {
            out.push_str(callee);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_expr(out, a, indent);
            }
            out.push(')');
        }
        ExprKind::Method { recv, method, args } => {
            format_expr(out, recv, indent);
            out.push('.');
            out.push_str(method);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_expr(out, a, indent);
            }
            out.push(')');
        }
        ExprKind::StageFrom { image } => {
            out.push_str("Stage.from(");
            format_expr(out, image, indent);
            out.push(')');
        }
        ExprKind::StageFromArg { name } => {
            out.push_str("Stage.from_arg(");
            write_string(out, name);
            out.push(')');
        }
        ExprKind::MountCtor { kind, args } => {
            out.push_str("Mount.");
            out.push_str(kind);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_expr(out, a, indent);
            }
            out.push(')');
        }
        ExprKind::Param { name, default } => {
            out.push_str("param(");
            write_string(out, name);
            if let Some(d) = default {
                out.push_str(", ");
                format_expr(out, d, indent);
            }
            out.push(')');
        }
        ExprKind::BinaryAdd { left, right } => {
            format_expr(out, left, indent);
            out.push_str(" + ");
            format_expr(out, right, indent);
        }
        ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            out.push_str("if ");
            format_expr(out, cond, indent);
            out.push(' ');
            format_block(out, then_block, indent);
            out.push_str(" else ");
            format_block(out, else_block, indent);
        }
        ExprKind::For { var, iter, body } => {
            out.push_str("for ");
            out.push_str(var);
            out.push_str(" in ");
            format_expr(out, iter, indent);
            out.push(' ');
            format_block(out, body, indent);
        }
        ExprKind::Block(b) => format_block(out, b, indent),
    }
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    push_escaped(out, s);
    out.push('"');
}

fn push_escaped(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_target() {
        let src = r#"pub target app=Stage.from("alpine:3.19").run("true").name("app");"#;
        let out = format_source("t.lam", src).unwrap();
        assert!(out.contains("pub target app = "));
        assert!(out.contains("Stage.from("));
    }
}
