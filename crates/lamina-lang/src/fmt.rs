//! Pretty-printer for Lamina sources (`lamina fmt`).
//!
//! Style (matches hand-written examples like hello-static / kitchen-sink):
//! - Multi-method chains break one `.method()` per line; a single call stays inline
//!   (`s.run("…")`, not `s` + newline + `.run("…")`).
//! - Dense runs of the same item kind (`use`, `const`, `let`, …).
//! - Blank line between different kinds / before `fn` and `target`.
//! - Leading `//` comments preserved; file header gets a blank after comments,
//!   section comments sit directly above the following item.

use crate::ast::*;
use crate::diag::Result;
use crate::parser::parse;
use crate::span::{FileId, SourceFile};

/// Format `src`; returns reformatted text.
pub fn format_source(name: &str, src: &str) -> Result<String> {
    let file = SourceFile::new(FileId(0), name, src);
    let module = parse(&file)?;
    Ok(format_module_preserving_comments(&module, src))
}

pub fn format_module(module: &Module) -> String {
    format_module_preserving_comments(module, "")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    Use,
    Arg,
    Const,
    Let,
    Fn,
    Target,
}

fn item_kind(item: &Item) -> ItemKind {
    match item {
        Item::Use(_) => ItemKind::Use,
        Item::Arg(_) => ItemKind::Arg,
        Item::Const(_) => ItemKind::Const,
        Item::Let(_) => ItemKind::Let,
        Item::Fn(_) => ItemKind::Fn,
        Item::Target(_) => ItemKind::Target,
    }
}

fn format_module_preserving_comments(module: &Module, src: &str) -> String {
    let mut out = String::new();
    let mut prev_end: usize = 0;
    let mut prev_kind: Option<ItemKind> = None;
    for (i, item) in module.items.iter().enumerate() {
        let kind = item_kind(item);
        let item_start = item_span(item).start as usize;
        let trivia = if !src.is_empty() && item_start >= prev_end {
            leading_line_comments(src, prev_end, item_start)
        } else {
            String::new()
        };
        let has_comments = !trivia.is_empty();

        if i > 0 {
            // Section break: different kind, or a comment introduces a new section.
            let section_break = has_comments || prev_kind != Some(kind);
            if section_break {
                out.push('\n'); // extra blank (previous item already ended with \n)
            }
        }

        if has_comments {
            out.push_str(&trivia);
            if !trivia.ends_with('\n') {
                out.push('\n');
            }
            // File header: blank line after the banner comments before the first item.
            if i == 0 {
                out.push('\n');
            }
        }

        format_item(&mut out, item);
        out.push('\n');
        prev_end = item_span(item).end as usize;
        prev_kind = Some(kind);
    }
    // Trailing file comments after last item
    if !src.is_empty() && prev_end < src.len() {
        let trail = leading_line_comments(src, prev_end, src.len());
        if !trail.is_empty() {
            out.push('\n');
            out.push_str(&trail);
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
    }
    out
}

fn item_span(item: &Item) -> crate::span::Span {
    match item {
        Item::Use(u) => u.span,
        Item::Arg(a) => a.span,
        Item::Const(c) => c.span,
        Item::Let(l) => l.span,
        Item::Fn(f) => f.span,
        Item::Target(t) => t.span,
    }
}

/// Collect full-line `//` comments in `src[from..to]` (byte range).
fn leading_line_comments(src: &str, from: usize, to: usize) -> String {
    let to = to.min(src.len());
    let from = from.min(to);
    let region = &src[from..to];
    let mut out = String::new();
    for line in region.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("//") {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(trimmed);
        }
        // Non-comment code in the gap is ignored (parser already owns it).
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
    for (i, stmt) in block.stmts.iter().enumerate() {
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
            BlockStmt::Assign { name, value, .. } => {
                out.push_str(name);
                out.push_str(" = ");
                format_expr(out, value, indent + 1);
                out.push(';');
            }
            BlockStmt::Expr(e) => {
                format_expr(out, e, indent + 1);
                out.push(';');
            }
        }
        out.push('\n');
        // Dense runs of let/assign; blank before a free expression or after last stmt before tail.
        if let Some(next) = block.stmts.get(i + 1) {
            if block_needs_blank(stmt, next) {
                out.push('\n');
            }
        } else if block.tail.is_some() {
            out.push('\n');
        }
    }
    if let Some(tail) = &block.tail {
        out.push_str(&ind);
        format_expr(out, tail, indent + 1);
        out.push('\n');
    }
    out.push_str(&"  ".repeat(indent));
    out.push('}');
}

fn is_binding_stmt(s: &BlockStmt) -> bool {
    matches!(s, BlockStmt::Let(_) | BlockStmt::Assign { .. })
}

fn block_needs_blank(prev: &BlockStmt, next: &BlockStmt) -> bool {
    // Keep consecutive `let` / `name = …` dense (Stage accumulation).
    // Blank before free expressions (or between expr stmts).
    !(is_binding_stmt(prev) && is_binding_stmt(next))
}

fn format_expr(out: &mut String, expr: &Expr, indent: usize) {
    // Method chains: peel and print one call per line.
    if matches!(expr.kind, ExprKind::Method { .. }) {
        format_method_chain(out, expr, indent);
        return;
    }
    format_expr_atom(out, expr, indent);
}

