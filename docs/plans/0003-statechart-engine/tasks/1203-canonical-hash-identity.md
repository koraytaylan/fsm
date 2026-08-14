---
id: canonical-hash-identity
title: "Canonical Hash Identity"
workstream: "0012"
kind: task
depends_on:
  - machine-spec-parse
gated: false
touches:
  - crates/fsm-core/src/hashes.rs
  - crates/fsm-core/tests/hashes_golden.rs
  - "crates/fsm-core/tests/fixtures/hashes/**"
status: planned
merged_as: ""
---
# Canonical Hash Identity

Machine identity is content-addressed: a domain-separated SHA-256 over the canonical definition bytes, rendered `name@sha256:<hex>`, resolvable by unique prefix — so a definition can never be silently redefined and references stay short.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/hashes/identity.jsonl` first: canonical definitions (including the `case_review` reference) paired with their expected `machine_id` strings computed once by hand from the architecture rule (`sha256(tag ‖ 0x0A ‖ canon_bytes)`, tag `fsm:machine:1`), plus prefix-resolution cases (full id, ≥ 12-hex unique prefix, bare unique name, ambiguous prefix, ambiguous bare name); plus `crates/fsm-core/tests/hashes_golden.rs` asserting every line.
2. Implement `domain_hash(tag, v)`, `machine_id(canonical_def)`, and `resolve_machine_ref(ids, query) -> Result<String, ResolveError>` in `crates/fsm-core/src/hashes.rs` per architecture, with ambiguity errors listing candidate versions.
3. Add an inline test pinning that two definitions differing only in `description` produce different ids (documentation is part of identity).

- **Done when:** every line of `identity.jsonl` holds under `cargo test -p fsm-core --test hashes_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
