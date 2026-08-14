---
id: lexer
title: "Lexer"
workstream: "0006"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-core/src/lib.rs
  - "crates/fsm-core/src/expr/**"
status: planned
merged_as: ""
---
# Lexer

The expression pipeline starts here: this task wires the `expr` module into the crate (so no later task touches `lib.rs` or `mod.rs` again) and lands the token set with byte-offset spans; there is no external source of truth for our own token set, so lexing is pinned by exhaustive inline unit tests rather than a fixtures directory.

**Steps:**

1. Add `pub mod expr;` to `crates/fsm-core/src/lib.rs` and create `crates/fsm-core/src/expr/mod.rs` declaring `pub mod ast; pub mod lexer; pub mod parser; pub mod typeck; pub mod eval; pub mod partial;` with empty stub files for each, plus the shared `ExprError { code, span, message, hint, details }` type in `mod.rs` per architecture.
2. Write the inline test module first (all tests live in `lexer.rs` — this task's touches carry no separate test file), encoding exactly the inventory under **Tests**.
3. Implement `Span`, `Tok`, and `lex(src) -> Result<Vec<(Tok, Span)>, ExprError>` in `crates/fsm-core/src/expr/lexer.rs`: reserved keywords `if then else and or not true false ctx evt`, lowercase identifiers, capitalized type identifiers, verbatim `Int`/`Dec` digit tokens, JSON-style string escapes, the operator/punctuation set, the 4,096-byte source cap (`expr/too_long`), and `expr/lex` with span for any unexpected byte.

**Tests:**

- Token coverage, inline in `lexer.rs`: every `Tok` variant produced at least once, asserted with its exact token content (`Int("123")`, `Dec("1.50")` verbatim digits).
- Keywords vs identifiers: `if` → `KwIf` but `iff`, `if_`, and `ifx` → `Ident`; `ctx`/`evt` reserved; `Risk` → `TypeIdent`; `half_even` and `ms` → ordinary `Ident` (contextual words are not reserved).
- Operator adjacency: `>=` lexes as one `Ge` while `> =` (with a space) lexes as `Gt` then an `expr/lex` on the bare `=`; `==` vs bare `=` → `expr/lex`; `!=` vs bare `!` → `expr/lex`.
- Rejected operators with teaching hints: `/` → `expr/lex` whose hint names `div(a, b, scale, mode)`; `%` → `expr/lex`.
- Numbers: `1.` and `.5` → `expr/lex` (a `Dec` needs at least one digit on each side of the dot); `1..2` → error at the second dot's span.
- Strings: each JSON-style escape decodes; an unterminated string → `expr/lex` at end-of-input span; an unknown escape (`\q`) → `expr/lex`.
- Span exactness: for a fixed multi-token source containing a multibyte UTF-8 character, every token's `[start, end)` byte offsets asserted literally.
- Limits and error shape: a 4,096-byte source lexes, 4,097 → `expr/too_long`; every produced `ExprError` has a non-empty `message` and `hint`.

- **Done when:** the lexer unit tests in `crates/fsm-core/src/expr/lexer.rs` cover every `Tok` variant and both lex error codes with exact spans, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
