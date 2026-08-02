//! Recursive-descent parser for frozen MVP grammar.

use crate::ast::*;
use crate::diag::{CompileError, DiagnosticMsg, Result};
use crate::lexer::{lex, StrPart, Token, TokenKind};
use crate::span::SourceFile;

pub fn parse(file: &SourceFile) -> Result<Module> {
    let tokens = lex(file)?;
    let mut p = Parser {
        _file: file,
        tokens,
        pos: 0,
    };
    p.parse_module()
        .map_err(|d| CompileError::single(Some(file), d))
}

struct Parser<'a> {
    _file: &'a SourceFile,
    tokens: Vec<Token>,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_at(&self, offset: usize) -> &Token {
        &self.tokens[(self.pos + offset).min(self.tokens.len() - 1)]
    }

    fn bump(&mut self) -> Token {
        let t = self.peek().clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t
    }

    /// `name = …` statement (not `let name =`).
    fn at_assign(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Ident(_))
            && matches!(self.peek_at(1).kind, TokenKind::Eq)
    }

    fn expect_punct(&mut self, kind: TokenKind) -> std::result::Result<Token, DiagnosticMsg> {
        let t = self.peek().clone();
        if t.kind == kind {
            Ok(self.bump())
        } else {
            Err(DiagnosticMsg::error(
                format!("expected {kind:?}, found {:?}", t.kind),
                Some(t.span),
            ))
        }
    }

    fn parse_module(&mut self) -> std::result::Result<Module, DiagnosticMsg> {
        let start = self.peek().span;
        let mut items = Vec::new();
        while !matches!(self.peek().kind, TokenKind::Eof) {
            items.push(self.parse_item()?);
        }
        let end = self.peek().span;
        Ok(Module {
            items,
            span: start.merge(end),
        })
    }

    fn parse_item(&mut self) -> std::result::Result<Item, DiagnosticMsg> {
        match &self.peek().kind {
            TokenKind::Use => Ok(Item::Use(self.parse_use()?)),
            TokenKind::Arg => Ok(Item::Arg(self.parse_arg()?)),
            TokenKind::Const => Ok(Item::Const(self.parse_const()?)),
            TokenKind::Let => Ok(Item::Let(self.parse_let()?)),
            TokenKind::Fn => Ok(Item::Fn(self.parse_fn(false)?)),
            TokenKind::Pub => {
                let start = self.bump().span;
                match self.peek().kind {
                    TokenKind::Target => Ok(Item::Target(self.parse_target_after_pub(start)?)),
                    TokenKind::Fn => Ok(Item::Fn(self.parse_fn(true)?)),
                    _ => Err(DiagnosticMsg::error(
                        "expected `target` or `fn` after `pub`",
                        Some(self.peek().span),
                    )),
                }
            }
            _ => Err(DiagnosticMsg::error(
                "expected item (use, arg, const, let, fn, pub fn, pub target)",
                Some(self.peek().span),
            )),
        }
    }

    fn parse_use(&mut self) -> std::result::Result<UseDecl, DiagnosticMsg> {
        let start = self.expect_punct(TokenKind::Use)?.span;
        let path_tok = self.bump();
        let path = match path_tok.kind {
            TokenKind::String(s) => s,
            _ => {
                return Err(DiagnosticMsg::error(
                    "use path must be a string literal",
                    Some(path_tok.span),
                ))
            }
        };
        let end = self.expect_punct(TokenKind::Semicolon)?.span;
        Ok(UseDecl {
            path,
            span: start.merge(end),
        })
    }

    fn parse_arg(&mut self) -> std::result::Result<ArgDecl, DiagnosticMsg> {
        let start = self.expect_punct(TokenKind::Arg)?.span;
        let name_tok = self.bump();
        let name = match name_tok.kind {
            TokenKind::String(s) => s,
            _ => {
                return Err(DiagnosticMsg::error(
                    "expected string build-arg name",
                    Some(name_tok.span),
                ))
            }
        };
        let default = if self.peek().kind == TokenKind::Comma {
            self.bump();
            let d = self.bump();
            match d.kind {
                TokenKind::String(s) => Some(s),
                _ => {
                    return Err(DiagnosticMsg::error(
                        "expected string default for arg",
                        Some(d.span),
                    ))
                }
            }
        } else {
            None
        };
        let end = self.expect_punct(TokenKind::Semicolon)?.span;
        Ok(ArgDecl {
            name,
            default,
            span: start.merge(end),
        })
    }

    fn parse_const(&mut self) -> std::result::Result<ConstDecl, DiagnosticMsg> {
        let start = self.expect_punct(TokenKind::Const)?.span;
        let name = self.expect_ident()?;
        self.expect_punct(TokenKind::Colon)?;
        let ty = self.parse_type()?;
        self.expect_punct(TokenKind::Eq)?;
        let value = self.parse_expr()?;
        let end = self.expect_punct(TokenKind::Semicolon)?.span;
        Ok(ConstDecl {
            name,
            ty,
            value,
            span: start.merge(end),
        })
    }

    fn parse_let(&mut self) -> std::result::Result<LetDecl, DiagnosticMsg> {
        let start = self.expect_punct(TokenKind::Let)?.span;
        let name = self.expect_ident()?;
        let ty = if self.peek().kind == TokenKind::Colon {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect_punct(TokenKind::Eq)?;
        let value = self.parse_expr()?;
        let end = self.expect_punct(TokenKind::Semicolon)?.span;
        Ok(LetDecl {
            name,
            ty,
            value,
            span: start.merge(end),
        })
    }

    fn parse_fn(&mut self, is_pub: bool) -> std::result::Result<FnDecl, DiagnosticMsg> {
        let start = self.expect_punct(TokenKind::Fn)?.span;
        let name = self.expect_ident()?;
        self.expect_punct(TokenKind::LParen)?;
        let mut params = Vec::new();
        if self.peek().kind != TokenKind::RParen {
            loop {
                params.push(self.parse_param()?);
                if self.peek().kind == TokenKind::Comma {
                    self.bump();
                    if self.peek().kind == TokenKind::RParen {
                        break;
                    }
                    continue;
                }
                break;
            }
        }
        self.expect_punct(TokenKind::RParen)?;
        self.expect_punct(TokenKind::Arrow)?;
        let ret = self.parse_type()?;
        let body = self.parse_block()?;
        Ok(FnDecl {
            is_pub,
            name,
            params,
            ret,
            body: body.clone(),
            span: start.merge(body.span),
        })
    }

    fn parse_param(&mut self) -> std::result::Result<Param, DiagnosticMsg> {
        let start = self.peek().span;
        let name = self.expect_ident()?;
        self.expect_punct(TokenKind::Colon)?;
        let ty = self.parse_type()?;
        Ok(Param {
            name,
            ty,
            span: start.merge(self.tokens[self.pos.saturating_sub(1)].span),
        })
    }

    fn parse_target_after_pub(
        &mut self,
        pub_span: crate::span::Span,
    ) -> std::result::Result<TargetDecl, DiagnosticMsg> {
        self.expect_punct(TokenKind::Target)?;
        let name = self.expect_ident()?;
        self.expect_punct(TokenKind::Eq)?;
        let value = self.parse_expr()?;
        let end = self.expect_punct(TokenKind::Semicolon)?.span;
        Ok(TargetDecl {
            name,
            value,
            span: pub_span.merge(end),
        })
    }

    fn parse_type(&mut self) -> std::result::Result<Type, DiagnosticMsg> {
        let t = self.bump();
        match t.kind {
            TokenKind::Ident(ref s) if s == "String" => Ok(Type::String),
            TokenKind::Ident(ref s) if s == "Int" => Ok(Type::Int),
            TokenKind::Ident(ref s) if s == "Bool" => Ok(Type::Bool),
            TokenKind::Ident(ref s) if s == "Stage" => Ok(Type::Stage),
            TokenKind::Ident(ref s) if s == "Mount" => Ok(Type::Mount),
            TokenKind::Ident(ref s) if s == "List" => {
                self.expect_punct(TokenKind::LBracket)?;
                let inner = self.parse_type()?;
                self.expect_punct(TokenKind::RBracket)?;
                Ok(Type::List(Box::new(inner)))
            }
            _ => Err(DiagnosticMsg::error("expected type", Some(t.span))),
        }
    }

    fn expect_ident(&mut self) -> std::result::Result<String, DiagnosticMsg> {
        let t = self.bump();
        match t.kind {
            TokenKind::Ident(s) => Ok(s),
            // soft keyword param can be used as ident in some positions
            _ => Err(DiagnosticMsg::error("expected identifier", Some(t.span))),
        }
    }

    fn parse_block(&mut self) -> std::result::Result<Block, DiagnosticMsg> {
        let start = self.expect_punct(TokenKind::LBrace)?.span;
        let mut stmts = Vec::new();
        let mut tail = None;
        while self.peek().kind != TokenKind::RBrace {
            if matches!(self.peek().kind, TokenKind::Eof) {
                return Err(DiagnosticMsg::error(
                    "unterminated block",
                    Some(self.peek().span),
                ));
            }
            if self.peek().kind == TokenKind::Let {
                stmts.push(BlockStmt::Let(self.parse_let()?));
                continue;
            }
            // `name = expr;` — reassignment (looks like Ident Eq …)
            if self.at_assign() {
                let name_tok = self.bump();
                let name = match name_tok.kind {
                    TokenKind::Ident(s) => s,
                    _ => unreachable!(),
                };
                self.expect_punct(TokenKind::Eq)?;
                let value = self.parse_expr()?;
                self.expect_punct(TokenKind::Semicolon)?;
                let span = name_tok.span.merge(value.span);
                stmts.push(BlockStmt::Assign { name, value, span });
                continue;
            }
            // Parse expression; if followed by `;` it's a stmt, else tail.
            let expr = self.parse_expr()?;
            if self.peek().kind == TokenKind::Semicolon {
                self.bump();
                stmts.push(BlockStmt::Expr(expr));
            } else if self.peek().kind == TokenKind::RBrace {
                tail = Some(Box::new(expr));
            } else if matches!(
                expr.kind,
                ExprKind::For { .. } | ExprKind::If { .. }
            ) {
                // `for` / `if` as statements may omit trailing `;` (like Rust).
                stmts.push(BlockStmt::Expr(expr));
            } else {
                return Err(DiagnosticMsg::error(
                    "expected `;` or end of block after expression",
                    Some(expr.span),
                ));
            }
        }
        let end = self.expect_punct(TokenKind::RBrace)?.span;
        Ok(Block {
            stmts,
            tail,
            span: start.merge(end),
        })
    }

    fn parse_expr(&mut self) -> std::result::Result<Expr, DiagnosticMsg> {
        match self.peek().kind {
            TokenKind::For => self.parse_for(),
            TokenKind::If => self.parse_if(),
            _ => self.parse_add(),
        }
    }

    fn parse_for(&mut self) -> std::result::Result<Expr, DiagnosticMsg> {
        let start = self.expect_punct(TokenKind::For)?.span;
        let var = self.expect_ident()?;
        self.expect_punct(TokenKind::In)?;
        let iter = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Expr {
            span: start.merge(body.span),
            kind: ExprKind::For {
                var,
                iter: Box::new(iter),
                body,
            },
        })
    }

    fn parse_if(&mut self) -> std::result::Result<Expr, DiagnosticMsg> {
        let start = self.expect_punct(TokenKind::If)?.span;
        let cond = self.parse_expr()?;
        let then_block = self.parse_block()?;
        self.expect_punct(TokenKind::Else)?;
        let else_block = self.parse_block()?;
        Ok(Expr {
            span: start.merge(else_block.span),
            kind: ExprKind::If {
                cond: Box::new(cond),
                then_block,
                else_block,
            },
        })
    }

    fn parse_add(&mut self) -> std::result::Result<Expr, DiagnosticMsg> {
        let mut left = self.parse_method()?;
        while self.peek().kind == TokenKind::Plus {
            self.bump();
            let right = self.parse_method()?;
            let span = left.span.merge(right.span);
            left = Expr {
                span,
                kind: ExprKind::BinaryAdd {
                    left: Box::new(left),
                    right: Box::new(right),
                },
            };
        }
        Ok(left)
    }

    fn parse_method(&mut self) -> std::result::Result<Expr, DiagnosticMsg> {
        let mut expr = self.parse_primary()?;
        while self.peek().kind == TokenKind::Dot {
            self.bump();
            // method name or Stage.from handled in primary mostly
            let name_tok = self.bump();
            // Method names may be keywords (e.g. Stage.arg) when they appear after `.`.
            let method = match &name_tok.kind {
                TokenKind::Ident(s) => s.clone(),
                TokenKind::Arg => "arg".into(),
                TokenKind::Const => "const".into(),
                TokenKind::Let => "let".into(),
                TokenKind::Fn => "fn".into(),
                TokenKind::Pub => "pub".into(),
                TokenKind::Target => "target".into(),
                TokenKind::If => "if".into(),
                TokenKind::Else => "else".into(),
                TokenKind::For => "for".into(),
                TokenKind::In => "in".into(),
                TokenKind::True => "true".into(),
                TokenKind::False => "false".into(),
                TokenKind::Use => "use".into(),
                _ => {
                    return Err(DiagnosticMsg::error(
                        "expected method name",
                        Some(name_tok.span),
                    ));
                }
            };
            // Special-case: already handled Stage.from in primary.
            // Here: .method(args)
            self.expect_punct(TokenKind::LParen)?;
            let args = self.parse_arg_list()?;
            self.expect_punct(TokenKind::RParen)?;
            let span = expr
                .span
                .merge(self.tokens[self.pos.saturating_sub(1)].span);
            expr = Expr {
                span,
                kind: ExprKind::Method {
                    recv: Box::new(expr),
                    method,
                    args,
                },
            };
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> std::result::Result<Expr, DiagnosticMsg> {
        let t = self.peek().clone();
        match t.kind {
            TokenKind::String(s) => {
                self.bump();
                Ok(Expr {
                    span: t.span,
                    kind: ExprKind::String(s),
                })
            }
            TokenKind::StringInterp(parts) => {
                self.bump();
                let parts = parts
                    .into_iter()
                    .map(|p| match p {
                        StrPart::Lit(s) => InterpPart::Lit(s),
                        StrPart::Ident(s) => InterpPart::Ident(s),
                    })
                    .collect();
                Ok(Expr {
                    span: t.span,
                    kind: ExprKind::StringInterp(parts),
                })
            }
            TokenKind::Int(n) => {
                self.bump();
                Ok(Expr {
                    span: t.span,
                    kind: ExprKind::Int(n),
                })
            }
            TokenKind::True => {
                self.bump();
                Ok(Expr {
                    span: t.span,
                    kind: ExprKind::Bool(true),
                })
            }
            TokenKind::False => {
                self.bump();
                Ok(Expr {
                    span: t.span,
                    kind: ExprKind::Bool(false),
                })
            }
            TokenKind::LBracket => {
                let start = self.bump().span;
                let mut els = Vec::new();
                if self.peek().kind != TokenKind::RBracket {
                    loop {
                        els.push(self.parse_expr()?);
                        if self.peek().kind == TokenKind::Comma {
                            self.bump();
                            if self.peek().kind == TokenKind::RBracket {
                                break;
                            }
                            continue;
                        }
                        break;
                    }
                }
                let end = self.expect_punct(TokenKind::RBracket)?.span;
                Ok(Expr {
                    span: start.merge(end),
                    kind: ExprKind::List(els),
                })
            }
            TokenKind::LBrace => {
                let b = self.parse_block()?;
                Ok(Expr {
                    span: b.span,
                    kind: ExprKind::Block(b),
                })
            }
            TokenKind::LParen => {
                self.bump();
                let e = self.parse_expr()?;
                self.expect_punct(TokenKind::RParen)?;
                Ok(e)
            }
            TokenKind::Ident(ref name) if name == "Stage" => {
                let start = self.bump().span;
                self.expect_punct(TokenKind::Dot)?;
                let m = self.expect_ident()?;
                self.expect_punct(TokenKind::LParen)?;
                match m.as_str() {
                    "from" => {
                        let image = self.parse_expr()?;
                        let end = self.expect_punct(TokenKind::RParen)?.span;
                        Ok(Expr {
                            span: start.merge(end),
                            kind: ExprKind::StageFrom {
                                image: Box::new(image),
                            },
                        })
                    }
                    "from_arg" => {
                        let n = self.bump();
                        let name = match n.kind {
                            TokenKind::String(s) => s,
                            _ => {
                                return Err(DiagnosticMsg::error(
                                    "from_arg expects string literal",
                                    Some(n.span),
                                ))
                            }
                        };
                        let end = self.expect_punct(TokenKind::RParen)?.span;
                        Ok(Expr {
                            span: start.merge(end),
                            kind: ExprKind::StageFromArg { name },
                        })
                    }
                    other => Err(DiagnosticMsg::error(
                        format!("unknown Stage constructor `{other}`"),
                        Some(start),
                    )),
                }
            }
            TokenKind::Ident(ref name) if name == "Mount" => {
                let start = self.bump().span;
                self.expect_punct(TokenKind::Dot)?;
                let kind = self.expect_ident()?;
                self.expect_punct(TokenKind::LParen)?;
                let args = self.parse_arg_list()?;
                let end = self.expect_punct(TokenKind::RParen)?.span;
                Ok(Expr {
                    span: start.merge(end),
                    kind: ExprKind::MountCtor { kind, args },
                })
            }
            TokenKind::Ident(ref name) if name == "param" => {
                let start = self.bump().span;
                self.expect_punct(TokenKind::LParen)?;
                let n = self.bump();
                let pname = match n.kind {
                    TokenKind::String(s) => s,
                    _ => {
                        return Err(DiagnosticMsg::error(
                            "param name must be string",
                            Some(n.span),
                        ))
                    }
                };
                let default = if self.peek().kind == TokenKind::Comma {
                    self.bump();
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };
                let end = self.expect_punct(TokenKind::RParen)?.span;
                Ok(Expr {
                    span: start.merge(end),
                    kind: ExprKind::Param {
                        name: pname,
                        default,
                    },
                })
            }
            TokenKind::Ident(name) => {
                let start = self.bump().span;
                if self.peek().kind == TokenKind::LParen {
                    self.bump();
                    let args = self.parse_arg_list()?;
                    let end = self.expect_punct(TokenKind::RParen)?.span;
                    Ok(Expr {
                        span: start.merge(end),
                        kind: ExprKind::Call { callee: name, args },
                    })
                } else {
                    Ok(Expr {
                        span: start,
                        kind: ExprKind::Ident(name),
                    })
                }
            }
            _ => Err(DiagnosticMsg::error(
                format!("unexpected token {:?}", t.kind),
                Some(t.span),
            )),
        }
    }

    fn parse_arg_list(&mut self) -> std::result::Result<Vec<Expr>, DiagnosticMsg> {
        let mut args = Vec::new();
        if self.peek().kind == TokenKind::RParen {
            return Ok(args);
        }
        loop {
            args.push(self.parse_expr()?);
            if self.peek().kind == TokenKind::Comma {
                self.bump();
                if self.peek().kind == TokenKind::RParen {
                    break;
                }
                continue;
            }
            break;
        }
        Ok(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::FileId;

    #[test]
    fn parse_target_stage() {
        let src = r#"
pub target app = Stage.from("alpine:3.19").arg("X").run("echo hi").name("app");
"#;
        let f = SourceFile::new(FileId(0), "t.lam", src);
        let m = parse(&f).expect("parse");
        assert_eq!(m.items.len(), 1);
    }

    #[test]
    fn parse_hello_shaped() {
        let src = r#"
pub target app = {
  let builder = Stage.from("golang:1.22-bookworm")
    .workdir("/src")
    .run("echo build")
    .name("builder");
  Stage.from("alpine:3.19")
    .copy_from(builder, "/out/app", "/app")
    .entrypoint(["/app"])
    .name("app")
};
"#;
        let f = SourceFile::new(FileId(0), "t.lam", src);
        parse(&f).expect("parse hello-shaped");
    }
}
