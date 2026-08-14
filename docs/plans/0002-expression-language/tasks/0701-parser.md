---
id: parser
title: "Parser"
workstream: "0007"
kind: task
depends_on:
  - lexer
gated: false
touches:
  - crates/fsm-core/src/expr/ast.rs
  - crates/fsm-core/src/expr/parser.rs
  - crates/fsm-core/tests/expr_golden.rs
  - crates/fsm-core/tests/fixtures/expr/parse.jsonl
status: planned
merged_as: ""
---
# Parser

The versioned `expr/1` grammar lands as a recursive-descent parser over the token stream — lazy `if/then/else`, non-chaining comparisons with a fix-it hint, no division token at all — pinned by a parse-golden corpus authored before the implementation.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/expr/parse.jsonl` first: lines pairing source with either the expected `render_ast` form or an expected error code and span, covering precedence and associativity, lazy-`if` shape, every parse error code (`expr/parse` with expected-token hints, `expr/chained_cmp`, `expr/int_range`, `expr/dec_range`, `expr/too_long`, `expr/too_deep`), and the lexer-level hint for `/` naming `div(a, b, scale, mode)`; plus `crates/fsm-core/tests/expr_golden.rs` asserting every line.
2. Implement the spanned AST, `Arg::{Expr, Word}`, `render_ast`, `node_count`, and `depth` in `crates/fsm-core/src/expr/ast.rs` per architecture.
3. Implement `parse(src) -> Result<Expr, ExprError>` in `crates/fsm-core/src/expr/parser.rs` following the EBNF exactly, with the 512-node and depth-32 limits and expected-token-set errors.

- **Done when:** every line of `parse.jsonl` holds under `cargo test -p fsm-core --test expr_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
