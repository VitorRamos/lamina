# Lamina MVP grammar (0.1)

Normative EBNF lives in [`design.md`](design.md) Appendix A. This file is the checked-in grammar companion for the parser issue.

## Keywords

`arg`, `const`, `let`, `fn`, `pub`, `target`, `if`, `else`, `for`, `in`, `true`, `false`, `use`

Soft: `param` (call form), `Stage` / `Mount` (constructor forms).

## Items

```ebnf
Module       ::= { Item } ;
Item         ::= UseDecl | ArgDecl | ConstDecl | LetDecl | FnDecl | TargetDecl ;
UseDecl      ::= "use" StringLiteral ";" ;
ArgDecl      ::= "arg" StringLiteral [ "," StringLiteral ] ";" ;
ConstDecl    ::= "const" Ident ":" Type "=" Expr ";" ;
LetDecl      ::= "let" Ident [ ":" Type ] "=" Expr ";" ;
FnDecl       ::= [ "pub" ] "fn" Ident "(" [ ParamList ] ")" "->" Type Block ;
TargetDecl   ::= "pub" "target" Ident "=" Expr ";" ;
```

## Types

`String` | `Int` | `Bool` | `Stage` | `Mount` | `List[T]`

## Modules (0.2)

- `use "./path.lam";` — relative to the importing file; must stay under project root
- `use "std/name.lam";` — resolved via `LAMINA_STDLIB` or repo `stdlib/`
- Only `pub fn` is exported from a used module (transitive `use` inside modules supported)
- Absolute paths and remote URLs are not allowed in 0.2

## Notes

- Statements end with `;`; block tail expression may omit `;`.
- `for x in <List[T]> { U }` yields `List[U]`.
