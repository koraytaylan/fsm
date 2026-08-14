---
id: json-value-and-parser
title: "Json Value And Parser"
workstream: "0002"
kind: task
depends_on:
  - workspace-scaffold
gated: false
touches:
  - crates/fsm-core/src/json/value.rs
  - crates/fsm-core/src/json/parse.rs
  - crates/fsm-core/tests/json_corpus.rs
  - "crates/fsm-core/tests/fixtures/json/**"
status: planned
merged_as: ""
---
# Json Value And Parser

Every byte entering the system flows through this hand-rolled parser, and no float may ever exist: number tokens are captured as raw strings, duplicate keys and lone surrogates are rejected, and limits are enforced — pinned by a committed verdict corpus authored before the implementation.

**Steps:**

1. Author the fixture corpus first under `crates/fsm-core/tests/fixtures/json/`: ~40 `y_*.json` (must parse) and `n_*.json` (must reject) cases per architecture — depth at limit and past it, extreme number tokens preserved verbatim, lone/unpaired surrogates, duplicate keys, trailing garbage, raw control characters, BOM — and `crates/fsm-core/tests/json_corpus.rs` asserting each filename's verdict.
2. Implement `Value` (with `Num(String)` and `BTreeMap` objects) and accessors in `crates/fsm-core/src/json/value.rs`.
3. Implement `parse(input, &JsonLimits) -> Result<Value, JsonError>` in `crates/fsm-core/src/json/parse.rs` as recursive descent with an explicit depth counter, per-rule `JsonErrorKind`s, and byte offsets in every error, exactly per architecture.
4. Add inline unit tests for each error kind and for verbatim number-token capture.

- **Done when:** every corpus fixture's verdict holds under `cargo test -p fsm-core --test json_corpus`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
