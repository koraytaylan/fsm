---
id: json-structural-parser
title: "Json Structural Parser"
workstream: "0002"
kind: task
depends_on:
  - json-value-and-scalars
gated: false
touches:
  - crates/fsm-core/src/json/parse.rs
  - crates/fsm-core/tests/json_corpus.rs
  - "crates/fsm-core/tests/fixtures/json/**"
status: planned
merged_as: ""
---
# Json Structural Parser

With string unescaping and number validation already landed as tested helpers, the parser itself is a plain recursive descent over object/array/scalar structure — pinned by a committed verdict corpus authored before the implementation.

**Steps:**

1. Author the fixture corpus first under `crates/fsm-core/tests/fixtures/json/`: ~40 `y_*.json` (must parse) and `n_*.json` (must reject) cases per architecture — depth at the limit and one past it, extreme number tokens preserved verbatim, duplicate keys, trailing garbage, BOM, raw control characters, plus structural surrogate cases routed through full documents — and `crates/fsm-core/tests/json_corpus.rs` asserting each filename's verdict.
2. Implement `JsonLimits`, `JsonError { kind, offset, message }`, and `parse(input, &JsonLimits) -> Result<Value, JsonError>` in `crates/fsm-core/src/json/parse.rs` following the dispatch skeleton in architecture: UTF-8 check, whitespace skipping, one top-level value with only whitespace after, a `parse_value` match on the first byte delegating strings to `unescape_string` (after a backslash-aware scan to the closing quote) and numbers to `check_number_token`, objects into `BTreeMap` with the duplicate-key error, arrays, literals, an explicit depth counter, and byte offsets in every error.
3. Add inline unit tests for each `JsonErrorKind` and for verbatim number-token capture.

- **Done when:** every corpus fixture's verdict holds under `cargo test -p fsm-core --test json_corpus`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
