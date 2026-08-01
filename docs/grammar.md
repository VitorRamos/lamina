# Lamina MVP grammar (0.1)

Normative EBNF lives in [`design.md`](design.md) Appendix A. This file is the checked-in grammar companion for the parser issue.

## Keywords

`arg`, `const`, `let`, `fn`, `pub`, `target`, `if`, `else`, `for`, `in`, `true`, `false`

Soft: `param` (call form), `Stage` (constructor form).

## Items

```ebnf
Module       ::= { Item } ;
Item         ::= ArgDecl | ConstDecl | LetDecl | FnDecl | TargetDecl ;
ArgDecl      ::= "arg" StringLiteral [ "," StringLiteral ] ";" ;
ConstDecl    ::= "const" Ident ":" Type "=" Expr ";" ;
LetDecl      ::= "let" Ident [ ":" Type ] "=" Expr ";" ;
FnDecl       ::= "fn" Ident "(" [ ParamList ] ")" "->" Type Block ;
TargetDecl   ::= "pub" "target" Ident "=" Expr ";" ;
```

## Types

`String` | `Int` | `Bool` | `Stage` | `List[T]`

## Notes

- Statements end with `;`; block tail expression may omit `;`.
- `for x in <List[T]> { U }` yields `List[U]`.
- No `import` in 0.1.
