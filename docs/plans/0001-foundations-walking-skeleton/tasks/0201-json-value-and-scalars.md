---
id: json-value-and-scalars
title: "Json Value And Scalars"
workstream: "0002"
kind: task
depends_on:
  - workspace-scaffold
gated: false
touches:
  - crates/fsm-core/src/json/value.rs
  - crates/fsm-core/src/json/parse.rs
  - crates/fsm-core/tests/json_scalars.rs
  - "crates/fsm-core/tests/fixtures/json-scalars/**"
status: planned
merged_as: ""
---
# Json Value And Scalars

The two fiddly parts of JSON — string unescaping (surrogate pairs included) and number-token validation — land first as standalone pure functions with their own verdict vectors, so the structural parser that follows is a plain recursive descent over already-solved scalars.

**Steps:**

1. Author the fixture files `crates/fsm-core/tests/fixtures/json-scalars/{strings.txt, numbers.txt}` and `crates/fsm-core/tests/json_scalars.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `Value` (with `Num(String)` and `BTreeMap` objects) and accessors in `crates/fsm-core/src/json/value.rs`.
3. Implement in `crates/fsm-core/src/json/parse.rs` the two helpers exactly per the architecture signatures and algorithms: `pub(crate) fn unescape_string(raw: &str) -> Result<String, ScalarError>` (input is the escaped contents between the quotes; the surrogate combination formula is given verbatim in architecture) and `pub(crate) fn check_number_token(tok: &str) -> bool` (the RFC 8259 grammar as the four-phase scan given in architecture).

**Tests:**

- `strings.txt` verdict lines (escaped-input → decoded-output or `ERR`), asserted line by line by `json_scalars.rs`: each of the eight simple escapes; the six-character backslash-u sequence for `A` → raw `A`; a correct surrogate pair (`😀` → the emoji); a high surrogate followed by a plain character → `ERR`; a high surrogate followed by a `\u` escape outside the low range → `ERR`; a bare low surrogate → `ERR`; truncated `\u12` → `ERR`; an unknown escape (`\q`) → `ERR`; a raw control byte (0x01) in the input → `ERR`.
- `numbers.txt` verdict lines (token → `OK`/`ERR`): accepted — `0`, `-0`, `1`, `123`, `1.5`, `0.10`, `1e5`, `1E+5`, `1.5e-3`, a 40-digit integer token; rejected — `01`, `+1`, `.5`, `1.`, `1.e5`, `1e`, `0x10`, `NaN`, `Infinity`, `--1`, empty string.
- `value.rs` inline unit tests: `get` on nested `Obj`; each `as_*` accessor returning `None` on a mismatched variant; `Eq` between structurally equal values.

- **Done when:** every line of both scalar vector files passes under `cargo test -p fsm-core --test json_scalars`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
