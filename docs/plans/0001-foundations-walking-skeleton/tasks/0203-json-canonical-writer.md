---
id: json-canonical-writer
title: "Json Canonical Writer"
workstream: "0002"
kind: task
depends_on:
  - json-structural-parser
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

1. Author the golden pairs under `crates/fsm-core/tests/fixtures/canon/` and `crates/fsm-core/tests/canon_golden.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `write_canonical(&Value, &mut Vec<u8>)` in `crates/fsm-core/src/json/write.rs` — single line, byte-sorted keys, minimal escaping, verbatim `Num` tokens — as the only JSON serializer in the system.
3. Implement `canon_bytes(&Value)` and `is_canonical(bytes, &JsonLimits)` (parse → re-serialize → byte-compare) in `crates/fsm-core/src/canon.rs`.

**Tests:**

- Golden input→canonical-bytes pairs in `fixtures/canon/`, asserted byte-for-byte by `canon_golden.rs`: an object whose keys arrive unsorted → byte-sorted output; escape normalization (a backslash-u escape for a printable character collapses to the raw character; an escaped forward slash becomes a bare `/`; quote, backslash, and the C0 escapes stay escaped, other control characters as lowercase `\u00xx`); non-ASCII passthrough (`é`, an emoji) as raw UTF-8; number tokens written verbatim (`1e309`, `-0.0` preserved exactly as parsed); nested empties `{"a":[],"b":{}}`; whitespace-heavy input collapsing to a single line.
- Round-trip property in `canon_golden.rs`: for every `y_*` fixture of the JSON corpus, `parse ∘ write_canonical ∘ parse` is identity, and canonicalizing the canonical bytes again is byte-identical (idempotence).
- `is_canonical`: returns true on canonical bytes; false on the same document with a single inserted space; false on the same document with two keys swapped; propagates a parse error (not `false`) on invalid JSON.

- **Done when:** all canon goldens, the corpus round-trip property, and the `is_canonical` cases pass under `cargo test -p fsm-core --test canon_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