/// Print `recv.m1(...).m2(...)`.
///
/// - **One** method: keep on one line (`s.run("x")`, `Stage.from("a").run("b")`).
/// - **Two or more**: break each `.method()` onto its own indented line.
fn format_method_chain(out: &mut String, expr: &Expr, indent: usize) {
    let mut methods: Vec<(&str, &[Expr])> = Vec::new();
    let mut cur = expr;
    while let ExprKind::Method { recv, method, args } = &cur.kind {
        methods.push((method.as_str(), args.as_slice()));
        cur = recv;
    }
    methods.reverse();

    format_expr_atom(out, cur, indent);
    let multiline = methods.len() > 1;
    let cont = "  ".repeat(indent + 1);
    for (method, args) in methods {
        if multiline {
            out.push('\n');
            out.push_str(&cont);
        }
        out.push('.');
        out.push_str(method);
        out.push('(');
        for (i, a) in args.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            format_expr(out, a, if multiline { indent + 1 } else { indent });
        }
        out.push(')');
    }
}

fn format_expr_atom(out: &mut String, expr: &Expr, indent: usize) {
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
        ExprKind::Method { .. } => {
            // Handled by format_expr → format_method_chain.
            format_method_chain(out, expr, indent);
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
    fn fmt_target_breaks_method_chain() {
        let src = r#"pub target app=Stage.from("alpine:3.19").run("true").name("app");"#;
        let out = format_source("t.lam", src).unwrap();
        assert!(out.contains("pub target app = "));
        assert!(
            out.contains("Stage.from(\"alpine:3.19\")\n  .run(\"true\")\n  .name(\"app\")"),
            "expected broken multi-method chain, got:\n{out}"
        );
    }

    #[test]
    fn fmt_single_method_stays_inline() {
        let src = r#"
pub target app = {
  let s = Stage.from("alpine:3.19");
  s = s.run("cargo install --locked trunk");
  s.name("app")
};
"#;
        let out = format_source("t.lam", src).unwrap();
        assert!(
            out.contains("s = s.run(\"cargo install --locked trunk\");"),
            "single method after assign should stay inline:\n{out}"
        );
        assert!(
            !out.contains("s = s\n"),
            "should not break before sole .run:\n{out}"
        );
        // Stage.from(...).name(...) is one method on a StageFrom atom → inline
        assert!(
            out.contains("s.name(\"app\")"),
            "single method on ident stays inline:\n{out}"
        );
    }

    #[test]
    fn fmt_dense_consecutive_assigns() {
        let src = r#"
pub target app = {
  let s = Stage.from("alpine:3.19");
  s = s.run("one");

  s = s.run("two");

  s = s.run("three");
  s.name("app")
};
"#;
        let out = format_source("t.lam", src).unwrap();
        assert!(
            out.contains(
                "let s = Stage.from(\"alpine:3.19\");\n  s = s.run(\"one\");\n  s = s.run(\"two\");\n  s = s.run(\"three\");\n\n  s.name(\"app\")"
            ),
            "let+assigns should be dense; blank only before tail expr:\n{out}"
        );
    }

    #[test]
    fn fmt_preserves_leading_comments_and_block_style() {
        let src = r#"// Multi-stage hello image (MVP dogfood).
// Builder writes a tiny script.

pub target app = {
  let builder = Stage.from("alpine:3.19")
    .workdir("/src")
    .run("true")
    .name("builder");

  Stage.from("alpine:3.19")
    .copy_from(builder, "/out/app", "/app")
    .name("app")
};
"#;
        let out = format_source("t.lam", src).unwrap();
        assert!(
            out.starts_with("// Multi-stage hello image"),
            "lost header comments:\n{out}"
        );
        assert!(out.contains("// Builder writes a tiny script."));
        assert!(
            out.contains(".workdir(\"/src\")"),
            "chain not broken:\n{out}"
        );
        assert!(
            out.contains("let builder = Stage.from(\"alpine:3.19\")\n"),
            "unexpected let/chain layout:\n{out}"
        );
        // blank line between let and tail expression
        assert!(
            out.contains(";\n\n  Stage.from"),
            "expected blank line between stmts:\n{out}"
        );
    }

    #[test]
    fn fmt_dense_const_and_use_groups() {
        let src = r#"// banner
// line 2

use "./a.lam";
use "std/golang.lam";

// constants
const A: String = "a";
const B: Int = 1;
const C: Bool = true;

// lets
let x = param("k", "v");
let y = A + "z";

fn helper(s: Stage) -> Stage {
  s.run("true")
}

pub target app = Stage.from("alpine:3.19").name("app");
"#;
        let out = format_source("t.lam", src).unwrap();
        // uses packed
        assert!(
            out.contains("use \"./a.lam\";\nuse \"std/golang.lam\";\n"),
            "uses should be dense:\n{out}"
        );
        // consts packed (section comment then dense consts)
        assert!(
            out.contains("// constants\nconst A: String = \"a\";\nconst B: Int = 1;\nconst C: Bool = true;\n"),
            "consts should be dense under section comment:\n{out}"
        );
        // blank before fn / target
        assert!(out.contains(";\n\nfn helper"), "blank before fn:\n{out}");
        assert!(
            out.contains("}\n\npub target app"),
            "blank before target:\n{out}"
        );
        // file header blank after comments
        assert!(
            out.starts_with("// banner\n// line 2\n\nuse "),
            "header blank after banner:\n{out}"
        );
        // idempotent
        let out2 = format_source("t.lam", &out).unwrap();
        assert_eq!(out, out2, "not idempotent:\n{out}");
    }

    #[test]
    fn fmt_hello_static_stable() {
        let src = include_str!("../../../examples/hello-static/src/image.lam");
        let out = format_source("image.lam", src).unwrap();
        let out2 = format_source("image.lam", &out).unwrap();
        assert_eq!(out, out2, "fmt not idempotent:\n{out}");
        // stays close to hand style: no double-spaced noise
        assert!(!out.contains("\n\n\n"), "too many blank lines:\n{out}");
    }
}
