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

1. Author the corpus under `crates/fsm-core/tests/fixtures/json/` (`y_*.json` must parse, `n_*.json` must reject) and `crates/fsm-core/tests/json_corpus.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `JsonLimits`, `JsonError { kind, offset, message }`, and `parse(input, &JsonLimits) -> Result<Value, JsonError>` in `crates/fsm-core/src/json/parse.rs` following the dispatch skeleton in architecture: UTF-8 check, whitespace skipping, a `parse_value` match on the first byte delegating strings to `unescape_string` (after a backslash-aware scan to the closing quote) and numbers to `check_number_token`, objects into `BTreeMap` with the duplicate-key error, arrays, literals, an explicit depth counter, and byte offsets in every error.

**Tests:**

- Corpus `y_*` families (each parses, and where noted, content is asserted): nesting at exactly depth 64; an extreme number token (`1e309`) whose `Value::Num` string equals the input token byte-for-byte; a 40-digit integer token preserved verbatim; empty object/array/string; all whitespace forms between tokens; a full document containing a correct surrogate-pair escape; unicode keys sorted into `BTreeMap`.
- Corpus `n_*` families (each rejects, with the expected `JsonErrorKind` encoded in the filename): depth 65; duplicate object keys; trailing garbage after the top-level value; a UTF-8 BOM prefix; a raw control character inside a string; a lone-surrogate escape at document level; truncated documents (`{`, `[1,`, `"abc`); invalid literals (`ture`, `nul`); a bare top-level number with garbage suffix.
- `json_corpus.rs` mechanics: iterates the fixture directory, asserts every `y_*` parses and every `n_*` fails, and fails the run if the directory contains a file matching neither prefix (no silently skipped fixtures).
- Inline unit tests in `parse.rs`: each `JsonErrorKind` constructed at least once with its byte `offset` asserted exactly; input over `max_bytes` rejected before any parsing; a 16 MiB-boundary case at the limit accepted (constructed in-test, not a committed fixture).

- **Done when:** every corpus fixture's verdict holds under `cargo test -p fsm-core --test json_corpus`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
