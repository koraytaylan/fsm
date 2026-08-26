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
status: done
merged_as: ""
---
# Parser

The versioned `expr/1` grammar lands as a recursive-descent parser over the token stream — lazy `if/then/else`, non-chaining comparisons with a fix-it hint, no division token at all — pinned by a parse-golden corpus authored before the implementation.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/expr/parse.jsonl` and `crates/fsm-core/tests/expr_golden.rs` first, encoding exactly the inventory under **Tests** (lines pair a source with either the expected `render_ast` form or an expected error code and span).
2. Implement the spanned AST, `Arg::{Expr, Word}`, `render_ast`, `node_count`, and `depth` in `crates/fsm-core/src/expr/ast.rs` per architecture.
3. Implement `parse(src) -> Result<Expr, ExprError>` in `crates/fsm-core/src/expr/parser.rs` following the EBNF exactly, with the 512-node and depth-32 limits and expected-token-set errors.

**Tests:**

- Precedence pins in `parse.jsonl`, asserted as exact `render_ast` strings: `ctx.a + ctx.b * ctx.c` (mul binds tighter), `-ctx.a * ctx.b` (unary minus binds tighter than `*`), `ctx.a - ctx.b - ctx.c` (left-associative), `not ctx.a and ctx.b` (`not` binds tighter than `and`), `ctx.a or ctx.b and ctx.c` (`and` tighter than `or`), a parenthesized override of each.
- `if` shape: `if ctx.a then 1 else if ctx.b then 2 else 3` renders right-nested; an `if` inside a `then` branch requires no parentheses per the grammar.
- Reference and literal forms: `ctx.a == Risk.low` (enum literal), string/bool/int/dec literals, `round(ctx.r, 2, half_even)` producing a `Word` argument, `min(ctx.a, ctx.b)` with expression arguments, empty-argument call `f()`.
- Error lines, code and span asserted: `ctx.a < ctx.b < ctx.c` → `expr/chained_cmp` at the second operator with hint exactly ``use `and` to combine comparisons``; a 20-digit integer → `expr/int_range`; a 39-digit decimal and a 13-fraction-digit decimal → `expr/dec_range`; a 513-node expression → `expr/too_long`; 33 nested parentheses → `expr/too_deep`; `a / b` surfacing the lexer hint naming `div(a, b, scale, mode)`.
- Expected-token-set contents asserted (not just the code) for two distinct positions: `ctx.` at end of input (hint lists an identifier) and `(ctx.a` at end of input (hint lists `)` among the expected set); trailing tokens after a complete expression → `expr/parse`.
- `expr_golden.rs` mechanics: every line asserted; a malformed fixture line fails the run rather than being skipped.
- Inline in `ast.rs`: `node_count` and `depth` pinned on two hand-counted expressions; `render_ast` is span-free (two sources differing only in whitespace render identically).

- **Done when:** every line of `parse.jsonl` holds under `cargo test -p fsm-core --test expr_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
