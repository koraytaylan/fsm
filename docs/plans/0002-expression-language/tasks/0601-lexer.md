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
2. Implement `Span`, `Tok`, and `lex(src) -> Result<Vec<(Tok, Span)>, ExprError>` in `crates/fsm-core/src/expr/lexer.rs`: reserved keywords `if then else and or not true false ctx evt`, lowercase identifiers, capitalized type identifiers, verbatim `Int`/`Dec` digit tokens, JSON-style string escapes, the operator/punctuation set, the 4,096-byte source cap (`expr/too_long`), and `expr/lex` with span for any unexpected byte.
3. Add inline unit tests covering every token kind, operator adjacency (`>=` vs `>` `=`), keyword-versus-identifier boundaries, escape handling, span exactness, and both error codes.

- **Done when:** the lexer unit tests in `crates/fsm-core/src/expr/lexer.rs` cover every `Tok` variant and both lex error codes with exact spans, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
