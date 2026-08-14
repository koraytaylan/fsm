---
id: json-canonical-writer
title: "Json Canonical Writer"
workstream: "0002"
kind: task
depends_on:
  - json-value-and-parser
gated: false
touches:
  - crates/fsm-core/src/json/write.rs
  - crates/fsm-core/src/canon.rs
  - crates/fsm-core/tests/canon_golden.rs
  - "crates/fsm-core/tests/fixtures/canon/**"
status: planned
merged_as: ""
---
# Json Canonical Writer

Every hash in the system — machine identity, journal chain, state hashes — is computed over the bytes this single serializer produces, so its behavior is pinned by golden byte fixtures before it is written.

**Steps:**

1. Author golden fixtures first under `crates/fsm-core/tests/fixtures/canon/`: input-JSON → expected-canonical-bytes pairs covering key reordering, escape normalization, Unicode passthrough, number-token verbatim output, and nested empties, plus `crates/fsm-core/tests/canon_golden.rs` asserting them.
2. Implement `write_canonical(&Value, &mut Vec<u8>)` in `crates/fsm-core/src/json/write.rs` — single line, byte-sorted keys, minimal escaping, verbatim `Num` tokens — as the only JSON serializer in the system.
3. Implement `canon_bytes(&Value)` and `is_canonical(bytes, &JsonLimits)` (parse → re-serialize → byte-compare) in `crates/fsm-core/src/canon.rs`.
4. Add the round-trip property to `canon_golden.rs`: for every `y_*` JSON corpus fixture, parse∘write∘parse is identity and a second canonicalization is byte-identical.

- **Done when:** all canon goldens and the corpus round-trip property pass under `cargo test -p fsm-core --test canon_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
