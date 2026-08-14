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

1. Author the scalar vectors first under `crates/fsm-core/tests/fixtures/json-scalars/`: `strings.txt` (escaped-input → expected-decoded or `ERR` lines: every simple escape, a correct surrogate pair, a high surrogate followed by a non-escape, a high surrogate followed by a low-range miss, a bare low surrogate, a truncated `\u12`, a raw control byte) and `numbers.txt` (token → `OK`/`ERR`: integers, fractions, exponents, `-0`, leading-zero violations, bare `.5`, `1.`, `+1`, hex, `NaN`), plus `crates/fsm-core/tests/json_scalars.rs` asserting every line.
2. Implement `Value` (with `Num(String)` and `BTreeMap` objects) and accessors in `crates/fsm-core/src/json/value.rs`.
3. Implement in `crates/fsm-core/src/json/parse.rs` the two helpers exactly per the architecture signatures and algorithms: `pub(crate) fn unescape_string(raw: &str) -> Result<String, ScalarError>` (input is the escaped contents between the quotes; the surrogate combination formula is given verbatim in architecture) and `pub(crate) fn check_number_token(tok: &str) -> bool` (the RFC 8259 grammar as the four-phase scan given in architecture).
4. Add inline unit tests for `Value` accessors.

- **Done when:** every line of both scalar vector files passes under `cargo test -p fsm-core --test json_scalars`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
