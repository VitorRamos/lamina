# Lamina MVP grammar (0.1)

Normative EBNF lives in [`design.md`](design.md) Appendix A. This file is the checked-in grammar companion for the parser issue.

## Keywords

`arg`, `const`, `let`, `fn`, `pub`, `target`, `if`, `else`, `for`, `in`, `true`, `false`, `use`

Soft: `param` (call form), `Stage` / `Mount` (constructor forms).

## Assignment

Blocks may reassign an existing binding:

```lam
name = expr;
```

Use this to accumulate a `Stage` across a `for` loop (outer `let` first):

```lam
let s = Stage.from("alpine:3.19");
for pkg in ["curl", "jq"] {
  s = s.run("apk add --no-cache " + pkg);
}
s
```

Notes:

- The name must already be bound (`let` / `const` / param in scope).
- Type must match the previous binding.
- In a `for` body, the loop shares the outer environment so reassignments persist across iterations.
- A `for` / `if` used as a statement may omit the trailing `;`.
- Prefer `"echo " + pkg` or `"echo ${pkg}"` for string building — shell `$pkg` is **not** Lamina interpolation.

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

## Modules (0.2 path + 1.1 git remotes)

- `use "./path.lam";` — relative to the importing file; must stay under project root
- `use "std/name.lam";` — resolved via `LAMINA_STDLIB` or repo `stdlib/`
- `use "git+https://host/repo.git?ref=TAG&path=file.lam";` — git remote (also `git+ssh`, `git+file`)
- Only `pub fn` is exported from a used module (transitive `use` inside modules supported)
- Absolute local paths are not allowed; bare `http://` remotes are rejected

## Notes

- Statements end with `;`; block tail expression may omit `;`.
- `for x in <List[T]> { U }` yields `List[U]`.
